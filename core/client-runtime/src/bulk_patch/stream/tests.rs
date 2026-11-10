//! The patch stream against a stand-in server on the loopback address: every way a stream can
//! end, each of which the frame loop turns into a decline so the patch completes the old way.

use super::*;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::time::Duration;

/// The most one wait may take: a guard against a hang, never the expected time.
const GUARD: Duration = Duration::from_secs(60);
/// The token every stand-in expects.
const TOKEN: [u8; 16] = [9; 16];

fn record(id: u32, n: usize) -> DddData {
    DddData {
        dat_file_type: 0,
        dat_file_id: 1,
        resource_type: 6,
        resource_id: id,
        iteration: 2073,
        compressed: 0,
        version: 3,
        data_size: u32::try_from(n + 4).unwrap(),
        data: vec![u8::try_from(id % 251).unwrap(); n],
    }
}

/// A record behind its length, as the stream frames it.
fn framed(d: &DddData) -> Vec<u8> {
    let body = dereth_protocol::write_body(d).unwrap();
    let mut out = u32::try_from(body.len()).unwrap().to_le_bytes().to_vec();
    out.extend_from_slice(&body);
    out
}

/// A stand-in that takes one connection, reads its hello, and hands the connection to `then`;
/// the hello it read comes back on the channel.
fn serve(then: impl FnOnce(TcpStream) + Send + 'static) -> (SocketAddr, mpsc::Receiver<[u8; 24]>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let mut hello = [0u8; 24];
        if s.read_exact(&mut hello).is_ok() {
            let _ = tx.send(hello);
            then(s);
        }
    });
    (addr, rx)
}

fn open(addr: SocketAddr, limits: Limits) -> BulkDownload {
    BulkDownload::start_with(
        move || TcpStream::connect_timeout(&addr, limits.connect),
        TOKEN,
        limits,
    )
    .unwrap()
}

/// Every record the stream brings, waiting for each, and how it ended.
fn all(d: &mut BulkDownload) -> (Vec<u32>, Result<(), String>) {
    let mut ids = Vec::new();
    while d.ended().is_none() {
        assert!(
            d.wait(GUARD),
            "the stream neither brought a record nor ended"
        );
        ids.extend(d.drain(usize::MAX).iter().map(|r| r.resource_id));
    }
    (ids, d.ended().cloned().unwrap())
}

/// Limits for a stand-in that stalls: a record must come within a moment. The stand-in never
/// sends it, so the outcome does not depend on how fast anything runs.
fn impatient() -> Limits {
    Limits {
        record: Duration::from_millis(200),
        ..Limits::default()
    }
}

/// Behaviour: net.dat-patch.a-bulk-stream-brings-every-record-and-tells-its-end-from-a-break
#[test]
fn a_stream_brings_every_record_in_order_after_its_hello_then_ends() {
    let (addr, hello) = serve(|mut s| {
        for id in 1..=600 {
            s.write_all(&framed(&record(id, 3))).unwrap();
        }
        s.write_all(&0u32.to_le_bytes()).unwrap();
    });
    let mut d = open(addr, Limits::default());
    assert_eq!(all(&mut d), ((1..=600).collect(), Ok(())));
    assert_eq!(d.progress().0, 600);
    let hello = hello.recv().unwrap();
    assert_eq!(&hello[..4], b"DBLK");
    assert_eq!(&hello[4..8], &1u32.to_le_bytes());
    assert_eq!(&hello[8..], &TOKEN);
}

/// Behaviour: net.dat-patch.every-way-a-bulk-stream-fails-is-a-failure-the-patch-falls-back-from
#[test]
fn every_way_a_stream_can_fail_ends_it_as_failed_with_what_came_before() {
    type Then = Box<dyn FnOnce(TcpStream) + Send>;
    let cases: Vec<(&str, Then, Limits, Vec<u32>)> = vec![
        (
            "cut part way",
            Box::new(|mut s| {
                s.write_all(&framed(&record(7, 3))).unwrap();
                s.write_all(&framed(&record(8, 3))).unwrap();
                // Half of a third record, then gone.
                let third = framed(&record(9, 300));
                s.write_all(&third[..100]).unwrap();
            }),
            Limits::default(),
            vec![7, 8],
        ),
        (
            "closed before its end",
            Box::new(|mut s| s.write_all(&framed(&record(7, 3))).unwrap()),
            Limits::default(),
            vec![7],
        ),
        (
            "closed at once (a wrong or used token, or a server past its cap)",
            Box::new(drop),
            Limits::default(),
            vec![],
        ),
        (
            "a record larger than the client takes",
            Box::new(|mut s| {
                s.write_all(&framed(&record(7, 3))).unwrap();
                s.write_all(&framed(&record(8, 5000))).unwrap();
            }),
            Limits {
                largest: 1000,
                ..Limits::default()
            },
            vec![7],
        ),
        (
            "a length its body does not fill",
            Box::new(|mut s| {
                // A length of 3 is too short for any record's fields.
                s.write_all(&3u32.to_le_bytes()).unwrap();
                s.write_all(&[1, 2, 3]).unwrap();
                s.write_all(&0u32.to_le_bytes()).unwrap();
            }),
            Limits::default(),
            vec![],
        ),
        (
            "stalled, sending nothing",
            Box::new(|s| {
                let mut s = s;
                // Hold the connection open until the client gives up on it.
                let _ = s.read(&mut [0u8; 1]);
            }),
            impatient(),
            vec![],
        ),
        (
            "trickling: a record's first bytes, then nothing",
            Box::new(|mut s| {
                s.write_all(&framed(&record(7, 3))).unwrap();
                s.write_all(&framed(&record(8, 3))[..2]).unwrap();
                let _ = s.read(&mut [0u8; 1]);
            }),
            impatient(),
            vec![7],
        ),
    ];
    for (name, then, limits, came) in cases {
        let (addr, _hello) = serve(then);
        let mut d = open(addr, limits);
        let (ids, end) = all(&mut d);
        assert_eq!(ids, came, "{name}");
        assert!(end.is_err(), "{name}: {end:?}");
    }
}

/// Behaviour: net.dat-patch.a-bulk-stream-nobody-answers-fails-so-the-patch-falls-back
#[test]
fn a_stream_refused_or_blocked_fails_at_once() {
    // Refused: nothing listens on the port.
    let addr = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let mut d = open(addr, Limits::default());
    let (ids, end) = all(&mut d);
    assert!(ids.is_empty() && end.is_err(), "refused: {end:?}");

    // Blocked by a firewall: the connection never opens, and the attempt times out.
    let mut d = BulkDownload::start_with(
        || Err(std::io::Error::from(ErrorKind::TimedOut)),
        TOKEN,
        Limits::default(),
    )
    .unwrap();
    let (ids, end) = all(&mut d);
    assert!(ids.is_empty() && end.is_err(), "blocked: {end:?}");
}

/// Behaviour: net.dat-patch.a-bulk-stream-holds-a-bounded-number-of-bytes-for-the-frame-loop
#[test]
fn a_stream_holds_no_more_bytes_than_its_bound_until_the_frame_loop_drains_them() {
    let (addr, _hello) = serve(|mut s| {
        for id in 1..=20 {
            s.write_all(&framed(&record(id, 1000))).unwrap();
        }
        s.write_all(&0u32.to_le_bytes()).unwrap();
    });
    let mut d = open(
        addr,
        Limits {
            queued: 2500,
            ..Limits::default()
        },
    );
    let mut ids = Vec::new();
    while d.ended().is_none() {
        assert!(d.wait(GUARD));
        assert!(d.queued() <= 2500, "{} bytes held", d.queued());
        // A frame takes one record at a time.
        ids.extend(d.drain(1).iter().map(|r| r.resource_id));
    }
    assert_eq!(ids, (1..=20).collect::<Vec<_>>());
    assert_eq!(d.ended(), Some(&Ok(())));
}

/// Behaviour: net.dat-patch.a-bulk-stream-the-client-stops-reading-closes
#[test]
fn a_stream_dropped_by_the_client_stops_its_reader_and_closes_the_connection() {
    let (closed_tx, closed) = mpsc::channel();
    let (addr, _hello) = serve(move |mut s| {
        // Records until the client's side goes away.
        let mut id = 0;
        let r = loop {
            id += 1;
            if let Err(e) = s.write_all(&framed(&record(id, 4000))) {
                break e;
            }
        };
        let _ = closed_tx.send(r.kind());
    });
    let mut d = open(
        addr,
        Limits {
            queued: 8000,
            ..Limits::default()
        },
    );
    assert!(d.wait(GUARD));
    drop(d);
    let kind = closed
        .recv_timeout(GUARD)
        .expect("the server's writes fail");
    assert!(
        matches!(
            kind,
            ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted | ErrorKind::BrokenPipe
        ),
        "{kind:?}"
    );
}
