//! `MotionTable` at runtime: the combined key, the four maps and the three helpers that
//! move a `MotionData` into a [`Sequence`].
//!
//! See `docs/formats/19-motion-table.md`.

pub mod manager;
pub mod sequence_gen;
pub mod state;

use std::sync::Arc;

use dereth_primitives::num::consts::EPSILON;

use crate::command::MotionCommand;
use crate::data::{AnimAssets, MotionData, MotionTableData};
use crate::frame::V3;
use crate::seq::Sequence;

pub use manager::{AnimNode, MotionTableManager, MovementStruct, MovementType};
pub use state::{MotionEntry, MotionState};

/// The loaded motion-table dat object, shared between every user of the id.
///
/// The per-object mutable state is only [`MotionState`] and the manager's pending list, which is
/// why this wraps an `Arc` and is cheap to clone.
#[derive(Debug, Clone)]
pub struct MotionTable {
    data: Arc<MotionTableData>,
}

impl MotionTable {
    #[must_use]
    pub fn new(data: Arc<MotionTableData>) -> Self {
        Self { data }
    }

    #[must_use]
    pub fn data(&self) -> &MotionTableData {
        &self.data
    }

    /// The combined key: `(style << 16) | (motion & 0x00FFFFFF)`.
    ///
    /// **Do not mask the style.** The shift keeps only its low 16 bits, which is what makes
    /// `NonCombat (0x8000003D)` become `0x003D0000`; masking first would be the same today and
    /// wrong the moment a style's payload exceeded 16 bits. The two halves formally overlap in
    /// bits 16–23 and never collide in the shipped data.
    #[must_use]
    pub const fn key(style: MotionCommand, motion: MotionCommand) -> u32 {
        (style.0 << 16) | (motion.0 & 0x00FF_FFFF)
    }

    /// `style_defaults` is the one table keyed by the **raw** stance value.
    #[must_use]
    pub fn style_default(&self, style: MotionCommand) -> Option<MotionCommand> {
        self.data.style_defaults.get(&style.0).copied()
    }

    #[must_use]
    pub fn default_style(&self) -> MotionCommand {
        self.data.default_style
    }

    #[must_use]
    pub fn cycle(&self, style: MotionCommand, motion: MotionCommand) -> Option<&MotionData> {
        self.data.cycles.get(&Self::key(style, motion))
    }

    /// The modifier lookup, including the client's bare `motion & 0xFFFFFF` fallback for a
    /// style-independent modifier.
    #[must_use]
    pub fn modifier(&self, style: MotionCommand, motion: MotionCommand) -> Option<&MotionData> {
        self.data
            .modifiers
            .get(&Self::key(style, motion))
            .or_else(|| self.data.modifiers.get(&motion.ordinal()))
    }

    /// The link between two motions.
    ///
    /// A **reversed** transition (either speed negative) swaps the roles of `from` and `to`, so one
    /// stored transition serves both directions when played backwards — which is what the negative
    /// `framerate` support in `AnimData` exists for. The two fallbacks differ: forwards falls back
    /// to the `links[style << 16]` "from anything" bucket, backwards falls back to the style's
    /// default substate.
    #[must_use]
    pub fn get_link(
        &self,
        style: MotionCommand,
        from: MotionCommand,
        from_speed: f32,
        to: MotionCommand,
        to_speed: f32,
    ) -> Option<&MotionData> {
        if to_speed < 0.0 || from_speed < 0.0 {
            if let Some(group) = self.data.links.get(&Self::key(style, to)) {
                if let Some(md) = group.get(&from.0) {
                    return Some(md);
                }
            }
            let def = self.style_default(style)?;
            let group = self.data.links.get(&Self::key(style, from))?;
            group.get(&def.0)
        } else {
            if let Some(group) = self.data.links.get(&Self::key(style, from)) {
                if let Some(md) = group.get(&to.0) {
                    return Some(md);
                }
            }
            let group = self.data.links.get(&(style.0 << 16))?;
            group.get(&to.0)
        }
    }

    /// Whether a motion is allowed in the current state.
    ///
    /// A motion whose `bitfield & 2` is set may only be started from the style's default substate,
    /// or when it is already the current substate. That is how "you cannot draw a bow while
    /// sitting" is expressed.
    #[must_use]
    pub fn is_allowed(&self, motion: MotionCommand, md: &MotionData, state: &MotionState) -> bool {
        if md.restricted_to_default_substate() && motion != state.substate {
            return self.style_default(state.style) == Some(state.substate);
        }
        true
    }
}

/// True when both are ≥ 0 or both < 0. You cannot smoothly reverse a
/// cycle, so this decides whether a cycle can be speed-adjusted in place.
#[must_use]
pub fn same_sign(a: f32, b: f32) -> bool {
    (a >= 0.0) == (b >= 0.0)
}

/// Replace the sequence's velocity and omega and append every animation.
///
/// Animation-rate scaling performs one `AnimData`-by-`float` multiply, and the expansion gate
/// compares the result as `f32::to_bits`, so this must not be reassociated.
pub fn add_motion(
    seq: &mut Sequence,
    md: Option<&MotionData>,
    speed: f32,
    assets: &dyn AnimAssets,
) {
    let Some(md) = md else {
        return;
    };
    seq.set_velocity(md.velocity.mul(speed));
    seq.set_omega(md.omega.mul(speed));
    for a in &md.anims {
        seq.append_animation(a.scaled(speed), assets);
    }
}

/// Add to the existing velocity/omega without touching animations.
pub fn combine_motion(seq: &mut Sequence, md: Option<&MotionData>, speed: f32) {
    if let Some(md) = md {
        seq.combine_physics(md.velocity.mul(speed), md.omega.mul(speed));
    }
}

/// The inverse of [`combine_motion`].
pub fn subtract_motion(seq: &mut Sequence, md: Option<&MotionData>, speed: f32) {
    if let Some(md) = md {
        seq.subtract_physics(md.velocity.mul(speed), md.omega.mul(speed));
    }
}

/// Retime a running cycle without re-queuing its animations.
///
/// Exact only because the cycle's framerate is always `base_framerate × old_speed`.
pub fn change_cycle_speed(seq: &mut Sequence, old_speed: f32, new_speed: f32) {
    if old_speed.abs() > EPSILON {
        seq.multiply_cyclic_animation_fr(new_speed / old_speed);
    } else if new_speed.abs() < EPSILON {
        seq.multiply_cyclic_animation_fr(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::AnimData;
    use dereth_primitives::Vec3;

    /// ORACLE: `docs/formats/19-motion-table.md`, "Key packing", and the worked
    /// example `0x0900013A` whose single cycle key is `0x003D0003`.
    #[test]
    fn the_combined_key_shifts_the_unmasked_style() {
        let k = MotionTable::key(MotionCommand::NON_COMBAT, MotionCommand::READY);
        assert_eq!(k, 0x003D_0003);
        // The "from anything" link bucket is `style << 16` with motion 0.
        assert_eq!(
            MotionTable::key(MotionCommand::NON_COMBAT, MotionCommand::NONE),
            0x003D_0000
        );
        // Masking the style first happens to give the same answer today; the point is that the
        // implementation does not do it.
        assert_eq!(
            MotionTable::key(MotionCommand::DUAL_WIELD_COMBAT, MotionCommand::READY),
            0x0046_0003
        );
    }

    /// `same_sign` treats zero as positive, which is what makes a stopped cycle re-timeable.
    #[test]
    fn same_sign_puts_zero_on_the_positive_side() {
        assert!(same_sign(1.0, 2.0));
        assert!(same_sign(0.0, 5.0));
        assert!(same_sign(-1.0, -0.5));
        assert!(!same_sign(-1.0, 1.0));
        assert!(!same_sign(0.0, -1.0));
    }

    /// `add_motion` scales velocity, omega **and** every framerate by the same speed.
    #[test]
    fn add_motion_scales_the_physics_and_every_framerate() {
        let (id, a) = crate::seq::tests::anim(1, 10);
        let assets = crate::seq::tests::assets_with(&[(id, a)]);
        let md = MotionData {
            anims: vec![
                AnimData {
                    anim_id: id,
                    low_frame: 0,
                    high_frame: -1,
                    framerate: 30.0,
                },
                AnimData {
                    anim_id: id,
                    low_frame: 0,
                    high_frame: -1,
                    framerate: 10.0,
                },
            ],
            bitfield: 0,
            velocity: Vec3::new(0.0, 4.0, 0.0),
            omega: Vec3::new(0.0, 0.0, 1.0),
        };
        let mut seq = Sequence::new();
        add_motion(&mut seq, Some(&md), 0.5, &assets);
        assert_eq!(seq.velocity, Vec3::new(0.0, 2.0, 0.0));
        assert_eq!(seq.omega, Vec3::new(0.0, 0.0, 0.5));
        assert_eq!(seq.nodes().len(), 2);
        assert_eq!(seq.nodes()[0].framerate.to_bits(), 15.0_f32.to_bits());
        assert_eq!(seq.nodes()[1].framerate.to_bits(), 5.0_f32.to_bits());
        assert_eq!(seq.first_cyclic(), Some(1));

        // A `None` motion is a no-op: `GetObjectSequence` passes NULL links straight through.
        let before = seq.velocity;
        add_motion(&mut seq, None, 2.0, &assets);
        assert_eq!(seq.velocity, before);
        assert_eq!(seq.nodes().len(), 2);
    }

    /// `combine_motion`/`subtract_motion` are additive and exactly inverse.
    #[test]
    fn combine_and_subtract_motion_are_inverse() {
        let md = MotionData {
            anims: Vec::new(),
            bitfield: 0,
            velocity: Vec3::new(1.0, 2.0, 3.0),
            omega: Vec3::new(0.0, 0.0, 1.0),
        };
        let mut seq = Sequence::new();
        combine_motion(&mut seq, Some(&md), 2.0);
        assert_eq!(seq.velocity, Vec3::new(2.0, 4.0, 6.0));
        subtract_motion(&mut seq, Some(&md), 2.0);
        assert_eq!(seq.velocity, Vec3::ZERO);
    }

    /// `change_cycle_speed` multiplies by `new/old`, and takes the "stop" path only when both are
    /// below the epsilon.
    #[test]
    fn change_cycle_speed_retimes_the_cyclic_block() {
        let (id, a) = crate::seq::tests::anim(1, 10);
        let assets = crate::seq::tests::assets_with(&[(id, a)]);
        let mut seq = Sequence::new();
        seq.append_animation(
            AnimData {
                anim_id: id,
                low_frame: 0,
                high_frame: -1,
                framerate: 30.0,
            },
            &assets,
        );
        change_cycle_speed(&mut seq, 1.0, 2.0);
        assert_eq!(seq.nodes()[0].framerate, 60.0);
        change_cycle_speed(&mut seq, 2.0, 1.0);
        assert_eq!(seq.nodes()[0].framerate, 30.0);
        // old ≈ 0 and new ≈ 0: the framerate is zeroed rather than divided by zero.
        change_cycle_speed(&mut seq, 0.0, 0.0);
        assert_eq!(seq.nodes()[0].framerate, 0.0);
    }

    /// `is_allowed`: `bitfield & 2` restricts a motion to the style's default substate.
    #[test]
    fn is_allowed_restricts_omega_motions_to_the_default_substate() {
        let mut d = MotionTableData {
            default_style: MotionCommand::NON_COMBAT,
            ..MotionTableData::default()
        };
        d.style_defaults
            .insert(MotionCommand::NON_COMBAT.0, MotionCommand::READY);
        let t = MotionTable::new(Arc::new(d));
        let md = MotionData {
            bitfield: 2,
            ..MotionData::default()
        };
        let free = MotionData {
            bitfield: 0,
            ..MotionData::default()
        };

        let at_default = MotionState {
            style: MotionCommand::NON_COMBAT,
            substate: MotionCommand::READY,
            ..MotionState::new()
        };
        let elsewhere = MotionState {
            style: MotionCommand::NON_COMBAT,
            substate: MotionCommand::CROUCH,
            ..MotionState::new()
        };
        assert!(t.is_allowed(MotionCommand::WALK_FORWARD, &md, &at_default));
        assert!(!t.is_allowed(MotionCommand::WALK_FORWARD, &md, &elsewhere));
        // Already the current substate: allowed from anywhere.
        assert!(t.is_allowed(MotionCommand::CROUCH, &md, &elsewhere));
        // No restriction bit: always allowed.
        assert!(t.is_allowed(MotionCommand::WALK_FORWARD, &free, &elsewhere));
    }
}
