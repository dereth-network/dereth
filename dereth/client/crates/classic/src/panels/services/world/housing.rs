use super::*;

// Gregorian conversion operates on the per-instant local offset supplied by the
// contract. It never uses the local machine's current time zone.
fn date(t: i64, offset: i32) -> String {
    if t == 0 {
        return "N/A".into();
    }
    crate::panels::hud::classic_date(t, offset)
}
pub(super) fn house(c: &Context<'_>) -> PanelFrame {
    let mut f = tiled(300, crate::panels::side_height() - 25, "060022BA");
    if let Some(h) = c.game.house_data() {
        label(
            &mut f,
            rect(10, 10, 280, 60),
            format!("The purchase price for this dwelling is:\n{}", h.buy_text),
            "15-6",
        );
        label(
            &mut f,
            rect(10, 70, 280, 60),
            format!("Rent:\n{}", h.rent_text),
            "15-6",
        );
        centered(
            &mut f,
            rect(10, 130, 280, 20),
            format!(
                "Maintenance period ends: {}",
                date(h.maintenance_period_end, h.utc_offset_secs[1])
            ),
            "15-6",
        );
        centered(
            &mut f,
            rect(10, 150, 280, 20),
            format!(
                "Maintenance is next due: {}",
                date(h.maintenance_next_due, h.utc_offset_secs[2])
            ),
            "15-6",
        );
        centered(
            &mut f,
            rect(10, 170, 280, 20),
            format!("Bought: {}", date(h.buy_time, h.utc_offset_secs[0])),
            "15-6",
        );
        if let Some((x, y)) = h.location {
            centered(
                &mut f,
                rect(10, 190, 280, 20),
                format!(
                    "{}, {}",
                    coord((y - 1024) as f32 * 0.1 + 0.5, "N", "S"),
                    coord((x - 1024) as f32 * 0.1 + 0.5, "E", "W")
                ),
                "15-6",
            );
        }
        centered(
            &mut f,
            rect(10, 210, 280, 70),
            if h.rent_owed {
                h.rent_warning
            } else {
                "The maintenance has already been paid for this period. You may not prepay next period's maintenance.".into()
            },
            "15-6",
        );
        if let Some(Command::TextBox { color, .. }) = f.screen.commands.last_mut() {
            *color = 0xffffff00;
        }
    } else {
        label(
            &mut f,
            rect(10, 10, 280, 60),
            "You do not currently own a house.",
            "15-6",
        );
    }
    let purchase = c.game.house_purchase();
    if purchase.have_player_desc {
        let text = if !purchase.wait_expired {
            format!("You may buy another landscape house at {}. This restriction does not apply to apartments.",date(purchase.purchase_timestamp as i64+0x278d00,purchase.utc_offset_secs))
        } else if c.game.house_data().is_none() {
            "You may buy another house immediately.".into()
        } else {
            "You may buy another house immediately after you abandon this one.".into()
        };
        centered(&mut f, rect(10, 280, 280, 60), text, "15-6");
    }
    f
}
#[derive(Debug, Default)]
pub struct Maintenance {
    rent: bool,
    buy: Vec<ObjectId>,
    rent_items: Vec<ObjectId>,
    selected: Option<ObjectId>,
    profile_edge: u64,
    width: u32,
    offsets: [i32; 2],
    pending_split: Option<(bool, ObjectId)>,
    confirmation_pending: [bool; 2],
}
impl Maintenance {
    fn add_object(
        &mut self,
        item: ObjectId,
        c: &Context<'_>,
        seen: &mut Vec<ObjectId>,
        out: &mut Vec<PanelAction>,
    ) {
        if seen.contains(&item) {
            return;
        }
        seen.push(item);
        let children = c.game.container_contents(item);
        if !children.is_empty() {
            out.push(PanelAction::Host(HostAction::LocalFeedback {
                severity: crate::panels::FeedbackSeverity::Information,
                text: format!("Adding contents of {}", c.game.name(item).unwrap_or("")),
            }));
            for &child in children {
                self.add_object(child, c, seen, out);
            }
            return;
        }
        let drops = self.drops(c);
        let class = c.game.item_wcid(item);
        let note = c.game.item_trade_note_value(item);
        if !self.items().contains(&item)
            && c.game.item_owned_by_player(item)
            && c.game.slumlord_needs_more(self.rent, &drops, class, note)
            && c.game.slumlord_pay(
                self.rent,
                &drops,
                class,
                c.game.item_house_payment(item),
                note,
            )
        {
            self.items_mut().push(item);
        }
    }
    fn send_payment(&mut self, c: &Context<'_>) -> Vec<PanelAction> {
        let Some(h) = c.game.slumlord() else {
            return vec![];
        };
        if self.items().is_empty()
            || self.rent != (h.owner.0 != 0)
            || (!self.rent
                && (h.owner.0 != 0 || !c.game.slumlord_payment(false, &self.drops(c)).paid_in_full))
        {
            return vec![];
        }
        let items = std::mem::take(self.items_mut());
        self.selected = None;
        self.offsets[usize::from(self.rent)] = 0;
        request(UiRequest::HousePayment {
            slumlord: h.slumlord,
            rent: self.rent,
            items,
        })
    }

    fn items(&self) -> &Vec<ObjectId> {
        if self.rent {
            &self.rent_items
        } else {
            &self.buy
        }
    }
    fn items_mut(&mut self) -> &mut Vec<ObjectId> {
        if self.rent {
            &mut self.rent_items
        } else {
            &mut self.buy
        }
    }
    fn drops(&self, c: &Context<'_>) -> Vec<(u32, i32, Option<i32>)> {
        self.items()
            .iter()
            .map(|i| {
                (
                    c.game.item_wcid(*i),
                    c.game.item_house_payment(*i),
                    c.game.item_trade_note_value(*i),
                )
            })
            .collect()
    }
}
impl Panel for Maintenance {
    fn resize(&mut self, width: u32, _: u32) {
        self.width = width;
    }
    fn id(&self) -> &'static str {
        "maintenance"
    }
    fn frame(&self, c: &Context<'_>) -> PanelFrame {
        let width = self.width.max(325) as i32;
        let half = width / 2;
        let mut f = tiled(
            width as u32,
            110,
            if self.rent { "0600129E" } else { "0600129D" },
        );
        for (id, text, x, w, art) in [
            ("buy", "Buy", 0, 92, [0x060012A2, 0x060012A4, 0x060012A3]),
            (
                "rent",
                "Maintenance",
                92,
                107,
                [0x0600223E, 0x06002240, 0x0600223F],
            ),
        ] {
            let b = f.button(id, rect(x, 0, w, 20), text, true);
            b.images = Some(art.map(|n| format!("{n:08X}")));
            b.font = "14-6".into();
        }
        image_button(
            &mut f,
            "close",
            rect(width - 22, 0, 22, 20),
            [0x060012AA, 0x060012A9, 0x060012AA],
            true,
        );
        if let Some(h) = c.game.slumlord() {
            let payment = c.game.slumlord_payment(self.rent, &self.drops(c));
            label(
                &mut f,
                rect(half + 5, 30, half - 109, 20),
                format!(
                    "Owner: {}",
                    if h.owner_name.is_empty() {
                        "None"
                    } else {
                        &h.owner_name
                    }
                ),
                "14-6",
            );
            label(
                &mut f,
                rect(5, 30, half - 10, 80),
                payment.requirements,
                "14-6",
            );
            let enabled = (self.rent || payment.paid_in_full)
                && !self.items().is_empty()
                && (self.rent == (h.owner.0 != 0));
            let b = f.button(
                "pay",
                rect(width - 99, 30, 94, 22),
                if self.rent { "Maintenance" } else { "Buy" },
                enabled,
            );
            b.images = Some(
                if self.rent {
                    ["06002347", "06002348", "06002349"]
                } else {
                    ["06002344", "06002345", "06002346"]
                }
                .map(String::from),
            );
            b.font = "14-6".into();
            horizontal_scroll(
                &mut f,
                "items",
                rect(half + 3, 58, half - 3, 32),
                self.items().len(),
                self.offsets[usize::from(self.rent)],
            );
            f.control(
                "items",
                rect(half + 3, 58, half - 3, 32),
                ControlKind::ItemStrip {
                    entries: self
                        .items()
                        .iter()
                        .map(|i| ItemEntry {
                            id: *i,
                            icon: c.game.icon(*i),
                            decoration: c.game.slot_decoration(*i),
                            caption: c.game.name(*i).unwrap_or("").into(),
                            count: c.game.item_house_payment(*i).max(1) as u32,
                            amount: None,
                            active_container: false,
                            disabled: false,
                        })
                        .collect(),
                    offset: self.offsets[usize::from(self.rent)],
                    slot_size: 32,
                    selected: self.selected,
                },
                true,
            );
        }
        f
    }
    fn event(&mut self, e: ControlEvent, c: &Context<'_>) -> Vec<PanelAction> {
        match e {
            ControlEvent::HousePaymentConfirmation { rent, confirmed } => {
                self.confirmation_pending[usize::from(rent)] = false;
                match confirmed {
                    None => vec![],
                    Some(false) => vec![PanelAction::Close],
                    Some(true) => {
                        let allowed = if rent {
                            !self.rent_items.is_empty()
                        } else {
                            let drops = self
                                .buy
                                .iter()
                                .map(|i| {
                                    (
                                        c.game.item_wcid(*i),
                                        c.game.item_house_payment(*i),
                                        c.game.item_trade_note_value(*i),
                                    )
                                })
                                .collect::<Vec<_>>();
                            c.game.slumlord_payment(false, &drops).paid_in_full
                        };
                        if allowed {
                            self.send_payment(c)
                        } else {
                            vec![]
                        }
                    }
                }
            }
            ControlEvent::DropStack {
                id,
                object,
                amount,
                max_amount,
                ..
            } if id == "items" => {
                if self.items().contains(&object) && amount < max_amount {
                    return vec![PanelAction::Host(HostAction::LocalFeedback {
                        severity: crate::panels::FeedbackSeverity::Warning,
                        text: "You cannot split items from this panel".into(),
                    })];
                }
                let Some(h) = c.game.slumlord() else {
                    return vec![];
                };
                if amount == 0
                    || amount > max_amount
                    || !c.game.item_owned_by_player(object)
                    || self.rent != (h.owner.0 != 0)
                    || self.pending_split.is_some()
                {
                    return vec![];
                }
                if amount == max_amount {
                    let mut out = vec![];
                    self.add_object(object, c, &mut vec![], &mut out);
                    return out;
                }
                if !c.game.slumlord_needs_more(
                    self.rent,
                    &self.drops(c),
                    c.game.item_wcid(object),
                    c.game.item_trade_note_value(object),
                ) {
                    return vec![];
                }
                self.pending_split = Some((self.rent, h.slumlord));
                vec![PanelAction::Host(HostAction::SplitForPanel {
                    object,
                    amount,
                })]
            }
            ControlEvent::SplitFailed => {
                self.pending_split = None;
                vec![]
            }
            ControlEvent::SplitReady(item) => {
                let Some((rent, slumlord)) = self.pending_split.take() else {
                    return vec![];
                };
                if c.game.slumlord().is_none_or(|h| h.slumlord != slumlord) {
                    return vec![];
                }
                let current = self.rent;
                self.rent = rent;
                let mut out = vec![];
                self.add_object(item, c, &mut vec![], &mut out);
                self.rent = current;
                out
            }
            ControlEvent::Scroll { id, value } if (id == "items" || id == "items-scroll") => {
                self.offsets[usize::from(self.rent)] = value.max(0);
                vec![]
            }
            ControlEvent::Tick => {
                let edge = c.game.slumlord_notices();
                if edge != self.profile_edge {
                    self.profile_edge = edge;
                    self.buy.clear();
                    self.rent_items.clear();
                    self.rent = c.game.slumlord().is_some_and(|h| h.owner.0 != 0);
                    self.selected = None;
                    self.offsets = [0; 2];
                    self.pending_split = None;
                }
                vec![]
            }
            ControlEvent::Activate(id) => match id.as_str() {
                "buy" => {
                    self.rent = false;
                    vec![]
                }
                "rent" => {
                    self.rent = true;
                    vec![]
                }
                "close" => vec![
                    PanelAction::Game(UiRequest::UnregisterSlumlordRange),
                    PanelAction::Close,
                ],
                "pay" => {
                    let Some(h) = c.game.slumlord() else {
                        return vec![];
                    };
                    if (self.rent != (h.owner.0 != 0))
                        || self.items().is_empty()
                        || (!self.rent
                            && !c
                                .game
                                .slumlord_payment(self.rent, &self.drops(c))
                                .paid_in_full)
                    {
                        return vec![];
                    }
                    let needs_confirmation = if self.rent {
                        !h.am_i_the_owner
                    } else {
                        h.house_type != 4
                    };
                    if needs_confirmation {
                        let pending = &mut self.confirmation_pending[usize::from(self.rent)];
                        if std::mem::replace(pending, true) {
                            vec![]
                        } else {
                            request(UiRequest::HousePaymentConfirmation { rent: self.rent })
                        }
                    } else {
                        self.send_payment(c)
                    }
                }
                _ => vec![],
            },
            ControlEvent::Drop {
                id,
                payload: DragPayload::Object(item),
                ..
            } if id == "items" => {
                let Some(h) = c.game.slumlord() else {
                    return vec![];
                };
                if self.rent != (h.owner.0 != 0) {
                    return vec![];
                }
                let mut out = vec![];
                self.add_object(item, c, &mut vec![], &mut out);
                out
            }
            ControlEvent::Select { id, index } if id == "items" => {
                self.selected = self.items().get(index).copied();
                self.selected
                    .map(|i| request(UiRequest::Select(i)))
                    .unwrap_or_default()
            }
            ControlEvent::DoubleClick { id, index } if id == "items" => {
                if index < self.items().len() {
                    self.items_mut().remove(index);
                }
                vec![]
            }
            _ => vec![],
        }
    }
}
#[cfg(test)]
mod tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;
    use dereth_client_contract::view::{SlumlordPayment, SlumlordView};

    #[derive(Debug)]
    struct House {
        profile: SlumlordView,
        paid: std::cell::Cell<bool>,
    }

    impl GameView for House {
        fn slumlord(&self) -> Option<SlumlordView> {
            Some(self.profile.clone())
        }
        fn item_owned_by_player(&self, _: ObjectId) -> bool {
            true
        }
        fn slumlord_payment(&self, _: bool, drops: &[(u32, i32, Option<i32>)]) -> SlumlordPayment {
            SlumlordPayment {
                paid_in_full: self.paid.get() && !drops.is_empty(),
                ..Default::default()
            }
        }
        fn slumlord_needs_more(
            &self,
            _: bool,
            _: &[(u32, i32, Option<i32>)],
            _: u32,
            _: Option<i32>,
        ) -> bool {
            true
        }
        fn slumlord_pay(
            &self,
            _: bool,
            _: &[(u32, i32, Option<i32>)],
            _: u32,
            _: i32,
            _: Option<i32>,
        ) -> bool {
            true
        }
    }

    fn with_house(
        rent: bool,
        owner: bool,
        house_type: u32,
        run: impl FnOnce(&Context<'_>, &House),
    ) {
        let game = House {
            profile: SlumlordView {
                slumlord: ObjectId(9),
                owner: ObjectId(u32::from(rent)),
                am_i_the_owner: owner,
                house_type,
                ..Default::default()
            },
            paid: std::cell::Cell::new(true),
        };
        let context = Context {
            game: &game,
            pregame: &Default::default(),
            keyboard: &Default::default(),
            settings: &Default::default(),
            map_teleport_allowed: false,
            classic: &Default::default(),
        };
        run(&context, &game);
    }

    fn drop_item(panel: &mut Maintenance, c: &Context<'_>, item: u32) {
        assert!(panel
            .event(
                ControlEvent::Drop {
                    id: "items".into(),
                    payload: DragPayload::Object(ObjectId(item)),
                    slot: 0,
                },
                c
            )
            .is_empty());
    }

    /// Behaviour: panels.house-purchase.paying-sends-buy-house-with-the-windows-items
    #[test]
    fn payments_keep_drop_order_for_purchase_and_maintenance() {
        for rent in [false, true] {
            with_house(rent, rent, 4, |c, _| {
                let mut panel = Maintenance {
                    rent,
                    ..Default::default()
                };
                for id in [7, 2, 6, 2] {
                    drop_item(&mut panel, c, id);
                }
                assert_eq!(
                    panel.event(ControlEvent::Activate("pay".into()), c),
                    request(UiRequest::HousePayment {
                        slumlord: ObjectId(9),
                        rent,
                        items: vec![ObjectId(7), ObjectId(2), ObjectId(6)],
                    })
                );
                assert!(panel.items().is_empty());
            });
        }
    }

    /// Behaviour: panels.house-purchase.confirmations-use-current-payment-and-handle-every-answer
    #[test]
    fn housing_questions_use_shared_confirmation_and_handle_every_answer() {
        for rent in [false, true] {
            with_house(rent, false, 1, |c, _| {
                let mut panel = Maintenance {
                    rent,
                    ..Default::default()
                };
                drop_item(&mut panel, c, 7);
                for answer in [None, Some(false), Some(true)] {
                    assert_eq!(
                        panel.event(ControlEvent::Activate("pay".into()), c),
                        request(UiRequest::HousePaymentConfirmation { rent })
                    );
                    assert!(panel
                        .event(ControlEvent::Activate("pay".into()), c)
                        .is_empty());
                    let result = panel.event(
                        ControlEvent::HousePaymentConfirmation {
                            rent,
                            confirmed: answer,
                        },
                        c,
                    );
                    match answer {
                        None => assert!(result.is_empty()),
                        Some(false) => assert_eq!(result, vec![PanelAction::Close]),
                        Some(true) => assert_eq!(
                            result,
                            request(UiRequest::HousePayment {
                                slumlord: ObjectId(9),
                                rent,
                                items: vec![ObjectId(7)],
                            })
                        ),
                    }
                    assert!(!panel.confirmation_pending[usize::from(rent)]);
                }
            });
        }
    }

    /// Behaviour: panels.house-purchase.confirmations-use-current-payment-and-handle-every-answer
    #[test]
    fn confirmation_rechecks_payment_after_the_question_was_opened() {
        for rent in [false, true] {
            with_house(rent, false, 1, |c, game| {
                let mut panel = Maintenance {
                    rent,
                    ..Default::default()
                };
                drop_item(&mut panel, c, 7);
                assert_eq!(
                    panel.event(ControlEvent::Activate("pay".into()), c),
                    request(UiRequest::HousePaymentConfirmation { rent })
                );
                if rent {
                    panel.rent_items.clear();
                } else {
                    game.paid.set(false);
                }
                assert!(panel
                    .event(
                        ControlEvent::HousePaymentConfirmation {
                            rent,
                            confirmed: Some(true)
                        },
                        c
                    )
                    .is_empty());
                assert!(!panel.confirmation_pending[usize::from(rent)]);
            });
        }
    }
    #[test]
    fn house_dates_use_the_instant_offset() {
        assert_eq!(date(0, 0), "N/A");
        assert_eq!(date(1, 0), "01/01/70 00:00:01");
        assert_eq!(date(1, -3600), "12/31/69 23:00:01");
    }
}
