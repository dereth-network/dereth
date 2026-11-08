//! What the animation records answer: a behavior table's combinations, ramp times, transitions
//! and the path between two modes; the character template's behaviors and sliders.
//!
//! A body is always in an element (on the ground, in the air...) and a mode (peaceful, in combat,
//! swimming...). Its behavior table says, for an element, a mode and a behavior, which animation
//! key the behavior plays and on which layer (a combination); how long a behavior ramps in and
//! what effect starts it; and how a body changes mode: which modes lead to which, how long each
//! step blends, and the behavior and effects of each step. The character template, one for the
//! whole game, says which behaviors are actions (they play once) or holds (they loop until let
//! go), and which sliders the network controls.

use auberean_assets::animation::{
    BehaviorKey, BehaviorStart, BehaviorTable, CharacterTemplate, ModeChange, ModeChangeEffects,
    ModeKey,
};

/// How long a behavior ramps in when its table does not say.
pub const DEFAULT_RAMP_TIME: f32 = 0.25;

/// The layer of a combination the table does not have.
pub const NO_LAYER: u32 = u32::MAX;

/// A key's bucket word: the product of its fields, in 32-bit arithmetic (the client takes it
/// modulo the bucket count).
pub trait KeyHash {
    fn key_hash(&self) -> u32;
}

impl KeyHash for BehaviorKey {
    // D2-15095
    fn key_hash(&self) -> u32 {
        self.behavior
            .wrapping_mul(self.mode)
            .wrapping_mul(self.element)
    }
}

impl KeyHash for ModeChange {
    // D2-15097
    fn key_hash(&self) -> u32 {
        self.to
            .mode
            .wrapping_mul(self.to.element)
            .wrapping_mul(self.from.mode)
            .wrapping_mul(self.from.element)
    }
}

impl KeyHash for ModeKey {
    // D2-15099
    fn key_hash(&self) -> u32 {
        self.mode.wrapping_mul(self.element)
    }
}

/// What a combination plays: an animation key on a layer. A combination the table lacks has no
/// key and layer [`NO_LAYER`].
// D2-15103, D2-15105
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Combo {
    pub anim_key: u32,
    pub layer: u32,
}

impl Combo {
    /// The combination a table lacks.
    // D2-15102
    pub const INVALID: Self = Self {
        anim_key: 0,
        layer: NO_LAYER,
    };

    // D2-15101
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.layer != NO_LAYER
    }
}

/// A behavior table's answers.
pub trait TableQueries {
    /// The behaviors a body starts with.
    fn init_behaviors(&self) -> &[u32];
    /// How many layers the table blends.
    fn layer_count(&self) -> u32;
    /// How long a change of mode blends; 0 when the table does not say.
    fn mode_change_ramp_time(&self, change: &ModeChange) -> f32;
    /// What a behavior plays in an element and a mode.
    fn combo(&self, key: &BehaviorKey) -> Combo;
    /// How long a behavior ramps in: [`DEFAULT_RAMP_TIME`] when the table does not say.
    fn behavior_ramp_time(&self, behavior: u32) -> f32;
    /// How a behavior starts: its ramp time and its effect, [`DEFAULT_RAMP_TIME`] and none when
    /// the table does not say.
    fn behavior_start(&self, behavior: u32) -> BehaviorStart;
    /// The behavior and effects of a step from one mode to another.
    fn transition(&self, change: &ModeChange) -> Option<&ModeChangeEffects>;
    /// The first step on the way from `change.from` to `change.to`, or `None` when there is no
    /// way. See [`next_step`].
    fn next_step(&self, change: &ModeChange) -> Option<ModeKey>;
}

impl TableQueries for BehaviorTable {
    // D2-14970
    fn init_behaviors(&self) -> &[u32] {
        &self.initial_behaviors
    }

    // D2-14973
    fn layer_count(&self) -> u32 {
        u32::try_from(self.layers.len()).unwrap_or(u32::MAX)
    }

    // D2-14990
    fn mode_change_ramp_time(&self, change: &ModeChange) -> f32 {
        self.ramp_times
            .iter()
            .find(|(k, _)| k == change)
            .map_or(0.0, |(_, t)| *t)
    }

    // D2-14991
    fn combo(&self, key: &BehaviorKey) -> Combo {
        self.combos.iter().find(|(k, _)| k == key).map_or(
            Combo::INVALID,
            |(_, (anim_key, layer))| Combo {
                anim_key: *anim_key,
                layer: *layer,
            },
        )
    }

    // D2-14992
    fn behavior_ramp_time(&self, behavior: u32) -> f32 {
        self.behaviors
            .iter()
            .find(|(b, _)| *b == behavior)
            .map_or(DEFAULT_RAMP_TIME, |(_, s)| s.ramp_time)
    }

    // D2-14993
    fn behavior_start(&self, behavior: u32) -> BehaviorStart {
        self.behaviors.iter().find(|(b, _)| *b == behavior).map_or(
            BehaviorStart {
                ramp_time: DEFAULT_RAMP_TIME,
                effect: 0,
            },
            |(_, s)| *s,
        )
    }

    // D2-14994
    fn transition(&self, change: &ModeChange) -> Option<&ModeChangeEffects> {
        self.transitions
            .iter()
            .find(|(k, _)| k == change)
            .map(|(_, v)| v)
    }

    fn next_step(&self, change: &ModeChange) -> Option<ModeKey> {
        next_step(self, change)
    }
}

/// The first step on the way from one mode to another, along the table's mode graph.
///
/// The search runs depth first from the source: a path is taken from the back of a list of paths,
/// each mode it can lead to that it has not been through is appended to it and the new path put at
/// the back. A mode is marked visited when it joins a path, and the marks start as the table's
/// own mode list holds them (a mode listed with a non-zero value is never entered). Within one
/// element only modes of that element are entered; between two elements, only modes of either.
/// The first path to reach the destination gives its second mode; a destination missing from the
/// mode list has no way. A mode the list lacks is never entered (the client would stop there).
// D2-15054
#[must_use]
pub fn next_step(table: &BehaviorTable, change: &ModeChange) -> Option<ModeKey> {
    let (src, dst) = (change.from, change.to);
    if !table.modes.iter().any(|(k, _)| *k == dst) {
        return None;
    }
    let mut visited: Vec<(ModeKey, i32)> = table.modes.clone();
    let mut paths: Vec<Vec<ModeKey>> = vec![vec![src]];
    let in_elements = |m: &ModeKey| {
        if src.element == dst.element {
            m.element == src.element
        } else {
            m.element == src.element || m.element == dst.element
        }
    };
    while let Some(path) = paths.pop() {
        let last = *path.last().expect("a path is never empty");
        let Some((_, next)) = table.next_modes.iter().find(|(k, _)| *k == last) else {
            continue;
        };
        for n in next {
            if path.contains(n) || !in_elements(n) {
                continue;
            }
            let Some(mark) = visited.iter_mut().find(|(k, _)| k == n) else {
                continue;
            };
            if mark.1 != 0 {
                continue;
            }
            mark.1 = 1;
            let mut longer = path.clone();
            longer.push(*n);
            if *n == dst {
                if !longer.iter().all(in_elements) {
                    return None;
                }
                return longer.get(1).copied();
            }
            paths.push(longer);
        }
    }
    None
}

/// A behavior's kind, from the character template's flags.
pub mod behavior_flags {
    /// It plays once and ends.
    pub const ACTION: u64 = 1;
    /// It loops until let go.
    pub const HOLD: u64 = 2;
    /// It moves the first-person camera with it.
    pub const DYNAMIC_FP_CAMERA: u64 = 4;
}

/// A slider the network sets.
pub const NETWORK_CONTROLLED: u32 = 1;

/// The character template's answers.
pub trait TemplateQueries {
    /// A behavior's flags, when the template has the behavior.
    fn behavior(&self, behavior: u32) -> Option<u64>;
    /// A slider's controller, when the template has the slider.
    fn slider(&self, slider: u32) -> Option<u32>;
    /// The element a body starts in.
    fn default_element(&self) -> u32;
    /// The mode a body starts in.
    fn default_mode(&self) -> u32;
}

impl TemplateQueries for CharacterTemplate {
    // D2-15071
    fn behavior(&self, behavior: u32) -> Option<u64> {
        self.behaviors
            .iter()
            .find(|(b, _)| *b == behavior)
            .map(|(_, f)| *f)
    }

    // D2-15072
    fn slider(&self, slider: u32) -> Option<u32> {
        self.sliders
            .iter()
            .find(|(s, _)| *s == slider)
            .map(|(_, c)| *c)
    }

    // D2-15059
    fn default_element(&self) -> u32 {
        self.default_element
    }

    // D2-15060
    fn default_mode(&self) -> u32 {
        self.default_mode
    }
}

/// The records a character template loads with it: four enumeration values, each looked up in
/// the first mapping (the game's sounds, effects and sliders tables).
// D2-15061
pub const TEMPLATE_DEPENDENCIES: [(u32, u32); 4] = [
    (0x4000_0018, 0x4000_0001),
    (0x4000_001A, 0x4000_0001),
    (0x4000_0016, 0x4000_0001),
    (0x4000_0017, 0x4000_0001),
];

/// The enumeration the character template is found by: its value and mapping.
pub const TEMPLATE_ENUM: (u32, u32) = (0x4000_0001, 0x4000_0008);

#[cfg(test)]
mod tests {
    //! Behaviour: none (the search's order is checked on a made-up table)
    use super::*;

    fn key(element: u32, mode: u32) -> ModeKey {
        ModeKey { element, mode }
    }

    #[test]
    fn the_mode_path_takes_the_first_step_toward_the_destination() {
        let (a, b, c, d) = (key(1, 1), key(1, 2), key(1, 3), key(2, 4));
        let table = BehaviorTable {
            modes: vec![(a, 0), (b, 0), (c, 0), (d, 0)],
            next_modes: vec![(a, vec![b]), (b, vec![c, d]), (c, vec![])],
            ..BehaviorTable::default()
        };
        let step = |from, to| table.next_step(&ModeChange { from, to });
        assert_eq!(step(a, c), Some(b), "through b");
        assert_eq!(step(b, c), Some(c), "one step");
        assert_eq!(step(a, d), Some(b), "into another element");
        assert_eq!(step(c, a), None, "no way back");
        let closed = BehaviorTable {
            modes: vec![(a, 0), (b, 1), (c, 0)],
            ..table.clone()
        };
        assert_eq!(
            closed.next_step(&ModeChange { from: a, to: c }),
            None,
            "a mode listed as visited is never entered"
        );
    }
}
