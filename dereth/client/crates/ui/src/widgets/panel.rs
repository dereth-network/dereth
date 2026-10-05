use super::*;
use crate::ElementId;
use std::collections::BTreeMap;

/// The property ids `setup_tab_page_hash` walks, with the retail `MasterProperty` names.
///
/// `0x2E` is the trigger: the panel's attribute handler is one line, which runs
/// `setup_tab_page_hash` when the property is `0x2E`, and initialisation step 4
/// re-dispatches every property in the description, so a panel builds its tab table the
/// moment its tree comes up.
pub mod attr {
    /// `UICore_Panel_pages` -- an `Array` of [`PAGE_DATA`] structs.
    pub const PAGES: u32 = 0x2E;
    /// `UICore_Panel_page_data` -- one `Struct`.
    pub const PAGE_DATA: u32 = 0x2F;
    /// `UICore_Panel_tab_element` -- the tab's element id.
    pub const TAB_ELEMENT: u32 = 0x30;
    /// `UICore_Panel_page_element` -- the page's element id.
    pub const PAGE_ELEMENT: u32 = 0x31;
    /// `UICore_Panel_page_open` -- set on the entry the panel opens with. **Absent means
    /// false**: three of `0x1000018F`'s four entries omit the member entirely.
    pub const PAGE_OPEN: u32 = 0x32;
}

/// The two states a tab element carries. Every one of the shipped tabs is a `TextElement`
/// declaring exactly `0x0B` and `0x0C`, which differ only in the font colour array (0x1B):
/// `0x0B` is `0xFF8080FF` and `0x0C` is `0xFFCDCDCC`.
///
/// The panel update puts every tab into one of the two, but which one is not recoverable
/// statically. The pairing here is the one a windowed retail run shows:
/// with the character panel open and the Skills tab clicked, the Skills label is the pale one
/// and Attributes and Titles are the blue ones.
pub mod tab_state {
    use crate::StateId;
    /// The tab of a page that is **not** open.
    pub const CLOSED: StateId = StateId(0x0B);
    /// The tab of the open page.
    pub const OPEN: StateId = StateId(0x0C);
}

#[derive(Debug, Default)]
pub struct Panel {
    /// Tab element id → page element id.
    pub tab_to_page: BTreeMap<ElementId, ElementId>,
    /// Page element id → tab element id.
    pub page_to_tab: BTreeMap<ElementId, ElementId>,
    /// The open tab.
    pub open_tab: Option<ElementId>,
    /// The open page.
    pub open_page: Option<ElementId>,
}

pub fn create(_l: &crate::LayoutDesc, _d: &crate::ElementDesc) -> Box<dyn Element> {
    Box::new(Panel::default())
}

impl Panel {
    pub fn add_tab(&mut self, tab: ElementId, page: ElementId) {
        self.tab_to_page.insert(tab, page);
        self.page_to_tab.insert(page, tab);
    }

    /// Rebuild both tab/page hashes from the `0x2E`
    /// array, then `update(default_page, default_tab)`.
    ///
    /// The client clears both tables first and resets the two tokens to 0, which is what makes
    /// the closing `update` run at all (see [`Self::update`]'s guard).
    pub fn setup_tab_page_hash(&mut self, ctx: &mut ElemCtx<'_>, v: Option<&crate::PropertyValue>) {
        self.tab_to_page.clear();
        self.page_to_tab.clear();
        self.open_tab = None;
        self.open_page = None;
        let Some(crate::PropertyValue::Array(rows)) = v else {
            return;
        };
        let mut default: Option<(ElementId, ElementId)> = None;
        for row in rows {
            if row.id != attr::PAGE_DATA {
                continue;
            }
            let crate::PropertyValue::Struct(members) = &row.value else {
                continue;
            };
            let member = |id: u32| {
                members
                    .iter()
                    .find(|(k, _)| *k == id)
                    .map(|(_, p)| &p.value)
            };
            let (Some(crate::PropertyValue::Enum(tab)), Some(crate::PropertyValue::Enum(page))) =
                (member(attr::TAB_ELEMENT), member(attr::PAGE_ELEMENT))
            else {
                continue;
            };
            let (tab, page) = (ElementId(*tab), ElementId(*page));
            // Each hash refuses a duplicate key, and the client tests both returns
            // before it does anything else with the row.
            if self.tab_to_page.contains_key(&tab) || self.page_to_tab.contains_key(&page) {
                continue;
            }
            self.tab_to_page.insert(tab, page);
            self.page_to_tab.insert(page, tab);
            if matches!(
                member(attr::PAGE_OPEN),
                Some(crate::PropertyValue::Bool(true))
            ) {
                default = Some((tab, page));
            }
        }
        // **The tabs have to be hit-testable, and nothing else in the data makes them so.**
        // Every shipped tab is a `TextElement`, and neither route to mouse visibility
        // reaches one: text mouse visibility is true only for editable or selectable text,
        // while the base rule requires either the context-menu bit or valid tooltip text.
        // Both rules answer false for a plain label with no tooltip,
        // which is what all six panels' tabs are. Context-menu lookup can therefore never
        // return one and no click can reach `open_tab`.
        //
        // This implementation makes each tab mouse-visible after finding it recursively,
        // so the tab-page container can route clicks to its pages.
        for tab in self.tab_to_page.keys().copied().collect::<Vec<_>>() {
            let me = ctx.me;
            if let Some(h) = ctx.ui.get_child_recursive(me, tab) {
                ctx.ui.set_mouse_visible(h, true);
            }
        }
        // The client seeds the pair from the entry carrying `UICore_Panel_page_open`; with no
        // such entry both tokens stay 0 and `update` hides every page.
        let (tab, page) = default.unwrap_or((ElementId(0), ElementId(0)));
        self.update(ctx, page, tab);
    }

    /// The panel's update.
    ///
    /// **Retail's guard is `&&`, not `||`**: it runs only if `page != open_page && tab !=
    /// open_tab`, so in retail a call naming the page that is already open (or the tab that is
    /// already open) does nothing, whatever the other token is.
    ///
    /// **This build does not reproduce that**: the code below returns only when *both* tokens
    /// match, so a call that changes just one of them still runs here.
    pub fn update(&mut self, ctx: &mut ElemCtx<'_>, page: ElementId, tab: ElementId) {
        let same_page = self.open_page.is_some_and(|p| p == page);
        let same_tab = self.open_tab.is_some_and(|t| t == tab);
        if same_page && same_tab {
            return;
        }
        self.open_tab = Some(tab);
        self.open_page = Some(page);
        let me = ctx.me;
        for (t, p) in self.tab_to_page.clone() {
            if let Some(h) = ctx.ui.get_child_recursive(me, p) {
                ctx.ui.set_visible(h, p == page);
            }
            if let Some(h) = ctx.ui.get_child_recursive(me, t) {
                let s = if t == tab {
                    tab_state::OPEN
                } else {
                    tab_state::CLOSED
                };
                ctx.ui.set_state(h, s);
            }
        }
        ctx.ui
            .broadcast_element_message(me, msgid::TAB_PAGE_CHANGED, 0, 0);
    }

    /// Resolve tab to page to tab, refusing unless the round
    /// trip lands back on the tab it started from.
    pub fn open_tab(&mut self, ctx: &mut ElemCtx<'_>, tab: ElementId) -> bool {
        let Some(page) = self.tab_to_page.get(&tab).copied() else {
            return false;
        };
        if self.page_to_tab.get(&page).copied() != Some(tab) {
            return false;
        }
        self.update(ctx, page, tab);
        true
    }
}

impl Element for Panel {
    /// `(0x08)` — a caller that wants the tab table.
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    /// A caller that adds a tab of its own ([`Panel::add_tab`]).
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }

    fn on_set_attribute(
        &mut self,
        ctx: &mut ElemCtx<'_>,
        id: u32,
        v: Option<&crate::PropertyValue>,
    ) {
        if id == attr::PAGES {
            self.setup_tab_page_hash(ctx, v);
        }
    }

    fn listen_to_element_message(&mut self, ctx: &mut ElemCtx<'_>, m: &ElementMessage) -> R {
        let me = ctx.me;
        let from_self = ctx
            .ui
            .node(me)
            .is_some_and(|n| n.element_id() == m.source_id);
        if from_self {
            // The panel's own visibility changed. Shown: put the open page back up. Hidden:
            // take every page down, so a re-show cannot reveal two at once.
            if m.id != msgid::VISIBILITY_CHANGED {
                return R::Default;
            }
            // **Queued, not applied inline.** Setting an element's visibility raises `0x18`,
            // so changing a child's
            // visibility from inside a `0x18` handler nests a broadcast, which takes the next
            // serial and stamps every ancestor; the message still bubbling is then refused at
            // each one by `claim_serial` and dies below whatever was listening.
            //
            // Applied inline, the **six** of
            // `PanelStack`'s sixteen pages that are element type `0x00000008` (this type) would
            // never re-broadcast their own visibility, so `PanelStack::on_page_visibility_changed`
            // would never hear about them and the panel stack would never learn which page was
            // current. Six of the seven toolbar panel buttons open one of those six.
            //
            // `queue_set_visible` runs it once the outermost broadcast has unwound, so the
            // panel's own `0x18` completes its bubble first and the page's follows. See
            // [`crate::UiSystem::queue_set_visible`].
            if m.p1 == 0 {
                for p in self.page_to_tab.keys().copied().collect::<Vec<_>>() {
                    if let Some(h) = ctx.ui.get_child_recursive(me, p) {
                        ctx.ui.queue_set_visible(h, false);
                    }
                }
            } else if let Some(open) = self.open_page {
                if let Some(h) = ctx.ui.get_child_recursive(me, open) {
                    ctx.ui.queue_set_visible(h, true);
                }
            }
            return R::Default;
        }
        // The panel opens a tab on **0x19** (mouse
        // click) or **0x29** (activated), never on a button message (compare
        // [`msgid::BUTTON_CLICKED`]).
        if m.id == msgid::MOUSE_CLICK || m.id == msgid::ACTIVATED {
            if self.open_tab(ctx, m.source_id) {
                return R::StopProcessing;
            }
            return R::Default;
        }
        // A direct registered page controls the tabbed container's visibility too. The
        // retail listener checks that the page's parent is this panel, reads the child's
        // current local-visible bit, then sets this panel visible iff its
        // selected page is locally visible. Without that tail F8/F9 (and F3-F6) only show
        // a page inside a hidden parent. Read live flags, not a potentially queued p1.
        if m.id == msgid::VISIBILITY_CHANGED
            && ctx
                .ui
                .node(m.source)
                .is_some_and(|n| n.region.parent == Some(me))
        {
            if let Some(tab) = self.page_to_tab.get(&m.source_id).copied() {
                if ctx
                    .ui
                    .node(m.source)
                    .is_some_and(|n| n.region.flags.visible)
                {
                    self.open_tab(ctx, tab);
                }
                let visible = self
                    .open_page
                    .and_then(|page| ctx.ui.get_child_recursive(me, page))
                    .and_then(|h| ctx.ui.node(h))
                    .is_some_and(|n| n.region.flags.visible);
                // Match the existing self-visibility arm's deferred writes: finish this
                // child's serial before broadcasting the parent's own visibility notice.
                ctx.ui.queue_set_visible(me, visible);
            }
        }
        R::Default
    }
}
