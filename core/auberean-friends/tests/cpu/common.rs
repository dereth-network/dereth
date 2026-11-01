//! What the friends stream's tests share: the vectors, an element's fields as the vectors write
//! them, the elements in a run of bytes, and a client and a world joined by a clock the tests turn.

use core::time::Duration;
use std::sync::OnceLock;

use auberean_friends::{
    Answer, ClientConfig, ClientOut, ClientSession, Element, Event, Extra, FrameReader,
    FriendStatus, Request, ServerConfig, ServerEvent, ServerOut, ServerStream, PREAMBLE,
};
use serde_json::{json, Map, Value};

/// The vectors, read once.
pub fn vectors() -> &'static Value {
    static V: OnceLock<Value> = OnceLock::new();
    V.get_or_init(|| {
        serde_json::from_str(include_str!("../../vectors/friends.json"))
            .expect("the vectors are JSON")
    })
}

/// One list of the vectors.
pub fn list(name: &str) -> &'static [Value] {
    vectors()[name]
        .as_array()
        .unwrap_or_else(|| panic!("the vectors have no list {name}"))
}

/// A string field of a vector.
pub fn text<'a>(v: &'a Value, name: &str) -> &'a str {
    v[name]
        .as_str()
        .unwrap_or_else(|| panic!("no string {name} in {v}"))
}

/// Seconds as a duration.
pub fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

/// Bytes from hex.
pub fn unhex(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex"))
        .collect()
}

fn friends(list: &[FriendStatus]) -> Value {
    list.iter()
        .map(|f| json!({"user": f.user, "mode": f.mode.0, "location": f.location}))
        .collect()
}

fn extra(e: Option<&Extra>) -> Value {
    e.map_or(Value::Null, |e| json!(e.to_string()))
}

/// An element's fields, as the vectors write them.
pub fn fields(e: &Element) -> Value {
    let mut m = Map::new();
    m.insert("element".into(), json!(e.name()));
    let mut put = |k: &str, v: Value| {
        m.insert(k.into(), v);
    };
    match e {
        Element::Hello(h) => {
            put("proto", json!(h.proto));
            put("software", json!(h.software));
            put("locale", json!(h.locale));
        }
        Element::Welcome(w) => {
            put("proto", json!(w.proto));
            put("world", json!(w.world));
            put("name", json!(w.name));
            put("ping", json!(w.ping));
        }
        Element::Ping { seq } | Element::Pong { seq } => put("seq", json!(seq)),
        Element::Bye { reason } => put("reason", json!(reason.as_str())),
        Element::Request { cx, request } => {
            put("cx", json!(cx.to_string()));
            match request {
                Request::PresentTicket { email, ticket } => {
                    put("email", json!(email));
                    put("ticket", json!(ticket));
                }
                Request::FriendAdd { user }
                | Request::FriendDelete { user }
                | Request::IgnoredAdd { user }
                | Request::IgnoredDelete { user } => put("user", json!(user)),
                Request::Flags { op, word, flags } => {
                    put("op", json!(op));
                    put("word", json!(word));
                    put("flags", json!(flags));
                }
                Request::SetLoc { flags, location } => {
                    put("flags", json!(flags));
                    put("location", json!(location));
                }
                Request::SendToUserEx {
                    user,
                    text,
                    extra: x,
                } => {
                    put("user", json!(user));
                    put("text", json!(text));
                    put("extra", extra(x.as_ref()));
                }
                Request::GetFriendsList
                | Request::GetIgnoredList
                | Request::CallCreate
                | Request::SendToRoomEx => {}
            }
        }
        Element::Answer { cx, hr, answer } => {
            put("cx", json!(cx.to_string()));
            put("hr", json!(hr.to_string()));
            match answer {
                Answer::PresentTicket { status } => put("status", json!(status)),
                Answer::GetFriendsList { friends: f } => put("friends", friends(f)),
                Answer::GetIgnoredList { ignored } => put("ignored", json!(ignored)),
                Answer::Flags { which_flag, flags } => {
                    put("which_flag", json!(which_flag));
                    put("flags", json!(flags));
                }
                _ => {}
            }
        }
        Element::Event(Event::FStart { friends: f }) => put("friends", friends(f)),
        Element::Event(Event::FriendStat { user, mode }) => {
            put("user", json!(user));
            put("mode", json!(mode.0));
        }
        Element::Event(Event::UTellEv {
            user,
            text,
            extra: x,
        }) => {
            put("user", json!(user));
            put("text", json!(text));
            put("extra", extra(x.as_ref()));
        }
        Element::Unknown { .. } => put("unknown", json!(true)),
    }
    Value::Object(m)
}

/// The elements in a run of bytes a session wrote, after the preamble if it begins with one.
pub fn elements(mut bytes: &[u8]) -> Vec<Element> {
    if bytes.starts_with(&PREAMBLE) {
        bytes = &bytes[4..];
    }
    let mut r = FrameReader::new();
    r.push(bytes);
    let mut out = Vec::new();
    while let Some(text) = r.next_frame().expect("whole frames") {
        out.push(Element::parse(&text).expect("an element"));
    }
    out
}

/// One element as a frame.
pub fn frame(e: &Element) -> Vec<u8> {
    let mut out = Vec::new();
    e.write_frame(&mut out).expect("fits a frame");
    out
}

/// Everything a client wrote, as elements, and the rest of what it said, in order.
pub fn split_client(outs: Vec<ClientOut>) -> (Vec<Element>, Vec<ClientOut>) {
    let mut written = Vec::new();
    let mut rest = Vec::new();
    for o in outs {
        match o {
            ClientOut::Write(b) => written.extend(elements(&b)),
            o => rest.push(o),
        }
    }
    (written, rest)
}

/// Everything a world wrote, as elements, and the rest of what it said, in order.
pub fn split_world(outs: Vec<ServerOut>) -> (Vec<Element>, Vec<ServerOut>) {
    let mut written = Vec::new();
    let mut rest = Vec::new();
    for o in outs {
        match o {
            ServerOut::Write(b) => written.extend(elements(&b)),
            o => rest.push(o),
        }
    }
    (written, rest)
}

/// A client session.
pub fn client() -> ClientSession {
    ClientSession::new(ClientConfig {
        software: "auberean-friends-tests/1".into(),
        locale: Some(1033),
    })
}

/// The world the tests' streams belong to.
pub fn world_config() -> ServerConfig {
    ServerConfig::new("dawnsong", "Dawnsong")
}

/// A client and its world joined by an in-memory connection, on a clock the test turns.
pub struct Pair {
    pub client: ClientSession,
    pub world: Option<ServerStream>,
    pub now: Duration,
    /// What the client told its host, but for dials, writes and closes.
    pub client_saw: Vec<ClientOut>,
    /// What the world's streams told their host.
    pub world_saw: Vec<ServerEvent>,
    /// How many connections the client opened.
    pub dials: usize,
}

impl Pair {
    pub fn new() -> Self {
        Self {
            client: client(),
            world: None,
            now: Duration::ZERO,
            client_saw: Vec::new(),
            world_saw: Vec::new(),
            dials: 0,
        }
    }

    /// Carry everything both sides have for each other until neither has more.
    pub fn settle(&mut self) {
        loop {
            let mut moved = false;
            for out in self.client.poll(self.now) {
                moved = true;
                match out {
                    ClientOut::Dial => {
                        self.dials += 1;
                        self.world = Some(ServerStream::new(self.now, world_config()));
                        self.client.opened(self.now);
                    }
                    ClientOut::Write(b) => {
                        if let Some(w) = &mut self.world {
                            w.on_bytes(self.now, &b);
                        }
                    }
                    ClientOut::Close => {
                        if let Some(w) = &mut self.world {
                            w.closed(self.now);
                        }
                    }
                    other => self.client_saw.push(other),
                }
            }
            let outs = self.world.as_mut().map(|w| w.poll(self.now));
            for out in outs.unwrap_or_default() {
                moved = true;
                match out {
                    ServerOut::Write(b) => self.client.on_bytes(self.now, &b),
                    ServerOut::Close => self.client.closed(self.now),
                    ServerOut::Event(e) => self.world_saw.push(e),
                }
            }
            if self.world.as_ref().is_some_and(ServerStream::is_ended) {
                // Its last words are carried above; a closed connection carries nothing more.
                if !moved {
                    self.world = None;
                }
            }
            if !moved {
                return;
            }
        }
    }

    /// Turn the clock to `t` and settle.
    pub fn at(&mut self, t: Duration) {
        self.now = t;
        self.settle();
    }

    /// The world's stream.
    pub fn stream(&mut self) -> &mut ServerStream {
        self.world.as_mut().expect("a stream is open")
    }

    /// Take what the client and the world have said so far.
    pub fn take(&mut self) -> (Vec<ClientOut>, Vec<ServerEvent>) {
        (
            std::mem::take(&mut self.client_saw),
            std::mem::take(&mut self.world_saw),
        )
    }
}
