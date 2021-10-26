use actlib::api::*;
use actlib::impl_message_handler;
use serde::Deserialize;
use serde::Serialize;

// Actor Handler implementations
impl_message_handler!(WorkerActor: IAmYourFather => handle_father_message, DoWorkMessage => handle_do_work_message, StartWorkMessage => handle_start_work_message, ResultMessage => handle_result_message);

#[derive(Debug, Clone)]
enum Children {
    None,
    Left(ActorRef),
    Right(ActorRef),
    LeftAndRight(ActorRef, ActorRef),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum ParentDirection {
    Left,
    Right,
    None,
}

#[derive(Debug)]
pub(crate) struct WorkerActor {
    env: Option<Environment>,
    self_ref: Option<ActorRef>,
    parent_info: (ParentDirection, Option<ActorRef>),
    partial_result: Vec<i32>,
    children: Children,
}

impl Actor for WorkerActor {
    fn on_start(&mut self, env: Environment, self_ref: ActorRef) {
        self.env = Some(env);
        self.self_ref = Some(self_ref);
    }
}

impl WorkerActor {
    pub fn new() -> Self {
        WorkerActor {
            env: None,
            self_ref: None,
            parent_info: (ParentDirection::None, Option::None),
            partial_result: Vec::with_capacity(1),
            children: Children::None,
        }
    }

    fn get_parent_info(&self) -> &(ParentDirection, Option<ActorRef>) {
        &self.parent_info
    }

    fn remove_child(&self, child_ref: &ActorRef) {
        if let Some(env) = &self.env {
            let _res = env.clone().remove(child_ref.clone());
        }
    }

    /// Splits and hands along parts of the workload to child actors
    fn hand_along_workload_parts(&mut self, local_workload: Vec<i32>) {
        let i: usize = (local_workload.len() / 2) as usize;
        let slice = &local_workload[0..i];
        let mut left_work = vec![0; slice.len()];
        left_work.copy_from_slice(slice);
        let slice = &local_workload[i..];
        let mut right_work = vec![0; slice.len()];
        right_work.copy_from_slice(slice);

        match &self.env {
            Some(env) => {
                // left worker
                match env.spawn("WorkerActor") {
                    Ok(actor_ref) => {
                        actor_ref.send_message(IAmYourFather(
                            ParentDirection::Left,
                            self.self_ref.clone().unwrap().clone_id(),
                        ));
                        actor_ref.send_message(DoWorkMessage {
                            workload: left_work,
                        });
                        match self.children.clone() {
                            Children::None => {
                                self.children = Children::Left(actor_ref);
                            }
                            Children::Right(right) => {
                                self.children = Children::LeftAndRight(actor_ref, right);
                            }
                            _ => {
                                panic!("Tried to register more than 1 left child");
                            }
                        }
                    }
                    Err(e) => {
                        panic!("{:?}", e);
                    }
                }
                // right worker
                match env.spawn("WorkerActor") {
                    Ok(actor_ref) => {
                        actor_ref.send_message(IAmYourFather(
                            ParentDirection::Right,
                            self.self_ref.clone().unwrap().clone_id(),
                        ));
                        actor_ref.send_message(DoWorkMessage {
                            workload: right_work,
                        });
                        match self.children.clone() {
                            Children::None => {
                                self.children = Children::Right(actor_ref);
                            }
                            Children::Left(left) => {
                                self.children = Children::LeftAndRight(left, actor_ref);
                            }
                            _ => {
                                panic!("Tried to register more than 1 right child");
                            }
                        }
                    }
                    Err(e) => {
                        panic!("{:?}", e);
                    }
                }
            }
            _ => panic!("Empty: actor's environment field"),
        }
    }
}

/// Hands over parts of the workload to a child actor
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct DoWorkMessage {
    workload: Vec<i32>,
}

/// lets the actor begin working, including the handover of parts of the workload
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct StartWorkMessage {
    pub workload: Vec<i32>,
}

/// hands back results to parent actors
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct ResultMessage(ParentDirection, i32);

/// tells an actor that the sender is the parent actor
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct IAmYourFather(ParentDirection, ActorId);

fn handle_father_message(actor: &mut WorkerActor, msg: &IAmYourFather) {
    let IAmYourFather(dir, actor_ref) = msg;
    match &actor.env {
        Some(env) => match env.to_actor_ref(actor_ref.clone()) {
            Ok(actor_ref) => {
                actor.parent_info = (dir.clone(), Some(actor_ref));
            }
            Err(e) => {
                panic!(e);
            }
        },
        None => {
            panic!("Empty: actor's environment field");
        }
    }
}

fn handle_do_work_message(actor: &mut WorkerActor, msg: &DoWorkMessage) {
    let mut local_workload = vec![0; msg.workload.len()];
    local_workload.copy_from_slice(msg.workload.as_slice());
    drop(msg);
    match local_workload.len() {
        0 => {
            // No work to do
            panic!("Error: No workload given in DoWorkMessage, should never happen!");
        }
        1 => {
            // println!("Got 1 piece of work to handle!");
            let result = (local_workload.pop().unwrap()) * 2 as i32;
            actor
                .get_parent_info()
                .1
                .clone()
                .unwrap()
                .send_message(ResultMessage(actor.get_parent_info().0.clone(), result));
        }
        _ => {
            // more than 1 piece of work - split workload
            actor.hand_along_workload_parts(local_workload)
        }
    }
}

fn handle_start_work_message(actor: &mut WorkerActor, msg: &StartWorkMessage) {
    let mut local_workload = vec![0; msg.workload.len()];
    local_workload.copy_from_slice(msg.workload.as_slice());
    drop(msg);
    match local_workload.len() {
        0 => {
            // No work to do
            println!("No workload given.");
            if let Some(env) = &actor.env {
                let _r = env.set_expired();
            }
        }
        1 => {
            // 1 Value: Compute Result instantly
            println!(
                "Easy Result: {}",
                (local_workload.pop().unwrap() * 2) as i32
            );
            if let Some(env) = &actor.env {
                let _r = env.set_expired();
            }
        }
        _ => {
            // more than 1 piece of work - split workload
            println!("Workload #: {}", local_workload.len());
            actor.hand_along_workload_parts(local_workload);
        }
    }
}

fn handle_result_message(actor: &mut WorkerActor, msg: &ResultMessage) {
    let ResultMessage(dir, result) = msg;

    match actor.partial_result.len() {
        0 => {
            actor.partial_result.push(result.clone());
            remove_child(&actor, dir);
        }
        1 => {
            match actor.get_parent_info() {
                // has parent, relay result
                (dir, Some(actor_ref)) => {
                    actor_ref
                        .send_message(ResultMessage(dir.clone(), actor.partial_result[0] + result));
                    remove_child(&actor, dir);
                }
                // has no parent, therefore this is the top level actor and returns the result
                (_dir, Option::None) => {
                    println!("END Result: {}", actor.partial_result[0] + result);
                    // end actor system
                    if let Some(env) = &actor.env {
                        let _r = env.set_expired();
                    }
                }
            }
        }
        _ => {
            panic!(
                "Invalid state, got result message, but internal result stack had size {}",
                actor.partial_result.len()
            );
        }
    }
}

/// Will silently fail if ParentDirection and registered children do not match
fn remove_child(actor: &WorkerActor, dir: &ParentDirection) {
    match &actor.children {
        Children::None => {}
        Children::Left(c) => {
            if *dir == ParentDirection::Left {
                actor.remove_child(c);
            }
        }
        Children::Right(c) => {
            if *dir == ParentDirection::Right {
                actor.remove_child(c);
            }
        }
        Children::LeftAndRight(l, r) => {
            if *dir == ParentDirection::Left {
                actor.remove_child(l);
            } else if *dir == ParentDirection::Right {
                actor.remove_child(r);
            }
        }
    }
}
