//! The pad's cross hotbars: bars of two halves of eight slots, a half's slots used by the d-pad
//! (up, right, down, left) and the face buttons (top, right, bottom, left) while its trigger is
//! held (LT the left half, RT the right). Both halves are drawn at the foot of the screen; the
//! half a trigger holds glows, stands a little larger and names what is in each slot.
//!
//! The stance chooses the bar: peace, melee and missile have one bar each, shown while the
//! stance is; magic has eight, numbered between the halves, which RB held with a face button or
//! the d-pad picks among (Y, B, A, X the first four; up, right, down, left the last four).
//!
//! A slot holds an item, a spell or one of the fighting stance's controls (lower or raise the
//! power, attack low, medium or high). It is bound by a drop on it with the mouse, or by the
//! bind-to-hotbar notice: the thing to bind shown, and the next slot used takes it.
//!
//! The sets are kept with the character on the server, as one text property of the player
//! module's gameplay-options collection (see [`SERVER_PROPERTY`] and [`encode`]), which the
//! server returns at login whichever client the character last played on. A copy is kept beside
//! the other settings, in `horizon-cross-<server>-<name>.txt`: at login the server's sets win,
//! and where the server has none yet the copy is read and sent up once.

use dereth_client_contract::UiRequest;
use dereth_primitives::ObjectId;

use crate::art::{Family, Sprite};
use crate::draw::{with_alpha, Rect, WHITE};
use crate::pad::CrossSet;
use crate::ui::game::{GameState, Item};
use crate::ui::kit::{self, Ctx};
use crate::ui::paint::{Align, Painter, TextStyle};
use crate::ui::Outcome;

use super::Hud;

/// The bars, by their place among the sets: peace's, melee's, missile's, then magic's eight.
pub const PEACE: usize = 0;
pub const MELEE: usize = 1;
pub const MISSILE: usize = 2;
pub const MAGIC: usize = 3;
/// How many bars magic has, and how many slots a bar has (two halves of eight).
pub const MAGIC_BARS: usize = 8;
pub const SLOTS: usize = 16;
/// Every bar.
pub const SETS: usize = MAGIC + MAGIC_BARS;

/// The melee and missile bars' slots until the player changes them: the right half's d-pad left
/// lowers and right raises the power (a missile weapon's accuracy), X, A and B attack low,
/// medium and high. The left half is empty.
pub const STANCE_DEFAULTS: [(usize, PowerAct); 5] = [
    (8 + 3, PowerAct::Lower),
    (8 + 1, PowerAct::Raise),
    (8 + 7, PowerAct::Low),
    (8 + 6, PowerAct::Medium),
    (8 + 5, PowerAct::High),
];

/// One of the fighting stance's controls, put on a slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerAct {
    /// Lower the attack's power (a missile weapon's accuracy).
    Lower,
    /// Raise it.
    Raise,
    /// Attack low, medium or high.
    Low,
    Medium,
    High,
}

impl PowerAct {
    pub const ALL: [Self; 5] = [
        Self::Lower,
        Self::Raise,
        Self::Low,
        Self::Medium,
        Self::High,
    ];

    /// Its word in the settings file.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Lower => "lower",
            Self::Raise => "raise",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    /// Its name on its slot: a missile weapon aims rather than powers.
    #[must_use]
    pub const fn name(self, missile: bool) -> &'static str {
        match (self, missile) {
            (Self::Lower, false) => "Lower Power",
            (Self::Raise, false) => "Raise Power",
            (Self::Lower, true) => "Lower Accuracy",
            (Self::Raise, true) => "Raise Accuracy",
            (Self::Low, _) => "Low",
            (Self::Medium, _) => "Medium",
            (Self::High, _) => "High",
        }
    }

    /// Its picture's piece, for the weapon in hand (`missile` or not).
    #[must_use]
    pub const fn piece(self, missile: bool) -> &'static str {
        match (self, missile) {
            (Self::Lower, false) => "stance.lower-power",
            (Self::Raise, false) => "stance.raise-power",
            (Self::Lower, true) => "stance.lower-accuracy",
            (Self::Raise, true) => "stance.raise-accuracy",
            (Self::Low, _) => "stance.low",
            (Self::Medium, _) => "stance.medium",
            (Self::High, _) => "stance.high",
        }
    }

    /// The game's action it is, for the weapon in hand (`missile` or not).
    #[must_use]
    pub const fn action(self, missile: bool) -> u32 {
        use dereth_client_contract::actions::mapped as m;
        let id = match (self, missile) {
            (Self::Lower, false) => m::COMBAT_DECREASE_ATTACK_POWER,
            (Self::Raise, false) => m::COMBAT_INCREASE_ATTACK_POWER,
            (Self::Lower, true) => m::COMBAT_DECREASE_MISSILE_ACCURACY,
            (Self::Raise, true) => m::COMBAT_INCREASE_MISSILE_ACCURACY,
            (Self::Low, _) => m::COMBAT_LOW_ATTACK,
            (Self::Medium, _) => m::COMBAT_MEDIUM_ATTACK,
            (Self::High, _) => m::COMBAT_HIGH_ATTACK,
        };
        id.0
    }
}

/// The player-module property the sets are kept in on the server: a text property the game's
/// property table declares and the game's own interface never reads, so a client that does not
/// know it keeps it and writes it back unchanged.
pub const SERVER_PROPERTY: u32 = 0x0000_00E5;

/// What the server's copy starts with: the format and its version. Text without it is not the
/// sets.
const SERVER_TAG: &str = "dxhb2";
/// The first version: eight sets the player picked among and a stance set for melee and missile.
const SERVER_TAG_1: &str = "dxhb1";

/// What a slot holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossBind {
    Item(ObjectId),
    Spell(u32),
    Power(PowerAct),
    /// The character: an item armed to be used on something is used on them.
    Myself,
}

/// The cross hotbars.
#[derive(Debug, Default)]
pub struct CrossBars {
    /// Each bar's sixteen slots, the bars in the order of [`PEACE`] to [`MAGIC`]'s eight: the
    /// left half's eight, then the right's, each half in the order of
    /// [`crate::pad::CROSS_BUTTONS`].
    pub sets: Vec<[Option<CrossBind>; SLOTS]>,
    /// The magic bar picked, from 0.
    pub set: usize,
    /// The bar shown, by its place among the sets, as the stance chooses it.
    pub shown: usize,
    /// The thing the bind-to-hotbar notice is binding, while it is up.
    pub binding: Option<CrossBind>,
    /// Where each slot was drawn this frame, with its slot number.
    pub drawn: Vec<(Rect, usize)>,
    /// The character the sets were read for, and their file.
    loaded_for: Option<String>,
    path: Option<std::path::PathBuf>,
    /// The character whose sets have been matched with the server's, and whether a change
    /// waits to go up to it.
    server_for: Option<String>,
    to_server: bool,
}

impl CrossBars {
    /// What slot `slot` of the bar shown holds.
    #[must_use]
    pub fn slot(&self, slot: usize) -> Option<CrossBind> {
        self.sets
            .get(self.shown)
            .and_then(|s| s.get(slot).copied().flatten())
    }

    /// Bind `bind` to slot `slot` of the bar shown, and keep it.
    pub fn assign(&mut self, slot: usize, bind: Option<CrossBind>) {
        if slot >= SLOTS {
            return;
        }
        while self.sets.len() < SETS {
            self.sets.push([None; SLOTS]);
        }
        let bar = self.shown.min(SETS - 1);
        self.sets[bar][slot] = bind;
        self.save();
    }

    /// Show the bar the stance `combat_mode` has: peace's, melee's, missile's, or the magic bar
    /// picked.
    pub fn follow_mode(&mut self, combat_mode: u32) {
        use dereth_client_contract::combat_mode as c;
        self.shown = match combat_mode {
            c::MELEE => MELEE,
            c::MISSILE => MISSILE,
            c::MAGIC => MAGIC + self.set.min(MAGIC_BARS - 1),
            _ => PEACE,
        };
    }

    /// Whether a magic bar is shown, which RB with a button picks among.
    #[must_use]
    pub fn magic(&self) -> bool {
        self.shown >= MAGIC
    }

    /// Pick magic bar `set` (from 0), while magic's bars are shown; in another stance it does
    /// nothing.
    pub fn pick(&mut self, set: usize) {
        if self.magic() && set < MAGIC_BARS && set != self.set {
            self.set = set;
            self.shown = MAGIC + set;
            self.save();
        }
    }

    /// Read the sets kept for character `name` on `server` in `dir`, once per character.
    pub fn load_for(&mut self, dir: Option<&std::path::Path>, server: &str, name: &str) {
        let key = format!("{server}|{name}");
        if name.is_empty() || self.loaded_for.as_deref() == Some(key.as_str()) {
            return;
        }
        let file = |parts: &[&str]| {
            let safe: String = parts
                .join("-")
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect();
            dir.map(|d| d.join(format!("horizon-cross-{safe}.txt")))
        };
        // Kept per server and character, as two servers may each have a character of one name;
        // a file kept by the character's name alone is read when there is none yet.
        self.path = file(&[server, name]);
        let read = |p: Option<std::path::PathBuf>| {
            p.and_then(|p| dereth_client_runtime::platform::files::read_to_string(&p).ok())
        };
        let text = read(self.path.clone())
            .or_else(|| read(file(&[name])))
            .unwrap_or_default();
        let (sets, set) = parse(&text);
        self.sets = sets;
        self.set = set.min(MAGIC_BARS - 1);
        self.loaded_for = Some(key);
    }

    /// Keep the sets: on the server soon, and in the file at once.
    fn save(&mut self) {
        self.to_server = true;
        self.write_file();
    }

    fn write_file(&self) {
        if let Some(path) = &self.path {
            if let Err(e) = std::fs::write(path, to_text(&self.sets, self.set)) {
                tracing::warn!("the cross hotbars were not kept: {e}");
            }
        }
    }

    /// Match the sets with the server's copy, `server` (`None` until the player module has
    /// arrived; then the property's text, if the module has it), and give what should go up to
    /// the server now, if anything.
    ///
    /// The first time for a character, the server's sets win and replace those read from the
    /// file; with none on the server, those from the file go up instead. After that every change
    /// goes up.
    pub fn sync_server(&mut self, server: Option<Option<&str>>) -> Option<String> {
        let key = self.loaded_for.clone()?;
        if self.server_for.as_deref() != Some(key.as_str()) {
            let server = server?;
            self.server_for = Some(key);
            match server.and_then(decode) {
                Some((sets, set)) => {
                    self.sets = sets;
                    self.set = set.min(MAGIC_BARS - 1);
                    if self.magic() {
                        self.shown = MAGIC + self.set;
                    }
                    self.to_server = false;
                    self.write_file();
                }
                None => self.to_server = true,
            }
        }
        if !std::mem::take(&mut self.to_server) {
            return None;
        }
        Some(encode(&self.sets, self.set))
    }
}

/// One bar's slots as the server's copy writes them: sixteen comma-separated slots, each
/// `i<id>` (an item), `s<id>` (a spell), both in hex, `c<n>` (a stance control, by its place in
/// [`PowerAct::ALL`]) or nothing.
fn bar_text(set: &[Option<CrossBind>; SLOTS]) -> String {
    set.iter()
        .map(|bind| match bind {
            None => String::new(),
            Some(CrossBind::Item(id)) => format!("i{:x}", id.0),
            Some(CrossBind::Spell(id)) => format!("s{id:x}"),
            Some(CrossBind::Power(p)) => {
                let n = PowerAct::ALL.iter().position(|a| a == p).unwrap_or(0);
                format!("c{n}")
            }
            Some(CrossBind::Myself) => "m".to_owned(),
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// [`bar_text`] read back; a slot it cannot read is left empty.
fn read_bar(text: &str) -> [Option<CrossBind>; SLOTS] {
    let mut set = [None; SLOTS];
    for (slot, word) in set.iter_mut().zip(text.split(',')) {
        *slot = match word.split_at_checked(1) {
            Some(("i", hex)) => u32::from_str_radix(hex, 16)
                .ok()
                .map(|v| CrossBind::Item(ObjectId(v))),
            Some(("s", hex)) => u32::from_str_radix(hex, 16).ok().map(CrossBind::Spell),
            Some(("c", n)) => n
                .parse::<usize>()
                .ok()
                .and_then(|n| PowerAct::ALL.get(n).copied())
                .map(CrossBind::Power),
            Some(("m", "")) => Some(CrossBind::Myself),
            _ => None,
        };
    }
    set
}

/// The server's copy of `sets`, `magic` the magic bar picked:
/// `dxhb2;<magic>;<peace>|<melee>|<missile>|<magic 1>|...|<magic 8>`, the magic bar counted
/// from 1, each bar as [`bar_text`] writes it. Plain ASCII, under two kilobytes for full bars.
#[must_use]
pub fn encode(sets: &[[Option<CrossBind>; SLOTS]], magic: usize) -> String {
    let empty = [None; SLOTS];
    let all: Vec<String> = (0..SETS)
        .map(|i| bar_text(sets.get(i).unwrap_or(&empty)))
        .collect();
    format!("{SERVER_TAG};{};{}", magic + 1, all.join("|"))
}

/// The bars and the magic bar picked in the server's copy, or `None` for text that is not one.
/// The first version's eight sets become magic's bars and its stance set the melee and missile
/// bars, its set shown the magic bar picked.
#[must_use]
pub fn decode(text: &str) -> Option<(Vec<[Option<CrossBind>; SLOTS]>, usize)> {
    let mut parts = text.splitn(3, ';');
    let tag = parts.next()?;
    if tag != SERVER_TAG && tag != SERVER_TAG_1 {
        return None;
    }
    let shown = parts
        .next()?
        .parse::<usize>()
        .ok()
        .and_then(|n| n.checked_sub(1))
        .unwrap_or(0);
    let bars: Vec<[Option<CrossBind>; SLOTS]> = parts.next()?.split('|').map(read_bar).collect();
    let mut sets = vec![[None; SLOTS]; SETS];
    if tag == SERVER_TAG {
        for (set, bar) in sets.iter_mut().zip(bars) {
            *set = bar;
        }
        return Some((sets, if shown < MAGIC_BARS { shown } else { 0 }));
    }
    for (i, bar) in bars.into_iter().enumerate().take(MAGIC_BARS + 1) {
        if i < MAGIC_BARS {
            sets[MAGIC + i] = bar;
        } else {
            sets[MELEE] = bar;
            sets[MISSILE] = bar;
        }
    }
    Some((sets, if shown < MAGIC_BARS { shown } else { 0 }))
}

/// The names of the bars in the settings file, in the sets' order.
fn bar_name(i: usize) -> String {
    match i {
        PEACE => "peace".to_owned(),
        MELEE => "melee".to_owned(),
        MISSILE => "missile".to_owned(),
        _ => format!("magic{}", i - MAGIC + 1),
    }
}

/// The bars and the magic bar picked, as a settings file has them: `bars=2`, `set=<n>` (the
/// magic bar, from 1), then one line a bound slot, `<bar> <slot> item <id>`, `... spell <id>` or
/// `... power <word>`, the bar by its name (`peace`, `melee`, `missile`, `magic1` to `magic8`) and
/// the slot from 1. A line it cannot read is left out. A file of the first kind, without `bars=`,
/// numbered its sets 1 to 9: the first eight become magic's bars, the ninth the melee and missile
/// bars. A melee or missile bar the file says nothing of has its defaults.
#[must_use]
pub fn parse(text: &str) -> (Vec<[Option<CrossBind>; SLOTS]>, usize) {
    let mut sets = vec![[None; SLOTS]; SETS];
    let mut shown = 0;
    let second = text.lines().any(|l| l.trim() == "bars=2");
    let mut named = [false; SETS];
    for line in text.lines() {
        if let Some(n) = line.trim().strip_prefix("set=") {
            shown = n
                .trim()
                .parse::<usize>()
                .ok()
                .and_then(|n| n.checked_sub(1))
                .filter(|n| *n < MAGIC_BARS)
                .unwrap_or(0);
            continue;
        }
        let f: Vec<&str> = line.split_whitespace().collect();
        let [bar, slot, kind, value] = f[..] else {
            continue;
        };
        let bars: Vec<usize> = if second {
            (0..SETS).filter(|i| bar_name(*i) == bar).collect()
        } else {
            match bar.parse::<usize>().ok().and_then(|n| n.checked_sub(1)) {
                Some(n) if n < MAGIC_BARS => vec![MAGIC + n],
                Some(n) if n == MAGIC_BARS => vec![MELEE, MISSILE],
                _ => Vec::new(),
            }
        };
        let Some(slot) = slot
            .parse::<usize>()
            .ok()
            .and_then(|n| n.checked_sub(1))
            .filter(|n| *n < SLOTS)
        else {
            continue;
        };
        let bind = match kind {
            "item" => value
                .parse::<u32>()
                .ok()
                .map(|v| CrossBind::Item(ObjectId(v))),
            "spell" => value.parse::<u32>().ok().map(CrossBind::Spell),
            "power" => PowerAct::ALL
                .into_iter()
                .find(|p| p.word() == value)
                .map(CrossBind::Power),
            "self" => Some(CrossBind::Myself),
            _ => None,
        };
        for b in bars {
            named[b] = true;
            if bind.is_some() {
                sets[b][slot] = bind;
            }
        }
    }
    for bar in [MELEE, MISSILE] {
        if !named[bar] {
            for (slot, act) in STANCE_DEFAULTS {
                sets[bar][slot] = Some(CrossBind::Power(act));
            }
        }
    }
    (sets, shown)
}

/// The settings file's text for `sets`, `magic` the magic bar picked.
#[must_use]
pub fn to_text(sets: &[[Option<CrossBind>; SLOTS]], magic: usize) -> String {
    use std::fmt::Write as _;
    let mut text = format!("bars=2\nset={}\n", magic + 1);
    for (s, set) in sets.iter().enumerate() {
        let mut any = false;
        for (i, bind) in set.iter().enumerate() {
            let Some(bind) = bind else {
                continue;
            };
            any = true;
            let (kind, value) = match bind {
                CrossBind::Item(id) => ("item", id.0.to_string()),
                CrossBind::Spell(id) => ("spell", id.to_string()),
                CrossBind::Power(p) => ("power", p.word().to_owned()),
                CrossBind::Myself => ("self", "-".to_owned()),
            };
            let _ = writeln!(text, "{} {} {kind} {value}", bar_name(s), i + 1);
        }
        // A melee or missile bar emptied is kept empty, not given its defaults again.
        if !any && (s == MELEE || s == MISSILE) {
            let _ = writeln!(text, "{} 1 none -", bar_name(s));
        }
    }
    text
}

/// The item `id` among what the character carries and wears.
fn find_item(state: &GameState, id: ObjectId) -> Option<&Item> {
    state
        .main_pack
        .iter()
        .chain(state.pack.iter())
        .chain(state.side_packs.iter().map(|(p, _)| p))
        .chain(state.side_packs.iter().flat_map(|(_, items)| items.iter()))
        .chain(state.equipped.iter())
        .find(|i| i.id == id)
}

/// Whether the weapon in hand is a missile weapon: the missile stance.
fn missile(state: &GameState) -> bool {
    state.combat_mode == dereth_client_contract::combat_mode::MISSILE
}

/// What a slot's picture, name and use are.
fn look(p: &Painter<'_>, state: &GameState, bind: CrossBind) -> (Option<Sprite>, String) {
    match bind {
        // An item's own picture, without the tile under it, centred in the slot's well.
        CrossBind::Item(id) => match find_item(state, id) {
            Some(it) => (
                p.art
                    .ac_item_drag(it)
                    .or_else(|| it.ac_icon.and_then(|d| p.art.ac_icon(d))),
                it.name.clone(),
            ),
            None => (None, String::new()),
        },
        CrossBind::Spell(id) => match state.spells.iter().find(|s| s.id == id) {
            Some(s) => (
                p.art
                    .ac_spell(s.ac_icon, s.icon_power, s.bitfield)
                    .or_else(|| p.art.ac_icon(s.ac_icon)),
                s.name.clone(),
            ),
            None => (None, String::new()),
        },
        CrossBind::Power(a) => (None, a.name(missile(state)).to_owned()),
        CrossBind::Myself => (
            state.main_pack.as_ref().and_then(|m| p.art.ac_item_drag(m)),
            state.name.clone(),
        ),
    }
}

/// Use what slot holds: an item used (or aimed at, while a use waits for its target), a spell
/// cast, a stance control tapped.
pub fn use_bind(state: &GameState, bind: CrossBind, out: &mut Outcome) {
    // An item armed to be used on something: this slot's thing is what it is used on.
    if state.targeting {
        // The armed item's own slot again puts it down.
        if matches!(bind, CrossBind::Item(id) if state.armed == Some(id)) {
            out.requests.push(UiRequest::SetTargetMode(
                dereth_client_contract::view::TargetMode::None,
            ));
            return;
        }
        let on = match bind {
            CrossBind::Myself => state.player_id,
            CrossBind::Item(id) => Some(id),
            _ => None,
        };
        if let Some(on) = on {
            out.requests.push(UiRequest::ExecuteTargetItem(on));
            return;
        }
    }
    match bind {
        // The character, used: what they carry is shown.
        CrossBind::Myself => out.actions.push(INVENTORY_ACTION),
        // The main pack, used, shows what it holds.
        CrossBind::Item(id) if state.main_pack.as_ref().is_some_and(|m| m.id == id) => {
            out.actions.push(INVENTORY_ACTION);
        }
        CrossBind::Item(id) => out.requests.push(if state.targeting {
            UiRequest::ExecuteTargetItem(id)
        } else {
            UiRequest::Use(id)
        }),
        CrossBind::Spell(spell_id) => out.requests.push(UiRequest::CastSpell { spell_id }),
        CrossBind::Power(a) => out.action_taps.push(a.action(missile(state))),
    }
}

/// How far each half's middle stands from the middle between them, in layout units: close
/// enough that the halves frame the stance button, the set's number and the power bar between
/// them without touching them.
const HALF_FROM_MIDDLE: f32 = 205.0;

/// How much further out a diamond's side slots stand than its top and bottom.
const DIAMOND_WIDE: f32 = 1.3;

/// The game's action that shows or hides the inventory.
const INVENTORY_ACTION: u32 = 0x1000_0019;

/// The slot's place in its diamond: up, right, down, left round the middle.
const ROUND: [(f32, f32); 4] = [(0.0, -1.0), (1.0, 0.0), (0.0, 1.0), (-1.0, 0.0)];

/// The face buttons' letters and colours, top, right, bottom, left.
const FACE: [(&str, u32); 4] = [
    ("Y", 0xFFE8_C040),
    ("B", 0xFFD8_4A3A),
    ("A", 0xFF5C_C04A),
    ("X", 0xFF4A_8CE0),
];

impl Hud {
    /// The cross hotbars, in gamepad mode: both halves of the set shown, the set's number between
    /// them, the half a trigger holds lit and named; a slot's button uses it, or binds it while the
    /// bind-to-hotbar notice is up; RB with a button picks the set; a melee or missile stance shows
    /// the stance set.
    pub fn cross_bars(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        out: &mut Outcome,
    ) {
        self.cross
            .load_for(self.settings_dir.as_deref(), &state.host, &state.name);
        let server = state.player_module_strings.as_ref().map(|all| {
            all.iter()
                .find(|(id, _)| *id == SERVER_PROPERTY)
                .map(|(_, text)| text.as_str())
        });
        if let Some(value) = self.cross.sync_server(server) {
            out.requests.push(UiRequest::SetPlayerModuleString {
                property: SERVER_PROPERTY,
                value,
            });
        }
        self.cross.follow_mode(state.combat_mode);
        if let Some(set) = ctx.input.pad.pick.take() {
            self.cross.pick(set);
        }
        let fired = std::mem::take(&mut ctx.input.pad.fired);
        let active = ctx.input.pad.set;
        if let Some(half) = active {
            let base = if half == CrossSet::Left { 0 } else { 8 };
            for at in fired {
                self.cross_slot_used(state, base + at, out);
            }
        }
        self.cross.drawn.clear();
        let k = p.scale;
        let (sw, sh) = p.screen;
        let mid = sw / 2.0 + 12.0 * k;
        let cy = sh - 135.0 * k;
        let label = (self.cross.set + 1).to_string();
        // The set's badge between the halves, with nothing drawn over the stance and power
        // controls there.
        if self.cross.magic() {
            draw_set_number(p, (mid, cy), &label);
        }
        for half in [CrossSet::Left, CrossSet::Right] {
            let lit = active == Some(half);
            let centre = mid
                + if half == CrossSet::Left {
                    -HALF_FROM_MIDDLE
                } else {
                    HALF_FROM_MIDDLE
                } * k;
            // The half held up stands a little larger.
            let s = if lit { 1.12 } else { 1.0 };
            let slot = 40.0 * k * s;
            let step = slot + 3.0 * k * s;
            // A diamond's side slots stand further out than its top and bottom, so it reads
            // wider than tall.
            let across = step * DIAMOND_WIDE;
            let gap = across + slot / 2.0 + 6.0 * k * s;
            let plate = Rect::new(
                centre - gap - across - slot / 2.0 - 8.0 * k,
                cy - 1.5 * step - 8.0 * k,
                2.0 * (gap + across + slot / 2.0 + 8.0 * k),
                3.0 * step + 16.0 * k,
            );
            draw_half_backplate(p, plate, lit, ctx.time);
            let mut names = Vec::new();
            for at in 0..8 {
                let (dx, dy) = ROUND[at % 4];
                let cx = centre + if at < 4 { -gap } else { gap };
                let r = Rect::new(
                    cx + dx * across - slot / 2.0,
                    cy + dy * step - slot / 2.0,
                    slot,
                    slot,
                );
                let index = if half == CrossSet::Left { at } else { 8 + at };
                let bind = self.cross.slot(index);
                let (icon, name) = bind.map_or((None, String::new()), |b| look(p, state, b));
                let dimmed = active.is_some() && !lit;
                let clicked =
                    draw_slot(p, ctx, r, icon.as_ref(), bind, missile(state), lit, dimmed);
                // An item armed to be used on something glows until it is.
                if matches!(bind, Some(CrossBind::Item(id)) if state.armed == Some(id)) {
                    let a = 0.6 + 0.4 * crate::ui::wave(ctx.time, 4.0);
                    p.outline(r.inset(-3.0 * k), 3.0 * k, with_alpha(0xFF70_E0FF, a));
                }
                if lit {
                    draw_button_glyph(p, r, at);
                    if !name.is_empty() {
                        names.push((at, cx, name));
                    }
                }
                if clicked {
                    self.cross_slot_used(state, index, out);
                }
                ctx.drops
                    .push((r, Some(crate::ui::kit::Drop::CrossSlot(index))));
                self.cross.drawn.push((r, index));
            }
            for (label, name) in name_labels(p, &names, across, plate.y) {
                draw_name_label(p, label, &name);
            }
        }
        self.binding_notice(p, ctx, state);
    }

    /// Slot `slot` of the set shown used, by its button or a click: bound to what the
    /// bind-to-hotbar notice holds, while it is up; otherwise what it holds used.
    fn cross_slot_used(&mut self, state: &GameState, slot: usize, out: &mut Outcome) {
        if let Some(bind) = self.cross.binding.take() {
            self.cross.assign(slot, Some(bind));
        } else if let Some(bind) = self.cross.slot(slot) {
            use_bind(state, bind, out);
        }
    }

    /// The bind-to-hotbar notice, while something waits to be bound: what it is, and how to bind
    /// it or leave.
    fn binding_notice(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, state: &GameState) {
        let Some(bind) = self.cross.binding else {
            return;
        };
        let k = p.scale;
        let (sw, sh) = p.screen;
        let (icon, name) = look(p, state, bind);
        let w = 460.0 * k;
        let r = Rect::new(sw / 2.0 + 12.0 * k - w / 2.0, sh - 300.0 * k, w, 86.0 * k);
        if !p.halves("notice", r.inset(-10.0 * k), WHITE) {
            p.fill(r, 0xE010_0C08);
            p.outline(r, 1.0 * k, 0xFFC8_B27A);
        }
        let tile = Rect::new(r.x + 12.0 * k, r.y + 12.0 * k, 40.0 * k, 40.0 * k);
        draw_slot(
            p,
            ctx,
            tile,
            icon.as_ref(),
            Some(bind),
            missile(state),
            false,
            false,
        );
        let title = TextStyle::new(Family::Heading, 20.0, ctx.colours.heading()).edge(0xFF00_0000);
        let text = TextStyle::new(Family::Body, 13.0, 0xFFEE_E1C5).edge(0xFF00_0000);
        let key = TextStyle::new(Family::Body, 13.0, 0xFFF0_C860).edge(0xFF00_0000);
        p.text(
            &title,
            tile.right() + 12.0 * k,
            r.y + 10.0 * k,
            "Set to Hotbar",
        );
        p.text(&text, tile.right() + 12.0 * k, r.y + 36.0 * k, &name);
        let line = r.bottom() - 26.0 * k;
        let mut x = tile.right() + 12.0 * k;
        for (b, what) in [("LT/RT", "+ button: Assign to the slot"), ("B", "Exit")] {
            let marks = super::pad::button_marks(p, b, x, line + 9.0 * k, 18.0 * k);
            if marks > 0.0 {
                x += marks + 6.0 * k;
            } else {
                x += p.text(&key, x, line, b) + 6.0 * k;
            }
            x += p.text(&text, x, line, what) + 18.0 * k;
        }
    }
}

// The pieces the cross hotbars are drawn from, one function each, so each can be drawn from the
// interface's own art in its place.

/// One slot at `r`: its frame and what it holds (`icon`; a stance control, which has no picture,
/// by its name), lit while its half is held up, dimmed while the other half is. Whether it was
/// clicked.
#[allow(clippy::too_many_arguments)]
pub fn draw_slot(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    r: Rect,
    icon: Option<&Sprite>,
    bind: Option<CrossBind>,
    missile: bool,
    lit: bool,
    dimmed: bool,
) -> bool {
    let k = p.scale;
    let tint = if dimmed { 0xA0FF_FFFF } else { WHITE };
    // A stance control's own picture, where the art has one.
    let stance = match bind {
        Some(CrossBind::Power(a)) => p.piece(a.piece(missile)),
        _ => None,
    };
    let icon = icon.or(stance.as_ref());
    let over = ctx.over(&r);
    let state = if over && ctx.input.down[0] {
        "pressed"
    } else if lit {
        "lit"
    } else if bind.is_some() {
        "filled"
    } else {
        "empty"
    };
    let clicked = if let Some(frame) = p.piece(&format!("cross.slot.{state}")) {
        // The frame's rim fills `r`, its glow round it; the icon is the largest square whose
        // corners clear the well's rounded ones.
        let v = p
            .art
            .piece_value("cross.slot")
            .filter(|v| v.len() == 4)
            .unwrap_or_else(|| vec![48.0, 40.0, 1.9, 5.5]);
        let (canvas, rim, inset, radius) = (v[0], v[1], v[2], v[3]);
        let u = r.w / rim;
        let margin = (canvas - rim) / 2.0 * u;
        p.sprite(
            &frame,
            Rect::new(
                r.x - margin,
                r.y - margin,
                r.w + 2.0 * margin,
                r.h + 2.0 * margin,
            ),
            tint,
        );
        if let Some(i) = icon {
            let pad = (inset + 1.0 + radius * (1.0 - std::f32::consts::FRAC_1_SQRT_2)) * u;
            p.sprite(i, r.inset(pad), tint);
        }
        over && ctx.input.clicked(&r)
    } else {
        kit::icon_slot(p, ctx, r, icon, tint)
    };
    if let (Some(CrossBind::Power(a)), None) = (bind, stance.as_ref()) {
        let small = TextStyle::new(Family::Body, 10.0, 0xFFEE_E1C5).edge(0xFF00_0000);
        let words = a.name(missile).replace(' ', "\n");
        for (n, line) in words.lines().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            p.text_in(
                &small,
                Rect::new(r.x, r.y + 6.0 * k + n as f32 * 12.0 * k, r.w, 12.0 * k),
                Align::Centre,
                line,
            );
        }
    }
    clicked
}

/// The plate under a half, `r`: nothing while the half rests; glowing while its trigger holds it
/// up.
pub fn draw_half_backplate(p: &mut Painter<'_>, r: Rect, lit: bool, time: f64) {
    if !lit {
        return;
    }
    let k = p.scale;
    let a = 0.55 + 0.25 * crate::ui::wave(time, 5.0);
    // Warm light spreading out past the half, breathing a little.
    if let Some(glow) = p.piece("cross.glow") {
        let (gx, gy) = (r.w * 0.18, r.h * 0.4);
        p.sprite(
            &glow,
            Rect::new(r.x - gx, r.y - gy, r.w + 2.0 * gx, r.h + 2.0 * gy),
            with_alpha(WHITE, 0.75 + 0.25 * a),
        );
        return;
    }
    p.fill(r, with_alpha(0xFF30_5070, a * 0.5));
    p.outline(r, 2.0 * k, with_alpha(0xFF80_C0FF, a));
}

/// The set's number (or "Stance"), centred at `at` between the halves.
pub fn draw_set_number(p: &mut Painter<'_>, at: (f32, f32), label: &str) {
    let k = p.scale;
    // On the badge: the set's number, or the stance's word.
    let on_badge = TextStyle::new(Family::Heading, 18.0, 0xFFF4_E6C4).edge(0xFF00_0000);
    let word = label.to_uppercase();
    let w = (p.measure(&on_badge, &word) + 26.0 * k).max(34.0 * k);
    let badge = Rect::new(at.0 - w / 2.0, at.1 - 13.0 * k, w, 26.0 * k);
    if p.halves("badge.normal", badge, WHITE) {
        p.text_in(&on_badge, badge, Align::Centre, &word);
        return;
    }
    let number = TextStyle::new(Family::Heading, 26.0, 0xFFEE_E1C5).edge(0xFF00_0000);
    p.text_in(
        &number,
        Rect::new(at.0 - 40.0 * k, at.1 - 16.0 * k, 80.0 * k, 32.0 * k),
        Align::Centre,
        label,
    );
}

/// The button that uses slot `at` (as [`crate::pad::CROSS_BUTTONS`]), in its corner: the d-pad as
/// a small cross with its arm lit, a face button by its letter in its colour.
pub fn draw_button_glyph(p: &mut Painter<'_>, r: Rect, at: usize) {
    let k = p.scale;
    let glyph = if at < 4 {
        ["up", "right", "down", "left"][at]
    } else {
        ["y", "b", "a", "x"][at - 4]
    };
    if let Some(g) = p.piece(&format!("glyph.{glyph}")) {
        let side = 20.0 * k;
        p.sprite(
            &g,
            Rect::new(
                r.right() - side + 3.0 * k,
                r.bottom() - side + 3.0 * k,
                side,
                side,
            ),
            WHITE,
        );
        return;
    }
    if at < 4 {
        let (dx, dy) = ROUND[at];
        let c = (r.right() - 10.0 * k, r.bottom() - 10.0 * k);
        let arm = 4.0 * k;
        p.fill(
            Rect::new(c.0 - 1.5 * arm, c.1 - arm / 2.0, 3.0 * arm, arm),
            0xFF8C_8478,
        );
        p.fill(
            Rect::new(c.0 - arm / 2.0, c.1 - 1.5 * arm, arm, 3.0 * arm),
            0xFF8C_8478,
        );
        p.fill(
            Rect::new(
                c.0 - arm / 2.0 + dx * arm,
                c.1 - arm / 2.0 + dy * arm,
                arm,
                arm,
            ),
            0xFFF0_C860,
        );
    } else {
        let (letter, colour) = FACE[at - 4];
        let label = TextStyle::new(Family::Body, 12.0, colour).edge(0xFF00_0000);
        p.text_in(
            &label,
            Rect::new(
                r.right() - 15.0 * k,
                r.bottom() - 17.0 * k,
                14.0 * k,
                16.0 * k,
            ),
            Align::Centre,
            letter,
        );
    }
}

/// The style the slots' names are written in.
fn name_style() -> TextStyle {
    TextStyle::new(Family::Body, 12.0, 0xFFFF_FFFF).edge(0xFF00_1030)
}

/// Where the lit half's names go, above it (its top at `top`), laid out as the diamonds are:
/// each top slot's on the highest row, the side slots' on the row below over their own places
/// (`across` either side of their diamond's middle), each bottom slot's lowest. `names` holds each
/// slot's place in its half, its diamond's middle and its name. A row's labels that would touch
/// are pushed apart, and the row kept centred where its slots are.
fn name_labels(
    p: &Painter<'_>,
    names: &[(usize, f32, String)],
    across: f32,
    top: f32,
) -> Vec<(Rect, String)> {
    let k = p.scale;
    let style = name_style();
    let h = 20.0 * k;
    let row_h = h + 2.0 * k;
    let gap = 4.0 * k;
    let mut out = Vec::new();
    for rows_up in [3_u8, 2, 1] {
        let mut row: Vec<(f32, f32, String)> = names
            .iter()
            .filter_map(|(at, centre, name)| {
                let (x, up) = match at % 4 {
                    0 => (*centre, 3),
                    1 => (centre + across, 2),
                    2 => (*centre, 1),
                    _ => (centre - across, 2),
                };
                (up == rows_up).then(|| {
                    let name = p.fit(&style, name, 130.0 * k);
                    (x, p.measure(&style, &name) + 24.0 * k, name)
                })
            })
            .collect();
        if row.is_empty() {
            continue;
        }
        row.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut lefts: Vec<f32> = Vec::with_capacity(row.len());
        for (i, (x, w, _)) in row.iter().enumerate() {
            let mut left = x - w / 2.0;
            if let Some(prev) = i.checked_sub(1) {
                left = left.max(lefts[prev] + row[prev].1 + gap);
            }
            lefts.push(left);
        }
        #[allow(clippy::cast_precision_loss)]
        let n = row.len() as f32;
        let wanted: f32 = row.iter().map(|(x, _, _)| x).sum::<f32>() / n;
        let placed: f32 = lefts
            .iter()
            .zip(&row)
            .map(|(l, (_, w, _))| l + w / 2.0)
            .sum::<f32>()
            / n;
        let shift = wanted - placed;
        let y = top - f32::from(rows_up) * row_h - 2.0 * k;
        for (left, (_, w, name)) in lefts.into_iter().zip(row) {
            out.push((Rect::new(left + shift, y, w, h), name));
        }
    }
    out
}

/// A slot's name on its label at `label`.
pub fn draw_name_label(p: &mut Painter<'_>, label: Rect, name: &str) {
    let k = p.scale;
    let style = name_style();
    if !p.halves("cross.label", label, WHITE) {
        p.fill(label, 0xE020_3C78);
        p.fill(Rect::new(label.x, label.y, label.w, 1.0 * k), 0xFF6A_9AE0);
    }
    p.text_in(&style, label, Align::Centre, name);
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (this client's own pad support)
    use super::*;
    use dereth_client_contract::combat_mode as c;

    #[test]
    fn the_bars_read_back_what_was_written_and_a_line_it_cannot_read_is_left_out() {
        let (mut sets, _) = parse("");
        sets[PEACE][0] = Some(CrossBind::Item(ObjectId(0x8000_0042)));
        sets[MAGIC + 2][15] = Some(CrossBind::Spell(62));
        sets[MAGIC + 7][6] = Some(CrossBind::Power(PowerAct::Medium));
        let text = to_text(&sets, 2);
        assert_eq!(parse(&text), (sets.clone(), 2));
        let (odd, shown) = parse(
            "bars=2\nset=12\npeace 17 item 5\nmagic9 1 item 5\npeace 1 power fierce\nnoise\nmagic2 2 spell 7\n",
        );
        assert_eq!(shown, 0, "a bar past the eighth is the first");
        assert_eq!(odd[MAGIC + 1][1], Some(CrossBind::Spell(7)));
        assert_eq!(
            odd[MAGIC..].iter().flatten().flatten().count(),
            1,
            "there is no ninth magic bar"
        );
    }

    #[test]
    fn a_file_of_the_first_kind_gives_its_eight_sets_to_magic_and_its_stance_set_to_melee_and_missile(
    ) {
        let (sets, shown) =
            parse("set=3\n1 1 spell 7\n8 2 item 9\n9 10 power raise\n9 12 spell 5\n");
        assert_eq!(shown, 2);
        assert_eq!(sets[MAGIC][0], Some(CrossBind::Spell(7)));
        assert_eq!(sets[MAGIC + 7][1], Some(CrossBind::Item(ObjectId(9))));
        for bar in [MELEE, MISSILE] {
            assert_eq!(sets[bar][9], Some(CrossBind::Power(PowerAct::Raise)));
            assert_eq!(sets[bar][11], Some(CrossBind::Spell(5)));
            assert_eq!(
                sets[bar][8 + 5],
                None,
                "the stance set as the player left it"
            );
        }
        assert_eq!(sets[PEACE], [None; SLOTS]);
    }

    #[test]
    fn the_sets_are_kept_per_server_and_character() {
        let dir = std::env::temp_dir().join(format!("horizon-cross-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut a = CrossBars::default();
        a.load_for(Some(&dir), "one.example:9000", "Cora");
        a.assign(0, Some(CrossBind::Spell(62)));
        let mut b = CrossBars::default();
        b.load_for(Some(&dir), "two.example:9000", "Cora");
        assert_eq!(b.slot(0), None, "a character of the same name elsewhere");
        let mut again = CrossBars::default();
        again.load_for(Some(&dir), "one.example:9000", "Cora");
        assert_eq!(again.slot(0), Some(CrossBind::Spell(62)));
        // A file kept by the name alone is read where there is none for the server yet.
        std::fs::write(dir.join("horizon-cross-Old.txt"), "1 1 spell 7\n").unwrap();
        let mut old = CrossBars::default();
        old.load_for(Some(&dir), "one.example:9000", "Old");
        assert_eq!(old.sets[MAGIC][0], Some(CrossBind::Spell(7)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn full_sets() -> Vec<[Option<CrossBind>; SLOTS]> {
        let (mut sets, _) = parse("");
        sets[PEACE][0] = Some(CrossBind::Item(ObjectId(0x8000_0042)));
        sets[PEACE][15] = Some(CrossBind::Spell(0x3E));
        sets[MAGIC + 4][8] = Some(CrossBind::Spell(0x10));
        sets
    }

    #[test]
    fn the_servers_copy_reads_back_what_was_written_and_reads_the_first_version_once() {
        let mut sets = full_sets();
        sets[MELEE][0] = Some(CrossBind::Spell(7));
        sets[MISSILE][9] = None;
        let text = encode(&sets, 3);
        assert!(text.starts_with("dxhb2;4;i80000042,"), "{text}");
        assert!(text.is_ascii());
        assert_eq!(decode(&text), Some((sets, 3)));
        // Every slot of every bar bound is still short.
        let (mut full, _) = parse("");
        for set in &mut full {
            for slot in set.iter_mut() {
                *slot = Some(CrossBind::Item(ObjectId(0xFFFF_FFFF)));
            }
        }
        assert!(encode(&full, 0).len() < 2000);
        assert_eq!(decode("something else"), None, "not the sets");
        assert_eq!(decode("dxhb3;1;"), None, "a version this does not read");
        // The first version: eight sets and the stance set.
        let first = "dxhb1;2;zz,i10,c9,c1|s3e|||||||,,,,,,,,,c1";
        let (old, shown) = decode(first).unwrap();
        assert_eq!(shown, 1);
        assert_eq!(
            old[MAGIC][..4],
            [
                None,
                Some(CrossBind::Item(ObjectId(0x10))),
                None,
                Some(CrossBind::Power(PowerAct::Raise))
            ]
        );
        assert_eq!(old[MAGIC + 1][0], Some(CrossBind::Spell(0x3E)));
        for bar in [MELEE, MISSILE] {
            assert_eq!(old[bar][9], Some(CrossBind::Power(PowerAct::Raise)));
        }
        assert_eq!(old[PEACE], [None; SLOTS]);
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("horizon-cross-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn with_none_on_the_server_the_sets_from_the_file_go_up_once_and_then_each_change() {
        let dir = scratch("up");
        std::fs::write(
            dir.join("horizon-cross-one_example_Cora.txt"),
            to_text(&full_sets(), 0),
        )
        .unwrap();
        let mut bars = CrossBars::default();
        bars.load_for(Some(&dir), "one.example", "Cora");
        assert_eq!(bars.sync_server(None), None, "the module has not arrived");
        let up = bars.sync_server(Some(None)).expect("the file's sets go up");
        assert_eq!(decode(&up), Some((full_sets(), 0)));
        assert_eq!(bars.sync_server(Some(None)), None, "once");
        bars.assign(1, Some(CrossBind::Spell(9)));
        let changed = bars.sync_server(Some(Some(&up))).expect("a change goes up");
        assert_eq!(
            decode(&changed).unwrap().0[PEACE][1],
            Some(CrossBind::Spell(9))
        );
        assert_eq!(bars.sync_server(Some(Some(&changed))), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_servers_sets_win_over_the_files_and_are_copied_to_it() {
        let dir = scratch("down");
        std::fs::write(
            dir.join("horizon-cross-one_example_Cora.txt"),
            "bars=2\nset=1\npeace 1 spell 5\n",
        )
        .unwrap();
        let mut bars = CrossBars::default();
        bars.load_for(Some(&dir), "one.example", "Cora");
        let server = encode(&full_sets(), 2);
        assert_eq!(
            bars.sync_server(Some(Some(&server))),
            None,
            "nothing to send back"
        );
        assert_eq!(bars.slot(15), Some(CrossBind::Spell(0x3E)));
        assert_eq!(bars.set, 2);
        let file = std::fs::read_to_string(dir.join("horizon-cross-one_example_Cora.txt")).unwrap();
        assert_eq!(
            parse(&file),
            (full_sets(), 2),
            "the file now holds the server's"
        );
        // An item no longer carried keeps its slot until the player changes it.
        assert_eq!(bars.slot(0), Some(CrossBind::Item(ObjectId(0x8000_0042))));
        // Clearing writes empty slots rather than nothing.
        bars.assign(0, None);
        let cleared = bars.sync_server(Some(Some(&server))).unwrap();
        assert!(cleared.starts_with("dxhb2;3;,"), "{cleared}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_melee_and_missile_bars_start_with_the_stance_controls_on_the_right_half_and_keep_changes(
    ) {
        let (sets, _) = parse("");
        for bar in [MELEE, MISSILE] {
            let stance = sets[bar];
            assert!(
                stance[..8].iter().all(Option::is_none),
                "the left half empty"
            );
            assert_eq!(
                stance[8 + 3],
                Some(CrossBind::Power(PowerAct::Lower)),
                "d-pad left"
            );
            assert_eq!(
                stance[8 + 1],
                Some(CrossBind::Power(PowerAct::Raise)),
                "d-pad right"
            );
            assert_eq!(stance[8 + 7], Some(CrossBind::Power(PowerAct::Low)), "X");
            assert_eq!(stance[8 + 6], Some(CrossBind::Power(PowerAct::Medium)), "A");
            assert_eq!(stance[8 + 5], Some(CrossBind::Power(PowerAct::High)), "B");
        }
        assert_eq!(sets[PEACE], [None; SLOTS], "peace starts empty");
        let mut changed = sets.clone();
        changed[MELEE][8 + 6] = Some(CrossBind::Spell(9));
        changed[MISSILE] = [None; SLOTS];
        assert_eq!(
            parse(&to_text(&changed, 0)).0,
            changed,
            "an emptied bar stays empty"
        );
    }

    #[test]
    fn each_stance_shows_its_own_bar_and_only_magic_picks_among_its_eight() {
        let mut bars = CrossBars {
            sets: parse("").0,
            ..CrossBars::default()
        };
        bars.follow_mode(c::NONCOMBAT);
        assert_eq!(bars.shown, PEACE);
        bars.pick(3);
        assert_eq!((bars.shown, bars.set), (PEACE, 0), "no picking in peace");
        bars.follow_mode(c::MELEE);
        assert_eq!(bars.shown, MELEE);
        bars.follow_mode(c::MISSILE);
        assert_eq!(bars.shown, MISSILE);
        bars.follow_mode(c::MAGIC);
        assert_eq!(bars.shown, MAGIC);
        bars.pick(3);
        assert_eq!((bars.shown, bars.set), (MAGIC + 3, 3));
        bars.follow_mode(c::NONCOMBAT);
        bars.follow_mode(c::MAGIC);
        assert_eq!(bars.shown, MAGIC + 3, "the magic bar picked is kept");
    }

    #[test]
    fn a_stance_control_names_and_acts_for_the_weapon_in_hand() {
        use dereth_client_contract::actions::mapped as m;
        assert_eq!(PowerAct::Lower.name(true), "Lower Accuracy");
        assert_eq!(PowerAct::Raise.name(false), "Raise Power");
        assert_eq!(
            PowerAct::Lower.action(true),
            m::COMBAT_DECREASE_MISSILE_ACCURACY.0
        );
        assert_eq!(PowerAct::High.action(false), m::COMBAT_HIGH_ATTACK.0);
    }
}
