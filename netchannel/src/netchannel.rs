//!
//! NetChannel
//!
//! A NetChannel is a TCP connection to a remote Host
//!
//! Right now we unfortunately require a couple of guarantees by the user:
//!
//!   * Every host opens only one NetChannel to a remote or there be dragons.
//!   * The server listens only on one interface. We don't have the possibility
//!     to spawn more then one server thread. The API is already there and
//!     won't change when this is implemented.
//!

use log::*;
use std::io::prelude::*;
use std::io::{Error, ErrorKind};
use std::net::{IpAddr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

type ExpectedConnection = (IpAddr, Sender<TcpStream>);

// I'm a Singleton, the only pattern I know :P
//
// While we can start multiple outgoing connection, there can only be one
// server listening to a port.
//
// https://docs.rust-embedded.org/book/peripherals/singletons.html
//
// We really want a Mutex here. Unfortunately that's not possible for statics.
// Instead we (ab)use the fact that only one thread can listen on a SockAddr at
// any time as a mutex -- see run_server() method.
#[allow(non_upper_case_globals)]
static mut server_communicator: Option<Mutex<Sender<ExpectedConnection>>> = None;

enum Mode {
    Client,
    Server,
}

#[derive(Debug)]
pub struct NetChannel {
    stream: Arc<Mutex<Option<TcpStream>>>,
}

impl NetChannel {
    ///
    /// Determine whether we must connect as a client to the other guy or wait
    /// for his connection.
    ///
    /// TODO: Deterministically randomize who is server and client. Right now
    ///       the machine with the higher IP is client, which makes the lower
    ///       IP ranges bear more load.
    ///       We were thinking about something like
    ///           ( SocketAddr-0 XOR SocketAddr-1 ) mod 1
    ///       but the type system gets in the way.

    fn machine_type(local: &SocketAddr, remote: &SocketAddr) -> Mode {
        if local.ip() < remote.ip() {
            return Mode::Client;
        } else {
            return Mode::Server;
        }
    }

    ///
    /// Initialize Client Mode
    ///
    /// Once a connection is initialized, the stream is stored in self.stream
    /// behind a Mutex.
    ///
    fn run_client(&self, remote: SocketAddr) {
        match self.stream.lock() {
            Ok(mut stream) => loop {
                match TcpStream::connect(remote) {
                    Ok(incoming_stream) => {
                        *stream = Some(incoming_stream);
                        break;
                    }
                    Err(_) => {
                        thread::yield_now();
                        continue;
                    }
                };
            },
            Err(_) => error!("Coudn't acquire Mutex log for client stream."),
        }
    }

    ///
    /// Initialize Server Mode
    ///
    /// Try to start a server listener. If it fails one has to connect
    /// to it using the global server_communicator sender.
    ///
    fn run_server(&self, local: SocketAddr, remote: SocketAddr) {
        // Try to create a new server thread.
        match TcpListener::bind(local) {
            // Winner winner chicken dinner
            // We're first so let's start the server thread.
            Ok(listener) => {
                let (sender, receiver) = channel();
                thread::spawn(move || server(listener, sender, receiver));
            }
            // another thread has already started the server
            Err(_) => {
                error!("Port {} already taken.", local.port())
                // port is taken by another process
                // TODO: what to do if  _every_ NetChannels hits Err() here?
            }
        }

        // Inform the server about the expected connection
        // The receiver is a 'callback' where the server can inform us about an
        // incoming expected connection.
        let (sender, receiver) = channel();
        unsafe {
            // Rust's limitations regarding global static variables require us
            // to spin here to make it safe.
            // We might be able to do this by using AtomicPtr. But now it's too
            // late to change..
            loop {
                match &server_communicator {
                    Some(mutex) => {
                        match mutex.lock() {
                            Ok(sc) => {
                                let _ = sc.send((remote.ip(), sender));
                                // TODO: Error handling
                            }
                            Err(error) => {
                                panic!("Error acquiring lock: {:?}", error);
                            }
                        }
                        break;
                    }
                    None => {
                        thread::yield_now();
                        continue;
                    }
                }
            }
        }

        // At this point we're safe. All unsafe {} blocks are misnomers.

        // Start listening for the server to inform us about a connecting
        // remote we expect.
        let stream = self.stream.clone();
        let stream = stream.lock();
        loop {
            match receiver.recv() {
                Ok(remote) => {
                    match stream {
                        Ok(mut stream) => {
                            *stream = Some(remote);
                        }
                        Err(_) => {}
                    }
                    break;
                }
                Err(_) => {
                    continue;
                }
            };
        }
    }

    ///
    /// Create NetChannel
    ///
    /// Whether it acts as server or client is determined by the (local, remote)
    /// pair. The remote with the flipped pair will automaticalle use the other
    /// mode.
    pub fn new(local: SocketAddr, remote: SocketAddr) -> NetChannel {
        match Self::machine_type(&local, &remote) {
            Mode::Client => {
                return Self::as_client(remote);
            }
            Mode::Server => {
                return Self::as_server(local, remote);
            }
        };
    }

    /// Create NetChannel in Client Mode
    pub fn as_client(remote: SocketAddr) -> NetChannel {
        let netchannel = NetChannel {
            stream: Arc::new(Mutex::new(None)),
        };

        netchannel.run_client(remote);

        netchannel
    }

    /// Create NetChannel in Server Mode
    pub fn as_server(local: SocketAddr, remote: SocketAddr) -> NetChannel {
        let netchannel = NetChannel {
            stream: Arc::new(Mutex::new(None)),
        };

        netchannel.run_server(local, remote);

        netchannel
    }

    /// Split NetChannel in Sender in Receiver halves, similar to
    /// std::sync::mpsc::channel().
    pub fn split(&mut self) -> std::io::Result<(NetSender, NetReceiver)> {
        match self.stream.lock() {
            Ok(stream) => match &*stream {
                Some(stream) => {
                    let reader: TcpStream;
                    let writer: TcpStream;

                    match stream.try_clone() {
                        Ok(s) => {
                            reader = s;
                        }
                        Err(e) => {
                            return Err(e);
                        }
                    }
                    match stream.try_clone() {
                        Ok(s) => {
                            writer = s;
                        }
                        Err(e) => {
                            return Err(e);
                        }
                    }

                    Ok((NetSender { stream: writer }, NetReceiver { stream: reader }))
                }
                None => {
                    panic!("No TCP stream established despite Mutex. This code should never get executed!");
                }
            },
            Err(_) => Err(Error::new(ErrorKind::Other, "Mutex Poisoned")),
        }
    }
}

///
/// The main Server thread
///
/// It is here that we wait for incoming connections and pass them on to the
/// requesting NetChannel instance.
///
fn server(
    listener: TcpListener,
    sender: Sender<ExpectedConnection>,
    receiver: Receiver<ExpectedConnection>,
) {
    use std::collections::HashMap;

    let incoming: Arc<Mutex<HashMap<IpAddr, Sender<TcpStream>>>> =
        Arc::new(Mutex::new(HashMap::new()));

    let incoming2 = Arc::clone(&incoming);
    let incoming3 = Arc::clone(&incoming);

    thread::spawn(move || {
        // It is important the only here the server_communicator is populated.
        // Else the NetChannels will start doing stuff before the server has
        // started and is listening for messages on its receiver.
        unsafe {
            server_communicator = Some(Mutex::new(sender));
        }

        for (client, thread) in receiver.iter() {
            let mut incoming = incoming2.lock().unwrap();
            incoming.insert(client, thread);
        }
    });

    loop {
        match listener.accept() {
            Ok((stream, socket)) => {
                match incoming3.lock().unwrap().get_mut(&socket.ip()) {
                    // If a NetChannel has requested this connection, pass it on
                    Some(receiver) => {
                        let _ = receiver.send(stream);
                    }
                    // ... else just close it.
                    None => {
                        let _ = stream.shutdown(Shutdown::Both);
                    }
                };
            }
            Err(_) => (),
        }
    }
}

/// Reading half of the NetChannel
#[derive(Debug)]
pub struct NetReceiver {
    stream: TcpStream,
}

// TODO: properly implement Read Trait.
impl NetReceiver {
    pub fn read<'a>(&mut self, buffer: &'a mut [u8]) -> std::io::Result<Vec<&'a [u8]>> {
        let mut results: Vec<&[u8]> = Vec::with_capacity(5);
        let size = self.stream.read(buffer)?;
        let mut pointer = 0_usize;

        if size == 0 {
            return Err(Error::new(
                std::io::ErrorKind::BrokenPipe,
                "Read 0 bytes form TCP stream",
            ));
        }

        let mut size_bits: &[u8] = &buffer[pointer..pointer + 2];
        let mut len = ((size_bits[0] as u16) * 256) | size_bits[1] as u16;

        while len > 0 {
            pointer = pointer + 2;
            let obj = &buffer[pointer..pointer + len as usize];
            results.push(obj);
            pointer += len as usize;
            size_bits = &buffer[pointer..pointer + 2];
            len = ((size_bits[0] as u16) * 256) | size_bits[1] as u16;
        }
        Ok(results)
    }
}

impl Clone for NetReceiver {
    fn clone(&self) -> Self {
        match self.stream.try_clone() {
            Ok(clone) => {
                return NetReceiver { stream: clone };
            }
            Err(error) => {
                panic!("Cloning NetReceiver failed: {:?}", error);
            }
        }
    }
}

/// Writing half of the NetChannel
#[derive(Debug)]
pub struct NetSender {
    stream: TcpStream,
}

// TODO: properly implement Write Trait.
impl NetSender {
    pub fn write(&mut self, bin_obj: &[u8]) -> std::io::Result<usize> {
        let mut array = Vec::with_capacity(bin_obj.len() + 2);
        let len = u16::to_be_bytes(bin_obj.len() as u16);
        for val in &len {
            array.push(*val);
        }
        for val in bin_obj {
            array.push(*val);
        }
        self.stream.write(&array[..])
    }
}

impl Clone for NetSender {
    fn clone(&self) -> Self {
        match self.stream.try_clone() {
            Ok(clone) => {
                return NetSender { stream: clone };
            }
            Err(error) => {
                panic!("Cloning NetSender failed: {:?}", error);
            }
        }
    }
}
