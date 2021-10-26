use std::fs::File;
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;

///
/// Generate Machine Configuration
///
/// We use this to generate a serialized configuration file that's easy to
/// load.
///
fn main() {
    for iface in get_if_addrs::get_if_addrs().unwrap() {
        println!("{:#?}\nis_loopback: {}\n", iface, iface.is_loopback());
    }
    // generate machines
    // ids are generated using `uuidgen | xxh32sum`

    let machines: [SocketAddr; 2] = [
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(141, 84, 94, 111)), 4020),
        // Machine {
        //     name: "agakauitai".to_string(),
        //     socket: SocketAddrV4::new(Ipv4Addr::new(141, 84, 94, 111), 4020),
        // },
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(141, 84, 94, 207)), 4020),
        // Machine {
        //     name: "haruku".to_string(),
        //     socket: SocketAddrV4::new(Ipv4Addr::new(141, 84, 94, 207), 4020),
        // },
    ];

    let machines_serialized = bincode::serialize(&machines).unwrap();

    let mut cfg = match File::create(Path::new("./cfg/machines.cfg")) {
        Err(err) => panic!("{:?}", err),
        Ok(file) => file,
    };

    if let Err(e) = cfg.write(&machines_serialized) {
        panic!("Write failed: {}", e);
    };
}
