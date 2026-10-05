//! The shipped gameplay tabs author two distinct state colours; every tab of every panel holds the
//! state its panel set; an open tab has its open-state colour; tree initialisation is children-
//! first.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;
use std::rc::Rc;

use crate::common::*;
use dereth_primitives::{AssetSource, DataId};
use dereth_ui::framework::LayoutEnum;
use dereth_ui::{ElemHandle, ElementId, PropertyValue, StateId, UiSystem};

/// `0x21000005`'s layout enum and root element — `screens::gameplay`'s own two constants.
const GAMEPLAY: LayoutEnum = LayoutEnum(0x1000_0006);
const GAMEPLAY_ROOT: ElementId = ElementId(0x1000_0495);

/// The panel's five property ids, from the retail `MasterProperty` table.
mod attr {
    /// The page table whose arrival initializes the tab pages.
    pub const PAGES: u32 = 0x2E;
    /// `page-data` — one row of it.
    pub const PAGE_DATA: u32 = 0x2F;
    /// `tab element id`.
    pub const TAB_ELEMENT: u32 = 0x30;
    /// `page element id`.
    pub const PAGE_ELEMENT: u32 = 0x31;
    /// `initial page visibility` — absent means false.
    pub const PAGE_OPEN: u32 = 0x32;
    /// `state-specific font colors`, the array a tab's two states differ in.
    pub const FONT_COLORS: u32 = 0x1B;
    /// The member of that array that carries the colour itself.
    pub const FONT_COLOR: u32 = 0x19;
}

/// The two state numbers a panel's update pass writes onto its tabs.
const TAB_CLOSED: StateId = StateId(0x0B);
const TAB_OPEN: StateId = StateId(0x0C);

/// A panel is element type 8.
const PANEL_TYPE: u32 = 8;

fn gameplay_root() -> (UiSystem, ElemHandle) {
    let (mut ui, _flow, store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    ui.assets = Some(Rc::clone(&store) as Rc<dyn dereth_ui::LayoutAssets>);

    let root =
        dereth_ui_screens::env::create_and_add_root_element(&mut ui, GAMEPLAY, GAMEPLAY_ROOT)
            .expect("classic_gameplay builds from the shipped layout");
    (ui, root)
}

/// One `page-data` row, read out of the shipped `0x2E` array.
#[derive(Debug, Clone, Copy)]
struct PageRow {
    tab: ElementId,
    open: bool,
}

/// The client's own read of the `0x2E` array, written out here rather than called,
/// including its duplicate-key refusal.
fn page_rows(v: &PropertyValue) -> Vec<PageRow> {
    let PropertyValue::Array(rows) = v else {
        return Vec::new();
    };
    let mut out: Vec<PageRow> = Vec::new();
    let mut pages: Vec<u32> = Vec::new();
    for row in rows {
        if row.id != attr::PAGE_DATA {
            continue;
        }
        let PropertyValue::Struct(members) = &row.value else {
            continue;
        };
        let member = |id: u32| {
            members
                .iter()
                .find(|(k, _)| *k == id)
                .map(|(_, p)| &p.value)
        };
        let (Some(PropertyValue::Enum(tab)), Some(PropertyValue::Enum(page))) =
            (member(attr::TAB_ELEMENT), member(attr::PAGE_ELEMENT))
        else {
            continue;
        };
        if out.iter().any(|r| r.tab.0 == *tab) || pages.contains(page) {
            continue;
        }
        pages.push(*page);
        out.push(PageRow {
            tab: ElementId(*tab),
            open: matches!(member(attr::PAGE_OPEN), Some(PropertyValue::Bool(true))),
        });
    }
    out
}

/// `state-specific font colors` as an element sees it in a given state — the value the text renderer
/// reads, which is the whole point of the tab having two states at all.
fn font_color_in_state(ui: &UiSystem, h: ElemHandle, s: StateId) -> Option<u32> {
    let n = ui.node(h)?;
    color_of(n.merged_properties_for(s).get(attr::FONT_COLORS)?)
}

fn live_font_color(ui: &UiSystem, h: ElemHandle) -> Option<u32> {
    let n = ui.node(h)?;
    color_of(n.merged_properties().get(attr::FONT_COLORS)?)
}

fn color_of(v: &PropertyValue) -> Option<u32> {
    let PropertyValue::Array(a) = v else {
        return None;
    };
    a.iter().find_map(|p| match (p.id, &p.value) {
        (attr::FONT_COLOR, PropertyValue::Color(c)) => Some(*c),
        _ => None,
    })
}

/// Every panel in `0x21000005` that declares `0x2E`, with its rows.
fn panels(ui: &UiSystem, root: ElemHandle) -> Vec<(ElemHandle, Vec<PageRow>)> {
    let mut all = Vec::new();
    walk(ui, root, &mut all);
    all.into_iter()
        .filter_map(|h| {
            let n = ui.node(h)?;
            if n.ty().0 != PANEL_TYPE {
                return None;
            }
            let v = n.merged_properties().get(attr::PAGES)?.clone();
            let rows = page_rows(&v);
            if rows.is_empty() {
                return None;
            }
            Some((h, rows))
        })
        .collect()
}

// ================================================================================================
// 0. The premise: the shipped layout really does put two differently coloured states on its tabs.
// ================================================================================================

/// If this fails, the assertions below are measuring nothing. `0x21000005` has tabbed panels,
/// each names its tabs through `0x2E`, and every tab authors both `0x0B` and `0x0C` with a font
/// colour that differs from its base state's.
#[test]
fn classic_gameplay_tabs_author_two_distinct_state_colours() {
    let (ui, root) = gameplay_root();
    let ps = panels(&ui, root);
    assert!(
        !ps.is_empty(),
        "classic_gameplay declares at least one tabbed panel element"
    );
    let mut checked = 0usize;
    for (p, rows) in &ps {
        for r in rows {
            let Some(tab) = ui.get_child_recursive(*p, r.tab) else {
                continue;
            };
            let base = font_color_in_state(&ui, tab, StateId(0));
            let closed = font_color_in_state(&ui, tab, TAB_CLOSED);
            let open = font_color_in_state(&ui, tab, TAB_OPEN);
            let (Some(base), Some(closed), Some(open)) = (base, closed, open) else {
                continue;
            };
            assert_ne!(
                base, closed,
                "tab {:#010X}'s closed state 0x0B must differ from its base state",
                r.tab.0
            );
            assert_ne!(
                base, open,
                "tab {:#010X}'s open state 0x0C must differ from its base state",
                r.tab.0
            );
            assert_ne!(
                closed, open,
                "tab {:#010X}'s two states must differ",
                r.tab.0
            );
            checked += 1;
        }
    }
    assert!(
        checked >= 8,
        "at least eight shipped tabs carry both states, saw {checked}"
    );
}

// ================================================================================================
// 1. The rejecting assertion.
// ================================================================================================

/// Behaviour: ui.tabs.a-tabbed-page-comes-up-with-each-tab-in-the-state-its-panel-set
/// Every tab of every panel holds the state the panel put it in.
#[test]
fn every_tab_of_every_panel_holds_the_state_the_panel_put_it_in() {
    let (ui, root) = gameplay_root();
    let ps = panels(&ui, root);
    let mut tabs = 0usize;
    let mut opens = 0usize;
    for (p, rows) in &ps {
        let pid = ui.node(*p).map_or(0, |n| n.element_id().0);
        for r in rows {
            let Some(tab) = ui.get_child_recursive(*p, r.tab) else {
                continue;
            };
            let want = if r.open { TAB_OPEN } else { TAB_CLOSED };
            // `Update` only restates a tab that the panel could find; a row whose tab element is
            // not in the subtree is the client's own silent skip, and is counted out above.
            let got = ui
                .node(tab)
                .map(|n| n.state)
                .expect("the tab is a live element");
            assert_eq!(
                got,
                want,
                "tab {:#010X} of panel {pid:#010X} is {} page's tab, so the panel's update \
                 pass left it in state {:#04X}",
                r.tab.0,
                if r.open { "the open" } else { "a closed" },
                want.0
            );
            tabs += 1;
            opens += usize::from(r.open);
        }
    }
    assert!(
        tabs >= 8,
        "classic_gameplay has at least eight panel tabs, saw {tabs}"
    );
    assert!(
        opens >= 1,
        "at least one panel opens with a page, saw {opens}"
    );
}

/// The same thing one level down, where a player sees it: the colour the text renderer reads off
/// each tab is the colour that tab's *own* state table authors for the state the panel chose, not
/// the base-state colour. This is the assertion that would still catch a `SetState` that moved the
/// state number without merging the state's properties.
#[test]
fn an_open_tab_is_the_colour_its_own_state_table_authors_for_the_open_state() {
    let (ui, root) = gameplay_root();
    let mut checked = 0usize;
    for (p, rows) in panels(&ui, root) {
        let pid = ui.node(p).map_or(0, |n| n.element_id().0);
        for r in rows {
            let Some(tab) = ui.get_child_recursive(p, r.tab) else {
                continue;
            };
            let want_state = if r.open { TAB_OPEN } else { TAB_CLOSED };
            let (Some(want), Some(got), Some(base)) = (
                font_color_in_state(&ui, tab, want_state),
                live_font_color(&ui, tab),
                font_color_in_state(&ui, tab, StateId(0)),
            ) else {
                continue;
            };
            if want == base {
                // This tab's authored colour for that state happens to equal its base; it proves
                // nothing either way, so it is not counted.
                continue;
            }
            assert_eq!(
                got, want,
                "tab {:#010X} of panel {pid:#010X} draws in the colour its state {:#04X} authors \
                 ({want:#010X}), not its base-state {base:#010X}",
                r.tab.0, want_state.0
            );
            checked += 1;
        }
    }
    assert!(
        checked >= 8,
        "at least eight tabs prove a colour, saw {checked}"
    );
}

// ================================================================================================
// 2. The general rule, on the routine itself.
// ================================================================================================

/// `initialize_tree` initialises the children before the element, for any tree — the plain
/// statement of the builder's order, so that a future edit that turns the walk round again
/// fails here first and not three stations away.
///
/// The oracle is a two-level tree whose root's default state passes to children and whose child
/// authors that state: after `initialize_tree` the child must hold the root's state, because the
/// root is initialised last.
#[test]
fn initialize_tree_initialises_the_children_first() {
    use dereth_ui::desc::{incorporation, ElementDesc, LayoutDesc, StateDesc};

    const PARENT: u32 = 0x1000_0001;
    const CHILD: u32 = 0x1000_0002;
    const S: u32 = 5;

    let child = ElementDesc {
        element_id: ElementId(CHILD),
        ty: dereth_ui::factory::ty::FIELD,
        default_state: StateId(0),
        states: std::iter::once((
            StateId(S),
            StateDesc {
                state_id: StateId(S),
                ..StateDesc::default()
            },
        ))
        .collect(),
        ..ElementDesc::default()
    };
    let parent = ElementDesc {
        base: StateDesc {
            incorporation: incorporation::LEGACY_ALL_GEOMETRY,
            width: 100,
            height: 100,
            ..StateDesc::default()
        },
        element_id: ElementId(PARENT),
        ty: dereth_ui::factory::ty::FIELD,
        default_state: StateId(S),
        states: std::iter::once((
            StateId(S),
            StateDesc {
                state_id: StateId(S),
                pass_to_children: true,
                ..StateDesc::default()
            },
        ))
        .collect(),
        children: std::iter::once((ElementId(CHILD), child)).collect(),
        child_chain: vec![ElementId(CHILD)],
        ..ElementDesc::default()
    };
    let layout = LayoutDesc {
        did: DataId(0x2100_0001),
        display_width: 800,
        display_height: 600,
        elements: std::iter::once((ElementId(PARENT), parent.clone())).collect(),
    };

    struct NoAssets;
    impl AssetSource for NoAssets {
        fn read(&self, _: DataId) -> Result<Vec<u8>, dereth_primitives::AssetError> {
            Err(dereth_primitives::AssetError::NotFound(DataId(0)))
        }
        fn exists(&self, _: DataId) -> bool {
            false
        }
        fn iter_type(
            &self,
            _: dereth_primitives::DataType,
        ) -> Box<dyn Iterator<Item = DataId> + '_> {
            Box::new(std::iter::empty())
        }
    }

    let mut ui = UiSystem::new((800, 600));
    let h = ui
        .create_element_recursive_from_full_desc(&NoAssets, &layout, &parent)
        .expect("no inheritance to resolve")
        .expect("type 3 is registered");
    let root = ui.root();
    ui.set_parent(h, Some(root));
    ui.initialize_tree(h);

    let c = ui
        .get_child_recursive(h, ElementId(CHILD))
        .expect("the child was built");
    assert_eq!(
        ui.node(h).map(|n| n.state),
        Some(StateId(S)),
        "the root took its default state"
    );
    assert_eq!(
        ui.node(c).map(|n| n.state),
        Some(StateId(S)),
        "the root is initialised LAST, so its pass-to-children \
         cascade is the last word on the child's state"
    );
}
