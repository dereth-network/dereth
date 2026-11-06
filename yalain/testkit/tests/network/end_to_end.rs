//! The run: three worlds of two games on one hub, and a player in each.

use std::time::{Duration, Instant};

use auberean_friends::{Hr, Mode, Request};

use crate::first_game;
use crate::programs::{
    first_game_ports, first_game_world, moved, program, register, second_game_ports, settle,
    start_first_game_world, start_second_game_world, wait_for, Hub, Scratch, FIRST_GAME_READY,
    HANG, SECOND_GAME_READY,
};
use crate::run::Run;
use crate::second_game;

/// The line a character off the network is refused with.
const NOT_ON: &str = "You are not on the Dereth Network. Say @dn on to join it.";

/// The refusal of another world's friend while the character or its world is off the network.
const FAILED: Hr = Hr(0x8000_4005);

fn tell(user: &str, text: &str) -> Request {
    Request::SendToUserEx {
        user: user.into(),
        text: text.into(),
        extra: None,
    }
}

fn friend(user: &str) -> Request {
    Request::FriendAdd { user: user.into() }
}

/// The hub, a world of the first game (`aa`) and two of this game (`bb`, `cc`), all on the
/// network, each a program of its own on loopback, with Ann in `bb`, Cara in `cc` and Bob in `aa`,
/// each in its own game's client and Ann and Cara with friends streams:
/// - each world is welcomed by the hub, and a world's console shows the hub connected;
/// - off the network, Ann is refused another world's friend and tell, with the line saying how to
///   join; `@dn on` said in chat puts each of them on it, and Ann's world lists her in the
///   network's directory and tells her her page;
/// - between this game's worlds, friends see each other online and tells go both ways;
/// - across games, Bob's `@friends add` and Ann's friend each show the other online, and tells go
///   both ways, each shown in its own game's client;
/// - a player leaving the world shows offline to friends on both other worlds, and online again
///   on its return; a first-game player quitting shows offline to a friend of this game;
/// - `@dn off` shows Ann offline to another world and its characters offline to her, refuses
///   her tells out and answers tells to her as to one not online; `@dn on` again shows each
///   online to the other;
/// - a world whose link drops shows its characters offline to their friends elsewhere.
///
/// Behaviour: server.network.three-worlds-of-two-games-on-one-hub
#[test]
#[allow(clippy::too_many_lines)]
fn players_of_both_games_reach_each_other_across_three_worlds_on_one_hub() {
    let scratch = Scratch::new("dereth-network");
    let dir = scratch.dir.clone();
    let hub_program = program("DERETH_NETWORK_HUB_BIN", "dereth-network-hub");
    let (pa, pb, pc) = (first_game_ports(), second_game_ports(), second_game_ports());
    let key_a = register(&hub_program, &dir, "aa", "dereth", "Empyrean", pa);
    let key_b = register(&hub_program, &dir, "bb", "auberean", "Yalain", pb);
    let key_c = register(&hub_program, &dir, "cc", "auberean", "Yalain", pc);
    let hub = Hub::start(&hub_program, &dir);
    let world = first_game_world(&dir);
    // Every world started, then each waited for until it has its ports. Each port was let go
    // before the world took it, so another program can take one first: the world then starts
    // again on fresh ports, its registration moved there.
    let aa = start_first_game_world(&dir, "aa", pa, hub.port, &key_a, &world);
    let bb = start_second_game_world(&dir, "bb", pb, hub.port, &key_b);
    let cc = start_second_game_world(&dir, "cc", pc, hub.port, &key_c);
    let watched = [&hub.process];
    let aa = settle(aa, pa, FIRST_GAME_READY, &watched, || {
        let p = first_game_ports();
        moved(&hub_program, &dir, "aa", "dereth", "Empyrean", p);
        (
            start_first_game_world(&dir, "aa", p, hub.port, &key_a, &world),
            p,
        )
    });
    let second = |slug: &str, process, port, key: &str| {
        settle(process, port, SECOND_GAME_READY, &watched, || {
            let p = second_game_ports();
            moved(&hub_program, &dir, slug, "auberean", "Yalain", p);
            (start_second_game_world(&dir, slug, p, hub.port, key), p)
        })
    };
    let (bb, cc) = (second("bb", bb, pb, &key_b), second("cc", cc, pc, &key_c));
    let (pa, pb, pc) = (aa.port, bb.port, cc.port);
    let worlds = vec![aa.process, bb.process, cc.process];
    let mut run = Run {
        second: Vec::new(),
        first: Vec::new(),
        worlds,
        hub,
        scratch,
    };
    for slug in ["aa", "bb", "cc"] {
        let welcome = format!("hub -> {slug}: welcome");
        wait_for(
            &format!("{slug} linked and welcomed"),
            &run.programs(),
            || run.hub.carried(&welcome) > 0,
            || run.context(),
        );
    }

    // The operator's console sees the real hub. The hub's welcome can still be on its way to the
    // world, so `dn` is asked again after each answer until the world has taken it.
    let connected = "This world is on the Dereth Network as bb. The hub is connected.";
    let status = "This world is on the Dereth Network as bb. The hub is";
    let until = Instant::now() + HANG;
    let mut asked = 0;
    loop {
        run.check();
        let bb = run.world("bb");
        if bb.saw(connected) {
            break;
        }
        if bb.count(status) >= asked {
            bb.type_line("dn");
            asked += 1;
        }
        assert!(
            Instant::now() < until,
            "bb's console shows the hub: it never came\n{}",
            run.context()
        );
        std::thread::sleep(Duration::from_millis(20));
    }

    // Everyone in the world.
    run.second.push(second_game::Player::new("Ann", pb));
    run.second.push(second_game::Player::new("Cara", pc));
    let ann = run.enter2("Ann");
    run.enter2("Cara");
    run.first.push(first_game::Player::connect("Bob", pa, &dir));
    run.until("Bob in the world", |r| r.p1("Bob").enter());

    // Off the network: another world's friend and tell refused, with the line.
    let from = run.lines2("Ann");
    assert_eq!(run.ask("Ann", friend("Cara@cc")), FAILED);
    run.shown2("Ann", from, NOT_ON);
    let from = run.lines2("Ann");
    assert_eq!(run.ask("Ann", tell("Bob@aa", "anyone there?")), Hr::DENIED);
    run.shown2("Ann", from, NOT_ON);

    // On the network, said in chat; Ann listed in the directory and told her page.
    let from = run.lines2("Ann");
    run.say2("Ann", "@dn on");
    run.shown2("Ann", from, "You are on the Dereth Network as Ann@bb.");
    run.shown2("Ann", from, "Your directory page: ");
    assert!(run.hub.carried("bb -> hub: listing") > 0, "Ann listed");
    let from = run.lines2("Cara");
    run.say2("Cara", "@dn on");
    run.shown2("Cara", from, "You are on the Dereth Network as Cara@cc.");
    run.p1("Bob").type_line("@dn on");
    run.until("Bob on the network", |r| {
        r.p1("Bob")
            .shown_since(0, "You are on the Dereth Network as Bob@aa.")
    });

    // Between this game's worlds.
    let mark = run.p2("Ann").stream.got.len();
    assert_eq!(run.ask("Ann", friend("Cara@cc")), Hr::OK);
    run.until("Ann sees Cara online", |r| {
        r.p2("Ann").stream.mode_since(mark, "Cara@cc") == Some(Mode::ONLINE)
    });
    let mark = run.p2("Cara").stream.got.len();
    assert_eq!(run.ask("Cara", friend("Ann@bb")), Hr::OK);
    run.until("Cara sees Ann online", |r| {
        r.p2("Cara").stream.mode_since(mark, "Ann@bb") == Some(Mode::ONLINE)
    });
    let mark = run.p2("Cara").stream.got.len();
    assert_eq!(run.ask("Ann", tell("Cara@cc", "hello from bb")), Hr::OK);
    run.until("Cara hears Ann", |r| {
        r.p2("Cara")
            .stream
            .told_since(mark, "Ann@bb", "hello from bb")
    });
    let mark = run.p2("Ann").stream.got.len();
    assert_eq!(run.ask("Cara", tell("Ann@bb", "hello from cc")), Hr::OK);
    run.until("Ann hears Cara", |r| {
        r.p2("Ann")
            .stream
            .told_since(mark, "Cara@cc", "hello from cc")
    });

    // Across games.
    let from = run.p1("Bob").chat.len();
    run.p1("Bob").type_line("@friends add Ann@bb");
    run.until("Bob adds Ann and sees her online", |r| {
        r.p1("Bob")
            .shown_since(from, "Ann@bb has been added to your friends list.")
            && r.p1("Bob").friend("Ann@bb") == Some(true)
    });
    let mark = run.p2("Ann").stream.got.len();
    assert_eq!(run.ask("Ann", friend("Bob@aa")), Hr::OK);
    run.until("Ann sees Bob online", |r| {
        r.p2("Ann").stream.mode_since(mark, "Bob@aa") == Some(Mode::ONLINE)
    });
    let from = run.p1("Bob").chat.len();
    assert_eq!(
        run.ask("Ann", tell("Bob@aa", "hi from the other game")),
        Hr::OK
    );
    run.until("Bob hears Ann", |r| {
        r.p1("Bob")
            .shown_since(from, "Ann@bb<\\Tell> tells you, \"hi from the other game\"")
    });
    let mark = run.p2("Ann").stream.got.len();
    run.p1("Bob").type_line("@tell Ann@bb, and hello back");
    run.until("Ann hears Bob", |r| {
        r.p2("Ann")
            .stream
            .told_since(mark, "Bob@aa", "and hello back")
    });

    // The Global room: each world carries it in its own room, and a line said in it reaches the
    // others, of either game. A new character of the first game does not listen to the room its
    // world carries it in, so its world's part is what the hub hands it.
    let to_cara = run.lines_of_rooms("Cara");
    run.say2("Ann", "@dng dance at 8");
    run.until("the hub hands Ann's line to the first game's world", |r| {
        r.hub.carried("hub -> aa: room.line") > 0
    });
    run.until("Cara hears Ann in the Global room", |r| {
        r.heard_in_room("Cara", to_cara, "Ann@bb")
    });
    let to_ann = run.lines_of_rooms("Ann");
    run.p1("Bob").type_line("@dng and at 9");
    run.until("Ann hears Bob in the Global room", |r| {
        r.heard_in_room("Ann", to_ann, "Bob@aa")
    });

    // Leaving the world, and coming back.
    let mark = run.p2("Cara").stream.got.len();
    run.log_off2("Ann");
    run.until("Ann offline to Bob and Cara", |r| {
        r.p1("Bob").friend("Ann@bb") == Some(false)
            && r.p2("Cara").stream.mode_since(mark, "Ann@bb") == Some(Mode::OFFLINE)
    });
    let mark = run.p2("Cara").stream.got.len();
    run.enter_again2("Ann", ann);
    run.until("Ann online to Bob and Cara again", |r| {
        r.p1("Bob").friend("Ann@bb") == Some(true)
            && r.p2("Cara").stream.mode_since(mark, "Ann@bb") == Some(Mode::ONLINE)
    });
    let mark = run.p2("Ann").stream.got.len();
    run.p1("Bob").quit();
    run.until("Bob offline to Ann", |r| {
        r.p2("Ann").stream.mode_since(mark, "Bob@aa") == Some(Mode::OFFLINE)
    });

    // Off the network again: offline to Cara, and Cara offline to her, since nothing of another
    // world is watched for her now.
    let from = run.lines2("Ann");
    let (mark, mine) = (
        run.p2("Cara").stream.got.len(),
        run.p2("Ann").stream.got.len(),
    );
    run.say2("Ann", "@dn off");
    run.shown2("Ann", from, "You are off the Dereth Network.");
    run.until("Ann and Cara offline to each other", |r| {
        r.p2("Cara").stream.mode_since(mark, "Ann@bb") == Some(Mode::OFFLINE)
            && r.p2("Ann").stream.mode_since(mine, "Cara@cc") == Some(Mode::OFFLINE)
    });
    let from = run.lines2("Ann");
    assert_eq!(run.ask("Ann", tell("Cara@cc", "still there?")), Hr::DENIED);
    run.shown2("Ann", from, NOT_ON);
    assert_eq!(
        run.ask("Cara", tell("Ann@bb", "where did you go?")),
        Hr::NOT_ONLINE
    );
    let (mark, mine) = (
        run.p2("Cara").stream.got.len(),
        run.p2("Ann").stream.got.len(),
    );
    run.say2("Ann", "@dn on");
    run.until("Ann and Cara online to each other again", |r| {
        r.p2("Cara").stream.mode_since(mark, "Ann@bb") == Some(Mode::ONLINE)
            && r.p2("Ann").stream.mode_since(mine, "Cara@cc") == Some(Mode::ONLINE)
    });

    // A world's link drops.
    let mark = run.p2("Ann").stream.got.len();
    let downs = run.hub.process.count("link down: cc");
    run.world("cc").kill();
    run.until("Cara offline to Ann once cc's link drops", |r| {
        r.hub.process.count("link down: cc") > downs
            && r.p2("Ann").stream.mode_since(mark, "Cara@cc") == Some(Mode::OFFLINE)
    });

    println!("{}", run.context());
}
