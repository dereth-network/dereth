//! Portal storms: a landblock that holds too many players is warned, then emptied a little.
//!
//! **The tick.** Every 11 s the world looks at each landblock's players (2017: 68 of 70 warnings
//! within 0.12 s of an 11 s grid). A warning goes to every player in the landblock at that tick, each
//! with its own cell: two players in the block at once got the same warnings at the same ticks (15
//! of 15); players outside it, none.
//!
//! **The storm's course.** A landblock holding at least [`Storms::threshold`] players starts to
//! brew: "A portal storm is brewing." (intensity 0.4), once. While it brews, a tick that finds it
//! below the threshold calls it off: "The portal storm has subsided." (0.0; 5 of 12 brewings, 1-6
//! ticks in). Six ticks after it began it is imminent: "A portal storm is imminent!" (0.6) at five
//! ticks in a row (7 of 7), never called off (8 of 8). The next tick is the storm: some of the
//! players in the block are told "The portal storm has teleported you!" and sent through portal
//! space to a place in a landblock two away (9 of 9 storms). The tick after, the players still in
//! the block are told it has subsided (6 of 6), and it may brew again.
//!
//! The client turns the intensity into the warning's words; the server sends only the number.
//!
//! **Not measured**: the threshold (2017 brewed at 38 or more players a recorder
//! could see, never 39 or more without; about 40 is read), how many are sent away beyond the least
//! (5; 2017's storms sent 5 to 28), who (by lot), and where in the landblock two away (cell 1,
//! 4 m in, as 3 of 3 seen).

use std::collections::BTreeMap;

use auberean_script::args::Arg;
use yalain_net::{ConnId, ServerNet};
use yalain_store::{Store, StoredPosition};

use crate::{progress, session, Clock, Server};

/// The warning call: the player's cell and the storm's intensity.
pub const WARNING: u32 = 0x8212_0029;

/// The intensities the warnings carry.
pub mod intensity {
    pub const BREWING: f32 = 0.4;
    pub const IMMINENT: f32 = 0.6;
    pub const SUBSIDED: f32 = 0.0;
}

/// "The portal storm has teleported you!", and the kind of text it is shown as.
pub const TELEPORTED: (u32, u32) = (0x5EA1_7F2D, 0x2500_0414);
pub const SHOWN_AS: u32 = 1;

/// Seconds between the world's looks at its landblocks.
pub const TICK: f64 = 11.0;
/// The ticks after a storm begins brewing at which it is imminent, its first and last, the storm's
/// own tick, and the tick it is over.
pub const IMMINENT_FROM: u32 = 6;
pub const IMMINENT_UNTIL: u32 = 10;
pub const STORM_AT: u32 = 11;
pub const OVER_AT: u32 = 12;

/// The world's storms: the settings, the next tick, and each landblock's storm by the ticks since it
/// began brewing.
#[derive(Debug, Clone, PartialEq)]
pub struct Storms {
    /// The players a landblock must hold for a storm to brew.
    pub threshold: usize,
    /// The least number of players a storm sends away.
    pub least: usize,
    next: Option<f64>,
    brewing: BTreeMap<u32, u32>,
}

impl Default for Storms {
    fn default() -> Self {
        Self {
            threshold: 40,
            least: 5,
            next: None,
            brewing: BTreeMap::new(),
        }
    }
}

/// The warning of an intensity to a player in a cell.
#[must_use]
pub fn warning(cell: u32, intensity: f32) -> auberean_protocol::ServerMessage {
    session::call(WARNING, &[Arg::Float(intensity.to_bits()), Arg::Int(cell)])
        .expect("plain values pack")
}

/// The players in the world each landblock holds (those not on their way through portal space).
fn crowds<S: Store>(s: &Server<S>) -> BTreeMap<u32, Vec<ConnId>> {
    let mut by: BTreeMap<u32, Vec<ConnId>> = BTreeMap::new();
    for (conn, p) in &s.players {
        if p.trip.is_none() {
            by.entry(p.position.cell >> 16).or_default().push(*conn);
        }
    }
    by
}

/// Warn the players of a landblock.
fn warn<S: Store>(s: &Server<S>, net: &mut ServerNet, players: &[ConnId], level: f32) {
    for conn in players {
        if let Some(p) = s.players.get(conn) {
            progress::private(net, *conn, &warning(p.position.cell, level));
        }
    }
}

/// Where a storm sends a player: cell 1 of a landblock two away, 4 m into it.
fn landing<S: Store>(s: &mut Server<S>, landblock: u32) -> StoredPosition {
    let (x, y) = (landblock >> 8 & 0xFF, landblock & 0xFF);
    let (dx, dy) = loop {
        let dx = i64::try_from(s.rng.below(5)).unwrap_or(0) - 2;
        let dy = i64::try_from(s.rng.below(5)).unwrap_or(0) - 2;
        if dx.abs() == 2 || dy.abs() == 2 {
            break (dx, dy);
        }
    };
    let step = |v: u32, d: i64| u32::try_from((i64::from(v) + d).clamp(0, 0xFF)).unwrap_or(v);
    let cell = (step(x, dx) << 24) | (step(y, dy) << 16) | 0x0001;
    let mut origin = [4.0, 4.0, 0.0];
    if let Some(z) = s.ground.as_deref().and_then(|g| g.height(cell, origin)) {
        origin[2] = z;
    }
    StoredPosition {
        cell,
        origin,
        rotation: [1.0, 0.0, 0.0, 0.0],
    }
}

/// Look at the landblocks when a tick is due.
pub fn look<S: Store>(s: &mut Server<S>, net: &mut ServerNet, clock: Clock) {
    let now = clock.local.0;
    match s.storms.next {
        None => {
            s.storms.next = Some(now + TICK);
            return;
        }
        Some(t) if now < t => return,
        Some(t) => s.storms.next = Some(t + TICK),
    }
    let crowds = crowds(s);
    let threshold = s.storms.threshold;
    for (block, players) in &crowds {
        if players.len() >= threshold && !s.storms.brewing.contains_key(block) {
            s.storms.brewing.insert(*block, 0);
            warn(s, net, players, intensity::BREWING);
        }
    }
    let brewing: Vec<(u32, u32)> = s.storms.brewing.iter().map(|(b, t)| (*b, *t)).collect();
    for (block, ticks) in brewing {
        if ticks == 0 {
            // Begun at this tick.
            s.storms.brewing.insert(block, 1);
            continue;
        }
        let here = crowds.get(&block).cloned().unwrap_or_default();
        if ticks < IMMINENT_FROM && here.len() < threshold {
            s.storms.brewing.remove(&block);
            warn(s, net, &here, intensity::SUBSIDED);
            continue;
        }
        if (IMMINENT_FROM..=IMMINENT_UNTIL).contains(&ticks) {
            warn(s, net, &here, intensity::IMMINENT);
        } else if ticks == STORM_AT {
            storm(s, net, block, &here, clock);
        } else if ticks >= OVER_AT {
            s.storms.brewing.remove(&block);
            warn(s, net, &here, intensity::SUBSIDED);
            continue;
        }
        s.storms.brewing.insert(block, ticks + 1);
    }
}

/// The storm: players of the landblock drawn by lot, as many as it holds beyond the threshold and
/// at least the least, sent away.
fn storm<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    block: u32,
    here: &[ConnId],
    clock: Clock,
) {
    let mut left: Vec<ConnId> = here.to_vec();
    let n = here
        .len()
        .saturating_sub(s.storms.threshold)
        .max(s.storms.least)
        .min(here.len());
    for _ in 0..n {
        let i = s.rng.below(left.len());
        let conn = left.swap_remove(i);
        progress::private(
            net,
            conn,
            &crate::chat::show_text(SHOWN_AS, progress::text(TELEPORTED)),
        );
        let to = landing(s, block);
        crate::places::sent_away(s, net, conn, to, clock);
    }
}
