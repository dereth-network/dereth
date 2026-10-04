//! Manager-level tests: the pieces whose behaviour only exists once a tree, a listener table and a
//! frame loop are all present.
//!
//! Tests that need shipped layout data live in `tests/dat/layout_conformance.rs`.

use std::cell::RefCell;
use std::rc::Rc;

use dereth_primitives::{AssetError, AssetSource, DataId, DataType, LocalTime};

use crate::desc::{incorporation, ElementDesc, LayoutDesc, StateDesc};
use crate::element::{Element, ElementMessageListenResult as R};
use crate::factory::ty;
use crate::layout::{EdgeMode, Edges};
use crate::msg::element::id as msgid;
use crate::props::PropertyValue;
use crate::region::{Box2D, DrawStep, GraphicRef};
use crate::{
    ElemCtx, ElemHandle, ElementId, ElementMessage, ElementType, ListenerId, MessageId,
    NullInputPump, StateId, UiSystem,
};

// ---------------------------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------------------------

#[derive(Debug)]
struct NoAssets;
impl AssetSource for NoAssets {
    fn read(&self, id: DataId) -> Result<Vec<u8>, AssetError> {
        Err(AssetError::NotFound(id))
    }
    fn exists(&self, _: DataId) -> bool {
        false
    }
    fn iter_type(&self, _: DataType) -> Box<dyn Iterator<Item = DataId> + '_> {
        Box::new(std::iter::empty())
    }
}

/// A widget that records every call it receives, so a dispatch order can be asserted.
#[derive(Debug, Default)]
struct Recorder {
    log: Rc<RefCell<Vec<String>>>,
    name: String,
    stop: bool,
}

impl Element for Recorder {
    fn listen_to_element_message(&mut self, _ctx: &mut ElemCtx<'_>, m: &ElementMessage) -> R {
        self.log
            .borrow_mut()
            .push(format!("{}:elem:{:X}", self.name, m.id.0));
        if self.stop {
            R::StopProcessing
        } else {
            R::Default
        }
    }
    fn listen_to_global_message(&mut self, _ctx: &mut ElemCtx<'_>, id: MessageId, p: u32) {
        self.log
            .borrow_mut()
            .push(format!("{}:glob:{:X}:{p}", self.name, id.0));
    }
    fn on_set_attribute(&mut self, _c: &mut ElemCtx<'_>, id: u32, v: Option<&PropertyValue>) {
        self.log.borrow_mut().push(format!(
            "{}:attr:{id:X}={}",
            self.name,
            if v.is_some() { "set" } else { "cleared" }
        ));
    }
}

fn desc(id: u32, ty: u32, x: i32, y: i32, w: i32, h: i32) -> ElementDesc {
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
        ty: ElementType(ty),
        ..ElementDesc::default()
    }
}

fn layout(did: u32, roots: Vec<ElementDesc>) -> LayoutDesc {
    LayoutDesc {
        did: DataId(did),
        display_width: 800,
        display_height: 600,
        elements: roots.into_iter().map(|d| (d.element_id, d)).collect(),
    }
}

/// Build a tree from a layout already in the library, with no dat.
fn build(ui: &mut UiSystem, l: &LayoutDesc, id: ElementId) -> ElemHandle {
    let d = l.access_element(id).cloned().expect("root exists");
    let h = ui
        .create_element_recursive_from_full_desc(&NoAssets, l, &d)
        .expect("no inheritance to resolve")
        .expect("a registered type");
    let root = ui.root();
    ui.set_parent(h, Some(root));
    h
}

// ---------------------------------------------------------------------------------------------
// the nine-step draw
// ---------------------------------------------------------------------------------------------

/// Oracle: the client's nine steps and head-to-tail order.
#[test]
fn the_draw_trace_follows_the_nine_steps_and_head_to_tail_child_order() {
    let mut ui = UiSystem::new((800, 600));
    let mut parent = desc(1, 3, 0, 0, 400, 300);
    parent
        .children
        .insert(ElementId(2), desc(2, 3, 0, 0, 10, 10));
    parent
        .children
        .insert(ElementId(3), desc(3, 3, 20, 0, 10, 10));
    let l = layout(0x2100_0001, vec![parent]);
    let h = build(&mut ui, &l, ElementId(1));

    let trace = ui.draw_trace();
    let of = |x: ElemHandle| -> Vec<DrawStep> {
        trace
            .iter()
            .filter(|e| e.who == x)
            .map(|e| e.step)
            .collect()
    };
    assert_eq!(
        of(h),
        vec![
            DrawStep::DrawStart,
            DrawStep::PreBlit,
            DrawStep::DrawSelf,
            DrawStep::PostBlit,
            DrawStep::DrawChildren,
            DrawStep::DrawDone,
        ]
    );
    // The children are drawn between the parent's DrawChildren and DrawDone, head first.
    let kids = ui.children(h);
    let pos = |x: ElemHandle| trace.iter().position(|e| e.who == x).unwrap();
    assert!(
        pos(kids[0]) < pos(kids[1]),
        "head = bottom-most = first drawn"
    );
    let dc = trace
        .iter()
        .position(|e| e.who == h && e.step == DrawStep::DrawChildren)
        .unwrap();
    let dd = trace
        .iter()
        .position(|e| e.who == h && e.step == DrawStep::DrawDone)
        .unwrap();
    assert!(dc < pos(kids[0]) && pos(kids[1]) < dd);

    // Draw-after-children moves the whole child block to the front.
    ui.on_set_attribute(
        h,
        crate::props::attr::DRAW_AFTER_CHILDREN,
        Some(&PropertyValue::Bool(true)),
    );
    let trace = ui.draw_trace();
    assert_eq!(
        trace.iter().filter(|e| e.who == h).map(|e| e.step).next(),
        Some(DrawStep::DrawChildren)
    );
}

/// Oracle: the dirty-region draw's per-element step 1 — an invisible or fully clipped region emits
/// nothing at all, and neither do its children.
#[test]
fn an_invisible_subtree_draws_nothing() {
    let mut ui = UiSystem::new((800, 600));
    let mut parent = desc(1, 3, 0, 0, 400, 300);
    parent
        .children
        .insert(ElementId(2), desc(2, 3, 0, 0, 10, 10));
    let l = layout(0x2100_0001, vec![parent]);
    let h = build(&mut ui, &l, ElementId(1));
    let mut back = crate::RecordingDrawBackend::default();
    ui.draw(&mut back);
    let before = back.calls.len();
    ui.set_visible(h, false);
    back.calls.clear();
    ui.draw(&mut back);
    assert_eq!(
        before - back.calls.len(),
        2,
        "the element and its child both stop drawing"
    );
}

/// A full z and read order tie is stacked by hash creation order.
#[test]
fn a_full_z_and_read_order_tie_is_stacked_by_hash_creation_order() {
    use crate::desc::incorporate::incorporate;
    let child = |id: u32, ro: u32| {
        let mut d = desc(id, 3, 0, 0, 10, 10);
        d.read_order = ro;
        d
    };
    // The base, as its record stores it: 23 buckets, the chain in stored order.
    let mut base = desc(0x1000_024D, 3, 0, 0, 300, 55);
    base.children_buckets = 23;
    for (id, ro) in [
        (0x1000_0244, 4),
        (0x1000_0245, 5),
        (0x1000_024E, 1),
        (0x1000_0242, 2),
        (0x1000_0243, 3),
    ] {
        base.children.insert(ElementId(id), child(id, ro));
        base.child_chain.push(ElementId(id));
    }
    // The partial, likewise.
    let mut partial = desc(0x1000_0247, 0, 0, 0, 300, 55);
    partial.children_buckets = 23;
    for (id, ro) in [
        (0x1000_0244, 4),
        (0x1000_0245, 5),
        (0x1000_0246, 6),
        (0x1000_0247, 1),
        (0x1000_05EB, 7),
        (0x1000_0242, 2),
        (0x1000_0243, 3),
    ] {
        partial.children.insert(ElementId(id), child(id, ro));
        partial.child_chain.push(ElementId(id));
    }
    let mut full = base;
    incorporate(&mut full, &partial);
    assert_eq!(
        full.children[&ElementId(0x1000_0247)].read_order,
        2,
        "the meter's, shifted past the title"
    );
    assert_eq!(
        full.children[&ElementId(0x1000_0242)].read_order,
        2,
        "the label's, from the base"
    );
    assert_eq!(
        full.creation_order()
            .iter()
            .map(|i| i.0)
            .collect::<Vec<_>>(),
        vec![
            0x1000_0244,
            0x1000_0245,
            0x1000_0246,
            0x1000_0247,
            0x1000_024E,
            0x1000_05EB,
            0x1000_0242,
            0x1000_0243
        ],
        "bucket order under 23: 0, 1, 2, 3, 10, 15, 21, 22"
    );

    // And through the factory: the meter precedes the label in the parent's child list.
    let mut ui = UiSystem::new((800, 600));
    let l = layout(0x2100_0045, vec![full]);
    let h = build(&mut ui, &l, ElementId(0x1000_0247));
    let ids: Vec<u32> = ui
        .children(h)
        .iter()
        .filter_map(|c| ui.node(*c))
        .map(|n| n.element_id().0)
        .collect();
    assert_eq!(
        ids,
        vec![
            0x1000_024E,
            0x1000_0247,
            0x1000_0242,
            0x1000_0243,
            0x1000_0244,
            0x1000_0245,
            0x1000_0246,
            0x1000_05EB
        ],
        "title, meter, label, value, label, value, button, button -- the meter under the label"
    );
}

// ---------------------------------------------------------------------------------------------
// lifecycle and anchoring on a live tree
// ---------------------------------------------------------------------------------------------

/// Oracle: the element manager's refresh event — broadcast global 5, re-query the display,
/// resize the root, cascade. The hollow root's four edges are all mode 1,
/// so it always covers the screen.
#[test]
fn a_resolution_change_broadcasts_five_and_cascades_through_the_tree() {
    let mut ui = UiSystem::new((800, 600));
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut child = desc(2, 3, 0, 560, 800, 40); // a footer strip
    child.edges = Edges {
        left: EdgeMode::Fixed,
        top: EdgeMode::AnchorEnd,
        right: EdgeMode::AnchorStart,
        bottom: EdgeMode::AnchorStart,
    };
    let mut root = desc(1, 3, 0, 0, 800, 600);
    root.edges = Edges {
        left: EdgeMode::Fixed,
        top: EdgeMode::Fixed,
        right: EdgeMode::AnchorStart,
        bottom: EdgeMode::AnchorStart,
    };
    root.children.insert(ElementId(2), child);
    let l = layout(0x2100_0001, vec![root]);
    let h = build(&mut ui, &l, ElementId(1));
    if let Some(n) = ui.node_mut(h) {
        n.flags.set_is_root_element(true);
    }
    let kid = ui.get_child(h, ElementId(2)).unwrap();

    // A listener for global 5.
    let rec = Recorder {
        log: Rc::clone(&log),
        name: "L".into(),
        stop: false,
    };
    let listener = ui.create_hollow(None);
    ui.put_behaviour(listener, Box::new(rec));
    ui.register_for_global_message(crate::msg::global::REFRESH, ListenerId::Element(listener));

    assert_eq!(
        ui.node(h).unwrap().region.box_,
        Box2D::from_xywh(0, 0, 800, 600)
    );
    ui.refresh_event((1920, 1080));
    assert!(
        log.borrow().iter().any(|s| s == "L:glob:5:0"),
        "global 5 was broadcast"
    );
    assert_eq!(
        ui.node(h).unwrap().region.box_,
        Box2D::from_xywh(0, 0, 1920, 1080)
    );
    // The footer follows the bottom and stretches to the new width.
    assert_eq!(
        ui.node(kid).unwrap().region.box_,
        Box2D::from_xywh(0, 1040, 1920, 40)
    );
}

/// Oracle: elements are never deleted inline while the tree is being walked, and the deletion
/// queue is drained at the very top of `UseTime`.
#[test]
fn deletion_is_deferred_to_the_top_of_the_next_frame() {
    let mut ui = UiSystem::new((800, 600));
    let mut root = desc(1, 3, 0, 0, 100, 100);
    root.children.insert(ElementId(2), desc(2, 3, 0, 0, 10, 10));
    let l = layout(0x2100_0001, vec![root]);
    let h = build(&mut ui, &l, ElementId(1));
    let kid = ui.get_child(h, ElementId(2)).unwrap();

    ui.add_to_delete_queue(h);
    ui.add_to_delete_queue(h); // queued at most once
    assert_eq!(ui.delete_queue_len(), 1);
    assert!(ui.is_alive(h), "still alive until the queue is drained");

    ui.use_time(LocalTime(1.0), &mut NullInputPump);
    assert!(!ui.is_alive(h));
    assert!(
        !ui.is_alive(kid),
        "element destruction deletes its children"
    );
    assert!(
        ui.get_element(ElementId(1)).is_none(),
        "and drops out of the element list"
    );
}

// ---------------------------------------------------------------------------------------------
// the factory
// ---------------------------------------------------------------------------------------------

/// An unknown element type drops its whole subtree and is logged.
#[test]
fn an_unknown_element_type_drops_its_whole_subtree_and_is_logged() {
    let mut ui = UiSystem::new((800, 600));
    // Type 4 has no registration in this build.
    let mut orphan = desc(2, 4, 0, 0, 10, 10);
    orphan.children.insert(ElementId(3), desc(3, 3, 0, 0, 5, 5));
    orphan.children.insert(ElementId(4), desc(4, 3, 0, 0, 5, 5));
    let mut root = desc(1, 3, 0, 0, 100, 100);
    root.children.insert(ElementId(2), orphan);
    root.children.insert(ElementId(5), desc(5, 3, 0, 0, 5, 5));
    let l = layout(0x2100_0001, vec![root]);
    let h = build(&mut ui, &l, ElementId(1));

    assert_eq!(ui.children(h).len(), 1, "only the sibling survives");
    assert!(ui.get_element(ElementId(3)).is_none());
    assert_eq!(ui.dropped.len(), 1);
    assert_eq!(ui.dropped[0].ty, ElementType(4));
    assert_eq!(
        ui.dropped[0].descendants, 3,
        "the node and its two children"
    );

    for t in ty::UNREGISTERED {
        assert!(
            !ui.is_registered(t),
            "{t:?} has no registration in this build (#115)"
        );
    }
}

/// Oracle: UI-manager initialization registers fourteen engine widgets followed by seven dialogs,
/// in this exact order.
#[test]
fn init_registers_the_documented_engine_classes() {
    let ui = UiSystem::new((800, 600));
    for t in [
        ty::FIELD,
        ty::BUTTON,
        ty::TEXT,
        ty::SCROLLBAR,
        ty::METER,
        ty::LISTBOX,
        ty::MENU,
        ty::DRAGBAR,
        ty::PANEL,
        ty::VIEWPORT,
        ty::RESIZEBAR,
        ty::BROWSER,
        ty::COLOR_PICKER,
        ty::GROUP_BOX,
    ] {
        assert!(ui.is_registered(t), "{t:?}");
    }
    for t in 0x13..=0x19_u32 {
        assert!(ui.is_registered(ElementType(t)), "dialog type {t:#X}");
    }
    assert_eq!(ui.registered_types().len(), 14 + 7);
}

/// Pinned behavior: element id 8, type 3, and the display size with
/// incorporation bits 8 and 0x10, all four edges mode 1, and an **800 × 600 fallback** when there
/// is no device yet.
#[test]
fn the_hollow_root_is_the_documented_prototype() {
    let ui = UiSystem::new((1024, 768));
    let n = ui.node(ui.root()).unwrap();
    assert_eq!(n.element_id(), ElementId(8));
    assert_eq!(n.ty(), ty::FIELD);
    assert_eq!(n.region.box_, Box2D::from_xywh(0, 0, 1024, 768));
    assert_eq!(
        n.desc.base.incorporation,
        incorporation::WIDTH | incorporation::HEIGHT
    );
    assert_eq!(
        n.desc.edges,
        Edges {
            left: EdgeMode::AnchorStart,
            top: EdgeMode::AnchorStart,
            right: EdgeMode::AnchorStart,
            bottom: EdgeMode::AnchorStart
        }
    );
    assert!(
        n.flags.activatable(),
        "Init marks it activatable and pushes it on the activatable-element list"
    );

    let ui = UiSystem::new((0, 0));
    assert_eq!(
        ui.node(ui.root()).unwrap().region.box_,
        crate::factory::hollow_fallback_box()
    );
}

/// Oracle: `create_element_recursive_from_full_desc` step 4 — registering for 0x19/0x1C/0x1D/0x40 on an
/// element id makes every element with that id hit-testable, both at creation time and
/// retroactively.
#[test]
fn registering_for_a_click_message_makes_the_element_mouse_visible() {
    let mut ui = UiSystem::new((800, 600));
    let l = layout(0x2100_0001, vec![desc(1, 3, 0, 0, 100, 100)]);
    // Before creation.
    ui.register_for_element_message(ElementId(1), msgid::MOUSE_CLICK, ListenerId::External(1));
    let h = build(&mut ui, &l, ElementId(1));
    assert!(ui.node(h).unwrap().is_mouse_visible);

    // Retroactively.
    let mut ui = UiSystem::new((800, 600));
    let h = build(&mut ui, &l, ElementId(1));
    assert!(!ui.node(h).unwrap().is_mouse_visible);
    ui.register_for_element_message(ElementId(1), msgid::MOUSE_TAP, ListenerId::External(1));
    assert!(ui.node(h).unwrap().is_mouse_visible);
}

// ---------------------------------------------------------------------------------------------
// the property surface
// ---------------------------------------------------------------------------------------------

/// Oracle: a state change produces the exact `on_set_attribute` call sequence asserted below,
/// including the value-less form.
#[test]
fn driving_an_element_through_its_states_produces_the_documented_attribute_calls() {
    let mut ui = UiSystem::new((800, 600));
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut d = desc(1, 3, 0, 0, 40, 20);
    d.base.properties.set(0x3B, PropertyValue::Bool(true)); // hidden, on the element
    d.base.properties.set(0x4D, PropertyValue::Float(1.0)); // alpha, on the element
    let mut pressed = StateDesc {
        state_id: StateId(5),
        ..StateDesc::default()
    };
    pressed.properties.set(0x4D, PropertyValue::Float(0.5)); // alpha differs in the pressed state
    pressed.properties.set(0x53, PropertyValue::Bool(true)); // and it adds one
    d.states.insert(StateId(5), pressed);
    let l = layout(0x2100_0001, vec![d]);
    let h = build(&mut ui, &l, ElementId(1));
    ui.put_behaviour(
        h,
        Box::new(Recorder {
            log: Rc::clone(&log),
            name: "b".into(),
            stop: false,
        }),
    );

    ui.set_state(h, StateId(5));
    assert_eq!(
        *log.borrow(),
        vec!["b:attr:4D=set", "b:attr:53=set"],
        "0x3B is identical in both states and is not reported"
    );
    assert_eq!(ui.node(h).unwrap().region.alpha_blend_mod, 0.5);

    log.borrow_mut().clear();
    ui.set_state(h, StateId(0));
    assert_eq!(
        *log.borrow(),
        vec!["b:attr:4D=set", "b:attr:53=cleared"],
        "0x53 disappeared, so it is reported with no value"
    );
    assert_eq!(ui.node(h).unwrap().region.alpha_blend_mod, 1.0);
}

/// Attribute 0x3b is hide so true takes the element down and false puts it back.
#[test]
fn attribute_0x3b_is_hide_so_true_takes_the_element_down_and_false_puts_it_back() {
    let mut ui = UiSystem::new((800, 600));
    let mut d = desc(1, 3, 0, 0, 40, 20);
    d.base
        .properties
        .set(crate::props::attr::HIDE, PropertyValue::Bool(true));
    // A state that *unhides* it, which is the shape `classic_intro`'s two media children use:
    // hidden at element level, shown in the one state that owns the frame's media.
    let mut shown = StateDesc {
        state_id: StateId(5),
        ..StateDesc::default()
    };
    shown
        .properties
        .set(crate::props::attr::HIDE, PropertyValue::Bool(false));
    d.states.insert(StateId(5), shown);
    let l = layout(0x2100_0001, vec![d]);
    let h = build(&mut ui, &l, ElementId(1));
    // The element's initialisation, step 4, is what pushes the description's properties through
    // `on_set_attribute`; the factory deliberately leaves it to the owning framework.
    ui.initialize_tree(h);

    assert!(
        !ui.node(h).unwrap().region.flags.visible,
        "hide = true means hidden"
    );
    ui.set_state(h, StateId(5));
    assert!(
        ui.node(h).unwrap().region.flags.visible,
        "hide = false means shown"
    );
    // Leaving the state drops the property entirely; the diff reports it value-less and the
    // element goes back to the constructor default, which is visible.
    ui.set_state(h, StateId(0));
    assert!(
        !ui.node(h).unwrap().region.flags.visible,
        "back to the element-level hide = true"
    );

    // And the run-time setter agrees with the description path.
    ui.set_attribute_bool(h, crate::props::attr::HIDE, false);
    assert!(ui.node(h).unwrap().region.flags.visible);
    ui.set_attribute_bool(h, crate::props::attr::HIDE, true);
    assert!(!ui.node(h).unwrap().region.flags.visible);
}

/// Oracle: the pass-to-children flag on a state
/// makes entering it recurse into children.
#[test]
fn pass_to_children_recurses_the_state_change() {
    let mut ui = UiSystem::new((800, 600));
    let mut child = desc(2, 3, 0, 0, 10, 10);
    child.states.insert(
        StateId(5),
        StateDesc {
            state_id: StateId(5),
            ..StateDesc::default()
        },
    );
    let mut root = desc(1, 3, 0, 0, 100, 100);
    root.states.insert(
        StateId(5),
        StateDesc {
            state_id: StateId(5),
            pass_to_children: true,
            ..StateDesc::default()
        },
    );
    root.children.insert(ElementId(2), child);
    let l = layout(0x2100_0001, vec![root]);
    let h = build(&mut ui, &l, ElementId(1));
    let kid = ui.get_child(h, ElementId(2)).unwrap();
    ui.set_state(h, StateId(5));
    assert_eq!(ui.node(kid).unwrap().state, StateId(5));
}

// ---------------------------------------------------------------------------------------------
// element messages
// ---------------------------------------------------------------------------------------------

/// Pinned behavior: the serial-number filter. the description says, "Without
/// it, a listener registered both on a child and on its parent receives the same message twice, and
/// several panels (notably the toolbar and item-list elements) would double-handle clicks."
///
#[test]
fn a_listener_on_both_a_child_and_its_parent_sees_one_message_not_two() {
    let mut ui = UiSystem::new((800, 600));
    let mut root = desc(1, 3, 0, 0, 100, 100);
    root.children.insert(ElementId(2), desc(2, 3, 0, 0, 10, 10));
    let l = layout(0x2100_0001, vec![root]);
    let h = build(&mut ui, &l, ElementId(1));
    let kid = ui.get_child(h, ElementId(2)).unwrap();

    let who = ListenerId::External(42);
    ui.register_for_element_messages(kid, who);
    ui.register_for_element_messages(h, who);

    ui.broadcast_element_message(kid, msgid::BUTTON_CLICKED, 0, 0);
    let out = ui.drain_outbox();
    assert_eq!(out.len(), 1, "one delivery, not two: {out:?}");

    // The bubble itself still happens: a *different* listener on the parent hears it.
    let other = ListenerId::External(43);
    ui.register_for_element_messages(h, other);
    ui.broadcast_element_message(kid, msgid::BUTTON_CLICKED, 0, 0);
    let out = ui.drain_outbox();
    assert_eq!(out.len(), 2);
}

/// Oracle: the same function's "if the result is stop-processing, return" — stop-processing
/// stops the bubble immediately.
#[test]
fn stop_processing_ends_the_bubble() {
    let mut ui = UiSystem::new((800, 600));
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut root = desc(1, 3, 0, 0, 100, 100);
    root.children.insert(ElementId(2), desc(2, 3, 0, 0, 10, 10));
    let l = layout(0x2100_0001, vec![root]);
    let h = build(&mut ui, &l, ElementId(1));
    let kid = ui.get_child(h, ElementId(2)).unwrap();
    ui.put_behaviour(
        h,
        Box::new(Recorder {
            log: Rc::clone(&log),
            name: "parent".into(),
            stop: false,
        }),
    );
    ui.put_behaviour(
        kid,
        Box::new(Recorder {
            log: Rc::clone(&log),
            name: "child".into(),
            stop: true,
        }),
    );

    ui.broadcast_element_message(kid, msgid::BUTTON_CLICKED, 0, 0);
    assert_eq!(
        *log.borrow(),
        vec!["child:elem:1"],
        "the parent never hears it"
    );
}

/// Oracle: deferred unregistration lets panels delete themselves inside a message handler without
/// corrupting the active broadcast.
#[test]
fn a_listener_that_unregisters_itself_mid_broadcast_does_not_corrupt_the_iteration() {
    /// A widget that unregisters itself and queues its own deletion from inside its handler.
    #[derive(Debug)]
    struct Suicidal {
        log: Rc<RefCell<Vec<String>>>,
    }
    impl Element for Suicidal {
        fn listen_to_element_message(&mut self, ctx: &mut ElemCtx<'_>, _m: &ElementMessage) -> R {
            self.log.borrow_mut().push("suicide".into());
            let me = ListenerId::Element(ctx.me);
            ctx.ui.unregister_for_all_messages(me);
            let h = ctx.me;
            ctx.ui.add_to_delete_queue(h);
            R::Default
        }
    }

    let mut ui = UiSystem::new((800, 600));
    let log = Rc::new(RefCell::new(Vec::new()));
    let l = layout(
        0x2100_0001,
        vec![
            desc(1, 3, 0, 0, 100, 100),
            desc(2, 3, 0, 0, 100, 100),
            desc(3, 3, 0, 0, 100, 100),
        ],
    );
    let a = build(&mut ui, &l, ElementId(1));
    let b = build(&mut ui, &l, ElementId(2));
    let c = build(&mut ui, &l, ElementId(3));
    ui.put_behaviour(
        b,
        Box::new(Suicidal {
            log: Rc::clone(&log),
        }),
    );
    ui.put_behaviour(
        c,
        Box::new(Recorder {
            log: Rc::clone(&log),
            name: "c".into(),
            stop: false,
        }),
    );

    // All three listen to element 1's message 1.
    for h in [a, b, c] {
        ui.register_for_element_message(
            ElementId(1),
            msgid::BUTTON_CLICKED,
            ListenerId::Element(h),
        );
    }
    ui.broadcast_element_message(a, msgid::BUTTON_CLICKED, 0, 0);
    assert_eq!(*log.borrow(), vec!["suicide", "c:elem:1"], "c still ran");
    assert!(
        ui.is_alive(b),
        "the removal and the deletion are both deferred"
    );

    ui.use_time(LocalTime(1.0), &mut NullInputPump);
    assert!(!ui.is_alive(b));
    // And the table no longer names it.
    log.borrow_mut().clear();
    ui.broadcast_element_message(a, msgid::BUTTON_CLICKED, 0, 0);
    assert_eq!(*log.borrow(), vec!["c:elem:1"]);
}

/// The base handler turns activation and focus into state changes.
#[test]
fn the_base_handler_turns_activation_and_focus_into_state_changes() {
    let mut ui = UiSystem::new((800, 600));
    let mut d = desc(1, 3, 0, 0, 40, 20);
    for s in [1_u32, 4, 5] {
        d.states.insert(
            StateId(s),
            StateDesc {
                state_id: StateId(s),
                ..StateDesc::default()
            },
        );
    }
    d.base.properties.set(
        crate::props::attr::VISIBILITY_TOGGLE_MODE,
        PropertyValue::Enum(1),
    );
    let l = layout(0x2100_0001, vec![d]);
    let h = build(&mut ui, &l, ElementId(1));

    ui.broadcast_element_message(h, msgid::ACTIVATED, 0, 0);
    assert_eq!(ui.node(h).unwrap().state, crate::element::state::ACTIVE);
    ui.broadcast_element_message(h, msgid::DEACTIVATED, 0, 0);
    assert_eq!(ui.node(h).unwrap().state, crate::element::state::NORMAL);
    ui.broadcast_element_message(h, msgid::FOCUS_CHANGED, 1, 0);
    assert_eq!(ui.node(h).unwrap().state, crate::element::state::FOCUSED);
    ui.broadcast_element_message(h, msgid::FOCUS_CHANGED, 0, 0);
    assert_eq!(ui.node(h).unwrap().state, crate::element::state::NORMAL);

    // 0x31 reads attribute 0x58: **1 = toggle**, asserted in both directions on the same element,
    // because a single `visible -> hidden` step reads identically to the `3 = hide` arm.
    assert!(ui.node(h).unwrap().region.flags.visible);
    ui.broadcast_element_message(h, msgid::VISIBILITY_TOGGLE, 0, 0);
    assert!(
        !ui.node(h).unwrap().region.flags.visible,
        "0x58 = 1 toggled it down"
    );
    ui.broadcast_element_message(h, msgid::VISIBILITY_TOGGLE, 0, 0);
    assert!(
        ui.node(h).unwrap().region.flags.visible,
        "…and back up, which `3` would not do"
    );
}

// ---------------------------------------------------------------------------------------------
// global messages and notices
// ---------------------------------------------------------------------------------------------

/// Pinned behavior: it walks the global-message listener table's entry for the id; there
/// is no bubbling, no serial-number filter and no return value."
#[test]
fn global_three_reaches_only_its_registered_listeners() {
    let mut ui = UiSystem::new((800, 600));
    let log = Rc::new(RefCell::new(Vec::new()));
    let l = layout(
        0x2100_0001,
        vec![desc(1, 3, 0, 0, 10, 10), desc(2, 3, 0, 0, 10, 10)],
    );
    let a = build(&mut ui, &l, ElementId(1));
    let b = build(&mut ui, &l, ElementId(2));
    ui.put_behaviour(
        a,
        Box::new(Recorder {
            log: Rc::clone(&log),
            name: "a".into(),
            stop: false,
        }),
    );
    ui.put_behaviour(
        b,
        Box::new(Recorder {
            log: Rc::clone(&log),
            name: "b".into(),
            stop: false,
        }),
    );
    ui.register_for_global_message(crate::msg::global::TICK, ListenerId::Element(a));

    ui.broadcast_global(crate::msg::global::TICK, 0);
    assert_eq!(*log.borrow(), vec!["a:glob:3:0"]);

    // An unregistration takes effect immediately outside a broadcast.
    log.borrow_mut().clear();
    ui.unregister_for_global_message(crate::msg::global::TICK, ListenerId::Element(a));
    ui.broadcast_global(crate::msg::global::TICK, 0);
    assert!(log.borrow().is_empty());
    // An id with no listeners is a no-op, not an error.
    ui.broadcast_global(MessageId(0x1234), 7);
}

// ---------------------------------------------------------------------------------------------
// hit testing, focus, capture, drag
// ---------------------------------------------------------------------------------------------

/// Oracle: the mouse hit tester's recursion plus
/// the region's point-is-over test — hit-test a fixture panel
/// with a real alpha mask and assert clicks pass through transparent corners".
#[test]
fn clicks_pass_through_a_panels_transparent_corners() {
    let mut ui = UiSystem::new((800, 600));
    // A 4x4 "panel frame" at (10,10) with transparent corners, over a 100x100 backdrop.
    let mut back = desc(1, 3, 0, 0, 100, 100);
    back.children.insert(ElementId(2), desc(2, 3, 10, 10, 4, 4));
    let l = layout(0x2100_0001, vec![back]);
    let h = build(&mut ui, &l, ElementId(1));
    let frame = ui.get_child(h, ElementId(2)).unwrap();
    ui.set_mouse_visible(h, true);
    ui.set_mouse_visible(frame, true);
    let mut mask = vec![true; 16];
    for (x, y) in [(0, 0), (3, 0), (0, 3), (3, 3)] {
        mask[y * 4 + x] = false;
    }
    ui.node_mut(frame).unwrap().region.alpha_image = Some(GraphicRef {
        did: DataId(0x0600_0001),
        source: crate::ImageSource::Interface,
        width: 4,
        height: 4,
        opaque: Some(mask),
        op: None,
    });

    assert_eq!(
        ui.hit_test_screen(11, 10),
        Some(frame),
        "an opaque edge hits the frame"
    );
    assert_eq!(
        ui.hit_test_screen(10, 10),
        Some(h),
        "the transparent corner falls through"
    );
    assert_eq!(
        ui.hit_test_screen(13, 13),
        Some(h),
        "and so does the other corner"
    );
    assert_eq!(ui.hit_test_screen(500, 500), None, "outside everything");
}

/// Oracle: the same function — "an element that is neither mouse-visible nor click-blocking is
/// transparent to the mouse but its children are still searched", and the block-clicks flag "makes an
/// invisible element swallow clicks (used by modal dialog backdrops)".
#[test]
fn a_click_blocking_backdrop_swallows_clicks_a_transparent_parent_does_not() {
    let mut ui = UiSystem::new((800, 600));
    let mut parent = desc(1, 3, 0, 0, 100, 100);
    parent
        .children
        .insert(ElementId(2), desc(2, 3, 10, 10, 10, 10));
    let l = layout(0x2100_0001, vec![parent]);
    let h = build(&mut ui, &l, ElementId(1));
    let kid = ui.get_child(h, ElementId(2)).unwrap();
    ui.set_mouse_visible(kid, true);

    assert_eq!(
        ui.hit_test_screen(15, 15),
        Some(kid),
        "the child is found through the parent"
    );
    assert_eq!(
        ui.hit_test_screen(50, 50),
        None,
        "the parent itself is transparent"
    );

    ui.node_mut(h).unwrap().region.flags.block_clicks = true;
    assert_eq!(ui.hit_test_screen(50, 50), Some(h), "now it swallows");
    assert_eq!(
        ui.hit_test_screen(15, 15),
        Some(kid),
        "the child still wins: tail -> head"
    );
}

/// Oracle: the manager's set- and release-mouse-capture and its
/// mouse-up event — capture is reference-counted and released when
/// set of capture-triggering actions empties.
#[test]
fn mouse_capture_is_reference_counted() {
    let mut ui = UiSystem::new((800, 600));
    let l = layout(0x2100_0001, vec![desc(1, 3, 0, 0, 100, 100)]);
    let h = build(&mut ui, &l, ElementId(1));
    ui.set_mouse_visible(h, true);

    ui.mouse_down(crate::focus::action::PRIMARY_CLICK, 50, 50);
    assert_eq!(ui.mouse.capture, Some(h));
    ui.mouse_down(crate::focus::action::SECONDARY_CLICK, 50, 50);
    assert_eq!(
        ui.mouse.capture,
        Some(h),
        "still captured, and now by two actions"
    );
    ui.mouse_up(crate::focus::action::PRIMARY_CLICK, 50, 50, false);
    assert_eq!(ui.mouse.capture, Some(h), "one action still holds it");
    ui.mouse_up(crate::focus::action::SECONDARY_CLICK, 50, 50, false);
    assert_eq!(ui.mouse.capture, None);
}

/// Oracle: the manager's mouse-up event → the element's `MouseUp` — 0x1D (release), then 0x19
/// (click) "when the corresponding press was on the same element"; a tap action (0x0D/0x0E/0x0F)
/// takes no capture and raises 0x40 instead.
#[test]
fn press_release_and_click_fire_in_the_documented_order() {
    let mut ui = UiSystem::new((800, 600));
    let l = layout(0x2100_0001, vec![desc(1, 3, 0, 0, 100, 100)]);
    let h = build(&mut ui, &l, ElementId(1));
    ui.set_mouse_visible(h, true);
    let who = ListenerId::External(1);
    for m in [
        msgid::MOUSE_PRESS,
        msgid::MOUSE_RELEASE,
        msgid::MOUSE_CLICK,
        msgid::MOUSE_TAP,
    ] {
        ui.register_for_element_message(ElementId(1), m, who);
    }
    ui.drain_outbox();

    ui.mouse_down(crate::focus::action::PRIMARY_CLICK, 50, 50);
    ui.mouse_up(crate::focus::action::PRIMARY_CLICK, 50, 50, false);
    let ids: Vec<u32> = ui
        .drain_outbox()
        .into_iter()
        .filter_map(|d| match d {
            crate::Delivery::Element { msg, .. } => Some(msg.id.0),
            _ => None,
        })
        .collect();
    assert_eq!(ids, vec![0x1C, 0x1D, 0x19]);

    ui.mouse_down(crate::focus::action::TAPS[0], 50, 50);
    let ids: Vec<u32> = ui
        .drain_outbox()
        .into_iter()
        .filter_map(|d| match d {
            crate::Delivery::Element { msg, .. } => Some(msg.id.0),
            _ => None,
        })
        .collect();
    assert_eq!(ids, vec![0x40], "a tap raises 0x40 and takes no capture");
    assert_eq!(ui.mouse.capture, None);
}

/// Oracle: the drag threshold `dx² + dy² > 15`, the proxy, the
/// catcher search and the 0x15/0x16 pair. Drag an item
/// between two elements and assert the callback fires once".
#[test]
fn dragging_between_two_elements_fires_the_drop_once() {
    let mut ui = UiSystem::new((800, 600));
    let mut root = desc(1, 3, 0, 0, 200, 100);
    let mut src = desc(2, 3, 0, 0, 50, 50);
    src.base
        .properties
        .set(crate::props::attr::DRAGABLE, PropertyValue::Bool(true));
    let mut dst = desc(3, 3, 100, 0, 50, 50);
    dst.base
        .properties
        .set(crate::props::attr::DROP_CATCHER, PropertyValue::Bool(true));
    root.children.insert(ElementId(2), src);
    root.children.insert(ElementId(3), dst);
    let l = layout(0x2100_0001, vec![root]);
    let h = build(&mut ui, &l, ElementId(1));
    ui.initialize_tree(h);
    let src = ui.get_child(h, ElementId(2)).unwrap();
    let dst = ui.get_child(h, ElementId(3)).unwrap();
    ui.set_mouse_visible(src, true);
    ui.set_mouse_visible(dst, true);

    let who = ListenerId::External(1);
    for m in [
        msgid::DRAG_PROXY_CREATED,
        msgid::DROP_FAILED,
        msgid::DROP_SUCCEEDED,
    ] {
        ui.register_for_element_message(ElementId(2), m, who);
        ui.register_for_element_message(ElementId(3), m, who);
    }
    ui.drain_outbox();

    ui.mouse_down(crate::focus::action::PRIMARY_CLICK, 10, 10);
    ui.mouse_move(LocalTime(0.0), 12, 12); // 8 <= 15: below the threshold
    assert!(!ui.drag.started);
    ui.mouse_move(LocalTime(0.0), 15, 15); // 50 > 15
    assert!(
        ui.drag.started,
        "the proxy exists once the threshold is passed"
    );
    ui.mouse_move(LocalTime(0.0), 120, 20);
    ui.mouse_up(crate::focus::action::PRIMARY_CLICK, 120, 20, false);

    let ids: Vec<u32> = ui
        .drain_outbox()
        .into_iter()
        .filter_map(|d| match d {
            crate::Delivery::Element { msg, .. } => Some(msg.id.0),
            _ => None,
        })
        .collect();
    assert_eq!(
        ids,
        vec![0x14, 0x15, 0x16],
        "proxy created, dropped on the target, succeeded"
    );
    assert!(!ui.drag.started);
    assert!(ui.drag.element.is_none());
}

/// Oracle: the manager's set-focus-element, its five steps.
#[test]
fn focus_transfer_raises_lost_then_gained() {
    let mut ui = UiSystem::new((800, 600));
    let l = layout(
        0x2100_0001,
        vec![desc(1, 3, 0, 0, 10, 10), desc(2, 3, 0, 0, 10, 10)],
    );
    let a = build(&mut ui, &l, ElementId(1));
    let b = build(&mut ui, &l, ElementId(2));
    let who = ListenerId::External(1);
    ui.register_for_element_message(ElementId(1), msgid::FOCUS_CHANGED, who);
    ui.register_for_element_message(ElementId(2), msgid::FOCUS_CHANGED, who);
    ui.drain_outbox();

    ui.take_focus(a);
    ui.take_focus(b);
    ui.take_focus(b); // "if the same element, do nothing"
    let seen: Vec<(u32, u32)> = ui
        .drain_outbox()
        .into_iter()
        .filter_map(|d| match d {
            crate::Delivery::Element { msg, .. } => Some((msg.source_id.0, msg.p1)),
            _ => None,
        })
        .collect();
    assert_eq!(seen, vec![(1, 1), (1, 0), (2, 1)]);
    assert_eq!(ui.focus_element(), Some(b));
}

// ---------------------------------------------------------------------------------------------
// text at the manager level
// ---------------------------------------------------------------------------------------------

/// Oracle: step 6 — "**always** broadcast element
/// message 0x12 with the character in `p1`, **even when the character was not inserted**".
#[test]
fn message_0x12_fires_for_a_rejected_character_too() {
    let mut ui = UiSystem::new((800, 600));
    let l = layout(0x2100_0001, vec![desc(1, 0x0C, 0, 0, 100, 20)]);
    let h = build(&mut ui, &l, ElementId(1));
    {
        // Downcast-free: replace the widget with one carrying the settings this test needs.
        let _ = ui.take_behaviour(h).expect("the text factory ran");
        let mut t = crate::text::TextElement::default();
        t.bits.set_editable(true);
        t.filter = Some(crate::text::edit::number_input_filter);
        ui.put_behaviour(h, Box::new(t));
    }
    ui.take_focus(h);
    let who = ListenerId::External(1);
    ui.register_for_element_message(ElementId(1), msgid::CHARACTER, who);
    ui.drain_outbox();

    ui.character(b'a' as u16); // rejected by the filter
    ui.character(b'7' as u16); // accepted
    let chars: Vec<u32> = ui
        .drain_outbox()
        .into_iter()
        .filter_map(|d| match d {
            crate::Delivery::Element { msg, .. } if msg.id == msgid::CHARACTER => Some(msg.p1),
            _ => None,
        })
        .collect();
    assert_eq!(chars, vec![u32::from(b'a'), u32::from(b'7')]);
}

// ---------------------------------------------------------------------------------------------
// the flow
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Default)]
struct StubScreen {
    name: &'static str,
    roots: Vec<ElemHandle>,
    log: Rc<RefCell<Vec<String>>>,
    error: Option<String>,
}

thread_local! {
    static SCREEN_LOG: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
}

impl crate::Screen for StubScreen {
    fn create(&mut self, cx: &mut crate::framework::ScreenCx<'_>) -> Result<(), crate::UiError> {
        let ui = &mut *cx.ui;
        self.log = SCREEN_LOG.with(Rc::clone);
        self.log.borrow_mut().push(format!("create:{}", self.name));
        let root = ui.create_hollow(Some(ui.root()));
        if let Some(n) = ui.node_mut(root) {
            n.flags.set_is_root_element(true);
        }
        self.roots.push(root);
        Ok(())
    }
    fn destroy(&mut self, _cx: &mut crate::framework::ScreenCx<'_>) {
        self.log.borrow_mut().push(format!("destroy:{}", self.name));
    }
    fn set_error_msg(&mut self, s: String) {
        self.error = Some(s.clone());
        SCREEN_LOG.with(|l| l.borrow_mut().push(format!("error:{s}")));
    }
    fn roots(&self) -> &[ElemHandle] {
        &self.roots
    }
}

fn intro() -> Box<dyn crate::Screen> {
    Box::new(StubScreen {
        name: "intro",
        ..StubScreen::default()
    })
}
fn charmgmt() -> Box<dyn crate::Screen> {
    Box::new(StubScreen {
        name: "charmgmt",
        ..StubScreen::default()
    })
}
fn gameplay() -> Box<dyn crate::Screen> {
    Box::new(StubScreen {
        name: "gameplay",
        ..StubScreen::default()
    })
}

/// Oracle: registering three stub screens and queueing a mode switches exactly once, on the tick,
/// after every other listener, and destroys the previous screen's roots and dialogs.
#[test]
fn a_queued_mode_switches_exactly_once_on_the_tick_and_destroys_the_old_screen() {
    SCREEN_LOG.with(|l| l.borrow_mut().clear());
    let mut ui = UiSystem::new((800, 600));
    let mut flow = crate::UiFlow::new();
    assert!(flow.register(crate::framework::mode::INTRO, intro));
    assert!(flow.register(crate::framework::mode::CHARACTER_MANAGEMENT, charmgmt));
    assert!(flow.register(crate::framework::mode::GAME_PLAY, gameplay));
    assert!(
        !flow.register(crate::framework::mode::INTRO, intro),
        "duplicate ids are rejected, matching the original table insertion"
    );

    flow.queue(crate::framework::mode::INTRO);
    assert!(flow.current().is_none(), "queueing alone changes nothing");
    flow.frame(
        &mut crate::framework::ScreenCx::new(&mut ui),
        LocalTime(0.0),
        &mut NullInputPump,
    );
    assert_eq!(flow.current_mode(), Some(crate::framework::mode::INTRO));
    let intro_root = flow.current().unwrap().roots()[0];
    assert!(ui.is_alive(intro_root));

    // A second frame with nothing queued does not switch again.
    let before = flow.switches;
    flow.frame(
        &mut crate::framework::ScreenCx::new(&mut ui),
        LocalTime(1.0),
        &mut NullInputPump,
    );
    assert_eq!(flow.switches, before);

    let mut data = crate::PropertyCollection::new();
    data.set(crate::props::attr::DIALOG_KIND, PropertyValue::Integer(1));
    let ctx = ui.dialogs.make_dialog(data, 1.0).unwrap();
    assert_eq!(
        ui.dialogs.pending_create(),
        vec![(ctx, crate::dialog::DialogKind::Confirmation)],
        "the dialog's element half has not run: the host still owes this context an element"
    );
    let dl = layout(0x2100_003C, vec![desc(0x15, 0x13, 0, 0, 400, 95)]);
    let dialog_root = build(&mut ui, &dl, ElementId(0x15));
    if let Some(n) = ui.node_mut(dialog_root) {
        n.flags.set_is_root_element(true);
    }
    assert!(
        crate::dialog::types::dialog_element(&ui, dialog_root).is_some(),
        "the registered ctor for element type 0x13 gives a real DialogElement"
    );
    assert!(
        ui.bind_dialog_element(ctx, dialog_root),
        "the context took the element"
    );
    assert!(
        ui.dialogs.pending_create().is_empty(),
        "and nothing is owed one now"
    );
    assert!(ui.dialogs.is_dialog_open(0));

    flow.queue(crate::framework::mode::CHARACTER_MANAGEMENT);
    flow.queue(crate::framework::mode::GAME_PLAY); // "queueing twice keeps only the last"
    flow.frame(
        &mut crate::framework::ScreenCx::new(&mut ui),
        LocalTime(2.0),
        &mut NullInputPump,
    );
    assert_eq!(flow.current_mode(), Some(crate::framework::mode::GAME_PLAY));
    assert_eq!(flow.switches, before + 1, "exactly one switch");
    assert!(
        !ui.is_alive(intro_root),
        "the previous screen's roots are gone"
    );
    assert!(
        !ui.is_alive(dialog_root),
        "framework destruction resets the dialog factory"
    );
    assert!(!ui.dialogs.is_dialog_open(0));

    let log = SCREEN_LOG.with(|l| l.borrow().clone());
    assert_eq!(
        log,
        vec!["create:intro", "destroy:intro", "create:gameplay"]
    );
}

/// Oracle: `use_new_mode`'s early return for an unregistered id — queueing one would leave
/// the current screen null.
#[test]
fn an_unregistered_mode_is_silently_dropped() {
    SCREEN_LOG.with(|l| l.borrow_mut().clear());
    let mut ui = UiSystem::new((800, 600));
    let mut flow = crate::UiFlow::new();
    flow.register(crate::framework::mode::INTRO, intro);
    flow.queue(crate::UiMode(0x1000_0004)); // not registered in this build
    flow.frame(
        &mut crate::framework::ScreenCx::new(&mut ui),
        LocalTime(0.0),
        &mut NullInputPump,
    );
    assert!(flow.current().is_none());
    assert_eq!(flow.switches, 0);
}

/// Oracle: the flow's queue-with-error and `use_new_mode` handing a valid pending error to the
/// new screen **before** showing it.
#[test]
fn a_queued_error_reaches_the_new_screen_before_it_is_shown() {
    SCREEN_LOG.with(|l| l.borrow_mut().clear());
    let mut ui = UiSystem::new((800, 600));
    let mut flow = crate::UiFlow::new();
    flow.register(crate::framework::mode::DISCONNECTED, intro);
    flow.queue_with_error(
        crate::framework::mode::DISCONNECTED,
        "ID_NetErr_ConnectionLost".into(),
    );
    flow.frame(
        &mut crate::framework::ScreenCx::new(&mut ui),
        LocalTime(0.0),
        &mut NullInputPump,
    );
    let log = SCREEN_LOG.with(|l| l.borrow().clone());
    assert_eq!(log, vec!["error:ID_NetErr_ConnectionLost", "create:intro"]);
    // The text is consumed: the next switch gets none.
    SCREEN_LOG.with(|l| l.borrow_mut().clear());
    flow.queue(crate::framework::mode::DISCONNECTED);
    flow.frame(
        &mut crate::framework::ScreenCx::new(&mut ui),
        LocalTime(1.0),
        &mut NullInputPump,
    );
    let log = SCREEN_LOG.with(|l| l.borrow().clone());
    assert_eq!(log, vec!["destroy:intro", "create:intro"]);
}

// ---------------------------------------------------------------------------------------------
// tooltips
// ---------------------------------------------------------------------------------------------

fn tooltip_window(id: u32, text_id: u32) -> ElementDesc {
    let mut d = desc(id, 3, 0, 0, 30, 30);
    d.base.properties.set(
        crate::props::attr::TOOLTIP_TEXT_CHILD,
        PropertyValue::Enum(text_id),
    );
    d.children
        .insert(ElementId(text_id), desc(text_id, 0x0C, 2, 2, 26, 26));
    d
}

/// An owner that asks for a tooltip, the way a shipped element does: `0x4B` on, `0x47` naming the
/// window. `0x48` is deliberately left out so the element's own-layout fallback is the
/// path under test.
fn tooltipped(id: u32, window: u32) -> ElementDesc {
    let mut d = desc(id, 3, 0, 0, 100, 100);
    d.base
        .properties
        .set(crate::props::attr::TOOLTIP_ON, PropertyValue::Bool(true));
    d.base.properties.set(
        crate::props::attr::TOOLTIP_ELEMENT,
        PropertyValue::Enum(window),
    );
    d
}

/// A tooltip waits for its delay and a per element override wins.
#[test]
fn a_tooltip_waits_for_its_delay_and_a_per_element_override_wins() {
    let mut ui = UiSystem::new((800, 600));
    let l = layout(0x2100_0001, vec![tooltipped(1, 2), tooltip_window(2, 3)]);
    ui.lib.insert(l.clone());
    ui.assets = Some(Rc::new(NoAssets));
    let h = build(&mut ui, &l, ElementId(1));
    ui.initialize_tree(h);
    ui.set_mouse_visible(h, true);
    ui.set_tooltip(h, Some("A sturdy shield.".into()));
    ui.tooltip.delay = 1.0;

    ui.mouse_move(LocalTime(0.0), 50, 50);
    ui.check_tooltip(LocalTime(0.5));
    assert!(ui.tooltip.element.is_none(), "not before the delay");
    ui.check_tooltip(LocalTime(1.5));
    let tip = ui.tooltip.element.expect("and after it");

    // The two halves `create_hollow` could never satisfy: a box, and glyphs inside it.
    let box_ = ui.screen_box(tip);
    assert!(
        box_.width() > 0 && box_.height() > 0,
        "the tooltip is a zero-size invisible box: {box_:?}"
    );
    let mut back = crate::RecordingDrawBackend::default();
    ui.draw(&mut back);
    let glyphs: String = back
        .calls
        .iter()
        .filter(|c| box_.contains(c.screen.x0, c.screen.y0))
        .flat_map(|c| {
            c.glyphs
                .iter()
                .map(|g| char::from_u32(u32::from(g.ch)).unwrap_or('?'))
        })
        .collect();
    assert_eq!(glyphs, "A sturdy shield.", "the tooltip drew no text");

    // A per-element override of 0x50 wins over the global preference.
    let mut ui = UiSystem::new((800, 600));
    let mut d = tooltipped(1, 2);
    d.base
        .properties
        .set(crate::props::attr::TOOLTIP_DELAY, PropertyValue::Float(5.0));
    let l = layout(0x2100_0001, vec![d, tooltip_window(2, 3)]);
    ui.lib.insert(l.clone());
    ui.assets = Some(Rc::new(NoAssets));
    let h = build(&mut ui, &l, ElementId(1));
    ui.initialize_tree(h);
    ui.set_mouse_visible(h, true);
    ui.set_tooltip(h, Some("Slow.".into()));
    ui.tooltip.delay = 0.1;
    ui.mouse_move(LocalTime(0.0), 50, 50);
    ui.check_tooltip(LocalTime(1.0));
    assert!(
        ui.tooltip.element.is_none(),
        "the 0x50 override of 5s wins over the 0.1s preference"
    );
    ui.check_tooltip(LocalTime(6.0));
    assert!(ui.tooltip.element.is_some());
}

/// An element's authored tooltip entry is shown with no call to set tooltip.
#[test]
fn an_elements_authored_tooltip_entry_is_shown_with_no_call_to_set_tooltip() {
    let mut ui = UiSystem::new((800, 600));
    let mut d = tooltipped(1, 2);
    d.base.properties.set(
        crate::props::attr::TOOLTIP_ENTRY,
        PropertyValue::StringInfo(Box::new(dereth_assets::ui::StringInfo {
            override_flag: 1,
            literal: Some("Log out".into()),
            string_id: None,
            table_id: None,
            is_adder: 0,
            adder: None,
            variables: Vec::new(),
        })),
    );
    let l = layout(0x2100_0001, vec![d, tooltip_window(2, 3)]);
    ui.lib.insert(l.clone());
    ui.assets = Some(Rc::new(NoAssets));
    let h = build(&mut ui, &l, ElementId(1));
    ui.initialize_tree(h);
    ui.set_mouse_visible(h, true);
    assert!(
        ui.node(h).expect("alive").tooltip_text.is_none(),
        "nothing called set_tooltip"
    );

    ui.mouse_move(LocalTime(0.0), 50, 50);
    ui.check_tooltip(LocalTime(1.0));
    let tip = ui.tooltip.element.expect("the authored 0x49 is a tooltip");
    let box_ = ui.screen_box(tip);
    assert!(box_.width() > 0 && box_.height() > 0, "zero-size: {box_:?}");
    let mut back = crate::RecordingDrawBackend::default();
    ui.draw(&mut back);
    let glyphs: String = back
        .calls
        .iter()
        .filter(|c| box_.contains(c.screen.x0, c.screen.y0))
        .flat_map(|c| {
            c.glyphs
                .iter()
                .map(|g| char::from_u32(u32::from(g.ch)).unwrap_or('?'))
        })
        .collect();
    assert_eq!(glyphs, "Log out");
}

/// The tooltip sits thirty two pixels off the pointer and is clamped to the display.
#[test]
fn the_tooltip_sits_thirty_two_pixels_off_the_pointer_and_is_clamped_to_the_display() {
    let make = || {
        let mut ui = UiSystem::new((800, 600));
        let l = layout(0x2100_0001, vec![tooltipped(1, 2), tooltip_window(2, 3)]);
        ui.lib.insert(l.clone());
        ui.assets = Some(Rc::new(NoAssets));
        let h = build(&mut ui, &l, ElementId(1));
        ui.initialize_tree(h);
        ui.set_mouse_visible(h, true);
        ui.set_tooltip(h, Some("Hi".into()));
        ui.tooltip.delay = 0.0;
        (ui, h)
    };

    let (mut ui, _h) = make();
    ui.mouse_move(LocalTime(0.0), 10, 10);
    ui.check_tooltip(LocalTime(1.0));
    let b = ui.screen_box(ui.tooltip.element.expect("a tooltip"));
    assert_eq!(
        (b.x0, b.y0),
        (42, 42),
        "the client offsets the tooltip by +0x20 in both axes"
    );

    // Against the right/bottom edge the two `min`s slide it back on screen instead of off it.
    let (mut ui, h) = make();
    ui.move_to(h, 700, 500);
    ui.mouse_move(LocalTime(0.0), 799, 599);
    ui.check_tooltip(LocalTime(1.0));
    let b = ui.screen_box(ui.tooltip.element.expect("a tooltip"));
    assert_eq!(
        (b.x1, b.y1),
        (799, 599),
        "clamped to display width - width / display height - height, not pushed off the edge"
    );
}

/// Writing the disabled attribute does not stop a button being hit testable.
#[test]
fn writing_the_disabled_attribute_does_not_stop_a_button_being_hit_testable() {
    let mut ui = UiSystem::new((800, 600));
    let l = layout(0x2100_0001, vec![desc(1, ty::BUTTON.0, 0, 0, 40, 20)]);
    let h = build(&mut ui, &l, ElementId(1));
    ui.initialize_tree(h);
    assert!(
        ui.node(h).unwrap().is_mouse_visible,
        "a button is always mouse-visible"
    );

    for v in [false, true, false] {
        ui.set_attribute_bool(h, crate::props::attr::DISABLED, v);
        assert!(
            ui.node(h).unwrap().is_mouse_visible,
            "writing Disabled={v} must not clear the mouse-visible flag"
        );
    }
}

/// Oracle: the button state setter, where state 0 **enables** a button.
///
/// The shipped `chargen_master` layout ships *Help*, *Exit* and *Random* with `Disabled = true` and
/// the character-generation main screen never enables them; retail's buttons work because initialization step 3's
/// setting the description's default state contradicts the attribute and this override answers by
/// clearing the **attribute** instead of setting the state.
#[test]
fn a_button_whose_layout_ships_it_disabled_is_enabled_by_its_own_set_state() {
    let mut ui = UiSystem::new((800, 600));
    let mut d = desc(1, ty::BUTTON.0, 0, 0, 40, 20);
    d.base
        .properties
        .set(crate::props::attr::DISABLED, PropertyValue::Bool(true));
    d.default_state = StateId(1);
    let l = layout(0x2100_0001, vec![d]);
    let h = build(&mut ui, &l, ElementId(1));
    ui.initialize_tree(h);

    assert_eq!(
        ui.node(h)
            .unwrap()
            .merged_properties()
            .get_bool(crate::props::attr::DISABLED),
        Some(false),
        "Initialize's SetState(default) cleared the attribute the layout shipped"
    );
    assert_ne!(
        ui.node(h).unwrap().state,
        StateId(0x0D),
        "and it is not in the disabled state"
    );
}

// ---------------------------------------------------------------------------------------------
// Nested child SetState calls must complete before the parent's next call.
// ---------------------------------------------------------------------------------------------

/// The group box's attribute setter sets the old child's state to 1
/// before setting the new child's state to 6, including when old and new are the same child.
/// The button's nested attribute handling and state update must finish each call synchronously.
#[test]
fn nested_child_restates_finish_before_the_parent_makes_its_next_state_call() {
    #[derive(Debug)]
    struct Parent {
        child: ElemHandle,
    }
    impl Element for Parent {
        fn listen_to_element_message(&mut self, ctx: &mut ElemCtx<'_>, _: &ElementMessage) -> R {
            // A parent's own queued restate is not safe until its behaviour is restored.
            ctx.ui.queue_set_state(ctx.me, StateId(6));
            for state in [StateId(1), StateId(6), StateId(1), StateId(6)] {
                ctx.ui.set_state(self.child, state);
                assert_eq!(
                    ctx.ui.node(self.child).unwrap().state,
                    state,
                    "the child's deferred state update completed before the state set returned"
                );
                assert_eq!(
                    ctx.ui.node(ctx.me).unwrap().state,
                    StateId(1),
                    "the still-lifted parent's own restate remains deferred"
                );
            }
            R::Default
        }
    }
    let mut ui = UiSystem::new((800, 600));
    let mut parent = desc(1, ty::FIELD.0, 0, 0, 100, 50);
    let mut button = desc(2, ty::BUTTON.0, 0, 0, 40, 20);
    button
        .base
        .properties
        .set(crate::props::attr::TOGGLE_BUTTON, PropertyValue::Bool(true));
    for d in [&mut parent, &mut button] {
        d.default_state = StateId(1);
        for s in [StateId(1), StateId(6)] {
            d.states.insert(
                s,
                StateDesc {
                    state_id: s,
                    ..StateDesc::default()
                },
            );
        }
    }
    parent.children.insert(button.element_id, button);
    let l = layout(0x2100_0001, vec![parent]);
    let parent = build(&mut ui, &l, ElementId(1));
    ui.initialize_tree(parent);
    let child = ui.get_child(parent, ElementId(2)).unwrap();
    ui.set_state(child, StateId(6));
    ui.node_mut(parent).unwrap().behaviour = Some(Box::new(Parent { child }));
    ui.broadcast_element_message(parent, MessageId(0xA57A), 0, 0);
    assert_eq!(ui.node(parent).unwrap().state, StateId(6));
    assert_eq!(ui.node(child).unwrap().state, StateId(6));
    assert!(
        ui.deferred_states.is_empty(),
        "all restates drained exactly once"
    );
}

/// The shipped stack-splitter slider's own geometry, read off the live `classic_gameplay` tree:
/// `0x100001A4` is 90 x 14 with a 16 x 14 thumb (element id 1) and no arrows.
fn splitter_slider(ui: &mut UiSystem) -> (ElemHandle, ElemHandle) {
    let mut bar = desc(0x100, ty::SCROLLBAR.0, 0, 0, 90, 14);
    bar.children
        .insert(ElementId(1), desc(1, ty::FIELD.0, 0, 0, 16, 14));
    let l = layout(0x2100_0001, vec![bar]);
    let h = build(ui, &l, ElementId(0x100));
    let thumb = ui
        .get_child(h, ElementId(1))
        .expect("the thumb is child id 1");
    (h, thumb)
}

fn horizontal_bar(
    ui: &mut UiSystem,
    h: ElemHandle,
    thumb: ElemHandle,
) -> crate::widgets::scrollbar::Scrollbar {
    use crate::widgets::scrollbar::{bits, Scrollbar};
    let mut sb = Scrollbar {
        bits: bits::HORIZONTAL,
        widget: Some(thumb),
        ..Scrollbar::default()
    };
    sb.update_scrolling_area(ui, h);
    sb
}

/// The scrollbar's travel is the track less the thumb.
#[test]
fn the_scrollbars_travel_is_the_track_less_the_thumb() {
    let mut ui = UiSystem::new((800, 600));
    let (h, thumb) = splitter_slider(&mut ui);
    let sb = horizontal_bar(&mut ui, h, thumb);
    // No arrow buttons on this bar, so the track is the whole box.
    assert_eq!(
        sb.scrolling_area,
        Box2D {
            x0: 0,
            y0: 0,
            x1: 90,
            y1: 14
        }
    );
    assert_eq!(
        sb.active_size(&ui, h),
        (90 - 16, 14 - 14),
        "the thumb is subtracted from both axes"
    );
}

/// Oracle: the scrollbar's point-to-position and its position-to-thumb-origin.
///
/// **The invisible-wrong case, in full.** A slider that moves by the wrong amount looks entirely
/// plausible; every number below is evaluated, both directions are asserted, and the two functions
/// are checked to be each other's inverse — which is what pins the position-to-thumb-origin
/// truncation's operand to `position · (span − 1)`.
#[test]
fn a_point_becomes_a_position_and_the_position_puts_the_thumb_back_under_it() {
    let mut ui = UiSystem::new((800, 600));
    let (h, thumb) = splitter_slider(&mut ui);
    let sb = horizontal_bar(&mut ui, h, thumb);
    // active width 74, so the divisor is 73 and the half-thumb offset is 8.
    let at = |x: i32| sb.point_to_position(&ui, h, x, 6);
    assert!(
        (at(8) - 0.0).abs() < 1e-6,
        "the pointer over the thumb's centre at rest is 0"
    );
    assert!(
        (at(8 + 73) - 1.0).abs() < 1e-6,
        "and at the far end it is 1"
    );
    assert!((at(8 + 36) - 36.0 / 73.0).abs() < 1e-6);

    // The clamp, both ends, and past both ends.
    assert_eq!(at(0), 0.0, "left of the track clamps to 0");
    assert_eq!(at(-500), 0.0);
    assert_eq!(at(89), 1.0, "right of the track clamps to 1");
    assert_eq!(at(5000), 1.0);

    // …and the inverse.
    assert_eq!(sb.position_to_widget_x0y0(&ui, h, 0.0), (0, 0));
    assert_eq!(sb.position_to_widget_x0y0(&ui, h, 1.0), (73, 0));
    assert_eq!(sb.position_to_widget_x0y0(&ui, h, 0.5), (36, 0));
    // Round trip: a thumb put at a position reports that position back from its own centre.
    for p in [0.0_f32, 0.25, 0.5, 0.75, 1.0] {
        let (x, _) = sb.position_to_widget_x0y0(&ui, h, p);
        let back = at(x + 8);
        assert!((back - p).abs() <= 1.0 / 73.0, "{p} -> x={x} -> {back}");
    }
}

/// Oracle: the barber panel's `0x10000321` message arm computes
/// `f = p1 (as unsigned) * 0.001`, which is where `p1`'s scale comes
/// from. `p2` is `-1` when the bar has no stop locations.
///
/// The thumb moving is asserted with it, because the position is only worth anything if the picture
/// follows: before this unit `message 0x0A` could not carry a non-zero position at all.
#[test]
fn setting_the_position_moves_the_thumb_and_raises_0x0a_with_the_position_times_1000() {
    let mut ui = UiSystem::new((800, 600));
    let (h, thumb) = splitter_slider(&mut ui);
    let mut sb = horizontal_bar(&mut ui, h, thumb);
    let who = ListenerId::External(1);
    ui.register_for_element_message(ElementId(0x100), msgid::SCROLL_POSITION, who);
    ui.drain_outbox();

    let seen = |ui: &mut UiSystem| -> Vec<(u32, u32)> {
        ui.drain_outbox()
            .into_iter()
            .filter_map(|d| match d {
                crate::Delivery::Element { msg, .. } if msg.id == msgid::SCROLL_POSITION => {
                    Some((msg.p1, msg.p2))
                }
                _ => None,
            })
            .collect()
    };

    sb.set_scrollbar_position(&mut ui, h, 0.5);
    assert_eq!(
        seen(&mut ui),
        vec![(500, u32::MAX)],
        "position x 1000, and no stop"
    );
    assert!((crate::widgets::scrollbar::Scrollbar::position(&ui, h) - 0.5).abs() < 1e-6);
    assert_eq!(
        ui.node(thumb).expect("alive").region.box_.x0,
        36,
        "the thumb followed"
    );

    // The clamp is the client's own `if (pos < 0) pos = 0; else if (pos > 1) pos = 1;`.
    sb.set_scrollbar_position(&mut ui, h, 4.0);
    assert_eq!(seen(&mut ui), vec![(1000, u32::MAX)]);
    assert_eq!(
        ui.node(thumb).expect("alive").region.box_.x0,
        73,
        "hard against the far end"
    );
    sb.set_scrollbar_position(&mut ui, h, -4.0);
    assert_eq!(seen(&mut ui), vec![(0, u32::MAX)]);
    assert_eq!(ui.node(thumb).expect("alive").region.box_.x0, 0);
}

/// Oracle: stop-based scrollbars snap through both branches of the clamping and stop-selection
/// behavior.
///
/// A bar with stop locations does not take arbitrary positions: `set_scrollbar_position` routes
/// through `set_scrollbar_stop`, so `p2` carries the stop and the position snaps.
#[test]
fn a_bar_with_stop_locations_snaps_to_them_and_reports_the_stop() {
    let mut ui = UiSystem::new((800, 600));
    let (h, thumb) = splitter_slider(&mut ui);
    let mut sb = horizontal_bar(&mut ui, h, thumb);
    use crate::widgets::scrollbar::{attr, bits, Scrollbar};

    // Five stops at the edges: 0, 0.25, 0.5, 0.75, 1.
    ui.set_attribute_int(h, attr::STOP_COUNT, 5);
    sb.bits |= bits::HAS_STOPS;
    for (stop, want) in [(0, 0.0_f32), (1, 0.25), (2, 0.5), (4, 1.0)] {
        assert!(
            (Scrollbar::stop_to_position(&ui, h, stop) - want).abs() < 1e-6,
            "stop {stop}"
        );
    }
    assert_eq!(
        Scrollbar::stop_to_position(&ui, h, 99),
        1.0,
        "clamped to n - 1"
    );
    assert_eq!(
        Scrollbar::position_to_stop(&ui, h, 0.6),
        2,
        "0.6 x 4 = 2.4, nearest stop 2"
    );
    assert_eq!(
        Scrollbar::position_to_stop(&ui, h, 0.63),
        3,
        "0.63 x 4 = 2.52, nearest stop 3"
    );
    assert!(
        (sb.validate_position(&ui, h, 0.6) - 0.5).abs() < 1e-6,
        "it snaps"
    );

    let who = ListenerId::External(1);
    ui.register_for_element_message(ElementId(0x100), msgid::SCROLL_POSITION, who);
    ui.drain_outbox();
    sb.set_scrollbar_position(&mut ui, h, 0.6);
    let seen: Vec<(u32, u32)> = ui
        .drain_outbox()
        .into_iter()
        .filter_map(|d| match d {
            crate::Delivery::Element { msg, .. } if msg.id == msgid::SCROLL_POSITION => {
                Some((msg.p1, msg.p2))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        seen,
        vec![(500, 2)],
        "snapped to stop 2, which is position 0.5"
    );

    // Centred stops put each stop in the middle of its band instead: 0.1, 0.3, 0.5, 0.7, 0.9.
    ui.set_attribute_bool(h, attr::CENTRED_STOPS, true);
    assert!((Scrollbar::stop_to_position(&ui, h, 0) - 0.1).abs() < 1e-6);
    assert!((Scrollbar::stop_to_position(&ui, h, 4) - 0.9).abs() < 1e-6);
    assert_eq!(
        Scrollbar::position_to_stop(&ui, h, 0.35),
        1,
        "0.35 x 5 = 1.75, truncated"
    );
}

/// Oracle: the client's proportional arm, on the shipped chat scrollbar's own
/// numbers: `0x10000012` is 16 x 73 with 16 x 16 arrows at both ends and `0x89` (minimum thumb
/// length) = 16.
///
/// Cheap tier — that a thumb appears in the track at all. Before this unit the chat window drew an
/// empty groove: nothing ever sized or moved the thumb child.
#[test]
fn a_proportional_bar_sizes_its_thumb_to_the_fraction_it_is_given() {
    use crate::widgets::scrollbar::{attr, bits, Scrollbar};
    let mut ui = UiSystem::new((800, 600));
    let mut bar = desc(0x100, ty::SCROLLBAR.0, 0, 0, 16, 73);
    bar.children
        .insert(ElementId(1), desc(1, ty::BUTTON.0, 0, 0, 16, 16));
    bar.children.insert(
        ElementId(0x1000_0071),
        desc(0x1000_0071, ty::BUTTON.0, 0, 0, 16, 16),
    );
    bar.children.insert(
        ElementId(0x1000_0072),
        desc(0x1000_0072, ty::BUTTON.0, 0, 57, 16, 16),
    );
    let l = layout(0x2100_0001, vec![bar]);
    let h = build(&mut ui, &l, ElementId(0x100));
    let thumb = ui.get_child(h, ElementId(1)).expect("the thumb");

    let mut sb = Scrollbar {
        bits: bits::PROPORTIONAL,
        widget: Some(thumb),
        increment_button: Some(ElementId(0x1000_0072)),
        decrement_button: Some(ElementId(0x1000_0071)),
        ..Scrollbar::default()
    };
    ui.set_attribute_int(h, attr::MIN_WIDGET_SIZE, 16);
    sb.update_layout(&mut ui, h);
    // The track is the box less the two arrows: 73 - 16 - 16 = 41 tall, starting at y 16. The
    // horizontal half of the rectangle goes nonsensical on a vertical bar (`x1 < x0`) and is
    // never read; that is the client's own arithmetic, applied to both axes unconditionally.
    assert_eq!(
        sb.scrolling_area,
        Box2D {
            x0: 16,
            y0: 16,
            x1: 0,
            y1: 57
        }
    );
    let box_ = ui.node(thumb).expect("alive").region.box_;
    assert_eq!(
        (box_.y0, box_.height()),
        (16, 41),
        "nothing to scroll: the thumb fills the track"
    );

    // A quarter of the content visible: a quarter-length thumb, and it moves over the rest.
    ui.set_attribute_float(h, attr::PROPORTION, 0.25);
    sb.update_layout(&mut ui, h);
    let box_ = ui.node(thumb).expect("alive").region.box_;
    assert_eq!(
        box_.height(),
        16,
        "0.25 x 41 = 10, lifted to the layout's minimum of 16"
    );
    ui.set_attribute_float(h, attr::PROPORTION, 0.5);
    sb.update_layout(&mut ui, h);
    let box_ = ui.node(thumb).expect("alive").region.box_;
    assert_eq!(box_.height(), 20, "0.5 x 41");
    sb.set_scrollbar_position(&mut ui, h, 1.0);
    let box_ = ui.node(thumb).expect("alive").region.box_;
    // 41 − 20 = 21 of travel, so the last reachable origin is 20 past the top of the track.
    assert_eq!(
        box_.y0,
        16 + 20,
        "the far end of a 20-long thumb in a 41-long track"
    );
}

/// Pinned behavior: the bar broadcasts element message `0x0E` for `delta < 1`, else `0x0D`, with
/// `p1 = delta` and `p2 = 0`, when it has no stop locations, and moves to stop `stop + delta` when
/// it has.
///
/// The first arm is why an arrow on an unwired list "does nothing": the bar reports the gesture and
/// its **owner** is what scrolls. That is the client's design, not a gap in this widget.
#[test]
fn an_arrow_moves_a_stop_when_there_are_stops_and_otherwise_only_reports_the_step() {
    use crate::widgets::scrollbar::{attr, bits, Scrollbar};
    let mut ui = UiSystem::new((800, 600));
    let (h, thumb) = splitter_slider(&mut ui);
    let mut sb = horizontal_bar(&mut ui, h, thumb);
    let who = ListenerId::External(1);
    for id in [
        msgid::SCROLL_PAGE_DOWN,
        msgid::SCROLL_PAGE_UP,
        msgid::SCROLL_POSITION,
    ] {
        ui.register_for_element_message(ElementId(0x100), id, who);
    }
    ui.drain_outbox();
    let drain = |ui: &mut UiSystem| -> Vec<(u32, u32)> {
        ui.drain_outbox()
            .into_iter()
            .filter_map(|d| match d {
                crate::Delivery::Element { msg, .. } => Some((msg.id.0, msg.p1)),
                _ => None,
            })
            .collect()
    };

    sb.handle_move_steps(&mut ui, h, 1);
    assert_eq!(
        drain(&mut ui),
        vec![(0x0D, 1)],
        "no stops: the step is reported, not taken"
    );
    sb.handle_move_steps(&mut ui, h, -1);
    assert_eq!(drain(&mut ui), vec![(0x0E, 0xFFFF_FFFF)]);
    assert_eq!(
        Scrollbar::position(&ui, h),
        0.0,
        "and the bar did not move itself"
    );

    // With stops, the same arrow walks them.
    ui.set_attribute_int(h, attr::STOP_COUNT, 5);
    ui.set_attribute_int(h, attr::STOP, 0);
    sb.bits |= bits::HAS_STOPS;
    ui.drain_outbox();
    sb.handle_move_steps(&mut ui, h, 1);
    assert_eq!(
        drain(&mut ui),
        vec![(0x0A, 250)],
        "stop 1 of 5 is position 0.25"
    );
    sb.handle_move_steps(&mut ui, h, 1);
    assert_eq!(drain(&mut ui), vec![(0x0A, 500)]);
}

/// Every engine member of the scrollable subtree takes focus from a press.
#[test]
fn every_engine_member_of_the_scrollable_subtree_takes_focus_from_a_press() {
    use crate::text::TextElement;
    use crate::widgets;

    // text element — and the state bits have nothing to do with it: all three shapes focus.
    let mut t = TextElement::default();
    assert!(
        Element::takes_focus_on_press(&t),
        "a plain label takes focus in retail too"
    );
    t.bits.set_selectable(true);
    assert!(Element::takes_focus_on_press(&t), "a selectable one does");
    t.bits.set_selectable(false);
    t.bits.set_editable(true);
    assert!(Element::takes_focus_on_press(&t), "and so does an edit box");

    // The other four engine members of the scrollable-element subtree.
    assert!(
        Element::takes_focus_on_press(&widgets::button::Button::default()),
        "Button"
    );
    assert!(
        Element::takes_focus_on_press(&widgets::listbox::ListBox::default()),
        "ListBox"
    );
    assert!(
        Element::takes_focus_on_press(&widgets::menu::Menu::default()),
        "Menu"
    );
    assert!(
        Element::takes_focus_on_press(&widgets::scrollbar::Scrollbar::default()),
        "Scrollbar"
    );

    // Everything outside the subtree uses the base element's answer, which never takes
    // focus on a press at all. If this half ever goes green-by-accident the widening has run away.
    assert!(
        !Element::takes_focus_on_press(&crate::element::PlainElement),
        "plain element"
    );
    assert!(
        !Element::takes_focus_on_press(&widgets::field::Field::default()),
        "Field"
    );
    assert!(
        !Element::takes_focus_on_press(&widgets::panel::Panel::default()),
        "Panel"
    );
    assert!(
        !Element::takes_focus_on_press(&widgets::groupbox::GroupBox::default()),
        "GroupBox"
    );
    assert!(
        !Element::takes_focus_on_press(&widgets::meter::Meter::default()),
        "Meter"
    );
    assert!(
        !Element::takes_focus_on_press(&widgets::dragbar::Dragbar::default()),
        "Dragbar"
    );
    assert!(
        !Element::takes_focus_on_press(&widgets::resizebar::Resizebar::default()),
        "Resizebar"
    );
    assert!(
        !Element::takes_focus_on_press(&widgets::colorpicker::ColorPicker::default()),
        "ColorPicker"
    );
}

/// Oracle: the scrollbar's default-hot-click setup, the button's `MouseDown` and
/// the scrollbar's own message handler, `case 2`.
///
/// **A shipped scrollbar layout sets attribute `0x0F` on nothing**, and a button raises the *hot*
/// click — the only message the bar's arrow arm listens for — only when `0x0F` is set. The element
/// sets it on its own children, so without `setup_default_hot_click` an arrow raises `0x01`, the bar
/// has no `0x01` arm, and the arrow is inert however well the rest of the class works.
///
/// Driven as the press does it: the arrow's own `MouseDown` raises `0x02`, and the bar's arm turns
/// it into a step.
#[test]
fn an_arrow_is_given_the_auto_repeat_that_makes_its_press_reach_the_bar() {
    use crate::widgets::scrollbar::attr;
    let mut ui = UiSystem::new((800, 600));
    let mut bar = desc(0x100, ty::SCROLLBAR.0, 0, 0, 16, 73);
    bar.children
        .insert(ElementId(1), desc(1, ty::BUTTON.0, 0, 0, 16, 16));
    bar.children.insert(
        ElementId(0x1000_0071),
        desc(0x1000_0071, ty::BUTTON.0, 0, 0, 16, 16),
    );
    bar.children.insert(
        ElementId(0x1000_0072),
        desc(0x1000_0072, ty::BUTTON.0, 0, 57, 16, 16),
    );
    let l = layout(0x2100_0001, vec![bar]);
    let h = build(&mut ui, &l, ElementId(0x100));
    ui.set_attribute_int(h, attr::STOP_COUNT, 5);
    ui.set_attribute_int(h, attr::STOP, 1);
    // The layout names its arrows; post-initialisation is what acts on the names.
    ui.on_set_attribute(
        h,
        attr::INCREMENT_BUTTON,
        Some(&PropertyValue::Enum(0x1000_0072)),
    );
    ui.on_set_attribute(
        h,
        attr::DECREMENT_BUTTON,
        Some(&PropertyValue::Enum(0x1000_0071)),
    );
    ui.initialize_tree(h);

    let up = ui
        .get_child(h, ElementId(0x1000_0071))
        .expect("the decrement arrow");
    assert_eq!(
        ui.node(up)
            .expect("alive")
            .merged_properties()
            .get_bool(crate::props::attr::HOT_CLICK),
        Some(true),
        "the bar gave its own arrow the auto-repeat"
    );
    // Both intervals, not one: the default hot-click setup writes 0x10 *and* 0x11, and the second
    // was once written and never read. `scrollbar_hold_repeat` pins the values.
    for id in [
        crate::props::attr::HOT_CLICK_FIRST_INTERVAL,
        crate::props::attr::HOT_CLICK_REPEAT_INTERVAL,
    ] {
        assert!(
            ui.node(h)
                .expect("alive")
                .merged_properties()
                .get_float(id)
                .is_some(),
            "and `Initialize` gave the bar itself attribute {id:#04X}, for the page click"
        );
    }

    let who = ListenerId::External(1);
    ui.register_for_element_message(ElementId(0x100), msgid::SCROLL_POSITION, who);
    ui.drain_outbox();

    // The press on the arrow, through the button's own auto-repeat arm.
    ui.broadcast_element_message_at(
        up,
        msgid::MOUSE_PRESS,
        crate::focus::action::PRIMARY_CLICK,
        0,
        crate::msg::MessagePoint::default(),
    );
    let seen: Vec<(u32, u32)> = ui
        .drain_outbox()
        .into_iter()
        .filter_map(|d| match d {
            crate::Delivery::Element { msg, .. } if msg.id == msgid::SCROLL_POSITION => {
                Some((msg.p1, msg.p2))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        seen,
        vec![(0, 0)],
        "the up arrow walked stop 1 back to stop 0, which is position 0"
    );
    assert_eq!(
        ui.node(h)
            .expect("alive")
            .merged_properties()
            .get_int(attr::STOP),
        Some(0),
        "and the stop it landed on was written back"
    );
}

/// Oracle: the element manager's input-action dispatch — the **only** place in the client that
/// touches the element input-action listener table. It looks up the action's listener list and
/// returns false if there is none; otherwise it broadcasts element message `0x31` with
/// `p1 = action`, `p2 = 0` to every non-null listener in the list and returns true.
///
/// Three clauses, and the third is the one a naive wiring gets wrong: **the loop has no break**,
/// so a second element registered for the same action receives it too.
///
/// The message id and the two property numbers are spelled out as **literals** here, because a
/// test that reads a constant through the same symbol it writes it through cannot detect a wrong
/// constant — and this whole mechanism is three numbers (`0x57`, `0x58`, `0x31`).
#[test]
fn an_input_action_raises_0x31_on_every_element_registered_for_it() {
    let mut ui = UiSystem::new((800, 600));
    let mk = |id: u32, action: u32, mode: u32| {
        let mut d = desc(id, 3, 0, 0, 40, 20);
        d.base.properties.set(0x57, PropertyValue::Enum(action));
        d.base.properties.set(0x58, PropertyValue::Enum(mode));
        d
    };
    let l = layout(
        0x2100_0001,
        vec![
            mk(1, 0x1000_001D, 1),
            mk(2, 0x1000_001D, 1),
            mk(3, 0x1000_001E, 1),
            mk(4, 0x1000_0020, 2),
            mk(5, 0x1000_0021, 3),
        ],
    );
    let hs: Vec<ElemHandle> = (1..=5)
        .map(|i| {
            let h = build(&mut ui, &l, ElementId(i));
            // `Initialize` is what dispatches the description's own properties through
            // `on_set_attribute`, and therefore what registers the element for its input action.
            ui.initialize_tree(h);
            h
        })
        .collect();
    for h in &hs {
        ui.set_visible(*h, false);
    }
    let vis = |ui: &UiSystem, h: ElemHandle| ui.node(h).unwrap().region.flags.visible;

    // The client's tail, reached through `key_press` itself.
    let e = crate::focus::InputEvent {
        action: 0x1000_001D,
        start: true,
        x: 0,
        y: 0,
    };
    ui.key_press(&e);
    assert!(vis(&ui, hs[0]), "the first listener");
    assert!(
        vis(&ui, hs[1]),
        "the second listener on the same action — the loop has no break"
    );
    assert!(
        !vis(&ui, hs[2]),
        "an element on a different action must not fire"
    );

    ui.key_press(&e);
    assert!(
        !vis(&ui, hs[0]),
        "0x58 = 1 is toggle: the second press closes it"
    );
    assert!(!vis(&ui, hs[1]), "…and closes the second listener too");

    // The return value is "the action had a bucket", not "somebody consumed it".
    assert!(ui.dispatch_input_action(0x1000_001D));
    assert!(!ui.dispatch_input_action(0xDEAD_BEEF));

    // The other two arms of attribute `0x58`: **2 shows, 3 hides**, and each is asserted where the
    // *other* value would have given a different answer — 2 on an element that starts hidden, 3 on
    // one that starts shown.
    assert!(!vis(&ui, hs[3]), "the show arm's element starts hidden");
    ui.dispatch_input_action(0x1000_0020);
    assert!(vis(&ui, hs[3]), "0x58 = 2 shows");
    ui.dispatch_input_action(0x1000_0020);
    assert!(vis(&ui, hs[3]), "…and shows again rather than toggling off");
    ui.set_visible(hs[4], true);
    ui.dispatch_input_action(0x1000_0021);
    assert!(!vis(&ui, hs[4]), "0x58 = 3 hides");
    ui.dispatch_input_action(0x1000_0021);
    assert!(
        !vis(&ui, hs[4]),
        "…and stays hidden rather than toggling back on"
    );

    // The numbers, as literals with their source.
    assert_eq!(
        crate::props::attr::INPUT_ACTION,
        0x57,
        "the input-action attribute"
    );
    assert_eq!(
        crate::props::attr::VISIBILITY_TOGGLE_MODE,
        0x58,
        "the visibility-toggle mode"
    );
    assert_eq!(
        msgid::VISIBILITY_TOGGLE,
        MessageId(0x31),
        "the visibility-toggle message"
    );
    for (mode, start, want, why) in [
        (
            1u32,
            true,
            false,
            "mode 1 pushes !((bitfield>>1)&1) — toggle",
        ),
        (1, false, true, "the same arm, from the other side"),
        (2, false, true, "mode 2 pushes 1 — show"),
        (
            2,
            true,
            true,
            "…and a show over a shown element is still shown",
        ),
        (3, true, false, "mode 3 pushes 0 — hide"),
        (
            3,
            false,
            false,
            "…and a hide over a hidden element is still hidden",
        ),
        (4, true, true, "anything that is not 1, 2 or 3 does nothing"),
        (4, false, false, "…in both directions"),
    ] {
        let mut d = desc(9, 3, 0, 0, 10, 10);
        d.base.properties.set(0x58, PropertyValue::Enum(mode));
        let l = layout(0x2100_0002, vec![d]);
        let h = build(&mut ui, &l, ElementId(9));
        ui.set_visible(h, start);
        ui.broadcast_element_message(h, msgid::VISIBILITY_TOGGLE, 0, 0);
        assert_eq!(
            vis(&ui, h),
            want,
            "0x58 = {mode} from visible={start}: {why}"
        );
    }
}

/// Pinned behavior: only a non-zero action with a non-null element is registered, and the
/// element is added to the action's list only if it is not already there.
///
/// Re-setting `0x57` on an element already in the bucket must not put it in twice; the registration
/// used to be a bare `push`, which would have raised `0x31` once per registration and toggled a
/// mode-3 panel straight back off again. Action 0 is refused outright.
#[test]
fn registering_the_same_element_for_an_action_twice_adds_it_once() {
    let mut ui = UiSystem::new((800, 600));
    let mut d = desc(1, 3, 0, 0, 40, 20);
    d.base
        .properties
        .set(0x57, PropertyValue::Enum(0x1000_001D));
    d.base.properties.set(0x58, PropertyValue::Enum(1));
    let l = layout(0x2100_0001, vec![d]);
    let h = build(&mut ui, &l, ElementId(1));
    ui.initialize_tree(h);
    ui.set_visible(h, false);

    // `Initialize` registered it once; write the attribute **once** more.
    //
    // The count has to end up **even** if the de-duplicating add is missing, because `0x58 = 1` toggles: a
    // first draft of this test wrote the attribute twice and could not fail — one toggle and three
    // toggles both leave the panel shown, so the assertion below read the same on a bucket of one
    // and a bucket of three. Two registrations toggle it back off; one leaves it on. The mutation
    // that reverts the registration to a bare `push` survived that draft and kills this one.
    ui.set_attribute_enum(h, 0x57, 0x1000_001D);

    ui.dispatch_input_action(0x1000_001D);
    assert!(
        ui.node(h).unwrap().region.flags.visible,
        "one element, one entry: two `0x31`s would have toggled it straight back off"
    );
    ui.dispatch_input_action(0x1000_001D);
    assert!(!ui.node(h).unwrap().region.flags.visible);
}
