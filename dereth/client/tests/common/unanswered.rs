//! A loopback address no server answers on, for the stations that need the client's link present
//! with nobody at the other end.
//!
//! Behaviour: none (shared fixtures)

use std::net::UdpSocket;
use std::sync::OnceLock;

/// A loopback UDP port this process holds and never reads: a connected client's datagrams land
/// there and nothing answers them. A fixed port is not that. The server's default port is where a
/// server running on the same machine listens, and its answers (an unknown account, then a
/// disconnect that resets the world) reach the client in the middle of the test.
pub fn unanswered_port() -> u16 {
    static HELD: OnceLock<UdpSocket> = OnceLock::new();
    HELD.get_or_init(|| UdpSocket::bind("127.0.0.1:0").expect("a loopback port binds"))
        .local_addr()
        .expect("the held port's address")
        .port()
}
