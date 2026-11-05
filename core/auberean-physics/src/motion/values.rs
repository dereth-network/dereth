//! A body's motion values (its speeds, accelerations, slowdowns, jump and hover height in one
//! element and mode on one kind of ground), and how they are found in its motion description.

use auberean_assets::animation::{ModeKey, MotionDesc, MotionParams};

/// Motion values before any are read: no speeds or accelerations, unit slowdowns (a turn slows at
/// 180 degrees a second), a terminal speed of 50 m/s, under gravity.
// D2-15463
#[must_use]
pub const fn default_values() -> MotionParams {
    MotionParams {
        lift: 0.0,
        drag: 0.0,
        terminal_speed: 50.0,
        gravity: 1,
        sink_offset: 0.0,
        hover_height: 0.0,
        jump_impulse: 0.0,
        max_forward: 0.0,
        max_backward: 0.0,
        max_strafe: 0.0,
        still_turn_rate: 0.0,
        running_turn_rate: 0.0,
        max_vertical: 0.0,
        forward_slowdown: 1.0,
        backward_slowdown: 1.0,
        strafe_slowdown: 1.0,
        turn_slowdown: 180.0,
        forward_acceleration: 0.0,
        backward_acceleration: 0.0,
        strafe_acceleration: 0.0,
        turn_acceleration: 0.0,
        vertical_acceleration: 0.0,
    }
}

/// A body's motion values: the record's fields, read as they are (the maximum forward, backward,
/// sideways and vertical speeds, the turn rates standing and running, the accelerations and
/// slowdowns, drag, terminal speed, sink offset, hover height and whether gravity holds).
// D2-15465, D2-15466, D2-15468, D2-15469, D2-15470, D2-15471, D2-15472, D2-15473, D2-15474
// D2-15467
// D2-15475, D2-15476, D2-15477, D2-15478, D2-15479, D2-15502, D2-15503, D2-15504, D2-15505
// D2-04271, D2-15438
pub type MotionValues = MotionParams;

/// A motion description before its record is read: the sea floor slows a body to a tenth, water
/// to three quarters.
// D2-15452
#[must_use]
pub fn empty_description() -> MotionDesc {
    MotionDesc {
        set_of: Vec::new(),
        sets: Vec::new(),
        sea_floor_multiplier: 0.1,
        water_multiplier: 0.75,
    }
}

/// The values a description gives an element in a mode: the set the element and mode use, then
/// within it the values for the global modifier if the set has them, else those of the kind of
/// ground (and, when a ground modifier applies, of that modifier on that ground, or none if the
/// ground has no values for it), else the set's own.
// D2-15442
#[must_use]
pub fn find_in_description(
    desc: &MotionDesc,
    key: ModeKey,
    global_mod: u32,
    terrain: u32,
    terrain_mod: Option<u32>,
) -> Option<&MotionParams> {
    let index = desc.set_of.iter().find(|(k, _)| *k == key)?.1;
    let set = desc.sets.get(usize::try_from(index).ok()?)?;
    if let Some((_, v)) = set.by_modifier.iter().find(|(m, _)| *m == global_mod) {
        return Some(v);
    }
    let Some((_, ground)) = set.by_terrain.iter().find(|(t, _)| *t == terrain) else {
        return Some(&set.default);
    };
    match terrain_mod {
        None => Some(&ground.default),
        Some(m) => ground
            .by_modifier
            .iter()
            .find(|(k, _)| *k == m)
            .map(|(_, v)| v),
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the lookup's order, held to a made-up description)
    use super::*;
    use auberean_assets::animation::{MotionSet, TerrainMotion};

    fn v(speed: f32) -> MotionParams {
        MotionParams {
            max_forward: speed,
            ..default_values()
        }
    }

    #[test]
    fn the_global_modifier_wins_then_the_ground_then_the_set() {
        let key = ModeKey {
            element: 1,
            mode: 2,
        };
        let desc = MotionDesc {
            set_of: vec![(key, 0)],
            sets: vec![MotionSet {
                by_terrain: vec![(
                    7,
                    TerrainMotion {
                        by_modifier: vec![(9, v(4.0))],
                        default: v(3.0),
                    },
                )],
                by_modifier: vec![(5, v(2.0))],
                default: v(1.0),
            }],
            ..empty_description()
        };
        let f = |g, t, m| find_in_description(&desc, key, g, t, m).map(|v| v.max_forward);
        assert_eq!(f(5, 7, Some(9)), Some(2.0));
        assert_eq!(f(0, 8, None), Some(1.0));
        assert_eq!(f(0, 7, None), Some(3.0));
        assert_eq!(f(0, 7, Some(9)), Some(4.0));
        assert_eq!(f(0, 7, Some(10)), None);
        let other = ModeKey {
            element: 1,
            mode: 3,
        };
        assert!(find_in_description(&desc, other, 0, 7, None).is_none());
    }
}
