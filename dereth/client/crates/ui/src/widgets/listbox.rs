use super::*;
use crate::ElementId;

#[derive(Debug, Default)]
pub struct ListBox {
    /// The original smart array of row **elements**.
    ///
    /// **Handles, not `ElementId`s.** An element id cannot stand in for a
    /// row here: every row of a menu popup is created from the *same* template description
    /// (`0x1000001E` for the chat window's talk-focus menu, all fourteen of them), so ids are
    /// not distinct, and the client's item-equals-selected-item
    /// pointer compare has no id-shaped equivalent.
    pub items: Vec<ElemHandle>,
    /// ItemList's active rows exclude its hidden slot cache. Other legacy binders still
    /// discover their rows from children until they publish the item list explicitly.
    pub items_are_authoritative: bool,
    /// Optional occupied prefix used for scroll extent while padded cells remain drawable.
    pub scroll_item_count: Option<usize>,
    /// The selected item.
    pub selected: Option<usize>,
    pub cols: u32,
    pub rows: u32,
    /// The element a drag was last over.
    pub drag_last_over: Option<ElementId>,
    /// The scroll animation: start time, end time, end x and end y.
    pub anim: Option<(f64, f64, i32, i32)>,
    /// `Scrollable`'s virtual content rectangle.
    pub scroll_offset: (i32, i32),
    /// The element is in state `0x0D` — the one state in which the list box's mouse-visibility
    /// override can still answer false. Tracked here because the
    /// override is asked before `UiSystem` has a state to consult.
    pub disabled: bool,

    // ---- shared scrolling state ------------------------------------
    /// The six pieces of scrolling state inherited by the original list box and composed here.
    ///
    /// The original list box shares scrolling behavior, established by its scroll-to-Y,
    /// scroll-to-X, adjust-to-scrollable-change, and scroll-delta operations. Its
    /// element-message handler delegates to that shared behavior last.
    ///
    /// Without it the wizard's skills list — 42 rows of 26 px in a 309 px box — would have a
    /// bar that reports the gesture and a list that never consumes it.
    pub scroll: crate::scrollable::Scrollable,
    /// Each row's **unscrolled** origin, re-captured whenever the row set changes.
    ///
    /// The client does not need this: its layout update re-places every row at
    /// `position - scroll offset` each time the dirty bit `0x200` is set, so the origin is
    /// implicit in the arithmetic. In this build the placement is the screen-side
    /// `ListBoxWidget::update_layout`, which places rows unscrolled; recording the origins is
    /// how the same picture is reached without owning that call.
    origins: Vec<(ElemHandle, i32, i32)>,
    /// `(handle, width, height)` per row when [`Self::origins`] was captured.
    fingerprint: Vec<(ElemHandle, i32, i32)>,
    /// `(handle, x0, y0, width, height)` for every row **exactly as [`Self::place_rows`] last
    /// left it** — the cache's invalidation key.
    ///
    /// [`Self::fingerprint`] alone, i.e. the handles and their sizes,
    /// cannot see the one event this cache exists to react to. The stat-management panel
    /// re-files a trained skill by **re-ordering** the item list and re-running
    /// the layout update: the same handles, at the same sizes, in the same tree order, at
    /// different positions. A size-only key would compare equal, `origins` would keep the
    /// *pre-click* values, and the very next tick's `place_rows` would put every row back where
    /// it had been — so the model would re-file the row, the row's own cells would update, and
    /// **the drawn frame would not move**. Comparing against what this element itself last wrote separates "the screen
    /// re-placed the rows" from "I placed them there", which is the distinction the size-only
    /// key could not make.
    placed: Vec<(ElemHandle, i32, i32, i32, i32)>,
    /// The column and row counts, derived from the placed rows — the two numbers used to divide
    /// the paper size into each row's step.
    pub grid: (i32, i32),
    /// Whether this list has already registered itself on its bar(s).
    bars_bound: bool,
}

pub fn create(_l: &crate::LayoutDesc, _d: &crate::ElementDesc) -> Box<dyn Element> {
    Box::new(ListBox::default())
}

/// `ListBox`'s own layout attributes, taken off the client's
/// attribute switch. Four of them are flag bits the constructor leaves clear.
///
/// **The flag word starts at `0x290`**, i.e.
/// `0x200 | 0x80 | 0x10` — so `ClickSelect`, `DragSelect`, `DragRollover`, `Horizontal` and
/// `SelectedItemStateChange` are all **off** until a layout asks for them, and each has
/// exactly one writer in the client (its setter helper) with exactly one caller (the arm
/// below).
///
/// They are read back out of the element's merged properties where they are needed rather
/// than mirrored into a field, as the existing code already does for `0x5F` and `0x5C`; a
/// mirror would be a second source of truth for data the node already holds. The attribute
/// setter writes the instance property before it calls
/// the attribute handler, so a runtime write is visible to the merged read too.
pub mod attr {
    /// `0x59` — click-select, flag bit `0x2`. **The gate on both press
    /// sites.** 20 of the 35 shipped list boxes declare it.
    pub const CLICK_SELECT: u32 = 0x59;
    /// `0x5A` — drag rollover, bit 3. Nothing in the shipped data declares it.
    pub const DRAG_ROLLOVER: u32 = 0x5A;
    /// `0x5B` — drag select, bit 2. Five shipped list boxes declare it **false** and none
    /// declares it true, which is why the behavior the bit gates is not transcribed: it is
    /// unreachable in shipped data.
    pub const DRAG_SELECT: u32 = 0x5B;
    /// `0x5C` — the horizontal flag, bit 0.
    pub const HORIZONTAL: u32 = 0x5C;
    /// `0x5D` — the state `set_selected_item` puts on the row **losing** the selection.
    /// Read off the **list box**, not off the row.
    pub const UNSELECTED_STATE: u32 = 0x5D;
    /// `0x5E` — the state it puts on the row **gaining** it.
    pub const SELECTED_STATE: u32 = 0x5E;
    /// `0x5F` — the layout update's only attribute read.
    pub const MAX_COLUMNS: u32 = 0x5F;
    /// `0x61` — selected-item state change, bit 5. The gate on the two row-state writes
    /// above.
    pub const SELECTED_ITEM_STATE_CHANGE: u32 = 0x61;
}

/// One of the flag word's layout-driven bits, off the live element. See `attr`.
fn bit(ui: &UiSystem, me: ElemHandle, id: u32) -> bool {
    ui.node(me)
        .map(crate::ElementNode::merged_properties)
        .and_then(|p| p.get_bool(id))
        .unwrap_or(false)
}

/// The two selecting **input actions** are 7 and `0x0A`.
///
/// `7` is `PRIMARY_CLICK` and `0x0A` is the double-click resolution of the same button —
/// input map 3 binds `DIMOFS_BUTTON0` twice, `Click` to 7 and `MouseDblClick` to `0x0A`, and
/// the input resolver prefers the larger activation, so **the second press
/// of a pair arrives as `0x0A`**. A list box that took only 7 would go deaf on every second
/// click, which is exactly the gesture the list box is listening for.
pub const PRESS_ACTIONS: [u32; 2] = [crate::focus::action::PRIMARY_CLICK, 0x0A];

impl ListBox {
    /// The list box's selected-item setter, whole.
    ///
    /// ```text
    /// if item == selected:
    ///     if notify: broadcast 0x43 with (selected, 0)
    ///     return
    /// index = -1; selected = none
    /// for i in 0 .. items.len():
    ///     if items[i] == item: index = i; selected = item; break
    /// if flag bit 0x20:
    ///     if old:      old.set_state(enum attribute 0x5D)
    ///     if selected: new.set_state(enum attribute 0x5E)
    /// if notify: broadcast 4 with (index, selected)
    /// ```
    ///
    /// # The equal branch is the only producer of `0x43` in the client
    ///
    /// Nothing else in the client raises `0x43`, so **`0x43` means
    /// "you pressed the row that was already selected"** — it is not "double click" and it is
    /// not raised by `0x1A`. The double-click detector's one-second window is a
    /// *detector* built on top of it, and because one press runs this function **twice** (see
    /// [`Self::press_select`]) a real double-click is two presses.
    ///
    /// An early return when the item is already selected would silence exactly the line that
    /// broadcasts here, and `0x43` would then have to come from `MOUSE_DOUBLE_CLICK` instead,
    /// which is wrong.
    ///
    /// # An easy misreading
    ///
    /// The **second** notify test is easy to misread as a test of the *item*. Both tests read
    /// `notify`.
    ///
    /// The row call sets the row's **state**, *not* its media state.
    ///
    /// # One representational deviation, named
    ///
    /// Retail's selected item is a pointer and this build carries an **index** into
    /// [`Self::items`], so the one state retail can hold and this cannot is "selected, and the
    /// selected element is no longer in the item list". Every writer here goes through the
    /// array, `ListBoxWidget::mirror_items` carries the selection across by handle, and
    /// deletion clears it, so no path in this build produces that state.
    pub fn set_selected_item(
        &mut self,
        ui: &mut UiSystem,
        me: ElemHandle,
        item: Option<ElemHandle>,
        notify: bool,
    ) {
        let old = self.selected.and_then(|i| self.items.get(i).copied());
        if old == item {
            if notify {
                // **Queued, not raised -- and that is the client's own behaviour.**
                // The manager's own broadcast stamps the serial
                // and then, if a broadcast is already in progress, queues the message and
                // dispatches it only after the broadcast in flight has unwound. This function
                // is reached from inside the `0x1C` broadcast, so raising here directly would
                // allocate a larger serial, stamp every ancestor as it bubbled, and kill the
                // `0x1C` where the nesting happened -- see
                // [`crate::UiSystem::queue_element_message`]. Measured: it took the
                // stat-management panel's own `0x1C` arm off the air, so neither stat panel
                // could select a row any more.
                ui.queue_element_message(
                    me,
                    msgid::LIST_ITEM_ACTIVATED,
                    old.map_or(0, ElemHandle::raw),
                    0,
                );
            }
            return;
        }
        // The walk: not found leaves `index = -1` **and** no selected item, so a
        // `set_selected_item(some_stranger, true)` *clears* the selection rather than leaving
        // it.
        self.selected = item.and_then(|h| self.items.iter().position(|i| *i == h));
        let new = self.selected.and_then(|i| self.items.get(i).copied());
        if bit(ui, me, attr::SELECTED_ITEM_STATE_CHANGE) {
            // The enum attribute is read off the **list box** and its result is used
            // whether or not the attribute was found -- it goes
            // straight into the state setter -- so the eleven shipped lists that enable `0x61`
            // without naming the pair get the `MasterProperty` defaults, `0x5D = 1` and
            // `0x5E = 6`. Skipping `set_state` when the list box named no state would leave
            // every list but the five vendor panes out of its row template's authored state 6,
            // and the Friends list would draw no band.
            if let Some(o) = old {
                let s = ui.get_attribute_enum(me, attr::UNSELECTED_STATE).1;
                ui.set_state(o, crate::StateId(s));
            }
            if let Some(n) = new {
                let s = ui.get_attribute_enum(me, attr::SELECTED_STATE).1;
                ui.set_state(n, crate::StateId(s));
            }
        }
        if notify {
            // The index is `0xffffffff` when the walk found nothing, and the client sends it
            // as-is. A reader that wants an index reads it back as -1.
            let index = self
                .selected
                .and_then(|i| u32::try_from(i).ok())
                .unwrap_or(u32::MAX);
            // Queued for the same reason as the `0x43` above.
            ui.queue_element_message(
                me,
                msgid::LIST_SELECTION_CHANGED,
                index,
                new.map_or(0, ElemHandle::raw),
            );
        }
    }

    /// Select by index, minus its scroll-to-show tail.
    ///
    /// ```text
    /// if index < items.len(): set_selected_item(items[index], notify)
    ///                         scroll items[index] into view
    /// else:                   set_selected_item(none, notify)
    /// ```
    ///
    /// **An out-of-range index clears the selection**, it does not leave it alone — and it
    /// still notifies. The scroll tail is [`scroll_item_to_view`], which needs this object out
    /// of its slot and so cannot be called from in here; a caller that wants both calls it
    /// afterwards, which is what the client's scroll-to-view users do.
    pub fn select(&mut self, ui: &mut UiSystem, me: ElemHandle, index: usize) {
        let item = self.items.get(index).copied();
        self.set_selected_item(ui, me, item, true);
    }

    /// The item-index-at-point query — which row is at an
    /// **element-relative** point.
    ///
    /// ```text
    /// if no items or x < 0 or width  <= x: return false
    /// if             y < 0 or height <= y: return false
    /// run the layout update
    /// ix = x + scroll.x;  iy = y + scroll.y
    /// col = first c with sum(item_widths [0..=c]) >= ix, else 0
    /// row = first r with sum(item_heights[0..=r]) >= iy, else 0
    /// index = horizontal ? cols * row + col : rows * col + row
    /// return index < items.len()
    /// ```
    ///
    /// The `else 0` is the client's: each loop's counter keeps the value it was initialised
    /// with when the accumulation never reaches the point, and both start at 0.
    ///
    /// The item widths and heights are the layout update's first pass — the per-column and
    /// per-row **maxima**. They are recomputed here from the rows' own **unscrolled** origins
    /// (`box + scroll offset`, reversing the placement subtraction) rather
    /// than cached, because in this build the rows may have been placed either by this
    /// element's `update_layout` or by a screen-side binder, and a cached pair would be stale
    /// for exactly one of the two. Over rows on a grid the two readings are the same numbers:
    /// the distinct origins on each axis are the column and row counts, and a band's size is
    /// the largest row sitting on it.
    /// **This is the function the item-under-mouse query needs and the widget did not have.** The twin in
    /// `dereth_ui_screens::panels::listbox::ListBoxWidget` stays where it is: it
    /// answers for the binder's own row array, which the two stat panels index into.
    #[must_use]
    pub fn inq_item_index_at_point(
        &self,
        ui: &UiSystem,
        me: ElemHandle,
        x: i32,
        y: i32,
    ) -> Option<usize> {
        if self.items.is_empty() {
            return None;
        }
        let b = ui.node(me).map(|n| n.region.box_)?;
        if x < 0 || b.width() <= x || y < 0 || b.height() <= y {
            return None;
        }
        // `(unscrolled x, unscrolled y, width, height)` per row, in item-list order.
        let cells: Vec<(i32, i32, i32, i32)> = self
            .items
            .iter()
            .map(|h| {
                ui.node(*h).map_or((0, 0, 0, 0), |n| {
                    let r = n.region.box_;
                    (
                        r.x0 + self.scroll.x,
                        r.y0 + self.scroll.y,
                        r.width(),
                        r.height(),
                    )
                })
            })
            .collect();
        let band_sizes = |axis: fn(&(i32, i32, i32, i32)) -> i32,
                          size: fn(&(i32, i32, i32, i32)) -> i32|
         -> Vec<i32> {
            let mut origins: Vec<i32> = cells.iter().map(axis).collect();
            origins.sort_unstable();
            origins.dedup();
            origins
                .iter()
                .map(|o| {
                    cells
                        .iter()
                        .filter(|c| axis(c) == *o)
                        .map(size)
                        .max()
                        .unwrap_or(0)
                })
                .collect()
        };
        let widths = band_sizes(|c| c.0, |c| c.2);
        let heights = band_sizes(|c| c.1, |c| c.3);
        let band = |sizes: &[i32], at: i32| -> usize {
            let mut acc = 0i32;
            for (i, s) in sizes.iter().enumerate() {
                acc += *s;
                if at <= acc {
                    return i;
                }
            }
            0
        };
        let col = band(&widths, x + self.scroll.x);
        let row = band(&heights, y + self.scroll.y);
        let index = if bit(ui, me, attr::HORIZONTAL) {
            widths.len() * row + col
        } else {
            heights.len() * col + row
        };
        (index < self.items.len()).then_some(index)
    }

    /// The list box's item-under-mouse query.
    ///
    /// ```text
    /// if the mouse is not over the list: return none
    /// if inq_item_index_at_point(mouse.x - screen.x0, mouse.y - screen.y0) is i:
    ///     return items[i]
    /// return none
    /// ```
    ///
    /// `(wx, wy)` is the pointer in **window** coordinates — `ElementMessage::point.window`,
    /// which the manager's mouse-down event stamped from the same
    /// input-device mouse position the client reads here. It is converted against **this
    /// element's** screen origin rather than the message's, so a message that bubbled up from a
    /// child still resolves against the right rectangle, exactly as the client's global read
    /// does.
    ///
    /// The mouse-over guard is subsumed by [`Self::inq_item_index_at_point`]'s own bounds
    /// test: a point outside the list box's rectangle is precisely the case the mouse-over flag
    /// is false for. (The same reading the screen-side twin records.)
    #[must_use]
    pub fn get_item_under_mouse(
        &self,
        ui: &UiSystem,
        me: ElemHandle,
        wx: i32,
        wy: i32,
    ) -> Option<ElemHandle> {
        let b = ui.screen_box(me);
        let i = self.inq_item_index_at_point(ui, me, wx - b.x0, wy - b.y0)?;
        self.items.get(i).copied()
    }

    /// One of the two press sites, which are the same four statements:
    ///
    /// ```text
    /// if flag bit 0x2:
    ///     item = get_item_under_mouse()
    ///     if item: set_selected_item(item, true)
    ///              if flag bit 0x4: start a drag select
    /// ```
    ///
    /// Drag selection is **not** transcribed: flag bit `0x4` is attribute
    /// `0x5B`, and a census of the shipped layouts finds five list boxes
    /// declaring it *false* and **none** declaring it true, so the call is unreachable in
    /// shipped data. Named here rather than silently dropped.
    ///
    /// Returns whether a row was under the pointer.
    fn press_select(&mut self, ui: &mut UiSystem, me: ElemHandle, wx: i32, wy: i32) -> bool {
        if !bit(ui, me, attr::CLICK_SELECT) {
            return false;
        }
        let Some(item) = self.get_item_under_mouse(ui, me, wx, wy) else {
            return false;
        };
        self.set_selected_item(ui, me, Some(item), true);
        true
    }

    /// Put one already-created element into
    /// the item list at `index`, under this list.
    ///
    /// The client's insert also marks the layout dirty; here row placement belongs to the
    /// list box and runs when the caller asks for it.
    pub fn insert_item(
        &mut self,
        ui: &mut UiSystem,
        me: ElemHandle,
        item: ElemHandle,
        index: usize,
    ) -> bool {
        if ui.node(item).is_none() {
            return false;
        }
        ui.set_parent(item, Some(me));
        let at = index.min(self.items.len());
        self.items.insert(at, item);
        if let Some(sel) = self.selected {
            if at <= sel {
                self.selected = Some(sel + 1);
            }
        }
        true
    }

    /// Drop every row and its element.
    pub fn flush(&mut self, ui: &mut UiSystem) {
        self.selected = None;
        for h in std::mem::take(&mut self.items) {
            ui.remove_and_delete_root(h);
        }
        self.origins.clear();
        self.fingerprint.clear();
        self.placed.clear();
    }

    /// Return row `i`, or `None` when it is out of range.
    #[must_use]
    pub fn get_item(&self, index: usize) -> Option<ElemHandle> {
        self.items.get(index).copied()
    }

    /// The client's two passes, over the item list.
    ///
    /// ```text
    /// cols = int attribute 0x5F                 // absent == 0
    /// if cols < 0: cols = n; rows = (n != 0)
    /// else:        cols = min(max(cols, 1), n); rows = ceil(n / cols)
    /// // widths[col] = max item width in that column, heights[row] = max item height in it,
    /// // then place each item at the running sum, wrapping on cols (horizontal) or on
    /// // rows (the default, which fills column-major)
    /// ```
    ///
    /// **This is the same arithmetic as
    /// `dereth_ui_screens::panels::listbox::ListBoxWidget::update_layout`, which
    /// keeps its own row array because it is a screen-side binder for lists
    /// whose rows come from a template list.** The copy is here because a menu popup is built
    /// inside this crate and has no screen-side binder; the two were compared line for line
    /// and they agree. They could be reconciled onto this one.
    ///
    /// Returns `(cols, rows)`.
    pub fn update_layout(&mut self, ui: &mut UiSystem, me: ElemHandle) -> (i32, i32) {
        let n = i32::try_from(self.items.len()).unwrap_or(0);
        let props = ui.node(me).map(crate::ElementNode::merged_properties);
        let max_columns = props.as_ref().and_then(|p| p.get_int(0x5F)).unwrap_or(0);
        let horizontal = props
            .as_ref()
            .and_then(|p| p.get_bool(0x5C))
            .unwrap_or(false);
        let (cols, rows) = if max_columns < 0 {
            (n, i32::from(n != 0))
        } else {
            let cols = max_columns.max(1).min(n);
            let rows = if cols == 0 { 0 } else { (n + cols - 1) / cols };
            (cols, rows)
        };
        self.grid = (cols.max(0), rows.max(0));
        if cols <= 0 || rows <= 0 {
            return self.grid;
        }
        let ncols = usize::try_from(cols).unwrap_or(0);
        let nrows = usize::try_from(rows).unwrap_or(0);
        let mut widths = vec![0i32; ncols];
        let mut heights = vec![0i32; nrows];
        let (mut col, mut row) = (0usize, 0usize);
        for h in &self.items {
            let b = ui.node(*h).map(|nd| nd.region.box_).unwrap_or_default();
            if let Some(w) = widths.get_mut(col) {
                *w = (*w).max(b.width());
            }
            if let Some(ht) = heights.get_mut(row) {
                *ht = (*ht).max(b.height());
            }
            if horizontal {
                if col == ncols - 1 {
                    col = 0;
                    row += 1;
                } else {
                    col += 1;
                }
            } else if row == nrows - 1 {
                row = 0;
                col += 1;
            } else {
                row += 1;
            }
        }
        let (mut x, mut y) = (0i32, 0i32);
        let (mut col, mut row) = (0usize, 0usize);
        for h in self.items.clone() {
            ui.move_to(h, x, y);
            if horizontal {
                if col == ncols - 1 {
                    x = 0;
                    col = 0;
                    y += heights.get(row).copied().unwrap_or(0);
                    row += 1;
                } else {
                    x += widths.get(col).copied().unwrap_or(0);
                    col += 1;
                }
            } else if row == nrows - 1 {
                y = 0;
                x += widths.get(col).copied().unwrap_or(0);
                row = 0;
                col += 1;
            } else {
                y += heights.get(row).copied().unwrap_or(0);
                row += 1;
            }
        }
        self.grid
    }

    // ---- `Scrollable`'s half ------------------------------------------------------

    /// The scroll refresh plus the tail of the layout update, driven off the rows' own
    /// geometry.
    ///
    /// The client sums the item widths over the columns and the item heights over the rows —
    /// the per-column and per-row maxima it built in the layout update's first pass — and hands
    /// the two totals to the scrollable-area resize. Over rows that are already placed on
    /// that grid, the same two totals are `max(x + width)` and `max(y + height)`, which is what
    /// this computes; and the number of distinct origins on each axis is the column/row count.
    ///
    /// Returns whether anything changed.
    pub fn refresh_scroll(&mut self, ui: &mut UiSystem, me: ElemHandle) -> bool {
        // The client's registration, done here rather than in
        // `post_init` because a list and its bar are **siblings**: nothing the bar broadcasts
        // would otherwise bubble through the list, and the bar may not be in the tree yet when
        // the list is initialised. Once, guarded, so the listener list cannot grow.
        if !self.bars_bound {
            for horizontal in [true, false] {
                if let Some(bar) = self.scroll.scrollbar(ui, me, horizontal) {
                    ui.register_for_element_messages(bar, crate::ListenerId::Element(me));
                    self.bars_bound = true;
                }
            }
        }
        let children = if self.items_are_authoritative {
            self.items.clone()
        } else {
            ui.children(me)
        };
        let rows: Vec<(ElemHandle, i32, i32, i32, i32)> = children
            .into_iter()
            .filter_map(|c| {
                let b = ui.node(c)?.region.box_;
                Some((c, b.x0, b.y0, b.width(), b.height()))
            })
            .collect();
        if rows != self.placed {
            // A row was added, removed, resized **or moved by someone other than this
            // element**, so `ListBoxWidget::update_layout` has just re-placed every one of
            // them **unscrolled**: their current boxes are the origins. Position is part of
            // the key because a re-order is the only change the stat panel's re-file
            // makes, and it changes nothing else.
            self.fingerprint = rows.iter().map(|(c, _, _, w, h)| (*c, *w, *h)).collect();
            self.origins = rows.iter().map(|(c, x, y, _, _)| (*c, *x, *y)).collect();
            let mut xs: Vec<i32> = self.origins.iter().map(|(_, x, _)| *x).collect();
            let mut ys: Vec<i32> = self.origins.iter().map(|(_, _, y)| *y).collect();
            xs.sort_unstable();
            xs.dedup();
            ys.sort_unstable();
            ys.dedup();
            self.grid = (
                i32::try_from(xs.len()).unwrap_or(0),
                i32::try_from(ys.len()).unwrap_or(0),
            );
        }
        let (mut w, mut h) = (0, 0);
        for ((_, ox, oy), (_, cw, ch)) in self
            .origins
            .iter()
            .zip(self.fingerprint.iter())
            .take(self.scroll_item_count.unwrap_or(usize::MAX))
        {
            w = w.max(ox + cw);
            h = h.max(oy + ch);
        }
        let mut changed = self.scroll.resize_scrollable_area(ui, me, w, h);
        changed |= self.scroll.update_scrollbar_size(ui, me, true);
        changed |= self.scroll.update_scrollbar_size(ui, me, false);
        self.place_rows(ui);
        changed
    }

    /// The client's second pass: every row sits at its own grid position less
    /// the scroll offset. Rows pushed outside the list box are clipped by
    /// the region draw routine's ancestor intersection, so nothing else is needed to hide
    /// them.
    fn place_rows(&mut self, ui: &mut UiSystem) {
        for (h, ox, oy) in self.origins.clone() {
            ui.move_to(h, ox - self.scroll.x, oy - self.scroll.y);
        }
        self.scroll_offset = (self.scroll.x, self.scroll.y);
        // Record what was just written, so the next `refresh_scroll` can tell its own
        // placement apart from one the screen made. The stored value keeps the
        // width and height, so this is exactly the boxes the rows now hold.
        self.placed = self
            .origins
            .iter()
            .zip(self.fingerprint.iter())
            .map(|((h, ox, oy), (_, w, ht))| (*h, ox - self.scroll.x, oy - self.scroll.y, *w, *ht))
            .collect();
    }

    /// Behavior: the scroll-delta query, the step in **pixels**
    /// one arrow click or one track click moves the list.
    ///
    /// ```text
    /// if page: step = horizontal ? width : height
    /// else:
    ///     n = horizontal ? cols : rows
    ///     if n != 0:
    ///         item = (horizontal ? scrollable width : scrollable height) / n
    ///         view = horizontal ? width : height
    ///         step = (view <= item) ? view : item      // i.e. min(view, one row)
    /// return negative ? -step : step
    /// ```
    ///
    /// So an **arrow** moves the list by exactly one row and a **track click** by one whole
    /// view — a page. Note this is not `TextElement`'s, whose
    /// page arm is `view - step`; the list's page arm is the plain view height.
    #[must_use]
    pub fn inq_scroll_delta(
        &self,
        ui: &UiSystem,
        me: ElemHandle,
        horizontal: bool,
        negative: bool,
        page: bool,
    ) -> i32 {
        let b = ui
            .node(me)
            .map_or_else(crate::Box2D::default, |n| n.region.box_);
        let view = if horizontal { b.width() } else { b.height() };
        let mut step = 0;
        if page {
            step = view;
        } else {
            let n = if horizontal { self.grid.0 } else { self.grid.1 };
            if n != 0 {
                let content = if horizontal {
                    self.scroll.width
                } else {
                    self.scroll.height
                };
                let item = content / n;
                step = if view <= item { view } else { item };
            }
        }
        if negative {
            -step
        } else {
            step
        }
    }
}

// ---- driving `Scrollable`'s half from outside the arena -----------------------
//
// The list box is a scrollable in the client, so the offset, the row placement
// and the hit test are all one object's business and the layout update does the lot
// in one call. In this build the *rows* are built and measured by
// `dereth_ui_screens::panels::listbox::ListBoxWidget`, which is a screen-side object with no
// access to this behaviour's slot. These three functions are the seam: they are what a screen
// calls instead of reaching into `ElementNode::behaviour` by hand, and they lift the behaviour
// through `take_behaviour`/`put_behaviour` so the deferred state and mouse-visibility flushes
// keep their bookkeeping.
//
// Every one of them answers `None`/`false` when the behaviour is not in its slot, which is the
// case while this element's own handler is running: a widget cannot be read back out of the
// arena from inside its own handler. A screen must not call them
// from there, and getting a silent `false` rather than a panic is deliberate.

/// Read the horizontal and vertical scroll offsets from a live list box.
///
/// **This is the number the client adds back before it walks the
/// bands**. A screen-side copy that nothing wrote would let
/// the rows scroll (this behaviour's row placement moves them) while the hit test
/// went on answering as though they had not.
#[must_use]
pub fn scroll_offset_of(ui: &UiSystem, list: ElemHandle) -> Option<(i32, i32)> {
    let l = ui
        .node(list)?
        .behaviour
        .as_ref()?
        .as_any()?
        .downcast_ref::<ListBox>()?;
    Some((l.scroll.x, l.scroll.y))
}

/// The scrollable's offset setter followed by the list box's adjust-to-scrollable-change —
/// move the offset (clamped) and
/// re-place every row against it. Returns whether the offset moved.
pub fn set_scroll_offset(ui: &mut UiSystem, list: ElemHandle, x: i32, y: i32) -> bool {
    let Some(mut b) = ui.take_behaviour(list) else {
        return false;
    };
    let moved = match b.as_any_mut().and_then(|a| a.downcast_mut::<ListBox>()) {
        Some(l) => {
            let m = l.scroll.set_scrollable_xy(ui, list, x, y, false);
            l.place_rows(ui);
            m
        }
        None => false,
    };
    ui.put_behaviour(list, b);
    moved
}

/// Scroll a row into view, over an active row's unscrolled origin.
/// The screen binder has already placed the grid; refresh captures its per-band sums.
/// Both before-viewport arms use the row's top-left. The right/bottom arms adjust only
/// one coordinate before passing both to set_scrollable_xy; do not independently minimize axes.
pub fn scroll_item_to_view(ui: &mut UiSystem, list: ElemHandle, item: ElemHandle) -> bool {
    let Some(mut b) = ui.take_behaviour(list) else {
        return false;
    };
    let moved = (|| {
        let l = b.as_any_mut()?.downcast_mut::<ListBox>()?;
        l.refresh_scroll(ui, list);
        let (_, x, y) = l.origins.iter().find(|(h, _, _)| *h == item).copied()?;
        let view = ui.node(list)?.region.box_;
        let row = ui.node(item)?.region.box_;
        let (sx, sy) = (l.scroll.x, l.scroll.y);
        let (nx, ny) = if x < sx || y < sy {
            (x, y)
        } else if x > sx + view.width() - row.width() {
            (x - view.width() + row.width(), y)
        } else if y > sy + view.height() - row.height() {
            (x, y - view.height() + row.height())
        } else {
            return Some(false);
        };
        let changed = l.scroll.set_scrollable_xy(ui, list, nx, ny, false);
        l.place_rows(ui);
        Some(changed)
    })()
    .unwrap_or(false);
    ui.put_behaviour(list, b);
    moved
}

/// The selected-item setter against a bare list-box handle performs the list box's normal
/// selection operation. It is reached from **outside** `dereth-ui`, where
/// [`UiSystem::take_behaviour`] / [`UiSystem::put_behaviour`] are `pub(crate)`.
///
/// The spell-component panel's selection-changed notice ends in
/// selecting `item` on the component list box with notify set — with the matched row on one arm
/// and a null on the clear arm — and it is a
/// *panel* in `dereth-ui-screens` that has to make that call. Writing the
/// selection straight onto the behaviour instead would lose the row **state** change
/// (the selection band) and the queued `4`, both of which this keeps.
///
/// Returns whether the handle was a list box at all, so a caller can tell "cleared" from
/// "there was nothing to clear".
pub fn set_selected_item_of(
    ui: &mut UiSystem,
    list: ElemHandle,
    item: Option<ElemHandle>,
    notify: bool,
) -> bool {
    let Some(mut b) = ui.take_behaviour(list) else {
        return false;
    };
    let ok = match b.as_any_mut().and_then(|a| a.downcast_mut::<ListBox>()) {
        Some(l) => {
            l.set_selected_item(ui, list, item, notify);
            true
        }
        None => false,
    };
    ui.put_behaviour(list, b);
    ok
}

/// The list box's layout-update tail — the scrollable-area resize
/// over the row grid, then the second pass that places every row at
/// `running_sum - scroll offset`.
///
/// The screen-side widget owns the *first* pass (the per-column and per-row maxima) and calls
/// this at the end of its own `update_layout`, so the two halves run in one frame exactly as
/// the client's single function does. Without it the rows sit unscrolled until the next global
/// tick, and anything that reads geometry in between sees a list that has forgotten where it
/// was scrolled to.
pub fn refresh_scroll_of(ui: &mut UiSystem, list: ElemHandle) -> bool {
    let Some(mut b) = ui.take_behaviour(list) else {
        return false;
    };
    let changed = match b.as_any_mut().and_then(|a| a.downcast_mut::<ListBox>()) {
        Some(l) => l.refresh_scroll(ui, list),
        None => false,
    };
    ui.put_behaviour(list, b);
    changed
}

impl Element for ListBox {
    /// The list box's original mouse-visibility behavior.
    ///
    /// The base mouse-visibility result matters only in disabled state `0x0D`; every other
    /// list-box state is mouse-visible.
    ///
    /// i.e. **a list box is always mouse-visible except when it is disabled** — the same folded
    /// `return true` shape as on `Button`, and the same consequence:
    /// without it the mouse hit tester can never return a list, so nothing in
    /// one can be clicked, dragged or dropped on. The item-list widget shares it, so without it
    /// the inventory lists are invisible to the pointer. The state-`0xD` (disabled)
    /// arm is the one exception and is reproduced by this type's `disabled` field.
    fn should_be_mouse_visible(&self) -> bool {
        !self.disabled
    }

    /// The original list box begins mouse-down with the shared scrollable behavior, so a press
    /// on a list moves
    /// the focus element.
    ///
    /// This is the case with a **picture**: a list box has no press state of its own, so the
    /// `0x2F` arm moves it out of state 0/1/5. That is retail's own behaviour and it is what
    /// makes focus-driven `0x0A` registration possible at all — a list that can
    /// never hold focus can never register the wheel map.
    fn takes_focus_on_press(&self) -> bool {
        true
    }

    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    /// The mutable list-box downcast used by menu operations.
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }

    /// The scrollable's two attribute arms, reached here because
    /// the list box is a scrollable and its attribute setter chains to
    /// the base.
    fn on_set_attribute(
        &mut self,
        ctx: &mut ElemCtx<'_>,
        id: u32,
        v: Option<&crate::props::PropertyValue>,
    ) {
        use crate::props::PropertyValue;
        let bar = match v {
            Some(PropertyValue::Enum(e)) => Some(crate::ElementId(*e)),
            Some(PropertyValue::Integer(i)) => u32::try_from(*i).ok().map(crate::ElementId),
            _ => None,
        };
        match id {
            crate::scrollable::attr::H_SCROLLBAR => {
                self.scroll.h_scrollbar = bar;
                if bar.is_some() {
                    self.scroll.update_scrollbar_size(ctx.ui, ctx.me, true);
                }
            }
            crate::scrollable::attr::V_SCROLLBAR => {
                self.scroll.v_scrollbar = bar;
                if bar.is_some() {
                    self.scroll.update_scrollbar_size(ctx.ui, ctx.me, false);
                }
            }
            _ => {}
        }
    }

    /// Behavior: register on whichever bars `0x71` and
    /// `0x72` name, and size them once. The registration is what makes the bar's messages
    /// arrive at all: a list and its bar are **siblings** in every shipped char-gen layout, so
    /// nothing the bar broadcasts would otherwise bubble through the list.
    fn post_init(&mut self, ctx: &mut ElemCtx<'_>) {
        if self.scroll.h_scrollbar.is_none() && self.scroll.v_scrollbar.is_none() {
            return;
        }
        let me = ctx.me;
        self.refresh_scroll(ctx.ui, me);
        // The layout update's dirty bit `0x200` is consumed by the client's own layout pass;
        // this crate has none, so the list re-measures itself on the frame tick instead.
        ctx.ui.want_tick(ctx.me, true);
    }

    fn listen_to_element_message(&mut self, ctx: &mut ElemCtx<'_>, m: &ElementMessage) -> R {
        // The scrollable's element-message handler, first arm, which
        // the list box's own handler reaches by chaining to the
        // base. Answered before anything else, because the message comes from the bar and not
        // from a row.
        if let Some(horizontal) =
            self.scroll
                .message_is_from_my_bar(ctx.ui, ctx.me, m.source_id, m.source)
        {
            let delta = if crate::scrollable::Scrollable::is_step_message(m.id) {
                // **The negate group is `0x0D` and `0x0F`, taken from retail rather than
                // reasoned about.** The scrollbar-message helper builds
                // `inq_scroll_delta`'s three arguments as
                //
                // ```text
                //   page     = (id == 0x0F || id == 0x10)    "a page rather than a line"
                //   negative = (id == 0x0D || id == 0x0F)    "negate the step"
                //   horizontal
                // ```
                //
                // and the text element's and the list box's scroll-delta queries both end
                // by negating the step when `negative` is set. It is not `0x0E || 0x0F`, as
                // where the arrows *sit* might suggest: `0x0F` is a track click above the
                // thumb and must page up, and it groups with `0x0D`, the **increment** arrow —
                // the one the scrolling-area update moves to `(0, 0)`, i.e. the top.
                //
                // Getting both this flag and the arrow placement wrong cancels on every
                // vertical bar in the shipped data, so a screenshot cannot see it; it does
                // not cancel on a horizontal bar. The scroll and chat tests assert direction.
                let negative = m.id.0 == 0x0D || m.id.0 == 0x0F;
                let page = m.id.0 == 0x0F || m.id.0 == 0x10;
                self.inq_scroll_delta(ctx.ui, ctx.me, horizontal, negative, page)
            } else {
                0
            };
            let me = ctx.me;
            if self
                .scroll
                .handle_scrollbar_message(ctx.ui, me, horizontal, m.id, delta)
            {
                // Behavior: mark the layout dirty, which is
                // what re-places every row against the new offset.
                self.place_rows(ctx.ui);
            }
            return R::StopProcessing;
        }
        // The base scrollable handler's second arm handles wheel input. See the twin in
        // `text::element_text` for why the reflected message is applied here rather than
        // waited for.
        if let Some(bar) = self.scroll.wheel_target(ctx.ui, ctx.me, m.id, m.p1) {
            let up = m.p1 == crate::focus::action::WHEEL_UP;
            if let Some(id) = crate::widgets::scrollbar::wheel(ctx.ui, bar, up) {
                // The same group as above; see `listen_to_element_message`'s first arm.
                let negative = id.0 == 0x0D || id.0 == 0x0F;
                let page = id.0 == 0x0F || id.0 == 0x10;
                let me = ctx.me;
                let delta = self.inq_scroll_delta(ctx.ui, me, false, negative, page);
                if self
                    .scroll
                    .handle_scrollbar_message(ctx.ui, me, false, id, delta)
                {
                    self.place_rows(ctx.ui);
                }
            }
            return R::StopProcessing;
        }
        // ---- the press, which has two list-box selection sites --------------------------
        // The inherited mouse-down first broadcasts `0x1C`; the list-box handler selects the
        // row for primary action 7 when click-select bit 2 is set. Mouse-down's own tail then
        // selects for action 7 or `0x0A` under the same bit, unless moving or resizing.
        //
        // **Both sites are here, in that order, and that is not a duplicate.** In the client
        // one is reached through the base class's broadcast and the other is the mouse-down's
        // own tail; this build has no separate mouse-down hook on the behaviour — the element's
        // press body is reached from its `0x1C` arm, which is the seam
        // `text::element_text::TextElement::mouse_down` already uses — so the two land in one
        // function. Collapsing them to one call would be a behaviour change, not a cleanup:
        // the **doubling is what makes a double-click two presses**. `set_selected_item` raises
        // `0x43` whenever it is handed the row that is already selected, so press one raises
        // one `0x43` (from the tail, over the row the first call just selected) and press two
        // raises two — and the double-click behavior is armed by the first and fires on
        // the second. A list that ran this once per press would need **three** clicks to open
        // a journal page.
        //
        // The two guards differ and both are transcribed: the `0x1C` arm takes action `7`
        // alone, the tail takes `7` or `0x0A` and is skipped while the element is being moved
        // or resized. Note this is *narrower* than the stat panels' own `0x1C` arms, which also
        // takes `8` and `0xB` — the two stat panels are more permissive than the widget, which
        // is why their copies are not duplicates of this one.
        if m.id == msgid::MOUSE_PRESS {
            let me = ctx.me;
            let (wx, wy) = m.point.window;
            if m.p1 == crate::focus::action::PRIMARY_CLICK {
                self.press_select(ctx.ui, me, wx, wy);
            }
            let busy = ctx
                .ui
                .node(me)
                .is_some_and(|n| n.flags.is_moving() || n.flags.is_resizing());
            if !busy && PRESS_ACTIONS.contains(&m.p1) {
                self.press_select(ctx.ui, me, wx, wy);
            }
            // The list box's handler delegates to the base, so
            // the base still sees the press: it is what moves the element into state 4.
            return R::Default;
        }
        R::Default
    }

    fn listen_to_global_message(&mut self, ctx: &mut ElemCtx<'_>, id: MessageId, _p: u32) {
        if id != crate::msg::global::TICK {
            return;
        }
        // A list that owns a bar keeps ticking: the tick is where the layout update's dirty bit
        // is consumed, and a row added after `post_init` has to reach the bar.
        if self.scroll.h_scrollbar.is_some() || self.scroll.v_scrollbar.is_some() {
            let me = ctx.me;
            self.refresh_scroll(ctx.ui, me);
            return;
        }
        if self.anim.is_none() {
            ctx.ui.want_tick(ctx.me, false);
        }
    }
}
