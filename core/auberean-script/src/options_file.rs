//! The gameplay options' text form: what the player's options are exported to and imported from (a
//! text tree, as the key maps' files are).
//!
//! ```text
//! "GAMEPLAY OPTIONS"
//! [
//!   Aliases [ name [ text ] ... ]
//!   Shortcuts
//!   DamageText [ All [ true ] Other [ false ] Range [ -1 ] ]
//!   ...
//! ]
//! ```
//!
//! Only the aliases and the damage text's options are written; the other sections (the windows'
//! places, the chat filters, auto-targeting) are written as bare names and read as nothing (a
//! confirmations section is read too). An import reads into a new profile, so everything the
//! file does not hold is a new profile's. The aliases are written and read in the English code
//! page, as the client's narrow text is.

use auberean_dat::tree::{self, Node};
use auberean_text::wide::{narrow, widen};

use crate::options::GameplayOptions;

/// The options' node and its sections' names.
pub const GAMEPLAY_OPTIONS: &[u8] = b"GAMEPLAY OPTIONS";
const ALIASES: &[u8] = b"Aliases";
const SHORTCUTS: &[u8] = b"Shortcuts";
const DAMAGE_TEXT: &[u8] = b"DamageText";
const UI_SAVE_LOCATIONS: &[u8] = b"UISaveLocations";
const FILTER_SETTINGS: &[u8] = b"FilterSettings";
const AUTO_TARGET: &[u8] = b"AutoTarget";
const CONFIRMATION_SETTINGS: &[u8] = b"ConfirmationSettings";
const ALL: &[u8] = b"All";
const OTHER: &[u8] = b"Other";
const RANGE: &[u8] = b"Range";

/// The damage text option bits: shown for every attack, shown for others'.
const SHOW_DAMAGE_TEXT: u32 = 4;
const SHOW_DAMAGE_TEXT_OTHER: u32 = 8;

/// The errors an export or import answers: no file name, the options not written or not read,
/// the file not written.
pub const NO_FILE_NAME: u32 = 0xC353;
pub const NOT_READ: u32 = 50_000;
pub const NOT_WRITTEN: u32 = 0xC352;
pub const NO_OPTIONS: u32 = 0xC354;

impl GameplayOptions {
    /// Show others' damage text, or not.
    // D2-01376
    pub fn set_show_damage_text_other(&mut self, on: bool) -> bool {
        self.set_bits(on, SHOW_DAMAGE_TEXT_OTHER)
    }

    /// The options as their text tree's node: the aliases, a bare shortcuts section, the damage
    /// text's options, bare sections for the windows' places, the chat filters and auto-targeting.
    // D2-11977, D2-11967, D2-11956, D2-11958, D2-11960, D2-11962
    #[must_use]
    pub fn to_node(&self) -> Node {
        let mut n = Node::named(GAMEPLAY_OPTIONS);
        let a = n.make_child_text(ALIASES);
        for (name, text) in &self.aliases.entries {
            a.make_child_text(narrow(name))
                .make_child_text(narrow(text));
        }
        n.make_child_text(SHORTCUTS);
        let d = n.make_child_text(DAMAGE_TEXT);
        d.make_child_text(ALL)
            .make_child_bool(self.bits & SHOW_DAMAGE_TEXT != 0);
        d.make_child_text(OTHER)
            .make_child_bool(self.bits & SHOW_DAMAGE_TEXT_OTHER != 0);
        d.make_child_text(RANGE)
            .make_child_f32(f32::from_bits(self.damage_text_range));
        n.make_child_text(UI_SAVE_LOCATIONS);
        n.make_child_text(FILTER_SETTINGS);
        n.make_child_text(AUTO_TARGET);
        n
    }

    /// The options read from their node, over these: its sections in any order, each a known one
    /// (an unknown one fails the read), the aliases set and the damage text's options taken.
    // D2-12004, D2-11955, D2-11959, D2-11961, D2-11963, D2-11964
    pub fn from_node(&mut self, node: &mut Node) -> bool {
        if !node.is_keyword(GAMEPLAY_OPTIONS) {
            return false;
        }
        for i in 0..node.len() {
            let Some(c) = node.child_mut(i) else {
                return false;
            };
            let ok = if c.is_keyword(ALIASES) {
                self.aliases_from_node(c)
            } else if c.is_keyword(DAMAGE_TEXT) {
                self.damage_text_from_node(c)
            } else {
                [
                    SHORTCUTS,
                    UI_SAVE_LOCATIONS,
                    FILTER_SETTINGS,
                    AUTO_TARGET,
                    CONFIRMATION_SETTINGS,
                ]
                .iter()
                .any(|w| c.is_keyword(w))
            };
            if !ok {
                return false;
            }
        }
        true
    }

    /// Each alias node's name and its first child's, set as an alias and what it stands for; an
    /// alias with nothing under it fails the read.
    // D2-12001
    fn aliases_from_node(&mut self, node: &mut Node) -> bool {
        for i in 0..node.len() {
            let Some(c) = node.child_mut(i) else {
                return false;
            };
            if c.is_empty() {
                return false;
            }
            let (Some(name), Some(text)) = (c.name_text(), c.child_text(0)) else {
                return false;
            };
            if !self.set_alias(&widen(&name), &widen(&text)) {
                return false;
            }
        }
        true
    }

    /// The damage text's options: `All` and `Other` flags, `Range` a number; anything else, a
    /// section with nothing under it or a range not a number fails the read.
    // D2-11957
    fn damage_text_from_node(&mut self, node: &mut Node) -> bool {
        for i in 0..node.len() {
            let Some(c) = node.child_mut(i) else {
                return false;
            };
            let all = c.is_keyword(ALL);
            let other = c.is_keyword(OTHER);
            let range = c.is_keyword(RANGE);
            if !(all || other || range) {
                return false;
            }
            let Some(v) = c.child_mut(0) else {
                return false;
            };
            if range {
                let Some(f) = v.name_f32() else {
                    return false;
                };
                self.damage_text_range = f.to_bits();
            } else {
                let on = v.name_bool().unwrap_or(false);
                if all {
                    if on {
                        self.bits |= SHOW_DAMAGE_TEXT;
                    } else {
                        self.bits &= !SHOW_DAMAGE_TEXT;
                    }
                } else {
                    self.set_show_damage_text_other(on);
                }
            }
        }
        true
    }

    /// The options as the file an export writes (its line breaks as a carriage return and a line
    /// feed), or the error: none for an empty file name.
    ///
    /// # Errors
    /// [`NO_FILE_NAME`] when `filename` is empty.
    // D2-11978
    pub fn export(&self, filename: &str) -> Result<Vec<u8>, u32> {
        if filename.is_empty() {
            return Err(NO_FILE_NAME);
        }
        let mut root = Node::new();
        root.adopt(self.to_node());
        Ok(tree::to_file_bytes(&mut root))
    }

    /// A new profile read from an export's file (`bytes`, as read), or the error: an empty file
    /// name, a file that does not parse or whose options do not read, or one holding nothing.
    ///
    /// # Errors
    /// [`NO_FILE_NAME`], [`NOT_READ`] or [`NO_OPTIONS`].
    // D2-12005
    pub fn import(filename: &str, bytes: &[u8]) -> Result<Self, u32> {
        if filename.is_empty() {
            return Err(NO_FILE_NAME);
        }
        let mut root = tree::parse_bytes(filename.as_bytes(), bytes).ok_or(NOT_READ)?;
        let node = root.child_mut(0).ok_or(NO_OPTIONS)?;
        let mut o = Self::default();
        if o.from_node(node) {
            Ok(o)
        } else {
            Err(NOT_READ)
        }
    }
}
