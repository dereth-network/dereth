//! The client's bank of preferences: one of each preference, by key, as its owner described it and
//! the preferences file holds it; the owners told what changed, and the options window's sections
//! shown what each holds.
//!
//! **Putting a preference in.** Its owner describes a preference (its label, range or choices,
//! default) and blesses it with its section and key. The bank keeps its own copy: the first
//! blessed of a key is copied and read from the file (its value there, else its default, taken
//! as applied); a key already kept takes the new one's description and values. The owner's
//! preference then holds the bank's value.
//!
//! **Who is told.** A key may be linked to one handler (the part of the client that uses it) and
//! a section attached to one monitor (the options window's page that shows it). Telling is what
//! the bank's calls return, in order ([`Notice`]): a preference changed goes to its key's handler
//! and its section's monitor; a section's preferences go to its monitor when attached and when one
//! is added or dropped; submitting, applying or recalling a section hands each handler its own
//! preferences there (submitting and applying only the changed ones, applying marks them applied,
//! recalling puts each back first).
//!
//! **Keeping.** Every preference is written to the file by its section's and key's names, as its
//! value now.

use super::preference::{Enumerations, Pref};

/// The enumerations that name the preferences' sections and keys.
pub const SECTION_NAMES: u32 = 0x4000_003D;
pub const KEY_NAMES: u32 = 0x4000_003E;

/// The size of the buffer a preference's value is read into, its terminating zero included.
const VALUE_SIZE: usize = 100;

/// The preferences file as the bank reads and writes it.
pub trait PreferenceFile {
    /// A key's value in a section, else `default` (less its trailing spaces), cut to `size` bytes
    /// with a terminating zero.
    fn read(&self, section: &str, key: &str, default: &str, size: usize) -> String;
    /// Set a key's value in a section.
    fn write(&mut self, section: &str, key: &str, value: &str);
}

/// Who is told: a handler or a monitor, by the number the caller gave it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Party {
    Handler(u32),
    Monitor(u32),
}

/// What the bank tells.
#[derive(Clone, Debug, PartialEq)]
pub enum Notice {
    /// A handler takes these preferences of a section (submitted, applied or recalled).
    Handle { handler: u32, prefs: Vec<Pref> },
    /// A preference changed, told to its key's handler or its section's monitor.
    Change { party: Party, pref: Pref },
    /// A section's preferences, listed for its monitor.
    Reload {
        monitor: u32,
        section: u32,
        prefs: Vec<Pref>,
    },
}

/// A preference's section and key by their names in the data.
// D2-15922
#[must_use]
pub fn names_of(pref: &Pref, names: &dyn Enumerations) -> Option<(String, String)> {
    let (section, key) = pref.description().identify();
    Some((
        names.name_of(SECTION_NAMES, section)?,
        names.name_of(KEY_NAMES, key)?,
    ))
}

/// Read a preference from the file: its value there by its names (its default's text when not
/// there), taken as applied. Nothing is read for a preference with no section or key.
// D2-15925
pub fn load(pref: &mut Pref, file: &dyn PreferenceFile, names: &dyn Enumerations) -> bool {
    let (section, key) = pref.description().identify();
    if section == 0 || key == 0 {
        return false;
    }
    let (s, k) = names_of(pref, names).unwrap_or_default();
    let default = pref.default_text(names);
    let text = file.read(&s, &k, &default, VALUE_SIZE);
    pref.set_value_text(&text, names);
    pref.apply();
    true
}

/// Write a preference's value now to the file by its names. Nothing is written for a preference
/// with no section or key.
// D2-15924
pub fn save(pref: &Pref, file: &mut dyn PreferenceFile, names: &dyn Enumerations) -> bool {
    let (section, key) = pref.description().identify();
    if section == 0 || key == 0 {
        return false;
    }
    let (s, k) = names_of(pref, names).unwrap_or_default();
    file.write(&s, &k, &pref.value_text(names));
    true
}

/// The bank.
// D2-16031, D2-16041
#[derive(Clone, Debug, PartialEq)]
pub struct Bank {
    /// The preferences, in the order they were put in.
    prefs: Vec<Pref>,
    /// Each section's monitor.
    monitors: Vec<(u32, u32)>,
    /// Each key's handler.
    handlers: Vec<(u32, u32)>,
    /// The preferences file's name.
    file: String,
}

impl Default for Bank {
    fn default() -> Self {
        Self::new()
    }
}

impl Bank {
    /// An empty bank, keeping its preferences in `Preferences.ini`.
    // D2-16045, D2-16046
    #[must_use]
    pub fn new() -> Self {
        Self {
            prefs: Vec::new(),
            monitors: Vec::new(),
            handlers: Vec::new(),
            file: "Preferences.ini".to_owned(),
        }
    }

    /// The preferences file's name.
    #[must_use]
    pub fn file(&self) -> &str {
        &self.file
    }

    /// Every preference, in the order put in.
    #[must_use]
    pub fn prefs(&self) -> &[Pref] {
        &self.prefs
    }

    fn at(&self, key: u32) -> Option<usize> {
        if key == 0 {
            return None;
        }
        self.prefs
            .iter()
            .position(|p| p.description().identify().1 == key)
    }

    /// The preference kept for a key.
    // D2-16021
    #[must_use]
    pub fn find(&self, key: u32) -> Option<&Pref> {
        self.at(key).map(|i| &self.prefs[i])
    }

    fn handler(&self, key: u32) -> Option<u32> {
        self.handlers
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, h)| *h)
    }

    fn monitor(&self, section: u32) -> Option<u32> {
        self.monitors
            .iter()
            .find(|(s, _)| *s == section)
            .map(|(_, m)| *m)
    }

    /// Put a preference in: a key already kept takes its description and values (its handler and
    /// monitor told of it first); a new key is copied, read from the file, kept, and its section
    /// listed again for its monitor.
    // D2-16042
    pub fn set(
        &mut self,
        pref: &Pref,
        file: &dyn PreferenceFile,
        names: &dyn Enumerations,
        out: &mut Vec<Notice>,
    ) -> bool {
        let (section, key) = pref.description().identify();
        if key == 0 {
            return false;
        }
        if let Some(i) = self.at(key) {
            self.change(pref, out);
            self.prefs[i].copy_from(pref);
            return true;
        }
        let mut kept = pref.duplicate();
        load(&mut kept, file, names);
        self.prefs.push(kept);
        self.reload(section, out);
        true
    }

    /// The owner's preference holds the bank's value for its key (the same kind's).
    // D2-16023
    pub fn connect_source(&self, pref: &mut Pref) -> bool {
        let key = pref.description().identify().1;
        if key == 0 {
            return false;
        }
        self.find(key).is_some_and(|kept| pref.connect_target(kept))
    }

    /// Whether the owner's preference is of the kind the bank keeps for its key, so that it reads
    /// the bank's.
    // D2-16024, D2-15931
    #[must_use]
    pub fn connect_target(&self, pref: &Pref) -> bool {
        let key = pref.description().identify().1;
        key != 0
            && self
                .find(key)
                .is_some_and(|kept| kept.kind() == pref.kind())
    }

    /// Bless a preference with its section and key: put in (replacing what a key already has),
    /// then made to hold the bank's value; whether it does. A preference with no key is left
    /// alone.
    // D2-15920
    pub fn bless(
        &mut self,
        pref: &mut Pref,
        section: u32,
        key: u32,
        file: &dyn PreferenceFile,
        names: &dyn Enumerations,
        out: &mut Vec<Notice>,
    ) -> bool {
        if key == 0 {
            return false;
        }
        let d = pref.description_mut();
        d.section = section;
        d.key = key;
        self.set(pref, file, names, out);
        self.connect_source(pref) && self.connect_target(pref)
    }

    /// Create a preference with its section and key: put in unless the key is kept already, then
    /// made to hold the bank's value; whether it does.
    // D2-15919
    pub fn create(
        &mut self,
        pref: &mut Pref,
        section: u32,
        key: u32,
        file: &dyn PreferenceFile,
        names: &dyn Enumerations,
        out: &mut Vec<Notice>,
    ) -> bool {
        if key == 0 {
            return false;
        }
        let d = pref.description_mut();
        d.section = section;
        d.key = key;
        if self.find(key).is_none() {
            self.set(pref, file, names, out);
        }
        self.connect_source(pref) && self.connect_target(pref)
    }

    /// Set the bank's value for a preference's key to the preference's own (the page's preference
    /// writing through to the bank's).
    pub fn take_value(&mut self, pref: &Pref) -> bool {
        let key = pref.description().identify().1;
        match self.at(key) {
            Some(i) => self.prefs[i].connect_target(pref),
            None => false,
        }
    }

    /// Drop a key's preference and its handler; its section listed again for its monitor.
    // D2-16043
    pub fn destroy(&mut self, key: u32, out: &mut Vec<Notice>) -> bool {
        if key == 0 {
            return false;
        }
        self.handlers.retain(|(k, _)| *k != key);
        let Some(i) = self.at(key) else {
            return false;
        };
        let section = self.prefs[i].description().identify().0;
        self.prefs.swap_remove(i);
        self.reload(section, out);
        true
    }

    /// A key's handler: none taken while another has the key.
    // D2-16032
    pub fn link(&mut self, key: u32, handler: u32) -> bool {
        if key == 0 {
            return false;
        }
        match self.handler(key) {
            Some(h) if h != handler => false,
            Some(_) => true,
            None => {
                self.handlers.push((key, handler));
                true
            }
        }
    }

    /// A section's monitor (none taken while another has the section), and the section listed for
    /// it.
    // D2-16044
    pub fn attach(&mut self, section: u32, monitor: u32, out: &mut Vec<Notice>) -> bool {
        match self.monitor(section) {
            Some(m) if m != monitor => return false,
            Some(_) => {}
            None => self.monitors.push((section, monitor)),
        }
        self.reload(section, out);
        true
    }

    /// A section's monitor gone.
    // D2-16030
    pub fn detach(&mut self, section: u32) -> bool {
        self.monitors.retain(|(s, _)| *s != section);
        true
    }

    /// A section's preferences, in the order put in.
    // D2-16025
    #[must_use]
    pub fn list(&self, section: u32) -> Vec<Pref> {
        self.prefs
            .iter()
            .filter(|p| p.description().identify().0 == section)
            .cloned()
            .collect()
    }

    /// The handlers, each once, in the order their keys were linked.
    // D2-16026
    #[must_use]
    pub fn list_handlers(&self) -> Vec<u32> {
        let mut out: Vec<u32> = Vec::new();
        for (_, h) in &self.handlers {
            if !out.contains(h) {
                out.push(*h);
            }
        }
        out
    }

    /// A section's preferences listed for its monitor.
    // D2-16036
    pub fn reload(&self, section: u32, out: &mut Vec<Notice>) {
        if let Some(monitor) = self.monitor(section) {
            out.push(Notice::Reload {
                monitor,
                section,
                prefs: self.list(section),
            });
        }
    }

    /// A preference changed: its key's handler told, then its section's monitor.
    // D2-16033
    pub fn change(&self, pref: &Pref, out: &mut Vec<Notice>) {
        let (section, key) = pref.description().identify();
        if let Some(h) = self.handler(key) {
            out.push(Notice::Change {
                party: Party::Handler(h),
                pref: pref.clone(),
            });
        }
        if let Some(m) = self.monitor(section) {
            out.push(Notice::Change {
                party: Party::Monitor(m),
                pref: pref.clone(),
            });
        }
    }

    /// Each handler handed a section's preferences of its own that `pick` keeps (each first
    /// treated by `then`), when it has any.
    fn hand_out(
        &mut self,
        section: u32,
        pick: impl Fn(&Pref) -> bool,
        then: impl Fn(&mut Pref),
        out: &mut Vec<Notice>,
    ) {
        for handler in self.list_handlers() {
            let mut prefs = Vec::new();
            for i in 0..self.prefs.len() {
                let (s, k) = self.prefs[i].description().identify();
                if s != section || !pick(&self.prefs[i]) || self.handler(k) != Some(handler) {
                    continue;
                }
                then(&mut self.prefs[i]);
                prefs.push(self.prefs[i].clone());
            }
            if !prefs.is_empty() {
                out.push(Notice::Handle { handler, prefs });
            }
        }
    }

    /// A section's changed preferences handed to their handlers (not yet marked applied).
    // D2-16034
    pub fn submit(&mut self, section: u32, out: &mut Vec<Notice>) {
        self.hand_out(section, Pref::changed, |_| {}, out);
    }

    /// A section's changed preferences marked applied and handed to their handlers.
    // D2-16038
    pub fn apply(&mut self, section: u32, out: &mut Vec<Notice>) {
        self.hand_out(section, Pref::changed, Pref::apply, out);
    }

    /// One preference's key: the bank's marked applied and handed to its handler alone.
    // D2-16037
    pub fn apply_one(&mut self, pref: &Pref, out: &mut Vec<Notice>) {
        let key = pref.description().identify().1;
        let Some(i) = self.at(key) else {
            return;
        };
        self.prefs[i].apply();
        if let Some(handler) = self.handler(key) {
            out.push(Notice::Handle {
                handler,
                prefs: vec![self.prefs[i].clone()],
            });
        }
    }

    /// A section's preferences put back to the values last applied and handed to their handlers.
    // D2-16035
    pub fn recall(&mut self, section: u32, out: &mut Vec<Notice>) {
        self.hand_out(section, |_| true, Pref::recall, out);
    }

    /// A section's preferences taken as applied as they are, no one told.
    // D2-16022
    pub fn synchronize(&mut self, section: u32) {
        for p in &mut self.prefs {
            if p.description().identify().0 == section {
                p.apply();
            }
        }
    }

    /// Every preference written to the file.
    // D2-16020
    pub fn save(&self, file: &mut dyn PreferenceFile, names: &dyn Enumerations) -> bool {
        for p in &self.prefs {
            save(p, file, names);
        }
        true
    }
}
