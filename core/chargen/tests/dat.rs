//! Character creation against the connected world's decoded tables.
use dereth_assets::motion::ClothingTable;
use dereth_assets::{CharGen, Decode, SkillTable};
use dereth_chargen::{
    Attr, CharGenRng, CharGenState, CreationEntry, CreationPolicy, CreationRandom, CreationTables,
    SkillAdvancementClass,
};
use dereth_dat::{ContainerEra, RetailDatStore};
use dereth_primitives::DataId;
use std::{collections::BTreeMap, rc::Rc};

fn tables(old: bool) -> CreationTables {
    let store = if old {
        dereth_dat::testing::open_classic_store_or_fail()
    } else {
        RetailDatStore::open_dir(&dereth_dat::testing::dat_dir()).unwrap()
    };
    let era = if old {
        ContainerEra::Classic
    } else {
        ContainerEra::Modern
    };
    let decode = |id| store.read_portal(DataId(id)).unwrap();
    let chargen =
        CharGen::decode_payload_in(era, DataId(0x0e00_0002), &decode(0x0e00_0002)).unwrap();
    let skills =
        SkillTable::decode_payload_in(era, DataId(0x0e00_0004), &decode(0x0e00_0004)).unwrap();
    let mut clothing = BTreeMap::new();
    for sex in chargen
        .heritage_groups
        .values()
        .flat_map(|h| h.sexes.values())
    {
        for gear in sex
            .headgear
            .iter()
            .chain(&sex.shirts)
            .chain(&sex.pants)
            .chain(&sex.footwear)
        {
            clothing.entry(gear.clothing_table).or_insert_with(|| {
                ClothingTable::decode_payload_in(
                    era,
                    gear.clothing_table,
                    &decode(gear.clothing_table.0),
                )
                .unwrap()
            });
        }
    }
    CreationTables {
        chargen,
        skills,
        clothing: Rc::new(clothing),
    }
}

/// Behaviour: chargen.random.classic-entry-keeps-nested-draw-order
#[test]
fn classic_entry_keeps_nested_draws_and_summary_only_completes_missing_choices() {
    let t = tables(true);
    assert_eq!(t.heritage_keys(), [1, 2, 3]);
    assert_eq!(t.sex_keys(1), [2, 1]);
    for (entry, next) in [(CreationEntry::Normal, 2995), (CreationEntry::Quick, 19895)] {
        let mut state = CharGenState::with_policy(CreationPolicy::Classic);
        state.begin_creation(&t, entry, false);
        assert_eq!(
            (
                state.heritage_group,
                state.gender,
                state.template,
                state.start_area
            ),
            if entry == CreationEntry::Normal {
                (1, 2, 4, 0)
            } else {
                (1, 2, 1, 1)
            }
        );
        assert_eq!(
            [
                state.eyes_strip,
                state.nose_strip,
                state.mouth_strip,
                state.hair_color,
                state.eye_color,
                state.hair_style
            ],
            [10, 10, 9, 2, 1, 3]
        );
        assert_eq!(
            Attr::BALANCE_ORDER.map(|a| state.get(a)),
            if entry == CreationEntry::Normal {
                [50, 40, 10, 30, 100, 100]
            } else {
                [40, 30, 100, 100, 50, 10]
            }
        );
        assert_eq!(state.rng.crt.clone().next_u16(), next);
        assert_eq!(
            state.rng.ran2.clone().next_f64(),
            CharGenRng::default().ran2.next_f64(),
            "Classic uses only the CRT stream"
        );
        if entry == CreationEntry::Normal {
            assert_eq!(
                [
                    state.headgear_style,
                    state.shirt_style,
                    state.trousers_style,
                    state.footwear_style
                ],
                [-1; 4]
            );
            assert_eq!(state.frozen, [false; 3]);
        } else {
            assert_eq!(
                [
                    state.headgear_style,
                    state.shirt_style,
                    state.trousers_style,
                    state.footwear_style
                ],
                [1, 0, 0, 0]
            );
            let r = state.get_char_gen_result();
            assert_eq!(
                [
                    r.headgear_color,
                    r.shirt_color,
                    r.trousers_color,
                    r.footwear_color
                ],
                [6, 2, 3, 3]
            );
            assert_eq!(state.frozen, [true; 3]);
        }
        let fixed = (
            state.heritage_group,
            state.gender,
            state.template,
            state.start_area,
        );
        state.prepare_summary(&t);
        assert_eq!(
            (
                state.heritage_group,
                state.gender,
                state.template,
                state.start_area
            ),
            fixed
        );
        assert!(state.shirt_style >= 0);
        assert_eq!(state.frozen, [true; 3]);
        let result = state.get_char_gen_result();
        let next = state.rng.crt.clone().next_u16();
        state.prepare_summary(&t);
        assert_eq!(state.get_char_gen_result(), result);
        assert_eq!(
            state.rng.crt.next_u16(),
            next,
            "a complete summary consumes no draws"
        );
    }
}

/// Behaviour: chargen.random.the-opening-roll-is-the-seeds-own-and-two-clients-roll-the-same-character
#[test]
fn modern_entry_preserves_existing_world_rolls_and_final_template_presentation() {
    for old in [false, true] {
        let t = tables(old);
        let mut direct = CharGenState::default();
        direct.clothing = Rc::clone(&t.clothing);
        direct.randomize_character(&t.chargen, &t.skills, false);
        let mut policy = CharGenState::with_policy(CreationPolicy::Modern);
        policy.begin_creation(&t, CreationEntry::Normal, false);
        assert_eq!(policy.get_char_gen_result(), direct.get_char_gen_result());
        assert_eq!(policy.rng.crt.next_u16(), direct.rng.crt.next_u16());
        assert_eq!(policy.rng.ran2.next_f64(), direct.rng.ran2.next_f64());
        if !old {
            for h in t.chargen.heritage_groups.values() {
                for &sex in &h.sex_order {
                    for (i, template) in h.templates.iter().enumerate() {
                        let p = h.template_presentation(sex, i).unwrap();
                        assert_eq!(p.icon, template.icon);
                        assert!(p.description.is_none());
                    }
                }
            }
        }
    }
}

/// Behaviour: chargen.tables.world-keys-and-costs-remain-authoritative
#[test]
fn classic_choices_keep_world_keys_and_templates_keep_unavailable_skills_inactive() {
    let mut t = tables(true);
    let mut h = t.chargen.heritage_groups.remove(&1).unwrap();
    let first = h.sexes.remove(&2).unwrap();
    let second = h.sexes.remove(&1).unwrap();
    h.sexes = BTreeMap::from([(9, first), (5, second)]);
    h.sex_order = vec![9, 5];
    h.template_presentations.clear();
    h.templates[1].normal_skills = vec![1, 2, 54];
    h.templates[1].primary_skills = vec![1, 2, 54];
    h.skills.push((2, -1, -1));
    t.chargen.heritage_groups = BTreeMap::from([(42, h)]);
    t.chargen.heritage_order = vec![42];
    t.skills.skills.remove(&1);
    let mut state = CharGenState::with_policy(CreationPolicy::Classic);
    state.begin_creation(&t, CreationEntry::Normal, false);
    assert_eq!(state.heritage_group, 42);
    assert!(matches!(state.gender, 9 | 5));
    assert!(!state.choose_heritage(&t, 1));
    assert!(!state.choose_gender(&t, 2));
    assert!(state.choose_gender(&t, 9));
    assert!(state.choose_template(&t, 1));
    for skill in [1, 2, 54] {
        assert_eq!(state.skill_levels[skill], SkillAdvancementClass::Inactive);
    }
    assert!(state.choose_template(&t, -1));
    assert_eq!(state.template, 0);
    assert!(!state.choose_template(&t, -2));
    let result = state.get_char_gen_result();
    assert_eq!((result.heritage_group, result.gender), (42, 9));
    assert_eq!(result.skill_advancement_classes.len(), 55);
}

/// Behaviour: chargen.random.classic-entry-keeps-nested-draw-order
#[test]
fn classic_page_random_attributes_preserve_locks_and_skills_use_the_world_range() {
    let t = tables(true);
    let mut state = CharGenState::with_policy(CreationPolicy::Classic);
    state.begin_creation(&t, CreationEntry::Normal, false);
    state.rng = CharGenRng::default();
    state.randomize_page(&t, CreationRandom::Attributes, false);
    assert_eq!(
        Attr::BALANCE_ORDER.map(|a| state.get(a)),
        [25, 80, 43, 25, 67, 90]
    );
    assert_eq!(state.rng.crt.clone().next_u16(), 24464);
    state.set_locked(Attr::Strength, true);
    let locked = state.get(Attr::Strength);
    state.randomize_page(&t, CreationRandom::Attributes, false);
    assert_eq!(state.get(Attr::Strength), locked);
    assert!(Attr::ALL.iter().map(|a| state.get(*a)).sum::<i32>() <= 330);
    state.rng = CharGenRng::default();
    state.randomize_page(&t, CreationRandom::Skills, false);
    assert_eq!(state.remaining_skill_credits, 0);
    assert_eq!(state.rng.crt.clone().next_u16(), 3902);
    assert_eq!(
        state
            .skill_levels
            .iter()
            .enumerate()
            .filter_map(|(i, s)| (*s == SkillAdvancementClass::Specialized).then_some(i))
            .collect::<Vec<_>>(),
        [19, 23, 33, 36]
    );
    assert_eq!(state.skill_levels.len(), 55);
    assert!(state.skill_levels[41..]
        .iter()
        .all(|s| *s == SkillAdvancementClass::Inactive));
}

/// Behaviour: chargen.skills.changes-refund-prior-cost-and-refuse-unaffordable-classes
#[test]
fn skill_changes_refund_prior_cost_and_refuse_unaffordable_or_unavailable_classes() {
    for old in [true, false] {
        let t = tables(old);
        let mut state = CharGenState::default();
        state.set_heritage_group(&t.chargen, &t.skills, 1);
        state.set_gender(&t.chargen, 1);
        state.reset_skill_levels(&t.chargen, &t.skills);
        let (id, trained, specialized) = t
            .skills
            .skills
            .keys()
            .find_map(|&id| {
                let (trained, specialized) =
                    CharGenState::skill_costs(&t.chargen, &t.skills, 1, id);
                (trained > 0 && specialized > trained && specialized <= 50).then_some((
                    id,
                    trained,
                    specialized,
                ))
            })
            .unwrap();
        state.total_skill_credits = trained;
        state.update_remaining_skill_credits(&t.chargen, &t.skills);
        state.set_skill_level(
            &t.chargen,
            &t.skills,
            id,
            SkillAdvancementClass::Specialized,
        );
        assert_eq!(state.skill_level(id), SkillAdvancementClass::Untrained);
        assert_eq!(state.remaining_skill_credits, trained);
        state.set_skill_level(&t.chargen, &t.skills, id, SkillAdvancementClass::Trained);
        assert_eq!(state.remaining_skill_credits, 0);
        state.set_skill_level(
            &t.chargen,
            &t.skills,
            id,
            SkillAdvancementClass::Specialized,
        );
        assert_eq!(state.skill_level(id), SkillAdvancementClass::Trained);
        assert_eq!(state.remaining_skill_credits, 0);
        state.total_skill_credits = specialized;
        state.update_remaining_skill_credits(&t.chargen, &t.skills);
        state.set_skill_level(
            &t.chargen,
            &t.skills,
            id,
            SkillAdvancementClass::Specialized,
        );
        assert_eq!(state.skill_level(id), SkillAdvancementClass::Specialized);
        assert_eq!(state.remaining_skill_credits, 0);
        state.set_skill_level(&t.chargen, &t.skills, id, SkillAdvancementClass::Untrained);
        assert_eq!(state.remaining_skill_credits, specialized);
        state.set_skill_level(&t.chargen, &t.skills, id, SkillAdvancementClass::Inactive);
        assert_eq!(state.skill_level(id), SkillAdvancementClass::Untrained);
    }
}

/// Behaviour: chargen.random.classic-entry-keeps-nested-draw-order
#[test]
fn classic_profession_random_applies_twice_and_explicit_custom_applies_row_zero() {
    let t = tables(true);
    let mut state = CharGenState::with_policy(CreationPolicy::Classic);
    state.begin_creation(&t, CreationEntry::Normal, false);
    state.rng = CharGenRng::default();
    state.randomize_page(&t, CreationRandom::Template, false);
    assert_eq!(state.template, 1);
    assert_eq!(
        state.rng.crt.next_u16(),
        26500,
        "selection then two singleton profile draws"
    );
    state.set_attribute(Attr::Strength, 100);
    state.rng = CharGenRng::default();
    assert!(state.choose_template(&t, -1));
    assert_eq!(state.template, 0);
    assert_ne!(
        state.get(Attr::Strength),
        100,
        "Custom replaces the edited build"
    );
    assert_eq!(
        state.rng.crt.next_u16(),
        18467,
        "explicit Custom applies its singleton profile"
    );
    state.set_attribute(Attr::Strength, 100);
    assert!(state.choose_template(&t, 0));
    assert_ne!(
        state.get(Attr::Strength),
        100,
        "Custom replaces the edited build"
    );
}

/// Behaviour: chargen.palette.samples-follow-entry-layout
#[test]
fn color_choices_sample_the_decoded_palette_layout() {
    use dereth_assets::material::{Palette, PaletteSet};
    use dereth_chargen::palette::{PaletteLayout, PaletteSample};
    use std::collections::BTreeSet;
    assert_eq!(PaletteLayout::from_entry_count(0), None);
    assert_eq!(PaletteLayout::from_entry_count(257), None);
    for old in [true, false] {
        let store = if old {
            dereth_dat::testing::open_classic_store_or_fail()
        } else {
            RetailDatStore::open_dir(&dereth_dat::testing::dat_dir()).unwrap()
        };
        let t = tables(old);
        let sex = &t.chargen.heritage_groups[&1].sexes[&2];
        let sample = |id: DataId, part: PaletteSample| {
            let p = Palette::decode_payload(id, &store.read_portal(id).unwrap()).unwrap();
            let layout = PaletteLayout::from_entry_count(p.colors_argb.len()).unwrap();
            p.colors_argb[part.index(layout)]
        };
        let eye_colors: BTreeSet<_> = sex
            .eye_colors
            .iter()
            .map(|&id| sample(DataId(id), PaletteSample::Eyes))
            .collect();
        assert_eq!(eye_colors.len(), sex.eye_colors.len());
        let hair_colors: BTreeSet<_> = sex
            .hair_colors
            .iter()
            .map(|&id| {
                let set =
                    PaletteSet::decode_payload(DataId(id), &store.read_portal(DataId(id)).unwrap())
                        .unwrap();
                sample(set.palette_ids[0], PaletteSample::Hair)
            })
            .collect();
        assert!(
            hair_colors.len() >= 3,
            "hair choices must not collapse to one unrelated color"
        );
        let set = PaletteSet::decode_payload(
            sex.skin_palset,
            &store.read_portal(sex.skin_palset).unwrap(),
        )
        .unwrap();
        let shades: BTreeSet<_> = set
            .palette_ids
            .iter()
            .map(|&id| sample(id, PaletteSample::Skin))
            .collect();
        assert_eq!(shades.len(), set.palette_ids.len());
    }
}

/// Behaviour: chargen.random.classic-random-picks-only-aluvian-gharundim-or-sho
#[test]
fn classic_random_heritage_on_the_final_world_is_aluvian_gharundim_or_sho() {
    let t = tables(false);
    assert!(
        t.heritage_keys().len() > 3,
        "the final world carries more heritages than the three"
    );
    let classic = dereth_chargen::CLASSIC_RANDOM_HERITAGES;
    let mut seen = std::collections::BTreeSet::new();
    for seed in 1..=60u32 {
        for entry in [CreationEntry::Normal, CreationEntry::Quick] {
            let mut state = CharGenState::with_policy(CreationPolicy::Classic);
            state.rng = CharGenRng::new(i32::try_from(seed).unwrap(), seed);
            state.begin_creation(&t, entry, true);
            assert!(
                classic.contains(&state.heritage_group),
                "seed {seed} {entry:?} opened on heritage {}",
                state.heritage_group
            );
            seen.insert(state.heritage_group);
            for _ in 0..4 {
                state.randomize_page(&t, CreationRandom::Heritage, true);
                assert!(
                    classic.contains(&state.heritage_group),
                    "seed {seed} Random gave heritage {}",
                    state.heritage_group
                );
                seen.insert(state.heritage_group);
            }
            // An explicit pick of another heritage stays possible, and Random leaves it again.
            assert!(state.choose_heritage(&t, dereth_chargen::HERITAGE_GEAR_KNIGHT));
            state.randomize_page(&t, CreationRandom::Heritage, true);
            assert!(classic.contains(&state.heritage_group));
        }
    }
    assert_eq!(
        seen.into_iter().collect::<Vec<_>>(),
        classic,
        "all three are reached"
    );
}

/// Behaviour: chargen.tables.world-keys-and-costs-remain-authoritative
#[test]
fn modern_random_heritage_never_rolls_a_heritage_the_world_lacks() {
    let t = tables(true);
    assert_eq!(
        t.heritage_keys(),
        [1, 2, 3],
        "the earlier world has no Viamontian"
    );
    let mut seen = std::collections::BTreeSet::new();
    for seed in 1..=60 {
        let mut state = CharGenState::with_policy(CreationPolicy::Modern);
        state.rng = CharGenRng::new(seed, 1);
        state.begin_creation(&t, CreationEntry::Normal, true);
        assert!(
            t.chargen
                .heritage_groups
                .contains_key(&state.heritage_group),
            "seed {seed} opened on heritage {}",
            state.heritage_group
        );
        seen.insert(state.heritage_group);
        for _ in 0..4 {
            state.randomize_page(&t, CreationRandom::Heritage, true);
            assert!(
                t.chargen
                    .heritage_groups
                    .contains_key(&state.heritage_group),
                "seed {seed} Random gave heritage {}",
                state.heritage_group
            );
            seen.insert(state.heritage_group);
        }
    }
    assert_eq!(seen.into_iter().collect::<Vec<_>>(), [1, 2, 3]);
}
