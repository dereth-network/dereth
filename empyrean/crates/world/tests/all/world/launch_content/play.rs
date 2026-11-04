//! Divergence: V462
//! A Launch character plays the 1999 world base on the launch dats: takes a portal, kills a
//! creature whose corpse holds treasure rolled by the world base's generator, buys from and sells
//! to its outpost vendor, brews a potion and cooks, and is saved with its possessions. Crafts and
//! rolls are seeded, and a test repeats one until it succeeds rather than counting on one draw.
//! Fixture: the hash-pinned launch dats and pack of `EMPYREAN_TEST_LAUNCH_FIXTURE`, driven through
//! the server's own handlers and world loop with injected time; no network host.

use std::time::Duration;

use empyrean_entity::enums::{
    CombatMode, DestinationType, MotionCommand, MotionStance, PositionType, Skill, WeenieType,
};
use empyrean_entity::ObjectGuid;
use empyrean_net::{SessionId, SessionState};
use empyrean_world::entity::actions::action_queue::run_actions;
use empyrean_world::entity::actions::i_actor::{action_queue_mut, Actor};
use empyrean_world::entity::item_profile::ItemProfile;
use empyrean_world::entity::landblock;
use empyrean_world::factories::world_object_factory as factory;
use empyrean_world::managers::recipe_manager as rm;
use empyrean_world::managers::{guid_manager, landblock_manager};
use empyrean_world::network::game_messages::game_message::{start_capture, take_sent};
use empyrean_world::network::motion::movement_data::Motion;
use empyrean_world::sessions::SessionData;
use empyrean_world::world_objects::world_object::{CtorEnv, WorldObject};
use empyrean_world::world_objects::{
    container, creature_combat, creature_death, player_commerce, player_database, player_inventory,
    player_location, portal, vendor,
};
use empyrean_world::World;

use super::{create, load_start, request, world};

const SESSION: SessionId = SessionId {
    client_id: 1,
    generation: 1,
};

/// A created Aluvian of the first template, standing at start `area` with a session.
fn enter(w: &mut World, area: usize) -> ObjectGuid {
    enter_as(w, area, 0)
}

/// A created Aluvian of `template`, standing at start `area` with a session.
fn enter_as(w: &mut World, area: usize, template: usize) -> ObjectGuid {
    let info = request(w, 1, 1, template, u32::try_from(area).unwrap());
    let created = create(w, &info);
    let guid = created.player.guid;
    load_start(w, area);
    for item in created
        .inventory
        .into_iter()
        .chain(created.equipped_objects)
    {
        w.objects.insert(item).expect("created possession");
    }
    w.objects.insert(created.player).expect("created player");
    w.sessions.insert(
        SESSION,
        SessionData {
            state: SessionState::WorldConnected,
            player: Some(guid),
            account: Some("offline-fixture".into()),
            ..Default::default()
        },
    );
    assert!(landblock_manager::add_object(w, guid, false));
    player_location::on_teleport_complete(w, guid);
    guid
}

/// Moves a placed player to stand beside `other`, as walking there would.
fn stand_beside(w: &mut World, player: ObjectGuid, other: ObjectGuid) {
    let mut at = w.objects.get(other).unwrap().location().unwrap();
    at.position_x += 0.5;
    empyrean_world::world_objects::player_tick::update_player_position(w, player, at, false);
}

/// `secs` of injected time, a tenth of a second a step, running the world's queues, the delay
/// manager and every loaded landblock's queue and tick as the world loop does between network
/// passes.
fn drain(w: &mut World, secs: u32, objects: &[ObjectGuid]) {
    for _ in 0..secs * 10 {
        w.now.utc = w.now.utc.add_seconds(0.1);
        w.now.unix_time += 0.1;
        w.now.monotonic += Duration::from_millis(100);
        w.now.portal_year_ticks += 0.1;
        run_actions(w, Actor::World);
        empyrean_world::entity::actions::delay_manager::run_actions(w);
        let blocks = landblock_manager::get_loaded_landblocks(w);
        let now = w.now.unix_time;
        for id in blocks {
            run_actions(w, Actor::Landblock(id));
            landblock::tick_multi_threaded_work(w, id, now);
            landblock::tick_single_threaded_work(w, id, now);
        }
        for &g in objects {
            if w.objects.contains(g) {
                run_actions(w, Actor::Object(g));
            }
        }
    }
}

/// The first Aluvian template that trains `skill`.
fn template_training(w: &World, skill: Skill) -> usize {
    let skill = u32::try_from(skill.0).unwrap();
    w.dats.portal_dat().char_gen().heritage_groups[&1]
        .templates
        .iter()
        .position(|t| t.normal_skills.contains(&skill) || t.primary_skills.contains(&skill))
        .expect("a template training the skill")
}

/// A new object of `wcid` in the world, and in `player`'s pack when one is named.
fn spawn(w: &mut World, wcid: u32, player: Option<ObjectGuid>) -> ObjectGuid {
    let weenie = w.content.get_cached_weenie(wcid).expect("pack weenie");
    let guid = guid_manager::new_dynamic_guid(w);
    let o = CtorEnv::with_world(w, |env| {
        factory::create_world_object(env, Some(weenie), guid)
    })
    .expect("constructible");
    assert!(w.objects.insert(o).is_ok());
    if let Some(player) = player {
        assert!(container::try_add_to_inventory(
            w, player, guid, 0, false, true
        ));
    }
    guid
}

fn pack(w: &World, player: ObjectGuid) -> Vec<u32> {
    container::inventory_values(w, player)
        .into_iter()
        .map(|g| w.objects.get(g).unwrap().biota.weenie_class_id)
        .collect()
}

fn health(w: &World, g: ObjectGuid) -> u32 {
    let o = w.objects.get(g).unwrap();
    o.health().current(o)
}

/// Every corpse in the world's dynamic objects.
fn corpses(w: &World) -> Vec<ObjectGuid> {
    (0x8000_0000..0x8000_4000)
        .map(ObjectGuid::new)
        .filter(|g| w.objects.get(*g).is_some_and(WorldObject::is_corpse))
        .collect()
}

#[test]
fn a_launch_player_takes_the_holtburg_tutorial_portal_to_its_destination() {
    let mut w = world();
    let player = enter(&mut w, 0);
    let portal = ObjectGuid::new(0x7A9B_000F);
    let destination = w
        .objects
        .get(portal)
        .and_then(WorldObject::destination)
        .expect("the tutorial portal is placed with its destination");
    stand_beside(&mut w, player, portal);
    // Past the pause a player who has just arrived waits before another portal.
    drain(&mut w, 4, &[player]);
    assert!(portal::portal_check_use_requirements(&mut w, portal, player).success);
    portal::portal_act_on_use(&mut w, portal, player);
    drain(&mut w, 2, &[player]);
    player_location::on_teleport_complete(&mut w, player);
    let at = w.objects.get(player).unwrap().location().unwrap();
    assert_eq!(at.cell(), destination.cell());
    assert!((at.position_x - destination.position_x).abs() < 0.5);
    assert!((at.position_y - destination.position_y).abs() < 0.5);
    assert_eq!(
        w.objects.get(player).unwrap().last_portal_did(),
        Some(w.objects.get(portal).unwrap().biota.weenie_class_id)
    );
}

#[test]
fn a_killed_drudge_s_corpse_holds_treasure_rolled_by_the_world_base_generator() {
    const SKULKER: u32 = 7;
    let mut w = world();
    let player = enter(&mut w, 0);
    let profile = w
        .content
        .get_cached_weenie(SKULKER)
        .and_then(|d| d.get_property(empyrean_entity::enums::PropertyDataId::DeathTreasureType))
        .expect("the skulker names a death profile");
    assert!(
        w.content.get_grouped_treasure(profile).is_some(),
        "its profile is one of the world base's own"
    );
    // Rolled until one corpse holds treasure; each roll is a fresh drudge.
    let mut looted = None;
    for seed in 0..40 {
        empyrean_common::thread_safe_random::ThreadSafeRandom::seed(seed);
        let drudge = spawn(&mut w, SKULKER, None);
        let mut at = w.objects.get(player).unwrap().location().unwrap();
        at.position_x += 3.0;
        w.objects.get_mut(drudge).unwrap().set_location(Some(at));
        assert!(landblock_manager::add_object(&mut w, drudge, false));
        let before = corpses(&w);
        creature_death::smite(&mut w, drudge, player, true);
        drain(&mut w, 10, &[drudge, player]);
        let corpse = corpses(&w)
            .into_iter()
            .find(|c| !before.contains(c))
            .expect("the drudge leaves a corpse");
        let items = creature_death::container_inventory(&w, corpse);
        if !items.is_empty() {
            looted = Some(items);
            break;
        }
    }
    let items = looted.expect("a tier 1 drudge drops treasure within forty kills");
    let create_list: Vec<u32> = w
        .content
        .get_cached_weenie(SKULKER)
        .unwrap()
        .properties_create_list
        .iter()
        .flat_map(|rows| rows.iter())
        .map(|c| c.weenie_class_id)
        .collect();
    assert!(
        items
            .iter()
            .any(|g| !create_list.contains(&w.objects.get(*g).unwrap().biota.weenie_class_id)),
        "something in the corpse was rolled, not listed"
    );
}

#[test]
fn a_practice_dummy_drops_its_food_list_as_treasure() {
    let w = world();
    let dummy = w.content.get_cached_weenie(2442).expect("the straw drudge");
    let treasure: Vec<_> = dummy
        .properties_create_list
        .iter()
        .flat_map(|rows| rows.iter())
        .filter(|c| i32::from(c.destination_type) == DestinationType::ContainTreasure.0)
        .map(|c| (c.weenie_class_id, c.shade))
        .collect();
    assert_eq!(
        treasure,
        [(5088, 0.16_f32), (5089, 0.16), (5090, 0.16)],
        "stale bread, old cheese and a bruised apple, each a sixth of the time"
    );
}

#[test]
fn a_launch_player_buys_from_and_sells_to_the_outpost_vendor() {
    let mut w = world();
    let player = enter(&mut w, 0);
    let id = w.objects.get(player).unwrap().current_landblock.unwrap();
    let shop = w
        .content
        .get_cached_instances_by_landblock(id.landblock())
        .iter()
        .find(|i| {
            w.content
                .get_cached_weenie(i.weenie_class_id)
                .is_some_and(|o| o.weenie_type == WeenieType::Vendor)
        })
        .map(|i| ObjectGuid::new(i.guid))
        .expect("the outpost vendor");
    stand_beside(&mut w, player, shop);
    vendor::load_inventory(&mut w, shop);
    let before = w.objects.get(player).unwrap().coin_value().unwrap_or(0);
    assert!(before > 0, "a starter carries pyreals");
    let cheapest = vendor::default_items_for_sale(&w, shop)
        .into_iter()
        .min_by_key(|g| w.objects.get(*g).unwrap().value().unwrap_or(i32::MAX))
        .expect("stock");
    let wcid = w.objects.get(cheapest).unwrap().biota.weenie_class_id;
    player_commerce::handle_action_buy_item(
        &mut w,
        player,
        shop.full(),
        vec![ItemProfile::new(1, cheapest.full())],
    );
    let after_buy = w.objects.get(player).unwrap().coin_value().unwrap_or(0);
    assert!(after_buy < before, "the purchase was paid for");
    let bought = container::inventory_values(&w, player)
        .into_iter()
        .find(|g| w.objects.get(*g).unwrap().biota.weenie_class_id == wcid)
        .expect("the bought item is in the pack");
    player_commerce::handle_action_sell_item(
        &mut w,
        player,
        shop.full(),
        vec![ItemProfile::new(1, bought.full())],
    );
    let after_sell = w.objects.get(player).unwrap().coin_value().unwrap_or(0);
    assert!(after_sell > after_buy, "the sale was paid for");
    assert!(!container::inventory_values(&w, player).contains(&bought));
}

/// Uses `source` on `target` through the confirmed path and completes the queued craft.
fn craft(w: &mut World, player: ObjectGuid, source: ObjectGuid, target: ObjectGuid) {
    creature_combat::set_combat_mode_field(w, player, CombatMode::NonCombat);
    w.objects
        .get_mut(player)
        .unwrap()
        .wo
        .world_object_properties
        .current_motion_state = Some(Motion::new(
        MotionStance::NonCombat,
        MotionCommand::Ready,
        1.0,
    ));
    rm::use_object_on_target(w, player, source, target, true);
    run_actions(w, Actor::Object(player));
    run_actions(w, Actor::Object(player));
    let queue = action_queue_mut(w, Actor::Object(player)).expect("player queue");
    assert_eq!(queue.act_exceptions(), 0);
    assert!(queue.is_empty());
}

/// Brews or cooks `source` on `target` until a craft succeeds, at most `tries` times with fresh
/// ingredients; each failure spends both and makes nothing. Returns the tries a success took.
fn craft_until_made(
    w: &mut World,
    player: ObjectGuid,
    (source, target, product): (u32, u32, u32),
    tries: u64,
    each: impl Fn(&World, bool),
) -> u64 {
    for seed in 0..tries {
        empyrean_common::thread_safe_random::ThreadSafeRandom::seed(seed);
        let (a, b) = (
            spawn(w, source, Some(player)),
            spawn(w, target, Some(player)),
        );
        let had = pack(w, player).iter().filter(|&&c| c == product).count();
        craft(w, player, a, b);
        assert!(
            !w.objects.contains(a) && !w.objects.contains(b),
            "both are spent"
        );
        let made = pack(w, player).iter().filter(|&&c| c == product).count() > had;
        each(w, made);
        if made {
            return seed + 1;
        }
    }
    panic!("no success in {tries} tries");
}

#[test]
fn a_launch_alchemist_brews_a_health_draught_at_its_health_cost() {
    const INFUSION: u32 = 5325;
    const BALM: u32 = 5338;
    const DRAUGHT: u32 = 2457;
    let mut w = world();
    let template = template_training(&w, Skill::Alchemy);
    let player = enter_as(&mut w, 0, template);
    let full = health(&w, player);
    // The recipe takes ten health on a success only.
    craft_until_made(&mut w, player, (INFUSION, BALM, DRAUGHT), 200, |w, made| {
        assert_eq!(health(w, player), if made { full - 10 } else { full });
    });
}

#[test]
fn a_launch_cook_makes_batter_from_dough_and_an_egg() {
    const DOUGH: u32 = 4760;
    const EGG: u32 = 546;
    const BATTER: u32 = 4752;
    let mut w = world();
    let template = template_training(&w, Skill::Cooking);
    let cook = enter_as(&mut w, 0, template);
    craft_until_made(&mut w, cook, (DOUGH, EGG, BATTER), 20, |_, _| {});
}

/// Takes `player` to stand beside `other` through the teleport path, loading its landblock.
fn go_to(w: &mut World, player: ObjectGuid, other: ObjectGuid) {
    let mut at = w.objects.get(other).expect("placed").location().unwrap();
    at.position_x += 0.5;
    player_location::teleport(w, player, &at, false);
    drain(w, 2, &[player]);
    player_location::on_teleport_complete(w, player);
    drain(w, 6, &[player]);
}

/// Gives `item` to `npc` through the give handler and runs the reaction out.
fn give_to(w: &mut World, player: ObjectGuid, npc: ObjectGuid, item: ObjectGuid) {
    player_inventory::handle_action_give_object_request(w, player, npc.full(), item.full(), 1);
    drain(w, 15, &[player, npc]);
}

/// Loads `block` as a player arriving there would, and finds the one object of class `wcid`
/// standing in it, placed or spawned by its generator.
fn npc_in(w: &mut World, block: u32, wcid: u32) -> ObjectGuid {
    landblock_manager::get_landblock(
        w,
        empyrean_entity::LandblockId::new(block << 16 | 0xFFFF),
        false,
        false,
    );
    drain(w, 3, &[]);
    let placed = (0..0x1000).map(|i| ObjectGuid::new(0x7000_0000 | block << 12 | i));
    let found: Vec<_> = placed
        .chain((0x8000_0000..0x8000_4000).map(ObjectGuid::new))
        .filter(|g| {
            w.objects
                .get(*g)
                .is_some_and(|o| o.biota.weenie_class_id == wcid)
        })
        .collect();
    assert_eq!(found.len(), 1, "one {wcid} in {block:04X}");
    found[0]
}

#[test]
fn a_launch_player_carries_the_wedding_band_and_its_letter_through_holtburg() {
    const BAND: u32 = 5032;
    const NOTE: u32 = 5040;
    const KEY: u32 = 5358;
    let mut w = world();
    let player = enter(&mut w, 0);
    let hardunna = npc_in(&mut w, 0xA9B0, 5034);
    let flinrala = npc_in(&mut w, 0xA9B4, 5033);

    let band = spawn(&mut w, BAND, Some(player));
    go_to(&mut w, player, hardunna);
    give_to(&mut w, player, hardunna, band);
    assert!(!pack(&w, player).contains(&BAND), "Hardunna takes the band");
    let note = container::inventory_values(&w, player)
        .into_iter()
        .find(|g| w.objects.get(*g).unwrap().biota.weenie_class_id == NOTE)
        .expect("and hands over a note for her sister");

    go_to(&mut w, player, flinrala);
    give_to(&mut w, player, flinrala, note);
    assert!(!pack(&w, player).contains(&NOTE), "Flinrala reads the note");
    assert!(
        pack(&w, player).contains(&KEY),
        "and gives the key to her father's tomb"
    );
}

#[test]
fn a_saved_launch_player_keeps_its_place_and_possessions_in_the_shard() {
    let mut w = world();
    let player = enter(&mut w, 0);
    let token = spawn(&mut w, 546, Some(player));
    let at = w.objects.get(player).unwrap().location().unwrap();
    for g in player_database::player_get_all_possessions(&w, player) {
        w.objects
            .get_mut(g)
            .unwrap()
            .wo
            .world_object_database
            .changes_detected = true;
    }
    player_database::save_player_to_database(&mut w, player);
    let saved = w
        .shard
        .base_database()
        .get_biota(player.full(), false)
        .expect("the player's biota is saved");
    let location = saved
        .biota_properties_position
        .iter()
        .find(|p| p.position_type == PositionType::Location.0)
        .expect("its location is saved");
    assert_eq!(location.obj_cell_id, at.cell());
    assert!((location.origin_x - at.position_x).abs() < 0.01);
    assert!(
        w.shard
            .base_database()
            .get_biota(token.full(), false)
            .is_some(),
        "the egg in its pack is saved with it"
    );
}

/// Divergence: V494
/// Defender I tested on a caster, against the 1999 tables: its gesture plays, the player returns to
/// ready, the client is told the cast ended and another may start, whether the roll fizzles or not.
#[test]
fn a_launch_player_tests_defender_on_a_caster_and_can_act_again() {
    use empyrean_entity::enums::{PropertyBool, PropertyInt};
    use empyrean_world::network::game_messages::game_message::{start_capture, take_sent};
    use empyrean_world::world_objects::{player_magic, spell_research};
    /// Defender I, as the 1999 spell table lays it.
    const DEFENDER: [u32; 5] = [1, 7, 36, 46, 57];

    let mut w = world();
    let player = enter(&mut w, 0);
    let item = container::inventory_values(&w, player)[0];
    w.objects
        .get_mut(item)
        .unwrap()
        .set_property(PropertyInt::ItemType, 0x8000);
    creature_combat::set_combat_mode_field(&mut w, player, CombatMode::Magic);
    let o = w.objects.get_mut(player).unwrap();
    o.wo.world_object_properties.current_motion_state =
        Some(Motion::new(MotionStance::Magic, MotionCommand::Ready, 1.0));
    o.set_property(PropertyBool::SpellComponentsRequired, false);

    start_capture();
    spell_research::handle_test_spell_formula(&mut w, player, &DEFENDER, item.full());
    assert!(player_magic::fields(&w, player).magic_state.is_casting);
    drain(&mut w, 4, &[player]);

    let (mut use_done, mut forwards) = (Vec::new(), Vec::new());
    for (_, _, b) in take_sent() {
        let op = u32::from_le_bytes(b[0..4].try_into().unwrap());
        if op == 0xF74C && u32::from_le_bytes(b[4..8].try_into().unwrap()) == player.full() {
            // The flagged state ends the fixed header: its style word, then its forward word.
            forwards.push(u16::from_le_bytes(b[26..28].try_into().unwrap()));
        } else if op == 0xF7B0 && u32::from_le_bytes(b[12..16].try_into().unwrap()) == 0x01C7 {
            use_done.push(u32::from_le_bytes(b[16..20].try_into().unwrap()));
        }
    }
    assert_eq!(use_done, [0], "the cast told the client it ended");
    assert!(
        forwards.contains(&0x37),
        "the talisman's gesture: {forwards:x?}"
    );
    assert_eq!(forwards.last(), Some(&3), "back to ready: {forwards:x?}");
    assert!(!player_magic::fields(&w, player).magic_state.is_casting);
    assert!(!player_magic::is_busy(&w, player));

    spell_research::handle_test_spell_formula(&mut w, player, &DEFENDER, item.full());
    assert!(
        player_magic::fields(&w, player).magic_state.is_casting,
        "the player acts again"
    );
}

/// Divergence: V495
/// No 1999 motion table has a logout gesture: a Launch character leaves with the lifestone gesture,
/// and leaves after it has played.
#[test]
fn a_launch_player_logs_out_with_the_lifestone_gesture() {
    use empyrean_world::network::game_messages::game_message::{start_capture, take_sent};
    use empyrean_world::physics::motion_table as mt;
    use empyrean_world::world_objects::player;
    /// `MotionCommand::Sanctuary`'s index on the wire.
    const SANCTUARY: [u8; 2] = [0x57, 0];

    let mut w = world();
    let guid = enter(&mut w, 0);
    let table = w.objects.get(guid).unwrap().motion_table_id();
    let length = |w: &World, motion| {
        mt::get_animation_length(w, table, MotionStance::NonCombat, motion, 1.0)
    };
    assert_eq!(length(&w, MotionCommand::LogOut), 0.0, "no logout gesture");
    assert!(
        length(&w, MotionCommand::Sanctuary) > 1.0,
        "the lifestone gesture"
    );
    assert_eq!(player::logout_motion(&w, table), MotionCommand::Sanctuary);

    start_capture();
    assert!(player::log_out(&mut w, guid, false, false));
    drain(&mut w, 1, &[guid]);
    assert!(
        take_sent().iter().any(|(_, _, b)| {
            u32::from_le_bytes(b[0..4].try_into().unwrap()) == 0xF74C
                && u32::from_le_bytes(b[4..8].try_into().unwrap()) == guid.full()
                && b.windows(2).any(|w| w == SANCTUARY)
        }),
        "the gesture went out"
    );
}

/// Divergence: V501
/// A Drudge Skulker, level 2 with no experience of its own, pays what the level curve says to the
/// one who killed it; a creature's own number wins, and 0 pays nothing.
#[test]
fn a_launch_creature_pays_the_level_curve_unless_it_sets_its_own() {
    use empyrean_common::era::{EraExt, EraFormulas};
    use empyrean_entity::enums::{PropertyInt, PropertyInt64};
    use empyrean_world::world_objects::creature_death::kill_xp;
    const SKULKER: u32 = 7;

    let mut w = world();
    let player = enter(&mut w, 0);
    let drudge = spawn(&mut w, SKULKER, None);
    let o = w.objects.get(drudge).unwrap();
    assert_eq!(o.level(), Some(2));
    assert_eq!(o.xp_override(), None);
    assert_eq!(EraFormulas::kill_xp_for_level(2), 23);
    assert_eq!(kill_xp(&w, o), 23);

    let total = |w: &World| {
        w.objects
            .get(player)
            .unwrap()
            .get_property(PropertyInt64::TotalExperience)
            .unwrap_or(0)
    };
    let before = total(&w);
    let mut at = w.objects.get(player).unwrap().location().unwrap();
    at.position_x += 3.0;
    w.objects.get_mut(drudge).unwrap().set_location(Some(at));
    assert!(landblock_manager::add_object(&mut w, drudge, false));
    creature_death::smite(&mut w, drudge, player, true);
    drain(&mut w, 2, &[drudge, player]);
    assert_eq!(total(&w) - before, 23, "the kill paid the curve");

    let drudge = spawn(&mut w, SKULKER, None);
    let o = w.objects.get_mut(drudge).unwrap();
    o.set_property(PropertyInt::XpOverride, 500);
    let o = w.objects.get(drudge).unwrap();
    assert_eq!(kill_xp(&w, o), 500, "its own number");
    let o = w.objects.get_mut(drudge).unwrap();
    o.set_property(PropertyInt::XpOverride, 0);
    let o = w.objects.get(drudge).unwrap();
    assert_eq!(kill_xp(&w, o), 0, "its own number, 0");

    w.era = empyrean_common::era::EraId::Eor.rules();
    let plain = spawn(&mut w, SKULKER, None);
    assert_eq!(
        kill_xp(&w, w.objects.get(plain).unwrap()),
        0,
        "other eras pay the creature's"
    );
}

/// Divergence: V500
/// A Drudge Skulker's innate resistances are for magic: a weapon's hit on it takes only what is
/// cast on it, on the world whose armor numbers rest on that, and all of them elsewhere.
#[test]
fn a_launch_skulker_takes_a_weapons_hit_without_its_innate_resistances() {
    use empyrean_common::era::EraExt;
    use empyrean_entity::enums::{DamageType, PropertyFloat, ResistanceType};
    use empyrean_world::world_objects::creature_properties as cp;
    const SKULKER: u32 = 7;

    let mut w = world();
    let drudge = spawn(&mut w, SKULKER, None);
    let o = w.objects.get(drudge).unwrap();
    let stored = |key| o.get_property(key).unwrap_or(1.0);
    let innate = [
        (
            DamageType::Slash,
            ResistanceType::Slash,
            stored(PropertyFloat::ResistSlash),
        ),
        (
            DamageType::Pierce,
            ResistanceType::Pierce,
            stored(PropertyFloat::ResistPierce),
        ),
        (
            DamageType::Bludgeon,
            ResistanceType::Bludgeon,
            stored(PropertyFloat::ResistBludgeon),
        ),
    ];
    for (damage, resistance, value) in innate {
        assert!(value < 1.0, "the data's {damage:?} resistance: {value}");
        let hit = cp::physical_hit_resistance_mod(&mut w, drudge, damage, None, None, 1.0);
        assert!((hit - 1.0).abs() < 1e-6, "{damage:?}: {hit}");
        let later = cp::get_resistance_mod(&mut w, drudge, resistance, None, None, 1.0);
        assert!(
            (later - value).abs() < 1e-3,
            "{damage:?}: {later} against {value}"
        );
    }
    assert!(w.era.formulas.physical_hits_skip_innate_resistance);
    assert!(
        !empyrean_common::era::EraId::Eor
            .rules()
            .formulas
            .physical_hits_skip_innate_resistance
    );
}

/// A wand carrying Strength Self I with mana, in `player`'s pack, as the world base's loot makes one.
fn magic_wand(w: &mut World, player: ObjectGuid) -> ObjectGuid {
    const STRENGTH_SELF_I: i32 = 2;
    let wand = spawn(w, 4914, Some(player));
    let o = w.objects.get_mut(wand).unwrap();
    o.biota
        .properties_spell_book
        .get_or_insert_with(Default::default)
        .insert(STRENGTH_SELF_I, 2.0);
    o.set_item_max_mana(Some(100));
    o.set_item_cur_mana(Some(100));
    wand
}

/// Wields `item` through the client's action and returns whether its spell reached the wielder.
fn wield_and_read_magic(w: &mut World, player: ObjectGuid, item: ObjectGuid) -> (bool, bool) {
    use empyrean_entity::enums::EquipMask;
    use empyrean_world::world_objects::creature_equipment;
    const STRENGTH_SELF_I: u32 = 2;
    player_inventory::handle_action_get_and_wield_item(w, player, item.full(), EquipMask::Held);
    drain(w, 3, &[player]);
    let wielded = creature_equipment::get_equipped_wand(w, player) == Some(item);
    let magic = w
        .objects
        .get(player)
        .unwrap()
        .biota
        .properties_enchantment_registry
        .as_ref()
        .is_some_and(|rows| rows.iter().any(|e| e.spell_id == STRENGTH_SELF_I as i32));
    (wielded, magic)
}

/// An item's gates hold back its magic, never the item: failing arcane lore, the item's skill, an
/// allegiance rank or a heritage leaves it in the hand with its mana whole, and the player told why.
#[test]
fn a_launch_item_failing_a_magic_gate_is_wielded_with_its_spells_off() {
    use empyrean_entity::enums::{PropertyDataId, PropertyInt};

    type Gate = fn(&mut WorldObject);
    let gates: [(&str, Gate); 4] = [
        ("Arcane Lore", |o| {
            o.set_property(PropertyInt::ItemDifficulty, 9999);
        }),
        ("Sword", |o| {
            let sword = u32::try_from(Skill::Sword.0).unwrap();
            o.set_property(PropertyDataId::ItemSkillLimit, sword);
            o.set_property(PropertyInt::ItemSkillLevelLimit, 9999);
        }),
        ("Allegiance Rank", |o| {
            o.set_property(PropertyInt::ItemAllegianceRankLimit, 2);
        }),
        ("Gharu", |o| {
            o.set_property(PropertyInt::HeritageGroup, 2);
        }),
    ];
    for (name, gate) in gates {
        let mut w = world();
        let player = enter(&mut w, 0);
        let wand = magic_wand(&mut w, player);
        gate(w.objects.get_mut(wand).unwrap());
        start_capture();
        let (wielded, magic) = wield_and_read_magic(&mut w, player, wand);
        let told = take_sent().iter().any(|(_, _, bytes)| {
            bytes
                .windows(name.len())
                .any(|part| part == name.as_bytes())
        });
        assert!(wielded, "{name}: the item is in the hand");
        assert!(!magic, "{name}: its spell is off");
        assert_eq!(
            w.objects.get(wand).unwrap().item_cur_mana(),
            Some(100),
            "{name}: no mana spent"
        );
        assert!(told, "{name}: the player hears why");
    }

    let mut w = world();
    let player = enter(&mut w, 0);
    let wand = magic_wand(&mut w, player);
    assert_eq!(
        wield_and_read_magic(&mut w, player, wand),
        (true, true),
        "with every gate met the spell lands"
    );
}

/// Divergence: V506
/// The world base's one weapon with a wield requirement goes in any hand: the 1999 client knows no
/// such requirement. Other eras keep the check.
#[test]
fn a_launch_lugian_mace_is_wielded_at_any_skill() {
    use empyrean_common::era::EraExt;
    use empyrean_entity::enums::{PropertyInt, WeenieError};
    use empyrean_world::managers::property_manager as pm;
    const LUGIAN_MACE: u32 = 543;

    let mut w = world();
    pm::initialize(&mut w, true);
    let player = enter(&mut w, 0);
    let mace = spawn(&mut w, LUGIAN_MACE, None);
    let o = w.objects.get(mace).unwrap();
    assert_eq!(o.get_property(PropertyInt::WieldDifficulty), Some(250));
    assert_eq!(
        player_inventory::check_wield_requirements(&mut w, player, mace),
        WeenieError::None
    );

    // An operator can turn the check on: the character has no row for the mace's skill.
    assert!(pm::modify_bool(&w, "use_wield_requirements", true));
    assert_eq!(
        player_inventory::check_wield_requirements(&mut w, player, mace),
        WeenieError::SkillTooLow
    );

    w.era = empyrean_common::era::EraId::Eor.rules();
    pm::initialize(&mut w, true);
    assert!(pm::get_bool(&w, "use_wield_requirements", false, true).item);
}
