use crate::workeractor::*;
use actlib::actor_builder;
use actlib::api::*;

mod workeractor;

fn main() {
    println!("HELLO WORLD");
    let (environment, expiration_checker) =
        Environment::new_local_only(actor_builder!("WorkerActor" => WorkerActor::new()));

    println!("ENV BUILT");

    let worker;

    match environment.spawn("WorkerActor") {
        Ok(actor_ref) => worker = actor_ref,
        Err(e) => panic!("{:?}", e),
    }

    println!("ACTOR SPAWNED");

    worker.send_message(StartWorkMessage {
        workload: vec![3; 8000],
    });

    println!("MSG SENT, WAITING...");

    match expiration_checker.wait_until_expiration() {
        Ok(_) => {}
        Err(e) => print!("{}", e),
    }
}
