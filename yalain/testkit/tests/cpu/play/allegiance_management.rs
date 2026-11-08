//! Running an allegiance: the message of the day, officers, members' titles and what examining a
//! member shows of its allegiance, the kingdom restriction, finding a member, and messages to a
//! patron, to vassals and to every follower; and the allegiance's record keeping all of it.

use auberean_protocol::queue;
use auberean_protocol::ServerMessage;
use auberean_script::args::Arg;
use auberean_script::native_object::{NativeObject, TextInfo};
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::allegiance::{self, error, Allegiances, Charter, Node as Member, Restriction};
use yalain_world::allegiance_management::{function as manage, text, ALLEGIANCE, MOST_OFFICERS};
use yalain_world::calls::{self, Node, Value};
use yalain_world::profile;
use yalain_world::social::{NOTICE, REFUSED};

/// A string argument.
fn string_arg(text: &str) -> Arg {
    let mut b = yalain_world::graph::Builder::new();
    b.native(0x76, NativeObject::Text(text.encode_utf16().collect()));
    Arg::Object(b.graph())
}

/// A string reference argument holding a literal, as the client sends typed words.
fn literal_arg(text: &str) -> Arg {
    let mut b = yalain_world::graph::Builder::new();
    b.native(
        0x6A,
        NativeObject::TextInfo(TextInfo {
            has_literal: 1,
            literal: Some(text.encode_utf16().collect()),
            ..TextInfo::default()
        }),
    );
    Arg::Object(b.graph())
}

/// The profiles a client was sent since `from`, each as its root's slots and its graph.
fn profiles(h: &Harness, b: usize, from: usize) -> Vec<Vec<Value>> {
    h.calls(b, from)
        .into_iter()
        .filter(|(f, _)| *f == allegiance::function::PROFILE)
        .map(|(_, v)| v)
        .collect()
}

/// A profile's message of the day, if it carries one.
fn motd_of(v: &[Value]) -> Option<String> {
    let Some(Value::Graph(g)) = v.first() else {
        return None;
    };
    let Some(Node::Script { slots, .. }) = g.first().map(|(_, n)| n) else {
        return None;
    };
    let at = slots[8].1;
    g.iter().find_map(|(i, n)| match n {
        Node::TextInfo(t) if *i == at => t.literal.as_ref().map(|l| String::from_utf16_lossy(l)),
        _ => None,
    })
}

/// The ids flagged as officers in a profile's member data.
fn officers_flagged(v: &[Value]) -> Vec<u64> {
    let Some(Value::Graph(g)) = v.first() else {
        return Vec::new();
    };
    g.iter()
        .filter_map(|(_, n)| match n {
            Node::Script { package, slots } if *package == 0x1B1 && slots[8].1 == 1 => {
                Some(u64::from(slots[10].1) << 32 | u64::from(slots[9].1))
            }
            _ => None,
        })
        .collect()
}

/// A monarch ("Patronus") and its vassal ("Vassalia"), both in the world.
fn sworn_pair() -> (Harness, (usize, u64), (usize, u64)) {
    let mut h = Harness::new();
    let v = h.player_in_world("one", "Vassalia");
    let m = h.player_in_world("two", "Patronus");
    swear(&mut h, v, m);
    (h, m, v)
}

fn swear(h: &mut Harness, (vb, vid): (usize, u64), (pb, pid): (usize, u64)) {
    h.ask(vb, allegiance::function::SWEAR, &[Arg::LongInt(pid)], 10);
    h.ask(
        pb,
        allegiance::function::ACCEPT_VASSAL,
        &[Arg::LongInt(vid)],
        10,
    );
}

/// The monarch sets the message of the day: it is told so, and every member in the world is sent
/// its profile carrying it; asked for, it is shown; a vassal who is no officer, and one in no
/// allegiance, are refused; cleared, profiles carry none and asking says there is none.
/// Behaviour: server.allegiance.message-of-the-day
#[test]
fn the_monarch_sets_the_message_of_the_day_and_every_member_is_sent_it() {
    let (mut h, (m, _), (v, _)) = sworn_pair();
    let (stranger, _) = h.player_in_world("three", "Strangera");
    let (fm, fv) = (h.bots[m].inbox.len(), h.bots[v].inbox.len());
    h.ask(m, manage::SET_MOTD, &[literal_arg("Welcome home")], 10);
    assert!(h.texts(m, fm).contains(&(NOTICE, text::MOTD_SET.0)));
    let vp = profiles(&h, v, fv);
    assert_eq!(vp.len(), 1, "the vassal is sent its profile again");
    assert_eq!(motd_of(&vp[0]).as_deref(), Some("Welcome home"));
    let fv = h.bots[v].inbox.len();
    h.ask(v, manage::SHOW_MOTD, &[], 10);
    assert_eq!(h.texts(v, fv), vec![(ALLEGIANCE, text::MOTD.0)]);
    let fv = h.bots[v].inbox.len();
    h.ask(v, manage::SET_MOTD, &[literal_arg("mine now")], 10);
    assert_eq!(h.texts(v, fv), vec![(REFUSED, text::MONARCH_OR_OFFICER.0)]);
    let fs = h.bots[stranger].inbox.len();
    h.ask(stranger, manage::SET_MOTD, &[literal_arg("hello")], 10);
    assert_eq!(
        h.texts(stranger, fs),
        vec![(REFUSED, text::SET_NOT_IN_ALLEGIANCE.0)]
    );
    let (fm, fv) = (h.bots[m].inbox.len(), h.bots[v].inbox.len());
    h.ask(m, manage::CLEAR_MOTD, &[], 10);
    assert!(h.texts(m, fm).contains(&(NOTICE, text::MOTD_CLEARED.0)));
    assert_eq!(motd_of(&profiles(&h, v, fv)[0]), None, "cleared");
    let fv = h.bots[v].inbox.len();
    h.ask(v, manage::SHOW_MOTD, &[], 10);
    assert_eq!(h.texts(v, fv), vec![(ALLEGIANCE, text::NO_MOTD.0)]);
}

/// A profile's allegiance name.
fn name_of(v: &[Value]) -> Option<TextInfo> {
    let Some(Value::Graph(g)) = v.first() else {
        return None;
    };
    let Some(Node::Script { slots, .. }) = g.first().map(|(_, n)| n) else {
        return None;
    };
    let at = slots[0].1;
    g.iter().find_map(|(i, n)| match n {
        Node::TextInfo(t) if *i == at => Some(t.clone()),
        _ => None,
    })
}

/// An allegiance not named yet is sent as "(monarch)'s Allegiance", as its members' examinations
/// name it; a named one by its name.
/// Behaviour: server.allegiance.unnamed-goes-by-its-monarch
#[test]
fn an_unnamed_allegiances_profile_names_it_by_its_monarch() {
    let (mut h, (m, mid), _) = sworn_pair();
    let from = h.bots[m].inbox.len();
    h.ask(m, allegiance::function::QUERY, &[Arg::LongInt(mid)], 10);
    let name = name_of(&profiles(&h, m, from)[0]).expect("a name");
    assert_eq!(
        (name.table, name.string_id),
        (
            profile::allegiance_line::NAMES_TABLE,
            profile::allegiance_line::UNNAMED
        )
    );
    assert_eq!(name.variables.len(), 1, "the monarch's name");
    assert_eq!(name.literal, None);
    h.server
        .allegiances
        .names
        .insert(mid, "Clan Example".into());
    let from = h.bots[m].inbox.len();
    h.ask(m, allegiance::function::QUERY, &[Arg::LongInt(mid)], 10);
    let name = name_of(&profiles(&h, m, from)[0]).expect("a name");
    assert_eq!(
        name.literal
            .map(|l| String::from_utf16_lossy(&l))
            .as_deref(),
        Some("Clan Example")
    );
}

/// The allegiance's record keeps each allegiance's message of the day, officers and restriction
/// and each member's title; a record written before it kept them still reads, with none.
/// Behaviour: server.allegiance.charter-kept
#[test]
fn the_allegiance_record_keeps_its_charter_and_titles_and_reads_an_older_record() {
    let mut a = Allegiances::default();
    a.nodes.insert(7, Member::default());
    a.nodes.insert(
        8,
        Member {
            patron: 7,
            ..Member::default()
        },
    );
    a.names.insert(7, "Order of the Lamp".into());
    a.charters.insert(
        7,
        Charter {
            motd: Some("Meet at the hall".encode_utf16().collect()),
            officers: [8].into(),
            restriction: Some(Restriction { neutrals: true }),
        },
    );
    a.titles.insert(8, "Keeper".into());
    let back = Allegiances::from_bytes(&a.to_bytes()).expect("reads back");
    assert_eq!(back.charters, a.charters);
    assert_eq!(back.titles, a.titles);
    assert_eq!(back.nodes, a.nodes);
    assert!(back.is_officer(8) && back.is_officer(7));
    // The older record: a version byte of 1, no members, no names.
    let old = [1u8, 0, 0, 0, 0, 0, 0, 0, 0];
    let read = Allegiances::from_bytes(&old).expect("an older record reads");
    assert!(read.charters.is_empty() && read.titles.is_empty());
}

/// The monarch appoints a member an officer by name: told whom and how many of how many, and the
/// member's data in every profile flags it; the officer may then set the message of the day.
/// Revoked, it is told so and the flag goes. A vassal may not appoint, a name outside the
/// allegiance is refused, and past the most officers the monarch is told it has them all.
/// Behaviour: server.allegiance.officers-appointed
#[test]
fn the_monarch_appoints_and_revokes_officers_by_name() {
    let (mut h, (m, mid), (v, vid)) = sworn_pair();
    let fv = h.bots[v].inbox.len();
    h.ask(v, manage::APPOINT, &[string_arg("Patronus")], 10);
    assert_eq!(h.texts(v, fv), vec![(REFUSED, text::MONARCH_ONLY.0)]);
    let fm = h.bots[m].inbox.len();
    h.ask(m, manage::APPOINT, &[string_arg("Nobody")], 10);
    assert_eq!(h.texts(m, fm), vec![(REFUSED, text::NOT_A_MEMBER.0)]);
    let (fm, fv) = (h.bots[m].inbox.len(), h.bots[v].inbox.len());
    h.ask(m, manage::APPOINT, &[string_arg("vassalia")], 10);
    assert!(h.texts(m, fm).contains(&(NOTICE, text::APPOINTED.0)));
    let vp = profiles(&h, v, fv);
    assert_eq!(officers_flagged(&vp[0]), vec![mid, vid], "both flagged");
    let fv = h.bots[v].inbox.len();
    h.ask(v, manage::SET_MOTD, &[literal_arg("from an officer")], 10);
    assert!(h.texts(v, fv).contains(&(NOTICE, text::MOTD_SET.0)));
    let fm = h.bots[m].inbox.len();
    h.ask(m, manage::APPOINT, &[string_arg("Vassalia")], 10);
    assert_eq!(h.texts(m, fm), vec![(REFUSED, text::CANNOT_APPOINT.0)]);
    let (fm, fv) = (h.bots[m].inbox.len(), h.bots[v].inbox.len());
    h.ask(m, manage::REVOKE, &[string_arg("Vassalia")], 10);
    assert!(h.texts(m, fm).contains(&(NOTICE, text::REVOKED.0)));
    assert_eq!(officers_flagged(&profiles(&h, v, fv)[0]), vec![mid]);
    // As many officers as allowed (members not in the world), then one more.
    let extra: Vec<u64> = (0..u64::from(MOST_OFFICERS))
        .map(|i| 0x2130_0000_0200_0000 + i)
        .collect();
    for (i, id) in extra.iter().enumerate() {
        h.server.allegiances.nodes.insert(
            *id,
            Member {
                patron: mid,
                joined: 1.0e9 + i as f64,
                ..Member::default()
            },
        );
    }
    h.server.allegiances.charter_mut(mid).officers = extra.iter().copied().collect();
    let fm = h.bots[m].inbox.len();
    h.ask(m, manage::APPOINT, &[string_arg("Vassalia")], 10);
    assert_eq!(h.texts(m, fm), vec![(REFUSED, text::OFFICERS_FULL.0)]);
}

/// The packed examination of a player another player asks for.
fn examination(h: &mut Harness, by: usize, of: u64) -> Vec<u8> {
    let mut req = Vec::new();
    for w in [0x4000_0002u32, 0] {
        req.extend(w.to_le_bytes());
    }
    req.extend(of.to_le_bytes());
    req.extend(0x203u32.to_le_bytes());
    let mut b = yalain_world::graph::Builder::new();
    b.native(
        profile::REQUEST_TYPE,
        NativeObject::Packed {
            ty: profile::REQUEST_TYPE.into(),
            bytes: req,
        },
    );
    let from = h.bots[by].inbox.len();
    h.bots[by].send(
        true,
        queue::WEENIE,
        &server_call(
            yalain_world::requests::function::EXAMINE,
            &[Arg::Object(b.graph())],
        ),
    );
    h.run(30);
    h.world_messages(by, from)
        .into_iter()
        .find_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == profile::EXAMINATION_PROFILE => {
                let v = calls::values(&c.args?.0).ok()?;
                match v[0].root() {
                    Some(Node::Native { bytes, .. }) => Some(bytes.clone()),
                    _ => None,
                }
            }
            _ => None,
        })
        .expect("an examination")
}

fn holds(bytes: &[u8], word: u32) -> bool {
    bytes.windows(4).any(|w| w == word.to_le_bytes())
}

/// An officer gives a member a title: told so, and examining the member shows its allegiance (its
/// name, its monarch, its rank) and the title after the rank; removed, the title no longer shows.
/// A member who is no officer may not, and examining one in no allegiance shows none of it.
/// Behaviour: server.allegiance.member-titles
#[test]
fn an_officer_gives_a_member_a_title_that_its_examination_shows() {
    let (mut h, (m, _), (v, vid)) = sworn_pair();
    let (stranger, sid) = h.player_in_world("three", "Strangera");
    let before = examination(&mut h, stranger, vid);
    assert!(holds(&before, profile::allegiance_line::ALLEGIANCE));
    assert!(holds(&before, profile::allegiance_line::MONARCH));
    assert!(holds(&before, profile::allegiance_line::RANK));
    assert!(
        !holds(&before, profile::allegiance_line::TITLE),
        "no title yet"
    );
    let none = examination(&mut h, v, sid);
    assert!(
        !holds(&none, profile::allegiance_line::ALLEGIANCE),
        "one in no allegiance shows none"
    );
    let fv = h.bots[v].inbox.len();
    h.ask(
        v,
        manage::SET_TITLE,
        &[string_arg("Archon"), string_arg("Patronus")],
        10,
    );
    assert_eq!(h.texts(v, fv), vec![(REFUSED, text::MONARCH_OR_OFFICER.0)]);
    let fm = h.bots[m].inbox.len();
    h.ask(
        m,
        manage::SET_TITLE,
        &[string_arg("Lamplighter"), string_arg("Vassalia")],
        10,
    );
    assert_eq!(h.texts(m, fm), vec![(NOTICE, text::TITLE_SET.0)]);
    let titled = examination(&mut h, stranger, vid);
    assert!(holds(&titled, profile::allegiance_line::TITLE));
    let view = allegiance::view(&mut h.server, vid).expect("a member");
    assert_eq!(view.title.as_deref(), Some("Lamplighter"));
    let fm = h.bots[m].inbox.len();
    h.ask(m, manage::REMOVE_TITLE, &[string_arg("Vassalia")], 10);
    assert_eq!(h.texts(m, fm), vec![(NOTICE, text::TITLE_REMOVED.0)]);
    let after = examination(&mut h, stranger, vid);
    assert!(!holds(&after, profile::allegiance_line::TITLE), "removed");
}

/// Examining a member names it by its rank's title and ends with its allegiance as retail wrote
/// it: the allegiance, its monarch, its patron when that is not the monarch, its rank and title.
/// Behaviour: server.examination.allegiance-lines
#[test]
fn examining_a_member_ends_with_its_allegiance_as_the_recorded_pages_did() {
    let mut h = Harness::new();
    let v = h.player_in_world("one", "Vassalia");
    let p = h.player_in_world("two", "Patronus");
    let g = h.player_in_world("three", "Novicia");
    swear(&mut h, v, p);
    swear(&mut h, g, v);
    h.server
        .allegiances
        .titles
        .insert(g.1, "Torchbearer".into());
    let bytes = examination(&mut h, p.0, g.1);
    let view = allegiance::view(&mut h.server, g.1).expect("a member");
    assert_eq!(
        view.patron.as_deref(),
        Some("Vassalia [M]"),
        "not the monarch"
    );
    assert_eq!(view.monarch, "Patronus [M]");
    assert!(!view.is_monarch);
    let mut request = Vec::new();
    for w in [0x4000_0002u32, 0] {
        request.extend(w.to_le_bytes());
    }
    request.extend(g.1.to_le_bytes());
    request.extend(0x203u32.to_le_bytes());
    let vitals = yalain_world::examine::Vitals::of_qualities(
        &h.server
            .players
            .values()
            .find(|q| q.character == g.1)
            .expect("in the world")
            .qualities,
    )
    .unwrap_or_default();
    let want = profile::member(
        0x203,
        "Novicia [M]",
        1,
        0x1000,
        1,
        None,
        vitals,
        0,
        Some(&view),
    );
    assert_eq!(bytes, want.packed(&request), "the whole profile");
    assert!(holds(&bytes, profile::allegiance_line::PATRON));
    assert!(holds(&bytes, profile::allegiance_line::TITLED_NAME));
    let mono = examination(&mut h, g.0, p.1);
    assert!(holds(&mono, profile::allegiance_line::IS_MONARCH));
    assert!(!holds(&mono, profile::allegiance_line::PATRON));
}

/// The monarch restricts its allegiance to its kingdom: an oath from a neutral player to any of its
/// members is refused and the player told which kingdom it would have to join; let neutrals in,
/// and it is taken; lifted, told so. Only the monarch may set it.
/// Behaviour: server.allegiance.kingdom-restriction
#[test]
fn a_kingdom_restriction_refuses_an_oath_from_another_kingdom() {
    let (mut h, (m, mid), (v, vid)) = sworn_pair();
    if let Some(q) = h.server.players.values_mut().find(|q| q.character == mid) {
        yalain_world::kingdoms::set_kingdom(&mut q.qualities, 1);
    }
    let fv = h.bots[v].inbox.len();
    h.ask(v, manage::RESTRICT, &[Arg::Int(0), Arg::Int(1)], 10);
    assert_eq!(h.texts(v, fv), vec![(REFUSED, text::MONARCH_ONLY.0)]);
    let fm = h.bots[m].inbox.len();
    h.ask(m, manage::RESTRICT, &[Arg::Int(0), Arg::Int(1)], 10);
    assert_eq!(h.texts(m, fm), vec![(NOTICE, text::RESTRICTED.0)]);
    let n = h.player_in_world("three", "Novicia");
    let fnv = h.bots[n.0].inbox.len();
    h.ask(n.0, allegiance::function::SWEAR, &[Arg::LongInt(vid)], 10);
    let refused = h
        .calls(n.0, fnv)
        .into_iter()
        .find(|(f, _)| *f == allegiance::function::VASSAL_SWORE)
        .expect("the oath answered");
    assert!(
        refused.1.contains(&Value::Int(error::KINGDOM)),
        "{refused:?}"
    );
    assert!(h
        .texts(n.0, fnv)
        .contains(&(REFUSED, allegiance::text::JOIN_KINGDOM.0)));
    assert!(!h.server.allegiances.member(n.1));
    let fm = h.bots[m].inbox.len();
    h.ask(m, manage::RESTRICT, &[Arg::Int(1), Arg::Int(1)], 10);
    assert_eq!(
        h.texts(m, fm),
        vec![(NOTICE, text::RESTRICTED_OR_NEUTRAL.0)]
    );
    swear(&mut h, n, (v, vid));
    assert!(h.server.allegiances.member(n.1), "a neutral is let in");
    let fm = h.bots[m].inbox.len();
    h.ask(m, manage::RESTRICT, &[Arg::Int(0), Arg::Int(0)], 10);
    assert_eq!(h.texts(m, fm), vec![(NOTICE, text::UNRESTRICTED.0)]);
}

/// Looking for a member by name says it was found and sends its profile for the allegiance page;
/// a name outside the allegiance is told so.
/// Behaviour: server.allegiance.member-search
#[test]
fn a_member_found_by_name_is_sent_as_its_profile() {
    let (mut h, (m, mid), (v, _)) = sworn_pair();
    let fv = h.bots[v].inbox.len();
    h.ask(v, manage::SEARCH, &[string_arg("patronus")], 10);
    assert_eq!(h.texts(v, fv), vec![(NOTICE, text::FOUND.0)]);
    let p = profiles(&h, v, fv);
    assert_eq!(p.len(), 1, "the found member's profile");
    let Some(Value::Graph(g)) = p[0].first() else {
        panic!("a profile");
    };
    let Some(Node::Script { slots, .. }) = g.first().map(|(_, n)| n) else {
        panic!("a profile");
    };
    let at = slots[2].1;
    let member = g.iter().find_map(|(i, n)| match n {
        Node::Script { slots, .. } if *i == at => {
            Some(u64::from(slots[10].1) << 32 | u64::from(slots[9].1))
        }
        _ => None,
    });
    assert_eq!(member, Some(mid), "centred on the one found");
    let fm = h.bots[m].inbox.len();
    h.ask(m, manage::SEARCH, &[string_arg("Strangera")], 10);
    assert_eq!(h.texts(m, fm), vec![(REFUSED, text::NOT_A_MEMBER.0)]);
}

/// A vassal's words to its patron reach the patron alone and the vassal sees what it said; the
/// patron's to its vassals reach them; a monarch's to all its followers reach every member below
/// it; one outside hears none of it. A monarch has no patron to tell, and a patron away is said so.
/// Behaviour: server.allegiance.messages
#[test]
fn words_to_patron_vassals_and_followers_reach_them_alone() {
    let mut h = Harness::new();
    let v = h.player_in_world("one", "Vassalia");
    let p = h.player_in_world("two", "Patronus");
    let g = h.player_in_world("three", "Novicia");
    let s = h.player_in_world("four", "Strangera");
    swear(&mut h, v, p);
    swear(&mut h, g, v);
    let froms = |h: &Harness| [v, p, g, s].map(|(b, _)| h.bots[b].inbox.len());
    let f = froms(&h);
    h.ask(g.0, manage::TO_PATRON, &[string_arg("hello up")], 10);
    assert_eq!(h.texts(v.0, f[0]), vec![(ALLEGIANCE, text::PATRON_HEARS.0)]);
    assert_eq!(h.texts(g.0, f[2]), vec![(ALLEGIANCE, text::TOLD_PATRON.0)]);
    assert!(h.texts(p.0, f[1]).is_empty() && h.texts(s.0, f[3]).is_empty());
    let f = froms(&h);
    h.ask(p.0, manage::TO_VASSALS, &[string_arg("hello down")], 10);
    assert_eq!(h.texts(v.0, f[0]), vec![(ALLEGIANCE, text::VASSALS_HEAR.0)]);
    assert!(
        h.texts(g.0, f[2]).is_empty(),
        "a vassal's vassal is no vassal"
    );
    let f = froms(&h);
    h.ask(p.0, manage::TO_FOLLOWERS, &[string_arg("hello all")], 10);
    assert_eq!(
        h.texts(v.0, f[0]),
        vec![(ALLEGIANCE, text::FOLLOWERS_HEAR.0)]
    );
    assert_eq!(
        h.texts(g.0, f[2]),
        vec![(ALLEGIANCE, text::FOLLOWERS_HEAR.0)]
    );
    assert_eq!(
        h.texts(p.0, f[1]),
        vec![(ALLEGIANCE, text::TOLD_FOLLOWERS.0)]
    );
    assert!(h.texts(s.0, f[3]).is_empty());
    let f = froms(&h);
    h.ask(p.0, manage::TO_PATRON, &[string_arg("anyone?")], 10);
    assert_eq!(h.texts(p.0, f[1]), vec![(REFUSED, text::NO_PATRON.0)]);
    let f = froms(&h);
    h.ask(s.0, manage::TO_VASSALS, &[string_arg("anyone?")], 10);
    assert_eq!(
        h.texts(s.0, f[3]),
        vec![(REFUSED, text::NOT_IN_ALLEGIANCE.0)]
    );
    // A patron not in the world.
    let away = 0x2130_0000_0300_0000;
    h.server.allegiances.nodes.insert(away, Member::default());
    if let Some(n) = h.server.allegiances.nodes.get_mut(&p.1) {
        n.patron = away;
    }
    let f = froms(&h);
    h.ask(p.0, manage::TO_PATRON, &[string_arg("are you there")], 10);
    assert_eq!(h.texts(p.0, f[1]), vec![(REFUSED, text::PATRON_AWAY.0)]);
}

/// A member breaking away leaves its old allegiance's officers and title behind.
/// Behaviour: server.allegiance.leaving-drops-office-and-title
#[test]
fn a_member_who_breaks_away_is_no_longer_an_officer_and_loses_its_title() {
    let (mut h, (m, mid), (v, vid)) = sworn_pair();
    h.ask(m, manage::APPOINT, &[string_arg("Vassalia")], 10);
    h.ask(
        m,
        manage::SET_TITLE,
        &[string_arg("Keeper"), string_arg("Vassalia")],
        10,
    );
    assert!(h.server.allegiances.is_officer(vid));
    h.ask(v, allegiance::function::BREAK, &[Arg::LongInt(mid)], 10);
    assert!(!h.server.allegiances.is_officer(vid));
    assert!(!h.server.allegiances.titles.contains_key(&vid));
    assert!(!h.server.allegiances.charters.contains_key(&mid));
}

/// Looking among a long allegiance's members comes out of a connection's allowance of rows: one
/// search is answered however long the allegiance, one straight after it isn't, and in time
/// another is.
/// Behaviour: server.allegiance.lookups-out-of-an-allowance
#[test]
fn a_search_among_a_long_allegiance_comes_out_of_an_allowance() {
    use yalain_world::market::{PAGE_ROWS_A_SECOND, PAGE_ROWS_HELD};
    let (mut h, (_, mid), (v, _)) = sworn_pair();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let members = PAGE_ROWS_HELD as u64;
    for i in 0..members {
        h.server.allegiances.nodes.insert(
            0x2130_0000_0400_0000 + i,
            Member {
                patron: mid,
                joined: 1.0e9 + i as f64,
                ..Member::default()
            },
        );
    }
    h.step_by(PAGE_ROWS_HELD / PAGE_ROWS_A_SECOND + 1.0);
    let found = |h: &Harness, from: usize| {
        h.texts(v, from)
            .iter()
            .filter(|t| t.1 == text::FOUND.0)
            .count()
    };
    let from = h.bots[v].inbox.len();
    let call = server_call(manage::SEARCH, &[string_arg("Patronus")]);
    h.bots[v].send(true, queue::WEENIE, &call);
    h.bots[v].send(true, queue::WEENIE, &call);
    assert!(
        h.run_until(50_000, |h| found(h, from) >= 1),
        "the first is answered"
    );
    assert!(
        !h.run_until(5_000, |h| found(h, from) >= 2),
        "the next isn't"
    );
    h.step_by(PAGE_ROWS_HELD / PAGE_ROWS_A_SECOND + 1.0);
    let from = h.bots[v].inbox.len();
    h.bots[v].send(true, queue::WEENIE, &call);
    assert!(h.run_until(50_000, |h| found(h, from) >= 1), "back in time");
}
