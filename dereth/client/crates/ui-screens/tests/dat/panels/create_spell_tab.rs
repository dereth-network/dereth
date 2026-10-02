//! The magic window's Create Spell tab, built in code on the shipped magic window: absent on a
//! world without spell research, a third tab sharing the strip on one with it; components double
//! clicked or dropped onto the formula are what Test sends, in the order laid, and Clear empties
//! the formula.
//! Fixture: shipped layouts loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;

use dereth_primitives::{DataId, EraFeatureOverrides, EraId, ObjectId};
use dereth_ui::msg::element::id as msgid;
use dereth_ui::{Delivery, ElemHandle, ListenerId, Screen, UiSystem};
use dereth_ui_screens::panels::remaining::SPELL_PAGE;
use dereth_ui_screens::panels::research::{self, ResearchPanel};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use dereth_ui_screens::view::{ComponentCategory, ComponentRow, EraView, GameView, UiRequest};

const LISTENER: u32 = 0x7F0;

#[derive(Debug)]
struct World {
    era: EraView,
    components: Vec<ComponentRow>,
}

impl GameView for World {
    fn era(&self) -> Option<&EraView> {
        Some(&self.era)
    }
    fn spell_components(&self) -> Vec<ComponentCategory> {
        vec![ComponentCategory {
            category: 0,
            rows: self.components.clone(),
        }]
    }
    fn object_is_owned_component(&self, obj: ObjectId) -> Option<u32> {
        self.components
            .iter()
            .find(|r| r.object == Some(obj) && r.owned > 0)
            .map(|r| r.wcid)
    }
}

/// An end-of-retail world, with spell research turned on or not. It carries ten kinds of
/// component and one it has run out of.
fn world(research: bool) -> World {
    let mut era = EraView {
        era: EraId::Eor,
        era_announced: true,
        ..EraView::default()
    };
    if research {
        era.announced_features = EraFeatureOverrides::parse("spell_research=true")
            .expect("parses")
            .0;
    }
    let components = (0..11u32)
        .map(|i| ComponentRow {
            wcid: 0x2B0 + i,
            name: format!("Component {i}"),
            icon: Some(DataId(0x0600_1000 + i)),
            owned: if i == 3 { 0 } else { 5 },
            desired: 0,
            object: Some(ObjectId(0x5000_0000 + i)),
        })
        .collect();
    World { era, components }
}

fn screen() -> (UiSystem, GamePlayScreen) {
    let (mut ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds from the shipped layout");
    let root = s.roots()[0];
    ui.register_for_element_messages(root, ListenerId::External(LISTENER));
    (ui, s)
}

/// Deliver the outbox: the screen's messages to the screen and this test's to the page.
fn pump(ui: &mut UiSystem, s: &mut GamePlayScreen, page: &mut ResearchPanel, view: &World) {
    for _ in 0..16 {
        let batch = ui.drain_outbox();
        if batch.is_empty() {
            break;
        }
        for d in batch {
            if let Delivery::Element { to, msg } = d {
                if to == ListenerId::External(LISTENER) {
                    page.on_element_message(ui, &msg, view);
                } else {
                    s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg);
                }
            }
        }
    }
}

/// The magic window, bound, with the page built on it.
fn magic_window(ui: &mut UiSystem, s: &GamePlayScreen) -> (ElemHandle, ResearchPanel) {
    let root = s.roots()[0];
    let window = ui
        .get_child_recursive(root, SPELL_PAGE)
        .expect("the magic window");
    let mut page = ResearchPanel::default();
    page.post_init(ui, window);
    assert!(page.bound(), "the page is built on the shipped window");
    (window, page)
}

fn open_tab(ui: &UiSystem, window: ElemHandle) -> Option<dereth_ui::ElementId> {
    ui.node(window)?
        .behaviour
        .as_ref()?
        .as_any()?
        .downcast_ref::<dereth_ui::widgets::panel::Panel>()?
        .open_tab
}

fn shown(ui: &UiSystem, h: ElemHandle) -> bool {
    ui.node(h).expect("alive").region.flags.visible
}

fn width(ui: &UiSystem, h: ElemHandle) -> i32 {
    ui.node(h).expect("alive").region.box_.width()
}

fn tabs(ui: &UiSystem, window: ElemHandle) -> [ElemHandle; 2] {
    research::SHIPPED_TABS.map(|t| ui.get_child_recursive(window, t).expect("a shipped tab"))
}

/// Open the magic window on the Create Spell page, as the toolbar and a click on the tab do.
fn open_page(ui: &mut UiSystem, s: &mut GamePlayScreen, page: &mut ResearchPanel, view: &World) {
    let magic = s
        .panels
        .pages
        .iter()
        .find(|p| p.element == SPELL_PAGE)
        .copied()
        .expect("the magic window is a page of the stack");
    s.recv_set_panel_visibility(ui, magic.panel_id, true);
    let tab = page.tab().expect("the tab");
    let b = ui.screen_box(tab);
    let (x, y) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
    assert_eq!(
        ui.hit_test_screen(x, y),
        Some(tab),
        "the tab is under the pointer"
    );
    ui.mouse_down(7, x, y);
    ui.mouse_up(7, x, y, false);
    pump(ui, s, page, view);
}

fn centre(ui: &UiSystem, h: ElemHandle) -> (i32, i32) {
    let b = ui.screen_box(h);
    ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
}

fn double_click(
    ui: &mut UiSystem,
    s: &mut GamePlayScreen,
    page: &mut ResearchPanel,
    view: &World,
    h: ElemHandle,
) {
    let (x, y) = centre(ui, h);
    assert_eq!(
        ui.hit_test_screen(x, y),
        Some(h),
        "the slot is under the pointer"
    );
    ui.mouse_down(10, x, y);
    ui.mouse_up(10, x, y, true);
    pump(ui, s, page, view);
}

fn click(
    ui: &mut UiSystem,
    s: &mut GamePlayScreen,
    page: &mut ResearchPanel,
    view: &World,
    h: ElemHandle,
) {
    let (x, y) = centre(ui, h);
    ui.mouse_down(7, x, y);
    ui.mouse_up(7, x, y, false);
    pump(ui, s, page, view);
}

fn disabled(ui: &UiSystem, h: ElemHandle) -> bool {
    ui.node(h)
        .expect("alive")
        .merged_properties()
        .get_bool(dereth_ui::props::attr::DISABLED)
        .unwrap_or(false)
}

/// Behaviour: presentation.era.the-retail-magic-window-has-a-create-spell-tab-with-spell-research
#[test]
fn the_create_spell_tab_is_absent_without_spell_research_and_shares_the_strip_with_it() {
    let (mut ui, mut s) = screen();
    let (window, mut page) = magic_window(&mut ui, &s);
    let tab = page.tab().expect("the tab");
    let [spells, components] = tabs(&ui, window);

    let plain = world(false);
    assert!(page.update(&mut ui, &plain));
    assert!(!shown(&ui, tab), "no tab without spell research");
    assert_eq!((width(&ui, spells), width(&ui, components)), (138, 138));

    let researching = world(true);
    assert!(page.update(&mut ui, &researching));
    assert!(shown(&ui, tab), "the tab with spell research");
    let caption = ui.text_element_mut(tab).map(|t| t.glyphs.inq_text(false));
    assert_eq!(caption.as_deref(), Some(research::TAB_TEXT));
    let strip: Vec<(i32, i32)> = [spells, components, tab]
        .iter()
        .map(|&h| {
            let b = ui.node(h).expect("alive").region.box_;
            (b.x0, b.width())
        })
        .collect();
    assert_eq!(
        strip,
        [(0, 92), (92, 92), (184, 92)],
        "three tabs share the strip"
    );

    open_page(&mut ui, &mut s, &mut page, &researching);
    assert_eq!(open_tab(&ui, window), Some(research::TAB));
    let p = page.page().expect("the page");
    assert!(shown(&ui, p), "the page opens with its tab");
    assert!(shown(&ui, window), "on the open magic window");
    assert_eq!(ui.screen_box(p).width(), 300);
    // Between the grid and the buttons the page's own backdrop takes the pointer, so nothing
    // behind the window shows or answers there.
    let b = ui.screen_box(p);
    let gap = ui.hit_test_screen(b.x0 + 150, b.y0 + 286);
    assert_eq!(
        gap.and_then(|h| ui.node(h)).map(|n| n.element_id()),
        Some(research::BACKDROP),
        "the backdrop is under the pointer in the gap"
    );

    // Spell research turned off with the page open: the window opens the Spells tab instead.
    assert!(page.update(&mut ui, &plain));
    pump(&mut ui, &mut s, &mut page, &plain);
    assert!(!shown(&ui, tab));
    assert!(!shown(&ui, p));
    assert_eq!(open_tab(&ui, window), Some(research::SHIPPED_TABS[0]));
    assert_eq!((width(&ui, spells), width(&ui, components)), (138, 138));
    // Applied once.
    assert!(!page.update(&mut ui, &plain));
}

/// Behaviour: presentation.era.the-retail-magic-window-has-a-create-spell-tab-with-spell-research
#[test]
fn components_laid_on_the_create_spell_page_are_what_test_sends_and_clear_empties_it() {
    let (mut ui, mut s) = screen();
    let (_window, mut page) = magic_window(&mut ui, &s);
    let view = world(true);
    page.update(&mut ui, &view);
    open_page(&mut ui, &mut s, &mut page, &view);
    let _ = ui.requests.take();
    let (test, clear) = page.buttons();
    let (test, clear) = (test.expect("Test"), clear.expect("Clear"));
    assert!(
        disabled(&ui, test) && disabled(&ui, clear),
        "both wait for a component"
    );

    // The grid holds the ten carried kinds: the one run out of is left out.
    let grid = page.grid_slots().to_vec();
    assert_eq!(grid.len(), research::GRID_SLOTS);
    let image = |ui: &UiSystem, h: ElemHandle| {
        ui.node(h)
            .and_then(|n| n.region.image.as_ref())
            .map(|g| g.did)
    };
    assert_eq!(image(&ui, grid[3]), Some(DataId(0x0600_1004)));

    // Two double clicks and a drop from the pack, laid in that order.
    double_click(&mut ui, &mut s, &mut page, &view, grid[4]);
    double_click(&mut ui, &mut s, &mut page, &view, grid[0]);
    let formula = page.formula_slots().to_vec();
    let catcher = ui.drag_and_drop_catcher(formula[2]);
    assert_eq!(
        catcher.and_then(|h| ui.node(h)).map(|n| n.element_id()),
        Some(research::FORMULA),
        "the formula catches a dragged item"
    );
    assert!(page.accept_drop(&mut ui, ObjectId(0x5000_0009), &view));
    assert!(
        !page.accept_drop(&mut ui, ObjectId(0x5000_0003), &view),
        "a component not carried is not laid"
    );
    assert_eq!(page.formula.components(), [0x2B5, 0x2B0, 0x2B9]);
    assert_eq!(image(&ui, formula[0]), Some(DataId(0x0600_1005)));
    assert_eq!(image(&ui, formula[2]), Some(DataId(0x0600_1009)));
    assert!(!disabled(&ui, test) && !disabled(&ui, clear));

    click(&mut ui, &mut s, &mut page, &view, test);
    assert_eq!(
        ui.requests.take(),
        [UiRequest::TestSpellFormula {
            components: vec![0x2B5, 0x2B0, 0x2B9],
        }]
    );

    // A double click on a laid component takes it out; the rest close up.
    double_click(&mut ui, &mut s, &mut page, &view, formula[0]);
    assert_eq!(page.formula.components(), [0x2B0, 0x2B9]);

    click(&mut ui, &mut s, &mut page, &view, clear);
    assert!(page.formula.is_empty());
    assert!(disabled(&ui, test), "Test waits for a component again");
    let empty = image(&ui, formula[0]);
    assert!(formula.iter().all(|&h| image(&ui, h) == empty));
    click(&mut ui, &mut s, &mut page, &view, test);
    assert!(
        !ui.requests
            .take()
            .iter()
            .any(|r| matches!(r, UiRequest::TestSpellFormula { .. })),
        "nothing to test"
    );
}

/// Behaviour: presentation.era.the-retail-magic-window-has-a-create-spell-tab-with-spell-research
#[test]
fn the_create_spell_grid_scrolls_by_rows_when_more_kinds_are_carried_than_it_shows() {
    let (mut ui, mut s) = screen();
    let (_window, mut page) = magic_window(&mut ui, &s);
    let mut view = world(true);
    view.components = (0..60u32)
        .map(|i| ComponentRow {
            wcid: 0x300 + i,
            icon: Some(DataId(0x0600_2000 + i)),
            owned: 1,
            object: Some(ObjectId(0x5100_0000 + i)),
            ..ComponentRow::default()
        })
        .collect();
    page.update(&mut ui, &view);
    open_page(&mut ui, &mut s, &mut page, &view);
    let bar = page.scrollbar().expect("the scrollbar");
    let stops = ui
        .node(bar)
        .expect("alive")
        .merged_properties()
        .get_int(dereth_ui::widgets::scrollbar::attr::STOP_COUNT);
    // Sixty kinds in rows of eight: eight rows, six showing, three positions.
    assert_eq!(stops, Some(3));
    // The scrollbar set to its middle stop moves the grid down a row.
    ui.broadcast_element_message(bar, msgid::SCROLL_POSITION, 500, 1);
    pump(&mut ui, &mut s, &mut page, &view);
    assert_eq!(page.scroll, 1);
    let first = page.grid_slots()[0];
    double_click(&mut ui, &mut s, &mut page, &view, first);
    assert_eq!(
        page.formula.components(),
        [0x308],
        "the first slot shows the second row"
    );
}
