//! Divergence: V467, V493
//! Starter grants through creation and its save handler, with synthetic content and formulas.

use super::*;
use empyrean_common::era::EraId;

const ITEMS: &[u32] = &[
    166, 258, 259, 261, 273, 511, 517, 518, 519, 520, 521, 522, 523, 524, 525, 526, 527, 528, 529,
    530, 531, 532, 533, 534, 535, 536, 537, 538, 539, 628, 1077, 2414, 4184, 4189, 4585, 4586,
    4746, 4751, 4761, 4914, 4915, 4916, 5084, 5339,
];
const SPELLS: &[u32] = &[1, 18, 678, 35, 51, 49, 23, 6, 53, 27, 86, 75];

pub(super) fn configure(w: &mut World, missing: Option<u32>) {
    configure_formula(w, missing, false);
}

fn configure_formula(w: &mut World, missing: Option<u32>, randomized: bool) {
    configure_capacity(w, missing, randomized, 102, 5);
}

/// The files of the Launch fixture's creation tables, with `curves` for experience.
fn launch_dats_with_curves(curves: empyrean_dat::file_types::XpTable) -> Arc<DatManager> {
    let mut cg = char_gen();
    for (id, name) in [(2, "Gharu'ndim"), (3, "Sho")] {
        let mut heritage = cg.heritage_groups[&1].clone();
        heritage.name = name.into();
        cg.heritage_groups.insert(id, heritage);
    }
    let mut skills = skill_table();
    for &id in EraId::Launch.rules().creation_skills.unwrap() {
        skills.skills.entry(id).or_insert_with(|| skill(1, 2));
    }
    let fake = dats_builder()
        .with_char_gen(cg)
        .with_skill_table(skills)
        .with_xp_table(curves);
    DatManager::initialize(Arc::new(Classic(fake))).unwrap()
}

fn configure_capacity(
    w: &mut World,
    missing: Option<u32>,
    randomized: bool,
    capacity: i32,
    burden: i32,
) {
    let mut cg = char_gen();
    for (id, name) in [(2, "Gharu'ndim"), (3, "Sho")] {
        let mut heritage = cg.heritage_groups[&1].clone();
        heritage.name = name.into();
        cg.heritage_groups.insert(id, heritage);
    }
    let mut skills = skill_table();
    for &id in EraId::Launch.rules().creation_skills.unwrap() {
        skills.skills.entry(id).or_insert_with(|| skill(1, 2));
    }
    let mut spells = sample::spell_table();
    let mut base = spells.spells[&1].clone();
    base.formula_version = u32::from(randomized);
    base.comps = if randomized {
        vec![1, 63, 2, 63, 2, 1, 63, 2]
    } else {
        vec![1, 2, 2]
    };
    base.raw_comps = [0; 8];
    for (raw, component) in base.raw_comps.iter_mut().zip(&base.comps) {
        *raw = component.wrapping_add(base.comp_key);
    }
    spells.spells = SPELLS.iter().map(|&id| (id, base.clone())).collect();
    let fake = dats_builder()
        .with_char_gen(cg)
        .with_skill_table(skills)
        .with_spell_table(spells);
    w.dats = DatManager::initialize(Arc::new(Classic(fake))).unwrap();
    let mut human = ContentWeenie::new(HUMAN, "human", WeenieType::Creature)
        .with_did(PropertyDataId::CombatTable, 0x3000_0000)
        .with_string(PropertyString::Name, "Human")
        .with_int(PropertyInt::ItemsCapacity, capacity)
        .with_int(PropertyInt::ContainersCapacity, 7);
    human.weenie_properties_create_list = [
        (273, 5, 1),
        (320, 10, 2),
        (329, 0, 1),
        (44, 0, 1),
        (300, 30, 1),
    ]
    .into_iter()
    .map(
        |(wcid, count, destination)| empyrean_content::models::world::WeeniePropertiesCreateList {
            object_id: HUMAN,
            weenie_class_id: wcid,
            stack_size: count,
            destination_type: destination,
            ..Default::default()
        },
    )
    .collect();
    let mut content = MemContent::new()
        .weenie(human)
        .weenie(clothing(HAT, EquipMask::HeadWear, CoverageMask::Head))
        .weenie(clothing(
            SHIRT,
            EquipMask::ChestWear,
            CoverageMask::UnderwearChest,
        ))
        .weenie(clothing(
            PANTS,
            EquipMask::UpperLegWear,
            CoverageMask::UnderwearUpperLegs,
        ))
        .weenie(clothing(SHOES, EquipMask::FootWear, CoverageMask::Feet));
    for &id in ITEMS {
        if Some(id) == missing {
            continue;
        }
        let kind = match id {
            166 => WeenieType::Container,
            273 => WeenieType::Coin,
            628 => WeenieType::Healer,
            522 | 532 | 4184 | 4189 | 4585 | 4586 | 4746 | 4761 | 5339 => WeenieType::Stackable,
            _ => WeenieType::Generic,
        };
        let mut item = generic(id, kind)
            .with_int(PropertyInt::Value, 1)
            .with_int(PropertyInt::EncumbranceVal, burden);
        if id == 166 {
            item = item.with_int(PropertyInt::ItemsCapacity, 24);
        }
        if [273, 522, 532, 4184, 4189, 4585, 4586, 4746, 4761, 5339].contains(&id) {
            item = item
                .with_int(PropertyInt::StackSize, 1)
                .with_int(PropertyInt::MaxStackSize, 20)
                .with_int(PropertyInt::StackUnitValue, 1)
                .with_int(PropertyInt::StackUnitEncumbrance, 0);
        }
        content = content.weenie(item);
    }
    for (wcid, component) in [(625, 1), (626, 2)]
        .into_iter()
        .chain((63..=74).map(|id| (1643 + id - 63, id)))
    {
        if Some(wcid) != missing {
            content = content.weenie(
                generic(wcid, WeenieType::SpellComponent)
                    .with_did(PropertyDataId::SpellComponent, component)
                    .with_int(PropertyInt::StackSize, 1)
                    .with_int(PropertyInt::MaxStackSize, 100),
            );
        }
    }
    for &id in SPELLS {
        if Some(id) != missing {
            content = content.spell(Spell {
                id,
                name: format!("Spell {id}"),
                ..Default::default()
            });
        }
    }
    w.content = Arc::new(content);
}

fn launch_world() -> World {
    let mut w = world();
    w.era = EraId::Launch.rules();
    configure(&mut w, None);
    w
}

fn inventory(p: &CreatedPlayer) -> BTreeMap<u32, i32> {
    let mut counts = BTreeMap::new();
    for item in &p.inventory {
        *counts.entry(item.biota.weenie_class_id).or_default() += item.stack_size().unwrap_or(1);
        assert_eq!(
            item.container_id(),
            Some(p.player.guid.full()),
            "main pack only"
        );
        if let Some(max) = item.max_stack_size() {
            assert!(item.stack_size().unwrap() <= i32::from(max));
        }
    }
    counts
}

fn known(p: &CreatedPlayer) -> Vec<i32> {
    let mut spells: Vec<_> = p
        .player
        .biota
        .properties_spell_book
        .as_ref()
        .map(|s| s.keys().copied().collect())
        .unwrap_or_default();
    spells.sort_unstable();
    spells
}

#[test]
fn basic_kit_is_unconditional_and_the_template_create_list_is_not_a_second_kit() {
    for (heritage, food) in [(1, 259), (2, 261), (3, 258)] {
        let mut w = launch_world();
        let (result, p) = create(&mut w, &request("Aldric", heritage, ABILITIES, sacs(&[])));
        assert_eq!(result, CreateResult::Success);
        assert_eq!(
            inventory(&p),
            BTreeMap::from([(166, 1), (273, 100), (1077, 1), (5084, 1), (food, 1)])
        );
        assert!(known(&p).is_empty());
        assert_eq!(
            p.equipped_objects.len(),
            4,
            "appearance clothing stays separate"
        );
        assert_eq!(p.player.coin_value(), Some(100));
    }
}

#[test]
fn weapon_grants_follow_each_original_skill_and_heritage() {
    for (skill, choices, ammo, count) in [
        (1, [517, 536, 530], None, 1),
        (2, [518, 537, 531], Some(4184), 1),
        (3, [521; 3], Some(4189), 1),
        (4, [527, 523, 527], None, 1),
        (5, [520, 526, 524], None, 1),
        (9, [534, 519, 539], None, 1),
        (10, [529, 528, 525], None, 1),
        (11, [535, 533, 538], None, 1),
        (12, [522, 522, 532], None, 30),
    ] {
        for (index, weapon) in choices.into_iter().enumerate() {
            for sac in [UNTRAINED, TRAINED, SPECIALIZED] {
                let mut w = launch_world();
                let (result, p) = create(
                    &mut w,
                    &request(
                        "Aldric",
                        u32::try_from(index + 1).unwrap(),
                        ABILITIES,
                        sacs(&[(skill, sac)]),
                    ),
                );
                assert_eq!(result, CreateResult::Success);
                let items = inventory(&p);
                assert_eq!(
                    items.get(&weapon).copied(),
                    (sac >= TRAINED).then_some(count)
                );
                if let Some(ammo) = ammo {
                    assert_eq!(items.get(&ammo).copied(), (sac >= TRAINED).then_some(30));
                }
            }
        }
    }
}

#[test]
fn craft_and_utility_specialization_keeps_quantities_and_separate_nonstackable_items() {
    for sac in [UNTRAINED, TRAINED, SPECIALIZED] {
        let mut w = launch_world();
        let (result, p) = create(
            &mut w,
            &request(
                "Aldric",
                1,
                ABILITIES,
                sacs(&[(21, sac), (23, sac), (37, sac), (38, sac), (39, sac)]),
            ),
        );
        assert_eq!(result, CreateResult::Success);
        let items = inventory(&p);
        for (id, count) in [
            (511, 1),
            (4751, 1),
            (4761, 6),
            (4746, 6),
            (4585, 30),
            (4586, 30),
            (5339, 30),
        ] {
            assert_eq!(items.get(&id).copied(), (sac >= TRAINED).then_some(count));
        }
        assert_eq!(
            items.get(&628).copied(),
            match sac {
                TRAINED => Some(1),
                SPECIALIZED => Some(2),
                _ => None,
            }
        );
        assert_eq!(items.get(&2414).copied(), (sac == SPECIALIZED).then_some(3));
    }
}

#[test]
fn each_magic_school_grants_two_spells_and_a_specialized_third() {
    for (skill, first, extra) in [
        (31, [1, 18], 678),
        (32, [35, 51], 49),
        (33, [6, 23], 53),
        (34, [27, 86], 75),
    ] {
        for sac in [UNTRAINED, TRAINED, SPECIALIZED] {
            let mut w = launch_world();
            let (result, p) = create(
                &mut w,
                &request("Aldric", 1, ABILITIES, sacs(&[(skill, sac)])),
            );
            assert_eq!(result, CreateResult::Success);
            let mut expected = if sac >= TRAINED {
                first.to_vec()
            } else {
                Vec::new()
            };
            if sac == SPECIALIZED {
                expected.push(extra);
            }
            expected.sort_unstable();
            assert_eq!(known(&p), expected);
        }
    }
}

#[test]
fn shared_spell_supplies_use_maximum_formula_multiplicity_and_one_heritage_wand() {
    for (heritage, wand) in [(1, 4914), (2, 4916), (3, 4915)] {
        let mut w = launch_world();
        let (result, p) = create(
            &mut w,
            &request(
                "Aldric",
                heritage,
                ABILITIES,
                sacs(&[
                    (31, SPECIALIZED),
                    (32, SPECIALIZED),
                    (33, SPECIALIZED),
                    (34, SPECIALIZED),
                ]),
            ),
        );
        assert_eq!(result, CreateResult::Success);
        let items = inventory(&p);
        assert_eq!(items.get(&wand), Some(&1));
        assert_eq!(items.get(&625), Some(&1));
        assert_eq!(
            items.get(&626),
            Some(&2),
            "one spell repeats this component; twelve spells do not multiply it"
        );
        assert_eq!(known(&p).len(), 12);
    }
}

#[test]
fn missing_starter_content_refuses_creation_without_saving_a_partial_character() {
    for missing in [1077, 626, 27] {
        let mut w = launch_world();
        configure(&mut w, Some(missing));
        let message = request("Aldric", 1, ABILITIES, sacs(&[(34, TRAINED)]));
        assert_eq!(
            create(&mut w, &message).0,
            CreateResult::StarterProvisioningFailed
        );
        assert_eq!(send(&mut w, &message).codes(), [Cgvr::Corrupt]);
        assert!(characters(&w).is_empty());
    }
}

#[test]
fn successful_creation_saves_the_starter_inventory_and_spells() {
    let mut w = launch_world();
    let message = request(
        "Aldric",
        1,
        ABILITIES,
        sacs(&[(34, SPECIALIZED), (21, SPECIALIZED)]),
    );
    assert_eq!(send(&mut w, &message).codes(), [Cgvr::Ok]);
    let b = saved_biota(&w, FIRST_PLAYER);
    let spells = b.properties_spell_book.as_ref().unwrap();
    for spell in [27, 75, 86] {
        assert!(spells.contains_key(&spell));
    }
    let (items, _) = possessions(&w, FIRST_PLAYER);
    assert_eq!(items.iter().filter(|b| b.weenie_class_id == 628).count(), 2);
    assert!(items.iter().any(|b| b.weenie_class_id == 1077));
}

#[derive(Debug)]
struct Classic(FakeDats);
impl empyrean_dat::DatSource for Classic {
    fn has_database(&self, db: empyrean_dat::DatDatabaseType) -> bool {
        self.0.has_database(db)
    }
    fn file_ids(&self, db: empyrean_dat::DatDatabaseType) -> Vec<u32> {
        self.0.file_ids(db)
    }
    fn contains(&self, db: empyrean_dat::DatDatabaseType, id: u32) -> bool {
        self.0.contains(db, id)
    }
    fn read(&self, db: empyrean_dat::DatDatabaseType, id: u32) -> Option<Vec<u8>> {
        self.0.read(db, id)
    }
    fn file_iteration(&self, db: empyrean_dat::DatDatabaseType, id: u32) -> Option<u32> {
        self.0.file_iteration(db, id)
    }
    fn decoded(
        &self,
        db: empyrean_dat::DatDatabaseType,
        id: u32,
    ) -> Option<empyrean_dat::database::CachedObject> {
        self.0.decoded(db, id)
    }
    fn describe(&self, db: empyrean_dat::DatDatabaseType) -> String {
        self.0.describe(db)
    }
    fn container_era(&self) -> dereth_primitives::ContainerEra {
        dereth_primitives::ContainerEra::Classic
    }
}

#[test]
fn formula_supplies_use_the_creating_account_without_a_logged_in_player() {
    for (name, tapers) in [("acct", [73, 66, 71]), ("second", [73, 72, 71])] {
        let mut w = launch_world();
        configure_formula(&mut w, None, true);
        let account_id = if name == "acct" {
            ACCOUNT_ID
        } else {
            w.auth
                .lock()
                .create_account(
                    name,
                    "pw",
                    AccessLevel::Player,
                    std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
                )
                .unwrap()
                .account_id
        };
        let request = request("Aldric", 1, ABILITIES, sacs(&[(34, TRAINED)]));
        let human = w.content.get_cached_weenie_by_class_name("human").unwrap();
        let guid = guid_manager::new_player_guid(&mut w);
        let (result, p) = player_factory::create(
            &mut w,
            &info_of(&request),
            human,
            guid,
            account_id,
            WeenieType::Creature,
        );
        assert_eq!(result, CreateResult::Success);
        let items = inventory(&p);
        assert_eq!(items.get(&625), Some(&2));
        assert_eq!(items.get(&626), Some(&3));
        for scid in 63..=74 {
            assert_eq!(
                items.get(&(1643 + scid - 63)).copied(),
                tapers.contains(&scid).then_some(1)
            );
        }
    }
}

#[test]
fn every_permitted_skill_choice_preserves_the_basic_kit() {
    for &id in EraId::Launch.rules().creation_skills.unwrap() {
        for sac in [TRAINED, SPECIALIZED] {
            let mut w = launch_world();
            let (result, p) = create(
                &mut w,
                &request(
                    "Aldric",
                    1,
                    ABILITIES,
                    sacs(&[(usize::try_from(id).unwrap(), sac)]),
                ),
            );
            assert_eq!(
                result,
                CreateResult::Success,
                "skill {id}, advancement {sac}"
            );
            let items = inventory(&p);
            assert_eq!(items.get(&273), Some(&100));
            assert_eq!(items.get(&1077), Some(&1));
        }
    }
}

#[test]
fn creation_grants_ignore_burden_but_refuse_insufficient_main_pack_capacity() {
    let mut w = launch_world();
    configure_capacity(&mut w, None, false, 102, 100_000);
    assert_eq!(
        create(&mut w, &request("Aldric", 1, ABILITIES, sacs(&[]))).0,
        CreateResult::Success
    );
    let mut w = launch_world();
    configure_capacity(&mut w, None, false, 1, 5);
    let message = request("Aldric", 1, ABILITIES, sacs(&[]));
    assert_eq!(
        create(&mut w, &message).0,
        CreateResult::StarterProvisioningFailed
    );
    assert_eq!(send(&mut w, &message).codes(), [Cgvr::Corrupt]);
    assert!(characters(&w).is_empty());
}

#[test]
fn missing_empty_or_malformed_dat_formulas_refuse_without_saving() {
    for mode in 0..3 {
        let mut w = launch_world();
        let mut spells = w.dats.portal_dat().spell_table().clone();
        if mode == 0 {
            spells.spells.remove(&27);
        } else {
            let spell = spells.spells.get_mut(&27).unwrap();
            spell.raw_comps = [0; 8];
            if mode == 2 {
                spell.formula_version = 1;
                spell.raw_comps[0] = spell.comp_key.wrapping_add(1);
            }
        }
        let fake = dats_builder()
            .with_char_gen(w.dats.portal_dat().char_gen().clone())
            .with_skill_table(w.dats.portal_dat().skill_table().clone())
            .with_spell_table(spells);
        w.dats = DatManager::initialize(Arc::new(Classic(fake))).unwrap();
        let message = request("Aldric", 1, ABILITIES, sacs(&[(34, TRAINED)]));
        assert_eq!(
            create(&mut w, &message).0,
            CreateResult::StarterProvisioningFailed
        );
        assert_eq!(send(&mut w, &message).codes(), [Cgvr::Corrupt]);
        assert!(characters(&w).is_empty());
    }
}

/// The 1999 curves' first ranks, as the Launch files hold them.
fn launch_curves() -> empyrean_dat::file_types::XpTable {
    empyrean_dat::file_types::XpTable {
        trained_xp: vec![0, 58, 138, 243, 372, 526, 704],
        specialized_xp: vec![0, 23, 56, 97, 149, 211, 283, 365, 457, 559, 671, 795],
        ..sample::xp_table()
    }
}

/// A skill trained and a skill specialized at creation start at +5 and +10 as free experience on
/// Launch, with nothing in the initial level, and the next rank costs that rank's price. At the end
/// of retail a specialized skill keeps its 10 initial ranks.
#[test]
fn a_launch_creation_bonus_is_experience_worth_its_ranks() {
    use empyrean_entity::enums::Skill;
    use empyrean_world::world_objects::player_skills;
    let state = |w: &mut World, era: EraId| {
        w.era = era.rules();
        let (result, mut p) = create(
            w,
            &request(
                "Aldric",
                1,
                ABILITIES,
                sacs(&[(21, TRAINED), (23, SPECIALIZED)]),
            ),
        );
        assert_eq!(result, CreateResult::Success);
        let mut read = |id: i32| {
            let s = p
                .player
                .biota
                .properties_skill
                .as_ref()
                .unwrap()
                .get(&Skill(id))
                .unwrap();
            let seen = (s.pp, s.level_from_pp, s.init_level);
            let cs = p
                .player
                .get_creature_skill(Skill(id), false)
                .expect("trained at creation");
            let next = player_skills::get_xp_to_next_rank(w, &p.player, cs);
            (seen, next)
        };
        (read(21), read(23))
    };
    let mut w = launch_world();
    w.dats = launch_dats_with_curves(launch_curves());
    assert_eq!(
        state(&mut w, EraId::Launch),
        (((526, 5, 0), Some(178)), ((671, 10, 0), Some(124)))
    );
    let mut w = launch_world();
    w.dats = launch_dats_with_curves(launch_curves());
    assert_eq!(
        state(&mut w, EraId::Eor),
        (((526, 5, 0), Some(178)), ((0, 0, 10), Some(23)))
    );
}
