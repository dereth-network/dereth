//! Shared virtual-time server fixture and message helpers.

#![allow(unused_imports)]

pub(crate) use std::collections::BTreeMap;
pub(crate) use std::sync::Arc;

pub(crate) use dereth_assets::geometry::AnimFrame;
pub(crate) use dereth_assets::motion::{AnimData, MotionData};
pub(crate) use dereth_assets::tables::CombatManeuver;
pub(crate) use dereth_assets::{AnimHook, Animation, CombatManeuverTable, HookData, MotionTable};
pub(crate) use dereth_primitives::{DataId, Vec3};
pub(crate) use dereth_protocol::comms::{
    CommunicationTextboxString, CommunicationWeenieError, CommunicationWeenieErrorWithString,
};
pub(crate) use dereth_protocol::events::split_ui_blob;
pub(crate) use dereth_protocol::qualities::{
    QualitiesPrivateUpdateInt, QualitiesPrivateUpdateInt64, QualitiesPrivateUpdateSkill,
};
pub(crate) use dereth_protocol::Message;
pub(crate) use empyrean_common::dotnet::DotNetDict;
pub(crate) use empyrean_common::random::DotNetRandom;
pub(crate) use empyrean_common::thread_safe_random::ThreadSafeRandom;
pub(crate) use empyrean_content::models::world::weenie_properties_attribute::WeeniePropertiesAttribute;
pub(crate) use empyrean_content::models::world::weenie_properties_attribute_2nd::WeeniePropertiesAttribute2nd;
pub(crate) use empyrean_content::models::world::weenie_properties_body_part::WeeniePropertiesBodyPart;
pub(crate) use empyrean_content::models::world::Weenie;
pub(crate) use empyrean_content::MemContent;
pub(crate) use empyrean_dat::FakeDats;
pub(crate) use empyrean_entity::enums::{
    AttackType, CombatBodyPart, ConfirmationType, DamageType, EquipMask, HeritageGroup,
    MotionCommand as Mc, MotionStance, PhysicsState, PropertyAttribute, PropertyAttribute2nd,
    PropertyBool, PropertyDataId, PropertyFloat, PropertyInt, PropertyInt64, PropertyString, Skill,
    SkillAdvancementClass, WeenieErrorWithString, WeenieType,
};
pub(crate) use empyrean_entity::models::PropertiesBodyPart;
pub(crate) use empyrean_entity::{LandblockId, ObjectGuid};
pub(crate) use empyrean_net::SessionState;
pub(crate) use empyrean_testkit::{dats, land, ClientId, TestServer};
pub(crate) use empyrean_world::dispatch::{self, Class};
pub(crate) use empyrean_world::entity::enlightenment;
pub(crate) use empyrean_world::managers::guid_manager as gm;
pub(crate) use empyrean_world::managers::landblock_manager as lm;
pub(crate) use empyrean_world::physics::phys_ext;
pub(crate) use empyrean_world::world_objects::managers::confirmation_manager;
pub(crate) use empyrean_world::world_objects::world_object::{CtorEnv, WorldObject};
pub(crate) use empyrean_world::world_objects::{container, creature_equipment, player_combat};
pub(crate) use empyrean_world::World;

pub(crate) use crate::combat::monster_ai::{creature, EmptyShard, CMT, LB, MT};

pub(crate) const PLAYER: ObjectGuid = ObjectGuid::new(0x5000_0001);
pub(crate) const MONSTER: ObjectGuid = ObjectGuid::new(0x8000_0100);

pub(crate) const PET_WCID: u32 = 60_001;
pub(crate) const ESSENCE_WCID: u32 = 60_002;
pub(crate) const GEM_WCID: u32 = 60_003;
pub(crate) const HOTSPOT_WCID: u32 = 60_004;
pub(crate) const CLOAK_WCID: u32 = 60_005;
pub(crate) const NPC_WCID: u32 = 60_006;
pub(crate) const CERTIFICATE_WCID: u32 = 46_421;
pub(crate) const DRUDGE_WCID: u32 = 60_007;

pub(crate) const CYCLE: u32 = 0x0300_0210;
pub(crate) const STANCE: u32 = 0x0300_0211;
pub(crate) const ATTACK: u32 = 0x0300_0212;
pub(crate) const NC: u32 = MotionStance::NonCombat.0;
pub(crate) const HC: u32 = MotionStance::HandCombat.0;

// message kinds: the game-event type inside a 0xF7B0, else the opcode
pub(crate) const WEENIE_ERROR: u32 = 0x028A;
pub(crate) const WEENIE_ERROR_STR: u32 = 0x028B;
pub(crate) const PRIVATE_INT: u32 = 0x02CD;
pub(crate) const PRIVATE_INT64: u32 = 0x02CF;
pub(crate) const PRIVATE_SKILL: u32 = 0x02DD;
pub(crate) const CHAT: u32 = 0xF7E0;

// ---------------------------------------------------------------------------------- the world

pub(crate) fn anim(id: u32) -> AnimData {
    AnimData {
        anim_id: DataId(id),
        low_frame: 0,
        high_frame: -1,
        framerate: 30.0,
    }
}

pub(crate) fn data(
    key: u32,
    anims: Vec<AnimData>,
    velocity: Option<Vec3>,
    omega: Option<Vec3>,
) -> MotionData {
    MotionData {
        key,
        bitfield: 0,
        flags: 0,
        anims,
        velocity,
        omega,
    }
}

pub(crate) fn key(style: u32, motion: u32) -> u32 {
    style.wrapping_shl(16) | (motion & 0xFF_FFFF)
}

pub(crate) fn animation(id: u32, n: u32, hook_frame: Option<u32>) -> Animation {
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

/// The synthetic body's setup as a dat file (`Pet.GetPetRadius` reads its first sphere).
pub(crate) fn setup_model() -> empyrean_dat::file_types::SetupModel {
    let sphere = |r: f32| dereth_assets::Sphere {
        center: Vec3::new(0.0, 0.0, 0.5),
        radius: r,
    };
    empyrean_dat::file_types::SetupModel {
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
    }
}

/// monster_ai.rs's world (a motion table with NonCombat and HandCombat, a run and a turn cycle, a
/// stance link, one attack; a combat table with that attack at every height), plus the setup
/// file, the character generator (Aluvian: 50 skill credits, Melee Defense specializes for 20)
/// and the Melee Defense skill table.
pub(crate) fn leaf_dats() -> Arc<empyrean_dat::DatManager> {
    let mut cycles = Vec::new();
    let mut modifiers = Vec::new();
    let mut style_defaults = BTreeMap::new();
    for style in [NC, HC] {
        style_defaults.insert(style, Mc::Ready.0);
        cycles.push(data(key(style, Mc::Ready.0), vec![anim(CYCLE)], None, None));
        cycles.push(data(
            key(style, Mc::RunForward.0),
            vec![anim(CYCLE)],
            Some(Vec3::new(0.0, 4.0, 0.0)),
            None,
        ));
        let omega = Some(Vec3::new(0.0, 0.0, -std::f32::consts::FRAC_PI_2));
        cycles.push(MotionData {
            bitfield: 2,
            ..data(key(style, Mc::TurnRight.0), vec![anim(CYCLE)], None, omega)
        });
        modifiers.push(data(key(style, Mc::TurnRight.0), Vec::new(), None, omega));
    }
    let mut links = BTreeMap::new();
    links.insert(
        key(NC, Mc::Ready.0),
        vec![data(HC, vec![anim(STANCE)], None, None)],
    );
    links.insert(
        key(HC, Mc::Ready.0),
        vec![
            data(NC, vec![anim(STANCE)], None, None),
            data(Mc::AttackHigh1.0, vec![anim(ATTACK)], None, None),
        ],
    );
    let table = MotionTable {
        id: DataId(MT),
        default_style: NC,
        style_defaults,
        cycles,
        modifiers,
        links,
    };
    let maneuver = |height: u32, t: AttackType| CombatManeuver {
        style: HC,
        attack_height: height,
        attack_type: t.0.cast_unsigned(),
        min_skill_level: 0,
        motion: Mc::AttackHigh1.0,
    };
    let cmt = CombatManeuverTable {
        id: DataId(CMT),
        maneuvers: vec![
            maneuver(1, AttackType::Punch),
            maneuver(2, AttackType::Punch),
            maneuver(3, AttackType::Kick),
        ],
    };
    dats::with_stat_tables(FakeDats::new())
        .with_skill_table(empyrean_dat::fake::sample::skill_table())
        .with_char_gen(empyrean_dat::fake::sample::char_gen())
        .with_xp_table(empyrean_dat::fake::sample::xp_table())
        .with_portal(MT, table)
        .with_portal(CMT, cmt)
        .with_portal(CYCLE, animation(CYCLE, 10, None))
        .with_portal(STANCE, animation(STANCE, 15, None))
        .with_portal(ATTACK, animation(ATTACK, 30, Some(10)))
        .with_portal(land::TEST_SETUP, setup_model())
        .build()
        .expect("fake dats")
}

pub(crate) fn weenie(wcid: u32, name: &str, weenie_type: WeenieType) -> Weenie {
    Weenie::new(wcid, name, weenie_type)
        .with_string(PropertyString::Name, name)
        .with_did(PropertyDataId::Setup, land::TEST_SETUP)
}

/// A combat pet: 60 in every attribute, 100 in every vital, one bludgeon body part, the arena's
/// motion and combat tables.
pub(crate) fn pet_weenie() -> Weenie {
    let mut d = weenie(PET_WCID, "Wolf", WeenieType::CombatPet)
        .with_bool(PropertyBool::Attackable, true)
        .with_did(PropertyDataId::MotionTable, MT)
        .with_did(PropertyDataId::CombatTable, CMT)
        .with_float(PropertyFloat::VisualAwarenessRange, 30.0);
    d.weenie_properties_attribute = [
        PropertyAttribute::Strength,
        PropertyAttribute::Endurance,
        PropertyAttribute::Coordination,
        PropertyAttribute::Quickness,
        PropertyAttribute::Focus,
        PropertyAttribute::Self_,
    ]
    .into_iter()
    .map(|a| WeeniePropertiesAttribute {
        object_id: PET_WCID,
        r#type: a.0,
        init_level: 60,
        ..Default::default()
    })
    .collect();
    d.weenie_properties_attribute_2nd = [
        PropertyAttribute2nd::MaxHealth,
        PropertyAttribute2nd::MaxStamina,
        PropertyAttribute2nd::MaxMana,
    ]
    .into_iter()
    .map(|v| WeeniePropertiesAttribute2nd {
        object_id: PET_WCID,
        r#type: v.0,
        init_level: 100,
        current_level: 100,
        ..Default::default()
    })
    .collect();
    // Run (as the drudge-like monster has it) and a strong unarmed attack
    d.weenie_properties_skill = [(Skill::Run, 100), (Skill::UnarmedCombat, 400)]
        .into_iter()
        .map(|(skill, level)| {
            empyrean_content::models::world::weenie_properties_skill::WeeniePropertiesSkill {
                object_id: PET_WCID,
                r#type: u16::try_from(skill.0).unwrap(),
                sac: SkillAdvancementClass::Trained.0,
                init_level: level,
                ..Default::default()
            }
        })
        .collect();
    d.weenie_properties_body_part = vec![WeeniePropertiesBodyPart {
        object_id: PET_WCID,
        key: u16::try_from(CombatBodyPart::Head.0).unwrap(),
        d_type: DamageType::Bludgeon.0,
        d_val: 12,
        d_var: 0.5,
        ..Default::default()
    }];
    d
}

/// The drudge's weenie: its body part table (`DamageEvent.GetBodyPart` rolls a creature
/// defender's part from its weenie; every quadrant hits the head).
pub(crate) fn drudge_weenie() -> Weenie {
    let mut d = weenie(DRUDGE_WCID, "Drudge", WeenieType::Creature);
    d.weenie_properties_body_part = vec![WeeniePropertiesBodyPart {
        object_id: DRUDGE_WCID,
        key: u16::try_from(CombatBodyPart::Head.0).unwrap(),
        d_type: DamageType::Bludgeon.0,
        d_val: 10,
        d_var: 0.5,
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
        ..Default::default()
    }];
    d
}

pub(crate) fn content() -> MemContent {
    MemContent::new()
        .weenie(drudge_weenie())
        .weenie(pet_weenie())
        .weenie(
            weenie(ESSENCE_WCID, "Wolf Essence", WeenieType::PetDevice)
                .with_int(PropertyInt::PetClass, PET_WCID.cast_signed())
                .with_int(PropertyInt::Structure, 5)
                .with_int(PropertyInt::MaxStructure, 5),
        )
        .weenie(
            weenie(
                GEM_WCID,
                "Gem of Enlightenment",
                WeenieType::SkillAlterationDevice,
            )
            .with_int(PropertyInt::TypeOfAlteration, 1)
            .with_int(PropertyInt::SkillToBeAltered, Skill::MeleeDefense.0),
        )
        .weenie(
            weenie(HOTSPOT_WCID, "Fire Pit", WeenieType::HotSpot)
                .with_bool(PropertyBool::IsHot, true)
                // Ethereal | ReportCollisions, as the world database's hotspots have it
                .with_int(PropertyInt::PhysicsState, 0x4 | 0x8)
                .with_bool(PropertyBool::Ethereal, true)
                .with_bool(PropertyBool::ReportCollisions, true)
                .with_int(PropertyInt::DamageType, DamageType::Fire.0)
                .with_int(PropertyInt::Damage, 10)
                .with_float(PropertyFloat::DamageVariance, 0.0)
                .with_float(PropertyFloat::HotspotCycleTime, 2.0)
                .with_string(PropertyString::ActivationTalk, "You burn for %i damage!"),
        )
        .weenie(
            weenie(CLOAK_WCID, "Cloak", WeenieType::Clothing)
                .with_int(
                    PropertyInt::ValidLocations,
                    EquipMask::Cloak.0.cast_signed(),
                )
                .with_int(PropertyInt::CloakWeaveProc, 2)
                .with_int(PropertyInt::ItemMaxLevel, 5)
                .with_int(PropertyInt::ItemXpStyle, 1)
                .with_int64(PropertyInt64::ItemBaseXp, 1_000_000_000)
                .with_int64(PropertyInt64::ItemTotalXp, 5_000_000_000),
        )
        .weenie(weenie(
            NPC_WCID,
            "Enlightenment Master",
            WeenieType::Creature,
        ))
        .weenie(weenie(
            CERTIFICATE_WCID,
            "Attribute Reset Certificate",
            WeenieType::Generic,
        ))
}

/// A player (logged in, entered as `DoPlayerEnterWorld` has it) at (100, 112), and a drudge-like
/// monster at (100, 88) that has not noticed anyone yet.
pub(crate) fn server() -> (TestServer, ClientId) {
    server_with_monster_at(88.0)
}

/// [`server`] with the monster at (100, `monster_y`).
pub(crate) fn server_with_monster_at(monster_y: f32) -> (TestServer, ClientId) {
    let mut ts = TestServer::with_setup(leaf_dats(), |w| {
        w.content = Arc::new(content());
        gm::initialize(w, &mut EmptyShard);
        land::use_flat_land_with_test_setup(w, &[0xA9B4], 10);
    });
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
    creature(w, Class::Player, PLAYER, 100.0, 112.0);
    {
        let o = w.objects.get_mut(PLAYER).unwrap();
        o.player.as_mut().expect("a player").player.character =
            Some(empyrean_store::models::shard::Character::default());
        o.set_property(PropertyString::Name, "Alpha".to_owned());
        o.set_property(PropertyInt::ItemsCapacity, 102);
        o.set_property(PropertyInt::ContainersCapacity, 7);
        o.set_property(PropertyInt::HeritageGroup, HeritageGroup::Aluvian.0);
        o.set_encumbrance_val(Some(0));
        o.set_value(Some(0));
    }
    let s = w.sessions.get_mut(session).expect("the game half");
    s.state = SessionState::WorldConnected;
    s.set_player(Some(PLAYER));
    assert!(
        lm::add_object(w, PLAYER, false),
        "the player joins its landblock"
    );
    phys_ext::set_physics_state(w, PLAYER, PhysicsState::Hidden, Some(false));
    creature(w, Class::Creature, MONSTER, 100.0, monster_y);
    let o = w.objects.get_mut(MONSTER).unwrap();
    o.set_property(PropertyDataId::CombatTable, CMT);
    o.set_property(PropertyString::Name, "Drudge".to_owned());
    o.biota.weenie_class_id = DRUDGE_WCID;
    let mut parts = DotNetDict::new();
    parts.insert(
        CombatBodyPart::Head,
        PropertiesBodyPart {
            d_type: DamageType::Bludgeon,
            d_val: 10,
            d_var: 0.5,
            ..PropertiesBodyPart::default()
        },
    );
    o.biota.properties_body_part = Some(Arc::new(parts));
    assert!(
        lm::add_object(w, MONSTER, false),
        "the monster joins its landblock"
    );
    ts.advance(0.1);
    (ts, client)
}

pub(crate) fn new_object(w: &mut World, wcid: u32) -> ObjectGuid {
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

/// A new object of `wcid` in the player's main pack.
pub(crate) fn in_pack(ts: &mut TestServer, wcid: u32) -> ObjectGuid {
    let g = new_object(&mut ts.world, wcid);
    assert!(container::try_add_to_inventory(
        &mut ts.world,
        PLAYER,
        g,
        0,
        false,
        true
    ));
    g
}

pub(crate) fn obj(ts: &TestServer, g: ObjectGuid) -> &WorldObject {
    ts.world.objects.get(g).expect("live object")
}

pub(crate) fn health(ts: &TestServer, g: ObjectGuid) -> u32 {
    let o = obj(ts, g);
    o.health().current(o)
}

// ---------------------------------------------------------------------------------- messages

/// A received message: its kind (the game-event type inside a 0xF7B0, else the opcode) and blob.
#[derive(Debug, Clone)]
pub(crate) struct Got {
    pub(crate) kind: u32,
    pub(crate) blob: Vec<u8>,
}

impl Got {
    pub(crate) fn decode<M: Message>(&self) -> M {
        let split = split_ui_blob(&self.blob).expect("a blob");
        let mut body = split.body;
        M::read(&mut body).unwrap_or_else(|e| panic!("0x{:04X} decodes: {e:?}", self.kind))
    }
}

/// `PrivateUpdatePropertyInt(PropertyInt.Age)`: the player's own heartbeat, left out.
pub(crate) fn is_age_update(blob: &[u8]) -> bool {
    blob.len() >= 9
        && u32::from_le_bytes(blob[0..4].try_into().unwrap()) == PRIVATE_INT
        && u32::from_le_bytes(blob[5..9].try_into().unwrap()) == u32::from(PropertyInt::Age.0)
}

pub(crate) fn got(ts: &TestServer, id: ClientId, from: usize) -> Vec<Got> {
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
        .filter(|g| !is_age_update(&g.blob))
        .collect()
}

pub(crate) fn chats(g: &[Got]) -> Vec<String> {
    g.iter()
        .filter(|m| m.kind == CHAT)
        .map(|m| m.decode::<CommunicationTextboxString>().text)
        .collect()
}

// ---------------------------------------------------------------------------------- pets

// ---------------------------------------------------------------------------------- cloaks

// ---------------------------------------------------------------------------------- gems

// ---------------------------------------------------------------------------------- enlightenment

// ---------------------------------------------------------------------------------- hotspots
