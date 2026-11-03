//! The browser as the shell's host: the page's clock, datagrams carried over the worker's one
//! WebSocket, the page's lock on the pointer, the client's files in browser storage, and the page's
//! audio worklet as the sound device. A browser has no way to open the friends service's stream, so
//! this host does not carry it, and the player is without friends.

use core::net::{IpAddr, SocketAddr};
use std::collections::{BTreeMap, VecDeque};

use auberean_client_sdk::script::local::{bare_name, LocalSystem, LocalTime};
use auberean_shell::Host;
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
        }
    }

    /// A message the WebSocket brought.
    pub fn received(&mut self, message: Vec<u8>) {
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

    fn send(&mut self, to: SocketAddr, bytes: &[u8]) {
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
    fn a_browser_does_not_carry_the_friends_service() {
        assert!(!host().carries_friends());
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
