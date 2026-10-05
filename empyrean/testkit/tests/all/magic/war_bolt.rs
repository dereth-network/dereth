//! ACE: Source/ACE.Server/WorldObjects/Player_Magic.cs::CreatePlayerSpell
//! A war bolt cast through the world loop flies, collides and kills the creature; a bolt at a
//! logged-off player flies straight ahead.
//! Fixture: a virtual-time TestServer, isolated stores and synthetic dats.

use std::collections::BTreeMap;
use std::sync::Arc;

use dereth_assets::tables::{SpellBase, SpellComponent};
use dereth_assets::{DidMapper, DualDidMapper, SpellComponentTable, SpellTable};
use dereth_primitives::DataId;
use dereth_protocol as proto;
use empyrean_common::dotnet::DotNetDict;
use empyrean_content::models::world::Spell as DbSpell;
use empyrean_content::models::world::Weenie as ContentWeenie;
use empyrean_content::MemContent;
use empyrean_dat::file_types::spell_table::compute_hash;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{
    CombatMode, DamageType, MagicSchool, MotionCommand as Mc, MotionStance, PhysicsState,
    PositionType, PropertyAttribute, PropertyAttribute2nd, PropertyBool, PropertyDataId,
    PropertyFloat, PropertyInt, PropertyInt64, PropertyString, Skill, SkillAdvancementClass,
    SpellType, WeenieType,
};
use empyrean_entity::models::properties_attribute::PropertiesAttribute;
use empyrean_entity::models::properties_attribute_2nd::PropertiesAttribute2nd;
use empyrean_entity::models::properties_skill::PropertiesSkill;
use empyrean_entity::{LandblockId, ObjectGuid, Position};
use empyrean_net::SessionState;
use empyrean_testkit::{dats, land, TestServer};
use empyrean_world::dispatch::Class;
use empyrean_world::entity::damage_history::DamageHistory;
use empyrean_world::managers::guid_manager as gm;
use empyrean_world::managers::landblock_manager as lm;
use empyrean_world::network::motion::movement_data::Motion;
use empyrean_world::physics::phys_ext;
use empyrean_world::world_objects::entity::creature_attribute::CreatureAttribute;
use empyrean_world::world_objects::entity::creature_vital::CreatureVital;
use empyrean_world::world_objects::world_object::WorldObject;
use empyrean_world::world_objects::{creature_combat, player_magic, world_object_tick};
use empyrean_world::World;

use empyrean_testkit::EmptyShard;

const LB: u32 = 0xA9B4_0000;
const PLAYER: ObjectGuid = ObjectGuid::new(0x5000_0001);
const TARGET: ObjectGuid = ObjectGuid::new(0x8000_0100);
const PROJ_WCID: u32 = 64100;
const CORPSE_WCID: u32 = 21;
const FORCE_BOLT: u32 = 3;
const FORMULA: [u32; 5] = [1, 10, 20, 30, 40];

fn spell_base() -> SpellBase {
    let (name, desc) = ("Force Bolt", "desc");
    let key = (compute_hash(name) % 0x1210_7680).wrapping_add(compute_hash(desc) % 0xBEAD_CF45);
    let mut raw_comps = [0u32; 8];
    for (slot, c) in raw_comps.iter_mut().zip(FORMULA) {
        *slot = c.wrapping_add(key);
    }
    SpellBase {
        name: name.into(),
        description: desc.into(),
        school: MagicSchool::WarMagic.0.cast_unsigned(),
        icon: 0,
        category: 1003,
        bitfield: 0,
        base_mana: 10,
        base_range_constant: 5.0,
        base_range_mod: 1.0,
        power: 10,
        spell_economy_mod: 0.0,
        formula_version: 0,
        component_loss: 0.5,
        meta_spell_type: SpellType::Projectile.0.cast_unsigned(),
        meta_spell_id: FORCE_BOLT,
        duration: None,
        portal_lifetime: None,
        raw_comps,
        comp_key: key,
        comps: FORMULA.to_vec(),
        caster_effect: 0,
        target_effect: 0,
        fizzle_effect: 0,
        recovery_interval: 0.0,
        recovery_amount: 0.0,
        display_order: 0,
        non_component_target_type: 0,
        mana_mod: 0,
    }
}

fn component(name: &str, component_type: u32, gesture: u32) -> SpellComponent {
    SpellComponent {
        name: name.into(),
        category: 0,
        icon: 0,
        component_type,
        gesture,
        time: 0.0,
        text: String::new(),
        cdm: 1.0,
    }
}

fn world_dats() -> Arc<empyrean_dat::DatManager> {
    let table = SpellTable {
        id: DataId(0x0E00_000E),
        spell_buckets: 64,
        spells: BTreeMap::from([(FORCE_BOLT, spell_base())]),
        spellset_bucket_index: 1,
        spellsets: BTreeMap::new(),
    };
    let comps = SpellComponentTable {
        id: DataId(0x0E00_000F),
        buckets: 256,
        components: BTreeMap::from([
            (1, component("Lead Scarab", 1, Mc::MagicHeal.0)),
            (10, component("Hyssop", 2, 0)),
            (20, component("Powdered Agate", 3, 0)),
            (30, component("Stibnite", 4, 0)),
            (40, component("Poplar Talisman", 5, Mc::MagicBlast.0)),
        ]),
    };
    let mapper = DualDidMapper(DidMapper {
        id: DataId(0x2700_0002),
        enum_to_id: vec![(1, 691), (10, 774), (20, 789), (30, 760), (40, 749)],
        enum_to_name: Vec::new(),
        enum_to_id_internal: Vec::new(),
        enum_to_name_internal: Vec::new(),
    });
    let sphere = |r: f32| dereth_assets::common::Sphere {
        center: dereth_primitives::Vec3::new(0.0, 0.0, 0.5),
        radius: r,
    };
    let setup = dereth_assets::geometry::Setup {
        id: DataId(land::TEST_SETUP),
        flags: 0,
        parts: Vec::new(),
        parent_index: None,
        default_scale: None,
        allow_free_heading: false,
        has_physics_bsp: false,
        holding_locations: BTreeMap::new(),
        connection_points: BTreeMap::new(),
        placement_frames: BTreeMap::new(),
        cylspheres: Vec::new(),
        spheres: vec![sphere(0.5)],
        height: 1.0,
        radius: 0.5,
        step_up_height: 0.0,
        step_down_height: 0.0,
        sorting_sphere: sphere(1.0),
        selection_sphere: sphere(1.0),
        lights: BTreeMap::new(),
        default_anim_id: DataId(0),
        default_script_id: DataId(0),
        default_mtable_id: DataId(0),
        default_stable_id: DataId(0),
        default_phstable_id: DataId(0),
    };
    dats::with_stat_tables(FakeDats::new())
        .with_spell_table(table)
        .with_spell_components_table(comps)
        .with_portal(0x2700_0002, mapper)
        .with_portal(land::TEST_SETUP, setup)
        .with_xp_table(empyrean_dat::fake::sample::xp_table())
        .build()
        .expect("fake dats")
}

fn content() -> MemContent {
    MemContent::new()
        .weenie(
            ContentWeenie::new(PROJ_WCID, "forcebolt", WeenieType::ProjectileSpell)
                .with_string(PropertyString::Name, "Force Bolt")
                .with_did(PropertyDataId::Setup, land::TEST_SETUP)
                .with_float(PropertyFloat::MaximumVelocity, 20.0),
        )
        .weenie(
            ContentWeenie::new(CORPSE_WCID, "corpse", WeenieType::Corpse)
                .with_string(PropertyString::Name, "Corpse")
                .with_did(PropertyDataId::Setup, land::TEST_SETUP),
        )
        .spell(DbSpell {
            id: FORCE_BOLT,
            name: "Force Bolt".into(),
            wcid: Some(PROJ_WCID),
            num_projectiles: Some(1),
            base_intensity: Some(500),
            variance: Some(0),
            e_type: Some(DamageType::Fire.0.cast_unsigned()),
            ..DbSpell::default()
        })
}

/// A creature as its constructor leaves it (vitals 100, attributes 60, the damage history, the
/// motion state), at `x, y`. Not placed.
fn creature(w: &mut World, class: Class, guid: ObjectGuid, name: &str, x: f32, y: f32) {
    let mut o = WorldObject::allocate(class);
    o.guid = guid;
    o.biota.id = guid.full();
    o.biota.weenie_type = WeenieType::Creature;
    o.biota.properties_enchantment_registry = Some(Vec::new());
    let vitals = [
        PropertyAttribute2nd::MaxHealth,
        PropertyAttribute2nd::MaxStamina,
        PropertyAttribute2nd::MaxMana,
    ];
    let mut v2 = DotNetDict::new();
    for v in vitals {
        v2.insert(
            v,
            PropertiesAttribute2nd {
                init_level: 100,
                level_from_cp: 0,
                cp_spent: 0,
                current_level: 100,
            },
        );
    }
    o.biota.properties_attribute_2nd = Some(v2);
    for v in vitals {
        let cv = CreatureVital::new(&mut o, v);
        o.vitals_mut().insert(v, cv);
    }
    let attributes = [
        PropertyAttribute::Strength,
        PropertyAttribute::Endurance,
        PropertyAttribute::Coordination,
        PropertyAttribute::Quickness,
        PropertyAttribute::Focus,
        PropertyAttribute::Self_,
    ];
    let mut a1 = DotNetDict::new();
    for a in attributes {
        a1.insert(
            a,
            PropertiesAttribute {
                init_level: 60,
                level_from_cp: 0,
                cp_spent: 0,
            },
        );
    }
    o.biota.properties_attribute = Some(a1);
    for a in attributes {
        let ca = CreatureAttribute::new(&mut o, a);
        o.attributes_mut().insert(a, ca);
    }
    o.set_property(PropertyString::Name, name.to_owned());
    o.set_property(PropertyDataId::Setup, land::TEST_SETUP);
    o.set_property(PropertyBool::Attackable, true);
    o.creature.as_mut().unwrap().creature_death.damage_history =
        DamageHistory::new(guid, w.now.utc);
    let pos = Position::from_components(LB | 0x0001, x, y, 20.0, 0.0, 0.0, 0.0, 1.0, false);
    o.set_location(Some(pos));
    o.set_position(PositionType::Home, Some(pos));
    o.wo.world_object_properties.current_motion_state =
        Some(Motion::new(MotionStance::NonCombat, Mc::Ready, 1.0));
    o.set_heartbeat_interval(Some(0.0));
    world_object_tick::world_object_initialize_heartbeats(&mut o, w.now.unix_time);
    w.objects.insert(o).expect("fresh guid");
}

fn health(ts: &TestServer, g: ObjectGuid) -> Option<u32> {
    ts.world.objects.get(g).map(|o| o.health().current(o))
}

#[test]
fn through_the_world_loop_a_player_casts_a_war_bolt_that_flies_hits_and_kills() {
    let mut ts = TestServer::with_setup(world_dats(), |w| {
        w.content = Arc::new(content());
        gm::initialize(w, &mut EmptyShard);
        land::use_flat_land_with_test_setup(w, &[0xA9B4], 10);
    });
    empyrean_world::world_objects::world_object_magic::clear_spell_cache();
    let client = ts.connect("acct", "pw");
    ts.advance(0.1);
    let session = ts
        .world
        .sessions
        .iter()
        .map(|(id, _)| id)
        .next()
        .expect("the session");
    let w = &mut ts.world;
    lm::get_landblock(w, LandblockId::new(LB | 0xFFFF), false, false);

    // the caster, entered as DoPlayerEnterWorld has it (session player set before the landblock)
    creature(w, Class::Player, PLAYER, "Caster", 100.0, 100.0);
    let o = w.objects.get_mut(PLAYER).unwrap();
    o.player.as_mut().expect("a player").player.character =
        Some(empyrean_store::models::shard::Character::default());
    let mut skills = DotNetDict::new();
    skills.insert(
        Skill::WarMagic,
        PropertiesSkill {
            init_level: 200,
            sac: SkillAdvancementClass::Trained,
            ..PropertiesSkill::default()
        },
    );
    o.biota.properties_skill = Some(skills);
    let mut book = DotNetDict::new();
    book.insert(i32::try_from(FORCE_BOLT).unwrap(), 2.0f32);
    o.biota.properties_spell_book = Some(book);
    o.set_spell_components_required(false);
    o.set_property(PropertyInt::Level, 1);
    o.set_property(PropertyInt64::TotalExperience, 0);
    o.set_property(PropertyInt64::AvailableExperience, 0);
    o.wo.world_object_properties.current_motion_state =
        Some(Motion::new(MotionStance::Magic, Mc::Ready, 1.0));
    let s = w.sessions.get_mut(session).expect("the game half");
    s.state = SessionState::WorldConnected;
    s.set_player(Some(PLAYER));
    assert!(
        lm::add_object(w, PLAYER, false),
        "the player joins its landblock"
    );
    phys_ext::set_physics_state(w, PLAYER, PhysicsState::Hidden, Some(false));
    creature_combat::set_combat_mode_field(w, PLAYER, CombatMode::Magic);

    // the target, 10 m north
    creature(w, Class::Creature, TARGET, "Drudge", 100.0, 110.0);
    assert!(
        lm::add_object(w, TARGET, false),
        "the drudge joins its landblock"
    );
    ts.advance(0.1);

    ts.send_game_action(
        client,
        &proto::combat::MagicCastTargetedSpell {
            target: dereth_primitives::ObjectId(TARGET.full()),
            spell_id: FORCE_BOLT,
        },
    );

    assert!(
        ts.run_until(1.0, |ts| player_magic::fields(&ts.world, PLAYER)
            .magic_state
            .is_casting),
        "the cast starts"
    );
    assert!(
        ts.run_until(3.0, |ts| health(ts, TARGET).is_none_or(|h| h == 0)),
        "the bolt kills the drudge"
    );
    assert!(
        ts.run_until(2.0, |ts| !player_magic::is_busy(&ts.world, PLAYER)),
        "the recoil ends"
    );
    ts.advance(0.2);

    // the client saw the cast finish (UseDone, a game event 0x01C7) and the kill
    let use_done = ts
        .received_raw(client)
        .iter()
        .filter(|m| {
            m.opcode == 0xF7B0
                && u32::from_le_bytes([m.body[8], m.body[9], m.body[10], m.body[11]]) == 0x01C7
        })
        .count();
    assert_eq!(use_done, 1, "one UseDone");
    // the kill: KillerNotification (0x01AD) names the drudge
    let killer = ts
        .received_raw(client)
        .iter()
        .filter(|m| {
            m.opcode == 0xF7B0
                && u32::from_le_bytes([m.body[8], m.body[9], m.body[10], m.body[11]]) == 0x01AD
        })
        .map(|m| String::from_utf8_lossy(&m.body[12..]).into_owned())
        .collect::<Vec<_>>();
    assert_eq!(killer.len(), 1, "one KillerNotification");
    assert!(killer[0].contains("Drudge"), "{killer:?}");
    // the projectile's impact, broadcast to the caster who sees it: SetState, the explosion
    // (PlayEffect) and the stop (VectorUpdate), in ProjectileImpact's order
    let opcodes: Vec<u32> = ts.received_raw(client).iter().map(|m| m.opcode).collect();
    let impact = opcodes.windows(3).any(|w| w == [0xF74B, 0xF755, 0xF74E]);
    assert!(impact, "SetState, PlayEffect, VectorUpdate in {opcodes:X?}");
    // the drudge, the projectile and the corpse were created on the client
    assert_eq!(
        opcodes.iter().filter(|&&o| o == 0xF745).count(),
        3,
        "{opcodes:X?}"
    );
    assert_eq!(
        player_magic::get_current_magic_skill(&ts.world, PLAYER),
        Skill::WarMagic
    );
}

/// A bolt at a logged off player flies straight ahead.
/// V325.
#[test]
fn a_bolt_at_a_logged_off_player_flies_straight_ahead() {
    let mut ts = TestServer::with_setup(world_dats(), |w| {
        w.content = Arc::new(content());
        gm::initialize(w, &mut EmptyShard);
        land::use_flat_land_with_test_setup(w, &[0xA9B4], 10);
    });
    empyrean_world::world_objects::world_object_magic::clear_spell_cache();
    let _client = ts.connect("acct", "pw");
    ts.advance(0.1);
    let session = ts
        .world
        .sessions
        .iter()
        .map(|(id, _)| id)
        .next()
        .expect("the session");
    let w = &mut ts.world;
    lm::get_landblock(w, LandblockId::new(LB | 0xFFFF), false, false);
    creature(w, Class::Player, PLAYER, "Target", 100.0, 100.0);
    let o = w.objects.get_mut(PLAYER).unwrap();
    o.player.as_mut().expect("a player").player.character =
        Some(empyrean_store::models::shard::Character::default());
    o.set_property(PropertyInt::Level, 1);
    let s = w.sessions.get_mut(session).expect("the game half");
    s.state = SessionState::WorldConnected;
    s.set_player(Some(PLAYER));
    assert!(
        lm::add_object(w, PLAYER, false),
        "the player joins its landblock"
    );
    // the caster, a drudge 10 m north
    creature(w, Class::Creature, TARGET, "Drudge", 100.0, 110.0);
    assert!(
        lm::add_object(w, TARGET, false),
        "the drudge joins its landblock"
    );

    let spell = empyrean_world::entity::spell::Spell::new(w, FORCE_BOLT, true);
    let bolt = empyrean_world::entity::spell_projectile_type::ProjectileSpellType::Bolt;
    let live = empyrean_world::world_objects::world_object_magic::calculate_projectile_velocity(
        w,
        TARGET,
        &spell,
        Some(PLAYER),
        bolt,
        empyrean_common::dotnet::Vector3::ZERO,
    );
    assert!(
        live.y < 0.0 && live.x.abs() < 1e-3,
        "south, at the player: {live:?}"
    );

    empyrean_world::world_objects::player::finalize_logout(w, PLAYER);
    assert!(
        phys_ext::physics_obj(w, PLAYER).is_none(),
        "off its landblock: no body"
    );
    let v = empyrean_world::world_objects::world_object_magic::calculate_projectile_velocity(
        w,
        TARGET,
        &spell,
        Some(PLAYER),
        bolt,
        empyrean_common::dotnet::Vector3::ZERO,
    );
    let untargeted =
        empyrean_world::world_objects::world_object_magic::calculate_projectile_velocity(
            w,
            TARGET,
            &spell,
            None,
            bolt,
            empyrean_common::dotnet::Vector3::ZERO,
        );
    assert_eq!(v, untargeted, "as with no target");
    // the drudge faces north (identity rotation): straight ahead, not towards block 0,0
    assert!(v.y > 0.0 && v.x.abs() < 1e-3 && v.z.abs() < 1e-3, "{v:?}");
}
