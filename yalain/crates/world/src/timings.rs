//! How long what a body plays takes, as the install's animations say: when a blow lands, when an
//! attacker's next attack comes, how long a special plays, how long a chest takes to open or close.
//!
//! The attacker's look in its combat mode plays the story's behavior: a cycle at time scale 1 and
//! its story hooks ([`auberean_animation::timeline`]). A story's hook numbered n runs at the n-th
//! hook over the time scale, or at the cycle's end when the cycle has fewer. What the 2017 servers
//! took besides is the server's own: a blow was told landed [`LANDS_AFTER`] after its hook, a
//! player's next story came [`PLAYER_PAUSE`] after its cycle ended, a creature's or a pet's
//! [`THING_PAUSE`] (2017's 1,431 sessions: 16,254 landings of 16,406 timed so, the median off by
//! 0.03 s; 147,013 player swings and 148,295 creature strikes to the next story, a spread of 0.08
//! s about the pause).

use std::collections::HashMap;
use std::sync::Mutex;

use auberean_animation::timeline::{self, TimelineRecords};
pub use auberean_animation::timeline::{Plays, Timeline};
use auberean_assets::Context;
use auberean_dat::DatFile;
use auberean_primitives::DataId;
use auberean_protocol::types::{PartGroupDesc, VisualDesc};
use yalain_net::ConnId;
use yalain_store::Store;

use crate::Server;

/// After a hook, when its blow is told landed, in seconds (2017: a median 0.041 s at every time
/// scale from 0.4 to 1.6, so it is the server's, not the animation's).
pub const LANDS_AFTER: f64 = 0.041;
/// After a player's cycle ends, when its next story comes, in seconds (2017: 0.146 to 0.154 by
/// behavior).
pub const PLAYER_PAUSE: f64 = 0.15;
/// After a creature's or a pet's cycle ends, when its next attack comes, in seconds (2017: 0.25).
pub const THING_PAUSE: f64 = 0.25;

/// The element every fight is on: the ground.
pub const EARTH: u32 = 0x4000_0001;

/// What a body plays when its look, or the install, says nothing of a behavior: a Tumerok's swing
/// of two handblades, the install's clip `0x05000B60` with the one beside it.
#[must_use]
pub fn unknown() -> Timeline {
    Timeline {
        period: 1.875,
        hooks: vec![0.748_6, 1.193_5],
    }
}

/// When the story's hook numbered `n` runs after the story, at time scale `ts`, and its blow is
/// told landed.
#[must_use]
pub fn lands(t: &Timeline, n: u32, ts: f32) -> f64 {
    f64::from(t.hook(n)) / f64::from(ts) + LANDS_AFTER
}

/// When a cycle played at time scale `ts` ends.
#[must_use]
pub fn ends(t: &Timeline, ts: f32) -> f64 {
    f64::from(t.period) / f64::from(ts)
}

/// The install's timelines, each read once.
#[derive(Debug)]
pub struct Timings {
    records: TimelineRecords,
    looks: Mutex<HashMap<u32, Option<Plays>>>,
    played: Mutex<HashMap<(Plays, u32, u32), Option<Timeline>>>,
    changes: Mutex<HashMap<(Plays, u32, u32), Option<f64>>>,
}

fn locked<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Timings {
    /// Read from the install's portal file.
    #[must_use]
    pub fn new(portal: DatFile, ctx: Context) -> Self {
        Self {
            records: TimelineRecords::new(portal, ctx),
            looks: Mutex::default(),
            played: Mutex::default(),
            changes: Mutex::default(),
        }
    }

    /// What a stored visual description plays with.
    #[must_use]
    pub fn plays_of_record(&self, look: u32) -> Option<Plays> {
        if let Some(p) = locked(&self.looks).get(&look) {
            return p.clone();
        }
        let p = self.records.plays_of_record(DataId(look));
        locked(&self.looks).insert(look, p.clone());
        p
    }

    /// What a sent visual description plays with: its record's or its parent's, its own behavior
    /// table and its own groups' animation maps over them.
    #[must_use]
    pub fn plays_of(&self, look: &VisualDesc) -> Option<Plays> {
        let base = [
            (VisualDesc::DID, look.did),
            (VisualDesc::PARENT, look.parent),
        ]
        .into_iter()
        .filter(|(bit, id)| look.bits & bit != 0 && *id != 0)
        .find_map(|(_, id)| self.plays_of_record(id));
        let mut p = base.unwrap_or_default();
        if look.bits & VisualDesc::BEHAVIOR_TABLE != 0 && look.behavior_table != 0 {
            p.table = DataId(look.behavior_table);
        }
        if look.bits & VisualDesc::PART_GROUPS != 0 {
            for (_, g) in &look.part_groups.entries {
                let map = DataId(g.anim_map);
                if g.bits & PartGroupDesc::ANIM_MAP != 0 && !map.is_none() && !p.maps.contains(&map)
                {
                    p.maps.push(map);
                }
            }
        }
        (!p.table.is_none()).then_some(p)
    }

    /// What a body plays for a behavior in a combat mode, on the ground.
    #[must_use]
    pub fn timeline(&self, plays: &Plays, mode: u32, behavior: u32) -> Option<Timeline> {
        let key = (plays.clone(), mode, behavior);
        if let Some(t) = locked(&self.played).get(&key) {
            return t.clone();
        }
        let t = timeline::timeline(&self.records, plays, EARTH, mode, behavior);
        locked(&self.played).insert(key, t.clone());
        t
    }

    /// How long a body takes to go from one mode to another, at time scale 1.
    #[must_use]
    pub fn mode_change(&self, plays: &Plays, from: u32, to: u32) -> Option<f64> {
        let key = (plays.clone(), from, to);
        if let Some(t) = locked(&self.changes).get(&key) {
            return *t;
        }
        let t = timeline::mode_change(&self.records, plays, EARTH, from, to).map(f64::from);
        locked(&self.changes).insert(key, t);
        t
    }
}

/// What a player plays for a behavior, in its combat mode now.
#[must_use]
pub fn of_player<S: Store>(s: &Server<S>, world: ConnId, behavior: u32) -> Timeline {
    let Some(p) = s.players.get(&world) else {
        return unknown();
    };
    let mode = crate::skill_use::player_mode(s, p.character);
    s.timings
        .as_ref()
        .and_then(|t| t.timeline(&t.plays_of(&p.look)?, mode, behavior))
        .unwrap_or_else(unknown)
}

/// The look a thing is drawn with: its own, else its template's.
fn thing_plays<S: Store>(s: &Server<S>, t: &Timings, id: u64) -> Option<Plays> {
    let thing = s.things.get(id)?;
    match &thing.drawn_as {
        Some(look) => t.plays_of(look),
        None => t.plays_of_record(thing.description.look),
    }
}

/// What a creature or a pet plays for a behavior, in the stance it fights in.
#[must_use]
pub fn of_thing<S: Store>(s: &Server<S>, id: u64, behavior: u32) -> Timeline {
    let mode = crate::arms::stance(s, id);
    s.timings
        .as_ref()
        .and_then(|t| t.timeline(&thing_plays(s, t, id)?, mode, behavior))
        .unwrap_or_else(unknown)
}

/// How long a thing takes to go from one mode to another, at time scale 1; `None` when the install
/// does not say.
#[must_use]
pub fn thing_mode_change<S: Store>(s: &Server<S>, id: u64, from: u32, to: u32) -> Option<f64> {
    let t = s.timings.as_ref()?;
    t.mode_change(&thing_plays(s, t, id)?, from, to)
}
