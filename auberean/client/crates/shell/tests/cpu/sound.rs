//! How the shell hears the objects making sounds.

use auberean_shell::sound::{heard, Heard};

/// Behaviour: desktop.sound.an-object-out-of-the-world-is-silent-and-one-gone-stops
#[test]
fn an_object_out_of_the_world_is_silent_and_one_gone_stops() {
    // In the world, as its body says.
    assert_eq!(heard(Some(true), Some(false)), Heard::Heard);
    assert_eq!(heard(Some(true), Some(true)), Heard::Silent);
    // Out of the world its sounds wait for it; with no body left to ask, too.
    assert_eq!(heard(Some(false), None), Heard::Silent);
    assert_eq!(heard(Some(true), None), Heard::Heard);
    // Only an object the client no longer holds stops them for good.
    assert_eq!(heard(None, Some(false)), Heard::Gone);
    assert_eq!(heard(None, None), Heard::Gone);
}
