//! `SpellcastingPanel` — the spell bar, and **the only thing in the client that casts a spell**.
//!
//! This module is the production caller of `cast_spell`, and through it of
//! `get_appropriate_spell_formula`.
//!
//! # Where the cast comes from, measured rather than assumed
//!
//! In retail there is **exactly one** caller of the cast: the spellcasting panel's cast step.
//! That in turn has three callers: the cast-current-spell notice, the cast-quickslot-spell notice
//! and the element-message handler. Nothing else in the client casts.
//!
//! **The spellbook is not one of them.** The spellbook's double-click arm (first message
//! parameter `10`) raises the add-spell-shortcut notice — it puts the spell **on this bar**.
//! Casting from the spellbook is one step longer than it looks.
//!
//! # The panel
//!
//! The panel's post-init binds the bar panel (`0x100000A2`), the **background**
//! (`0x100000A0`), the cast button (`0x100000B2`), the endowment icon (`0x100000B1`)
//! and then sets up the eight sub-menus, one per tab. Each sub-menu set-up finds the page's item
//! list at the *same* child id `0x100000B6` in every one of the eight.
//!
//! # The sub-menu set-up's arguments, and the drop target
//!
//! The sub-menu set-up takes **four** arguments (parent, page id, tab element id, tab index) and
//! stores three of them: it finds the page under the parent, finds the item list `0x100000B6`
//! under the page, and registers the panel as that list's drag handler. That registration is the
//! spell bar's drop handler; without it nothing can be dropped on the bar.
//!
//! `0x100000A3`…`0x100000A9`, `0x100005C2` is the **third** argument, it becomes the sub-menu's
//! tab element, and the drop
//! handler matches the drop target against all eight of them — that is how
//! a spell dropped on a *tab* reaches a tab that is not the open one. The fourth argument is the
//! plain tab index `0`…`7`, which is the favourite-spells list's bank number and goes
//! out on the wire in `0x01E3`/`0x01E4`. See [`SUB_MENU_TAB_BUTTONS`].
//!
//! # The transfer
//!
//! There are two ways to put a spell on the bar. `SpellbookPanel`'s double-click (the
//! element-message handler, first message parameter `10`) raises
//! the add-spell-shortcut notice, which lands here; and message `0x15` reaches the drop
//! handling. Both funnel into the spell-cast sub-menu's favorite insert,
//! which is [`SpellcastingPanel::add_favorite`].
//!
//! # The open sub menu index read
//!
//! A `switch` on the bar panel's open-page token mapping `0x100000AB`…`0x100000B0` to 1…6 and
//! `0x100005C3` to 7, with **`0x100000AA` absent** — it falls into the default `return 0`, which
//! is the same answer, and so does an unbound panel. Reproduced including the fall-through.

use dereth_primitives::ObjectId;
use dereth_ui::{ElemHandle, ElementId, UiSystem};

use crate::items::widget::ItemListWidget;
use crate::view::{GameView, MagicNotice, SpellEntry, UiRequest};

/// Which tab the next/previous/first/last spell-tab notices ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabJump {
    /// The next spell tab notice -> the next tab id read.
    Next,
    /// The prev spell tab notice -> the prev tab id read.
    Prev,
    /// The first spell tab notice -> open tab `0x100000A3`, a literal.
    First,
    /// The last spell tab notice -> open tab `0x100005C2`, a literal.
    Last,
}

/// `SpellcastingPanel` itself — element type `0x10000015`, the `<ENVP>` spell bar.
pub const PANEL: ElementId = ElementId(0x1000_00A2);

/// The cast button — bound in the post-init, and the element-message handler's message-`1`
/// case.
pub const CAST_BUTTON: ElementId = ElementId(0x1000_00B2);

/// The endowment icon, bound in the post-init.
pub const ENDOWMENT_ICON: ElementId = ElementId(0x1000_00B1);

/// The endowment icon's underlay — **a child of [`ENDOWMENT_ICON`], not of the panel**.
///
/// `panels/catalogue.rs` lists it as an underlay beside the panel's own children; the post-init
/// looks it up *inside* the endowment icon:
///
/// Find endowment icon `0x100000B1`. If present, look up its children `0x10000453`
/// (overlay), `0x10000452` (underlay), and `0x10000454` (selected), in that order;
/// then make the icon mouse-visible and hide it. A missing icon skips all these steps.
///
/// The underlay carries the **spell's** picture — the endowed spell's icon, alpha-blitted —
/// and the overlay the **item's** drag icon. They are not a hint pair and they
/// are nothing to do with the spell-bar rows.
pub const ENDOWMENT_UNDERLAY: ElementId = ElementId(0x1000_0452);
/// The endowment icon's overlay — the wand's own drag icon, over the spell picture. See
/// [`ENDOWMENT_UNDERLAY`].
pub const ENDOWMENT_OVERLAY: ElementId = ElementId(0x1000_0453);
/// The endowment icon's selected ring, shown exactly while the open sub-menu's
/// endowment-selected flag is set. See [`ENDOWMENT_UNDERLAY`].
///
/// The selected overlay is visible exactly when the open submenu's endowment-selected
/// flag is set; otherwise it is hidden.
pub const ENDOWMENT_SELECTED: ElementId = ElementId(0x1000_0454);

/// The spell-name caption under the bar, bound in the post-init and required to be a text
/// element.
///
/// `0x0C` is `TextElement`, and the element's only writers are the cast-button tooltip update's
/// text clear and text set. See [`SpellcastingPanel::update_spell_name`] for what it
/// carries.
pub const SPELL_NAME: ElementId = ElementId(0x1000_048B);

/// The bar's background, bound in the post-init.
///
/// The drop handler compares the drop target against exactly this handle: a spell
/// let go anywhere on the bar that is not a row and not a tab appends to the **open** tab.
pub const SPELLCAST_BACKGROUND: ElementId = ElementId(0x1000_00A0);

/// The eight tab **pages**, in the post-init's sub-menu set-up order. Index 7's id sits far
/// from the other seven, like `SpellbookPanel`'s Void filter and level-8 button.
pub const SUB_MENU_PAGES: [u32; 8] = [
    0x1000_00AA,
    0x1000_00AB,
    0x1000_00AC,
    0x1000_00AD,
    0x1000_00AE,
    0x1000_00AF,
    0x1000_00B0,
    0x1000_05C3,
];

/// Each sub-menu's tab element — the sub-menu set-up's **third** argument, and a drop target.
/// See this module's header.
pub const SUB_MENU_TAB_BUTTONS: [u32; 8] = [
    0x1000_00A3,
    0x1000_00A4,
    0x1000_00A5,
    0x1000_00A6,
    0x1000_00A7,
    0x1000_00A8,
    0x1000_00A9,
    0x1000_05C2,
];

/// Each sub-menu's item list — the **same** child id inside every one of the eight pages.
pub const SUB_MENU_LIST: ElementId = ElementId(0x1000_00B6);

/// How many sub-menus (tabs) the spell bar has.
pub const SUB_MENUS: usize = 8;

/// Element-message ids this panel switches on.
pub mod msg {
    /// The item-list message `SpellcastingPanel` and `SpellbookPanel` both key off.
    pub const ITEM_LIST: u32 = 0x1C;
    /// `Button`'s clicked message — the cast button's case.
    pub const BUTTON_CLICKED: u32 = 1;
    /// `0x15` — a drag was let go over one of this panel's elements. The element-message handler
    /// passes it to the drop handling.
    pub const DROP_RELEASE: u32 = 0x15;
    /// `0x21` — the drag manager refused; the owning `ItemListWidget` starts its own.
    /// This is the message that makes a spell pickable up at all.
    pub const DRAG_REJECTED: u32 = 0x21;
    /// `0x3E` — the drag cursor entered (first parameter `1`) or left (`0`) a drop catcher.
    /// The drag manager is its only producer, and the client's arm for it is
    /// what reaches the spell-cast sub-menu's item list drag over handler.
    pub const DRAG_CURSOR_OVER: u32 = 0x3E;
}

/// The element message's first parameter inside a [`msg::ITEM_LIST`].
pub mod item_action {
    /// Single click: select.
    pub const SELECT: u32 = 7;
    /// Right-click / examine — the endowment icon's arm.
    pub const EXAMINE: u32 = 8;
    /// Double click: **cast**.
    pub const DOUBLE_CLICK: u32 = 10;
}

/// The equipment location (`0x1000000`) the client reads the endowment from.
///
/// It lives in [`dereth_client_contract::panels::spellcasting`], because `dereth_client::hud` is
/// what resolves it against the object table.
pub use dereth_client_contract::panels::spellcasting::ENDOWMENT_LOCATION;

/// The endowment icon's tooltip, after the `"<item> (<spell>)"` pair.
///
/// The retail format is `L"%s (%hs)\nDouble-click to cast this spell"` —
/// verbatim, newline included.
pub const ENDOWMENT_TOOLTIP_TAIL: &str = "\nDouble-click to cast this spell";

/// The two states the cast-button tooltip update puts the **Cast button** in, and the eight
/// tooltip formats it chooses between.
///
/// Every format below is verbatim. `%s` is a wide string and `%hs` a narrow one: the **item** names come through
/// the wide object-name lookup, the **spell** names through the spell's narrow name. That is why the two families of strings differ by
/// a *the*: `"USE the %s"` against `"CAST %hs"`.
pub mod cast_button {
    use dereth_ui::StateId;

    /// State `0x0D` — the state the function opens with, every time, before it knows
    /// anything. The greyed-out Cast button.
    pub const DISABLED: StateId = StateId(0x0000_000D);
    /// State `1` — the four arms on which the
    /// player can press Cast *right now*. Two are the endowment's and two the selected spell's.
    pub const ENABLED: StateId = StateId(0x0000_0001);

    /// No endowment, nothing selected, and not one favourite on any of the eight
    /// tabs (the client checks the endowment flag, then the sum of every tab's spell count).
    pub const NO_SPELLS: &str = "You have no spells ready to cast";
    /// The same arm with something on the bar.
    pub const SELECT_A_SPELL: &str = "Select a spell to cast";
    /// The endowed item is castable now.
    pub const USE_THE: &str = "USE the %s";
    /// Prompt shown when the endowed item needs a target.
    pub const ITEM_NEEDS_TARGET: &str = "You must select a target for the %s";
    /// Prompt shown when the endowed item needs a better target. **The newline is the client's**,
    /// in the middle of the sentence.
    pub const ITEM_NEEDS_BETTER_TARGET: &str = "You must select an appropriate\ntarget for the %s";
    /// The selected spell is castable now.
    pub const CAST: &str = "CAST %hs";
    /// Prompt shown when the selected spell needs a target.
    pub const SPELL_NEEDS_TARGET: &str = "You must select a target for %hs";
    /// Prompt shown when the selected spell needs a better target.
    pub const SPELL_NEEDS_BETTER_TARGET: &str = "You must select an appropriate target for %hs";
    /// **Appended** to [`USE_THE`] or [`CAST`] by
    /// a formatted append, never used alone.
    pub const ON_TARGET: &str = " on %s";

    /// A wide-string `sprintf` with one argument — the only shape any of the
    /// formats above takes.
    ///
    /// The `%s`/`%hs` distinction is a *width* marker and not a different substitution: both
    /// consume one string. Substituting the first of either is therefore exactly what the client's
    /// one-argument `sprintf` does, and keeping the retail format verbatim is what lets a station
    /// check it against retail.
    #[must_use]
    pub fn sprintf1(fmt: &str, arg: &str) -> String {
        if let Some(i) = fmt.find("%hs") {
            let mut s = String::with_capacity(fmt.len() + arg.len());
            s.push_str(&fmt[..i]);
            s.push_str(arg);
            s.push_str(&fmt[i + 3..]);
            return s;
        }
        match fmt.find("%s") {
            Some(i) => {
                let mut s = String::with_capacity(fmt.len() + arg.len());
                s.push_str(&fmt[..i]);
                s.push_str(arg);
                s.push_str(&fmt[i + 2..]);
                s
            }
            None => fmt.to_owned(),
        }
    }
}

/// One spell-bar sub-menu (tab), reduced to what the cast path reads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SubMenu {
    /// The tab's item list, once the sub-menu set-up found it.
    pub list: Option<ElemHandle>,
    /// The tab element — the sub-menu set-up's third argument, resolved. This is the element the
    /// drop handler sweeps the eight sub-menus looking for.
    pub tab_element: Option<ElemHandle>,
    /// The selected spell id.
    pub selected_spell: u32,
    /// Whether the endowment, not a spell row, is selected on this tab.
    pub endowment_selected: bool,
}

/// `SpellcastingPanel`, as much of it as decides what a click casts.
#[derive(Debug, Default)]
pub struct SpellcastingPanel {
    /// The bar panel.
    pub panel: Option<ElemHandle>,
    /// The cast button.
    pub cast_button: Option<ElemHandle>,
    /// The endowment icon.
    pub endowment_icon: Option<ElemHandle>,
    /// The endowment icon's underlay; see [`ENDOWMENT_UNDERLAY`].
    pub endowment_underlay: Option<ElemHandle>,
    /// The endowment icon's overlay.
    pub endowment_overlay: Option<ElemHandle>,
    /// The endowment icon's selected ring.
    pub endowment_ring: Option<ElemHandle>,
    /// The spell-name caption.
    pub spell_name: Option<ElemHandle>,
    /// What [`Self::update_spell_name`] last wrote into [`Self::spell_name`], so a station can
    /// tell "the caption says nothing" from "the caption was never written" and so the write is
    /// skipped on the frames where nothing moved. Retail has no such memo — it re-runs the
    /// cast-button tooltip update from all sixteen of its entry points — but it also does not
    /// run per frame, and this projection does.
    pub caption: Option<String>,
    /// What [`Self::update_cast_button_tooltip`] last wrote on the cast button, memoised for
    /// the same reason as [`Self::caption`]. `None` is the tooltip-clearing arm.
    pub cast_tooltip: Option<String>,
    /// Whether the last [`Self::update_cast_button_tooltip`] left the button in
    /// [`cast_button::ENABLED`] rather than [`cast_button::DISABLED`]. Part of the memo, so a
    /// frame that changes only the state still writes.
    pub cast_enabled: Option<bool>,
    /// The bar's background.
    pub background: Option<ElemHandle>,
    /// The eight sub-menus.
    pub sub_menus: Vec<SubMenu>,
    visible_tabs: Option<usize>,
    /// The eight item-list consumers, in the same order as [`Self::sub_menus`].
    pub lists: Vec<Option<ItemListWidget>>,
    /// Snapshot guard for PlayerDesc/PlayerModule spell changes; never reorder the favorites.
    last_tabs: Vec<Option<Vec<SpellEntry>>>,
    /// Favourites [`Self::update`]'s is-spell-known sweep has already asked the host to remove,
    /// per tab — see that method for why the ask has to be remembered here and cannot be
    /// remembered in the module, which this crate does not own.
    pruned: Vec<Vec<u32>>,
    /// The endowed item's id.
    pub endowment_item: Option<ObjectId>,
    /// The endowed item's spell id.
    pub endowment_spell: u32,
    /// Whether an endowed item is equipped.
    pub endowment_present: bool,
    /// How many [`Self::cast`] calls reached [`UiRequest::CastSpell`]. A denominator, so that a
    /// test can tell "cast and nothing was selected" from "the panel never ran".
    pub casts: u32,
    /// How many [`Self::add_favorite`] calls reached [`UiRequest::AddSpellFavorite`]. The same
    /// kind of denominator as [`Self::casts`], and the one that separates "the drop was refused"
    /// from "the drop never arrived".
    pub favorites_added: u32,
    /// How many favourites [`Self::update`]'s is-spell-known sweep took off a tab: the counter
    /// that separates "the spell is hidden because the join dropped it"
    /// from "the spell was pruned and the shard was told", which look identical on screen.
    pub favorites_pruned: u32,
}

impl SpellcastingPanel {
    /// The spellcasting panel's post-init, bindings and ItemList initialization.
    ///
    /// `root` is the gameplay screen root: the bar lives inside the `<ENVP>` environment window
    /// like `VendorPanel` does, not on a toolbar page, so the search starts at the root.
    pub fn post_init(&mut self, ui: &mut UiSystem, root: ElemHandle) {
        self.panel = ui.get_child_recursive(root, PANEL);
        self.cast_button = ui.get_child_recursive(root, CAST_BUTTON);
        // The client resolves the spell-name caption as a text element, before
        // the background and the two buttons; a child that is not one leaves the field null,
        // which is why every writer below is `if let Some`.
        self.spell_name = ui.get_child_recursive(root, SPELL_NAME);
        self.endowment_icon = ui.get_child_recursive(root, ENDOWMENT_ICON);
        // The three sub-elements are looked up **inside the endowment icon**, and an icon the
        // layout does not carry skips all three, the mouse-visible and the hide. See
        // [`ENDOWMENT_UNDERLAY`] for the lookup order.
        self.endowment_overlay = None;
        self.endowment_underlay = None;
        self.endowment_ring = None;
        if let Some(icon) = self.endowment_icon {
            self.endowment_overlay = ui.get_child_recursive(icon, ENDOWMENT_OVERLAY);
            self.endowment_underlay = ui.get_child_recursive(icon, ENDOWMENT_UNDERLAY);
            self.endowment_ring = ui.get_child_recursive(icon, ENDOWMENT_SELECTED);
            // the icon can be clicked and dragged onto from the first frame, and starts hidden.
            // Without the hide, a layout that authors it visible shows an empty frame beside the
            // Cast button on every character with no wand.
            ui.set_mouse_visible(icon, true);
            ui.set_visible(icon, false);
        }
        self.caption = None;
        self.cast_tooltip = None;
        self.cast_enabled = None;
        // The post-init binds the background and, if found, makes it
        // mouse-visible. That is not decoration: an element the pointer cannot see is an element
        // a drag cannot be released over, so without it the client's drop target is inert.
        self.background = ui.get_child_recursive(root, SPELLCAST_BACKGROUND);
        if let Some(h) = self.background {
            ui.set_mouse_visible(h, true);
        }
        self.sub_menus = (0..SUB_MENUS).map(|_| SubMenu::default()).collect();
        self.lists = (0..SUB_MENUS).map(|_| None).collect();
        self.last_tabs = vec![None; SUB_MENUS];
        self.pruned = vec![Vec::new(); SUB_MENUS];
        self.endowment_item = None;
        self.endowment_spell = 0;
        self.endowment_present = false;
        let Some(panel) = self.panel else { return };
        for (i, page_id) in SUB_MENU_PAGES.iter().enumerate() {
            // Binding the tab element is the *last* step of the sub-menu set-up and is **not**
            // under the page — it hangs off the panel, and it is bound even when the page is
            // missing, because the set-up's page miss only skips the item-list half. Reproduced in
            // that order for that reason.
            self.sub_menus[i].tab_element =
                ui.get_child_recursive(panel, ElementId(SUB_MENU_TAB_BUTTONS[i]));
            // The set-up finds the page under the panel, then the item list under the page. A page
            // it cannot find leaves the item list untouched, i.e. null.
            let Some(page) = ui.get_child_recursive(panel, ElementId(*page_id)) else {
                continue;
            };
            self.sub_menus[i].list = ui.get_child_recursive(page, SUB_MENU_LIST);
            self.lists[i] = self.sub_menus[i].list.map(|h| ItemListWidget::init(ui, h));
        }
    }

    /// The sub-menu's update from the player module: flush, refill in favorite-list
    /// order, restore the selected ID, then refresh shortcut numerals. The production frame
    /// observes the existing player snapshot instead of repeating this on unchanged frames.
    ///
    /// The current GameView spellbook is known spells joined with SpellTable metadata. This
    /// projects favorites present in that join; a known ID without metadata still has no row,
    /// because neither a fabricated icon nor a fabricated name is substituted.
    ///
    /// ## The prune
    ///
    /// Between the flush and the refill, retail walks the favourites list and **deletes every
    /// entry the character does not know**, telling the shard about each one:
    ///
    /// Start an empty removal list and walk the favorite spell ids. Keep spells known by
    /// the player; for unknown spells, allocate an eight-byte `(id, next)` node and link it
    /// into the removal list. After the walk, free those nodes while removing each spell
    /// from the player module, sending `0x01E4`.
    ///
    /// It is collected first and removed second because the removal unlinks the very node the
    /// walk is standing on. That removal is the player-module removal **plus**
    /// the `RemoveSpellFavorite` request — so this is where
    /// a spell deleted from the book leaves the bar *persistently*, and it is the only writer of
    /// that edit. Without it the bar hides the dead favourite (the join drops it) while
    /// `client_packed_module` keeps re-packing it into the next `0x01A1`, and the row comes back on
    /// the next login.
    ///
    /// The predicate is [`GameView::is_spell_known`] and **not** the `book` join below, for the
    /// reason that method's own documentation gives: the join is narrower than the spellbook, and
    /// the difference would be an unrequested `0x01E4` for a spell the player still has.
    pub fn update(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        let count = view.era_ui().spell_favorite_tabs;
        let era_changed = self.visible_tabs != Some(count);
        self.visible_tabs = Some(count);
        for (i, sub) in self.sub_menus.iter().enumerate() {
            if let Some(tab) = sub.tab_element {
                ui.set_visible(tab, i < count);
                ui.set_mouse_visible(tab, i < count);
            }
        }
        if self.open_sub_menu_index(ui) >= count {
            self.open_spell_tab(ui, TabJump::First);
        }
        let book: std::collections::BTreeMap<_, _> =
            view.spellbook().iter().map(|s| (s.id, s)).collect();
        let mut changed = era_changed;
        for tab in 0..self.lists.len().min(count) {
            // The update-from-player-module's first pass, ahead of the refill and ahead of the
            // unchanged-rows shortcut: a tab whose *rows* are unchanged (both entries were
            // already invisible) can still owe the shard a removal.
            let ids = view.spell_tab(tab);
            // Retail unlinks the node *inside* this call, so it asks once and the next call
            // cannot see the entry again. Here the module belongs to the host and the ask is a
            // queued `UiRequest`, while this projection re-runs every frame — so the ask is
            // remembered until the id really leaves the tab. Without that, a frame that ran
            // before the host drained the queue would put a second `0x01E4` on the wire, and so
            // would every frame after it.
            if let Some(asked) = self.pruned.get_mut(tab) {
                asked.retain(|id| ids.contains(id));
            }
            for id in ids {
                if view.is_spell_known(*id) {
                    continue;
                }
                match self.pruned.get_mut(tab) {
                    Some(asked) if asked.contains(id) => continue,
                    Some(asked) => asked.push(*id),
                    None => {}
                }
                ui.requests
                    .emit(UiRequest::RemoveSpellFavorite { spell_id: *id, tab });
                self.favorites_pruned += 1;
            }
            let rows: Vec<_> = view
                .spell_tab(tab)
                .iter()
                .filter_map(|id| book.get(id).map(|s| (*s).clone()))
                .collect();
            if self.last_tabs[tab].as_ref() == Some(&rows) {
                continue;
            }
            let Some(w) = self.lists[tab].as_mut() else {
                continue;
            };
            w.set_spells(ui, &rows);
            self.set_selected(ui, tab, self.sub_menus[tab].selected_spell);
            // The shortcut-overlay update: first NINE rows, including empty padding;
            // indices 9 and beyond get -1. These are zero-based shortcut numbers.
            if let Some(w) = self.lists[tab].as_mut() {
                for (i, slot) in w.slots.iter_mut().enumerate() {
                    let num = if i < 9 {
                        i32::try_from(i).unwrap_or(-1)
                    } else {
                        -1
                    };
                    slot.set_shortcut_num(ui, num, false);
                }
            }
            self.last_tabs[tab] = Some(rows);
            changed = true;
        }
        changed
    }

    /// True once [`Self::post_init`] found the panel. The denominator for everything below.
    #[must_use]
    pub fn bound(&self) -> bool {
        self.panel.is_some()
    }

    /// How many of the eight tabs have their item list — a second denominator, because a panel
    /// that bound but found no lists looks exactly like a panel that never bound.
    #[must_use]
    pub fn lists_bound(&self) -> usize {
        self.sub_menus.iter().filter(|s| s.list.is_some()).count()
    }

    /// The bar panel's open-page token, read two ways.
    ///
    /// The primary route is the panel's behaviour object, whose `open_page` **is** the token. The
    /// fallback is which of the eight pages is visible, because opening a page sets exactly one
    /// page visible per call and that is the only way a page becomes visible.
    ///
    /// The fallback exists for one reason and it is worth naming: a behaviour object is **lifted
    /// out of its arena slot** while its own handler runs (a widget cannot be read back out of
    /// the arena from inside its own handler), so a caller reached from inside
    /// the panel's own dispatch would see `None` and silently get tab 0. Nothing this module
    /// handles comes from the panel element itself, but the fallback makes the failure impossible
    /// rather than merely unlikely.
    #[must_use]
    fn open_page_token(&self, ui: &UiSystem) -> Option<ElementId> {
        let panel = self.panel?;
        let from_behaviour = ui
            .node(panel)
            .and_then(|n| n.behaviour.as_ref())
            .and_then(|b| b.as_any())
            .and_then(<dyn std::any::Any>::downcast_ref::<dereth_ui::widgets::panel::Panel>)
            .and_then(|p| p.open_page);
        if from_behaviour.is_some() {
            return from_behaviour;
        }
        SUB_MENU_PAGES.iter().copied().map(ElementId).find(|id| {
            ui.get_child_recursive(panel, *id)
                .and_then(|h| ui.node(h))
                .is_some_and(|n| n.region.flags.visible)
        })
    }

    /// The spellcasting panel's open sub menu index read.
    ///
    /// **`0x100000AA` is deliberately not in the client's `switch`** and falls to the default `0`,
    /// which is the answer it would have given anyway — so an unbound panel, an unknown token and
    /// tab 0 are one arm here as they are there.
    #[must_use]
    pub fn open_sub_menu_index(&self, ui: &UiSystem) -> usize {
        open_sub_menu_index_of(self.open_page_token(ui).map_or(0, |e| e.0))
    }

    /// Rows, rings, viewport, and selected ID.
    ///
    /// The client walks the list setting each row's selected state and, **on the
    /// row that matches**, clears the endowment-selected flag and scrolls it into view. The flag
    /// is therefore cleared only when the id is found in the list and is non-zero, which is why
    /// selecting `0` (the endowment arm's own call) does **not** clear it.
    pub fn set_selected(&mut self, ui: &mut UiSystem, tab: usize, spell_id: u32) {
        let Some(sub) = self.sub_menus.get_mut(tab) else {
            return;
        };
        if let Some(w) = self.lists.get_mut(tab).and_then(Option::as_mut) {
            for index in 0..w.slots.len() {
                let selected = spell_id != 0 && w.slots[index].spell == Some(spell_id);
                w.slots[index].set_selected_state(ui, selected);
                if selected {
                    sub.endowment_selected = false;
                    w.scroll_to_view(ui, index);
                }
            }
        }
        sub.selected_spell = spell_id;
    }

    // ---- the transfer ------------------------------------------------------------------------

    /// **The one function every way of putting a
    /// spell on the bar goes through**, argument for argument: the spell, the index, and a
    /// caller-stack out-parameter.
    ///
    /// In order, retail:
    ///
    /// 1. returns if there is no player module;
    /// 2. unless a move is allowed, returns if the spell is already in the tab's favourites list;
    /// 3. returns on spell `0`;
    /// 4. removes the spell's existing row, if any (sending `0x01E4`), and if that row was before
    ///    the destination index, moves the index down by one;
    /// 5. inserts the row and bumps the tab's spell count;
    /// 6. returns if there is no magic system (no wire, no model); otherwise sets the row's
    ///    tooltip to the spell name;
    /// 7. turns an index of `-1` into the spell count;
    /// 8. adds the spell to the player module (sending `0x01E3`), selects it and refreshes the
    ///    shortcut numerals.
    ///
    /// Three things in there are load-bearing and none of them is obvious.
    ///
    /// **`move_allowed` inverts the duplicate check.** `false` — the double-click's value —
    /// refuses a spell the tab already has. `true` — every drag's — skips the check, so
    /// the menu removal takes the row out and the insert puts it back somewhere else. That
    /// *is* how a spell is reordered within a tab, and it costs two messages: `0x01E4` then
    /// `0x01E3`.
    ///
    /// **The duplicate check reads the player-module list, not the rows.** They can differ: a
    /// favourite the player no longer knows is in the module and not in the list. So the two
    /// sources are kept apart here as well — `view.spell_tab(tab)` for the check,
    /// `Self::list_contents` for the menu removal and for the spell count.
    ///
    /// **The append index is the spell count *after* the increment**, i.e. one past the last row.
    /// The client walks that many nodes, runs off the end and pushes at
    /// the tail, so the list is right — but the number on the wire is `n+1`, and that is what the
    /// shard persists. Reproduced rather than corrected.
    ///
    /// Returns whether anything was raised.
    pub fn add_favorite(
        &mut self,
        ui: &mut UiSystem,
        view: &dyn GameView,
        tab: usize,
        spell_id: u32,
        mut index: i32,
        move_allowed: bool,
    ) -> bool {
        if tab >= self.sub_menus.len().min(view.era_ui().spell_favorite_tabs) {
            return false;
        }
        if !move_allowed && view.spell_tab(tab).contains(&spell_id) {
            return false;
        }
        if spell_id == 0 {
            return false;
        }
        // The spell count before anything moves — the client keeps it as a counter, this reads
        // the rows it counts (the refill sets it to exactly this).
        let mut num_spells = self.list_contents(tab).len();
        if let Some(old) = self.remove_spell_from_menu(ui, tab, spell_id) {
            num_spells -= 1;
            if i32::try_from(old).unwrap_or(i32::MAX) < index {
                index -= 1;
            }
        }
        // The insert and the count increment. The row itself follows from the model:
        // the next `update` rebuilds the tab out of `GameView::spell_tab`, which is what the host
        // will have written by then. The client inserts directly because it owns the module.
        num_spells += 1;
        if index == -1 {
            index = i32::try_from(num_spells).unwrap_or(i32::MAX);
        }
        ui.requests.emit(UiRequest::AddSpellFavorite {
            spell_id,
            index,
            tab,
        });
        self.set_selected(ui, tab, spell_id);
        self.favorites_added += 1;
        true
    }

    /// Find the spell among the **rows** and,
    /// if it is there, take it out of the module and tell the shard.
    ///
    /// Spell `0` answers "not found". Otherwise retail walks the rows, skipping any that is not
    /// a spell row or not in the filled state `0x1000001D` (padding), and on the first row
    /// carrying the spell: decrements the spell count, removes the spell from the player module
    /// (sending `0x01E4`), refills the tab, clears the selection if that spell was selected, and
    /// returns the row index.
    ///
    /// The `0x1000001D` test is why the padding cells are skipped rather than counted, and the
    /// returned index is the **row** number — which is what `add_favorite`'s `old < index` compare
    /// then uses to shift a move's destination down by one.
    ///
    /// Returns the row it removed, or `None`.
    pub fn remove_spell_from_menu(
        &mut self,
        ui: &mut UiSystem,
        tab: usize,
        spell_id: u32,
    ) -> Option<usize> {
        if spell_id == 0 {
            return None;
        }
        let index = self
            .list_contents(tab)
            .iter()
            .position(|s| *s == spell_id)?;
        ui.requests
            .emit(UiRequest::RemoveSpellFavorite { spell_id, tab });
        // Clear the selection if the removed spell was the selected one — and *only* then; a removal of
        // something else leaves the selection alone.
        if self
            .sub_menus
            .get(tab)
            .is_some_and(|s| s.selected_spell == spell_id)
        {
            self.set_selected(ui, tab, 0);
        }
        Some(index)
    }

    /// `add_favorite` on the **open** tab, then
    /// the two refreshes.
    pub fn add_spell_shortcut(
        &mut self,
        ui: &mut UiSystem,
        view: &dyn GameView,
        spell_id: u32,
        index: i32,
        move_allowed: bool,
    ) -> bool {
        let tab = self.open_sub_menu_index(ui);
        let added = self.add_favorite(ui, view, tab, spell_id, index, move_allowed);
        self.update_endowment(ui, view);
        added
    }

    /// The spellcasting panel's add spell shortcut notice — **the spellbook's
    /// double-click**, and the whole of what it does: `add_favorite` on the open tab with index
    /// `-1` and moves not allowed, then the endowment-icon and cast-button-tooltip refreshes.
    ///
    /// `-1` is append and `false` is "refuse a duplicate", which together put the spell at the end
    /// of the current spell tab.
    pub fn on_add_spell_shortcut(
        &mut self,
        ui: &mut UiSystem,
        view: &dyn GameView,
        spell_id: u32,
    ) -> bool {
        self.add_spell_shortcut(ui, view, spell_id, -1, false)
    }

    /// Message `0x15`'s arm, and the drag half of the transfer.
    ///
    /// Without both a drag source and a drop target it does nothing. It reads the dragged
    /// payload's spell id, then:
    ///
    /// * target inside the open tab's item list: a zero spell or no spell row under the pointer
    ///   refuses; otherwise add the spell at that row's index, move allowed;
    /// * target is the bar's background: add the spell at the end (`-1`), move allowed;
    /// * target is one of the eight tab elements: a zero spell refuses; otherwise `add_favorite`
    ///   on *that* tab at the end, move allowed, then the endowment and tooltip refreshes.
    ///
    /// **The zero-spell guard is not in the background arm.** The client passes the spell id on
    /// without testing, so a drop of an *object* on the bar's background reaches the add with
    /// spell `0` — and `add_favorite`'s own zero-spell return is what stops it there. The two arms
    /// that do test return one call earlier.
    /// The difference is invisible and is kept anyway, because a guard the client does not have
    /// is a first step away from its behaviour.
    ///
    /// `owner` is the drag proxy — the element [`crate::items::widget::inq_drop_icon_info`] reads
    /// the payload off, which is this crate's.
    pub fn handle_drop_release(
        &mut self,
        ui: &mut UiSystem,
        view: &dyn GameView,
        target: ElemHandle,
        owner: ElemHandle,
    ) -> bool {
        let spell_id = crate::items::widget::inq_drop_icon_info(ui, owner)
            .spell
            .unwrap_or(0);
        let tab = self.open_sub_menu_index(ui);
        if let Some(w) = self.lists.get(tab).and_then(Option::as_ref) {
            if ui.is_ancestor_of(w.handle, target) {
                if spell_id == 0 {
                    return false;
                }
                // The drop's own target
                // *is* the row under the pointer — the element manager picks it the same way — so
                // the row is recovered from the target rather than from a mouse position the
                // message does not carry. A target that is the list itself and not a row is
                // the item-under-mouse lookup answering null, which is a refusal.
                let Some(index) = w.slot_of(target).or_else(|| {
                    let mut cur = ui.parent(target);
                    while let Some(h) = cur {
                        if let Some(i) = w.slot_of(h) {
                            return Some(i);
                        }
                        cur = ui.parent(h);
                    }
                    None
                }) else {
                    return false;
                };
                let index = i32::try_from(index).unwrap_or(i32::MAX);
                return self.add_spell_shortcut(ui, view, spell_id, index, true);
            }
        }
        if self.background.is_some() && self.background == Some(target) {
            return self.add_spell_shortcut(ui, view, spell_id, -1, true);
        }
        let Some(i) = self
            .sub_menus
            .iter()
            .position(|s| s.tab_element == Some(target))
        else {
            return false;
        };
        if spell_id == 0 {
            return false;
        }
        let added = self.add_favorite(ui, view, i, spell_id, -1, true);
        self.update_endowment(ui, view);
        added
    }

    /// The **state** half; the icon, the overlay blits and the tooltip are the drawing half and
    /// live in `draw_endowment_icon`.
    ///
    /// The endowment is the item wielded at [`ENDOWMENT_LOCATION`] whose `ITEM_TYPE` carries
    /// `Caster (0x8000)` and whose spell id is non-zero; the client tests the sign of the type's
    /// second byte, i.e. bit 15. The host answers both, because the item
    /// type constants live below this crate. Returns true when an endowment is present.
    pub fn update_endowment(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        let tab = self.open_sub_menu_index(ui);
        let present = match view.endowment() {
            None => {
                self.endowment_item = None;
                self.endowment_spell = 0;
                if let Some(s) = self.sub_menus.get_mut(tab) {
                    s.endowment_selected = false;
                }
                self.endowment_present = false;
                // The client hides the endowment icon **and does nothing else**. The arm clears no image and touches neither the underlay, the overlay
                // nor the ring: hiding the parent takes the whole group down with it, and the
                // stale pictures are what the next endowment overwrites.
                if let Some(h) = self.endowment_icon {
                    ui.set_visible(h, false);
                }
                false
            }
            Some((item, spell)) => {
                // "With nothing else selected the endowment selects itself" — with no spell
                // selected the client sets the endowment-selected flag and selects `0`.
                if self
                    .sub_menus
                    .get(tab)
                    .is_some_and(|s| s.selected_spell == 0)
                {
                    if let Some(s) = self.sub_menus.get_mut(tab) {
                        s.endowment_selected = true;
                    }
                    self.set_selected(ui, tab, 0);
                }
                self.endowment_item = Some(item);
                self.endowment_spell = spell;
                self.endowment_present = true;
                self.draw_endowment_icon(ui, view, tab, item, spell);
                true
            }
        };
        // Every one of the sixteen spell-bar entry points that calls
        // the endowment icon update also calls the cast-button tooltip update
        // (sixteen callers, every one calling both). The one caller of the
        // tooltip that is *not* in the endowment block is the selection-changed notice;
        // this build has no such notice and reaches the same place because
        // `RemainingPanels::update` calls this method every frame. The write itself is memoised
        // on [`Self::caption`], so an unchanged frame is the no-op the notice not arriving is.
        self.update_spell_name(ui, view, tab);
        // The caption half of the cast-button tooltip update, run from the
        // same place and for the same reason: retail computes both in one function,
        // reached from sixteen callers.
        self.update_cast_button_tooltip(ui, view, tab);
        present
    }

    /// The client's **element** half — the three children of the endowment icon.
    ///
    /// In order, retail clears the underlay image; returns if there is no magic system (before
    /// the overlay, so such a host leaves the overlay alone); sets the underlay to the endowed
    /// spell's icon, alpha-blitted, if that icon resolves; clears the overlay and sets it,
    /// alpha-blitted and unconditionally, to the item's drag icon; shows the selected ring
    /// exactly when the open tab's endowment-selected flag is set; sets the icon's tooltip to
    /// `"<item name> (<spell name>)\nDouble-click to cast this spell"`; and shows the icon.
    ///
    /// Three facts worth stating:
    ///
    /// * **the underlay is the spell and the overlay is the item**, not a hint pair. The underlay
    ///   takes the spell-icon composite — the same one a spell-bar row draws, so
    ///   [`crate::items::widget::spell_recipe`] is the right builder — and the overlay takes
    ///   the item's drag icon, i.e.
    ///   [`dereth_ui::region::IconRecipe::drag_surface`] and not the slot's own composite;
    /// * **the overlay's image is set unconditionally** while the underlay's is guarded on the
    ///   spell icon resolving. A wand whose spell has no table row therefore shows the wand over
    ///   an empty underlay rather than nothing at all;
    /// * **the alpha blit mode on both**, which is the spell-slot blit mode and not the item
    ///   slot's normal one.
    fn draw_endowment_icon(
        &mut self,
        ui: &mut UiSystem,
        view: &dyn GameView,
        tab: usize,
        item: ObjectId,
        spell: u32,
    ) {
        use dereth_ui::region::{BlitMode, SurfaceOp};
        use dereth_ui::GraphicRef;
        let base = view.spell(spell);
        if let Some(h) = self.endowment_underlay {
            let recipe = base
                .as_ref()
                .map(|b| crate::items::widget::spell_recipe(ui, b.level, b.icon, b.bitfield));
            if let Some(n) = ui.node_mut(h) {
                n.region.image = None; // Cleared first.
                if let Some(r) = recipe {
                    if let Some(did) = r.base() {
                        n.region.blit_mode = BlitMode::Alpha3;
                        n.region.image = Some(GraphicRef {
                            op: Some(SurfaceOp::Icon(r)),
                            ..GraphicRef::world_surface(did, 0, 0)
                        });
                    }
                }
            }
        }
        if let Some(h) = self.endowment_overlay {
            // The drag icon is the drag surface of the item's own composite: icon, custom
            // overlay and the effects recolour, with no item-type tile and no custom underlay.
            // `ItemSlot::prepare_drag_icon` takes the same `drag_surface()`.
            let drag = view
                .slot_decoration(item)
                .map(|d| crate::items::widget::object_recipe(ui, &d).drag_surface());
            if let Some(n) = ui.node_mut(h) {
                n.region.image = None; // Cleared first.
                n.region.blit_mode = BlitMode::Alpha3; // Unconditional in the client.
                if let Some(r) = drag {
                    if let Some(did) = r.base() {
                        n.region.image = Some(GraphicRef {
                            op: Some(SurfaceOp::Icon(r)),
                            ..GraphicRef::world_surface(did, 0, 0)
                        });
                    }
                }
            }
        }
        let selected = self
            .sub_menus
            .get(tab)
            .is_some_and(|s| s.endowment_selected);
        if let Some(h) = self.endowment_ring {
            ui.set_visible(h, selected);
        }
        if let Some(h) = self.endowment_icon {
            // The client: the tooltip is the same `"%s (%hs)"` pair the caption uses, plus the
            // second line. A spell with no table row still gets a tooltip: the format runs on
            // whatever name lookup returned, and a missing spell's name is empty.
            let name = view.name(item).unwrap_or_default().to_owned();
            let spell_name = base.as_ref().map_or("", |b| b.name.as_str());
            ui.set_tooltip(
                h,
                Some(format!("{name} ({spell_name}){ENDOWMENT_TOOLTIP_TAIL}")),
            );
            ui.set_tooltip_on(h, true);
            ui.set_visible(h, true);
        }
    }

    /// The spellcasting panel's cast button tooltip update's **spell-name half** — the
    /// caption under the spell bar.
    ///
    /// The function computes two strings and the caption is the *second* of them. It clears the
    /// caption unconditionally, first; at the end, if the tooltip is still empty it switches the
    /// cast button's tooltip off and clears it, and otherwise it sets the caption, sets the
    /// button's tooltip and switches it on.
    ///
    /// **The gate is on the tooltip, not on the caption**, and that costs nothing here: in both
    /// branches that reach the end with an empty tooltip the caption is empty too (the spell
    /// lookup missed), and the third branch — an endowment item the object table does not know —
    /// leaves before reaching the end at all. So the clear is the visible answer for all three,
    /// and the rule reduces to:
    ///
    /// | the open tab's state | caption |
    /// |---|---|
    /// | a spell selected (non-zero selected spell id) | the spell's name, alone |
    /// | no spell, a wand endowed | `"<item name> (<wand spell name>)"` |
    /// | neither | empty |
    ///
    /// **The selected spell wins over the endowment** — the client goes to the spell arm on a
    /// non-zero selected spell id, whichever way the endowment test went —
    /// which is the opposite of [`Self::cast`]'s order, where the endowment wins. The two do not
    /// conflict in practice: selecting clears the endowment-selected flag on the row it finds, so
    /// a tab whose endowment is armed has a selected spell id of `0` anyway.
    ///
    /// **The name is the `SpellTable`'s, not the spellbook's.** The lookup is a table lookup;
    /// a wand's spell is usually not in the player's book, and [`GameView::spell`] is the
    /// accessor that says so. There is **no level and no school** in the caption: the `%hs` is
    /// the spell's narrow name and nothing is appended to it. The two literals the
    /// function also builds — `"Select a spell to cast"` and `"You have no spells ready to cast"`
    /// — are the **button's tooltip** and never the caption; see
    /// [`Self::update_cast_button_tooltip`].
    ///
    /// Returns true when the caption changed.
    pub fn update_spell_name(
        &mut self,
        ui: &mut UiSystem,
        view: &dyn GameView,
        tab: usize,
    ) -> bool {
        let selected = self.sub_menus.get(tab).map_or(0, |s| s.selected_spell);
        let caption = if selected != 0 {
            // The client: the selected spell's name, widened and stored.
            // A table miss leaves the caption cleared.
            view.spell(selected).map(|b| b.name).unwrap_or_default()
        } else if let Some(item) = self.endowment_item {
            // The client: the endowment arm. A missing spell skips only the format
            // and still writes the button's tooltip, so the caption stays empty
            // rather than falling back to the item's name alone.
            view.spell(self.endowment_spell).map_or(String::new(), |b| {
                format!("{} ({})", view.name(item).unwrap_or_default(), b.name)
            })
        } else {
            String::new()
        };
        if self.caption.as_deref() == Some(caption.as_str()) {
            return false;
        }
        if let Some(t) = self.spell_name.and_then(|h| ui.text_element_mut(h)) {
            // Clear-then-set is one assignment here: `TextElement::set_text` replaces the whole
            // content, which is what the pair does.
            t.set_text(&caption);
        }
        self.caption = Some(caption);
        true
    }

    /// The spellcasting panel's cast button tooltip update's **cast-button half** — the
    /// button's two states and its tooltip.
    ///
    /// One local string is the caption ([`Self::update_spell_name`]) and the other is this tooltip;
    /// the tail writes the caption **only when this string is non-empty**,
    /// which is why the two are one function in retail and two here (see
    /// [`Self::update_spell_name`] for why decoupling them is safe).
    ///
    /// The decision, as retail makes it. The button is first set to state `0x0D` (disabled) and
    /// its tooltip switched off and cleared. Then:
    ///
    /// * **a selected spell** (checked first in both of the other cases): a spell-table miss
    ///   writes nothing. An untargeted or self-targeted (flag `8`) spell enables the button with
    ///   `"CAST %hs"`. With no selected object: `"You must select a target for %hs"`. Otherwise,
    ///   if the selected object is compatible with the spell, enable with `"CAST %hs"` plus
    ///   `" on %s"` and the target's name; if not, `"You must select an appropriate target for
    ///   %hs"`.
    /// * **an endowment item, no spell selected**: an item the object table does not know returns
    ///   with nothing written. An item usable on self enables with `"USE the %s"`. With no
    ///   selected object: `"You must select a target for the %s"`. Otherwise, if the target is
    ///   compatible with the item, enable with `"USE the %s"` plus `" on %s"`; if not, `"You must
    ///   select an appropriate\ntarget for the %s"`.
    /// * **neither**: `"Select a spell to cast"` if an endowment is present or any tab has a
    ///   favourite, else `"You have no spells ready to cast"`.
    ///
    /// At the end, an empty tooltip switches the button's tooltip off and clears it; otherwise
    /// the tooltip is set and switched on.
    ///
    /// Five readings a plausible implementation gets wrong:
    ///
    /// * **The disable is unconditional and first.** The button is greyed *every* time the
    ///   function runs and only the four enabling arms bring it back, so "can I cast" is recomputed
    ///   from scratch on every selection, tab change and equip — there is no sticky enabled state;
    /// * **the selected spell outranks the endowment**, which is the opposite of
    ///   [`Self::cast`]'s order. Same non-conflict as the caption's;
    /// * **untargeted and the self-targeted flag (`8`) are two different tests, both enabling.**
    ///   Untargeted means the target type of the *formula's* last non-zero component is `0`, and
    ///   self-targeted is a flag on the spell. A spell can be either and neither implies the
    ///   other;
    /// * **both "compatible" checks are quiet**, so the tooltip is computed without the player
    ///   being told anything. `cast_spell`'s call to the same predicate is **loud**; this one must
    ///   not be, or hovering the Cast button would spam the chat;
    /// * **`" on %s"` is an append**, not a second format: it is glued onto the string the arm
    ///   above it just built, and a target the object table does not know leaves the first half
    ///   standing alone.
    ///
    /// Returns true when the tooltip changed.
    pub fn update_cast_button_tooltip(
        &mut self,
        ui: &mut UiSystem,
        view: &dyn GameView,
        tab: usize,
    ) -> bool {
        use cast_button::sprintf1;
        let selected = self.sub_menus.get(tab).map_or(0, |s| s.selected_spell);
        let target = view.selected_object().filter(|t| t.0 != 0);
        let mut enabled = false;
        let tip: Option<String> = if selected != 0 {
            // A spell-table miss writes nothing at all.
            view.spell(selected).map(|b| {
                let name = b.name;
                if view.spell_is_untargeted(selected)
                    || b.bitfield & crate::items::widget::icon_background::SPELL_SELF_TARGETED != 0
                {
                    enabled = true;
                    sprintf1(cast_button::CAST, &name)
                } else if let Some(t) = target {
                    if view.spell_target_compatible(selected) {
                        enabled = true;
                        let mut s = sprintf1(cast_button::CAST, &name);
                        if let Some(n) = view.name(t) {
                            s.push_str(&sprintf1(cast_button::ON_TARGET, n));
                        }
                        s
                    } else {
                        sprintf1(cast_button::SPELL_NEEDS_BETTER_TARGET, &name)
                    }
                } else {
                    sprintf1(cast_button::SPELL_NEEDS_TARGET, &name)
                }
            })
        } else if let Some(item) = self.endowment_item {
            // The client: an endowment the object table does not know returns early and writes
            // neither string — not even the tooltip clear.
            let Some(item_name) = view.name(item).map(ToOwned::to_owned) else {
                return false;
            };
            // The same `L"%s (%hs)"` pair the caption builds; the client hands *it* to the
            // formats below, which is why the tooltip carries the spell's name too.
            let caption = view
                .spell(self.endowment_spell)
                .map_or(String::new(), |b| format!("{item_name} ({})", b.name));
            Some(if view.item_useable_self_target(item) {
                enabled = true;
                sprintf1(cast_button::USE_THE, &caption)
            } else if let Some(t) = target {
                if view.item_target_compatible(item) {
                    enabled = true;
                    let mut s = sprintf1(cast_button::USE_THE, &caption);
                    if let Some(n) = view.name(t) {
                        s.push_str(&sprintf1(cast_button::ON_TARGET, n));
                    }
                    s
                } else {
                    sprintf1(cast_button::ITEM_NEEDS_BETTER_TARGET, &caption)
                }
            } else {
                sprintf1(cast_button::ITEM_NEEDS_TARGET, &caption)
            })
        } else {
            // The client: an endowment present, or any favourite on any tab.
            let any = self.endowment_present
                || self
                    .lists
                    .iter()
                    .flatten()
                    .any(|w| w.slots.iter().any(|s| s.spell.is_some_and(|id| id != 0)));
            Some(
                if any {
                    cast_button::SELECT_A_SPELL
                } else {
                    cast_button::NO_SPELLS
                }
                .to_owned(),
            )
        };
        if self.cast_tooltip == tip && self.cast_enabled == Some(enabled) {
            return false;
        }
        if let Some(h) = self.cast_button {
            // The client: greyed first, every time.
            ui.set_state(h, cast_button::DISABLED);
            match tip.as_deref() {
                // The client: an empty tooltip clears both the flag and the text.
                None | Some("") => {
                    ui.set_tooltip_on(h, false);
                    ui.clear_tooltip(h);
                }
                Some(t) => {
                    if enabled {
                        ui.set_state(h, cast_button::ENABLED);
                    }
                    ui.set_tooltip(h, Some(t.to_owned()));
                    ui.set_tooltip_on(h, true);
                }
            }
        }
        self.cast_tooltip = tip;
        self.cast_enabled = Some(enabled);
        true
    }

    /// The whole function, in its own order: read the open tab's selected spell id; if the
    /// endowment is selected and an endowed item exists, use the item and return; if the spell
    /// id is `0`, show a string (type `0x1A`) and return; otherwise, if there is a magic system,
    /// cast the spell.
    ///
    /// Note the ordering: **the spell id is read before the endowment test**, and the endowment
    /// test wins. So a bar with a wand equipped and the endowment slot selected *uses the wand*
    /// even though a spell id is sitting in the sub-menu.
    ///
    /// Retail's zero-spell arm shows the literal `"You must select a spell to cast"` (a plain
    /// literal, not a string-table entry) as a type-`0x1A` string. **This build does not show it
    /// yet:** the request is simply not raised and [`Self::casts`] does not move. The tooltip
    /// update's own `"Select a spell to cast"` is a *tooltip* and is not it.
    ///
    /// Returns the request it raised, if any — also emitted through the UI's request queue
    /// ([`dereth_ui::UiSystem::requests`]), so a caller inside an element-message dispatch does not
    /// need the return value.
    pub fn cast(&mut self, ui: &mut UiSystem) -> Option<UiRequest> {
        let tab = self.open_sub_menu_index(ui);
        let spell_id = self.sub_menus.get(tab).map_or(0, |s| s.selected_spell);
        let endowment_selected = self
            .sub_menus
            .get(tab)
            .is_some_and(|s| s.endowment_selected);
        if endowment_selected {
            if let Some(item) = self.endowment_item {
                // Use the endowed item.
                let r = UiRequest::Use(item);
                ui.requests.emit(r.clone());
                return Some(r);
            }
        }
        if spell_id == 0 {
            return None;
        }
        let r = UiRequest::CastSpell { spell_id };
        ui.requests.emit(r.clone());
        self.casts += 1;
        Some(r)
    }

    /// Its entire body is [`Self::cast`].
    /// This is the keyboard action's route.
    pub fn cast_current_spell(&mut self, ui: &mut UiSystem) -> Option<UiRequest> {
        self.cast(ui)
    }

    /// The cast-quickslot-spell notice (`slot`) — the numbered
    /// quick-cast keys.
    ///
    /// The row at `slot`, then the row's spell id; a zero
    /// id or a slot past the end does **nothing at all** — not even a refusal. Selection is moved
    /// first and the cast follows, which is why pressing the key also changes what the Cast button
    /// would do.
    pub fn cast_quickslot_spell(
        &mut self,
        ui: &mut UiSystem,
        slot: usize,
        view: &dyn GameView,
    ) -> Option<UiRequest> {
        let tab = self.open_sub_menu_index(ui);
        let spell_id = self.lists.get(tab)?.as_ref()?.spell_at(slot)?;
        if spell_id == 0 {
            return None;
        }
        self.set_selected(ui, tab, spell_id);
        self.update_endowment(ui, view);
        self.cast(ui)
    }

    /// **The spell bar's drop hint.**
    ///
    /// The handler is registered by the sub-menu set-up on **all eight** `0x100000B6` lists, so the
    /// item list's drag-over finds it on every tab. The whole function is tiny:
    ///
    /// Read the spell id from the third argument. Zero leaves the hover state untouched;
    /// otherwise set the hovered item to acceptance state `0x10000040`. Both branches return true.
    ///
    /// Four readings, each of which is a visible difference if it is wrong:
    ///
    /// * **The bar refuses nothing.** There is no `0x10000041` anywhere in the function, no
    ///   duplicate check and no "is this spell known" test — `add_favorite` does refuse
    ///   a *double-click* duplicate, but the drag arm sets `move_allowed` and the hover never
    ///   asks. A spell carried over any row of any tab is green.
    /// * **It keys on the spell id, not the item id.** An inventory item carried over the bar
    ///   gets **no hint at all** and the tile keeps whatever state it had — the same
    ///   "say nothing rather than lie" shape `TradePanel` uses for the partner's list
    ///   and `VendorSellView` uses for an alias drag.
    /// * **It never looks at the drop-item flags** at all, so a spell dragged off the quickbar
    ///   (the is-shortcut drop flag) is green here too.
    /// * **The `true` return is unconditional**, so the item list's three-way default can never
    ///   run on a spell list. That matters because the default's zero-item early-out would have
    ///   painted nothing anyway, but its no-container-list accept arm would paint green for a
    ///   carried *item* — which is exactly the lie the zero-spell test above avoids.
    ///
    /// A spell that is already on the bar carries a spell id too: the item list's begin-drag
    /// builds the proxy from the pressed row, and
    /// [`crate::items::widget::ItemSlot::prepare_drag_icon`] writes `SPELL_ID` for any row whose
    /// `spell` is set, whichever list it came from. So the reorder drag is row 2 of the table and
    /// not a separate rule. (The spell leaves its tab on **pick-up** — see
    /// [`Self::on_element_message`]'s `0x21`
    /// arm — so the row the hint is painted on is never the row the spell came from.)
    ///
    /// Returns whether the message was one of this panel's slots'.
    pub fn on_drag_cursor_over(
        &mut self,
        ui: &mut UiSystem,
        m: &dereth_ui::ElementMessage,
    ) -> bool {
        use crate::items::widget::{drag_accept_state, inq_drop_icon_info};
        if m.id.0 != msg::DRAG_CURSOR_OVER {
            return false;
        }
        let Some((tab, slot)) = self
            .lists
            .iter()
            .enumerate()
            .find_map(|(t, l)| l.as_ref().and_then(|w| w.slot_of(m.source)).map(|s| (t, s)))
        else {
            return false;
        };
        // Both of the client's clearing routes, neither of which reaches the handler at all:
        // `p1 == 0` (the cursor left this tile) and `p1 != 0` with no drag element. Each sets
        // drag-accept state `0x1000003F` on the one tile the message names, which is why the poll's list-wide sweep is not needed here.
        let Some(proxy) = ui.drag_state().element.filter(|_| m.p1 != 0) else {
            if let Some(w) = self.lists.get_mut(tab).and_then(Option::as_mut) {
                if let Some(s) = w.slots.get_mut(slot) {
                    s.set_drag_accept_state(ui, drag_accept_state::NONE);
                }
            }
            return true;
        };
        let info = inq_drop_icon_info(ui, proxy);
        // A non-zero spell id -- the only question the bar asks.
        if info.spell.is_some_and(|s| s != 0) {
            if let Some(w) = self.lists.get_mut(tab).and_then(Option::as_mut) {
                if let Some(s) = w.slots.get_mut(slot) {
                    s.set_drag_accept_state(ui, drag_accept_state::ACCEPT);
                }
            }
        }
        // `true` on both arms: consumed either way, and the default never runs.
        true
    }

    /// The client's **`0x15` arm** — the clear that
    /// takes [`Self::on_drag_cursor_over`]'s hint back down when the drop lands — the same clear
    /// the vendor's three lists, the quickbar's eighteen, the salvage window's one and the trade
    /// table's two each have.
    ///
    /// Without it the green stays on the row the spell was dropped on: `0x3E` is raised only by
    /// the mouse-over switch, and the pointer does not move between the release and the drag ending,
    /// so no leave message is ever sent for the tile the drop landed on.
    pub fn on_drop_release(&mut self, ui: &mut UiSystem, m: &dereth_ui::ElementMessage) -> bool {
        use crate::items::widget::drag_accept_state;
        if m.id.0 != msg::DROP_RELEASE {
            return false;
        }
        for w in self.lists.iter_mut().flatten() {
            if let Some(slot) = w.slot_of(m.source) {
                if let Some(s) = w.slots.get_mut(slot) {
                    s.set_drag_accept_state(ui, drag_accept_state::NONE);
                }
            }
        }
        // The `0x15` still has to reach the drop handling, so this never consumes the message.
        false
    }

    /// The spellcasting panel's element-message handler, the arms that select or cast.
    ///
    /// * `0x1C` on a row of the open tab's item list: `p1 == 7` selects the row's spell and
    ///   `p1 == 10` casts (both only for a non-zero spell);
    /// * `0x1C` on the endowment icon (`0x100000B1`): `p1 == 7` sets the endowment-selected flag
    ///   and selects `0`, `p1 == 10` casts, `p1 == 8` examines the endowed item (only when there
    ///   is one);
    /// * `1` on the cast button (`0x100000B2`): cast.
    ///
    /// The `0x15` (drop release) and `0x2C` arms are the drag-a-spell-onto-the-bar path and the
    /// per-frame overlay refresh; neither casts, and both are handled elsewhere.
    ///
    /// Returns true when the message was consumed.
    pub fn on_element_message(
        &mut self,
        ui: &mut UiSystem,
        m: &dereth_ui::ElementMessage,
        view: &dyn GameView,
    ) -> bool {
        let tab = self.open_sub_menu_index(ui);
        if m.id.0 == msg::BUTTON_CLICKED {
            if m.source_id == CAST_BUTTON {
                self.cast(ui);
                return true;
            }
            return false;
        }
        // The client's third arm: dropping a dragged spell on the bar. It is tested before
        // `ITEM_LIST` because `0x15` is not `0x1C` and the early-out below would swallow it.
        if m.id.0 == msg::DROP_RELEASE {
            // The drop-release handling returns without a drag source and a drop target. The drop
            // is broadcast twice, once to the **target** with the owner in `p2` and once to the
            // owner with `p2 == 0`; the client's null test is what tells them apart and this is
            // the same test.
            if m.p2 == 0 {
                return false;
            }
            return self.handle_drop_release(ui, view, m.source, ElemHandle::from_raw(m.p2));
        }
        // The item row's `0x21` arm, then the item list's begin-drag notice, then
        // the spell bar's handler for it: find the sub-menu that owns the list, take the row at
        // the slot (returning if there is no spell row), and if it carries a spell, remove that
        // spell from *that* sub-menu and refresh the endowment icon and cast-button tooltip.
        //
        // **The spell leaves the tab the moment it is picked up, not when it is dropped**, and it
        // leaves the sub-menu that owns the list rather than the open one. That is how a spell is
        // dragged *off* the bar, and it is also why `add_favorite`'s `old < index` shift only ever
        // fires for a drag that started somewhere other than a spell tab -- from the spellbook.
        if m.id.0 == msg::DRAG_REJECTED {
            let (x, y) = m.point.window;
            // **The `0x21` must name a slot.** It is the item row's listener's arm and walks
            // *up* to the list; the item list's own element-message handler has no `0x21` arm, so
            // a message naming the list itself starts no drag in retail.
            let Some(tab) = self
                .lists
                .iter()
                .position(|l| l.as_ref().is_some_and(|w| w.slot_of(m.source).is_some()))
            else {
                return false;
            };
            let Some(start) = self.lists[tab]
                .as_mut()
                .and_then(|w| w.begin_drag(ui, x, y))
            else {
                return false;
            };
            if let Some(spell) = start.spell.filter(|s| *s != 0) {
                self.remove_spell_from_menu(ui, tab, spell);
                self.update_endowment(ui, view);
            }
            return true;
        }
        if m.id.0 != msg::ITEM_LIST {
            return false;
        }
        if m.source_id == ENDOWMENT_ICON {
            match m.p1 {
                item_action::SELECT => {
                    if let Some(s) = self.sub_menus.get_mut(tab) {
                        s.endowment_selected = true;
                    }
                    self.set_selected(ui, tab, 0);
                    self.update_endowment(ui, view);
                }
                item_action::DOUBLE_CLICK => {
                    self.cast(ui);
                }
                item_action::EXAMINE => {
                    if let Some(item) = self.endowment_item {
                        ui.requests.emit(UiRequest::Examine(item));
                    }
                }
                _ => return false,
            }
            return true;
        }
        // The element-message handler: ancestry first, then the item under the mouse.
        // p2 is not a row index. Use the pointer against the actual, scrolled ItemList.
        let Some(w) = self.lists.get(tab).and_then(Option::as_ref) else {
            return false;
        };
        if !ui.is_ancestor_of(w.handle, m.source) {
            return false;
        }
        let (ox, oy) = ui.screen_origin(w.handle);
        let (x, y) = m.point.window;
        let Some(index) = w.item_index_at_point(ui, x - ox, y - oy) else {
            return false;
        };
        let Some(spell_id) = w.spell_at(index) else {
            return false;
        };
        if spell_id == 0 {
            return false;
        }
        match m.p1 {
            item_action::SELECT => {
                self.set_selected(ui, tab, spell_id);
                self.update_endowment(ui, view);
                true
            }
            item_action::DOUBLE_CLICK => {
                self.cast(ui);
                true
            }
            // The spellcasting panel's element-message handler has no `8`
            // arm for a spell row — only for the endowment icon above — and it does not need one:
            // the sub-menu's item list is an `ItemListWidget`, and the item list answers the
            // press itself: a row with an item id selects and examines the object; otherwise a
            // row with a spell id examines the spell.
            //
            // A spell row carries a spell id and no item id, so the second branch is the one the
            // cast bar takes. It is handled here rather than in `items::widget` because both
            // spell lists in this build reach their rows through their own panel's handler.
            //
            // **It does not select.** The item list's `8` arm selects only on the item branch;
            // the spell branch is a bare spell examine, so right-clicking a spell on the bar
            // leaves the selected spell id — and therefore what the
            // cast button would cast — exactly where it was.
            item_action::EXAMINE => {
                ui.notice_inbox.emit_examine_spell(spell_id);
                true
            }
            _ => false,
        }
    }

    /// The row at `index`, then the row's
    /// spell id; a missing row or a zero id selects nothing and returns `false`.
    ///
    /// Reads the real list row, not the raw favorite snapshot (which may contain removed spells).
    pub fn select_spell_from_index(&mut self, ui: &mut UiSystem, tab: usize, index: usize) -> bool {
        let Some(spell_id) = self
            .lists
            .get(tab)
            .and_then(Option::as_ref)
            .and_then(|w| w.spell_at(index))
        else {
            return false;
        };
        if spell_id == 0 {
            return false;
        }
        self.set_selected(ui, tab, spell_id);
        true
    }

    /// The tab's filled rows' spell ids; excludes the trailing empty padding cells.
    fn list_contents(&self, tab: usize) -> Vec<u32> {
        self.lists
            .get(tab)
            .and_then(Option::as_ref)
            .map_or_else(Vec::new, |w| {
                w.slots
                    .iter()
                    .take(w.num_ui_items())
                    .map(|s| s.spell.unwrap_or(0))
                    .collect()
            })
    }

    /// The spellcasting panel's next and previous spell selection notices, in one function because
    /// the two differ in exactly three places (`+1` vs `-1`, the wrap target, and which index the
    /// endowment arm hands to the select-from-index).
    ///
    /// The order of the tests is the client's, and it matters:
    ///
    /// 1. an empty tab selects the endowment if there is one, and otherwise does nothing;
    /// 2. with the endowment selected, it leaves the endowment cell for row `0` (next) or the
    ///    last row (previous);
    /// 3. otherwise it starts at index `0`, finds the selected spell's row and steps one way; a
    ///    step past either end lands on the endowment if one is present and wraps otherwise; the
    ///    row it lands on is selected if it carries a spell.
    ///
    /// Then it refreshes the endowment icon and the cast-button tooltip.
    ///
    /// So **the endowment is one extra cell at the wrap point**: stepping off the end of the list
    /// lands on it when a caster is wielded and wraps round when one is not. The first and last
    /// spell-selection notices confirm it independently — the first prefers the endowment and the
    /// last falls back to it — which is why the field is named for endowment presence here.
    /// `\[verified\]` structurally in all four
    /// handlers; the field *name* is `[inferred]` from those four agreeing.
    ///
    /// **A selection that is not in the list starts from index 0**, because the index starts at
    /// 0 and the loop only ever assigns on a match. That is how the first press with nothing
    /// selected picks the first spell.
    ///
    /// Returns whether anything moved.
    pub fn step_spell_selection(&mut self, ui: &mut UiSystem, next: bool) -> bool {
        let tab = self.open_sub_menu_index(ui);
        let contents = self.list_contents(tab);
        let n = contents.len();
        if n == 0 {
            return self.select_endowment(ui, tab);
        }
        if self
            .sub_menus
            .get(tab)
            .is_some_and(|s| s.endowment_selected)
        {
            let i = if next { 0 } else { n - 1 };
            return self.select_spell_from_index(ui, tab, i);
        }
        let selected = self.sub_menus.get(tab).map_or(0, |s| s.selected_spell);
        let mut idx = 0usize;
        for (j, id) in contents.iter().enumerate() {
            if *id != selected {
                continue;
            }
            if next {
                if j + 1 == n {
                    if self.endowment_present {
                        return self.select_endowment(ui, tab);
                    }
                    idx = 0;
                } else {
                    idx = j + 1;
                }
            } else if j == 0 {
                if self.endowment_present {
                    return self.select_endowment(ui, tab);
                }
                idx = n - 1;
            } else {
                idx = j - 1;
            }
            break;
        }
        self.select_spell_from_index(ui, tab, idx)
    }

    /// The spellcasting panel's first and last spell selection notices.
    ///
    /// The two are mirror images and the *order of the two tests is reversed between them*, which
    /// is the whole content of the pair: `First` tries the endowment and then index 0, `Last`
    /// tries index `n-1` and then the endowment. Reproduced in that order rather than folded into
    /// one guard, because the two orders differ for a character who has both.
    pub fn jump_spell_selection(&mut self, ui: &mut UiSystem, last: bool) -> bool {
        let tab = self.open_sub_menu_index(ui);
        let contents = self.list_contents(tab);
        if last {
            if !contents.is_empty() {
                return self.select_spell_from_index(ui, tab, contents.len() - 1);
            }
            return self.select_endowment(ui, tab);
        }
        if self.endowment_present {
            return self.select_endowment(ui, tab);
        }
        if contents.is_empty() {
            return false;
        }
        self.select_spell_from_index(ui, tab, 0)
    }

    /// The arm all four selection handlers share: set the endowment-selected flag, then select
    /// `0`.
    ///
    /// Selecting `0` deliberately does **not** clear the endowment-selected flag — see
    /// [`Self::set_selected`], where the client's own "only on the row that matches" is why.
    /// Returns `false` when there is no endowment, which is the `return`.
    fn select_endowment(&mut self, ui: &mut UiSystem, tab: usize) -> bool {
        if !self.endowment_present {
            return false;
        }
        if let Some(s) = self.sub_menus.get_mut(tab) {
            s.endowment_selected = true;
        }
        self.set_selected(ui, tab, 0);
        true
    }

    /// The spellcasting panel's next tab id read and the prev tab id read, over
    /// [`SUB_MENU_TAB_BUTTONS`].
    ///
    /// Both are `switch`es over the eight **tab** ids with the default `0x100000A3` (tab 1), which
    /// is why an unknown token lands on the first tab in either direction. One shipped quirk is
    /// reproduced by construction rather than by a special case: **the previous-tab lookup has
    /// no case for `0x100000A4`**, so the tab-2 → tab-1 step falls to the default — which is tab
    /// 1, the same answer the missing case would have given. It is harmless, not a bug.
    #[must_use]
    pub fn step_tab_id(current: u32, next: bool) -> u32 {
        let Some(i) = SUB_MENU_TAB_BUTTONS.iter().position(|t| *t == current) else {
            return SUB_MENU_TAB_BUTTONS[0];
        };
        let n = SUB_MENU_TAB_BUTTONS.len();
        let j = if next { (i + 1) % n } else { (i + n - 1) % n };
        SUB_MENU_TAB_BUTTONS[j]
    }

    /// The spell-cast panel's open-tab token — what the next/previous tab-id lookups are given.
    #[must_use]
    pub fn open_tab_token(&self, ui: &UiSystem) -> Option<ElementId> {
        let panel = self.panel?;
        ui.node(panel)
            .and_then(|n| n.behaviour.as_ref())
            .and_then(|b| b.as_any())
            .and_then(<dyn std::any::Any>::downcast_ref::<dereth_ui::widgets::panel::Panel>)
            .and_then(|p| p.open_tab)
    }

    /// The four tab notices — next, previous, first and last spell tab —
    /// follow the same client sequence.
    ///
    /// Each opens a tab and then refreshes the endowment icon and the cast-button tooltip.
    /// `First` is the literal `0x100000A3` and `Last` the literal
    /// `0x100005C2`; `Next`/`Prev` go through [`Self::step_tab_id`].
    ///
    /// **Declared deviation, one message hop.** The client opens the tab on the panel
    /// directly. A `dereth_ui` behaviour cannot be fetched out of the arena and called from outside
    /// (a widget cannot be read back out of the arena from inside its own handler, and the same
    /// restriction applies from outside, because the call needs an
    /// `ElemCtx`), so this raises **element message `0x19`** on the tab element instead, which is
    /// exactly what opens the tab for a real click. Same function, one broadcast earlier.
    ///
    /// Returns the tab id it asked for, or `None` when the panel is not bound.
    pub fn open_spell_tab(&mut self, ui: &mut UiSystem, which: TabJump) -> Option<ElementId> {
        let panel = self.panel?;
        let count = self.visible_tabs.unwrap_or(SUB_MENUS);
        let current = self.open_tab_token(ui).and_then(|token| {
            SUB_MENU_TAB_BUTTONS[..count]
                .iter()
                .position(|id| *id == token.0)
        });
        let index = match which {
            TabJump::First => 0,
            TabJump::Last => count - 1,
            TabJump::Next => current.map_or(0, |i| (i + 1) % count),
            TabJump::Prev => current.map_or(0, |i| (i + count - 1) % count),
        };
        let tab = SUB_MENU_TAB_BUTTONS[index];
        let tab = ElementId(tab);
        let h = ui.get_child_recursive(panel, tab)?;
        ui.broadcast_element_message(h, dereth_ui::msg::element::id::MOUSE_CLICK, 0, 0);
        Some(tab)
    }

    /// `SpellcastingPanel`'s side of the ten magic notices, and the consumer
    /// end of the client combat system's magic action handling.
    ///
    /// Every one of the eighteen magic-combat keys arrives here. Returns whether the notice
    /// changed anything, which is the observable half: a key that dispatched into an empty spell
    /// bar and one that never dispatched are otherwise the same silence.
    pub fn recv_magic_notice(
        &mut self,
        ui: &mut UiSystem,
        n: MagicNotice,
        view: &dyn GameView,
    ) -> bool {
        let handled = match n {
            MagicNotice::CastCurrentSpell => return self.cast_current_spell(ui).is_some(),
            MagicNotice::CastQuickslotSpell { slot } => {
                return self.cast_quickslot_spell(ui, slot, view).is_some();
            }
            MagicNotice::NextSpellSelection => self.step_spell_selection(ui, true),
            MagicNotice::PrevSpellSelection => self.step_spell_selection(ui, false),
            MagicNotice::FirstSpellSelection => self.jump_spell_selection(ui, false),
            MagicNotice::LastSpellSelection => self.jump_spell_selection(ui, true),
            MagicNotice::NextSpellTab => self.open_spell_tab(ui, TabJump::Next).is_some(),
            MagicNotice::PrevSpellTab => self.open_spell_tab(ui, TabJump::Prev).is_some(),
            MagicNotice::FirstSpellTab => self.open_spell_tab(ui, TabJump::First).is_some(),
            MagicNotice::LastSpellTab => self.open_spell_tab(ui, TabJump::Last).is_some(),
        };
        if handled {
            self.update_endowment(ui, view);
        }
        handled
    }
}

/// The client's `switch`, as a pure function of the token so that it can be
/// asserted without a UI tree.
#[must_use]
pub const fn open_sub_menu_index_of(token: u32) -> usize {
    match token {
        0x1000_00AB => 1,
        0x1000_00AC => 2,
        0x1000_00AD => 3,
        0x1000_00AE => 4,
        0x1000_00AF => 5,
        0x1000_00B0 => 6,
        0x1000_05C3 => 7,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the client's eight Init calls, argument by argument, and `panels::catalogue`'s
    /// binding table for the elements the post-init also names.
    #[test]
    fn the_eight_tabs_and_the_two_buttons_are_the_retail_ids() {
        assert_eq!(SUB_MENUS, 8, "eight spell-bar sub-menus");
        assert_eq!(SUB_MENU_PAGES.len(), 8);
        // Seven contiguous, then the eighth far away — the "added later" discontinuity.
        for (i, id) in SUB_MENU_PAGES.iter().enumerate().take(7) {
            assert_eq!(*id, 0x1000_00AA + u32::try_from(i).expect("< 7"), "tab {i}");
        }
        assert_eq!(SUB_MENU_PAGES[7], 0x1000_05C3);
        for (i, id) in SUB_MENU_TAB_BUTTONS.iter().enumerate().take(7) {
            assert_eq!(
                *id,
                0x1000_00A3 + u32::try_from(i).expect("< 7"),
                "tab button {i}"
            );
        }
        assert_eq!(SUB_MENU_TAB_BUTTONS[7], 0x1000_05C2);
        assert_eq!(
            SUB_MENU_LIST,
            ElementId(0x1000_00B6),
            "the same child id in all eight"
        );
        // The catalogue's `SpellcastingPanel` row must still name the two this module drives.
        let spec = crate::panels::catalogue::spec("SpellcastingPanel").expect("catalogued");
        assert!(
            spec.children.iter().any(|c| c.id == CAST_BUTTON),
            "the cast button"
        );
        assert!(
            spec.children.iter().any(|c| c.id == ENDOWMENT_ICON),
            "the endowment icon"
        );
        assert!(spec.children.iter().any(|c| c.id == PANEL), "the bar panel");
    }

    /// Oracle: the client's `switch`, including the arm that **is not
    /// there**.
    ///
    /// The function has cases for `0x100000AB`…`0x100000B0` and `0x100005C3` and none for
    /// `0x100000AA`; tab 0 comes out of the `default`. That is testable without a UI tree because
    /// the mapping is a pure function of the token.
    #[test]
    fn the_open_tab_switch_has_no_case_for_tab_zero() {
        let index_of = open_sub_menu_index_of;
        for (i, id) in SUB_MENU_PAGES.iter().enumerate() {
            assert_eq!(index_of(*id), i, "page {id:#x}");
        }
        // Tab 0's own token and an id belonging to nothing both answer 0, and they are the same
        // arm in the client.
        assert_eq!(index_of(0x1000_00AA), 0);
        assert_eq!(index_of(0), 0);
        assert_eq!(index_of(0x1000_00B2), 0, "the cast button is not a page");
        // Both sides of the `< 0x100005C4` bound, so the ceiling is pinned rather than
        // assumed: `0x100005C3` is inside it and `0x100005C4` is not.
        assert_eq!(index_of(0x1000_05C3), 7);
        assert_eq!(index_of(0x1000_05C4), 0);
    }

    /// Oracle: the three message shapes the panel acts on.
    #[test]
    fn the_message_constants_are_the_retail_ones() {
        assert_eq!(msg::ITEM_LIST, 0x1C);
        assert_eq!(msg::BUTTON_CLICKED, 1);
        assert_eq!(item_action::SELECT, 7);
        assert_eq!(item_action::EXAMINE, 8);
        assert_eq!(item_action::DOUBLE_CLICK, 10);
        // The endowment icon update's equipment location.
        assert_eq!(ENDOWMENT_LOCATION, 0x0100_0000);
    }
}
