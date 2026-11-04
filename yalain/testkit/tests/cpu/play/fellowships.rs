//! Fellowships: made, recruited into (walked to, asked, accepted, run out), left, disbanded; the
//! fellows' updates; shared experience.

use auberean_primitives::Vec3;
use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use auberean_script::native_object::{native_type, NativeObject};
use yalain_testkit::play::report;
use yalain_testkit::Harness;
use yalain_world::calls::Value;
use yalain_world::fellowship::{function, text};

fn string_arg(s: &str) -> Arg {
    let mut b = yalain_world::graph::Builder::new();
    #[allow(clippy::cast_possible_truncation)]
    b.native(
        native_type::WP_STRING as u16,
        NativeObject::Text(s.encode_utf16().collect()),
    );
    Arg::Object(b.graph())
}

/// Put one player where another stands (`dx` meters east of it).
fn beside(h: &mut Harness, who: u64, of: u64, dx: f32) {
    let at = h
        .server
        .players
        .values()
        .find(|p| p.character == of)
        .expect("in the world")
        .position;
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == who)
        .expect("in the world");
    p.position = at;
    p.position.origin[0] += dx;
}

fn create(h: &mut Harness, b: usize, name: &str) {
    h.ask(
        b,
        function::CREATE,
        &[Arg::Int(1), Arg::Int(0), string_arg(name)],
        10,
    );
}

fn three() -> (Harness, [(usize, u64); 3]) {
    let mut h = Harness::new();
    let a = h.player_in_world("one", "Leader");
    let b = h.player_in_world("two", "Second");
    let c = h.player_in_world("three", "Third");
    beside(&mut h, b.1, a.1, 0.5);
    beside(&mut h, c.1, a.1, -0.5);
    (h, [a, b, c])
}

/// Recruit a player beside the leader and have it accept.
fn join(h: &mut Harness, (la, lid): (usize, u64), (ra, rid): (usize, u64)) {
    h.ask(la, function::RECRUIT, &[Arg::LongInt(rid)], 10);
    h.ask(ra, function::ACCEPT, &[Arg::LongInt(lid)], 10);
}

/// Making a fellowship enters its channel, sends the fellowship (the maker its leader and only
/// fellow) and says it was made; a name with a digit is refused.
/// Behaviour: server.fellowship.made
#[test]
fn a_fellowship_made_is_sent_to_its_maker_and_a_bad_name_is_refused() {
    let (mut h, [(a, aid), _, _]) = three();
    let from = h.bots[a].inbox.len();
    create(&mut h, a, "fellow1");
    assert_eq!(h.texts(a, from), vec![(1, text::BAD_NAME.0)]);
    assert!(!h.functions(a, from).contains(&function::UPDATE_FELLOWSHIP));
    let from = h.bots[a].inbox.len();
    create(&mut h, a, "The Band");
    let t = h.texts(a, from);
    assert!(t.contains(&(8, text::CHANNEL_ENTERED.0)) && t.contains(&(8, text::CREATED.0)));
    let i = h.server.fellowships.of(aid).expect("made");
    let f = &h.server.fellowships.list[i];
    assert_eq!((f.leader, f.fellows.len(), f.flags), (aid, 1, 0x1 | 0x2));
    assert!(h.functions(a, from).contains(&function::UPDATE_FELLOWSHIP));
}

/// Recruiting a player beside the leader tells the leader it waits and asks the recruit; taking
/// the place tells the recruit it joined (the whole fellowship sent, the channel entered) and
/// every fellow already in it of the new fellow; a second fellowship's member is refused as in one
/// already.
/// Behaviour: server.fellowship.recruited
#[test]
fn a_recruit_asked_and_accepting_joins_and_every_fellow_is_told() {
    let (mut h, [(a, aid), (b, bid), (c, cid)]) = three();
    create(&mut h, a, "Band");
    let (fa, fb) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    h.ask(a, function::RECRUIT, &[Arg::LongInt(bid)], 10);
    assert!(h.texts(a, fa).contains(&(8, text::WAITING.0)));
    let asked = h.calls(b, fb);
    let (_, v) = asked
        .iter()
        .find(|(f, _)| *f == function::REQUEST_RECRUITMENT)
        .expect("the recruit is asked");
    assert!(v.contains(&Value::Long(aid)));
    let (fa, fb) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    h.ask(b, function::ACCEPT, &[Arg::LongInt(aid)], 10);
    let t = h.texts(b, fb);
    assert!(t.contains(&(8, text::YOU_JOIN.0)) && t.contains(&(8, text::JOINED.0)));
    assert!(h.functions(b, fb).contains(&function::UPDATE_FELLOWSHIP));
    assert!(h.functions(a, fa).contains(&function::UPDATE_FELLOW));
    assert!(h.texts(a, fa).contains(&(8, text::HAS_JOINED.0)));
    // The third joins too; then a second fellowship's leader cannot recruit any of them.
    join(&mut h, (a, aid), (c, cid));
    assert_eq!(
        h.server.fellowships.list[h.server.fellowships.of(aid).expect("in one")]
            .fellows
            .len(),
        3
    );
    let d = h.player_in_world("four", "Fourth");
    beside(&mut h, d.1, aid, 1.0);
    create(&mut h, d.0, "Other");
    let from = h.bots[d.0].inbox.len();
    h.ask(d.0, function::RECRUIT, &[Arg::LongInt(bid)], 10);
    assert_eq!(h.texts(d.0, from), vec![(1, text::ALREADY_IN.0)]);
}

/// A recruit out of reach is walked to: the leader gets a movement order and the recruit is asked
/// only once the leader reports itself there.
/// Behaviour: server.fellowship.recruited
#[test]
fn a_leader_is_walked_to_a_recruit_out_of_reach_before_it_is_asked() {
    let (mut h, [(a, aid), (b, bid), _]) = three();
    create(&mut h, a, "Band");
    beside(&mut h, bid, aid, 12.0);
    let (fa, fb) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    h.ask(a, function::RECRUIT, &[Arg::LongInt(bid)], 10);
    assert!(h
        .world_messages(a, fa)
        .iter()
        .any(|m| matches!(m, ServerMessage::MoveTo(_))));
    assert!(!h.functions(b, fb).contains(&function::REQUEST_RECRUITMENT));
    let at = h
        .server
        .players
        .values()
        .find(|p| p.character == bid)
        .expect("in the world")
        .position;
    let stamp = h
        .server
        .players
        .values()
        .find(|p| p.character == aid)
        .expect("in the world")
        .teleport_stamp;
    h.bots[a].send(
        true,
        queue::WEENIE,
        &report(
            at.cell,
            Vec3::new(at.origin[0] - 1.8, at.origin[1], at.origin[2]),
            0,
            yalain_world::world::IDLE_MOTION,
            stamp,
        ),
    );
    h.run(10);
    assert!(h.functions(b, fb).contains(&function::REQUEST_RECRUITMENT));
}

/// An asking unanswered for twenty seconds runs out: the recruit is told; a place taken after it
/// is not given.
/// Behaviour: server.fellowship.asking-runs-out
#[test]
fn an_unanswered_asking_runs_out_after_twenty_seconds() {
    let (mut h, [(a, aid), (b, bid), _]) = three();
    create(&mut h, a, "Band");
    h.ask(a, function::RECRUIT, &[Arg::LongInt(bid)], 10);
    let fb = h.bots[b].inbox.len();
    let t = h.now.0;
    h.run_to(yalain_net::LocalTime(t + 19.0));
    h.run(5);
    assert!(!h.functions(b, fb).contains(&function::EXPIRE_RECRUITMENT));
    h.run_to(yalain_net::LocalTime(t + 21.0));
    h.run(5);
    assert!(h.functions(b, fb).contains(&function::EXPIRE_RECRUITMENT));
    h.ask(b, function::ACCEPT, &[Arg::LongInt(aid)], 10);
    assert_eq!(
        h.server.fellowships.list[0].fellows.len(),
        1,
        "the place ran out"
    );
}

/// A fellow leaving is cleared of the fellowship and told; the others are told it left; a leader
/// leaving leaves the oldest fellow leading; disbanding clears every fellow and tells each.
/// Behaviour: server.fellowship.left-and-disbanded
#[test]
fn leaving_and_disbanding_clear_the_fellows_and_tell_them() {
    let (mut h, [(a, aid), (b, bid), (c, cid)]) = three();
    create(&mut h, a, "Band");
    join(&mut h, (a, aid), (b, bid));
    join(&mut h, (a, aid), (c, cid));
    let (fa, fb, fc) = (
        h.bots[a].inbox.len(),
        h.bots[b].inbox.len(),
        h.bots[c].inbox.len(),
    );
    h.ask(a, function::LEAVE, &[], 10);
    assert!(h.functions(a, fa).contains(&function::CLEAR_FELLOWSHIP));
    assert!(h.texts(a, fa).contains(&(8, text::YOU_LEAVE.0)));
    for (x, from) in [(b, fb), (c, fc)] {
        assert!(h.functions(x, from).contains(&function::CLEAR_FELLOW));
        assert!(h.texts(x, from).contains(&(8, text::HAS_LEFT.0)));
    }
    let i = h.server.fellowships.of(bid).expect("still one");
    assert_eq!(h.server.fellowships.list[i].leader, bid, "the oldest leads");
    let (fb, fc) = (h.bots[b].inbox.len(), h.bots[c].inbox.len());
    h.ask(b, function::DISBAND, &[], 10);
    for (x, from) in [(b, fb), (c, fc)] {
        assert!(h.functions(x, from).contains(&function::CLEAR_FELLOWSHIP));
        assert!(h.texts(x, from).contains(&(8, text::DISBANDED.0)));
    }
    assert!(h.server.fellowships.list.is_empty());
}

/// A fellow's changed health goes to every fellow, itself included, within seven and a half
/// seconds; nothing is sent while nothing changes.
/// Behaviour: server.fellowship.updates
#[test]
fn a_fellows_changed_health_goes_to_every_fellow_and_nothing_unchanged_does() {
    let (mut h, [(a, aid), (b, bid), _]) = three();
    create(&mut h, a, "Band");
    join(&mut h, (a, aid), (b, bid));
    // The first looks send what the fellows are; then quiet.
    let t = h.now.0;
    h.run_to(yalain_net::LocalTime(t + 16.0));
    let (fa, fb) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    let t = h.now.0;
    h.run_to(yalain_net::LocalTime(t + 16.0));
    assert!(!h.functions(a, fa).contains(&function::HEALTH));
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == bid)
        .expect("in the world");
    yalain_world::quals::set_int(&mut p.qualities, 258, 41);
    let (fa, fb2) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    let t = h.now.0;
    h.run_to(yalain_net::LocalTime(t + 7.6));
    h.run(5);
    for (x, from) in [(a, fa), (b, fb2)] {
        let c = h.calls(x, from);
        let (_, v) = c
            .iter()
            .find(|(f, _)| *f == function::HEALTH)
            .expect("health sent");
        assert!(
            v.contains(&Value::Long(bid)) && v.contains(&Value::Int(41)),
            "{v:?}"
        );
    }
    let _ = fb;
}

/// A fellow's changes seen at one look go out in 2017's order: greatest health, health, greatest
/// vigor, vigor, then its place.
/// Behaviour: server.fellowship.updates-in-order
#[test]
fn a_fellows_changes_go_out_in_order_health_before_vigor_before_place() {
    let (mut h, [(a, aid), (b, bid), _]) = three();
    create(&mut h, a, "Band");
    join(&mut h, (a, aid), (b, bid));
    let t = h.now.0;
    h.run_to(yalain_net::LocalTime(t + 16.0));
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == bid)
        .expect("in the world");
    for (q, v) in [(263, 210), (258, 41), (264, 220), (259, 37)] {
        yalain_world::quals::set_int(&mut p.qualities, q, v);
    }
    p.position.cell = p.position.cell.wrapping_add(1);
    let fa = h.bots[a].inbox.len();
    let t = h.now.0;
    h.run_to(yalain_net::LocalTime(t + 7.6));
    h.run(5);
    let order: Vec<u32> = h
        .calls(a, fa)
        .into_iter()
        .filter(|(f, v)| {
            [
                function::MAX_HEALTH,
                function::HEALTH,
                function::MAX_VIGOR,
                function::VIGOR,
                function::CELL,
            ]
            .contains(f)
                && v.contains(&Value::Long(bid))
        })
        .map(|(f, _)| f)
        .collect();
    // The first look's five (regeneration may send health and vigor again after).
    assert_eq!(
        order.get(..5).unwrap_or_default(),
        [
            function::MAX_HEALTH,
            function::HEALTH,
            function::MAX_VIGOR,
            function::VIGOR,
            function::CELL
        ]
    );
}

/// A kill's experience in a fellowship that shares it: each fellow of a level to share gets the
/// creature's experience by its own level times the share for how many share (0.6 for two); a
/// fellow more than five levels from the leader shares nothing; a group creature's takes the
/// factor for the fellowship's size.
/// Behaviour: server.fellowship.shared-experience
#[test]
fn a_kill_is_shared_by_the_fellows_of_a_level_to_share() {
    let (mut h, [(a, aid), (b, bid), (c, cid)]) = three();
    create(&mut h, a, "Band");
    join(&mut h, (a, aid), (b, bid));
    join(&mut h, (a, aid), (c, cid));
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == cid)
        .expect("in the world");
    yalain_world::quals::set_int(&mut p.qualities, 302, 9);
    let shares =
        yalain_world::fellowship::shares(&h.server, aid, 100, 1, false, (1, 1)).expect("shares");
    let mut got: Vec<u64> = shares.iter().map(|(_, n)| *n).collect();
    got.sort_unstable();
    assert_eq!(got, vec![60, 60], "two of a level to share, 0.6 each");
    let group =
        yalain_world::fellowship::shares(&h.server, aid, 100, 1, true, (1, 1)).expect("shares");
    assert!(
        group.iter().all(|(_, n)| *n == 90),
        "{group:?}: three fellows, 1.5"
    );
    let _ = (b, c);
}

/// In a fellowship that shares experience, each part of a kill a fellow earns is shared out: every
/// fellow of a level to share is paid the share of the experience (0.6 for two) times the part's
/// share of the health taken, rounded to the nearest, each part in turn, without a text; the
/// killer then hears that it killed the creature. A player outside the fellowship is paid nothing.
/// Behaviour: server.fellowship.shares-each-part
#[test]
fn each_part_of_a_kill_is_shared_out_among_the_fellows_without_a_text() {
    use super::Told;
    use yalain_world::progress::text as texts;
    let (mut h, [(a, aid), (b, bid), (c, cid)]) = three();
    create(&mut h, a, "Band");
    join(&mut h, (a, aid), (b, bid));
    h.add_creatures(1);
    h.run(20);
    let beetle = h
        .server
        .things
        .by_id
        .values()
        .find(|t| t.creature.is_some())
        .map(|t| t.id)
        .expect("a beetle");
    let level = h
        .server
        .players
        .values()
        .find(|p| p.character == aid)
        .map(|p| yalain_world::examine::level_of(&p.qualities))
        .expect("in the world");
    if let Some(t) = h.server.things.get_mut(beetle) {
        t.health = 301;
        t.max_health = 301;
        if let Some(cr) = t.creature.as_mut() {
            cr.stats.death_xp = 1000;
            cr.stats.level = level;
            cr.stats.attacks_first = false;
        }
    }
    let from: Vec<usize> = [a, b, c].iter().map(|x| h.bots[*x].inbox.len()).collect();
    let clock = h.clock();
    // The first takes 101, the second 200 and the kill: 600 x 101 / 301 = 201.3 and
    // 600 x 200 / 301 = 398.7.
    yalain_world::combat::hurt_creature(&mut h.server, &mut h.net, beetle, aid, 101, clock);
    yalain_world::combat::hurt_creature(&mut h.server, &mut h.net, beetle, bid, 200, clock);
    h.run(10);
    let told = |bot: usize, i: usize| -> Vec<Told> {
        super::experience(&h, bot, from[i])
            .into_iter()
            .filter(|t| match t {
                Told::Text(id, _) => {
                    [texts::GAINED.0, texts::KILLED.0, texts::UNFAIR.0].contains(id)
                }
                Told::Total(_) => true,
            })
            .collect()
    };
    let paid = vec![Told::Total(201), Told::Total(600)];
    assert_eq!(told(a, 0), paid, "the first fellow");
    let mut killer = paid;
    killer.push(Told::Text(texts::KILLED.0, None));
    assert_eq!(told(b, 1), killer, "the second fellow, the killer");
    assert_eq!(told(c, 2), vec![], "outside the fellowship");
    let _ = cid;
}

fn set_level(h: &mut Harness, who: u64, level: i32) {
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == who)
        .expect("in the world");
    yalain_world::quals::set_int(&mut p.qualities, 302, level);
}

fn world(h: &Harness, who: u64) -> yalain_net::ConnId {
    h.server
        .players
        .iter()
        .find(|(_, p)| p.character == who)
        .map(|(w, _)| *w)
        .expect("in the world")
}

/// The share goes by how many fellows are of a level to share, near or not: a fellow 500 m away
/// still counts (0.45 for three), but only those within 160 m of the fellow whose part it is are
/// paid.
/// Behaviour: server.fellowship.share-counts-far-fellows
#[test]
fn the_share_counts_every_fellow_of_a_level_to_share_near_or_not_and_pays_those_within_160_m() {
    let (mut h, [(a, aid), (b, bid), (c, cid)]) = three();
    create(&mut h, a, "Band");
    join(&mut h, (a, aid), (b, bid));
    join(&mut h, (a, aid), (c, cid));
    beside(&mut h, cid, aid, 500.0);
    let shares =
        yalain_world::fellowship::shares(&h.server, aid, 100, 1, false, (1, 1)).expect("shares");
    let mut got: Vec<(yalain_net::ConnId, u64)> = shares;
    got.sort_unstable();
    let mut want = vec![(world(&h, aid), 45), (world(&h, bid), 45)];
    want.sort_unstable();
    assert_eq!(got, want, "0.45 each for three; the far one unpaid");
    let _ = (b, c);
}

/// Shares go evenly: every fellow of a level to share gets the same share of the creature's
/// experience by its own level, whatever its level against the others'.
/// Behaviour: server.fellowship.shares-evenly
#[test]
fn shares_go_evenly_whatever_the_fellows_levels() {
    let (mut h, [(a, aid), (b, bid), _]) = three();
    create(&mut h, a, "Band");
    join(&mut h, (a, aid), (b, bid));
    set_level(&mut h, aid, 4);
    set_level(&mut h, bid, 8);
    let shares =
        yalain_world::fellowship::shares(&h.server, aid, 1000, 4, false, (1, 1)).expect("shares");
    let mut got: Vec<(yalain_net::ConnId, u64)> = shares;
    got.sort_unstable();
    let mut want = vec![(world(&h, aid), 600), (world(&h, bid), 600)];
    want.sort_unstable();
    assert_eq!(got, want, "0.6 each, levels 4 and 8");
    let _ = b;
}

/// A fellow more than five levels from the leader's is not of a level to share: its own part of a
/// kill is paid to it alone, whole by its own level (with no share factor) and without a text, and
/// no other part reaches it; the fellows of a level to share split only their own parts.
/// Behaviour: server.fellowship.unlevelled-fellow-paid-alone
#[test]
fn a_fellow_not_of_a_level_to_share_is_paid_its_own_part_whole_and_none_of_the_others() {
    use super::Told;
    use yalain_world::progress::text as texts;
    let (mut h, [(a, aid), (b, bid), (c, cid)]) = three();
    create(&mut h, a, "Band");
    join(&mut h, (a, aid), (b, bid));
    join(&mut h, (a, aid), (c, cid));
    h.add_creatures(1);
    h.run(20);
    let beetle = h
        .server
        .things
        .by_id
        .values()
        .find(|t| t.creature.is_some())
        .map(|t| t.id)
        .expect("a beetle");
    let level = h
        .server
        .players
        .values()
        .find(|p| p.character == aid)
        .map(|p| yalain_world::examine::level_of(&p.qualities))
        .expect("in the world");
    set_level(&mut h, cid, level + 11);
    if let Some(t) = h.server.things.get_mut(beetle) {
        t.health = 300;
        t.max_health = 300;
        if let Some(cr) = t.creature.as_mut() {
            cr.stats.death_xp = 1000;
            cr.stats.level = level;
            cr.stats.attacks_first = false;
        }
    }
    let from: Vec<usize> = [a, b, c].iter().map(|x| h.bots[*x].inbox.len()).collect();
    let clock = h.clock();
    // The third, eleven levels above the creature and the leader, takes 100: by its own level the
    // kill is worth 85% of 1,000, and its 850 x 100 / 300 = 283 is its alone. The first takes 200
    // and the kill: 0.6 x 1,000 x 200 / 300 = 400 to each of the two of a level.
    yalain_world::combat::hurt_creature(&mut h.server, &mut h.net, beetle, cid, 100, clock);
    yalain_world::combat::hurt_creature(&mut h.server, &mut h.net, beetle, aid, 200, clock);
    h.run(10);
    let told = |bot: usize, i: usize| -> Vec<Told> {
        super::experience(&h, bot, from[i])
            .into_iter()
            .filter(|t| match t {
                Told::Text(id, _) => [texts::GAINED.0, texts::KILLED.0].contains(id),
                Told::Total(_) => true,
            })
            .collect()
    };
    assert_eq!(
        told(a, 0),
        vec![Told::Total(400), Told::Text(texts::KILLED.0, None)],
        "the killer"
    );
    assert_eq!(told(b, 1), vec![Told::Total(400)], "its fellow of a level");
    assert_eq!(told(c, 2), vec![Told::Total(283)], "the one not of a level");
}

/// The focus of a part earned by a fellow not of a level to share is its own, whole: the
/// creature's death focus times the part's share of the health taken, and no one else's.
/// Behaviour: server.focus.unlevelled-fellow-keeps-focus
#[test]
fn a_fellow_not_of_a_level_to_share_keeps_its_parts_focus_whole() {
    let (mut h, [(a, aid), (b, bid), (c, cid)]) = three();
    create(&mut h, a, "Band");
    join(&mut h, (a, aid), (b, bid));
    join(&mut h, (a, aid), (c, cid));
    set_level(&mut h, cid, 9);
    let got = yalain_world::fellowship::focus_shares(&h.server, cid, 20, (1, 2));
    assert_eq!(got, vec![(world(&h, cid), 10)], "its own half of 20");
    let fellows = yalain_world::fellowship::focus_shares(&h.server, aid, 20, (1, 2));
    assert!(
        fellows.iter().all(|(w, f)| *w != world(&h, cid) && *f == 6),
        "{fellows:?}: 20 x 0.6 x 1/2 to the two of a level"
    );
    assert_eq!(fellows.len(), 2);
    let _ = b;
}

/// A kill goes to the player that did the creature the most damage: it hears it killed the
/// creature. A fellow of its whose blow killed it hears only that its blow did, naming the
/// creature; a player outside the killer's fellowship whose blow killed it hears nothing of it.
/// Behaviour: server.fellowship.killing-blow-told
#[test]
fn a_fellows_killing_blow_on_a_kill_credited_to_another_fellow_is_told_to_it_alone() {
    use yalain_world::death::KILLING_BLOW;
    use yalain_world::progress::text as texts;
    for fellows in [true, false] {
        let (mut h, [(a, aid), (b, bid), (c, cid)]) = three();
        if fellows {
            create(&mut h, a, "Band");
            join(&mut h, (a, aid), (b, bid));
            join(&mut h, (a, aid), (c, cid));
        }
        h.add_creatures(1);
        h.run(20);
        let beetle = h
            .server
            .things
            .by_id
            .values()
            .find(|t| t.creature.is_some())
            .map(|t| t.id)
            .expect("a beetle");
        const NAME: (u32, u32) = (0xA860_7CEC, 0x2500_2A26);
        if let Some(t) = h.server.things.get_mut(beetle) {
            t.facts.name = Some(NAME);
            if let Some(cr) = t.creature.as_mut() {
                cr.damage_by = vec![(bid, 10), (aid, 30)];
            }
        }
        let from: Vec<usize> = [a, b, c].iter().map(|x| h.bots[*x].inbox.len()).collect();
        let clock = h.clock();
        yalain_world::death::creature_dies(&mut h.server, &mut h.net, beetle, bid, clock);
        h.run(10);
        let calls = |i: usize, bot: usize| -> Vec<(u32, Vec<Value>)> {
            h.world_messages(bot, from[i])
                .iter()
                .filter_map(|m| match m {
                    ServerMessage::PlayerCall(c) => Some((
                        c.function.0,
                        c.args
                            .as_ref()
                            .and_then(|x| yalain_world::calls::values(&x.0).ok())
                            .unwrap_or_default(),
                    )),
                    _ => None,
                })
                .collect()
        };
        let killed_text = |v: &[Value]| {
            v.iter().any(|x| {
                matches!(x.root(), Some(yalain_world::calls::Node::TextInfo(t))
                    if t.string_id == texts::KILLED.0)
            })
        };
        let (got_a, got_b) = (calls(0, a), calls(1, b));
        assert!(
            got_a.iter().any(|(_, v)| killed_text(v)),
            "the most damage gets the kill: {got_a:x?}"
        );
        assert!(!got_b.iter().any(|(_, v)| killed_text(v)), "not the blow");
        let told: Vec<&Vec<Value>> = got_b
            .iter()
            .filter(|(f, _)| *f == KILLING_BLOW)
            .map(|(_, v)| v)
            .collect();
        assert_eq!(told.len(), usize::from(fellows), "fellows: {fellows}");
        for v in told {
            assert!(
                matches!(v.first().and_then(Value::root), Some(yalain_world::calls::Node::TextInfo(t))
                    if (t.string_id, t.table) == NAME),
                "the creature's name"
            );
        }
        assert!(!got_a.iter().any(|(f, _)| *f == KILLING_BLOW));
        assert!(!calls(2, c).iter().any(|(f, _)| *f == KILLING_BLOW));
    }
}

/// A kill by a player the creature could not reach is no fair fight: the killer hears "You receive
/// no experience, since that wasn't a fair fight." and then that it killed the creature, and
/// nothing else; neither it nor its fellow is paid or sent totals, and the kill moves no quest on.
/// The same kill by a player it could reach pays them both and moves the quest on.
/// Behaviour: server.death.no-fair-fight
#[test]
fn a_kill_by_a_player_the_creature_could_not_reach_pays_no_one_and_moves_no_quest_on() {
    use yalain_content::quests::{QuestDef, Step, Trigger, Update};
    use yalain_world::progress::{q, text as texts};
    const QUEST: u32 = 0x4000_004C;
    for unreached in [true, false] {
        let (mut h, [(a, aid), (b, bid), _]) = three();
        create(&mut h, a, "Band");
        join(&mut h, (a, aid), (b, bid));
        h.add_creatures(1);
        h.run(20);
        let beetle = h
            .server
            .things
            .by_id
            .values()
            .find(|t| t.creature.is_some())
            .map(|t| t.id)
            .expect("a beetle");
        let template = h.server.things.get(beetle).expect("the beetle").template;
        if let Some(t) = h.server.things.get_mut(beetle) {
            t.facts.name = Some((0xA860_7CEC, 0x2500_2A26));
        }
        if let Some(cr) = h
            .server
            .things
            .get_mut(beetle)
            .and_then(|t| t.creature.as_mut())
        {
            cr.stats.death_xp = 1000;
            cr.damage_by = vec![(aid, 30)];
            cr.taken_by = vec![(aid, 30)];
            if unreached {
                cr.unreached = vec![aid];
            }
        }
        h.server.quests.quests.push(QuestDef {
            id: QUEST,
            name: (1, 2),
            description: (3, 4),
            icon: 0,
            play_fx: false,
            journals: vec![(5, 6)],
            challenge: 1,
            duration: 0.0,
            timed_phase: 0,
            retry: 0.0,
            xp: 0,
            cancellable: false,
            placed: "made up for the test".into(),
            hidden: true,
            effects: Vec::new(),
            subquests: Vec::new(),
            min_level: 0,
            fails_on_death: false,
            once: false,
            fellows: yalain_content::quests::Fellows::default(),
            last_waits: false,
            hero_only: false,
            pay: yalain_content::quests::CompletionPay::default(),
            required: None,
        });
        h.server.quests.steps.push(Step {
            quest: QUEST,
            needs: 0,
            trigger: Trigger::Kill(template),
            tell: None,
            update: Update::Complete,
            xp: 0,
            gives: Vec::new(),
            counts: Vec::new(),
            effects: Vec::new(),
            text: None,
            placed: "made up for the test".into(),
            requires: Vec::new(),
            takes: Vec::new(),
            races: Vec::new(),
            effects_last: false,
            wants: None,
            fellows: false,
            to: None,
        });
        let from = [h.bots[a].inbox.len(), h.bots[b].inbox.len()];
        let clock = h.clock();
        yalain_world::death::creature_dies(&mut h.server, &mut h.net, beetle, aid, clock);
        h.run(10);
        let ids: Vec<u32> = h.texts(a, from[0]).iter().map(|(_, id)| *id).collect();
        let totals = |bot: usize, i: usize| {
            h.world_messages(bot, from[i])
                .iter()
                .filter(
                    |m| matches!(m, ServerMessage::UpdateLongInt(x) if x.quality == q::TOTAL_XP),
                )
                .count()
        };
        let held = h
            .server
            .players
            .values()
            .find(|p| p.character == aid)
            .is_some_and(|p| p.play.quests.iter().any(|x| x.quest == QUEST));
        if unreached {
            assert_eq!(
                ids,
                vec![texts::UNFAIR.0, texts::KILLED.0],
                "told, then the kill"
            );
            assert_eq!((totals(a, 0), totals(b, 1)), (0, 0), "no one paid");
            assert!(!held, "no quest moved on");
        } else {
            assert!(!ids.contains(&texts::UNFAIR.0), "{ids:x?}");
            assert!(ids.contains(&texts::KILLED.0));
            assert!(totals(a, 0) > 0 && totals(b, 1) > 0, "both paid");
            assert!(held, "the quest moved on");
        }
    }
}
