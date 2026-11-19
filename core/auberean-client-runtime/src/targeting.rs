//! Selecting as the selection keys do ([`Action::SelectCycle`], [`Action::SelectNearest`],
//! [`Action::SelectSelf`], [`Action::SelectLast`], [`Action::SelectLastAttacker`]), and the
//! selection followed out of the world.
//!
//! The keys select by kind among the objects around the player within the radar's range (indoors
//! the shorter): the next, the previous and the nearest creature, item, player, foe, fellow or
//! pet, ordered by how far each stands (along the ground, plus a fifth more for each meter up or
//! down) and then by id; after the last comes the first again. An object that may not be selected
//! is passed over; what a body holds stands where the body does. A foe is a player or creature the
//! player may attack that is no fellow, nor the pet of the player or of a fellow; a pet is the
//! player's own. The player itself, the last selection and the last attacker have keys of their
//! own.
//!
//! An object taken out of the world while selected is no longer selected: when the player targets
//! what attacks it, the attack on it stops and the nearest foe is selected in its place, if there
//! is one. An object the scripts say may no longer be selected while it is the selection (and not
//! the player) is no longer selected unless it is dead, the attack on it stops, and when the player
//! targets what attacks it the nearest foe is selected.
//!
//! Without the player's scripts the client keeps the selection itself and makes the server's calls
//! for selecting and using as the scripts would ([`crate::calls`]).

use std::collections::BTreeSet;

use auberean_client_contract::notice::{Said, SelectionNotice};
use auberean_client_contract::{Action, SelectKind};

use crate::host::PanelNotice;
use crate::interface::function;
use crate::view::{quality, seen_by_panels};
use crate::{Client, InterfaceArg};

/// The kind bits of an object (its number quality 20).
pub use auberean_client_contract::view::kind;

/// The radar's ranges, indoors and outdoors, when the player's scripts give none.
pub const RANGES: (f32, f32) = auberean_client_contract::view::RADAR_RANGES;

/// An object the selection keys may select, as they read it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Candidate {
    pub id: u64,
    /// Where it stands from the player, in meters (x and y along the ground, z up).
    pub from_player: [f32; 3],
    /// It may be selected.
    pub selectable: bool,
    /// Its kind bits.
    pub kind: u32,
    /// A pet's owner.
    pub owner: Option<u64>,
    /// It is in the player's fellowship; the player may attack it on its own; its owner is in the
    /// fellowship.
    pub fellow: bool,
    pub attackable: bool,
    pub owner_fellow: bool,
}

/// How far an object stands for the selection keys: along the ground, and a fifth more for each
/// meter above or below.
#[must_use]
pub fn distance(from_player: [f32; 3]) -> f32 {
    let (x, y) = (from_player[0], from_player[1]);
    (x * x + y * y).sqrt() + from_player[2].abs() * 1.2
}

/// Whether `(d1, id1)` lies beyond `(d2, id2)`: farther, or as far with a greater id.
// D2-01979
#[must_use]
pub fn farther(d1: f32, id1: u64, d2: f32, id2: u64) -> bool {
    if d2 < d1 {
        return true;
    }
    d1 == d2 && id1 > id2
}

/// Whether an object of these kind bits answers a selection key of `kind`: items the item keys;
/// players the player, foe and fellow keys; other creatures the creature, foe and pet keys.
// D2-01980
#[must_use]
pub fn answers(kind_bits: u32, kind: SelectKind) -> bool {
    use SelectKind as K;
    let item = self::kind::ITEM.iter().any(|k| kind_bits & k == *k);
    if item {
        return kind == K::Item;
    }
    if kind_bits & self::kind::PLAYER == self::kind::PLAYER {
        return matches!(kind, K::Player | K::Foe | K::Fellow);
    }
    if kind_bits & self::kind::CREATURE == self::kind::CREATURE {
        return matches!(kind, K::Creature | K::Foe | K::Pet);
    }
    false
}

/// What a selection key of `kind` selects among `things` (`step` -1 the previous, 1 the next, 0
/// the nearest), from the selection `selection` (0 for none), for the player `me` with the radar
/// at `range` meters; `None` when nothing answers.
// D2-01986, D2-01987
#[must_use]
pub fn pick(
    things: &[Candidate],
    me: u64,
    selection: u64,
    range: f32,
    kind: SelectKind,
    step: i32,
) -> Option<u64> {
    let fellow = kind == SelectKind::Fellow;
    let foe = kind == SelectKind::Foe;
    let pet = kind == SelectKind::Pet;
    // The square of the range bounds the search on the ground, and stands for the farthest
    // distance a search starts from (the two are compared as they are).
    let range_sq = range * range;
    // Where the search starts: from the selection (when it is in range), or from the nearest (or
    // the farthest) for a fresh search.
    let mut current = 0u64;
    let mut current_d = 0.0f32;
    if step != 0 && selection != 0 {
        if let Some(t) = things.iter().find(|t| t.id == selection) {
            let [x, y, _] = t.from_player;
            if x * x + y * y <= range_sq {
                current = t.id;
                current_d = distance(t.from_player);
            }
        }
    }
    let backwards = step == -1;
    let mut best_d = range_sq;
    if current != 0 || backwards {
        if current == 0 {
            current_d = range_sq;
        }
        if backwards {
            best_d = 0.0;
        }
    }
    let mut best = 0u64;
    for pass in 0..2 {
        for t in things {
            if t.id == current || t.id == me || !t.selectable {
                continue;
            }
            if !answers(t.kind, kind) {
                continue;
            }
            if fellow && !t.fellow {
                continue;
            }
            if foe && !(t.attackable && !t.fellow) {
                continue;
            }
            if pet && t.owner != Some(me) {
                continue;
            }
            if foe {
                if let Some(o) = t.owner {
                    if o == me || t.owner_fellow {
                        continue;
                    }
                }
            }
            let [x, y, _] = t.from_player;
            if x * x + y * y > range_sq {
                continue;
            }
            let d = distance(t.from_player);
            if farther(d, t.id, best_d, best) == backwards
                && farther(d, t.id, current_d, current) != backwards
            {
                best = t.id;
                best_d = d;
            }
        }
        if best != 0 {
            return Some(best);
        }
        if current == 0 || pass == 1 {
            return None;
        }
        // Round again from the other end.
        current_d = if backwards { range_sq } else { 0.0 };
    }
    None
}

/// A cell's block and a place in it on the one grid across every block.
#[allow(clippy::cast_precision_loss)]
fn global(cell: u32, origin: [f32; 3]) -> [f32; 3] {
    [
        f32::from(u8::try_from(cell >> 24).unwrap_or(0)) * 160.0 + origin[0],
        f32::from(u8::try_from((cell >> 16) & 0xFF).unwrap_or(0)) * 160.0 + origin[1],
        origin[2],
    ]
}

/// The selection as the client keeps it without the player's scripts, and the objects the last
/// frame saw around the player.
#[derive(Clone, Debug, Default)]
pub struct Targeting {
    /// What is selected and what was before it, without the scripts.
    selected: u64,
    last: u64,
    /// The objects around the player at the last frame, to tell which went.
    seen: BTreeSet<u64>,
    /// How many of the notices waiting to be taken were heard, and the objects the notices heard
    /// said may no longer be selected, in order, not yet followed.
    heard: usize,
    unselectable: Vec<u64>,
}

impl Targeting {
    /// The notices not heard yet of those waiting to be taken (`waiting`): each object they say may
    /// no longer be selected, kept to follow.
    pub(crate) fn hear(&mut self, waiting: &[PanelNotice]) {
        let from = self.heard.min(waiting.len());
        for n in &waiting[from..] {
            if n.native != crate::said::native::SELECTABLE {
                continue;
            }
            let said = crate::said::read(n);
            if let (
                Some(about),
                Said::Selection(SelectionNotice::Selectable { selectable: false }),
            ) = (said.about, said.said)
            {
                self.unselectable.push(about);
            }
        }
        self.heard = waiting.len();
    }

    /// The notices waiting were taken: those not heard first, then none heard of what comes next.
    pub(crate) fn taken(&mut self, waiting: &[PanelNotice]) {
        self.hear(waiting);
        self.heard = 0;
    }
}

impl Client {
    /// What is selected (0 for nothing): as the player's scripts hold it; without them, as the
    /// client keeps it.
    pub fn selection(&mut self) -> u64 {
        if self.scripts.is_none() {
            return self.targeting.selected;
        }
        self.interface_long(function::GET_SELECTION)
            .map_or(0, |(_, v)| v)
    }

    /// What was selected before (0 for nothing), as [`Client::selection`] reads it.
    pub fn last_selection(&mut self) -> u64 {
        if self.scripts.is_none() {
            return self.targeting.last;
        }
        self.interface_long(function::GET_LAST_SELECTION)
            .map_or(0, |(_, v)| v)
    }

    /// Whether the player targets what attacks it (its option; without the scripts, never).
    fn auto_targets(&mut self) -> bool {
        self.interface(function::GET_AUTO_TARGET, &[])
            .is_some_and(|(_, v)| v != 0)
    }

    /// Without the player's scripts, a selection made: the client keeps it and makes the server's
    /// call; with them, nothing (the scripts make it).
    pub(crate) fn select_unscripted(&mut self, target: u64) {
        if self.scripts.is_some() {
            return;
        }
        if target != self.targeting.selected {
            self.targeting.last = self.targeting.selected;
        }
        self.targeting.selected = target;
        self.act(crate::calls::select(target));
    }

    /// Without the player's scripts, the selection used: the server's call for it, when something
    /// is selected.
    pub(crate) fn use_unscripted(&mut self) {
        if self.scripts.is_some() || self.targeting.selected == 0 {
            return;
        }
        let id = self.targeting.selected;
        self.act(crate::calls::use_object(id));
    }

    /// A selection key's selection: see the module's description. With no player here, nothing.
    pub(crate) fn select_by(&mut self, action: &Action) {
        let Some(me) = self.model.player_object().map(|o| o.id) else {
            return;
        };
        let target = match *action {
            Action::SelectSelf => {
                if self.selection() == me {
                    self.last_selection()
                } else {
                    me
                }
            }
            Action::SelectLast => self.last_selection(),
            Action::SelectLastAttacker => self.last_attacker,
            Action::SelectCycle { kind, previous } => {
                let step = if previous { -1 } else { 1 };
                let Some(t) = self.select_kind(me, kind, step) else {
                    return;
                };
                t
            }
            Action::SelectNearest { kind } => {
                let Some(t) = self.select_kind(me, kind, 0) else {
                    return;
                };
                t
            }
            _ => return,
        };
        self.act(Action::Select { target });
    }

    /// What a selection key of `kind` selects now; `None` when nothing answers.
    fn select_kind(&mut self, me: u64, kind: SelectKind, step: i32) -> Option<u64> {
        let things = self.candidates(me, matches!(kind, SelectKind::Foe | SelectKind::Fellow));
        let (inside, outside) = self.radar_ranges().unwrap_or(RANGES);
        let cell = if self.body.placed() {
            self.body.cell
        } else {
            self.model.player_object()?.place?.cell
        };
        let range = if cell & 0xFFFF >= 0x100 {
            inside
        } else {
            outside
        };
        let selection = if step == 0 { 0 } else { self.selection() };
        pick(&things, me, selection, range, kind, step)
    }

    /// The objects around the player as the selection keys read them, each where it stands from
    /// the player (what a body holds, where the body stands); with `targets`, which are fellows and
    /// which the player may attack, as its scripts answer.
    fn candidates(&mut self, me: u64, targets: bool) -> Vec<Candidate> {
        let place_of = |c: &Self, id: u64| {
            let o = c.model.object(id)?;
            let o = match o.parent {
                Some((p, _)) => c.model.object(p)?,
                None => o,
            };
            o.place
                .map(|p| global(p.cell, [p.origin.x, p.origin.y, p.origin.z]))
        };
        let at = if self.body.placed() {
            let o = self.body.origin;
            Some(global(self.body.cell, [o.x, o.y, o.z]))
        } else {
            place_of(self, me)
        };
        let Some(at) = at else {
            return Vec::new();
        };
        let ids: Vec<u64> = self
            .model
            .objects()
            .filter(|o| seen_by_panels(o))
            .map(|o| o.id)
            .collect();
        let mut things = Vec::with_capacity(ids.len());
        for id in ids {
            let Some(p) = place_of(self, id) else {
                continue;
            };
            #[allow(clippy::cast_sign_loss)]
            let kind = self.int_quality(id, quality::KIND).unwrap_or(0) as u32;
            let owner = self
                .instance_quality(id, quality::OWNER)
                .filter(|o| *o != 0);
            let mut t = Candidate {
                id,
                from_player: [p[0] - at[0], p[1] - at[1], p[2] - at[2]],
                selectable: self.bool_quality(id, quality::SELECTABLE).unwrap_or(false),
                kind,
                owner,
                ..Candidate::default()
            };
            if targets && t.selectable && id != me {
                // D2-00604, D2-01184, D2-00427, D2-01073
                let ask = |c: &mut Self, function: u32, id: u64| {
                    c.interface(function, &[InterfaceArg::Long(id)])
                        .is_some_and(|(_, v)| v != 0)
                };
                t.fellow = ask(self, function::IS_FELLOWSHIP_MEMBER, id);
                t.attackable = ask(self, function::CAN_AUTO_ATTACK_TARGET, id);
                t.owner_fellow =
                    owner.is_some_and(|o| ask(self, function::IS_FELLOWSHIP_MEMBER, o));
            }
            things.push(t);
        }
        things
    }

    /// Each frame, the selection followed out of the world: when the object selected is no
    /// longer around the player, the player stops attacking it and selects the nearest foe when
    /// it targets what attacks it, and it is no longer selected if nothing else was.
    // D2-01991
    pub(crate) fn follow_selection(&mut self) {
        let Some(me) = self.model.player_object().map(|o| o.id) else {
            self.targeting.seen.clear();
            return;
        };
        let now: BTreeSet<u64> = self
            .model
            .objects()
            .filter(|o| seen_by_panels(o))
            .map(|o| o.id)
            .collect();
        let went = self.targeting.seen.difference(&now).next().is_some();
        let seen = std::mem::replace(&mut self.targeting.seen, now);
        if !went {
            return;
        }
        let selected = self.selection();
        if selected == 0 || !seen.contains(&selected) || self.targeting.seen.contains(&selected) {
            return;
        }
        let mut other = None;
        if self.auto_targets() {
            self.act(Action::CeaseAttack { target: selected });
            other = self
                .select_kind(me, SelectKind::Foe, 0)
                .filter(|t| *t != selected);
        }
        self.act(Action::Select {
            target: other.unwrap_or(0),
        });
    }

    /// Each frame, the objects the scripts said may no longer be selected: one that is the
    /// selection, and not the player, is no longer selected unless it is dead; the player stops
    /// attacking it; and the nearest foe is selected when the player targets what attacks it.
    // D2-01990
    pub(crate) fn follow_selectability(&mut self) {
        self.targeting.hear(&self.notices);
        let told = std::mem::take(&mut self.targeting.unselectable);
        let Some(me) = self.model.player_object().map(|o| o.id) else {
            return;
        };
        for object in told {
            if object == 0 || object == me || self.selection() != object {
                continue;
            }
            let dead = self.model.object(object).is_some_and(seen_by_panels)
                && self.bool_quality(object, quality::DEAD).unwrap_or(false);
            if !dead {
                self.act(Action::Select { target: 0 });
            }
            self.act(Action::CeaseAttack { target: object });
            if self.auto_targets() {
                self.act(Action::SelectNearest {
                    kind: SelectKind::Foe,
                });
            }
        }
    }
}
