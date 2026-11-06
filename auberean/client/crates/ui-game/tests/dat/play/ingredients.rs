//! The ingredient panel on the retail layouts: a recipe opened by the scripts, an item dropped in a
//! slot, the spinner turned, the recipe made.

use std::collections::BTreeMap;

use auberean_text::StringInfo;
use auberean_ui::types::button_id;
use auberean_ui::{Handle, Token, Ui};
use auberean_ui_game::craft::Facts as CraftFacts;
use auberean_ui_game::gameplay::{GamePlay, Notice, Request, View};
use auberean_ui_game::ingredients::{notice, Ingredient, IngredientRequest, RecipeAt};
use auberean_ui_game::inventory::{Container, Contents, Facts, ItemFacts};
use auberean_ui_game::toolbar::action as tb;

use crate::common::records;

const PLAYER: u64 = 0x5000_0000_0000_0001;
const SWORD: u64 = 0x5000_0000_0000_0010;
const ORE: u64 = 0x5000_0000_0000_0011;
const SWORD_ICON: u32 = 0x4100_0ADE;
const RECIPE: u32 = 0x3800_0200;

fn item(name: &str) -> ItemFacts {
    ItemFacts {
        quantity: 1,
        icon: SWORD_ICON,
        icon_layers: Vec::new(),
        stackable: false,
        stack_size: 1,
        name: name.to_owned(),
        wear: None,
        preferred_location: 0,
        had: true,
        worn: false,
        melts_at_once: false,
        offered_name: None,
        data_id: 0,
    }
}

/// The player with a sword and some ore in the main pack.
fn inventory() -> Facts {
    let pack = vec![SWORD, ORE];
    let slots = vec![(0, 2), (2, 0), (2, 0), (2, 0)];
    let segments = slots
        .iter()
        .map(|(first, count)| Contents {
            items: pack.clone(),
            first: *first,
            count: *count,
        })
        .collect();
    let mut f = Facts {
        version: 1,
        player: PLAYER,
        segments: slots.clone(),
        coin_stack: 1000,
        ..Facts::default()
    };
    f.containers.insert(
        PLAYER,
        Container {
            size: 78,
            segments,
            slots,
        },
    );
    f.items.insert(SWORD, item("Sword"));
    f.items.insert(ORE, item("Ore"));
    f
}

fn ingredient(id: u32, name: &str) -> Ingredient {
    Ingredient {
        id,
        description: StringInfo::literal(name),
    }
}

/// A recipe with two ingredients at spinner value 1 (a blade and ore, the ore filling itself in)
/// and a third at 2, the sword fitting the blade's slot and the ore the ore's; one charge left.
fn recipe_at(spinner: i32) -> RecipeAt {
    let mut ingredients = vec![ingredient(1, "Blade"), ingredient(2, "Ore")];
    let mut mask = 0b11;
    if spinner == 2 {
        ingredients.push(ingredient(3, "Flux"));
        mask = 0b111;
    }
    let mut fits = BTreeMap::new();
    for i in &ingredients {
        fits.insert((i.id, SWORD), i.id == 1);
        fits.insert((i.id, ORE), i.id == 2);
    }
    let mut auto_populate = BTreeMap::new();
    auto_populate.insert(2, true);
    RecipeAt {
        recipe: RECIPE,
        spinner,
        name: StringInfo::literal("Iron Sword"),
        charges: 1,
        min_spinner: 1,
        max_spinner: 2,
        last_spinner: 1,
        auto_populating: true,
        max_ingredients: 3,
        cost: 25,
        craft_skill: 0x3700_0010,
        skill_level: 40,
        difficulty: 30,
        mask,
        ingredients,
        auto_populate,
        items: vec![SWORD, ORE],
        fits,
        ..RecipeAt::default()
    }
}

struct World {
    ui: Ui,
    play: GamePlay,
    t: f64,
    inventory: Facts,
    craft: CraftFacts,
}

impl World {
    fn new() -> Self {
        Self::over(records())
    }

    fn over(assets: std::sync::Arc<dyn auberean_ui::Assets + Send + Sync>) -> Self {
        let mut ui = Ui::new(assets, 1024, 768);
        let mut out = Vec::new();
        let mut play = GamePlay::new(&mut ui, &mut out);
        play.login_finished(&mut ui, &mut out);
        Self {
            ui,
            play,
            t: 0.0,
            inventory: inventory(),
            craft: CraftFacts {
                version: 1,
                now: 100.0,
                opened: Some(recipe_at(1)),
                ..CraftFacts::default()
            },
        }
    }

    fn tick(&mut self, notices: &[Notice]) -> Vec<Request> {
        self.t += 0.05;
        self.ui.use_time(self.t);
        let view = View {
            inventory: Some(&self.inventory),
            craft: Some(&self.craft),
            everyone: notices,
            ..View::default()
        };
        let mut out = Vec::new();
        self.play.tick(&mut self.ui, &view, notices, &mut out);
        out
    }

    fn el(&self, name: &str) -> Handle {
        self.ui
            .element(Token::of(name))
            .unwrap_or_else(|| panic!("{name} made"))
    }

    fn middle(&self, h: Handle) -> (i32, i32) {
        let r = self.ui.screen_rect(h).expect("placed");
        ((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2)
    }

    fn click(&mut self, h: Handle) -> Vec<Request> {
        let (x, y) = self.middle(h);
        self.ui.pointer_move(x, y);
        let mut out = self.tick(&[]);
        for down in [true, false] {
            self.ui.pointer_button(button_id::LEFT, down);
            out.extend(self.tick(&[]));
        }
        out
    }

    fn drag(&mut self, from: Handle, to: Handle) -> Vec<Request> {
        let (x, y) = self.middle(from);
        self.ui.pointer_move(x, y);
        let mut out = self.tick(&[]);
        self.ui.pointer_button(button_id::LEFT, true);
        out.extend(self.tick(&[]));
        self.ui.pointer_move(x + 8, y + 8);
        out.extend(self.tick(&[]));
        assert!(self.ui.dragged().is_some(), "dragging");
        let (tx, ty) = self.middle(to);
        self.ui.pointer_move(tx, ty);
        out.extend(self.tick(&[]));
        self.ui.pointer_button(button_id::LEFT, false);
        out.extend(self.tick(&[]));
        out
    }

    fn slots(&self) -> Vec<Handle> {
        self.ui.list_items(self.el("IngredientListBox"))
    }

    fn label_of(&self, row: Handle) -> String {
        self.ui
            .child_recursive(row, Token::of("IngredientLabel"))
            .and_then(|l| self.ui.text(l))
            .unwrap_or_default()
    }
}

fn said(native: u32, words: Vec<u32>) -> Notice {
    Notice {
        native,
        about: Some(PLAYER),
        words,
        records: Vec::new(),
    }
}

fn adds(out: &[Request]) -> Vec<IngredientRequest> {
    out.iter()
        .filter_map(|r| match r {
            Request::Ingredients(i) => Some(i.clone()),
            _ => None,
        })
        .collect()
}

/// The scripts open the panel on a recipe: its name, its cost and charges, a slot per ingredient
/// it takes at its last spinner value. An item dropped on a slot is asked to go in; when the
/// scripts say it went, the slot shows its icon. Fill-in puts the first fitting item in each
/// empty slot that fills itself in. Turning the spinner asks for the recipe there and brings the
/// slots it takes. The craft button asks to make the recipe from the items in the slots; when it
/// is done and no charges are left, the panel closes.
///
/// Behaviour: ui.craft.the-ingredient-panel-takes-items-and-makes-the-recipe
#[test]
fn the_ingredient_panel_takes_items_and_makes_the_recipe() {
    let mut w = World::new();
    w.tick(&[]);
    let field = w.el("RealIngredientField");
    assert!(!w.ui.shown(field), "hidden at first");
    w.tick(&[said(notice::OPEN_INGREDIENT_PANEL, vec![RECIPE])]);
    assert!(w.ui.shown(field), "open");
    assert_eq!(
        w.ui.text(w.el("RecipeLabel")).as_deref(),
        Some("Iron Sword")
    );
    assert!(w
        .ui
        .text(w.el("CostField"))
        .is_some_and(|t| t.contains("25")));
    let names: Vec<String> = w.slots().iter().map(|r| w.label_of(*r)).collect();
    assert_eq!(names, ["Blade", "Ore"]);
    assert_eq!(w.ui.text(w.el("SpinnerValueField")).as_deref(), Some("1"));

    // The sword dropped on the blade's slot.
    w.ui.key(tb::INVENTORY_PANEL, true);
    w.tick(&[]);
    w.ui.key(tb::INVENTORY_PANEL, false);
    w.tick(&[]);
    let sword = w
        .play
        .child::<auberean_ui_game::toolbar::Toolbar>()
        .and_then(|t| t.child::<auberean_ui_game::inventory::Inventory>())
        .and_then(|i| i.backpack())
        .and_then(|b| b.packs()[0].item_display(SWORD))
        .and_then(|d| d.icon())
        .expect("the sword's icon");
    let blade = w.slots()[0];
    let out = w.drag(sword, blade);
    assert_eq!(
        adds(&out),
        vec![IngredientRequest::Add {
            recipe: RECIPE,
            ordinal: 1,
            item: SWORD,
            spinner: 1,
        }]
    );
    assert_eq!(w.ui.node(blade).expect("slot").state, Token::of("pending"));
    let hi = u32::try_from(SWORD >> 32).unwrap_or(0);
    let lo = u32::try_from(SWORD & 0xFFFF_FFFF).unwrap_or(0);
    w.tick(&[said(
        notice::ADD_INGREDIENT_DONE,
        vec![RECIPE, 1, hi, lo, 1, 0],
    )]);
    assert_eq!(
        w.ui.node(blade).expect("slot").state,
        Token::of("stateless")
    );
    assert_eq!(
        w.play
            .child::<auberean_ui_game::ingredients::Ingredients>()
            .map(|i| i.state().2.clone()),
        Some([(1, SWORD)].into_iter().collect())
    );

    // Fill in: the ore goes in its slot.
    let out = w.click(w.el("AutoPopulateButton"));
    assert_eq!(
        adds(&out),
        vec![IngredientRequest::Add {
            recipe: RECIPE,
            ordinal: 2,
            item: ORE,
            spinner: 1,
        }]
    );

    // The spinner up: the recipe at 2 asked for, then its third slot.
    let out = w.click(w.el("SpinnerUpButton"));
    assert!(
        adds(&out).contains(&IngredientRequest::Inquire {
            recipe: RECIPE,
            spinner: 2
        }),
        "{out:?}"
    );
    w.craft.version = 2;
    w.craft.opened = Some(recipe_at(2));
    w.tick(&[]);
    let names: Vec<String> = w.slots().iter().map(|r| w.label_of(*r)).collect();
    assert_eq!(names, ["Blade", "Ore", "Flux"]);

    // Made, the last charge used: the panel closes.
    let out = w.click(w.el("CraftButton"));
    assert_eq!(
        adds(&out),
        vec![IngredientRequest::Execute {
            recipe: RECIPE,
            spinner: 2,
            ingredients: vec![(1, SWORD)],
        }]
    );
    let mut done = recipe_at(2);
    done.charges = 0;
    w.craft.version = 3;
    w.craft.opened = Some(done);
    w.tick(&[]);
    let out = w.tick(&[said(notice::EXECUTE_CRAFT_DONE, vec![RECIPE, 0])]);
    assert!(!w.ui.shown(field), "closed");
    assert!(adds(&out).contains(&IngredientRequest::Inquire {
        recipe: 0,
        spinner: 0
    }));
}

/// A recipe's name and its ingredients' names are given to their text elements as string
/// references: one whose string table the data lacks shows once the table has come. The recipe's
/// name is then colored by how hard it is, written again from its element's text: one still
/// waiting stays empty, as in the client.
///
/// Behaviour: ui.craft.recipe-and-ingredient-names-wait-for-their-string-tables
#[test]
fn recipe_and_ingredient_names_wait_for_their_string_tables() {
    use super::stage::{Lacking, LACKING};
    let assets = Lacking::new();
    let mut w = World::over(assets.clone());
    let mut r = recipe_at(1);
    r.name = StringInfo::entry(LACKING, 1);
    r.ingredients[0].description = StringInfo::entry(LACKING, 2);
    w.craft.opened = Some(r);
    w.tick(&[]);
    w.tick(&[said(notice::OPEN_INGREDIENT_PANEL, vec![RECIPE])]);
    assert_eq!(w.ui.text(w.el("RecipeLabel")).as_deref(), Some(""));
    let names: Vec<String> = w.slots().iter().map(|r| w.label_of(*r)).collect();
    assert_eq!(names, ["", "Ore"], "waiting on the table");
    assets.arrive();
    w.tick(&[]);
    assert_eq!(
        w.ui.text(w.el("RecipeLabel")).as_deref(),
        Some(""),
        "the name's color written over it while it waited"
    );
    let names: Vec<String> = w.slots().iter().map(|r| w.label_of(*r)).collect();
    assert!(names[0].starts_with("From the server"), "{names:?}");
}

/// The spinner's value as the client writes it: signed when the panel opens, unsigned once
/// changed (a negative value written as its 32-bit word, of which the field shows the first three
/// characters), and written only when the field reads otherwise, both read as unsigned words; a
/// typed value is read as the C library reads a number in base 0 (a leading 0 octal) and kept as
/// typed when it is the value taken.
///
/// Behaviour: ui.craft.the-spinner-writes-a-changed-value-unsigned
#[test]
fn the_spinner_writes_a_changed_value_unsigned() {
    let mut w = World::new();
    let mut r = recipe_at(1);
    r.min_spinner = -3;
    r.max_spinner = 20;
    r.last_spinner = -1;
    w.craft.opened = Some(r);
    w.tick(&[]);
    w.tick(&[said(notice::OPEN_INGREDIENT_PANEL, vec![RECIPE])]);
    let value = w.el("SpinnerValueField");
    let shown = |w: &World| w.ui.text(value).unwrap_or_default();
    assert_eq!(shown(&w), "-1", "opened: signed");
    // -2 written as 4294967294, of which the field holds three characters.
    w.click(w.el("SpinnerDownButton"));
    assert_eq!(shown(&w), "429", "changed: unsigned");
    w.click(w.el("SpinnerUpButton"));
    assert_eq!(shown(&w), "429", "-1 as 4294967295");
    w.click(w.el("SpinnerUpButton"));
    assert_eq!(shown(&w), "0");
    w.ui.set_text(value, "");
    w.click(value);
    w.ui.type_text("010");
    w.tick(&[]);
    w.craft.now += 3.0;
    let mut out = w.tick(&[]);
    out.extend(w.tick(&[]));
    assert!(
        adds(&out).contains(&IngredientRequest::Inquire {
            recipe: RECIPE,
            spinner: 8
        }),
        "{out:?}"
    );
    assert_eq!(shown(&w), "010", "the value read is the value taken");
}

/// The making's end reads the recipe as it stands that very frame, so the last charge used closes
/// the panel though the answer came with the notice, even with an answer for another spinner
/// value in.
///
/// Behaviour: ui.craft.the-makings-end-reads-the-charges-of-that-frame
#[test]
fn the_makings_end_reads_the_charges_of_that_frame() {
    let mut w = World::new();
    w.craft.opened = Some(recipe_at(1));
    w.tick(&[]);
    w.tick(&[said(notice::OPEN_INGREDIENT_PANEL, vec![RECIPE])]);
    let field = w.el("RealIngredientField");
    assert!(w.ui.shown(field), "open");
    let mut done = recipe_at(2);
    done.charges = 0;
    w.craft.version = 2;
    w.craft.opened = Some(done);
    w.tick(&[said(notice::EXECUTE_CRAFT_DONE, vec![RECIPE, 0])]);
    assert!(!w.ui.shown(field), "closed by the charges just answered");
}
