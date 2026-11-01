//! One dependency for whatever builds on the client: the runtime, the contract's actions and the
//! core they use.
//!
//! **Depends on** the runtime (`auberean-client-runtime`) and the crates whose types it hands out:
//! the actions (`auberean-client-contract`), the model (`auberean-client-model`), the session
//! (`auberean-client-net`), the messages (`auberean-protocol`), the data files (`auberean-dat`),
//! the script machine (`auberean-script`) and localized text (`auberean-text`). **Used by** the
//! headless client (`auberean-headless`) and every front end to come.
//!
//! **Must never** add behavior of its own: it hands out what the crates under it are, so a front end
//! depends on one crate and no more.

pub use auberean_client_contract as contract;
pub use auberean_client_model as model;
pub use auberean_client_net as net;
pub use auberean_client_runtime as runtime;
pub use auberean_dat as dat;
pub use auberean_protocol as protocol;
pub use auberean_script as script;
pub use auberean_text as text;

pub use auberean_client_contract::{Action, Motion, NewCharacter};
pub use auberean_client_runtime::{Client, ClientData, ClientSettings};
