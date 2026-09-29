//! XP curves, raise costs, level, and the item-levelling curves.
//!
//! **Never mutate a skill or attribute locally in response to a raise click.** The client sends
//! `Train*`, sets its awaiting-raise latch and does not touch its local copy until the server sends the
//! changed quality back; the latch exists precisely to stop double-spending.
//!
//! The pure rules of this module live in [`dereth_rules::advancement`]; they are re-exported
//! here, so every `dereth_client_model::advancement::*` path resolves. So do the raise
//! costs (`skill_cost_to_raise`, `skill_cost_to_raise_10`), written against
//! `dereth_rules::quality::QualityRead`.

use crate::qualities::Qualities;
use crate::skills::Sac;
#[cfg(test)]
use dereth_assets::tables::{SkillTable, XpTable};

pub use dereth_rules::advancement::*;

/// The experience types: what a spend of experience raises.
pub mod experience_type {
    pub const UNDEF: u32 = 0;
    pub const ATTRIBUTE: u32 = 1;
    pub const ATTRIBUTE_2ND: u32 = 2;
    pub const TRAINED_SKILL: u32 = 3;
    pub const SPECIALIZED_SKILL: u32 = 4;
    pub const LEVEL: u32 = 5;
    pub const CREDIT: u32 = 6;
}

/// The generic consent dialog's discriminator, shared with the dialog engine.
pub use dereth_client_contract::confirmation::ConfirmationType;

// The **client-side** caller of [`vitae_cp_pool_threshold`] is
// `dereth_client::hud::HudView::vitae_display`.

// ---------------------------------------------------------------------------------------------
// The senders
// ---------------------------------------------------------------------------------------------

/// The train-skill request — `0x0046`.
///
/// The "raise 1" and "raise 10" buttons both send this one request; the only difference between
/// them is whether the amount came from [`skill_cost_to_raise`] or [`skill_cost_to_raise_10`].
/// There is no "raise ten" opcode.
///
/// **The one gate the client applies is advancement class `> UNTRAINED`**, read fresh out of
/// the player's qualities immediately before the send — not off the panel. A skill at or below
/// `UNTRAINED` is *trained*, not raised, and takes [`send_train_skill_advancement_class`]
/// instead. Returns false when nothing was sent, which is the caller's cue to report rather than
/// to assume delivery.
///
/// Nothing local is mutated: the client sets
/// its awaiting-raise latch and waits for the changed quality; a local increment would double-spend
/// against the server's own answer.
pub fn send_train_skill(
    q: &Qualities,
    req: &mut dyn crate::RequestSink,
    skill_id: u32,
    xp: u32,
) -> bool {
    let sac = q
        .skill(skill_id)
        .map_or(Sac::Undef, |s| Sac::from_raw(s.sac));
    if sac <= Sac::Untrained {
        return false;
    }
    req.send(crate::Request::TrainSkill(
        dereth_protocol::admin::TrainSkill {
            skill_id,
            xp_spent: xp,
        },
    ));
    true
}

/// The train-skill request, `TrainSkillAdvancementClass(skill, credits)` — `0x0047`.
///
/// The *train* half, reached through the confirmation
/// dialog's callback. The mirror gate of
/// [`send_train_skill`]: a skill already `TRAINED` or better cannot be trained again.
pub fn send_train_skill_advancement_class(
    q: &Qualities,
    req: &mut dyn crate::RequestSink,
    skill_id: u32,
    credits: u32,
) -> bool {
    let sac = q
        .skill(skill_id)
        .map_or(Sac::Undef, |s| Sac::from_raw(s.sac));
    if sac > Sac::Untrained {
        return false;
    }
    req.send(crate::Request::TrainSkillAdvancementClass(
        dereth_protocol::admin::TrainSkillAdvancementClass {
            skill_id,
            credits_spent: credits,
        },
    ));
    true
}

// ---------------------------------------------------------------------------------------------
// The attribute senders, the mirror of the skill pair above.
// ---------------------------------------------------------------------------------------------

/// The raise-attribute request, `TrainAttribute(attribute, xp)` — `0x0045`.
///
/// **There is no gate, and inventing one would be the defect.** The attribute panel's raise
/// handler is the mirror of the skill panel's in every respect but this: where the skill sender
/// re-reads the training state and refuses at or below `UNTRAINED`, the attribute sender has no
/// such comparison in it at all. Its only three conditions are that a row is selected (which
/// `AttributesPanel` already enforces), that the player description resolves, and the row's own
/// stat type — which chooses *which* of the two opcodes is sent, not *whether* one is. An
/// attribute cannot be untrained, so there is nothing for a gate to refuse.
///
/// **`q` is taken and deliberately not read.** It stands for the player description whose
/// acquisition is the client's one real precondition for the send. Requiring the caller to hold
/// it is how that precondition is expressed here; reading a level or a cost out of it would be
/// adding the gate the client does not have.
///
/// `attribute_id` is the value the row reports, which for a primary is its own id. Nothing local
/// is mutated: the panel marks itself as awaiting the raise before it calls this and waits for the
/// changed quality, exactly as [`send_train_skill`] documents.
pub fn send_train_attribute(
    _q: &Qualities,
    req: &mut dyn crate::RequestSink,
    attribute_id: u32,
    xp: u32,
) {
    req.send(crate::Request::TrainAttribute(
        dereth_protocol::admin::TrainAttribute {
            attribute_id,
            xp_spent: xp,
        },
    ));
}

/// The raise-vital request, `TrainAttribute2nd(vital, xp)` — `0x0044`.
///
/// The other arm of the same stat-type-8 test in the panel's raise handler, and ungated for
/// the same reason as [`send_train_attribute`].
///
/// **`vital_id` is the *odd*, maximum id, and this is the one thing here that is invisible on
/// screen and wrong on the wire.** The attribute panel builds the three secondary-attribute rows
/// with the even, *current* ids `2, 4, 6`; each row stores `max = current - 1`, and the value the
/// row reports back to the raise handler is *that*, one less — which
/// is what the raise handler hands the message — so the Health row asks the server
/// to raise `1 MaxHealth`, and every observed `0x0044` request carries an odd id. The
/// conversion is `AttributeRow::wire_stat`'s, upstream of this function; what this doc note buys
/// is that a future caller passing the row's `stat` has been warned.
pub fn send_train_attribute_2nd(
    _q: &Qualities,
    req: &mut dyn crate::RequestSink,
    vital_id: u32,
    xp: u32,
) {
    req.send(crate::Request::TrainAttribute2nd(
        dereth_protocol::admin::TrainAttribute2nd {
            vital_id,
            xp_spent: xp,
        },
    ));
}

/// Send spellbook-filter mask event `0x0286`.
///
/// The spellbook panel sends this unconditionally once it has established
/// that the mask changed, so there is no gate here beyond the caller's own `new != old`.
pub fn send_spellbook_filter(req: &mut dyn crate::RequestSink, mask: u32) {
    req.send(crate::Request::SpellbookFilterEvent(
        dereth_protocol::combat::CharacterSpellbookFilterEvent { filter_mask: mask },
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_primitives::DataId;

    fn xp_table() -> XpTable {
        XpTable {
            id: DataId(0x0E00_0018),
            attribute_xp: vec![0, 10, 30, 60, 100],
            vital_xp: vec![0, 5, 15, 30, 50],
            trained_xp: vec![0, 100, 300, 700, 1500],
            specialized_xp: vec![0, 50, 150, 350, 750],
            level_xp: vec![0, 1000, 3000, 7000],
            level_credits: vec![0, 0, 1, 1],
        }
    }

    /// Oracle: §6.1 — the cost is measured against `_pp`, so a partially-filled level costs less.
    #[test]
    fn raise_costs_are_measured_against_experience_already_sunk() {
        use dereth_protocol::types::qualities::Skill;
        let xp = xp_table();
        let skills = {
            use dereth_assets::tables::{SkillBase, SkillFormula};
            let mut m = std::collections::BTreeMap::new();
            m.insert(
                7u32,
                SkillBase {
                    description: String::new(),
                    name: String::new(),
                    icon: 0,
                    trained_cost: 6,
                    specialized_cost: 12,
                    category: 1,
                    chargen_use: 0,
                    min_level: 1,
                    formula: SkillFormula {
                        w: 0,
                        x: 1,
                        y: 0,
                        z: 1,
                        attr1: 1,
                        attr2: 0,
                    },
                    upper_bound: 0.0,
                    lower_bound: 0.0,
                    learn_mod: 0.0,
                },
            );
            SkillTable {
                id: DataId(0x0E00_0004),
                buckets: 11,
                skills: m,
            }
        };
        let mut q = Qualities::new();
        q.set_skill(
            7,
            Skill {
                level_from_pp: 1,
                format_version: 1,
                sac: Sac::Trained as u32,
                pp: 150, // already 50 past the level-1 threshold of 100
                init_level: 0,
                resistance_of_last_check: 0,
                last_used_time: 0.0,
            },
        );
        // level 2 costs 300 total; 300 - 150 already sunk = 150.
        assert_eq!(skill_cost_to_raise(&q, &skills, &xp, 7), 150);
        // Ten levels would overshoot the table's max of 4, so n = 3: 1500 - 150.
        assert_eq!(skill_cost_to_raise_10(&q, &xp, 7), 1350);

        // An untrained skill's "cost" is the credit cost from the skill table.
        q.set_skill(
            7,
            Skill {
                sac: Sac::Untrained as u32,
                ..*q.skill(7).unwrap()
            },
        );
        assert_eq!(skill_cost_to_raise(&q, &skills, &xp, 7), 6);
        assert_eq!(skill_cost_to_raise_10(&q, &xp, 7), 0);

        // A capped skill costs nothing.
        q.set_skill(
            7,
            Skill {
                level_from_pp: 4,
                sac: Sac::Trained as u32,
                ..*q.skill(7).unwrap()
            },
        );
        assert_eq!(skill_cost_to_raise(&q, &skills, &xp, 7), 0);
    }

    /// Oracle: §7's `ConfirmationType` table.
    #[test]
    fn confirmation_types_have_the_documented_ordinals() {
        assert_eq!(
            ConfirmationType::from_raw(1),
            ConfirmationType::AllegianceSwear
        );
        assert_eq!(ConfirmationType::from_raw(2), ConfirmationType::AlterSkill);
        assert_eq!(
            ConfirmationType::from_raw(3),
            ConfirmationType::AlterAttribute
        );
        assert_eq!(
            ConfirmationType::from_raw(4),
            ConfirmationType::FellowshipRecruit
        );
        assert_eq!(
            ConfirmationType::from_raw(5),
            ConfirmationType::CraftInteraction
        );
        assert_eq!(
            ConfirmationType::from_raw(6),
            ConfirmationType::UseAugmentation
        );
        assert_eq!(ConfirmationType::from_raw(7), ConfirmationType::YesNo);
        assert_eq!(ConfirmationType::from_raw(99), ConfirmationType::Undef);
    }
}
