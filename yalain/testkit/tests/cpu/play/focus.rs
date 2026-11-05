//! Focus: a hero's kill pays the creature's death focus in parts with its experience, whatever the
//! levels; a part worth focus alone says so; a player who is no hero gains none; focus stops at its
//! greatest; a fellowship shares it within 100 m without a group creature's factor; making a
//! character a hero. Fixture: the test world's beetles, one of them the player's pet, the prey worth
//! 1000 experience and 20 focus with 300 health.

use auberean_protocol::ServerMessage;
use auberean_script::native_object::VariableValue;
use yalain_testkit::Harness;
use yalain_world::calls::Node;
use yalain_world::pets::{Order, Pet};
use yalain_world::progress::{q, text, var};
use yalain_world::{combat, focus, pets, quals};

const HEALTH: i32 = 300;
const FOCUS: u32 = 266;

/// What a client was told of its experience and focus: a gained text (its string id, the
/// experience and the focus it names), a new focus, a new total.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Told {
    Text(u32, u64, u32),
    Focus(i32),
    Total(u64),
}

fn told(h: &Harness, b: usize, from: usize) -> Vec<Told> {
    let gained = [text::GAINED.0, text::GAINED_FOCUS.0, text::FOCUS_ONLY.0];
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c)
                if c.function.0 == yalain_world::session::function::SHOW_TEXT =>
            {
                let v = c
                    .args
                    .as_ref()
                    .and_then(|a| yalain_world::calls::values(&a.0).ok())
                    .unwrap_or_default();
                v.iter().find_map(|x| match x.root() {
                    Some(Node::TextInfo(t)) if gained.contains(&t.string_id) => {
                        let (mut xp, mut f) = (0, 0);
                        for v in &t.variables {
                            match v.value {
                                VariableValue::Long(n) if v.id == var::XP => xp = n,
                                VariableValue::Word(n) if v.id == var::FOCUS => f = n,
                                _ => {}
                            }
                        }
                        Some(Told::Text(t.string_id, xp, f))
                    }
                    _ => None,
                })
            }
            ServerMessage::UpdateInt(x) if x.quality == FOCUS => Some(Told::Focus(x.value)),
            ServerMessage::UpdateLongInt(x) if x.quality == q::TOTAL_XP => {
                Some(Told::Total(x.value))
            }
            _ => None,
        })
        .collect()
}

fn level_of(h: &Harness, me: u64) -> i32 {
    h.server
        .players
        .values()
        .find(|p| p.character == me)
        .map(|p| yalain_world::examine::level_of(&p.qualities))
        .expect("in the world")
}

fn set_focus(h: &mut Harness, me: u64, f: i32) {
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == me)
        .expect("in the world");
    quals::set_int(&mut p.qualities, FOCUS, f);
}

/// A player (made a hero or not, its focus set), three beetles: the prey (worth 1000 experience
/// and 20 focus, 300 health, `above` levels over the player), the player's pet, another.
fn world(hero: bool, above: i32) -> (Harness, usize, u64, [u64; 3]) {
    let mut h = Harness::new();
    h.add_creatures(3);
    let (b, me) = h.player_in_world("one", "Kestrel");
    h.run(20);
    if hero {
        focus::set_hero(&mut h.server, &mut h.net, "Kestrel", true).expect("made a hero");
        h.run(5);
    }
    set_focus(&mut h, me, 50);
    let level = level_of(&h, me);
    let mut beetles: Vec<u64> = h
        .server
        .things
        .by_id
        .values()
        .filter(|t| t.creature.is_some())
        .map(|t| t.id)
        .collect();
    beetles.sort_unstable();
    let [prey, pet, other] = beetles[..] else {
        panic!("three beetles: {beetles:?}")
    };
    for id in beetles {
        if let Some(c) = h
            .server
            .things
            .get_mut(id)
            .and_then(|t| t.creature.as_mut())
        {
            c.stats.attacks_first = false;
        }
    }
    if let Some(t) = h.server.things.get_mut(prey) {
        t.health = HEALTH;
        t.max_health = HEALTH;
        if let Some(c) = t.creature.as_mut() {
            c.stats.death_xp = 1000;
            c.stats.death_focus = 20;
            c.stats.level = level + above;
        }
    }
    h.server.pets.by_id.insert(
        pet,
        Pet {
            id: pet,
            owner: me,
            template: 0,
            class: 0,
            flags: 0,
            mode: 0,
            name: None,
            order: Order::Follow,
            next_plan: 0.0,
            goal: None,
            effect: 0,
            assist: None,
            next_tend: 0.0,
        },
    );
    (h, b, me, [prey, pet, other])
}

fn total_of(h: &Harness, me: u64) -> u64 {
    h.server
        .players
        .values()
        .find(|p| p.character == me)
        .and_then(|p| quals::long(&p.qualities, q::TOTAL_XP))
        .unwrap_or(0)
}

/// A hero's kill pays the creature's death focus in parts as its experience: each part the death
/// focus times the health the damager took over all taken, rounded to the nearest, named in "You
/// have gained %1 experience points and %2 Focus points!", the new focus sent next, then the totals.
/// Behaviour: server.focus.kill-pays-in-parts
#[test]
fn a_heros_kill_pays_its_death_focus_in_parts_with_the_experience() {
    let (mut h, b, me, [prey, pet, _]) = world(true, 0);
    let was = total_of(&h, me);
    let from = h.bots[b].inbox.len();
    let clock = h.clock();
    // The player takes 119, its pet 181: 20 x 119 / 300 = 7.9 and 20 x 181 / 300 = 12.1.
    combat::hurt_creature(&mut h.server, &mut h.net, prey, me, 119, clock);
    pets::struck(&mut h.server, &mut h.net, prey, (pet, 0), 181, clock);
    h.run(10);
    assert_eq!(
        told(&h, b, from),
        vec![
            Told::Text(text::GAINED_FOCUS.0, 397, 8),
            Told::Focus(58),
            Told::Total(was + 397),
            Told::Text(text::GAINED_FOCUS.0, 603, 12),
            Told::Focus(70),
            Told::Total(was + 1000),
        ]
    );
}

/// A kill's focus is not weighed by the levels: a creature fifteen levels above the hero pays its
/// whole death focus, while its experience is cut to a hundredth.
/// Behaviour: server.focus.whatever-the-levels
#[test]
fn a_kills_focus_is_the_same_whatever_the_levels() {
    let (mut h, b, me, [prey, _, _]) = world(true, 15);
    let was = total_of(&h, me);
    let from = h.bots[b].inbox.len();
    let clock = h.clock();
    combat::hurt_creature(&mut h.server, &mut h.net, prey, me, HEALTH, clock);
    h.run(10);
    assert_eq!(
        told(&h, b, from),
        vec![
            Told::Text(text::GAINED_FOCUS.0, 10, 20),
            Told::Focus(70),
            Told::Total(was + 10),
        ]
    );
}

/// A part worth focus and no experience says "You have gained %2 Focus points!" (no experience
/// named) and sends the new focus, and no totals.
/// Behaviour: server.focus.focus-alone
#[test]
fn a_part_worth_focus_and_no_experience_names_the_focus_alone() {
    let (mut h, b, me, [prey, _, _]) = world(true, 0);
    if let Some(c) = h
        .server
        .things
        .get_mut(prey)
        .and_then(|t| t.creature.as_mut())
    {
        c.stats.death_xp = 0;
    }
    let from = h.bots[b].inbox.len();
    let clock = h.clock();
    combat::hurt_creature(&mut h.server, &mut h.net, prey, me, HEALTH, clock);
    h.run(10);
    assert_eq!(
        told(&h, b, from),
        vec![Told::Text(text::FOCUS_ONLY.0, 0, 20), Told::Focus(70)]
    );
}

/// A player who is no hero gains no focus: its part says "You have gained %1 experience
/// points!", the focus the part was worth named in it, and no focus is sent.
/// Behaviour: server.focus.heroes-only
#[test]
fn a_player_who_is_no_hero_gains_no_focus() {
    let (mut h, b, me, [prey, _, _]) = world(false, 0);
    let was = total_of(&h, me);
    let from = h.bots[b].inbox.len();
    let clock = h.clock();
    combat::hurt_creature(&mut h.server, &mut h.net, prey, me, HEALTH, clock);
    h.run(10);
    assert_eq!(
        told(&h, b, from),
        vec![
            Told::Text(text::GAINED.0, 1000, 20),
            Told::Total(was + 1000)
        ]
    );
}

/// Focus stops at its greatest (100): an award past it sends 100; an award at it sends no focus,
/// its text still naming the whole part.
/// Behaviour: server.focus.greatest
#[test]
fn focus_stops_at_its_greatest_and_an_award_at_it_sends_no_focus() {
    let (mut h, b, me, [prey, _, other]) = world(true, 0);
    set_focus(&mut h, me, 95);
    let from = h.bots[b].inbox.len();
    let clock = h.clock();
    combat::hurt_creature(&mut h.server, &mut h.net, prey, me, HEALTH, clock);
    h.run(10);
    let got = told(&h, b, from);
    assert_eq!(
        &got[..2],
        &[Told::Text(text::GAINED_FOCUS.0, 1000, 20), Told::Focus(100)]
    );
    // Another kill at the greatest: the text, and no focus.
    let level = level_of(&h, me);
    if let Some(t) = h.server.things.get_mut(other) {
        t.health = HEALTH;
        t.max_health = HEALTH;
        if let Some(c) = t.creature.as_mut() {
            c.stats.death_xp = 1000;
            c.stats.death_focus = 20;
            c.stats.level = level;
        }
    }
    let from = h.bots[b].inbox.len();
    let clock = h.clock();
    combat::hurt_creature(&mut h.server, &mut h.net, other, me, HEALTH, clock);
    h.run(10);
    let got = told(&h, b, from);
    assert_eq!(got[0], Told::Text(text::GAINED_FOCUS.0, 1000, 20));
    assert!(!got.iter().any(|t| matches!(t, Told::Focus(_))), "{got:?}");
}

/// Making a character a hero in the world sends its hero quality and its focus at its greatest;
/// out of the world, its stored qualities are changed; no such name, refused.
/// Behaviour: server.focus.made-a-hero
#[test]
fn a_character_made_a_hero_is_told_and_has_its_greatest_focus() {
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Kestrel");
    h.run(20);
    set_focus(&mut h, me, 30);
    let from = h.bots[b].inbox.len();
    let said = focus::set_hero(&mut h.server, &mut h.net, "kestrel", true).expect("made one");
    assert_eq!(said, "Kestrel is now a hero.");
    h.run(5);
    let sent: Vec<(u32, i32)> = h
        .world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateBool(x) => Some((x.quality, x.value)),
            ServerMessage::UpdateInt(x) if x.quality == FOCUS => Some((x.quality, x.value)),
            _ => None,
        })
        .collect();
    assert_eq!(sent, vec![(focus::HERO, 1), (FOCUS, 100)]);
    let p = h.server.players.values().next().expect("in the world");
    assert!(focus::is_hero(&p.qualities));
    assert!(focus::set_hero(&mut h.server, &mut h.net, "Nobody", true).is_err());
    // A character not in the world: made a hero in the store, and enters as one at its greatest.
    let c = h.at_character_list("two");
    let id = h.create_character(c, "two", "Merrow");
    let said = focus::set_hero(&mut h.server, &mut h.net, "Merrow", true).expect("made one");
    assert_eq!(said, "Merrow is now a hero (out of the world).");
    h.enter_world(c, "two", id);
    let p = h
        .server
        .players
        .values()
        .find(|p| p.character == id)
        .expect("in the world");
    assert!(focus::is_hero(&p.qualities));
    assert_eq!(quals::int(&p.qualities, FOCUS), Some(100));
    assert!(
        yalain_world::examine::level_of(&p.qualities) >= 1
            && quals::int(&p.qualities, 258).is_some(),
        "entered with its first qualities"
    );
}

/// A part of a kill in a fellowship that shares experience: each fellow of a level to share
/// within 100 m of the one whose part it is gets the death focus times the share (0.45 for
/// three) times the part's share, rounded, without a text; a fellow beyond 100 m (within 160 m)
/// gets the experience and no focus; a group creature's factor (1.5 for three) is the
/// experience's alone.
/// Behaviour: server.focus.fellowship-shares
#[test]
fn a_fellowship_shares_a_kills_focus_within_100_m_without_the_group_factor() {
    use auberean_script::args::Arg;
    use yalain_world::fellowship::function;
    let mut h = Harness::new();
    let names = ["Leader", "Second", "Third"];
    let ps: Vec<(usize, u64)> = names
        .iter()
        .zip(["one", "two", "three"])
        .map(|(n, a)| h.player_in_world(a, n))
        .collect();
    h.run(20);
    let at = h
        .server
        .players
        .values()
        .find(|p| p.character == ps[0].1)
        .expect("in the world")
        .position;
    for (i, dx) in [(1usize, 0.5f32), (2, 130.0)] {
        let p = h
            .server
            .players
            .values_mut()
            .find(|p| p.character == ps[i].1)
            .expect("in the world");
        p.position = at;
        p.position.origin[0] += dx;
    }
    for n in names {
        focus::set_hero(&mut h.server, &mut h.net, n, true).expect("a hero");
    }
    for (_, id) in &ps {
        set_focus(&mut h, *id, 50);
    }
    let mut b = yalain_world::graph::Builder::new();
    #[allow(clippy::cast_possible_truncation)]
    b.native(
        auberean_script::native_object::native_type::WP_STRING as u16,
        auberean_script::native_object::NativeObject::Text("Band".encode_utf16().collect()),
    );
    h.ask(
        ps[0].0,
        function::CREATE,
        &[Arg::Int(1), Arg::Int(0), Arg::Object(b.graph())],
        10,
    );
    for r in [ps[1], ps[2]] {
        // Recruited where it stands beside the leader, then put back.
        let keep = h
            .server
            .players
            .values()
            .find(|p| p.character == r.1)
            .expect("in the world")
            .position;
        if let Some(p) = h.server.players.values_mut().find(|p| p.character == r.1) {
            p.position = at;
        }
        h.ask(ps[0].0, function::RECRUIT, &[Arg::LongInt(r.1)], 10);
        h.ask(r.0, function::ACCEPT, &[Arg::LongInt(ps[0].1)], 10);
        if let Some(p) = h.server.players.values_mut().find(|p| p.character == r.1) {
            p.position = keep;
        }
    }
    h.add_creatures(2);
    h.run(20);
    let level = level_of(&h, ps[0].1);
    let mut beetles: Vec<u64> = h
        .server
        .things
        .by_id
        .values()
        .filter(|t| t.creature.is_some())
        .map(|t| t.id)
        .collect();
    beetles.sort_unstable();
    for (beetle, group) in beetles.into_iter().zip([false, true]) {
        if let Some(t) = h.server.things.get_mut(beetle) {
            t.health = HEALTH;
            t.max_health = HEALTH;
            t.facts.group = group;
            if let Some(c) = t.creature.as_mut() {
                c.stats.death_xp = 1000;
                c.stats.death_focus = 20;
                c.stats.level = level;
                c.stats.attacks_first = false;
            }
        }
        for (_, id) in &ps {
            set_focus(&mut h, *id, 50);
        }
        let available = |h: &Harness, id: u64| {
            h.server
                .players
                .values()
                .find(|p| p.character == id)
                .and_then(|p| quals::long(&p.qualities, q::AVAILABLE_XP))
                .unwrap_or(0)
        };
        let before: Vec<u64> = ps.iter().map(|(_, id)| available(&h, *id)).collect();
        let from: Vec<usize> = ps.iter().map(|(x, _)| h.bots[*x].inbox.len()).collect();
        let clock = h.clock();
        combat::hurt_creature(&mut h.server, &mut h.net, beetle, ps[0].1, HEALTH, clock);
        h.run(10);
        // 20 x 0.45 = 9 to those within 100 m; 1000 x 0.45 = 450 (x 1.5 = 675 for a group).
        let xp = if group { 675 } else { 450 };
        for (i, f) in [(0, Some(59)), (1, Some(59)), (2, None)] {
            let got: Vec<Told> = told(&h, ps[i].0, from[i])
                .into_iter()
                .filter(|t| !matches!(t, Told::Total(_)))
                .collect();
            let want: Vec<Told> = f.map(Told::Focus).into_iter().collect();
            assert_eq!(got, want, "fellow {i}, group {group}");
            assert_eq!(
                available(&h, ps[i].1) - before[i],
                xp,
                "fellow {i}'s experience, group {group}"
            );
        }
    }
}
