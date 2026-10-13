//! Divergence: V449
//! Lines said from Discord: the hub asks a world to say a line as one of its characters, which
//! the character's person said in a Discord mirror of a room. The world says it as the character,
//! online or not, only under the rules a line the character says itself meets; it shows it to the
//! room's local listeners under the character's own id, its name marked `Ann (Discord)`, and sends
//! it on as a `room.say` with the same mark, which the other worlds show marked
//! `Ann@aa (Discord)`; and it answers `said`. Otherwise it answers `refused` with the reason, and
//! shows and sends nothing.
//! Fixture: virtual-time worlds through the hub's routing (`super::Net`); the hub's say is handed
//! to the world as a frame, as the hub writes it, and the world's answer kept.

use dereth_network_protocol::say::{reason, SayMessage, SayOutcome, VIA_DISCORD};
use dereth_network_protocol::standin::is_stand_in;
use dereth_network_protocol::{Address, Message};
use dereth_protocol::turbine::{decode_incoming, IncomingPayload};
use empyrean_entity::enums::{CharacterOption, HeritageGroup, PropertyBool, PropertyInt};
use empyrean_entity::ObjectGuid;
use empyrean_world::entity::i_player::{self, IPlayer};

use super::discord_rooms::general;
use super::persons::{ask_code, log_off};
use super::{seed, Net, Seed, TURBINE_CHAT};

const ANN: u32 = 0x5000_0001;
const CY: u32 = 0x5000_0002;

/// The TurbineChat rooms the tests hear in.
const GENERAL: u32 = 2;
const ROLEPLAY: u32 = 5;

const GENERAL_AND_ROLEPLAY: &[CharacterOption] = &[
    CharacterOption::ListenToGeneralChat,
    CharacterOption::ListenToRoleplayChat,
];

fn ann() -> Seed {
    let mut s = seed("ann", ANN, "Ann");
    s.listens = GENERAL_AND_ROLEPLAY;
    s
}
fn cy() -> Seed {
    let mut s = seed("cy", CY, "Cy");
    s.listens = GENERAL_AND_ROLEPLAY;
    s
}
fn dee() -> Seed {
    seed("dee", 0x5000_0003, "Dee")
}
fn bob() -> Seed {
    seed("bob", 0x5000_0001, "Bob")
}

fn addr(s: &str) -> Address {
    Address::parse(s).expect("an address")
}

/// Ann, Cy and Dee on aa, Bob on bb; Ann, Cy and Bob on the network, Dee not; Ann asked for her
/// link code, Cy never did. The hub mirrors aa's General room into Discord.
fn worlds() -> Net {
    let mut net = Net::new(&[("aa", &[ann(), cy(), dee()]), ("bb", &[bob()])]);
    net.hub.set_bridged([general()].into());
    net.link("aa");
    net.link("bb");
    for (slug, s) in [("aa", ann()), ("aa", cy()), ("aa", dee()), ("bb", bob())] {
        net.enter(slug, s);
    }
    for (slug, who) in [("aa", "Ann"), ("aa", "Cy"), ("bb", "Bob")] {
        net.say(slug, who, "@dn on");
    }
    let _ = ask_code(&mut net, "aa", "Ann");
    let _ = net.hub.take_bridged();
    net
}

fn ask(id: &str, room: &str, who: &str, text: &str) -> SayMessage {
    SayMessage::Ask {
        id: id.into(),
        room: room.into(),
        who: addr(who),
        text: text.into(),
        via: VIA_DISCORD.into(),
    }
}

fn said(id: &str) -> SayMessage {
    SayMessage::Result {
        id: id.into(),
        outcome: SayOutcome::Said,
        reason: None,
    }
}

fn refused(id: &str, why: &str) -> SayMessage {
    SayMessage::Result {
        id: id.into(),
        outcome: SayOutcome::Refused,
        reason: Some(why.into()),
    }
}

/// The room events `who` was shown since `mark`: the room, the speaker's name, the text and the
/// speaker's id.
fn room_events(
    net: &mut Net,
    slug: &str,
    who: &str,
    mark: usize,
) -> Vec<(u32, String, String, u32)> {
    net.since(slug, who, mark)
        .iter()
        .filter(|g| g.kind == TURBINE_CHAT)
        .filter_map(|g| {
            match decode_incoming(&g.blob[4..])
                .expect("a TurbineChat blob")
                .payload
            {
                IncomingPayload::RoomEvent(e) => {
                    let sender = e
                        .extra
                        .get(0..4)
                        .map_or(0, |b| u32::from_le_bytes(b.try_into().expect("four bytes")));
                    Some((e.room, e.name, e.text, sender))
                }
                _ => None,
            }
        })
        .collect()
}

/// The room lines `slug` sent the hub since `mark`: the room, the speaker, the text and the mark.
fn room_says(net: &Net, slug: &str, mark: usize) -> Vec<(String, String, String, Option<String>)> {
    net.sent_since(slug, mark)
        .into_iter()
        .filter_map(|m| match m {
            Message::RoomSay {
                room,
                from,
                text,
                via,
                ..
            } => Some((room, from.to_string(), text, via)),
            _ => None,
        })
        .collect()
}

/// Divergence: V449
#[test]
fn a_line_said_from_discord_is_shown_marked_under_the_characters_own_id_and_sent_on_marked() {
    let mut net = worlds();
    let (mc, mb) = (net.mark("aa", "Cy"), net.mark("bb", "Bob"));
    let (wire, sent) = (net.wire.0.len(), net.sent.0.len());
    let answers = net.hub_asks_say("aa", &ask("h1", "global", "Ann@aa", "evening all"));
    net.advance(0.3);
    assert_eq!(answers, [said("h1")]);
    assert_eq!(
        room_events(&mut net, "aa", "Cy", mc),
        [(
            ROLEPLAY,
            "Ann (Discord)".to_owned(),
            "evening all".to_owned(),
            ANN
        )],
        "aa's listeners hear it in the room that carries the Global room, under Ann's own id"
    );
    assert_eq!(
        room_says(&net, "aa", sent),
        [(
            "global".to_owned(),
            "Ann@aa".to_owned(),
            "evening all".to_owned(),
            Some(VIA_DISCORD.to_owned())
        )],
        "it goes on to the hub as a line of the room, marked"
    );
    let order: Vec<String> = net
        .wire_since(wire)
        .into_iter()
        .filter(|l| l.starts_with("aa -> hub"))
        .collect();
    assert_eq!(
        order,
        ["aa -> hub: room.say", "aa -> hub: say.result"],
        "the line goes before the answer"
    );
    let heard = room_events(&mut net, "bb", "Bob", mb);
    assert!(
        heard.len() == 1
            && heard[0].0 == ROLEPLAY
            && heard[0].1 == "Ann@aa (Discord)"
            && heard[0].2 == "evening all"
            && is_stand_in(heard[0].3),
        "the other world shows it marked, under Ann's stand-in id: {heard:?}"
    );
}

/// Divergence: V449
#[test]
fn a_character_not_online_still_says_a_line_from_discord_and_is_told_nothing() {
    let mut net = worlds();
    log_off(&mut net, "aa", "Ann");
    let mc = net.mark("aa", "Cy");
    let sent = net.sent.0.len();
    let answers = net.hub_asks_say("aa", &ask("h2", "global", "ann@aa", "on my phone"));
    net.advance(0.3);
    assert_eq!(answers, [said("h2")], "the name as any case spells it");
    assert_eq!(
        room_events(&mut net, "aa", "Cy", mc),
        [(
            ROLEPLAY,
            "Ann (Discord)".to_owned(),
            "on my phone".to_owned(),
            ANN
        )]
    );
    assert_eq!(room_says(&net, "aa", sent).len(), 1);
}

/// Divergence: V449
#[test]
fn a_line_from_discord_in_one_of_the_worlds_own_rooms_goes_to_discord_alone_marked() {
    let mut net = worlds();
    let (mc, mb) = (net.mark("aa", "Cy"), net.mark("bb", "Bob"));
    let sent = net.sent.0.len();
    let answers = net.hub_asks_say("aa", &ask("h3", "general", "Ann@aa", "selling scarabs"));
    net.advance(0.3);
    assert_eq!(answers, [said("h3")]);
    assert_eq!(
        room_events(&mut net, "aa", "Cy", mc),
        [(
            GENERAL,
            "Ann (Discord)".to_owned(),
            "selling scarabs".to_owned(),
            ANN
        )]
    );
    assert_eq!(
        room_says(&net, "aa", sent),
        [(
            "general".to_owned(),
            "Ann@aa".to_owned(),
            "selling scarabs".to_owned(),
            Some(VIA_DISCORD.to_owned())
        )]
    );
    let mirrored: Vec<(String, Option<String>)> = net
        .hub
        .take_bridged()
        .into_iter()
        .map(|l| (l.from.to_string(), l.via))
        .collect();
    assert_eq!(
        mirrored,
        [("Ann@aa".to_owned(), Some(VIA_DISCORD.to_owned()))],
        "the hub mirrors it into Discord with its mark"
    );
    assert!(
        room_events(&mut net, "bb", "Bob", mb).is_empty(),
        "no other world is handed a line of aa's own room"
    );
}

/// Divergence: V449
#[test]
fn a_line_from_another_world_said_from_discord_is_shown_marked() {
    let mut net = worlds();
    let mc = net.mark("aa", "Cy");
    net.hub_hands(
        "aa",
        &Message::RoomLine {
            room: "global".into(),
            from: addr("Eve@bb"),
            text: "hello from afar".into(),
            via: Some(VIA_DISCORD.into()),
        },
    );
    net.hub_hands(
        "aa",
        &Message::RoomLine {
            room: "global".into(),
            from: addr("Eve@bb"),
            text: "and from the game".into(),
            via: None,
        },
    );
    net.advance(0.3);
    let heard = room_events(&mut net, "aa", "Cy", mc);
    assert_eq!(heard.len(), 2, "{heard:?}");
    assert_eq!(
        (heard[0].1.as_str(), heard[1].1.as_str()),
        ("Eve@bb (Discord)", "Eve@bb"),
        "the mark is where it was said"
    );
    assert_eq!(
        heard[0].3, heard[1].3,
        "both are under Eve's one stand-in id, never the marked name's"
    );
}

/// What the world showed and sent for a say the hub asked of it, and its answers.
fn asked(net: &mut Net, m: &SayMessage) -> (Vec<SayMessage>, usize, usize) {
    let mc = net.mark("aa", "Cy");
    let mb = net.mark("bb", "Bob");
    let sent = net.sent.0.len();
    let answers = net.hub_asks_say("aa", m);
    net.advance(0.3);
    let shown = room_events(net, "aa", "Cy", mc).len() + room_events(net, "bb", "Bob", mb).len();
    (answers, shown, room_says(net, "aa", sent).len())
}

/// Divergence: V449
#[test]
fn a_line_from_discord_is_refused_with_the_reason_and_nothing_is_shown_or_sent() {
    let mut net = worlds();
    for (id, who, why) in [
        ("n1", "Nobody@aa", reason::NO_CHARACTER),
        ("n2", "Bob@bb", reason::NO_CHARACTER),
        ("n3", "Dee@aa", reason::NOT_ON_NETWORK),
        ("n4", "Cy@aa", reason::NOT_LINKED),
    ] {
        assert_eq!(
            asked(&mut net, &ask(id, "global", who, "hi")),
            (vec![refused(id, why)], 0, 0),
            "{who}"
        );
    }
    // Ann is gagged.
    let g = IPlayer::Online(ObjectGuid::new(ANN));
    i_player::set_property(
        &mut net.world("aa").ts.world,
        g,
        PropertyBool::IsGagged,
        true,
    );
    assert_eq!(
        asked(&mut net, &ask("g1", "global", "Ann@aa", "hi")),
        (vec![refused("g1", reason::GAGGED)], 0, 0)
    );
    i_player::remove_property(&mut net.world("aa").ts.world, g, PropertyBool::IsGagged);
    // Ann is an Olthoi player, whom the rooms open to every player keep out.
    olthoi(&mut net, g, true);
    assert_eq!(
        asked(&mut net, &ask("o1", "global", "Ann@aa", "hi")),
        (vec![refused("o1", reason::NOT_ALLOWED)], 0, 0)
    );
    olthoi(&mut net, g, false);
    // The operator asks a speaker in the world's chat rooms to be level 50; Ann is level 10.
    i_player::set_property(&mut net.world("aa").ts.world, g, PropertyInt::Level, 10);
    let w = &net.world("aa").ts.world;
    assert!(empyrean_world::managers::property_manager::modify_long(
        w,
        "chat_requires_player_level",
        50
    ));
    assert_eq!(
        asked(&mut net, &ask("v1", "global", "Ann@aa", "hi")),
        (vec![refused("v1", reason::NOT_ALLOWED)], 0, 0)
    );
    let w = &net.world("aa").ts.world;
    assert!(empyrean_world::managers::property_manager::modify_long(
        w,
        "chat_requires_player_level",
        0
    ));
    // The operator asks for an hour's play before a character sends to the network.
    net.world("aa")
        .ts
        .world
        .federation
        .settings
        .min_character_age_secs = 3_600;
    assert_eq!(
        asked(&mut net, &ask("t1", "global", "Ann@aa", "hi")),
        (vec![refused("t1", reason::TOO_NEW)], 0, 0)
    );
    net.world("aa")
        .ts
        .world
        .federation
        .settings
        .min_character_age_secs = 0;
    // A room aa does not carry, and the Global room's room turned off.
    assert_eq!(
        asked(&mut net, &ask("r1", "trade", "Ann@aa", "hi")),
        (vec![refused("r1", reason::ROOM_OFF)], 0, 0)
    );
    let w = &net.world("aa").ts.world;
    assert!(empyrean_world::managers::property_manager::modify_bool(
        w,
        "chat_disable_roleplay",
        true
    ));
    assert_eq!(
        asked(&mut net, &ask("r2", "global", "Ann@aa", "hi")),
        (vec![refused("r2", reason::ROOM_OFF)], 0, 0)
    );
    let w = &net.world("aa").ts.world;
    assert!(empyrean_world::managers::property_manager::modify_bool(
        w,
        "chat_disable_roleplay",
        false
    ));
    assert_eq!(
        asked(&mut net, &ask("ok", "global", "Ann@aa", "hi")),
        (vec![said("ok")], 2, 1),
        "said once nothing holds it back"
    );
}

/// Makes `p` on aa an Olthoi player by its heritage, or no longer one.
fn olthoi(net: &mut Net, p: IPlayer, on: bool) {
    let w = &mut net.world("aa").ts.world;
    if on {
        i_player::set_property(w, p, PropertyInt::HeritageGroup, HeritageGroup::Olthoi.0);
    } else {
        i_player::remove_property(w, p, PropertyInt::HeritageGroup);
    }
}

/// Divergence: V449
#[test]
fn an_olthoi_players_line_from_discord_is_refused_online_or_not() {
    let mut net = worlds();
    log_off(&mut net, "aa", "Ann");
    olthoi(&mut net, IPlayer::Offline(ObjectGuid::new(ANN)), true);
    assert_eq!(
        asked(&mut net, &ask("o2", "general", "Ann@aa", "hi")),
        (vec![refused("o2", reason::NOT_ALLOWED)], 0, 0),
        "what the world keeps of her while she is away says so"
    );
}

/// Divergence: V449
#[test]
fn a_line_said_from_discord_is_written_to_the_rooms_chat_log_as_the_characters_marked_line() {
    let mut net = worlds();
    let w = &net.world("aa").ts.world;
    assert!(empyrean_world::managers::property_manager::modify_bool(
        w,
        "chat_log_roleplay",
        true
    ));
    let (answers, logged) =
        info_lines(|| net.hub_asks_say("aa", &ask("l1", "global", "Ann@aa", "evening all")));
    assert_eq!(answers, [said("l1")]);
    assert!(
        logged
            .iter()
            .any(|l| l.starts_with("[CHAT]") && l.ends_with("Ann (Discord) says, \"evening all\"")),
        "{logged:?}"
    );
}

thread_local! {
    /// The info lines logged on this thread, while [`info_lines`] runs.
    static LOGGED: std::cell::RefCell<Option<Vec<String>>> = const { std::cell::RefCell::new(None) };
}

struct Capture;

impl log::Log for Capture {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            LOGGED.with_borrow_mut(|l| {
                if let Some(l) = l {
                    l.push(record.args().to_string());
                }
            });
        }
    }

    fn flush(&self) {}
}

/// Runs `f` and returns its result with the info lines it logged on this thread.
fn info_lines<T>(f: impl FnOnce() -> T) -> (T, Vec<String>) {
    static INSTALL: std::sync::Once = std::sync::Once::new();
    INSTALL.call_once(|| {
        log::set_logger(&Capture).expect("no other logger in this test binary");
        log::set_max_level(log::LevelFilter::Info);
    });
    LOGGED.with_borrow_mut(|l| *l = Some(Vec::new()));
    let out = f();
    let lines = LOGGED.with_borrow_mut(Option::take).unwrap_or_default();
    (out, lines)
}

/// Divergence: V449
#[test]
fn a_line_from_discord_by_a_banned_accounts_character_is_refused() {
    let mut net = worlds();
    log_off(&mut net, "aa", "Ann");
    {
        let w = &net.world("aa").ts;
        let until = w.world.now.utc.add_days(3.0);
        let mut auth = w.auth();
        let mut account = auth.get_account_by_name("ann").expect("Ann's account");
        account.ban_expire_time = Some(until);
        auth.update_account(&account);
    }
    assert_eq!(
        asked(&mut net, &ask("b1", "global", "Ann@aa", "hi")),
        (vec![refused("b1", reason::BANNED)], 0, 0)
    );
}

/// Divergence: V449
#[test]
fn lines_from_discord_are_held_to_the_characters_pace_of_room_lines() {
    let mut net = worlds();
    let mut outcomes = Vec::new();
    for i in 0..4 {
        let id = format!("p{i}");
        outcomes.push(asked(&mut net, &ask(&id, "global", "Ann@aa", "again")));
    }
    assert_eq!(
        outcomes,
        [
            (vec![said("p0")], 2, 1),
            (vec![said("p1")], 2, 1),
            (vec![said("p2")], 2, 1),
            (vec![refused("p3", reason::RATE_LIMITED)], 0, 0),
        ],
        "a burst of three, as Ann's own lines in the room; the fourth is neither shown nor sent"
    );
}
