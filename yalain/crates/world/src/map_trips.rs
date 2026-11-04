//! People who send players where their surveyor's maps lead: the cartomancers of each land, and
//! a miller with the starter map his errand asks for.
//!
//! **Who, and which maps.** A person who has a line for it at a give sends players with the maps
//! of its own land (its template's table names the land: a cartomancer's "map teleporter" of
//! Linvak, Omishan or Osteth; a map's land is the one its summoning effect names) and with the maps
//! its lines name (2017: 46 maps to the cartomancers of their own land, the miller's starter map,
//! 47 of 47 sent).
//!
//! **Where.** To the place the world names for the map's template: one place for each location a
//! map marks, whatever its material (2017: every trip of a map's template came out at the same
//! point, facing the same way, 47 of 47; maps of three materials marking one location came out at
//! one place). A map whose place the world does not hold is refused as one of another land.
//!
//! **How.** The give is answered as any give; the person says its line and hands the map back as
//! a person hands back an item (its handing-over, the map back 1.30 s later: 46 of 46 within
//! 1.30-1.41 s); in the instant the map is back the player goes into portal space (47 of 47 within
//! 0.1 s), and comes out at the place as from any trip. No portal is made (0 of 47 windows made
//! one) and no one else goes: only the giver is sent. The map is kept.
//!
//! **Refused.** A map it does not send anyone with (another land's) is handed back with the
//! person's own line for such a map ("... my magic is not strong enough to send you to a different
//! continent.", "I do not work with such maps."), else its refusal of a give; anything else is
//! refused as any give no step wants.

use yalain_content::talk::Role;
use yalain_net::{ConnId, ServerNet};
use yalain_store::{Store, StoredPosition};

use crate::inventory::Move;
use crate::{Clock, Server};

/// The table that makes a person send players with its land's maps, and the land after it.
pub const TABLE: &str = "NPCTable_MapTeleporter_";

/// What a person does with a map given it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sends {
    /// It is no person who sends players by maps, or the item is no map.
    Not,
    /// A map it does not send anyone with.
    Refuses,
    /// It sends the giver here.
    To(StoredPosition),
}

/// What the person at `i` (in the world's objects) does with an item of `item` given it.
#[must_use]
pub fn sends<S: Store>(s: &Server<S>, i: usize, item: u32) -> Sends {
    let who = s.content.objects[i].template;
    let lines: Vec<Option<u32>> = s
        .talk
        .of(who, Some(i))
        .filter(|l| l.when == yalain_content::talk::When::Give && l.role == Some(Role::Teleport))
        .map(|l| l.item)
        .collect();
    if lines.is_empty() {
        return Sends::Not;
    }
    let Some(map) = s.catalog.get(item).map(|t| &t.props) else {
        return Sends::Not;
    };
    let Some(land_of_map) = crate::usable::maps::region(map) else {
        return Sends::Not;
    };
    let land = s
        .catalog
        .get(who)
        .and_then(|t| t.props.text("AI_NPCTable"))
        .and_then(|t| t.strip_prefix(TABLE));
    let takes = lines.contains(&Some(item)) || land.is_some_and(|l| l == land_of_map);
    match s.places_named.get(&item) {
        Some(p) if takes => Sends::To(StoredPosition {
            cell: p.cell,
            origin: p.origin,
            rotation: p.rotation,
        }),
        _ => Sends::Refuses,
    }
}

/// The person at `i` sends the giver where the map leads: its line, then the map handed back and
/// the giver into portal space as it is back.
pub fn send<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    i: usize,
    m: &Move,
    to: StoredPosition,
    clock: Clock,
) {
    let item = s.things.get(m.item).map_or(0, |t| t.template);
    say(s, net, world, i, Role::Teleport, item);
    let object = s.content.objects[i].kind == yalain_content::Kind::Static;
    crate::gives::hand_back(s, net, world, i, m, object, Some(to), clock);
}

/// The person at `i` refuses a map: its line for a map it does not take (else its refusal of a
/// give), and the map back.
pub fn refuse<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    i: usize,
    m: &Move,
    clock: Clock,
) {
    let item = s.things.get(m.item).map_or(0, |t| t.template);
    let who = s.content.objects[i].template;
    let line = crate::talk::at_give_as(s, who, Some(i), Role::RefuseMap, item)
        .or_else(|| crate::talk::at_give(s, who, Some(i), item));
    if let Some(line) = line {
        let speaker = crate::talk::content_speaker(s, i);
        crate::talk::say(s, net, world, &speaker, &line.words, line.how);
    }
    let object = s.content.objects[i].kind == yalain_content::Kind::Static;
    crate::gives::hand_back(s, net, world, i, m, object, None, clock);
}

fn say<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    i: usize,
    role: Role,
    item: u32,
) {
    let who = s.content.objects[i].template;
    if let Some(line) = crate::talk::at_give_as(s, who, Some(i), role, item) {
        let speaker = crate::talk::content_speaker(s, i);
        crate::talk::say(s, net, world, &speaker, &line.words, line.how);
    }
}
