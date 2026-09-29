//! DAT-tier tests for `dereth-input`: each module reads the shipped action map and master input
//! maps from the retail dats under `$DERETH_TEST_DAT_DIR` (through `shipped`). Selected by
//! `--features retail-dats`; absent dats are a failure, never a skip.

mod key_rebinding;
mod shipped;
mod shipped_maps;
