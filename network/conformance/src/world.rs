//! The world checks: what any world must do, with the suite playing the hub.
//!
//! A world implementation supplies a [`WorldUnderTest`]: a way to carry link frames to and from
//! the world, to let its time pass, and to act as, and look at, two of its characters online. The
//! checks then script the hub's side of each exchange and look at what the world sent and what
//! its characters' clients were shown. Empyrean runs them in process; another implementation can
//! wrap a real world and its clients the same way.

use dereth_network_protocol::directory::{OthersSee, Refusal, Seeable};
use dereth_network_protocol::person::EarlierAlts;
use dereth_network_protocol::standin::is_stand_in;
use dereth_network_protocol::wire::PROTOCOL;
use dereth_network_protocol::{
    Address, DirectoryMessage, ListingOutcome, Message, OffBy, Outcome, PersonFriend, PersonOp,
    PersonOutcome, Profile, WorldInfo,
};

use crate::Check;

/// What a character's client was shown, as far as the checks look.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shown {
    /// `Communication_HearDirectSpeech`: a tell, from `from` under `id`.
    Tell { from: String, id: u32, text: String },
    /// A system line (`Communication_TextboxString`).
    Line(String),
    /// `Character not available`.
    NotAvailable,
    /// The away message (`Communication_WeenieErrorWithString`, AFK).
    Away(String),
    /// A friends row (`Social_FriendsUpdate`): the update type, id, name and online flag.
    FriendRow {
        kind: u32,
        id: u32,
        name: String,
        online: bool,
    },
    /// A room line (`Communication_TurbineChat` room event).
    RoomLine { name: String, text: String, id: u32 },
}

/// A world under test, with two of its characters online: [`WorldUnderTest::characters`].
pub trait WorldUnderTest {
    /// The world's slug.
    fn slug(&self) -> String;
    /// The names of its two characters, both online and not yet on the network.
    fn characters(&self) -> (String, String);
    /// The link comes up (the world should then send `hello`).
    fn link_up(&mut self);
    /// The link goes down.
    fn link_down(&mut self);
    /// Hands the world a frame from the hub.
    fn give(&mut self, m: &Message);
    /// Everything the world has sent the hub since the last call.
    fn take(&mut self) -> Vec<Message>;
    /// Lets `secs` of the world's time pass.
    fn advance(&mut self, secs: f64);
    /// A chat command typed by `who` (`@dn on`, `@dn link`, ...).
    fn command(&mut self, who: &str, line: &str);
    /// `@tell <to>, <text>` typed by `who`.
    fn tell(&mut self, who: &str, to: &str, text: &str);
    /// A reply by id from `who`.
    fn tell_id(&mut self, who: &str, id: u32, text: &str);
    /// `@friends add <name>` by `who`.
    fn add_friend(&mut self, who: &str, name: &str);
    /// `/squelch <name>` by `who`.
    fn squelch(&mut self, who: &str, name: &str);
    /// What `who`'s client was shown since the last call.
    fn shown(&mut self, who: &str) -> Vec<Shown>;
    /// Whether the world carries the Global room (its operator chose a room for it).
    fn carries_global_room(&self) -> bool {
        true
    }
    /// Hands the world one of the directory's frames from the hub. The directory's messages are
    /// a family of their own: [`WorldUnderTest::take`] carries the link's, and
    /// [`WorldUnderTest::take_directory`] these, neither losing the other's.
    fn give_directory(&mut self, m: &DirectoryMessage) {
        let _ = m;
    }
    /// The directory's messages the world has sent the hub since the last call.
    fn take_directory(&mut self) -> Vec<DirectoryMessage> {
        Vec::new()
    }
    /// `who` leaves the world, as a player logging off.
    fn log_out(&mut self, who: &str) {
        let _ = who;
    }
    /// `who` enters the world again.
    fn log_in(&mut self, who: &str) {
        let _ = who;
    }
    /// `who` gains levels: its level is `level` from now on.
    fn set_level(&mut self, who: &str, level: u32) {
        let _ = (who, level);
    }
    /// `who`'s date of birth, age, deaths, chess rank, fishing skill and number of titles are
    /// `values`, and its "Allow others to see your ..." options for them are `allowed`.
    fn set_seeable(&mut self, who: &str, values: Seeable, allowed: OthersSee) {
        let _ = (who, values, allowed);
    }
    /// `vassal` swears allegiance to `patron`, both this world's characters and in the world:
    /// `patron` becomes its patron and monarch, of an allegiance its monarch has not named.
    fn swear(&mut self, vassal: &str, patron: &str) {
        let _ = (vassal, patron);
    }
}

fn welcome(slug: &str, others: &[&str]) -> Message {
    let mut worlds = vec![WorldInfo {
        slug: slug.to_owned(),
        name: slug.to_uppercase(),
        online: true,
    }];
    for o in others {
        worlds.push(WorldInfo {
            slug: (*o).to_owned(),
            name: o.to_uppercase(),
            online: true,
        });
    }
    Message::Welcome {
        proto: PROTOCOL,
        world: slug.to_owned(),
        hub: "conformance suite".into(),
        worlds,
        rooms: vec!["global".into()],
        limits: dereth_network_protocol::limits::Limits::DEFAULT,
    }
}

/// The remote world the checks speak for.
pub const REMOTE: &str = "remote";

fn a(s: &str) -> Address {
    Address::parse(s).expect("the suite's own addresses are valid")
}

/// The page the suite's hub links characters on, and the link its code request is answered with.
const LINK_PAGE: &str = "https://example.org/link/";

/// Brings `w`'s link up and welcomes it, and puts `who` on the network (the listing each sends is
/// left unanswered).
fn ready(w: &mut dyn WorldUnderTest, who: &[&str]) -> Result<(), String> {
    w.link_up();
    w.advance(0.2);
    let first = w.take();
    match first.first() {
        Some(Message::Hello { proto, .. }) if *proto == PROTOCOL => {}
        other => return Err(format!("the first frame was {other:?}, not hello")),
    }
    w.give(&welcome(&w.slug(), &[REMOTE]));
    w.advance(0.2);
    for c in who {
        w.command(c, "@dn on");
    }
    w.advance(0.2);
    let _ = w.take();
    let _ = w.take_directory();
    Ok(())
}

fn answer_of(sent: &[Message], id: &str) -> Option<(Outcome, Option<String>)> {
    sent.iter().find_map(|m| match m {
        Message::TellResult {
            id: i,
            outcome,
            message,
        } if i == id => Some((*outcome, message.clone())),
        _ => None,
    })
}

fn incoming(id: &str, from: &str, to: &str, text: &str, friend: bool) -> Message {
    Message::Tell {
        id: id.into(),
        from: a(from),
        to: a(to),
        text: text.into(),
        friend,
    }
}

/// A person view for `to`: each person friend's handle and the characters it lists.
fn view(to: &str, persons: &[(&str, &[&str])]) -> Message {
    Message::PersonFriends {
        to: a(to),
        linked: true,
        alts: EarlierAlts,
        persons: persons
            .iter()
            .map(|(handle, characters)| PersonFriend {
                handle: a(handle),
                characters: characters.iter().map(|c| a(c)).collect(),
            })
            .collect(),
    }
}

/// The names of the friends rows of `kind` among `shown`.
fn rows_of(shown: &[Shown], kind: u32) -> Vec<String> {
    shown
        .iter()
        .filter_map(|x| match x {
            Shown::FriendRow { kind: k, name, .. } if *k == kind => Some(name.clone()),
            _ => None,
        })
        .collect()
}

/// `who` asks for a link code (`/dn link`), and the hub gives one: a link the hub reports after is
/// the character's own.
fn asks_to_link(w: &mut dyn WorldUnderTest, who: &str) -> Result<(), String> {
    w.command(who, "@dn link");
    w.advance(0.2);
    let sent = w.take();
    let id = sent
        .iter()
        .find_map(|m| match m {
            Message::Person {
                id,
                op: PersonOp::Code,
                ..
            } => Some(id.clone()),
            _ => None,
        })
        .ok_or_else(|| format!("no code asked for: {sent:?}"))?;
    w.give(&Message::PersonResult {
        id,
        outcome: PersonOutcome::Ok,
        code: Some("K7QD-2MXP".into()),
        expires_secs: Some(600),
        url: Some(format!("{LINK_PAGE}#K7QD-2MXP")),
        characters: Vec::new(),
    });
    w.advance(0.2);
    w.shown(who);
    Ok(())
}

/// Whether `sent` holds a watch of `target`, on or off.
fn watches(sent: &[Message], target: &str, on: bool) -> bool {
    sent.iter().any(|m| {
        matches!(m, Message::Watch { target: t, on: o, .. } if t.to_string() == target && *o == on)
    })
}

/// Runs every world check. `make` builds a fresh world for each check.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn run(make: &mut dyn FnMut() -> Box<dyn WorldUnderTest>) -> Vec<Check> {
    let mut out = Vec::new();
    let mut check =
        |name: &'static str, f: &mut dyn FnMut(&mut dyn WorldUnderTest) -> Result<(), String>| {
            let mut w = make();
            out.push(Check::new("world", name, f(w.as_mut())));
        };

    check(
        "link.hello_comes_first_then_the_room_and_the_watches",
        &mut |w| {
            let (c, _) = w.characters();
            w.link_up();
            w.advance(0.2);
            let first = w.take();
            if !matches!(first.first(), Some(Message::Hello { proto, .. }) if *proto == PROTOCOL) {
                return Err(format!("first frames {first:?}"));
            }
            w.give(&welcome(&w.slug(), &[REMOTE]));
            w.advance(0.2);
            let joined = w.take();
            let joins = |sent: &[Message]| {
                sent.iter()
                    .any(|m| matches!(m, Message::RoomJoin { room, on: true } if room == "global"))
            };
            if w.carries_global_room() && !joins(&joined) {
                return Err(format!("no room.join after welcome: {joined:?}"));
            }
            w.command(&c, "@dn on");
            w.add_friend(&c, &format!("Ann@{REMOTE}"));
            w.advance(0.2);
            let watched = |sent: &[Message]| {
                sent.iter().any(|m| {
                    matches!(m, Message::Watch { owner, target, on: true }
                        if owner.name() == c && target.to_string() == format!("Ann@{REMOTE}"))
                })
            };
            let sent = w.take();
            if !watched(&sent) {
                return Err(format!("no watch for the new friend: {sent:?}"));
            }
            // A new link: hello alone, then after the welcome the room and the watch again.
            w.link_down();
            w.link_up();
            w.advance(0.2);
            let again = w.take();
            if !matches!(again.as_slice(), [Message::Hello { .. }]) {
                return Err(format!("on the new link, before welcome: {again:?}"));
            }
            w.give(&welcome(&w.slug(), &[REMOTE]));
            w.advance(0.5);
            let sent = w.take();
            if w.carries_global_room() && !joins(&sent) {
                return Err(format!("no room.join after the second welcome: {sent:?}"));
            }
            watched(&sent)
                .then_some(())
                .ok_or_else(|| format!("no watch again after the second welcome: {sent:?}"))
        },
    );
    check(
        "tell.to_a_character_it_does_not_have_is_offline",
        &mut |w| {
            ready(w, &[])?;
            let s = w.slug();
            w.give(&incoming(
                "h1",
                &format!("Ann@{REMOTE}"),
                &format!("Nobody@{s}"),
                "hi",
                false,
            ));
            w.advance(0.3);
            match answer_of(&w.take(), "h1") {
                Some((Outcome::Offline, _)) => Ok(()),
                other => Err(format!("{other:?}")),
            }
        },
    );
    check(
        "tell.to_a_character_off_the_network_is_offline_and_unseen",
        &mut |w| {
            ready(w, &[])?;
            let (c, _) = w.characters();
            let s = w.slug();
            w.give(&incoming(
                "h1",
                &format!("Ann@{REMOTE}"),
                &format!("{c}@{s}"),
                "hi",
                false,
            ));
            w.advance(0.3);
            let answered = answer_of(&w.take(), "h1");
            let shown = w.shown(&c);
            match answered {
                Some((Outcome::Offline, _))
                    if !shown.iter().any(|x| matches!(x, Shown::Tell { .. })) =>
                {
                    Ok(())
                }
                other => Err(format!("{other:?}, shown {shown:?}")),
            }
        },
    );
    check(
        "tell.a_character_on_the_network_is_shown_it_from_a_stand_in_id",
        &mut |w| {
            let (c, _) = w.characters();
            ready(w, &[&c])?;
            w.shown(&c);
            let s = w.slug();
            let from = format!("Ann@{REMOTE}");
            w.give(&incoming(
                "h1",
                &from,
                &format!("{c}@{s}"),
                "long time!",
                false,
            ));
            w.give(&incoming("h2", &from, &format!("{c}@{s}"), "again", false));
            w.advance(0.3);
            let sent = w.take();
            if answer_of(&sent, "h1").map(|x| x.0) != Some(Outcome::Delivered) {
                return Err(format!("answered {:?}", answer_of(&sent, "h1")));
            }
            let tells: Vec<(String, u32, String)> = w
                .shown(&c)
                .into_iter()
                .filter_map(|x| match x {
                    Shown::Tell { from, id, text } => Some((from, id, text)),
                    _ => None,
                })
                .collect();
            match tells.as_slice() {
                [(f1, i1, t1), (_, i2, _)]
                    if *f1 == from && is_stand_in(*i1) && i1 == i2 && t1 == "long time!" =>
                {
                    Ok(())
                }
                other => Err(format!("shown {other:?}")),
            }
        },
    );
    check(
        "tell.a_character_on_the_network_is_reachable_by_anyone",
        &mut |w| {
            let (c, _) = w.characters();
            ready(w, &[&c])?;
            w.shown(&c);
            let s = w.slug();
            w.give(&incoming(
                "h1",
                &format!("Eve@{REMOTE}"),
                &format!("{c}@{s}"),
                "hi",
                false,
            ));
            w.advance(0.3);
            let answered = answer_of(&w.take(), "h1");
            let shown = w.shown(&c);
            match answered {
                Some((Outcome::Delivered, _))
                    if shown.iter().any(|x| matches!(x, Shown::Tell { .. })) =>
                {
                    Ok(())
                }
                other => Err(format!("{other:?}, shown {shown:?}")),
            }
        },
    );
    check(
        "presence.a_character_on_the_network_shows_online_to_anyone_who_watches",
        &mut |w| {
            let (c, _) = w.characters();
            ready(w, &[&c])?;
            let stranger = format!("Eve@{REMOTE}");
            let me = format!("{c}@{}", w.slug());
            let answered = |sent: Vec<Message>| {
                sent.into_iter().find_map(|m| match m {
                    Message::Presence { to, of, online }
                        if to.to_string() == stranger && of.to_string() == me =>
                    {
                        Some(online)
                    }
                    _ => None,
                })
            };
            w.give(&Message::Watch {
                owner: a(&stranger),
                target: a(&me),
                on: true,
            });
            w.advance(0.3);
            let first = answered(w.take());
            if first != Some(true) {
                return Err(format!("to a watcher it does not list: {first:?}"));
            }
            w.command(&c, "@dn off");
            w.advance(0.3);
            let then = answered(w.take());
            (then == Some(false))
                .then_some(())
                .ok_or_else(|| format!("once off the network: {then:?}"))
        },
    );
    check(
        "room.a_line_from_another_world_passes_the_text_rules",
        &mut |w| {
            let (c, _) = w.characters();
            ready(w, &[&c])?;
            w.shown(&c);
            w.give(&Message::RoomLine {
                room: "global".into(),
                from: a(&format!("Ann@{REMOTE}")),
                text: "free <Tell:IIDString:1:Admin>Admin<\\Tell>\ngold".into(),
                via: None,
            });
            w.advance(0.3);
            let shown = w.shown(&c);
            let text = shown.iter().find_map(|x| match x {
                Shown::RoomLine { text, .. } => Some(text.clone()),
                _ => None,
            });
            match text {
                Some(t) if t == "free Admin gold" => Ok(()),
                other if !w.carries_global_room() && other.is_none() => Ok(()),
                other => Err(format!("shown {other:?}")),
            }
        },
    );
    check(
        "tell.the_sender_is_shown_each_outcome_as_a_local_tell_would_show_it",
        &mut |w| {
            let (c, _) = w.characters();
            ready(w, &[&c])?;
            let to = format!("Bob@{REMOTE}");
            let send = |w: &mut dyn WorldUnderTest,
                        outcome: Outcome,
                        message: Option<&str>|
             -> Result<Vec<Shown>, String> {
                w.shown(&c);
                w.tell(&c, &to, "hello?");
                w.advance(0.3);
                let id = w
                    .take()
                    .into_iter()
                    .find_map(|m| match m {
                        Message::Tell { id, .. } => Some(id),
                        _ => None,
                    })
                    .ok_or("no tell sent")?;
                w.give(&Message::TellResult {
                    id,
                    outcome,
                    message: message.map(str::to_owned),
                });
                w.advance(0.3);
                Ok(w.shown(&c))
            };
            let echo = Shown::Line(format!("You tell {to}, \"hello?\""));
            let afk = send(w, Outcome::Afk, Some("brb"))?;
            if !(afk.contains(&echo) && afk.contains(&Shown::Away(format!("{to} is away: brb")))) {
                return Err(format!("afk: {afk:?}"));
            }
            let lines = |shown: &[Shown]| -> Vec<String> {
                shown
                    .iter()
                    .filter_map(|x| match x {
                        Shown::Line(l) => Some(l.clone()),
                        _ => None,
                    })
                    .collect()
            };
            for (outcome, line) in [
                (
                    Outcome::Unavailable,
                    format!("The Dereth Network can't reach {REMOTE} right now."),
                ),
                (
                    Outcome::NotOnNetwork,
                    format!("{REMOTE} isn't on the network."),
                ),
            ] {
                let shown = send(w, outcome, None)?;
                if !(shown.contains(&Shown::NotAvailable) && lines(&shown).contains(&line)) {
                    return Err(format!("{outcome:?}: {shown:?}"));
                }
            }
            let shown = send(w, Outcome::RateLimited, None)?;
            if !lines(&shown).iter().any(|l| l.contains("try again")) {
                return Err(format!("rate_limited: {shown:?}"));
            }
            Ok(())
        },
    );
    check(
        "text.a_remote_line_is_shown_only_after_the_text_rules",
        &mut |w| {
            let (c, _) = w.characters();
            ready(w, &[&c])?;
            w.shown(&c);
            let s = w.slug();
            w.give(&incoming(
                "h1",
                &format!("Ann@{REMOTE}"),
                &format!("{c}@{s}"),
                "go <Tell:IIDString:1:Admin>Admin<\\Tell> \u{1f600}\nnow",
                false,
            ));
            w.advance(0.3);
            let shown = w.shown(&c);
            match shown.iter().find_map(|x| match x {
                Shown::Tell { text, .. } => Some(text.clone()),
                _ => None,
            }) {
                Some(t) if t == "go Admin ? now" => Ok(()),
                other => Err(format!("shown {other:?}")),
            }
        },
    );
    check(
        "tell.a_local_sender_sees_the_echo_only_once_the_hub_answers",
        &mut |w| {
            let (c, _) = w.characters();
            ready(w, &[&c])?;
            w.shown(&c);
            let to = format!("Bob@{REMOTE}");
            w.tell(&c, &to, "still playing?");
            w.advance(0.3);
            let sent = w.take();
            let Some(id) = sent.iter().find_map(|m| match m {
                Message::Tell {
                    id,
                    from,
                    to: t,
                    text,
                    ..
                } if from.name() == c
                    && from.world() == w.slug()
                    && t.to_string() == to
                    && text == "still playing?" =>
                {
                    Some(id.clone())
                }
                _ => None,
            }) else {
                return Err(format!("no tell sent: {sent:?}"));
            };
            if w.shown(&c)
                .iter()
                .any(|x| matches!(x, Shown::Line(l) if l.starts_with("You tell")))
            {
                return Err("the echo came before the answer".into());
            }
            w.give(&Message::TellResult {
                id,
                outcome: Outcome::Delivered,
                message: None,
            });
            w.advance(0.3);
            let want = format!("You tell {to}, \"still playing?\"");
            let shown = w.shown(&c);
            shown
                .iter()
                .any(|x| *x == Shown::Line(want.clone()))
                .then_some(())
                .ok_or_else(|| format!("shown {shown:?}"))
        },
    );
    check(
        "tell.an_offline_answer_is_character_not_available",
        &mut |w| {
            let (c, _) = w.characters();
            ready(w, &[&c])?;
            w.shown(&c);
            w.tell(&c, &format!("Bob@{REMOTE}"), "hi");
            w.advance(0.3);
            let id = w.take().iter().find_map(|m| match m {
                Message::Tell { id, .. } => Some(id.clone()),
                _ => None,
            });
            let Some(id) = id else {
                return Err("no tell sent".into());
            };
            w.give(&Message::TellResult {
                id,
                outcome: Outcome::Offline,
                message: None,
            });
            w.advance(0.3);
            let shown = w.shown(&c);
            shown
                .contains(&Shown::NotAvailable)
                .then_some(())
                .ok_or_else(|| format!("shown {shown:?}"))
        },
    );
    check(
        "tell.a_reply_by_id_goes_to_the_stand_in_ids_identity",
        &mut |w| {
            let (c, _) = w.characters();
            ready(w, &[&c])?;
            w.shown(&c);
            let s = w.slug();
            w.give(&incoming(
                "h1",
                &format!("Ann@{REMOTE}"),
                &format!("{c}@{s}"),
                "hi",
                false,
            ));
            w.advance(0.3);
            let _ = w.take();
            let Some(id) = w.shown(&c).iter().find_map(|x| match x {
                Shown::Tell { id, .. } => Some(*id),
                _ => None,
            }) else {
                return Err("the tell was not shown".into());
            };
            w.tell_id(&c, id, "hello back");
            w.advance(0.3);
            let sent = w.take();
            sent.iter()
            .any(|m| matches!(m, Message::Tell { to, text, .. } if to.to_string() == format!("Ann@{REMOTE}") && text == "hello back"))
            .then_some(())
            .ok_or_else(|| format!("sent {sent:?}"))
        },
    );
    check(
        "tell.by_name_to_an_away_linked_character_of_its_own_goes_to_the_hub",
        &mut |w| {
            let (c, d) = w.characters();
            ready(w, &[&c, &d])?;
            asks_to_link(w, &d)?;
            w.log_out(&d);
            let _ = w.take();
            w.shown(&c);
            let s = w.slug();
            w.tell(&c, &d, "lunch?");
            w.advance(0.3);
            let sent = w.take();
            let Some(id) = sent.iter().find_map(|m| match m {
                Message::Tell {
                    id, from, to, text, ..
                } if from.to_string() == format!("{c}@{s}")
                    && to.to_string() == format!("{d}@{s}")
                    && text == "lunch?" =>
                {
                    Some(id.clone())
                }
                _ => None,
            }) else {
                return Err(format!("no tell to the hub: {sent:?}"));
            };
            w.give(&Message::TellResult {
                id,
                outcome: Outcome::Offline,
                message: None,
            });
            w.advance(0.3);
            let shown = w.shown(&c);
            shown
                .contains(&Shown::NotAvailable)
                .then_some(())
                .ok_or_else(|| format!("shown {shown:?}"))
        },
    );
    check(
        "tell.by_name_to_an_online_character_of_its_own_stays_local",
        &mut |w| {
            let (c, d) = w.characters();
            ready(w, &[&c, &d])?;
            asks_to_link(w, &d)?;
            let _ = w.take();
            w.shown(&d);
            w.tell(&c, &d, "hi");
            w.advance(0.3);
            let sent = w.take();
            if sent.iter().any(|m| matches!(m, Message::Tell { .. })) {
                return Err(format!("a tell went to the hub: {sent:?}"));
            }
            let shown = w.shown(&d);
            shown
                .iter()
                .any(|x| matches!(x, Shown::Tell { from, id, .. } if *from == c && !is_stand_in(*id)))
                .then_some(())
                .ok_or_else(|| format!("shown {shown:?}"))
        },
    );
    check(
        "tell.a_reply_as_an_away_linked_character_of_its_own_is_shown",
        &mut |w| {
            let (c, d) = w.characters();
            ready(w, &[&c, &d])?;
            asks_to_link(w, &d)?;
            w.log_out(&d);
            let _ = w.take();
            w.shown(&c);
            let s = w.slug();
            let from = format!("{d}@{s}");
            w.give(&incoming(
                "h1",
                &from,
                &format!("{c}@{s}"),
                "on my way",
                false,
            ));
            w.advance(0.3);
            let answered = answer_of(&w.take(), "h1").map(|x| x.0);
            let shown = w.shown(&c);
            match answered {
                Some(Outcome::Delivered)
                    if shown.iter().any(|x| {
                        matches!(x, Shown::Tell { from: f, id, text }
                            if *f == from && is_stand_in(*id) && text == "on my way")
                    }) =>
                {
                    Ok(())
                }
                other => Err(format!("{other:?}, shown {shown:?}")),
            }
        },
    );
    check(
        "tell.from_its_own_character_in_the_world_is_offline_and_unseen",
        &mut |w| {
            let (c, d) = w.characters();
            ready(w, &[&c, &d])?;
            w.shown(&c);
            let s = w.slug();
            w.give(&incoming(
                "h1",
                &format!("{d}@{s}"),
                &format!("{c}@{s}"),
                "hi",
                false,
            ));
            w.advance(0.3);
            let answered = answer_of(&w.take(), "h1");
            let shown = w.shown(&c);
            match answered {
                Some((Outcome::Offline, _))
                    if !shown.iter().any(|x| matches!(x, Shown::Tell { .. })) =>
                {
                    Ok(())
                }
                other => Err(format!("{other:?}, shown {shown:?}")),
            }
        },
    );
    check(
        "squelch.a_squelched_sender_is_answered_squelched_and_unseen",
        &mut |w| {
            let (c, _) = w.characters();
            ready(w, &[&c])?;
            w.squelch(&c, &format!("Ann@{REMOTE}"));
            w.shown(&c);
            let s = w.slug();
            w.give(&incoming(
                "h1",
                &format!("Ann@{REMOTE}"),
                &format!("{c}@{s}"),
                "hi",
                false,
            ));
            w.advance(0.3);
            let answered = answer_of(&w.take(), "h1").map(|x| x.0);
            let shown = w.shown(&c);
            match answered {
                Some(Outcome::Squelched)
                    if !shown.iter().any(|x| matches!(x, Shown::Tell { .. })) =>
                {
                    Ok(())
                }
                other => Err(format!("{other:?}, shown {shown:?}")),
            }
        },
    );
    check("presence.a_friend_shows_as_its_home_world_says", &mut |w| {
        let (c, _) = w.characters();
        ready(w, &[&c])?;
        let friend = format!("Ann@{REMOTE}");
        w.add_friend(&c, &friend);
        w.advance(0.2);
        w.shown(&c);
        w.give(&Message::Presence {
            to: a(&format!("{c}@{}", w.slug())),
            of: a(&friend),
            online: true,
        });
        w.advance(0.3);
        let shown = w.shown(&c);
        shown
            .iter()
            .any(|x| matches!(x, Shown::FriendRow { kind: 4, name, online: true, id } if *name == friend && is_stand_in(*id)))
            .then_some(())
            .ok_or_else(|| format!("shown {shown:?}"))
    });
    check(
        "room.a_line_from_another_world_is_shown_to_listeners",
        &mut |w| {
            let (c, _) = w.characters();
            ready(w, &[&c])?;
            w.shown(&c);
            w.give(&Message::RoomLine {
                room: "global".into(),
                from: a(&format!("Ann@{REMOTE}")),
                text: "dance at 8".into(),
                via: None,
            });
            w.advance(0.3);
            let shown = w.shown(&c);
            shown
            .iter()
            .any(|x| matches!(x, Shown::RoomLine { name, text, id } if *name == format!("Ann@{REMOTE}") && text == "dance at 8" && is_stand_in(*id)))
            .then_some(())
            .ok_or_else(|| format!("shown {shown:?}"))
        },
    );
    check(
        "link.a_tell_queued_while_down_goes_after_the_next_welcome",
        &mut |w| {
            let (c, _) = w.characters();
            ready(w, &[&c])?;
            w.link_down();
            w.tell(&c, &format!("Bob@{REMOTE}"), "are you there?");
            w.advance(0.3);
            if w.take().iter().any(|m| matches!(m, Message::Tell { .. })) {
                return Err("sent with no link".into());
            }
            w.link_up();
            w.advance(0.2);
            let first = w.take();
            if !matches!(first.as_slice(), [Message::Hello { .. }]) {
                return Err(format!("on reconnect, before welcome: {first:?}"));
            }
            w.give(&welcome(&w.slug(), &[REMOTE]));
            w.advance(0.2);
            let sent = w.take();
            sent.iter()
                .any(|m| matches!(m, Message::Tell { text, .. } if text == "are you there?"))
                .then_some(())
                .ok_or_else(|| format!("after welcome: {sent:?}"))
        },
    );
    check(
        "person.a_character_on_the_network_asks_for_its_person_view",
        &mut |w| {
            let (c, _) = w.characters();
            let s = w.slug();
            w.link_up();
            w.advance(0.2);
            let _ = w.take();
            w.give(&welcome(&s, &[REMOTE]));
            w.advance(0.2);
            let _ = w.take();
            w.command(&c, "@dn on");
            w.advance(0.2);
            let sent = w.take();
            sent.iter()
                .any(|m| {
                    matches!(m, Message::PersonWatch { who, on: true }
                        if who.name() == c && who.world() == s)
                })
                .then_some(())
                .ok_or_else(|| format!("no person.watch: {sent:?}"))
        },
    );
    check(
        "person.a_link_is_asked_for_the_character_and_shown",
        &mut |w| {
            let (c, _) = w.characters();
            ready(w, &[&c])?;
            let s = w.slug();
            w.shown(&c);
            w.command(&c, "@dn link");
            w.advance(0.2);
            let sent = w.take();
            let Some(id) = sent.iter().find_map(|m| match m {
                Message::Person {
                    id,
                    op: PersonOp::Code,
                    who,
                    ..
                } if who.name() == c && who.world() == s => Some(id.clone()),
                _ => None,
            }) else {
                return Err(format!("no code asked for: {sent:?}"));
            };
            let link = format!("{LINK_PAGE}#K7QD-2MXP");
            w.give(&Message::PersonResult {
                id,
                outcome: PersonOutcome::Ok,
                code: Some("K7QD-2MXP".into()),
                expires_secs: Some(600),
                url: Some(link.clone()),
                characters: Vec::new(),
            });
            w.advance(0.2);
            let shown = w.shown(&c);
            // One line, the link whole in it, a word of its own.
            let lines: Vec<&String> = shown
                .iter()
                .filter_map(|x| match x {
                    Shown::Line(l) => Some(l),
                    _ => None,
                })
                .collect();
            match lines.as_slice() {
                [l] if l.split_whitespace().any(|w| w == link) && !l.contains('\n') => Ok(()),
                other => Err(format!("shown {other:?}")),
            }
        },
    );
    check(
        "person.a_link_holds_only_for_the_character_that_asked_for_its_code",
        &mut |w| {
            let (c, _) = w.characters();
            ready(w, &[&c])?;
            let s = w.slug();
            w.shown(&c);
            let (bob, me) = (format!("Bob@{REMOTE}"), format!("{c}@{s}"));
            w.give(&view(&me, &[(&bob, &[&bob])]));
            w.advance(0.3);
            let listed = rows_of(&w.shown(&c), 1);
            if !listed.is_empty() {
                return Err(format!("listed {listed:?} by a link it never asked for"));
            }
            let sent = w.take();
            sent.iter()
                .any(|m| {
                    matches!(m, Message::Person { op: PersonOp::Unlink, who, .. }
                        if who.name() == c && who.world() == s)
                })
                .then_some(())
                .ok_or_else(|| format!("not unlinked: {sent:?}"))
        },
    );
    check(
        "person.a_person_friends_characters_are_listed_and_watched",
        &mut |w| {
            let (c, _) = w.characters();
            ready(w, &[&c])?;
            asks_to_link(w, &c)?;
            let s = w.slug();
            w.shown(&c);
            let (bob, alt) = (format!("Bob@{REMOTE}"), format!("Alt@{REMOTE}"));
            let (me, local) = (format!("{c}@{s}"), format!("Local@{s}"));
            w.give(&view(&me, &[(&bob, &[&bob, &alt, &me, &local])]));
            w.advance(0.3);
            let added = rows_of(&w.shown(&c), 1);
            if added != [bob.clone(), alt.clone()] {
                return Err(format!("rows {added:?}"));
            }
            let sent = w.take();
            (watches(&sent, &bob, true) && watches(&sent, &alt, true))
                .then_some(())
                .ok_or_else(|| format!("not both watched: {sent:?}"))
        },
    );
    check(
        "person.a_listed_character_comes_online_as_its_person",
        &mut |w| {
            let (c, _) = w.characters();
            ready(w, &[&c])?;
            asks_to_link(w, &c)?;
            let s = w.slug();
            let (bob, alt, me) = (
                format!("Bob@{REMOTE}"),
                format!("Alt@{REMOTE}"),
                format!("{c}@{s}"),
            );
            w.give(&view(&me, &[(&bob, &[&bob, &alt])]));
            w.advance(0.3);
            let _ = w.take();
            for online in [false, true] {
                w.give(&Message::Presence {
                    to: a(&me),
                    of: a(&alt),
                    online,
                });
                w.advance(0.3);
            }
            let shown = w.shown(&c);
            let want = format!("{bob} is online as ");
            shown
                .iter()
                .any(|x| {
                    matches!(x, Shown::Line(l)
                        if l.starts_with(&want) && l.ends_with(&format!("{alt}<\\Tell>.")))
                })
                .then_some(())
                .ok_or_else(|| format!("shown {shown:?}"))
        },
    );
    check(
        "person.a_linked_characters_friend_is_asked_of_the_hub",
        &mut |w| {
            let (c, _) = w.characters();
            ready(w, &[&c])?;
            let bob = format!("Bob@{REMOTE}");
            w.add_friend(&c, &bob);
            w.advance(0.2);
            let sent = w.take();
            if sent.iter().any(|m| matches!(m, Message::Person { .. })) {
                return Err(format!(
                    "an unlinked character's friend went to the hub: {sent:?}"
                ));
            }
            w.add_friend(&c, &format!("Cat@{REMOTE}"));
            asks_to_link(w, &c)?;
            let s = w.slug();
            w.give(&view(&format!("{c}@{s}"), &[]));
            w.advance(0.3);
            let _ = w.take();
            w.add_friend(&c, &format!("Dee@{REMOTE}"));
            w.advance(0.2);
            let sent = w.take();
            sent.iter()
                .any(|m| {
                    matches!(m, Message::Person { op: PersonOp::Befriend, target: Some(t), .. }
                        if t.to_string() == format!("Dee@{REMOTE}"))
                })
                .then_some(())
                .ok_or_else(|| format!("not asked of the hub: {sent:?}"))
        },
    );
    check(
        "person.linking_carries_the_characters_friends_to_its_person",
        &mut |w| {
            let (c, _) = w.characters();
            ready(w, &[&c])?;
            let bob = format!("Bob@{REMOTE}");
            w.add_friend(&c, &bob);
            w.advance(0.2);
            asks_to_link(w, &c)?;
            let s = w.slug();
            let me = format!("{c}@{s}");
            w.shown(&c);
            w.give(&view(&me, &[]));
            w.advance(0.3);
            let sent = w.take();
            if !sent.iter().any(|m| {
                matches!(m, Message::Person { op: PersonOp::Carry, targets, .. }
                    if targets.iter().map(ToString::to_string).collect::<Vec<_>>() == [bob.clone()])
            }) {
                return Err(format!("not carried: {sent:?}"));
            }
            w.give(&view(&me, &[(&bob, &[&bob])]));
            w.advance(0.3);
            let shown = w.shown(&c);
            let (added, removed) = (rows_of(&shown, 1), rows_of(&shown, 2));
            (added.is_empty() && removed.is_empty())
                .then_some(())
                .ok_or_else(|| format!("listed twice or lost: {shown:?}"))
        },
    );
    check(
        "person.friends_handed_back_on_unlinking_are_the_characters_own",
        &mut |w| {
            let (c, _) = w.characters();
            ready(w, &[&c])?;
            asks_to_link(w, &c)?;
            let s = w.slug();
            let (bob, me) = (format!("Bob@{REMOTE}"), format!("{c}@{s}"));
            w.give(&view(&me, &[(&bob, &[&bob])]));
            w.advance(0.3);
            let _ = w.take();
            w.shown(&c);
            let unlinked = |persons: &[(&str, &[&str])]| match view(&me, persons) {
                Message::PersonFriends { to, persons, .. } => Message::PersonFriends {
                    to,
                    linked: false,
                    alts: EarlierAlts,
                    persons,
                },
                other => other,
            };
            w.give(&unlinked(&[(&bob, &[&bob])]));
            w.advance(0.3);
            w.give(&unlinked(&[]));
            w.advance(0.3);
            let removed = rows_of(&w.shown(&c), 2);
            removed
                .is_empty()
                .then_some(())
                .ok_or_else(|| format!("removed {removed:?}"))
        },
    );
    check(
        "person.a_character_the_view_no_longer_names_is_removed",
        &mut |w| {
            let (c, _) = w.characters();
            ready(w, &[&c])?;
            asks_to_link(w, &c)?;
            let s = w.slug();
            let (bob, alt, me) = (
                format!("Bob@{REMOTE}"),
                format!("Alt@{REMOTE}"),
                format!("{c}@{s}"),
            );
            w.give(&view(&me, &[(&bob, &[&bob, &alt])]));
            w.advance(0.3);
            let _ = w.take();
            w.shown(&c);
            w.give(&view(&me, &[(&bob, &[&bob])]));
            w.advance(0.3);
            let removed = rows_of(&w.shown(&c), 2);
            if removed != [alt.clone()] {
                return Err(format!("removed rows {removed:?}"));
            }
            let sent = w.take();
            watches(&sent, &alt, false)
                .then_some(())
                .ok_or_else(|| format!("still watched: {sent:?}"))
        },
    );
    directory_checks(&mut check);
    out
}

// ---- the directory -------------------------------------------------------------------------------

/// The listings among what the world sent: each as its address, whether it lists, and its
/// profile.
fn listings(sent: &[DirectoryMessage]) -> Vec<(String, String, bool, Option<Profile>)> {
    sent.iter()
        .filter_map(|m| match m {
            DirectoryMessage::Listing {
                id,
                who,
                on,
                profile,
            } => Some((
                id.clone(),
                who.to_string(),
                *on,
                profile.as_deref().cloned(),
            )),
            _ => None,
        })
        .collect()
}

/// The hub's answer to a listing.
fn answer(id: &str, outcome: ListingOutcome, reason: Option<Refusal>) -> DirectoryMessage {
    DirectoryMessage::ListingResult {
        id: id.to_owned(),
        outcome,
        reason,
        url: (outcome == ListingOutcome::Ok)
            .then(|| "https://example.org/directory/character/".to_owned()),
    }
}

/// `who` goes on the network: the listing it sends (its id and profile), not yet answered.
fn list(w: &mut dyn WorldUnderTest, who: &str) -> Result<(String, Profile), String> {
    w.command(who, "@dn on");
    w.advance(0.3);
    let _ = w.take();
    let sent = listings(&w.take_directory());
    let me = format!("{who}@{}", w.slug());
    match sent.as_slice() {
        [(id, a, true, Some(p))] if *a == me => Ok((id.clone(), p.clone())),
        other => Err(format!("going on the network sent {other:?}")),
    }
}

/// `who` goes on the network, and the hub lists it.
fn listed(w: &mut dyn WorldUnderTest, who: &str) -> Result<Profile, String> {
    let (id, p) = list(w, who)?;
    w.give_directory(&answer(&id, ListingOutcome::Ok, None));
    w.advance(0.2);
    let _ = w.take_directory();
    let _ = w.shown(who);
    Ok(p)
}

/// Whether `shown` holds a line starting with `start`.
fn shows(shown: &[Shown], start: &str) -> bool {
    shown
        .iter()
        .any(|x| matches!(x, Shown::Line(l) if l.starts_with(start)))
}

/// The six option-governed values the checks give a character.
const SEEABLE: Seeable = Seeable {
    birth: Some(1_083_412_800),
    age: Some(3_600_000),
    chess: Some(1_400),
    fishing: Some(10),
    deaths: Some(12),
    titles: Some(37),
};

/// The six fields as a profile carries them.
fn seeable_of(p: &Profile) -> Seeable {
    Seeable {
        birth: p.birth,
        age: p.age,
        chess: p.chess,
        fishing: p.fishing,
        deaths: p.deaths,
        titles: p.titles,
    }
}

/// Whether `p` carries the six fields exactly as `allowed` lets others see [`SEEABLE`]: each
/// present only while its option allows, with its value (the age, which grows as the character
/// plays, at least its value).
fn as_allowed(p: &Profile, allowed: OthersSee) -> bool {
    let mut want = Profile::default();
    want.set_seeable(SEEABLE, allowed);
    let (got, want) = (seeable_of(p), seeable_of(&want));
    let age_ok = match (got.age, want.age) {
        (Some(g), Some(w)) => g >= w,
        (g, w) => g == w,
    };
    age_ok && Seeable { age: None, ..got } == Seeable { age: None, ..want }
}

/// The directory's world checks (`docs/network/03-world-behaviour.md` section 10). `check` runs
/// one, by name, on a fresh world.
#[allow(clippy::too_many_lines, clippy::type_complexity)]
fn directory_checks(
    check: &mut dyn FnMut(
        &'static str,
        &mut dyn FnMut(&mut dyn WorldUnderTest) -> Result<(), String>,
    ),
) {
    check("listing.nothing_is_sent_while_off_the_network", &mut |w| {
        let (c, _) = w.characters();
        ready(w, &[])?;
        w.set_level(&c, 20);
        w.advance(400.0);
        w.log_out(&c);
        w.advance(1.0);
        w.log_in(&c);
        w.advance(1.0);
        let sent = w.take_directory();
        sent.is_empty()
            .then_some(())
            .ok_or_else(|| format!("sent {sent:?}"))
    });
    check(
        "listing.going_on_the_network_sends_the_characters_own_profile",
        &mut |w| {
            let (c, _) = w.characters();
            ready(w, &[])?;
            w.set_level(&c, 42);
            let _ = w.shown(&c);
            let (id, p) = list(w, &c)?;
            let me = a(&format!("{c}@{}", w.slug()));
            if p.level != 42 || p.check(&me).is_err() {
                return Err(format!("profile {p:?}"));
            }
            if !shows(
                &w.shown(&c),
                &format!("You are on the Dereth Network as {me}."),
            ) {
                return Err("no line saying it is on the network".into());
            }
            w.give_directory(&answer(&id, ListingOutcome::Ok, None));
            w.advance(0.2);
            let shown = w.shown(&c);
            shows(
                &shown,
                "Your directory page: https://example.org/directory/character/",
            )
            .then_some(())
            .ok_or_else(|| format!("no page shown: {shown:?}"))
        },
    );
    check("listing.a_field_its_option_hides_is_never_sent", &mut |w| {
        let (c, _) = w.characters();
        ready(w, &[])?;
        // Each option's field is checked hidden, then shown, in two halves: one option of each
        // pair is on at a time, so a field following another's option shows.
        let first = OthersSee {
            date_of_birth: true,
            chess_rank: true,
            number_of_deaths: true,
            ..OthersSee::default()
        };
        let second = OthersSee {
            age: true,
            fishing_skill: true,
            number_of_titles: true,
            ..OthersSee::default()
        };
        w.set_seeable(&c, SEEABLE, first);
        let p = listed(w, &c)?;
        if !as_allowed(&p, first) {
            return Err(format!(
                "with the first half shown, sent {:?}",
                seeable_of(&p)
            ));
        }
        w.set_seeable(&c, SEEABLE, second);
        w.advance(400.0);
        let sent = listings(&w.take_directory());
        match sent.as_slice() {
            [(_, _, true, Some(p))] if as_allowed(p, second) => Ok(()),
            other => Err(format!("with the second half shown, sent {other:?}")),
        }
    });
    check(
        "listing.changes_go_at_most_every_five_minutes_and_at_once_at_logout",
        &mut |w| {
            let (c, _) = w.characters();
            ready(w, &[])?;
            w.set_level(&c, 10);
            let _ = listed(w, &c)?;
            w.set_level(&c, 11);
            w.advance(60.0);
            w.set_level(&c, 12);
            w.advance(120.0);
            let early = w.take_directory();
            if !early.is_empty() {
                return Err(format!("within five minutes: {early:?}"));
            }
            w.advance(200.0);
            let sent = listings(&w.take_directory());
            let id = match sent.as_slice() {
                [(id, _, true, Some(p))] if p.level == 12 => id.clone(),
                other => return Err(format!("after five minutes: {other:?}")),
            };
            w.give_directory(&answer(&id, ListingOutcome::Ok, None));
            w.set_level(&c, 13);
            w.advance(1.0);
            if !w.take_directory().is_empty() {
                return Err("a change went within five minutes of the last".into());
            }
            w.log_out(&c);
            w.advance(0.5);
            let sent = listings(&w.take_directory());
            match sent.as_slice() {
                [(_, _, true, Some(p))] if p.level == 13 => Ok(()),
                other => Err(format!("at logout: {other:?}")),
            }
        },
    );
    check(
        "listing.taking_out_goes_at_once_and_is_kept_until_answered",
        &mut |w| {
            let (c, d) = w.characters();
            ready(w, &[])?;
            let s = w.slug();
            let (cc, dd) = (format!("{c}@{s}"), format!("{d}@{s}"));
            let _ = listed(w, &c)?;
            let _ = listed(w, &d)?;
            let out_of = |sent: &[DirectoryMessage], who: &str| {
                listings(sent)
                    .into_iter()
                    .find(|(_, a, on, _)| a == who && !on)
                    .map(|(id, ..)| id)
            };
            w.command(&c, "@dn off");
            w.command(&d, "@dn off");
            w.advance(0.2);
            let sent = w.take_directory();
            if out_of(&sent, &cc).is_none() || out_of(&sent, &dd).is_none() {
                return Err(format!("not taken out at once: {sent:?}"));
            }
            // Unanswered: both go again on the next link, after its welcome.
            w.link_down();
            w.link_up();
            w.advance(0.2);
            let _ = w.take();
            if !w.take_directory().is_empty() {
                return Err("a listing went before the welcome".into());
            }
            w.give(&welcome(&s, &[REMOTE]));
            w.advance(0.3);
            let sent = w.take_directory();
            let (Some(ic), Some(id)) = (out_of(&sent, &cc), out_of(&sent, &dd)) else {
                return Err(format!("not sent again on the next link: {sent:?}"));
            };
            w.give_directory(&answer(&ic, ListingOutcome::Ok, None));
            w.give_directory(&answer(&id, ListingOutcome::Ok, None));
            w.advance(0.2);
            // Answered: never again.
            w.link_down();
            w.link_up();
            w.advance(0.2);
            let _ = w.take();
            w.give(&welcome(&s, &[REMOTE]));
            w.advance(0.3);
            let sent = w.take_directory();
            sent.is_empty()
                .then_some(())
                .ok_or_else(|| format!("sent again once answered: {sent:?}"))
        },
    );
    check(
        "listing.an_off_from_the_hub_holds_the_character_out_until_its_next_login",
        &mut |w| {
            let (c, d) = w.characters();
            ready(w, &[])?;
            let s = w.slug();
            let _ = listed(w, &c)?;
            let _ = listed(w, &d)?;
            w.give_directory(&DirectoryMessage::ListingOff {
                who: a(&format!("{c}@{s}")),
                by: OffBy::Operator,
            });
            w.advance(0.3);
            let sent = listings(&w.take_directory());
            if !sent
                .iter()
                .any(|(_, who, on, _)| *who == format!("{c}@{s}") && !on)
            {
                return Err(format!("not confirmed off: {sent:?}"));
            }
            let taken = "The Dereth Network's operator has taken this character out of the directory for now.";
            if !shows(&w.shown(&c), taken) {
                return Err("the player was not told".into());
            }
            w.set_level(&c, 30);
            w.advance(400.0);
            if !w.take_directory().is_empty() {
                return Err("listed again after the off".into());
            }
            // At its next login it is listed again, and the hub refuses it while it holds it out.
            w.log_out(&c);
            w.advance(1.0);
            let _ = w.take_directory();
            w.log_in(&c);
            w.advance(0.5);
            let sent = listings(&w.take_directory());
            if !sent
                .iter()
                .any(|(_, who, on, _)| *who == format!("{c}@{s}") && *on)
            {
                return Err(format!("not listed again at its next login: {sent:?}"));
            }
            // One away is told at its next login.
            w.log_out(&d);
            w.advance(1.0);
            let _ = w.take_directory();
            w.give_directory(&DirectoryMessage::ListingOff {
                who: a(&format!("{d}@{s}")),
                by: OffBy::Operator,
            });
            w.advance(0.3);
            let sent = listings(&w.take_directory());
            if !sent
                .iter()
                .any(|(_, who, on, _)| *who == format!("{d}@{s}") && !on)
            {
                return Err(format!("not confirmed off while away: {sent:?}"));
            }
            w.log_in(&d);
            w.advance(0.5);
            let shown = w.shown(&d);
            shows(&shown, taken)
                .then_some(())
                .ok_or_else(|| format!("not told at the next login: {shown:?}"))
        },
    );
    check(
        "listing.an_off_by_its_person_takes_the_character_off_the_network",
        &mut |w| {
            let (c, d) = w.characters();
            ready(w, &[])?;
            let s = w.slug();
            let _ = listed(w, &c)?;
            let _ = listed(w, &d)?;
            let forgotten =
                "You asked the Dereth Network to forget you, so this character is off the network now. Type @dn on to join it again.";
            w.give_directory(&DirectoryMessage::ListingOff {
                who: a(&format!("{c}@{s}")),
                by: OffBy::Person,
            });
            w.advance(0.3);
            let sent = listings(&w.take_directory());
            if !sent
                .iter()
                .any(|(_, who, on, _)| *who == format!("{c}@{s}") && !on)
            {
                return Err(format!("not confirmed off: {sent:?}"));
            }
            if !shows(&w.shown(&c), forgotten) {
                return Err("the player was not told".into());
            }
            // Off the network: a tell to it is answered as to any character off it.
            w.give(&incoming(
                "t1",
                &format!("Bob@{REMOTE}"),
                &format!("{c}@{s}"),
                "still there?",
                false,
            ));
            w.advance(0.3);
            match answer_of(&w.take(), "t1") {
                Some((Outcome::Offline, _)) => {}
                other => return Err(format!("a tell to it was answered {other:?}")),
            }
            w.command(&c, "@dn");
            w.advance(0.2);
            if !shows(&w.shown(&c), "You are not on the Dereth Network.") {
                return Err("its status does not say it is off".into());
            }
            // One away is told at its next login, and is off the network then.
            w.log_out(&d);
            w.advance(1.0);
            let _ = w.take_directory();
            w.give_directory(&DirectoryMessage::ListingOff {
                who: a(&format!("{d}@{s}")),
                by: OffBy::Person,
            });
            w.advance(0.3);
            let sent = listings(&w.take_directory());
            if !sent
                .iter()
                .any(|(_, who, on, _)| *who == format!("{d}@{s}") && !on)
            {
                return Err(format!("not confirmed off while away: {sent:?}"));
            }
            w.log_in(&d);
            w.advance(0.5);
            if !w.take_directory().is_empty() {
                return Err("listed at its next login, off the network".into());
            }
            let shown = w.shown(&d);
            shows(&shown, forgotten)
                .then_some(())
                .ok_or_else(|| format!("not told at the next login: {shown:?}"))
        },
    );
    check(
        "listing.only_listed_patrons_and_monarchs_are_named",
        &mut |w| {
            let (c, d) = w.characters();
            ready(w, &[])?;
            let s = w.slug();
            let dd = a(&format!("{d}@{s}"));
            w.set_level(&d, 50);
            w.set_level(&c, 20);
            w.swear(&c, &d);
            w.advance(0.5);
            let p = listed(w, &c)?;
            let Some(al) = p.allegiance else {
                return Err("no allegiance sent".into());
            };
            if al.patron.is_some() || al.monarch.is_some() {
                return Err(format!("an unlisted patron or monarch named: {al:?}"));
            }
            if al.name.is_some() {
                return Err(format!(
                    "an allegiance its monarch did not name sent a name: {al:?}"
                ));
            }
            // The patron lists itself: its vassal's listing names it from then on.
            let _ = listed(w, &d)?;
            w.advance(400.0);
            let sent = listings(&w.take_directory());
            let named = sent.iter().any(|(_, who, on, p)| {
                *who == format!("{c}@{s}")
                    && *on
                    && p.as_ref()
                        .and_then(|p| p.allegiance.as_ref())
                        .is_some_and(|a| {
                            a.patron.as_ref().is_some_and(|x| x.same(&dd))
                                && a.monarch.as_ref().is_some_and(|x| x.same(&dd))
                        })
            });
            if !named {
                return Err(format!("a listed patron not named: {sent:?}"));
            }
            // And leaves the network: the vassal's listing names it no more.
            w.command(&d, "@dn off");
            w.advance(400.0);
            let sent = listings(&w.take_directory());
            let unnamed = sent.iter().any(|(_, who, on, p)| {
                *who == format!("{c}@{s}")
                    && *on
                    && p.as_ref()
                        .and_then(|p| p.allegiance.as_ref())
                        .is_some_and(|a| a.patron.is_none() && a.monarch.is_none())
            });
            unnamed
                .then_some(())
                .ok_or_else(|| format!("an unlisted patron still named: {sent:?}"))
        },
    );
    check(
        "listing.a_refusal_holds_the_character_out_until_its_next_login",
        &mut |w| {
            let (c, d) = w.characters();
            ready(w, &[])?;
            let (id, _) = list(w, &c)?;
            let _ = w.shown(&c);
            w.give_directory(&answer(
                &id,
                ListingOutcome::Refused,
                Some(Refusal::TakenDown),
            ));
            w.advance(0.2);
            if !shows(
            &w.shown(&c),
            "The Dereth Network's operator has taken this character out of the directory for now.",
        ) {
            return Err("taken_down not shown".into());
        }
            let (id, _) = list(w, &d)?;
            let _ = w.shown(&d);
            w.give_directory(&answer(&id, ListingOutcome::Refused, Some(Refusal::Full)));
            w.advance(0.2);
            if !shows(
            &w.shown(&d),
            "The Dereth Network's directory holds as many of this world's characters as it may.",
        ) {
            return Err("full not shown".into());
        }
            w.set_level(&c, 30);
            w.advance(400.0);
            if !w.take_directory().is_empty() {
                return Err("listed again after a refusal".into());
            }
            // Held back: the latest profile goes again a minute later.
            w.command(&c, "@dn off");
            w.advance(0.2);
            let _ = w.take_directory();
            let (id, _) = list(w, &c)?;
            w.give_directory(&answer(&id, ListingOutcome::RateLimited, None));
            w.set_level(&c, 31);
            w.advance(30.0);
            if !w.take_directory().is_empty() {
                return Err("sent again within the minute".into());
            }
            w.advance(40.0);
            let sent = listings(&w.take_directory());
            match sent.as_slice() {
                [(_, _, true, Some(p))] if p.level == 31 => Ok(()),
                other => Err(format!("a minute after rate_limited: {other:?}")),
            }
        },
    );
}
