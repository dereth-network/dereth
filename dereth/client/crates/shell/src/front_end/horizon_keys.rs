//! The Horizon interface's key bindings page, carried out on the input manager: the page shows the
//! rows and asks; this captures the key pressed, binds and clears keys by the input manager's own
//! rules, keeps the key map file, and hands the page the bindings in the game's words.
//!
//! The rows are the shared catalog's ([`dereth_input::presentation`]) that the Horizon interface shows,
//! bound in the modern key map, which the Horizon interface plays with.

use dereth_horizon::ui::game::{KeyBindingsView, KeyCapture, KeyRequest, KeyRow};
use dereth_input::binding::{Capture, Conflict, DO_NOTHING};
use dereth_input::presentation::{self, Group, Interface};
use dereth_input::{ActionId, ControlChord, InputMapId};

use crate::input::InputShell;

/// How many keys a row of the page shows: a key bound beyond them takes the place of the first.
const KEY_SLOTS: usize = 3;

/// The camera's zoom actions in the camera map, and the wheel turn the mouse-turning settings
/// give each.
const MOUSE_TURNING_KEYS: [(u32, u32); 2] = [(0x33, 0x0008_0101), (0x34, 0x0008_0201)];

/// The camera map the zoom actions are bound in.
const CAMERA_MAP: InputMapId = InputMapId(5);

/// A key being captured: for which row, in place of which of its keys, and the key waiting on the
/// player's answer to whether it may replace what it does elsewhere.
#[derive(Debug, Clone)]
struct Capturing {
    map: InputMapId,
    action: ActionId,
    slot: Option<usize>,
    pending: Option<ControlChord>,
    question: Option<String>,
}

/// The page's state on the input manager's side.
#[derive(Debug, Default)]
pub(crate) struct HorizonKeys {
    capturing: Option<Capturing>,
    refused: Option<String>,
    /// The view is out of date with the key map.
    stale: bool,
    built: bool,
    /// The game's string tables, for the keys' names.
    strings: dereth_horizon::strings::Strings,
    /// The chat lines the mouse-turning keys print, for the interface to show.
    pub lines: Vec<&'static str>,
}

/// The game's names for keys, from its string tables.
struct Labels<'a> {
    strings: std::cell::RefCell<&'a mut dereth_horizon::strings::Strings>,
    store: &'a dereth_dat::RetailDatStore,
}

impl dereth_input::labels::LabelProvider for Labels<'_> {
    fn resolve_token(&self, table: u32, token: &str) -> Option<String> {
        self.strings
            .borrow_mut()
            .lookup(self.store, table, token)
            .filter(|s| !s.is_empty())
    }
    fn format_subcontrol(&self, key: &str, subcontrol: &str) -> Option<String> {
        Some(format!("{key} {subcontrol}"))
    }
    fn delimiter(&self) -> &str {
        "+"
    }
}

/// The page's tab for a row's group, in the order the page lists them.
fn group_index(group: Group) -> usize {
    match group {
        Group::Movement => 0,
        Group::Camera => 1,
        Group::Combat => 2,
        Group::Selection => 3,
        Group::Panels => 4,
        Group::Chat => 5,
        Group::Shortcuts => 6,
        Group::CharacterOptions => 7,
        Group::Emotes => 8,
        Group::Other => 9,
    }
}

/// A row's caption, by its map and action.
fn caption(map: InputMapId, action: ActionId) -> String {
    presentation::find(map, action)
        .or_else(|| {
            presentation::HORIZON_ROWS
                .iter()
                .find(|r| r.map == map && r.action() == action)
        })
        .map_or_else(
            || dereth_client_contract::actions::names::enum_name_for_action(action),
            |r| r.label_in(Interface::Horizon).to_owned(),
        )
}

/// What a captured key does elsewhere, less the keys freed by an earlier rebinding: a freed key
/// is bound to nothing, and taking it displaces nothing the player would miss.
fn without_freed(conflicts: Vec<Conflict>) -> Vec<Conflict> {
    conflicts
        .into_iter()
        .filter(|c| c.action != DO_NOTHING)
        .collect()
}

/// Whether two lists hold the same controls.
fn same(a: &[ControlChord], b: &[ControlChord]) -> bool {
    a.len() == b.len() && a.iter().all(|x| b.iter().any(|y| x.is_exactly_equal(y)))
}

impl HorizonKeys {
    /// Carry out what the page asked, and take the key it is waiting for, if one came.
    pub fn serve(&mut self, input: &mut InputShell, requests: Vec<KeyRequest>) {
        for request in requests {
            self.request(input, request);
        }
        if let Some(c) = self.capturing.clone() {
            if c.pending.is_none() {
                for hit in input.take_key_hits() {
                    if self.capturing.is_none() {
                        break;
                    }
                    self.captured(input, &c, hit);
                }
            }
        }
    }

    fn request(&mut self, input: &mut InputShell, request: KeyRequest) {
        match request {
            KeyRequest::Capture { map, action, slot } => {
                self.refused = None;
                let _ = input.take_key_hits();
                input.set_key_hit_handler(true);
                self.capturing = Some(Capturing {
                    map: InputMapId(map),
                    action: ActionId(action),
                    slot,
                    pending: None,
                    question: None,
                });
                self.stale = true;
            }
            KeyRequest::Cancel => self.end(input),
            KeyRequest::Answer(yes) => {
                if let Some(c) = self.capturing.clone() {
                    if let (true, Some(control)) = (yes, c.pending) {
                        self.bind(input, c.map, c.action, c.slot, control);
                    }
                }
                self.end(input);
            }
            KeyRequest::Clear { map, action, slot } => {
                let map = InputMapId(map);
                let keys = input.keys_for_action(ActionId(action), map);
                if let Some(control) = keys.get(slot) {
                    // The freed key is bound to nothing rather than left out, so the shipped map
                    // does not hand it back on the next run.
                    input.manager.unbind_by_key(control, map);
                    input.manager.bind_action(*control, DO_NOTHING, map);
                    self.save(input);
                }
            }
            KeyRequest::RestoreDefaults => {
                for row in presentation::horizon_page_rows() {
                    let (map, action) = (row.map, row.action());
                    // The Horizon interface's own layout while its keys are in use.
                    let want = if input.horizon_keys_active() {
                        input.horizon_default_keys(action, map)
                    } else {
                        input.manager.default_keys_for_action(action, map)
                    };
                    for control in input.keys_for_action(action, map) {
                        if !want.iter().any(|w| w.is_exactly_equal(&control)) {
                            input.manager.unbind_by_key(&control, map);
                            input.manager.bind_action(control, DO_NOTHING, map);
                        }
                    }
                    input.manager.unbind_all_by_action(action, map);
                    for control in &want {
                        input.manager.bind_action(*control, action, map);
                    }
                }
                self.save(input);
            }
            KeyRequest::MouseTurningKeys => {
                for (k, (action, wheel)) in MOUSE_TURNING_KEYS.into_iter().enumerate() {
                    let control = ControlChord::new(
                        dereth_input::spec::ControlCode(wheel),
                        0,
                        dereth_input::spec::activation::UP,
                    );
                    if let Capture::Ready { control, .. } =
                        input.capture_key_hit(CAMERA_MAP, ActionId(action), control, true)
                    {
                        self.bind(input, CAMERA_MAP, ActionId(action), None, control);
                        self.lines.push(
                            dereth_ui_screens::options::config::MOUSE_TURNING_KEY_MESSAGES[k],
                        );
                    }
                }
            }
        }
    }

    /// The key the page was waiting for, by the input manager's own rules.
    fn captured(&mut self, input: &mut InputShell, c: &Capturing, hit: ControlChord) {
        let verdict = match input.capture_key_hit(c.map, c.action, hit, false) {
            // A key only a freed binding holds is free: asked again, as already answered.
            Capture::Refused(conflicts) | Capture::NeedsConfirmation { conflicts, .. }
                if conflicts.iter().all(|x| x.action == DO_NOTHING) =>
            {
                input.capture_key_hit(c.map, c.action, hit, true)
            }
            Capture::Refused(conflicts) => {
                let conflicts = without_freed(conflicts);
                if conflicts.iter().all(|x| {
                    input
                        .manager
                        .action_map
                        .is_user_bindable(x.input_map, x.action)
                }) {
                    let control = ControlChord::new(
                        hit.control,
                        hit.meta_mode,
                        dereth_input::spec::activation::CLICK,
                    );
                    Capture::NeedsConfirmation { control, conflicts }
                } else {
                    Capture::Refused(conflicts)
                }
            }
            Capture::NeedsConfirmation { control, conflicts } => Capture::NeedsConfirmation {
                control,
                conflicts: without_freed(conflicts),
            },
            other => other,
        };
        match verdict {
            Capture::Ignored | Capture::Rejected => {}
            Capture::Cancelled | Capture::Unchanged => self.end(input),
            Capture::Refused(conflicts) => {
                let what: Vec<String> = conflicts
                    .iter()
                    .map(|x| caption(x.input_map, x.action))
                    .collect();
                self.refused = Some(format!(
                    "That key cannot be bound: it is the game's own for {}.",
                    what.join(", ")
                ));
                self.end(input);
            }
            Capture::NeedsConfirmation { control, conflicts } => {
                let what: Vec<String> = conflicts
                    .iter()
                    .map(|x| caption(x.input_map, x.action))
                    .collect();
                let me = caption(c.map, c.action);
                if let Some(cap) = self.capturing.as_mut() {
                    cap.pending = Some(control);
                    cap.question = Some(format!(
                        "That key is bound to {}. Bind it to {me} instead?",
                        what.join(", ")
                    ));
                }
                input.set_key_hit_handler(false);
                self.stale = true;
            }
            Capture::Ready { control, .. } => {
                self.bind(input, c.map, c.action, c.slot, control);
                self.end(input);
            }
        }
    }

    /// Bind `control` to `action` in `map`: in place of key `slot` when the row has one there,
    /// beside its keys while the row has room, and in place of its first key when it has none.
    fn bind(
        &mut self,
        input: &mut InputShell,
        map: InputMapId,
        action: ActionId,
        slot: Option<usize>,
        control: ControlChord,
    ) {
        let keys = input.keys_for_action(action, map);
        let replacing = slot.filter(|i| *i < keys.len());
        if replacing.is_none() && keys.len() >= KEY_SLOTS {
            if let Some(head) = keys.first().copied() {
                input.manager.unbind_by_key(&head, map);
                if !control.is_conflicting(&head) {
                    input.manager.bind_action(head, DO_NOTHING, map);
                }
            }
        }
        if input.set_binding(map, action, replacing, control) {
            self.save(input);
        }
    }

    fn end(&mut self, input: &mut InputShell) {
        self.capturing = None;
        input.set_key_hit_handler(false);
        self.stale = true;
    }

    fn save(&mut self, input: &mut InputShell) {
        self.stale = true;
        if let Err(e) = input.save_active_keymap() {
            tracing::warn!("the keymap was not saved: {e}");
        }
    }

    /// The page shows the key map as another interface may have left it: build its rows again.
    pub fn invalidate(&mut self) {
        self.stale = true;
    }

    /// The bindings as the page shows them, when they changed since the page last had them.
    pub fn view(
        &mut self,
        input: &InputShell,
        store: &dereth_dat::RetailDatStore,
    ) -> Option<KeyBindingsView> {
        if self.built && !self.stale {
            return None;
        }
        self.built = true;
        self.stale = false;
        let labels = Labels {
            strings: std::cell::RefCell::new(&mut self.strings),
            store,
        };
        let rows = presentation::horizon_page_rows()
            .map(|r| {
                let (map, action) = (r.map, r.action());
                let keys = input.keys_for_action(action, map);
                let defaults = input.manager.default_keys_for_action(action, map);
                KeyRow {
                    map: map.0,
                    action: action.0,
                    group: group_index(r.group),
                    caption: r.label_in(Interface::Horizon).to_owned(),
                    changed: !same(&keys, &defaults),
                    keys: keys
                        .iter()
                        .map(|k| {
                            dereth_input::labels::binding_label(&input.manager.keymap, *k, &labels)
                        })
                        .collect(),
                }
            })
            .collect();
        let capture = self.capturing.as_ref().map(|c| KeyCapture {
            map: c.map.0,
            action: c.action.0,
            caption: caption(c.map, c.action),
            question: c.question.clone(),
        });
        Some(KeyBindingsView {
            rows,
            capture,
            refused: self.refused.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental Horizon interface; the binding rules are the input manager's)
    use super::*;
    use dereth_input::spec::{activation, ControlCode, SubControlIndex};

    /// The input manager over the shipped maps, keeping its key map in a folder of its own.
    fn input(name: &str) -> (InputShell, std::path::PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("dereth-horizon-keys-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = dereth_dat::RetailDatStore::open_dir(&dereth_dat::testing::dat_dir())
            .expect("the retail dats open");
        let keymap = dir.join("test.keymap");
        let input = InputShell::new(&store, Some(&keymap)).expect("the shipped maps");
        (input, dir)
    }

    /// The first row the Horizon page shows with a key bound to it.
    fn bound_row(input: &InputShell) -> (InputMapId, ActionId) {
        presentation::ROWS
            .iter()
            .filter(|r| r.shown(Interface::Horizon))
            .map(|r| (r.map, r.action()))
            .find(|(m, a)| !input.keys_for_action(*a, *m).is_empty())
            .expect("a bound row")
    }

    #[test]
    #[cfg_attr(not(feature = "retail-dats"), ignore = "reads the retail dats")]
    fn a_cleared_key_does_nothing_and_restoring_the_defaults_brings_it_back() {
        let (mut input, dir) = input("clear");
        let (map, action) = bound_row(&input);
        let before = input.keys_for_action(action, map);
        let mut keys = HorizonKeys::default();
        keys.serve(
            &mut input,
            vec![KeyRequest::Clear {
                map: map.0,
                action: action.0,
                slot: 0,
            }],
        );
        let after = input.keys_for_action(action, map);
        assert!(!after.iter().any(|k| k.is_exactly_equal(&before[0])));
        assert!(input
            .keys_for_action(DO_NOTHING, map)
            .iter()
            .any(|k| k.is_exactly_equal(&before[0])));
        keys.serve(&mut input, vec![KeyRequest::RestoreDefaults]);
        let restored = input.keys_for_action(action, map);
        assert!(same(
            &restored,
            &input.manager.default_keys_for_action(action, map)
        ));
        assert!(
            dir.join("test.keymap").is_file(),
            "each change keeps the key map"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg_attr(not(feature = "retail-dats"), ignore = "reads the retail dats")]
    fn a_free_key_pressed_while_waiting_is_bound_beside_the_row_s_keys() {
        let (mut input, dir) = input("bind");
        let (map, action) = bound_row(&input);
        // A key the row's map leaves free, as the input manager judges it.
        let free = (0x02..0x58)
            .map(|scan| {
                ControlChord::new(
                    ControlCode::new(0, SubControlIndex::None, scan),
                    0,
                    activation::UP,
                )
            })
            .find(|c| {
                matches!(
                    input.capture_key_hit(map, action, *c, false),
                    Capture::Ready { .. }
                )
            })
            .expect("a free key");
        let mut keys = HorizonKeys::default();
        keys.serve(
            &mut input,
            vec![KeyRequest::Capture {
                map: map.0,
                action: action.0,
                slot: None,
            }],
        );
        let store = dereth_dat::RetailDatStore::open_dir(&dereth_dat::testing::dat_dir()).unwrap();
        let view = keys.view(&input, &store).expect("the first view");
        assert_eq!(view.capture.as_ref().map(|c| c.action), Some(action.0));
        assert!(
            view.rows
                .iter()
                .any(|r| r.keys.iter().any(|k| !k.is_empty())),
            "the keys are named"
        );
        let c = keys.capturing.clone().unwrap();
        keys.captured(&mut input, &c, free);
        assert!(
            keys.capturing.is_none(),
            "the capture ends with the key bound"
        );
        assert!(input
            .keys_for_action(action, map)
            .iter()
            .any(|k| k.control == free.control && k.activation == activation::CLICK));
        let view = keys
            .view(&input, &store)
            .expect("the view follows the change");
        assert!(view.capture.is_none());
        let row = view
            .rows
            .iter()
            .find(|r| r.map == map.0 && r.action == action.0)
            .unwrap();
        assert!(row.changed);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg_attr(not(feature = "retail-dats"), ignore = "reads the retail dats")]
    fn a_key_a_rebinding_freed_binds_again_without_naming_what_it_did_before() {
        let (mut input, dir) = input("freed");
        input.activate_horizon(true);
        let name = |n| dereth_client_contract::actions::names::action_for_enum_name(n).unwrap();
        let (next, previous) = (
            name("SelectionNextMonster"),
            name("SelectionPreviousMonster"),
        );
        let map = presentation::rows_of(next).next().unwrap().input_map();
        let key = |input: &InputShell, meta| {
            let k = dereth_input::scheme::keyboard_key(&input.manager.keymap, 0x0F, meta).unwrap();
            ControlChord::new(k.control, k.meta_mode, activation::UP)
        };
        let (tab, shift_tab) = (
            key(&input, 0),
            key(&input, crate::input::horizon_scheme::SHIFT),
        );
        let mut keys = HorizonKeys::default();
        // Next takes Shift+Tab in place of Tab, which is freed; Previous takes Shift+Tab back.
        for (action, hit) in [(next, shift_tab), (previous, shift_tab)] {
            keys.serve(
                &mut input,
                vec![KeyRequest::Capture {
                    map: map.0,
                    action: action.0,
                    slot: Some(0),
                }],
            );
            let c = keys.capturing.clone().unwrap();
            keys.captured(&mut input, &c, hit);
            keys.serve(&mut input, vec![KeyRequest::Answer(true)]);
        }
        assert!(input
            .keys_for_action(DO_NOTHING, map)
            .iter()
            .any(|k| k.control == tab.control && k.meta_mode == 0));
        // Tab for Next again: free, bound at once, nothing asked.
        keys.serve(
            &mut input,
            vec![KeyRequest::Capture {
                map: map.0,
                action: next.0,
                slot: None,
            }],
        );
        let c = keys.capturing.clone().unwrap();
        keys.captured(&mut input, &c, tab);
        assert!(keys.capturing.is_none(), "nothing asked");
        assert!(keys.refused.is_none());
        assert!(input
            .keys_for_action(next, map)
            .iter()
            .any(|k| k.control == tab.control && k.meta_mode == 0));
        assert!(!input
            .keys_for_action(DO_NOTHING, map)
            .iter()
            .any(|k| k.control == tab.control && k.meta_mode == 0));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg_attr(not(feature = "retail-dats"), ignore = "reads the retail dats")]
    fn restoring_the_horizon_defaults_leaves_no_freed_arrow_over_the_camera_s() {
        let (mut input, dir) = input("arrows");
        input.activate_horizon(true);
        let name = |n| dereth_client_contract::actions::names::action_for_enum_name(n).unwrap();
        let movement = presentation::rows_of(name("MovementTurnLeft"))
            .next()
            .unwrap()
            .input_map();
        let left = dereth_input::scheme::keyboard_key(&input.manager.keymap, 0xCB, 0).unwrap();
        // The left arrow freed among the movement keys, which the layout gives no arrow.
        input.manager.bind_action(left, DO_NOTHING, movement);
        let mut keys = HorizonKeys::default();
        keys.serve(&mut input, vec![KeyRequest::RestoreDefaults]);
        assert!(!input
            .keys_for_action(DO_NOTHING, movement)
            .iter()
            .any(|k| k.is_exactly_equal(&left)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg_attr(not(feature = "retail-dats"), ignore = "reads the retail dats")]
    fn the_page_shows_the_chat_keys_the_last_shortcuts_and_the_wheel_by_their_keys() {
        let (mut input, dir) = input("shown");
        input.activate_horizon(true);
        let mut keys = HorizonKeys::default();
        let store = dereth_dat::RetailDatStore::open_dir(&dereth_dat::testing::dat_dir()).unwrap();
        let view = keys.view(&input, &store).expect("the first view");
        let keys_of = |caption: &str| {
            view.rows
                .iter()
                .find(|r| r.caption == caption)
                .unwrap_or_else(|| panic!("{caption} is listed"))
                .keys
                .clone()
        };
        for caption in [
            "Enter Chat Mode",
            "Issue Slash Command",
            "Use Shortcut 10",
            "Use Shortcut 11",
            "Use Shortcut 12",
            "Camera Closer",
            "Camera Farther",
            "Toggle Walk/Run",
        ] {
            let keys = keys_of(caption);
            assert!(
                !keys.is_empty() && keys.iter().all(|k| !k.is_empty()),
                "{caption}: {keys:?}"
            );
        }
        for caption in [
            "Hold Sidestep",
            "Switch Chat Mode",
            "Create Shortcut",
            "End Character Session",
        ] {
            assert!(view.rows.iter().all(|r| r.caption != caption), "{caption}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg_attr(not(feature = "retail-dats"), ignore = "reads the retail dats")]
    fn the_mouse_turning_keys_put_the_wheel_on_the_camera_zoom() {
        let (mut input, dir) = input("wheel");
        let mut keys = HorizonKeys::default();
        keys.serve(&mut input, vec![KeyRequest::MouseTurningKeys]);
        for (action, wheel) in MOUSE_TURNING_KEYS {
            assert!(input
                .keys_for_action(ActionId(action), CAMERA_MAP)
                .iter()
                .any(|k| k.control == ControlCode(wheel)));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
