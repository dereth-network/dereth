//! What the tests share: a scratch folder, stand-in servers and clients on loopback, and a wait
//! for a datagram whose timeout only guards against a hang.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use crate::record::any_adjacent_pair;

/// Only guards against a hang.
pub const HANG: Duration = Duration::from_secs(20);

/// A folder of its own in the system's temporary folder, removed when the test ends.
pub struct Scratch(PathBuf);

impl Scratch {
    pub fn new() -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "dereth-recorder-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&dir).expect("a scratch folder");
        Self(dir)
    }

    pub fn file(&self) -> PathBuf {
        self.0.join("session.pcap")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub fn v4(a: SocketAddr) -> SocketAddrV4 {
    match a {
        SocketAddr::V4(a) => a,
        SocketAddr::V6(a) => panic!("an IPv6 address {a}"),
    }
}

/// A stand-in server: two sockets on adjacent loopback ports.
pub fn server_pair() -> [UdpSocket; 2] {
    let pair = any_adjacent_pair(Ipv4Addr::LOCALHOST).expect("two adjacent ports");
    for s in &pair {
        s.set_read_timeout(Some(HANG)).expect("a hang guard");
    }
    pair
}

pub fn first_port(pair: &[UdpSocket; 2]) -> SocketAddrV4 {
    v4(pair[0].local_addr().expect("bound"))
}

pub fn client() -> UdpSocket {
    let c = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("a client socket");
    c.set_read_timeout(Some(HANG)).expect("a hang guard");
    c
}

pub fn receive(s: &UdpSocket) -> (Vec<u8>, SocketAddrV4) {
    let mut buf = vec![0u8; 65_536];
    let (n, from) = s
        .recv_from(&mut buf)
        .expect("a datagram before the hang guard");
    buf.truncate(n);
    (buf, v4(from))
}

/// Which way a datagram goes, and to or from which of the server's two ports.
#[derive(Debug, Clone, Copy)]
pub enum Way {
    ToServer(usize),
    ToClient(usize),
}

/// Play `datagrams` through a recorder between a stand-in client and a stand-in server, each sent
/// once the one before it has arrived, into the scratch folder's file. The first goes to the
/// server.
pub fn play(scratch: &Scratch, datagrams: &[(Way, Vec<u8>)]) {
    let servers = server_pair();
    let recorder = crate::record::Recorder::start(&crate::record::Options {
        listen: SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        server: first_port(&servers),
        out: scratch.file(),
    })
    .expect("the recorder starts");
    let listen = recorder.listen();
    let c = client();
    let mut upstream = None;
    for (way, bytes) in datagrams {
        match *way {
            Way::ToServer(port) => {
                let to = SocketAddrV4::new(
                    *listen.ip(),
                    listen.port() + u16::try_from(port).expect("0 or 1"),
                );
                c.send_to(bytes, to).expect("sent");
                let (got, from) = receive(&servers[port]);
                assert_eq!(&got, bytes);
                upstream = Some(from);
            }
            Way::ToClient(port) => {
                let to = upstream.expect("the client has sent first");
                servers[port].send_to(bytes, to).expect("sent");
                assert_eq!(&receive(&c).0, bytes);
            }
        }
    }
    recorder.stop().expect("the file is finished");
}
