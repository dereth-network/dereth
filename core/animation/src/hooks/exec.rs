//! The deferred hook queue and the events a hook raises outside this crate.
//!
//! The client appends hooks to an ordered queue and executes every entry **in order** at the end of the
//! physics step, then empties the array wholesale. Because the array is emptied wholesale rather
//! than deduplicated, **a hook whose frame is crossed twice in one step executes twice**.
//!
//! Animation hooks are deferred like this; *physics-script* hooks execute inline during the
//! script update. Both orders are observable when a script hook creates
//! an emitter that the same frame's animation hook then stops, which is why `script.rs` does not
//! use this queue.

use dereth_primitives::{DataId, Vec3};

use super::{AnimHook, AttackCone};
use crate::command::MotionCommand;

/// A logical sound id, resolved through the object's sound table. The audio crate owns the
/// resolution; this crate only names the type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoundType(pub u32);

/// What a hook asks of the world outside this crate.
///
/// Raised rather than called, because the target lives in another crate. **The order is
/// observable**, so this is drained FIFO from a `Vec` and never collected into a
/// set.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnimEvent {
    /// For the audio crate. Both sound-entry forms carry `priority`, `probability` and `volume`;
    /// a plain `SoundHook` uses the tweaked form's defaults (0.9, 1.0, 1.0).
    PlaySound {
        gid: DataId,
        volume: f32,
        priority: f32,
        probability: f32,
    },
    /// For the audio crate. Play a sound by `SoundType` on the object.
    PlaySoundType {
        kind: SoundType,
    },
    /// For physics. The object's attack.
    Attack {
        cone: AttackCone,
    },
    /// For physics. Changes collision, not just appearance.
    SetEthereal(bool),
    /// For physics and the client. The object's no-draw flag.
    SetNoDraw(bool),
    /// For physics and the client. The object's lights flag.
    SetLights(bool),
    /// For physics. A *set*, not an add; the last hook wins.
    SetOmega(Vec3),
    /// For physics: the value stored as the object's scale, which is the one collision reads.
    /// The variant carries the scale in force, not the hook's raw `{ end, time }`: the driver runs
    /// the ramp itself, as retail does, and raises this once per step with the scale that is now in
    /// force — so the receiver is a plain assignment and never has to know about `time`.
    SetScale(f32),
    /// Create a particle emitter, blocking or not.
    CreateParticleEmitter {
        info: DataId,
        part: u32,
        offset: dereth_primitives::Frame,
        id: u32,
        blocking: bool,
    },
    DestroyParticleEmitter(u32),
    StopParticleEmitter(u32),
    /// For the renderer. The object's or one part's texture velocity.
    SetTextureVelocity {
        part: Option<u32>,
        u: f32,
        v: f32,
    },
    /// Play the object's default script, optionally for one part.
    PlayDefaultScript {
        part: Option<u32>,
    },
    /// Call a particle effect script after a delay.
    CallPes {
        script: DataId,
        pause: f32,
    },
    /// The motion-done notification into the motion interpreter.
    MotionDone {
        motion: MotionCommand,
        success: bool,
    },
    /// In the client the replace-object hook's execute step is an unimplemented stub. The hook is
    /// unpacked and stored; raising this event records that it fired without inventing a
    /// behaviour.
    ReplaceObjectNoOp {
        part_index: u32,
        part_id: DataId,
    },
}

/// Deferred animation-hook queue for the physics object.
///
/// The client's `SmartArray` grows 0 → 8 → 16 → …; the growth policy is not observable, the order
/// is.
#[derive(Debug, Default, Clone)]
pub struct HookQueue {
    pending: Vec<AnimHook>,
}

impl HookQueue {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            pending: Vec::new(),
        }
    }

    /// Queue one animation hook on the object.
    pub fn push(&mut self, h: AnimHook) {
        self.pending.push(h);
    }

    pub fn extend(&mut self, hooks: impl IntoIterator<Item = AnimHook>) {
        self.pending.extend(hooks);
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    #[must_use]
    pub fn peek(&self) -> &[AnimHook] {
        &self.pending
    }

    /// Take everything queued, in queue order, and empty the array — the second half of
    /// `process_hooks`. Deliberately not a deduplicating drain.
    #[must_use]
    pub fn take(&mut self) -> Vec<AnimHook> {
        std::mem::take(&mut self.pending)
    }

    /// Teardown paths drop the queue without executing it.
    pub fn clear(&mut self) {
        self.pending.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::HookKind;

    /// ORACLE: the recovered animation-hook behavior section 1 —
    /// `process_hooks` executes in order and empties the array wholesale, so a hook queued twice
    /// executes twice.
    #[test]
    fn the_queue_is_fifo_and_does_not_deduplicate() {
        let mut q = HookQueue::new();
        let a = AnimHook::new(0, HookKind::NoOp);
        let b = AnimHook::new(0, HookKind::DefaultScript);
        q.push(a);
        q.push(b);
        q.push(a);
        assert_eq!(q.len(), 3);
        let taken = q.take();
        assert_eq!(
            taken.iter().map(|h| h.hook_type()).collect::<Vec<_>>(),
            vec![0, 17, 0]
        );
        assert!(q.is_empty(), "the array is emptied wholesale");
    }
}
