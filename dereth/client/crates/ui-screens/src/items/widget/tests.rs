use super::*;

/// A widget with `n` synthetic slots and a known cell, for the pure layout arithmetic.
fn widget(n: usize, cols: i32, horizontal: bool) -> ItemListWidget {
    ItemListWidget {
        element: ElementId(1),
        handle: ElemHandle::for_test(1),
        slot_id: ElementId(0x1000_033A),
        container_list: false,
        shortcut_list: false,
        vendor_list: false,
        salvage_list: false,
        allow_dragging: true,
        horizontal,
        at_least_one_empty: false,
        single_selection: false,
        open_item_id: None,
        max_columns: cols,
        fixed_list_size: i32::try_from(n).unwrap(),
        cell: (32, 32),
        slots: (0..n)
            .map(|i| ItemSlot::bare(ElemHandle::for_test(u32::try_from(i).unwrap() + 2)))
            .collect(),
        cached_slots: std::collections::VecDeque::new(),
        parent_container: None,
        created: 0,
        create_failures: 0,
        drag_icon_failures: 0,
    }
}

fn origins(ui: &UiSystem, w: &ItemListWidget) -> Vec<(i32, i32)> {
    w.slots
        .iter()
        .map(|s| {
            let b = ui.node(s.handle).map(|n| n.region.box_).unwrap_or_default();
            (b.x0, b.y0)
        })
        .collect()
}

/// Oracle: the layout pass. With the horizontal bit **clear** — the
/// default, and what the shipped inventory lists carry — the placement loop advances `y` by
/// the row's height until the last row and only then resets `y` and advances `x`.
/// That is **column-major**, which is the opposite of what "max_columns" reads like.
#[test]
fn a_vertical_list_fills_column_major_down_m_n_rows() {
    let mut ui = UiSystem::new((800, 600));
    // Six items, three columns: 3 columns, ceil(6/3) = 2 rows.
    let mut w = widget(0, 3, false);
    for _ in 0..6 {
        let h = ui.create_hollow(None);
        ui.resize_to(h, 32, 32);
        w.slots.push(ItemSlot::bare(h));
    }
    w.fixed_list_size = 6;
    w.update_layout(&mut ui);
    assert_eq!(
        origins(&ui, &w),
        vec![(0, 0), (0, 32), (32, 0), (32, 32), (64, 0), (64, 32)],
        "down each column of two, then across"
    );
}

/// Oracle: the same function's other arm — the horizontal bit set fills **row-major** across
/// the columns, which is the eighteen-slot shortcut bar's own arrangement.
#[test]
fn a_horizontal_list_fills_row_major_across_m_n_cols() {
    let mut ui = UiSystem::new((800, 600));
    let mut w = widget(0, 3, true);
    for _ in 0..5 {
        let h = ui.create_hollow(None);
        ui.resize_to(h, 32, 32);
        w.slots.push(ItemSlot::bare(h));
    }
    w.fixed_list_size = 5;
    w.update_layout(&mut ui);
    assert_eq!(
        origins(&ui, &w),
        vec![(0, 0), (32, 0), (64, 0), (0, 32), (32, 32)],
        "across three, then wrap"
    );
}

/// Oracle: the list box element's item-index-at-point read **against** the layout pass.
/// The two are inverses, and asserting them against each other
/// rather than against a table written here is what makes the claim checkable.
///
/// **Full rigour, deliberately** (the evidence conventions, "tier the evidence bar by
/// risk"): this arithmetic decides *which slot* a press and a drop land on. Off by one column
/// and an icon goes into the wrong pack, which is invisible on screen and corrupts a player's
/// inventory. So every cell of both fill orders is swept, at four points inside each cell, and
/// each answer is checked against the position `update_layout` actually gave that slot.
#[test]
fn every_point_in_every_cell_resolves_to_the_slot_update_layout_put_there() {
    for horizontal in [false, true] {
        for (n, cols) in [(6usize, 3i32), (5, 3), (7, 3), (12, 6), (1, 1), (4, -1)] {
            let mut ui = UiSystem::new((800, 600));
            let list = ui.create_hollow(None);
            let mut w = widget(0, cols, horizontal);
            w.handle = list;
            // The column and row counts exactly as the layout pass computes them, so the list is
            // sized to the grid **before** anything is placed in it — resizing a parent
            // afterwards reflows its children and would be measuring the wrong thing.
            let ni = i32::try_from(n).unwrap();
            let (cols_n, rows_n) = if cols < 0 {
                (ni, 1)
            } else {
                let c = cols.max(1).min(ni);
                (c, (ni + c - 1) / c)
            };
            ui.resize_to(list, cols_n * 32, rows_n * 32);
            for _ in 0..n {
                let h = ui.create_hollow(Some(list));
                ui.resize_to(h, 32, 32);
                w.slots.push(ItemSlot::bare(h));
            }
            w.fixed_list_size = ni;
            w.update_layout(&mut ui);
            let (mx, my) = (cols_n * 32, rows_n * 32);

            for (i, s) in w.slots.iter().enumerate() {
                let b = ui.node(s.handle).expect("alive").region.box_;
                // Strictly inside the cell. Retail's <= cumulative-band comparison
                // assigns exact top/left boundaries to the preceding band.
                for (dx, dy) in [(1, 1), (31, 1), (1, 31), (31, 31), (16, 16)] {
                    let (x, y) = (b.x0 + dx, b.y0 + dy);
                    assert_eq!(
                        w.item_index_at_point(&ui, x, y),
                        Some(i),
                        "n={n} cols={cols} horizontal={horizontal}: ({x},{y}) is in cell {i} \
                             at {:?}",
                        (b.x0, b.y0)
                    );
                }
            }
            // Outside the list's own rectangle is nothing at all — the index read's
            // four opening guards, which is what stops a drop just past the last row from
            // landing on the last slot.
            for (x, y) in [(-1, 0), (0, -1), (mx, 0), (0, my)] {
                assert_eq!(
                    w.item_index_at_point(&ui, x, y),
                    None,
                    "({x},{y}) is off the list"
                );
            }
        }
    }
}

/// Oracle: the layout pass's first branch — attribute `0x5F < 0` sets the column count to
/// the item count and the row count to `(items != 0)`, i.e. **one row of everything**,
/// whatever the box is. Six of the shipped lists are in that state.
#[test]
fn a_negative_max_columns_puts_everything_in_one_row() {
    let mut ui = UiSystem::new((800, 600));
    let mut w = widget(0, -1, false);
    for _ in 0..4 {
        let h = ui.create_hollow(None);
        ui.resize_to(h, 32, 32);
        w.slots.push(ItemSlot::bare(h));
    }
    w.fixed_list_size = -1;
    w.update_layout(&mut ui);
    assert_eq!(origins(&ui, &w), vec![(0, 0), (32, 0), (64, 0), (96, 0)]);
}
