//! The hub checks: what any hub must do, run against a real one over its WebSocket and, for the
//! directory, its public HTTP routes at the same address.
//!
//! The suite plays two or three worlds. It needs the hub's address and, for each world, a slug
//! and key the hub's registry accepts with the federation flag on; optionally one more world whose
//! flag is not on. Each check opens its own links and names its own characters and channel, so the
//! checks can run in any order against a hub that is also carrying other traffic.

mod directory;
mod http;

use std::net::TcpStream;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dereth_network_protocol::person::is_link_code;
use dereth_network_protocol::wire::PROTOCOL;
use dereth_network_protocol::{
    Address, ChannelOp, ChannelOutcome, DirectoryMessage, ListingOutcome, Message, Outcome,
    PersonOp, PersonOutcome, Profile, ReachMessage, RoomsMessage, SayMessage,
};
use tungstenite::client::IntoClientRequest as _;
use tungstenite::WebSocket;

use crate::Check;

/// What the suite is run against.
#[derive(Debug, Clone)]
pub struct HubTarget {
    /// `ws://host:port`.
    pub base: String,
    /// Federated worlds: `(slug, key)`. Two are needed; a third adds the room checks that need a
    /// world that does not carry the room.
    pub worlds: Vec<(String, String)>,
    /// A world whose federation flag is not on, with its valid key.
    pub off: Option<(String, String)>,
    /// Run the checks that wait out the hub's timers (about half a minute).
    pub slow: bool,
    /// The hub's clock, when the suite may let its time pass: the directory keeps the hour a
    /// profile last changed, which only hours passing show. Without it those checks hold what one
    /// hour shows.
    pub clock: Option<HubClock>,
}

/// Lets a hub's time pass, as a hub's own tests can: the hour of the directory's listings, by the
/// hub's clock, moves on by the time given.
#[derive(Clone)]
pub struct HubClock(Arc<dyn Fn(Duration) + Send + Sync>);

impl HubClock {
    /// A clock that `advance` moves on.
    pub fn new(advance: impl Fn(Duration) + Send + Sync + 'static) -> Self {
        Self(Arc::new(advance))
    }

    /// Lets `by` pass on the hub's clock.
    pub fn advance(&self, by: Duration) {
        (self.0)(by);
    }
}

impl std::fmt::Debug for HubClock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HubClock")
    }
}

/// How opening a link went.
#[derive(Debug)]
pub enum Open {
    /// The hub answered the upgrade with this status.
    Refused(u16),
    Failed(String),
}

/// One frame a link carried: a link message, or one of the directory's.
#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    Link(Message),
    Directory(DirectoryMessage),
}

/// One link, as a world holds it.
pub struct Link {
    ws: WebSocket<TcpStream>,
    pub slug: String,
    /// The reach messages read while looking for something else, oldest first, for
    /// [`Link::expect_reach`].
    reach: std::collections::VecDeque<ReachMessage>,
    /// The say messages read while looking for something else, oldest first, for
    /// [`Link::expect_say`].
    says: std::collections::VecDeque<SayMessage>,
    /// The rooms messages read while looking for something else, oldest first, for
    /// [`Link::expect_rooms`].
    rooms: std::collections::VecDeque<RoomsMessage>,
}

impl std::fmt::Debug for Link {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Link")
            .field("slug", &self.slug)
            .finish_non_exhaustive()
    }
}

impl Link {
    /// Opens `base`'s link for `slug` with `key`.
    ///
    /// # Errors
    /// [`Open::Refused`] for a status answer, [`Open::Failed`] for anything else.
    pub fn open(base: &str, slug: &str, key: &str) -> Result<Self, Open> {
        let url = format!("{}/v1/servers/{slug}/link", base.trim_end_matches('/'));
        let rest = url
            .strip_prefix("ws://")
            .ok_or_else(|| Open::Failed(format!("{url}: the suite speaks ws:// only")))?;
        let authority = rest.split('/').next().unwrap_or("");
        let tcp = TcpStream::connect(authority).map_err(|e| Open::Failed(e.to_string()))?;
        tcp.set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|e| Open::Failed(e.to_string()))?;
        let mut request = url
            .as_str()
            .into_client_request()
            .map_err(|e| Open::Failed(e.to_string()))?;
        request.headers_mut().insert(
            "Authorization",
            format!("Bearer {key}")
                .parse()
                .map_err(|_| Open::Failed("a key that is no header".into()))?,
        );
        match tungstenite::client::client(request, tcp) {
            Ok((ws, _)) => {
                let _ = ws
                    .get_ref()
                    .set_read_timeout(Some(Duration::from_millis(50)));
                Ok(Self {
                    ws,
                    slug: slug.to_owned(),
                    reach: std::collections::VecDeque::new(),
                    says: std::collections::VecDeque::new(),
                    rooms: std::collections::VecDeque::new(),
                })
            }
            Err(tungstenite::HandshakeError::Failure(tungstenite::Error::Http(r))) => {
                Err(Open::Refused(r.status().as_u16()))
            }
            Err(e) => Err(Open::Failed(e.to_string())),
        }
    }

    /// Sends a message.
    ///
    /// # Errors
    /// When the link has failed.
    pub fn send(&mut self, m: &Message) -> Result<(), String> {
        self.send_raw(&m.to_json())
    }

    /// Sends a frame as it is.
    ///
    /// # Errors
    /// When the link has failed.
    pub fn send_raw(&mut self, text: &str) -> Result<(), String> {
        self.ws
            .send(tungstenite::Message::Text(text.to_owned().into()))
            .map_err(|e| e.to_string())
    }

    /// Sends a binary frame, which no side of the protocol may send.
    ///
    /// # Errors
    /// When the link has failed.
    pub fn send_binary(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.ws
            .send(tungstenite::Message::Binary(bytes.to_vec().into()))
            .map_err(|e| e.to_string())
    }

    /// The next message within `within`; `Ok(None)` when none came, `Err` when the link closed.
    /// The directory's messages are let by.
    ///
    /// # Errors
    /// When the link closed or failed.
    pub fn recv(&mut self, within: Duration) -> Result<Option<Message>, String> {
        let until = Instant::now() + within;
        loop {
            let left = until.saturating_duration_since(Instant::now());
            match self.recv_frame(left)? {
                Some(Frame::Link(m)) => return Ok(Some(m)),
                Some(Frame::Directory(_)) => {}
                None => return Ok(None),
            }
        }
    }

    /// Reads until a directory message `pick` takes, within `within` (link messages are let by).
    ///
    /// # Errors
    /// When none came, or the link closed first.
    pub fn expect_directory<T>(
        &mut self,
        within: Duration,
        what: &str,
        mut pick: impl FnMut(&DirectoryMessage) -> Option<T>,
    ) -> Result<T, String> {
        let until = Instant::now() + within;
        loop {
            let left = until.saturating_duration_since(Instant::now());
            match self.recv_frame(left)? {
                Some(Frame::Directory(m)) => {
                    if let Some(t) = pick(&m) {
                        return Ok(t);
                    }
                }
                Some(Frame::Link(_)) => {}
                None => return Err(format!("{}: no {what} within {within:?}", self.slug)),
            }
        }
    }

    /// Reads until a reach message `pick` takes, within `within` (other frames are let by), the
    /// reach messages read while looking for other frames first.
    ///
    /// # Errors
    /// When none came, or the link closed first.
    pub fn expect_reach<T>(
        &mut self,
        within: Duration,
        what: &str,
        mut pick: impl FnMut(&ReachMessage) -> Option<T>,
    ) -> Result<T, String> {
        let until = Instant::now() + within;
        loop {
            while let Some(m) = self.reach.pop_front() {
                if let Some(t) = pick(&m) {
                    return Ok(t);
                }
            }
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(format!("{}: no {what} within {within:?}", self.slug));
            }
            self.recv_frame(left.min(Duration::from_millis(50)))?;
        }
    }

    /// The reach messages read so far and not yet taken, oldest first; none are kept after.
    pub fn take_reach(&mut self) -> Vec<ReachMessage> {
        self.reach.drain(..).collect()
    }

    /// Reads until a say message `pick` takes, within `within` (other frames are let by), the say
    /// messages read while looking for other frames first.
    ///
    /// # Errors
    /// When none came, or the link closed first.
    pub fn expect_say<T>(
        &mut self,
        within: Duration,
        what: &str,
        mut pick: impl FnMut(&SayMessage) -> Option<T>,
    ) -> Result<T, String> {
        let until = Instant::now() + within;
        loop {
            while let Some(m) = self.says.pop_front() {
                if let Some(t) = pick(&m) {
                    return Ok(t);
                }
            }
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(format!("{}: no {what} within {within:?}", self.slug));
            }
            self.recv_frame(left.min(Duration::from_millis(50)))?;
        }
    }

    /// The say messages read so far and not yet taken, oldest first; none are kept after.
    pub fn take_say(&mut self) -> Vec<SayMessage> {
        self.says.drain(..).collect()
    }

    /// Reads until a rooms message `pick` takes, within `within` (other frames are let by), the
    /// rooms messages read while looking for other frames first.
    ///
    /// # Errors
    /// When none came, or the link closed first.
    pub fn expect_rooms<T>(
        &mut self,
        within: Duration,
        what: &str,
        mut pick: impl FnMut(&RoomsMessage) -> Option<T>,
    ) -> Result<T, String> {
        let until = Instant::now() + within;
        loop {
            while let Some(m) = self.rooms.pop_front() {
                if let Some(t) = pick(&m) {
                    return Ok(t);
                }
            }
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(format!("{}: no {what} within {within:?}", self.slug));
            }
            self.recv_frame(left.min(Duration::from_millis(50)))?;
        }
    }

    /// The rooms messages read so far and not yet taken, oldest first; none are kept after.
    pub fn take_rooms(&mut self) -> Vec<RoomsMessage> {
        self.rooms.drain(..).collect()
    }

    /// The next frame of any kind within `within`; `Ok(None)` when none came. A reach message is
    /// kept for [`Link::expect_reach`], a say message for [`Link::expect_say`], and a rooms
    /// message for [`Link::expect_rooms`], at most 1,000 of each, the oldest let go past that.
    ///
    /// # Errors
    /// When the link closed or failed.
    pub fn recv_frame(&mut self, within: Duration) -> Result<Option<Frame>, String> {
        let until = Instant::now() + within;
        loop {
            match self.ws.read() {
                Ok(tungstenite::Message::Text(t)) => {
                    if let Ok(m) = Message::from_json(&t) {
                        return Ok(Some(Frame::Link(m)));
                    }
                    if let Ok(m) = DirectoryMessage::from_json(&t) {
                        return Ok(Some(Frame::Directory(m)));
                    }
                    if let Ok(m) = ReachMessage::from_json(&t) {
                        if self.reach.len() >= 1_000 {
                            self.reach.pop_front();
                        }
                        self.reach.push_back(m);
                    }
                    if let Ok(m) = SayMessage::from_json(&t) {
                        if self.says.len() >= 1_000 {
                            self.says.pop_front();
                        }
                        self.says.push_back(m);
                    }
                    if let Ok(m) = RoomsMessage::from_json(&t) {
                        if self.rooms.len() >= 1_000 {
                            self.rooms.pop_front();
                        }
                        self.rooms.push_back(m);
                    }
                }
                Ok(tungstenite::Message::Close(_)) => return Err("closed".into()),
                Ok(_) => {}
                Err(tungstenite::Error::Io(e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(e) => return Err(e.to_string()),
            }
            let _ = self.ws.flush();
            if Instant::now() >= until {
                return Ok(None);
            }
        }
    }

    /// Reads until a message `pick` takes, within `within`.
    ///
    /// # Errors
    /// When none came, or the link closed first.
    pub fn expect<T>(
        &mut self,
        within: Duration,
        what: &str,
        mut pick: impl FnMut(&Message) -> Option<T>,
    ) -> Result<T, String> {
        let until = Instant::now() + within;
        loop {
            let left = until.saturating_duration_since(Instant::now());
            match self.recv(left)? {
                Some(m) => {
                    if let Some(t) = pick(&m) {
                        return Ok(t);
                    }
                }
                None => return Err(format!("{}: no {what} within {within:?}", self.slug)),
            }
        }
    }

    /// Nothing `pick` takes arrives within `within` (other messages are let by).
    ///
    /// # Errors
    /// When one does.
    pub fn quiet(
        &mut self,
        within: Duration,
        what: &str,
        mut pick: impl FnMut(&Message) -> bool,
    ) -> Result<(), String> {
        let until = Instant::now() + within;
        while Instant::now() < until {
            let left = until.saturating_duration_since(Instant::now());
            match self.recv(left) {
                Ok(Some(m)) if pick(&m) => {
                    return Err(format!("{}: unexpected {what}: {m:?}", self.slug))
                }
                Ok(_) => {}
                Err(_) => return Ok(()),
            }
        }
        Ok(())
    }

    /// Whether the hub closes the link within `within`.
    pub fn closes_within(&mut self, within: Duration) -> bool {
        let until = Instant::now() + within;
        while Instant::now() < until {
            if self.recv(Duration::from_millis(100)).is_err() {
                return true;
            }
        }
        false
    }

    /// `hello`, and the `welcome` it is answered with.
    ///
    /// # Errors
    /// When no welcome comes.
    pub fn greet(&mut self) -> Result<Message, String> {
        self.send(&Message::Hello {
            proto: PROTOCOL,
            software: "conformance suite".into(),
            websocket_url: None,
            status_url: None,
        })?;
        self.expect(Duration::from_secs(5), "welcome", |m| {
            matches!(m, Message::Welcome { .. }).then(|| m.clone())
        })
    }

    /// Closes the link.
    pub fn close(mut self) {
        let _ = self.ws.close(None);
        let _ = self.ws.flush();
    }
}

/// A run's tag: letters only, so it fits in a character's and a channel's name.
fn tag() -> String {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros();
    let mut n = u64::try_from(n % 1_000_000_000_000).unwrap_or(0);
    let mut s = String::new();
    while n > 0 {
        s.push(char::from(b'a' + u8::try_from(n % 26).unwrap_or(0)));
        n /= 26;
    }
    s
}

fn addr(name: &str, world: &str) -> Address {
    Address::new(name, world).expect("the suite's own addresses are valid")
}

fn open_greeted(t: &HubTarget, i: usize) -> Result<Link, String> {
    let (slug, key) = &t.worlds[i];
    let mut l = Link::open(&t.base, slug, key).map_err(|e| format!("{slug}: {e:?}"))?;
    l.greet()?;
    Ok(l)
}

/// A listing of `who`, with the smallest profile when it is `on`.
fn listing(id: &str, who: &Address, on: bool) -> DirectoryMessage {
    DirectoryMessage::Listing {
        id: id.to_owned(),
        who: who.clone(),
        on,
        profile: on.then(|| {
            Box::new(Profile {
                level: 1,
                heritage: 1,
                gender: 1,
                ..Profile::default()
            })
        }),
    }
}

fn result_of(m: &Message, id: &str) -> Option<Outcome> {
    match m {
        Message::TellResult { id: i, outcome, .. } if i == id => Some(*outcome),
        _ => None,
    }
}

const WAIT: Duration = Duration::from_secs(5);

/// Runs every hub check against `t`.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn run(t: &HubTarget) -> Vec<Check> {
    let mut out = Vec::new();
    if t.worlds.len() < 2 {
        out.push(Check::new(
            "hub",
            "hub.setup",
            Err("two federated worlds are needed".into()),
        ));
        return out;
    }
    let (a, b) = (t.worlds[0].0.clone(), t.worlds[1].0.clone());
    let tg = tag();
    let ann = format!("Ann{tg}");
    let bob = format!("Bob{tg}");
    let mut check = |name: &'static str, f: &mut dyn FnMut() -> Result<(), String>| {
        out.push(Check::new("hub", name, f()));
    };

    check("link.a_wrong_key_is_refused_401", &mut || {
        let wrong = format!("drth_{}", "A".repeat(43));
        match Link::open(&t.base, &a, &wrong) {
            Err(Open::Refused(401)) => Ok(()),
            other => Err(format!("{other:?}")),
        }
    });
    if let Some((off, key)) = &t.off {
        check(
            "link.a_world_whose_flag_is_not_on_is_refused_403",
            &mut || match Link::open(&t.base, off, key) {
                Err(Open::Refused(403)) => Ok(()),
                other => Err(format!("{other:?}")),
            },
        );
    }
    check(
        "link.hello_is_welcomed_with_the_worlds_own_slug",
        &mut || {
            let mut l = Link::open(&t.base, &t.worlds[0].0, &t.worlds[0].1)
                .map_err(|e| format!("{e:?}"))?;
            match l.greet()? {
                Message::Welcome { proto, world, .. } if proto == PROTOCOL && world == a => {
                    l.close();
                    Ok(())
                }
                other => Err(format!("{other:?}")),
            }
        },
    );
    check("link.anything_before_hello_is_refused", &mut || {
        let mut l =
            Link::open(&t.base, &t.worlds[0].0, &t.worlds[0].1).map_err(|e| format!("{e:?}"))?;
        l.send(&Message::Status {
            state: "online".into(),
            players_online: None,
        })?;
        let code = l.expect(WAIT, "error", |m| match m {
            Message::Error { code, .. } => Some(code.clone()),
            _ => None,
        })?;
        if code != "order" {
            return Err(format!("code {code}"));
        }
        if !l.closes_within(WAIT) {
            return Err("the link stayed open".into());
        }
        Ok(())
    });
    check("link.another_protocol_version_is_refused", &mut || {
        let mut l =
            Link::open(&t.base, &t.worlds[0].0, &t.worlds[0].1).map_err(|e| format!("{e:?}"))?;
        l.send(&Message::Hello {
            proto: PROTOCOL + 100,
            software: "x".into(),
            websocket_url: None,
            status_url: None,
        })?;
        let code = l.expect(WAIT, "error", |m| match m {
            Message::Error { code, .. } => Some(code.clone()),
            _ => None,
        })?;
        (code == "protocol" && l.closes_within(WAIT))
            .then_some(())
            .ok_or_else(|| format!("code {code}, or the link stayed open"))
    });
    check("link.an_unknown_type_is_skipped", &mut || {
        let mut la = open_greeted(t, 0)?;
        let mut lb = open_greeted(t, 1)?;
        la.send_raw(r#"{"type":"mail.send","to":"x"}"#)?;
        let id = format!("u{tg}");
        la.send(&Message::Tell {
            id: id.clone(),
            from: addr(&ann, &a),
            to: addr(&bob, &b),
            text: "after an unknown type".into(),
            friend: false,
        })?;
        lb.expect(WAIT, "the tell after the unknown message", |m| {
            matches!(m, Message::Tell { .. }).then_some(())
        })?;
        la.close();
        lb.close();
        Ok(())
    });
    check(
        "rooms.a_world_is_told_its_rooms_after_its_welcome",
        &mut || {
            let mut l = open_greeted(t, 0)?;
            l.expect_rooms(WAIT, "its rooms", |m| {
                matches!(m, RoomsMessage::Setup { id: None, .. }).then_some(())
            })?;
            l.close();
            Ok(())
        },
    );
    check(
        "rooms.a_worlds_global_room_is_set_and_a_seed_taken_only_while_none_is_held",
        &mut || {
            let mut l = open_greeted(t, 0)?;
            let answer = |l: &mut Link, id: &str, room: &str, seed: bool| {
                l.send_raw(
                    &RoomsMessage::Global {
                        id: id.to_owned(),
                        room: room.to_owned(),
                        seed,
                    }
                    .to_json(),
                )?;
                l.expect_rooms(WAIT, "the answer", |m| match m {
                    RoomsMessage::Setup {
                        id: Some(i),
                        global,
                        mirrored,
                    } if i == id => Some((global.clone(), mirrored.clone())),
                    _ => None,
                })
            };
            let (global, mirrored) = answer(&mut l, &format!("g{tg}"), "lfg", false)?;
            if global.as_deref() != Some("lfg") || mirrored.iter().any(|r| r == "lfg") {
                return Err(format!("set: {global:?} {mirrored:?}"));
            }
            let (global, _) = answer(&mut l, &format!("s{tg}"), "trade", true)?;
            if global.as_deref() != Some("lfg") {
                return Err(format!("a seed changed the room held: {global:?}"));
            }
            l.close();
            Ok(())
        },
    );
    check(
        "rooms.a_worlds_rooms_message_is_refused_as_the_hubs_own",
        &mut || {
            let mut l = open_greeted(t, 0)?;
            l.send_raw(r#"{"type":"rooms","global":"trade","mirrored":[]}"#)?;
            let code = l.expect(WAIT, "error", |m| match m {
                Message::Error { code, .. } => Some(code.clone()),
                _ => None,
            })?;
            l.close();
            (code == "malformed")
                .then_some(())
                .ok_or_else(|| format!("code {code}"))
        },
    );
    check("world.a_link_up_and_down_is_announced", &mut || {
        let mut la = open_greeted(t, 0)?;
        let lb = open_greeted(t, 1)?;
        la.expect(WAIT, "world up", |m| match m {
            Message::World(w) if w.slug == b && w.online => Some(()),
            _ => None,
        })?;
        lb.close();
        la.expect(WAIT, "world down", |m| match m {
            Message::World(w) if w.slug == b && !w.online => Some(()),
            _ => None,
        })?;
        la.close();
        Ok(())
    });
    check(
        "link.a_worlds_web_addresses_are_listed_only_while_its_link_is_up",
        &mut || {
            let listed = |slug: &str| -> Result<(serde_json::Value, serde_json::Value), String> {
                let w =
                    http::get(&t.base, &format!("/v1/servers/{}", http::encode(slug)))?.json()?;
                Ok((w["websocketUrl"].clone(), w["statusUrl"].clone()))
            };
            let none = (serde_json::Value::Null, serde_json::Value::Null);
            let link_naming = |ws: String, status: String| -> Result<Link, String> {
                let mut l =
                    Link::open(&t.base, &b, &t.worlds[1].1).map_err(|e| format!("{e:?}"))?;
                l.send(&Message::Hello {
                    proto: PROTOCOL,
                    software: "conformance suite".into(),
                    websocket_url: Some(ws),
                    status_url: Some(status),
                })?;
                l.expect(WAIT, "welcome", |m| {
                    matches!(m, Message::Welcome { .. }).then_some(())
                })?;
                Ok(l)
            };
            let ws = format!("wss://{b}.example.org/ws");
            let status = format!("https://{b}.example.org/status");
            let mut la = open_greeted(t, 0)?;
            let lb = link_naming(ws.clone(), status.clone())?;
            // This link's coming up, so that the going down awaited below is this link's.
            la.expect(WAIT, "world up", |m| match m {
                Message::World(w) if w.slug == b && w.online => Some(()),
                _ => None,
            })?;
            let up = listed(&b)?;
            if up != (serde_json::json!(ws), serde_json::json!(status)) {
                return Err(format!("while its link is up: {up:?}"));
            }
            lb.close();
            la.expect(WAIT, "world down", |m| match m {
                Message::World(w) if w.slug == b && !w.online => Some(()),
                _ => None,
            })?;
            let down = listed(&b)?;
            if down != none {
                return Err(format!("once its link is down: {down:?}"));
            }
            let lb = link_naming(
                format!("ws://{b}.example.org/ws"),
                format!("http://{b}.example.org/status"),
            )?;
            let plain = listed(&b)?;
            lb.close();
            la.close();
            if plain != none {
                return Err(format!("plain addresses to another machine: {plain:?}"));
            }
            Ok(())
        },
    );
    check(
        "tell.is_routed_and_answered_under_the_senders_id",
        &mut || {
            let mut la = open_greeted(t, 0)?;
            let mut lb = open_greeted(t, 1)?;
            let id = format!("t{tg}");
            la.send(&Message::Tell {
                id: id.clone(),
                from: addr(&ann, &a),
                to: addr(&bob, &b),
                text: "still playing?".into(),
                friend: true,
            })?;
            let (hub_id, from, text, friend) = lb.expect(WAIT, "the tell", |m| match m {
                Message::Tell {
                    id,
                    from,
                    text,
                    friend,
                    ..
                } => Some((id.clone(), from.clone(), text.clone(), *friend)),
                _ => None,
            })?;
            if from.to_string() != format!("{ann}@{a}") || text != "still playing?" || !friend {
                return Err(format!("forwarded as {from} {text:?} friend={friend}"));
            }
            if hub_id == id {
                return Err("forwarded under the sender's own id, not one of the hub's".into());
            }
            lb.send(&Message::TellResult {
                id: hub_id,
                outcome: Outcome::Delivered,
                message: None,
            })?;
            let o = la.expect(WAIT, "the result", |m| result_of(m, &id))?;
            la.close();
            lb.close();
            (o == Outcome::Delivered)
                .then_some(())
                .ok_or_else(|| format!("{o:?}"))
        },
    );
    check(
        "tell.a_world_cannot_send_as_another_worlds_character",
        &mut || {
            let mut la = open_greeted(t, 0)?;
            let mut lb = open_greeted(t, 1)?;
            la.send(&Message::Tell {
                id: format!("s{tg}"),
                from: addr(&bob, &b),
                to: addr(&bob, &b),
                text: "spoofed".into(),
                friend: false,
            })?;
            let code = la.expect(WAIT, "error", |m| match m {
                Message::Error { code, .. } => Some(code.clone()),
                _ => None,
            })?;
            lb.quiet(Duration::from_secs(1), "spoofed tell", |m| {
                matches!(m, Message::Tell { .. })
            })?;
            la.close();
            lb.close();
            (code == "identity").then_some(()).ok_or(code)
        },
    );
    check(
        "tell.to_a_world_not_on_the_network_is_not_on_network",
        &mut || {
            let mut la = open_greeted(t, 0)?;
            let mut targets = vec![format!("nowhere{}", &tg[..tg.len().min(4)])];
            if let Some((off, _)) = &t.off {
                targets.push(off.clone());
            }
            for (i, w) in targets.iter().enumerate() {
                let id = format!("n{i}{tg}");
                la.send(&Message::Tell {
                    id: id.clone(),
                    from: addr(&ann, &a),
                    to: addr("Cid", w),
                    text: "hello".into(),
                    friend: false,
                })?;
                let o = la.expect(WAIT, "the result", |m| result_of(m, &id))?;
                if o != Outcome::NotOnNetwork {
                    return Err(format!("{w}: {o:?}"));
                }
            }
            la.close();
            Ok(())
        },
    );
    check("tell.a_resent_id_is_delivered_once", &mut || {
        let mut la = open_greeted(t, 0)?;
        let mut lb = open_greeted(t, 1)?;
        let m = Message::Tell {
            id: format!("d{tg}"),
            from: addr(&ann, &a),
            to: addr(&bob, &b),
            text: "once".into(),
            friend: false,
        };
        la.send(&m)?;
        la.send(&m)?;
        lb.expect(WAIT, "the tell", |m| {
            matches!(m, Message::Tell { .. }).then_some(())
        })?;
        lb.quiet(Duration::from_secs(1), "a second copy", |m| {
            matches!(m, Message::Tell { .. })
        })?;
        la.close();
        lb.close();
        Ok(())
    });
    check("tell.held_for_a_world_that_reconnects", &mut || {
        let mut la =
            Link::open(&t.base, &t.worlds[0].0, &t.worlds[0].1).map_err(|e| format!("{e:?}"))?;
        // b must be away first: an earlier check's link may still be closing.
        if let Message::Welcome { worlds, .. } = la.greet()? {
            if worlds.iter().any(|w| w.slug == b && w.online) {
                la.expect(WAIT, "b going down", |m| match m {
                    Message::World(w) if w.slug == b && !w.online => Some(()),
                    _ => None,
                })?;
            }
        }
        let id = format!("h{tg}");
        la.send(&Message::Tell {
            id: id.clone(),
            from: addr(&ann, &a),
            to: addr(&bob, &b),
            text: "are you back?".into(),
            friend: false,
        })?;
        la.quiet(
            Duration::from_millis(500),
            "an answer while b is away",
            |m| result_of(m, &id).is_some(),
        )?;
        let mut lb = open_greeted(t, 1)?;
        let hub_id = lb.expect(WAIT, "the held tell", |m| match m {
            Message::Tell { id, text, .. } if text == "are you back?" => Some(id.clone()),
            _ => None,
        })?;
        lb.send(&Message::TellResult {
            id: hub_id,
            outcome: Outcome::Delivered,
            message: None,
        })?;
        la.expect(WAIT, "the result", |m| result_of(m, &id))?;
        la.close();
        lb.close();
        Ok(())
    });
    check("limits.a_characters_tells_are_rate_limited", &mut || {
        let mut la =
            Link::open(&t.base, &t.worlds[0].0, &t.worlds[0].1).map_err(|e| format!("{e:?}"))?;
        let burst = match la.greet()? {
            Message::Welcome { limits, .. } => limits.tells_per_character.burst,
            other => return Err(format!("{other:?}")),
        };
        let mut lb = open_greeted(t, 1)?;
        let sender = format!("Rat{tg}");
        let mut limited_at = None;
        for i in 0..40u32 {
            let id = format!("r{i}{tg}");
            la.send(&Message::Tell {
                id: id.clone(),
                from: addr(&sender, &a),
                to: addr(&bob, &b),
                text: "spam".into(),
                friend: false,
            })?;
            // b answers every tell that reaches it while a waits, so a's answer comes from b or
            // from the hub, however the two links are scheduled.
            let until = Instant::now() + WAIT;
            let outcome = loop {
                if let Some(Message::Tell { id, .. }) = lb.recv(Duration::from_millis(10))? {
                    lb.send(&Message::TellResult {
                        id,
                        outcome: Outcome::Delivered,
                        message: None,
                    })?;
                }
                if let Some(m) = la.recv(Duration::from_millis(10))? {
                    if let Some(o) = result_of(&m, &id) {
                        break o;
                    }
                }
                if Instant::now() >= until {
                    return Err(format!("{a}: no result for tell {i} within {WAIT:?}"));
                }
            };
            if outcome == Outcome::RateLimited {
                limited_at = Some(i);
                break;
            }
        }
        la.close();
        lb.close();
        // A token may come back while the tells go, at the limit's own rate.
        #[allow(clippy::cast_precision_loss)]
        match limited_at {
            Some(i) if f64::from(i) >= burst && f64::from(i) <= burst + 2.0 => Ok(()),
            Some(i) => Err(format!(
                "limited at tell {i}, not near the burst of {burst}"
            )),
            None => Err("40 tells at once, none limited".into()),
        }
    });
    check("link.a_second_link_replaces_the_first", &mut || {
        let mut first = open_greeted(t, 0)?;
        let mut second = open_greeted(t, 0)?;
        if !first.closes_within(WAIT) {
            return Err("the first link stayed open".into());
        }
        if second.closes_within(Duration::from_millis(500)) {
            return Err("the second link closed too".into());
        }
        second.close();
        Ok(())
    });
    check(
        "link.a_binary_or_oversized_frame_closes_the_link",
        &mut || {
            let mut l = open_greeted(t, 0)?;
            l.send_binary(b"not text")?;
            if !l.closes_within(WAIT) {
                return Err("a binary frame left the link open".into());
            }
            let mut l = open_greeted(t, 0)?;
            let big = format!(
                "{{\"type\":\"status\",\"state\":\"online\",\"pad\":\"{}\"}}",
                "x".repeat(dereth_network_protocol::wire::MAX_MESSAGE_BYTES)
            );
            // The hub may close before it has read the whole frame.
            let _ = l.send_raw(&big);
            if !l.closes_within(WAIT) {
                return Err("a frame over the limit left the link open".into());
            }
            Ok(())
        },
    );
    check(
        "identity.a_world_cannot_watch_say_or_ask_as_another_worlds_character",
        &mut || {
            let mut la = open_greeted(t, 0)?;
            let mut lb = open_greeted(t, 1)?;
            lb.send(&Message::RoomJoin {
                room: "global".into(),
                on: true,
            })?;
            la.send(&Message::RoomJoin {
                room: "global".into(),
                on: true,
            })?;
            let spoofed = addr(&bob, &b);
            let sends = [
                Message::Watch {
                    owner: spoofed.clone(),
                    target: addr(&bob, &b),
                    on: true,
                },
                Message::RoomSay {
                    id: format!("is{tg}"),
                    room: "global".into(),
                    from: spoofed.clone(),
                    text: "spoofed".into(),
                    via: None,
                },
                Message::Channel {
                    id: format!("ic{tg}"),
                    op: ChannelOp::Create,
                    channel: format!("s{}", &tg[..tg.len().min(16)]),
                    by: spoofed,
                    who: None,
                },
            ];
            for m in &sends {
                la.send(m)?;
                la.expect(WAIT, "an identity error", |r| {
                    matches!(r, Message::Error { code, .. } if code == "identity").then_some(())
                })?;
            }
            lb.quiet(Duration::from_millis(500), "a spoofed message", |m| {
                matches!(m, Message::Watch { .. } | Message::RoomLine { .. })
            })?;
            la.close();
            lb.close();
            Ok(())
        },
    );
    check(
        "presence.watches_and_presence_go_between_the_named_worlds",
        &mut || {
            let mut la = open_greeted(t, 0)?;
            let mut lb = open_greeted(t, 1)?;
            la.send(&Message::Watch {
                owner: addr(&ann, &a),
                target: addr(&bob, &b),
                on: true,
            })?;
            lb.expect(WAIT, "the watch", |m| {
                matches!(m, Message::Watch { .. }).then_some(())
            })?;
            lb.send(&Message::Presence {
                to: addr(&ann, &a),
                of: addr(&bob, &b),
                online: true,
            })?;
            la.expect(WAIT, "the presence", |m| {
                matches!(m, Message::Presence { online: true, .. }).then_some(())
            })?;
            // a may not speak for b's characters.
            la.send(&Message::Presence {
                to: addr(&bob, &b),
                of: addr(&bob, &b),
                online: true,
            })?;
            la.expect(WAIT, "an identity error", |m| {
                matches!(m, Message::Error { code, .. } if code == "identity").then_some(())
            })?;
            lb.quiet(Duration::from_millis(500), "spoofed presence", |m| {
                matches!(m, Message::Presence { .. })
            })?;
            la.close();
            lb.close();
            Ok(())
        },
    );
    check(
        "room.a_line_reaches_the_other_worlds_carrying_the_room",
        &mut || {
            let mut la = open_greeted(t, 0)?;
            let mut lb = open_greeted(t, 1)?;
            for l in [&mut la, &mut lb] {
                l.send(&Message::RoomJoin {
                    room: "global".into(),
                    on: true,
                })?;
            }
            let mut lc = if t.worlds.len() > 2 {
                Some(open_greeted(t, 2)?)
            } else {
                None
            };
            std::thread::sleep(Duration::from_millis(200));
            let id = format!("e{tg}");
            la.send(&Message::RoomSay {
                id: id.clone(),
                room: "global".into(),
                from: addr(&ann, &a),
                text: "dance at 8".into(),
                via: None,
            })?;
            lb.expect(WAIT, "the room line", |m| match m {
                Message::RoomLine { from, text, .. }
                    if text == "dance at 8" && from.world() == a =>
                {
                    Some(())
                }
                _ => None,
            })?;
            // Its own line back is a failure whether it comes before the result or after it.
            let mut echoed = None;
            let o = la.expect(WAIT, "the room result", |m| match m {
                Message::RoomResult { id: i, outcome } if *i == id => Some(*outcome),
                Message::RoomLine { .. } => {
                    echoed.get_or_insert_with(|| format!("{m:?}"));
                    None
                }
                _ => None,
            })?;
            if let Some(e) = echoed {
                return Err(format!("{a}: its own line back: {e}"));
            }
            if o != Outcome::Ok {
                return Err(format!("{o:?}"));
            }
            la.quiet(Duration::from_millis(500), "its own line back", |m| {
                matches!(m, Message::RoomLine { .. })
            })?;
            if let Some(lc) = lc.as_mut() {
                lc.quiet(
                    Duration::from_millis(500),
                    "a line for a room it does not carry",
                    |m| matches!(m, Message::RoomLine { .. }),
                )?;
                let id = format!("f{tg}");
                lc.send(&Message::RoomSay {
                    id: id.clone(),
                    room: "global".into(),
                    from: addr("Cid", &t.worlds[2].0),
                    text: "not carried".into(),
                    via: None,
                })?;
                let o = lc.expect(WAIT, "the room result", |m| match m {
                    Message::RoomResult { id: i, outcome } if *i == id => Some(*outcome),
                    _ => None,
                })?;
                if o != Outcome::NotMember {
                    return Err(format!("a world not carrying the room: {o:?}"));
                }
            }
            la.close();
            lb.close();
            if let Some(lc) = lc {
                lc.close();
            }
            Ok(())
        },
    );
    check(
        "channel.joined_by_invitation_and_its_lines_reach_the_other_members",
        &mut || {
            let mut la = open_greeted(t, 0)?;
            let mut lb = open_greeted(t, 1)?;
            let channel = format!("c{}", &tg[..tg.len().min(16)]);
            let req = |l: &mut Link,
                       n: &str,
                       op: ChannelOp,
                       by: &Address,
                       who: Option<Address>|
             -> Result<ChannelOutcome, String> {
                let id = format!("{n}{tg}");
                l.send(&Message::Channel {
                    id: id.clone(),
                    op,
                    channel: channel.clone(),
                    by: by.clone(),
                    who,
                })?;
                l.expect(WAIT, "the channel result", |m| match m {
                    Message::ChannelResult { id: i, outcome, .. } if *i == id => Some(*outcome),
                    _ => None,
                })
            };
            let (x, y) = (addr(&ann, &a), addr(&bob, &b));
            if req(&mut lb, "cj", ChannelOp::Join, &y, None)? != ChannelOutcome::NoChannel {
                return Err("joined a channel that does not exist".into());
            }
            if req(&mut la, "cc", ChannelOp::Create, &x, None)? != ChannelOutcome::Ok {
                return Err("could not create".into());
            }
            if req(&mut lb, "cn", ChannelOp::Join, &y, None)? != ChannelOutcome::NotInvited {
                return Err("joined uninvited".into());
            }
            if req(&mut lb, "cx", ChannelOp::Invite, &y, Some(y.clone()))?
                != ChannelOutcome::NotMember
            {
                return Err("a non-member invited".into());
            }
            let outsider = format!("co{tg}");
            lb.send(&Message::Tell {
                id: outsider.clone(),
                from: y.clone(),
                to: addr(&channel, "channel"),
                text: "let me in".into(),
                friend: false,
            })?;
            if lb.expect(WAIT, "the result", |m| result_of(m, &outsider))? != Outcome::NotMember {
                return Err("a non-member's channel tell was not refused not_member".into());
            }
            if req(&mut la, "ci", ChannelOp::Invite, &x, Some(y.clone()))? != ChannelOutcome::Ok {
                return Err("could not invite".into());
            }
            lb.expect(WAIT, "the invitation", |m| {
                matches!(m, Message::ChannelInvite { .. }).then_some(())
            })?;
            if req(&mut lb, "cy", ChannelOp::Join, &y, None)? != ChannelOutcome::Ok {
                return Err("could not join".into());
            }
            let id = format!("cl{tg}");
            lb.send(&Message::Tell {
                id: id.clone(),
                from: y.clone(),
                to: addr(&channel, "channel"),
                text: "anyone around?".into(),
                friend: false,
            })?;
            la.expect(WAIT, "the channel line", |m| match m {
                Message::ChannelLine { to, from, text, .. }
                    if to.same(&x) && from.same(&y) && text == "anyone around?" =>
                {
                    Some(())
                }
                _ => None,
            })?;
            if lb.expect(WAIT, "the result", |m| result_of(m, &id))? != Outcome::Delivered {
                return Err("the channel tell was not delivered".into());
            }
            req(&mut lb, "cl1", ChannelOp::Leave, &y, None)?;
            req(&mut la, "cl2", ChannelOp::Leave, &x, None)?;
            if req(&mut lb, "cz", ChannelOp::Join, &y, None)? != ChannelOutcome::NoChannel {
                return Err("the channel outlived its last member".into());
            }
            la.close();
            lb.close();
            Ok(())
        },
    );
    check(
        "person.a_world_cannot_ask_for_another_worlds_character",
        &mut || {
            let mut la = open_greeted(t, 0)?;
            for m in [
                Message::Person {
                    id: format!("pi{tg}"),
                    op: PersonOp::Code,
                    who: addr(&bob, &b),
                    target: None,
                    targets: Vec::new(),
                    shown: None,
                },
                Message::PersonWatch {
                    who: addr(&bob, &b),
                    on: true,
                },
            ] {
                la.send(&m)?;
                let code = la.expect(WAIT, "an identity error", |m| match m {
                    Message::Error { code, .. } => Some(code.clone()),
                    Message::PersonResult { .. } | Message::PersonFriends { .. } => {
                        Some(format!("an answer: {m:?}"))
                    }
                    _ => None,
                })?;
                if code != "identity" {
                    return Err(code);
                }
            }
            la.close();
            Ok(())
        },
    );
    check(
        "person.a_character_nobody_linked_has_no_person",
        &mut || {
            let mut la = open_greeted(t, 0)?;
            let who = addr(&format!("Nobody{tg}"), &a);
            la.send(&Message::PersonWatch {
                who: who.clone(),
                on: true,
            })?;
            let view = la.expect(WAIT, "the view", |m| match m {
                Message::PersonFriends {
                    to,
                    linked,
                    persons,
                    ..
                } if to.same(&who) => Some((*linked, persons.len())),
                _ => None,
            })?;
            if view != (false, 0) {
                return Err(format!("its view: {view:?}"));
            }
            let asks = [
                (
                    PersonOp::Characters,
                    Some(addr(&bob, &b)),
                    Vec::new(),
                    None,
                    PersonOutcome::Ok,
                ),
                (
                    PersonOp::Befriend,
                    Some(addr(&bob, &b)),
                    Vec::new(),
                    None,
                    PersonOutcome::NotLinked,
                ),
                (
                    PersonOp::Carry,
                    None,
                    vec![addr(&bob, &b)],
                    None,
                    PersonOutcome::NotLinked,
                ),
                (
                    PersonOp::Alts,
                    None,
                    Vec::new(),
                    Some(true),
                    PersonOutcome::NotLinked,
                ),
            ];
            for (i, (op, target, targets, shown, want)) in asks.into_iter().enumerate() {
                let id = format!("pn{tg}{i}");
                la.send(&Message::Person {
                    id: id.clone(),
                    op,
                    who: who.clone(),
                    target,
                    targets,
                    shown,
                })?;
                let (outcome, characters) = la.expect(WAIT, "the answer", |m| match m {
                    Message::PersonResult {
                        id: i,
                        outcome,
                        characters,
                        ..
                    } if *i == id => Some((*outcome, characters.len())),
                    _ => None,
                })?;
                if outcome != want || characters != 0 {
                    return Err(format!("{op:?}: {outcome:?} naming {characters}"));
                }
            }
            la.close();
            Ok(())
        },
    );
    check(
        "person.a_link_code_is_well_formed_or_refused_unavailable",
        &mut || {
            let mut la = open_greeted(t, 0)?;
            let id = format!("pc{tg}");
            la.send(&Message::Person {
                id: id.clone(),
                op: PersonOp::Code,
                who: addr(&ann, &a),
                target: None,
                targets: Vec::new(),
                shown: None,
            })?;
            let answer = la.expect(WAIT, "the answer", |m| match m {
                Message::PersonResult { id: i, .. } if *i == id => Some(m.clone()),
                _ => None,
            })?;
            la.close();
            match answer {
                Message::PersonResult {
                    outcome: PersonOutcome::Unavailable,
                    code: None,
                    ..
                } => Ok(()),
                Message::PersonResult {
                    outcome: PersonOutcome::Ok,
                    code: Some(code),
                    expires_secs: Some(secs),
                    url: Some(_),
                    ..
                } if is_link_code(&code) && (1..=600).contains(&secs) => Ok(()),
                other => Err(format!("{other:?}")),
            }
        },
    );
    check(
        "listing.a_world_cannot_list_another_worlds_character",
        &mut || {
            let mut la = open_greeted(t, 0)?;
            la.send_raw(&listing(&format!("li{tg}"), &addr(&bob, &b), true).to_json())?;
            let until = Instant::now() + WAIT;
            loop {
                let left = until.saturating_duration_since(Instant::now());
                match la.recv_frame(left)? {
                    Some(Frame::Link(Message::Error { code, .. })) if code == "identity" => break,
                    Some(Frame::Link(Message::Error { code, .. })) => return Err(code),
                    Some(Frame::Directory(m)) => return Err(format!("an answer: {m:?}")),
                    Some(Frame::Link(_)) => {}
                    None => return Err("no identity error".into()),
                }
            }
            la.close();
            Ok(())
        },
    );
    check(
        "listing.a_listing_is_answered_and_taking_it_out_is_ok",
        &mut || {
            let mut la = open_greeted(t, 0)?;
            let who = addr(&format!("Lis{tg}"), &a);
            let answer = |la: &mut Link, id: String, on: bool| {
                la.send_raw(&listing(&id, &who, on).to_json())?;
                la.expect_directory(WAIT, "the listing's answer", |m| match m {
                    DirectoryMessage::ListingResult {
                        id: i,
                        outcome,
                        url,
                        ..
                    } if *i == id => Some((*outcome, url.is_some())),
                    _ => None,
                })
            };
            let on = answer(&mut la, format!("lo{tg}"), true)?;
            let off = answer(&mut la, format!("lf{tg}"), false)?;
            la.close();
            match (on, off) {
                ((ListingOutcome::Ok, true), (ListingOutcome::Ok, _))
                | ((ListingOutcome::Unavailable, _), (ListingOutcome::Unavailable, _)) => Ok(()),
                other => Err(format!("{other:?}")),
            }
        },
    );
    if t.slow {
        check(
            "tell.an_unanswered_tell_is_answered_unavailable",
            &mut || {
                let mut la = open_greeted(t, 0)?;
                let mut lb = open_greeted(t, 1)?;
                let id = format!("x{tg}");
                la.send(&Message::Tell {
                    id: id.clone(),
                    from: addr(&ann, &a),
                    to: addr(&bob, &b),
                    text: "never answered".into(),
                    friend: false,
                })?;
                lb.expect(WAIT, "the tell", |m| {
                    matches!(m, Message::Tell { .. }).then_some(())
                })?;
                let o = la.expect(Duration::from_secs(45), "the hub's answer", |m| {
                    result_of(m, &id)
                })?;
                la.close();
                lb.close();
                (o == Outcome::Unavailable)
                    .then_some(())
                    .ok_or_else(|| format!("{o:?}"))
            },
        );
        check("tell.a_held_tell_lapses_unavailable", &mut || {
            let mut la = Link::open(&t.base, &t.worlds[0].0, &t.worlds[0].1)
                .map_err(|e| format!("{e:?}"))?;
            if let Message::Welcome { worlds, .. } = la.greet()? {
                if worlds.iter().any(|w| w.slug == b && w.online) {
                    la.expect(WAIT, "b going down", |m| match m {
                        Message::World(w) if w.slug == b && !w.online => Some(()),
                        _ => None,
                    })?;
                }
            }
            let id = format!("hx{tg}");
            la.send(&Message::Tell {
                id: id.clone(),
                from: addr(&ann, &a),
                to: addr(&bob, &b),
                text: "nobody comes back".into(),
                friend: false,
            })?;
            let o = la.expect(Duration::from_secs(75), "the hub's answer", |m| {
                result_of(m, &id)
            })?;
            la.close();
            (o == Outcome::Unavailable)
                .then_some(())
                .ok_or_else(|| format!("{o:?}"))
        });
    }
    out.extend(directory::checks(t, &tg));
    out
}
