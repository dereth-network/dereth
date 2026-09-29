//! ACE: Source/ACE.Server/Managers/GuidManager.cs::NewDynamicGuid
//! NewDynamicGuid reissues recycled guids after recycleTime as ACE, but skips guids whose objects
//! are still kept or live.
//! Fixture: isolated world state.

use empyrean_common::dotnet::datetime::TimeSpan;
use empyrean_entity::ObjectGuid;
use empyrean_world::managers::guid_manager as gm;
use empyrean_world::world_objects::container;
use empyrean_world::world_objects::world_object::{self as wo, WorldObject};
use empyrean_world::World;

use super::containers::{add, obj, spawn, world, CREATURE, ITEM, STORAGE};

/// Moves the virtual clock on by `span`.
fn advance(w: &mut World, span: TimeSpan) {
    w.now.utc = w.now.utc + span;
    w.now.unix_time += span.total_seconds();
}

/// Just past `DynamicGuidAllocator.recycleTime` (exactly 360 minutes is not enough).
fn past_recycle_time() -> TimeSpan {
    TimeSpan::from_minutes(360.0) + TimeSpan::from_ticks(1)
}

fn recycled(w: &mut World) -> usize {
    let info = gm::get_dynamic_guid_debug_info(w);
    info.rsplit("recycled GUIDs available: ")
        .next()
        .and_then(|n| n.trim().parse().ok())
        .unwrap_or_else(|| panic!("{info}"))
}

/// An item destroyed while a live container still lists it (the shape ACE's
/// `Creature.GenerateTreasure` left before V291) stays in the store, destroyed, with its guid
/// recycled. When that guid comes due it is not reissued (ACE would reissue it and have
/// two objects with one guid): it goes back into the queue with the current time, the next
/// recycled guid is used instead, and it is handed out once its object has left the store.
#[test]
fn a_recycled_guid_whose_object_is_still_kept_is_not_reissued() {
    let mut w = world();
    let creature = spawn(&mut w, CREATURE);
    let corpse = spawn(&mut w, STORAGE);
    let loot = spawn(&mut w, ITEM);
    assert!(add(&mut w, creature, loot));
    assert!(
        add(&mut w, corpse, loot),
        "added without leaving the creature's Inventory"
    );

    // Destroy walks the Inventory first: the queue is [loot, creature], both at t0.
    wo::destroy(&mut w, creature, true, false);
    assert!(
        obj(&w, loot).wo.world_object.is_destroyed,
        "kept, destroyed, while the corpse lists it"
    );
    assert_eq!(recycled(&mut w), 2);

    advance(&mut w, past_recycle_time());
    // The loot's guid is due first, but its object is still in the store: skipped and re-queued;
    // the creature's guid, due too, is the one handed out.
    let first = spawn(&mut w, ITEM);
    assert_eq!(first, creature, "the next due recycled guid");
    assert_eq!(obj(&w, loot).guid, loot, "the kept object is untouched");
    assert_eq!(container::inventory_values(&w, corpse), vec![loot]);
    assert_eq!(recycled(&mut w), 1, "the loot's guid is back in the queue");

    // Re-queued at the current time, it is not due again for another 360 minutes.
    let second = spawn(&mut w, ITEM);
    assert!(
        second != loot && second.full() > loot.full(),
        "a fresh guid: {second}"
    );
    assert_eq!(recycled(&mut w), 1);

    // The corpse's Destroy drops the kept object without recycling it a second time.
    wo::destroy(&mut w, corpse, true, false);
    assert!(w.objects.get(loot).is_none());
    assert_eq!(
        recycled(&mut w),
        2,
        "the corpse's guid, and the loot's once"
    );

    advance(&mut w, past_recycle_time());
    assert_eq!(
        spawn(&mut w, ITEM),
        loot,
        "now free, handed out in queue order"
    );
    assert_eq!(spawn(&mut w, ITEM), corpse);
    assert_eq!(recycled(&mut w), 0);
}

/// Without kept objects the allocator's order is ACE's: recycled (when due), then the next id.
#[test]
fn with_nothing_kept_recycled_guids_come_back_as_in_ace() {
    let mut w = world();
    let a = spawn(&mut w, ITEM);
    let b = spawn(&mut w, ITEM);
    wo::destroy(&mut w, a, true, false);
    assert_eq!(spawn(&mut w, ITEM).full(), b.full() + 1, "not yet due");
    advance(&mut w, TimeSpan::from_minutes(360.0));
    assert_eq!(
        spawn(&mut w, ITEM).full(),
        b.full() + 2,
        "exactly 360 minutes is not enough"
    );
    advance(&mut w, TimeSpan::from_ticks(1));
    assert_eq!(spawn(&mut w, ITEM), a);
}

/// An undestroyed object under a guid the allocator did not issue is skipped without recycling:
/// its own `Destroy` recycles it later.
#[test]
fn a_live_object_under_the_next_guid_is_skipped_and_not_recycled() {
    let mut w = world();
    let next = ObjectGuid::new(gm::dynamic_current(&w));
    assert!(w
        .objects
        .insert(WorldObject {
            guid: next,
            ..Default::default()
        })
        .is_ok());
    let issued = spawn(&mut w, ITEM);
    assert_eq!(issued.full(), next.full() + 1);
    assert_eq!(recycled(&mut w), 0);
}

mod failed_creation {
    use crate::support::position_and_inventory::*;

    /// `CreateNewWorldObject(weenie)`: a weenie the factory builds nothing from (WeenieType Undef)
    /// hands its new dynamic guid back (`GuidManager.RecycleDynamicGuid`); a missing weenie takes no
    /// guid at all; a built object keeps its guid.
    #[test]
    fn a_failed_creation_recycles_its_guid() {
        let mut w = world();
        let recycled = gm::recycled_guids_total(&w);
        let next = gm::dynamic_current(&w);

        assert!(factory::create_new_world_object_by_wcid_in_world(&mut w, UNDEF).is_none());
        assert_eq!(gm::recycled_guids_total(&w), recycled + 1, "recycled");
        assert_eq!(
            gm::dynamic_current(&w),
            next + 1,
            "the guid was taken first"
        );

        assert!(factory::create_new_world_object_by_wcid_in_world(&mut w, 99_999).is_none());
        assert_eq!(
            (gm::recycled_guids_total(&w), gm::dynamic_current(&w)),
            (recycled + 1, next + 1),
            "no guid for a missing weenie"
        );

        let coin = factory::create_new_world_object_by_wcid_in_world(&mut w, COIN).expect("a coin");
        assert_eq!(coin.guid.full(), next + 1);
        assert_eq!(gm::recycled_guids_total(&w), recycled + 1);
    }
}
