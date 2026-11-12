//! What a recipe taught by an effect tells: the line of the effect's own table, naming the effect
//! and who gave it, or nothing where that table holds no line; a step's or a give's text that is
//! the same line is told once; none of the effect's own lines where the row keeps its effects
//! quiet. Fixture: a geyser that teaches a recipe at a give, as the Ulkas geysers did, and quest
//! steps run by a person, with effects modeled on the install's (one whose table holds its line,
//! one whose table holds none).

use auberean_protocol::queue;
use auberean_script::native_object::{TextInfo, VariableValue};
use yalain_content::quests::{Give, QuestDef, QuestTable, Step, Trigger, Update};
use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind, Props};
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::Node;
use yalain_world::craft::function::ADD_RECIPE;
use yalain_world::effect_defs::{EffectDef, Kind as EffectKind, Scale};
use yalain_world::effects::learned;
use yalain_world::gives::{self, GIVE, HANDED_AFTER};
use yalain_world::inventory::{move_graph, Move};
use yalain_world::session::function::SHOW_TEXT;

/// The geyser, its template and its name.
const GEYSER: u64 = 0x8130_0000_0000_0731;
const GEYSER_T: u32 = 0x4700_31AE;
const GEYSER_NAME: (u32, u32) = (0xA860_7CEC, 0x2500_2A1B);
/// The alembic dipped in it.
const ALEMBIC: u32 = 0x4700_0B38;

/// An effect teaching a recipe whose table holds its line (the Infusion of Aconite's), and its
/// recipe.
const TAUGHT: u32 = 0x6F00_1DA7;
const TAUGHT_TABLE: u32 = 0x2500_10DF;
const TAUGHT_RECIPE: u32 = 0x6F00_02D1;
/// One whose table holds none (a Drudge crafter's), and its recipe.
const UNTOLD: u32 = 0x6F00_0E9E;
const UNTOLD_TABLE: u32 = 0x2500_1756;
const UNTOLD_RECIPE: u32 = 0x6F00_0E9F;

/// The person whose steps teach, and its name.
const PERSON: u64 = 0x8130_0000_0000_0732;
const PERSON_NAME: (u32, u32) = (0xA860_7CEC, 0x2500_2A1E);
const QUEST: u32 = 0x4000_018F;

const MOVE_DONE_STEPS: f64 = HANDED_AFTER + gives::HANDED_BACK_AFTER;

fn teaching(state: u32, recipe: u32, table: u32) -> EffectDef {
    EffectDef {
        state,
        kind: EffectKind::GrantRecipe(recipe),
        duration: Scale(Vec::new()),
        priority: Scale(Vec::new()),
        fx: 0,
        external_flags: 0,
        internal_flags: 0x4140_0012,
        ends_at_logout: false,
        ends_at_teleport: false,
        enum_value: 0,
        eq_class: 0,
        name: Some((0xA860_7CEC, table)),
    }
}

fn geyser() -> ContentObject {
    ContentObject {
        object: Some(GEYSER),
        template: GEYSER_T,
        description: Description {
            name_text: None,
            package: 0x13B1,
            look: 0x1F00_0ED5,
            enter_fx: 0,
            scale: None,
            name: Some(GEYSER_NAME),
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell: 0x0299_0109,
        origin: [58.0, 241.0, -44.6],
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

fn quest() -> QuestDef {
    QuestDef {
        id: QUEST,
        name: yalain_content::Words::Ref(0, 0),
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
        hidden: false,
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
        cancel_wait: None,
    }
}

/// The geyser's give: the alembic kept, the recipe taught, and `text` after.
fn dip(text: Option<(u32, u32)>) -> Give {
    Give {
        to: GEYSER_T,
        item: Some(ALEMBIC),
        wants: None,
        gives: Vec::new(),
        counts: Vec::new(),
        effects: vec![TAUGHT],
        text,
        effects_quiet: false,
        fx: None,
        requires: Vec::new(),
        placed: "authored for a test".into(),
    }
}

/// A step of the quest that puts this effect on, with this text after it.
fn step(effect: u32, text: Option<(u32, u32)>) -> Step {
    Step {
        recipes: Vec::new(),
        quest: QUEST,
        needs: 0,
        trigger: Trigger::Use(0x4700_31B4),
        tell: None,
        update: Update::Bestow,
        xp: 0,
        gives: Vec::new(),
        counts: Vec::new(),
        effects: vec![effect],
        text,
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
    }
}

/// A world of the geyser and its give, the quest and the two effects, the first's table holding
/// its line as the install's does; a player in it, its client and its world connection.
fn world(text: Option<(u32, u32)>) -> (Harness, usize, u64, yalain_net::ConnId) {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    h.server.catalog.insert(yalain_testkit::test_template(
        ALEMBIC,
        0x1067,
        Props::default(),
    ));
    h.server.content = ContentPack {
        objects: vec![geyser()],
        ..ContentPack::default()
    };
    h.server.quests = QuestTable {
        quests: vec![quest()],
        steps: Vec::new(),
        gives: vec![dip(text)],
        rooms: Vec::new(),
    };
    for d in [
        teaching(TAUGHT, TAUGHT_RECIPE, TAUGHT_TABLE),
        teaching(UNTOLD, UNTOLD_RECIPE, UNTOLD_TABLE),
    ] {
        h.server.effect_table.effects.insert(d.state, d);
    }
    h.server
        .effect_table
        .lines
        .insert(TAUGHT, (learned::LINE, TAUGHT_TABLE));
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(10);
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0;
    (h, b, me, world)
}

/// Give the geyser an alembic from the pack and wait until it is through. Returns where the
/// client's messages stood before.
fn dip_alembic(h: &mut Harness, b: usize, me: u64, world: yalain_net::ConnId) -> usize {
    let item =
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, ALEMBIC, 1).expect("made");
    h.run(3);
    let slot = h.server.players[&world]
        .play
        .items
        .iter()
        .find(|x| x.id == item)
        .map_or(0, |x| x.slot);
    let slot = i32::try_from(slot).unwrap_or(0);
    let from = h.bots[b].inbox.len();
    let m = Move {
        move_type: GIVE,
        merge: true,
        unlock: true,
        check_take: true,
        all_taken: true,
        grab: true,
        contain: true,
        auto_merge: true,
        target: GEYSER,
        actual_target: GEYSER,
        merge_container: GEYSER,
        from: me,
        actual_from: me,
        item,
        split_item: item,
        quantity: 1,
        quantity_left: 1,
        from_slot: slot,
        actual_from_slot: slot,
        ..Move::default()
    };
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(yalain_world::drops::function::MOVE_ITEM, &[move_graph(&m)]),
    );
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = (MOVE_DONE_STEPS / yalain_testkit::STEP).ceil() as usize;
    h.run(n + 8);
    from
}

/// Run a step for the player, as the person's.
fn run_step(h: &mut Harness, b: usize, world: yalain_net::ConnId, s: &Step) -> usize {
    let from = h.bots[b].inbox.len();
    let clock = h.clock();
    yalain_world::quests::run(
        &mut h.server,
        &mut h.net,
        world,
        s,
        Some((PERSON, Some(PERSON_NAME))),
        clock,
    );
    h.run(2);
    from
}

/// The texts a client was shown since `from`.
fn shown(h: &Harness, b: usize, from: usize) -> Vec<TextInfo> {
    h.calls(b, from)
        .into_iter()
        .filter(|(f, _)| *f == SHOW_TEXT)
        .filter_map(|(_, v)| {
            v.iter().find_map(|x| match x.root() {
                Some(Node::TextInfo(t)) => Some(t.clone()),
                _ => None,
            })
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

fn taught(h: &Harness, b: usize, from: usize, recipe: u32) -> bool {
    h.calls(b, from).iter().any(|(f, v)| {
        *f == ADD_RECIPE
            && v.iter()
                .any(|x| matches!(x, yalain_world::calls::Value::Int(r) if *r == recipe))
    })
}

/// A geyser's give that teaches a recipe tells the line of the effect's own table, naming the
/// effect and the geyser (2017: 26 of 26 at the one geyser, each in the effect's table), never
/// another effect's.
/// Behaviour: server.effects.a-recipe-taught-tells-its-own-tables-line
#[test]
fn a_recipe_taught_at_a_give_tells_the_line_of_its_effects_own_table() {
    let (mut h, b, me, world) = world(None);
    let from = dip_alembic(&mut h, b, me, world);
    assert!(taught(&h, b, from, TAUGHT_RECIPE), "the recipe taught");
    let said = shown(&h, b, from);
    assert_eq!(said.len(), 1, "one line: {said:?}");
    assert_eq!(
        (said[0].string_id, said[0].table),
        (learned::LINE, TAUGHT_TABLE)
    );
    assert_eq!(
        var(&said[0], learned::EFFECT_VAR),
        Some((0xA860_7CEC, TAUGHT_TABLE)),
        "the effect's name"
    );
    assert_eq!(var(&said[0], learned::GIVER_VAR), Some(GEYSER_NAME));
}

/// A give's text that is the line its effect told is told once (2017: one line at each such give).
/// Behaviour: server.effects.a-recipe-taught-tells-its-own-tables-line
#[test]
fn a_gives_text_that_is_its_effects_line_is_told_once() {
    let (mut h, b, me, world) = world(Some((learned::LINE, TAUGHT_TABLE)));
    let from = dip_alembic(&mut h, b, me, world);
    let said: Vec<(u32, u32)> = shown(&h, b, from)
        .iter()
        .map(|t| (t.string_id, t.table))
        .collect();
    assert_eq!(said, [(learned::LINE, TAUGHT_TABLE)]);
}

/// A step's effect whose table holds no line teaches its recipe and tells nothing (2017: 15 of 15
/// such grants); one whose table holds it tells it, and a step's text that is that line is told
/// once, another text after it.
/// Behaviour: server.effects.a-recipe-taught-tells-its-own-tables-line
#[test]
fn a_steps_recipe_tells_its_own_tables_line_or_nothing_and_a_text_that_is_it_once() {
    let (mut h, b, _, world) = world(None);
    let from = run_step(&mut h, b, world, &step(UNTOLD, None));
    assert!(taught(&h, b, from, UNTOLD_RECIPE), "the recipe taught");
    assert!(shown(&h, b, from).is_empty(), "nothing told");

    let line = (learned::LINE, TAUGHT_TABLE);
    let from = run_step(&mut h, b, world, &step(TAUGHT, Some(line)));
    let said = shown(&h, b, from);
    assert_eq!(
        said.iter()
            .map(|t| (t.string_id, t.table))
            .collect::<Vec<_>>(),
        [line],
        "told once"
    );
    assert_eq!(var(&said[0], learned::GIVER_VAR), Some(PERSON_NAME));

    let other = (0x912E_FF1D, TAUGHT_TABLE);
    let from = run_step(&mut h, b, world, &step(TAUGHT, Some(other)));
    let said: Vec<(u32, u32)> = shown(&h, b, from)
        .iter()
        .map(|t| (t.string_id, t.table))
        .collect();
    assert_eq!(said, [line, other], "the line, then the step's own text");
}

/// The step, its effects kept quiet.
fn quiet(mut s: Step) -> Step {
    s.effects_quiet = true;
    s
}

/// A step whose effects are kept quiet teaches its recipe and tells none of their lines, though its
/// effect's table holds one (2017: the Hero Shrine's three recipes, 15 grants at 5 uses, none told
/// in the effect's table); its own text is still told, even where it is that line, and the same
/// step not kept quiet tells the line.
/// Behaviour: server.effects.a-quiet-rows-effects-tell-no-line
#[test]
fn a_quiet_steps_recipe_is_taught_in_silence_and_its_text_still_told() {
    let (mut h, b, _, world) = world(None);
    let from = run_step(&mut h, b, world, &quiet(step(TAUGHT, None)));
    assert!(taught(&h, b, from, TAUGHT_RECIPE), "the recipe taught");
    assert!(shown(&h, b, from).is_empty(), "nothing told");

    let line = (learned::LINE, TAUGHT_TABLE);
    let other = (0x912E_FF1D, TAUGHT_TABLE);
    for text in [other, line] {
        let from = run_step(&mut h, b, world, &quiet(step(TAUGHT, Some(text))));
        let said: Vec<(u32, u32)> = shown(&h, b, from)
            .iter()
            .map(|t| (t.string_id, t.table))
            .collect();
        assert_eq!(said, [text], "the step's own text alone");
    }

    let from = run_step(&mut h, b, world, &step(TAUGHT, None));
    let said: Vec<(u32, u32)> = shown(&h, b, from)
        .iter()
        .map(|t| (t.string_id, t.table))
        .collect();
    assert_eq!(said, [line], "not kept quiet: the effect's line");
}

/// A give whose effects are kept quiet teaches its recipe and tells nothing; its text is still
/// told (2017: Elysa's Initiation recipe at its give, 1 of 1 told nothing in the effect's table).
/// Behaviour: server.effects.a-quiet-rows-effects-tell-no-line
#[test]
fn a_quiet_gives_recipe_is_taught_in_silence_and_its_text_still_told() {
    let (mut h, b, me, w) = world(None);
    h.server.quests.gives[0].effects_quiet = true;
    let from = dip_alembic(&mut h, b, me, w);
    assert!(taught(&h, b, from, TAUGHT_RECIPE), "the recipe taught");
    assert!(shown(&h, b, from).is_empty(), "nothing told");

    let other = (0x912E_FF1D, TAUGHT_TABLE);
    let (mut h, b, me, w) = world(Some(other));
    h.server.quests.gives[0].effects_quiet = true;
    let from = dip_alembic(&mut h, b, me, w);
    let said: Vec<(u32, u32)> = shown(&h, b, from)
        .iter()
        .map(|t| (t.string_id, t.table))
        .collect();
    assert_eq!(said, [other], "the give's own text alone");
}
