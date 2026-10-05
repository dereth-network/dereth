//! The classic interface's Keyboard Configuration page and its keys, over its own key map.
//!
//! The page lists the rows of the one set of bindings both interfaces have
//! ([`dereth_input::presentation`]) that this interface acts on, under the January 2005 page's
//! categories and labels, three key slots a row. A key is a keyboard
//! key held with Shift, Ctrl or Alt or none, and the page shows and captures all of them. Every
//! key the page binds or clears is asked of the host's map at once ([`KeyStoreRequest`]), so
//! nothing is ever left unsaved; the schemes the page lists are "Default", this interface's
//! defaults, and this interface's own saved key maps.
//!
//! Dispatch belongs to the shared input manager; this module owns the editor projection and capture dialogs.
use crate::int::u32_from;
use crate::keystore::{ClassicKeys, KeyStoreRequest, Scheme};
use crate::panels::{HostAction, KeyBinding, KeyboardState};
use dereth_client_contract::actions::{Action, ActionId};
#[cfg(test)]
use dereth_input::presentation::map;
use dereth_input::presentation::{self, Interface};
use dereth_input::InputMapId;

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

#[derive(Debug)]
pub struct KeyBindings {
    keys: ClassicKeys,
    bindings: Vec<Binding>,
    /// What the page asks of the host's map, oldest first. The host carries each out and hands
    /// back the map as it then is ([`Self::set_keys`]).
    pub requests: Vec<KeyStoreRequest>,
    /// The scheme chosen last, by name; `None` for the defaults. Kept by name, so a list that
    /// changes under it (a save, a delete) keeps the same one chosen.
    selected: Option<String>,
    capture: Option<Capture>,
    capture_revision: u64,
    swallowed: Vec<u16>,
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
            selected: None,
            capture: None,
            capture_revision: 0,
            swallowed: Vec::new(),
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
        if let Some(name) = &self.selected {
            if !self.keys.files.iter().any(|f| f.eq_ignore_ascii_case(name)) {
                self.selected = None;
            }
        }
        self.end_capture();
    }

    /// The schemes the page lists: "Default", this interface's defaults, then this interface's
    /// saved key maps.
    fn schemes(&self) -> Vec<String> {
        let mut out = vec![DEFAULT_SCHEME.to_owned()];
        out.extend(self.keys.files.iter().cloned());
        out
    }

    fn scheme_at(&self, index: usize) -> Option<Scheme> {
        match index {
            0 => Some(Scheme::Default),
            _ => self.keys.files.get(index - 1).cloned().map(Scheme::File),
        }
    }

    fn selected_index(&self) -> usize {
        self.selected
            .as_ref()
            .and_then(|n| {
                self.keys
                    .files
                    .iter()
                    .position(|f| f.eq_ignore_ascii_case(n))
            })
            .map_or(0, |i| i + 1)
    }

    fn selected_scheme(&self) -> Scheme {
        self.selected.clone().map_or(Scheme::Default, Scheme::File)
    }

    /// Whether the key `vk` held with `modifiers` is bound to `action`.
    #[must_use]
    pub fn bound_to(&self, vk: u16, modifiers: u8, action: u32) -> bool {
        self.bindings
            .iter()
            .any(|b| b.key == vk && b.chord == modifiers && b.action == action)
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
            map: r.map.0,
            label: r.label.to_owned(),
            keys: self
                .row_keys(r.action().0, r.map.0)
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
        KeyboardState {
            capture_revision: self.capture_revision,
            warning: self.warning.clone(),
            dirty: false,
            scheme: u32_from(self.selected_index()),
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
    pub fn captures_key(&self, vk: u16) -> bool {
        self.capture.is_some() || self.swallowed.contains(&vk)
    }

    /// The name of the key map in use, or the defaults' row: neither is overwritten or deleted.
    fn protected(&self, name: &str) -> bool {
        name.eq_ignore_ascii_case(DEFAULT_SCHEME)
            || (!self.keys.active.is_empty() && self.keys.active.eq_ignore_ascii_case(name))
    }

    /// Returns false for a host action outside this module's ownership.
    pub fn handle(&mut self, action: &HostAction) -> Result<bool, String> {
        match action {
            HostAction::KeyboardScheme(i) => {
                let scheme = self
                    .scheme_at(*i as usize)
                    .ok_or("Unknown keyboard scheme")?;
                self.selected = match &scheme {
                    Scheme::Default => None,
                    Scheme::File(name) => Some(name.clone()),
                };
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
                self.keys.files.sort_by_key(|f| f.to_lowercase());
                self.selected = Some(name.clone());
            }
            HostAction::OverwriteKeyMap { name } => {
                validate_name(name)?;
                let name = self
                    .keys
                    .files
                    .iter()
                    .find(|s| s.eq_ignore_ascii_case(name))
                    .cloned()
                    .ok_or("Unknown keyboard scheme")?;
                if self.protected(&name) {
                    return Err("That keyboard scheme cannot be overwritten".into());
                }
                self.requests.push(KeyStoreRequest::SaveAs {
                    name: name.clone(),
                    overwrite: true,
                });
                self.selected = Some(name);
            }
            HostAction::DeleteKeyScheme { name } => {
                let name = self
                    .keys
                    .files
                    .iter()
                    .find(|s| s.eq_ignore_ascii_case(name))
                    .cloned()
                    .ok_or("Unknown scheme")?;
                if self.protected(&name) {
                    return Err("That keyboard scheme cannot be deleted".into());
                }
                self.requests.push(KeyStoreRequest::Delete(name.clone()));
                self.keys.files.retain(|f| !f.eq_ignore_ascii_case(&name));
                if self
                    .selected
                    .as_ref()
                    .is_some_and(|s| s.eq_ignore_ascii_case(&name))
                {
                    self.selected = None;
                }
                self.end_capture();
            }
            HostAction::RestoreBindings => {
                // Back to the scheme chosen last, as it is.
                self.requests
                    .push(KeyStoreRequest::Load(self.selected_scheme()));
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
    /// `modifiers` are the Shift, Ctrl and Alt held. Only binding capture consumes transitions here.
    pub fn key(
        &mut self,
        vk: u16,
        pressed: bool,
        repeat: bool,
        modifiers: u8,
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
        Ok(KeyOutcome::default())
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

    /// End capture and forget its pending release when focus is lost.
    pub fn focus_lost(&mut self) -> KeyOutcome {
        self.end_capture();
        self.swallowed.clear();
        KeyOutcome::default()
    }
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
    use dereth_input::{ControlChord, ControlCode, MasterInputMap, SubControlIndex};
    let Some(scan) = crate::keystore::scan_code(vk) else {
        return classic_name(vk);
    };
    let control = |scan| ControlCode::new(0, SubControlIndex::None, scan);
    let map = MasterInputMap {
        meta_keys: vec![
            (control(0x2A), 0x8000_0000),
            (control(0x1D), 0x4000_0000),
            (control(0x38), 0x2000_0000),
        ],
        ..MasterInputMap::default()
    };
    dereth_input::labels::binding_label(
        &map,
        ControlChord::new(
            control(scan),
            crate::keystore::meta_of_modifiers(chord),
            dereth_input::spec::activation::CLICK,
        ),
        &ClassicLabels,
    )
}

struct ClassicLabels;
impl dereth_input::labels::LabelProvider for ClassicLabels {
    fn resolve_token(&self, _: u32, _: &str) -> Option<String> {
        None
    }
    fn format_subcontrol(&self, _: &str, _: &str) -> Option<String> {
        None
    }
    fn delimiter(&self) -> &str {
        "+"
    }
    fn modifier_order(&self) -> &[u32] {
        &[0x8000_0000, 0x4000_0000, 0x2000_0000]
    }
    fn override_name(
        &self,
        device: dereth_input::DeviceType,
        control: dereth_input::ControlCode,
        meta: bool,
    ) -> Option<String> {
        if device != dereth_input::DeviceType::Keyboard {
            return None;
        }
        let vk = crate::default_keys::virtual_key(control.offset())?;
        Some(if meta {
            match vk {
                0xA0 | 0xA1 => "Shift".into(),
                0xA2 | 0xA3 => "Ctrl".into(),
                0xA4 | 0xA5 => "Alt".into(),
                _ => classic_name(vk),
            }
        } else {
            classic_name(vk)
        })
    }
}

pub fn key_name(vk: u16) -> String {
    chord_name(vk, 0)
}

fn classic_name(vk: u16) -> String {
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
                b(W, 0, FORWARD, map::MOVEMENT.0),
                b(X, 0, BACK, map::MOVEMENT.0),
                b(Q, 0, AUTORUN, map::MOVEMENT.0),
                b(DELETE, 0, action("CombatLowAttack"), map::MELEE.0),
                b(DELETE, 0, action("CombatAimLow"), map::MISSILE.0),
                b(DELETE, 0, action("CombatPrevSpell"), map::MAGIC.0),
            ],
            files: vec!["pvp".into(), "wer".into()],
            active: "dereth".into(),
            conflicts: vec![(map::MOVEMENT.0, vec![map::MOVEMENT.0])],
            holds: vec![(map::MOVEMENT.0, FORWARD), (map::MOVEMENT.0, BACK)],
        }
    }

    fn bind(k: &mut KeyBindings, action: u32, map: u32, slot: usize, vk: u16, m: u8) -> KeyOutcome {
        k.handle(&HostAction::CaptureBinding { action, map, slot })
            .unwrap();
        k.key(vk, true, false, m).unwrap()
    }

    #[test]
    fn a_captured_key_is_asked_of_the_map_and_replaces_the_slot_it_was_given() {
        let mut k = KeyBindings::new(&keys());
        assert_eq!(
            bind(&mut k, FORWARD, map::MOVEMENT.0, 0, 0x45, 0).capture,
            CaptureResult::Finished
        );
        assert!(k.key(0x45, false, false, 0).unwrap().consumed);
        assert_eq!(
            k.requests,
            [KeyStoreRequest::Bind {
                scan: 0x12,
                modifiers: 0,
                action: FORWARD,
                map: map::MOVEMENT.0,
                replaced: Some((W, 0)),
            }]
        );
        assert!(k.bound_to(0x45, 0, FORWARD));
        assert!(k.key(0x57, true, false, 0).unwrap().actions.is_empty());
    }

    /// Behaviour: keys.classic.the-page-captures-a-key-with-its-modifiers
    #[test]
    fn capture_records_the_modifiers_held_and_a_modifier_let_go_alone() {
        let mut k = KeyBindings::new(&keys());
        k.handle(&HostAction::CaptureBinding {
            action: BACK,
            map: map::MOVEMENT.0,
            slot: 1,
        })
        .unwrap();
        // Ctrl held, then W: Ctrl+W, which nothing else has, though W walks forward.
        assert_eq!(
            k.key(0xA2, true, false, CTRL).unwrap().capture,
            CaptureResult::Waiting
        );
        assert_eq!(
            k.key(0x57, true, false, CTRL).unwrap().capture,
            CaptureResult::Finished
        );
        k.key(0x57, false, false, CTRL).unwrap();
        k.key(0xA2, false, false, 0).unwrap();
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
            map: map::MOVEMENT.0,
            slot: 0,
        })
        .unwrap();
        k.key(0xA0, true, false, SHIFT).unwrap();
        assert_eq!(
            k.key(0xA0, false, false, 0).unwrap().capture,
            CaptureResult::Finished
        );
        assert!(k.bound_to(0xA0, 0, AUTORUN));
    }

    #[test]
    fn a_cleared_key_is_asked_of_the_map_to_stay_cleared() {
        let mut k = KeyBindings::new(&keys());
        k.handle(&HostAction::ClearBindingSlot {
            action: FORWARD,
            map: map::MOVEMENT.0,
            slot: 0,
        })
        .unwrap();
        assert_eq!(
            k.requests,
            [KeyStoreRequest::Clear {
                scan: W,
                modifiers: 0,
                action: FORWARD,
                map: map::MOVEMENT.0,
            }]
        );
        assert!(k.key(0x57, true, false, 0).unwrap().actions.is_empty());
    }

    #[test]
    fn conflict_requires_confirmation_before_displacing_another_action() {
        let mut k = KeyBindings::new(&keys());
        assert_eq!(
            bind(&mut k, FORWARD, map::MOVEMENT.0, 0, 0x58, 0).capture,
            CaptureResult::Conflict("Walk Backwards".into())
        );
        assert_eq!(k.confirm_capture(false).unwrap(), CaptureResult::Waiting);
        k.key(0x58, false, false, 0).unwrap();
        assert!(k.requests.is_empty());
        assert!(matches!(
            k.key(0x58, true, false, 0).unwrap().capture,
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
                bind(&mut k, FORWARD, map::MOVEMENT.0, 2, vk, 0).capture,
                CaptureResult::Finished
            );
            k.key(vk, false, false, 0).unwrap();
        }
        k.handle(&HostAction::CaptureBinding {
            action: FORWARD,
            map: map::MOVEMENT.0,
            slot: 0,
        })
        .unwrap();
        assert_eq!(
            k.key(0x1B, true, false, 0).unwrap().capture,
            CaptureResult::Cancelled
        );
        assert!(!k.is_capturing());
        assert!(matches!(
            bind(&mut k, FORWARD, map::MOVEMENT.0, 0, 0x90, 0).capture,
            CaptureResult::Rejected(_)
        ));
    }

    #[test]
    fn the_schemes_are_the_default_and_this_interfaces_saved_maps() {
        let mut k = KeyBindings::new(&keys());
        assert_eq!(k.snapshot().schemes, [DEFAULT_SCHEME, "pvp", "wer"]);
        k.handle(&HostAction::KeyboardScheme(1)).unwrap();
        k.handle(&HostAction::KeyboardScheme(0)).unwrap();
        k.handle(&HostAction::DeleteKeyScheme { name: "pvp".into() })
            .unwrap();
        assert_eq!(
            k.requests,
            [
                KeyStoreRequest::Load(Scheme::File("pvp".into())),
                KeyStoreRequest::Load(Scheme::Default),
                KeyStoreRequest::Delete("pvp".into()),
            ]
        );
        for name in ["dereth", "Default"] {
            assert!(k
                .handle(&HostAction::OverwriteKeyMap { name: name.into() })
                .is_err());
            assert!(k
                .handle(&HostAction::DeleteKeyScheme { name: name.into() })
                .is_err());
            assert!(k
                .handle(&HostAction::SaveKeyMapAs { name: name.into() })
                .is_err());
        }
    }

    /// Behaviour: keys.classic.a-scheme-saved-under-a-name-is-the-one-chosen-after
    #[test]
    fn a_scheme_saved_under_a_name_stays_chosen_when_the_list_comes_back_sorted() {
        let mut k = KeyBindings::new(&keys());
        k.handle(&HostAction::SaveKeyMapAs {
            name: "bananas".into(),
        })
        .unwrap();
        assert_eq!(
            k.snapshot().schemes,
            [DEFAULT_SCHEME, "bananas", "pvp", "wer"]
        );
        assert_eq!(k.snapshot().scheme, 1, "bananas, at once");
        // The host writes the file and hands the list back as the folder holds it.
        let mut back = keys();
        back.files = vec!["bananas".into(), "pvp".into(), "wer".into()];
        k.set_keys(&back);
        let page = k.snapshot();
        assert_eq!(page.schemes[page.scheme as usize], "bananas");
        k.handle(&HostAction::RestoreBindings).unwrap();
        assert_eq!(
            k.requests.last(),
            Some(&KeyStoreRequest::Load(Scheme::File("bananas".into())))
        );
    }

    #[test]
    fn the_page_lists_the_rows_this_interface_acts_on_and_no_others() {
        let page = KeyBindings::new(&keys()).snapshot();
        let rows = page.bindings.iter().filter(|b| b.action != 0).count();
        assert_eq!(rows, presentation::ROWS.len() - 18);
        assert_eq!(
            page.bindings.iter().filter(|b| b.action == 0).count(),
            presentation::CATEGORIES.len(),
            "a heading for each category and no other"
        );
        assert!(!page.bindings.iter().any(|b| b.label.contains("alternate")));
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
