//! The drag walk is a recursion. A press that nothing up the chain will accept raises one
//! drag-rejected (`0x21`) per level on the unwind, the highest element reached first and the
//! pressed element last; a parent that refuses the drag-proxy request (its own `0x39`) refuses the
//! drag instead of letting a higher element be picked up; without the `0x39` the pressed element is
//! picked up at its own level. No shipped layout element carries a `0x39`, so the shipped data
//! cannot tell the two shapes apart.
//!
//! The walk takes the element's parent, recurses into it with the grab offset moved into its space,
//! and only when that answers false builds an element message naming **this** level's element and
//! broadcasts it. The draggable arm asks the parent for a drag proxy and returns false the instant
//! it says no. Fixture: three nested elements from a `LayoutDesc` written here, and an external
//! listener registered by element id; the census reads the retail dats.

use crate::common::NoAssets;
use dereth_primitives::{AssetSource, DataId, LocalTime};
use dereth_ui::desc::{incorporation, ElementDesc, LayoutDesc, StateDesc};
use dereth_ui::factory::ty;
use dereth_ui::focus::action;
use dereth_ui::msg::element::id as msgid;
use dereth_ui::props::attr;
use dereth_ui::{
    Delivery, ElemHandle, ElementId, ElementType, ListenerId, PropertyValue, UiSystem,
};

/// The three levels, outermost first. The chain continues into `UiSystem::root()` above `OUTER`,
/// which is where the walk's null-parent case finally lands.
const OUTER: u32 = 0x1000_0010;
const MID: u32 = 0x1000_0011;
const LEAF: u32 = 0x1000_0012;

/// The screen, in the arrangement every `dereth-ui-screens` panel uses: an `External` listener
/// registered by element id, so what it hears names the element that raised it.
const SCREEN: ListenerId = ListenerId::External(0x9001);

/// Inside `LEAF`, whose screen box is `(10+20, 10+20) = (30, 30)`.
const PRESS: (i32, i32) = (40, 40);
/// `5*5 + 5*5 = 50 > 15`, so the drag threshold is crossed on the first move.
const PAST_THRESHOLD: (i32, i32) = (45, 45);

fn desc(id: u32, ty: ElementType, x: i32, y: i32, w: i32, h: i32) -> ElementDesc {
    ElementDesc {
        base: StateDesc {
            incorporation: incorporation::LEGACY_ALL_GEOMETRY,
            x,
            y,
            width: w,
            height: h,
            ..StateDesc::default()
        },
        element_id: ElementId(id),
        ty,
        ..ElementDesc::default()
    }
}

struct Tree {
    ui: UiSystem,
    leaf: ElemHandle,
}

/// `OUTER > MID > LEAF`, three real levels of ancestry. `outer_dragable` and `leaf_dragable` write
/// attribute `0x3A`; `mid_refuses_proxy` writes `0x39` on the element that would have to build
/// `LEAF`'s proxy.
fn tree(outer_dragable: bool, mid_refuses_proxy: bool, leaf_dragable: bool) -> Tree {
    let mut ui = UiSystem::new((800, 600));
    let mut outer = desc(OUTER, ty::FIELD, 0, 0, 400, 200);
    let mut mid = desc(MID, ty::FIELD, 10, 10, 200, 100);
    let mut leaf = desc(LEAF, ty::FIELD, 20, 20, 50, 50);

    if outer_dragable {
        outer
            .base
            .properties
            .set(dereth_ui::props::attr::DRAGABLE, PropertyValue::Bool(true));
    }
    if mid_refuses_proxy {
        mid.base.properties.set(
            dereth_ui::props::attr::NO_DRAG_PROXY,
            PropertyValue::Bool(true),
        );
    }
    if leaf_dragable {
        leaf.base
            .properties
            .set(dereth_ui::props::attr::DRAGABLE, PropertyValue::Bool(true));
    }

    mid.children.insert(ElementId(LEAF), leaf);
    outer.children.insert(ElementId(MID), mid);
    let l = LayoutDesc {
        did: DataId(0x2100_0001),
        display_width: 800,
        display_height: 600,
        elements: std::iter::once((ElementId(OUTER), outer)).collect(),
    };
    let d = l
        .access_element(ElementId(OUTER))
        .cloned()
        .expect("the authored root");
    let o = ui
        .create_element_recursive_from_full_desc(&NoAssets, &l, &d)
        .expect("no inheritance to resolve")
        .expect("a registered type");
    let root = ui.root();
    ui.set_parent(o, Some(root));
    let m = ui.get_child(o, ElementId(MID)).expect("MID");
    let leaf = ui.get_child(m, ElementId(LEAF)).expect("LEAF");
    // Initialisation is what dispatches the description's properties through `on_set_attribute`,
    // and therefore what turns `0x3A` into the element's draggable flag.
    ui.initialize_tree(o);
    ui.set_mouse_visible(leaf, true);

    for id in [OUTER, MID, LEAF] {
        ui.register_for_element_message(ElementId(id), msgid::DRAG_REJECTED, SCREEN);
        ui.register_for_element_message(ElementId(id), msgid::DRAG_PROXY_CREATED, SCREEN);
    }
    ui.drain_outbox();
    Tree { ui, leaf }
}

impl Tree {
    /// A real press and a move
    /// past the 16-px threshold, which is the only thing that starts a drag-and-drop.
    fn press_and_drag(&mut self) {
        self.ui.mouse_down(action::PRIMARY_CLICK, PRESS.0, PRESS.1);
        self.ui
            .mouse_move(LocalTime(0.0), PAST_THRESHOLD.0, PAST_THRESHOLD.1);
    }

    /// Every `0x21` the screen heard, in delivery order, as `(element id, window point)`.
    fn rejections(&mut self) -> Vec<(u32, (i32, i32))> {
        self.ui
            .drain_outbox()
            .into_iter()
            .filter_map(|d| match d {
                Delivery::Element { to, msg } if to == SCREEN && msg.id == msgid::DRAG_REJECTED => {
                    Some((msg.source_id.0, msg.point.window))
                }
                _ => None,
            })
            .collect()
    }

    /// Every `0x14` the screen heard — the drag-proxy request actually built a proxy for that
    /// element.
    fn proxies(&mut self) -> Vec<u32> {
        self.ui
            .drain_outbox()
            .into_iter()
            .filter_map(|d| match d {
                Delivery::Element { to, msg }
                    if to == SCREEN && msg.id == msgid::DRAG_PROXY_CREATED =>
                {
                    Some(msg.source_id.0)
                }
                _ => None,
            })
            .collect()
    }
}

/// Behaviour: ui.drag.a-refused-drag-raises-rejected-at-every-level-it-climbed
///
/// Shape (1). Nothing in the chain is draggable, so the recursion runs to the top and every
/// level it entered raises its own `0x21` on the way back down. `LEAF` is **last**, not only.
///
/// Each message carries the latched drag x/y as its window point (the walk passes the press point
/// it latched), which is the same for every level — the element-local
/// half is what differs.
#[test]
fn every_level_the_walk_entered_raises_its_own_drag_rejected() {
    let mut t = tree(false, false, false);
    t.press_and_drag();

    assert_eq!(
        t.rejections(),
        vec![(OUTER, PRESS), (MID, PRESS), (LEAF, PRESS)],
        "the unwind fires once per level, outermost first and the pressed element last"
    );
    assert!(
        t.ui.drag_state().element.is_none(),
        "nothing accepted, so there is no proxy"
    );
    assert!(
        t.ui.drag_state().started,
        "the drag is armed before the walk begins"
    );
}

/// Shape (2). `LEAF` carries `0x3A`, so the draggable arm is taken; `MID` carries `0x39`, so
/// its drag-proxy call refuses and the walk returns false at once. `OUTER` is
/// draggable and must **not** be picked up instead, and no `0x21` is raised at any level.
#[test]
fn a_parent_that_refuses_the_proxy_refuses_the_drag_instead_of_climbing() {
    let mut t = tree(true, true, true);
    t.press_and_drag();

    assert!(
        t.ui.drag_state().element.is_none(),
        "the walk returns false; the draggable ancestor above the 0x39 is not a fallback"
    );
    assert_eq!(t.ui.drag_state().owner, None);
    assert_eq!(
        t.proxies(),
        Vec::<u32>::new(),
        "the drag-proxy request was refused, so no proxy was ever built"
    );
    assert_eq!(
        t.rejections(),
        Vec::<(u32, (i32, i32))>::new(),
        "the draggable arm has no 0x21 in it -- the refusal jumps straight to the epilogue"
    );
}

/// The control, so the two tests above cannot pass by refusing everything: with the `0x39` off
/// `MID`, the same press picks `LEAF` up at the first level and raises no `0x21` at all.
#[test]
fn without_the_0x39_the_pressed_element_is_picked_up_at_its_own_level() {
    let mut t = tree(true, false, true);
    t.press_and_drag();

    assert!(
        t.ui.drag_state().element.is_some(),
        "MID built LEAF's proxy"
    );
    assert_eq!(
        t.ui.drag_state().owner,
        Some(t.leaf),
        "the owner is the element that carried 0x3A"
    );
    assert_eq!(t.proxies(), vec![LEAF]);
    assert_eq!(t.rejections(), Vec::<(u32, (i32, i32))>::new());
}

// ---------------------------------------------------------------------------------------------
// Can any shipped layout observe shape (2)?
// ---------------------------------------------------------------------------------------------

/// The first 101 `LayoutDesc` ids, as `layout_conformance.rs` enumerates them.
const LAYOUT_FIRST: u32 = 0x2100_0000;
const LAYOUT_LAST: u32 = 0x2100_0075;

/// Every element description in one layout, roots and children alike.
fn walk(d: &ElementDesc, did: DataId, out: &mut Vec<(u32, u32)>) {
    let carries = |s: &StateDesc| s.properties.get_bool(attr::NO_DRAG_PROXY) == Some(true);
    if carries(&d.base) || d.states.values().any(carries) {
        out.push((did.0, d.element_id.0));
    }
    for c in d.children.values() {
        walk(c, did, out);
    }
}

/// Shape (2) decides what happens when the element that carries `0x3A` has a parent that
/// carries `0x39`. Nothing in the shipped data writes `0x39` at all, in any state of any element
/// of any of the 101 layouts, so the shape cannot be observed on retail content, and this is the
/// number that says so. If a later layout edit ever
/// introduces one, this goes red and the two shapes stop being interchangeable.
#[test]
fn no_shipped_layout_element_carries_the_0x39_that_would_make_the_walk_differ() {
    let dir = dereth_dat::testing::dat_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are the oracle here: set DERETH_TEST_DAT_DIR ({})",
        dir.display()
    );
    let store = dereth_dat::RetailDatStore::open_dir(&dir).expect("the retail dat store opens");
    let master_id = DataId(0x3900_0001);
    let bytes = store.read(master_id).expect("MasterProperty 0x39000001");
    let master =
        <dereth_assets::MasterProperty as dereth_assets::Decode>::decode_payload(master_id, &bytes)
            .expect("the property type table decodes");
    let types = master.property_types();

    let mut seen = 0_usize;
    let mut carriers: Vec<(u32, u32)> = Vec::new();
    for raw in LAYOUT_FIRST..=LAYOUT_LAST {
        let did = DataId(raw);
        if !store.exists(did) {
            continue;
        }
        let bytes = store.read(did).unwrap_or_else(|e| panic!("{did}: {e}"));
        let l = LayoutDesc::read(did, &bytes, &types).unwrap_or_else(|e| panic!("{did}: {e}"));
        seen += 1;
        for d in l.elements.values() {
            walk(d, did, &mut carriers);
        }
    }

    assert_eq!(seen, 101, "the shipped layout count from 36-ui-layouts.md");
    assert_eq!(
        carriers,
        Vec::<(u32, u32)>::new(),
        "no shipped (layout, element) sets UI_NoDragProxy (0x39)"
    );
}
