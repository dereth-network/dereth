//! Target acquisition for using one item on another, and its confirmation/request boundary.
//! The item-use implementation keeps the callback's live send, busy-state, and using-item block,
//! all of which retail performs.
use super::{use_object::ItemUses, SplitState};
use crate::{Notice, NoticeSink, RequestSink, World};
use dereth_primitives::{ObjectId, ServerTime};

/// Useability bits for the least-limited source and target checks.
mod u {
    pub const SELF: u32 = 2;
    pub const WIELDED: u32 = 4;
    pub const CONTAINED: u32 = 8;
    pub const VIEWED: u32 = 16;
    pub const REMOTE: u32 = 32;
}

#[cfg(test)]
mod feedback_tests {
    use super::*;
    use crate::{RecordingRequests, RecordingSink, Weenie};

    fn world() -> World {
        let mut world = World::default();
        world.player = Some(ObjectId(3));
        let mut source = Weenie::new(ObjectId(1));
        source.pwd.name = "Gem".into();
        source.pwd.stack_size = Some(2);
        source.pwd.useability = Some(0x0020_0020);
        source.pwd.target_type = Some(1);
        let mut target = Weenie::new(ObjectId(2));
        target.pwd.name = "Ring".into();
        target.pwd.obj_type = 1;
        world.tables.weenies.insert(source.id, source);
        world.tables.weenies.insert(target.id, target);
        world
    }

    #[test]
    fn binary_seven_refusal_literals_keep_gate_order_and_plural_names() {
        for (station, expected) in [
            (0, "You must own the Gems to use it"),
            (1, "You must wield the Gems to use it"),
            (2, "You can't use the Gems on an item you are trading"),
            (3, "You can't use the Gems on what you don't own"),
            (4, "You can't use the Gems on what you aren't wielding"),
            (5, "Cannot use the Gems on yourself"),
            (6, "Cannot use the Gems with the Ring"),
        ] {
            let mut world = world();
            match station {
                0 => world.weenie_mut(ObjectId(1)).unwrap().pwd.useability = Some(0x0020_0008),
                1 => world.weenie_mut(ObjectId(1)).unwrap().pwd.useability = Some(0x0020_0004),
                2 => world.weenie_mut(ObjectId(2)).unwrap().trade_state = 1,
                3 => world.weenie_mut(ObjectId(1)).unwrap().pwd.useability = Some(0x0008_0020),
                4 => world.weenie_mut(ObjectId(1)).unwrap().pwd.useability = Some(0x0004_0020),
                5 => world.player = Some(ObjectId(2)),
                6 => world.weenie_mut(ObjectId(1)).unwrap().pwd.target_type = Some(0),
                _ => unreachable!(),
            }
            // Add later failure too, proving earlier refusal wins rather than merely checking text.
            world.weenie_mut(ObjectId(1)).unwrap().pwd.target_type = Some(0);
            assert!(!world.target_compatible_with_object(ObjectId(2), ObjectId(1)));
            let mut out = RecordingSink::default();
            let mut req = RecordingRequests::default();
            world.targeting_object = ObjectId(1);
            assert_eq!(
                world.target_acquired(
                    &mut req,
                    &mut out,
                    ObjectId(2),
                    SplitState::default(),
                    ServerTime(1.0)
                ),
                TargetedUseOutcome::Incompatible
            );
            assert_eq!(
                out.0,
                vec![Notice::DisplayString {
                    channel: 0x1a,
                    text: expected.into()
                }]
            );
            assert!(req.0.is_empty());
            assert_eq!(world.targeting_object, ObjectId(0));
        }
    }

    #[test]
    fn success_uses_is_player_not_is_creature_and_quiet_missing_is_silent() {
        let mut world = world();
        let mut out = RecordingSink::default();
        assert!(!world.report_target_compatibility(&mut out, ObjectId(0), ObjectId(1)));
        assert!(!world.report_target_compatibility(&mut out, ObjectId(2), ObjectId(99)));
        assert!(out.0.is_empty());
        world.weenie_mut(ObjectId(2)).unwrap().pwd.obj_type = 0x10; // creature, NOT player
        world.weenie_mut(ObjectId(1)).unwrap().pwd.target_type = Some(0x10);
        assert!(world.target_compatible_with_object(ObjectId(2), ObjectId(1)));
        assert!(world.report_target_compatibility(&mut out, ObjectId(2), ObjectId(1)));
        world.weenie_mut(ObjectId(2)).unwrap().pwd.bitfield |= crate::weenie::bitfield::PLAYER;
        assert!(world.report_target_compatibility(&mut out, ObjectId(2), ObjectId(1)));
        assert_eq!(
            out.0,
            vec![
                Notice::DisplayString {
                    channel: 0x1a,
                    text: "Using the Gems with the Ring".into()
                },
                Notice::DisplayString {
                    channel: 0x1a,
                    text: "Using the Gems on Ring".into()
                },
            ]
        );
    }

    #[test]
    fn type_refusal_composes_source_and_target_material_names() {
        let mut world = world();
        world.install_material_names(std::collections::BTreeMap::from([(0x3a, "Bronze".into())]));
        world.weenie_mut(ObjectId(1)).unwrap().pwd.material_type = Some(0x3a);
        world.weenie_mut(ObjectId(2)).unwrap().pwd.material_type = Some(0x3a);
        world.weenie_mut(ObjectId(1)).unwrap().pwd.target_type = Some(0);
        let mut out = RecordingSink::default();
        let mut req = RecordingRequests::default();
        world.targeting_object = ObjectId(1);
        assert_eq!(
            world.target_acquired(
                &mut req,
                &mut out,
                ObjectId(2),
                SplitState::default(),
                ServerTime(1.0)
            ),
            TargetedUseOutcome::Incompatible
        );
        assert_eq!(
            out.0,
            vec![Notice::DisplayString {
                channel: 0x1a,
                text: "Cannot use the Bronze Gems with the Bronze Ring".into()
            }]
        );
        assert!(req.0.is_empty());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetedUsageConfirmation {
    ManaStone,
    Salvage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetedUseOutcome {
    Incompatible,
    Retained,
    Confirming(TargetedUsageConfirmation),
    Sent,
}

#[derive(Clone, Copy)]
enum Refusal {
    Missing,
    OwnSource,
    WieldSource,
    Trading,
    OwnTarget,
    WieldTarget,
    SelfTarget,
    Type,
}

impl World {
    /// Quiet compatibility for cursor/hover; acquisition uses the same ordered gates with feedback.
    #[must_use]
    pub fn target_compatible_with_object(&self, target: ObjectId, source: ObjectId) -> bool {
        self.target_compatibility(target, source).is_ok()
    }

    fn target_compatibility(&self, target: ObjectId, source: ObjectId) -> Result<(), Refusal> {
        if source.0 == 0 || target.0 == 0 {
            return Err(Refusal::Missing);
        }
        let (Some(src), Some(tgt)) = (self.weenie(source), self.weenie(target)) else {
            return Err(Refusal::Missing);
        };
        let bits = src.pwd.useability.unwrap_or(0);
        let uses = ItemUses(bits);
        if !self.is_owned_by_player(source) {
            let source_use = uses.least_limited_source_use();
            if source_use & u::CONTAINED != 0 {
                return Err(Refusal::OwnSource);
            }
            if source_use & u::WIELDED != 0 {
                return Err(Refusal::WieldSource);
            }
        }
        if tgt.trade_state == 1 {
            return Err(Refusal::Trading);
        }
        let target_bits = bits >> 16;
        let target_use = [u::REMOTE, u::VIEWED, u::CONTAINED, u::WIELDED, u::SELF]
            .into_iter()
            .find(|b| target_bits & b != 0)
            .unwrap_or(target_bits & 0x80);
        let self_target = target_bits & u::SELF != 0;
        if !self.is_owned_by_player(target) {
            if target_use & u::CONTAINED != 0 {
                if self.player != Some(target) || !self_target {
                    return Err(Refusal::OwnTarget);
                }
            } else if target_use & u::WIELDED != 0 {
                return Err(Refusal::WieldTarget);
            }
        }
        if self.player == Some(target) && !self_target {
            return Err(Refusal::SelfTarget);
        }
        if src.pwd.target_type.unwrap_or(0) & tgt.inq_type() == 0 {
            return Err(Refusal::Type);
        }
        Ok(())
    }

    /// The target-compatibility check, called as `(target, source, false, true)`. All seven
    /// format literals were checked against the retail strings. No added newline.
    fn report_target_compatibility(
        &self,
        out: &mut dyn NoticeSink,
        target: ObjectId,
        source: ObjectId,
    ) -> bool {
        let result = self.target_compatibility(target, source);
        if matches!(result, Err(Refusal::Missing)) {
            return false;
        }
        let tgt = self.weenie(target).expect("compatibility required target");
        let name = self.notice_name(source);
        let text = match result {
            Err(Refusal::OwnSource) => format!("You must own the {name} to use it"),
            Err(Refusal::WieldSource) => format!("You must wield the {name} to use it"),
            Err(Refusal::Trading) => format!("You can't use the {name} on an item you are trading"),
            Err(Refusal::OwnTarget) => format!("You can't use the {name} on what you don't own"),
            Err(Refusal::WieldTarget) => {
                format!("You can't use the {name} on what you aren't wielding")
            }
            Err(Refusal::SelfTarget) => format!("Cannot use the {name} on yourself"),
            Err(Refusal::Type) => {
                format!(
                    "Cannot use the {name} with the {}",
                    self.notice_name(target)
                )
            }
            Ok(()) => format!(
                "Using the {name}{}{}",
                if tgt.is_player() {
                    " on "
                } else {
                    " with the "
                },
                self.notice_name(target)
            ),
            Err(Refusal::Missing) => unreachable!(),
        };
        out.emit(Notice::DisplayString {
            channel: 0x1a,
            text,
        });
        result.is_ok()
    }

    /// No fresh readiness/throttle check here: those belong to the first use-object call.
    /// Snapshot and clear occur before *every* compatibility/refusal/confirmation exit.
    pub fn target_acquired(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        target: ObjectId,
        split: SplitState,
        now: ServerTime,
    ) -> TargetedUseOutcome {
        let source = std::mem::replace(&mut self.targeting_object, ObjectId(0));
        if !self.report_target_compatibility(out, target, source) {
            return TargetedUseOutcome::Incompatible;
        }
        // Both objects were required by the compatibility gate above.
        let kind = if let (Some(src), Some(tgt)) = (self.weenie(source), self.weenie(target)) {
            // Empty mana stone first, retained refusal before dialog.
            if src.inq_type() & 0x0008_0000 != 0 && src.pwd.effects.unwrap_or(0) & 1 == 0 {
                if tgt.pwd.bitfield & 0x0100_0000 != 0 {
                    out.emit(Notice::DisplayString {
                        channel: 0,
                        text:
                            "You cannot drain the mana of this item because it is \"Retained\".\n"
                                .into(),
                    });
                    return TargetedUseOutcome::Retained;
                }
                Some(TargetedUsageConfirmation::ManaStone)
            // UseCraftSuccessDialog is the player module's option word 1, bit 31.
            } else if self.player_system.options.options & 0x8000_0000 == 0
                && src.inq_type() & 0x4000_0000 != 0
            {
                Some(TargetedUsageConfirmation::Salvage)
            } else {
                None
            }
        } else {
            None
        };
        if let Some(kind) = kind {
            out.emit(Notice::TargetedUsageConfirmation {
                source,
                target,
                kind,
            });
            return TargetedUseOutcome::Confirming(kind);
        }
        self.confirm_targeted_usage(req, out, source, target, split, now);
        TargetedUseOutcome::Sent
    }

    /// The targeted-usage callback: an accepted dialog with both ids nonzero sends 0035,
    /// increments busy, then marks the source as in use `(source, 1, 0)`, without consulting the
    /// current selection or re-running compatibility. A cancelled dialog never calls this accepted
    /// branch.
    pub fn confirm_targeted_usage(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        source: ObjectId,
        target: ObjectId,
        split: SplitState,
        now: ServerTime,
    ) {
        if source.0 == 0 || target.0 == 0 {
            return;
        }
        self.attempt_use_with_target(req, source, target);
        self.magic.busy_count += 1;
        let result = self.determine_use_result(source);
        self.using_item(req, out, source, result, split, now);
    }
}
