use super::*;
use dereth_client_contract::view::{TradeButtonState, TradeControls, TradeView};
mod vendor;
pub fn make(id: &str) -> Option<Box<dyn Panel>> {
    match id {
        "trade" => Some(Box::new(Trade::default())),
        "salvage" => Some(Box::new(Salvage::default())),
        "vendor" => Some(Box::new(vendor::Vendor::default())),
        _ => None,
    }
}
fn entry(c: &Context<'_>, id: ObjectId) -> ItemEntry {
    ItemEntry {
        id,
        icon: c.game.icon(id),
        decoration: c.game.slot_decoration(id),
        caption: c.game.name(id).unwrap_or("").into(),
        count: 1,
        amount: None,
        active_container: false,
        disabled: false,
    }
}
/// A strip of item slots. With `fill` it runs on to its end in empty slots, each showing the
/// empty slot's art and, on a strip that takes drops, lit by a drag; `filter` is the rule a
/// strip's drag hints follow.
#[allow(clippy::too_many_arguments)] // one field per argument of the strip
fn items(
    f: &mut PanelFrame,
    id: &str,
    r: Rect,
    mut entries: Vec<ItemEntry>,
    selected: Option<ObjectId>,
    offset: i32,
    fill: bool,
    filter: Option<crate::panels::DropFilter>,
) {
    horizontal_scroll(f, id, r, entries.len(), offset);
    let shown = usize::try_from((r.w + offset + 31) / 32).unwrap_or(0);
    if fill && entries.len() < shown {
        entries.resize_with(shown, ItemEntry::empty);
    }
    f.control(
        id,
        r,
        ControlKind::ItemStrip {
            entries,
            offset,
            slot_size: 32,
            selected,
        },
        true,
    )
    .drop_filter = filter;
}
#[derive(Debug, Default)]
struct Trade {
    displayed: std::cell::Cell<TradeControls>,
    last: std::cell::RefCell<Option<TradeView>>,
    selected: Option<ObjectId>,
    width: u32,
    offsets: [i32; 2],
}
impl Panel for Trade {
    fn resize(&mut self, width: u32, _: u32) {
        self.width = width;
    }
    fn id(&self) -> &'static str {
        "trade"
    }
    fn frame(&self, c: &Context<'_>) -> PanelFrame {
        let width = self.width.max(325);
        let w = width as i32;
        let half = w / 2;
        let mut f = PanelFrame::new(width, 110);
        let t = c.game.trade();
        if self.last.borrow().as_ref() != Some(&t) {
            self.displayed
                .set(t.controls(t.self_rows.len(), t.partner_rows.len(), false));
            *self.last.borrow_mut() = Some(t.clone());
        }
        let controls = self.displayed.get();
        for (did, r) in [
            ("0600129D", rect(0, 3, half - 3, 107)),
            ("0600129E", rect(half + 3, 3, half - 3, 110)),
            ("06001DBD", rect(half - 3, 0, 3, 110)),
            ("06001DBE", rect(0, 0, half - 3, 3)),
            ("06001DC1", rect(half, 3, 3, 110)),
            ("06001DC2", rect(half, 0, half, 3)),
        ] {
            f.image(did, r, true, false);
        }
        // The partner's name on the left, the player's own on the right.
        centered(&mut f, rect(5, 0, half - 40, 40), &t.partner_name, "16-7");
        let own = c.game.player().and_then(|p| c.game.name(p)).unwrap_or("");
        centered(&mut f, rect(half + 48, 0, half - 74, 40), own, "16-7");
        centered(
            &mut f,
            rect(5, 38, half - 56, 20),
            format!("Total Items: {}", t.partner_rows.len()),
            "14-6",
        );
        centered(
            &mut f,
            rect(half + 30, 38, half - 40, 20),
            format!("Total Items: {}", t.self_rows.len()),
            "14-6",
        );
        for (id, x, rows) in [
            ("partner", 2, &t.partner_rows),
            ("offer", half + 3, &t.self_rows),
        ] {
            items(
                &mut f,
                id,
                rect(x, 58, half - if x == 2 { 5 } else { 3 }, 32),
                rows.iter()
                    .map(|r| ItemEntry {
                        id: r.item,
                        icon: r.icon,
                        decoration: c.game.slot_decoration(r.item),
                        caption: r.name.clone(),
                        count: 1,
                        amount: None,
                        active_container: false,
                        disabled: t.acceptance_darkened,
                    })
                    .collect(),
                self.selected,
                self.offsets[usize::from(id == "offer" || id == "offer-scroll")],
                true,
                (id == "offer").then_some(crate::panels::DropFilter::Trade),
            );
        }
        image_button(
            &mut f,
            "close",
            rect(w - 22, 0, 22, 20),
            [0x060012AA, 0x060012A9, 0x060012AA],
            true,
        );
        // The negotiation, not the window, makes the buttons live: a cancelled trade leaves the
        // window up with nothing to accept or clear.
        let trading = t.open && t.partner.is_some();
        let b = f.button(
            "accept",
            rect(half, 0, 46, 30),
            " Trade",
            trading && controls.button != TradeButtonState::Disabled,
        );
        b.font = "14-6".into();
        b.images = Some(
            [
                if controls.button == TradeButtonState::Accepted {
                    "06001DC0"
                } else {
                    "06001DC3"
                },
                "06001DC0",
                "06001DBF",
            ]
            .map(String::from),
        );
        f.image(
            if controls.partner_accepted {
                "06001DBC"
            } else {
                "06001DBB"
            },
            rect(half - 35, 9, 35, 27),
            false,
            true,
        );
        let b = f.button(
            "clear",
            rect(half - 30, 41, 60, 14),
            "Clear All",
            trading && !t.self_rows.is_empty(),
        );
        b.images = Some(["06001DC6", "06001DC5", "06001DC6"].map(String::from));
        b.font = "14-6".into();
        f
    }
    fn event(&mut self, e: ControlEvent, c: &Context<'_>) -> Vec<PanelAction> {
        let t = c.game.trade();
        match e {
            ControlEvent::Scroll { id, value } => {
                self.offsets[usize::from(id == "offer" || id == "offer-scroll")] = value.max(0);
                vec![]
            }
            ControlEvent::Activate(id) => match id.as_str() {
                "close" => vec![PanelAction::Game(UiRequest::TradeClose), PanelAction::Close],
                "clear" if t.open && t.partner.is_some() => request(UiRequest::TradeReset),
                "accept" if t.open && t.partner.is_some() => {
                    let mut controls = self.displayed.get();
                    let action = controls
                        .button
                        .press(controls.displayed_self, controls.displayed_partner);
                    self.displayed.set(controls);
                    action.map(request).unwrap_or_default()
                }
                _ => vec![],
            },
            ControlEvent::DropStack {
                id,
                object,
                amount,
                max_amount,
                slot,
            } if id == "offer" && t.open && t.partner.is_some() => {
                if amount == 0 || amount > max_amount || !c.game.trade_drag_item_acceptable(object)
                {
                    return vec![];
                }
                request(if amount < max_amount {
                    UiRequest::TradeSplitItem {
                        item: object,
                        split: amount,
                        max: max_amount,
                    }
                } else {
                    UiRequest::TradeAddItem {
                        item: object,
                        position: slot,
                    }
                })
            }
            ControlEvent::Drop {
                id,
                payload: DragPayload::Object(item),
                slot,
            } if id == "offer" && t.open && t.partner.is_some() => {
                if !c.game.trade_drag_item_acceptable(item) {
                    return vec![];
                }
                let max = crate::panels::hud::stack_split(c).1;
                if max > 1 {
                    request(UiRequest::TradeSplitItem {
                        item,
                        split: crate::panels::hud::stack_split(c).0.max(1) as u32,
                        max: max as u32,
                    })
                } else {
                    request(UiRequest::TradeAddItem {
                        item,
                        position: slot,
                    })
                }
            }
            ControlEvent::Select { id, index } => {
                let rows = if id == "offer" {
                    &t.self_rows
                } else {
                    &t.partner_rows
                };
                self.selected = rows.get(index).map(|r| r.item);
                self.selected
                    .map(|id| request(UiRequest::Select(id)))
                    .unwrap_or_default()
            }
            _ => vec![],
        }
    }
}
#[derive(Debug, Default)]
struct Salvage {
    selected: Option<ObjectId>,
    width: u32,
    offset: i32,
}
impl Panel for Salvage {
    fn resize(&mut self, width: u32, _: u32) {
        self.width = width;
    }
    fn id(&self) -> &'static str {
        "salvage"
    }
    fn frame(&self, c: &Context<'_>) -> PanelFrame {
        let state = c.game.salvage_list();
        let width = self.width.max(325);
        let w = width as i32;
        let mut f = tiled(width, 100, "06001CBA");
        label(
            &mut f,
            rect(10, 10, w - 20, 20),
            "WARNING: Items in this panel will be destroyed!",
            "14-6",
        );
        items(
            &mut f,
            "items",
            rect(10, 40, w - 124, 32),
            state.items.iter().map(|i| entry(c, *i)).collect(),
            self.selected,
            self.offset,
            true,
            Some(crate::panels::DropFilter::Salvage {
                material: state.material,
            }),
        );
        let b = f.button(
            "salvage",
            rect(w - 104, 45, 94, 22),
            "Salvage",
            state.tool.is_some() && !state.items.is_empty(),
        );
        b.images = Some(["06002344", "06002345", "06002346"].map(String::from));
        image_button(
            &mut f,
            "close",
            rect(w - 25, 0, 25, 23),
            [0x060012AA, 0x060012A9, 0x060012AA],
            true,
        );
        f
    }
    fn event(&mut self, e: ControlEvent, c: &Context<'_>) -> Vec<PanelAction> {
        use dereth_client_contract::panels::salvage::SalvageAction;
        let state = c.game.salvage_list();
        match e {
            ControlEvent::Tick | ControlEvent::Salvage(_) => {
                if self.selected.is_some_and(|id| !state.items.contains(&id)) {
                    self.selected = None;
                }
                vec![]
            }
            ControlEvent::Scroll { id, value } if id == "items" || id == "items-scroll" => {
                self.offset = value.max(0);
                vec![]
            }
            ControlEvent::Drop {
                id,
                payload: DragPayload::Object(item),
                ..
            } if id == "items" => request(UiRequest::SalvageList(SalvageAction::Add(item))),
            ControlEvent::Select { id, index } if id == "items" => {
                self.selected = state.items.get(index).copied();
                self.selected
                    .map(|i| request(UiRequest::Select(i)))
                    .unwrap_or_default()
            }
            ControlEvent::DoubleClick { id, index } if id == "items" => state
                .items
                .get(index)
                .map(|i| request(UiRequest::SalvageList(SalvageAction::Remove(*i))))
                .unwrap_or_default(),
            ControlEvent::Activate(id) if id == "salvage" => {
                request(UiRequest::SalvageList(SalvageAction::Submit))
            }
            ControlEvent::Activate(id) if id == "close" => vec![
                PanelAction::Game(UiRequest::SalvageList(SalvageAction::Close)),
                PanelAction::Close,
            ],
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod trade_tests {
    use super::*;
    use dereth_client_contract::view::TradeRow;

    #[derive(Debug)]
    struct Game(std::cell::RefCell<TradeView>);

    impl GameView for Game {
        fn trade(&self) -> TradeView {
            self.0.borrow().clone()
        }
    }

    fn with(game: &Game, run: impl FnOnce(&Context<'_>)) {
        let (state, pregame, keyboard, settings) = Default::default();
        run(&Context {
            resources: &crate::resources::Resources::default(),
            layout: crate::panels::Layout::default(),
            now: dereth_primitives::LocalTime(0.0),
            game,
            pregame: &pregame,
            keyboard: &keyboard,
            settings: &settings,
            map_teleport_allowed: false,
            classic: &state,
        });
    }

    fn offer() -> Game {
        Game(std::cell::RefCell::new(TradeView {
            open: true,
            partner: Some(ObjectId(2)),
            partner_rows: vec![TradeRow {
                item: ObjectId(3),
                ..Default::default()
            }],
            ..Default::default()
        }))
    }

    /// Behaviour: trade.table.anything-either-side-changes-puts-both-agreements-out
    #[test]
    fn a_changed_offer_darkens_both_lights_and_the_next_press_accepts() {
        let game = offer();
        let mut panel = Trade::default();
        {
            let mut trade = game.0.borrow_mut();
            trade.accepted = true;
            trade.partner_accepted = true;
        }
        with(&game, |c| {
            panel.frame(c);
            assert_eq!(panel.displayed.get().button, TradeButtonState::Accepted);
            assert!(panel.displayed.get().partner_accepted);
            game.0.borrow_mut().acceptance_darkened = true;
            let frame = panel.frame(c);
            let button = frame.controls.iter().find(|c| c.id == "accept").unwrap();
            assert!(button.enabled);
            assert_eq!(button.images.as_ref().unwrap()[0], "06001DC3");
            assert!(!panel.displayed.get().partner_accepted);
            assert!(frame.screen.commands.iter().any(|command| {
                matches!(command, Command::Image { did, .. } if did == "06001DBB")
            }));
            assert!(!frame.screen.commands.iter().any(|command| {
                matches!(command, Command::Image { did, .. } if did == "06001DBC")
            }));
            assert_eq!(
                panel.event(ControlEvent::Activate("accept".into()), c),
                vec![PanelAction::Game(UiRequest::TradeAccept {
                    displayed_self: 0,
                    displayed_partner: 1
                })]
            );
        });
    }

    /// Behaviour: trade.controls.the-one-button-agrees-and-then-takes-it-back-without-ending-anything
    #[test]
    fn an_empty_offer_disables_accept_and_a_partner_row_enables_it() {
        let game = offer();
        game.0.borrow_mut().partner_rows.clear();
        let mut panel = Trade::default();
        with(&game, |c| {
            let frame = panel.frame(c);
            assert!(
                !frame
                    .controls
                    .iter()
                    .find(|c| c.id == "accept")
                    .unwrap()
                    .enabled
            );
            assert!(panel
                .event(ControlEvent::Activate("accept".into()), c)
                .is_empty());
            game.0.borrow_mut().partner_rows.push(TradeRow::default());
            let frame = panel.frame(c);
            assert!(
                frame
                    .controls
                    .iter()
                    .find(|c| c.id == "accept")
                    .unwrap()
                    .enabled
            );
        });
    }

    /// Behaviour: trade.controls.a-stale-window-accepts-an-empty-trade
    #[test]
    fn accept_uses_the_last_drawn_rows_when_the_model_changes_before_the_click() {
        let game = offer();
        let mut panel = Trade::default();
        with(&game, |c| {
            panel.frame(c);
            game.0.borrow_mut().partner_rows.push(TradeRow::default());
            assert_eq!(
                panel.event(ControlEvent::Activate("accept".into()), c),
                vec![PanelAction::Game(UiRequest::TradeAccept {
                    displayed_self: 0,
                    displayed_partner: 1
                })]
            );
        });
    }

    /// Behaviour: trade.controls.the-one-button-agrees-and-then-takes-it-back-without-ending-anything
    #[test]
    fn a_second_press_declines_even_before_the_server_confirms_the_first() {
        let game = offer();
        let mut panel = Trade::default();
        with(&game, |c| {
            panel.frame(c);
            panel.event(ControlEvent::Activate("accept".into()), c);
            panel.frame(c);
            assert_eq!(
                panel.event(ControlEvent::Activate("accept".into()), c),
                vec![PanelAction::Game(UiRequest::TradeDecline)]
            );
        });
    }
}

#[cfg(test)]
mod salvage_tests {
    use super::*;
    use dereth_client_contract::panels::salvage::{SalvageAction, SalvageListView};

    #[derive(Debug)]
    struct Game(SalvageListView);
    impl GameView for Game {
        fn salvage_list(&self) -> SalvageListView {
            self.0.clone()
        }
    }

    /// Behaviour: salvage.row.a-double-click-takes-a-row-out-and-a-single-click-does-not
    #[test]
    fn shared_rows_are_removed_only_by_double_click_and_submit_is_semantic() {
        let game = Game(SalvageListView {
            tool: Some(ObjectId(9)),
            items: vec![ObjectId(7), ObjectId(2)],
            material: 16,
            visible: true,
        });
        let (state, pregame, keyboard, settings) = Default::default();
        let c = Context {
            resources: &crate::resources::Resources::default(),
            layout: crate::panels::Layout::default(),
            now: dereth_primitives::LocalTime(0.0),
            game: &game,
            pregame: &pregame,
            keyboard: &keyboard,
            settings: &settings,
            map_teleport_allowed: false,
            classic: &state,
        };
        let mut panel = Salvage::default();
        assert_eq!(
            panel.event(
                ControlEvent::Select {
                    id: "items".into(),
                    index: 0
                },
                &c
            ),
            vec![PanelAction::Game(UiRequest::Select(ObjectId(7)))]
        );
        assert_eq!(
            panel.event(
                ControlEvent::DoubleClick {
                    id: "items".into(),
                    index: 0
                },
                &c
            ),
            vec![PanelAction::Game(UiRequest::SalvageList(
                SalvageAction::Remove(ObjectId(7))
            ))]
        );
        assert_eq!(
            panel.event(ControlEvent::Activate("salvage".into()), &c),
            vec![PanelAction::Game(UiRequest::SalvageList(
                SalvageAction::Submit
            ))]
        );
        assert_eq!(game.0.items, vec![ObjectId(7), ObjectId(2)]);
    }
}
