//! `@dn` said in chat.

use super::*;
use dereth_network_protocol::{ChannelOp, ChannelOutcome, Outcome, PersonOp, PersonOutcome};
use yalain_federation::{Friends as _, PAUSED, WORLD_OFF};
use yalain_friends::{Config, Service, HELP};

/// `@dn` shows the character's place on the network and the hub's link; `@dn on` and `@dn off`
/// put it on or take it off at once, kept in the store; `@dn help` and any word it does not know
/// show the commands. With the world off the network, every one says so.
/// Behaviour: server.friends.dn-said-in-chat
#[test]
fn dn_said_in_chat_shows_puts_on_and_takes_off_the_network() {
    let mut off = World::new();
    off.character("Ann");
    for words in [
        "",
        "on",
        "help",
        "link",
        "friends",
        "squelch coldeve",
        "channel list",
    ] {
        assert_eq!(
            off.said("Ann", words),
            vec![WORLD_OFF.to_owned()],
            "{words}"
        );
    }

    let mut w = World::linked();
    let ann = w.player("Ann");
    assert_eq!(
        w.said("Ann", ""),
        vec![
            "You are not on the Dereth Network. Say @dn on to join it as Ann@dawnsong.".to_owned()
        ]
    );
    assert_eq!(
        w.said("Ann", "off"),
        vec!["You are not on the Dereth Network.".to_owned()]
    );
    assert_eq!(
        w.said("Ann", "ON"),
        vec![
            "You are on the Dereth Network as Ann@dawnsong. Anyone on the network can see when \
             you are online and send you tells. Say @dn help for the commands."
                .to_owned(),
            "Say @dn off to leave the network.".to_owned(),
        ]
    );
    let (_, id) = w.ids("Ann");
    assert_eq!(w.store.network_characters().expect("reads"), vec![id]);
    assert_eq!(
        w.said("Ann", "on"),
        vec!["You are on the Dereth Network already, as Ann@dawnsong.".to_owned()]
    );
    w.request(
        ann,
        Request::FriendAdd {
            user: "Bob@coldeve".into(),
        },
    );
    assert_eq!(
        w.said("Ann", ""),
        vec![
            "You are on the Dereth Network as Ann@dawnsong. The hub is connected. You have 1 \
             friend on other worlds."
                .to_owned()
        ]
    );
    assert_eq!(
        w.said("Ann", "off"),
        vec![
            "You are off the Dereth Network. Nobody on another world can see you or tell you."
                .to_owned()
        ]
    );
    assert!(w.store.network_characters().expect("reads").is_empty());
    let help: Vec<String> = HELP.iter().map(|l| (*l).to_owned()).collect();
    assert_eq!(w.said("Ann", "help"), help);
    assert_eq!(w.said("Ann", "frobnicate"), help);
}

/// `@dn link` asks the hub and the answer comes as a line; `@dn status` reads as `@dn`, and says a
/// linked character is linked and where its directory page is; `@dn off` says a linked character's
/// link is paused; off the network `@dn link` and `@dn unlink` say so.
/// Behaviour: server.friends.dn-link-said
#[test]
fn dn_link_is_answered_by_the_hub_and_status_and_off_say_a_character_is_linked() {
    let mut w = World::linked();
    w.player("Ann");
    let no_address_yet = vec![yalain_federation::NOT_ON.to_owned()];
    assert_eq!(w.said("Ann", "link"), no_address_yet);
    assert_eq!(w.said("Ann", "unlink"), no_address_yet);
    w.on("Ann");
    w.frames();

    assert_eq!(w.said("Ann", "LINK"), Vec::<String>::new());
    let id = match w.frames().pop().expect("the request goes") {
        Message::Person { id, op, .. } => {
            assert_eq!(op, PersonOp::Code);
            id
        }
        other => panic!("{other:?}"),
    };
    w.hub(&Message::PersonResult {
        id,
        outcome: PersonOutcome::Ok,
        code: Some("K7QD-2MXP".into()),
        expires_secs: Some(60),
        url: Some("https://dereth.network/link/#K7QD-2MXP".into()),
        characters: Vec::new(),
    });
    assert_eq!(
        w.lines_of("Ann"),
        ["Open https://dereth.network/link/#K7QD-2MXP within 1 minute to link this character to \
          your Discord account. Keep the link to yourself."]
    );

    let not_linked = w.said("Ann", "");
    assert_eq!(not_linked, w.said("Ann", "status"));
    assert!(!not_linked[0].contains("linked"), "{not_linked:?}");
    w.hub(&Message::PersonFriends {
        to: addr("Ann@dawnsong"),
        linked: true,
        alts: Default::default(),
        persons: Vec::new(),
    });
    assert!(
        w.said("Ann", "")[0].ends_with("This character is linked to a Discord account."),
        "{:?}",
        w.said("Ann", "")
    );
    let (_, ann) = w.ids("Ann");
    let off = w.said("Ann", "off");
    assert_eq!(off.last().map(String::as_str), Some(PAUSED), "{off:?}");
    assert!(w.store.network_characters().expect("reads").is_empty());
    assert!(w.fed.as_ref().is_some_and(|f| f.asked_link(ann)));
}

/// `@dn friends` lists the friends on other worlds with their state; `add` and `remove` do what the
/// client's own add and delete do, with no request to answer, and say so; given wrongly each is its
/// own help line.
/// Behaviour: server.friends.dn-friends-said
#[test]
fn dn_friends_lists_adds_and_removes_friends_on_other_worlds() {
    let mut w = World::linked();
    let ann = w.player("Ann");
    w.on("Ann");
    w.frames();
    assert_eq!(
        w.said("Ann", "friends"),
        ["You have no friends on other worlds. Add one with @dn friends add Name@world."]
    );
    assert_eq!(
        w.said("Ann", "friends add"),
        ["@dn friends add <Name@world> - add a friend on another world"]
    );
    assert_eq!(
        w.said("Ann", "friends add Bob"),
        ["@dn friends add <Name@world> - add a friend on another world"],
        "only an address"
    );
    assert_eq!(
        w.said("Ann", "friends remove"),
        ["@dn friends remove <Name@world> - remove a friend on another world"]
    );
    assert_eq!(
        w.said("Ann", "friends add tide@channel"),
        ["A channel is not a character; it cannot be a friend."]
    );
    w.take(ann);
    assert_eq!(
        w.said("Ann", "FRIENDS add Bob@coldeve (Discord)"),
        ["Bob@coldeve has been added to your friends list."]
    );
    assert!(
        w.frames().iter().any(|m| matches!(m, Message::Watch { target, on: true, .. } if *target == addr("Bob@coldeve"))),
        "watched"
    );
    assert_eq!(
        w.said("Ann", "friends add Bob@coldeve"),
        ["That character is already in your friends list"]
    );
    w.hub(&Message::Presence {
        to: addr("Ann@dawnsong"),
        of: addr("Bob@coldeve"),
        online: true,
    });
    w.flush();
    assert!(
        w.take(ann).iter().any(
            |e| matches!(e, Element::Event(Event::FriendStat { user, mode })
            if user == "Bob@coldeve" && *mode == Mode::ONLINE)
        ),
        "the client is told its state"
    );
    assert_eq!(
        w.said("Ann", "friends"),
        ["Your friends on other worlds:", "Bob@coldeve: online"]
    );
    assert_eq!(
        w.said("Ann", "friends remove Bob@coldeve"),
        ["Bob@coldeve has been removed from your friends list."]
    );
    assert_eq!(
        w.said("Ann", "friends remove Bob@coldeve"),
        ["That character is not in your friends list!"]
    );
    assert!(
        w.frames().iter().any(|m| matches!(m, Message::Watch { target, on: false, .. } if *target == addr("Bob@coldeve"))),
        "no longer watched"
    );

    // Off the network, another world's friend is refused with the network's line.
    w.said("Ann", "off");
    assert_eq!(
        w.said("Ann", "friends add Bob@coldeve"),
        Vec::<String>::new()
    );
    assert_eq!(w.lines_of("Ann"), [yalain_federation::NOT_ON]);
}

/// `@dn squelch <world>` ignores every character of a world for the one character, kept across a
/// restart, and never shown in the client's own squelch list; at most fifty; its own world, or what
/// is no world, is the help line.
/// Behaviour: server.friends.dn-squelch-a-world
#[test]
fn dn_squelch_ignores_a_whole_world_without_showing_it_in_the_clients_list() {
    let mut w = World::linked();
    let ann = w.player("Ann");
    w.on("Ann");
    w.frames();
    let usage = ["@dn squelch <world> - squelch every character of a world"];
    assert_eq!(w.said("Ann", "squelch"), usage);
    assert_eq!(w.said("Ann", "squelch dawnsong"), usage, "its own world");
    assert_eq!(w.said("Ann", "squelch not_a_world!"), usage);
    assert_eq!(
        w.said("Ann", "unsquelch coldeve"),
        ["coldeve is not squelched."]
    );
    assert_eq!(
        w.said("Ann", "SQUELCH Coldeve"),
        ["Every character of coldeve has been squelched."]
    );
    assert_eq!(
        w.said("Ann", "squelch coldeve"),
        ["coldeve is already squelched."]
    );
    assert!(w.service.ignores("Ann", &addr("Bob@coldeve")));
    assert!(!w.service.ignores("Ann", &addr("Bob@frost")));

    // Not in the list the client is sent.
    w.take(ann);
    w.request(ann, Request::GetIgnoredList);
    let ignored = w.take(ann).into_iter().find_map(|e| match e {
        Element::Answer {
            answer: Answer::GetIgnoredList { ignored },
            ..
        } => Some(ignored),
        _ => None,
    });
    assert_eq!(
        ignored,
        Some(Vec::new()),
        "the client's list holds no world"
    );

    // A tell from it is not shown.
    w.told("t1", "Bob@coldeve", "Ann@dawnsong", "hello");
    assert!(
        w.frames().iter().any(|m| matches!(
            m,
            Message::TellResult {
                outcome: Outcome::Squelched,
                ..
            }
        )),
        "the teller sees what a squelch shows"
    );

    assert_eq!(
        w.said("Ann", "unsquelch coldeve"),
        ["coldeve has been unsquelched."]
    );
    assert!(!w.service.ignores("Ann", &addr("Bob@coldeve")));

    for n in 0..dereth_network_protocol::limits::MAX_SQUELCHED_WORLDS {
        assert_eq!(
            w.said("Ann", &format!("squelch world{n}")),
            [format!("Every character of world{n} has been squelched.")]
        );
    }
    assert_eq!(
        w.said("Ann", "squelch another"),
        ["You squelch 50 worlds already. Unsquelch one first."]
    );
    let loaded = Service::load(Config::new("Dawnsong", SLUG), &mut w.store).expect("loads");
    assert!(loaded.ignores("Ann", &addr("Bob@world7")), "kept");
}

/// `@dn channel ...` asks the hub, given rightly and on the network, and is answered as it answers;
/// given wrongly it is its help line, a bad name the rule for names, a gagged character the gag's
/// line. A tell to a channel goes through the link; squelching a channel's address leaves it.
/// Behaviour: server.friends.dn-channel-said
#[test]
fn dn_channel_asks_the_hub_and_a_squelch_of_a_channel_leaves_it() {
    let mut w = World::linked();
    let ann = w.player("Ann");
    let (_, id) = w.ids("Ann");
    assert_eq!(
        w.said("Ann", "channel create tide"),
        [yalain_federation::NOT_ON]
    );
    w.on("Ann");
    w.frames();
    assert_eq!(
        w.said("Ann", "channel create"),
        ["@dn channel create <channel> - make a channel"]
    );
    assert_eq!(
        w.said("Ann", "channel invite tide"),
        ["@dn channel invite <channel> <Name@world> - invite a character to a channel"]
    );
    assert_eq!(
        w.said("Ann", "channel invite tide other@channel"),
        ["@dn channel invite <channel> <Name@world> - invite a character to a channel"]
    );
    assert_eq!(w.said("Ann", "channel frobnicate").len(), HELP.len());
    assert_eq!(
        w.said("Ann", "channel create x"),
        ["A channel's name is 2 to 20 letters, digits and hyphens."]
    );
    assert_eq!(w.frames(), []);

    assert_eq!(
        w.said("Ann", "CHANNEL Create Tide@channel"),
        Vec::<String>::new()
    );
    let id_of_create = match w.frames().pop().expect("it goes") {
        Message::Channel {
            id,
            op: ChannelOp::Create,
            channel,
            ..
        } => {
            assert_eq!(channel, "tide");
            id
        }
        other => panic!("{other:?}"),
    };
    w.hub(&Message::ChannelResult {
        id: id_of_create,
        outcome: ChannelOutcome::Exists,
        channel: "tide".into(),
        members: Vec::new(),
        channels: Vec::new(),
    });
    assert_eq!(w.lines_of("Ann"), ["tide@channel already exists."]);
    assert_eq!(w.said("Ann", "channel list"), Vec::<String>::new());
    assert!(matches!(
        w.frames().pop(),
        Some(Message::Channel {
            op: ChannelOp::List,
            ..
        })
    ));

    // A tell to the channel.
    w.request(
        ann,
        Request::SendToUserEx {
            user: "tide@channel".into(),
            text: "evening".into(),
            extra: None,
        },
    );
    assert!(
        w.frames()
            .iter()
            .any(|m| matches!(m, Message::Tell { to, .. } if *to == addr("tide@channel"))),
        "through the link"
    );

    // Squelching its address leaves it.
    w.request(
        ann,
        Request::IgnoredAdd {
            user: "tide@channel".into(),
        },
    );
    assert!(
        w.frames().iter().any(|m| matches!(m, Message::Channel { op: ChannelOp::Leave, channel, .. } if channel == "tide")),
        "it leaves"
    );
    assert!(w.service.ignores("Ann", &addr("tide@channel")));

    // A gagged character makes no request.
    w.service.set_gagged(std::iter::once(id).collect());
    assert_eq!(
        w.said("Ann", "channel create tide2"),
        [yalain_friends::GAGGED]
    );
    assert_eq!(w.frames(), []);
}

/// `@dn global`, as `@dng` is given to the service: on a world that carries no Global room it says
/// so, with nothing to say its help, and a character off the network that it is off; otherwise the
/// line is kept for the host to say in the room, and nothing is answered.
/// Behaviour: server.friends.dn-global-said
#[test]
fn dn_global_says_a_line_in_the_room_that_carries_it_or_says_why_not() {
    use yalain_federation::{Room, NOT_CARRIED, NOT_ON};
    let mut none = World::linked();
    none.player("Ann");
    assert_eq!(none.said("Ann", "global hello"), [NOT_CARRIED]);
    none.on("Ann");
    none.frames();
    assert_eq!(none.said("Ann", "GLOBAL hello"), [NOT_CARRIED]);

    let mut w = World::linked();
    w.carry_global_in("general");
    w.player("Ann");
    assert_eq!(w.said("Ann", "global hello"), [NOT_ON], "off the network");
    assert_eq!(w.global_lines(), []);
    w.on("Ann");
    w.frames();
    assert_eq!(
        w.said("Ann", "global"),
        ["@dng <text> - say a line in the Global room"],
        "nothing to say"
    );
    assert_eq!(w.said("Ann", "GLOBAL hello   Arwic"), Vec::<String>::new());
    let (_, ann) = w.ids("Ann");
    assert_eq!(
        w.global_lines(),
        [yalain_federation::GlobalLine {
            character: ann,
            room: Room::General,
            text: "hello Arwic".into(),
        }]
    );
    assert_eq!(
        w.frames(),
        [],
        "the world says it, and it goes on from there"
    );

    // The hub moves the room, and the line goes there.
    w.carry_global_in("roleplay");
    assert_eq!(w.said("Ann", "global hello"), Vec::<String>::new());
    assert_eq!(w.global_lines()[0].room, Room::Pvp);
}
