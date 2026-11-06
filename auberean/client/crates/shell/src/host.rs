//! The host: what a desktop window (or later a browser page) gives the shell. The shell is generic
//! over it and never names a platform; a test names [`TestHost`].

use core::net::SocketAddr;
use std::collections::VecDeque;

use auberean_client_sdk::script::local::LocalSystem;

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
    /// Open a web page in the player's browser.
    ///
    /// # Errors
    /// The platform's error code when it could not (a host with no browser answers 0).
    fn open_page(&mut self, url: &str) -> Result<(), i32> {
        let _ = url;
        Err(0)
    }
    /// Show a box of text the player closes, outside the game's own screens.
    fn error_box(&mut self, title: &str, text: &str) {
        let _ = (title, text);
    }
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
    /// The sound device, when the host has one: what the game's sound plays through.
    fn sound_output(&mut self) -> Option<&mut dyn dereth_audio::output::AudioOutput> {
        None
    }
    /// The next thing that happened on the friends service's stream.
    fn friends_news(&mut self) -> Option<FriendsNews> {
        None
    }
    /// The machine's wall clock and the player's files kept with the settings, for the game's
    /// scripts (a chat log, an options export); none when the host has neither.
    fn local_system(&self) -> Option<Box<dyn LocalSystem + Send>> {
        None
    }
    /// Show the game on the screen this way.
    fn set_presentation(&mut self, presentation: &crate::settings::Presentation) {
        let _ = presentation;
    }
    /// Where the platform's input method writes, in the window's pixels, while an entry that
    /// allows it takes the keyboard; none turns it off.
    fn set_input_method(&mut self, area: Option<InputArea>) {
        let _ = area;
    }
}

/// The part of the window the input method writes over: its top left corner and its size, in
/// the window's pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputArea {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
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

/// A sound device for tests: two channels at 48,000 frames a second, whose blocks a test pulls by
/// hand.
#[derive(Clone, Default)]
pub struct TestSound {
    #[allow(clippy::type_complexity)] // the stream's own callback, kept to be pulled
    fill: std::sync::Arc<std::sync::Mutex<Option<Box<dyn FnMut(&mut [f32]) + Send>>>>,
}

impl std::fmt::Debug for TestSound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestSound").finish_non_exhaustive()
    }
}

impl TestSound {
    /// The device's rate.
    pub const RATE: u32 = 48_000;

    /// The next `frames` frames the device plays, two channels interleaved (silence before the
    /// stream starts).
    #[must_use]
    pub fn pull(&self, frames: usize) -> Vec<f32> {
        let mut out = vec![0.0; frames * 2];
        if let Ok(mut f) = self.fill.lock() {
            if let Some(fill) = f.as_mut() {
                fill(&mut out);
            }
        }
        out
    }
}

impl dereth_audio::output::AudioOutput for TestSound {
    fn probe(&mut self) -> Result<(u32, u16), String> {
        Ok((Self::RATE, 2))
    }

    fn start(
        &mut self,
        fill: Box<dyn FnMut(&mut [f32]) + Send>,
    ) -> Result<Box<dyn std::any::Any>, String> {
        if let Ok(mut f) = self.fill.lock() {
            *f = Some(fill);
        }
        Ok(Box::new(()))
    }
}

/// A host for tests: a clock set by hand, datagrams kept in queues, settings and files in memory,
/// and a sound device when given one.
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
    /// The sound device, when the test gives it one.
    pub sound: Option<TestSound>,
    /// Where the input method writes, as last told.
    pub input_method: Option<InputArea>,
    /// The web pages asked for.
    pub pages: Vec<String>,
    /// The error code opening a page fails with; none opens it.
    pub page_error: Option<i32>,
    /// The boxes of text shown: the title and the text.
    pub boxes: Vec<(String, String)>,
    /// How the game was asked to be shown, each time.
    pub presentations: Vec<crate::settings::Presentation>,
}

impl Host for TestHost {
    fn set_presentation(&mut self, presentation: &crate::settings::Presentation) {
        self.presentations.push(*presentation);
    }
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
    fn sound_output(&mut self) -> Option<&mut dyn dereth_audio::output::AudioOutput> {
        self.sound
            .as_mut()
            .map(|s| s as &mut dyn dereth_audio::output::AudioOutput)
    }
    fn set_input_method(&mut self, area: Option<InputArea>) {
        self.input_method = area;
    }
    fn open_page(&mut self, url: &str) -> Result<(), i32> {
        self.pages.push(url.to_owned());
        self.page_error.map_or(Ok(()), Err)
    }
    fn error_box(&mut self, title: &str, text: &str) {
        self.boxes.push((title.to_owned(), text.to_owned()));
    }
}
