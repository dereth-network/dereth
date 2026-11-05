//! The keys: the key maps that bind each input (a key, held with Shift or Ctrl or not, a button) to
//! an action, and what an action does.
//!
//! The key maps are a stack, the last pushed in front. At startup the client pushes the base map
//! the game keeps for itself, then the player's maps for keys alone, with Shift and with Ctrl,
//! each read from the file the preferences name (`[KeyMap]`: `Main.keymap`, `Shift.keymap`,
//! `Control.keymap`) or, when that file does not load, the data's Defaults map. The plain map is
//! read with the version update that binds R to reply; a file it updates is written again.
//!
//! An input is looked up from the front: the first map that binds it, and whose modifier is held
//! (a Shift map's while Shift is held, a plain map's always), names its action. The key map
//! screen's Apply writes the three files and pushes them again ([`Keys::apply`]).
//!
//! What an action does is the action map's: how it toggles (held, latched, a single press), and by
//! the modifiers held its class (movement, a behavior, the camera, a modifier, the interface), its
//! value and its extent.

use auberean_assets::keymap::{ReadKeyMap, VersionUpdate, NORMAL_UPDATES};
use auberean_assets::strings::EnumMap;
use auberean_assets::ui::{ActionBindings, ActionValues, KeyMap};
use auberean_dat::tree;
use auberean_world::records::WorldRecords;

/// The mapping the key maps are found in, and the base map's and the Defaults maps' values in it.
pub const KEY_MAPS: u32 = 0x4000_000F;
pub const BASE_MAP: u32 = 0x4000_0001;
pub const DEFAULT_NORMAL: u32 = 0x4100_0001;
pub const DEFAULT_CONTROL: u32 = 0x4100_0002;
pub const DEFAULT_SHIFT: u32 = 0x4100_0003;
/// The input actions enumeration (its enum map's value in the enum maps' mapping).
pub const INPUT_ACTIONS: u32 = 0x4000_001D;
const ENUM_MAPS: u32 = 0x4000_0001;
/// The action map.
pub const ACTION_MAP: u32 = 0x5100_0000;

/// The input words of the keys that are modifiers.
pub const LEFT_SHIFT: u32 = 0x8100_042A;
pub const RIGHT_SHIFT: u32 = 0x8100_0436;
pub const LEFT_CTRL: u32 = 0x8100_041D;
pub const RIGHT_CTRL: u32 = 0x8100_049D;

/// The modifier bits a key map is pushed with, and a held modifier key adds: Shift 1, Ctrl 4.
pub const META_SHIFT: u32 = 1;
pub const META_CTRL: u32 = 4;

/// The names the saved maps are given.
pub const SAVED_NAMES: [&str; 3] = ["mydefault", "mydefaults", "mydefaultc"];

/// Whether an input is a modifier key.
// D2-04913
#[must_use]
pub const fn is_meta_key(input: u32) -> bool {
    matches!(input, LEFT_SHIFT | RIGHT_SHIFT | LEFT_CTRL | RIGHT_CTRL)
}

/// The modifier bits of a modifier key, the right keys folded onto the left first (so only Shift 1
/// and Ctrl 4 occur).
// D2-04914
#[must_use]
pub const fn meta_mode(input: u32) -> u32 {
    match input {
        LEFT_SHIFT | RIGHT_SHIFT => META_SHIFT,
        LEFT_CTRL | RIGHT_CTRL => META_CTRL,
        _ => 0,
    }
}

/// One map of the stack: its bindings, and the modifier bits it is pushed for.
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub map: KeyMap,
    pub meta: u32,
}

/// The stack of key maps, the front last.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KeyStack {
    layers: Vec<Layer>,
}

impl KeyStack {
    /// Push a map in front, for the modifier key `meta_key` (0 for none).
    // D2-04963, D2-04857
    pub fn push(&mut self, map: KeyMap, meta_key: u32) {
        self.layers.push(Layer {
            map,
            meta: meta_mode(meta_key),
        });
    }

    /// Take the front map off; none when the stack is empty.
    // D2-04953
    pub fn pop(&mut self) -> Option<Layer> {
        self.layers.pop()
    }

    /// The maps from the bottom (the first pushed) to the front.
    #[must_use]
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    /// The action an input names with these modifier bits held: from the front, the first map
    /// that binds the input and whose own modifier bits are all held.
    // D2-04943
    #[must_use]
    pub fn action(&self, input: u32, held: u32) -> Option<u32> {
        self.find(input, held).map(|(_, a)| a)
    }

    /// As [`Self::action`], with the place of the map that names it (0 the base map).
    #[must_use]
    pub fn find(&self, input: u32, held: u32) -> Option<(usize, u32)> {
        self.layers.iter().enumerate().rev().find_map(|(i, l)| {
            if held & l.meta != l.meta {
                return None;
            }
            l.map
                .bindings
                .iter()
                .find(|b| b.input == input)
                .map(|b| (i, b.action))
        })
    }
}

/// The data the keys need: the base map and the Defaults maps the files fall back to, the input
/// actions' names and the action map.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KeyRecords {
    pub base: KeyMap,
    /// The Defaults maps: keys alone, with Shift, with Ctrl.
    pub defaults: [KeyMap; 3],
    /// The input actions' enum map and its base maps, the map itself first.
    pub actions: Vec<EnumMap>,
    pub action_map: ActionBindings,
}

impl KeyRecords {
    /// Read from the data.
    #[must_use]
    pub fn read(records: &WorldRecords) -> Self {
        let map = |e: u32| -> KeyMap {
            records
                .get::<KeyMap>(records.data_id_by_enum(e, KEY_MAPS))
                .unwrap_or_default()
        };
        let mut actions = Vec::new();
        let mut id = records.data_id_by_enum(INPUT_ACTIONS, ENUM_MAPS);
        for _ in 0..8 {
            let Some(m) = records.get::<EnumMap>(id) else {
                break;
            };
            id = m.base;
            actions.push(m);
            if id == 0 {
                break;
            }
        }
        Self {
            base: map(BASE_MAP),
            defaults: [
                map(DEFAULT_NORMAL),
                map(DEFAULT_SHIFT),
                map(DEFAULT_CONTROL),
            ],
            actions,
            action_map: records
                .get::<ActionBindings>(ACTION_MAP)
                .unwrap_or_default(),
        }
    }

    /// The action a name names in the input actions enumeration (letter case ignored; the map
    /// itself before its bases).
    #[must_use]
    pub fn action_of(&self, name: &str) -> Option<u32> {
        if self.actions.is_empty() {
            return None;
        }
        self.actions.iter().find_map(|m| m.enum_of(name))
    }

    /// An action's name in the input actions enumeration.
    #[must_use]
    pub fn action_name(&self, action: u32) -> Option<&str> {
        self.actions.iter().find_map(|m| m.name(action))
    }

    /// What an action does with these modifier bits held: the entry for exactly them, else
    /// without the bit 8, else with none.
    // D2-03449
    #[must_use]
    pub fn values(&self, action: u32, modifiers: u32) -> Option<ActionValues> {
        let m = &self.action_map;
        m.action_values(action, modifiers)
            .or_else(|| {
                (modifiers & 8 != 0)
                    .then(|| m.action_values(action, modifiers & !8))
                    .flatten()
            })
            .or_else(|| m.action_values(action, 0))
    }
}

/// The files the key maps are kept in, as the preferences name them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyFiles {
    pub normal: String,
    pub shift: String,
    pub control: String,
}

impl Default for KeyFiles {
    fn default() -> Self {
        Self {
            normal: "Main.keymap".into(),
            shift: "Shift.keymap".into(),
            control: "Control.keymap".into(),
        }
    }
}

/// Where the key maps' files are read from and written to.
pub trait KeyFileStore {
    /// A file's bytes, if it is there.
    fn load(&self, name: &str) -> Option<Vec<u8>>;
    /// Write a file; whether it was written.
    fn save(&mut self, name: &str, bytes: &[u8]) -> bool;
}

/// The keys: the data, the stack, and the files the maps are kept in.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Keys {
    pub records: KeyRecords,
    pub stack: KeyStack,
    pub files: KeyFiles,
}

impl Keys {
    /// A key map read from its file: parsed, read with `updates`, and when an update added a
    /// binding, written back. None when the file is missing or does not read.
    // D2-04965
    fn read_file(
        &self,
        store: &mut dyn KeyFileStore,
        name: &str,
        updates: &[VersionUpdate],
    ) -> Option<KeyMap> {
        let bytes = store.load(name)?;
        let mut root = tree::parse_bytes(name.as_bytes(), &bytes)?;
        let action_of = |n: &str| self.records.action_of(n);
        let ReadKeyMap { map, updated, .. } = KeyMap::from_tree(&mut root, &action_of, updates)?;
        if updated {
            let mut t = map.to_tree();
            store.save(name, &tree::to_file_bytes(&mut t));
        }
        Some(map)
    }

    /// The keys at startup: the base map, then the plain, Shift and Ctrl maps from their files,
    /// each the data's Defaults map when its file does not load.
    // D2-01694, D2-04942
    #[must_use]
    pub fn start(records: KeyRecords, files: KeyFiles, store: &mut dyn KeyFileStore) -> Self {
        let mut keys = Self {
            records,
            stack: KeyStack::default(),
            files,
        };
        keys.stack.push(keys.records.base.clone(), 0);
        keys.push_files(store, &NORMAL_UPDATES, true);
        keys
    }

    /// The three player's maps pushed from their files (the plain one with `updates`), each the
    /// Defaults map when its file does not load (`fall_back`).
    fn push_files(
        &mut self,
        store: &mut dyn KeyFileStore,
        updates: &[VersionUpdate],
        fall_back: bool,
    ) {
        let names = [
            self.files.normal.clone(),
            self.files.shift.clone(),
            self.files.control.clone(),
        ];
        let metas = [0, LEFT_SHIFT, LEFT_CTRL];
        for (i, name) in names.iter().enumerate() {
            let u = if i == 0 { updates } else { &[] };
            match self.read_file(store, name, u) {
                Some(m) => self.stack.push(m, metas[i]),
                None if fall_back => self.stack.push(self.records.defaults[i].clone(), metas[i]),
                None => {}
            }
        }
    }

    /// The player's maps as the key map screen applies them (each a list of an input and its
    /// action, keys alone, with Shift, with Ctrl): written to their files as `mydefault`,
    /// `mydefaults` and `mydefaultc`, each binding named by its action's name; when every file was
    /// written, the three front maps taken off and the files pushed again. Whether they were.
    // D2-01768
    pub fn apply(&mut self, maps: [&[(u32, u32)]; 3], store: &mut dyn KeyFileStore) -> bool {
        let names = [
            self.files.normal.clone(),
            self.files.shift.clone(),
            self.files.control.clone(),
        ];
        let mut ok = true;
        for ((bindings, saved), file) in maps.iter().zip(SAVED_NAMES).zip(&names) {
            let mut m = KeyMap {
                name: saved.into(),
                ..KeyMap::default()
            };
            for &(input, action) in *bindings {
                let name = self
                    .records
                    .action_name(action)
                    .unwrap_or_default()
                    .to_owned();
                m.add_action(action, input, &name);
            }
            let mut t = m.to_tree();
            ok &= store.save(file, &tree::to_file_bytes(&mut t));
        }
        if ok {
            for _ in 0..3 {
                self.stack.pop();
            }
            self.push_files(store, &[], false);
        }
        ok
    }

    /// An interface action a key names with these modifier bits held in the player's maps (not
    /// the base map): the action's value, which the interface hears.
    #[must_use]
    pub fn interface_action(&self, input: u32, held: u32) -> Option<u32> {
        let (layer, action) = self.stack.find(input, held)?;
        let v = self.records.values(action, 0)?;
        (layer > 0 && v.class == crate::controls::class::INTERFACE).then_some(v.value)
    }

    /// The player's three maps as the stack holds them now (the front three over the base), keys
    /// alone, with Shift, with Ctrl; empty where the stack has fewer.
    #[must_use]
    pub fn player_maps(&self) -> [&[auberean_assets::ui::KeyBinding]; 3] {
        let l = self.stack.layers();
        let at = |i: usize| l.get(i).map_or(&[][..], |x| x.map.bindings.as_slice());
        [at(1), at(2), at(3)]
    }
}
