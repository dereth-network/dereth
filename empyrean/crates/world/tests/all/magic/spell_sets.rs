//! ACE: Source/ACE.Server/WorldObjects/WorldObject_Set.cs::GetSpellSet
//! Tests of spell sets.
//! Fixture: isolated world state and the shared area fixtures.

mod tiers {
    use crate::support::position_and_inventory::*;

    /// `GetSpellSetAll`: every spell of every tier, in tier order, each once (a `HashSet<uint>` filled
    /// in order); an unknown set has none.
    #[test]
    fn get_spell_set_all_lists_each_tier_spell_once() {
        let w = world();
        assert_eq!(
            ids(&world_object_set::get_spell_set_all(&w, EquipmentSet(7))),
            [10, 11, 12, 13]
        );
        assert!(world_object_set::get_spell_set_all(&w, EquipmentSet(8)).is_empty());
    }

    /// `GetSpellSet`: without ItemXpStyle the level is the piece count; it is capped at the highest
    /// tier and answered from the tier at or below it (`SpellSetTiersNoGaps`), none below the first.
    /// With ItemXpStyle it is the summed ItemLevel (0 for items without one) plus `levelDiff`, cast to
    /// uint, so a negative total wraps and is capped at the highest tier.
    #[test]
    fn get_spell_set_answers_the_tier_at_or_below_the_level() {
        let mut w = world();
        let items: Vec<ObjectGuid> = (0..7).map(|_| set_item(&mut w, Some(7), None)).collect();
        let by_count =
            |w: &World, n: usize| ids(&world_object_set::get_spell_set(w, &items[..n], 0));
        assert!(by_count(&w, 0).is_empty(), "no items");
        assert!(by_count(&w, 1).is_empty(), "level 1: below the first tier");
        assert_eq!(by_count(&w, 2), [10, 11]);
        assert_eq!(by_count(&w, 3), [10, 11], "level 3: the tier at 2");
        assert_eq!(by_count(&w, 5), [12, 10], "the tier's own order");
        assert_eq!(by_count(&w, 7), [13], "capped at the highest tier, 6");

        let levelled: Vec<ObjectGuid> =
            (0..2).map(|_| set_item(&mut w, Some(7), Some(1))).collect();
        assert!(
            ids(&world_object_set::get_spell_set(&w, &levelled, 0)).is_empty(),
            "no ItemLevel: level 0"
        );
        assert_eq!(
            ids(&world_object_set::get_spell_set(&w, &levelled, 4)),
            [12, 10]
        );
        assert_eq!(
            ids(&world_object_set::get_spell_set(&w, &levelled, -1)),
            [13],
            "(uint)(-1) capped at 6"
        );

        let other = set_item(&mut w, Some(8), None);
        assert!(
            world_object_set::get_spell_set(&w, &[other, items[0]], 0).is_empty(),
            "the first item's set has no spells"
        );
    }

    /// `ItemSetContains`: true for a spell in any tier of the item's set; false for any other spell,
    /// and for an item in no set.
    #[test]
    fn item_set_contains_looks_through_every_tier() {
        let mut w = world();
        let item = set_item(&mut w, Some(7), None);
        let plain = set_item(&mut w, None, None);
        assert!(world_object_set::item_set_contains(&w, item, 13));
        assert!(world_object_set::item_set_contains(&w, item, 10));
        assert!(!world_object_set::item_set_contains(&w, item, 99));
        assert!(!world_object_set::item_set_contains(&w, plain, 10));
    }
}
