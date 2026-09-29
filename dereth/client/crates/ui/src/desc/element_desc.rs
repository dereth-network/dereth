//! `ElementDesc` — one node of a layout.
//!
//! Element-description decoding and runtime defaults for UI layouts.

use std::collections::BTreeMap;

use dereth_primitives::DataId;

use crate::desc::state_desc::StateDesc;
use crate::layout::{EdgeMode, Edges};
use crate::region::Box2D;
use crate::{ElementId, ElementType, StateId};

/// An element description with a base state and per-state overrides.
///
/// The maps are `BTreeMap`s, but the observed child traversal follows hash order,
/// not UI read order. An ordered map alone would lose something: siblings tied on both
/// z-level and UI read order are stacked by creation order. That makes the traversal order
/// observable, so it is retained alongside the map in [`Self::children_buckets`],
/// [`Self::child_chain`] and [`Self::creation_order`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ElementDesc {
    /// The `StateDesc` base: rectangle, z-level, properties, media, incorporation flags.
    pub base: StateDesc,
    /// The element id — unique inside a layout, and the key of the whole message system.
    pub element_id: ElementId,
    /// The element type. **0 means "partial": inherit from `base_layout`/`base_element`.**
    pub ty: ElementType,
    /// The engine type — fallback class id used when `ty` has no factory.
    pub engine_ty: ElementType,
    /// The base element a partial inherits from.
    pub base_element: ElementId,
    /// The layout the base element lives in.
    pub base_layout: DataId,
    /// The default state — passed to `set_state` in `initialize`.
    pub default_state: StateId,
    /// The left, top, right and bottom edge modes.
    pub edges: Edges,
    /// The per-state overrides.
    pub states: BTreeMap<StateId, StateDesc>,
    /// The child descriptions, keyed by element id.
    pub children: BTreeMap<ElementId, ElementDesc>,
    /// The UI read order — authoring order.
    pub read_order: u32,
    /// The child table's bucket count, selected by the bucket-size index stored in the layout
    /// record; `0` for a description built by hand.
    pub children_buckets: u32,
    /// The child table's chain order: the record's order for a desc read from the dat
    /// (an add while unpacking appends after the previous node, and assignment keeps the
    /// order when `inq_full_desc` copies the base), with the children a partial adds pushed at the
    /// **front** — adding a child links the new node at the head of its
    /// bucket's chain, and a front insert puts it ahead of every existing member of that bucket
    /// while leaving every other bucket's order alone. See [`Self::creation_order`].
    pub child_chain: Vec<ElementId>,
}

impl ElementDesc {
    /// True when this is a "partial" description that must be resolved through `inq_full_desc`.
    #[must_use]
    pub const fn is_partial(&self) -> bool {
        self.ty.0 == 0
    }

    /// The layout's state-description accessor — "returns nothing for the default state 0, in which
    /// case the element uses the `ElementDesc`'s own `StateDesc` base".
    #[must_use]
    pub fn access_state(&self, id: StateId) -> Option<&StateDesc> {
        if id.0 == 0 {
            return None;
        }
        self.states.get(&id)
    }

    /// The design rectangle, inclusive.
    #[must_use]
    pub fn box_(&self) -> Box2D {
        self.base.box_()
    }

    /// Count this node and every descendant. Used by the conformance gate.
    #[must_use]
    pub fn subtree_len(&self) -> usize {
        1 + self.children.values().map(Self::subtree_len).sum::<usize>()
    }

    /// The child ids in the order retail creates them: the child table's own iteration order —
    /// bucket by bucket, then down each bucket's chain, then the next non-empty bucket — where a
    /// child's bucket is `element_id % children_buckets`.
    ///
    /// **This order is load-bearing.** Adding a child orders siblings
    /// by z-level and then UI read order, and on a
    /// full tie inserts the new child *after* the existing one, so two siblings that tie on both
    /// keep their creation order and the one created first is drawn first, underneath. The skills
    /// footer `0x10000247` has such a pair: the partial merge shifts a
    /// partial's new child's read order by the number of base children the partial leaves alone
    /// (one, the title), which puts the meter `0x10000247` (read order 1 + 1) on the same read
    /// order 2 as the label `0x10000242` inherited from the base `0x1000024D`. That base's record
    /// stores a 23-bucket table, `0x10000247 % 23 == 3` and `0x10000242 % 23 == 21`, so retail
    /// creates the meter first and paints its opaque trough `0x060011A6` under the label. An
    /// element-id walk creates them the other way round and the label vanishes under the trough.
    ///
    /// A desc with no recorded table (`child_chain` empty and `children_buckets` 0, as one built
    /// by hand is) walks in element-id order; a child in the map but not in the chain is appended
    /// in id order, so the walk is always complete. The sort is stable, so a bucket's chain keeps
    /// its recorded order.
    #[must_use]
    pub fn creation_order(&self) -> Vec<ElementId> {
        let mut ids: Vec<ElementId> = self
            .child_chain
            .iter()
            .copied()
            .filter(|id| self.children.contains_key(id))
            .collect();
        for id in self.children.keys() {
            if !ids.contains(id) {
                ids.push(*id);
            }
        }
        if self.children_buckets > 0 {
            ids.sort_by_key(|id| id.0 % self.children_buckets);
        }
        ids
    }

    /// Convert one of the asset crate's decoded records.
    ///
    /// The asset crate hands the five conditional geometry fields back as `Option`, which is the wire form.
    /// In memory the client keeps plain ints initialised to 0 by the state description's
    /// constructor, so an absent field is 0 with its incorporation bit clear — and stays inheritable.
    #[must_use]
    pub fn from_asset(d: &dereth_assets::ui::ElementDesc) -> Self {
        let mut base = StateDesc::from_asset(&d.state);
        base.x = d.x.unwrap_or(0);
        base.y = d.y.unwrap_or(0);
        base.width = d.width.unwrap_or(0);
        base.height = d.height.unwrap_or(0);
        base.z_level = d.z_level.unwrap_or(0);
        Self {
            base,
            element_id: ElementId(d.element_id),
            ty: ElementType(d.element_type),
            // The wire form has no separate engine-type field:
            // the record carries six ids and the engine type is not one of them. It is set at run time by the
            // subclass, so it starts at 0 here — which makes the factory fallback a no-op for
            // dat-loaded descs, exactly as it is in the client.
            engine_ty: ElementType(0),
            base_element: ElementId(d.base_element),
            base_layout: d.base_layout,
            default_state: StateId(d.default_state),
            edges: Edges {
                left: EdgeMode::from_u32(d.left_edge),
                top: EdgeMode::from_u32(d.top_edge),
                right: EdgeMode::from_u32(d.right_edge),
                bottom: EdgeMode::from_u32(d.bottom_edge),
            },
            states: d
                .states
                .iter()
                .map(|(k, v)| (StateId(*k), StateDesc::from_asset(v)))
                .collect(),
            children: d
                .children
                .iter()
                .map(|(k, v)| (ElementId(*k), Self::from_asset(v)))
                .collect(),
            read_order: d.ui_read_order,
            children_buckets: d.children_buckets,
            child_chain: d.children.iter().map(|(k, _)| ElementId(*k)).collect(),
        }
    }
}
