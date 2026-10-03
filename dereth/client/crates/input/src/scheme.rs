//! A key scheme: a whole key map as one interface keeps it, a file merged over that interface's
//! default maps.
//!
//! Both interfaces keep their keys the way the retail client keeps its own: the player's file is
//! read first and each default map after it, each adding what the file does not mention, so a
//! default key is only bound on a control the file leaves free. Clearing a key binds it to
//! [`DO_NOTHING`] in its map rather than dropping it, so the file mentions the control and the
//! default stays away when the file is read again. That is what the retail key page does with a
//! cleared key, and what these helpers do for any page.
//!
//! A default whose control the file binds in a map that conflicts with the default's own map is
//! kept away too, as the retail client's binding step refuses a mapping a conflicting map already
//! has: a key the player moved out of the movement map into one that conflicts with it does not
//! come back to walking at the next start.

use crate::actionmap::ActionMap;
use crate::binding::DO_NOTHING;
use crate::keymap::{InputMap, MasterInputMap};
use crate::spec::{ControlChord, DeviceType};
use crate::{ActionId, InputMapId};

/// What `file` reads as over `defaults`: the file's bindings, then each default map's, each
/// adding only what is not there yet. `header` gives the devices and meta keys when there is no
/// file. A default whose control the file binds in a map that conflicts with the default's is
/// left out (see the module documentation); `action_map` gives the conflicts.
#[must_use]
pub fn over_defaults(
    file: Option<&MasterInputMap>,
    defaults: &[&MasterInputMap],
    action_map: Option<&ActionMap>,
) -> MasterInputMap {
    let mut out = MasterInputMap::default();
    if let Some(file) = file {
        out.merge(file, true);
    }
    for d in defaults {
        let mut kept = (*d).clone();
        if let (Some(file), Some(am)) = (file, action_map) {
            for section in &mut kept.sections {
                let id = section.input_map_id;
                let refused: Vec<ControlChord> = section
                    .bindings()
                    .iter()
                    .filter(|(qc, _)| {
                        am.conflicting_input_maps(id).iter().any(|m| {
                            *m != id
                                && file.section(*m).is_some_and(|s| {
                                    s.bindings().iter().any(|(k, _)| k.is_conflicting(qc))
                                })
                        })
                    })
                    .map(|(qc, _)| *qc)
                    .collect();
                for qc in refused {
                    section.unbind_by_key(&qc);
                }
            }
        }
        out.merge(&kept, true);
    }
    out
}

/// The bindings to write so that the file, read over `defaults`, binds the key pages' rows
/// exactly as `scheme` does: `scheme`'s keys for the rows, and [`DO_NOTHING`] on every control a
/// default binds to a row's action in a map `scheme` leaves it free in. What no key page lists
/// (the text box's keys, the dialogs', the system keys) stays the defaults'.
#[must_use]
pub fn exactly(scheme: &MasterInputMap, defaults: &[&MasterInputMap]) -> MasterInputMap {
    let row = |m: InputMapId, a: ActionId| crate::presentation::find(m, a).is_some();
    let mut out = scheme.clone();
    out.sections.iter_mut().for_each(|s| {
        let id = s.input_map_id;
        let kept: Vec<_> = s
            .bindings()
            .iter()
            .filter(|(_, a)| *a != DO_NOTHING && row(id, *a))
            .copied()
            .collect();
        *s = InputMap::new(id);
        for (qc, a) in kept {
            s.add_mapping(qc, a);
        }
    });
    for d in defaults {
        for section in &d.sections {
            for (qc, action) in section.bindings() {
                if !row(section.input_map_id, *action) {
                    continue;
                }
                let free = out
                    .section(section.input_map_id)
                    .is_none_or(|s| s.bindings().iter().all(|(k, _)| k != qc));
                if free {
                    out.create_input_map(section.input_map_id)
                        .add_mapping(*qc, DO_NOTHING);
                }
            }
        }
    }
    out
}

/// The input map an action's keys go in: the first map the key pages list it in, else the first
/// map one of `shipped` binds it in, else the first the action map allows it in, else this
/// client's own map.
#[must_use]
pub fn home_map(
    action_map: &ActionMap,
    shipped: &[&MasterInputMap],
    action: ActionId,
) -> InputMapId {
    if let Some(row) = crate::presentation::rows_of(action).next() {
        return row.input_map();
    }
    for m in shipped {
        if let Some(s) = m
            .sections
            .iter()
            .find(|s| s.bindings().iter().any(|(_, a)| *a == action))
        {
            return s.input_map_id;
        }
    }
    action_map
        .input_maps()
        .find(|m| action_map.is_action_allowed_in_input_map(*m, action))
        .unwrap_or(crate::dereth::INPUT_MAP)
}

/// The keyboard key `scan` (bit 7 for the extended prefix) held with the meta mode `meta`, as
/// `template` binds keyboard keys: its keyboard device, sub-control and activation.
#[must_use]
pub fn keyboard_key(template: &MasterInputMap, scan: u16, meta: u32) -> Option<ControlChord> {
    crate::dereth::keyboard_chord(template, scan)
        .map(|c| ControlChord::new(c.control, meta, c.activation))
}

/// Every keyboard binding of `map` but the cleared ones, as `(map, key, action)`.
pub fn keyboard_bindings(
    map: &MasterInputMap,
) -> impl Iterator<Item = (InputMapId, ControlChord, ActionId)> + '_ {
    map.sections.iter().flat_map(move |s| {
        s.bindings()
            .iter()
            .filter(move |(qc, a)| {
                *a != DO_NOTHING && map.device_type_of(qc.control) == Some(DeviceType::Keyboard)
            })
            .map(move |(qc, a)| (s.input_map_id, *qc, *a))
    })
}

/// Bind `key` to `action` in `home`, in place of whatever `key` did there and in the maps that
/// conflict with it. `replaced`, when set, is a key `action` no longer has: it is cleared
/// ([`clear`]) unless it is `key` itself.
pub fn bind(
    map: &mut MasterInputMap,
    action_map: &ActionMap,
    home: InputMapId,
    key: ControlChord,
    action: ActionId,
    replaced: Option<ControlChord>,
) {
    let mut maps: Vec<InputMapId> = action_map.conflicting_input_maps(home).to_vec();
    if !maps.contains(&home) {
        maps.push(home);
    }
    for m in maps {
        if let Some(s) = map.section_mut(m) {
            let taken: Vec<ControlChord> = s
                .bindings()
                .iter()
                .filter(|(k, _)| k.is_conflicting(&key))
                .map(|(k, _)| *k)
                .collect();
            for k in taken {
                s.unbind_by_key(&k);
            }
        }
    }
    if let Some(old) = replaced.filter(|old| !old.is_conflicting(&key)) {
        clear(map, home, old, action);
    }
    map.create_input_map(home).add_mapping(key, action);
}

/// `action` no longer has `key` in `home`: the key is bound to [`DO_NOTHING`] there, so the
/// clear outlives the file. A key bound to something else there is left alone.
pub fn clear(map: &mut MasterInputMap, home: InputMapId, key: ControlChord, action: ActionId) {
    let section = map.create_input_map(home);
    let bound: Vec<ControlChord> = section
        .bindings()
        .iter()
        .filter(|(k, a)| *a == action && k.control == key.control && k.meta_mode == key.meta_mode)
        .map(|(k, _)| *k)
        .collect();
    for k in bound {
        section.unbind_by_key(&k);
        section.add_mapping(k, DO_NOTHING);
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the scheme helpers over hand-built maps; the key pages that use them are
    //! tested where they are).
    use super::*;
    use crate::keymap::DeviceKeyMapEntry;
    use crate::spec::{ControlCode, SubControlIndex};

    const MOVEMENT: InputMapId = InputMapId(4);
    const FORWARD: ActionId = ActionId(0x29);
    const BACK: ActionId = ActionId(0x2A);

    fn key(scan: u16) -> ControlChord {
        ControlChord::new(
            ControlCode::new(0, SubControlIndex::from_raw(0), scan),
            0,
            3,
        )
    }

    fn defaults() -> MasterInputMap {
        let mut m = MasterInputMap {
            devices: vec![DeviceKeyMapEntry {
                device_type: DeviceType::Keyboard,
                guid: [1; 16],
            }],
            ..MasterInputMap::default()
        };
        let s = m.create_input_map(MOVEMENT);
        s.add_mapping(key(0x11), FORWARD);
        s.add_mapping(key(0x2D), BACK);
        m
    }

    fn actions(m: &MasterInputMap, scan: u16) -> Vec<ActionId> {
        m.section(MOVEMENT)
            .map(|s| {
                s.bindings()
                    .iter()
                    .filter(|(k, _)| k.control.offset() == scan)
                    .map(|(_, a)| *a)
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn a_cleared_key_stays_cleared_when_its_file_is_read_again() {
        let d = defaults();
        let mut live = over_defaults(None, &[&d], None);
        clear(&mut live, MOVEMENT, key(0x11), FORWARD);
        let text = live.to_keymap_text();
        let back = MasterInputMap::from_keymap_text(&text).expect("reads back");
        let again = over_defaults(Some(&back), &[&d], None);
        assert_eq!(actions(&again, 0x11), [DO_NOTHING]);
        assert_eq!(keyboard_bindings(&again).count(), 1, "only X, backing up");
    }

    #[test]
    fn a_scheme_applied_exactly_reads_back_as_itself() {
        let d = defaults();
        // A scheme that walks forward on Q and binds nothing on W or X.
        let mut scheme = MasterInputMap::default();
        scheme
            .create_input_map(MOVEMENT)
            .add_mapping(key(0x10), FORWARD);
        let file = exactly(&scheme, &[&d]);
        let live = over_defaults(Some(&file), &[&d], None);
        let bound: Vec<_> = keyboard_bindings(&live)
            .map(|(_, k, a)| (k.control.offset(), a))
            .collect();
        assert_eq!(bound, [(0x10, FORWARD)]);
    }

    #[test]
    fn a_rebind_frees_the_old_key_for_good() {
        let d = defaults();
        let am = ActionMap::default();
        let mut live = over_defaults(None, &[&d], None);
        bind(
            &mut live,
            &am,
            MOVEMENT,
            key(0x10),
            FORWARD,
            Some(key(0x11)),
        );
        let again = over_defaults(Some(&live), &[&d], None);
        assert_eq!(actions(&again, 0x10), [FORWARD]);
        assert_eq!(actions(&again, 0x11), [DO_NOTHING]);
    }
}
