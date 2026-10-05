//! The App frame's phase order: UI messages received during a frame's network phase run at the
//! next frame's entry, against the previous simulated time; object messages are admitted per
//! instance generation, so deletes, same-id recreates and later events land on the right lifetime;
//! parked blobs release inside the arrival that unblocks them; selection, toolbar queries and
//! inventory refills complete in the frame that produced them; and the HUD's scroll phase and
//! login's per-frame reports run on frames with no link or no session event.
//!
//! Fixture: a real `App` frame over an explicit socket-free endpoint. Packet envelopes come from
//! the transport writer; object and UI bodies are written in the test (one case replays
//! long-solo-play's remote human). No GUI, socket, owner settings, or injected expected delivery
//! or model state.
#![cfg(gpu)]
use dereth_primitives::{LocalTime, ObjectId, ServerTime};
use dereth_protocol::objects::{ItemCreateObject, ItemServerSaysRemove, ObjectCreatePayload};
use {
    dereth_client::app::App, dereth_client_runtime::config::Config,
    dereth_client_runtime::net::ClientNetwork,
    dereth_client_runtime::platform::clock::HEADLESS_STEP,
};

struct Peer {
    crypto: dereth_transport::CryptoSystem,
    sequence: u32,
    blob: u32,
}
impl Peer {
    fn new() -> (Self, ClientNetwork) {
        let mut net =
            ClientNetwork::new("127.0.0.1:19000", 7304, "phase-station", "unused", 0).unwrap();
        // Explicit private transport connection facts; no authentication or socket is started.
        net.session.transport.add_connection(
            0xB,
            0,
            1,
            0xDEADBEEF,
            0x12345678,
            Some("127.0.0.1:19000".parse().unwrap()),
        );
        (
            Self {
                crypto: dereth_transport::CryptoSystem::new(0xDEADBEEF),
                sequence: 1,
                blob: 0,
            },
            net,
        )
    }
    fn send(&mut self, app: &mut App, queue: u16, bytes: Vec<u8>) {
        self.sequence += 1;
        self.blob += 1;
        let mut packet = dereth_transport::OutPacket::new(dereth_transport::ProtoHeader {
            seq_id: self.sequence,
            rec_id: 0xB,
            interval: 0x100,
            iteration: 1,
            ..Default::default()
        });
        packet
            .add_fragment(dereth_transport::Fragment::new(
                dereth_transport::FragmentHeader {
                    blob_id_low: self.blob,
                    blob_id_high: 0x80000000,
                    num_frags: 1,
                    blob_frag_size: 0,
                    blob_num: 0,
                    queue_id: queue,
                },
                bytes,
            ))
            .unwrap();
        let raw = packet.serialize(Some(self.crypto.next())).unwrap();
        app.replay_network_mut()
            .unwrap()
            .session
            .transport
            .feed(&raw, None, LocalTime(0.0))
            .unwrap();
    }
}
fn app() -> App {
    app_with_ui(false)
}
fn app_with_ui(ui: bool) -> App {
    App::new(Config {
        headless: true,
        sound: false,
        ui,
        preferences_file: std::env::temp_dir().join("dere-frame-phases-not-created/prefs.ini"),
        dat_dir: dereth_dat::testing::dat_dir(),
        ..Default::default()
    })
    .unwrap()
}
fn object(id: ObjectId, parent: ObjectId) -> ObjectCreatePayload {
    let mut p = ObjectCreatePayload {
        id,
        ..Default::default()
    };
    // Shipped Aluvian setup, explicitly authored rather than relying on assetless-model
    // PhysicsPresence defaults.
    p.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::SETUP;
    p.physicsdesc.setup_id = Some(0x02000001);
    p.physicsdesc.timestamps.instance = 1;
    p.wdesc.header |= dereth_protocol::types::weeniedesc::header::CONTAINER_ID;
    p.wdesc.container_id = Some(parent);
    p
}

/// Behaviour: objects.lifecycle.ui-that-arrives-mid-frame-waits-for-the-next-frame-entry
///
/// The frame processes UI before advancing the timer and polling the network. UI first received
/// during that network phase cannot execute until the next frame, whose UI handlers still see
/// the preceding frame's time.
#[test]
fn newly_received_ui_waits_for_next_entry_and_uses_pre_timer_clock() {
    let mut app = app();
    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net).unwrap();
    let id = ObjectId(0x89000001);
    let parent = ObjectId(0x50000001);
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&ItemCreateObject(object(id, parent))).unwrap(),
    );
    assert!(app.frame());
    assert!(
        app.objects().world.physics(id).is_some(),
        "explicit setup initialized physical owner"
    );
    assert_eq!(
        app.objects().world.weenie(id).unwrap().pwd.container_id,
        Some(parent)
    );
    peer.send(
        &mut app,
        9,
        dereth_protocol::write_blob(&ItemServerSaysRemove { object: id }).unwrap(),
    );
    assert!(app.frame());
    assert_eq!(
        app.objects().world.weenie(id).unwrap().pwd.container_id,
        Some(parent),
        "newly polled UI belongs to the next UIQueue phase"
    );
    assert!(app.objects().world.tables.doomed.get(id).is_none());
    assert!(app.frame());
    assert_eq!(
        app.objects().world.weenie(id).unwrap().pwd.container_id,
        Some(ObjectId(0))
    );
    assert_eq!(
        app.objects().world.tables.doomed.get(id),
        Some(&ServerTime(25.0 + 2.0 * HEADLESS_STEP)),
        "UI at entry reads the previous simulated time, neither the new timer nor raw network time"
    );
}

#[test]
fn replay_endpoint_is_explicit_and_does_not_claim_socket_sends() {
    let (_, net) = Peer::new();
    let mut link = dereth_client_runtime::net::NetLink::replay(net);
    assert_eq!(link.local_addr(), None);
    link.tick(LocalTime(0.0));
    assert!(
        !link.net.take_outgoing().is_empty(),
        "retained login/transport output is observable"
    );
    let (sent, queued) = link.log_off_server();
    assert_eq!(sent, 0, "there is no physical socket send");
    assert!(queued > 0);
    assert_eq!(link.net.take_outgoing().len(), queued);
}

/// Behaviour: objects.lifecycle.a-parked-delete-never-removes-a-newer-instance
#[test]
fn mixed_ui_remove_then_newer_create_must_remove_old_instance_only() {
    use dereth_protocol::objects::{ItemDeleteObject, ItemSetState, ItemUpdateObject};
    let mut app = app();
    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net).unwrap();
    let id = ObjectId(0x89000002);
    let old_parent = ObjectId(0x50000002);
    let new_parent = ObjectId(0x50000003);
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&ItemCreateObject(object(id, old_parent))).unwrap(),
    );
    assert!(app.frame());
    peer.send(
        &mut app,
        9,
        dereth_protocol::write_blob(&ItemServerSaysRemove { object: id }).unwrap(),
    );
    assert!(app.frame()); // receive only: this UI is pending at next frame's entry
    let mut next = object(id, new_parent);
    next.physicsdesc.timestamps.instance = 2;
    let state = |instance, event, value| {
        dereth_protocol::write_blob(&ItemSetState {
            id,
            state: value,
            timestamps: dereth_protocol::types::PhysicsEventStamp { instance, event },
        })
        .unwrap()
    };
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&ItemCreateObject(next.clone())).unwrap(),
    );
    peer.send(&mut app, 10, state(1, 99, 0x4000)); // old packet must never touch replacement
    peer.send(&mut app, 10, state(2, 30, 0x800)); // first admitted only after create
    assert!(app.frame());
    assert_eq!(app.objects().presence(id).unwrap().instance, 2);
    assert_eq!(
        app.objects().world.weenie(id).unwrap().pwd.container_id,
        Some(new_parent)
    );
    assert!(app.objects().world.tables.doomed.get(id).is_none());
    assert_eq!(app.objects().physics_state(id), Some(0x800));
    // Immediate delete then same-id create then a new-generation event in ONE raw queue.
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&ItemDeleteObject {
            id,
            instance_sequence: 2,
        })
        .unwrap(),
    );
    next.physicsdesc.timestamps.instance = 3;
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&ItemCreateObject(next.clone())).unwrap(),
    );
    peer.send(&mut app, 10, state(3, 1, 0x4000));
    assert!(app.frame());
    assert_eq!(app.objects().physics_state(id), Some(0x4000));
    assert_eq!(app.objects().presence(id).unwrap().state_ts, 1);
    // A forced `Item_UpdateObject 0xF7DB` resets even an equal sequence; the later operation
    // starts from its actual stamps.
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&ItemUpdateObject(next)).unwrap(),
    );
    peer.send(&mut app, 10, state(3, 1, 0x800));
    assert!(app.frame());
    assert_eq!(app.objects().physics_state(id), Some(0x800));
}

/// Behaviour: objects.lifecycle.a-parked-delete-never-removes-a-newer-instance
#[test]
fn same_batch_new_generation_message_after_delete_create_must_not_be_lost_on_old_parked_list() {
    use dereth_protocol::objects::{ItemDeleteObject, ItemSetState};
    let mut app = app();
    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net).unwrap();
    let id = ObjectId(932);
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&ItemCreateObject(object(id, ObjectId(0)))).unwrap(),
    );
    assert!(app.frame());
    let mut next = object(id, ObjectId(0));
    next.physicsdesc.timestamps.instance = 2;
    for blob in [
        dereth_protocol::write_blob(&ItemDeleteObject {
            id,
            instance_sequence: 1,
        })
        .unwrap(),
        dereth_protocol::write_blob(&ItemCreateObject(next)).unwrap(),
        dereth_protocol::write_blob(&ItemSetState {
            id,
            state: 0x4000,
            timestamps: dereth_protocol::types::PhysicsEventStamp {
                instance: 2,
                event: 30,
            },
        })
        .unwrap(),
    ] {
        peer.send(&mut app, 10, blob);
    }
    assert!(app.frame());
    assert_eq!(app.objects().presence(id).unwrap().instance, 2);
    assert_eq!(
        app.objects().physics_state(id),
        Some(0x4000),
        "later new-instance message belongs after create, not to old parked list"
    );
}

#[test]
fn arriving_objects_release_weenie_ui_before_parked_physics_delete_and_next_entry() {
    use dereth_protocol::objects::ItemDeleteObject;
    let mut app = app();
    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net).unwrap();
    let owner = ObjectId(0x89000003);
    let other = ObjectId(0x89000004);
    let parent = ObjectId(0x50000004);
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&ItemCreateObject(object(other, parent))).unwrap(),
    );
    assert!(app.frame());
    // Constructed legal per-object order owner, separate from the event's target object.
    peer.send(
        &mut app,
        9,
        dereth_protocol::events::pack_event(owner, 1, &ItemServerSaysRemove { object: other })
            .unwrap(),
    );
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&ItemDeleteObject {
            id: owner,
            instance_sequence: 1,
        })
        .unwrap(),
    );
    assert!(app.frame()); // physical delete is parked; UI isn't processed yet
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&ItemCreateObject(object(owner, parent))).unwrap(),
    );
    assert!(app.frame()); // UI parks in null-Weenie; arrival releases UI, THEN physical delete
    assert!(app.objects().world.weenie(owner).is_none());
    assert_eq!(
        app.objects().world.weenie(other).unwrap().pwd.container_id,
        Some(ObjectId(0)),
        "the surviving target proves UI callback ran before deleting the ordering owner"
    );
    assert_eq!(
        app.objects().world.tables.doomed.get(other),
        Some(&ServerTime(25.0 + 3.0 * HEADLESS_STEP)),
        "arrival callback uses the current world-controller clock, not the prior UI-at-entry clock"
    );
    // The fresh owner's ordering starts at 1; the old continuation must not consume or clear this
    // generation.
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&ItemCreateObject(object(owner, parent))).unwrap(),
    );
    assert!(app.frame());
    peer.send(
        &mut app,
        9,
        dereth_protocol::events::pack_event(owner, 1, &ItemServerSaysRemove { object: owner })
            .unwrap(),
    );
    assert!(app.frame());
    assert!(app.frame());
    assert_eq!(
        app.objects().world.weenie(owner).unwrap().pwd.container_id,
        Some(ObjectId(0))
    );
}

#[test]
fn blocked_cell_owner_still_drains_incoming_and_early_exit_only_runs_entry_ui() {
    use dereth_protocol::objects::ItemSetState;
    let mut app = app();
    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net).unwrap();
    let id = ObjectId(0x89000005);
    let later = ObjectId(0x89000006);
    let parent = ObjectId(0x50000005);
    app.defer_static_scene(Default::default()); // no player yet: actual available cell-load guard
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&ItemCreateObject(object(id, parent))).unwrap(),
    );
    assert!(app.frame());
    app.probe_mut()
        .objects_mut()
        .world
        .schedule_destroy(id, ServerTime(-26.0)); // overdue maintenance stimulus
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&ItemSetState {
            id,
            state: 0x800,
            timestamps: dereth_protocol::types::PhysicsEventStamp {
                instance: 1,
                event: 1,
            },
        })
        .unwrap(),
    );
    peer.send(
        &mut app,
        9,
        dereth_protocol::write_blob(&ItemServerSaysRemove { object: id }).unwrap(),
    );
    assert!(app.frame());
    assert_eq!(
        app.objects().physics_state(id),
        Some(0x800),
        "blocked cells do not block incoming"
    );
    assert!(
        app.objects().world.weenie(id).is_some(),
        "blocked maintenance does not expire owner"
    );
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&ItemCreateObject(object(later, parent))).unwrap(),
    );
    let sent = app.replay_network_mut().unwrap().take_outgoing();
    app.done();
    assert!(!app.frame());
    assert_eq!(
        app.objects().world.weenie(id).unwrap().pwd.container_id,
        Some(ObjectId(0)),
        "UI-at-entry still runs before Device discovers Done"
    );
    assert!(
        app.objects().world.weenie(later).is_none(),
        "device exit prevents network admission"
    );
    assert!(
        app.replay_network_mut().unwrap().take_outgoing().is_empty(),
        "no extra send phase"
    );
    assert!(!sent.is_empty(), "earlier real endpoint output exists");
}

/// Object creation retains a gameplay object even if initializing its physical placeholder fails.
/// Only a successful physical return immediately releases the object's queued network blobs.
#[test]
fn failed_physical_init_preserves_ordered_weenie_then_equal_instance_replaces_its_lifetime() {
    use dereth_protocol::objects::ItemSetState;
    use dereth_protocol::types::physicsdesc::{flags, ChildLink};
    let mut app = app();
    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net).unwrap();
    let holder = ObjectId(0x89000020);
    let child = ObjectId(0x89000021);
    let parked_target = ObjectId(0x89000022);
    let stale_target = ObjectId(0x89000023);
    let parent = ObjectId(0x50000020);
    let mut holding = object(holder, parent);
    holding.physicsdesc.bitfield |= flags::CHILDREN;
    holding.physicsdesc.children = Some(vec![ChildLink {
        child_id: child,
        location_id: 1,
    }]);
    for payload in [
        holding,
        object(parked_target, parent),
        object(stale_target, parent),
    ] {
        peer.send(
            &mut app,
            10,
            dereth_protocol::write_blob(&ItemCreateObject(payload)).unwrap(),
        );
    }
    assert!(app.frame());
    assert!(
        app.objects().world.physics(holder).is_some(),
        "actual DAT setup initialized"
    );
    assert!(app.objects().world.tables.null_physics.contains_key(child));
    assert_eq!(
        app.objects().world.physics_parent(child),
        Some((holder, 1)),
        "shipped holding location creates the old physical-null owner"
    );
    assert!(app.objects().world.weenie(child).is_none());
    let state = |event, value| {
        dereth_protocol::write_blob(&ItemSetState {
            id: child,
            state: value,
            timestamps: dereth_protocol::types::PhysicsEventStamp { instance: 1, event },
        })
        .unwrap()
    };
    peer.send(&mut app, 10, state(7, 0x4000));
    peer.send(
        &mut app,
        9,
        dereth_protocol::events::pack_event(
            child,
            2,
            &ItemServerSaysRemove {
                object: parked_target,
            },
        )
        .unwrap(),
    );
    assert!(app.frame()); // receive UI; physics blob parks on the old null
    let mut failed = object(child, parent);
    failed.physicsdesc.setup_id = Some(0); // A zero setup ID makes physical placeholder initialization fail.
    failed.wdesc.name = "Partial Weenie".into();
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&ItemCreateObject(failed)).unwrap(),
    );
    assert!(app.frame()); // UI parks first; physical initialization then fails
    let partial = app.objects().world.weenie(child).unwrap();
    assert_eq!(partial.pwd.name, "Partial Weenie");
    assert_eq!(partial.instance_seq, 1);
    assert!(!partial.has_phys_obj);
    assert!(app.objects().world.physics(child).is_none());
    assert!(app.objects().presence(child).is_none());
    assert!(!app.objects().world.tables.null_physics.contains_key(child));
    assert!(!app
        .replay_network_mut()
        .unwrap()
        .session
        .instances_mut()
        .knows(child));
    // Destructive observation only on failure: correct physical retirement leaves nothing
    // to drain. No identity/final state is supplied; any returned blob fails immediately.
    assert!(
        app.replay_network_mut()
            .unwrap()
            .session
            .take_object_message(child)
            .is_empty(),
        "the failed physical owner destroys its parked 0xF74B, independently of the surviving UI"
    );
    assert_eq!(
        app.objects()
            .world
            .weenie(parked_target)
            .unwrap()
            .pwd
            .container_id,
        Some(parent)
    );
    assert!(app.frame());
    assert_eq!(
        app.objects()
            .world
            .weenie(parked_target)
            .unwrap()
            .pwd
            .container_id,
        Some(parent),
        "no packet frame must not invent an immediate/finalized arrival drain"
    );
    // The real later UI owner lookup finds the surviving Weenie. Stamp 1 delivers now and only
    // its successful callback continuation releases the earlier parked stamp 2.
    peer.send(
        &mut app,
        9,
        dereth_protocol::events::pack_event(child, 1, &ItemServerSaysRemove { object: child })
            .unwrap(),
    );
    assert!(app.frame());
    assert_eq!(
        app.objects().world.weenie(child).unwrap().pwd.container_id,
        Some(parent)
    );
    assert!(app.frame());
    assert_eq!(
        app.objects().world.weenie(child).unwrap().pwd.container_id,
        Some(ObjectId(0))
    );
    assert_eq!(
        app.objects()
            .world
            .weenie(parked_target)
            .unwrap()
            .pwd
            .container_id,
        Some(ObjectId(0)),
        "old UI stamp 2 survived physical-only destruction and ran after fresh stamp 1"
    );
    assert!(app.objects().world.physics(child).is_none());
    assert!(app.objects().presence(child).is_none());
    assert!(
        app.objects().world.tables.doomed.get(child).is_some(),
        "the 0x0024 remove scheduled its owner"
    );

    // Keep an old-generation gap to prove equal-instance recovery deletes the Weenie's
    // SequenceGate owner rather than merely publishing physics onto a surviving ordering window.
    peer.send(
        &mut app,
        9,
        dereth_protocol::events::pack_event(
            child,
            4,
            &ItemServerSaysRemove {
                object: stale_target,
            },
        )
        .unwrap(),
    );
    assert!(app.frame());
    assert!(app.frame());
    let mut valid = object(child, parent);
    valid.wdesc.name = "Recovered Weenie".into();
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&ItemCreateObject(valid)).unwrap(),
    );
    assert!(app.frame());
    assert_eq!(
        app.objects().world.weenie(child).unwrap().pwd.name,
        "Recovered Weenie"
    );
    assert_eq!(
        app.objects().presence(child).unwrap().instance,
        1,
        "equal instance is legal without old physics"
    );
    assert!(app.objects().world.physics(child).is_some());
    assert!(app
        .replay_network_mut()
        .unwrap()
        .session
        .instances_mut()
        .knows(child));
    assert_eq!(
        app.objects().physics_state(child),
        Some(0),
        "no retired old physical state replays"
    );
    assert_eq!(app.objects().stats.state_events, 0);
    assert!(
        app.objects().world.tables.doomed.get(child).is_none(),
        "the new lifetime does not inherit the 0x0024 remove's deadline"
    );
    for stamp in 1..=3 {
        peer.send(
            &mut app,
            9,
            dereth_protocol::events::pack_event(
                child,
                stamp,
                &ItemServerSaysRemove { object: child },
            )
            .unwrap(),
        );
    }
    peer.send(&mut app, 10, state(1, 0x800));
    assert!(app.frame());
    assert_eq!(
        app.objects().physics_state(child),
        Some(0x800),
        "fresh physical packet reaches new owner"
    );
    assert_eq!(app.objects().presence(child).unwrap().state_ts, 1);
    assert!(app.frame());
    assert_eq!(
        app.objects().world.weenie(child).unwrap().pwd.container_id,
        Some(ObjectId(0)),
        "fresh UI stamp 1 was accepted by the replacement, not rejected against the old highest 2"
    );
    assert_eq!(
        app.objects()
            .world
            .weenie(stale_target)
            .unwrap()
            .pwd
            .container_id,
        Some(parent),
        "fresh stamps 1..3 must not release the old lifetime's blocked stamp 4"
    );
    peer.send(
        &mut app,
        9,
        dereth_protocol::events::pack_event(
            child,
            4,
            &ItemServerSaysRemove {
                object: stale_target,
            },
        )
        .unwrap(),
    );
    assert!(app.frame());
    assert!(app.frame());
    assert_eq!(
        app.objects()
            .world
            .weenie(stale_target)
            .unwrap()
            .pwd
            .container_id,
        Some(ObjectId(0)),
        "a subsequent genuinely arriving stamp 4 remains usable"
    );
}

/// Ordered insertion does not drain a ready old head when the NEW stamp is itself
/// out of order. A Weenie-only arrival must not quietly invent the skipped physical tail.
#[test]
fn failed_init_without_prior_physical_null_does_not_eagerly_release_parked_ui() {
    let mut app = app();
    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net).unwrap();
    let owner = ObjectId(0x89000030);
    let first = ObjectId(0x89000031);
    let second = ObjectId(0x89000032);
    let parent = ObjectId(0x50000030);
    for id in [first, second] {
        peer.send(
            &mut app,
            10,
            dereth_protocol::write_blob(&ItemCreateObject(object(id, parent))).unwrap(),
        );
    }
    peer.send(
        &mut app,
        9,
        dereth_protocol::events::pack_event(owner, 1, &ItemServerSaysRemove { object: first })
            .unwrap(),
    );
    assert!(app.frame());
    assert!(!app.objects().world.tables.null_physics.contains_key(owner));
    let mut failed = object(owner, parent);
    failed.physicsdesc.setup_id = Some(0);
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&ItemCreateObject(failed)).unwrap(),
    );
    assert!(app.frame());
    assert!(app.objects().world.weenie(owner).is_some());
    assert!(app.objects().world.physics(owner).is_none());
    assert!(app.objects().presence(owner).is_none());
    assert!(!app
        .replay_network_mut()
        .unwrap()
        .session
        .instances_mut()
        .knows(owner));
    peer.send(
        &mut app,
        9,
        dereth_protocol::events::pack_event(owner, 2, &ItemServerSaysRemove { object: second })
            .unwrap(),
    );
    assert!(app.frame());
    assert!(app.frame());
    assert!(app.frame()); // no eager global ready-window polling while no packets arrive
    for id in [first, second] {
        assert_eq!(
            app.objects().world.weenie(id).unwrap().pwd.container_id,
            Some(parent),
            "highest is still 0: parked 1 plus new 2 does not call the success continuation"
        );
    }
    // Missing-physical recovery destroys that partial Weenie and its undrained SequenceGate.
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&ItemCreateObject(object(owner, parent))).unwrap(),
    );
    assert!(app.frame());
    assert!(app.objects().world.physics(owner).is_some());
    assert!(app.objects().presence(owner).is_some());
    for id in [first, second] {
        assert_eq!(
            app.objects().world.weenie(id).unwrap().pwd.container_id,
            Some(parent),
            "recovery does not release the deleted Weenie's blocked window"
        );
    }
    peer.send(
        &mut app,
        9,
        dereth_protocol::events::pack_event(owner, 1, &ItemServerSaysRemove { object: first })
            .unwrap(),
    );
    assert!(app.frame());
    assert!(app.frame());
    assert_eq!(
        app.objects().world.weenie(first).unwrap().pwd.container_id,
        Some(ObjectId(0))
    );
    assert_eq!(
        app.objects().world.weenie(second).unwrap().pwd.container_id,
        Some(parent),
        "fresh stamp 1 must not release old-generation stamp 2"
    );
}

#[test]
fn recent_attacker_removal_dispatches_selection_while_old_weenie_is_being_removed() {
    use dereth_client_model::{
        combat::CombatMode,
        qualities::{Qualities, StatKey, StatType, StatValue},
    };
    use dereth_protocol::types::{physicsdesc::flags, weeniedesc::header};
    let mut app = app_with_ui(true);
    app.start_shell().unwrap();
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    // Establish the subscriber during the UI phase before the object phase loads a scene, as the
    // production frame does. World loading is not part of the query-tail stimulus below.
    assert!(app.frame());
    app.load_static_scene(dereth_client_runtime::scene::SceneConfig {
        character: true,
        cell_statics: false,
        mesh_collision: false,
        land_radius: 1,
        scenery_radius: 0,
        particles: false,
        ..Default::default()
    })
    .unwrap();
    let player = dereth_client_runtime::character::PLAYER_OBJECT_ID;
    let attacker = ObjectId(0x89000007);
    let survivor = ObjectId(0x89000008);
    // Explicit private player/option prerequisites, not an injected final selection or target.
    let mut weenie = dereth_client_model::Weenie::new(player);
    let mut qualities = Qualities::new();
    qualities.set(
        StatKey::new(
            StatType::Iid,
            dereth_client_model::selection::LAST_ATTACKER_IID,
        ),
        StatValue::Iid(attacker),
    );
    weenie.qualities = Some(qualities);
    app.probe_mut()
        .objects_mut()
        .world
        .tables
        .weenies
        .insert(player, weenie);
    app.probe_mut().objects_mut().world.set_player(player);
    app.probe_mut()
        .objects_mut()
        .world
        .player_system
        .options
        .set(
            dereth_client_model::player::options::option::AUTO_TARGET,
            true,
        );
    app.probe_mut().objects_mut().world.combat.combat_mode = CombatMode::Melee;
    let here = app
        .world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .position();
    let creature = |id, offset: f32| {
        let mut p = object(id, ObjectId(0));
        p.wdesc.header &= !header::CONTAINER_ID;
        p.wdesc.container_id = None;
        p.wdesc.header |= header::RADAR_ENUM;
        p.wdesc.radar_enum = Some(4);
        p.wdesc.obj_type = dereth_client_model::weenie::item_type::CREATURE;
        p.wdesc.bitfield = dereth_client_model::weenie::bitfield::ATTACKABLE;
        p.physicsdesc.bitfield |= flags::POSITION;
        let at = dereth_physics::math::localtoglobal(
            &here.frame,
            dereth_primitives::Vec3::new(0.0, offset, 0.0),
        );
        p.physicsdesc.position = Some(dereth_protocol::types::PositionWire {
            objcell_id: here.cell.0,
            frame: dereth_protocol::types::Frame {
                origin: at.into(),
                orientation: dereth_primitives::Quat::IDENTITY.into(),
            },
        });
        p
    };
    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net).unwrap();
    for (id, offset) in [(attacker, 5.0), (survivor, 9.0)] {
        peer.send(
            &mut app,
            10,
            dereth_protocol::write_blob(&ItemCreateObject(creature(id, offset))).unwrap(),
        );
    }
    assert!(app.frame());
    app.probe_mut()
        .objects_mut()
        .world
        .update_visible_object_list(); // the visible-list refresh, called explicitly
    peer.send(
        &mut app,
        9,
        dereth_protocol::write_blob(&dereth_protocol::combat::EvasionDefenderNotification {
            attacker_name: "Private Attacker".into(),
        })
        .unwrap(),
    );
    assert!(app.frame());
    assert!(app.frame());
    assert_eq!(
        app.objects().world.selected,
        Some(attacker),
        "real defender UI callback selected recent attacker"
    );
    let mut replacement = creature(attacker, 5.0);
    replacement.physicsdesc.timestamps.instance = 2;
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&ItemCreateObject(replacement)).unwrap(),
    );
    assert!(app.frame());
    assert_eq!(app.objects().presence(attacker).unwrap().instance, 2);
    assert_eq!(app.objects().world.selected,Some(survivor),
        "selection callback must reject old being_removed attacker before same-ID replacement exists");
    assert!(!app.objects().world.weenie(attacker).unwrap().being_removed);
    // A later genuine defender operation may now select the live replacement. No stale suppression.
    peer.send(
        &mut app,
        9,
        dereth_protocol::write_blob(&dereth_protocol::combat::EvasionDefenderNotification {
            attacker_name: "Private Replacement".into(),
        })
        .unwrap(),
    );
    // The 0x0024 remove clears the survivor while it is being removed; the fresh last attacker is
    // the replacement.
    peer.send(
        &mut app,
        9,
        dereth_protocol::write_blob(&ItemServerSaysRemove { object: survivor }).unwrap(),
    );
    assert!(app.frame());
    assert!(app.frame());
    assert_eq!(app.objects().world.selected, Some(attacker));
    // Two object removal/selection boundaries after the UI phase in the SAME frame. Both
    // toolbar query tails must reach the owning Request queue immediately and in order,
    // not survive in the UI TLS queue to observe only the final replacement next frame.
    peer.send(
        &mut app,
        9,
        dereth_protocol::write_blob(&dereth_protocol::combat::CombatQueryHealthResponse {
            object: attacker,
            health: 0.5,
        })
        .unwrap(),
    );
    assert!(app.frame());
    assert!(app.frame());
    let health = {
        let any: &dyn std::any::Any = app.ui().unwrap().flow.current().unwrap();
        any.downcast_ref::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
            .unwrap()
            .toolbar_children
            .get("sel_object_health_meter")
            .unwrap()
    };
    assert!(
        app.ui().unwrap().ui.is_visible(health),
        "actual server reply exposes health meter before clear"
    );
    let mut third = creature(attacker, 5.0);
    third.physicsdesc.timestamps.instance = 3;
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&ItemCreateObject(third)).unwrap(),
    );
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&dereth_protocol::objects::ItemDeleteObject {
            id: survivor,
            instance_sequence: 1,
        })
        .unwrap(),
    );
    assert!(app.frame());
    assert_eq!(app.objects().world.selected, Some(attacker));
    let queries: Vec<_> = app
        .interaction()
        .last_sent
        .iter()
        .filter_map(|request| {
            if let dereth_client_model::Request::QueryHealth(query) = request {
                Some(query.target)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        queries,
        vec![ObjectId(0), survivor, attacker],
        "actual toolbar clear/query calls preserve both synchronous selection edges"
    );
    let residual = app.ui_mut().expect("the UI shell is up").ui.requests.take();
    assert!(
        !residual.iter().any(|r| matches!(
            r,
            dereth_ui_screens::view::UiRequest::QueryHealth(_)
                | dereth_ui_screens::view::UiRequest::QueryItemMana(_)
        )),
        "no query deferred into next frame"
    );
    for request in residual {
        app.ui_mut()
            .expect("the UI shell is up")
            .ui
            .requests
            .emit(request);
    }
}

#[test]
fn two_remote_commands_complete_callbacks_without_coalescing_or_advancing_physics() {
    use dereth_animation::command::MotionCommand;
    use dereth_client_net::client_session::testing::{Corpus, Direction};
    use dereth_protocol::{
        movement::{
            motion_flags, InterpretedMotionState, MotionAction, MovementBody, MovementBuffer,
            MovementSetObjectMovement,
        },
        Opcode,
    };
    let mut app = app();
    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net).unwrap();
    let id = ObjectId(0x5000000a);
    let corpus = Corpus::load("long-solo-play")
        .unwrap()
        .expect("required recorded setup/position/state");
    for row in corpus.blobs.iter().filter(|r| {
        r.dir == Direction::ServerToClient
            && r.t_rel_micros < 91_500_548
            && [
                Opcode::ITEM_CREATE_OBJECT.0,
                Opcode::MOVEMENT_POSITION_EVENT.0,
                Opcode::ITEM_SET_STATE.0,
            ]
            .contains(&r.opcode)
            && r.payload.get(4..8) == Some(id.0.to_le_bytes().as_slice())
    }) {
        peer.send(&mut app, 10, row.payload.clone());
    }
    assert!(app.frame());
    let initial = app.objects().presence(id).unwrap().position.unwrap();
    let block = initial.cell.landblock();
    app.load_static_scene(dereth_client_runtime::scene::SceneConfig {
        landblock: (u16::from(block.x()) << 8) | u16::from(block.y()),
        character: true,
        land_radius: 1,
        scenery_radius: 0,
        cell_statics: false,
        mesh_collision: false,
        particles: false,
        ..Default::default()
    })
    .unwrap();
    // Private observer placement, as in the recorded remote-motion station. The recorded
    // human must be near the physical owner, not outside its active cell neighborhood.
    let character = app
        .probe_mut()
        .world_state_mut()
        .unwrap()
        .character
        .as_mut()
        .unwrap();
    character.land().load_block_cells(block);
    let mut observer = initial;
    observer.frame.origin.x += 3.0;
    character.teleport(observer);
    for _ in 0..120 {
        assert!(app.frame());
    }
    assert_eq!(
        app.objects().player(),
        None,
        "recorded human follows remote, not local, owner"
    );
    let scene = app.world_scene().unwrap();
    let handle = app
        .objects()
        .physics
        .handle(id)
        .expect("physical attachment prerequisite");
    let body = scene.character.as_ref().unwrap().world.get(handle).unwrap();
    assert!(!body.state.is_hidden());
    assert!(body.motion.is_some());
    assert_eq!(scene.server_object_motions_pending(id), Some(0));
    let presence = app.objects().presence(id).unwrap();
    let (instance, stamp, control) = (
        presence.instance,
        presence.movement_ts,
        presence.server_control_ts,
    );
    let style = MotionCommand::NON_COMBAT.to_index().unwrap();
    let first = MovementBody {
        current_style: style,
        motion_flags: motion_flags::STICK_TO_OBJECT,
        sticky_object: Some(id),
        interpreted: Some(InterpretedMotionState {
            actions: vec![MotionAction {
                command_index: MotionCommand::WAVE.to_index().unwrap(),
                stamp_and_autonomy: 1,
                speed: 1.0,
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    let second = MovementBody {
        current_style: style,
        interpreted: Some(InterpretedMotionState {
            forward_command: Some(MotionCommand::READY.to_index().unwrap()),
            ..Default::default()
        }),
        ..Default::default()
    };
    for (offset, body) in [(1, first), (2, second)] {
        let movement = MovementBuffer {
            movement_timestamp: stamp.wrapping_add(offset),
            server_control_timestamp: control,
            autonomous: false,
            body,
        };
        peer.send(
            &mut app,
            10,
            dereth_protocol::write_blob(&MovementSetObjectMovement {
                id,
                instance_sequence: instance,
                movement: MovementSetObjectMovement::encode_movement(&movement).unwrap(),
            })
            .unwrap(),
        );
    }
    assert!(app.frame());
    let scene = app.world_scene().unwrap();
    let driver = scene
        .server_object_motion_lifetime(id)
        .unwrap()
        .upgrade()
        .unwrap();
    assert!(driver.borrow().movement.interp.pending_motions.iter().any(|n| n.motion == MotionCommand::WAVE),
        "first accepted action must reach actual driver before second packet replaces Presence slot");
    assert_eq!(
        scene.server_object_sticky(id),
        None,
        "second buffer synchronously un-sticks first"
    );
    assert_eq!(
        scene.server_object_target(id),
        None,
        "second buffer synchronously removes its subscription"
    );
    let at = scene.server_object_sequence(id).unwrap().frame_number();
    let time = scene
        .character
        .as_ref()
        .unwrap()
        .world
        .get(handle)
        .unwrap()
        .update_time;
    assert!(app.frame());
    let scene = app.world_scene().unwrap();
    assert_ne!(
        scene.server_object_sequence(id).unwrap().frame_number(),
        at,
        "next legitimate physical update advances action"
    );
    let next_time = scene
        .character
        .as_ref()
        .unwrap()
        .world
        .get(handle)
        .unwrap()
        .update_time;
    assert!(
        next_time > time && next_time - time <= 2.0 * HEADLESS_STEP + 1e-6,
        "one source-throttled physical update, not one per accepted packet: {time} -> {next_time}"
    );
    assert_eq!(scene.server_object_target(id), None);
}

#[test]
fn world_view_deletion_refills_actual_inventory_before_the_producing_frame_draw() {
    use dereth_client_model::{objects::ObjectInventory, Weenie};
    use dereth_protocol::objects::ItemDeleteObject;
    use dereth_ui_screens::screens::gameplay::GamePlayScreen;
    let mut app = App::new(Config {
        headless: true,
        sound: false,
        ui: true,
        preferences_file: std::env::temp_dir().join("dere-frame-phases-not-created/prefs.ini"),
        dat_dir: dereth_dat::testing::dat_dir(),
        ..Default::default()
    })
    .unwrap();
    app.start_shell().unwrap();
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net).unwrap();
    let player = ObjectId(0x50007777);
    let id = ObjectId(0x89000009);
    app.apply_hud_events(&[
        dereth_client_net::client_session::SessionEvent::PlayerDescription(Box::default()),
    ]);
    app.probe_mut().objects_mut().world.set_player(player);
    let mut owner = Weenie::new(player);
    owner.valid = true;
    owner.pwd.items_capacity = Some(24);
    app.probe_mut()
        .objects_mut()
        .world
        .tables
        .weenies
        .insert(player, owner);
    app.probe_mut()
        .objects_mut()
        .world
        .tables
        .inventories
        .insert(
            player,
            ObjectInventory {
                container: player,
                items: vec![id],
                ..Default::default()
            },
        );
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&ItemCreateObject(object(id, player))).unwrap(),
    );
    assert!(app.frame());
    assert!(app.frame());
    let rows = |app: &App| {
        let any: &dyn std::any::Any = app.ui().unwrap().flow.current().unwrap();
        any.downcast_ref::<GamePlayScreen>()
            .unwrap()
            .inventory
            .item_list
            .as_ref()
            .unwrap()
            .slots
            .iter()
            .filter_map(|slot| slot.item)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        rows(&app),
        vec![id],
        "actual populated DAT inventory prerequisite"
    );
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&ItemDeleteObject {
            id,
            instance_sequence: 1,
        })
        .unwrap(),
    );
    assert!(app.frame());
    assert!(app.objects().world.weenie(id).is_none());
    assert!(
        rows(&app).is_empty(),
        "no stale deleted item in the producing frame's real ItemList"
    );
    // Same ID is a new lifetime, with a new parent transition. The first frame must refill
    // immediately; no old deleted-ID suppression or frame-late snapshot may hide it.
    let mut replacement = object(id, player);
    replacement.physicsdesc.timestamps.instance = 2;
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&ItemCreateObject(replacement)).unwrap(),
    );
    assert!(app.frame());
    assert_eq!(
        rows(&app),
        vec![id],
        "new lifetime's actual ItemMoved callback refills immediately"
    );
    let foreign = ObjectId(0x8900000a);
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&ItemCreateObject(object(foreign, ObjectId(0)))).unwrap(),
    );
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&ItemDeleteObject {
            id: foreign,
            instance_sequence: 1,
        })
        .unwrap(),
    );
    assert!(app.frame());
    assert_eq!(
        rows(&app),
        vec![id],
        "foreign owner's notices must not remove the player's row"
    );
    peer.send(
        &mut app,
        9,
        dereth_protocol::write_blob(&ItemServerSaysRemove { object: id }).unwrap(),
    );
    assert!(app.frame());
    assert_eq!(
        rows(&app),
        vec![id],
        "newly received UI does not leak into the old phase"
    );
    assert!(app.frame());
    assert!(
        rows(&app).is_empty(),
        "next operation uses the same real subscriber at UI entry"
    );
}

/// A create whose parent is unknown first parks its whole blob on that **parent**, never on
/// the child it would create. The parent's arrival releases that list within its own operation.
/// The released create is itself an arrival, so its own parked list must be released during
/// the same operation, not deferred to an unrelated later batch.
///
/// This runs through the production `Session`/`ObjectStream`/`App` seams, and asserts final
/// identities and attachment rather than counters.
#[test]
fn nested_parent_arrival_releases_parked_child_create_and_its_own_state_in_one_operation() {
    use dereth_protocol::objects::ItemSetState;
    use dereth_protocol::types::physicsdesc::flags;
    let mut app = app();
    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net).unwrap();
    let holder = ObjectId(0x7000_00A1);
    let child = ObjectId(0x8000_00A1);
    let held = |parent: Option<ObjectId>| {
        let mut p = ObjectCreatePayload {
            id: child,
            ..Default::default()
        };
        p.physicsdesc.bitfield |= flags::SETUP;
        p.physicsdesc.setup_id = Some(0x0200_0001);
        p.physicsdesc.timestamps.instance = 1;
        if let Some(parent) = parent {
            p.physicsdesc.bitfield |= flags::PARENT;
            p.physicsdesc.parent = Some((parent, 1));
        }
        p
    };
    let state = |value: u32, event: u16| {
        dereth_protocol::write_blob(&ItemSetState {
            id: child,
            state: value,
            timestamps: dereth_protocol::types::PhysicsEventStamp { instance: 1, event },
        })
        .unwrap()
    };
    // The child's create names a parent the client has never heard of, and a later state
    // message names the child the client has never heard of. Neither may be applied or lost.
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&ItemCreateObject(held(Some(holder)))).unwrap(),
    );
    peer.send(&mut app, 10, state(0x4000, 30));
    assert!(app.frame());
    assert!(
        app.objects().presence(child).is_none(),
        "step 1 parks on the parent, it does not create"
    );
    assert!(
        app.objects().presence(holder).is_none(),
        "parking must not invent a real parent"
    );
    assert_eq!(app.objects().physics_state(child), None);

    // The parent arrives. Its own arrival operation must release the parked child create, and
    // that create's arrival must release the child's own parked state before this frame ends.
    let mut parent = ObjectCreatePayload {
        id: holder,
        ..Default::default()
    };
    parent.physicsdesc.bitfield |= flags::SETUP;
    parent.physicsdesc.setup_id = Some(0x0200_0001);
    parent.physicsdesc.timestamps.instance = 1;
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&ItemCreateObject(parent)).unwrap(),
    );
    assert!(app.frame());
    assert!(
        app.objects().presence(holder).is_some(),
        "the parent's own create ran"
    );
    let c = app
        .objects()
        .presence(child)
        .expect("released child create ran in the same operation");
    assert_eq!(
        c.instance, 1,
        "the released blob is the original bytes, not a re-stamped copy"
    );
    assert_eq!(
        c.parent,
        Some((holder, 1)),
        "parent assignment attached the child to the arrival"
    );
    assert_eq!(
        app.objects().physics_state(child),
        Some(0x4000),
        "the child's own parked list is released inside its arrival, not in a later batch"
    );
    assert_eq!(c.state_ts, 30);
    // Successful child attachment cancels the placeholder deadline as the child leaves the world.
    assert!(app.objects().world.tables.doomed.get(child).is_none());
    assert!(!app.objects().world.tables.null_physics.contains_key(child));

    // Neither list may replay a second copy on a later frame.
    assert!(app.frame());
    assert_eq!(app.objects().physics_state(child), Some(0x4000));
    assert_eq!(
        app.objects().presence(child).unwrap().parent,
        Some((holder, 1))
    );

    // Another message after the callback reaches the object the callback created.
    peer.send(&mut app, 10, state(0x0800, 31));
    assert!(app.frame());
    assert_eq!(app.objects().physics_state(child), Some(0x0800));
    assert_eq!(app.objects().presence(child).unwrap().state_ts, 31);
}

/// The HUD's scroll delivery is a frame phase, not a consequence of this frame having had
/// network events. A received scroll line is queued immediately; the head of
/// `Hud::apply_events` hands those lines to the spew box and `ChatInterface` and pushes the three
/// once-a-frame `display_time_stamps`/clock inputs. It runs on every frame, so a frame with **no
/// link at all**, or no new packet, still delivers a line a UI request raised in the frame before
/// (a confirmation feedback bubble, for example).
#[test]
fn no_link_frame_still_delivers_scroll_lines_to_the_hud() {
    let mut app = app();
    assert!(
        app.replay_network_mut().is_none(),
        "no link at all on this App"
    );
    assert!(app.frame());
    let before = app.hud().stats.scroll_lines;
    app.probe_mut()
        .objects_mut()
        .world
        .scroll
        .add_text_to_scroll("Using the Oil with the Bow", 0x1a, false, 0);
    assert_eq!(app.objects().world.scroll.pending().len(), 1);
    assert!(app.frame());
    assert_eq!(
        app.objects().world.scroll.pending().len(),
        0,
        "a frame with no link and no packet still runs the HUD's scroll phase"
    );
    assert_eq!(
        app.hud().stats.scroll_lines,
        before + 1,
        "the line reached the HUD, exactly once"
    );
    // The next frame must not deliver a second copy of it.
    assert!(app.frame());
    assert_eq!(app.hud().stats.scroll_lines, before + 1);
}

/// Login processing's per-frame work (the link-status and rejected-datagram reports, the net-error
/// report with its no-UI exit, and the `--linger` deadline) runs before its event loop, on every
/// frame: everything in `App::process_logon_event_queue` above `for e in events` is
/// unconditional, and the function ends with that loop. A `NET_ERROR` optional header carries no
/// fragment and therefore produces no session event at all, and a `--no-ui` run still ends on it.
#[test]
fn net_error_without_any_session_event_still_ends_a_no_ui_run() {
    let mut app = app();
    let (_, net) = Peer::new();
    app.attach_replay_network(net).unwrap();
    assert!(app.frame(), "an ordinary frame keeps running");
    let mut packet = dereth_transport::OutPacket::new(dereth_transport::ProtoHeader {
        seq_id: 1,
        rec_id: 0xB,
        interval: 0x100,
        iteration: 1,
        ..Default::default()
    });
    packet
        .add_optional_header(
            dereth_transport::PacketFlags::NET_ERROR,
            dereth_transport::conn::NetErrorCode::ServerFull
                .pack()
                .to_vec(),
        )
        .unwrap();
    let raw = packet.serialize(None).unwrap();
    // Straight into ClientNetwork::feed, the seam receive_socket uses: no fragment, no queue,
    // and therefore nothing for the per-event delivery path to be triggered by.
    app.replay_network_mut().unwrap().feed(
        &raw,
        "127.0.0.1:19000".parse().unwrap(),
        LocalTime(0.0),
    );
    assert_eq!(
        app.replay_network_mut().unwrap().error(),
        Some(dereth_transport::conn::NetErrorCode::ServerFull),
        "the transport recorded the server's code"
    );
    assert!(!app.frame(), "the frame that observes it ends a no-UI run");
}
