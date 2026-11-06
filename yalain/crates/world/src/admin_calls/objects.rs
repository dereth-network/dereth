//! An admin's commands over objects and items: making them, deleting them, what a container holds,
//! emptying it, dropping it, and money.

use yalain_net::ConnId;
use yalain_store::{Store, StoredPosition};

use super::{other, Args, Cx, Target};
use crate::things::{Kind, Thing};
use crate::{inventory, presence, progress, quals};

/// The calls, by their function ids.
pub mod function {
    pub const CREATE: u32 = 0x8201_006F;
    pub const CREATE_ENTITY: u32 = 0x8201_006C;
    pub const CREATE_DISTANT: u32 = 0x8201_0007;
    pub const CREATE_INTERNAL: u32 = 0x8201_006A;
    pub const DELETE: u32 = 0x8201_002E;
    pub const GENERATE: u32 = 0x8201_0037;
    pub const RESET_GENERATOR: u32 = 0x8201_006D;
    pub const DUMP_GENERATOR: u32 = 0x8201_000C;
    pub const CONTENTS: u32 = 0x8201_004A;
    pub const PURGE: u32 = 0x8201_0070;
    pub const PURGE_WORN: u32 = 0x8201_0020;
    pub const FUMBLE: u32 = 0x8201_001F;
    pub const RESET_SEGMENTS: u32 = 0x8201_0120;
}

/// How far in front of the admin an object is made, and how far `/createdistant` makes one.
const NEAR: f32 = 2.0;
const DISTANT: f32 = 20.0;
/// The most creatures one `/generate` makes.
const MOST_GENERATED: i32 = 100;

/// A call of this kind: its answer; `None` for any other.
pub(super) fn call<S: Store>(cx: &mut Cx<'_, S>, f: u32, a: &Args<'_>) -> Option<Vec<String>> {
    Some(match f {
        function::CREATE | function::CREATE_ENTITY => make(cx, a, NEAR),
        function::CREATE_DISTANT => make(cx, a, DISTANT),
        function::CREATE_INTERNAL => make_inside(cx, a),
        function::DELETE => delete(cx, a),
        function::GENERATE => generate(cx, a),
        function::RESET_GENERATOR => reset_generator(cx, a),
        function::DUMP_GENERATOR => dump_generator(cx, a),
        function::CONTENTS => contents(cx, a),
        function::PURGE => purge(cx, a, false),
        function::PURGE_WORN => purge(cx, a, true),
        function::FUMBLE => fumble(cx, a),
        function::RESET_SEGMENTS => reset_segments(cx, a),
        other::MONEY => money(cx, a),
        other::STORE_OPEN | other::STORE_CLOSE | other::STORE_EXPIRE => {
            vec!["This server has no player stores of an admin's own.".to_owned()]
        }
        _ => return None,
    })
}

/// The template a word names: its id in hex (this server holds templates by id, not by the
/// original's package names).
fn template_of<S: Store>(cx: &Cx<'_, S>, word: &str) -> Result<u32, String> {
    let id = u32::from_str_radix(word.trim().trim_start_matches("0x"), 16).map_err(|_| {
        format!("{word}: give a template's id in hex; this server has no names for them.")
    })?;
    if cx.s.catalog.get(id).is_some() {
        Ok(id)
    } else {
        Err(format!("The world has no template {id:08X}."))
    }
}

/// Make an object of a template on the landscape, `ahead` meters in front of the admin: a creature
/// that fights if the template is one, else an item at rest.
fn make<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>, ahead: f32) -> Vec<String> {
    let template = match template_of(cx, &a.text(0).unwrap_or_default()) {
        Ok(t) => t,
        Err(why) => return vec![why],
    };
    let Some(p) = cx.s.players.get(&cx.world) else {
        return Vec::new();
    };
    let at = in_front(&p.position, ahead);
    let Some(t) = cx.s.catalog.get(template).cloned() else {
        return Vec::new();
    };
    let id = cx.s.things.new_id();
    let kind = if yalain_content::CreatureStats::from_props(&t.props).is_some() {
        Kind::Creature
    } else {
        Kind::Item
    };
    let mut thing = Thing::of(id, &t, kind);
    thing.at = Some(at);
    thing.quantity = thing.quantity.max(1);
    cx.s.things.insert(thing);
    presence::thing_appears(cx.s, cx.net, id);
    vec![format!("Made {id:#x} of template {template:08X}.")]
}

/// A place `ahead` meters in front of where a player stands.
fn in_front(p: &StoredPosition, ahead: f32) -> StoredPosition {
    let [w, _, _, z] = p.rotation;
    // The heading's direction from the rotation about the vertical.
    let (dx, dy) = (-2.0 * w * z, 1.0 - 2.0 * z * z);
    StoredPosition {
        origin: [
            p.origin[0] + dx * ahead,
            p.origin[1] + dy * ahead,
            p.origin[2],
        ],
        ..*p
    }
}

fn make_inside<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let template = match template_of(cx, &a.text(0).unwrap_or_default()) {
        Ok(t) => t,
        Err(why) => return vec![why],
    };
    match inventory::give(cx.s, cx.net, cx.world, template, 1) {
        Some(id) => vec![format!(
            "Made {id:#x} of template {template:08X} in your pack."
        )],
        None => vec![format!("The template {template:08X} could not be made.")],
    }
}

fn delete<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let id = a.long(0).unwrap_or(0);
    match cx.target(id) {
        Some(Target::Thing(id)) => {
            let placed = cx.s.things.get(id).is_some_and(|t| t.at.is_some());
            if !placed {
                return vec!["Take it out of what holds it first.".to_owned()];
            }
            crate::death::vanish(cx.s, cx.net, id);
            vec![format!("Deleted {id:#x}.")]
        }
        Some(Target::Player(_)) => vec!["A player cannot be deleted so.".to_owned()],
        None => vec![format!("Nothing has the id {id:#x}.")],
    }
}

/// `/generate <profile> <quantity>`: run the camp the words name, by its index or id, that many
/// times, at most `MOST_GENERATED`.
fn generate<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let word = a.text(0).unwrap_or_default();
    let quantity = a.signed(1).unwrap_or(1).clamp(1, MOST_GENERATED);
    let Some(i) = cx.s.spawns.camps.iter().position(|c| c.id == word) else {
        return vec![format!("The world has no camp {word}.")];
    };
    let now = cx.clock.local.0;
    let mut made = 0;
    for _ in 0..quantity {
        if let Some(thing) = crate::spawn::spawn(cx.s, i, now) {
            presence::thing_appears(cx.s, cx.net, thing);
            made += 1;
        }
    }
    vec![format!("Camp {word} made {made}.")]
}

/// The camp a thing of the world belongs to.
fn camp_of<S: Store>(cx: &Cx<'_, S>, id: u64) -> Option<usize> {
    cx.s.things.get(id)?.creature.as_ref()?.camp
}

/// `/resetgenerator`: the camp of the target clears its creatures, and makes them again as it does.
fn reset_generator<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let id = a.long(0).unwrap_or(0);
    let Some(camp) = camp_of(cx, id) else {
        return vec![format!("{id:#x} is not of a camp.")];
    };
    let gone: Vec<u64> =
        cx.s.things
            .placed()
            .filter(|t| t.creature.as_ref().is_some_and(|c| c.camp == Some(camp)))
            .map(|t| t.id)
            .collect();
    for g in &gone {
        crate::death::vanish(cx.s, cx.net, *g);
    }
    vec![format!("Cleared {} creatures of the camp.", gone.len())]
}

fn dump_generator<S: Store>(cx: &Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let id = a.long(0).unwrap_or(0);
    let Some(camp) = camp_of(cx, id).and_then(|i| cx.s.spawns.camps.get(i)) else {
        return vec![format!("{id:#x} is not of a camp.")];
    };
    vec![format!(
        "Camp {} at {:08X} {:?}: radius {}, templates {:08X?}.",
        camp.id,
        camp.cell,
        camp.origin,
        camp.radius,
        camp.templates.iter().map(|(t, _)| *t).collect::<Vec<_>>()
    )]
}

fn contents<S: Store>(cx: &Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let w = match cx.player(a.long(0).unwrap_or(0)) {
        Ok(w) => w,
        Err(why) => return vec![why],
    };
    let Some(p) = cx.s.players.get(&w) else {
        return Vec::new();
    };
    let mut lines = vec![format!("{} holds {} items.", p.name, p.play.items.len())];
    lines.extend(p.play.items.iter().map(|h| {
        format!(
            "{:#x}: template {:08X}, slot {}, worn at {:#x}",
            h.id, h.template, h.slot, h.location
        )
    }));
    lines
}

/// The items of a player a purge takes: only what it wears, or what is in its pack.
fn purged<S: Store>(cx: &Cx<'_, S>, w: ConnId, worn: bool) -> Vec<(u64, u32)> {
    cx.s.players.get(&w).map_or_else(Vec::new, |p| {
        p.play
            .items
            .iter()
            .filter(|h| (h.location != 0) == worn)
            .map(|h| (h.id, h.template))
            .collect()
    })
}

fn purge<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>, worn: bool) -> Vec<String> {
    let w = match cx.player(a.long(0).unwrap_or(0)) {
        Ok(w) => w,
        Err(why) => return vec![why],
    };
    let items = purged(cx, w, worn);
    for (id, template) in &items {
        inventory::take_item(cx.s, cx.net, w, *id, *template);
    }
    crate::world::keep_player(cx.s, w);
    vec![format!(
        "Took {} items from {}.",
        items.len(),
        cx.name_of(w)
    )]
}

/// `/fumble`: a player drops everything in its pack on the ground at its feet.
fn fumble<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let w = match cx.player(a.long(0).unwrap_or(0)) {
        Ok(w) => w,
        Err(why) => return vec![why],
    };
    let items = purged(cx, w, false);
    let Some(at) = cx.s.players.get(&w).map(|p| p.position) else {
        return Vec::new();
    };
    let mut dropped = 0;
    for (id, template) in &items {
        let Some(template_t) = cx.s.catalog.get(*template).cloned() else {
            continue;
        };
        if inventory::take_item(cx.s, cx.net, w, *id, *template) {
            let nid = cx.s.things.new_id();
            let mut t = Thing::of(nid, &template_t, Kind::Item);
            t.at = Some(at);
            cx.s.things.insert(t);
            presence::thing_appears(cx.s, cx.net, nid);
            dropped += 1;
        }
    }
    crate::world::keep_player(cx.s, w);
    vec![format!("{} dropped {dropped} items.", cx.name_of(w))]
}

fn reset_segments<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let w = match cx.player(a.long(0).unwrap_or(0)) {
        Ok(w) => w,
        Err(why) => return vec![why],
    };
    let m = inventory::reset_contents(cx.s, w);
    progress::private(cx.net, w, &m);
    vec![format!("{}'s pack is told again.", cx.name_of(w))]
}

fn money<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let add = a.signed(0).unwrap_or(0);
    let Some(p) = cx.s.players.get_mut(&cx.world) else {
        return Vec::new();
    };
    let now = crate::store::money(&p.qualities).cast_signed();
    let total = now.saturating_add(add).max(0);
    quals::set_int(&mut p.qualities, crate::store::MONEY, total);
    progress::send_int(cx.net, cx.world, crate::store::MONEY, total);
    vec![format!("You hold {total}.")]
}
