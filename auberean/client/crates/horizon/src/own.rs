//! Horizon's own settings, which no other interface has: kept in one of the player's files beside
//! the client's preferences ([`FILE`]), a line each, `name value`. Read as Horizon is first shown,
//! and written whenever one changes. The scale and the vista are always written, the others only
//! where they differ from their defaults (the vista shown last only once one was picked at
//! random, so the next pick is another). The chat tabs' names and the log window's opacities are
//! this interface's alone: the server keeps each chat window's filter, never what its tab is
//! called.

use std::fmt::Write as _;

/// The file Horizon's own settings are kept in.
pub const FILE: &str = "auberean-horizon.txt";

/// The smallest and largest scale the file may hold; outside them it is Auto.
pub const SCALE_RANGE: (f32, f32) = (0.5, 3.0);

/// How many chat tabs the log window has.
pub const TABS: usize = crate::ui::chat::WINDOWS;

/// How opaque a log window's ground is at first, active and not.
pub const CHAT_OPACITY: (f32, f32) = (
    dereth_ui_kit::hud::LOG_OPACITY,
    dereth_ui_kit::hud::LOG_OPACITY,
);

/// The longest name a chat tab takes, in characters.
pub const TAB_NAME_MAX: usize = 32;

/// Horizon's own settings.
#[derive(Clone, Debug, PartialEq)]
pub struct Own {
    /// The interface scale the player chose; none for Auto ([`crate::scale`]).
    pub scale: Option<f32>,
    /// The view behind character select and creation.
    pub vista: crate::vista::Choice,
    /// The view last picked at random, which the next pick is not.
    pub vista_last: Option<crate::vista::Vista>,
    /// The map's view follows the player as the map opens.
    pub map_follow: bool,
    /// The log window's width and height, in layout units, as the player sized it by its corner;
    /// none for its default.
    pub chat_size: Option<(f32, f32)>,
    /// How opaque the log window's ground is (the first) and each chat tab's popped out, while it
    /// is active (under the pointer, or its line being typed) and while not, from 0 to 1.
    pub chat_opacity: [(f32, f32); TABS],
    /// Each chat tab's name as the player typed it; empty for its own name.
    pub tab_names: [String; TABS],
    /// Where each chat tab popped out of the log window was last put, its top left in layout
    /// units; none until it is moved. The first tab never pops out.
    pub chat_popped: [Option<(f32, f32)>; TABS],
    /// How large each chat tab popped out was last made, its tab included, in layout units; none
    /// for its first size.
    pub chat_popped_size: [Option<(f32, f32)>; TABS],
}

impl Default for Own {
    fn default() -> Self {
        Self {
            scale: None,
            vista: crate::vista::Choice::default(),
            vista_last: None,
            map_follow: true,
            chat_size: None,
            chat_opacity: [CHAT_OPACITY; TABS],
            tab_names: Default::default(),
            chat_popped: [None; TABS],
            chat_popped_size: [None; TABS],
        }
    }
}

/// A pair of finite numbers from `words`, the second required; with `positive`, both above 0.
fn pair<'a>(mut words: impl Iterator<Item = &'a str>, positive: bool) -> Option<(f32, f32)> {
    let a = words.next()?.parse::<f32>().ok()?;
    let b = words.next()?.parse::<f32>().ok()?;
    (a.is_finite() && b.is_finite() && (!positive || (a > 0.0 && b > 0.0))).then_some((a, b))
}

/// The chat tab a file's tab number names (1 the first), as an index.
fn tab_of(word: Option<&str>) -> Option<usize> {
    word?
        .parse::<usize>()
        .ok()?
        .checked_sub(1)
        .filter(|t| *t < TABS)
}

/// An opacity active and not from `words`, each held between 0 and 1.
fn opacity<'a>(words: impl Iterator<Item = &'a str>) -> Option<(f32, f32)> {
    pair(words, false).map(|(a, b)| (a.clamp(0.0, 1.0), b.clamp(0.0, 1.0)))
}

/// `on` or `off` as a yes or a no; anything else, none.
fn switch(word: &str) -> Option<bool> {
    match word {
        "on" => Some(true),
        "off" => Some(false),
        _ => None,
    }
}

const fn word(on: bool) -> &'static str {
    if on {
        "on"
    } else {
        "off"
    }
}

impl Own {
    /// Tab `tab` renamed `name`: spaces at its ends taken off, cut to [`TAB_NAME_MAX`]
    /// characters; an empty name gives it its own name again.
    pub fn set_tab_name(&mut self, tab: usize, name: &str) {
        if let Some(n) = self.tab_names.get_mut(tab) {
            *n = name.trim().chars().take(TAB_NAME_MAX).collect();
        }
    }

    /// The settings `text` holds: a line it does not know (one an earlier Horizon wrote, such as
    /// the tabs' lights), or a value that does not read, is left at its default.
    #[must_use]
    pub fn read(text: &str) -> Self {
        let mut own = Self::default();
        for line in text.lines() {
            let mut words = line.split_whitespace();
            match (words.next(), words.next()) {
                (Some("scale"), Some(v)) => {
                    own.scale = v
                        .parse::<f32>()
                        .ok()
                        .filter(|s| s.is_finite() && (SCALE_RANGE.0..=SCALE_RANGE.1).contains(s));
                }
                (Some("vista"), Some(v)) => {
                    own.vista = crate::vista::Choice::of_word(v).unwrap_or_default();
                }
                (Some("vista-last"), Some(v)) => {
                    own.vista_last = crate::vista::Vista::of_word(v);
                }
                (Some("map-follow"), Some(v)) => {
                    own.map_follow = switch(v).unwrap_or(own.map_follow);
                }
                (Some("chat-size"), Some(w)) => {
                    own.chat_size = pair(std::iter::once(w).chain(words), true);
                }
                (Some("chat-opacity"), n) => {
                    if let (Some(tab), Some(o)) = (tab_of(n), opacity(words)) {
                        own.chat_opacity[tab] = o;
                    }
                }
                (Some("tab-name"), n) => {
                    // The name is the rest of the line after the tab's number, spaces and all.
                    if let Some(tab) = tab_of(n) {
                        let rest = line.trim_start()["tab-name".len()..].trim_start();
                        let name = rest.split_once(char::is_whitespace).map_or("", |(_, r)| r);
                        own.set_tab_name(tab, name);
                    }
                }
                (Some("chat-popped"), n) => {
                    if let Some(tab) = tab_of(n).filter(|t| *t > 0) {
                        own.chat_popped[tab] = pair(words, false);
                    }
                }
                (Some("chat-popped-size"), n) => {
                    if let Some(tab) = tab_of(n).filter(|t| *t > 0) {
                        own.chat_popped_size[tab] = pair(words, true);
                    }
                }
                _ => {}
            }
        }
        own
    }

    /// The settings as the file keeps them.
    #[must_use]
    pub fn write(&self) -> String {
        let mut text = match self.scale {
            Some(s) => format!("scale {s}\n"),
            None => "scale auto\n".to_owned(),
        };
        let _ = writeln!(text, "vista {}", self.vista.word());
        if let Some(v) = self.vista_last {
            let _ = writeln!(text, "vista-last {}", v.word());
        }
        if !self.map_follow {
            let _ = writeln!(text, "map-follow {}", word(self.map_follow));
        }
        if let Some((w, h)) = self.chat_size {
            let _ = writeln!(text, "chat-size {w} {h}");
        }
        for (tab, (on, off)) in self.chat_opacity.iter().enumerate() {
            if (*on, *off) != CHAT_OPACITY {
                let _ = writeln!(text, "chat-opacity {} {on} {off}", tab + 1);
            }
        }
        for (tab, name) in self.tab_names.iter().enumerate() {
            if !name.is_empty() {
                let _ = writeln!(text, "tab-name {} {name}", tab + 1);
            }
        }
        for (tab, at) in self.chat_popped.iter().enumerate() {
            if let Some((x, y)) = at {
                let _ = writeln!(text, "chat-popped {} {x} {y}", tab + 1);
            }
        }
        for (tab, size) in self.chat_popped_size.iter().enumerate() {
            if let Some((w, h)) = size {
                let _ = writeln!(text, "chat-popped-size {} {w} {h}", tab + 1);
            }
        }
        text
    }
}
