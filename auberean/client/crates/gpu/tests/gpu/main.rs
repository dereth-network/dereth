//! The device's claims that need a graphics device, on the hardware GPU: every pipeline it makes
//! builds, frames with every kind of draw go through it, materials show as they should, frames
//! seen from inside draw what the doorways show, and a body's volume of shadow darkens the ground.
//!
//! Fixture: a hardware adapter (a software rasteriser is refused) and the end-of-retail install
//! under `$AUBEREAN_TEST_DAT_DIR`. These tests run with `--features retail-dats`, one at a time:
//! they share the machine's device.

mod common;
mod inside;
mod materials;
mod pipelines;
mod shadows;
