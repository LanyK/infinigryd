use std::collections::HashMap;
use std::io::{Error, Read};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};

use serde::{Deserialize, Serialize};

use actlib::actor::ActorId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub(crate) x: i64,
    pub(crate) y: i64,
}

#[derive(Serialize, Deserialize, Debug)]
struct ActorInfo {
    position: Position,
    num_figures: usize,
}

type State = HashMap<ActorId, ActorInfo>;

fn main() {
    let collector = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(141, 84, 94, 111)), 4028);

    let mut buffer = [0_u8; 4194304];
    //let mut buffer = BufReader::new(f);
    let mut read_bytes;
    loop {
        match TcpStream::connect(collector) {
            Ok(mut stream) => {
                read_bytes = stream.read(&mut buffer);
                //println!("Read {:?} bytes", read_bytes);
                if let Ok(rb) = read_bytes {
                    //println!("\n read buffer raw {:?}", buffer[0..rb].to_vec());

                    match bincode::deserialize::<HashMap<ActorId, ActorInfo>>(&buffer[0..rb]) {
                        Ok(data_in) => {
                            let mut data_out: HashMap<String, ActorInfo> =
                                HashMap::with_capacity(data_in.len());

                            for (actor_id, actor_info) in data_in {
                                data_out.insert(actor_id.to_string(), actor_info);
                            }

                            //println!("deserialized state is {:?}", data_out);

                            println!("\n{:?}", serde_json::to_string(&data_out).unwrap());
                            break;
                        }
                        Err(_) => {
                            continue;
                        }
                    }
                }
            }
            Err(_) => {
                println!(
                    "unable to connect to collector on {:?}, try again in one second",
                    collector
                );
                std::thread::sleep(std::time::Duration::from_secs(1));
                continue;
            }
        };
    }
    if let Ok(rb) = read_bytes {
        //println!("\n read buffer raw {:?}", buffer[0..rb].to_vec());

        let data_in: HashMap<ActorId, ActorInfo> =
            bincode::deserialize::<HashMap<ActorId, ActorInfo>>(&buffer[0..rb]).unwrap();

        let mut data_out: HashMap<String, ActorInfo> = HashMap::with_capacity(data_in.len());

        for (actor_id, actor_info) in data_in {
            data_out.insert(actor_id.to_string(), actor_info);
        }

        //println!("deserialized state is {:?}", data_out);

        println!("\n{:?}\n", serde_json::to_string(&data_out).unwrap());
    }
}
