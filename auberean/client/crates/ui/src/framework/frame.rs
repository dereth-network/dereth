//! Main and child frameworks: a screen's top and the panels inside it.

use std::collections::HashMap;

use crate::listener::{ListenerId, Mail};
use crate::location::Location;
use crate::manager::LAYOUT_MAPPING;
use crate::system::{Handle, Ui, Value};
use crate::token::Token;

/// A framework in a [`Ui`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FrameId(pub u32);

/// One framework.
#[derive(Clone, Debug)]
pub(crate) struct FrameData {
    /// What it hears.
    pub listener: ListenerId,
    /// Its own shown flag (a new framework is shown).
    pub shown: bool,
    /// Its child frameworks.
    pub children: Vec<FrameId>,
    /// A child framework's parent; a main framework has none.
    pub parent: Option<FrameId>,
    /// A main framework's top-level elements, in the order made.
    pub roots: Vec<Handle>,
}

/// Kept places, by layout record and element name.
pub type Locations = HashMap<(u32, Token), Location>;

impl Ui {
    fn frame(&self, f: FrameId) -> Option<&FrameData> {
        self.mgr.frames.get(f.0 as usize)?.as_ref()
    }

    fn frame_mut(&mut self, f: FrameId) -> Option<&mut FrameData> {
        self.mgr.frames.get_mut(f.0 as usize)?.as_mut()
    }

    fn new_frame(&mut self, parent: Option<FrameId>) -> FrameId {
        let listener = self.new_listener();
        let id = FrameId(u32::try_from(self.mgr.frames.len()).unwrap_or(u32::MAX));
        self.mgr.frames.push(Some(FrameData {
            listener,
            shown: true,
            children: Vec::new(),
            parent,
            roots: Vec::new(),
        }));
        id
    }

    /// A screen's top: a framework of its own, shown, with no elements yet.
    // D2-11894, D2-11877
    pub fn new_main_frame(&mut self) -> FrameId {
        self.new_frame(None)
    }

    /// A panel inside `parent`: shown by its own flag, counted among the parent's children.
    // D2-11899
    pub fn new_child_frame(&mut self, parent: FrameId) -> FrameId {
        let id = self.new_frame(Some(parent));
        self.frame_add_child(parent, id);
        id
    }

    /// A framework goes: a child leaves its parent's children; a screen's top removes the
    /// elements it made; its listening is undone.
    // D2-11900, D2-11895, D2-11878, D2-11882, D2-11897, D2-11891, D2-11905, D2-11893
    pub fn drop_frame(&mut self, f: FrameId) {
        let Some(d) = self.frame(f).cloned() else {
            return;
        };
        if let Some(p) = d.parent {
            self.frame_remove_child(p, f);
        } else {
            for r in d.roots {
                self.remove(r);
            }
        }
        self.drop_listener(d.listener);
        if let Some(slot) = self.mgr.frames.get_mut(f.0 as usize) {
            *slot = None;
        }
    }

    /// The listener a framework hears with.
    #[must_use]
    pub fn frame_listener(&self, f: FrameId) -> Option<ListenerId> {
        self.frame(f).map(|d| d.listener)
    }

    /// A framework's parent, for a child framework.
    #[must_use]
    pub fn frame_parent(&self, f: FrameId) -> Option<FrameId> {
        self.frame(f)?.parent
    }

    /// Where a child framework stands among its parent's children, if it does.
    // D2-11880
    #[must_use]
    pub fn frame_find_child(&self, f: FrameId, child: FrameId) -> Option<usize> {
        self.frame(f)?.children.iter().position(|c| *c == child)
    }

    /// Count a child framework among a framework's children (once).
    // D2-11884
    pub fn frame_add_child(&mut self, f: FrameId, child: FrameId) {
        if self.frame_find_child(f, child).is_some() {
            return;
        }
        if let Some(d) = self.frame_mut(f) {
            d.children.push(child);
        }
    }

    /// Take a child framework out of a framework's children (the last child takes its place).
    // D2-11883
    pub fn frame_remove_child(&mut self, f: FrameId, child: FrameId) {
        let Some(i) = self.frame_find_child(f, child) else {
            return;
        };
        if let Some(d) = self.frame_mut(f) {
            d.children.swap_remove(i);
        }
    }

    /// Show or hide a framework: its flag set, and each child told to show again by its own flag
    /// (so a hidden parent hides them all, and showing it brings back the ones that were shown).
    // D2-11879
    pub fn frame_show(&mut self, f: FrameId, on: bool) {
        let Some(d) = self.frame_mut(f) else { return };
        d.shown = on;
        let children = d.children.clone();
        for c in children {
            if let Some(cd) = self.frame(c) {
                let (l, shown) = (cd.listener, cd.shown);
                self.post(l, Mail::Show { shown });
            }
        }
    }

    /// Whether a framework shows: a screen's top by its flag; a panel by its flag while its
    /// parent shows.
    // D2-01715, D2-11904
    #[must_use]
    pub fn frame_shown(&self, f: FrameId) -> bool {
        let Some(d) = self.frame(f) else {
            return false;
        };
        match d.parent {
            None => d.shown,
            Some(p) => d.shown && self.frame_shown(p),
        }
    }

    /// The screen's top a framework belongs to.
    fn frame_main(&self, f: FrameId) -> Option<FrameId> {
        let mut at = f;
        for _ in 0..64 {
            match self.frame(at)?.parent {
                Some(p) => at = p,
                None => return Some(at),
            }
        }
        None
    }

    /// Make the top-level element `name` of the layout an enumeration value names, through the
    /// screen's top: a panel's elements are made by the framework above it.
    // D2-11901, D2-11898, D2-11194
    pub fn frame_create_root(
        &mut self,
        f: FrameId,
        layout_enum: u32,
        name: Token,
    ) -> Option<Handle> {
        let main = self.frame_main(f)?;
        let layout = self.assets().data_id_by_enum(layout_enum, LAYOUT_MAPPING);
        if layout == 0 {
            return None;
        }
        self.frame_create_root_by_data_id(main, layout, name)
    }

    /// Make the top-level element `name` of the layout an enumeration value names without putting
    /// it on the screen (a cell a panel puts in its list), and let it finish.
    // D2-11876
    pub fn frame_create_element(&mut self, layout_enum: u32, name: Token) -> Option<Handle> {
        let id = self.assets().data_id_by_enum(layout_enum, LAYOUT_MAPPING);
        if id == 0 {
            return None;
        }
        let l = self.load_layout(id)?;
        let h = self.create(l, name)?;
        if name.raw() != 0 {
            self.initialized(h);
        }
        Some(h)
    }

    /// Make the top-level element `name` of the layout record `layout` on the screen, kept by
    /// the screen's top, marked as moving its window.
    // D2-11896
    pub fn frame_create_root_by_data_id(
        &mut self,
        f: FrameId,
        layout: u32,
        name: Token,
    ) -> Option<Handle> {
        let main = self.frame_main(f)?;
        let h = self.create_root_element_by_data_id(layout, name)?;
        self.set_attr(h, ROOT_MOVER, Value::Bool(true));
        self.place_tree(h);
        if let Some(d) = self.frame_mut(main) {
            if !d.roots.contains(&h) {
                d.roots.push(h);
            }
        }
        Some(h)
    }

    /// Take one of the screen's top-level elements off the screen (it stays made).
    // D2-11902, D2-11892
    pub fn frame_remove_root(&mut self, f: FrameId, root: Handle) {
        let Some(main) = self.frame_main(f) else {
            return;
        };
        if let Some(d) = self.frame_mut(main) {
            d.roots.retain(|r| *r != root);
        }
        self.remove_root_element(root);
    }

    /// The top-level elements a screen's top made.
    #[must_use]
    pub fn frame_roots(&self, f: FrameId) -> Vec<Handle> {
        self.frame_main(f)
            .and_then(|m| self.frame(m))
            .map(|d| d.roots.clone())
            .unwrap_or_default()
    }

    /// The kept places of the screen's top-level elements whose place is kept (`save_loc`), each
    /// by its layout and name.
    // D2-11888, D2-13082
    #[must_use]
    pub fn frame_inq_locations(&self, f: FrameId) -> Locations {
        self.frame_kept_places(f).into_iter().collect()
    }

    /// As [`Ui::frame_inq_locations`], in the order the screen's top made its elements.
    #[must_use]
    pub fn frame_kept_places(&self, f: FrameId) -> Vec<((u32, Token), Location)> {
        let mut out = Vec::new();
        for r in self.frame_roots(f) {
            let Some(n) = self.node(r) else { continue };
            let layout = self
                .library()
                .layout(self.library().get(n.refdesc).layout)
                .id;
            if let Some(loc) = self.location(r) {
                out.push(((layout, n.id), loc));
            }
        }
        out
    }

    /// Put the screen's top-level elements back where they were kept: nothing moves unless every
    /// kept element still keeps its place; then each takes its place and the children are told.
    // D2-11889, D2-11881, D2-13077
    pub fn frame_apply_locations(&mut self, f: FrameId, kept: &Locations) -> bool {
        let roots = self.frame_roots(f);
        let key = |ui: &Self, r: Handle| {
            ui.node(r).map(|n| {
                let layout = ui.library().layout(ui.library().get(n.refdesc).layout).id;
                (layout, n.id)
            })
        };
        for r in &roots {
            if let Some(k) = key(self, *r) {
                if kept.contains_key(&k) && !self.keeps_location(*r) {
                    return false;
                }
            }
        }
        for r in &roots {
            if let Some(loc) = key(self, *r).and_then(|k| kept.get(&k)).copied() {
                self.apply_location(*r, &loc);
            }
        }
        if let Some(d) = self.frame(f) {
            let children = d.children.clone();
            for c in children {
                if let Some(l) = self.frame_listener(c) {
                    self.post(l, Mail::PlacesPutBack);
                }
            }
        }
        true
    }
}

/// Marks a top-level element made by a screen's top.
const ROOT_MOVER: Token = Token::of("root_mover");
