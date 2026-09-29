//! Animation hooks: the 27 types, `direction_`, and the deferred queue.
//!
//! Three rules decide the shape of this module:
//!
//! * **Type 4 (`AnimationDone`) and any unknown type are silently dropped** by the decoded-hook
//!   switch: the read position is aligned and nothing is returned. The client
//!   does not need a data-driven animation-done hook because sequence updating pushes
//!   one global whenever a link animation completes. `from_decoded`
//!   reproduces the drop, and it is a `None`, not an error.
//! * **`direction_ == -2` never fires.** It is every constructor's initial value, overwritten by
//!   unpacking; the firing test is `d == 0 || d == direction` and
//!   `direction` is only ever `+1` or `-1`.
//! * **The list is in file order.** Appending walks the list to the end and links there, so the
//!   list is not reversed.

pub mod exec;

use dereth_primitives::{DataId, Frame, Vec3};

pub use exec::{AnimEvent, HookQueue, SoundType};

/// `AttackCone` — the payload of hook type 3.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AttackCone {
    pub part_index: u32,
    pub left: (f32, f32),
    pub right: (f32, f32),
    pub radius: f32,
    pub height: f32,
}

/// The 27 hook payloads. Variants that share a payload but not a target are kept separate, because
/// the *target* is the observable part: type 7 is `SetPartTranslucency`, type 9 is
/// `SetPartLuminosity` and type 11 is `SetPartDiffusion`, all with `{part, start, end, time}`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HookKind {
    /// 0: no-op — no payload, no effect.
    NoOp,
    /// 1: play sound `gid` on the object.
    Sound { gid: DataId },
    /// 2: play the object's sound-table entry for `sound_type`.
    SoundTable { sound_type: u32 },
    /// 3: attack using the supplied cone.
    Attack(AttackCone),
    /// 4 `AnimDoneHook`. **Never unpacked**: unpacking drops type 4 and the client instead
    /// pushes one global whenever a
    /// link animation finishes. Constructed only by the player, in [`crate::seq::update`].
    AnimationDone,
    /// 5 `ReplaceObjectHook`. Retail's execute step has no body, so the
    /// hook is unpacked and stored and executing it is a no-op that logs once.
    ReplaceObject { part_index: u32, part_id: DataId },
    /// 6 `EtherealHook` → set the object ethereal.
    Ethereal { ethereal: i32 },
    /// 7 `TransparentPartHook` → ramp one part's translucency.
    TransparentPart {
        part: u32,
        start: f32,
        end: f32,
        time: f32,
    },
    /// 8 `LuminousHook` → ramp the object's luminosity.
    Luminous { start: f32, end: f32, time: f32 },
    /// 9 `LuminousPartHook` → ramp one part's luminosity.
    LuminousPart {
        part: u32,
        start: f32,
        end: f32,
        time: f32,
    },
    /// 10 `DiffuseHook` → ramp the object's diffusion.
    Diffuse { start: f32, end: f32, time: f32 },
    /// 11 `DiffusePartHook` → ramp one part's diffusion.
    DiffusePart {
        part: u32,
        start: f32,
        end: f32,
        time: f32,
    },
    /// 12 `ScaleHook` → ramp the object's scale.
    Scale { end: f32, time: f32 },
    /// 13 `CreateParticleHook` → create a particle emitter.
    CreateParticle {
        info: DataId,
        part_index: u32,
        offset: Frame,
        emitter_id: u32,
    },
    /// 14 `DestroyParticleHook` → destroy one.
    DestroyParticle { emitter_id: u32 },
    /// 15 `StopParticleHook` → stop one.
    StopParticle { emitter_id: u32 },
    /// 16 `NoDrawHook` → set the object's no-draw flag.
    NoDraw { nodraw: i32 },
    /// 17: default script → `play_default_script() `.
    DefaultScript,
    /// 18 `DefaultScriptPartHook` → `play_default_script(part) `.
    DefaultScriptPart { part_index: u32 },
    /// 19 `CallPESHook` → call a particle effect script after a delay.
    CallPes { pes: DataId, pause: f32 },
    /// 20 `TransparentHook` → ramp the object's translucency.
    Transparent { start: f32, end: f32, time: f32 },
    /// 21: play sound `gid` with priority, probability and volume overrides.
    ///
    /// **Probability before priority on the wire.** ACE has them the other way round; contract
    /// 10.6, confirmed against all 541 shipped hooks.
    SoundTweaked {
        gid: DataId,
        probability: f32,
        priority: f32,
        volume: f32,
    },
    /// 22 `SetOmegaHook` → set the angular velocity. A *set*, not an add.
    SetOmega { axis: Vec3 },
    /// 23 `TextureVelocityHook` → set the object's texture velocity.
    TextureVelocity { u_speed: f32, v_speed: f32 },
    /// 24 `TextureVelocityPartHook` → set one part's texture velocity.
    TextureVelocityPart {
        part_index: u32,
        u_speed: f32,
        v_speed: f32,
    },
    /// 25 `SetLightHook` → set the object's lights flag.
    SetLight { lights_on: i32 },
    /// 26: create-blocking-particle → create a blocking particle emitter. Same
    /// payload as type 13; "blocking" means "do not restart an effect already running".
    CreateBlockingParticle {
        info: DataId,
        part_index: u32,
        offset: Frame,
        emitter_id: u32,
    },
}

impl HookKind {
    /// The number that is written to the dat.
    ///
    /// The hook serializer asks each hook for it. The base is abstract, and each concrete hook
    /// returns its own constant.
    /// `\[verified\]`
    #[must_use]
    pub const fn hook_type(self) -> u32 {
        match self {
            Self::NoOp => 0,
            Self::Sound { .. } => 1,
            Self::SoundTable { .. } => 2,
            Self::Attack(_) => 3,
            Self::AnimationDone => 4,
            Self::ReplaceObject { .. } => 5,
            Self::Ethereal { .. } => 6,
            Self::TransparentPart { .. } => 7,
            Self::Luminous { .. } => 8,
            Self::LuminousPart { .. } => 9,
            Self::Diffuse { .. } => 10,
            Self::DiffusePart { .. } => 11,
            Self::Scale { .. } => 12,
            Self::CreateParticle { .. } => 13,
            Self::DestroyParticle { .. } => 14,
            Self::StopParticle { .. } => 15,
            Self::NoDraw { .. } => 16,
            Self::DefaultScript => 17,
            Self::DefaultScriptPart { .. } => 18,
            Self::CallPes { .. } => 19,
            Self::Transparent { .. } => 20,
            Self::SoundTweaked { .. } => 21,
            Self::SetOmega { .. } => 22,
            Self::TextureVelocity { .. } => 23,
            Self::TextureVelocityPart { .. } => 24,
            Self::SetLight { .. } => 25,
            Self::CreateBlockingParticle { .. } => 26,
        }
    }
}

/// The animation-hook direction values. `-2` is the pre-unpack default and never fires.
pub const BOTH_ANIMHOOK: i32 = 0;
pub const FORWARD_ANIMHOOK: i32 = 1;
pub const BACKWARD_ANIMHOOK: i32 = -1;
pub const UNKNOWN_ANIMHOOK: i32 = -2;

/// One hook: its direction and its payload. Hooks are shared, immutable dat objects in the client
/// and carry no per-object state, so this is `Copy`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnimHook {
    /// Hook playback direction.
    pub direction: i32,
    pub kind: HookKind,
}

impl AnimHook {
    #[must_use]
    pub const fn new(direction: i32, kind: HookKind) -> Self {
        Self { direction, kind }
    }

    /// The sequence's hook execution: `d == 0 || direction == d`.
    ///
    /// `direction` is `+1` when `frame_number` is increasing and `-1` when it is decreasing, so a
    /// hook with `direction_ == -2` never fires.
    #[must_use]
    pub const fn fires(self, direction: i32) -> bool {
        self.direction == BOTH_ANIMHOOK || self.direction == direction
    }

    #[must_use]
    pub const fn hook_type(self) -> u32 {
        self.kind.hook_type()
    }
}

/// Append at the **tail**, i.e. keep file order.
///
/// A one-line function that exists to be named: the ordering it fixes is file order, and it is
/// the one an implementer is most likely to invert by pushing at the head.
pub fn add_to_list(list: &mut Vec<AnimHook>, hook: AnimHook) {
    list.push(hook);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `direction_ == -2` never fires; `0` fires both ways.
    #[test]
    fn the_direction_test_matches_execute_hooks() {
        let both = AnimHook::new(BOTH_ANIMHOOK, HookKind::NoOp);
        let fwd = AnimHook::new(FORWARD_ANIMHOOK, HookKind::NoOp);
        let back = AnimHook::new(BACKWARD_ANIMHOOK, HookKind::NoOp);
        let never = AnimHook::new(UNKNOWN_ANIMHOOK, HookKind::NoOp);
        assert!(both.fires(1) && both.fires(-1));
        assert!(fwd.fires(1) && !fwd.fires(-1));
        assert!(!back.fires(1) && back.fires(-1));
        assert!(!never.fires(1) && !never.fires(-1));
    }

    /// `add_to_list` appends at the tail. Contract 7.10.
    #[test]
    fn add_to_list_appends_at_the_tail() {
        let mut l = Vec::new();
        add_to_list(&mut l, AnimHook::new(0, HookKind::NoOp));
        add_to_list(&mut l, AnimHook::new(0, HookKind::DefaultScript));
        assert_eq!(l[0].hook_type(), 0);
        assert_eq!(l[1].hook_type(), 17);
    }
}
