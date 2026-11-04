//! The browser as the shell's host: the page's clock, datagrams carried over the worker's one
//! WebSocket, the page's lock on the pointer, the client's files in browser storage, and the page's
//! audio worklet as the sound device. A browser can open no connection of the friends service's
//! own, so its stream rides the game's WebSocket by a port of its own, where a ticket may ride it.

use core::net::{IpAddr, SocketAddr};
use std::collections::{BTreeMap, VecDeque};

use auberean_client_sdk::script::local::{bare_name, LocalSystem, LocalTime};
use auberean_shell::{FriendsNews, Host};
use dereth_transport::web_frame;

use crate::store;

/// What the worker's browser answers for the host.
pub trait Browser {
    /// Seconds since the page started, never going back.
    fn now(&self) -> f64;
    /// Seconds since 1970.
    fn unix_time(&self) -> u32;
    /// Fill `out` with unpredictable bytes.
    fn random(&self, out: &mut [u8]);
}

/// The machine's clock, for the game's scripts: the local date and time, and seconds since 1970.
#[derive(Clone, Copy, Debug)]
pub struct LocalClock {
    pub local: fn() -> Option<LocalTime>,
    pub real: fn() -> Option<i64>,
}

/// The settings file among the client's files.
pub const SETTINGS_FILE: &str = "Preferences.ini";

/// The sound device the page plays through: two channels at this rate, which the page's audio
/// context is opened at.
pub const OUTPUT_RATE: u32 = 48_000;

type Fill = Box<dyn FnMut(&mut [f32]) + Send>;

thread_local! {
    static FILL: std::cell::RefCell<Option<Fill>> = const { std::cell::RefCell::new(None) };
}

/// The page's audio output: the worker pulls what the page's audio clock has used.
#[derive(Debug, Default)]
pub struct PageSound;

impl dereth_audio::output::AudioOutput for PageSound {
    fn probe(&mut self) -> Result<(u32, u16), String> {
        Ok((OUTPUT_RATE, 2))
    }

    fn start(&mut self, fill: Fill) -> Result<Box<dyn std::any::Any>, String> {
        FILL.with(|f| *f.borrow_mut() = Some(fill));
        Ok(Box::new(()))
    }
}

/// The next `frames` sample frames of the mix, interleaved stereo; empty before the client has
/// started its output.
#[must_use]
pub fn pull_sound(frames: usize) -> Vec<f32> {
    FILL.with(|f| {
        let mut f = f.borrow_mut();
        let Some(fill) = f.as_mut() else {
            return Vec::new();
        };
        let mut out = vec![0.0; frames * 2];
        fill(&mut out);
        out
    })
}

/// The browser as the shell's host.
#[derive(Debug)]
pub struct PageHost<B> {
    browser: B,
    clock: LocalClock,
    sound: PageSound,
    /// The server as the client numbers it; the WebSocket's far end alone knows where it is.
    server: SocketAddr,
    /// The address the client last sent to on each pair of a server's ports: a referral names the
    /// world service's, and its answers must come back from there.
    peers: BTreeMap<u16, IpAddr>,
    inbox: VecDeque<Vec<u8>>,
    outbox: Vec<Vec<u8>>,
    /// Whether the client holds the pointer, as last told, for the page to lock it.
    held: bool,
    /// Whether the WebSocket may carry the friends service's stream, whose ticket it would carry.
    friends_allowed: bool,
    /// The friends stream while it is open: when it opened, until its world first answers.
    friends_open: Option<Option<f64>>,
    friends_news: VecDeque<FriendsNews>,
    /// A message has opened the connection's numbering of its ports.
    numbered: bool,
}

impl<B: Browser> PageHost<B> {
    /// A host over `browser` and its `clock`, whose client numbers its server `server`.
    pub fn new(browser: B, clock: LocalClock, server: SocketAddr) -> Self {
        Self {
            browser,
            clock,
            sound: PageSound,
            server,
            peers: BTreeMap::new(),
            inbox: VecDeque::new(),
            outbox: Vec::new(),
            held: false,
            friends_allowed: false,
            friends_open: None,
            friends_news: VecDeque::new(),
            numbered: false,
        }
    }

    /// Let the WebSocket carry the friends service's stream: it is one a ticket may ride.
    #[must_use]
    pub const fn with_friends(mut self) -> Self {
        self.friends_allowed = true;
        self
    }

    /// The port the friends stream goes by, in the client's numbering.
    fn friends_port(&self) -> u16 {
        self.server.port() + web_frame::FRIENDS_STREAM
    }

    /// A message the WebSocket brought.
    pub fn received(&mut self, message: Vec<u8>) {
        if let Some((port, bytes)) = web_frame::decode(&message) {
            if port == self.friends_port() {
                if self.friends_open.is_some() {
                    if bytes.is_empty() {
                        self.friends_open = None;
                        self.friends_news.push_back(FriendsNews::Closed);
                    } else {
                        self.friends_open = Some(None);
                        self.friends_news
                            .push_back(FriendsNews::Bytes(bytes.to_vec()));
                    }
                }
                return;
            }
        }
        self.inbox.push_back(message);
    }

    /// The messages for the WebSocket since the last call, each one datagram.
    pub fn take_messages(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.outbox)
    }

    /// Whether the client holds the pointer.
    #[must_use]
    pub fn pointer_held(&self) -> bool {
        self.held
    }
}

/// How long a world has to answer on the friends stream before the client takes it as not served,
/// seconds.
const FRIENDS_ANSWER: f64 = 3.0;

/// The lower of the pair of ports `port` is one of.
const fn pair_of(port: u16) -> u16 {
    port & !1
}

impl<B: Browser> Host for PageHost<B> {
    fn now(&self) -> f64 {
        self.browser.now()
    }

    fn unix_time(&self) -> u32 {
        self.browser.unix_time()
    }

    fn random(&mut self, out: &mut [u8]) {
        self.browser.random(out);
    }

    /// The WebSocket is already there; the worker opened it before the client's first frame.
    fn connect(&mut self, _server: SocketAddr) -> Result<(), String> {
        Ok(())
    }

    fn carries_friends(&self) -> bool {
        self.friends_allowed
    }

    /// The stream rides the WebSocket, already there. The connection numbers its ports from its
    /// first message, so that opens with the login's port even as the stream goes first; a server
    /// takes the empty datagram for nothing.
    fn friends_dial(&mut self, _host: &str, _port: u16) {
        if !self.numbered {
            self.numbered = true;
            self.outbox.push(web_frame::encode(self.server.port(), &[]));
        }
        self.friends_open = Some(Some(self.browser.now()));
        self.friends_news.push_back(FriendsNews::Opened);
    }

    fn friends_send(&mut self, bytes: &[u8]) {
        if self.friends_open.is_some() {
            self.outbox
                .push(web_frame::encode(self.friends_port(), bytes));
        }
    }

    fn friends_close(&mut self) {
        if self.friends_open.take().is_some() {
            self.outbox
                .push(web_frame::encode(self.friends_port(), &[]));
        }
    }

    /// A server that does not carry the stream drops it unanswered, as a relay to a server's UDP
    /// ports does: the login waits on the stream no longer than [`FRIENDS_ANSWER`].
    fn friends_news(&mut self) -> Option<FriendsNews> {
        if let Some(Some(opened)) = self.friends_open {
            if self.browser.now() - opened > FRIENDS_ANSWER {
                self.friends_open = None;
                self.friends_news.push_back(FriendsNews::Failed);
            }
        }
        self.friends_news.pop_front()
    }

    fn send(&mut self, to: SocketAddr, bytes: &[u8]) {
        self.numbered = true;
        self.peers.insert(pair_of(to.port()), to.ip());
        self.outbox.push(web_frame::encode(to.port(), bytes));
    }

    fn receive(&mut self) -> Option<(SocketAddr, Vec<u8>)> {
        while let Some(message) = self.inbox.pop_front() {
            if let Some((port, datagram)) = web_frame::decode(&message) {
                let ip = self
                    .peers
                    .get(&pair_of(port))
                    .copied()
                    .unwrap_or_else(|| self.server.ip());
                return Some((SocketAddr::new(ip, port), datagram.to_vec()));
            }
        }
        None
    }

    fn hold_pointer(&mut self, held: bool) {
        self.held = held;
    }

    /// The page's title stays the page's.
    fn set_title(&mut self, _title: &str) {}

    fn load_settings(&self) -> Option<String> {
        store::read(SETTINGS_FILE).and_then(|b| String::from_utf8(b).ok())
    }

    fn save_settings(&mut self, text: &str) {
        store::write(SETTINGS_FILE, text.as_bytes());
    }

    fn load_file(&self, name: &str) -> Option<Vec<u8>> {
        store::read(name)
    }

    fn save_file(&mut self, name: &str, bytes: &[u8]) -> bool {
        store::write(name, bytes);
        true
    }

    fn sound_output(&mut self) -> Option<&mut dyn dereth_audio::output::AudioOutput> {
        Some(&mut self.sound)
    }

    fn local_system(&self) -> Option<Box<dyn LocalSystem + Send>> {
        Some(Box::new(PageFiles { clock: self.clock }))
    }
}

/// The machine's clock and the player's files for the game's scripts: the page's clock, and files
/// kept among the client's own, by bare names only.
#[derive(Debug)]
struct PageFiles {
    clock: LocalClock,
}

impl LocalSystem for PageFiles {
    fn local_time(&self) -> Option<LocalTime> {
        (self.clock.local)()
    }

    fn real_time(&self) -> Option<i64> {
        (self.clock.real)()
    }

    fn append_line(&mut self, file: &str, bytes: &[u8]) -> bool {
        bare_name(file) && {
            store::append(file, bytes);
            true
        }
    }

    fn write_file(&mut self, file: &str, bytes: &[u8]) -> bool {
        bare_name(file) && {
            store::write(file, bytes);
            true
        }
    }

    fn read_file(&mut self, file: &str) -> Option<Vec<u8>> {
        bare_name(file).then(|| store::read(file)).flatten()
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (a browser as the shell's host, over one WebSocket: no retail behavior)
    use super::*;

    #[derive(Debug)]
    struct Fake;

    impl Browser for Fake {
        fn now(&self) -> f64 {
            1.5
        }
        fn unix_time(&self) -> u32 {
            7
        }
        fn random(&self, out: &mut [u8]) {
            out.fill(9);
        }
    }

    const CLOCK: LocalClock = LocalClock {
        local: || None,
        real: || Some(11),
    };

    fn host() -> PageHost<Fake> {
        PageHost::new(Fake, CLOCK, SocketAddr::from(([127, 0, 0, 1], 9000)))
    }

    #[test]
    fn a_datagram_goes_out_as_one_message_behind_the_port_the_client_sent_it_to() {
        let mut h = host();
        h.send(SocketAddr::from(([127, 0, 0, 1], 9000)), b"login");
        h.send(SocketAddr::from(([192, 0, 2, 7], 9002)), b"world");
        assert_eq!(
            h.take_messages(),
            vec![
                web_frame::encode(9000, b"login"),
                web_frame::encode(9002, b"world")
            ]
        );
        assert!(h.take_messages().is_empty());
    }

    /// A referral names the world service at an address that means nothing on a connection; its
    /// answers must seem to come from there, or the client would not know them.
    #[test]
    fn an_answer_comes_from_the_address_the_client_sent_that_pair_of_ports_to() {
        let mut h = host();
        h.received(web_frame::encode(9001, b"from the login"));
        h.send(SocketAddr::from(([192, 0, 2, 7], 9002)), b"world");
        h.received(web_frame::encode(9003, b"from the world"));
        assert_eq!(
            h.receive(),
            Some((
                SocketAddr::from(([127, 0, 0, 1], 9001)),
                b"from the login".to_vec()
            ))
        );
        assert_eq!(
            h.receive(),
            Some((
                SocketAddr::from(([192, 0, 2, 7], 9003)),
                b"from the world".to_vec()
            ))
        );
        assert_eq!(h.receive(), None);
    }

    #[test]
    fn a_message_too_short_to_be_a_frame_is_passed_over() {
        let mut h = host();
        h.received(vec![0]);
        h.received(web_frame::encode(9000, b"after"));
        assert_eq!(h.receive().map(|(_, d)| d), Some(b"after".to_vec()));
    }

    #[test]
    fn the_friends_stream_rides_the_websocket_only_where_a_ticket_may() {
        assert!(!host().carries_friends());
        assert!(host().with_friends().carries_friends());
    }

    #[test]
    fn the_friends_stream_opens_the_numbering_and_goes_by_its_own_port() {
        let mut h = host().with_friends();
        h.friends_dial("ignored", 1);
        assert_eq!(h.friends_news(), Some(FriendsNews::Opened));
        h.friends_send(b"hello");
        assert_eq!(
            h.take_messages(),
            vec![
                web_frame::encode(9000, &[]),
                web_frame::encode(9004, b"hello")
            ],
            "the login's port first, though the stream went first"
        );
        h.received(web_frame::encode(9004, b"welcome"));
        h.received(web_frame::encode(9000, b"game"));
        assert_eq!(
            h.friends_news(),
            Some(FriendsNews::Bytes(b"welcome".to_vec()))
        );
        assert_eq!(h.receive().map(|(_, d)| d), Some(b"game".to_vec()));
        h.received(web_frame::encode(9004, &[]));
        assert_eq!(h.friends_news(), Some(FriendsNews::Closed));
        h.friends_send(b"late");
        assert!(h.take_messages().is_empty());
    }

    #[test]
    fn a_friends_stream_ending_says_so_and_a_login_first_opens_the_numbering_itself() {
        let mut h = host().with_friends();
        h.send(SocketAddr::from(([127, 0, 0, 1], 9000)), b"login");
        h.friends_dial("ignored", 1);
        h.friends_close();
        assert_eq!(
            h.take_messages(),
            vec![
                web_frame::encode(9000, b"login"),
                web_frame::encode(9004, &[])
            ]
        );
    }

    #[test]
    fn the_client_s_settings_and_files_are_kept_in_the_store_and_a_script_keeps_bare_names_only() {
        store::install(&[], None);
        let mut h = host();
        h.save_settings("[Display]");
        assert_eq!(h.load_settings().as_deref(), Some("[Display]"));
        assert!(h.save_file("keys.map", b"map"));
        assert_eq!(h.load_file("keys.map").as_deref(), Some(&b"map"[..]));
        let mut local = h.local_system().expect("a browser has a clock and files");
        assert_eq!(local.real_time(), Some(11));
        assert!(local.append_line("chat.txt", b"one"));
        assert_eq!(local.read_file("chat.txt").as_deref(), Some(&b"one"[..]));
        assert!(!local.write_file("../chat.txt", b"out"));
        assert_eq!(local.read_file("../chat.txt"), None);
    }

    #[test]
    fn nothing_is_pulled_before_the_client_starts_its_sound_and_then_its_mix_is() {
        assert!(pull_sound(4).is_empty());
        let mut h = host();
        let output = h.sound_output().expect("a page has a sound device");
        assert_eq!(output.probe(), Ok((OUTPUT_RATE, 2)));
        drop(output.start(Box::new(|block: &mut [f32]| block.fill(0.25))));
        assert_eq!(pull_sound(4), vec![0.25; 8]);
    }
}
