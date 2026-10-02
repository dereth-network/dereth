//! Dropping an item on the classic shortcut bar: which items a shortcut accepts, and how a new
//! shortcut displaces the old ones.
use crate::int::{i32_from, u32_from};
use dereth_client_model::inventory::SplitState;
use dereth_client_model::{Notice, NoticeSink, Request, RequestSink, World};
use dereth_primitives::{ObjectId, ServerTime};
use dereth_protocol::login::{CharacterAddShortCut, CharacterRemoveShortCut, ShortCutData};

const SLOTS: usize = 9;
fn remove(world: &mut World, requests: &mut dyn RequestSink, slot: usize) {
    if world.player_system.remove_shortcut(slot) {
        requests.send(Request::RemoveShortCut(CharacterRemoveShortCut {
            index: u32_from(slot),
        }));
    }
}
fn add(
    world: &mut World,
    requests: &mut dyn RequestSink,
    item: ObjectId,
    slot: usize,
    now: ServerTime,
) {
    // Adding a shortcut first removes every other shortcut to the same object.
    for other in 0..SLOTS {
        if other != slot && world.player_system.shortcut_at(other) == Some(item) {
            remove(world, requests, other);
        }
    }
    let shortcut = ShortCutData {
        index: i32_from(slot),
        object_id: item,
        spell_id: 0,
    };
    world.player_system.add_shortcut(shortcut);
    world.player_system.mark_dirty(now);
    requests.send(Request::AddShortCut(CharacterAddShortCut { shortcut }));
}
fn refuse(notices: &mut dyn NoticeSink, text: &str) -> bool {
    notices.emit(Notice::DisplayString {
        channel: 0x1a,
        text: text.into(),
    });
    false
}
fn displaced_slot(world: &World, slot: usize, from: Option<u32>) -> Option<usize> {
    from.filter(|n| (*n as usize) < SLOTS)
        .map(|n| n as usize)
        .or_else(|| {
            (slot + 1..SLOTS)
                .chain((0..slot).rev())
                .find(|i| world.player_system.shortcut_at(*i).is_none())
        })
}

/// The source alias has already been removed at pickup when `from` is Some.
/// Returns false on refusal; requests/notices already contain the precise visible effects.
#[allow(clippy::too_many_arguments)] // the drop's world, sinks, object, slots, split and time
pub fn apply(
    world: &mut World,
    requests: &mut dyn RequestSink,
    notices: &mut dyn NoticeSink,
    object: ObjectId,
    slot: u32,
    from: Option<u32>,
    split: SplitState,
    now: ServerTime,
) -> bool {
    let slot = slot as usize;
    if slot >= SLOTS || object.0 == 0 {
        return false;
    }
    let Some(item) = world.weenie(object) else {
        return false;
    };
    let displaced = world.player_system.shortcut_at(slot);
    // Ordinary item over a container alias means put into that container, before shortcut gates.
    let container = displaced.filter(|target| {
        from.is_none()
            && world.weenie(*target).is_some_and(|dst| {
                dst.is_container()
                    && item.pwd.obj_type != 0x10
                    && (!item.is_container()
                        || (item.pwd.containers_capacity.unwrap_or(0) == 0
                            && dst.pwd.containers_capacity.unwrap_or(0) != 0))
            })
    });
    if let Some(target) = container {
        return world.attempt_to_place_in_container(
            requests, notices, object, target, target, true, 0, split, now,
        );
    }
    if split.split_size != split.max_split_size {
        return refuse(notices, "Move the split item before making a shortcut");
    }
    // Read from the object's public description: its flag bits and its item type.
    if !item.is_player() && (item.pwd.bitfield & 4 != 0 || item.pwd.obj_type & 0x10 != 0) {
        return refuse(notices, "Only items that can be picked up can go here");
    }
    if world.vendor_id().filter(|v| v.0 != 0) == item.pwd.container_id.filter(|v| v.0 != 0)
        && item.pwd.container_id.is_some_and(|v| v.0 != 0)
    {
        return refuse(notices, "Only items that can be picked up can go here");
    }
    // An item not yet owned is picked up before the shortcut changes, always into the main pack.
    if !world.is_owned_by_player(object)
        && !world.place_in_backpack(requests, notices, object, true, split, now)
    {
        return false;
    }
    if let Some(old) = displaced {
        let back = displaced_slot(world, slot, from);
        remove(world, requests, slot);
        if let Some(back) = back {
            add(world, requests, old, back, now);
        }
    }
    add(world, requests, object, slot, now);
    true
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;
    use dereth_client_model::{weenie::Weenie, RecordingRequests, RecordingSink};
    fn world() -> World {
        let mut w = World::default();
        w.player = Some(ObjectId(1));
        for id in 1..20 {
            let mut item = Weenie::new(ObjectId(id));
            item.pwd.container_id = Some(ObjectId(1));
            item.pwd.obj_type = 1;
            w.tables.weenies.insert(ObjectId(id), item);
        }
        w
    }
    fn put(w: &mut World, slot: i32, id: u32) {
        w.player_system.add_shortcut(ShortCutData {
            index: slot,
            object_id: ObjectId(id),
            spell_id: 0,
        });
    }
    #[test]
    fn rejected_pickup_preserves_the_occupied_destination() {
        let mut w = world();
        put(&mut w, 4, 3);
        w.tables
            .weenies
            .get_mut(ObjectId(2))
            .unwrap()
            .pwd
            .container_id = None;
        w.player = None;
        let mut req = RecordingRequests::default();
        let mut out = RecordingSink::default();
        assert!(!apply(
            &mut w,
            &mut req,
            &mut out,
            ObjectId(2),
            4,
            None,
            SplitState::whole_stack(1),
            ServerTime(1.)
        ));
        assert_eq!(w.player_system.shortcut_at(4), Some(ObjectId(3)));
        assert!(req.0.is_empty());
    }
    #[test]
    fn displacement_searches_nearest_left_after_all_nine_right_slots() {
        let mut w = world();
        for slot in 4..9 {
            put(&mut w, slot, slot as u32 + 3);
        }
        let mut req = RecordingRequests::default();
        let mut out = RecordingSink::default();
        assert!(apply(
            &mut w,
            &mut req,
            &mut out,
            ObjectId(2),
            4,
            None,
            SplitState::whole_stack(1),
            ServerTime(1.)
        ));
        assert_eq!(w.player_system.shortcut_at(3), Some(ObjectId(7)));
        assert_eq!(w.player_system.shortcut_at(4), Some(ObjectId(2)));
        assert_eq!(w.player_system.shortcut_at(9), None);
        assert!(
            matches!(req.0.as_slice(),[Request::RemoveShortCut(r),Request::AddShortCut(a),Request::AddShortCut(b)]
            if r.index==4 && a.shortcut.index==3 && b.shortcut.index==4)
        );
    }
    #[test]
    fn source_alias_slot_precedes_nearer_empty_slots() {
        let mut w = world();
        put(&mut w, 4, 3);
        let mut req = RecordingRequests::default();
        let mut out = RecordingSink::default();
        assert!(apply(
            &mut w,
            &mut req,
            &mut out,
            ObjectId(2),
            4,
            Some(0),
            SplitState::whole_stack(1),
            ServerTime(1.)
        ));
        assert_eq!(w.player_system.shortcut_at(0), Some(ObjectId(3)));
        assert_eq!(w.player_system.shortcut_at(5), None);
    }
    #[test]
    fn stuck_nonplayers_and_partial_stacks_never_evict_aliases() {
        let mut w = world();
        put(&mut w, 4, 3);
        w.tables.weenies.get_mut(ObjectId(2)).unwrap().pwd.bitfield = 4;
        let mut req = RecordingRequests::default();
        let mut out = RecordingSink::default();
        assert!(!apply(
            &mut w,
            &mut req,
            &mut out,
            ObjectId(2),
            4,
            None,
            SplitState::whole_stack(1),
            ServerTime(1.)
        ));
        w.tables.weenies.get_mut(ObjectId(2)).unwrap().pwd.bitfield = 0;
        assert!(!apply(
            &mut w,
            &mut req,
            &mut out,
            ObjectId(2),
            4,
            None,
            SplitState {
                split_size: 1,
                max_split_size: 2
            },
            ServerTime(1.)
        ));
        assert_eq!(w.player_system.shortcut_at(4), Some(ObjectId(3)));
        assert!(req.0.is_empty());
        assert_eq!(out.0.len(), 2);
    }
}
