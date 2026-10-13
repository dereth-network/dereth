//! Divergence: V449
//! A world's rooms as its hub says: the hub keeps which of the world's rooms carries the Global
//! room, seeded once from the world's settings and moved by its operator on the site or in game
//! (`/dn admin global`); the world carries it there at once, keeps what it was last told across a
//! restart without the hub, and takes a name given with the mark a line said from Discord shows
//! beside its speaker as the character's own.
//! Fixture: virtual-time worlds through the hub's routing (`super::Net`), the hub's own registry
//! holding each world's room.

use dereth_network_protocol::RoomsMessage;
use dereth_protocol::comms::CommunicationHearDirectSpeech;
use dereth_protocol::turbine::{decode_incoming, IncomingPayload};
use empyrean_entity::enums::{AccessLevel, CharacterOption};
use empyrean_world::federation::{self, FederationSettings, NetworkStore, RoomSlot};

use super::discord_rooms::{general, post};
use super::{seed, Net, Seed, TELL, TURBINE_CHAT};

const TRADE: u32 = 3;
const ROLEPLAY: u32 = 5;

fn ann() -> Seed {
    let mut s = seed("ann", 0x5000_0001, "Ann");
    s.listens = &[
        CharacterOption::ListenToRoleplayChat,
        CharacterOption::ListenToTradeChat,
    ];
    s
}

fn bob() -> Seed {
    seed("bob", 0x5000_0001, "Bob")
}

fn cy() -> Seed {
    seed("cy", 0x5000_0002, "Cy")
}

fn boss() -> Seed {
    let mut s = seed("boss", 0x5000_0003, "Boss");
    s.access = AccessLevel::Admin;
    s
}

/// Ann, Cy and Boss on aa, Bob on bb; Ann and Bob on the network.
fn worlds() -> Net {
    let mut net = Net::new(&[("aa", &[ann(), cy(), boss()]), ("bb", &[bob()])]);
    net.link("aa");
    net.link("bb");
    for (slug, s) in [("aa", ann()), ("aa", cy()), ("aa", boss()), ("bb", bob())] {
        net.enter(slug, s);
    }
    net.say("aa", "Ann", "@dn on");
    net.say("bb", "Bob", "@dn on");
    net
}

/// Where the hub's registry carries `slug`'s Global room.
fn held(net: &Net, slug: &str) -> Option<String> {
    net.registry.1.global_room(slug).expect("read")
}

/// The room events `who` was shown since `mark`: the room, the speaker's name and the text.
fn room_events(net: &mut Net, slug: &str, who: &str, mark: usize) -> Vec<(u32, String, String)> {
    net.since(slug, who, mark)
        .iter()
        .filter(|g| g.kind == TURBINE_CHAT)
        .filter_map(|g| {
            match decode_incoming(&g.blob[4..])
                .expect("a TurbineChat blob")
                .payload
            {
                IncomingPayload::RoomEvent(e) => Some((e.room, e.name, e.text)),
                _ => None,
            }
        })
        .collect()
}

/// The `rooms.global` requests `slug` sent the hub: the room, and whether it was a seed.
fn asked(net: &Net, slug: &str) -> Vec<(String, bool)> {
    net.rooms_asked
        .iter()
        .filter(|(s, _)| s == slug)
        .filter_map(|(_, m)| match m {
            RoomsMessage::Global { room, seed, .. } => Some((room.clone(), *seed)),
            RoomsMessage::Setup { .. } => None,
        })
        .collect()
}

/// The operator moves `slug`'s Global room on the site, and the hub tells the world at once.
fn moved_on_the_site(net: &mut Net, slug: &str, room: &str) {
    net.registry
        .1
        .set_global_room(slug, room, false, "op_1")
        .expect("set");
    net.hub.rooms_changed();
    net.advance(1.0);
}

/// Divergence: V449
#[test]
fn a_world_seeds_its_global_room_once_and_the_hubs_room_holds_after() {
    let mut net = Net::new(&[("aa", &[ann()]), ("bb", &[bob()])]);
    {
        let s = &mut net.world("bb").ts.world.federation.settings;
        s.global_room = Some(RoomSlot::Trade);
        s.global_room_named = true;
    }
    net.link("aa");
    net.link("bb");
    assert_eq!(held(&net, "aa").as_deref(), Some("roleplay"), "the default");
    assert_eq!(
        held(&net, "bb").as_deref(),
        Some("trade"),
        "as bb's settings say"
    );
    assert_eq!(asked(&net, "aa"), [("roleplay".to_owned(), true)]);
    assert_eq!(asked(&net, "bb"), [("trade".to_owned(), true)]);
    let audit = net.registry.1.audit("aa").expect("the log");
    assert_eq!(
        (
            &audit[0]["kind"],
            &audit[0]["actor"],
            &audit[0]["details"]["seed"]
        ),
        (
            &serde_json::json!("global-room"),
            &serde_json::json!("world"),
            &serde_json::json!(true)
        )
    );

    // The hub holds a room now: the world sends none again, and its settings are not read.
    {
        let s = &mut net.world("aa").ts.world.federation.settings;
        s.global_room = Some(RoomSlot::General);
        s.global_room_named = true;
    }
    net.unlink("aa");
    net.link("aa");
    assert_eq!(
        asked(&net, "aa").len(),
        1,
        "no seed while the hub holds a room"
    );
    assert_eq!(held(&net, "aa").as_deref(), Some("roleplay"));
    assert_eq!(
        federation::global_slot(&net.world("aa").ts.world),
        Some(RoomSlot::Roleplay)
    );
}

/// Divergence: V449
#[test]
fn the_world_carries_the_global_room_where_the_hub_says_at_once() {
    let mut net = worlds();
    moved_on_the_site(&mut net, "aa", "trade");
    assert_eq!(
        federation::global_slot(&net.world("aa").ts.world),
        Some(RoomSlot::Trade)
    );

    // aa's Trade carries the Global room both ways; its Roleplay is its own again.
    let mb = net.mark("bb", "Bob");
    post(
        &mut net,
        "aa",
        "Ann",
        0x5000_0001,
        TRADE,
        "trade carries it",
    );
    post(
        &mut net,
        "aa",
        "Ann",
        0x5000_0001,
        ROLEPLAY,
        "roleplay stays here",
    );
    assert_eq!(
        room_events(&mut net, "bb", "Bob", mb),
        [(ROLEPLAY, "Ann@aa".to_owned(), "trade carries it".to_owned())]
    );
    let ma = net.mark("aa", "Ann");
    post(&mut net, "bb", "Bob", 0x5000_0001, ROLEPLAY, "hello aa");
    assert_eq!(
        room_events(&mut net, "aa", "Ann", ma),
        [(TRADE, "Bob@bb".to_owned(), "hello aa".to_owned())]
    );

    // Carried nowhere: aa leaves the Global room, and nothing goes either way.
    let sent = net.sent.0.len();
    moved_on_the_site(&mut net, "aa", "off");
    assert!(
        net.sent_since("aa", sent).iter().any(|m| matches!(
            m,
            dereth_network_protocol::Message::RoomJoin { room, on: false } if room == "global"
        )),
        "aa leaves the Global room"
    );
    let (ma, mb) = (net.mark("aa", "Ann"), net.mark("bb", "Bob"));
    post(&mut net, "aa", "Ann", 0x5000_0001, TRADE, "only here");
    post(
        &mut net,
        "bb",
        "Bob",
        0x5000_0001,
        ROLEPLAY,
        "anyone on aa?",
    );
    assert!(room_events(&mut net, "bb", "Bob", mb)
        .iter()
        .all(|(_, from, _)| from != "Ann@aa"));
    assert!(room_events(&mut net, "aa", "Ann", ma)
        .iter()
        .all(|(_, from, _)| from != "Bob@bb"));
    let ma = net.mark("aa", "Ann");
    net.say("aa", "Ann", "@dng anyone?");
    assert_eq!(
        Net::lines(&net.since("aa", "Ann", ma)),
        ["This world does not carry the Global room."]
    );
}

/// Divergence: V449
#[test]
fn the_operators_dn_admin_global_asks_the_hub_and_says_where_the_room_is_now() {
    let mut net = worlds();
    let mb = net.mark("aa", "Boss");
    net.say("aa", "Boss", "@dn admin global");
    assert_eq!(
        Net::lines(&net.since("aa", "Boss", mb)),
        ["This world carries the Global room in Roleplay, as the Dereth Network's hub keeps it. Type @dn admin global <general|trade|lfg|roleplay|off> to move it."]
    );
    let mb = net.mark("aa", "Boss");
    net.say("aa", "Boss", "@dn admin global lfg");
    assert_eq!(
        Net::lines(&net.since("aa", "Boss", mb)),
        [
            "Asking the Dereth Network's hub to carry this world's Global room in LFG...",
            "The Dereth Network's hub carries this world's Global room in LFG now.",
        ]
    );
    assert_eq!(held(&net, "aa").as_deref(), Some("lfg"));
    assert_eq!(asked(&net, "aa").last(), Some(&("lfg".to_owned(), false)));
    assert_eq!(
        net.registry.1.audit("aa").expect("the log")[0]["actor"],
        "world",
        "the world set it"
    );
    assert_eq!(
        federation::global_slot(&net.world("aa").ts.world),
        Some(RoomSlot::Lfg)
    );

    let mb = net.mark("aa", "Boss");
    net.say("aa", "Boss", "@dn admin global lobby");
    assert_eq!(
        Net::lines(&net.since("aa", "Boss", mb)),
        ["@dn admin global <general|trade|lfg|roleplay|off> - carry the Global room in another of this world's rooms"]
    );
    let ma = net.mark("aa", "Ann");
    net.say("aa", "Ann", "@dn admin global trade");
    assert!(Net::lines(&net.since("aa", "Ann", ma))[0].starts_with("@dn - "));
    assert_eq!(
        held(&net, "aa").as_deref(),
        Some("lfg"),
        "a player moves nothing"
    );

    // Without the hub there is nobody to ask.
    net.unlink("aa");
    let mb = net.mark("aa", "Boss");
    net.say("aa", "Boss", "@dn admin global trade");
    assert_eq!(
        Net::lines(&net.since("aa", "Boss", mb)),
        ["The Dereth Network's hub can't be reached now. Try again once the link is up."]
    );
    // A hub that keeps no room for a world never tells it its rooms.
    net.link("aa");
    let w = &mut net.world("aa").ts.world;
    federation::on_link_frame(
        w,
        &dereth_network_protocol::Message::Welcome {
            proto: dereth_network_protocol::wire::PROTOCOL,
            world: "aa".into(),
            hub: "an older hub".into(),
            worlds: Vec::new(),
            rooms: vec!["global".into()],
            limits: dereth_network_protocol::limits::Limits::DEFAULT,
        }
        .to_json(),
    );
    let out = federation::commands::admin(w, &["global".into(), "trade".into()]);
    assert_eq!(
        out,
        "This network's hub does not keep where a world carries the Global room: this world carries it as federation.global_room says."
    );
}

/// Divergence: V449
#[test]
fn a_world_keeps_its_rooms_across_a_restart_and_starts_right_without_the_hub() {
    let mut net = worlds();
    net.hub.set_bridged([general()].into());
    moved_on_the_site(&mut net, "aa", "trade");
    net.unlink("aa");

    // The world stops and starts again from what it saved, with no hub to ask.
    let w = &mut net.world("aa").ts.world;
    let saved = serde_json::to_string(&w.federation.store).expect("the store saves");
    let store: NetworkStore = serde_json::from_str(&saved).expect("the store loads");
    w.federation = federation::FederationState::default();
    federation::configure(
        w,
        FederationSettings {
            enabled: true,
            slug: "aa".into(),
            global_room: Some(RoomSlot::Roleplay),
            global_room_named: true,
            min_character_age_secs: 0,
        },
        store,
    );
    assert_eq!(federation::global_slot(w), Some(RoomSlot::Trade));
    assert_eq!(federation::mirrored_slots(w), [RoomSlot::General]);
    let ma = net.mark("aa", "Ann");
    net.say("aa", "Ann", "@dng still here");
    assert_eq!(
        room_events(&mut net, "aa", "Ann", ma),
        [(TRADE, "Ann".to_owned(), "still here".to_owned())],
        "said in the room the hub last named"
    );
    net.link("aa");
    assert_eq!(asked(&net, "aa").len(), 1, "the first link's seed alone");
    assert_eq!(held(&net, "aa").as_deref(), Some("trade"));
}

/// The tells `who` was shown since `mark`: the sender's name and the text.
fn tells(net: &mut Net, slug: &str, who: &str, mark: usize) -> Vec<(String, String)> {
    net.since(slug, who, mark)
        .iter()
        .filter(|g| g.kind == TELL)
        .map(|g| g.decode::<CommunicationHearDirectSpeech>())
        .map(|t| (t.sender_name, t.message))
        .collect()
}

/// Divergence: V449
#[test]
fn a_name_given_with_the_discord_mark_names_the_character_without_it() {
    let mut net = worlds();
    let mc = net.mark("aa", "Cy");
    net.tell("aa", "Ann", "Cy (Discord)", "hi Cy");
    assert_eq!(
        tells(&mut net, "aa", "Cy", mc),
        [("Ann".to_owned(), "hi Cy".to_owned())],
        "a local name clicked in a marked line"
    );
    let mb = net.mark("bb", "Bob");
    net.tell("aa", "Ann", "Bob@bb (discord)", "hi Bob");
    assert_eq!(
        tells(&mut net, "bb", "Bob", mb),
        [("Ann@aa".to_owned(), "hi Bob".to_owned())],
        "an address goes as any network tell"
    );
    super::squelch::squelch(&mut net, "bb", "Bob", "Ann@aa (Discord)", true);
    let mb = net.mark("bb", "Bob");
    net.tell("aa", "Ann", "Bob@bb", "can you hear me?");
    assert!(tells(&mut net, "bb", "Bob", mb).is_empty(), "squelched");
    super::add_friend(&mut net, "aa", "Ann", "Bob@bb (Discord)");
    assert!(net
        .world("aa")
        .ts
        .world
        .federation
        .record(0x5000_0001)
        .is_some_and(|r| r.friends.contains_key("bob@bb")));
    let ma = net.mark("aa", "Ann");
    net.say("aa", "Ann", "@dn friends remove Bob@bb (Discord)");
    assert!(!Net::lines(&net.since("aa", "Ann", ma))
        .iter()
        .any(|l| l.contains("not in your friends list")));

    // With the network off, a name is taken as it is given, as before.
    net.world("aa").ts.world.federation.settings.enabled = false;
    let mc = net.mark("aa", "Cy");
    net.tell("aa", "Ann", "Cy (Discord)", "anyone?");
    assert!(tells(&mut net, "aa", "Cy", mc).is_empty());
}
