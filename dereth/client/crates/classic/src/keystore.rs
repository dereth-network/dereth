//! The classic interface's key map: its own file, over its own default scheme.
//!
//! **One set of keys, a map per interface.** Both interfaces bind the same actions, in the
//! shared action vocabulary and the shared `.keymap` format. Each keeps its own map: the retail
//! interface its key map file over the final client's shipped maps, the classic interface its
//! own file over the 2004 default scheme ([`crate::default_keys`]). A key bound in one does not
//! change the other. Each interface's saved key maps are its own: `<name>-classic.keymap` for
//! this one, `<name>-modern.keymap` for the other, and each page lists only its own.
//!
//! The host keeps the file. It hands the classic interface [`ClassicKeys`], the map as it is, and
//! carries out the [`KeyStoreRequest`]s the Keyboard Configuration page raises.

pub use crate::default_keys::{meta_of_modifiers, modifiers_of_meta, scan_code, ALT, CTRL, SHIFT};

/// One key of the classic map: a keyboard key (its scan code, bit 7 for the extended prefix) held
/// with `modifiers`, bound to `action` in the input map `map`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassicBinding {
    pub scan: u16,
    pub modifiers: u8,
    pub action: u32,
    pub map: u32,
}

/// What the host's classic key map holds for the interface.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClassicKeys {
    /// Every keyboard key the map binds, the cleared ones left out.
    pub bindings: Vec<ClassicBinding>,
    /// This interface's saved key maps in the folder, by the name the player gave each (the file
    /// is `<name>-classic.keymap`), sorted, the one in use left out.
    pub files: Vec<String>,
    /// The name of the key map in use, which the page neither overwrites nor deletes.
    pub active: String,
    /// For each input map a key is bound in, the maps a key there takes away from (its own
    /// among them).
    pub conflicts: Vec<(u32, Vec<u32>)>,
    /// The `(map, action)`s held for as long as their key is: they end when it is let go.
    pub holds: Vec<(u32, u32)>,
}

/// A scheme the page can load into the classic map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scheme {
    /// This interface's default scheme, the 2004 table.
    Default,
    /// One of this interface's saved key maps, by the name the player gave it.
    File(String),
}

/// What the Keyboard Configuration page asks of the classic map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyStoreRequest {
    /// Bind the key `scan` with `modifiers` to `action` in `map`, in place of whatever the key
    /// did there and in the maps that conflict with it; `replaced`, when set, is a key `action`
    /// no longer has.
    Bind {
        scan: u16,
        modifiers: u8,
        action: u32,
        map: u32,
        replaced: Option<(u16, u8)>,
    },
    /// `action` no longer has the key `scan` with `modifiers` in `map`. The key stays cleared:
    /// the map keeps it bound to nothing.
    Clear {
        scan: u16,
        modifiers: u8,
        action: u32,
        map: u32,
    },
    /// Save the classic map as `name` (the file `<name>-classic.keymap`), replacing one of that
    /// name when `overwrite`.
    SaveAs { name: String, overwrite: bool },
    /// Make the classic map exactly the scheme.
    Load(Scheme),
    /// Delete the saved key map `name`.
    Delete(String),
}
