//! Tests that decode the retail data files, found through `auberean_dat::testing`. The checked set
//! (the end of retail) is opened once per test binary ([`common::installs`]); a test fails, naming
//! the variable to set, when it is not found.

mod common;
mod game;
mod look;
mod world;
