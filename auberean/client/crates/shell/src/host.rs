//! The host: what a desktop window (or later a browser page) gives the shell. The shell is generic
//! over it and never names a platform; a test names [`TestHost`].

use core::net::SocketAddr;
use std::collections::VecDeque;

/// What the shell needs from the platform under it.
pub trait Host {
    /// Seconds since the program started, never going back.
    fn now(&self) -> f64;
    /// Seconds since 1970, for the login's time.
    fn unix_time(&self) -> u32;
    /// Fill `out` with unpredictable bytes (the session key).
    fn random(&mut self, out: &mut [u8]);
    /// Get ready to talk to the server at `server`.
    ///
    /// # Errors
    /// When the host cannot reach it (no socket, an address it refuses).
    fn connect(&mut self, server: SocketAddr) -> Result<(), String>;
    /// Send a datagram.
    fn send(&mut self, to: SocketAddr, bytes: &[u8]);
    /// The next datagram that arrived, and from where.
    fn receive(&mut self) -> Option<(SocketAddr, Vec<u8>)>;
    /// Hide the pointer and hold it in the window (mouse look), or give it back.
    fn hold_pointer(&mut self, held: bool);
    /// Show a line of text where the platform shows the window's name.
    fn set_title(&mut self, title: &str);
    /// The saved settings' text, if any.
    fn load_settings(&self) -> Option<String>;
    /// Save the settings' text.
    fn save_settings(&mut self, text: &str);
    /// One of the player's files kept with the settings (a key map), by name: the one saved, else
    /// the game's own beside its data files. None when there is neither.
    fn load_file(&self, name: &str) -> Option<Vec<u8>> {
        let _ = name;
        None
    }
    /// Save one of the player's files with the settings; whether it was saved.
    fn save_file(&mut self, name: &str, bytes: &[u8]) -> bool {
        let _ = (name, bytes);
        false
    }
    /// Whether this host carries the friends service's stream. One that does not leaves the player
    /// without friends.
    fn carries_friends(&self) -> bool {
        false
    }
    /// Open the friends service's stream to `host` and `port` (closing one open before); how it went
    /// comes back from [`Host::friends_news`].
    fn friends_dial(&mut self, host: &str, port: u16) {
        let _ = (host, port);
    }
    /// Write to the friends service's stream.
    fn friends_send(&mut self, bytes: &[u8]) {
        let _ = bytes;
    }
    /// Close the friends service's stream.
    fn friends_close(&mut self) {}
    /// The next thing that happened on the friends service's stream.
    fn friends_news(&mut self) -> Option<FriendsNews> {
        None
    }
}

/// What happened on the friends service's stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FriendsNews {
    /// It opened.
    Opened,
    /// It could not be opened.
    Failed,
    /// It closed.
    Closed,
    /// Bytes read from it.
    Bytes(Vec<u8>),
}

/// The key maps' files, kept by a host.
#[derive(Debug)]
pub struct HostFiles<'a, H: Host>(pub &'a mut H);

impl<H: Host> crate::keys::KeyFileStore for HostFiles<'_, H> {
    fn load(&self, name: &str) -> Option<Vec<u8>> {
        self.0.load_file(name)
    }
    fn save(&mut self, name: &str, bytes: &[u8]) -> bool {
        self.0.save_file(name, bytes)
    }
}

/// A host for tests: a clock set by hand, datagrams kept in queues, settings and files in memory.
#[derive(Clone, Debug, Default)]
pub struct TestHost {
    pub time: f64,
    pub unix: u32,
    pub connected: Option<SocketAddr>,
    pub sent: Vec<(SocketAddr, Vec<u8>)>,
    pub inbox: VecDeque<(SocketAddr, Vec<u8>)>,
    pub pointer_held: bool,
    pub title: String,
    pub settings: Option<String>,
    /// The player's files, by name.
    pub files: std::collections::BTreeMap<String, Vec<u8>>,
}

impl Host for TestHost {
    fn now(&self) -> f64 {
        self.time
    }
    fn unix_time(&self) -> u32 {
        self.unix
    }
    fn random(&mut self, out: &mut [u8]) {
        for (i, b) in out.iter_mut().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            {
                *b = (i as u8).wrapping_mul(37).wrapping_add(11);
            }
        }
    }
    fn connect(&mut self, server: SocketAddr) -> Result<(), String> {
        self.connected = Some(server);
        Ok(())
    }
    fn send(&mut self, to: SocketAddr, bytes: &[u8]) {
        self.sent.push((to, bytes.to_vec()));
    }
    fn receive(&mut self) -> Option<(SocketAddr, Vec<u8>)> {
        self.inbox.pop_front()
    }
    fn hold_pointer(&mut self, held: bool) {
        self.pointer_held = held;
    }
    fn set_title(&mut self, title: &str) {
        title.clone_into(&mut self.title);
    }
    fn load_settings(&self) -> Option<String> {
        self.settings.clone()
    }
    fn save_settings(&mut self, text: &str) {
        self.settings = Some(text.to_owned());
    }
    fn load_file(&self, name: &str) -> Option<Vec<u8>> {
        self.files.get(name).cloned()
    }
    fn save_file(&mut self, name: &str, bytes: &[u8]) -> bool {
        self.files.insert(name.to_owned(), bytes.to_vec());
        true
    }
}
