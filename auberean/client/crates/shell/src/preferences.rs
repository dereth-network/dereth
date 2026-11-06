//! The client's preferences as its startup reads them: values asked for by section and key (each
//! an enumeration value, named by the data's two preference enumerations), looked up first among
//! those the command line set and then in the preferences file ([`Profile`]).
//!
//! A value set here (from the command line) is kept for this run only; the file is never written
//! from here. A value asked for by name reads from the file alone, and only when the file, the
//! section and the key all have names; an empty value is no value.

use std::collections::BTreeMap;

use auberean_assets::strings::EnumMap;
use auberean_dat::tree::number;
use auberean_world::records::WorldRecords;

use crate::profile::Profile;

/// The enumerations that name the preferences' sections and their keys (their enum maps' values in
/// the enum maps' mapping).
pub const SECTION_NAMES: u32 = 0x4000_003D;
pub const KEY_NAMES: u32 = 0x4000_003E;
const ENUM_MAPS: u32 = 0x4000_0001;

/// The preferences' sections the client itself asks for.
pub mod section {
    pub const KEY_MAP: u32 = 0x4000_0006;
    pub const NET: u32 = 0x4000_000F;
    pub const INTERNATIONAL: u32 = 0x4000_0010;
}

/// Keys the client itself asks for.
pub mod key {
    pub const LANGUAGE: u32 = 0x4000_003C;
    pub const COMPUTE_UNIQUE_PORT: u32 = 0x4000_0070;
    pub const USER_SPECIFIED_PORT: u32 = 0x4000_0071;
    pub const USE_INPUT_METHOD_EDITOR: u32 = 0x4000_0077;
}

/// The size of the buffer a value is read into, its terminating zero included.
const VALUE_SIZE: usize = 0x400;

/// An enumeration's names: its enum map and the maps it builds on, the map itself first.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EnumNames {
    maps: Vec<EnumMap>,
}

impl EnumNames {
    /// An enumeration's names from the data (none when the data has no map for it).
    #[must_use]
    pub fn read(records: &WorldRecords, enumeration: u32) -> Self {
        let mut maps = Vec::new();
        let mut id = records.data_id_by_enum(enumeration, ENUM_MAPS);
        for _ in 0..8 {
            let Some(m) = records.get::<EnumMap>(id) else {
                break;
            };
            id = m.base;
            maps.push(m);
            if id == 0 {
                break;
            }
        }
        Self { maps }
    }

    /// Names given directly: each value and its name.
    #[must_use]
    pub fn of(names: &[(u32, &str)]) -> Self {
        Self {
            maps: vec![EnumMap {
                id: 0,
                base: 0,
                names: names.iter().map(|(v, n)| (*v, (*n).to_owned())).collect(),
            }],
        }
    }

    /// A value's name: the map's own, else its bases'.
    #[must_use]
    pub fn name(&self, value: u32) -> Option<&str> {
        self.maps.iter().find_map(|m| m.name(value))
    }

    /// The value a name names, letter case ignored: the map's own, else its bases'.
    #[must_use]
    pub fn value(&self, name: &str) -> Option<u32> {
        self.maps.iter().find_map(|m| m.enum_of(name))
    }
}

/// The names of the preferences' sections and of their keys.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Names {
    pub sections: EnumNames,
    pub keys: EnumNames,
}

impl Names {
    /// From the data.
    #[must_use]
    pub fn read(records: &WorldRecords) -> Self {
        Self {
            sections: EnumNames::read(records, SECTION_NAMES),
            keys: EnumNames::read(records, KEY_NAMES),
        }
    }

    /// A preference's section and key by name.
    #[must_use]
    pub fn of(&self, section: u32, key: u32) -> Option<(&str, &str)> {
        Some((self.sections.name(section)?, self.keys.name(key)?))
    }
}

/// The preferences: the values set for this run, over the file.
// D2-16059, D2-16060, D2-16061
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Preferences {
    /// The file, if one was chosen.
    file: Option<Profile>,
    names: Names,
    /// The values set, by section and key.
    set: BTreeMap<u32, BTreeMap<u32, String>>,
    /// The sections and keys a value was set in, each once, in the order first set.
    sections: Vec<u32>,
    keys: Vec<u32>,
}

impl Preferences {
    /// The preferences over a file, named as the data names them.
    // D2-16064
    #[must_use]
    pub fn startup(file: Profile, names: Names) -> Self {
        Self {
            file: Some(file),
            names,
            ..Self::default()
        }
    }

    /// The file read from.
    #[must_use]
    pub fn file(&self) -> Option<&Profile> {
        self.file.as_ref()
    }

    /// A value from the file by its section's and key's names: none when there is no file, either
    /// name is empty, or the value is (or is not there).
    // D2-16049
    #[must_use]
    pub fn load_value_by_name(&self, section: &str, key: &str) -> Option<String> {
        let file = self.file.as_ref()?;
        if section.is_empty() || key.is_empty() {
            return None;
        }
        let v = file.read(section, key, "", VALUE_SIZE);
        (!v.is_empty()).then_some(v)
    }

    /// A value from the file by its section and key, each named as the data names it.
    // D2-16050
    #[must_use]
    pub fn load_value(&self, section: u32, key: u32) -> Option<String> {
        let (s, k) = self.names.of(section, key)?;
        self.load_value_by_name(s, k)
    }

    /// A value set for this run.
    // D2-16051
    #[must_use]
    pub fn lookup(&self, section: u32, key: u32) -> Option<&str> {
        self.set.get(&section)?.get(&key).map(String::as_str)
    }

    /// A number: the value set, else the file's, when it reads whole as an unsigned 32-bit number
    /// (decimal, `0x` hexadecimal or `0` octal). A yes-or-no is asked for the same way.
    // D2-16054
    #[must_use]
    pub fn inq_uint32(&self, section: u32, key: u32) -> Option<u32> {
        let v = match self.lookup(section, key) {
            Some(v) => v.to_owned(),
            None => self.load_value(section, key)?,
        };
        number::is_uint32(v.as_bytes()).then(|| number::strtoul(v.as_bytes()).value)
    }

    /// A yes-or-no: the number asked for, not zero.
    #[must_use]
    pub fn inq_bool(&self, section: u32, key: u32) -> Option<bool> {
        self.inq_uint32(section, key).map(|v| v != 0)
    }

    /// A text: the value set, else the file's.
    // D2-16055
    #[must_use]
    pub fn inq_string(&self, section: u32, key: u32) -> Option<String> {
        match self.lookup(section, key) {
            Some(v) => Some(v.to_owned()),
            None => self.load_value(section, key),
        }
    }

    /// Set a value for this run.
    // D2-16057
    fn store(&mut self, section: u32, key: u32, value: String) {
        self.set.entry(section).or_default().insert(key, value);
    }

    fn note(&mut self, section: u32, key: u32) {
        if !self.sections.contains(&section) {
            self.sections.push(section);
        }
        if !self.keys.contains(&key) {
            self.keys.push(key);
        }
    }

    /// Set a yes-or-no (any number) for this run, kept as its decimal text.
    // D2-16062
    pub fn set_bool(&mut self, section: u32, key: u32, value: i32) {
        self.note(section, key);
        self.store(section, key, value.to_string());
    }
}

/// The languages enumeration, and the language the client runs in when none is named: English.
pub const LANGUAGES: u32 = 0x4000_002A;
pub const ENGLISH: u32 = 0x8000_0001;

/// The language to run in: the command line's, else the preferences' (`[International]
/// Language`, its ends trimmed), as its name in the languages enumeration names it (letter case
/// ignored); English when neither names one there.
// D2-00111
#[must_use]
pub fn language(line: Option<&str>, prefs: &Preferences, languages: &EnumNames) -> u32 {
    let named = match line.filter(|l| !l.is_empty()) {
        Some(l) => l.to_owned(),
        None => {
            let p = prefs
                .inq_string(section::INTERNATIONAL, key::LANGUAGE)
                .unwrap_or_default();
            let p = p.trim();
            if p.is_empty() {
                return ENGLISH;
            }
            p.to_owned()
        }
    };
    languages.value(&named).unwrap_or(ENGLISH)
}
