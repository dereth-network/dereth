//! `ActionMap` decode, dat type 0x27, DID `0x26000000`.
//!
//! The layout follows the client's action-map serialiser, an entry's serialiser and the two
//! intrusive-hash-list serialisers; the accessors follow the client's own.

use std::collections::BTreeMap;

use dereth_assets::Decode;

use crate::error::InputError;
use crate::{ActionId, InputMapId};

/// The toggle type of an action. The client gives the enum no name; the semantics come from
/// the action-fire path and the per-frame input tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToggleType {
    /// The action-fire path substitutes 3 for this.
    Invalid = 0,
    Hold = 1,
    Toggle = 2,
    OneShot = 3,
    HoldRepeat = 4,
    HoldContinuous = 5,
}

impl ToggleType {
    #[must_use]
    pub const fn from_raw(v: u32) -> Self {
        match v {
            1 => Self::Hold,
            2 => Self::Toggle,
            4 => Self::HoldRepeat,
            5 => Self::HoldContinuous,
            // 0 is Invalid in the data but every consumer substitutes 3, and so does the client:
            // it reads the toggle type and replaces 0 with 3.
            _ => Self::OneShot,
        }
    }

    /// True for the three hold families the action-fire path releases on key-up (1, 4, 5).
    #[must_use]
    pub const fn is_hold(self) -> bool {
        matches!(self, Self::Hold | Self::HoldRepeat | Self::HoldContinuous)
    }
}

/// `ActionMapValue` = `{ toggle type, UserBindingValue }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionMapValue {
    pub toggle_type: ToggleType,
    /// Action class: 0 internal, 1 Movement, 2 Camera, 3 UI, 4 Combat,
    /// 5 Emote, 7 Character option.
    pub action_class: u32,
    /// `StringInfo` id into the action map's string table. Zero means "not user-visible".
    pub action_name: u32,
    pub description: u32,
}

/// `ActionMap` — the catalogue of every action the client knows.
#[derive(Debug, Clone, Default)]
pub struct ActionMap {
    /// The input maps, in file order (the intrusive *list* order, which is insertion order).
    input_maps: Vec<(InputMapId, Vec<(ActionId, ActionMapValue)>)>,
    lookup: BTreeMap<(u32, u32), ActionMapValue>,
    /// The string table's DID — `0x23000005` in retail.
    pub string_table: u32,
    /// The conflicting-maps table, 16 entries in retail.
    conflicts: BTreeMap<u32, Vec<InputMapId>>,
}

impl ActionMap {
    /// Deserialise an action map: the dat decoder reads the bytes, and this builds the catalogue
    /// and its lookups from them.
    ///
    /// # Errors
    /// [`InputError::Decode`] for a payload the decoder refuses (a short read, bytes left over, a
    /// list claiming more than twice as many elements as buckets), [`InputError::BadMagic`] for a
    /// wrong DID or a non-zero entry signature, [`InputError::Malformed`] for an entry's discarded
    /// flag out of range.
    pub fn read(payload: &[u8]) -> Result<Self, InputError> {
        let decoded = dereth_assets::ui::ActionMap::decode_bytes(payload)?;
        let mut out = Self {
            string_table: 0,
            ..Self::default()
        };
        let did = decoded.id.0;
        if did != 0x2600_0000 {
            return Err(InputError::BadMagic {
                what: "ActionMap DID",
                got: did,
                want: 0x2600_0000,
            });
        }

        // Both lists use the intrusive hash-list header (a bucket COUNT, not an index), the only
        // place in the dat that does; the decoder refuses more than twice as many elements as
        // buckets, as the client's reader does.
        for (map_id, entries) in decoded.input_maps {
            let map_id = InputMapId(map_id);
            let mut actions = Vec::with_capacity(entries.len());
            for (action, v) in entries {
                let action = ActionId(action);
                // The serialized magic number (0) must read back as 0.
                if v.magic != 0 {
                    return Err(InputError::BadMagic {
                        what: "action-map entry signature",
                        got: v.magic,
                        want: 0,
                    });
                }
                // A bool read into a stack local and discarded; one byte on the wire.
                if v.unused_bool > 1 {
                    return Err(InputError::Malformed(
                        "action-map entry unused flag out of range",
                    ));
                }
                // The stack-local list serialised after the toggle type is thrown away; it is
                // empty in every retail entry.
                let value = ActionMapValue {
                    toggle_type: ToggleType::from_raw(v.toggle_type),
                    action_class: v.binding.action_class,
                    action_name: v.binding.action_name_strid,
                    description: v.binding.description_strid,
                };
                out.lookup.insert((map_id.0, action.0), value);
                actions.push((action, value));
            }
            out.input_maps.push((map_id, actions));
        }

        out.string_table = decoded.string_table.0;

        // The conflicting-maps table: per entry a key, the conflicts value's own map id (unused
        // here), and the list of maps.
        for (key, (_own, list)) in decoded.conflicting_maps {
            out.conflicts
                .insert(key, list.into_iter().map(InputMapId).collect());
        }
        Ok(out)
    }

    /// Every `(input map, action)` entry in file order. 389 rows in the shipped map.
    pub fn entries(&self) -> impl Iterator<Item = (InputMapId, ActionId, ActionMapValue)> + '_ {
        self.input_maps
            .iter()
            .flat_map(|(m, acts)| acts.iter().map(move |(a, v)| (*m, *a, *v)))
    }

    /// The input maps in file order. 27 in the shipped map.
    pub fn input_maps(&self) -> impl Iterator<Item = InputMapId> + '_ {
        self.input_maps.iter().map(|(m, _)| *m)
    }

    #[must_use]
    pub fn value(&self, m: InputMapId, a: ActionId) -> Option<ActionMapValue> {
        self.lookup.get(&(m.0, a.0)).copied()
    }

    /// Look an action up by name. An unknown action yields `Invalid`, which every
    /// caller then substitutes 3 for.
    #[must_use]
    pub fn toggle_type(&self, m: InputMapId, a: ActionId) -> ToggleType {
        self.value(m, a)
            .map_or(ToggleType::Invalid, |v| v.toggle_type)
    }

    /// Add or replace one entry: `a` in input map `m`.
    pub(crate) fn insert(&mut self, m: InputMapId, a: ActionId, v: ActionMapValue) {
        self.lookup.insert((m.0, a.0), v);
        match self.input_maps.iter_mut().find(|(id, _)| *id == m) {
            Some((_, actions)) => {
                actions.retain(|(x, _)| *x != a);
                actions.push((a, v));
            }
            None => self.input_maps.push((m, vec![(a, v)])),
        }
    }

    /// Whether the action is a toggle -- **always true for action id 1**.
    #[must_use]
    pub fn is_action_allowed_in_input_map(&self, m: InputMapId, a: ActionId) -> bool {
        a.0 == 1 || self.value(m, a).is_some()
    }

    /// User-bindable -- true iff **both** the action class and
    /// the action name are non-zero. 306 of the 389 shipped entries.
    #[must_use]
    pub fn is_user_bindable(&self, m: InputMapId, a: ActionId) -> bool {
        self.value(m, a)
            .is_some_and(|v| v.action_class != 0 && v.action_name != 0)
    }

    /// The action's class.
    #[must_use]
    pub fn action_class(&self, m: InputMapId, a: ActionId) -> u32 {
        self.value(m, a).map_or(0, |v| v.action_class)
    }

    /// The `(name, description)` `StringInfo` ids.
    #[must_use]
    pub fn descrip_values(&self, m: InputMapId, a: ActionId) -> (u32, u32) {
        self.value(m, a)
            .map_or((0, 0), |v| (v.action_name, v.description))
    }

    /// Make `a` and `b` conflict, both ways: a key bound in one is taken from the other. A map
    /// that had no list gets one holding itself first.
    pub(crate) fn add_conflict(&mut self, a: InputMapId, b: InputMapId) {
        for (x, y) in [(a, b), (b, a)] {
            let list = self.conflicts.entry(x.0).or_insert_with(|| vec![x]);
            if !list.contains(&y) {
                list.push(y);
            }
        }
    }

    /// The maps an action conflicts in. The list **includes the map itself**.
    #[must_use]
    pub fn conflicting_input_maps(&self, m: InputMapId) -> &[InputMapId] {
        self.conflicts.get(&m.0).map_or(&[][..], Vec::as_slice)
    }

    #[must_use]
    pub fn conflict_count(&self) -> usize {
        self.conflicts.len()
    }

    /// Build a synthetic `ActionMap` from `(input map, action, toggle type)` triples.
    ///
    /// Test-only: the real one comes from the dat. A unit test that needs the *toggle machine* to
    /// behave should not have to carry a 12 KB payload, and a manager with no `ActionMap` treats
    /// every action as a one-shot, which silently hides hold behaviour.
    #[cfg(test)]
    pub(crate) fn from_toggles(entries: &[(u32, u32, ToggleType)]) -> Self {
        let mut out = Self::default();
        for (m, a, t) in entries {
            let v = ActionMapValue {
                toggle_type: *t,
                action_class: 0,
                action_name: 0,
                description: 0,
            };
            out.lookup.insert((*m, *a), v);
            match out.input_maps.iter_mut().find(|(id, _)| id.0 == *m) {
                Some((_, acts)) => acts.push((ActionId(*a), v)),
                None => out
                    .input_maps
                    .push((InputMapId(*m), vec![(ActionId(*a), v)])),
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An action map payload: the DID, one input map (`outer` packed bucket count) holding
    /// `entries` entries under a packed bucket count `inner`, the string table and a one-entry
    /// conflict table.
    fn payload(outer: &[u8], inner: &[u8], entries: u32, magic: u32) -> Vec<u8> {
        let mut p = 0x2600_0000u32.to_le_bytes().to_vec();
        p.extend_from_slice(outer);
        p.push(1);
        p.extend_from_slice(&5u32.to_le_bytes());
        p.extend_from_slice(inner);
        p.push(u8::try_from(entries).expect("few entries"));
        for a in 0..entries {
            p.extend_from_slice(&(0x10 + a).to_le_bytes());
            p.extend_from_slice(&magic.to_le_bytes());
            p.push(0);
            p.extend_from_slice(&1u32.to_le_bytes());
            p.extend_from_slice(&0u32.to_le_bytes());
            for v in [3u32, 0x0100_0000 + a, 0] {
                p.extend_from_slice(&v.to_le_bytes());
            }
        }
        p.extend_from_slice(&0x2300_0005u32.to_le_bytes());
        p.push(0);
        p.push(1);
        for v in [5u32, 5, 2, 5, 6] {
            p.extend_from_slice(&v.to_le_bytes());
        }
        p
    }

    /// The two list headers are packed bucket counts: `17` is 23 buckets, `1B` 27 and the
    /// two-byte form `83 69` is 873. A list may hold up to twice its buckets.
    #[test]
    fn the_list_headers_are_packed_bucket_counts() {
        for outer in [&[0x17u8][..], &[0x1B], &[0x83, 0x69]] {
            let am = ActionMap::read(&payload(outer, &[1], 2, 0)).expect("decodes");
            assert_eq!(am.entries().count(), 2);
            assert_eq!(
                am.toggle_type(InputMapId(5), ActionId(0x11)),
                ToggleType::Hold
            );
            assert!(am.is_user_bindable(InputMapId(5), ActionId(0x10)));
            assert_eq!(am.string_table, 0x2300_0005);
            assert_eq!(
                am.conflicting_input_maps(InputMapId(5)),
                &[InputMapId(5), InputMapId(6)]
            );
        }
    }

    /// Three elements under one bucket, a non-zero entry signature, a short payload and a
    /// leftover byte are each refused.
    #[test]
    fn a_malformed_action_map_is_refused() {
        assert!(matches!(
            ActionMap::read(&payload(&[1], &[1], 3, 0)),
            Err(InputError::Decode(_))
        ));
        assert!(matches!(
            ActionMap::read(&payload(&[1], &[1], 1, 7)),
            Err(InputError::BadMagic { got: 7, .. })
        ));
        let good = payload(&[1], &[1], 1, 0);
        assert!(ActionMap::read(&good).is_ok());
        assert!(ActionMap::read(&good[..good.len() - 1]).is_err());
        let mut extra = good.clone();
        extra.push(0);
        assert!(matches!(
            ActionMap::read(&extra),
            Err(InputError::Decode(_))
        ));
        let mut wrong_did = good;
        wrong_did[3] = 0x27;
        assert!(matches!(
            ActionMap::read(&wrong_did),
            Err(InputError::BadMagic {
                got: 0x2700_0000,
                ..
            })
        ));
    }
}
