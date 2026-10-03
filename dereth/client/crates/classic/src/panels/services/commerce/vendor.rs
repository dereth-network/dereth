use super::*;
use dereth_client_contract::view::ShopRow;

/// The category list's choice, kept from one shop to the next (the list itself is rebuilt from
/// each shop's stock, so the same position may name another category).
static LAST_FILTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// A strip's entries padded with empty slots to fill its visible width; the selling list keeps
/// one more empty slot after its items once they fill it, for the next item dropped on it.
fn padded(mut entries: Vec<ItemEntry>, strip_width: i32, selling: bool) -> Vec<ItemEntry> {
    let visible = usize::try_from((strip_width / 32).max(0)).unwrap_or(0);
    if selling && entries.len() >= visible {
        entries.push(ItemEntry::empty());
    }
    while entries.len() < visible {
        entries.push(ItemEntry::empty());
    }
    entries
}

#[derive(Debug, Default)]
pub struct Vendor {
    tab: usize,
    selected: Option<ObjectId>,
    filter: Option<u32>,
    opened: Option<ObjectId>,
    offset: i32,
    width: u32,
}
impl Vendor {
    fn stock(&self, c: &Context<'_>) -> Vec<ShopRow> {
        c.game
            .shop()
            .stock
            .into_iter()
            .filter(|r| {
                self.filter
                    .is_none_or(|mask| mask == 0 || r.obj_type & mask != 0)
            })
            .collect()
    }
    fn rows(&self, c: &Context<'_>) -> Vec<ShopRow> {
        let s = c.game.shop();
        match self.tab {
            0 => self.stock(c),
            1 => s.buy_list,
            _ => s.sell_list,
        }
    }
}
impl Panel for Vendor {
    fn resize(&mut self, width: u32, _: u32) {
        self.width = width;
    }
    fn id(&self) -> &'static str {
        "vendor"
    }
    fn frame(&self, c: &Context<'_>) -> PanelFrame {
        let s = c.game.shop();
        let rows = self.rows(c);
        let width = self.width.max(325) as i32;
        let strip_width = (width - 62) / 32 * 32;
        let strip_x = (width - strip_width - if self.tab == 0 { 62 } else { 54 }) / 2;
        let mut f = tiled(
            width as u32,
            110,
            match self.tab {
                0 => "0600129C",
                1 => "0600129D",
                _ => "0600129E",
            },
        );
        for (i, text, art) in [
            (0, "Items", [0x0600129F, 0x060012A1, 0x060012A0]),
            (1, "Buying", [0x060012A2, 0x060012A4, 0x060012A3]),
            (2, "Selling", [0x060012A5, 0x060012A7, 0x060012A6]),
        ] {
            let b = f.button(format!("tab{i}"), rect(i * 92, 0, 92, 20), text, true);
            b.images = Some(art.map(|n| format!("{n:08X}")));
            b.font = "14-6".into();
        }
        f.image("060012A8", rect(276, 0, width - 298, 20), true, false);
        image_button(
            &mut f,
            "close",
            rect(width - 22, 0, 22, 20),
            [0x060012AA, 0x060012A9, 0x060012AA],
            true,
        );
        items(
            &mut f,
            "items",
            rect(strip_x, 52, strip_width, 32),
            padded(
                rows.iter()
                    .map(|r| ItemEntry {
                        id: r.item,
                        icon: r.icon,
                        decoration: c.game.slot_decoration(r.item),
                        caption: r.name.clone(),
                        count: r.amount.max(1) as u32,
                        amount: (r.amount >= 0).then_some(r.amount as u32),
                        active_container: false,
                        disabled: !s.open,
                    })
                    .collect(),
                strip_width,
                self.tab == 2,
            ),
            self.selected,
            self.offset,
            false,
            None,
        );
        if self.tab == 0 {
            f.control(
                "filter",
                rect(5, 24, 117, 18),
                ControlKind::Choice {
                    options: s
                        .type_filters
                        .iter()
                        .map(|(name, _)| (*name).into())
                        .collect(),
                    selected: s
                        .type_filters
                        .iter()
                        .position(|(_, m)| Some(*m) == self.filter)
                        .unwrap_or(0),
                },
                true,
            )
            .list_skin = Some(crate::panels::ListSkin::VENDOR);
            if let Some(r) = rows.iter().find(|r| Some(r.item) == self.selected) {
                centered(&mut f, rect(125, 20, width - 183, 16), &r.name, "14-6");
                centered(
                    &mut f,
                    rect(125, 36, width - 183, 16),
                    format!("{}p", r.price),
                    "14-6",
                );
            }
            for (id, txt, y, art) in [
                ("buy", "Buy Item", 25, [0x060012AD, 0x060012AB, 0x060012AC]),
                (
                    "add",
                    "Add to List",
                    50,
                    [0x060012E5, 0x060012E6, 0x060012E7],
                ),
            ] {
                let b = f.button(
                    id,
                    rect(width - 58, y, 54, if id == "add" { 32 } else { 22 }),
                    txt,
                    s.open && self.selected.is_some(),
                );
                b.images = Some(art.map(|n| format!("{n:08X}")));
                b.font = if id == "add" { "14-5" } else { "15-5" }.into();
            }
        } else {
            let sell = self.tab == 2;
            let (count, total) = if sell {
                (s.sell_items, s.sell_transaction)
            } else {
                (s.buy_items, s.buy_transaction)
            };
            label(
                &mut f,
                rect(75, 20, width - 155, 16),
                format!(
                    "{} {} {} worth {}p",
                    if sell { "Selling" } else { "Buying" },
                    count,
                    if count == 1 { "item" } else { "items" },
                    total
                ),
                "14-6",
            );
            label(
                &mut f,
                rect(75, 36, width - 155, 16),
                format!("You have {}p", s.total_value),
                "14-6",
            );
            for (id, txt, y, art) in [
                (
                    "clear-item",
                    "Clear Item",
                    22,
                    [0x060012B7, 0x060012B6, 0x060012B5],
                ),
                (
                    "clear-list",
                    "Clear List",
                    36,
                    [0x060012BA, 0x060012B9, 0x060012B8],
                ),
            ] {
                let b = f.button(id, rect(5, y, 65, 14), txt, s.open && !rows.is_empty());
                b.images = Some(art.map(|n| format!("{n:08X}")));
                b.font = "14-5".into();
            }
            for (id, y) in [("one", 25), ("all", 60)] {
                let b = f.button(
                    id,
                    rect(width - 58, y, 54, 22),
                    format!(
                        "{} {}",
                        if sell { "Sell" } else { "Buy" },
                        if id == "one" { "Item" } else { "All" }
                    ),
                    s.open && !rows.is_empty(),
                );
                b.images = Some(
                    if sell {
                        ["060012E8", "060012E9", "060012AC"]
                    } else {
                        ["06001925", "06001926", "060012AC"]
                    }
                    .map(String::from),
                );
                b.font = "15-5".into();
            }
        }
        f
    }
    fn event(&mut self, e: ControlEvent, c: &Context<'_>) -> Vec<PanelAction> {
        let s = c.game.shop();
        match e {
            ControlEvent::Scroll { id, value } if (id == "items" || id == "items-scroll") => {
                self.offset = value.max(0);
                vec![]
            }
            ControlEvent::Tick => {
                if self.opened != s.vendor {
                    self.opened = s.vendor;
                    self.tab = if s.sell_mode { 2 } else { 0 };
                    self.selected = None;
                    // The category list opens on the choice made last time (the first one at
                    // first), and the stock is shown filtered by it from the start.
                    let last = LAST_FILTER.load(std::sync::atomic::Ordering::Relaxed);
                    let index = last.min(s.type_filters.len().saturating_sub(1));
                    self.filter = s.type_filters.get(index).map(|(_, mask)| *mask);
                }
                vec![]
            }
            ControlEvent::Select { id, index } if id == "filter" => {
                LAST_FILTER.store(index, std::sync::atomic::Ordering::Relaxed);
                self.filter = s.type_filters.get(index).map(|(_, mask)| *mask);
                self.offset = 0;
                self.selected = self.stock(c).first().map(|r| r.item);
                self.selected
                    .map(|i| request(UiRequest::Select(i)))
                    .unwrap_or_default()
            }
            ControlEvent::Select { id, index } if id == "items" => {
                self.selected = self.rows(c).get(index).map(|r| r.item);
                self.selected
                    .map(|i| request(UiRequest::Select(i)))
                    .unwrap_or_default()
            }
            ControlEvent::DoubleClick { id, index } if id == "items" && self.tab == 0 => self
                .rows(c)
                .get(index)
                .map(|r| {
                    request(UiRequest::VendorBuySingle {
                        item: r.item,
                        split: crate::panels::hud::stack_split(c).0,
                    })
                })
                .unwrap_or_default(),
            ControlEvent::DoubleClick { id, index } if id == "items" && self.tab != 0 => self
                .rows(c)
                .get(index)
                .map(|r| {
                    vec![
                        PanelAction::Host(HostAction::LocalFeedback {
                            severity: crate::panels::FeedbackSeverity::Information,
                            text: format!("Removing {} from shopping list", r.name),
                        }),
                        PanelAction::Game(UiRequest::VendorClearList {
                            sell: self.tab == 2,
                            item: Some(r.item),
                        }),
                    ]
                })
                .unwrap_or_default(),
            ControlEvent::Drop {
                id,
                payload: DragPayload::Object(item),
                ..
            } if id == "items" && self.tab == 2 && c.game.vendor_drag_item_accepted(item) => {
                request(UiRequest::VendorAddToSell { item })
            }
            ControlEvent::Activate(id) => match id.as_str() {
                "tab0" | "tab1" | "tab2" => {
                    self.tab = id.as_bytes()[3] as usize - b'0' as usize;
                    self.selected = None;
                    self.offset = 0;
                    vec![]
                }
                "close" => vec![
                    PanelAction::Game(UiRequest::VendorClose),
                    PanelAction::Close,
                ],
                "buy" | "one" => self
                    .selected
                    .map(|item| {
                        request(if self.tab == 2 {
                            UiRequest::VendorSellSingle { item }
                        } else {
                            UiRequest::VendorBuySingle {
                                item,
                                split: crate::panels::hud::stack_split(c).0,
                            }
                        })
                    })
                    .unwrap_or_default(),
                "add" => self
                    .selected
                    .map(|item| {
                        request(UiRequest::VendorAddToBuyList {
                            item,
                            split: crate::panels::hud::stack_split(c).0,
                        })
                    })
                    .unwrap_or_default(),
                "all" => {
                    if self.tab == 2 {
                        vec![PanelAction::Host(HostAction::VendorSellAll)]
                    } else {
                        request(UiRequest::VendorBuyAll)
                    }
                }
                "clear-item" => self
                    .selected
                    .map(|item| {
                        request(UiRequest::VendorClearList {
                            sell: self.tab == 2,
                            item: Some(item),
                        })
                    })
                    .unwrap_or_default(),
                "clear-list" => request(UiRequest::VendorClearList {
                    sell: self.tab == 2,
                    item: None,
                }),
                _ => vec![],
            },
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod padding_tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;
    fn item(n: u32) -> ItemEntry {
        ItemEntry {
            id: ObjectId(n),
            ..ItemEntry::empty()
        }
    }
    #[test]
    fn the_strips_fill_their_width_with_empty_slots_and_selling_keeps_one_more() {
        let two = padded(vec![item(1), item(2)], 9 * 32, false);
        assert_eq!(two.len(), 9);
        assert!(two[2..].iter().all(|e| e.id.0 == 0));
        let full: Vec<_> = (1..=9).map(item).collect();
        assert_eq!(padded(full.clone(), 9 * 32, false).len(), 9);
        let selling = padded(full, 9 * 32, true);
        assert_eq!(selling.len(), 10);
        assert_eq!(selling[9].id.0, 0);
    }
}
