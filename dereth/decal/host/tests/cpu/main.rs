//! The Decal host through its C interface, as the Decal plugin drives it: the recorded sessions'
//! messages in, ops out.
//!
//! - [`model`]: a plugin hears the same session in this host as in Dereth's own driver;
//! - [`actions`]: plugins' actions become the Decal calls that send the same requests, or fail at
//!   once where Decal has none;
//! - [`bundled`]: the plugins that come with Dereth run in this host;
//! - [`conformance`]: the spec's cases this host can run, through the interface alone;
//! - [`folders`]: the plugins the launcher installed load beside the plugins folders;
//! - [`keys`]: with the player's leave, plugins' attacks, jumps and held movement as the game's own
//!   keys;
//! - [`interface`]: the interface's own contract (settings, claimed lines, the queue a message is
//!   read on).

mod abi;
mod actions;
mod bundled;
mod conformance;
mod folders;
mod interface;
mod keys;
mod model;
