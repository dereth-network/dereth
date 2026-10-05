//! Element arena and parent-child ownership.

use crate::{ElemHandle, ElementId, ElementNode, Slot, UiSystem};
use std::collections::BTreeMap;

impl UiSystem {
    // ---- arena ----------------------------------------------------------------------------

    #[must_use]
    pub fn node(&self, h: ElemHandle) -> Option<&ElementNode> {
        let s = self.slots.get(h.index())?;
        if s.generation == h.generation() {
            s.node.as_ref()
        } else {
            None
        }
    }

    pub fn node_mut(&mut self, h: ElemHandle) -> Option<&mut ElementNode> {
        let s = self.slots.get_mut(h.index())?;
        if s.generation == h.generation() {
            s.node.as_mut()
        } else {
            None
        }
    }

    #[must_use]
    pub fn is_alive(&self, h: ElemHandle) -> bool {
        self.node(h).is_some()
    }

    pub(crate) fn alloc(&mut self, node: ElementNode) -> ElemHandle {
        if let Some(i) = self.free.pop() {
            let idx = i as usize;
            self.slots[idx].node = Some(node);
            let h = ElemHandle::new(i, self.slots[idx].generation);
            self.element_list.push(h);
            h
        } else {
            // The only `expect` in this crate's library code. It guards an internal capacity
            // invariant, not input: the handle's index field is 20 bits (1 048 575 live elements)
            // and the retail dat ships 2 162 element *descriptions* in total. Saturating instead
            // would hand out a colliding handle and corrupt the tree silently, which is strictly
            // worse than stopping.
            let i = u32::try_from(self.slots.len())
                .expect("the element arena exceeded 2^32 slots, which cannot happen");
            self.slots.push(Slot {
                generation: 0,
                node: Some(node),
            });
            let h = ElemHandle::new(i, 0);
            self.element_list.push(h);
            h
        }
    }

    /// Behavior: a **linear** search returning the first match.
    /// Kept linear on purpose: the id-uniqueness contract in the doc comment on [`ElementId`]
    /// exists precisely because this is what the client does.
    #[must_use]
    pub fn get_element(&self, id: ElementId) -> Option<ElemHandle> {
        self.element_list
            .iter()
            .copied()
            .find(|h| self.node(*h).is_some_and(|n| n.element_id() == id))
    }

    /// Every live element, in creation order.
    #[must_use]
    pub fn element_list(&self) -> &[ElemHandle] {
        &self.element_list
    }

    // ---- tree -----------------------------------------------------------------------------

    #[must_use]
    pub fn parent(&self, h: ElemHandle) -> Option<ElemHandle> {
        self.node(h).and_then(|n| n.region.parent)
    }

    #[must_use]
    pub fn children(&self, h: ElemHandle) -> Vec<ElemHandle> {
        self.node(h)
            .map(|n| n.region.children.clone())
            .unwrap_or_default()
    }

    /// The element's direct-child lookup.
    #[must_use]
    pub fn get_child(&self, h: ElemHandle, id: ElementId) -> Option<ElemHandle> {
        self.children(h)
            .into_iter()
            .find(|c| self.node(*c).is_some_and(|n| n.element_id() == id))
    }

    /// Behavior: depth-first search of the subtree.
    #[must_use]
    pub fn get_child_recursive(&self, h: ElemHandle, id: ElementId) -> Option<ElemHandle> {
        for c in self.children(h) {
            if self.node(c).is_some_and(|n| n.element_id() == id) {
                return Some(c);
            }
            if let Some(found) = self.get_child_recursive(c, id) {
                return Some(found);
            }
        }
        None
    }

    /// Behavior: walk up until an element flagged as a root element.
    #[must_use]
    pub fn root_of(&self, h: ElemHandle) -> Option<ElemHandle> {
        let mut cur = h;
        loop {
            let n = self.node(cur)?;
            if n.flags.is_root_element() {
                return Some(cur);
            }
            cur = n.region.parent?;
        }
    }

    /// The is-ancestor-of-me test.
    #[must_use]
    pub fn is_ancestor_of(&self, ancestor: ElemHandle, of: ElemHandle) -> bool {
        let mut cur = self.parent(of);
        while let Some(c) = cur {
            if c == ancestor {
                return true;
            }
            cur = self.parent(c);
        }
        false
    }

    /// Set the parent: detach from the old parent, add the child to the
    /// new one (which pushes at the tail and re-sorts by `z_level`), then cascade the size,
    /// position and visibility updates down the subtree.
    pub fn set_parent(&mut self, child: ElemHandle, parent: Option<ElemHandle>) {
        if let Some(old) = self.parent(child) {
            if let Some(n) = self.node_mut(old) {
                n.region.remove_child(child);
            }
        }
        if let Some(n) = self.node_mut(child) {
            n.region.parent = parent;
        }
        if let Some(p) = parent {
            // The z-level comparison: `z_level` first, then `read_order` — see
            // [`region::Region::add_child`], which is where the tie-break is justified.
            let key: BTreeMap<ElemHandle, (i32, u32)> = self
                .children(p)
                .into_iter()
                .chain(std::iter::once(child))
                .filter_map(|c| {
                    self.node(c)
                        .map(|n| (c, (n.region.z_level, n.desc.read_order)))
                })
                .collect();
            if let Some(n) = self.node_mut(p) {
                n.region
                    .add_child(child, |h| key.get(&h).copied().unwrap_or((0, 0)));
            }
        }
        self.update_for_parent_size_change(child);
    }

    /// Behavior: **a child-list operation and nothing else.**
    ///
    /// It removes the child from the parent's list; if the child was absent it does nothing, and
    /// otherwise it appends the child at the tail.
    ///
    /// There is **no [`Self::set_parent`], no child-add and no [`Self::update_for_parent_size_change`]**: nothing in
    /// it writes the parent and nothing re-anchors the child. That is the
    /// whole point of the function and it is why drag startup uses it to bring the proxy to the
    /// top
    /// on the original manager's root element — to put the drag proxy on the screen.
    /// See [`crate::UiSystem::start_drag_and_drop_at`] for what re-anchoring it
    /// instead would do to the ghost.
    ///
    /// The one bookkeeping divergence: retail's proxy keeps a null parent (it was created as a child
    /// element with no parent, and the client's *same-parent* early-out made the
    /// one parent-set call a no-op), and this sets `region.parent` so that deletion, hit-testing
    /// and `screen_origin` find the same tree every other element lives in. The root's own box
    /// starts at `(0, 0)`, so the two readings of every screen coordinate are the same number.
    pub fn bring_child_to_top(&mut self, parent: ElemHandle, child: ElemHandle) {
        if let Some(old) = self.parent(child) {
            if let Some(n) = self.node_mut(old) {
                n.region.remove_child(child);
            }
        }
        if let Some(n) = self.node_mut(child) {
            n.region.parent = Some(parent);
        }
        if let Some(n) = self.node_mut(parent) {
            n.region.remove_child(child);
            n.region.children.push(child);
        }
    }

    /// Move a region to the tail of its parent's child list.
    pub fn bring_to_front(&mut self, h: ElemHandle) {
        if let Some(p) = self.parent(h) {
            if let Some(n) = self.node_mut(p) {
                n.region.bring_child_to_front(h);
            }
        }
    }
}
