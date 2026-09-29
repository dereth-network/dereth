//! GPU-tier tests for `dereth-client`, including DAT and CPU controls beside the device fixtures.
//! `--features retail-dats,vulkan` enables the default Vulkan backend and the tier switch;
//! renderer-gated modules also support the optional Windows D3D12 backend.
//!
//! One test binary per crate per tier: run one area or module with
//! `--test gpu <area>::<module>::`.

mod common;

mod audio;
mod camera;
mod chat;
mod combat;
mod instruments;
mod inventory;
mod login;
mod magic;
mod movement;
mod net;
mod objects;
mod panels;
mod presentation;
mod rendering;
mod selection;
mod ui;
mod world;
