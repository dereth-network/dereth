//! The classic interface: the game's screens as they were before the 2005 redesign, as a front end
//! on the shared runtime.
//!
//! The game underneath is the shared client's: the runtime's frame loop, network session, object
//! model and physics, and the scene that draws the world. What this crate adds is the interface
//! the game had before its 2005 redesign: the login and character screens, the creation wizard,
//! the side panels, the chat, the radar, the vitals and the combat bar, drawn from the early-2005
//! portal ([`art`]) and driven by the classic key maps. It reaches the game through the front-end
//! context alone: it reads the model and asks for what it wants done.
//!
//! **Depends on** the shared crates (`dereth-primitives`, `dereth-client-contract`,
//! `dereth-client-model`, `dereth-client-runtime`, `dereth-protocol`, `dereth-rules`,
//! `dereth-chargen`, `dereth-dat`, `dereth-assets`, `dereth-physics`, `dereth-animation`), the client's
//! `dereth-scene` (item icons decoded as the scene decodes them, the preview dressing rules),
//! `dereth-ui` (its clip regions), `dereth-text` (tags and string-table composition), the game's presentation rules
//! (`dereth-presentation`: the appraisal and character sheet text it shares with the other
//! interface) and `dereth-input` (the host's device events and the final key maps),
//! and on `dereth-classic-dat`. **Used by** the client shell, which runs it while it is the
//! interface chosen.
//!
//! **Must never** hold the application, send on the session or write the game model: it reads the
//! model through its context and asks for everything else, as the other interface does.
//!
//! The pieces: [`runtime`] is the front end the shell drives ([`runtime::ClassicUi`]);
//! [`desktop`] lays the panels out and routes input to them; [`panels`] holds every screen;
//! [`renderer`] turns them into the overlay; [`art`] reads the early-2005 portal, and [`composed`]
//! makes the images it lacks from its own pieces.
pub mod art;
pub mod composed;
pub mod control_host;
pub mod cursor;
pub mod default_keys;
pub mod desktop;
pub mod dialogs;
pub mod help;
pub mod int;
pub mod item_art;
pub mod keybindings;
pub mod keyboard_runtime;
pub mod keystore;
pub mod panels;
pub mod previews;
pub mod renderer;
pub mod resources;
pub mod runtime;
pub mod screens;
pub mod settings_host;
pub mod steering;
pub mod text_edit;
pub mod widgets;
pub mod world_overlay;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TextRun {
    pub text: String,
    pub color: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Screen {
    pub width: u32,
    pub height: u32,
    pub commands: Vec<Command>,
}

/// Clip arrays use exclusive right/bottom; text coordinates are the line's top left.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Command {
    Invert {
        rect: [i32; 4],
        #[serde(default)]
        clip: Option<[i32; 4]>,
    },
    ItemIcon {
        recipe: item_art::Recipe,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        #[serde(default)]
        clip: Option<[i32; 4]>,
    },
    Preview {
        index: usize,
    },
    IndexedImage {
        #[serde(default)]
        flip_x: bool,
        did: String,
        palette: Vec<[u8; 4]>,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        #[serde(default)]
        clip: Option<[i32; 4]>,
    },
    SpellIcon {
        #[serde(default)]
        transparent: bool,
        icon: u32,
        power: u32,
        bitfield: u32,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        #[serde(default)]
        clip: Option<[i32; 4]>,
    },
    RichTextBox {
        runs: Vec<TextRun>,
        rect: [i32; 4],
        font: String,
        align: TextAlign,
        wrap: bool,
        #[serde(default)]
        clip: Option<[i32; 4]>,
    },
    TextBox {
        text: String,
        rect: [i32; 4],
        font: String,
        color: u32,
        align: TextAlign,
        wrap: bool,
        #[serde(default)]
        clip: Option<[i32; 4]>,
    },
    Image {
        did: String,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        #[serde(default)]
        clip: Option<[i32; 4]>,
        #[serde(default)]
        color_key: Option<[u8; 3]>,
        #[serde(default)]
        key_bits: Option<[u8; 3]>,
        #[serde(default)]
        tile: bool,
    },
    Text {
        text: String,
        x: i32,
        y: i32,
        font: String,
        color: u32,
        #[serde(default)]
        clip: Option<[i32; 4]>,
    },
    Fill {
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        color: u32,
    },
}

/// What `dereth-classic --version` prints.
#[must_use]
pub fn version_text() -> String {
    format!(
        "dereth-classic {}\ncommit: {}\ntarget: {}\n",
        env!("CARGO_PKG_VERSION"),
        option_env!("DERETH_BUILD_COMMIT").unwrap_or("unknown"),
        option_env!("DERETH_BUILD_TARGET").unwrap_or("unknown"),
    )
}

mod clock;
