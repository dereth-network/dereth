//! ACE: Source/ACE.Server/WorldObjects/Player_Melee.cs::HandleActionTargetedMeleeAttack
//! Sword attack kills a drudge; repeat attacks follow power bar/queue; bow arrow reaches target;
//! no arrow at a target gone during windup; arrow passes non-targets; cross-cell shot; spawn
//! origin radius; missile mode without weapon reverts; charge attack; sticky range; cleave skip.
//! Fixture: a virtual-time TestServer, isolated stores and synthetic dats.

use std::collections::BTreeMap;
use std::sync::Arc;

use dereth_assets::geometry::AnimFrame;
use dereth_assets::motion::{AnimData, MotionData};
use dereth_assets::tables::CombatManeuver;
use dereth_assets::{AnimHook, Animation, CombatManeuverTable, HookData, MotionTable};
use dereth_primitives::{DataId, ObjectId, Vec3};
use dereth_protocol::combat::{
    AttackerNotification, CombatChangeCombatMode, CombatHandleAttackDoneEvent,
    CombatTargetedMeleeAttack, CombatTargetedMissileAttack,
};
use dereth_protocol::events::split_ui_blob;
use dereth_protocol::objects::ItemCreateObject;
use dereth_protocol::Message;
use empyrean_common::dotnet::DotNetDict;
use empyrean_common::not_ported::take_local;
use empyrean_content::models::world::{Weenie, WeeniePropertiesBodyPart};
use empyrean_content::MemContent;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{
    AttackType, CharacterOptions1, CombatBodyPart, CombatMode, CombatStyle, DamageType, EquipMask,
    MotionCommand as Mc, MotionStance, PhysicsState, PropertyAttribute, PropertyAttribute2nd,
    PropertyBool, PropertyDataId, PropertyFloat, PropertyInt, PropertyString, Skill,
    SkillAdvancementClass, Tolerance, WeenieType,
};
use empyrean_entity::models::properties_attribute::PropertiesAttribute;
use empyrean_entity::models::properties_attribute_2nd::PropertiesAttribute2nd;
use empyrean_entity::models::properties_skill::PropertiesSkill;
use empyrean_entity::models::PropertiesBodyPart;
use empyrean_entity::{LandblockId, ObjectGuid, Position};
use empyrean_net::{SessionId, SessionState};
use empyrean_testkit::{dats, land, ClientId, TestServer};
use empyrean_world::dispatch::Class;
use empyrean_world::entity::damage_history::DamageHistory;
use empyrean_world::managers::guid_manager::{self as gm, ShardGuidQueries};
use empyrean_world::managers::landblock_manager as lm;
use empyrean_world::network::motion::movement_data::Motion;
use empyrean_world::physics::phys_ext;
use empyrean_world::world_objects::entity::creature_attribute::CreatureAttribute;
use empyrean_world::world_objects::entity::creature_vital::CreatureVital;
use empyrean_world::world_objects::world_object::{CtorEnv, WorldObject};
use empyrean_world::world_objects::{
    creature_combat, creature_equipment, player_combat, player_melee, world_object_tick,
};
use empyrean_world::World;

struct EmptyShard;

impl ShardGuidQueries for EmptyShard {
    fn get_max_guid_found_in_range(&mut self, _min: u32, _max: u32) -> u32 {
        u32::MAX
    }

    fn get_sequence_gaps(&mut self, _min: u32, _limit: u32) -> Vec<(u32, u32)> {
        Vec::new()
    }
}

const LB: u32 = 0xA9B4_0000;
const MT: u32 = 0x0900_0320;
const CMT: u32 = 0x3000_0320;
const CYCLE: u32 = 0x0300_0220;
const STANCE: u32 = 0x0300_0221;
const SLASH: u32 = 0x0300_0222;
const RELOAD: u32 = 0x0300_0223;
const ARROW_SETUP: u32 = 0x0200_1020;
const PLAYER: ObjectGuid = ObjectGuid::new(0x5000_0001);
const MONSTER: ObjectGuid = ObjectGuid::new(0x8000_0100);
const NC: u32 = MotionStance::NonCombat.0;
const SC: u32 = MotionStance::SwordCombat.0;
const BC: u32 = MotionStance::BowCombat.0;

const SWORD: u32 = 5;
const CLEAVER: u32 = 6;
const DRUDGE: u32 = 7;
const BOW: u32 = 8;
const ARROW: u32 = 9;
const CORPSE: u32 = 21;

// message kinds: the game-event type inside a 0xF7B0, else the opcode
const ATTACK_DONE: u32 = 0x01A7;
const ATTACKER_NOTE: u32 = 0x01B1;
const EVADED_NOTE: u32 = 0x01B3;
const KILLER_NOTE: u32 = 0x01AD;
const WEENIE_ERROR_WITH_STRING: u32 = 0x028B;
const PRIVATE_INT: u32 = 0x02CD;
const CREATE_OBJECT: u32 = 0xF745;
const MOVEMENT: u32 = 0xF74C;
const SOUND: u32 = 0xF750;

// ------------------------------------------------------------------ synthetic content

fn anim(id: u32) -> AnimData {
    AnimData {
        anim_id: DataId(id),
        low_frame: 0,
        high_frame: -1,
        framerate: 30.0,
    }
}

fn data(key: u32, anims: Vec<AnimData>) -> MotionData {
    MotionData {
        key,
        bitfield: 0,
        flags: 0,
        anims,
        velocity: None,
        omega: None,
    }
}

fn key(style: u32, motion: u32) -> u32 {
    style.wrapping_shl(16) | (motion & 0xFF_FFFF)
}

fn animation(id: u32, n: u32, hook_frame: Option<u32>) -> Animation {
    let attack = AnimHook {
        hook_type: 3,
        direction: 1,
        data: HookData::Attack(dereth_primitives::records::AttackCone {
            part_index: 0,
            left: (0.0, 0.0),
            right: (0.0, 0.0),
            radius: 1.0,
            height: 1.0,
        }),
    };
    let part_frames = (0..n)
        .map(|i| AnimFrame {
            frames: Vec::new(),
            hooks: if Some(i) == hook_frame {
                vec![attack.clone()]
            } else {
                Vec::new()
            },
        })
        .collect();
    Animation {
        id: DataId(id),
        flags: 0,
        num_parts: 0,
        num_frames: n,
        has_hooks: hook_frame.is_some(),
        pos_frames: None,
        part_frames,
    }
}

fn world_dats() -> Arc<empyrean_dat::DatManager> {
    let mut cycles = Vec::new();
    let mut style_defaults = BTreeMap::new();
    let mut modifiers = Vec::new();
    for style in [NC, SC, BC] {
        style_defaults.insert(style, Mc::Ready.0);
        cycles.push(data(key(style, Mc::Ready.0), vec![anim(CYCLE)]));
        // the charge: a run cycle and a turn
        cycles.push(MotionData {
            velocity: Some(Vec3::new(0.0, 4.0, 0.0)),
            ..data(key(style, Mc::RunForward.0), vec![anim(CYCLE)])
        });
        let omega = Some(Vec3::new(0.0, 0.0, -std::f32::consts::FRAC_PI_2));
        cycles.push(MotionData {
            bitfield: 2,
            omega,
            ..data(key(style, Mc::TurnRight.0), vec![anim(CYCLE)])
        });
        modifiers.push(MotionData {
            omega,
            ..data(key(style, Mc::TurnRight.0), Vec::new())
        });
    }
    let mut links = BTreeMap::new();
    links.insert(
        key(NC, Mc::Ready.0),
        vec![data(SC, vec![anim(STANCE)]), data(BC, vec![anim(STANCE)])],
    );
    links.insert(
        key(SC, Mc::Ready.0),
        vec![
            data(NC, vec![anim(STANCE)]),
            data(Mc::SlashMed.0, vec![anim(SLASH)]),
        ],
    );
    links.insert(
        key(BC, Mc::Ready.0),
        vec![
            data(NC, vec![anim(STANCE)]),
            data(Mc::Reload.0, vec![anim(RELOAD)]),
        ],
    );
    links.insert(
        key(BC, Mc::Reload.0),
        vec![data(Mc::Ready.0, vec![anim(STANCE)])],
    );
    let table = MotionTable {
        id: DataId(MT),
        default_style: NC,
        style_defaults,
        cycles,
        modifiers,
        links,
    };
    let maneuver = |height: u32| CombatManeuver {
        style: SC,
        attack_height: height,
        attack_type: AttackType::Slash.0.cast_unsigned(),
        min_skill_level: 0,
        motion: Mc::SlashMed.0,
    };
    let cmt = CombatManeuverTable {
        id: DataId(CMT),
        maneuvers: vec![maneuver(1), maneuver(2), maneuver(3)],
    };
    dats::with_stat_tables(FakeDats::new().with_xp_table(empyrean_dat::fake::sample::xp_table()))
        .with_portal(MT, table)
        .with_portal(CMT, cmt)
        .with_portal(CYCLE, animation(CYCLE, 10, None))
        .with_portal(STANCE, animation(STANCE, 15, None))
        .with_portal(SLASH, animation(SLASH, 30, Some(10)))
        .with_portal(RELOAD, animation(RELOAD, 15, None))
        .with_portal(ARROW_SETUP, arrow_setup())
        .build()
        .expect("fake dats")
}

/// The arrow's dat setup (`GetProjectileRadius` reads its first sphere): one 5 cm sphere.
fn arrow_setup() -> dereth_assets::Setup {
    let sphere = |r: f32| dereth_assets::Sphere {
        center: Vec3::new(0.0, 0.0, r),
        radius: r,
    };
    dereth_assets::Setup {
        id: DataId(ARROW_SETUP),
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
        spheres: vec![sphere(0.05)],
        height: 0.1,
        radius: 0.05,
        step_up_height: 0.0,
        step_down_height: 0.0,
        sorting_sphere: sphere(0.05),
        selection_sphere: sphere(0.05),
        lights: BTreeMap::new(),
        default_anim_id: DataId(0),
        default_script_id: DataId(0),
        default_mtable_id: DataId(0),
        default_stable_id: DataId(0),
        default_phstable_id: DataId(0),
    }
}

fn weenie(wcid: u32, name: &str, weenie_type: WeenieType) -> Weenie {
    Weenie::new(wcid, name, weenie_type)
        .with_string(PropertyString::Name, name)
        .with_did(PropertyDataId::Setup, land::TEST_SETUP)
}

/// The drudge's one body part (every quadrant), strong enough to be hit and weak armour.
fn drudge_part() -> PropertiesBodyPart {
    PropertiesBodyPart {
        d_type: DamageType::Bludgeon,
        d_val: 5,
        d_var: 0.5,
        base_armor: 0,
        bh: 1,
        hlf: 1.0,
        mlf: 1.0,
        llf: 1.0,
        hrf: 1.0,
        mrf: 1.0,
        lrf: 1.0,
        hlb: 1.0,
        mlb: 1.0,
        llb: 1.0,
        hrb: 1.0,
        mrb: 1.0,
        lrb: 1.0,
        ..PropertiesBodyPart::default()
    }
}

fn content() -> MemContent {
    let mut drudge = weenie(DRUDGE, "Drudge", WeenieType::Creature);
    let p = drudge_part();
    drudge
        .weenie_properties_body_part
        .push(WeeniePropertiesBodyPart {
            id: 1,
            object_id: DRUDGE,
            key: u16::try_from(CombatBodyPart::Chest.0).unwrap(),
            d_type: p.d_type.0,
            d_val: p.d_val,
            d_var: p.d_var,
            base_armor: p.base_armor,
            bh: p.bh,
            hlf: 1.0,
            mlf: 1.0,
            llf: 1.0,
            hrf: 1.0,
            mrf: 1.0,
            lrf: 1.0,
            hlb: 1.0,
            mlb: 1.0,
            llb: 1.0,
            hrb: 1.0,
            mrb: 1.0,
            lrb: 1.0,
            ..WeeniePropertiesBodyPart::default()
        });
    MemContent::new()
        .weenie(drudge)
        .weenie(weenie(CORPSE, "Corpse", WeenieType::Corpse))
        .weenie(
            weenie(SWORD, "Sword", WeenieType::MeleeWeapon)
                .with_int(
                    PropertyInt::ValidLocations,
                    i32::try_from(EquipMask::MeleeWeapon.0).unwrap(),
                )
                .with_int(PropertyInt::DefaultCombatStyle, CombatStyle::OneHanded.0)
                .with_int(PropertyInt::WeaponSkill, Skill::HeavyWeapons.0)
                .with_int(PropertyInt::DamageType, DamageType::Slash.0)
                .with_int(PropertyInt::AttackType, AttackType::Slash.0)
                .with_int(PropertyInt::Damage, 40)
                .with_float(PropertyFloat::DamageVariance, 0.1)
                .with_int(PropertyInt::WeaponTime, 30)
                .with_int(PropertyInt::EncumbranceVal, 700),
        )
        .weenie(
            weenie(CLEAVER, "Cleaver", WeenieType::MeleeWeapon)
                .with_int(
                    PropertyInt::ValidLocations,
                    i32::try_from(EquipMask::MeleeWeapon.0).unwrap(),
                )
                .with_int(PropertyInt::DefaultCombatStyle, CombatStyle::OneHanded.0)
                .with_int(PropertyInt::WeaponSkill, Skill::HeavyWeapons.0)
                .with_int(PropertyInt::DamageType, DamageType::Slash.0)
                .with_int(PropertyInt::AttackType, AttackType::Slash.0)
                .with_int(PropertyInt::Damage, 40)
                .with_float(PropertyFloat::DamageVariance, 0.1)
                .with_int(PropertyInt::WeaponTime, 30)
                .with_int(PropertyInt::Cleaving, 2)
                .with_int(PropertyInt::EncumbranceVal, 700),
        )
        .weenie(
            weenie(BOW, "Bow", WeenieType::MissileLauncher)
                .with_int(
                    PropertyInt::ValidLocations,
                    i32::try_from(EquipMask::MissileWeapon.0).unwrap(),
                )
                .with_int(PropertyInt::DefaultCombatStyle, CombatStyle::Bow.0)
                .with_int(PropertyInt::WeaponSkill, Skill::MissileWeapons.0)
                .with_int(PropertyInt::AmmoType, 1)
                .with_float(PropertyFloat::MaximumVelocity, 30.0)
                .with_int(PropertyInt::WeaponTime, 30)
                .with_int(PropertyInt::EncumbranceVal, 400),
        )
        .weenie(
            Weenie::new(ARROW, "Arrow", WeenieType::Ammunition)
                .with_string(PropertyString::Name, "Arrow")
                .with_did(PropertyDataId::Setup, ARROW_SETUP)
                .with_int(
                    PropertyInt::ValidLocations,
                    i32::try_from(EquipMask::MissileAmmo.0).unwrap(),
                )
                .with_int(PropertyInt::AmmoType, 1)
                .with_int(PropertyInt::DamageType, DamageType::Pierce.0)
                .with_int(PropertyInt::Damage, 40)
                .with_float(PropertyFloat::DamageVariance, 0.1)
                .with_int(PropertyInt::MaxStackSize, 100)
                .with_int(PropertyInt::StackSize, 10)
                .with_int(PropertyInt::EncumbranceVal, 20)
                .with_int(PropertyInt::StackUnitEncumbrance, 2)
                .with_bool(PropertyBool::GravityStatus, true),
        )
}

/// A creature as its constructor leaves it (vitals, attributes, skills, motion state, damage
/// history), home where it stands. Not placed.
#[allow(clippy::too_many_arguments)]
fn creature(
    w: &mut World,
    class: Class,
    guid: ObjectGuid,
    wcid: u32,
    name: &str,
    health: u32,
    skills: &[(Skill, u32)],
    x: f32,
    y: f32,
) {
    let mut o = WorldObject::allocate(class);
    o.guid = guid;
    o.biota.id = guid.full();
    o.biota.weenie_class_id = wcid;
    o.biota.properties_enchantment_registry = Some(Vec::new());
    let mut v2 = DotNetDict::new();
    for (v, level) in [
        (PropertyAttribute2nd::MaxHealth, health),
        (PropertyAttribute2nd::MaxStamina, 100),
        (PropertyAttribute2nd::MaxMana, 100),
    ] {
        v2.insert(
            v,
            PropertiesAttribute2nd {
                init_level: level,
                level_from_cp: 0,
                cp_spent: 0,
                current_level: level,
            },
        );
    }
    o.biota.properties_attribute_2nd = Some(v2);
    for v in [
        PropertyAttribute2nd::MaxHealth,
        PropertyAttribute2nd::MaxStamina,
        PropertyAttribute2nd::MaxMana,
    ] {
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
                init_level: 100,
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
    let mut d = DotNetDict::new();
    for &(s, level) in skills {
        d.insert(
            s,
            PropertiesSkill {
                init_level: level,
                sac: SkillAdvancementClass::Specialized,
                ..PropertiesSkill::default()
            },
        );
    }
    o.biota.properties_skill = Some(d);
    o.set_property(PropertyString::Name, name.to_owned());
    o.set_property(PropertyDataId::Setup, land::TEST_SETUP);
    o.set_property(PropertyDataId::MotionTable, MT);
    o.set_property(PropertyDataId::CombatTable, CMT);
    o.set_property(PropertyBool::Attackable, true);
    o.creature.as_mut().unwrap().creature_death.damage_history =
        DamageHistory::new(guid, w.now.utc);
    let pos = Position::from_components(LB | 0x0001, x, y, 20.0, 0.0, 0.0, 0.0, 1.0, false);
    o.set_location(Some(pos));
    o.set_position(empyrean_entity::enums::PositionType::Home, Some(pos));
    o.wo.world_object_properties.current_motion_state =
        Some(Motion::new(MotionStance::NonCombat, Mc::Ready, 1.0));
    o.set_heartbeat_interval(Some(0.0));
    world_object_tick::world_object_initialize_heartbeats(&mut o, w.now.unix_time);
    w.objects.insert(o).expect("fresh guid");
}

/// A new object of `wcid` in the store (`WorldObjectFactory.CreateNewWorldObject`).
fn new_object(w: &mut World, wcid: u32) -> ObjectGuid {
    let weenie = w.content.get_cached_weenie(wcid).expect("test weenie");
    let guid = gm::new_dynamic_guid(w);
    let o = CtorEnv::with_world(w, |env| {
        empyrean_world::factories::world_object_factory::create_world_object(
            env,
            Some(weenie),
            guid,
        )
    })
    .expect("constructible");
    w.objects.insert(o).expect("fresh");
    guid
}

/// A server on flat land with the synthetic content, the arrow's small sphere registered.
fn server() -> TestServer {
    let mut ts = TestServer::with_setup(world_dats(), |w| {
        w.content = Arc::new(content());
        gm::initialize(w, &mut EmptyShard);
        land::use_flat_land_with_test_setup(w, &[0xA9B4], 10);
        let mut arrow = land::test_setup_geometry();
        arrow.spheres = vec![dereth_physics::geom::Sphere::new(
            Vec3::new(0.0, 0.0, 0.05),
            0.05,
        )];
        arrow.radius = 0.05;
        arrow.height = 0.1;
        phys_ext::register_setup(w, ARROW_SETUP, arrow);
    });
    ts.advance(0.1);
    ts
}

/// The player enters as `DoPlayerEnterWorld` has it (a world-connected session whose player is set
/// before it joins its landblock), wielding `weapons` (and their ammo), with `options` set.
fn enter(
    ts: &mut TestServer,
    options: CharacterOptions1,
    weapons: &[(u32, EquipMask)],
) -> (ClientId, SessionId) {
    let before: Vec<SessionId> = ts.world.sessions.iter().map(|(id, _)| id).collect();
    let id = ts.connect("acct", "pw");
    ts.advance(0.1);
    let session = ts
        .world
        .sessions
        .iter()
        .map(|(id, _)| id)
        .find(|s| !before.contains(s))
        .expect("the new session");
    let w = &mut ts.world;
    lm::get_landblock(w, LandblockId::new(LB | 0xFFFF), false, false);
    creature(
        w,
        Class::Player,
        PLAYER,
        1,
        "Tester",
        200,
        &[(Skill::HeavyWeapons, 400), (Skill::MissileWeapons, 400)],
        100.0,
        100.0,
    );
    let o = w.objects.get_mut(PLAYER).unwrap();
    o.set_property(PropertyInt::Level, 1);
    o.set_property(empyrean_entity::enums::PropertyInt64::TotalExperience, 0);
    o.set_property(
        empyrean_entity::enums::PropertyInt64::AvailableExperience,
        0,
    );
    let character = empyrean_store::models::shard::Character {
        character_options_1: i32::try_from(options.0).unwrap(),
        ..Default::default()
    };
    w.objects
        .get_mut(PLAYER)
        .unwrap()
        .player
        .as_mut()
        .expect("a player")
        .player
        .character = Some(character);
    for &(wcid, mask) in weapons {
        let item = new_object(w, wcid);
        assert!(
            creature_equipment::try_equip_object(w, PLAYER, item, mask),
            "wields {wcid}"
        );
    }
    let s = w.sessions.get_mut(session).expect("the game half");
    s.state = SessionState::WorldConnected;
    s.set_player(Some(PLAYER));
    assert!(
        lm::add_object(w, PLAYER, false),
        "the player joins its landblock"
    );
    phys_ext::set_physics_state(w, PLAYER, PhysicsState::Hidden, Some(false));
    (id, session)
}

/// A drudge `dist` metres north of the player (the player faces north), which never attacks.
fn drudge(ts: &mut TestServer, dist: f32, health: u32) {
    drudge_at(ts, MONSTER, 100.0, 100.0 + dist, health);
}

/// A drudge `guid` at (`x`, `y`), which never attacks.
fn drudge_at(ts: &mut TestServer, guid: ObjectGuid, x: f32, y: f32, health: u32) {
    let w = &mut ts.world;
    creature(
        w,
        Class::Creature,
        guid,
        DRUDGE,
        "Drudge",
        health,
        &[],
        x,
        y,
    );
    let o = w.objects.get_mut(guid).unwrap();
    o.biota.properties_body_part = Some(Arc::new(
        [(CombatBodyPart::Chest, drudge_part())]
            .into_iter()
            .collect(),
    ));
    o.set_property(PropertyInt::Tolerance, Tolerance::NoAttack.0);
    assert!(
        lm::add_object(w, guid, false),
        "the drudge joins its landblock"
    );
}

/// A received message as the client's dispatcher sees it: its kind and the whole blob.
#[derive(Debug, Clone)]
struct Got {
    kind: u32,
    blob: Vec<u8>,
}

impl Got {
    /// Decodes the message (a game event through its 0xF7B0 wrapper).
    fn decode<M: Message>(&self) -> M {
        let split = split_ui_blob(&self.blob).expect("a blob");
        let mut body = split.body;
        let m = M::read(&mut body).unwrap_or_else(|e| panic!("0x{:04X} decodes: {e:?}", self.kind));
        if split.order.is_none() {
            assert_eq!(split.sub_type.0, M::OPCODE.0);
        }
        m
    }
}

/// Every message the client received from index `from` on, with the virtual time it was read.
fn got(ts: &TestServer, id: ClientId, from: usize) -> Vec<Got> {
    ts.received_raw(id)[from..]
        .iter()
        .map(|m| {
            let mut blob = m.opcode.to_le_bytes().to_vec();
            blob.extend_from_slice(&m.body);
            let kind = if m.opcode == 0xF7B0 {
                u32::from_le_bytes(m.body[8..12].try_into().unwrap())
            } else {
                m.opcode
            };
            Got { kind, blob }
        })
        .collect()
}

fn kinds(g: &[Got]) -> Vec<u32> {
    g.iter().map(|g| g.kind).collect()
}

fn health(ts: &TestServer, g: ObjectGuid) -> Option<u32> {
    ts.world.objects.get(g).map(|o| o.health().current(o))
}

/// The objects in the test landblock (its processed list), with their weenie class ids.
fn landblock_objects(ts: &TestServer) -> Vec<(ObjectGuid, u32)> {
    let l = ts
        .world
        .landblock_manager
        .landblocks
        .get(LandblockId::new(LB | 0xFFFF))
        .expect("the landblock");
    l.get_all_world_objects_for_diagnostics()
        .into_iter()
        .filter_map(|g| {
            ts.world
                .objects
                .get(g)
                .map(|o| (g, o.biota.weenie_class_id))
        })
        .collect()
}

fn stamina(ts: &TestServer, g: ObjectGuid) -> u32 {
    let o = ts.world.objects.get(g).expect("live");
    o.stamina().current(o)
}

// ------------------------------------------------------------------ melee

/// Melee mode, then a targeted attack on a drudge in reach: the swing (a sticky SlashMed at the
/// Quickness/weapon speed), the stamina cost, the strike at the attack frame, the attacker
/// notification; with repeat attacks on, the next swing after the power bar refills, until the
/// drudge dies and leaves a corpse.
#[test]
fn a_player_attacks_a_drudge_with_a_sword_until_it_dies() {
    let mut ts = server();
    let (id, _session) = enter(
        &mut ts,
        CharacterOptions1::AutoRepeatAttack,
        &[(SWORD, EquipMask::MeleeWeapon)],
    );
    drudge(&mut ts, 1.4, 60);
    ts.advance(0.2);
    let _ = take_local();

    // Combat_ChangeCombatMode(Melee): the stance switch and the combat mode update
    let n = ts.received_raw(id).len();
    ts.send_game_action(
        id,
        &CombatChangeCombatMode {
            combat_mode: u32::try_from(CombatMode::Melee.0).unwrap(),
        },
    );
    ts.advance(1.0);
    let g = got(&ts, id, n);
    assert!(
        kinds(&g).contains(&PRIVATE_INT),
        "PrivateUpdatePropertyInt(CombatMode): {:04X?}",
        kinds(&g)
    );
    assert_eq!(
        creature_combat::combat_mode(&ts.world, PLAYER),
        CombatMode::Melee
    );
    assert_eq!(
        player_melee::current_stance(&ts.world, PLAYER),
        MotionStance::SwordCombat
    );

    // Combat_TargetedMeleeAttack(drudge, Medium, full power)
    let before = stamina(&ts, PLAYER);
    let n = ts.received_raw(id).len();
    ts.send_game_action(
        id,
        &CombatTargetedMeleeAttack {
            target: ObjectId(MONSTER.full()),
            attack_height: 2,
            power_level: 1.0,
        },
    );
    ts.advance(0.05);
    let g = got(&ts, id, n);
    assert!(
        kinds(&g).contains(&MOVEMENT),
        "the swing motion: {:04X?}",
        kinds(&g)
    );
    assert!(
        player_combat::fields(&ts.world, PLAYER).attacking,
        "a swing is under way"
    );
    // GetAttackStamina(High): 1 point per 700 burden units at the high bar is 2 (the sword weighs
    // 700), less the Endurance discount (1 - 50/480 at Endurance 100) = 1.79 -> 2
    assert_eq!(before - stamina(&ts, PLAYER), 2, "the stamina cost");

    let dead = ts.run_until(20.0, |ts| health(ts, MONSTER).is_none_or(|h| h == 0));
    let g = got(&ts, id, n);
    assert!(dead, "the drudge dies: {:04X?}", kinds(&g));
    // a strike that leaves it alive is reported (hit or evaded); the killing blow is the
    // KillerNotification from its OnDeath instead
    for h in g
        .iter()
        .filter(|m| m.kind == ATTACKER_NOTE)
        .map(Got::decode::<AttackerNotification>)
    {
        assert_eq!(h.defender_name, "Drudge");
    }
    assert_eq!(
        g.iter().filter(|m| m.kind == KILLER_NOTE).count(),
        1,
        "one kill: {:04X?}",
        kinds(&g)
    );

    // the corpse appears and the drudge leaves the world
    assert!(
        ts.run_until(30.0, |ts| ts.world.objects.get(MONSTER).is_none()),
        "the drudge is destroyed after its death"
    );
    let corpse = landblock_objects(&ts)
        .into_iter()
        .find(|&(_, wcid)| wcid == CORPSE);
    assert!(corpse.is_some(), "a corpse was created");
}

/// The repeat-attack cadence and the attack queue (`Player.Attack`'s last action, `AttackQueue`):
/// a swing lasts the slash's length at the attack speed; then the power bar refills for
/// `PowerLevel` seconds, the level fetched from the queue. A second click during the first swing
/// (power 0.5) queues behind the first (1.0): Fetch drops the 1.0, so every later swing waits 0.5 s
/// and costs the Medium stamina (1 instead of 2).
#[test]
fn repeat_attacks_follow_the_power_bar_and_the_attack_queue() {
    let mut ts = server();
    let (id, _session) = enter(
        &mut ts,
        CharacterOptions1::AutoRepeatAttack,
        &[(SWORD, EquipMask::MeleeWeapon)],
    );
    drudge(&mut ts, 1.4, 5000);
    ts.advance(0.2);
    ts.send_game_action(
        id,
        &CombatChangeCombatMode {
            combat_mode: u32::try_from(CombatMode::Melee.0).unwrap(),
        },
    );
    ts.advance(1.0);

    // GetAnimSpeed: 1 / (1 - Quickness 100 / 300 + WeaponTime 30 / 150), clamped to 0.5-2
    #[allow(clippy::cast_possible_truncation)] // ACE's `(float)`
    let anim_speed = (1.0 / (1.0 - 100.0 / 300.0 + 30.0 / 150.0)) as f32;
    // GetAnimationLength: the 30-frame slash at 30 fps, over the speed
    let anim_length = f64::from(1.0f32 / anim_speed);

    ts.send_game_action(
        id,
        &CombatTargetedMeleeAttack {
            target: ObjectId(MONSTER.full()),
            attack_height: 2,
            power_level: 1.0,
        },
    );
    let mut swings: Vec<(f64, u32)> = Vec::new(); // (start, stamina spent)
    let mut was_attacking = false;
    let mut last_stamina = stamina(&ts, PLAYER);
    let mut queued = false;
    ts.run_until(8.0, |ts| {
        let attacking = player_combat::fields(&ts.world, PLAYER).attacking;
        let now_stamina = stamina(ts, PLAYER);
        if attacking && !was_attacking {
            swings.push((ts.seconds(), last_stamina - now_stamina));
        }
        if attacking && !queued {
            // the second click, while the first swing is under way
            ts.send_game_action(
                id,
                &CombatTargetedMeleeAttack {
                    target: ObjectId(MONSTER.full()),
                    attack_height: 2,
                    power_level: 0.5,
                },
            );
            queued = true;
        }
        was_attacking = attacking;
        last_stamina = now_stamina;
        swings.len() >= 3
    });
    assert!(swings.len() >= 3, "three swings: {swings:?}");
    assert_eq!(
        swings.iter().map(|s| s.1).collect::<Vec<_>>()[..3],
        [2, 1, 1],
        "High, then Medium stamina costs"
    );
    // two delays in a row (the slash's end, then the refill), each run on the first 60 Hz tick at
    // or after its end time: up to two ticks late
    let want = anim_length + 0.5;
    for k in 1..3 {
        let gap = swings[k].0 - swings[k - 1].0;
        assert!(
            gap >= want - 1e-9 && gap <= want + 2.0 / 60.0 + 1e-9,
            "swing {k} starts the slash length + 0.5 s later: {gap} vs {want}"
        );
    }
}

// ------------------------------------------------------------------ missile

/// Missile mode with a bow and ten arrows, then a targeted attack: an arrow is created, flies,
/// hits the drudge and leaves the world, and the stack shrinks by one.
#[test]
fn a_bow_attack_launches_an_arrow_that_reaches_the_target() {
    let mut ts = server();
    let (id, _session) = enter(
        &mut ts,
        CharacterOptions1(0),
        &[
            (BOW, EquipMask::MissileWeapon),
            (ARROW, EquipMask::MissileAmmo),
        ],
    );
    drudge(&mut ts, 8.0, 500);
    ts.advance(0.2);

    ts.send_game_action(
        id,
        &CombatChangeCombatMode {
            combat_mode: u32::try_from(CombatMode::Missile.0).unwrap(),
        },
    );
    ts.advance(2.0);
    assert_eq!(
        creature_combat::combat_mode(&ts.world, PLAYER),
        CombatMode::Missile
    );
    assert_eq!(
        player_melee::current_stance(&ts.world, PLAYER),
        MotionStance::BowCombat
    );
    let ammo = creature_equipment::get_equipped_ammo(&ts.world, PLAYER).expect("arrows");
    assert_eq!(ts.world.objects.get(ammo).unwrap().stack_size(), Some(10));

    let n = ts.received_raw(id).len();
    let stamina_before = stamina(&ts, PLAYER);
    let objects_before: Vec<ObjectGuid> =
        landblock_objects(&ts).into_iter().map(|(g, _)| g).collect();
    ts.send_game_action(
        id,
        &CombatTargetedMissileAttack {
            target: ObjectId(MONSTER.full()),
            attack_height: 2,
            power_level: 1.0,
        },
    );

    // the arrow is created
    let mut arrow = None;
    assert!(
        ts.run_until(3.0, |ts| {
            arrow = landblock_objects(ts)
                .into_iter()
                .find(|(g, wcid)| !objects_before.contains(g) && *wcid == ARROW)
                .map(|(g, _)| g);
            arrow.is_some()
        }),
        "an arrow was launched"
    );
    let arrow = arrow.unwrap();
    let links = ts
        .world
        .objects
        .get(arrow)
        .unwrap()
        .projectile
        .expect("projectile links");
    assert_eq!((links.source, links.target), (Some(PLAYER), Some(MONSTER)));
    assert_eq!(
        ts.world.objects.get(ammo).unwrap().stack_size(),
        Some(9),
        "one arrow used"
    );
    // GetAttackStamina(High) with the 400-burden bow: no 700-unit step, so the floor of 1
    assert_eq!(
        stamina_before - stamina(&ts, PLAYER),
        1,
        "the launch's stamina cost"
    );

    // it reaches the drudge and leaves the world
    assert!(
        ts.run_until(3.0, |ts| ts.world.objects.get(arrow).is_none()),
        "the arrow hit something and left the world"
    );
    // the reload, then (repeat attacks off) the end of the attack
    assert!(
        ts.run_until(5.0, |ts| !player_combat::fields(&ts.world, PLAYER)
            .attacking
            && player_melee::attack_target(&ts.world, PLAYER).is_none()),
        "the attack ends"
    );
    let g = got(&ts, id, n);
    let created: Vec<ItemCreateObject> = g
        .iter()
        .filter(|m| m.kind == CREATE_OBJECT)
        .map(Got::decode)
        .collect();
    assert!(
        created.iter().any(|c| c.0.id.0 == arrow.full()),
        "the client saw the arrow created: {:04X?}",
        kinds(&g)
    );
    assert!(kinds(&g).contains(&SOUND), "the launch sound");
    let struck = g
        .iter()
        .any(|m| m.kind == ATTACKER_NOTE || m.kind == EVADED_NOTE);
    assert!(
        struck,
        "the arrow struck the drudge (hit or evaded): {:04X?}",
        kinds(&g)
    );
    let done: Vec<CombatHandleAttackDoneEvent> = g
        .iter()
        .filter(|m| m.kind == ATTACK_DONE)
        .map(Got::decode)
        .collect();
    assert!(!done.is_empty(), "the attack ends with AttackDone");
}

/// No arrow is fired at a target that left the world during the windup.
/// V325.
#[test]
fn no_arrow_is_fired_at_a_target_that_left_the_world_during_the_windup() {
    let mut ts = server();
    let (id, _session) = enter(
        &mut ts,
        CharacterOptions1(0),
        &[
            (BOW, EquipMask::MissileWeapon),
            (ARROW, EquipMask::MissileAmmo),
        ],
    );
    drudge(&mut ts, 8.0, 500);
    ts.advance(0.2);
    ts.send_game_action(
        id,
        &CombatChangeCombatMode {
            combat_mode: u32::try_from(CombatMode::Missile.0).unwrap(),
        },
    );
    ts.advance(2.0);
    let ammo = creature_equipment::get_equipped_ammo(&ts.world, PLAYER).expect("arrows");
    let objects_before: Vec<ObjectGuid> =
        landblock_objects(&ts).into_iter().map(|(g, _)| g).collect();
    let n = ts.received_raw(id).len();
    ts.send_game_action(
        id,
        &CombatTargetedMissileAttack {
            target: ObjectId(MONSTER.full()),
            attack_height: 2,
            power_level: 1.0,
        },
    );

    // past the point of no return: the aim motion is playing
    assert!(
        ts.run_until(2.0, |ts| player_combat::fields(&ts.world, PLAYER).attacking),
        "the windup"
    );
    empyrean_world::entity::landblock::remove_world_object(
        &mut ts.world,
        LandblockId::new(LB | 0xFFFF),
        MONSTER,
        false,
        false,
        true,
    );
    assert!(
        ts.world.objects.get(MONSTER).is_some()
            && phys_ext::physics_obj(&ts.world, MONSTER).is_none(),
        "held, but no body"
    );

    assert!(
        ts.run_until(8.0, |ts| got(ts, id, n)
            .iter()
            .any(|m| m.kind == ATTACK_DONE)),
        "the attack ends"
    );
    ts.advance(1.0);
    let arrows = landblock_objects(&ts)
        .into_iter()
        .filter(|(g, wcid)| !objects_before.contains(g) && *wcid == ARROW)
        .count();
    assert_eq!(arrows, 0, "no arrow was launched");
    assert_eq!(
        ts.world.objects.get(ammo).unwrap().stack_size(),
        Some(10),
        "no arrow used"
    );
    assert!(!player_combat::fields(&ts.world, PLAYER).attacking);
}

/// Missile mode with a bow, then one targeted attack at `MONSTER`; runs until the attack is over.
/// Answers every message received from the attack on, and the system chat lines among them.
fn one_bow_shot(ts: &mut TestServer, id: ClientId) -> (Vec<Got>, Vec<String>) {
    ts.send_game_action(
        id,
        &CombatChangeCombatMode {
            combat_mode: u32::try_from(CombatMode::Missile.0).unwrap(),
        },
    );
    ts.advance(2.0);
    let n = ts.received_raw(id).len();
    ts.send_game_action(
        id,
        &CombatTargetedMissileAttack {
            target: ObjectId(MONSTER.full()),
            attack_height: 2,
            power_level: 1.0,
        },
    );
    assert!(
        ts.run_until(8.0, |ts| got(ts, id, n)
            .iter()
            .any(|m| m.kind == ATTACK_DONE)),
        "the attack ends"
    );
    ts.advance(1.0);
    let g = got(ts, id, n);
    let chat =
        empyrean_testkit::decode::all_of::<dereth_protocol::comms::CommunicationTextboxString>(
            &ts.received_raw(id)[n..],
        )
        .into_iter()
        .map(|t| t.text)
        .collect();
    (g, chat)
}

/// An arrow passes through a creature that is not its target.
/// V137.
#[test]
fn an_arrow_passes_through_a_creature_that_is_not_its_target() {
    const BLOCKER: ObjectGuid = ObjectGuid::new(0x8000_0101);
    let mut ts = server();
    let (id, _session) = enter(
        &mut ts,
        CharacterOptions1(0),
        &[
            (BOW, EquipMask::MissileWeapon),
            (ARROW, EquipMask::MissileAmmo),
        ],
    );
    drudge(&mut ts, 8.0, 500);
    drudge_at(&mut ts, BLOCKER, 100.0, 104.0, 500);
    ts.advance(0.2);

    let (g, chat) = one_bow_shot(&mut ts, id);
    assert!(
        !chat
            .iter()
            .any(|t| t == "Your missile attack hit the environment."),
        "{chat:?}"
    );
    assert!(
        g.iter()
            .any(|m| m.kind == ATTACKER_NOTE || m.kind == EVADED_NOTE),
        "the arrow struck its target: {:04X?}",
        kinds(&g)
    );
    assert_eq!(
        health(&ts, BLOCKER),
        Some(500),
        "the drudge in between is untouched"
    );
}

/// A player who walked into another cell can shoot.
#[test]
fn a_player_who_walked_into_another_cell_can_shoot() {
    use dereth_protocol::movement::{
        AutonomousPosition, MoveTimestamps, MovementAutonomousPosition,
    };
    use dereth_protocol::types::space::{Frame, PositionWire, Quat, Vec3 as WVec3};
    let mut ts = server();
    let (id, _session) = enter(
        &mut ts,
        CharacterOptions1(0),
        &[
            (BOW, EquipMask::MissileWeapon),
            (ARROW, EquipMask::MissileAmmo),
        ],
    );
    // north from (100, 100) in cell 0x25 (y 96..120) to (100, 124) in cell 0x26
    for step in 1..=48u16 {
        let at = ts
            .world
            .objects
            .get(PLAYER)
            .and_then(WorldObject::location)
            .unwrap();
        let y = 100.0 + f32::from(step) * 0.5;
        let cell = LB | (4 * 8 + u32::from(y >= 120.0) + 4 + 1);
        ts.send_game_action(
            id,
            &MovementAutonomousPosition(AutonomousPosition {
                position: PositionWire {
                    objcell_id: cell,
                    frame: Frame {
                        origin: WVec3 {
                            x: at.position_x,
                            y,
                            z: at.position_z,
                        },
                        orientation: Quat {
                            w: at.rotation_w,
                            x: at.rotation_x,
                            y: at.rotation_y,
                            z: at.rotation_z,
                        },
                    },
                },
                timestamps: MoveTimestamps::default(),
                contact: 1,
            }),
        );
        ts.advance(0.1);
    }
    let at = ts
        .world
        .objects
        .get(PLAYER)
        .and_then(WorldObject::location)
        .unwrap();
    assert_eq!(
        (at.cell(), at.position_y),
        (LB | 0x26, 124.0),
        "walked into the next cell"
    );
    drudge_at(&mut ts, MONSTER, 100.0, 132.0, 500);
    ts.advance(0.2);

    let (g, chat) = one_bow_shot(&mut ts, id);
    assert!(
        !chat
            .iter()
            .any(|t| t == "Your missile attack hit the environment."),
        "{chat:?}"
    );
    assert!(
        g.iter()
            .any(|m| m.kind == ATTACKER_NOTE || m.kind == EVADED_NOTE),
        "the arrow struck its target: {:04X?}",
        kinds(&g)
    );
}

/// The spawn origin uses the physics radius.
#[test]
fn the_spawn_origin_uses_the_physics_radius() {
    use empyrean_world::world_objects::creature_missile::get_projectile_spawn_origin;
    let mut ts = server();
    let _ = enter(
        &mut ts,
        CharacterOptions1(0),
        &[
            (BOW, EquipMask::MissileWeapon),
            (ARROW, EquipMask::MissileAmmo),
        ],
    );
    let w = &mut ts.world;
    let h = phys_ext::physics_obj(w, PLAYER).expect("a body");
    let body = w.physics.get_mut(h).expect("a body");
    let mut g = (*body.geometry).clone();
    g.radius = 0.9; // the bounding radius, which ACE does not use here
    g.spheres = vec![dereth_physics::geom::Sphere::new(
        Vec3::new(0.0, 0.0, 0.5),
        0.4,
    )];
    body.geometry = Arc::new(g.clone());
    body.scale = 1.5;
    let origin = |w: &World| get_projectile_spawn_origin(w, PLAYER, ARROW, Mc::AimLevel);
    // spheres: 2 * 0.4 * 1.5 + 2 * 0.05 + 0.0002 ahead; height 1.0 * 1.5
    let o = origin(w);
    assert!(
        (o.y - (2.0 * 0.4 * 1.5 + 2.0 * 0.05 + 0.0002)).abs() < 1e-6 && o.x.abs() < 1e-6,
        "{o:?}"
    );
    assert!((o.z - 1.5 * 0.8454).abs() < 1e-6, "{o:?}");
    // a cylinder sphere wins over the spheres
    g.cyl_spheres = vec![dereth_physics::CylSphere {
        low_pt: Vec3::new(0.0, 0.0, 0.0),
        height: 1.0,
        radius: 0.3,
    }];
    w.physics.get_mut(h).unwrap().geometry = Arc::new(g);
    let o = origin(w);
    assert!(
        (o.y - (2.0 * 0.3 * 1.5 + 2.0 * 0.05 + 0.0002)).abs() < 1e-6,
        "{o:?}"
    );
    // a physics BSP: no radius at all
    let state = phys_ext::state(w, h);
    phys_ext::set_state(w, h, PhysicsState(state.0 | PhysicsState::HasPhysicsBSP.0));
    let o = origin(w);
    assert!((o.y - (2.0 * 0.05 + 0.0002)).abs() < 1e-6, "{o:?}");
}

/// `HandleActionChangeCombatMode_Inner`: asking for missile mode with no missile weapon (the
/// client has already switched its own bar) reverts to peace, so the client resyncs.
#[test]
fn missile_mode_without_a_missile_weapon_reverts_to_peace() {
    let mut ts = server();
    let (id, _session) = enter(
        &mut ts,
        CharacterOptions1(0),
        &[(SWORD, EquipMask::MeleeWeapon)],
    );
    ts.advance(0.2);
    let n = ts.received_raw(id).len();
    ts.send_game_action(
        id,
        &CombatChangeCombatMode {
            combat_mode: u32::try_from(CombatMode::Missile.0).unwrap(),
        },
    );
    ts.advance(0.5);
    assert_eq!(
        creature_combat::combat_mode(&ts.world, PLAYER),
        CombatMode::NonCombat
    );
    let g = got(&ts, id, n);
    let update: dereth_protocol::qualities::QualitiesPrivateUpdateInt = g
        .iter()
        .find(|m| m.kind == PRIVATE_INT)
        .expect("the combat mode update")
        .decode();
    assert_eq!(
        (update.0.property_id, update.0.value),
        (
            u32::from(PropertyInt::CombatMode.0),
            CombatMode::NonCombat.0
        ),
        "CombatMode back to NonCombat"
    );
}

/// `HandleActionTargetedMeleeAttack_Inner` out of reach with Use Charge Attack: the player charges
/// (`Player.MoveTo`), and the move's completion (`Player.OnMoveComplete`, status None) swings at
/// the melee target (`Player.Attack`).
#[test]
fn a_charge_attack_swings_when_the_charge_arrives() {
    let mut ts = server();
    let (id, _session) = enter(
        &mut ts,
        CharacterOptions1::UseChargeAttack,
        &[(SWORD, EquipMask::MeleeWeapon)],
    );
    drudge(&mut ts, 8.0, 5000);
    ts.advance(0.2);
    ts.send_game_action(
        id,
        &CombatChangeCombatMode {
            combat_mode: u32::try_from(CombatMode::Melee.0).unwrap(),
        },
    );
    ts.advance(1.0);

    ts.send_game_action(
        id,
        &CombatTargetedMeleeAttack {
            target: ObjectId(MONSTER.full()),
            attack_height: 2,
            power_level: 1.0,
        },
    );
    ts.advance(0.1);
    assert!(
        !player_combat::fields(&ts.world, PLAYER).attacking,
        "out of reach: no swing yet"
    );
    assert!(
        ts.run_until(8.0, |ts| player_combat::fields(&ts.world, PLAYER).attacking),
        "the charge arrives and the swing starts"
    );
}

/// `HandleActionTargetedMeleeAttack_Inner`: a target within sticky distance (4 m) and in sight
/// (`IsMeleeVisible`, a line-of-sight transition) is swung at at once, with no move-to.
#[test]
fn a_target_in_sticky_range_and_sight_is_attacked_at_once() {
    let mut ts = server();
    let (id, _session) = enter(
        &mut ts,
        CharacterOptions1(0),
        &[(SWORD, EquipMask::MeleeWeapon)],
    );
    drudge(&mut ts, 3.0, 5000);
    ts.advance(0.2);
    ts.send_game_action(
        id,
        &CombatChangeCombatMode {
            combat_mode: u32::try_from(CombatMode::Melee.0).unwrap(),
        },
    );
    ts.advance(1.0);
    ts.send_game_action(
        id,
        &CombatTargetedMeleeAttack {
            target: ObjectId(MONSTER.full()),
            attack_height: 2,
            power_level: 1.0,
        },
    );
    ts.advance(0.05);
    assert!(
        player_combat::fields(&ts.world, PLAYER).attacking,
        "the swing starts at once"
    );
}

/// `Creature.GetCleaveTarget`: a cleaving weapon also strikes a creature beside the target, unless
/// the player could not attack it (`CheckPKStatusVsTarget`: a PK creature against an NPK player).
#[test]
fn a_cleave_skips_a_creature_the_player_may_not_attack() {
    const OTHER: ObjectGuid = ObjectGuid::new(0x8000_0200);
    for pk in [false, true] {
        let mut ts = server();
        let (id, _session) = enter(
            &mut ts,
            CharacterOptions1(0),
            &[(CLEAVER, EquipMask::MeleeWeapon)],
        );
        drudge(&mut ts, 1.4, 5000);
        drudge_at(&mut ts, OTHER, 100.8, 101.4, 5000);
        if pk {
            let status = empyrean_entity::enums::PlayerKillerStatus::PK
                .0
                .cast_signed();
            ts.world
                .objects
                .get_mut(OTHER)
                .unwrap()
                .set_property(PropertyInt::PlayerKillerStatus, status);
        }
        ts.advance(0.2);
        ts.send_game_action(
            id,
            &CombatChangeCombatMode {
                combat_mode: u32::try_from(CombatMode::Melee.0).unwrap(),
            },
        );
        ts.advance(1.0);
        let n = ts.received_raw(id).len();
        ts.send_game_action(
            id,
            &CombatTargetedMeleeAttack {
                target: ObjectId(MONSTER.full()),
                attack_height: 2,
                power_level: 1.0,
            },
        );
        // one swing (repeat attacks off); its strike at the attack frame
        ts.advance(3.0);
        let target_hurt = health(&ts, MONSTER).is_some_and(|h| h < 5000);
        let other_hurt = health(&ts, OTHER).is_some_and(|h| h < 5000);
        assert!(target_hurt, "the target is struck (pk {pk})");
        assert_eq!(
            other_hurt, !pk,
            "the cleave reaches the other drudge only when it may be attacked"
        );
        // skipped by GetCleaveTarget, not refused by DamageTarget's own PK check (which would
        // send the player a WeenieErrorWithString)
        let refusals = got(&ts, id, n)
            .iter()
            .filter(|m| m.kind == WEENIE_ERROR_WITH_STRING)
            .count();
        assert_eq!(refusals, 0, "no PK refusal message (pk {pk})");
    }
}

#[cfg(feature = "real-content")]
mod recorded_content_real {
    //! ACE: Source/ACE.Server/Managers/WorldManager.cs::DoPlayerEnterWorld
    use crate::support::real_content_bot::real::*;

    /// Successful melee attacks train the weapon skill (`Proficiency.OnSuccessUse`).
    #[test]
    #[ignore = "Weapon skills do not yet gain experience from successful hits"]
    fn melee_hits_train_the_weapon_skill() {
        let mut l = create_and_enter();
        pick_up_and_wield(&mut l);
        // (ResistanceAtLastCheck, pp): ACE records the difficulty and raises the skill by the pp it grants
        let heavy = |l: &Loop| {
            l.ts.world
                .objects
                .get(l.g)
                .and_then(|o| o.biota.properties_skill.as_ref())
                .and_then(|s| s.get(&empyrean_entity::enums::Skill::HeavyWeapons))
                .map(|k| (k.resistance_at_last_check, k.pp))
                .expect("Heavy Weapons")
        };
        let before = heavy(&l);
        let drudge = create_a_drudge(&mut l);
        fight(&mut l, drudge);
        let after = heavy(&l);
        assert!(
            after.0 > 0 && after.1 > before.1,
            "Heavy Weapons use recorded and trained: {before:?} -> {after:?}"
        );
    }

    /// The selected drudge's health bar follows its damage: after the QueryHealth answer, each hit
    /// sends an UpdateHealth (`Creature.OnHealthUpdate` to `selectedTargets`).
    #[test]
    fn the_selected_monsters_health_bar_follows_damage() {
        use dereth_protocol::combat::{CombatQueryHealth, CombatQueryHealthResponse};
        let mut l = create_and_enter();
        pick_up_and_wield(&mut l);
        let drudge = create_a_drudge(&mut l);
        let mark = l.mark();
        l.action(&CombatQueryHealth {
            target: ObjectId(drudge.full()),
        });
        l.advance(0.5);
        let answers = l.since::<CombatQueryHealthResponse>(mark);
        assert_eq!(answers.len(), 1, "the QueryHealth answer");
        fight(&mut l, drudge);
        let updates = l.since::<CombatQueryHealthResponse>(mark);
        assert!(
            updates.len() > 1 && updates.iter().any(|u| u.health < 1.0),
            "health bar updates: {:?}",
            updates.iter().map(|u| u.health).collect::<Vec<_>>()
        );
    }
}
