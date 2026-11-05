//! The notice registry: who hears a notice, and in what order.

use auberean_ui_game::notices::{keys, Registry};

/// A notice is handed to its handlers in the order they registered; a handler registered again
/// for the same notice stays where it was; one unregistered from a notice no longer hears it, and
/// one that goes hears nothing more; a notice nobody registered for has no handlers.
///
/// Behaviour: ui.notices.handlers-hear-in-the-order-they-registered
#[test]
fn handlers_hear_a_notice_in_the_order_they_registered() {
    let mut r = Registry::new();
    r.register(0x417, 'b');
    r.register(0x417, 'a');
    r.register(0x436, 'a');
    r.register(0x417, 'c');
    r.register(0x417, 'b');
    assert_eq!(r.handlers(0x417), ['b', 'a', 'c'], "in order; once each");
    assert_eq!(r.handlers(0x999), [] as [char; 0]);
    r.unregister_all('a');
    assert_eq!(r.handlers(0x417), ['b', 'c']);
    assert_eq!(r.handlers(0x436), [] as [char; 0]);
}

/// A panel's registrations, written by the notices' own numbers, name a notice the player's
/// scripts send by the native that sends it and the client's own by its number; the examination
/// window's opening by both; each once, in order.
///
/// Behaviour: ui.notices.registrations-name-the-scripts-notices-by-their-natives
#[test]
fn registrations_name_the_scripts_notices_by_their_natives() {
    assert_eq!(
        keys(&[0x417, 0x436, 0x411, 0x417, 0x45F, 0x1_040C]),
        vec![
            0x0085 << 16 | 0x0009,
            0x436,
            0x0085 << 16 | 0x0008,
            0x0091 << 16 | 0x0002,
            0x45F,
            0x1_040C
        ]
    );
}
