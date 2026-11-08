//! Instruments of the whole world on this executable (feature `soak`): each measures and writes
//! out what it measured, is ignored in every run that does not ask for it by name, and asserts only
//! that the world ran.

mod soak;
