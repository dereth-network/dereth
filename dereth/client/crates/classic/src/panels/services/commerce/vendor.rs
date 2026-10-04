use super::*;
use dereth_client_contract::view::ShopRow;

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
    last_shop: Option<dereth_client_contract::view::ShopView>,
    opened: Option<ObjectId>,
    offset: i32,
    width: u32,
}
impl Vendor {
    fn stock(&self, c: &Context<'_>) -> Vec<ShopRow> {
        let shop = c.game.shop();
        dereth_client_contract::vendor::stock(&shop, shop.filter_mask()).rows
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
            c.game.selected_object(),
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
                    selected: s.filter_index(),
                },
                true,
            )
            .list_skin = Some(crate::panels::ListSkin::VENDOR);
            if let Some(r) = rows
                .iter()
                .find(|r| Some(r.item) == c.game.selected_object())
            {
                let split = u32::try_from(c.game.split_size()).unwrap_or(1).max(1);
                centered(
                    &mut f,
                    rect(125, 20, width - 183, 16),
                    dereth_presentation::vendor::item_name_line(
                        &r.name,
                        c.game.plural_name(r.item),
                        split,
                    ),
                    "14-6",
                );
                centered(
                    &mut f,
                    rect(125, 36, width - 183, 16),
                    dereth_presentation::vendor::item_cost_line(r.price, s.total_value, split),
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
                    s.open
                        && rows
                            .iter()
                            .any(|r| Some(r.item) == c.game.selected_object()),
                );
                b.images = Some(art.map(|n| format!("{n:08X}")));
                b.font = if id == "add" { "14-5" } else { "15-5" }.into();
            }
        } else {
            let sell = self.tab == 2;
            let controls =
                dereth_client_contract::vendor::basket_controls(&s, sell, c.game.selected_object());
            let (count, total) = if sell {
                (s.sell_items, s.sell_transaction)
            } else {
                (s.buy_items, s.buy_transaction)
            };
            label(
                &mut f,
                rect(75, 20, width - 155, 16),
                dereth_presentation::vendor::transaction_line(
                    if sell { "Selling" } else { "Buying" },
                    count,
                    total,
                ),
                "14-6",
            );
            label(
                &mut f,
                rect(75, 36, width - 155, 16),
                dereth_presentation::vendor::purse_line(s.total_value),
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
                let b = f.button(
                    id,
                    rect(5, y, 65, 14),
                    txt,
                    if id == "clear-item" {
                        controls.item
                    } else {
                        controls.all
                    },
                );
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
                    if id == "one" {
                        controls.item
                    } else {
                        controls.all
                    },
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
                }
                if self.last_shop.as_ref() == Some(&s) {
                    return vec![];
                }
                let projection = dereth_client_contract::vendor::stock(&s, s.filter_mask());
                self.last_shop = Some(s);
                projection
                    .sizes
                    .into_iter()
                    .map(|(item, size)| {
                        PanelAction::Game(UiRequest::VendorSetObjectStackSize { item, size })
                    })
                    .collect()
            }
            ControlEvent::Select { id, index } if id == "filter" => {
                let mut changed = s;
                changed.filter = index;
                self.offset = 0;
                let first =
                    dereth_client_contract::vendor::stock(&changed, changed.filter_mask()).first;
                vec![
                    PanelAction::Game(UiRequest::VendorFilter(index)),
                    PanelAction::Game(UiRequest::Select(first)),
                ]
            }
            ControlEvent::Select { id, index } if id == "items" => self
                .rows(c)
                .get(index)
                .map(|r| r.item)
                .map(|i| request(UiRequest::Select(i)))
                .unwrap_or_default(),
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
            ControlEvent::Action(id) if id == "vendor-drag-over" => {
                self.tab = 2;
                self.offset = 0;
                vec![]
            }
            ControlEvent::DropStack {
                id,
                object,
                amount,
                max_amount,
                ..
            } if id == "items" && c.game.vendor_drag_item_accepted(object) => {
                self.tab = 2;
                request(if amount < max_amount {
                    UiRequest::VendorSplitToSell {
                        item: object,
                        split: amount,
                        max: max_amount,
                    }
                } else {
                    UiRequest::VendorAddToSell { item: object }
                })
            }
            ControlEvent::Drop {
                id,
                payload: DragPayload::Object(item),
                ..
            } if id == "items" && c.game.vendor_drag_item_accepted(item) => {
                self.tab = 2;
                request(UiRequest::VendorAddToSell { item })
            }
            ControlEvent::Activate(id) => match id.as_str() {
                "tab0" | "tab1" | "tab2" => {
                    self.tab = id.as_bytes()[3] as usize - b'0' as usize;
                    self.offset = 0;
                    vec![]
                }
                "close" => vec![
                    PanelAction::Game(UiRequest::VendorClose),
                    PanelAction::Close,
                ],
                "buy" | "one" => c
                    .game
                    .selected_object()
                    .filter(|id| self.rows(c).iter().any(|r| r.item == *id))
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
                "add" => c
                    .game
                    .selected_object()
                    .filter(|id| self.rows(c).iter().any(|r| r.item == *id))
                    .map(|item| {
                        request(UiRequest::VendorAddToBuyList {
                            item,
                            split: crate::panels::hud::stack_split(c).0,
                        })
                    })
                    .unwrap_or_default(),
                "all"
                    if dereth_client_contract::vendor::basket_controls(
                        &s,
                        self.tab == 2,
                        c.game.selected_object(),
                    )
                    .all =>
                {
                    if self.tab == 2 {
                        vec![PanelAction::Host(HostAction::VendorSellAll)]
                    } else {
                        request(UiRequest::VendorBuyAll)
                    }
                }
                "clear-item" => c
                    .game
                    .selected_object()
                    .filter(|id| self.rows(c).iter().any(|r| r.item == *id))
                    .map(|item| {
                        request(UiRequest::VendorClearList {
                            sell: self.tab == 2,
                            item: Some(item),
                        })
                    })
                    .unwrap_or_default(),
                "clear-list"
                    if dereth_client_contract::vendor::basket_controls(
                        &s,
                        self.tab == 2,
                        c.game.selected_object(),
                    )
                    .all =>
                {
                    request(UiRequest::VendorClearList {
                        sell: self.tab == 2,
                        item: None,
                    })
                }
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

#[cfg(test)]
mod shared_stock_tests {
    use super::*;
    use dereth_client_contract::view::ShopView;

    #[derive(Debug)]
    struct Shop(ShopView);
    impl GameView for Shop {
        fn selected_object(&self) -> Option<ObjectId> {
            Some(ObjectId(1))
        }
        fn shop(&self) -> ShopView {
            self.0.clone()
        }
    }
    fn with<T>(game: &Shop, f: impl FnOnce(&Context<'_>) -> T) -> T {
        f(&Context {
            game,
            pregame: &PregameView::default(),
            keyboard: &KeyboardState::default(),
            settings: &ClassicSettings::default(),
            classic: &ClassicState::default(),
            map_teleport_allowed: false,
        })
    }

    /// Behaviour: vendor.stock.shared-projection-keeps-availability-and-order
    #[test]
    fn classic_stock_uses_shared_remaining_quantities_and_retains_the_filter() {
        let row = |id, kind, amount, contained| ShopRow {
            item: ObjectId(id),
            obj_type: kind,
            amount,
            max_stack_size: 100,
            contained_items: contained,
            ..Default::default()
        };
        let mut game = Shop(ShopView {
            open: true,
            vendor: Some(ObjectId(99)),
            type_filters: vec![("Food", 0x20), ("Tools", 0x4000)],
            stock: vec![
                row(1, 0x20, 5, 0),
                row(2, 0x20, 10, 0),
                row(3, 0x20, -1, 1),
                row(4, 0x4000, -1, 0),
            ],
            buy_list: vec![row(1, 0x20, 5, 0), row(2, 0x20, 3, 0)],
            ..Default::default()
        });
        let mut panel = Vendor::default();
        with(&game, |c| {
            assert_eq!(
                panel.rows(c).iter().map(|r| r.item).collect::<Vec<_>>(),
                [ObjectId(2)]
            );
            let actions = panel.event(ControlEvent::Tick, c);
            let requests: Vec<_> = actions
                .into_iter()
                .filter_map(|a| {
                    if let PanelAction::Game(r) = a {
                        Some(r)
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(
                requests,
                [
                    UiRequest::VendorSetObjectStackSize {
                        item: ObjectId(2),
                        size: 7
                    },
                    UiRequest::VendorSetObjectStackSize {
                        item: ObjectId(3),
                        size: 100
                    }
                ]
            );
            assert!(panel.event(ControlEvent::Tick, c).is_empty());
        });
        game.0.filter = 1;
        with(&game, |c| {
            assert_eq!(
                panel.rows(c).iter().map(|r| r.item).collect::<Vec<_>>(),
                [ObjectId(4)]
            );
            let rebuilt = Vendor::default();
            assert_eq!(rebuilt.rows(c), panel.rows(c));
        });
        game.0.filter = 0;
        with(&game, |c| {
            let projection = dereth_client_contract::vendor::stock(&game.0, 0x20);
            assert_eq!(projection.first, ObjectId(1));

            assert!(
                panel
                    .event(ControlEvent::Activate("add".into()), c)
                    .is_empty(),
                "an exhausted row cannot be added from stale selection"
            );
        });
    }
}

#[cfg(test)]
mod basket_tests {
    use super::*;
    use dereth_client_contract::view::ShopView;
    #[derive(Debug, Default)]
    struct View {
        shop: ShopView,
        selected: Option<ObjectId>,
    }
    impl GameView for View {
        fn shop(&self) -> ShopView {
            self.shop.clone()
        }
        fn selected_object(&self) -> Option<ObjectId> {
            self.selected
        }
    }
    fn with<T>(v: &View, f: impl FnOnce(&Context<'_>) -> T) -> T {
        f(&Context {
            game: v,
            pregame: &PregameView::default(),
            keyboard: &KeyboardState::default(),
            settings: &ClassicSettings::default(),
            classic: &ClassicState::default(),
            map_teleport_allowed: false,
        })
    }
    /// Behaviour: vendor.controls.selection-follows-basket-membership
    #[test]
    fn classic_basket_buttons_and_highlight_follow_the_shared_selection() {
        let mut view = View {
            shop: ShopView {
                open: true,
                vendor: Some(ObjectId(9)),
                buy_list: vec![ShopRow {
                    item: ObjectId(1),
                    ..Default::default()
                }],
                sell_list: vec![ShopRow {
                    item: ObjectId(2),
                    ..Default::default()
                }],
                ..Default::default()
            },
            selected: Some(ObjectId(1)),
        };
        let mut panel = Vendor::default();
        for (tab, id) in [(1, ObjectId(1)), (2, ObjectId(2))] {
            panel.tab = tab;
            for (selection, item_enabled, all_enabled) in [
                (Some(id), true, true),
                (Some(ObjectId(77)), false, true),
                (None, false, true),
            ] {
                view.selected = selection;
                with(&view, |c| {
                    let frame = panel.frame(c);
                    for key in ["one", "clear-item"] {
                        assert_eq!(
                            frame.controls.iter().find(|b| b.id == key).unwrap().enabled,
                            item_enabled
                        );
                    }
                    for key in ["all", "clear-list"] {
                        assert_eq!(
                            frame.controls.iter().find(|b| b.id == key).unwrap().enabled,
                            all_enabled
                        );
                    }
                    assert_eq!(
                        !panel
                            .event(ControlEvent::Activate("one".into()), c)
                            .is_empty(),
                        item_enabled
                    );
                    if item_enabled {
                        assert!(frame.controls.iter().any(|b| matches!(&b.kind,ControlKind::ItemStrip { selected:Some(i),.. } if *i==id)));
                    }
                });
            }
        }
        view.shop.buy_list.clear();
        view.shop.sell_list.clear();
        for tab in [1, 2] {
            panel.tab = tab;
            with(&view, |c| {
                let f = panel.frame(c);
                for key in ["one", "all", "clear-item", "clear-list"] {
                    assert!(!f.controls.iter().find(|b| b.id == key).unwrap().enabled);
                    assert!(panel
                        .event(ControlEvent::Activate(key.into()), c)
                        .is_empty());
                }
            });
        }
    }
}
