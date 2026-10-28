//! Lyceum, the world builder: a world's packs over the player's data files, drawn as the client draws them,
//! edited in place and saved as the packs' sources.
//!
//! **Depends on** the Foundry's editing logic (`lyceum-foundry`: the edit session, its operations,
//! undo, saves, brushes and picks), the free camera the world builders share (`dereth-builder`),
//! the client's crates for everything it shows (the runtime's
//! world and physics, `dereth-client-runtime`; the overlay and the projection's parameters,
//! `dereth-client-contract`; the player's options it sets, `dereth-client-model`; the drawn world,
//! `dereth-scene`; the device, `dereth-render`; the renderer that is the runtime's presentation,
//! `dereth-client-shell`), the desktop host for its window, log and crash log (`dereth-desktop`),
//! the terminal it borrows and the system's error box (`dereth-console`),
//! the device input's events (`dereth-input`), ids, frames and textures (`dereth-primitives`),
//! the client's physics the flying camera collides through (`dereth-physics`) and the ground's
//! mesh its scenery stands on (`dereth-terrain`), a playtest character's build and checks
//! (`dereth-progression`), a weenie's look as a server describes it (`dereth-rules`) and as the
//! client applies it to the parts (`dereth-animation`),
//! its panels (`egui`, docked with `egui_dock`, with `egui_extras`' tables), and for the model
//! browser's pictures the files a model is read from (`dereth-dat`), its parts
//! (`dereth-world-data`), the preview's light (`dereth-world-render`) and the cached pictures'
//! format (`png`), and for its names the region record (`dereth-assets`) and the world map's
//! places (`dereth-ui-screens`). **Used by** nothing: it is an application.
//!
//! **Must never** change what the client does: it is a separate application that builds on the
//! client's crates, and the client brings up none of this.
//!
//! The editor runs the runtime's application with a front end of its own ([`shell`]): no game
//! interface, only the editor's panels and the world. Each edit is an operation of the edit
//! session; the session compiles the landblocks it reaches into a scratch overlay, the store is
//! laid over it again, and the scene builds those landblocks again ([`app`]). It runs in a window,
//! or off screen with a script of edits and pointer events ([`script`]), capturing frames.
//!
//! | Module | What it owns |
//! |---|---|
//! | [`args`] | the command line |
//! | [`catalog`] | what the model browser knows of each model, gathered off the frame loop |
//! | [`camera`] | the free camera, its keys, its speeds and the snap to the ground |
//! | [`docks`] | the docked tabs and their layout, outdoors and in dungeon mode, kept between starts |
//! | [`view`] | the projection: world to screen, and a pixel's ray |
//! | [`gizmo`] | the move and turn handles: where they are drawn and what a drag on one does |
//! | [`gui`] | the panels' toolkit: egui, fed the window's events and drawn through the overlay |
//! | [`live`] | the editor's own server on the loopback: started, and each change pushed to it |
//! | [`look`] | a weenie's look, read from its SQL and the data files, for the editor's preview |
//! | [`programs`] | where the editor finds its bundled server programs |
//! | [`script`] | the scripted mode's steps |
//! | [`shell`] | the front end the runtime calls: the window's events and the overlay |
//! | [`text`] | every word the editor shows, from the creator's `lyceum-text.toml` |
//! | `app` | the editor: the session, the tools, the reload, the frame, and its panels (with a device) |
//! | `thumbs` | the model browser's pictures, drawn off screen and cached (with a device) |

pub mod args;
pub mod browser_state;
pub mod camera;
pub mod catalog;
pub mod character_extras;
pub mod characters;
pub mod database;
pub mod docks;
pub mod gizmo;
pub mod gui;
pub mod kits;
pub mod live;
pub mod look;
pub mod place;
pub mod programs;
pub mod script;
pub mod shell;
pub mod text;
pub mod view;

#[cfg(gpu)]
pub mod app;
#[cfg(gpu)]
pub mod thumbs;

/// Lyceum, as a product on the desktop host.
#[derive(Debug, Clone, Copy, Default)]
pub struct Lyceum;

impl dereth_desktop::Product for Lyceum {
    const BINARY_NAME: &'static str = "lyceum-editor";
    const BUILD_ID: &'static str = concat!("lyceum-editor ", env!("CARGO_PKG_VERSION"));
    const SETTINGS_DIR_NAME: &'static str = "lyceum";
    const TITLE: &'static str = "Lyceum";
    const ICON_PNG: &'static [u8] = include_bytes!("../assets/lyceum-256.png");
    const APP_ID: &'static str = "network.dereth.lyceum";
    // Resized freely: the docked panels and the world view take the window's new size.
    const RESIZABLE: bool = true;
}
