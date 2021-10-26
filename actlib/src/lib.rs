//! The *actlib* library provides a simple implementation of the Actor model.
//!
//! [Actors](./actor/trait.Actor.html) are isolated computing units with an internal state
//! and the ability to send [messages](./message/trait.Message.html) to other locally known actors
//! as well as [spawn](./api/struct.Environment.html#method.spawn) new actors on demand.
//!
//! Actors can perform required initialization and cleanup by implementing the ```on_start```, ```on_reset``` and ```on_stop``` methods.
//!
//! The library furthermore provides a means of abstraction where the [Environment](./api/struct.Environment.html)
//! that holds the actors can be seamlessly distributed along multiple machines without changing the API usage
//! or view on the data for the executing main program.
//!
//! An example program that defines an Actor type, sends it a String message which than gets printed to screen is shown below.
//! ```rust
//! use actlib::api::*;
//!
//! #[derive(Debug)]
//! struct MessageActor;
//!
//! impl Actor for MessageActor {}
//!
//! impl_message_handler!(MessageActor: String => |_actor, msg| {println!("{}", msg);});
//!
//! fn main() {
//!     let (env, expiration_checker) = Environment::new_local_only(
//!         actor_builder!("MessageActor" => MessageActor)
//!     );
//!
//!     match env.spawn(MessageActor) {
//!         Ok(actor_ref) => actor_ref.send_message("Hello, World".to_string()),
//!         Err(e) => {
//!             // Sth. went wrong when spawning the actor.
//!             // Inform the user and exit
//!             println!("Encountered a problem while spawning an actor: {}", e);
//!             return;
//!         }
//!     }
//!
//!     // Each Actor has its own thread to handle messages.
//!     // Since all of them terminate once the main function finishes
//!     // we have to block the current thread until the ```Environment::set_expired()```-method is called.
//!     // Note: This doesn't happen here, so we block indefinitely (until the user hits 'Ctrl+C').
//!     if let Err(e) = expiration_checker.wait_until_expiration() {
//!         panic!("Something went wrong: {:?}", e);
//!     }
//! }
//! ```

pub mod actor;
pub mod api;
pub(crate) mod environment;
pub(crate) mod errors;
pub mod message;
