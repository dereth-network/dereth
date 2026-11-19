//! The making window: opened on the game's reading of a recipe, items carried into its slots and
//! out, the spinner, Fill in, Craft and how a making ends; and how hard a recipe reads.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use std::collections::BTreeMap;

use auberean_client_sdk::contract::notice::{CraftNotice, Notice, Said};
use auberean_client_sdk::contract::view::{RecipeAt, View};
use auberean_client_sdk::contract::{Action, CraftAction};
use auberean_horizon::keys::Keyed;
use auberean_horizon::state::craft::difficulty_colour;
use auberean_horizon::state::GameState;
use auberean_horizon::ui::making::{Layout, Unlit};

use super::{follow, page, sword_at, view, SWORD};
use crate::things::{carry, item, pack};
use crate::{centre, Harness};

const INGOT: u64 = 0x7000_0000_0000_0101;
const HILT: u64 = 0x7000_0000_0000_0102;

fn said(said: Said) -> Notice {
    Notice { about: None, said }
}

fn added(slot: u32, item: u64, spinner: u32, error: u32) -> Notice {
    said(Said::Craft(CraftNotice::IngredientAdded {
        recipe: SWORD,
        slot,
        item,
        amount: spinner,
        error,
    }))
}

fn made(error: u32) -> Notice {
    said(Said::Craft(CraftNotice::Made {
        recipe: SWORD,
        error,
    }))
}

/// The game's view: the crafting page, the pack with an ingot and a hilt, and the recipe opened
/// as `opened` reads.
fn the_view(opened: Option<RecipeAt>, version: u64) -> View {
    let mut c = page();
    c.version = version;
    c.opened = opened;
    let mut v = view(c);
    v.inventory = Some(pack(
        [&[INGOT, HILT], &[], &[], &[]],
        &[(INGOT, item("Ingot")), (HILT, item("Hilt"))],
        &[],
        0,
    ));
    v
}

/// The sword opened for making: the game asks, then reads it; the harness and the state, the
/// window open.
fn opened_with(at: RecipeAt) -> (Harness, GameState) {
    let mut h = Harness::new();
    let mut s = GameState::default();
    follow(
        &mut s,
        &the_view(None, 1),
        &[said(Said::Craft(CraftNotice::Open { recipe: SWORD }))],
    );
    h.frame(&s);
    assert!(!h.ui.making.is_open(), "not until the game read it");
    follow(&mut s, &the_view(Some(at), 2), &[]);
    h.frame(&s);
    (h, s)
}

fn opened() -> (Harness, GameState) {
    opened_with(sword_at(2, &[INGOT, HILT]))
}

fn layout(h: &Harness) -> Layout {
    h.ui.making.layout(h.k()).expect("open")
}

/// Hear `notices` in a frame, then a frame with none.
fn hear(h: &mut Harness, s: &mut GameState, view: &View, notices: &[Notice]) -> Vec<Action> {
    follow(s, view, notices);
    let mut acts = h.frame(s).acts;
    follow(s, view, &[]);
    acts.extend(h.frame(s).acts);
    acts
}

#[test]
fn it_opens_once_the_game_has_read_the_recipe_at_its_last_value_with_a_slot_an_ingredient() {
    let (h, _) = opened();
    assert!(h.ui.making.is_open());
    assert_eq!(h.ui.making.recipe, SWORD);
    assert_eq!(h.ui.making.spinner, 2);
    let shown = h.ui.making.shown().expect("shown");
    assert_eq!(
        shown.ingredients,
        vec![(1, "Ingot".to_owned()), (2, "Hilt".to_owned())]
    );
    assert_eq!(shown.name, "Sword");
}

#[test]
fn an_item_carried_onto_a_slot_is_asked_to_go_in_and_holds_it_once_the_game_says_it_went() {
    let (mut h, mut s) = opened();
    h.ui.keyed(Keyed::Inventory, &s);
    h.frame(&s);
    let k = h.k();
    let inv = h.ui.goods.inventory.layout(k).expect("open");
    let l = layout(&h);
    let acts = carry(&mut h, &s, centre(inv.cell(0, k)), centre(l.slot(0, k)));
    assert!(
        acts.contains(&Action::Craft(CraftAction::AddIngredient {
            recipe: SWORD,
            ordinal: 1,
            item: INGOT,
            spinner: 2,
        })),
        "{acts:?}"
    );
    assert!(h.ui.making.waiting.contains(&1), "the slot waits");
    let v = the_view(Some(sword_at(2, &[INGOT, HILT])), 2);
    hear(&mut h, &mut s, &v, &[added(1, INGOT, 2, 0)]);
    assert_eq!(h.ui.making.slots, BTreeMap::from([(1, INGOT)]));
    assert!(h.ui.making.waiting.is_empty());

    let acts = carry(&mut h, &s, centre(inv.cell(1, k)), centre(l.slot(1, k)));
    assert!(acts.contains(&Action::Craft(CraftAction::AddIngredient {
        recipe: SWORD,
        ordinal: 2,
        item: HILT,
        spinner: 2,
    })));
    hear(&mut h, &mut s, &v, &[added(2, HILT, 2, 7)]);
    assert_eq!(
        h.ui.making.slots,
        BTreeMap::from([(1, INGOT)]),
        "it did not go in"
    );
}

#[test]
fn an_item_carried_out_of_its_slot_leaves_it_and_onto_another_slot_moves_there() {
    let (mut h, mut s) = opened();
    let v = the_view(Some(sword_at(2, &[INGOT, HILT])), 2);
    let mut asked = Vec::new();
    h.ui.making.put_in(INGOT, 1, &mut asked);
    hear(&mut h, &mut s, &v, &[added(1, INGOT, 2, 0)]);
    let k = h.k();
    let l = layout(&h);
    let acts = carry(&mut h, &s, centre(l.slot(0, k)), centre(l.slot(1, k)));
    assert_eq!(
        acts,
        [Action::Craft(CraftAction::AddIngredient {
            recipe: SWORD,
            ordinal: 2,
            item: INGOT,
            spinner: 2,
        })]
    );
    assert!(h.ui.making.slots.is_empty(), "out of the slot it left");
    hear(&mut h, &mut s, &v, &[added(2, INGOT, 2, 0)]);
    assert_eq!(h.ui.making.slots, BTreeMap::from([(2, INGOT)]));
    let acts = carry(&mut h, &s, centre(l.slot(1, k)), (960.0, 300.0));
    assert!(acts.is_empty(), "nothing asked of the game: {acts:?}");
    assert!(h.ui.making.slots.is_empty(), "carried out, it leaves");
}

#[test]
fn the_spinner_asks_for_the_recipe_at_its_value_and_the_slots_follow_the_reading() {
    let (mut h, mut s) = opened();
    let mut asked = Vec::new();
    h.ui.making.put_in(HILT, 2, &mut asked);
    h.ui.making.put_in(INGOT, 1, &mut asked);
    let v = the_view(Some(sword_at(2, &[INGOT, HILT])), 2);
    hear(
        &mut h,
        &mut s,
        &v,
        &[added(2, HILT, 2, 0), added(1, INGOT, 2, 0)],
    );
    let l = layout(&h);
    let out = h.click(&s, centre(l.up));
    assert_eq!(
        out.acts,
        [Action::Craft(CraftAction::Inquire {
            recipe: SWORD,
            spinner: 3,
        })]
    );
    assert_eq!(h.ui.making.spinner, 3);
    assert_eq!(h.ui.making.slots.len(), 2, "kept until the reading");
    // At 3 the sword takes only the ingot, and the hilt no longer fits.
    let at3 = the_view(Some(sword_at(3, &[INGOT])), 3);
    hear(&mut h, &mut s, &at3, &[]);
    assert_eq!(h.ui.making.slots, BTreeMap::from([(1, INGOT)]));
    assert_eq!(h.ui.making.shown().expect("shown").ingredients.len(), 1);
    // Read again there, the ingot no longer fits (the player no longer has it): it leaves.
    let gone = the_view(Some(sword_at(3, &[HILT])), 4);
    hear(&mut h, &mut s, &gone, &[]);
    assert!(h.ui.making.slots.is_empty(), "{:?}", h.ui.making.slots);
    // At 4 it stops.
    let out = h.click(&s, centre(l.up));
    assert_eq!(
        out.acts,
        [Action::Craft(CraftAction::Inquire {
            recipe: SWORD,
            spinner: 4,
        })]
    );
    let out = h.click(&s, centre(l.up));
    assert!(out.acts.is_empty(), "at its greatest");
}

#[test]
fn fill_in_asks_each_empty_slot_that_fills_itself_in_for_the_first_item_that_fits() {
    let mut at = sword_at(2, &[INGOT, HILT]);
    at.auto_populating = true;
    at.auto_populate = BTreeMap::from([(1, true), (2, true)]);
    let (mut h, s) = opened_with(at);
    let out = h.click(&s, centre(layout(&h).fill_in));
    assert_eq!(
        out.acts,
        [
            Action::Craft(CraftAction::AddIngredient {
                recipe: SWORD,
                ordinal: 1,
                item: INGOT,
                spinner: 2,
            }),
            Action::Craft(CraftAction::AddIngredient {
                recipe: SWORD,
                ordinal: 2,
                item: HILT,
                spinner: 2,
            }),
        ]
    );
}

#[test]
fn craft_makes_the_recipe_from_its_slots_and_the_window_waits_until_the_making_is_done() {
    let (mut h, mut s) = opened();
    let v = the_view(Some(sword_at(2, &[INGOT, HILT])), 2);
    let mut asked = Vec::new();
    h.ui.making.put_in(INGOT, 1, &mut asked);
    hear(&mut h, &mut s, &v, &[added(1, INGOT, 2, 0)]);
    let out = h.click(&s, centre(layout(&h).craft));
    assert_eq!(
        out.acts,
        [Action::Craft(CraftAction::ExecuteCraft {
            recipe: SWORD,
            spinner: 2,
            ingredients: vec![(1, INGOT)],
        })]
    );
    assert_eq!(h.ui.making.unlit(false), Some(Unlit::Making));
    let mut closing = Vec::new();
    h.ui.close_top(&mut closing);
    assert!(h.ui.making.is_open(), "it will not close while making");
    assert!(closing.is_empty(), "{closing:?}");
    let acts = hear(&mut h, &mut s, &v, &[made(0)]);
    assert!(acts.is_empty(), "no charges: it stays open");
    assert!(h.ui.making.is_open());
    assert_eq!(h.ui.making.went, Some(true));
    assert!(!h.ui.making.making);
}

#[test]
fn a_making_that_went_with_no_charges_left_closes_the_window() {
    let mut at = sword_at(2, &[INGOT]);
    at.charges = 1;
    let (mut h, mut s) = opened_with(at.clone());
    h.click(&s, centre(layout(&h).craft));
    at.charges = 0;
    let v = the_view(Some(at), 3);
    let acts = hear(&mut h, &mut s, &v, &[made(0)]);
    assert!(!h.ui.making.is_open());
    assert_eq!(
        acts,
        [Action::Craft(CraftAction::Inquire {
            recipe: 0,
            spinner: 0,
        })]
    );
}

#[test]
fn craft_is_not_lit_while_mounted_or_past_what_the_skill_may_make() {
    let mut at = sword_at(2, &[INGOT]);
    at.cutoff = Some(2);
    let (mut h, s) = opened_with(at);
    assert_eq!(
        h.ui.making.unlit(false),
        Some(Unlit::TooHard),
        "30 is 5 over 25"
    );
    let out = h.click(&s, centre(layout(&h).craft));
    assert!(out.acts.is_empty());

    let (mut h, mut s) = opened();
    assert_eq!(h.ui.making.unlit(false), None);
    s.pets.mounted = true;
    assert_eq!(h.ui.making.unlit(true), Some(Unlit::Mounted));
    let out = h.click(&s, centre(layout(&h).craft));
    assert!(out.acts.is_empty(), "mounted: {out:?}");
}

#[test]
fn closing_the_window_tells_the_game_nothing_is_open() {
    let (mut h, _) = opened();
    let mut asked = Vec::new();
    assert!(h.ui.close_top(&mut asked));
    assert_eq!(
        asked,
        [Action::Craft(CraftAction::Inquire {
            recipe: 0,
            spinner: 0,
        })]
    );
    assert!(!h.ui.making.is_open());
}

#[test]
fn a_recipes_name_reads_how_hard_it_is_against_the_craft_skill() {
    let mut at = sword_at(2, &[]);
    assert_eq!(difficulty_colour(&at), 4, "5 over the skill");
    at.skill_level = 120;
    assert_eq!(difficulty_colour(&at), 0, "far under it");
    at.skill_level = 0;
    at.difficulty = 60;
    assert_eq!(difficulty_colour(&at), 5, "far over it");
    at.cutoff = Some(10);
    assert_eq!(difficulty_colour(&at), 6, "past the cutoff");
    at.cutoff = None;
    at.name_coloring = true;
    at.name_color = 2;
    assert_eq!(difficulty_colour(&at), 2, "the recipe's own");
    at.name_coloring = false;
    at.difficulty = 0;
    assert_eq!(difficulty_colour(&at), 0, "no difficulty");
    at.difficulty = 10;
    at.skill_level = 25;
    at.dynamic_difficulty = true;
    at.spinner_difficulty = Some((60, 0));
    assert_eq!(difficulty_colour(&at), 5, "the value's own difficulty");
}

/// A recipe the game opens for making while the making window is open behind another window
/// brings it to the front.
#[test]
fn a_recipe_opened_while_the_making_window_is_behind_another_comes_to_the_front() {
    use auberean_horizon::ui::stack::Window;
    let (mut h, mut s) = opened();
    h.frame(&s);
    h.ui.keyed(Keyed::Skills, &s);
    h.frame(&s);
    h.frame(&s);
    let front = |h: &Harness| h.ui.stack.order().last().copied();
    assert_eq!(
        front(&h),
        Some(Window::Skills),
        "the skill window opened last"
    );
    let v = the_view(Some(sword_at(2, &[INGOT, HILT])), 3);
    follow(
        &mut s,
        &v,
        &[said(Said::Craft(CraftNotice::Open { recipe: SWORD }))],
    );
    h.frame(&s);
    assert!(h.ui.making.is_open());
    assert_eq!(front(&h), Some(Window::Making), "opened again, in front");
}
