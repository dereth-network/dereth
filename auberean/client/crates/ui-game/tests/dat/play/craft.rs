//! The skill window's crafting page on the retail layouts: the craft skills, a craft skill's
//! categories, a category's recipes, as the scripts answer for them.

use std::collections::BTreeMap;

use auberean_text::StringInfo;
use auberean_ui::types::button_id;
use auberean_ui::{Handle, Token, Ui};
use auberean_ui_game::craft::{examine, CraftRequest, CraftSkill, Facts, Recipe, SkillFacts};
use auberean_ui_game::examination::ExamRequest;
use auberean_ui_game::gameplay::{notice, Examination, Notice, Record, Request};
use auberean_ui_game::toolbar::action as tb;

use super::dialogs::World;

const SMITHING: u32 = 0x3700_0010;
const TAILORING: u32 = 0x3700_0011;
const WEAPONS: u32 = 0x4000_0021;
const ARMOR: u32 = 0x4000_0022;
const SWORD: u32 = 0x3800_0100;
const AXE: u32 = 0x3800_0101;
const HELM: u32 = 0x3800_0102;
const HIDDEN: u32 = 0x3800_0103;

fn skill(did: u32, name: &str, level: u32, raw: u32, xp_mod: f32) -> CraftSkill {
    CraftSkill {
        did,
        name: StringInfo::literal(name),
        level,
        raw_level: raw,
        cost_to_raise: 2500,
        icon: 0x0600_3000,
        usage_xp_mod: xp_mod,
    }
}

fn recipe(did: u32, name: &str, category: u32, shown: bool, used_again: f64) -> Recipe {
    Recipe {
        did,
        name: StringInfo::literal(name),
        icon: if did == AXE { 0 } else { 0x0600_4000 },
        shown,
        craft_skill: SMITHING,
        category,
        time_use_again: used_again,
    }
}

/// A smith (raised by a buff, its use earning the usual experience, raiseable now) with a sword
/// and an axe among its weapons, a helm among its armor and a hidden recipe; a tailor whose use
/// earns less.
fn facts() -> Facts {
    let smith = skill(SMITHING, "Smithing", 42, 40, 1.0);
    let mut categories = BTreeMap::new();
    categories.insert(WEAPONS, StringInfo::literal("Weapons"));
    categories.insert(ARMOR, StringInfo::literal("Armor"));
    Facts {
        version: 1,
        skills: vec![
            SkillFacts {
                listed: smith.clone(),
                own: Some(smith),
                raw_level: 40,
                level: 42,
                raiseable: true,
                can_raise: true,
                recipes: vec![
                    recipe(HELM, "Helm", ARMOR, true, 0.0),
                    recipe(SWORD, "Sword", WEAPONS, true, 0.0),
                    recipe(AXE, "Axe", WEAPONS, true, 500.0),
                    recipe(HIDDEN, "Hidden", WEAPONS, false, 0.0),
                ],
            },
            SkillFacts {
                listed: skill(TAILORING, "Tailoring", 5, 5, 0.5),
                own: Some(skill(TAILORING, "Tailoring", 5, 5, 0.5)),
                raw_level: 5,
                level: 5,
                ..SkillFacts::default()
            },
        ],
        experience: 9000,
        categories,
        now: 100.0,
        opened: None,
    }
}

fn find(ui: &Ui, name: &str) -> Handle {
    let page = ui.element(Token::of("CraftPage")).expect("the page");
    ui.child_recursive(page, Token::of(name))
        .unwrap_or_else(|| panic!("{name} on the page"))
}

fn rows(ui: &Ui, list: &str) -> Vec<Handle> {
    ui.list_items(find(ui, list))
}

fn child_text(ui: &Ui, h: Handle, name: &str) -> String {
    ui.child_recursive(h, Token::of(name))
        .and_then(|t| ui.text(t))
        .unwrap_or_default()
}

fn child_state(ui: &Ui, h: Handle, name: &str) -> Token {
    let c = ui.child_recursive(h, Token::of(name)).expect(name);
    ui.node(c).expect("node").state
}

fn state(ui: &Ui, h: Handle) -> Token {
    ui.node(h).expect("node").state
}

/// Press an element with a button (twice for the double ones), a frame for each step.
fn press(w: &mut World, h: Handle, twice: bool) -> Vec<Request> {
    let r = w.ui.screen_rect(h).expect("placed");
    w.ui.pointer_move((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2);
    let mut out = w.tick(&[]);
    let presses: &[u32] = if twice {
        &[button_id::LEFT, button_id::DOUBLE_LEFT]
    } else {
        &[button_id::LEFT]
    };
    for b in presses {
        w.ui.pointer_button(*b, true);
        out.extend(w.tick(&[]));
        w.ui.pointer_button(*b, false);
        out.extend(w.tick(&[]));
    }
    out
}

fn request_changed(kind: u32, data_enum: u32, data_id: u32) -> Notice {
    Notice {
        native: notice::EXAMINATION_REQUEST_CHANGED,
        about: Some(1),
        words: vec![0],
        records: vec![Some(Record::Request(ExamRequest {
            kind,
            data_enum,
            data_id,
            ..ExamRequest::default()
        }))],
    }
}

/// The crafting tab shows the craft skills: each its name (disabled while its use earns less
/// than the usual experience) and rating (buffed when its level stands above its own). A craft
/// skill's forward button selects it (the examination asked for it) and shows its categories: it
/// at the head, its display (the raise button lit, what raising costs against the experience to
/// spend), a row per category of its recipes in order of their numbers. A category's forward
/// button shows its recipes that are shown (one not yet usable again disabled); the examination's
/// request for a recipe marks its row; a recipe pressed twice is opened; the back button goes up;
/// the raise button raises the craft skill.
///
/// Behaviour: ui.craft.the-crafting-page-goes-from-craft-skills-to-categories-to-recipes
#[test]
fn the_crafting_page_goes_from_craft_skills_to_categories_to_recipes() {
    let mut w = World::new();
    w.craft = Some(facts());
    w.key(tb::SKILL_PANEL);
    let tab =
        w.ui.element(Token::of("CraftTab"))
            .expect("the crafting tab");
    press(&mut w, tab, false);
    let page = w.ui.element(Token::of("CraftPage")).expect("the page");
    assert_eq!(state(&w.ui, page), Token::of("craftskillview"));
    let skills = rows(&w.ui, "CraftSkillList");
    let names: Vec<String> = skills
        .iter()
        .map(|r| child_text(&w.ui, *r, "Name"))
        .collect();
    assert_eq!(names, ["Smithing", "Tailoring"]);
    assert!(child_text(&w.ui, skills[0], "Level").contains("42"));
    assert_eq!(child_state(&w.ui, skills[0], "Level"), Token::of("buff"));
    assert_eq!(child_state(&w.ui, skills[0], "Name"), Token::of("enabled"));
    assert_eq!(child_state(&w.ui, skills[1], "Name"), Token::of("disabled"));

    // Forward: the smith selected, its categories.
    let forward =
        w.ui.child_recursive(skills[0], Token::of("ForwardButton"))
            .expect("forward");
    let out = w.click(forward);
    assert!(
        out.contains(&Request::Examine(Examination {
            kind: examine::CRAFT_SKILL,
            data_id: SMITHING,
            ..Examination::default()
        })),
        "{out:?}"
    );
    assert!(
        !out.contains(&Request::OpenExaminationPanel),
        "the examination window stays as it is: {out:?}"
    );
    assert_eq!(state(&w.ui, page), Token::of("craftskillcategoryview"));
    let head = find(&w.ui, "CraftSkillCategoryHeader");
    assert_eq!(child_text(&w.ui, head, "Name"), "Smithing");
    let cats: Vec<String> = rows(&w.ui, "CraftSkillCategoryList")
        .iter()
        .map(|r| child_text(&w.ui, *r, "Name"))
        .collect();
    assert_eq!(cats, ["Weapons", "Armor"], "by the categories' numbers");
    let misc = w.ui.text(find(&w.ui, "MiscText")).unwrap_or_default();
    assert!(misc.contains("9,000") || misc.contains("9000"), "{misc}");
    let raise = find(&w.ui, "RaiseCraftButton");
    assert!(!w
        .ui
        .node(raise)
        .expect("node")
        .bool_attr(auberean_ui::token::attr::GHOSTED));

    // Forward on weapons: its recipes that are shown.
    let weapons = rows(&w.ui, "CraftSkillCategoryList")[0];
    let forward =
        w.ui.child_recursive(weapons, Token::of("ForwardButton"))
            .expect("forward");
    w.click(forward);
    assert_eq!(state(&w.ui, page), Token::of("recipeview"));
    let recipes = rows(&w.ui, "RecipeList");
    let names: Vec<String> = recipes
        .iter()
        .map(|r| child_text(&w.ui, *r, "Name"))
        .collect();
    assert_eq!(names, ["Sword", "Axe"]);
    assert_eq!(child_state(&w.ui, recipes[0], "Name"), Token::of("enabled"));
    assert_eq!(
        child_state(&w.ui, recipes[1], "Name"),
        Token::of("disabled")
    );
    w.tick(&[request_changed(examine::RECIPE, 0, SWORD)]);
    assert_eq!(state(&w.ui, recipes[0]), Token::of("selected"));

    // The sword pressed twice is opened.
    let out = press(&mut w, recipes[0], true);
    assert!(
        out.contains(&Request::Craft(CraftRequest::Open(SWORD))),
        "{out:?}"
    );
    let out = w.click(find(&w.ui, "RaiseCraftButton"));
    assert!(
        out.contains(&Request::Craft(CraftRequest::RaiseCraftSkill(SMITHING))),
        "{out:?}"
    );

    // Back to the categories.
    let head = find(&w.ui, "RecipeHeader");
    let back =
        w.ui.child_recursive(head, Token::of("BackButton"))
            .expect("back");
    w.click(back);
    assert_eq!(state(&w.ui, page), Token::of("craftskillcategoryview"));
}

/// An element's place on the page: its top and bottom against the page's top.
fn on_page(ui: &Ui, page: Handle, name: &str) -> (i32, i32) {
    let top = ui.screen_rect(page).expect("placed").y0;
    let r = ui.screen_rect(find(ui, name)).expect("placed");
    (r.y0 - top, r.y1 - top)
}

/// The page's bottom against its top.
fn page_bottom(ui: &Ui, page: Handle) -> i32 {
    let r = ui.screen_rect(page).expect("placed");
    r.y1 - r.y0
}

/// The skill window on a screen of a size, the crafting tab up, down at a craft skill's categories.
fn categories_at(width: i32, height: i32) -> (World, Handle) {
    let mut w = World::new();
    w.craft = Some(facts());
    w.ui.refresh(width, height);
    w.key(tb::SKILL_PANEL);
    let tab =
        w.ui.element(Token::of("CraftTab"))
            .expect("the crafting tab");
    press(&mut w, tab, false);
    let page = w.ui.element(Token::of("CraftPage")).expect("the page");
    let skills = rows(&w.ui, "CraftSkillList");
    let forward =
        w.ui.child_recursive(skills[0], Token::of("ForwardButton"))
            .expect("forward");
    w.click(forward);
    assert_eq!(state(&w.ui, page), Token::of("craftskillcategoryview"));
    (w, page)
}

/// At a window height of 768 and below the page is its layout's size and everything on it stands
/// where the layout puts it, in a craft skill's categories and a category's recipes.
///
/// Behaviour: ui.craft.the-page-stands-as-laid-out-up-to-a-height-of-768
#[test]
fn the_page_stands_as_laid_out_up_to_a_height_of_768() {
    for (width, height) in [(1024, 768), (1024, 700), (800, 600), (640, 480)] {
        let (mut w, page) = categories_at(width, height);
        let at = format!("{width}x{height}");
        let bottom = page_bottom(&w.ui, page);
        assert_eq!(bottom, 363, "{at}");
        for (head, list) in [
            ("CraftSkillCategoryHeader", "CraftSkillCategoryList"),
            ("RecipeHeader", "RecipeList"),
        ] {
            assert_eq!(on_page(&w.ui, page, head), (20, 49), "{at} {head}");
            assert_eq!(on_page(&w.ui, page, list), (50, 262), "{at} {list}");
            assert_eq!(
                on_page(&w.ui, page, "SelectedCraftSkill"),
                (267, 346),
                "{at} {head}"
            );
            if head != "RecipeHeader" {
                let weapons = rows(&w.ui, list)[0];
                let forward =
                    w.ui.child_recursive(weapons, Token::of("ForwardButton"))
                        .expect("forward");
                w.click(forward);
            }
        }
    }
}

/// On a window taller than 768 the head of a craft skill's categories and of a category's recipes
/// keeps its height, the display goes down with the page's foot and the list takes the rest: the
/// three never overlap, and a press on a row reaches the row, not the head, at every size.
///
/// Behaviour: ui.craft.a-tall-window-grows-the-list-and-not-the-head
#[test]
fn a_tall_window_grows_the_list_and_not_the_head() {
    for (width, height) in [
        (1024, 769),
        (1280, 960),
        (1280, 1004),
        (1600, 1180),
        (2560, 1440),
    ] {
        let (mut w, page) = categories_at(width, height);
        let at = format!("{width}x{height}");
        let bottom = page_bottom(&w.ui, page);
        let grown = bottom - 363;
        assert!(grown > 0, "{at}: the page grew with the window");
        for (head, list) in [
            ("CraftSkillCategoryHeader", "CraftSkillCategoryList"),
            ("RecipeHeader", "RecipeList"),
        ] {
            assert_eq!(on_page(&w.ui, page, head), (20, 49), "{at} {head}");
            assert_eq!(on_page(&w.ui, page, list), (50, 262 + grown), "{at} {list}");
            assert_eq!(
                on_page(&w.ui, page, "SelectedCraftSkill"),
                (267 + grown, 346 + grown),
                "{at} {head}"
            );
            let row = rows(&w.ui, list)[0];
            let out = press(&mut w, row, false);
            assert!(
                out.iter().any(|r| matches!(
                    r,
                    Request::Examine(Examination { kind, .. })
                        if *kind == examine::CATEGORY || *kind == examine::RECIPE
                )),
                "{at} {list}: the row hears the press: {out:?}"
            );
            let button = if head == "RecipeHeader" {
                "RecipeButton"
            } else {
                "ForwardButton"
            };
            let forward =
                w.ui.child_recursive(row, Token::of(button))
                    .expect("the row's button");
            let out = w.click(forward);
            if head == "RecipeHeader" {
                assert!(
                    out.contains(&Request::Craft(CraftRequest::Open(SWORD))),
                    "{at}: the row's button opens its recipe: {out:?}"
                );
            } else {
                assert_eq!(
                    state(&w.ui, page),
                    Token::of("recipeview"),
                    "{at}: the row's button goes a view deeper"
                );
            }
        }
    }
}
