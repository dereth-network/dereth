//! Effects, recipes, basic attacks and bodies as the server reads them from the September 2013
//! data.

use yalain_testkit::authored::Portal;
use yalain_world::craft::Action;
use yalain_world::effect_defs::Kind;

/// The effects of the heals, a buff, a quest's blessing and the Arwic mines' recipe read as the
/// data defines them: Ease Suffering heals 25 health at a rating of 1 and 250 at 50, Inner Calm
/// 200 vigor; Dash multiplies speed by 1.08 and acceleration by 1.05 for 30 s at 1; Aunmauri's
/// Courage adds 30 to the greatest health for two hours; the mines' bestowal teaches the sword's
/// recipe, and the recipe's effect moves the quest to its fourth phase.
/// Behaviour: server.effects.read-from-their-definitions
#[test]
fn the_effects_read_as_the_data_defines_them() {
    let p = Portal::open();
    let t = p.effects(&[
        0x7000_0772,
        0x7000_077A,
        0x7000_07E4,
        0x6F00_1DF7,
        0x6F00_19EF,
        0x6F00_11AA,
    ]);
    let ease = t.get(0x7000_0772).expect("Ease Suffering's heal");
    let Kind::InstantVital { flags, change } = &ease.kind else {
        panic!("{:?}", ease.kind)
    };
    assert_eq!(
        (flags & 0x3, change.at(1.0), change.at(50.0)),
        (1, Some(25.0), Some(250.0))
    );
    let calm = t.get(0x7000_077A).expect("Inner Calm's heal");
    assert!(
        matches!(&calm.kind, Kind::InstantVital { flags, change } if flags & 0x3 == 2 && change.at(1.0) == Some(200.0))
    );
    let dash = t.get(0x7000_07E4).expect("Dash");
    assert_eq!(dash.duration.at(1.0), Some(30.0));
    assert!(dash.removed_on_death());
    let changes = dash.changes(1.0);
    assert!(changes.contains(&(0x3_0127, true, 1.08)), "{changes:?}");
    assert!(changes.contains(&(0x3_0128, true, 1.05)), "{changes:?}");
    let courage = t.get(0x6F00_1DF7).expect("Courage");
    assert_eq!(courage.duration.at(1.0), Some(7200.0));
    assert_eq!(courage.changes(1.0), [(0x4_0107, false, 30.0)]);
    assert!(!courage.removed_on_death());
    assert_eq!(
        t.get(0x6F00_19EF).map(|d| (d.kind.clone(), d.fx)),
        Some((Kind::GrantRecipe(0x6F00_02A6), 0x4100_0499))
    );
    assert_eq!(
        t.get(0x6F00_11AA).map(|d| d.kind.clone()),
        Some(Kind::Quest {
            quest: 0x4000_00C1,
            update: 0x2000_0004
        })
    );
}

/// The Arwic mines' sword recipe reads as the data defines it: twenty iron in its one slot, two
/// cycles of its animation, one outcome making the sword, moving the quest on, using up the iron
/// and showing its text; the starting knife's recipe takes nothing and waits three hours; and the
/// craft skills read with their caps.
/// Behaviour: server.craft.recipes-read-from-their-definitions
#[test]
fn the_recipes_read_as_the_data_defines_them() {
    let p = Portal::open();
    let c = p.crafting(&[0x6F00_06A0, 0x6F00_06A7], &[0x6F00_02A6, 0x6F00_1071]);
    assert_eq!(
        c.skills.iter().map(|k| (k.did, k.cap)).collect::<Vec<_>>(),
        [(0x6F00_06A0, 1), (0x6F00_06A7, 500)]
    );
    let mines = &c.recipes[&0x6F00_02A6];
    assert_eq!(
        (mines.skill, mines.animation, mines.cycles),
        (0x6F00_06A0, 0x4100_00A4, 2)
    );
    assert_eq!(mines.ingredients.len(), 1);
    assert_eq!(
        (
            mines.ingredients[0].ordinal,
            mines.ingredients[0].quantity,
            mines.ingredients[0].items.clone()
        ),
        (1, 20, vec![0x4700_06C1])
    );
    assert!(!mines.checked(), "one check: it always succeeds");
    let o = mines.outcome(2.0, 500.0).expect("an outcome");
    assert!(o.actions.iter().any(|a| matches!(
        a,
        Action::Produce {
            template: 0x4700_0B21,
            quantity: 1,
            ..
        }
    )));
    assert!(o.actions.contains(&Action::Destroy {
        ordinal: 1,
        quantity: 20,
        by_spinner: None,
        spinners: None,
    }));
    assert!(o.actions.iter().any(|a| matches!(
        a,
        Action::Effect {
            state: 0x6F00_11AA,
            ..
        }
    )));
    assert!(o.actions.iter().any(|a| matches!(
        a,
        Action::Text {
            text: (0xE7E3_18D9, _),
            ..
        }
    )));
    let knife = &c.recipes[&0x6F00_1071];
    assert!(knife.ingredients.is_empty());
    assert_eq!(knife.recovery, 10_800.0);
}

/// A human's basic attacks read as the data defines them: the bow's thrown ones from 1 to 80 m
/// and struck ones within 1, the stave's within 10 and from 9 to 20, each one of the basic
/// attacks a run draws from.
/// Behaviour: server.weapons.basic-attacks-read-from-their-definitions
#[test]
fn the_basic_attacks_read_as_the_data_defines_them() {
    let t = Portal::open().skills();
    for (skill, min, max, right) in [
        (199, 1.0, 80.0, 0x4),
        (194, -1.0, 1.0, 0x4),
        (62, -1.0, 10.0, 0x4_0000),
        (65, 9.0, 20.0, 0x4_0000),
        (122, -1.0, 1.0, 0x8_0062),
    ] {
        let d = &t.skills[&skill];
        assert!(d.auto_attack, "{skill} is a basic attack");
        let u = d.usage.as_ref().expect("used");
        assert_eq!(
            (u.min_range, u.max_range, u.implements_right),
            (min, max, right),
            "{skill}"
        );
    }
    assert!(!t.skills[&207].auto_attack, "Deadeye is a special");
}

/// The skills that strike twice read as the data defines them: each of the 19 has two hooks that
/// strike, each naming its hand; handblades and a pair of one-handed weapons strike with each hand
/// and a polearm twice with the right; 817 and 1403 start with the left.
/// Behaviour: server.weapons.two-blows-read
#[test]
fn the_skills_that_strike_twice_read_with_each_blows_hand() {
    use yalain_world::skill_use::{hook_flag::LEFT_HAND, strikes};
    let t = Portal::open().skills();
    let hands = |skill: u32| -> Vec<bool> {
        let u = t.skills[&skill].usage.as_ref().expect("used");
        strikes(u)
            .into_iter()
            .map(|i| u.hooks[i].flags & LEFT_HAND != 0)
            .collect()
    };
    for skill in [313, 314, 315, 316, 317, 816, 818, 984, 985, 986, 1402, 1404] {
        assert_eq!(hands(skill), [false, true], "{skill}");
    }
    for skill in [817, 1403] {
        assert_eq!(hands(skill), [true, false], "{skill}");
    }
    for skill in 335..=339 {
        assert_eq!(hands(skill), [false, false], "{skill}");
    }
    assert_eq!(hands(122), [false], "one blow");
    assert_eq!(hands(62), [false], "the hook that only plays is no blow");
}

/// What a weapon is wielded as is what its class says: each starting weapon's table entry is its
/// class's, and the install's other weapons are known by theirs.
/// Behaviour: server.weapons.implement-by-class-read
#[test]
fn a_weapons_implement_is_what_its_class_says() {
    use yalain_world::skill_use::{implements_of_catalog, starting_implement, STARTING_WEAPONS};
    let p = Portal::open();
    let lib = p.library();
    let mut catalog = yalain_content::Catalog::default();
    for (name, _) in yalain_world::skill_use::IMPLEMENT_CLASSES {
        assert!(lib.package_id(name).is_some(), "the library has {name}");
    }
    for (t, _) in STARTING_WEAPONS {
        catalog.insert(p.template(t));
    }
    let read = implements_of_catalog(&catalog, |package, class| {
        lib.package_id(class)
            .is_some_and(|base| lib.derives_from(package, base))
    });
    for (t, _) in STARTING_WEAPONS {
        assert_eq!(read.get(&t).copied(), starting_implement(t), "{t:#x}");
    }
    let of = |class: &str, base: &str| {
        lib.derives_from(
            lib.package_id(class).expect("the class"),
            lib.package_id(base).expect("its base"),
        )
    };
    assert!(of("HandbladeRavager", "HandbladeTemplate"));
    assert!(of("PolearmLugianHammerbladeGoldenAge", "PolearmTemplate"));
}

/// The hall's practice dummy has the body its look's setup gives (radius 0.71, height 2.0).
/// Behaviour: server.specials.range-between-bodies
#[test]
fn a_creatures_body_is_its_looks_setups() {
    let body = Portal::open().body(0x4700_23B5).expect("a body");
    assert!(
        (body[0] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-3 && (body[1] - 1.995).abs() < 1e-3,
        "{body:?}"
    );
}

/// A taunt reads at the low end of the range its data gives: Growl a twentieth of a creature's
/// health, Ruse's and Taunt Force's a hundredth, Challenge's 0.15, Cragstone's Rebuke a quarter;
/// a detaunt the same below nothing, Blinding Arrow's and Challenge's own.
/// Behaviour: server.creatures.a-taunt-counts-at-the-draw
#[test]
fn a_taunt_reads_at_the_low_end_of_its_share_of_a_creatures_health() {
    let p = Portal::open();
    let wanted = [
        (0x7000_090D, 0.05),
        (0x7000_1FE3, 0.05),
        (0x7000_07C9, 0.01),
        (0x7000_1EDB, 0.01),
        (0x7000_0872, 0.15),
        (0x7000_1F3E, 0.25),
        (0x7000_1F55, -0.10),
        (0x7000_1F7A, -0.05),
    ];
    let t = p.effects(&wanted.map(|w| w.0));
    for (state, share) in wanted {
        match t.get(state).map(|d| &d.kind) {
            Some(Kind::Taunt(s)) => assert!((s - share).abs() < 1e-6, "{state:08X}: {s}"),
            other => panic!("{state:08X}: {other:?}"),
        }
    }
}

/// A recipe's attune outcome reads with the ordinal it names and its spinners: Armor Dyeing attunes
/// what it dyes at any spinner of its own, the Atlan Bow its product at 45 to 85.
/// Behaviour: server.craft.made-item-attuned
#[test]
fn a_recipes_attune_outcome_reads_with_its_ordinal_and_spinners() {
    let p = Portal::open();
    let c = p.crafting(&[], &[0x6F00_56B2, 0x6F00_1CB2]);
    let attunes = |recipe: u32| -> Vec<(u32, (u32, u32))> {
        c.recipes[&recipe]
            .checks
            .iter()
            .flat_map(|k| k.outcomes.iter())
            .flat_map(|o| o.actions.iter())
            .filter_map(|a| match a {
                Action::Attune { ordinal, spinners } => Some((*ordinal, *spinners)),
                _ => None,
            })
            .collect()
    };
    let dyeing = attunes(0x6F00_56B2);
    assert!(!dyeing.is_empty() && dyeing.iter().all(|a| *a == (1, (1, 150))));
    let bow = attunes(0x6F00_1CB2);
    assert!(bow.contains(&(10_001, (45, 85))), "{bow:?}");
}

/// The Small Sword reads as the data defines it: a spinner from 1 to 100, needing the crafting
/// workshop's effect; at item level 1 only its iron is asked for, 26 of it, used up by its action's
/// table; coal comes in at 20 and steel at 35, in their tables' amounts.
/// Behaviour: server.craft.spinner-recipes-read
#[test]
fn a_spinner_recipe_reads_with_its_ranges_tables_and_needed_effect() {
    let p = Portal::open();
    let c = p.crafting(&[0x6F00_0697], &[0x6F00_0F6F]);
    let r = &c.recipes[&0x6F00_0F6F];
    assert_eq!(r.spinner, Some((1, 100)));
    assert_eq!(r.required_effect, Some(0x6F00_0EB2));
    let at = |spinner: u32| -> Vec<(u32, u32)> {
        r.ingredients_at(spinner)
            .map(|i| (i.ordinal, c.required(i, spinner)))
            .collect()
    };
    assert_eq!(at(1), [(2, 26)]);
    assert_eq!(at(9), [(2, 100)]);
    assert_eq!(at(10), [(2, 112), (4, c.required(&r.ingredients[3], 10))]);
    assert_eq!(
        at(20).iter().map(|x| x.0).collect::<Vec<_>>(),
        [1, 2, 4],
        "coal from 20"
    );
    assert_eq!(at(20)[0], (1, 240));
    assert_eq!(at(35).len(), 4, "steel from 35");
    assert_eq!(at(45).len(), 5, "flawless pyreal from 45");
    let failed = r.outcome(1.0, 500.0).expect("a failure");
    let used: Vec<(u32, u32)> = failed
        .actions
        .iter()
        .filter_map(|a| c.destroyed(a, 1))
        .collect();
    assert_eq!(used, [(2, 26)], "only what level 1 asks for");
    let used_at_20: Vec<(u32, u32)> = failed
        .actions
        .iter()
        .filter_map(|a| c.destroyed(a, 20))
        .collect();
    assert_eq!(
        used_at_20,
        at(20),
        "a failure uses up what the spinner asked for"
    );
    // Each check by the value it covers up to: failed, flawed, made, superior.
    let makes = |o: &yalain_world::craft::Outcome| {
        o.actions
            .iter()
            .any(|a| matches!(a, Action::Produce { .. }))
    };
    let share = |value: f32| {
        let o = r.outcome(value, 500.0).expect("an outcome");
        (makes(o), o.xp_mod)
    };
    assert_eq!(share(1.0), (false, 0.0), "failed");
    assert_eq!(share(1.005), (true, 0.5), "flawed");
    assert_eq!(share(1.5), (true, 1.0), "made");
    assert_eq!(share(2.0), (true, 2.0), "superior");
}

/// Craft skills read with their tables, and a spinner recipe with its experience and difficulty
/// tables: tools' second level takes 40, a fifth of an award goes to the pool; butchery pays 2 a
/// use at level 7, 120 a day before uses pay less, a hundredth to the pool; the Tumerok armor
/// practice recipe pays 8 at spinner 3, 17 at 8, 67 at 16.
/// Behaviour: server.craft.experience-tables-read
#[test]
fn craft_skills_and_spinner_recipes_read_with_their_experience_tables() {
    use yalain_world::state::CraftSkillState;
    let p = Portal::open();
    let c = p.crafting(&[0x6F00_06A7, 0x6F00_068F], &[0x6F00_56AC]);
    let tools = &c.skills[0];
    assert_eq!(tools.step(1), Some(40));
    assert!((tools.pool_share - 0.2).abs() < 1e-6);
    assert!(
        tools.usage_xp.is_none(),
        "a recipe skill pays by its recipes"
    );
    assert_eq!(
        butchery_name(&c),
        (0x020C_2386, 0x2500_13CA),
        "its name, for the rating text"
    );
    let butchery = &c.skills[1];
    assert_eq!(butchery.cap, 150);
    assert_eq!(butchery.daily.at(7), 120);
    assert!((butchery.pool_share - 0.01).abs() < 1e-6);
    let at_7 = CraftSkillState {
        skill: 0x6F00_068F,
        level: 7,
        today: 0,
        available: 0,
    };
    assert_eq!(yalain_world::craft::usage_xp(butchery, &at_7), 2);
    let practice = &c.recipes[&0x6F00_56AC];
    assert_eq!(
        [3, 8, 16].map(|s| practice.base_xp(s)),
        [8, 17, 67],
        "the recipe's table at the spinner"
    );
    assert!(practice.difficulty_map.is_some());
}

fn butchery_name(c: &yalain_world::craft::Crafting) -> (u32, u32) {
    c.skills
        .iter()
        .find(|k| k.did == 0x6F00_068F)
        .map_or((0, 0), |k| k.name)
}

/// The effects of skills used on creatures read as the data defines them, and as the tests' copies
/// have them: Vacuum's debuff, Paralyze's hold, Charged Air's damage over time, Sinew Snap's link,
/// Lilitha's Vengeance's combination (which finds the link's type and adds a strike), and Espirit's
/// aura (its pulsed effect, the players it reaches and its range).
/// Behaviour: server.effects.creature-effects-read-from-their-definitions
#[test]
fn the_effects_on_creatures_combinations_and_auras_read_as_the_data_defines_them() {
    let p = Portal::open();
    let wanted: Vec<u32> = yalain_testkit::creature_effects()
        .iter()
        .map(|d| d.state)
        .collect();
    let t = p.effects(&wanted);
    for d in yalain_testkit::creature_effects() {
        let read = t
            .get(d.state)
            .unwrap_or_else(|| panic!("{:08X} reads", d.state));
        if let Kind::Chained(_) = d.kind {
            assert_eq!(read.changes(1.0), d.changes(1.0), "{:08X}", d.state);
            assert_eq!(read.changes(50.0), d.changes(50.0), "{:08X}", d.state);
        } else {
            assert_eq!(read.kind, d.kind, "{:08X}", d.state);
        }
        assert_eq!(
            (
                read.duration.at(1.0),
                read.duration.at(50.0),
                read.fx,
                read.enum_value,
                read.eq_class
            ),
            (
                d.duration.at(1.0),
                d.duration.at(50.0),
                d.fx,
                d.enum_value,
                d.eq_class
            ),
            "{:08X}",
            d.state
        );
    }
    let Kind::Aura(aura) = &t.get(yalain_testkit::ESPIRIT_AURA).expect("the aura").kind else {
        panic!("an aura")
    };
    assert_eq!(aura.range, 300.0);
}

/// The skills used on creatures read with their hooks as the data defines them: an area hook's
/// radius and flags (Vacuum's and Charged Air's ten meters around the target, the Eye of the
/// Storm's around its user), the missiles of Sinew Snap and Lilitha's Vengeance, and Espirit's aura
/// as its user effect.
/// Behaviour: server.skills.area-hooks-read-from-their-definitions
#[test]
fn the_skills_used_on_creatures_read_with_their_areas_as_the_data_defines_them() {
    let t = Portal::open().skills();
    for (id, _, want) in yalain_testkit::effect_skills() {
        let got = t.skills[&id].usage.clone().expect("used");
        assert_eq!(got.hooks.len(), want.hooks.len(), "{id}");
        for (g, w) in got.hooks.iter().zip(&want.hooks) {
            assert_eq!(
                (g.flags, g.radius, g.missile, g.anim_hook),
                (w.flags, w.radius, w.missile, w.anim_hook),
                "{id}"
            );
        }
        assert_eq!(
            (got.ui_flags, got.max_range),
            (want.ui_flags, want.max_range),
            "{id}"
        );
        assert!(
            want.target_effects
                .iter()
                .all(|e| got.target_effects.contains(e)),
            "{id}"
        );
        assert_eq!(got.user_effects, want.user_effects, "{id}");
    }
}

/// The summoning skills' pets read from their data: Nature's Aid leaves its user its pet effect,
/// a Lasher's class 16 with no lifetime, and the pet shows the skill's icon; every kind of pet the
/// server summons is a skill's pet effect, the Volcanic Rift's lasting 60 s.
/// Behaviour: server.pets.read-from-the-skills
#[test]
fn the_summoning_skills_pets_read_as_the_data_defines_them() {
    let p = Portal::open();
    let skills = p.skills();
    let aid = &skills.skills[&227];
    assert_eq!(aid.icon, 0x4100_47E6);
    let u = aid.usage.clone().expect("Nature's Aid is used");
    assert_eq!(u.user_effects, vec![0x7000_0805]);
    let mut states: Vec<u32> = yalain_world::pets::fallback_kinds()
        .iter()
        .map(|k| k.effect)
        .collect();
    states.push(0x7000_0830);
    let t = p.effects(&states);
    let lasher = t.get(0x7000_0805).expect("the Lasher's");
    let Kind::Pet { class, lifetime } = &lasher.kind else {
        panic!("not a pet: {lasher:?}")
    };
    assert_eq!((*class, lifetime.is_empty()), (16, true));
    // A mercenary's kind is known by its contract: no effect of the data summons it.
    for k in yalain_world::pets::fallback_kinds()
        .into_iter()
        .filter(|k| k.hire.is_none())
    {
        assert!(
            matches!(t.get(k.effect).map(|d| &d.kind), Some(Kind::Pet { .. })),
            "{:08X}",
            k.effect
        );
    }
    let Kind::Pet { lifetime, .. } = &t.get(0x7000_0830).expect("the rift's").kind else {
        panic!("not a pet")
    };
    assert_eq!(lifetime.at(1.0), Some(60.0));
}

/// The effects items' uses put on, read from the names their templates give, each the saved state
/// of the name's effect enumeration value: Bloodstone Ooze's two at spellcraft 50 and 100; Pure
/// Ebon Mana's infusion and its quest's bestowal; a Crystalline Tyrant Egg's mount and its focus
/// cost (five off the focus at once); Rabbit's Foot's luck; and the mount read as a speed of 1.9
/// with no duration.
/// Behaviour: server.items.usage-effects-read-from-their-names
#[test]
fn the_effects_of_items_uses_read_from_their_names() {
    use yalain_world::effect_defs::vital;
    use yalain_world::item_use::UsageEffect;
    let p = Portal::open();
    let uses = p.item_uses(&[0x4700_11B1, 0x4700_150B, 0x4700_14CC, 0x4700_190C]);
    let of = |t: u32| -> Vec<(u32, f32)> {
        uses.get(&t)
            .map(|l| {
                l.iter()
                    .map(|e: &UsageEffect| (e.state, e.spellcraft))
                    .collect()
            })
            .unwrap_or_default()
    };
    assert_eq!(
        of(0x4700_11B1),
        vec![(0x6F00_2659, 50.0), (0x6F00_2665, 100.0)]
    );
    assert_eq!(
        of(0x4700_150B),
        vec![(0x6F00_1C71, 100.0), (0x6F00_3E46, 1.0)]
    );
    assert_eq!(
        of(0x4700_14CC),
        vec![(0x6F00_1C4C, 1.0), (0x6F00_1C4D, 1.0)]
    );
    assert_eq!(of(0x4700_190C), vec![(0x6F00_1F1F, 1.0)]);
    let t = p.effects(&[0x6F00_1C4D]);
    let Kind::InstantVital { flags, change } = &t.get(0x6F00_1C4D).expect("the focus cost").kind
    else {
        panic!("not a change to vitals")
    };
    assert!(flags & vital::FOCUS != 0 && flags & (vital::HEALTH | vital::VIGOR) == 0);
    assert_eq!(change.at(1.0), Some(-5.0));
    let t = p.effects(&[0x6F00_1C4C]);
    let mount = t.get(0x6F00_1C4C).expect("the mount");
    assert!(mount.duration.is_empty());
    let speed = mount
        .changes(1.0)
        .into_iter()
        .find(|c| c.0 == yalain_world::effects::speed::VELOCITY);
    assert!(
        matches!(speed, Some((_, true, m)) if (m - 1.9).abs() < 1e-4),
        "{speed:?}"
    );
}

/// The effects that give a player its regeneration rates read as the data defines them: Robustify
/// at a rating of 50 adds 0.08 to the combat health rate and doubles the peace health rate; a
/// crafted item's bonus adds 0.08 to the combat health rate; a life-regeneration jewel adds 0.07 to it and
/// takes 0.03 from the combat vigor rate.
/// Behaviour: server.regen.rate-effects-read-from-their-definitions
#[test]
fn the_effects_that_give_regeneration_rates_read_as_the_data_defines_them() {
    let p = Portal::open();
    let t = p.effects(&[0x6F00_10F0, 0x6F00_1730, 0x6F00_0550]);
    let near = |got: &[(u32, bool, f32)], want: (u32, bool, f32)| {
        got.iter()
            .any(|g| g.0 == want.0 && g.1 == want.1 && (g.2 - want.2).abs() < 1e-4)
    };
    let robustify = t.get(0x6F00_10F0).expect("Robustify").changes(50.0);
    assert!(near(&robustify, (0x3_010B, false, 0.08)), "{robustify:?}");
    assert!(near(&robustify, (0x3_0106, true, 2.0)), "{robustify:?}");
    let crafted = t.get(0x6F00_1730).expect("the crafted bonus").changes(1.0);
    assert!(near(&crafted, (0x3_010B, false, 0.08)), "{crafted:?}");
    let ring = t.get(0x6F00_0550).expect("the jewel").changes(1.0);
    assert!(near(&ring, (0x3_010B, false, 0.07)), "{ring:?}");
    assert!(near(&ring, (0x3_010C, false, -0.03)), "{ring:?}");
}

/// The effects items put on whoever wears or wields them, read from the names their templates
/// give, each the saved state of the name's effect enumeration value at the template's spellcraft
/// (else 1): the Soulbound Staff of Linvak's, an Artefon breastplate's at 50, the Shiny Temple
/// Amulet's two; the clarinet's appearance is one its wearer is never shown. The records' categories
/// follow the effects' flags: the staff's ordinary, the breastplates' watching effects put on (9) or
/// watching vitals change and effects change them (19).
/// Behaviour: server.equip.worn-effects-read-from-their-names
#[test]
fn the_effects_items_put_on_their_wearers_read_from_their_names() {
    use yalain_world::item_use::UsageEffect;
    let p = Portal::open();
    let worn = p.worn_effects(&[0x4700_0724, 0x4700_0B17, 0x4700_060F, 0x4700_1148]);
    let of = |t: u32| -> Vec<(u32, f32)> {
        worn.get(&t)
            .map(|l| {
                l.iter()
                    .map(|e: &UsageEffect| (e.state, e.spellcraft))
                    .collect()
            })
            .unwrap_or_default()
    };
    assert_eq!(of(0x4700_0724), vec![(0x6F00_1F3D, 1.0)]);
    assert_eq!(of(0x4700_0B17), vec![(0x6F00_1962, 50.0)]);
    assert_eq!(
        of(0x4700_060F),
        vec![(0x6F00_1C46, 1.0), (0x6F00_1C47, 1.0)]
    );
    assert_eq!(of(0x4700_1148), vec![(0x6F00_186C, 1.0)]);
    let t = p.effects(&[
        0x6F00_1F3D,
        0x6F00_1962,
        0x6F00_186C,
        0x6F00_1964,
        0x6F00_1961,
    ]);
    let d = |s: u32| t.get(s).expect("read");
    assert!(d(0x6F00_1F3D).client_visible() && !d(0x6F00_1F3D).removed_on_death());
    assert!(d(0x6F00_1962).client_visible() && d(0x6F00_1962).removed_on_death());
    assert!(!d(0x6F00_186C).client_visible(), "the clarinet's unseen");
    assert_eq!(
        [0x6F00_1F3D, 0x6F00_1961, 0x6F00_1964].map(|s| d(s).categories()),
        [1, 9, 19]
    );
}

/// The items naming a concealing effect whose class is read from its words name those states: the
/// fanatics' disguise, the badges of the two sides; the amulet of safe passage's name resolves to
/// nothing, so it puts nothing on.
/// Behaviour: server.creatures.an-effects-words-name-its-class
#[test]
fn the_items_of_the_effects_named_for_a_class_name_their_states() {
    let p = Portal::open();
    let states = |l: Option<&Vec<yalain_world::item_use::UsageEffect>>| -> Vec<u32> {
        l.map(|l| l.iter().map(|e| e.state).collect())
            .unwrap_or_default()
    };
    let used = p.item_uses(&[0x4700_1281, 0x4700_1518, 0x4700_1516]);
    assert_eq!(states(used.get(&0x4700_1281)), [0x6F00_19F1]);
    assert_eq!(states(used.get(&0x4700_1518)), [0x6F00_1C77]);
    assert_eq!(states(used.get(&0x4700_1516)), [0x6F00_1C78]);
    let worn = p.worn_effects(&[0x4700_1668]);
    assert!(states(worn.get(&0x4700_1668)).is_empty());
}

/// A stun skill's target effects name the short immunity; it reads as hidden and leaving the long
/// one, whose data gives its minute, which keeps off stuns and the immunity's own class; the slowing
/// and holding kinds read the same with their classes, in both of the install's sets of states.
/// Behaviour: server.effects.crowd-control-immunity-read
#[test]
fn the_crowd_control_immunities_read_as_the_data_defines_them() {
    let p = Portal::open();
    let paralyze = p.skills().skills.get(&yalain_testkit::PARALYZE).cloned();
    let usage = paralyze.and_then(|d| d.usage).expect("Paralyze's use");
    assert!(usage.target_effects.contains(&0x7000_0720), "{usage:?}");
    let t = p.effects(&[
        0x7000_0720,
        0x7000_071C,
        0x7000_071E,
        0x6F00_210D,
        0x7000_1EDF,
    ]);
    let read = |short: u32, long: u32, classes: &[u32], seconds: f32| {
        let s = t.get(short).expect("the short one");
        assert_eq!(s.kind, Kind::Immunity { then: long });
        assert_eq!(s.duration.at(1.0), Some(5.0));
        assert!(!s.client_visible(), "{short:08X} is hidden");
        let l = t.get(long).expect("the long one comes with it");
        assert_eq!(l.duration.at(1.0), Some(seconds));
        assert!(l.client_visible(), "{long:08X} shows");
        match &l.kind {
            Kind::Ward { classes: c, chance } => {
                assert_eq!(c.as_slice(), classes);
                assert_eq!(chance.at(50.0), Some(1.0));
            }
            k => panic!("{long:08X}: {k:?}"),
        }
    };
    read(0x7000_0720, 0x7000_0721, &[55, 100], 60.0);
    read(0x7000_071C, 0x7000_071D, &[77, 101], 60.0);
    read(0x7000_071E, 0x7000_071F, &[52, 5015, 51, 5017, 97], 60.0);
    read(0x6F00_210D, 0x6F00_210E, &[55, 100], 60.0);
    read(0x7000_1EDF, 0x7000_1EE0, &[1124], 120.0);
}

/// The counters read with the effect each answers with, and its chance; the wards with what they
/// keep off.
/// Behaviour: server.effects.counters-and-wards-read
#[test]
fn the_counters_and_wards_read_as_the_data_defines_them() {
    let p = Portal::open();
    let t = p.effects(&[
        0x6F00_2646,
        0x6F00_2648,
        0x6F00_24C8,
        0x6F00_24CB,
        0x6F00_24CE,
        0x6F00_266E,
        0x6F00_2671,
        0x6F00_2672,
        0x6F00_2675,
    ]);
    let counter = |s: u32| match &t.get(s).expect("read").kind {
        Kind::Counter { effect, chance } => (*effect, chance.at(30.0)),
        k => panic!("{s:08X}: {k:?}"),
    };
    assert_eq!(counter(0x6F00_2646), (0x6F00_2647, Some(0.05)));
    assert_eq!(counter(0x6F00_2648), (0x6F00_2649, Some(0.1)));
    assert_eq!(counter(0x6F00_24C8), (0x6F00_24C9, Some(0.05)));
    assert_eq!(counter(0x6F00_24CB), (0x6F00_24CC, Some(0.03)));
    assert_eq!(counter(0x6F00_24CE), (0x6F00_24CF, Some(0.03)));
    // Its answers come with it, and do what they say: harm, and less armor.
    let harm = t.get(0x6F00_2647).expect("Countering's answer");
    assert!(
        matches!(&harm.kind, Kind::VitalOverTime { per_second, .. } if per_second.at(0.0) == Some(-120.0))
    );
    assert_eq!(harm.duration.at(30.0), Some(1.0));
    let brittle = t.get(0x6F00_24C9).expect("Brittle Bones's answer");
    assert_eq!(brittle.changes(30.0), [(0x4_0125, false, -50.0)]);
    assert_eq!(brittle.duration.at(30.0), Some(30.0));
    let ward = |s: u32| match &t.get(s).expect("read").kind {
        Kind::Ward { classes, chance } => (classes.clone(), chance.at(30.0).expect("a chance")),
        k => panic!("{s:08X}: {k:?}"),
    };
    assert_eq!(ward(0x6F00_266E), (vec![22, 5061], 1.0));
    assert_eq!(ward(0x6F00_2672), (vec![48, 1119, 5013], 0.15));
    assert_eq!(ward(0x6F00_2675), (vec![53, 5007, 5022, 5046], 0.15));
    assert_eq!(
        ward(0x6F00_2671),
        (vec![49, 5014, 26, 4015, 5043, 5050], 0.15)
    );
    // They go on their wearers as records.
    for s in [0x6F00_2646, 0x6F00_266E] {
        assert!(t.get(s).expect("read").client_visible(), "{s:08X}");
    }
    // What an Armor Ward keeps off holds the counters' own.
    assert_eq!(brittle.eq_class, 5061);
}

/// The effects that watch attacks read with what they take and send back: a creature's Absorption,
/// a caster's Thornskin, Darkfoe's always a fifth, the armor's one time in five, Thorns' at a third
/// against a player, an amount rather than a share, and what lowers vigor (which an attack doesn't).
/// Behaviour: server.effects.watchers-read
#[test]
fn the_watchers_read_as_the_data_defines_them() {
    let p = Portal::open();
    let t = p.effects(&[
        0x6F00_0186,
        0x6F00_018A,
        0x7000_1C79,
        0x7000_08D8,
        0x7000_070E,
        0x6F00_1964,
        0x7000_09FD,
        0x6F00_1AA6,
        0x6F00_264F,
    ]);
    let w = |s: u32| match &t.get(s).expect("read").kind {
        Kind::Watcher(w) => (**w).clone(),
        k => panic!("{s:08X}: {k:?}"),
    };
    let absorption = w(0x6F00_0186);
    assert_eq!(absorption.absorbs.chance, 1.0);
    assert_eq!(absorption.absorbs.amount.at(1.0), Some(0.05));
    assert!(absorption.sends_back.amount.is_empty());
    assert!(absorption.shares && absorption.watches);
    let reflection = w(0x6F00_018A);
    assert_eq!(reflection.sends_back.amount.at(1.0), Some(0.025));
    assert!(reflection.absorbs.amount.is_empty());
    // A practice dummy's: a hundredth a point, for good.
    let dummy = w(0x7000_1C79);
    assert_eq!(dummy.sends_back.amount.at(1.0), Some(0.01));
    assert_eq!(dummy.sends_back.amount.at(500.0), Some(5.0));
    assert!((dummy.sends_back.amount.at(35.0).unwrap_or(0.0) - 0.35).abs() < 1e-3);
    assert!(dummy.watches);
    let thornskin = w(0x7000_08D8);
    assert_eq!(thornskin.sends_back.amount.at(1.0), Some(0.08));
    assert_eq!(thornskin.sends_back.amount.at(50.0), Some(0.35));
    assert_eq!(w(0x7000_070E).sends_back.amount.at(30.0), Some(0.2));
    let armor = w(0x6F00_1964);
    assert_eq!(armor.sends_back.chance, 0.2);
    assert_eq!(armor.sends_back.amount.at(0.0), Some(0.1));
    let thorns = w(0x7000_09FD);
    assert_eq!(thorns.against_a_player, 1.0 / 3.0);
    let might = w(0x6F00_1AA6);
    assert_eq!(might.absorbs.amount.at(0.0), Some(135.0));
    assert!(!might.shares);
    assert!(
        !w(0x6F00_264F).watches,
        "vigor's, which no attack lowers here"
    );
    // They go on as records.
    assert!(t.get(0x7000_070E).expect("read").client_visible());
    assert!(t.get(0x6F00_0186).expect("read").lasts());
}

/// The protections read with their lists and chances: Extinguish Blaze's along a line from a fifth
/// at 1 to 0.7 at 50, Immolation's, Cure Disease's and the Blockers' always.
/// Behaviour: server.effects.protections-read
#[test]
fn the_protections_read_as_the_data_defines_them() {
    let p = Portal::open();
    let t = p.effects(&[
        0x7000_1F48,
        0x7000_1F53,
        0x7000_1F63,
        0x7000_1F19,
        0x6F00_21A9,
        0x6F00_1141,
    ]);
    let ward = |s: u32| match &t.get(s).expect("read").kind {
        Kind::Ward { classes, chance } => (classes.clone(), chance.clone()),
        k => panic!("{s:08X}: {k:?}"),
    };
    let (blaze, chance) = ward(0x7000_1F48);
    assert_eq!(blaze, [48, 1119, 5013]);
    assert_eq!((chance.at(1.0), chance.at(50.0)), (Some(0.2), Some(0.7)));
    let (immolation, chance) = ward(0x7000_1F53);
    assert_eq!(immolation, [48, 5013]);
    assert_eq!(chance.at(30.0), Some(1.0));
    let (disease, chance) = ward(0x7000_1F63);
    assert_eq!(disease, [49, 5014, 26, 4015, 5043, 5050]);
    assert_eq!(chance.at(30.0), Some(1.0));
    for s in [0x7000_1F19, 0x6F00_21A9, 0x6F00_1141] {
        let (blocker, chance) = ward(s);
        assert_eq!(blocker, [117], "{s:08X}");
        assert_eq!(chance.at(30.0), Some(1.0));
    }
}

/// No use of a skill and no aura keeps off its own: each effect after a protection in its list is
/// of a class the protection leaves alone, over every skill and every aura they read.
/// Behaviour: server.effects.protections-read
#[test]
fn no_skill_or_aura_keeps_off_its_own_effects() {
    let p = Portal::open();
    let skills = p.skills();
    let mut lists: Vec<(String, Vec<u32>)> = skills
        .skills
        .iter()
        .filter_map(|(k, d)| d.usage.as_ref().map(|u| (k, u)))
        .flat_map(|(k, u)| {
            [
                (format!("skill {k}"), u.target_effects.clone()),
                (format!("skill {k} on its user"), u.user_effects.clone()),
            ]
        })
        .collect();
    let mut states: Vec<u32> = lists.iter().flat_map(|(_, l)| l.iter().copied()).collect();
    // The aura that names a Blocker, with the skills' own.
    states.push(0x6F00_0225);
    states.sort_unstable();
    states.dedup();
    let t = p.effects(&states);
    for d in t.effects.values() {
        if let Kind::Aura(a) = &d.kind {
            lists.push((format!("aura {:08X}", d.state), a.pulsed.clone()));
        }
    }
    let mut wards = 0;
    for (who, list) in &lists {
        for (i, state) in list.iter().enumerate() {
            let Some(Kind::Ward { classes, .. }) = t.get(*state).map(|d| &d.kind) else {
                continue;
            };
            wards += 1;
            for later in &list[i + 1..] {
                let class = t.get(*later).map_or(0, |d| d.eq_class);
                assert!(
                    !classes.is_empty() && !classes.contains(&class),
                    "{who}: {state:08X} would keep off {later:08X}"
                );
            }
        }
    }
    assert!(wards >= 3, "the protections are among them: {wards}");
    // The Blocker comes on after what it blocks.
    let Some(Kind::Aura(a)) = t.get(0x6F00_0225).map(|d| &d.kind) else {
        panic!("the aura")
    };
    assert_eq!(a.pulsed, [0x6F00_1140, 0x6F00_1141]);
    assert_eq!(t.get(0x6F00_1140).map(|d| d.eq_class), Some(117));
}

/// A skill used on oneself names its effects in its list for its user, not its target's: Immolation's
/// protection and Bladeskin's aura come on by using them. The plain Bladeskin's aura pulses Bladeskin
/// alone; only the Soul Defractor's names a Blocker.
/// Behaviour: server.effects.self-cast-lists
#[test]
fn immolation_and_bladeskin_put_their_effects_on_their_user() {
    use auberean_protocol::{queue, ClientMessage};
    use auberean_script::args::Arg;
    use yalain_testkit::play::server_call;
    use yalain_testkit::Harness;
    use yalain_world::effects::{self, Cast, On};
    use yalain_world::skill_use::function;
    use yalain_world::state::SkillState;
    const IMMOLATION: u32 = 1079;
    const BLADESKIN: u32 = 1006;
    const PROTECTION: u32 = 0x7000_1F53;
    const AURA: u32 = 0x6F00_00C8;
    const WATCHER: u32 = 0x6F00_10F5;
    /// A fire effect over time.
    const BURNS: u32 = 0x6F00_10C7;

    let p = Portal::open();
    let mut skills = p.skills();
    let list = |s: u32| skills.skills[&s].usage.as_ref().expect("used").clone();
    assert!(list(IMMOLATION).target_effects.is_empty());
    assert!(list(BLADESKIN).target_effects.is_empty());
    assert_eq!(
        list(IMMOLATION).user_effects,
        [0x7000_07F8, 0x7000_1F51, PROTECTION, 0x7000_1F52]
    );
    assert_eq!(list(BLADESKIN).user_effects, [AURA]);
    let mut states = vec![BURNS, 0x6F00_0225];
    states.extend(list(IMMOLATION).user_effects);
    states.extend(list(BLADESKIN).user_effects);
    let table = p.effects(&states);
    let Some(Kind::Aura(a)) = table.get(AURA).map(|d| &d.kind) else {
        panic!("the aura")
    };
    assert_eq!(a.pulsed, [WATCHER]);
    for s in [IMMOLATION, BLADESKIN] {
        let u = skills
            .skills
            .get_mut(&s)
            .and_then(|d| {
                d.parents.clear();
                d.prereqs.clear();
                d.usage.as_mut()
            })
            .expect("used");
        u.implements_right = -1;
        u.implements_left = -1;
        u.vigor_add = 0.0;
        u.vigor_mult = 0.0;
        u.min_vigor = 0.0;
    }

    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    h.server.skill_table = skills;
    h.server.effect_table = table;
    let world = *h.server.players.keys().next().expect("in the world");
    for (n, skill) in [IMMOLATION, BLADESKIN].into_iter().enumerate() {
        let held = SkillState {
            skill,
            granted: 50,
            mask: 1,
            cost: 0,
            children: 0,
            dependents: 0,
            spent: 0,
        };
        if let Some(p) = h.server.players.get_mut(&world) {
            p.play.skills.push(held);
        }
        h.bots[b].send(
            true,
            queue::WEENIE,
            &server_call(
                function::USE,
                &[
                    Arg::Int(u32::try_from(n).unwrap_or(0) + 1),
                    Arg::Int(skill),
                    Arg::LongInt(me),
                ],
            ),
        );
        h.run(400);
    }
    let holds = |h: &Harness, s: u32| {
        h.server
            .players
            .get(&world)
            .is_some_and(|p| p.play.effects.iter().any(|e| e.state == s))
    };
    assert!(holds(&h, PROTECTION), "Immolation's protection");
    assert!(holds(&h, WATCHER), "Bladeskin");
    let clock = h.clock();
    effects::apply_on(
        &mut h.server,
        &mut h.net,
        On::Player(world),
        BURNS,
        Cast {
            spellcraft: 30.0,
            ..Cast::default()
        },
        clock,
    );
    assert!(!holds(&h, BURNS), "fire kept off");
}

/// Plan B, Immolation and Taunt Force answer what comes at them, of any class and always: Plan B and
/// Immolation with fire over time. Taunt Force has no time to last, so its Challenge doesn't come;
/// the Challenge itself is a taunt.
/// Behaviour: server.effects.replies-read
#[test]
fn the_effects_that_answer_what_comes_at_them_read_as_the_data_defines_them() {
    let p = Portal::open();
    let t = p.effects(&[0x6F00_0027, 0x7000_1F51, 0x7000_1EDA]);
    let reply = |s: u32| match &t.get(s).expect("read").kind {
        Kind::Reply {
            effect,
            chance,
            classes,
        } => (*effect, chance.at(30.0), classes.is_empty()),
        k => panic!("{s:08X}: {k:?}"),
    };
    assert_eq!(reply(0x6F00_0027), (0x6F00_10C7, Some(1.0), true));
    assert_eq!(reply(0x7000_1F51), (0x7000_1F54, Some(1.0), true));
    assert_eq!(reply(0x7000_1EDA), (0x7000_1EDB, Some(1.0), true));
    assert!(t.get(0x6F00_0027).expect("read").lasts());
    assert!(t.get(0x7000_1F51).expect("read").lasts());
    assert!(!t.get(0x7000_1EDA).expect("read").lasts());
    for burn in [0x6F00_10C7, 0x7000_1F54] {
        assert!(matches!(
            t.get(burn).expect("what it adds").kind,
            Kind::VitalOverTime { .. }
        ));
    }
    assert_eq!(t.get(0x7000_1EDB).expect("read").kind, Kind::Taunt(0.01));
}

/// A recipe that binds its product names it by the product's ordinal over the whole spinner range
/// (`0x6F000E9D`: each outcome makes `0x470008D4` and binds it, spinners 1 to 100).
/// Behaviour: server.craft.recipe-binds-read
#[test]
fn a_recipe_that_binds_its_product_reads_the_bind_on_its_ordinal() {
    use yalain_world::craft::Action;
    let p = Portal::open();
    let c = p.crafting(&[], &[0x6F00_0E9D]);
    let r = &c.recipes[&0x6F00_0E9D];
    let outcomes: Vec<_> = r.checks.iter().flat_map(|k| k.outcomes.iter()).collect();
    let mut bound = 0;
    for o in &outcomes {
        let made: Vec<u32> = o
            .actions
            .iter()
            .filter_map(|a| match a {
                Action::Produce {
                    template: 0x4700_08D4,
                    ordinal,
                    ..
                } => Some(*ordinal),
                _ => None,
            })
            .collect();
        for a in &o.actions {
            if let Action::Bind { ordinal, spinners } = a {
                assert!(made.contains(ordinal), "binds what it makes");
                assert_eq!(*spinners, (1, 100));
                bound += 1;
            }
        }
    }
    assert!(bound >= 3, "{bound} binds");
}

/// A forager's food is drunk as a potion is and is no item its use destroys; its two effects heal
/// health and vigor, 244 at the spellcraft of 40 (2017: the heal of 10 of 15 foods).
/// Behaviour: server.items.food-read
#[test]
fn a_forager_s_food_reads_as_a_drink_that_heals_health_and_vigor() {
    use yalain_world::effect_defs::Kind;
    use yalain_world::item_use::{self, FOOD, FOOD_EFFECTS, FOOD_SPELLCRAFT};
    let p = Portal::open();
    let food = p.template(FOOD);
    assert_eq!(
        item_use::behavior_of(FOOD, &food.props),
        Some(item_use::DRINK)
    );
    let lib = p
        .dat
        .read_checked(
            auberean_script::library::RECORD,
            auberean_script::library::Library::from_record,
        )
        .expect("reads")
        .value;
    assert!(!yalain_world::profile::destroyed_on_use_kind(
        food.description.package,
        |n| lib.package_id(n),
        |a, b| lib.derives_from(a, b),
    ));
    let t = p.effects(&FOOD_EFFECTS);
    let mut flags = Vec::new();
    for state in FOOD_EFFECTS {
        let Some(Kind::InstantVital { flags: f, change }) = t.get(state).map(|d| d.kind.clone())
        else {
            panic!("{state:08X} heals at once");
        };
        let at = change.at(FOOD_SPELLCRAFT).expect("a change");
        assert_eq!(at.floor(), 244.0, "{state:08X}: {at}");
        flags.push(f & 0x3);
    }
    assert_eq!(flags, [1, 2], "health, then vigor");
}

/// The recalls' effects read where they go: the lifestone's and Portal Recall's to the places a
/// player keeps (links 0x40000001 and 0x40000004), each city's to its place by name (told, in its
/// own table), Arramora's to its place untold (the generic class's own), Allegiance Recall's to the
/// allegiance's hall; a city recall's protection lasts 30 s with the lifestone's glow.
/// Behaviour: server.effects.recalls-read
#[test]
fn the_recalls_read_where_they_go() {
    use yalain_world::effect_defs::Recall;
    let p = Portal::open();
    let t = p.effects(&[
        0x7000_0722,
        0x7000_0727,
        0x7000_1FC0,
        0x7000_1FC1,
        0x7000_1FC2,
        0x6F00_0222,
        0x7000_1EC5,
        0x7000_2014,
    ]);
    let recall = |state: u32| match &t.get(state).expect("read").kind {
        Kind::Recall(r) => (r.clone(), t.get(state).and_then(|d| d.name).map(|n| n.1)),
        other => panic!("{state:08X}: {other:?}"),
    };
    assert_eq!(
        recall(0x7000_0722),
        (Recall::Link(0x4000_0001), Some(0x2500_115A))
    );
    assert_eq!(
        recall(0x7000_0727),
        (Recall::Link(0x4000_0004), Some(0x2500_115B))
    );
    for (state, name, table) in [
        (0x7000_1FC0, "human_recall_cragstone_01", 0x2500_1239),
        (0x7000_1FC1, "tumerok_recall_ikeras_01", 0x2500_123A),
        (0x7000_1FC2, "lugian_recall_linvaktukal_01", 0x2500_123B),
    ] {
        assert_eq!(
            recall(state),
            (
                Recall::Named {
                    names: vec![name.into()],
                    told: true
                },
                Some(table)
            )
        );
    }
    assert_eq!(
        recall(0x6F00_0222).0,
        Recall::Named {
            names: vec!["arramora_start".into()],
            told: false
        }
    );
    assert_eq!(recall(0x7000_1EC5).0, Recall::Allegiance);
    let protection = t.get(0x7000_2014).expect("the protection");
    assert_eq!(
        (protection.duration.at(1.0), protection.fx),
        (Some(30.0), 0x4100_000F)
    );
}

/// Every refining recipe the install holds reads with the template it yields, a stack of a material,
/// each of the ten products by the recipes of its own (2017: Steel, the recipe recorded).
/// Behaviour: server.craft.refined-products-in-the-install
#[test]
fn every_refining_recipe_reads_with_its_product() {
    use yalain_world::craft::Custom;
    let p = Portal::open();
    let steel: Vec<u32> = (0x6F00_10AA..=0x6F00_10AD).collect();
    let dramastic: Vec<u32> = (0x6F00_109E..=0x6F00_10A1).collect();
    let bricks = [0x6F00_1FDF, 0x6F00_1FE1, 0x6F00_1FE3];
    for (recipes, product) in [
        (steel.as_slice(), 0x4700_0B40),
        (dramastic.as_slice(), 0x4700_0B46),
        (bricks.as_slice(), 0x4700_0655),
    ] {
        let c = p.crafting(&[], recipes);
        for r in recipes {
            assert_eq!(
                c.recipes.get(r).map(|d| d.custom),
                Some(Custom::Refine { product }),
                "{r:08X}"
            );
        }
        assert!(c.templates().contains(&product), "the catalog makes it");
        assert!(
            p.template(product).props.int("Item_MaxStackSize").is_some(),
            "a stack"
        );
    }
    // Card melds work out their own product; spellbinding makes a totem.
    let c = p.crafting(&[], &[0x6F00_1ABB, 0x6F00_1081]);
    assert_eq!(c.recipes[&0x6F00_1ABB].custom, Custom::Unknown);
    assert_eq!(c.recipes[&0x6F00_1081].custom, Custom::Spellbind);
}

/// The install's slots read what they ask: spellbinding's sword by the spinner, a level up to 50
/// and an Arcane Lore from 51; an armor empowerment's refused flag, which the recipe itself adds;
/// a recipe's cutoff.
/// Behaviour: server.craft.slots-ask-level-and-lore
#[test]
fn the_installs_slots_read_their_level_lore_flags_and_cutoff() {
    use yalain_world::craft::{error, Action, Limit};
    let p = Portal::open();
    let c = p.crafting(&[], &[0x6F00_1081, 0x6F00_030A, 0x6F00_106C]);
    let slot = &c.recipes[&0x6F00_1081].ingredients[0];
    assert_eq!(slot.level, Limit::ByTable(0x6F00_06B6));
    assert_eq!(slot.lore, Limit::ByTable(0x6F00_0679));
    assert_eq!(
        c.level_or_lore_refused(slot, 20, 20, 0),
        0,
        "as 2017, level 20"
    );
    assert_eq!(
        c.level_or_lore_refused(slot, 20, 23, 0),
        error::ITEM_LEVEL_TOO_HIGH
    );
    assert_eq!(c.level_or_lore_refused(slot, 51, 90, 7), 0, "lore 7 at 51");
    assert_eq!(
        c.level_or_lore_refused(slot, 51, 1, 8),
        error::ITEM_LORE_TOO_HIGH
    );
    let empower = &c.recipes[&0x6F00_030A];
    assert_eq!(empower.ingredients[1].restricted_flags, 8);
    assert!(empower
        .checks
        .iter()
        .flat_map(|x| &x.outcomes)
        .flat_map(|o| &o.actions)
        .any(|a| matches!(
            a,
            Action::Flag {
                ordinal: 2,
                flags: 8,
                ..
            }
        )));
    assert_eq!(c.recipes[&0x6F00_106C].cutoff, Some(50));
    assert_eq!(empower.cutoff, None);
}

/// The install's things say how they are held: a looted bow takes both hands and hangs as a bow,
/// a polearm too, a handblade from its own places in either hand, a shield in the off hand, a tool
/// from the first place.
/// Behaviour: server.weapons.hold-read
#[test]
fn a_things_hold_reads_from_its_template() {
    use yalain_world::equip::{flags, hold_of};
    let p = Portal::open();
    let mut catalog = yalain_content::Catalog::default();
    for t in [
        0x4700_07A3,
        0x4700_0730,
        0x4700_071F,
        0x4700_051B,
        0x4700_06FD,
        0x4700_0CC5,
    ] {
        catalog.insert(p.template(t));
    }
    let bow = hold_of(&catalog, 0x4700_07A3);
    assert_eq!(
        (
            bow.primary,
            bow.secondary,
            bow.valid,
            bow.taken,
            bow.precluded,
            bow.flags
        ),
        (2, 1, 64, 192, 128, flags::WIELDED)
    );
    let polearm = hold_of(&catalog, 0x4700_0730);
    assert_eq!(
        (polearm.primary, polearm.secondary, polearm.taken),
        (1, 2, 192)
    );
    let blade = hold_of(&catalog, 0x4700_071F);
    assert_eq!(
        (blade.primary, blade.secondary, blade.valid, blade.taken),
        (1, 0x11, 192, 64)
    );
    assert_eq!(
        hold_of(&catalog, 0x4700_051B),
        bow,
        "the starting bow as a looted one"
    );
    let strung = |t: u32| yalain_world::skill_use::is_bow(catalog.get(t).expect("in the catalog"));
    assert!(strung(0x4700_07A3), "a looted bow is a bow");
    assert!(!strung(0x4700_071F), "a handblade is not");
    let shield = hold_of(&catalog, 0x4700_06FD);
    assert_eq!(
        (shield.valid, shield.preferred, shield.taken, shield.flags),
        (128, 128, 128, flags::WORN)
    );
    let tool = hold_of(&catalog, 0x4700_0CC5);
    assert_eq!(
        (tool.primary, tool.secondary, tool.taken, tool.flags),
        (1, 1, 192, flags::WORN)
    );
}

/// The orbs' three basic attacks read with two thrown hooks each, a hand each.
/// Behaviour: server.weapons.two-throws-read
#[test]
fn the_skills_that_throw_twice_read_with_each_throws_hand() {
    let t = Portal::open().skills();
    for skill in [1883, 1884, 1885] {
        let u = t.skills[&skill].usage.as_ref().expect("used");
        let thrown: Vec<u32> = u
            .hooks
            .iter()
            .filter(|h| h.missile != 0 && h.flags & 0x4 != 0)
            .map(|h| h.flags & 0x3000)
            .collect();
        assert_eq!(thrown.len(), 2, "{skill}");
        assert_ne!(thrown[0], thrown[1], "{skill}: a hand each");
    }
}

/// A hook's damage by rating reads with all its points: a rating of 30 on the middle line.
/// Behaviour: server.specials.hook-adds-by-rating
#[test]
fn a_hooks_points_read_in_full_and_add_along_their_lines() {
    let t = Portal::open().skills();
    let hook = t.skills[&733]
        .usage
        .as_ref()
        .expect("used")
        .hooks
        .iter()
        .find(|h| !h.add_by_rating.is_empty())
        .expect("a hook with points")
        .clone();
    assert_eq!(
        hook.add_by_rating,
        [(1.0, 12.0), (50.0, 260.0), (150.0, 1300.0)]
    );
    assert_eq!(hook.rated_add(1.0), 12);
    assert_eq!(hook.rated_add(30.0), 158);
    assert_eq!(hook.rated_add(100.0), 780);
    assert_eq!(hook.rated_add(200.0), 1300, "held at its last");
}

/// The buff that lowers what skills cost is read with its stat and its line by spellcraft.
/// Behaviour: server.specials.vigor-lowered-by-effects
#[test]
fn the_buff_that_lowers_skills_costs_reads_with_its_stat() {
    let p = Portal::open();
    let t = p.effects(&[0x6F00_1147]);
    let d = t.get(0x6F00_1147).expect("the buff");
    let changes = d.changes(20.0);
    assert_eq!(changes.len(), 1);
    let (stat, multiply, by) = changes[0];
    assert_eq!(stat, yalain_world::effects::VIGOR_COST_ADD);
    assert!(!multiply);
    assert!((by + 22.449).abs() < 0.01, "{by}");
}

/// A weapon's three stances read from its template's qualities.
/// Behaviour: server.weapons.mode-by-stance
#[test]
fn a_weapons_three_stances_read_from_its_template() {
    let t = Portal::open().template(0x4700_051C);
    assert_eq!(t.props.word("Weapon_SingleWeaponStance"), Some(0x4100_000A));
    assert_eq!(t.props.word("Weapon_ShieldStance"), Some(0x4100_000B));
    assert_eq!(t.props.word("Weapon_TwoWeaponStance"), Some(0x4100_000E));
}

/// The auras that raise a weapon's damage read as the data defines them: Infected Caress holds on a
/// weapon and adds 10 at 1, 80 at 50; Wound Twister on any item and multiplies by 1.03 at 1, 1.15 at
/// 50.
/// Behaviour: server.auras.damage-held-on-a-weapon
#[test]
fn the_auras_on_a_weapons_damage_read_as_the_data_defines_them() {
    use yalain_world::effect_defs::aura_target;
    use yalain_world::made::stat;
    const CARESS: u32 = 0x6F00_00CD;
    const TWISTER: u32 = 0x6F00_011C;
    let p = Portal::open();
    let t = p.effects(&[CARESS, TWISTER]);
    let aura = |state: u32| match &t.get(state).expect("the aura").kind {
        Kind::Aura(a) => a.clone(),
        k => panic!("{k:?}"),
    };
    let (caress, twister) = (aura(CARESS), aura(TWISTER));
    assert_eq!(caress.targets, aura_target::WEAPON);
    assert_eq!(twister.targets, aura_target::ITEM);
    let change = |state: u32, at: f32| {
        let d = t.get(state).expect("the pulse");
        d.changes(at)
    };
    assert_eq!(change(caress.pulsed[0], 1.0), [(stat::DAMAGE, false, 10.0)]);
    assert_eq!(
        change(caress.pulsed[0], 50.0),
        [(stat::DAMAGE, false, 80.0)]
    );
    assert_eq!(change(twister.pulsed[0], 1.0), [(stat::DAMAGE, true, 1.03)]);
    assert_eq!(
        change(twister.pulsed[0], 50.0),
        [(stat::DAMAGE, true, 1.15)]
    );
}

/// The templates a pickup's Increment names are attunable in the data: an Attuned Tessera, and a Lost
/// Artifact.
/// Behaviour: server.inventory.an-attunable-item-is-not-put-down
#[test]
fn the_templates_whose_taking_increments_a_quest_are_attunable() {
    let p = Portal::open();
    for did in [
        0x4700_15B1,
        0x4700_16C6,
        0x4700_16CE,
        0x4700_16D0,
        0x4700_16D2,
    ] {
        let t = p.template(did);
        assert!(t.props.flag("Item_IsAttunable"), "{did:#x}");
        assert!(
            t.props
                .text("Item_AcquireEffect1")
                .is_some_and(|e| e.starts_with("Eff_IncrementQuest")),
            "{did:#x}"
        );
    }
}

/// The three Contracts on sale are bought for what their acquiring does, which the world can't do;
/// a template whose acquiring moves a quest is not.
/// Behaviour: server.store.a-sale-the-world-cannot-make-is-not-paid-for
#[test]
fn the_contracts_are_bought_for_what_their_acquiring_does() {
    use yalain_world::quests::acquiring_does_more_than_quests as more;
    let p = Portal::open();
    for did in [0x4700_18D1, 0x4700_18D2, 0x4700_18D3] {
        assert!(more(&p.template(did).props), "{did:#x}");
    }
    assert!(!more(&p.template(0x4700_16C6).props), "a Lost Artifact");
}

/// Immolation's effects turn an alchemist's basic attacks: its plain ones go, the fiery ones of the
/// install come with their rating raised by the buff, and with the dark side on, their melee kind.
/// Behaviour: server.weapons.immolations-attacks
#[test]
fn immolation_turns_an_alchemists_basic_attacks_fiery() {
    use auberean_protocol::{queue, ClientMessage, ServerMessage};
    use auberean_script::args::Arg;
    use yalain_testkit::play::server_call;
    use yalain_testkit::{Harness, TEST_CREATURE};
    use yalain_world::effects::{self, Cast, On};
    use yalain_world::state::SkillState;
    const START_ATTACK: u32 = 0x8204_0008;
    const VIAL: u32 = 0x4700_7A01;
    const IMMOLATION: u32 = 1079;
    const DARK_SIDE: u32 = 0x7000_1F43;

    let p = Portal::open();
    let skills = p.skills();
    let mut states = skills.skills[&IMMOLATION]
        .usage
        .as_ref()
        .expect("used")
        .user_effects
        .clone();
    states.push(DARK_SIDE);
    let table = p.effects(&states);

    let mut h = Harness::new();
    h.add_creatures(1);
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    let beetle = h
        .world_messages(b, 0)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == TEST_CREATURE => Some(o.id),
            _ => None,
        })
        .expect("the camp's creature");
    h.server.skill_table = skills;
    h.server.effect_table = table;
    h.server
        .catalog
        .implements
        .insert(VIAL, yalain_world::skill_use::implement::ALCHEMY);
    h.arm(b, me, VIAL);
    let world = *h.server.players.keys().next().expect("in the world");
    let at = h.server.players[&world].position;
    if let Some(t) = h.server.things.get_mut(beetle) {
        if let Some(c) = t.creature.as_mut() {
            c.stats.movement.perception = 0.0;
            c.stats.scale = 0.0;
        }
        let mut place = at;
        place.origin[0] += 0.5;
        t.at = Some(place);
        t.max_health = 10_000_000;
        t.health = 10_000_000;
    }
    if let Some(p) = h.server.players.get_mut(&world) {
        // A human's saved list holds every one of these, Fire Breath once it is trained.
        for skill in [
            189, 804, 805, 806, 953, 954, 955, 1371, 1372, 1373, 1374, 1375, 1376,
        ] {
            p.play.skills.push(SkillState {
                skill,
                granted: 1,
                mask: 1,
                cost: 0,
                children: 0,
                dependents: 0,
                spent: 0,
            });
        }
        yalain_world::quals::set_int(
            &mut p.qualities,
            yalain_world::player::quality::HEALTH_MAX,
            10_000_000,
        );
        yalain_world::quals::set_int(
            &mut p.qualities,
            yalain_world::player::quality::HEALTH,
            10_000_000,
        );
    }
    let told = |h: &mut Harness| {
        let from = h.bots[b].inbox.len();
        h.bots[b].send(
            true,
            queue::WEENIE,
            &server_call(START_ATTACK, &[Arg::LongInt(beetle)]),
        );
        h.run(400);
        let mut seen: Vec<u32> = h
            .world_messages(b, from)
            .iter()
            .filter_map(|m| match m {
                ServerMessage::DoStory(e) if e.sender.id == me => Some(e.action.skill),
                _ => None,
            })
            .collect();
        seen.sort_unstable();
        seen.dedup();
        h.run(20);
        seen
    };
    let apply = |h: &mut Harness, state: u32| {
        let clock = h.clock();
        assert!(effects::apply_on(
            &mut h.server,
            &mut h.net,
            On::Player(world),
            state,
            Cast {
                spellcraft: 50.0,
                ..Cast::default()
            },
            clock,
        ));
    };
    let rating = |h: &Harness| yalain_world::skill_use::rating_of_for_test(&h.server, world, 1371);
    assert_eq!(told(&mut h), [804, 805, 806]);
    let plain = rating(&h);
    for state in [0x7000_1F51, 0x7000_1F52] {
        apply(&mut h, state);
    }
    assert_eq!(told(&mut h), [1371, 1372, 1373]);
    assert!(rating(&h) >= plain + 40.0, "{plain} to {}", rating(&h));
    apply(&mut h, DARK_SIDE);
    assert_eq!(told(&mut h), [1374, 1375, 1376]);
}

/// The Illuminated Drudge's curse as the install says: it lasts four hours and moves no quest; the
/// other death effect of the template moves a quest.
/// Behaviour: server.creatures.death-effect-on-killer
#[test]
fn the_illuminated_drudges_curse_lasts_four_hours() {
    let p = Portal::open();
    let drudge = 0x4700_2340;
    let named = p.death_effects(&[drudge]);
    let states: Vec<u32> = named[&drudge].iter().map(|e| e.state).collect();
    let table = p.effects(&states);
    let lasting: Vec<f32> = states
        .iter()
        .filter_map(|s| table.get(*s))
        .filter(|d| d.lasts() && !matches!(d.kind, Kind::Quest { .. }))
        .map(|d| d.duration.at(1.0).expect("a duration"))
        .collect();
    assert_eq!(lasting, vec![14_400.0], "{states:08X?}");
}

/// A template that names nothing itself has what its class's code says: boots their armor, delay
/// and race; a weapon its damage, variance, speed and vigor; both their value.
/// Behaviour: server.items.class-properties
#[test]
fn a_template_with_no_properties_has_what_its_class_says() {
    let p = Portal::open();
    let boots = p.template(0x4700_0D69).props;
    assert_eq!(boots.int("Item_ArmorLevel"), Some(10));
    assert_eq!(boots.int("Item_CombatDelay"), Some(4));
    assert_eq!(boots.word("Usage_RequiredRace"), Some(1));
    assert_eq!(boots.int("Item_Value"), Some(10), "the armor classes' base");
    let starter = p.template(0x4700_051B).props;
    assert_eq!(starter.int("Weapon_Damage"), Some(9));
    assert!((starter.float("Weapon_Variance").expect("a variance") - 0.24).abs() < 1e-6);
    assert_eq!(starter.int("Weapon_Speed"), Some(70));
    assert_eq!(starter.int("Item_Value"), Some(1));
    let named = p.template(0x4700_14D0).props;
    assert_eq!(named.int("Weapon_Damage"), Some(240));
    assert_eq!(named.int("Item_Value"), Some(50));
    assert_eq!(
        p.template(0x4700_2FD2).props.int("Item_ArmorLevel"),
        None,
        "a person's class says none"
    );
}

/// A class's own value stands, though it be nothing and its base sets one; a class that sets none
/// takes its base's.
/// Behaviour: server.items.class-properties
#[test]
fn a_classs_value_is_its_own_though_it_be_nothing_else_its_bases() {
    let p = Portal::open();
    let lib = p.library();
    let of = |name: &str| {
        let package = lib.package_id(name).expect("a package");
        yalain_world_db::install::class_properties(&lib, package)
            .into_iter()
            .find(|(n, _)| *n == "Item_Value")
            .map(|(_, v)| v)
    };
    use yalain_content::Prop;
    assert_eq!(of("DrumTumerokStarter"), Some(Prop::Int(1)), "its own");
    assert_eq!(
        of("PotionTrainingHallHealing"),
        Some(Prop::Int(10)),
        "its base's"
    );
    assert_eq!(
        of("DoctrineDominion"),
        Some(Prop::Int(0)),
        "nothing, its own"
    );
}

/// Each glyph is named, though its entity says nothing and only its qualities record does, as each
/// key's answer asks of it; and so is a piece of treasure kept the same way.
/// Behaviour: server.places.a-keys-answer-names-it
#[test]
fn a_glyph_and_treasure_are_named_though_only_their_qualities_say() {
    let p = Portal::open();
    for (t, _) in yalain_world::usable::vaults::GLYPH_KEYS {
        assert!(p.template(t).facts.name.is_some(), "{t:#x}");
    }
    assert_eq!(
        p.template(0x4700_10A2).facts.name.map(|n| n.0),
        Some(0xA860_7CEC)
    );
}

/// A template whose least level is only its class's isn't one that sets it, as one whose entity says.
/// Behaviour: server.loot.treasure-own-properties-over-a-class
#[test]
fn only_a_templates_own_entity_sets_its_least_level() {
    let p = Portal::open();
    for t in [0x4700_1DA1, 0x4700_0E9C] {
        let template = p.template(t);
        assert!(template.props.int("Usage_MinLevel").is_some(), "{t:#x}");
        assert!(!template.props.sets_least_level(), "{t:#x}");
    }
    assert!(p.template(0x4700_2117).props.sets_least_level());
}

/// A corpse's armor and weapon of such templates are treasure of the creature's level: a least level
/// of its own, and numbers by it; made anew, any other keeps the least level its class says.
/// Behaviour: server.loot.treasure-own-properties-over-a-class
#[test]
fn a_corpses_armor_and_weapon_of_a_class_with_a_least_level_are_made_by_the_creatures_level() {
    use yalain_testkit::Harness;
    use yalain_world::things::Mode;
    let p = Portal::open();
    for t in [0x4700_1DA1, 0x4700_0E9C] {
        let template = p.template(t);
        let mut h = Harness::new();
        h.add_creatures(1);
        h.server
            .treasure
            .equipment
            .insert(template.description.package);
        h.server.catalog.insert(template.clone());
        for profile in &mut h.server.loot.profiles {
            profile.items = vec![(t, 1)];
            profile.counts = vec![0.0, 1.0];
        }
        let (_, me) = h.player_in_world("one", "Kestrel");
        h.run(20);
        let beetle = h
            .server
            .things
            .by_id
            .values()
            .find(|x| x.creature.as_ref().is_some_and(|c| c.mode != Mode::Dead))
            .map(|x| x.id)
            .expect("a beetle");
        let clock = h.clock();
        yalain_world::combat::hurt_creature(
            &mut h.server,
            &mut h.net,
            beetle,
            me,
            1_000_000,
            clock,
        );
        assert!(
            h.run_until(200, |h| h
                .server
                .things
                .by_id
                .values()
                .any(|x| x.template == t)),
            "{t:#x} dropped"
        );
        let own = h
            .server
            .things
            .by_id
            .values()
            .find(|x| x.template == t)
            .expect("in the corpse")
            .own;
        let least = own.min_level.expect("its own least level");
        assert!((2..=5).contains(&least), "{t:#x} at level 2: {least}");
        assert!(own.made.is_some(), "{t:#x} has numbers of its level");
        let plain = yalain_world::inventory::make_item(&mut h.server, t, 1, 0).expect("made");
        let thing = h.server.things.get(plain).expect("made");
        assert_eq!(thing.own.min_level, None);
        let view = yalain_world::requests::item_view(&h.server, thing);
        assert_eq!(
            i64::from(view.min_level),
            template.props.int("Usage_MinLevel").expect("the class's")
        );
    }
}

fn corpse_rot_of(p: &yalain_testkit::authored::Portal, template: u32) -> yalain_content::CorpseRot {
    yalain_content::CorpseRot::from_props(&p.template(template).props)
}

/// A creature, a pet and a talking creature's corpses take their classes' times, written in the
/// install's code; a template that sets its own takes those, and its class's for what it leaves out.
/// Behaviour: server.corpse.rot-times-from-the-install
#[test]
fn a_corpses_times_are_its_templates_over_its_classes() {
    let p = yalain_testkit::authored::Portal::open();
    let times = |r: yalain_content::CorpseRot| (r.untouched, r.items_left, r.empty);
    for ordinary in [0x4700_2AF6, 0x4700_083D, 0x4700_21BA] {
        let props = p.template(ordinary).props;
        assert_eq!(
            (
                props.float("NPC_BaseRotTime"),
                props.float("NPC_MaxAcceleratedRotTime"),
                props.float("NPC_MinAcceleratedRotTime"),
            ),
            (Some(180.0), Some(60.0), Some(15.0)),
            "{ordinary:08X}"
        );
        assert_eq!(times(corpse_rot_of(&p, ordinary)), (180.0, 60.0, 15.0));
    }
    for (template, want) in [
        (0x4700_3440, (180.0, 2.0, 1.0)),
        (0x4700_26C2, (180.0, 180.0, 180.0)),
        (0x4700_26C9, (180.0, 1790.0, 1790.0)),
        (0x4700_26DA, (15.0, 60.0, 15.0)),
    ] {
        assert_eq!(times(corpse_rot_of(&p, template)), want, "{template:08X}");
    }
}
