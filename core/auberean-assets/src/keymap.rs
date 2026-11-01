//! A key map's text form, as the client reads it from the files beside the program
//! (`Main.keymap`, `Shift.keymap`, `Control.keymap`) and writes it when the player applies keys,
//! and the names of the inputs a key map binds.
//!
//! The file is a text tree ([`auberean_dat::tree`]): an optional `PREPROC_HEADER`, an optional
//! `VERSION [ n ]`, the map's name, its 11-part identity, then a binding a node,
//! `[ ACTION INPUT ]`.
//! The client writes it as `VERSION [ 1 ]`, `"" [ name ]`, `"" [ identity ]` and `"" [ ACTION
//! INPUT ]` lines; the hand-written sources use `{ }` and comments, which read the same.
//!
//! An action is named by the game's input actions enumeration (the data's own names, letter case
//! ignored); an input by the names below (exact).

use auberean_dat::tree::{number, Node};

use crate::ui::{KeyBinding, KeyMap};

/// The input names, in the order the client registers them: the keyboard's keys (`0x81000400` and
/// the key's scan code), the mouse's axes and buttons (`0x82000000`+), and the device-independent
/// buttons, axes and hats (`0xFF000000`+, `0x0A000000`+).
// D2-04897
pub const INPUT_NAMES: [(u32, &str); 192] = [
    (0x8100_0401, "DIKEYBOARD_ESCAPE"),
    (0x8100_0402, "DIKEYBOARD_1"),
    (0x8100_0403, "DIKEYBOARD_2"),
    (0x8100_0404, "DIKEYBOARD_3"),
    (0x8100_0405, "DIKEYBOARD_4"),
    (0x8100_0406, "DIKEYBOARD_5"),
    (0x8100_0407, "DIKEYBOARD_6"),
    (0x8100_0408, "DIKEYBOARD_7"),
    (0x8100_0409, "DIKEYBOARD_8"),
    (0x8100_040A, "DIKEYBOARD_9"),
    (0x8100_040B, "DIKEYBOARD_0"),
    (0x8100_040C, "DIKEYBOARD_MINUS"),
    (0x8100_040D, "DIKEYBOARD_EQUALS"),
    (0x8100_040E, "DIKEYBOARD_BACK"),
    (0x8100_040F, "DIKEYBOARD_TAB"),
    (0x8100_0410, "DIKEYBOARD_Q"),
    (0x8100_0411, "DIKEYBOARD_W"),
    (0x8100_0412, "DIKEYBOARD_E"),
    (0x8100_0413, "DIKEYBOARD_R"),
    (0x8100_0414, "DIKEYBOARD_T"),
    (0x8100_0415, "DIKEYBOARD_Y"),
    (0x8100_0416, "DIKEYBOARD_U"),
    (0x8100_0417, "DIKEYBOARD_I"),
    (0x8100_0418, "DIKEYBOARD_O"),
    (0x8100_0419, "DIKEYBOARD_P"),
    (0x8100_041A, "DIKEYBOARD_LBRACKET"),
    (0x8100_041B, "DIKEYBOARD_RBRACKET"),
    (0x8100_041C, "DIKEYBOARD_RETURN"),
    (0x8100_041D, "DIKEYBOARD_LCONTROL"),
    (0x8100_041E, "DIKEYBOARD_A"),
    (0x8100_041F, "DIKEYBOARD_S"),
    (0x8100_0420, "DIKEYBOARD_D"),
    (0x8100_0421, "DIKEYBOARD_F"),
    (0x8100_0422, "DIKEYBOARD_G"),
    (0x8100_0423, "DIKEYBOARD_H"),
    (0x8100_0424, "DIKEYBOARD_J"),
    (0x8100_0425, "DIKEYBOARD_K"),
    (0x8100_0426, "DIKEYBOARD_L"),
    (0x8100_0427, "DIKEYBOARD_SEMICOLON"),
    (0x8100_0428, "DIKEYBOARD_APOSTROPHE"),
    (0x8100_0429, "DIKEYBOARD_GRAVE"),
    (0x8100_042A, "DIKEYBOARD_LSHIFT"),
    (0x8100_042B, "DIKEYBOARD_BACKSLASH"),
    (0x8100_042C, "DIKEYBOARD_Z"),
    (0x8100_042D, "DIKEYBOARD_X"),
    (0x8100_042E, "DIKEYBOARD_C"),
    (0x8100_042F, "DIKEYBOARD_V"),
    (0x8100_0430, "DIKEYBOARD_B"),
    (0x8100_0431, "DIKEYBOARD_N"),
    (0x8100_0432, "DIKEYBOARD_M"),
    (0x8100_0433, "DIKEYBOARD_COMMA"),
    (0x8100_0434, "DIKEYBOARD_PERIOD"),
    (0x8100_0435, "DIKEYBOARD_SLASH"),
    (0x8100_0436, "DIKEYBOARD_RSHIFT"),
    (0x8100_0437, "DIKEYBOARD_MULTIPLY"),
    (0x8100_0438, "DIKEYBOARD_LMENU"),
    (0x8100_0439, "DIKEYBOARD_SPACE"),
    (0x8100_043A, "DIKEYBOARD_CAPITAL"),
    (0x8100_043B, "DIKEYBOARD_F1"),
    (0x8100_043C, "DIKEYBOARD_F2"),
    (0x8100_043D, "DIKEYBOARD_F3"),
    (0x8100_043E, "DIKEYBOARD_F4"),
    (0x8100_043F, "DIKEYBOARD_F5"),
    (0x8100_0440, "DIKEYBOARD_F6"),
    (0x8100_0441, "DIKEYBOARD_F7"),
    (0x8100_0442, "DIKEYBOARD_F8"),
    (0x8100_0443, "DIKEYBOARD_F9"),
    (0x8100_0444, "DIKEYBOARD_F10"),
    (0x8100_0445, "DIKEYBOARD_NUMLOCK"),
    (0x8100_0446, "DIKEYBOARD_SCROLL"),
    (0x8100_0447, "DIKEYBOARD_NUMPAD7"),
    (0x8100_0448, "DIKEYBOARD_NUMPAD8"),
    (0x8100_0449, "DIKEYBOARD_NUMPAD9"),
    (0x8100_044A, "DIKEYBOARD_SUBTRACT"),
    (0x8100_044B, "DIKEYBOARD_NUMPAD4"),
    (0x8100_044C, "DIKEYBOARD_NUMPAD5"),
    (0x8100_044D, "DIKEYBOARD_NUMPAD6"),
    (0x8100_044E, "DIKEYBOARD_ADD"),
    (0x8100_044F, "DIKEYBOARD_NUMPAD1"),
    (0x8100_0450, "DIKEYBOARD_NUMPAD2"),
    (0x8100_0451, "DIKEYBOARD_NUMPAD3"),
    (0x8100_0452, "DIKEYBOARD_NUMPAD0"),
    (0x8100_0453, "DIKEYBOARD_DECIMAL"),
    (0x8100_0456, "DIKEYBOARD_OEM_102"),
    (0x8100_0457, "DIKEYBOARD_F11"),
    (0x8100_0458, "DIKEYBOARD_F12"),
    (0x8100_0464, "DIKEYBOARD_F13"),
    (0x8100_0465, "DIKEYBOARD_F14"),
    (0x8100_0466, "DIKEYBOARD_F15"),
    (0x8100_0470, "DIKEYBOARD_KANA"),
    (0x8100_0473, "DIKEYBOARD_ABNT_C1"),
    (0x8100_0479, "DIKEYBOARD_CONVERT"),
    (0x8100_047B, "DIKEYBOARD_NOCONVERT"),
    (0x8100_047D, "DIKEYBOARD_YEN"),
    (0x8100_047E, "DIKEYBOARD_ABNT_C2"),
    (0x8100_048D, "DIKEYBOARD_NUMPADEQUALS"),
    (0x8100_0490, "DIKEYBOARD_PREVTRACK"),
    (0x8100_0491, "DIKEYBOARD_AT"),
    (0x8100_0492, "DIKEYBOARD_COLON"),
    (0x8100_0493, "DIKEYBOARD_UNDERLINE"),
    (0x8100_0494, "DIKEYBOARD_KANJI"),
    (0x8100_0495, "DIKEYBOARD_STOP"),
    (0x8100_0496, "DIKEYBOARD_AX"),
    (0x8100_0497, "DIKEYBOARD_UNLABELED"),
    (0x8100_0499, "DIKEYBOARD_NEXTTRACK"),
    (0x8100_049C, "DIKEYBOARD_NUMPADENTER"),
    (0x8100_049D, "DIKEYBOARD_RCONTROL"),
    (0x8100_04A0, "DIKEYBOARD_MUTE"),
    (0x8100_04A1, "DIKEYBOARD_CALCULATOR"),
    (0x8100_04A2, "DIKEYBOARD_PLAYPAUSE"),
    (0x8100_04A4, "DIKEYBOARD_MEDIASTOP"),
    (0x8100_04AE, "DIKEYBOARD_VOLUMEDOWN"),
    (0x8100_04B0, "DIKEYBOARD_VOLUMEUP"),
    (0x8100_04B2, "DIKEYBOARD_WEBHOME"),
    (0x8100_04B3, "DIKEYBOARD_NUMPADCOMMA"),
    (0x8100_04B5, "DIKEYBOARD_DIVIDE"),
    (0x8100_04B7, "DIKEYBOARD_SYSRQ"),
    (0x8100_04B8, "DIKEYBOARD_RMENU"),
    (0x8100_04C5, "DIKEYBOARD_PAUSE"),
    (0x8100_04C7, "DIKEYBOARD_HOME"),
    (0x8100_04C8, "DIKEYBOARD_UP"),
    (0x8100_04C9, "DIKEYBOARD_PRIOR"),
    (0x8100_04CB, "DIKEYBOARD_LEFT"),
    (0x8100_04CD, "DIKEYBOARD_RIGHT"),
    (0x8100_04CF, "DIKEYBOARD_END"),
    (0x8100_04D0, "DIKEYBOARD_DOWN"),
    (0x8100_04D1, "DIKEYBOARD_NEXT"),
    (0x8100_04D2, "DIKEYBOARD_INSERT"),
    (0x8100_04D3, "DIKEYBOARD_DELETE"),
    (0x8100_04DB, "DIKEYBOARD_LWIN"),
    (0x8100_04DC, "DIKEYBOARD_RWIN"),
    (0x8100_04DD, "DIKEYBOARD_APPS"),
    (0x8100_04DE, "DIKEYBOARD_POWER"),
    (0x8100_04DF, "DIKEYBOARD_SLEEP"),
    (0x8100_04E3, "DIKEYBOARD_WAKE"),
    (0x8100_04E5, "DIKEYBOARD_WEBSEARCH"),
    (0x8100_04E6, "DIKEYBOARD_WEBFAVORITES"),
    (0x8100_04E7, "DIKEYBOARD_WEBREFRESH"),
    (0x8100_04E8, "DIKEYBOARD_WEBSTOP"),
    (0x8100_04E9, "DIKEYBOARD_WEBFORWARD"),
    (0x8100_04EA, "DIKEYBOARD_WEBBACK"),
    (0x8100_04EB, "DIKEYBOARD_MYCOMPUTER"),
    (0x8100_04EC, "DIKEYBOARD_MAIL"),
    (0x8100_04ED, "DIKEYBOARD_MEDIASELECT"),
    (0x8200_0300, "DIMOUSE_XAXIS"),
    (0x8200_0304, "DIMOUSE_YAXIS"),
    (0x8200_0308, "DIMOUSE_WHEEL"),
    (0x8200_040C, "DIMOUSE_BUTTON0"),
    (0x8200_040D, "DIMOUSE_BUTTON1"),
    (0x8200_040E, "DIMOUSE_BUTTON2"),
    (0x8200_040F, "DIMOUSE_BUTTON3"),
    (0x8200_0410, "DIMOUSE_BUTTON4"),
    (0x8200_0411, "DIMOUSE_BUTTON5"),
    (0x8200_0412, "DIMOUSE_BUTTON6"),
    (0x8200_0413, "DIMOUSE_BUTTON7"),
    (0xFF00_4601, "DIPOV_ANY_1"),
    (0xFF00_4400, "DIBUTTON_ANY_0"),
    (0xFF00_4401, "DIBUTTON_ANY_1"),
    (0xFF00_4402, "DIBUTTON_ANY_2"),
    (0xFF00_4403, "DIBUTTON_ANY_3"),
    (0xFF00_4404, "DIBUTTON_ANY_4"),
    (0xFF00_4405, "DIBUTTON_ANY_5"),
    (0xFF00_4406, "DIBUTTON_ANY_6"),
    (0xFF00_4407, "DIBUTTON_ANY_7"),
    (0xFF00_C201, "DIAXIS_ANY_X_1"),
    (0xFF01_4201, "DIAXIS_ANY_Y_1"),
    (0xFF01_C201, "DIAXIS_ANY_Z_1"),
    (0xFF02_4201, "DIAXIS_ANY_R_1"),
    (0xFF02_C201, "DIAXIS_ANY_U_1"),
    (0xFF03_4201, "DIAXIS_ANY_V_1"),
    (0xFF03_C201, "DIAXIS_ANY_A_1"),
    (0xFF04_4201, "DIAXIS_ANY_B_1"),
    (0xFF04_C201, "DIAXIS_ANY_C_1"),
    (0x0A02_0201, "DIAXIS_TPS_TURN"),
    (0x0A01_0202, "DIAXIS_TPS_MOVE"),
    (0x0A00_0401, "DIBUTTON_TPS_RUN"),
    (0x0A00_0402, "DIBUTTON_TPS_ACTION"),
    (0x0A00_0403, "DIBUTTON_TPS_SELECT"),
    (0x0A00_0404, "DIBUTTON_TPS_USE"),
    (0x0A00_0405, "DIBUTTON_TPS_JUMP"),
    (0x0A00_04FD, "DIBUTTON_TPS_MENU"),
    (0x0A00_4601, "DIHATSWITCH_TPS_GLANCE"),
    (0x0A00_4406, "DIBUTTON_TPS_VIEW"),
    (0x0A00_4407, "DIBUTTON_TPS_STEPLEFT"),
    (0x0A00_4408, "DIBUTTON_TPS_STEPRIGHT"),
    (0x0A00_C203, "DIAXIS_TPS_STEP"),
    (0x0A00_4409, "DIBUTTON_TPS_DODGE"),
    (0x0A00_440A, "DIBUTTON_TPS_INVENTORY"),
    (0x0A00_44FE, "DIBUTTON_TPS_DEVICE"),
    (0x0A00_44FC, "DIBUTTON_TPS_PAUSE"),
    (0xFF00_4704, "DIMOUSE_SCROLL_UP"),
    (0xFF00_4705, "DIMOUSE_SCROLL_DOWN"),
];

/// The input a name names (the exact name).
// D2-04890
#[must_use]
pub fn input_by_name(name: &[u8]) -> Option<u32> {
    INPUT_NAMES
        .iter()
        .find(|(_, n)| n.as_bytes() == name)
        .map(|(v, _)| *v)
}

/// The name of an input, if it has one.
// D2-04883
#[must_use]
pub fn input_name(input: u32) -> Option<&'static str> {
    INPUT_NAMES
        .iter()
        .find(|(v, _)| *v == input)
        .map(|(_, n)| *n)
}

/// The node a preprocessed file may start with.
pub const PREPROC_HEADER: &[u8] = b"PREPROC_HEADER";
/// The node naming the file's version.
pub const VERSION_HEADER: &[u8] = b"VERSION";
/// The version the client writes.
pub const VERSION: u32 = 1;

/// A binding a key map gains when it is read from a file older than `version`, if `input` is
/// free in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VersionUpdate {
    pub version: i32,
    pub input: u32,
    pub action: &'static str,
}

/// The updates the plain key map (`Main.keymap`) is read with: version 1 made R reply to the last
/// tell.
pub const NORMAL_UPDATES: [VersionUpdate; 1] = [VersionUpdate {
    version: 1,
    input: 0x8100_0413,
    action: "REPLY_KEY",
}];

/// A key map read from its file: the map, the file's version (0 when it has none), and whether
/// the version's updates added bindings (the client then writes the file again).
#[derive(Clone, Debug, PartialEq)]
pub struct ReadKeyMap {
    pub map: KeyMap,
    pub version: u32,
    pub updated: bool,
}

/// Text as a narrow string's characters (each byte one character, as the data's narrow strings).
fn latin1(t: &[u8]) -> String {
    t.iter().map(|&b| char::from(b)).collect()
}

/// A narrow string's bytes.
fn bytes_of(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?'))
        .collect()
}

/// An unsigned number read whole from a node's child, as the key map's reader checks one.
fn number_at(node: &mut Node, i: usize) -> Option<u32> {
    let t = node.child_text(i)?;
    number::is_uint32(&t).then(|| number::strtoul(&t).value)
}

impl KeyMap {
    /// A key map read from its file's tree: the optional preprocessor header and `VERSION`, the
    /// name, the 11-part identity (each part a whole unsigned number, the last ten cut to 16, 16
    /// and 8 bits), then every node of exactly two children as a binding whose action (by
    /// `action_of`, its name in the input actions enumeration) and input (by its name) are known
    /// and whose input is not bound already (the first binding of an input wins). Then the
    /// `updates` for a file older than theirs. None when the file has fewer than two nodes, an
    /// identity not of 11 numbers, or a version that is not a number.
    ///
    /// The version's value is read from the **first** node's first child, whichever node is the
    /// version (so after a preprocessor header, from the header).
    // D2-04907, D2-04873, D2-04887
    pub fn from_tree(
        root: &mut Node,
        action_of: &dyn Fn(&str) -> Option<u32>,
        updates: &[VersionUpdate],
    ) -> Option<ReadKeyMap> {
        if root.len() < 2 {
            // "must have at least 2 parts (name, GUID, mapping(s))"
            root.report_error();
            return None;
        }
        let mut at = usize::from(root.child(0)?.is_keyword(PREPROC_HEADER));
        let mut version = 0;
        if root.child(at)?.is_keyword(VERSION_HEADER) {
            let Some(v) = number_at(root.child_mut(0)?, 0) else {
                // "invalid version info (it should be a number)"
                root.report_error();
                return None;
            };
            version = v;
            at += 1;
        }
        let name = root.child_mut(at)?.child_text(0)?;
        at += 1;
        let identity = root.child_mut(at)?;
        if identity.len() != 11 {
            // "GUID must have 11 parts"
            root.report_error();
            return None;
        }
        let mut parts = [0u32; 11];
        for (i, p) in parts.iter_mut().enumerate() {
            let Some(v) = number_at(identity, i) else {
                // "invalid GUID part (it should be a number)"
                root.report_error();
                return None;
            };
            *p = v;
        }
        let mut guid = [0u8; 16];
        guid[..4].copy_from_slice(&parts[0].to_le_bytes());
        guid[4..6].copy_from_slice(&parts[1].to_le_bytes()[..2]);
        guid[6..8].copy_from_slice(&parts[2].to_le_bytes()[..2]);
        for (k, p) in parts[3..].iter().enumerate() {
            guid[8 + k] = p.to_le_bytes()[0];
        }
        let mut map = Self {
            name: latin1(&name),
            guid,
            bindings: Vec::new(),
        };
        // The bindings are looked for from the identity on (it has eleven children, never two).
        for i in at..root.len() {
            let Some(b) = root.child_mut(i) else { break };
            if b.len() != 2 {
                continue;
            }
            let Some(action_name) = b.child_text(0).map(|t| latin1(&t)) else {
                continue;
            };
            let Some(action) = action_of(&action_name).filter(|a| *a != 0) else {
                continue;
            };
            let Some(input) = b.child_text(1).and_then(|t| input_by_name(&t)) else {
                continue;
            };
            if map.bindings.iter().any(|x| x.input == input) {
                continue;
            }
            map.add_action(action, input, &action_name);
        }
        let updated = map.update_version(version, updates, action_of);
        Some(ReadKeyMap {
            map,
            version,
            updated,
        })
    }

    /// Add a binding at the end.
    // D2-04888
    pub fn add_action(&mut self, action: u32, input: u32, name: &str) {
        self.bindings.push(KeyBinding {
            action,
            input,
            name: name.to_owned(),
        });
    }

    /// The bindings of the `updates` newer than `version` whose input is free, added at the end.
    /// Whether any was.
    // D2-04891
    pub fn update_version(
        &mut self,
        version: u32,
        updates: &[VersionUpdate],
        action_of: &dyn Fn(&str) -> Option<u32>,
    ) -> bool {
        let mut changed = false;
        for u in updates {
            let free = !self.bindings.iter().any(|b| b.input == u.input);
            if version.cast_signed() < u.version && free {
                let action = action_of(u.action).unwrap_or(0);
                self.add_action(action, u.input, u.action);
                changed = true;
            }
        }
        changed
    }

    /// The key map as the client writes its file: `VERSION [ 1 ]`, the name, the identity's 11
    /// parts as numbers, then each binding's action name and input name (an input with no name
    /// written empty).
    // D2-04908
    #[must_use]
    pub fn to_tree(&self) -> Node {
        let mut root = Node::new();
        let v = root.make_child_text(VERSION_HEADER);
        v.make_child_u32(VERSION);
        root.make_child().make_child_text(bytes_of(&self.name));
        let id = root.make_child();
        let g = &self.guid;
        id.make_child_u32(u32::from_le_bytes([g[0], g[1], g[2], g[3]]));
        id.make_child_u32(u32::from(u16::from_le_bytes([g[4], g[5]])));
        id.make_child_u32(u32::from(u16::from_le_bytes([g[6], g[7]])));
        for b in &g[8..] {
            id.make_child_u32(u32::from(*b));
        }
        for b in &self.bindings {
            let n = root.make_child();
            n.make_child_text(bytes_of(&b.name));
            n.make_child_text(input_name(b.input).unwrap_or_default());
        }
        root
    }

    /// Copy the name, the identity and the bindings into another map (its own bindings kept,
    /// these after them).
    // D2-04909
    pub fn copy_into(&self, other: &mut Self) {
        other.name.clone_from(&self.name);
        other.guid = self.guid;
        for b in &self.bindings {
            other.add_action(b.action, b.input, &b.name);
        }
    }

    /// The bindings dropped.
    // D2-04896
    pub fn destroy(&mut self) {
        self.bindings.clear();
    }
}
