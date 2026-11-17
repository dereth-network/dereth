//! A bought untraining: a person who, for a price, finishes the untraining a player has under way.
//! The world's step (a give of the price that waits on an untraining under way, its effect one that
//! finishes it) and give (a token, likewise) run only for a player untraining a skill; for any other
//! the price is handed back with the person's line for it. Fixture: a person beside the start, a
//! made-up skill with experience in it trained by a new player, coins and a token in its pack.

use std::collections::HashMap;

use auberean_protocol::{queue, ServerMessage};
use yalain_content::quests::{Give, QuestDef, QuestTable, Step, Trigger, Update};
use yalain_content::talk::{How, Line, Role, TalkTable, When, Words};
use yalain_content::untraining::UntrainingGates;
use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind, Prop, Props};
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::{Node, Value};
use yalain_world::effect_defs::{EffectDef, Kind as EffectKind, Scale};
use yalain_world::gives::{GIVE, HANDED_AFTER, HANDED_BACK_AFTER};
use yalain_world::inventory::{move_graph, Move};
use yalain_world::progress::q::AVAILABLE_XP;
use yalain_world::skills::{untraining, SkillDef, SkillTable, Untraining};
use yalain_world::state::SkillState;
use yalain_world::{quals, quests};

const HEAR_TELL: u32 = 0x8205_0017;

/// The person, its quest counting the untrainings it has sold a player, and what it takes.
const PERSON: u64 = 0x8130_0000_0000_0711;
const PERSON_T: u32 = 0x4700_338D;
const QUEST: u32 = 0x4000_02A4;
const COINS: u32 = 0x4700_0538;
const PRICE: u32 = 15_000;
const TOKEN: u32 = 0x4700_1B42;
/// The effect that finishes an untraining under way.
const FINISHES: u32 = 0x6F00_248D;
/// Its lines: the price refused, the token refused.
const NOT_UNTRAINING: u32 = 0x61B4_57E8;
const TOKEN_REFUSED: u32 = 0xFD56_B34A;

/// A made-up skill trained for two credits, with experience in it.
const SKILL: u32 = 15;
const COST: u32 = 2;
const XP: u64 = 40;
const AVAILABLE: u64 = 301;

fn place(id: u64, template: u32) -> ContentObject {
    ContentObject {
        object: Some(id),
        template,
        description: Description {
            name_text: None,
            package: 0x13A4,
            look: 0x1F00_0100,
            enter_fx: 0,
            scale: None,
            name: Some((0xA860_7CEC, 0x2500_2AD7)),
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell: 0x0299_0109,
        origin: [60.0, 241.0, -44.6],
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

fn quest() -> QuestDef {
    QuestDef {
        duration_by_calendar: false,
        id: QUEST,
        name: Words::Ref(0xA860_7CEC, 0x2500_17F7),
        description: (0x6C45_9364, 0x2500_17F7),
        icon: 0x4100_034B,
        play_fx: true,
        journals: (0..4).map(|i| (0x2E5C_1E52 + i, 0x2500_17F7)).collect(),
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

/// The price given: the quest given, the coins kept, the untraining finished.
fn sold() -> Step {
    Step {
        out_of_turn: false,
        skills: Vec::new(),
        key: "sold".into(),
        quest: QUEST,
        needs: 0,
        trigger: Trigger::Give(COINS, PERSON_T),
        tell: None,
        update: Update::Bestow,
        xp: 0,
        gives: Vec::new(),
        counts: Vec::new(),
        effects: vec![FINISHES],
        text: None,
        recipes: Vec::new(),
        placed: "authored for a test".into(),
        requires: Vec::new(),
        takes: vec![COINS],
        races: Vec::new(),
        effects_last: false,
        effects_quiet: false,
        wants: Some(PRICE),
        fellows: false,
        levels: None,
        xp_share: 0.0,
        to: None,
        transforms: None,
        chance: None,
        to_fellows: false,
    }
}

/// The token given: kept, the untraining finished, no quest moved.
fn token() -> Give {
    Give {
        to: PERSON_T,
        item: Some(TOKEN),
        wants: Some(1),
        gives: Vec::new(),
        counts: Vec::new(),
        effects: vec![FINISHES],
        text: None,
        effects_quiet: false,
        fx: None,
        requires: Vec::new(),
        placed: "authored for a test".into(),
    }
}

fn refusal(item: u32, string: u32) -> Line {
    Line {
        who: PERSON_T,
        object: None,
        words: Words::Ref(string, 0x2500_2AD7),
        role: Some(Role::RefuseGive),
        how: How::Tell,
        when: When::Give,
        quest: None,
        update: None,
        phase: None,
        item: Some(item),
        order: 0,
        measured: true,
        placed: "authored for a test".into(),
    }
}

/// The world, and the player in it holding the skill (with experience in it), one credit, the
/// price in coins and a token: its client, character and connection. With `under_way`, it has
/// begun untraining the skill.
fn fixture(under_way: bool) -> (Harness, usize, u64, yalain_net::ConnId) {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    for (template, most) in [(COINS, 1_000_000), (TOKEN, 25)] {
        let mut props = Props::default();
        props.set("Item_MaxStackSize", Prop::Int(most));
        h.server
            .catalog
            .insert(yalain_testkit::test_template(template, 0x1005, props));
    }
    h.server.content = ContentPack {
        objects: vec![place(PERSON, PERSON_T)],
        untraining_gates: UntrainingGates {
            steps: vec![sold()],
            gives: vec![token()],
        },
        ..ContentPack::default()
    };
    h.server.quests = QuestTable {
        quests: vec![quest()],
        steps: vec![sold()],
        gives: vec![token()],
        rooms: Vec::new(),
    };
    h.server.talk = TalkTable {
        lines: vec![
            refusal(COINS, NOT_UNTRAINING),
            refusal(TOKEN, TOKEN_REFUSED),
        ],
    };
    h.server.effect_table.effects.insert(
        FINISHES,
        EffectDef {
            state: FINISHES,
            kind: EffectKind::Untrains,
            duration: Scale::default(),
            priority: Scale::default(),
            fx: 0,
            external_flags: 0,
            internal_flags: 0,
            ends_at_logout: false,
            ends_at_teleport: false,
            enum_value: 0,
            eq_class: 0,
            name: None,
        },
    );
    let mut skills = HashMap::new();
    skills.insert(
        SKILL,
        SkillDef {
            id: SKILL,
            name: (0xA860_7CEC, 0x2500_085B),
            cost: COST,
            flags: 0x1,
            ..SkillDef::default()
        },
    );
    h.server.skill_table = SkillTable {
        skills,
        tables: HashMap::new(),
        ..SkillTable::default()
    };
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    let world = *h.server.players.keys().next().expect("in");
    let p = h.server.players.get_mut(&world).expect("in");
    p.play.credits = 1;
    p.play.skills.push(SkillState {
        skill: SKILL,
        granted: 0,
        mask: 0x3,
        cost: COST,
        children: 0,
        dependents: 0,
        spent: XP,
    });
    quals::set_long(&mut p.qualities, AVAILABLE_XP, AVAILABLE);
    if under_way {
        p.untraining = Some(Untraining {
            skill: SKILL,
            xp: XP,
            total_at_start: 0,
        });
    }
    for (template, n) in [(COINS, PRICE), (TOKEN, 2)] {
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, template, n).expect("made");
    }
    h.run(3);
    (h, b, me, world)
}

/// The pack's item of a template, and its slot.
fn held(h: &Harness, world: yalain_net::ConnId, template: u32) -> Option<(u64, i32, u32)> {
    let p = &h.server.players[&world];
    let x = p.play.items.iter().find(|x| x.template == template)?;
    let n = h.server.things.get(x.id).map_or(0, |t| t.quantity);
    Some((x.id, i32::try_from(x.slot).unwrap_or(0), n))
}

/// The client gives one of its items (all `quantity` of it) to the person, and the give runs out.
fn give(h: &mut Harness, b: usize, me: u64, world: yalain_net::ConnId, template: u32) -> usize {
    let (item, slot, quantity) = held(h, world, template).expect("held");
    let quantity = if template == TOKEN { 1 } else { quantity };
    let m = Move {
        move_type: GIVE,
        merge: true,
        unlock: true,
        check_take: true,
        all_taken: true,
        grab: true,
        contain: true,
        auto_merge: true,
        target: PERSON,
        actual_target: PERSON,
        merge_container: PERSON,
        from: me,
        actual_from: me,
        item,
        split_item: item,
        quantity,
        quantity_left: quantity,
        from_slot: slot,
        actual_from_slot: slot,
        ..Move::default()
    };
    let from = h.bots[b].inbox.len();
    let call = server_call(yalain_world::drops::function::MOVE_ITEM, &[move_graph(&m)]);
    h.bots[b].send(true, queue::WEENIE, &call);
    let steps = (HANDED_AFTER + HANDED_BACK_AFTER) / yalain_testkit::STEP;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    h.run(steps.ceil() as usize + 10);
    from
}

/// The lines the person told the client since `from`.
fn told(h: &Harness, b: usize, from: usize) -> Vec<u32> {
    h.calls(b, from)
        .into_iter()
        .filter(|(f, _)| *f == HEAR_TELL)
        .filter_map(|(_, v)| {
            v.iter().find_map(|x: &Value| match x.root() {
                Some(Node::TextInfo(t)) => Some(t.string_id),
                _ => None,
            })
        })
        .collect()
}

fn untrained_told(h: &Harness, b: usize, from: usize) -> bool {
    h.texts(b, from)
        .iter()
        .any(|(_, id)| *id == untraining::UNTRAINED)
}

fn holds_skill(h: &Harness, world: yalain_net::ConnId) -> bool {
    h.server.players[&world]
        .play
        .skills
        .iter()
        .any(|s| s.skill == SKILL)
}

fn available(h: &Harness, world: yalain_net::ConnId) -> u64 {
    quals::long(&h.server.players[&world].qualities, AVAILABLE_XP).unwrap_or(0)
}

/// A player untraining a skill who gives the person its price is sold the untraining: the coins
/// kept, the quest that counts the sales given, and the untraining finished at once, as earning
/// its experience again would (the skill untrained and the player told, its credits back, its
/// experience given back to spend); a token does the same, moving no quest.
///
/// Behaviour: server.skills.a-person-finishes-an-untraining-for-a-price
#[test]
fn a_player_untraining_a_skill_who_pays_the_price_has_the_untraining_finished_at_once() {
    let (mut h, b, me, world) = fixture(true);
    let from = give(&mut h, b, me, world, COINS);
    assert!(held(&h, world, COINS).is_none(), "the coins kept");
    assert!(untrained_told(&h, b, from), "told the skill is untrained");
    assert!(!holds_skill(&h, world), "the skill gone");
    assert!(h.server.players[&world].untraining.is_none());
    assert_eq!(h.server.players[&world].play.credits, 1 + COST);
    assert_eq!(available(&h, world), AVAILABLE + XP, "its experience back");
    assert_eq!(
        quests::phase_of(&h.server.players[&world].play.quests, QUEST),
        1,
        "the quest given"
    );
    assert!(
        h.world_messages(b, from).iter().any(|m| matches!(m,
            ServerMessage::UpdateLongInt(q) if q.quality == AVAILABLE_XP
                && q.value == AVAILABLE + XP)),
        "the client told the experience it may spend"
    );

    // The token, for a second untraining.
    let (mut h, b, me, world) = fixture(true);
    let from = give(&mut h, b, me, world, TOKEN);
    assert_eq!(
        held(&h, world, TOKEN).map(|(_, _, n)| n),
        Some(1),
        "one kept"
    );
    assert!(untrained_told(&h, b, from));
    assert!(!holds_skill(&h, world));
    assert_eq!(
        quests::phase_of(&h.server.players[&world].play.quests, QUEST),
        0
    );
}

/// A player untraining nothing who gives the person its price or a token has it handed back with
/// the person's line for that item; the skill stays, and no quest is given.
///
/// Behaviour: server.skills.a-person-finishes-an-untraining-for-a-price
#[test]
fn a_player_untraining_nothing_has_the_price_and_the_token_handed_back() {
    let (mut h, b, me, world) = fixture(false);
    let from = give(&mut h, b, me, world, COINS);
    assert_eq!(told(&h, b, from), [NOT_UNTRAINING]);
    assert_eq!(
        held(&h, world, COINS).map(|(_, _, n)| n),
        Some(PRICE),
        "handed back"
    );
    let from = give(&mut h, b, me, world, TOKEN);
    assert_eq!(told(&h, b, from), [TOKEN_REFUSED]);
    assert_eq!(held(&h, world, TOKEN).map(|(_, _, n)| n), Some(2));
    assert!(holds_skill(&h, world));
    assert_eq!(available(&h, world), AVAILABLE);
    assert_eq!(
        quests::phase_of(&h.server.players[&world].play.quests, QUEST),
        0
    );
    assert!(!untrained_told(&h, b, from));
}
