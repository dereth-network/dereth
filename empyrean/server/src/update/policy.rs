//! Which release, if any, the server installs: the shared decision
//! ([`dereth_server_update::policy`]) over Empyrean's own facts and declarations.

use super::release::{Declaration, Facts};

pub use dereth_server_update::policy::{needs, select, Needs, Policy, Selection};

/// A newer release, as the updater found it.
pub type Candidate = dereth_server_update::policy::Candidate<Declaration>;

/// What the selection needs to know about this server beyond the releases.
pub type Situation<'a> = dereth_server_update::policy::Situation<'a, Facts>;
