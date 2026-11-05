//! Wielding: a player uses a weapon in its pack to wield it (and uses it again to put it away).
//!
//! **Wielding** sends, to every client that sees the player: its weapon drawn (a slider), the item
//! leaving the pack, the player as its wielder and container, the item attached to the player's
//! hand; then the player's answer: where the item went, the hand slots it takes and those it rules
//! out. **Putting it away**: the item is contained in the pack again, the answer, the item
//! detached, the weapon sheathed, the wielder cleared.
//!
//! A thing's hands and slots are its template's: the places it may go and prefers, those it rules
//! out, where it hangs (2017: 808 of 808 answers for a thing in a hand said what its template
//! does); a template that does not say is wielded as a sword is.
//!
//! **Wearing** armor, clothing or jewelry (an item whose template names places on the body):
//! the item goes to the place it prefers, or another of its places that is free, taking off
//! whatever is there first (that item goes where the worn one was in the pack). The player is
//! answered (the places, those ruled out, the item it replaced), its look gains the item's
//! appearance for its race and sex (seen by all who see it; jewelry gives none), and it is told the
//! item's container and wearer. **Taking it off**: the answer, the look without it, the wearer
//! cleared; the item goes to the pack's front.
//!
//! **What an item does to its wearer.** Worn or wielded, an item puts its effects on the player
//! just before the answer: its template's (`Item_EquipperEffectN`, at `Item_EquipperEffectNStat`)
//! and its own (a crafted bonus), each a record held until the item comes off
//! ([`crate::effects::put_on_worn`]). As it comes off, by its use, a directive, or to make room,
//! its records are taken away just before that answer. They stay across logouts and deaths.
//!
//! **Who may put it on.** An item is refused, nothing done, when its template names a race the
//! player is not of (10015), a skill the player does not hold (10013), a least level above the
//! player's (10009), or a quest not where the item needs it (a hidden one's refusal is the
//! mysterious one), checked in that order. A use is answered with the refusal alone; a directive
//! with its equipping answer, the refusal for its status.

use auberean_protocol::physics::{Attach, ObjectEvent, Slide};
use auberean_protocol::quality::VisualQuality;
use auberean_protocol::table::{Table, SMALL};
use auberean_protocol::types::ObjectStamp;
use auberean_protocol::types::{PartGroupDesc, VisualDesc};
use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_net::{BlobKind, ConnId, ServerNet};
use yalain_store::Store;

use crate::combat::to_all_seeing;
use crate::graph::{tag, Builder, Slot};
use crate::inventory::visual;
use crate::{effects, progress, session, Clock, Server};

/// The equipping functions.
pub mod function {
    pub const EQUIP_DONE: u32 = 0x820E_0051;
    pub const UNEQUIP_DONE: u32 = 0x820E_0056;
    /// The client's directives: equip an item at a place it names (dragged to the paper doll),
    /// and take one off to a slot of the pack it names.
    pub const DIRECTIVE_EQUIP: u32 = 0x820E_004F;
    pub const DIRECTIVE_UNEQUIP: u32 = 0x820E_0015;
}

/// The equipping description and the item's equipping profile (packages and slot counts).
pub const EQUIP_DESC: (u16, usize) = (0x1F6, 19);
pub const EQUIP_PROFILE: (u16, usize) = (0x1F5, 7);

/// The warning an item that binds when used in combat is equipped with: "This item will become
/// bound to you when used in combat. ...", shown by the client as a message of its own.
pub const DISPLAY_MESSAGE: u32 = 0x8212_0024;
pub const BINDS_WARNING: (u32, u32) = (0xD71B_6077, 0x2500_04A9);

/// An item bound, to its holder: "By using it, you have bound #1 to you. It can no longer be
/// equipped by anyone else, and cannot be sold in shops.", and the variable that names it.
pub const BOUND_TEXT: (u32, u32) = (0xF696_A984, 0x2500_0014);
const BOUND_TEXT_ITEM: u32 = 0x9352_2CD0;

/// The status of an item bound to another player: the client writes why.
pub const BOUND_TO_ANOTHER: u32 = 10074;

/// The chance that a blow struck with what a player holds binds it, marked to (one in about ten:
/// 2, 5, 11 and 12 attacks before each of the 4 weapons bound in view).
pub const SWING_BINDS: f32 = 0.1;

/// The slider of a player's weapon being drawn.
pub const WEAPON_DRAWN: u32 = 0x4000_001F;

/// Qualities of a wielded item.
pub mod q {
    pub const CONTAINER: u32 = 258;
    pub const EQUIPPER: u32 = 261;
}

/// The primary hand.
pub const PRIMARY_HAND: u32 = 64;

/// The places on the body armor, clothing and jewelry are worn on (the hands hold weapons).
pub const BODY_PLACES: u32 = 0x1 | 0x2 | 0x8 | 0x10 | 0x20 | 0x100 | 0x200 | 0x400;

/// The answers' control flags: an item worn, taken off by its use, taken off to make room for
/// another, a weapon wielded.
pub mod flags {
    pub const WORN: u32 = 142;
    pub const TAKEN_OFF: u32 = 158;
    pub const MADE_ROOM: u32 = 184;
    pub const WIELDED: u32 = 206;
    /// Equipped by a directive, at the place the client named.
    pub const DIRECTED: u32 = 222;
}

/// The off hand.
pub const OFF_HAND: u32 = 128;

/// Why an item may not be put on, as its answer says (with the item uses' least level and quest
/// refusals).
pub mod refusal {
    /// The player's race is not among those the item names.
    pub const WRONG_RACE: u32 = 10_015;
    /// The player does not hold a skill the item needs.
    pub const MISSING_SKILL: u32 = 10_013;
    /// The player's Arcane Lore is below the item's.
    pub const ARCANE_LORE_TOO_LOW: u32 = 10_050;
}

/// The control flags of a use refused for the item's requirements.
pub const REFUSED_FLAGS: u32 = 316;

/// The status of a take-off refused because the pack has no room.
pub const PACK_FULL: u32 = 1085;

/// The appearance modifier a worn garment sets to 1: shown.
const SHOWN: u32 = 0x10;

/// Where an item is worn, from its template: the places it may go, the one it goes to first, the
/// places it rules out, and the places it hangs from (none for what is worn on the body).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Fit {
    pub valid: u32,
    pub preferred: u32,
    pub precluded: u32,
    pub primary: u32,
    pub secondary: u32,
}

/// How an item of these properties is worn on the body; `None` for anything else (weapons and
/// what the hands hold included).
#[must_use]
pub fn fit_of(props: &yalain_content::Props) -> Option<Fit> {
    let valid = props.word("Item_ValidInventoryLocations").unwrap_or(0);
    if valid & BODY_PLACES == 0 || valid & !BODY_PLACES != 0 {
        return None;
    }
    let preferred = props
        .word("Item_PreferredInventoryLocation")
        .filter(|p| p & valid != 0)
        .unwrap_or(valid.isolate_lowest_one());
    Some(Fit {
        valid,
        preferred,
        precluded: props.word("Item_PrecludedInventoryLocations").unwrap_or(0),
        primary: props.word("Item_PrimaryParentingLocation").unwrap_or(0),
        secondary: props.word("Item_SecondaryParentingLocation").unwrap_or(0),
    })
}

/// The appearance modifiers a dye sets to its value: an item's color (2017: 32 of 32 armor pieces
/// dyed held the three at the plant's value when next made).
pub const DYE_KEYS: [u32; 3] = [2, 14, 0x4100_0004];

/// The appearance table a dye writes to: an item's table for lying in a pack, else its first for
/// being worn (2017: 32 of 32 armor pieces dyed).
#[must_use]
pub fn dye_table(props: &yalain_content::Props) -> Option<u32> {
    props
        .word("Item_PileAppearance")
        .or_else(|| props.word("Item_WornAppearanceFile1"))
}

/// What a kept item carries of its own.
#[must_use]
pub fn own_kept(kept: &[crate::state::ItemState], item: u64) -> crate::treasure::Own {
    kept.iter()
        .find(|k| k.id == item)
        .map(|k| k.own)
        .unwrap_or_default()
}

/// The appearance an item of these properties gives a wearer of this race and sex (its template
/// names one per race and sex), with the garment shown, the item's own appearance value (one its
/// template sets), what it drew of its look, and the color a dye gave it.
#[must_use]
pub fn worn_look(
    props: &yalain_content::Props,
    race: u32,
    sex: u32,
    own: &crate::treasure::Own,
) -> Option<(u32, Table<f32>)> {
    let file = (1..=16).find_map(|n| {
        let file = props.word(&format!("Item_WornAppearanceFile{n}"))?;
        let r = props.word(&format!("Item_WornAppearanceRace{n}"))?;
        let x = props.word(&format!("Item_WornAppearanceSex{n}"))?;
        (r & race != 0 && x & sex != 0).then_some(file)
    })?;
    let mut values = vec![(SHOWN, 1.0)];
    if let (Some(key), Some(v)) = (
        props.word("AppearanceMutationKey"),
        props.float("AppearanceMutationKeyValue"),
    ) {
        if key != SHOWN {
            values.push((key, v));
        }
    }
    if let Some(look) = own.look {
        for (key, value) in look.keys() {
            values.retain(|(k, _)| *k != key);
            values.push((key, value));
        }
    }
    if let Some((_, d)) = own.dye {
        values.retain(|(k, _)| !DYE_KEYS.contains(k));
        values.extend(DYE_KEYS.map(|k| (k, d)));
    }
    values.sort_by_key(|(k, _)| *k);
    Some((file, Table::from_inserted(SMALL, values)))
}

/// A look with a worn item's appearance added to its whole-object values: first in its bucket, as
/// the client's tables add an entry (one already there of the same appearance is replaced).
pub fn put_on(look: &mut VisualDesc, table: u32, values: Table<f32>) {
    look.bits |= VisualDesc::GLOBAL;
    let g = &mut look.global;
    if g.bits & PartGroupDesc::KEY == 0 {
        g.bits |= PartGroupDesc::KEY;
        g.key = crate::chargen::AVATAR_KEY;
    }
    g.bits |= PartGroupDesc::APPEARANCE;
    let entries = &mut g.appearance.entries;
    entries.retain(|(k, _)| *k != table);
    let buckets = auberean_protocol::table::BUCKET_COUNTS
        .get(usize::from(g.appearance.size_class))
        .copied()
        .unwrap_or(47);
    let at = entries
        .iter()
        .position(|(k, _)| k % buckets >= table % buckets)
        .unwrap_or(entries.len());
    entries.insert(at, (table, values));
}

/// A look without a worn item's appearance; the rest keep their order.
pub fn take_from(look: &mut VisualDesc, table: u32) {
    look.global.appearance.entries.retain(|(k, _)| *k != table);
}

/// A body of a race and sex wearing these items: the look it is drawn with.
#[must_use]
pub fn dressed(
    base: &VisualDesc,
    catalog: &yalain_content::Catalog,
    items: &[crate::state::Held],
    kept: &[crate::state::ItemState],
    race: u32,
    sex: u32,
) -> VisualDesc {
    let mut look = base.clone();
    let mut worn: Vec<&crate::state::Held> = items
        .iter()
        .filter(|h| h.location & BODY_PLACES != 0)
        .collect();
    worn.sort_by_key(|h| h.location);
    for h in worn {
        if let Some((table, values)) = catalog
            .get(h.template)
            .and_then(|t| worn_look(&t.props, race, sex, &own_kept(kept, h.id)))
        {
            put_on(&mut look, table, values);
        }
    }
    look
}

/// How a weapon is held: its two parenting places, the hands it may go to and prefers, the hand
/// slots it takes in the hand it prefers and those it rules out, the answer's control flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hold {
    pub primary: u32,
    pub secondary: u32,
    pub valid: u32,
    pub preferred: u32,
    pub taken: u32,
    pub precluded: u32,
    pub flags: u32,
}

/// A one-handed sword's hold.
pub const SWORD: Hold = Hold {
    primary: 1,
    secondary: 0x4000_0006,
    valid: 192,
    preferred: 64,
    taken: 64,
    precluded: 0,
    flags: flags::WIELDED,
};

/// How a thing is held, as its template says: where it may go and hangs, what it rules out; a
/// weapon (a template with a stance) answered as wielded, anything else (a shield, a tool) as
/// worn; hung from the place 1 where it names none (2017: 808 of 808 answers). A template that
/// names no hand is held as the starting weapons are: the human's bow and stave in both hands,
/// anything else as a sword.
#[must_use]
pub fn hold_of(catalog: &yalain_content::Catalog, template: u32) -> Hold {
    let Some((props, valid)) = catalog.get(template).and_then(|t| {
        t.props
            .word("Item_ValidInventoryLocations")
            .map(|v| (&t.props, v & (PRIMARY_HAND | OFF_HAND)))
            .filter(|(_, v)| *v != 0)
    }) else {
        return match template {
            0x4700_051B => Hold {
                primary: 2,
                secondary: 1,
                valid: 64,
                taken: 192,
                precluded: 128,
                ..SWORD
            },
            0x4700_051D => Hold {
                primary: 1,
                secondary: 2,
                valid: 64,
                taken: 192,
                precluded: 128,
                ..SWORD
            },
            _ => SWORD,
        };
    };
    let preferred = props
        .word("Item_PreferredInventoryLocation")
        .filter(|p| p & valid != 0)
        .unwrap_or(valid.isolate_lowest_one());
    let precluded = props.word("Item_PrecludedInventoryLocations").unwrap_or(0);
    Hold {
        primary: props.word("Item_PrimaryParentingLocation").unwrap_or(1),
        secondary: props.word("Item_SecondaryParentingLocation").unwrap_or(1),
        valid,
        preferred,
        taken: preferred | precluded,
        precluded,
        flags: if props.word("Weapon_SingleWeaponStance").is_some() {
            flags::WIELDED
        } else {
            flags::WORN
        },
    }
}

fn long(v: u64) -> [Slot; 2] {
    #[allow(clippy::cast_possible_truncation)]
    [
        Slot::Tagged(tag::LONG, v as u32),
        Slot::Tagged(tag::HIGH, (v >> 32) as u32),
    ]
}

/// What an equipping answer says.
#[derive(Debug, Clone, Copy, Default)]
struct Answer {
    item: u64,
    player: u64,
    /// The item's profile: where it hangs from, the places it may go and prefers (all 0 in a
    /// take-off's answer).
    fit: Fit,
    taken: u32,
    precluded: u32,
    location: u32,
    /// The slot in the pack it is sent to, and the slot it is at.
    target_slot: u32,
    slot: u32,
    flags: u32,
    status: u32,
    /// The item it replaced, and where that was.
    blocking: (u64, u32),
    /// The item binds to its holder when used in combat, and has not yet.
    binds_on_use: bool,
}

/// The equipping answer's description.
fn desc(a: &Answer) -> Arg {
    let mut b = Builder::new();
    let root = b.reserve(EQUIP_DESC.0);
    let f = a.fit;
    let profile = b.object(
        EQUIP_PROFILE.0,
        &[
            Slot::Int(0),
            Slot::Int(f.primary),
            Slot::Int(f.secondary),
            Slot::Int(f.valid),
            Slot::Int(u32::from(a.binds_on_use)),
            Slot::Int(f.preferred),
            Slot::Int(0),
        ],
    );
    let [i0, i1] = long(a.item);
    let [e0, e1] = long(a.player);
    let [c0, c1] = long(a.player);
    let [z0, z1] = long(a.blocking.0);
    b.fill(
        root,
        &[
            Slot::Int(a.blocking.1),
            Slot::Ref(Some(profile)),
            Slot::Int(1),
            i0,
            i1,
            Slot::Int(a.precluded),
            Slot::Int(a.target_slot),
            Slot::Int(a.location),
            e0,
            e1,
            c0,
            c1,
            Slot::Int(a.flags),
            Slot::Int(a.status),
            Slot::Int(a.taken),
            Slot::Int(a.location),
            z0,
            z1,
            Slot::Int(a.slot),
        ],
    );
    Arg::Object(b.graph())
}

fn iid_update<S: Store>(s: &mut Server<S>, item: u64, quality: u32, value: u64) -> ServerMessage {
    ServerMessage::UpdateInstanceIdVisual(VisualQuality {
        object: visual(s, item),
        quality,
        value,
    })
}

/// Why a player may not put on an item of these properties, if it may not: its race is not among
/// the item's, it does not hold a skill the item needs, it is below the item's least level, its
/// Arcane Lore is below the item's, or the quest the item needs is not where it needs it. A
/// treasure item's own least level and Arcane Lore stand in for its template's (2017: the item's own
/// least level above the player's, 8 of 9 refusals 10009 whose player's level is known; its own
/// Arcane Lore, 47 to 53, 5 of 5 refusals 10050 of players of level 51 to 54; the level first, 3
/// of 3 items needing both refused 10009 to players below it).
fn refused<S: Store>(
    s: &Server<S>,
    world: ConnId,
    props: &yalain_content::Props,
    own: crate::treasure::Own,
    clock: Clock,
) -> Option<u32> {
    let p = s.players.get(&world)?;
    if crate::item_use::not_monarch(s, world, props) {
        return Some(crate::item_use::refusal::MONARCH_ONLY);
    }
    if props
        .word("Usage_RequiredRace")
        .is_some_and(|races| races != 0 && races & p.race == 0)
    {
        return Some(refusal::WRONG_RACE);
    }
    let missing = ["Usage_RequiredSkill1", "Usage_RequiredSkill2"]
        .iter()
        .filter_map(|n| props.word(n).filter(|k| *k != 0))
        .any(|k| !p.play.skills.iter().any(|h| h.skill == k));
    if missing {
        return Some(refusal::MISSING_SKILL);
    }
    let least = own
        .min_level
        .map(i64::from)
        .or_else(|| props.int("Usage_MinLevel"))
        .unwrap_or(0);
    if i64::from(crate::examine::level_of(&p.qualities)) < least {
        return Some(crate::item_use::refusal::LEVEL_TOO_LOW);
    }
    let lore = own
        .arcane_lore
        .map(i64::from)
        .or_else(|| props.int("Usage_RequiredArcaneLore"))
        .unwrap_or(0);
    if lore > 0 && i64::from(arcane_lore_of(s, p)) < lore {
        return Some(refusal::ARCANE_LORE_TOO_LOW);
    }
    crate::item_use::quest_gate(s, world, props, clock)
}

/// A player's Arcane Lore: the rating of its trained skill of the Arcane Lore attribute, times the
/// skill's measure of it (none without one).
#[must_use]
pub fn arcane_lore_of<S: Store>(s: &Server<S>, p: &crate::world::Player) -> i32 {
    let starting = s.start.skills.get(&p.race).cloned().unwrap_or_default();
    let skills = crate::skills::held(&starting, &p.play.skills);
    skills
        .iter()
        .filter(|h| h.mask & crate::skills::mask::TRAINED != 0)
        .filter_map(|h| {
            let d = s.skill_table.skills.get(&h.skill)?;
            (d.category == crate::skills::ARCANE_LORE_CATEGORY).then(|| {
                #[allow(clippy::cast_precision_loss)]
                let rating = crate::skill_use::effective_rating(s, p, &skills, h.skill) as f32;
                dereth_primitives::num::to_i32(rating * d.attribute_mod)
            })
        })
        .max()
        .unwrap_or(0)
}

/// Why a player may not put on this item of its pack, if it may not.
fn refused_item<S: Store>(
    s: &Server<S>,
    world: ConnId,
    h: &crate::state::Held,
    clock: Clock,
) -> Option<u32> {
    let t = s.catalog.get(h.template)?;
    let own = s.things.get(h.id).map(|i| i.own).unwrap_or_default();
    let me = s.players.get(&world).map_or(0, |p| p.character);
    if own.bound_to != 0 && own.bound_to != me {
        return Some(BOUND_TO_ANOTHER);
    }
    refused(s, world, &t.props, own, clock)
}

/// A player uses an item it holds: a weapon in its pack is wielded, a wielded one put away; one it
/// may not put on is refused, its answer alone.
pub fn use_held<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: &crate::places::Use,
    clock: Clock,
) {
    let item = u.item;
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let Some(h) = p.play.items.iter().find(|h| h.id == item).copied() else {
        return;
    };
    if h.location == 0 {
        if let Some(why) = refused_item(s, world, &h, clock) {
            let no = crate::places::use_refused(u, 0, Some(REFUSED_FLAGS), 0.0, why);
            progress::private(net, world, &no);
            return;
        }
    }
    let fit = s.catalog.get(h.template).and_then(|t| fit_of(&t.props));
    match (h.location, fit) {
        (0, Some(fit)) => wear(s, net, world, item, fit, None, flags::WORN, clock),
        (0, None) if !wields_by_use(s, h.template) => {
            let no = crate::places::use_refused(
                u,
                0,
                Some(crate::places::USE_FLAGS_PACK),
                0.0,
                crate::places::USE_STATUS_IN_USE,
            );
            progress::private(net, world, &no);
        }
        (0, None) => {
            let hand = hand_of_use(s, item, h.template);
            wield(s, net, world, item, hand, None, clock);
        }
        (at, _) if at & BODY_PLACES != 0 => {
            take_off(s, net, world, item, false);
        }
        _ => put_away(s, net, world, item, false, None),
    }
}

/// The player's look sent to everyone who sees it (a rider's, on its mount).
pub(crate) fn show_look<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    if s.players.get(&world).is_some_and(crate::mounts::mounted) {
        crate::mounts::look_changed(s, net, world);
        return;
    }
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let player = p.character;
    let m = ServerMessage::UpdateVisualDesc(crate::combat::player_event(p, p.look.clone()));
    to_all_seeing(s, net, player, &m);
}

/// Wear an item of the pack on the body: at the place named, else the place it prefers or another
/// of its places that is free, what is there taken off first and put where the item was in the
/// pack.
#[allow(clippy::too_many_arguments)]
fn wear<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    item: u64,
    fit: Fit,
    at: Option<u32>,
    answer_flags: u32,
    clock: Clock,
) {
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let (race, sex) = (p.race, p.sex);
    let holds = |p: &crate::world::Player, place: u32| -> Vec<u64> {
        p.play
            .items
            .iter()
            .filter(|h| h.id != item && h.location & BODY_PLACES != 0)
            .filter(|h| (h.location | precluded_of(s, h.template)) & (place | fit.precluded) != 0)
            .map(|h| h.id)
            .collect()
    };
    let free = (0..32)
        .map(|b| 1u32 << b)
        .filter(|b| fit.valid & b != 0)
        .find(|b| holds(p, *b).is_empty());
    let location = if let Some(at) = at.filter(|a| fit.valid & a != 0) {
        at
    } else if holds(p, fit.preferred).is_empty() {
        fit.preferred
    } else {
        free.unwrap_or(fit.preferred)
    };
    let in_the_way = holds(p, location);
    let mut blocking = (0, 0);
    for other in in_the_way {
        let at = s
            .players
            .get(&world)
            .and_then(|p| p.play.items.iter().find(|h| h.id == other))
            .map_or(0, |h| h.location);
        let slot = s
            .players
            .get(&world)
            .and_then(|p| p.play.items.iter().find(|h| h.id == item))
            .map_or(0, |h| h.slot);
        if !take_off_to(s, net, world, other, true, slot) {
            return;
        }
        blocking = (other, at);
    }
    let look = s
        .things
        .get(item)
        .and_then(|t| s.catalog.get(t.template).map(|p| (p, t.own)))
        .and_then(|(t, own)| worn_look(&t.props, race, sex, &own));
    // Its effects come on just before the answer.
    effects_on(s, net, world, item, clock);
    let binds_on_use = binds_on_use(s, item);
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let player = p.character;
    let slot = p
        .play
        .items
        .iter()
        .find(|h| h.id == item)
        .map_or(0, |h| h.slot);
    let done = session::call(
        function::EQUIP_DONE,
        &[desc(&Answer {
            item,
            player,
            fit: Fit {
                primary: 0,
                secondary: 0,
                ..fit
            },
            taken: location | fit.precluded,
            precluded: fit.precluded,
            location,
            target_slot: 0,
            slot,
            flags: answer_flags,
            blocking,
            binds_on_use,
            ..Answer::default()
        })],
    )
    .expect("packs");
    progress::private(net, world, &done);
    if binds_on_use {
        progress::private(net, world, &binds_warning());
    }
    if let Some(h) = p.play.items.iter_mut().find(|h| h.id == item) {
        h.location = location;
    }
    crate::inventory::close_up(&mut p.play.items);
    // Jewelry gives no look: the look is not sent.
    if let Some((table, values)) = look {
        put_on(&mut p.look, table, values);
        show_look(s, net, world);
    }
    // The item's holders are the wearer's to know.
    for (quality, value) in [
        (q::CONTAINER, 0),
        (q::EQUIPPER, player),
        (q::CONTAINER, player),
    ] {
        let m = iid_update(s, item, quality, value);
        crate::inventory::to_holder(s, net, world, item, &m);
    }
    if let Some(t) = s.things.get_mut(item) {
        t.wielded = Some((player, location));
    }
    crate::world::keep_player(s, world);
}

/// The effects an item puts on whoever wears or wields it: its template's, then its own (a crafted
/// bonus), each saved state at its spellcraft.
#[must_use]
pub fn worn_effects<S: Store>(s: &Server<S>, item: u64) -> Vec<(u32, f32)> {
    let Some(t) = s.things.get(item) else {
        return Vec::new();
    };
    s.worn_effects
        .get(&t.template)
        .into_iter()
        .flatten()
        .map(|e| (e.state, e.spellcraft))
        .chain(t.bonuses.iter().copied())
        .collect()
}

/// Put an item's effects on the player wearing or wielding it.
fn effects_on<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    item: u64,
    clock: Clock,
) {
    let list = worn_effects(s, item);
    if !list.is_empty() {
        effects::put_on_worn(s, net, world, item, &list, clock);
    }
}

/// The places a worn item of a template rules out.
fn precluded_of<S: Store>(s: &Server<S>, template: u32) -> u32 {
    s.catalog
        .get(template)
        .and_then(|t| fit_of(&t.props))
        .map_or(0, |f| f.precluded)
}

/// Take off a worn item: to the pack's front by its own use.
fn take_off<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    item: u64,
    made_room: bool,
) -> bool {
    take_off_to(s, net, world, item, made_room, 0)
}

/// Take off a worn item into the pack at a slot: answered, the look without it, the wearer
/// cleared. Refused (the pack full) when there is no room: returns whether it came off.
fn take_off_to<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    item: u64,
    made_room: bool,
    slot: u32,
) -> bool {
    let slot = crate::inventory::named_slot(slot);
    let Some(p) = s.players.get(&world) else {
        return false;
    };
    let (player, race, sex) = (p.character, p.race, p.sex);
    let Some(h) = p.play.items.iter().find(|h| h.id == item).copied() else {
        return false;
    };
    let fit = s
        .catalog
        .get(h.template)
        .and_then(|t| fit_of(&t.props))
        .unwrap_or_default();
    let full = crate::inventory::pack_full(&p.play.items);
    // Its effects go just before the answer, unless it cannot come off.
    let stopped = if full {
        Vec::new()
    } else {
        effects::take_off_worn(s, net, world, item)
    };
    let done = session::call(
        function::UNEQUIP_DONE,
        &[desc(&Answer {
            item,
            player,
            taken: h.location | fit.precluded,
            precluded: fit.precluded,
            location: h.location,
            target_slot: slot,
            slot,
            flags: if made_room {
                flags::MADE_ROOM
            } else {
                flags::TAKEN_OFF
            },
            status: if full { PACK_FULL } else { 0 },
            ..Answer::default()
        })],
    )
    .expect("packs");
    progress::private(net, world, &done);
    if full {
        return false;
    }
    effects::stop_fx(s, net, world, &stopped);
    let look = s
        .catalog
        .get(h.template)
        .and_then(|t| worn_look(&t.props, race, sex, &crate::treasure::Own::default()));
    if let Some(p) = s.players.get_mut(&world) {
        crate::inventory::insert_at(&mut p.play.items, slot);
        if let Some(h) = p.play.items.iter_mut().find(|h| h.id == item) {
            h.location = 0;
            h.slot = slot;
        }
        if let Some((table, _)) = &look {
            take_from(&mut p.look, *table);
        }
    }
    if look.is_some() {
        show_look(s, net, world);
    }
    let m = iid_update(s, item, q::EQUIPPER, 0);
    crate::inventory::to_holder(s, net, world, item, &m);
    if let Some(t) = s.things.get_mut(item) {
        t.wielded = None;
        t.container = Some((player, slot));
    }
    crate::world::keep_player(s, world);
    true
}

fn player_stamp<S: Store>(s: &mut Server<S>, world: ConnId) -> Option<(u64, ObjectStamp)> {
    let p = s.players.get_mut(&world)?;
    p.visual = p.visual.wrapping_add(1);
    Some((
        p.character,
        ObjectStamp {
            other_stamp: p.visual,
            ..p.stamp()
        },
    ))
}

/// The hand a use puts a thing in: the one it was last held in, else the one it prefers, whatever
/// the other hand holds (2017, 213 uses of one-handed weapons: 58 of 58 last in the off hand went
/// there, 138 of 138 last in the primary hand went there, 17 of 17 never seen held to the primary
/// hand; so a second weapon reaches the off hand by a directive, not by a use).
fn hand_of_use<S: Store>(s: &Server<S>, item: u64, template: u32) -> u32 {
    let hold = hold_of(&s.catalog, template);
    s.things
        .get(item)
        .map(|t| t.own.hand)
        .filter(|hand| hold.valid & hand != 0)
        .unwrap_or(hold.preferred)
}

/// Is a use of this template from the pack a wielding: does it name a hand, or is it known as something
/// wielded? (2017: of 931 uses of items in the pack naming no place, none was wielded, nor of 10,433 of
/// anything naming none. A thing with no use of its own is refused: 7 of the 7 such uses of
/// motes and broken medallions.)
fn wields_by_use<S: Store>(s: &Server<S>, template: u32) -> bool {
    crate::skill_use::implement_of(&s.catalog, template).is_some()
        || s.catalog.get(template).is_some_and(|t| {
            t.props
                .word("Item_ValidInventoryLocations")
                .is_some_and(|v| v & (PRIMARY_HAND | OFF_HAND) != 0)
        })
}

/// Wield an item of the pack in a hand, by its use or by a directive naming the hand: whatever is in
/// the way put back where the item was in the pack, the answer naming it (2017: 380 of 380).
fn wield<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    item: u64,
    location: u32,
    directed: Option<u32>,
    clock: Clock,
) {
    let Some(template) = s.things.get(item).map(|t| t.template) else {
        return;
    };
    let mut hold = hold_of(&s.catalog, template);
    // A hand it may not go to is its preferred one.
    let location = if hold.valid & location == 0 {
        hold.preferred
    } else {
        location
    };
    hold.taken = location | hold.precluded;
    if let Some(f) = directed {
        hold.flags = f;
    }
    // Whatever is in the way goes back first.
    let wanted = hold.taken | hold.precluded;
    let wielded: Vec<(u64, u32)> = s
        .players
        .get(&world)
        .map(|p| {
            p.play
                .items
                .iter()
                .filter(|h| h.location != 0 && h.location & BODY_PLACES == 0)
                .filter(|h| h.location & wanted != 0)
                .map(|h| (h.id, h.location))
                .collect()
        })
        .unwrap_or_default();
    let from_slot = s
        .players
        .get(&world)
        .and_then(|p| p.play.items.iter().find(|h| h.id == item))
        .map_or(0, |h| h.slot);
    let mut blocking = (0, 0);
    for (w, at) in wielded {
        put_away(s, net, world, w, true, Some((from_slot, 1.0)));
        blocking = (w, at & !hold.precluded);
    }
    let Some(player) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    let slot = s
        .players
        .get(&world)
        .and_then(|p| p.play.items.iter().find(|h| h.id == item))
        .map_or(0, |h| h.slot);
    // Its effects come on just before the answer.
    effects_on(s, net, world, item, clock);
    // The answer first, while the item is still in the pack where the client looks for it
    // (2017's order: the answer, then the weapon drawn and the item's moves).
    let done = session::call(
        function::EQUIP_DONE,
        &[desc(&Answer {
            item,
            player,
            fit: Fit {
                valid: hold.valid,
                preferred: hold.preferred,
                primary: hold.primary,
                secondary: hold.secondary,
                ..Fit::default()
            },
            taken: hold.taken,
            precluded: hold.precluded,
            location,
            target_slot: 0,
            slot,
            flags: hold.flags,
            blocking,
            binds_on_use: binds_on_use(s, item),
            ..Answer::default()
        })],
    )
    .expect("packs");
    progress::private(net, world, &done);
    if binds_on_use(s, item) {
        progress::private(net, world, &binds_warning());
    }
    // The weapon drawn: only for the primary hand, and only it takes a stamp: the client waits for
    // each stamp in turn before it plays what comes after.
    if location == PRIMARY_HAND {
        let Some((_, stamp)) = player_stamp(s, world) else {
            return;
        };
        let drawn = ServerMessage::MoveSlider(ObjectEvent {
            sender: stamp,
            action: Slide {
                slider: WEAPON_DRAWN,
                value: 0.0,
                time: 0.1,
            },
        });
        to_all_seeing(s, net, player, &drawn);
    }
    // The wielder, and those who see it and know the item, are told the item's holders and see it
    // attached; those who do not know it have it made in the hand (below).
    let watching = crate::presence::watching(s, world, player, item);
    for (quality, value) in [
        (q::CONTAINER, 0),
        (q::EQUIPPER, player),
        (q::CONTAINER, player),
    ] {
        let m = iid_update(s, item, quality, value).encode();
        net.send(world, queue::EVENT, BlobKind::VISUAL, m.clone());
        for (c, _) in watching.iter().filter(|(_, knows)| *knows) {
            net.send(*c, queue::EVENT, BlobKind::VISUAL, m.clone());
        }
    }
    // It hangs from its primary place in its preferred hand, else from its secondary (2017: 504 of
    // 504 and 63 of 63).
    let hang = if location == hold.preferred {
        hold.primary
    } else {
        hold.secondary
    };
    let attach = attach_message(s, item, player, hang).encode();
    if let Some(stream) = net.new_stream(world) {
        net.send_ephemeral(world, queue::EVENT, stream, attach.clone());
    }
    for (c, _) in watching.iter().filter(|(_, knows)| *knows) {
        crate::presence::on_its_stream(s, net, *c, item, attach.clone());
    }
    if let Some(p) = s.players.get_mut(&world) {
        if let Some(h) = p.play.items.iter_mut().find(|h| h.id == item) {
            h.location = hold.taken;
        }
        // The client's pack closes up behind it.
        crate::inventory::close_up(&mut p.play.items);
    }
    if let Some(t) = s.things.get_mut(item) {
        t.wielded = Some((player, location));
    }
    let instance = s.players.get(&world).map_or(0, |p| p.instance);
    if let Some(made) = s.things.get(item).map(|t| {
        ServerMessage::CreateObject(crate::inventory::held_create(&s.catalog, t, instance))
    }) {
        for (c, _) in watching.iter().filter(|(_, knows)| !*knows) {
            crate::presence::make_for(s, net, *c, item, &made);
        }
    }
    crate::world::keep_player(s, world);
}

fn attach_message<S: Store>(
    s: &mut Server<S>,
    item: u64,
    player: u64,
    location: u32,
) -> ServerMessage {
    let (instance, next) = s.things.get_mut(item).map_or((0, 0), |t| {
        t.position_stamp = t.position_stamp.wrapping_add(1);
        (t.instance, t.position_stamp)
    });
    let player_instance = s
        .players
        .values()
        .find(|p| p.character == player)
        .map_or(1, |p| p.instance);
    ServerMessage::Parent(ObjectEvent {
        sender: ObjectStamp {
            id: item,
            instance_stamp: instance,
            other_stamp: 0,
        },
        action: Attach {
            parent: ObjectStamp {
                id: player,
                instance_stamp: player_instance,
                other_stamp: next,
            },
            location,
            orientation: 0,
        },
    })
}

/// Put a wielded item back in the pack: by its own use, or to make room for another; at the
/// pack's end, or at a slot named with the value the weapon slider goes to.
fn put_away<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    item: u64,
    made_room: bool,
    to: Option<(u32, f32)>,
) {
    let Some(template) = s.things.get(item).map(|t| t.template) else {
        return;
    };
    let mut hold = hold_of(&s.catalog, template);
    let held_at = s
        .players
        .get(&world)
        .and_then(|p| p.play.items.iter().find(|h| h.id == item))
        .map_or(hold.taken, |h| h.location);
    if held_at == OFF_HAND {
        hold.taken = OFF_HAND;
        hold.precluded = 0;
    }
    let location = if held_at == OFF_HAND {
        OFF_HAND
    } else {
        PRIMARY_HAND
    };
    hold.taken = location | hold.precluded;
    let Some(player) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    // Back at the pack's end, where the client puts it, or at the slot named.
    let slot = match to {
        Some((slot, _)) => crate::inventory::named_slot(slot),
        None => s
            .players
            .get(&world)
            .map_or(0, |p| crate::inventory::end_slot(&p.play.items)),
    };
    let contain = s.things.get_mut(item).map(|t| {
        t.position_stamp = t.position_stamp.wrapping_add(1);
        ObjectStamp {
            id: item,
            instance_stamp: t.instance,
            other_stamp: t.position_stamp,
        }
    });
    if let Some(c) = contain {
        net.send(
            world,
            queue::EVENT,
            BlobKind::UNORDERED,
            ServerMessage::Contain(auberean_protocol::physics::ObjectOnly(c)).encode(),
        );
    }
    // Its effects go just before the answer.
    let stopped = effects::take_off_worn(s, net, world, item);
    let done = session::call(
        function::UNEQUIP_DONE,
        &[desc(&Answer {
            item,
            player,
            taken: hold.taken,
            precluded: hold.precluded,
            location,
            target_slot: slot,
            slot,
            flags: if made_room {
                flags::MADE_ROOM
            } else {
                flags::TAKEN_OFF
            },
            ..Answer::default()
        })],
    )
    .expect("packs");
    progress::private(net, world, &done);
    effects::stop_fx(s, net, world, &stopped);
    // The weapon slider: only for the primary hand.
    if location == PRIMARY_HAND {
        let Some((_, stamp)) = player_stamp(s, world) else {
            return;
        };
        let sheathed = ServerMessage::MoveSlider(ObjectEvent {
            sender: stamp,
            action: Slide {
                slider: WEAPON_DRAWN,
                value: to.map_or(1.0, |(_, v)| v),
                time: 0.1,
            },
        });
        to_all_seeing(s, net, player, &sheathed);
    }
    let m = iid_update(s, item, q::EQUIPPER, 0).encode();
    net.send(world, queue::EVENT, BlobKind::VISUAL, m.clone());
    // Those who see it see the item let go of: the holder lets go of the item, and those who know
    // the item are told it has no holder (2017: 543 of 659 of the next visual messages about an item
    // after its holder let go of it took the next stamp, those read holding no wearer).
    let player_stamp = s.players.get(&world).map(crate::world::Player::stamp);
    let item_stamp = s.things.get(item).map(|t| ObjectStamp {
        id: item,
        instance_stamp: t.instance,
        other_stamp: t.position_stamp,
    });
    if let (Some(sender), Some(child)) = (player_stamp, item_stamp) {
        let gone = ServerMessage::DeParent(ObjectEvent {
            sender: ObjectStamp {
                other_stamp: 0,
                ..sender
            },
            action: child,
        })
        .encode();
        for (c, knows) in crate::presence::watching(s, world, player, item) {
            net.send(c, queue::EVENT, BlobKind::UNORDERED, gone.clone());
            if knows {
                net.send(c, queue::EVENT, BlobKind::VISUAL, m.clone());
            }
        }
    }
    if let Some(p) = s.players.get_mut(&world) {
        crate::inventory::insert_at(&mut p.play.items, slot);
        if let Some(h) = p.play.items.iter_mut().find(|h| h.id == item) {
            h.location = 0;
            h.slot = slot;
        }
    }
    if let Some(t) = s.things.get_mut(item) {
        t.wielded = None;
        t.container = Some((player, slot));
        t.own.hand = location;
    }
    crate::world::keep_player(s, world);
}

/// The item, the place and the pack slots an equipping directive names.
fn directive(v: &[crate::calls::Value]) -> Option<(u64, u32, u32, u32)> {
    use crate::calls::Node;
    let Node::Script { package, slots } = v.first()?.root()? else {
        return None;
    };
    if *package != EQUIP_DESC.0 || slots.len() != EQUIP_DESC.1 {
        return None;
    }
    let item = u64::from(slots[4].1) << 32 | u64::from(slots[3].1);
    Some((item, slots[7].1, slots[6].1, slots[18].1))
}

/// A player drags an item of its pack to a place on its paper doll: worn or wielded there, what is
/// in the way taken off to where the item was in the pack, and answered as a directive.
pub fn directive_equip<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    v: &[crate::calls::Value],
    clock: Clock,
) {
    let Some((item, location, _, _)) = directive(v) else {
        s.unhandled.note("an equip directive that does not read");
        return;
    };
    let Some(h) = s
        .players
        .get(&world)
        .and_then(|p| p.play.items.iter().find(|h| h.id == item))
        .copied()
    else {
        return;
    };
    if h.location != 0 {
        return;
    }
    let fit = s.catalog.get(h.template).and_then(|t| fit_of(&t.props));
    if let Some(why) = refused_item(s, world, &h, clock) {
        // Answered as the equipping it asked for, with the refusal for its status.
        let player = s.players.get(&world).map_or(0, |p| p.character);
        let hold = hold_of(&s.catalog, h.template);
        let fit = fit.unwrap_or(Fit {
            valid: hold.valid,
            preferred: hold.preferred,
            primary: hold.primary,
            secondary: hold.secondary,
            ..Fit::default()
        });
        let no = session::call(
            function::EQUIP_DONE,
            &[desc(&Answer {
                item,
                player,
                fit,
                taken: location,
                location,
                slot: h.slot,
                flags: flags::DIRECTED,
                status: why,
                ..Answer::default()
            })],
        )
        .expect("packs");
        progress::private(net, world, &no);
        return;
    }
    match fit {
        Some(fit) => wear(
            s,
            net,
            world,
            item,
            fit,
            Some(location),
            flags::DIRECTED,
            clock,
        ),
        None => {
            let hand = if location == OFF_HAND {
                OFF_HAND
            } else {
                PRIMARY_HAND
            };
            wield(s, net, world, item, hand, Some(flags::DIRECTED), clock);
        }
    }
}

/// A player drags an item it wears or wields to a slot of its pack: taken off to that slot.
pub fn directive_unequip<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    v: &[crate::calls::Value],
) {
    let Some((item, _, target, _)) = directive(v) else {
        s.unhandled.note("an unequip directive that does not read");
        return;
    };
    let Some(at) = s
        .players
        .get(&world)
        .and_then(|p| p.play.items.iter().find(|h| h.id == item))
        .map(|h| h.location)
    else {
        return;
    };
    if at == 0 {
        return;
    }
    if at & BODY_PLACES != 0 {
        take_off_to(s, net, world, item, false, target);
    } else {
        put_away(s, net, world, item, false, Some((target, 0.0)));
    }
}

/// Does an item bind to its holder when used in combat, not having been yet?
fn binds_on_use<S: Store>(s: &Server<S>, item: u64) -> bool {
    s.things.get(item).is_some_and(|t| t.binds_on_use)
}

/// The warning an item that binds when used in combat is equipped with (not shown on top).
#[must_use]
pub fn binds_warning() -> ServerMessage {
    session::call(
        DISPLAY_MESSAGE,
        &[
            Arg::Int(0),
            crate::chat::text_arg(crate::progress::text(BINDS_WARNING)),
        ],
    )
    .expect("a string reference packs")
}

/// A player is hit: the blow lands on one piece of what it wears, and that piece binds if it is
/// marked to (2017: 30 of 30 armor pieces told within 1.1 s after a hit taken; each of the 5 sets
/// in view told its first piece on its first hit, the next ones in 2.0, 2.8 and 3.6 hits, the
/// pieces in no fixed order. Which piece a blow lands on is not told, so a piece at even odds).
pub fn hit_taken<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let worn: Vec<u64> = p
        .play
        .items
        .iter()
        .filter(|h| h.location & BODY_PLACES != 0)
        .map(|h| h.id)
        .collect();
    // No dice for a player with nothing left to bind.
    if !worn.iter().any(|id| binds_on_use(s, *id)) {
        return;
    }
    let piece = worn[s.rng.below(worn.len())];
    if binds_on_use(s, piece) {
        bind(s, net, world, piece);
    }
}

/// A player's blow lands: what it holds that is marked to bind may bind.
pub fn blow_landed<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let held: Vec<u64> = p
        .play
        .items
        .iter()
        .filter(|h| h.location != 0 && h.location & BODY_PLACES == 0)
        .map(|h| h.id)
        .collect();
    for id in held {
        if binds_on_use(s, id) && s.rng.chance(SWING_BINDS) {
            bind(s, net, world, id);
        }
    }
}

/// An item binds to its holder, and the holder is told.
fn bind<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, item: u64) {
    let Some(me) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    let Some(t) = s.things.get_mut(item) else {
        return;
    };
    t.binds_on_use = false;
    t.own.bound_to = me;
    let (template, made) = (t.template, t.own.made);
    let Some(entry) = s.catalog.get(template) else {
        return;
    };
    let name = made.map_or_else(
        || crate::profile::item_name(&entry.facts),
        |m| crate::profile::made_name(&entry.facts, m.prefix),
    );
    let told = progress::text_with(BOUND_TEXT, vec![progress::nested(BOUND_TEXT_ITEM, name)]);
    progress::private(
        net,
        world,
        &crate::chat::show_text(crate::social::NOTICE, told),
    );
}
