//! Behaviour: none (a host-opened store and event queue reach the complete front end).

use std::{cell::Cell, rc::Rc, sync::Arc};

use dereth_client::{
    app::{App, Platform},
    config::Config,
    present::NullPresentation,
};
use dereth_input::host::HostEvent;

#[test]
fn host_startup_reuses_the_store_and_routes_the_supplied_event_queue() {
    let store = Arc::new(
        dereth_dat::RetailDatStore::open_dir(&crate::common::client_dir()).expect("retail dats"),
    );
    let events = dereth_client::platform::window::WindowEvents::default();
    let phase = Cell::new(0);
    let cfg = Config {
        dat_dir: std::path::PathBuf::from("host-store-has-no-disk-directory"),
        headless: true,
        sound: false,
        ui: false,
        ..Config::default()
    };
    let mut app = App::bring_up_with_store(
        cfg,
        Some(Arc::clone(&store)),
        Rc::clone(&events),
        |cfg| {
            assert_eq!(phase.replace(1), 0);
            Ok(Platform::headless(cfg.width, cfg.height))
        },
        |_, width, height, _| {
            assert_eq!(phase.replace(2), 1);
            Ok(Box::new(NullPresentation::new(width, height)))
        },
    )
    .expect("the supplied store bypasses opening a directory");
    assert_eq!(phase.get(), 2);
    assert!(Arc::ptr_eq(app.dat_store(), &store));
    app.start_shell()
        .expect("the supplied presentation supports shell startup");
    events.borrow_mut().push(HostEvent::CloseRequested);
    assert!(!app.frame(), "the shell consumes its host's close request");
    assert!(events.borrow().is_empty());
}
