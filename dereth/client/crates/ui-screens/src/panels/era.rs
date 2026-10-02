//! What the world's era lacks, taken off the screens.
//!
//! The screens are the end-of-retail ones whatever era the world plays, so a system the era does
//! not have would otherwise show as an empty tab. The era is the one the server announced, else
//! the one the data files imply ([`crate::view::EraView`]); its features
//! ([`dereth_primitives::EraFeatures`]) say what to hide:
//!
//! | feature | what goes |
//! |---|---|
//! | `contracts` | the quest page's Contracts tab (the page opens on its next tab instead) |
//! | `titles` | the character page's Titles tab (likewise) |
//! | `cloaks` | the paper doll's cloak slot |
//! | `trinkets` | the paper doll's trinket slot |
//! | `luminance` | the character sheet's luminance section (see [`super::characterinfo`]) |
//! | `housing` | the map page's House tab (the page opens on its Map tab instead) |
//!
//! A page that loses a tab keeps no gap in its strip: its other tabs share the strip in equal
//! shares, and get their own places back when the tab returns.
//!
//! The gameplay screen takes away whole pages and their toolbar buttons, and refuses to show them
//! whatever asks (`GamePlayScreen::apply_era`, `era_lacks_page`):
//!
//! | feature | what goes |
//! |---|---|
//! | `journal` | the quest page and its toolbar button |
//! | `trade` | the secure-trade window |
//! | `tinkering` | the salvage window a tinkering tool opens |
//! | `housing` | the house purchase and maintenance window a deed's slumlord opens |
//! | `chess` | the chess window |
//!
//! The aetheria sigil slots need nothing here: the paper doll hides them for every era, and a
//! character with no aetheria slots never has them restored.

use dereth_primitives::EraFeatures;
use dereth_ui::msg::element::id as msgid;
use dereth_ui::widgets::panel::Panel;
use dereth_ui::{ElemHandle, ElementId, UiSystem};

use crate::view::GameView;

/// The paper doll's cloak slot (worn location `0x08000000`).
pub const CLOAK_SLOT: ElementId = ElementId(0x1000_05E9);
/// The paper doll's trinket slot (worn location `0x04000000`).
pub const TRINKET_SLOT: ElementId = ElementId(0x1000_058E);

/// A tab an era can take away, with the tabbed page it belongs to.
#[derive(Debug, Clone, Copy)]
struct Tab {
    tab: ElemHandle,
    id: ElementId,
    page: ElemHandle,
}

/// One tab of a tabbed page's strip, with its own box and its parts' (the end caps and the
/// middle) as the layout put them.
#[derive(Debug, Clone)]
struct StripTab {
    tab: ElemHandle,
    own: dereth_ui::Box2D,
    parts: Vec<(ElemHandle, dereth_ui::Box2D)>,
}

/// The tab strip of a page an era can take a tab from. The shown tabs share the strip: a hidden
/// tab leaves no gap, and the others grow over it.
#[derive(Debug, Clone)]
struct Strip {
    tabs: Vec<StripTab>,
}

impl Strip {
    /// The strip of the tabbed page `page`, its tabs left to right.
    fn of(ui: &UiSystem, page: ElemHandle) -> Option<Self> {
        let p = panel(ui, page)?;
        let mut tabs: Vec<StripTab> = p
            .tab_to_page
            .keys()
            .filter_map(|&id| ui.get_child_recursive(page, id))
            .filter_map(|tab| {
                let own = ui.node(tab)?.region.box_;
                let parts = ui
                    .children(tab)
                    .into_iter()
                    .filter_map(|c| Some((c, ui.node(c)?.region.box_)))
                    .collect();
                Some(StripTab { tab, own, parts })
            })
            .collect();
        tabs.sort_by_key(|t| t.own.x0);
        Some(Self { tabs })
    }

    /// Lay the shown tabs out across the strip in equal shares, each tab's end caps keeping their
    /// width and its middle taking the rest; with every tab shown each takes its own box back.
    fn lay_out(&self, ui: &mut UiSystem) {
        let shown: Vec<&StripTab> = self
            .tabs
            .iter()
            .filter(|t| ui.node(t.tab).is_some_and(|n| n.region.flags.visible))
            .collect();
        if shown.len() == self.tabs.len() {
            for t in &self.tabs {
                place(ui, t.tab, t.own);
                for &(c, b) in &t.parts {
                    place(ui, c, b);
                }
            }
            return;
        }
        let (Some(x0), Some(x1)) = (
            self.tabs.iter().map(|t| t.own.x0).min(),
            self.tabs.iter().map(|t| t.own.x1 + 1).max(),
        ) else {
            return;
        };
        let n = i32::try_from(shown.len()).unwrap_or(i32::MAX).max(1);
        for (i, t) in (0i32..).zip(shown) {
            let left = x0 + (x1 - x0) * i / n;
            let right = x0 + (x1 - x0) * (i + 1) / n;
            let width = right - left;
            place(
                ui,
                t.tab,
                dereth_ui::Box2D::from_xywh(left, t.own.y0, width, t.own.height()),
            );
            let own_width = t.own.width();
            for &(c, b) in &t.parts {
                let (x, w) = (b.x0, b.width());
                let (x, w) = if x == 0 && w < own_width {
                    // The left cap.
                    (x, w)
                } else if x > 0 && x + w == own_width {
                    // The right cap.
                    (width - w, w)
                } else {
                    // The middle keeps its margins.
                    (x, (width - x - (own_width - x - w)).max(0))
                };
                place(ui, c, dereth_ui::Box2D::from_xywh(x, b.y0, w, b.height()));
            }
        }
    }
}

/// Move and size `h` to `b`, in its parent's coordinates.
fn place(ui: &mut UiSystem, h: ElemHandle, b: dereth_ui::Box2D) {
    ui.move_to(h, b.x0, b.y0);
    ui.resize_to(h, b.width(), b.height());
}

/// The era's features, applied to the screen once each time they change.
#[derive(Debug, Default)]
pub struct EraPanels {
    /// The quest page's Contracts tab ([`super::contracts::TAB`]).
    contracts_tab: Option<Tab>,
    /// The character page's Titles tab: the tab of [`super::titles::PANEL`].
    titles_tab: Option<Tab>,
    /// The map page's House tab: the tab of [`super::house::PANEL`].
    house_tab: Option<Tab>,
    /// [`CLOAK_SLOT`].
    cloak_slot: Option<ElemHandle>,
    /// [`TRINKET_SLOT`].
    trinket_slot: Option<ElemHandle>,
    /// The tab strips of the pages above, which close up over a hidden tab.
    strips: Vec<Strip>,
    /// The features last applied; `None` before the first frame.
    applied: Option<EraFeatures>,
}

fn panel(ui: &UiSystem, page: ElemHandle) -> Option<&Panel> {
    ui.node(page)?
        .behaviour
        .as_ref()?
        .as_any()?
        .downcast_ref::<Panel>()
}

impl EraPanels {
    /// Find the elements an era can take away.
    pub fn post_init(&mut self, ui: &UiSystem, root: ElemHandle) {
        self.contracts_tab = ui
            .get_child_recursive(root, super::contracts::TAB)
            .and_then(|tab| {
                Some(Tab {
                    tab,
                    id: super::contracts::TAB,
                    page: ui.node(tab)?.region.parent?,
                })
            });
        self.titles_tab = Self::tab_of(ui, root, super::titles::PANEL);
        self.house_tab = Self::tab_of(ui, root, super::house::PANEL);
        self.cloak_slot = ui.get_child_recursive(root, CLOAK_SLOT);
        self.trinket_slot = ui.get_child_recursive(root, TRINKET_SLOT);
        let mut pages: Vec<ElemHandle> = Vec::new();
        for t in [self.contracts_tab, self.titles_tab, self.house_tab]
            .into_iter()
            .flatten()
        {
            if !pages.contains(&t.page) {
                pages.push(t.page);
            }
        }
        self.strips = pages.into_iter().filter_map(|p| Strip::of(ui, p)).collect();
        self.applied = None;
    }

    /// The tab that opens the sub-panel `sub`: the tabbed page among `sub`'s ancestors that pairs
    /// a tab with it.
    fn tab_of(ui: &UiSystem, root: ElemHandle, sub: ElementId) -> Option<Tab> {
        let mut at = ui.node(ui.get_child_recursive(root, sub)?)?.region.parent;
        while let Some(page) = at {
            if let Some(&id) = panel(ui, page).and_then(|p| p.page_to_tab.get(&sub)) {
                let tab = ui.get_child_recursive(page, id)?;
                return Some(Tab { tab, id, page });
            }
            at = ui.node(page)?.region.parent;
        }
        None
    }

    /// The features the screen was last made to show.
    #[must_use]
    pub fn applied(&self) -> Option<EraFeatures> {
        self.applied
    }

    /// Show what the era has and hide what it lacks, when the era's features changed since the
    /// last frame. A view with no era shows everything. Returns whether anything was applied.
    pub fn update(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        let features = view.era().map_or(EraFeatures::ALL, |e| e.features());
        if self.applied == Some(features) {
            return false;
        }
        self.applied = Some(features);
        for (tab, has) in [
            (self.contracts_tab, features.contracts),
            (self.titles_tab, features.titles),
            (self.house_tab, features.housing),
        ] {
            let Some(tab) = tab else { continue };
            ui.set_visible(tab.tab, has);
            ui.set_mouse_visible(tab.tab, has);
            if !has {
                Self::leave_tab(ui, tab);
            }
        }
        for strip in &self.strips {
            strip.lay_out(ui);
        }
        for (slot, has) in [
            (self.cloak_slot, features.cloaks),
            (self.trinket_slot, features.trinkets),
        ] {
            let Some(slot) = slot else { continue };
            ui.set_visible(slot, has);
            ui.set_mouse_visible(slot, has);
        }
        true
    }

    /// When the tab's page is open on it, open the page's next tab instead, as a click on that
    /// tab would.
    fn leave_tab(ui: &mut UiSystem, tab: Tab) {
        let (open, other) = {
            let Some(panel) = panel(ui, tab.page) else {
                return;
            };
            (
                panel.open_tab,
                panel.tab_to_page.keys().copied().find(|&t| t != tab.id),
            )
        };
        if open != Some(tab.id) {
            return;
        }
        let Some(other) = other.and_then(|t| ui.get_child_recursive(tab.page, t)) else {
            return;
        };
        ui.broadcast_element_message(other, msgid::MOUSE_CLICK, 0, 0);
    }
}
