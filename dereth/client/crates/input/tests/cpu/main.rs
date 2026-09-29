//! CPU-tier tests for `dereth-input`: no retail dats, no GPU device.
//!
//! Every integration test of this crate reads the shipped input maps, so they are all in the `dat`
//! tier and this binary declares no module. It exists so that `cargo test -p dereth-input --test
//! cpu` is the same command in every crate.
