//! ACE: Source/ACE.Server/Command/CommandManager.cs::ParseCommand
//! Player @commands in chat get ACE's replies; sentinel command refused to a player; sudo raw
//! line; sentinel boot with reason; reportbug/source; myquests; debugcast/check-collision read
//! the body.
//! Fixture: a virtual-time TestServer, isolated stores and synthetic dats.

use std::sync::Arc;

use dereth_protocol::comms::{CommunicationTalk, CommunicationTextboxString};
use dereth_protocol::login::LoginAccountBooted;
use empyrean_command::command_manager;
use empyrean_content::models::world::Weenie;
use empyrean_content::MemContent;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{
    AccessLevel, ChatMessageType, PropertyDataId, PropertyInt, PropertyString, WeenieType,
};
use empyrean_entity::{ObjectGuid, Position};
use empyrean_net::{SessionId, SessionState};
use empyrean_testkit::land::{self, TEST_SETUP};
use empyrean_testkit::{ClientId, TestServer};
use empyrean_world::managers::guid_manager::{self, ShardGuidQueries};
use empyrean_world::managers::player_manager::OnlinePlayer;
use empyrean_world::managers::{landblock_manager, property_manager};
use empyrean_world::world_objects::world_object::CtorEnv;

const LB: u32 = 0xA9B4_0000;
const PLAYER_WCID: u32 = 1;
const ALPHA: u32 = 0x5000_0001;
const BRAVO: u32 = 0x5000_0002;

struct EmptyShard;

impl ShardGuidQueries for EmptyShard {
    fn get_max_guid_found_in_range(&mut self, _min: u32, _max: u32) -> u32 {
        u32::MAX
    }
    fn get_sequence_gaps(&mut self, _min: u32, _limit: u32) -> Vec<(u32, u32)> {
        Vec::new()
    }
}

fn server() -> TestServer {
    server_with(|c| c)
}

fn server_with(extra: impl FnOnce(MemContent) -> MemContent) -> TestServer {
    let content = extra(MemContent::new()).weenie(
        Weenie::new(PLAYER_WCID, "human", WeenieType::Creature)
            .with_did(
                empyrean_entity::enums::PropertyDataId::CombatTable,
                0x3000_0000,
            )
            .with_string(PropertyString::Name, "human")
            .with_did(PropertyDataId::Setup, TEST_SETUP),
    );
    let mut ts = TestServer::with_setup(
        empyrean_testkit::dats::with_stat_tables(FakeDats::new())
            .build()
            .expect("fake dats"),
        |w| {
            w.content = Arc::new(content);
            guid_manager::initialize(w, &mut EmptyShard);
        },
    );
    land::use_flat_land_with_test_setup(&mut ts.world, &[0xA9B4], 0);
    // Program.Main's "Initializing CommandManager..." (no console in tests).
    command_manager::initialize(None);
    ts
}

fn at(x: f32, y: f32) -> Position {
    Position::from_components(LB | 0x0001, x, y, 0.0, 0.0, 0.0, 0.0, 1.0, false)
}

/// A client logged in as `account` whose session plays `guid` (named `name`), in the world and
/// online in `PlayerManager`.
fn join(
    ts: &mut TestServer,
    account: &str,
    guid: u32,
    name: &str,
    pos: Position,
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
    o.set_property(PropertyString::Name, name.to_owned());
    o.player.as_mut().expect("a player").player.character =
        Some(empyrean_store::models::shard::Character::default());
    o.set_location(Some(pos));
    w.objects.insert(o).expect("fresh");
    assert!(w.player_manager.online_players.try_add(
        guid,
        OnlinePlayer {
            guid: ObjectGuid::new(guid),
            account: None
        }
    ));

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

fn help(text: &str) -> (String, u32) {
    (text.to_owned(), ChatMessageType::Help.0)
}

#[test]
fn player_commands_in_chat_get_aces_replies() {
    let mut ts = server();
    let (id, _) = join(&mut ts, "u61alpha", ALPHA, "Alpha Tester", at(10.0, 10.0));
    let _ = TestServer::take_not_ported();

    assert_eq!(
        say(&mut ts, id, "@pop"),
        [broadcast("Current world population: 1")]
    );

    // @age, @die and @lifestone are the client's own commands (it sends them as game actions);
    // typed into chat they reach ACE's command table, which has none of them.
    assert_eq!(say(&mut ts, id, "@age"), [help("Unknown command: age")]);
    assert_eq!(say(&mut ts, id, "@die"), [help("Unknown command: die")]);
    assert_eq!(
        say(&mut ts, id, "@lifestone now"),
        [help("Unknown command: lifestone")]
    );
    assert_eq!(say(&mut ts, id, "@"), [help("Unknown command: ")]);
    // a line of spaces makes ParseCommand throw: logged, no reply
    assert_eq!(say(&mut ts, id, "@   "), []);

    // the property-gated commands, off by default
    assert_eq!(
        say(&mut ts, id, "@config ShowHelm off"),
        [broadcast(
            "The command \"config\" is not currently enabled on this server."
        )]
    );
    assert_eq!(
        say(&mut ts, id, "@myquests"),
        [broadcast(
            "The command \"myquests\" is not currently enabled on this server."
        )]
    );
    // (Empyrean's text names our command, also for ACE's name)
    assert_eq!(
        say(&mut ts, id, "@ACEVERSION"),
        [broadcast(
            "The command \"empversion\" is not currently enabled on this server."
        )]
    );
    assert_eq!(
        say(&mut ts, id, "@empversion"),
        [broadcast(
            "The command \"empversion\" is not currently enabled on this server."
        )]
    );
    assert_eq!(
        say(&mut ts, id, "@reportbug code"),
        [broadcast(
            "The command \"reportbug\" is not currently enabled on this server."
        )]
    );

    // Enabled: /config list, an unknown option, and a known option.
    assert!(property_manager::modify_bool(
        &ts.world,
        "player_config_command",
        true
    ));
    let list = say(&mut ts, id, "@config list");
    assert_eq!(list.len(), 6);
    assert_eq!(list[0], broadcast("Common settings:\nConfirmVolatileRareUse, MainPackPreferred, SalvageMultiple, SideBySideVitals, UseCraftSuccessDialog"));
    assert_eq!(
        say(&mut ts, id, "@config NoSuchOption on"),
        [broadcast("Unknown character option: NoSuchOption")]
    );
    assert_eq!(
        say(&mut ts, id, "@config showhelm off"),
        [broadcast("Character option showhelm is now off.")]
    );
    let _ = TestServer::take_not_ported();

    assert_eq!(
        say(&mut ts, id, "@castmeter on"),
        [broadcast("Cast efficiency meter enabled")]
    );
    assert_eq!(
        say(&mut ts, id, "@castmeter off"),
        [broadcast("Cast efficiency meter disabled")]
    );
    assert_eq!(
        TestServer::take_not_ported().get("ACE: MagicState.CastMeter"),
        None
    );

    // house-select with a non-number: int.TryParse fails and the handler returns silently
    assert_eq!(say(&mut ts, id, "@house-select x"), []);

    // plain speech still goes to Player.HandleActionTalk
    let _ = say(&mut ts, id, "hello @pop");
    assert!(!TestServer::take_not_ported().contains_key("ACE: GameActionTalk.Handle"));
}

#[test]
fn an_under_privileged_player_is_refused_a_sentinel_command() {
    let mut ts = server();
    let (id, session) = join(&mut ts, "u61alpha", ALPHA, "Alpha Tester", at(10.0, 10.0));
    let player = ObjectGuid::new(ALPHA);

    // NotAuthorized: ACE's Talk handler answers nothing (only InvalidCommand and
    // InvalidParameterCount have a reply) and the handler does not run.
    assert_eq!(say(&mut ts, id, "@neversaydie"), []);
    assert!(!ts.world.objects.get(player).unwrap().invincible());
    assert_eq!(say(&mut ts, id, "@portal_bypass"), []);
    assert!(!ts
        .world
        .objects
        .get(player)
        .unwrap()
        .ignore_portal_restrictions());

    // the account's access level alone is not enough: GetCommandHandler reads the player's
    // IsSentinel/IsEnvoy/IsArch/IsAdmin properties, except through sudo
    ts.world.sessions.get_mut(session).unwrap().access_level = AccessLevel::Sentinel;
    assert_eq!(say(&mut ts, id, "@neversaydie"), []);
    assert_eq!(
        say(&mut ts, id, "@sudo neversaydie"),
        [broadcast("You are now immortal.")]
    );
    assert!(ts.world.objects.get(player).unwrap().invincible());
    // the handler gets the parameters after the sudo'd command
    assert_eq!(
        say(&mut ts, id, "@sudo neversaydie off"),
        [broadcast("You are once again mortal.")]
    );
    assert!(!ts.world.objects.get(player).unwrap().invincible());
    // sudo of a command above the account's level is not authorized either
    assert_eq!(
        say(&mut ts, id, "@sudo version"),
        [help("Unknown command: sudo")]
    );

    // a Sentinel player
    ts.world
        .objects
        .get_mut(player)
        .unwrap()
        .set_is_sentinel_prop(true);
    assert_eq!(
        say(&mut ts, id, "@neversaydie off"),
        [broadcast("You are once again mortal.")]
    );
    assert!(!ts.world.objects.get(player).unwrap().invincible());
    assert_eq!(
        say(&mut ts, id, "@portal_bypass"),
        [broadcast("You are no longer bound by portal restrictions.")]
    );
    assert_eq!(
        say(&mut ts, id, "@portal_bypass"),
        [broadcast(
            "You are once again bound by portal restrictions."
        )]
    );
    // console-only commands are refused silently in chat (NoConsoleInvoke)
    assert_eq!(say(&mut ts, id, "@version"), []);
    // cloak: the state property and ACE's messages; "player" needs an access level above Envoy
    assert_eq!(
        say(&mut ts, id, "@cloak player"),
        [broadcast("You do not have permission to do that state")]
    );
    assert_eq!(
        say(&mut ts, id, "@cloak on"),
        [broadcast(
            "You are now cloaked.\nYou are now ethereal and can pass through doors."
        )]
    );
    assert_eq!(
        ts.world
            .objects
            .get(player)
            .unwrap()
            .get_property(PropertyInt::CloakStatus),
        Some(2)
    );
    assert_eq!(say(&mut ts, id, "@cloak on"), [], "already on: nothing");
    assert_eq!(
        say(&mut ts, id, "@cloak sideways"),
        [broadcast("Please specify if you want cloaking on or off.")]
    );
    assert_eq!(
        say(&mut ts, id, "@cloak"),
        [
            help("Invalid parameter count, got 0, expected 1!"),
            broadcast("@cloak - Sets your cloaking state."),
            broadcast(&format!(
                "Usage: @cloak {}",
                command_manager::get_command_by_name("cloak")[0]
                    .attribute
                    .usage
            )),
        ]
    );
}

static RAW_SEEN: std::sync::Mutex<Vec<Vec<String>>> = std::sync::Mutex::new(Vec::new());

fn record_raw(_w: &mut empyrean_world::World, _session: Option<SessionId>, parameters: &[String]) {
    RAW_SEEN.lock().unwrap().push(parameters.to_vec());
}

#[test]
fn a_sudoed_raw_command_gets_its_raw_line() {
    // V361 (a fix): ACE dropped the raw line of a sudo'd IncludeRaw command. No command of
    // ACE's own sets IncludeRaw; an added one (a plugin's) does.
    use empyrean_command::command_handler_attribute::CommandHandlerAttribute;
    use empyrean_command::command_handler_flag::CommandHandlerFlag;
    use empyrean_command::command_handler_info::CommandHandlerInfo;
    let mut ts = server();
    let attribute = CommandHandlerAttribute::new(
        "u110-raw",
        AccessLevel::Sentinel,
        CommandHandlerFlag::None,
        true,
        -1,
        "",
        "",
    );
    let (handler, handler_name) = empyrean_command::handler!(record_raw);
    assert!(command_manager::try_add_command(
        Some(CommandHandlerInfo {
            handler,
            handler_name,
            attribute
        }),
        true
    ));
    let (id, session) = join(&mut ts, "u110alpha", ALPHA, "Alpha Tester", at(10.0, 10.0));
    ts.world.sessions.get_mut(session).unwrap().access_level = AccessLevel::Sentinel;

    let _ = say(&mut ts, id, "@sudo u110-raw bravo tester, be nice");
    ts.world
        .objects
        .get_mut(ObjectGuid::new(ALPHA))
        .unwrap()
        .set_is_sentinel_prop(true);
    let _ = say(&mut ts, id, "@u110-raw bravo tester, be nice");
    assert!(command_manager::try_remove_command("u110-raw"));

    let expected: Vec<String> = ["bravo tester, be nice", "bravo", "tester,", "be", "nice"]
        .map(str::to_owned)
        .to_vec();
    assert_eq!(
        *RAW_SEEN.lock().unwrap(),
        [expected.clone(), expected],
        "sudo'd and direct alike"
    );
}

#[test]
fn a_sentinel_boots_a_player_by_name_with_a_reason() {
    let mut ts = server();
    let (admin, _) = join(&mut ts, "u61alpha", ALPHA, "Alpha Tester", at(10.0, 10.0));
    let (target, _) = join(&mut ts, "u61bravo", BRAVO, "Bravo Tester", at(12.0, 10.0));
    ts.world
        .objects
        .get_mut(ObjectGuid::new(ALPHA))
        .unwrap()
        .set_is_sentinel_prop(true);

    assert_eq!(say(&mut ts, admin, "@boot char Nobody Here"), [broadcast(
        "Cannot boot \"Nobody Here\" because that character is not currently online or cannot be found. Check syntax/spelling and try again."
    )]);
    assert_eq!(
        say(&mut ts, admin, "@boot iid 5000000"),
        [broadcast(
            "That is not a valid Instance ID (IID). IIDs must be between 0x50000001 and 0x5FFFFFFF"
        )]
    );
    assert_eq!(say(&mut ts, admin, "@boot guild x"), [broadcast("You must specify what you are booting with char, account, or iid as the first parameter.")]);

    let replies = say(&mut ts, admin, "@boot char bravo tester, being rude");
    assert_eq!(
        replies[0],
        broadcast("Booting character bravo tester. Reason: being rude")
    );

    let booted = ts.received::<LoginAccountBooted>(target);
    assert_eq!(
        booted,
        [LoginAccountBooted {
            reason: Some(" - being rude".to_owned())
        }]
    );
}

/// `Convert.ToBase64String(Encoding.UTF8.GetBytes(s))`, written out for the test.
fn b64(s: &str) -> String {
    let alphabet: Vec<char> = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
        .chars()
        .collect();
    let mut bits = String::new();
    for b in s.bytes() {
        bits += &format!("{b:08b}");
    }
    let mut out: String = bits
        .as_bytes()
        .chunks(6)
        .map(|c| {
            let mut six = String::from_utf8(c.to_vec()).unwrap();
            while six.len() < 6 {
                six.push('0');
            }
            alphabet[usize::from_str_radix(&six, 2).unwrap()]
        })
        .collect();
    while !out.len().is_multiple_of(4) {
        out.push('=');
    }
    out
}

/// Empyrean's `@reportbug` (brand): the issue tracker's address and nothing about the player.
#[test]
fn reportbug_points_at_the_issue_tracker() {
    let mut ts = server();
    let (id, _) = join(&mut ts, "u61alpha", ALPHA, "Alpha Tester", at(10.0, 10.0));
    assert!(property_manager::modify_bool(
        &ts.world,
        "reportbug_enabled",
        true
    ));
    let expected = "\n\n\n\nBug Report - Open the following page in your browser to report a bug\n-=-\nhttps://github.com/dereth-network/dereth/issues\n-=-\n\n\n\n\n";
    assert_eq!(
        say(
            &mut ts,
            id,
            "@reportbug creature Drudge Prowler is over powered"
        ),
        [(expected.to_owned(), ChatMessageType::AdminTell.0)]
    );
    assert_eq!(
        say(&mut ts, id, "@reportbug"),
        [(expected.to_owned(), ChatMessageType::AdminTell.0)]
    );
}

/// Empyrean's `@source` (the AGPL source offer; ACE has none): every player gets the licence and the
/// source address, the build's repository unless `server.source_url` names another.
#[test]
fn source_names_the_licence_and_the_source() {
    let mut ts = server();
    let (id, _) = join(&mut ts, "u61alpha", ALPHA, "Alpha Tester", at(10.0, 10.0));
    let text = |url: &str| {
        broadcast(&format!(
            "Empyrean is free software, licensed under the GNU Affero General Public License v3.0 only (AGPL-3.0-only).\nThis server's source code: {url}"
        ))
    };
    assert_eq!(
        say(&mut ts, id, "@source"),
        [text("https://github.com/dereth-network/dereth")]
    );
    assert_eq!(
        say(&mut ts, id, "@SOURCE extra words"),
        [text("https://github.com/dereth-network/dereth")]
    );

    let mut config = empyrean_common::master_configuration::MasterConfiguration::default();
    config.server.source_url = " https://example.org/my-fork ".to_owned();
    let _fork = empyrean_common::config_manager::ConfigManager::override_for_thread(config);
    assert_eq!(
        say(&mut ts, id, "@source"),
        [text("https://example.org/my-fork")]
    );
}

/// ACE's `reportbug` handler builds ACE's URL; the command table uses Empyrean's handler.
#[test]
fn reportbug_builds_aces_url() {
    let mut ts = server_with(|c| {
        c.version(empyrean_content::models::world::Version {
            id: 1,
            base_version: Some("u61-base".to_owned()),
            patch_version: Some("u61-patch".to_owned()),
            last_modified: empyrean_common::dotnet::DotNetDateTime::MIN_VALUE,
        })
    });
    let (id, session) = join(&mut ts, "u61alpha", ALPHA, "Alpha Tester", at(10.0, 10.0));
    assert!(property_manager::modify_bool(
        &ts.world,
        "reportbug_enabled",
        true
    ));
    let loc = ts
        .world
        .objects
        .get(ObjectGuid::new(ALPHA))
        .unwrap()
        .location()
        .unwrap()
        .to_loc_string();

    let ace_reportbug = |ts: &mut TestServer, line: &str| {
        let before = ts.received::<CommunicationTextboxString>(id).len();
        let parameters: Vec<String> = line.split(' ').map(str::to_owned).collect();
        empyrean_command::handlers::player_commands::handle_reportbug(
            &mut ts.world,
            Some(session),
            &parameters,
        );
        ts.advance(0.5);
        ts.received::<CommunicationTextboxString>(id)[before..]
            .iter()
            .map(|m| (m.text.clone(), m.text_type))
            .collect::<Vec<_>>()
    };
    let got = ace_reportbug(&mut ts, "bogus I was killed");
    // an unknown category becomes Other; `sv` is ServerBuildInfo.FullVersion; the description
    // is trimmed (V365, a fix: ACE discarded its Trim, keeping the trailing space)
    let url = format!(
        "https://www.accpp.net/bug?sn={}&c={}&st={}&sv={}&pv={}&cg={}&l={}&i={}",
        b64("Empyrean"),
        b64("Alpha Tester"),
        b64("ACE"),
        b64(&empyrean_common::server_build_info::full_version()),
        b64("u61-patch"),
        b64("Other"),
        b64(&loc),
        b64("I was killed")
    );
    let expected = format!(
        "



Bug Report - Copy and Paste the following URL into your browser to submit a bug report
-=-
{url}
-=-




"
    );
    assert_eq!(got, [(expected, ChatMessageType::AdminTell.0)]);

    // npc: the category is upper-cased; with no target, no w/g
    let got = ace_reportbug(&mut ts, "NPC x");
    assert!(
        got[0].0.contains(&format!("&cg={}&l=", b64("NPC"))),
        "{}",
        got[0].0
    );
}

/// Myquests lists the players quest registry.
#[test]
fn myquests_lists_the_players_quest_registry() {
    use empyrean_world::managers::quest_manager::{self, QuestOwner};
    let mut ts = server_with(|c| {
        c.quest(empyrean_content::models::world::Quest {
            id: 1,
            name: "RatHunt".to_owned(),
            min_delta: 60,
            max_solves: 5,
            message: Some("rats".to_owned()),
            ..Default::default()
        })
    });
    let (id, _) = join(&mut ts, "i9alpha", ALPHA, "Alpha Tester", at(10.0, 10.0));
    assert!(empyrean_world::managers::property_manager::modify_bool(
        &ts.world,
        "quest_info_enabled",
        true
    ));
    assert_eq!(
        say(&mut ts, id, "@myquests"),
        [broadcast("Quest list is empty.")]
    );

    quest_manager::stamp(
        &mut ts.world,
        &mut QuestOwner::Creature(ObjectGuid::new(ALPHA)),
        "RatHunt@comment",
    );
    let q = quest_manager::get_quest(
        &ts.world,
        &QuestOwner::Creature(ObjectGuid::new(ALPHA)),
        "RatHunt",
    )
    .expect("stamped");
    let min_delta = {
        let rate = empyrean_world::managers::property_manager::get_double(
            &ts.world,
            "quest_mindelta_rate",
            0.0,
            true,
        )
        .item;
        empyrean_common::dotnet::CsCast::cs_cast(60.0 * rate)
    };
    let line: u32 = min_delta;
    assert_eq!(
        say(&mut ts, id, "@myquests"),
        [broadcast(&format!(
            "rathunt - 1 solves ({})\"rats\" 5 {line}",
            q.last_time_completed
        ))]
    );
}

/// `@debugcast` reads the player's body: moving or animating, the MoveTo's pending actions and
/// the animation playing (none for this setup, which has no motion table).
#[test]
fn debugcast_reads_the_players_body() {
    let mut ts = server();
    let (id, _) = join(&mut ts, "u61alpha", ALPHA, "Alpha Tester", at(10.0, 10.0));
    let _ = TestServer::take_not_ported();
    let lines = say(&mut ts, id, "@debugcast");
    assert_eq!(lines.len(), 4, "{lines:?}");
    assert_eq!(
        &lines[1..],
        [
            broadcast("IsMovingOrAnimating: False"),
            broadcast("PendingActions: 0"),
            broadcast("CurrAnim: ")
        ]
    );
    let unported = TestServer::take_not_ported();
    for member in [
        "ACE: MoveToManager.PendingActions",
        "ACE: Sequence.CurrAnim",
        "ACE: PhysicsObj.IsMovingOrAnimating",
    ] {
        assert!(!unported.contains_key(member), "{member}");
    }
}

/// `@check-collision` asks the player's body whether another body stands in it: another player
/// never does (players pass through each other), so the answer here is no.
#[test]
fn check_collision_asks_the_players_body() {
    let mut ts = server();
    let (id, session) = join(&mut ts, "u61alpha", ALPHA, "Alpha Tester", at(10.0, 10.0));
    let _ = join(&mut ts, "u61bravo", BRAVO, "Bravo Tester", at(10.5, 10.0));
    ts.world.sessions.get_mut(session).unwrap().access_level = AccessLevel::Developer;
    let _ = TestServer::take_not_ported();

    assert_eq!(
        say(&mut ts, id, "@sudo check-collision"),
        [broadcast("IsColliding: False")]
    );
    assert!(!TestServer::take_not_ported()
        .contains_key("ACE: PhysicsObj.ethereal_check_for_collisions"));
}
