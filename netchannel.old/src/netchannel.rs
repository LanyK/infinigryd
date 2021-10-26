use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::prelude::*;
use std::net::{SocketAddr, SocketAddrV4};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::Sender;
use std::time;
#[macro_use]
extern crate log;
//use hostname;
use std::sync::mpsc::*;

static SERVER: &str = "127.0.0.1:54542";

#[derive(PartialEq, Eq, Hash, Debug, Clone, Serialize, Deserialize)]
pub struct Machine {
    pub name: String,
    pub socket: SocketAddrV4,
}

impl PartialOrd for Machine {
    fn partial_cmp(&self, other: &Machine) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Machine {
    fn cmp(&self, other: &Machine) -> std::cmp::Ordering {
        match self.name.cmp(&other.name) {
            std::cmp::Ordering::Equal => match self.socket.ip().cmp(&other.socket.ip()) {
                std::cmp::Ordering::Equal => self.socket.port().cmp(&other.socket.port()),
                ordering => ordering,
            },
            ordering => ordering,
        }
    }
}

// static machines: [Machine; 3] = [
//     Machine {
//         name: "agakauitai",
//         socket: SocketAddrV4::new(Ipv4Addr::new(141, 84, 94, 111), 4020),
//     },
//     Machine {
//         name: "haruku",
//         socket: SocketAddrV4::new(Ipv4Addr::new(141, 84, 94, 207), 4020),
//     },
//     Machine {
//         name: "client",
//         socket: SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 4020),
//     },
// ];

// I'm a Singleton, the only pattern I know :P
//
// While we can start multiple outgoing connection, there can only be one
// server listening to a port.
//
// https://docs.rust-embedded.org/book/peripherals/singletons.html

static mut ServerListener: Option<TcpListener> = None;

/// TODO
struct Server {}

impl Server {
    //
    // GET SINGLETON
    //
    fn get_instance() -> &'static TcpListener {
        unsafe {
            loop {
                match &ServerListener {
                    Some(listener) => {
                        return listener;
                    },
                    None => {
                        Server::start();
                    }
                }
            }
        }
    }

    //
    // START SERVER
    //
    // TODO: handle error here or pass it up?
    //
    fn start() {
        unsafe {
            ServerListener = match TcpListener::bind(SERVER) {
                Ok(listener) => Some(listener),
                Err(e) => {
                    error!("{:?}", e);
                    None
                }
            };
        }
    }
}

//
// PUBLIC API
//
// TODO: no, we don't want static lifetimes here
//

// Identifier for remote Machine.
// TODO: so far copied from cfg-generator, maybe change this in the future
// #[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Clone, Serialize, Deserialize)]
// pub struct Machine {
//     pub name: String,
//     pub id: u32,
//     pub ip: String,
//     pub port: u16,
// }

// impl ToSocketAddrs for Machine {
//     type Iter = std::vec::IntoIter<SocketAddr>;
//     fn to_socket_addrs(&self) -> std::io::Result<Self::Iter> {
//         let ip: &str = &self.ip;
//         (ip, self.port).to_socket_addrs()
//     }
// }

/// Channel ends connecting this machine to a remote machine
#[derive(Debug)]
pub struct NetChannel {
    from_remote: NetReceiver,
    to_remote: NetSender,
}

#[derive(Debug)]
pub struct NetSender(TcpStream);

impl NetSender {
    pub fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }
}

impl NetSender {
    pub fn send(&mut self, message: Vec<u8>) {
        match self.0.write(&message[..]) {
            Ok(_) => {
                println!("You are at OK");

                match self.0.flush() {
                    Ok(_) => {}
                    Err(error) => {
                        println!("{:?}", error);
                    }
                };
            }
            Err(error) => {
                println!("You are at Error");
                println!("{:?}", error);
                println!("You leaving Error");
            }
        };
    }
}

#[derive(Debug)]
pub struct NetReceiver(TcpStream);

impl NetReceiver {
    pub fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buf)
    }
}

impl NetChannel {
    ///
    /// This method will **block** until all connections have been established
    ///
    pub fn new(remotes: Vec<Machine>) -> HashMap<Machine, NetChannel> {
        if remotes.is_empty() {
            return HashMap::new();
        }

        // 3 Steps in this function:
        //  1. start worker thread gathering incoming connections
        //  2. start worker thread establishing outgoing connections
        //  3. wait for all pairs of connections and return them

        // listen for incoming connection attempts from other peers
        // get server singleton instance
        // a channel will relay the established connections back to this method
        let server = Server::get_instance();
        let (listener_thread_sender, listener_thread_receiver) = channel();
        let remote_count = remotes.len();

        let hostname = match hostname::get() {
            Ok(hostname) => hostname.into_string().unwrap(),
            Err(error) => panic!("{:?}", error),
        };

        // only accept as many incoming connections as there are known remotes
        let mut established_connections = 0;

        for remote in &remotes {
            if remote.name == hostname {
                // don't establish incoming connection with self
                established_connections += 1;
            }
        }

        // listener thread waiting for incoming connections
        // terminates after all machines are connected
        // TODO if the connections drop, can we try to reconnect?
        //      for that we'd need e.g. a function only returning a single NetChannel upon request of a Machine instance.
        std::thread::spawn(move || {
            while established_connections < remote_count {
                match server.accept() {
                    Ok((stream, sock)) => {
                        // established incoming connections are relayed back to the function
                        listener_thread_sender.send((stream, sock));
                        established_connections += 1;
                    }
                    Err(e) => {
                        println!("{:?}", e);
                    }
                }
            }
        });

        // channels for the outgoing connections to be sent back to this function from the worker thread
        let (outgoing_thread_sender, outgoing_thread_receiver) = channel();
        // try to reach every remote machine
        for remote in &remotes {
            // don't connect to yourself
            if remote.name == hostname {
                continue;
            }

            let sender_clone = outgoing_thread_sender.clone();
            let socket_clone = remote.socket.clone();

            std::thread::spawn(move || {
                let stream = connect_no_timeout(socket_clone);
                // send back connections to the function
                sender_clone.send((socket_clone, stream));
            });
        }

        // gather incoming and outgoing connections from the worker threads,
        //   starting with the outgoing senders
        let mut sender_map = HashMap::<SocketAddrV4, Option<TcpStream>>::new();
        for _machine in &remotes {
            if let Ok((listener, sock)) = listener_thread_receiver.recv() {
                if let SocketAddr::V4(sock4) = sock {
                    sender_map.insert(sock4, Some(listener));
                }
            }
        }

        // gather incoming connections and build the final NetConnections instances
        // collect into a HashMap and return
        let mut resulting_map = HashMap::new();
        for _machine in &remotes {
            if let Ok((sock, writer)) = outgoing_thread_receiver.recv() {
                if let Some(Some(listener)) = sender_map.remove(&sock) {
                    match NetChannel::get_machine_name(&remotes, &sock) {
                        Ok(name) => {
                            resulting_map.insert(
                                Machine {
                                    name: name,
                                    socket: sock,
                                },
                                NetChannel {
                                    from_remote: NetReceiver(listener),
                                    to_remote: NetSender(writer),
                                },
                            );
                        }
                        Err(e) => {
                            // TODO can that happen?
                            error!("{}", e);
                        }
                    }
                }
            }
        }
        resulting_map
    }

    /// Splits this NetChannel into (sender,receiver), where sender is an outgoing connection to a remote machine and receiver is the corresponding incoming connection.
    pub fn split(self) -> (NetSender, NetReceiver) {
        (self.to_remote, self.from_remote)
    }

    pub fn split_ref(&self) -> (&NetSender, &NetReceiver) {
        (&self.to_remote, &self.from_remote)
    }

    pub fn split_mut(&mut self) -> (&mut NetSender, &mut NetReceiver) {
        (&mut self.to_remote, &mut self.from_remote)
    }

    fn get_machine_name(machines: &Vec<Machine>, addr: &SocketAddrV4) -> Result<String, String> {
        for machine in machines {
            if &machine.socket == addr {
                return Ok(machine.name.clone());
            }
        }
        Err(
            "Error: Tried to get machine name of address that was not in the machines vector!"
                .to_string(),
        )
    }
    pub fn send(&mut self, message: Vec<u8>) {
        self.to_remote.send(message)
    }

    pub fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.from_remote.read(buf)
    }

    // TODO
    // decide what methods to implement:
    // - ref/mut-versions of split
    // - send/read defined on NetChannel
    // - both
}

fn connect_no_timeout(remote: SocketAddrV4) -> TcpStream {
    loop {
        match TcpStream::connect(remote) {
            Ok(stream) => {
                return stream;
            }
            Err(_error) => {
                // Don't spam - only try once per second.
                std::thread::sleep(time::Duration::from_millis(1000));
                continue;
            }
        };
    }
}

fn tcp_handler(mut stream: TcpStream, environment_sender: Sender<Vec<u8>>) {
    let mut buffer = [0; 32];
    stream
        .set_read_timeout(None)
        .expect("set_read_timeout call failed");
    loop {
        let res = stream.read(&mut buffer);
        match res {
            Ok(res) => {
                if res == 0 {
                    println!("client disconnected");
                    break;
                } else {
                    if let Err(e) = environment_sender.send(buffer.to_vec()) {
                        println!("Could not send incoming message to environment {}", e);
                    }
                }
            }
            Err(res) => {
                println!("Connection terminated {}", res);
            }
        }
    }
}
