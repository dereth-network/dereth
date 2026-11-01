//! The client's half of the stream, with no I/O: its host dials, writes and reads, and passes the
//! time in.
//!
//! The session greets the world, gives up one that does not welcome it within 10 seconds, keeps the
//! stream alive, answers its pings, and dials again after a stream ends, waiting 1, 2, 5, 10, then 30
//! seconds between attempts. Requests carry the caller's own [`Cx`], and every request gets exactly
//! one answer: the world's, or the session's own [`Hr::NOT_CONNECTED`] when the stream was down when
//! it was made or went down before the world answered. An answer that matches no request waiting
//! for one (a second answer, or one for a mark never sent) is dropped.
//!
//! The ticket is the caller's to present, as a request like any other, once the stream is up
//! ([`ClientOut::Up`]) and again after each reconnect: the client's own friends layer always did
//! so, and its answer decides the login.

use core::time::Duration;

use crate::element::{
    Answer, ByeReason, Cx, Element, Event, Hello, Hr, Kind, Request, Welcome, PROTO,
};
use crate::frame::{FrameReader, PREAMBLE};
use crate::stream::{secs, Ended, HELLO_DEADLINE, RECONNECT_WAITS, SILENCE, STOOD};

/// What the client says of itself in its greeting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientConfig {
    /// The client's name and version: 1 to 64 printable ASCII characters.
    pub software: String,
    /// The client's Windows locale number, if it has one.
    pub locale: Option<u32>,
}

/// What the session asks of its host, and tells it, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientOut {
    /// Open a connection to the world now, then report [`ClientSession::opened`] or
    /// [`ClientSession::failed`].
    Dial,
    /// Write these bytes to the connection.
    Write(Vec<u8>),
    /// Close the connection, after writing everything before this.
    Close,
    /// The stream is up: the world's greeting. Present the ticket now.
    Up(Welcome),
    /// The stream is down. The session dials again after `retry_in`, unless the caller asked it to
    /// disconnect (`None`).
    Down {
        ended: Ended,
        retry_in: Option<Duration>,
    },
    /// An answer to a request: the world's, or the session's own.
    Answer { cx: Cx, hr: Hr, answer: Answer },
    /// An event from the world.
    Event(Event),
}

#[derive(Debug)]
struct Open {
    opened: Duration,
    last_heard: Duration,
    last_sent: Duration,
    reader: FrameReader,
    welcome: Option<Welcome>,
    /// Requests sent and not yet answered, in the order sent.
    in_flight: Vec<(Cx, Kind)>,
}

#[derive(Debug)]
enum Link {
    Idle,
    Waiting { until: Duration },
    Dialing,
    Open(Open),
}

/// The client's half of one friends stream at a time, across reconnects.
#[derive(Debug)]
pub struct ClientSession {
    config: ClientConfig,
    want: bool,
    link: Link,
    /// The index of the next wait in [`RECONNECT_WAITS`].
    step: usize,
    next_seq: u32,
    out: Vec<ClientOut>,
}

impl ClientSession {
    /// A session that is not connected and does not want to be.
    #[must_use]
    pub fn new(config: ClientConfig) -> Self {
        Self {
            config,
            want: false,
            link: Link::Idle,
            step: 0,
            next_seq: 1,
            out: Vec::new(),
        }
    }

    /// Be connected from now on: dial at once if no wait stands, and dial again whenever the stream
    /// ends. Asking again while connected or waiting changes nothing.
    pub fn connect(&mut self, _now: Duration) {
        self.want = true;
        if matches!(self.link, Link::Idle) {
            self.link = Link::Dialing;
            self.out.push(ClientOut::Dial);
        }
    }

    /// Stop: say goodbye on an open stream, close it, and dial no more.
    pub fn disconnect(&mut self, now: Duration) {
        self.want = false;
        match self.link {
            Link::Open(_) => self.bye(now, ByeReason::Shutdown),
            Link::Dialing => self.end(now, Ended::OwnBye(ByeReason::Shutdown)),
            Link::Waiting { .. } => self.link = Link::Idle,
            Link::Idle => {}
        }
    }

    /// The host's connection is open: the session writes the preamble and its greeting.
    pub fn opened(&mut self, now: Duration) {
        if !matches!(self.link, Link::Dialing) {
            return;
        }
        self.link = Link::Open(Open {
            opened: now,
            last_heard: now,
            last_sent: now,
            reader: FrameReader::new(),
            welcome: None,
            in_flight: Vec::new(),
        });
        self.write_raw(now, &PREAMBLE);
        let hello = Element::Hello(Hello {
            proto: PROTO,
            software: self.config.software.clone(),
            locale: self.config.locale,
        });
        self.send(now, &hello);
    }

    /// The host's attempt to connect failed.
    pub fn failed(&mut self, now: Duration) {
        if matches!(self.link, Link::Dialing) {
            self.end(now, Ended::DialFailed);
        }
    }

    /// The host's connection ended.
    pub fn closed(&mut self, now: Duration) {
        match self.link {
            Link::Open(_) => self.end(now, Ended::Lost),
            Link::Dialing => self.end(now, Ended::DialFailed),
            Link::Idle | Link::Waiting { .. } => {}
        }
    }

    /// Bytes read from the connection.
    pub fn on_bytes(&mut self, now: Duration, bytes: &[u8]) {
        let Link::Open(open) = &mut self.link else {
            return;
        };
        open.last_heard = now;
        open.reader.push(bytes);
        loop {
            let Link::Open(open) = &mut self.link else {
                return;
            };
            match open.reader.next_frame() {
                Ok(None) => return,
                Ok(Some(text)) => match Element::parse(&text) {
                    Ok(element) => self.handle(now, element),
                    Err(_) => return self.bye(now, ByeReason::Protocol),
                },
                Err(_) => return self.bye(now, ByeReason::Protocol),
            }
        }
    }

    fn handle(&mut self, now: Duration, element: Element) {
        let Link::Open(open) = &mut self.link else {
            return;
        };
        match element {
            Element::Welcome(w) if open.welcome.is_none() => {
                if w.proto != PROTO {
                    return self.bye(now, ByeReason::Protocol);
                }
                open.welcome = Some(w.clone());
                self.out.push(ClientOut::Up(w));
            }
            Element::Ping { seq } => self.send(now, &Element::Pong { seq }),
            Element::Bye { reason } => self.end(now, Ended::PeerBye(reason)),
            Element::Answer { cx, hr, answer } => {
                let kind = answer.kind();
                if let Some(i) = open.in_flight.iter().position(|&w| w == (cx, kind)) {
                    open.in_flight.remove(i);
                    self.out.push(ClientOut::Answer { cx, hr, answer });
                }
            }
            Element::Event(e) => self.out.push(ClientOut::Event(e)),
            // A second greeting, a pong, a request or an element this version does not know.
            _ => {}
        }
    }

    /// Send a request with the caller's mark. Its answer comes out of [`ClientSession::poll`]: the
    /// world's, or [`Hr::NOT_CONNECTED`] while the stream is not up, or
    /// [`Hr::INVALID_ARGUMENT`] for one holding a value out of its bounds.
    pub fn request(&mut self, now: Duration, cx: Cx, request: Request) {
        let kind = request.kind();
        let element = Element::Request { cx, request };
        let local = |hr| ClientOut::Answer {
            cx,
            hr,
            answer: Answer::empty(kind),
        };
        if element.out_of_bounds().is_some() {
            return self.out.push(local(Hr::INVALID_ARGUMENT));
        }
        let up = matches!(&self.link, Link::Open(o) if o.welcome.is_some());
        if !up {
            return self.out.push(local(Hr::NOT_CONNECTED));
        }
        let mut bytes = Vec::new();
        if element.write_frame(&mut bytes).is_err() {
            return self.out.push(local(Hr::INVALID_ARGUMENT));
        }
        self.write_raw(now, &bytes);
        if let Link::Open(open) = &mut self.link {
            open.in_flight.push((cx, kind));
        }
    }

    /// Run the clock: dial when a wait is over, ping a quiet stream, give up a silent one. Then
    /// everything the session has for its host, in order.
    pub fn poll(&mut self, now: Duration) -> Vec<ClientOut> {
        match &self.link {
            Link::Waiting { until } if now >= *until => {
                self.link = Link::Dialing;
                self.out.push(ClientOut::Dial);
            }
            Link::Open(open) => {
                let ping = open.welcome.as_ref().map(|w| secs(w.ping));
                let unwelcomed =
                    ping.is_none() && now.saturating_sub(open.opened) >= HELLO_DEADLINE;
                if unwelcomed || now.saturating_sub(open.last_heard) >= SILENCE {
                    self.bye(now, ByeReason::Idle);
                } else if ping.is_some_and(|p| now.saturating_sub(open.last_sent) >= p) {
                    let seq = self.next_seq;
                    self.next_seq = self.next_seq.wrapping_add(1);
                    self.send(now, &Element::Ping { seq });
                }
            }
            _ => {}
        }
        core::mem::take(&mut self.out)
    }

    /// When the session next needs [`ClientSession::poll`] if nothing arrives first.
    #[must_use]
    pub fn next_wake(&self) -> Option<Duration> {
        match &self.link {
            Link::Waiting { until } => Some(*until),
            Link::Open(open) => {
                let silent = open.last_heard + SILENCE;
                Some(match &open.welcome {
                    Some(w) => silent.min(open.last_sent + secs(w.ping)),
                    None => silent.min(open.opened + HELLO_DEADLINE),
                })
            }
            Link::Idle | Link::Dialing => None,
        }
    }

    /// The world's greeting while the stream is up.
    #[must_use]
    pub fn welcome(&self) -> Option<&Welcome> {
        match &self.link {
            Link::Open(open) => open.welcome.as_ref(),
            _ => None,
        }
    }

    /// Whether the stream is up: open and greeted.
    #[must_use]
    pub fn is_up(&self) -> bool {
        self.welcome().is_some()
    }

    fn write_raw(&mut self, now: Duration, bytes: &[u8]) {
        if let Link::Open(open) = &mut self.link {
            open.last_sent = now;
        }
        match self.out.last_mut() {
            Some(ClientOut::Write(buf)) => buf.extend_from_slice(bytes),
            _ => self.out.push(ClientOut::Write(bytes.to_vec())),
        }
    }

    /// Send one of the session's own small elements, which always fit a frame.
    fn send(&mut self, now: Duration, element: &Element) {
        let mut bytes = Vec::new();
        if element.write_frame(&mut bytes).is_ok() {
            self.write_raw(now, &bytes);
        }
    }

    fn bye(&mut self, now: Duration, reason: ByeReason) {
        self.send(now, &Element::Bye { reason });
        self.end(now, Ended::OwnBye(reason));
    }

    /// The stream is over: close it, answer what it owed, and set the next wait.
    fn end(&mut self, now: Duration, ended: Ended) {
        match core::mem::replace(&mut self.link, Link::Idle) {
            Link::Open(open) => {
                self.out.push(ClientOut::Close);
                for (cx, kind) in open.in_flight {
                    self.out.push(ClientOut::Answer {
                        cx,
                        hr: Hr::NOT_CONNECTED,
                        answer: Answer::empty(kind),
                    });
                }
                if now.saturating_sub(open.opened) >= STOOD {
                    self.step = 0;
                }
            }
            Link::Dialing => self.out.push(ClientOut::Close),
            Link::Idle | Link::Waiting { .. } => {}
        }
        let retry_in = self.want.then(|| {
            let last = RECONNECT_WAITS.len() - 1;
            let wait = if ended == Ended::PeerBye(ByeReason::Replaced) {
                RECONNECT_WAITS[last]
            } else {
                RECONNECT_WAITS[self.step.min(last)]
            };
            self.step = (self.step + 1).min(last);
            wait
        });
        if let Some(wait) = retry_in {
            self.link = Link::Waiting { until: now + wait };
        }
        self.out.push(ClientOut::Down { ended, retry_in });
    }
}
