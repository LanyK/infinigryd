use crate::collector::*;
use crate::position::*;
use actlib::api::*;
use colored::Colorize;
use log::*;
use rand::prelude::{thread_rng, SliceRandom};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// dummy type with a u64 to have different Players.
#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct Player(pub u64);

/// Notify a FieldInstance a player entering from a Direction.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PlayerEnters {
    pub(crate) player: Player,
    pub(crate) from: Direction,
}

/// System Message to trigger a Player to leave in a certain Direction.
#[derive(Debug, Serialize, Deserialize, Clone)]
struct ForcePlayerLeave {
    player: Player,
    to: Direction,
}

/// System Message to force a field to print its current state.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DebugQuery;

/// System Message to inform about a newly spawned neighbouring actor in the specified direction.
#[derive(Debug, Serialize, Deserialize, Clone)]
struct FieldInstanceSpawned {
    actor_id: ActorId,
    direction: Direction,
}

/// System Message to inform about a removed neighbouring actor in the specified direction.
#[derive(Debug, Serialize, Deserialize, Clone)]
struct FieldInstanceDied {
    actor_id: ActorId,
    direction: Direction,
}

/// System Message to forward a Message to the nearest neighbour in the specified direction.
#[derive(Debug, Serialize, Deserialize, Clone)]
struct ForwardSpawnMessage {
    message: FieldInstanceSpawned,
    to: Direction,
}

pub(crate) const FIELD_INSTANCE_TYPE_ID: &str = "FieldInstance";

/// One Pacman-like Field
#[derive(Debug)]
pub struct FieldInstance {
    /// The players currently on this instance.
    pub players: HashSet<Player>,
    /// own ActorRef
    pub(crate) own_ref: Option<ActorRef>,
    /// actlib environment
    pub(crate) environment: Option<Environment>,
    /// The position of the Field in the infinite Grid.
    pub position: Option<Position>,
    ///Collector
    pub collector: Option<ActorRef>,
}

impl FieldInstance {
    /// Return a new uninitialized FieldInstance.
    pub fn new() -> FieldInstance {
        FieldInstance {
            players: HashSet::new(),
            own_ref: None,
            environment: None,
            position: None,
            collector: None,
        }
    }
    /// unwrap-wrapper for self.own_ref
    pub(crate) fn unwrap_own_ref(&self) -> &ActorRef {
        self.own_ref.as_ref().unwrap()
    }

    /// unwrap-wrapper for self.environment
    pub(crate) fn unwrap_environment(&self) -> &Environment {
        self.environment.as_ref().unwrap()
    }

    /// unwrap-wrapper for self.position
    pub(crate) fn unwrap_position(&self) -> &Position {
        self.position.as_ref().unwrap()
    }

    /// unwrap-wrapper for self.own_ref
    pub(crate) fn unwrap_mut_own_ref(&mut self) -> &mut ActorRef {
        self.own_ref.as_mut().unwrap()
    }

    /// unwrap-wrapper for self.environment
    pub(crate) fn unwrap_mut_environment(&mut self) -> &mut Environment {
        self.environment.as_mut().unwrap()
    }

    /// unwrap-wrapper for self.position
    pub(crate) fn unwrap_mut_position(&mut self) -> &mut Position {
        self.position.as_mut().unwrap()
    }

    fn inject_collector(&mut self, collector: &InjectCollector) {
        match self
            .unwrap_environment()
            .to_actor_ref(collector.collector_id.clone())
        {
            Ok(collector_ref) => {
                self.collector = Some(collector_ref);
            }
            Err(_) => {
                println!(
                    "Error creating actor ref from env for {:?}",
                    collector.collector_id
                );
            }
        }
    }

    fn send_state_update(&mut self) {
        if let Some(collector) = &self.collector {
            if let Some(position) = &self.position {
                collector.send_message(UpdateState {
                    actor_id: self.unwrap_own_ref().clone_id(),
                    position: position.clone(),
                    num_figures: self.players.len(),
                });
            } else {
                println!("Failed position {:?}", self.players.len());
            }
        }
    }

    fn handle_incoming_actor(&mut self, new_player_message: &PlayerEnters) {
        self.players.insert(new_player_message.player.clone());
        self.send_state_update();
        let mut rng = thread_rng();
        // unwrap is safe here, since DIRECTIONS is non-empty
        let outgoing = DIRECTIONS.choose(&mut rng).unwrap();
        // let max_delay: u64;
        // if outgoing == &new_player_message.from {
        //     max_delay = 100;
        // } else if outgoing.reverse() == new_player_message.from {
        //     max_delay = 50;
        // } else {
        //     max_delay = 150;
        // }
        let delay = std::time::Duration::from_millis(1500); //rng.gen_range(50, max_delay) + 1000);
                                                            // unwrap is save here, since we can only get messages after on_start has been called.

        self.unwrap_own_ref().send_delayed_message(
            ForcePlayerLeave {
                player: new_player_message.player.clone(),
                to: outgoing.clone(),
            },
            delay,
        );
    }

    fn handle_force_player_leave(&mut self, outgoing_player_message: &ForcePlayerLeave) {
        let local_id =
            match bincode::serialize(&self.unwrap_position().next(&outgoing_player_message.to)) {
                Ok(actor_id) => actor_id,
                Err(e) => panic!("Could not serialize neighbor position: {:?}", e),
            };
        // unwrap is save here, since we can only receive Messages after on_start has been called.
        let own_actor_id = self.own_ref.as_ref().unwrap().clone_id();
        match self
            .unwrap_environment()
            .find_actor_ref(&local_id, own_actor_id.clone(), true)
        {
            Ok(Some(neighbour)) => {
                // ignore non-existent actor: make sure only moves occur
                let _ = self.players.remove(&outgoing_player_message.player);

                neighbour.send_message(PlayerEnters {
                    player: outgoing_player_message.player.clone(),
                    from: outgoing_player_message.to.reverse(),
                });
                self.send_state_update();
                self.unwrap_environment()
                    .drop_protector(own_actor_id, neighbour.clone_id());
                if self.players.is_empty() {
                    // println!("[E] Removing Field: {:?}", self.unwrap_position());
                    let own_ref = self.unwrap_own_ref().clone();
                    self.unwrap_mut_environment().remove(own_ref);
                    // println!("[E] After Remove call: was empty");
                }
            }
            Ok(None) => {
                // spawn new field actor in desired direction
                match self
                    .unwrap_environment()
                    .spawn_with_id(FIELD_INSTANCE_TYPE_ID, local_id)
                {
                    Ok(new_ref) => {
                        match &self.collector {
                            Some(c) => {
                                new_ref.send_message(InjectCollector {
                                    collector_id: c.clone_id(),
                                });
                            }
                            None => {
                                match self.unwrap_environment().find_actor_ref(
                                    &Vec::new(),
                                    self.unwrap_own_ref().clone_id(),
                                    false,
                                ) {
                                    Ok(opt_collector) => {
                                        if let Some(collector) = opt_collector {
                                            new_ref.send_message(InjectCollector {
                                                collector_id: collector.clone_id(),
                                            });
                                            self.collector = Some(collector);
                                        } else {
                                            error!(
                                                "Could not find Collector via it's fixed address!"
                                            );
                                        }
                                    }
                                    Err(actlib_err) => {
                                        error!("{:?}", actlib_err);
                                    }
                                }
                            }
                        }

                        // unwraps used (own_ref, environment) are save here
                        // we can only get messages after on_start has been called.
                        // send message to self to move player there (no infinite loop, since actor now exists)
                        self.unwrap_own_ref()
                            .send_message(outgoing_player_message.clone());
                    }
                    Err(e) => {
                        // Failed to spawn actor
                        error!("{:?}", e);
                    }
                }
            }
            Err(e) => {
                // an error at searching for an actor happened
                panic!("An error when searching for an Actor happened: {:?}", e);
            }
        }
    }

    fn debug_query(&self, _debug_query: &DebugQuery) {
        println_green(&format!(
            "Field at {:?} holds {} players.",
            self.position,
            self.players.len()
        ));
    }
}

impl Actor for FieldInstance {
    fn on_start(&mut self, mut local_env: Environment, own_ref: ActorRef) {
        if let Some(local_id) = own_ref.clone_id().when_specified() {
            match bincode::deserialize::<Position>(&local_id) {
                Ok(position) => {
                    self.environment = Some(local_env);
                    self.own_ref = Some(own_ref);
                    self.position = Some(position);
                }
                Err(e) => {
                    warn!("Spawned Field with invalid user specified Id: {:?}", e);
                    let _ = local_env.remove(own_ref);
                }
            }
        } else {
            error!("Spawned Field without user specified Id.");
            let _ = local_env.remove(own_ref);
        }
    }

    fn on_reset(&mut self) {
        info!("Reset {} players.", self.players.len());
        self.players.clear();
    }
}

fn println_green(s: &str) {
    println!("{}", s.green());
}

fn println_red(s: &str) {
    println!("{}", s.red());
}

impl_message_handler!(FieldInstance:
    PlayerEnters => FieldInstance::handle_incoming_actor,
    ForcePlayerLeave => FieldInstance::handle_force_player_leave,
    DebugQuery => FieldInstance::debug_query,
    InjectCollector => FieldInstance::inject_collector
);
