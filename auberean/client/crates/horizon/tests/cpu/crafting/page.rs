//! The crafting window: the craft skills, a skill's categories and recipes, the examinations
//! they ask for, opening a recipe, raising a skill, and a recipe carried to the hotbar.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use auberean_client_sdk::contract::notice::examination::kind;
use auberean_client_sdk::contract::{shortcut_kind, Action, CraftAction, ToolbarShortcut};
use auberean_horizon::keys::Keyed;
use auberean_horizon::state::{Bar, GameState, Slot};
use auberean_horizon::ui::crafting::{self, Parts, RECIPE_ROW, ROW};

use super::{following, page, view, ARMOR, BLADES, DAGGER, SMITHING, SWORD, TAILORING};
use crate::{centre, Harness};

fn examined(kind: u32, data_enum: u32, data_id: u32) -> Action {
    Action::Examine {
        kind,
        data_enum,
        item: 0,
        data_id,
        selection: 0,
    }
}

/// The window open on the crafting page; its parts.
fn open(h: &mut Harness, s: &GameState) -> Parts {
    h.ui.keyed(Keyed::Crafting, s);
    h.frame(s);
    h.ui.crafting.parts(h.k()).expect("open")
}

#[test]
fn the_crafting_key_opens_and_closes_the_window() {
    let mut h = Harness::new();
    let s = following(&view(page()));
    assert!(!h.ui.crafting.is_open());
    h.ui.keyed(Keyed::Crafting, &s);
    assert!(h.ui.crafting.is_open());
    h.ui.keyed(Keyed::Crafting, &s);
    assert!(!h.ui.crafting.is_open());
    assert_eq!(
        auberean_horizon::keys::keyed(auberean_horizon::keys::action::CRAFTING),
        Some(Keyed::Crafting)
    );
    let n = auberean_shell::input::Key::N.input_word();
    assert!(
        auberean_shell::interface_keys::HORIZON_KEYS.contains(&(
            n,
            0,
            auberean_shell::interface_keys::own::CRAFTING
        )),
        "N opens it"
    );
    assert_eq!(
        auberean_shell::interface_keys::own::by_name("Crafting"),
        Some(auberean_shell::interface_keys::own::CRAFTING)
    );
}

#[test]
fn a_craft_skill_pressed_is_selected_and_examined_with_its_first_category() {
    let mut h = Harness::new();
    let s = following(&view(page()));
    let parts = open(&mut h, &s);
    let k = h.k();
    let out = h.click(&s, centre(Parts::row(parts.skills, 0, ROW, k)));
    assert_eq!(out.acts, [examined(kind::CRAFT_SKILL, 0, SMITHING)]);
    assert_eq!(h.ui.crafting.skill, SMITHING);
    assert_eq!(h.ui.crafting.category, ARMOR, "the first by its number");
}

#[test]
fn a_skills_categories_are_listed_once_in_order_and_its_unlisted_recipes_left_out() {
    let s = following(&view(page()));
    let smithing = s.craft.skill(SMITHING).expect("smithing");
    assert_eq!(
        crafting::categories(&s.craft, smithing),
        vec![(ARMOR, "Armor".to_owned()), (BLADES, "Blades".to_owned())]
    );
    let blades: Vec<u32> = crafting::recipes(smithing, BLADES)
        .iter()
        .map(|r| r.id)
        .collect();
    assert_eq!(blades, [SWORD, DAGGER]);
    let tailoring = s.craft.skill(TAILORING).expect("tailoring");
    assert!(!tailoring.earns, "earns less than the usual experience");
    assert!(smithing.earns);
    assert_eq!(crafting::level_text(smithing), "Level 25 (+5)");
}

#[test]
fn a_recipe_pressed_is_examined_and_pressed_again_is_opened_as_make_opens_it() {
    let mut h = Harness::new();
    let s = following(&view(page()));
    let parts = open(&mut h, &s);
    let k = h.k();
    h.click(&s, centre(Parts::row(parts.skills, 0, ROW, k)));
    h.click(&s, centre(Parts::row(parts.categories, 1, ROW, k)));
    assert_eq!(h.ui.crafting.category, BLADES);
    let dagger = centre(Parts::row(parts.recipes, 1, RECIPE_ROW, k));
    let first = h.click(&s, dagger);
    assert_eq!(first.acts, [examined(kind::RECIPE, 0, DAGGER)]);
    let second = h.click(&s, dagger);
    assert_eq!(
        second.acts,
        [
            examined(kind::RECIPE, 0, DAGGER),
            Action::Craft(CraftAction::Open(DAGGER))
        ]
    );
    let make = h.click(&s, centre(parts.make));
    assert_eq!(make.acts, [Action::Craft(CraftAction::Open(DAGGER))]);
}

#[test]
fn a_category_pressed_is_examined_by_its_number() {
    let mut h = Harness::new();
    let s = following(&view(page()));
    let parts = open(&mut h, &s);
    let k = h.k();
    h.click(&s, centre(Parts::row(parts.skills, 0, ROW, k)));
    let out = h.click(&s, centre(Parts::row(parts.categories, 1, ROW, k)));
    assert_eq!(out.acts, [examined(kind::RECIPE_CATEGORY, BLADES, 0)]);
}

#[test]
fn raise_is_lit_only_while_the_skill_may_be_raised_now() {
    let mut h = Harness::new();
    let s = following(&view(page()));
    let parts = open(&mut h, &s);
    let k = h.k();
    h.click(&s, centre(Parts::row(parts.skills, 0, ROW, k)));
    let out = h.click(&s, centre(parts.raise));
    assert_eq!(
        out.acts,
        [Action::Craft(CraftAction::RaiseCraftSkill(SMITHING))]
    );
    h.click(&s, centre(Parts::row(parts.skills, 1, ROW, k)));
    let out = h.click(&s, centre(parts.raise));
    assert!(out.acts.is_empty(), "tailoring may not be raised: {out:?}");
}

#[test]
fn a_recipe_carried_onto_the_hotbar_becomes_a_shortcut_to_it() {
    let mut h = Harness::new();
    let mut s = following(&view(page()));
    s.bar = Some(Bar {
        version: 1,
        slots: vec![Slot::default(); 100],
        set: 1,
        rows: 1,
        locked: false,
    });
    let parts = open(&mut h, &s);
    let k = h.k();
    h.click(&s, centre(Parts::row(parts.skills, 0, ROW, k)));
    h.click(&s, centre(Parts::row(parts.categories, 1, ROW, k)));
    let sword = centre(Parts::row(parts.recipes, 0, RECIPE_ROW, k));
    let slot = auberean_horizon::ui::hotbar::slot_rect(h.screen, k, 0, 3);
    let out = h.drag(&s, sword, centre(slot));
    let set: Vec<&Action> = out
        .acts
        .iter()
        .filter(|a| matches!(a, Action::SetToolbarSlot { .. }))
        .collect();
    assert_eq!(
        set,
        [&Action::SetToolbarSlot {
            slot: 3,
            shortcut: ToolbarShortcut {
                kind: shortcut_kind::RECIPE,
                data_id: SWORD,
                ..ToolbarShortcut::default()
            }
        }]
    );
}

#[test]
fn every_craft_skill_reads_its_whole_name_and_no_two_rows_read_the_same() {
    let names = [
        "Butchery",
        "Refining: Dragonsteel",
        "Weapons: Empyrean Magical",
        "Weapons: Human Martial",
        "Weapons: Human Ranged",
    ];
    let mut craft = page();
    let template = craft.skills[1].clone();
    craft.skills = names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let mut s = template.clone();
            s.listed.did = 0x3700_0100 + u32::try_from(i).expect("few");
            s.listed.name = super::text(name);
            s.own = Some(s.listed.clone());
            s
        })
        .collect();
    let s = following(&view(craft));
    for screen in [(1920.0, 1080.0), (1280.0, 960.0)] {
        let mut h = Harness::with_built_in_art();
        h.screen = screen;
        open(&mut h, &s);
        let shown = h.ui.crafting.shown_names().to_vec();
        assert_eq!(shown, names, "{screen:?}: each whole");
        let mut distinct = shown.clone();
        distinct.dedup();
        assert_eq!(distinct.len(), shown.len());
    }
}

/// A craft skill's level, three digits too, is written whole at its row's right and clear of the
/// list's scrollbar while the list scrolls, on a small, a full and a large window.
#[test]
fn a_skills_level_is_whole_and_clear_of_the_lists_scrollbar() {
    use dereth_ui_kit::art::Family;
    use dereth_ui_kit::paint::TextStyle;
    let mut craft = page();
    let template = craft.skills[1].clone();
    craft.skills = (0..30)
        .map(|i| {
            let mut s = template.clone();
            s.listed.did = 0x3700_0200 + i;
            s.level = 125;
            s.raw_level = 125;
            s
        })
        .collect();
    let s = following(&view(craft));
    for screen in [(1280.0, 960.0), (1920.0, 1080.0), (3840.0, 2160.0)] {
        let mut h = Harness::with_built_in_art();
        h.screen = screen;
        let parts = open(&mut h, &s);
        let k = h.k();
        let (bar, wide) = crate::measured(&h, |p| {
            let style = TextStyle::new(Family::Numerals, 12.0, 0xFFFF_FFFF);
            (
                dereth_ui_kit::widgets::scrollbar_width(p).expect("the scrollbar's art"),
                p.measure(&style, "125"),
            )
        });
        let row = Parts::row(parts.skills, 0, ROW, k);
        let level = Parts::level(parts.skills, row, bar, k);
        assert!(
            level.right() <= parts.skills.right() - bar * k - 2.0 * k,
            "{screen:?}: {level:?} clear of the bar in {:?}",
            parts.skills
        );
        assert!(wide <= level.w, "{screen:?}: {wide} in {}", level.w);
    }
}
