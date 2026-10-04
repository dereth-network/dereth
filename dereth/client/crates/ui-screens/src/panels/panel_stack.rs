//! `PanelStack` — the sixteen page containers, the panel-id indirection and the restore rule.
//!
//! **The pairing of buttons and pages is the whole point of this file.** Nothing in the client says which toolbar button
//! opens which page. Both a button and a page carry element attribute `0x10000029` — the *panel
//! id*, which is itself an element id — and the only channel between them is the
//! set-panel-visibility notice. So this module reads the attribute off the sixteen page containers
//! at run time and never pairs an id with a panel name.

use dereth_ui::{ElemHandle, ElementId, UiSystem};

use crate::bind::{attr_bool, attr_enum, attr_int};
use crate::view::UiRequest;

/// One child-info entry built during panel child setup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageInfo {
    /// The page container's own element id, one of [`super::catalogue::PANEL_PAGES`].
    pub element: ElementId,
    pub handle: ElemHandle,
    /// Attribute `0x10000029` off the page. **Panel id 0 is ignored** by
    /// the panel-visibility notice handler's step 1.
    pub panel_id: u32,
    /// Attribute `0x10000049` — "this page is a transient overlay".
    pub transient: bool,
}

/// `PanelStack` — element type `0x10000008`.
#[derive(Debug, Default)]
pub struct PanelStack {
    /// The registered pages, in child set-up order.
    pub pages: Vec<PageInfo>,
    /// The page currently shown.
    pub current: Option<ElementId>,
    /// The previously shown page — restored when a transient overlay is closed.
    pub previous: Option<ElementId>,
    /// The element the stack itself hangs off, when this instance is the one whose own visibility
    /// follows the current page.
    ///
    /// The client shows the stack itself on every show path and hides it on the one hide path
    /// that leaves no page
    /// current — so opening a toolbar panel is what makes `<PANS>` appear and closing the last one
    /// is what makes it go away. The environment sibling also owns its root's visibility, plus
    /// child-height resizing; it uses `recv_env_panel_visibility` below.
    pub root: Option<ElemHandle>,
}

/// The four size clamps the resize applies, read from the layout rather than hard-coded: the
/// engine's own clamp record.
pub use dereth_ui::SizeClamps;

impl PanelStack {
    /// The panel's child set-up: look each of the sixteen documented page ids up under
    /// `root`, read attributes `0x10000029` and `0x10000049` off it, and push the pair.
    ///
    /// A page id that is not in the layout is **skipped**, not an error: the child lookup finds
    /// nothing and the entry is simply not added.
    ///
    /// **And then it hides every page it registered** — its last act is to set every registered
    /// page invisible (the visibility setter, not the lose-focus callback),
    /// which is what makes "exactly one page at a time" true from the first frame rather than only
    /// after the first `SetPanelVisibility`. **None of the sixteen pages carries `0x3B` at all** —
    /// checked against the shipped layouts, and true of `EnvironmentPanelStack`'s five and
    /// `CombatPanelStack`'s two as well — so all twenty-three come up visible whichever way round
    /// `UICore_Element_hide` is read, and without this loop the whole panel stack draws at once.
    /// **This loop is therefore retail behaviour and not a compensation for anything.**
    /// The sibling environment and combat panel stacks end initialization with the same loop, so
    /// all three stacks hide their discovered pages after setup: 16 here, 5 environment pages,
    /// and 2 combat pages. None relies on a hide attribute. See [`Self::setup_children_with`].
    pub fn setup_children(&mut self, ui: &mut UiSystem, root: ElemHandle) {
        self.setup_children_with(ui, root, &super::catalogue::PANEL_PAGES);
    }

    /// [`Self::setup_children`] over an explicit page-id list.
    ///
    /// Three classes share the body — `PanelStack` with its sixteen pages, `EnvironmentPanelStack` with
    /// `0x1000005D`…`0x10000062` and `CombatPanelStack` with `0x1000005C`/`0x10000061` — and all
    /// three keep a list of (child, panel id) pairs, read `0x10000029` off each child and then hide
    /// every one of them.
    pub fn setup_children_with(&mut self, ui: &mut UiSystem, root: ElemHandle, ids: &[u32]) {
        use crate::bind::attr;
        self.pages.clear();
        self.current = None;
        self.previous = None;
        for id in ids {
            let id = ElementId(*id);
            let Some(h) = ui.get_child_recursive(root, id) else {
                continue;
            };
            self.pages.push(PageInfo {
                element: id,
                handle: h,
                panel_id: attr_enum(ui, h, attr::PANEL_ID)
                    .or_else(|| attr_int(ui, h, attr::PANEL_ID).map(|v| v as u32))
                    .unwrap_or(0),
                transient: attr_bool(ui, h, attr::TRANSIENT_OVERLAY).unwrap_or(false),
            });
        }
        // The trailing loop. Every registered page, hidden.
        for p in &self.pages {
            ui.set_visible(p.handle, false);
        }
    }

    /// Register the four notices, set up the children, and then show the stack itself if a page is
    /// current and hide it otherwise — i.e. **the panel stack itself is visible exactly when a page
    /// is current**. `root` is the element the stack hangs off — the floaty panel's `0x100005FF`
    /// for the toolbar stack.
    pub fn post_init(&mut self, ui: &mut UiSystem, root: ElemHandle, ids: &[u32]) {
        self.setup_children_with(ui, root, ids);
        self.root = Some(root);
        ui.set_visible(root, self.current.is_some());
    }

    /// Read the four clamps off the panel-stack element.
    #[must_use]
    pub fn size_clamps(ui: &UiSystem, root: ElemHandle) -> SizeClamps {
        use crate::bind::attr;
        SizeClamps {
            min_w: attr_int(ui, root, attr::MIN_WIDTH),
            min_h: attr_int(ui, root, attr::MIN_HEIGHT),
            max_w: attr_int(ui, root, attr::MAX_WIDTH),
            max_h: attr_int(ui, root, attr::MAX_HEIGHT),
        }
    }

    fn find(&self, panel_id: u32) -> Option<PageInfo> {
        self.pages.iter().copied().find(|p| p.panel_id == panel_id)
    }

    fn info(&self, element: ElementId) -> Option<PageInfo> {
        self.pages.iter().copied().find(|p| p.element == element)
    }

    /// The environment panel stack's set-panel-visibility notice, not PanelStack's transient
    /// restore rule. Child setup passes "should be default" = false for all five
    /// registrations, so there is no default page to restore. The handler resizes the parent
    /// to its own width and the child's height, then makes the parent visible.
    pub fn recv_env_panel_visibility(
        &mut self,
        ui: &mut UiSystem,
        panel_id: u32,
        show: bool,
    ) -> Vec<UiRequest> {
        let mut out = Vec::new();
        if panel_id == 0 {
            return out;
        }
        let Some(target) = self.find(panel_id) else {
            return out;
        };
        if show {
            let changed = self.current != Some(target.element);
            let old = self.current.and_then(|e| self.info(e));
            self.current = Some(target.element);
            if changed {
                if let Some(old) =
                    old.filter(|p| ui.node(p.handle).is_some_and(|n| n.region.flags.visible))
                {
                    ui.set_visible(old.handle, false);
                    out.push(UiRequest::SetPanelVisibility {
                        panel: old.panel_id,
                        visible: false,
                    });
                }
            }
            ui.set_visible(target.handle, true);
            if let Some(root) = self.root {
                if changed {
                    let size = ui
                        .node(root)
                        .zip(ui.node(target.handle))
                        .map(|(parent, child)| {
                            (parent.region.box_.width(), child.region.box_.height())
                        });
                    if let Some((w, h)) = size {
                        ui.resize_to(root, w, h);
                    }
                }
                ui.set_visible(root, true);
            }
        } else {
            ui.set_visible(target.handle, false);
            if self.current == Some(target.element) {
                self.current = None;
                if let Some(root) = self.root {
                    ui.set_visible(root, false);
                }
            }
        }
        out
    }

    /// The panel's set-panel-visibility notice, in the documented three steps.
    ///
    /// Returns the notices the client re-sends as a side effect, in order, so a test can assert
    /// them; the caller queues them as [`UiRequest::SetPanelVisibility`].
    pub fn recv_set_panel_visibility(
        &mut self,
        ui: &mut UiSystem,
        panel_id: u32,
        show: bool,
    ) -> Vec<UiRequest> {
        let mut out = Vec::new();
        // 1. "ignore panel id 0".
        if panel_id == 0 {
            return out;
        }
        let Some(target) = self.find(panel_id) else {
            return out;
        };

        if show {
            // 2. If the page is already the current one, just re-show it.
            if self.current == Some(target.element) {
                ui.set_visible(target.handle, true);
                if let Some(root) = self.root {
                    ui.set_visible(root, true);
                }
                return out;
            }
            let old = self.current.and_then(|e| self.info(e));
            self.current = Some(target.element);
            if let Some(old) = old {
                // "the old page becomes the previously shown page only when the new page has
                // 0x10000049 set and the old one does not (i.e. the new page is a transient
                // overlay). Otherwise the previously shown page is cleared."
                self.previous = if target.transient && !old.transient {
                    Some(old.element)
                } else {
                    None
                };
                ui.set_visible(old.handle, false);
                out.push(UiRequest::SetPanelVisibility {
                    panel: old.panel_id,
                    visible: false,
                });
            } else {
                self.previous = None;
            }
            ui.set_visible(target.handle, true);
        } else {
            // 3. Hiding the current page falls back to the previously shown page.
            if self.current != Some(target.element) {
                ui.set_visible(target.handle, false);
                return out;
            }
            ui.set_visible(target.handle, false);
            match self.previous.take().and_then(|e| self.info(e)) {
                Some(prev) => {
                    self.current = Some(prev.element);
                    ui.set_visible(prev.handle, true);
                    out.push(UiRequest::SetPanelVisibility {
                        panel: prev.panel_id,
                        visible: true,
                    });
                }
                None => self.current = None,
            }
        }
        // The stack is visible iff a page is current: the client shows it on each of the three
        // show paths and hides it on the single hide path that clears the current page. See
        // [`Self::root`].
        if let Some(root) = self.root {
            ui.set_visible(root, self.current.is_some());
        }
        out
    }

    /// The panel stack's element-message handler — element message `0x18` (visibility
    /// changed) from a registered page re-broadcasts the notice in the other direction, so the
    /// server-side "which panels are open" record and the toolbar button states stay in sync.
    #[must_use]
    pub fn on_page_visibility_changed(
        &self,
        source: ElementId,
        visible: bool,
    ) -> Option<UiRequest> {
        let info = self.info(source)?;
        if info.panel_id == 0 {
            return None;
        }
        Some(UiRequest::SetPanelVisibility {
            panel: info.panel_id,
            visible,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a stack whose pages are synthetic: the restore rule is pure logic over
    /// `(panel_id, transient)` and needs no layout.
    fn stack(rows: &[(u32, u32, bool)]) -> PanelStack {
        PanelStack {
            pages: rows
                .iter()
                .enumerate()
                .map(|(i, (element, panel_id, transient))| PageInfo {
                    element: ElementId(*element),
                    handle: ElemHandle::for_test(u32::try_from(i).unwrap() + 1),
                    panel_id: *panel_id,
                    transient: *transient,
                })
                .collect(),
            current: None,
            previous: None,
            root: None,
        }
    }

    /// Oracle: the toolbar and panel behavior for the panel's set-visibility notice,
    /// steps 2 and 3 — the exact restore behaviour, including the transient-overlay rule via
    /// attribute `0x10000049`.
    #[test]
    fn a_transient_overlay_restores_the_persistent_page_it_covered() {
        let mut ui = UiSystem::new((800, 600));
        // page A persistent, page B transient.
        let mut s = stack(&[(0x1000_0181, 10, false), (0x1000_0182, 20, true)]);

        s.recv_set_panel_visibility(&mut ui, 10, true);
        assert_eq!(s.current, Some(ElementId(0x1000_0181)));
        assert_eq!(s.previous, None);

        // Opening the transient overlay remembers A.
        let out = s.recv_set_panel_visibility(&mut ui, 20, true);
        assert_eq!(s.current, Some(ElementId(0x1000_0182)));
        assert_eq!(s.previous, Some(ElementId(0x1000_0181)));
        assert_eq!(
            out,
            vec![UiRequest::SetPanelVisibility {
                panel: 10,
                visible: false
            }]
        );

        // Closing it restores A and re-broadcasts.
        let out = s.recv_set_panel_visibility(&mut ui, 20, false);
        assert_eq!(s.current, Some(ElementId(0x1000_0181)));
        assert_eq!(s.previous, None);
        assert_eq!(
            out,
            vec![UiRequest::SetPanelVisibility {
                panel: 10,
                visible: true
            }]
        );
    }

    /// Oracle: the same function — "Otherwise the previously shown page is cleared". Two persistent
    /// pages never restore each other.
    #[test]
    fn a_persistent_page_over_a_persistent_page_clears_the_restore_slot() {
        let mut ui = UiSystem::new((800, 600));
        let mut s = stack(&[(0x1000_0181, 10, false), (0x1000_0183, 30, false)]);
        s.recv_set_panel_visibility(&mut ui, 10, true);
        s.recv_set_panel_visibility(&mut ui, 30, true);
        assert_eq!(s.current, Some(ElementId(0x1000_0183)));
        assert_eq!(s.previous, None);
        let out = s.recv_set_panel_visibility(&mut ui, 30, false);
        assert_eq!(s.current, None, "nothing to fall back to");
        assert!(out.is_empty());
    }

    /// Oracle: the same function — a transient page opened over another *transient* page does not
    /// become a restore target either, because the rule requires the old page to be non-transient.
    #[test]
    fn a_transient_page_over_a_transient_page_does_not_restore() {
        let mut ui = UiSystem::new((800, 600));
        let mut s = stack(&[(0x1000_0182, 20, true), (0x1000_0184, 40, true)]);
        s.recv_set_panel_visibility(&mut ui, 20, true);
        s.recv_set_panel_visibility(&mut ui, 40, true);
        assert_eq!(s.previous, None);
    }

    /// Oracle: step 1 — "ignore panel id 0". A page whose `0x10000029` is absent reads 0 and is
    /// unreachable, which is how the layout disables a page container.
    #[test]
    fn panel_id_zero_is_ignored_in_both_directions() {
        let mut ui = UiSystem::new((800, 600));
        let mut s = stack(&[(0x1000_0181, 0, false)]);
        assert!(s.recv_set_panel_visibility(&mut ui, 0, true).is_empty());
        assert_eq!(s.current, None);
        assert_eq!(
            s.on_page_visibility_changed(ElementId(0x1000_0181), true),
            None
        );
    }

    /// Oracle: the panel stack's element-message handler — a page that changes visibility on
    /// its own re-broadcasts the notice so the toolbar button state follows.
    #[test]
    fn a_pages_own_visibility_change_is_re_broadcast() {
        let s = stack(&[(0x1000_0181, 10, false)]);
        assert_eq!(
            s.on_page_visibility_changed(ElementId(0x1000_0181), false),
            Some(UiRequest::SetPanelVisibility {
                panel: 10,
                visible: false
            })
        );
        assert_eq!(s.on_page_visibility_changed(ElementId(0xDEAD), true), None);
    }

    /// Oracle: the panel stack's resize — "clamps the panel stack to four size attributes
    /// before resizing": `0x3C` min width, `0x3D` min height, `0x3E` max width, `0x3F` max height.
    #[test]
    fn the_four_size_clamps_bound_a_resize_in_both_directions() {
        let c = SizeClamps {
            min_w: Some(100),
            min_h: Some(50),
            max_w: Some(400),
            max_h: Some(300),
        };
        assert_eq!(c.apply(10, 10), (100, 50));
        assert_eq!(c.apply(1000, 1000), (400, 300));
        assert_eq!(c.apply(200, 100), (200, 100));
        // An absent attribute simply does not clamp.
        let none = SizeClamps::default();
        assert_eq!(none.apply(7, 9), (7, 9));
    }
}
