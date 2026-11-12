//! A chain of other effects put on a player puts each on at once, in its order, as each does itself:
//! two recipes taught in the chain's order, a chain held in another put on in its place, its own
//! appearance played; a chain that holds itself ends. Fixture: a player alone in the test world and
//! effects modeled on the install's chain of recipe grants.

use auberean_protocol::ServerMessage;
use yalain_net::ConnId;
use yalain_testkit::Harness;
use yalain_world::calls::Value;
use yalain_world::craft::function::ADD_RECIPE;
use yalain_world::effect_defs::{EffectDef, Kind, Scale};
use yalain_world::effects;

/// Two effects that each teach a recipe, and their recipes.
const FIRST: u32 = 0x6F00_1CAD;
const FIRST_RECIPE: u32 = 0x6F00_1CAE;
const SECOND: u32 = 0x6F00_1CAF;
const SECOND_RECIPE: u32 = 0x6F00_1CB0;
/// A chain of the first and a second chain; the second chain holds the second effect and the
/// first chain again.
const CHAIN: u32 = 0x6F00_1CAC;
const INNER: u32 = 0x6F00_1CDF;
const CHAIN_FX: u32 = 0x4100_0499;

fn effect(state: u32, kind: Kind, fx: u32) -> EffectDef {
    EffectDef {
        state,
        kind,
        duration: Scale(Vec::new()),
        priority: Scale(Vec::new()),
        fx,
        external_flags: 0,
        internal_flags: 0x4040_0012,
        ends_at_logout: false,
        ends_at_teleport: false,
        enum_value: 0,
        eq_class: 0,
        name: None,
    }
}

/// A player in the world, with the effects in its server's table.
fn player_with(chains: &[(u32, Vec<u32>, u32)]) -> (Harness, usize, ConnId) {
    let mut h = Harness::new();
    let (b, _) = h.player_in_world("one", "Elara");
    h.run(10);
    let world = *h.server.players.keys().next().expect("in the world");
    let t = &mut h.server.effect_table.effects;
    t.insert(FIRST, effect(FIRST, Kind::GrantRecipe(FIRST_RECIPE), 0));
    t.insert(SECOND, effect(SECOND, Kind::GrantRecipe(SECOND_RECIPE), 0));
    for (state, list, fx) in chains {
        t.insert(*state, effect(*state, Kind::Instants(list.clone()), *fx));
    }
    (h, b, world)
}

fn put_on(h: &mut Harness, world: ConnId, state: u32) -> bool {
    let caster = h.server.players[&world].character;
    let cast = effects::Cast {
        caster,
        spellcraft: 1.0,
        ..effects::Cast::default()
    };
    let clock = h.clock();
    effects::apply(&mut h.server, &mut h.net, world, state, cast, clock)
}

/// The recipes taught since `from`, in the order they were told.
fn taught(h: &Harness, b: usize, from: usize) -> Vec<u32> {
    h.calls(b, from)
        .iter()
        .filter(|(f, _)| *f == ADD_RECIPE)
        .flat_map(|(_, v)| v.iter())
        .filter_map(|x| match x {
            Value::Int(r) => Some(*r),
            _ => None,
        })
        .collect()
}

/// A chain teaches its two recipes in its own order, each once, and plays its own appearance; put
/// in the other order, it teaches them in that order.
/// Behaviour: server.effects.a-chain-puts-on-each-of-its-effects
#[test]
fn a_chain_of_effects_puts_each_on_in_its_order_and_plays_its_appearance() {
    let (mut h, b, world) = player_with(&[(CHAIN, vec![FIRST, SECOND], CHAIN_FX)]);
    let from = h.bots[b].inbox.len();
    assert!(put_on(&mut h, world, CHAIN), "the chain did something");
    h.run(2);
    assert_eq!(taught(&h, b, from), [FIRST_RECIPE, SECOND_RECIPE]);
    let fx = h
        .world_messages(b, from)
        .into_iter()
        .filter(|m| matches!(m, ServerMessage::DoFx(e) if e.action.fx == CHAIN_FX))
        .count();
    assert_eq!(fx, 1, "its own appearance, once");
    let (mut h, b, world) = player_with(&[(CHAIN, vec![SECOND, FIRST], 0)]);
    let from = h.bots[b].inbox.len();
    assert!(put_on(&mut h, world, CHAIN));
    h.run(2);
    assert_eq!(taught(&h, b, from), [SECOND_RECIPE, FIRST_RECIPE]);
}

/// A chain held in a chain puts its own effects on in its place; one that comes back to the first
/// chain ends there, each recipe taught once.
/// Behaviour: server.effects.a-chain-puts-on-each-of-its-effects
#[test]
fn a_chain_held_in_a_chain_puts_its_effects_on_in_its_place_and_a_loop_ends() {
    let (mut h, b, world) = player_with(&[
        (CHAIN, vec![INNER, FIRST], 0),
        (INNER, vec![SECOND, CHAIN], 0),
    ]);
    let from = h.bots[b].inbox.len();
    assert!(put_on(&mut h, world, CHAIN));
    h.run(2);
    assert_eq!(taught(&h, b, from), [SECOND_RECIPE, FIRST_RECIPE]);
}

/// A chain of effects the server doesn't know does nothing.
/// Behaviour: server.effects.a-chain-puts-on-each-of-its-effects
#[test]
fn a_chain_of_unknown_effects_does_nothing() {
    let (mut h, b, world) = player_with(&[(CHAIN, vec![0x6F00_9999], 0)]);
    let from = h.bots[b].inbox.len();
    assert!(!put_on(&mut h, world, CHAIN));
    h.run(2);
    assert!(taught(&h, b, from).is_empty());
}

/// Only the chains a start keeps are chains: one an item's use names and a chain it holds, one a
/// quest's update names, and one a recipe's outcome puts on each put their effects on; a chain only
/// a skill's combination adds (a finisher) reads as nothing and teaches nothing.
/// Behaviour: server.effects.a-chain-puts-on-each-of-its-effects
#[test]
fn only_the_chains_items_quests_and_recipes_lead_to_put_anything_on() {
    use yalain_content::quests::{QuestDef, QuestTable};
    use yalain_world::effect_defs::chain_roots;
    use yalain_world::item_use::UsageEffect;
    const QUEST_CHAIN: u32 = 0x6F00_0DEE;
    const CRAFT_CHAIN: u32 = 0x6F00_1CCE;
    const FINISHER: u32 = 0x6F00_10D8;
    const THIRD: u32 = 0x6F00_1CB1;
    const THIRD_RECIPE: u32 = 0x6F00_1CB2;
    let (mut h, b, world) = player_with(&[
        (CHAIN, vec![INNER], 0),
        (INNER, vec![FIRST], 0),
        (QUEST_CHAIN, vec![SECOND], 0),
        (CRAFT_CHAIN, vec![THIRD], 0),
        (FINISHER, vec![FIRST, SECOND, THIRD], 0),
    ]);
    h.server
        .effect_table
        .effects
        .insert(THIRD, effect(THIRD, Kind::GrantRecipe(THIRD_RECIPE), 0));
    let uses = [(
        0x4700_15AE,
        vec![UsageEffect {
            state: CHAIN,
            spellcraft: 1.0,
        }],
    )]
    .into_iter()
    .collect();
    let quests = QuestTable {
        quests: vec![QuestDef {
            id: 0x4000_0123,
            name: yalain_content::Words::Ref(0, 0),
            description: (0, 0),
            icon: 0,
            play_fx: false,
            journals: Vec::new(),
            challenge: 0,
            duration: 0.0,
            timed_phase: 0,
            retry: 0.0,
            xp: 0,
            cancellable: false,
            placed: "authored for a test".into(),
            hidden: false,
            effects: vec![(0x1000_0000, QUEST_CHAIN, 1.0)],
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
        }],
        ..QuestTable::default()
    };
    // A recipe whose outcome puts the crafted chain on its maker.
    use yalain_world::craft::{Action, Check, Crafting, Outcome, RecipeDef};
    let mut craft = Crafting::default();
    craft.recipes.insert(
        0x6F00_1CAE,
        RecipeDef {
            checks: vec![Check {
                upto: 1.0,
                outcomes: vec![Outcome {
                    upto: 1.0,
                    xp_mod: 1.0,
                    actions: vec![Action::Effect {
                        state: CRAFT_CHAIN,
                        spellcraft: 1.0,
                    }],
                }],
            }],
            ..RecipeDef::default()
        },
    );
    let roots = chain_roots(&uses, &h.server.item_rows, &quests, &craft);
    h.server.effect_table.keep_chains_of(roots);
    for (chain, kept) in [
        (CHAIN, true),
        (INNER, true),
        (QUEST_CHAIN, true),
        (CRAFT_CHAIN, true),
        (FINISHER, false),
    ] {
        assert_eq!(
            matches!(
                h.server.effect_table.get(chain).map(|d| &d.kind),
                Some(Kind::Instants(_))
            ),
            kept,
            "{chain:08X}"
        );
    }
    let from = h.bots[b].inbox.len();
    assert!(
        !put_on(&mut h, world, FINISHER),
        "the finisher does nothing"
    );
    h.run(2);
    assert!(taught(&h, b, from).is_empty());
    for chain in [CHAIN, QUEST_CHAIN, CRAFT_CHAIN] {
        assert!(put_on(&mut h, world, chain), "{chain:08X}");
    }
    h.run(2);
    assert_eq!(
        taught(&h, b, from),
        [FIRST_RECIPE, SECOND_RECIPE, THIRD_RECIPE]
    );
}
