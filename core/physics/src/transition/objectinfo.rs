//! `OBJECTINFO` — who is moving and how.
//!
//! Transcribed against the client's own object-info code.

use dereth_primitives::ObjectId;

use crate::arena::PhysHandle;

/// `ObjectInfoState` bits. Names from ACE; every value is confirmed by a client test site.
///
/// `PERFECT_CLIP (0x40)` and `IGNORE_CREATURES (0x400)` are both *read* by the client in its
/// collision branches.
///
/// * `PERFECT_CLIP` **has** a writer:
///   player-transition initialization uses flags `0x5C`, which this crate writes as
///   [`crate::globals::VIEWER_OBJECT_INFO_STATE`] and `dereth-client`'s camera hands to
///   the world's sphere sweep. The camera sweep runs the `PERFECT_CLIP` branches.
/// * `IGNORE_CREATURES` still has none, and stays unwritten so that if it ever shows up set it is
///   because a `PhysicsDesc` carried it and not because we invented it.
///
/// The test `perfect_clip_is_written_numerically_and_ignore_creatures_still_is_not` asserts both,
/// by value rather than by name — see its own note for why.
#[derive(Debug)]
pub struct ObjectInfoState;

impl ObjectInfoState {
    /// The object is in contact and the contact is still meaningful.
    pub const CONTACT: u32 = 0x0001;
    /// The contact plane is walkable.
    pub const ON_WALKABLE: u32 = 0x0002;
    /// Set by the viewer/camera path; `validate_walkable` then takes a completely different
    /// branch and `calc_num_steps` uses a different step-count rule.
    pub const IS_VIEWER: u32 = 0x0004;
    /// `MISSILE_PS`: exact time-of-impact instead of landing, and the path stops at the first
    /// collision normal.
    pub const PATH_CLIPPED: u32 = 0x0008;
    /// The end rotation is set up front and per-step rotation
    /// interpolation is skipped.
    pub const FREE_ROTATE: u32 = 0x0010;
    /// Written by exactly one path: the viewer/camera sweep, as part of
    /// [`crate::globals::VIEWER_OBJECT_INFO_STATE`] (`0x5C`).
    pub const PERFECT_CLIP: u32 = 0x0040;
    /// The weenie's impenetrable test.
    pub const IS_IMPENETRABLE: u32 = 0x0080;
    /// The weenie's player test.
    pub const IS_PLAYER: u32 = 0x0100;
    /// `EDGE_SLIDE_PS`.
    pub const EDGE_SLIDE: u32 = 0x0200;
    /// Read, never written.
    pub const IGNORE_CREATURES: u32 = 0x0400;
    /// The weenie's player-killer test.
    pub const IS_PK: u32 = 0x0800;
    /// The weenie's player-killer-lite test.
    pub const IS_PK_LITE: u32 = 0x1000;
}

/// `OBJECTINFO`.
#[derive(Debug, Clone, Default)]
pub struct ObjectInfo {
    pub object: Option<PhysHandle>,
    pub state: u32,
    pub scale: f32,
    pub step_up_height: f32,
    pub step_down_height: f32,
    pub ethereal: bool,
    /// `!(state & MISSILE_PS)`: missiles never step down.
    pub step_down: bool,
    /// The projectile's target.
    pub target_id: ObjectId,
}

impl ObjectInfo {
    #[inline]
    #[must_use]
    pub const fn has(&self, bit: u32) -> bool {
        self.state & bit != 0
    }

    #[inline]
    pub const fn set(&mut self, bit: u32, on: bool) {
        if on {
            self.state |= bit;
        } else {
            self.state &= !bit;
        }
    }

    /// Forwards to the object, which is the bare
    /// `n.z >= floor_z`.
    #[inline]
    #[must_use]
    pub fn is_valid_walkable(n: dereth_primitives::Vec3) -> bool {
        crate::is_valid_walkable(n)
    }

    /// The object info's walkable-z threshold.
    #[inline]
    #[must_use]
    pub const fn get_walkable_z() -> f32 {
        crate::globals::FLOOR_Z
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Oracle: the ObjectInfoState table in
    // the recovered collision and transition behavior, whose every value the
    // document marks as confirmed by a client test site.

    #[test]
    fn every_documented_bit_has_its_documented_value() {
        assert_eq!(ObjectInfoState::CONTACT, 0x0001);
        assert_eq!(ObjectInfoState::ON_WALKABLE, 0x0002);
        assert_eq!(ObjectInfoState::IS_VIEWER, 0x0004);
        assert_eq!(ObjectInfoState::PATH_CLIPPED, 0x0008);
        assert_eq!(ObjectInfoState::FREE_ROTATE, 0x0010);
        assert_eq!(ObjectInfoState::PERFECT_CLIP, 0x0040);
        assert_eq!(ObjectInfoState::IS_IMPENETRABLE, 0x0080);
        assert_eq!(ObjectInfoState::IS_PLAYER, 0x0100);
        assert_eq!(ObjectInfoState::EDGE_SLIDE, 0x0200);
        assert_eq!(ObjectInfoState::IGNORE_CREATURES, 0x0400);
        assert_eq!(ObjectInfoState::IS_PK, 0x0800);
        assert_eq!(ObjectInfoState::IS_PK_LITE, 0x1000);
        // Bit 5 (0x20) has no name in the client or in ACE; leaving a gap is deliberate.
    }

    /// Perfect clip is written numerically and ignore creatures still is not.
    #[test]
    fn perfect_clip_is_written_numerically_and_ignore_creatures_still_is_not() {
        // Every bit this crate has a name for.
        const NAMED: u32 = ObjectInfoState::CONTACT
            | ObjectInfoState::ON_WALKABLE
            | ObjectInfoState::IS_VIEWER
            | ObjectInfoState::PATH_CLIPPED
            | ObjectInfoState::FREE_ROTATE
            | ObjectInfoState::PERFECT_CLIP
            | ObjectInfoState::IS_IMPENETRABLE
            | ObjectInfoState::IS_PLAYER
            | ObjectInfoState::EDGE_SLIDE
            | ObjectInfoState::IGNORE_CREATURES
            | ObjectInfoState::IS_PK
            | ObjectInfoState::IS_PK_LITE;

        assert_eq!(
            crate::globals::VIEWER_OBJECT_INFO_STATE,
            ObjectInfoState::IS_VIEWER
                | ObjectInfoState::PATH_CLIPPED
                | ObjectInfoState::FREE_ROTATE
                | ObjectInfoState::PERFECT_CLIP,
            "0x5C decomposes into exactly these four; if it stops doing so the camera sweep is \
             taking a different branch than the client's"
        );
        let viewer = ObjectInfo {
            state: crate::globals::VIEWER_OBJECT_INFO_STATE,
            ..ObjectInfo::default()
        };
        assert!(
            viewer.has(ObjectInfoState::PERFECT_CLIP),
            "the crate sets PERFECT_CLIP; the old test claimed nothing did"
        );
        assert!(!viewer.has(ObjectInfoState::IGNORE_CREATURES));

        // 2. Every object-info state constant in globals.rs, parsed as a number.
        let globals = include_str!("../globals.rs");
        let mut found = 0_usize;
        for line in globals.lines() {
            let Some(rest) = line.trim_start().strip_prefix("pub const ") else {
                continue;
            };
            let Some((name, value)) = rest.split_once(": u32 = ") else {
                continue;
            };
            if !name.ends_with("OBJECT_INFO_STATE") {
                continue;
            }
            let text = value.trim().trim_end_matches(';').replace('_', "");
            let v = text.strip_prefix("0x").map_or_else(
                || text.parse::<u32>().expect("decimal"),
                |hex| u32::from_str_radix(hex, 16).expect("hex"),
            );
            found += 1;
            assert_eq!(
                v & ObjectInfoState::IGNORE_CREATURES,
                0,
                "{name} = {v:#x} sets IGNORE_CREATURES, which has no writer in the client"
            );
            assert_eq!(v & !NAMED, 0, "{name} = {v:#x} sets a bit with no name");
        }
        assert_eq!(
            found, 1,
            "globals.rs must still declare exactly one such constant"
        );

        // 3. `get_object_info` is a closed sum of named bits.
        let step = include_str!("../step.rs");
        let body = step
            .split_once("fn get_object_info")
            .expect("the builder is still there")
            .1
            .split_once("\n    }\n")
            .expect("its body ends")
            .0;
        let allowed = [
            "EDGE_SLIDE",
            "CONTACT",
            "ON_WALKABLE",
            "FREE_ROTATE",
            "PATH_CLIPPED",
        ];
        let mut mentions = 0_usize;
        for piece in body.split("ObjectInfoState::").skip(1) {
            let bit: String = piece
                .chars()
                .take_while(|c| c.is_ascii_uppercase() || *c == '_')
                .collect();
            assert!(
                allowed.contains(&bit.as_str()),
                "get_object_info now contributes {bit}, which is outside the client's five"
            );
            mentions += 1;
        }
        assert!(
            mentions >= allowed.len(),
            "only {mentions} bits found; did it move?"
        );

        // 4. Nothing writes the field from a bare number.
        for (name, text) in [
            ("insert", include_str!("insert.rs")),
            ("walk", include_str!("walk.rs")),
            ("collide", include_str!("collide.rs")),
            ("place", include_str!("place.rs")),
            ("cylinder", include_str!("cylinder.rs")),
            ("mod", include_str!("mod.rs")),
            ("step", step),
        ] {
            for piece in text.split("object_info.state").skip(1) {
                let rhs = piece.trim_start();
                let Some(rhs) = rhs.strip_prefix("=").or_else(|| rhs.strip_prefix("|=")) else {
                    continue; // a read, not a write
                };
                let rhs = rhs.trim_start();
                assert!(
                    !rhs.starts_with(|c: char| c.is_ascii_digit()),
                    "{name}.rs assigns a bare number to object_info.state: {}",
                    &rhs[..rhs.len().min(40)]
                );
            }
        }
    }

    #[test]
    fn set_and_has_round_trip_without_disturbing_neighbours() {
        let mut o = ObjectInfo::default();
        o.set(ObjectInfoState::CONTACT, true);
        o.set(ObjectInfoState::EDGE_SLIDE, true);
        assert!(o.has(ObjectInfoState::CONTACT));
        assert!(o.has(ObjectInfoState::EDGE_SLIDE));
        assert!(!o.has(ObjectInfoState::ON_WALKABLE));
        o.set(ObjectInfoState::CONTACT, false);
        assert!(!o.has(ObjectInfoState::CONTACT));
        assert!(o.has(ObjectInfoState::EDGE_SLIDE));
    }
}
