//! Divergence: V467
//! Actual Launch template kits and account formulas, using the pinned pack and DAT pair.

use super::*;
use empyrean_entity::enums::{PropertyDataId, Skill};
use std::collections::BTreeMap;

pub(super) fn assert_kit(w: &World, p: &CreatedPlayer, info: &CharacterCreateInfo) {
    let heritage = usize::try_from(info.heritage.0 - 1).unwrap();
    let trained = |skill: Skill| {
        matches!(
            info.skill_advancement_classes[usize::try_from(skill.0).unwrap()],
            SkillAdvancementClass::Trained | SkillAdvancementClass::Specialized
        )
    };
    let specialized = |skill: Skill| {
        info.skill_advancement_classes[usize::try_from(skill.0).unwrap()]
            == SkillAdvancementClass::Specialized
    };
    let mut expected = BTreeMap::from([
        (166, 1),
        (273, 100),
        (1077, 1),
        (5084, 1),
        ([259, 261, 258][heritage], 1),
    ]);
    for (skill, weapons, quantity) in [
        (Skill::Axe, [517, 536, 530], 1),
        (Skill::Bow, [518, 537, 531], 1),
        (Skill::Crossbow, [521; 3], 1),
        (Skill::Dagger, [527, 523, 527], 1),
        (Skill::Mace, [520, 526, 524], 1),
        (Skill::Spear, [534, 519, 539], 1),
        (Skill::Staff, [529, 528, 525], 1),
        (Skill::Sword, [535, 533, 538], 1),
        (Skill::ThrownWeapon, [522, 522, 532], 30),
    ] {
        if trained(skill) {
            expected.insert(weapons[heritage], quantity);
        }
    }
    for (skill, id, count) in [
        (Skill::Bow, 4184, 30),
        (Skill::Crossbow, 4189, 30),
        (
            Skill::Healing,
            628,
            if specialized(Skill::Healing) { 2 } else { 1 },
        ),
        (Skill::Lockpick, 511, 1),
        (Skill::Alchemy, 4751, 1),
        (Skill::Cooking, 4761, 6),
        (Skill::Cooking, 4746, 6),
        (Skill::Fletching, 4585, 30),
        (Skill::Fletching, 4586, 30),
        (Skill::Fletching, 5339, 30),
    ] {
        if trained(skill) {
            expected.insert(id, count);
        }
    }
    if specialized(Skill::Alchemy) {
        expected.insert(2414, 3);
    }
    let mut spells = BTreeSet::new();
    for (skill, first, extra) in [
        (Skill::CreatureEnchantment, [1, 18], 678),
        (Skill::ItemEnchantment, [35, 51], 49),
        (Skill::LifeMagic, [6, 23], 53),
        (Skill::WarMagic, [27, 86], 75),
    ] {
        if trained(skill) {
            spells.extend(first);
        }
        if specialized(skill) {
            spells.insert(extra);
        }
    }
    if !spells.is_empty() {
        expected.insert([4914, 4916, 4915][heritage], 1);
    }
    let actual_spells: BTreeSet<_> = p
        .player
        .biota
        .properties_spell_book
        .iter()
        .flat_map(|book| book.keys().copied())
        .collect();
    assert_eq!(actual_spells, spells, "complete starter spell book");
    let mut actual = BTreeMap::new();
    let mut components = BTreeMap::new();
    for item in &p.inventory {
        let count = item.stack_size().unwrap_or(1);
        assert!(count > 0);
        assert_eq!(item.container_id(), Some(p.player.guid.full()));
        if let Some(max) = item.max_stack_size() {
            assert!(count <= i32::from(max));
        }
        if let Some(component) = item
            .get_property(PropertyDataId::SpellComponent)
            .filter(|id| *id != 0)
        {
            *components.entry(component).or_insert(0) += count;
        } else {
            *actual.entry(item.biota.weenie_class_id).or_insert(0) += count;
        }
        if let Some(setup) = item.get_property(PropertyDataId::Setup) {
            assert!(
                w.dats
                    .portal_dat()
                    .read_from_dat::<dereth_assets::Setup>(setup)
                    .is_some(),
                "starter {} setup {setup:08X} must decode",
                item.biota.weenie_class_id
            );
        }
    }
    assert_eq!(actual, expected, "complete starter inventory");
    assert_eq!(p.player.coin_value(), Some(100));
    let mut needed = BTreeMap::new();
    for id in spells {
        let formula = empyrean_dat::file_types::spell_table::get_spell_formula(
            w.dats.portal_dat().spell_table(),
            u32::try_from(id).unwrap(),
            "offline-fixture",
        )
        .expect("account formula");
        let mut one = BTreeMap::new();
        for component in formula.into_iter().filter(|id| *id != 0) {
            *one.entry(component).or_insert(0) += 1;
        }
        for (id, count) in one {
            let greatest = needed.entry(id).or_insert(0);
            *greatest = (*greatest).max(count);
        }
    }
    assert_eq!(
        components, needed,
        "supplies cover each granted account formula"
    );
}

fn advance(w: &mut World, player: ObjectGuid, steps: usize) {
    for _ in 0..steps {
        w.now.portal_year_ticks += 0.05;
        w.now.unix_time += 0.05;
        w.now.utc = w.now.utc.add_seconds(0.05);
        w.now.monotonic += Duration::from_millis(50);
        empyrean_world::entity::actions::delay_manager::run_actions(w);
        run_actions(w, Actor::Object(player));
    }
    assert_eq!(w.world_manager.delay_manager.act_exceptions(), 0);
    assert_eq!(
        action_queue_mut(w, Actor::Object(player))
            .unwrap()
            .act_exceptions(),
        0
    );
}

#[test]
fn a_launch_starter_casts_a_granted_spell_with_its_actual_components_and_wand() {
    use empyrean_entity::enums::{CombatMode, EquipMask};
    use empyrean_world::world_objects::{
        creature_combat, creature_equipment, player_combat, player_inventory, player_magic,
    };

    let mut w = world();
    let template = w.dats.portal_dat().char_gen().heritage_groups[&1]
        .templates
        .iter()
        .position(|t| t.normal_skills.contains(&31) || t.primary_skills.contains(&31))
        .expect("a Creature Enchantment starter template");
    let info = request(&w, 1, 1, template, 0);
    let created = create(&mut w, &info);
    assert_kit(&w, &created, &info);
    let player = created.player.guid;
    let wand = created
        .inventory
        .iter()
        .find(|o| o.biota.weenie_class_id == 4914)
        .expect("granted heritage wand")
        .guid;
    load_start(&mut w, 0);
    for item in created
        .inventory
        .into_iter()
        .chain(created.equipped_objects)
    {
        w.objects.insert(item).unwrap();
    }
    w.objects.insert(created.player).unwrap();
    w.sessions.insert(
        SessionId {
            client_id: 1,
            generation: 1,
        },
        SessionData {
            state: SessionState::WorldConnected,
            player: Some(player),
            account: Some("offline-fixture".into()),
            ..Default::default()
        },
    );
    assert!(landblock_manager::add_object(&mut w, player, false));
    player_location::on_teleport_complete(&mut w, player);
    assert!(w.objects.get(player).unwrap().spell_components_required());
    player_inventory::handle_action_get_and_wield_item(
        &mut w,
        player,
        wand.full(),
        EquipMask::Held,
    );
    advance(&mut w, player, 100);
    assert_eq!(
        creature_equipment::get_equipped_wand(&w, player),
        Some(wand)
    );
    player_combat::handle_action_change_combat_mode(&mut w, player, CombatMode::Magic, false, None);
    advance(&mut w, player, 100);
    assert_eq!(creature_combat::combat_mode(&w, player), CombatMode::Magic);
    let mana = |w: &World| {
        let o = w.objects.get(player).unwrap();
        o.mana().current(o)
    };
    let before = mana(&w);
    ThreadSafeRandom::seed(5);
    player_magic::handle_action_cast_targeted_spell(&mut w, player, player.full(), 1, None);
    assert!(
        player_magic::fields(&w, player).magic_state.is_casting,
        "the normal cast starts"
    );
    advance(&mut w, player, 300);
    assert!(
        w.objects
            .get(player)
            .unwrap()
            .biota
            .properties_enchantment_registry
            .as_ref()
            .is_some_and(|rows| rows.iter().any(|e| e.spell_id == 1)),
        "the starter buff lands"
    );
    assert!(mana(&w) < before, "the successful cast pays mana");
    assert!(
        !player_magic::is_busy(&w, player),
        "casting and recoil finish"
    );
}
