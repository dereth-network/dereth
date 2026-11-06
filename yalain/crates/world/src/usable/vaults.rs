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

use std::collections::HashMap;

use auberean_protocol::physics::Fx;
use auberean_protocol::types::BehaviorParams;
use auberean_protocol::ServerMessage;
use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;

use crate::places::{self, Use};
use crate::{presence, progress, Clock, Server};

/// The key id each glyph carries, by its template.
pub const GLYPH_KEYS: [(u32, u32); 22] = [
    (0x4700_10AC, 0x4000_0004),
    (0x4700_10AD, 0x4000_002B),
    (0x4700_10AE, 0x4000_002D),
    (0x4700_10AF, 0x4000_0020),
    (0x4700_10B0, 0x4000_001F),
    (0x4700_10B1, 0x4000_0021),
    (0x4700_10B2, 0x4000_001C),
    (0x4700_10B3, 0x4000_0023),
    (0x4700_10B4, 0x4000_0029),
    (0x4700_10B5, 0x4000_002E),
    (0x4700_10B6, 0x4000_002C),
    (0x4700_10B7, 0x4000_0022),
    (0x4700_10B8, 0x4000_0028),
    (0x4700_10B9, 0x4000_001B),
    (0x4700_10BA, 0x4000_0025),
    (0x4700_10BB, 0x4000_001A),
    (0x4700_10BC, 0x4000_001E),
    (0x4700_10BD, 0x4000_0027),
    (0x4700_10BE, 0x4000_0026),
    (0x4700_10BF, 0x4000_001D),
    (0x4700_10C0, 0x4000_002A),
    (0x4700_10C1, 0x4000_0024),
];

/// How long after the glyph's use begins the vault opens, in seconds (2017: median 2.58 s, 45
/// trips).
pub const OPENS_AFTER: f64 = 2.58;

/// How long a vault waits between trips, in seconds.
pub const BETWEEN_TRIPS: f64 = 5.0;

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

/// The key id a glyph's template carries.
#[must_use]
pub fn glyph_key(template: u32) -> Option<u32> {
    GLYPH_KEYS
        .iter()
        .find(|(t, _)| *t == template)
        .map(|(_, k)| *k)
}

/// A vault of the world, by its id: a locked object that takes a player somewhere. Its key id,
/// when one is known.
#[must_use]
pub fn vault<S: Store>(s: &Server<S>, id: u64) -> Option<Option<u32>> {
    let i = presence::content_by_id(&s.content.objects, id)?;
    s.content.objects[i].destination?;
    s.locks.of(id).map(|l| l.key)
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

/// A vault used bare, or walked into: it needs its glyph, and flashes.
pub fn needs_glyph<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: &Use,
    vault: u64,
) {
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
    let busy = s
        .vault_trips
        .get(&vault_id)
        .and_then(|t| t.under_way)
        .is_some_and(|w| w != world);
    if busy {
        refuse(net, world, &u, refusal::IN_USE, refusal::IN_USE_FLAGS);
        return;
    }
    let glyph = s.things.get(u.item).and_then(|t| glyph_key(t.template));
    if key.is_none() || glyph != key {
        needs_glyph(s, net, world, &u, vault_id);
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
    let trips = s.vault_trips.entry(u.target).or_default();
    if trips.last.is_some_and(|t| now - t < BETWEEN_TRIPS) {
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

/// The trip through a vault has arrived: the glyph is used up, and the use is told it fit.
/// Returns the use to answer.
pub fn arrived<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, u: &Use) -> Use {
    if vault(s, u.target).is_none() {
        return u.clone();
    }
    if let Some(t) = s.vault_trips.get_mut(&u.target) {
        t.under_way = None;
    }
    let name = presence::content_by_id(&s.content.objects, u.target)
        .and_then(|i| s.content.objects[i].description.name);
    let told = crate::usable::doors::fits_answer(s, u, u.item, name);
    crate::craft::use_up(s, net, world, u.item, 1);
    told
}
