//! The classic Character options page: its rows, its default option words, and what Apply,
//! Defaults and each checkbox change. The model emits actions; it does not emulate the game's
//! side effects.
use crate::{Command, Screen};
use serde::Deserialize;
use std::sync::OnceLock;

/// The page's Defaults: the character's default option words, which are the final client's, with
/// the classic interface's own right-click mouse look on (its bit is the interface's setting,
/// never the character's; see [`crate::keyboard_runtime::CLASSIC_ONLY`]).
pub const DEFAULT_WORDS: [u32; 2] = [
    dereth_client_model::player::options::DEFAULT_OPTIONS
        | crate::keyboard_runtime::RIGHT_CLICK_LOOK,
    dereth_client_model::player::options::DEFAULT_OPTIONS2,
];

/// Ignoring fellowship requests (first word bit 3) and accepting them automatically (bit 29)
/// cannot both be on: turning one on turns the other off, as the game's own option change does.
const IGNORE_FELLOWSHIP: u32 = 0x8;
const AUTO_ACCEPT_FELLOWSHIP: u32 = 0x2000_0000;
pub const MAX_SCROLL: i32 = 636;
pub const VIEWPORT: [i32; 4] = [4, 41, 284, 305];

#[derive(Clone, Debug, Deserialize)]
pub struct OptionRow {
    pub caption: String,
    pub y: i32,
    pub kind: String,
    #[serde(default)]
    pub word: usize,
    #[serde(default)]
    pub bit: u32,
    #[serde(default)]
    pub invert: bool,
}
#[derive(Deserialize)]
struct Blueprint {
    rows: Vec<OptionRow>,
}

pub fn rows() -> &'static [OptionRow] {
    static BLUEPRINT: OnceLock<Blueprint> = OnceLock::new();
    &BLUEPRINT
        .get_or_init(|| {
            serde_json::from_str(include_str!("../screens/character-blueprint.json"))
                .expect("checked blueprint")
        })
        .rows
}

/// The host can translate this result into the character-options action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OptionsApplied {
    pub words: [u32; 2],
    pub timestamp_format: String,
}

#[derive(Clone, Debug)]
pub struct OptionsModel {
    current: [u32; 2],
    applied: [u32; 2],
    timestamp: String,
    applied_timestamp: String,
    buttons: [bool; 3],
}
impl Default for OptionsModel {
    fn default() -> Self {
        Self::new()
    }
}
impl OptionsModel {
    pub fn new() -> Self {
        Self::from_words(DEFAULT_WORDS)
    }
    pub fn from_words(words: [u32; 2]) -> Self {
        // The refresh path uses OR here; the Apply path below uses AND.
        Self {
            current: words,
            applied: words,
            timestamp: String::new(),
            applied_timestamp: String::new(),
            buttons: [false, false, words[0] != DEFAULT_WORDS[0] && words[1] != 0],
        }
    }
    pub fn dirty(&self) -> bool {
        self.buttons[0]
    }
    pub fn set_checked(&mut self, row: usize, checked: bool) -> bool {
        match self.checked(row) {
            Some(value) if value != checked => self.toggle(row),
            Some(_) => true,
            None => false,
        }
    }
    pub fn current_words(&self) -> [u32; 2] {
        self.current
    }
    /// Local player words, including the immediate change made by Defaults.
    pub fn applied_words(&self) -> [u32; 2] {
        self.applied
    }
    pub fn button_enabled(&self, index: usize) -> bool {
        self.buttons.get(index).copied().unwrap_or(false)
    }
    pub fn timestamp_format(&self) -> &str {
        &self.timestamp
    }
    pub fn load_timestamp_format(&mut self, text: String) {
        self.timestamp = text.clone();
        self.applied_timestamp = text;
    }
    pub fn set_timestamp_format(&mut self, text: String) {
        self.timestamp = text;
        self.buttons = [true; 3];
    }
    /// Index is the 45-row blueprint index, including its six headings.
    pub fn checked(&self, row: usize) -> Option<bool> {
        let r = rows().get(row)?;
        (r.kind != "heading").then(|| (self.current[r.word] & (1 << r.bit) != 0) ^ r.invert)
    }
    pub fn toggle(&mut self, row: usize) -> bool {
        let Some(r) = rows().get(row).filter(|r| r.kind != "heading") else {
            return false;
        };
        self.current[r.word] ^= 1 << r.bit;
        if r.word == 0 {
            let mask = 1 << r.bit;
            if mask == IGNORE_FELLOWSHIP && self.current[0] & mask != 0 {
                self.current[0] &= !AUTO_ACCEPT_FELLOWSHIP;
            } else if mask == AUTO_ACCEPT_FELLOWSHIP && self.current[0] & mask != 0 {
                self.current[0] &= !IGNORE_FELLOWSHIP;
            }
        }
        // Any change to a control enables all three buttons, even if it is toggled back.
        self.buttons = [true; 3];
        true
    }
    pub fn apply(&mut self) -> OptionsApplied {
        self.applied = self.current;
        self.applied_timestamp.clone_from(&self.timestamp);
        self.buttons = [false, false, self.current != DEFAULT_WORDS];
        OptionsApplied {
            words: self.current,
            timestamp_format: self.timestamp.clone(),
        }
    }
    pub fn reset(&mut self) {
        self.current = self.applied;
        self.timestamp.clone_from(&self.applied_timestamp);
        self.buttons = [
            false,
            false,
            self.applied[0] != DEFAULT_WORDS[0] && self.applied[1] != 0,
        ];
    }
    pub fn defaults(&mut self) {
        // Defaults writes the player module immediately, then waits for Apply to send it.
        self.applied = DEFAULT_WORDS;
        self.current = DEFAULT_WORDS;
        self.timestamp.clone_from(&self.applied_timestamp);
        self.buttons = [true, false, false];
    }
    pub fn row_at(&self, x: i32, y: i32, scroll: i32) -> Option<usize> {
        if !(14..27).contains(&x) || !(41..305).contains(&y) {
            return None;
        }
        let local_y = y - 41 + scroll.clamp(0, MAX_SCROLL);
        rows()
            .iter()
            .position(|r| r.kind != "heading" && (r.y..r.y + 13).contains(&local_y))
    }
    pub fn render(&self, scroll: i32) -> Screen {
        let scroll = scroll.clamp(0, MAX_SCROLL);
        let mut screen: Screen =
            serde_json::from_str(include_str!("../screens/character-top.json"))
                .expect("checked screen");
        screen.commands.retain(|c| match c {
            Command::Image { clip, .. } | Command::Text { clip, .. } => *clip != Some(VIEWPORT),
            _ => true,
        });
        for c in &mut screen.commands {
            match c {
                Command::Image { did, y, .. } if did == "06001263" => {
                    *y = 57 + 217 * scroll / MAX_SCROLL
                }
                Command::Image { did, x, y, .. } if *y == 322 => {
                    let index = if *x < 110 {
                        0
                    } else if *x < 195 {
                        1
                    } else {
                        2
                    };
                    if self.buttons[index] {
                        *did = if did == "0600120A" {
                            "06001207"
                        } else {
                            "06001206"
                        }
                        .into();
                    }
                }
                Command::Text { text, color, .. } => {
                    if let Some(index) = ["Apply", "Reset", "Defaults"]
                        .iter()
                        .position(|s| *s == text)
                    {
                        *color = if self.buttons[index] {
                            0xffd2d2c8
                        } else {
                            0xff646464
                        };
                    }
                }
                _ => {}
            }
        }
        screen
            .commands
            .push(image("0600128A", 4, 41 - scroll, 300, 900, true, false));
        for (index, r) in rows().iter().enumerate() {
            let y = 41 + r.y - scroll;
            if y + 20 <= 41 || y >= 305 {
                continue;
            }
            let heading = r.kind == "heading";
            if !heading {
                screen.commands.push(image(
                    if self.checked(index) == Some(true) {
                        "0600128B"
                    } else {
                        "0600128D"
                    },
                    14,
                    y,
                    13,
                    13,
                    false,
                    true,
                ));
            }
            let mut clip = VIEWPORT;
            if r.caption == "Display Timestamps" {
                clip[2] = 159;
            }
            screen.commands.push(Command::Text {
                text: r.caption.clone(),
                x: if heading { 16 } else { 36 },
                y: y + 2,
                font: if heading { "courier-14-7" } else { "15-6" }.into(),
                color: if heading { 0xff00c8e1 } else { 0xffd2d2c8 },
                clip: Some(clip),
            });
            if r.caption == "Display Timestamps" && !self.timestamp.is_empty() {
                screen.commands.push(Command::Text {
                    text: self.timestamp.clone(),
                    x: 161,
                    y: y + 2,
                    font: "15-6".into(),
                    color: 0xff00ff00,
                    clip: Some([159, 41, 284, 305]),
                });
            }
        }
        screen
    }
}
fn image(did: &str, x: i32, y: i32, width: u32, height: u32, tile: bool, keyed: bool) -> Command {
    Command::Image {
        did: did.into(),
        x,
        y,
        width,
        height,
        clip: Some(VIEWPORT),
        tile,
        color_key: keyed.then_some([0, 0, 0]),
        key_bits: keyed.then_some([5, 6, 5]),
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;
    #[test]
    fn accepting_requests_inverts_ignore_bits_and_apply_preserves_unshown_bits() {
        let mut model = OptionsModel::from_words([0, 0xffff_ff00]);
        let index = rows()
            .iter()
            .position(|r| r.caption == "Accept Allegiance Requests")
            .unwrap();
        assert_eq!(model.checked(index), Some(true));
        model.toggle(index);
        assert_eq!(model.checked(index), Some(false));
        assert_eq!(model.apply().words, [4, 0xffff_ff00]);
    }
    #[test]
    fn reset_discards_edits_but_defaults_changes_local_player_before_apply() {
        let mut model = OptionsModel::from_words([0, 1]);
        model.toggle(1);
        model.reset();
        assert_eq!(model.current_words(), [0, 1]);
        model.defaults();
        assert_eq!(model.applied_words(), DEFAULT_WORDS);
        assert!(model.button_enabled(0));
        assert!(!model.button_enabled(1));
        model.reset();
        assert_eq!(model.current_words(), DEFAULT_WORDS);
    }
    #[test]
    fn accepting_fellowship_requests_automatically_and_refusing_them_turn_each_other_off() {
        let accept = rows()
            .iter()
            .position(|r| r.caption == "Accept Fellowship Requests")
            .unwrap();
        let auto = rows()
            .iter()
            .position(|r| r.caption == "Automatically Accept Fellowship Requests")
            .unwrap();
        let mut model = OptionsModel::from_words([0, 0]);
        assert_eq!(model.checked(accept), Some(true));
        model.set_checked(auto, true);
        model.set_checked(accept, false);
        assert_eq!(
            model.checked(auto),
            Some(false),
            "refusing turns automatic off"
        );
        model.set_checked(auto, true);
        assert_eq!(
            model.checked(accept),
            Some(true),
            "automatic turns refusing off"
        );
    }
    #[test]
    fn the_defaults_are_the_characters_default_words_with_right_click_mouse_look() {
        assert_eq!(DEFAULT_WORDS, [0x50c4_a54a | 0x4000, 0x0294_8700]);
    }
    #[test]
    fn all_39_options_have_unique_bindings_and_headings_cannot_toggle() {
        let mut pairs = std::collections::HashSet::new();
        let mut model = OptionsModel::default();
        for (i, row) in rows().iter().enumerate() {
            if row.kind == "heading" {
                assert!(!model.toggle(i));
            } else {
                assert!(pairs.insert((row.word, row.bit)));
            }
        }
        assert_eq!(pairs.len(), 39);
    }
    #[test]
    fn clipped_checkbox_hit_testing_tracks_scrolled_content() {
        let model = OptionsModel::default();
        assert_eq!(model.row_at(15, 67, 0), Some(1));
        assert_eq!(model.row_at(15, 291, 636), Some(44));
        assert_eq!(model.row_at(15, 306, 636), None);
        assert_eq!(model.row_at(36, 67, 0), None);
    }
}
