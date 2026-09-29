//! ACE: Source/ACE.Server/Entity/Landblock.cs::GetObject
//! ObjectStore guid-keyed insert/get/remove and two-object borrow.
//! Fixture: explicit values and local in-memory state.

use empyrean_entity::ObjectGuid;
use empyrean_world::world_objects::world_object::WorldObject;
use empyrean_world::ObjectStore;

fn obj(g: u32) -> WorldObject {
    WorldObject {
        guid: ObjectGuid::new(g),
        ..Default::default()
    }
}

#[test]
fn insert_get_remove() {
    let mut s = ObjectStore::new();
    assert!(s.insert(obj(0x8000_0001)).is_ok());
    assert!(s.insert(obj(0x8000_0001)).is_err(), "a guid is owned once");
    assert!(s.contains(ObjectGuid::new(0x8000_0001)));
    assert_eq!(s.len(), 1);
    assert!(s.remove(ObjectGuid::new(0x8000_0001)).is_some());
    assert!(
        s.get(ObjectGuid::new(0x8000_0001)).is_none(),
        "a removed object reads as null"
    );
}

#[test]
fn get2_mut_requires_two_distinct_live_objects() {
    let (a, b) = (ObjectGuid::new(0x5000_0001), ObjectGuid::new(0x8000_0002));
    let mut s = ObjectStore::new();
    s.insert(obj(a.full())).unwrap();
    assert!(s.get2_mut(a, b).is_none(), "missing second");
    s.insert(obj(b.full())).unwrap();
    assert!(s.get2_mut(a, a).is_none(), "same guid twice");
    let (x, y) = s.get2_mut(a, b).expect("both live");
    assert_eq!((x.guid, y.guid), (a, b));
}
