//! Vectors: fixtures/vectors/chargen/
//! CharacterCreate/Ex, PlayerFactory(Ex) and StarterGearFactory build the ACE character (skills,
//! starter gear, positions) incl. retail deviations.
//! Fixture: ACE vectors and explicit expected values, synthetic dats, isolated world state, retail dats or world.pack in the real-content tier.

use empyrean_common::era::EraExt as _;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use dereth_assets::tables::{
    CharGenTemplate, EyeStrip, GearItem, HairStyle, HeritageGroup as CgHeritage, ObjDesc, SexCg,
    SkillBase, SkillFormula, StarterArea,
};
use dereth_primitives::{CellId, DataId, Frame, Position as DatPosition, Quat, Vec3};
use dereth_protocol::login::{
    CharGenResult, CharGenVerificationResponse, CharacterSendCharGenResult,
};
use dereth_protocol::{self as proto, Message};
use empyrean_common::clock::ClockSnapshot;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::vectors;
use empyrean_content::models::world::{Spell, Weenie as ContentWeenie};
use empyrean_content::MemContent;
use empyrean_dat::fake::sample;
use empyrean_dat::file_types::{PaletteSet, SecondaryAttributeTable, SkillTable, TabooTable};
use empyrean_dat::{file_id, DatManager, FakeDats};
use empyrean_entity::enums::{
    AccessLevel, CharacterOption, CharacterTitle, CoverageMask, EquipMask, HeritageGroup,
    PositionType, PropertyAttribute, PropertyAttribute2nd, PropertyBool, PropertyDataId,
    PropertyFloat, PropertyInstanceId, PropertyInt, PropertyString, Skill, SkillAdvancementClass,
    WeaponType, WeenieType,
};
use empyrean_entity::{BinaryReader, CharacterCreateInfo};
use empyrean_net::enums::CharacterGenerationVerificationResponse as Cgvr;
use empyrean_net::{ClientMessage, SessionId, SessionState};
use empyrean_store::adapter::BiotaConverter;
use empyrean_world::factories::player_factory::{self, CreateResult, CreatedPlayer};
use empyrean_world::factories::player_factory_ex;
use empyrean_world::factories::starter_gear_factory;
use empyrean_world::managers::guid_manager::{self, ShardGuidQueries};
use empyrean_world::managers::world_manager::{self as wm, WorldStatusState};
use empyrean_world::network::game_messages::game_message;
use empyrean_world::network::managers::inbound_message_manager::{
    handle_client_message, run_inbound_message_queue,
};
use empyrean_world::sessions::SessionData;
use empyrean_world::World;

const S: SessionId = SessionId {
    client_id: 1,
    generation: 1,
};
const ACCOUNT: &str = "acct";
const ACCOUNT_ID: u32 = 1;
const FIRST_PLAYER: u32 = 0x5000_0001;

// ---- the synthetic world ------------------------------------------------------------------------

const HUMAN: u32 = 1;
const HAT: u32 = 100;
const SHIRT: u32 = 101;
const PANTS: u32 = 102;
const SHOES: u32 = 103;
const SKIN_PALSET: u32 = 0x0F00_0001;
const HAIR_PALSET: u32 = 0x0F00_0002;

fn objdesc(texture: Option<(u32, u32)>, parts: &[u32]) -> ObjDesc {
    ObjDesc {
        version: 0x11,
        palette: None,
        subpalettes: Vec::new(),
        texture_changes: texture
            .map(|(o, n)| (0u8, DataId(o), DataId(n)))
            .into_iter()
            .collect(),
        anim_part_changes: parts.iter().map(|p| (16u8, DataId(*p))).collect(),
    }
}

fn gear(weenie_default: u32) -> GearItem {
    GearItem {
        name: String::new(),
        clothing_table: DataId(0),
        weenie_default,
    }
}

/// Heritage 1 (Aluvian, one male sex) and heritage 6 (Gear Knight, which gets no clothing), both
/// with one template and starting in the sample's Holtburg area.
fn char_gen() -> empyrean_dat::file_types::CharGen {
    let mut cg = sample::char_gen();
    let sex = SexCg {
        name: "Male".into(),
        scale: 110,
        setup: DataId(0x0200_0001),
        sound_table: DataId(0x2000_0001),
        icon: 0x0600_0001,
        base_palette: DataId(0x0400_007E),
        skin_palset: DataId(SKIN_PALSET),
        physics_table: DataId(0x3400_0004),
        motion_table: DataId(0x0900_0001),
        combat_table: DataId(0x3000_0000),
        base_objdesc: objdesc(None, &[]),
        hair_colors: vec![HAIR_PALSET],
        hair_styles: vec![
            HairStyle {
                icon: 0,
                bald: 0,
                alternate_setup: DataId(0),
                objdesc: objdesc(Some((0x0500_0001, 0x0500_0002)), &[0x0100_0001]),
            },
            HairStyle {
                icon: 0,
                bald: 1,
                alternate_setup: DataId(0x0200_0099),
                objdesc: objdesc(None, &[0x0100_0002, 0x0100_0003]),
            },
        ],
        eye_colors: vec![0x0400_0E01],
        eye_strips: vec![EyeStrip {
            icon: 0,
            icon_bald: 0,
            objdesc: objdesc(Some((0x0500_0010, 0x0500_0011)), &[]),
            objdesc_bald: objdesc(Some((0x0500_0012, 0x0500_0013)), &[]),
        }],
        nose_strips: vec![(0, objdesc(Some((0x0500_0020, 0x0500_0021)), &[]))],
        mouth_strips: vec![(0, objdesc(Some((0x0500_0030, 0x0500_0031)), &[]))],
        headgear: vec![gear(HAT)],
        shirts: vec![gear(SHIRT)],
        pants: vec![gear(PANTS)],
        footwear: vec![gear(SHOES)],
        clothing_colors: vec![0x0F00_0004],
    };
    let template = CharGenTemplate {
        name: "Custom".into(),
        icon: 0,
        title: CharacterTitle::Adventurer.0,
        attributes: [10; 6],
        normal_skills: Vec::new(),
        primary_skills: Vec::new(),
    };
    let heritage = |name: &str| CgHeritage {
        name: name.into(),
        icon: 0,
        setup: DataId(0x0200_0001),
        environment_setup: DataId(0x0200_0002),
        attribute_credits: 330,
        skill_credits: 50,
        primary_start_areas: vec![0],
        secondary_start_areas: Vec::new(),
        // Arcane Lore at a discount: normal 0, primary 1 (the table's costs are 4 and 6 - 4).
        skills: vec![(14, 0, 1)],
        templates: vec![template.clone()],
        sex_table_marker: 0,
        sexes: BTreeMap::from([(1, sex.clone())]),
    };
    cg.heritage_groups = BTreeMap::from([(1, heritage("Aluvian")), (6, heritage("Gear Knight"))]);
    cg.starter_areas = vec![
        StarterArea {
            name: "Holtburg".into(),
            locations: vec![DatPosition::new(
                sample::START_CELL,
                Frame::new(Vec3::new(84.0, 7.1, 94.005), Quat::IDENTITY),
            )],
        },
        StarterArea {
            name: "OlthoiLair".into(),
            locations: vec![DatPosition::new(
                CellId(0x0101_0001),
                Frame::new(Vec3::new(1.0, 2.0, 3.0), Quat::IDENTITY),
            )],
        },
    ];
    cg
}

fn skill(trained_cost: i32, specialized_cost: i32) -> SkillBase {
    SkillBase {
        description: String::new(),
        name: String::new(),
        icon: 0,
        trained_cost,
        specialized_cost,
        category: 1,
        chargen_use: 1,
        min_level: 1,
        formula: SkillFormula {
            w: 0,
            x: 1,
            y: 0,
            z: 3,
            attr1: 4,
            attr2: 3,
        },
        upper_bound: 0.0,
        lower_bound: 0.0,
        learn_mod: 0.0,
    }
}

/// `(id, trained, specialized-total)`; the specialization upgrade is the difference.
const SKILLS: [(u32, i32, i32); 8] = [
    (6, 10, 20),
    (14, 4, 6),
    (21, 6, 12),
    (22, 0, 4),
    (24, 0, 4),
    (31, 8, 16),
    (36, 0, 2),
    (49, 8, 16),
];

fn skill_table() -> SkillTable {
    let mut t = sample::skill_table();
    t.skills = SKILLS
        .iter()
        .map(|&(id, tc, sc)| (id, skill(tc, sc)))
        .collect();
    t
}

/// Max health = Endurance / 2, stamina = Endurance, mana = Self (the retail shapes).
fn secondary_attribute_table() -> SecondaryAttributeTable {
    let f = |attr1: u32, z: u32| SkillFormula {
        w: 0,
        x: 1,
        y: 0,
        z,
        attr1,
        attr2: 0,
    };
    SecondaryAttributeTable {
        id: DataId(file_id::SECONDARY_ATTRIBUTE_TABLE),
        health: f(2, 2),
        stamina: f(2, 1),
        mana: f(6, 1),
    }
}

fn taboo_table() -> TabooTable {
    TabooTable {
        id: DataId(file_id::TABOO_TABLE),
        audiences: vec![(1, vec![(1, vec!["*tabooword*".to_owned()])])],
    }
}

/// V227: the name check is the shared matcher in plain mode. On patterns of
/// `a`-`z` and `*` only (as all 266 retail patterns are) it refuses exactly the names ACE's regex
/// check refuses.
#[test]
fn the_shared_taboo_matcher_refuses_what_aces_regex_refuses() {
    let patterns = ["*tabooword*", "ass", "*bitch*", "boner*", "*jerk"].map(str::to_owned);
    let table = TabooTable {
        id: DataId(file_id::TABOO_TABLE),
        audiences: vec![(1, vec![(1, patterns.to_vec())])],
    };
    let names = [
        "Aldric",
        "ass",
        "Ass",
        "Glass",
        "Big Ass",
        "Mr Tabooword",
        "xtaboowordx",
        "Bitchy",
        "B1tch",
        "Bit Ch",
        "Boner",
        "Bonerific",
        "Aboner",
        "Jerk",
        "Knee Jerk",
        "Jerky",
        "",
        " ",
        "Ass Hat",
    ];
    for name in names {
        let ace = empyrean_dat::file_types::taboo_table::entry_contains_bad_word(&patterns, name);
        let shared = dereth_rules::taboo::contains_taboo_word(
            &table,
            &dereth_protocol::cp1252::Cp1252,
            name,
        );
        assert_eq!(shared, ace, "{name:?}");
    }
}

fn dats() -> Arc<DatManager> {
    empyrean_testkit::dats::with_stat_tables(FakeDats::new())
        .with_xp_table(sample::xp_table())
        .with_char_gen(char_gen())
        .with_skill_table(skill_table())
        .with_portal(
            file_id::SECONDARY_ATTRIBUTE_TABLE,
            secondary_attribute_table(),
        )
        .with_portal(file_id::TABOO_TABLE, taboo_table())
        .with_portal(
            SKIN_PALSET,
            PaletteSet {
                id: DataId(SKIN_PALSET),
                palette_ids: vec![DataId(0x0400_0A01), DataId(0x0400_0A02)],
            },
        )
        .build()
        .expect("fake dats")
}

fn clothing(wcid: u32, valid: EquipMask, priority: CoverageMask) -> ContentWeenie {
    ContentWeenie::new(wcid, &format!("clothing{wcid}"), WeenieType::Clothing)
        .with_int(PropertyInt::ValidLocations, signed(valid.0))
        .with_int(PropertyInt::ClothingPriority, signed(priority.0))
        .with_int(PropertyInt::EncumbranceVal, 50)
        .with_int(PropertyInt::Value, 7)
}

fn generic(wcid: u32, weenie_type: WeenieType) -> ContentWeenie {
    ContentWeenie::new(wcid, &format!("item{wcid}"), weenie_type)
        .with_int(PropertyInt::EncumbranceVal, 5)
}

/// `human`, the four clothes, the starter items of Healing, Jump (and its Aluvian extra) except
/// 20646 (missing: no IOU while `iou_trades` is off), a creature for the name check, and the free
/// ride spell.
fn content() -> MemContent {
    MemContent::new()
        .weenie(
            ContentWeenie::new(HUMAN, "human", WeenieType::Creature)
                .with_did(
                    empyrean_entity::enums::PropertyDataId::CombatTable,
                    0x3000_0000,
                )
                .with_string(PropertyString::Name, "Human")
                .with_int(PropertyInt::ItemsCapacity, 102)
                .with_int(PropertyInt::ContainersCapacity, 7),
        )
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
        .weenie(clothing(SHOES, EquipMask::FootWear, CoverageMask::Feet))
        .weenie(generic(628, WeenieType::Healer))
        .weenie(
            ContentWeenie::new(273, "coinstack", WeenieType::Coin)
                .with_int(PropertyInt::MaxStackSize, 25_000)
                .with_int(PropertyInt::StackSize, 1)
                .with_int(PropertyInt::StackUnitValue, 1)
                .with_int(PropertyInt::StackUnitEncumbrance, 0)
                .with_int(PropertyInt::Value, 1),
        )
        .weenie(
            ContentWeenie::new(166, "backpack", WeenieType::Container)
                .with_int(PropertyInt::ItemsCapacity, 24),
        )
        .weenie(generic(5084, WeenieType::Generic))
        .weenie(generic(33613, WeenieType::Generic))
        .weenie(generic(259, WeenieType::Generic))
        .weenie(generic(30988, WeenieType::Generic))
        .weenie(
            ContentWeenie::new(900, "drudge", WeenieType::Creature)
                .with_string(PropertyString::Name, "Drudge Skulker"),
        )
        .spell(Spell {
            id: 3815,
            name: "Free Ride to Holtburg".into(),
            position_obj_cell_id: Some(0xA9B4_0020),
            position_origin_x: Some(80.0),
            position_origin_y: Some(170.0),
            position_origin_z: Some(30.0),
            position_angles_w: Some(1.0),
            position_angles_x: Some(0.0),
            position_angles_y: Some(0.0),
            position_angles_z: Some(0.0),
            ..Spell::default()
        })
}

/// `GuidManager.Initialize` over an empty shard: no guid in use, no gaps.
struct EmptyShard;

impl ShardGuidQueries for EmptyShard {
    fn get_max_guid_found_in_range(&mut self, _min: u32, _max: u32) -> u32 {
        u32::MAX
    }
    fn get_sequence_gaps(&mut self, _min: u32, _limit: u32) -> Vec<(u32, u32)> {
        Vec::new()
    }
}

fn world() -> World {
    let now = ClockSnapshot {
        portal_year_ticks: 1000.0,
        unix_time: 1_767_225_600.0,
        utc: DotNetDateTime::new(2026, 1, 1),
        monotonic: Duration::ZERO,
    };
    let mut w = World::new(now, dats());
    w.content = Arc::new(content());
    w.world_manager.world_status = WorldStatusState::Open;
    guid_manager::initialize(&mut w, &mut EmptyShard);
    let account_id = w
        .auth
        .lock()
        .create_account(
            ACCOUNT,
            "pw",
            AccessLevel::Player,
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        )
        .expect("created")
        .account_id;
    assert_eq!(account_id, ACCOUNT_ID);
    let mut s = SessionData {
        state: SessionState::AuthConnected,
        ..Default::default()
    };
    s.set_account(ACCOUNT_ID, ACCOUNT.to_owned(), AccessLevel::Player);
    w.sessions.insert(S, s);
    w
}

// ---- driving the handler --------------------------------------------------------------------------

const INACTIVE: i32 = 0;
const UNTRAINED: i32 = 1;
const TRAINED: i32 = 2;
const SPECIALIZED: i32 = 3;

/// 55 skill advancement classes, Inactive except `set`.
fn sacs(set: &[(usize, i32)]) -> Vec<i32> {
    let mut v = vec![INACTIVE; 55];
    for &(skill, sac) in set {
        v[skill] = sac;
    }
    v
}

/// Healing, Jump and Run trained; Melee Defense and Arcane Lore specialized; Loyalty untrained:
/// 6 + 0 + 0 + (10 + 10) + (0 + 1, the heritage discount) = 27 of 50 credits.
fn standard_sacs() -> Vec<i32> {
    sacs(&[
        (21, TRAINED),
        (22, TRAINED),
        (24, TRAINED),
        (6, SPECIALIZED),
        (14, SPECIALIZED),
        (36, UNTRAINED),
    ])
}

/// Strength 10, Endurance 100, Coordination 100, Quickness 100, Focus 10, Self 10 (330).
const ABILITIES: [i32; 6] = [10, 100, 100, 100, 10, 10];

fn request(
    name: &str,
    heritage: u32,
    abilities: [i32; 6],
    skill_advancement_classes: Vec<i32>,
) -> CharacterSendCharGenResult {
    let mut result = CharGenResult {
        version: 1,
        heritage_group: heritage,
        gender: 1,
        eyes_strip: 0,
        nose_strip: 0,
        mouth_strip: 0,
        hair_color: 0,
        eye_color: 0,
        hair_style: 0,
        headgear_style: 0,
        headgear_color: 3,
        shirt_style: 0,
        shirt_color: 4,
        trousers_style: 0,
        trousers_color: 5,
        footwear_style: 0,
        footwear_color: 6,
        skin_shade: 1.0,
        hair_shade: 0.5,
        headgear_shade: 0.25,
        shirt_shade: 0.125,
        trousers_shade: 0.375,
        footwear_shade: 0.625,
        template_num: 0,
        strength: abilities[0],
        endurance: abilities[1],
        coordination: abilities[2],
        quickness: abilities[3],
        focus: abilities[4],
        self_: abilities[5],
        slot: 0,
        class_id: 1,
        skill_advancement_classes,
        name: name.to_owned(),
        start_area: 0,
        is_admin: 0,
        is_envoy: 0,
        checksum_value: 0,
    };
    result.checksum_value = result.checksum();
    CharacterSendCharGenResult {
        account: ACCOUNT.to_owned(),
        result,
    }
}

/// What the client was sent, in order: `(opcode, body)`.
struct Sent(Vec<(u32, Vec<u8>)>);

impl Sent {
    /// The `CharGenVerificationResponse`s, decoded by the client's decoder.
    fn responses(&self) -> Vec<CharGenVerificationResponse> {
        self.0
            .iter()
            .filter(|(op, _)| *op == CharGenVerificationResponse::OPCODE.0)
            .map(|(_, body)| {
                proto::read_body::<CharGenVerificationResponse>(body)
                    .expect("the client decodes the response")
            })
            .collect()
    }

    fn codes(&self) -> Vec<Cgvr> {
        self.responses()
            .iter()
            .map(|r| Cgvr(r.response_type))
            .collect()
    }

    fn opcodes(&self) -> Vec<u32> {
        self.0.iter().map(|(op, _)| *op).collect()
    }
}

/// Hands the message to the inbound manager and runs the queue and the shard callback stage until
/// nothing is left (the first stage runs the two name checks, the second the save's callback).
fn send_raw(w: &mut World, data: Vec<u8>) -> Sent {
    game_message::start_capture();
    handle_client_message(w, ClientMessage::new(data).expect("opcode"), S);
    run_inbound_message_queue(w);
    for _ in 0..4 {
        wm::run_shard_callbacks(w);
    }
    Sent(
        game_message::take_sent()
            .into_iter()
            .map(|(_, _, bytes)| {
                (
                    u32::from_le_bytes(bytes[..4].try_into().unwrap()),
                    bytes[4..].to_vec(),
                )
            })
            .collect(),
    )
}

fn send(w: &mut World, m: &CharacterSendCharGenResult) -> Sent {
    send_raw(w, proto::write_blob(m).expect("encodes"))
}

fn characters(w: &World) -> Vec<(u32, String)> {
    w.shard
        .base_database()
        .get_characters(ACCOUNT_ID, false)
        .into_iter()
        .map(|c| (c.id, c.name))
        .collect()
}

fn saved_biota(w: &World, id: u32) -> empyrean_entity::Biota {
    let b = w.shard.base_database().get_biota(id, false).expect("saved");
    BiotaConverter::convert_to_entity_biota(&b, false)
}

/// `(inventory, wielded)`, as entity biotas in the shard's order.
fn possessions(w: &World, id: u32) -> (Vec<empyrean_entity::Biota>, Vec<empyrean_entity::Biota>) {
    let p = w.shard.base_database().get_possessed_biotas_in_parallel(id);
    let conv = |v: &[empyrean_store::models::shard::Biota]| {
        v.iter()
            .map(|b| BiotaConverter::convert_to_entity_biota(b, false))
            .collect()
    };
    (conv(&p.inventory), conv(&p.wielded_items))
}

fn int(b: &empyrean_entity::Biota, p: PropertyInt) -> Option<i32> {
    b.properties_int.as_ref().and_then(|d| d.get(&p).copied())
}

fn did(b: &empyrean_entity::Biota, p: PropertyDataId) -> Option<u32> {
    b.properties_did.as_ref().and_then(|d| d.get(&p).copied())
}

fn float(b: &empyrean_entity::Biota, p: PropertyFloat) -> Option<f64> {
    b.properties_float.as_ref().and_then(|d| d.get(&p).copied())
}

fn string(b: &empyrean_entity::Biota, p: PropertyString) -> Option<String> {
    b.properties_string
        .as_ref()
        .and_then(|d| d.get(&p).cloned())
}

fn iid(b: &empyrean_entity::Biota, p: PropertyInstanceId) -> Option<u32> {
    b.properties_iid.as_ref().and_then(|d| d.get(&p).copied())
}

fn sac_of(b: &empyrean_entity::Biota, skill: Skill) -> Option<SkillAdvancementClass> {
    b.get_skill(skill).map(|s| s.sac)
}

fn signed(v: u32) -> i32 {
    i32::from_ne_bytes(v.to_ne_bytes())
}

// ---- the create path ------------------------------------------------------------------------------

#[test]
fn a_valid_create_succeeds_and_the_character_is_listed() {
    let mut w = world();
    let sent = send(&mut w, &request("Aldric", 1, ABILITIES, standard_sacs()));

    let responses = sent.responses();
    assert_eq!(responses.len(), 1, "one response: {:?}", sent.opcodes());
    assert_eq!(Cgvr(responses[0].response_type), Cgvr::Ok);
    assert_eq!(responses[0].identity.gid.0, FIRST_PLAYER);
    assert_eq!(responses[0].identity.name, "Aldric");
    assert_eq!(responses[0].identity.seconds_greyed_out, 0);

    assert_eq!(characters(&w), vec![(FIRST_PLAYER, "Aldric".to_owned())]);
    let session = w.sessions.get(S).expect("session");
    assert_eq!(session.characters.len(), 1);
    assert_eq!(
        (
            session.characters[0].id,
            session.characters[0].name.as_str()
        ),
        (FIRST_PLAYER, "Aldric")
    );
}

#[test]
fn the_saved_character_carries_the_requested_appearance_attributes_and_skills() {
    let mut w = world();
    let _ = send(&mut w, &request("Aldric", 1, ABILITIES, standard_sacs()));
    let b = saved_biota(&w, FIRST_PLAYER);

    // Heritage and sex, from CharGen.
    assert_eq!(
        int(&b, PropertyInt::HeritageGroup),
        Some(HeritageGroup::Aluvian.0)
    );
    assert_eq!(
        string(&b, PropertyString::HeritageGroup).as_deref(),
        Some("Aluvian")
    );
    assert_eq!(int(&b, PropertyInt::Gender), Some(1));
    assert_eq!(string(&b, PropertyString::Sex).as_deref(), Some("Male"));
    assert_eq!(string(&b, PropertyString::Name).as_deref(), Some("Aldric"));
    assert_eq!(
        float(&b, PropertyFloat::DefaultScale),
        Some(f64::from(110.0_f32 / 100.0_f32))
    );
    assert_eq!(did(&b, PropertyDataId::MotionTable), Some(0x0900_0001));
    assert_eq!(
        did(&b, PropertyDataId::Setup),
        Some(0x0200_0001),
        "hair style 0 has no alternate setup"
    );
    assert_eq!(
        int(&b, PropertyInt::Hairstyle),
        None,
        "one anim part change: not a body style"
    );
    assert_eq!(did(&b, PropertyDataId::EyesTexture), Some(0x0500_0011));
    assert_eq!(
        did(&b, PropertyDataId::DefaultEyesTexture),
        Some(0x0500_0010)
    );
    assert_eq!(did(&b, PropertyDataId::NoseTexture), Some(0x0500_0021));
    assert_eq!(
        did(&b, PropertyDataId::DefaultNoseTexture),
        Some(0x0500_0020)
    );
    assert_eq!(did(&b, PropertyDataId::MouthTexture), Some(0x0500_0031));
    assert_eq!(
        did(&b, PropertyDataId::DefaultMouthTexture),
        Some(0x0500_0030)
    );
    assert_eq!(did(&b, PropertyDataId::HeadObject), Some(0x0100_0001));
    assert_eq!(
        did(&b, PropertyDataId::SkinPalette),
        Some(0x0400_0A02),
        "hue 1.0: the last palette"
    );
    assert_eq!(float(&b, PropertyFloat::Shade), Some(1.0));
    assert_eq!(
        did(&b, PropertyDataId::HairPalette),
        Some(0),
        "a missing palette set answers 0"
    );
    assert_eq!(did(&b, PropertyDataId::EyesPalette), Some(0x0400_0E01));
    assert_eq!(
        string(&b, PropertyString::Template).as_deref(),
        Some("Custom")
    );
    assert_eq!(
        int(&b, PropertyInt::CharacterTitleId),
        Some(signed(CharacterTitle::Adventurer.0))
    );
    assert_eq!(int(&b, PropertyInt::NumCharacterTitles), Some(1));

    // Attributes, and vitals at their base (health = endurance / 2).
    let attrs: Vec<u32> = [
        PropertyAttribute::Strength,
        PropertyAttribute::Endurance,
        PropertyAttribute::Coordination,
        PropertyAttribute::Quickness,
        PropertyAttribute::Focus,
        PropertyAttribute::Self_,
    ]
    .iter()
    .map(|a| {
        b.properties_attribute
            .as_ref()
            .unwrap()
            .get(a)
            .unwrap()
            .init_level
    })
    .collect();
    assert_eq!(attrs, [10, 100, 100, 100, 10, 10]);
    let vital = |v: PropertyAttribute2nd| {
        b.properties_attribute_2nd
            .as_ref()
            .unwrap()
            .get(&v)
            .unwrap()
            .current_level
    };
    assert_eq!(
        (
            vital(PropertyAttribute2nd::MaxHealth),
            vital(PropertyAttribute2nd::MaxStamina),
            vital(PropertyAttribute2nd::MaxMana)
        ),
        (50, 100, 10)
    );

    // Skills: credits 50 - 27; Trained with the creation bonus, Specialized reset to 10.
    assert_eq!(int(&b, PropertyInt::TotalSkillCredits), Some(50));
    assert_eq!(int(&b, PropertyInt::AvailableSkillCredits), Some(23));
    let jump = b.get_skill(Skill::Jump).expect("jump");
    assert_eq!(
        (jump.sac, jump.pp, jump.level_from_pp, jump.init_level),
        (SkillAdvancementClass::Trained, 526, 5, 0)
    );
    let md = b.get_skill(Skill::MeleeDefense).expect("melee defense");
    assert_eq!(
        (md.sac, md.pp, md.level_from_pp, md.init_level),
        (SkillAdvancementClass::Specialized, 0, 0, 10)
    );
    assert_eq!(
        sac_of(&b, Skill::ArcaneLore),
        Some(SkillAdvancementClass::Specialized)
    );
    assert_eq!(
        sac_of(&b, Skill::Loyalty),
        Some(SkillAdvancementClass::Untrained)
    );
    assert_eq!(sac_of(&b, Skill::Axe), None, "Inactive skills are skipped");

    // Heritage extras.
    assert_eq!(
        int(&b, PropertyInt::MeleeMastery),
        Some(WeaponType::Dagger.0)
    );
    assert_eq!(int(&b, PropertyInt::RangedMastery), Some(WeaponType::Bow.0));
    assert_eq!(int(&b, PropertyInt::AugmentationJackOfAllTrades), Some(1));

    // Positions: the start area, Sanctuary the same, Instantiation from the free ride spell.
    let pos = |t: PositionType| {
        b.properties_position
            .as_ref()
            .unwrap()
            .get(&t)
            .cloned()
            .expect("position")
    };
    let location = pos(PositionType::Location);
    assert_eq!(
        (
            location.obj_cell_id,
            location.position_x,
            location.position_y,
            location.position_z,
            location.rotation_w
        ),
        (0xA9B4_0019, 84.0, 7.1, 94.005, 1.0)
    );
    assert_eq!(pos(PositionType::Sanctuary), location);
    let inst = pos(PositionType::Instantiation);
    assert_eq!(
        (
            inst.obj_cell_id,
            inst.position_x,
            inst.position_y,
            inst.position_z
        ),
        (0xA9B4_0020, 80.0, 170.0, 30.0)
    );
    assert_eq!(
        b.properties_bool
            .as_ref()
            .and_then(|d| d.get(&PropertyBool::RecallsDisabled).copied()),
        Some(true)
    );

    // The character row: hair textures, the title book, the default options.
    let c = w
        .shard
        .base_database()
        .get_character(FIRST_PLAYER)
        .expect("character");
    assert_eq!(
        (c.account_id, c.hair_texture, c.default_hair_texture),
        (ACCOUNT_ID, 0x0500_0002, 0x0500_0001)
    );
    assert_eq!(
        c.character_properties_title_book
            .iter()
            .map(|t| t.title_id)
            .collect::<Vec<_>>(),
        [CharacterTitle::Adventurer.0]
    );
    let o1 = CharacterOption::CharacterOptions1Default
        .character_options1()
        .unwrap()
        .0;
    assert_eq!(c.character_options_1, signed(o1));
    let o2 = CharacterOption::CharacterOptions2Default
        .character_options2()
        .unwrap()
        .0
        | CharacterOption::ListenToPKDeathMessages
            .character_options2()
            .unwrap()
            .0;
    assert_eq!(c.character_options_2, signed(o2));
    assert!(!c.is_plussed);
}

#[test]
fn the_chosen_clothes_are_worn() {
    let mut w = world();
    let _ = send(&mut w, &request("Aldric", 1, ABILITIES, standard_sacs()));
    let (_, wielded) = possessions(&w, FIRST_PLAYER);

    type Worn = (u32, Option<i32>, Option<u32>, Option<i32>, Option<f64>);
    let mut worn: Vec<Worn> = wielded
        .iter()
        .map(|b| {
            (
                b.weenie_class_id,
                int(b, PropertyInt::CurrentWieldedLocation),
                iid(b, PropertyInstanceId::Wielder),
                int(b, PropertyInt::PaletteTemplate),
                float(b, PropertyFloat::Shade),
            )
        })
        .collect();
    worn.sort_by_key(|x| x.0);
    let loc = |m: EquipMask| Some(signed(m.0));
    assert_eq!(
        worn,
        vec![
            (
                HAT,
                loc(EquipMask::HeadWear),
                Some(FIRST_PLAYER),
                Some(3),
                Some(0.25)
            ),
            (
                SHIRT,
                loc(EquipMask::ChestWear),
                Some(FIRST_PLAYER),
                Some(4),
                Some(0.125)
            ),
            (
                PANTS,
                loc(EquipMask::UpperLegWear),
                Some(FIRST_PLAYER),
                Some(5),
                Some(0.375)
            ),
            (
                SHOES,
                loc(EquipMask::FootWear),
                Some(FIRST_PLAYER),
                Some(6),
                Some(0.625)
            ),
        ]
    );
}

/// The starter items ACE's own deserializer builds from starterGear.json (the harness vectors),
/// walked as `PlayerFactory.Create` walks them, for the trained skills and the heritage, keeping
/// the weenies the content has.
fn expected_starter_items(trained: &[u64], heritage: u64, content_has: &[u32]) -> Vec<(u32, u64)> {
    let file = vectors::load_named("chargen", "starter_gear");
    let mut out: Vec<(u32, u64)> = Vec::new();
    for case in &file.cases {
        let skill = case.output["skill_id"].as_u64().unwrap();
        if !trained.contains(&skill) {
            continue;
        }
        let mut items: Vec<&serde_json::Value> =
            case.output["gear"].as_array().unwrap().iter().collect();
        if let Some(h) = case.output["heritage"]
            .as_array()
            .unwrap()
            .iter()
            .find(|h| h["heritage_id"].as_u64() == Some(heritage))
        {
            items.extend(h["gear"].as_array().unwrap());
        }
        for item in items {
            let wcid = u32::try_from(item["weenie_id"].as_u64().unwrap()).unwrap();
            if content_has.contains(&wcid) && !out.iter().any(|(w, _)| *w == wcid) {
                out.push((wcid, item["stack_size"].as_u64().unwrap()));
            }
        }
    }
    out
}

#[test]
fn the_starter_gear_matches_the_table_for_the_heritage() {
    let mut w = world();
    let _ = send(&mut w, &request("Aldric", 1, ABILITIES, standard_sacs()));
    let (inventory, _) = possessions(&w, FIRST_PLAYER);

    let content_has = [628, 273, 166, 5084, 33613, 259, 30988];
    let expected = expected_starter_items(&[6, 14, 21, 22, 24, 36], 1, &content_has);
    assert_eq!(
        expected.iter().map(|e| e.0).collect::<Vec<_>>(),
        [628, 273, 166, 5084, 33613, 259, 30988]
    );

    // The shard returns inventory in guid order, which is creation order here.
    let got: Vec<u32> = inventory.iter().map(|b| b.weenie_class_id).collect();
    assert_eq!(got, expected.iter().map(|e| e.0).collect::<Vec<_>>());
    for b in &inventory {
        assert_eq!(iid(b, PropertyInstanceId::Container), Some(FIRST_PLAYER));
        assert_eq!(iid(b, PropertyInstanceId::Owner), Some(FIRST_PLAYER));
    }
    // Pyreals: stack 10000 of max 25000, value follows the stack.
    let coins = inventory.iter().find(|b| b.weenie_class_id == 273).unwrap();
    assert_eq!(
        (
            int(coins, PropertyInt::StackSize),
            int(coins, PropertyInt::Value)
        ),
        (Some(10_000), Some(10_000))
    );
    // Each item is placed at 0 and pushes the earlier ones along, packs (the sack, UseBackpackSlot)
    // and other items each in their own order.
    let placements: Vec<Option<i32>> = inventory
        .iter()
        .map(|b| int(b, PropertyInt::PlacementPosition))
        .collect();
    assert_eq!(placements, [5, 4, 0, 3, 2, 1, 0].map(Some));
}

#[test]
fn a_gear_knight_wears_nothing() {
    let mut w = world();
    let sent = send(&mut w, &request("Cogsworth", 6, ABILITIES, standard_sacs()));
    assert_eq!(sent.codes(), [Cgvr::Ok]);
    let (inventory, wielded) = possessions(&w, FIRST_PLAYER);
    assert!(wielded.is_empty(), "Gear Knights do not get clothing");
    // Jump's Gear Knight extras (43018, 42979, 43022) are not in this content: skipped.
    assert!(
        !inventory.iter().any(|b| b.weenie_class_id == 30988),
        "the Aluvian extra is not granted"
    );
    let b = saved_biota(&w, FIRST_PLAYER);
    assert_eq!(int(&b, PropertyInt::AugmentationDamageReduction), Some(1));
    assert_eq!(int(&b, PropertyInt::MeleeMastery), Some(WeaponType::Mace.0));
}

#[test]
fn starter_spells_follow_trained_or_specialized() {
    let spells = |sac: i32| {
        let mut w = world();
        let _ = send(&mut w, &request("Aldric", 1, ABILITIES, sacs(&[(31, sac)])));
        let b = saved_biota(&w, FIRST_PLAYER);
        let mut v: Vec<i32> = b
            .properties_spell_book
            .map(|s| s.keys().copied().collect())
            .unwrap_or_default();
        v.sort_unstable();
        v
    };
    assert_eq!(spells(TRAINED), [17, 18, 1421]);
    assert_eq!(spells(SPECIALIZED), [17, 18, 653, 1421, 1445]);
}

// ---- refusals ---------------------------------------------------------------------------------------

#[test]
fn a_taboo_name_is_banned() {
    let mut w = world();
    let sent = send(
        &mut w,
        &request("Mytabooword", 1, ABILITIES, standard_sacs()),
    );
    assert_eq!(sent.codes(), [Cgvr::NameBanned]);
    assert!(characters(&w).is_empty());
}

/// Divergence: V392
#[test]
fn an_infiltration_world_answers_a_later_heritage_pending_with_a_popup() {
    let mut w = world();
    w.era = empyrean_common::era::EraId::Infiltration.rules();
    // Viamontian (7), which only Throne of Destiny accounts could choose.
    let sent = send(&mut w, &request("Aldric", 7, ABILITIES, standard_sacs()));
    assert_eq!(sent.codes(), [Cgvr::Pending]);
    assert!(
        sent.opcodes().contains(&0xF7B0),
        "the refusal's popup arrives as a game event: {:X?}",
        sent.opcodes()
    );
    assert!(characters(&w).is_empty());
    // Aluvian (1) is the era's.
    let sent = send(&mut w, &request("Aldric", 1, ABILITIES, standard_sacs()));
    assert_eq!(sent.codes(), [Cgvr::Ok]);
}

#[test]
fn a_creature_name_is_banned() {
    let mut w = world();
    let sent = send(
        &mut w,
        &request("drudge skulker", 1, ABILITIES, standard_sacs()),
    );
    assert_eq!(
        sent.codes(),
        [Cgvr::NameBanned],
        "IsCreatureNameInWorldDatabase ignores case"
    );
    assert!(characters(&w).is_empty());
}

#[test]
fn a_taken_name_is_answered_name_in_use_once() {
    let mut w = world();
    assert_eq!(
        send(&mut w, &request("Aldric", 1, ABILITIES, standard_sacs())).codes(),
        [Cgvr::Ok]
    );
    // Case and trailing-space variants collide under utf8mb4_uca1400_ai_ci (F21). V242: one
    // answer (ACE's first check also answered, so it sent two).
    for variant in ["Aldric", "aldric", "ALDRIC", "Aldric "] {
        let sent = send(&mut w, &request(variant, 1, ABILITIES, standard_sacs()));
        assert_eq!(sent.codes(), [Cgvr::NameInUse], "{variant:?}: one answer");
    }
    assert_eq!(characters(&w).len(), 1);
}

#[test]
fn invalid_attribute_credits_are_corrupt() {
    for abilities in [
        [10, 100, 100, 100, 10, 11],
        [9, 100, 100, 100, 10, 10],
        [10, 101, 100, 100, 10, 9],
    ] {
        let mut w = world();
        let sent = send(&mut w, &request("Aldric", 1, abilities, standard_sacs()));
        assert_eq!(sent.codes(), [Cgvr::Corrupt], "{abilities:?}");
        assert!(characters(&w).is_empty());
    }
}

#[test]
fn too_many_skill_credits_are_corrupt() {
    // 50 credits: Melee Defense 10+10, Creature Enchantment 8+8, Dual Wield 8+8 = 52.
    let over = sacs(&[(6, SPECIALIZED), (31, SPECIALIZED), (49, SPECIALIZED)]);
    let mut w = world();
    let sent = send(&mut w, &request("Aldric", 1, ABILITIES, over));
    assert_eq!(sent.codes(), [Cgvr::Corrupt]);
    assert!(characters(&w).is_empty());

    // A skill the SkillTable does not have (7; the retired weapon skills are added at load):
    // InvalidSkillRequested, also Corrupt.
    let mut w = world();
    let sent = send(
        &mut w,
        &request("Aldric", 1, ABILITIES, sacs(&[(7, TRAINED)])),
    );
    assert_eq!(sent.codes(), [Cgvr::Corrupt]);
}

/// V248/V249: retail's costing charges a skill the table does not name -1, so
/// skill 0 (no skill) and any unnamed skill stay refused; a pile of them cannot buy credits back.
#[test]
fn skill_zero_and_unnamed_skills_cannot_refund_credits() {
    // 52 credits of picks (over the 50), plus skill 0 and two unnamed skills "trained".
    let mut picks = sacs(&[
        (6, SPECIALIZED),
        (31, SPECIALIZED),
        (49, SPECIALIZED),
        (7, TRAINED),
        (8, TRAINED),
    ]);
    picks[0] = TRAINED;
    let mut w = world();
    assert_eq!(
        send(&mut w, &request("Aldric", 1, ABILITIES, picks)).codes(),
        [Cgvr::Corrupt]
    );
    assert!(characters(&w).is_empty());

    // Skill 0 alone, within budget.
    let mut picks = standard_sacs();
    picks[0] = SPECIALIZED;
    let mut w = world();
    assert_eq!(
        send(&mut w, &request("Aldric", 1, ABILITIES, picks)).codes(),
        [Cgvr::Corrupt]
    );
    assert!(characters(&w).is_empty());
}

/// V248/V249: retail's wizard stores 1..=32 characters, so an empty name or a
/// longer one is answered `Corrupt` and nothing is saved; 32 is accepted.
#[test]
fn a_name_outside_one_to_thirty_two_characters_is_answered_corrupt() {
    for name in [String::new(), "A".repeat(33), "Aldric".repeat(50)] {
        let mut w = world();
        assert_eq!(
            send(&mut w, &request(&name, 1, ABILITIES, standard_sacs())).codes(),
            [Cgvr::Corrupt],
            "{} characters",
            name.len()
        );
        assert!(characters(&w).is_empty());
    }
    let mut w = world();
    assert_eq!(
        send(
            &mut w,
            &request(&"A".repeat(32), 1, ABILITIES, standard_sacs())
        )
        .codes(),
        [Cgvr::Ok]
    );
}

#[test]
fn a_skill_list_of_other_than_55_boots_the_client_without_a_response() {
    let mut w = world();
    let mut short = standard_sacs();
    short.pop();
    let sent = send(&mut w, &request("Aldric", 1, ABILITIES, short));
    assert!(
        sent.codes().is_empty(),
        "the session is terminated with a boot message instead"
    );
    assert!(characters(&w).is_empty());
}

#[test]
fn the_account_world_and_shutdown_gates() {
    // Another account's string: ignored.
    let mut w = world();
    let mut m = request("Aldric", 1, ABILITIES, standard_sacs());
    m.account = "other".to_owned();
    assert!(send(&mut w, &m).0.is_empty());

    // World closed, player access: CharacterError LogonServerFull.
    let mut w = world();
    w.world_manager.world_status = WorldStatusState::Closed;
    let sent = send(&mut w, &request("Aldric", 1, ABILITIES, standard_sacs()));
    assert_eq!(sent.opcodes(), [0xF659]);
    assert_eq!(sent.0[0].1, 0x15u32.to_le_bytes());

    // World closed, but an advocate may create.
    let mut w = world();
    w.world_manager.world_status = WorldStatusState::Closed;
    w.sessions.get_mut(S).unwrap().access_level = AccessLevel::Advocate;
    assert_eq!(
        send(&mut w, &request("Aldric", 1, ABILITIES, standard_sacs())).codes(),
        [Cgvr::Ok]
    );

    // Shutting down: LogonServerFull.
    let mut w = world();
    w.server_manager.shutdown_in_progress = true;
    assert_eq!(
        send(&mut w, &request("Aldric", 1, ABILITIES, standard_sacs())).opcodes(),
        [0xF659]
    );
}

// ---- PlayerFactory.Create directly ------------------------------------------------------------------

fn info_of(m: &CharacterSendCharGenResult) -> CharacterCreateInfo {
    let blob = proto::write_blob(m).unwrap();
    let mut r = BinaryReader::new(&blob[4..]);
    let _ = r.read_string16l().unwrap();
    let mut info = CharacterCreateInfo::default();
    info.unpack(&mut r).expect("unpacks");
    info
}

fn create(w: &mut World, m: &CharacterSendCharGenResult) -> (CreateResult, CreatedPlayer) {
    let weenie = w.content.get_cached_weenie_by_class_name("human").unwrap();
    let guid = guid_manager::new_player_guid(w);
    player_factory::create(
        w,
        &info_of(m),
        weenie,
        guid,
        ACCOUNT_ID,
        WeenieType::Creature,
    )
}

/// A property dictionary as a sorted map, for comparison.
fn sorted<K: Ord + Copy + std::hash::Hash, V: Clone>(
    d: Option<&empyrean_common::dotnet::DotNetDict<K, V>>,
) -> BTreeMap<K, V> {
    d.map(|d| d.iter().map(|(k, v)| (*k, v.clone())).collect())
        .unwrap_or_default()
}

#[test]
fn the_saved_biota_round_trips() {
    let mut w = world();
    let (result, mut p) = create(&mut w, &request("Aldric", 1, ABILITIES, standard_sacs()));
    assert_eq!(result, CreateResult::Success);

    let mut possessions: Vec<empyrean_entity::Biota> = p
        .get_all_possessions(&mut w)
        .iter()
        .map(|o| o.biota.clone())
        .collect();
    assert_eq!(possessions.len(), 11, "7 starter items and 4 clothes");
    let saved = w.shard.base_database().add_character_in_parallel(
        &mut p.player.biota.clone(),
        &mut possessions,
        p.character(),
    );
    assert!(saved);

    let back = saved_biota(&w, p.player.guid.full());
    let b = &p.player.biota;
    assert_eq!(
        (back.id, back.weenie_class_id, back.weenie_type),
        (b.id, b.weenie_class_id, b.weenie_type)
    );
    assert_eq!(
        sorted(back.properties_int.as_ref()),
        sorted(b.properties_int.as_ref())
    );
    assert_eq!(
        sorted(back.properties_int64.as_ref()),
        sorted(b.properties_int64.as_ref())
    );
    assert_eq!(
        sorted(back.properties_bool.as_ref()),
        sorted(b.properties_bool.as_ref())
    );
    assert_eq!(
        sorted(back.properties_float.as_ref()),
        sorted(b.properties_float.as_ref())
    );
    assert_eq!(
        sorted(back.properties_did.as_ref()),
        sorted(b.properties_did.as_ref())
    );
    assert_eq!(
        sorted(back.properties_iid.as_ref()),
        sorted(b.properties_iid.as_ref())
    );
    assert_eq!(
        sorted(back.properties_string.as_ref()),
        sorted(b.properties_string.as_ref())
    );
    assert_eq!(
        sorted(back.properties_position.as_ref()),
        sorted(b.properties_position.as_ref())
    );
    assert_eq!(
        sorted(back.properties_attribute.as_ref()),
        sorted(b.properties_attribute.as_ref())
    );
    assert_eq!(
        sorted(back.properties_attribute_2nd.as_ref()),
        sorted(b.properties_attribute_2nd.as_ref())
    );
    assert_eq!(
        sorted(back.properties_skill.as_ref()),
        sorted(b.properties_skill.as_ref())
    );
    assert_eq!(
        sorted(back.properties_spell_book.as_ref()),
        sorted(b.properties_spell_book.as_ref())
    );

    let c = w
        .shard
        .base_database()
        .get_character(p.character().id)
        .unwrap();
    assert_eq!(
        (c.name.as_str(), c.account_id, c.hair_texture),
        ("Aldric", ACCOUNT_ID, 0x0500_0002)
    );
}

#[test]
fn a_body_style_hair_sets_the_hairstyle_and_the_alternate_setup() {
    let mut w = world();
    let mut m = request("Aldric", 1, ABILITIES, standard_sacs());
    m.result.hair_style = 1;
    let (result, p) = create(&mut w, &m);
    assert_eq!(result, CreateResult::Success);
    assert_eq!(p.player.get_property(PropertyInt::Hairstyle), Some(1));
    assert_eq!(
        p.player.get_property(PropertyDataId::Setup),
        Some(0x0200_0099)
    );
    assert_eq!(
        p.player.get_property(PropertyDataId::HeadObject),
        None,
        "two part changes: no head object"
    );
    assert_eq!(
        p.player.get_property(PropertyDataId::EyesTexture),
        Some(0x0500_0013),
        "bald eyes"
    );
    assert_eq!(
        (
            p.character().hair_texture,
            p.character().default_hair_texture
        ),
        (0, 0),
        "no texture change"
    );
}

#[test]
fn no_headgear_is_max_uint_and_an_unknown_start_area_throws() {
    let mut w = world();
    let mut m = request("Aldric", 1, ABILITIES, standard_sacs());
    m.result.headgear_style = -1;
    let (_, p) = create(&mut w, &m);
    assert!(!p
        .equipped_objects
        .iter()
        .any(|o| o.biota.weenie_class_id == HAT));
    assert_eq!(p.equipped_objects.len(), 3);

    // StarterAreas[5]: ArgumentOutOfRangeException, which the handler's catch logs.
    let mut w = world();
    let mut m = request("Aldric", 1, ABILITIES, standard_sacs());
    m.result.start_area = 5;
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| create(&mut w, &m)));
    assert!(r.is_err());
    // Through the handler: no create response, nothing saved.
    let sent = send(&mut w, &m);
    assert!(sent.codes().is_empty());
    assert!(characters(&w).is_empty());
}

#[test]
fn the_olthoi_lair_starts_and_instantiates_in_the_same_place() {
    let mut w = world();
    let mut m = request("Aldric", 1, ABILITIES, standard_sacs());
    m.result.start_area = 1;
    let (_, p) = create(&mut w, &m);
    let loc = p.player.get_position(PositionType::Location).unwrap();
    let inst = p.player.get_position(PositionType::Instantiation).unwrap();
    assert_eq!(
        (loc.landblock_id().raw(), inst.landblock_id().raw()),
        (0x0101_0001, 0x0101_0001)
    );
    assert_eq!((inst.pos(), inst.rotation()), (loc.pos(), loc.rotation()));
}

// ---- an era's listed start positions ----------------------------------------------------------------

fn recalls_disabled(p: &CreatedPlayer) -> Option<bool> {
    p.player
        .biota
        .properties_bool
        .as_ref()
        .and_then(|d| d.get(&PropertyBool::RecallsDisabled).copied())
}

/// Divergence: V386
#[test]
fn an_infiltration_character_starts_outdoors_in_its_town_with_recalls_enabled() {
    let rules = empyrean_common::era::EraId::Infiltration.rules();
    let mut seen = std::collections::BTreeSet::new();
    for start_area in [0, 1] {
        for _ in 0..24 {
            let mut w = world();
            w.era = rules;
            let mut m = request("Aldric", 1, ABILITIES, standard_sacs());
            m.result.start_area = start_area;
            let (result, p) = create(&mut w, &m);
            assert_eq!(result, CreateResult::Success);
            let loc = p.player.get_position(PositionType::Location).unwrap();
            let landblock = loc.landblock_id().raw() >> 16;
            // "Holtburg" is Holtburg's; the Olthoi lair, which the era has no start for, is too.
            assert!(
                [0xA9B0, 0xA5B4].contains(&landblock),
                "area {start_area} starts in {landblock:04X}"
            );
            seen.insert(landblock);
            let inst = p.player.get_position(PositionType::Instantiation).unwrap();
            let sanctuary = p.player.get_position(PositionType::Sanctuary).unwrap();
            for other in [inst, sanctuary] {
                assert_eq!(other.landblock_id(), loc.landblock_id());
                assert_eq!((other.pos(), other.rotation()), (loc.pos(), loc.rotation()));
            }
            assert_eq!(recalls_disabled(&p), None, "recalls are enabled");
        }
    }
    assert_eq!(seen.len(), 2, "both Holtburg areas are used: {seen:04X?}");
}

/// Divergence: V392
#[test]
fn an_infiltration_character_may_not_train_a_skill_from_after_the_2012_consolidation() {
    let rules = empyrean_common::era::EraId::Infiltration.rules();
    // Dual Wield (49), which the test table has, and Healing (21).
    for (skill, allowed) in [(49, false), (21, true)] {
        let mut w = world();
        w.era = rules;
        let m = request(
            "Aldric",
            1,
            ABILITIES,
            sacs(&[(skill, TRAINED), (6, TRAINED)]),
        );
        let (result, _) = create(&mut w, &m);
        if allowed {
            assert_eq!(result, CreateResult::Success, "skill {skill}");
        } else {
            assert_eq!(result, CreateResult::InvalidSkillRequested, "skill {skill}");
        }
    }
    // At the end of retail the same Dual Wield character is ACE's success.
    let mut w = world();
    let m = request("Aldric", 1, ABILITIES, sacs(&[(49, TRAINED), (6, TRAINED)]));
    assert_eq!(create(&mut w, &m).0, CreateResult::Success);
}

/// Divergence: V408, V409, V411
/// An Infiltration character is given ClassicACE's Infiltration starter gear (Jump's pyreals,
/// Welcome Letter, sack, Calling Stone and the heritage's food; the heritage's starter weapon for
/// an old weapon skill), and neither weapon masteries nor an innate augmentation.
#[test]
fn an_infiltration_character_gets_the_eras_starter_gear_and_no_innate_augmentation() {
    let mut w = world();
    w.era = empyrean_common::era::EraId::Infiltration.rules();
    w.content = Arc::new(
        content()
            .weenie(generic(535, WeenieType::MeleeWeapon))
            .weenie(generic(1077, WeenieType::Book)),
    );
    let skills = sacs(&[
        (21, TRAINED),
        (22, TRAINED),
        (24, TRAINED),
        (6, SPECIALIZED),
        (11, TRAINED),
    ]);
    let sent = send(&mut w, &request("Aldric", 1, ABILITIES, skills));
    assert_eq!(sent.codes(), [Cgvr::Ok]);
    let (inventory, _) = possessions(&w, FIRST_PLAYER);
    let got: std::collections::BTreeSet<u32> =
        inventory.iter().map(|b| b.weenie_class_id).collect();
    // The kit, the pyreals, the letter, the sack, the Calling Stone, the Aluvian's bread and the
    // Aluvian's Starter Sword; none of ACE's extras (33613, 30988).
    assert_eq!(
        got,
        [628, 273, 1077, 166, 5084, 259, 535].into_iter().collect()
    );
    let b = saved_biota(&w, FIRST_PLAYER);
    assert_eq!(int(&b, PropertyInt::AugmentationJackOfAllTrades), None);
    assert_eq!(int(&b, PropertyInt::MeleeMastery), None);
}

/// Divergence: V391
#[test]
fn an_infiltration_starter_area_named_as_a_listed_area_starts_exactly_there() {
    let rules = empyrean_common::era::EraId::Infiltration.rules();
    let empyrean_common::era::StartPositions::Towns(towns) = rules.start_positions else {
        panic!("listed towns");
    };
    let area = empyrean_common::era::StartPositions::area(towns, "Shoushi West").expect("listed");
    assert_eq!(area.cell, 0xD655_0023);
    assert!(empyrean_common::era::StartPositions::area(towns, "Shoushi").is_none());
}

/// Divergence: V386
#[test]
fn an_end_of_retail_character_starts_in_the_starter_area_with_recalls_disabled() {
    let mut w = world();
    assert_eq!(w.era.id, empyrean_common::era::EraId::Eor);
    let m = request("Aldric", 1, ABILITIES, standard_sacs());
    let (_, p) = create(&mut w, &m);
    let loc = p.player.get_position(PositionType::Location).unwrap();
    assert_eq!(loc.landblock_id().raw(), sample::START_CELL.0);
    assert_eq!(recalls_disabled(&p), Some(true));
}

// ---- ACE's own values (the vector harness) -------------------------------------------------------------

#[test]
fn the_starter_gear_table_is_what_aces_deserializer_builds() {
    let file = vectors::load_named("chargen", "starter_gear");
    let config = starter_gear_factory::get_starter_gear_configuration().expect("config");
    assert_eq!(config.skills.len(), file.cases.len());
    let items = |gear: &[starter_gear_factory::StarterItem]| -> serde_json::Value {
        gear.iter()
            .map(|i| serde_json::json!({"stack_size": i.stack_size, "weenie_id": i.weenie_id}))
            .collect()
    };
    let spells = |spells: &[starter_gear_factory::StarterSpell]| -> serde_json::Value {
        spells
            .iter()
            .map(|s| serde_json::json!({"name": s.name, "specialized_only": s.specialized_only, "spell_id": s.spell_id}))
            .collect()
    };
    for (skill, case) in config.skills.iter().zip(&file.cases) {
        let heritage: serde_json::Value = skill
            .heritage
            .iter()
            .map(|h| serde_json::json!({"gear": items(h.gear), "heritage_id": h.heritage_id, "name": h.name, "spells": spells(h.spells)}))
            .collect();
        let ours = serde_json::json!({
            "gear": items(skill.gear),
            "heritage": heritage,
            "name": skill.name,
            "skill_id": skill.skill_id,
            "spells": spells(skill.spells),
        });
        // V292: ACE's record has weenie 0 for the two items its file keys
        // "weenieID" (Light Weapons: Gear Knight's Training Club, Tumerok's Training Spear); the
        // table carries their ids. Everything else is ACE's record exactly.
        let mut want = case.output.clone();
        if skill.skill_id == 45 {
            for (heritage_id, weenie_id) in [(6u64, 45542u64), (7, 45546)] {
                let h = want["heritage"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|h| h["heritage_id"] == heritage_id)
                    .unwrap();
                assert_eq!(
                    h["gear"][0]["weenie_id"], 0,
                    "ACE's record has the misspelt key's 0"
                );
                h["gear"][0]["weenie_id"] = weenie_id.into();
            }
        }
        assert_eq!(ours, want, "skill {}", skill.skill_id);
    }
    let zeros = config
        .skills
        .iter()
        .flat_map(|s| s.heritage.iter())
        .flat_map(|h| h.gear.iter())
        .filter(|i| i.weenie_id == 0)
        .count();
    assert_eq!(zeros, 0, "V292: no starter item loads as weenie 0");
}

#[test]
fn validate_attribute_credits_matches_ace() {
    let file = vectors::load_named("chargen", "validate_attribute_credits");
    assert!(file.cases.len() > 50);
    for case in &file.cases {
        let a: Vec<u32> = case.input["abilities"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| u32::try_from(v.as_u64().unwrap()).unwrap())
            .collect();
        let info = CharacterCreateInfo {
            strength_ability: a[0],
            endurance_ability: a[1],
            coordination_ability: a[2],
            quickness_ability: a[3],
            focus_ability: a[4],
            self_ability: a[5],
            ..Default::default()
        };
        let max = u32::try_from(case.input["max_attributes"].as_u64().unwrap()).unwrap();
        assert_eq!(
            player_factory::validate_attribute_credits(&info, max).name(),
            case.output.as_str().unwrap(),
            "{:?}",
            case.input
        );
    }
}

#[test]
fn get_masteries_matches_ace() {
    let file = vectors::load_named("chargen", "get_masteries");
    assert_eq!(file.cases.len(), 17);
    for case in &file.cases {
        let h = HeritageGroup(i32::try_from(case.input["heritage"].as_i64().unwrap()).unwrap());
        let (melee, ranged) = player_factory::get_masteries(h);
        assert_eq!(
            i64::from(melee.0),
            case.output["melee"].as_i64().unwrap(),
            "{h:?}"
        );
        assert_eq!(
            i64::from(ranged.0),
            case.output["ranged"].as_i64().unwrap(),
            "{h:?}"
        );
    }
}

#[test]
fn player_factory_ex_template_matches_ace() {
    let w = world();
    let file = vectors::load_named("chargen", "create_character_create_info");
    let u = |v: &serde_json::Value| u32::try_from(v.as_u64().unwrap()).unwrap();
    for case in &file.cases {
        let a: Vec<u32> = case.input["abilities"]
            .as_array()
            .unwrap()
            .iter()
            .map(u)
            .collect();
        let name = case.input["name"].as_str().unwrap();
        let info = player_factory_ex::create_character_create_info(
            &w, name, a[0], a[1], a[2], a[3], a[4], a[5], false,
        );
        let o = &case.output;
        let ap = &o["appearance"];
        assert_eq!(i64::from(info.heritage.0), o["heritage"].as_i64().unwrap());
        assert_eq!(info.gender, u(&o["gender"]));
        assert_eq!(info.name.as_deref(), o["name"].as_str());
        assert_eq!(
            i64::from(info.template_option),
            o["template_option"].as_i64().unwrap()
        );
        assert_eq!(
            (info.character_slot, info.class_id, info.start_area),
            (
                u(&o["character_slot"]),
                u(&o["class_id"]),
                u(&o["start_area"])
            )
        );
        assert_eq!(
            (info.is_admin, info.is_sentinel),
            (
                o["is_admin"].as_bool().unwrap(),
                o["is_sentinel"].as_bool().unwrap()
            )
        );
        let abilities = [
            info.strength_ability,
            info.endurance_ability,
            info.coordination_ability,
            info.quickness_ability,
            info.focus_ability,
            info.self_ability,
        ];
        assert_eq!(
            abilities.to_vec(),
            o["abilities"]
                .as_array()
                .unwrap()
                .iter()
                .map(u)
                .collect::<Vec<_>>()
        );
        let sacs: Vec<u32> = info.skill_advancement_classes.iter().map(|s| s.0).collect();
        assert_eq!(
            sacs,
            o["skill_advancement_classes"]
                .as_array()
                .unwrap()
                .iter()
                .map(u)
                .collect::<Vec<_>>()
        );
        let x = &info.appearance;
        let ours = [
            x.eyes,
            x.nose,
            x.mouth,
            x.hair_color,
            x.eye_color,
            x.hair_style,
            x.headgear_style,
            x.headgear_color,
            x.shirt_style,
            x.shirt_color,
            x.pants_style,
            x.pants_color,
            x.footwear_style,
            x.footwear_color,
        ];
        let keys = [
            "eyes",
            "nose",
            "mouth",
            "hair_color",
            "eye_color",
            "hair_style",
            "headgear_style",
            "headgear_color",
            "shirt_style",
            "shirt_color",
            "pants_style",
            "pants_color",
            "footwear_style",
            "footwear_color",
        ];
        assert_eq!(
            ours.to_vec(),
            keys.iter().map(|k| u(&ap[*k])).collect::<Vec<_>>()
        );
        let hues = [
            x.skin_hue,
            x.hair_hue,
            x.headgear_hue,
            x.shirt_hue,
            x.pants_hue,
            x.footwear_hue,
        ];
        let hue_keys = [
            "skin_hue",
            "hair_hue",
            "headgear_hue",
            "shirt_hue",
            "pants_hue",
            "footwear_hue",
        ];
        for (h, k) in hues.iter().zip(hue_keys) {
            assert!(
                vectors::same_f64(*h, vectors::f64_of(&ap[k]).unwrap()),
                "{k}"
            );
        }
    }
}

// ---- PlayerFactoryEx --------------------------------------------------------------------------------------

#[test]
fn player_factory_ex_levels_up_and_loads_the_default_spell_bars() {
    let mut w = world();
    let (_, mut p) = create(&mut w, &request("Aldric", 1, ABILITIES, standard_sacs()));
    player_factory_ex::level_up_player(&mut p.player);
    assert_eq!(p.player.get_property(PropertyInt::Level), Some(275));
    // 191,226,310,247 less: extra pack 4e9, carrying capacity 5 x 1e9, death item loss 3 x 2e9,
    // spells past death 4e9, spell duration 5 x 1e9 (an Aluvian already has Jack of All Trades).
    assert_eq!(
        p.player.available_experience(),
        Some(191_226_310_247 - 24_000_000_000)
    );
    assert_eq!(
        p.player.get_property(PropertyInt::AvailableSkillCredits),
        Some(23 + 46)
    );
    assert_eq!(
        p.player.get_property(PropertyInt::TotalSkillCredits),
        Some(50 + 46)
    );

    player_factory_ex::load_default_spell_bars(&mut p);
    let per_bar: Vec<usize> = (0..7)
        .map(|b| p.character().get_spells_in_bar(b).len())
        .collect();
    assert_eq!(per_bar, [20, 9, 37, 14, 40, 24, 24]);
    let bar0: Vec<u32> = p
        .character()
        .get_spells_in_bar(0)
        .iter()
        .map(|s| s.spell_id)
        .take(3)
        .collect();
    assert_eq!(bar0, [2645, 48, 157]);
}

// ---- real content (`--features real-content`) --------------------------------------------------------

/// The real-content tier: the retail dats under `DERETH_TEST_DAT_DIR`,
/// `world.pack` under `EMPYREAN_TEST_WORLD_PACK` (default `world.pack` in the repository) and the
/// recorded character creation in `fixtures/packet-captures/early-inventory-and-casting.jsonl`. Nothing from the capture is
/// written anywhere: the test decodes it at run time and compares structure, never names.
#[cfg(feature = "real-content")]
mod real_content {

    use empyrean_content::PackContent;
    use empyrean_dat::file_types::CellLandblock;
    use empyrean_dat::RealDats;

    use super::*;

    fn dats() -> Arc<DatManager> {
        let dir = dereth_dat::testing::dat_dir();
        let source = RealDats::open(&dir).unwrap_or_else(|e| {
            panic!(
                "the real-content tier needs the retail dats under {} (set DERETH_TEST_DAT_DIR): {e}",
                dir.display()
            )
        });
        DatManager::initialize(Arc::new(source)).expect("retail dats")
    }

    fn pack() -> PackContent {
        let path = empyrean_common::test_paths::world_pack();
        PackContent::open(&path).unwrap_or_else(|e| {
            panic!(
                "the real-content tier needs world.pack at {} (set EMPYREAN_TEST_WORLD_PACK): {e}",
                path.display()
            )
        })
    }

    fn real_world(account: &str) -> World {
        let now = ClockSnapshot {
            portal_year_ticks: 1000.0,
            unix_time: 1_767_225_600.0,
            utc: DotNetDateTime::new(2026, 1, 1),
            monotonic: Duration::ZERO,
        };
        let mut w = World::new(now, dats());
        w.content = Arc::new(pack());
        w.world_manager.world_status = WorldStatusState::Open;
        guid_manager::initialize(&mut w, &mut EmptyShard);
        let account_id = w
            .auth
            .lock()
            .create_account(
                account,
                "pw",
                AccessLevel::Player,
                std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            )
            .expect("created")
            .account_id;
        assert_eq!(account_id, ACCOUNT_ID);
        let mut s = SessionData {
            state: SessionState::AuthConnected,
            ..Default::default()
        };
        s.set_account(ACCOUNT_ID, account.to_owned(), AccessLevel::Player);
        w.sessions.insert(S, s);
        w
    }

    /// A cell id the cell dat can place: its landblock exists, and an indoor cell is an EnvCell of
    /// it (an outdoor cell is one of the 64).
    fn cell_is_valid(w: &World, cell: u32) -> bool {
        let landblock = w
            .dats
            .cell_dat()
            .read_from_dat::<CellLandblock>(cell | 0xFFFF)
            .is_some();
        let low = cell & 0xFFFF;
        landblock
            && if low >= 0x100 {
                w.dats
                    .cell_dat()
                    .read_from_dat::<dereth_assets::EnvCell>(cell)
                    .is_some()
            } else {
                (1..=64).contains(&low)
            }
    }

    #[test]
    fn every_heritage_creates_at_a_valid_start_with_its_starter_items() {
        let mut w = real_world(ACCOUNT);
        let char_gen = w.dats.portal_dat().char_gen().clone();
        assert!(
            !char_gen.heritage_groups.is_empty(),
            "{} heritages",
            char_gen.heritage_groups.len()
        );

        for area in &char_gen.starter_areas {
            for loc in &area.locations {
                assert!(
                    cell_is_valid(&w, loc.cell.0),
                    "start area {:?}: cell {:08X}",
                    area.name,
                    loc.cell.0
                );
            }
        }

        let mut created = 0;
        for (&heritage, group) in &char_gen.heritage_groups {
            let is_olthoi = heritage == 12 || heritage == 13;
            let weenie_name = match heritage {
                12 => "olthoiplayer",
                13 => "olthoiacidplayer",
                _ => "human",
            };
            let weenie = w
                .content
                .get_cached_weenie_by_class_name(weenie_name)
                .unwrap_or_else(|| panic!("{weenie_name} weenie"));
            let (&gender, _) = group.sexes.iter().next().expect("a sex");
            let mut m = request(
                "Realtest",
                heritage,
                [10; 6],
                sacs(&[(22, TRAINED), (24, TRAINED), (21, TRAINED)]),
            );
            m.result.gender = gender;
            m.result.start_area = group.primary_start_areas.first().copied().unwrap_or(0);
            let guid = guid_manager::new_player_guid(&mut w);
            let (result, p) = player_factory::create(
                &mut w,
                &info_of(&m),
                weenie,
                guid,
                ACCOUNT_ID,
                WeenieType::Creature,
            );
            assert_eq!(result, CreateResult::Success, "heritage {heritage}");

            let loc = p
                .player
                .get_position(PositionType::Location)
                .expect("location");
            assert!(
                cell_is_valid(&w, loc.landblock_id().raw()),
                "heritage {heritage}: {:08X}",
                loc.landblock_id().raw()
            );
            assert!(p.player.get_position(PositionType::Instantiation).is_some());
            if !is_olthoi {
                // Healing, Jump and Run gear exists in the pack, and so every item was created.
                let wcids: Vec<u32> = p
                    .inventory
                    .iter()
                    .map(|o| o.biota.weenie_class_id)
                    .collect();
                assert!(
                    wcids.contains(&628) && wcids.contains(&273),
                    "heritage {heritage}: {wcids:?}"
                );
                if heritage != 6 {
                    assert_eq!(
                        p.equipped_objects.len(),
                        4,
                        "heritage {heritage}: hat, shirt, pants, shoes"
                    );
                }
            }
            created += 1;
        }
        assert_eq!(created, char_gen.heritage_groups.len());
    }

    /// A character created in each starter area starts where the pack's era says: on the end of
    /// retail in the area's first location with recalls disabled; on an era with listed towns
    /// outdoors in the town's landblocks, on a cell the dats have, with recalls enabled.
    /// Divergence: V386
    #[test]
    fn a_character_created_in_each_town_starts_where_the_packs_era_says() {
        let mut w = real_world(ACCOUNT);
        w.era = w.content.era().rules();
        let char_gen = w.dats.portal_dat().char_gen().clone();
        let weenie = w
            .content
            .get_cached_weenie_by_class_name("human")
            .expect("human weenie");
        for (index, area) in char_gen.starter_areas.iter().enumerate() {
            let mut m = request(
                "Towntest",
                1,
                [10; 6],
                sacs(&[(22, TRAINED), (24, TRAINED), (21, TRAINED)]),
            );
            m.result.start_area = u32::try_from(index).unwrap();
            let guid = guid_manager::new_player_guid(&mut w);
            let (result, p) = player_factory::create(
                &mut w,
                &info_of(&m),
                Arc::clone(&weenie),
                guid,
                ACCOUNT_ID,
                WeenieType::Creature,
            );
            assert_eq!(result, CreateResult::Success, "{}", area.name);
            let loc = p.player.get_position(PositionType::Location).unwrap();
            let cell = loc.landblock_id().raw();
            assert!(cell_is_valid(&w, cell), "{}: {cell:08X}", area.name);
            match w.era.start_positions {
                empyrean_common::era::StartPositions::FromCharGen => {
                    assert_eq!(cell, area.locations[0].cell.0, "{}", area.name);
                }
                empyrean_common::era::StartPositions::Towns(towns) => {
                    let town = empyrean_common::era::StartPositions::town(towns, &area.name);
                    let landblocks = town.areas.map(|a| a.cell >> 16);
                    assert!(
                        landblocks.contains(&(cell >> 16)),
                        "{} starts in {cell:08X}, not in {}",
                        area.name,
                        town.town
                    );
                    assert_eq!(recalls_disabled(&p), None, "{}", area.name);
                    let inst = p.player.get_position(PositionType::Instantiation).unwrap();
                    assert_eq!(inst.landblock_id(), loc.landblock_id());
                    assert_eq!(inst.pos(), loc.pos());
                }
            }
        }
    }

    #[test]
    fn every_starter_item_and_chargen_garment_is_in_the_pack() {
        let w = real_world(ACCOUNT);
        let config = starter_gear_factory::get_starter_gear_configuration().unwrap();
        let mut ids: Vec<u32> = Vec::new();
        for s in config.skills {
            ids.extend(s.gear.iter().map(|i| i.weenie_id));
            ids.extend(
                s.heritage
                    .iter()
                    .flat_map(|h| h.gear.iter())
                    .map(|i| i.weenie_id),
            );
        }
        for group in w.dats.portal_dat().char_gen().heritage_groups.values() {
            for sex in group.sexes.values() {
                ids.extend(
                    sex.headgear
                        .iter()
                        .chain(&sex.shirts)
                        .chain(&sex.pants)
                        .chain(&sex.footwear)
                        .map(|g| g.weenie_default),
                );
            }
        }
        ids.sort_unstable();
        ids.dedup();
        let missing: Vec<u32> = ids
            .iter()
            .copied()
            .filter(|&id| w.content.get_cached_weenie(id).is_none())
            .collect();
        // V292: the two "weenieID" entries now carry their real ids (45542, 45546), which the pack has.
        assert!(
            missing.is_empty(),
            "missing {missing:?} of {} ids checked",
            ids.len()
        );
    }

    /// The recorded `Character_SendCharGenResult` payload of `early-inventory-and-casting` (opcode included).
    fn recorded_char_gen_payload() -> Vec<u8> {
        use dereth_client_net::client_session::testing::{Corpus, Direction};
        Corpus::shared("early-inventory-and-casting")
            .blobs
            .iter()
            .find(|blob| blob.dir == Direction::ClientToServer && blob.opcode == 0xF656)
            .expect("Character_SendCharGenResult in early-inventory-and-casting")
            .payload
            .clone()
    }

    #[test]
    fn the_recorded_creation_decodes_alike_and_creates_the_same_character() {
        let payload = recorded_char_gen_payload();
        let decoded: CharacterSendCharGenResult =
            proto::read_body_padded(&payload[4..]).expect("dereth-protocol decodes the capture");
        let r = &decoded.result;

        // ACE's reader on the same bytes agrees with the client's layout, field by field.
        let ace = info_of(&decoded);
        let mut reader = BinaryReader::new(&payload[4..]);
        let _ = reader.read_string16l().unwrap();
        let mut ace_raw = CharacterCreateInfo::default();
        ace_raw
            .unpack(&mut reader)
            .expect("ACE unpacks the capture");
        assert_eq!(
            ace_raw, ace,
            "re-encoding the decoded payload changes nothing ACE reads"
        );
        assert_eq!(u32::try_from(ace.heritage.0).unwrap(), r.heritage_group);
        assert_eq!(ace.gender, r.gender);
        assert_eq!(
            [
                ace.strength_ability,
                ace.endurance_ability,
                ace.coordination_ability,
                ace.quickness_ability,
                ace.focus_ability,
                ace.self_ability
            ],
            [
                r.strength,
                r.endurance,
                r.coordination,
                r.quickness,
                r.focus,
                r.self_
            ]
            .map(|v| u32::try_from(v).unwrap())
        );
        assert_eq!(
            ace.skill_advancement_classes
                .iter()
                .map(|s| s.0)
                .collect::<Vec<_>>(),
            r.skill_advancement_classes
                .iter()
                .map(|&v| u32::try_from(v).unwrap())
                .collect::<Vec<_>>()
        );
        assert_eq!(ace.start_area, r.start_area);
        assert!(
            ace.name.as_deref() == Some(r.name.as_str()),
            "the name reads the same (not printed)"
        );

        // Through the handler with the retail dats and the pack: created, and the saved character is
        // what the payload asked for.
        let mut w = real_world(&decoded.account);
        let sent = send_raw(&mut w, payload);
        let responses = sent.responses();
        assert_eq!(
            responses
                .iter()
                .map(|x| Cgvr(x.response_type))
                .collect::<Vec<_>>(),
            [Cgvr::Ok]
        );
        assert!(
            responses[0].identity.name == r.name,
            "the response names the character (not printed)"
        );
        let guid = responses[0].identity.gid.0;
        let b = saved_biota(&w, guid);

        assert_eq!(int(&b, PropertyInt::HeritageGroup), Some(ace.heritage.0));
        assert_eq!(
            int(&b, PropertyInt::Gender),
            Some(i32::try_from(r.gender).unwrap())
        );
        let attr = |a: PropertyAttribute| {
            b.properties_attribute
                .as_ref()
                .unwrap()
                .get(&a)
                .unwrap()
                .init_level
        };
        assert_eq!(
            [
                attr(PropertyAttribute::Strength),
                attr(PropertyAttribute::Endurance),
                attr(PropertyAttribute::Coordination),
                attr(PropertyAttribute::Quickness),
                attr(PropertyAttribute::Focus),
                attr(PropertyAttribute::Self_)
            ],
            [
                ace.strength_ability,
                ace.endurance_ability,
                ace.coordination_ability,
                ace.quickness_ability,
                ace.focus_ability,
                ace.self_ability
            ]
        );
        for (i, sac) in ace.skill_advancement_classes.iter().enumerate() {
            if *sac == SkillAdvancementClass::Inactive {
                continue;
            }
            let skill = Skill(i32::try_from(i).unwrap());
            assert_eq!(sac_of(&b, skill), Some(*sac), "skill {i}");
        }
        // Every credit spent: the recording's picks cost exactly its heritage's skill credits.
        assert_eq!(int(&b, PropertyInt::AvailableSkillCredits), Some(0));
        let area =
            &w.dats.portal_dat().char_gen().starter_areas[usize::try_from(r.start_area).unwrap()];
        let loc = b
            .properties_position
            .as_ref()
            .unwrap()
            .get(&PositionType::Location)
            .unwrap();
        assert_eq!(loc.obj_cell_id, area.locations[0].cell.0);
        let (inventory, wielded) = possessions(&w, guid);
        assert!(!inventory.is_empty());
        assert!(!wielded.is_empty() || r.heritage_group == 6);
    }
}

/// V243: a template number outside the heritage's list is answered
/// `Corrupt` at once and nothing is saved. ACE's unchecked index threw, and the client heard
/// nothing until its 110-second timeout.
#[test]
fn an_out_of_range_template_is_answered_corrupt() {
    for template in [-1i32, 99] {
        let mut w = world();
        let mut m = request("Aldric", 1, ABILITIES, standard_sacs());
        m.result.template_num = template;
        assert_eq!(
            send(&mut w, &m).codes(),
            [Cgvr::Corrupt],
            "template {template:#x}"
        );
        assert!(characters(&w).is_empty());
    }
}
