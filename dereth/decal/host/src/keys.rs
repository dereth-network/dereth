//! The game's own keys, pressed for a plugin: its attacks, its jump and its held movement.
//!
//! Decal has no call that attacks, jumps or holds a movement key, so with the player's leave (the
//! `keys` setting, or the plugin panel's switch) the host presses the key the game binds to each:
//! the key goes down, is held as long as the action needs, and comes up, as the player's own
//! would. Each op names the key by its virtual-key code and its place on the keyboard (the scan
//! code, and whether it is one of the extended keys), which is what the game reads.
//!
//! **Which key.** The player's own binding for each of the game's actions, as Decal reads it from
//! the game's key map ([`Keymap::set`]): a virtual-key code per action name. An action Decal names
//! no key for keeps the game's default.
//!
//! | action | the game's name | default |
//! |---|---|---|
//! | high, medium, low attack | `CombatHighAttack`, `CombatMediumAttack`, `CombatLowAttack` | Page Down, End, Delete |
//! | high, medium, low aim (a missile weapon) | `CombatAimHigh`, `CombatAimMedium`, `CombatAimLow` | Page Down, End, Delete |
//! | forward, backward | `MovementForward`, `MovementBackup` | W, X |
//! | strafe left, right | `MovementStrafeLeft`, `MovementStrafeRight` | Z, C |
//! | turn left, right | `MovementTurnLeft`, `MovementTurnRight` | A, D |
//! | walk | `MovementWalkMode` | Left Shift |
//! | jump | `MovementJump` | Space |
//!
//! **When.** Keys reach the game only while its window is in front and its chat line is closed:
//! otherwise a plugin's attack, jump or held movement fails at once as `unavailable`, with why,
//! and nothing is pressed.

use std::collections::BTreeMap;

use dereth_client_sdk::plugin_api::Value;
use dereth_client_sdk::runtime::frame_hook::NavKey;

/// One of the game's actions a plugin's request presses a key for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GameKey {
    HighAttack,
    MediumAttack,
    LowAttack,
    AimHigh,
    AimMedium,
    AimLow,
    Forward,
    Backward,
    StrafeLeft,
    StrafeRight,
    TurnLeft,
    TurnRight,
    Walk,
    Jump,
}

/// Virtual-key codes the defaults and the names below use.
mod vk {
    pub const SHIFT: u32 = 0x10;
    pub const CONTROL: u32 = 0x11;
    pub const MENU: u32 = 0x12;
    pub const SPACE: u32 = 0x20;
    pub const NEXT: u32 = 0x22;
    pub const END: u32 = 0x23;
    pub const DELETE: u32 = 0x2E;
    pub const LSHIFT: u32 = 0xA0;
    pub const RSHIFT: u32 = 0xA1;
    pub const LCONTROL: u32 = 0xA2;
    pub const RCONTROL: u32 = 0xA3;
    pub const LMENU: u32 = 0xA4;
    pub const RMENU: u32 = 0xA5;
}

impl GameKey {
    /// Every one, in the order the game lists them.
    pub const ALL: [Self; 14] = [
        Self::HighAttack,
        Self::MediumAttack,
        Self::LowAttack,
        Self::AimHigh,
        Self::AimMedium,
        Self::AimLow,
        Self::Forward,
        Self::Backward,
        Self::StrafeLeft,
        Self::StrafeRight,
        Self::TurnLeft,
        Self::TurnRight,
        Self::Walk,
        Self::Jump,
    ];

    /// The game's name for the action, as its key map names it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::HighAttack => "CombatHighAttack",
            Self::MediumAttack => "CombatMediumAttack",
            Self::LowAttack => "CombatLowAttack",
            Self::AimHigh => "CombatAimHigh",
            Self::AimMedium => "CombatAimMedium",
            Self::AimLow => "CombatAimLow",
            Self::Forward => "MovementForward",
            Self::Backward => "MovementBackup",
            Self::StrafeLeft => "MovementStrafeLeft",
            Self::StrafeRight => "MovementStrafeRight",
            Self::TurnLeft => "MovementTurnLeft",
            Self::TurnRight => "MovementTurnRight",
            Self::Walk => "MovementWalkMode",
            Self::Jump => "MovementJump",
        }
    }

    /// The game's own key for the action, as a virtual-key code.
    #[must_use]
    pub const fn default_key(self) -> u32 {
        match self {
            Self::HighAttack | Self::AimHigh => vk::NEXT,
            Self::MediumAttack | Self::AimMedium => vk::END,
            Self::LowAttack | Self::AimLow => vk::DELETE,
            // The letters' codes are their capitals: W, X, Z, C, A and D.
            Self::Forward => 0x57,
            Self::Backward => 0x58,
            Self::StrafeLeft => 0x5A,
            Self::StrafeRight => 0x43,
            Self::TurnLeft => 0x41,
            Self::TurnRight => 0x44,
            Self::Walk => vk::LSHIFT,
            Self::Jump => vk::SPACE,
        }
    }

    /// The attack at `height` (1 high, 2 medium, 3 low): with a missile weapon's aim when
    /// `missile`.
    #[must_use]
    pub const fn attack(height: u32, missile: bool) -> Self {
        match (height, missile) {
            (1, false) => Self::HighAttack,
            (3, false) => Self::LowAttack,
            (1, true) => Self::AimHigh,
            (3, true) => Self::AimLow,
            (_, false) => Self::MediumAttack,
            (_, true) => Self::AimMedium,
        }
    }

    /// Whether it is one of the attacks or aims.
    #[must_use]
    pub const fn is_attack(self) -> bool {
        matches!(
            self,
            Self::HighAttack
                | Self::MediumAttack
                | Self::LowAttack
                | Self::AimHigh
                | Self::AimMedium
                | Self::AimLow
        )
    }

    /// The movement control a plugin's movement holds.
    #[must_use]
    pub const fn of_nav(key: NavKey) -> Self {
        match key {
            NavKey::Forward => Self::Forward,
            NavKey::Backward => Self::Backward,
            NavKey::StrafeLeft => Self::StrafeLeft,
            NavKey::StrafeRight => Self::StrafeRight,
            NavKey::TurnLeft => Self::TurnLeft,
            NavKey::TurnRight => Self::TurnRight,
            NavKey::Walk => Self::Walk,
            NavKey::Jump => Self::Jump,
        }
    }
}

/// A key on the keyboard: its virtual-key code, and its place (the scan code, and whether it is
/// one of the extended keys: the arrows and the block above them, the right-hand Ctrl and Alt).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    pub vk: u32,
    pub scan: u32,
    pub extended: bool,
}

impl Key {
    /// The key whose virtual-key code is `code`, at its place on a US keyboard (where the game's
    /// key map names it); none for a code with no key of its own there. A left- or right-hand
    /// Shift, Ctrl or Alt goes as Windows sends its own, by the generic code.
    #[must_use]
    pub fn of(code: u32) -> Option<Self> {
        let (scan, extended) = place(code)?;
        let vk = match code {
            vk::LSHIFT | vk::RSHIFT => vk::SHIFT,
            vk::LCONTROL | vk::RCONTROL => vk::CONTROL,
            vk::LMENU | vk::RMENU => vk::MENU,
            other => other,
        };
        Some(Self { vk, scan, extended })
    }
}

/// Where the key with virtual-key code `code` is on a US keyboard: its scan code, and whether it
/// is extended.
fn place(code: u32) -> Option<(u32, bool)> {
    const LETTERS: [u32; 26] = [
        0x1E, 0x30, 0x2E, 0x20, 0x12, 0x21, 0x22, 0x23, 0x17, 0x24, 0x25, 0x26, 0x32, 0x31, 0x18,
        0x19, 0x10, 0x13, 0x1F, 0x14, 0x16, 0x2F, 0x11, 0x2D, 0x15, 0x2C,
    ];
    const NUMPAD: [u32; 10] = [0x52, 0x4F, 0x50, 0x51, 0x4B, 0x4C, 0x4D, 0x47, 0x48, 0x49];
    let plain = |s: u32| Some((s, false));
    let ext = |s: u32| Some((s, true));
    match code {
        0x41..=0x5A => plain(LETTERS[(code - 0x41) as usize]),
        0x30 => plain(0x0B),
        0x31..=0x39 => plain(code - 0x31 + 0x02),
        0x70..=0x79 => plain(code - 0x70 + 0x3B),
        0x7A => plain(0x57),
        0x7B => plain(0x58),
        0x7C..=0x7E => plain(code - 0x7C + 0x64),
        0x60..=0x69 => plain(NUMPAD[(code - 0x60) as usize]),
        0x08 => plain(0x0E),
        0x09 => plain(0x0F),
        0x0D => plain(0x1C),
        // The number pad's Enter.
        0x2B => ext(0x1C),
        0x13 => plain(0x45),
        0x14 => plain(0x3A),
        0x1B => plain(0x01),
        0x20 => plain(0x39),
        0x21 => ext(0x49),
        0x22 => ext(0x51),
        0x23 => ext(0x4F),
        0x24 => ext(0x47),
        0x25 => ext(0x4B),
        0x26 => ext(0x48),
        0x27 => ext(0x4D),
        0x28 => ext(0x50),
        0x2D => ext(0x52),
        0x2E => ext(0x53),
        0x5B => ext(0x5B),
        0x5C => ext(0x5C),
        0x5D => ext(0x5D),
        0x6A => plain(0x37),
        0x6B => plain(0x4E),
        0x6D => plain(0x4A),
        0x6E => plain(0x53),
        0x6F => ext(0x35),
        0x90 => ext(0x45),
        0x91 => plain(0x46),
        vk::SHIFT | vk::LSHIFT => plain(0x2A),
        vk::RSHIFT => plain(0x36),
        vk::CONTROL | vk::LCONTROL => plain(0x1D),
        vk::RCONTROL => ext(0x1D),
        vk::MENU | vk::LMENU => plain(0x38),
        vk::RMENU => ext(0x38),
        0xBA => plain(0x27),
        0xBB => plain(0x0D),
        0xBC => plain(0x33),
        0xBD => plain(0x0C),
        0xBE => plain(0x34),
        0xBF => plain(0x35),
        0xC0 => plain(0x29),
        0xDB => plain(0x1A),
        0xDC => plain(0x2B),
        0xDD => plain(0x1B),
        0xDE => plain(0x28),
        0xE2 => plain(0x56),
        _ => None,
    }
}

/// A key's name, as the player reads it in a line.
#[must_use]
pub fn name_of(code: u32) -> String {
    let named = match code {
        0x08 => "Backspace",
        0x09 => "Tab",
        0x0D => "Enter",
        0x13 => "Pause",
        0x14 => "Caps Lock",
        0x1B => "Esc",
        0x20 => "Space",
        0x21 => "Page Up",
        0x22 => "Page Down",
        0x23 => "End",
        0x24 => "Home",
        0x25 => "Left",
        0x26 => "Up",
        0x27 => "Right",
        0x28 => "Down",
        0x2D => "Insert",
        0x2E => "Delete",
        vk::SHIFT | vk::LSHIFT => "Left Shift",
        vk::RSHIFT => "Right Shift",
        vk::CONTROL | vk::LCONTROL => "Left Ctrl",
        vk::RCONTROL => "Right Ctrl",
        vk::MENU | vk::LMENU => "Left Alt",
        vk::RMENU => "Right Alt",
        _ => "",
    };
    if !named.is_empty() {
        return named.to_owned();
    }
    match code {
        0x30..=0x39 | 0x41..=0x5A => char::from_u32(code).map_or_else(String::new, String::from),
        0x60..=0x69 => format!("Num {}", code - 0x60),
        0x70..=0x87 => format!("F{}", code - 0x6F),
        other => format!("key 0x{other:02X}"),
    }
}

/// The key bound to each of the game's actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keymap {
    keys: BTreeMap<GameKey, Key>,
    /// Whether each came from the player's own key map.
    theirs: BTreeMap<GameKey, bool>,
}

impl Default for Keymap {
    fn default() -> Self {
        let keys = GameKey::ALL
            .into_iter()
            .filter_map(|g| Key::of(g.default_key()).map(|k| (g, k)))
            .collect();
        Self {
            keys,
            theirs: BTreeMap::new(),
        }
    }
}

impl Keymap {
    /// The key for `action`.
    #[must_use]
    pub fn key(&self, action: GameKey) -> Option<Key> {
        self.keys.get(&action).copied()
    }

    /// The player's bindings as Decal reads them from the game's key map: `{ <the game's name>:
    /// <virtual-key code> }`. An action with no code, a code of 0 or one of no key keeps the game's
    /// default.
    pub fn set(&mut self, bindings: &Value) {
        *self = Self::default();
        for g in GameKey::ALL {
            let code = bindings
                .get(g.name())
                .as_i64()
                .and_then(|c| u32::try_from(c).ok())
                .filter(|c| *c != 0);
            if let Some(k) = code.and_then(Key::of) {
                self.keys.insert(g, k);
                self.theirs.insert(g, true);
            }
        }
    }

    /// The keys a plugin's attacks and jump press, as the player reads them.
    #[must_use]
    pub fn told(&self) -> String {
        let name = |g: GameKey| {
            self.key(g)
                .map_or_else(|| "none".to_owned(), |k| name_of(k.vk))
        };
        format!(
            "attacks {}, {} and {}; jump {}",
            name(GameKey::HighAttack),
            name(GameKey::MediumAttack),
            name(GameKey::LowAttack),
            name(GameKey::Jump)
        )
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (a host inside another program: the keys it presses)
    use super::*;

    #[test]
    fn the_games_defaults_are_its_attack_jump_and_movement_keys_at_their_places() {
        let m = Keymap::default();
        assert_eq!(
            m.key(GameKey::HighAttack),
            Some(Key {
                vk: 0x22,
                scan: 0x51,
                extended: true
            }),
            "Page Down, on the block above the arrows"
        );
        assert_eq!(
            m.key(GameKey::Jump),
            Some(Key {
                vk: 0x20,
                scan: 0x39,
                extended: false
            })
        );
        assert_eq!(
            m.key(GameKey::Forward),
            Some(Key {
                vk: 0x57,
                scan: 0x11,
                extended: false
            })
        );
        assert_eq!(
            m.key(GameKey::Walk),
            Some(Key {
                vk: 0x10,
                scan: 0x2A,
                extended: false
            }),
            "Left Shift goes as Windows sends it"
        );
    }

    #[test]
    fn the_players_own_binding_replaces_the_default_and_a_missing_one_keeps_it() {
        let mut m = Keymap::default();
        m.set(
            &Value::map()
                .with("CombatHighAttack", 0x31_i64)
                .with("CombatLowAttack", 0_i64)
                .with("MovementJump", 0xE7_i64),
        );
        assert_eq!(
            m.key(GameKey::HighAttack),
            Some(Key {
                vk: 0x31,
                scan: 0x02,
                extended: false
            }),
            "the player's 1"
        );
        assert_eq!(
            m.key(GameKey::LowAttack).map(|k| k.vk),
            Some(0x2E),
            "no key named: Delete"
        );
        assert_eq!(
            m.key(GameKey::Jump).map(|k| k.vk),
            Some(0x20),
            "a code of no key: Space"
        );
        assert_eq!(m.told(), "attacks 1, End and Delete; jump Space");
    }

    #[test]
    fn a_number_pad_key_and_its_twin_above_the_arrows_are_told_apart() {
        assert_eq!(
            Key::of(0x63),
            Some(Key {
                vk: 0x63,
                scan: 0x51,
                extended: false
            }),
            "number pad 3"
        );
        assert_eq!(Key::of(0x22).map(|k| k.extended), Some(true), "Page Down");
        assert_eq!(
            Key::of(0x90).map(|k| (k.scan, k.extended)),
            Some((0x45, true)),
            "Num Lock"
        );
        assert_eq!(
            Key::of(0x13).map(|k| (k.scan, k.extended)),
            Some((0x45, false)),
            "Pause"
        );
    }

    #[test]
    fn an_attacks_key_follows_its_height_and_the_weapon() {
        assert_eq!(GameKey::attack(1, false), GameKey::HighAttack);
        assert_eq!(GameKey::attack(2, false), GameKey::MediumAttack);
        assert_eq!(GameKey::attack(3, true), GameKey::AimLow);
    }
}
