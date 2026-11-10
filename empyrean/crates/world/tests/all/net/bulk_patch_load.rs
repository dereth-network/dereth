//! Divergence: V519
//! The patch stream's listener under load and abuse: it holds no more connections than its cap,
//! closing one past it before reading anything, so that client falls back to `0xF7E2` with its
//! token still good; it serves many clients at once without stalling, and every thread it starts
//! ends with its connection. A ticket opens once, only from its session's address, only with its
//! token; a connection that never says hello is closed.
//! Fixture: synthetic dats and loopback connections.

use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dereth_protocol::admin::{DddData, BULK_MAGIC, BULK_VERSION};
use empyrean_dat::{DatDatabaseType, DatManager, FakeDats};
use empyrean_world::managers::bulk_patch::{BulkItem, Limits, Service};

/// The most one wait may take: a guard against a hang, never the expected time.
const GUARD: Duration = Duration::from_secs(60);
const FIRST: u32 = 0x0600_0100;
const RECORDS: u32 = 40;
const LOOPBACK: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

fn dats() -> Arc<DatManager> {
    let mut fake = FakeDats::new();
    for n in 0..RECORDS {
        fake = fake.with_raw(
            DatDatabaseType::Portal,
            FIRST + n,
            vec![u8::try_from(n).expect("a byte"); 2000],
        );
    }
    fake.build().expect("fake dats")
}

fn items() -> Vec<BulkItem> {
    (0..RECORDS)
        .map(|n| BulkItem {
            id: FIRST + n,
            db: DatDatabaseType::Portal,
            compress: n % 2 == 0,
        })
        .collect()
}

fn service(streams: usize) -> Arc<Service> {
    Service::listen(
        SocketAddr::from(([127, 0, 0, 1], 0)),
        Limits {
            streams,
            workers: 2,
            hello: GUARD,
            ..Limits::default()
        },
    )
    .expect("the stream listens")
}

fn connect(s: &Service) -> TcpStream {
    let c = TcpStream::connect(("127.0.0.1", s.port())).expect("a connection");
    c.set_read_timeout(Some(GUARD)).expect("a timeout");
    c
}

fn hello(c: &mut TcpStream, token: &[u8; 16]) {
    let mut h = BULK_MAGIC.to_vec();
    h.extend_from_slice(&BULK_VERSION.to_le_bytes());
    h.extend_from_slice(token);
    // A server that closed the connection may refuse the write; what it reads says so.
    let _ = c.write_all(&h);
}

/// What a connection brought: every record and its end, or `None` when it was closed first.
fn read_all(c: &mut TcpStream) -> Option<Vec<u32>> {
    let mut ids = Vec::new();
    loop {
        let mut len = [0u8; 4];
        c.read_exact(&mut len).ok()?;
        let len = u32::from_le_bytes(len) as usize;
        if len == 0 {
            return Some(ids);
        }
        let mut body = vec![0u8; len];
        c.read_exact(&mut body).ok()?;
        ids.push(
            dereth_protocol::read_body::<DddData>(&body)
                .expect("a record")
                .resource_id,
        );
    }
}

/// Whether the server closed `c` without sending anything.
fn closed_unread(c: &mut TcpStream) -> bool {
    let mut b = [0u8; 1];
    match c.read(&mut b) {
        Ok(0) => true,
        Ok(_) => false,
        Err(e) => matches!(
            e.kind(),
            ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted
        ),
    }
}

/// Wait for `cond`, [`GUARD`] at most.
fn until(cond: impl Fn() -> bool) -> bool {
    let end = Instant::now() + GUARD;
    while Instant::now() < end {
        if cond() {
            return true;
        }
        std::thread::yield_now();
    }
    cond()
}

fn every_id() -> Vec<u32> {
    (FIRST..FIRST + RECORDS).collect()
}

fn sorted(mut v: Vec<u32>) -> Vec<u32> {
    v.sort_unstable();
    v
}

#[test]
fn a_connection_past_the_cap_is_closed_unread_and_its_token_still_opens_the_stream_later() {
    let s = service(2);
    let dats = dats();
    let token = s
        .offer(items(), Arc::clone(&dats), "late", LOOPBACK)
        .expect("a ticket");
    // Two connections that say nothing hold both places.
    let first = connect(&s);
    let second = connect(&s);
    assert!(until(|| s.held() == 2), "both held");
    let mut late = connect(&s);
    hello(&mut late, &token);
    assert!(closed_unread(&mut late), "the third is closed at once");
    assert!(until(|| s.turned_away() == 1));
    assert_eq!(s.held(), 2);

    // One place frees, and the same token opens the stream.
    drop(first);
    assert!(until(|| s.held() == 1), "the closed one is let go");
    let mut again = connect(&s);
    hello(&mut again, &token);
    assert_eq!(read_all(&mut again).map(sorted), Some(every_id()));
    drop((second, again));
    assert!(
        until(|| s.held() == 0 && s.threads() == 0),
        "nothing left running"
    );
    assert!(s.peak() <= 2);
}

#[test]
fn many_clients_at_once_stay_within_the_cap_and_every_thread_ends() {
    const CLIENTS: usize = 24;
    let s = service(4);
    let dats = dats();
    let tokens: Vec<[u8; 16]> = (0..CLIENTS)
        .map(|n| {
            s.offer(items(), Arc::clone(&dats), &format!("c{n}"), LOOPBACK)
                .expect("a ticket")
        })
        .collect();
    let port = s.port();
    let outcomes: Vec<Option<Vec<u32>>> = std::thread::scope(|scope| {
        let handles: Vec<_> = tokens
            .iter()
            .map(|token| {
                scope.spawn(move || {
                    let mut c = TcpStream::connect(("127.0.0.1", port)).expect("a connection");
                    c.set_read_timeout(Some(GUARD)).expect("a timeout");
                    hello(&mut c, token);
                    read_all(&mut c)
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("a client"))
            .collect()
    });
    let served = outcomes.iter().flatten().count();
    for o in outcomes.iter().flatten() {
        assert_eq!(
            sorted(o.clone()),
            every_id(),
            "a served client has every record"
        );
    }
    assert!(served >= 1, "some are served");
    assert_eq!(
        served + s.turned_away(),
        CLIENTS,
        "the rest were turned away, not cut"
    );
    assert!(s.peak() <= 4, "held at most the cap: {}", s.peak());
    assert!(
        until(|| s.held() == 0 && s.threads() == 0),
        "nothing left running"
    );

    // A client turned away still holds a good token: it is patched over UDP, or opens it again.
    let unserved: Vec<&[u8; 16]> = tokens
        .iter()
        .zip(&outcomes)
        .filter(|(_, o)| o.is_none())
        .map(|(t, _)| t)
        .collect();
    for token in unserved {
        assert!(s.withdraw(token), "never consumed");
    }
}

#[test]
fn a_ticket_opens_once_only_with_its_token_and_from_its_sessions_address() {
    let s = service(4);
    let dats = dats();
    let elsewhere = s
        .offer(
            items(),
            Arc::clone(&dats),
            "elsewhere",
            IpAddr::V4(Ipv4Addr::new(10, 0, 1, 1)),
        )
        .expect("a ticket");
    // Only a ticket for another address: a connection from here is closed before its hello.
    let mut c = connect(&s);
    hello(&mut c, &elsewhere);
    assert!(
        closed_unread(&mut c),
        "another address's ticket opens nothing here"
    );
    assert!(s.withdraw(&elsewhere), "and is left for its own client");

    let token = s
        .offer(items(), Arc::clone(&dats), "here", LOOPBACK)
        .expect("a ticket");
    let mut wrong = token;
    wrong[15] ^= 0x80;
    let mut c = connect(&s);
    hello(&mut c, &wrong);
    assert!(closed_unread(&mut c), "a wrong token opens nothing");
    let mut c = connect(&s);
    c.write_all(b"GET / HTTP/1.1\r\nHost: x\r\n")
        .expect("written");
    assert!(closed_unread(&mut c), "not a hello");

    let mut c = connect(&s);
    hello(&mut c, &token);
    assert_eq!(read_all(&mut c).map(sorted), Some(every_id()));
    let mut c = connect(&s);
    hello(&mut c, &token);
    assert!(closed_unread(&mut c), "a used token opens nothing");
    assert!(!s.withdraw(&token), "it is gone");
    assert!(until(|| s.held() == 0 && s.threads() == 0));
}

#[test]
fn a_connection_that_never_says_hello_is_closed() {
    let s = Service::listen(
        SocketAddr::from(([127, 0, 0, 1], 0)),
        Limits {
            streams: 1,
            // It never comes, so how long is waited does not change what happens.
            hello: Duration::from_millis(100),
            ..Limits::default()
        },
    )
    .expect("the stream listens");
    let _ticket = s.offer(items(), dats(), "quiet", LOOPBACK);
    let mut c = connect(&s);
    assert!(closed_unread(&mut c), "closed for its silence");
    assert!(
        until(|| s.held() == 0 && s.threads() == 0),
        "its place is free"
    );
}
