//! What admin characters send: plain script calls of the admin family, each for admin characters
//! only. Fixture: new players in the world; an admin character is one of an admin's account made so
//! at the console before it entered.

use auberean_protocol::login::Empty;
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use auberean_script::args::Arg;
use auberean_script::native_object::{NativeObject, TextInfo};
use yalain_store::Store as _;
use yalain_testkit::play::server_call;
use yalain_testkit::{Harness, TEST_CREATURE, TEST_ITEM};
use yalain_world::admin::{self, Issuer};
use yalain_world::admin_calls::{self, ethereal_value, other, LOG_EVENT};
use yalain_world::calls::{Node, Value};
use yalain_world::graph::{Builder, Slot};
use yalain_world::player::quality;
use yalain_world::requests::function;
use yalain_world::{allegiance, combat, fellowship, quals, skill_use, trade};

/// The natives that send an event encrypted and in the clear.
const SEND_ENCRYPTED: u32 = 0x7F << 16 | 0x37;
const SEND_PLAIN: u32 = 0x7F << 16 | 0x36;

/// Run a console command: its answer.
fn console(h: &mut Harness, line: &str) -> Vec<String> {
    let c = admin::parse(line)
        .expect("a moderation command")
        .expect("it reads");
    let clock = h.clock();
    admin::run(&mut h.server, &mut h.net, clock, &Issuer::Console, &c)
}

/// A character of an admin's account that entered as an admin character.
fn admin_in_world(h: &mut Harness, account: &str, name: &str) -> (usize, u64) {
    let b = h.at_character_list(account);
    let id = h.create_character(b, account, name);
    console(h, &format!("set-accountadmin {account}"));
    console(h, &format!("set-characteradmin {name}"));
    h.enter_world(b, account, id);
    (b, id)
}

/// A string as a call's value.
fn text(t: &str) -> Arg {
    let mut b = Builder::new();
    b.native(118, NativeObject::Text(t.encode_utf16().collect()));
    Arg::Object(b.graph())
}

/// A string reference as a call's value.
fn info(t: &str) -> Arg {
    let mut b = Builder::new();
    b.native(
        106,
        NativeObject::TextInfo(TextInfo {
            has_literal: 1,
            literal: Some(t.encode_utf16().collect()),
            ..TextInfo::default()
        }),
    );
    Arg::Object(b.graph())
}

/// Send a call, its parameters as declared (they travel last first).
fn send(h: &mut Harness, b: usize, f: u32, args: &[Arg]) {
    let args: Vec<Arg> = args.iter().rev().cloned().collect();
    h.bots[b].send(true, queue::WEENIE, &server_call(f, &args));
    h.run(10);
}

/// The answers a client was sent since `from`: the lines for the admin's log.
fn answers(h: &Harness, b: usize, from: usize) -> Vec<String> {
    h.calls(b, from)
        .into_iter()
        .filter(|(f, _)| *f == LOG_EVENT)
        .filter_map(|(_, v)| match v.first().and_then(Value::root) {
            Some(Node::Text(t)) => Some(t.clone()),
            _ => None,
        })
        .flat_map(|t| t.lines().map(str::to_owned).collect::<Vec<_>>())
        .collect()
}

/// A health of a player.
fn health(h: &Harness, id: u64) -> i32 {
    let p = h
        .server
        .players
        .values()
        .find(|p| p.character == id)
        .expect("in the world");
    quals::int(&p.qualities, quality::HEALTH).unwrap_or(0)
}

/// Every call of the admin family and the admin's calls of the other families: from a player's
/// account, and from an ordinary character of an admin's account, each is refused and counted and
/// nothing comes back; a call that does not even read is refused the same, before it is read. An
/// admin character's is let through.
/// Behaviour: server.admin.calls-refused-for-anyone-else
#[test]
fn every_admin_call_is_refused_for_anyone_but_an_admin_character() {
    let mut h = Harness::new();
    let (player, _) = h.player_in_world("one", "Elara");
    let (ordinary, _) = h.player_in_world("two", "Babyfare");
    console(&mut h, "set-accountadmin two");
    let mut all = admin_calls::admin_family();
    all.extend(other::ALL);
    for b in [player, ordinary] {
        for f in &all {
            let before = h.server.admin_calls.refused.get(f).copied().unwrap_or(0);
            let from = h.bots[b].inbox.len();
            h.bots[b].send(true, queue::WEENIE, &server_call(*f, &[]));
            h.run(2);
            assert_eq!(
                h.server.admin_calls.refused.get(f).copied().unwrap_or(0),
                before + 1,
                "{f:#x}"
            );
            assert!(answers(&h, b, from).is_empty(), "{f:#x} is not answered");
        }
    }
    let (me, _) = admin_in_world(&mut h, "three", "Cara");
    let refused = h.server.admin_calls.refused.clone();
    let from = h.bots[me].inbox.len();
    send(&mut h, me, 0x8201_002F, &[]);
    assert_eq!(h.server.admin_calls.refused, refused, "not refused");
    assert_eq!(answers(&h, me, from).len(), 1);
}

/// A family of real commands, each with values that would do harm: none has any effect from a
/// player's account, or from an ordinary character of an admin's. Boot, ban, smite, exterminate,
/// money, a make, a teleport, a gamecast and a rename each do what they say from an admin character,
/// so the tests would see the harm.
/// Behaviour: server.admin.commands-do-nothing-for-anyone-else
#[test]
fn a_players_admin_calls_do_nothing_and_an_admins_do_what_they_say() {
    let mut h = Harness::new();
    h.add_creatures(1);
    let (victim, victim_id) = h.player_in_world("zero", "Victim");
    let (player, _) = h.player_in_world("one", "Elara");
    let (ordinary, _) = h.player_in_world("two", "Babyfare");
    console(&mut h, "set-accountadmin two");
    h.run(30);
    let creatures = |h: &Harness| h.server.things.placed().filter(|t| t.health > 0).count();
    let before = creatures(&h);
    assert!(before > 0, "a creature to exterminate");
    let harm = |h: &mut Harness, b: usize| {
        let victim_arg = Arg::LongInt(victim_id);
        send(
            h,
            b,
            0x8201_0092,
            &[Arg::Int(0), text("zero"), text(""), Arg::LongInt(0)],
        );
        send(
            h,
            b,
            0x8201_0016,
            &[
                Arg::Int(0),
                text("zero"),
                Arg::Int(1),
                Arg::Int(0),
                Arg::Int(0),
            ],
        );
        send(h, b, other::SMITE, std::slice::from_ref(&victim_arg));
        send(h, b, 0x8201_013B, &[]);
        send(h, b, other::MONEY, &[Arg::Int(1000)]);
        send(h, b, 0x8201_006A, &[text(&format!("{TEST_ITEM:08X}"))]);
        send(h, b, 0x8201_006F, &[text(&format!("{TEST_CREATURE:08X}"))]);
        send(h, b, 0x8201_0058, &[text("Victim"), text("10.0n 10.0e")]);
        send(h, b, 0x8201_0134, &[info("Hello from a player.")]);
        send(h, b, 0x8201_0130, &[victim_arg, text("Stolen")]);
    };
    for b in [player, ordinary] {
        let items = h
            .server
            .players
            .values()
            .map(|p| p.play.items.len())
            .sum::<usize>();
        let shown = h.bots[victim].inbox.len();
        harm(&mut h, b);
        let kept = h
            .server
            .store
            .account_by_name("zero")
            .expect("reads")
            .expect("kept");
        assert_eq!(kept.ban, None, "not banned");
        assert!(
            h.server.sessions.values().any(|x| x.account.name == "zero"),
            "not booted"
        );
        assert!(health(&h, victim_id) > 0, "not smitten");
        assert_eq!(creatures(&h), before, "nothing exterminated or made");
        assert_eq!(
            h.server
                .players
                .values()
                .map(|p| p.play.items.len())
                .sum::<usize>(),
            items,
            "nothing made in a pack"
        );
        let p = h
            .server
            .players
            .values()
            .find(|p| p.name == "Elara")
            .expect("in the world");
        assert_eq!(yalain_world::store::money(&p.qualities), 0, "no money made");
        assert!(
            h.server.players.values().all(|p| p.trip.is_none()),
            "nobody sent anywhere"
        );
        assert!(
            !h.bots[victim].inbox[shown..]
                .iter()
                .any(|r| matches!(r.message(), Some(ServerMessage::Notice(_)))),
            "nothing shown"
        );
        assert_eq!(
            h.server
                .store
                .character(victim_id)
                .expect("reads")
                .expect("kept")
                .name,
            "Victim",
            "not renamed"
        );
    }
    // And the same commands, from an admin character.
    let (me, _) = admin_in_world(&mut h, "three", "Cara");
    let shown = h.bots[victim].inbox.len();
    send(&mut h, me, 0x8201_0134, &[info("Hello from an admin.")]);
    assert!(
        h.bots[victim].inbox[shown..]
            .iter()
            .any(|r| matches!(r.message(), Some(ServerMessage::Notice(_)))),
        "the gamecast is shown"
    );
    send(&mut h, me, other::MONEY, &[Arg::Int(1000)]);
    let p = h
        .server
        .players
        .values()
        .find(|p| p.name == "Cara")
        .expect("in the world");
    assert_eq!(yalain_world::store::money(&p.qualities), 1000);
    send(
        &mut h,
        me,
        0x8201_006A,
        &[text(&format!("{TEST_ITEM:08X}"))],
    );
    send(
        &mut h,
        me,
        0x8201_0130,
        &[Arg::LongInt(victim_id), text("Renamed")],
    );
    assert_eq!(
        h.server
            .store
            .character(victim_id)
            .expect("reads")
            .expect("kept")
            .name,
        "Renamed"
    );
    send(
        &mut h,
        me,
        0x8201_0058,
        &[text("Renamed"), text("10.0n 10.0e")],
    );
    assert!(h
        .server
        .players
        .values()
        .any(|p| p.character == victim_id && p.trip.is_some()));
    send(&mut h, me, 0x8201_013B, &[]);
    let creature_alive = |h: &Harness| {
        h.server
            .things
            .placed()
            .any(|t| t.kind == yalain_world::things::Kind::Creature && t.health > 0)
    };
    assert!(!creature_alive(&h), "exterminated");
    send(
        &mut h,
        me,
        0x8201_0016,
        &[
            Arg::Int(0),
            text("zero"),
            Arg::Int(1),
            Arg::Int(0),
            Arg::Int(0),
        ],
    );
    let kept = h
        .server
        .store
        .account_by_name("zero")
        .expect("reads")
        .expect("kept");
    assert!(kept.ban.is_some(), "banned");
}

/// An admin character's rights end at once: when its account is no longer an admin's, and when it is
/// no longer an admin character; each call from then on is refused.
/// Behaviour: server.admin.rights-end-at-once
#[test]
fn an_admin_characters_rights_end_at_once() {
    let mut h = Harness::new();
    let (me, _) = admin_in_world(&mut h, "one", "Elara");
    const LIST: u32 = 0x8201_002F;
    let count = |h: &Harness| {
        h.server
            .admin_calls
            .refused
            .get(&LIST)
            .copied()
            .unwrap_or(0)
    };
    send(&mut h, me, LIST, &[]);
    assert_eq!(count(&h), 0);
    console(&mut h, "set-accountadmin one off");
    send(&mut h, me, LIST, &[]);
    assert_eq!(count(&h), 1, "the account's flag, at the time of the call");
    console(&mut h, "set-accountadmin one");
    send(&mut h, me, LIST, &[]);
    assert_eq!(count(&h), 1, "back");
    console(&mut h, "set-characteradmin Elara off");
    send(&mut h, me, LIST, &[]);
    assert_eq!(count(&h), 2, "the character");
}

/// The moderation calls from an admin character: who an account or a character is, the world's limit,
/// what is shown a player, and a call without a counterpart answered that this server has none.
/// Behaviour: server.admin.moderation-calls
#[test]
fn the_moderation_calls_answer_and_act() {
    let mut h = Harness::new();
    let (other_b, other_id) = h.player_in_world("zero", "Victim");
    let (me, _) = admin_in_world(&mut h, "one", "Elara");
    let from = h.bots[me].inbox.len();
    send(&mut h, me, 0x8201_001B, &[Arg::Int(0), text("Victim")]);
    send(
        &mut h,
        me,
        0x8201_0023,
        &[Arg::Int(0), Arg::LongInt(other_id)],
    );
    send(&mut h, me, 0x8201_0027, &[Arg::Int(0), text("zero")]);
    send(&mut h, me, 0x8201_008B, &[Arg::Int(0), Arg::Int(99)]);
    let said = answers(&h, me, from);
    assert!(said[0].contains("on account zero"), "{said:?}");
    assert!(said[1].contains("on account zero"), "{said:?}");
    assert!(said[2].contains("has 1 characters"), "{said:?}");
    assert!(said[3].contains("Victim"), "{said:?}");
    assert_eq!(h.server.cfg.max_sessions, 99);
    let shown = h.bots[other_b].inbox.len();
    send(&mut h, me, 0x8201_013F, &[info("Victim"), info("Hello.")]);
    assert!(h.bots[other_b].inbox[shown..]
        .iter()
        .any(|r| matches!(r.message(), Some(ServerMessage::Notice(n)) if n.text.literal.as_ref().is_some_and(|l| l.text().trim_end() == "Hello."))));
    let from = h.bots[me].inbox.len();
    send(&mut h, me, 0x8201_0123, &[]);
    assert_eq!(
        answers(&h, me, from),
        ["This server has no database locks."]
    );
}

/// A cloaked admin is seen by no player's client, and those that knew it see it leave; a cloak lifted
/// shows it again; an admin who sees what is hidden still sees it.
/// Behaviour: server.admin.cloak-hides-an-admin
#[test]
fn a_cloaked_admin_is_seen_by_no_players_client() {
    let mut h = Harness::new();
    h.player_in_world("zero", "Viewer");
    let (me, id) = admin_in_world(&mut h, "one", "Elara");
    let (second, _) = admin_in_world(&mut h, "two", "Cara");
    h.run(10);
    let knows = |h: &Harness, name: &str| {
        h.server
            .players
            .values()
            .find(|p| p.name == name)
            .is_some_and(|p| p.known.contains_key(&id))
    };
    assert!(knows(&h, "Viewer"));
    send(&mut h, second, 0x8201_0088, &[Arg::Int(1)]);
    send(&mut h, me, 0x8201_0017, &[]);
    assert!(!knows(&h, "Viewer"), "a player's client no longer");
    assert!(knows(&h, "Cara"), "one who sees what is hidden does");
    send(&mut h, me, 0x8201_0061, &[]);
    assert!(knows(&h, "Viewer"), "shown again");
}

/// Never say die: an admin's health stops at one under a killing blow, and the setting off lets it
/// fall as usual.
/// Behaviour: server.admin.never-say-die-keeps-an-admin-alive
#[test]
fn never_say_die_holds_an_admins_health_at_one() {
    let mut h = Harness::new();
    let (me, id) = admin_in_world(&mut h, "one", "Elara");
    send(&mut h, me, 0x8201_0057, &[Arg::Int(1)]);
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == id)
        .map(|(w, _)| w)
        .expect("in");
    let hit = |h: &mut Harness| {
        yalain_world::combat::lose_health_for_test(&mut h.server, &mut h.net, world, 1_000_000);
    };
    hit(&mut h);
    assert_eq!(health(&h, id), 1, "stops at one");
    send(&mut h, me, 0x8201_0057, &[Arg::Int(0)]);
    hit(&mut h);
    assert_eq!(health(&h, id), 0, "falls");
}

/// The attackable setting: a creature does not notice a stealthy admin near it, nor a super one, and
/// notices one that is normal.
/// Behaviour: server.admin.attackable-hides-from-creatures
#[test]
fn a_stealthy_admin_is_not_noticed_by_creatures() {
    let mut h = Harness::new();
    h.add_creatures(1);
    let (me, id) = admin_in_world(&mut h, "one", "Elara");
    let stealth = 0x4000_0002;
    send(&mut h, me, other::AI_ATTACKABLE, &[Arg::Int(stealth)]);
    h.bots[me].send(
        true,
        queue::WEENIE,
        &ClientMessage::SceneRenderingComplete(Empty),
    );
    h.run(10);
    let at = h
        .server
        .players
        .values()
        .find(|p| p.character == id)
        .map(|p| p.position)
        .expect("in");
    assert_eq!(yalain_world::ai::notices(&h.server, &at, 1_000.0), None);
    send(&mut h, me, other::AI_ATTACKABLE, &[Arg::Int(0x4000_0001)]);
    assert_eq!(yalain_world::ai::notices(&h.server, &at, 1_000.0), Some(id));
}

/// Ethereality: an override changes what the admin passes through, every client that knows it told
/// with the movement ethereality quality; moving between areas does not change it, and normal puts
/// its own back.
/// Behaviour: server.admin.ethereality-override
#[test]
fn an_ethereality_override_is_told_and_lifted() {
    let mut h = Harness::new();
    let (viewer, _) = h.player_in_world("zero", "Viewer");
    let (me, id) = admin_in_world(&mut h, "one", "Elara");
    h.run(10);
    let told = |h: &Harness, b: usize, from: usize| {
        h.world_messages(b, from)
            .iter()
            .rev()
            .find_map(|m| match m {
                ServerMessage::UpdateIntVisual(q) if q.object.id == id && q.quality == 7 => {
                    Some(q.value)
                }
                _ => None,
            })
    };
    let kept = |h: &Harness| {
        let p = h
            .server
            .players
            .values()
            .find(|p| p.character == id)
            .expect("in");
        yalain_world::kingdoms::kept_ethereality(&p.qualities)
    };
    let own = kept(&h);
    let from = h.bots[viewer].inbox.len();
    send(
        &mut h,
        me,
        other::ETHEREALITY,
        &[Arg::Int(ethereal_value::SUPER)],
    );
    assert_eq!(told(&h, viewer, from), Some(-1), "another client is told");
    assert_eq!(kept(&h), -1);
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == id)
        .map(|(w, _)| w)
        .expect("in");
    yalain_world::kingdoms::settle_ethereality(&mut h.server, &mut h.net, world);
    assert_eq!(kept(&h), -1, "stands until lifted");
    send(
        &mut h,
        me,
        other::ETHEREALITY,
        &[Arg::Int(ethereal_value::NORMAL)],
    );
    assert_eq!(kept(&h), own, "its own again");
}

/// An admin's own state commands: experience, vitals, teleport to its saved place and back, and an
/// object made and deleted.
/// Behaviour: server.admin.character-and-object-calls
#[test]
fn the_state_and_object_calls_act() {
    let mut h = Harness::new();
    h.add_creatures(1);
    let (me, id) = admin_in_world(&mut h, "one", "Elara");
    h.run(10);
    send(&mut h, me, 0x8201_008E, &[Arg::Float(5.0_f32.to_bits())]);
    assert_eq!(health(&h, id), 5, "health set");
    send(&mut h, me, 0x8201_0051, &[Arg::LongInt(id)]);
    assert!(health(&h, id) > 5, "and made full");

    send(&mut h, me, 0x8201_0133, &[Arg::LongInt(id), Arg::Int(3)]);
    let p = h
        .server
        .players
        .values()
        .find(|p| p.character == id)
        .expect("in");
    assert!(
        p.places.iter().any(|(k, _)| *k == 602),
        "slot 3 is the third"
    );

    let from = h.bots[me].inbox.len();
    send(
        &mut h,
        me,
        0x8201_006F,
        &[text(&format!("{TEST_ITEM:08X}"))],
    );
    let said = answers(&h, me, from);
    assert!(said[0].starts_with("Made 0x"), "{said:?}");
    let made =
        u64::from_str_radix(said[0][7..].split(' ').next().expect("an id"), 16).expect("hex");
    assert!(h.server.things.get(made).is_some_and(|t| t.at.is_some()));
    send(&mut h, me, 0x8201_002E, &[Arg::LongInt(made)]);
    assert!(h.server.things.get(made).is_none(), "deleted");

    let from = h.bots[me].inbox.len();
    send(&mut h, me, 0x8201_006F, &[text("wizard")]);
    assert!(answers(&h, me, from)[0].contains("in hex"));
    let from = h.bots[me].inbox.len();
    send(&mut h, me, 0x8201_006F, &[text("DEADBEEF")]);
    assert!(answers(&h, me, from)[0].contains("no template"));
}

/// `/generate` makes the number asked for, one if the number is not a count, and at most a hundred
/// however many are asked for.
/// Behaviour: server.admin.generate-makes-at-most-a-hundred
#[test]
fn generate_makes_what_is_asked_up_to_a_hundred() {
    let mut h = Harness::new();
    h.add_creatures(0);
    let (me, _) = admin_in_world(&mut h, "one", "Elara");
    h.run(10);
    for (asked, makes) in [(3, 3), (0, 1), (-5, 1), (101, 100), (i32::MAX, 100)] {
        let from = h.bots[me].inbox.len();
        send(
            &mut h,
            me,
            0x8201_0037,
            &[text("test"), Arg::Int(asked.cast_unsigned())],
        );
        assert_eq!(
            answers(&h, me, from),
            [format!("Camp test made {makes}.")],
            "{asked} asked for"
        );
    }
}

/// Every call of the family has an answer from an admin character, whatever comes with it: none
/// stops the server, and none needs its values to be there.
/// Behaviour: server.admin.every-call-answers-an-admin
#[test]
fn every_admin_call_answers_an_admin_character_even_with_no_values() {
    let mut h = Harness::new();
    let (me, _) = admin_in_world(&mut h, "one", "Elara");
    let mut all = admin_calls::admin_family();
    all.extend(other::ALL);
    for f in all {
        // These two end the admin's own session; the others leave it in the world.
        if f == other::SMITE {
            continue;
        }
        let from = h.bots[me].inbox.len();
        h.bots[me].send(true, queue::WEENIE, &server_call(f, &[]));
        h.run(6);
        assert!(!answers(&h, me, from).is_empty(), "{f:#x} answers");
    }
}

/// A command the operator keeps to named groups is refused to an admin in none of them, in game and
/// as a call, logged and told so; an admin in one may give it, and a command kept to nothing is open
/// to every admin. The console is never refused; a change of an account's groups holds at once.
/// Behaviour: server.admin.named-groups-gate-the-account-commands
#[test]
fn the_account_level_commands_are_kept_to_their_named_groups() {
    const USER_LIMIT: u32 = 0x8201_008B;
    const FINGER: u32 = 0x8201_0027;
    let mut h = Harness::new();
    h.server.cfg.admin_commands = vec![("userlimit".to_owned(), vec!["leads".to_owned()])];
    let (me, _) = admin_in_world(&mut h, "one", "Elara");
    let limit = h.server.cfg.max_sessions;
    let from = h.bots[me].inbox.len();
    send(&mut h, me, USER_LIMIT, &[Arg::Int(0), Arg::Int(7)]);
    assert_eq!(h.server.cfg.max_sessions, limit, "refused");
    assert_eq!(answers(&h, me, from), ["userlimit needs the group leads."]);
    assert_eq!(h.server.admin_calls.refused.get(&USER_LIMIT), Some(&1));
    let from = h.bots[me].inbox.len();
    send(&mut h, me, FINGER, &[Arg::Int(0), text("one")]);
    assert!(
        answers(&h, me, from)[0].contains("has 1 characters"),
        "not kept: open"
    );

    console(&mut h, "set-accountgroups one leads,support");
    send(&mut h, me, USER_LIMIT, &[Arg::Int(0), Arg::Int(7)]);
    assert_eq!(h.server.cfg.max_sessions, 7, "in the group");
    console(&mut h, "set-accountgroups one");
    send(&mut h, me, USER_LIMIT, &[Arg::Int(0), Arg::Int(9)]);
    assert_eq!(h.server.cfg.max_sessions, 7, "out of it again, at once");

    h.server.cfg.admin_commands = vec![("ban".to_owned(), vec!["leads".to_owned()])];
    let clock = h.clock();
    let c = admin::parse("ban one 1 0 0")
        .expect("a command")
        .expect("reads");
    let by_account = Issuer::Account {
        name: "two".into(),
        admin: true,
        groups: vec![],
        character: "Two".into(),
    };
    assert_eq!(
        admin::run(&mut h.server, &mut h.net, clock, &by_account, &c),
        ["ban needs the group leads."]
    );
    assert!(
        console(&mut h, "ban one 1 0 0")
            .join(" ")
            .contains("Banned"),
        "the console"
    );
    let table = yalain_world::admin_groups::table(&h.server);
    assert!(table.contains(&"ban: leads".to_owned()));
    assert!(table.contains(&"boot: every admin".to_owned()));
    let kept = h
        .server
        .store
        .account_by_name("one")
        .expect("reads")
        .expect("kept");
    assert!(kept.groups.is_empty());
}

/// A library record's encrypted sends made plain, aligned pairs only, the rest of its bytes as they
/// were; too few and it is not the library this was made for.
/// Behaviour: server.admin.library-sends-admin-calls-in-the-clear
#[test]
fn only_aligned_encrypted_sends_are_made_plain() {
    use yalain_world::admin_library::{plain, AdminLibrary};
    let word = |w: u32| w.to_le_bytes();
    let mut b = Vec::new();
    for w in [7, 0x0C, SEND_ENCRYPTED, 0x0C, SEND_PLAIN, SEND_ENCRYPTED] {
        b.extend(word(w));
    }
    b.extend([0xCC, 0x0C, 0, 0, 0, 0x37, 0, 0x7F, 0]);
    let (out, n) = plain(&b);
    assert_eq!(n, 1);
    assert_eq!(out[8..12], word(SEND_PLAIN));
    assert_eq!(out[20..], b[20..]);
    assert!(AdminLibrary::of(&b).is_err());
}

/// The library goes to an admin's account alone, before its characters: the client told of it, sent
/// the record, and its list held back until it has stored it. A player's account is sent none; nor is
/// an admin's that lists the library's iteration.
/// Behaviour: server.admin.library-served-only-to-admin-accounts
#[test]
fn only_an_admins_login_is_served_the_library() {
    use auberean_protocol::login::{DataFileIterations, DataHeld};
    use auberean_protocol::types::{IterationRun, IterationSet};
    use yalain_world::admin_library::{plain, AdminLibrary, ITERATION};
    use yalain_world::download::PORTAL_FILE;
    let mut record = Vec::new();
    for _ in 0..200 {
        record.extend(0x0C_u32.to_le_bytes());
        record.extend(SEND_ENCRYPTED.to_le_bytes());
    }
    assert_eq!(plain(&record).1, 200);
    let mut h = Harness::new();
    h.server.admin_library = Some(AdminLibrary::of(&record).expect("made"));
    h.server
        .store
        .create_account(
            "one",
            Some(yalain_world::login::secret_for(b"ticket", 4).expect("made")),
            1,
        )
        .expect("made");
    console(&mut h, "set-accountadmin one");
    let listed = |h: &Harness, b: usize| {
        h.bots[b]
            .messages(false)
            .iter()
            .any(|m| matches!(m, ServerMessage::CharacterList(_)))
    };
    let served = |h: &Harness, b: usize| {
        h.bots[b]
            .messages(false)
            .iter()
            .any(|m| matches!(m, ServerMessage::DataRecord(_)))
    };
    // The harness's login sends what the client holds, and waits for its list.
    let player = h.at_character_list("two");
    assert!(!served(&h, player), "a player's is not");

    let b = h.logging_in("one");
    let held = ClientMessage::DataHeld(DataHeld {
        language: 0x8000_0001,
        files: Vec::new(),
    });
    h.bots[b].send(false, queue::DATABASE, &held);
    h.run(20);
    assert!(served(&h, b), "the admin's is");
    assert!(!listed(&h, b), "its list held back");
    h.bots[b].send(false, queue::DATABASE, &ClientMessage::DownloadEnds(Empty));
    h.run(20);
    assert!(listed(&h, b), "and sent once it has stored it");

    h.server.login_guard = yalain_world::login::LoginGuard::default();
    let c = h.logging_in("one");
    let held = ClientMessage::DataHeld(DataHeld {
        language: 0x8000_0001,
        files: vec![DataFileIterations {
            dat_file: PORTAL_FILE,
            iterations: IterationSet {
                total: 1,
                runs: vec![IterationRun::Run {
                    count: 1,
                    first: ITERATION,
                }],
            },
        }],
    });
    h.bots[c].send(false, queue::DATABASE, &held);
    h.run(20);
    assert!(!served(&h, c), "not when it holds the library");
    assert!(listed(&h, c));
}

/// With the library goes the record of the `/ethereality` words, which no install holds: the client's
/// own lookup reads each word as the value this server takes for it, a retail client is told to expect
/// the record, and the record is sent with it.
/// Behaviour: server.admin.library-carries-the-ethereality-words
#[test]
fn the_library_carries_the_words_of_the_ethereality_command() {
    use auberean_assets::strings::EnumMap;
    use auberean_dat::Reader;
    use yalain_world::admin_calls::Ethereality;
    use yalain_world::admin_library::{
        ethereality_record, AdminLibrary, ENUM_KIND, ETHEREALITY_RECORD,
    };
    let bytes = ethereality_record();
    let map = EnumMap::read(&mut Reader::new(&bytes)).expect("reads");
    assert_eq!(map.id, ETHEREALITY_RECORD);
    assert_eq!(map.base, 0);
    for (word, value, over) in [
        ("super", ethereal_value::SUPER, Ethereality::Super),
        ("ADMIN", ethereal_value::ADMIN, Ethereality::Admin),
        ("normal", ethereal_value::NORMAL, Ethereality::Normal),
    ] {
        assert_eq!(map.enum_of(word), Some(value), "{word}");
        let mut h = Harness::new();
        let (me, id) = admin_in_world(&mut h, "one", "Elara");
        send(&mut h, me, other::ETHEREALITY, &[Arg::Int(value)]);
        let p = h
            .server
            .players
            .values()
            .find(|p| p.character == id)
            .expect("in the world");
        assert_eq!(p.admin.ethereality, over, "{word} does what it says");
    }

    let mut record = Vec::new();
    for _ in 0..200 {
        record.extend(0x0C_u32.to_le_bytes());
        record.extend(SEND_ENCRYPTED.to_le_bytes());
    }
    let lib = AdminLibrary::of(&record).expect("made");
    let ServerMessage::DownloadBegins(begins) = &lib.begins else {
        panic!("it begins the download");
    };
    assert!(begins.files[0].download.contains(&ETHEREALITY_RECORD));
    let sent: Vec<_> = lib
        .records
        .iter()
        .filter_map(|m| match m {
            ServerMessage::DataRecord(r) => Some(r),
            _ => None,
        })
        .collect();
    let words = sent
        .iter()
        .find(|r| r.id == ETHEREALITY_RECORD)
        .expect("sent");
    assert_eq!(words.kind, ENUM_KIND);
    assert_eq!(
        sent.iter().map(|r| r.id).collect::<Vec<_>>(),
        begins.files[0].download,
        "every record it lists, and no other"
    );
    assert_eq!(
        begins.bytes as usize,
        sent.iter().map(|r| r.bytes.len()).sum::<usize>(),
        "the bytes it says"
    );
}

/// With the library goes the record of the log type's names, which no install holds: the client reads
/// the type every answer goes out as under a name, so it shows no empty `[]`. A client holding the
/// iteration before is sent that record alone, never the library again; one holding any other, or
/// none, the whole; one holding this one, nothing.
/// Behaviour: server.admin.library-names-the-log-type
#[test]
fn the_library_names_the_log_type_and_a_client_holding_the_last_gets_only_that() {
    use auberean_assets::strings::EnumMap;
    use auberean_dat::Reader;
    use auberean_protocol::login::{DataFileIterations, DataRecord};
    use auberean_protocol::types::{IterationRun, IterationSet};
    use yalain_world::admin_calls::LOG_TYPE;
    use yalain_world::admin_library::{
        log_type_record, AdminLibrary, ENUM_KIND, ETHEREALITY_RECORD, ITERATION, LOG_TYPE_RECORD,
        PREVIOUS_ITERATION, RECORD,
    };
    use yalain_world::download::PORTAL_FILE;
    let bytes = log_type_record();
    let map = EnumMap::read(&mut Reader::new(&bytes)).expect("reads");
    assert_eq!(map.id, LOG_TYPE_RECORD);
    assert_eq!(map.base, 0);
    assert_eq!(map.names, [(LOG_TYPE, "Admin".to_owned())]);

    let mut record = Vec::new();
    for _ in 0..200 {
        record.extend(0x0C_u32.to_le_bytes());
        record.extend(SEND_ENCRYPTED.to_le_bytes());
    }
    let lib = AdminLibrary::of(&record).expect("made");
    let held = |file, runs| {
        vec![DataFileIterations {
            dat_file: file,
            iterations: IterationSet { total: 1, runs },
        }]
    };
    let one = |v: u32| IterationRun::One(v.cast_signed());
    let sent = |files: &[DataFileIterations]| -> Option<(Vec<u32>, usize, u32, Vec<u32>)> {
        let (begins, records) = lib.to_send(files)?;
        let ServerMessage::DownloadBegins(b) = begins else {
            panic!("it begins the download");
        };
        let records: Vec<&DataRecord> = records
            .iter()
            .filter_map(|m| match m {
                ServerMessage::DataRecord(r) => Some(r),
                _ => None,
            })
            .collect();
        assert_eq!(
            b.bytes as usize,
            records.iter().map(|r| r.bytes.len()).sum::<usize>(),
            "the bytes it says"
        );
        assert!(records
            .iter()
            .filter(|r| r.id == LOG_TYPE_RECORD)
            .all(|r| r.kind == ENUM_KIND));
        Some((
            b.files[0].download.clone(),
            b.files.len(),
            b.files[0].iteration,
            records.iter().map(|r| r.id).collect(),
        ))
    };
    let whole = vec![RECORD, ETHEREALITY_RECORD, LOG_TYPE_RECORD];
    let newer = vec![LOG_TYPE_RECORD];
    for (files, expected) in [
        (Vec::new(), Some(&whole)),
        (held(PORTAL_FILE, vec![one(9_000)]), Some(&whole)),
        (
            held(PORTAL_FILE + 1, vec![one(PREVIOUS_ITERATION)]),
            Some(&whole),
        ),
        (
            held(PORTAL_FILE, vec![one(PREVIOUS_ITERATION)]),
            Some(&newer),
        ),
        (
            held(
                PORTAL_FILE,
                vec![IterationRun::Run {
                    count: 2,
                    first: PREVIOUS_ITERATION - 1,
                }],
            ),
            Some(&newer),
        ),
        (held(PORTAL_FILE, vec![one(ITERATION)]), None),
        (
            held(
                PORTAL_FILE,
                vec![IterationRun::Run {
                    count: 2,
                    first: PREVIOUS_ITERATION,
                }],
            ),
            None,
        ),
    ] {
        let got = sent(&files);
        match expected {
            Some(ids) => assert_eq!(
                got,
                Some((ids.clone(), 1, ITERATION, ids.clone())),
                "{files:?}"
            ),
            None => assert_eq!(got, None, "{files:?}"),
        }
    }
}

/// What makes an admin is the operator's alone: said in game by an admin, raising an account, making
/// an admin character, changing groups or a new admin's account are refused and change nothing, as is
/// another admin's password; a player's password may be set, as before.
/// Behaviour: server.admin.what-makes-an-admin-is-the-consoles
#[test]
fn an_admin_in_game_cannot_make_admins_or_join_groups() {
    let mut h = Harness::new();
    h.player_in_world("two", "Babyfare");
    let (me, _) = admin_in_world(&mut h, "one", "Elara");
    let (_, _) = admin_in_world(&mut h, "three", "Cara");
    let said = |h: &mut Harness, line: &str| {
        let from = h.bots[me].inbox.len();
        let text = line.to_owned();
        let call = server_call(
            yalain_world::requests::function::SAY,
            &[Arg::Int(0), info(&format!("@{text}"))],
        );
        h.bots[me].send(true, queue::WEENIE, &call);
        h.run(20);
        h.bots[me].inbox[from..]
            .iter()
            .filter_map(|r| match r.message() {
                Some(ServerMessage::Notice(n)) => {
                    n.text.literal.map(|l| l.text().trim_end().to_owned())
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let no = [admin::CONSOLE_ONLY];
    assert_eq!(said(&mut h, "set-accountadmin two"), no);
    assert_eq!(said(&mut h, "set-characteradmin Babyfare"), no);
    assert_eq!(said(&mut h, "set-accountgroups one leads"), no);
    assert_eq!(said(&mut h, "accountcreate mole pw admin"), no);
    assert_eq!(said(&mut h, "set-accountpassword three stolen"), no);
    let kept = |h: &mut Harness, n: &str| h.server.store.account_by_name(n).expect("reads");
    assert!(!kept(&mut h, "two").expect("kept").admin);
    assert!(kept(&mut h, "one").expect("kept").groups.is_empty());
    assert!(kept(&mut h, "mole").is_none());
    assert_eq!(
        said(&mut h, "set-accountpassword two fine"),
        ["Account password for two successfully changed."]
    );
    assert!(!console(&mut h, "set-accountadmin two")[0].contains("operator"));
}

/// A client asking again for a cloaked admin's object by its id is told nothing.
/// Behaviour: server.admin.cloak-hides-an-admin
#[test]
fn a_cloaked_admin_cannot_be_asked_for_by_id() {
    use auberean_protocol::physics::Target;
    let mut h = Harness::new();
    let (viewer, _) = h.player_in_world("zero", "Viewer");
    let (me, id) = admin_in_world(&mut h, "one", "Elara");
    send(&mut h, me, 0x8201_0017, &[]);
    let from = h.bots[viewer].inbox.len();
    h.bots[viewer].send(true, queue::WEENIE, &ClientMessage::ForceCreate(Target(id)));
    h.run(10);
    assert_eq!(told(&h, viewer, from, id), None, "not told");
}

fn told(h: &Harness, b: usize, from: usize, id: u64) -> Option<u64> {
    h.world_messages(b, from).iter().find_map(|m| match m {
        ServerMessage::CreateObject(o) if o.id == id => Some(o.id),
        _ => None,
    })
}

/// Set everything an admin's commands set on itself, with a viewer that may not see it and another
/// admin hidden from all but it.
fn set_all(h: &mut Harness, me: usize) {
    send(h, me, 0x8201_0017, &[]);
    send(h, me, 0x8201_0088, &[Arg::Int(1)]);
    send(h, me, 0x8201_0057, &[Arg::Int(1)]);
    send(h, me, other::AI_ATTACKABLE, &[Arg::Int(0x4000_0003)]);
    send(
        h,
        me,
        other::ETHEREALITY,
        &[Arg::Int(ethereal_value::SUPER)],
    );
    send(h, me, 0x8201_0044, &[Arg::Float(5.0_f32.to_bits())]);
}

/// All of it is gone at once: the viewer sees it again, it has its own ethereality, and it holds no
/// admin hidden from players; a killing blow kills it.
fn assert_all_gone(h: &mut Harness, id: u64, hidden: u64, own: i32) {
    let p = h
        .server
        .players
        .values()
        .find(|p| p.character == id)
        .expect("in the world");
    assert_eq!(p.admin, yalain_world::admin_calls::AdminState::default());
    assert_eq!(
        yalain_world::kingdoms::kept_ethereality(&p.qualities),
        own,
        "its own ethereality"
    );
    assert!(
        !p.known.contains_key(&hidden),
        "no longer sees what is hidden"
    );
    let viewer = h
        .server
        .players
        .values()
        .find(|p| p.name == "Viewer")
        .expect("in the world");
    assert!(viewer.known.contains_key(&id), "seen again");
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == id)
        .map(|(w, _)| w)
        .expect("in");
    yalain_world::combat::lose_health_for_test(&mut h.server, &mut h.net, world, 1_000_000);
    assert_eq!(health(h, id), 0, "it can die");
}

/// A character made a player again by the console is at once: no longer hidden, killable, noticed,
/// passing through nothing it should not, at its own speed and blind to what is hidden, whether its
/// account lost the flag or the character its template.
/// Behaviour: server.admin.taking-admin-away-ends-what-it-set
#[test]
fn taking_admin_away_ends_what_the_admins_commands_set() {
    for line in ["set-accountadmin one off", "set-characteradmin Elara off"] {
        let mut h = Harness::new();
        h.player_in_world("zero", "Viewer");
        let (me, id) = admin_in_world(&mut h, "one", "Elara");
        let (second, hidden) = admin_in_world(&mut h, "two", "Cara");
        h.run(10);
        let own = {
            let p = h
                .server
                .players
                .values()
                .find(|p| p.character == id)
                .expect("in");
            yalain_world::kingdoms::kept_ethereality(&p.qualities)
        };
        set_all(&mut h, me);
        send(&mut h, second, 0x8201_0017, &[]);
        let p = h
            .server
            .players
            .values()
            .find(|p| p.character == id)
            .expect("in");
        assert!(p.known.contains_key(&hidden), "it saw the hidden admin");
        assert_eq!(yalain_world::kingdoms::kept_ethereality(&p.qualities), -1);
        console(&mut h, line);
        h.run(5);
        assert_all_gone(&mut h, id, hidden, own);
    }
}

/// What does a kept command's work is kept to its groups: an admin kept from `ban` cannot give a
/// player a new secret, lift a ban or list them, and one kept from `fingeraccount` cannot read accounts.
/// Out of those groups an admin keeps every other command; the console all of them.
/// Behaviour: server.admin.named-groups-gate-the-account-commands
#[test]
fn what_does_a_kept_commands_work_follows_its_groups() {
    let mut h = Harness::new();
    h.player_in_world("two", "Babyfare");
    h.server.cfg.admin_commands = vec![
        ("ban".to_owned(), vec!["leads".to_owned()]),
        ("fingeraccount".to_owned(), vec!["leads".to_owned()]),
    ];
    let by = |groups: &[&str]| Issuer::Account {
        name: "one".into(),
        admin: true,
        groups: groups.iter().map(|g| (*g).to_owned()).collect(),
        character: "One".into(),
    };
    for line in [
        "set-accountpassword two stolen",
        "unban two",
        "banlist",
        "accountget two",
        "accountlist",
    ] {
        let c = admin::parse(line).expect("a command").expect("reads");
        let clock = h.clock();
        let said = admin::run(&mut h.server, &mut h.net, clock, &by(&[]), &c);
        assert_eq!(said.len(), 1, "{line}");
        assert!(
            said[0].contains("needs the group leads"),
            "{line}: {said:?}"
        );
    }
    let c = admin::parse("accountlist")
        .expect("a command")
        .expect("reads");
    let clock = h.clock();
    let said = admin::run(&mut h.server, &mut h.net, clock, &by(&["leads"]), &c);
    assert!(said[0].ends_with("accounts:"), "in the group: {said:?}");
    let c = admin::parse("gag Babyfare")
        .expect("a command")
        .expect("reads");
    let clock = h.clock();
    let said = admin::run(&mut h.server, &mut h.net, clock, &by(&[]), &c);
    assert!(said[0].contains("gagged"), "another command: {said:?}");
    assert!(
        console(&mut h, "accountlist")[0].ends_with("accounts:"),
        "the console"
    );
}

/// `/dumpadmintable` answers with the commands and the groups each is kept to, as the operator set
/// them.
/// Behaviour: server.admin.dumpadmintable-shows-the-groups
#[test]
fn the_admin_table_shows_the_groups_as_set() {
    let mut h = Harness::new();
    h.server.cfg.admin_commands = vec![("ban".to_owned(), vec!["leads".to_owned()])];
    let (me, _) = admin_in_world(&mut h, "one", "Elara");
    console(&mut h, "set-accountgroups one leads");
    let from = h.bots[me].inbox.len();
    send(&mut h, me, 0x8201_0075, &[Arg::Int(0)]);
    let said = answers(&h, me, from);
    assert!(said.contains(&"ban: leads".to_owned()), "{said:?}");
    assert!(said.contains(&"boot: every admin".to_owned()), "{said:?}");
}

/// Who a lookup is asked about: an admin in the world, the same admin cloaked, or an id and a name
/// nobody has.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Asked {
    Shown,
    Cloaked,
    Nobody,
}

/// What a player's client and the admin's are sent for one call naming "Elara" and an id, with the
/// id written out of it, so that two answers compare whoever they were about. For nobody the admin
/// has another name and the id is not its own.
fn answer_to(asked: Asked, call: impl Fn(&mut Harness, usize, u64, &str)) -> String {
    let mut h = Harness::new();
    let (viewer, _) = h.player_in_world("zero", "Viewer");
    send(
        &mut h,
        viewer,
        fellowship::function::CREATE,
        &[text("Band"), Arg::Int(0), Arg::Int(1)],
    );
    let its_name = if asked == Asked::Nobody {
        "Cara"
    } else {
        "Elara"
    };
    let (me, id) = admin_in_world(&mut h, "one", its_name);
    h.run(10);
    if asked != Asked::Shown {
        send(&mut h, me, 0x8201_0017, &[]);
    }
    let id = if asked == Asked::Nobody {
        id + 1000
    } else {
        id
    };
    let (from, its_from) = (h.bots[viewer].inbox.len(), h.bots[me].inbox.len());
    call(&mut h, viewer, id, "Elara");
    format!(
        "{:?} {:?}",
        h.world_messages(viewer, from),
        h.world_messages(me, its_from)
    )
    .replace(&id.to_string(), "ID")
}

/// A use of an object as the client asks for it.
fn use_arg(item: u64, player: u64) -> Arg {
    let mut b = Builder::new();
    b.object(
        0x245,
        &[
            Slot::Ref(None),
            Slot::Ref(None),
            Slot::Long(item),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Long(player),
            Slot::Int((-1.0f32).to_bits()),
            Slot::Long(0),
            Slot::Int(0),
            Slot::Long(0),
            Slot::Int(0),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Int(316),
            Slot::Int(0),
            Slot::Int(0),
        ],
    );
    Arg::Object(b.graph())
}

/// An examination request about an object.
fn examining(object: u64) -> Arg {
    let mut bytes = Vec::new();
    for w in [0x4000_0002u32, 0] {
        bytes.extend(w.to_le_bytes());
    }
    bytes.extend(object.to_le_bytes());
    bytes.extend(0x203u32.to_le_bytes());
    let mut b = Builder::new();
    b.native(154, NativeObject::Packed { ty: 154, bytes });
    Arg::Object(b.graph())
}

/// A modified client can name anyone by an id or a name it guesses. Everything that answers one
/// answers about a cloaked admin exactly as about nobody at all, and differently about an admin
/// who is not cloaked (so each of these does look).
/// Behaviour: server.admin.cloak-hides-from-ids-and-names
#[test]
fn a_cloaked_admin_is_answered_for_as_if_nobody_had_its_id_or_name() {
    type Call = fn(&mut Harness, usize, u64, &str);
    let calls: [(&str, Call); 8] = [
        ("selecting", |h, b, id, _| {
            send(h, b, function::SET_SELECTION, &[Arg::LongInt(id)]);
        }),
        ("examining", |h, b, id, _| {
            send(h, b, function::EXAMINE, &[examining(id)]);
        }),
        ("attacking", |h, b, id, _| {
            send(h, b, combat::function::START_ATTACK, &[Arg::LongInt(id)]);
        }),
        ("recruiting", |h, b, id, _| {
            send(h, b, fellowship::function::RECRUIT, &[Arg::LongInt(id)]);
        }),
        ("swearing", |h, b, id, _| {
            send(h, b, allegiance::function::SWEAR, &[Arg::LongInt(id)]);
        }),
        ("trading", |h, b, id, _| {
            send(h, b, trade::function::OPEN, &[Arg::LongInt(id)]);
        }),
        ("using", |h, b, id, _| {
            let me = h
                .server
                .players
                .values()
                .find(|p| p.name == "Viewer")
                .expect("in the world")
                .character;
            send(h, b, function::USE, &[use_arg(id, me)]);
        }),
        ("telling", |h, b, _, name| {
            send(
                h,
                b,
                function::TELL,
                &[info(name), info("Psst."), Arg::Int(0)],
            );
        }),
    ];
    for (what, call) in calls {
        let cloaked = answer_to(Asked::Cloaked, call);
        assert_eq!(cloaked, answer_to(Asked::Nobody, call), "{what}: as nobody");
        assert_ne!(cloaked, answer_to(Asked::Shown, call), "{what}: it looks");
    }
}

/// A cloaked admin still knows its own player, and one who sees what is hidden still finds it.
/// Behaviour: server.admin.cloak-hides-from-ids-and-names
#[test]
fn a_cloaked_admin_is_found_only_by_itself_and_those_who_see_what_is_hidden() {
    let mut h = Harness::new();
    let (viewer, _) = h.player_in_world("zero", "Viewer");
    let (me, id) = admin_in_world(&mut h, "one", "Elara");
    let (second, _) = admin_in_world(&mut h, "two", "Cara");
    send(&mut h, second, 0x8201_0088, &[Arg::Int(1)]);
    send(&mut h, me, 0x8201_0017, &[]);
    let selected = |h: &mut Harness, b: usize| {
        let from = h.bots[b].inbox.len();
        send(h, b, function::SET_SELECTION, &[Arg::LongInt(id)]);
        !h.functions(b, from).is_empty()
    };
    assert!(!selected(&mut h, viewer), "a player does not");
    assert!(selected(&mut h, me), "itself");
    assert!(selected(&mut h, second), "one who sees what is hidden");
}

/// A cloaked admin's allegiance is not told that it is in the world, as it is once the cloak is
/// lifted.
/// Behaviour: server.admin.cloak-hides-from-ids-and-names
#[test]
fn a_cloaked_admin_shows_as_out_of_the_world_to_its_allegiance() {
    let mut h = Harness::new();
    let (me, id) = admin_in_world(&mut h, "one", "Elara");
    assert!(allegiance::facts(&mut h.server, id).online);
    send(&mut h, me, 0x8201_0017, &[]);
    assert!(!allegiance::facts(&mut h.server, id).online);
    send(&mut h, me, 0x8201_0061, &[]);
    assert!(allegiance::facts(&mut h.server, id).online);
}

/// A helpful skill aimed at a cloaked admin falls back on its user, as one aimed at nobody does.
/// Behaviour: server.admin.cloak-hides-from-ids-and-names
#[test]
fn a_helpful_skill_aimed_at_a_cloaked_admin_goes_to_its_user() {
    let mut h = Harness::new();
    h.player_in_world("zero", "Viewer");
    let (me, id) = admin_in_world(&mut h, "one", "Elara");
    let viewer = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.name == "Viewer")
        .expect("in the world")
        .0;
    let u = yalain_world::skills::Usage {
        ui_flags: skill_use::ui_flag::NON_DAMAGING,
        valid_targets: 3,
        ..Default::default()
    };
    let r = skill_use::Request {
        target: id,
        skill: 1,
        id: 1,
    };
    assert!(matches!(
        skill_use::aim_of(&h.server, viewer, r, &u),
        skill_use::Aim::Player(_, _)
    ));
    send(&mut h, me, 0x8201_0017, &[]);
    assert_eq!(
        skill_use::aim_of(&h.server, viewer, r, &u),
        skill_use::Aim::Me
    );
}

/// A player walking up to another for a trade when the other cloaks gives up the walk on its next
/// report, and nothing comes of it.
/// Behaviour: server.admin.cloak-hides-from-ids-and-names
#[test]
fn a_walk_to_someone_who_cloaks_on_the_way_comes_to_nothing() {
    let mut h = Harness::new();
    let (viewer, _) = h.player_in_world("zero", "Viewer");
    let (me, id) = admin_in_world(&mut h, "one", "Elara");
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.name == "Viewer")
        .expect("in the world")
        .0;
    let stand = |h: &mut Harness, off: f32| {
        let at = h.server.players[&world].position;
        let p = h.server.players.get_mut(&world).expect("in the world");
        p.position.origin[1] = at.origin[1] + off;
    };
    stand(&mut h, 4.0);
    send(&mut h, viewer, trade::function::OPEN, &[Arg::LongInt(id)]);
    assert!(h.server.players[&world].approaching.is_some(), "walking");
    send(&mut h, me, 0x8201_0017, &[]);
    stand(&mut h, -4.0);
    let from = h.bots[me].inbox.len();
    let clock = h.clock();
    yalain_world::social::moved(&mut h.server, &mut h.net, world, clock);
    assert!(h.server.players[&world].approaching.is_none(), "given up");
    h.run(10);
    assert!(h.functions(me, from).is_empty(), "the admin hears nothing");
}

/// A client sending refused admin calls as fast as it can fills no log: the first from a character
/// in each stay is logged, every one is counted.
/// Behaviour: server.admin.refused-calls-log-one-line
#[test]
fn refused_admin_calls_log_one_line_a_stay_however_many_come() {
    let mut h = Harness::new();
    let (player, _) = h.player_in_world("one", "Elara");
    let (other, _) = h.player_in_world("two", "Babyfare");
    for f in [
        0x8201_0017u32,
        0x8201_0061,
        0x8201_0017,
        0x8201_0088,
        0x8201_0017,
    ] {
        h.bots[player].send(true, queue::WEENIE, &server_call(f, &[]));
    }
    h.run(10);
    assert_eq!(h.server.admin_calls.refused[&0x8201_0017], 3, "all counted");
    assert_eq!(h.server.admin_calls.logged, 1, "one line");
    h.bots[other].send(true, queue::WEENIE, &server_call(0x8201_0017, &[]));
    h.run(10);
    assert_eq!(h.server.admin_calls.logged, 2, "one for another character");
}

/// An answer of many lines is one message, its lines in their order, so that none can overtake
/// another on the wire.
/// Behaviour: server.admin.a-long-answer-is-one-message
#[test]
fn an_answer_of_many_lines_comes_as_one_message_in_its_order() {
    let mut h = Harness::new();
    let (me, _) = admin_in_world(&mut h, "one", "Elara");
    let from = h.bots[me].inbox.len();
    send(&mut h, me, 0x8201_0075, &[Arg::Int(0)]);
    let sent: Vec<_> = h
        .calls(me, from)
        .into_iter()
        .filter(|(f, _)| *f == LOG_EVENT)
        .collect();
    assert_eq!(sent.len(), 1, "{sent:?}");
    let said = answers(&h, me, from);
    assert!(said.len() > 3, "{said:?}");
}

/// The update kinds of a client's quest calls since `from`, each with the quest's id.
fn quest_updates(h: &Harness, b: usize, from: usize) -> Vec<(u32, u32)> {
    h.calls(b, from)
        .into_iter()
        .filter(|(f, _)| *f == yalain_world::quests::UPDATE_QUEST)
        .filter_map(|(_, v)| {
            let Some(Value::Int(kind)) = v.last() else {
                return None;
            };
            // A quest's record starts with its id.
            let Some(Node::Native { bytes, .. }) = v.get(1).and_then(Value::root) else {
                return None;
            };
            Some((*kind, u32::from_le_bytes(bytes[..4].try_into().ok()?)))
        })
        .collect()
}

/// `/qst remove` of a quest takes off the subquests of its phases, done or not, and theirs, without
/// a word; one another quest it holds still needs at that phase stays, as does any other quest.
/// Behaviour: server.admin.quest-removal-takes-its-subquests
#[test]
fn a_quest_an_admin_takes_off_a_player_takes_its_subquests_too() {
    use yalain_content::quests::{QuestDef, QuestTable, Step, Subquests, Trigger, Update};
    use yalain_world::quests::{phase_of, status, update};
    const USED: u32 = 0x4700_0001;
    const PARENT: u32 = 0x4000_0100;
    const SUB: u32 = 0x4000_0101;
    const SUB_OF_SUB: u32 = 0x4000_0102;
    const SHARED: u32 = 0x4000_0103;
    const OTHER_PARENT: u32 = 0x4000_0104;
    const DONE_SUB: u32 = 0x4000_0105;
    const UNRELATED: u32 = 0x4000_0106;
    let def = |id, subquests| QuestDef {
        id,
        name: (1, 0x2500_0001),
        description: (2, 0x2500_0001),
        icon: 0,
        play_fx: false,
        journals: Vec::new(),
        challenge: 0,
        duration: 0.0,
        timed_phase: 0,
        retry: 0.0,
        xp: 0,
        cancellable: false,
        placed: "authored for a test".into(),
        hidden: false,
        effects: Vec::new(),
        subquests,
        min_level: 0,
        fails_on_death: false,
        once: false,
        fellows: yalain_content::quests::Fellows::default(),
        last_waits: false,
        hero_only: false,
        required: None,
        pay: yalain_content::quests::CompletionPay::default(),
    };
    let at = |phase, quests: &[u32]| Subquests {
        phase,
        quests: quests.to_vec(),
        any: false,
    };
    let mut h = Harness::new();
    let (victim, victim_id) = h.player_in_world("one", "Elara");
    let (me, _) = admin_in_world(&mut h, "two", "Cara");
    h.server.quests = QuestTable {
        quests: vec![
            def(PARENT, vec![at(2, &[SUB, SHARED]), at(3, &[DONE_SUB])]),
            def(SUB, vec![at(1, &[SUB_OF_SUB])]),
            def(SUB_OF_SUB, Vec::new()),
            def(SHARED, Vec::new()),
            def(OTHER_PARENT, vec![at(1, &[SHARED])]),
            def(DONE_SUB, Vec::new()),
            def(UNRELATED, Vec::new()),
        ],
        steps: [(PARENT, 2u32), (PARENT, 3)]
            .map(|(quest, to)| Step {
                quest,
                needs: to - 1,
                trigger: Trigger::Use(USED),
                tell: None,
                update: Update::Advance(to),
                xp: 0,
                gives: Vec::new(),
                counts: Vec::new(),
                effects: Vec::new(),
                text: None,
                placed: "authored for a test".into(),
                requires: Vec::new(),
                takes: Vec::new(),
                races: Vec::new(),
                effects_last: false,
                wants: None,
                fellows: false,
                to: None,
            })
            .to_vec(),
        gives: Vec::new(),
    };
    let world = h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == victim_id)
        .map(|(w, _)| *w)
        .expect("in the world");
    let state = |quest, status, phase| yalain_world::state::QuestState {
        quest,
        status,
        phase,
        started: 0,
        updated: 0,
    };
    let held = |h: &Harness| -> Vec<u32> {
        let mut v: Vec<u32> = h.server.players[&world]
            .play
            .quests
            .iter()
            .map(|st| st.quest)
            .collect();
        v.sort_unstable();
        v
    };
    {
        let p = h.server.players.get_mut(&world).expect("in the world");
        p.play.quests = vec![
            state(PARENT, status::UNDERWAY, 1),
            state(SUB_OF_SUB, status::UNDERWAY, 1),
            state(OTHER_PARENT, status::UNDERWAY, 1),
            state(UNRELATED, status::UNDERWAY, 1),
        ];
    }
    for _ in 0..2 {
        let clock = h.clock();
        yalain_world::quests::triggered(
            &mut h.server,
            &mut h.net,
            world,
            Trigger::Use(USED),
            None,
            clock,
        );
    }
    {
        let p = h.server.players.get_mut(&world).expect("in the world");
        assert_eq!(phase_of(&p.play.quests, PARENT), 3, "moved on");
        for st in &mut p.play.quests {
            if st.quest == DONE_SUB {
                *st = state(DONE_SUB, status::COMPLETED, 0);
            }
        }
    }
    assert_eq!(
        held(&h),
        [
            PARENT,
            SUB,
            SUB_OF_SUB,
            SHARED,
            OTHER_PARENT,
            DONE_SUB,
            UNRELATED
        ],
        "the world gave them"
    );
    let remove = |h: &mut Harness, quest: u32| {
        send(
            h,
            me,
            0x8201_0019,
            &[Arg::LongInt(victim_id), Arg::Int(quest)],
        );
    };
    h.run(10);
    let from = h.bots[victim].inbox.len();
    remove(&mut h, PARENT);
    assert_eq!(
        quest_updates(&h, victim, from),
        [(update::REMOVE, PARENT)],
        "its client never knew the rest"
    );
    assert_eq!(held(&h), [SHARED, OTHER_PARENT, UNRELATED]);
    remove(&mut h, OTHER_PARENT);
    assert_eq!(held(&h), [UNRELATED], "the shared one goes with its last");
}

/// `/qst remove` and `/qst retry` tell the player's client at once, as the update its own handling
/// takes off the list; a quest its client never knew (a hidden one) goes without a word.
/// Behaviour: server.admin.quest-removal-told-at-once
#[test]
fn a_quest_an_admin_takes_off_a_player_leaves_its_clients_list_at_once() {
    use yalain_world::quests::update;
    use yalain_world::state::QuestState;
    const SHOWN: u32 = 0x8000_0101;
    const OTHER: u32 = 0x8000_0102;
    const HIDDEN: u32 = 0x8000_0103;
    for (call, shown) in [(0x8201_0019, SHOWN), (0x8201_0116, OTHER)] {
        let mut h = Harness::new();
        let (victim, victim_id) = h.player_in_world("one", "Elara");
        let (me, _) = admin_in_world(&mut h, "two", "Cara");
        let def = |id, hidden| yalain_content::quests::QuestDef {
            id,
            name: (1, 0x2500_0001),
            description: (2, 0x2500_0001),
            icon: 0,
            play_fx: false,
            journals: Vec::new(),
            challenge: 0,
            duration: 0.0,
            timed_phase: 0,
            retry: 0.0,
            xp: 0,
            cancellable: false,
            placed: "authored for a test".into(),
            hidden,
            effects: Vec::new(),
            subquests: Vec::new(),
            min_level: 0,
            fails_on_death: false,
            once: false,
            fellows: yalain_content::quests::Fellows::default(),
            last_waits: false,
            hero_only: false,
            required: None,
            pay: yalain_content::quests::CompletionPay::default(),
        };
        h.server.quests = yalain_content::QuestTable {
            quests: vec![def(SHOWN, false), def(OTHER, false), def(HIDDEN, true)],
            steps: Vec::new(),
            gives: Vec::new(),
        };
        for p in h.server.players.values_mut() {
            for quest in [SHOWN, OTHER, HIDDEN] {
                p.play.quests.push(QuestState {
                    quest,
                    status: yalain_world::quests::status::UNDERWAY,
                    phase: 1,
                    started: 0,
                    updated: 0,
                });
            }
        }
        let holds = |h: &Harness, q: u32| {
            h.server
                .players
                .values()
                .find(|p| p.character == victim_id)
                .expect("in the world")
                .play
                .quests
                .iter()
                .any(|st| st.quest == q)
        };
        let from = h.bots[victim].inbox.len();
        send(
            &mut h,
            me,
            call,
            &[Arg::LongInt(victim_id), Arg::Int(shown)],
        );
        assert_eq!(quest_updates(&h, victim, from), [(update::REMOVE, shown)]);
        assert!(!holds(&h, shown), "gone");
        assert!(holds(&h, HIDDEN), "the other stays");

        let from = h.bots[victim].inbox.len();
        let said = h.bots[me].inbox.len();
        send(
            &mut h,
            me,
            0x8201_0019,
            &[Arg::LongInt(victim_id), Arg::Int(HIDDEN)],
        );
        assert_eq!(quest_updates(&h, victim, from), [], "never told of it");
        assert!(!holds(&h, HIDDEN), "gone");
        assert!(answers(&h, me, said)[0].contains("no longer holds"));

        let from = h.bots[victim].inbox.len();
        let said = h.bots[me].inbox.len();
        send(
            &mut h,
            me,
            0x8201_0019,
            &[Arg::LongInt(victim_id), Arg::Int(HIDDEN)],
        );
        assert_eq!(quest_updates(&h, victim, from), [], "held no longer");
        assert!(answers(&h, me, said)[0].contains("does not hold"));
    }
}

/// An admin teaching a player a recipe or taking one away tells its client at once, as the world's
/// own learning and forgetting do; a change that changes nothing tells it nothing.
/// Behaviour: server.admin.recipes-told-at-once
#[test]
fn a_recipe_an_admin_adds_or_removes_reaches_the_players_client_at_once() {
    use yalain_world::craft::function::{ADD_RECIPE, REMOVE_RECIPE};
    const RECIPE: u32 = 0x4700_0123;
    let mut h = Harness::new();
    let (victim, victim_id) = h.player_in_world("one", "Elara");
    let (me, _) = admin_in_world(&mut h, "two", "Cara");
    let told = |h: &Harness, from: usize, f: u32| {
        h.calls(victim, from)
            .into_iter()
            .filter(|(c, v)| *c == f && matches!(v.first(), Some(Value::Int(RECIPE))))
            .count()
    };
    for (call, f, known) in [
        (0x8201_004D, ADD_RECIPE, true),
        (0x8201_0078, REMOVE_RECIPE, false),
    ] {
        let args = [Arg::LongInt(victim_id), Arg::Int(RECIPE)];
        let from = h.bots[victim].inbox.len();
        send(&mut h, me, call, &args);
        assert_eq!(told(&h, from, f), 1, "{call:#x}");
        let p = h
            .server
            .players
            .values()
            .find(|p| p.character == victim_id)
            .expect("in the world");
        assert_eq!(p.play.recipes.iter().any(|r| r.recipe == RECIPE), known);
        let from = h.bots[victim].inbox.len();
        send(&mut h, me, call, &args);
        assert_eq!(told(&h, from, f), 0, "{call:#x} again changes nothing");
    }
}

/// `/rename` reaches the renamed client and every client that knows it at once, as a change of its
/// name's quality; its own description holds the new name when it next enters, as another client's
/// new sight of it does.
/// Behaviour: server.admin.rename-told-at-once
#[test]
fn a_rename_reaches_every_client_at_once_and_lasts() {
    use auberean_protocol::text::StringInfo;
    let mut h = Harness::new();
    let (victim, victim_id) = h.player_in_world("one", "Elara");
    let (viewer, _) = h.player_in_world("three", "Babyfare");
    let (me, _) = admin_in_world(&mut h, "two", "Cara");
    h.run(20);
    let name_told = |h: &Harness, b: usize, from: usize| {
        h.world_messages(b, from)
            .into_iter()
            .filter_map(|m| match m {
                ServerMessage::UpdateStringInfoVisual(u)
                    if u.object.id == victim_id
                        && u.quality == yalain_world::player::NAME_QUALITY =>
                {
                    Some(u.value)
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let (seen, own) = (h.bots[viewer].inbox.len(), h.bots[victim].inbox.len());
    send(
        &mut h,
        me,
        0x8201_0130,
        &[Arg::LongInt(victim_id), text("Renamed")],
    );
    h.run(20);
    let new = StringInfo::literal("Renamed [M]");
    assert_eq!(
        name_told(&h, viewer, seen),
        std::slice::from_ref(&new),
        "one who knows it"
    );
    assert_eq!(
        name_told(&h, victim, own),
        std::slice::from_ref(&new),
        "its own client"
    );

    h.bots[victim].leave_world();
    assert!(h.run_until(400, |h| !h
        .server
        .players
        .values()
        .any(|p| p.character == victim_id)));
    h.run(20);
    let from = h.bots[victim].inbox.len();
    h.enter_world(victim, "one", victim_id);
    let described = h
        .world_messages(victim, from)
        .into_iter()
        .find_map(|m| match m {
            ServerMessage::PlayerQualities(q) => Some(q.0.weenie.name),
            _ => None,
        })
        .expect("described");
    assert_eq!(described, new, "its own description, at its next entry");
}

/// A character leaves the world and comes back on the same client, seen to go.
fn relog(h: &mut Harness, b: usize, account: &str, id: u64) {
    h.bots[b].leave_world();
    assert!(h.run_until(400, |h| !h
        .server
        .players
        .values()
        .any(|p| p.character == id)));
    h.run(20);
    h.enter_world(b, account, id);
}

/// A cloak outlasts the character's stay, so an admin who thinks it is hidden stays hidden: it enters
/// seen by no player's client and told it is still cloaked. Nothing else its commands set lasts:
/// never-say-die, how creatures treat it, what it passes through, its speed and its sight all end.
/// Lifted, it enters as anyone does.
/// Behaviour: server.admin.cloak-kept-past-a-stay
#[test]
fn a_cloak_lasts_past_a_stay_and_nothing_else_an_admin_set_does() {
    use yalain_world::admin_calls::{AdminState, STILL_CLOAKED};
    let mut h = Harness::new();
    let (viewer, _) = h.player_in_world("zero", "Viewer");
    let (me, id) = admin_in_world(&mut h, "one", "Elara");
    h.run(20);
    set_all(&mut h, me);
    let seen_create = |h: &Harness, from: usize| {
        h.world_messages(viewer, from)
            .iter()
            .any(|m| matches!(m, ServerMessage::CreateObject(o) if o.id == id))
    };

    let (from, own) = (h.bots[viewer].inbox.len(), h.bots[me].inbox.len());
    relog(&mut h, me, "one", id);
    h.run(20);
    let p = h
        .server
        .players
        .values()
        .find(|p| p.character == id)
        .expect("in the world");
    assert_eq!(
        p.admin,
        AdminState {
            cloaked: true,
            ..AdminState::default()
        },
        "the cloak alone"
    );
    assert!(!seen_create(&h, from), "no client is told it came back");
    assert!(h.bots[me].notices(own).iter().any(|n| n == STILL_CLOAKED));
    let v = h
        .server
        .players
        .values()
        .find(|p| p.name == "Viewer")
        .expect("in the world");
    assert!(!v.known.contains_key(&id));

    send(&mut h, me, 0x8201_0061, &[]);
    h.run(20);
    let (from, own) = (h.bots[viewer].inbox.len(), h.bots[me].inbox.len());
    relog(&mut h, me, "one", id);
    h.run(20);
    assert!(seen_create(&h, from), "lifted, it is seen to enter");
    assert!(!h.bots[me].notices(own).iter().any(|n| n == STILL_CLOAKED));
}

/// A kept cloak is an admin's: taking admin away ends it whether the character is in the world or out
/// of it, and it doesn't come back with the rights. A character no longer an admin's that was out of
/// the world cloaked enters seen.
/// Behaviour: server.admin.taking-admin-away-ends-what-it-set
#[test]
fn taking_admin_away_ends_a_kept_cloak_in_the_world_and_out_of_it() {
    for (off, on, in_world) in [
        ("set-accountadmin one off", "set-accountadmin one", true),
        ("set-accountadmin one off", "set-accountadmin one", false),
        (
            "set-characteradmin Elara off",
            "set-characteradmin Elara",
            true,
        ),
        (
            "set-characteradmin Elara off",
            "set-characteradmin Elara",
            false,
        ),
    ] {
        let mut h = Harness::new();
        let (me, id) = admin_in_world(&mut h, "one", "Elara");
        send(&mut h, me, 0x8201_0017, &[]);
        let world = *h
            .server
            .players
            .iter()
            .find(|(_, p)| p.character == id)
            .map(|(w, _)| w)
            .expect("in the world");
        yalain_world::world::keep_player(&mut h.server, world);
        if in_world {
            console(&mut h, off);
            let st = h
                .server
                .store
                .world_state(id)
                .expect("reads")
                .expect("kept");
            let play = yalain_world::state::PlayState::from_bytes(&st.play).expect("reads");
            assert!(
                !play.cloaked,
                "{off}: forgotten at once, not at its leaving"
            );
        }
        h.bots[me].leave_world();
        assert!(h.run_until(400, |h| !h
            .server
            .players
            .values()
            .any(|p| p.character == id)));
        h.run(20);
        if !in_world {
            console(&mut h, off);
        }
        console(&mut h, on);
        h.enter_world(me, "one", id);
        let p = h
            .server
            .players
            .values()
            .find(|p| p.character == id)
            .expect("in the world");
        assert!(p.admin_character, "{off}: an admin's again");
        assert!(!p.admin.cloaked, "{off}, in the world {in_world}: no cloak");
    }

    // Its rights gone when it enters, whatever was kept: not cloaked.
    let mut h = Harness::new();
    let (me, id) = admin_in_world(&mut h, "one", "Elara");
    send(&mut h, me, 0x8201_0017, &[]);
    h.bots[me].leave_world();
    assert!(h.run_until(400, |h| !h
        .server
        .players
        .values()
        .any(|p| p.character == id)));
    h.run(20);
    let mut st = h
        .server
        .store
        .world_state(id)
        .expect("reads")
        .expect("kept");
    let mut play = yalain_world::state::PlayState::from_bytes(&st.play).expect("reads");
    assert!(play.cloaked, "it left cloaked");
    play.cloaked = true;
    st.play = play.to_bytes();
    h.server.store.set_world_state(id, &st).expect("kept");
    h.server
        .store
        .set_character_entity(id, yalain_world::player::PLAYER_ENTITY)
        .expect("kept");
    h.enter_world(me, "one", id);
    let p = h
        .server
        .players
        .values()
        .find(|p| p.character == id)
        .expect("in the world");
    assert!(!p.admin_character);
    assert!(!p.admin.cloaked, "an ordinary character is never cloaked");
}

/// A client's line to a chat room, as the client sends it: what the service answered, if anything.
fn room_line(h: &mut Harness, b: usize, room: u32, words: &str) -> Vec<(u32, u32)> {
    use auberean_protocol::login::ChatPacket;
    use dereth_protocol::turbine::{decode_incoming, ExtraBlob, IncomingPayload, SendToRoomById};
    let from = h.bots[b].inbox.len();
    let packet = SendToRoomById {
        context: 7,
        room,
        text: words.into(),
        extra: ExtraBlob::Speaker {
            sender: 1,
            chat_type: 100 << 16 | 1,
        },
    }
    .network_packet()
    .expect("packs");
    h.bots[b].send(false, 4, &ClientMessage::Chat(ChatPacket(packet)));
    h.run(20);
    h.bots[b].inbox[from..]
        .iter()
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
        .filter_map(|p| match p {
            IncomingPayload::RoomResponse(r) => Some((r.context, r.result)),
            _ => None,
        })
        .collect()
}

const ROOM_CREATE: u32 = 0x8201_002A;
const ROOM_JOIN: u32 = 0x8201_0005;
const ROOM_LEAVE: u32 = 0x8201_004E;
const ROOM_LEAVE_ALL: u32 = 0x8201_005A;

/// An admin's chat rooms: one made by name holds its maker and whoever joins by name; a line reaches
/// its members as an admin's answer and no one else; leaving, or the last member's leaving, ends it.
/// Bad names and rooms that are not there are answered.
/// Behaviour: server.admin.rooms-of-an-admins-own
#[test]
fn an_admins_chat_room_reaches_its_members_alone() {
    use yalain_world::chat::REFUSED;
    let mut h = Harness::new();
    let (player, _) = h.player_in_world("zero", "Viewer");
    let (a, _) = admin_in_world(&mut h, "one", "Elara");
    let (b, _) = admin_in_world(&mut h, "two", "Cara");
    let (c, _) = admin_in_world(&mut h, "three", "Babyfare");

    let said = |h: &mut Harness, who: usize, call: u32, args: &[Arg]| {
        let from = h.bots[who].inbox.len();
        send(h, who, call, args);
        answers(h, who, from)
    };
    assert_eq!(
        said(&mut h, b, ROOM_JOIN, &[text("Staff")]),
        ["There is no room Staff."]
    );
    assert_eq!(
        said(&mut h, a, ROOM_CREATE, &[text("Staff")]),
        ["You made room 1000000, Staff, and are in it."]
    );
    assert_eq!(
        said(&mut h, b, ROOM_CREATE, &[text("staff")]),
        ["There is a room staff already: room 1000000."]
    );
    assert_eq!(
        said(&mut h, b, ROOM_JOIN, &[text("STAFF")]),
        ["You are in room 1000000, Staff."]
    );
    for bad in ["", "   ", "two\nlines", &"x".repeat(65)] {
        assert_eq!(
            said(&mut h, b, ROOM_CREATE, &[text(bad)]),
            ["A room's name is 1 to 64 characters, on one line."],
            "{bad:?}"
        );
    }
    let room = 1_000_000;
    let heard: [usize; 4] = [player, a, b, c].map(|w| h.bots[w].inbox.len());
    assert_eq!(
        room_line(&mut h, a, room, "hello"),
        [(7, 0)],
        "answered done"
    );
    assert_eq!(answers(&h, a, heard[1]), ["[Staff] Elara: hello"]);
    assert_eq!(answers(&h, b, heard[2]), ["[Staff] Elara: hello"]);
    assert_eq!(
        answers(&h, c, heard[3]),
        Vec::<String>::new(),
        "not a member"
    );
    assert_eq!(
        answers(&h, player, heard[0]),
        Vec::<String>::new(),
        "nor a player"
    );

    // A line from outside goes nowhere, answered as one to a room that isn't there is.
    let nowhere = room_line(&mut h, c, 999_999, "anyone");
    let shut_out = room_line(&mut h, c, room, "let me in");
    assert_eq!(nowhere, [(7, REFUSED)]);
    assert_eq!(shut_out, nowhere, "an admin outside it");
    assert_eq!(
        room_line(&mut h, player, room, "let me in"),
        nowhere,
        "a player"
    );
    let quiet = h.bots[a].inbox.len();
    h.run(5);
    assert_eq!(answers(&h, a, quiet), Vec::<String>::new());

    assert_eq!(
        said(&mut h, a, ROOM_LEAVE, &[Arg::Int(room)]),
        [format!("You left room {room}.")]
    );
    assert_eq!(
        said(&mut h, a, ROOM_LEAVE, &[Arg::Int(room)]),
        [format!("You are not in room {room}.")]
    );
    assert_eq!(room_line(&mut h, a, room, "still here?"), nowhere, "left");
    assert_eq!(
        said(&mut h, b, ROOM_LEAVE_ALL, &[]),
        ["You left every room."]
    );
    assert_eq!(
        room_line(&mut h, b, room, "anyone"),
        nowhere,
        "gone with its last"
    );
    assert!(h.server.admin_rooms.rooms.is_empty());
}

/// A place in an admin's room needs an admin's rights at that moment: taking admin away, or leaving
/// the world, takes it, and a line to the room then goes nowhere. A player's calls of these do nothing
/// at all.
/// Behaviour: server.admin.taking-admin-away-ends-what-it-set
#[test]
fn only_an_admin_in_the_world_stays_in_an_admins_room() {
    use yalain_world::chat::REFUSED;
    for line in ["set-accountadmin two off", "set-characteradmin Cara off"] {
        let mut h = Harness::new();
        let (player, _) = h.player_in_world("zero", "Viewer");
        let (a, _) = admin_in_world(&mut h, "one", "Elara");
        let (b, cara) = admin_in_world(&mut h, "two", "Cara");
        send(&mut h, a, ROOM_CREATE, &[text("Staff")]);
        send(&mut h, b, ROOM_JOIN, &[text("Staff")]);
        let room = 1_000_000;
        console(&mut h, line);
        h.run(5);
        assert!(
            !h.server.admin_rooms.rooms[&room].members.contains(&cara),
            "{line}: out of it"
        );
        let (ha, hb) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
        assert_eq!(
            room_line(&mut h, b, room, "secret"),
            [(7, REFUSED)],
            "{line}"
        );
        assert_eq!(answers(&h, a, ha), Vec::<String>::new(), "{line}: unheard");
        assert_eq!(room_line(&mut h, a, room, "hello"), [(7, 0)], "{line}");
        assert_eq!(
            answers(&h, b, hb),
            Vec::<String>::new(),
            "{line}: hears nothing"
        );

        // A player's calls of these are the gate's.
        let refused = h.bots[player].inbox.len();
        send(&mut h, player, ROOM_JOIN, &[text("Staff")]);
        send(&mut h, player, ROOM_CREATE, &[text("Mine")]);
        assert_eq!(h.server.admin_rooms.rooms.len(), 1, "{line}");
        assert_eq!(answers(&h, player, refused), Vec::<String>::new());
    }

    // Rights lost another way than the console's own path still count at the moment of a line.
    let mut h = Harness::new();
    let (a, _) = admin_in_world(&mut h, "one", "Elara");
    let (b, cara) = admin_in_world(&mut h, "two", "Cara");
    send(&mut h, a, ROOM_CREATE, &[text("Staff")]);
    send(&mut h, b, ROOM_JOIN, &[text("Staff")]);
    let room = 1_000_000;
    for x in h.server.sessions.values_mut() {
        if x.account.name == "two" {
            x.account.admin = false;
        }
    }
    let (ha, hb) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    assert_eq!(room_line(&mut h, b, room, "secret"), [(7, REFUSED)]);
    assert_eq!(
        answers(&h, a, ha),
        Vec::<String>::new(),
        "checked as it speaks"
    );
    assert_eq!(room_line(&mut h, a, room, "hello"), [(7, 0)]);
    assert_eq!(
        answers(&h, b, hb),
        Vec::<String>::new(),
        "checked as it hears"
    );

    // Leaving the world leaves the room.
    h.bots[a].leave_world();
    assert!(h.run_until(400, |h| h
        .server
        .players
        .values()
        .all(|p| p.name != "Elara")));
    h.run(20);
    assert!(h.server.admin_rooms.rooms[&room].members.contains(&cara));
    assert_eq!(
        h.server.admin_rooms.rooms[&room].members.len(),
        1,
        "left it"
    );
}

/// `/suffix` puts words after a player's name, for every client that knows it and for good; empty, it
/// takes them off. Nothing that reads as the name's grammar mark or breaks a line is taken.
/// Behaviour: server.admin.suffix-shown-after-the-name
#[test]
fn a_suffix_shows_after_the_name_for_every_client_and_lasts() {
    use auberean_protocol::text::StringInfo;
    const SUFFIX: u32 = 0x8201_0113;
    let mut h = Harness::new();
    let (victim, id) = h.player_in_world("one", "Elara");
    let (viewer, _) = h.player_in_world("three", "Babyfare");
    let (me, _) = admin_in_world(&mut h, "two", "Cara");
    h.run(20);
    let told = |h: &Harness, b: usize, from: usize| {
        h.world_messages(b, from)
            .into_iter()
            .filter_map(|m| match m {
                ServerMessage::UpdateStringInfoVisual(u)
                    if u.object.id == id && u.quality == yalain_world::player::NAME_QUALITY =>
                {
                    Some(u.value)
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let (seen, own) = (h.bots[viewer].inbox.len(), h.bots[victim].inbox.len());
    send(
        &mut h,
        me,
        SUFFIX,
        &[Arg::LongInt(id), text("\"the Bold\"")],
    );
    h.run(20);
    let new = StringInfo::literal("Elara the Bold [Mv]");
    assert_eq!(
        told(&h, viewer, seen),
        std::slice::from_ref(&new),
        "one who knows it"
    );
    assert_eq!(
        told(&h, victim, own),
        std::slice::from_ref(&new),
        "its own client"
    );
    for bad in ["[F]", "two\nlines", "<b>", &"x".repeat(25)] {
        let from = h.bots[viewer].inbox.len();
        let said = h.bots[me].inbox.len();
        send(&mut h, me, SUFFIX, &[Arg::LongInt(id), text(bad)]);
        h.run(20);
        assert_eq!(told(&h, viewer, from), [], "{bad:?}");
        assert!(
            answers(&h, me, said)[0].starts_with("A suffix is"),
            "{bad:?}"
        );
    }

    let from = h.bots[victim].inbox.len();
    relog(&mut h, victim, "one", id);
    h.run(20);
    let described = h
        .world_messages(victim, from)
        .into_iter()
        .find_map(|m| match m {
            ServerMessage::PlayerQualities(q) => Some(q.0.weenie.name),
            _ => None,
        })
        .expect("described");
    assert_eq!(described, new, "its own description, at its next entry");

    let (fresh, _) = h.player_in_world("four", "Wrenna");
    h.run(20);
    let created = h
        .world_messages(fresh, 0)
        .into_iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.id == id => Some(o.weenie.name),
            _ => None,
        })
        .expect("seen");
    assert_eq!(created, new, "one who meets it later");

    let seen = h.bots[viewer].inbox.len();
    send(&mut h, me, SUFFIX, &[Arg::LongInt(id), text("")]);
    h.run(20);
    assert_eq!(
        told(&h, viewer, seen),
        [StringInfo::literal("Elara [Mv]")],
        "taken off"
    );
}

/// A suffix an admin gave itself goes with its rights, in the world or out of it, for it may say it is
/// staff.
/// Behaviour: server.admin.taking-admin-away-ends-what-it-set
#[test]
fn taking_admin_away_takes_a_suffix_off_its_name() {
    const SUFFIX: u32 = 0x8201_0113;
    for in_world in [true, false] {
        let mut h = Harness::new();
        let (me, id) = admin_in_world(&mut h, "one", "Elara");
        send(&mut h, me, SUFFIX, &[Arg::LongInt(id), text("Staff")]);
        if in_world {
            console(&mut h, "set-accountadmin one off");
        }
        h.bots[me].leave_world();
        assert!(h.run_until(400, |h| !h
            .server
            .players
            .values()
            .any(|p| p.character == id)));
        h.run(20);
        if !in_world {
            console(&mut h, "set-accountadmin one off");
        }
        let from = h.bots[me].inbox.len();
        h.enter_world(me, "one", id);
        let p = h
            .server
            .players
            .values()
            .find(|p| p.character == id)
            .expect("in the world");
        assert_eq!(p.shown_name(), "Elara", "in the world {in_world}");
        let described = h
            .world_messages(me, from)
            .into_iter()
            .find_map(|m| match m {
                ServerMessage::PlayerQualities(q) => Some(q.0.weenie.name),
                _ => None,
            })
            .expect("described");
        assert_eq!(
            described,
            auberean_protocol::text::StringInfo::literal("Elara [Mv]"),
            "in the world {in_world}"
        );
    }
}

/// `/gunservertell` shows a player the words, by its name; a name not in the world is answered.
/// Behaviour: server.admin.rooms-of-an-admins-own
#[test]
fn a_server_tell_reaches_a_player_by_name() {
    const SERVER_TELL: u32 = 0x8201_0084;
    let mut h = Harness::new();
    let (victim, _) = h.player_in_world("one", "Elara");
    let (me, _) = admin_in_world(&mut h, "two", "Cara");
    let from = h.bots[victim].inbox.len();
    send(
        &mut h,
        me,
        SERVER_TELL,
        &[text("Elara"), text("Hello there")],
    );
    assert_eq!(h.bots[victim].notices(from), ["Hello there"]);
    h.server
        .players
        .values_mut()
        .find(|p| p.name == "Elara")
        .expect("in the world")
        .play
        .suffix = "the Bold".to_owned();
    let from = h.bots[victim].inbox.len();
    send(
        &mut h,
        me,
        SERVER_TELL,
        &[text("Elara the Bold [Fv]"), text("And again")],
    );
    assert_eq!(h.bots[victim].notices(from), ["And again"], "as shown");
    let said = h.bots[me].inbox.len();
    send(&mut h, me, SERVER_TELL, &[text("Nobody"), text("Hello")]);
    assert_eq!(answers(&h, me, said), ["Nobody is not in the world."]);
}
/// A player typing admin commands as fast as it can talk fills no log: its first line each stay is
/// logged, every one is counted, and each is said as usual.
/// Behaviour: server.admin.refused-lines-log-one-line
#[test]
fn refused_admin_lines_log_one_line_a_stay_however_many_come() {
    let mut h = Harness::new();
    let (player, _) = h.player_in_world("one", "Elara");
    let (other, _) = h.player_in_world("two", "Babyfare");
    let line = |h: &mut Harness, b: usize, text: &str| {
        let call = server_call(
            yalain_world::requests::function::SAY,
            &[Arg::Int(0), info(text)],
        );
        h.bots[b].send(true, queue::WEENIE, &call);
        h.run(10);
    };
    for _ in 0..3 {
        line(&mut h, player, "@accountlist");
    }
    assert_eq!(h.server.admin_calls.refused_lines, 3, "all counted");
    assert_eq!(h.server.admin_calls.logged_lines, 1, "one line");
    line(&mut h, player, "@nothing");
    assert_eq!(
        h.server.admin_calls.refused_lines, 3,
        "a word that is no command is only said"
    );
    line(&mut h, other, "@accountlist");
    assert_eq!(
        h.server.admin_calls.logged_lines, 2,
        "one for another character"
    );
}

/// An effect an admin shows or ends on a player or a creature carries that object's next visual
/// stamp, of its own incarnation, for every client that knows it: a stamp of its own would be dropped
/// or would hold back what follows. What the server sends of the object next follows on.
/// Behaviour: server.admin.effect-stamps-follow-the-object
#[test]
fn an_effect_an_admin_shows_follows_the_objects_own_stamps() {
    const DO_FX: u32 = 0x8201_0032;
    const STOP_FX: u32 = 0x8201_0040;
    const FX: u32 = 0x4100_0010;
    let mut h = Harness::new();
    h.add_creatures(1);
    let (me, id) = admin_in_world(&mut h, "one", "Elara");
    let (other, _) = h.player_in_world("two", "Babyfare");
    h.run(20);
    let sent = |h: &Harness, b: usize, about: u64| {
        h.bots[b]
            .messages(true)
            .into_iter()
            .filter_map(|m| match m {
                ServerMessage::DoFx(e) if e.sender.id == about => Some((e.sender, e.action.fx)),
                ServerMessage::StopFx(e) if e.sender.id == about => Some((e.sender, e.action.0)),
                _ => None,
            })
            .next_back()
    };
    let player = |h: &Harness| {
        let p = h.server.players.values().find(|p| p.character == id);
        let p = p.expect("in the world");
        (p.instance, p.visual)
    };
    for (f, args) in [
        (
            DO_FX,
            vec![
                Arg::LongInt(id),
                Arg::Int(FX),
                Arg::Float(1.0_f32.to_bits()),
            ],
        ),
        (STOP_FX, vec![Arg::LongInt(id), Arg::Int(FX)]),
    ] {
        let (instance, visual) = player(&h);
        send(&mut h, me, f, &args);
        for b in [me, other] {
            let (stamp, fx) = sent(&h, b, id).expect("told");
            assert_eq!(fx, FX);
            assert_eq!(stamp.instance_stamp, instance);
            assert_eq!(stamp.other_stamp, visual.wrapping_add(1));
        }
        assert_eq!(player(&h), (instance, visual.wrapping_add(1)));
    }

    let creature = h
        .server
        .things
        .placed()
        .find(|t| t.kind == yalain_world::things::Kind::Creature)
        .map(|t| t.id)
        .expect("a creature");
    let of = |h: &Harness| {
        let t = h.server.things.get(creature).expect("there");
        (t.instance, t.visual)
    };
    for (f, args) in [
        (
            DO_FX,
            vec![
                Arg::LongInt(creature),
                Arg::Int(FX),
                Arg::Float(1.0_f32.to_bits()),
            ],
        ),
        (STOP_FX, vec![Arg::LongInt(creature), Arg::Int(FX)]),
    ] {
        let (instance, visual) = of(&h);
        send(&mut h, me, f, &args);
        let (stamp, _) = sent(&h, me, creature).expect("told");
        assert_eq!(stamp.instance_stamp, instance);
        assert_eq!(stamp.other_stamp, visual.wrapping_add(1));
        assert_eq!(of(&h).1, visual.wrapping_add(1));
    }
}
