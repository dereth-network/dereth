//! ACE: Source/ACE.Server/Managers/RecipeManager.cs::RecipeManager
//! A simple recipe makes its product and consumes both; tinker success on a seeded roll, failure
//! destroys the target; salvaging yields ACE's amount in a bag.
//! Fixture: a virtual-time TestServer, isolated stores and synthetic dats.

use std::sync::Arc;

use dereth_primitives::ObjectId;
use dereth_protocol::comms::{CharacterConfirmationRequest, CommunicationTextboxString};
use dereth_protocol::events::split_ui_blob;
use dereth_protocol::items::{InventoryCreateTinkeringTool, SalvageResultMessage};
use dereth_protocol::objects::{ItemServerSaysContainId, ItemUseDone};
use dereth_protocol::Message;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_content::models::world::{CookBook, Recipe, RecipeMod, RecipeRequirementsInt, Weenie};
use empyrean_content::MemContent;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{
    CharacterOptions1, ConfirmationType, ItemType, MaterialType, PropertyAttribute, PropertyBool,
    PropertyDataId, PropertyInt, PropertyString, Skill, SkillAdvancementClass, Usable,
    WeenieClassName, WeenieError, WeenieType,
};
use empyrean_entity::models::properties_skill::PropertiesSkill;
use empyrean_entity::{ObjectGuid, Position};
use empyrean_net::{SessionId, SessionState};
use empyrean_testkit::land::{self, TEST_SETUP};
use empyrean_testkit::{ClientId, TestServer};
use empyrean_world::managers::guid_manager::{self, ShardGuidQueries};
use empyrean_world::managers::landblock_manager;
use empyrean_world::managers::recipe_manager as rm;
use empyrean_world::network::game_messages::game_message::{start_capture, take_sent};
use empyrean_world::world_objects::container;
use empyrean_world::world_objects::managers::confirmation_manager;
use empyrean_world::world_objects::world_object::CtorEnv;

const LB: u32 = 0xA9B4_0000;

const PLAYER_WCID: u32 = 1;
pub(crate) const FLOUR: u32 = 101;
pub(crate) const WATER: u32 = 102;
pub(crate) const DOUGH: u32 = 103;
pub(crate) const SWORD: u32 = 104;
const DOUGH_RECIPE: u32 = 7001;
const IRON_RECIPE: u32 = 3853;
pub(crate) const IRON_BAG: u32 = WeenieClassName::W_MATERIALIRON_CLASS.0 as u32; // 20986
const UST: u32 = WeenieClassName::W_TINKERINGTOOL_CLASS.0 as u32; // 20646
const STEEL_BAG: u32 = 20993; // Player.MaterialSalvage[Steel]

/// A pet device weenie for `CraftTool.HandleActionUseOnTarget`'s spirit refill.
pub(crate) const PET_DEVICE: u32 = 105;

/// A rare gem (`RareId`, `RareUsesTimer`) for the rare-gem confirmation.
pub(crate) const RARE_GEM: u32 = 106;

pub(crate) const ALPHA: u32 = 0x5000_0001;

// message kinds: the game-event type inside a 0xF7B0, else the opcode
pub(crate) const REMOVE: u32 = 0x0024;
pub(crate) const CONTAIN_ID: u32 = 0x0022;
pub(crate) const USE_DONE: u32 = 0x01C7;
pub(crate) const PRIVATE_INT: u32 = 0x02CD;
const PUBLIC_INT: u32 = 0x02CE;
const PUBLIC_STRING: u32 = 0x02D2;
pub(crate) const CREATE_OBJECT: u32 = 0xF745;
pub(crate) const UPDATE_OBJECT: u32 = 0xF7DB;
pub(crate) const MOTION: u32 = 0xF74C;
pub(crate) const CHAT: u32 = 0xF7E0;
pub(crate) const CONFIRM: u32 = 0x0274;
const SALVAGE_RESULT: u32 = 0x02B4;

fn weenie(wcid: u32, name: &str, weenie_type: WeenieType) -> Weenie {
    Weenie::new(wcid, name, weenie_type)
        .with_string(PropertyString::Name, name)
        .with_did(PropertyDataId::Setup, TEST_SETUP)
}

fn item_type(t: ItemType) -> i32 {
    i32::try_from(t.0).unwrap()
}

const SOURCE_CONTAINED_TARGET_CONTAINED: i32 = 0x0008_0008; // Usable.SourceContainedTargetContained

fn content() -> MemContent {
    let dough = Recipe {
        id: DOUGH_RECIPE,
        success_wcid: DOUGH,
        success_amount: 1,
        success_message: Some("You make some dough.".into()),
        fail_message: Some("You spill the water.".into()),
        success_destroy_source_chance: 1.0,
        success_destroy_source_amount: 1,
        success_destroy_target_chance: 1.0,
        success_destroy_target_amount: 1,
        ..Recipe::default()
    };
    // recipe 3853 as ACE's world database has it
    let req =
        |id: u32, index: i8, stat: i32, value: i32, e: i32, message: &str| RecipeRequirementsInt {
            id,
            recipe_id: IRON_RECIPE,
            index,
            stat,
            value,
            r#enum: e,
            message: Some(message.into()),
        };
    let iron = Recipe {
        id: IRON_RECIPE,
        skill: u32::try_from(Skill::WeaponTinkering.0).unwrap(),
        salvage_type: 1,
        success_message: Some("You apply the iron.".into()),
        fail_message: Some("You apply the iron, but in the process you destroy the target.".into()),
        success_destroy_source_chance: 1.0,
        success_destroy_source_amount: 1,
        success_destroy_target_message: Some(String::new()),
        success_destroy_source_message: Some(String::new()),
        fail_destroy_source_chance: 1.0,
        fail_destroy_source_amount: 1,
        fail_destroy_source_message: Some(String::new()),
        fail_destroy_target_chance: 1.0,
        fail_destroy_target_amount: 1,
        fail_destroy_target_message: Some(String::new()),
        recipe_mod: vec![
            RecipeMod {
                id: 1147,
                recipe_id: IRON_RECIPE,
                executes_on_success: true,
                data_id: 0x3800_001A,
                ..RecipeMod::default()
            },
            RecipeMod {
                id: 1148,
                recipe_id: IRON_RECIPE,
                executes_on_success: false,
                ..RecipeMod::default()
            },
        ],
        recipe_requirements_int: vec![
            req(
                364,
                0,
                i32::from(PropertyInt::ItemWorkmanship.0),
                1,
                2,
                "The target item cannot be tinkered!",
            ),
            req(
                365,
                0,
                i32::from(PropertyInt::NumTimesTinkered.0),
                10,
                3,
                "The target item has been tinkered too many times already!",
            ),
            req(
                366,
                1,
                i32::from(PropertyInt::Structure.0),
                100,
                2,
                "The material is not complete!",
            ),
        ],
        ..Recipe::default()
    };
    let bag = |wcid: u32, name: &str, material: MaterialType| {
        weenie(wcid, name, WeenieType::CraftTool)
            .with_int(
                PropertyInt::ItemType,
                item_type(ItemType::TinkeringMaterial),
            )
            .with_int(PropertyInt::ItemUseable, SOURCE_CONTAINED_TARGET_CONTAINED)
            .with_int(PropertyInt::MaxStructure, 100)
            .with_int(PropertyInt::Value, 10)
            .with_int(
                PropertyInt::MaterialType,
                i32::try_from(material.0).unwrap(),
            )
    };

    MemContent::new()
        .weenie(
            weenie(PLAYER_WCID, "human", WeenieType::Creature)
                .with_did(
                    empyrean_entity::enums::PropertyDataId::CombatTable,
                    0x3000_0000,
                )
                .with_int(PropertyInt::ItemsCapacity, 102)
                .with_int(PropertyInt::ContainersCapacity, 7),
        )
        .weenie(
            weenie(FLOUR, "Flour", WeenieType::Generic)
                .with_int(PropertyInt::ItemType, item_type(ItemType::Food))
                .with_int(PropertyInt::ItemUseable, SOURCE_CONTAINED_TARGET_CONTAINED)
                .with_int(PropertyInt::EncumbranceVal, 5),
        )
        .weenie(
            weenie(WATER, "Water", WeenieType::Generic)
                .with_int(PropertyInt::ItemType, item_type(ItemType::Food))
                .with_int(PropertyInt::EncumbranceVal, 10),
        )
        .weenie(
            weenie(DOUGH, "Dough", WeenieType::Generic)
                .with_int(PropertyInt::ItemType, item_type(ItemType::Food))
                .with_int(PropertyInt::EncumbranceVal, 12),
        )
        .weenie(
            weenie(SWORD, "Sword", WeenieType::MeleeWeapon)
                .with_int(PropertyInt::ItemType, item_type(ItemType::MeleeWeapon))
                .with_int(PropertyInt::Damage, 10)
                .with_int(PropertyInt::EncumbranceVal, 200)
                .with_int(PropertyInt::Value, 1000),
        )
        .weenie(bag(IRON_BAG, "Salvaged Iron", MaterialType::Iron))
        .weenie(bag(STEEL_BAG, "Salvage", MaterialType::Steel))
        // CraftTool.HandleActionUseOnTarget's special sources and targets (the world crate's vectors/world_object_leaves.rs)
        .weenie(weenie(49485, "Encapsulated Spirit", WeenieType::CraftTool))
        .weenie(weenie(PET_DEVICE, "Pet Device", WeenieType::PetDevice))
        .weenie(weenie(42645, "Aetheria Mana Stone", WeenieType::CraftTool))
        .weenie(weenie(42635, "Coalesced Aetheria", WeenieType::Gem))
        .weenie(weenie(
            42979,
            "Core Plating Integrator",
            WeenieType::CraftTool,
        ))
        .weenie(weenie(51445, "Weapon Tailoring Kit", WeenieType::Gem))
        .weenie(
            weenie(RARE_GEM, "Rare Gem", WeenieType::Gem)
                .with_int(PropertyInt::RareId, 1)
                .with_bool(PropertyBool::RareUsesTimer, true),
        )
        .weenie(
            weenie(UST, "Ust", WeenieType::CraftTool)
                .with_int(PropertyInt::ItemType, 0x2000_0000)
                .with_int(
                    PropertyInt::ItemUseable,
                    i32::try_from(Usable::Contained.0).unwrap(),
                ),
        )
        .cook_book(CookBook {
            id: 1,
            recipe_id: DOUGH_RECIPE,
            source_wcid: FLOUR,
            target_wcid: WATER,
            ..CookBook::default()
        })
        .recipe(dough)
        .recipe(iron)
}

/// `GuidManager.Initialize` over an empty shard.
struct EmptyShard;

impl ShardGuidQueries for EmptyShard {
    fn get_max_guid_found_in_range(&mut self, _min: u32, _max: u32) -> u32 {
        u32::MAX
    }
    fn get_sequence_gaps(&mut self, _min: u32, _limit: u32) -> Vec<(u32, u32)> {
        Vec::new()
    }
}

fn at(x: f32, y: f32) -> Position {
    Position::from_components(LB | 0x0001, x, y, 0.0, 0.0, 0.0, 0.0, 1.0, false)
}

pub(crate) fn server() -> TestServer {
    let mut ts = TestServer::with_setup(
        empyrean_testkit::dats::with_stat_tables(FakeDats::new())
            .build()
            .expect("fake dats"),
        |w| {
            w.content = Arc::new(content());
            guid_manager::initialize(w, &mut EmptyShard);
        },
    );
    land::use_flat_land_with_test_setup(&mut ts.world, &[0xA9B4], 0);
    ts
}

/// A client logged in as `account` whose session plays `guid` (Weapon Tinkering and Salvaging
/// trained at `skill`, the chance-of-success dialog on), as `vendors.rs` joins.
pub(crate) fn join(
    ts: &mut TestServer,
    account: &str,
    guid: u32,
    skill: u32,
) -> (ClientId, SessionId) {
    let before: Vec<SessionId> = ts.world.sessions.iter().map(|(id, _)| id).collect();
    let id = ts.connect(account, "pw");
    ts.advance(0.1);
    let session = ts
        .world
        .sessions
        .iter()
        .map(|(id, _)| id)
        .find(|s| !before.contains(s))
        .expect("the new session");

    let w = &mut ts.world;
    let weenie = w
        .content
        .get_cached_weenie(PLAYER_WCID)
        .expect("player weenie");
    let mut o = CtorEnv::with_world(w, |env| {
        empyrean_world::world_objects::player::player_from_weenie(
            env,
            empyrean_world::dispatch::Class::Player,
            weenie,
            ObjectGuid::new(guid),
            1,
        )
    });
    o.set_property(PropertyString::Name, "Alpha".to_owned());
    let rec = o
        .biota
        .properties_attribute
        .get_or_insert_with(Default::default)
        .get_or_insert_with(PropertyAttribute::Strength, Default::default);
    rec.init_level = 100;
    let skills = o
        .biota
        .properties_skill
        .get_or_insert_with(Default::default);
    for s in [Skill::WeaponTinkering, Skill::Salvaging] {
        skills.insert(
            s,
            PropertiesSkill {
                sac: SkillAdvancementClass::Trained,
                init_level: skill,
                ..PropertiesSkill::default()
            },
        );
    }
    o.set_encumbrance_val(Some(0));
    o.set_value(Some(0));
    let character = empyrean_store::models::shard::Character {
        character_options_1: CharacterOptions1::UseCraftSuccessDialog.0.cast_signed(),
        ..Default::default()
    };
    o.player.as_mut().expect("a player").player.character = Some(character);
    o.set_location(Some(at(20.0, 20.0)));
    w.objects.insert(o).expect("fresh");
    // `PlayerManager.AddPlayerToOnlinePlayers`: the Confirmation targets find the player there
    w.player_manager.online_players.insert(
        guid,
        empyrean_world::managers::player_manager::OnlinePlayer {
            guid: ObjectGuid::new(guid),
            account: None,
        },
    );

    let s = w.sessions.get_mut(session).expect("the game half");
    s.state = SessionState::WorldConnected;
    s.set_player(Some(ObjectGuid::new(guid)));
    assert!(
        landblock_manager::add_object(w, ObjectGuid::new(guid), false),
        "the player joins its landblock"
    );
    ts.advance(0.1);
    (id, session)
}

/// A new `wcid` in the player's main pack.
pub(crate) fn give(ts: &mut TestServer, wcid: u32) -> ObjectGuid {
    let w = &mut ts.world;
    let weenie = w.content.get_cached_weenie(wcid).expect("test weenie");
    let guid = guid_manager::new_dynamic_guid(w);
    let o = CtorEnv::with_world(w, |env| {
        empyrean_world::factories::world_object_factory::create_world_object(
            env,
            Some(weenie),
            guid,
        )
    })
    .expect("constructible");
    w.objects.insert(o).expect("fresh");
    assert!(container::try_add_to_inventory(
        w,
        ObjectGuid::new(ALPHA),
        guid,
        0,
        false,
        true
    ));
    guid
}

/// A received message: its kind and the whole blob.
#[derive(Debug, Clone)]
pub(crate) struct Got {
    pub(crate) kind: u32,
    blob: Vec<u8>,
}

impl Got {
    pub(crate) fn decode<M: Message>(&self) -> M {
        let split = split_ui_blob(&self.blob).expect("a blob");
        let mut body = split.body;
        M::read(&mut body).unwrap_or_else(|e| panic!("0x{:04X} decodes: {e:?}", self.kind))
    }
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
        .collect()
}

/// `PrivateUpdatePropertyInt(PropertyInt.Age)`, the player's own heartbeat, is left out.
fn is_age_update(blob: &[u8]) -> bool {
    blob.len() >= 9
        && u32::from_le_bytes(blob[0..4].try_into().unwrap()) == PRIVATE_INT
        && u32::from_le_bytes(blob[5..9].try_into().unwrap()) == u32::from(PropertyInt::Age.0)
}

/// What the world sent to `session` while `act` ran and the server advanced `secs` (ACE's send
/// order), and what the client received meanwhile. Everything sent must have arrived.
pub(crate) fn exchange(
    ts: &mut TestServer,
    id: ClientId,
    session: SessionId,
    secs: f64,
    act: impl FnOnce(&mut TestServer),
) -> (Vec<u32>, Vec<Got>) {
    let n = ts.received_raw(id).len();
    start_capture();
    act(ts);
    ts.advance(secs);
    let sent: Vec<u32> = take_sent()
        .into_iter()
        .filter(|(s, _, b)| *s == session && !is_age_update(b))
        .map(|(_, _, b)| {
            let word = |i: usize| u32::from_le_bytes(b[i..i + 4].try_into().unwrap());
            if word(0) == 0xF7B0 {
                word(12)
            } else {
                word(0)
            }
        })
        .collect();
    let received: Vec<Got> = got(ts, id, n)
        .into_iter()
        .filter(|m| !is_age_update(&m.blob))
        .collect();
    let (mut a, mut b) = (
        sent.clone(),
        received.iter().map(|g| g.kind).collect::<Vec<_>>(),
    );
    a.sort_unstable();
    b.sort_unstable();
    assert_eq!(a, b, "everything sent arrives: sent {sent:04X?}");
    (sent, received)
}

fn all(g: &[Got], kind: u32) -> Vec<&Got> {
    g.iter().filter(|m| m.kind == kind).collect()
}

pub(crate) fn first(g: &[Got], kind: u32) -> &Got {
    g.iter().find(|m| m.kind == kind).unwrap_or_else(|| {
        panic!(
            "no 0x{kind:04X} in {:04X?}",
            g.iter().map(|m| m.kind).collect::<Vec<_>>()
        )
    })
}

pub(crate) fn chats(g: &[Got]) -> Vec<String> {
    all(g, CHAT)
        .into_iter()
        .map(|m| m.decode::<CommunicationTextboxString>().text)
        .collect()
}

// ------------------------------------------------------------------ the scenarios

/// Flour on water, a cook-book recipe with no skill: `UseObjectOnTarget` claps (UpdateMotion), then
/// `HandleRecipe` rolls, destroys the target and then the source (each: remove, burden),
/// creates the dough (CreateObject, ContainId, burden), sends the recipe's success message, and
/// `UseDone`. Both ingredients are gone and the dough is in the pack.
#[test]
fn a_simple_recipe_makes_its_product_and_consumes_both_ingredients() {
    let mut ts = server();
    let (id, session) = join(&mut ts, "alpha", ALPHA, 100);
    let alpha = ObjectGuid::new(ALPHA);
    let flour = give(&mut ts, FLOUR);
    let water = give(&mut ts, WATER);
    ts.advance(0.1);

    let (sent, g) = exchange(&mut ts, id, session, 0.5, |ts| {
        rm::use_object_on_target(&mut ts.world, alpha, flour, water, false)
    });
    assert_eq!(
        sent,
        [
            MOTION,
            REMOVE,
            PRIVATE_INT,
            REMOVE,
            PRIVATE_INT,
            CREATE_OBJECT,
            CONTAIN_ID,
            PRIVATE_INT,
            CHAT,
            USE_DONE
        ],
        "{sent:04X?}"
    );
    assert_eq!(chats(&g), ["You make some dough."]);
    let made: ItemServerSaysContainId = first(&g, CONTAIN_ID).decode();
    let dough = ObjectGuid::new(made.item.0);
    assert_eq!(
        ts.world.objects.get(dough).map(|o| o.biota.weenie_class_id),
        Some(DOUGH)
    );
    let pack = container::inventory_values(&ts.world, alpha);
    assert!(
        pack.contains(&dough) && !pack.contains(&flour) && !pack.contains(&water),
        "the dough replaces both ingredients"
    );
    assert!(
        ts.world.objects.get(flour).is_none() && ts.world.objects.get(water).is_none(),
        "both ingredients are destroyed"
    );
    let done: ItemUseDone = first(&g, USE_DONE).decode();
    assert_eq!(done.failure_type, 0);
    assert!(
        !ts.world.objects.get(alpha).unwrap().wo.world_object.is_busy,
        "the player is free again"
    );

    // no recipe for these two: "cannot be used on", UseDone
    let bag = give(&mut ts, IRON_BAG);
    let dough2 = dough;
    let (sent, g) = exchange(&mut ts, id, session, 0.2, |ts| {
        rm::use_object_on_target(&mut ts.world, alpha, dough2, bag, false)
    });
    assert_eq!(sent, [CHAT, USE_DONE], "{sent:04X?}");
    assert_eq!(
        chats(&g),
        ["The Dough cannot be used on the Iron Salvaged ."]
    );
}

/// Iron on a sword (recipe 3853 through `GetNewRecipe`): the chance-of-success dialog first (clap,
/// ConfirmationRequest, UseDone); answered yes, the tinker runs confirmed (no clap). With a
/// skill far above the difficulty the chance is 1.0 and the seeded roll succeeds: the bag is
/// consumed, the iron script adds 1 Damage and a tinker, the tinker log gets Iron (61), the
/// tinker broadcast, and the sword's UpdateObject. With no skill the chance is tiny: the seeded
/// roll fails and the sword is destroyed with the bag ("fails to apply ... The target is
/// destroyed.").
#[test]
fn a_tinker_succeeds_on_a_seeded_roll_and_a_failed_one_destroys_the_target() {
    let mut ts = server();
    let (id, session) = join(&mut ts, "alpha", ALPHA, 10_000);
    let alpha = ObjectGuid::new(ALPHA);
    let sword = give(&mut ts, SWORD);
    let bag = give(&mut ts, IRON_BAG);
    for (g, iw, n) in [(sword, 6, None), (bag, 100, Some(10))] {
        let o = ts.world.objects.get_mut(g).unwrap();
        o.set_item_workmanship(Some(iw));
        o.set_num_items_in_material(n);
    }
    ts.world
        .objects
        .get_mut(bag)
        .unwrap()
        .set_structure(Some(100));
    ts.advance(0.1);

    // ---- the dialog: the clap, then ShowDialog: ConfirmationRequest (CraftInteraction), UseDone
    let (sent, g) = exchange(&mut ts, id, session, 0.3, |ts| {
        rm::use_object_on_target(&mut ts.world, alpha, bag, sword, false)
    });
    assert_eq!(sent, [MOTION, CONFIRM, USE_DONE], "{sent:04X?}");
    let ask: CharacterConfirmationRequest = first(&g, CONFIRM).decode();
    assert_eq!(
        ask.confirmation_type,
        i32::try_from(ConfirmationType::CraftInteraction.0).unwrap()
    );
    assert_eq!(
        ask.text,
        "You determine that you have a 100 percent chance to succeed."
    );

    // ---- yes: UseObjectOnTarget(confirmed): no clap; HandleRecipe draws the success roll (the
    // chance is exactly 1.0, so any draw succeeds), then CreateDestroyItems draws twice (target
    // kept, source destroyed: remove, burden), TryMutate draws once (Damage +1, NumTimesTinkered
    // 1, TinkerLog "61"), the local tinker broadcast, then UpdateObj(sword): UpdateObject. With the
    // dialog shown, no second UseDone.
    // (the draw order is the crafting/modify_item and try_mutate vectors'; the world tick draws too)
    ThreadSafeRandom::seed(5901);
    let (sent, g) = exchange(&mut ts, id, session, 0.3, |ts| {
        assert!(confirmation_manager::handle_response(
            &mut ts.world,
            alpha,
            ConfirmationType::CraftInteraction,
            ask.context_id,
            true,
            false
        ));
    });
    assert_eq!(
        sent,
        [REMOVE, PRIVATE_INT, CHAT, UPDATE_OBJECT],
        "{sent:04X?}"
    );
    assert_eq!(
        chats(&g),
        ["Alpha successfully applies the Iron Salvaged  (workmanship 10.00) to the Sword."]
    );
    let s = ts.world.objects.get(sword).expect("the sword is kept");
    assert_eq!(
        (
            s.damage(),
            s.num_times_tinkered(),
            s.tinker_log().as_deref()
        ),
        (Some(11), 1, Some("61"))
    );
    assert!(ts.world.objects.get(bag).is_none(), "the bag is used up");

    // ---- no skill: a new bag, the chance is about 0.0003, so the seeded roll fails
    ts.world
        .objects
        .get_mut(alpha)
        .unwrap()
        .biota
        .properties_skill
        .as_mut()
        .unwrap()
        .get_mut(&Skill::WeaponTinkering)
        .unwrap()
        .init_level = 0;
    let bag = give(&mut ts, IRON_BAG);
    {
        let o = ts.world.objects.get_mut(bag).unwrap();
        o.set_item_workmanship(Some(10));
        o.set_num_items_in_material(Some(10));
        o.set_structure(Some(100));
    }
    ts.advance(0.1);
    let (_, g) = exchange(&mut ts, id, session, 0.3, |ts| {
        rm::use_object_on_target(&mut ts.world, alpha, bag, sword, false)
    });
    let ask: CharacterConfirmationRequest = first(&g, CONFIRM).decode();
    assert_eq!(
        ask.text,
        "You determine that you have a 0 percent chance to succeed."
    );
    ThreadSafeRandom::seed(7);
    let (sent, g) = exchange(&mut ts, id, session, 0.3, |ts| {
        assert!(confirmation_manager::handle_response(
            &mut ts.world,
            alpha,
            ConfirmationType::CraftInteraction,
            ask.context_id,
            true,
            false
        ));
    });
    // the sword is destroyed (remove, burden), then the bag; no mods run on failure; the
    // broadcast names the destroyed sword; nothing is left to UpdateObject
    assert_eq!(
        sent,
        [REMOVE, PRIVATE_INT, REMOVE, PRIVATE_INT, CHAT],
        "{sent:04X?}"
    );
    assert_eq!(chats(&g), ["Alpha fails to apply the Iron Salvaged  (workmanship 1.00) to the Sword. The target is destroyed."]);
    assert!(
        ts.world.objects.get(sword).is_none() && ts.world.objects.get(bag).is_none(),
        "both are destroyed"
    );
    let _ = (PUBLIC_INT, PUBLIC_STRING);
}

/// Salvaging a sword with the Ust, through the game action: GetStructure takes the Salvaging
/// skill's `1 + floor(100 / 194 * 5)` = 3 units over the untrained tinkering yield; the sword is
/// consumed (remove, burden), a Steel bag is created (CreateObject, ContainId, burden) holding 3
/// units, workmanship 5, value 10 (the bag weenie's; GetSalvageBag keeps it) + round(1000 * 100 /
/// 387) = 268, named "Salvage (3)"; then one
/// SalvageOperationsResult (Salvaging: Steel, workmanship 5, 3 units, no augmentation bonus).
#[test]
fn salvaging_an_item_yields_aces_salvage_amount_in_a_bag() {
    let mut ts = server();
    let (id, session) = join(&mut ts, "alpha", ALPHA, 100);
    let alpha = ObjectGuid::new(ALPHA);
    // tinkering untrained: only Salvaging counts
    ts.world
        .objects
        .get_mut(alpha)
        .unwrap()
        .biota
        .properties_skill
        .as_mut()
        .unwrap()
        .get_mut(&Skill::WeaponTinkering)
        .unwrap()
        .sac = SkillAdvancementClass::Untrained;
    let ust = give(&mut ts, UST);
    let sword = give(&mut ts, SWORD);
    {
        let o = ts.world.objects.get_mut(sword).unwrap();
        o.set_item_workmanship(Some(5));
        o.set_property(
            PropertyInt::MaterialType,
            i32::try_from(MaterialType::Steel.0).unwrap(),
        );
    }
    ts.advance(0.1);

    let salvage = InventoryCreateTinkeringTool {
        tool: ObjectId(ust.full()),
        items: vec![ObjectId(sword.full())],
    };
    let (sent, g) = exchange(&mut ts, id, session, 0.3, |ts| {
        ts.send_game_action(id, &salvage)
    });
    assert_eq!(
        sent,
        [
            REMOVE,
            PRIVATE_INT,
            CREATE_OBJECT,
            CONTAIN_ID,
            PRIVATE_INT,
            SALVAGE_RESULT
        ],
        "{sent:04X?}"
    );
    let made: ItemServerSaysContainId = first(&g, CONTAIN_ID).decode();
    let bag = ObjectGuid::new(made.item.0);
    let b = ts.world.objects.get(bag).expect("the bag");
    assert_eq!(b.biota.weenie_class_id, STEEL_BAG);
    assert_eq!(
        (
            b.structure(),
            b.item_workmanship(),
            b.num_items_in_material(),
            b.value()
        ),
        (Some(3), Some(5), Some(1), Some(268))
    );
    assert_eq!(
        b.get_property(PropertyString::Name).as_deref(),
        Some("Salvage (3)")
    );
    assert!(
        container::inventory_values(&ts.world, alpha).contains(&bag)
            && ts.world.objects.get(sword).is_none()
    );
    let result: SalvageResultMessage = first(&g, SALVAGE_RESULT).decode();
    assert_eq!(
        result.skill_used,
        u32::try_from(Skill::Salvaging.0).unwrap()
    );
    assert!(result.not_salvagable.is_empty());
    assert_eq!(result.results.len(), 1);
    assert_eq!(
        (
            result.results[0].material,
            result.results[0].workmanship,
            result.results[0].units
        ),
        (MaterialType::Steel.0, 5.0, 3)
    );
    assert_eq!(result.aug_bonus, 0);

    // a tool that is not a Ust: NotASalvageTool, nothing salvaged
    let other = give(&mut ts, SWORD);
    let wrong = InventoryCreateTinkeringTool {
        tool: ObjectId(sword.full()),
        items: vec![ObjectId(other.full())],
    };
    let (sent, g) = exchange(&mut ts, id, session, 0.2, |ts| {
        ts.send_game_action(id, &wrong)
    });
    assert_eq!(sent, [0x028A], "{sent:04X?}");
    let err: dereth_protocol::comms::CommunicationWeenieError = first(&g, 0x028A).decode();
    assert_eq!(
        err.error_type,
        u32::try_from(WeenieError::NotASalvageTool.0).unwrap()
    );
    assert!(ts.world.objects.get(other).is_some());
}

/// Divergence: V421
/// A world without tinkering refuses salvaging with the Ust (the item is kept and no bag made) and
/// a tinkering recipe (the bag and the sword are kept, nothing rolls): each tells the player
/// why, and the recipe answers `UseDone`.
#[test]
fn a_world_without_tinkering_refuses_salvaging_and_tinkering_recipes() {
    use empyrean_common::era::{with_features, EraExt as _, EraFeatures, EraId};
    let mut ts = server();
    ts.world.era = with_features(
        EraId::Eor.rules(),
        EraFeatures {
            tinkering: false,
            ..EraId::Eor.features()
        },
    );
    let (id, session) = join(&mut ts, "alpha", ALPHA, 10_000);
    let alpha = ObjectGuid::new(ALPHA);
    let ust = give(&mut ts, UST);
    let sword = give(&mut ts, SWORD);
    {
        let o = ts.world.objects.get_mut(sword).unwrap();
        o.set_item_workmanship(Some(5));
        o.set_property(
            PropertyInt::MaterialType,
            i32::try_from(MaterialType::Steel.0).unwrap(),
        );
    }
    let bag = give(&mut ts, IRON_BAG);
    {
        let o = ts.world.objects.get_mut(bag).unwrap();
        o.set_item_workmanship(Some(10));
        o.set_num_items_in_material(Some(10));
        o.set_structure(Some(100));
    }
    ts.advance(0.1);

    let salvage = InventoryCreateTinkeringTool {
        tool: ObjectId(ust.full()),
        items: vec![ObjectId(sword.full())],
    };
    let (sent, g) = exchange(&mut ts, id, session, 0.3, |ts| {
        ts.send_game_action(id, &salvage)
    });
    assert_eq!(sent, [CHAT], "{sent:04X?}");
    assert_eq!(chats(&g), ["This world has no tinkering."]);

    let (sent, g) = exchange(&mut ts, id, session, 0.3, |ts| {
        rm::use_object_on_target(&mut ts.world, alpha, bag, sword, false)
    });
    assert_eq!(sent, [CHAT, USE_DONE], "{sent:04X?}");
    assert_eq!(chats(&g), ["This world has no tinkering."]);
    assert!(ts.world.objects.get(sword).is_some() && ts.world.objects.get(bag).is_some());
    assert_eq!(ts.world.objects.get(sword).unwrap().num_times_tinkered(), 0);

    // February 2005 had tinkering: the same recipe asks for its confirmation.
    ts.world.era = EraId::Infiltration.rules();
    let (sent, _) = exchange(&mut ts, id, session, 0.3, |ts| {
        rm::use_object_on_target(&mut ts.world, alpha, bag, sword, false)
    });
    assert_eq!(sent, [MOTION, CONFIRM, USE_DONE], "{sent:04X?}");
}
