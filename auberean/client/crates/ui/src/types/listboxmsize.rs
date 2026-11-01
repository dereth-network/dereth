//! List boxes whose rows keep their own sizes: each column as wide as its widest row and each row
//! as tall as its tallest, the view scrolling by those sizes.

use crate::system::{Handle, Ui, Value};
use crate::token::attr;
use crate::types::listbox::{bits, name, Scroll};

/// Which run of sizes holds `pos`: the first whose running total reaches it.
// D2-11582
fn index_at(pos: i32, sizes: &[i32]) -> Option<usize> {
    let mut total = 0;
    for (i, s) in sizes.iter().enumerate() {
        total += *s;
        if total >= pos {
            return Some(i);
        }
    }
    None
}

/// The total of the first `n` sizes.
// D2-11583
fn size_of(n: i32, sizes: &[i32]) -> i32 {
    sizes
        .iter()
        .take(usize::try_from(n).unwrap_or(0))
        .copied()
        .sum()
}

impl Ui {
    /// A new list of rows of their own sizes: a list box, with no column widths or row heights
    /// yet.
    // D2-11597, D2-11598, D2-11600
    pub(crate) fn msize_created(&mut self, h: Handle) {
        if let Some(n) = self.node_mut(h) {
            n.lists.list.widths.clear();
            n.lists.list.heights.clear();
        }
        self.listbox_created(h);
    }

    /// A view value kept on the sheet: 0 when the whole sheet shows or it is below 0; not past
    /// the sheet's last full view.
    // D2-11581
    pub(crate) fn msize_constrain_view_value(&self, h: Handle, horizontal: bool, v: i32) -> i32 {
        let Some(n) = self.node(h) else { return v };
        let l = &n.lists.list;
        let (paper, view) = if horizontal {
            (l.paper_w, n.rect.width())
        } else {
            (l.paper_h, n.rect.height())
        };
        if v < 0 || paper <= view {
            0
        } else if paper < v + view {
            paper - view
        } else {
            v
        }
    }

    /// Scroll so a row's cell shows: back to its column or row when the view is past it.
    // D2-11584
    pub(crate) fn msize_scroll_to_show(&mut self, h: Handle, index: usize, at_once: bool) {
        let i = i32::try_from(index).unwrap_or(-1);
        let n = i32::try_from(self.list_items(h).len()).unwrap_or(0);
        if i < 0 || i >= n {
            return;
        }
        let (Some(col), Some(row)) = (self.listbox_col(h, i), self.listbox_row(h, i)) else {
            return;
        };
        let Some(nd) = self.node(h) else { return };
        let (w, hh) = (nd.rect.width(), nd.rect.height());
        let l = &nd.lists.list;
        let top = size_of(row, &l.heights);
        let left = size_of(col, &l.widths);
        let (xv, yv, iw, ih) = (l.x_view, l.y_view, l.item_w, l.item_h);
        if left < xv {
            self.msize_scroll_to_col(h, col, at_once);
        } else if 0 < iw && self.listbox_left_col(h) + w / iw <= col {
            self.msize_scroll_to_col(h, col + 1 - w / iw, at_once);
        }
        if top < yv {
            self.msize_scroll_to_row(h, row, at_once);
        } else if 0 < ih && self.listbox_top_row(h) + hh / ih <= row {
            self.msize_scroll_to_row(h, row + 1 - hh / ih, at_once);
        }
    }

    /// Scroll to a row: the heights of the rows before it, at once or as a goal.
    // D2-11585
    pub(crate) fn msize_scroll_to_row(&mut self, h: Handle, row: i32, at_once: bool) {
        let y = self
            .node(h)
            .map_or(0, |n| size_of(row, &n.lists.list.heights));
        let a = if at_once {
            name::Y_VIEW
        } else {
            name::GOAL_Y_VIEW
        };
        self.set_attr(h, a, Value::Int(y));
    }

    /// Scroll to a column: the widths of the columns before it, at once or as a goal.
    // D2-11586
    pub(crate) fn msize_scroll_to_col(&mut self, h: Handle, col: i32, at_once: bool) {
        let x = self
            .node(h)
            .map_or(0, |n| size_of(col, &n.lists.list.widths));
        let a = if at_once {
            name::X_VIEW
        } else {
            name::GOAL_X_VIEW
        };
        self.set_attr(h, a, Value::Int(x));
    }

    /// A scroll by line or page, as a goal, by the sizes of the row or column at the view: a line
    /// is that row's height (or column's width); a page the view's length less it.
    // D2-11588, D2-11589, D2-11590, D2-11591, D2-11592, D2-11593, D2-11594, D2-11595
    pub(crate) fn msize_scroll(&mut self, h: Handle, s: Scroll) {
        let Some(n) = self.node(h) else { return };
        let l = &n.lists.list;
        let vertical = matches!(
            s,
            Scroll::VPageDec | Scroll::VPageInc | Scroll::VLineDec | Scroll::VLineInc
        );
        let (sizes, view, len, paper, goal) = if vertical {
            (
                l.heights.clone(),
                l.y_view,
                n.rect.height(),
                l.paper_h,
                name::GOAL_Y_VIEW,
            )
        } else {
            (
                l.widths.clone(),
                l.x_view,
                n.rect.width(),
                l.paper_w,
                name::GOAL_X_VIEW,
            )
        };
        if sizes.is_empty() {
            return;
        }
        let Some(i) = index_at(view, &sizes) else {
            return;
        };
        let size = sizes[i];
        let last = paper - len;
        let (delta, on, limit) = match s {
            Scroll::VPageDec | Scroll::HPageDec => (size - len, false, 0),
            Scroll::VPageInc | Scroll::HPageInc => (len - size, true, last),
            Scroll::VLineDec | Scroll::HLineDec => (-size, false, 0),
            Scroll::VLineInc | Scroll::HLineInc => (size, true, last),
        };
        self.listbox_new_goal(h, view, delta, on, limit, goal);
    }

    /// The row under the point `(x, y)` on the screen, by the column and row whose sizes hold it.
    // D2-11596
    pub(crate) fn msize_index_at(&self, h: Handle, x: i32, y: i32) -> Option<usize> {
        let r = self.screen_rect(h)?;
        if !r.contains(x, y) {
            return None;
        }
        let n = self.node(h)?;
        let l = &n.lists.list;
        let px = l.x_view + x - r.x0;
        let py = l.y_view + y - r.y0;
        if px >= l.paper_w || py >= l.paper_h {
            return None;
        }
        let col = i32::try_from(index_at(px, &l.widths).unwrap_or(0)).ok()?;
        let row = i32::try_from(index_at(py, &l.heights).unwrap_or(0)).ok()?;
        let i = if l.bits & bits::HORIZONTAL != 0 {
            l.cols * row + col
        } else {
            l.rows * col + row
        };
        let i = usize::try_from(i).ok()?;
        (i < self.list_items(h).len()).then_some(i)
    }

    /// Each column's width (its widest row) and each row's height (its tallest), and the sheet's
    /// size from them; `max_columns` columns and as many rows as the rows need.
    // D2-11599
    fn msize_calculate_sizes(&mut self, h: Handle) {
        let items = self.list_items(h);
        let horizontal = self.lb_test(h, bits::HORIZONTAL);
        let cols = self.int_or(h, attr::MAX_COLUMNS, 0).max(1);
        let count = i32::try_from(items.len()).unwrap_or(i32::MAX);
        let rows = ((count + cols - 1) / cols).max(1);
        let mut widths = vec![0; usize::try_from(cols).unwrap_or(1)];
        let mut heights = vec![0; usize::try_from(rows).unwrap_or(1)];
        let (mut col, mut row) = (0usize, 0usize);
        for it in &items {
            if let Some(n) = self.node(*it) {
                widths[col] = widths[col].max(n.rect.width());
                heights[row] = heights[row].max(n.rect.height());
            }
            if horizontal {
                if col == widths.len() - 1 {
                    col = 0;
                    row += 1;
                } else {
                    col += 1;
                }
            } else if row == heights.len() - 1 {
                row = 0;
                col += 1;
            } else {
                row += 1;
            }
        }
        if let Some(n) = self.node_mut(h) {
            let l = &mut n.lists.list;
            l.cols = cols;
            l.rows = rows;
            l.paper_h += heights.iter().sum::<i32>();
            l.paper_w += widths.iter().sum::<i32>();
            l.widths = widths;
            l.heights = heights;
        }
    }

    /// Lay the rows out, each column as wide as its widest row and each row as tall as its
    /// tallest, a vertical list filling down its columns and a horizontal one across its rows,
    /// each row at its place less the view.
    // D2-11601
    pub(crate) fn msize_update_layout(&mut self, h: Handle) {
        if !self.lb_test(h, bits::ALLOW_UPDATING) {
            return;
        }
        if let Some(n) = self.node_mut(h) {
            let l = &mut n.lists.list;
            l.cols = 1;
            l.rows = 1;
            l.paper_w = 0;
            l.paper_h = 0;
            l.widths.clear();
            l.heights.clear();
        }
        let items = self.list_items(h);
        if items.is_empty() {
            return;
        }
        self.listbox_adopt_items(h, &items);
        self.msize_calculate_sizes(h);
        let horizontal = self.lb_test(h, bits::HORIZONTAL);
        let Some(n) = self.node(h) else { return };
        let l = &n.lists.list;
        let (xv, yv) = (l.x_view, l.y_view);
        let (widths, heights) = (l.widths.clone(), l.heights.clone());
        let (cols, rows) = (widths.len(), heights.len());
        let (mut x, mut y) = (-xv, -yv);
        let (mut col, mut row) = (0usize, 0usize);
        for it in &items {
            self.place_at(*it, x, y);
            if horizontal {
                if col == cols - 1 {
                    x = -xv;
                    col = 0;
                    y += heights[row];
                    row += 1;
                } else {
                    x += widths[col];
                    col += 1;
                }
            } else if row == rows - 1 {
                y = -yv;
                x += widths[col];
                col += 1;
                row = 0;
            } else {
                y += heights[row];
                row += 1;
            }
        }
    }
}
