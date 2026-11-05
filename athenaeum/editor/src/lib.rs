//! Athenaeum, the Asheron's Call 2 world builder: a world's rows drawn where they stand in the
//! world as the client draws it, selected, placed, moved, turned, changed and removed, saved into
//! the world's pack, and played on Yalain.
//!
//! The editor's state and panels ([`app`]), the panels' toolkit and painter ([`gui`], [`paint`]),
//! the world it shows ([`scene`]) and where things are on the screen ([`view`]), playing the world
//! on the editor's own Yalain ([`live`]) as its playtest characters ([`characters`]), starting the
//! programs it runs without console windows and stopping the server as its operator does
//! ([`stop`]), and scripts of editor steps ([`script`]), shared by the program (`main.rs`, which
//! brings the window and the event loop) and its tests.
//!
//! **Depends on** the world machinery (`athenaeum-foundry`), the world database and the server's
//! tables (`yalain-world-db`, `yalain-content`), the pack format (`dereth-pack`), the world as the
//! client draws it (`auberean-world`, `auberean-assets`, `auberean-dat`, `auberean-animation`,
//! `auberean-physics`, `auberean-primitives`, `auberean-draw`, `auberean-gpu`), the free camera
//! (`dereth-builder`), the shared math and collision (`dereth-primitives`, `dereth-physics`), the panels (`egui`), and the window
//! system (`winit`) and device (`wgpu`, `pollster`, `bytemuck`, `png`), with `toml` for the
//! playtest characters' recipes, `serde_json` for the character files the server exports and
//! `auberean-protocol` for the qualities a file of the first version holds, and `tracing` and
//! `tracing-subscriber`. **Used by** nothing but its program and its tests.
//!
//! **Must never** write a base database or an install, or use the server as a crate: it writes
//! only the world's pack, its recipes, its build folder and its playtest folder, and starts Yalain
//! as a program.

pub mod app;
pub mod characters;
pub mod gizmo;
pub mod gui;
pub mod icon;
pub mod live;
pub mod map;
pub mod paint;
pub mod scene;
pub mod script;
pub mod stop;
pub mod view;
