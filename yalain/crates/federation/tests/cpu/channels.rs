//! Channels: the requests, an invitation, a channel's line, and a tell to one.

use dereth_network_protocol::{ChannelOp, ChannelOutcome, Message, Outcome};
use yalain_federation::{Ended, NOT_ON, UNAVAILABLE};

use crate::{addr, World};

/// A request, whom it names, the hub's answer, its members and channels, and the line shown.
type Case<'a> = (
    ChannelOp,
    Option<&'a str>,
    ChannelOutcome,
    Vec<&'a str>,
    Vec<&'a str>,
    &'a str,
);

fn with_ann() -> (World, u64) {
    let mut w = World::new();
    w.on_and_present("Ann");
    w.link(&[("coldeve", true)]);
    w.frames();
    let id = w.ids["Ann"];
    (w, id)
}

/// `Ann` asks; the request's id and the message the hub is sent.
fn asked(w: &mut World, ann: u64, op: ChannelOp, channel: &str, who: Option<&str>) -> String {
    let t = w.t;
    w.fed
        .ask_channel(t, ann, "Ann", op, channel, who.map(addr))
        .expect("it goes");
    match w.frames().pop().expect("the request goes") {
        Message::Channel {
            id,
            op: sent,
            channel: named,
            by,
            who: invited,
        } => {
            assert_eq!(sent, op);
            assert_eq!(named, channel);
            assert_eq!(by, addr("Ann@dawnsong"));
            assert_eq!(invited, who.map(addr));
            id
        }
        other => panic!("{other:?}"),
    }
}

/// A channel request goes to the hub under the character's address, and each answer is shown as the
/// network's own line for it. Off the network nothing goes; a request never answered ends in the
/// network's line.
/// Behaviour: server.network.channel-requests-answered
#[test]
fn a_channel_request_goes_to_the_hub_and_its_answer_is_shown() {
    let (mut w, ann) = with_ann();
    let answered = |w: &mut World,
                    op: ChannelOp,
                    who: Option<&str>,
                    outcome: ChannelOutcome,
                    members: Vec<&str>,
                    channels: Vec<&str>| {
        let id = asked(w, ann, op, "oldguard", who);
        w.hub(&Message::ChannelResult {
            id,
            outcome,
            channel: "oldguard".into(),
            members: members.into_iter().map(addr).collect(),
            channels: channels.into_iter().map(str::to_owned).collect(),
        });
        w.service.lines.pop().expect("a line")
    };
    let cases: Vec<Case> = vec![
        (ChannelOp::Create, None, ChannelOutcome::Ok, vec![], vec![],
         "You have made the channel oldguard@channel. Invite someone with @dn channel invite oldguard Name@world."),
        (ChannelOp::Create, None, ChannelOutcome::Exists, vec![], vec![], "oldguard@channel already exists."),
        (ChannelOp::Invite, Some("Bob@coldeve"), ChannelOutcome::Ok, vec![], vec![], "Bob@coldeve is invited to oldguard@channel."),
        (ChannelOp::Join, None, ChannelOutcome::NotInvited, vec![], vec![], "You have not been invited to oldguard@channel."),
        (ChannelOp::Leave, None, ChannelOutcome::Ok, vec![], vec![], "You have left oldguard@channel."),
        (ChannelOp::Members, None, ChannelOutcome::Ok, vec!["Ann@dawnsong", "Bob@coldeve"], vec![],
         "Members of oldguard@channel: Ann@dawnsong, Bob@coldeve."),
        (ChannelOp::List, None, ChannelOutcome::Ok, vec![], vec!["oldguard", "tide"], "Your channels: oldguard@channel, tide@channel."),
        (ChannelOp::List, None, ChannelOutcome::Ok, vec![], vec![], "You are in no channel."),
    ];
    for (op, who, outcome, members, channels, line) in cases {
        assert_eq!(
            answered(&mut w, op, who, outcome, members, channels),
            (ann, line.to_owned())
        );
    }

    let t = w.t;
    w.set_on("Ann", false);
    w.frames();
    assert_eq!(
        w.fed
            .ask_channel(t, ann, "Ann", ChannelOp::Create, "tide", None),
        Err(NOT_ON)
    );
    assert_eq!(w.frames(), []);

    let (mut w, ann) = with_ann();
    asked(&mut w, ann, ChannelOp::Create, "tide", None);
    w.advance(10_000.0);
    assert_eq!(w.service.lines, [(ann, UNAVAILABLE.to_owned())]);
}

/// An invitation is shown as a tell from the inviter would be: to a present character on the
/// network, never past an ignore of the inviter, its world or the channel, or the operator's blocks.
/// Behaviour: server.network.channel-invitation-shown-as-a-tell
#[test]
fn an_invitation_is_shown_only_as_a_tell_from_the_inviter_would_be() {
    let (mut w, ann) = with_ann();
    let invite = |w: &mut World, to: &str| {
        w.hub(&Message::ChannelInvite {
            channel: "oldguard".into(),
            by: addr("Bob@coldeve"),
            to: addr(to),
        });
        std::mem::take(&mut w.service.lines)
    };
    assert_eq!(
        invite(&mut w, "Ann@dawnsong"),
        [(
            ann,
            "Bob@coldeve invites you to the channel oldguard@channel. Say @dn channel join \
             oldguard to join it."
                .to_owned()
        )]
    );
    assert_eq!(invite(&mut w, "Ann@frost"), [], "not of this world");
    assert_eq!(invite(&mut w, "Nobody@dawnsong"), [], "no such character");
    for ignored in ["bob@coldeve", "coldeve", "oldguard@channel"] {
        w.service.ignored.insert("ann".into(), vec![ignored.into()]);
        assert_eq!(invite(&mut w, "Ann@dawnsong"), [], "{ignored}");
    }
    w.service.ignored.clear();
    w.operator(&["block", "coldeve"]);
    assert_eq!(invite(&mut w, "Ann@dawnsong"), [], "blocked");
    w.operator(&["unblock", "coldeve"]);
    w.leave("Ann");
    assert_eq!(invite(&mut w, "Ann@dawnsong"), [], "not present");
}

/// A channel's line reaches a member as a tell from the channel's address, its text the speaker's
/// address and the line; never past an ignore of the speaker or the channel, a block, or to one not
/// present.
/// Behaviour: server.network.channel-line-shown-as-a-tell
#[test]
fn a_channels_line_reaches_a_member_as_a_tell_from_the_channels_address() {
    let (mut w, _) = with_ann();
    let line = |w: &mut World, to: &str| {
        w.hub(&Message::ChannelLine {
            channel: "oldguard".into(),
            from: addr("Bob@coldeve"),
            to: addr(to),
            text: "anyone around tonight?".into(),
        });
        std::mem::take(&mut w.service.tells)
    };
    assert_eq!(
        line(&mut w, "Ann@dawnsong"),
        [(
            "Ann".to_owned(),
            addr("oldguard@channel"),
            "Bob@coldeve: anyone around tonight?".to_owned()
        )]
    );
    for ignored in ["bob@coldeve", "coldeve", "oldguard@channel"] {
        w.service.ignored.insert("ann".into(), vec![ignored.into()]);
        assert_eq!(line(&mut w, "Ann@dawnsong"), [], "{ignored}");
    }
    w.service.ignored.clear();
    w.operator(&["block", "Bob@coldeve"]);
    assert_eq!(line(&mut w, "Ann@dawnsong"), [], "blocked");
    w.operator(&["unblock", "Bob@coldeve"]);
    w.leave("Ann");
    assert_eq!(line(&mut w, "Ann@dawnsong"), [], "not present");
}

/// A tell to a channel goes to the hub as any tell; a teller who is no member is told so.
/// Behaviour: server.network.tell-to-a-channel
#[test]
fn a_tell_to_a_channel_goes_as_any_tell_and_a_non_member_is_told() {
    let (mut w, _) = with_ann();
    let t = w.t;
    let to = addr("oldguard@channel");
    w.fed.tell(&mut w.service, t, 7, "Ann", &to, "hello");
    let id = match w.frames().pop().expect("the tell goes") {
        Message::Tell {
            id, to: told, text, ..
        } => {
            assert_eq!((told, text.as_str()), (to, "hello"));
            id
        }
        other => panic!("{other:?}"),
    };
    w.hub(&Message::TellResult {
        id,
        outcome: Outcome::NotMember,
        message: None,
    });
    let (tell, end) = w.service.ended.pop().expect("it ended");
    assert_eq!(tell, 7);
    assert_eq!(end.ended, Ended::NotMember);
    assert_eq!(
        end.line.as_deref(),
        Some("You are not a member of oldguard@channel.")
    );
}

/// A character younger than the operator asks makes no channel request, and is told why.
/// Behaviour: server.network.too-new-sends-nothing
#[test]
fn a_character_too_new_makes_no_channel_request() {
    let (mut w, ann) = with_ann();
    w.fed
        .set_too_new(std::collections::HashSet::from(["ann".to_owned()]));
    let t = w.t;
    assert_eq!(
        w.fed
            .ask_channel(t, ann, "Ann", ChannelOp::Create, "newbies", None),
        Err(yalain_federation::TOO_NEW)
    );
    assert!(w.frames().is_empty());
}
