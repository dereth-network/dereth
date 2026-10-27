//! Behaviour: none (a host inside another program: its C interface driven as the Decal plugin drives it)
//!
//! The C interface as the Decal plugin uses it ([`Dll`]); the views and marks its ops describe,
//! kept as Decal would keep them ([`Surface`]); and a recorded session's messages handed over at
//! their own pace, with frames at the rate Dereth's conformance driver runs them ([`Replay`]).
#![allow(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use dereth_client_sdk::net::client_session::testing::{Corpus, CorpusBlob, Direction};
use dereth_client_sdk::plugin_api::Value;
use dereth_client_sdk::primitives::{LocalTime, Transport as _};
use dereth_decal_host::ffi;

/// Frames a second, as Dereth's conformance driver runs them.
pub const FRAME_RATE: f64 = 30.0;

/// The folder of this crate's test plugins.
#[must_use]
pub fn plugin_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/plugins")
        .join(name)
}

/// Settings for a run: the plugin folders given, every entry unhurried and no time counted against
/// a plugin's share of a frame (so nothing turns on how fast the machine is), frames only when the
/// run asks, and the host named as Dereth's conformance driver names its own.
#[must_use]
pub fn settings(plugins: &[PathBuf]) -> Value {
    Value::map()
        .with(
            "plugins",
            plugins
                .iter()
                .map(|p| Value::from(p.to_string_lossy().into_owned()))
                .collect::<Vec<_>>(),
        )
        .with("untimed", true)
        .with("entry_budget_ms", 10_000.0)
        .with("frames_when_asked", true)
        .with("name", "dereth-conformance")
}

/// A host behind the C interface.
#[derive(Debug)]
pub struct Dll {
    h: *mut ffi::DerethHost,
}

/// The bytes the interface handed back, as JSON.
fn read_out(p: *const u8, len: usize) -> Value {
    assert!(!p.is_null(), "the interface answers");
    // SAFETY: the interface hands back `len` bytes at `p`, valid until the next call into it.
    let bytes = unsafe { std::slice::from_raw_parts(p, len) };
    Value::from_json(std::str::from_utf8(bytes).expect("UTF-8")).expect("JSON")
}

impl Dll {
    /// A host made from `settings`.
    ///
    /// # Panics
    /// When the interface makes none.
    #[must_use]
    pub fn new(settings: &Value) -> Self {
        let text = settings.to_json();
        // SAFETY: the settings' bytes, read during the call.
        let h = unsafe { ffi::dereth_host_new(text.as_ptr(), text.len()) };
        assert!(!h.is_null(), "the host is made from {text}");
        Self { h }
    }

    pub fn server(&mut self, blob: &[u8], now: f64) {
        // SAFETY: a live host and the blob's bytes.
        unsafe { ffi::dereth_host_server_message(self.h, blob.as_ptr(), blob.len(), now) }
    }

    pub fn client(&mut self, blob: &[u8], now: f64) {
        // SAFETY: a live host and the blob's bytes.
        unsafe { ffi::dereth_host_client_message(self.h, blob.as_ptr(), blob.len(), now) }
    }

    pub fn frame(&mut self, now: f64) {
        // SAFETY: a live host.
        unsafe { ffi::dereth_host_frame(self.h, now) }
    }

    pub fn flush(&mut self) {
        // SAFETY: a live host.
        unsafe { ffi::dereth_host_flush(self.h) }
    }

    pub fn place(&mut self, landcell: u32, x: f64, y: f64, z: f64, heading: f64) {
        // SAFETY: a live host.
        unsafe { ffi::dereth_host_place(self.h, landcell, x, y, z, heading) }
    }

    pub fn player_moved(&mut self) {
        // SAFETY: a live host.
        unsafe { ffi::dereth_host_player_moved(self.h) }
    }

    /// The object selected in the game, as Decal says (0 for none).
    pub fn selected(&mut self, id: u32) {
        // SAFETY: a live host.
        unsafe { ffi::dereth_host_selected(self.h, id) }
    }

    /// Whether the game's window is in front and its chat line open, as Decal says.
    pub fn game_state(&mut self, focused: bool, chat_open: bool) {
        // SAFETY: a live host.
        unsafe {
            ffi::dereth_host_game_state(self.h, i32::from(focused), i32::from(chat_open));
        }
    }

    /// The player's key bindings, as Decal reads them.
    pub fn bindings(&mut self, bindings: &Value) {
        let text = bindings.to_json();
        // SAFETY: a live host and the bindings' bytes.
        unsafe { ffi::dereth_host_bindings(self.h, text.as_ptr(), text.len()) }
    }

    /// Whether a plugin claims the typed line.
    pub fn typed(&mut self, line: &str) -> bool {
        // SAFETY: a live host and the line's bytes.
        unsafe { ffi::dereth_host_typed(self.h, line.as_ptr(), line.len()) == 1 }
    }

    pub fn view_event(&mut self, key: &str, control: &str, event: &str, value: Value) {
        let text = Value::map()
            .with("key", key)
            .with("control", control)
            .with("event", event)
            .with("value", value)
            .to_json();
        // SAFETY: a live host and the event's bytes.
        unsafe { ffi::dereth_host_view_event(self.h, text.as_ptr(), text.len()) }
    }

    /// The ops since the last call.
    pub fn ops(&mut self) -> Vec<Value> {
        let mut len = 0;
        // SAFETY: a live host and a writable length.
        let p = unsafe { ffi::dereth_host_ops(self.h, &mut len) };
        read_out(p, len).as_list().expect("a list of ops").to_vec()
    }

    pub fn report(&mut self) -> Value {
        let mut len = 0;
        // SAFETY: a live host and a writable length.
        let p = unsafe { ffi::dereth_host_report(self.h, &mut len) };
        read_out(p, len)
    }
}

impl Drop for Dll {
    fn drop(&mut self) {
        // SAFETY: the host this wrapper made, freed once.
        unsafe { ffi::dereth_host_free(self.h) }
    }
}

/// Decal's side of the ops: each view's controls and whether it shows, the marks, the lines
/// printed and hidden, and every other op in order.
#[derive(Debug, Default)]
pub struct Surface {
    pub views: BTreeMap<String, (bool, BTreeMap<String, Value>)>,
    pub marks: BTreeMap<String, Value>,
    pub printed: Vec<String>,
    pub hidden: Vec<String>,
    /// Every op but the views', the marks' and the lines', in order.
    pub calls: Vec<Value>,
}

impl Surface {
    pub fn apply(&mut self, ops: &[Value]) {
        for op in ops {
            let s = |k: &str| op.get(k).as_str().unwrap_or_default().to_owned();
            let controls = |v: &Value| -> Vec<(String, Value)> {
                v.get("controls")
                    .as_list()
                    .unwrap_or_default()
                    .iter()
                    .map(|c| {
                        (
                            c.get("name").as_str().unwrap_or_default().to_owned(),
                            c.clone(),
                        )
                    })
                    .collect()
            };
            match op.get("op").as_str().unwrap_or_default() {
                "view" => {
                    let open = op.get("open").as_bool().unwrap_or(false);
                    self.views
                        .insert(s("key"), (open, controls(op).into_iter().collect()));
                }
                "update" => {
                    if let Some(v) = self.views.get_mut(&s("key")) {
                        v.1.extend(controls(op));
                    }
                }
                "show" => {
                    if let Some(v) = self.views.get_mut(&s("key")) {
                        v.0 = op.get("open").as_bool().unwrap_or(false);
                    }
                }
                "dispose" => {
                    self.views.remove(&s("key"));
                }
                "mark" => {
                    self.marks.insert(s("key"), op.clone());
                }
                "unmark" => {
                    self.marks.remove(&s("key"));
                }
                "print" => self.printed.push(s("text")),
                "hide" => self.hidden.push(s("text")),
                _ => self.calls.push(op.clone()),
            }
        }
    }

    /// The control of view `key` that stands for node `node` (the one that takes `event`'s
    /// value), with its state.
    #[must_use]
    pub fn control(&self, key: &str, node: &str) -> Option<(String, Value)> {
        let (_, controls) = self.views.get(key)?;
        controls
            .iter()
            .find(|(name, c)| c.get("node").as_str() == Some(node) && !name.ends_with('l'))
            .map(|(n, c)| (n.clone(), c.clone()))
    }
}

/// A recorded session handed to a host through the C interface, both ways, at its own pace.
#[derive(Debug)]
pub struct Replay {
    pub dll: Dll,
    pub surface: Surface,
    blobs: &'static [CorpusBlob],
    /// The messages, by index, that end a datagram: where the host applies what it was handed.
    ends: BTreeSet<usize>,
    /// When each received datagram that completed no message came: Dereth's driver sets its clock
    /// by every datagram, so the frames here fall where its do.
    bare: Vec<f64>,
    next: usize,
    next_bare: usize,
    pub now: f64,
    pub entered_at: f64,
}

/// Which of the recording's messages end a received datagram, and when each received datagram that
/// completed none came. The corpus marks no datagram, so the recording's raw datagrams are handed
/// to the client's own transport one at a time, and each message it completes is matched, in
/// order, to the corpus's.
fn datagrams(session: &str, blobs: &[CorpusBlob]) -> (BTreeSet<usize>, Vec<f64>) {
    let records = dereth_testkit::replay::records(session);
    let mut net = dereth_testkit::replay::recorded_endpoint(&records);
    let received: Vec<(usize, &CorpusBlob)> = blobs
        .iter()
        .enumerate()
        .filter(|(_, b)| b.dir == Direction::ServerToClient)
        .collect();
    let mut cursor = 0;
    let mut ends = BTreeSet::new();
    let mut bare = Vec::new();
    for r in records.iter().filter(|r| !r.c2s) {
        let now = LocalTime(r.t);
        net.feed(&r.raw, r.peer(), now);
        let mut last = None;
        while let Some(m) = net.session.transport.poll() {
            let mut payload = m.opcode.to_le_bytes().to_vec();
            payload.extend_from_slice(&m.body);
            while let Some((i, b)) = received.get(cursor) {
                cursor += 1;
                if b.payload == payload {
                    last = Some(*i);
                    break;
                }
            }
        }
        match last {
            Some(i) => {
                ends.insert(i);
            }
            None => bare.push(r.t),
        }
        net.tick(now);
    }
    (ends, bare)
}

fn at(b: &CorpusBlob) -> f64 {
    #[allow(clippy::cast_precision_loss)] // microseconds in an hour's recording
    let micros = b.t_rel_micros as f64;
    micros / 1e6
}

impl Replay {
    /// `session`'s messages for a host made from `settings`; nothing is fed yet.
    #[must_use]
    pub fn new(settings: &Value, session: &str) -> Self {
        let (ends, bare) = datagrams(session, &Corpus::shared(session).blobs);
        Self {
            dll: Dll::new(settings),
            surface: Surface::default(),
            blobs: &Corpus::shared(session).blobs,
            ends,
            bare,
            next: 0,
            next_bare: 0,
            now: 0.0,
            entered_at: 0.0,
        }
    }

    /// A host with no recording: frames only.
    #[must_use]
    pub fn frames_only(settings: &Value) -> Self {
        Self {
            dll: Dll::new(settings),
            surface: Surface::default(),
            blobs: &[],
            ends: BTreeSet::new(),
            bare: Vec::new(),
            next: 0,
            next_bare: 0,
            now: 0.0,
            entered_at: 0.0,
        }
    }

    fn take_ops(&mut self) {
        let ops = self.dll.ops();
        self.surface.apply(&ops);
    }

    /// One frame at the current time.
    pub fn frame(&mut self) {
        self.dll.frame(self.now);
        self.take_ops();
    }

    fn frames_until(&mut self, t: f64) {
        while self.now + 1.0 / FRAME_RATE <= t {
            self.now += 1.0 / FRAME_RATE;
            self.frame();
        }
    }

    /// The frames up to `t`, the clock set by each datagram that came before it with no message.
    fn bare_until(&mut self, t: f64) {
        while let Some(&at) = self.bare.get(self.next_bare).filter(|at| **at <= t) {
            self.next_bare += 1;
            self.frames_until(at);
            self.now = self.now.max(at);
        }
    }

    /// One recorded message handed over; the batch applied when it ends a datagram, as the
    /// client applies a datagram's messages together.
    ///
    /// The frames run on the received datagrams' clock, as Dereth's conformance driver runs them
    /// (it is handed no client message): a client message is handed over after the frames before
    /// it, but sets no clock of its own, so the frames after it fall where the driver's do.
    fn feed(&mut self, i: usize, b: &CorpusBlob) {
        self.bare_until(at(b));
        match b.dir {
            Direction::ServerToClient => {
                self.frames_until(at(b));
                self.now = self.now.max(at(b));
                self.dll.server(&b.payload, self.now);
            }
            Direction::ClientToServer => {
                self.frames_until(at(b));
                self.dll.client(&b.payload, self.now);
            }
        }
        if self.ends.contains(&i) {
            self.dll.flush();
        }
        self.take_ops();
    }

    /// The recording up to the character's entry into the world, then a frame.
    ///
    /// # Panics
    /// When the recording never reaches the world.
    pub fn login(&mut self) {
        while let Some(b) = self.blobs.get(self.next) {
            let i = self.next;
            self.next += 1;
            self.feed(i, b);
            if b.dir == Direction::ServerToClient
                && self.dll.report().get("phase").as_str() == Some("in_world")
            {
                break;
            }
        }
        assert_eq!(
            self.dll.report().get("phase").as_str(),
            Some("in_world"),
            "the recording reaches the world"
        );
        self.entered_at = self.now;
        self.frame();
    }

    /// `secs` more of the recording, frames between its messages.
    pub fn advance(&mut self, secs: f64) {
        let until = self.now + secs;
        while let Some(b) = self.blobs.get(self.next).filter(|b| at(b) <= until) {
            let i = self.next;
            self.next += 1;
            self.feed(i, b);
        }
        self.bare_until(until);
        self.frames_until(until);
    }

    /// Seconds since the character entered the world.
    #[must_use]
    pub fn since_entry(&self) -> f64 {
        self.now - self.entered_at
    }

    /// The player types `line`; a frame runs. Answers whether a plugin claimed it.
    pub fn type_line(&mut self, line: &str) -> bool {
        let claimed = self.dll.typed(line);
        self.frame();
        claimed
    }

    /// An event on a view's control; a frame runs.
    pub fn view_event(&mut self, key: &str, control: &str, event: &str, value: Value) {
        self.dll.view_event(key, control, event, value);
        self.frame();
    }

    /// A plugin's console lines.
    pub fn console(&mut self, plugin: &str) -> Vec<String> {
        self.dll
            .report()
            .get("plugins")
            .as_list()
            .unwrap_or_default()
            .iter()
            .find(|p| p.get("id").as_str() == Some(plugin))
            .map(|p| {
                p.get("console")
                    .as_list()
                    .unwrap_or_default()
                    .iter()
                    .map(|l| l.as_str().unwrap_or_default().to_owned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// A plugin's status word, or `absent`.
    pub fn status(&mut self, plugin: &str) -> String {
        self.dll
            .report()
            .get("plugins")
            .as_list()
            .unwrap_or_default()
            .iter()
            .find(|p| p.get("id").as_str() == Some(plugin))
            .map_or_else(
                || "absent".to_owned(),
                |p| p.get("status").as_str().unwrap_or_default().to_owned(),
            )
    }
}
