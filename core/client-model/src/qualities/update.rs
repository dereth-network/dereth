//! The twenty-six `Qualities_*Update*` events, decoded and applied.
//!
//! Source: `docs/networking/messages/03-qualities-and-updates.md` (the opcode table, the two body
//! shapes and the update-stat tag table), plus the client's sequence-gate behavior, represented
//! by [`PropertySequenceGate`].
//!
//! # Why this exists
//!
//! Every one of the twenty-six has to land, through the sequence gate, in a [`crate::StatValue`]
//! variant a skill or an attribute can take. Handling only the four `Attribute2nd` forms, as a
//! vitals-only path would, leaves the character panel showing the character's stats **as of
//! login, for ever**: a raise the server accepted would change no number on screen.
//!
//! # What one of these is
//!
//! Every update handler follows the same four steps for
//! all ten value types:
//!
//! ```text
//! obj = look up objectId;    // private form substitutes the current player id
//! if (!obj) return 0;
//! if (!update_timestamp(obj._stamper, prop | tag, seq)) return 0;    // stale
//! if (obj has qualities) set the typed quality (prop, val);
//! call the changed-quality handler with (obj, StatType, prop);
//! ```
//!
//! The gate runs **before** the setter and consumes the sequence whether or not the value could be
//! stored, which is why [`apply`] distinguishes [`Applied`](Outcome::Applied) from
//! [`Unstorable`](Outcome::Unstorable) rather than folding both into a boolean.
//!
//! # The one thing that is easy to get wrong
//!
//! *Skill*, *skill level* and *skill AC* share tag `0x00040000`, and *attribute* and *attribute
//! level* share `0x00080000`. So the three (respectively two) messages for one property share a
//! **single** 8-bit counter, and a decoder that gave each opcode its own key would silently accept
//! stale updates. [`decode`] therefore returns a [`StatKey`] built from the *tag*, not from the
//! opcode.

use crate::qualities::{PropertySequenceGate, StatKey, StatType, StatValue};
use crate::Qualities;
use dereth_primitives::{DataId, ObjectId};
use dereth_protocol::archive::Reader;
use dereth_protocol::{qualities as wire, Message, Opcode};

/// The twenty-six opcodes [`decode`] answers for, as a predicate a dispatcher can switch on.
///
/// Kept as its own function rather than folded into `decode` so the *routing* decision and the
/// *decoding* one can fail separately: an opcode this returns true for and `decode` then refuses
/// is a decode bug and is counted as one, where a dispatcher written as
/// `if decode(op, body).is_some()` would silently file it as "not ours".
#[must_use]
pub fn is_update_opcode(op: Opcode) -> bool {
    // `0x02CD`..=`0x02EA` is exactly the update block of the opcode table: ten types, each with a
    // private and a public form, plus the four extra partial forms for skills and attributes.
    // The removes are elsewhere (`0x01D1`..=`0x01DE`, `0x02B8`, `0x02B9`) and are
    // [`crate::qualities::remove::is_remove_opcode`]'s. The two predicates
    // partition the family and `remove`'s own test walks all 65,536 opcodes to prove it.
    (Opcode::QUALITIES_PRIVATE_UPDATE_INT.0..=Opcode::QUALITIES_UPDATE_ATTRIBUTE2ND_LEVEL.0)
        .contains(&op.0)
}

/// One decoded `Qualities_*Update*` event.
#[derive(Debug, Clone, PartialEq)]
pub struct QualityUpdate {
    /// The object the event names. `None` is the **private** form, whose subject is
    /// the current smart-box player id by construction and is never on the wire.
    pub subject: Option<ObjectId>,
    /// The 8-bit sequence consumed by the timestamp gate.
    pub sequence: u8,
    /// `(StatType << 16) | propertyId` — the counter, not the opcode.
    pub key: StatKey,
    pub value: StatValue,
}

/// What [`apply`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The timestamp gate rejected it: an older stamp under the wrap rule. Nothing was
    /// written and no change handler fired.
    Stale,
    /// The value was written.
    Applied,
    /// The gate passed — so the sequence **is** consumed — and the value had nowhere to land: one
    /// of the four partial forms naming a skill or attribute this object does not carry.
    Unstorable,
}

/// Decode one event body into its subject, sequence, key and value.
///
/// `body` is the message body **with its leading type dword still in place** — that is, exactly
/// what `dereth_client_net::client_session::SessionEvent::UiEvent` carries — because [`Message::read`] here is called
/// on a reader positioned after it. Returns `None` for an opcode that is not one of the
/// twenty-six, and for a body that will not decode.
///
/// The removes (`0x01D1`…`0x01DE`, `0x02B8`, `0x02B9`) are **not** here, and that is a statement
/// about retail and not only about this module: the per-type remove is a *different*
/// function from the per-type update — it carries no value, skips the
/// mirror entirely, and ends in the removed handler rather than
/// the changed handler. It lives in [`crate::qualities::remove`].
#[must_use]
pub fn decode(opcode: Opcode, body: &[u8]) -> Option<QualityUpdate> {
    macro_rules! private {
        ($t:ty, $tag:expr, $wrap:expr) => {{
            let mut r = Reader::new(body);
            let m = <$t as Message>::read(&mut r).ok()?;
            #[allow(clippy::redundant_closure_call)]
            Some(QualityUpdate {
                subject: None,
                sequence: m.0.sequence,
                key: StatKey::new($tag, m.0.property_id),
                value: ($wrap)(m.0.value),
            })
        }};
    }
    macro_rules! public {
        ($t:ty, $tag:expr, $wrap:expr) => {{
            let mut r = Reader::new(body);
            let m = <$t as Message>::read(&mut r).ok()?;
            #[allow(clippy::redundant_closure_call)]
            Some(QualityUpdate {
                subject: Some(m.0.object),
                sequence: m.0.sequence,
                key: StatKey::new($tag, m.0.property_id),
                value: ($wrap)(m.0.value),
            })
        }};
    }

    use StatType as T;
    match opcode {
        Opcode::QUALITIES_PRIVATE_UPDATE_INT => {
            private!(wire::QualitiesPrivateUpdateInt, T::Int, StatValue::Int)
        }
        Opcode::QUALITIES_UPDATE_INT => {
            public!(wire::QualitiesUpdateInt, T::Int, StatValue::Int)
        }
        Opcode::QUALITIES_PRIVATE_UPDATE_INT64 => {
            private!(
                wire::QualitiesPrivateUpdateInt64,
                T::Int64,
                StatValue::Int64
            )
        }
        Opcode::QUALITIES_UPDATE_INT64 => {
            public!(wire::QualitiesUpdateInt64, T::Int64, StatValue::Int64)
        }
        // The bool forms carry an `int32`, not a byte (see `0x02D1`).
        Opcode::QUALITIES_PRIVATE_UPDATE_BOOL => {
            private!(wire::QualitiesPrivateUpdateBool, T::Bool, |v: i32| {
                StatValue::Bool(v != 0)
            })
        }
        Opcode::QUALITIES_UPDATE_BOOL => {
            public!(
                wire::QualitiesUpdateBool,
                T::Bool,
                |v: i32| StatValue::Bool(v != 0)
            )
        }
        Opcode::QUALITIES_PRIVATE_UPDATE_FLOAT => {
            private!(
                wire::QualitiesPrivateUpdateFloat,
                T::Float,
                StatValue::Float
            )
        }
        Opcode::QUALITIES_UPDATE_FLOAT => {
            public!(wire::QualitiesUpdateFloat, T::Float, StatValue::Float)
        }
        Opcode::QUALITIES_PRIVATE_UPDATE_STRING => {
            private!(
                wire::QualitiesPrivateUpdateString,
                T::String,
                |v: wire::AlignedString| { StatValue::Str(v.0) }
            )
        }
        Opcode::QUALITIES_UPDATE_STRING => public!(
            wire::QualitiesUpdateString,
            T::String,
            |v: wire::AlignedString| StatValue::Str(v.0)
        ),
        Opcode::QUALITIES_PRIVATE_UPDATE_DATA_ID => {
            private!(wire::QualitiesPrivateUpdateDataId, T::Did, |v: u32| {
                StatValue::Did(DataId(v))
            })
        }
        Opcode::QUALITIES_UPDATE_DATA_ID => {
            public!(
                wire::QualitiesUpdateDataId,
                T::Did,
                |v: u32| StatValue::Did(DataId(v))
            )
        }
        Opcode::QUALITIES_PRIVATE_UPDATE_INSTANCE_ID => {
            private!(
                wire::QualitiesPrivateUpdateInstanceId,
                T::Iid,
                StatValue::Iid
            )
        }
        Opcode::QUALITIES_UPDATE_INSTANCE_ID => {
            public!(wire::QualitiesUpdateInstanceId, T::Iid, StatValue::Iid)
        }
        Opcode::QUALITIES_PRIVATE_UPDATE_POSITION => {
            private!(
                wire::QualitiesPrivateUpdatePosition,
                T::Position,
                StatValue::Position
            )
        }
        Opcode::QUALITIES_UPDATE_POSITION => {
            public!(
                wire::QualitiesUpdatePosition,
                T::Position,
                StatValue::Position
            )
        }
        // ---- the six skill forms, all on tag 4 -------------------------------------------------
        Opcode::QUALITIES_PRIVATE_UPDATE_SKILL => {
            private!(
                wire::QualitiesPrivateUpdateSkill,
                T::Skill,
                StatValue::Skill
            )
        }
        Opcode::QUALITIES_UPDATE_SKILL => {
            public!(wire::QualitiesUpdateSkill, T::Skill, StatValue::Skill)
        }
        Opcode::QUALITIES_PRIVATE_UPDATE_SKILL_LEVEL => {
            private!(
                wire::QualitiesPrivateUpdateSkillLevel,
                T::Skill,
                StatValue::SkillLevel
            )
        }
        Opcode::QUALITIES_UPDATE_SKILL_LEVEL => {
            public!(
                wire::QualitiesUpdateSkillLevel,
                T::Skill,
                StatValue::SkillLevel
            )
        }
        Opcode::QUALITIES_PRIVATE_UPDATE_SKILL_AC => private!(
            wire::QualitiesPrivateUpdateSkillAc,
            T::Skill,
            StatValue::SkillAdvancementClass
        ),
        Opcode::QUALITIES_UPDATE_SKILL_AC => {
            public!(
                wire::QualitiesUpdateSkillAc,
                T::Skill,
                StatValue::SkillAdvancementClass
            )
        }
        // ---- the four attribute forms, all on tag 8 ---------------------------------------------
        Opcode::QUALITIES_PRIVATE_UPDATE_ATTRIBUTE => {
            private!(
                wire::QualitiesPrivateUpdateAttribute,
                T::Attribute,
                StatValue::Attribute
            )
        }
        Opcode::QUALITIES_UPDATE_ATTRIBUTE => {
            public!(
                wire::QualitiesUpdateAttribute,
                T::Attribute,
                StatValue::Attribute
            )
        }
        Opcode::QUALITIES_PRIVATE_UPDATE_ATTRIBUTE_LEVEL => private!(
            wire::QualitiesPrivateUpdateAttributeLevel,
            T::Attribute,
            StatValue::AttributeLevel
        ),
        Opcode::QUALITIES_UPDATE_ATTRIBUTE_LEVEL => {
            public!(
                wire::QualitiesUpdateAttributeLevel,
                T::Attribute,
                StatValue::AttributeLevel
            )
        }
        // ---- the four vital forms, all on tag 9 -------------------------------------------------
        Opcode::QUALITIES_PRIVATE_UPDATE_ATTRIBUTE2ND => private!(
            wire::QualitiesPrivateUpdateAttribute2nd,
            T::Attribute2nd,
            StatValue::Attribute2nd
        ),
        Opcode::QUALITIES_UPDATE_ATTRIBUTE2ND => {
            public!(
                wire::QualitiesUpdateAttribute2nd,
                T::Attribute2nd,
                StatValue::Attribute2nd
            )
        }
        Opcode::QUALITIES_PRIVATE_UPDATE_ATTRIBUTE2ND_LEVEL => private!(
            wire::QualitiesPrivateUpdateAttribute2ndLevel,
            T::Attribute2nd,
            StatValue::Attribute2ndLevel
        ),
        Opcode::QUALITIES_UPDATE_ATTRIBUTE2ND_LEVEL => public!(
            wire::QualitiesUpdateAttribute2ndLevel,
            T::Attribute2nd,
            StatValue::Attribute2ndLevel
        ),
        _ => None,
    }
}

/// The client's per-type stat update: gate-then-write, against one quality set and its `PropertySequenceGate`.
///
/// The order is the client's: the stamp is taken first and is consumed even when the value cannot
/// be stored, so a server that sends `SkillLevel` for a skill the client does not carry burns the
/// sequence exactly as retail does. Reversing the two would let a later, storable update be
/// rejected as stale.
pub fn apply(q: &mut Qualities, stamper: &mut PropertySequenceGate, u: &QualityUpdate) -> Outcome {
    if !stamper.update(u.key.0, u.sequence) {
        return Outcome::Stale;
    }
    if q.set(u.key, u.value.clone()) {
        Outcome::Applied
    } else {
        Outcome::Unstorable
    }
}

/// True for a key that makes the stat-management panel redraw after a raise.
///
/// The stat panel's `0x10000004` arm clears
/// its awaiting-raise flag and re-runs the footer, and it has two
/// sources: the changed quality itself (`Qualities_UpdateSkill` and its attribute siblings) and
/// the int64 update for `AvailableExperience`. `AvailableSkillCredits` is here for the same reason:
/// it is the untrained footer's line two and the currency a `Train_TrainSkillAdvancementClass`
/// spends.
#[must_use]
pub fn answers_a_raise(key: StatKey) -> bool {
    /// `PropertyInt64::AvailableExperience`.
    const AVAILABLE_EXPERIENCE: u32 = 2;
    /// `PropertyInt64::TotalExperience` — the header's own number.
    const TOTAL_EXPERIENCE: u32 = 1;
    /// `PropertyInt::AvailableSkillCredits`.
    const AVAILABLE_SKILL_CREDITS: u32 = 0x18;
    /// `PropertyInt::Level`.
    const LEVEL: u32 = 0x19;
    match key.stat_type() {
        StatType::Skill | StatType::Attribute => true,
        StatType::Int64 => matches!(key.property(), AVAILABLE_EXPERIENCE | TOTAL_EXPERIENCE),
        StatType::Int => matches!(key.property(), AVAILABLE_SKILL_CREDITS | LEVEL),
        _ => false,
    }
}

/// True for a **decoded update** the stat-management panel redraws itself for.
///
/// [`answers_a_raise`] is keyed on the [`StatKey`] alone, and that is not enough for the vitals.
/// `Attribute2nd` (`0x02E7` / `0x02E8`, the whole `SecondaryAttribute` record) and
/// `Attribute2ndLevel` (`0x02E9` / `0x02EA`, the current level alone) share tag 9, so a predicate
/// that can only see the key must either take both or refuse both. Taking both is wrong:
/// `0x02E9` is a regeneration tick and by far the commonest quality event on the
/// wire, and re-running the footer several times a second is not what the stat panel's
/// `0x10000004` arm does.
///
/// Refusing both is wrong too. `Train_TrainAttribute2nd (0x0044)`'s
/// answer *is* `Qualities_PrivateUpdateAttribute2nd (0x02E7)` — the whole-record form — so under
/// the key-only predicate a vital raise's answer never reaches the arm, the awaiting-raise flag never
/// clears and the raise button stays in state `0x0D` for the rest of the session. Across all seven
/// captures the corpus carries **four** `0x02E7`, and every one of them is the answer to a
/// `0x0044`; the 427 regeneration ticks are all `0x02E9`. The *value variant* is what tells them
/// apart, which is why this takes the update and not the key.
///
/// [`answers_a_raise`] is kept and unchanged: it is the key-only question, and it is still the
/// right one for a caller that only has a key.
#[must_use]
pub fn answers_a_raise_update(u: &QualityUpdate) -> bool {
    answers_a_raise(u.key) || matches!(u.value, StatValue::Attribute2nd(_))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_protocol::types::qualities::{Attribute, SecondaryAttribute, Skill};

    /// The body `dereth_client_net::client_session` hands over: the type dword, then the message.
    fn blob<M: Message>(m: &M) -> Vec<u8> {
        let mut w = dereth_protocol::archive::Writer::new();
        w.u32(M::OPCODE.0);
        m.write(&mut w).expect("encode");
        w.into_inner()
    }

    fn body_of(b: &[u8]) -> &[u8] {
        &b[4..]
    }

    /// Oracle: `docs/networking/messages/03-qualities-and-updates.md`'s update-stat tag table. Skill,
    /// skill level and skill AC share tag `0x00040000`, while attribute and attribute level share
    /// `0x00080000`; the three (respectively two) messages for one skill or attribute therefore
    /// share a **single** sequence counter.
    ///
    /// This is the test that would catch a decoder giving each opcode its own key, which is the
    /// natural mistake and is invisible until two forms of the same property interleave.
    #[test]
    fn the_six_skill_forms_share_one_counter_and_the_four_vital_forms_share_another() {
        let s = Skill {
            level_from_pp: 3,
            format_version: 1,
            sac: 2,
            pp: 100,
            ..Skill::default()
        };
        let want = StatKey::new(StatType::Skill, 7);
        for u in [
            decode(
                Opcode::QUALITIES_PRIVATE_UPDATE_SKILL,
                body_of(&blob(&wire::QualitiesPrivateUpdateSkill(
                    wire::PrivateUpdate {
                        sequence: 0,
                        property_id: 7,
                        value: s,
                    },
                ))),
            ),
            decode(
                Opcode::QUALITIES_PRIVATE_UPDATE_SKILL_LEVEL,
                body_of(&blob(&wire::QualitiesPrivateUpdateSkillLevel(
                    wire::PrivateUpdate {
                        sequence: 0,
                        property_id: 7,
                        value: 3u32,
                    },
                ))),
            ),
            decode(
                Opcode::QUALITIES_PRIVATE_UPDATE_SKILL_AC,
                body_of(&blob(&wire::QualitiesPrivateUpdateSkillAc(
                    wire::PrivateUpdate {
                        sequence: 0,
                        property_id: 7,
                        value: 2u32,
                    },
                ))),
            ),
        ] {
            assert_eq!(u.expect("decoded").key, want);
        }
        assert_eq!(
            want.0, 0x0004_0007,
            "tag 4 in the high half, property in the low"
        );

        let v = SecondaryAttribute {
            attribute: Attribute::default(),
            current_level: 50,
        };
        let want = StatKey::new(StatType::Attribute2nd, 2);
        for u in [
            decode(
                Opcode::QUALITIES_PRIVATE_UPDATE_ATTRIBUTE2ND,
                body_of(&blob(&wire::QualitiesPrivateUpdateAttribute2nd(
                    wire::PrivateUpdate {
                        sequence: 0,
                        property_id: 2,
                        value: v,
                    },
                ))),
            ),
            decode(
                Opcode::QUALITIES_PRIVATE_UPDATE_ATTRIBUTE2ND_LEVEL,
                body_of(&blob(&wire::QualitiesPrivateUpdateAttribute2ndLevel(
                    wire::PrivateUpdate {
                        sequence: 0,
                        property_id: 2,
                        value: 50u32,
                    },
                ))),
            ),
        ] {
            assert_eq!(u.expect("decoded").key, want);
        }
        assert_eq!(want.0, 0x0009_0002);
    }

    /// Oracle: the two body shapes of `docs/networking/messages/03-qualities-and-updates.md` §1 —
    /// private `[u32 type][u8 seq][u32 property][value…]`, public
    /// `[u32 type][u8 seq][u32 object][u32 property][value…]`. The private form carries no subject
    /// at all, so a rebuild
    /// that read one out of it would be reading the property id.
    #[test]
    fn the_private_form_has_no_subject_and_the_public_form_does() {
        let s = Skill {
            level_from_pp: 1,
            format_version: 1,
            sac: 2,
            pp: 5,
            ..Skill::default()
        };
        let priv_ = decode(
            Opcode::QUALITIES_PRIVATE_UPDATE_SKILL,
            body_of(&blob(&wire::QualitiesPrivateUpdateSkill(
                wire::PrivateUpdate {
                    sequence: 9,
                    property_id: 0x2C,
                    value: s,
                },
            ))),
        )
        .expect("decoded");
        assert_eq!(priv_.subject, None);
        assert_eq!(priv_.sequence, 9);
        assert_eq!(priv_.key.property(), 0x2C);

        let pub_ = decode(
            Opcode::QUALITIES_UPDATE_SKILL,
            body_of(&blob(&wire::QualitiesUpdateSkill(wire::PublicUpdate {
                sequence: 9,
                object: ObjectId(0x5000_1234),
                property_id: 0x2C,
                value: s,
            }))),
        )
        .expect("decoded");
        assert_eq!(pub_.subject, Some(ObjectId(0x5000_1234)));
        assert_eq!(pub_.key, priv_.key, "the same counter either way");
    }

    /// Oracle: the per-type stat update — the gate runs **before** the setter and is consumed
    /// whether or not the value could be stored.
    #[test]
    fn a_stale_stamp_is_rejected_before_the_setter_and_an_unstorable_one_still_burns_it() {
        let mut q = Qualities::new();
        let mut st = PropertySequenceGate::new();
        let key = StatKey::new(StatType::Skill, 7);
        let s = Skill {
            level_from_pp: 3,
            format_version: 1,
            sac: 2,
            pp: 300,
            ..Skill::default()
        };

        let up = |seq: u8, level: u32| QualityUpdate {
            subject: None,
            sequence: seq,
            key,
            value: StatValue::SkillLevel(level),
        };

        // Nothing under id 7 yet: the gate passes, the setter cannot land it.
        assert_eq!(apply(&mut q, &mut st, &up(0, 4)), Outcome::Unstorable);
        assert_eq!(
            st.stamp(key.0),
            Some(0),
            "and the sequence was consumed anyway"
        );

        q.set_skill(7, s);
        // Sequence 0 again is "not older", so it applies; the client's rule is not-older, not newer.
        assert_eq!(apply(&mut q, &mut st, &up(0, 4)), Outcome::Applied);
        assert_eq!(q.skill(7).expect("skill").level_from_pp, 4);

        assert_eq!(apply(&mut q, &mut st, &up(1, 5)), Outcome::Applied);
        assert_eq!(q.skill(7).expect("skill").level_from_pp, 5);
        // 0 after 1 is one step backwards under the half-range window.
        assert_eq!(apply(&mut q, &mut st, &up(0, 99)), Outcome::Stale);
        assert_eq!(
            q.skill(7).expect("skill").level_from_pp,
            5,
            "a stale update must not reach the setter"
        );
    }

    /// Oracle: the four partial setters' contract — there is no record to hang a bare level on.
    #[test]
    fn the_partial_forms_need_a_record_and_the_whole_forms_create_one() {
        let mut q = Qualities::new();
        assert!(!q.set_skill_level(7, 3));
        assert!(!q.set_skill_advancement_class(7, 2));
        assert!(!q.set_attribute_level(1, 40));
        assert!(!q.set_attribute_2nd_level(2, 40));

        assert!(q.set_attribute(
            1,
            Attribute {
                level_from_cp: 5,
                init_level: 10,
                cp_spent: 55
            }
        ));
        assert!(q.set_attribute_level(1, 6));
        assert_eq!(q.attribute(1).expect("strength").level_from_cp, 6);
        assert_eq!(
            q.attribute(1).expect("strength").init_level,
            10,
            "the rest is untouched"
        );
        // The presence bit is set with the slot, or unpacking would not expose it.
        assert_eq!(
            q.attributes.as_ref().expect("cache").flags
                & dereth_protocol::types::qualities::attribute_cache_mask::STRENGTH,
            dereth_protocol::types::qualities::attribute_cache_mask::STRENGTH
        );
        assert_eq!(q.attribute(7), None, "there is no seventh attribute");
    }

    /// **Every one of the seven new [`StatValue`] variants reaches its own setter, and says
    /// truthfully whether it landed.**
    ///
    /// `Qualities::set` is a dispatch table, and a mis-wired arm — `AttributeLevel` routed to
    /// `set_attribute_2nd_level`, say, or a partial form reporting success when it wrote nothing —
    /// is the invisible kind of wrong: the counter says "applied", the sequence is consumed, and
    /// the number on screen is somebody else's. So each arm is driven through `set` (not through
    /// the setter it should reach), on a `Qualities` where exactly one record exists, and both the
    /// return value and the field written are checked.
    ///
    /// This test exists because a mutation that made `StatValue::AttributeLevel` a no-op returning
    /// `true` **survived** the first run: the partial setters were covered directly and the table
    /// that reaches them was not.
    #[test]
    fn every_new_stat_value_routes_to_its_own_setter_and_reports_honestly() {
        use crate::qualities::{StatKey, StatType};
        let skill_key = StatKey::new(StatType::Skill, 7);
        let attr_key = StatKey::new(StatType::Attribute, 1);
        let vital_key = StatKey::new(StatType::Attribute2nd, 2);

        // Nothing exists yet: the three whole-record forms create, the four partial forms refuse.
        let mut q = Qualities::new();
        assert!(!q.set(skill_key, StatValue::SkillLevel(4)));
        assert!(!q.set(skill_key, StatValue::SkillAdvancementClass(2)));
        assert!(!q.set(attr_key, StatValue::AttributeLevel(4)));
        assert!(!q.set(vital_key, StatValue::Attribute2ndLevel(4)));
        assert!(q.skill(7).is_none() && q.attribute(1).is_none() && q.attribute_2nd(2).is_none());

        let s = Skill {
            level_from_pp: 1,
            format_version: 1,
            sac: 2,
            pp: 100,
            ..Skill::default()
        };
        let a = Attribute {
            level_from_cp: 1,
            init_level: 10,
            cp_spent: 20,
        };
        let v = SecondaryAttribute {
            attribute: a,
            current_level: 33,
        };
        assert!(q.set(skill_key, StatValue::Skill(s)));
        assert!(q.set(attr_key, StatValue::Attribute(a)));
        assert!(q.set(vital_key, StatValue::Attribute2nd(v)));
        assert_eq!(q.skill(7).copied(), Some(s));
        assert_eq!(q.attribute(1), Some(a));
        assert_eq!(q.attribute_2nd(2), Some(v));

        // Now each partial form lands, and lands on **its own** field and nothing else.
        assert!(q.set(skill_key, StatValue::SkillLevel(4)));
        assert_eq!(q.skill(7).expect("skill").level_from_pp, 4);
        assert_eq!(
            q.skill(7).expect("skill").pp,
            100,
            "a level does not move the experience"
        );
        assert!(q.set(skill_key, StatValue::SkillAdvancementClass(3)));
        assert_eq!(q.skill(7).expect("skill").sac, 3);
        assert_eq!(
            q.skill(7).expect("skill").level_from_pp,
            4,
            "and the AC does not move the level"
        );

        assert!(q.set(attr_key, StatValue::AttributeLevel(9)));
        assert_eq!(q.attribute(1).expect("strength").level_from_cp, 9);
        assert_eq!(
            q.attribute(1).expect("strength").cp_spent,
            20,
            "the rest is untouched"
        );
        assert_eq!(
            q.attribute_2nd(2).expect("health").attribute.level_from_cp,
            1,
            "an *attribute* level must not reach the vital that shares its property number"
        );

        assert!(q.set(vital_key, StatValue::Attribute2ndLevel(7)));
        assert_eq!(q.attribute_2nd(2).expect("health").current_level, 7);
        assert_eq!(
            q.attribute(1).expect("strength").level_from_cp,
            9,
            "and a *vital* level must not reach the attribute"
        );

        // And `get` reads each of the three records back as its whole record.
        assert_eq!(
            q.get(skill_key),
            Some(StatValue::Skill(*q.skill(7).expect("skill")))
        );
        assert_eq!(
            q.get(attr_key),
            Some(StatValue::Attribute(q.attribute(1).expect("a")))
        );
        assert_eq!(
            q.get(vital_key),
            Some(StatValue::Attribute2nd(q.attribute_2nd(2).expect("v")))
        );
    }

    /// The raise answer is the changed quality or the experience total.
    #[test]
    fn the_raise_answer_is_the_changed_quality_or_the_experience_total() {
        assert!(answers_a_raise(StatKey::new(StatType::Skill, 7)));
        assert!(answers_a_raise(StatKey::new(StatType::Attribute, 1)));
        assert!(
            answers_a_raise(StatKey::new(StatType::Int64, 2)),
            "AvailableExperience"
        );
        assert!(
            answers_a_raise(StatKey::new(StatType::Int, 0x18)),
            "AvailableSkillCredits"
        );
        // A vital tick is not an answer to a raise — 55 of early-inventory-and-casting's 65 quality
        // events are these, and treating them as one would re-run the footer several times a
        // second.
        assert!(!answers_a_raise(StatKey::new(StatType::Attribute2nd, 2)));
        assert!(
            !answers_a_raise(StatKey::new(StatType::Int, 12)),
            "StackSize"
        );
        assert!(!answers_a_raise(StatKey::new(StatType::Int64, 7)));
    }

    /// A vital raise answer is the whole record and a vital tick is not.
    #[test]
    fn a_vital_raise_answer_is_the_whole_record_and_a_vital_tick_is_not() {
        let key = StatKey::new(StatType::Attribute2nd, 1);
        let raise = QualityUpdate {
            subject: None,
            sequence: 0,
            key,
            value: StatValue::Attribute2nd(SecondaryAttribute {
                attribute: Attribute {
                    level_from_cp: 1,
                    init_level: 0,
                    cp_spent: 73,
                },
                current_level: 15,
            }),
        };
        let tick = QualityUpdate {
            subject: None,
            sequence: 0,
            key: StatKey::new(StatType::Attribute2nd, 2),
            value: StatValue::Attribute2ndLevel(15),
        };
        assert!(
            answers_a_raise_update(&raise),
            "0x02E7 is the answer to a 0x0044"
        );
        assert!(
            !answers_a_raise_update(&tick),
            "0x02E9 is a regeneration tick"
        );
        // The key-only predicate cannot make that distinction, which is why it is not asked to.
        assert!(!answers_a_raise(raise.key));
        assert!(!answers_a_raise(tick.key));
        // And the forms that were already right are unchanged through the new door.
        for (t, p) in [
            (StatType::Skill, 7u32),
            (StatType::Attribute, 1),
            (StatType::Int64, 2),
        ] {
            let u = QualityUpdate {
                subject: None,
                sequence: 0,
                key: StatKey::new(t, p),
                value: StatValue::Int64(0),
            };
            assert!(answers_a_raise_update(&u), "{t:?}/{p}");
        }
    }

    /// An opcode outside the twenty-six, and a body too short for its value, both answer `None`
    /// rather than a default-filled update.
    #[test]
    fn an_unknown_opcode_and_a_truncated_body_decode_to_nothing() {
        assert_eq!(decode(Opcode::COMMUNICATION_TEXTBOX_STRING, &[0; 32]), None);
        assert_eq!(
            decode(Opcode::QUALITIES_PRIVATE_UPDATE_SKILL, &[0, 1, 2]),
            None
        );
        // The right opcode with a body one byte short of a `Skill` is still nothing.
        let full = blob(&wire::QualitiesPrivateUpdateSkill(wire::PrivateUpdate {
            sequence: 0,
            property_id: 7,
            value: Skill::default(),
        }));
        let body = body_of(&full);
        assert!(decode(Opcode::QUALITIES_PRIVATE_UPDATE_SKILL, body).is_some());
        assert_eq!(
            decode(
                Opcode::QUALITIES_PRIVATE_UPDATE_SKILL,
                &body[..body.len() - 1]
            ),
            None
        );
    }
}
