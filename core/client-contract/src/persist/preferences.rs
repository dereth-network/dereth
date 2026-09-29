//! `UserPreferences.ini` and the options metadata layered over it.
//!
//! `UserPreferences` is a plain INI in the **install** directory with **one section per category** —
//! `[Sound]`, `[Render]`, `[Display]`, `[Camera]`, `[Input]`, `[Misc]`, `[UI]`, `[Net]`,
//! `[International]` — and the registry name is `sprintf("%s.%s", section, key)` on the way in,
//! which the save splits back apart at the name's **last** dot.
//! The all-static options façade *decorates* an entry with the metadata an options
//! page needs.
//!
//! The name token is not computed here: it is `dereth_primitives::num::hash::str_hash`, which
//! `dereth-ui` exposes as the free function `dereth_ui::persist::preferences::token_of`. Everything
//! else is reachable through `dereth_ui::persist::preferences`'s re-export.

use std::collections::BTreeMap;

use super::PersistError;

/// The file name the default-file search builds, and that the client seeds its preferences-file
/// field with.
pub const FILE_NAME: &str = "UserPreferences.ini";

/// The section a name with **no** `.` in it falls back to.
///
/// It is *not* "the one section the client writes"; a parser that assumes so loses the section.
/// The save splits every registered name at its **last**
/// dot and writes `WritePrivateProfileStringA(<before>, <after>, value, path)`, so a real file is
/// `[Sound]` / `SoundVolume=…`, `[Render]` / `TextureFiltering=…`, one section per category. All 43
/// registered preferences contain a dot, so in practice `Default` never appears in a file the
/// client wrote.
pub const SECTION: &str = "Default";

/// The parsed `.ini`.
///
/// Entries keep their file order so a re-save is byte-identical: an INI reader that sorts its keys
/// cannot round-trip a file it did not write.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UserPreferences {
    /// `(key, value)` in file order, keys **qualified** as `Category.Name`.
    ///
    /// The load's fifth step builds the name it pushes into the global registry with
    /// `sprintf("%s.%s", section, key)` — one format string, used by the load and by
    /// nothing else in retail. The bare
    /// key as written in the file is *not* what the registry is keyed by, and a reader that pushes
    /// it is looking `TextureFiltering` up in a registry that holds `Render.TextureFiltering`: it
    /// matches nothing at all.
    pub entries: Vec<(String, String)>,
    /// Lines that are not a `key=value` pair — blanks, comments **and section headers** — recorded
    /// with the index of the entry they precede so they survive a round trip in their original
    /// order. Headers live here rather than in a field of their own precisely so that a blank line
    /// written before a `[Section]` cannot be re-emitted after it.
    pub interleaved: Vec<(usize, String)>,
    /// The section *name* each header opened, with the index of the first entry under it. Used only
    /// by [`Self::to_text`], to take the prefix back off a qualified key.
    pub sections: Vec<(usize, String)>,
    /// Whether the file used CRLF. `fopen(path, "w")` is text mode on Windows, so a file the client
    /// wrote has CRLF.
    pub crlf: bool,
}

impl UserPreferences {
    /// Look a **qualified** `Category.Name` up, case-sensitively. The registry's own lookup is
    /// case-insensitive, and the reader that needs that is
    /// `dereth_ui_screens::options::store::load`.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// The bool overload of preference registration.
    #[must_use]
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        match self.get(key)?.trim() {
            "1" | "true" | "True" | "TRUE" => Some(true),
            "0" | "false" | "False" | "FALSE" => Some(false),
            _ => None,
        }
    }

    /// The float overload.
    #[must_use]
    pub fn get_f32(&self, key: &str) -> Option<f32> {
        self.get(key)?.trim().parse().ok()
    }

    /// Set a **qualified** key. A key that is not already present is appended, which puts it under
    /// whatever section was last opened; rewriting the file properly is the client's job and
    /// is not this type's.
    pub fn set(&mut self, key: &str, value: &str) {
        if let Some(e) = self.entries.iter_mut().find(|(k, _)| k == key) {
            e.1 = value.to_string();
        } else {
            self.entries.push((key.to_string(), value.to_string()));
        }
    }

    /// The load's steps 3-5.
    ///
    /// Retail enumerates section names (`GetPrivateProfileSectionNamesA`), then reads each section
    /// (`GetPrivateProfileSectionA`) and qualifies every key with its section. This is that, done
    /// in one pass over the text: for a file written by `WritePrivateProfileStringA`, where a
    /// section's keys are contiguous, it visits the same keys and builds the same names.
    ///
    /// A key seen **before** any `[Section]` header keeps its bare name. Retail cannot reach that
    /// state at all — `GetPrivateProfileSection` only ever hands out keys that are under a header —
    /// so the choice is free, and keeping the bare name leaves the two other readers in this
    /// workspace (`dereth_audio::Prefs::from_ini` and `dereth_client::config::Preferences::parse`, both
    /// of which default the section to `Default`) as the *more* forgiving ones rather than this.
    pub fn parse(text: &str) -> Result<Self, PersistError> {
        let crlf = text.contains("\r\n");
        let mut out = Self {
            crlf,
            ..Self::default()
        };
        let mut section = String::new();
        for raw in text.split('\n') {
            let line = raw.strip_suffix('\r').unwrap_or(raw);
            let t = line.trim();
            if t.len() >= 2 && t.starts_with('[') && t.ends_with(']') {
                section = t[1..t.len() - 1].to_string();
                out.sections.push((out.entries.len(), section.clone()));
                out.interleaved.push((out.entries.len(), line.to_string()));
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                if !k.trim().is_empty() {
                    let key = if section.is_empty() {
                        k.to_string()
                    } else {
                        format!("{section}.{k}")
                    };
                    out.entries.push((key, v.to_string()));
                    continue;
                }
            }
            out.interleaved.push((out.entries.len(), line.to_string()));
        }
        // A trailing newline leaves one empty interleaved line; drop it so the writer's own
        // terminator does not duplicate it.
        if out
            .interleaved
            .last()
            .is_some_and(|(i, l)| *i == out.entries.len() && l.is_empty())
        {
            out.interleaved.pop();
        }
        Ok(out)
    }

    /// The section in force at `entries[i]` — the last header at or before it.
    fn section_at(&self, i: usize) -> Option<&str> {
        self.sections
            .iter()
            .rev()
            .find(|(at, _)| *at <= i)
            .map(|(_, s)| s.as_str())
    }

    /// The key as it appears **in the file**: the qualified name with its section prefix taken back
    /// off, which is the save's split at the last dot run in reverse.
    fn bare_key<'a>(&self, i: usize, key: &'a str) -> &'a str {
        match self.section_at(i) {
            Some(s) if !s.is_empty() => key
                .strip_prefix(s)
                .and_then(|rest| rest.strip_prefix('.'))
                .unwrap_or(key),
            _ => key,
        }
    }

    /// The index one past the last entry that belongs to `section`, or the header's own index when
    /// the section exists and holds nothing. `None` when the file has no such section.
    fn section_end(&self, section: &str) -> Option<usize> {
        let mut end = None;
        for i in 0..self.entries.len() {
            if self
                .section_at(i)
                .is_some_and(|s| s.eq_ignore_ascii_case(section))
            {
                end = Some(i + 1);
            }
        }
        end.or_else(|| {
            self.sections
                .iter()
                .find(|(_, s)| s.eq_ignore_ascii_case(section))
                .map(|(at, _)| *at)
        })
    }

    /// `WritePrivateProfileStringA(section, key, value, path)` — the one call the save writes
    /// every preference with, and Win32's **merge**
    /// semantics rather than a rewrite.
    ///
    /// Three behaviours, and all three matter to a file the client did not write alone:
    ///
    /// 1. an existing key is replaced **in place**, so every other line keeps its position;
    /// 2. a new key is appended to the **end of its own section**, not to the end of the file —
    ///    which is what keeps `[Sound]`'s keys contiguous, and therefore what keeps
    ///    `GetPrivateProfileSectionA` (and [`Self::parse`], which is that in one pass) able to
    ///    read them back under the right section name;
    /// 3. a section that is not in the file yet gets a `[Section]` header appended first.
    ///
    /// Keys the registry does not know — `Net.*`, `Input.KeymapFile`, anything a later client
    /// version added — are never visited and therefore survive untouched. That is the whole reason
    /// this is a merge: the save walks the *registry*, not the file, and a rewrite would silently
    /// drop every preference this build has not transcribed.
    ///
    /// Returns whether the file changed.
    pub fn write_profile_string(&mut self, section: &str, key: &str, value: &str) -> bool {
        let qualified = Self::key(section, key);
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|(k, _)| k.eq_ignore_ascii_case(&qualified))
        {
            let changed = e.1 != value;
            e.1 = value.to_string();
            return changed;
        }
        let (at, new_section) = match self.section_end(section) {
            Some(at) => (at, false),
            None => (self.entries.len(), true),
        };
        // Everything that was anchored at or after `at` moves one along, so that a `[Render]`
        // header that sat in front of `entries[at]` before the insert still sits in front of it.
        for (i, _) in &mut self.interleaved {
            if *i >= at {
                *i += 1;
            }
        }
        for (i, _) in &mut self.sections {
            if *i >= at {
                *i += 1;
            }
        }
        if new_section {
            self.sections.push((at, section.to_string()));
            self.interleaved.push((at, format!("[{section}]")));
        }
        self.entries.insert(at, (qualified, value.to_string()));
        true
    }

    /// The save, reproducing the input byte for byte when nothing changed.
    #[must_use]
    pub fn to_text(&self) -> String {
        let nl = if self.crlf { "\r\n" } else { "\n" };
        let mut lines: Vec<String> = Vec::new();
        for (i, (k, v)) in self.entries.iter().enumerate() {
            for (at, l) in &self.interleaved {
                if *at == i {
                    lines.push(l.clone());
                }
            }
            lines.push(format!("{}={v}", self.bare_key(i, k)));
        }
        for (at, l) in &self.interleaved {
            if *at == self.entries.len() {
                lines.push(l.clone());
            }
        }
        let mut s = lines.join(nl);
        s.push_str(nl);
        s
    }

    /// The key format, `"%s.%s"` = `Category.Name`.
    #[must_use]
    pub fn key(category: &str, name: &str) -> String {
        format!("{category}.{name}")
    }
}

/// The UI's own registered preferences.
pub mod keys {
    /// Bound to the UI element manager's tooltip delay.
    pub const TOOLTIP_DELAY: &str = "Misc.TooltipDelay";
    /// Bound to the UI element manager's tooltip enable flag.
    pub const TOOLTIP_ENABLE: &str = "Misc.TooltipEnable";
    /// Bound to the process-wide IME preference.
    pub const USE_IME: &str = "International.UseIME";
    pub const KEYMAP_FILE: &str = "Input.KeymapFile";
    pub const CHAT_FONT_FACE: &str = "UI.ChatFontFace";
    pub const CHAT_FONT_SIZE: &str = "UI.ChatFontSize";
}

/// The type supplied when attaching a preference.
///
/// > 2 = enum, 3 = float, 4 = bool [verified from the call sites; 1 and 5 are presumably int and
/// > string].
///
/// 1 and 5 are unused in this build and their assignment is inferred. Only 2/3/4 are implemented,
/// which is every shipped call site; 1 and 5 are named here and rejected by
/// [`UiPreferenceItem::new`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreferenceDataType {
    Enum = 2,
    Float = 3,
    Bool = 4,
}

impl PreferenceDataType {
    #[must_use]
    pub const fn from_u32(v: u32) -> Option<Self> {
        Some(match v {
            2 => Self::Enum,
            3 => Self::Float,
            4 => Self::Bool,
            // 1 (int) and 5 (string) are inferred and unused.
            _ => return None,
        })
    }
}

/// Fields shared by the preference metadata entries.
#[derive(Debug, Clone, PartialEq)]
pub struct UiPreferenceItem {
    /// The preference name — the `Category.Name` key.
    pub preference: String,
    /// The data type.
    pub data_type: PreferenceDataType,
    /// The string table — **always** `0x10000003`, the UI string table.
    pub string_table: u32,
    /// The name token — the string hash of the localisation id.
    pub token_name: u32,
    /// The tooltip token.
    pub token_tooltip: u32,
    /// The preference's range, set and asked for by the options surface.
    pub range: Option<(f32, f32)>,
    /// The enumerated choices — the string-id list.
    pub choices: Vec<u32>,
}

/// The string table every preference attach passes.
pub const UI_STRING_TABLE: u32 = 0x1000_0003;

impl UiPreferenceItem {
    /// Attach a preference with its name, type, string-table enum, label token, and tooltip token.
    pub fn new(
        preference: &str,
        data_type: u32,
        string_table: u32,
        token_name: u32,
        token_tooltip: u32,
    ) -> Result<Self, PersistError> {
        let data_type = PreferenceDataType::from_u32(data_type).ok_or_else(|| {
            PersistError(format!(
                "UI preferences dataType {data_type} is not implemented: its layout is not known"
            ))
        })?;
        Ok(Self {
            preference: preference.to_string(),
            data_type,
            string_table,
            token_name,
            token_tooltip,
            range: None,
            choices: Vec::new(),
        })
    }
}

/// The options surface: a map from narrow-string names to preference metadata entries.
#[derive(Debug, Default, Clone)]
pub struct UiPreferences {
    items: BTreeMap<String, UiPreferenceItem>,
}

impl UiPreferences {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn attach(&mut self, item: UiPreferenceItem) {
        self.items.insert(item.preference.clone(), item);
    }

    /// Detach a preference.
    pub fn detach(&mut self, key: &str) {
        self.items.remove(key);
    }

    /// Ask for a preference.
    #[must_use]
    pub fn inq(&self, key: &str) -> Option<&UiPreferenceItem> {
        self.items.get(key)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the load's step 5 (`sprintf("%s.%s", section, key)`) and the save's step 2
    /// (split at the **last** dot, section before, key after), which together make a
    /// **byte-identical** round trip. The fixture has the shape a file the client wrote actually
    /// has: one section per category.
    #[test]
    fn the_ini_round_trips_byte_identically() {
        let text = "[Misc]\r\nTooltipDelay=0.50\r\nTooltipEnable=1\r\n\
                    [International]\r\nUseIME=0\r\n[UI]\r\nChatFontSize=Medium\r\n";
        let p = UserPreferences::parse(text).unwrap();
        assert_eq!(p.to_text(), text, "byte-identical, headers and all");
        // The key the registry is keyed by is the qualified one — never the bare `TooltipDelay`.
        assert_eq!(p.get(keys::TOOLTIP_DELAY), Some("0.50"));
        assert_eq!(
            p.get("TooltipDelay"),
            None,
            "the bare key is not a registry name"
        );
        assert_eq!(p.get_f32(keys::TOOLTIP_DELAY), Some(0.5));
        assert_eq!(p.get_bool(keys::TOOLTIP_ENABLE), Some(true));
        assert_eq!(p.get_bool(keys::USE_IME), Some(false));
        assert_eq!(p.get(keys::CHAT_FONT_SIZE), Some("Medium"));
        assert_eq!(p.get("Nope.Missing"), None);
        assert_eq!(p.sections.len(), 3);
        assert!(p.crlf);
    }

    /// The same, with LF and with comment and blank lines interleaved **between** sections: an INI
    /// reader that sorts, drops what it does not understand, or re-emits a header ahead of the
    /// blank line that preceded it cannot round-trip a file it did not write.
    #[test]
    fn comments_blank_lines_section_headers_and_key_order_all_survive() {
        let text = "[Sound]\n; a comment\nSoundVolume=1.00\n\n[Render]\nFieldOfView=90.00\n";
        let p = UserPreferences::parse(text).unwrap();
        assert_eq!(p.entries.len(), 2);
        assert_eq!(
            p.entries[0].0, "Sound.SoundVolume",
            "qualified, and in file order"
        );
        assert_eq!(p.entries[1].0, "Render.FieldOfView");
        assert_eq!(p.to_text(), text, "the blank line stays ahead of [Render]");
        assert!(!p.crlf);
    }

    /// A key ahead of any header keeps its bare name, and a `[Default]` header qualifies with
    /// `Default.` like any other — `Load`'s `"%s.%s"` has no special case for it, which is why a
    /// hand-written `[Default]` file full of already-dotted names matches **nothing** in the
    /// registry. Stated here because that was the shape three of this workspace's own fixtures had.
    #[test]
    fn the_default_section_is_not_special_cased() {
        let p = UserPreferences::parse("[Default]\r\nSound.SoundVolume=1.00\r\n").unwrap();
        assert_eq!(p.entries[0].0, "Default.Sound.SoundVolume");
        assert_eq!(p.to_text(), "[Default]\r\nSound.SoundVolume=1.00\r\n");
        let p = UserPreferences::parse("Stray=1\n[Sound]\nSoundVolume=1.00\n").unwrap();
        assert_eq!(p.entries[0].0, "Stray");
        assert_eq!(p.entries[1].0, "Sound.SoundVolume");
        assert_eq!(p.to_text(), "Stray=1\n[Sound]\nSoundVolume=1.00\n");
    }

    /// The key builder uses the `"%s.%s"` format of §3.1, and the six keys the UI
    /// registers are named there.
    #[test]
    fn the_registered_keys_are_the_documented_ones() {
        assert_eq!(
            UserPreferences::key("Misc", "TooltipDelay"),
            keys::TOOLTIP_DELAY
        );
        assert_eq!(keys::TOOLTIP_ENABLE, "Misc.TooltipEnable");
        assert_eq!(keys::USE_IME, "International.UseIME");
        assert_eq!(keys::KEYMAP_FILE, "Input.KeymapFile");
        assert_eq!(keys::CHAT_FONT_FACE, "UI.ChatFontFace");
        assert_eq!(keys::CHAT_FONT_SIZE, "UI.ChatFontSize");
    }

    /// Oracle: §3.2 — "`dataType`: 2 = enum, 3 = float, 4 = bool [verified from the call sites]",
    /// No shipped call site registers a preference of type 1 or 5, so those are refused.
    #[test]
    fn only_the_three_shipped_data_types_are_accepted() {
        for (v, t) in [
            (2, PreferenceDataType::Enum),
            (3, PreferenceDataType::Float),
            (4, PreferenceDataType::Bool),
        ] {
            assert_eq!(PreferenceDataType::from_u32(v), Some(t));
            assert!(UiPreferenceItem::new("Misc.X", v, UI_STRING_TABLE, 0, 0).is_ok());
        }
        for v in [0, 1, 5, 6] {
            assert_eq!(PreferenceDataType::from_u32(v), None);
            assert!(UiPreferenceItem::new("Misc.X", v, UI_STRING_TABLE, 0, 0).is_err());
        }
    }

    /// Oracle: the client's one write call,
    /// `WritePrivateProfileStringA(<before the last dot>, <after it>, value, path)`, and Win32's
    /// merge semantics for it.
    ///
    /// The three cases are the three a real profile hits: a key that is already there, a key that
    /// is new inside a section that exists, and a section that does not exist yet. The second is
    /// the one that matters and the one a naive `set` gets wrong — appending `SoundVolume` to the
    /// end of the file would put it under `[Camera]`, and it would come back as
    /// `Camera.SoundVolume`.
    #[test]
    fn write_profile_string_merges_rather_than_rewriting() {
        let text = "[Sound]\r\nSoundVolume=1.00\r\n[Camera]\r\nStiffness=0.45\r\n";
        let mut p = UserPreferences::parse(text).unwrap();

        // 1. in place, and the file is otherwise byte-identical.
        assert!(p.write_profile_string("Sound", "SoundVolume", "0.30"));
        assert!(
            !p.write_profile_string("Sound", "SoundVolume", "0.30"),
            "no change is no change"
        );
        assert_eq!(
            p.to_text(),
            "[Sound]\r\nSoundVolume=0.30\r\n[Camera]\r\nStiffness=0.45\r\n"
        );

        // 2. a new key lands at the end of **its own** section, ahead of the next header.
        assert!(p.write_profile_string("Sound", "SoundFeatures", "Mono"));
        assert_eq!(
            p.to_text(),
            "[Sound]\r\nSoundVolume=0.30\r\nSoundFeatures=Mono\r\n[Camera]\r\nStiffness=0.45\r\n"
        );
        // …and it is readable back under the right name, which is the point of putting it there.
        let back = UserPreferences::parse(&p.to_text()).unwrap();
        assert_eq!(back.get("Sound.SoundFeatures"), Some("Mono"));
        assert_eq!(back.get("Camera.SoundFeatures"), None);

        // 3. a section that is not in the file yet gets its header first.
        assert!(p.write_profile_string("UI", "ChatFontSize", "Large"));
        assert!(p
            .to_text()
            .ends_with("[Camera]\r\nStiffness=0.45\r\n[UI]\r\nChatFontSize=Large\r\n"));
        let back = UserPreferences::parse(&p.to_text()).unwrap();
        assert_eq!(back.get("UI.ChatFontSize"), Some("Large"));
        assert_eq!(back.entries.len(), 4);

        // A key nobody wrote is never touched: `Save` walks the registry, not the file.
        assert_eq!(back.get("Camera.Stiffness"), Some("0.45"));
    }
}
