//! The sixteen `Qualities_*Remove*Event` messages, decoded and applied.
//!
//! Source: `docs/networking/messages/03-qualities-and-updates.md` has the opcode table and the two
//! body shapes; the behaviour below is the retail client's.
//!
//! # What one of these is
//!
//! There are **eight** remove handlers, not ten — Int, Int64, Bool, Float, String,
//! DataID, InstanceID, Position, exactly the eight generic quality tables. **There
//! is no remove for skills, attributes or vitals**, in the client or on the wire:
//! attributes, vitals and skills can only be updated. That is why the sixteen opcodes are eight
//! pairs and not thirteen.
//!
//! The integer remove, in full; the other seven differ only in the tag, the setter and the
//! `StatType` argument:
//!
//! ```text
//!   obj = look up the world object by objectId;   if (!obj) return 0
//!   stamper = the object's timestamp stamper;     if (!stamper) return 0
//!   key = prop | (StatType << 16)
//!   update the timestamp for this key;            if (stale) return 0
//!   if (the object has qualities)                 // a NULL TEST, not an allocation
//!       remove the integer quality                // return value IGNORED
//!   call the removed-quality handler with (obj, the integer stat type, prop)
//! ```
//!
//! The eight tags: Int `0x10000`, Int64 `0xE0000`,
//! Bool `0xD0000`, Float `0x20000`, String `0x50000`, DataID `0x60000`, InstanceID `0x70000`,
//! Position `0x30000`. They are the *same* keys the update path uses, which is the whole point
//! of [`crate::qualities::StatKey`]: **a remove and an update of one property share one 8-bit
//! counter**, so a remove that arrives out of order is dropped by the same gate.
//!
//! # The two differences from the update path, and both are load-bearing
//!
//! Set the integer update beside the integer remove and they are the
//! same function up to the qualities check. After the setter they diverge:
//!
//! | | the update path | the remove path |
//! |---|---|---|
//! | the store | set integer quality | remove integer quality |
//! | the `PublicWeenieDesc` mirror | refresh the mirrored stat | **absent** |
//! | the fan-out | the changed handler | the removed handler |
//!
//! **The mirror does not run on a remove.** There is no call to the mirror anywhere in the eight
//! handlers; a remove goes straight from the setter to the quality registrar. So a removed `ItemType` or
//! `Burden` leaves the `PublicWeenieDesc` copy exactly as it was, and this module must not run
//! `mirror_stat_update` — that would be this client inventing behaviour.
//!
//! # What "remove" does to the cached quality: it *deletes the key*
//!
//! ```text
//! remove integer quality
//!     NULL table -> return 0, nothing else
//!     remove the key from the packable hash table
//!
//! query integer quality
//!     NULL table -> miss
//!     look up the quality in the packed hash table
//!     MISS: return 0 and DO NOT TOUCH the out parameter
//!     HIT:  *out = value; return 1
//! ```
//!
//! So after a remove the integer query answers **false** and leaves the caller's pre-seeded default in
//! place. It does *not* answer zero. Every caller in the client pre-seeds: the vendor panel writes
//! its total value as 0 first, while the ready-position check pre-seeds `INVALID_DID`. Thus "absent"
//! and "the default" are the same answer to a *reader*, and
//! a *different* answer to the table. [`crate::Qualities::get`] is the reader that can tell them
//! apart, and it is what this module's tests assert on.
//!
//! Note also what the integer remove does **not** do: it never frees the table. A table emptied by
//! removes stays allocated, which is why [`crate::Qualities::remove`] leaves the `Option` alone.
//!
//! # The private forms
//!
//! The private remove handler is the public handler with the object id
//! substituted, exactly as the update pair is:
//!
//! ```text
//!   id = the smart box's player id, or 0 if there is no smart box
//!   the integer remove (id, ...)
//! ```
//!
//! and the public one forwards the wire's own object id to the same remove. One store, two
//! addressings — the same shape as the updates.
//!
//! # This family is absent from the observed shard traffic, and that is a fact worth pinning
//!
//! ACE has **no sender** for any of the sixteen: ACE's `Source` carries the sixteen names
//! in `ACE.Entity/PacketOpCodeNames.cs` and nowhere else, and
//! `Network/GameMessages/Messages/` has `GameMessagePrivateUpdateProperty{Bool,Float,Int,Int64,
//! String}` and their `Public` twins with no `Remove` counterpart. A scan of all twelve promoted
//! `fixtures/message-corpus` scenarios (13,535 blobs) finds **zero** of the sixteen against **1,443**
//! `0x02CD`..`0x02EA` updates in the same pass — the instrument was pointed at the target.
//!
//! So the receivers are here for **parity with retail**, which has all sixteen, and the rejecting
//! test synthesises the bodies.

use crate::qualities::{PropertySequenceGate, StatKey, StatType};
use crate::Qualities;
use dereth_primitives::ObjectId;
use dereth_protocol::archive::Reader;
use dereth_protocol::{qualities as wire, Message, Opcode};

/// The sixteen opcodes [`decode`] answers for, as a predicate a dispatcher can switch on.
///
/// The same split-routing-from-decoding reason [`crate::qualities::update::is_update_opcode`]
/// gives: an opcode this accepts and `decode` then refuses is a decode bug and is counted as one.
///
/// The block is **not** contiguous. `0x01D1`..=`0x01DE` is fourteen — the seven pairs that shipped
/// with the original protocol — and the Int64 pair was added later at `0x02B8` / `0x02B9`, six
/// opcodes below the update block. A range test alone would silently drop the two newest.
#[must_use]
pub fn is_remove_opcode(op: Opcode) -> bool {
    (Opcode::QUALITIES_PRIVATE_REMOVE_INT_EVENT.0..=Opcode::QUALITIES_REMOVE_POSITION_EVENT.0)
        .contains(&op.0)
        || op == Opcode::QUALITIES_PRIVATE_REMOVE_INT64_EVENT
        || op == Opcode::QUALITIES_REMOVE_INT64_EVENT
}

/// One decoded `Qualities_*Remove*Event`.
///
/// There is no value: `PrivateRemove` is `[u8 seq][u32 property]` and `PublicRemove` is
/// `[u8 seq][u32 object][u32 property]`, nine and thirteen bytes with the opcode dword.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QualityRemove {
    /// The object the event names. `None` is the **private** form, whose subject is
    /// the current smart-box player id by construction and is never on the wire.
    pub subject: Option<ObjectId>,
    /// The 8-bit sequence gates on — **the same counter the matching
    /// update opcode consumes**, because the key is built from the tag and not from the opcode.
    pub sequence: u8,
    /// `(StatType << 16) | propertyId`.
    pub key: StatKey,
}

/// What [`apply`] did.
///
/// Retail cannot tell [`Removed`](Outcome::Removed) from [`Absent`](Outcome::Absent): the remove
/// **ignores** the table removal's return value and calls the removed-quality handler either way
/// (the ignored return falls through to the fan-out). The two are separated here because a test can use the
/// difference and a caller must not — every caller in this workspace treats them alike, which is
/// what keeps the transcription honest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The timestamp gate rejected it: an older stamp under the wrap rule. Nothing was
    /// deleted and no remove handler fired.
    Stale,
    /// The gate passed and a key was deleted.
    Removed,
    /// The gate passed — so the sequence **is** consumed — and there was nothing under that key:
    /// a NULL table, or a property this object was not carrying. The removed-quality handler
    /// still runs.
    Absent,
}

impl Outcome {
    /// True for the two outcomes that consumed the sequence and fired the removed-quality handler.
    #[must_use]
    pub fn fired(self) -> bool {
        matches!(self, Self::Removed | Self::Absent)
    }
}

/// Decode one event body into its subject, sequence and key.
///
/// `body` is the message body **after** the leading type dword — the same slice
/// [`crate::qualities::update::decode`] takes, which is `blob[4..]` at both production dispatch
/// sites. Returns `None` for an opcode that is not one of the sixteen, and for a body that will
/// not decode.
#[must_use]
pub fn decode(opcode: Opcode, body: &[u8]) -> Option<QualityRemove> {
    macro_rules! private {
        ($t:ty, $tag:expr) => {{
            let mut r = Reader::new(body);
            let m = <$t as Message>::read(&mut r).ok()?;
            Some(QualityRemove {
                subject: None,
                sequence: m.0.sequence,
                key: StatKey::new($tag, m.0.property_id),
            })
        }};
    }
    macro_rules! public {
        ($t:ty, $tag:expr) => {{
            let mut r = Reader::new(body);
            let m = <$t as Message>::read(&mut r).ok()?;
            Some(QualityRemove {
                subject: Some(m.0.object),
                sequence: m.0.sequence,
                key: StatKey::new($tag, m.0.property_id),
            })
        }};
    }

    use StatType as T;
    match opcode {
        // `0x01D1` / `0x01D2` — the integer-quality remove, tag `0x10000`.
        Opcode::QUALITIES_PRIVATE_REMOVE_INT_EVENT => {
            private!(wire::QualitiesPrivateRemoveInt, T::Int)
        }
        Opcode::QUALITIES_REMOVE_INT_EVENT => public!(wire::QualitiesRemoveInt, T::Int),
        // `0x01D3` / `0x01D4` — the boolean-quality remove, tag `0xD0000`.
        Opcode::QUALITIES_PRIVATE_REMOVE_BOOL_EVENT => {
            private!(wire::QualitiesPrivateRemoveBool, T::Bool)
        }
        Opcode::QUALITIES_REMOVE_BOOL_EVENT => public!(wire::QualitiesRemoveBool, T::Bool),
        // `0x01D5` / `0x01D6` — the float-quality remove, tag `0x20000`.
        Opcode::QUALITIES_PRIVATE_REMOVE_FLOAT_EVENT => {
            private!(wire::QualitiesPrivateRemoveFloat, T::Float)
        }
        Opcode::QUALITIES_REMOVE_FLOAT_EVENT => public!(wire::QualitiesRemoveFloat, T::Float),
        // `0x01D7` / `0x01D8` — the string-quality remove, tag `0x50000`.
        Opcode::QUALITIES_PRIVATE_REMOVE_STRING_EVENT => {
            private!(wire::QualitiesPrivateRemoveString, T::String)
        }
        Opcode::QUALITIES_REMOVE_STRING_EVENT => public!(wire::QualitiesRemoveString, T::String),
        // `0x01D9` / `0x01DA` — the data-id-quality remove, tag `0x60000`.
        Opcode::QUALITIES_PRIVATE_REMOVE_DATA_IDEVENT => {
            private!(wire::QualitiesPrivateRemoveDataId, T::Did)
        }
        Opcode::QUALITIES_REMOVE_DATA_IDEVENT => public!(wire::QualitiesRemoveDataId, T::Did),
        // `0x01DB` / `0x01DC` — the instance-id-quality remove, tag `0x70000`.
        Opcode::QUALITIES_PRIVATE_REMOVE_INSTANCE_IDEVENT => {
            private!(wire::QualitiesPrivateRemoveInstanceId, T::Iid)
        }
        Opcode::QUALITIES_REMOVE_INSTANCE_IDEVENT => {
            public!(wire::QualitiesRemoveInstanceId, T::Iid)
        }
        // `0x01DD` / `0x01DE` — the position-quality remove, tag `0x30000`.
        Opcode::QUALITIES_PRIVATE_REMOVE_POSITION_EVENT => {
            private!(wire::QualitiesPrivateRemovePosition, T::Position)
        }
        Opcode::QUALITIES_REMOVE_POSITION_EVENT => {
            public!(wire::QualitiesRemovePosition, T::Position)
        }
        // `0x02B8` / `0x02B9` — the 64-bit-integer-quality remove, tag `0xE0000`. The
        // late pair, six opcodes below the update block rather than beside its thirteen siblings.
        Opcode::QUALITIES_PRIVATE_REMOVE_INT64_EVENT => {
            private!(wire::QualitiesPrivateRemoveInt64, T::Int64)
        }
        Opcode::QUALITIES_REMOVE_INT64_EVENT => public!(wire::QualitiesRemoveInt64, T::Int64),
        _ => None,
    }
}

/// The client's remove, gate then delete, against one quality set and its `PropertySequenceGate`.
///
/// The order is the client's and it is the same order [`crate::qualities::update::apply`] uses:
/// the stamp is taken first and is consumed whether or not a key was there to delete, so a remove
/// for a property this object never carried still burns the sequence exactly as retail does.
/// Reversing the two would let a later, real update be rejected as stale.
pub fn apply(q: &mut Qualities, stamper: &mut PropertySequenceGate, r: &QualityRemove) -> Outcome {
    if !stamper.update(r.key.0, r.sequence) {
        return Outcome::Stale;
    }
    if q.remove(r.key) {
        Outcome::Removed
    } else {
        Outcome::Absent
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qualities::{PropertySequenceGate, StatValue};
    use dereth_protocol::types::qualities::Skill;

    /// The body a dispatcher hands over: `blob[4..]`, i.e. everything after the type dword.
    fn body<M: Message>(m: &M) -> Vec<u8> {
        dereth_protocol::write_body(m).expect("encode")
    }

    fn priv_int(seq: u8, prop: u32) -> Vec<u8> {
        body(&wire::QualitiesPrivateRemoveInt(wire::PrivateRemove {
            sequence: seq,
            property_id: prop,
        }))
    }

    /// Oracle: the eight tags, one per remove handler — Int
    /// `0x10000`, Int64 `0xE0000`, Bool `0xD0000`, Float `0x20000`, String `0x50000`,
    /// DataID `0x60000`, InstanceID `0x70000`, Position `0x30000`.
    ///
    /// This is the test that catches a decoder keying the gate off the opcode instead of the tag,
    /// which is invisible until a remove and an update of the same property interleave.
    #[test]
    fn all_sixteen_decode_and_carry_the_tag_their_remove_stat_template_ors_in() {
        let want: [(Opcode, StatType, bool); 16] = [
            (
                Opcode::QUALITIES_PRIVATE_REMOVE_INT_EVENT,
                StatType::Int,
                false,
            ),
            (Opcode::QUALITIES_REMOVE_INT_EVENT, StatType::Int, true),
            (
                Opcode::QUALITIES_PRIVATE_REMOVE_BOOL_EVENT,
                StatType::Bool,
                false,
            ),
            (Opcode::QUALITIES_REMOVE_BOOL_EVENT, StatType::Bool, true),
            (
                Opcode::QUALITIES_PRIVATE_REMOVE_FLOAT_EVENT,
                StatType::Float,
                false,
            ),
            (Opcode::QUALITIES_REMOVE_FLOAT_EVENT, StatType::Float, true),
            (
                Opcode::QUALITIES_PRIVATE_REMOVE_STRING_EVENT,
                StatType::String,
                false,
            ),
            (
                Opcode::QUALITIES_REMOVE_STRING_EVENT,
                StatType::String,
                true,
            ),
            (
                Opcode::QUALITIES_PRIVATE_REMOVE_DATA_IDEVENT,
                StatType::Did,
                false,
            ),
            (Opcode::QUALITIES_REMOVE_DATA_IDEVENT, StatType::Did, true),
            (
                Opcode::QUALITIES_PRIVATE_REMOVE_INSTANCE_IDEVENT,
                StatType::Iid,
                false,
            ),
            (
                Opcode::QUALITIES_REMOVE_INSTANCE_IDEVENT,
                StatType::Iid,
                true,
            ),
            (
                Opcode::QUALITIES_PRIVATE_REMOVE_POSITION_EVENT,
                StatType::Position,
                false,
            ),
            (
                Opcode::QUALITIES_REMOVE_POSITION_EVENT,
                StatType::Position,
                true,
            ),
            (
                Opcode::QUALITIES_PRIVATE_REMOVE_INT64_EVENT,
                StatType::Int64,
                false,
            ),
            (Opcode::QUALITIES_REMOVE_INT64_EVENT, StatType::Int64, true),
        ];
        assert_eq!(
            want.len(),
            16,
            "eight tables, a private and a public form each"
        );

        let object = ObjectId(0x8000_1234);
        for (op, tag, public) in want {
            assert!(is_remove_opcode(op), "{:#06X} is one of the sixteen", op.0);
            let bytes = if public {
                body(&wire::QualitiesRemoveInt(wire::PublicRemove {
                    sequence: 7,
                    object,
                    property_id: 0x2C,
                }))
            } else {
                priv_int(7, 0x2C)
            };
            let d = decode(op, &bytes).unwrap_or_else(|| panic!("{:#06X} decodes", op.0));
            assert_eq!(
                d.key,
                StatKey::new(tag, 0x2C),
                "{:#06X} keys off the tag",
                op.0
            );
            assert_eq!(d.key.0 >> 16, tag.raw(), "the tag is in the high half");
            assert_eq!(d.sequence, 7);
            assert_eq!(
                d.subject,
                if public { Some(object) } else { None },
                "{:#06X}: the private form carries no subject and the public form does",
                op.0
            );
        }
    }

    /// The update block and the remove block do not overlap, and neither predicate answers for the
    /// other's opcodes. Without this the two dispatch arms could both take a message, or neither.
    #[test]
    fn the_two_predicates_partition_the_family() {
        for raw in 0x0000u32..=0xFFFF {
            let op = Opcode(raw);
            assert!(
                !(is_remove_opcode(op) && crate::qualities::update::is_update_opcode(op)),
                "{raw:#06X} cannot be both"
            );
        }
        let mut n = 0;
        for raw in 0x0000u32..=0xFFFF {
            if is_remove_opcode(Opcode(raw)) {
                n += 1;
            }
        }
        assert_eq!(
            n, 16,
            "the predicate accepts exactly the sixteen and nothing else"
        );
        // The late Int64 pair is the one a range test loses: it sits *below* the update block.
        assert!(is_remove_opcode(Opcode(0x02B8)) && is_remove_opcode(Opcode(0x02B9)));
        assert!(
            !is_remove_opcode(Opcode(0x02BA)),
            "0x02BA is Fellowship_FullUpdate"
        );
        assert!(!is_remove_opcode(Opcode(0x01DF)), "0x01DF is not a remove");
    }

    /// Oracle: removing an integer quality deletes the key, and the integer query's
    /// miss arm returns 0 — **the out parameter is not written**.
    /// So after a remove the quality is *absent*, which is a different fact from *zero*.
    #[test]
    fn a_remove_deletes_the_key_rather_than_zeroing_it() {
        let mut q = Qualities::new();
        let mut s = PropertySequenceGate::new();
        let k = StatKey::new(StatType::Int, 0x86); // PropertyInt::PlayerKillerStatus
        q.set(k, StatValue::Int(4));
        assert_eq!(q.get(k), Some(StatValue::Int(4)));

        let d = decode(
            Opcode::QUALITIES_PRIVATE_REMOVE_INT_EVENT,
            &priv_int(1, 0x86),
        )
        .expect("decodes");
        assert_eq!(apply(&mut q, &mut s, &d), Outcome::Removed);
        assert_eq!(q.get(k), None, "the key is gone, not set to 0");

        // And the difference is observable: a *zeroing* implementation would be indistinguishable
        // from this one if `get` were allowed to answer `Some(Int(0))`.
        let mut zeroed = Qualities::new();
        zeroed.set(k, StatValue::Int(0));
        assert_eq!(zeroed.get(k), Some(StatValue::Int(0)));
        assert_ne!(
            q.get(k),
            zeroed.get(k),
            "absent and zero are different answers"
        );

        // Removing an integer never frees the table -- the removal has no delete in it.
        assert!(q.ints.is_some(), "the emptied table stays allocated");
    }

    /// Oracle: the stamper's timestamp update runs **before** the integer removal, and
    /// the stale branch skips both the delete and the removed-quality handler. The key is
    /// `prop | tag`, which is the *update's* key too.
    #[test]
    fn the_stamp_gate_is_the_updates_own_counter_and_runs_before_the_delete() {
        let mut q = Qualities::new();
        let mut s = PropertySequenceGate::new();
        let k = StatKey::new(StatType::Int, 0x14); // CoinValue
        q.set(k, StatValue::Int(9995));

        // An *update* at stamp 5 moves the shared counter...
        let u = crate::qualities::update::QualityUpdate {
            subject: None,
            sequence: 5,
            key: k,
            value: StatValue::Int(9930),
        };
        assert_eq!(
            crate::qualities::update::apply(&mut q, &mut s, &u),
            crate::qualities::update::Outcome::Applied
        );

        // ...so a remove at stamp 3 is stale, and the value survives untouched.
        let stale = decode(
            Opcode::QUALITIES_PRIVATE_REMOVE_INT_EVENT,
            &priv_int(3, 0x14),
        )
        .expect("decodes");
        assert_eq!(apply(&mut q, &mut s, &stale), Outcome::Stale);
        assert!(
            !Outcome::Stale.fired(),
            "no removed-quality handler on the stale path"
        );
        assert_eq!(
            q.get(k),
            Some(StatValue::Int(9930)),
            "a stale remove deletes nothing"
        );
        assert_eq!(s.rejected(), 1);

        // A remove at stamp 6 lands, and consumes the counter for the update path too.
        let fresh = decode(
            Opcode::QUALITIES_PRIVATE_REMOVE_INT_EVENT,
            &priv_int(6, 0x14),
        )
        .expect("decodes");
        assert_eq!(apply(&mut q, &mut s, &fresh), Outcome::Removed);
        assert_eq!(q.get(k), None);
        assert_eq!(s.stamp(k.0), Some(6));
        let replay = crate::qualities::update::QualityUpdate { sequence: 5, ..u };
        assert_eq!(
            crate::qualities::update::apply(&mut q, &mut s, &replay),
            crate::qualities::update::Outcome::Stale,
            "one counter, shared: the remove moved it past the update's old stamp"
        );
    }

    /// Oracle: the delete falls through to the fan-out — the integer removal's return is never
    /// tested, so the removed-quality handler runs for a property the object was not carrying, and
    /// the stamp is spent either way.
    #[test]
    fn removing_a_property_that_was_never_there_still_spends_the_stamp_and_fires_the_handler() {
        let mut q = Qualities::new();
        let mut s = PropertySequenceGate::new();
        let d = decode(
            Opcode::QUALITIES_PRIVATE_REMOVE_INT_EVENT,
            &priv_int(4, 0x99),
        )
        .expect("decodes");
        assert_eq!(
            apply(&mut q, &mut s, &d),
            Outcome::Absent,
            "NULL table: the integer removal returns 0"
        );
        assert!(
            Outcome::Absent.fired(),
            "and the removed-quality handler runs anyway"
        );
        assert_eq!(
            s.stamp(StatKey::new(StatType::Int, 0x99).0),
            Some(4),
            "the stamp is spent"
        );
        // Which means a later, *earlier-stamped* message is refused -- the observable cost of the
        // client consuming a sequence for a no-op.
        let earlier = decode(
            Opcode::QUALITIES_PRIVATE_REMOVE_INT_EVENT,
            &priv_int(2, 0x99),
        )
        .expect("decodes");
        assert_eq!(apply(&mut q, &mut s, &earlier), Outcome::Stale);
    }

    /// **There are eight quality-removal forms and no ninth.** The client has the
    /// eight generic-table instantiations and no
    /// skill, attribute or vital form, in the client or on the wire — attributes,
    /// vitals and skills can only be updated. [`crate::Qualities::remove`]'s `_ => false` arm is
    /// that fact, and this is what stops a successor "completing" the family.
    #[test]
    fn there_is_no_remove_for_a_skill_an_attribute_or_a_vital() {
        let mut q = Qualities::new();
        let s = Skill {
            level_from_pp: 3,
            format_version: 1,
            sac: 2,
            pp: 100,
            ..Skill::default()
        };
        q.set_skill(7, s);
        assert!(q.skill(7).is_some());
        for t in [StatType::Skill, StatType::Attribute, StatType::Attribute2nd] {
            assert!(
                !q.remove(StatKey::new(t, 7)),
                "{t:?} has no quality-removal form"
            );
        }
        assert!(q.skill(7).is_some(), "and nothing was deleted");
        // Nor is there an opcode that could name one: every tag `decode` can produce is one of
        // the eight generic tables.
        for raw in 0x0000u32..=0xFFFF {
            let op = Opcode(raw);
            if !is_remove_opcode(op) {
                continue;
            }
            let Some(d) = decode(op, &priv_int(0, 1)).or_else(|| {
                decode(
                    op,
                    &body(&wire::QualitiesRemoveInt(wire::PublicRemove {
                        sequence: 0,
                        object: ObjectId(1),
                        property_id: 1,
                    })),
                )
            }) else {
                panic!("{raw:#06X} is routed and does not decode")
            };
            assert!(
                !matches!(
                    d.key.stat_type(),
                    StatType::Skill | StatType::Attribute | StatType::Attribute2nd
                ),
                "{raw:#06X} produced tag {:?}",
                d.key.stat_type()
            );
        }
    }
}
