//! The allegiance: an oath sworn and taken, profiles asked for, an allegiance broken, the
//! allegiance as a member enters again, and experience passed up to a patron.

use auberean_protocol::login::CharacterRef;
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use auberean_script::args::Arg;
use yalain_testkit::play::STEPS;
use yalain_testkit::Harness;
use yalain_world::allegiance::{error, function, text};
use yalain_world::calls::{Node, Value};

/// The members a profile names: the member, its patron, its monarch and its vassals' ids, read
/// from the profile's graph (the member first after the patron, as the record holds them).
fn profile_ids(v: &[Value]) -> Vec<u64> {
    let Some(Value::Graph(g)) = v.first() else {
        return Vec::new();
    };
    g.iter()
        .filter_map(|(_, n)| match n {
            Node::Script { package, slots } if *package == 0x1B1 => {
                Some(u64::from(slots[10].1) << 32 | u64::from(slots[9].1))
            }
            _ => None,
        })
        .collect()
}

/// The profile's slots (the allegiance's record).
fn profile_slots(v: &[Value]) -> Vec<(u8, u32)> {
    match v.first().and_then(Value::root) {
        Some(Node::Script { package, slots }) if *package == 0x1B5 => slots.clone(),
        _ => Vec::new(),
    }
}

fn two() -> (Harness, (usize, u64), (usize, u64)) {
    let mut h = Harness::new();
    let a = h.player_in_world("one", "Vassalia");
    let b = h.player_in_world("two", "Patronus");
    (h, a, b)
}

/// Swear one player to another, the patron taking the oath.
fn sworn(h: &mut Harness, (va, vid): (usize, u64), (pa, pid): (usize, u64)) {
    h.ask(va, function::SWEAR, &[Arg::LongInt(pid)], 10);
    h.ask(pa, function::ACCEPT_VASSAL, &[Arg::LongInt(vid)], 10);
    let _ = vid;
}

/// Swearing to a player beside one asks it; taking the oath tells the patron who swore and its
/// rank (a new monarch: its allegiance's chat room too), tells the vassal the chat room, to whom
/// it swore and its rank, and gives each its profile: the patron's with the vassal under it, the
/// vassal's with the patron over it (the monarch's own profile names no patron and no monarch).
/// Behaviour: server.allegiance.an-oath-sworn-and-taken
#[test]
fn an_oath_sworn_and_taken_tells_both_and_gives_each_its_profile() {
    let (mut h, (va, vid), (pa, pid)) = two();
    let (fv, fp) = (h.bots[va].inbox.len(), h.bots[pa].inbox.len());
    h.ask(va, function::SWEAR, &[Arg::LongInt(pid)], 10);
    let asked = h.calls(pa, fp);
    let (_, req) = asked
        .iter()
        .find(|(f, _)| *f == function::REQUEST_VASSAL)
        .expect("the patron is asked");
    assert!(req.contains(&Value::Long(vid)), "{req:?}");
    let (fv2, fp2) = (h.bots[va].inbox.len(), h.bots[pa].inbox.len());
    h.ask(pa, function::ACCEPT_VASSAL, &[Arg::LongInt(vid)], 10);
    let p = h.functions(pa, fp2);
    let at = |v: &[u32], f: u32| v.iter().position(|x| *x == f);
    let done = at(&p, function::PATRON_SWORE).expect("the patron is told");
    let prof = at(&p, function::PROFILE).expect("the patron's profile");
    assert!(done < prof, "{p:x?}");
    assert!(
        at(&p, function::CHAT_ROOM).is_some(),
        "a new monarch hears its room"
    );
    let pdone = &h.calls(pa, fp2)[done].1;
    assert!(pdone.contains(&Value::Int(error::DONE)) && pdone.contains(&Value::Long(vid)));
    let v = h.functions(va, fv2);
    let room = at(&v, function::CHAT_ROOM).expect("the vassal hears its room");
    let vdone = at(&v, function::VASSAL_SWORE).expect("the vassal is told");
    let vprof = at(&v, function::PROFILE).expect("the vassal's profile");
    assert!(room < vdone && vdone < vprof, "{v:x?}");
    assert!(
        h.texts(va, fv2).contains(&(1, text::RANK_UP.0)),
        "the vassal is told its rank"
    );
    // The profiles: the patron's names the vassal; the vassal's its patron and monarch.
    let pc = h.calls(pa, fp2);
    let pv = &pc[prof].1;
    let ids = profile_ids(pv);
    assert_eq!(ids, vec![pid, vid], "the monarch, then its vassal");
    let slots = profile_slots(pv);
    assert_eq!(slots[1].1, 0xFFFF_FFFF, "no patron over a monarch");
    assert_eq!(slots[3].1, 0xFFFF_FFFF, "no monarch named for the monarch");
    assert_eq!(slots[11], (4, 2), "two members");
    let vc = h.calls(va, fv2);
    let ids = profile_ids(&vc[vprof].1);
    assert_eq!(
        ids,
        vec![pid, vid],
        "patron, member; the patron is the monarch"
    );
    assert_eq!(
        profile_slots(&vc[vprof].1)[3].1,
        0xFFFF_FFFF,
        "no monarch named apart from the patron"
    );
    let _ = fv;
}

/// Asking for a member's profile is answered with the profile centred on that member; asking for
/// nothing (id 0) is not answered; one in no allegiance is sent the empty profile.
/// Behaviour: server.allegiance.profiles-asked-for
#[test]
fn a_profile_asked_for_is_centred_on_the_member_and_nothing_is_not_answered() {
    let (mut h, a, b) = two();
    let (va, vid) = a;
    let (_, pid) = b;
    let from = h.bots[va].inbox.len();
    h.ask(va, function::QUERY, &[Arg::LongInt(vid)], 10);
    let c = h.calls(va, from);
    let (_, v) = c
        .iter()
        .find(|(f, _)| *f == function::PROFILE)
        .expect("answered");
    assert!(
        profile_ids(v).is_empty(),
        "no allegiance: the empty profile"
    );
    sworn(&mut h, a, b);
    let from = h.bots[va].inbox.len();
    h.ask(va, function::QUERY, &[Arg::LongInt(pid)], 10);
    let c = h.calls(va, from);
    let (_, v) = c
        .iter()
        .find(|(f, _)| *f == function::PROFILE)
        .expect("answered");
    assert_eq!(profile_ids(v), vec![pid, vid], "centred on the patron");
    let from = h.bots[va].inbox.len();
    h.ask(va, function::QUERY, &[Arg::LongInt(0)], 10);
    assert!(
        !h.functions(va, from).contains(&function::PROFILE),
        "nothing asked, nothing answered"
    );
}

/// A native of a graph by its id: its type and bytes.
fn native(g: &[(u32, Node)], id: u32) -> Option<(u16, &[u8])> {
    g.iter().find_map(|(i, n)| match n {
        Node::Native { ty, bytes } if *i == id => Some((*ty, bytes.as_slice())),
        _ => None,
    })
}

/// A packed set of 64-bit ids: its bucket count's place and its ids.
fn id_set(bytes: &[u8]) -> (u8, Vec<u64>) {
    let head = u32::from_le_bytes(bytes[..4].try_into().unwrap());
    let ids = bytes[4..]
        .as_chunks::<8>()
        .0
        .iter()
        .map(|c| u64::from_le_bytes(*c))
        .collect();
    ((head >> 24) as u8, ids)
}

/// A string argument.
fn string_arg(text: &str) -> Arg {
    let mut b = yalain_world::graph::Builder::new();
    b.native(
        0x76,
        auberean_script::native_object::NativeObject::Text(text.encode_utf16().collect()),
    );
    Arg::Object(b.graph())
}

/// A profile names its allegiance's officers: the monarch, whose own data says it is one; a
/// vassal is not.
/// Behaviour: server.allegiance.the-monarch-is-an-officer
#[test]
fn a_profile_names_the_monarch_its_allegiances_officer() {
    let (mut h, a, b) = two();
    let (va, vid) = a;
    let (_, pid) = b;
    sworn(&mut h, a, b);
    let from = h.bots[va].inbox.len();
    h.ask(va, function::QUERY, &[Arg::LongInt(vid)], 10);
    let c = h.calls(va, from);
    let (_, v) = c
        .iter()
        .find(|(f, _)| *f == function::PROFILE)
        .expect("answered");
    let Some(Value::Graph(g)) = v.first() else {
        panic!("a profile graph");
    };
    let slots = profile_slots(v);
    let (ty, bytes) = native(g, slots[12].1).expect("the officers");
    assert_eq!(ty, 0x1AC8, "a set of ids");
    assert_eq!(id_set(bytes), (2, vec![pid]), "the monarch alone");
    for (_, n) in g {
        if let Node::Script { package, slots } = n {
            if *package == 0x1B1 {
                let id = u64::from(slots[10].1) << 32 | u64::from(slots[9].1);
                assert_eq!(slots[8].1, u32::from(id == pid), "officer flag of {id:x}");
            }
        }
    }
}

/// When a vassal swears, every other member of the allegiance in the world is sent its profile
/// again with the allegiance's new size; a player outside it is not.
/// Behaviour: server.allegiance.others-see-it-grow
#[test]
fn every_other_member_in_the_world_is_sent_its_profile_when_one_swears() {
    let (mut h, a, b) = two();
    let (ma, _) = b;
    sworn(&mut h, a, b);
    let c = h.player_in_world("three", "Novicia");
    let d = h.player_in_world("four", "Strangera");
    let (fm, fd) = (h.bots[ma].inbox.len(), h.bots[d.0].inbox.len());
    sworn(&mut h, c, a);
    let mc = h.calls(ma, fm);
    let (_, v) = mc
        .iter()
        .find(|(f, _)| *f == function::PROFILE)
        .expect("the monarch is sent its profile");
    assert_eq!(profile_slots(v)[11], (4, 3), "three members now");
    assert!(
        !h.functions(d.0, fd).contains(&function::PROFILE),
        "no profile outside the allegiance"
    );
}

/// The tree's export sends the asker its whole allegiance and the file's name: every member's
/// node with its patron and its vassals, the tables of nodes by name (filed by the name's hash)
/// and by id, the allegiance's chat room.
/// Behaviour: server.allegiance.tree-exported
#[test]
fn the_tree_export_sends_the_whole_allegiance_and_the_files_name() {
    let (mut h, a, b) = two();
    let (ma, mid) = b;
    let (_, vid) = a;
    sworn(&mut h, a, b);
    let c = h.player_in_world("three", "Novicia");
    sworn(&mut h, c, a);
    let from = h.bots[ma].inbox.len();
    h.ask(
        ma,
        function::TREE_EXPORT,
        &[string_arg("allegianceexport.xml")],
        10,
    );
    let calls = h.calls(ma, from);
    let (_, v) = calls
        .iter()
        .find(|(f, _)| *f == function::EXPORT)
        .expect("the export");
    let file = v.iter().find_map(|x| match x.root() {
        Some(Node::Text(t)) => Some(t.clone()),
        _ => None,
    });
    assert_eq!(file.as_deref(), Some("allegianceexport.xml"));
    let g = v
        .iter()
        .find_map(|x| match x {
            Value::Graph(g) if matches!(x.root(), Some(Node::Script { package: 0x1B2, .. })) => {
                Some(g)
            }
            _ => None,
        })
        .expect("the allegiance");
    let Some(Node::Script { slots: root, .. }) = g.first().map(|(_, n)| n) else {
        panic!("its root");
    };
    let room = h.calls(ma, 0).iter().find_map(|(f, v)| {
        (*f == function::CHAT_ROOM).then(|| match v.first() {
            Some(Value::Int(r)) => *r,
            _ => 0,
        })
    });
    assert_eq!(Some(root[0].1), room, "the allegiance's chat room");
    // Each node: its data's id, its patron node, its vassal nodes.
    let node = |id: u32| {
        g.iter().find_map(|(i, n)| match n {
            Node::Script {
                package: 0x1B3,
                slots,
            } if *i == id => Some(slots.clone()),
            _ => None,
        })
    };
    let member = |n: &[(u8, u32)]| {
        g.iter()
            .find_map(|(i, x)| match x {
                Node::Script {
                    package: 0x1B1,
                    slots,
                } if *i == n[2].1 => Some(u64::from(slots[10].1) << 32 | u64::from(slots[9].1)),
                _ => None,
            })
            .unwrap_or(0)
    };
    let vassals = |n: &[(u8, u32)]| -> Vec<u32> {
        let (_, bytes) = native(g, n[0].1).expect("a list");
        bytes[4..]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| u32::from_le_bytes(*c))
            .collect()
    };
    let top = node(root[3].1).expect("the monarch's node");
    assert_eq!(member(&top), mid);
    assert_eq!(top[1].1, 0xFFFF_FFFF, "no patron over the monarch");
    let under = vassals(&top);
    assert_eq!(under.len(), 1);
    let second = node(under[0]).expect("a vassal's node");
    assert_eq!(member(&second), vid);
    assert_eq!(second[1].1, root[3].1, "its patron's node");
    let third = node(vassals(&second)[0]).expect("its vassal's node");
    assert_eq!(member(&third), c.1);
    // The tables: three names and three ids.
    let (ty, names) = native(g, root[8].1).expect("by name");
    assert_eq!(ty, 0x4D);
    assert_eq!(
        u32::from_le_bytes(names[..4].try_into().unwrap()),
        3 | 7 << 24
    );
    // Each name filed under its hash (1,531 buckets), the hash not used.
    for e in names[4..].as_chunks::<16>().0 {
        let w = |at: usize| u32::from_le_bytes(e[at..at + 4].try_into().unwrap());
        let name = g
            .iter()
            .find_map(|(i, n)| match n {
                Node::Text(t) if *i == w(0) => Some(t.clone()),
                _ => None,
            })
            .expect("a name");
        assert_eq!(
            w(4),
            yalain_world::allegiance::name_hash(&name) % 1531,
            "{name}"
        );
        assert_eq!(w(8), 0);
    }
    let (ty, ids) = native(g, root[9].1).expect("by id");
    assert_eq!(ty, 0x40);
    assert_eq!(
        u32::from_le_bytes(ids[..4].try_into().unwrap()),
        3 | 7 << 24
    );
}

/// Breaking with a patron tells the player whom it broke with and sends it the empty profile;
/// the patron, left with no vassal, is sent the empty profile too.
/// Behaviour: server.allegiance.broken
#[test]
fn breaking_with_a_patron_tells_whom_and_empties_both_profiles() {
    let (mut h, a, b) = two();
    let (va, _) = a;
    let (pa, pid) = b;
    sworn(&mut h, a, b);
    let (fv, fp) = (h.bots[va].inbox.len(), h.bots[pa].inbox.len());
    h.ask(va, function::BREAK, &[Arg::LongInt(pid)], 10);
    assert!(h.texts(va, fv).contains(&(8, text::BROKEN.0)), "told whom");
    for (b, from) in [(va, fv), (pa, fp)] {
        let c = h.calls(b, from);
        let (_, v) = c
            .iter()
            .find(|(f, _)| *f == function::PROFILE)
            .expect("a profile");
        assert!(profile_ids(v).is_empty(), "the empty profile");
    }
}

/// A member entering the world again hears its allegiance's chat room twice before its profile,
/// which names its patron; one in no allegiance hears no room.
/// Behaviour: server.allegiance.at-entry
#[test]
fn a_member_entering_again_hears_its_room_twice_then_its_profile() {
    let (mut h, a, b) = two();
    let (va, vid) = a;
    let (_, pid) = b;
    sworn(&mut h, a, b);
    let before = h.bots[va].inbox.len();
    h.bots[va].send(
        false,
        queue::LOGON,
        &ClientMessage::LogOff(CharacterRef { character: vid }),
    );
    assert!(h.run_until(STEPS, |h| h.bots[va].inbox[before..]
        .iter()
        .any(|r| matches!(r.message(), Some(ServerMessage::CharacterList(_))))));
    let from = h.bots[va].inbox.len();
    h.enter_world(va, "one", vid);
    let f = h.functions(va, from);
    let rooms: Vec<usize> = f
        .iter()
        .enumerate()
        .filter(|(_, x)| **x == function::CHAT_ROOM)
        .map(|(i, _)| i)
        .collect();
    let prof = f
        .iter()
        .position(|x| *x == function::PROFILE)
        .expect("a profile");
    assert_eq!(rooms.len(), 2, "{f:x?}");
    assert!(rooms[1] < prof);
    let c = h.calls(va, from);
    assert_eq!(
        profile_ids(&c[prof].1),
        vec![pid, vid],
        "its patron, the monarch"
    );
}

fn levels() -> yalain_world::levels::LevelTable {
    use yalain_world::levels::Level;
    yalain_world::levels::LevelTable {
        levels: [0u64, 1_000, 3_000, 6_000]
            .iter()
            .map(|xp| Level {
                xp: *xp,
                health: 100,
                vigor: 100,
                skill_credits: 1,
                hero_credits: 0,
            })
            .collect(),
    }
}

fn award(h: &mut Harness, id: u64, amount: u64) {
    let w = yalain_world::social::world_of(&h.server, id).expect("in the world");
    yalain_world::progress::award(
        &mut h.server,
        &mut h.net,
        w,
        amount,
        yalain_world::progress::Award::Step,
    );
    h.run(5);
}

/// A quarter of what a vassal earns goes into its patron's pool; a pool reaching a fifth of the
/// patron's next level's experience is paid to the patron (up to a third and a half of it for the
/// level) with a text naming the vassal; a patron away is paid as it enters, told what its vassals
/// earned for it.
/// Behaviour: server.allegiance.experience-passed-up
#[test]
fn a_vassals_experience_is_pooled_and_paid_to_its_patron_up_to_the_levels_cap() {
    let (mut h, a, b) = two();
    h.server.levels = levels();
    let (_, vid) = a;
    let (pa, pid) = b;
    sworn(&mut h, a, b);
    // Away, the patron is paid as it enters: 800 earned, 200 pooled (the threshold) and paid.
    let before = h.bots[pa].inbox.len();
    h.bots[pa].send(
        false,
        queue::LOGON,
        &ClientMessage::LogOff(CharacterRef { character: pid }),
    );
    assert!(h.run_until(STEPS, |h| h.bots[pa].inbox[before..]
        .iter()
        .any(|r| matches!(r.message(), Some(ServerMessage::CharacterList(_))))));
    award(&mut h, vid, 800);
    let from = h.bots[pa].inbox.len();
    h.enter_world(pa, "two", pid);
    assert!(h.texts(pa, from).contains(&(8, text::AWAY_EARNED.0)));
    let from = h.bots[pa].inbox.len();
    // 1,000 earned: 250 pooled, over the threshold, paid up to the level's cap of 350 (150 of
    // it).
    award(&mut h, vid, 1_000);
    assert!(h.texts(pa, from).contains(&(8, text::VASSAL_EARNED.0)));
    let total = |h: &Harness| {
        let p = h
            .server
            .players
            .values()
            .find(|p| p.character == pid)
            .expect("patron");
        yalain_world::quals::long(&p.qualities, 300).unwrap_or(0)
    };
    assert_eq!(total(&h), 350);
    // Another 1,000: 250 pooled, but nothing left under the level's cap.
    award(&mut h, vid, 1_000);
    assert_eq!(total(&h), 350);
}

/// A store edited by hand into a circle of oaths still has an end: every walk counts each member
/// once, and none of them hangs.
/// Behaviour: server.allegiance.a-stored-circle-has-an-end
#[test]
fn a_stored_circle_of_oaths_is_walked_once_round() {
    use yalain_world::allegiance::{Allegiances, Node as Member};
    let mut a = Allegiances::default();
    for (id, patron) in [(1, 3), (2, 1), (3, 2), (4, 3)] {
        a.nodes.insert(
            id,
            Member {
                patron,
                ..Member::default()
            },
        );
    }
    let mut members = a.tree(1);
    members.sort_unstable();
    assert_eq!(members, [1, 2, 3, 4]);
    assert_eq!(a.size(1), 4);
    assert!(a.rank(1) >= 1);
    assert_eq!(a.monarch(1), None);
}
