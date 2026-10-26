//! The plugin interface as types: what a plugin package declares, what it may do, what it hears
//! and what it shows, with no engine, no game and no I/O in it.
//!
//! **Depends on** `dereth-primitives` alone. **Used by** the plugin host (`dereth-plugin-host`),
//! the script engine (`dereth-plugin-lua`), the plugin window renderer (`dereth-plugin-ui`) and the
//! products that load plugins.
//!
//! **Must never** depend on another crate of this workspace, or on anything that does I/O, draws or
//! reaches a platform (`cargo xtask seams`, `seam: plugin-api deps`). The spec in
//! `docs/plugins/` is written against these types: a change here is a change to the spec.
//!
//! [`manifest`] is `plugin.toml` as data, [`permission`] and [`capability`] what a plugin may do
//! and what a host offers, [`policy`] what a world allows, [`event`] what a plugin hears, [`ui`]
//! the declared UI tree and [`layout`] how every host lays it out, [`value`] the plain values
//! that cross between a plugin and its host (and the JSON a store keeps them in), [`engine`] the
//! boundary a script engine implements, [`audit`] the answer to a server's plugin audit and
//! [`version`] the API version rule.

#![forbid(unsafe_code)]

pub mod audit;
pub mod capability;
pub mod draw;
pub mod engine;
pub mod event;
pub mod layout;
pub mod manifest;
pub mod permission;
pub mod policy;
pub mod schema;
pub mod ui;
pub mod value;
pub mod version;

pub use capability::HostCapability;
pub use engine::{
    CallError, HostCalls, Limits, ScriptEngine, ScriptError, ScriptErrorKind, ScriptInstance,
};
pub use event::Event;
pub use manifest::{Context, Manifest, ManifestError, PluginId};
pub use permission::{AutoDomain, Permission};
pub use policy::{DomainRule, WorldPolicy};
pub use ui::{NodeKind, UiError, UiNode};
pub use value::Value;
pub use version::ApiVersion;
