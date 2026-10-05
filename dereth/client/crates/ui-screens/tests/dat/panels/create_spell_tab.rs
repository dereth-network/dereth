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
    success: Option<dereth_client_contract::research::ResearchSuccess>,
    learned: Option<(u64, u32)>,
}

impl GameView for World {
    fn research_success(&self) -> Option<dereth_client_contract::research::ResearchSuccess> {
        self.success.clone()
    }
    fn last_learned_spell(&self) -> Option<(u64, u32)> {
        self.learned
    }
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
    World {
        era,
        components,
        success: None,
        learned: None,
    }
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
    // Every slot stands in front of the empty item slot's frame: a carried component's icon is
    // drawn over it, and an empty slot is the frame alone.
    let window = page.page().expect("the page");
    for (i, &slot) in grid.iter().enumerate() {
        let frame = ui
            .get_child_recursive(
                window,
                dereth_ui::ElementId(research::FIRST_SLOT_FRAME + 8 + i as u32),
            )
            .expect("a frame behind the slot");
        assert_eq!(
            image(&ui, frame),
            Some(DataId(0x0600_4D20)),
            "the empty slot"
        );
        let parent = ui.parent(slot).expect("the grid");
        let order = ui.children(parent);
        let at = |h| order.iter().position(|&c| c == h);
        assert!(at(frame) < at(slot), "slot {i} is drawn over its frame");
    }
    assert_eq!(
        image(&ui, grid[research::GRID_SLOTS - 1]),
        None,
        "an empty slot is its frame"
    );

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

/// Behaviour: presentation.era.a-component-drags-from-the-create-spell-grid-onto-the-formula
#[test]
fn a_component_dragged_from_the_grid_onto_the_formula_shows_the_drag_hint_and_is_laid() {
    let (mut ui, mut s) = screen();
    let (_window, mut page) = magic_window(&mut ui, &s);
    let view = world(true);
    page.update(&mut ui, &view);
    open_page(&mut ui, &mut s, &mut page, &view);
    let grid = page.grid_slots().to_vec();
    let formula = page.formula_slots().to_vec();

    // A component's icon is drawn with its white outline turned black, as the Components tab's
    // rows draw it.
    let op = ui
        .node(grid[0])
        .and_then(|n| n.region.image.as_ref())
        .and_then(|g| g.op);
    assert_eq!(
        op,
        Some(dereth_ui::region::SurfaceOp::ReplaceColor {
            from: dereth_ui::region::SurfaceOp::OPAQUE_WHITE,
            to: dereth_ui::region::SurfaceOp::OPAQUE_BLACK,
        })
    );

    // Press on the third carried component and carry it over the formula.
    let (x, y) = centre(&ui, grid[2]);
    ui.mouse_down(7, x, y);
    pump(&mut ui, &mut s, &mut page, &view);
    let (fx, fy) = centre(&ui, formula[3]);
    let now = dereth_primitives::LocalTime(1.0);
    ui.mouse_move(now, x + 10, y);
    pump(&mut ui, &mut s, &mut page, &view);
    assert!(
        ui.drag_state().element.is_some(),
        "the component is picked up"
    );
    ui.mouse_move(now, fx, fy);
    pump(&mut ui, &mut s, &mut page, &view);
    assert_eq!(
        page.hint(&ui),
        Some((0, true)),
        "the hint shows where it will be laid: the formula's first place"
    );

    ui.mouse_up(7, fx, fy, false);
    pump(&mut ui, &mut s, &mut page, &view);
    assert_eq!(page.formula.components(), [0x2B2], "the component is laid");
    assert_eq!(page.hint(&ui), None, "the hint comes down with the drop");
    let laid = ui
        .node(formula[0])
        .and_then(|n| n.region.image.as_ref())
        .map(|g| g.did);
    assert_eq!(laid, Some(DataId(0x0600_1002)));

    // A drag that leaves the formula takes the hint down and lays nothing.
    let (x, y) = centre(&ui, grid[0]);
    ui.mouse_down(7, x, y);
    ui.mouse_move(now, x + 10, y);
    ui.mouse_move(now, fx, fy);
    pump(&mut ui, &mut s, &mut page, &view);
    assert_eq!(page.hint(&ui), Some((1, true)));
    ui.mouse_move(now, x, y + 200);
    pump(&mut ui, &mut s, &mut page, &view);
    assert_eq!(page.hint(&ui), None, "off the formula, no hint");
    ui.mouse_up(7, x, y + 200, false);
    pump(&mut ui, &mut s, &mut page, &view);
    assert_eq!(
        page.formula.components(),
        [0x2B2],
        "dropped elsewhere, nothing laid"
    );

    // An empty grid slot picks nothing up.
    let empty = grid[research::GRID_SLOTS - 1];
    let (x, y) = centre(&ui, empty);
    ui.mouse_down(7, x, y);
    ui.mouse_move(now, x + 10, y);
    pump(&mut ui, &mut s, &mut page, &view);
    assert!(ui.drag_state().element.is_none(), "nothing to drag");
    ui.mouse_up(7, x + 10, y, false);
    pump(&mut ui, &mut s, &mut page, &view);
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

/// Behaviour: magic.research.confirmed-success-clears-the-tested-formula
#[test]
fn successful_test_clears_only_its_matching_formula_and_redraws_the_buttons() {
    let (mut ui, mut screen) = screen();
    let (_, mut page) = magic_window(&mut ui, &screen);
    let mut view = world(true);
    page.update(&mut ui, &view);
    open_page(&mut ui, &mut screen, &mut page, &view);
    let grid = page.grid_slots().to_vec();
    double_click(&mut ui, &mut screen, &mut page, &view, grid[0]);
    assert_eq!(page.formula.components(), [0x2B0]);
    view.success = Some(dereth_client_contract::research::ResearchSuccess {
        serial: 1,
        components: vec![0x2B1],
    });
    page.update(&mut ui, &view);
    assert_eq!(
        page.formula.components(),
        [0x2B0],
        "another formula's completion"
    );
    view.success = Some(dereth_client_contract::research::ResearchSuccess {
        serial: 2,
        components: vec![0x2B0],
    });
    page.update(&mut ui, &view);
    assert!(page.formula.is_empty());
    let test = ui.get_element(research::TEST_BUTTON).unwrap();
    assert!(disabled(&ui, test));
    double_click(&mut ui, &mut screen, &mut page, &view, grid[0]);
    page.update(&mut ui, &view);
    assert_eq!(
        page.formula.components(),
        [0x2B0],
        "receipt is consumed only once"
    );
    assert!(!disabled(&ui, test));
}

/// Behaviour: presentation.era.a-spell-learned-on-the-create-spell-page-brings-up-the-spells-tab
#[test]
fn a_spell_learned_while_the_create_spell_page_is_open_brings_up_the_spells_tab() {
    let (mut ui, mut screen) = screen();
    let (window, mut page) = magic_window(&mut ui, &screen);
    let mut view = world(true);
    // A spell learned before the page was bound moves nothing.
    view.learned = Some((4, 1));
    page.update(&mut ui, &view);
    open_page(&mut ui, &mut screen, &mut page, &view);
    page.update(&mut ui, &view);
    assert_eq!(open_tab(&ui, window), Some(research::TAB));
    view.learned = Some((5, 157));
    page.update(&mut ui, &view);
    pump(&mut ui, &mut screen, &mut page, &view);
    assert_eq!(
        open_tab(&ui, window),
        Some(research::SHIPPED_TABS[0]),
        "the Spells tab is up"
    );
    // On the Spells tab another spell learned leaves the window as it is.
    view.learned = Some((6, 158));
    page.update(&mut ui, &view);
    pump(&mut ui, &mut screen, &mut page, &view);
    assert_eq!(open_tab(&ui, window), Some(research::SHIPPED_TABS[0]));
}
