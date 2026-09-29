//! Vectors: loopback UDP login and quiet-period delivery cases in this module
//! A client logs in over loopback UDP without waiting; listeners lose no datagram after a quiet
//! spell.
//! Fixture: loopback UDP sockets and the shared client transport.

// V268, V282.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use empyrean_common::clock::SystemClock;
use empyrean_net::driver::udp::UdpDriver;
use empyrean_net::testing::{ClientStatus, TestAccounts, TestClient};
use empyrean_net::{ClockSnapshot, Event, NetConfig, PortKind, ServerNet};

/// A client socket read by its own thread, blocked with no timeout, as the driver reads its own.
struct ClientSocket {
    socket: Arc<UdpSocket>,
    rx: Receiver<(SocketAddr, Vec<u8>)>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ClientSocket {
    fn bind(ip: IpAddr) -> Self {
        let socket = Arc::new(UdpSocket::bind(SocketAddr::new(ip, 0)).expect("client socket"));
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let (s, st) = (Arc::clone(&socket), Arc::clone(&stop));
        let thread = std::thread::spawn(move || {
            let mut buf = [0u8; 2048];
            loop {
                let r = s.recv_from(&mut buf);
                if st.load(Ordering::SeqCst) {
                    return;
                }
                // Errors (an ICMP-caused reset on Windows) are skipped, as the driver skips them.
                if let Ok((n, from)) = r {
                    if tx.send((from, buf[..n].to_vec())).is_err() {
                        return;
                    }
                }
            }
        });
        Self {
            socket,
            rx,
            stop,
            thread: Some(thread),
        }
    }

    fn addr(&self) -> SocketAddr {
        self.socket.local_addr().expect("addr")
    }
}

impl Drop for ClientSocket {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = self.socket.send_to(&[], self.addr());
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[test]
fn a_client_logs_in_over_real_sockets_without_waiting() {
    let loopback = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let driver = UdpDriver::bind(loopback, 0).expect("bind P and P + 1");
    let server_addr = driver.local_addr(PortKind::C2S).expect("addr");
    let s2c = driver.local_addr(PortKind::S2C).expect("addr");
    assert_eq!(s2c.port(), server_addr.port() + 1);

    let start = Instant::now();
    let system = SystemClock::new();
    let clock = move || ClockSnapshot::take(&system, start.elapsed().as_secs_f64());
    let stop = Arc::new(AtomicBool::new(false));
    let stop_server = Arc::clone(&stop);
    let server = std::thread::spawn(move || {
        let mut net = ServerNet::new(
            NetConfig {
                port: server_addr.port(),
                ..NetConfig::default()
            },
            crate::common::messages(),
        );
        let mut accounts = TestAccounts::default();
        while !stop_server.load(Ordering::SeqCst) {
            driver.pump(&mut net, Duration::from_millis(1), &clock);
            let events: Vec<Event> = net.events().collect();
            for e in events {
                if let Event::LoginRequest {
                    session, request, ..
                } = e
                {
                    accounts.answer(&mut net, session, &request, clock());
                }
            }
        }
        driver.shutdown();
    });

    let socket = ClientSocket::bind(loopback);
    let mut client = TestClient::new(socket.addr(), server_addr, "sockets", "pw");
    let t0 = Instant::now();
    while client.status() != ClientStatus::Connected && t0.elapsed() < Duration::from_secs(5) {
        client.tick(start.elapsed().as_secs_f64());
        for (to, bytes) in client.take_outgoing() {
            socket.socket.send_to(&bytes, to).expect("send");
        }
        let first = socket.rx.recv_timeout(Duration::from_millis(1)).ok();
        for (from, bytes) in first
            .into_iter()
            .chain(std::iter::from_fn(|| socket.rx.try_recv().ok()))
        {
            client.handle_datagram(from, &bytes, start.elapsed().as_secs_f64());
        }
    }
    let took = t0.elapsed();
    stop.store(true, Ordering::SeqCst);
    server.join().expect("server thread");

    eprintln!("handshake over loopback: {took:?}");
    assert_eq!(client.status(), ClientStatus::Connected);
    assert_eq!(
        client.login_requests_sent, 1,
        "the first LoginRequest was answered"
    );
    assert_eq!(
        client.connect_responses_sent, 1,
        "the first ConnectResponse on P + 1 was answered"
    );
    assert!(took < Duration::from_millis(300), "handshake took {took:?}");
}

/// The listeners lose no datagram after a quiet spell.
#[test]
fn the_listeners_lose_no_datagram_after_a_quiet_spell() {
    const COUNT: u32 = 40;
    let loopback = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let driver = UdpDriver::bind(loopback, 0).expect("bind P and P + 1");
    let ports = [
        driver.local_addr(PortKind::C2S).expect("addr"),
        driver.local_addr(PortKind::S2C).expect("addr"),
    ];
    let sender = UdpSocket::bind(SocketAddr::new(loopback, 0)).expect("sender");
    let (_keep, tick) = mpsc::channel::<()>();
    let mut received = [0u32; 2];
    let mut count = |port_kind: PortKind| received[usize::from(port_kind == PortKind::S2C)] += 1;
    for i in 0..COUNT {
        let _ = tick.recv_timeout(Duration::from_millis(101));
        sender
            .send_to(&i.to_le_bytes(), ports[usize::from(i % 2 == 1)])
            .expect("send");
        while let Some(d) = driver.try_recv() {
            count(d.port_kind);
        }
    }
    let end = Instant::now() + Duration::from_secs(1);
    while Instant::now() < end {
        match driver.recv_timeout(Duration::from_millis(10)) {
            Some(d) => count(d.port_kind),
            None => break,
        }
    }
    let started = Instant::now();
    driver.shutdown();
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "shutdown woke the blocked listeners"
    );
    assert_eq!(
        received,
        [COUNT / 2, COUNT / 2],
        "datagrams received on P and P + 1"
    );
}
