//! What the Dereth launcher knows, with no window, no renderer and no operating system in it.
//!
//! **Depends on** the transport crate (`dereth-transport`), for the server-tracker login that asks
//! a server whether it is up, the primitives crate (`dereth-primitives`), for the client's era
//! table, and the data-file crate (`dereth-dat`), for a dat's identity and iterations in either
//! layout. **Used by** the launcher's window, the Tauri app in `dereth/launcher` (a
//! workspace of its own), and the web client's front page (`dereth-web`).
//!
//! **Must never** reach for a window, a web view, a GPU or the network, or name a platform API:
//! it reads and writes files (a dat, an executable, its own state and folders), because a launcher
//! on any platform must, and nothing else.
//!
//! The launcher's job is to get a player from "a folder of files, or nothing at all" to "in the
//! world I chose, with the right client and the right dats", and to catch every mismatch it can
//! **before** the client connects. This crate is that job, as data and pure functions:
//!
//! | module | what it knows |
//! |---|---|
//! | [`world`] | a registry entry, read tolerantly: which clients a world accepts, which dats it expects |
//! | [`serverlist`] | the community's world list, and its day-old copy |
//! | [`known`] | the worlds that run a client of their own, by list id: logon, files, rules |
//! | [`worlds`] | the worlds a launcher shows, each with what it said of itself and what the player chose |
//! | [`eras`] | the eras and systems a world can play, and the player's choice for a world that does not say |
//! | [`dat`] | a dat's iteration number, read straight off disk in a few small reads |
//! | [`datset`] | a Modern set (the four later dats, identified by iterations) or a Classic one (`portal.dat` and `cell.dat`) |
//! | [`install`] | which client build a folder holds, by the executable's hash and a manifest |
//! | [`library`] | the one retail client, and the dat sets, as the player adds them |
//! | [`choices`] | which clients and dat sets a world may be played with |
//! | [`mod@check`] | the pre-launch check: what the world would refuse, and the fix for each |
//! | [`launch`] | the command line per client and server, a form of it that is safe to log, and what the client is told about the world |
//! | [`copy`] | a private dat set, copied from the shared one |
//! | [`state`] | `launcher-state.json`: the retail client, sets, accounts, favourites, per-world choices |
//! | [`folders`] | where the state and the private sets live on each system, and the one-time move there |
//! | [`releases`] | which of the repository's GitHub releases the launcher updates from |
//! | [`swap`] | putting a downloaded release in place of the running one, where no installer does |
//! | [`desktop`] | the Linux desktop entries that give the launcher's window and the client's their icon |
//! | [`vault`] | the seam to the operating system's secret store; passwords live nowhere else |
//! | [`registry`] | the signed snapshot, and which source is trusted for which fields |
//! | [`status`] | Empyrean's public status document |
//! | [`probe`] | whether any server is up, asked with the server-tracker login |
//!
//! The launcher's window is one front end over this. The web client's front page is another: it
//! builds for `wasm32-unknown-unknown` and reaches files only where a caller hands it a path, so a
//! page uses the same list, table, matching and command words through its own storage.

#![forbid(unsafe_code)]

pub mod check;
pub mod choices;
pub mod copy;
pub mod dat;
pub mod datset;
pub mod desktop;
pub mod eras;
pub mod folders;
pub mod install;
pub mod known;
pub mod launch;
pub mod library;
pub mod probe;
pub mod registry;
pub mod releases;
pub mod serverlist;
pub mod state;
pub mod status;
pub mod swap;
pub mod vault;
pub mod world;
pub mod worlds;

#[cfg(any(test, feature = "testing"))]
#[doc(hidden)]
pub mod testing;

pub use check::{check, CheckId, CheckInput, Finding, Fix, Report, RunningClient, Verdict};
pub use datset::{DatOrigin, DatRole, DatSet, Iterations, SetKind};
pub use install::{ClientKind, Installation};
pub use launch::{plan, LaunchPlan, LaunchRequest};
pub use state::{Account, Favourite, LauncherState, WorldPrefs};
pub use vault::Vault;
pub use world::{Emulator, World, WorldState};
