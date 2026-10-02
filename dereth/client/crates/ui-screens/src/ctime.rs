//! The two CRT time formatters the retail client links against, and the zone shift they run in:
//! the contract's (`dereth_client_contract::ctime`), because the disconnect text any UI shows is
//! formatted with them. Re-exported here at the old path.

pub use dereth_client_contract::ctime::*;
