//! What the interface reads of the game each frame, taken by the shell from the runtime.
//!
//! A plain snapshot, so the screens can be drawn (and tested) without a runtime behind them.

use dereth_primitives::ObjectId;

/// One character on the account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterEntry {
    pub id: ObjectId,
    pub name: String,
    /// Seconds until a pending deletion completes; `0` when the character is not being deleted.
    pub delete_seconds: u32,
}

/// A vital: current and maximum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Vital {
    pub current: u32,
    pub max: u32,
}

impl Vital {
    /// How full, 0 to 1.
    #[must_use]
    pub fn ratio(&self) -> f32 {
        if self.max == 0 {
            0.0
        } else {
            #[allow(clippy::cast_precision_loss)]
            let r = self.current as f32 / self.max as f32;
            r.clamp(0.0, 1.0)
        }
    }
}

/// How the selected object relates to the player.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Relation {
    /// A creature that can be attacked.
    Hostile,
    /// A creature that can be attacked and is fighting the player.
    Engaged,
    /// A non-player character.
    Npc,
    /// Another player.
    Player,
    /// Anything else: an item, a door, a portal.
    #[default]
    Object,
}

/// The selected object, for the target bar.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Target {
    pub id: ObjectId,
    pub name: String,
    pub relation: Relation,
    /// Health as a ratio, when the client knows it.
    pub health: Option<f32>,
    pub level: Option<u32>,
    /// The icon the object shows in lists.
    pub icon: Option<u32>,
    /// The object as an item tile shows it, for its picture.
    pub look: Option<Item>,
}

/// An active enchantment, for the status row.
#[derive(Debug, Clone, PartialEq)]
pub struct Effect {
    /// The spell the enchantment is, for its examination.
    pub spell: u32,
    pub name: String,
    /// The spell's icon in the game data (AC's `0x06` picture).
    pub ac_icon: u32,
    pub harmful: bool,
    /// Seconds left; `None` for one that does not run out.
    pub remaining: Option<f64>,
}

/// Where getting into the world stands before character select can be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectPhase {
    /// Reaching the server.
    #[default]
    Connecting,
    /// The server is checking or updating the data files.
    Updating,
    /// Character select can be used: log in, create, delete.
    Ready,
}

/// One radar blip, in metres from the player, north up.
#[derive(Debug, Clone, PartialEq)]
pub struct Blip {
    pub id: ObjectId,
    pub dx: f32,
    pub dy: f32,
    pub kind: BlipKind,
    /// The game's own colour for it, `0xAARRGGBB`, and its shape (the game's blip shape
    /// number); 0 for either leaves the kind to decide.
    pub colour: u32,
    pub shape: u8,
    /// Its name, for the radar's tooltip.
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlipKind {
    Creature,
    Npc,
    Player,
    Fellow,
    Portal,
    Item,
    Selected,
}

/// A name over something in the world: where its top is on screen, what it is, and whether it is
/// the selection.
#[derive(Debug, Clone, PartialEq)]
pub struct Nameplate {
    /// What it names.
    pub id: ObjectId,
    pub name: String,
    /// The top of its body as drawn, over its origin, in screen pixels: the name stands on it.
    pub x: f32,
    pub y: f32,
    pub kind: BlipKind,
    pub selected: bool,
    /// The object's box on screen, in pixels.
    pub bounds: crate::draw::Rect,
    /// How far the object is from the player, in metres, when both have a place in the world.
    pub distance: Option<f32>,
    /// How near it is for the name's fade: 1 well within the names' range, falling to 0 at its
    /// edge.
    pub nearness: f32,
}

/// A box the game has put up: a question for Yes or No, or a message to acknowledge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub id: u64,
    pub text: String,
    /// Yes and No, rather than one OK.
    pub question: bool,
    /// It takes every press until it is answered.
    pub modal: bool,
    /// How many questions wait behind it.
    pub waiting: usize,
}

/// Which of the examine window's panes an appraisal fills.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExaminePane {
    #[default]
    Item,
    Creature,
    Character,
}

/// What the examine window shows: the game's appraisal of one object, in the game's own words.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Examined {
    pub id: ObjectId,
    pub pane: ExaminePane,
    pub title: String,
    pub icon: Option<u32>,
    /// The object as an item tile shows it, for its picture.
    pub look: Option<Item>,
    /// The item's description, block by block: the text, whether it follows on the next line
    /// (rather than after a blank one), and its colour (0 plain, 1 raised by magic, 2 lowered,
    /// 3 unknown).
    pub runs: Vec<(String, bool, u8)>,
    /// The inscription and its signature line, as the game shows them.
    pub inscription: Option<(String, String)>,
    /// Whether the player may write on the item: it is inscribable, and theirs and unsigned or
    /// signed by them, as the game's examine panel decides.
    pub inscription_editable: bool,
    /// What is written on the item now, and who signed it: what an edit is compared with.
    pub inscription_value: String,
    pub scribe: String,
    pub level: String,
    /// The creature's attributes and vitals: label, value, colour.
    pub rows: Vec<(String, String, u8)>,
    /// The extra rows under them (a player's allegiance, armour, ratings; a creature's
    /// ratings): label, value, colour.
    pub misc: Vec<(String, String, u8)>,
    /// A player's heritage, profession, PK status and allegiance.
    pub heritage: Option<String>,
    pub profession: Option<String>,
    pub pk: Option<String>,
    pub allegiance: Option<String>,
}

/// A chat line, as the log window shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatLine {
    pub chat_type: u8,
    pub text: String,
    /// The chat window it is addressed to; `0` is a broadcast every window's filter decides.
    pub window: u32,
}

impl ChatLine {
    /// A line as the game delivers it, its timestamp prefix in front.
    #[must_use]
    pub fn from_message(m: dereth_client_contract::chat::interface::ChatMessage) -> Self {
        Self {
            chat_type: m.ty,
            // The game ends many lines (combat's among them) with a line break, which the log
            // starts each line on anyway.
            text: match m.prefix {
                Some(p) if !p.is_empty() => format!("{p}{}", m.body),
                _ => m.body,
            }
            .trim_end_matches(['\n', '\r'])
            .to_owned(),
            window: m.window,
        }
    }
}

/// One of the game's chat windows, as the log window's tabs show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatWindow {
    /// The game's window id.
    pub id: u32,
    /// Which kinds of message it shows of the broadcast ones.
    pub filter: u64,
    /// Its title, when the game has set one.
    pub title: Option<String>,
}

/// A slot of the shortcut bar: an item or a spell.
#[derive(Debug, Clone, PartialEq)]
pub struct Shortcut {
    pub index: u32,
    pub name: String,
    pub object: Option<ObjectId>,
    pub spell: Option<u32>,
    pub ac_icon: Option<u32>,
    /// The item's cooldown: seconds left and its whole length, while it is cooling down.
    pub cooldown: Option<(f64, f64)>,
    /// The item as an item tile shows it, for its picture.
    pub look: Option<Item>,
}

/// An item, for the inventory and character windows.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Item {
    pub id: ObjectId,
    pub name: String,
    pub ac_icon: Option<u32>,
    /// The pictures under and over the icon that the game composes an item's tile from.
    pub underlay: Option<u32>,
    pub overlay: Option<u32>,
    pub stack: Option<u32>,
    /// Where it is worn, as a bit mask; `0` when carried.
    pub worn: u32,
    /// Where it can be worn or wielded, as its description says; `0` for a thing that cannot be.
    pub equip_locations: u32,
    /// It cannot be given away or dropped, as its appraisal says (when it has been appraised).
    pub attuned: bool,
    /// Its weenie class.
    pub wcid: u32,
    pub container: bool,
    /// What the game's item lists draw it from: its type and effects (the tile and the glow its
    /// icon is composed with), a container's capacity and how full it is.
    pub decoration: Option<dereth_client_contract::view::SlotDecoration>,
    /// Its cooldown: seconds left and its whole length, while it is cooling down.
    pub cooldown: Option<(f64, f64)>,
}

/// A skill, for the character window.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Skill {
    pub name: String,
    pub value: u32,
    /// 1 untrained, 2 trained, 3 specialised.
    pub training: u32,
    /// The skill's id, as the game's training messages name it.
    pub id: u32,
    pub icon: Option<u32>,
    /// What raising it costs: experience to raise a trained skill by one and by ten, or the
    /// skill credits to train an untrained one (in `cost`).
    pub cost: u32,
    pub cost_10: u32,
}

/// An attribute or vital the player can raise with experience, for the character window.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StatRow {
    pub name: String,
    /// The id the game's training messages name it by: an attribute's own, a vital's maximum.
    pub wire: u32,
    /// A vital rather than an attribute.
    pub vital: bool,
    /// What the row shows: the value, or a vital's current and maximum.
    pub shown: String,
    /// Raised or lowered by magic: 1 raised, 2 lowered, 0 neither.
    pub colour: u8,
    /// Experience to raise it by one and by ten.
    pub cost: u32,
    pub cost_10: u32,
}

/// A spell of the spellbook, for the actions window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spell {
    pub id: u32,
    pub name: String,
    pub school: u32,
    pub level: u32,
    pub ac_icon: u32,
    /// What the icon is composed with: the first component's power and the spell's bitfield.
    pub icon_power: u32,
    pub bitfield: u32,
}

/// Everything the interface reads of the game this frame.
#[derive(Debug, Clone, Default)]
pub struct GameState {
    /// The renderers this build can create and the one drawing, for the options page's renderer
    /// choice; none offered where there is nothing to choose.
    pub renderers: dereth_client_contract::options::renderer::RendererStatus,
    /// Where the experimental rendering effects stand on the device the client draws with, for
    /// the options page.
    #[cfg(feature = "hifi")]
    pub hifi: dereth_client_contract::options::fidelity::Availability,
    // ---- the connection ----
    /// The running client's version, as its host names it (`0.3.0`): what the pre-game screens
    /// show.
    pub client_version: String,
    /// A server link exists.
    pub connected: bool,
    /// Where getting into the world stands before character select can be used: still
    /// connecting, the server updating the data files, or ready.
    pub connect_phase: ConnectPhase,
    /// While the data files are being updated: the bytes received so far and the bytes expected,
    /// once the server has said how many.
    pub patch_progress: Option<(u64, u64)>,
    /// The session's state, in words.
    pub session: String,
    /// The account.
    pub account: String,
    /// The server's world name, once it has said.
    pub world: Option<String>,
    /// What the world says to the players choosing a character, when it says anything.
    pub world_message: Option<String>,
    /// The characters whose look is remembered from their last time in the world, so character
    /// select can show them.
    pub known_looks: Vec<ObjectId>,
    /// Each listed character's heritage group, where its looks are remembered.
    pub heritages: std::collections::BTreeMap<ObjectId, u32>,
    /// The selection is a thing to pick up off the ground: an item of no one's.
    /// The selection is used where it stands (a door, a lever), from a distance.
    pub target_usable_here: bool,
    pub target_pickable: bool,
    /// The selection is a container in the world (a corpse, a chest, a pack on the ground).
    pub target_container: bool,
    /// The selection cannot be moved from where it stands (a corpse, a chest).
    pub target_stuck: bool,
    /// Each listed character's level as it was when last seen in the world, for character
    /// select; a character never seen, or seen before levels were kept, has none.
    pub levels: std::collections::BTreeMap<ObjectId, u32>,
    /// The player's own clock, "8:43 PM".
    pub local_time: Option<String>,
    /// Whether the account may make the heritages and start town the third expansion added.
    pub account_has_tod: bool,
    /// How many items the main pack holds, as the game has the player's capacity.
    pub pack_capacity: Option<u32>,
    /// Which world's map notes the map shows: the towns and places named on its picture.
    pub map_profile: dereth_primitives::EraId,
    /// The account's first free character slot, when it has one.
    pub free_slot: Option<i32>,
    /// The game's last verification answer to a character sent, and how many it has had.
    pub chargen_response: Option<u32>,
    pub chargen_response_notices: u32,
    /// The two seeds the client's start-up gave the creation dice, once it has them.
    pub chargen_seeds: Option<(i32, u32)>,
    /// The last answer's refusal, in the game's words; `None` for an acceptance.
    pub chargen_refusal: Option<String>,
    /// The host the client connected to.
    pub host: String,
    /// The player looks where the camera does (camera-based movement), rather than along the
    /// character's facing.
    pub look_by_camera: bool,
    /// Where each thing the pad can select stands on screen, for those in view.
    pub on_screen: std::collections::BTreeMap<dereth_primitives::ObjectId, (f32, f32)>,
    /// The things the last world draw could see (a click could pick them): `None` before a draw.
    pub in_sight: Option<std::collections::BTreeSet<dereth_primitives::ObjectId>>,
    /// The text properties of the player module the server keeps for the character: `None`
    /// before it has arrived.
    pub player_module_strings: Option<Vec<(u32, String)>>,
    /// The account's characters, once the server has listed them.
    pub characters: Vec<CharacterEntry>,
    /// How many characters the account may have.
    pub character_slots: u32,
    /// Why the connection failed, in words.
    pub failure: Option<String>,
    /// A log-on is in progress.
    pub entering: bool,
    /// The world is hidden for a log-in or a portal, and the tunnel's swirl is showing.
    pub teleporting: bool,
    /// The session has ended; [`Self::failure`] says why.
    pub disconnected: bool,

    // ---- the player ----
    pub in_world: bool,
    pub name: String,
    pub level: u32,
    pub health: Vital,
    pub stamina: Vital,
    pub mana: Vital,
    pub total_xp: u64,
    pub unassigned_xp: u64,
    /// Experience needed for the next level, from the level table.
    pub xp_next: Option<u64>,
    /// Experience at which this level began.
    pub xp_this: Option<u64>,
    pub pyreals: u64,
    pub burden: Option<f32>,
    pub vitae: Option<f32>,
    /// The vitae window's numbers: the multiplier, the experience earned back towards the next
    /// point, and the experience a point takes.
    pub vitae_display: Option<dereth_client_contract::VitaeDisplay>,
    /// The vitae and burden windows' explanations, in the game's own words, while each is open.
    pub vitae_words: Option<String>,
    pub burden_words: Option<String>,
    /// `1` peace, `2` melee, `4` missile, `8` magic.
    pub combat_mode: u32,
    /// The power bar's charge, 0 to 1, while an attack or a jump is being built.
    pub power: Option<f32>,
    /// It is a jump's charge rather than an attack's.
    pub power_jump: bool,
    /// The combat window's settings: the attack height asked for and the power it aims at.
    pub combat_bar: dereth_client_contract::view::CombatBar,
    pub race: String,
    pub gender: String,
    pub heritage: String,
    pub title: String,
    pub attributes: Vec<(String, u32)>,
    pub vitals_base: Vec<(String, u32)>,
    pub skills: Vec<Skill>,
    /// The attributes and vitals with their raise costs, in the character window's order.
    pub stats: Vec<StatRow>,
    /// The skill credits the player has to train skills with.
    pub skill_credits: i64,
    pub effects: Vec<Effect>,
    pub target: Option<Target>,
    pub blips: Vec<Blip>,
    /// Everything about the player the pad can select, radar or not: doors, corpses, chests,
    /// people, creatures, things on the ground.
    pub targets: Vec<Blip>,
    /// The names over the people and creatures near enough to read.
    pub nameplates: Vec<Nameplate>,
    /// The player, whose main pack is the player itself.
    pub player_id: Option<ObjectId>,
    /// The player as the main pack's tile shows them: the game's backpack picture.
    pub main_pack: Option<Item>,
    /// The chest or corpse open on the ground: its id, its name and its contents.
    pub loot: Option<(ObjectId, String, Vec<Item>)>,
    /// The vendor the player is dealing with, while its window is open.
    pub shop: Option<dereth_client_contract::view::ShopView>,
    /// The game asked for the vendor's buying tab (filling spell components).
    pub shop_buying: bool,
    /// The objects the vendor and trade windows list, as item tiles show them, for their
    /// pictures.
    pub looks: Vec<Item>,
    /// The secure trade under way.
    pub trade: Option<dereth_client_contract::view::TradeView>,
    /// `@clear` was typed: the log empties.
    pub chat_cleared: bool,
    /// The spellbook's chosen spell and the game's examination of it.
    pub spell_detail: Option<(u32, dereth_client_contract::view::SpellExamineView)>,
    /// The spell the spell information window shows and the game's examination of it.
    pub spell_identify: Option<(u32, dereth_client_contract::view::SpellExamineView)>,
    /// The fellowship, whole, the allegiance and the friends, for the social windows.
    pub fellowship_view: Option<dereth_client_contract::view::FellowshipView>,
    pub roster: dereth_client_contract::view::AllegianceRoster,
    pub friend_list: Vec<dereth_client_contract::view::FriendEntry>,
    /// The player's own options (the character options), as the game holds them.
    pub options: Vec<(dereth_client_contract::view::PlayerOption, bool)>,
    /// The book open, as the game holds it.
    pub book: Option<dereth_client_contract::view::BookView>,
    /// The salvage window: the tool, and the items gathered to break down.
    pub salvage: Option<(ObjectId, Vec<Item>)>,
    /// The spell components, by category, with how many are carried and wanted.
    pub components: Vec<dereth_client_contract::view::ComponentCategory>,
    /// The contracts held, whole, for the Journal.
    pub contracts: Vec<dereth_client_contract::view::ContractEntry>,
    /// The character's titles and the one shown.
    pub titles: dereth_client_contract::view::CharacterTitles,
    /// The world's services: housing, chess, the barber, research, and the era's systems.
    pub world_services: crate::ui::panels::world::WorldServices,
    /// The sigil slots the character has unlocked on a world that has them, one bit each.
    pub sigil_slots: u8,
    /// The character's life facts: age, deaths, burden, augmentations, innate attributes.
    pub character_info: Option<dereth_client_contract::view::CharacterInfo>,
    /// The link: the last round trip in milliseconds, the packet loss as a ratio, and the
    /// seconds since the server was last heard (none when it is not).
    pub ping_ms: Option<f64>,
    pub packet_loss: f32,
    pub link_quiet: Option<f64>,
    /// The object being examined, once the game's appraisal of it has come.
    pub examined: Option<Examined>,
    /// An examine was asked for since the last frame: the window opens.
    pub examine_opened: bool,
    /// The game asked for the examine window to close (the examine key again).
    pub examine_closed: bool,
    /// A use or an examine is waiting for its target: a click on an item or a shortcut gives it.
    pub targeting: bool,
    /// The item whose use waits for what it is used on, while it does.
    pub armed: Option<ObjectId>,
    /// How far the radar reaches, in metres: further outdoors than in.
    pub radar_range: f32,
    /// The boxes the game has put up, messages first and then the question on screen.
    pub prompts: Vec<Prompt>,
    /// Where on screen the pad's alternate selection stands, raised to its middle.
    pub alt_at: Option<(f32, f32)>,
    /// The player's heading, degrees clockwise from north.
    pub heading: f32,
    /// The way the camera looks, degrees clockwise from north, as `heading` is given.
    pub camera_heading: f32,
    /// The player's map coordinates, north and east positive.
    pub coords: Option<(f32, f32)>,
    /// The region the player is in.
    pub zone: String,
    pub shortcuts: Vec<Shortcut>,
    pub spells: Vec<Spell>,
    /// The spell bar's tabs, each a list of spell ids.
    pub spell_tabs: Vec<Vec<u32>>,
    /// The keys that turn the spell bar to the previous and the next tab, as the key map names
    /// them, for the bar to show; empty while unknown or unbound.
    pub spell_tab_keys: (String, String),
    /// The duty list: each contract's title and its lines.
    pub duties: Vec<(String, Vec<String>)>,
    /// Friends, and whether each is online.
    pub friends: Vec<(String, bool)>,
    /// Pack contents: the main pack's items, then each side pack with its items.
    pub pack: Vec<Item>,
    pub side_packs: Vec<(Item, Vec<Item>)>,
    pub equipped: Vec<Item>,
    pub allegiance: Vec<(String, String)>,
    pub fellowship: Vec<(String, Vital, Vital, Vital, bool)>,
    /// The in-game date and time of day, as the game words them.
    pub game_time: Option<(String, String)>,
    /// Frames per second.
    pub fps: Option<f32>,
    /// Chat lines received this frame.
    pub new_chat: Vec<ChatLine>,
    /// What the world's era has, for the option rows that need it; `None` has everything.
    pub era: Option<dereth_primitives::EraFeatures>,
    /// Each chat window's filter, as the game holds it: `(window, filter)`.
    pub chat_filters: Vec<(u32, u64)>,
    /// The chat windows' two opacities, inactive then active, as the game holds them.
    pub chat_opacity: [Option<f32>; 2],
    /// The key bindings, as the client's input manager holds them.
    pub key_bindings: Option<KeyBindingsView>,
    /// The abuse log's answer to the last report, in the game's words.
    pub abuse_answer: Option<String>,
    /// The talk-focus destinations: the current one, which can be picked, and the tell target.
    pub chat_focus: dereth_client_contract::chat::mainchat::ChatFocusView,
    /// The game's chat windows: the main one, then the four floating ones.
    pub chat_windows: Vec<ChatWindow>,
    /// The open book as the game's book session has it: the page, its text, whether it is
    /// writable.
    pub book_session: dereth_client_contract::book::BookSessionView,
    /// The player's notebook, as the game's shared notebook holds it, and whether the world's
    /// era has one.
    pub journal: dereth_client_contract::journal::JournalView,
    pub notebook: bool,
    /// The characters and accounts the player has squelched, as the game holds them.
    pub squelches: Vec<dereth_client_contract::view::SquelchEntry>,
    /// Each talk-focus destination's label in the game's words, by focus.
    pub chat_focus_labels: Vec<String>,
    /// The main chat entry's text as the game keeps it between openings.
    pub chat_draft: String,
    /// What the game did to the chat entry this frame (a reply, a recalled line, a tell begun).
    pub chat_entry: Vec<dereth_client_contract::chat::entry::EntryUpdate>,
}

/// One bindable action as the key bindings page lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyRow {
    /// The input map the action is bound in.
    pub map: u32,
    pub action: u32,
    /// The page's tab ([`KEY_GROUPS`]).
    pub group: usize,
    pub caption: String,
    /// The controls bound to it, as the game names them.
    pub keys: Vec<String>,
    /// Whether its keys differ from the shipped ones.
    pub changed: bool,
}

/// The key bindings page's tabs, in the order the rows' groups come.
pub const KEY_GROUPS: [&str; 10] = [
    "Movement",
    "Camera",
    "Combat",
    "Selection",
    "Windows",
    "Chat",
    "Shortcuts",
    "Character",
    "Emotes",
    "Other",
];

/// A key being captured for a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyCapture {
    pub map: u32,
    pub action: u32,
    pub caption: String,
    /// The question asked before the captured key replaces what it does elsewhere.
    pub question: Option<String>,
}

/// The key bindings, for the key bindings page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeyBindingsView {
    pub rows: Vec<KeyRow>,
    /// The key being captured, while one is.
    pub capture: Option<KeyCapture>,
    /// Why the last key could not be bound, until the next capture.
    pub refused: Option<String>,
}

/// What the key bindings page asks of the input manager, which the client's shell keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyRequest {
    /// Wait for the next control and bind it to `action` in `map`: in place of key `slot` when
    /// the row has one there, beside its keys otherwise.
    Capture {
        map: u32,
        action: u32,
        slot: Option<usize>,
    },
    /// The answer to the capture's question: Yes binds the key anyway.
    Answer(bool),
    /// Stop waiting for a key.
    Cancel,
    /// Key `slot` no longer does `action`.
    Clear { map: u32, action: u32, slot: usize },
    /// Every row back to its shipped keys.
    RestoreDefaults,
    /// The mouse-turning settings' keys: the wheel zooms the camera.
    MouseTurningKeys,
}

impl GameState {
    /// A sample player for looking at the HUD with no server: what `--horizon-screen game` draws when
    /// the client is not connected. Every value is invented and plausible.
    #[must_use]
    pub fn preview() -> Self {
        let chat = [
            (5, "Welcome to Dereth!"),
            (2, "Aurelia Vale says, \"Anyone heading to the Direlands?\""),
            (
                19,
                "[Fellowship] Borin Stonefist says, \"Ready when you are.\"",
            ),
            (3, "Tirra tells you, \"Meet me at the Holtburg lifestone.\""),
            (
                6,
                "You hit the Drudge Prowler for 34 points of slashing damage!",
            ),
            (21, "The Drudge Prowler bashes you for 12 points of damage!"),
            (13, "You've earned 1,250 experience."),
            (
                18,
                "[Allegiance] Selene the Fair says, \"Tithes are up, thank you all.\"",
            ),
        ];
        Self {
            connected: false,
            world: Some("Dereth".into()),
            in_world: true,
            name: "Aurelia Vale".into(),
            level: 42,
            health: Vital {
                current: 214,
                max: 260,
            },
            stamina: Vital {
                current: 301,
                max: 380,
            },
            mana: Vital {
                current: 188,
                max: 240,
            },
            total_xp: 3_412_880,
            xp_this: Some(612_880),
            xp_next: Some(1_240_000),
            unassigned_xp: 85_000,
            pyreals: 128_450,
            burden: Some(0.64),
            combat_mode: 2,
            power: Some(0.6),
            heritage: "Aluvian Female".into(),
            title: "Adventurer".into(),
            attributes: vec![
                ("Strength".into(), 190),
                ("Endurance".into(), 150),
                ("Coordination".into(), 110),
                ("Quickness".into(), 100),
                ("Focus".into(), 60),
                ("Self".into(), 40),
            ],
            skills: vec![
                Skill {
                    name: "Heavy Weapons".into(),
                    value: 285,
                    training: 3,
                    ..Skill::default()
                },
                Skill {
                    name: "Melee Defense".into(),
                    value: 260,
                    training: 3,
                    ..Skill::default()
                },
                Skill {
                    name: "Healing".into(),
                    value: 180,
                    training: 2,
                    ..Skill::default()
                },
                Skill {
                    name: "Run".into(),
                    value: 210,
                    training: 2,
                    ..Skill::default()
                },
            ],
            effects: vec![
                Effect {
                    spell: 0,
                    name: "Strength Self VI".into(),
                    ac_icon: 0,
                    harmful: false,
                    remaining: Some(1742.0),
                },
                Effect {
                    spell: 0,
                    name: "Heavy Weapon Mastery V".into(),
                    ac_icon: 0,
                    harmful: false,
                    remaining: Some(412.0),
                },
                Effect {
                    spell: 0,
                    name: "Regeneration Self IV".into(),
                    ac_icon: 0,
                    harmful: false,
                    remaining: Some(8.0),
                },
                Effect {
                    spell: 0,
                    name: "Vulnerability Other III".into(),
                    ac_icon: 0,
                    harmful: true,
                    remaining: Some(24.0),
                },
            ],
            target: Some(Target {
                id: dereth_primitives::ObjectId(0x8000_1234),
                name: "Drudge Prowler".into(),
                relation: Relation::Engaged,
                health: Some(0.43),
                level: Some(38),
                icon: None,
                look: None,
            }),
            blips: vec![
                Blip {
                    id: ObjectId(0),
                    dx: 20.0,
                    dy: 12.0,
                    colour: 0,
                    shape: 0,
                    name: String::new(),
                    kind: BlipKind::Creature,
                },
                Blip {
                    id: ObjectId(0),
                    dx: -35.0,
                    dy: 40.0,
                    colour: 0,
                    shape: 0,
                    name: String::new(),
                    kind: BlipKind::Npc,
                },
                Blip {
                    id: ObjectId(0),
                    dx: 8.0,
                    dy: -30.0,
                    colour: 0,
                    shape: 0,
                    name: String::new(),
                    kind: BlipKind::Fellow,
                },
                Blip {
                    id: ObjectId(0),
                    dx: 14.0,
                    dy: 22.0,
                    colour: 0,
                    shape: 0,
                    name: String::new(),
                    kind: BlipKind::Selected,
                },
            ],
            heading: 30.0,
            coords: Some((42.1, -33.6)),
            game_time: Some(("Morningthaw 3, 10 P.Y.".into(), "Midsong".into())),
            fellowship: vec![
                (
                    "Aurelia Vale".into(),
                    Vital {
                        current: 214,
                        max: 260,
                    },
                    Vital {
                        current: 301,
                        max: 380,
                    },
                    Vital {
                        current: 188,
                        max: 240,
                    },
                    true,
                ),
                (
                    "Borin Stonefist".into(),
                    Vital {
                        current: 310,
                        max: 330,
                    },
                    Vital {
                        current: 200,
                        max: 290,
                    },
                    Vital {
                        current: 90,
                        max: 120,
                    },
                    false,
                ),
            ],
            shortcuts: [
                ("Healing Kit", None),
                ("Mana Potion", Some((12.0, 30.0))),
                ("Stamina Elixir", None),
            ]
            .into_iter()
            .enumerate()
            .map(|(i, (name, cooldown))| Shortcut {
                index: u32::try_from(i).unwrap_or(0),
                name: name.into(),
                object: Some(dereth_primitives::ObjectId(
                    0x8000_0100 + u32::try_from(i).unwrap_or(0),
                )),
                spell: None,
                ac_icon: None,
                cooldown,
                look: None,
            })
            .collect(),
            duties: vec![(
                "Drudge Hideout".into(),
                vec!["Slay the Drudge Prowlers 3/5".into()],
            )],
            new_chat: chat
                .iter()
                .map(|(t, s)| ChatLine {
                    chat_type: *t,
                    text: (*s).into(),
                    window: 0,
                })
                .collect(),
            ..Self::default()
        }
    }
}

impl GameState {
    /// Whether the player's option `o` is on.
    #[must_use]
    pub fn option(&self, o: dereth_client_contract::view::PlayerOption) -> bool {
        self.options.iter().any(|(p, on)| *p == o && *on)
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental Horizon interface)
    use super::ChatLine;

    #[test]
    fn a_line_the_game_ends_with_a_line_break_shows_without_an_empty_line_after_it() {
        let line = ChatLine::from_message(dereth_client_contract::chat::interface::ChatMessage {
            feedback: dereth_client_contract::feedback::Feedback::LOCAL,
            ty: 0x15,
            body: "You hit the Drudge for 12 points of damage!\n".into(),
            prefix: None,
            window: 0,
        });
        assert_eq!(line.text, "You hit the Drudge for 12 points of damage!");
    }
}
