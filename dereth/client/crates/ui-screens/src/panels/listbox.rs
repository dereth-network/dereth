//! The live list-box widget — the row factory the panel lists are built from.
//!
//! Sources: the list box element's add-from-template-list,
//! its item insert and its layout update, and the stat-management panel's list flush.
//!
//! # Why this exists next to [`ItemListWidget`](crate::items::widget::ItemListWidget)
//!
//! Item-list type `0x10000031` extends list-box type `5` with
//! an item-slot cache: every one of its rows is the *same* `ItemSlot` layout, named
//! once by `UI_ItemList_ItemSlotID`. An ordinary list box has no slot id and no cache — its
//! rows come from **the template list**, element property `0x64`, an array of
//! `(layout DataID, element id)` pairs, and different rows of one list use different entries of it.
//! The skills panel is the clearest case: it builds four
//! group headers from templates 1…4 and every skill row from template 0, all in the one list box
//! `0x1000023D`.
//!
//! So this is not a second item list. It supplies the ordinary row-list behavior reused by item lists, and
//! [`ItemListWidget`](crate::items::widget::ItemListWidget) stays the only thing that creates
//! item-slot elements.
//!
//! # The list box's layout is not the item list's cell arithmetic
//!
//! `ItemListWidget::update_layout` assumes one cell size because every slot of an item list is one
//! `ItemSlot`. A plain list box does not: it keeps a width per column and a height per row, each
//! the **maximum** over the items that land in that column or row, and walks the items placing
//! each at the running sum. A skills list whose headers are taller than its rows lays out
//! correctly only that way, which is why it is reproduced here rather than shared.

use dereth_primitives::DataId;
use dereth_ui::{ElemHandle, ElementId, UiSystem};

/// The template list — element property `0x64`.
///
/// An `Array` whose members are `Struct`s of `0x63` (the template's layout, a `DataFile`) and
/// `0x62` (the template's element id, an `Enum`). The constants live here as well as in
/// `screens::chargen` so that neither module depends on the other.
pub const ATTR_TEMPLATE_LIST: u32 = 0x64;
/// The template struct's layout member.
pub const ATTR_TEMPLATE_LAYOUT: u32 = 0x63;
/// The template struct's element member.
pub const ATTR_TEMPLATE_ELEMENT: u32 = 0x62;
/// `UICore_ListBox_max_columns` — the layout update's only attribute read.
pub const ATTR_MAX_COLUMNS: u32 = 0x5F;
/// `UICore_ListBox_horizontal` — the list box's horizontal flag (bit 0 of its flags).
pub const ATTR_HORIZONTAL: u32 = 0x5C;

/// One live list-box widget.
#[derive(Debug, Clone)]
pub struct ListBoxWidget {
    pub element: ElementId,
    pub handle: ElemHandle,
    /// The template list, in array order — the index [`Self::add_from_template`] takes.
    pub templates: Vec<(DataId, ElementId)>,
    /// The horizontal flag.
    pub horizontal: bool,
    /// The `0x5F` attribute's value.
    ///
    /// **The default is 0, not -1.** The layout update starts its column count at 0 and reads
    /// attribute `0x5F` into it, which leaves it at 0 when the attribute is absent — and
    /// `0 >= 0` takes the `max(v, 1)` arm, so a list box with no `0x5F` is **one column**, not one
    /// row. `0x1000023D` (the skills list) carries no `0x5F` at all, so getting this wrong lays
    /// forty-odd rows out side by side off the right edge of the panel.
    pub max_columns: i32,
    /// The list's items, in list order.
    pub items: Vec<ElemHandle>,
    /// How many rows this list has created since it was bound. Counted for the same reason
    /// `ItemListWidget::created` is: a list that silently creates nothing is the defect.
    pub created: u32,
    /// Row-creation calls that produced no element.
    pub create_failures: u32,
    /// [`Self::mirror_items`] calls that could not reach a
    /// [`dereth_ui::widgets::listbox::ListBox`] behaviour on [`Self::handle`].
    ///
    /// A denominator rather than a silent skip: the mirror is a no-op both when the bound element
    /// is not type `5` at all and when its behaviour is currently lifted out of the arena (a widget
    /// cannot be read back out of the arena from inside its own handler), and those two look
    /// identical from here. A non-zero here with rows created is
    /// the signal that the behaviour's item list is **not** in step with [`Self::items`].
    pub behaviour_unreachable: u32,
    /// The per-column maxima [`Self::update_layout`] computed, kept because
    /// the client walks them to turn a pointer position into a row. Resolving a click by walking
    /// the message's source upwards instead never reaches a row, because the element a press
    /// lands on is the **list box**.
    pub item_widths: Vec<i32>,
    /// The per-row maxima. See [`Self::item_widths`].
    pub item_heights: Vec<i32>,
    /// The column / row counts, as of the last [`Self::update_layout`].
    pub cols: i32,
    /// See [`Self::cols`].
    pub rows: i32,
}

impl ListBoxWidget {
    /// Bind an existing list box and read the three attributes the layout and
    /// [`Self::add_from_template`] use.
    #[must_use]
    pub fn bind(ui: &UiSystem, handle: ElemHandle) -> Self {
        let element = ui
            .node(handle)
            .map_or(ElementId(0), dereth_ui::ElementNode::element_id);
        Self {
            element,
            handle,
            templates: template_list(ui, handle),
            horizontal: crate::bind::attr_bool(ui, handle, ATTR_HORIZONTAL).unwrap_or(false),
            // See the field comment: the client's default is 0 and 0 means one column.
            max_columns: crate::bind::attr_int(ui, handle, ATTR_MAX_COLUMNS).unwrap_or(0),
            items: Vec::new(),
            created: 0,
            create_failures: 0,
            behaviour_unreachable: 0,
            item_widths: Vec::new(),
            item_heights: Vec::new(),
            cols: 0,
            rows: 0,
        }
    }

    /// The scroll offset `(x, y)` — how far the list is scrolled,
    /// **read off the element itself** rather than kept here.
    ///
    /// The list box owns scrollable behavior, so in the client the offset belongs to the
    /// element; in this build the element's half is `dereth_ui::widgets::listbox::scroll`,
    /// which is driven by the shipped scrollbar `0x1000023E` through
    /// the scrollable element's element-message handler and moves the rows on its own.
    /// A second copy here could only ever disagree with it: the rows would scroll while
    /// [`Self::inq_item_index_at_point`] went on adding a zero, so a click at a scrolled viewport
    /// would resolve to the row that *would* have been there unscrolled.
    ///
    /// `(0, 0)` when the element carries no list-box behaviour, and when this is called from
    /// inside that behaviour's own handler — see
    /// [`dereth_ui::widgets::listbox::scroll_offset_of`].
    #[must_use]
    pub fn scroll(&self, ui: &UiSystem) -> (i32, i32) {
        dereth_ui::widgets::listbox::scroll_offset_of(ui, self.handle).unwrap_or((0, 0))
    }

    /// The list box element's item insert / the insert item / the delete item
    /// all end by writing **the item list**, which is one array on the element and not two.
    ///
    /// This binder keeps [`Self::items`] **and** mirrors it into
    /// `dereth_ui::widgets::listbox::items`. Without the mirror a screen-side list box has an
    /// empty item list while its rows are real elements in the tree. The consequence is in
    /// listen_to_element_message, whose row arm is
    /// `if let Some(i) = self.items.iter().position(|it| *it == m.source)`: with the array empty
    /// the position is always `None`, so a click on a row raises **no**
    /// `LIST_SELECTION_CHANGED` (element message `0x04`) and a double click raises no
    /// `LIST_ITEM_ACTIVATED` (`0x43`) — setting the selected item, getting an item,
    /// the item count and the selected index all answer as though the list were empty.
    ///
    /// It is written as a mirror rather than as an insert because this binder owns the order (it
    /// is the thing the sorted-skill insert re-orders) and the behaviour is the element's own
    /// copy of it. The selected item is a **reference to the element**, so the selection is carried across by
    /// handle and not by index — an insert before the selected row moves its index and does not
    /// change what is selected, which is the client's own `if (at <= sel) sel + 1`.
    fn mirror_items(&mut self, ui: &mut UiSystem) {
        let items = self.items.clone();
        let Some(l) = ui.node_mut(self.handle).and_then(|n| {
            n.behaviour
                .as_mut()?
                .as_any_mut()?
                .downcast_mut::<dereth_ui::widgets::listbox::ListBox>()
        }) else {
            self.behaviour_unreachable += 1;
            return;
        };
        let selected = l.selected.and_then(|i| l.items.get(i).copied());
        l.items = items;
        l.selected = selected.and_then(|h| l.items.iter().position(|x| *x == h));
    }

    /// Delete every row and empty the item list.
    ///
    /// The client deletes the `InfoRegion` objects and then removing all items
    /// puts each row element on the delete queue; here the elements go straight out of the tree,
    /// which is what the delete queue amounts to once the frame ends.
    pub fn flush(&mut self, ui: &mut UiSystem) {
        for h in std::mem::take(&mut self.items) {
            ui.remove_and_delete_root(h);
        }
        self.mirror_items(ui);
    }

    /// The list box element's item from template list insert — create a row from
    /// template `index` and append it, or use the indexed insertion path when `insert_at`
    /// is given, which
    /// inserts before the item currently at that index.
    ///
    /// Both end in the item insert: create a child element from the template's layout DataID
    /// and element id, then append or insert it, and destroy the element again if the insert
    /// fails.
    pub fn add_from_template(
        &mut self,
        ui: &mut UiSystem,
        index: usize,
        insert_at: Option<usize>,
    ) -> Option<ElemHandle> {
        let (layout, element) = *self.templates.get(index)?;
        match ui
            .require_env()
            .and_then(|e| e.create_child_element_by_data_id(ui, self.handle, layout, element))
        {
            Ok(h) => {
                self.created += 1;
                let at = insert_at.unwrap_or(self.items.len()).min(self.items.len());
                self.items.insert(at, h);
                self.mirror_items(ui);
                Some(h)
            }
            Err(_) => {
                self.create_failures += 1;
                None
            }
        }
    }

    /// Create a child element from a layout enum, then insert it —
    /// the *other* way a row gets into a list box.
    ///
    /// [`Self::add_from_template`] above resolves a row from the list's own
    /// template list; the other route does not use it at all. It creates element `0x1000004A`
    /// from layout **enum** `0x10000012`, with no parent, and then inserts it at 0. Passing the
    /// list box as the parent here is the same tree in one step: the insert is what reparents it
    /// in the client, and nothing observes the element between the two calls.
    pub fn add_from_layout_enum(
        &mut self,
        ui: &mut UiSystem,
        layout: dereth_ui::framework::LayoutEnum,
        element: ElementId,
        insert_at: Option<usize>,
    ) -> Option<ElemHandle> {
        match ui
            .require_env()
            .and_then(|e| e.create_child_element_by_enum(ui, self.handle, layout, element))
        {
            Ok(h) => {
                self.created += 1;
                let at = insert_at.unwrap_or(self.items.len()).min(self.items.len());
                self.items.insert(at, h);
                self.mirror_items(ui);
                Some(h)
            }
            Err(_) => {
                self.create_failures += 1;
                None
            }
        }
    }

    /// Delete item `index` — drop one row and destroy its element.
    pub fn delete_item(&mut self, ui: &mut UiSystem, index: usize) -> bool {
        if index >= self.items.len() {
            return false;
        }
        let h = self.items.remove(index);
        ui.remove_and_delete_root(h);
        self.mirror_items(ui);
        true
    }

    /// The index of a row in the item list — the client's own linear scan over
    /// the items, which is how the skills panel turns a header element into
    /// the bound of a group.
    #[must_use]
    pub fn index_of(&self, h: ElemHandle) -> Option<usize> {
        self.items.iter().position(|i| *i == h)
    }

    /// The list box element's layout update.
    ///
    /// Attribute `0x5F` gives the column limit (absent reads as 0). A negative limit makes one
    /// row of `n` columns (no rows when `n` is 0); otherwise there are `min(max(limit, 1), n)`
    /// columns and `ceil(n / cols)` rows. Each column's width is the widest item in it and each
    /// row's height the tallest, and each item is placed at the running sum, wrapping on the
    /// column count (horizontal) or the row count (the default, which fills column-major).
    ///
    /// Returns `(cols, rows)`, which is what a test asserts against instead of a pixel.
    ///
    /// # This is the **second** implementation of this layout in the workspace
    ///
    /// `dereth_ui::widgets::listbox::update_layout` (for popups built inside `dereth-ui`, which
    /// have no screen-side binder) is the other. A differential test asserts that the two agree
    /// over a swept grid of row counts, sizes, `0x5F` values and both orientations, so the
    /// agreement is measured rather than remembered.
    ///
    /// **Why there are two**: collapsing them means calling the behaviour's method, which needs `UiSystem::take_behaviour` /
    /// `put_behaviour` (they are `pub(crate)` in `dereth-ui`, and they carry the `lifted` counter
    /// that gates the deferred mouse-visibility and state flushes — hand-rolling the pair out of
    /// the public `ElementNode::behaviour` field would bypass that bookkeeping). So the merge
    /// is a `dereth-ui` change.
    ///
    /// One deliberate difference, in the client's favour on the other side: this reads `0x5F` and
    /// `0x5C` as cached by [`Self::bind`], where the client's layout update re-reads both attributes on every
    /// call. Nothing in this build rewrites either attribute after a list is bound.
    pub fn update_layout(&mut self, ui: &mut UiSystem) -> (i32, i32) {
        let n = i32::try_from(self.items.len()).unwrap_or(0);
        let (cols, rows) = if self.max_columns < 0 {
            (n, i32::from(n != 0))
        } else {
            let cols = self.max_columns.max(1).min(n);
            let rows = if cols == 0 { 0 } else { (n + cols - 1) / cols };
            (cols, rows)
        };
        self.cols = cols.max(0);
        self.rows = rows.max(0);
        if cols <= 0 || rows <= 0 {
            self.item_widths.clear();
            self.item_heights.clear();
            return (cols.max(0), rows.max(0));
        }
        let ncols = usize::try_from(cols).unwrap_or(0);
        let nrows = usize::try_from(rows).unwrap_or(0);
        let mut widths = vec![0i32; ncols];
        let mut heights = vec![0i32; nrows];

        // Pass 1 — the per-column widths and per-row heights, as maxima.
        let (mut col, mut row) = (0usize, 0usize);
        for h in &self.items {
            let b = ui.node(*h).map(|nd| nd.region.box_).unwrap_or_default();
            if let Some(w) = widths.get_mut(col) {
                *w = (*w).max(b.width());
            }
            if let Some(ht) = heights.get_mut(row) {
                *ht = (*ht).max(b.height());
            }
            if self.horizontal {
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

        // Pass 2 — place each item at its **unscrolled** `running_sum`.
        //
        // The client's own pass 2 places at `running_sum - scroll offset`, and the subtraction
        // is deliberately *not* done here. It is done by the element's own scrollable behavior
        // half — `dereth_ui::widgets::listbox::place_rows` — which is called through
        // [`refresh_scroll_of`](dereth_ui::widgets::listbox::refresh_scroll_of) at the end of this
        // function, on the same frame. That half records each row's unscrolled position as its
        // *origin* and re-derives the placed box from it on every offset change, so it has to see
        // the grid untranslated; subtracting the offset in both places would apply it twice, and
        // a rebuild while scrolled would then bake the offset into the origins for good.
        let (mut x, mut y) = (0i32, 0i32);
        let (mut col, mut row) = (0usize, 0usize);
        for h in &self.items {
            ui.move_to(*h, x, y);
            if self.horizontal {
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
        self.item_widths = widths;
        self.item_heights = heights;
        // The client's tail: resize the scrollable area to `(sum(widths), sum(heights))`,
        // which sizes the thumb and enables or disables the bar, and then the placement against
        // the offset. One call, one frame, exactly where the client has it.
        dereth_ui::widgets::listbox::refresh_scroll_of(ui, self.handle);
        (cols, rows)
    }

    /// Which row is at an
    /// **element-relative** point.
    ///
    /// No items, or a point outside the element's width or height, is no row. Otherwise the
    /// layout is updated, the scroll offset is added to the point, the column is the first whose
    /// running width sum reaches `x` (else 0), the row the first whose running height sum reaches
    /// `y` (else 0), and the index is `cols * row + col` when horizontal, else `rows * col + row`;
    /// it is a row if it is below the item count.
    ///
    /// **The `else 0` is the client's, not a simplification**: each loop's result is left at the
    /// value it was initialised with when the accumulation never reaches the point, and both are
    /// initialised to 0. The widths and heights are [`Self::item_widths`] /
    /// [`Self::item_heights`], written by [`Self::update_layout`], which is the same
    /// layout update the client calls first.
    ///
    /// **The scroll offset is [`Self::scroll`], read live off the element.** The shipped
    /// `0x1000023D` names the scrollbar `0x1000023E` in attribute `0x72`, the element's own
    /// scrollable behavior consumes its messages and moves the rows, and the bands below
    /// are the **unscrolled** grid. Ignoring the offset breaks a scrolled list: six presses of
    /// the skills panel's lower arrow put the viewport at a y offset of 240, and a click ten pixels
    /// below the top of the list box — over the row of skill **6** — would resolve to item **0**,
    /// the "Specialized" header.
    #[must_use]
    pub fn inq_item_index_at_point(&self, ui: &UiSystem, x: i32, y: i32) -> Option<usize> {
        if self.items.is_empty() {
            return None;
        }
        let b = ui.node(self.handle).map(|n| n.region.box_)?;
        if x < 0 || b.width() <= x || y < 0 || b.height() <= y {
            return None;
        }
        let band = |sizes: &[i32], at: i32| -> i32 {
            let mut acc = 0i32;
            for (i, s) in sizes.iter().enumerate() {
                acc += *s;
                if at <= acc {
                    return i32::try_from(i).unwrap_or(0);
                }
            }
            0
        };
        let (sx, sy) = self.scroll(ui);
        let col = band(&self.item_widths, x + sx);
        let row = band(&self.item_heights, y + sy);
        let index = if self.horizontal {
            self.cols * row + col
        } else {
            self.rows * col + row
        };
        let index = usize::try_from(index).ok()?;
        (index < self.items.len()).then_some(index)
    }

    /// The list box element's item under mouse read.
    ///
    /// Retail answers nothing unless the mouse is over the list box; otherwise it takes the
    /// mouse position relative to the list box's screen origin and returns the item at that
    /// point, if any.
    ///
    /// `(wx, wy)` is the pointer in **window** coordinates — `ElementMessage::point.window`, which
    /// is stamped from the same mouse position
    /// the client reads here. The mouse-over guard is subsumed by
    /// [`Self::inq_item_index_at_point`]'s own bounds test: a point outside the list box's own
    /// rectangle is exactly the case the mouse-over flag is false for.
    ///
    /// **This is the function the two stat panels needed and did not have.** See
    /// [`Self::item_widths`].
    #[must_use]
    pub fn item_under_mouse(&self, ui: &UiSystem, wx: i32, wy: i32) -> Option<ElemHandle> {
        let b = ui.screen_box(self.handle);
        let i = self.inq_item_index_at_point(ui, wx - b.x0, wy - b.y0)?;
        self.items.get(i).copied()
    }

    /// Whether an element message with this source belongs to **this** list box —
    /// the list box itself or a descendant of it.
    ///
    /// **This is the client's bubble, which this crate has to
    /// supply by hand.** In the client each panel receives element messages only from
    /// its own subtree. Here the
    /// **screen** is the registered listener and `RemainingPanels::on_element_message` fans one
    /// message out to every panel in turn, so without this test the first panel offered a press
    /// answers for the second's.
    ///
    /// That is not hypothetical and it is why this method exists: the attribute and skill panels
    /// are two sibling panels on the same page, and **both** carry a `0x1000023D`,
    /// and the two list boxes sit at the *same screen rectangle*. A geometric lookup alone
    /// therefore lets the attributes panel — which is offered the message first — claim every
    /// press meant for a skill row. Measured: with the check removed, a press on a skill row sets
    /// the attributes panel's selected index and the skills panel's selected skill stays 0.
    #[must_use]
    pub fn owns(&self, ui: &UiSystem, mut h: ElemHandle) -> bool {
        for _ in 0..32 {
            if h == self.handle {
                return true;
            }
            match ui.parent(h) {
                Some(p) => h = p,
                None => return false,
            }
        }
        false
    }

    /// The list box element's column and row calculation.
    ///
    /// No rows or no columns (retail also asserts), or an index outside the items, gives
    /// `(-1, -1)`. Otherwise, when horizontal the column is `index % cols` and the row
    /// `index / cols`; by default the column is `index / rows` and the row `index % rows`.
    ///
    /// The quotient and the remainder swap with the horizontal bit, and the divisor swaps with
    /// them: horizontal divides by the column count, column-major by the row count. \[verified\]
    #[must_use]
    pub fn cell_of(&self, index: usize) -> Option<(i32, i32)> {
        if self.rows <= 0 || self.cols <= 0 || index >= self.items.len() {
            return None;
        }
        let i = i32::try_from(index).ok()?;
        Some(if self.horizontal {
            (i % self.cols.max(1), i / self.cols.max(1))
        } else {
            (i / self.rows.max(1), i % self.rows.max(1))
        })
    }

    /// Move the viewport by the least amount that
    /// brings item `index` inside the list box's own rectangle.
    ///
    /// An index outside the items does nothing. Otherwise the layout is updated and the item's
    /// cell found (nothing if it has none); `x` and `y` are the sums of the widths and heights
    /// before it, `(sx, sy)` the scroll offset, `w`/`h` the list box's size and `iw`/`ih` the
    /// item's (0 if missing). Then, in order: `x < sx` or `y < sy` scrolls to `(x, y)`;
    /// `x > sx + w - iw` scrolls to `(x - w + iw, y)`; `y > sy + h - ih` scrolls to
    /// `(x, y - h + ih)`; otherwise the item is already fully visible.
    ///
    /// **Both "before the viewport" arms scroll to the item's own top-left**, not just along the
    /// offending axis — every scrolling arm sets the offset from the same `x` and `y`,
    /// and only the two "after the viewport" arms adjust one of them first. The scrollable element then clamps into
    /// `0 ..= content - view` unless attribute `0x73` says otherwise, which is what stops the last
    /// row scrolling off the bottom. \[verified\]
    ///
    /// It drives the element's own scrollable state, which is where the client keeps it; a private
    /// offset would leave the bar unaware that the viewport moved and place the rows twice against
    /// two different offsets.
    ///
    /// Returns whether the offset moved. **The original stat-management panel does not call this** —
    /// its selection arm sets the selection, not the selected index, and it is
    /// the selected-index write that auto-scrolls (through the scroll-to-show path). It is
    /// reproduced because retail's spellbook selection and
    /// the spell-cast sub-menu call it on their item lists. The spellbook's
    /// production consumer now shares this inherited implementation.
    pub fn scroll_to_view(&mut self, ui: &mut UiSystem, index: usize) -> bool {
        let Some(item) = self.items.get(index).copied() else {
            return false;
        };
        // Shared with ItemList's live consumers: one scroll-to-view branch order, one offset.
        dereth_ui::widgets::listbox::scroll_item_to_view(ui, self.handle, item)
    }
}

/// The template list off a live element — property [`ATTR_TEMPLATE_LIST`].
#[must_use]
pub fn template_list(ui: &UiSystem, h: ElemHandle) -> Vec<(DataId, ElementId)> {
    use dereth_assets::ui::PropertyValue;
    let Some(node) = ui.node(h) else {
        return Vec::new();
    };
    let merged = node.merged_properties();
    let Some(PropertyValue::Array(items)) = merged.get(ATTR_TEMPLATE_LIST) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for it in items {
        let PropertyValue::Struct(members) = &it.value else {
            continue;
        };
        let mut layout = None;
        let mut element = None;
        for (id, m) in members {
            match (*id, &m.value) {
                (ATTR_TEMPLATE_LAYOUT, PropertyValue::DataFile(d)) => layout = Some(*d),
                (ATTR_TEMPLATE_ELEMENT, PropertyValue::Enum(e)) => element = Some(ElementId(*e)),
                _ => {}
            }
        }
        if let (Some(l), Some(e)) = (layout, element) {
            out.push((l, e));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No layout in this module's tests references another, so nothing is ever read.
    #[derive(Debug)]
    struct NoAssets;
    impl dereth_primitives::AssetSource for NoAssets {
        fn read(&self, id: DataId) -> Result<Vec<u8>, dereth_primitives::AssetError> {
            Err(dereth_primitives::AssetError::NotFound(id))
        }
        fn exists(&self, _: DataId) -> bool {
            false
        }
        fn iter_type(
            &self,
            _: dereth_primitives::DataType,
        ) -> Box<dyn Iterator<Item = DataId> + '_> {
            Box::new(std::iter::empty())
        }
    }

    fn real_widget(
        sizes: &[(i32, i32)],
        cols: i32,
        horizontal: bool,
        view: (i32, i32),
    ) -> (UiSystem, ListBoxWidget) {
        use dereth_ui::desc::{incorporation, ElementDesc, LayoutDesc, StateDesc};
        const LIST: u32 = 0x1000_023D;
        let mut ui = UiSystem::new((800, 600));
        let d = ElementDesc {
            base: StateDesc {
                incorporation: incorporation::LEGACY_ALL_GEOMETRY,
                width: view.0,
                height: view.1,
                ..StateDesc::default()
            },
            element_id: ElementId(LIST),
            ty: dereth_ui::factory::ty::LISTBOX,
            ..ElementDesc::default()
        };
        let l = LayoutDesc {
            did: DataId(0x2100_0001),
            display_width: 800,
            display_height: 600,
            elements: std::iter::once((ElementId(LIST), d.clone())).collect(),
        };
        let h = ui
            .create_element_recursive_from_full_desc(&NoAssets, &l, &d)
            .expect("no inheritance to resolve")
            .expect("type 5 is registered");
        let root = ui.root();
        ui.set_parent(h, Some(root));
        // The two attributes the layout reads, set through the element's own
        // setter so `ListBoxWidget::bind` sees exactly what a shipped layout would give it.
        ui.set_attribute_int(h, ATTR_MAX_COLUMNS, cols);
        ui.set_attribute_bool(h, ATTR_HORIZONTAL, horizontal);
        ui.initialize_tree(h);

        let mut w = ListBoxWidget::bind(&ui, h);
        assert_eq!(w.horizontal, horizontal, "bind read 0x5C");
        assert_eq!(w.max_columns, cols, "bind read 0x5F");
        for (cw, ch) in sizes {
            let e = ui.create_hollow(None);
            ui.set_parent(e, Some(h));
            ui.resize_to(e, *cw, *ch);
            w.items.push(e);
        }
        w.update_layout(&mut ui);
        (ui, w)
    }

    /// A widget with `n` synthetic rows of a known size, for the pure layout arithmetic.
    fn widget(sizes: &[(i32, i32)], cols: i32, horizontal: bool) -> (UiSystem, ListBoxWidget) {
        let mut ui = UiSystem::new((800, 600));
        let root = ui.create_hollow(None);
        let mut items = Vec::new();
        for (w, h) in sizes {
            let e = ui.create_hollow(None);
            ui.resize_to(e, *w, *h);
            items.push(e);
        }
        let w = ListBoxWidget {
            element: ElementId(0x1000_023D),
            handle: root,
            templates: Vec::new(),
            horizontal,
            max_columns: cols,
            items,
            created: 0,
            create_failures: 0,
            behaviour_unreachable: 0,
            item_widths: Vec::new(),
            item_heights: Vec::new(),
            cols: 0,
            rows: 0,
        };
        (ui, w)
    }

    /// Oracle: the client's column/row count computation — the
    /// **absent** attribute reads 0, and `0 >= 0` takes the `max(v, 1)` arm, so no `0x5F` means
    /// one column and not one row. This is the arm `ItemListWidget` does not have, and getting it
    /// the other way round lays the skills list out sideways.
    #[test]
    fn a_list_box_with_no_max_columns_attribute_is_one_column() {
        let (mut ui, mut w) = widget(&[(100, 10); 4], 0, false);
        assert_eq!(w.update_layout(&mut ui), (1, 4));
        // The explicit `-1` is the other arm: one row of everything.
        let (mut ui, mut w) = widget(&[(100, 10); 4], -1, false);
        assert_eq!(w.update_layout(&mut ui), (4, 1));
        // And an explicit count is clamped to the item count.
        let (mut ui, mut w) = widget(&[(100, 10); 3], 6, false);
        assert_eq!(w.update_layout(&mut ui), (3, 1));
    }

    /// Oracle: the same function's two placement passes — a row's height is the **maximum**
    /// height of the items in that row, so a tall header pushes the row below it down by its own
    /// height and not by the rows' height. `ItemListWidget`'s uniform-cell version cannot express
    /// this, which is the whole reason this module exists.
    #[test]
    fn a_row_is_as_tall_as_its_tallest_item() {
        // One column: a 20-tall header, then three 10-tall rows.
        let (mut ui, mut w) = widget(&[(100, 20), (100, 10), (100, 10), (100, 10)], 0, false);
        assert_eq!(w.update_layout(&mut ui), (1, 4));
        let tops: Vec<i32> = w
            .items
            .iter()
            .map(|h| ui.node(*h).unwrap().region.box_.y0)
            .collect();
        assert_eq!(
            tops,
            vec![0, 20, 30, 40],
            "the header's own 20 is the first row's height"
        );
    }

    /// Oracle: the same function — the default (horizontal bit clear) fills **column-major**, one
    /// column of `rows` at a time, and the horizontal bit fills row-major across `cols`.
    #[test]
    fn the_horizontal_bit_swaps_the_fill_order() {
        let sizes = [(50, 10); 4];
        let (mut ui, mut w) = widget(&sizes, 2, true);
        assert_eq!(w.update_layout(&mut ui), (2, 2));
        let pos: Vec<(i32, i32)> = w
            .items
            .iter()
            .map(|h| {
                let b = ui.node(*h).unwrap().region.box_;
                (b.x0, b.y0)
            })
            .collect();
        assert_eq!(pos, vec![(0, 0), (50, 0), (0, 10), (50, 10)], "row-major");

        let (mut ui, mut w) = widget(&sizes, 2, false);
        assert_eq!(w.update_layout(&mut ui), (2, 2));
        let pos: Vec<(i32, i32)> = w
            .items
            .iter()
            .map(|h| {
                let b = ui.node(*h).unwrap().region.box_;
                (b.x0, b.y0)
            })
            .collect();
        assert_eq!(
            pos,
            vec![(0, 0), (0, 10), (50, 0), (50, 10)],
            "column-major"
        );
    }

    /// A point inside the list box names the row whose band it falls in.
    #[test]
    fn a_point_inside_the_list_box_names_the_row_whose_band_it_falls_in() {
        // Four 20-tall rows in one 100-wide column, plus a taller header first.
        let (mut ui, mut w) = widget(&[(100, 30), (100, 20), (100, 20), (100, 20)], 0, false);
        ui.resize_to(w.handle, 100, 90);
        assert_eq!(w.update_layout(&mut ui), (1, 4));
        assert_eq!(w.item_heights, vec![30, 20, 20, 20], "row heights");
        assert_eq!(w.item_widths, vec![100], "column widths");
        assert_eq!(
            w.inq_item_index_at_point(&ui, 50, 0),
            Some(0),
            "the header's own band"
        );
        assert_eq!(w.inq_item_index_at_point(&ui, 50, 29), Some(0));
        assert_eq!(
            w.inq_item_index_at_point(&ui, 50, 30),
            Some(0),
            "sum 30, `at <= acc`"
        );
        assert_eq!(
            w.inq_item_index_at_point(&ui, 50, 31),
            Some(1),
            "the first real row"
        );
        assert_eq!(w.inq_item_index_at_point(&ui, 50, 50), Some(1));
        assert_eq!(w.inq_item_index_at_point(&ui, 50, 51), Some(2));
        assert_eq!(
            w.inq_item_index_at_point(&ui, 50, 89),
            Some(3),
            "the last row"
        );
        // The two rejections the point lookup opens with.
        assert_eq!(w.inq_item_index_at_point(&ui, -1, 40), None, "x < 0");
        assert_eq!(w.inq_item_index_at_point(&ui, 100, 40), None, "width <= x");
        assert_eq!(w.inq_item_index_at_point(&ui, 50, -1), None, "y < 0");
        assert_eq!(w.inq_item_index_at_point(&ui, 50, 90), None, "height <= y");
        // An empty list answers nothing at all, whatever the point.
        let (ui2, w2) = widget(&[], 0, false);
        assert_eq!(w2.inq_item_index_at_point(&ui2, 0, 0), None, "no items");
    }

    /// The horizontal bit swaps which axis the index strides by.
    #[test]
    fn the_horizontal_bit_swaps_which_axis_the_index_strides_by() {
        let sizes = [(50, 10); 4];
        let (mut ui, mut w) = widget(&sizes, 2, true);
        ui.resize_to(w.handle, 100, 20);
        assert_eq!(w.update_layout(&mut ui), (2, 2));
        // Row-major: (0,0)=0 (50,0)=1 (0,10)=2 (50,10)=3.
        assert_eq!(w.inq_item_index_at_point(&ui, 10, 5), Some(0));
        assert_eq!(w.inq_item_index_at_point(&ui, 60, 5), Some(1));
        assert_eq!(w.inq_item_index_at_point(&ui, 10, 15), Some(2));
        assert_eq!(w.inq_item_index_at_point(&ui, 60, 15), Some(3));

        let (mut ui, mut w) = widget(&sizes, 2, false);
        ui.resize_to(w.handle, 100, 20);
        assert_eq!(w.update_layout(&mut ui), (2, 2));
        // Column-major: (0,0)=0 (0,10)=1 (50,0)=2 (50,10)=3.
        assert_eq!(w.inq_item_index_at_point(&ui, 10, 5), Some(0));
        assert_eq!(w.inq_item_index_at_point(&ui, 10, 15), Some(1));
        assert_eq!(w.inq_item_index_at_point(&ui, 60, 5), Some(2));
        assert_eq!(w.inq_item_index_at_point(&ui, 60, 15), Some(3));
    }

    /// The item under the mouse is the window point less the list boxs screen origin.
    #[test]
    fn the_item_under_the_mouse_is_the_window_point_less_the_list_boxs_screen_origin() {
        let (mut ui, mut w) = widget(&[(100, 20); 3], 0, false);
        ui.move_to(w.handle, 400, 250);
        ui.resize_to(w.handle, 100, 60);
        assert_eq!(w.update_layout(&mut ui), (1, 3));
        let want = w.items.clone();
        assert_eq!(w.item_under_mouse(&ui, 450, 260), Some(want[0]));
        assert_eq!(w.item_under_mouse(&ui, 450, 280), Some(want[1]));
        assert_eq!(w.item_under_mouse(&ui, 450, 300), Some(want[2]));
        // Outside the box in either axis is the not-mouse-over answer.
        assert_eq!(w.item_under_mouse(&ui, 450, 249), None, "above");
        assert_eq!(w.item_under_mouse(&ui, 450, 310), None, "below");
        assert_eq!(w.item_under_mouse(&ui, 399, 260), None, "left");
        assert_eq!(w.item_under_mouse(&ui, 500, 260), None, "right");
    }

    /// The horizontal fixture the three tests below share: 15 items of 40 x 30 in 5 columns,
    /// inside a 100 x 45 rectangle. Content 200 x 90, so the offset can travel 100 in x and 45
    /// in y and nothing here meets the scrollable-XY write's clamp.
    fn h_list() -> (UiSystem, ListBoxWidget) {
        let (mut ui, w) = real_widget(&[(40, 30); 15], 5, true, (100, 45));
        assert_eq!((w.cols, w.rows), (5, 3), "cols / rows");
        assert_eq!(w.item_widths, vec![40; 5], "column widths, five entries");
        assert_eq!(w.item_heights, vec![30; 3], "row heights, three");
        assert_eq!(
            w.scroll(&ui),
            (0, 0),
            "and a real scrollable half, reading its own offset"
        );
        // The premise, asserted rather than assumed: this element *can* be scrolled in x. A
        // fixture whose content fits the viewport would satisfy every assertion below by never
        // moving. The stated testability rule: assert the premise, not only the difference.
        assert!(
            dereth_ui::widgets::listbox::set_scroll_offset(&mut ui, w.handle, 100, 45),
            "the content is 200 x 90 in a 100 x 45 view, so the offset has somewhere to go"
        );
        assert_eq!(
            w.scroll(&ui),
            (100, 45),
            "and it is not clamped short of it"
        );
        dereth_ui::widgets::listbox::set_scroll_offset(&mut ui, w.handle, 0, 0);
        (ui, w)
    }

    /// A horizontal list boxs cells are row major and a column major one is not.
    #[test]
    fn a_horizontal_list_boxs_cells_are_row_major_and_a_column_major_one_is_not() {
        let (_ui, w) = h_list();
        let cells: Vec<Option<(i32, i32)>> = (0..16).map(|i| w.cell_of(i)).collect();
        assert_eq!(
            cells,
            vec![
                Some((0, 0)),
                Some((1, 0)),
                Some((2, 0)),
                Some((3, 0)),
                Some((4, 0)),
                Some((0, 1)),
                Some((1, 1)),
                Some((2, 1)),
                Some((3, 1)),
                Some((4, 1)),
                Some((0, 2)),
                Some((1, 2)),
                Some((2, 2)),
                Some((3, 2)),
                Some((4, 2)),
                // An index at or past the item count is the column calculation's second rejection.
                None,
            ],
            "row-major: (i % 5, i / 5)"
        );

        // The same fifteen items, the same five columns, the bit cleared: column-major, so the
        // divisor is `rows` and the pair comes out the other way round.
        let (_ui, cm) = real_widget(&[(40, 30); 15], 5, false, (100, 45));
        assert_eq!((cm.cols, cm.rows), (5, 3));
        assert_eq!(cm.cell_of(6), Some((2, 0)), "column-major: (6 / 3, 6 % 3)");
        assert_ne!(
            cm.cell_of(6),
            w.cell_of(6),
            "and it disagrees with the horizontal answer"
        );
        assert_eq!(cm.cell_of(1), Some((0, 1)));
        assert_ne!(cm.cell_of(1), w.cell_of(1));
    }

    /// **The x "after the viewport" arm, at its own exclusive boundary.**
    ///
    /// The client skips the arm when `x == sx + w - iw`, so an item whose left edge sits
    /// exactly at the last fully-visible position does **not** move the viewport. With the offset
    /// at 20 that boundary is `20 + 100 - 40 = 80`, which is item 2's own `x` — so item 2 is the
    /// station on the boundary and item 3 is the one past it. An inclusive comparison scrolls on
    /// item 2 and this test says so.
    ///
    /// The move itself is `x - w + iw` and it leaves `y` **alone**, which is the half that
    /// distinguishes this arm from the two before it.
    #[test]
    fn an_item_past_the_right_edge_scrolls_in_x_only_and_the_boundary_is_exclusive() {
        let (mut ui, mut w) = h_list();
        dereth_ui::widgets::listbox::set_scroll_offset(&mut ui, w.handle, 20, 0);
        assert_eq!(w.scroll(&ui), (20, 0));

        // Item 2 is at x = 80 == sx + w - iw. Not past -> already visible, no move.
        assert!(
            !w.scroll_to_view(&mut ui, 2),
            "x == sx + w - iw is inside, not past"
        );
        assert_eq!(w.scroll(&ui), (20, 0), "and nothing moved");

        // Item 3 is at x = 120, one item past it.
        assert!(w.scroll_to_view(&mut ui, 3), "x > sx + w - iw");
        assert_eq!(
            w.scroll(&ui),
            (60, 0),
            "x - w + iw = 120 - 100 + 40; y untouched"
        );

        // And the item really is inside the rectangle afterwards, which is what the function is
        // for -- asserted on the geometry rather than on the offset arithmetic that produced it.
        let b = ui.node(w.handle).expect("alive").region.box_;
        let item = ui.node(w.items[3]).expect("alive").region.box_;
        assert!(
            item.x0 >= 0 && item.x1 <= b.width(),
            "item 3 must sit inside the list box's own width: {item:?} in {b:?}"
        );
    }

    /// **The x-after boundary is only observable through the arm it stands in front of, and the
    /// station that misses that is worth keeping as a comment.**
    ///
    /// The first version of the test above asserted `x == sx + w - iw` by requiring
    /// `scroll_to_view` to answer `false`, and a mutation making the comparison `>=` **survived**
    /// it. The reason is arithmetic, not laziness: the arm's own answer is `x - w + iw`, which at
    /// exactly the boundary *is* `sx`, so taking the arm there writes the offset it already has.
    /// From the offset alone, `>` and `>=` are indistinguishable for ever.
    ///
    /// What separates them is that the arms are a **chain**. Taking the x arm at the boundary
    /// skips the `else if` below it, so an item that is on the x boundary **and** past the y
    /// boundary scrolls in y under the client and does not under the mutant. Both coordinates
    /// come out different, and the client's answer is the surprising one — the y arm writes the
    /// item's own `x`, not the offset's.
    ///
    /// Item 12 is cell (2, 2): `x = 80`, which is exactly `20 + 100 - 40`, and `y = 60`, which is
    /// past `0 + 45 - 30`.
    #[test]
    fn an_item_on_the_x_boundary_and_below_the_bottom_falls_through_to_the_y_arm() {
        let (mut ui, mut w) = h_list();
        dereth_ui::widgets::listbox::set_scroll_offset(&mut ui, w.handle, 20, 0);
        assert_eq!(
            w.cell_of(12),
            Some((2, 2)),
            "x = 80 = sx + w - iw, y = 60 > sy + h - ih"
        );

        assert!(w.scroll_to_view(&mut ui, 12));
        assert_eq!(
            w.scroll(&ui),
            (80, 45),
            "the x arm is skipped on equality, so the y arm runs and writes the item's own x; \
             an inclusive x comparison answers (20, 45) and never scrolls down"
        );
    }

    /// **The x-after arm leaves `y` exactly where it was — and saying so needs `sy != 0` and an
    /// item whose `y` is not before the viewport, or the assertion is vacuous.**
    ///
    /// A mutation zeroing the arm's `y` also **survived** the first version of this file, because
    /// every station it had happened to run at `sy = 0`. The stated testability rule's *"look for the
    /// asymmetric case"* applies inside a test as much as between fixtures: a station at the
    /// origin cannot tell "kept" from "zeroed".
    ///
    /// Item 8 is cell (3, 1): `x = 120`, past `20 + 100 - 40`; `y = 30`, which is neither before
    /// the viewport (`sy = 30`) nor past it, so no y arm could be the one that fires.
    #[test]
    fn the_x_after_arm_leaves_a_non_zero_y_offset_alone() {
        let (mut ui, mut w) = h_list();
        dereth_ui::widgets::listbox::set_scroll_offset(&mut ui, w.handle, 20, 30);
        assert_eq!(
            w.scroll(&ui),
            (20, 30),
            "a non-zero y offset, which is the whole point"
        );
        assert_eq!(w.cell_of(8), Some((3, 1)));

        assert!(w.scroll_to_view(&mut ui, 8), "x = 120 > sx + w - iw = 80");
        assert_eq!(
            w.scroll(&ui),
            (60, 30),
            "x - w + iw = 60, and y is carried through untouched at 30"
        );
    }

    /// The x before arm writes the items y as well even when y was already visible.
    #[test]
    fn the_x_before_arm_writes_the_items_y_as_well_even_when_y_was_already_visible() {
        let (mut ui, mut w) = h_list();
        dereth_ui::widgets::listbox::set_scroll_offset(&mut ui, w.handle, 100, 20);
        assert_eq!(w.scroll(&ui), (100, 20));

        // Item 6 -> cell (1, 1) -> x = 40, y = 30.
        let (col, row) = w.cell_of(6).expect("item 6 has a cell");
        assert_eq!((col, row), (1, 1));

        // The station's premise, computed from the fixture rather than restated: y is inside the
        // viewport on its own terms, so **neither** y arm can be the one that fires and the y
        // that comes out can only have been written by the x arm. Clippy rightly refuses a
        // constant assertion here, and it is the same objection this unit's other half is about.
        let (sx, sy) = w.scroll(&ui);
        let (x, y): (i32, i32) = (
            w.item_widths[..col as usize].iter().sum(),
            w.item_heights[..row as usize].iter().sum(),
        );
        let (bw, bh) = {
            let b = ui.node(w.handle).expect("alive").region.box_;
            (b.width(), b.height())
        };
        let (iw, ih) = {
            let b = ui.node(w.items[6]).expect("alive").region.box_;
            (b.width(), b.height())
        };
        assert_eq!(
            (x, y, sx, sy, bw, bh, iw, ih),
            (40, 30, 100, 20, 100, 45, 40, 30)
        );
        assert!(y >= sy, "y >= sy, so the y-before arm is not taken");
        assert!(
            y <= sy + bh - ih,
            "y <= sy + h - ih, so the y-after arm is not taken either"
        );
        assert!(x < sx, "and the x-before arm is the one that fires");

        assert!(w.scroll_to_view(&mut ui, 6), "x = 40 < sx = 100");
        assert_eq!(
            w.scroll(&ui),
            (40, 30),
            "the x-before arm writes the item's own top-left; a per-axis one would leave y at 20"
        );
    }

    /// The **y** arms on the same asymmetric list, so both directions stay watched and the pair
    /// cannot be swapped without something reddening. `y - h + ih` uses the *height* numbers, and
    /// on this fixture they differ from the width numbers in every term.
    #[test]
    fn the_y_arms_still_use_the_row_numbers_on_a_horizontal_list() {
        let (mut ui, mut w) = h_list();
        // Item 10 -> cell (0, 2) -> x = 0, y = 60, which is past `sy + h - ih = 15`.
        assert_eq!(w.cell_of(10), Some((0, 2)));
        assert!(w.scroll_to_view(&mut ui, 10), "y > sy + h - ih");
        assert_eq!(
            w.scroll(&ui),
            (0, 45),
            "y - h + ih = 60 - 45 + 30; x untouched"
        );

        // And back to the top-left through the y-before arm, which also writes x.
        dereth_ui::widgets::listbox::set_scroll_offset(&mut ui, w.handle, 40, 45);
        assert!(w.scroll_to_view(&mut ui, 0), "y = 0 < sy = 45");
        assert_eq!(w.scroll(&ui), (0, 0), "both coordinates, again");

        // An item already wholly inside moves nothing at all -- the fifth arm, which is the one
        // that returns without setting the offset.
        assert!(
            !w.scroll_to_view(&mut ui, 0),
            "item 0 at the origin is already in view"
        );
        assert_eq!(w.scroll(&ui), (0, 0));
    }
}
