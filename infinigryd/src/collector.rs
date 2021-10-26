use crate::position::*;
use actlib::api::*;
use hostname;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, Shutdown, SocketAddr, TcpListener};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct CollectingActor {
    pub state: Arc<Mutex<HashMap<ActorId, ActorInfo>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorInfo {
    position: Position,
    num_figures: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InjectCollector {
    pub collector_id: ActorId,
}

fn collecting_actor_handler(
    actor_state: Arc<Mutex<HashMap<ActorId, ActorInfo>>>,
    listener: TcpListener,
) {
    loop {
        match listener.accept() {
            Ok((mut stream, _socket)) => match actor_state.lock() {
                Ok(locked_state) => match bincode::serialize(&locked_state.clone()) {
                    Ok(ser_state) => {
                        drop(locked_state);
                        // println!("Hello???");
                        // println!(
                        //     "[Collecting Actor] Write current state of len {:?} to client",
                        //     ser_state.len()
                        // );
                        //println!("Writing to client {:?}", ser_state);
                        let _ = stream.write(&ser_state[..]);
                        let _ = stream.flush();
                        //let _ = stream.shutdown(Shutdown::Both);
                    }
                    Err(_) => {
                        println!("could not serialize state");
                        let _ = stream.shutdown(Shutdown::Both);
                    }
                },
                Err(e) => {
                    println!("Cannot get lock of collecting actor");
                }
            },
            Err(e) => {
                println!("Listener accept failed");
            }
        }
    }
}

impl Actor for CollectingActor {
    fn on_start(&mut self, _local_env: Environment, _own_ref: ActorRef) {
        // println!("{:?}", "ON_START called");
        match TcpListener::bind(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(141, 84, 94, 111)),
            4028,
        )) {
            Ok(listener) => {
                let state_clone = self.state.clone();
                std::thread::spawn(move || {
                    collecting_actor_handler(state_clone, listener);
                });
            }
            //server already existing?? // TODO
            Err(_) => {}
        }
    }
    fn on_stop(&mut self) {
        println!("{:?}", "Collector went offline.");
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateState {
    pub actor_id: ActorId,
    pub position: Position,
    pub num_figures: usize,
}

impl UpdateState {}

fn update_state(actor: &mut CollectingActor, new_state: &UpdateState) {
    match actor.state.lock() {
        Ok(mut locked_state) => {
            if new_state.num_figures == 0 {
                locked_state.remove(&new_state.actor_id.clone());
            } else {
                locked_state.insert(
                    new_state.actor_id.clone(),
                    ActorInfo {
                        position: new_state.position.clone(),
                        num_figures: new_state.num_figures.clone(),
                    },
                );
            }
        }
        Err(_) => {
            println!("Could not get lock of actor state");
        }
    }
}

impl_message_handler!(CollectingActor: UpdateState => update_state);
