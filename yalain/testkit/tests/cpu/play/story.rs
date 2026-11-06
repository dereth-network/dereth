//! The story: a story shard bestows its scene and takes the player on; its protection ends after
//! the scenes are done; a new character holds the starting scenes; the act registry at entry.

use auberean_protocol::login::CharacterRef;
use auberean_protocol::script::{FunctionId, ScriptArgs, ServerCall};
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use yalain_content::story::{Keyframe, Scene, StoryTable};
use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind, Place};
use yalain_store::Store as _;
use yalain_testkit::play::{use_of, STEPS};
use yalain_testkit::Harness;
use yalain_world::places::function as trip;
use yalain_world::scenes::{function, protection};
use yalain_world::session::function::{MOVEMENT_FROZEN, SESSION_START};

const SHARD: u64 = 0x4002_9900_0000_0059;
const SCENE: u32 = 0x4000_0021;
const ACT: u32 = 0x4000_0008;
const STARTING: u32 = 0x4000_0025;

fn shard() -> ContentObject {
    ContentObject {
        object: Some(SHARD),
        template: 0x4700_1BFF,
        description: Description {
            name_text: None,
            package: 0x19C,
            look: 0x1F00_0ED5,
            enter_fx: 0,
            scale: None,
            name: None,
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell: 0x0299_0109,
        origin: [59.5, 240.2, -44.6],
        rotation: [1.0, 0.0, 0.0, 0.0],
        kind: Kind::Static,
        label: String::new(),
        placed: "authored for a test".into(),
        destination: Some(Place {
            cell: 0x7000_0001,
            origin: [10.0, 10.0, 50.0],
            rotation: [1.0, 0.0, 0.0, 0.0],
        }),
        facts: Facts::default(),
        walks: None,
        gate: None,
    }
}

fn story() -> StoryTable {
    let scene = |id: u32, act: u32, number: u32| Scene {
        id,
        act,
        number,
        name: (0xD413_85E6, 0x2500_18A3),
        hidden: false,
        playable: true,
        keyframes: vec![Keyframe {
            seconds: 3.0,
            picture: 0x4100_49E6,
            music: 0,
            voice: 0x2C00_0190,
            subtitle: Some((0xC96D_8B37, 0x2500_18A3)),
            tracks: 0x15,
        }],
    };
    let mut t = StoryTable::default();
    t.scenes.insert(SCENE, scene(SCENE, ACT, 1));
    t.scenes.insert(STARTING, scene(STARTING, 0x4000_0009, 1));
    t.shards.insert(SHARD, SCENE);
    t.at_start = vec![STARTING];
    t
}

fn world() -> Harness {
    let mut h = Harness::new();
    h.server.content = ContentPack {
        objects: vec![shard()],
        ..ContentPack::default()
    };
    h.server.story = story();
    h
}

fn put_beside_the_shard(h: &mut Harness, id: u64) {
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == id)
        .expect("in the world");
    p.position.cell = 0x0299_0109;
    p.position.origin = [59.06, 240.2, -44.6];
}

/// A new character holds the story's starting scenes: each is added as it first enters.
/// Behaviour: server.story.starting-scenes
#[test]
fn a_new_character_is_given_the_starting_scenes_as_it_first_enters() {
    let mut h = world();
    let b = h.at_character_list("one");
    let id = h.create_character(b, "one", "Elara");
    let from = h.bots[b].inbox.len();
    h.enter_world(b, "one", id);
    let c = h.calls(b, from);
    let start = c
        .iter()
        .position(|(f, _)| *f == SESSION_START)
        .expect("the session start");
    let added: Vec<usize> = c
        .iter()
        .enumerate()
        .filter(|(_, (f, _))| *f == function::STORY_QUEST)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(added.len(), 1, "one starting scene");
    assert!(added[0] > start);
    assert!(c[added[0]]
        .1
        .contains(&yalain_world::calls::Value::Int(STARTING)));
}

/// A vault's shard that has a quest completes it as the player comes out, before its use is
/// answered: the quest's pay and points, with no text and no update told; each time it is used, and
/// the scene stays one. A shard without one pays nothing.
/// Behaviour: server.story.shard-completes-its-quest
#[test]
fn a_vaults_shard_completes_its_quest_as_the_player_comes_out() {
    const QUEST: u32 = 0x4000_0033;
    let mut h = world();
    h.server.quests.quests = vec![yalain_content::quests::QuestDef {
        id: QUEST,
        name: (1, 2),
        description: (3, 4),
        icon: 0,
        play_fx: false,
        journals: Vec::new(),
        challenge: -999,
        duration: 0.0,
        timed_phase: 0,
        retry: 0.0,
        xp: 800,
        cancellable: false,
        placed: "authored for a test".into(),
        hidden: true,
        effects: Vec::new(),
        subquests: Vec::new(),
        min_level: 0,
        fails_on_death: false,
        once: false,
        fellows: yalain_content::quests::Fellows::default(),
        last_waits: false,
        hero_only: false,
        pay: yalain_content::quests::CompletionPay {
            points: 400,
            ..yalain_content::quests::CompletionPay::default()
        },
        required: None,
    }];
    let (a, ida) = h.player_in_world("one", "Elara");
    let conn = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == ida)
        .expect("in the world")
        .0;
    let trip = |h: &mut Harness| {
        put_beside_the_shard(h, ida);
        let from = h.bots[a].inbox.len();
        h.ask(
            a,
            yalain_world::requests::function::USE,
            &[use_of(SHARD, ida)],
            10,
        );
        let t = h.now.0;
        h.run_to(yalain_net::LocalTime(t + 10.5));
        h.run(3);
        // Into portal space: nothing paid yet.
        let paid = yalain_world::quals::long(&h.server.players[&conn].qualities, 300);
        h.run_to(yalain_net::LocalTime(t + 14.0));
        h.run(3);
        (from, paid)
    };
    let total = |h: &Harness| yalain_world::quals::long(&h.server.players[&conn].qualities, 300);
    let rating = |h: &Harness| h.server.players[&conn].play.quest_rating;
    // No quest named.
    let before = total(&h);
    trip(&mut h);
    assert_eq!((total(&h), rating(&h)), (before, 0), "no quest, no pay");
    h.server.story.shard_quests.insert(SHARD, QUEST);
    for times in 1..=2u32 {
        let before = total(&h).unwrap_or(0);
        let (from, at_scene) = trip(&mut h);
        assert_eq!(at_scene.unwrap_or(0), before, "not as the scene plays");
        assert_eq!(total(&h).unwrap_or(0), before + 800, "the quest's pay");
        assert_eq!(rating(&h), 400 * times, "the quest's points, each time");
        let f = h.functions(a, from);
        assert!(
            !f.contains(&yalain_world::quests::UPDATE_QUEST),
            "no update"
        );
        let out = f
            .iter()
            .position(|x| *x == trip::EXIT_PORTAL_SPACE)
            .expect("arrived");
        let answered = f
            .iter()
            .rposition(|x| *x == 0x8216_0021)
            .expect("the use answered");
        assert!(out < answered, "{f:x?}");
    }
    let held: Vec<_> = h.server.players[&conn]
        .play
        .scenes
        .iter()
        .filter(|s| **s == SCENE)
        .collect();
    assert_eq!(held.len(), 1, "the scene once");
}

/// Using a story shard plays the shard's behavior; ten seconds later the scene is added, the
/// player protected and played the scene and taken into portal space, coming out at the shard's
/// destination; the scenes done, the protection ends three and a half seconds later; the scene is
/// in the act registry the next time the player enters.
/// Behaviour: server.story.shard-bestows-its-scene
#[test]
fn a_story_shard_bestows_its_scene_and_takes_the_player_on() {
    let mut h = world();
    let (a, ida) = h.player_in_world("one", "Elara");
    put_beside_the_shard(&mut h, ida);
    let from = h.bots[a].inbox.len();
    h.ask(
        a,
        yalain_world::requests::function::USE,
        &[use_of(SHARD, ida)],
        10,
    );
    assert!(
        h.world_messages(a, from).iter().any(
            |m| matches!(m, ServerMessage::PlayBehavior(e) if e.action.camera_target == SHARD)
        ),
        "the shard's behavior, the camera on the shard"
    );
    assert!(
        !h.functions(a, from).contains(&function::PLAY_SCENES),
        "not yet"
    );
    let t = h.now.0;
    h.run_to(yalain_net::LocalTime(t + 10.5));
    h.run(3);
    let f = h.functions(a, from);
    let at = |x: u32| f.iter().position(|y| *y == x);
    let added = at(function::STORY_QUEST).expect("the scene added");
    let played = at(function::PLAY_SCENES).expect("the scene played");
    let into = at(trip::ENTER_PORTAL_SPACE).expect("into portal space");
    assert!(added < played && played < into, "{f:x?}");
    assert!(at(0x8208_0017).is_some(), "protected");
    h.run_to(yalain_net::LocalTime(t + 14.0));
    assert!(
        h.functions(a, from).contains(&trip::EXIT_PORTAL_SPACE),
        "arrived"
    );
    // The client's scenes are done (with the scene drawn): the protection ends a little later.
    let done_at = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &ClientMessage::ServerCall(ServerCall {
            function: FunctionId(function::DONE),
            args: ScriptArgs::from_values(&[]).expect("packs"),
        }),
    );
    h.run(10);
    assert!(!h.texts(a, done_at).contains(&(8, protection::ENDED.0)));
    let t = h.now.0;
    h.run_to(yalain_net::LocalTime(t + 3.6));
    h.run(3);
    assert!(h.texts(a, done_at).contains(&(8, protection::ENDED.0)));
    assert!(h.functions(a, done_at).contains(&MOVEMENT_FROZEN));
    assert!(
        h.functions(a, done_at).contains(&0x8208_0016),
        "the effect gone"
    );
    // The next entry's session start holds the scene in its act.
    let before = h.bots[a].inbox.len();
    h.bots[a].send(
        false,
        queue::LOGON,
        &ClientMessage::LogOff(CharacterRef { character: ida }),
    );
    assert!(h.run_until(STEPS, |h| h.bots[a].inbox[before..]
        .iter()
        .any(|r| matches!(r.message(), Some(ServerMessage::CharacterList(_))))));
    let p = h
        .server
        .store
        .world_state(ida)
        .expect("reads")
        .expect("kept");
    let play = yalain_world::state::PlayState::from_bytes(&p.play).expect("reads");
    assert!(play.scenes.contains(&SCENE));
    let reg = yalain_world::scenes::registry(&h.server, &play.scenes);
    assert!(reg.iter().any(|(act, v)| *act == ACT && v.contains(&SCENE)));
}
