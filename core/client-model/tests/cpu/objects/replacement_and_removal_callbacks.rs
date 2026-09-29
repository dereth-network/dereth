//! Replacement callbacks see the old weenie properties and a transient remove before the new
//! instance; a deferred UI remove keeps the body and flag until expiry retires it once.
//! Fixture: recorded messages and synthetic state or packets.

use dereth_client_model::{Notice, RecordingSink, World};
use dereth_primitives::{ObjectId, ServerTime};
use dereth_protocol::objects::ObjectCreatePayload;

fn object(id: ObjectId, instance: u16, name: &str) -> ObjectCreatePayload {
    let mut p = ObjectCreatePayload {
        id,
        ..Default::default()
    };
    p.physicsdesc.timestamps.instance = instance;
    p.wdesc.name = name.into();
    p
}

/// Behaviour: objects.replacement.a-newer-instance-wins-and-the-old-ones-traffic-is-dropped
#[test]
fn replacement_callbacks_observe_old_properties_and_transient_remove_before_new_instance() {
    let id = ObjectId(46);
    let mut world = World::new();
    world
        .create_or_merge(
            &object(id, 1, "Old Weenie"),
            ServerTime(1.0),
            &mut RecordingSink::default(),
        )
        .unwrap();
    world.set_selected_object(Some(id), false, &mut RecordingSink::default());
    let mut seen = Vec::new();
    world
        .create_or_merge_with_dispatch(
            &object(id, 2, "New Weenie"),
            ServerTime(2.0),
            &mut |world, notice| match notice {
                Notice::ItemMoved { object, .. } if object == id => {
                    let w = world
                        .weenie(id)
                        .expect("old Weenie lives through ItemMoved");
                    if w.pwd.name == "New Weenie" {
                        assert!(!w.being_removed);
                        assert!(
                            world.physics(id).is_some(),
                            "new descriptor's own ItemMoved follows creation"
                        );
                        return;
                    }
                    seen.push(("moved", w.pwd.name.clone(), w.being_removed));
                    assert!(
                        world.physics(id).is_none(),
                        "physical table is removed before Weenie callback"
                    );
                }
                Notice::SelectionChanged {
                    previous: Some(old),
                    current: None,
                } if old == id => {
                    let w = world
                        .weenie(id)
                        .expect("old Weenie lives through selection callback");
                    seen.push(("selection", w.pwd.name.clone(), w.being_removed));
                    assert!(world.physics(id).is_none());
                }
                Notice::ObjectDeleted(old) if old == id => {
                    assert!(
                        world.weenie(id).is_none(),
                        "final deletion precedes replacement instantiate"
                    );
                    seen.push(("deleted", String::new(), false));
                }
                _ => {}
            },
        )
        .unwrap();
    assert_eq!(
        seen,
        vec![
            ("moved", "Old Weenie".into(), true),
            ("selection", "Old Weenie".into(), true),
            ("deleted", String::new(), false)
        ]
    );
    assert_eq!(world.weenie(id).unwrap().pwd.name, "New Weenie");
    assert!(!world.weenie(id).unwrap().being_removed);
}

#[test]
fn deferred_ui_remove_keeps_body_and_callback_flag_then_expiry_retires_it_once() {
    let id = ObjectId(47);
    let mut world = World::new();
    world
        .create_or_merge(
            &object(id, 1, "Deferred"),
            ServerTime(1.0),
            &mut RecordingSink::default(),
        )
        .unwrap();
    world.set_selected_object(Some(id), false, &mut RecordingSink::default());
    let mut callbacks = 0;
    world.server_says_remove_with_dispatch(id, ServerTime(3.0), &mut |world, notice| {
        if matches!(
            notice,
            Notice::ItemMoved { .. } | Notice::SelectionChanged { .. }
        ) {
            assert!(world.weenie(id).unwrap().being_removed);
            assert!(
                world.physics(id).is_some(),
                "UI remove isn't immediate physical deletion"
            );
            callbacks += 1;
        }
    });
    assert_eq!(callbacks, 2);
    assert!(!world.weenie(id).unwrap().being_removed);
    assert_eq!(world.tables.doomed.get(id), Some(&ServerTime(28.0)));
    let mut notices = RecordingSink::default();
    world.use_time(
        ServerTime(28.0),
        &mut notices,
        &mut dereth_client_model::RecordingRequests::default(),
    );
    assert!(world.weenie(id).is_some(), "strict equality is not expired");
    world.use_time_with_dispatch(
        ServerTime(28.1),
        &mut |world, notice| {
            if let Notice::ItemMoved { .. } = notice {
                assert!(world.weenie(id).unwrap().being_removed);
                assert!(world.physics(id).is_none());
            }
        },
        &mut dereth_client_model::RecordingRequests::default(),
    );
    assert!(world.weenie(id).is_none());
}
