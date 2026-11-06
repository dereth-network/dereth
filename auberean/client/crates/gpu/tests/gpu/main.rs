//! The device's claims that need a graphics device, on the hardware GPU: every pipeline it makes
//! builds, frames with every kind of draw go through it, materials show as they should, frames
//! seen from inside draw what the doorways show, a body's volume of shadow darkens the ground,
//! a swung streak leaves a ribbon of light, the water shows half the ground under it and half its
//! reflection, and its ripples push the reflection one way, the shores write the frame's alpha
//! and the foam lies over the reflection, a viewport's scene covers only where it draws, and
//! textures are taken at their detail and filtered as the graphics preferences say, and a faded
//! object shows what is behind it.
//!
//! Fixture: a hardware adapter (a software rasteriser is refused) and the end-of-retail install
//! under `$AUBEREAN_TEST_DAT_DIR`. These tests run with `--features retail-dats`, on the binary's
//! threads; one test at a time has a device ([`common::renderer`]).

mod common;
mod fade;
mod inside;
mod lost;
mod materials;
mod pipelines;
mod shadows;
mod shores;
mod streaks;
mod textures;
mod uniforms;
mod water;
