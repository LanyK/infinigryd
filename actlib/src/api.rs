//! This module contains the API for the *actlib* library.
//!
//! This module (re)exports every type, function and macro required for general usage of the *actlib* library.
//!
//! To use the *actlib* library you have to:
//! * First, specify how your [Actors](../actor/trait.Actor.html) handle [Messages](../message/trait.Message.html).
//!     The [impl_message_handler!](../macro.impl_message_handler.html) macro is provided to reduce required boilerplate code.
//! * Second, you create a [Environment](struct.Environment.html) using the desired constructor.
//!     For ease-of-use, the [actor_builder!](../macro.actor_builder.html) macro is provided.
//! * Third, you spawn one/several Actor(s) using the [spawn](struct.Environment.html#method.spawn) method.
//! * The first [Message](../message/trait.Message.html) send, either by the main thread or an Actor [on_spawn](../actor/trait.Actor.html#method.on_start), gets the ball rolling.

pub use crate::actor::*;
use crate::environment::*;
pub use crate::errors::ActlibError;
use crate::log_err_as;
pub use crate::message::*;
pub use crate::{actor_builder, impl_message_handler};
use log::*;
use std::net::SocketAddr;
use std::sync::mpsc::channel;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::RecvError;
use uuid::Uuid;

/// Struct that supports `wait_until_expiration()`, a blocking function that waits for a termination signal by the associated Environment.
///
/// Use this struct to halt the main thread of the program until the actor system has finished work and the program can be finished regularly.
///
/// The expiration signal is created by calling [Environment::set_expired](struct.Environment#method.set_expired).
pub struct EnvironmentExpirationChecker {
    termination_receiver: Receiver<()>,
}

impl EnvironmentExpirationChecker {
    /// Blocks the current thread until the associated Environment's [set expired](struct.Environment#method.set_expired) method has been called by another thread or the underlying `channel` has been compromised which usually signals a fatal condition of the associated Environment.
    pub fn wait_until_expiration(&self) -> Result<(), RecvError> {
        self.termination_receiver.recv()
    }
}

/// The Environment knows about all [Actors](../actor/trait.Actor.html) in the system.
///
/// It can [spawn](struct.Environment.html#method.spawn) new actors and construct an [ActorRef](../actor/struct.ActorRef.html) from an identifier using [to_actor_ref](struct.Environment.html#method.to_actor_ref) and [find_actor_ref](struct.Environment.html#method.find_actor_ref).
///
/// Using the associated [EnvironmentExpirationChecker](struct.EnvironmentExpirationChecker.html) you can block the main thread until the Environment is [set_expired](struct.Environment#method.set_expired).
#[derive(Clone, Debug)]
pub struct Environment {
    pub(crate) env: ArcEnvironment,
}

impl Environment {
    /// Create a new Environment with several remote sibling Environments.
    ///
    /// This method has to be called on every machine.
    /// The local IP address is automatically filtered and ignored.
    ///
    /// *own_port* is used to establish a TCP-connection to remote machines.
    /// This function blocks until a TCP-Connection to every remote host has been established.
    ///
    /// The returned [EnvironmentExpirationChecker](struct.EnvironmentExpirationChecker.html) can be used to block the main thread until the Environment is [set_expired](struct.Environment#method.set_expired).
    ///
    /// It is not possible to add new machines after creation of the environment.
    pub fn new(
        own_port: u16,
        remotes: &[SocketAddr],
        actor_builder: fn(&str) -> Result<Box<dyn Actor>, ActlibError>,
    ) -> (Self, EnvironmentExpirationChecker) {
        let (termination_sender, termination_receiver) = channel();
        (
            Environment {
                env: LocalEnvironment::new(
                    own_port,
                    remotes.to_vec(),
                    actor_builder,
                    termination_sender,
                ),
            },
            EnvironmentExpirationChecker {
                termination_receiver,
            },
        )
    }

    /// Like [new](struct.Environment.html#method.new), but without the ability to specify additional remote machines.
    pub fn new_local_only(
        actor_builder: fn(&str) -> Result<Box<dyn Actor>, ActlibError>,
    ) -> (Self, EnvironmentExpirationChecker) {
        Environment::new(0, &Vec::with_capacity(0), actor_builder)
    }

    /// Spawn a given [Actor](../actor/trait.Actor.html) object inside this Environment.
    ///
    /// This method registers the [Actor](../actor/trait.Actor.html) inside this Environment and subsequently calls it's [on_start](../actor/trait.Actor.html#method.on_start) Method.
    ///
    /// It may spawn either on the local machine, or a remote Environment located on a machine specified in the [new](struct.Environment.html#method.new)[(_local_only)](struct.Environment.html#method.new_local_only) function used to create this Environment.
    ///
    /// The return value is an [ActorRef](../actor/struct.ActorRef.html) object as the [Actor](../actor/trait.Actor.html) address.
    /// Use it to send messages to the now alive [Actor](../actor/trait.Actor.html).
    pub fn spawn(&self, actor_type_id: &str) -> Result<ActorRef, ActlibError> {
        LocalEnvironment::spawn(self.clone(), actor_type_id, SpawnId::Automatic)
    }

    /// Like [spawn](struct.Environment.html#method.spawn), but the Actor is guaranteed to execute code only on the local machine.
    pub fn spawn_local(&self, actor_type_id: &str) -> Result<ActorRef, ActlibError> {
        LocalEnvironment::spawn(
            self.clone(),
            actor_type_id,
            SpawnId::SpawnHere(LocalId::Automatic(Uuid::new_v4())),
        )
    }

    /// Like [spawn](struct.Environment.html#method.spawn), but the Actor is guaranteed to execute code only on the local machine.
    /// The Actor is guaranteed to have the specified ID.
    ///
    /// You can retrieve it from an associated ActorRef using ```actor_ref.clone_id().when_specified()```.
    ///
    /// Once spawned, the Actor can be found with [find_actor_ref](struct.Environment.html#method.find_actor_ref) until it is [removed](struct.Environment.html#method.remove).
    pub fn spawn_local_with_id(
        &self,
        actor_type_id: &str,
        actor_id: Vec<u8>,
    ) -> Result<ActorRef, ActlibError> {
        LocalEnvironment::spawn(
            self.clone(),
            actor_type_id,
            SpawnId::SpawnHere(LocalId::Specified(actor_id)),
        )
    }

    /// Like [spawn](struct.Environment.html#method.spawn), but the Actor is guaranteed to have the specified ID.
    ///
    /// You can retrieve it from an associated ActorRef using ```actor_ref.clone_id().when_specified()```.
    ///
    /// Once spawned, the Actor can be found with [find_actor_ref](struct.Environment.html#method.find_actor_ref) until it is [removed](struct.Environment.html#method.remove).
    pub fn spawn_with_id(
        &self,
        actor_type_id: &str,
        actor_id: Vec<u8>,
    ) -> Result<ActorRef, ActlibError> {
        LocalEnvironment::spawn(
            self.clone(),
            actor_type_id,
            SpawnId::User(LocalId::Specified(actor_id)),
        )
    }

    /// Remove the specified Actor from the Environment.
    ///
    /// The [on_stop](../actor/trait.Actor#tymethod.on_stop) method is called.
    /// Afterwards, the Actor can't react to any new [Messages](../message/trait.Message.html).
    pub fn remove(&mut self, actor_ref: ActorRef) {
        match &actor_ref.sender {
            ActorRefChannel::Local(s) => {
                s.send(EitherMessage::Special(Token::Stop));
            }
            ActorRefChannel::Remote(s) => match bincode::serialize(&Token::Stop) {
                Ok(token_serialized) => {
                    match s.send((
                        actor_ref.clone_id(),
                        SerNetMessageContent::Token(token_serialized),
                    )) {
                        Ok(_) => {}
                        Err(e) => log_err_as!(
                            err,
                            ActlibError::NetworkError(format!(
                                "Failed to send Stop token: {:?}",
                                e
                            ))
                        ),
                    }
                }
                Err(e) => log_err_as!(
                    err,
                    ActlibError::NetworkError(format!("Failed to send Stop token: {:?}", e))
                ),
            },
        }
    }

    /// Convert the ActorId to the corresponding ActorRef.
    ///
    /// This method can fail if the Actor should reside on the local Environment, but is not found
    /// (e.g. it was removed).
    pub fn to_actor_ref(&self, actor_id: ActorId) -> Result<ActorRef, ActlibError> {
        self.env.to_actor_ref(actor_id)
    }

    /// Create the ActorRef for an alive Actor with a User-specified ActorId.
    ///
    /// First, check if the Actor is located locally. If not try every known remote machine.
    ///
    /// If the Actor is located on a remote Machine block the current thread until an answer was received.
    ///
    /// * *searcher* is the Actor querying the ActorRef.
    /// * *protect* ensures that the specified Actor, if it exists, will not be removed from its environment
    /// until the [drop_protector](struct.Environment.html#method.drop_protector) method is called with the *searcher* as *protector_id*.
    pub fn find_actor_ref(
        &self,
        queried_id: &Vec<u8>,
        searcher: ActorId,
        protect: bool,
    ) -> Result<Option<ActorRef>, ActlibError> {
        let (receiver, num_remotes) =
            match self
                .env
                .find_actor_ref(queried_id, searcher.clone(), protect)
            {
                Ok((receiver, num_remotes)) => (receiver, num_remotes),
                Err(e) => {
                    return Err(e);
                }
            };
        let mut result: Result<Option<ActorRef>, ActlibError> = Ok(None);
        // listen for the answer of each remote machine
        for _ in 0..num_remotes {
            if let Ok(Some(actor_ref)) = receiver.recv() {
                result = Ok(Some(actor_ref));
                break;
            }
        }
        self.env.remove_remote_query(queried_id, searcher);
        result
    }

    /// Remove the *protect*-flag set by [find_actor_ref](struct.Environment.html#method.find_actor_ref).
    ///
    /// After all *protector_id*s have been dropped, the *target_id* can be [removed](struct.Environment.html#method.remove) again.
    pub fn drop_protector(&self, protector_id: ActorId, target_id: ActorId) {
        self.env.remove_protector(protector_id, target_id);
    }

    /// Mark this Environment as expired.
    ///
    /// This will [stop](../actor/trait.Actor.html#method.on_stop) all Actors and release the [wait_until_expiration](struct.EnvironmentExpirationChecker.html#method.wait_until_expiration) method.
    pub fn set_expired(&self) -> Result<(), String> {
        match self.env.send_expiration_signal() {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("{:?}", e)),
        }
    }

    /// Send a Message to all known actors.
    pub fn broadcast<'de, M: Message<'de> + Clone + 'static>(&self, message: M) {
        self.env.broadcast(message)
    }
}
