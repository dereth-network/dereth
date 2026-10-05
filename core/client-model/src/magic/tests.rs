use super::*;

/// Behaviour: none (receipt identity survives session state replacement).
#[test]
fn new_session_keeps_receipt_sequence_but_discards_pending_test_and_result() {
    let previous = MagicState {
        learned_serial: 4,
        research_serial: 7,
        research_success: Some(dereth_client_contract::research::ResearchSuccess {
            serial: 7,
            components: vec![1],
        }),
        last_learned_spell: Some((4, 10)),
        pending_research: Some(PendingResearch {
            expected_spell: Some(11),
            components: vec![2],
            updated: true,
        }),
        ..Default::default()
    };
    let mut fresh = MagicState::default();
    fresh.preserve_receipt_serials_from(&previous);
    assert_eq!((fresh.learned_serial, fresh.research_serial), (4, 7));
    assert!(fresh.pending_research.is_none());
    assert!(fresh.research_success.is_none());
    assert!(fresh.last_learned_spell.is_none());
}

/// Behaviour: world.rules.a-world-profile-sets-its-clients-rules-over-the-end-of-retails
#[test]
fn a_world_that_leaves_expiries_unannounced_writes_no_expiry_line() {
    let base = SpellBase {
        name: "Strength Self I".into(),
        description: String::new(),
        school: 4,
        icon: 0,
        category: 0,
        bitfield: 0,
        base_mana: 0,
        base_range_constant: 0.0,
        base_range_mod: 0.0,
        power: 0,
        spell_economy_mod: 0.0,
        formula_version: 0,
        component_loss: 0.0,
        meta_spell_type: 1,
        meta_spell_id: 0,
        duration: None,
        portal_lifetime: None,
        raw_comps: [0; 8],
        comp_key: 0,
        comps: Vec::new(),
        caster_effect: 0,
        target_effect: 0,
        fizzle_effect: 0,
        recovery_interval: 0.0,
        recovery_amount: 0.0,
        display_order: 0,
        non_component_target_type: 0,
        mana_mod: 0,
    };
    let mut world = World::new();
    world.magic.spell_table = Some(std::sync::Arc::new(dereth_assets::tables::SpellTable {
        id: dereth_primitives::DataId(0x0e00000e),
        spell_buckets: 1,
        spells: [(2, base)].into_iter().collect(),
        spellset_bucket_index: 0,
        spellsets: Default::default(),
    }));
    assert!(world.notify_of_enchantment_removal(2));
    assert_eq!(world.scroll.drain()[0].body, "Strength Self I has expired.");
    world.world_rules.enchantment_expiry_line = false;
    assert!(!world.notify_of_enchantment_removal(2));
    assert!(world.scroll.drain().is_empty());
}

/// Behaviour: feedback.producers.successful-casting-keeps-warning-emphasis
#[test]
fn real_targeted_cast_announces_dynamic_spell_name_as_warning_and_unknown_stays_silent() {
    use dereth_client_contract::feedback::Feedback;
    let base = SpellBase {
        name: "Novel Spell".into(),
        description: String::new(),
        school: 1,
        icon: 0,
        category: 0,
        bitfield: 0,
        base_mana: 0,
        base_range_constant: 0.0,
        base_range_mod: 0.0,
        power: 0,
        spell_economy_mod: 0.0,
        formula_version: 0,
        component_loss: 0.0,
        meta_spell_type: 2,
        meta_spell_id: 0,
        duration: None,
        portal_lifetime: None,
        raw_comps: [1, 2, 3, 4, 0x31, 0, 0, 0],
        comp_key: 0,
        comps: Vec::new(),
        caster_effect: 0,
        target_effect: 0,
        fizzle_effect: 0,
        recovery_interval: 0.0,
        recovery_amount: 0.0,
        display_order: 0,
        non_component_target_type: 0x0008_8B8F,
        mana_mod: 0,
    };
    let mut world = World::new();
    let mut target = crate::Weenie::new(ObjectId(2));
    target.pwd.obj_type = 0x10;
    target.pwd.bitfield = 0x10;
    world.player = Some(ObjectId(1));
    world
        .tables
        .weenies
        .insert(ObjectId(1), crate::Weenie::new(ObjectId(1)));
    world.tables.weenies.insert(target.id, target);
    world.selected = Some(ObjectId(2));
    world.magic.spell_table = Some(std::sync::Arc::new(dereth_assets::tables::SpellTable {
        id: dereth_primitives::DataId(0x0e00000e),
        spell_buckets: 1,
        spells: [(1, base)].into_iter().collect(),
        spellset_bucket_index: 0,
        spellsets: Default::default(),
    }));
    let mut notices = crate::RecordingSink::default();
    let mut requests = crate::RecordingRequests::default();
    assert_eq!(world.cast_spell(&mut requests, &mut notices, 999), Ok(()));
    assert!(notices.0.is_empty());
    assert_eq!(world.cast_spell(&mut requests, &mut notices, 1), Ok(()));
    assert!(notices.0.iter().any(|n| matches!(n, crate::Notice::DisplayString { text, feedback, .. } if text == "Casting Novel Spell" && *feedback == Feedback::WARNING)));
    assert!(!requests.0.is_empty());
}

/// Oracle: the client's scarab table, including the fact that 0x6F maps to 0.
#[test]
fn scarab_power_levels_match_the_documented_table() {
    for i in 1..=6u32 {
        assert_eq!(scarab_power_level(i), i);
    }
    assert_eq!(scarab_power_level(0x6E), 7);
    assert_eq!(scarab_power_level(0x70), 8);
    assert_eq!(scarab_power_level(0xC0), 9);
    assert_eq!(scarab_power_level(0xC1), 10);
    assert_eq!(
        scarab_power_level(0x6F),
        0,
        "0x6F is in InqScarabOnlyFormula's keep-list but has no power level"
    );
    assert_eq!(scarab_power_level(0), 0);
}

/// Oracle: the client's rough spell-level heuristic.
#[test]
fn the_rough_heuristic_maps_ten_powers_onto_eight_levels() {
    let levels: Vec<u32> = (1..=10).map(spell_level_by_rough_heuristic).collect();
    assert_eq!(levels, vec![1, 2, 3, 4, 5, 6, 6, 7, 7, 8]);
}

/// Oracle: formula completeness tests the **first five** slots.
#[test]
fn an_incomplete_formula_makes_a_spell_untargetable() {
    assert!(formula_is_complete(&[1, 2, 3, 4, 5]));
    assert!(
        formula_is_complete(&[1, 2, 3, 4, 5, 0, 0, 0]),
        "only the first five matter"
    );
    assert!(!formula_is_complete(&[1, 2, 3, 4, 0, 6, 7, 8]));
    assert!(!formula_is_complete(&[1, 2, 3, 4]));
}

/// Oracle: the target component is the last non-zero slot of the run from slot 5 up, read from
/// the decrypted formula.
#[test]
fn the_target_slot_is_the_last_nonzero_slot_from_five_up() {
    // 0x31 is a creature component, 0x39 an item one, 0x3B a portal one, 0x40 names nothing.
    let five = [1, 2, 3, 4, 0x39, 0, 0, 0];
    let six = [1, 2, 3, 4, 0x40, 0x3B, 0, 0];
    let seven = [1, 2, 3, 4, 0x40, 0x40, 0x31, 0];
    let eight = [1, 2, 3, 4, 0x40, 0x40, 0x40, 0x39];
    assert_eq!(formula_target_type(&five), 0x0008_8B8F, "slot 4");
    assert_eq!(formula_target_type(&six), 0x1001_0000, "slot 5");
    assert_eq!(formula_target_type(&seven), 0x10, "slot 6");
    assert_eq!(formula_target_type(&eight), 0x0008_8B8F, "slot 7");
    // A hole at slot 5 stops the walk there even with later slots filled.
    assert_eq!(
        formula_target_type(&[1, 2, 3, 4, 0x31, 0, 0x3B, 0x3B]),
        0x10
    );
    assert_eq!(
        formula_target_type(&[1, 2, 3, 0, 0x31, 0x31, 0, 0]),
        0,
        "incomplete"
    );
    // Slot 4 names nothing and nothing follows: untargeted.
    assert_eq!(formula_target_type(&[1, 2, 3, 4, 0x40, 0, 0, 0]), 0);

    // Through the spell record: the stored slots are the decrypted ones plus the key, and the
    // record's non-component target field is ignored.
    let key = 0x1234_5678u32;
    let mut raw = [0u32; 8];
    for (r, d) in raw.iter_mut().zip(seven) {
        *r = if d == 0 { 0 } else { d.wrapping_add(key) };
    }
    let mut base = SpellBase {
        name: String::new(),
        description: String::new(),
        school: 1,
        icon: 0,
        category: 0,
        bitfield: 0,
        base_mana: 0,
        base_range_constant: 0.0,
        base_range_mod: 0.0,
        power: 0,
        spell_economy_mod: 0.0,
        formula_version: 0,
        component_loss: 0.0,
        meta_spell_type: 2,
        meta_spell_id: 0,
        duration: None,
        portal_lifetime: None,
        raw_comps: raw,
        comp_key: key,
        comps: Vec::new(),
        caster_effect: 0,
        target_effect: 0,
        fizzle_effect: 0,
        recovery_interval: 0.0,
        recovery_amount: 0.0,
        display_order: 0,
        non_component_target_type: 0x0008_8B8F,
        mana_mod: 0,
    };
    assert_eq!(spell_target_type(&base), 0x10);
    base.raw_comps[6] = 0;
    assert_eq!(spell_target_type(&base), 0, "slot 5 names nothing");
}

/// Oracle: the five-school infused-magic augmentation table.
#[test]
fn the_five_infused_augmentations_map_to_the_documented_properties() {
    assert_eq!(infused_augmentation_for_school(1), Some(0x129));
    assert_eq!(infused_augmentation_for_school(2), Some(0x128));
    assert_eq!(infused_augmentation_for_school(3), Some(0x127));
    assert_eq!(infused_augmentation_for_school(4), Some(0x126));
    assert_eq!(infused_augmentation_for_school(5), Some(0x148));
    assert_eq!(infused_augmentation_for_school(0), None);
}

/// A catalogue with two scarabs and a taper, so the category split is exercised.
fn catalogue() -> ComponentCatalogue {
    ComponentCatalogue::new(
        [(1, 100), (2, 200), (3, 300)],
        [
            (
                1,
                ComponentBase {
                    name: "Lead Scarab".into(),
                    category: 0,
                    icon: 0x600_0001,
                },
            ),
            (
                2,
                ComponentBase {
                    name: "Iron Scarab".into(),
                    category: 0,
                    icon: 0x600_0002,
                },
            ),
            (
                3,
                ComponentBase {
                    name: "Red Taper".into(),
                    category: 5,
                    icon: 0x600_0003,
                },
            ),
        ],
    )
}

/// Oracle: ownership tests **presence**, never count, which is why the client sends casts the
/// server refuses. Quantity lives on the component-data row.
#[test]
fn the_component_tracker_tests_presence_not_quantity() {
    let c = catalogue();
    let mut t = ComponentTracker::new();
    assert!(!t.component_is_owned(100));
    t.add_component(&c, ObjectId(1), 100, "Lead Scarab", 0, 1);
    assert!(t.component_is_owned(100));
    assert_eq!(t.num_component(&c, 100), 1);
    // A stack of exactly one is "owned" — the whole point.
    assert_eq!(
        t.update_stack_size(&c, ObjectId(1), 100, 1),
        ComponentTrackerUpdate::None,
        "the same size is not a change"
    );
    assert!(t.component_is_owned(100));
    t.add_component(&c, ObjectId(2), 200, "Iron Scarab", 0, 50);
    assert_eq!(t.num_component(&c, 200), 50);
    t.remove_component(&c, ObjectId(2), 200, 50);
    assert!(
        !t.component_is_owned(200),
        "the last object of a class un-owns it"
    );
}

/// Removing a component drops the class **only when `numItems` reaches 0** — the
/// defect the old WCID-keyed transcription had, and the reason it could not have been right:
/// two stacks of the same component in two slots is the ordinary case.
#[test]
fn removing_one_of_two_stacks_leaves_the_component_owned() {
    let c = catalogue();
    let mut t = ComponentTracker::new();
    t.add_component(&c, ObjectId(1), 100, "Lead Scarab", 0, 40);
    t.add_component(&c, ObjectId(2), 100, "Lead Scarab", 0, 60);
    assert_eq!(t.num_component(&c, 100), 100, "one row, two objects");
    assert_eq!(t.category(0).len(), 1);
    assert_eq!(t.category(0)[0].object_count(), 2);
    t.remove_component(&c, ObjectId(1), 100, 40);
    assert!(t.component_is_owned(100), "the other stack is still held");
    assert_eq!(t.num_component(&c, 100), 60);
    t.remove_component(&c, ObjectId(2), 100, 60);
    assert!(!t.component_is_owned(100));
    assert_eq!(t.category(0).len(), 0);
}

/// Add, remove, and stack-size update all substitute **1** for a stack size of 0, and the
/// update reports its direction.
#[test]
fn a_zero_stack_size_counts_as_one_and_an_update_reports_its_direction() {
    let c = catalogue();
    let mut t = ComponentTracker::new();
    t.add_component(&c, ObjectId(1), 300, "Red Taper", 0, 0);
    assert_eq!(
        t.num_component(&c, 300),
        1,
        "an unstackable component is one item"
    );
    assert_eq!(
        t.category(5).len(),
        1,
        "and it is in the Taper bucket, not the Scarab one"
    );
    assert_eq!(
        t.update_stack_size(&c, ObjectId(1), 300, 5),
        ComponentTrackerUpdate::Add
    );
    assert_eq!(t.num_component(&c, 300), 5);
    assert_eq!(
        t.update_stack_size(&c, ObjectId(1), 300, 2),
        ComponentTrackerUpdate::Remove
    );
    assert_eq!(t.num_component(&c, 300), 2);
    assert_eq!(
        t.update_stack_size(&c, ObjectId(1), 300, 2),
        ComponentTrackerUpdate::None
    );
}

/// Adding a component keeps each category list sorted by `pwd._name` — the `strcmp`
/// walk. `SpellComponentPanel` draws the list in that order and does no sorting of its own.
#[test]
fn a_category_list_is_kept_in_name_order() {
    let c = catalogue();
    let mut t = ComponentTracker::new();
    t.add_component(&c, ObjectId(1), 100, "Lead Scarab", 0, 1);
    t.add_component(&c, ObjectId(2), 200, "Iron Scarab", 0, 1);
    let names: Vec<&str> = t.category(0).iter().map(|d| d.name.as_str()).collect();
    assert_eq!(names, vec!["Iron Scarab", "Lead Scarab"]);
}

/// The client's three arms, which is the only thing that calls the other
/// three. `Undef` (8) is where a component the SCID map does not know lands: the add
/// indexes the category lists unchecked, so it is **kept**, not dropped, and the
/// panel — which walks 0..7 — never draws it.
#[test]
fn the_update_funnel_picks_add_remove_or_stack_and_undef_is_a_real_bucket() {
    let c = catalogue();
    let mut t = ComponentTracker::new();
    let up = |t: &mut ComponentTracker, id, wcid, name, stack, owned| {
        t.update_component(&c, ObjectId(id), wcid, name, 0, stack, owned)
    };
    assert_eq!(
        up(&mut t, 7, 100, "Lead Scarab", 3, false),
        ComponentTrackerUpdate::None
    );
    assert_eq!(
        up(&mut t, 7, 100, "Lead Scarab", 3, true),
        ComponentTrackerUpdate::Add
    );
    assert_eq!(
        up(&mut t, 7, 100, "Lead Scarab", 9, true),
        ComponentTrackerUpdate::Add
    );
    assert_eq!(t.num_component(&c, 100), 9);
    assert_eq!(
        up(&mut t, 7, 100, "Lead Scarab", 9, false),
        ComponentTrackerUpdate::Remove
    );
    assert!(!t.component_is_owned(100));
    assert_eq!(t.tracked_objects(), 0);

    // A WCID the mapper does not carry: `wcid_to_scid` answers 0, `inq_spell_component_base(0)`
    // misses, and the category is `Undef`.
    assert_eq!(
        c.determine_component_category(999),
        component_category::UNDEF
    );
    assert_eq!(c.comp_name_from_wcid(999), "");
    assert_eq!(
        up(&mut t, 8, 999, "Mystery Powder", 1, true),
        ComponentTrackerUpdate::Add
    );
    assert!(t.component_is_owned(999), "kept, in the Undef bucket");
    assert_eq!(t.category(component_category::UNDEF).len(), 1);
    assert_eq!(
        t.categories().map(|(_, l)| l.len()).sum::<usize>(),
        0,
        "and never drawn"
    );
}

/// The catalogue's two hops, and the literal each is pinned against.
///
/// WCID-to-SCID lookup walks a `DualDidMapper` **backwards**, so
/// the constructor takes `(SCID, WCID)` pairs in the mapper's own direction and the lookup
/// answers the other way. A miss is **0**, not an error — the client's out parameter is left
/// at its initialiser.
#[test]
fn the_catalogue_reverses_the_dual_enum_map_and_answers_zero_on_a_miss() {
    let c = catalogue();
    assert_eq!(c.len(), 3);
    assert!(!c.is_empty());
    assert_eq!(c.wcid_to_scid(100), 1);
    assert_eq!(c.wcid_to_scid(300), 3);
    assert_eq!(
        c.wcid_to_scid(0xDEAD),
        0,
        "a miss leaves the out parameter at 0"
    );
    assert_eq!(c.comp_name_from_wcid(300), "Red Taper");
    assert_eq!(
        c.determine_component_category(300),
        component_category::TAPER
    );
    assert_eq!(component_category::TAPER, 5);
    assert!(ComponentCatalogue::default().is_empty());
    assert_eq!(
        ComponentCatalogue::default().determine_component_category(100),
        component_category::UNDEF,
        "no dats loaded is Undef, not Scarab"
    );
}

/// Oracle: target-type compatibility, arm by arm.
#[test]
fn target_type_validation_follows_the_documented_arms() {
    use crate::weenie::Weenie;
    use dereth_protocol::types::PublicWeenieDesc;
    let mut w = World::new();
    w.set_player(ObjectId(1));
    let mut me = Weenie::new(ObjectId(1));
    me.pwd = PublicWeenieDesc {
        name: "Lark".into(),
        bitfield: dereth_rules::weenie::bitfield::PLAYER,
        ..PublicWeenieDesc::default()
    };
    w.tables.weenies.insert(ObjectId(1), me);
    let mut stack = Weenie::new(ObjectId(2));
    stack.pwd = PublicWeenieDesc {
        name: "Pyreal".into(),
        stack_size: Some(5),
        obj_type: item_type::MONEY,
        bitfield: 0x10,
        ..PublicWeenieDesc::default()
    };
    w.tables.weenies.insert(ObjectId(2), stack);

    let mut out = crate::RecordingSink::default();
    let check = |out: &mut crate::RecordingSink, obj, mask| {
        w.object_compatible_with_spell_target_type(out, obj, mask, false)
    };
    // Mask 0: a target is a refusal, no target is fine.
    assert_eq!(check(&mut out, None, 0), Ok(()));
    assert_eq!(
        check(&mut out, Some(ObjectId(1)), 0),
        Err(messages::WOULD_REQUIRE_NO_TARGET.into())
    );
    // A non-zero mask with no target.
    assert_eq!(
        check(&mut out, None, item_type::CREATURE),
        Err(messages::WOULD_REQUIRE_A_TARGET.into())
    );
    // Self-cast without the gear family.
    assert_eq!(
        check(&mut out, Some(ObjectId(1)), item_type::CREATURE),
        Err(messages::CANNOT_CAST_ON_SELF.into())
    );
    // ...but the gear family exempts it.
    assert_eq!(
        check(
            &mut out,
            Some(ObjectId(1)),
            item_type::REDIRECTABLE_ITEM_ENCHANTMENT_TARGET
        ),
        Ok(())
    );
    // A stack is refused before the type test.
    assert_eq!(
        check(&mut out, Some(ObjectId(2)), item_type::MONEY),
        Err(messages::STACK_OF_ITEMS.into())
    );

    let lines: Vec<(u32, String)> = out
        .0
        .iter()
        .filter_map(|n| match n {
            crate::Notice::DisplayString { channel, text, .. } => Some((*channel, text.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        lines.len(),
        4,
        "one per refusal, none for the two successes: {lines:?}"
    );
    assert!(lines.iter().all(|(c, _)| *c == 0x1A), "{lines:?}");
    assert_eq!(
        lines.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>(),
        vec![
            "This spell would require no target",
            "This spell would require a target",
            "You cannot cast this spell upon yourself",
            "Cannot cast spell on a stack of items.",
        ]
    );

    // `quiet = true` refuses **without** the message — the client's quiet early
    // return sitting in front of each `StringInfo`. Both directions, because a probe that
    // only ever reads "no message" cannot tell suppression from an instrument that never
    // looked.
    let mut quiet = crate::RecordingSink::default();
    assert_eq!(
        w.object_compatible_with_spell_target_type(
            &mut quiet,
            Some(ObjectId(2)),
            item_type::MONEY,
            true
        ),
        Err(messages::STACK_OF_ITEMS.into())
    );
    assert!(
        quiet.0.is_empty(),
        "quiet suppresses the message, not the refusal"
    );
}

/// The refusal strings are the binarys own literals.
#[test]
fn the_refusal_strings_are_the_binarys_own_literals() {
    assert_eq!(
        messages::WOULD_REQUIRE_NO_TARGET,
        "This spell would require no target"
    );
    assert_eq!(
        messages::WOULD_REQUIRE_A_TARGET,
        "This spell would require a target"
    );
    assert_eq!(
        messages::CANNOT_CAST_ON_SELF,
        "You cannot cast this spell upon yourself"
    );
    assert_eq!(
        messages::STACK_OF_ITEMS,
        "Cannot cast spell on a stack of items."
    );
    assert_eq!(
        messages::MISSING_COMPONENTS,
        "You do not have all of this spell's components"
    );
    assert_eq!(
        messages::NEED_TARGET,
        "You must select a suitable target before casting this spell"
    );
    // `"This spell cannot be cast on %s"`, `%s` = the object's name in form 2.
    assert_eq!(
        messages::cannot_be_cast_on("a Drudge Slave"),
        "This spell cannot be cast on a Drudge Slave"
    );
    // `"Casting %hs"`, where `%hs` is the spell's narrow name.
    assert_eq!(
        messages::casting("Strength Self VI"),
        "Casting Strength Self VI"
    );
    // And the one that was wrong: no such sentence exists in the retail client.
    assert!(!messages::cannot_be_cast_on("x").contains("is not a valid target"));
}

/// Oracle: §4.3 — the augmentation **or** the spell pack selects the scarab-only formula.
#[test]
fn either_the_augmentation_or_the_pack_selects_the_scarab_only_formula() {
    use crate::qualities::{Qualities, StatKey, StatType, StatValue};
    let mut w = World::new();
    assert_eq!(
        w.get_appropriate_spell_formula(1, false),
        SpellFormulaKind::Customized,
        "no player yet"
    );
    w.set_player(ObjectId(1));
    let mut me = crate::weenie::Weenie::new(ObjectId(1));
    me.qualities = Some(Qualities::new());
    w.tables.weenies.insert(ObjectId(1), me);

    assert_eq!(
        w.get_appropriate_spell_formula(1, false),
        SpellFormulaKind::Customized
    );
    assert_eq!(
        w.get_appropriate_spell_formula(1, true),
        SpellFormulaKind::ScarabOnly,
        "owning the school's spell pack is enough"
    );
    w.player_qualities_mut()
        .unwrap()
        .set(StatKey::new(StatType::Int, 0x129), StatValue::Int(1));
    assert_eq!(
        w.get_appropriate_spell_formula(1, false),
        SpellFormulaKind::ScarabOnly,
        "AugmentationInfusedWarMagic is enough on its own"
    );
    assert_eq!(
        w.get_appropriate_spell_formula(2, false),
        SpellFormulaKind::Customized,
        "and it is per school"
    );
}

/// Oracle: §1's other enum tables.
#[test]
fn the_remaining_enums_match() {
    assert_eq!(spell_index::DAMAGE_OVER_TIME, 0x1_0000);
    assert_eq!(component_category::UNDEF, 8);
    assert_eq!(NUM_SPELLCAST_BANKS, 8);
    assert_eq!(LOWEST_TAPER_ID, 63);
    assert_eq!(NUM_TAPERS, 12);
    assert_eq!(portal_recall::LINKED_PORTAL_TWO, 5);
    assert_eq!(portal_summon::LINKED_PORTAL_TWO, 2);
    assert_eq!(MAX_DESIRED_COMP_LEVEL, 5001);
}

/// Decryption subtracts the key from each of eight nonzero component slots. It is applied in
/// place so **slot
/// positions survive**.
#[test]
fn decrypt_leaves_the_zero_slots_alone_and_keeps_positions() {
    // A key larger than a slot: the client's `-` on a `ulong` wraps, and so must this.
    let raw = [110, 0, 300, 0, 0, 0, 0, 0];
    assert_eq!(decrypt_formula(&raw, 10), [100, 0, 290, 0, 0, 0, 0, 0]);
    assert_eq!(
        decrypt_formula(&raw, 200),
        [110u32.wrapping_sub(200), 0, 100, 0, 0, 0, 0, 0]
    );
    // Zero stays zero whatever the key: that is the whole point of the `if`.
    assert_eq!(decrypt_formula(&[0; 8], 7), [0; 8]);
}

/// Pinned behavior: eight independent `if`s, i.e.
/// the **count** of non-zero slots and not the length of the leading run.
///
/// The distinction is load-bearing: `CastSpell` uses this as its loop bound while indexing
/// from 0, so on a holed formula it asks about a zero slot. The second assertion pins that.
#[test]
fn the_component_count_is_a_population_count_not_a_run_length() {
    assert_eq!(num_spell_components(&[1, 2, 3, 4, 5, 0, 0, 0]), 5);
    assert_eq!(
        num_spell_components(&[1, 0, 3, 0, 5, 0, 7, 0]),
        4,
        "a holed formula"
    );
    assert_eq!(num_spell_components(&[0; 8]), 0);
    assert_eq!(num_spell_components(&[9; 8]), 8);
}

/// Oracle: both halves — the
/// **id** it returns and the **power level** it writes through its out parameter, which is
/// the one `InqScarabOnlyFormula` switches on.
#[test]
fn the_most_powerful_power_component_returns_id_and_level() {
    // `0xC1` is level 10, the highest; `3` is level 3.
    assert_eq!(
        find_most_powerful_power_component(&[3, 0xC1, 0, 0, 0, 0, 0, 0]),
        (0xC1, 10)
    );
    assert_eq!(
        find_most_powerful_power_component(&[0xC1, 3, 0, 0, 0, 0, 0, 0]),
        (0xC1, 10)
    );
    // Nothing with a power level: the pair stays (0, 0), which is the `default` filler count.
    assert_eq!(
        find_most_powerful_power_component(&[0x6F, 0, 0, 0, 0, 0, 0, 0]),
        (0, 0)
    );
    assert_eq!(find_most_powerful_power_component(&[0; 8]), (0, 0));
    // The client's own loop bound: it walks `count` slots **from index 0**, so a power
    // component sitting past the count is never examined. Two non-zero slots -> two slots
    // walked -> the `0xC1` at index 4 is invisible.
    assert_eq!(
        find_most_powerful_power_component(&[1, 0, 0, 0, 0xC1, 0, 0, 0]),
        (1, 1)
    );
}

/// Oracle: read arm by arm.
///
/// **This is the arm the old `cast_spell` got wrong** — it took `components[..1]`.
#[test]
fn the_scarab_only_formula_is_the_power_components_plus_pyreal_filler() {
    // A level-6 scarab (id 6) and four non-power components. `switch (6) -> 4` fillers.
    let f = scarab_only_formula(&[6, 70, 71, 72, 73, 0, 0, 0]);
    assert_eq!(f, [6, 0xBC, 0xBC, 0xBC, 0xBC, 0, 0, 0]);
    assert_eq!(num_spell_components(&f), 5, "not one component, five");
    // Level 1 -> one filler; level 2 -> two; 3 and 4 and **7** -> three.
    assert_eq!(
        scarab_only_formula(&[1, 70, 71, 0, 0, 0, 0, 0]),
        [1, 0xBC, 0, 0, 0, 0, 0, 0]
    );
    assert_eq!(
        scarab_only_formula(&[2, 70, 0, 0, 0, 0, 0, 0]),
        [2, 0xBC, 0xBC, 0, 0, 0, 0, 0]
    );
    assert_eq!(
        scarab_only_formula(&[3, 70, 0, 0, 0, 0, 0, 0]),
        [3, 0xBC, 0xBC, 0xBC, 0, 0, 0, 0]
    );
    assert_eq!(
        scarab_only_formula(&[0x6E, 70, 0, 0, 0, 0, 0, 0]),
        [0x6E, 0xBC, 0xBC, 0xBC, 0, 0, 0, 0],
        "0x6E is power level 7, and 7 sits with 3 and 4 rather than with 5 and 6"
    );
    assert_eq!(
        scarab_only_formula(&[0x70, 70, 0, 0, 0, 0, 0, 0]),
        [0x70, 0xBC, 0xBC, 0xBC, 0xBC, 0, 0, 0],
        "0x70 is power level 8, and 8 sits with 5, 6, 9 and 10"
    );
    // `0x6F` is in the **keep-list** and has power level 0, so it survives and pads nothing.
    // Both halves matter: keeping it is the switch, padding nothing is the level table.
    assert_eq!(
        scarab_only_formula(&[0x6F, 70, 0, 0, 0, 0, 0, 0]),
        [0x6F, 0, 0, 0, 0, 0, 0, 0]
    );
    // The source loop **breaks** on the first zero slot, so a power component behind a hole is
    // dropped rather than packed.
    assert_eq!(scarab_only_formula(&[0, 1, 0, 0, 0, 0, 0, 0]), [0; 8]);
    // Every filler count, as a literal table, so a wrong arm cannot hide behind a symbol.
    let counts: Vec<u32> = (0..=11).map(scarab_only_filler_count).collect();
    assert_eq!(counts, vec![0, 1, 2, 3, 3, 4, 4, 3, 4, 4, 4, 0]);
    assert_eq!(SCARAB_ONLY_FILLER_SCID, 0xBC);
}

/// Pinned behavior: a walk of the side-pack list
/// comparing `pwd._wcid`, and **not** of the items list.
#[test]
fn a_spell_pack_is_looked_for_among_the_side_packs_only() {
    use crate::weenie::Weenie;
    use dereth_protocol::types::PublicWeenieDesc;
    let mut w = World::new();
    assert!(!w.magic_pack_is_owned(1234), "no player");
    w.set_player(ObjectId(1));
    w.tables
        .weenies
        .insert(ObjectId(1), Weenie::new(ObjectId(1)));
    let mut pack = Weenie::new(ObjectId(2));
    pack.pwd = PublicWeenieDesc {
        wcid: 1234,
        ..PublicWeenieDesc::default()
    };
    w.tables.weenies.insert(ObjectId(2), pack);
    let mut loose = Weenie::new(ObjectId(3));
    loose.pwd = PublicWeenieDesc {
        wcid: 1234,
        ..PublicWeenieDesc::default()
    };
    w.tables.weenies.insert(ObjectId(3), loose);

    let mut inv = crate::objects::ObjectInventory::default();
    inv.items.push(ObjectId(3));
    w.tables.inventories.insert(ObjectId(1), inv.clone());
    assert!(
        !w.magic_pack_is_owned(1234),
        "an item in the main pack is not a side pack"
    );
    inv.containers.push(ObjectId(2));
    w.tables.inventories.insert(ObjectId(1), inv);
    assert!(w.magic_pack_is_owned(1234));
    assert!(
        !w.magic_pack_is_owned(1235),
        "and it is the WCID that decides"
    );
    // `INVALID_DID` — what the school-to-WCID lookup leaves when the dat mapper is absent.
    assert!(!w.magic_pack_is_owned(0));

    // With no school map every result is `INVALID_DID`; with a map, each school uses its mapped
    // WCID. Both directions are checked so "always 0" cannot pass as "the map is empty".
    assert_eq!(w.school_of_magic_to_wcid(1), 0);
    w.magic.school_pack_wcid.insert(1, 1234);
    assert_eq!(w.school_of_magic_to_wcid(1), 1234);
    assert_eq!(w.school_of_magic_to_wcid(2), 0, "and it is per school");
}

/// A component the world met before it had the spell component table is filed under no
/// category; installing the table files it under its own, and a second install changes
/// nothing.
#[test]
fn a_component_met_before_the_table_is_filed_under_its_category_once_the_table_arrives() {
    use crate::weenie::Weenie;
    use dereth_protocol::types::PublicWeenieDesc;
    let mut w = World::new();
    w.set_player(ObjectId(1));
    w.tables
        .weenies
        .insert(ObjectId(1), Weenie::new(ObjectId(1)));
    let mut scarab = Weenie::new(ObjectId(5));
    scarab.pwd = PublicWeenieDesc {
        wcid: 100,
        name: "Lead Scarab".into(),
        obj_type: item_type::SPELL_COMPONENTS,
        stack_size: Some(3),
        container_id: Some(ObjectId(1)),
        ..PublicWeenieDesc::default()
    };
    w.tables.weenies.insert(ObjectId(5), scarab);
    let mut inv = crate::objects::ObjectInventory::default();
    inv.items.push(ObjectId(5));
    w.tables.inventories.insert(ObjectId(1), inv);

    w.update_spell_component(ObjectId(5));
    assert_eq!(w.magic.components.tracked_objects(), 1);
    assert!(
        w.magic.components.category(0).is_empty(),
        "without the table the scarab is in no category"
    );

    assert!(w.install_component_catalogue(&catalogue()));
    let scarabs = w.magic.components.category(0);
    assert_eq!(scarabs.len(), 1, "the scarab is filed with the scarabs");
    assert_eq!(scarabs[0].num_items(), 3);
    assert!(
        !w.install_component_catalogue(&catalogue()),
        "installed once"
    );
}
