//! The remaster's claims that need a graphics device, on the hardware GPU: at the stations the
//! retail comparison measures, the remaster draws the frame the retail device draws, within the
//! comparison's tolerance, on a device of its own with the validation layer quiet.
//!
//! Fixture: a hardware adapter and the end-of-retail install under `$AUBEREAN_TEST_DAT_DIR`. These
//! tests run with `--features retail-dats`; one test at a time has a device.

mod common;
mod identity;
