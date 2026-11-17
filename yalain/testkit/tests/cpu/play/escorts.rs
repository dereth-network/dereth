//! Escorts: a quest step sets a creature to follow its player, as one of its pets. It follows, its
//! death fails the quest, a later phase or the quest's end takes it off with nothing failed, its
//! player's use of it runs the steps it triggers, and it fails the quest where its row says a
//! portal or distance does. Fixture: a three-phase quest whose den's use at the first phase makes a
//! lost pet follow, whose person's use at the second moves it on, and whose pet's use at the third
//! completes it; the pet's `[[pet]]` kind; a portal beside the start.

use auberean_protocol::queue;
use auberean_script::native_object::QuestInfo;
use auberean_script::wire::In;
use yalain_content::escorts::{Escort, EscortTable};
use yalain_content::quests::{QuestDef, QuestTable, Step, Trigger, Update};
use yalain_content::talk::Words;
use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind, Prop, Props};
use yalain_testkit::play::{server_call, use_of};
use yalain_testkit::Harness;
use yalain_world::calls::{Node, Value};
use yalain_world::pets::{self, function, PetKind};
use yalain_world::quests::status;

const UPDATE_QUEST: u32 = 0x8213_0024;
const QUEST: u32 = 0x8000_037A;
const DEN: u64 = 0x8130_0000_0000_0901;
const DEN_T: u32 = 0x4700_2EFD;
const PERSON: u64 = 0x8130_0000_0000_0902;
const PERSON_T: u32 = 0x4700_2EFB;
const PORTAL: u64 = 0x4002_9900_0000_0056;
const PET_T: u32 = 0x4700_341C;
/// What the escort's kind does by itself: take on what its player attacks.
const FLAGS: u32 = 0x8;

fn place(id: u64, template: u32, kind: Kind, origin: [f32; 3]) -> ContentObject {
    ContentObject {
        object: Some(id),
        template,
        description: Description {
            name_text: None,
            package: 0x103E,
            look: 0x1F00_0100,
            enter_fx: 0,
            scale: None,
            name: Some((0xA860_7CEC, 0x2500_28B9)),
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell: 0x0299_0109,
        origin,
        rotation: [1.0, 0.0, 0.0, 0.0],
        kind,
        label: String::new(),
        placed: "authored for a test".into(),
        destination: None,
        facts: Facts::default(),
        walks: None,
        gate: None,
    }
}

fn quest() -> QuestDef {
    QuestDef {
        duration_by_calendar: false,
        id: QUEST,
        name: Words::Ref(0xDC00_3F6C, 0x2500_13D7),
        description: (0x9A21_1010, 0x2500_13D7),
        icon: 0x4100_034B,
        play_fx: true,
        journals: (0..3).map(|i| (0xD338_6448 + i, 0x2500_13D7)).collect(),
        challenge: -999,
        duration: 0.0,
        timed_phase: 0,
        retry: 0.0,
        xp: 0,
        cancellable: true,
        placed: "authored for a test".into(),
        hidden: false,
        effects: Vec::new(),
        subquests: Vec::new(),
        min_level: 0,
        fails_on_death: false,
        fails_on_logout: false,
        fails_on_teleport: false,
        once: false,
        fellows: yalain_content::quests::Fellows::default(),
        ranges: Vec::new(),
        hero_only: false,
        pay: yalain_content::quests::CompletionPay::default(),
        required: None,
        cancel_wait: None,
    }
}

fn step(needs: u32, trigger: Trigger, update: Update) -> Step {
    Step {
        out_of_turn: false,
        skills: Vec::new(),
        key: format!("at-{needs}"),
        quest: QUEST,
        needs,
        trigger,
        tell: None,
        update,
        xp: 0,
        gives: Vec::new(),
        counts: Vec::new(),
        effects: Vec::new(),
        text: None,
        recipes: Vec::new(),
        placed: "authored for a test".into(),
        requires: Vec::new(),
        takes: Vec::new(),
        races: Vec::new(),
        effects_last: false,
        effects_quiet: false,
        wants: None,
        fellows: false,
        levels: None,
        xp_share: 0.0,
        to: None,
        transforms: None,
        chance: None,
        to_fellows: false,
    }
}

fn escort() -> Escort {
    Escort {
        template: PET_T,
        icon: 0,
        until: None,
        far: None,
        portal_fails: false,
    }
}

/// The den's use at the first phase makes the escort follow (as `escort` says); the person's use
/// at the second moves the quest to its third; the escort's use at the third completes it.
fn world(escort: Escort) -> Harness {
    let mut h = Harness::new();
    let mut props = Props::default();
    props
        .set("GameplayStatistics_Level", Prop::Int(30))
        .set("GameplayStatistics_MaxHealth", Prop::Int(750))
        .set("GameplayStatistics_MaxVigor", Prop::Int(750))
        .set("AICombat_NaturalDamage", Prop::Float(100.0))
        .set("AICombat_NaturalVariance", Prop::Float(0.2))
        .set("AI_PerceptionRadius", Prop::Float(1.0))
        .set("AI_HomesickRadius", Prop::Float(1.0));
    let mut pet = yalain_testkit::test_template(PET_T, 0x2838, props);
    pet.description.name = Some((0xA860_7CEC, 0x2500_2B3D));
    h.server.catalog.insert(pet);
    h.server
        .pets
        .read_rows([PetKind::new(PET_T, PET_T, FLAGS, &[])], []);
    let start = [60.0, 241.0, -44.6];
    let mut portal = place(PORTAL, 0x4700_1154, Kind::Static, start);
    portal.destination = Some(yalain_content::Place {
        cell: 0x7000_0001,
        origin: [10.0, 10.0, 50.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    });
    let den = step(1, Trigger::Use(DEN_T), Update::Advance(2));
    h.server.content = ContentPack {
        objects: vec![
            place(DEN, DEN_T, Kind::Static, [58.0, 241.0, -44.6]),
            place(PERSON, PERSON_T, Kind::Npc, [62.0, 241.0, -44.6]),
            portal,
        ],
        escorts: EscortTable {
            steps: vec![(den.clone(), escort)],
        },
        ..ContentPack::default()
    };
    h.server.quests = QuestTable {
        quests: vec![quest()],
        steps: vec![
            den,
            step(2, Trigger::Use(PERSON_T), Update::Advance(3)),
            step(3, Trigger::Use(PET_T), Update::Complete),
        ],
        gives: Vec::new(),
        rooms: Vec::new(),
    };
    h
}

/// A player in the world with the quest at its first phase.
fn player(h: &mut Harness) -> (usize, u64) {
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    for p in h.server.players.values_mut() {
        p.play.quests.push(yalain_world::state::QuestState {
            quest: QUEST,
            status: status::UNDERWAY,
            phase: 1,
            started: 0,
            updated: 0,
        });
    }
    (b, me)
}

fn conn_of(h: &Harness, me: u64) -> yalain_net::ConnId {
    *h.server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .map(|(c, _)| c)
        .expect("in the world")
}

/// Use one of the world's objects (its step's trigger), and let the world run a moment.
fn uses(h: &mut Harness, me: u64, template: u32) {
    let world = conn_of(h, me);
    let clock = h.clock();
    yalain_world::quests::triggered(
        &mut h.server,
        &mut h.net,
        world,
        Trigger::Use(template),
        None,
        clock,
    );
    h.run(5);
}

/// The player's status and phase of the quest.
fn held(h: &Harness, me: u64) -> Option<(u32, u32)> {
    h.server.players[&conn_of(h, me)]
        .play
        .quests
        .iter()
        .find(|q| q.quest == QUEST)
        .map(|q| (q.status, q.phase))
}

/// The pet a call names.
fn pet_of(v: &[Value]) -> Option<u64> {
    v.iter().find_map(|x| match x {
        Value::Long(l) => Some(*l),
        _ => None,
    })
}

/// The update kinds of the quest a client was told since `from`.
fn updates(h: &Harness, b: usize, from: usize) -> Vec<u32> {
    h.calls(b, from)
        .into_iter()
        .filter(|(f, _)| *f == UPDATE_QUEST)
        .filter_map(|(_, v)| {
            let record = match v.get(1)?.root()? {
                Node::Native { bytes, .. } => QuestInfo::unpack(&mut In::new(bytes)).ok()?,
                _ => return None,
            };
            match v.last() {
                Some(Value::Int(k)) if record.id == QUEST => Some(*k),
                _ => None,
            }
        })
        .collect()
}

/// The den used: the escort the player is told of, which then follows.
fn led(h: &mut Harness, b: usize, me: u64) -> u64 {
    let from = h.bots[b].inbox.len();
    uses(h, me, DEN_T);
    h.calls(b, from)
        .iter()
        .find(|(f, _)| *f == function::ADD)
        .and_then(|(_, v)| pet_of(v))
        .expect("an escort told of")
}

/// The den's step makes its escort: the player is told of it as a new pet (of the quest
/// companions' class, no icon), it appears beside the player naming it as its summoner, at its
/// template's level, and it keeps three meters short of its player as the player walks off. A
/// second use at the same phase makes no second one.
///
/// Behaviour: server.quests.escorts-follow
#[test]
fn a_steps_escort_is_told_as_a_pet_and_follows_its_player() {
    let mut h = world(escort());
    let (b, me) = player(&mut h);
    let from = h.bots[b].inbox.len();
    let pet = led(&mut h, b, me);
    let add = h
        .calls(b, from)
        .into_iter()
        .find(|(f, _)| *f == function::ADD)
        .expect("told");
    let data = add.1.iter().find_map(|v| match v.root() {
        Some(Node::Script { package, slots }) if *package == pets::PET_DATA => Some(slots.clone()),
        _ => None,
    });
    let words: Vec<u32> = data.expect("its pet data").iter().map(|s| s.1).collect();
    assert_eq!(
        &words[2..],
        &[yalain_world::escorts::CLASS, FLAGS],
        "the companions' class, its kind's flags"
    );
    assert!(add.1.contains(&Value::Int(0)), "no icon");
    assert_eq!(held(&h, me), Some((status::UNDERWAY, 2)));
    let t = h.server.things.get(pet).expect("in the world");
    assert_eq!(t.summoner, Some(me));
    assert_eq!(t.creature.as_ref().map(|c| c.stats.level), Some(30));
    assert!(h.server.pets.is_pet(pet));
    // Its player walks off: it follows.
    let conn = conn_of(&h, me);
    let start = h.server.players[&conn].position;
    let to =
        auberean_primitives::Vec3::new(start.origin[0] + 20.0, start.origin[1], start.origin[2]);
    let stamp = h.server.players[&conn].teleport_stamp;
    h.bots[b].send(
        true,
        queue::WEENIE,
        &yalain_testkit::play::report(start.cell, to, 0, yalain_world::world::IDLE_MOTION, stamp),
    );
    h.run(200);
    let owner_at = h.server.players[&conn].position;
    let at = h.server.things.get(pet).and_then(|t| t.at).expect("placed");
    let d = yalain_world::presence::distance(&at, &owner_at).expect("near");
    assert!((2.0..4.0).contains(&d), "three meters short: {d}");
    // One at a time.
    let from = h.bots[b].inbox.len();
    let clock = h.clock();
    let den = h.server.content.escorts.steps[0].0.clone();
    yalain_world::escorts::step_ran(&mut h.server, &mut h.net, conn, &den, clock);
    h.run(5);
    assert!(
        !h.functions(b, from).contains(&function::ADD),
        "no second escort"
    );
}

/// The escort killed: it is taken off its player's list and its quest fails, told as a failure.
/// The player's own death takes it the same way.
///
/// Behaviour: server.quests.escorts-death-fails
#[test]
fn an_escorts_death_fails_its_quest() {
    for by_owner in [false, true] {
        let mut h = world(escort());
        let (b, me) = player(&mut h);
        let pet = led(&mut h, b, me);
        let from = h.bots[b].inbox.len();
        let clock = h.clock();
        if by_owner {
            let conn = conn_of(&h, me);
            yalain_world::death::player_dies(&mut h.server, &mut h.net, conn, None, clock);
        } else {
            pets::dies(&mut h.server, &mut h.net, pet, 0, clock);
        }
        h.run(5);
        assert!(!h.server.pets.is_pet(pet));
        assert!(h.functions(b, from).contains(&function::REMOVE));
        assert_eq!(held(&h, me), Some((status::FAILED, 0)), "{by_owner}");
        assert_eq!(updates(&h, b, from), vec![0x4000_0000], "{by_owner}");
        assert!(h.server.escorts.by_pet.is_empty());
    }
}

/// The player's use of its escort is a person's use and runs the step it triggers: the quest
/// completes, and the escort is taken off the list and leaves the world in that tick, with nothing
/// failed. An escort that lasts until a phase goes, failing nothing, as the quest reaches it.
///
/// Behaviour: server.quests.escorts-end-with-their-phase
#[test]
fn an_escort_goes_with_nothing_failed_as_its_quest_ends_or_reaches_its_last_phase() {
    let mut h = world(escort());
    let (b, me) = player(&mut h);
    let pet = led(&mut h, b, me);
    uses(&mut h, me, PERSON_T);
    assert!(h.server.pets.is_pet(pet), "it stays at a later phase");
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(yalain_world::requests::function::USE, &[use_of(pet, me)]),
    );
    h.run(20);
    assert_eq!(held(&h, me), Some((status::COMPLETED, 0)), "petted home");
    assert_eq!(updates(&h, b, from), vec![0x3000_0000]);
    assert!(h.functions(b, from).contains(&function::REMOVE));
    assert!(!h.server.pets.is_pet(pet) && h.server.things.get(pet).is_none());
    // One that lasts until the third phase goes at the person's use.
    let mut h = world(Escort {
        until: Some(3),
        ..escort()
    });
    let (b, me) = player(&mut h);
    let pet = led(&mut h, b, me);
    let from = h.bots[b].inbox.len();
    uses(&mut h, me, PERSON_T);
    assert_eq!(held(&h, me), Some((status::UNDERWAY, 3)));
    assert!(h.functions(b, from).contains(&function::REMOVE));
    assert!(h.server.things.get(pet).is_none());
}

/// An escort whose row names a distance fails its quest once its player is farther from it than
/// that; one whose row says a portal fails it does so as its player comes out of portal space, and
/// goes; one that says nothing of portals comes along beside its player.
///
/// Behaviour: server.quests.escorts-lost
#[test]
fn an_escort_fails_its_quest_past_its_distance_or_through_a_portal_where_its_row_says_so() {
    let mut h = world(Escort {
        far: Some(30.0),
        ..escort()
    });
    let (b, me) = player(&mut h);
    let pet = led(&mut h, b, me);
    let conn = conn_of(&h, me);
    h.run(40);
    assert_eq!(held(&h, me), Some((status::UNDERWAY, 2)), "near enough");
    if let Some(p) = h.server.players.get_mut(&conn) {
        p.position.origin[1] -= 60.0;
    }
    h.run(80);
    assert_eq!(held(&h, me), Some((status::FAILED, 0)), "lost");
    assert!(!h.server.pets.is_pet(pet));
    for portal_fails in [true, false] {
        let mut h = world(Escort {
            portal_fails,
            ..escort()
        });
        let (b, me) = player(&mut h);
        let pet = led(&mut h, b, me);
        h.bots[b].send(
            true,
            queue::WEENIE,
            &server_call(yalain_world::requests::function::USE, &[use_of(PORTAL, me)]),
        );
        h.run(60);
        let conn = conn_of(&h, me);
        assert_eq!(h.server.players[&conn].position.cell, 0x7000_0001);
        if portal_fails {
            assert_eq!(held(&h, me), Some((status::FAILED, 0)));
            assert!(!h.server.pets.is_pet(pet));
        } else {
            assert_eq!(held(&h, me), Some((status::UNDERWAY, 2)));
            let owner_at = h.server.players[&conn].position;
            let at = h.server.things.get(pet).and_then(|t| t.at).expect("placed");
            let d = yalain_world::presence::distance(&at, &owner_at).expect("beside it");
            assert!(d < 3.0, "{d}");
        }
    }
}

/// A player leaving the world takes its escort out of it, its quest still under way; entering
/// again, it is told of its escort once more, beside it.
///
/// Behaviour: server.quests.escorts-come-back
#[test]
fn an_escort_leaves_with_its_player_and_follows_it_again_on_entry() {
    use auberean_protocol::login::CharacterRef;
    use auberean_protocol::ClientMessage;
    use auberean_protocol::ServerMessage;
    let mut h = world(escort());
    let (b, me) = player(&mut h);
    let pet = led(&mut h, b, me);
    let conn = conn_of(&h, me);
    yalain_world::world::keep_player(&mut h.server, conn);
    let before = h.bots[b].inbox.len();
    h.bots[b].send(
        false,
        queue::LOGON,
        &ClientMessage::LogOff(CharacterRef { character: me }),
    );
    assert!(h.run_until(400, |h| h.bots[b].inbox[before..]
        .iter()
        .any(|r| matches!(r.message(), Some(ServerMessage::CharacterList(_))))));
    assert!(h.server.things.get(pet).is_none(), "out of the world");
    assert!(h.server.escorts.by_pet.is_empty());
    let from = h.bots[b].inbox.len();
    h.enter_world(b, "one", me);
    assert_eq!(held(&h, me), Some((status::UNDERWAY, 2)));
    let again = h
        .calls(b, from)
        .iter()
        .find(|(f, _)| *f == function::ADD)
        .and_then(|(_, v)| pet_of(v))
        .expect("told of it again");
    let conn = conn_of(&h, me);
    let owner_at = h.server.players[&conn].position;
    let at = h
        .server
        .things
        .get(again)
        .and_then(|t| t.at)
        .expect("placed");
    let d = yalain_world::presence::distance(&at, &owner_at).expect("beside it");
    assert!(d < 3.0, "{d}");
    assert!(h.server.escorts.follows(again, me));
}

/// A step that makes an escort and sends its player somewhere takes the escort along on that trip,
/// however its row says portals fail it; the player's next trip fails it as the row says.
///
/// Behaviour: server.quests.escorts-lost
#[test]
fn an_escort_comes_along_on_the_trip_of_the_step_that_made_it() {
    const THERE: u32 = 0x7000_0001;
    let mut h = world(Escort {
        portal_fails: true,
        ..escort()
    });
    let den = Step {
        to: Some(yalain_content::quests::Place {
            cell: THERE,
            origin: [10.0, 10.0, 50.0],
            heading: 0,
        }),
        ..step(1, Trigger::Use(DEN_T), Update::Advance(2))
    };
    h.server.quests.steps[0] = den.clone();
    h.server.content.escorts.steps[0].0 = den;
    let (b, me) = player(&mut h);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(yalain_world::requests::function::USE, &[use_of(DEN, me)]),
    );
    h.run(400);
    let pet = h
        .calls(b, from)
        .iter()
        .find(|(f, _)| *f == function::ADD)
        .and_then(|(_, v)| pet_of(v))
        .expect("an escort told of");
    let conn = conn_of(&h, me);
    assert_eq!(h.server.players[&conn].position.cell, THERE, "sent");
    assert_eq!(held(&h, me), Some((status::UNDERWAY, 2)), "nothing failed");
    let owner_at = h.server.players[&conn].position;
    let at = h.server.things.get(pet).and_then(|t| t.at).expect("placed");
    let d = yalain_world::presence::distance(&at, &owner_at).expect("beside it");
    assert!(d < 3.0, "come along: {d}");
    // The next trip is its row's.
    let clock = h.clock();
    yalain_world::escorts::owner_arrives(&mut h.server, &mut h.net, conn, clock);
    h.run(5);
    assert_eq!(held(&h, me), Some((status::FAILED, 0)));
    assert!(!h.server.pets.is_pet(pet));
}
