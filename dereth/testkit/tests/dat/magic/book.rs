use dereth_primitives::DataId;
use dereth_testkit::HeadlessClient;
use dereth_ui_screens::panels::remaining::SPELL_PAGE;
use dereth_ui_screens::panels::spellbook::{SpellbookPanel, DEFAULT_SPELL_FILTERS};
use dereth_ui_screens::view::{GameView, SpellEntry};

use super::examine;

const CREATURE: u32 = 4;
const LIFE: u32 = 2;
/// The flag whose badge the shipped overlay table draws over a spell's picture.
pub const FELLOWSHIP: u32 = 0x2000;

/// A book the scenario writes. See this section's header for why it is not the production
/// host's.
#[derive(Debug)]
pub struct Book(pub Vec<SpellEntry>);

impl GameView for Book {
    fn spellbook(&self) -> &[SpellEntry] {
        &self.0
    }
    fn spell_filters(&self) -> u32 {
        DEFAULT_SPELL_FILTERS
    }
}

fn entry(id: u32, name: &str, school: u32, level: u32, display_order: i32) -> SpellEntry {
    SpellEntry {
        id,
        name: name.to_owned(),
        icon: Some(DataId(0x0600_1000 + id)),
        school,
        level,
        icon_power: level,
        display_order,
        bitfield: 0,
    }
}

pub fn a_book() -> Book {
    Book(vec![
        entry(157, "Strength Self I", CREATURE, 1, 10),
        entry(1074, "Heal Self I", LIFE, 1, 20),
        entry(158, "Strength Self II", CREATURE, 2, 30),
    ])
}

pub fn ids(b: &Book) -> Vec<u32> {
    let mut rows = b.0.clone();
    rows.sort_by_key(|s| s.display_order);
    rows.iter().map(|s| s.id).collect()
}

/// The panel's own startup against the shipped tree: the spell page off the live gameplay
/// root, then its list.
pub fn bound_panel(c: &mut HeadlessClient) -> SpellbookPanel {
    let (ui, screen) = examine::parts(c);
    let root = screen.root().expect("the gameplay screen's root");
    let page = ui
        .get_child_recursive(root, SPELL_PAGE)
        .expect("the spell page");
    let mut p = SpellbookPanel::default();
    p.post_init(ui, page);
    assert!(
        p.list.is_some(),
        "the premise: the shipped tree carries the spell list"
    );
    p
}

/// One pass of the panel's own per-frame update, and whether it rebuilt.
pub fn update(c: &mut HeadlessClient, p: &mut SpellbookPanel, v: &Book) -> bool {
    let ui = examine::parts(c).0;
    p.update(ui, v)
}

/// The spells the rows hold, in row order -- read off the elements and not off the panel's
/// own mirror.
pub fn slot_order(p: &SpellbookPanel) -> Vec<u32> {
    p.list
        .as_ref()
        .expect("the spell list")
        .slots
        .iter()
        .filter_map(|s| s.spell)
        .collect()
}

/// The badge the row holding `spell` is drawing over its picture.
pub fn badge(c: &HeadlessClient, p: &SpellbookPanel, spell: u32) -> Option<DataId> {
    let ui = &c.view().expect_app().ui().expect("the UI shell").ui;
    let s = p
        .list
        .as_ref()?
        .slots
        .iter()
        .find(|s| s.spell == Some(spell))?;
    match s.icon_recipe(ui)? {
        dereth_ui::region::IconRecipe::Spell { overlay, .. } => overlay,
        dereth_ui::region::IconRecipe::Object { .. } => None,
    }
}
