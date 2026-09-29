//! ACE: Source/ACE.Server/Command/Handlers/AdvocateCommands.cs::AdvocateCommands
//! Bestow/remove advocate and rank rule; attackable/tele move the admin; password change rate-
//! limited to 5 s; help; admin deletes an online character.
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
use dereth_protocol::objects::EffectsPlayerTeleport;
use empyrean_command::command_manager;
use empyrean_content::models::world::Weenie;
use empyrean_content::MemContent;
use empyrean_dat::file_types::SecondaryAttributeTable;
use empyrean_dat::{file_id, FakeDats};
use empyrean_entity::enums::{
    Channel, ChatMessageType, PropertyBool, PropertyDataId, PropertyInt, PropertyString,
    WeenieError, WeenieType,
};
use empyrean_entity::{ObjectGuid, Position};
use empyrean_testkit::{land, ClientId, TestServer};
use empyrean_world::managers::guid_manager;
use empyrean_world::managers::player_manager;
use empyrean_world::world_objects::container;
use empyrean_world::world_objects::world_object::WorldObject;

const HOME: u16 = 0xA9B4;
const ALPHA: u32 = 0x5000_0001;
const BRAVO: u32 = 0x5000_0002;

/// The advocate books and shields (`Advocate.AdvocateBooks`, `AdvocateItems`).
const ADVOCATE_ITEMS: [(u32, &str); 9] = [
    (3653, "bookadvocatefane"),
    (3941, "bookadvocateinstructions"),
    (2628, "shieldadvocate1"),
    (2629, "shieldadvocate2"),
    (2630, "shieldadvocate3"),
    (2631, "shieldadvocate4"),
    (2632, "shieldadvocate5"),
    (2633, "shieldadvocate6"),
    (3594, "shieldadvocate7"),
];

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
        .build()
        .expect("fake dats")
}

fn content() -> MemContent {
    let mut c = MemContent::new().weenie(Weenie::new(1, "human", WeenieType::Creature).with_did(
        empyrean_entity::enums::PropertyDataId::CombatTable,
        0x3000_0000,
    ));
    for (wcid, name) in ADVOCATE_ITEMS {
        c = c.weenie(
            Weenie::new(wcid, name, WeenieType::Generic)
                .with_string(PropertyString::Name, &format!("Advocate Item {wcid}"))
                .with_did(PropertyDataId::Setup, land::TEST_SETUP)
                .with_int(PropertyInt::EncumbranceVal, 5),
        );
    }
    c
}

fn seed(ts: &TestServer, account: &str, guid: u32, name: &str, at: Position) {
    crate::support::command_characters::seed(ts, account, guid, name, at, false);
}

/// The server with "Alpha Admin" and "Bravo Player" in the shard, and the command table.
fn server() -> TestServer {
    let mut ts = TestServer::with_dats(dats());
    land::use_flat_land_with_test_setup(&mut ts.world, &[HOME], 10);
    ts.world.content = Arc::new(content());
    guid_manager::initialize(&mut ts.world, &mut EmptyShard);
    seed(
        &ts,
        "u64alpha",
        ALPHA,
        "Alpha Admin",
        outdoor(100.0, 100.0, 20.0),
    );
    seed(
        &ts,
        "u64bravo",
        BRAVO,
        "Bravo Player",
        outdoor(150.0, 60.0, 20.0),
    );
    player_manager::initialize(&mut ts.world);
    command_manager::initialize(None);
    ts
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

/// The server with Alpha (an admin) and Bravo (a player) in the world.
fn two_players() -> (TestServer, ClientId, ClientId) {
    let mut ts = server();
    let alpha = enter(&mut ts, "u64alpha", ALPHA);
    let bravo = enter(&mut ts, "u64bravo", BRAVO);
    ts.world
        .objects
        .get_mut(ObjectGuid::new(ALPHA))
        .unwrap()
        .set_is_admin_prop(true);
    let _ = TestServer::take_not_ported();
    (ts, alpha, bravo)
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

/// The class names of the items in the player's main pack.
fn pack(ts: &TestServer, guid: u32) -> Vec<u32> {
    let o = ts.world.objects.get(ObjectGuid::new(guid)).unwrap();
    let mut wcids: Vec<u32> = container::inventory(o)
        .keys()
        .map(|g| ts.world.objects.get(*g).unwrap().biota.weenie_class_id)
        .collect();
    wcids.sort_unstable();
    wcids
}

fn player(ts: &TestServer, guid: u32) -> &WorldObject {
    ts.world.objects.get(ObjectGuid::new(guid)).unwrap()
}

#[test]
fn an_admin_bestows_and_removes_an_advocate() {
    let (mut ts, alpha, bravo) = two_players();

    assert_eq!(
        say(&mut ts, alpha, "@bestow Bravo Player 9"),
        [broadcast("9 is not a valid advocate level.")]
    );
    assert_eq!(
        say(&mut ts, alpha, "@bestow Nobody 2"),
        [broadcast("Nobody was not found in the database.")]
    );
    assert_eq!(
        say(&mut ts, alpha, "@remove Bravo Player"),
        [broadcast("Bravo Player is not an Advocate.")]
    );

    let mark = ts.received::<CommunicationTextboxString>(bravo).len();
    assert_eq!(
        say(&mut ts, alpha, "@bestow Bravo Player 3"),
        [broadcast("Bravo Player is now an Advocate, level 3.")]
    );
    let told: Vec<String> = ts.received::<CommunicationTextboxString>(bravo)[mark..]
        .iter()
        .map(|m| m.text.clone())
        .collect();
    assert_eq!(told, ["You have been bestowed as an Advocate, level 3!"]);
    let p = player(&ts, BRAVO);
    assert!(p.advocate_quest_player() && p.is_advocate() && !p.is_psr());
    assert_eq!(p.advocate_level(), Some(3));
    let channels = Channel(
        Channel::Help.0
            | Channel::Abuse.0
            | Channel::Advocate1.0
            | Channel::Advocate2.0
            | Channel::Advocate3.0
            | Channel::TownChans.0,
    );
    assert_eq!(
        (p.channels_allowed(), p.channels_active()),
        (Some(channels), Some(channels))
    );
    assert_eq!(
        pack(&ts, BRAVO),
        [2630, 3653, 3941],
        "the tome, the instructions and the level 3 aegis"
    );

    // a higher rank swaps the aegis; level 5 and up is a PSR
    assert_eq!(
        say(&mut ts, alpha, "@bestow Bravo Player 5"),
        [broadcast("Bravo Player is now an Advocate, level 5.")]
    );
    assert_eq!(pack(&ts, BRAVO), [2632, 3653, 3941]);
    assert!(player(&ts, BRAVO).is_psr());
    assert_eq!(
        say(&mut ts, alpha, "@bestow Bravo Player 5"),
        [broadcast(
            "Bravo Player's Advocate rank is already at level 5."
        )]
    );

    // an advocate cannot bestow at or above their own rank
    assert_eq!(say(&mut ts, bravo, "@bestow Alpha Admin 5"), [broadcast("You cannot bestow Alpha Admin's Advocate rank to 5 because that is equal to or higher than your rank.")]);

    let mark = ts.received::<CommunicationTextboxString>(bravo).len();
    assert_eq!(
        say(&mut ts, alpha, "@remove Bravo Player"),
        [broadcast("Bravo Player is no longer an Advocate.")]
    );
    let told: Vec<String> = ts.received::<CommunicationTextboxString>(bravo)[mark..]
        .iter()
        .map(|m| m.text.clone())
        .collect();
    assert_eq!(told, ["You have been removed from the Advocate ranks!"]);
    let p = player(&ts, BRAVO);
    assert!(!p.advocate_quest_player() && !p.is_advocate() && !p.is_psr());
    assert_eq!(
        (
            p.advocate_level(),
            p.channels_allowed(),
            p.channels_active()
        ),
        (None, None, None)
    );
    assert!(
        pack(&ts, BRAVO).is_empty(),
        "the advocate items are consumed"
    );
}

/// An advocate cannot remove an advocate of the same level (ACE e0f9ce83; before it, ACE refused
/// only a higher level).
#[test]
fn an_advocate_cannot_remove_an_advocate_of_equal_rank() {
    let (mut ts, alpha, bravo) = two_players();
    assert_eq!(
        say(&mut ts, alpha, "@bestow Bravo Player 5"),
        [broadcast("Bravo Player is now an Advocate, level 5.")]
    );
    let a = ts.world.objects.get_mut(ObjectGuid::new(ALPHA)).unwrap();
    a.set_advocate_quest_player(true);
    a.set_is_advocate(true);
    a.set_advocate_level(Some(5));

    assert_eq!(
        say(&mut ts, bravo, "@remove Alpha Admin"),
        [broadcast("You cannot remove Alpha Admin's Advocate status because they are equal to or out rank you.")]
    );
    assert!(player(&ts, ALPHA).is_advocate(), "still an advocate");
    assert_eq!(player(&ts, ALPHA).advocate_level(), Some(5));
}

#[test]
fn attackable_and_tele_move_the_admin() {
    let (mut ts, alpha, _) = two_players();
    assert_eq!(
        say(&mut ts, alpha, "@attackable off"),
        [broadcast(
            "Monsters will only attack you if provoked by you first."
        )]
    );
    assert_eq!(
        player(&ts, ALPHA).get_property(PropertyBool::Attackable),
        Some(false)
    );
    assert_eq!(
        say(&mut ts, alpha, "@attackable sideways"),
        [broadcast("Monsters will attack you normally.")]
    );
    assert_eq!(
        player(&ts, ALPHA).get_property(PropertyBool::Attackable),
        Some(true)
    );

    let usage = say(&mut ts, alpha, "@tele");
    assert_eq!(
        usage[0],
        (
            "Invalid parameter count, got 0, expected 1!".to_owned(),
            ChatMessageType::Help.0
        )
    );
    // 42.4N, 33.6E is in the admin's landblock (0xA9B4)
    let reply = say(&mut ts, alpha, "@tele 42.4n 33.6e");
    assert_eq!(reply.len(), 1, "{reply:?}");
    assert!(
        reply[0].0.starts_with("Position: [Cell: 0xA9B4 | Offset: "),
        "{reply:?}"
    );
    assert!(reply[0].0.ends_with(" | Facing: 0, 0, 0, 1]"), "{reply:?}");
    assert_eq!(
        ts.received::<EffectsPlayerTeleport>(alpha).len(),
        1,
        "PlayerTeleport"
    );
    let l = player(&ts, ALPHA).location().unwrap();
    assert_eq!(l.cell() >> 16, u32::from(HOME));
}

#[test]
fn a_player_changes_their_password_at_most_every_five_seconds() {
    let (mut ts, _, bravo) = two_players();
    assert_eq!(
        say(&mut ts, bravo, "@passwd pw n3wpass"),
        [broadcast("Account password successfully changed.")]
    );
    assert_eq!(
        say(&mut ts, bravo, "@passwd n3wpass other"),
        [broadcast(
            "This command may only be run once every 5 seconds."
        )]
    );
    ts.advance(5.0);
    assert_eq!(say(&mut ts, bravo, "@passwd wrong other"), [broadcast(
        "Unable to change password: Password provided in first parameter does not match current account password for this account!"
    )]);
    let mut account = ts.auth().get_account_by_name("u64bravo").unwrap();
    let mut auth = ts.world.auth.lock();
    assert!(account.password_matches("n3wpass", &mut **auth));
}

#[test]
fn a_player_reads_the_help() {
    let (mut ts, _, bravo) = two_players();
    let help = say(&mut ts, bravo, "@emphelp");
    assert_eq!(help.len(), 1);
    assert!(
        help[0].0.starts_with(
            "Note: You may substitute a forward slash (/) for the at symbol (@).\nUse @help"
        ),
        "{help:?}"
    );
    assert!(help[0].0.contains("\n@emphelp commands - Lists all commands.\nYou can also use @empcommands to get a complete list of the supported Empyrean commands available to you.\n"), "{help:?}");
    // ACE's name is the same command (brand)
    assert_eq!(say(&mut ts, bravo, "@acehelp"), help);
    assert_eq!(
        say(&mut ts, bravo, "@emphelp passwd"),
        say(&mut ts, bravo, "@acehelp passwd")
    );
    assert_eq!(
        say(&mut ts, bravo, "@emphelp passwd"),
        [broadcast(
            "@passwd - Change your account password.\nUsage: @passwd oldpassword newpassword\n\n"
        )]
    );

    // an admin command is hidden from a player: "Unknown command", the weenie error, the hint
    let mark = ts.received_raw(bravo).len();
    assert_eq!(say(&mut ts, bravo, "@acehelp bestow"), [
        ("Unknown command: bestow".to_owned(), ChatMessageType::Help.0),
        broadcast("Use @empcommands to get a complete list of commands available for you to use.\nTo get more information about a specific command, use @emphelp command\n"),
    ]);
    // the chat line, the WeenieError game event (0xF7B0 / 0x028A), the chat line
    let sent: Vec<(u32, Option<u32>)> = ts.received_raw(bravo)[mark..]
        .iter()
        .filter_map(|m| match m.opcode {
            0xF7E0 => Some((0xF7E0, None)),
            0xF7B0 if u32::from_le_bytes(m.body[8..12].try_into().unwrap()) == 0x028A => Some((
                0x028A,
                Some(u32::from_le_bytes(m.body[12..16].try_into().unwrap())),
            )),
            _ => None,
        })
        .collect();
    assert_eq!(
        sent,
        [
            (0xF7E0, None),
            (
                0x028A,
                Some(WeenieError::ThatIsNotAValidCommand.0.cast_unsigned())
            ),
            (0xF7E0, None)
        ]
    );

    // the player's list: player-level commands, by name, console-only ones left out
    let list = say(&mut ts, bravo, "@empcommands");
    assert_eq!(
        say(&mut ts, bravo, "@acecommands"),
        list,
        "ACE's name is the same command"
    );
    assert_eq!(list.len(), 1);
    let lines: Vec<&str> = list[0].0.lines().collect();
    assert_eq!(
        lines[..2],
        [
            "Note: You may substitute a forward slash (/) for the at symbol (@).",
            "For more information, type @emphelp < command >."
        ]
    );
    assert!(
        lines.contains(&"@passwd - Change your account password.")
            && lines.contains(&"@emphelp - Displays help."),
        "{lines:?}"
    );
    assert!(
        lines.contains(&"@acehelp - Same as @emphelp (ACE's name).")
            && lines.contains(&"@aceversion - Same as @empversion (ACE's name)."),
        "{lines:?}"
    );
    assert!(
        !lines
            .iter()
            .any(|l| l.starts_with("@bestow") || l.starts_with("@accountget")),
        "{lines:?}"
    );
    // (a description may span lines: `config`'s second line is not an entry)
    let names: Vec<&str> = lines[2..]
        .iter()
        .filter(|l| l.starts_with('@'))
        .map(|l| l.split(" - ").next().unwrap())
        .collect();
    let mut sorted = names.clone();
    sorted.sort_by(|a, b| empyrean_command::handlers::admin_commands::culture_compare(a, b));
    assert_eq!(names, sorted, "sorted by name");
}

#[test]
fn an_admin_deletes_an_online_character() {
    let (mut ts, alpha, _) = two_players();
    // V363 (a fix): the logout has only started and its end completes the deletion, so the
    // command reports success (ACE: "Unable to boot and delete ... due to PlayerManager failure.")
    let reply = say(&mut ts, alpha, "@deletecharacter Bravo Player");
    assert_eq!(
        reply,
        [broadcast(
            "Successfully booted and deleted character Bravo Player (0x50000002)."
        )]
    );
    ts.advance(10.0);
    assert_eq!(
        player_manager::get_online_count(&ts.world),
        1,
        "Bravo is logged off"
    );
    let c = ts.shard().get_character_stub_by_guid(BRAVO).unwrap();
    assert!(
        c.is_deleted && c.delete_time > 0,
        "the logout saved the deleted character"
    );
    // the end of the logout (Session.CheckCharactersForDeletion) completes the deletion
    assert!(player_manager::find_by_name(&ts.world, "Bravo Player")
        .0
        .is_none());
    assert_eq!(
        say(&mut ts, alpha, "@deletecharacter Bravo Player"),
        [broadcast(
            "There is no character named Bravo Player in the database."
        )]
    );
}

pub(crate) use crate::support::empty_shard::EmptyShard;
