//! January 2005 keyboard schemes: virtual keys and six-field .map files.
//!
//! A key map file is a count, then one row per binding of six fields: the input map, the key,
//! the chord, the command name, the command type and the analog type. Capturing a key binds map 0,
//! chord 0 and asks before displacing a key. These are not modern InputManager
//! maps or its release-capture policy. Only the public action vocabulary is shared.
use crate::int::u32_from;
use crate::panels::{HostAction, KeyBinding, KeyboardState};
use dereth_client_contract::actions::{names, Action, ActionId, ActionPhase};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Deserialize)]
pub struct Command {
    pub id: u32,
    pub name: String,
    pub flags: u32,
    pub category: usize,
    pub label: String,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Catalogue {
    pub categories: Vec<String>,
    pub actions: Vec<Command>,
    #[serde(default)]
    pub fixed: Vec<FixedCommand>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct FixedCommand {
    pub label: String,
    pub key: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Binding {
    map: u32,
    key: u16,
    chord: u8,
    action: u32,
    command_type: u8,
    analog_type: u8,
}
#[derive(Clone, Debug)]
struct Scheme {
    name: String,
    bindings: Vec<Binding>,
    saved: Vec<Binding>,
}
#[derive(Clone, Debug)]
struct Capture {
    action: u32,
    map: u32,
    slot: usize,
    pending_key: Option<u16>,
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
    pub legacy_commands: Vec<LegacyCommand>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyCommand {
    pub name: String,
    pub phase: ActionPhase,
}

#[derive(Debug)]
pub struct KeyBindings {
    catalogue: Catalogue,
    directory: PathBuf,
    schemes: Vec<Scheme>,
    selected: usize,
    capture: Option<Capture>,
    capture_revision: u64,
    swallowed: Vec<u16>,
    held: BTreeMap<u16, Vec<ActionId>>,
    held_legacy: BTreeMap<u16, Vec<String>>,
    combat_mode: u32,
    pub warning: Option<String>,
}

impl KeyBindings {
    /// The key schemes over `catalogue`. `defaults` is the text of the default key map, if there
    /// is one (or why it could not be read): its scheme is "Default". Without one the first scheme
    /// is "Unbound" and binds nothing. The player's own schemes (`*.map`) and the name of the
    /// scheme in use (`current.txt`) live in `directory`.
    pub fn load(
        catalogue: Catalogue,
        directory: &Path,
        defaults: Option<Result<String, String>>,
    ) -> Result<Self, String> {
        if catalogue.actions.is_empty() {
            return Err("The keyboard command catalogue is empty".into());
        }
        fs::create_dir_all(directory).map_err(err)?;
        let default_result = defaults
            .map(|text| text.and_then(|s| parse_map(&s, &catalogue)))
            .transpose();
        let (defaults, default_error) = match default_result {
            Ok(defaults) => (defaults, None),
            Err(error) => (None, Some(error)),
        };
        let mut schemes = vec![Scheme {
            name: if defaults.is_some() {
                "Default"
            } else {
                "Unbound"
            }
            .into(),
            bindings: defaults.clone().unwrap_or_default(),
            saved: defaults.clone().unwrap_or_default(),
        }];
        let mut files = fs::read_dir(directory)
            .map_err(err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(err)?;
        files.sort_by_key(|e| e.file_name());
        for file in files {
            let path = file.path();
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("map"))
            {
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or("Non-Unicode scheme name")?;
                validate_name(name)?;
                if schemes.iter().any(|s| s.name.eq_ignore_ascii_case(name)) {
                    continue;
                }
                let text = fs::read_to_string(&path).map_err(err)?;
                let bindings =
                    parse_map(&text, &catalogue).map_err(|e| format!("{}: {e}", path.display()))?;
                schemes.push(Scheme {
                    name: name.into(),
                    saved: bindings.clone(),
                    bindings,
                });
            }
        }
        let current = fs::read_to_string(directory.join("current.txt")).unwrap_or_default();
        let selected = schemes
            .iter()
            .position(|s| s.name.eq_ignore_ascii_case(current.trim()))
            .unwrap_or(0);
        let warning = defaults.is_none().then(|| {
            let reason = default_error.map_or_else(|| "unavailable".into(), |e| format!("unusable ({e})"));
            format!("The default key map is {reason}; the Unbound scheme binds no keys. Bind keys and Save As to create a scheme.")
        });
        Ok(Self {
            catalogue,
            directory: directory.into(),
            schemes,
            selected,
            capture: None,
            capture_revision: 0,
            swallowed: vec![],
            held: BTreeMap::new(),
            held_legacy: BTreeMap::new(),
            combat_mode: 1,
            warning,
        })
    }

    pub fn snapshot(&self) -> KeyboardState {
        let mut commands: Vec<_> = self.catalogue.actions.iter().collect();
        commands.sort_by_key(|c| (c.category, c.label.to_lowercase()));
        let mut bindings = Vec::new();
        for (category, label) in self.catalogue.categories.iter().enumerate() {
            bindings.push(KeyBinding {
                action: 0,
                map: 0,
                label: label.clone(),
                keys: vec!["Key 1".into(), "Key 2".into(), "Key 3".into()],
            });
            for c in commands.iter().filter(|c| c.category == category) {
                bindings.push(KeyBinding {
                    action: c.id,
                    map: 0,
                    label: c.label.clone(),
                    keys: self
                        .scheme()
                        .bindings
                        .iter()
                        .filter(|b| b.action == c.id && b.map == 0 && b.chord == 0)
                        .take(3)
                        .map(|b| key_name(b.key))
                        .collect(),
                });
            }
        }
        bindings.extend(self.catalogue.fixed.iter().map(|c| KeyBinding {
            action: u32::MAX,
            map: 0,
            label: c.label.clone(),
            keys: vec![c.key.clone()],
        }));
        KeyboardState {
            capture_revision: self.capture_revision,
            warning: self.warning.clone(),
            dirty: self.scheme().bindings != self.scheme().saved,
            scheme: u32_from(self.selected),
            schemes: self.schemes.iter().map(|s| s.name.clone()).collect(),
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
    pub fn category(&self, action: u32) -> Option<&str> {
        self.command(action)
            .and_then(|c| self.catalogue.categories.get(c.category))
            .map(String::as_str)
    }
    fn command(&self, id: u32) -> Option<&Command> {
        self.catalogue.actions.iter().find(|c| c.id == id)
    }
    fn scheme(&self) -> &Scheme {
        &self.schemes[self.selected]
    }

    /// Returns false for a host action outside this module's ownership.
    pub fn handle(&mut self, action: &HostAction) -> Result<bool, String> {
        match action {
            HostAction::KeyboardScheme(i) => {
                if *i as usize >= self.schemes.len() {
                    return Err("Unknown keyboard scheme".into());
                }
                self.schemes[self.selected].bindings = self.scheme().saved.clone();
                self.selected = *i as usize;
                self.schemes[self.selected].bindings = self.scheme().saved.clone();
                self.end_capture();
                self.persist_selection()?;
            }
            HostAction::CaptureBinding { action, map, slot } => {
                if *map != 0 || *slot >= 3 || self.command(*action).is_none() {
                    return Err("Unknown action or unsupported classic key slot".into());
                }
                self.capture = Some(Capture {
                    action: *action,
                    map: *map,
                    slot: *slot,
                    pending_key: None,
                });
            }
            HostAction::CancelBindingCapture => self.end_capture(),
            HostAction::ClearBindingSlot { action, map, slot } => {
                if *map != 0 || *slot >= 3 {
                    return Err("Unsupported classic key slot".into());
                }
                let key = self
                    .scheme()
                    .bindings
                    .iter()
                    .filter(|b| b.action == *action && b.map == *map && b.chord == 0)
                    .nth(*slot)
                    .map(|b| b.key);
                if let Some(key) = key {
                    self.schemes[self.selected]
                        .bindings
                        .retain(|b| !(b.map == *map && b.key == key && b.chord == 0));
                }
                self.end_capture();
            }
            HostAction::ClearBinding { action, map } => {
                self.schemes[self.selected]
                    .bindings
                    .retain(|b| !(b.map == *map && b.action == *action));
                self.end_capture();
            }
            HostAction::SaveKeyMapAs { name } => {
                validate_name(name)?;
                if self
                    .schemes
                    .iter()
                    .any(|s| s.name.eq_ignore_ascii_case(name))
                {
                    return Err("A keyboard scheme with that name already exists".into());
                }
                let bindings = self.scheme().bindings.clone();
                fs::write(
                    self.directory.join(format!("{name}.map")),
                    write_map(&bindings, &self.catalogue)?,
                )
                .map_err(err)?;
                self.schemes.push(Scheme {
                    name: name.clone(),
                    saved: bindings.clone(),
                    bindings,
                });
                self.selected = self.schemes.len() - 1;
                self.persist_selection()?;
            }
            HostAction::OverwriteKeyMap { name } => {
                validate_name(name)?;
                let index = self
                    .schemes
                    .iter()
                    .position(|s| s.name.eq_ignore_ascii_case(name))
                    .ok_or("Unknown keyboard scheme")?;
                if index == 0 {
                    return Err("The base keyboard scheme cannot be overwritten".into());
                }
                let bindings = self.scheme().bindings.clone();
                // Write successfully before replacing the in-memory target selection.
                fs::write(
                    self.directory
                        .join(format!("{}.map", self.schemes[index].name)),
                    write_map(&bindings, &self.catalogue)?,
                )
                .map_err(err)?;
                self.schemes[index].saved = bindings.clone();
                self.schemes[index].bindings = bindings;
                self.selected = index;
                self.persist_selection()?;
            }
            HostAction::DeleteKeyScheme { name } => {
                let index = self
                    .schemes
                    .iter()
                    .position(|s| s.name.eq_ignore_ascii_case(name))
                    .ok_or("Unknown scheme")?;
                if index == 0 {
                    return Err("The base keyboard scheme cannot be deleted".into());
                }
                fs::remove_file(
                    self.directory
                        .join(format!("{}.map", self.schemes[index].name)),
                )
                .map_err(err)?;
                self.schemes.remove(index);
                if self.selected == index {
                    self.selected = 0;
                } else if self.selected > index {
                    self.selected -= 1;
                }
                self.end_capture();
                self.persist_selection()?;
            }
            HostAction::RestoreBindings => {
                self.schemes[self.selected].bindings = self.scheme().saved.clone();
                self.end_capture();
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
    fn persist_selection(&self) -> Result<(), String> {
        fs::write(
            self.directory.join("current.txt"),
            format!("{}\n", self.scheme().name),
        )
        .map_err(err)
    }
    /// `vk` is the Windows virtual key, never a scan-code or Unicode scalar.
    /// The capture page creates unmodified keyboard-down entries only. Existing
    /// files retain all five input types, all eight chords, and both payload fields.
    /// Host passes allow_actions=false for editable text; a generic modal alone does not disable movement maps.
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
            let mut result = KeyOutcome {
                consumed: true,
                capture: CaptureResult::Waiting,
                ..KeyOutcome::default()
            };
            if pressed && !repeat {
                self.swallowed.push(vk);
                if vk == 0x1B {
                    self.end_capture();
                    result.capture = CaptureResult::Cancelled;
                } else if reserved(vk) || vk > 255 {
                    result.capture = CaptureResult::Rejected(
                        "That key cannot be used. Please try another.".into(),
                    );
                } else {
                    let capture = self.capture.as_ref().unwrap();
                    let conflict = self
                        .scheme()
                        .bindings
                        .iter()
                        .find(|b| b.map == 0 && b.key == vk && b.chord == 0);
                    if conflict.is_some_and(|b| b.action == capture.action) {
                        self.end_capture();
                        result.capture = CaptureResult::Finished;
                    } else if let Some(binding) = conflict {
                        let label = self
                            .command(binding.action)
                            .map_or("another command", |c| c.label.as_str())
                            .to_owned();
                        self.capture.as_mut().unwrap().pending_key = Some(vk);
                        result.capture = CaptureResult::Conflict(label);
                    } else {
                        self.capture.as_mut().unwrap().pending_key = Some(vk);
                        self.confirm_capture(true)?;
                        result.capture = CaptureResult::Finished;
                    }
                }
            }
            return Ok(result);
        }
        let mut result = KeyOutcome::default();
        if !pressed {
            if let Some(commands) = self.held_legacy.remove(&vk) {
                result.legacy_commands.extend(
                    commands
                        .into_iter()
                        .filter(|name| !self.held_legacy.values().any(|v| v.contains(name)))
                        .map(|name| LegacyCommand {
                            name,
                            phase: ActionPhase::End,
                        }),
                );
            }
            if let Some(actions) = self.held.remove(&vk) {
                for action in actions {
                    if !self.held.values().any(|v| v.contains(&action))
                        && !self
                            .scheme()
                            .bindings
                            .iter()
                            .any(|b| b.map == 1 && b.key == vk && b.chord == modifiers)
                    {
                        result.actions.push(Action::end(action));
                    }
                }
            }
            // An explicit up entry overrides the automatic end in the old dispatcher.
            if allow_actions {
                result
                    .actions
                    .extend(self.resolve(1, vk, modifiers, repeat));
                result
                    .legacy_commands
                    .extend(self.resolve_legacy(1, vk, modifiers, repeat));
            }
            return Ok(result);
        }
        if !allow_actions || vk > 255 {
            return Ok(result);
        }
        result.actions = self.resolve(0, vk, modifiers, repeat);
        result.legacy_commands = self.resolve_legacy(0, vk, modifiers, repeat);
        if !repeat {
            let held: Vec<_> = self
                .scheme()
                .bindings
                .iter()
                .filter(|b| {
                    b.map == 0 && b.key == vk && b.chord == modifiers && b.command_type & 9 == 0
                })
                .filter_map(|b| self.command(b.action))
                .filter(|c| c.flags & 0x04000000 != 0)
                .filter_map(|c| runtime_action_in_mode(&c.name, self.combat_mode))
                .collect();
            if !held.is_empty() {
                self.held.insert(vk, held);
            }
            let commands = self
                .scheme()
                .bindings
                .iter()
                .filter(|b| {
                    b.map == 0 && b.key == vk && b.chord == modifiers && b.command_type & 9 == 0
                })
                .filter_map(|b| self.command(b.action))
                .filter(|c| {
                    c.flags & 0x04000000 != 0
                        && runtime_action_in_mode(&c.name, self.combat_mode).is_none()
                })
                .map(|c| c.name.clone())
                .collect::<Vec<_>>();
            if !commands.is_empty() {
                self.held_legacy.insert(vk, commands);
            }
        }
        Ok(result)
    }
    fn resolve(&self, map: u32, key: u16, chord: u8, repeat: bool) -> Vec<Action> {
        self.scheme()
            .bindings
            .iter()
            .filter(|b| b.map == map && b.key == key && b.chord == chord && b.command_type & 1 == 0)
            .filter_map(|b| {
                self.command(b.action)
                    .and_then(|c| runtime_action_in_mode(&c.name, self.combat_mode))
                    .map(|id| {
                        if b.command_type & 8 != 0 {
                            Action::end(id)
                        } else if repeat {
                            Action::repeat(id, 1)
                        } else {
                            Action::begin(id)
                        }
                    })
            })
            .collect()
    }
    fn resolve_legacy(&self, map: u32, key: u16, chord: u8, repeat: bool) -> Vec<LegacyCommand> {
        self.scheme()
            .bindings
            .iter()
            .filter(|b| b.map == map && b.key == key && b.chord == chord && b.command_type & 1 == 0)
            .filter_map(|b| {
                self.command(b.action)
                    .filter(|c| runtime_action_in_mode(&c.name, self.combat_mode).is_none())
                    .map(|c| LegacyCommand {
                        name: c.name.clone(),
                        phase: if b.command_type & 8 != 0 {
                            ActionPhase::End
                        } else if repeat {
                            ActionPhase::Repeat
                        } else {
                            ActionPhase::Begin
                        },
                    })
            })
            .collect()
    }
    pub fn confirm_capture(&mut self, accept: bool) -> Result<CaptureResult, String> {
        let Some(capture) = self.capture.clone() else {
            return Ok(CaptureResult::None);
        };
        if !accept {
            self.capture.as_mut().unwrap().pending_key = None;
            return Ok(CaptureResult::Waiting);
        }
        let Some(key) = capture.pending_key else {
            return Ok(CaptureResult::Waiting);
        };
        let old = self
            .scheme()
            .bindings
            .iter()
            .filter(|b| b.map == capture.map && b.action == capture.action && b.chord == 0)
            .nth(capture.slot)
            .map(|b| b.key);
        let bindings = &mut self.schemes[self.selected].bindings;
        bindings.retain(|b| {
            !(b.map == capture.map && b.chord == 0 && (b.key == key || Some(b.key) == old))
        });
        bindings.push(Binding {
            map: capture.map,
            key,
            chord: 0,
            action: capture.action,
            command_type: 0,
            analog_type: 0,
        });
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
        let mut commands: Vec<_> = self.held_legacy.values().flatten().cloned().collect();
        commands.sort();
        commands.dedup();
        self.held_legacy.clear();
        KeyOutcome {
            actions: actions.into_iter().map(Action::end).collect(),
            legacy_commands: commands
                .into_iter()
                .map(|name| LegacyCommand {
                    name,
                    phase: ActionPhase::End,
                })
                .collect(),
            ..KeyOutcome::default()
        }
    }
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
/// Keys the classic interface keeps for itself and never binds: Tab, Enter, Escape, the number
/// row (the shortcut bar), F1 (Help) and Num Lock.
pub fn reserved(vk: u16) -> bool {
    matches!(vk, 0 | 9 | 13 | 0x1B | 0x30..=0x39 | 0x70 | 0x90)
}
fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.len() >= 80
        || name.trim() != name
        || name.ends_with('.')
        || name
            .chars()
            .any(|c| c.is_control() || "\\?/:*<>|\"".contains(c))
        || matches!(
            name.to_ascii_uppercase().split('.').next().unwrap_or(""),
            "CON"
                | "PRN"
                | "AUX"
                | "NUL"
                | "COM1"
                | "COM2"
                | "COM3"
                | "COM4"
                | "COM5"
                | "COM6"
                | "COM7"
                | "COM8"
                | "COM9"
                | "LPT1"
                | "LPT2"
                | "LPT3"
                | "LPT4"
                | "LPT5"
                | "LPT6"
                | "LPT7"
                | "LPT8"
                | "LPT9"
        )
    {
        Err("Invalid keyboard scheme filename".into())
    } else {
        Ok(())
    }
}
fn parse_map(text: &str, catalogue: &Catalogue) -> Result<Vec<Binding>, String> {
    let mut tokens = text.split_whitespace();
    let count: usize = tokens
        .next()
        .ok_or("Missing keymap count")?
        .parse()
        .map_err(err)?;
    if count > 8 * (256 + 256 + 6 + 6 + 22) {
        return Err("Excessive keymap count".into());
    }
    let mut bindings = Vec::with_capacity(count);
    for _ in 0..count {
        let fields: Vec<_> = tokens.by_ref().take(6).collect();
        if fields.len() != 6 {
            return Err("Truncated keymap row".into());
        }
        let map: u32 = fields[0].parse().map_err(err)?;
        let key: u16 = fields[1].parse().map_err(err)?;
        let chord: u8 = fields[2].parse().map_err(err)?;
        let command_type: u8 = fields[4].parse().map_err(err)?;
        let analog_type: u8 = fields[5].parse().map_err(err)?;
        let limit = match map {
            0 | 1 => 256,
            2 | 3 => 6,
            4 => 22,
            _ => 0,
        };
        if key >= limit || chord >= 8 || command_type >= 16 {
            return Err("Keymap field outside source bounds".into());
        }
        // Only bindable command names are read; an older or unknown row is
        // consumed and ignored without invalidating the remaining map.
        let Some(action) = catalogue
            .actions
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(fields[3]))
            .map(|c| c.id)
        else {
            continue;
        };
        // A reserved keyboard key keeps its fixed meaning: a row binding one is ignored.
        if map == 0 && reserved(key) {
            continue;
        }
        let row = Binding {
            map,
            key,
            chord,
            action,
            command_type,
            analog_type,
        };
        if let Some(old) = bindings
            .iter_mut()
            .find(|b: &&mut Binding| b.map == map && b.key == key && b.chord == chord)
        {
            *old = row;
        } else {
            bindings.push(row);
        }
    }
    let tail = tokens.collect::<Vec<_>>();
    if !tail.is_empty()
        && tail
            != [
                "InputType",
                "Input",
                "Chording",
                "Command",
                "CommandType",
                "AnalogType",
            ]
    {
        return Err("Unexpected text after keymap rows".into());
    }
    Ok(bindings)
}
fn write_map(bindings: &[Binding], catalogue: &Catalogue) -> Result<String, String> {
    let mut text = format!("{}\n", bindings.len());
    let mut rows: Vec<_> = bindings.iter().collect();
    rows.sort_by_key(|b| (b.map, b.key, b.chord));
    for b in rows {
        let command = catalogue
            .actions
            .iter()
            .find(|c| c.id == b.action)
            .ok_or("Unknown action in keymap")?;
        text += &format!(
            "{}\t{}\t{}\t{}\t{}\t{}\n",
            b.map, b.key, b.chord, command.name, b.command_type, b.analog_type
        );
    }
    text += "InputType\tInput\tChording\tCommand\tCommandType\tAnalogType\n";
    Ok(text)
}

pub fn key_name(vk: u16) -> String {
    match vk {
        0x41..=0x5A | 0x30..=0x39 => char::from_u32(vk as u32).unwrap().to_string(),
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

/// Explicit name translation prevents the old motion-command ordinals from
/// accidentally invoking an unrelated modern action. Unmapped actions stay inert.
pub fn runtime_action(name: &str) -> Option<ActionId> {
    // The classic auto-run is the shared run-lock action.
    if name == "AutoRun" {
        return Some(ActionId(0x30));
    }
    let translated = match name {
        "WalkForward" => "MovementForward",
        "WalkBackwards" => "MovementBackup",
        "TurnRight" => "MovementTurnRight",
        "TurnLeft" => "MovementTurnLeft",
        "SideStepRight" => "MovementStrafeRight",
        "SideStepLeft" => "MovementStrafeLeft",
        "Jump" => "MovementJump",
        "ResetView" => "CameraViewDefault",
        "CameraLeftRotate" => "CameraRotateLeft",
        "CameraRightRotate" => "CameraRotateRight",
        "CameraRaise" => "CameraRotateUp",
        "CameraLower" => "CameraRotateDown",
        "CameraCloser" => "CameraMoveToward",
        "CameraFarther" => "CameraMoveAway",
        "FloorView" => "CameraViewLookDown",
        "FirstPersonView" => "CameraViewFirstPerson",
        "MapView" => "CameraViewMapMode",
        "UseSelected" => "USE",
        // Moving the selection to the backpack is the final client's pick-up action.
        "AutosortSelected" => "SelectionPickUp",
        "DropSelected" => "SelectionDrop",
        "GiveSelected" => "SelectionGive",
        "SplitSelected" => "SelectionSplitStack",
        "ExamineSelected" => "SelectionExamine",
        "SelectSelf" => "SelectionSelf",
        "ToggleCombat" => "CombatToggleCombat",
        "OptionsPanel" => "ToggleOptionsPanel",
        "AllegiancePanel" => "ToggleAllegiancePanel",
        "FellowshipPanel" => "ToggleFellowshipPanel",
        "SpellbookPanel" => "ToggleSpellbookPanel",
        "SpellComponentsPanel" => "ToggleSpellComponentsPanel",
        "CharacterTitlePanel" => "ToggleCharacterTitlePanel",
        "ContractsPanel" => "ToggleContractsPanel",
        "HousePanel" => "ToggleHousePanel",
        "AttributesPanel" => "ToggleAttributesPanel",
        "SkillsPanel" => "ToggleSkillsPanel",
        "MapPanel" => "ToggleMapPanel",
        "InventoryPanel" => "ToggleInventoryPanel",
        "CharacterOptionsPanel" => "ToggleCharacterOptionsPanel",
        "SoundAndGraphicsPanel" => "ToggleConfigOptionsPanel",
        "HelpfulSpellsPanel" => "TogglePositiveEffectsPanel",
        "HarmfulSpellsPanel" => "ToggleNegativeEffectsPanel",
        "CharacterInformationPanel" => "ToggleCharacterInfoPanel",
        "LinkStatusPanel" => "ToggleLinkStatusPanel",
        "VitaePanel" => "ToggleVitaePanel",
        "CaptureScreenshotToFile" => "CaptureScreenshot",
        "AutoRepeatAttacks" => "PlayerOption_AutoRepeatAttack",
        "AutoTarget" => "PlayerOption_AutoTarget",
        "AdvancedCombatInterface" => "PlayerOption_AdvancedCombatUI",
        "IgnoreAllegianceRequests" => "PlayerOption_IgnoreAllegianceRequests",
        "IgnoreFellowshipRequests" => "PlayerOption_IgnoreFellowshipRequests",
        "LetPlayersGiveYouItems" => "PlayerOption_AllowGive",
        "AutoTrackCombatTargets" => "PlayerOption_ViewCombatTarget",
        "DisplayTooltips" => "PlayerOption_ShowTooltips",
        "AttemptToDeceivePlayers" => "PlayerOption_UseDeception",
        "RunAsDefaultMovement" => "PlayerOption_ToggleRun",
        "StayInChatModeAfterSend" => "PlayerOption_StayInChatMode",
        "VividTargetIndicator" => "PlayerOption_VividTargetingIndicator",
        "ShareFellowshipXP" => "PlayerOption_FellowshipShareXP",
        "ShareFellowshipLoot" => "PlayerOption_FellowshipShareLoot",
        "AcceptCorpseLooting" => "PlayerOption_AcceptLootPermits",
        "IgnoreTradeRequests" => "PlayerOption_IgnoreTradeRequests",
        "DisableWeather" => "PlayerOption_DisableMostWeatherEffects",
        "DisableHouseEffect" => "PlayerOption_DisableHouseRestrictionEffects",
        "ShowRadarCoordinates" => "PlayerOption_CoordinatesOnRadar",
        "ShowSpellDurations" => "PlayerOption_SpellDuration",
        "AutomaticallyAcceptFellowshipRequests" => "PlayerOption_FellowshipAutoAcceptRequests",
        "ToggleCraftingChanceOfSuccessDialog" => "PlayerOption_UseCraftSuccessDialog",
        "AllegianceChat" => "PlayerOption_HearAllegianceChat",
        name => name,
    };
    if let Some(action) = names::action_for_enum_name(translated) {
        return Some(action);
    }
    if matches!(
        name,
        "PreviousCompassItem"
            | "NextCompassItem"
            | "ClosestCompassItem"
            | "PreviousSelection"
            | "LastAttacker"
            | "PreviousFellow"
            | "NextFellow"
            | "PreviousItem"
            | "NextItem"
            | "ClosestItem"
            | "NextMonster"
            | "PreviousMonster"
            | "ClosestMonster"
            | "NextPlayer"
            | "PreviousPlayer"
            | "ClosestPlayer"
            | "UseClosestUnopenedCorpse"
            | "UseNextUnopenedCorpse"
    ) {
        return names::action_for_enum_name(&format!("Selection{name}"));
    }
    None
}

/// The five shared combat keys are selected by stance in 0050B0xx/0050BBxx.
/// Keep the action chosen on key-down in `held`, so changing stance while held
/// still sends the release to the action which actually began.
pub fn runtime_action_in_mode(name: &str, mode: u32) -> Option<ActionId> {
    let index = match name {
        "HighAttack" => 0,
        "MediumAttack" => 1,
        "LowAttack" => 2,
        "DecreasePowerSetting" => 3,
        "IncreasePowerSetting" => 4,
        _ => return runtime_action(name),
    };
    let names = match mode {
        2 => [
            "CombatHighAttack",
            "CombatMediumAttack",
            "CombatLowAttack",
            "CombatDecreaseAttackPower",
            "CombatIncreaseAttackPower",
        ],
        4 => [
            "CombatAimHigh",
            "CombatAimMedium",
            "CombatAimLow",
            "CombatDecreaseMissileAccuracy",
            "CombatIncreaseMissileAccuracy",
        ],
        8 => [
            "CombatNextSpell",
            "CombatCastCurrentSpell",
            "CombatPrevSpell",
            "CombatPrevSpellTab",
            "CombatNextSpellTab",
        ],
        _ => return None,
    };
    dereth_client_contract::actions::names::action_for_enum_name(names[index])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture {
        dir: PathBuf,
        catalogue: PathBuf,
        defaults: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!(
                "dereth-classic-keybindings-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&dir).unwrap();
            let catalogue = dir.join("catalogue.json");
            fs::write(&catalogue, r#"{"categories":["Movement"],"actions":[
                {"id":5,"name":"WalkForward","flags":1157627909,"category":0,"label":"Walk Forward"},
                {"id":6,"name":"WalkBackwards","flags":1157627910,"category":0,"label":"Walk Backwards"},
                {"id":177,"name":"ToggleCombat","flags":150995121,"category":0,"label":"Combat Mode"}
            ]}"#).unwrap();
            let defaults = dir.join("original.map");
            fs::write(
                &defaults,
                "2\n0 87 0 WalkForward 0 0\n0 83 0 WalkBackwards 0 0\n",
            )
            .unwrap();
            Self {
                dir,
                catalogue,
                defaults,
            }
        }
        fn catalogue(&self) -> Catalogue {
            serde_json::from_slice(&fs::read(&self.catalogue).unwrap()).unwrap()
        }
        fn load(&self) -> KeyBindings {
            KeyBindings::load(
                self.catalogue(),
                &self.dir.join("state"),
                Some(fs::read_to_string(&self.defaults).map_err(|e| e.to_string())),
            )
            .unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
    fn bind(k: &mut KeyBindings, action: u32, slot: usize, key: u16) -> KeyOutcome {
        k.handle(&HostAction::CaptureBinding {
            action,
            map: 0,
            slot,
        })
        .unwrap();
        k.key(key, true, false, 0, true).unwrap()
    }
    #[test]
    fn legacy_map_round_trip_retains_devices_chords_and_payload_bits() {
        let f = Fixture::new();
        let k = f.load();
        let text = "5\n0 87 7 WalkForward 8 255\n1 87 7 WalkBackwards 0 0\n2 5 1 ToggleCombat 0 4\n3 5 2 WalkForward 3 5\n4 21 3 WalkBackwards 2 7\nInputType Input Chording Command CommandType AnalogType\n";
        let first = parse_map(text, &k.catalogue).unwrap();
        let again = parse_map(&write_map(&first, &k.catalogue).unwrap(), &k.catalogue).unwrap();
        assert_eq!(first, again);
        assert!(parse_map("1 0 256 0 WalkForward 0 0", &k.catalogue).is_err());
        assert!(parse_map("1 0 87 8 WalkForward 0 0", &k.catalogue).is_err());
        assert!(parse_map("1 0 87 0 WalkForward 0", &k.catalogue).is_err());
        assert!(parse_map("0 unexpected", &k.catalogue).is_err());
    }
    #[test]
    fn capture_replaces_only_selected_slot_and_survives_restart() {
        let f = Fixture::new();
        let mut k = f.load();
        assert_eq!(bind(&mut k, 5, 0, 0x51).capture, CaptureResult::Finished);
        assert!(k.key(0x51, false, false, 0, true).unwrap().consumed);
        assert!(k.snapshot().dirty);
        assert_eq!(f.load().scheme().name, "Default");
        assert_eq!(f.load().scheme().bindings[0].key, 0x57);
        k.handle(&HostAction::SaveKeyMapAs {
            name: "Custom".into(),
        })
        .unwrap();
        assert!(!k.snapshot().dirty);
        let mut restarted = f.load();
        assert_eq!(restarted.scheme().name, "Custom");
        assert!(restarted
            .key(0x57, true, false, 0, true)
            .unwrap()
            .actions
            .is_empty());
        assert_eq!(
            restarted.key(0x51, true, false, 0, true).unwrap().actions,
            vec![Action::begin(ActionId(41))]
        );
        assert_eq!(
            fs::read_to_string(&f.defaults).unwrap(),
            "2\n0 87 0 WalkForward 0 0\n0 83 0 WalkBackwards 0 0\n"
        );
    }
    #[test]
    fn conflict_requires_confirmation_before_displacing_another_action() {
        let f = Fixture::new();
        let mut k = f.load();
        assert_eq!(
            bind(&mut k, 5, 0, 0x53).capture,
            CaptureResult::Conflict("Walk Backwards".into())
        );
        assert_eq!(k.scheme().bindings.len(), 2);
        assert_eq!(k.confirm_capture(false).unwrap(), CaptureResult::Waiting);
        k.key(0x53, false, false, 0, true).unwrap();
        assert!(matches!(
            k.key(0x53, true, false, 0, true).unwrap().capture,
            CaptureResult::Conflict(_)
        ));
        k.confirm_capture(true).unwrap();
        assert_eq!(k.scheme().bindings.len(), 1);
        assert_eq!(k.scheme().bindings[0].action, 5);
    }
    #[test]
    fn reserved_keys_do_not_end_capture_or_fire_actions() {
        let f = Fixture::new();
        let mut k = f.load();
        for key in [9, 13, 0x30, 0x39, 0x70, 0x90] {
            let outcome = bind(&mut k, 5, 0, key);
            assert!(matches!(outcome.capture, CaptureResult::Rejected(_)));
            assert!(outcome.actions.is_empty());
            k.key(key, false, false, 0, true).unwrap();
        }
        assert_eq!(
            k.key(0x1B, true, false, 0, true).unwrap().capture,
            CaptureResult::Cancelled
        );
        assert!(!k.is_capturing());
    }
    #[test]
    fn unchanged_capture_and_escape_still_notify_the_editor_without_dirtying_the_scheme() {
        let f = Fixture::new();
        let mut k = f.load();
        assert_eq!(k.snapshot().capture_revision, 0);
        assert_eq!(bind(&mut k, 5, 0, 0x57).capture, CaptureResult::Finished);
        assert_eq!(k.snapshot().capture_revision, 1);
        assert!(!k.snapshot().dirty);
        k.key(0x57, false, false, 0, true).unwrap();
        k.handle(&HostAction::CaptureBinding {
            action: 5,
            map: 0,
            slot: 0,
        })
        .unwrap();
        assert_eq!(
            k.key(27, true, false, 0, true).unwrap().capture,
            CaptureResult::Cancelled
        );
        assert_eq!(k.snapshot().capture_revision, 2);
        assert!(!k.snapshot().dirty);
    }
    #[test]
    fn source_key_names_distinguish_sides_and_keypad_enter() {
        for (key, expected) in [
            (0xA0, "Left Shift"),
            (0xA1, "Right Shift"),
            (0xA2, "Left Ctrl"),
            (0xA3, "Right Ctrl"),
            (0xA4, "Left Alt"),
            (0xA5, "Right Alt"),
            (0x2B, "Keypad Enter"),
            (0x60, "Keypad 0"),
            (0x6B, "Keypad +"),
            (0x20, "Spacebar"),
            (0x25, "Left Arrow"),
        ] {
            assert_eq!(key_name(key), expected);
        }
    }
    #[test]
    fn capture_uses_mapped_key_and_zero_chord_even_with_native_modifiers() {
        let f = Fixture::new();
        let mut k = f.load();
        for key in [0xA0, 0x51] {
            k.handle(&HostAction::CaptureBinding {
                action: 5,
                map: 0,
                slot: 0,
            })
            .unwrap();
            let outcome = k.key(key, true, false, 255, true).unwrap();
            assert_eq!(outcome.capture, CaptureResult::Finished);
            assert!(outcome.actions.is_empty());
            let binding = k.scheme().bindings.iter().find(|b| b.action == 5).unwrap();
            assert_eq!((binding.map, binding.key, binding.chord), (0, key, 0));
            assert!(k.key(key, false, false, 255, true).unwrap().consumed);
            assert_eq!(
                k.key(key, true, false, 0, true).unwrap().actions,
                vec![Action::begin(ActionId(41))]
            );
            k.key(key, false, false, 0, true).unwrap();
        }
    }
    #[test]
    fn three_slots_compact_after_removing_one_and_other_maps_survive() {
        let f = Fixture::new();
        let mut k = f.load();
        bind(&mut k, 5, 1, 0x51);
        k.key(0x51, false, false, 0, true).unwrap();
        bind(&mut k, 5, 2, 0x45);
        k.key(0x45, false, false, 0, true).unwrap();
        k.schemes[k.selected].bindings.push(Binding {
            map: 2,
            key: 1,
            chord: 3,
            action: 5,
            command_type: 0,
            analog_type: 42,
        });
        k.handle(&HostAction::ClearBindingSlot {
            action: 5,
            map: 0,
            slot: 1,
        })
        .unwrap();
        let row = k
            .snapshot()
            .bindings
            .into_iter()
            .find(|b| b.action == 5)
            .unwrap();
        assert_eq!(row.keys, ["W", "E"]);
        k.handle(&HostAction::SaveKeyMapAs {
            name: "Three slots".into(),
        })
        .unwrap();
        let loaded = f.load();
        assert!(loaded
            .scheme()
            .bindings
            .iter()
            .any(|b| b.map == 2 && b.analog_type == 42));
    }
    #[test]
    fn save_delete_switch_and_reset_are_persistent() {
        let f = Fixture::new();
        let mut k = f.load();
        k.handle(&HostAction::SaveKeyMapAs {
            name: "My Keys".into(),
        })
        .unwrap();
        k.handle(&HostAction::ClearBinding { action: 5, map: 0 })
            .unwrap();
        assert_eq!(k.scheme().bindings.len(), 1);
        assert!(k.snapshot().dirty);
        assert_eq!(f.load().scheme().bindings.len(), 2);
        k.handle(&HostAction::RestoreBindings).unwrap();
        assert!(!k.snapshot().dirty);
        assert_eq!(k.scheme().bindings.len(), 2);
        assert_eq!(f.load().scheme().bindings.len(), 2);
        k.handle(&HostAction::DeleteKeyScheme {
            name: "My Keys".into(),
        })
        .unwrap();
        assert_eq!(f.load().scheme().name, "Default");
        assert!(k
            .handle(&HostAction::DeleteKeyScheme {
                name: "Default".into()
            })
            .is_err());
    }
    #[test]
    fn filenames_cannot_escape_state_directory_or_use_windows_device_names() {
        for bad in [
            "../escape",
            "x/y",
            "x\\y",
            "C:foo",
            "CON",
            "NUL.map",
            "trail.",
            " leading",
            "a\nb",
            "",
        ] {
            assert!(validate_name(bad).is_err(), "{bad}");
        }
        assert!(validate_name("My keyboard 2").is_ok());
    }
    #[test]
    fn held_movement_releases_when_typing_begins_or_focus_is_lost() {
        let f = Fixture::new();
        let mut k = f.load();
        assert_eq!(
            k.key(0x57, true, false, 0, true).unwrap().actions,
            vec![Action::begin(ActionId(41))]
        );
        assert_eq!(
            k.key(0x57, false, false, 0, false).unwrap().actions,
            vec![Action::end(ActionId(41))]
        );
        assert!(k
            .key(0x57, true, false, 0, false)
            .unwrap()
            .actions
            .is_empty());
        k.key(0x57, true, false, 0, true).unwrap();
        assert_eq!(k.focus_lost().actions, vec![Action::end(ActionId(41))]);
        assert!(k
            .key(0x57, false, false, 0, true)
            .unwrap()
            .actions
            .is_empty());
    }
    #[test]
    fn absent_original_defaults_remain_explicitly_unbound() {
        let f = Fixture::new();
        let mut k = KeyBindings::load(f.catalogue(), &f.dir.join("state"), None).unwrap();
        assert_eq!(k.scheme().name, "Unbound");
        assert!(k.warning.is_some());
        assert!(k
            .snapshot()
            .bindings
            .iter()
            .filter(|b| b.action > 0 && b.action != u32::MAX)
            .all(|b| b.keys.is_empty()));
        assert!(k.handle(&HostAction::RestoreBindings).is_ok());
        assert_eq!(bind(&mut k, 5, 0, 0x57).capture, CaptureResult::Finished);
    }
    #[test]
    fn malformed_defaults_do_not_hide_saved_schemes_or_abort_configuration() {
        let f = Fixture::new();
        let mut original = f.load();
        bind(&mut original, 5, 0, 0x58);
        original
            .handle(&HostAction::SaveKeyMapAs {
                name: "Custom".into(),
            })
            .unwrap();
        fs::write(&f.defaults, "2\n0 87 truncated").unwrap();
        let mut restored = f.load();
        assert!(restored
            .warning
            .as_deref()
            .is_some_and(|s| s.contains("unusable")));
        assert!(restored.snapshot().schemes.iter().any(|s| s == "Custom"));
        assert_eq!(
            restored.key(0x58, true, false, 0, true).unwrap().actions,
            vec![Action::begin(ActionId(41))]
        );
        assert!(restored.handle(&HostAction::RestoreBindings).is_ok());
    }
    #[test]
    fn translation_uses_names_and_live_combat_mode_not_old_ordinals() {
        assert_eq!(runtime_action("WalkForward"), Some(ActionId(41)));
        assert_eq!(runtime_action("Ready"), Some(ActionId(0x10000094)));
        assert_eq!(runtime_action("HoldSidestep"), None);
        assert_eq!(
            runtime_action_in_mode("HighAttack", 8),
            Some(ActionId(0x10000062))
        );
        assert_eq!(
            runtime_action_in_mode("HighAttack", 4),
            Some(ActionId(0x100000f3))
        );
        assert_eq!(
            runtime_action_in_mode("HighAttack", 2),
            Some(ActionId(0x1000005f))
        );
        assert_eq!(runtime_action_in_mode("HighAttack", 1), None);
    }
    #[test]
    fn untranslatable_held_commands_keep_a_private_release_path() {
        let f = Fixture::new();
        let mut k = f.load();
        k.catalogue.actions.push(Command {
            id: 1,
            name: "HoldRun".into(),
            flags: 0x85000001,
            category: 0,
            label: "Hold Run".into(),
        });
        k.schemes[0].bindings.push(Binding {
            map: 0,
            key: 16,
            chord: 0,
            action: 1,
            command_type: 0,
            analog_type: 0,
        });
        let pressed = k.key(16, true, false, 0, true).unwrap();
        assert!(pressed.actions.is_empty());
        assert_eq!(
            pressed.legacy_commands,
            vec![LegacyCommand {
                name: "HoldRun".into(),
                phase: ActionPhase::Begin
            }]
        );
        assert_eq!(
            k.focus_lost().legacy_commands,
            vec![LegacyCommand {
                name: "HoldRun".into(),
                phase: ActionPhase::End
            }]
        );
        assert!(k
            .key(16, false, false, 0, true)
            .unwrap()
            .legacy_commands
            .is_empty());
    }
    #[test]
    fn explicit_overwrite_replaces_only_the_named_custom_scheme_and_keeps_base_immutable() {
        let f = Fixture::new();
        let mut k = f.load();
        k.handle(&HostAction::SaveKeyMapAs {
            name: "Target".into(),
        })
        .unwrap();
        k.handle(&HostAction::SaveKeyMapAs {
            name: "Source".into(),
        })
        .unwrap();
        k.handle(&HostAction::ClearBinding { action: 5, map: 0 })
            .unwrap();
        let expected = k.scheme().bindings.clone();
        assert!(k
            .handle(&HostAction::SaveKeyMapAs {
                name: "target".into()
            })
            .is_err());
        k.handle(&HostAction::OverwriteKeyMap {
            name: "tArGeT".into(),
        })
        .unwrap();
        assert_eq!(k.scheme().name, "Target");
        assert_eq!(f.load().scheme().bindings, expected);
        assert_eq!(k.schemes.len(), 3);
        assert!(k
            .handle(&HostAction::OverwriteKeyMap {
                name: "Default".into()
            })
            .is_err());
        assert_eq!(k.schemes[0].bindings.len(), 2);
    }
    #[test]
    fn older_unknown_commands_are_skipped_without_losing_following_bindings() {
        let f = Fixture::new();
        let k = f.load();
        let rows = parse_map(
            "3\n0 87 0 WalkForward 0 0\n0 118 0 RetiredCommand 0 0\n0 83 0 WalkBackwards 0 0\n",
            &k.catalogue,
        )
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[0].key, rows[1].key), (87, 83));
        assert!(parse_map("1\n0 118 0 RetiredCommand 0", &k.catalogue).is_err());
    }
}
