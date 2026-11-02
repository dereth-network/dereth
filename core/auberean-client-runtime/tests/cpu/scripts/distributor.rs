//! An object's scripts as the scripts ask about them.

use auberean_script::graph::field;
use auberean_script::heap::{Object, Target};
use auberean_script::value::NULL;
use auberean_script::Status;

use crate::common::{Bench, ITEM, ITEM_REF};

const DISTRIBUTOR: u32 = 0x7E;

/// An object's package is the package of its scripts' instance; every bit set while it has none.
///
/// Behaviour: script.model.an-objects-package-is-its-scripts
#[test]
fn an_objects_package_is_its_scripts_or_none() {
    let mut b = Bench::new();
    assert_eq!(b.answer(DISTRIBUTOR, 0x02, ITEM_REF, &[]), u32::MAX);
    let instance = b.vm.heap.add(
        field::OBJECT | field::LOADED,
        Target::Object(Box::new(Object::script(0x1A2B, 0))),
    );
    b.host.instances.insert(ITEM, instance);
    assert_eq!(b.answer(DISTRIBUTOR, 0x02, ITEM_REF, &[]), 0x1A2B);
    assert_eq!(b.call(DISTRIBUTOR, 0x02, NULL, &[], 1).0, Status::Crash);
}

/// When the character leaves the world, the player's command line is made anew: the commands its
/// scripts made known are forgotten (the next session's scripts make theirs known again).
///
/// Behaviour: script.model.the-players-commands-go-with-the-session
#[test]
fn the_players_commands_go_with_the_session() {
    let mut s = crate::common::scripts();
    assert!(s.host.commands.add("tell;t", 0x00B4_0100, 1));
    assert!(s.host.commands.names.contains_key("tell"));
    s.reset();
    assert!(
        s.host.commands.names.is_empty() && s.host.commands.expandable.is_empty(),
        "{:?}",
        s.host.commands
    );
}

/// An object that goes takes the calls of its scripts that wait with it: every call of its process
/// waiting to sleep out or to go on is dropped, other processes' calls kept in their order.
///
/// Behaviour: script.model.an-objects-waiting-calls-go-with-it
#[test]
fn an_objects_waiting_calls_go_with_it() {
    use auberean_client_runtime::latent::{purge, Waiting};
    let w = |process: usize, context: u32| Waiting { process, context };
    let mut waiting = vec![
        (1.0, w(3, 1)),
        (f64::NEG_INFINITY, w(4, 2)),
        (2.0, w(3, 5)),
        (3.0, w(7, 1)),
    ];
    purge(&mut waiting, 3);
    assert_eq!(waiting, vec![(f64::NEG_INFINITY, w(4, 2)), (3.0, w(7, 1))]);
}
