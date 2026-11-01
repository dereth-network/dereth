//! Test support: the behaviour registry, and where the private test data is.
//!
//! **Depends on** `auberean-dat` (where the data files are), and with its feature the capture
//! index's database engine (`captures`). **Used by** the tests of every Asheron's Call 2 crate.
//!
//! - [`behaviours`]: every claim this workspace's tests make about the game, each a row with an
//!   id, a sentence, the build it was read off, an opaque evidence handle and the test that
//!   asserts it.
//! - [`divergences`]: the places this client deliberately differs from retail.
//! - [`data`]: where the private test data is, the two sets of retail data files (through
//!   `auberean_dat::testing`) and the capture index.
//! - `captures` (feature `captures`): the recorded sessions' messages, read from the capture
//!   index in session order.
//!
//! **Must never** be a dependency of a crate's own code: nothing in any crate's `src/` depends on
//! it; tests do (`cargo xtask seams`).

pub mod behaviours;
#[cfg(feature = "captures")]
pub mod captures;
pub mod data;
pub mod divergences;
