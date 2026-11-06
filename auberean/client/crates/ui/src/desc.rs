//! Descriptions: the layout records, indexed the way the client looks them up.
//!
//! A layout's top-level elements are found by name. An element's states are found by name. A
//! state's children are kept in a hashed table, and the client creates them in that table's
//! order, which decides how siblings at the same depth overlap; [`Library::add`] stores each
//! state's children in that order.

use std::collections::HashMap;

use auberean_assets::ui::{Attribute, Element, Media, Transition, UiLayout};

use crate::token::{kind, state, Token};
use crate::Rect;

/// An element description in a [`Library`].
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct DescIx(pub u32);

/// A layout in a [`Library`].
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct LayoutIx(pub u32);

/// One element as its layout describes it.
#[derive(Clone, Debug)]
pub struct ElemDesc {
    pub layout: LayoutIx,
    pub id: Token,
    pub kind: Token,
    pub default_state: Token,
    pub rect: Rect,
    pub z: i32,
    /// How the left, top, right and bottom edges follow the parent's size.
    pub edges: [u8; 4],
    pub states: Vec<StateDesc>,
}

/// One state of an element.
#[derive(Clone, Debug)]
pub struct StateDesc {
    pub id: Token,
    /// Bit 0: entering this state puts every child into the same state.
    pub flags: u8,
    /// The state's own rectangle and depth, when it has one.
    pub rect: Option<(Rect, i32)>,
    pub attributes: Vec<Attribute>,
    pub media: Vec<Media>,
    pub transitions: Vec<Transition>,
    /// The children, in the order the client creates them.
    pub children: Vec<DescIx>,
}

impl StateDesc {
    /// Whether entering this state passes it on to the children.
    #[must_use]
    pub const fn passes_state_to_children(&self) -> bool {
        self.flags & 1 != 0
    }
}

/// A layout's screen and its top-level elements.
#[derive(Clone, Debug)]
pub struct LayoutInfo {
    pub id: u32,
    pub width: u16,
    pub height: u16,
    pub string_table: u32,
    roots: HashMap<Token, DescIx>,
    /// How many elements stand made from it.
    refs: i32,
    /// Let go once its last element went: loading its record again reads the record anew.
    released: bool,
    /// The record it was indexed from.
    record: std::sync::Arc<UiLayout>,
}

/// Every loaded layout's descriptions.
#[derive(Clone, Debug, Default)]
pub struct Library {
    layouts: Vec<LayoutInfo>,
    elements: Vec<ElemDesc>,
}

/// The bucket count of a state's table of children: the first of the core library's prime sizes
/// at or above the sixteen the table is made with.
const CHILD_BUCKETS: u32 = 23;

/// The order a hashed table of `ids` iterates in: buckets in turn, each from its newest entry; a
/// repeated id is refused, so the first one stays.
fn table_order(ids: &[Token]) -> Vec<usize> {
    let mut buckets: Vec<Vec<usize>> = vec![Vec::new(); CHILD_BUCKETS as usize];
    let mut seen = std::collections::HashSet::new();
    for (i, id) in ids.iter().enumerate() {
        if seen.insert(*id) {
            buckets[(id.0 % CHILD_BUCKETS) as usize].push(i);
        }
    }
    buckets
        .into_iter()
        .flat_map(|b| b.into_iter().rev())
        .collect()
}

impl Library {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Index a layout record; returns the layout's place in the library. A layout let go whose
    /// record reads the same is taken back as it was.
    pub fn add(&mut self, id: u32, layout: &std::sync::Arc<UiLayout>) -> LayoutIx {
        if let Some(i) = self
            .layouts
            .iter()
            .position(|l| l.id == id && l.released && *l.record == **layout)
        {
            self.layouts[i].released = false;
            return LayoutIx(u32::try_from(i).unwrap_or(u32::MAX));
        }
        let ix = LayoutIx(u32::try_from(self.layouts.len()).unwrap_or(u32::MAX));
        let mut roots = HashMap::new();
        for e in &layout.elements {
            let d = self.add_element(ix, e);
            roots.entry(Token(e.id)).or_insert(d);
        }
        self.layouts.push(LayoutInfo {
            id,
            width: layout.width,
            height: layout.height,
            string_table: layout.string_table.0,
            roots,
            refs: 0,
            released: false,
            record: layout.clone(),
        });
        ix
    }

    /// One more element made from a layout.
    pub(crate) fn add_ref(&mut self, l: LayoutIx) {
        if let Some(info) = self.layouts.get_mut(l.0 as usize) {
            info.refs += 1;
        }
    }

    /// One element made from a layout goes; the last one lets the layout go.
    pub(crate) fn drop_ref(&mut self, l: LayoutIx) {
        let Some(info) = self.layouts.get_mut(l.0 as usize) else {
            return;
        };
        info.refs -= 1;
        if info.refs == 0 {
            self.release_layout(l);
        }
    }

    /// Let a layout go: it is no longer found by its record id, so the next load reads it again.
    /// What was made from it keeps its descriptions.
    // D2-11243
    pub fn release_layout(&mut self, l: LayoutIx) {
        if let Some(info) = self.layouts.get_mut(l.0 as usize) {
            info.released = true;
        }
    }

    fn add_element(&mut self, layout: LayoutIx, e: &Element) -> DescIx {
        let ix = DescIx(u32::try_from(self.elements.len()).unwrap_or(u32::MAX));
        self.elements.push(ElemDesc {
            layout,
            id: Token(e.id),
            kind: Token(e.kind),
            default_state: Token(e.default_state),
            rect: Rect::new(
                i32::from(e.left),
                i32::from(e.top),
                i32::from(e.right),
                i32::from(e.bottom),
            ),
            z: i32::from(e.z),
            edges: e.edges,
            states: Vec::new(),
        });
        let mut states = Vec::with_capacity(e.states.len());
        for s in &e.states {
            let ids: Vec<Token> = s.children.iter().map(|c| Token(c.id)).collect();
            let children = table_order(&ids)
                .into_iter()
                .map(|i| self.add_element(layout, &s.children[i]))
                .collect();
            let rect = (s.left != -1).then(|| {
                (
                    Rect::new(
                        i32::from(s.left),
                        i32::from(s.top),
                        i32::from(s.right),
                        i32::from(s.bottom),
                    ),
                    i32::from(s.z),
                )
            });
            states.push(StateDesc {
                id: Token(s.id),
                flags: s.flags,
                rect,
                attributes: s.attributes.clone(),
                media: s.media.clone(),
                transitions: s.transitions.clone(),
                children,
            });
        }
        self.elements[ix.0 as usize].states = states;
        ix
    }

    #[must_use]
    pub fn layout(&self, ix: LayoutIx) -> &LayoutInfo {
        &self.layouts[ix.0 as usize]
    }

    /// The layout with record id `id`, if loaded (and not let go since).
    #[must_use]
    pub fn layout_by_id(&self, id: u32) -> Option<LayoutIx> {
        self.layouts
            .iter()
            .position(|l| l.id == id && !l.released)
            .map(|i| LayoutIx(u32::try_from(i).unwrap_or(u32::MAX)))
    }

    #[must_use]
    pub fn get(&self, d: DescIx) -> &ElemDesc {
        &self.elements[d.0 as usize]
    }

    /// A layout's top-level element by name.
    // D2-11066
    #[must_use]
    pub fn root(&self, layout: LayoutIx, id: Token) -> Option<DescIx> {
        self.layouts.get(layout.0 as usize)?.roots.get(&id).copied()
    }

    /// One state of an element, by name.
    #[must_use]
    pub fn state(&self, d: DescIx, s: Token) -> Option<&StateDesc> {
        self.get(d).states.iter().find(|st| st.id == s)
    }

    /// How the child named `child` of a state of an element follows its parent's size.
    pub(crate) fn set_child_edges(&mut self, d: DescIx, s: Token, child: Token, edges: [u8; 4]) {
        let Some(c) = self.state(d, s).and_then(|st| {
            st.children
                .iter()
                .copied()
                .find(|c| self.get(*c).id == child)
        }) else {
            return;
        };
        self.elements[c.0 as usize].edges = edges;
    }

    /// What a reference stands for: the top-level element its base state's `referent` names,
    /// followed through further references; an element that is no reference is itself.
    // D2-11067
    #[must_use]
    pub fn dereference(&self, mut d: DescIx) -> Option<DescIx> {
        // A chain longer than the layout cannot end.
        for _ in 0..=self.elements.len() {
            let e = self.get(d);
            if e.kind != kind::REFERENCE {
                return Some(d);
            }
            let base = self.state(d, state::STATELESS)?;
            let target = base.attributes.iter().find_map(|a| match a.value {
                auberean_assets::ui::AttributeValue::Id(t)
                    if Token(a.id) == crate::token::attr::REFERENT =>
                {
                    Some(t)
                }
                _ => None,
            })?;
            d = self.root(e.layout, Token(target))?;
        }
        None
    }

    /// A copy of an element's description, for an element that changes its own: its states with
    /// their attributes, media (each kind's fields), transitions and children the same.
    // D2-11083, D2-11081, D2-11090
    // D2-11109, D2-11117, D2-11122, D2-11127, D2-11130, D2-11134, D2-11138, D2-11142, D2-11146
    // D2-11150
    pub fn copy(&mut self, d: DescIx) -> DescIx {
        let e = self.get(d).clone();
        self.elements.push(e);
        DescIx(u32::try_from(self.elements.len() - 1).unwrap_or(u32::MAX))
    }

    /// Add a media item to the end of one state's list; false when the description has no such
    /// state.
    pub fn add_media(&mut self, d: DescIx, s: Token, m: Media) -> bool {
        let Some(st) = self.elements[d.0 as usize]
            .states
            .iter_mut()
            .find(|st| st.id == s)
        else {
            return false;
        };
        st.media.push(m);
        true
    }

    /// The kind of element a description makes: a reference's is its target's.
    // D2-11072
    #[must_use]
    pub fn referent_kind(&self, d: DescIx) -> Option<Token> {
        self.dereference(d).map(|t| self.get(t).kind)
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the order of a hashed table, on synthetic ids)
    use super::*;

    #[test]
    fn children_come_out_bucket_by_bucket_newest_first() {
        // 1 and 24 share bucket 1; 2 is alone in bucket 2; a repeated 1 is refused.
        let ids = [Token(24), Token(2), Token(1), Token(1)];
        assert_eq!(table_order(&ids), vec![2, 0, 1]);
    }
}
