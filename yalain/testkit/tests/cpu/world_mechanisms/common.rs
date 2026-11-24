//! What the tests of the world's general mechanisms share: a quest, its steps, a placed object
//! beside the player, and a player in the world under way at a phase.

use yalain_content::quests::{QuestDef, Step, Trigger, Update};
use yalain_content::{ContentObject, Description, Facts, Kind};
use yalain_testkit::Harness;
use yalain_world::state::QuestState;

pub(super) const OBJECT: u64 = 0x4002_9900_0000_0E21;
pub(super) const TEMPLATE: u32 = 0x4700_1154;
pub(super) const CELL: u32 = 0x0299_0109;
pub(super) const QUEST: u32 = 0x4000_0E21;
pub(super) const START: i64 = 1_700_000_000 + 120;

/// A static object of [`TEMPLATE`] beside where a player enters the test world.
pub(super) fn object() -> ContentObject {
    ContentObject {
        object: Some(OBJECT),
        template: TEMPLATE,
        description: Description {
            name_text: None,
            package: 0x13C1,
            look: 0x1F00_0ED5,
            enter_fx: 0,
            scale: None,
            name: None,
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell: CELL,
        origin: [61.0, 241.0, -44.6],
        rotation: [1.0, 0.0, 0.0, 0.0],
        kind: Kind::Static,
        label: String::new(),
        placed: "authored for a test".into(),
        destination: None,
        facts: Facts::default(),
        walks: None,
        gate: None,
    }
}

/// A quest of five phases with no time of its own.
pub(super) fn quest() -> QuestDef {
    QuestDef {
        id: QUEST,
        name: yalain_content::Words::Ref(0xA860_7CEC, 0x2500_17EE),
        description: (0x6C45_9364, 0x2500_17EE),
        icon: 0x4100_034B,
        play_fx: true,
        journals: (1..=5).map(|n| (n, 0x2500_17EE)).collect(),
        challenge: -999,
        duration: 0.0,
        duration_by_calendar: false,
        timed_phase: 0,
        retry: 0.0,
        xp: 0,
        cancellable: false,
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

/// A step of the quest at a phase, run by a use of [`TEMPLATE`].
pub(super) fn step(needs: u32, update: Update) -> Step {
    Step {
        key: String::new(),
        quest: QUEST,
        needs,
        trigger: Trigger::Use(TEMPLATE),
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
        told_refused: false,
        effects_quiet: false,
        wants: None,
        fellows: false,
        levels: None,
        xp_share: 0.0,
        to: None,
        transforms: None,
        chance: None,
        to_fellows: false,
        out_of_turn: false,
        skills: Vec::new(),
    }
}

/// The world connection of a player's character.
pub(super) fn world_of(h: &Harness, me: u64) -> yalain_net::ConnId {
    *h.server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0
}

/// A player's quest under way at a phase.
pub(super) fn under_way(h: &mut Harness, me: u64, phase: u32) {
    let world = world_of(h, me);
    if let Some(p) = h.server.players.get_mut(&world) {
        p.play.quests.push(QuestState {
            quest: QUEST,
            status: yalain_world::quests::status::UNDERWAY,
            phase,
            started: 0,
            updated: 0,
        });
    }
}

/// The phase of the player's quest (0: not under way).
pub(super) fn phase(h: &Harness, me: u64) -> u32 {
    yalain_world::quests::phase_of(&h.server.players[&world_of(h, me)].play.quests, QUEST)
}

/// The player uses the object.
pub(super) fn use_it(h: &mut Harness, me: u64) {
    let world = world_of(h, me);
    let clock = h.clock();
    yalain_world::quests::triggered(
        &mut h.server,
        &mut h.net,
        world,
        Trigger::Use(TEMPLATE),
        None,
        clock,
    );
    h.run(2);
}

/// Run the clock on to `unix`, a second at a time.
pub(super) fn run_to_unix(h: &mut Harness, unix: i64) {
    while h.unix < unix {
        h.step_by(1.0);
    }
    h.run(4);
}
