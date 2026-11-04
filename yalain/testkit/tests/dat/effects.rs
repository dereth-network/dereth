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
    let mut states: Vec<u32> = yalain_world::pets::KINDS.iter().map(|k| k.effect).collect();
    states.push(0x7000_0830);
    let t = p.effects(&states);
    let lasher = t.get(0x7000_0805).expect("the Lasher's");
    let Kind::Pet { class, lifetime } = &lasher.kind else {
        panic!("not a pet: {lasher:?}")
    };
    assert_eq!((*class, lifetime.is_empty()), (16, true));
    for k in yalain_world::pets::KINDS {
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
