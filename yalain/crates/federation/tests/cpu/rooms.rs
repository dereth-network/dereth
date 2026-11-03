//! The Global room and the world's own rooms: the setting, the hub's word, a line each way,
//! `@dn admin global` and a line said from elsewhere.

use dereth_network_protocol::limits::{Limits, Rate};
use dereth_network_protocol::rooms::UNCARRIED_SECS;
use dereth_network_protocol::say::{SayMessage, SayOutcome};
use dereth_network_protocol::wire::GLOBAL_ROOM;
use dereth_network_protocol::{Message, Outcome, RoomsMessage};
use yalain_federation::{
    Character, Federation, Heard, Room, SayAsk, Settings, Speaker, HELP, IN_GAME_HELP,
};

use crate::{addr, World, SLUG};

/// A world whose setting says `room`, its link down.
fn carrying(room: Option<&'static str>) -> World {
    let mut w = World::new();
    w.fed = Federation::new(settings(room), 1_790_000_000);
    w
}

fn settings(room: Option<&'static str>) -> Settings {
    Settings {
        enabled: true,
        slug: SLUG.into(),
        software: "yalain test".into(),
        global_room: room,
        global_room_named: false,
        websocket_url: None,
    }
}

/// The hub's word on the world's rooms.
fn told(w: &mut World, global: Option<&str>, mirrored: &[&str]) {
    let m = RoomsMessage::Setup {
        id: None,
        global: global.map(str::to_owned),
        mirrored: mirrored.iter().map(|r| (*r).to_owned()).collect(),
    };
    w.frame(&m.to_json());
}

/// A character on the network, by its id.
fn on(w: &mut World, name: &str) -> u64 {
    w.on_and_present(name);
    w.ids[name]
}

/// `name` says `text` in `room`, after it has reached the world's own players.
fn said(w: &mut World, name: &str, room: Room, text: &str) {
    let id = w.ids[name];
    w.fed.room_said(&mut w.service, w.t, id, name, room, text);
}

/// The world's `room.say`s now, by room, with their ids and marks; everything else it sends is
/// dropped.
fn lines_out(w: &mut World) -> Vec<(String, String, Option<String>)> {
    w.frames()
        .into_iter()
        .filter_map(|m| match m {
            Message::RoomSay { id, room, via, .. } => Some((id, room, via)),
            _ => None,
        })
        .collect()
}

fn joins(frames: &[Message]) -> Vec<(String, bool)> {
    frames
        .iter()
        .filter_map(|m| match m {
            Message::RoomJoin { room, on } => Some((room.clone(), *on)),
            _ => None,
        })
        .collect()
}

/// A line from another world's character.
fn line_in(w: &mut World, room: &str, from: &str, text: &str, via: Option<&str>) {
    w.hub(&Message::RoomLine {
        room: room.into(),
        from: addr(from),
        text: text.into(),
        via: via.map(str::to_owned),
    });
}

/// The frames the world sends now, as text: `rooms.global` is no message of the link's family.
fn raw(w: &mut World) -> Vec<String> {
    w.fed.take_frames(w.t)
}

/// `general` and `trade` are General and Trade; this client has no room for looking for company or
/// for roleplay, so `lfg` and `roleplay` are PvP; nothing else is a room.
/// Behaviour: server.network.global-room-words-name-this-clients-rooms
#[test]
fn each_of_the_hubs_room_names_is_a_room_this_client_has() {
    assert_eq!(Room::of("general"), Some(Room::General));
    assert_eq!(Room::of("trade"), Some(Room::Trade));
    assert_eq!(Room::of("lfg"), Some(Room::Pvp));
    assert_eq!(Room::of("roleplay"), Some(Room::Pvp));
    for word in ["off", "global", "pvp", "General", ""] {
        assert_eq!(Room::of(word), None, "{word}");
    }
    let names: Vec<&str> = [Room::General, Room::Trade, Room::Pvp]
        .iter()
        .map(|r| r.name())
        .collect();
    assert_eq!(names, ["General", "Trade", "PvP"]);
    for (word, room) in [
        ("general", Room::General),
        ("trade", Room::Trade),
        ("lfg", Room::Pvp),
        ("roleplay", Room::Pvp),
    ] {
        assert_eq!(carrying(Some(word)).fed.global_room(), Some(room), "{word}");
    }
}

/// A world joins the Global room at its welcome, ahead of what was queued, and a hub that holds no
/// room for it is told the room its setting names, once on each link.
/// Behaviour: server.network.global-room-carried-as-the-setting-says
#[test]
fn a_world_joins_the_global_room_and_tells_a_hub_that_holds_none_its_setting() {
    let mut w = carrying(Some("general"));
    assert_eq!(
        joins(&w.link(&[])),
        [(GLOBAL_ROOM.to_owned(), true)],
        "its first frame after the welcome"
    );
    assert_eq!(w.fed.global_room(), Some(Room::General));

    told(&mut w, None, &[]);
    let seed = raw(&mut w);
    assert_eq!(seed.len(), 1, "{seed:?}");
    let Ok(RoomsMessage::Global {
        room, seed: true, ..
    }) = RoomsMessage::from_json(&seed[0])
    else {
        panic!("a seed: {seed:?}")
    };
    assert_eq!(room, "general");
    told(&mut w, None, &[]);
    assert_eq!(raw(&mut w), Vec::<String>::new(), "once on each link");

    // The next link's hub may hold none as well.
    w.fed.on_link_down(&mut w.service, w.t);
    w.link(&[]);
    told(&mut w, None, &[]);
    assert_eq!(raw(&mut w).len(), 1, "and once on the next");

    // A hub that keeps nothing never tells it its rooms: the setting holds.
    let mut older = carrying(Some("trade"));
    older.link(&[]);
    assert_eq!(older.fed.global_room(), Some(Room::Trade));
    assert_eq!(raw(&mut older), Vec::<String>::new());

    // Nor does a world join a room the hub does not offer.
    let mut none = carrying(Some("general"));
    none.fed.on_link_up();
    none.frames();
    none.hub(&Message::Welcome {
        proto: dereth_network_protocol::wire::PROTOCOL,
        world: SLUG.into(),
        hub: "test hub".into(),
        worlds: Vec::new(),
        rooms: Vec::new(),
        limits: Limits::DEFAULT,
    });
    assert_eq!(joins(&none.frames()), []);
}

/// The hub's room has the last word over the setting, holds at once (joining or leaving the Global
/// room as it goes on or off), and is kept, so that the world carries it across a restart.
/// Behaviour: server.network.global-room-as-the-hub-says
#[test]
fn the_hubs_room_has_the_last_word_and_is_kept_across_a_restart() {
    let mut w = carrying(Some("general"));
    w.link(&[]);
    told(&mut w, Some("roleplay"), &[]);
    assert_eq!(w.fed.global_room(), Some(Room::Pvp));
    assert_eq!(
        raw(&mut w),
        Vec::<String>::new(),
        "no seed to a hub that holds one"
    );

    told(&mut w, Some("off"), &[]);
    assert_eq!(w.fed.global_room(), None);
    assert_eq!(joins(&w.frames()), [(GLOBAL_ROOM.to_owned(), false)]);
    told(&mut w, Some("trade"), &[]);
    assert_eq!(w.fed.global_room(), Some(Room::Trade));
    assert_eq!(joins(&w.frames()), [(GLOBAL_ROOM.to_owned(), true)]);

    // A hub that holds none later leaves the room it last said.
    told(&mut w, None, &[]);
    assert_eq!(w.fed.global_room(), Some(Room::Trade));

    w.fed.keep(&mut w.store).expect("kept");
    let again = Federation::load(settings(Some("general")), 1, &mut w.store).expect("read");
    assert_eq!(
        again.global_room(),
        Some(Room::Trade),
        "the hub's, not the setting's"
    );
}

/// A world whose setting is empty carries no Global room: it joins nothing, tells a hub that holds
/// none `off`, sends no line, shows no line, and its status says so.
/// Behaviour: server.network.global-room-empty-carries-nothing
#[test]
fn a_world_whose_setting_is_empty_carries_nothing() {
    let mut w = carrying(None);
    on(&mut w, "Ann");
    assert_eq!(joins(&w.link(&[])), []);
    assert_eq!(w.fed.global_room(), None);

    told(&mut w, None, &[]);
    let seed = raw(&mut w);
    assert!(
        matches!(RoomsMessage::from_json(&seed[0]), Ok(RoomsMessage::Global { room, seed: true, .. }) if room == "off"),
        "{seed:?}"
    );

    for room in [Room::General, Room::Trade, Room::Pvp] {
        said(&mut w, "Ann", room, "hello");
    }
    assert_eq!(lines_out(&mut w), []);

    line_in(&mut w, GLOBAL_ROOM, "Bob@coldeve", "anyone?", None);
    assert_eq!(w.fed.take_heard(), []);

    assert!(
        w.operator(&[])[0]
            .ends_with("The Global room is carried in no room. Rooms mirrored into Discord: none."),
        "{:?}",
        w.operator(&[])
    );
}

/// A line a character on the network says in the room that carries the Global room goes to the hub
/// under its address; off the network, or in another room, it stays here.
/// Behaviour: server.network.global-room-line-out
#[test]
fn a_line_said_in_the_carrying_room_goes_to_the_hub_under_its_speakers_address() {
    let mut w = carrying(Some("general"));
    on(&mut w, "Ann");
    w.link(&[]);
    w.frames();

    let (id, cara) = (w.ids["Ann"], w.character("Cara"));
    w.fed.room_said(
        &mut w.service,
        w.t,
        id,
        "ann",
        Room::General,
        "Anyone near Arwic?",
    );
    let frames = w.frames();
    let [Message::RoomSay {
        id: said_id,
        room,
        from,
        text,
        via: None,
    }] = &frames[..]
    else {
        panic!("{frames:?}")
    };
    assert_eq!(room, GLOBAL_ROOM);
    assert_eq!(from.to_string(), "Ann@dawnsong", "as the world writes it");
    assert_eq!(text, "Anyone near Arwic?");
    w.hub(&Message::RoomResult {
        id: said_id.clone(),
        outcome: Outcome::Ok,
    });
    assert_eq!(w.service.lines, [], "the hub's taking it is no news");

    said(&mut w, "Ann", Room::Trade, "not the carrying room");
    w.fed.room_said(
        &mut w.service,
        w.t,
        cara,
        "Cara",
        Room::General,
        "off the network",
    );
    assert_eq!(lines_out(&mut w), []);

    // The room is the hub's to move, and the line follows.
    told(&mut w, Some("roleplay"), &[]);
    said(&mut w, "Ann", Room::General, "General again");
    assert_eq!(lines_out(&mut w), []);
    said(&mut w, "Ann", Room::Pvp, "PvP now");
    assert_eq!(lines_out(&mut w).len(), 1);
}

/// A line waits for the link as a tell does, and goes when it comes; one that goes down with the
/// line in hand sends it again.
/// Behaviour: server.network.global-room-line-out-after-the-link
#[test]
fn a_line_waits_for_the_link_and_is_sent_again_when_the_link_went() {
    let mut w = carrying(Some("general"));
    on(&mut w, "Ann");
    said(&mut w, "Ann", Room::General, "before the link");
    assert_eq!(lines_out(&mut w).len(), 0);
    w.fed.on_link_up();
    w.frames();
    let frames = w.link(&[]);
    let rooms: Vec<&Message> = frames
        .iter()
        .filter(|m| matches!(m, Message::RoomSay { .. }))
        .collect();
    assert_eq!(rooms.len(), 1, "{frames:?}");
    assert_eq!(joins(&frames), [(GLOBAL_ROOM.to_owned(), true)]);
    assert!(
        matches!(frames[0], Message::RoomJoin { .. }),
        "the room is joined first: {frames:?}"
    );
    w.fed.on_link_down(&mut w.service, w.t);
    let again = w.link(&[]);
    assert_eq!(
        again
            .iter()
            .filter(|m| matches!(m, Message::RoomSay { .. }))
            .count(),
        1,
        "{again:?}"
    );
}

/// A speaker past its own pace is told so and the line stays here, and one the hub refuses for the
/// world's share of the room is told that. The pace is the hub's, on the world's own clock.
/// Behaviour: server.network.global-room-line-out-refused
#[test]
fn a_line_past_its_pace_or_held_back_by_the_hub_tells_its_speaker() {
    let mut limits = Limits::DEFAULT;
    limits.room_lines_per_character = Rate {
        burst: 2.0,
        per_sec: 1.0,
    };
    let mut w = carrying(Some("general"));
    let ann = on(&mut w, "Ann");
    w.link_with(&[], limits);

    said(&mut w, "Ann", Room::General, "one");
    said(&mut w, "Ann", Room::General, "two");
    let sent = lines_out(&mut w);
    assert_eq!(sent.len(), 2);
    assert_eq!(w.service.lines, []);
    said(&mut w, "Ann", Room::General, "three");
    assert_eq!(lines_out(&mut w), [], "past its pace");
    assert_eq!(
        w.service.lines,
        [(
            ann,
            "Your line did not reach the other worlds: you are sending too quickly.".to_owned()
        )]
    );
    w.service.lines.clear();

    w.advance(1.5);
    said(&mut w, "Ann", Room::General, "later");
    assert_eq!(lines_out(&mut w).len(), 1, "the pace fills again");

    w.hub(&Message::RoomResult {
        id: sent[0].0.clone(),
        outcome: Outcome::RateLimited,
    });
    assert_eq!(
        w.service.lines,
        [(
            ann,
            "Your line did not reach the other worlds: the Dereth Network is holding back this world's lines for now."
                .to_owned()
        )]
    );
}

/// A line from another world is kept for the players in the room under its speaker's address, its
/// text and name made safe and its name marked with where it was said. A line of this world's own,
/// one from what the operator blocks, one with nothing left to say, one in another room, and a line
/// in one of the world's own rooms never reach anyone.
/// Behaviour: server.network.global-room-line-in
#[test]
fn a_line_from_another_world_is_kept_for_the_room_under_its_speakers_address() {
    let mut w = carrying(Some("general"));
    w.link(&[]);
    w.frames();

    line_in(&mut w, GLOBAL_ROOM, "Bob@coldeve", "Ballroom at 8!", None);
    assert_eq!(
        w.fed.take_heard(),
        [Heard {
            room: Room::General,
            name: "Bob@coldeve".into(),
            text: "Ballroom at 8!".into(),
            speaker: Speaker::Remote(addr("Bob@coldeve")),
        }]
    );

    line_in(
        &mut w,
        GLOBAL_ROOM,
        "Bob@coldeve",
        "evening",
        Some("discord"),
    );
    line_in(
        &mut w,
        GLOBAL_ROOM,
        "Bob@coldeve",
        "<Tell:IID:1:Bob>x<\\Tell>",
        None,
    );
    let heard = w.fed.take_heard();
    assert_eq!(heard[0].name, "Bob@coldeve (Discord)");
    assert_eq!(heard[0].speaker, Speaker::Remote(addr("Bob@coldeve")));
    assert_eq!(heard[1].text, "x", "no link markup");

    line_in(&mut w, GLOBAL_ROOM, "Ann@dawnsong", "our own", None);
    line_in(&mut w, GLOBAL_ROOM, "Bob@coldeve", "\r\n", None);
    line_in(&mut w, "elsewhere", "Bob@coldeve", "another room", None);
    line_in(&mut w, "general", "Bob@coldeve", "an own room", None);
    w.operator(&["block", "frost"]);
    line_in(&mut w, GLOBAL_ROOM, "Mal@frost", "blocked", None);
    assert_eq!(w.fed.take_heard(), []);

    // The hub moves the room: the line follows it.
    told(&mut w, Some("lfg"), &[]);
    line_in(&mut w, GLOBAL_ROOM, "Bob@coldeve", "where now", None);
    assert_eq!(w.fed.take_heard()[0].room, Room::Pvp);
}

/// Another world's lines are held to the room's pace by this world too, for a hub that does not
/// hold them, a world at a time, on the world's own clock.
/// Behaviour: server.network.global-room-line-in-paced
#[test]
fn another_worlds_lines_are_held_to_the_rooms_pace_by_this_world_too() {
    let mut limits = Limits::DEFAULT;
    limits.room_lines_per_world = Rate {
        burst: 3.0,
        per_sec: 1.0,
    };
    let mut w = carrying(Some("general"));
    w.link_with(&[], limits);
    for _ in 0..10 {
        line_in(&mut w, GLOBAL_ROOM, "Bob@coldeve", "again", None);
    }
    assert_eq!(w.fed.take_heard().len(), 3, "its burst");
    line_in(
        &mut w,
        GLOBAL_ROOM,
        "Cara@frost",
        "another world's own",
        None,
    );
    assert_eq!(w.fed.take_heard().len(), 1);
    w.advance(2.5);
    line_in(&mut w, GLOBAL_ROOM, "Bob@coldeve", "later", None);
    assert_eq!(w.fed.take_heard().len(), 1, "the pace fills again");
}

/// `dn admin global` says where the world carries the Global room and whence it knows; with a room
/// it asks the hub, whose answer goes to whoever asked; `off` and `none` carry it in no room; a word
/// that is no room is the command's help. A link that is down, a hub that keeps no room, and a link
/// that goes before the hub answers each say so, and nothing else changes.
/// Behaviour: server.network.global-room-admin-word
#[test]
fn dn_admin_global_says_where_the_room_is_and_asks_the_hub_to_move_it() {
    let mut w = carrying(Some("general"));
    let ann = on(&mut w, "Ann");
    assert_eq!(
        w.operator(&["admin", "global"]),
        ["This world carries the Global room in General, as federation.global_room says, until the Dereth Network's hub keeps a room. Say @dn admin global <general|trade|lfg|roleplay|off> to move it."]
    );
    assert_eq!(
        w.operator(&["admin", "global", "trade"]),
        ["The Dereth Network's hub can't be reached now. Try again once the link is up."]
    );
    w.link(&[]);
    w.frames();
    assert_eq!(
        w.operator(&["admin", "global", "trade"]),
        ["This network's hub does not keep where a world carries the Global room: this world carries it as federation.global_room says."]
    );
    told(&mut w, Some("general"), &[]);

    // Said by an admin in game: the hub's answer comes to its character.
    let said = |w: &mut World, words: &[&str]| {
        w.fed
            .operator_said(&mut w.store, &mut w.service, ann, words)
    };
    assert_eq!(
        said(&mut w, &["global", "Trade"]),
        ["Asking the Dereth Network's hub to carry this world's Global room in Trade..."]
    );
    let frames = raw(&mut w);
    let Ok(RoomsMessage::Global {
        id,
        room,
        seed: false,
    }) = RoomsMessage::from_json(&frames[0])
    else {
        panic!("{frames:?}")
    };
    assert_eq!(room, "trade");
    assert_eq!(
        w.fed.global_room(),
        Some(Room::General),
        "until the hub says"
    );
    w.frame(
        &RoomsMessage::Setup {
            id: Some(id),
            global: Some("trade".into()),
            mirrored: Vec::new(),
        }
        .to_json(),
    );
    assert_eq!(w.fed.global_room(), Some(Room::Trade));
    assert_eq!(
        w.service.lines,
        [(
            ann,
            "The Dereth Network's hub carries this world's Global room in Trade now.".to_owned()
        )]
    );
    w.service.lines.clear();
    assert_eq!(
        said(&mut w, &["global"]),
        ["This world carries the Global room in Trade, as the Dereth Network's hub keeps it. Say @dn admin global <general|trade|lfg|roleplay|off> to move it."]
    );

    // Off and a room this client has under another name.
    for (word, sent, shown) in [
        ("off", "off", "no room"),
        ("NONE", "off", "no room"),
        ("roleplay", "roleplay", "PvP"),
        ("lfg", "lfg", "PvP"),
        ("general", "general", "General"),
    ] {
        assert_eq!(
            said(&mut w, &["global", word]),
            [format!(
                "Asking the Dereth Network's hub to carry this world's Global room in {shown}..."
            )],
            "{word}"
        );
        let frames = raw(&mut w);
        assert!(
            matches!(RoomsMessage::from_json(&frames[0]), Ok(RoomsMessage::Global { room, .. }) if room == sent),
            "{word}: {frames:?}"
        );
    }

    // A word that is no room is the command's line of the help, where it was typed.
    let usage = IN_GAME_HELP
        .iter()
        .find(|l| l.starts_with("@dn admin global <"))
        .expect("the line");
    for words in [&["global", "pvp"][..], &["global", "off", "now"][..]] {
        assert_eq!(said(&mut w, words), [*usage]);
    }
    let usage = HELP
        .iter()
        .find(|l| l.starts_with("dn admin global <"))
        .expect("the line");
    assert_eq!(w.operator(&["admin", "global", "pvp"]), [*usage]);
    assert_eq!(raw(&mut w), Vec::<String>::new(), "nothing asked");

    // A link that goes before the hub answers: whoever asked is told, and the room stays.
    w.fed.on_link_down(&mut w.service, w.t);
    w.link(&[]);
    told(&mut w, Some("trade"), &[]);
    said(&mut w, &["global", "general"]);
    w.service.lines.clear();
    w.fed.on_link_down(&mut w.service, w.t);
    assert_eq!(
        w.service.lines,
        [(
            ann,
            "The link to the Dereth Network's hub went before it answered. Say @dn admin global again once it is back."
                .to_owned()
        )]
    );
    assert_eq!(w.fed.global_room(), Some(Room::Trade));
}

/// A mirror set on either word for the room both name (the hub's page shows them as one row) is
/// joined under that word and carries that room's lines; set on both, it is joined once, under the
/// first.
/// Behaviour: server.network.own-rooms-mirrored
#[test]
fn a_mirror_on_either_word_for_one_room_carries_that_room() {
    for word in ["lfg", "roleplay"] {
        let mut w = carrying(Some("general"));
        on(&mut w, "Ann");
        w.link(&[]);
        told(&mut w, Some("general"), &[word]);
        assert_eq!(joins(&w.frames()), [(word.to_owned(), true)], "{word}");
        said(&mut w, "Ann", Room::Pvp, "one");
        assert_eq!(lines_out(&mut w)[0].1, word);
    }
    let mut w = carrying(Some("general"));
    on(&mut w, "Ann");
    w.link(&[]);
    told(&mut w, Some("general"), &["lfg", "roleplay"]);
    assert_eq!(joins(&w.frames()), [("lfg".to_owned(), true)]);
    said(&mut w, "Ann", Room::Pvp, "one");
    let sent = lines_out(&mut w);
    assert_eq!((sent.len(), sent[0].1.as_str()), (1, "lfg"));
}

/// The world's own rooms the hub mirrors are joined by their names, and left when they stop; a line
/// there goes to the hub under the room's name, and its speaker is told of Discord. A room the hub
/// answers it does not mirror keeps its lines here for a minute. The room carrying the Global room
/// is never one of them, and two names for one room here go under the first.
/// Behaviour: server.network.own-rooms-mirrored
#[test]
fn the_worlds_own_rooms_the_hub_mirrors_are_joined_and_their_lines_go_to_discord() {
    let mut limits = Limits::DEFAULT;
    limits.room_lines_per_character = Rate {
        burst: 1.0,
        per_sec: 0.0001,
    };
    let mut w = carrying(Some("roleplay"));
    let ann = on(&mut w, "Ann");
    w.link_with(&[], limits);
    told(&mut w, Some("roleplay"), &["general"]);
    assert_eq!(joins(&w.frames()), [("general".to_owned(), true)]);

    said(&mut w, "Ann", Room::General, "one");
    let sent = lines_out(&mut w);
    assert_eq!(
        (sent[0].1.as_str(), sent[0].2.as_deref()),
        ("general", None)
    );
    said(&mut w, "Ann", Room::General, "past its pace");
    assert_eq!(
        w.service.lines,
        [(
            ann,
            "Your line did not reach Discord: you are sending too quickly.".to_owned()
        )]
    );
    assert_eq!(
        w.operator(&[])[0].rsplit_once(". The Global").map(|p| p.1),
        Some(" room is carried in PvP. Rooms mirrored into Discord: General.")
    );

    // The hub answers it does not mirror it: nothing to the speaker, and the room rests a minute.
    let mut w = carrying(Some("roleplay"));
    on(&mut w, "Ann");
    w.link(&[]);
    told(&mut w, Some("roleplay"), &["general"]);
    w.frames();
    said(&mut w, "Ann", Room::General, "one");
    let sent = lines_out(&mut w);
    w.hub(&Message::RoomResult {
        id: sent[0].0.clone(),
        outcome: Outcome::NotMember,
    });
    assert_eq!(w.service.lines, []);
    said(&mut w, "Ann", Room::General, "resting");
    assert_eq!(lines_out(&mut w), []);
    w.advance(UNCARRIED_SECS + 1.0);
    said(&mut w, "Ann", Room::General, "again");
    let again = lines_out(&mut w);
    assert_eq!(again.len(), 1, "the next line after the minute");
    w.hub(&Message::RoomResult {
        id: again[0].0.clone(),
        outcome: Outcome::Ok,
    });

    // A new welcome ends the wait at once.
    said(&mut w, "Ann", Room::General, "one");
    let sent = lines_out(&mut w);
    w.hub(&Message::RoomResult {
        id: sent[0].0.clone(),
        outcome: Outcome::NotMember,
    });
    said(&mut w, "Ann", Room::General, "resting");
    assert_eq!(lines_out(&mut w), []);
    w.fed.on_link_down(&mut w.service, w.t);
    w.link(&[]);
    told(&mut w, Some("roleplay"), &["general"]);
    w.frames();
    said(&mut w, "Ann", Room::General, "this hub may mirror it");
    assert_eq!(lines_out(&mut w).len(), 1);

    // Stopping and starting joins and leaves, and a new welcome joins the rooms the hub last said.
    told(&mut w, Some("roleplay"), &[]);
    assert_eq!(joins(&w.frames()), [("general".to_owned(), false)]);
    told(&mut w, Some("roleplay"), &["general"]);
    assert_eq!(joins(&w.frames()), [("general".to_owned(), true)]);
    w.fed.on_link_down(&mut w.service, w.t);
    assert_eq!(
        joins(&w.link(&[])),
        [(GLOBAL_ROOM.to_owned(), true), ("general".to_owned(), true)]
    );

    // The carrying room is never its own room, and `lfg` and `roleplay` are one room here.
    let names =
        |w: &World| -> Vec<String> { w.fed.mirrored_rooms().into_iter().map(|(n, _)| n).collect() };
    told(&mut w, Some("general"), &["lfg", "roleplay"]);
    assert_eq!(names(&w), ["lfg"]);
    told(&mut w, Some("roleplay"), &["lfg", "general"]);
    assert_eq!(
        names(&w),
        ["general"],
        "lfg is the room carrying the Global room"
    );
}

/// The hub's asking.
fn ask(room: &str, who: &str) -> SayAsk {
    SayAsk {
        id: "h12".into(),
        room: room.into(),
        who: addr(who),
        text: "evening all".into(),
        via: "discord".into(),
    }
}

/// The answers owed to the hub now, read.
fn answers(w: &mut World) -> Vec<(SayOutcome, Option<String>)> {
    raw(w)
        .iter()
        .filter_map(|f| match SayMessage::from_json(f) {
            Ok(SayMessage::Result {
                outcome, reason, ..
            }) => Some((outcome, reason)),
            _ => None,
        })
        .collect()
}

/// A world on the network and linked, with Ann on it, who asked for her link, the room carried.
fn with_ann() -> (World, Character) {
    let mut w = carrying(Some("general"));
    let id = on(&mut w, "Ann");
    w.link(&[]);
    w.frames();
    w.fed.link_person(&mut w.store, w.t, id, "Ann");
    w.frames();
    let ann = Character {
        id,
        name: "Ann".into(),
        banned: false,
        gagged: false,
        too_new: false,
    };
    (w, ann)
}

/// A `say` waits for the host to look its character up; the character's line reaches the room's
/// players marked with where it was said, and goes on to the hub with the same mark, and the hub is
/// answered `said`. The character is told nothing: its player said it.
/// Behaviour: server.network.say-said-from-discord
#[test]
fn a_line_the_hub_asks_for_is_said_as_the_character_marked_with_where_it_was_said() {
    let (mut w, ann) = with_ann();
    w.frame(
        &SayMessage::Ask {
            id: "h12".into(),
            room: GLOBAL_ROOM.into(),
            who: addr("Ann@dawnsong"),
            text: "evening all".into(),
            via: "discord".into(),
        }
        .to_json(),
    );
    let asks = w.fed.take_says();
    assert_eq!(asks, [ask(GLOBAL_ROOM, "Ann@dawnsong")]);
    w.fed.answer_say(w.t, &asks[0], Some(ann));
    assert_eq!(
        w.fed.take_heard(),
        [Heard {
            room: Room::General,
            name: "Ann (Discord)".into(),
            text: "evening all".into(),
            speaker: Speaker::Own("Ann".into()),
        }]
    );
    let frames = raw(&mut w);
    assert!(
        frames
            .iter()
            .any(|f| matches!(Message::from_json(f), Ok(Message::RoomSay { room, via: Some(via), .. }) if room == GLOBAL_ROOM && via == "discord")),
        "{frames:?}"
    );
    assert!(
        frames.iter().any(|f| matches!(
            SayMessage::from_json(f),
            Ok(SayMessage::Result {
                outcome: SayOutcome::Said,
                reason: None,
                ..
            })
        )),
        "{frames:?}"
    );
    assert_eq!(w.service.lines, []);
}

/// A `say` the world's rules refuse is answered with the reason, and nothing is shown or sent.
/// Behaviour: server.network.say-refused
#[test]
fn a_line_the_hub_asks_for_is_refused_for_each_reason_and_nothing_is_shown() {
    let refused = |w: &mut World, ask: &SayAsk, c: Option<Character>, why: &str| {
        w.fed.answer_say(w.t, ask, c);
        assert_eq!(w.fed.take_heard(), [], "{why}");
        assert_eq!(
            answers(w),
            [(SayOutcome::Refused, Some(why.to_owned()))],
            "{why}"
        );
    };
    let global = ask(GLOBAL_ROOM, "Ann@dawnsong");

    let (mut w, ann) = with_ann();
    refused(&mut w, &global, None, "no_character");
    refused(
        &mut w,
        &ask(GLOBAL_ROOM, "Ann@coldeve"),
        Some(ann.clone()),
        "no_character",
    );
    let banned = Character {
        banned: true,
        ..ann.clone()
    };
    refused(&mut w, &global, Some(banned), "banned");
    let gagged = Character {
        gagged: true,
        ..ann.clone()
    };
    refused(&mut w, &global, Some(gagged), "gagged");
    let young = Character {
        too_new: true,
        ..ann.clone()
    };
    refused(&mut w, &global, Some(young), "too_new");
    refused(
        &mut w,
        &ask("general", "Ann@dawnsong"),
        Some(ann.clone()),
        "room_off",
    );
    w.set_on("Ann", false);
    w.frames();
    refused(&mut w, &global, Some(ann.clone()), "not_on_network");

    // On the network, and never asked for its link.
    let mut w = carrying(Some("general"));
    let id = on(&mut w, "Ann");
    w.link(&[]);
    w.frames();
    refused(
        &mut w,
        &global,
        Some(Character { id, ..ann.clone() }),
        "not_linked",
    );

    // Empty after the line rules: refused with no word for it.
    let (mut w, ann) = with_ann();
    let empty = SayAsk {
        text: " \r\n".into(),
        ..global.clone()
    };
    w.fed.answer_say(w.t, &empty, Some(ann.clone()));
    assert_eq!(answers(&mut w), [(SayOutcome::Refused, None)]);

    // Its pace of room lines, spent by lines said: until the first refusal, at least its burst.
    let mut said = 0;
    loop {
        w.fed.answer_say(w.t, &global, Some(ann.clone()));
        let heard = w.fed.take_heard().len();
        if heard == 0 {
            break;
        }
        said += heard;
        assert!(said < 1_000, "a pace");
        // Each answer is owed in turn, and the world's own pace of frames is no part of this.
        w.advance(0.0);
    }
    assert!(said >= 3, "its burst: {said}");

    // No carried room at all.
    let (mut w, ann) = with_ann();
    told(&mut w, Some("off"), &[]);
    w.frames();
    refused(&mut w, &global, Some(ann), "room_off");
}

/// A character younger than the operator asks says nothing to the network from a room, and is told
/// why; it is no longer, and its line goes.
/// Behaviour: server.network.too-new-sends-nothing
#[test]
fn a_character_too_new_is_told_its_line_did_not_reach_the_network() {
    let (mut w, ann) = with_ann();
    w.fed
        .set_too_new(std::collections::HashSet::from(["ann".to_owned()]));
    said(&mut w, "Ann", Room::General, "hello, other worlds");
    assert_eq!(lines_out(&mut w), []);
    assert_eq!(
        w.service.lines,
        [(
            ann.id,
            dereth_network_protocol::rooms::too_new_line(GLOBAL_ROOM)
        )]
    );
    w.fed.set_too_new(std::collections::HashSet::new());
    said(&mut w, "Ann", Room::General, "hello, other worlds");
    assert_eq!(lines_out(&mut w).len(), 1);
}
