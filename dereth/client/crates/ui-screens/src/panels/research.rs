//! The magic window's Create Spell tab: spell research on the end-of-retail screens.
//!
//! The early clients researched spells on a page of the magic window: up to eight carried
//! components laid into a formula, then tested on the selected target in magic mode. The shipped
//! layouts are the end-of-retail ones and have no such page, so on a world with spell research
//! ([`dereth_primitives::EraFeatures::spell_research`]) this module builds it in code, beside the
//! Spells and Components tabs, out of the window's own pieces:
//!
//! | piece | made from |
//! |---|---|
//! | the Create Spell tab | a copy of the Components tab; the three tabs share the strip |
//! | the two headings | a copy of the spellbook's own heading text |
//! | the formula and the components grid | 32-pixel slots showing the component icons, each over the empty item slot's frame |
//! | the grid's scrollbar | a copy of the component list's scrollbar, one stop per row |
//! | Test and Clear | copies of the spellbook's text button |
//!
//! The page works as the classic interface's does: a double click on a carried component lays it
//! at the formula's end, a component dragged from the pack onto the formula is laid the same way,
//! a double click on a laid component takes it out, Test sends the formula as
//! [`UiRequest::TestSpellFormula`] and Clear empties it. A confirmed successful test clears its
//! matching formula. Test and Clear wait for a component.
//! What the test teaches reaches the spellbook as any learned spell does; the page is told
//! nothing.
//!
//! A world without spell research has no Create Spell tab (a window left open on it moves to
//! the Spells tab), and the other two tabs take back the whole strip.

use dereth_client_contract::research::{Formula, FORMULA_SLOTS};
use dereth_primitives::{DataId, ObjectId};
use dereth_ui::msg::element::id as msgid;
use dereth_ui::widgets::panel::Panel;
use dereth_ui::{ElemHandle, ElementDesc, ElementId, LayoutDesc, StateDesc, UiSystem};

use crate::view::{ComponentRow, GameView, UiRequest};

/// The magic window's two shipped tabs, Spells and Components, in strip order.
pub const SHIPPED_TABS: [ElementId; 2] = [ElementId(0x1000_02A9), ElementId(0x1000_02AA)];
/// The spellbook's heading text the page's headings copy.
const HEADING_SOURCE: ElementId = ElementId(0x1000_02A3);
/// The spellbook's text button Test and Clear copy.
const BUTTON_SOURCE: ElementId = ElementId(0x1000_02A5);
/// The spellbook's filter area, whose opaque backdrop the page copies under itself.
const BACKDROP_SOURCE: ElementId = ElementId(0x1000_0297);
/// The component list's scrollbar the grid's copies.
const SCROLLBAR_SOURCE: ElementId = ElementId(0x1000_0465);

/// The page's element ids, from a range no shipped layout uses.
/// The page's backdrop, under everything on it.
pub const BACKDROP: ElementId = ElementId(0x7F00_0000);
pub const TAB: ElementId = ElementId(0x7F00_0001);
pub const PAGE: ElementId = ElementId(0x7F00_0002);
pub const FORMULA_HEADING: ElementId = ElementId(0x7F00_0003);
pub const COMPONENTS_HEADING: ElementId = ElementId(0x7F00_0004);
pub const TEST_BUTTON: ElementId = ElementId(0x7F00_0005);
pub const CLEAR_BUTTON: ElementId = ElementId(0x7F00_0006);
pub const SCROLLBAR: ElementId = ElementId(0x7F00_0007);
/// The formula row: the drop target for a component dragged from the pack.
pub const FORMULA: ElementId = ElementId(0x7F00_0008);
/// The components grid.
pub const GRID: ElementId = ElementId(0x7F00_0009);
/// The first formula slot; the eight are consecutive.
pub const FIRST_FORMULA_SLOT: u32 = 0x7F00_0010;
/// The first grid slot; the [`GRID_SLOTS`] are consecutive.
pub const FIRST_GRID_SLOT: u32 = 0x7F00_0020;
/// The first slot frame: the empty item slot drawn under each slot, the formula's eight then the
/// grid's, consecutive.
pub const FIRST_SLOT_FRAME: u32 = 0x7F00_0100;

/// The tab's caption.
pub const TAB_TEXT: &str = "Create Spell";

/// The components grid: eight columns of 32-pixel slots, six rows showing.
pub const COLUMNS: usize = 8;
pub const ROWS_SHOWN: usize = 6;
pub const GRID_SLOTS: usize = COLUMNS * ROWS_SHOWN;
const SLOT: i32 = 32;

/// The tab strip's width, left of the window's close button.
const STRIP_WIDTH: i32 = 276;
/// The width of a tab's two end caps.
const TAB_CAP: i32 = 17;

/// The page's geometry, in the page's own coordinates. The page sits where the window's other
/// pages do, under the tabs.
const PAGE_BOX: (i32, i32, i32, i32) = (0, 25, 300, 337);
const FORMULA_HEADING_BOX: (i32, i32, i32, i32) = (0, 4, 300, 19);
const FORMULA_BOX: (i32, i32) = (22, 26);
const COMPONENTS_HEADING_BOX: (i32, i32, i32, i32) = (0, 64, 300, 19);
const GRID_BOX: (i32, i32) = (12, 86);
const SCROLLBAR_BOX: (i32, i32, i32, i32) = (272, 86, 16, 192);
const TEST_AT: (i32, i32) = (30, 294);
const CLEAR_AT: (i32, i32) = (170, 294);

/// The empty item slot's frame.
const EMPTY_SLOT: DataId = DataId(0x0600_4D20);

/// The item slot's drag hint, each formula slot's child.
const DRAG_HINT: ElementId = ElementId(crate::items::widget::child::DRAG_ACCEPT);

/// The text element's caption and horizontal justification attributes; 1 centres.
const ATTR_TEXT: u32 = 0x17;
const ATTR_H_JUSTIFY: u32 = 0x14;

/// The double click's mouse action, and the primary click's.
const DOUBLE_CLICK: u32 = 10;
const PRIMARY_CLICK: u32 = 7;

/// What the page last drew, so an unchanged frame writes nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Drawn {
    formula: Vec<u32>,
    carried: Vec<(u32, Option<DataId>, String)>,
    scroll: usize,
}

/// The Create Spell tab and its page, bound to a live magic window.
#[derive(Debug, Default)]
pub struct ResearchPanel {
    /// The formula being laid.
    pub formula: Formula,
    success_seen: u64,
    /// The magic window: the tabbed panel the tab is registered with.
    window: Option<ElemHandle>,
    /// The three tabs, in strip order; the last is Create Spell.
    tabs: Vec<ElemHandle>,
    page: Option<ElemHandle>,
    formula_slots: Vec<ElemHandle>,
    grid_slots: Vec<ElemHandle>,
    scrollbar: Option<ElemHandle>,
    /// Each formula slot's drag hint, in slot order: the item slot's own.
    hints: Vec<ElemHandle>,
    /// The formula slot whose hint is up, while a component is dragged over the formula.
    hinted: Option<usize>,
    test: Option<ElemHandle>,
    clear: Option<ElemHandle>,
    /// How far the grid is scrolled, in rows.
    pub scroll: usize,
    /// The carried components, as last drawn.
    carried: Vec<ComponentRow>,
    last: Option<Drawn>,
    /// Whether the tab was last shown; `None` before the first frame.
    shown: Option<bool>,
}

/// A count of slots or tabs, which is small, as a coordinate factor.
fn small(n: usize) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}

/// The id of slot `i` of a run starting at `first`.
fn slot_id(first: u32, i: usize) -> ElementId {
    ElementId(first.saturating_add(u32::try_from(i).unwrap_or(u32::MAX)))
}

/// The carried components, in the component tracker's order: one slot per kind of component.
fn carried(view: &dyn GameView) -> Vec<ComponentRow> {
    view.spell_components()
        .into_iter()
        .flat_map(|c| c.rows)
        .filter(|r| r.owned > 0)
        .collect()
}

fn window_panel(ui: &UiSystem, window: ElemHandle) -> Option<&Panel> {
    ui.node(window)?
        .behaviour
        .as_ref()?
        .as_any()?
        .downcast_ref::<Panel>()
}

fn window_panel_mut(ui: &mut UiSystem, window: ElemHandle) -> Option<&mut Panel> {
    ui.node_mut(window)?
        .behaviour
        .as_mut()?
        .as_any_mut()?
        .downcast_mut::<Panel>()
}

/// A live element's description with its live children's own, so the copy needs nothing
/// resolved from the data files.
fn full_desc(ui: &UiSystem, h: ElemHandle) -> Option<ElementDesc> {
    let mut d = ui.node(h)?.desc.clone();
    d.children.clear();
    for c in ui.children(h) {
        let cd = full_desc(ui, c)?;
        d.children.insert(cd.element_id, cd);
    }
    Some(d)
}

/// Put `d` at `(x, y)`, sized `w` by `h`, as its own design position.
fn place_desc(d: &mut ElementDesc, (x, y, w, h): (i32, i32, i32, i32)) {
    d.base.incorporation |= dereth_ui::desc::incorporation::LEGACY_ALL_GEOMETRY;
    d.base.x = x;
    d.base.y = y;
    d.base.width = w;
    d.base.height = h;
}

/// A plain element of the page.
fn field(id: ElementId, geometry: (i32, i32, i32, i32)) -> ElementDesc {
    let mut d = ElementDesc {
        base: StateDesc::default(),
        element_id: id,
        ty: dereth_ui::factory::ty::FIELD,
        ..ElementDesc::default()
    };
    place_desc(&mut d, geometry);
    d
}

/// Move and size `h`, and make that its design position, so a later move of the window
/// re-anchors it where it now is.
fn place(ui: &mut UiSystem, h: ElemHandle, (x, y, w, hgt): (i32, i32, i32, i32)) {
    ui.move_to(h, x, y);
    ui.resize_to(h, w, hgt);
    settle(ui, h);
}

/// Make every element's present box its design position, down the subtree.
fn settle(ui: &mut UiSystem, h: ElemHandle) {
    if let Some(n) = ui.node_mut(h) {
        n.default_location = n.region.box_;
    }
    for c in ui.children(h) {
        settle(ui, c);
    }
}

/// No data files: every description handed to the builder here is already whole.
#[derive(Debug)]
struct NoAssets;
impl dereth_primitives::AssetSource for NoAssets {
    fn read(&self, id: DataId) -> Result<Vec<u8>, dereth_primitives::AssetError> {
        Err(dereth_primitives::AssetError::NotFound(id))
    }
    fn exists(&self, _: DataId) -> bool {
        false
    }
    fn iter_type(&self, _: dereth_primitives::DataType) -> Box<dyn Iterator<Item = DataId> + '_> {
        Box::new(std::iter::empty())
    }
}

impl ResearchPanel {
    /// Build the tab and its page on the magic window `window` (the spell page), or bind the
    /// ones already built there. Both start hidden; [`Self::update`] shows them on a world with
    /// spell research.
    pub fn post_init(&mut self, ui: &mut UiSystem, window: ElemHandle) {
        *self = Self {
            formula: std::mem::take(&mut self.formula),
            ..Self::default()
        };
        if window_panel(ui, window).is_none() {
            return;
        }
        if ui.get_child_recursive(window, PAGE).is_none() && !Self::build(ui, window) {
            return;
        }
        let find = |ui: &UiSystem, id| ui.get_child_recursive(window, id);
        let mut tabs: Vec<ElemHandle> = SHIPPED_TABS.iter().filter_map(|&t| find(ui, t)).collect();
        tabs.extend(find(ui, TAB));
        if tabs.len() != 3 {
            return;
        }
        self.window = Some(window);
        self.tabs = tabs;
        self.page = find(ui, PAGE);
        self.formula_slots = (0..FORMULA_SLOTS)
            .filter_map(|i| find(ui, slot_id(FIRST_FORMULA_SLOT, i)))
            .collect();
        self.grid_slots = (0..GRID_SLOTS)
            .filter_map(|i| find(ui, slot_id(FIRST_GRID_SLOT, i)))
            .collect();
        self.hints = self
            .formula_slots
            .iter()
            .filter_map(|&s| ui.get_child_recursive(s, DRAG_HINT))
            .collect();
        self.scrollbar = find(ui, SCROLLBAR);
        self.test = find(ui, TEST_BUTTON);
        self.clear = find(ui, CLEAR_BUTTON);
    }

    /// Whether the tab and its page are bound.
    #[must_use]
    pub fn bound(&self) -> bool {
        self.page.is_some()
    }

    /// The Create Spell tab.
    #[must_use]
    pub fn tab(&self) -> Option<ElemHandle> {
        self.tabs.get(2).copied()
    }

    /// The page.
    #[must_use]
    pub fn page(&self) -> Option<ElemHandle> {
        self.page
    }

    /// The formula's eight slots, in order.
    #[must_use]
    pub fn formula_slots(&self) -> &[ElemHandle] {
        &self.formula_slots
    }

    /// The grid's slots, in order.
    #[must_use]
    pub fn grid_slots(&self) -> &[ElemHandle] {
        &self.grid_slots
    }

    /// The Test and Clear buttons.
    #[must_use]
    pub fn buttons(&self) -> (Option<ElemHandle>, Option<ElemHandle>) {
        (self.test, self.clear)
    }

    /// The grid's scrollbar.
    #[must_use]
    pub fn scrollbar(&self) -> Option<ElemHandle> {
        self.scrollbar
    }

    /// Build the tab and the page. False when the window lacks a piece they are made from.
    fn build(ui: &mut UiSystem, window: ElemHandle) -> bool {
        let find = |ui: &UiSystem, id| ui.get_child_recursive(window, id);
        let (Some(source_tab), Some(heading), Some(button), Some(bar), Some(backdrop)) = (
            find(ui, SHIPPED_TABS[1]),
            find(ui, HEADING_SOURCE),
            find(ui, BUTTON_SOURCE),
            find(ui, SCROLLBAR_SOURCE),
            find(ui, BACKDROP_SOURCE),
        ) else {
            return false;
        };
        let Some(layout_did) = ui.node(window).map(|n| n.layout_did) else {
            return false;
        };
        let layout = LayoutDesc {
            did: layout_did,
            display_width: 800,
            display_height: 600,
            ..LayoutDesc::default()
        };
        let copy = |ui: &UiSystem, from: ElemHandle, id: ElementId| {
            full_desc(ui, from).map(|mut d| {
                d.element_id = id;
                d
            })
        };

        // The tab: the Components tab's copy, captioned in code.
        let Some(mut tab) = copy(ui, source_tab, TAB) else {
            return false;
        };
        tab.base.properties.remove(ATTR_TEXT);
        // The page.
        let mut page = field(PAGE, PAGE_BOX);
        let heading_desc = |ui: &UiSystem, id, geometry| {
            copy(ui, heading, id).map(|mut d| {
                d.states.clear();
                d.base.properties.remove(ATTR_TEXT);
                d.base
                    .properties
                    .set(ATTR_H_JUSTIFY, dereth_ui::PropertyValue::Enum(1));
                place_desc(&mut d, geometry);
                d
            })
        };
        let button_desc = |ui: &UiSystem, id, (x, y): (i32, i32)| {
            copy(ui, button, id).map(|mut d| {
                for c in d.children.values_mut() {
                    c.base.properties.remove(ATTR_TEXT);
                }
                let (w, h) = (d.base.width, d.base.height);
                place_desc(&mut d, (x, y, w, h));
                d
            })
        };
        let mut formula = field(FORMULA, (FORMULA_BOX.0, FORMULA_BOX.1, SLOT * 8, SLOT));
        for i in 0..FORMULA_SLOTS {
            let geometry = (small(i) * SLOT, 0, SLOT, SLOT);
            let frame = field(slot_id(FIRST_SLOT_FRAME, i), geometry);
            formula.children.insert(frame.element_id, frame);
            let d = field(slot_id(FIRST_FORMULA_SLOT, i), geometry);
            formula.children.insert(d.element_id, d);
        }
        let mut grid = field(
            GRID,
            (
                GRID_BOX.0,
                GRID_BOX.1,
                SLOT * small(COLUMNS),
                SLOT * small(ROWS_SHOWN),
            ),
        );
        for i in 0..GRID_SLOTS {
            let (col, row) = (small(i % COLUMNS), small(i / COLUMNS));
            let geometry = (col * SLOT, row * SLOT, SLOT, SLOT);
            let frame = field(slot_id(FIRST_SLOT_FRAME, FORMULA_SLOTS + i), geometry);
            grid.children.insert(frame.element_id, frame);
            let d = field(slot_id(FIRST_GRID_SLOT, i), geometry);
            grid.children.insert(d.element_id, d);
        }
        // The window's own background is translucent; the backdrop covers the whole page so what
        // is behind the window neither shows nor takes the mouse through it.
        let backdrop_desc = copy(ui, backdrop, BACKDROP).map(|mut d| {
            d.children.clear();
            place_desc(&mut d, (0, 0, PAGE_BOX.2, PAGE_BOX.3));
            d
        });
        let parts = [
            backdrop_desc,
            heading_desc(ui, FORMULA_HEADING, FORMULA_HEADING_BOX),
            heading_desc(ui, COMPONENTS_HEADING, COMPONENTS_HEADING_BOX),
            button_desc(ui, TEST_BUTTON, TEST_AT),
            button_desc(ui, CLEAR_BUTTON, CLEAR_AT),
            copy(ui, bar, SCROLLBAR),
            Some(formula),
            Some(grid),
        ];
        for d in parts {
            let Some(d) = d else { return false };
            page.children.insert(d.element_id, d);
        }

        let mut built = Vec::new();
        for d in [tab, page] {
            let Ok(Some(h)) = ui.create_element_recursive_from_full_desc(&NoAssets, &layout, &d)
            else {
                return false;
            };
            // Hidden before the window knows it, so its visibility reaches no tab table.
            ui.set_visible(h, false);
            ui.set_parent(h, Some(window));
            ui.initialize_tree(h);
            built.push(h);
        }
        let (tab, page) = (built[0], built[1]);
        if let Some(t) = ui.text_element_mut(tab) {
            t.set_text(TAB_TEXT);
        }
        ui.set_state(tab, dereth_ui::widgets::panel::tab_state::CLOSED);
        for (id, text) in [
            (FORMULA_HEADING, "Formula"),
            (COMPONENTS_HEADING, "Components"),
        ] {
            if let Some(t) = find(ui, id).and_then(|h| ui.text_element_mut(h)) {
                t.set_text(text);
            }
        }
        for (id, text) in [(TEST_BUTTON, "Test"), (CLEAR_BUTTON, "Clear")] {
            let Some(b) = find(ui, id) else { continue };
            for c in ui.children(b) {
                if let Some(t) = ui.text_element_mut(c) {
                    t.set_text(text);
                }
            }
        }
        // The scrollbar's copy is the component list's full height; it is cut to the grid's.
        if let Some(bar) = find(ui, SCROLLBAR) {
            place(ui, bar, SCROLLBAR_BOX);
        }
        settle(ui, page);
        // Every slot stands on the empty item slot's frame, which shows through around a
        // component's icon and is the whole of an empty slot; the slots are in front of it.
        for i in 0..FORMULA_SLOTS + GRID_SLOTS {
            if let Some(n) = find(ui, slot_id(FIRST_SLOT_FRAME, i)).and_then(|h| ui.node_mut(h)) {
                n.region.image = Some(dereth_ui::GraphicRef::opaque_surface(EMPTY_SLOT, 0, 0));
            }
        }
        let slots = (0..FORMULA_SLOTS)
            .map(|i| slot_id(FIRST_FORMULA_SLOT, i))
            .chain((0..GRID_SLOTS).map(|i| slot_id(FIRST_GRID_SLOT, i)));
        for id in slots {
            if let Some(h) = find(ui, id) {
                ui.bring_to_front(h);
            }
        }
        // The slots answer the mouse; the formula row catches a dragged component.
        let slots = (0..FORMULA_SLOTS)
            .map(|i| slot_id(FIRST_FORMULA_SLOT, i))
            .chain((0..GRID_SLOTS).map(|i| slot_id(FIRST_GRID_SLOT, i)));
        for id in slots {
            if let Some(h) = find(ui, id) {
                ui.set_mouse_visible(h, true);
            }
        }
        if let Some(h) = find(ui, BACKDROP) {
            ui.set_mouse_visible(h, true);
        }
        // Each formula slot carries the item slot's drag hint, shown while a component is dragged
        // over the formula. Without the interface's layouts there is no hint, and the drop still
        // works.
        for i in 0..FORMULA_SLOTS {
            let Some(slot) = find(ui, slot_id(FIRST_FORMULA_SLOT, i)) else {
                continue;
            };
            let built = ui.require_env().and_then(|e| {
                e.create_nested_child_element_by_enum(
                    ui,
                    slot,
                    crate::items::widget::ITEM_SLOT_LAYOUT,
                    DRAG_HINT,
                )
            });
            if let Ok(h) = built {
                ui.move_to(h, 0, 0);
                ui.resize_to(h, SLOT, SLOT);
                ui.set_mouse_visible(h, false);
                ui.set_state(h, crate::items::widget::drag_accept_state::NONE);
            }
        }
        if let Some(n) = find(ui, FORMULA).and_then(|h| ui.node_mut(h)) {
            n.drop_catcher = true;
        }
        if let Some(p) = window_panel_mut(ui, window) {
            p.add_tab(TAB, PAGE);
        }
        true
    }

    /// Show the tab on a world with spell research and hide it on one without, then redraw the
    /// page when what it shows has changed. Returns whether anything was written.
    pub fn update(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        let Some(window) = self.window else {
            return false;
        };
        if let Some(success) = view.research_success() {
            if self.success_seen != success.serial {
                self.success_seen = success.serial;
                if self.formula.components() == success.components {
                    self.formula.clear();
                }
            }
        }
        let has = view.era_features().spell_research;
        let mut wrote = false;
        if self.shown != Some(has) {
            self.shown = Some(has);
            self.show_tab(ui, window, has);
            wrote = true;
        }
        self.carried = carried(view);
        let max = self.max_scroll();
        self.scroll = self.scroll.min(max);
        let drawn = Drawn {
            formula: self.formula.components().to_vec(),
            carried: self
                .carried
                .iter()
                .map(|r| (r.wcid, r.icon, r.name.clone()))
                .collect(),
            scroll: self.scroll,
        };
        if self.last.as_ref() != Some(&drawn) {
            self.draw(ui, &drawn);
            self.last = Some(drawn);
            wrote = true;
        }
        wrote
    }

    fn max_scroll(&self) -> usize {
        self.carried
            .len()
            .div_ceil(COLUMNS)
            .saturating_sub(ROWS_SHOWN)
    }

    /// Show or hide the tab; the shown tabs share the strip. A window open on a tab being hidden
    /// opens its first tab instead, as a click on it would.
    fn show_tab(&mut self, ui: &mut UiSystem, window: ElemHandle, has: bool) {
        let tab = self.tabs[2];
        ui.set_visible(tab, has);
        ui.set_mouse_visible(tab, has);
        let shown: Vec<ElemHandle> = if has {
            self.tabs.clone()
        } else {
            self.tabs[..2].to_vec()
        };
        let width = STRIP_WIDTH / small(shown.len());
        for (i, &t) in shown.iter().enumerate() {
            place(ui, t, (small(i) * width, 0, width, 25));
            // The two end caps keep their width and the middle takes the rest.
            let children = ui.children(t);
            for c in children {
                let Some(b) = ui.node(c).map(|n| n.default_location) else {
                    continue;
                };
                let geometry = if b.x0 == 0 {
                    (0, 0, TAB_CAP, b.height())
                } else if b.width() == TAB_CAP {
                    (width - TAB_CAP, 0, TAB_CAP, b.height())
                } else {
                    (TAB_CAP, 0, width - 2 * TAB_CAP, b.height())
                };
                place(ui, c, geometry);
            }
        }
        if has {
            return;
        }
        let open = window_panel(ui, window).and_then(|p| p.open_tab);
        if open == Some(TAB) {
            ui.broadcast_element_message(self.tabs[0], msgid::MOUSE_CLICK, 0, 0);
        }
    }

    fn draw(&mut self, ui: &mut UiSystem, drawn: &Drawn) {
        let icon_of = |wcid: u32| {
            self.carried
                .iter()
                .find(|r| r.wcid == wcid)
                .and_then(|r| r.icon)
        };
        for (i, &slot) in self.formula_slots.iter().enumerate() {
            // A component no longer carried keeps its place, drawn as an empty slot.
            let icon = drawn.formula.get(i).and_then(|&w| icon_of(w));
            Self::draw_slot(ui, slot, icon, None);
        }
        let first = drawn.scroll * COLUMNS;
        for (i, &slot) in self.grid_slots.iter().enumerate() {
            let row = self.carried.get(first + i);
            Self::draw_slot(
                ui,
                slot,
                row.and_then(|r| r.icon),
                row.map(|r| r.name.clone()),
            );
            // A slot holding a component is picked up and dragged; an empty one is not.
            if let Some(n) = ui.node_mut(slot) {
                n.flags.set_dragable(row.is_some());
            }
        }
        let any = !drawn.formula.is_empty();
        for b in [self.test, self.clear].into_iter().flatten() {
            ui.set_attribute_bool(b, dereth_ui::props::attr::DISABLED, !any);
        }
        if let Some(bar) = self.scrollbar {
            use dereth_ui::widgets::scrollbar::attr;
            let max = self.max_scroll();
            ui.set_attribute_bool(bar, attr::DISABLED, max == 0);
            ui.set_attribute_bool(bar, attr::HAS_STOP_LOCATIONS, true);
            ui.set_attribute_int(bar, attr::STOP_COUNT, i32::try_from(max + 1).unwrap_or(1));
            ui.set_attribute_int(bar, attr::STOP, i32::try_from(drawn.scroll).unwrap_or(0));
            let pos = if max == 0 {
                0.0
            } else {
                drawn.scroll as f32 / max as f32
            };
            ui.set_attribute_float(bar, attr::POSITION, pos);
            dereth_ui::widgets::scrollbar::update_layout_of(ui, bar);
        }
    }

    fn draw_slot(ui: &mut UiSystem, slot: ElemHandle, icon: Option<DataId>, tip: Option<String>) {
        if let Some(n) = ui.node_mut(slot) {
            // A component's icon is drawn as the Components tab draws it, over the slot's frame;
            // an empty slot shows the frame alone.
            n.region.image = icon.map(super::spellcomponent::component_icon);
        }
        ui.set_tooltip(slot, tip);
    }

    /// The carried component drawn in grid slot `index`.
    fn grid_row(&self, index: usize) -> Option<&ComponentRow> {
        self.carried.get(self.scroll * COLUMNS + index)
    }

    /// Lay a component at the formula's end. Returns whether it was laid.
    pub fn lay(&mut self, wcid: u32) -> bool {
        self.formula.add(wcid)
    }

    /// A component object dropped on the formula: laid when the player carries it.
    pub fn accept_drop(
        &mut self,
        ui: &mut UiSystem,
        object: ObjectId,
        view: &dyn GameView,
    ) -> bool {
        let laid = view
            .object_is_owned_component(object)
            .is_some_and(|wcid| self.lay(wcid));
        if laid {
            self.update(ui, view);
        }
        laid
    }

    /// The page's gestures: a press or double click on a slot, Test, Clear and the scrollbar.
    /// Returns whether the message was the page's.
    pub fn on_element_message(
        &mut self,
        ui: &mut UiSystem,
        m: &dereth_ui::ElementMessage,
        view: &dyn GameView,
    ) -> bool {
        if self.page.is_none() {
            return false;
        }
        let consumed = match m.id {
            msgid::MOUSE_PRESS => self.on_press(ui, m),
            msgid::DRAG_CURSOR_OVER if m.source_id == FORMULA => {
                self.drag_over(ui, m.p1 != 0, view);
                true
            }
            // The formula's copy of the drop, whose `p2` names the drag's owner: a component
            // dragged from the grid is laid. One dragged from the pack reaches
            // [`Self::accept_drop`] through the screen's drop handling instead.
            msgid::DROP_FAILED if m.source_id == FORMULA && m.p2 != 0 => {
                self.set_hint(ui, None, false);
                let owner = ElemHandle::from_raw(m.p2);
                if let Some(row) = self
                    .grid_slots
                    .iter()
                    .position(|&s| s == owner)
                    .and_then(|i| self.grid_row(i).cloned())
                {
                    self.lay(row.wcid);
                }
                true
            }
            msgid::BUTTON_CLICKED if m.source_id == TEST_BUTTON => {
                if !self.formula.is_empty() {
                    ui.requests.emit(UiRequest::TestSpellFormula {
                        components: self.formula.components().to_vec(),
                    });
                }
                true
            }
            msgid::BUTTON_CLICKED if m.source_id == CLEAR_BUTTON => {
                self.formula.clear();
                true
            }
            msgid::SCROLL_POSITION if m.source_id == SCROLLBAR => {
                let max = self.max_scroll();
                self.scroll = if m.p2 == u32::MAX {
                    (m.p1 as usize * max + 500) / 1000
                } else {
                    m.p2 as usize
                }
                .min(max);
                true
            }
            _ => false,
        };
        if consumed {
            self.update(ui, view);
        }
        consumed
    }

    /// The component being dragged, when the drag is one: a slot of the grid, or a carried
    /// component picked up from the pack.
    fn dragged_component(&self, ui: &UiSystem, view: &dyn GameView) -> Option<u32> {
        let drag = ui.drag_state();
        let owner = drag.owner?;
        if let Some(i) = self.grid_slots.iter().position(|&s| s == owner) {
            return self.grid_row(i).map(|r| r.wcid);
        }
        let proxy = drag.element?;
        let info = crate::items::widget::inq_drop_icon_info(ui, proxy);
        view.object_is_owned_component(info.item?)
    }

    /// The drag hint over the formula: entering with a component puts the hint up on the slot it
    /// would be laid in, green while the formula has room and red when it is full; leaving takes
    /// it down. A drag that is not a component shows none.
    fn drag_over(&mut self, ui: &mut UiSystem, entered: bool, view: &dyn GameView) {
        if !entered || self.dragged_component(ui, view).is_none() {
            self.set_hint(ui, None, false);
            return;
        }
        let at = self.formula.components().len();
        let room = at < FORMULA_SLOTS;
        self.set_hint(ui, Some(at.min(FORMULA_SLOTS - 1)), room);
    }

    /// Put the drag hint up on formula slot `at` (accepting or refusing), or take it down.
    fn set_hint(&mut self, ui: &mut UiSystem, at: Option<usize>, accept: bool) {
        use crate::items::widget::drag_accept_state as hint;
        if let Some(old) = self.hinted.take().and_then(|i| self.hints.get(i).copied()) {
            ui.set_state(old, hint::NONE);
        }
        let Some(i) = at else { return };
        let Some(&h) = self.hints.get(i) else { return };
        ui.set_state(h, if accept { hint::ACCEPT } else { hint::REFUSE });
        self.hinted = Some(i);
    }

    /// The formula slot whose drag hint is up, and whether it accepts.
    #[must_use]
    pub fn hint(&self, ui: &UiSystem) -> Option<(usize, bool)> {
        let i = self.hinted?;
        let state = ui.node(*self.hints.get(i)?)?.state;
        Some((i, state == crate::items::widget::drag_accept_state::ACCEPT))
    }

    fn on_press(&mut self, ui: &mut UiSystem, m: &dereth_ui::ElementMessage) -> bool {
        let id = m.source_id.0;
        if let Some(i) = id
            .checked_sub(FIRST_FORMULA_SLOT)
            .map(|i| i as usize)
            .filter(|&i| i < FORMULA_SLOTS)
        {
            // A double click on a laid component takes it out.
            if m.p1 == DOUBLE_CLICK {
                self.formula.remove(i);
            }
            return true;
        }
        if let Some(i) = id
            .checked_sub(FIRST_GRID_SLOT)
            .map(|i| i as usize)
            .filter(|&i| i < GRID_SLOTS)
        {
            let Some(row) = self.grid_row(i).cloned() else {
                return true;
            };
            match m.p1 {
                // A double click lays it at the formula's end.
                DOUBLE_CLICK => {
                    self.lay(row.wcid);
                }
                // A click selects the carried component.
                PRIMARY_CLICK => {
                    if let Some(object) = row.object {
                        ui.requests.emit(UiRequest::Select(object));
                    }
                }
                _ => {}
            }
            return true;
        }
        false
    }
}
