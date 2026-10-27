//! The host: the game's model built from the messages the retail client receives and sends, and the
//! plugins run in it.
//!
//! The model is the one the Dereth client keeps and its conformance driver replays: the client's
//! session takes each message as the transport hands it over (its queue read off the opcode table,
//! since Decal gives the message alone), the object stream, the HUD model and the interaction apply
//! what it decodes, and the plugin host is stepped with the same frame the Dereth client hands it.
//! What the plugins ask for goes through the same interaction step; the requests it builds become
//! the Decal calls that make the retail client send them ([`crate::ops`]).
//!
//! **The player's place** comes from Decal's own reads of it, given each frame
//! ([`RetailHost::set_place`]); before the first, from what the server told of the character.
//! **The player's own movement** is any key pressed in the game while the chat line is closed
//! ([`RetailHost::player_moved`]): a plugin's movement then gives way at once, and the run lock it
//! set is let go (Decal cannot tell a movement key from any other). **The server taking
//! control** of the character's movement is read from the messages: a move-to or turn-to of the
//! character's own. **Portal space** lasts from the server's teleport (or the character's arrival
//! in the world) until the client says it has arrived.
//!
//! **The game's keys.** With the player's leave (the `keys` setting, or the plugin panel's switch)
//! a plugin's attack, jump and held movement press the game's own keys ([`crate::keys`]): an
//! attack's key is pressed after its target's selection and let go when the power bar has reached
//! the attack's power; a movement's keys are pressed and let go as the plugin host's movement holds
//! them. Keys go to the game only while its window is in front and its chat line is closed, as the
//! Decal plugin tells each frame ([`RetailHost::set_game_state`]); otherwise those actions fail at
//! once as `unavailable`, with why. A key the player presses lets go of every key the host holds,
//! and so does the game's window going to the back or its chat line opening.

use std::collections::VecDeque;
use std::path::PathBuf;

use dereth_client_sdk::contract::chat::interface::ChatMessage;
use dereth_client_sdk::contract::UiRequest;
use dereth_client_sdk::net::client_session::{Session, SessionEvent};
use dereth_client_sdk::plugin_api::{HostCapability, Value};
use dereth_client_sdk::plugins::{
    HostConfig, MemoryStorage, PluginHost, PluginStorage, StaticSource, VmClock,
};
use dereth_client_sdk::primitives::{
    CellId, Frame, IncomingMessage, LocalTime, NetBlobId, NetQueue, ObjectId, Position, Quat,
    RecipientId, ServerTime, Transport, Vec3,
};
use dereth_client_sdk::protocol::{Message as _, Opcode};
use dereth_client_sdk::runtime::frame_hook::{
    claim_line, outcomes_of, ExtensionCommand, HookFrame, NavRequest, Outcome, Places,
    SessionFacts, SessionPhase, SessionRequest, StreamPlaces, WireMessage,
};
use dereth_client_sdk::runtime::hud::{Hud, NoPanels};
use dereth_client_sdk::runtime::interaction::{apply_events_at_boundary, Interaction};
use dereth_client_sdk::runtime::objects::ObjectStream;

use crate::keys::{GameKey, Keymap};
use crate::marks::Marks;
use crate::ops::{self, Op};
use crate::views::{Asked, Views};

/// The host's name, as plugins read it (`host.info().name`).
pub const HOST_NAME: &str = "dereth-decal";

/// `Character_LoginCompleteNotification`, the game action the client sends once it has placed the
/// character after logging in or a teleport.
const LOGIN_COMPLETE: u32 = 0x00A1;

/// The plugin panel's switch that lets plugins press the game's keys.
pub const KEYS_SWITCH: &str = "game_keys";
/// The `keys` setting's name in the settings file.
pub const KEYS_SETTING: &str = "keys";

/// What the host is given when it is made.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    /// Folders of plugins to load (a run that names its plugins): each a plugin's folder or zip,
    /// or holding them.
    pub plugins: Vec<PathBuf>,
    /// Folders where plugins are found, as `plugins`: each found plugin is listed in the plugin
    /// panel and off until the player ticks it.
    pub found: Vec<PathBuf>,
    /// The plugins that come with the host: found, after the others.
    pub bundled: Option<PathBuf>,
    /// Where the plugins' stores (`stores/`) and data folders (`plugin-data/`) are kept; in memory
    /// when none.
    pub data: Option<PathBuf>,
    /// The game's folder, whose data files give the model its tables (spells, skills, vitals) and
    /// plugins the landblocks and cells they read.
    pub dats: Option<PathBuf>,
    /// The folder the clients on this machine list themselves in, for the bus between them.
    pub peers: Option<PathBuf>,
    /// Find the plugins the launcher installed too, as it offers them to the Dereth client: in its
    /// folders as the environment names them, or in `launcher_data`.
    pub launcher: bool,
    /// The launcher's data folder, in place of the one the environment names.
    pub launcher_data: Option<PathBuf>,
    /// The world's plugin policy document.
    pub policy: Option<String>,
    /// How long one entry into a plugin's VM may run, in milliseconds (the host's own when none).
    pub entry_budget_ms: Option<f64>,
    /// Count no time against a plugin's share of a frame: for a run whose outcome must not turn on
    /// how fast the machine is.
    pub untimed: bool,
    /// Apply the messages handed over only when asked ([`RetailHost::flush`], or a frame), and run
    /// frames only when asked ([`RetailHost::frame`]), never on a message that shows chat lines:
    /// for a run that batches and frames the way another host does, to compare the two.
    pub frames_when_asked: bool,
    /// The host's name as plugins read it ([`HOST_NAME`] when none): a run that compares this
    /// host with another names it after the other.
    pub name: Option<String>,
    /// Tell plugins nothing of portal space: for a run that compares this host with one that does
    /// not tell of it.
    pub no_portal_space: bool,
    /// Let plugins press the game's keys (the player's `keys = on`): their attacks, jumps and held
    /// movement ([`crate::keys`]). Off, those fail at once as `unavailable`.
    pub keys: bool,
}

impl Settings {
    /// Settings from their JSON: `{ plugins, found, bundled, data, dats, peers, launcher,
    /// launcher_data, policy, entry_budget_ms, untimed, frames_when_asked, name, no_portal_space,
    /// keys }`, every key optional.
    ///
    /// # Errors
    /// The text is not a JSON object.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let v = Value::from_json(text).map_err(|e| format!("the settings do not read: {e:?}"))?;
        if v.as_map().is_none() {
            return Err("the settings are a JSON object".into());
        }
        let path = |k: &str| {
            v.get(k)
                .as_str()
                .filter(|s| !s.is_empty())
                .map(PathBuf::from)
        };
        let paths = |k: &str| -> Vec<PathBuf> {
            v.get(k)
                .as_list()
                .unwrap_or_default()
                .iter()
                .filter_map(|p| p.as_str().map(PathBuf::from))
                .collect()
        };
        Ok(Self {
            plugins: paths("plugins"),
            found: paths("found"),
            bundled: path("bundled"),
            data: path("data"),
            dats: path("dats"),
            peers: path("peers"),
            launcher: v.get("launcher").as_bool().unwrap_or(false),
            launcher_data: path("launcher_data"),
            policy: v.get("policy").as_str().map(str::to_owned),
            entry_budget_ms: v.get("entry_budget_ms").as_f64(),
            untimed: v.get("untimed").as_bool().unwrap_or(false),
            frames_when_asked: v.get("frames_when_asked").as_bool().unwrap_or(false),
            name: v.get("name").as_str().map(str::to_owned),
            no_portal_space: v.get("no_portal_space").as_bool().unwrap_or(false),
            keys: v.get("keys").as_bool().unwrap_or(false),
        })
    }
}

/// The messages Decal hands over, waiting for the session.
#[derive(Debug, Default)]
struct Inbox {
    waiting: VecDeque<IncomingMessage>,
}

impl Transport for Inbox {
    // The retail client sends; the session's own sends go nowhere.
    fn send(&mut self, _queue: NetQueue, _ordered: bool, _payload: &[u8]) {}

    fn poll(&mut self) -> Option<IncomingMessage> {
        self.waiting.pop_front()
    }
}

/// A clock on which a plugin's entry takes no time.
#[derive(Debug)]
struct NoVmTime;

impl VmClock for NoVmTime {
    fn now(&self) -> std::time::Duration {
        std::time::Duration::ZERO
    }
}

/// The capabilities of a host inside the retail client: windows (as Decal views), markers and the
/// arrow (as Decal's 3-D objects), the character's movement by its facing and run lock, the
/// character list and logging off, the message stream; the bus between clients with a folder to
/// meet in, and the game's data with its files.
#[must_use]
pub fn capabilities(peers: bool, game_data: bool) -> Vec<HostCapability> {
    let mut caps = vec![
        HostCapability::Core,
        HostCapability::UiWindow,
        HostCapability::UiHud,
        HostCapability::OverlayWorld,
        HostCapability::OverlayArrow,
        HostCapability::NavMoveto,
        HostCapability::SessionPregame,
        HostCapability::Messages,
    ];
    if peers {
        caps.push(HostCapability::PeersLocal);
    }
    if game_data {
        caps.push(HostCapability::GameData);
    }
    caps
}

/// The player's place, as Decal reads it.
#[derive(Debug, Clone, Copy)]
struct Placed {
    position: Position,
}

/// The game's keys the host presses for plugins.
#[derive(Debug, Default)]
struct Keys {
    /// The player's leave, as the panel's switch last stood.
    on: bool,
    keymap: Keymap,
    /// Whether the game's window is in front, and its chat line open, as the Decal plugin last
    /// told.
    focused: bool,
    chat_open: bool,
    /// The keys down for plugins, in the order they went down, each with when the host lets it go
    /// when the host times it (an attack's).
    held: Vec<(GameKey, Option<f64>)>,
    /// The attack's power the plugin host last asked for, from 0 to 1.
    power: f64,
    /// What the plugin host's unavailable actions were last set for: the leave, the window in
    /// front, the chat line open.
    set_for: Option<(bool, bool, bool)>,
    /// A movement's key could not go down: the movement gives way at the next frame.
    refused: bool,
    /// The object a plugin's attack selected, while it is still the selection: stopping the
    /// attack clears it, which ends the game's own repeat of the swing. The player selecting
    /// anything else lets it go.
    attack_selected: Option<u32>,
}

impl Keys {
    /// Whether keys reach the game now.
    fn usable(&self) -> bool {
        self.on && self.focused && !self.chat_open
    }

    /// Why keys do not reach the game now, though the player lets plugins use them.
    fn why_not(&self) -> Option<&'static str> {
        if !self.focused {
            Some("the game window is not in front")
        } else if self.chat_open {
            Some("the chat line is open")
        } else {
            None
        }
    }

    /// Press `action`'s key, unless it is down; `until` is when the host lets it go.
    fn press(&mut self, action: GameKey, until: Option<f64>, ops: &mut Vec<Op>) {
        if self.held.iter().any(|(g, _)| *g == action) {
            return;
        }
        if let Some(key) = self.keymap.key(action) {
            ops.push(Op::Key { key, down: true });
            self.held.push((action, until));
        }
    }

    /// Let go of `action`'s key, if it is down.
    fn release(&mut self, action: GameKey, ops: &mut Vec<Op>) {
        if let Some(i) = self.held.iter().position(|(g, _)| *g == action) {
            self.held.remove(i);
            if let Some(key) = self.keymap.key(action) {
                ops.push(Op::Key { key, down: false });
            }
        }
    }

    /// Let go of every key down, the last pressed first.
    fn release_all(&mut self, ops: &mut Vec<Op>) {
        while let Some((g, _)) = self.held.last().copied() {
            self.release(g, ops);
        }
    }

    /// The attack key down, if one is.
    fn attack_held(&self) -> Option<GameKey> {
        self.held.iter().map(|(g, _)| *g).find(|g| g.is_attack())
    }
}

/// The game's model and the plugins in it.
#[derive(Debug)]
pub struct RetailHost {
    host: PluginHost,
    session: Session<Inbox>,
    objects: ObjectStream,
    hud: Hud,
    interaction: Interaction,
    now: f64,
    chat: Vec<ChatMessage>,
    outcomes: Vec<Outcome>,
    typed: Vec<ExtensionCommand>,
    verbs: Vec<String>,
    messages: Vec<WireMessage>,
    facts: SessionFacts,
    place: Option<Placed>,
    /// The run lock as the host last set it.
    autorun: bool,
    player_input: bool,
    server_moving: bool,
    /// Whether a batch was applied since the last frame.
    applied: bool,
    ops: Vec<Op>,
    views: Views,
    marks: Marks,
    keys: Keys,
    /// Lines for the player from before the first frame (a store or a data file that did not
    /// open).
    notes: Vec<String>,
    frames_when_asked: bool,
    no_portal_space: bool,
}

impl RetailHost {
    /// A host with `settings`, its plugins loaded at the first frame.
    #[must_use]
    pub fn new(settings: &Settings) -> Self {
        let mut notes = Vec::new();
        let store = settings.dats.as_ref().and_then(|d| {
            match dereth_client_sdk::dat::RetailDatStore::open_dir(d) {
                Ok(s) => Some(std::sync::Arc::new(s)),
                Err(e) => {
                    notes.push(format!(
                        "The game's data files did not open ({e}); spells and game data are not read."
                    ));
                    None
                }
            }
        });
        let peers = settings.peers.as_ref().and_then(|dir| {
            match dereth_headless::plugins::LoopbackPeers::open(dir) {
                Ok(p) => Some(p),
                Err(e) => {
                    notes.push(format!("The bus to your other clients is off: {e}"));
                    None
                }
            }
        });
        let mut config = HostConfig::new(
            settings.name.as_deref().unwrap_or(HOST_NAME),
            capabilities(peers.is_some(), store.is_some()),
        );
        config.version = env!("CARGO_PKG_VERSION").to_owned();
        config.unavailable = ops::UNAVAILABLE.to_vec();
        config.panel_switches = vec![(KEYS_SWITCH, "Let plugins use the game's keys".to_owned())];
        if let Some(ms) = settings.entry_budget_ms {
            config.limits.call_budget_ms = ms;
        }
        config.adding_plugins = settings.found.first().map(|d| {
            format!(
                "Put a plugin's folder in {} and restart the game.",
                d.display()
            )
        });
        let storage: Box<dyn PluginStorage> = match &settings.data {
            Some(d) => Box::new(dereth_headless::plugins::DirStorage {
                root: d.join("stores"),
            }),
            None => Box::new(MemoryStorage::default()),
        };
        let roots = settings.plugins.clone();
        let source: Box<dyn dereth_client_sdk::plugins::PluginSource> = if roots.is_empty() {
            Box::new(StaticSource::default())
        } else {
            Box::new(dereth_headless::plugins::DirSource { roots })
        };
        // The plugins found: the player's folders, the launcher's installs, then the ones that
        // come with the host, each off until the player ticks it.
        let mut found = settings.found.clone();
        if settings.launcher {
            found.extend(launcher_packages(settings.launcher_data.as_deref()));
        }
        found.extend(settings.bundled.iter().cloned());
        let mut host = PluginHost::new(
            config,
            Box::new(dereth_client_sdk::plugin_lua::LuauEngine),
            source,
            storage,
        )
        .with_engine(Box::new(
            dereth_client_sdk::plugin_wasm::WasmEngine::interpreted(),
        ));
        if !found.is_empty() {
            host = host.with_found(Box::new(dereth_headless::plugins::DirSource {
                roots: found,
            }));
        }
        if let Some(d) = &settings.data {
            host = host.with_files(Box::new(dereth_headless::plugins::DirFiles {
                root: d.join("plugin-data"),
            }));
        }
        if let Some(p) = peers {
            host = host.with_peers(Box::new(p));
        }
        if let Some(s) = &store {
            host = host.with_game_data(Box::new(dereth_client_sdk::DatGameData(s.clone())));
        }
        if settings.untimed {
            host = host.with_vm_clock(Box::new(NoVmTime));
        }
        if let Some(text) = &settings.policy {
            match dereth_client_sdk::plugins::policy_doc::parse(text) {
                Ok(p) => host.set_world_policy(p),
                Err(e) => notes.push(format!(
                    "The world's plugin policy did not read ({e}); none applies."
                )),
            }
        }
        // The toasts the plugins show go to the chat window: Decal has nothing to float them in.
        host.set_windows_shown(false);
        host.set_switch(KEYS_SWITCH, settings.keys);
        host.load_all();
        let mut hud = Hud::new();
        if let Some(s) = &store {
            let objects = ObjectStream::new();
            hud.load_tables(s, &objects.world);
        }
        Self {
            host,
            session: Session::new(Inbox::default()),
            objects: ObjectStream::new(),
            hud,
            interaction: Interaction::default(),
            now: 0.0,
            chat: Vec::new(),
            outcomes: Vec::new(),
            typed: Vec::new(),
            verbs: Vec::new(),
            messages: Vec::new(),
            facts: SessionFacts::default(),
            place: None,
            autorun: false,
            player_input: false,
            server_moving: false,
            applied: false,
            ops: Vec::new(),
            views: Views::default(),
            marks: Marks::default(),
            keys: Keys {
                on: settings.keys,
                ..Keys::default()
            },
            notes,
            frames_when_asked: settings.frames_when_asked,
            no_portal_space: settings.no_portal_space,
        }
    }

    /// The plugin host.
    #[must_use]
    pub fn plugins(&self) -> &PluginHost {
        &self.host
    }

    /// The plugin host, to drive directly.
    pub fn plugins_mut(&mut self) -> &mut PluginHost {
        &mut self.host
    }

    /// The object model.
    #[must_use]
    pub fn world(&self) -> &dereth_client_sdk::model::World {
        &self.objects.world
    }

    /// A message the client received: the whole of it, its type first, as Decal hands it over.
    /// When it gives chat lines a frame runs at once, so that a line a plugin hides is known before
    /// the client shows it.
    pub fn server_message(&mut self, blob: &[u8], now: f64) {
        let Some(head) = blob.get(..4) else {
            return;
        };
        let opcode = u32::from_le_bytes([head[0], head[1], head[2], head[3]]);
        let Some(queue) = queue_of(opcode) else {
            return;
        };
        self.now = self.now.max(now);
        self.session.transport.waiting.push_back(IncomingMessage {
            opcode,
            queue,
            sender: RecipientId(0),
            blob_id: NetBlobId(0),
            body: blob[4..].to_vec(),
        });
        if self.frames_when_asked {
            return;
        }
        if self.flush() > 0 {
            self.frame(self.now);
        }
    }

    /// The messages handed over so far, decoded and applied as one batch (as the client applies a
    /// datagram's); the number of chat lines they gave. A host that frames when asked applies them
    /// here, or at the next frame; any other applies each as it is handed over.
    pub fn flush(&mut self) -> usize {
        if self.session.transport.waiting.is_empty() {
            return 0;
        }
        let at = LocalTime(self.now);
        self.session.tick(at);
        let events = self.objects.pump_session(&mut self.session, at);
        self.apply(&events)
    }

    /// A message the client sent, as Decal hands it over.
    pub fn client_message(&mut self, blob: &[u8], now: f64) {
        let Some(head) = blob.get(..4) else {
            return;
        };
        // What the client received before it sent this is applied first.
        self.flush();
        self.now = self.now.max(now);
        let opcode = u32::from_le_bytes([head[0], head[1], head[2], head[3]]);
        match Opcode(opcode) {
            // The player chose a character: the session follows the client into the world.
            Opcode::LOGIN_SEND_ENTER_WORLD_REQUEST => {
                let account = self.session.characters().account.clone();
                self.session.enter_world(ObjectId(0), &account);
            }
            Opcode::LOGIN_EXECUTE_LOG_OFF => self.session.log_off(),
            // A game action: the client's word that it has placed the character ends portal space.
            Opcode(0xF7B1)
                if blob
                    .get(8..12)
                    .map(|t| u32::from_le_bytes([t[0], t[1], t[2], t[3]]))
                    == Some(LOGIN_COMPLETE) =>
            {
                self.facts.portal_space = false;
            }
            _ => {}
        }
        if self.host.wants_messages() {
            self.messages.push(WireMessage::sent(blob.to_vec()));
        }
    }

    /// The player's place as Decal reads it: the cell, the place in its landblock, and the heading
    /// in degrees clockwise from north.
    pub fn set_place(&mut self, landcell: u32, x: f64, y: f64, z: f64, heading: f64) {
        #[allow(clippy::cast_possible_truncation)] // meters in a landblock
        let mut frame = Frame::new(Vec3::new(x as f32, y as f32, z as f32), Quat::IDENTITY);
        #[allow(clippy::cast_possible_truncation)]
        dereth_client_sdk::primitives::frame::set_heading(&mut frame, heading as f32);
        self.place = Some(Placed {
            position: Position::new(CellId(landcell), frame),
        });
    }

    /// The object the player selected, as Decal says (0 for none).
    pub fn set_selected(&mut self, id: u32) {
        self.objects.world.selected = (id != 0).then_some(ObjectId(id));
        if self.keys.attack_selected != Some(id) {
            self.keys.attack_selected = None;
        }
    }

    /// The player pressed a key in the game: their own movement, which a plugin's gives way to.
    pub fn player_moved(&mut self) {
        self.player_input = true;
    }

    /// Whether the game's window is in front, and whether its chat line is open, as Decal says
    /// each frame: keys reach the game only while it is in front with the chat line closed.
    pub fn set_game_state(&mut self, focused: bool, chat_open: bool) {
        self.keys.focused = focused;
        self.keys.chat_open = chat_open;
    }

    /// The player's key bindings, as Decal reads them from the game's key map: `{ <the game's
    /// action name>: <virtual-key code> }` ([`Keymap::set`]). While plugins may use the game's
    /// keys, the player is told which.
    pub fn set_bindings(&mut self, bindings: &Value) {
        self.keys.keymap.set(bindings);
        if self.keys.on {
            self.tell_keys();
        }
    }

    /// Whether plugins may press the game's keys: the player's leave.
    #[must_use]
    pub fn keys_on(&self) -> bool {
        self.keys.on
    }

    /// The line that tells the player plugins may use the game's keys, and which.
    fn tell_keys(&mut self) {
        self.ops.push(Op::Print {
            text: format!(
                "[Plugins] Plugins can use the game's keys now ({}).",
                self.keys.keymap.told()
            ),
            color: 5,
        });
    }

    /// The panel's switch as the player left it: a change is kept in the settings file, and told.
    fn follow_keys_switch(&mut self) {
        let on = self.host.switch(KEYS_SWITCH);
        if on == self.keys.on {
            return;
        }
        self.keys.on = on;
        self.ops.push(Op::Setting {
            name: KEYS_SETTING.to_owned(),
            value: if on { "on" } else { "off" }.to_owned(),
        });
        if on {
            self.tell_keys();
        } else {
            self.ops.push(Op::Print {
                text: "[Plugins] Plugins no longer use the game's keys.".to_owned(),
                color: 5,
            });
        }
    }

    /// The plugin host's unavailable actions as the keys stand: without the player's leave the
    /// key actions are unavailable as the rest are; with it, only while the game's window is not
    /// in front or its chat line is open, and then with why.
    fn set_key_actions(&mut self) {
        let k = &self.keys;
        let state = (k.on, k.focused, k.chat_open);
        if k.set_for == Some(state) {
            return;
        }
        self.keys.set_for = Some(state);
        let others: Vec<&'static str> = ops::UNAVAILABLE
            .iter()
            .copied()
            .filter(|a| !ops::KEY_ACTIONS.contains(a))
            .collect();
        let (plain, why) = match (self.keys.on, self.keys.why_not()) {
            (false, _) => (ops::UNAVAILABLE.to_vec(), Vec::new()),
            (true, Some(why)) => (
                others,
                ops::KEY_ACTIONS
                    .iter()
                    .map(|a| (*a, why.to_owned()))
                    .collect(),
            ),
            (true, None) => (others, Vec::new()),
        };
        self.host.set_unavailable(plain, why);
    }

    /// The keys before the plugins are stepped: every key let go when the player pressed one,
    /// when keys no longer reach the game, or when a movement's key could not go down (each the
    /// player's own control, which a plugin's movement gives way to), and an attack's key once
    /// its time is up. Answers whether a plugin's movement gives way.
    fn keys_before_step(&mut self, player_input: bool, now: f64) -> bool {
        self.follow_keys_switch();
        let lost = !self.keys.usable() && !self.keys.held.is_empty();
        let refused = std::mem::take(&mut self.keys.refused);
        if player_input || lost {
            self.keys.release_all(&mut self.ops);
        }
        let due: Vec<GameKey> = self
            .keys
            .held
            .iter()
            .filter(|(_, until)| until.is_some_and(|t| now >= t))
            .map(|(g, _)| *g)
            .collect();
        for g in due {
            self.keys.release(g, &mut self.ops);
        }
        self.set_key_actions();
        lost || refused
    }

    /// A plugin's attack at `height`: its key pressed, after its target's selection, and let go
    /// once the power bar has reached the power asked for (it fills in a second, or eight tenths
    /// of one while dual-wielding).
    fn attack_key(&mut self, height: u32, now: f64) {
        use dereth_client_sdk::model::combat::{
            CombatMode, DUAL_WIELD_COMBAT_STYLE, POWER_BAR_SECONDS, POWER_BAR_SECONDS_DUAL_WIELD,
        };
        let combat = &self.objects.world.combat;
        let missile = combat.combat_mode == CombatMode::Missile;
        let full = if combat.current_style == DUAL_WIELD_COMBAT_STYLE {
            POWER_BAR_SECONDS_DUAL_WIELD
        } else {
            POWER_BAR_SECONDS
        };
        if let Some(g) = self.keys.attack_held() {
            self.keys.release(g, &mut self.ops);
        }
        let until = now + self.keys.power * full;
        self.keys
            .press(GameKey::attack(height, missile), Some(until), &mut self.ops);
    }

    /// A line the player typed: whether a plugin (or the host) claims it, in which case the client
    /// is not to send it. A claimed line reaches its plugin at the next frame.
    pub fn typed(&mut self, line: &str) -> bool {
        match claim_line(line, &self.verbs, self.interaction.command_table()) {
            Some(c) => {
                self.typed.push(c);
                true
            }
            None => false,
        }
    }

    /// An event on a plugin window's view (see [`Views::event`]).
    pub fn view_event(&mut self, key: &str, control: &str, event: &str, value: &Value) {
        match self.views.event(key, control, event, value) {
            Some(Asked::Input {
                plugin,
                window,
                input,
            }) => {
                let _ = self.host.ui_input(&plugin, &window, input);
            }
            Some(Asked::Hotkey { plugin, node }) => {
                let _ = self.host.press_hotkey(&plugin, &node);
            }
            Some(Asked::Open { plugin, window }) => {
                if self.is_bar_window(&plugin, &window) {
                    let _ = self.host.toggle_window(&plugin);
                } else {
                    // Only the plugin opens its other windows: the view goes back to how it was.
                    self.ops.push(Op::Show {
                        key: crate::views::key(&plugin, &window),
                        open: false,
                    });
                }
            }
            Some(Asked::Close { plugin, window }) => {
                if self.is_bar_window(&plugin, &window) {
                    let _ = self.host.toggle_window(&plugin);
                } else {
                    let _ = self.host.ui_input(
                        &plugin,
                        &window,
                        dereth_client_sdk::plugins::UiInput::Close,
                    );
                }
            }
            None => {}
        }
    }

    /// Whether `window` is `plugin`'s window on the plugin bar, which the bar opens and closes.
    fn is_bar_window(&self, plugin: &str, window: &str) -> bool {
        self.host
            .bar()
            .iter()
            .any(|b| b.id == plugin && b.window.as_deref() == Some(window))
    }

    /// The session's phase, as plugins read it (`in_world` once a character is in the world).
    #[must_use]
    pub fn phase(&self) -> SessionPhase {
        SessionPhase::of(self.session.state())
    }

    /// What the host holds, as text a person or a test reads: each plugin's status and console,
    /// the host's own console, every window as the host keeps it (`window <plugin> <id>
    /// open=<bool>`, then its tree), the plugin bar and the markers.
    #[must_use]
    pub fn report(&self) -> Value {
        let plugins: Vec<Value> = self
            .host
            .plugins()
            .into_iter()
            .map(|p| {
                Value::map()
                    .with("id", p.id.as_str())
                    .with("status", p.status.as_str())
                    .with("reason", p.reason.clone())
                    .with(
                        "console",
                        self.host
                            .console(&p.id)
                            .unwrap_or_default()
                            .into_iter()
                            .map(Value::from)
                            .collect::<Vec<_>>(),
                    )
            })
            .collect();
        let mut ui = String::new();
        for (p, w, win) in self.host.windows() {
            ui.push_str(&format!("window {p} {w} open={}", win.open));
            if let Some((sw, sh)) = win.size {
                ui.push_str(&format!(" size={sw}x{sh}"));
            }
            ui.push('\n');
            ui.push_str(&win.tree.dump());
        }
        let bar: Vec<Value> = self
            .host
            .bar()
            .into_iter()
            .map(|e| {
                Value::from(format!(
                    "{}\t{}\t{}",
                    e.id,
                    e.state,
                    if e.open { "open" } else { "closed" }
                ))
            })
            .collect();
        Value::map()
            .with("phase", self.phase().name())
            .with("plugins", plugins)
            .with(
                "host_console",
                self.host
                    .host_console()
                    .into_iter()
                    .map(Value::from)
                    .collect::<Vec<_>>(),
            )
            .with("ui", ui)
            .with("bar", bar)
            .with("overlays", self.host.overlays().dump())
            .with(
                "keys",
                Value::map().with("on", self.keys.on).with(
                    "held",
                    self.keys
                        .held
                        .iter()
                        .map(|(g, _)| Value::from(g.name()))
                        .collect::<Vec<_>>(),
                ),
            )
    }

    /// What the Decal plugin is to do, since the last call.
    pub fn take_ops(&mut self) -> Vec<Op> {
        std::mem::take(&mut self.ops)
    }

    /// The server events through the HUD and the interaction, as a frame applies them; the number
    /// of chat lines they gave.
    fn apply(&mut self, events: &[SessionEvent]) -> usize {
        self.applied = true;
        self.hud.now = LocalTime(self.now);
        let chat = self.hud.apply_events_with_combat_mode_handler(
            events,
            &mut self.objects.world,
            &mut NoPanels,
            None,
            &mut |_, _| {},
        );
        apply_events_at_boundary(
            &mut self.interaction,
            events,
            &mut self.objects.world,
            None,
            &mut |_, _, _| {},
        );
        let player = self.objects.world.player;
        for e in events {
            match e {
                SessionEvent::CharacterSet(set) => {
                    self.facts.characters = set
                        .characters
                        .iter()
                        .map(|c| (c.gid, c.name.clone()))
                        .collect();
                }
                SessionEvent::PlayerCreated(_) if !self.no_portal_space => {
                    self.facts.portal_space = true;
                }
                SessionEvent::WorldObject { opcode, body }
                    if *opcode == Opcode::MOVEMENT_SET_OBJECT_MOVEMENT =>
                {
                    if server_moves(body, player) {
                        self.server_moving = true;
                    }
                }
                SessionEvent::WorldObject { opcode, .. }
                    if *opcode == Opcode::EFFECTS_PLAYER_TELEPORT && !self.no_portal_space =>
                {
                    self.facts.portal_space = true;
                }
                _ => {}
            }
        }
        let lines = chat.len();
        self.chat.extend(chat);
        self.outcomes.extend(outcomes_of(events));
        if self.host.wants_messages() {
            self.messages
                .extend(events.iter().filter_map(WireMessage::received));
        }
        lines
    }

    /// One frame at `now` (seconds on the caller's clock): the plugins stepped, what they asked
    /// for turned into ops.
    pub fn frame(&mut self, now: f64) {
        // A batch a frame, as the client applies one: what was handed over since the last frame,
        // or nothing; either way the lines an earlier batch left for the next one come out.
        self.flush();
        if !self.applied {
            self.apply(&[]);
        }
        self.applied = false;
        self.now = self.now.max(now);
        let now = self.now;
        for n in std::mem::take(&mut self.notes) {
            self.ops.push(Op::Print {
                text: format!("[Plugins] {n}"),
                color: 5,
            });
        }
        self.facts.phase = SessionPhase::of(self.session.state());
        let chat = std::mem::take(&mut self.chat);
        let outcomes = std::mem::take(&mut self.outcomes);
        let typed = std::mem::take(&mut self.typed);
        let messages = std::mem::take(&mut self.messages);
        let player_input = std::mem::take(&mut self.player_input);
        let keys_lost = self.keys_before_step(player_input, now);
        let player_input = player_input || keys_lost;
        if player_input && self.autorun {
            // Decal cannot tell a movement key from any other, so the key that ends a plugin's
            // movement lets go of the run lock the plugin set too, lest the character run on.
            self.autorun = false;
            self.ops.push(Op::Autorun { on: false });
        }
        let server_moving = std::mem::take(&mut self.server_moving);
        let account = self.session.characters().account.clone();
        let out = {
            let view = self.hud.view(&self.objects);
            let places = StreamPlaces {
                player: self.place.map(|p| p.position),
                objects: &self.objects,
                autorun: self.autorun,
                sight: None,
            };
            let places: &dyn Places = &places;
            let mut frame = HookFrame::new(
                &view,
                &self.objects.world,
                self.interaction.command_table(),
                now,
                now,
                &chat,
                &typed,
            )
            .with_outcomes(&outcomes)
            .with_messages(&messages)
            .with_session(
                self.session.world_name(),
                (!account.is_empty()).then_some(account.as_str()),
            )
            .with_pregame(&self.facts)
            .with_places(places)
            .with_control(player_input, server_moving);
            self.host.step(&mut frame);
            frame.into_output()
        };
        for n in &out.nav {
            match *n {
                NavRequest::Face(h) => self.ops.push(Op::Face {
                    heading: f64::from(h),
                }),
                NavRequest::Autorun(on) => {
                    self.autorun = on;
                    self.ops.push(Op::Autorun { on });
                }
                // A movement's key, pressed only while keys reach the game; one that cannot go
                // down ends the movement at the next frame.
                NavRequest::Hold(key, true) => {
                    if self.keys.usable() {
                        self.keys.press(GameKey::of_nav(key), None, &mut self.ops);
                    } else {
                        self.keys.refused = true;
                    }
                }
                NavRequest::Hold(key, false) => {
                    self.keys.release(GameKey::of_nav(key), &mut self.ops);
                }
                // The game puts a charged jump down only when its window loses the keyboard: its
                // key let go, the jump goes at what it has charged.
                NavRequest::CancelJump => self.keys.release(GameKey::Jump, &mut self.ops),
            }
        }
        for s in &out.session {
            match s {
                SessionRequest::LogOff | SessionRequest::Quit => self.ops.push(Op::Logout),
                SessionRequest::LogOn(_) => {}
            }
        }
        for &i in &out.hidden {
            if let Some(line) = chat.get(i) {
                self.ops.push(Op::Hide {
                    text: line.body.clone(),
                });
            }
        }
        for line in &out.printed {
            self.ops.push(Op::Print {
                text: line.body.clone(),
                color: u32::from(line.ty).min(22),
            });
        }
        if let Some(v) = out.verbs {
            self.verbs = v;
        }
        let mut requests = Vec::new();
        // The object a plugin's request selected this frame in place of another: an attack's
        // target.
        let mut selected_now = None;
        for r in out.requests {
            match r {
                UiRequest::ChatLineOnBehalf { text, .. } => self.ops.push(Op::Chat { line: text }),
                UiRequest::Select(id) => {
                    self.ops.push(Op::Select { id: id.0 });
                    // Only a change of the selection is the plugin's: what the player had
                    // selected already stays theirs.
                    selected_now = (self.objects.world.selected != Some(id)).then_some(id.0);
                    requests.push(UiRequest::Select(id));
                }
                // A plugin's attack: the power it asks for, then its key, after its target's
                // selection. The host's own interaction still runs it, and its power bar and the
                // server's answer settle the plugin's promise.
                UiRequest::CombatSetDesiredPower { position } => {
                    self.keys.power = (f64::from(position) / 1000.0).clamp(0.0, 1.0);
                    requests.push(UiRequest::CombatSetDesiredPower { position });
                }
                UiRequest::CombatEndAttack { height } => {
                    if self.keys.usable() {
                        self.attack_key(height, now);
                        self.keys.attack_selected = selected_now;
                    }
                    requests.push(UiRequest::CombatEndAttack { height });
                }
                // A plugin's stop: its attack key let go, and the target its attack selected,
                // while it is still the selection, cleared; the game repeats no swing at nothing.
                UiRequest::CombatAbortAttack => {
                    if let Some(g) = self.keys.attack_held() {
                        self.keys.release(g, &mut self.ops);
                    }
                    if let Some(id) = self.keys.attack_selected.take() {
                        if self.objects.world.selected == Some(ObjectId(id)) {
                            self.ops.push(Op::Select { id: 0 });
                            self.objects.world.selected = None;
                        }
                    }
                    requests.push(UiRequest::CombatAbortAttack);
                }
                other => requests.push(other),
            }
        }
        self.interact(requests, now);
        self.views.sync(&self.host, &mut self.ops);
        self.marks.sync(self.host.overlays(), &mut self.ops);
    }

    /// The interaction step for what the plugins asked: the requests it builds, as ops.
    fn interact(&mut self, requests: Vec<UiRequest>, now: f64) {
        if !requests.is_empty() {
            self.interaction.queue(Vec::new(), requests);
            let _ = self.interaction.run_ui_requests_with_chat_focus(
                &mut self.objects.world,
                self.hud.player_desc_received,
                ServerTime(now),
                &mut |_| {},
            );
        }
        // A stance asked for, and the power bar of an attack: as the client's own frame runs them.
        let world = &mut self.objects.world;
        let attack_ready = world.ready_position_mode_arm(true, false);
        self.interaction
            .run_power_bar(world, attack_ready, LocalTime(now));
        let ready = world.ready_position_mode_arm(false, false);
        let phys = dereth_client_sdk::runtime::selection_geometry::SceneSelectionPhysics::new(
            None,
            &self.objects,
        );
        self.interaction.run_combat_mode_toggle(
            &mut self.objects.world,
            ready,
            &phys,
            25.0,
            LocalTime(now),
        );
        for r in self.interaction.take_pending_requests() {
            self.ops.extend(ops::of_request(&r, &self.objects.world));
        }
    }
}

/// The plugins the launcher offers the Dereth client: the plugins in the folders on this computer
/// it lists, and each installed plugin's current release, from the launcher's data folder (`data`,
/// or the one the environment names), read without changing anything there. None when there is no
/// launcher's folder or list.
#[must_use]
pub fn launcher_packages(data: Option<&std::path::Path>) -> Vec<PathBuf> {
    use dereth_launch::folders::{self, Folders, System};
    let folders = match data {
        Some(d) => Some(Folders::single(d)),
        None => folders::resolve(System::HOST, &|name| std::env::var_os(name)),
    };
    folders.map_or_else(Vec::new, |f| {
        dereth_launch::plugins::Shelf::read(f.plugins()).offered_packages()
    })
}

/// The queue the client receives a message of `opcode` on, by the opcode table (a game event, under
/// its ordered header, on the interface's queue); `None` for one the client never receives.
#[must_use]
pub fn queue_of(opcode: u32) -> Option<NetQueue> {
    if opcode == dereth_client_sdk::protocol::order::OrderedEventHeader::MAGIC {
        return Some(NetQueue::UiQueue);
    }
    Opcode(opcode).info().and_then(|i| i.recv_queue)
}

/// Whether a `Movement_SetObjectMovement` (`0xF74C`) body moves `player` under the server's control:
/// a move to an object or a place, or a turn to one.
fn server_moves(body: &[u8], player: Option<ObjectId>) -> bool {
    use dereth_client_sdk::protocol::movement::{
        movement_type, MovementBuffer, MovementSetObjectMovement,
    };
    let Some(player) = player else {
        return false;
    };
    let mut r = dereth_client_sdk::protocol::Reader::new(body);
    let Ok(m) = MovementSetObjectMovement::read(&mut r) else {
        return false;
    };
    if m.id != player {
        return false;
    }
    let mut r = dereth_client_sdk::protocol::Reader::new(&m.movement);
    MovementBuffer::read(&mut r).is_ok_and(|b| {
        matches!(
            b.body.movement_type,
            movement_type::MOVE_TO_OBJECT
                | movement_type::MOVE_TO_POSITION
                | movement_type::TURN_TO_OBJECT
                | movement_type::TURN_TO_HEADING
        )
    })
}
