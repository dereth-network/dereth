//! Crafting: the crafting window's craft skills, categories and recipes, the making window's
//! slots, spinner and Craft button, and crafting as Horizon follows it from the view and the
//! notices; and what their tests share, a craft page as the view gives it, followed into a state.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

mod making;
mod page;

use std::collections::BTreeMap;

use auberean_client_sdk::contract::notice::Notice;
use auberean_client_sdk::contract::text::TextRef;
use auberean_client_sdk::contract::view::{
    self as v, Craft, CraftSkill, CraftSkillFacts, Ingredient, Recipe, RecipeAt, View,
};
use auberean_horizon::state::{Frame, GameState};

use crate::PLAYER;

pub const SMITHING: u32 = 0x3700_0001;
pub const TAILORING: u32 = 0x3700_0002;
pub const BLADES: u32 = 3;
pub const ARMOR: u32 = 1;
pub const SWORD: u32 = 0x3800_0010;
pub const DAGGER: u32 = 0x3800_0011;
pub const HELM: u32 = 0x3800_0012;
pub const HIDDEN: u32 = 0x3800_0013;

/// Text as the view gives it: its words themselves.
pub fn text(s: &str) -> TextRef {
    TextRef::literal(s)
}

fn recipe(did: u32, name: &str, category: u32, shown: bool) -> Recipe {
    Recipe {
        did,
        name: text(name),
        icon: 0x0600_0200,
        shown,
        craft_skill: SMITHING,
        category,
        time_use_again: 0.0,
    }
}

fn skill(did: u32, name: &str, level: u32, raw: u32, xp_mod: f32) -> CraftSkill {
    CraftSkill {
        did,
        name: text(name),
        level,
        raw_level: raw,
        cost_to_raise: 1500,
        icon: 0x0600_0100,
        usage_xp_mod: xp_mod,
    }
}

/// The crafting page: Smithing (raised for now, may be raised) with a sword and a dagger in
/// Blades, a helm in Armor and one recipe not listed; Tailoring with none.
pub fn page() -> Craft {
    let smithing = skill(SMITHING, "Smithing", 25, 20, 1.0);
    let tailoring = skill(TAILORING, "Tailoring", 5, 5, 0.5);
    Craft {
        version: 1,
        skills: vec![
            CraftSkillFacts {
                listed: smithing.clone(),
                own: Some(smithing),
                raw_level: 20,
                level: 25,
                raiseable: true,
                can_raise: true,
                recipes: vec![
                    recipe(SWORD, "Sword", BLADES, true),
                    recipe(HELM, "Helm", ARMOR, true),
                    recipe(DAGGER, "Dagger", BLADES, true),
                    recipe(HIDDEN, "Secret", BLADES, false),
                ],
            },
            CraftSkillFacts {
                listed: tailoring.clone(),
                own: Some(tailoring),
                raw_level: 5,
                level: 5,
                ..CraftSkillFacts::default()
            },
        ],
        experience: 9000,
        categories: BTreeMap::from([(BLADES, text("Blades")), (ARMOR, text("Armor"))]),
        opened: None,
    }
}

/// The sword at a spinner value: two slots (or one at 3 and up), from 1 to 4, last made at 2.
pub fn sword_at(spinner: i32, items: &[u64]) -> RecipeAt {
    let mut ingredients = vec![Ingredient {
        id: 1,
        description: text("Ingot"),
    }];
    if spinner < 3 {
        ingredients.push(Ingredient {
            id: 2,
            description: text("Hilt"),
        });
    }
    let mask = if spinner < 3 { 3 } else { 1 };
    let mut fits = BTreeMap::new();
    for item in items {
        fits.insert((1, *item), true);
        if spinner < 3 {
            fits.insert((2, *item), true);
        }
    }
    RecipeAt {
        recipe: SWORD,
        spinner,
        name: text("Sword"),
        charges: -1,
        min_spinner: 1,
        max_spinner: 4,
        last_spinner: 2,
        max_ingredients: 2,
        difficulty: 30,
        craft_skill: SMITHING,
        skill_level: 25,
        cost: 12,
        mask,
        ingredients,
        items: items.to_vec(),
        fits,
        ..RecipeAt::default()
    }
}

/// The view with the crafting page and, when given, the recipe opened.
pub fn view(craft: Craft) -> View {
    View {
        player_id: Some(PLAYER),
        player: Some(v::Player {
            id: PLAYER,
            ..v::Player::default()
        }),
        craft: Some(craft),
        ..View::default()
    }
}

fn written(t: &TextRef) -> Option<String> {
    t.literal.as_ref().map(|l| String::from_utf16_lossy(l))
}

/// Follow one frame of `view` and `notices` into `s`.
pub fn follow(s: &mut GameState, view: &View, notices: &[Notice]) {
    s.follow(&Frame {
        view: Some(view),
        notices,
        events: &[],
        told: &[],
        lines: &[],
        write: &written,
        unknown_command: "",
    });
}

/// A state following `view` once.
pub fn following(view: &View) -> GameState {
    let mut s = GameState::default();
    follow(&mut s, view, &[]);
    s
}
