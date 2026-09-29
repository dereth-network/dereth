//! The element description's incorporation (partial-over-base merge) and size-and-position update.
//!
//! Matches the retail client's element-description merge. Four behaviors that affect inheritance
//! and ordering are called out below.

use crate::desc::element_desc::ElementDesc;
use crate::layout::{update_size_and_position, Edges};

/// Update the size and position — transform this node and then hand *its own*
/// before/after boxes to each child, so the whole description tree is re-laid out relative to its
/// own parent. Each child uses **its own** edge modes.
///
/// Alternate states are deliberately not visited: retail walks the children only.
pub fn update_desc_tree(
    d: &mut ElementDesc,
    old_ref: crate::region::Box2D,
    new_ref: crate::region::Box2D,
) {
    let before = d.box_();
    let after = update_size_and_position(before, old_ref, new_ref, d.edges);
    d.base.x = after.x0;
    d.base.y = after.y0;
    d.base.width = after.width();
    d.base.height = after.height();
    for c in d.children.values_mut() {
        update_desc_tree(c, before, after);
    }
}

/// Incorporate — overlay `partial` onto `full` (which is the resolved
/// base), in the client's order.
///
/// Four things the prose does not say and the code does:
///
/// 1. **`ty`, `engine_ty`, `base_element` and `base_layout` are not copied from the
///    partial.** They stay with the base. They must: the partial's `ty` is 0 by definition, and
///    copying it would undo the resolution that just happened.
/// 2. A state or child the partial does not mention keeps the base's, but is then **re-anchored**
///    if the incorporation changed the element's *size* — states with the element's edge modes,
///    children with their own.
/// 3. Children the partial adds get their `read_order` shifted by the number of base children
///    the partial did *not* override, so the merged authoring order stays consistent.
/// 4. `element_id`, `default_state`, the four edges and `read_order` all come from the partial.
pub fn incorporate(full: &mut ElementDesc, partial: &ElementDesc) {
    let old_box = full.box_();

    full.base.incorporate(&partial.base);

    let new_box = full.box_();

    full.element_id = partial.element_id;
    full.default_state = partial.default_state;
    full.edges = partial.edges;
    full.read_order = partial.read_order;

    // ---- states -------------------------------------------------------------------------------
    for (id, s) in &partial.states {
        match full.states.get_mut(id) {
            Some(existing) => existing.incorporate(s),
            None => {
                full.states.insert(*id, s.clone());
            }
        }
    }

    // ---- children -----------------------------------------------------------------------------
    // How many of the base's children the partial does not mention.
    let extra: u32 = full
        .children
        .keys()
        .filter(|k| !partial.children.contains_key(k))
        .count()
        .try_into()
        .unwrap_or(u32::MAX);

    // The partial's children follow its intrusive-hash-table iteration order;
    // a child absent from the base is inserted into the full description's
    // table -- the insert links it at the **head** of its bucket's chain,
    // which is what the front insert into `child_chain` records. The full's table keeps
    // the base's bucket count: assignment resizes to the smallest table prime not below the
    // source's bucket count, i.e. the same prime.
    for id in partial.creation_order() {
        let Some(c) = partial.children.get(&id) else {
            continue;
        };
        match full.children.get_mut(&id) {
            Some(existing) => incorporate(existing, c),
            None => {
                let mut copy = c.clone();
                copy.read_order = copy.read_order.wrapping_add(extra);
                full.children.insert(id, copy);
                full.child_chain.retain(|k| *k != id);
                full.child_chain.insert(0, id);
            }
        }
    }

    // ---- re-anchor what the partial left alone, if the size changed ---------------------------
    if old_box.width() != new_box.width() || old_box.height() != new_box.height() {
        let element_edges: Edges = full.edges;
        for (id, s) in full.states.iter_mut() {
            if partial.states.contains_key(id) {
                continue;
            }
            let r = update_size_and_position(s.box_(), old_box, new_box, element_edges);
            s.x = r.x0;
            s.y = r.y0;
            s.width = r.width();
            s.height = r.height();
        }
        for (id, c) in full.children.iter_mut() {
            if partial.children.contains_key(id) {
                continue;
            }
            update_desc_tree(c, old_box, new_box);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desc::state_desc::{incorporation, StateDesc};
    use crate::layout::EdgeMode;
    use crate::region::Box2D;
    use crate::{ElementId, ElementType, StateId};

    fn desc(ty: u32, x: i32, y: i32, w: i32, h: i32) -> ElementDesc {
        ElementDesc {
            ty: ElementType(ty),
            base: StateDesc {
                incorporation: incorporation::LEGACY_ALL_GEOMETRY,
                x,
                y,
                width: w,
                height: h,
                ..StateDesc::default()
            },
            ..ElementDesc::default()
        }
    }

    /// Incorporation assigns `element_id`,
    /// `default_state`, the four edges and `read_order`, and **nothing else** among the ids.
    #[test]
    fn incorporate_never_clobbers_the_resolved_type() {
        let mut full = desc(3, 0, 0, 100, 50);
        let mut partial = desc(0, 0, 0, 0, 0);
        partial.base.incorporation = 0;
        partial.element_id = ElementId(0x1000_0007);
        partial.default_state = StateId(4);
        partial.base_layout = dereth_primitives::DataId(0x2100_0037);
        partial.base_element = ElementId(9);
        incorporate(&mut full, &partial);
        assert_eq!(full.ty, ElementType(3), "the base's concrete type survives");
        assert_eq!(full.element_id, ElementId(0x1000_0007));
        assert_eq!(full.default_state, StateId(4));
        assert_eq!(
            full.base_layout,
            dereth_primitives::DataId(0),
            "not copied from the partial"
        );
        assert_eq!(
            full.box_(),
            Box2D::from_xywh(0, 0, 100, 50),
            "no geometry bits set"
        );
    }

    /// Retail counts the untouched base children and adds that shift to each new child's read
    /// order.
    #[test]
    fn a_new_child_read_order_is_shifted_past_the_untouched_base_children() {
        let mut full = desc(3, 0, 0, 10, 10);
        for (i, id) in [11_u32, 12, 13].into_iter().enumerate() {
            let mut c = desc(3, 0, 0, 1, 1);
            c.read_order = u32::try_from(i).unwrap();
            full.children.insert(ElementId(id), c);
        }
        let mut partial = desc(0, 0, 0, 0, 0);
        partial.base.incorporation = 0;
        // overrides one existing child (12) and adds one new (99)
        partial.children.insert(ElementId(12), desc(0, 0, 0, 0, 0));
        let mut fresh = desc(3, 0, 0, 1, 1);
        fresh.read_order = 0;
        partial.children.insert(ElementId(99), fresh);
        incorporate(&mut full, &partial);
        assert_eq!(full.children.len(), 4);
        // two base children (11 and 13) were not mentioned, so the new child shifts by 2.
        assert_eq!(full.children[&ElementId(99)].read_order, 2);
    }

    /// When the size changes, children the partial leaves alone
    /// are re-anchored with **their own** edge modes against the element's old and new boxes.
    #[test]
    fn a_size_change_reanchors_the_children_the_partial_left_alone() {
        let mut full = desc(3, 0, 0, 100, 100);
        let mut child = desc(3, 0, 90, 100, 10); // a footer strip
        child.edges = Edges {
            left: EdgeMode::Fixed,
            top: EdgeMode::AnchorEnd,
            right: EdgeMode::AnchorStart,
            bottom: EdgeMode::AnchorStart,
        };
        full.children.insert(ElementId(1), child);

        let mut partial = desc(0, 0, 0, 0, 0);
        partial.base.incorporation = incorporation::WIDTH | incorporation::HEIGHT;
        partial.base.width = 200;
        partial.base.height = 200;
        incorporate(&mut full, &partial);

        assert_eq!(full.box_(), Box2D::from_xywh(0, 0, 200, 200));
        let c = &full.children[&ElementId(1)];
        // top: mode 2 -> y0 += dh (100); right/bottom: mode 1 -> follow the deltas.
        assert_eq!(c.box_(), Box2D::new(0, 190, 199, 199));
    }

    /// The child gets **the element's
    /// own** before/after boxes, not the grandparent's.
    #[test]
    fn the_desc_tree_transform_is_relative_to_each_parent() {
        let mut root = desc(3, 0, 0, 800, 600);
        let mut child = desc(3, 0, 0, 400, 300);
        child.edges = Edges {
            left: EdgeMode::Fixed,
            top: EdgeMode::Fixed,
            right: EdgeMode::AnchorStart,
            bottom: EdgeMode::AnchorStart,
        };
        let mut grand = desc(3, 10, 10, 10, 10);
        // Pinned to the parent's far edge, keeping its size: near edges follow the delta (mode 2),
        // far edges follow it too (mode 1).
        grand.edges = Edges {
            left: EdgeMode::AnchorEnd,
            top: EdgeMode::AnchorEnd,
            right: EdgeMode::AnchorStart,
            bottom: EdgeMode::AnchorStart,
        };
        child.children.insert(ElementId(2), grand);
        root.edges = Edges {
            left: EdgeMode::Fixed,
            top: EdgeMode::Fixed,
            right: EdgeMode::AnchorStart,
            bottom: EdgeMode::AnchorStart,
        };
        root.children.insert(ElementId(1), child);

        update_desc_tree(
            &mut root,
            Box2D::new(0, 0, 799, 599),
            Box2D::new(0, 0, 1599, 599),
        );
        assert_eq!(root.box_().width(), 1600);
        let c = &root.children[&ElementId(1)];
        assert_eq!(
            c.box_().width(),
            1200,
            "child grows by the root's own delta of 800"
        );
        let g = &c.children[&ElementId(2)];
        // the grandchild is pinned to the child's far edge, so it slides by the child's delta.
        assert_eq!(g.box_(), Box2D::from_xywh(810, 10, 10, 10));
    }
}
