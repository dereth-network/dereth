//! Divergence: V566
//! What the character's items make used on one another, asked by a Dereth client
//! (`0xF7F0`) and answered (`0xF7F1`): every ordered pair with a recipe among the character's own
//! items, with its product, the character's chance and what a failure destroys; nothing for an
//! item it does not hold, and nothing looked up for a query sooner than a second after the last.
//! Fixture: the crafting fixture's server, with its flour-on-water recipe.

use dereth_primitives::NetQueue;
use dereth_protocol::admin::{RecipeAnswer, RecipeAnswerStatus, RecipeQuery};

use super::recipes_tinkering_salvage::{give, join, server, ALPHA, DOUGH, FLOUR, SWORD, WATER};

fn answer(
    ts: &empyrean_testkit::TestServer,
    id: empyrean_testkit::ClientId,
    request: u32,
) -> RecipeAnswer {
    ts.received::<RecipeAnswer>(id)
        .into_iter()
        .find(|a| a.request == request)
        .unwrap_or_else(|| panic!("an answer to query {request}"))
}

#[test]
fn a_query_names_each_recipe_among_the_characters_own_items_once_a_second() {
    let mut ts = server();
    let (id, _) = join(&mut ts, "alpha", ALPHA, 100);
    let flour = give(&mut ts, FLOUR).full();
    let water = give(&mut ts, WATER).full();
    let sword = give(&mut ts, SWORD).full();
    ts.send_message(
        id,
        NetQueue::ClientCache,
        &RecipeQuery {
            request: 1,
            items: vec![flour, water, sword],
        },
    );
    ts.advance(0.1);
    let a = answer(&ts, id, 1);
    assert_eq!(a.status, RecipeAnswerStatus::Answered);
    // Flour on water makes dough; water on flour, or either with the sword, makes nothing.
    assert_eq!(a.matches.len(), 1, "{a:?}");
    let m = &a.matches[0];
    assert_eq!((m.source, m.target), (flour, water));
    assert_eq!(
        (m.makes_wcid, m.makes_name.as_str(), m.makes_count),
        (DOUGH, "Dough", 1)
    );
    // No difficulty: it cannot fail, and a failure destroys nothing.
    assert!((m.chance - 1.0).abs() < f32::EPSILON, "{m:?}");
    assert_eq!(m.fails, 0);
    // Asked again at once: too soon, nothing looked up.
    ts.send_message(
        id,
        NetQueue::ClientCache,
        &RecipeQuery {
            request: 2,
            items: vec![flour, water],
        },
    );
    ts.advance(0.1);
    let a = answer(&ts, id, 2);
    assert_eq!(a.status, RecipeAnswerStatus::TooSoon);
    assert!(a.matches.is_empty());
    // A second on, an item the character does not hold: nothing is looked up.
    ts.advance(1.0);
    ts.send_message(
        id,
        NetQueue::ClientCache,
        &RecipeQuery {
            request: 3,
            items: vec![flour, 0x8000_7777],
        },
    );
    ts.advance(0.1);
    let a = answer(&ts, id, 3);
    assert_eq!(a.status, RecipeAnswerStatus::NotHeld);
    assert!(a.matches.is_empty());
    // And the character's own again: answered.
    ts.advance(1.1);
    ts.send_message(
        id,
        NetQueue::ClientCache,
        &RecipeQuery {
            request: 4,
            items: vec![water, flour],
        },
    );
    ts.advance(0.1);
    let a = answer(&ts, id, 4);
    assert_eq!(a.status, RecipeAnswerStatus::Answered);
    assert_eq!(a.matches.len(), 1);
}
