//! Test support for the client: the behaviour registry and the harness a behaviour scenario is
//! written in.
//!
//! **Depends on** the products it tests (`dereth-client`, `dereth-headless`) and the crates a
//! scenario reaches through them (`dereth-client-runtime`, `dereth-client-model`,
//! `dereth-client-contract`, `dereth-client-net`, `dereth-input`, `dereth-ui`, `dereth-ui-screens`,
//! `dereth-primitives`, `dereth-dat`, `dereth-physics`, `dereth-transport`, `dereth-protocol`).
//! **Used by** nothing: it is a workspace member only so its tests are built and run.
//!
//! **Must never** be depended on from any crate's `src/` or from `tools/`, and it holds no oracle
//! of its own: every fact a scenario asserts over comes from the recorded corpus, the shipped data
//! or the client's own production code.
//!
//! | piece | what it is |
//! |---|---|
//! | [`HeadlessClient`] | the client under test, set up in one call |
//! | [`behaviours`] | the registry: each claim a public sentence with an id and an evidence handle |
//! | [`divergences`] | where this client deliberately differs from retail, published as `dereth/DIVERGENCES.md` |
//! | [`golden`] | one output file per run instead of a dozen counters |
//! | [`Inbound::from_corpus`] | messages from the recorded corpus, whose bytes are proved once |
//! | [`input_steps`] | the action and input driver scenarios share |
//!
//! ```no_run
//! use dereth_testkit::{HeadlessClient, Given, Inbound, Player};
//! use dereth_primitives::ObjectId;
//!
//! let mut c = HeadlessClient::model();
//! c.given(Given::APlayer(ObjectId(0x5000_0001)))
//!     .when(Player::DoubleClick(ObjectId(0x5000_0002)))
//!     .assert_behaviour("use.progress-notice.names-the-object", |v| {
//!         v.notice_text() == ["Using the Chest"]
//!     });
//! ```
//!
//! `given` sets the client up, `when` does something to it, `tick` runs frames, and the closure
//! handed to `assert_behaviour` reads a [`ScenarioView`], which can see everything and write
//! nothing. The id names a row in [`behaviours::SUBJECTS`]; [`behaviours::census`] compares what
//! scenarios declare with the registry to say which documented claims nothing tests. The
//! `AC-EVID-*` handles in the registry are evidence identifiers resolved in a separate research
//! archive.
//!
//! `tests/cpu` opens no retail data file; `tests/dat` opens the data under `$DERETH_TEST_DAT_DIR` and
//! **must run serially**, because two headless clients in one process share the UI request globals.

pub mod adapters_chat;
pub mod adapters_inventory;
pub mod adapters_shell;
pub mod adapters_social;
pub mod behaviours;
pub mod client;
pub mod corpus_steps;
pub mod divergences;
pub mod golden;
pub mod inbound;
pub mod input_steps;
mod login;
pub mod outbound;
pub mod player;
pub mod replay;
mod scenarios;
pub mod tier_census;
pub mod ui_snapshot;
pub mod view;
pub mod wire;

pub use behaviours::{census, Behaviour, Census, Evidence, Subject, Tier, SUBJECTS};
pub use client::{Assets, ClientSpec, Given, HeadlessClient, ScratchSettings, Step};
pub use inbound::Inbound;
pub use outbound::Outbound;
pub use player::{Direction, Player, ScreenPoint, Target};
pub use replay::Peer;
pub use ui_snapshot::UiSnapshot;
pub use view::ScenarioView;
pub use wire::Wire;
