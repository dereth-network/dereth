//! The one device lock of the `gpu` binary.
//!
//! Every module of the binary takes this one lock before it opens a device, so no two tests hold
//! a device at once however many test threads libtest runs. Separate per-module locks would be
//! separate locks in one binary, and many modules asking for a device at once can take the
//! adapter down with an access violation inside the driver. The tests are independent, so
//! serialising them costs only wall time.
//!
//! **Poisoning is ignored on purpose**: a panic in one test must
//! still let the others run and report their own failures, and the guarded data is `()`, so there
//! is nothing a panicking holder could have left half-written.
//!
//! `-- --test-threads=1` is still the documented way to run this binary (`README.md`, and
//! `cargo xtask gate` now passes it for the two crates that have a `gpu` binary). This lock is
//! what makes a *parallel* run correct rather than lucky; the flag is what keeps a run's wall
//! clock and its output readable.

use std::sync::{Mutex, MutexGuard};

/// Take the binary's device lock. One `App`/`Gpu` at a time, for the whole `gpu` binary.
pub fn gpu_lock() -> MutexGuard<'static, ()> {
    static GPU: Mutex<()> = Mutex::new(());
    GPU.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
