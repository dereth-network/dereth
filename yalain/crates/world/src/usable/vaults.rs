//! Vaults: a locked portal to a dungeon that only its glyph opens.
//!
//! **A glyph on its vault.** A player uses a glyph it holds on the vault, walking to within five
//! meters first. It plays its use, and a moment later the vault flashes and the player goes
//! through portal space to the vault's one destination. On arrival the glyph is used up and the
//! use is answered "Your glyph fits the lock.".
//!
//! **Refusals.** A vault used bare, or walked into, needs its glyph: refused with the vault's
//! flash. While another player's trip through the vault is under way a use is refused at once; a
//! trip that would come within five seconds of the vault's last is refused as its use ends. The
//! glyph is kept on every refusal.
//!
//! **Locked portal doors.** A portal that stands as a door, or a door that leads somewhere, locked,
//! is a vault its key opens, with a door's answers: used bare it is locked, from wherever the
//! player stands (its lock is a portal's, not a door's), and another key is the wrong one. A player
//! its quest keeps out is refused at once. Its own trips wait three seconds between them. On
//! arrival its key is used up unless the key's kind keeps it, and what the door's kind names for its
//! use moves its user's quests.

use std::collections::HashMap;

use auberean_protocol::physics::Fx;
use auberean_protocol::types::BehaviorParams;
use auberean_protocol::ServerMessage;
use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;

use crate::places::{self, Use};
use crate::{presence, progress, Clock, Server};

/// How long after the glyph's use begins the vault opens, in seconds (2017: median 2.58 s, 45
/// trips).
pub const OPENS_AFTER: f64 = 2.58;

/// How long a vault waits between trips, in seconds.
pub const BETWEEN_TRIPS: f64 = 5.0;

/// The script package every portal door derives from.
pub const PORTAL_DOOR: &str = "PortalDoorTemplate";

/// How long a locked portal door waits between trips, in seconds (its kind's time between uses).
pub const DOOR_BETWEEN_TRIPS: f64 = 3.0;

/// Why a vault refuses, as the use's answer says, and each refusal's control flags.
pub mod refusal {
    /// It needs its glyph.
    pub const GLYPH_NEEDED: u32 = 10_033;
    pub const GLYPH_NEEDED_FLAGS: u32 = 956;
    /// Another player's trip through it is under way.
    pub const IN_USE: u32 = 1030;
    pub const IN_USE_FLAGS: u32 = 300;
    /// Its last trip was too recent.
    pub const TOO_SOON: u32 = 10_007;
    pub const TOO_SOON_FLAGS: u32 = 316;
}

/// A vault's trips: whose is under way, and when its last one left (world seconds).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Trips {
    pub under_way: Option<ConnId>,
    pub last: Option<f64>,
}

/// The vaults' trips, by vault.
pub type VaultTrips = HashMap<u64, Trips>;

/// A vault of the world, by its id: a locked object that takes a player somewhere. Its key id,
/// when one is known.
#[must_use]
pub fn vault<S: Store>(s: &Server<S>, id: u64) -> Option<Option<u32>> {
    let i = presence::content_by_id(&s.content.objects, id)?;
    s.content.objects[i].destination?;
    crate::usable::lock_of(s, id).map(|l| l.key)
}

/// Is a vault a portal door (a locked one, its key's to carry its user through): of a portal
/// door's kind, or of a door's that leads somewhere?
#[must_use]
pub fn portal_door<S: Store>(s: &Server<S>, id: u64) -> bool {
    presence::content_by_id(&s.content.objects, id).is_some_and(|i| {
        let package = s.content.objects[i].description.package;
        s.portal_door_packages.contains(&package) || s.doors.packages.contains_key(&package)
    })
}

/// How far a player stands from a vault.
fn distance_to<S: Store>(s: &Server<S>, from: &yalain_store::StoredPosition, id: u64) -> f32 {
    presence::content_by_id(&s.content.objects, id)
        .and_then(|i| {
            let o = &s.content.objects[i];
            places::reach(from, o.cell, o.origin)
        })
        .unwrap_or(0.0)
}

/// The vault flashes for everyone who sees it.
fn flash<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64) {
    let m = ServerMessage::DoFx(auberean_protocol::physics::ObjectEvent {
        sender: places::object_stamp(s, id),
        action: Fx {
            fx: places::fx::PORTAL_USED,
            scalar: 1.0,
        },
    });
    places::to_knowers(s, net, id, &m);
}

fn refuse(net: &mut ServerNet, world: ConnId, u: &Use, why: u32, flags: u32) {
    let m = places::use_refused(u, 0, Some(flags), 0.0, why);
    progress::private(net, world, &m);
}

/// A vault used bare, or walked into: it needs its glyph, and flashes; a locked portal door is
/// locked.
pub fn needs_glyph<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: &Use,
    vault: u64,
) {
    if portal_door(s, vault) {
        let distance = distance_to(s, &u.from, vault);
        let why = crate::usable::doors::refusal::LOCKED;
        let flags = crate::usable::doors::refusal_flags(why);
        let m = places::use_refused(u, 0, flags, distance, why);
        progress::private(net, world, &m);
        return;
    }
    refuse(
        net,
        world,
        u,
        refusal::GLYPH_NEEDED,
        refusal::GLYPH_NEEDED_FLAGS,
    );
    flash(s, net, vault);
}

/// A player in reach uses a glyph on a vault: refused, or its use plays and the vault opens a
/// moment later.
pub fn glyph_at_vault<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: Use,
    clock: Clock,
) {
    let vault_id = u.target;
    let Some(key) = vault(s, vault_id) else {
        return;
    };
    let door = portal_door(s, vault_id);
    if door {
        let gate = presence::content_by_id(&s.content.objects, vault_id)
            .and_then(|i| places::quest_keeps_out(s, world, &s.content.objects[i], clock));
        if let Some(why) = gate {
            let distance = distance_to(s, &u.from, vault_id);
            let flags = Some(places::USE_FLAGS_QUEST_KEEPS_OUT);
            let m = places::use_refused(&u, 0, flags, distance, why);
            progress::private(net, world, &m);
            return;
        }
    }
    let busy = s
        .vault_trips
        .get(&vault_id)
        .and_then(|t| t.under_way)
        .is_some_and(|w| w != world);
    if busy {
        refuse(net, world, &u, refusal::IN_USE, refusal::IN_USE_FLAGS);
        return;
    }
    let glyph = s
        .things
        .get(u.item)
        .and_then(|t| crate::usable::doors::key_id(s, t.template));
    if key.is_none() || glyph != key {
        if door {
            let distance = distance_to(s, &u.from, vault_id);
            let why = crate::usable::doors::refusal::WRONG_KEY;
            let m = places::use_refused(&u, 0, None, distance, why);
            progress::private(net, world, &m);
        } else {
            needs_glyph(s, net, world, &u, vault_id);
        }
        return;
    }
    let Some(i) = presence::content_by_id(&s.content.objects, vault_id) else {
        return;
    };
    let Some(to) = s.content.objects[i].destination else {
        return;
    };
    s.next_context = s.next_context.wrapping_add(1);
    let context = s.next_context;
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    p.walking_to = None;
    let plays = ServerMessage::PlayBehavior(places::player_event(
        p,
        BehaviorParams {
            bits: BehaviorParams::BEHAVIOR | places::USE_BEHAVIOR_FLAGS | BehaviorParams::CONTEXT,
            behavior: places::behavior::USES_ON,
            context,
            words: [0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0],
            ..BehaviorParams::default()
        },
    ));
    let me = p.character;
    p.trip = Some(places::Trip {
        to: yalain_store::StoredPosition {
            cell: to.cell,
            origin: to.origin,
            rotation: to.rotation,
        },
        used: Some(u),
        step: None,
        resurrect: false,
        sent: false,
        said: None,
        updates: Vec::new(),
    });
    places::to_all_knowing(s, net, me, &plays);
    s.vault_trips.entry(vault_id).or_default().under_way = Some(world);
    s.after(
        clock,
        OPENS_AFTER,
        crate::Timer::Trip {
            world,
            step: places::Step::Opened,
        },
    );
}

/// A player has left the world: a trip of its through a vault is under way no longer.
pub fn left<S: Store>(s: &mut Server<S>, world: ConnId) {
    for t in s.vault_trips.values_mut() {
        if t.under_way == Some(world) {
            t.under_way = None;
        }
    }
}

/// The glyph's use has played: a trip too soon after the vault's last is refused (the trip
/// dropped, the glyph kept); else the trip goes on. Returns whether it goes on.
pub fn opens<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    clock: Clock,
) -> bool {
    let Some(u) = s
        .players
        .get(&world)
        .and_then(|p| p.trip.as_ref())
        .and_then(|t| t.used.clone())
    else {
        return true;
    };
    if vault(s, u.target).is_none() {
        return true;
    }
    let now = clock.local.0;
    let between = if portal_door(s, u.target) {
        DOOR_BETWEEN_TRIPS
    } else {
        BETWEEN_TRIPS
    };
    let trips = s.vault_trips.entry(u.target).or_default();
    if trips.last.is_some_and(|t| now - t < between) {
        trips.under_way = None;
        if let Some(p) = s.players.get_mut(&world) {
            p.trip = None;
        }
        let name = presence::content_by_id(&s.content.objects, u.target)
            .and_then(|i| s.content.objects[i].description.name);
        let told = crate::usable::doors::fits_answer(s, &u, u.item, name);
        refuse(
            net,
            world,
            &told,
            refusal::TOO_SOON,
            refusal::TOO_SOON_FLAGS,
        );
        return false;
    }
    trips.last = Some(now);
    true
}

/// The trip through a vault has arrived: the glyph is used up (a portal door's key unless its
/// kind keeps it, and what the door's kind names for its use comes on its user), and the use is
/// told it fit. Returns the use to answer.
pub fn arrived<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: &Use,
    clock: Clock,
) -> Use {
    if vault(s, u.target).is_none() {
        return u.clone();
    }
    if let Some(t) = s.vault_trips.get_mut(&u.target) {
        t.under_way = None;
    }
    let found = presence::content_by_id(&s.content.objects, u.target);
    let name = found.and_then(|i| s.content.objects[i].description.name);
    let told = crate::usable::doors::fits_answer(s, u, u.item, name);
    let door = portal_door(s, u.target);
    if !door || !key_kept(s, u.item) {
        crate::craft::use_up(s, net, world, u.item, 1);
    }
    if let Some(template) = found
        .filter(|_| door)
        .map(|i| s.content.objects[i].template)
    {
        crate::usable::chests::effects_on_user(s, net, world, (u.target, template), clock);
    }
    told
}

/// Does a key's kind keep it when it opens its lock (its template says it is not used up)? A key
/// is used up unless it says so.
fn key_kept<S: Store>(s: &Server<S>, key: u64) -> bool {
    s.things
        .get(key)
        .and_then(|t| s.catalog.get(t.template))
        .is_some_and(|t| {
            matches!(
                t.props.get(DESTROYED_ON_USE),
                Some(yalain_content::Prop::Bool(false))
            )
        })
}

/// The property of an item's kind that says whether a use uses it up.
const DESTROYED_ON_USE: &str = "Item_IsDestroyOnUse";
