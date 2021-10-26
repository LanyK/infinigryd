//! This module defines the LocalEnvironment type which is the centerpiece of the *actlib* library.
//!
//! The [LocalEnvironment](struct.LocalEnvironment.html) is able to [spawn](struct.Environment.html#method.spawn) new Actors,
//! [remove](struct.Environment.html#method.remove) (despawn) them
//! and handles sending and receiving messages from [Actors](../actor/trait.Actor.html) that live on a remote machine.

use crate::actor::*;
use crate::api::Environment;
use crate::errors::ActlibError;
use crate::log_err_as;
use crate::message::*;
use indexmap::IndexMap;
#[allow(unused_imports)]
use log::{error, info, warn};
use netchannel::{NetChannel, NetReceiver, NetSender};
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::net::IpAddr;
use std::net::SocketAddr;
use std::sync::mpsc::*;
use std::sync::{Arc, Mutex, RwLock};
use uuid::Uuid;

/// Abbreviation for ```Arc<Mutex<LocalEnvironment>>```.
pub(crate) type ArcEnvironment = Arc<LocalEnvironment>;

#[macro_export]
/// This macro builds and **returns** an `actor_builder` function object expected by [Environment::new](./api/struct.Environment.html#method.new)[(_local_only)](./api/struct.Environment.html#method.new_local_only).
///
///```
/// let actor_builder = actor_builder!("ExampleActor" => ExampleActor{state: 32});
///```
/// This method is needed to spawn new instances of Actors both locally as well as on a distributed [Environment](../lib/api/Environment.html).
///
///```rust
/// let actor_builder = actor_builder!("ExampleActor" => ExampleActor{state: 32});
/// let (environment, timer) = Environment::new(remotes, actor_builder);
/// let actor_ref = environment.spawn("ExampleActor");
///```
///
/// The macro takes an arbitrary number of arguments of the form ```$identifier:expr => $new_actor:expr``` where *$identifier* is a unique string representing the following *$new_actor* expression, and *$new_actor* is an expression returning a new Actor object.
///
/// It is allowed to register an arbitrary number of unique strings pointing to the same Actor type or different parametrizations of the same Actor type.
///
/// **Important:** If `actlib` is used in a distributed setting, it is paramount that all client programs use compatible `actor_builder!` calls to be able to distribute Actor spawns across machines!
/// This of course depends on whether new instances of a given kind are ever to be spawned inside the distributed Environment at all.
///
macro_rules! actor_builder {
    ($($identifier:expr => $new_actor:expr),+$(,)?) => {
        {
            fn actor_builder(type_id: &str) -> Result<Box<dyn Actor>, ActlibError> {
                $(
                    if type_id == $identifier {
                        Ok(Box::new($new_actor))
                    } else
                )+
                {Err(ActlibError::SpawnFailed(format!("Unknown actor type: {}", type_id)))}
            }
        actor_builder
        }
    };
}

/// Environment holding the ```Sender```-End of a Channel to every [Actor](../actor/trait.Actor.html).
///
/// It can spawn new [Actors](../actor/trait.Actor.html) and is responsible that messages to/from an external environment reach the specified [Actor](../actor/trait.Actor.html).
pub(crate) struct LocalEnvironment {
    /// Holds the channels towards the mailbox of every Actor living in this Environment, indexed by it's ActorId
    local_actor_channels: Mutex<HashMap<ActorId, Sender<EitherMessage>>>,
    /// Holds the sender of the channel to use for all ActorRefs with actors living on another machine.
    /// The channel content is a <b>tuple</b> of (ActorId,Box[Message as Any]).
    /// This is being held for future cloning when creating new ActorRefs.
    ///
    /// The receiving end of the channel is a thread spawned at environment creation.
    /// This thread serialized the messages and sends it to the environment with the associated mac_address.
    external_actor_ref_sender: Mutex<Sender<(ActorId, SerNetMessageContent)>>,
    /// Unique local address of this machine
    pub local_machine: SocketAddr,
    /// Mapping from Machine-identifier to associated TCP-connection.
    net_senders: Mutex<IndexMap<IpAddr, NetSender>>,
    /// How to build a new Actor specified by a Type Id
    actor_builder: fn(&str) -> Result<Box<dyn Actor>, ActlibError>,
    /// Sender-end of a channel the main thread is supposed to block on the Receiver.
    termination_sender: Mutex<Sender<()>>,
    /// Load Balancer for distributing the spawn process of new Actors
    load_balancer: Mutex<LoadBalancer>,
    /// A map for alive-queries about actors located on a remote machine
    /// queried_id, searcher_id
    remote_queries: Mutex<HashMap<(Vec<u8>, ActorId), Sender<Option<ActorRef>>>>,
    /// Actors protected by other Actors. They can't be removed.
    /// target_id, protector_id
    invincible_actors: RwLock<HashMap<ActorId, HashSet<ActorId>>>,
}

impl Debug for LocalEnvironment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        write!(f, "LocalEnvironment {{local_actor_channels: {:?}, external_actor_ref_sender: {:?}, local_machine: {:?}, net_senders: {:?}, actor_builder: /*omitted*/, termination_sender: {:?}, load_balancer: {:?}}}", self.local_actor_channels, self.external_actor_ref_sender, self.local_machine, self.net_senders, self.termination_sender, self.load_balancer)
    }
}

/// How should the LocalId-part of the ActorId be created
pub(crate) enum SpawnId {
    /// Create Automatic, currently using Uuid::new_v4
    Automatic,
    /// Use the specified Id and spawn the Actor on this Environment.
    /// Triggered by Remote spawn request and the _local-variants of the spawn-method.
    SpawnHere(LocalId),
    /// A User specified Id.
    User(LocalId),
}

impl SpawnId {
    /// Returns ```true``` if the Id was received from a remote machine, or spawn_local(_with_id) method.
    pub(crate) fn is_spawn_here(&self) -> bool {
        match self {
            SpawnId::SpawnHere(_) => true,
            _ => false,
        }
    }

    /// Unwrap a PassedId, creating a new one if none was given
    pub(crate) fn unwrap_or_automatic(self) -> LocalId {
        match self {
            SpawnId::Automatic => LocalId::Automatic(Uuid::new_v4()),
            SpawnId::SpawnHere(id) => id,
            SpawnId::User(id) => id,
        }
    }
}

/// Buffer-size for reading remote messages.
const BUFFERSIZE: usize = 1 * 1024 * 512;

impl LocalEnvironment {
    /// Create a new Environment.
    ///
    /// [Actors](../actor/trait.Actor.html) can be located either on the same machine,
    /// or on any [Machine](../../netchannel/struct.Machine.html) given in the argument.
    ///
    /// *own_port* is used to establish a TCP-connection to remote machines.
    ///
    /// It is not possible to add new machines after creation of the environment.
    pub(crate) fn new(
        own_port: u16,
        mut remotes: Vec<SocketAddr>,
        actor_builder: fn(&str) -> Result<Box<dyn Actor>, ActlibError>,
        termination_sender: Sender<()>,
    ) -> ArcEnvironment {
        // create the ActorRef -> Env channel for this environment
        let (external_actor_ref_sender, external_actor_ref_receiver): (
            Sender<(ActorId, SerNetMessageContent)>,
            Receiver<(ActorId, SerNetMessageContent)>,
        ) = channel();

        // construct local machine identifier
        let local_machine;
        match get_if_addrs::get_if_addrs() {
            Ok(ifaces) => {
                match ifaces
                    .into_iter()
                    .filter(|iface| !iface.is_loopback())
                    .next()
                {
                    Some(interface) => {
                        local_machine = SocketAddr::new(interface.ip(), own_port);
                        println!(
                            "Starting up Environment on local machine: {:?}",
                            local_machine
                        );
                    }
                    None => {
                        panic!("Could not find local network connection");
                    }
                }
            }
            Err(e) => {
                panic!("Could not find local network connection: {:?}", e);
            }
        }

        // remove self from remotes (if it was passed there)
        remotes = remotes
            .into_iter()
            .filter(|remote| remote.ip() != local_machine.ip())
            .collect();

        let num_machines = 1 + remotes.len();
        // Create net_channels map
        let net_senders = Mutex::new(IndexMap::with_capacity(remotes.len()));
        let mut net_receivers = Vec::with_capacity(remotes.len());

        // connect to remote machines
        for remote in &remotes {
            let mut net_channel = NetChannel::new(local_machine.clone(), remote.clone());
            match net_channel.split() {
                Ok((sender, receiver)) => {
                    if let Ok(mut senders) = net_senders.lock() {
                        senders.insert(remote.ip(), sender);
                        net_receivers.push(receiver);
                    }
                }
                Err(e) => {
                    panic!("Could not split NetChannel instance: {:?}", e);
                }
            }
        }

        // Create new Environment instance
        let env = Arc::new(LocalEnvironment {
            local_actor_channels: Mutex::new(HashMap::new()),
            external_actor_ref_sender: Mutex::new(external_actor_ref_sender),
            local_machine,
            net_senders,
            actor_builder,
            termination_sender: Mutex::new(termination_sender),
            load_balancer: Mutex::new(LoadBalancer::new(num_machines)),
            remote_queries: Mutex::new(HashMap::new()),
            invincible_actors: RwLock::new(HashMap::new()),
        });

        // if no remote exist there is no need to create threads dedicated to handling remote connections
        if !remotes.is_empty() {
            let env_remote_send = env.clone();
            // Start listener Thread for message passing to an external environment.
            //
            // Messages are sent to this environment's receiver, serialized and send to the specified machine.
            std::thread::spawn(move || {
                LocalEnvironment::wait_for_local_messages(
                    env_remote_send,
                    external_actor_ref_receiver,
                );
            });

            // start receive thread for each remote machine
            for net_receiver in net_receivers.into_iter() {
                let env_remote_receive = env.clone();
                std::thread::spawn(move || {
                    LocalEnvironment::wait_for_remote_messages(env_remote_receive, net_receiver);
                });
            }
        }

        return env;
    }

    /// private helper function used in the receiver thread for **foreign-to-local** messages
    fn wait_for_remote_messages(env_remote_receive: ArcEnvironment, mut net_receiver: NetReceiver) {
        loop {
            // create buffer
            let mut buffer = [0; BUFFERSIZE];
            // read message from TCP stream
            match net_receiver.read(&mut buffer) {
                Ok(vec) => {
                    for bin_message in vec {
                        match bincode::deserialize::<NetMessage>(bin_message) {
                            Ok(NetMessage::Broadcast(content)) => {
                                match env_remote_receive.local_actor_channels.lock() {
                                    Ok(channels) => {
                                        let actor_ids: Vec<ActorId> =
                                            channels.keys().map(Clone::clone).collect();
                                        drop(channels);
                                        // broadcast serialized Message to all Actors
                                        for actor_id in actor_ids {
                                            env_remote_receive.handle_net_message(
                                                SerNetMessageContent::Message(content.clone()),
                                                actor_id.clone(),
                                            );
                                        }
                                    }
                                    Err(e) => {
                                        error!("{:?}", ActlibError::from_poison_error(&e));
                                    }
                                }
                            }
                            Ok(NetMessage::SpawnByTypeId(actor_type_id, local_id)) => {
                                // spawn a new actor on this machine with matching local_id to the sender of the NetMessage
                                if let Err(e) = LocalEnvironment::spawn(
                                    Environment {
                                        env: env_remote_receive.clone(),
                                    },
                                    &actor_type_id,
                                    SpawnId::SpawnHere(local_id),
                                ) {
                                    error!("{:?}", e);
                                    panic!("{:?}", e)
                                    // only possibility for this error is when spawn(..) can't acquire the lock because of bad poison.
                                    // this is an invalid state and warrants a poison
                                }
                            }
                            Ok(NetMessage::Message(actor_id, msg)) => {
                                // relay User Message
                                env_remote_receive.handle_net_message(
                                    SerNetMessageContent::Message(msg),
                                    actor_id,
                                );
                            }
                            Ok(NetMessage::SpecialToken(actor_id, bin_token)) => {
                                // relay Token Message
                                env_remote_receive.handle_net_message(
                                    SerNetMessageContent::Token(bin_token),
                                    actor_id,
                                );
                            }
                            Ok(NetMessage::RemoveProtector(protector_id, target_id)) => {
                                // remove protector for target id, so it can be removed (if all are removed)
                                env_remote_receive.remove_protector(protector_id, target_id);
                            }
                            Ok(NetMessage::QuerySpecifiedId(
                                queried_id,
                                sender_ip_addr,
                                searcher,
                                protected,
                            )) => {
                                // build dummy ActorId for local search
                                let actor_id: ActorId = ActorId {
                                    local_id: LocalId::Specified(queried_id.clone()),
                                    location: env_remote_receive.local_machine.ip(),
                                };
                                // does this actor exist on THIS machine?
                                // if yes, `result` holds the local ip to be handed out
                                let result = {
                                    match env_remote_receive.local_actor_channels.lock() {
                                        Ok(channels) => {
                                            if channels.contains_key(&actor_id) {
                                                if protected {
                                                    env_remote_receive
                                                        .add_protector(searcher.clone(), actor_id);
                                                }
                                                Some(env_remote_receive.local_machine.ip())
                                            } else {
                                                None
                                            }
                                        }
                                        Err(e) => {
                                            error!("{:?}", ActlibError::from_poison_error(&e));
                                            None
                                        }
                                    }
                                };
                                let result_msg = NetMessage::QuerySpecifiedIdResult(
                                    queried_id, searcher, result,
                                );
                                if let Ok(serialized_msg) = bincode::serialize(&result_msg) {
                                    match env_remote_receive.net_senders.lock() {
                                        Ok(mut senders) =>
                                            match senders.get_mut(&sender_ip_addr) {
                                                Some(net_sender) => {
                                                    // send result to querying machine
                                                    let _ = net_sender.write(&serialized_msg);
                                                    // if this fails the connection was dropped
                                                    // nothing we can do here
                                                }
                                                None => log_err_as!(error, ActlibError::ActorNotFound("Failed to find Actor channel to relay remote message to local actor!".to_string()))
                                        },
                                        Err(e) => {
                                            error!("{:?}", ActlibError::from_poison_error(&e));
                                        }
                                    }
                                } else {
                                    warn!("Warning: Failed to serialize result message of type QuerySpecifiedIdResult");
                                }
                            }
                            Ok(NetMessage::QuerySpecifiedIdResult(
                                queried_id,
                                searcher_id,
                                result,
                            )) => {
                                match result {
                                    Some(ip_addr) => {
                                        // found queried_id on machine with ip_addr
                                        match env_remote_receive.remote_queries.lock() {
                                            Ok(mut queries) => {
                                                if let Some(sender) = queries
                                                    .remove(&(queried_id.clone(), searcher_id))
                                                {
                                                    if let Ok(actor_ref_sender) = env_remote_receive
                                                        .external_actor_ref_sender
                                                        .lock()
                                                    {
                                                        // send result
                                                        let _ = sender.send(Some(ActorRef::new(
                                                            ActorId {
                                                                local_id: LocalId::Specified(
                                                                    queried_id,
                                                                ),
                                                                location: ip_addr,
                                                            },
                                                            ActorRefChannel::Remote(
                                                                actor_ref_sender.clone(),
                                                            ),
                                                        )));
                                                    }
                                                }
                                            }
                                            Err(e) => log_err_as!(
                                                error,
                                                ActlibError::from_poison_error(&e)
                                            ),
                                        }
                                    }
                                    None => {
                                        // didn't find queried_id on remote machine
                                        match env_remote_receive.remote_queries.lock() {
                                            Ok(queries) => {
                                                if let Some(sender) =
                                                    queries.get(&(queried_id, searcher_id))
                                                {
                                                    // channel might be closed, if another remote already send Some(...)
                                                    // we don't have to unblock anyone in that case
                                                    let _ = sender.send(None);
                                                }
                                            }
                                            Err(e) => log_err_as!(
                                                error,
                                                ActlibError::from_poison_error(&e)
                                            ),
                                        }
                                    }
                                }
                            }
                            Ok(NetMessage::SendExpirationSignal) => {
                                // this only returns Err(_) when no one is waiting on the termination_receiver
                                let _ = env_remote_receive.send_expiration_signal();
                            }
                            Err(e) => {
                                // do nothing. Deserialize failed, unrecognised message
                                warn!(
                                    "Warning: Failed to deserialize remote messsage: {:?} ({:?})",
                                    bin_message, e
                                );
                            }
                        }
                    }
                }
                Err(_) => {
                    panic!("Warning: NetReceiver::read returned error.");
                    // we don't re-acquire the net connection anytime, so this is effectively a terminating condition. but scary likely.
                }
            }
        }
    }

    // private helper function used in the receiver thread for **local-to-foreign** messages.
    fn wait_for_local_messages(
        env_remote_send: ArcEnvironment,
        external_actor_ref_receiver: Receiver<(ActorId, SerNetMessageContent)>,
    ) {
        // Waits for messages and handles them sequentially
        loop {
            match external_actor_ref_receiver.recv() {
                // a outgoing net message always has the form (ActorId,SerializedNetMessageContent)
                // with SerializedNetMessageContent being either ::Message(Vec<u8>) or ::Token(Vec<u8>)
                Ok((actor_id, content)) => {
                    match env_remote_send.net_senders.lock() {
                        Ok(mut senders) => {
                            if let Some(net_sender) = senders.get_mut(&actor_id.location) {
                                match content {
                                    SerNetMessageContent::Message(msg) => {
                                        // try to serialize the message, silently failing if not possible
                                        if let Ok(tuple_serialized) = bincode::serialize(
                                            &NetMessage::Message(actor_id.clone(), msg.clone()),
                                        ) {
                                            if let Err(e) = net_sender.write(&tuple_serialized) {
                                                warn!(
                                                    "Warning: Write on net_sender failed: {:?}",
                                                    e
                                                );
                                            }
                                        } else {
                                            warn!(
                                                "Serializing NetMessage failed: {:?}",
                                                (actor_id, msg)
                                            );
                                        }
                                    }
                                    SerNetMessageContent::Token(tok) => {
                                        // try to serialize the message, silently failing if not possible
                                        if let Ok(tuple_serialized) = bincode::serialize(
                                            &NetMessage::SpecialToken(actor_id, tok),
                                        ) {
                                            if let Err(e) = net_sender.write(&tuple_serialized) {
                                                warn!(
                                                    "Warning: Write on net_sender failed: {:?}",
                                                    e
                                                );
                                            }
                                        }
                                    }
                                }
                            } else {
                                warn!(
                                    "Error: Could not get NetSender for {:?}",
                                    &actor_id.location
                                );
                            }
                        }
                        Err(e) => error!("{:?}", ActlibError::from_poison_error(&e)),
                    }
                }
                Err(_) => {
                    // No one holds the sender end any more, so this thread can terminate
                    warn!("All copies of the Sender part of the local-to-environment/remote channel dropped, stopping the worker thread");
                    break;
                }
            }
        }
    }

    /// Remove the [Actor](../actor/trait.Actor.html) associated with the [ActorId](../actor/struct.ActorId.html) from the Environment.
    fn remove(&self, actor_id: ActorId) {
        if actor_id.location != self.local_machine.ip() {
            // remote case:
            match self.net_senders.lock() {
                Ok(mut senders) => {
                    match senders.get_mut(&actor_id.location) {
                        Some(sender) => {
                            // serialize on-stop message to trigger the remove method over at the remote machine
                            match bincode::serialize(&Token::Stop) {
                                Ok(bin_token) => {
                                    match bincode::serialize(&NetMessage::SpecialToken(
                                        actor_id, bin_token,
                                    )) {
                                        Ok(bin_msg) => {
                                            sender.write(&bin_msg);
                                        }
                                        Err(_) => {
                                            warn!("Could not send Stop command to remote machine because the NetMessage could not be serialized.");
                                        }
                                    }
                                }
                                Err(_) => {
                                    error!("Could not send Stop command to remote machine because the Stop Token could not be serialized.");
                                }
                            }
                        }
                        None => {
                            error!( "Could not find net sender object to machine {:?}. Message Dropped.", actor_id.location );
                        }
                    }
                }
                Err(e) => log_err_as!(error, ActlibError::from_poison_error(&e)),
            }
        } else {
            // local case:
            if let Ok(inv_actors) = self.invincible_actors.read() {
                if inv_actors.contains_key(&actor_id) {
                    return;
                }
            }
            match self.local_actor_channels.lock() {
                Ok(mut channels) => {
                    channels.remove(&actor_id);
                }
                Err(e) => log_err_as!(error, ActlibError::from_poison_error(&e)),
            }
        }
    }

    /// Create the ActorRef for an alive Actor with a User-specified ActorId.
    /// First see if the Actor is located locally, if not try every known remote machine.
    /// If the Actor is located on a remote Machine block the current thread until an answer was received.
    pub(crate) fn find_actor_ref(
        &self,
        queried_id: &Vec<u8>,
        searcher: ActorId,
        protected: bool,
    ) -> Result<(Receiver<Option<ActorRef>>, usize), ActlibError> {
        // build local variant for comparison with existing actors
        let target_actor_id = ActorId {
            local_id: LocalId::Specified(queried_id.clone()),
            location: self.local_machine.ip(),
        };
        let (sender, receiver) = channel();
        // Local search
        match self.local_actor_channels.lock() {
            Ok(mut channels) => {
                if channels.contains_key(&target_actor_id) {
                    // This is Some() only if the sending actor wants to make the target actor invincible
                    if protected {
                        self.add_protector(searcher.clone(), target_actor_id.clone());
                    }
                    // get local actor's sender
                    match channels.get_mut(&target_actor_id) {
                        Some(actor_ref_sender) => {
                            // pre-fill the receiver's sender part that will be returned.
                            // in this local case, this will be the only element of this channel that will be waited for.
                            let new_actor_ref = ActorRef::new(
                                target_actor_id,
                                ActorRefChannel::Local(actor_ref_sender.clone()),
                            );
                            sender.send(Some(new_actor_ref));
                            Ok((receiver, 1)) // 1: this will be the only message in this channel
                        }
                        None => Err(ActlibError::ActorNotFound(
                            "Requested Actor was removed just a short time ago.".to_string(),
                        )),
                    }
                } else {
                    // remote Search
                    drop(channels); // drop MutexGuard, not needed in else case
                                    // register LocalEnvironment level sender to propagate answers from remotes back to the receiver that is handed out at the end of this function
                    match self.remote_queries.lock() {
                        Ok(mut queries) => {
                            queries.insert((queried_id.clone(), searcher.clone()), sender);
                            drop(queries); // drop lock after use
                            match self.net_senders.lock() {
                                Ok(mut senders) => {
                                    for (_remote_machine, net_sender) in &mut *senders {
                                        if let Ok(net_message) =
                                            bincode::serialize(&NetMessage::QuerySpecifiedId(
                                                queried_id.clone(),
                                                self.local_machine.ip(),
                                                searcher.clone(),
                                                protected,
                                            ))
                                        {
                                            if let Err(e) = net_sender.write(&net_message) {
                                                if let Ok(mut queries) = self.remote_queries.lock()
                                                {
                                                    queries.remove(&(queried_id.clone(), searcher));
                                                }
                                                warn!("Failed to write Actor Query to remote stream, potentially deadlocking an actor waiting for response!");
                                                return Err(ActlibError::NetworkError("Failed to write Actor Query to remote stream, potentially deadlocking an actor waiting for response!".to_string()));
                                            }
                                        } else {
                                            error!("Error: Serializing the ActorRef query failed!");
                                        }
                                    }
                                    Ok((receiver, senders.len()))
                                }
                                Err(e) => Err(ActlibError::from_poison_error(&e)),
                            }
                        }
                        Err(e) => Err(ActlibError::from_poison_error(&e)),
                    }
                }
            }
            Err(e) => Err(ActlibError::from_poison_error(&e)),
        }
    }

    /// Create a new [ActorRef](../actor/struct.ActorRef.html) corresponding to the [ActorId](../actor/struct.ActorId.html).
    ///
    /// [ActorRefs](../actor/struct.ActorRef.html) for local [Actors](../actor/trait.Actor.html) are only created if it exists.
    ///
    /// [ActorRefs](../actor/struct.ActorRef.html) for remote [Actors](../actor/trait.Actor.html) are always created.
    pub(crate) fn to_actor_ref(&self, actor_id: ActorId) -> Result<ActorRef, ActlibError> {
        if actor_id.location == self.local_machine.ip() {
            match self.local_actor_channels.lock() {
                Ok(channels) => {
                    if let Some(sender) = channels.get(&actor_id) {
                        Ok(ActorRef::new(
                            actor_id,
                            ActorRefChannel::Local(sender.clone()),
                        ))
                    } else {
                        Err(ActlibError::ActorNotFound(format!(
                            "Did not find local actor with id {:?}",
                            actor_id
                        )))
                    }
                }
                Err(e) => Err(ActlibError::from_poison_error(&e)),
            }
        } else {
            match self.external_actor_ref_sender.lock() {
                Ok(sender) => Ok(ActorRef::new(
                    actor_id,
                    ActorRefChannel::Remote(sender.clone()),
                )),
                Err(e) => Err(ActlibError::from_poison_error(&e)),
            }
        }
    }

    fn add_protector(&self, protector_id: ActorId, target_id: ActorId) {
        match self.invincible_actors.write() {
            Ok(mut inv_actors) => {
                if let Some(protectors) = inv_actors.get_mut(&target_id) {
                    protectors.insert(protector_id);
                } else {
                    let mut hs = HashSet::new();
                    hs.insert(protector_id);
                    inv_actors.insert(target_id, hs);
                }
            }
            Err(e) => {
                error!("{:?}", e);
            }
        }
    }

    /// Removes the given protector from protecting the given target actor. Will send this request to all remote environments too.
    pub(crate) fn remove_protector(&self, protector_id: ActorId, target_id: ActorId) {
        match self.invincible_actors.write() {
            Ok(mut invincible_actors) => {
                if let Some(protectors) = invincible_actors.get_mut(&target_id) {
                    protectors.remove(&protector_id);
                } else {
                    // drop the write guard since its not needed in the else case
                    drop(invincible_actors);
                    //
                    match self.net_senders.lock() {
                        Ok(mut senders) => {
                            for (addr, net_channel) in &mut *senders {
                                match bincode::serialize(&NetMessage::RemoveProtector(
                                    protector_id.clone(),
                                    target_id.clone(),
                                )) {
                                    Ok(msg) => {
                                        net_channel.write(&msg);
                                    }
                                    Err(e) => {
                                        warn!("Unable to send RemoveProtector to remote {:?}, possible MemLeak! Error Message: {:?}", addr, e);
                                    }
                                }
                            }
                        }
                        Err(e) => log_err_as!(error, e),
                    }
                }
            }
            Err(e) => log_err_as!(error, e),
        }
    }

    pub(crate) fn remove_remote_query(&self, queried_id: &Vec<u8>, searcher: ActorId) {
        if let Ok(mut queries) = self.remote_queries.lock() {
            queries.remove(&(queried_id.clone(), searcher));
        } else {
            warn!("Unable to acquire remote_queries lock in remove_remote_query, possible MemLeak");
        }
    }

    /// This method is called when an incoming message from another machine is detected.
    fn handle_net_message(&self, message_or_token: SerNetMessageContent, actor_id: ActorId) {
        match self.local_actor_channels.lock() {
            Ok(mut channels) => {
                match channels.get_mut(&actor_id) {
                    Some(sender) => {
                        match message_or_token {
                            SerNetMessageContent::Message(bin) => {
                                if let Err(e) = sender.send(EitherMessage::Serialized(bin)) {
                                    info!("Received remote message but internal actor channel is closed, probably because the actor does not exist anymore: {:?}", e);
                                }
                            }
                            SerNetMessageContent::Token(bin) => {
                                match bincode::deserialize::<Token>(&bin) {
                                    Ok(token) => {
                                        // special Tokens that are handled only by the Actor itself are passed on as a message to the actor
                                        if let Err(e) = sender.send(EitherMessage::Special(token)) {
                                            info!("Received remote Token message but internal actor channel is closed, probably because the actor does not exist anymore: {:?}", e);
                                        }
                                    }
                                    Err(_e) => {
                                        warn!("Unable to de-serialize Token message from remote, system state potentially compromised.");
                                    }
                                }
                            }
                        }
                    }
                    None => {
                        warn!(
                            "Actor {:?} not found. Remote message {:?} ignored.",
                            actor_id, message_or_token
                        );
                    }
                }
            }
            Err(e) => log_err_as!(warn, e),
        }
    }

    /// Spawn a given [Actor](../actor/trait.Actor.html) object inside this Environment.
    ///
    /// This method registers the [Actor](../actor/trait.Actor.html) inside this Environment and subsequently calls it's [on_start](../actor/trait.Actor.html#method.on_start) Method.
    ///
    /// The return value is an [ActorRef](../actor/struct.ActorRef.html) object as the [Actor](../actor/trait.Actor.html) address.
    /// Use it to send messages to the now alive [Actor](../actor/trait.Actor.html).
    pub(crate) fn spawn(
        env: Environment,
        actor_type_id: &str,
        local_id: SpawnId,
    ) -> Result<ActorRef, ActlibError> {
        let local_environment = &env.env;

        let mut machine_no = 0;
        if !local_id.is_spawn_here() {
            match local_environment.load_balancer.lock() {
                Ok(mut balancer) => {
                    machine_no = balancer.next_machine_no();
                }
                Err(e) => {
                    warn!("Could not acquire LoadBalancer Mutex lock, defaulted to local spawn.");
                }
            }
        }
        match machine_no {
            0 => {
                let new_actor = (local_environment.actor_builder)(&actor_type_id)?;

                let actor_id = ActorId {
                    local_id: local_id.unwrap_or_automatic(),
                    location: local_environment.local_machine.ip(),
                };

                // create new channel for the new actor's mailbox
                let (mailbox_sender, mailbox_receiver) = channel();

                // create new ActorRef pointing to the new actor instance
                let actor_ref = ActorRef::new(
                    actor_id.clone(),
                    ActorRefChannel::Local(mailbox_sender.clone()),
                );

                // register channel in this environment
                match local_environment.local_actor_channels.lock() {
                    Ok(mut channels) => {
                        channels.insert(actor_id.clone(), mailbox_sender);
                    }
                    Err(e) => {
                        return Err(ActlibError::SpawnFailed(
                            "Failed to insert Actor to Environment".to_string(),
                        ));
                    }
                }

                // spawn mailbox check thread
                // it will loop over received messages, breaking on error
                let actor_ref_clone = actor_ref.clone();

                let env_clone = env.clone();

                std::thread::spawn(move || {
                    LocalEnvironment::actor_mailbox_loop(
                        mailbox_receiver,
                        new_actor,
                        env_clone,
                        actor_ref_clone,
                    );
                });

                return Ok(actor_ref);
            }
            remote_machine_no => {
                // machine no that is returned from the load balancer is 1 higher than the index, because id 0 is local.
                match local_environment.net_senders.lock() {
                    Ok(mut senders) => match senders.get_index_mut(remote_machine_no - 1) {
                        Some((machine, net_sender)) => {
                            let new_actor_local_id = match local_id {
                                SpawnId::Automatic => LocalId::Automatic(Uuid::new_v4()),
                                SpawnId::User(id) => id,
                                SpawnId::SpawnHere(_) => {
                                    unreachable!("SpawnHere wanted to send to remote machine.")
                                }
                            };
                            let machine_clone = machine.clone();

                            match bincode::serialize(&NetMessage::SpawnByTypeId(
                                actor_type_id.to_string(),
                                new_actor_local_id.clone(),
                            )) {
                                Ok(msg) => match net_sender.write(&msg) {
                                    Ok(_size) => {
                                        return local_environment.to_actor_ref(ActorId {
                                            local_id: new_actor_local_id,
                                            location: machine_clone,
                                        });
                                    }
                                    Err(e) => {
                                        return Err(ActlibError::SpawnFailed(
                                            "Failed to serialize SpawnByTypeId message".to_string(),
                                        ));
                                    }
                                },
                                Err(_) => {
                                    return Err(ActlibError::SpawnFailed(
                                        "Failed to serialize SpawnByTypeId message".to_string(),
                                    ));
                                }
                            }
                        }
                        None => Err(ActlibError::InvalidState(format!(
                            "Error: LoadBalancer returned machine no that is invalid: {}",
                            machine_no
                        ))),
                    },
                    Err(e) => Err(ActlibError::from_poison_error(&e)),
                }
            }
        }
    }

    fn actor_mailbox_loop(
        mailbox_receiver: Receiver<EitherMessage>,
        mut actor: Box<dyn Actor>,
        env: Environment,
        this_actor_ref: ActorRef,
    ) {
        // create actor's mailbox
        let mailbox = Mailbox::new(mailbox_receiver);

        // keep a ActorId copy at hand
        let this_actor_id = this_actor_ref.clone_id();

        // actor is now registered and has a mailbox, call on_start
        actor.on_start(env.clone(), this_actor_ref);

        loop {
            // The Actor listens for messages incoming to it's mailbox.
            // The messages are handled sequentially, and special Token messages may be handled without direct outside visibility to the actlib API.
            //
            match mailbox.wait_for_msg() {
                Ok(EitherMessage::Special(Token::Stop)) => {
                    // local case:
                    if let Ok(inv_actors) = env.env.invincible_actors.read() {
                        if inv_actors.contains_key(&this_actor_id) {
                            continue;
                        }
                    }
                    actor.on_stop();
                    env.env.remove(this_actor_id);
                    break;
                }
                Ok(EitherMessage::Special(Token::Reset)) => {
                    // triggers the optional user-given on_reset function of this actor
                    actor.on_reset();
                }
                Ok(EitherMessage::Regular(msg)) => {
                    actor.handle(msg);
                }
                Ok(EitherMessage::Serialized(msg_serialized)) => {
                    if let Some(msg) = actor.deserialize_to_any(&msg_serialized) {
                        actor.handle(msg);
                    }
                }
                Err(recv_error) => {
                    error!("Actor Mailbox ended! {:?}", recv_error);
                    // no one holds the sender end anymore (even Environment dropped)
                    // so it is save to stop here
                    break;
                }
            }
        }
    }

    pub(crate) fn send_expiration_signal(&self) -> Result<(), SendError<()>> {
        // Send Expiration-Message to remote machines
        // They will send it back, but we don't care about that since we shut down
        match self.net_senders.lock() {
            Ok(mut senders) => {
                for (_, net_sender) in &mut *senders {
                    if let Ok(ser_net_msg) = &bincode::serialize(&NetMessage::SendExpirationSignal)
                    {
                        // we want to shutdown here, so we don't care about crashed remotes anymore
                        let _ = net_sender.write(&ser_net_msg);
                    }
                }
                drop(senders);
            }
            Err(_e) => return Err(SendError(())),
        }
        // send Token::Stop to all actors
        match self.local_actor_channels.lock() {
            Ok(local_actor_channels) => {
                for (_actor_id, actor_sender) in local_actor_channels.iter() {
                    // we want to shutdown so we don't care about non-responsive actors here
                    let _ = actor_sender.send(EitherMessage::Special(Token::Stop));
                }
            }
            Err(_) => return Err(SendError(())),
        }
        // wait a bit so actors don't try to use stdout during shutdown (causes panic)
        std::thread::sleep(std::time::Duration::from_millis(500));
        match self.termination_sender.lock() {
            Ok(sender) => sender.send(()),
            Err(_) => Err(SendError(())),
        }
    }

    /// Send a Message to all known actors located on this environment.
    pub(crate) fn broadcast<'de, M: Message<'de> + Clone + 'static>(&self, message: M) {
        match self.local_actor_channels.lock() {
            Ok(channels) => {
                for (_actor_id, sender) in &*channels {
                    let _ = sender.send(EitherMessage::Regular(Box::new(message.clone())));
                }
            }
            Err(e) => log_err_as!(error, ActlibError::from_poison_error(&e)),
        }
        match self.net_senders.lock() {
            Ok(mut senders) => {
                for (_, net_sender) in &mut *senders {
                    if let Ok(ser_msg) = bincode::serialize(&message) {
                        if let Ok(ser_net_msg) =
                            &bincode::serialize(&NetMessage::Broadcast(ser_msg))
                        {
                            // if this fails the connection broke down
                            // nothing we can do here
                            let _ = net_sender.write(&ser_net_msg);
                        }
                    }
                }
            }
            Err(e) => log_err_as!(error, ActlibError::from_poison_error(&e)),
        }
    }
}

/// Simple Round Robin load balancer
/// next_machine_no() returns integers from 0 to num_machines excluding,
/// restarting at 0 after each iteration
#[derive(Debug)]
struct LoadBalancer {
    counter: usize,
    num_machines: usize,
}

impl LoadBalancer {
    fn new(num_machines: usize) -> Self {
        LoadBalancer {
            counter: 0,
            num_machines,
        }
    }

    /// Returns numbers incrementally until num_machines is reached, then restarts at 0.
    fn next_machine_no(&mut self) -> usize {
        if self.counter < self.num_machines {
            let res = self.counter.clone();
            self.counter += 1;
            return res;
        } else {
            self.counter = 0;
            return 0_usize;
        }
    }
}
