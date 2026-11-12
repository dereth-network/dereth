//! Skills granted by an effect: a world's rule makes an effect train its player a skill for good,
//! told as the install's grants are; a quest step gated by an act runs only once the player holds
//! the act whole, so a person whose use completes a hidden quest with such an effect grants its
//! skill once the act is whole, and says its own line at a use otherwise.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use auberean_script::native_object::{TextInfo, VariableValue};
use yalain_content::quests::{QuestDef, QuestTable, Step, Trigger, Update};
use yalain_content::story::{Scene, StoryTable};
use yalain_content::talk::{How, Line, Role, TalkTable, When};
use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind, Words};
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::{self, Node};
use yalain_world::effect_defs::{EffectDef, Kind as EffectKind, Scale};
use yalain_world::effects::{self, learned, Cast, On};
use yalain_world::graph::{Builder, Slot};
use yalain_world::skills::{function, info_update, mask, Held, SkillDef};

const USE: u32 = 0x8216_0016;
const SHOW_TEXT: u32 = 0x8205_0011;

/// The person, its template and its name.
const PERSON: u64 = 0x8130_0000_0000_0951;
const PERSON_T: u32 = 0x4700_2F68;
const PERSON_NAME: (u32, u32) = (0xA860_7CEC, 0x2500_28F2);
/// Its line at a use that moves nothing.
const NO_NOTE: u32 = 0x6369_1D8F;

/// The hidden quest its use completes, the act that must be whole, the effect it puts on and the
/// skill that trains, the effect's own table and appearance.
const QUEST: u32 = 0x4000_0132;
const ACT: u32 = 0x4000_0005;
const EFFECT: u32 = 0x6F00_1A98;
const SKILL: u32 = 925;
const EFFECT_TABLE: u32 = 0x2500_10AA;
const FX: u32 = 0x4100_0499;

/// The act's scenes: seven shown and one hidden.
const SCENES: [u32; 8] = [
    0x4000_000A,
    0x4000_000B,
    0x4000_000C,
    0x4000_000D,
    0x4000_000E,
    0x4000_000F,
    0x4000_0010,
    0x4000_0011,
];

fn person() -> ContentObject {
    ContentObject {
        object: Some(PERSON),
        template: PERSON_T,
        description: Description {
            name_text: None,
            package: 0x103E,
            look: 0x1F00_0100,
            enter_fx: 0,
            scale: None,
            name: Some(PERSON_NAME),
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell: 0x0299_0109,
        origin: [61.0, 241.0, -44.6],
        rotation: [1.0, 0.0, 0.0, 0.0],
        kind: Kind::Npc,
        label: String::new(),
        placed: "authored for a test".into(),
        destination: None,
        facts: Facts::default(),
        walks: None,
        gate: None,
    }
}

fn hidden_quest() -> QuestDef {
    QuestDef {
        id: QUEST,
        name: Words::Ref(0, 0),
        description: (0, 0),
        icon: 0x4100_034B,
        play_fx: true,
        journals: Vec::new(),
        challenge: 0,
        duration: 0.0,
        timed_phase: 0,
        retry: 0.0,
        xp: 0,
        cancellable: false,
        placed: "authored for a test".into(),
        hidden: true,
        effects: Vec::new(),
        subquests: Vec::new(),
        min_level: 0,
        fails_on_death: false,
        once: true,
        fellows: yalain_content::quests::Fellows::default(),
        last_waits: false,
        hero_only: false,
        pay: yalain_content::quests::CompletionPay::default(),
        required: None,
        cancel_wait: None,
    }
}

fn grant_step() -> Step {
    Step {
        key: String::new(),
        quest: QUEST,
        needs: 0,
        trigger: Trigger::Use(PERSON_T),
        tell: None,
        update: Update::Complete,
        xp: 0,
        gives: Vec::new(),
        counts: Vec::new(),
        effects: vec![EFFECT],
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

fn granting(skill: u32) -> EffectDef {
    EffectDef {
        state: EFFECT,
        kind: EffectKind::GrantSkill(skill),
        duration: Scale(Vec::new()),
        priority: Scale(Vec::new()),
        fx: FX,
        external_flags: 0,
        internal_flags: 0x4140_0013,
        ends_at_logout: false,
        ends_at_teleport: false,
        enum_value: 3068,
        eq_class: 0,
        name: Some((0xA860_7CEC, EFFECT_TABLE)),
    }
}

/// A world of the person and its line, the act's scenes, the skill and the effect's definition;
/// with `rows`, the step its use runs, gated by the act.
fn world(rows: bool) -> Harness {
    let mut h = Harness::new();
    h.server.content = ContentPack {
        objects: vec![person()],
        ..ContentPack::default()
    };
    h.server.talk = TalkTable {
        lines: vec![Line {
            who: PERSON_T,
            object: None,
            words: Words::Ref(NO_NOTE, PERSON_NAME.1),
            role: Some(Role::RefuseUse),
            how: How::Emote,
            when: When::Use,
            quest: None,
            update: None,
            phase: None,
            item: None,
            order: 0,
            measured: true,
            placed: "authored for a test".into(),
        }],
    };
    let mut story = StoryTable::default();
    for (i, id) in SCENES.iter().enumerate() {
        story.scenes.insert(
            *id,
            Scene {
                id: *id,
                act: ACT,
                number: u32::try_from(i).expect("few") + 1,
                hidden: i == 7,
                ..Scene::default()
            },
        );
    }
    let step = grant_step();
    if rows {
        story.step_acts.push((step.clone(), ACT));
    }
    h.server.story = story;
    h.server.quests = QuestTable {
        quests: vec![hidden_quest()],
        steps: if rows { vec![step] } else { Vec::new() },
        gives: Vec::new(),
        rooms: Vec::new(),
    };
    h.server.skill_table.skills.insert(
        SKILL,
        SkillDef {
            id: SKILL,
            flags: 0x4C,
            ..SkillDef::default()
        },
    );
    h.server
        .effect_table
        .effects
        .insert(EFFECT, granting(SKILL));
    // Its table holds its line, as the install's does.
    h.server
        .effect_table
        .lines
        .insert(EFFECT, (learned::LINE, EFFECT_TABLE));
    h
}

fn world_of(h: &Harness, me: u64) -> yalain_net::ConnId {
    *h.server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .map(|(c, _)| c)
        .expect("in the world")
}

/// Give a player these of the act's scenes.
fn holding(h: &mut Harness, me: u64, scenes: &[u32]) {
    let w = world_of(h, me);
    h.server
        .players
        .get_mut(&w)
        .expect("in the world")
        .play
        .scenes = scenes.to_vec();
}

fn use_of(item: u64, player: u64) -> Arg {
    let mut b = Builder::new();
    b.object(
        0x245,
        &[
            Slot::Ref(None),
            Slot::Ref(None),
            Slot::Long(item),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Long(player),
            Slot::Int((-1.0f32).to_bits()),
            Slot::Long(0),
            Slot::Int(0),
            Slot::Long(0),
            Slot::Int(0),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Int(316),
            Slot::Int(0),
            Slot::Int(0),
        ],
    );
    Arg::Object(b.graph())
}

fn use_it(h: &mut Harness, b: usize, me: u64) -> usize {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_of(PERSON, me)]),
    );
    h.run(20);
    from
}

/// The texts a client was shown since `from`.
fn texts(h: &Harness, b: usize, from: usize) -> Vec<TextInfo> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == SHOW_TEXT => c
                .args
                .as_ref()
                .and_then(|a| calls::values(&a.0).ok())?
                .iter()
                .find_map(|x| match x.root() {
                    Some(Node::TextInfo(t)) => Some(t.clone()),
                    _ => None,
                }),
            _ => None,
        })
        .collect()
}

/// A text's variable that holds another text: its string and table.
fn var(t: &TextInfo, id: u32) -> Option<(u32, u32)> {
    t.variables.iter().find_map(|v| match &v.value {
        VariableValue::Nested(n) if v.id == id => Some((n.string_id, n.table)),
        _ => None,
    })
}

/// What a grant tells since `from`: the repository, a skill's information and the effect's text in
/// the order they came, and whether its appearance was played (that travels apart, with what is
/// seen).
fn grant_order(h: &Harness, b: usize, from: usize) -> (Vec<&'static str>, bool) {
    let msgs = h.world_messages(b, from);
    let told = msgs
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == function::UPDATE_REPOSITORY => {
                Some("repository")
            }
            ServerMessage::PlayerCall(c) if c.function.0 == function::UPDATE_INFO => Some("skill"),
            ServerMessage::PlayerCall(c) if c.function.0 == SHOW_TEXT => Some("text"),
            _ => None,
        })
        .collect();
    let fx = msgs
        .iter()
        .any(|m| matches!(m, ServerMessage::DoFx(e) if e.action.fx == FX));
    (told, fx)
}

fn held_skill(h: &Harness, me: u64) -> Option<(u32, u32, u32)> {
    h.server.players[&world_of(h, me)]
        .play
        .skills
        .iter()
        .find(|k| k.skill == SKILL)
        .map(|k| (k.granted, k.mask, k.cost))
}

fn quest_done(h: &Harness, me: u64) -> bool {
    yalain_world::quests::done(&h.server.players[&world_of(h, me)].play.quests, QUEST)
}

/// A person whose use completes a hidden quest with an effect that trains a skill, the step gated
/// by an act: with the act short of whole (six of its scenes and its hidden one) the person says its
/// line and nothing is granted; with seven of its scenes the skill is trained for good (level 1 by
/// the grant, unraisable, costing nothing), told as the repository, the skill's information, the
/// effect's line of its own table naming the effect and the person, then the effect's appearance,
/// the hidden quest done and never told; used again, the person says its line, the quest being
/// done once.
/// Behaviour: server.quests.a-step-waits-for-its-act-whole
/// Behaviour: server.skills.an-effect-trains-a-skill-for-good
#[test]
fn a_step_gated_by_an_act_runs_once_the_act_is_whole_and_its_effect_trains_the_skill() {
    let mut h = world(true);
    let (b, me) = h.player_in_world("one", "Elara");
    h.run(10);
    let mut short = SCENES[..6].to_vec();
    short.push(SCENES[7]);
    holding(&mut h, me, &short);
    let from = use_it(&mut h, b, me);
    let said: Vec<u32> = texts(&h, b, from).iter().map(|t| t.string_id).collect();
    assert_eq!(said, [NO_NOTE], "its line alone");
    assert_eq!(held_skill(&h, me), None, "nothing granted");
    assert!(!quest_done(&h, me));

    holding(&mut h, me, &SCENES[..7]);
    let from = use_it(&mut h, b, me);
    assert_eq!(
        held_skill(&h, me),
        Some((1, mask::TRAINED | mask::CANNOT_RAISE, 0))
    );
    assert!(quest_done(&h, me), "the hidden quest done");
    assert_eq!(
        grant_order(&h, b, from),
        (vec!["repository", "skill", "text"], true)
    );
    let granted = Held {
        skill: SKILL,
        granted: 1,
        xp: 0,
        mask: mask::TRAINED | mask::CANNOT_RAISE,
        cost: 0,
        children: 0,
        dependents: 0,
    };
    assert!(
        h.world_messages(b, from)
            .contains(&info_update(&granted, None)),
        "the skill's information"
    );
    let said = texts(&h, b, from);
    assert_eq!(said.len(), 1, "the effect's line alone");
    assert_eq!(
        (said[0].string_id, said[0].table),
        (learned::LINE, EFFECT_TABLE)
    );
    assert_eq!(
        var(&said[0], learned::EFFECT_VAR),
        Some((0xA860_7CEC, EFFECT_TABLE))
    );
    assert_eq!(var(&said[0], learned::GIVER_VAR), Some(PERSON_NAME));
    assert!(
        !h.calls(b, from)
            .iter()
            .any(|(f, _)| *f == yalain_world::quests::UPDATE_QUEST),
        "the hidden quest never told"
    );

    let from = use_it(&mut h, b, me);
    let said: Vec<u32> = texts(&h, b, from).iter().map(|t| t.string_id).collect();
    assert_eq!(said, [NO_NOTE], "done once: its line again");
    assert_eq!(grant_order(&h, b, from), (vec!["text"], false));
}

/// A world without the step: the person says its line whatever the player holds.
/// Behaviour: server.quests.a-step-waits-for-its-act-whole
#[test]
fn a_person_with_no_step_says_its_line_with_the_act_whole() {
    let mut h = world(false);
    let (b, me) = h.player_in_world("one", "Elara");
    h.run(10);
    holding(&mut h, me, &SCENES);
    let from = use_it(&mut h, b, me);
    let said: Vec<u32> = texts(&h, b, from).iter().map(|t| t.string_id).collect();
    assert_eq!(said, [NO_NOTE]);
    assert_eq!(held_skill(&h, me), None);
}

/// An effect that trains a skill gives nothing to a player who holds the skill already, and tells
/// nothing; the skill a grant trained is kept with the character.
/// Behaviour: server.skills.an-effect-trains-a-skill-for-good
#[test]
fn an_effect_that_trains_a_skill_gives_nothing_to_a_player_holding_it() {
    let mut h = world(false);
    let (b, me) = h.player_in_world("one", "Elara");
    h.run(10);
    let w = world_of(&h, me);
    let clock = h.clock();
    let on = |h: &mut Harness| {
        effects::apply_on(
            &mut h.server,
            &mut h.net,
            On::Player(w),
            EFFECT,
            Cast {
                caster: me,
                spellcraft: 1.0,
                ..Cast::default()
            },
            clock,
        )
    };
    assert!(on(&mut h), "trained");
    h.run(5);
    assert_eq!(
        held_skill(&h, me),
        Some((1, mask::TRAINED | mask::CANNOT_RAISE, 0))
    );
    let from = h.bots[b].inbox.len();
    assert!(!on(&mut h), "held already");
    h.run(5);
    assert_eq!(
        grant_order(&h, b, from),
        (Vec::new(), false),
        "nothing told"
    );
    // A skill the install doesn't have trains nothing.
    h.server
        .effect_table
        .effects
        .insert(EFFECT, granting(SKILL + 1));
    assert!(!on(&mut h));
}

/// An effect that trains a skill whose own table holds no line trains it and tells no line, the
/// skill and its appearance told as ever.
/// Behaviour: server.skills.an-effect-trains-a-skill-for-good
#[test]
fn an_effect_that_trains_a_skill_with_no_line_of_its_table_tells_none() {
    let mut h = world(false);
    h.server.effect_table.lines.clear();
    let (b, me) = h.player_in_world("one", "Elara");
    h.run(10);
    let w = world_of(&h, me);
    let from = h.bots[b].inbox.len();
    let clock = h.clock();
    assert!(effects::apply_on(
        &mut h.server,
        &mut h.net,
        On::Player(w),
        EFFECT,
        Cast {
            caster: me,
            spellcraft: 1.0,
            giver: Some((PERSON, Some(PERSON_NAME))),
            ..Cast::default()
        },
        clock,
    ));
    h.run(5);
    assert_eq!(
        held_skill(&h, me),
        Some((1, mask::TRAINED | mask::CANNOT_RAISE, 0))
    );
    assert_eq!(
        grant_order(&h, b, from),
        (vec!["repository", "skill"], true),
        "no line"
    );
}

/// A step whose effects are kept quiet trains the skill and tells no line, though the effect's
/// table holds one; the skill and its appearance are told as ever.
/// Behaviour: server.effects.a-quiet-rows-effects-tell-no-line
#[test]
fn a_quiet_steps_skill_is_trained_without_its_effects_line() {
    let mut h = world(true);
    h.server.quests.steps[0].effects_quiet = true;
    for (step, _) in &mut h.server.story.step_acts {
        step.effects_quiet = true;
    }
    let (b, me) = h.player_in_world("one", "Elara");
    h.run(10);
    holding(&mut h, me, &SCENES[..7]);
    let from = use_it(&mut h, b, me);
    assert_eq!(
        held_skill(&h, me),
        Some((1, mask::TRAINED | mask::CANNOT_RAISE, 0))
    );
    assert_eq!(
        grant_order(&h, b, from),
        (vec!["repository", "skill"], true),
        "no line"
    );
}
