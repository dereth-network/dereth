//! The operator's moderation, as Empyrean's: accounts made and changed while the world runs,
//! bans, gags and boots, at the console and said in game by admins. Fixture: new players in the
//! world, their accounts logged in with the harness's ticket.

use auberean_protocol::login::{ChatPacket, Text};
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use auberean_script::args::Arg;
use auberean_script::native_object::{NativeObject, TextInfo};
use dereth_protocol::turbine::{decode_incoming, ExtraBlob, IncomingPayload, SendToRoomById};
use yalain_store::Store as _;
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::admin::{self, Command, Issuer, Ticket};
use yalain_world::chat;
use yalain_world::graph::Builder;
use yalain_world::requests::function;

/// Run a console command: its answer.
fn console(h: &mut Harness, line: &str) -> Vec<String> {
    let c = admin::parse(line)
        .expect("a moderation command")
        .expect("it reads");
    let clock = h.clock();
    admin::run(&mut h.server, &mut h.net, clock, &Issuer::Console, &c)
}

/// The server's lines a client was shown since `from`.
fn notices(h: &Harness, b: usize, from: usize) -> Vec<String> {
    h.bots[b].inbox[from..]
        .iter()
        .filter_map(|r| match r.message() {
            Some(ServerMessage::Notice(n)) => {
                n.text.literal.map(|l| l.text().trim_end().to_owned())
            }
            _ => None,
        })
        .collect()
}

/// The string ids of the login-ending messages a client was sent since `from`.
fn ended(h: &Harness, b: usize, from: usize) -> Vec<(u32, u32)> {
    h.bots[b].inbox[from..]
        .iter()
        .filter(|r| !r.world)
        .filter_map(|r| match r.message() {
            Some(ServerMessage::AccountBooted(Text(t))) => Some((t.table_id, t.string_id)),
            _ => None,
        })
        .collect()
}

/// A text as the client packs one: a string reference holding a literal.
fn literal(text: &str) -> Arg {
    let mut b = Builder::new();
    b.native(
        106,
        NativeObject::TextInfo(TextInfo {
            has_literal: 1,
            literal: Some(text.encode_utf16().collect()),
            ..TextInfo::default()
        }),
    );
    Arg::Object(b.graph())
}

fn say(h: &mut Harness, b: usize, text: &str) {
    let call = server_call(function::SAY, &[Arg::Int(0), literal(text)]);
    h.bots[b].send(true, queue::WEENIE, &call);
    h.run(20);
}

/// How many sayings of a speaker a client heard since `from`.
fn heard(h: &Harness, b: usize, from: usize, speaker: u64) -> usize {
    h.world_messages(b, from)
        .iter()
        .filter(|m| {
            matches!(m, ServerMessage::CellObjectCall(c)
                if c.object.id == speaker && c.call.function.0 == chat::function::SAYING)
        })
        .count()
}

/// The room lines a client was sent by the chat service since `from`.
fn room_lines(h: &Harness, b: usize, from: usize) -> usize {
    h.bots[b].inbox[from..]
        .iter()
        .filter(|r| !r.world)
        .filter_map(|r| match r.message() {
            Some(ServerMessage::Chat(ChatPacket(p))) => decode_incoming(
                &[
                    &u32::try_from(p.len()).expect("small").to_le_bytes()[..],
                    &p,
                ]
                .concat(),
            )
            .ok()
            .map(|i| i.payload),
            _ => None,
        })
        .filter(|p| matches!(p, IncomingPayload::RoomEvent(_)))
        .count()
}

fn room_line(h: &mut Harness, b: usize, text: &str) {
    let packet = SendToRoomById {
        context: 7,
        room: 1,
        text: text.into(),
        extra: ExtraBlob::Speaker {
            sender: 1,
            chat_type: 100 << 16 | 1,
        },
    }
    .network_packet()
    .expect("packs");
    h.bots[b].send(false, 4, &ClientMessage::Chat(ChatPacket(packet)));
    h.run(20);
}

/// Does a login of this account with the harness's ticket get as far as the world's name, or is
/// it ended at once? Its client, and whether it got in.
fn logs_in(h: &mut Harness, account: &str) -> (usize, bool) {
    let b = h.add_bot();
    let now = h.now;
    h.bots[b].log_in(account, b"ticket", now);
    assert!(h.run_until(400, |h| !h.bots[b].inbox.is_empty()));
    h.run(10);
    let named = h.bots[b]
        .messages(false)
        .iter()
        .any(|m| matches!(m, ServerMessage::WorldName(_)));
    (b, named)
}

/// Accounts made at the console are kept with a bcrypt secret, a player's unless given the word
/// `admin`, and log in with their ticket; a name taken is refused. A new secret admits only itself
/// after. The admin flag set reaches the account's login at once. Accounts are listed with their ids
/// and whether each is an admin's. The old levels' command answers that they are gone, and a word
/// other than `admin` where a new account's rights go is how the command is written.
/// Behaviour: server.operator.accounts-at-the-console
#[test]
fn accounts_are_made_rekeyed_flagged_and_listed_at_the_console() {
    let mut h = Harness::new();
    let made = console(&mut h, "accountcreate Kara s3cret admin");
    assert_eq!(made.len(), 1);
    assert!(
        made[0].starts_with("Account successfully created for Kara (")
            && made[0].ends_with(") with access rights as an Admin."),
        "{made:?}"
    );
    let kara = h
        .server
        .store
        .account_by_name("kara")
        .expect("reads")
        .expect("made");
    assert!(kara.admin);
    let secret = kara.secret.expect("a secret");
    assert!(secret.starts_with(b"$2"), "kept as bcrypt");
    assert_ne!(secret, b"s3cret");
    assert_eq!(
        console(&mut h, "accountcreate kara other"),
        ["Account already exists. Try a new name."]
    );
    assert_eq!(
        console(&mut h, "accountcreate Ivo pw"),
        [format!(
            "Account successfully created for Ivo ({}) with access rights as a Player.",
            kara.id + 1
        )]
    );
    assert_eq!(
        console(&mut h, "set-accountpassword KARA ticket"),
        ["Account password for Kara successfully changed."]
    );
    let admits = |h: &mut Harness, t: &[u8]| {
        let unix = h.unix;
        matches!(
            admin::ticket(&mut h.server, "kara", t, unix),
            Ticket::Admitted(_)
        )
    };
    assert!(admits(&mut h, b"ticket"), "the new secret");
    assert!(!admits(&mut h, b"s3cret"), "not the old");
    let (_, got_in) = logs_in(&mut h, "Kara");
    assert!(got_in);
    assert_eq!(
        console(&mut h, "set-accountadmin kara off"),
        ["Account Kara updated with access rights set as a Player."]
    );
    let held = |h: &Harness| {
        h.server
            .sessions
            .values()
            .find(|x| x.account.name == "Kara")
            .map(|x| x.account.admin)
    };
    assert_eq!(held(&h), Some(false), "its login holds the new flag");
    assert_eq!(
        console(&mut h, "set-accountadmin kara"),
        ["Account Kara updated with access rights set as an Admin."]
    );
    assert_eq!(held(&h), Some(true));
    assert_eq!(
        console(&mut h, "set-accountadmin nobody on"),
        ["Account nobody does not exist."]
    );
    console(&mut h, "set-accountadmin kara off");
    assert_eq!(
        console(&mut h, "accountlist"),
        [
            "2 accounts:".to_owned(),
            format!("User: Ivo, ID: {}, Access: Player", kara.id + 1),
            format!("User: Kara, ID: {}, Access: Player", kara.id),
        ]
    );
    assert_eq!(
        console(&mut h, "accountget ivo"),
        [format!("User: Ivo, ID: {}, Access: Player", kara.id + 1)]
    );
    assert!(matches!(
        admin::parse("accountcreate a b wizard"),
        Some(Err(_))
    ));
    assert!(matches!(admin::parse("accountcreate a b 5"), Some(Err(_))));
    assert!(matches!(
        admin::parse("set-accountaccess a 5"),
        Some(Err(e)) if e.contains("set-accountadmin")
    ));
    assert_eq!(admin::parse("say hello"), None, "not a moderation command");
}

/// A ban boots the account's login at once, and keeps its logins out until it ends: each is
/// ended before the world's name with the client's line that the account is banned, and its
/// friends ticket refused as banned (a wrong secret only as wrong). The ban list names it, who
/// banned it, until when and why. Once the ban has run out the account logs in, and the ban is
/// lifted; an unban lifts one at once. A length below zero is refused.
/// Behaviour: server.operator.ban-keeps-an-account-out
#[test]
fn a_ban_boots_the_account_and_keeps_it_out_until_it_ends_or_is_lifted() {
    let mut h = Harness::new();
    let (a, elara) = h.player_in_world("one", "Elara");
    let from = h.bots[a].inbox.len();
    let answer = console(&mut h, "ban one 0 0 10 spamming the market");
    assert_eq!(
        answer,
        [
            "Booting account one. Reason: spamming the market",
            "Banned account one for 0 days, 0 hours and 10 minutes. Reason: spamming the market",
        ]
    );
    h.run(10);
    assert_eq!(
        ended(&h, a, from),
        [(admin::STRINGS, admin::BOOTED)],
        "its login is told it was booted"
    );
    h.step_by(admin::BOOT_CLOSE_AFTER);
    h.run(5);
    assert!(
        !h.server.sessions.values().any(|x| x.account.name == "one"),
        "its login is closed"
    );
    h.run_to(yalain_net::LocalTime(h.now.0 + 20.0));
    assert!(
        !h.server.players.values().any(|p| p.character == elara),
        "its character has left the world"
    );

    let (b, got_in) = logs_in(&mut h, "one");
    assert!(!got_in, "a banned login gets no further");
    assert_eq!(ended(&h, b, 0), [(admin::STRINGS, admin::BANNED)]);
    let unix = h.unix;
    assert_eq!(
        admin::ticket(&mut h.server, "one", b"ticket", unix),
        Ticket::Banned
    );
    assert_eq!(
        admin::ticket(&mut h.server, "one", b"wrong", unix),
        Ticket::Refused
    );
    let list = console(&mut h, "banlist");
    assert_eq!(list.len(), 2);
    assert_eq!(list[0], "The following accounts are banned:");
    assert!(
        list[1].starts_with("one -- banned by CONSOLE until ")
            && list[1].ends_with(" UTC -- Reason: spamming the market"),
        "{list:?}"
    );

    h.advance(600.0);
    let (_, got_in) = logs_in(&mut h, "one");
    assert!(got_in, "the ban has run out");
    let kept = h.server.store.account_by_name("one").expect("reads");
    assert_eq!(kept.and_then(|a| a.ban), None, "and is lifted");
    assert_eq!(
        console(&mut h, "banlist"),
        ["There are no accounts currently banned."]
    );

    console(&mut h, "ban one 2 0 0");
    assert_eq!(console(&mut h, "unban ONE"), ["UnBanned account one."]);
    assert_eq!(
        console(&mut h, "unban one"),
        ["Cannot unban \"one\" because that account is not banned."]
    );
    let (_, got_in) = logs_in(&mut h, "one");
    assert!(got_in, "unbanned");
    assert_eq!(
        console(&mut h, "ban one 0 -1 0"),
        ["Hours must not be less than 0."]
    );
    assert_eq!(
        console(&mut h, "ban nobody 1 0 0")[0],
        "Cannot ban \"nobody\" because that account cannot be found in database. Check \
         syntax/spelling and try again."
    );
}

/// A gagged character is told so, and then says, tells, emotes and speaks into a room to nobody,
/// told each time it may not; others talk on, to it too. Its gag runs five minutes of its time
/// in the world, then it is told it may talk again, and is heard.
/// Behaviour: server.operator.gag-stops-chat-five-minutes
#[test]
fn a_gagged_character_is_heard_by_nobody_for_five_minutes_in_the_world() {
    let mut h = Harness::new();
    let (a, elara) = h.player_in_world("one", "Elara");
    let (b, babyfare) = h.player_in_world("two", "Babyfare");
    h.run(10);
    let from = [h.bots[a].inbox.len(), h.bots[b].inbox.len()];
    assert_eq!(
        console(&mut h, "gag elara"),
        ["Elara has been gagged for five minutes."]
    );
    h.run(10);
    assert_eq!(notices(&h, a, from[0]), [admin::GAG_NOTICE]);
    say(&mut h, a, "Hello?");
    let tell = server_call(
        function::TELL,
        &[Arg::Int(0), literal("Psst."), literal("Babyfare")],
    );
    h.bots[a].send(true, queue::WEENIE, &tell);
    h.run(20);
    let wave = chat::EMOTES[2];
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::EMOTE, &[Arg::Int(wave.id)]),
    );
    h.run(20);
    room_line(&mut h, a, "Anyone?");
    assert_eq!(heard(&h, b, from[1], elara), 0, "nobody hears it");
    assert_eq!(heard(&h, a, from[0], elara), 0, "not even itself");
    let told = h.bots[b].inbox[from[1]..].iter().any(|r| {
        matches!(r.message(), Some(ServerMessage::PlayerCall(c)) if c.function.0 == chat::function::HEAR_TELL)
    });
    assert!(!told, "its tell reaches nobody");
    assert!(
        !h.world_messages(b, from[1])
            .iter()
            .any(|m| matches!(m, ServerMessage::PlayBehavior(_))),
        "its emote plays for nobody"
    );
    assert_eq!(
        room_lines(&h, b, from[1]),
        0,
        "its room line reaches nobody"
    );
    assert_eq!(
        notices(&h, a, from[0])[1..],
        [admin::GAGGED; 4],
        "told each time"
    );
    let fb = h.bots[a].inbox.len();
    say(&mut h, b, "You there?");
    assert_eq!(heard(&h, a, fb, babyfare), 1, "it hears others");

    // Five minutes of its time in the world, then the gag is over.
    let begun = h.now.0;
    h.run_to(yalain_net::LocalTime(begun + 280.0));
    assert!(h.server.gags.gagged(elara), "still gagged at 4:40");
    let fa = h.bots[a].inbox.len();
    h.run_to(yalain_net::LocalTime(begun + 300.0));
    assert!(!h.server.gags.gagged(elara), "over at five minutes");
    assert_eq!(notices(&h, a, fa), [admin::UNGAG_NOTICE]);
    assert_eq!(h.server.store.gag(elara).expect("reads"), None);
    let fb = h.bots[b].inbox.len();
    say(&mut h, a, "Back.");
    assert_eq!(heard(&h, b, fb, elara), 1, "heard again");
}

/// A gag waits out of the world: a gagged character that leaves keeps the rest of its gag, and is
/// told and gagged again as it comes back; a character gagged out of the world is gagged as it
/// enters. Ungagged, it is told, and talks at once. A name no character has is refused.
/// Behaviour: server.operator.gag-stops-chat-five-minutes
#[test]
fn a_gag_waits_out_of_the_world_and_ungag_lifts_it_at_once() {
    let mut h = Harness::new();
    let (a, elara) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Babyfare");
    let tamsin = h.create_character(b, "two", "Tamsin");
    console(&mut h, "gag Elara");
    assert_eq!(
        console(&mut h, "gag tamsin"),
        ["Tamsin has been gagged for five minutes."]
    );
    assert_eq!(h.server.store.gag(tamsin).expect("reads"), Some(300.0));
    h.run_to(yalain_net::LocalTime(h.now.0 + 100.0));
    h.bots[a].leave();
    h.run_to(yalain_net::LocalTime(h.now.0 + 20.0));
    assert!(!h.server.players.values().any(|p| p.character == elara));
    let left = h.server.store.gag(elara).expect("reads").expect("kept");
    assert!(
        (180.0..=205.0).contains(&left),
        "the rest of its gag, counted only in the world: {left}"
    );
    // Out of the world, nothing runs.
    h.run_to(yalain_net::LocalTime(h.now.0 + 400.0));
    let c = h.at_character_list("one");
    let from = h.bots[c].inbox.len();
    h.enter_world(c, "one", elara);
    assert_eq!(
        notices(&h, c, from),
        [admin::GAG_NOTICE],
        "told as it enters"
    );
    assert!(h.server.gags.gagged(elara));
    let from = h.bots[c].inbox.len();
    assert_eq!(console(&mut h, "ungag elara"), ["Elara has been ungagged."]);
    h.run(10);
    assert_eq!(notices(&h, c, from), [admin::UNGAG_NOTICE]);
    let fb = h.bots[b].inbox.len();
    say(&mut h, c, "Free.");
    assert_eq!(heard(&h, b, fb, elara), 1);
    assert_eq!(h.server.store.gag(elara).expect("reads"), None);
    assert_eq!(
        console(&mut h, "gag Nobody Here"),
        ["Unable to gag a character named Nobody Here, check the name and re-try the command."]
    );
}

/// A boot ends a login, named by its character, its account or its character's id: its client is
/// sent the message that ends a login with the client's own line that its character was booted,
/// its connection is closed a moment later, and its character leaves the world as when a client
/// goes away. One not online is answered so.
/// Behaviour: server.operator.boot-ends-a-login
#[test]
fn a_boot_ends_the_login_and_its_character_leaves_the_world() {
    let mut h = Harness::new();
    let (a, elara) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Babyfare");
    let (c, tamsin) = h.player_in_world("three", "Tamsin");
    let from = [
        h.bots[a].inbox.len(),
        h.bots[b].inbox.len(),
        h.bots[c].inbox.len(),
    ];
    assert_eq!(
        console(&mut h, "boot char elara, away too long"),
        ["Booting character elara. Reason: away too long"]
    );
    assert_eq!(
        console(&mut h, "boot account TWO"),
        ["Booting account TWO."]
    );
    let iid = format!("boot iid {tamsin:#018x}");
    assert_eq!(
        console(&mut h, &iid),
        [format!("Booting instance id {tamsin:#018x}.")]
    );
    h.run(10);
    for (bot, f) in [(a, from[0]), (b, from[1]), (c, from[2])] {
        assert_eq!(ended(&h, bot, f), [(admin::STRINGS, admin::BOOTED)]);
    }
    assert_eq!(
        h.server.sessions.len(),
        3,
        "not closed before the line goes"
    );
    h.step_by(admin::BOOT_CLOSE_AFTER);
    h.run(5);
    assert!(h.server.sessions.is_empty(), "every login closed");
    assert!(
        h.server.players.values().any(|p| p.character == elara),
        "its character lingers"
    );
    h.run_to(yalain_net::LocalTime(h.now.0 + 20.0));
    assert!(h.server.players.is_empty(), "and has left the world");
    assert_eq!(
        console(&mut h, "boot char elara"),
        [
            "Cannot boot \"elara\" because that character is not currently online or cannot be \
          found. Check syntax/spelling and try again."
        ]
    );
    assert!(matches!(
        admin::parse("boot player elara"),
        Some(Err(e)) if e.starts_with("You must specify what you are booting")
    ));
}

/// An admin's account says the moderation commands in game, after `@`, aloud or into a room: its
/// `@gag` gags, is answered in its chat window and heard by nobody. A player's `@ban` line is said
/// as any other, and bans nobody; an issuer who is no admin is refused by the command itself too.
/// Behaviour: server.operator.admin-commands-said-in-game
#[test]
fn admins_say_the_commands_in_game_and_a_players_line_is_only_said() {
    let mut h = Harness::new();
    let (a, elara) = h.player_in_world("one", "Elara");
    let (b, babyfare) = h.player_in_world("two", "Babyfare");
    console(&mut h, "set-accountadmin one");
    let from = [h.bots[a].inbox.len(), h.bots[b].inbox.len()];
    say(&mut h, a, "@gag Babyfare");
    assert!(h.server.gags.gagged(babyfare), "the command runs");
    assert_eq!(heard(&h, b, from[1], elara), 0, "nobody hears it");
    assert_eq!(heard(&h, a, from[0], elara), 0);
    assert_eq!(
        notices(&h, a, from[0]),
        ["Babyfare has been gagged for five minutes."]
    );
    assert_eq!(notices(&h, b, from[1]), [admin::GAG_NOTICE]);
    let from = h.bots[a].inbox.len();
    room_line(&mut h, a, "@ungag babyfare");
    assert!(!h.server.gags.gagged(babyfare), "said into a room too");
    assert_eq!(notices(&h, a, from), ["Babyfare has been ungagged."]);

    let from = h.bots[a].inbox.len();
    say(&mut h, b, "@ban one 1 0 0");
    assert_eq!(heard(&h, a, from, babyfare), 1, "a player's line is said");
    let one = h.server.store.account_by_name("one").expect("reads");
    assert_eq!(one.and_then(|x| x.ban), None, "and bans nobody");

    let player = Issuer::Account {
        name: "two".into(),
        admin: false,
        groups: vec![],
        character: "Babyfare".into(),
    };
    let c = Command::Gag {
        name: "Elara".into(),
    };
    let clock = h.clock();
    assert_eq!(
        admin::run(&mut h.server, &mut h.net, clock, &player, &c),
        [admin::NOT_AN_ADMIN]
    );
    assert!(!h.server.gags.gagged(elara), "and gags nobody");
}

/// `@dn verify <token>` needs an admin's account: an admin's line is kept for the network's half
/// marked as an admin's; a player's line is kept as any other of its `@dn` words, for the friends
/// service to answer. Nobody hears any of them.
/// Behaviour: server.network.verify-in-game-for-admins
#[test]
fn dn_verify_said_in_game_is_only_an_admins() {
    let mut h = Harness::new();
    let (a, elara) = h.player_in_world("one", "Elara");
    let (b, babyfare) = h.player_in_world("two", "Babyfare");
    let (c, cara) = h.player_in_world("three", "Cara");
    console(&mut h, "set-accountadmin one");
    let token = "dnv_abcdefghijklmnopqrstuvwxyzAB-_01";
    let from = [
        h.bots[a].inbox.len(),
        h.bots[b].inbox.len(),
        h.bots[c].inbox.len(),
    ];
    say(&mut h, a, &format!("@dn verify {token}"));
    say(&mut h, b, &format!("@DN Verify {token}"));
    room_line(&mut h, c, &format!("@dn verify {token}"));
    say(&mut h, b, "@dn on");
    let kept: Vec<(u64, String, bool)> = h
        .server
        .network_commands
        .drain(..)
        .map(|k| (k.character, k.words, k.admin))
        .collect();
    assert_eq!(
        kept,
        [
            (elara, format!("verify {token}"), true),
            (babyfare, format!("Verify {token}"), false),
            (cara, format!("verify {token}"), false),
            (babyfare, "on".to_owned(), false),
        ]
    );
    assert_eq!(notices(&h, a, from[0]), Vec::<String>::new());
    for (bot, f) in [(a, from[0]), (b, from[1]), (c, from[2])] {
        for speaker in [elara, babyfare, cara] {
            assert_eq!(heard(&h, bot, f, speaker), 0, "nobody hears it");
        }
        assert_eq!(room_lines(&h, bot, f), 0, "nor in the room");
    }
    assert!(chat::is_verify("VERIFY x") && !chat::is_verify("verified") && !chat::is_verify(""));
}

/// `@dn admin ...` needs an admin's account, as `@dn verify` does: an admin's line is kept marked as
/// an admin's; a player's is kept as any other of its `@dn` words, which the friends service answers
/// with its help. Nobody hears any of them.
/// Behaviour: server.network.operator-words-in-game-for-admins
#[test]
fn dn_admin_said_in_game_is_only_an_admins() {
    let mut h = Harness::new();
    let (a, elara) = h.player_in_world("one", "Elara");
    let (c, cara) = h.player_in_world("three", "Cara");
    console(&mut h, "set-accountadmin one");
    let from = [h.bots[a].inbox.len(), h.bots[c].inbox.len()];
    say(&mut h, a, "@dn admin block coldeve");
    room_line(&mut h, c, "@dn admin block coldeve");
    let kept: Vec<(u64, String, bool)> = h
        .server
        .network_commands
        .drain(..)
        .map(|k| (k.character, k.words, k.admin))
        .collect();
    assert_eq!(
        kept,
        [
            (elara, "admin block coldeve".to_owned(), true),
            (cara, "admin block coldeve".to_owned(), false),
        ]
    );
    for (bot, f) in [(a, from[0]), (c, from[1])] {
        for speaker in [elara, cara] {
            assert_eq!(heard(&h, bot, f, speaker), 0, "nobody hears it");
        }
        assert_eq!(room_lines(&h, bot, f), 0, "nor in the room");
    }
    assert!(
        chat::is_admin_word("ADMIN x")
            && !chat::is_admin_word("admins")
            && !chat::is_admin_word("")
    );
}

/// Each line of a moderation command's answer says whether it was done or why nothing was, for the
/// console's tags: a boot of one online is done, of one not online or by an id that does not read
/// refused; a gag of a character done, of nobody refused; an account made again, a lift of no ban
/// and a negative ban refused; a list done; each command answers under its own word.
/// Behaviour: server.operator.moderation-answers-done-or-refused
#[test]
fn a_moderation_commands_answer_says_whether_it_was_done() {
    let mut h = Harness::new();
    let (_, _) = h.player_in_world("one", "Elara");
    let mut ask = |line: &str| {
        let c = admin::parse(line)
            .expect("a moderation command")
            .expect("it reads");
        let clock = h.clock();
        (
            c.word(),
            admin::answered(&mut h.server, &mut h.net, clock, &Issuer::Console, &c),
        )
    };
    let done = |a: &[admin::Answer]| a.iter().all(Result::is_ok);
    let refused = |a: &[admin::Answer]| a.iter().all(Result::is_err);
    let (word, a) = ask("gag Elara");
    assert_eq!(word, "gag");
    assert!(done(&a), "{a:?}");
    let (_, a) = ask("gag Nobody");
    assert!(refused(&a), "{a:?}");
    let (_, a) = ask("accountcreate one again");
    assert!(refused(&a), "{a:?}");
    let (_, a) = ask("unban one");
    assert!(refused(&a), "{a:?}");
    let (_, a) = ask("ban one 0 0 -5");
    assert!(refused(&a), "{a:?}");
    let (word, a) = ask("accountlist");
    assert_eq!(word, "accountlist");
    assert!(a.len() > 1 && done(&a), "{a:?}");
    let (_, a) = ask("boot iid 12");
    assert!(refused(&a), "{a:?}");
    let (_, a) = ask("boot char Nobody");
    assert!(refused(&a), "{a:?}");
    let (word, a) = ask("BOOT char elara");
    assert_eq!(word, "boot");
    assert_eq!(a, [Ok("Booting character elara.".to_owned())]);
}

/// `gamecast` shows every player in the world `Broadcast from <who>> <message>`, as Empyrean's:
/// from the console, `System` and the message as typed; said in game by an admin, its character's
/// name. A player's is only said, and a gamecast with no message is how it is written.
/// Behaviour: server.operator.gamecast-shows-everyone-a-message
#[test]
fn gamecast_shows_every_player_in_the_world_the_message() {
    let mut h = Harness::new();
    let (a, _) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Babyfare");
    let from = [h.bots[a].inbox.len(), h.bots[b].inbox.len()];
    let said = console(&mut h, "gamecast Back in  five minutes. ");
    h.run(5);
    for (bot, f) in [(a, from[0]), (b, from[1])] {
        assert_eq!(
            notices(&h, bot, f),
            ["Broadcast from System> Back in  five minutes."]
        );
    }
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].contains("2 players"), "{said:?}");

    console(&mut h, "set-accountadmin one");
    let from = [h.bots[a].inbox.len(), h.bots[b].inbox.len()];
    say(&mut h, a, "@gamecast Hello, all.");
    assert!(
        notices(&h, b, from[1]).contains(&"Broadcast from Elara> Hello, all.".to_owned()),
        "{:?}",
        notices(&h, b, from[1])
    );
    let from = [h.bots[a].inbox.len(), h.bots[b].inbox.len()];
    say(&mut h, b, "@gamecast Not mine to say.");
    assert!(notices(&h, a, from[0]).is_empty(), "nothing shown");
    assert!(matches!(admin::parse("gamecast"), Some(Err(_))));
}

/// A player changes its own account's password in game, as Empyrean's `@passwd`: a wrong old
/// password is refused and changes nothing; a second try within five seconds is refused whatever
/// it holds; the right one keeps the new password, which then admits the account and the old one
/// no longer does; fewer than two words is answered with the count and the command's use. None of
/// it is heard by anyone.
/// Behaviour: server.operator.passwd-changes-your-own-password
#[test]
fn a_player_changes_its_own_password_in_game_only_with_the_right_old_one() {
    let mut h = Harness::new();
    let (a, ida) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Babyfare");
    h.run(10);
    let admits = |h: &mut Harness, secret: &[u8]| {
        let unix = h.unix;
        matches!(
            admin::ticket(&mut h.server, "one", secret, unix),
            Ticket::Admitted(_)
        )
    };
    assert!(admits(&mut h, b"ticket"));
    let from = [h.bots[a].inbox.len(), h.bots[b].inbox.len()];
    say(&mut h, a, "@passwd wrong n3w-secret");
    assert_eq!(
        notices(&h, a, from[0]),
        [
            "Unable to change password: Password provided in first parameter does not match \
          current account password for this account!"
        ]
    );
    assert!(admits(&mut h, b"ticket") && !admits(&mut h, b"n3w-secret"));
    let fa = h.bots[a].inbox.len();
    say(&mut h, a, "@passwd ticket n3w-secret");
    assert_eq!(
        notices(&h, a, fa),
        ["This command may only be run once every 5 seconds."]
    );
    assert!(admits(&mut h, b"ticket"), "nothing changed");
    h.advance(admin::PASSWD_WAIT);
    let fa = h.bots[a].inbox.len();
    say(&mut h, a, "@PASSWD ticket \"n3w secret\"");
    assert_eq!(
        notices(&h, a, fa),
        ["Account password successfully changed."]
    );
    assert!(admits(&mut h, b"n3w secret") && !admits(&mut h, b"ticket"));
    let kept = h
        .server
        .store
        .account_by_name("one")
        .expect("reads")
        .and_then(|x| x.secret)
        .expect("a secret");
    assert!(kept.starts_with(b"$2"), "kept as a bcrypt secret");
    h.advance(admin::PASSWD_WAIT);
    let fa = h.bots[a].inbox.len();
    say(&mut h, a, "@passwd n3w");
    assert_eq!(
        notices(&h, a, fa),
        [
            "Invalid parameter count, got 1, expected 2!\n@passwd - Change your account password.\n\
             Usage: @passwd oldpassword newpassword",
        ]
    );
    assert!(admits(&mut h, b"n3w secret"));
    assert_eq!(heard(&h, b, from[1], ida), 0, "heard by nobody");
}

/// A long answer to a command said in game is one notice with its lines in order: separate notices
/// of different lengths sent in a row could overtake each other on the wire.
/// Behaviour: server.admin.a-long-answer-is-one-message
#[test]
fn a_long_ban_list_said_in_game_comes_as_one_notice_in_its_order() {
    let mut h = Harness::new();
    let (a, _) = h.player_in_world("one", "Elara");
    console(&mut h, "set-accountadmin one");
    for (i, reason) in [
        "a long reason given at great length, and then some",
        "x",
        "y",
    ]
    .iter()
    .enumerate()
    {
        h.at_character_list(&format!("banned{i}"));
        console(&mut h, &format!("ban banned{i} 1 0 0 {reason}"));
    }
    h.run(10);
    let from = h.bots[a].inbox.len();
    say(&mut h, a, "@banlist");
    let said = notices(&h, a, from);
    assert_eq!(said.len(), 1, "one notice: {said:?}");
    let lines: Vec<&str> = said[0].lines().collect();
    assert_eq!(lines.len(), 4, "{lines:?}");
    assert_eq!(lines[0], "The following accounts are banned:");
    for (i, line) in lines[1..].iter().enumerate() {
        assert!(line.starts_with(&format!("banned{i} ")), "{line}");
    }
}
