//! How long a behavior plays on a body and when its game hooks come, read from the records alone:
//! what a server needs to time a blow as the client plays it.
//!
//! A body plays a behavior in an element and a mode as its behavior table combines them: an
//! animation key, which each part group's animation map turns into an animation. The behavior's
//! cycle is that animation's period at time scale 1; its game hooks of the story's kind come at
//! moments of the cycle, and a story's hook numbered n waits for the n-th of them (one numbered
//! past the last waits for the cycle's end). A change of mode plays each step's behavior, or
//! blends for the step's ramp time when the step plays none.
//!
//! Graphs are read against the starting sliders: a blend along an aim or a speed slider is taken
//! where its sliders start.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use auberean_assets::animation::{
    AnimMap, Animation, BehaviorKey, BehaviorTable, ModeChange, ModeKey,
};
use auberean_assets::visual::{Appearance, VisualDesc};
use auberean_assets::{Context, Decode};
use auberean_dat::DatFile;
use auberean_primitives::DataId;
use auberean_world::look::{Look, LookRecords};

use crate::graph::{self, Animations, GraphState};
use crate::queries::{TableQueries, DEFAULT_RAMP_TIME};
use crate::sliders::Sliders;
use crate::triggers::Gathered;

/// The kind of game hook a story counts.
pub const STORY_HOOK: u32 = 0x4000_0002;

/// How many steps a change of mode may take before it is taken for a broken graph.
const MAX_STEPS: usize = 16;

/// The records a timeline is read from.
pub trait PlayRecords: Animations {
    fn anim_map(&self, id: DataId) -> Option<Arc<AnimMap>>;
    fn behavior_table(&self, id: DataId) -> Option<Arc<BehaviorTable>>;
}

impl<T: crate::body::BodyRecords> PlayRecords for T {
    fn anim_map(&self, id: DataId) -> Option<Arc<AnimMap>> {
        crate::body::BodyRecords::anim_map(self, id)
    }
    fn behavior_table(&self, id: DataId) -> Option<Arc<BehaviorTable>> {
        crate::body::BodyRecords::behavior_table(self, id)
    }
}

/// What a body plays with: its behavior table and its part groups' animation maps.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct Plays {
    pub table: DataId,
    pub maps: Vec<DataId>,
}

/// A behavior's cycle on a body, at time scale 1: its length and its story hooks' moments, in
/// seconds, in order.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Timeline {
    pub period: f32,
    pub hooks: Vec<f32>,
}

impl Timeline {
    /// When the story's hook numbered `n` runs, at time scale 1: at once for 0, the n-th hook's
    /// moment, or the cycle's end for a number past the last.
    #[must_use]
    pub fn hook(&self, n: u32) -> f32 {
        if n == 0 {
            return 0.0;
        }
        usize::try_from(n - 1)
            .ok()
            .and_then(|i| self.hooks.get(i))
            .copied()
            .unwrap_or(self.period)
    }
}

fn state(sliders: &Sliders, element: u32, mode: u32) -> GraphState<'_> {
    GraphState {
        sliders,
        mode,
        element,
        terrain: 0,
    }
}

/// What `behavior` plays on a body in `element` and `mode`: the longest of its groups' cycles,
/// and every group's story hooks; none when the table has no such combination or no group's map
/// holds its key.
#[must_use]
pub fn timeline<R: PlayRecords>(
    records: &R,
    plays: &Plays,
    element: u32,
    mode: u32,
    behavior: u32,
) -> Option<Timeline> {
    let table = records.behavior_table(plays.table)?;
    let combo = table.combo(&BehaviorKey {
        element,
        mode,
        behavior,
    });
    if !combo.is_valid() {
        return None;
    }
    key_timeline(records, plays, element, mode, combo.anim_key)
}

fn key_timeline<R: PlayRecords>(
    records: &R,
    plays: &Plays,
    element: u32,
    mode: u32,
    key: u32,
) -> Option<Timeline> {
    let sliders = Sliders::new();
    let st = state(&sliders, element, mode);
    let mut found = false;
    let mut out = Timeline::default();
    for map in &plays.maps {
        let Some(m) = records.anim_map(*map) else {
            continue;
        };
        let Some(id) = m.entries.iter().find(|(k, _)| *k == key).map(|(_, id)| *id) else {
            continue;
        };
        if records.animation(id).is_none() {
            continue;
        }
        found = true;
        let period = graph::period(records, id, &st);
        out.period = out.period.max(period);
        let mut g = Gathered::default();
        graph::triggers(records, id, &st, 1.0, false, &mut g);
        out.hooks.extend(
            g.hooks
                .iter()
                .filter(|h| h.kind == STORY_HOOK)
                .map(|h| h.time * period),
        );
    }
    out.hooks.sort_by(f32::total_cmp);
    found.then_some(out)
}

/// How long a body takes to go from one mode to another, at time scale 1: each step along the
/// table's mode graph plays its behavior's cycle (as the source mode combines it, else the
/// destination's), or blends for the step's ramp time when it plays none. None when there is no
/// way.
#[must_use]
pub fn mode_change<R: PlayRecords>(
    records: &R,
    plays: &Plays,
    element: u32,
    from: u32,
    to: u32,
) -> Option<f32> {
    let table = records.behavior_table(plays.table)?;
    let mut at = ModeKey {
        element,
        mode: from,
    };
    let goal = ModeKey { element, mode: to };
    let mut total = 0.0;
    for _ in 0..MAX_STEPS {
        if at == goal {
            return Some(total);
        }
        let change = ModeChange { from: at, to: goal };
        let next = table.next_step(&change)?;
        let step = ModeChange { from: at, to: next };
        let behavior = table.transition(&step).map_or(0, |t| t.behavior);
        let played = (behavior != 0)
            .then(|| {
                [at.mode, next.mode].into_iter().find_map(|mode| {
                    timeline(records, plays, element, mode, behavior).map(|t| t.period)
                })
            })
            .flatten();
        total += played.unwrap_or_else(|| {
            let ramp = table.mode_change_ramp_time(&step);
            if ramp > 0.0 {
                ramp
            } else {
                DEFAULT_RAMP_TIME
            }
        });
        at = next;
    }
    None
}

type Cache<T> = Mutex<HashMap<u32, Option<Arc<T>>>>;

/// The portal file's records a timeline and a look read, decoded once each.
#[derive(Debug)]
pub struct TimelineRecords {
    portal: DatFile,
    ctx: Context,
    animations: Cache<Animation>,
    maps: Cache<AnimMap>,
    tables: Cache<BehaviorTable>,
    visuals: Cache<VisualDesc>,
    appearances: Cache<Appearance>,
}

impl TimelineRecords {
    #[must_use]
    pub fn new(portal: DatFile, ctx: Context) -> Self {
        Self {
            portal,
            ctx,
            animations: Mutex::default(),
            maps: Mutex::default(),
            tables: Mutex::default(),
            visuals: Mutex::default(),
            appearances: Mutex::default(),
        }
    }

    fn get<T: Decode>(&self, cache: &Cache<T>, id: DataId) -> Option<Arc<T>> {
        if id.is_none() {
            return None;
        }
        let mut c = cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        c.entry(id.0)
            .or_insert_with(|| {
                let bytes = self.portal.read(id.0).ok()?;
                auberean_assets::decode::<T>(id.0, &bytes, &self.ctx)
                    .ok()
                    .map(Arc::new)
            })
            .clone()
    }

    /// What a stored visual description plays with: its total's behavior table and the
    /// animation maps of its part groups.
    #[must_use]
    pub fn plays_of_record(&self, id: DataId) -> Option<Plays> {
        let look = Look::of_record(self, id).ok()?;
        Some(plays_of_look(&look))
    }
}

/// What a total look plays with.
#[must_use]
pub fn plays_of_look(look: &Look) -> Plays {
    Plays {
        table: look.behavior_table,
        maps: look
            .groups
            .entries
            .iter()
            .map(|(_, g)| g.anim_map)
            .filter(|m| !m.is_none())
            .collect(),
    }
}

impl Animations for TimelineRecords {
    fn animation(&self, id: DataId) -> Option<Arc<Animation>> {
        self.get(&self.animations, id)
    }
}

impl LookRecords for TimelineRecords {
    fn visual(&self, id: DataId) -> Option<Arc<VisualDesc>> {
        self.get(&self.visuals, id)
    }
    fn appearance(&self, id: DataId) -> Option<Arc<Appearance>> {
        self.get(&self.appearances, id)
    }
}

impl PlayRecords for TimelineRecords {
    fn anim_map(&self, id: DataId) -> Option<Arc<AnimMap>> {
        self.get(&self.maps, id)
    }
    fn behavior_table(&self, id: DataId) -> Option<Arc<BehaviorTable>> {
        self.get(&self.tables, id)
    }
}
