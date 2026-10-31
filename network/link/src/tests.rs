use std::io::{BufRead as _, BufReader};
use std::net::TcpListener;
use std::sync::Mutex;

use super::*;

const KEY: &str = "drth_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

/// A guard against a link that never does what a test waits for, not the time it is expected to
/// take.
const HANG: Duration = Duration::from_secs(60);

/// A clock the test drives: each reading moves it on by `step`, and each pause between attempts is
/// handed to the test, and lasts until the test lets it end.
struct Fake {
    start: Instant,
    passed: Mutex<Duration>,
    step: Duration,
    waits: Sender<Duration>,
    permits: Mutex<Receiver<()>>,
}

/// The test's end of a [`Fake`] clock: the pauses asked for, and the permits that end them.
struct Pauses {
    waits: Receiver<Duration>,
    permits: Sender<()>,
}

impl Pauses {
    /// The next pause the link asks for; it ends at once.
    fn next(&self) -> Duration {
        let d = self.waits.recv_timeout(HANG).expect("the link pauses");
        let _ = self.permits.send(());
        d
    }
}

fn fake(step: Duration) -> (Fake, Pauses) {
    let (waits_tx, waits) = mpsc::channel();
    let (permits, permits_rx) = mpsc::channel();
    (
        Fake {
            start: Instant::now(),
            passed: Mutex::new(Duration::ZERO),
            step,
            waits: waits_tx,
            permits: Mutex::new(permits_rx),
        },
        Pauses { waits, permits },
    )
}

impl Clock for Fake {
    fn now(&self) -> Instant {
        let mut p = self.passed.lock().expect("the clock");
        *p += self.step;
        self.start + *p
    }

    fn pause(&self, d: Duration, stop: &AtomicBool) {
        let _ = self.waits.send(d);
        let permits = self.permits.lock().expect("the permits");
        while !stop.load(Ordering::SeqCst) {
            match permits.recv_timeout(Duration::from_millis(10)) {
                Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => return,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    }
}

/// The next event of `link`, waited for.
fn next(link: &mut LinkClient) -> Event {
    let until = Instant::now() + HANG;
    loop {
        if let Some(e) = link.try_event() {
            return e;
        }
        assert!(Instant::now() < until, "no event came");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn loopback(port: u16) -> HubUrl {
    HubUrl::parse(&format!("ws://127.0.0.1:{port}")).expect("a loopback hub")
}

/// A port on loopback nothing listens on.
fn closed_port() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").expect("a port");
    l.local_addr().expect("an address").port()
}

/// Reads a request's head off `s`: its lines, the request line first.
fn head_of(s: &TcpStream) -> Vec<String> {
    let mut r = BufReader::new(s.try_clone().expect("the stream"));
    let mut lines = Vec::new();
    loop {
        let mut l = String::new();
        if r.read_line(&mut l).unwrap_or(0) == 0 {
            break;
        }
        let l = l.trim_end().to_owned();
        if l.is_empty() {
            break;
        }
        lines.push(l);
    }
    lines
}

/// Spec: link protocol: a world reconnects with growing waits, 1, 2, 5, 10, then every 30
/// seconds, and a minute after a refusal; a link that came up starts the waits again.
#[test]
fn the_reconnect_schedule_grows_to_thirty_seconds_and_starts_again_after_a_link() {
    let mut r = Reconnect::default();
    let secs = |d: Duration| d.as_secs();
    let failures: Vec<u64> = (0..7).map(|_| secs(r.after_failure())).collect();
    assert_eq!(failures, [1, 2, 5, 10, 30, 30, 30]);
    assert_eq!(r.after_refusal(), Duration::from_secs(60));
    assert_eq!(
        secs(r.after_failure()),
        30,
        "a refusal does not reset the steps"
    );
    assert_eq!(secs(r.after_link()), 1);
    assert_eq!(
        secs(r.after_failure()),
        1,
        "after a link, the first step again"
    );
    assert_eq!(secs(r.after_failure()), 2);
}

/// Spec: link protocol: a world that cannot reach its hub tries again after 1, 2, 5, 10, then
/// every 30 seconds.
#[test]
fn a_hub_that_cannot_be_reached_is_tried_after_growing_waits() {
    let (clock, pauses) = fake(Duration::ZERO);
    let mut link =
        LinkClient::start_with(&loopback(closed_port()), "aa", KEY, clock).expect("started");
    for want in [1, 2, 5, 10, 30, 30] {
        assert_eq!(pauses.next(), Duration::from_secs(want));
        match next(&mut link) {
            Event::Unreachable { retry_in, .. } => {
                assert_eq!(retry_in, Duration::from_secs(want));
            }
            other => panic!("{other:?}"),
        }
    }
    link.stop();
}

/// A stand-in hub on loopback: each connection is handed to `serve`, with its number from 0.
fn hub(serve: impl Fn(usize, TcpStream) + Send + 'static) -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").expect("a port");
    let port = l.local_addr().expect("an address").port();
    std::thread::spawn(move || {
        for (i, s) in l.incoming().enumerate() {
            let Ok(s) = s else { return };
            serve(i, s);
        }
    });
    port
}

/// Spec: link protocol: a world whose key (`401`) or whose federation flag (`403`) the hub
/// refuses waits a minute before it tries again.
#[test]
fn a_refused_key_or_world_waits_a_minute_before_the_next_attempt() {
    let port = hub(|i, mut s| {
        let _ = head_of(&s);
        let status = if i % 2 == 0 {
            "401 Unauthorized"
        } else {
            "403 Forbidden"
        };
        let _ = write!(
            s,
            "HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
    });
    let (clock, pauses) = fake(Duration::ZERO);
    let mut link = LinkClient::start_with(&loopback(port), "aa", KEY, clock).expect("started");
    for want in [401, 403, 401] {
        assert_eq!(pauses.next(), REFUSED_WAIT);
        assert_eq!(
            next(&mut link),
            Event::Refused {
                status: want,
                retry_in: REFUSED_WAIT
            }
        );
    }
    link.stop();
}

/// Upgrades `s` as the hub's link route does.
fn accept(s: TcpStream) -> tungstenite::WebSocket<TcpStream> {
    tungstenite::accept(s).expect("the upgrade")
}

/// Reads `ws` until the far side closes it.
fn drain(ws: &mut tungstenite::WebSocket<TcpStream>) {
    let _ = ws.get_mut().set_read_timeout(Some(HANG));
    while ws.read().is_ok() {}
}

/// Spec: link protocol: after a link that came up is lost, a world tries again after the first
/// wait, and its waits grow from there.
#[test]
fn after_a_link_is_lost_the_waits_start_again_from_the_first() {
    // Two connections dropped before the upgrade, one link that comes up and is closed, then
    // dropped ones again.
    let port = hub(|i, s| {
        if i == 2 {
            let mut ws = accept(s);
            let _ = ws.close(None);
            drain(&mut ws);
        } else {
            drop(s);
        }
    });
    let (clock, pauses) = fake(Duration::ZERO);
    let mut link = LinkClient::start_with(&loopback(port), "aa", KEY, clock).expect("started");
    let mut seen = Vec::new();
    for want in [1, 2, 1, 1, 2] {
        assert_eq!(pauses.next(), Duration::from_secs(want));
    }
    while seen.len() < 6 {
        seen.push(match next(&mut link) {
            Event::Unreachable { retry_in, .. } => format!("unreachable {}", retry_in.as_secs()),
            Event::Up => "up".into(),
            Event::Down(why) => format!("down: {why}"),
            other => format!("{other:?}"),
        });
    }
    assert_eq!(
        seen,
        [
            "unreachable 1",
            "unreachable 2",
            "up",
            "down: closed by the hub",
            "unreachable 1",
            "unreachable 2"
        ]
    );
    link.stop();
}

/// Spec: link protocol: a world opens its link with its key as a bearer token, frames go each way
/// as text, and nothing the world gave before its link came up is sent on it.
#[test]
// The upgrade's callback answers with tungstenite's own large refusal type.
#[allow(clippy::result_large_err)]
fn frames_go_each_way_with_the_key_and_one_for_a_gone_connection_is_dropped() {
    let (seen_tx, seen) = mpsc::channel();
    let port = hub(move |_, s| {
        let seen_tx = seen_tx.clone();
        std::thread::spawn(move || {
            let mut auth = None;
            let mut ws = tungstenite::accept_hdr(
                s,
                |req: &tungstenite::handshake::server::Request,
                 res: tungstenite::handshake::server::Response| {
                    auth = req
                        .headers()
                        .get("Authorization")
                        .and_then(|v| v.to_str().ok())
                        .map(str::to_owned);
                    let _ = seen_tx.send(format!("path {}", req.uri().path()));
                    Ok(res)
                },
            )
            .expect("the upgrade");
            let _ = seen_tx.send(format!("auth {}", auth.unwrap_or_default()));
            ws.send(tungstenite::Message::Text("from the hub".into()))
                .expect("sent");
            let _ = ws.get_mut().set_read_timeout(Some(HANG));
            while let Ok(m) = ws.read() {
                if let tungstenite::Message::Text(t) = m {
                    let _ = seen_tx.send(format!("frame {t}"));
                }
            }
        });
    });
    let (clock, _pauses) = fake(Duration::ZERO);
    let mut link = LinkClient::start_with(&loopback(port), "aa", KEY, clock).expect("started");
    link.send("before the link".into());
    assert_eq!(next(&mut link), Event::Up);
    link.send("hello".into());
    assert_eq!(next(&mut link), Event::Frame("from the hub".into()));
    let got: Vec<String> = (0..3)
        .map(|_| seen.recv_timeout(HANG).expect("the hub saw it"))
        .collect();
    assert_eq!(
        got,
        [
            "path /v1/servers/aa/link".to_owned(),
            format!("auth Bearer {KEY}"),
            "frame hello".to_owned(),
        ]
    );
    link.stop();
}

/// Spec: link protocol: a world gives up a link its hub has said nothing on for 70 seconds.
#[test]
fn a_hub_that_falls_silent_loses_the_link() {
    let port = hub(|_, s| {
        std::thread::spawn(move || {
            let mut ws = accept(s);
            drain(&mut ws);
        });
    });
    // Each reading of the clock is a second later: the hub's silence passes 70 seconds within
    // a hundred or so turns of the link's loop.
    let (clock, _pauses) = fake(Duration::from_secs(1));
    let mut link = LinkClient::start_with(&loopback(port), "aa", KEY, clock).expect("started");
    assert_eq!(next(&mut link), Event::Up);
    assert_eq!(next(&mut link), Event::Down("the hub fell silent".into()));
    link.stop();
}

/// Spec: link protocol: a frame longer than the protocol allows loses the link.
#[test]
fn a_frame_longer_than_the_protocol_allows_loses_the_link() {
    let port = hub(|_, s| {
        std::thread::spawn(move || {
            let mut ws = accept(s);
            let long = "x".repeat(dereth_network_protocol::wire::MAX_MESSAGE_BYTES + 1);
            let _ = ws.send(tungstenite::Message::Text(long.into()));
            drain(&mut ws);
        });
    });
    let (clock, _pauses) = fake(Duration::ZERO);
    let mut link = LinkClient::start_with(&loopback(port), "aa", KEY, clock).expect("started");
    assert_eq!(next(&mut link), Event::Up);
    match next(&mut link) {
        Event::Down(why) => assert!(why.contains("too long") || why.contains("size"), "{why}"),
        other => panic!("{other:?}"),
    }
    link.stop();
}

/// Spec: link protocol: a world drops a hub that sends faster than it takes the frames, and links
/// again.
#[test]
fn a_hub_sending_faster_than_the_world_takes_its_frames_loses_the_link() {
    let port = hub(|i, s| {
        if i > 0 {
            return;
        }
        std::thread::spawn(move || {
            let mut ws = accept(s);
            // The world takes nothing, so it sends nothing: the hub just sends.
            for _ in 0..(MAX_WAITING_FRAMES + 5_000) {
                if ws
                    .send(tungstenite::Message::Text(
                        r#"{"type":"a.later.type"}"#.into(),
                    ))
                    .is_err()
                {
                    break;
                }
            }
            drain(&mut ws);
        });
    });
    let (clock, _pauses) = fake(Duration::ZERO);
    let link = LinkClient::start_with(&loopback(port), "aa", KEY, clock).expect("started");
    // Read the channel as the link thread fills it, without taking the frames as the world does.
    let mut frames = 0usize;
    let down = loop {
        match link.events.recv_timeout(HANG).expect("an event") {
            (_, Event::Frame(_)) => frames += 1,
            (_, Event::Down(why)) => break why,
            _ => {}
        }
    };
    assert_eq!(frames, MAX_WAITING_FRAMES, "no more wait than the bound");
    assert_eq!(
        down,
        "the hub sends faster than this world takes its frames"
    );
    link.stop();
}

/// Spec: link protocol: a world's host takes the events a link hands it and sends on the
/// connection it last heard come up.
#[test]
fn a_detached_link_hands_its_events_and_keeps_what_was_sent_on_the_connection_up() {
    let (mut link, mut far) = LinkClient::detached();
    far.hand(Event::Up);
    far.hand(Event::Frame("one".into()));
    assert_eq!(far.waiting(), 1);
    assert_eq!(link.try_event(), Some(Event::Up));
    link.send("hello".into());
    assert_eq!(far.sent(), ["hello"]);
    assert_eq!(link.try_event(), Some(Event::Frame("one".into())));
    assert_eq!(far.waiting(), 0, "a frame taken no longer waits");
    far.hand(Event::Down("gone".into()));
    far.hand(Event::Up);
    link.send("for the old connection".into());
    assert!(
        far.sent().is_empty(),
        "the world has not yet heard the new one"
    );
    assert_eq!(link.try_event(), Some(Event::Down("gone".into())));
    assert_eq!(link.try_event(), Some(Event::Up));
    link.send("hello again".into());
    assert_eq!(far.sent(), ["hello again"]);
    assert_eq!(link.try_event(), None);
}

/// Spec: link protocol: a hub is reached over `wss://`, and plain `ws://` only on loopback.
#[test]
fn a_hub_address_is_wss_or_loopback_ws() {
    let u = HubUrl::parse("wss://hub.example.org").expect("wss");
    assert_eq!(
        (u.tls, u.host.as_str(), u.port),
        (true, "hub.example.org", 443)
    );
    assert_eq!(
        u.link("coldeve"),
        "wss://hub.example.org:443/v1/servers/coldeve/link"
    );
    let l = HubUrl::parse("ws://127.0.0.1:8650/").expect("loopback ws");
    assert_eq!(l.link("aa"), "ws://127.0.0.1:8650/v1/servers/aa/link");
    let p = HubUrl::parse("wss://example.org:9443/net").expect("a prefix");
    assert_eq!(
        p.link("aa"),
        "wss://example.org:9443/net/v1/servers/aa/link"
    );
    assert_eq!(
        p.verify("aa"),
        "https://example.org:9443/net/v1/servers/aa/verify"
    );
    assert_eq!(
        p.answer("aa"),
        "https://example.org:9443/net/v1/servers/aa/verify/answer"
    );
    assert_eq!(l.verify("aa"), "http://127.0.0.1:8650/v1/servers/aa/verify");
    let v6 = HubUrl::parse("ws://[::1]:8650").expect("loopback v6");
    assert_eq!(v6.link("aa"), "ws://[::1]:8650/v1/servers/aa/link");
    assert!(
        HubUrl::parse("ws://hub.example.org").is_err(),
        "plain ws only on loopback"
    );
    assert!(HubUrl::parse("https://hub.example.org").is_err());
    assert!(HubUrl::parse("wss://user@hub.example.org").is_err());
    assert!(HubUrl::parse("wss://hub.example.org:port").is_err());
}

/// Spec: hub: a key is `drth_` and 43 characters of base64url.
#[test]
fn a_key_is_drth_and_43_characters_of_base64url() {
    assert!(is_key(KEY));
    assert!(is_key(&format!("drth_{}", "a-_9".repeat(10) + "abc")));
    assert!(!is_key("drth_short"));
    assert!(!is_key(&format!("drth_{}", "A".repeat(42) + "=")));
    assert!(!is_key(&format!("DRTH_{}", "A".repeat(43))));
}
