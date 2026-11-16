//! Behaviour: none (experimental)
//!
//! The remaster between the interface and the host: the host says whether it can draw it, and
//! the remaster asked for on the command line goes to the host whatever the options window asks.

use std::cell::RefCell;
use std::rc::Rc;

use auberean_shell::host::TestHost;
use auberean_shell::interface::{Game, Interface, Request};
use auberean_shell::{Presentation, Settings, Shell};

use crate::session::data;

/// An interface whose rendering section turns the remaster's box off when asked to flip full
/// screen, and which hears what the host can draw.
struct RemasterBox {
    offered: Rc<RefCell<Vec<bool>>>,
}

impl Interface for RemasterBox {
    fn update(&mut self, _game: &Game<'_>, _requests: &mut Vec<Request>) {}
    fn offer_remaster(&mut self, offered: bool) {
        self.offered.borrow_mut().push(offered);
    }
    fn toggle_full_screen(&mut self, requests: &mut Vec<Request>) {
        requests.push(Request::Presentation(Presentation {
            size: (800, 600),
            full_screen: false,
            triple_buffering: false,
            sync_to_display: true,
            antialiasing: true,
            remaster: false,
        }));
    }
}

/// The remaster asked for on the command line reaches the host this run though the options
/// window turns its box off, and the settings keep the box's value, not the command line's. What
/// the host can draw reaches the interface.
#[test]
fn the_remaster_asked_for_this_run_reaches_the_host_and_the_box_is_kept() {
    let (world, data) = data();
    let offered = Rc::new(RefCell::new(Vec::new()));
    let mut s = Shell::new(
        TestHost::default(),
        world,
        data.clone(),
        Box::new(RemasterBox {
            offered: Rc::clone(&offered),
        }),
    );
    s.settings = Settings::read("[Rendering]\r\nRemaster=1\r\n");
    s.settings.remaster_this_run = true;
    s.offer_remaster(true);
    s.offer_remaster(false);
    assert_eq!(*offered.borrow(), [true, false]);
    s.toggle_full_screen();
    assert_eq!(s.host.presentations.len(), 1);
    assert!(s.host.presentations[0].remaster, "drawn this run");
    assert!(!s.settings.remaster, "the box's value");
    assert!(s.settings.write().contains("Remaster=0"));
    // Without the command line the box's value is what the host is told.
    s.settings.remaster_this_run = false;
    s.toggle_full_screen();
    assert!(!s.host.presentations[1].remaster);
}
