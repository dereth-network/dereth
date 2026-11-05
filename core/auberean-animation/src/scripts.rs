//! Effect scripts played on their own: a script an object is asked to play runs from its start to
//! its end over its period, setting off its effects and impulses as it passes them.
//!
//! Each frame every playing script moves on by the time over its period and gathers what is due;
//! once it reaches its end its weight drops below zero, so the trigger update stops every effect
//! it started, and a frame later (once past its end) it is dropped. A script whose record is not
//! in the data is dropped, and is refused when asked for.

use std::sync::Arc;

use auberean_assets::fx::FxScript;
use auberean_primitives::DataId;
use auberean_world::chargen::Random;

use crate::body::BodyRecords;
use crate::triggers::{script_triggers, FrameTriggers, Gathered};

/// An effect script playing: where it is (0 at its start, 1 at its end), its weight (1 while it
/// plays, below zero once it has reached its end) and what it has set off.
// D2-15130, D2-15131
#[derive(Clone, Debug, PartialEq)]
pub struct ScriptTrack {
    pub script: u32,
    pub time: f32,
    pub weight: f32,
    pub looped: bool,
    pub gathered: Gathered,
    cached: Option<Arc<FxScript>>,
}

impl Default for ScriptTrack {
    // D2-15133
    fn default() -> Self {
        Self::new(0)
    }
}

impl ScriptTrack {
    /// Script `script`, at its start, at full weight.
    // D2-15132
    #[must_use]
    pub fn new(script: u32) -> Self {
        Self {
            script,
            time: 0.0,
            weight: 1.0,
            looped: false,
            gathered: Gathered::default(),
            cached: None,
        }
    }

    /// Find the script's record, once; `false` when it is not in the data.
    // D2-15127
    pub fn fill_cache(&mut self, records: &dyn BodyRecords) -> bool {
        if self.cached.is_some() {
            return true;
        }
        self.cached = records.fx_script(DataId(self.script));
        self.cached.is_some()
    }

    /// The script's record, once found.
    #[must_use]
    pub fn cached(&self) -> Option<&Arc<FxScript>> {
        self.cached.as_ref()
    }

    /// Take another track's script, time, weight, effects and impulses (its hooks, its loop and
    /// the effects it started stay this track's own).
    // D2-15129
    pub fn assign(&mut self, other: &Self) {
        self.time = other.time;
        self.weight = other.weight;
        self.gathered.effects.clone_from(&other.gathered.effects);
        self.gathered.impulses.clone_from(&other.gathered.impulses);
        self.script = other.script;
        self.cached.clone_from(&other.cached);
    }

    /// Whether it has reached its end: its weight is zero or less.
    // D2-14951
    #[must_use]
    pub fn is_ramping_down(&self) -> bool {
        self.weight <= 0.0
    }
}

/// The effect scripts an object plays on their own, in the order asked.
// D2-15142, D2-15143
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScriptTracker {
    pub tracks: Vec<ScriptTrack>,
}

impl ScriptTracker {
    /// Play effect script `script` from its start; `false` (and nothing played) when its record is
    /// not in the data.
    // D2-15147
    pub fn do_script(&mut self, records: &dyn BodyRecords, script: u32) -> bool {
        let mut t = ScriptTrack::new(script);
        if !t.fill_cache(records) {
            return false;
        }
        self.tracks.push(t);
        true
    }

    /// Advance every script `dt` seconds and hand what each set off to `triggers`; see the module's
    /// description.
    // D2-15137
    pub fn update(
        &mut self,
        dt: f32,
        records: &dyn BodyRecords,
        triggers: &mut FrameTriggers,
        dice: &mut Random,
    ) {
        let mut i = 0;
        while i < self.tracks.len() {
            let t = &mut self.tracks[i];
            if t.cached.is_none() && !t.fill_cache(records) {
                self.tracks.remove(i);
                continue;
            }
            let script = t.cached.clone().expect("filled");
            #[allow(clippy::cast_possible_truncation)]
            let time = (f64::from(dt) / f64::from(script.period) + f64::from(t.time)) as f32;
            t.time = time;
            script_triggers(&script, time, &mut t.gathered);
            if t.time >= 1.0 {
                t.weight = -1.0;
            }
            let ramping_down = t.is_ramping_down();
            triggers.add_track_triggers(&mut t.gathered, ramping_down, &mut t.looped, dice);
            if t.time > 1.0 {
                self.tracks.remove(i);
            } else {
                i += 1;
            }
        }
    }
}
