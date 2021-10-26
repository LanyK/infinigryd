//! This file contains a sample program using the actlib library.
//!
//! It includes among other things:
//! defining Actors with and without state,
//! creating an Environment,
//! sending Messages that are/aren't handled.

use actlib::api::*;
use log::error;
#[allow(unused_imports)]
use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use std::fs::File;
#[allow(unused_imports)]
use std::io::Read;
use std::net::SocketAddr;
use std::thread;
use std::time;

/// This is an example for an [Actor](../actlib/actor/trait.Actor.html) without a state.
#[derive(Debug)]
pub struct ExampleActor;

impl Actor for ExampleActor {
    fn on_start(&mut self, _local_env: Environment, _own_ref: ActorRef) {
        println!("{:?}", "ON_START called");
    }

    fn on_stop(&mut self) {
        println!("{:?}", "ON_STOP called.");
    }
}

/// Example [Message](../actlib/message/trait.Message.html), consisting of ```i32``` and a ```String```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ping(i32, String);

impl Ping {
    /// Clone self and increase own counter, leaving the Message unchanged.
    fn clone_and_increase(&mut self) -> Ping {
        let new_ping = self.clone();
        self.0 += 1;
        new_ping
    }
}

fn example_handle_ping(_example: &ExampleActor, Ping(i, _str): &Ping) {
    println!("Received message: {}", i)
}

impl_message_handler!(ExampleActor: Ping => example_handle_ping);

/// This is an example for an [Actor](../actlib/actor/trait.Actor.html) with a state.
#[derive(Debug)]
pub struct StateActor {
    state: i32,
    own_ref: Option<ActorRef>,
    local_env: Option<Environment>,
}

impl Actor for StateActor {
    fn on_start(&mut self, local_env: Environment, own_ref: ActorRef) {
        self.own_ref = Some(own_ref);
        self.local_env = Some(local_env);
        println!(
            "Hello from {:?}. My current state is: {}",
            self.own_ref.as_ref().unwrap().clone_id(),
            self.state
        );
    }

    fn on_stop(&mut self) {
        println!(
            "Goodbye from {:?}. My final state is: {}",
            self.own_ref.as_ref().unwrap().clone_id(),
            self.state
        );
        println!("Spawning an ExampleActor...");
        match self.local_env.as_ref().unwrap().spawn("ExampleActor") {
            Ok(actor_ref) => {
                println!("... and got a actor_ref: {:?}", actor_ref);
            }
            Err(e) => {
                println!("... and got an Error: {:?}", e);
            }
        }
    }
}

fn state_handle_ping(mut actor: &mut StateActor, Ping(i, _str): &Ping) {
    println!("Received message {} with state {}.", i, actor.state);
    actor.state = *i;
    if let Some(self_ref) = &actor.own_ref {
        self_ref.send_message(QueryState);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryState;

fn state_handle_query(actor: &StateActor, _msg: &QueryState) {
    println!("--> {}", actor.state)
}

impl_message_handler!(StateActor:
    Ping => state_handle_ping,
    QueryState => state_handle_query,
);

/// Sleep for a short while.
fn wait_a_bit() {
    thread::sleep(time::Duration::from_millis(400));
}

// fn control_listener(control_remote: SocketAddr, env_clone: Environment) {
//     let mut netchannel =
//         NetChannel::as_server(env_clone.get_local_machine().unwrap(), control_remote);
//     match netchannel.split() {
//         Ok((mut writer, mut reader)) => loop {
//             let mut buf = [0_u8; 1024];
//             match reader.read(&mut buf) {
//                 Ok(num_bytes) => {
//                     if num_bytes == 0 {
//                         break;
//                     }
//                     if let Ok(env_serialize) = bincode::serialize(env_clone.get_state()) {
//                         writer.write(&env_serialize[..]);
//                     }
//                 }
//                 Err(_) => {}
//             }
//         },
//         Err(_) => {} //TODO?
//     }
// }

fn main() {
    let hostname = match hostname::get() {
        Ok(hostname) => hostname.into_string().unwrap(),
        Err(error) => panic!("{:?}", error),
    };
    // load remote machines from a configuration file
    let mut cfg = match File::open("./machines.cfg") {
        Ok(cfg) => cfg,
        Err(_e) => match File::open("./cfg/machines.cfg") {
            Ok(cfg) => cfg,
            Err(e) => panic!(
                "Failed to open config: {}\nCurrent working directory: {}",
                e,
                std::env::current_dir().unwrap().display()
            ),
        },
    };
    let mut cfg_contents: [u8; 1024] = [0; 1024];
    if let Err(e) = cfg.read(&mut cfg_contents) {
        panic!("Failed to read config: {}", e);
    };
    let remotes = match bincode::deserialize::<[SocketAddr; 2]>(&cfg_contents) {
        Ok(remotes) => remotes.to_vec(),
        Err(e) => panic!("Desrealisation of config failed: {}", e),
    };

    println!("actlib main: {:?}, we are {:?}", remotes, hostname);

    //create a new environment
    let actor_builder = actor_builder!(
        "StateActor" => StateActor {
            state: 0,
            own_ref: None,
            local_env: None,
        },
        "ExampleActor" => ExampleActor
    );

    // let env = Environment::new(&remotes);
    let (mut env, expiration_checker) = Environment::new(4020, &remotes, actor_builder);
    // let (mut env, expiration_checker) = Environment::new_local_only(actor_builder);
    // let env_clone = env.clone();
    // std::thread::spawn(move || {
    //     control_listener(control_remote, env_clone);
    // });

    if &hostname == "agakauitai" {
        let actor_example;
        let actor_state;

        match env.spawn("ExampleActor") {
            Ok(actor_ref) => {
                actor_example = actor_ref;
            }
            Err(e) => {
                error!("Error: {:?}", e);
                return;
            }
        }

        match env.spawn("StateActor") {
            Ok(actor_ref) => {
                actor_state = actor_ref;
            }
            Err(e) => {
                error!("Error: {:?}", e);
                return;
            }
        }

        wait_a_bit();

        // send Messages that both actors can handle.
        println!("\nSending Pings.");
        let mut ping = Ping(1, String::from("Ping"));
        let _ignored = actor_example.send_message(ping.clone_and_increase());
        let _ignored = actor_state.send_message(ping.clone_and_increase());

        wait_a_bit();

        // send Messages that no actor can handle.
        println!("\nSending ActorId.");
        let _ignored = actor_example.send_message(actor_state.clone_id());
        let _ignored = actor_state.send_message(actor_example.clone_id());

        wait_a_bit();

        // send Messages that only StateActor can handle.
        println!("\nQuerying State. Ping is now {:?}", ping);
        let _ignored = actor_example.send_message(QueryState);
        let _ignored = actor_state.send_message(QueryState);

        wait_a_bit();

        // remove Actors from the Environment (let them die).
        println!("\nRemove actors from Environment");
        let _ignore = env.remove(actor_example.clone());
        let _ignore = env.remove(actor_state.clone());

        wait_a_bit();

        // send Messages to removed (dead) Actors.
        println!("\nSending Pings after actor removal.");
        let _ignored = actor_example.send_message(ping.clone_and_increase());
        let _ignored = actor_state.send_message(ping.clone_and_increase());

        wait_a_bit();
        println!("done.");

        // set expire (comment to check if it blocks otherwise)
        
        let _ignored = env.set_expired();
    }
    // wait until expiration
    println!("Wait until expiration.");
    if let Err(e) = expiration_checker.wait_until_expiration() {
        panic!("Something went wrong: {:?}", e);
    }
}
