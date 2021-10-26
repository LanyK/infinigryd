//! This module defines traits describing the ability to be passed as, or receive a [Message](trait.Message.html).

use crate::actor::*;
pub use crate::impl_message_handler;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::fmt::Debug;
use std::net::IpAddr;
use std::sync::mpsc::{Receiver, RecvError};

/// Trait to enable types to [handle](#tymethod.handle) [Messages](trait.Message.html).
///
/// For an automatic implementation the [impl_message_handler!](../macro.impl_message_handler.html)-macro is provided.
pub trait MessageHandler {
    /// Specify how to handle a message implementing the ```std::any::Any``` trait.
    ///
    /// Possible reactions include mutating your own state, sending new messages, ignoring the message, etc.
    ///
    /// **Note:** It is expected that this function terminates.
    fn handle(&mut self, message: Box<dyn Any>);

    /// Specify how to deserialize a message to an ```std::any::Any``` trait object.
    ///
    /// This method is called, before an incoming message from an external environment is relayed to a local actor.
    /// The message was serialized using ```bincode::serialize```.
    ///
    /// It has to be user-specified, since we don't know the types which we should deserialize to.
    ///
    /// **Note:** It is expected that this function terminates.
    fn deserialize_to_any(&self, message: &[u8]) -> Option<Box<dyn Any + Send>>;
}

/// Trait that enables a type to be send to an [Actor](../actor/trait.Actor.html).
///
/// This is just a shortcut summarizing the traits required for a type to be send.
pub trait Message<'de>: Debug + Send + Serialize + Deserialize<'de> {}

// Implementation for a generic type that satisfies all requirements.
// This way the Message-trait is truly a shortcut to all required traits.
impl<'de, T: Debug + Send + Serialize + Deserialize<'de>> Message<'de> for T {}

#[macro_export]
/// This macro tries to implement the [MessageHandler](message/trait.MessageHandler.html)-Trait for the specified type.
///
/// The first argument is *$actor_type* followed by a colon.
///
/// The following arguments are of the form ```$message_type => $handle_function``` separated by commas.
///
/// The [handle](message/trait.MessageHandler.html#method.handle)-method is implemented in the following way:
///
/// * For every type, a conversion of the Message to specified $message_type using ```downcast_ref``` is attempted.
/// * If this conversion succeeds, the associated $handle_function is called.
/// * This is repeated for every specified *$message_type => $handle_function* pair.
///
/// The [deserialize_to_any](message/trait.MessageHandler.html#tymethod.deserialize_to_any)-method is implemented in a similar fashion,
/// replacing ```downcast_ref``` with ```bincode::deserialize```.
///
/// **Note:** It is expected that all $handle_function terminate.
///
/// For example, calling the macro as
/// ```rust
/// impl_message_handler!(ExampleActor, String => my_handle_function)
/// ```
/// will result in the expansion
///
/// ```rust
/// impl MessageHandler for ExampleActor {
///     fn handle(&mut self, message: Box<dyn std::any::Any>) {
///         if let Some(message_typed) = message.downcast_ref::<String>() {
///             my_handle_function(self, message_typed);
///         }
///     }
///
///     fn deserialize_to_any(&self, message: &[u8]) -> Option<Box<dyn std::any::Any + Send>> {
///         let mut result: Option<Box<dyn std::any::Any + Send>> = None;
///         if let Ok(message_deserialized) = bincode::deserialize::<String>(&message) {
///             result = Some(Box::new(message_deserialized));
///         }
///         result
///     }
/// }
/// ```
macro_rules! impl_message_handler {
    ($actor_type:ty: $($message_type:ty => $handle_function:expr),*$(,)?) => {
        impl MessageHandler for $actor_type {
            fn handle(&mut self, message: Box<dyn std::any::Any>) {
                $(
                    if let Some(message_typed) = message.downcast_ref::<$message_type>() {
                        $handle_function(self, message_typed);
                    } else
                )*
                {
                    // log::warn!("All downcast-attempts failed.");
                    // all conversion attempts failed
                    // ignore message
                }
            }

            fn deserialize_to_any(&self, message: &[u8]) -> Option<Box<dyn std::any::Any + Send>> {
                let result: Option<Box<dyn std::any::Any + Send>>;
                $(
                    if let Ok(message_deserialized) = bincode::deserialize::<$message_type>(&message) {
                        result = Some(Box::new(message_deserialized));
                    } else
                )*
                {
                    // all conversion attempts failed
                    // log::warn!("All desrealisation-attempts failed.");
                    result = None;
                }
                result
            }
        }
    };
}

/// An specialization of the ```std::sync::mpsc::Receiver```-type that only exposes a limited set of methods.
pub(crate) struct Mailbox {
    receiver: Receiver<EitherMessage>, // buffered receiving end of a channel
}

impl Mailbox {
    /// Create a new Mailbox.
    pub(crate) fn new(receiver: Receiver<EitherMessage>) -> Mailbox {
        Mailbox { receiver }
    }

    /// Attempts to wait for a value on this Mailbox, returning an error if the corresponding channel has hung up.
    ///
    /// Every remark from ```std::sync::mpsc::Receiver::recv``` apply to this method as well.
    pub(crate) fn wait_for_msg(&self) -> Result<EitherMessage, RecvError> {
        self.receiver.recv() // blocking
    }
}

/// Either type variant vocalized to the use case: An EitherMessage is either a regular message or a serialized message.
#[derive(Debug)]
pub(crate) enum EitherMessage {
    /// A serialized message of type ```String```
    Serialized(Vec<u8>),
    /// A non-serialized message of type ```Box<dyn Any + Send>```
    Regular(Box<dyn Any + Send>),
    /// Special Message-Token
    Special(Token),
}

/// Special Message-Token we send at specific points in the program.
/// The user should never see those.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum Token {
    /// Special Message-Token signaling a Stop-Request to an Actor.
    Stop,
    /// Special Message-Token signaling a Reset-Request to an Actor.
    Reset,
}

/// Messages that can be send to a remote Environment.
#[derive(Serialize, Deserialize)]
pub(crate) enum NetMessage {
    /// A User-defined, serialized Message
    Message(ActorId, Vec<u8>),
    /// binary serialized [Token]
    SpecialToken(ActorId, Vec<u8>),
    /// Spawn an Actor using the specified TypeId and LocalId
    SpawnByTypeId(String, LocalId),
    /// queried_id, return_addr, searcher_id, protected?
    QuerySpecifiedId(Vec<u8>, IpAddr, ActorId, bool),
    /// queried_id, searcher_id, result
    QuerySpecifiedIdResult(Vec<u8>, ActorId, Option<IpAddr>),
    /// RemoveProtector(protector: ActorId, target: ActorId)`
    RemoveProtector(ActorId, ActorId),
    /// Broadcast this Message to all Actors
    Broadcast(Vec<u8>),
    /// call send_expiration_signal
    SendExpirationSignal,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum SerNetMessageContent {
    Message(Vec<u8>),
    Token(Vec<u8>),
}
