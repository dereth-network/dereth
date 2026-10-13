//! Divergence: V451
//! The link to the hub: `/dn on` and `/dn off`, the help, the minute a tell waits while the hub is away,
//! what a sender is told when it never returns, and remote friends showing offline when a world
//! or the hub is gone.
//! Fixture: virtual-time worlds through the hub's routing (`super::Net`).

use dereth_protocol::comms::CommunicationHearDirectSpeech;
use dereth_protocol::social::SocialFriendsUpdate;

use super::{seed, two_worlds, Net, FRIENDS_UPDATE, TELL, WEENIE_ERROR};

/// Divergence: V451
#[test]
fn dn_on_puts_one_character_on_the_network_for_anyone_to_see_and_reach_and_dn_off_takes_it_off() {
    let ann = seed("ann", 0x5000_0001, "Ann");
    let mut net = Net::new(&[("aa", &[ann])]);
    net.link("aa");
    net.enter("aa", ann);
    let on = |net: &mut Net| {
        net.world("aa")
            .ts
            .world
            .federation
            .record(0x5000_0001)
            .is_some_and(|r| r.settings.on)
    };
    assert!(!on(&mut net), "off until its player puts it on");
    let ma = net.mark("aa", "Ann");
    net.say("aa", "Ann", "@dn on");
    assert!(on(&mut net));
    assert_eq!(
        Net::lines(&net.since("aa", "Ann", ma)),
        [
            "You are on the Dereth Network as Ann@aa. Anyone on the network can see when you are online, send you tells and find your profile in its directory. Type @dn help for the commands.",
            "Your profile shows your level, skills, allegiance and more, and changes as you play. Type @dn off to leave the network.",
            "Your directory page: https://dereth.network/directory/character/?a=Ann%40aa",
        ]
    );
    let ma = net.mark("aa", "Ann");
    net.say("aa", "Ann", "@dn on");
    net.say("aa", "Ann", "@dn");
    assert_eq!(
        Net::lines(&net.since("aa", "Ann", ma)),
        [
            "You are on the Dereth Network already, as Ann@aa.",
            "You are on the Dereth Network as Ann@aa. The hub is connected. You have no friends on other worlds.",
            "Your directory page: https://dereth.network/directory/character/?a=Ann%40aa",
        ]
    );
    let ma = net.mark("aa", "Ann");
    net.say("aa", "Ann", "@dn off");
    net.say("aa", "Ann", "@dn");
    net.say("aa", "Ann", "@dn off");
    assert!(!on(&mut net));
    assert_eq!(
        Net::lines(&net.since("aa", "Ann", ma)),
        [
            "You are off the Dereth Network. Nobody on another world can see you or tell you, and your profile is out of its directory.",
            "You are not on the Dereth Network. Type @dn on to join it as Ann@aa.",
            "You are not on the Dereth Network.",
        ]
    );
    assert!(
        net.world("aa").ts.world.federation.dirty,
        "the change is to be saved"
    );
}

/// Divergence: V451
#[test]
fn dn_help_is_one_line_per_command_and_the_removed_commands_are_gone() {
    let ann = seed("ann", 0x5000_0001, "Ann");
    let mut net = Net::new(&[("aa", &[ann])]);
    net.link("aa");
    net.enter("aa", ann);
    net.say("aa", "Ann", "@dn on");
    let ma = net.mark("aa", "Ann");
    net.say("aa", "Ann", "@dn help");
    let help = Net::lines(&net.since("aa", "Ann", ma));
    let help: Vec<&str> = help[0].lines().collect();
    assert_eq!(
        help[..3],
        [
            "@dn - show your Dereth Network status",
            "@dn on - put this character on the Dereth Network",
            "@dn off - take this character off the Dereth Network",
        ]
    );
    for line in &help {
        assert!(
            line.starts_with('@') && line.matches(" - ").count() == 1,
            "{line}"
        );
        assert!(!line.contains("  "), "{line}");
    }
    for gone in [
        "@dn visible anyone",
        "@dn reachable nobody",
        "@dn listed on",
        "@dn alts hidden",
        "@dn worlds",
    ] {
        let ma = net.mark("aa", "Ann");
        net.say("aa", "Ann", gone);
        let lines = Net::lines(&net.since("aa", "Ann", ma));
        assert!(
            lines.len() == 1 && lines[0].starts_with("@dn - show your Dereth Network status\n"),
            "{gone}: {lines:?}"
        );
    }
}

/// Divergence: V451
#[test]
fn derethnetwork_is_dn_spelled_out_and_dn_admin_is_the_operators_alone() {
    let ann = seed("ann", 0x5000_0001, "Ann");
    let mut boss = seed("boss", 0x5000_0002, "Boss");
    boss.access = empyrean_entity::enums::AccessLevel::Sentinel;
    let mut net = Net::new(&[("aa", &[ann, boss])]);
    net.link("aa");
    net.enter("aa", ann);
    net.enter("aa", boss);
    net.say("aa", "Ann", "@derethnetwork on");
    assert!(
        net.world("aa")
            .ts
            .world
            .federation
            .record(0x5000_0001)
            .is_some_and(|r| r.settings.on),
        "the long name opts the character in"
    );
    let ma = net.mark("aa", "Ann");
    net.say("aa", "Ann", "@dn admin block bb");
    assert!(
        net.world("aa").ts.world.federation.store.blocked.is_empty(),
        "a player blocks nothing"
    );
    assert!(
        Net::lines(&net.since("aa", "Ann", ma))[0].starts_with("@dn - "),
        "and is shown the player's help"
    );
    let mb = net.mark("aa", "Boss");
    net.say("aa", "Boss", "@dn admin block bb");
    assert!(net
        .world("aa")
        .ts
        .world
        .federation
        .store
        .blocked
        .contains("bb"));
    assert_eq!(
        Net::lines(&net.since("aa", "Boss", mb)),
        ["bb is blocked for everyone on this world."]
    );
    // Without a target, block and unblock show the network's state and the blocks.
    for bare in ["@dn admin block", "@dn admin unblock"] {
        let mb = net.mark("aa", "Boss");
        net.say("aa", "Boss", bare);
        let lines = Net::lines(&net.since("aa", "Boss", mb));
        let lines: Vec<&str> = lines[0].lines().collect();
        assert!(
            lines[0]
                .starts_with("This world is on the Dereth Network as aa. The hub is connected."),
            "{bare}: {lines:?}"
        );
        assert_eq!(lines[1], "Blocked: bb.", "{bare}");
    }
    let mb = net.mark("aa", "Boss");
    net.say("aa", "Boss", "@dn admin status");
    assert!(
        Net::lines(&net.since("aa", "Boss", mb))[0]
            .starts_with("@dn admin block <world or Name@world> - "),
        "the words it no longer has show the operator's help"
    );
}

/// Divergence: V451
#[test]
fn dn_verify_is_an_admins_and_hands_the_token_to_the_host_never_waiting_on_the_hub() {
    let ann = seed("ann", 0x5000_0001, "Ann");
    let mut boss = seed("boss", 0x5000_0002, "Boss");
    boss.access = empyrean_entity::enums::AccessLevel::Admin;
    let mut net = Net::new(&[("aa", &[ann, boss])]);
    net.enter("aa", ann);
    net.enter("aa", boss);
    let token = format!("dnv_{}", "Q".repeat(32));
    let take = |net: &mut Net| {
        empyrean_world::federation::take_verify_requests(&mut net.world("aa").ts.world)
    };

    let ma = net.mark("aa", "Ann");
    net.say("aa", "Ann", &format!("@dn verify {token}"));
    assert!(take(&mut net).is_empty(), "a player verifies nothing");
    assert!(Net::lines(&net.since("aa", "Ann", ma))[0].starts_with("@dn - "));

    let mb = net.mark("aa", "Boss");
    net.say("aa", "Boss", "@dn verify not-a-token");
    assert_eq!(
        Net::lines(&net.since("aa", "Boss", mb)),
        ["@dn verify <token> - verify this world with the token the site gave you"]
    );
    assert!(take(&mut net).is_empty());

    let mb = net.mark("aa", "Boss");
    net.say("aa", "Boss", &format!("@dn verify {token}"));
    net.say("aa", "Boss", &format!("@derethnetwork verify {token}"));
    assert_eq!(
        Net::lines(&net.since("aa", "Boss", mb)),
        [
            "Asking the Dereth Network's hub to verify this world...",
            "A verification is on its way to the hub already."
        ]
    );
    let sent = take(&mut net);
    assert_eq!(sent.len(), 1);
    assert_eq!(
        (sent[0].token.as_str(), sent[0].by.as_str()),
        (token.as_str(), "Boss")
    );
    let boss_guid = sent[0].who.expect("the admin who typed it");

    // The host's answer comes back to the admin on a later pump.
    let mb = net.mark("aa", "Boss");
    empyrean_world::federation::on_verify_answer(
        &mut net.world("aa").ts.world,
        Some(boss_guid),
        "The Dereth Network verified this world.",
    );
    net.advance(0.2);
    assert_eq!(
        Net::lines(&net.since("aa", "Boss", mb)),
        ["The Dereth Network verified this world."]
    );

    // At the console, as `dn verify`.
    let args = vec!["verify".to_owned(), token.clone()];
    empyrean_command::empyrean::dn::handle_dn(&mut net.world("aa").ts.world, None, &args);
    let sent = take(&mut net);
    assert_eq!(
        (sent.len(), sent[0].by.as_str(), sent[0].who),
        (1, "the console", None)
    );
}

/// Divergence: V451
#[test]
fn a_tell_sent_while_the_hub_is_away_goes_when_it_returns_within_the_minute() {
    let mut net = two_worlds(false);
    net.unlink("aa");
    let mb = net.mark("bb", "Bob");
    net.tell("aa", "Ann", "Bob@bb", "are you there?");
    assert_eq!(
        net.world("aa").ts.world.federation.queued(),
        1,
        "held while the hub is away"
    );
    net.advance(20.0);
    net.link("aa");
    let got: Vec<CommunicationHearDirectSpeech> = net
        .since("bb", "Bob", mb)
        .iter()
        .filter(|g| g.kind == TELL)
        .map(|g| g.decode())
        .collect();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].message, "are you there?");
}

/// Divergence: V451
#[test]
fn a_tell_the_hub_never_takes_is_dropped_after_a_minute_and_the_sender_told() {
    let mut net = two_worlds(false);
    net.unlink("aa");
    let ma = net.mark("aa", "Ann");
    net.tell("aa", "Ann", "Bob@bb", "hello?");
    net.advance(59.0);
    assert!(net.since("aa", "Ann", ma).is_empty(), "nothing yet");
    net.advance(1.5);
    let ann = net.since("aa", "Ann", ma);
    assert_eq!(ann[0].kind, WEENIE_ERROR);
    assert_eq!(Net::lines(&ann), ["The Dereth Network is unavailable."]);
    assert_eq!(net.world("aa").ts.world.federation.queued(), 0);
}

/// Divergence: V451
#[test]
fn a_world_that_goes_away_shows_its_characters_offline_without_a_line() {
    let mut net = two_worlds(true);
    let ma = net.mark("aa", "Ann");
    net.unlink("bb");
    let got = net.since("aa", "Ann", ma);
    let ups: Vec<SocialFriendsUpdate> = got
        .iter()
        .filter(|g| g.kind == FRIENDS_UPDATE)
        .map(|g| g.decode())
        .collect();
    assert!(ups
        .iter()
        .any(|u| u.update_type == 4 && u.friends[0].online == 0));
    assert!(
        Net::lines(&got).is_empty(),
        "the world went away; Bob did not log off"
    );

    // It comes back: the watches go again and Bob shows online.
    let ma = net.mark("aa", "Ann");
    net.link("bb");
    let got = net.since("aa", "Ann", ma);
    let ups: Vec<SocialFriendsUpdate> = got
        .iter()
        .filter(|g| g.kind == FRIENDS_UPDATE)
        .map(|g| g.decode())
        .collect();
    assert!(
        ups.iter()
            .any(|u| u.update_type == 4 && u.friends[0].online == 1),
        "{ups:?}"
    );
    assert!(
        Net::lines(&got).is_empty(),
        "the world came back; Bob did not log in"
    );
}

/// Divergence: V451
#[test]
fn a_hub_lost_for_good_answers_a_waiting_tell_that_the_world_is_unreachable() {
    let mut net = two_worlds(false);
    // bb's link is down at the hub but bb itself does not know yet; the hub holds the tell a
    // minute for bb, then answers.
    let outs = net.hub.disconnect("bb");
    drop(outs);
    net.world("bb").linked = false;
    let ma = net.mark("aa", "Ann");
    net.tell("aa", "Ann", "Bob@bb", "anyone?");
    net.advance(61.0);
    let ann = net.since("aa", "Ann", ma);
    assert_eq!(ann[0].kind, WEENIE_ERROR);
    assert_eq!(
        Net::lines(&ann),
        ["The Dereth Network can't reach bb right now."]
    );
}

/// Divergence: V451
#[test]
fn a_burst_of_watches_at_a_reconnect_is_paced_under_the_hubs_limit() {
    let mut net = two_worlds(false);
    {
        let rec = net
            .world("aa")
            .ts
            .world
            .federation
            .store
            .characters
            .get_mut(&0x5000_0001)
            .map(std::sync::Arc::make_mut)
            .expect("Ann's record");
        for i in 0..300u32 {
            let name = format!(
                "Fr{}{}",
                char::from(b'a' + u8::try_from(i % 26).expect("a letter")),
                char::from(b'a' + u8::try_from(i / 26).expect("a letter"))
            );
            let a =
                dereth_network_protocol::Address::parse(&format!("{name}@bb")).expect("an address");
            rec.friends.insert(a.key(), a);
        }
    }
    net.unlink("aa");
    let wire = net.wire.0.len();
    net.link("aa");
    let watches = |net: &Net| {
        net.wire_since(wire)
            .iter()
            .filter(|l| *l == "aa -> hub: watch")
            .count()
    };
    let first = watches(&net);
    assert!(first < 300, "not all at once: {first}");
    // To one world, at that world's share of the pace (a quarter, a tenth under).
    net.advance(30.0);
    assert_eq!(watches(&net), 300, "every watch reaches the hub in the end");
}

/// Divergence: V451
#[test]
fn an_operator_cannot_rename_a_character_to_an_address_while_the_network_is_on() {
    let mut boss = seed("boss", 0x5000_0002, "Boss");
    boss.access = empyrean_entity::enums::AccessLevel::Admin;
    let ann = seed("ann", 0x5000_0001, "Ann");
    let mut net = Net::new(&[("aa", &[ann, boss])]);
    net.enter("aa", boss);
    let m = net.mark("aa", "Boss");
    net.say("aa", "Boss", "@rename Ann, Bob@bb");
    assert_eq!(
        Net::lines(&net.since("aa", "Boss", m)),
        ["Error, cannot rename. A name may not hold @ while this world is on the Dereth Network."]
    );
}

/// Divergence: V451
#[test]
fn a_tell_the_link_lost_on_its_way_goes_again_when_the_link_is_back() {
    let mut net = two_worlds(false);
    let mb = net.mark("bb", "Bob");
    // Ann's tell is handed to the link, which drops before the hub reads it.
    let w = net.world("aa");
    let ann = w.clients["Ann"];
    w.ts.send_game_action(
        ann,
        &dereth_protocol::comms::CommunicationTalkDirectByName {
            message: "did this get through?".into(),
            target_name: "Bob@bb".into(),
        },
    );
    w.ts.advance(0.1);
    let lost = empyrean_world::federation::take_frames(&mut w.ts.world);
    assert!(lost.iter().any(|f| f.contains("\"tell\"")), "{lost:?}");
    net.unlink("aa");
    net.link("aa");
    let got: Vec<CommunicationHearDirectSpeech> = net
        .since("bb", "Bob", mb)
        .iter()
        .filter(|g| g.kind == TELL)
        .map(|g| g.decode())
        .collect();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].message, "did this get through?");
}

/// Divergence: V451
#[test]
fn a_tell_the_hub_holds_for_a_returning_world_is_not_given_up_on_first() {
    let mut net = two_worlds(false);
    net.unlink("bb");
    net.unlink("aa");
    let ma = net.mark("aa", "Ann");
    net.tell("aa", "Ann", "Bob@bb", "still there?");
    // aa is back after 40 seconds and hands the tell over; the hub holds it for bb, which is
    // back 40 seconds after that.
    net.advance(40.0);
    net.link("aa");
    net.advance(40.0);
    net.link("bb");
    net.advance(1.0);
    let ann = net.since("aa", "Ann", ma);
    assert_eq!(
        Net::lines(&ann),
        ["You tell Bob@bb, \"still there?\""],
        "Bob was shown it, so Ann sees the echo and nothing else"
    );
    assert!(ann.iter().all(|g| g.kind != WEENIE_ERROR));
}

/// Divergence: V451
#[test]
fn a_tell_answered_while_the_senders_link_was_down_is_shown_to_the_sender_as_answered() {
    let mut net = two_worlds(true);
    // bb is away, so the hub holds Ann's tell; then aa's own link goes too.
    net.unlink("bb");
    let ma = net.mark("aa", "Ann");
    net.tell("aa", "Ann", "Bob@bb", "are you there?");
    net.unlink("aa");
    // bb is back and shows the tell; its answer finds aa's link down.
    let mb = net.mark("bb", "Bob");
    net.link("bb");
    net.advance(1.0);
    let shown = net
        .since("bb", "Bob", mb)
        .iter()
        .filter(|g| g.kind == TELL)
        .count();
    assert_eq!(shown, 1, "Bob is shown the tell");
    // aa is back and sends the tell again; the hub answers it as it answered the first.
    net.link("aa");
    net.advance(110.0);
    let ann = net.since("aa", "Ann", ma);
    assert_eq!(
        Net::lines(&ann),
        ["You tell Bob@bb, \"are you there?\""],
        "Ann sees the echo, nothing else"
    );
    assert!(
        ann.iter().all(|g| g.kind != WEENIE_ERROR),
        "and no Character not available"
    );
    assert_eq!(
        net.since("bb", "Bob", mb)
            .iter()
            .filter(|g| g.kind == TELL)
            .count(),
        1,
        "and Bob is not shown it twice"
    );
}
