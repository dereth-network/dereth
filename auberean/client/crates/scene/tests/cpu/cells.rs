//! The cells' asks are heard by the cells alone.

use auberean_client_sdk::runtime::download::listeners;
use auberean_client_sdk::runtime::{creation, strings};
use auberean_scene::cells::CELLS;

/// The data cache tells each listener only its own batches: the cells' asks are told by a number
/// of their own, neither the objects' creations' nor the interface's string downloads', so the
/// scene hears when the cells about the player are in.
///
/// Behaviour: desktop.world.the-cells-asks-are-told-by-their-own-number
#[test]
fn the_cells_asks_are_told_by_a_number_of_their_own() {
    assert_eq!(CELLS, listeners::CELLS, "the cache's table of listeners");
    assert_ne!(CELLS, creation::CREATION.to, "the creations'");
    assert_ne!(CELLS, strings::STRINGS, "the string downloads'");
}
