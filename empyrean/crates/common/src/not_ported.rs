//! The honest stub. Any ACE behaviour not yet ported calls [`not_ported!`](crate::not_ported!)
//! instead of a panicking placeholder macro or an empty body: it logs once per site, counts every hit (globally and on
//! the calling thread, so a test can assert its own path is fully ported) and returns, leaving the
//! caller to run ACE's base-class behaviour. The port ledger counts these sites as unported.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

static GLOBAL: OnceLock<Mutex<BTreeMap<&'static str, u64>>> = OnceLock::new();

thread_local! {
    static LOCAL: RefCell<BTreeMap<&'static str, u64>> = const { RefCell::new(BTreeMap::new()) };
}

/// Records one hit on the unported member `what` (for example `"ACE: Door.ActOnUse"`).
pub fn hit(what: &'static str) {
    LOCAL.with(|m| *m.borrow_mut().entry(what).or_insert(0) += 1);
    let mut global = GLOBAL
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let count = global.entry(what).or_insert(0);
    *count += 1;
    if *count == 1 {
        log::warn!("not ported: {what}");
    }
}

/// Hits recorded on the current thread since it started or since the last [`take_local`].
pub fn take_local() -> BTreeMap<&'static str, u64> {
    LOCAL.with(|m| std::mem::take(&mut *m.borrow_mut()))
}

/// Process-wide hit counts, for the status endpoint and the ledger's "wired" column.
pub fn global_snapshot() -> BTreeMap<&'static str, u64> {
    GLOBAL
        .get_or_init(Default::default)
        .lock()
        .map(|m| m.clone())
        .unwrap_or_default()
}

/// Marks an ACE member that has not been ported yet: `not_ported!("ACE: Door.ActOnUse")`.
#[macro_export]
macro_rules! not_ported {
    ($what:literal) => {
        $crate::not_ported::hit($what)
    };
}
