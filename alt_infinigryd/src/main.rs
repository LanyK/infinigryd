use log::{warn,error,info};
use hostname;
use actlib::api::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use crate::field::Field;

mod field;
mod messages;



fn main() {
    simple_logger::init().unwrap();
    info!("Starting a infinigryd!");

    let hostname = match hostname::get() {
        Ok(hostname) => hostname.into_string().unwrap(),
        Err(error) => panic!("{:?}", error),
    };

    // TODO args parsing for --local or --remote_master --remote_slave

    // read config for remotes TODO

    // Use port 4020 to establish a TCP-connection
    // let (env, expiration_checker) = Environment::new_local_only(
    let (env, expiration_checker) = Environment::new_local_only(
        //4020,
        //&remotes,
        actor_builder!(
            Field::type_id() => Field::new(),
        ),
    );


    if let Err(e) = expiration_checker.wait_until_expiration() {
        panic!("Something went wrong: {:?}", e);
    }
}
