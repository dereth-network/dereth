//! The player's requests through the interface, made again at the moments the recorded player
//! made them: what the player's scripts send then is what the recorded client sent; and what the
//! player's position reports carry, against the recorded client's.

use std::collections::{BTreeMap, VecDeque};

use auberean_client_contract::{Action, PlayerOption, SocialAction};
use auberean_client_net::LocalTime;
use auberean_client_runtime::examination::Request as Examination;
use auberean_client_runtime::host::{argument_slots, is_panel_notice};
use auberean_client_runtime::interface::function;
use auberean_client_runtime::interface::OPTION_SETTERS;
use auberean_client_runtime::notices::{reading, Hearers, NOTICES};
use auberean_client_runtime::{CallResult, Client, ClientData, ClientSettings, InterfaceArg};
use auberean_protocol::{ClientMessage, ServerMessage};
use auberean_script::args::{self, Arg};
use auberean_script::graph::{Body, Graph};
use auberean_script::native_object::{Item, NativeObject};
use auberean_script::options::kind;
use auberean_testkit::captures::{self, carrier, ScriptCall};

use crate::calls::{data, PLAY_CHARACTER};

/// The player's report of its own position.
const MY_POSITION: u32 = 0x1008E;
/// The server function a new selection is sent to.
const SET_SELECTION: u32 = 0x8212_0016;
/// The server function an attack's start is sent to.
const START_ATTACK: u32 = 0x8204_0008;
/// The server function an attack with a skill is sent to.
const DO_ATTACK: u32 = 0x8204_000B;
/// The server function using an item is sent to.
const USE_ITEM: u32 = 0x8216_0016;
/// The server functions the windows' saved places and the game options are sent to.
const UPLOAD_UI_SETTINGS: u32 = 0x8212_002F;
const UPLOAD_GAME_OPTIONS: u32 = 0x8212_000C;
/// The server function a shortcut put in a slot is sent to.
const SET_SHORTCUT: u32 = 0x8212_0018;

/// How a recorded send is made again: the interface's request that sends it.
#[derive(Clone, Copy)]
enum Request {
    /// Select the object the send names.
    Select,
    /// Start attacking.
    Attack,
    /// Open the examination panel with the tabs the request asks for.
    Examine,
    /// Use the toolbar slot holding the skill the send names.
    SkillShortcut,
    /// Use the item the send names: the selection when it is selected, else the toolbar slot
    /// holding it; an item used on something then has that something selected, which the
    /// scripts take as the target.
    UseItem,
    /// Put the shortcut the send carries in its slot.
    PutInSlot,
    /// Keep the windows' places and opacities the send carries, as the interface keeps them,
    /// then send them.
    UiSettings,
    /// Set the options the send carries, as the options panel sets them, then send them.
    GameOptions,
    /// An interface function, given the send's values.
    Interface(u32),
    /// An interface function, given the send's values after the first few.
    InterfaceAfter(u32, usize),
    /// An interface function, given so many of the send's values after the first few (the
    /// scripts add the others: a shop's keeper, what a trade holds, a consignment's shop).
    InterfaceSome(u32, usize, usize),
    /// Take everything out of the container the send's request names (its 64-bit slot at the
    /// index given), or melt it all into money.
    FromContainer(usize, bool),
    /// Close the container the send names.
    Close,
    /// Stop attacking.
    StopAttack,
    /// Set the chat options the send carries, each entry as the chat options' window sets it,
    /// then send them.
    ChatOptions,
    /// Move the item the send's request names (its item at slots 44 and 45, from the container at
    /// 16 and 17 and its slot at 39, to the container at 2 and 3 and its slot at 11, so many at
    /// 26), as the pack's drag does.
    Move,
    /// Follow what is selected: the command typed.
    Follow,
    /// Say what the send's string reference holds as its literal: the line typed (the literal's
    /// escapes taken off: a backslash keeps the mark after it from being read as one).
    Say,
    /// Buy the item the send's transaction names (its slots 0 and 1), so many (slot 5), into a
    /// slot of the pack (slot 9), and answer the purchase's question yes.
    Purchase,
    /// The social window's request that sends it (the fellowship's, the allegiance's).
    Social,
    /// An emote by its id: its command typed (the first word of the emote's command text), or
    /// for the away emote, the friends window's status menu.
    Emote,
    /// A tell: `/reply` and the message typed when it is to whom the player's scripts keep as the
    /// last to tell the player something, `/retell` and the message when to the last the player
    /// told something, else `/tell`, the name and a comma, and the message (each literal's escapes
    /// taken off).
    Tell,
    /// Wear the item the send's request names (its item at slots 3 and 4) at its place (slot 7),
    /// as dropping it on the paper doll does.
    Wear,
}

/// The server functions the social window's requests are sent to.
mod social {
    pub const CREATE_FELLOWSHIP: u32 = 0x820C_001B;
    pub const DISBAND_FELLOWSHIP: u32 = 0x820C_0003;
    pub const LEAVE_FELLOWSHIP: u32 = 0x820C_001C;
    pub const RECRUIT: u32 = 0x820C_0015;
    pub const DISMISS: u32 = 0x820C_000E;
    pub const ACCEPT_RECRUITMENT: u32 = 0x820C_000D;
    pub const DECLINE_RECRUITMENT: u32 = 0x820C_001A;
    pub const QUERY_ALLEGIANCE: u32 = 0x8203_0005;
    pub const SWEAR_ALLEGIANCE: u32 = 0x8203_000A;
    pub const BREAK_ALLEGIANCE: u32 = 0x8203_0009;
    pub const ACCEPT_VASSAL: u32 = 0x8203_001C;
}

/// The recorded sends made again, by server function.
const MADE: &[(u32, Request)] = &[
    (SET_SELECTION, Request::Select),
    (START_ATTACK, Request::Attack),
    (0x820A_0007, Request::Examine),
    (DO_ATTACK, Request::SkillShortcut),
    (USE_ITEM, Request::UseItem),
    (SET_SHORTCUT, Request::PutInSlot),
    (UPLOAD_UI_SETTINGS, Request::UiSettings),
    (UPLOAD_GAME_OPTIONS, Request::GameOptions),
    (0x8204_0020, Request::Interface(0x00B4_00CF)),
    (0x8214_000C, Request::Interface(0x00B4_002E)),
    (0x8214_0002, Request::Interface(0x00B4_002B)),
    (0x8214_0007, Request::Interface(0x00B4_002C)),
    (0x8207_000E, Request::Interface(0x00B4_006B)),
    (0x8212_0030, Request::Interface(0x00B4_00F1)),
    (0x8212_0038, Request::Interface(0x00B4_00E4)),
    (0x8212_0041, Request::Interface(0x00B4_00E1)),
    (social::QUERY_ALLEGIANCE, Request::Social),
    (social::CREATE_FELLOWSHIP, Request::Social),
    (social::DISBAND_FELLOWSHIP, Request::Social),
    (social::LEAVE_FELLOWSHIP, Request::Social),
    (social::RECRUIT, Request::Social),
    (social::DISMISS, Request::Social),
    (social::ACCEPT_RECRUITMENT, Request::Social),
    (social::DECLINE_RECRUITMENT, Request::Social),
    (social::SWEAR_ALLEGIANCE, Request::Social),
    (social::BREAK_ALLEGIANCE, Request::Social),
    (social::ACCEPT_VASSAL, Request::Social),
    (0x8213_0027, Request::Interface(0x00B4_011B)),
    (0x8206_0006, Request::Interface(0x00B4_009D)),
    (0x8215_0015, Request::Interface(0x00B4_00BC)),
    (0x8212_0037, Request::Interface(0x00B4_00E5)),
    (0x820F_0002, Request::Interface(0x00B4_0016)),
    (0x8206_0012, Request::InterfaceAfter(0x00B4_009E, 1)),
    // The trade window's: open, close, accept (the scripts add what the trade holds), decline,
    // take back.
    (0x8215_002F, Request::Interface(0x00B4_00B8)),
    (0x8215_003E, Request::Interface(0x00B4_00B9)),
    (0x8215_0035, Request::InterfaceSome(0x00B4_00BA, 0, 0)),
    (0x8215_0023, Request::Interface(0x00B4_00BB)),
    (0x8215_003F, Request::Interface(0x00B4_00BD)),
    // The shop windows': the scripts add the shop's keeper (the send's last value), and a
    // consignment's shop after its price.
    (0x9B5E_000C, Request::InterfaceSome(0x00B4_00D4, 1, 0)),
    (0x9B5E_0022, Request::InterfaceSome(0x00B4_00D5, 1, 0)),
    (0x9B5E_002A, Request::InterfaceSome(0x00B4_00D6, 1, 0)),
    (0x9B5E_002B, Request::InterfaceSome(0x00B4_00D7, 1, 0)),
    (0x9B5E_0029, Request::InterfaceSome(0x00B4_00D8, 0, 1)),
    (0x9B5E_0023, Request::InterfaceSome(0x00B4_00D9, 1, 0)),
    (0x9B5E_0024, Request::InterfaceSome(0x00B4_00DA, 1, 0)),
    (0x9B5E_0028, Request::InterfaceSome(0x00B4_00DD, 1, 3)),
    (0x9B5E_003F, Request::InterfaceSome(0x00B4_00E0, 0, 2)),
    // A pet renamed; the story's scenes done.
    (0x8212_003A, Request::Interface(0x00B4_00E6)),
    (0x8213_0013, Request::Interface(0x00B4_0090)),
    // A container melted into money or emptied into the pack (the requests hold the container at
    // their slots 8 and 6), a container closed, an attack stopped.
    (0x820E_0065, Request::FromContainer(8, true)),
    (0x820E_0068, Request::FromContainer(6, false)),
    (0x820E_000C, Request::Close),
    (0x8204_0010, Request::StopAttack),
    // A craft skill raised; the chat options sent.
    (0x8206_0022, Request::Interface(0x00B4_012A)),
    (0x8212_0032, Request::ChatOptions),
    // A purchase from a shop.
    (0x9B5E_0031, Request::Purchase),
    // An item moved within a container or into another; following what is selected.
    (0x820E_0049, Request::Move),
    (0x820E_0036, Request::Move),
    (0x8212_0010, Request::Follow),
    (0x8205_0006, Request::Say),
    // An emote, a tell, an item worn.
    (0x8205_000B, Request::Emote),
    (0x8205_0002, Request::Tell),
    (0x820E_004F, Request::Wear),
];

/// The table of the emotes' command texts, as its enumeration value names it.
const EMOTE_COMMANDS: u32 = 0x4100_001C;

/// The away-from-the-keyboard emote: its typed command sets the player's away status instead of
/// playing it, so it is played as the friends window's status menu plays it.
const AWAY_EMOTE: u32 = 0x32B4_A467;

/// The emotes' table, as the client's data map its enumeration value.
fn emote_table(c: &mut Client) -> Option<u32> {
    let s = c.scripts.as_mut()?;
    Some(
        s.host
            .text
            .data_id_by_enum(EMOTE_COMMANDS, auberean_text::source::TABLE_MAPPING),
    )
}

/// The literal a string reference argument holds, as it is.
fn literal_of(v: Option<&Arg>) -> Option<Vec<u16>> {
    let Some(Arg::Object(g)) = v else {
        return None;
    };
    g.entries.iter().find_map(|e| match &e.body {
        Body::Object(o) => match &o.native {
            Some(NativeObject::TextInfo(t)) => t.literal.clone(),
            _ => None,
        },
        _ => None,
    })
}

/// The slots of the player's object where its scripts keep the last name that told the player
/// something and the last name the player told something.
const LAST_TELLER: usize = 49;
const LAST_TELLEE: usize = 48;

/// The literal of the name the player's scripts keep at a slot of the player's object.
fn kept_name(c: &Client, slot: usize) -> Option<Vec<u16>> {
    let player = c.model.player?;
    let s = c.scripts.as_ref()?;
    let instance = *s.host.instances.get(&player)?;
    let r = *s.vm.heap.object(instance).ok()?.slots.get(slot)?;
    match s.vm.heap.native(r).ok()? {
        NativeObject::TextInfo(t) => t.literal.clone(),
        _ => None,
    }
}

/// The literal a string reference argument holds, as typed: its escapes taken off (a backslash
/// keeps the mark after it from being read as one).
fn typed_literal(v: Option<&Arg>) -> Option<String> {
    let literal = literal_of(v)?;
    let text = String::from_utf16(&literal).ok()?;
    let mut typed = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        typed.extend(if ch == '\\' { chars.next() } else { Some(ch) });
    }
    Some(typed)
}

/// An emote's command as typed: the first word of its command text (the words the scripts match
/// a typed command against).
fn emote_command(c: &mut Client, emote: u32) -> Option<String> {
    let table = emote_table(c)?;
    let s = c.scripts.as_mut()?;
    let text = auberean_text::StringInfo::entry(table, emote).text(&mut s.host.text);
    text.split([' ', ',', ';', ':'])
        .find(|w| !w.is_empty())
        .map(str::to_lowercase)
}

/// Where the options window reads an option of its character page: a bit of the options (or its
/// opposite), a bit of the radar's mask, the range, the rows.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Word {
    Bit(u32),
    NotBit(u32),
    Radar(u32),
    Range,
    Height,
}

/// The character page's options, in its order.
const PAGE: &[(PlayerOption, Word)] = &[
    (PlayerOption::AutoTarget, Word::Bit(0x20)),
    (PlayerOption::DamageNumbers, Word::Bit(0x4)),
    (PlayerOption::OthersDamageNumbers, Word::Bit(0x8)),
    (PlayerOption::PlayerKillerDamage, Word::Bit(0x1000)),
    (PlayerOption::OthersDamageRange, Word::Range),
    (PlayerOption::RadarMonsters, Word::Radar(0x1)),
    (PlayerOption::RadarPeople, Word::Radar(0x2)),
    (PlayerOption::RadarPlayers, Word::Radar(0x4)),
    (PlayerOption::RadarPortals, Word::Radar(0x8)),
    (PlayerOption::RadarLifestones, Word::Radar(0x10)),
    (PlayerOption::RadarItems, Word::Radar(0x20)),
    (PlayerOption::RadarCoordinates, Word::Bit(0x10)),
    (PlayerOption::TakeAllegianceOffers, Word::Bit(0x40)),
    (PlayerOption::TakeFellowshipOffers, Word::Bit(0x80)),
    (PlayerOption::TakeTradeOffers, Word::Bit(0x100)),
    (PlayerOption::ShortcutNumbers, Word::Bit(0x1)),
    (PlayerOption::ShortcutLocked, Word::Bit(0x800)),
    (PlayerOption::ShortcutInstantScroll, Word::Bit(0x2000)),
    (PlayerOption::AskBeforeMelting, Word::NotBit(0x400)),
    (PlayerOption::LatencyGraph, Word::Bit(0x4000)),
    (PlayerOption::FellowsOnMap, Word::Bit(0x8000)),
    (PlayerOption::PetPanel, Word::Bit(0x1_0000)),
    (PlayerOption::ClassicExamination, Word::Bit(0x2_0000)),
    (PlayerOption::ShortcutHeight, Word::Height),
];

/// How long a send the scripts made on their own waits to be matched with a recorded one.
const PATIENCE: f64 = 2.0;

fn client(data: &ClientData) -> Client {
    let settings = ClientSettings {
        login_server: std::net::SocketAddr::from(([127, 0, 0, 1], 9000)),
        account: Vec::new(),
        ticket: Vec::new(),
        timestamp: 0,
        session_key: [0; 64],
        // What the scripts send is what is compared.
        script_sends: true,
    };
    Client::new(settings, data.clone())
}

/// How one server function's recorded sends went.
#[derive(Default, Debug, Clone)]
struct Tally {
    recorded: u64,
    /// Sent by the player's scripts on their own (a server's call had them, or another request
    /// made again did) shortly before: the same bytes, or not.
    own_same: u64,
    own_differ: u64,
    /// Made again through the interface.
    made_same: u64,
    made_differ: u64,
    made_nothing: u64,
    /// Not made: no request here makes it.
    not_made: u64,
    stopped: BTreeMap<u32, u64>,
    other: BTreeMap<String, u64>,
}

/// A graph's entries with every reference to one of its own objects replaced by that object's
/// place in the graph (each side numbers its own objects).
fn by_place(g: &Graph) -> Vec<auberean_script::graph::Entry> {
    let place: std::collections::HashMap<u32, u32> = g
        .ids
        .iter()
        .enumerate()
        .map(|(i, id)| (*id, u32::try_from(i).unwrap_or(0) | 0x8000_0000))
        .collect();
    let mut f = |r: u32| place.get(&r).copied().unwrap_or(r);
    g.entries
        .iter()
        .map(|e| {
            let mut e = e.clone();
            match &mut e.body {
                Body::Object(o) => {
                    for (v, t) in o.slots.iter_mut().zip(&o.tags) {
                        if auberean_script::value::tag::is_reference(*t) {
                            *v = f(*v);
                        }
                    }
                    if let Some(n) = &mut o.native {
                        n.remap(&mut f);
                    }
                }
                Body::Relative(r) => *r = f(*r),
                _ => {}
            }
            e
        })
        .collect()
}

/// Two argument buffers carry the same values: objects compared by what they hold, not by the ids
/// they are packed under (each side numbers its own objects).
fn same_values(a: &[u8], b: &[u8], lib: &auberean_script::library::Library) -> bool {
    if a == b {
        return true;
    }
    let (Ok(x), Ok(y)) = (args::decode(a, lib), args::decode(b, lib)) else {
        return false;
    };
    x.len() == y.len()
        && x.iter().zip(&y).all(|(p, q)| match (p, q) {
            (Arg::Object(g), Arg::Object(h)) => by_place(g) == by_place(h),
            _ => p == q,
        })
}

/// The packed bytes of the first native object a graph holds, with its type.
fn native_of(g: &Graph) -> Option<(u32, Vec<u8>)> {
    g.entries.iter().find_map(|e| match &e.body {
        Body::Object(o) => match &o.native {
            Some(NativeObject::Packed { ty, bytes }) => Some((*ty, bytes.clone())),
            _ => None,
        },
        _ => None,
    })
}

/// The entries of the first word table a graph holds, in packed order.
fn word_table(g: &Graph) -> Vec<(u32, u32)> {
    g.entries
        .iter()
        .find_map(|e| match &e.body {
            Body::Object(o) => match &o.native {
                Some(NativeObject::Hash(h)) => Some(
                    h.entries()
                        .filter_map(|(k, v)| match (k, v) {
                            (Item::Word(k), Item::Word(v)) => Some((*k, *v)),
                            _ => None,
                        })
                        .collect(),
                ),
                _ => None,
            },
            _ => None,
        })
        .unwrap_or_default()
}

/// The item a use carries and what it is used on: its root's third and fourth slots, and its
/// tenth and eleventh (0 for nothing).
fn used_item(g: &Graph) -> Option<(u64, u64)> {
    g.entries.first().and_then(|e| match &e.body {
        Body::Object(o) if o.slots.len() > 10 => Some((
            u64::from(o.slots[2]) | u64::from(o.slots[3]) << 32,
            u64::from(o.slots[9]) | u64::from(o.slots[10]) << 32,
        )),
        _ => None,
    })
}

/// The first toolbar slot whose shortcut is of a kind and holds a value.
fn slot_holding(
    c: &mut Client,
    k: u32,
    holds: impl Fn(&auberean_script::options::Shortcut) -> bool,
) -> Option<u32> {
    let all = c.shortcuts()?;
    all.iter()
        .position(|s| s.kind == k && holds(s))
        .and_then(|i| u32::try_from(i).ok())
}

/// The scripts' sends not yet matched with a recorded one: function, bytes, when.
type Pending = BTreeMap<u32, VecDeque<(Vec<u8>, f64)>>;

fn take_sends(c: &Client, seen: &mut usize, pending: &mut Pending) {
    for s in &c.sent()[*seen..] {
        if let ClientMessage::ServerCall(sc) = &s.message {
            pending
                .entry(sc.function.0)
                .or_default()
                .push_back((sc.args.0.clone(), s.at));
        }
    }
    *seen = c.sent().len();
}

/// The server function the next target in a fight is sent to.
const SET_NEXT_TARGET: u32 = 0x8204_0001;
/// The server function an examination is asked of.
const QUERY_EXAMINATION: u32 = 0x820A_0007;
/// The scripts' notice that the player's options were read again.
const RELOAD_OPTIONS: u32 = 0x0085 << 16 | 0x0009;
/// The option bit of the classic examination window (every tab at once).
const CLASSIC_EXAMINATION: u32 = 0x2_0000;
/// The examination window's tabs: all of them (the classic window), and the first tab's.
const ALL_TABS: u32 = 0x3FF;
const FIRST_TAB: u32 = 0x203;

/// The examination request a send carries.
fn examination_of(call: &[u8], lib: &auberean_script::library::Library) -> Option<Examination> {
    let v = args::decode(call, lib).ok()?;
    let Some(Arg::Object(g)) = v.first() else {
        return None;
    };
    native_of(g).map(|(_, b)| Examination::unpack(&b))
}

/// The recorded sessions replayed: the server's messages taken, the recorded player's requests
/// made again, what the scripts send compared with what the recorded client sent.
struct Replay<'a> {
    data: &'a ClientData,
    lib: &'a auberean_script::library::Library,
    c: Client,
    by: BTreeMap<u32, Tally>,
    seen: usize,
    pending: Pending,
    /// What the recorded player last selected.
    selected: u64,
    /// The recorded client's calls sent at one moment, held until the moment passes: the client
    /// sent them in an order the recording does not keep.
    batch: Vec<(f64, ScriptCall)>,
    /// The examination window is the classic one, as it starts; the scripts' notices read so far.
    classic: bool,
    notices: usize,
    /// The client's clock: the last moment taken.
    frame: f64,
    /// Saved states the clients read again after a read that failed once, and the ones that
    /// would not read at all.
    reread: u32,
    unread: std::collections::BTreeSet<u32>,
    /// The chat panel was made: the player's commands are known.
    commands_known: bool,
    /// The purchases the recorded player made, and those the scripts said it could afford.
    purchases: (u32, u32),
}

impl<'a> Replay<'a> {
    fn new(data: &'a ClientData, lib: &'a auberean_script::library::Library) -> Self {
        Self {
            data,
            lib,
            c: client(data),
            by: BTreeMap::new(),
            seen: 0,
            pending: Pending::new(),
            selected: 0,
            batch: Vec::new(),
            classic: true,
            notices: 0,
            frame: f64::NEG_INFINITY,
            reread: 0,
            unread: std::collections::BTreeSet::new(),
            commands_known: false,
            purchases: (0, 0),
        }
    }

    /// The client's clock moved on to `time`.
    fn advance(&mut self, time: f64) {
        if time > self.frame {
            self.frame = time;
            self.c.set_time(LocalTime(time));
            self.c.upkeep();
        }
    }

    /// A new recording: a new client.
    fn restart(&mut self) {
        self.flush();
        self.reads_done();
        self.c = client(self.data);
        self.seen = 0;
        self.pending.clear();
        self.selected = 0;
        self.classic = true;
        self.notices = 0;
        self.frame = f64::NEG_INFINITY;
        self.commands_known = false;
    }

    /// The client's saved states' reads, counted before it goes.
    fn reads_done(&mut self) {
        if let Some(s) = &self.c.scripts {
            self.reread += s.host.states.reread;
            self.unread
                .extend(s.host.states.failures.iter().map(|(id, _)| *id));
        }
    }

    /// The examination window hears the player's options read again: switched between the
    /// classic window and the tabbed one, it asks to examine with all tabs, or the first.
    fn options_read(&mut self) {
        let reloaded = self.c.notices[self.notices..]
            .iter()
            .any(|n| n.native == RELOAD_OPTIONS);
        self.notices = self.c.notices.len();
        if !reloaded {
            return;
        }
        let Some(options) = self.c.options() else {
            return;
        };
        let classic = options.bits & CLASSIC_EXAMINATION != 0;
        if classic != self.classic {
            self.classic = classic;
            let tabs = if classic { ALL_TABS } else { FIRST_TAB };
            self.c.act(Action::Examination { tabs: Some(tabs) });
        }
    }

    /// The calls held for one moment, in the order the client made them: a selection before the
    /// examination it sent (recorded after it at times).
    fn flush(&mut self) {
        let mut batch = std::mem::take(&mut self.batch);
        if let Some((time, _)) = batch.first() {
            self.advance(*time);
        }
        for i in 0..batch.len() {
            if batch[i].1.function != SET_SELECTION {
                continue;
            }
            let Some(target) = selection_of(&batch[i].1.args, self.lib) else {
                continue;
            };
            // Back to the last selection before it: the first examination of what it selects,
            // and the examinations sent just before that one.
            let from = batch[..i]
                .iter()
                .rposition(|(_, q)| q.function == SET_SELECTION)
                .map_or(0, |k| k + 1);
            let examines = |q: &ScriptCall| {
                q.function == QUERY_EXAMINATION
                    && examination_of(&q.args, self.lib).is_some_and(|r| r.object == target)
            };
            let Some(mut j) = (from..i).find(|k| examines(&batch[*k].1)) else {
                continue;
            };
            while j > from && batch[j - 1].1.function == QUERY_EXAMINATION {
                j -= 1;
            }
            let s = batch.remove(i);
            batch.insert(j, s);
        }
        for (time, call) in batch {
            self.call(time, &call);
        }
    }

    /// A recorded call of the client's, at `time`.
    fn call(&mut self, time: f64, call: &ScriptCall) {
        let lib = self.lib;
        if self.c.model.player_object().is_none() {
            return;
        }
        // The chat panel, made with the game's screen, makes the player's commands known.
        if !self.commands_known
            || self
                .c
                .with_host(|h| h.commands.names.is_empty())
                .unwrap_or(true)
        {
            self.commands_known = true;
            self.c.know_commands();
        }
        if call.function == SET_SELECTION {
            if let Some(t) = selection_of(&call.args, lib) {
                self.selected = t;
            }
        }
        let t = self.by.entry(call.function).or_default();
        t.recorded += 1;
        take_sends(&self.c, &mut self.seen, &mut self.pending);
        for q in self.pending.values_mut() {
            while q.front().is_some_and(|(_, at)| *at < time - PATIENCE) {
                q.pop_front();
            }
        }
        // Sends made at one moment may leave in another order: the same one anywhere in the
        // window first, else the oldest.
        let own = self.pending.get_mut(&call.function).and_then(|q| {
            let at = q
                .iter()
                .position(|(b, _)| same_values(b, &call.args, lib))
                .unwrap_or(0);
            q.remove(at)
        });
        if let Some((own, _)) = own {
            if same_values(&own, &call.args, lib) {
                t.own_same += 1;
            } else {
                t.own_differ += 1;
            }
            return;
        }
        let Some((_, request)) = MADE.iter().find(|(f, _)| *f == call.function) else {
            t.not_made += 1;
            return;
        };
        let Ok(values) = args::decode(&call.args, lib) else {
            return;
        };
        // The scripts' sends of this function already waiting (none the same as this one).
        let waiting = self.pending.get(&call.function).map_or(0, VecDeque::len);
        let c = &mut self.c;
        let result = match *request {
            Request::Select => {
                let [Arg::LongInt(target)] = values[..] else {
                    return;
                };
                let asked = c.asked.len();
                c.act(Action::Select { target });
                c.asked.get(asked).map(|(_, r)| r.clone())
            }
            Request::Examine => {
                let Some(r) = examination_of(&call.args, lib) else {
                    return;
                };
                // A request about nothing is the window closing; any other, the window opening
                // or its tabs changing, with the tabs it asks for.
                let tabs = (r.kind != 0).then_some(r.current);
                let asked = c.asked.len();
                c.act(Action::Examination { tabs });
                c.asked.get(asked).map(|(_, r)| r.clone())
            }
            Request::Attack | Request::StopAttack => {
                let asked = c.asked.len();
                let start = matches!(request, Request::Attack);
                c.act(Action::Attack { start });
                c.asked.get(asked).map(|(_, r)| r.clone())
            }
            Request::FromContainer(at, melt) => {
                let Some(Arg::Object(g)) = values.first() else {
                    return;
                };
                let Some(container) = g.entries.first().and_then(|e| match &e.body {
                    Body::Object(o) if o.slots.len() > at + 1 => {
                        Some(u64::from(o.slots[at]) | u64::from(o.slots[at + 1]) << 32)
                    }
                    _ => None,
                }) else {
                    return;
                };
                let asked = c.asked.len();
                c.act(if melt {
                    Action::MeltAll { container }
                } else {
                    Action::TakeAll { container }
                });
                c.asked.get(asked).map(|(_, r)| r.clone())
            }
            Request::ChatOptions => {
                // Last first: the popup flags, the devoted windows, the font colors, the font
                // sizes, the filters; each set by its own setter (the key, then the value), the
                // window's kinds and windows in their order (a key new to the profile goes to the
                // head of its chain, so the order shows in what is sent).
                const SETTERS: [u32; 5] = [
                    0x00B4_0119,
                    0x00B4_0117,
                    0x00B4_0116,
                    0x00B4_0115,
                    0x00B4_0083,
                ];
                if values.len() != SETTERS.len() {
                    return;
                }
                for (v, f) in values.iter().zip(SETTERS).rev() {
                    let Arg::Object(g) = v else {
                        return;
                    };
                    let mut entries = word_table(g);
                    entries.sort_unstable();
                    for (k, x) in entries {
                        c.interface(f, &[InterfaceArg::Word(x), InterfaceArg::Word(k)]);
                    }
                }
                c.interface(0x00B4_011C, &[]).map(|(r, _)| r)
            }
            Request::Move => {
                let Some(Arg::Object(g)) = values.first() else {
                    return;
                };
                let Some(Body::Object(o)) = g.entries.first().map(|e| &e.body) else {
                    return;
                };
                if o.slots.len() < 46 {
                    return;
                }
                let long = |k: usize| u64::from(o.slots[k]) | u64::from(o.slots[k + 1]) << 32;
                let asked = c.asked.len();
                c.act(Action::PutItem {
                    item: long(44),
                    from: long(16),
                    to: long(2),
                    old_slot: o.slots[39],
                    new_slot: o.slots[11],
                    quantity: o.slots[26],
                });
                c.asked.get(asked).map(|(_, r)| r.clone())
            }
            Request::Say => {
                let Some(text) = typed_literal(values.get(1)) else {
                    return;
                };
                c.process_text(&text);
                c.asked.last().map(|(_, r)| r.clone())
            }
            Request::Emote => {
                let [Arg::Int(emote)] = values[..] else {
                    return;
                };
                if emote == AWAY_EMOTE {
                    let Some(table) = emote_table(c) else {
                        return;
                    };
                    let asked = c.asked.len();
                    c.act(Action::Emote {
                        string: emote,
                        table,
                    });
                    c.asked.get(asked).map(|(_, r)| r.clone())
                } else {
                    let Some(word) = emote_command(c, emote) else {
                        return;
                    };
                    c.process_text(&format!("/{word}"));
                    c.asked.last().map(|(_, r)| r.clone())
                }
            }
            Request::Tell => {
                // Last first: the flags, the message, the name.
                let (Some(name), Some(message)) =
                    (typed_literal(values.get(2)), typed_literal(values.get(1)))
                else {
                    return;
                };
                // A reply or a tell again names whom the scripts keep, as it is.
                let held = literal_of(values.get(2));
                let line = if held.is_some() && held == kept_name(c, LAST_TELLER) {
                    format!("/reply {message}")
                } else if held.is_some() && held == kept_name(c, LAST_TELLEE) {
                    format!("/retell {message}")
                } else {
                    format!("/tell {name}, {message}")
                };
                c.process_text(&line);
                c.asked.last().map(|(_, r)| r.clone())
            }
            Request::Wear => {
                let Some(Arg::Object(g)) = values.first() else {
                    return;
                };
                let Some(Body::Object(o)) = g.entries.first().map(|e| &e.body) else {
                    return;
                };
                if o.slots.len() < 8 {
                    return;
                }
                let asked = c.asked.len();
                c.act(Action::Wear {
                    item: u64::from(o.slots[3]) | u64::from(o.slots[4]) << 32,
                    location: o.slots[7],
                });
                c.asked.get(asked).map(|(_, r)| r.clone())
            }
            Request::Follow => {
                c.process_text("/follow");
                c.asked.last().map(|(_, r)| r.clone())
            }
            Request::Purchase => {
                let Some(Arg::Object(g)) = values.first() else {
                    return;
                };
                let Some((item, quantity, slot)) = g.entries.first().and_then(|e| match &e.body {
                    Body::Object(o) if o.slots.len() > 9 => Some((
                        u64::from(o.slots[0]) | u64::from(o.slots[1]) << 32,
                        o.slots[5],
                        o.slots[9],
                    )),
                    _ => None,
                }) else {
                    return;
                };
                c.refresh_store();
                let most = c.store().max_purchase.get(&item).copied().unwrap_or(0);
                self.purchases.0 += 1;
                self.purchases.1 += u32::from(most >= quantity);
                // Pushed last first: the slot, the quantity, the item.
                let a = [
                    InterfaceArg::Word(slot),
                    InterfaceArg::Word(quantity),
                    InterfaceArg::Long(item),
                ];
                c.interface(0x00B4_00D0, &a);
                c.interface(0x00B4_00D1, &[InterfaceArg::Word(1)])
                    .map(|(r, _)| r)
            }
            Request::Close => {
                let [Arg::LongInt(container)] = values[..] else {
                    return;
                };
                let asked = c.asked.len();
                c.act(Action::SetOpen {
                    container,
                    open: false,
                });
                c.asked.get(asked).map(|(_, r)| r.clone())
            }
            Request::SkillShortcut => {
                let Some(Arg::Int(skill)) = values.get(1) else {
                    return;
                };
                let skill = *skill;
                let Some(slot) = slot_holding(c, kind::SKILL, |s| s.data_enum == skill) else {
                    *t.other.entry("no slot holds it".into()).or_default() += 1;
                    return;
                };
                let asked = c.asked.len();
                c.act(Action::UseToolbarSlot { slot });
                c.asked.get(asked).map(|(_, r)| r.clone())
            }
            Request::UseItem => {
                let Some((item, target)) = values.first().and_then(|v| match v {
                    Arg::Object(g) => used_item(g),
                    _ => None,
                }) else {
                    return;
                };
                let asked = c.asked.len();
                if item == self.selected {
                    c.act(Action::UseSelected);
                } else if let Some(slot) = slot_holding(c, kind::ITEM, |s| s.item == item) {
                    c.act(Action::UseToolbarSlot { slot });
                } else {
                    *t.other
                        .entry("neither selected nor in a slot".into())
                        .or_default() += 1;
                    return;
                }
                let first = c.asked.get(asked).map(|(_, r)| r.clone());
                // An item used on something waits for its target: the player selects it.
                let used = c.sent()[self.seen.min(c.sent().len())..].iter().any(|s| {
                    matches!(&s.message, ClientMessage::ServerCall(sc) if sc.function.0 == USE_ITEM)
                });
                if target != 0 && !used {
                    let at = c.asked.len();
                    c.act(Action::Select { target });
                    c.asked.get(at).map(|(_, r)| r.clone()).or(first)
                } else {
                    first
                }
            }
            Request::PutInSlot => {
                // The values come last first: the shortcut, then the slot.
                let (Some(Arg::Object(g)), Some(Arg::Int(slot))) = (values.first(), values.get(1))
                else {
                    return;
                };
                let Some((ty, bytes)) = native_of(g) else {
                    return;
                };
                // Pushed in the order they were sent: the shortcut, then the slot.
                let a = [
                    InterfaceArg::Native { ty, bytes },
                    InterfaceArg::Word(*slot),
                ];
                c.interface(function::SET_SHORTCUT, &a).map(|(r, _)| r)
            }
            Request::UiSettings => {
                // Last first: the opacities, then the places.
                let (Some(Arg::Object(o)), Some(Arg::Object(l))) = (values.first(), values.get(1))
                else {
                    return;
                };
                let Some((ty, bytes)) = native_of(l) else {
                    return;
                };
                let places = [InterfaceArg::Native { ty, bytes }];
                c.interface(function::SET_UI_SAVE_LOCATIONS, &places);
                for (window, opacity) in word_table(o) {
                    let a = [InterfaceArg::Word(opacity), InterfaceArg::Word(window)];
                    c.interface(function::SET_WINDOW_OPACITY, &a);
                }
                c.interface(function::UPLOAD_UI_SETTINGS, &[])
                    .map(|(r, _)| r)
            }
            Request::GameOptions => {
                // Last first: the option bits, the radar's mask, the height, the range.
                let [Arg::Int(bits), Arg::Int(radar), Arg::Int(height), Arg::Float(range)] =
                    values[..]
                else {
                    return;
                };
                let Some(now) = c.options() else { return };
                // A bit the options window does not show (the names over heads, by their key)
                // set by its own setter first.
                for (bit, f) in OPTION_SETTERS {
                    let on = bits & bit != 0;
                    if on != (now.bits & bit != 0)
                        && !PAGE.iter().any(|(_, w)| *w == Word::Bit(*bit))
                    {
                        c.interface(*f, &[InterfaceArg::Word(u32::from(on))]);
                    }
                }
                // The options window's changes, in its order, as it applies them.
                let mut changes = Vec::new();
                for (option, word) in PAGE {
                    let (want, held) = match *word {
                        Word::Bit(b) => (u32::from(bits & b != 0), u32::from(now.bits & b != 0)),
                        Word::NotBit(b) => (u32::from(bits & b == 0), u32::from(now.bits & b == 0)),
                        Word::Radar(b) => (
                            u32::from(radar & b != 0),
                            u32::from(now.radar_mask & b != 0),
                        ),
                        Word::Range => (range, now.damage_text_range),
                        Word::Height => (height, now.shortcut_height),
                    };
                    if want != held {
                        changes.push((*option, want));
                    }
                }
                let at = c.asked.len();
                c.act(Action::SetOptions {
                    changes,
                    tell: true,
                });
                c.asked
                    .get(at..)
                    .and_then(|a| {
                        a.iter()
                            .rev()
                            .find(|(f, _)| *f == function::UPLOAD_GAME_OPTIONS)
                    })
                    .map(|(_, r)| r.clone())
            }
            Request::Social => {
                let Some(a) = social_action(call.function, &values) else {
                    *t.other.entry(format!("{values:?}")).or_default() += 1;
                    return;
                };
                let at = c.asked.len();
                c.act(Action::Social(a));
                c.asked.get(at).map(|(_, r)| r.clone())
            }
            Request::Interface(f)
            | Request::InterfaceAfter(f, _)
            | Request::InterfaceSome(f, _, _) => {
                // The values come in the order they were pushed, which the interface's function
                // takes them in; one the scripts add of their own (the target) comes first.
                let (skip, take) = match *request {
                    Request::InterfaceAfter(_, n) => (n, usize::MAX),
                    Request::InterfaceSome(_, n, k) => (n, k),
                    _ => (0, usize::MAX),
                };
                let a: Vec<InterfaceArg> = values
                    .iter()
                    .skip(skip)
                    .take(take)
                    .map(|v| match v {
                        Arg::LongInt(l) => InterfaceArg::Long(*l),
                        Arg::Int(w) | Arg::Float(w) => InterfaceArg::Word(*w),
                        Arg::Object(g) => match g.entries.first().map(|e| &e.body) {
                            Some(Body::Object(o)) if o.native.is_some() => {
                                let mut w = auberean_script::wire::Out::new();
                                let n = o.native.as_ref().map(|n| n.write(&mut w));
                                match n {
                                    Some(Ok(())) => InterfaceArg::Native {
                                        ty: u32::from(o.native_type),
                                        bytes: w.buf,
                                    },
                                    _ => InterfaceArg::Word(0),
                                }
                            }
                            _ => InterfaceArg::Word(0),
                        },
                        _ => InterfaceArg::Word(0),
                    })
                    .collect();
                c.interface(f, &a).map(|(r, _)| r)
            }
        };
        match result {
            Some(CallResult::Completed) => {}
            Some(CallResult::NotPorted(n)) => *t.stopped.entry(n).or_default() += 1,
            r => *t.other.entry(format!("{r:?}")).or_default() += 1,
        }
        take_sends(&self.c, &mut self.seen, &mut self.pending);
        // What the request sent: the same one among them first.
        let made = self.pending.get_mut(&call.function).and_then(|q| {
            let at = (waiting..q.len())
                .find(|i| same_values(&q[*i].0, &call.args, lib))
                .unwrap_or(waiting);
            q.remove(at)
        });
        match made {
            None => t.made_nothing += 1,
            Some((sent, _)) if same_values(&sent, &call.args, lib) => t.made_same += 1,
            Some(_) => t.made_differ += 1,
        }
    }
}

/// The social window's request a send is made by, from its values: a fellowship's name and its
/// settings, the one asked in or sent out, the reason a place or an oath is turned down, the
/// member a profile is asked about, the one broken with. Offers are answered about the one who
/// made them, and an oath sworn to the selection, which the scripts know.
fn social_action(function: u32, values: &[Arg]) -> Option<SocialAction> {
    use SocialAction as S;
    let id = || match values {
        [Arg::LongInt(id)] => Some(*id),
        _ => None,
    };
    Some(match function {
        social::CREATE_FELLOWSHIP => {
            let ints: Vec<u32> = values
                .iter()
                .filter_map(|v| match v {
                    Arg::Int(w) => Some(*w),
                    _ => None,
                })
                .collect();
            let name = values.iter().find_map(|v| match v {
                Arg::Object(g) => match g.entries.first().map(|e| &e.body) {
                    Some(Body::Object(o)) => match o.native.as_ref() {
                        Some(NativeObject::Text(units)) => Some(units.clone()),
                        _ => None,
                    },
                    _ => None,
                },
                _ => None,
            })?;
            // In the order the interface's function takes them: how loot is shared, then the
            // experience setting.
            let [looting, social] = ints[..] else {
                return None;
            };
            S::CreateFellowship {
                name,
                social: social != 0,
                looting,
            }
        }
        social::DISBAND_FELLOWSHIP => S::DisbandFellowship,
        social::LEAVE_FELLOWSHIP => S::LeaveFellowship,
        social::RECRUIT => S::Recruit(id()?),
        social::DISMISS => S::Dismiss(id()?),
        social::ACCEPT_RECRUITMENT => S::AcceptRecruitment,
        social::DECLINE_RECRUITMENT => match values.first() {
            Some(Arg::Int(why)) => S::DeclineRecruitment(*why),
            _ => return None,
        },
        social::QUERY_ALLEGIANCE => S::QueryAllegiance(id()?),
        social::SWEAR_ALLEGIANCE => S::Swear,
        social::BREAK_ALLEGIANCE => S::Break(id()?),
        social::ACCEPT_VASSAL => S::TakeOath,
        _ => return None,
    })
}

/// The object a selection's send names.
fn selection_of(call: &[u8], lib: &auberean_script::library::Library) -> Option<u64> {
    match args::decode(call, lib).ok()?[..] {
        [Arg::LongInt(t)] => Some(t),
        _ => None,
    }
}

/// Every selection the recorded player sent in 2013 and 2017 is sent again with the same argument
/// bytes: by the player's scripts on their own, when a server's call before it had them select
/// (ending a trade deselects) or a corpse placed in the world took the selection from its
/// creature, else by the interface's selection made again at that moment (the recorded server's
/// messages before it taken); the replay places a corpse as its message arrives where the client
/// waits for its parts to load, so a few selections fall the other way. The other requests made again (attacking, the
/// examination window, skills, resurrection, pets, allegiance, quests) and what the scripts send on
/// their own are counted, and these families send what the recorded client sent, each at least so
/// many times: examining what is selected, the window opened and closed and its tabs changed (the
/// classic window or the tabbed one as the player's options say); the social window's requests
/// through its own actions (an allegiance's profile asked for, a fellowship made, broken up and
/// left, someone asked in, a place taken, an oath sworn, taken and broken), none different; the
/// next target in a fight;
/// starting an attack; attacking with the skill a toolbar slot holds; using an item, selected or in
/// a slot, on its target when it has one; putting a shortcut in a slot; the windows' places and
/// opacities and the game options, as the interface keeps them, sent; a recipe's ingredients and
/// its making; an item offered in trade; a pet's mode; coins put in the money bag. The calls the
/// recorded client sent at one moment are made in the order it made them (a selection before the
/// examination it asked for). The player's body is where the recorded player reported it. The
/// trade window's requests (open, close, accept with what the trade holds, decline, take back),
/// the pets' (attack, return, rename), the story's scenes done, a shop's (leave, pages, enter
/// from a catalog, leave a catalog, a purchase and its question answered), a container's (melt
/// everything, take everything, close), stopping an attack, raising a craft skill, the chat
/// options (set as their window sets them, then sent), items moved and worn, and the lines typed
/// (following, saying, emotes, tells; the away emote from the friends window's status menu) are
/// made again too.
///
/// Behaviour: script.model.selecting-sends-what-the-recorded-client-sent
#[test]
fn selecting_what_the_recorded_player_selected_sends_what_the_recorded_client_sent() {
    let data = data().expect("the September 2013 install opens");
    let lib = &data.scripts.as_ref().expect("the scripts").1.library;
    let index = auberean_testkit::data::require_capture_index();
    let mut types: Vec<u32> = (0x1_0000..0x1_0100)
        .filter(|t| ServerMessage::is_known_type(*t))
        .collect();
    types.extend([PLAY_CHARACTER, MY_POSITION, carrier::TO_SERVER]);
    let mut capture = -1;
    let mut r = Replay::new(&data, lib);
    captures::messages_in_time_order(&index, &types, &[2013, 2017], |m| {
        if m.capture != capture {
            capture = m.capture;
            r.restart();
        }
        let call = (!m.to_client)
            .then(|| m.script_call())
            .flatten()
            .filter(|_| m.message_type == carrier::TO_SERVER);
        if let Some(call) = call {
            if r.batch.first().is_some_and(|(t, _)| *t != m.time) {
                r.flush();
            }
            r.batch.push((m.time, call));
            return;
        }
        r.flush();
        r.advance(m.time);
        if m.to_client {
            if let Ok(msg) = ServerMessage::decode(&m.raw) {
                r.c.take_message(m.blob_id, msg);
            }
            r.options_read();
            return;
        }
        // Where the recorded player's body was: the client's own report of it.
        if m.message_type == MY_POSITION {
            if let Ok(ClientMessage::Position(p)) = ClientMessage::decode(&m.raw) {
                let heading = (p.heading_motion >> 24) as f32 * 360.0 / 255.0;
                r.c.body.follow(p.cell, p.origin, heading);
            }
            return;
        }
        if m.message_type == PLAY_CHARACTER {
            if let Ok(ClientMessage::PlayCharacter(p)) = ClientMessage::decode(&m.raw) {
                r.c.act(Action::PlayCharacter {
                    character: p.character,
                });
                r.seen = r.c.sent().len();
                r.pending.clear();
            }
        }
    })
    .expect("the index reads");
    r.flush();
    r.reads_done();
    eprintln!(
        "saved states read again after a failed read: {}; never read: {:x?}",
        r.reread, r.unread
    );
    let by = &r.by;
    let mut rows: Vec<(&u32, &Tally)> = by.iter().collect();
    rows.sort_by_key(|(_, t)| std::cmp::Reverse(t.recorded));
    eprintln!("function    recorded  own same/differ  made same/differ/nothing  not made  stopped");
    let socials = [
        social::CREATE_FELLOWSHIP,
        social::DISBAND_FELLOWSHIP,
        social::LEAVE_FELLOWSHIP,
        social::RECRUIT,
        social::DISMISS,
        social::ACCEPT_RECRUITMENT,
        social::DECLINE_RECRUITMENT,
        social::QUERY_ALLEGIANCE,
        social::SWEAR_ALLEGIANCE,
        social::BREAK_ALLEGIANCE,
        social::ACCEPT_VASSAL,
    ];
    let made_here: Vec<u32> = MADE.iter().map(|(f, _)| *f).collect();
    for (f, t) in rows
        .iter()
        .enumerate()
        .filter(|(i, (f, _))| *i < 40 || made_here.contains(f) || socials.contains(f))
        .map(|(_, r)| r)
    {
        let stops: Vec<String> = t
            .stopped
            .iter()
            .map(|(n, k)| format!("{k} {}", lib.native_name(*n)))
            .collect();
        eprintln!(
            "{f:08X}  {:>8}  {:>8}/{:<6}  {:>8}/{}/{:<6}  {:>8}  {stops:?} {:?}",
            t.recorded,
            t.own_same,
            t.own_differ,
            t.made_same,
            t.made_differ,
            t.made_nothing,
            t.not_made,
            t.other
        );
    }
    // What the recorded player bought, the scripts said it could: the shop window asks before it
    // buys.
    assert!(
        r.purchases.0 > 0 && r.purchases.1 == r.purchases.0,
        "{:?}",
        r.purchases
    );
    // A corpse placed in the world takes the player's selection when its creature was selected:
    // those selections are the scripts' own. The replay places a corpse as its message arrives,
    // the client once its parts have loaded, so a few selections (and the uses, attacks and
    // examinations of the selection that follow them) fall the other way. The recording does not
    // show when a corpse's parts finished loading (the client sends nothing then), so the replay
    // cannot place it at that moment.
    let s = by.get(&SET_SELECTION).cloned().unwrap_or_default();
    assert!(s.own_same >= 3_400, "{s:?}");
    assert!(s.made_same + s.own_same >= 63_450, "{s:?}");
    assert!(s.made_differ + s.own_differ <= 30, "{s:?}");
    assert!(s.made_nothing <= 15, "{s:?}");
    // Each family made again: at least this many sends the same.
    for (function, at_least) in [
        (START_ATTACK, 4_000),
        (DO_ATTACK, 40_700),
        (USE_ITEM, 21_850),
        (UPLOAD_UI_SETTINGS, 17_400),
        (UPLOAD_GAME_OPTIONS, 1_000),
        (SET_SHORTCUT, 980),
        (0x8206_0012, 1_050),
        (0x8206_0006, 560),
        (0x820F_0002, 830),
        (0x8215_0015, 330),
        (0x8212_0037, 275),
        // The pets' attack and return; the trade window's accept, open, decline and take-back;
        // the story's scenes done; a shop left, its pages, a store entered, the catalog left; a
        // pet renamed.
        (0x8212_0041, 940),
        (0x8212_0038, 440),
        (0x8215_0035, 165),
        (0x8215_002F, 100),
        (0x8215_0023, 2),
        (0x8215_003F, 3),
        (0x8213_0013, 115),
        (0x9B5E_000C, 29),
        (0x9B5E_0023, 16),
        (0x9B5E_0024, 5),
        (0x9B5E_0029, 11),
        (0x9B5E_0022, 7),
        (0x8212_003A, 12),
        // A container melted into money, emptied into the pack, closed; an attack stopped; a
        // craft skill raised; the chat options sent; a purchase.
        (0x820E_0065, 3_380),
        (0x820E_0068, 155),
        (0x820E_000C, 2_250),
        (0x8204_0010, 1_130),
        (0x8206_0022, 85),
        (0x8212_0032, 18),
        (0x9B5E_0031, 20),
        // An item moved within a container and into another; following what is selected (the
        // command typed once the chat panel made the commands known); a line said.
        (0x820E_0049, 1_550),
        (0x820E_0036, 950),
        (0x8212_0010, 1_680),
        (0x8205_0006, 135),
        // An emote (typed, or the away emote from the friends window's status menu); a tell
        // (typed: a reply, a tell again, or a new one); an item worn.
        (0x8205_000B, 200),
        (0x8205_0002, 60),
        (0x820E_004F, 38),
    ] {
        let t = by.get(&function).cloned().unwrap_or_default();
        assert!(t.made_same >= at_least, "{function:#010x}: {t:?}");
    }
    // Closing a trade: the window's, or the scripts' own when the partner's closing comes first.
    let c = by.get(&0x8215_003E).cloned().unwrap_or_default();
    assert!(c.made_same + c.own_same >= 26, "{c:?}");
    // The social window's requests, made through its own actions: every one the same.
    for (function, at_least) in [
        (social::QUERY_ALLEGIANCE, 1_650),
        (social::ACCEPT_RECRUITMENT, 55),
        (social::RECRUIT, 48),
        (social::CREATE_FELLOWSHIP, 25),
        (social::DISBAND_FELLOWSHIP, 12),
    ] {
        let t = by.get(&function).cloned().unwrap_or_default();
        assert!(t.made_same >= at_least, "{function:#010x}: {t:?}");
    }
    for f in socials {
        let t = by.get(&f).cloned().unwrap_or_default();
        assert_eq!(t.made_differ + t.made_nothing, 0, "{f:#010x}: {t:?}");
    }
    // Examining: the scripts' own on a selection, the window's when it opens, closes or changes
    // its tabs.
    let e = by.get(&QUERY_EXAMINATION).cloned().unwrap_or_default();
    assert!(e.own_same + e.made_same >= 106_450, "{e:?}");
    assert!(e.own_differ + e.made_differ <= 100, "{e:?}");
    // The next target the scripts pick in a fight, sent on their own.
    let n = by.get(&SET_NEXT_TARGET).cloned().unwrap_or_default();
    assert!(n.own_same >= 6_500, "{n:?}");
}

/// The quality that says an object may be selected.
const SELECTABLE: u32 = 0x270;

/// Each object the recorded 2013 and 2017 players selected, at the moment they selected it, is
/// one the client's qualities say may be selected: most servers' descriptions leave the flag
/// out, and the object's qualities record answers it from the record's own description (50,735
/// of 51,013; 3,445 answered by the object's description; 56 the qualities say may not be, each
/// one the server had said so of in an update shortly before, which the description takes; 222
/// not known to the client then).
///
/// Behaviour: script.model.what-the-recorded-player-selected-may-be-selected
#[test]
fn what_the_recorded_player_selected_may_be_selected() {
    let data = data().expect("the September 2013 install opens");
    let lib = &data.scripts.as_ref().expect("the scripts").1.library;
    let index = auberean_testkit::data::require_capture_index();
    let mut types: Vec<u32> = (0x1_0000..0x1_0100)
        .filter(|t| ServerMessage::is_known_type(*t))
        .collect();
    types.extend([PLAY_CHARACTER, carrier::TO_SERVER]);
    let mut capture = -1;
    let mut c = client(&data);
    let mut frame = f64::NEG_INFINITY;
    // Selectable, not, not known; of those, how many the description itself answered.
    let (mut yes, mut no, mut unknown, mut by_description) = (0u64, 0u64, 0u64, 0u64);
    captures::messages_in_time_order(&index, &types, &[2013, 2017], |m| {
        if m.capture != capture {
            capture = m.capture;
            c = client(&data);
            frame = f64::NEG_INFINITY;
        }
        if m.time > frame {
            frame = m.time;
            c.set_time(LocalTime(m.time));
            c.upkeep();
        }
        if m.to_client {
            if let Ok(msg) = ServerMessage::decode(&m.raw) {
                c.take_message(m.blob_id, msg);
            }
            return;
        }
        if m.message_type == PLAY_CHARACTER {
            if let Ok(ClientMessage::PlayCharacter(p)) = ClientMessage::decode(&m.raw) {
                c.act(Action::PlayCharacter {
                    character: p.character,
                });
            }
            return;
        }
        let Some(call) = m.script_call() else { return };
        if call.function != SET_SELECTION || c.model.player_object().is_none() {
            return;
        }
        let Ok(v) = args::decode(&call.args, lib) else {
            return;
        };
        let [Arg::LongInt(target)] = v[..] else {
            return;
        };
        if target == 0 || Some(target) == c.model.player {
            return;
        }
        if c.model.object(target).is_some_and(|o| {
            auberean_client_runtime::description::bool(&o.weenie, SELECTABLE).is_some()
        }) {
            by_description += 1;
        }
        match c.bool_quality(target, SELECTABLE) {
            Some(true) => yes += 1,
            Some(false) => no += 1,
            None => unknown += 1,
        }
    })
    .expect("the index reads");
    eprintln!("selected: selectable {yes}, not {no}, not known {unknown}; {by_description} by their description");
    assert!(yes >= 50_700, "{yes} {no} {unknown}");
    assert!(no <= 60 && unknown <= 230, "{yes} {no} {unknown}");
}

/// Every notice the library's scripts send (105 natives) is one the client reads, with as many
/// words as each of its call sites pushes; the client binds one more no script calls. Three are
/// heard by every panel: portal space, the portal's scene and opening the examination panel.
///
/// Behaviour: script.natives.every-notice-is-read-with-the-words-its-call-sites-push
#[test]
fn every_notice_is_read_with_the_words_its_call_sites_push() {
    let data = data().expect("the September 2013 install opens");
    let lib = &data.scripts.as_ref().expect("the scripts").1.library;
    let slots = argument_slots(lib);
    let mut called = 0;
    for (id, n) in &slots {
        if !is_panel_notice(lib, *id) {
            continue;
        }
        called += 1;
        let r = reading(*id).unwrap_or_else(|| panic!("{}", lib.native_name(*id)));
        assert_eq!(r.slots, *n, "{}", lib.native_name(*id));
    }
    assert_eq!(called, 105);
    assert_eq!(NOTICES.len(), 106);
    // The interface's own package: portal space, opening the examination panel, the portal's
    // scene.
    let everyone: Vec<u32> = NOTICES
        .iter()
        .filter(|n| n.hearers == Hearers::Everyone)
        .map(|n| n.native)
        .collect();
    let interface = |n: u32| (0x91 << 16) | n;
    assert_eq!(everyone, [interface(1), interface(2), interface(3)]);
}

/// The player's private flag that it rides a mount.
const MOUNTED: u32 = 2002;

/// A prefab emote's server call.
const PREFAB_EMOTE: u32 = 0x8205_000B;

/// The wave's command text in the emotes' table (`ID_EmoteWaveCmd`).
const WAVE_EMOTE: u32 = token("ID_EmoteWaveCmd");

/// A string table key's number: each byte shifted to the top seven bits, rotated right seven bits
/// more than the byte before, all xored.
const fn token(name: &str) -> u32 {
    let b = name.as_bytes();
    let (mut hash, mut shift, mut i) = (0u32, 0u32, 0);
    while i < b.len() {
        hash ^= ((b[i] as u32) << 25).rotate_right(shift);
        shift = (shift + 7) & 31;
        i += 1;
    }
    hash
}

/// The custom emote command's name (in the commands' table, by its enumeration value).
const CUSTOM_EMOTE_COMMAND: (u32, u32) = (0x4100_0007, 0x28E8_995D);

/// What a rider asked of its scripts and what came of it.
#[derive(Default, Debug)]
struct Rides {
    /// Recorded mounts with the player in the world, and those that came while the private stream
    /// held them.
    mounted: u64,
    held: u64,
    /// The lines shown, by the request and the line, and the server calls the requests sent.
    lines: BTreeMap<(&'static str, String), u64>,
    sends: u64,
    /// The recorded clients' attacks, emotes and skill uses sent while riding, by function, and
    /// on foot.
    recorded_riding: BTreeMap<u32, u64>,
    recorded_on_foot: BTreeMap<u32, u64>,
}

/// A command's name as the player types it: its table entry's first word, lower case.
fn command_word(c: &mut Client, (table, string): (u32, u32)) -> Option<String> {
    let s = c.scripts.as_mut()?;
    let table = s
        .host
        .text
        .data_id_by_enum(table, auberean_text::source::TABLE_MAPPING);
    let text = auberean_text::StringInfo::entry(table, string).text(&mut s.host.text);
    text.split([' ', ',', ';', ':'])
        .find(|w| !w.is_empty())
        .map(str::to_lowercase)
}

/// At every recorded mount (the server's private flag 2002 set with the player in the world),
/// the player's scripts refuse an attack, an emote from the emote menu, a typed prefab emote and a
/// typed custom emote with the client's own lines ("You cannot attack while you are on your
/// mount.", "You cannot perform emotes while you are on your mount."), and send nothing: no server
/// recorded those lines. The recorded clients started no attack and sent no emote while riding
/// (their only calls of the kind then were two skill uses).
///
/// Behaviour: script.model.a-rider-is-refused-attacks-and-emotes-by-its-scripts
#[test]
fn a_rider_is_refused_attacks_and_emotes_by_its_scripts() {
    let data = data().expect("the September 2013 install opens");
    let index = auberean_testkit::data::require_capture_index();
    let mut types: Vec<u32> = (0x1_0000..0x1_0100)
        .filter(|t| ServerMessage::is_known_type(*t))
        .collect();
    types.extend([PLAY_CHARACTER, carrier::TO_SERVER]);
    let mut capture = -1;
    let mut c = client(&data);
    let mut riding = false;
    let mut commands = false;
    let mut frame = f64::NEG_INFINITY;
    let mut t = Rides::default();
    captures::messages_in_time_order(&index, &types, &[2013, 2017], |m| {
        if m.capture != capture {
            capture = m.capture;
            c = client(&data);
            riding = false;
            commands = false;
            frame = f64::NEG_INFINITY;
        }
        if m.time > frame {
            frame = m.time;
            c.set_time(LocalTime(m.time));
            c.upkeep();
        }
        if !m.to_client {
            if m.message_type == carrier::TO_SERVER {
                let call = m
                    .script_call()
                    .map(|s| s.function)
                    .filter(|f| [START_ATTACK, DO_ATTACK, PREFAB_EMOTE].contains(f));
                if let Some(f) = call {
                    let by = if riding {
                        &mut t.recorded_riding
                    } else {
                        &mut t.recorded_on_foot
                    };
                    *by.entry(f).or_default() += 1;
                }
            } else if let Ok(ClientMessage::PlayCharacter(p)) = ClientMessage::decode(&m.raw) {
                c.act(Action::PlayCharacter {
                    character: p.character,
                });
            }
            return;
        }
        let Ok(msg) = ServerMessage::decode(&m.raw) else {
            return;
        };
        let mounts = match &msg {
            ServerMessage::UpdateBool(q) if q.quality == MOUNTED => Some(q.value != 0),
            _ => None,
        };
        c.take_message(m.blob_id, msg);
        let Some(now) = mounts else {
            return;
        };
        riding = now;
        if !now || c.model.player_object().is_none() {
            return;
        }
        // Held behind a missing private stamp, the mount has not taken effect yet.
        let on_mount = c
            .model
            .player_object()
            .is_some_and(|o| o.qualities.bools.get(&MOUNTED).is_some_and(|v| *v != 0));
        if !on_mount {
            t.held += 1;
            return;
        }
        t.mounted += 1;
        // Made known with each session's play screen.
        if !commands || c.with_host(|h| h.commands.names.is_empty()).unwrap_or(true) {
            c.know_commands();
            commands = true;
        }
        c.shown.clear();
        let before = c.sent().len();
        let mut heard = |c: &mut Client, request: &'static str| {
            for s in std::mem::take(&mut c.shown) {
                let line = c.write_text(&s.text).unwrap_or_default();
                *t.lines.entry((request, line)).or_default() += 1;
            }
        };
        c.act(Action::Attack { start: true });
        heard(&mut c, "attack");
        if let Some(table) = emote_table(&mut c) {
            c.act(Action::Emote {
                string: AWAY_EMOTE,
                table,
            });
        }
        heard(&mut c, "menu emote");
        if let Some(word) = emote_command(&mut c, WAVE_EMOTE) {
            c.process_text(&format!("/{word}"));
        }
        heard(&mut c, "typed emote");
        if let Some(word) = command_word(&mut c, CUSTOM_EMOTE_COMMAND) {
            c.process_text(&format!("/{word} waves from the saddle"));
        }
        heard(&mut c, "custom emote");
        t.sends += c.sent()[before..]
            .iter()
            .filter(|s| matches!(s.message, ClientMessage::ServerCall(_)))
            .count() as u64;
    })
    .expect("the index reads");
    eprintln!("{t:?}");
    let attack = "You cannot attack while you are on your mount.
";
    let emote = "You cannot perform emotes while you are on your mount.
";
    assert!(t.mounted >= 200, "{t:?}");
    assert!(t.held * 100 < t.mounted, "{t:?}");
    let shown = |request: &'static str, line: &str| t.lines.get(&(request, line.to_owned()));
    assert_eq!(shown("attack", attack), Some(&t.mounted), "{t:?}");
    for request in ["menu emote", "typed emote", "custom emote"] {
        assert_eq!(shown(request, emote), Some(&t.mounted), "{request}: {t:?}");
    }
    assert_eq!(t.lines.len(), 4, "{t:?}");
    assert_eq!(t.sends, 0, "{t:?}");
    for f in [START_ATTACK, PREFAB_EMOTE] {
        assert_eq!(t.recorded_riding.get(&f), None, "{t:?}");
        assert!(t.recorded_on_foot.get(&f).is_some_and(|n| *n > 0), "{t:?}");
    }
}

/// The client's report of where the player is.
const CLIENT_POSITION: u32 = 0x1008E;

/// Each recorded 2017 player's reports carry the stamp of the last move the server ordered its
/// body to make, and so do this client's, fed the same messages: after the first order, 99 in 100
/// recorded reports carry the stamp this client keeps (the rest follow orders the recordings
/// missed, or went out just before an order arrived). Only captures that hold their player's
/// creation are counted.
///
/// Behaviour: session.position.recorded-reports-carry-the-stamp-of-the-last-move-ordered
#[test]
fn recorded_reports_carry_the_stamp_of_the_last_move_ordered() {
    let data = data().expect("the September 2013 install opens");
    let data = ClientData {
        scripts: None,
        ..data
    };
    let index = auberean_testkit::data::require_capture_index();
    let mut types: Vec<u32> = (0x1_0000..0x1_0100)
        .filter(|t| ServerMessage::is_known_type(*t))
        .collect();
    types.extend([PLAY_CHARACTER, CLIENT_POSITION]);
    let mut capture = -1;
    let mut c = client(&data);
    let mut frame = f64::NEG_INFINITY;
    let (mut before, mut before_zero, mut after, mut agree) = (0u64, 0u64, 0u64, 0u64);
    // Whether the capture holds its player's creation: one that began later began after the
    // recorded client had kept a stamp.
    let (mut player, mut created) = (None, false);
    captures::messages_in_time_order(&index, &types, &[2017], |m| {
        if m.capture != capture {
            capture = m.capture;
            c = client(&data);
            (player, created) = (None, false);
            frame = f64::NEG_INFINITY;
        }
        if m.time > frame {
            frame = m.time;
            c.set_time(LocalTime(m.time));
        }
        if m.to_client {
            if let Ok(msg) = ServerMessage::decode(&m.raw) {
                match &msg {
                    ServerMessage::PlayerObject(p) => player = Some(p.id),
                    ServerMessage::CreateObject(o) => created |= Some(o.id) == player,
                    _ => {}
                }
                c.take_message(m.blob_id, msg);
            }
            return;
        }
        match ClientMessage::decode(&m.raw) {
            Ok(ClientMessage::PlayCharacter(p)) => c.act(Action::PlayCharacter {
                character: p.character,
            }),
            Ok(ClientMessage::Position(p)) if created => {
                if c.body.move_to_stamp == 0 {
                    before += 1;
                    before_zero += u64::from(p.move_to_stamp == 0);
                } else {
                    after += 1;
                    agree += u64::from(p.move_to_stamp == c.body.move_to_stamp);
                }
            }
            _ => {}
        }
    })
    .expect("the index reads");
    eprintln!(
        "{after} recorded reports after an order, {agree} carrying the stamp kept here; \
         {before} before any, {before_zero} carrying 0"
    );
    assert!(after > 1_000_000, "only {after} reports after an order");
    #[allow(clippy::cast_precision_loss)]
    let share = agree as f64 / after as f64;
    assert!(share >= 0.99, "{agree} of {after} agree");
}
