//! The classic Character options page: its rows, its default option words, and what Apply,
//! Defaults and each checkbox change. The rows are the shared options set's Character Options
//! page as the classic interface shows it ([`dereth_client_contract::options::sheet`]). The model
//! emits actions; it does not emulate the game's side effects.
use std::sync::OnceLock;

/// The page's Defaults: the character's default option words, which are the final client's.
/// The classic interface's own settings in the first word ([`crate::keyboard_runtime::CLASSIC_ONLY`])
/// are not the page's and keep their values.
pub const DEFAULT_WORDS: [u32; 2] = [
    dereth_client_model::player::options::DEFAULT_OPTIONS,
    dereth_client_model::player::options::DEFAULT_OPTIONS2,
];

/// Ignoring fellowship requests (first word bit 3) and accepting them automatically (bit 29)
/// cannot both be on: turning one on turns the other off, as the game's own option change does.
const IGNORE_FELLOWSHIP: u32 = 0x8;
const AUTO_ACCEPT_FELLOWSHIP: u32 = 0x2000_0000;
/// How far apart the page's rows are.
pub const ROW_HEIGHT: i32 = 20;

/// One row of the page: a heading, or a check box on one bit of the character's option words.
#[derive(Clone, Debug)]
pub struct OptionRow {
    pub caption: String,
    pub option: Option<dereth_client_contract::PlayerOption>,
    pub y: i32,
    pub kind: String,
    pub word: usize,
    pub bit: u32,
    pub invert: bool,
    /// What of the world's era the option needs; a row whose need is not met is not shown.
    pub needs: dereth_client_contract::options::sheet::Needs,
}

/// The word and bit `option` is kept in.
fn option_bit(option: dereth_client_contract::PlayerOption) -> (usize, u32) {
    use dereth_client_model::player::options::{OptionWord, PLAYER_OPTIONS};
    let (_, word, mask) = PLAYER_OPTIONS[dereth_client_runtime::hud::option_ordinal(option)];
    (usize::from(word == OptionWord::Two), mask.trailing_zeros())
}

/// The page's rows: the shared set's headings and the classic interface's options under them,
/// each at its place down the page.
pub fn rows() -> &'static [OptionRow] {
    use dereth_client_contract::options::interface::Interface;
    use dereth_client_contract::options::sheet::{headings_for, Needs, PageId, Value};
    static ROWS: OnceLock<Vec<OptionRow>> = OnceLock::new();
    ROWS.get_or_init(|| {
        let mut out = Vec::new();
        let mut y = 6;
        for (heading, rows) in headings_for(PageId::Character, Interface::Classic) {
            out.push(OptionRow {
                caption: heading.title.to_owned(),
                option: None,
                y,
                kind: "heading".into(),
                word: 0,
                bit: 0,
                invert: false,
                needs: Needs::Nothing,
            });
            y += ROW_HEIGHT;
            for r in rows {
                let (word, bit) = match r.value {
                    Value::Option(o) => option_bit(o),
                    Value::Bit { mask } => (0, mask.trailing_zeros()),
                    _ => continue,
                };
                out.push(OptionRow {
                    caption: r.caption_for(Interface::Classic).to_owned(),
                    option: match r.value {
                        Value::Option(o) => Some(o),
                        _ => None,
                    },
                    y,
                    kind: "checkbox".into(),
                    word,
                    bit,
                    invert: r.classic_inverted(),
                    needs: r.needs,
                });
                y += ROW_HEIGHT;
            }
        }
        out
    })
}

/// The bits of the two words the page shows; the rest it leaves as they are.
pub fn shown_bits() -> [u32; 2] {
    rows()
        .iter()
        .filter(|r| r.kind != "heading")
        .fold([0, 0], |mut m, r| {
            m[r.word] |= 1 << r.bit;
            m
        })
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
        // Defaults writes the player module immediately, then waits for Apply to send it. Only the
        // page's own bits move.
        let shown = shown_bits();
        let words = [0, 1].map(|i| (DEFAULT_WORDS[i] & shown[i]) | (self.current[i] & !shown[i]));
        self.applied = words;
        self.current = words;
        self.timestamp.clone_from(&self.applied_timestamp);
        self.buttons = [true, false, false];
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
    fn the_defaults_are_the_characters_default_words_and_leave_the_interfaces_own_bits() {
        assert_eq!(DEFAULT_WORDS, [0x50c4_a54a, 0x0294_8700]);
        // Right-click mouse look and the stretched layout, the interface's own, stay as they were.
        let own = crate::keyboard_runtime::RIGHT_CLICK_LOOK | crate::keyboard_runtime::STRETCH_UI;
        let mut model = OptionsModel::from_words([own, 0]);
        model.defaults();
        assert_eq!(model.applied_words()[0] & own, own);
        assert_eq!(
            model.applied_words()[0] & shown_bits()[0],
            DEFAULT_WORDS[0] & shown_bits()[0]
        );
    }
    #[test]
    fn the_rows_are_the_shared_character_page_under_its_headings() {
        let headings: Vec<&str> = rows()
            .iter()
            .filter(|r| r.kind == "heading")
            .map(|r| r.caption.as_str())
            .collect();
        assert_eq!(
            headings,
            [
                "Interface Behavior",
                "World Display",
                "Chat",
                "Fellowship and Allegiance",
                "Other Players",
                "Allow Others to See Your",
                "Combat and Movement"
            ]
        );
        let caption = |c: &str| rows().iter().any(|r| r.caption == c);
        // The classic interface's own wording, its own automatic shortcuts, and no side-by-side
        // vitals (the other interface's).
        assert!(caption("Accept Allegiance Requests"));
        assert!(caption("Global Allegiance Chat"));
        assert!(caption("Automatically Create Shortcuts"));
        assert!(!caption("Side By Side Vitals"));
        // Rows are evenly spaced down the page.
        for (k, r) in rows().iter().enumerate() {
            assert_eq!(r.y, 6 + ROW_HEIGHT * i32::try_from(k).unwrap());
        }
    }
    #[test]
    fn all_fifty_options_have_unique_bindings_and_headings_cannot_toggle() {
        let mut pairs = std::collections::HashSet::new();
        let mut model = OptionsModel::default();
        for (i, row) in rows().iter().enumerate() {
            if row.kind == "heading" {
                assert!(!model.toggle(i));
            } else {
                assert!(pairs.insert((row.word, row.bit)));
            }
        }
        assert_eq!(pairs.len(), 50);
    }
}
