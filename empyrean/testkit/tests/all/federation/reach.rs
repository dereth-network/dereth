//! Divergence: V447
//! Tells into a person's Discord: a tell to a linked character that is not online is answered
//! `offline` as ever, and the hub then asks the character's world whether the character would have
//! been shown it (`reach`). The world answers by what it keeps for the character while it is away:
//! its squelches of the sender and of the sender's world, the operator's blocks, and its pace of
//! tells from the sender's world; anyone else reaches it, a stranger included, so the hub carries
//! the tell into the person's direct messages when the world says so, with no setting turned on. A
//! character the world does not have, one online, one off the network and another world's are not
//! reachable, alike; an answer shows nothing and gives no stand-in id; and the person's reply comes
//! back to the teller as a tell from the character.
//! Fixture: three virtual-time worlds through the hub's routing (`super::Net`), the hub carrying
//! tells into Discord with its own registry for persons, and what it hands its Discord thread
//! taken where that thread takes it; a person's sign-in on the site is the registry's, called
//! directly. For one test, a real hub on loopback with its sign-in and broker
//! on, the worlds' links WebSockets, and a stand-in on loopback for Discord's direct messages.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use dereth_network_conformance::hub::{Frame, Link};
use dereth_network_hub::broker::Broker;
use dereth_network_hub::core::DiscordTell;
use dereth_network_hub::discord::{any_address, Discord};
use dereth_network_hub::dms::dm_text;
use dereth_network_hub::dms::Carry;
use dereth_network_hub::registry::{Registration, Registry};
use dereth_network_hub::server::{start_with, HubConfig};
use dereth_network_protocol::{Address, Federation, ReachMessage};
use dereth_protocol::comms::CommunicationHearDirectSpeech;
use empyrean_world::federation::{self, commands, LinkState};
use empyrean_world::managers::player_manager;
use serde_json::{json, Value};

use super::persons::{ask_code, log_off, person};
use super::squelch::squelch;
use super::{seed, Got, Net, Seed, TELL, WEENIE_ERROR};

/// The person's Discord user id.
const NELLY: &str = "80351110224678930";

fn addr(s: &str) -> Address {
    Address::parse(s).expect("an address")
}

fn bob() -> Seed {
    seed("bob", 0x5000_0001, "Bob")
}
fn dan() -> Seed {
    seed("dan", 0x5000_0002, "Dan")
}
fn eve() -> Seed {
    seed("eve", 0x5000_0003, "Eve")
}
fn cy() -> Seed {
    seed("cy", 0x5000_0001, "Cy")
}
fn ann() -> Seed {
    seed("ann", 0x5000_0001, "Ann")
}
fn fay() -> Seed {
    seed("fay", 0x5000_0002, "Fay")
}
fn gus() -> Seed {
    seed("gus", 0x5000_0003, "Gus")
}

/// Bob, Dan and Eve on aa, Cy on cc, and Ann and Fay on bb (with Gus, who never logs in), each in
/// the world on the network; the hub carries tells into Discord. Ann does what `before` says, asks
/// for her link and logs off; then her player opens it on the site, signed in with Discord.
fn away(before: impl FnOnce(&mut Net)) -> Net {
    let mut net = in_the_worlds();
    before(&mut net);
    let code = ask_code(&mut net, "bb", "Ann");
    log_off(&mut net, "bb", "Ann");
    let op = person(&net, NELLY);
    net.registry
        .1
        .redeem_link_code(&op, &code)
        .expect("linked on the site");
    net
}

/// Bob, Dan and Eve on aa, Cy on cc, and Ann and Fay on bb (with Gus, who never logs in), each in
/// the world on the network; the hub carries tells into Discord.
fn in_the_worlds() -> Net {
    let mut net = Net::new(&[
        ("aa", &[bob(), dan(), eve()]),
        ("bb", &[ann(), fay(), gus()]),
        ("cc", &[cy()]),
    ]);
    net.hub.set_discord_tells(true);
    for slug in ["aa", "bb", "cc"] {
        net.link(slug);
    }
    for (slug, s) in [
        ("aa", bob()),
        ("aa", dan()),
        ("aa", eve()),
        ("bb", ann()),
        ("bb", fay()),
        ("cc", cy()),
    ] {
        net.enter(slug, s);
        net.say(slug, s.name, "@dn on");
    }
    net
}

/// `who` on `slug` tells Ann@bb `text`: what the hub handed to Discord, and what `who` was shown.
fn tell_ann(net: &mut Net, slug: &str, who: &str, text: &str) -> (Vec<DiscordTell>, Vec<Got>) {
    let m = net.mark(slug, who);
    let (asked, answered) = (net.reach_asked.len(), net.reach_answers.len());
    net.tell(slug, who, "Ann@bb", text);
    let shown = net.since(slug, who, m);
    assert!(
        shown.len() == 1 && shown[0].kind == WEENIE_ERROR,
        "{who} is answered as for any character not online, whatever the world says to the hub"
    );
    assert_eq!(
        (net.reach_asked.len(), net.reach_answers.len()),
        (asked + 1, answered + 1),
        "bb is asked about {who}'s tell, and answers once"
    );
    (net.hub.take_discord_tells(), shown)
}

/// The text of each tell the hub handed to Discord, as the bot says it in the person's direct
/// messages.
fn said(tells: &[DiscordTell]) -> Vec<String> {
    tells
        .iter()
        .map(|t| dm_text(&t.from, &t.to, &t.text))
        .collect()
}

/// The hub hands bb `frame`, as a hub that writes its own frames would, and the network carries
/// what follows: bb's reach answers since.
fn ask_bb(net: &mut Net, frame: &str) -> Vec<ReachMessage> {
    let mark = net.reach_answers.len();
    federation::on_link_frame(&mut net.world("bb").ts.world, frame);
    net.pump();
    net.reach_answers[mark..]
        .iter()
        .map(|(_, m)| m.clone())
        .collect()
}

fn question(id: &str, from: &str, to: &str) -> String {
    ReachMessage::Ask {
        id: id.into(),
        from: addr(from),
        to: addr(to),
    }
    .to_json()
}

fn answer(id: &str, reachable: bool) -> ReachMessage {
    ReachMessage::Result {
        id: id.into(),
        reachable,
    }
}

/// Divergence: V447
#[test]
fn a_tell_to_a_character_away_goes_to_its_persons_discord_once_its_world_says_so_and_a_reply_comes_back(
) {
    let mut net = away(|_| {});
    let (tells, _) = tell_ann(&mut net, "aa", "Bob", "still playing?");
    let (world, asked) = net.reach_asked.last().expect("a question").clone();
    assert!(
        world == "bb"
            && matches!(&asked, ReachMessage::Ask { from, to, .. }
                if from.same(&addr("Bob@aa")) && to.same(&addr("Ann@bb"))),
        "the character's own world is asked, about the tell: {world} {asked:?}"
    );
    let ReachMessage::Ask { id, .. } = &asked else {
        unreachable!()
    };
    assert_eq!(
        net.reach_answers.last(),
        Some(&("bb".to_owned(), answer(id, true))),
        "Ann, away, would have been shown it"
    );
    assert_eq!(
        said(&tells),
        ["Bob@aa tells Ann@bb, \"still playing?\"\n-# Reply to answer as Ann@bb."]
    );
    assert!(
        net.world("bb")
            .ts
            .world
            .federation
            .store
            .stand_ins
            .get(&addr("Bob@aa"))
            .is_none(),
        "the answer gives the teller no stand-in id"
    );

    // The person answers from Discord: the reply reaches Bob as a tell from Ann@bb while she is
    // away, and Bob may answer it as any tell.
    let m = net.mark("aa", "Bob");
    let now = net.now();
    let (outs, carry) = net.hub.tell_from_discord(
        &net.registry,
        &tells[0].person,
        &addr("Ann@bb"),
        &addr("Bob@aa"),
        "yes! come say hi Sat",
        now,
    );
    net.route(outs);
    net.advance(0.3);
    assert_eq!(carry, Carry::Carried);
    let heard: Vec<CommunicationHearDirectSpeech> = net
        .since("aa", "Bob", m)
        .iter()
        .filter(|g| g.kind == TELL)
        .map(|g| g.decode::<CommunicationHearDirectSpeech>())
        .collect();
    assert!(
        heard.len() == 1
            && heard[0].sender_name == "Ann@bb"
            && heard[0].message == "yes! come say hi Sat",
        "{heard:?}"
    );
    let (again, _) = tell_ann(&mut net, "aa", "Bob", "see you then");
    assert_eq!(
        said(&again),
        ["Bob@aa tells Ann@bb, \"see you then\"\n-# Reply to answer as Ann@bb."]
    );
}

/// `who` on bb tells Ann by `name`: what the hub handed to Discord. `who` is answered as for any
/// character not online, and bb is asked about the tell and answers once.
fn tell_ann_at_home(net: &mut Net, who: &str, name: &str, text: &str) -> Vec<DiscordTell> {
    let m = net.mark("bb", who);
    let (asked, answered) = (net.reach_asked.len(), net.reach_answers.len());
    net.tell("bb", who, name, text);
    let shown = net.since("bb", who, m);
    assert!(
        shown.len() == 1 && shown[0].kind == WEENIE_ERROR,
        "{who} is answered as for any character not online: {} shown",
        shown.len()
    );
    assert_eq!(
        (net.reach_asked.len(), net.reach_answers.len()),
        (asked + 1, answered + 1),
        "bb is asked about {who}'s tell to {name}, and answers once"
    );
    net.hub.take_discord_tells()
}

/// Divergence: V447
#[test]
fn a_tell_by_name_from_the_characters_own_world_reaches_its_persons_discord_and_the_reply_comes_back(
) {
    let mut net = away(|_| {});
    let tells = tell_ann_at_home(&mut net, "Fay", "Ann", "lunch?");
    assert_eq!(
        said(&tells),
        ["Fay@bb tells Ann@bb, \"lunch?\"\n-# Reply to answer as Ann@bb."],
        "a tell by her name from Fay, on Ann's own world"
    );
    for name in ["Ann@bb", "Ann (Discord)"] {
        let again = tell_ann_at_home(&mut net, "Fay", name, "lunch?");
        assert_eq!(said(&again).len(), 1, "and by {name}");
    }

    // The person answers from Discord: the reply reaches Fay as a tell from Ann@bb while Ann is
    // away.
    let m = net.mark("bb", "Fay");
    let now = net.now();
    let (outs, carry) = net.hub.tell_from_discord(
        &net.registry,
        &tells[0].person,
        &addr("Ann@bb"),
        &addr("Fay@bb"),
        "on my way",
        now,
    );
    net.route(outs);
    net.advance(0.3);
    assert_eq!(carry, Carry::Carried);
    let heard: Vec<CommunicationHearDirectSpeech> = net
        .since("bb", "Fay", m)
        .iter()
        .filter(|g| g.kind == TELL)
        .map(|g| g.decode::<CommunicationHearDirectSpeech>())
        .collect();
    assert!(
        heard.len() == 1
            && heard[0].sender_name == "Ann@bb"
            && heard[0].message == "on my way"
            && dereth_network_protocol::standin::is_stand_in(heard[0].sender_id.0),
        "{heard:?}"
    );
}

/// Divergence: V447
#[test]
fn a_tell_from_the_characters_own_world_is_held_back_by_the_squelches_it_kept_as_it_left() {
    let mut net = away(|net| squelch(net, "bb", "Ann", "Fay", true));
    let tells = tell_ann_at_home(&mut net, "Fay", "Ann", "are you ignoring me?");
    assert!(
        tells.is_empty(),
        "a teller Ann squelched: {:?}",
        said(&tells)
    );
    assert!(
        matches!(
            net.reach_answers.last(),
            Some((w, ReachMessage::Result { reachable: false, .. })) if w == "bb"
        ),
        "answered not reachable"
    );
    let (tells, _) = tell_ann(&mut net, "aa", "Bob", "and from aa?");
    assert_eq!(
        said(&tells).len(),
        1,
        "a teller of another world she does not squelch"
    );

    // A reply from Discord to a teller who squelches the character is not shown.
    let mut net = away(|_| {});
    let tells = tell_ann_at_home(&mut net, "Fay", "Ann", "hi");
    squelch(&mut net, "bb", "Fay", "Ann", true);
    let m = net.mark("bb", "Fay");
    let now = net.now();
    let (outs, _) = net.hub.tell_from_discord(
        &net.registry,
        &tells[0].person,
        &addr("Ann@bb"),
        &addr("Fay@bb"),
        "hello?",
        now,
    );
    net.route(outs);
    net.advance(0.3);
    assert!(
        !net.since("bb", "Fay", m).iter().any(|g| g.kind == TELL),
        "Fay squelches Ann"
    );
}

/// Divergence: V447
#[test]
fn a_tell_by_name_to_a_character_online_unlinked_or_from_a_sender_off_the_network_stays_the_worlds_own(
) {
    let mut net = in_the_worlds();
    let asked = net.reach_asked.len();
    let m = net.mark("bb", "Ann");
    net.tell("bb", "Fay", "Ann", "hi");
    let heard: Vec<CommunicationHearDirectSpeech> = net
        .since("bb", "Ann", m)
        .iter()
        .filter(|g| g.kind == TELL)
        .map(|g| g.decode::<CommunicationHearDirectSpeech>())
        .collect();
    assert!(
        heard.len() == 1 && heard[0].sender_name == "Fay" && heard[0].sender_id.0 == fay().guid,
        "Ann online is told by Fay as by any local tell: {heard:?}"
    );
    log_off(&mut net, "bb", "Ann");
    let m = net.mark("bb", "Fay");
    net.tell("bb", "Fay", "Ann", "hi");
    let shown = net.since("bb", "Fay", m);
    assert!(
        shown.len() == 1 && shown[0].kind == WEENIE_ERROR,
        "Ann away and never linked: Character not available, as ever"
    );
    assert_eq!(net.reach_asked.len(), asked, "and no question is asked");

    // Linked and away, told by a character off the network: the world's own tell.
    let mut net = away(|_| {});
    net.say("bb", "Fay", "@dn off");
    let asked = net.reach_asked.len();
    let m = net.mark("bb", "Fay");
    net.tell("bb", "Fay", "Ann", "hi");
    let shown = net.since("bb", "Fay", m);
    assert!(
        shown.len() == 1 && shown[0].kind == WEENIE_ERROR && net.reach_asked.len() == asked,
        "a sender off the network keeps the world's own tell"
    );
}

/// Divergence: V447
#[test]
fn a_teller_or_a_world_the_character_squelches_does_not_reach_its_persons_discord() {
    let mut net = away(|net| {
        squelch(net, "bb", "Ann", "Bob@aa", true);
        net.say("bb", "Ann", "@dn squelch cc");
    });
    let (tells, _) = tell_ann(&mut net, "aa", "Bob", "are you ignoring me?");
    assert!(tells.is_empty(), "a squelched teller: {:?}", said(&tells));
    let (tells, _) = tell_ann(&mut net, "cc", "Cy", "hello from cc");
    assert!(
        tells.is_empty(),
        "a teller of a squelched world: {:?}",
        said(&tells)
    );
    assert!(
        net.reach_answers.iter().all(|(_, m)| matches!(
            m,
            ReachMessage::Result {
                reachable: false,
                ..
            }
        )),
        "both answered not reachable"
    );
    let (tells, _) = tell_ann(&mut net, "aa", "Dan", "and from me?");
    assert_eq!(
        said(&tells),
        ["Dan@aa tells Ann@bb, \"and from me?\"\n-# Reply to answer as Ann@bb."],
        "a teller she does not squelch, of a world she does not squelch"
    );
}

/// Divergence: V447
#[test]
fn anyone_reaches_a_linked_characters_persons_discord_a_stranger_included() {
    let mut net = away(|_| {});
    let (tells, _) = tell_ann(&mut net, "aa", "Eve", "hi stranger");
    assert_eq!(
        said(&tells),
        ["Eve@aa tells Ann@bb, \"hi stranger\"\n-# Reply to answer as Ann@bb."],
        "a stranger, whom Ann does not list, with no setting turned on"
    );
    let (tells, _) = tell_ann(&mut net, "cc", "Cy", "it's Cy");
    assert_eq!(said(&tells).len(), 1, "from another world too");
}

/// Divergence: V447
#[test]
fn a_world_or_teller_the_operator_blocks_does_not_reach_a_characters_persons_discord() {
    let mut net = away(|_| {});
    let admin = |net: &mut Net, args: &[&str]| {
        let args: Vec<String> = args.iter().map(|s| (*s).to_owned()).collect();
        commands::admin(&mut net.world("bb").ts.world, &args)
    };
    assert_eq!(
        admin(&mut net, &["block", "aa"]),
        "aa is blocked for everyone on this world."
    );
    let (tells, _) = tell_ann(&mut net, "aa", "Bob", "hello?");
    assert!(tells.is_empty(), "a blocked world: {:?}", said(&tells));
    let (tells, _) = tell_ann(&mut net, "cc", "Cy", "hello from cc");
    assert_eq!(said(&tells).len(), 1, "a world not blocked");
    admin(&mut net, &["unblock", "aa"]);
    admin(&mut net, &["block", "Bob@aa"]);
    let (tells, _) = tell_ann(&mut net, "aa", "Bob", "hello again?");
    assert!(tells.is_empty(), "a blocked teller: {:?}", said(&tells));
    let (tells, _) = tell_ann(&mut net, "aa", "Dan", "and me?");
    assert_eq!(
        said(&tells).len(),
        1,
        "a teller not blocked, of the same world"
    );
}

/// Divergence: V447
#[test]
fn a_character_the_world_lacks_one_online_one_off_the_network_and_another_worlds_are_not_reachable_alike(
) {
    let mut net = away(|_| {});
    let asked = [
        ("q1", "Bob@aa", "Ann@bb", true),
        ("q2", "Bob@aa", "Zed@bb", false),
        ("q3", "Bob@aa", "Fay@bb", false),
        ("q4", "Bob@aa", "Gus@bb", false),
        ("q5", "Bob@aa", "Ann@cc", false),
        ("q6", "Ann@bb", "Ann@bb", false),
    ];
    for (id, from, to, reachable) in asked {
        assert_eq!(
            ask_bb(&mut net, &question(id, from, to)),
            [answer(id, reachable)],
            "{from} to {to}"
        );
    }
    // Fay away, on the network, never having asked for a link: the hub asks about an earlier
    // character of her name.
    log_off(&mut net, "bb", "Fay");
    assert_eq!(
        ask_bb(&mut net, &question("q3b", "Bob@aa", "Fay@bb")),
        [answer("q3b", false)]
    );
    // Ann waiting to be deleted, and then not.
    let waiting = |net: &mut Net, delete_time: u64| {
        let shard = &mut net.world("bb").ts.shard();
        let mut stub = shard.get_character_stub_by_guid(ann().guid).expect("Ann");
        stub.delete_time = delete_time;
        assert!(shard.save_character(&stub));
    };
    waiting(&mut net, 1);
    assert_eq!(
        ask_bb(&mut net, &question("q1d", "Bob@aa", "Ann@bb")),
        [answer("q1d", false)]
    );
    waiting(&mut net, 0);
    assert_eq!(
        ask_bb(&mut net, &question("q1e", "Bob@aa", "Ann@bb")),
        [answer("q1e", true)]
    );
    let bb = &net.world("bb").ts.world;
    assert!(
        bb.federation.store.stand_ins.get(&addr("Bob@aa")).is_none()
            && bb.federation.answers_owed() == 0,
        "nothing given, nothing left owed"
    );
    // Malformed questions, and an answer, which only a world sends, are not answered; the link
    // carries on.
    for frame in [
        r#"{"type":"reach","id":"q7","from":"Bob@aa","to":"oldguard@channel"}"#,
        r#"{"type":"reach","id":"","from":"Bob@aa","to":"Ann@bb"}"#,
        r#"{"type":"reach.result","id":"q9","reachable":true}"#,
    ] {
        assert!(ask_bb(&mut net, frame).is_empty(), "{frame}");
    }
    assert_eq!(
        ask_bb(&mut net, &question("q10", "Bob@aa", "Ann@bb")),
        [answer("q10", true)],
        "and the next question is answered"
    );
}

/// Divergence: V447
#[test]
fn a_yes_is_held_to_the_worlds_pace_of_tells_to_the_character_and_answers_owed_are_bounded_and_never_queued(
) {
    let mut net = away(|_| {});
    // Twenty yeses at once, the pace's burst of tells from one world to one character; the rest
    // are no until it refills, and a tell from another world is not held to aa's.
    let mut yes = 0;
    for i in 0..25 {
        let id = format!("p{i}");
        let got = ask_bb(&mut net, &question(&id, "Bob@aa", "Ann@bb"));
        if got == [answer(&id, true)] {
            yes += 1;
        } else {
            assert_eq!(got, [answer(&id, false)]);
        }
    }
    assert_eq!(yes, 20);
    assert_eq!(
        ask_bb(&mut net, &question("p-cc", "Cy@cc", "Ann@bb")),
        [answer("p-cc", true)]
    );
    net.advance(1.1);
    assert_eq!(
        ask_bb(&mut net, &question("p-later", "Bob@aa", "Ann@bb")),
        [answer("p-later", true)],
        "the pace refills"
    );

    // At most a world's share of answers is owed at once; the rest are not answered.
    let bb = &mut net.world("bb").ts.world;
    for i in 0..150 {
        federation::on_link_frame(bb, &question(&format!("o{i}"), "Dan@aa", "Zed@bb"));
    }
    assert_eq!(
        bb.federation.answers_owed(),
        federation::MAX_ANSWERS_PER_WORLD
    );
    let mark = net.reach_answers.len();
    net.advance(3.0);
    assert_eq!(
        net.reach_answers.len() - mark,
        federation::MAX_ANSWERS_PER_WORLD
    );

    // Answers owed when the link goes are dropped, not sent on the next one.
    let bb = &mut net.world("bb").ts.world;
    for i in 0..5 {
        federation::on_link_frame(bb, &question(&format!("d{i}"), "Dan@aa", "Zed@bb"));
    }
    assert_eq!(bb.federation.answers_owed(), 5);
    net.unlink("bb");
    let mark = net.reach_answers.len();
    net.link("bb");
    assert_eq!(net.world("bb").ts.world.federation.answers_owed(), 0);
    assert_eq!(net.reach_answers.len(), mark, "none is sent after");
}

/// Discord's API for the bot's direct messages, on loopback: a user's direct messages opened,
/// the bot's lines said there, and their lines read after a line, as the hub's broker calls them.
/// Everything else is `404`.
#[derive(Debug, Default)]
struct DirectMessages {
    next: u64,
    /// By channel: the user, and the lines, oldest first.
    channels: BTreeMap<String, (String, Vec<Value>)>,
}

impl DirectMessages {
    fn id(&mut self) -> String {
        self.next += 1;
        (1_500_000_000_000_000_000 + self.next).to_string()
    }

    /// The bot's lines in `user`'s direct messages.
    fn said_to(&self, user: &str) -> Vec<String> {
        self.channels
            .values()
            .filter(|(u, _)| u == user)
            .flat_map(|(_, lines)| lines.iter())
            .filter(|l| l["author"]["bot"] == true)
            .filter_map(|l| l["content"].as_str().map(str::to_owned))
            .collect()
    }

    /// `user` writes `content` in its direct messages with the bot.
    fn writes(&mut self, user: &str, content: &str) {
        let id = self.id();
        let (channel, (_, lines)) = self
            .channels
            .iter_mut()
            .find(|(_, (u, _))| u == user)
            .expect("the direct messages are open");
        lines.push(json!({
            "id": id, "type": 0, "channel_id": channel, "content": content,
            "author": { "id": user, "username": "nelly", "global_name": "Nelly" },
            "mentions": [], "attachments": [],
        }));
    }

    fn answer(&mut self, method: &str, path: &str, body: &Value) -> (u16, Value) {
        let path = path.strip_prefix("/api").unwrap_or("");
        let (route, query) = path.split_once('?').unwrap_or((path, ""));
        let segs: Vec<&str> = route.trim_matches('/').split('/').collect();
        match (method, segs.as_slice()) {
            ("POST", ["users", "@me", "channels"]) => {
                let user = body["recipient_id"].as_str().unwrap_or("").to_owned();
                let open = self
                    .channels
                    .iter()
                    .find(|(_, (u, _))| *u == user)
                    .map(|(c, _)| c.clone());
                let channel = match open {
                    Some(c) => c,
                    None => {
                        let c = self.id();
                        self.channels.insert(c.clone(), (user.clone(), Vec::new()));
                        c
                    }
                };
                (
                    200,
                    json!({ "id": channel, "type": 1, "recipients": [{ "id": user }] }),
                )
            }
            ("POST", ["channels", channel, "messages"]) if self.channels.contains_key(*channel) => {
                let id = self.id();
                let line = json!({
                    "id": id, "type": 0, "channel_id": channel, "content": body["content"],
                    "author": { "id": "1", "username": "Dereth Network", "bot": true },
                });
                if let Some((_, lines)) = self.channels.get_mut(*channel) {
                    lines.push(line.clone());
                }
                (200, line)
            }
            ("GET", ["channels", channel, "messages"]) if self.channels.contains_key(*channel) => {
                let after: u64 = query
                    .split('&')
                    .find_map(|kv| kv.strip_prefix("after="))
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                let lines: Vec<Value> = self.channels[*channel]
                    .1
                    .iter()
                    .filter(|l| {
                        l["id"]
                            .as_str()
                            .and_then(|i| i.parse::<u64>().ok())
                            .is_some_and(|i| i > after)
                    })
                    .cloned()
                    .collect();
                (200, Value::Array(lines))
            }
            _ => (404, json!({ "message": "404: Not Found", "code": 0 })),
        }
    }
}

/// Serves [`DirectMessages`] on a port of loopback until `stop`: Discord's API address for it.
fn serve_direct_messages(dms: Arc<Mutex<DirectMessages>>, stop: Arc<AtomicBool>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    listener.set_nonblocking(true).expect("nonblocking");
    let api = format!("http://{}/api", listener.local_addr().expect("an address"));
    std::thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) {
            let Ok((mut s, _)) = listener.accept() else {
                std::thread::sleep(Duration::from_millis(5));
                continue;
            };
            let _ = s.set_nonblocking(false);
            let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
            let mut got = Vec::new();
            let mut b = [0u8; 4096];
            let head_end = loop {
                let Ok(n) = s.read(&mut b) else { break None };
                if n == 0 {
                    break None;
                }
                got.extend_from_slice(&b[..n]);
                if let Some(i) = got.windows(4).position(|w| w == b"\r\n\r\n") {
                    break Some(i + 4);
                }
            };
            let Some(head_end) = head_end else {
                continue;
            };
            let head = String::from_utf8_lossy(&got[..head_end]).into_owned();
            let length: usize = head
                .lines()
                .filter_map(|l| l.split_once(':'))
                .find(|(k, _)| k.trim().eq_ignore_ascii_case("content-length"))
                .and_then(|(_, v)| v.trim().parse().ok())
                .unwrap_or(0);
            while got.len() < head_end + length {
                match s.read(&mut b) {
                    Ok(n) if n > 0 => got.extend_from_slice(&b[..n]),
                    _ => break,
                }
            }
            let mut first = head.split(' ');
            let (method, path) = (first.next().unwrap_or(""), first.next().unwrap_or(""));
            let body = serde_json::from_slice(&got[head_end..]).unwrap_or(Value::Null);
            let (status, answer) = dms
                .lock()
                .expect("the stand-in")
                .answer(method, path, &body);
            let text = answer.to_string();
            let _ = write!(
                s,
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
                text.len()
            );
        }
    });
    api
}

/// The worlds' links to a real hub: each world's frames go to the hub over its WebSocket, and the
/// hub's come back, the reach messages among them.
fn bridge(net: &mut Net, links: &mut [(&'static str, Link)]) {
    for (slug, link) in links.iter_mut() {
        let w = &mut net.world(slug).ts.world;
        for f in federation::take_frames(w) {
            link.send_raw(&f).expect("sent");
        }
        while let Ok(Some(frame)) = link.recv_frame(Duration::from_millis(5)) {
            let text = match frame {
                Frame::Link(m) => m.to_json(),
                Frame::Directory(m) => m.to_json(),
            };
            federation::on_link_frame(w, &text);
        }
        for m in link.take_reach() {
            federation::on_link_frame(w, &m.to_json());
        }
    }
}

/// Lets the worlds' time pass and carries their frames until `done` holds, at most 20 seconds.
fn until(
    net: &mut Net,
    links: &mut [(&'static str, Link)],
    what: &str,
    mut done: impl FnMut(&mut Net) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        for w in &mut net.worlds {
            w.ts.advance(0.05);
            federation::tick(&mut w.ts.world);
        }
        bridge(net, links);
        if done(net) {
            return;
        }
        assert!(Instant::now() < deadline, "{what}: not within 20 seconds");
    }
}

/// Divergence: V447
#[test]
fn a_tell_to_a_character_away_reaches_its_persons_direct_messages_through_a_real_hub_and_the_reply_comes_back(
) {
    let dms = Arc::new(Mutex::new(DirectMessages::default()));
    let stop = Arc::new(AtomicBool::new(false));
    let api = serve_direct_messages(Arc::clone(&dms), Arc::clone(&stop));
    // A hub that signs persons in and brokers Discord, so that it carries tells there. Nothing
    // here calls Discord: the person is recorded as a sign-in records it, and the bot's calls go to
    // the stand-in.
    let running = start_with(
        HubConfig {
            listen: "127.0.0.1:0".parse().expect("an address"),
            db: ":memory:".into(),
            public_url: Some("https://hub.test".into()),
            site_origins: vec!["https://site.test".into()],
            discord: Some(Discord {
                client_id: "app".into(),
                client_secret: "secret".into(),
                authorize: "https://discord.test/oauth2/authorize".into(),
                api: api.clone(),
                rule: any_address,
            }),
            broker: Some(Broker {
                api,
                poll: Duration::from_millis(100),
                ..Broker::new("bot".into())
            }),
            loopback_allowed: true,
            ..HubConfig::default()
        },
        Registry::open(
            ":memory:",
            "dereth.network",
            Box::new(std::time::SystemTime::now),
        )
        .expect("a registry"),
    )
    .expect("the hub starts");
    let mut keys = Vec::new();
    for slug in ["aa", "bb"] {
        let reg = running.registry();
        let (_, key) = reg
            .register(
                &Registration::from_json(&json!({
                    "slug": slug, "name": format!("World {slug}"), "ruleset": "PvE",
                    "endpoint": { "mode": "manual", "address": "192.0.2.1", "port": 9000 },
                    "emulator": { "family": "Empyrean" },
                }))
                .expect("a registration"),
            )
            .expect("registered");
        reg.set_federation(slug, Federation::On).expect("on");
        keys.push((slug, key));
    }

    // The worlds, linked to the hub over WebSockets; Bob on aa, and Ann on bb asking for her link
    // code before she logs off.
    let mut net = Net::new(&[("aa", &[bob()]), ("bb", &[ann()])]);
    let mut links: Vec<(&'static str, Link)> = keys
        .iter()
        .map(|(slug, key)| {
            let link = Link::open(&format!("ws://{}", running.local_addr()), slug, key)
                .expect("the world links");
            federation::on_link_up(&mut net.world(slug).ts.world);
            (*slug, link)
        })
        .collect();
    until(&mut net, &mut links, "welcomed", |net| {
        ["aa", "bb"]
            .iter()
            .all(|s| net.world(s).ts.world.federation.link() == LinkState::Up)
    });
    net.enter("aa", bob());
    net.enter("bb", ann());
    net.say("aa", "Bob", "@dn on");
    net.say("bb", "Ann", "@dn on");
    let m = net.mark("bb", "Ann");
    net.say("bb", "Ann", "@dn link");
    let mut code = None;
    until(&mut net, &mut links, "a link code", |net| {
        code = Net::lines(&net.since("bb", "Ann", m)).iter().find_map(|l| {
            l.strip_prefix("Open https://site.test/link/#")
                .map(|r| r[..9].to_owned())
        });
        code.is_some()
    });
    let id = net.world("bb").clients["Ann"];
    net.world("bb").ts.client_mut(id).log_off();
    net.advance(12.0);
    until(&mut net, &mut links, "Ann logs off", |net| {
        player_manager::get_online_player_by_name(&net.world("bb").ts.world, "Ann").is_none()
    });
    {
        let reg = running.registry();
        let person = reg.signed_in(NELLY, "Nelly").expect("signed in");
        reg.redeem_link_code(&person, &code.expect("a code"))
            .expect("linked on the site");
    }

    // Bob's tell is answered as for any character not online, and goes to the person's direct
    // messages.
    let m = net.mark("aa", "Bob");
    net.tell("aa", "Bob", "Ann@bb", "still playing?");
    until(&mut net, &mut links, "the direct message", |net| {
        !dms.lock().expect("the stand-in").said_to(NELLY).is_empty()
            && !net.since("aa", "Bob", m).is_empty()
    });
    assert_eq!(
        dms.lock().expect("the stand-in").said_to(NELLY),
        ["Bob@aa tells Ann@bb, \"still playing?\"\n-# Reply to answer as Ann@bb."]
    );
    let shown = net.since("aa", "Bob", m);
    assert!(
        shown.len() == 1 && shown[0].kind == WEENIE_ERROR,
        "Character not available"
    );

    // The person answers there, and Bob hears it from Ann@bb.
    let m = net.mark("aa", "Bob");
    dms.lock()
        .expect("the stand-in")
        .writes(NELLY, "yes! come say hi Sat");
    let mut heard = Vec::new();
    until(&mut net, &mut links, "the reply", |net| {
        heard = net
            .since("aa", "Bob", m)
            .iter()
            .filter(|g| g.kind == TELL)
            .map(|g| g.decode::<CommunicationHearDirectSpeech>())
            .collect();
        !heard.is_empty()
    });
    assert!(
        heard.len() == 1
            && heard[0].sender_name == "Ann@bb"
            && heard[0].message == "yes! come say hi Sat",
        "{heard:?}"
    );
    for (_, link) in links {
        link.close();
    }
    running.shutdown();
    stop.store(true, Ordering::SeqCst);
}
