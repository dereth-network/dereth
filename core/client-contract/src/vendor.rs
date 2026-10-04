//! Stock membership and ordered quantity writes shared by vendor panels.

use crate::view::{ShopRow, ShopView};
use dereth_primitives::ObjectId;

#[derive(Debug, Default)]
pub struct StockProjection {
    pub rows: Vec<ShopRow>,
    pub sizes: Vec<(ObjectId, i32)>,
    pub first: ObjectId,
    pub basket_drops: u32,
    pub container_drops: u32,
}

/// The first category match precedes availability and containment checks. Quantity writes
/// precede containment too, so a hidden container can still receive its advertised stack size.
#[must_use]
pub fn stock(shop: &ShopView, mask: u32) -> StockProjection {
    let mut out = StockProjection::default();
    if !shop.open {
        return out;
    }
    for row in &shop.stock {
        if row.obj_type & mask == 0 {
            continue;
        }
        if out.first.0 == 0 {
            out.first = row.item;
        }
        let size = if row.amount == -1 {
            (row.max_stack_size > 1).then(|| i32::try_from(row.max_stack_size).unwrap_or(i32::MAX))
        } else {
            let mut basket = shop
                .buy_list
                .iter()
                .filter(|b| b.item == row.item)
                .peekable();
            let remaining = if basket.peek().is_some() {
                let n = row.amount - basket.map(|b| b.amount).sum::<i32>();
                if n <= 0 {
                    out.basket_drops += 1;
                    continue;
                }
                n
            } else {
                row.amount
            };
            (row.max_stack_size > 1)
                .then(|| remaining.min(i32::try_from(row.max_stack_size).unwrap_or(i32::MAX)))
        };
        if let Some(size) = size {
            out.sizes.push((row.item, size));
        }
        if row.contained_items != 0 || row.contained_containers != 0 {
            out.container_drops += 1;
            continue;
        }
        out.rows.push(row.clone());
    }
    out
}

impl ShopView {
    #[must_use]
    pub fn filter_index(&self) -> usize {
        self.filter.min(self.type_filters.len().saturating_sub(1))
    }
    #[must_use]
    pub fn filter_mask(&self) -> u32 {
        self.type_filters
            .get(self.filter_index())
            .map_or(0, |(_, mask)| *mask)
    }
}

/// The selected-row and whole-basket controls share the same membership decision.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BasketControls {
    pub item: bool,
    pub all: bool,
}

#[must_use]
pub fn basket_controls(shop: &ShopView, sell: bool, selected: Option<ObjectId>) -> BasketControls {
    let rows = if sell {
        &shop.sell_list
    } else {
        &shop.buy_list
    };
    BasketControls {
        item: shop.open && selected.is_some_and(|id| rows.iter().any(|r| r.item == id)),
        all: shop.open && !rows.is_empty(),
    }
}
