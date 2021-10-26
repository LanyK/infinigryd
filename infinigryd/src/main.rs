use crate::collector::*;
use crate::field::*;
use crate::position::*;
use actlib::api::*;
use hostname;
use log::{warn, info};
use simple_logger;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

pub mod collector;
pub mod field;
pub mod position;
// pub mod supervisor;

fn main() {
    simple_logger::init().unwrap();
    warn!("Starting the program :)");

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
        Ok(remotes) => remotes,
        Err(e) => panic!("Deseralisation of config failed: {}", e),
    };
    println!("infinygrid main: {:?}, we are {:?}", remotes, hostname);

    // Use port 4020 to establish a TCP-connection
    // let (env, expiration_checker) = Environment::new_local_only(
    let (env, expiration_checker) = Environment::new(
        4020,
        &remotes,
        actor_builder!(
            FIELD_INSTANCE_TYPE_ID => FieldInstance::new(),
            "CollectingActor" => CollectingActor{
                state: Arc::new(Mutex::new(HashMap::new()))
            }
        ),
    );

    if &hostname == "agakauitai" {
        let collecting_actor;
        match env.spawn_local_with_id("CollectingActor", Vec::new()) {
            Ok(actor_ref) => {
                collecting_actor = actor_ref;
            }
            Err(e) => {
                println!("Error: {:?}", e);
                return;
            }
        }

        let start_id: Vec<u8>;
        match bincode::serialize(&Position { x: 0, y: 0 }) {
            Ok(position) => start_id = position,
            Err(e) => panic!("Failed to serialize start Position: {:?}", e),
        }
        match env.spawn_with_id(FIELD_INSTANCE_TYPE_ID, start_id) {
            Ok(actor_ref) => {
                actor_ref.send_message(InjectCollector {
                    collector_id: collecting_actor.clone_id(),
                });
                for i in 0..128 {
                    actor_ref.send_message(PlayerEnters {
                        player: Player(i),
                        from: Direction::South,
                    });
                }
            }
            Err(e) => {
                // Sth. went wrong when spawning the actor.
                // Inform the user and exit
                println!("Encountered a problem while spawning an actor: {:?}", e);
                return;
            }
        }

        info!("RUNNING FOR SOME TIME...");
        std::thread::sleep(std::time::Duration::from_secs(64));
        env.broadcast(DebugQuery);

        std::thread::sleep(std::time::Duration::from_secs(2));
        info!("ENDING THE PROGRAM AFTER THE SET TIMER - NOW.");
        let _ignored = env.set_expired();
    }

    // Each Actor has its own thread to handle messages.
    // Since all of them terminate once the main function finishes
    // we have to wait block the current thread until the ```Environment::set_expired()```-method is called.
    // Note: This doesn't happen here, so we block indefinitely (until the user hits 'Ctrl+C').
    if let Err(e) = expiration_checker.wait_until_expiration() {
        panic!("Something went wrong: {:?}", e);
    }
}
