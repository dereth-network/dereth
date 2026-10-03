//! The classic interface's Keyboard Configuration page and its keys, over its own key map.
//!
//! The page lists every row of the one set of bindings both interfaces have
//! ([`dereth_input::presentation`]), under the January 2005 page's categories and labels, three
//! key slots a row; the rows that do nothing in this interface come last, under
//! [`presentation::NOT_USED_HEADING`], and bind and clear like the others. A key is a keyboard
//! key held with Shift, Ctrl or Alt or none, and the page shows and captures all of them. Every
//! key the page binds or clears is asked of the host's map at once ([`KeyStoreRequest`]), so
//! nothing is ever left unsaved; the schemes the page lists are this interface's defaults, the
//! retail interface's, and the key map files.
//!
//! The keys themselves are dispatched here: a key fires the action it is bound to in an input map
//! that is live, the stance's combat map only in that stance, as the retail client registers
//! them, and the first of several in the retail client's walk order.
use crate::int::u32_from;
use crate::keystore::{ClassicKeys, KeyStoreRequest, Scheme};
use crate::panels::{HostAction, KeyBinding, KeyboardState};
use dereth_client_contract::actions::{Action, ActionId};
use dereth_input::presentation::{self, map, Interface};
use dereth_input::InputMapId;
use std::collections::BTreeMap;

/// One key of the map as this page and the dispatch read it: a classic key code with its
/// modifiers, bound to an action in an input map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Binding {
    map: u32,
    key: u16,
    chord: u8,
    action: u32,
}

#[derive(Clone, Debug)]
struct Capture {
    action: u32,
    map: u32,
    slot: usize,
    /// The key waiting on the answer to a conflict.
    pending: Option<(u16, u8)>,
    /// A modifier key pressed and not yet let go: let go alone, it is the key captured.
    modifier: Option<u16>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum CaptureResult {
    #[default]
    None,
    Waiting,
    Rejected(String),
    Conflict(String),
    Finished,
    Cancelled,
}

#[derive(Debug, Default)]
pub struct KeyOutcome {
    /// Capture consumes both edges, including the release after a successful bind.
    pub consumed: bool,
    pub capture: CaptureResult,
    pub actions: Vec<Action>,
}

/// The first scheme the page lists: this interface's defaults.
pub const DEFAULT_SCHEME: &str = "Default";
/// The second: the retail interface's defaults.
pub const RETAIL_SCHEME: &str = "Retail Defaults";

#[derive(Debug)]
pub struct KeyBindings {
    keys: ClassicKeys,
    bindings: Vec<Binding>,
    /// What the page asks of the host's map, oldest first. The host carries each out and hands
    /// back the map as it then is ([`Self::set_keys`]).
    pub requests: Vec<KeyStoreRequest>,
    selected: usize,
    capture: Option<Capture>,
    capture_revision: u64,
    swallowed: Vec<u16>,
    held: BTreeMap<u16, Vec<ActionId>>,
    combat_mode: u32,
    pub warning: Option<String>,
}

impl KeyBindings {
    /// The page and the keys over the host's map as it is.
    #[must_use]
    pub fn new(keys: &ClassicKeys) -> Self {
        let mut k = Self {
            keys: ClassicKeys::default(),
            bindings: Vec::new(),
            requests: Vec::new(),
            selected: 0,
            capture: None,
            capture_revision: 0,
            swallowed: Vec::new(),
            held: BTreeMap::new(),
            combat_mode: 1,
            warning: None,
        };
        k.set_keys(keys);
        k
    }

    /// Follow the host's map as it now is.
    pub fn set_keys(&mut self, keys: &ClassicKeys) {
        self.keys = keys.clone();
        self.bindings = keys
            .bindings
            .iter()
            .filter_map(|b| {
                Some(Binding {
                    map: b.map,
                    key: crate::default_keys::virtual_key(b.scan)?,
                    chord: b.modifiers,
                    action: b.action,
                })
            })
            .collect();
        let schemes = self.schemes();
        if self.selected >= schemes.len() {
            self.selected = 0;
        }
        self.end_capture();
    }

    /// The schemes the page lists: the two defaults, then every key map file but this
    /// interface's own.
    fn schemes(&self) -> Vec<String> {
        let mut out = vec![DEFAULT_SCHEME.to_owned(), RETAIL_SCHEME.to_owned()];
        out.extend(
            self.keys
                .files
                .iter()
                .filter(|f| !f.eq_ignore_ascii_case(&self.keys.own_files[1]))
                .cloned(),
        );
        out
    }

    fn scheme_at(&self, index: usize) -> Option<Scheme> {
        match index {
            0 => Some(Scheme::ClassicDefaults),
            1 => Some(Scheme::RetailDefaults),
            _ => self.schemes().get(index).cloned().map(Scheme::File),
        }
    }

    /// Whether the key `vk` held with `modifiers` is bound to `action`.
    #[must_use]
    pub fn bound_to(&self, vk: u16, modifiers: u8, action: u32) -> bool {
        self.bindings
            .iter()
            .any(|b| b.key == vk && b.chord == modifiers && b.action == action)
    }

    /// The chord a key press is looked up under: the modifiers held, when some binding of the key
    /// is held with exactly those; else none, so a modifier held for its own sake (Shift to run)
    /// does not take a plain key away.
    fn chord_for(&self, vk: u16, modifiers: u8) -> u8 {
        if modifiers != 0
            && self
                .bindings
                .iter()
                .any(|b| b.key == vk && b.chord == modifiers && live(b.map, self.combat_mode))
        {
            modifiers
        } else {
            0
        }
    }

    fn conflicting_maps(&self, home: u32) -> Vec<u32> {
        let mut maps = self
            .keys
            .conflicts
            .iter()
            .find(|(m, _)| *m == home)
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        if !maps.contains(&home) {
            maps.push(home);
        }
        maps
    }

    fn is_hold(&self, b: &Binding) -> bool {
        self.keys.holds.contains(&(b.map, b.action))
    }

    /// The keys of one row, in the map's order.
    fn row_keys(&self, action: u32, map: u32) -> Vec<Binding> {
        self.bindings
            .iter()
            .filter(|b| b.action == action && b.map == map)
            .copied()
            .collect()
    }

    pub fn snapshot(&self) -> KeyboardState {
        let header = |label: &str| KeyBinding {
            action: 0,
            map: 0,
            label: label.to_owned(),
            keys: vec!["Key 1".into(), "Key 2".into(), "Key 3".into()],
        };
        let row = |r: &presentation::Row| KeyBinding {
            action: r.action().0,
            map: r.map,
            label: r.label.to_owned(),
            keys: self
                .row_keys(r.action().0, r.map)
                .iter()
                .take(3)
                .map(|b| chord_name(b.key, b.chord))
                .collect(),
        };
        let mut bindings = Vec::new();
        for (index, label) in presentation::CATEGORIES.iter().enumerate() {
            bindings.push(header(label));
            let mut rows: Vec<_> = presentation::ROWS
                .iter()
                .filter(|r| r.category == index && r.not_used(Interface::Classic).is_none())
                .collect();
            rows.sort_by_key(|r| r.label.to_lowercase());
            bindings.extend(rows.into_iter().map(row));
        }
        bindings.push(header(presentation::NOT_USED_HEADING));
        let mut rows: Vec<_> = presentation::ROWS
            .iter()
            .filter(|r| r.not_used(Interface::Classic).is_some())
            .collect();
        rows.sort_by_key(|r| (r.group, r.label.to_lowercase()));
        bindings.extend(rows.into_iter().map(row));
        KeyboardState {
            capture_revision: self.capture_revision,
            warning: self.warning.clone(),
            dirty: false,
            scheme: u32_from(self.selected),
            schemes: self.schemes(),
            bindings,
        }
    }

    fn end_capture(&mut self) {
        if self.capture.take().is_some() {
            self.capture_revision = self.capture_revision.wrapping_add(1);
        }
    }
    pub fn is_capturing(&self) -> bool {
        self.capture.is_some()
    }
    /// 1 peace, 2 melee, 4 missile, 8 magic, matching the live character state.
    pub fn set_combat_mode(&mut self, mode: u32) {
        self.combat_mode = mode;
    }

    fn protected(&self, name: &str) -> bool {
        self.keys
            .own_files
            .iter()
            .any(|f| !f.is_empty() && f.eq_ignore_ascii_case(name))
    }

    /// Returns false for a host action outside this module's ownership.
    pub fn handle(&mut self, action: &HostAction) -> Result<bool, String> {
        match action {
            HostAction::KeyboardScheme(i) => {
                let scheme = self
                    .scheme_at(*i as usize)
                    .ok_or("Unknown keyboard scheme")?;
                self.selected = *i as usize;
                self.requests.push(KeyStoreRequest::Load(scheme));
                self.end_capture();
            }
            HostAction::CaptureBinding { action, map, slot } => {
                if *slot >= 3 || presentation::find(InputMapId(*map), ActionId(*action)).is_none() {
                    return Err("Unknown action or unsupported key slot".into());
                }
                self.capture = Some(Capture {
                    action: *action,
                    map: *map,
                    slot: *slot,
                    pending: None,
                    modifier: None,
                });
            }
            HostAction::CancelBindingCapture => self.end_capture(),
            HostAction::ClearBindingSlot { action, map, slot } => {
                if *slot >= 3 {
                    return Err("Unsupported key slot".into());
                }
                if let Some(b) = self.row_keys(*action, *map).get(*slot).copied() {
                    self.clear(b);
                }
                self.end_capture();
            }
            HostAction::ClearBinding { action, map } => {
                for b in self.row_keys(*action, *map) {
                    self.clear(b);
                }
                self.end_capture();
            }
            HostAction::SaveKeyMapAs { name } => {
                validate_name(name)?;
                if self.schemes().iter().any(|s| s.eq_ignore_ascii_case(name))
                    || self.protected(name)
                {
                    return Err("A keyboard scheme with that name already exists".into());
                }
                self.requests.push(KeyStoreRequest::SaveAs {
                    name: name.clone(),
                    overwrite: false,
                });
                self.keys.files.push(name.clone());
                self.selected = self.schemes().len() - 1;
            }
            HostAction::OverwriteKeyMap { name } => {
                validate_name(name)?;
                let index = self
                    .schemes()
                    .iter()
                    .position(|s| s.eq_ignore_ascii_case(name))
                    .ok_or("Unknown keyboard scheme")?;
                if index < 2 || self.protected(name) {
                    return Err("That keyboard scheme cannot be overwritten".into());
                }
                let name = self.schemes()[index].clone();
                self.requests.push(KeyStoreRequest::SaveAs {
                    name,
                    overwrite: true,
                });
                self.selected = index;
            }
            HostAction::DeleteKeyScheme { name } => {
                let index = self
                    .schemes()
                    .iter()
                    .position(|s| s.eq_ignore_ascii_case(name))
                    .ok_or("Unknown scheme")?;
                if index < 2 || self.protected(name) {
                    return Err("That keyboard scheme cannot be deleted".into());
                }
                let name = self.schemes()[index].clone();
                self.requests.push(KeyStoreRequest::Delete(name.clone()));
                self.keys.files.retain(|f| !f.eq_ignore_ascii_case(&name));
                if self.selected == index {
                    self.selected = 0;
                } else if self.selected > index {
                    self.selected -= 1;
                }
                self.end_capture();
            }
            HostAction::RestoreBindings => {
                // Back to the scheme chosen last, as it is.
                if let Some(scheme) = self.scheme_at(self.selected) {
                    self.requests.push(KeyStoreRequest::Load(scheme));
                }
                self.end_capture();
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// Ask the host to clear one key, and drop it here at once.
    fn clear(&mut self, b: Binding) {
        self.bindings.retain(|x| *x != b);
        if let Some(scan) = crate::keystore::scan_code(b.key) {
            self.requests.push(KeyStoreRequest::Clear {
                scan,
                modifiers: b.chord,
                action: b.action,
                map: b.map,
            });
        }
    }

    /// One key transition. `vk` is the classic key code, never a scan code or Unicode scalar;
    /// `modifiers` are the Shift, Ctrl and Alt held. The host passes `allow_actions` false while
    /// a text box takes the keys.
    pub fn key(
        &mut self,
        vk: u16,
        pressed: bool,
        repeat: bool,
        modifiers: u8,
        allow_actions: bool,
    ) -> Result<KeyOutcome, String> {
        if let Some(index) = self.swallowed.iter().position(|k| *k == vk) {
            if !pressed {
                self.swallowed.remove(index);
            }
            return Ok(KeyOutcome {
                consumed: true,
                ..KeyOutcome::default()
            });
        }
        if self.capture.is_some() {
            return self.capture_key(vk, pressed, repeat, modifiers);
        }
        let mut result = KeyOutcome::default();
        if !pressed {
            if let Some(actions) = self.held.remove(&vk) {
                for action in actions {
                    if !self.held.values().any(|v| v.contains(&action)) {
                        result.actions.push(Action::end(action));
                    }
                }
            }
            return Ok(result);
        }
        if !allow_actions || vk > 255 {
            return Ok(result);
        }
        let chord = self.chord_for(vk, modifiers);
        let Some(b) = self
            .bindings
            .iter()
            .filter(|b| b.key == vk && b.chord == chord && live(b.map, self.combat_mode))
            // A row this interface does nothing with keeps its keys on the page and fires
            // nothing.
            .filter(|b| {
                presentation::find(InputMapId(b.map), ActionId(b.action))
                    .is_some_and(|r| r.not_used(Interface::Classic).is_none())
            })
            .min_by_key(|b| rank(b.map))
            .copied()
        else {
            return Ok(result);
        };
        let id = ActionId(b.action);
        if repeat {
            result.actions.push(Action::repeat(id, 1));
        } else {
            result.actions.push(Action::begin(id));
            if self.is_hold(&b) {
                self.held.entry(vk).or_default().push(id);
            }
        }
        Ok(result)
    }

    fn capture_key(
        &mut self,
        vk: u16,
        pressed: bool,
        repeat: bool,
        modifiers: u8,
    ) -> Result<KeyOutcome, String> {
        let mut result = KeyOutcome {
            consumed: true,
            capture: CaptureResult::Waiting,
            ..KeyOutcome::default()
        };
        if repeat {
            return Ok(result);
        }
        if !pressed {
            // A modifier let go with no other key pressed is the key captured, unmodified.
            let alone = self
                .capture
                .as_ref()
                .is_some_and(|c| c.modifier == Some(vk) && c.pending.is_none());
            if alone {
                result.capture = self.captured(vk, 0)?;
            }
            return Ok(result);
        }
        if vk == 0x1B {
            self.swallowed.push(vk);
            self.end_capture();
            result.capture = CaptureResult::Cancelled;
            return Ok(result);
        }
        if is_modifier(vk) {
            if let Some(c) = self.capture.as_mut() {
                c.modifier = Some(vk);
            }
            return Ok(result);
        }
        self.swallowed.push(vk);
        if reserved(vk) || vk > 255 {
            result.capture =
                CaptureResult::Rejected("That key cannot be used. Please try another.".into());
            return Ok(result);
        }
        result.capture = self.captured(vk, modifiers)?;
        Ok(result)
    }

    /// The key `vk` with `chord` taken for the row being captured: bound at once, or a conflict
    /// to answer first.
    fn captured(&mut self, vk: u16, chord: u8) -> Result<CaptureResult, String> {
        let Some(capture) = self.capture.clone() else {
            return Ok(CaptureResult::None);
        };
        if let Some(c) = self.capture.as_mut() {
            c.modifier = None;
        }
        let maps = self.conflicting_maps(capture.map);
        let conflict = self
            .bindings
            .iter()
            .find(|b| b.key == vk && b.chord == chord && maps.contains(&b.map))
            .copied();
        match conflict {
            Some(b) if b.action == capture.action && b.map == capture.map => {
                self.end_capture();
                Ok(CaptureResult::Finished)
            }
            Some(b) => {
                let label = presentation::find(InputMapId(b.map), ActionId(b.action))
                    .map_or("another command", |r| r.label)
                    .to_owned();
                if let Some(c) = self.capture.as_mut() {
                    c.pending = Some((vk, chord));
                }
                Ok(CaptureResult::Conflict(label))
            }
            None => {
                if let Some(c) = self.capture.as_mut() {
                    c.pending = Some((vk, chord));
                }
                self.confirm_capture(true)
            }
        }
    }

    pub fn confirm_capture(&mut self, accept: bool) -> Result<CaptureResult, String> {
        let Some(capture) = self.capture.clone() else {
            return Ok(CaptureResult::None);
        };
        if !accept {
            if let Some(c) = self.capture.as_mut() {
                c.pending = None;
            }
            return Ok(CaptureResult::Waiting);
        }
        let Some((key, chord)) = capture.pending else {
            return Ok(CaptureResult::Waiting);
        };
        let old = self
            .row_keys(capture.action, capture.map)
            .get(capture.slot)
            .copied();
        let maps = self.conflicting_maps(capture.map);
        self.bindings.retain(|b| {
            !(b.key == key && b.chord == chord && maps.contains(&b.map)) && Some(*b) != old
        });
        let new = Binding {
            map: capture.map,
            key,
            chord,
            action: capture.action,
        };
        self.bindings.push(new);
        if let Some(scan) = crate::keystore::scan_code(key) {
            self.requests.push(KeyStoreRequest::Bind {
                scan,
                modifiers: chord,
                action: capture.action,
                map: capture.map,
                replaced: old
                    .filter(|o| (o.key, o.chord) != (key, chord))
                    .and_then(|o| Some((crate::keystore::scan_code(o.key)?, o.chord))),
            });
        }
        self.end_capture();
        Ok(CaptureResult::Finished)
    }

    /// Release every held action on focus loss.
    pub fn focus_lost(&mut self) -> KeyOutcome {
        let mut actions: Vec<_> = self.held.values().flatten().copied().collect();
        actions.sort();
        actions.dedup();
        self.held.clear();
        self.swallowed.clear();
        KeyOutcome {
            actions: actions.into_iter().map(Action::end).collect(),
            ..KeyOutcome::default()
        }
    }
}

/// Whether the input map `m` is live in the combat mode `mode` (1 peace, 2 melee, 4 missile,
/// 8 magic): a stance's combat map only in that stance, the alternate camera map never (this
/// interface has no alternate camera mode), every other map always.
fn live(m: u32, mode: u32) -> bool {
    match m {
        map::MELEE => mode == 2,
        map::MISSILE => mode == 4,
        map::MAGIC => mode == 8,
        map::CAMERA_ALTERNATE => false,
        _ => true,
    }
}

/// Where an input map comes in the retail client's walk of them, first first: the chat entry's
/// toggle above everything, then the stance's map, registered last, then the rest of the game's.
fn rank(m: u32) -> usize {
    [
        map::TOGGLE_CHAT_ENTRY,
        map::MELEE,
        map::MISSILE,
        map::MAGIC,
        map::CHAT,
        map::QUICKSLOTS,
        map::UI,
        map::ITEM_SELECTION,
        map::CHARACTER_OPTIONS,
        map::COMBAT,
        map::EMOTES,
        map::MOVEMENT,
        map::CAMERA,
        map::OWN,
    ]
    .iter()
    .position(|x| *x == m)
    .unwrap_or(usize::MAX)
}

fn is_modifier(vk: u16) -> bool {
    matches!(vk, 0x10..=0x12 | 0xA0..=0xA5)
}

/// Keys no binding takes: none, and Num Lock, which the keypad's keys depend on.
pub fn reserved(vk: u16) -> bool {
    matches!(vk, 0 | 0x90)
}

fn validate_name(name: &str) -> Result<(), String> {
    let device = matches!(
        name.to_ascii_uppercase().split('.').next().unwrap_or(""),
        "CON" | "PRN" | "AUX" | "NUL"
    ) || {
        let upper = name.to_ascii_uppercase();
        let stem = upper.split('.').next().unwrap_or("");
        (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0'
    };
    if name.is_empty()
        || name.len() >= 80
        || name.trim() != name
        || name.ends_with('.')
        || name
            .chars()
            .any(|c| c.is_control() || "\\?/:*<>|\"".contains(c))
        || device
    {
        Err("Invalid keyboard scheme filename".into())
    } else {
        Ok(())
    }
}

/// A key's name with its modifiers: `Ctrl+W`.
#[must_use]
pub fn chord_name(vk: u16, chord: u8) -> String {
    let mut out = String::new();
    for (bit, name) in [
        (crate::keystore::SHIFT, "Shift+"),
        (crate::keystore::CTRL, "Ctrl+"),
        (crate::keystore::ALT, "Alt+"),
    ] {
        if chord & bit != 0 {
            out.push_str(name);
        }
    }
    out.push_str(&key_name(vk));
    out
}

pub fn key_name(vk: u16) -> String {
    match vk {
        0x41..=0x5A | 0x30..=0x39 => char::from_u32(u32::from(vk)).unwrap_or('?').to_string(),
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        0x60..=0x69 => format!("Keypad {}", vk - 0x60),
        8 => "Backspace".into(),
        9 => "Tab".into(),
        13 => "Enter".into(),
        16 => "Shift".into(),
        17 => "Ctrl".into(),
        18 => "Alt".into(),
        19 => "Pause".into(),
        20 => "Caps Lock".into(),
        27 => "Esc".into(),
        32 => "Spacebar".into(),
        33 => "Page Up".into(),
        34 => "Page Down".into(),
        35 => "End".into(),
        36 => "Home".into(),
        37 => "Left Arrow".into(),
        38 => "Up Arrow".into(),
        39 => "Right Arrow".into(),
        40 => "Down Arrow".into(),
        0x2B => "Keypad Enter".into(),
        45 => "Insert".into(),
        46 => "Delete".into(),
        0x6A => "Keypad *".into(),
        0x6B => "Keypad +".into(),
        0x6D => "Keypad -".into(),
        0x6E => "Keypad .".into(),
        0x6F => "Keypad /".into(),
        0x91 => "Scroll Lock".into(),
        0xA0 => "Left Shift".into(),
        0xA1 => "Right Shift".into(),
        0xA2 => "Left Ctrl".into(),
        0xA3 => "Right Ctrl".into(),
        0xA4 => "Left Alt".into(),
        0xA5 => "Right Alt".into(),
        0xBA => ";".into(),
        0xBB => "=".into(),
        0xBC => ",".into(),
        0xBD => "-".into(),
        0xBE => ".".into(),
        0xBF => "/".into(),
        0xC0 => "`".into(),
        0xDB => "[".into(),
        0xDC => "\\".into(),
        0xDD => "]".into(),
        0xDE => "'".into(),
        _ => format!("VK {vk:02X}"),
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the classic key page's own engine; the map it edits is tested where the
    //! host keeps it).
    use super::*;
    use crate::keystore::{ClassicBinding, CTRL, SHIFT};

    const W: u16 = 0x11;
    const X: u16 = 0x2D;
    const Q: u16 = 0x10;
    const DELETE: u16 = 0x80 | 0x53;
    const FORWARD: u32 = 0x29;
    const BACK: u32 = 0x2A;
    const AUTORUN: u32 = 0x30;

    fn action(name: &str) -> u32 {
        dereth_client_contract::actions::names::action_for_enum_name(name)
            .unwrap()
            .0
    }

    fn keys() -> ClassicKeys {
        let b = |scan, modifiers, action, map| ClassicBinding {
            scan,
            modifiers,
            action,
            map,
        };
        ClassicKeys {
            bindings: vec![
                b(W, 0, FORWARD, map::MOVEMENT),
                b(X, 0, BACK, map::MOVEMENT),
                b(Q, 0, AUTORUN, map::MOVEMENT),
                b(DELETE, 0, action("CombatLowAttack"), map::MELEE),
                b(DELETE, 0, action("CombatAimLow"), map::MISSILE),
                b(DELETE, 0, action("CombatPrevSpell"), map::MAGIC),
            ],
            files: vec!["dereth".into(), "dereth-classic".into(), "pvp".into()],
            own_files: ["dereth".into(), "dereth-classic".into()],
            conflicts: vec![(map::MOVEMENT, vec![map::MOVEMENT])],
            holds: vec![(map::MOVEMENT, FORWARD), (map::MOVEMENT, BACK)],
        }
    }

    fn bind(k: &mut KeyBindings, action: u32, map: u32, slot: usize, vk: u16, m: u8) -> KeyOutcome {
        k.handle(&HostAction::CaptureBinding { action, map, slot })
            .unwrap();
        k.key(vk, true, false, m, true).unwrap()
    }

    #[test]
    fn a_captured_key_is_asked_of_the_map_and_replaces_the_slot_it_was_given() {
        let mut k = KeyBindings::new(&keys());
        assert_eq!(
            bind(&mut k, FORWARD, map::MOVEMENT, 0, 0x45, 0).capture,
            CaptureResult::Finished
        );
        assert!(k.key(0x45, false, false, 0, true).unwrap().consumed);
        assert_eq!(
            k.requests,
            [KeyStoreRequest::Bind {
                scan: 0x12,
                modifiers: 0,
                action: FORWARD,
                map: map::MOVEMENT,
                replaced: Some((W, 0)),
            }]
        );
        assert_eq!(
            k.key(0x45, true, false, 0, true).unwrap().actions,
            [Action::begin(ActionId(FORWARD))]
        );
        assert!(k
            .key(0x57, true, false, 0, true)
            .unwrap()
            .actions
            .is_empty());
    }

    /// Behaviour: keys.classic.the-page-captures-a-key-with-its-modifiers
    #[test]
    fn capture_records_the_modifiers_held_and_a_modifier_let_go_alone() {
        let mut k = KeyBindings::new(&keys());
        k.handle(&HostAction::CaptureBinding {
            action: BACK,
            map: map::MOVEMENT,
            slot: 1,
        })
        .unwrap();
        // Ctrl held, then W: Ctrl+W, which nothing else has, though W walks forward.
        assert_eq!(
            k.key(0xA2, true, false, CTRL, true).unwrap().capture,
            CaptureResult::Waiting
        );
        assert_eq!(
            k.key(0x57, true, false, CTRL, true).unwrap().capture,
            CaptureResult::Finished
        );
        k.key(0x57, false, false, CTRL, true).unwrap();
        k.key(0xA2, false, false, 0, true).unwrap();
        assert_eq!(
            k.snapshot()
                .bindings
                .iter()
                .find(|b| b.action == BACK)
                .unwrap()
                .keys,
            ["X", "Ctrl+W"]
        );
        // Left Shift alone, as the run key is bound.
        k.handle(&HostAction::CaptureBinding {
            action: AUTORUN,
            map: map::MOVEMENT,
            slot: 0,
        })
        .unwrap();
        k.key(0xA0, true, false, SHIFT, true).unwrap();
        assert_eq!(
            k.key(0xA0, false, false, 0, true).unwrap().capture,
            CaptureResult::Finished
        );
        assert!(k.bound_to(0xA0, 0, AUTORUN));
    }

    #[test]
    fn a_key_bound_with_a_modifier_fires_held_with_it_and_plain_keys_still_work_with_one_held() {
        let mut keys = keys();
        keys.bindings.push(ClassicBinding {
            scan: W,
            modifiers: CTRL,
            action: BACK,
            map: map::MOVEMENT,
        });
        let mut k = KeyBindings::new(&keys);
        assert_eq!(
            k.key(0x57, true, false, CTRL, true).unwrap().actions,
            [Action::begin(ActionId(BACK))]
        );
        k.key(0x57, false, false, CTRL, true).unwrap();
        assert_eq!(
            k.key(0x57, true, false, SHIFT, true).unwrap().actions,
            [Action::begin(ActionId(FORWARD))],
            "W with Shift held (to run) still walks forward"
        );
    }

    #[test]
    fn a_combat_key_fires_its_stance_action_only_in_that_stance() {
        let mut k = KeyBindings::new(&keys());
        assert!(k
            .key(0x2E, true, false, 0, true)
            .unwrap()
            .actions
            .is_empty());
        k.key(0x2E, false, false, 0, true).unwrap();
        for (mode, name) in [
            (2, "CombatLowAttack"),
            (4, "CombatAimLow"),
            (8, "CombatPrevSpell"),
        ] {
            k.set_combat_mode(mode);
            assert_eq!(
                k.key(0x2E, true, false, 0, true).unwrap().actions,
                [Action::begin(ActionId(action(name)))]
            );
            k.key(0x2E, false, false, 0, true).unwrap();
        }
    }

    #[test]
    fn a_held_action_ends_with_its_key_and_a_one_shot_does_not() {
        let mut k = KeyBindings::new(&keys());
        k.key(0x57, true, false, 0, true).unwrap();
        assert_eq!(
            k.key(0x57, false, false, 0, true).unwrap().actions,
            [Action::end(ActionId(FORWARD))]
        );
        k.key(0x51, true, false, 0, true).unwrap();
        assert!(k
            .key(0x51, false, false, 0, true)
            .unwrap()
            .actions
            .is_empty());
        k.key(0x57, true, false, 0, true).unwrap();
        assert_eq!(k.focus_lost().actions, [Action::end(ActionId(FORWARD))]);
    }

    #[test]
    fn a_cleared_key_is_asked_of_the_map_to_stay_cleared() {
        let mut k = KeyBindings::new(&keys());
        k.handle(&HostAction::ClearBindingSlot {
            action: FORWARD,
            map: map::MOVEMENT,
            slot: 0,
        })
        .unwrap();
        assert_eq!(
            k.requests,
            [KeyStoreRequest::Clear {
                scan: W,
                modifiers: 0,
                action: FORWARD,
                map: map::MOVEMENT,
            }]
        );
        assert!(k
            .key(0x57, true, false, 0, true)
            .unwrap()
            .actions
            .is_empty());
    }

    #[test]
    fn conflict_requires_confirmation_before_displacing_another_action() {
        let mut k = KeyBindings::new(&keys());
        assert_eq!(
            bind(&mut k, FORWARD, map::MOVEMENT, 0, 0x58, 0).capture,
            CaptureResult::Conflict("Walk Backwards".into())
        );
        assert_eq!(k.confirm_capture(false).unwrap(), CaptureResult::Waiting);
        k.key(0x58, false, false, 0, true).unwrap();
        assert!(k.requests.is_empty());
        assert!(matches!(
            k.key(0x58, true, false, 0, true).unwrap().capture,
            CaptureResult::Conflict(_)
        ));
        k.confirm_capture(true).unwrap();
        assert!(k.bound_to(0x58, 0, FORWARD) && !k.bound_to(0x58, 0, BACK));
    }

    #[test]
    fn escape_cancels_capture_and_the_permanent_keys_can_be_bound() {
        let mut k = KeyBindings::new(&keys());
        for vk in [0x09, 0x0D, 0x31, 0x70] {
            assert_eq!(
                bind(&mut k, FORWARD, map::MOVEMENT, 2, vk, 0).capture,
                CaptureResult::Finished
            );
            k.key(vk, false, false, 0, true).unwrap();
        }
        k.handle(&HostAction::CaptureBinding {
            action: FORWARD,
            map: map::MOVEMENT,
            slot: 0,
        })
        .unwrap();
        assert_eq!(
            k.key(0x1B, true, false, 0, true).unwrap().capture,
            CaptureResult::Cancelled
        );
        assert!(!k.is_capturing());
        assert!(matches!(
            bind(&mut k, FORWARD, map::MOVEMENT, 0, 0x90, 0).capture,
            CaptureResult::Rejected(_)
        ));
    }

    #[test]
    fn the_schemes_are_the_two_defaults_and_the_files_but_this_interfaces_own() {
        let mut k = KeyBindings::new(&keys());
        assert_eq!(
            k.snapshot().schemes,
            [DEFAULT_SCHEME, RETAIL_SCHEME, "dereth", "pvp"]
        );
        k.handle(&HostAction::KeyboardScheme(1)).unwrap();
        k.handle(&HostAction::KeyboardScheme(2)).unwrap();
        k.handle(&HostAction::KeyboardScheme(0)).unwrap();
        k.handle(&HostAction::SaveKeyMapAs {
            name: "My Keys".into(),
        })
        .unwrap();
        k.handle(&HostAction::DeleteKeyScheme { name: "pvp".into() })
            .unwrap();
        assert_eq!(
            k.requests,
            [
                KeyStoreRequest::Load(Scheme::RetailDefaults),
                KeyStoreRequest::Load(Scheme::File("dereth".into())),
                KeyStoreRequest::Load(Scheme::ClassicDefaults),
                KeyStoreRequest::SaveAs {
                    name: "My Keys".into(),
                    overwrite: false
                },
                KeyStoreRequest::Delete("pvp".into()),
            ]
        );
        for name in ["dereth", "dereth-classic", "Default"] {
            assert!(k
                .handle(&HostAction::OverwriteKeyMap { name: name.into() })
                .is_err());
            assert!(k
                .handle(&HostAction::DeleteKeyScheme { name: name.into() })
                .is_err());
        }
        assert!(k
            .handle(&HostAction::SaveKeyMapAs {
                name: "dereth-classic".into()
            })
            .is_err());
    }

    #[test]
    fn the_page_lists_every_row_and_the_ones_not_used_here_last() {
        let page = KeyBindings::new(&keys()).snapshot();
        let rows = page.bindings.iter().filter(|b| b.action != 0).count();
        assert_eq!(rows, presentation::ROWS.len());
        let heading = page
            .bindings
            .iter()
            .position(|b| b.label == presentation::NOT_USED_HEADING)
            .unwrap();
        assert_eq!(page.bindings.len() - heading - 1, 18);
        let combat: Vec<&str> = page
            .bindings
            .iter()
            .skip_while(|b| b.label != "Combat Keys")
            .skip(1)
            .take_while(|b| b.action != 0)
            .map(|b| b.label.as_str())
            .collect();
        assert_eq!(
            combat.len(),
            32,
            "the fifteen stance keys each a row of its own"
        );
        assert!(combat.contains(&"Attack Low") && combat.contains(&"Previous Spell"));
    }

    #[test]
    fn filenames_cannot_escape_the_folder_or_use_windows_device_names() {
        for bad in [
            "../escape",
            "x/y",
            "x\\y",
            "C:foo",
            "CON",
            "NUL.map",
            "COM1",
            "trail.",
            " leading",
            "a\nb",
            "",
        ] {
            assert!(validate_name(bad).is_err(), "{bad}");
        }
        assert!(validate_name("My keyboard 2").is_ok());
        assert!(validate_name("COMMANDS").is_ok());
    }

    #[test]
    fn key_names_distinguish_sides_and_the_keypad_and_carry_their_modifiers() {
        for (key, expected) in [
            (0xA0, "Left Shift"),
            (0xA5, "Right Alt"),
            (0x2B, "Keypad Enter"),
            (0x60, "Keypad 0"),
            (0x20, "Spacebar"),
        ] {
            assert_eq!(key_name(key), expected);
        }
        assert_eq!(chord_name(0x52, CTRL), "Ctrl+R");
        assert_eq!(chord_name(0x31, SHIFT | CTRL), "Shift+Ctrl+1");
    }
}
