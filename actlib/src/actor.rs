//! This module defines what an [Actor](trait.Actor.html) has to fulfil and how to [reference](struct.ActorRef.html) it after it's [spawned](../api/struct.Environment.html#method.spawn).
//!
//! - The [Actor](trait.Actor.html)-trait enables a type to be used as an actor in *actlib*.
//! - The [ActorRef](struct.ActorRef.html) type is the address of an Actor.
//!     Use it to send messages to the associated Actor.
//! - The [ActorId](struct.ActorId.html) is a unique identifier for each Actor.
//!     It can be used to construct an [ActorRef](struct.ActorRef.html) with help
//!     from the [Environment](../api/struct.Environment.html).

use crate::api::{ActlibError, Environment};
use crate::message::*;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::net::IpAddr;
use std::sync::mpsc::Sender;
use uuid::Uuid;
/// Trait that enables types to become [Actors](trait.Actor.html) used in the *actlib* library.
///
/// Actors are isolated entities that communicate via [messages](../message/trait.Message.html).
///
/// Actors are created with the [spawn](../api/struct.Environment.html#method.spawn)-method from the then-associated [Environment](../api/struct.Environment.html).
pub trait Actor: Debug + Send + MessageHandler {
    /// Called after a new instance has been created.
    ///
    /// [on_start](#method.on_start) will be called inside [spawn](../api/struct.Environment.html#method.spawn) after the [actor](trait.Actor.html) has been successfully created and it's mailbox is initialized, but before the [ActorRef](struct.ActorRef.html) is returned to the caller of [spawn](../api/struct.Environment.html#method.spawn).
    ///
    /// **Note:** It is expected that this function terminates.
    fn on_start(&mut self, _local_env: Environment, _own_ref: ActorRef) {}

    /// Called when this Actor stops being active.
    ///
    /// **Note:** It is expected that this function terminates.
    fn on_stop(&mut self) {}

    /// Implement this function to define how this actor is to be reset.
    /// This function can either be called manually inside a message handler or is called every time this actor receives the special ```Reset``` message by calling [on_reset](../api/struct.Environment.html#method.on_reset).
    /// **Note** the occurrence of this token in the program flow is left entirely to the implementation that uses `actlib` and as such is entirely optional.
    fn on_reset(&mut self) {}
}

/// Unique [Actor](trait.Actor.html) identifier.
///
/// Constructed out of a locally unique ID and a machine-unique ID.
#[derive(Eq, PartialEq, Ord, PartialOrd, Debug, Clone, Serialize, Deserialize, Hash)]
pub struct ActorId {
    pub(crate) local_id: LocalId,
    pub(crate) location: IpAddr,
}

impl ToString for ActorId {
    fn to_string(&self) -> String {
        format!("{}:{}", self.local_id.to_string(), self.location)
    }
}

impl ActorId {
    /// Return a explicitly specified ActorId.
    /// If the ActorId was automatically created ```None``` is returned.
    pub fn when_specified(self) -> Option<Vec<u8>> {
        match self.local_id {
            LocalId::Specified(byte_vector) => Some(byte_vector),
            _ => None,
        }
    }
}

/// The local_id can either be automatically created, or User specified.
#[derive(Eq, PartialEq, Ord, PartialOrd, Debug, Clone, Serialize, Deserialize, Hash)]
pub(crate) enum LocalId {
    Automatic(Uuid),
    Specified(Vec<u8>),
}

impl ToString for LocalId {
    fn to_string(&self) -> String {
        match self {
            LocalId::Automatic(uuid) => uuid.to_string(),
            LocalId::Specified(vec) => format!("{:?}", vec),
        }
    }
}

/// A reference (address) to an [Actor](trait.Actor.html).
#[derive(Debug, Clone)]
pub struct ActorRef {
    pub(crate) actor_id: ActorId,
    pub(crate) sender: ActorRefChannel,
}

/// Possible Channel-Types for an [ActorRef](struct.ActorRef.html).
///
/// Either for Message between Actors located on the same machine,
/// or for Messages that get relayed by the local Environment.
#[derive(Debug, Clone)]
pub(crate) enum ActorRefChannel {
    /// A channel to an actor on the same machine.
    Local(Sender<EitherMessage>),
    /// A channel to the local environment, which will relay it to an actor on a remote machine.
    Remote(Sender<(ActorId, SerNetMessageContent)>),
}

impl ActorRef {
    /// Create a new [ActorRef](struct.ActorRef.html) if you know the Sender-End from the associated channel.
    pub(crate) fn new(actor_id: ActorId, sender: ActorRefChannel) -> ActorRef {
        ActorRef { actor_id, sender }
    }

    /// Tries to send a special reset message to the actor behind this [ActorRef](struct.ActorRef.html).
    ///
    /// The message is sent unblocking.
    ///

    /// The receiving actor will call its [on_reset](trait.Actor.html#method.on_reset) implementation.
    pub fn send_reset_message(&self) -> Result<(), ActlibError> {
        match &self.sender {
            ActorRefChannel::Local(s) => {
                if let Err(e) = s.send(EitherMessage::Special(Token::Reset)) {
                    Err(ActlibError::InvalidActorRef(
                        "This ActorRef is no longer connected to an Actor".to_string(),
                    ))
                } else {
                    Ok(())
                }
            }
            ActorRefChannel::Remote(s) => {
                if let Ok(token_serialized) = bincode::serialize(&Token::Reset) {
                    match s.send((
                        self.clone_id(),
                        SerNetMessageContent::Token(token_serialized),
                    )) {
                        Ok(_) => Ok(()),
                        Err(e) => Err(ActlibError::NetworkError(format!(
                            "Failed to send Reset token: {:?}",
                            e
                        ))),
                    }
                } else {
                    Err(ActlibError::NetworkError(
                        "Unable to serialize message".to_string(),
                    ))
                }
            }
        }
    }

    /// Tries to send the message to the actor behind this [ActorRef](struct.ActorRef.html).
    ///
    /// The message is sent unblocking. There is no guarantee that the Actor handles the Message (it may be already [removed](../api/struct.Environment.html#method.remove)).
    ///
    /// The method can fail with [InvalidActorRef](../api/enum.ActlibError.html#variant.InvalidActorRef) and [NetworkError](../api/enum.ActlibError.html#variant.NetworkError).
    pub fn send_message<'de, M: Message<'de> + 'static>(
        &self,
        message: M,
    ) -> Result<(), ActlibError> {
        match &self.sender {
            ActorRefChannel::Local(s) => match s.send(EitherMessage::Regular(Box::new(message))) {
                Ok(_) => Ok(()),
                Err(_e) => Err(ActlibError::InvalidActorRef(
                    "This ActorRef is no longer connected to an Actor".to_string(),
                )),
            },
            ActorRefChannel::Remote(s) => {
                if let Ok(message_serialized) = bincode::serialize(&message) {
                    match s.send((
                        self.clone_id(),
                        SerNetMessageContent::Message(message_serialized),
                    )) {
                        Ok(_) => Ok(()),
                        Err(e) => Err(ActlibError::InvalidActorRef(format!(
                            "Can no longer send Messages to remote Actors: {:?}",
                            e
                        ))),
                    }
                } else {
                    Err(ActlibError::NetworkError(
                        "Unable to serialize message".to_string(),
                    ))
                }
            }
        }
    }

    /// Send a Message after some time has passed.
    /// The current thread is not blocked.
    pub fn send_delayed_message<'de, M: Message<'de> + 'static>(
        &self,
        message: M,
        delay: std::time::Duration,
    ) {
        let actor_ref_clone = self.clone();
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            // there is no way to react to this error, except blocking the calling thread
            // we don't want that
            let _ = actor_ref_clone.send_message(message);
        });
    }

    /// Clones only the associated [ActorId](struct.ActorId).
    ///
    /// **Hint**: [ActorRef](struct.ActorRef.html) as a whole implements Clone.
    pub fn clone_id(&self) -> ActorId {
        self.actor_id.clone()
    }
}
