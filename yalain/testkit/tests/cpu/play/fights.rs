//! Fights: a player's attack run on a creature, the creature's death, the experience and level it
//! pays, its corpse and what the corpse holds; taking from it and turning it into money.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_testkit::play::server_call;
use yalain_testkit::{Harness, TEST_CREATURE, TEST_ITEM};
use yalain_world::calls::{self, Value};
use yalain_world::graph::{Builder, Slot};
use yalain_world::levels::{Level, LevelTable};

const START_ATTACK: u32 = 0x8204_0008;
const ATTACK_STATE: u32 = 0x8204_001B;
const USE: u32 = 0x8216_0016;
const OPEN_DONE: u32 = 0x820E_005D;
const CLOSE_DONE: u32 = 0x820E_0060;
const MOVE_DONE: u32 = 0x820E_005E;
const CLOSE: u32 = 0x820E_000C;
const USE_DONE: u32 = 0x8216_0021;
const TRANSMUTE_ALL: u32 = 0x820E_0065;
const TRANSMUTE_ALL_DONE: u32 = 0x820E_006A;
const CORPSE: u32 = 0x4700_0539;
/// The human starting sword (a player attacks with a weapon in hand).
const STARTER_SWORD: u32 = 0x4700_051C;

/// Made-up levels for the tests: level 2 at 10 experience.
fn levels() -> LevelTable {
    LevelTable {
        levels: vec![
            Level {
                xp: 0,
                health: 100,
                vigor: 100,
                skill_credits: 0,
                hero_credits: 0,
            },
            Level {
                xp: 10,
                health: 130,
                vigor: 130,
                skill_credits: 1,
                hero_credits: 0,
            },
            Level {
                xp: 1000,
                health: 160,
                vigor: 160,
                skill_credits: 1,
                hero_credits: 0,
            },
        ],
    }
}

fn scene_drawn(h: &mut Harness, b: usize) {
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
}

fn creature_of(h: &Harness, b: usize) -> u64 {
    h.world_messages(b, 0)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == TEST_CREATURE => Some(o.id),
            _ => None,
        })
        .expect("the camp's creature is made")
}

/// The calls a client received since `from`: their functions and values.
fn calls_of(h: &Harness, b: usize, from: usize) -> Vec<(u32, Vec<Value>)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) => Some((
                c.function.0,
                c.args
                    .as_ref()
                    .and_then(|a| calls::values(&a.0).ok())
                    .unwrap_or_default(),
            )),
            _ => None,
        })
        .collect()
}

/// A use of an object as the client asks for it.
fn use_of(item: u64, player: u64) -> Arg {
    let mut b = Builder::new();
    b.object(
        0x245,
        &[
            Slot::Ref(None),
            Slot::Ref(None),
            Slot::Long(item),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Long(player),
            Slot::Int((-1.0f32).to_bits()),
            Slot::Long(0),
            Slot::Int(0),
            Slot::Long(0),
            Slot::Int(0),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Int(316),
            Slot::Int(0),
            Slot::Int(0),
        ],
    );
    Arg::Object(b.graph())
}

/// A world with one beetle beside a new player, which attacks it until it dies; returns the
/// harness, the client, the player and the beetle.
fn fight() -> (Harness, usize, u64, u64) {
    let mut h = Harness::new();
    h.add_creatures(1);
    h.server.levels = levels();
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    h.arm(b, me, STARTER_SWORD);
    let beetle = creature_of(&h, b);
    // The run is the claim, not who wins it: the beetle hits too softly to kill the player first.
    if let Some(c) = h
        .server
        .things
        .get_mut(beetle)
        .and_then(|t| t.creature.as_mut())
    {
        c.stats.natural_damage = 1.0;
    }
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(START_ATTACK, &[Arg::LongInt(beetle)]),
    );
    let dead = h.run_until(4000, |h| {
        h.world_messages(b, 0).iter().any(|m| {
            matches!(m, ServerMessage::UpdateBoolVisual(q) if q.object.id == beetle && q.quality == 279 && q.value == 1)
        })
    });
    assert!(dead, "the beetle dies");
    (h, b, me, beetle)
}

/// Asked once to attack, the server answers that the player attacks and swings for it, each swing
/// a story from the player with a hit or an evade on the target, until the target dies; its
/// killer is paid its death experience (11 for a level 2 beetle at level 1) and reaches level 2,
/// and everyone who sees it sees it dead.
/// Behaviour: server.combat.attack-run-to-the-death
#[test]
fn an_attack_run_swings_until_the_creature_dies_and_pays_its_experience() {
    let (h, b, me, beetle) = fight();
    let calls = calls_of(&h, b, 0);
    assert!(
        calls
            .iter()
            .any(|(f, v)| *f == ATTACK_STATE && v == &vec![Value::Int(1)]),
        "the player is attacking"
    );
    let swings = h
        .world_messages(b, 0)
        .iter()
        .filter(|m| matches!(m, ServerMessage::DoStory(e) if e.action.attacker == me))
        .count();
    assert!(swings >= 2, "it swings again and again: {swings}");
    let xp: Vec<u64> = h
        .world_messages(b, 0)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateLongInt(q) if q.quality == 300 => Some(q.value),
            _ => None,
        })
        .collect();
    assert_eq!(xp, vec![11], "the beetle's death experience");
    let level = h.world_messages(b, 0).iter().find_map(|m| match m {
        ServerMessage::UpdateInt(q) if q.quality == 302 => Some(q.value),
        _ => None,
    });
    assert_eq!(level, Some(2), "past 10 experience: level 2");
    let _ = beetle;
}

/// A player's swing at a creature is told first; the creature loses the health as the strike
/// lands, its skill's time over its time scale later, and only then does it fall.
/// Behaviour: server.combat.strike-lands
#[test]
fn a_players_swing_hurts_a_creature_as_it_lands_by_its_skill() {
    let mut h = Harness::new();
    h.add_creatures(1);
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    h.arm(b, me, STARTER_SWORD);
    let beetle = creature_of(&h, b);
    let thing = h.server.things.get_mut(beetle).expect("the beetle");
    thing.health = 1;
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(START_ATTACK, &[Arg::LongInt(beetle)]),
    );
    let hit = |m: &ServerMessage| {
        matches!(m, ServerMessage::DoStory(e) if e.action.attacker == me
            && e.action.hooks.iter().any(|k| k.target == beetle && k.bits & 0x800 != 0))
    };
    assert!(
        h.run_until(4000, |h| h.world_messages(b, from).iter().any(hit)),
        "a hit is told"
    );
    let told = h.now.0;
    let (skill, scale) = h
        .world_messages(b, from)
        .iter()
        .find_map(|m| match m {
            ServerMessage::DoStory(e) if hit(m) => {
                Some((e.action.skill, e.action.attacker_behavior.time_scale))
            }
            _ => None,
        })
        .expect("its story");
    let sword = Some(yalain_world::skill_use::implement::SWORD);
    let lands = yalain_world::combat::hit_seconds(skill, sword) / f64::from(scale);
    let alive = |h: &Harness| h.server.things.get(beetle).is_some_and(|t| t.health > 0);
    assert!(alive(&h), "not hurt as it is told");
    let dead = |m: &ServerMessage| {
        matches!(m, ServerMessage::UpdateBoolVisual(q)
            if q.object.id == beetle && q.quality == 279 && q.value == 1)
    };
    assert!(
        h.run_until(100, |h| h.world_messages(b, from).iter().any(dead)),
        "it lands"
    );
    let gap = h.now.0 - told;
    assert!(
        (gap - lands).abs() < 0.12,
        "it landed {gap:.3} s after its story, not {lands:.3} s"
    );
}

/// A strike lands at its own skill's time; a skill seen too little by what is held.
/// Behaviour: server.combat.strike-lands
#[test]
fn a_strike_lands_at_its_skills_own_time() {
    use yalain_world::combat::hit_seconds;
    use yalain_world::skill_use::implement::{BOW, STAVE, SWORD};
    assert_eq!(hit_seconds(690, Some(SWORD)), 0.552);
    assert_eq!(hit_seconds(691, Some(SWORD)), 0.785);
    assert_eq!(hit_seconds(617, Some(STAVE)), 1.056);
    assert_eq!(hit_seconds(816, Some(SWORD)), 0.777);
    assert_eq!(hit_seconds(817, Some(SWORD)), 0.844);
    assert_eq!(hit_seconds(818, Some(SWORD)), 0.780);
    assert_eq!(hit_seconds(955, Some(SWORD)), 0.732);
    assert_eq!(hit_seconds(1, Some(BOW)), 0.572);
    assert_eq!(hit_seconds(1, Some(STAVE)), 0.817);
    assert_eq!(hit_seconds(1, Some(SWORD)), 0.713);
}

/// A practice target (a creature that never fights back, as the halls' Straw Drudges) is attacked
/// like any other: the run is answered and swings at it, and it does not strike back.
/// Behaviour: server.combat.practice-targets-are-struck
#[test]
fn a_practice_target_is_struck_and_never_strikes_back() {
    let mut h = Harness::new();
    h.add_creatures(1);
    h.server.levels = levels();
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    h.arm(b, me, STARTER_SWORD);
    let dummy = creature_of(&h, b);
    let mut beside = h
        .server
        .players
        .values()
        .next()
        .expect("the player")
        .position;
    beside.origin[0] += 1.5;
    if let Some(t) = h.server.things.get_mut(dummy) {
        t.at = Some(beside);
        t.max_health = 25_000;
        t.health = 25_000;
        if let Some(c) = t.creature.as_mut() {
            c.stats.fights = false;
        }
    }
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(START_ATTACK, &[Arg::LongInt(dummy)]),
    );
    h.run(200);
    let calls = calls_of(&h, b, from);
    assert!(
        calls
            .iter()
            .any(|(f, v)| *f == ATTACK_STATE && v == &vec![Value::Int(1)]),
        "the player is attacking: {calls:x?}"
    );
    let msgs = h.world_messages(b, from);
    let swings = msgs
        .iter()
        .filter(|m| matches!(m, ServerMessage::DoStory(e) if e.action.attacker == me))
        .count();
    assert!(swings >= 3, "it swings again and again: {swings}");
    assert!(
        !msgs
            .iter()
            .any(|m| matches!(m, ServerMessage::DoStory(e) if e.action.attacker == dummy)),
        "the target never strikes back"
    );
}

/// Two seconds after its death the creature's corpse appears where it fell, holding what its loot
/// profile leaves; three seconds later the creature leaves the world.
/// Behaviour: server.death.corpse-and-leaving
#[test]
fn a_dead_creature_leaves_a_corpse_with_its_loot_and_then_leaves() {
    let (mut h, b, _, beetle) = fight();
    let corpse = h.run_until(200, |h| {
        h.world_messages(b, 0)
            .iter()
            .any(|m| matches!(m, ServerMessage::CreateObject(o) if o.weenie.entity_did == CORPSE))
    });
    assert!(corpse, "a corpse appears");
    let left = h.run_until(200, |h| {
        h.world_messages(b, 0)
            .iter()
            .any(|m| matches!(m, ServerMessage::ForgetObject(o) if o.0.id == beetle))
    });
    assert!(left, "the creature leaves");
}

/// An item drawn from a creature's loot profile carries what its dropper's level gives it of its
/// own: a piece of equipment dropped by the level 2 beetle (no loot quality of its own) a least
/// level of 2 to 5 and no Arcane Lore.
/// Behaviour: server.loot.treasure-own-properties
#[test]
fn equipment_a_creature_drops_carries_a_least_level_of_its_own_by_the_creatures_level() {
    let (mut h, b, _, _) = fight();
    h.server.treasure.equipment.insert(0x200);
    assert!(h.run_until(200, |h| h.world_messages(b, 0).iter().any(
        |m| matches!(m, ServerMessage::CreateObject(o) if o.weenie.entity_did == CORPSE)
    )));
    let own: Vec<_> = h
        .server
        .things
        .by_id
        .values()
        .filter(|t| t.template == TEST_ITEM)
        .map(|t| t.own)
        .collect();
    assert!(!own.is_empty(), "the corpse holds the item");
    for o in own {
        let least = o.min_level.expect("its own least level");
        assert!((2..=5).contains(&least), "{least}");
        assert_eq!(o.arcane_lore, None, "no Arcane Lore below a base of 52");
        assert_eq!(o.effect, None);
    }
}

/// Jewelry a creature leaves is worth what any treasure of its level is, whatever its template says
/// (2017: the same bands, 198 pieces by their droppers' levels, against a template's 10).
/// Behaviour: server.loot.treasure-value-by-level
#[test]
fn jewelry_a_creature_leaves_is_worth_what_treasure_of_its_level_is() {
    let (mut h, b, _, _) = fight();
    h.server.treasure.jewelry.insert(0x200);
    if let Some(t) = h.server.catalog.templates.get_mut(&TEST_ITEM) {
        t.props.set("Item_Value", yalain_content::Prop::Int(10));
    }
    assert!(h.run_until(200, |h| h.world_messages(b, 0).iter().any(
        |m| matches!(m, ServerMessage::CreateObject(o) if o.weenie.entity_did == CORPSE)
    )));
    let values: Vec<_> = h
        .server
        .things
        .by_id
        .values()
        .filter(|t| t.template == TEST_ITEM)
        .map(|t| t.value)
        .collect();
    assert!(!values.is_empty(), "the corpse holds the piece");
    for v in values {
        assert!((1..=4).contains(&v), "a level 2 creature's: {v}");
    }
}

/// A least level only its class says doesn't stop a drop having its own, and numbers by it; one its
/// template sets does.
/// Behaviour: server.loot.treasure-own-properties-over-a-class
#[test]
fn a_least_level_only_its_class_says_gives_way_to_a_drops_own() {
    use yalain_content::{Prop, Props};
    let dropped = |props: Props| {
        let (mut h, b, _, _) = fight();
        h.server.treasure.equipment.insert(0x200);
        if let Some(t) = h.server.catalog.templates.get_mut(&TEST_ITEM) {
            t.props = props;
        }
        assert!(h.run_until(200, |h| h.world_messages(b, 0).iter().any(
            |m| matches!(m, ServerMessage::CreateObject(o) if o.weenie.entity_did == CORPSE)
        )));
        let mut own: Vec<_> = h
            .server
            .things
            .by_id
            .values()
            .filter(|t| t.template == TEST_ITEM)
            .map(|t| t.own)
            .collect();
        own.pop().expect("the corpse holds the item")
    };
    let mut armor = Props::default();
    armor.set("Item_ArmorLevel", Prop::Int(10));
    let class = dropped(
        armor
            .clone()
            .with_class(&[("Usage_MinLevel", Prop::Int(15))]),
    );
    let least = class.min_level.expect("its own least level");
    assert!((2..=5).contains(&least), "a level 2 creature's: {least}");
    assert!(
        class.made.and_then(|m| m.armor).is_some(),
        "its own numbers"
    );
    armor.set("Usage_MinLevel", Prop::Int(15));
    let set = dropped(armor);
    assert_eq!((set.min_level, set.made), (None, None));
}

/// A treasure weapon, shield or piece of armor has numbers of its own by its level, where its template
/// holds a placeholder: a weapon's damage near a made one's, its delay and vigor among the recorded;
/// armor's rating; a robe's three slots; anything else nothing.
/// Behaviour: server.loot.treasure-numbers-by-level
#[test]
fn a_treasure_items_numbers_follow_its_level() {
    use yalain_content::{Prop, Props};
    use yalain_world::rng::Rng;
    use yalain_world::treasure::numbers;
    let mut weapon = Props::default();
    weapon
        .set("Weapon_Damage", Prop::Int(10))
        .set("Weapon_Variance", Prop::Float(0.25));
    let mut robe = Props::default();
    robe.set("Item_ArmorLevel", Prop::Int(10))
        .set("Item_ValidInventoryLocations", Prop::Word(2))
        .set("Item_PrecludedInventoryLocations", Prop::Word(8));
    let mut shield = weapon.clone();
    shield.set("Item_ArmorLevel", Prop::Int(10));
    let mut rng = Rng::new(7);
    assert_eq!(numbers(&mut rng, &Props::default(), 20, 9), None);
    for _ in 0..500 {
        let w = numbers(&mut rng, &weapon, 20, 9).expect("a weapon");
        let (most, variance) = w.damage.expect("its damage");
        assert!((88..=102).contains(&most), "at level 20: {most}");
        assert!((variance - 0.25).abs() < f32::EPSILON);
        assert!((4..=19).contains(&w.delay.expect("a delay")));
        assert!((4..=5).contains(&w.vigor.expect("a vigor cost")));
        assert_eq!((w.value, w.slots, w.armor), (9, 1, None));
        let high = numbers(&mut rng, &weapon, 50, 9).expect("a weapon");
        let (most, _) = high.damage.expect("its damage");
        assert!((264..=306).contains(&most), "at level 50: {most}");
        let r = numbers(&mut rng, &robe, 20, 9).expect("armor");
        assert!((13..=25).contains(&r.armor.expect("a rating")), "{r:?}");
        assert_eq!((r.damage, r.slots, r.covers), (None, 3, 8));
        let sh = numbers(&mut rng, &shield, 20, 9).expect("a shield");
        assert!(sh.armor.is_some() && sh.damage.is_some(), "{sh:?}");
        assert!((5..=11).contains(&sh.delay.expect("a delay")));
    }
}

/// A weapon drawn from a loot profile takes those numbers as it drops, worth what it drew, and a hit
/// with it reads them.
/// Behaviour: server.loot.treasure-numbers-by-level
#[test]
fn a_weapon_a_creature_drops_carries_its_levels_numbers() {
    use yalain_content::{Prop, Props};
    const SWORD: u32 = 0x4700_7003;
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    let mut props = Props::default();
    props
        .set("Weapon_Damage", Prop::Int(10))
        .set("Weapon_Variance", Prop::Float(0.25));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(SWORD, 0x200, props));
    h.server.treasure.equipment.insert(0x200);
    let item = yalain_world::inventory::make_item(&mut h.server, SWORD, 1, 9).expect("made");
    yalain_world::treasure::endow(&mut h.server, item, 20, None);
    let t = h.server.things.get(item).expect("made");
    let least = t.own.min_level.expect("its own least level");
    assert!((20..=23).contains(&least), "{least}");
    let m = t.own.made.expect("its numbers");
    assert_eq!(m.value, 9);
    let hit = yalain_world::combat::weapon_damage(&h.server, item).expect("a weapon");
    assert!(
        (88..=125).contains(&hit.most),
        "not its template's 10: {}",
        hit.most
    );
}

/// A weapon of a template known to draw its look draws its glow as it drops, within what 2017's
/// plain weapons show: it comes in the item's look, it changes its icon, and it is kept with the item.
/// Behaviour: server.loot.treasure-look-drawn
#[test]
fn a_weapon_a_creature_drops_draws_its_look() {
    use yalain_content::treasure_looks::{GLOW, TREASURE_LOOKS};
    use yalain_content::IconTable;
    let (template, table, _) = *TREASURE_LOOKS
        .iter()
        .find(|(_, _, k)| k[0].0 == GLOW)
        .expect("a weapon's row");
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    h.server.catalog.insert(yalain_testkit::test_template(
        template,
        0x200,
        yalain_content::Props::default(),
    ));
    h.server.treasure.equipment.insert(0x200);
    h.server.catalog.icons.insert(template, vec![[1, 7, 8]]);
    h.server.catalog.icon_tables.insert(
        table,
        vec![IconTable {
            keys: vec![(GLOW, vec![(0.0, vec![[1, 0, 9]])])],
        }],
    );
    let mut seen = Vec::new();
    for _ in 0..200 {
        let item = yalain_world::inventory::make_item(&mut h.server, template, 1, 9).expect("made");
        yalain_world::treasure::endow(&mut h.server, item, 20, None);
        let t = h.server.things.get(item).expect("made");
        let look = t.own.look.expect("it draws one");
        assert_eq!(look.table, table);
        let (key, value) = look.keys().next().expect("a key");
        assert_eq!(key, GLOW);
        seen.push(value);
        let create = yalain_world::inventory::item_create(t);
        let (shown, values) = create
            .visual
            .global
            .appearance
            .entries
            .first()
            .expect("its look's values");
        assert_eq!(*shown, table);
        assert!(values
            .entries
            .iter()
            .any(|(k, v)| *k == GLOW && *v == value));
        assert_eq!(
            yalain_world::inventory::icon_of(&h.server, t),
            vec![[1, 7, 9]]
        );
    }
    assert!(seen.iter().all(|v| (0.0..=0.65).contains(v)), "{seen:?}");
    let least = seen.iter().copied().fold(1.0, f32::min);
    let most = seen.iter().copied().fold(0.0, f32::max);
    assert!(least < 0.1 && most > 0.5, "it spreads: {least} to {most}");
}

/// A color drawn stays within its key's highest, in hundredths, and a piece worn shows what it drew,
/// until a dye takes its place.
/// Behaviour: server.loot.treasure-look-drawn
#[test]
fn a_piece_of_armor_draws_its_colors_within_their_keys_and_wears_them() {
    use yalain_content::treasure_looks::TREASURE_LOOKS;
    use yalain_content::{Prop, Props};
    use yalain_world::treasure::{self, Own};
    let (template, table, keys) = *TREASURE_LOOKS
        .iter()
        .find(|(_, _, k)| k.len() == 2)
        .expect("a two-colored row");
    let mut rng = yalain_world::rng::Rng::new(7);
    let mut props = Props::default();
    props
        .set("Item_WornAppearanceFile1", Prop::Word(0x2000_0001))
        .set("Item_WornAppearanceRace1", Prop::Word(1))
        .set("Item_WornAppearanceSex1", Prop::Word(0x1000));
    for _ in 0..200 {
        let look = treasure::draw_look(&mut rng, template).expect("it draws one");
        assert_eq!(look.table, table);
        for (key, top) in keys {
            let (_, v) = look.keys().find(|(k, _)| k == key).expect("the key");
            assert!((0.0..=*top).contains(&v), "{v} above {top}");
            assert!(((v * 100.0).round() - v * 100.0).abs() < 1e-3, "{v}");
        }
        let own = Own {
            look: Some(look),
            ..Own::default()
        };
        let (_, worn) = yalain_world::equip::worn_look(&props, 1, 0x1000, &own).expect("worn");
        for (k, v) in look.keys() {
            assert!(worn.entries.contains(&(k, v)), "{k:x}");
        }
        let dyed = Own {
            dye: Some((table, 0.5)),
            ..own
        };
        let (_, worn) = yalain_world::equip::worn_look(&props, 1, 0x1000, &dyed).expect("worn");
        assert!(worn.entries.contains(&(2, 0.5)), "the dye's");
        assert!(!worn.entries.iter().any(|(k, v)| *k == 2 && *v != 0.5));
    }
}

/// How many enchantments an item draws goes by its sort and its level; a family of five gives the
/// tier of the level, and no family comes twice.
/// Behaviour: server.loot.treasure-enchantments
#[test]
fn an_items_enchantments_follow_its_sort_and_level() {
    use yalain_content::treasure_effects::{self as t, When};
    use yalain_world::treasure::{enchantments, Sort};
    let mut rng = yalain_world::rng::Rng::new(11);
    let share = |rng: &mut yalain_world::rng::Rng, sort: Sort, level: i32| {
        let draws: Vec<_> = (0..4000).map(|_| enchantments(rng, sort, level)).collect();
        let some = draws.iter().filter(|d| !d.is_empty()).count();
        #[allow(clippy::cast_precision_loss)]
        (some as f32 / 4000.0, draws.iter().map(Vec::len).max())
    };
    let (low, most) = share(&mut rng, Sort::Weapon, 5);
    assert!((0.12..0.21).contains(&low), "{low}");
    assert_eq!(most, Some(1), "none has two so low");
    let (high, most) = share(&mut rng, Sort::Weapon, 50);
    assert!((0.27..0.37).contains(&high), "{high}");
    assert!(
        most >= Some(3) && most <= Some(5),
        "three draws, two of them perhaps pairs"
    );
    let (armor, _) = share(&mut rng, Sort::Armor, 50);
    assert!((0.29..0.39).contains(&armor), "{armor}");
    let (shield, _) = share(&mut rng, Sort::Shield, 10);
    assert!((0.04..0.14).contains(&shield), "{shield}");
    let tiered: Vec<&t::Family> = t::WEAPON_FAMILIES.iter().filter(|f| f.by_level).collect();
    assert!(tiered.iter().all(|f| f.tiers.len() == 5));
    let (mut at_ten, mut at_47) = (0, 0);
    for _ in 0..4000 {
        for (when, tier) in enchantments(&mut rng, Sort::Weapon, 10) {
            if let Some(f) = tiered.iter().find(|f| f.tiers.contains(&tier)) {
                assert_eq!(tier, f.tiers[0], "the first tier at level 10");
                assert_eq!(when, f.when);
                at_ten += 1;
            }
        }
        let drawn = enchantments(&mut rng, Sort::Weapon, 47);
        for (i, (_, tier)) in drawn.iter().enumerate() {
            let f = t::WEAPON_FAMILIES
                .iter()
                .position(|f| f.tiers.contains(tier))
                .expect("a family");
            for (_, other) in &drawn[i + 1..] {
                assert!(
                    !t::WEAPON_FAMILIES[f].tiers.contains(other),
                    "no family twice"
                );
            }
            if let Some(f) = tiered.iter().find(|f| f.tiers.contains(tier)) {
                assert_eq!(*tier, f.tiers[4], "the last tier at level 47");
                at_47 += 1;
            }
        }
    }
    assert!(at_ten > 100 && at_47 > 100, "{at_ten} {at_47}");
    assert_eq!(
        When::Hit,
        t::tier_of(t::WEAPON_FAMILIES[0].tiers[0].state)
            .expect("known")
            .0
    );
}

/// A weapon that draws an enchantment glows as that enchantment says, and carries it to be cast at
/// one below its level; one on itself changes what it shows.
/// Behaviour: server.loot.treasure-enchantments
#[test]
fn a_weapon_with_an_enchantment_glows_as_it_says() {
    use yalain_content::treasure_effects::{tier_of, When};
    use yalain_content::treasure_looks::{GLOW, TREASURE_LOOKS};
    use yalain_content::{Prop, Props};
    let (template, ..) = *TREASURE_LOOKS
        .iter()
        .find(|(_, _, k)| k[0].0 == GLOW)
        .expect("a weapon's row");
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    let mut props = Props::default();
    props
        .set("Weapon_Damage", Prop::Int(10))
        .set("Weapon_Variance", Prop::Float(0.25));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(template, 0x200, props));
    h.server.treasure.equipment.insert(0x200);
    let (mut plain, mut enchanted) = (0, 0);
    for _ in 0..600 {
        let item = yalain_world::inventory::make_item(&mut h.server, template, 1, 9).expect("made");
        yalain_world::treasure::endow(&mut h.server, item, 50, None);
        let t = h.server.things.get(item).expect("made");
        let word = t.own.made.expect("its numbers").prefix;
        assert!(
            yalain_content::treasure_words::WEAPON_WORDS
                .iter()
                .any(|w| w.word == word),
            "{word:x} a weapon's word"
        );
        let glow = t
            .own
            .look
            .and_then(|l| l.keys().find(|(k, _)| *k == GLOW))
            .expect("a glow")
            .1;
        let of_it: Vec<_> = t
            .enchantments
            .iter()
            .chain(&t.bonuses)
            .map(|(state, sc)| {
                assert_eq!(*sc, 49.0, "one below its level");
                tier_of(*state).expect("a known effect")
            })
            .collect();
        for (when, tier) in &of_it {
            let on_wearer = t.bonuses.contains(&(tier.state, 49.0));
            assert_eq!(*when == When::Equipped, on_wearer);
        }
        let first = t
            .enchantments
            .iter()
            .chain(&t.bonuses)
            .filter_map(|(state, _)| tier_of(*state))
            .map(|(when, tier)| (when != When::Permanent, tier.glow))
            .filter(|(_, g)| *g > 0.0)
            .min_by_key(|(later, _)| *later)
            .map(|(_, g)| g);
        if let Some(first) = first {
            enchanted += 1;
            assert_eq!(glow, first, "the first that gives one");
        } else {
            plain += 1;
            assert!(glow <= yalain_world::treasure::GLOW_DRAW, "{glow}");
        }
    }
    assert!(plain > 300 && enchanted > 60, "{plain} {enchanted}");
}

/// An effect on the item itself changes the numbers it says: a combat delay no lower than 1.
/// Behaviour: server.loot.treasure-enchantments
#[test]
fn a_permanent_enchantment_changes_the_items_numbers() {
    use yalain_world::effect_defs::{EffectDef, Kind, Scale};
    use yalain_world::made::{stat, Made};
    use yalain_world::treasure::{apply_numbers, Own};
    let def = |which: u32, by: f32| EffectDef {
        state: 1,
        kind: Kind::Numeric {
            flags: 0,
            stats: vec![which],
            magnitude: Scale(vec![(0.0, by)]),
        },
        duration: Scale::default(),
        priority: Scale::default(),
        fx: 0,
        external_flags: 0,
        internal_flags: 0,
        enum_value: 0,
        eq_class: 0,
        name: None,
    };
    let mut own = Own {
        min_level: Some(20),
        made: Some(Made {
            damage: Some((100, 0.25)),
            armor: Some(10),
            delay: Some(11),
            vigor: Some(15),
            ..Made::default()
        }),
        ..Own::default()
    };
    apply_numbers(&mut own, &def(stat::COMBAT_DELAY, -10.0), 20.0);
    apply_numbers(&mut own, &def(stat::COMBAT_DELAY, -10.0), 20.0);
    apply_numbers(&mut own, &def(stat::VIGOR, -4.0), 20.0);
    apply_numbers(&mut own, &def(stat::ARMOR, 3.0), 20.0);
    apply_numbers(&mut own, &def(stat::DAMAGE, 2.0), 20.0);
    apply_numbers(&mut own, &def(stat::LEAST_LEVEL, -1.0), 20.0);
    let m = own.made.expect("made");
    assert_eq!(m.delay, Some(1));
    assert_eq!(m.vigor, Some(11));
    assert_eq!(m.armor, Some(13));
    assert_eq!(m.damage, Some((102, 0.25)));
    assert_eq!(own.min_level, Some(19));
    apply_numbers(&mut own, &def(stat::SPAN, -0.0625), 20.0);
    assert_eq!(own.made.and_then(|m| m.damage), Some((102, 0.1875)));
}

/// The rarer enchantments are drawn: a slayer and an edge on a weapon, a ward on who wields it, a
/// skill's boost on who wears a piece of armor; each only from the level that reaches it, and rarely.
/// Behaviour: server.loot.treasure-rarer-enchantments
#[test]
fn the_rarer_enchantments_are_drawn_rarely_and_from_their_levels() {
    use yalain_content::treasure_effects::{self as t, When};
    use yalain_world::treasure::{enchantments, Sort};
    let mut rng = yalain_world::rng::Rng::new(3);
    let family = |families: &'static [t::Family], least: i32, when: When| {
        families
            .iter()
            .find(|f| f.when == when && f.tiers[0].least == least && !f.by_level)
            .expect("a family")
    };
    let slayers = family(t::WEAPON_FAMILIES, 17, When::Hit);
    let wards = family(t::WEAPON_FAMILIES, 50, When::Equipped);
    let boosts = family(t::ARMOR_FAMILIES, 0, When::Equipped);
    assert!(slayers.tiers.len() > 20 && boosts.tiers.len() > 100);
    assert!(slayers.tiers.iter().all(|x| x.puts_on == 0), "names a kind");
    let mut count = |sort: Sort, level: i32, of: &t::Family| {
        (0..20_000)
            .flat_map(|_| enchantments(&mut rng, sort, level))
            .filter(|(_, tier)| of.tiers.contains(tier))
            .count()
    };
    assert_eq!(count(Sort::Weapon, 16, slayers), 0, "not yet");
    let some = count(Sort::Weapon, 20, slayers);
    assert!((30..400).contains(&some), "{some}");
    assert_eq!(count(Sort::Weapon, 49, wards), 0, "not yet");
    let some = count(Sort::Weapon, 50, wards);
    assert!((100..800).contains(&some), "{some}");
    let some = count(Sort::Armor, 50, boosts);
    assert!((500..1500).contains(&some), "{some}");
}

/// A weapon's effect that puts nothing on what it hits, a slayer's, goes on nothing and asks for
/// nothing the server doesn't know.
/// Behaviour: server.loot.treasure-rarer-enchantments
#[test]
fn a_slayer_on_a_weapon_puts_nothing_on_what_it_hits() {
    use yalain_content::treasure_effects::{When, WEAPON_FAMILIES};
    let slayer = WEAPON_FAMILIES
        .iter()
        .flat_map(|f| f.tiers.iter().map(move |x| (f.when, *x)))
        .find(|(when, x)| *when == When::Hit && x.puts_on == 0)
        .expect("one")
        .1;
    let mut h = Harness::new();
    h.add_creatures(1);
    h.server.levels = levels();
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    let weapon = h.arm(b, me, STARTER_SWORD);
    h.server
        .things
        .get_mut(weapon)
        .expect("held")
        .enchantments
        .push((slayer.state, 9.0));
    let beetle = creature_of(&h, b);
    {
        let t = h.server.things.get_mut(beetle).expect("the beetle");
        t.max_health = 1_000_000;
        t.health = 1_000_000;
        t.creature
            .as_mut()
            .expect("a creature")
            .stats
            .natural_damage = 0.0;
    }
    let before = h.server.unhandled.counts().len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(START_ATTACK, &[Arg::LongInt(beetle)]),
    );
    h.run(2000);
    assert!(h.server.things.get(beetle).expect("alive").health < 1_000_000);
    assert_eq!(h.server.unhandled.counts().len(), before);
}

/// The word before a treasure item's name goes by its sort, its level and its combat delay: a weapon
/// of a short delay takes a word of speed, one of a usual delay never does, and the low levels'
/// plain word isn't drawn at 50.
/// Behaviour: server.loot.treasure-word
#[test]
fn a_treasure_items_word_goes_by_its_sort_level_and_combat_delay() {
    use yalain_content::treasure_words as w;
    use yalain_world::treasure::{word_of, Sort};
    const QUICK: [u32; 4] = [0xDFD0_258D, 0xDFD0_058D, 0xDFD0_E58D, 0xDFD0_458D];
    const BALANCED: u32 = 0xEF7F_89B9;
    let mut rng = yalain_world::rng::Rng::new(5);
    let draws = |rng: &mut yalain_world::rng::Rng, sort, level, delay| -> Vec<u32> {
        (0..500).map(|_| word_of(rng, sort, level, delay)).collect()
    };
    let short = draws(&mut rng, Sort::Weapon, 50, 6);
    assert!(short.iter().any(|x| QUICK.contains(x)));
    let usual = draws(&mut rng, Sort::Weapon, 50, 13);
    assert!(usual.iter().all(|x| !QUICK.contains(x)));
    assert!(usual.iter().all(|x| *x != BALANCED), "a low level's");
    assert!(draws(&mut rng, Sort::Weapon, 5, 13).contains(&BALANCED));
    let armor = draws(&mut rng, Sort::Armor, 30, 14);
    assert!(armor
        .iter()
        .all(|x| w::ARMOR_WORDS.iter().any(|a| a.word == *x)));
    let shield = draws(&mut rng, Sort::Shield, 30, 8);
    assert!(shield
        .iter()
        .all(|x| w::SHIELD_WORDS.iter().any(|a| a.word == *x)));
    assert_eq!(
        word_of(&mut rng, Sort::Jewelry, 30, 8),
        0,
        "jewelry has none"
    );
    // A delay none showed still takes a word of its level.
    let odd = draws(&mut rng, Sort::Weapon, 50, 999);
    assert!(odd
        .iter()
        .all(|x| w::WEAPON_WORDS.iter().any(|a| a.word == *x)));
}

/// An enchantment 2017 showed in a pair comes with its other half as one draw; neither half comes
/// alone, and what no item was named for names none.
/// Behaviour: server.loot.treasure-paired-enchantments
#[test]
fn a_paired_enchantment_comes_with_its_other_half_and_names_nothing() {
    use yalain_content::treasure_effects::{self as t, When};
    use yalain_world::treasure::{enchantments, Sort};
    const WARD: u32 = 0x6F00_266E;
    const FIRE: u32 = 0x6F00_2500;
    const EDGE: u32 = 0x6F00_24F4;
    let mut rng = yalain_world::rng::Rng::new(3);
    let (mut warded, mut burning) = (0, 0);
    for _ in 0..40_000 {
        let drawn = enchantments(&mut rng, Sort::Weapon, 50);
        let has = |s: u32| drawn.iter().any(|(_, x)| x.state == s);
        for (a, b) in t::PAIRS {
            assert!(!has(*a) || has(*b), "{a:x} without {b:x}");
        }
        assert_eq!(has(FIRE), has(EDGE), "the fire and the edge");
        if has(WARD) {
            let counters = t::PAIRS.iter().filter(|(_, b)| *b == WARD);
            assert_eq!(counters.filter(|(a, _)| has(*a)).count(), 1, "one counter");
            warded += 1;
        }
        burning += usize::from(has(FIRE));
        for (when, x) in &drawn {
            if x.state == EDGE {
                assert_eq!(*when, When::Permanent);
            }
            if x.state == WARD {
                assert_eq!(*when, When::Equipped);
            }
            if x.state == FIRE {
                assert_eq!(*when, When::Hit);
            }
        }
    }
    assert!(warded > 0 && burning > 0, "{warded} {burning}");
    assert!(enchantments(&mut rng, Sort::Weapon, 30)
        .iter()
        .all(|(_, x)| x.state != WARD));
    // What no item was named for names none.
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    use yalain_content::{Prop, Props};
    use yalain_world::effect_defs::{EffectDef, Kind, Scale};
    const TEMPLATE: u32 = 0x4700_7A01;
    let mut props = Props::default();
    props
        .set("Weapon_Damage", Prop::Int(10))
        .set("Weapon_Variance", Prop::Float(0.25));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(TEMPLATE, 0x200, props));
    h.server.treasure.equipment.insert(0x200);
    for state in t::states() {
        h.server.effect_table.effects.insert(
            state,
            EffectDef {
                state,
                kind: Kind::Other,
                duration: Scale::default(),
                priority: Scale::default(),
                fx: 0,
                external_flags: 0,
                internal_flags: 0,
                enum_value: 0,
                eq_class: 0,
                name: Some((1, state)),
            },
        );
    }
    let mut seen = 0;
    for _ in 0..30_000 {
        let item = yalain_world::inventory::make_item(&mut h.server, TEMPLATE, 1, 9).expect("made");
        yalain_world::treasure::endow(&mut h.server, item, 50, None);
        let o = h.server.things.get(item).expect("made");
        let carried: Vec<u32> = o
            .enchantments
            .iter()
            .chain(&o.bonuses)
            .map(|e| e.0)
            .collect();
        if carried.contains(&WARD) {
            seen += 1;
            let named = o.own.suffix;
            assert!(
                named.is_none_or(|n| !t::UNNAMING.contains(&n)),
                "{named:x?}"
            );
            if carried.iter().all(|s| t::UNNAMING.contains(s)) {
                assert_eq!(named, None, "with nothing that names it");
            }
        }
    }
    assert!(seen > 0);
}

/// Water's and Wounding's tiers that 2017 showed as the effect itself carry it, not the apply state
/// that holds the next tier's name; each still puts the same effect on what it hits.
/// Behaviour: server.loot.treasure-named-for-an-enchantment
#[test]
fn water_and_wounding_tiers_carry_the_effect_2017_showed() {
    use yalain_content::treasure_effects::{tier_of, When};
    for (itself, apply) in [(0x6F00_03DB, 0x6F00_2614), (0x6F00_03E0, 0x6F00_261B)] {
        let (when, tier) = tier_of(itself).expect("a tier");
        assert_eq!(when, When::Hit);
        assert_eq!(tier.puts_on, itself);
        assert!(tier.chance > 0.0 && tier.chance < 1.0, "{}", tier.chance);
        assert!(tier_of(apply).is_none(), "{apply:x}");
    }
}

/// A slayer harms what it names by its spellcraft as each hit lands, told as a story of its own, and
/// does nothing to any other creature.
/// Behaviour: server.loot.treasure-slayer
#[test]
fn a_slayer_harms_the_creatures_it_names_by_its_spellcraft_and_no_others() {
    use yalain_world::effect_defs::{EffectDef, Kind, Scale};
    const SLAYER: u32 = 0x6F00_0003;
    const NAMED: u32 = 25;
    let told = |superclass: u32| -> (usize, usize) {
        let mut h = Harness::new();
        h.add_creatures(1);
        h.server.levels = levels();
        let (b, me) = h.player_in_world("one", "Elara");
        scene_drawn(&mut h, b);
        let weapon = h.arm(b, me, STARTER_SWORD);
        h.server.effect_table.effects.insert(
            SLAYER,
            EffectDef {
                state: SLAYER,
                kind: Kind::Slayer(vec![(NAMED, 1.0)]),
                duration: Scale::default(),
                priority: Scale::default(),
                fx: 0,
                external_flags: 0,
                internal_flags: 0,
                enum_value: 0,
                eq_class: 0,
                name: None,
            },
        );
        h.server
            .things
            .get_mut(weapon)
            .expect("held")
            .enchantments
            .push((SLAYER, 30.0));
        let beetle = creature_of(&h, b);
        let template = h.server.things.get(beetle).expect("the beetle").template;
        let mut t = h
            .server
            .catalog
            .get(template)
            .expect("its template")
            .clone();
        t.props.set(
            yalain_content::props::creature_property::SUPER_CLASS,
            yalain_content::Prop::Word(superclass),
        );
        h.server.catalog.insert(t);
        {
            let t = h.server.things.get_mut(beetle).expect("the beetle");
            t.max_health = 1_000_000;
            t.health = 1_000_000;
            let c = t.creature.as_mut().expect("a creature");
            c.stats.natural_damage = 0.0;
        }
        let from = h.bots[b].inbox.len();
        h.bots[b].send(
            true,
            queue::WEENIE,
            &server_call(START_ATTACK, &[Arg::LongInt(beetle)]),
        );
        let mut told = 0;
        h.run_until(60_000, |h| {
            told = h.world_messages(b, from).len();
            h.server.things.get(beetle).expect("alive").health < 999_000
        });
        assert!(told > 0);
        // The player's hits are told as its own stories; the slayer's harm as the creature's, at
        // the creature. The creature's own blows at the player aren't counted.
        let (mut hits, mut harms) = (0, 0);
        for m in h.world_messages(b, from) {
            let ServerMessage::DoStory(e) = m else {
                continue;
            };
            for k in &e.action.hooks {
                if k.target == beetle && k.numbers[3] < 0 {
                    if e.sender.id == beetle {
                        harms += 1;
                    } else {
                        hits += 1;
                    }
                }
            }
        }
        (hits, harms)
    };
    let (hits, harms) = told(NAMED);
    assert!(hits >= 5, "{hits} hits");
    assert!(hits.abs_diff(harms) <= 1, "{harms} harms, {hits} hits");
    let (hits, harms) = told(NAMED + 1);
    assert!(hits >= 5, "{hits} hits");
    assert_eq!(harms, 0);
}

/// What a weapon puts on what it hits comes on by its own chance as its hits land, and is named on
/// its profile as an effect on a hit.
/// Behaviour: server.loot.treasure-effect-on-hit
#[test]
fn a_weapons_effect_on_a_hit_goes_on_what_it_hits() {
    use yalain_content::treasure_effects::WEAPON_FAMILIES;
    use yalain_world::effect_defs::{EffectDef, Kind, Scale};
    let tier = WEAPON_FAMILIES
        .iter()
        .find(|f| f.when == yalain_content::treasure_effects::When::Hit)
        .expect("a family on a hit")
        .tiers[0];
    let mut h = Harness::new();
    h.add_creatures(1);
    h.server.levels = levels();
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    let weapon = h.arm(b, me, STARTER_SWORD);
    h.server.effect_table.effects.insert(
        tier.puts_on,
        EffectDef {
            state: tier.puts_on,
            kind: Kind::Other,
            duration: Scale(vec![(0.0, 600.0)]),
            priority: Scale::default(),
            fx: 0,
            external_flags: 0,
            internal_flags: 0,
            enum_value: 1,
            eq_class: 1,
            name: None,
        },
    );
    h.server
        .things
        .get_mut(weapon)
        .expect("held")
        .enchantments
        .push((tier.state, 9.0));
    let beetle = creature_of(&h, b);
    {
        let t = h.server.things.get_mut(beetle).expect("the beetle");
        t.max_health = 1_000_000;
        t.health = 1_000_000;
        t.creature
            .as_mut()
            .expect("a creature")
            .stats
            .natural_damage = 0.0;
    }
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(START_ATTACK, &[Arg::LongInt(beetle)]),
    );
    let on = |h: &Harness| {
        h.server
            .creature_effects
            .on
            .get(&beetle)
            .is_some_and(|v| v.iter().any(|e| e.state == tier.puts_on))
    };
    assert!(h.run_until(60_000, on), "it comes on at last");
    // Named on its profile among what it does on a hit.
    h.server.effect_table.effects.insert(
        tier.state,
        EffectDef {
            state: tier.state,
            kind: Kind::Other,
            duration: Scale::default(),
            priority: Scale::default(),
            fx: 0,
            external_flags: 0,
            internal_flags: 1,
            enum_value: 2,
            eq_class: 2,
            name: Some((1, 2)),
        },
    );
    let t = h.server.things.get_mut(weapon).expect("held");
    t.own.made = Some(yalain_world::made::Made::default());
    let t = h.server.things.get(weapon).expect("held");
    let mut view = yalain_world::profile::ItemView::of(&yalain_content::Props::default(), 1, None);
    yalain_world::made::show(&h.server, t, &mut view);
    assert_eq!(view.effects.len(), 1);
    assert_eq!(view.effects[0].when, yalain_world::profile::When::Hit);
}

/// Treasure jewelry is plain with no least level, or carries one effect on its wearer and a least
/// level, as often as its dropper's level says; it shows what it carries as worn.
/// Behaviour: server.loot.treasure-jewelry-enchantments
#[test]
fn treasure_jewelry_is_plain_or_has_one_effect_and_a_least_level() {
    use yalain_content::treasure_effects::{tier_of, When};
    use yalain_world::effect_defs::{EffectDef, Kind, Scale};
    const RING: u32 = 0x4700_7011;
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    h.server.catalog.insert(yalain_testkit::test_template(
        RING,
        0x777,
        yalain_content::Props::default(),
    ));
    h.server.treasure.jewelry.insert(0x777);
    let mut share = |level: i32| {
        let mut enchanted = 0;
        for _ in 0..2000 {
            let item = yalain_world::inventory::make_item(&mut h.server, RING, 1, 9).expect("made");
            yalain_world::treasure::endow(&mut h.server, item, level, None);
            let t = h.server.things.get(item).expect("made");
            assert!(t.enchantments.is_empty(), "on the wearer");
            assert_eq!(t.own.min_level.is_some(), !t.bonuses.is_empty());
            if let Some((state, spellcraft)) = t.bonuses.first().copied() {
                enchanted += 1;
                assert_eq!(t.bonuses.len(), 1);
                let (when, tier) = tier_of(state).expect("a known effect");
                assert_eq!(when, When::Equipped);
                let least = t.own.min_level.expect("a least level");
                assert!(tier.least <= least.max(level), "{tier:?} at {least}");
                #[allow(clippy::cast_precision_loss)]
                let cast = (least.max(level) - 1) as f32;
                assert_eq!(spellcraft, cast);
            }
        }
        enchanted
    };
    assert_eq!(share(1), 0, "nothing reaches so low");
    let low = share(14);
    let high = share(50);
    assert!((100..400).contains(&low), "{low}");
    assert!((500..800).contains(&high), "{high}");
    // What it carries it shows, with no numbers of its own.
    let item = yalain_world::inventory::make_item(&mut h.server, RING, 1, 9).expect("made");
    let state = yalain_content::treasure_effects::JEWELRY_FAMILIES[0].tiers[0].state;
    h.server.effect_table.effects.insert(
        state,
        EffectDef {
            state,
            kind: Kind::Other,
            duration: Scale::default(),
            priority: Scale::default(),
            fx: 0,
            external_flags: 0,
            internal_flags: 1,
            enum_value: 2,
            eq_class: 2,
            name: Some((1, 2)),
        },
    );
    let t = h.server.things.get_mut(item).expect("made");
    t.bonuses.push((state, 20.0));
    let t = h.server.things.get(item).expect("made");
    let mut view = yalain_world::profile::ItemView::of(&yalain_content::Props::default(), 1, None);
    yalain_world::made::show(&h.server, t, &mut view);
    assert_eq!(view.effects.len(), 1);
    assert_eq!(view.effects[0].when, yalain_world::profile::When::Equipped);
    view.jewelry = true;
    let bytes = yalain_world::profile::item(0x3FF, &view).packed(&[]);
    let worn = yalain_world::profile::item_line::EFFECTS_WORN.to_le_bytes();
    let worn = bytes.windows(4).any(|w| w == worn);
    assert!(worn, "its heading says worn");
}

/// A treasure item that draws enchantments is named for one of them, any of them in time; one that
/// draws none has no name of the kind.
/// Behaviour: server.loot.treasure-named-for-an-enchantment
#[test]
fn a_treasure_item_with_enchantments_is_named_for_one_of_them() {
    use yalain_content::treasure_effects::{states, WEAPON_FAMILIES};
    use yalain_world::effect_defs::{EffectDef, Kind, Scale};
    const SWORD: u32 = 0x4700_7003;
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    for state in states() {
        h.server.effect_table.effects.insert(
            state,
            EffectDef {
                state,
                kind: Kind::Other,
                duration: Scale::default(),
                priority: Scale::default(),
                fx: 0,
                external_flags: 0,
                internal_flags: 1,
                enum_value: 2,
                eq_class: 2,
                name: Some((1, state ^ 0x4B00_0000)),
            },
        );
    }
    let mut props = yalain_content::Props::default();
    props
        .set("Weapon_Damage", yalain_content::Prop::Int(10))
        .set("Weapon_Variance", yalain_content::Prop::Float(0.25));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(SWORD, 0x200, props));
    h.server.treasure.equipment.insert(0x200);
    assert!(WEAPON_FAMILIES.len() > 2);
    let (mut plain, mut one, mut many) = (0, 0, std::collections::HashSet::new());
    for _ in 0..3000 {
        let item = yalain_world::inventory::make_item(&mut h.server, SWORD, 1, 9).expect("made");
        yalain_world::treasure::endow(&mut h.server, item, 50, None);
        let t = h.server.things.get(item).expect("made");
        let carried: Vec<u32> = t
            .enchantments
            .iter()
            .chain(&t.bonuses)
            .map(|(state, _)| state ^ 0x4B00_0000)
            .collect();
        match t.own.suffix {
            None => {
                plain += 1;
                assert!(carried.iter().all(
                    |c| yalain_content::treasure_effects::UNNAMING.contains(&(c ^ 0x4B00_0000))
                ));
            }
            Some(table) => {
                let at = carried.iter().position(|c| *c == table);
                assert!(at.is_some(), "one of what it carries");
                if carried.len() == 1 {
                    one += 1;
                } else {
                    many.insert(at);
                }
            }
        }
    }
    assert!(plain > 1000 && one > 100, "{plain} {one}");
    assert!(many.len() >= 2, "any of them: {many:?}");
}

/// A weapon's effect that adds to the chance of a critical does: at 1, every hit it lands is
/// critical; a weapon without it lands plain hits.
/// Behaviour: server.combat.critical-by-weapon
#[test]
fn a_weapons_critical_bonus_makes_its_hits_critical() {
    use yalain_world::combat::{critical_bonus, CRITICAL_BONUS};
    use yalain_world::effect_defs::{numeric, EffectDef, Kind, Scale};
    const BONUS: u32 = 0x6F00_0001;
    let outcomes = |bonus: bool| -> Vec<i32> {
        let mut h = Harness::new();
        h.add_creatures(1);
        h.server.levels = levels();
        let (b, me) = h.player_in_world("one", "Elara");
        scene_drawn(&mut h, b);
        let weapon = h.arm(b, me, STARTER_SWORD);
        h.server.effect_table.effects.insert(
            BONUS,
            EffectDef {
                state: BONUS,
                kind: Kind::Numeric {
                    flags: numeric::ADD,
                    stats: vec![CRITICAL_BONUS],
                    magnitude: Scale(vec![(0.0, 1.0)]),
                },
                duration: Scale::default(),
                priority: Scale::default(),
                fx: 0,
                external_flags: 0,
                internal_flags: 0,
                enum_value: 0,
                eq_class: 0,
                name: None,
            },
        );
        if bonus {
            h.server
                .things
                .get_mut(weapon)
                .expect("held")
                .enchantments
                .push((BONUS, 9.0));
        }
        assert_eq!(
            critical_bonus(&h.server, Some(weapon)),
            if bonus { 1.0 } else { 0.0 }
        );
        let beetle = creature_of(&h, b);
        {
            let t = h.server.things.get_mut(beetle).expect("the beetle");
            t.max_health = 1_000_000;
            t.health = 1_000_000;
            t.creature
                .as_mut()
                .expect("a creature")
                .stats
                .natural_damage = 0.0;
        }
        let from = h.bots[b].inbox.len();
        h.bots[b].send(
            true,
            queue::WEENIE,
            &server_call(START_ATTACK, &[Arg::LongInt(beetle)]),
        );
        h.run(4000);
        h.world_messages(b, from)
            .iter()
            .filter_map(|m| match m {
                ServerMessage::DoStory(e) if e.sender.id == me => Some(e.action.hooks.clone()),
                _ => None,
            })
            .flatten()
            .filter(|k| k.target == beetle && k.numbers[3] < 0)
            .map(|k| k.numbers[0])
            .collect()
    };
    let critical = yalain_world::rules::Outcome::Critical.code().cast_signed();
    let with = outcomes(true);
    assert!(with.len() > 20, "{}", with.len());
    assert!(with.iter().all(|o| *o == critical), "{with:x?}");
    let without = outcomes(false);
    assert!(without.len() > 20, "{}", without.len());
    assert!(
        without.iter().filter(|o| **o == critical).count() < without.len() / 2,
        "{without:x?}"
    );
}

/// Using the corpse opens it: the opener is told what it holds (the beetle's profile always
/// leaves one item) and of the item; using the item takes it, and once the taking is through the
/// move is answered and the item is the player's.
/// Behaviour: server.loot.open-and-take
#[test]
fn a_corpse_opens_to_its_killer_and_an_item_taken_from_it_is_the_players() {
    let (mut h, b, me, _) = fight();
    assert!(h.run_until(200, |h| h.world_messages(b, 0).iter().any(
        |m| matches!(m, ServerMessage::CreateObject(o) if o.weenie.entity_did == CORPSE)
    )));
    let corpse = h
        .world_messages(b, 0)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == CORPSE => Some(o.id),
            _ => None,
        })
        .expect("corpse");
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_of(corpse, me)]),
    );
    h.run(20);
    let opened = calls_of(&h, b, from)
        .into_iter()
        .find(|(f, _)| *f == OPEN_DONE)
        .expect("the corpse opens");
    assert_eq!(opened.1.last(), Some(&Value::Long(corpse)));
    let item = h
        .world_messages(b, from)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == TEST_ITEM => Some(o.id),
            _ => None,
        })
        .expect("the item it holds is made");
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &server_call(USE, &[use_of(item, me)]));
    assert!(
        h.run_until(100, |h| calls_of(h, b, from)
            .iter()
            .any(|(f, _)| *f == MOVE_DONE)),
        "the take is answered"
    );
    let held = h
        .server
        .players
        .values()
        .any(|p| p.play.items.iter().any(|x| x.id == item));
    assert!(held, "the item is in the player's pack");
}

/// A corpse is open to one player at a time: another player's use of it while it is open is
/// refused (status 10001, flags 956) and opens nothing; once the first closes it, the other's use
/// opens it.
/// Behaviour: server.loot.one-viewer-at-a-time
#[test]
fn a_corpse_another_player_has_open_refuses_the_use_until_it_is_closed() {
    let (mut h, b, me, _) = fight();
    assert!(h.run_until(200, |h| h.world_messages(b, 0).iter().any(
        |m| matches!(m, ServerMessage::CreateObject(o) if o.weenie.entity_did == CORPSE)
    )));
    let corpse = h
        .world_messages(b, 0)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == CORPSE => Some(o.id),
            _ => None,
        })
        .expect("corpse");
    let (other, oid) = h.player_in_world("two", "Fellowa");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_of(corpse, me)]),
    );
    h.run(20);
    let from = h.bots[other].inbox.len();
    h.bots[other].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_of(corpse, oid)]),
    );
    h.run(20);
    let got = calls_of(&h, other, from);
    assert!(
        !got.iter().any(|(f, _)| *f == OPEN_DONE),
        "nothing opens for the second"
    );
    let refused = got.iter().any(|(f, v)| {
        *f == USE_DONE
            && v.iter().any(|x| match x {
                Value::Graph(g) => g.iter().any(|(_, n)| match n {
                    calls::Node::Script { slots, .. } => {
                        slots.iter().any(|s| s.1 == 10001) && slots.iter().any(|s| s.1 == 956)
                    }
                    _ => false,
                }),
                _ => false,
            })
    });
    assert!(refused, "the use is refused: {got:?}");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(CLOSE, &[Arg::LongInt(corpse)]),
    );
    h.run(20);
    let from = h.bots[other].inbox.len();
    h.bots[other].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_of(corpse, oid)]),
    );
    h.run(20);
    assert!(
        calls_of(&h, other, from)
            .iter()
            .any(|(f, _)| *f == OPEN_DONE),
        "closed, it opens for the other"
    );
}

/// Turning a corpse's items into money moves each into the pack and destroys it there, the money
/// going up by its value; the answer says what was earned; each item is let go of (its holder
/// none) before it is gone.
/// Behaviour: server.loot.turned-into-money
#[test]
fn turning_a_corpses_items_into_money_pays_their_value() {
    let (mut h, b, me, _) = fight();
    assert!(h.run_until(200, |h| h.world_messages(b, 0).iter().any(
        |m| matches!(m, ServerMessage::CreateObject(o) if o.weenie.entity_did == CORPSE)
    )));
    let corpse = h
        .world_messages(b, 0)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == CORPSE => Some(o.id),
            _ => None,
        })
        .expect("corpse");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_of(corpse, me)]),
    );
    h.run(20);
    let from = h.bots[b].inbox.len();
    let mut g = Builder::new();
    g.object(
        0x1B59,
        &[
            Slot::Int(0),
            Slot::Int(0),
            Slot::Ref(None),
            Slot::Int(1),
            Slot::Int(0),
            Slot::Int(0),
            Slot::Int(0),
            Slot::Int(0),
            Slot::Long(corpse),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Int(0),
            Slot::Long(me),
        ],
    );
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(TRANSMUTE_ALL, &[Arg::Object(g.graph())]),
    );
    h.run(20);
    assert!(
        calls_of(&h, b, from)
            .iter()
            .any(|(f, _)| *f == TRANSMUTE_ALL_DONE),
        "answered"
    );
    let money: Vec<i32> = h
        .world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateInt(q) if q.quality == 3100 => Some(q.value),
            _ => None,
        })
        .collect();
    assert_eq!(money.len(), 1, "one item, one payment");
    assert!(money[0] >= 1, "worth something: {money:?}");
    let msgs = h.world_messages(b, from);
    let gone: Vec<u64> = msgs
        .iter()
        .filter_map(|m| match m {
            ServerMessage::ForgetObject(o) => Some(o.0.id),
            _ => None,
        })
        .collect();
    assert_eq!(gone.len(), 1, "the item gone");
    assert!(
        msgs.iter()
            .any(|m| matches!(m, ServerMessage::UpdateInstanceIdVisual(q)
            if q.object.id == gone[0] && q.quality == 258 && q.value == 0)),
        "let go of first"
    );
}

/// The corpse a client was told of, from its messages since `from`.
fn corpse_made(
    h: &Harness,
    b: usize,
    from: usize,
) -> Option<auberean_protocol::physics::CreateObject> {
    h.world_messages(b, from).into_iter().find_map(|m| match m {
        ServerMessage::CreateObject(o) if o.weenie.entity_did == CORPSE => Some(o),
        _ => None,
    })
}

/// The death pose a client saw a creature fall in.
fn pose_of(h: &Harness, b: usize, creature: u64) -> f32 {
    h.world_messages(b, 0)
        .iter()
        .find_map(|m| match m {
            ServerMessage::MoveSlider(e) if e.sender.id == creature && e.action.slider == POSE => {
                Some(e.action.value)
            }
            _ => None,
        })
        .expect("the death pose")
}

const POSE: u32 = 0x4000_001D;
const DEATH: u32 = 0x4100_001F;
const DEAD_MODE: u32 = 0x4100_0005;
const CORPSE_MODE: u32 = 0x4100_0003;

/// A creature that dies is told once more where it fell, standing still, after its death.
/// Behaviour: server.death.stops-where-it-falls
#[test]
fn a_dying_creature_is_told_standing_where_it_fell_after_its_death() {
    let (mut h, b, _, beetle) = fight();
    h.run(2);
    let msgs = h.world_messages(b, 0);
    let death = msgs
        .iter()
        .position(|m| matches!(m, ServerMessage::PlayBehavior(e) if e.sender.id == beetle && e.action.behavior == DEATH))
        .expect("its death");
    let fell = h
        .server
        .things
        .get(beetle)
        .and_then(|t| t.at)
        .expect("still there");
    let stop = msgs[death..]
        .iter()
        .find_map(|m| match m {
            ServerMessage::Position(p) | ServerMessage::PositionCell(p)
                if p.sender.id == beetle =>
            {
                Some(*p)
            }
            _ => None,
        })
        .expect("its place after its death");
    assert_eq!(
        stop.heading_motion & 0x00FF_FFFF,
        yalain_world::world::IDLE_MOTION,
        "standing still"
    );
    assert_eq!(stop.origin.x, fell.origin[0]);
    assert_eq!(stop.origin.y, fell.origin[1]);
}

/// The death pose is one of six whole poses, and the corpse lies in it: its object carries the
/// pose's slider unless it is the first pose. In the same tick come the corpse's scale, its two
/// effects, its pose, its mode, then nobody's claim, its killer, the creature it was and its name,
/// their visual stamps 2 to 10, the object's own.
/// Behaviour: server.death.corpse-settles
#[test]
fn a_corpse_lies_in_the_death_pose_and_is_followed_by_its_settling() {
    let (mut h, b, me, beetle) = fight();
    let pose = pose_of(&h, b, beetle);
    assert!((0..6).any(|p| pose == p as f32), "a whole pose: {pose}");
    let from = h.bots[b].inbox.len();
    assert!(h.run_until(200, |h| corpse_made(h, b, from).is_some()));
    let corpse = corpse_made(&h, b, from).expect("corpse");
    assert_eq!(corpse.physics.mode, CORPSE_MODE);
    assert_eq!(corpse.physics.visual_ordering_stamp, 10);
    let lies = corpse
        .physics
        .sliders
        .iter()
        .find(|s| s.slider == POSE)
        .map(|s| s.value);
    if pose == 0.0 {
        assert_eq!(lies, None, "the first pose is left out");
    } else {
        assert_eq!(lies, Some(pose));
    }
    // Whichever pose this fight drew, each is made so.
    let mut t = h.server.things.get(corpse.id).cloned().expect("the corpse");
    for p in 0..6u8 {
        t.pose = Some(f32::from(p));
        let made = yalain_world::death::corpse_create(&t);
        let slider = made
            .physics
            .sliders
            .iter()
            .find(|s| s.slider == POSE)
            .map(|s| s.value);
        assert_eq!(slider, (p > 0).then_some(f32::from(p)), "pose {p}");
    }
    let msgs = h.world_messages(b, from);
    let at = msgs
        .iter()
        .position(|m| matches!(m, ServerMessage::CreateObject(o) if o.id == corpse.id))
        .expect("made");
    let after: Vec<(String, u16)> = msgs[at + 1..]
        .iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateFloatVisual(q) if q.object.id == corpse.id => {
                Some((format!("scale {}", q.value), q.object.other_stamp))
            }
            ServerMessage::DoFx(e) if e.sender.id == corpse.id => {
                Some((format!("fx {:x}", e.action.fx), e.sender.other_stamp))
            }
            ServerMessage::MoveSlider(e) if e.sender.id == corpse.id => {
                Some((format!("pose {}", e.action.value), e.sender.other_stamp))
            }
            ServerMessage::SetMode(e) if e.sender.id == corpse.id => {
                Some((format!("mode {:x}", e.action.0), e.sender.other_stamp))
            }
            ServerMessage::UpdateInstanceIdVisual(q) if q.object.id == corpse.id => {
                Some((format!("{} {:x}", q.quality, q.value), q.object.other_stamp))
            }
            ServerMessage::UpdateStringInfoVisual(q) if q.object.id == corpse.id => Some((
                if q.value == corpse.weenie.name {
                    format!("{} its name", q.quality)
                } else {
                    format!("{} another name", q.quality)
                },
                q.object.other_stamp,
            )),
            _ => None,
        })
        .collect();
    let expected: Vec<(String, u16)> = vec![
        ("scale 1".into(), 2),
        ("fx 41000002".into(), 3),
        ("fx 41000004".into(), 4),
        (format!("pose {pose}"), 5),
        ("mode 41000003".into(), 6),
        ("322 0".into(), 7),
        (format!("320 {me:x}"), 8),
        (format!("321 {beetle:x}"), 9),
        ("256 its name".into(), 10),
    ];
    assert_eq!(after, expected);
}

/// A corpse made for a client whose view it comes into after it appeared is followed by the same
/// changes it was made up of, its name last at stamp 10 (2017: 369 of 442 corpses first seen
/// after their creature's death).
/// Behaviour: server.death.corpse-settles-for-newcomers
#[test]
fn a_corpse_seen_later_is_followed_by_its_settling_too() {
    let (mut h, b, _me, _beetle) = fight();
    let from = h.bots[b].inbox.len();
    assert!(h.run_until(200, |h| corpse_made(h, b, from).is_some()));
    let corpse = corpse_made(&h, b, from).expect("corpse");
    let (c, _) = h.player_in_world("two", "Babyfare");
    h.run(10);
    let msgs = h.world_messages(c, 0);
    assert!(
        msgs.iter()
            .any(|m| matches!(m, ServerMessage::CreateObject(o) if o.id == corpse.id)),
        "made for the newcomer"
    );
    // Sent after the object, on the visual channel: they may arrive before it.
    let mut stamps: Vec<u16> = msgs
        .iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateFloatVisual(q) if q.object.id == corpse.id => {
                Some(q.object.other_stamp)
            }
            ServerMessage::DoFx(e) if e.sender.id == corpse.id => Some(e.sender.other_stamp),
            ServerMessage::MoveSlider(e) if e.sender.id == corpse.id => Some(e.sender.other_stamp),
            ServerMessage::SetMode(e) if e.sender.id == corpse.id => Some(e.sender.other_stamp),
            ServerMessage::UpdateInstanceIdVisual(q) if q.object.id == corpse.id => {
                Some(q.object.other_stamp)
            }
            _ => None,
        })
        .collect();
    stamps.sort_unstable();
    let named = msgs.iter().find_map(|m| match m {
        ServerMessage::UpdateStringInfoVisual(q) if q.object.id == corpse.id => {
            Some((q.value == corpse.weenie.name, q.object.other_stamp))
        }
        _ => None,
    });
    assert_eq!(
        stamps,
        [2, 3, 4, 5, 6, 7, 8, 9],
        "the changes it was made up of"
    );
    assert_eq!(named, Some((true, 10)), "its name last");
}

/// A corpse made again for a client (one asking for it again, one whose view it comes into) lies
/// as it was first made, and a dead creature made again before it leaves is in its dead mode and
/// pose.
/// Behaviour: server.death.made-again-lying
#[test]
fn a_corpse_or_dead_creature_made_again_lies_in_its_pose() {
    let (mut h, b, _, beetle) = fight();
    let pose = pose_of(&h, b, beetle);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::ForceCreate(auberean_protocol::physics::Target(beetle)),
    );
    h.run(4);
    let again = h
        .world_messages(b, from)
        .into_iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.id == beetle => Some(o),
            _ => None,
        })
        .expect("the dead creature made again");
    assert_eq!(again.physics.mode, DEAD_MODE);
    let lies = |o: &auberean_protocol::physics::CreateObject| {
        o.physics
            .sliders
            .iter()
            .find(|s| s.slider == POSE)
            .map_or(0.0, |s| s.value)
    };
    assert_eq!(lies(&again), pose);
    assert!(h.run_until(200, |h| corpse_made(h, b, from).is_some()));
    let first = corpse_made(&h, b, from).expect("corpse");
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::ForceCreate(auberean_protocol::physics::Target(
            first.id,
        )),
    );
    h.run(4);
    let asked = corpse_made(&h, b, from).expect("the corpse made again");
    let (c, _) = h.player_in_world("two", "Babyfare");
    scene_drawn(&mut h, c);
    let seen = corpse_made(&h, c, 0).expect("the corpse made for one coming in");
    for o in [&asked, &seen] {
        assert_eq!(o.physics.mode, CORPSE_MODE);
        assert_eq!(lies(o), pose);
        assert_eq!(o.weenie.name, first.weenie.name, "Corpse of the creature");
        assert_eq!(
            o.weenie.objects, first.weenie.objects,
            "its killer and what it was"
        );
    }
}

/// Open the first corpse a fight leaves; returns it.
fn open_corpse(h: &mut Harness, b: usize, me: u64) -> u64 {
    assert!(h.run_until(200, |h| corpse_made(h, b, 0).is_some()));
    let corpse = corpse_made(h, b, 0).expect("corpse").id;
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_of(corpse, me)]),
    );
    h.run(20);
    corpse
}

/// An open corpse whose opener goes more than six meters from it closes: the opener is told, as
/// when it asks to close it; nearer, it stays open.
/// Behaviour: server.loot.corpse-closes-when-its-opener-walks-away
#[test]
fn an_open_corpse_closes_when_its_opener_walks_away() {
    let (mut h, b, me, _) = fight();
    let corpse = open_corpse(&mut h, b, me);
    let world = *h.server.players.keys().next().expect("in");
    let at = h
        .server
        .things
        .get(corpse)
        .and_then(|t| t.at)
        .expect("the corpse stands in the world");
    let stamp = h.server.players[&world].teleport_stamp;
    let step = |h: &mut Harness, dx: f32| {
        let to = auberean_primitives::Vec3::new(at.origin[0] + dx, at.origin[1], at.origin[2]);
        let report =
            yalain_testkit::play::report(at.cell, to, 0, yalain_world::world::IDLE_MOTION, stamp);
        h.bots[b].send(true, queue::WEENIE, &report);
        h.run(20);
    };
    let from = h.bots[b].inbox.len();
    step(&mut h, 4.0);
    assert!(
        !calls_of(&h, b, from).iter().any(|(f, _)| *f == CLOSE_DONE),
        "still near"
    );
    assert_eq!(
        yalain_world::inventory::open_corpse_of(&h.server, world),
        Some(corpse)
    );
    step(&mut h, 8.0);
    let closed: Vec<_> = calls_of(&h, b, from)
        .into_iter()
        .filter(|(f, _)| *f == CLOSE_DONE)
        .collect();
    assert_eq!(closed.len(), 1, "told once");
    assert_eq!(closed[0].1.last(), Some(&Value::Long(corpse)));
    assert_eq!(
        yalain_world::inventory::open_corpse_of(&h.server, world),
        None
    );
}

/// Ask to turn a corpse's items into money; returns the answer's values.
fn transmute(h: &mut Harness, b: usize, me: u64, corpse: u64) -> Vec<Value> {
    let from = h.bots[b].inbox.len();
    let mut g = Builder::new();
    g.object(
        0x1B59,
        &[
            Slot::Int(0),
            Slot::Int(0),
            Slot::Ref(None),
            Slot::Int(1),
            Slot::Int(0),
            Slot::Int(0),
            Slot::Int(0),
            Slot::Int(0),
            Slot::Long(corpse),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Int(0),
            Slot::Long(me),
        ],
    );
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(TRANSMUTE_ALL, &[Arg::Object(g.graph())]),
    );
    h.run(20);
    calls_of(h, b, from)
        .into_iter()
        .find(|(f, _)| *f == TRANSMUTE_ALL_DONE)
        .expect("answered")
        .1
}

/// An answer's slot, and whether the list a slot of it refers to holds an id.
fn slot(v: &[Value], i: usize) -> u32 {
    match v.first().and_then(Value::root) {
        Some(calls::Node::Script { slots, .. }) => slots[i].1,
        _ => panic!("an answer object"),
    }
}
fn lists(v: &[Value], i: usize, id: u64) -> bool {
    let entry = slot(v, i);
    match v.first().and_then(|g| g.entry(entry)) {
        Some(calls::Node::Native { bytes, .. }) => bytes.windows(8).any(|w| w == id.to_le_bytes()),
        _ => false,
    }
}

/// The items a corpse holds now.
fn held_by(h: &Harness, corpse: u64) -> Vec<u64> {
    h.server
        .things
        .get(corpse)
        .and_then(|t| t.corpse.as_ref())
        .map(|c| c.contents.clone())
        .unwrap_or_default()
}

/// Turning a corpse's items into money keeps back an item worth nothing: it stays in the corpse,
/// listed as not turned into money with error 1105, the answer's status 1105 too when nothing was
/// turned into money, and nothing is paid.
/// Behaviour: server.loot.quest-items-kept-from-money
#[test]
fn turning_a_corpses_items_into_money_keeps_an_item_worth_nothing() {
    let (mut h, b, me, _) = fight();
    let corpse = open_corpse(&mut h, b, me);
    let item = held_by(&h, corpse)[0];
    if let Some(t) = h.server.things.get_mut(item) {
        t.value = 0;
    }
    let from = h.bots[b].inbox.len();
    let v = transmute(&mut h, b, me, corpse);
    assert_eq!(slot(&v, 0), 1105, "the error");
    assert_eq!(slot(&v, 7), 1105, "the status: nothing turned into money");
    assert_eq!(slot(&v, 4), 0, "nothing earned");
    assert!(lists(&v, 10, item), "listed as not turned into money");
    assert!(!lists(&v, 2, item));
    assert_eq!(held_by(&h, corpse), vec![item], "still in the corpse");
    assert!(h.server.things.get(item).is_some(), "not destroyed");
    let paid = h
        .world_messages(b, from)
        .iter()
        .any(|m| matches!(m, ServerMessage::UpdateInt(q) if q.quality == 3100));
    assert!(!paid, "nothing paid");
}

/// A stack in a corpse turned into money pays for each of its pieces, as it would sold from the pack.
/// Behaviour: server.inventory.a-stack-pays-for-each-piece
#[test]
fn turning_a_corpses_stack_into_money_pays_for_each_piece() {
    let (mut h, b, me, _) = fight();
    let corpse = open_corpse(&mut h, b, me);
    let item = held_by(&h, corpse)[0];
    if let Some(t) = h.server.things.get_mut(item) {
        t.value = 7;
        t.quantity = 3;
    }
    let v = transmute(&mut h, b, me, corpse);
    assert_eq!(slot(&v, 4), 21, "earned");
}

/// Turning a corpse's items into money pays their seller's transmutation perk as the money bag does:
/// each piece's value times it, rounded down.
/// Behaviour: server.inventory.money-bag-pays-a-perk
#[test]
fn turning_a_corpses_items_into_money_pays_the_players_transmutation_perk() {
    let (mut h, b, me, _) = fight();
    h.server.skill_table.skills.insert(
        9001,
        yalain_world::skills::SkillDef {
            id: 9001,
            perk: Some(yalain_world::skills::Perk {
                kind: yalain_world::skills::perk_kind::TRANSMUTATION,
                value: 1.03,
                priority: 1,
            }),
            ..yalain_world::skills::SkillDef::default()
        },
    );
    h.server
        .players
        .values_mut()
        .find(|p| p.character == me)
        .expect("in")
        .play
        .skills
        .push(yalain_world::state::SkillState {
            skill: 9001,
            granted: 0,
            mask: 0,
            cost: 0,
            children: 0,
            dependents: 0,
            spent: 0,
        });
    let corpse = open_corpse(&mut h, b, me);
    let item = held_by(&h, corpse)[0];
    if let Some(t) = h.server.things.get_mut(item) {
        t.value = 150;
        t.quantity = 2;
    }
    let v = transmute(&mut h, b, me, corpse);
    assert_eq!(slot(&v, 4), 308, "154 each");
}

/// A quest item is kept back however much it is worth, while the corpse's other items are turned
/// into money: the answer's status 0, its error 1105.
/// Behaviour: server.loot.quest-items-kept-from-money
#[test]
fn turning_a_corpses_items_into_money_keeps_a_quest_item_and_pays_the_rest() {
    let mut h = Harness::new();
    h.add_creatures(1);
    let quest_item = 0x4700_2109;
    let mut props = yalain_content::Props::default();
    props.set("Item_IsQuestItem", yalain_content::Prop::Bool(true));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(quest_item, 0x200, props));
    h.server.loot.profiles[0].extras = vec![(quest_item, 1.0)];
    h.server.loot.profiles[0].counts = vec![0.0, 0.0, 1.0];
    h.server.levels = levels();
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    h.arm(b, me, STARTER_SWORD);
    let beetle = creature_of(&h, b);
    if let Some(c) = h
        .server
        .things
        .get_mut(beetle)
        .and_then(|t| t.creature.as_mut())
    {
        c.stats.natural_damage = 1.0;
    }
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(START_ATTACK, &[Arg::LongInt(beetle)]),
    );
    assert!(
        h.run_until(4000, |h| corpse_made(h, b, 0).is_some()),
        "it dies"
    );
    let corpse = open_corpse(&mut h, b, me);
    let held = held_by(&h, corpse);
    let quest = *held
        .iter()
        .find(|i| {
            h.server
                .things
                .get(**i)
                .is_some_and(|t| t.template == quest_item)
        })
        .expect("the quest item");
    let other = *held.iter().find(|i| **i != quest).expect("the other item");
    if let Some(t) = h.server.things.get_mut(quest) {
        t.value = 50;
    }
    let v = transmute(&mut h, b, me, corpse);
    assert_eq!(slot(&v, 0), 1105, "the error");
    assert_eq!(
        slot(&v, 7),
        0,
        "the status: something was turned into money"
    );
    assert!(slot(&v, 4) >= 1, "the other item paid");
    assert!(lists(&v, 10, quest) && !lists(&v, 2, quest));
    assert!(lists(&v, 2, other) && !lists(&v, 10, other));
    assert_eq!(held_by(&h, corpse), vec![quest], "the quest item stays");
}

/// What a treasure item carries of its own follows its dropper: the loot base is the loot quality
/// in tens (else the level); equipment's least level the base plus 0 to 3, every one of those
/// drawn, at most 50; Arcane Lore only above a base of 51, from the game's table, below the base's
/// own; a potion's effect of its level's tier (up to 15, 16-29, 30-39, 40-44, 45 up) at a
/// spellcraft from 0 to 100.
/// Behaviour: server.loot.treasure-own-properties-by-level
#[test]
fn a_treasure_items_own_properties_follow_its_droppers_level() {
    use yalain_world::rng::Rng;
    use yalain_world::treasure::{
        arcane_lore, base_of, lore_at, min_level, potion_effect, potion_tier, POTION_EFFECTS,
    };
    assert_eq!(base_of(43, Some(440)), 44);
    assert_eq!(base_of(20, None), 20);
    let tiers: Vec<usize> = [1, 15, 16, 29, 30, 39, 40, 44, 45, 60]
        .iter()
        .map(|l| potion_tier(*l))
        .collect();
    assert_eq!(tiers, [0, 0, 1, 1, 2, 2, 3, 3, 4, 4]);
    assert_eq!(lore_at(50.0), None);
    assert_eq!(lore_at(50.5), Some(8));
    assert_eq!(lore_at(51.0), Some(17));
    assert_eq!(lore_at(150.0), Some(887));
    let mut rng = Rng::new(7);
    let mut seen = [false; 4];
    let (mut low, mut high) = (100.0f32, 0.0f32);
    for _ in 0..2000 {
        let l = min_level(&mut rng, 20);
        assert!((20..=23).contains(&l), "{l}");
        seen[usize::try_from(l - 20).unwrap()] = true;
        assert!(min_level(&mut rng, 49) <= 50);
        assert_eq!(min_level(&mut rng, 60), 50);
        assert_eq!(arcane_lore(&mut rng, 51), None);
        let lore = arcane_lore(&mut rng, 57).expect("a base of 57 always draws above 50");
        assert!((17..=lore_at(56.0).unwrap()).contains(&lore), "{lore}");
        let (state, spellcraft) = potion_effect(&mut rng, 10);
        assert!(POTION_EFFECTS.iter().any(|k| k[0] == state), "{state:08X}");
        assert!((0.0..=100.0).contains(&spellcraft));
        low = low.min(spellcraft);
        high = high.max(spellcraft);
    }
    assert_eq!(seen, [true; 4], "each of the base plus 0 to 3 drawn");
    assert!(
        low < 5.0 && high > 95.0,
        "spellcraft over 0 to 100: {low} to {high}"
    );
    let some = (0..200)
        .filter(|_| arcane_lore(&mut rng, 53).is_some())
        .count();
    assert!(
        (40..=120).contains(&some),
        "about two in five above 50: {some}"
    );
}
