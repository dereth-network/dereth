use super::*;

/// Oracle: the recovered world-object behavior §4's bucket-count table.
#[test]
fn the_tables_have_the_retail_bucket_counts() {
    let t = ObjectTables::new();
    assert_eq!(t.weenies.num_buckets(), 0x80);
    assert_eq!(t.null_weenies.num_buckets(), 0x10);
    assert_eq!(t.physics.num_buckets(), 0x80);
    assert_eq!(t.null_physics.num_buckets(), 0x10);
    assert_eq!(t.inventories.num_buckets(), 0x20);
    assert_eq!(t.lost_cells.num_buckets(), 0x80);
    assert_eq!(t.doomed.num_buckets(), 0x40);
}

/// Oracle: §13 — the three constants are observable and must be exact.
#[test]
fn the_three_observable_constants_are_exact() {
    assert_eq!(DESTRUCTION_DELAY, 25.0);
    assert_eq!(FORCE_OBJDESC_INTERVAL, 20.0);
    assert_eq!(VISIBLE_REBUILD_INTERVAL, 1.0);
    // The re-check tolerance is an f32 2e-4 widened to double, not the decimal 2e-4.
    assert_eq!(DESTRUCTION_RECHECK_EPSILON, f64::from(2e-4f32));
}

/// Oracle: the inventory placement list — a new placement is inserted at the head and the list
/// kept in clothing-priority
/// order, because the paperdoll draw order depends on it.
#[test]
fn placements_are_kept_in_priority_order() {
    let mut inv = ObjectInventory::new(ObjectId(1));
    inv.set_placement(ObjectId(10), 0x1, 5);
    inv.set_placement(ObjectId(11), 0x2, 9);
    inv.set_placement(ObjectId(12), 0x4, 1);
    assert_eq!(
        inv.placements.iter().map(|p| p.iid).collect::<Vec<_>>(),
        vec![ObjectId(11), ObjectId(10), ObjectId(12)]
    );
    assert_eq!(inv.location_on_object(ObjectId(11)), 0x2);
    // loc 0 removes.
    inv.set_placement(ObjectId(11), 0, 9);
    assert_eq!(inv.location_on_object(ObjectId(11)), 0);
    assert_eq!(inv.placements.len(), 2);
}

#[test]
fn content_lists_are_ordered_and_split_by_kind() {
    let mut inv = ObjectInventory::new(ObjectId(1));
    inv.add_content(ObjectId(10), false, 0);
    inv.add_content(ObjectId(11), false, 0);
    inv.add_content(ObjectId(12), true, 0);
    assert_eq!(inv.items, vec![ObjectId(11), ObjectId(10)]);
    assert_eq!(inv.containers, vec![ObjectId(12)]);
    assert_eq!(inv.place_of(ObjectId(10)), Some(1));
    assert!(inv.remove_content(ObjectId(11)));
    assert_eq!(inv.items, vec![ObjectId(10)]);
}

/// Static ps is bit zero and is not hidden ps.
#[test]
fn static_ps_is_bit_zero_and_is_not_hidden_ps() {
    assert_eq!(STATIC_PS, 1, "PhysicsState::STATIC_PS");
    const HIDDEN_PS: u32 = 16384;
    assert_eq!(HIDDEN_PS, 0x4000);
    assert_ne!(
        STATIC_PS, HIDDEN_PS,
        "they are different bits, which is the whole finding"
    );

    // An object the server has *hidden* is not static, and `update_visible_object_list`
    // therefore keeps it in the visible list -- which is the behaviour the old name denied.
    let hidden = PhysicsPresence {
        state: HIDDEN_PS,
        ..PhysicsPresence::default()
    };
    assert!(!hidden.is_static());
    let stat = PhysicsPresence {
        state: STATIC_PS,
        ..PhysicsPresence::default()
    };
    assert!(stat.is_static());
    // ...and the mask is a mask: a word carrying both answers the STATIC question.
    let both = PhysicsPresence {
        state: STATIC_PS | HIDDEN_PS,
        ..PhysicsPresence::default()
    };
    assert!(both.is_static());
}

/// The state word carries the bits select next reads.
#[test]
fn the_state_word_carries_the_bits_select_next_reads() {
    const CLOAKED_PS: u32 = 0x0010_0000;
    const REPORT_COLLISIONS_AS_ENVIRONMENT_PS: u32 = 0x0020_0000;
    assert_eq!(CLOAKED_PS, 1_048_576);
    assert_eq!(REPORT_COLLISIONS_AS_ENVIRONMENT_PS, 2_097_152);
    let p = PhysicsPresence {
        state: CLOAKED_PS | REPORT_COLLISIONS_AS_ENVIRONMENT_PS,
        ..PhysicsPresence::default()
    };
    assert_ne!(p.state & CLOAKED_PS, 0);
    assert_ne!(p.state & REPORT_COLLISIONS_AS_ENVIRONMENT_PS, 0);
    assert!(!p.is_static(), "neither of them is bit 0");
}
