//! Divergence: V471
//! Fixture: synthetic formulas and inventory, seeded draws, explicit burn and terminal outcomes.

use super::*;
use empyrean_common::era::EraId;
use empyrean_world::entity::magic_state;

const BURN_SEED: i32 = 11;

fn fixture(era: EraId, level: u32) -> (H, Spell) {
    let mut h = H::new();
    h.player(100.0, 100.0);
    h.w.era = era.rules();
    for a in [PropertyAttribute::Focus, PropertyAttribute::Self_] {
        h.w.objects
            .get_mut(PLAYER)
            .unwrap()
            .biota
            .properties_attribute
            .as_mut()
            .unwrap()
            .insert(a, PropertiesAttribute::default());
    }
    for skill in [
        Skill::CreatureEnchantment,
        Skill::ItemEnchantment,
        Skill::LifeMagic,
        Skill::WarMagic,
    ] {
        h.set_skill(skill, level);
    }
    let mut spell = Spell::new(&h.w, STRENGTH_SELF_2, true);
    spell
        .formula
        .as_mut()
        .unwrap()
        .get_player_formula(&h.w, PLAYER);
    let pos = phys_ext::position(&h.w, h.o(PLAYER).phys.unwrap()).unwrap();
    player_magic::fields_mut(&mut h.w, PLAYER).start_pos = Some(pos);
    (h, spell)
}

fn expected(formula: &[u32], rate: f64) -> (Vec<u32>, f64) {
    let mut rng = DotNetRandom::new(BURN_SEED);
    let burned = formula
        .iter()
        .copied()
        .filter(|_| rng.next_double() < rate)
        .collect();
    (burned, rng.next_double())
}

fn seed() {
    ThreadSafeRandom::seed(u64::from(BURN_SEED.unsigned_abs()));
}

fn formula(spell: &Spell) -> &[u32] {
    spell
        .formula
        .as_ref()
        .unwrap()
        .current_formula
        .as_deref()
        .unwrap()
}

fn left(h: &H, component: u32) -> i32 {
    let wcid = COMPONENT_WCIDS
        .iter()
        .find(|(id, _)| *id == component)
        .unwrap()
        .1;
    container::get_num_inventory_items_of_wcid(&h.w, PLAYER, wcid)
}

#[test]
fn launch_burn_is_skill_independent_in_each_original_school() {
    for school in [
        MagicSchool::CreatureEnchantment,
        MagicSchool::ItemEnchantment,
        MagicSchool::LifeMagic,
        MagicSchool::WarMagic,
    ] {
        for skill in [0, 50, 100, 400] {
            let (mut h, mut s) = fixture(EraId::Launch, skill);
            s.spell_base.as_mut().unwrap().school = school.0.cast_unsigned();
            let (want, next) = expected(formula(&s), 0.5);
            seed();
            assert_eq!(
                s.try_burn_components(&mut h.w, PLAYER),
                want,
                "school {school:?}, skill {skill}"
            );
            assert_eq!(ThreadSafeRandom::next_float(0.0, 1.0), next);
        }
    }
}

#[test]
fn later_eras_keep_the_power_over_skill_discount_and_draw_order() {
    for era in [EraId::Infiltration, EraId::Eor] {
        for (skill, rate) in [(0, 0.5), (50, 0.5), (100, 0.5), (400, 0.125)] {
            let (mut h, s) = fixture(era, skill);
            let (want, next) = expected(formula(&s), rate);
            seed();
            assert_eq!(s.try_burn_components(&mut h.w, PLAYER), want);
            assert_eq!(ThreadSafeRandom::next_float(0.0, 1.0), next);
        }
    }
}

#[test]
fn repeated_formula_occurrences_roll_separately_in_order() {
    let (mut h, mut s) = fixture(EraId::Launch, 400);
    s.formula.as_mut().unwrap().current_formula = Some(vec![10, 10, 20, 10, 20, 10]);
    let (want, next) = expected(formula(&s), 0.5);
    assert!(!want.is_empty());
    seed();
    assert_eq!(s.try_burn_components(&mut h.w, PLAYER), want);
    assert_eq!(ThreadSafeRandom::next_float(0.0, 1.0), next);
}

#[test]
fn repeated_burns_consume_across_stacks_without_taking_other_components() {
    let (mut h, mut s) = fixture(EraId::Launch, 400);
    let first = h.give_components();
    let second = h.give_components();
    let stack = h.w.objects.get_mut(first[1]).unwrap();
    stack.set_max_stack_size(Some(100));
    stack.set_stack_size(Some(2));
    assert_eq!(left(&h, 10), 3);
    s.spell_base.as_mut().unwrap().component_loss = 1.0;
    s.formula.as_mut().unwrap().current_formula = Some(vec![10, 10, 10]);
    seed();
    start_capture();
    player_magic::try_burn_components(&mut h.w, PLAYER, &s);
    assert_eq!(left(&h, 10), 0);
    for component in [1, 20, 30, 40] {
        assert_eq!(left(&h, component), 2);
    }
    assert!(!container::get_inventory_items_of_wcid(&h.w, PLAYER, 774).contains(&first[1]));
    assert!(!container::get_inventory_items_of_wcid(&h.w, PLAYER, 774).contains(&second[1]));
    assert!(sent().iter().any(|m| m.text.as_deref()
        == Some("The spell consumed the following components: Hyssop, Hyssop, Hyssop")));
}

#[test]
fn zero_loss_never_consumes_components_and_keeps_one_draw_per_occurrence() {
    for era in EraId::ALL {
        let (mut h, mut s) = fixture(era, 400);
        h.give_components();
        s.spell_base.as_mut().unwrap().component_loss = 0.0;
        let (_, next) = expected(formula(&s), 0.0);
        seed();
        player_magic::try_burn_components(&mut h.w, PLAYER, &s);
        for component in FORMULA {
            assert_eq!(left(&h, component), 1);
        }
        assert_eq!(ThreadSafeRandom::next_float(0.0, 1.0), next);
    }
}

#[test]
fn success_and_fizzle_terminals_preserve_base_burn_and_later_era_results() {
    for era in EraId::ALL {
        for status in [
            CastingPreCheckStatus::Success,
            CastingPreCheckStatus::CastFailed,
        ] {
            let (mut h, s) = fixture(era, 400);
            h.give_components();
            let rate = if era == EraId::Launch { 0.5 } else { 0.125 };
            let (burned, _) = expected(formula(&s), rate);
            let mana = h.vital(PLAYER, PropertyAttribute2nd::MaxMana);
            seed();
            start_capture();
            player_magic::do_cast_spell_inner(
                &mut h.w,
                PLAYER,
                &s,
                None,
                5,
                Some(PLAYER),
                status,
                false,
            );
            for component in FORMULA {
                assert_eq!(left(&h, component), i32::from(!burned.contains(&component)));
            }
            assert_eq!(h.vital(PLAYER, PropertyAttribute2nd::MaxMana), mana - 5);
            assert_eq!(
                !registry_ids(&h.w, PLAYER).is_empty(),
                status == CastingPreCheckStatus::Success
            );
            assert_eq!(
                !events(&sent(), EV_WEENIE_ERROR).is_empty(),
                status == CastingPreCheckStatus::CastFailed
            );
        }
    }
}

#[test]
fn interrupted_casts_preserve_the_existing_fizzle_or_cancel_decision() {
    for era in EraId::ALL {
        for fizzle in [false, true] {
            let (mut h, s) = fixture(era, 400);
            h.give_components();
            let (burned, _) = expected(formula(&s), if era == EraId::Launch { 0.5 } else { 0.125 });
            magic_state::set_cast_params(
                &mut h.w,
                PLAYER,
                s,
                None,
                400,
                5,
                Some(PLAYER),
                CastingPreCheckStatus::Success,
            );
            seed();
            start_capture();
            player_magic::fail_cast(&mut h.w, PLAYER, fizzle);
            for component in FORMULA {
                assert_eq!(
                    left(&h, component),
                    i32::from(!(fizzle && burned.contains(&component)))
                );
            }
            assert!(registry_ids(&h.w, PLAYER).is_empty());
            assert!(!player_magic::fields(&h.w, PLAYER).magic_state.is_casting);
            if !fizzle {
                assert_eq!(
                    ThreadSafeRandom::next_float(0.0, 1.0),
                    DotNetRandom::new(BURN_SEED).next_double()
                );
            }
        }
    }
}

#[test]
fn safe_components_skip_consumption_and_random_draws_in_launch() {
    let (mut h, s) = fixture(EraId::Launch, 400);
    h.give_components();
    h.w.objects
        .get_mut(PLAYER)
        .unwrap()
        .set_safe_spell_components(true);
    seed();
    player_magic::try_burn_components(&mut h.w, PLAYER, &s);
    for component in FORMULA {
        assert_eq!(left(&h, component), 1);
    }
    assert_eq!(
        ThreadSafeRandom::next_float(0.0, 1.0),
        DotNetRandom::new(BURN_SEED).next_double()
    );
}
