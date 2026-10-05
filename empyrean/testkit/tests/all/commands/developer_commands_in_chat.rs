//! ACE: Source/ACE.Server/Command/Handlers/DeveloperCommands.cs::DeveloperCommands
//! Developer chat commands: whoami/gps/echo/chatdump, tele* variants, vitals tweaks/harm/rip,
//! player flags/coins/comps/faction, appraised-monster inspect/edit, inventory create,
//! forcelogoff, object dumps to console.
//! Fixture: a virtual-time TestServer, isolated stores and synthetic dats.

use std::sync::Arc;

use dereth_assets::tables::SkillFormula;
use dereth_primitives::NetQueue;
use dereth_primitives::ObjectId;
use dereth_protocol::comms::{CommunicationTalk, CommunicationTextboxString};
use dereth_protocol::login::{
    CharacterLoginCompleteNotification, LoginCharacterSet, LoginSendEnterWorld,
    LoginSendEnterWorldRequest,
};
use dereth_protocol::objects::{EffectsPlayerTeleport, ItemCreateObject};
use empyrean_command::command_manager;
use empyrean_common::dotnet::{format, to_string, DotNetDict};
use empyrean_content::models::world::Weenie;
use empyrean_content::MemContent;
use empyrean_dat::file_types::SecondaryAttributeTable;
use empyrean_dat::{file_id, FakeDats};
use empyrean_entity::enums::{
    ChatMessageType, FactionBits, PositionType, PropertyAttribute, PropertyAttribute2nd,
    PropertyBool, PropertyDataId, PropertyInt, PropertyString, WeenieType,
};
use empyrean_entity::models::{PropertiesAttribute, PropertiesAttribute2nd};
use empyrean_entity::{ObjectGuid, Position};
use empyrean_testkit::{land, ClientId, TestServer};
use empyrean_world::dispatch::Class;
use empyrean_world::entity::damage_history::DamageHistory;
use empyrean_world::managers::guid_manager;
use empyrean_world::managers::quest_manager::{self, QuestOwner};
use empyrean_world::managers::{landblock_manager as lm, player_manager};
use empyrean_world::world_objects::entity::creature_attribute::{CreatureAttribute, StatCtx};
use empyrean_world::world_objects::entity::creature_vital::CreatureVital;
use empyrean_world::world_objects::world_object::WorldObject;
use empyrean_world::world_objects::{container, world_object_tick};

const HOME: u16 = 0xA9B4;
const ALPHA: u32 = 0x5000_0001;
const BRAVO: u32 = 0x5000_0002;
const MONSTER: ObjectGuid = ObjectGuid::new(0x8000_0100);
const PYREAL_WCID: u32 = 273;
const PORTAL_WCID: u32 = 3002;
const PARCHMENT_WCID: u32 = 365;

fn outdoor(x: f32, y: f32, z: f32) -> Position {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let cell = (x / 24.0) as u32 * 8 + (y / 24.0) as u32 + 1;
    Position::from_components(
        u32::from(HOME) << 16 | cell,
        x,
        y,
        z,
        0.0,
        0.0,
        0.0,
        1.0,
        false,
    )
}

fn dats() -> Arc<empyrean_dat::DatManager> {
    let f = |attr1: u32, z: u32| SkillFormula {
        w: 0,
        x: 1,
        y: 0,
        z,
        attr1,
        attr2: 0,
    };
    let table = SecondaryAttributeTable {
        id: dereth_primitives::DataId(file_id::SECONDARY_ATTRIBUTE_TABLE),
        health: f(2, 2),
        stamina: f(2, 1),
        mana: f(6, 1),
    };
    empyrean_testkit::dats::with_stat_tables(FakeDats::new())
        .with_portal(file_id::SECONDARY_ATTRIBUTE_TABLE, table)
        .with_xp_table(empyrean_dat::fake::sample::xp_table())
        .build()
        .expect("fake dats")
}

fn content() -> MemContent {
    MemContent::new()
        .weenie(Weenie::new(1, "human", WeenieType::Creature).with_did(
            empyrean_entity::enums::PropertyDataId::CombatTable,
            0x3000_0000,
        ))
        .weenie(
            Weenie::new(PYREAL_WCID, "u63pyreal", WeenieType::Coin)
                .with_string(PropertyString::Name, "Pyreal")
                .with_string(PropertyString::PluralName, "Pyreals")
                .with_did(PropertyDataId::Setup, land::TEST_SETUP)
                .with_int(PropertyInt::MaxStackSize, 25000)
                .with_int(PropertyInt::StackSize, 1)
                .with_int(PropertyInt::Value, 1)
                .with_int(PropertyInt::EncumbranceVal, 0),
        )
        .weenie(
            Weenie::new(PARCHMENT_WCID, "parchment", WeenieType::Book)
                .with_string(PropertyString::Name, "Parchment")
                .with_did(PropertyDataId::Setup, land::TEST_SETUP),
        )
        .weenie(
            Weenie::new(PORTAL_WCID, "u63portal", WeenieType::Portal)
                .with_string(PropertyString::Name, "Portal to Market")
                .with_position(
                    PositionType::Destination,
                    u32::from(HOME) << 16 | 0x0020,
                    [60.0, 170.0, 20.0],
                    [1.0, 0.0, 0.0, 0.0],
                ),
        )
}

fn seed(ts: &TestServer, account: &str, guid: u32, name: &str, at: Position) -> u32 {
    crate::support::command_characters::seed(ts, account, guid, name, at, true)
}

/// The server with "Alpha Admin" and "Bravo Player" in the shard, and the command table; the two
/// account ids.
fn server() -> (TestServer, u32, u32) {
    let mut ts = TestServer::with_dats(dats());
    land::use_flat_land_with_test_setup(&mut ts.world, &[HOME], 10);
    ts.world.content = Arc::new(content());
    guid_manager::initialize(&mut ts.world, &mut EmptyShard);
    let a = seed(
        &ts,
        "u63alpha",
        ALPHA,
        "Alpha Admin",
        outdoor(100.0, 100.0, 20.0),
    );
    let b = seed(
        &ts,
        "u63bravo",
        BRAVO,
        "Bravo Player",
        outdoor(150.0, 60.0, 20.0),
    );
    player_manager::initialize(&mut ts.world);
    command_manager::initialize(None);
    (ts, a, b)
}

/// Logs `account` in and enters the world with `guid`, out of the login pink bubble.
fn enter(ts: &mut TestServer, account: &str, guid: u32) -> ClientId {
    let id = ts.connect(account, "pw");
    assert!(
        ts.run_until(1.0, |ts| !ts.received::<LoginCharacterSet>(id).is_empty()),
        "the character list arrives"
    );
    ts.send_message(id, NetQueue::Logon, &LoginSendEnterWorldRequest);
    ts.advance(0.1);
    ts.send_message(
        id,
        NetQueue::Logon,
        &LoginSendEnterWorld {
            character: ObjectId(guid),
            account: account.to_owned(),
        },
    );
    let g = ObjectGuid::new(guid);
    assert!(
        ts.run_until(1.0, |ts| ts
            .world
            .objects
            .get(g)
            .is_some_and(|o| o.current_landblock.is_some())),
        "{account} enters the world"
    );
    ts.send_game_action(id, &CharacterLoginCompleteNotification);
    ts.advance(0.1);
    id
}

/// The server with Alpha (an admin) and Bravo (a player) in the world, and their account ids.
fn two_players() -> (TestServer, ClientId, ClientId, u32, u32) {
    let (mut ts, a, b) = server();
    let alpha = enter(&mut ts, "u63alpha", ALPHA);
    let bravo = enter(&mut ts, "u63bravo", BRAVO);
    ts.world
        .objects
        .get_mut(ObjectGuid::new(ALPHA))
        .unwrap()
        .set_is_admin_prop(true);
    let _ = TestServer::take_not_ported();
    (ts, alpha, bravo, a, b)
}

/// Types `line` in chat and returns the chat lines the client received for it, with their types.
fn say(ts: &mut TestServer, id: ClientId, line: &str) -> Vec<(String, u32)> {
    let before = ts.received::<CommunicationTextboxString>(id).len();
    ts.send_game_action(
        id,
        &CommunicationTalk {
            message: line.to_owned(),
        },
    );
    ts.advance(0.5);
    ts.received::<CommunicationTextboxString>(id)[before..]
        .iter()
        .map(|m| (m.text.clone(), m.text_type))
        .collect()
}

fn broadcast(text: &str) -> (String, u32) {
    (text.to_owned(), ChatMessageType::Broadcast.0)
}

fn system(text: &str) -> (String, u32) {
    (text.to_owned(), ChatMessageType::System.0)
}

fn location(ts: &TestServer, guid: ObjectGuid) -> Position {
    ts.world
        .objects
        .get(guid)
        .and_then(WorldObject::location)
        .expect("a location")
}

fn me(ts: &TestServer) -> &WorldObject {
    ts.world.objects.get(ObjectGuid::new(ALPHA)).unwrap()
}

/// A creature as its constructor leaves it (100 in each vital, 60 in each attribute), standing
/// beside `(x, y)`.
fn monster(ts: &mut TestServer, x: f32, y: f32) {
    let w = &mut ts.world;
    let mut o = WorldObject::allocate(Class::Creature);
    o.guid = MONSTER;
    o.biota.id = MONSTER.full();
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
    o.set_property(PropertyString::Name, "Drudge".to_owned());
    o.set_property(PropertyDataId::Setup, land::TEST_SETUP);
    o.set_property(PropertyBool::Attackable, true);
    o.creature.as_mut().unwrap().creature_death.damage_history =
        DamageHistory::new(MONSTER, w.now.utc);
    o.set_location(Some(outdoor(x, y, 20.0)));
    o.set_heartbeat_interval(Some(0.0));
    world_object_tick::world_object_initialize_heartbeats(&mut o, w.now.unix_time);
    w.objects.insert(o).expect("fresh guid");
    assert!(
        lm::add_object(w, MONSTER, false),
        "the monster joins its landblock"
    );
}

/// The admin appraises the monster (what `GetLastAppraisedObject` reads).
fn appraise_monster(ts: &mut TestServer) {
    ts.world
        .objects
        .get_mut(ObjectGuid::new(ALPHA))
        .unwrap()
        .set_requested_appraisal_target(Some(MONSTER.full()));
}

#[test]
fn whoami_gps_echo_and_chatdump() {
    let (mut ts, alpha, _, _, _) = two_players();

    assert_eq!(
        say(&mut ts, alpha, "@whoami"),
        [broadcast(
            "GUID: 1342177281 (0x50000001) | ID(low): 1 High:80"
        )]
    );

    let l = location(&ts, ObjectGuid::new(ALPHA));
    let gps = format!(
        "Position: [Cell: 0x{} | Offset: {}, {}, {} | Facing: {}, {}, {}, {}]",
        format(u16::try_from(l.cell() >> 16).unwrap(), "X4"),
        to_string(l.position_x),
        to_string(l.position_y),
        to_string(l.position_z),
        to_string(l.rotation_x),
        to_string(l.rotation_y),
        to_string(l.rotation_z),
        to_string(l.rotation_w)
    );
    assert_eq!(say(&mut ts, alpha, "@gps"), [broadcast(&gps)]);

    // echo: the text in the chosen type; one parameter falls into the catch (Broadcast); a number
    // that is no ChatMessageType is Broadcast; a name that does not parse sends nothing
    assert_eq!(
        say(&mut ts, alpha, "@echo \"hello there\" tell"),
        [("hello there".to_owned(), ChatMessageType::Tell.0)]
    );
    assert_eq!(say(&mut ts, alpha, "@echo hello"), [broadcast("hello")]);
    assert_eq!(
        say(&mut ts, alpha, "@echo hello 12345"),
        [broadcast("hello")]
    );
    assert_eq!(say(&mut ts, alpha, "@echo hello nosuchtype"), []);

    let lines = say(&mut ts, alpha, "@chatdump");
    assert_eq!(lines.len(), 1000);
    assert_eq!(lines[0], broadcast("Test Message 0"));
    assert_eq!(lines[999], broadcast("Test Message 999"));

    // listplayers in game: every online player with its account id
    let text = say(&mut ts, alpha, "@listplayers");
    assert_eq!(text.len(), 1);
    assert!(
        text[0].0.ends_with("Total connected Players: 2\n"),
        "{:?}",
        text[0].0
    );
    assert!(
        text[0].0.contains("Alpha Admin : ") && text[0].0.contains("Bravo Player : "),
        "{:?}",
        text[0].0
    );
    assert_eq!(
        say(&mut ts, alpha, "@listplayers Admin"),
        [broadcast(
            "Listing only Admins:\nTotal connected Players: 0\n"
        )]
    );
    assert_eq!(
        say(&mut ts, alpha, "@listplayers nosuch"),
        [broadcast("Invalid AccessLevel value")]
    );
}

#[test]
fn telexyz_setposition_teletype_and_teledungeon() {
    let (mut ts, alpha, _, _, _) = two_players();
    let me_guid = ObjectGuid::new(ALPHA);

    // telexyz: the cell in decimal, then seven floats; any bad number is silence
    assert_eq!(say(&mut ts, alpha, "@telexyz x 1 2 3 0 0 0 1"), []);
    assert_eq!(say(&mut ts, alpha, "@telexyz 1 2 3 4 5 6 7 y"), []);
    assert!(ts.received::<EffectsPlayerTeleport>(alpha).is_empty());
    let cell = u32::from(HOME) << 16 | 0x0011;
    assert_eq!(
        say(&mut ts, alpha, &format!("@telexyz {cell} 60 60 20 0 0 0 1")),
        []
    );
    assert_eq!(ts.received::<EffectsPlayerTeleport>(alpha).len(), 1);
    let l = location(&ts, me_guid);
    assert_eq!(
        (l.cell() >> 16, l.position_x, l.position_y),
        (u32::from(HOME), 60.0, 60.0)
    );
    ts.send_game_action(alpha, &CharacterLoginCompleteNotification);
    ts.advance(0.1);

    // setposition saves the current location under a position type
    let here = location(&ts, me_guid);
    assert_eq!(
        say(&mut ts, alpha, "@setposition Sanctuary"),
        [broadcast(&format!("Set: Sanctuary to Loc: {here}"))]
    );
    assert_eq!(
        me(&ts)
            .get_position(PositionType::Sanctuary)
            .map(|p| p.cell()),
        Some(here.cell())
    );
    assert_eq!(say(&mut ts, alpha, "@setposition Undef"), [broadcast(
        "Could not determine the correct position type.\nPlease supply a single integer value from within the range of 1 through 27."
    )]);
    let listed = say(&mut ts, alpha, "@listpositions");
    assert_eq!(listed.len(), 1);
    let sanctuary = format!("ID: {} Loc: {here}\n", PositionType::Sanctuary.0);
    assert!(
        listed[0].0.starts_with("Saved character positions:\n") && listed[0].0.contains(&sanctuary),
        "{:?}",
        listed[0].0
    );

    // teletype: the first three characters parsed (`san` is no PositionType; the number is)
    assert_eq!(say(&mut ts, alpha, "@teletype sanctuary"), []);
    let teleports = ts.received::<EffectsPlayerTeleport>(alpha).len();
    // V360/V362 (a fix): the reply names the saved position, not where the player still stands
    let sanctuary = outdoor(30.0, 40.0, 20.0);
    ts.world
        .objects
        .get_mut(me_guid)
        .unwrap()
        .set_position(PositionType::Sanctuary, Some(sanctuary));
    let reply = say(
        &mut ts,
        alpha,
        &format!("@teletype {}", PositionType::Sanctuary.0),
    );
    assert_eq!(reply, [broadcast(&format!("Location {sanctuary}"))]);
    assert_ne!(
        sanctuary.cell(),
        here.cell(),
        "(the player stood elsewhere)"
    );
    assert_eq!(
        ts.received::<EffectsPlayerTeleport>(alpha).len(),
        teleports + 1
    );
    assert_eq!(
        say(
            &mut ts,
            alpha,
            &format!("@teletype {}", PositionType::LastPortal.0)
        ),
        [broadcast(
            "Error finding saved character position: LastPortal"
        )]
    );
    ts.send_game_action(alpha, &CharacterLoginCompleteNotification);
    ts.advance(0.1);

    // teledungeon: by landblock (hex), then by name: the first portal whose Destination leads there
    assert_eq!(
        say(&mut ts, alpha, "@teledungeon ABCD"),
        [broadcast("Couldn't find dungeon ABCD")]
    );
    assert_eq!(
        say(&mut ts, alpha, "@teledungeon no such place"),
        [broadcast("Couldn't find dungeon name no such place")]
    );
    let teleports = ts.received::<EffectsPlayerTeleport>(alpha).len();
    assert_eq!(say(&mut ts, alpha, &format!("@teledungeon {HOME:X}")), []);
    assert_eq!(
        ts.received::<EffectsPlayerTeleport>(alpha).len(),
        teleports + 1
    );
    let l = location(&ts, me_guid);
    assert_eq!(
        (l.cell() >> 16, l.position_x, l.position_y),
        (u32::from(HOME), 60.0, 170.0)
    );
    ts.send_game_action(alpha, &CharacterLoginCompleteNotification);
    ts.advance(0.1);
    assert_eq!(say(&mut ts, alpha, "@teledungeon to MARKET"), []);
    assert_eq!(
        ts.received::<EffectsPlayerTeleport>(alpha).len(),
        teleports + 2
    );

    // dungeonname: the portal names whose Destination is in this landblock, trimmed
    assert_eq!(say(&mut ts, alpha, "@dungeonname"), [broadcast("Market")]);
}

#[test]
fn vitals_tweaks_set_and_harm_and_rip() {
    let (mut ts, alpha, _, _, _) = two_players();
    let me_guid = ObjectGuid::new(ALPHA);
    let health = me(&ts).health();
    let current = |ts: &TestServer, v: fn(&WorldObject) -> CreatureVital| {
        let o = me(ts);
        v(o).current(o)
    };

    assert_eq!(
        say(&mut ts, alpha, "@sethealth 50"),
        [broadcast("Attempting to set health to 50...")]
    );
    assert_eq!(current(&ts, WorldObject::health), 50);
    assert_eq!(
        say(&mut ts, alpha, "@sethealth -5"),
        [broadcast("Usage: /sethealth 200 (max Max Health)")]
    );

    assert_eq!(say(&mut ts, alpha, "@setvital hp +10"), []);
    assert_eq!(current(&ts, WorldObject::health), 60);
    assert_eq!(say(&mut ts, alpha, "@setvital stam 5"), []);
    assert_eq!(current(&ts, WorldObject::stamina), 5);
    assert_eq!(
        say(&mut ts, alpha, "@setvital xx 5"),
        [broadcast("setvital Error: Invalid vital")]
    );
    assert_eq!(
        say(&mut ts, alpha, "@setvital hp 1x"),
        [broadcast("setvital Error: Invalid set value")]
    );

    assert_eq!(say(&mut ts, alpha, "@harmself"), []);
    assert_eq!(
        (
            current(&ts, WorldObject::health),
            current(&ts, WorldObject::stamina),
            current(&ts, WorldObject::mana)
        ),
        (1, 1, 1)
    );

    // rip: TakeDamage for the current health
    let max = health.max_value(&mut StatCtx::in_world(&mut ts.world, me_guid));
    assert!(max > 1);
    let _ = say(&mut ts, alpha, "@rip");
    assert_eq!(current(&ts, WorldObject::health), 0, "dead");
}

#[test]
fn player_flags_coins_comps_faction_and_toggles() {
    let (mut ts, alpha, _, _, _) = two_players();
    let me_guid = ObjectGuid::new(ALPHA);

    assert_eq!(say(&mut ts, alpha, "@setcoin 12345"), []);
    assert_eq!(me(&ts).coin_value(), Some(12345));
    assert_eq!(
        say(&mut ts, alpha, "@setcoin lots"),
        [broadcast(
            "Not a valid number - must be a number between 0 - 2,147,483,647"
        )]
    );

    assert_eq!(
        say(&mut ts, alpha, "@safecomps OFF"),
        [broadcast(
            "Your spell components will now be consumed when casting spells."
        )]
    );
    assert!(!me(&ts).safe_spell_components());
    assert_eq!(
        say(&mut ts, alpha, "@safecomps"),
        [broadcast(
            "Your spell components are now safe, and will not be consumed when casting spells."
        )]
    );
    assert!(me(&ts).safe_spell_components());

    assert_eq!(
        say(&mut ts, alpha, "@requirecomps off"),
        [broadcast("You can now cast spells without components.")]
    );
    assert!(!me(&ts).spell_components_required());
    // (anything but "off" turns them on)
    assert_eq!(
        say(&mut ts, alpha, "@requirecomps OFF"),
        [broadcast(
            "You can no longer cast spells without components."
        )]
    );
    assert!(me(&ts).spell_components_required());

    assert_eq!(
        say(&mut ts, alpha, "@debugspell"),
        [broadcast("Spell projectile debugging is enabled")]
    );
    assert_eq!(
        say(&mut ts, alpha, "@debugspell On"),
        [broadcast("Spell projectile debugging is enabled")]
    );
    assert_eq!(
        say(&mut ts, alpha, "@debugspell whatever"),
        [broadcast("Spell projectile debugging is disabled")]
    );
    assert_eq!(
        say(&mut ts, alpha, "@recordcast"),
        [broadcast("Record cast enabled")]
    );
    assert_eq!(
        say(&mut ts, alpha, "@recordcast off"),
        [broadcast("Record cast disabled")]
    );
    assert_eq!(
        say(&mut ts, alpha, "@pktimer"),
        [broadcast("Updated PK timer")]
    );

    assert!(!me(&ts).barber_active());
    let _ = say(&mut ts, alpha, "@barbershop");
    assert!(me(&ts).barber_active());

    // faction: society, rank and the quest flags; the rank's title in the reply
    let reply = say(&mut ts, alpha, "@faction ch 3");
    assert_eq!(
        reply,
        [broadcast(
            "Your current Faction state is now set to: Celestial Hand with a rank of Knight"
        )]
    );
    assert_eq!(me(&ts).faction1_bits(), Some(FactionBits::CelestialHand));
    assert_eq!(
        (me(&ts).society_rank_celhan(), me(&ts).society_rank_eldweb()),
        (Some(301), None)
    );
    assert!(quest_manager::has_quest(
        &ts.world,
        &QuestOwner::Creature(me_guid),
        "CelestialHandMember"
    ));
    assert_eq!(
        say(&mut ts, alpha, "@faction ew"),
        [broadcast(
            "Your current Faction state is now set to: Eldrytch Web with a rank of Initiate"
        )]
    );
    assert_eq!(
        (me(&ts).society_rank_celhan(), me(&ts).society_rank_eldweb()),
        (None, Some(1))
    );
    assert!(!quest_manager::has_quest(
        &ts.world,
        &QuestOwner::Creature(me_guid),
        "CelestialHandMember"
    ));
    assert_eq!(
        say(&mut ts, alpha, "@faction none"),
        [broadcast("Your current Faction state is now set to: None")]
    );
    assert_eq!(me(&ts).faction1_bits(), None);
    let help = say(&mut ts, alpha, "@faction");
    assert!(
        help[0].0.starts_with(
            "Your current Faction state is: None\nYou can change it to the following:\n"
        ),
        "{:?}",
        help[0].0
    );

    let before = me(&ts).total_experience().unwrap_or(0);
    // (GrantXP queues UpdateXpAndLevel on the player, so the level-up follows the reply)
    let reply = say(&mut ts, alpha, "@grantxp 1234");
    assert_eq!(
        reply[0],
        (
            "1,234 experience granted.".to_owned(),
            ChatMessageType::Advancement.0
        )
    );
    assert!(reply[1].0.starts_with("You are now level 2!"), "{reply:?}");
    assert_eq!(me(&ts).total_experience().unwrap_or(0), before + 1234);
}

#[test]
fn faction_sends_its_private_updates_to_the_player_alone() {
    // V360/V362 (a fix): ACE broadcast these self-only updates to every player who knew this one.
    use dereth_protocol::qualities::QualitiesPrivateUpdateInt;
    let (mut ts, alpha, bravo, _, _) = two_players();
    let faction = |ts: &TestServer, id: ClientId| -> Vec<i64> {
        ts.received::<QualitiesPrivateUpdateInt>(id)
            .into_iter()
            .map(|m| m.0)
            .filter(|u| u.property_id == u32::from(PropertyInt::Faction1Bits.0))
            .map(|u| i64::from(u.value))
            .collect()
    };
    let (mine, theirs) = (faction(&ts, alpha).len(), faction(&ts, bravo).len());
    let _ = say(&mut ts, alpha, "@faction ch 3");
    assert_eq!(
        faction(&ts, alpha)[mine..],
        [i64::from(FactionBits::CelestialHand.0)]
    );
    assert_eq!(
        faction(&ts, bravo)[theirs..],
        [] as [i64; 0],
        "the other player's client is not told"
    );
}

/// All the chat lines a client received.
fn all_chat(ts: &TestServer, id: ClientId) -> Vec<String> {
    ts.received::<CommunicationTextboxString>(id)
        .into_iter()
        .map(|m| m.text)
        .collect()
}

#[test]
fn admin_command_fall_throughs_and_wrong_messages_are_fixed() {
    // V360/V362 (a fix): each of these followed ACE's defect before.
    let (mut ts, alpha, bravo, _, _) = two_players();
    let me_guid = ObjectGuid::new(ALPHA);
    let session = ts
        .world
        .sessions
        .iter()
        .map(|(s, _)| s)
        .find(|&s| ts.world.sessions.player(s) == Some(me_guid))
        .expect("Alpha's session");

    // delete with nothing selected: the one message (ACE added "Object not found.")
    assert_eq!(
        say(&mut ts, alpha, "@delete"),
        [broadcast(
            "Delete failed. Please identify the object you wish to delete first."
        )]
    );

    // regen of a selection that is not in the landblock: nothing (ACE threw)
    ts.world
        .objects
        .get_mut(me_guid)
        .unwrap()
        .set_health_query_target(Some(0x8000_0999));
    empyrean_command::handlers::admin_commands::handle_regen(&mut ts.world, Some(session), &[]);
    ts.world
        .objects
        .get_mut(me_guid)
        .unwrap()
        .set_health_query_target(None);

    // monsterspell names the text typed (ACE: "Undef")
    assert_eq!(
        say(&mut ts, alpha, "@monsterspell nosuchspell"),
        [broadcast("Invalid SpellId nosuchspell")]
    );

    // de_n: the text after the comma, with or without a space (ACE lost "name,text"'s first
    // character, and threw on "name,")
    let seen = all_chat(&ts, bravo).len();
    let _ = say(&mut ts, alpha, "@de_n bravo player, hi there");
    let _ = say(&mut ts, alpha, "@de_n bravo player,hi there");
    empyrean_command::handlers::admin_commands::handledirect_emote_name(
        &mut ts.world,
        Some(session),
        &["bravo".to_owned(), "player,".to_owned()],
    );
    ts.advance(0.2);
    assert_eq!(all_chat(&ts, bravo)[seen..], ["hi there", "hi there", ""]);

    // modifyvital raises the max vital and fills it, adding no record under the bare vital id
    monster(&mut ts, 104.0, 100.0);
    appraise_monster(&mut ts);
    let _ = say(&mut ts, alpha, "@modifyvital Health 5");
    let m = ts.world.objects.get(MONSTER).unwrap();
    let vitals = m.biota.properties_attribute_2nd.as_ref().unwrap();
    assert!(
        !vitals.contains_key(&PropertyAttribute2nd::Health),
        "no stray Health record"
    );
    assert_eq!(
        vitals
            .get(&PropertyAttribute2nd::MaxHealth)
            .map(|v| v.init_level),
        Some(100)
    );
    let max = m
        .health()
        .max_value(&mut StatCtx::in_world(&mut ts.world, MONSTER));
    let m = ts.world.objects.get(MONSTER).unwrap();
    assert_eq!(
        m.biota
            .properties_attribute_2nd
            .as_ref()
            .unwrap()
            .get(&PropertyAttribute2nd::MaxHealth)
            .map(|v| (v.level_from_cp, v.current_level)),
        Some((5, max))
    );

    // vendordump of a vendor whose currency weenie is missing: the dump is still sent
    let vendor = ObjectGuid::new(0x8000_0200);
    let mut o = WorldObject::allocate(Class::Vendor);
    o.guid = vendor;
    o.biota.id = vendor.full();
    o.set_property(PropertyString::Name, "Shopkeep".to_owned());
    o.set_property(PropertyDataId::Setup, land::TEST_SETUP);
    o.set_alternate_currency(Some(999_999));
    // (a vendor is a creature: its heartbeat reads the vitals)
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
    o.set_location(Some(outdoor(102.0, 100.0, 20.0)));
    o.set_heartbeat_interval(Some(5.0));
    world_object_tick::world_object_initialize_heartbeats(&mut o, ts.world.now.unix_time);
    ts.world.objects.insert(o).expect("fresh guid");
    assert!(lm::add_object(&mut ts.world, vendor, false));
    ts.world
        .objects
        .get_mut(me_guid)
        .unwrap()
        .set_health_query_target(Some(vendor.full()));
    let dump = say(&mut ts, alpha, "@vendordump summary");
    assert_eq!(dump.len(), 1, "{dump:?}");
    assert!(
        dump[0].0.contains(
            "WCID 999999, which comes from PropertyDataId.AlternateCurrency, is not found"
        ),
        "{}",
        dump[0].0
    );
    assert!(
        dump[0].0.contains("MoneyOutflow: 0 WCID 999999\n"),
        "{}",
        dump[0].0
    );
}

#[test]
fn the_last_appraised_monster_is_inspected_and_edited() {
    let (mut ts, alpha, _, _, _) = two_players();
    monster(&mut ts, 104.0, 100.0);

    assert_eq!(
        say(&mut ts, alpha, "@getinfo"),
        [broadcast("GetLastAppraisedObject() - no appraisal target")]
    );
    appraise_monster(&mut ts);

    // getproperty
    assert_eq!(
        say(&mut ts, alpha, "@getproperty PropertyString.Name"),
        [broadcast("Drudge (80000100): PropertyString.Name = Drudge")]
    );
    assert_eq!(
        say(&mut ts, alpha, "@getproperty propertybool.ATTACKABLE"),
        [broadcast(
            "Drudge (80000100): propertybool.ATTACKABLE = True"
        )]
    );
    assert_eq!(
        say(&mut ts, alpha, "@getproperty PropertyInt.Level"),
        [broadcast("Drudge (80000100): PropertyInt.Level = ")]
    );
    assert_eq!(
        say(&mut ts, alpha, "@getproperty Name"),
        [broadcast("Unknown Name")]
    );
    assert_eq!(
        say(&mut ts, alpha, "@getproperty Nothing.Name"),
        [broadcast("Unknown property type: Nothing")]
    );
    assert_eq!(
        say(&mut ts, alpha, "@getproperty PropertyInt.NoSuchThing"),
        [broadcast("Couldn't find PropertyInt.NoSuchThing")]
    );

    assert_eq!(
        say(&mut ts, alpha, "@setproperty PropertyBool.Attackable null"),
        [broadcast(
            "Drudge (80000100): PropertyBool.Attackable = null"
        )]
    );
    assert_eq!(
        ts.world
            .objects
            .get(MONSTER)
            .unwrap()
            .get_property(PropertyBool::Attackable),
        None
    );
    let _ = TestServer::take_not_ported();
    assert_eq!(
        say(&mut ts, alpha, "@setproperty PropertyInt.Level 0x10"),
        [broadcast("Drudge (80000100): PropertyInt.Level = 0x10")]
    );
    assert_eq!(
        TestServer::take_not_ported().get("ACE: Player.UpdateProperty"),
        None
    );
    assert_eq!(
        ts.world
            .objects
            .get(MONSTER)
            .unwrap()
            .get_property(PropertyInt::Level),
        Some(16),
        "UpdateProperty set it"
    );
    assert_eq!(
        say(&mut ts, alpha, "@setproperty PropertyInt.Level 0x1G"),
        []
    );

    // resist-info: every resistance is null here; the stable sort keeps ACE's list order
    let reply = say(&mut ts, alpha, "@resist-info");
    let names = [
        "ResistSlash",
        "ResistPierce",
        "ResistBludgeon",
        "ResistFire",
        "ResistCold",
        "ResistAcid",
        "ResistElectric",
        "ResistNether",
    ];
    let mut expected = vec![broadcast("Drudge (80000100):")];
    expected.extend(names.iter().map(|n| broadcast(&format!("{n} - "))));
    assert_eq!(reply, expected);

    // showstats: the attributes, then the vitals; no skills
    let max = |ts: &mut TestServer, v: fn(&WorldObject) -> CreatureVital| {
        let vital = v(ts.world.objects.get(MONSTER).unwrap());
        vital.max_value(&mut StatCtx::in_world(&mut ts.world, MONSTER))
    };
    let (h, s, m) = (
        max(&mut ts, WorldObject::health),
        max(&mut ts, WorldObject::stamina),
        max(&mut ts, WorldObject::mana),
    );
    let stats = format!(
        "Strength: 60\nEndurance: 60\nCoordination: 60\nQuickness: 60\nFocus: 60\nSelf: 60\n\nHealth: 100/{h}\nStamina: 100/{s}\nMana: 100/{m}"
    );
    assert_eq!(say(&mut ts, alpha, "@showstats"), [broadcast(&stats)]);

    // debugdamage: set, toggle and the unknown parameter
    assert_eq!(
        say(&mut ts, alpha, "@debugdamage Attacking"),
        [broadcast("DebugDamage: - Attacker (Drudge)")]
    );
    assert_eq!(
        say(&mut ts, alpha, "@debugdamage"),
        [broadcast("DebugDamage: - None (Drudge)")]
    );
    assert_eq!(
        say(&mut ts, alpha, "@debugdamage"),
        [broadcast("DebugDamage: - All (Drudge)")]
    );
    assert_eq!(
        say(&mut ts, alpha, "@debugdamage sideways"),
        [broadcast("DebugDamage: - unknown sideways (Drudge)")]
    );
    let c = ts
        .world
        .objects
        .get(MONSTER)
        .unwrap()
        .creature
        .as_ref()
        .unwrap();
    assert_eq!(
        c.creature_combat.debug_damage_target,
        ObjectGuid::new(ALPHA)
    );

    // propertydump: Empyrean's header and the object's class (brand; ACE's says "ACE Debug Output:"
    // and names the class's .cs file)
    let dump = say(&mut ts, alpha, "@propertydump");
    assert_eq!(dump.len(), 1);
    assert!(
        dump[0]
            .0
            .starts_with("\nEmpyrean Debug Output:\nObject class: Creature\nGuid: "),
        "{:?}",
        dump[0].0
    );

    // givemana: no mana on a creature: min(amount, 0 - 0)
    assert_eq!(
        say(&mut ts, alpha, "@givemana 5"),
        [(
            "You give 0 points of mana to the Drudge.".to_owned(),
            ChatMessageType::Magic.0
        )]
    );

    // dist: 4 m east of the admin
    let reply = say(&mut ts, alpha, "@dist");
    assert_eq!(reply.len(), 3);
    assert_eq!(reply[..2], [broadcast("Dist: 4"), broadcast("2D Dist: 4")]);
    assert!(reply[2].0.starts_with("CylDist: "), "{:?}", reply[2].0);

    // the dumps of a selected object that is no generator or vendor
    ts.world
        .objects
        .get_mut(ObjectGuid::new(ALPHA))
        .unwrap()
        .set_health_query_target(Some(MONSTER.full()));
    assert_eq!(
        say(&mut ts, alpha, "@generatordump"),
        [system("Drudge (0x80000100) is not a generator.")]
    );
    assert_eq!(
        say(&mut ts, alpha, "@vendordump"),
        [system("Drudge (0x80000100) is not a vendor.")]
    );

    // item spells need a spell id
    assert_eq!(
        say(&mut ts, alpha, "@additemspell nosuchspell"),
        [broadcast("nosuchspell is not a valid spell id")]
    );
}

#[test]
fn inventory_commands_create_items_in_the_pack() {
    let (mut ts, alpha, _, _, _) = two_players();
    let me_guid = ObjectGuid::new(ALPHA);

    // currency: 273 and 20630; only the pyreal weenie exists, at its max stack size
    let before = container::inventory(me(&ts)).len();
    let creates = ts.received::<ItemCreateObject>(alpha).len();
    assert_eq!(say(&mut ts, alpha, "@currency"), []);
    let pack: Vec<ObjectGuid> = container::inventory(me(&ts)).keys().copied().collect();
    assert_eq!(pack.len(), before + 1);
    let coins = ts.world.objects.get(*pack.last().unwrap()).unwrap();
    assert_eq!(
        (coins.biota.weenie_class_id, coins.stack_size()),
        (PYREAL_WCID, Some(25000))
    );
    assert_eq!(coins.container_id(), Some(me_guid.full()));
    assert!(
        ts.received::<ItemCreateObject>(alpha).len() > creates,
        "the client is told"
    );

    assert_eq!(
        say(&mut ts, alpha, "@cirand Nothing"),
        [broadcast("Nothing is not a valid WeenieType")]
    );
    assert_eq!(
        say(&mut ts, alpha, "@cirand admin"),
        [broadcast(
            "Admin is not a valid WeenieType for create commands"
        )]
    );
    assert_eq!(
        say(&mut ts, alpha, "@cirand Coin 51"),
        [broadcast("<num to create> must be a number between 1 - 50")]
    );

    // makeiou: IOUs are off by default (iou_trades), so nothing is made
    let pack_len = container::inventory(me(&ts)).len();
    assert_eq!(
        say(&mut ts, alpha, "@makeiou x"),
        [broadcast("WCID must be a valid weenie id")]
    );
    assert_eq!(say(&mut ts, alpha, "@makeiou 12345"), []);
    assert_eq!(container::inventory(me(&ts)).len(), pack_len);
    // on: the IOU is signed with our name (brand; ACE signs it "ACEmulator"), and the Town Crier
    // redeems both
    assert!(empyrean_world::managers::property_manager::modify_bool(
        &ts.world,
        "iou_trades",
        true
    ));
    let _ = say(&mut ts, alpha, "@makeiou 12345");
    let pack: Vec<ObjectGuid> = container::inventory(me(&ts)).keys().copied().collect();
    assert_eq!(pack.len(), pack_len + 1);
    let iou = ts.world.objects.get(*pack.last().unwrap()).unwrap();
    assert_eq!(
        iou.get_property(PropertyString::Name).as_deref(),
        Some("IOU")
    );
    assert_eq!(
        (
            iou.scribe_name().as_deref(),
            iou.scribe_account().as_deref()
        ),
        (Some("Empyrean"), Some("prewritten"))
    );
    use empyrean_world::world_objects::player_inventory::is_iou_author;
    assert!(is_iou_author(Some("Empyrean")) && is_iou_author(Some("ACEmulator")));
    assert!(
        !is_iou_author(Some("Someone")) && !is_iou_author(Some("empyrean")) && !is_iou_author(None)
    );
}

#[test]
fn forcelogoff_boots_a_player_and_reports_the_path() {
    let (mut ts, alpha, _, _, _) = two_players();

    assert_eq!(
        say(&mut ts, alpha, "@forcelogoff Nobody Here"),
        [broadcast(
            "Unable to force log off for Nobody Here: Player not found in manager."
        )]
    );

    let reply = say(&mut ts, alpha, "@forcelogoff bravo player");
    assert_eq!(reply.len(), 1);
    let text = &reply[0].0;
    assert!(text.starts_with("Player Bravo Player (0x50000002) found in PlayerManager.onlinePlayers.\n------- Session: C2S: "), "{text:?}");
    assert!(text.contains("------- IsLoggingOut: False\n------- IsInDeathProcess: False\n------- FoundOnLandblock: True\n------- ForcedLogOffRequested: False\n"), "{text:?}");
    assert!(
        text.ends_with("Log off path taken: player.ForcedLogOffRequested = true | player.Session.Terminate()\nUse this command again if this player does not properly log off within the next minute."),
        "{text:?}"
    );
    // asked again before the log off completes: the report points at Empyrean's issue tracker
    // (brand; ACE's names the ACEmulator team's Discord)
    let again = say(&mut ts, alpha, "@forcelogoff bravo player");
    let text = &again[0].0;
    assert!(text.contains("ForcedLogOffRequested: True"), "{text:?}");
    assert!(
        text.ends_with(
            "\nPlease report the above at https://github.com/dereth-network/dereth/issues."
        ),
        "{text:?}"
    );
    assert!(
        ts.run_until(30.0, |ts| player_manager::get_online_count(&ts.world) == 1),
        "Bravo is logged off"
    );
}

#[test]
fn object_maintenance_dumps_go_to_the_console() {
    let (mut ts, alpha, _, _, _) = two_players();

    command_manager::start_console_capture();
    assert_eq!(say(&mut ts, alpha, "@knownobjs"), []);
    let out = command_manager::take_console_output();
    assert!(
        out[0].starts_with("\nKnown objects to Alpha Admin: "),
        "{out:?}"
    );
    assert_eq!(
        out.len() - 1,
        out[0]
            .rsplit(": ")
            .next()
            .unwrap()
            .parse::<usize>()
            .unwrap(),
        "one line per object"
    );

    // a guid that does not parse, or names no physics object, leaves the player as the target
    command_manager::start_console_capture();
    assert_eq!(say(&mut ts, alpha, "@knownplayers nosuchguid"), []);
    assert_eq!(say(&mut ts, alpha, "@visibleplayers DEADBEEF"), []);
    let out = command_manager::take_console_output();
    assert!(
        out[0].starts_with(
            "
Known players to Alpha Admin: "
        ),
        "{out:?}"
    );
    assert!(
        out.iter().any(|l| l.starts_with(
            "
Visible players to Alpha Admin: "
        )),
        "{out:?}"
    );
    // `target` with nothing appraised finds nothing
    assert_eq!(
        say(&mut ts, alpha, "@visibletargets target"),
        [
            broadcast("GetLastAppraisedObject() - no appraisal target"),
            broadcast("Couldn't find target target")
        ]
    );

    command_manager::start_console_capture();
    assert_eq!(say(&mut ts, alpha, "@lostest"), []);
    assert_eq!(
        command_manager::take_console_output(),
        ["ERROR: no appraisal target"]
    );

    let reply = say(&mut ts, alpha, "@myloc");
    assert_eq!(reply.len(), 3);
    assert_eq!(
        reply[0],
        broadcast(&format!("CurrentLandblock: {HOME:04X}"))
    );
    assert!(reply[1].0.starts_with("Location: 0x"), "{:?}", reply[1].0);
    assert!(reply[2].0.starts_with("Physics : 0x"), "{:?}", reply[2].0);
}

pub(crate) use empyrean_testkit::EmptyShard;
