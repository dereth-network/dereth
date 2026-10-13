//! Divergence: V450
//! Squelching across worlds: `/squelch Name@world` (sent by the client as
//! `Communication_ModifyCharacterSquelch`, `0x0058`, with the name and id 0) squelches a remote
//! character, `/dn squelch <world>` a whole world, and the operator's `/dn admin block` refuses
//! one for everyone; a squelched sender sees only its own echo, as a local one does.
//! Fixture: two virtual-time worlds through the hub's routing (`super::Net`).

use dereth_primitives::ObjectId;
use dereth_protocol::comms::{CommunicationHearDirectSpeech, CommunicationModifyCharacterSquelch};
use empyrean_entity::enums::ChatMessageType;

use super::{seed, two_worlds, Net, TELL};

pub(super) fn squelch(net: &mut Net, slug: &str, who: &str, name: &str, add: bool) {
    let w = net.world(slug);
    let id = w.clients[who];
    w.ts.send_game_action(
        id,
        &CommunicationModifyCharacterSquelch {
            add: i32::from(add),
            character_id: ObjectId(0),
            character_name: name.to_owned(),
            msg_type: ChatMessageType::AllChannels.0,
        },
    );
    net.advance(0.3);
}

fn tells(net: &mut Net, slug: &str, who: &str, mark: usize) -> usize {
    net.since(slug, who, mark)
        .iter()
        .filter(|g| {
            g.kind == TELL
                && !g
                    .decode::<CommunicationHearDirectSpeech>()
                    .message
                    .is_empty()
        })
        .count()
}

/// Divergence: V450
#[test]
fn a_squelched_remote_character_is_not_heard_and_sees_only_its_echo() {
    let mut net = two_worlds(false);
    let mb = net.mark("bb", "Bob");
    squelch(&mut net, "bb", "Bob", "Ann@aa", true);
    assert_eq!(
        Net::lines(&net.since("bb", "Bob", mb)),
        ["Ann@aa has been squelched."]
    );
    let (ma, mb) = (net.mark("aa", "Ann"), net.mark("bb", "Bob"));
    net.tell("aa", "Ann", "Bob@bb", "are you ignoring me?");
    assert_eq!(tells(&mut net, "bb", "Bob", mb), 0);
    assert_eq!(
        Net::lines(&net.since("aa", "Ann", ma)),
        ["You tell Bob@bb, \"are you ignoring me?\""],
        "nothing tells Ann she is squelched"
    );
    squelch(&mut net, "bb", "Bob", "Ann@aa", false);
    let mb = net.mark("bb", "Bob");
    net.tell("aa", "Ann", "Bob@bb", "hello again");
    assert_eq!(tells(&mut net, "bb", "Bob", mb), 1);
}

/// Divergence: V450
#[test]
fn squelching_a_world_silences_every_character_of_it() {
    let mut net = two_worlds(false);
    net.say("bb", "Bob", "@dn squelch aa");
    let mb = net.mark("bb", "Bob");
    net.tell("aa", "Ann", "Bob@bb", "hi");
    assert_eq!(tells(&mut net, "bb", "Bob", mb), 0);
}

/// Divergence: V450
#[test]
fn an_operator_block_refuses_a_world_for_everyone_and_looks_like_no_one_is_there() {
    let mut net = two_worlds(false);
    let out = empyrean_world::federation::commands::admin(
        &mut net.world("bb").ts.world,
        &["block".to_owned(), "aa".to_owned()],
    );
    assert_eq!(out, "aa is blocked for everyone on this world.");
    let (ma, mb) = (net.mark("aa", "Ann"), net.mark("bb", "Bob"));
    net.tell("aa", "Ann", "Bob@bb", "hi");
    assert_eq!(tells(&mut net, "bb", "Bob", mb), 0);
    let ann = net.since("aa", "Ann", ma);
    assert_eq!(ann.len(), 1);
    assert_eq!(ann[0].kind, super::WEENIE_ERROR, "Character not available");
}

/// Divergence: V450
#[test]
fn squelching_a_channel_leaves_it() {
    let mut net = two_worlds(false);
    net.say("aa", "Ann", "@dn channel create oldguard");
    net.say("aa", "Ann", "@dn channel invite oldguard Bob@bb");
    net.say("bb", "Bob", "@dn channel join oldguard");
    squelch(&mut net, "bb", "Bob", "oldguard@channel", true);
    let mb = net.mark("bb", "Bob");
    net.say("bb", "Bob", "@dn channel list");
    assert_eq!(
        Net::lines(&net.since("bb", "Bob", mb)),
        ["You are in no channel."]
    );
}

/// Divergence: V450
#[test]
fn an_operator_block_hides_this_worlds_characters_from_the_blocked_worlds_watches() {
    let mut net = two_worlds(false);
    empyrean_world::federation::commands::admin(
        &mut net.world("aa").ts.world,
        &["block".to_owned(), "bb".to_owned()],
    );
    let mb = net.mark("bb", "Bob");
    super::add_friend(&mut net, "aa", "Ann", "Bob@bb");
    super::add_friend(&mut net, "bb", "Bob", "Ann@aa");
    net.advance(1.0);
    let online_rows = net
        .since("bb", "Bob", mb)
        .iter()
        .filter(|g| g.kind == super::FRIENDS_UPDATE)
        .map(|g| g.decode::<dereth_protocol::social::SocialFriendsUpdate>())
        .filter(|u| u.friends.iter().any(|f| f.online == 1))
        .count();
    assert_eq!(
        online_rows, 0,
        "Bob, on a blocked world, never sees Ann online"
    );
}

/// The status rows `who` was sent since `mark`, as (name, online).
fn status_rows(net: &mut Net, slug: &str, who: &str, mark: usize) -> Vec<(String, bool)> {
    net.since(slug, who, mark)
        .iter()
        .filter(|g| g.kind == super::FRIENDS_UPDATE)
        .map(|g| g.decode::<dereth_protocol::social::SocialFriendsUpdate>())
        .filter(|u| u.update_type == 4)
        .flat_map(|u| u.friends.into_iter().map(|f| (f.name, f.online == 1)))
        .collect()
}

fn dn_admin(net: &mut Net, slug: &str, words: &[&str]) -> String {
    let args: Vec<String> = words.iter().map(|s| (*s).to_owned()).collect();
    let out = empyrean_world::federation::commands::admin(&mut net.world(slug).ts.world, &args);
    net.pump();
    net.advance(0.3);
    out
}

/// Divergence: V450
#[test]
fn a_block_made_while_mutual_friends_are_online_shows_each_offline_to_the_other_at_once() {
    let mut net = two_worlds(true);
    let (ma, mb) = (net.mark("aa", "Ann"), net.mark("bb", "Bob"));
    assert_eq!(
        dn_admin(&mut net, "aa", &["block", "bb"]),
        "bb is blocked for everyone on this world."
    );
    assert_eq!(
        status_rows(&mut net, "aa", "Ann", ma),
        [("Bob@bb".to_owned(), false)],
        "Ann sees Bob offline, with no line"
    );
    assert!(Net::lines(&net.since("aa", "Ann", ma)).is_empty());
    assert_eq!(
        status_rows(&mut net, "bb", "Bob", mb),
        [("Ann@aa".to_owned(), false)],
        "Bob sees Ann offline"
    );
    // Nothing more about Ann reaches bb: not her logout, not a watch.
    let mark = net.sent.0.len();
    let ann = net.world("aa").clients["Ann"];
    net.world("aa").ts.client_mut(ann).log_off();
    net.advance(12.0);
    assert!(
        net.sent_since("aa", mark).iter().all(|m| !matches!(
            m,
            dereth_network_protocol::Message::Watch { .. }
                | dereth_network_protocol::Message::Presence { .. }
        )),
        "{:?}",
        net.sent_since("aa", mark)
    );
    assert!(
        status_rows(&mut net, "bb", "Bob", mb)
            .iter()
            .all(|(_, online)| !online),
        "Bob never sees Ann online again"
    );
}

/// Divergence: V450
#[test]
fn lifting_a_block_shows_mutual_friends_to_each_other_again() {
    let mut net = two_worlds(true);
    dn_admin(&mut net, "aa", &["block", "bb"]);
    let (ma, mb) = (net.mark("aa", "Ann"), net.mark("bb", "Bob"));
    assert_eq!(
        dn_admin(&mut net, "aa", &["unblock", "bb"]),
        "bb is no longer blocked."
    );
    assert_eq!(
        status_rows(&mut net, "aa", "Ann", ma),
        [("Bob@bb".to_owned(), true)]
    );
    assert_eq!(
        status_rows(&mut net, "bb", "Bob", mb),
        [("Ann@aa".to_owned(), true)]
    );
}

/// Divergence: V450
#[test]
fn a_character_never_watches_a_friend_on_a_blocked_world() {
    let mut net = two_worlds(false);
    dn_admin(&mut net, "aa", &["block", "bb"]);
    let mark = net.sent.0.len();
    super::add_friend(&mut net, "aa", "Ann", "Bob@bb");
    net.say("aa", "Ann", "@dn off");
    net.say("aa", "Ann", "@dn on");
    let to_bb: Vec<_> = net
        .sent_since("aa", mark)
        .into_iter()
        .filter(|m| match m {
            dereth_network_protocol::Message::Watch { target, .. } => target.world() == "bb",
            dereth_network_protocol::Message::Presence { to, .. } => to.world() == "bb",
            _ => false,
        })
        .collect();
    assert!(to_bb.is_empty(), "{to_bb:?}");
}

/// Divergence: V451
#[test]
fn with_the_network_off_dn_admin_blocks_nothing_and_says_so() {
    let ann = seed("ann", 0x5000_0001, "Ann");
    let mut net = Net::new(&[("aa", &[ann])]);
    net.world("aa").ts.world.federation.settings.enabled = false;
    for words in [&["block", "bb"][..], &["unblock", "bb"], &["block"]] {
        assert_eq!(
            dn_admin(&mut net, "aa", words),
            "This world is not on the Dereth Network.",
            "{words:?}"
        );
    }
    assert!(net.world("aa").ts.world.federation.store.blocked.is_empty());
    assert!(!net.world("aa").ts.world.federation.dirty);
    assert!(dn_admin(&mut net, "aa", &["status"])
        .starts_with("dn admin block <world or Name@world> - "));
}

fn invite_lines(net: &mut Net, mark: usize) -> Vec<String> {
    Net::lines(&net.since("bb", "Bob", mark))
        .into_iter()
        .filter(|l| l.contains("invites you"))
        .collect()
}

/// Divergence: V450
#[test]
fn an_invitation_reaches_only_a_character_its_inviter_may_reach() {
    let mut net = two_worlds(false);
    squelch(&mut net, "bb", "Bob", "Ann@aa", true);
    let mb = net.mark("bb", "Bob");
    net.say("aa", "Ann", "@dn channel create freegold2");
    net.say("aa", "Ann", "@dn channel invite freegold2 Bob@bb");
    assert!(
        invite_lines(&mut net, mb).is_empty(),
        "nor past a squelch of her"
    );

    squelch(&mut net, "bb", "Bob", "Ann@aa", false);
    net.say("bb", "Bob", "@dn squelch aa");
    let mb = net.mark("bb", "Bob");
    net.say("aa", "Ann", "@dn channel create freegold3");
    net.say("aa", "Ann", "@dn channel invite freegold3 Bob@bb");
    assert!(
        invite_lines(&mut net, mb).is_empty(),
        "nor past a squelch of her world"
    );

    net.say("bb", "Bob", "@dn unsquelch aa");
    let mb = net.mark("bb", "Bob");
    net.say("aa", "Ann", "@dn channel create freegold4");
    net.say("aa", "Ann", "@dn channel invite freegold4 Bob@bb");
    net.say("aa", "Ann", "@dn channel invite freegold4 Bob@bb");
    assert_eq!(
        invite_lines(&mut net, mb).len(),
        1,
        "and a second invitation to the same channel is not shown again"
    );
}

/// Divergence: V450
#[test]
fn lifting_a_block_shows_a_watcher_the_character_does_not_list_what_it_may_see_again() {
    let mut net = two_worlds(false);
    // Bob lists Ann; Ann lists nobody, and shows to anyone.
    let mb = net.mark("bb", "Bob");
    super::add_friend(&mut net, "bb", "Bob", "Ann@aa");
    assert_eq!(
        status_rows(&mut net, "bb", "Bob", mb),
        [("Ann@aa".to_owned(), true)]
    );
    let mb = net.mark("bb", "Bob");
    dn_admin(&mut net, "aa", &["block", "bb"]);
    assert_eq!(
        status_rows(&mut net, "bb", "Bob", mb),
        [("Ann@aa".to_owned(), false)]
    );
    let mb = net.mark("bb", "Bob");
    dn_admin(&mut net, "aa", &["unblock", "bb"]);
    net.advance(1.0);
    assert_eq!(
        status_rows(&mut net, "bb", "Bob", mb),
        [("Ann@aa".to_owned(), true)],
        "Bob sees Ann online again, without relogging"
    );
    // And he is still told of her changes.
    let mb = net.mark("bb", "Bob");
    let ann = net.world("aa").clients["Ann"];
    net.world("aa").ts.client_mut(ann).log_off();
    net.advance(12.0);
    assert_eq!(
        status_rows(&mut net, "bb", "Bob", mb),
        [("Ann@aa".to_owned(), false)]
    );
}

/// Divergence: V450
#[test]
fn a_character_squelches_at_most_so_many_remote_characters_and_worlds() {
    let mut net = two_worlds(false);
    let ma = net.mark("aa", "Ann");
    for i in 0..=empyrean_world::federation::MAX_REMOTE_SQUELCHES {
        squelch(
            &mut net,
            "aa",
            "Ann",
            &format!(
                "{}@bb",
                super::friends::minted(u32::try_from(i).expect("small"))
            ),
            true,
        );
    }
    let lines = Net::lines(&net.since("aa", "Ann", ma));
    assert_eq!(
        lines.last().map(String::as_str),
        Some("You squelch 100 characters on other worlds already. Unsquelch one first.")
    );
    let ma = net.mark("aa", "Ann");
    for i in 0..=empyrean_world::federation::MAX_SQUELCHED_WORLDS {
        let world: String = super::friends::minted(u32::try_from(i).expect("small"))
            .to_lowercase()
            .chars()
            .skip(4)
            .collect();
        net.say("aa", "Ann", &format!("@dn squelch w{world}x"));
    }
    let lines = Net::lines(&net.since("aa", "Ann", ma));
    assert_eq!(
        lines.last().map(String::as_str),
        Some("You squelch 50 worlds already. Unsquelch one first.")
    );
    let rec = net
        .world("aa")
        .ts
        .world
        .federation
        .record(0x5000_0001)
        .expect("Ann's record")
        .clone();
    assert_eq!(
        (rec.squelched.len(), rec.squelched_worlds.len()),
        (
            empyrean_world::federation::MAX_REMOTE_SQUELCHES,
            empyrean_world::federation::MAX_SQUELCHED_WORLDS
        )
    );
}
