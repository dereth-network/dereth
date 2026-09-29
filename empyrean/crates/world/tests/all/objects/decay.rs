//! ACE: Source/ACE.Server/WorldObjects/WorldObject_Decay.cs::Decay
//! Tests of decay.
//! Fixture: isolated world state and the shared area fixtures.

mod object_decay {
    use crate::support::player_world::*;

    /// `WorldObject.IsDecayable`: dynamic objects rot, unless generator-made, with a lifespan, or
    /// `TimeToRot == -1`; static (non-dynamic) guids never do.
    #[test]
    fn is_decayable_follows_aces_rules() {
        let mut h = H::new();
        lm::get_landblock(&mut h.w, lb_id(), false, false);
        h.monster(100.0, 100.0);
        item(&mut h);
        assert!(world_object_decay::is_decayable(&h.w, ITEM));

        let o = h.w.objects.get_mut(ITEM).unwrap();
        o.set_time_to_rot(Some(-1.0));
        assert!(
            !world_object_decay::is_decayable(&h.w, ITEM),
            "-1 = never rot"
        );
        let o = h.w.objects.get_mut(ITEM).unwrap();
        o.set_time_to_rot(Some(0.0));
        assert!(
            world_object_decay::is_decayable(&h.w, ITEM),
            "0 = instant rot"
        );

        let o = h.w.objects.get_mut(ITEM).unwrap();
        o.set_property(PropertyInt::Lifespan, 60);
        assert!(
            !world_object_decay::is_decayable(&h.w, ITEM),
            "a lifespan handles its own decay"
        );
        let o = h.w.objects.get_mut(ITEM).unwrap();
        o.remove_property(PropertyInt::Lifespan);
        o.wo.world_object_generators.generator = Some(MONSTER);
        assert!(
            !world_object_decay::is_decayable(&h.w, ITEM),
            "linked to a generator"
        );

        // a static guid (the landblock's own objects)
        let mut s = WorldObject::allocate(Class::GenericObject);
        s.guid = ObjectGuid::new(0x7A9B_4001);
        h.w.objects.insert(s).unwrap();
        assert!(!world_object_decay::is_decayable(
            &h.w,
            ObjectGuid::new(0x7A9B_4001)
        ));
    }

    /// `WorldObject.Decay`: the first call only sets `TimeToRot` to the default 5 minutes; each later
    /// call counts it down by the elapsed time, and at zero the object is destroyed.
    #[test]
    fn an_item_rots_after_the_default_five_minutes() {
        let mut h = H::new();
        lm::get_landblock(&mut h.w, lb_id(), false, false);
        h.monster(100.0, 100.0);
        item(&mut h);

        world_object_decay::decay(&mut h.w, ITEM, TimeSpan::from_seconds(5.0));
        assert_eq!(
            h.w.objects.get(ITEM).unwrap().time_to_rot(),
            Some(world_object_decay::DEFAULT_TIME_TO_ROT)
        );

        world_object_decay::decay(&mut h.w, ITEM, TimeSpan::from_seconds(200.0));
        assert_eq!(h.w.objects.get(ITEM).unwrap().time_to_rot(), Some(100.0));
        assert!(!world_object_decay::decay_completed(
            h.w.objects.get(ITEM).unwrap()
        ));

        world_object_decay::decay(&mut h.w, ITEM, TimeSpan::from_seconds(100.0));
        assert!(
            h.w.objects
                .get(ITEM)
                .is_none_or(|o| o.wo.world_object.is_destroyed),
            "destroyed"
        );
    }

    /// An open container does not rot away (`container.IsOpen` keeps it until closed); the countdown
    /// has still reached -2.
    #[test]
    fn an_open_container_waits() {
        let mut h = H::new();
        lm::get_landblock(&mut h.w, lb_id(), false, false);
        h.monster(100.0, 100.0);
        let mut o = WorldObject::allocate(Class::Container);
        o.guid = ITEM;
        o.biota.id = ITEM.full();
        o.set_time_to_rot(Some(1.0));
        o.set_property(empyrean_entity::enums::PropertyBool::Open, true);
        h.w.objects.insert(o).unwrap();

        world_object_decay::decay(&mut h.w, ITEM, TimeSpan::from_seconds(5.0));
        let o = h.w.objects.get(ITEM).expect("still there");
        assert_eq!(o.time_to_rot(), Some(-2.0));
        assert!(!world_object_decay::decay_completed(o));
    }
}
