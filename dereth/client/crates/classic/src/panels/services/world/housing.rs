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
                    coord((y - 1024) as f32 * 0.1 + 0.5, 'N', 'S'),
                    coord((x - 1024) as f32 * 0.1 + 0.5, 'E', 'W')
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
    selected: Option<ObjectId>,
    width: u32,
    offsets: [i32; 2],
    confirmation_pending: [bool; 2],
}
impl Maintenance {
    fn send_payment(&mut self, c: &Context<'_>) -> Vec<PanelAction> {
        let state = c.game.payment_lists();
        let rows = if state.op.is_rent() {
            &state.rent
        } else {
            &state.buy
        };
        if rows.is_empty()
            || (state.op == dereth_client_contract::panels::slumlord::HouseOp::Buy
                && !state.buy_payment.paid_in_full)
        {
            return vec![];
        }
        request(UiRequest::PaymentList(
            dereth_client_contract::panels::slumlord::PaymentAction::Submit,
        ))
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
        let state = c.game.payment_lists();
        let rent = state.op.is_rent();
        let offered = if rent { &state.rent } else { &state.buy };
        let width = self.width.max(325) as i32;
        let half = width / 2;
        let mut f = tiled(
            width as u32,
            110,
            if rent { "0600129E" } else { "0600129D" },
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
            let payment = if rent {
                state.rent_payment.clone()
            } else {
                state.buy_payment.clone()
            };
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
            let enabled =
                (rent || payment.paid_in_full) && !offered.is_empty() && (rent == (h.owner.0 != 0));
            let b = f.button(
                "pay",
                rect(width - 99, 30, 94, 22),
                if rent { "Maintenance" } else { "Buy" },
                enabled,
            );
            b.images = Some(
                if rent {
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
                offered.len(),
                self.offsets[usize::from(rent)],
            );
            f.control(
                "items",
                rect(half + 3, 58, half - 3, 32),
                ControlKind::ItemStrip {
                    entries: offered
                        .iter()
                        .map(|i| ItemEntry {
                            id: i.id,
                            icon: c.game.icon(i.id),
                            decoration: c.game.slot_decoration(i.id),
                            caption: c.game.name(i.id).unwrap_or("").into(),
                            count: i.amount.max(1) as u32,
                            amount: None,
                            active_container: false,
                            disabled: false,
                        })
                        .collect(),
                    offset: self.offsets[usize::from(rent)],
                    slot_size: 32,
                    selected: self.selected,
                },
                true,
            );
        }
        f
    }
    fn event(&mut self, e: ControlEvent, c: &Context<'_>) -> Vec<PanelAction> {
        use dereth_client_contract::panels::slumlord::{HouseOp, PaymentAction};
        let state = c.game.payment_lists();
        let rent = state.op.is_rent();
        let offered = if rent { &state.rent } else { &state.buy };
        match e {
            ControlEvent::HousePaymentConfirmation {
                rent: question_rent,
                confirmed,
            } => {
                self.confirmation_pending[usize::from(question_rent)] = false;
                match confirmed {
                    None => vec![],
                    Some(false) => vec![
                        PanelAction::Game(UiRequest::PaymentList(PaymentAction::Close)),
                        PanelAction::Close,
                    ],
                    Some(true) => {
                        let allowed = if question_rent {
                            !state.rent.is_empty()
                        } else {
                            state.buy_payment.paid_in_full
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
                if amount == 0 || amount > max_amount {
                    return vec![];
                }
                vec![
                    PanelAction::Game(UiRequest::StackSliderChanged {
                        split: amount,
                        max: max_amount,
                    }),
                    PanelAction::Game(UiRequest::PaymentList(PaymentAction::Add(object))),
                ]
            }
            ControlEvent::Drop {
                id,
                payload: DragPayload::Object(item),
                ..
            } if id == "items" => request(UiRequest::PaymentList(PaymentAction::Add(item))),
            ControlEvent::Scroll { id, value } if id == "items" || id == "items-scroll" => {
                self.offsets[usize::from(rent)] = value.max(0);
                vec![]
            }
            ControlEvent::Tick => {
                if self
                    .selected
                    .is_some_and(|id| !offered.iter().any(|i| i.id == id))
                {
                    self.selected = None;
                }
                vec![]
            }
            ControlEvent::Activate(id) => match id.as_str() {
                "buy" => request(UiRequest::PaymentList(PaymentAction::Select(HouseOp::Buy))),
                "rent" => request(UiRequest::PaymentList(PaymentAction::Select(HouseOp::Rent))),
                "close" => vec![
                    PanelAction::Game(UiRequest::PaymentList(PaymentAction::Close)),
                    PanelAction::Game(UiRequest::UnregisterSlumlordRange),
                    PanelAction::Close,
                ],
                "pay" => {
                    let Some(h) = c.game.slumlord() else {
                        return vec![];
                    };
                    if rent != (h.owner.0 != 0)
                        || offered.is_empty()
                        || (!rent && !state.buy_payment.paid_in_full)
                    {
                        return vec![];
                    }
                    let question = if rent {
                        !h.am_i_the_owner
                    } else {
                        h.house_type != 4
                    };
                    if question {
                        if std::mem::replace(
                            &mut self.confirmation_pending[usize::from(rent)],
                            true,
                        ) {
                            vec![]
                        } else {
                            request(UiRequest::HousePaymentConfirmation { rent })
                        }
                    } else {
                        self.send_payment(c)
                    }
                }
                _ => vec![],
            },
            ControlEvent::Select { id, index } if id == "items" => {
                self.selected = offered.get(index).map(|i| i.id);
                self.selected
                    .map(|i| request(UiRequest::Select(i)))
                    .unwrap_or_default()
            }
            ControlEvent::DoubleClick { id, index } if id == "items" => offered
                .get(index)
                .map(|i| request(UiRequest::PaymentList(PaymentAction::Remove(i.id))))
                .unwrap_or_default(),
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;
    use dereth_client_contract::view::{SlumlordPayment, SlumlordView};

    use dereth_client_contract::panels::slumlord::{
        HouseOp, PaymentAction, PaymentItem, PaymentListsView,
    };
    #[derive(Debug)]
    struct House {
        profile: SlumlordView,
        state: std::cell::RefCell<PaymentListsView>,
    }
    impl GameView for House {
        fn slumlord(&self) -> Option<SlumlordView> {
            Some(self.profile.clone())
        }
        fn payment_lists(&self) -> PaymentListsView {
            self.state.borrow().clone()
        }
    }
    fn with_house(rent: bool, run: impl FnOnce(&Context<'_>, &House)) {
        let row = PaymentItem {
            id: ObjectId(7),
            wcid: 273,
            amount: 30,
            trade_note_value: None,
        };
        let game = House {
            profile: SlumlordView {
                slumlord: ObjectId(9),
                owner: ObjectId(u32::from(rent)),
                house_type: 1,
                ..Default::default()
            },
            state: std::cell::RefCell::new(PaymentListsView {
                op: if rent { HouseOp::Rent } else { HouseOp::Buy },
                buy: if rent { vec![] } else { vec![row.clone()] },
                rent: if rent { vec![row] } else { vec![] },
                buy_payment: SlumlordPayment {
                    paid_in_full: true,
                    ..Default::default()
                },
                visible: true,
                ..Default::default()
            }),
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
    /// Behaviour: panels.house-purchase.paying-sends-buy-house-with-the-windows-items
    #[test]
    fn drop_and_split_result_route_through_the_shared_list_without_local_insertion() {
        with_house(false, |c, _| {
            let mut panel = Maintenance::default();
            assert_eq!(
                panel.event(
                    ControlEvent::Drop {
                        id: "items".into(),
                        payload: DragPayload::Object(ObjectId(2)),
                        slot: 0
                    },
                    c
                ),
                request(UiRequest::PaymentList(PaymentAction::Add(ObjectId(2))))
            );
            assert!(panel
                .event(ControlEvent::SplitReady(ObjectId(3)), c)
                .is_empty());
        });
    }
    /// Behaviour: panels.house-purchase.confirmations-use-current-payment-and-handle-every-answer
    #[test]
    fn housing_questions_use_shared_confirmation_and_handle_every_answer() {
        for rent in [false, true] {
            with_house(rent, |c, _| {
                let mut panel = Maintenance::default();
                for answer in [None, Some(false), Some(true)] {
                    assert_eq!(
                        panel.event(ControlEvent::Activate("pay".into()), c),
                        request(UiRequest::HousePaymentConfirmation { rent })
                    );
                    assert!(panel
                        .event(ControlEvent::Activate("pay".into()), c)
                        .is_empty());
                    let actions = panel.event(
                        ControlEvent::HousePaymentConfirmation {
                            rent,
                            confirmed: answer,
                        },
                        c,
                    );
                    match answer {
                        None => assert!(actions.is_empty()),
                        Some(false) => assert_eq!(
                            actions,
                            vec![
                                PanelAction::Game(UiRequest::PaymentList(PaymentAction::Close)),
                                PanelAction::Close
                            ]
                        ),
                        Some(true) => assert_eq!(
                            actions,
                            request(UiRequest::PaymentList(PaymentAction::Submit))
                        ),
                    }
                }
            });
        }
    }
    /// Behaviour: panels.house-purchase.confirmations-use-current-payment-and-handle-every-answer
    #[test]
    fn confirmation_rechecks_shared_payment_after_the_question_was_opened() {
        for rent in [false, true] {
            with_house(rent, |c, game| {
                let mut panel = Maintenance::default();
                panel.event(ControlEvent::Activate("pay".into()), c);
                if rent {
                    game.state.borrow_mut().rent.clear();
                } else {
                    game.state.borrow_mut().buy_payment.paid_in_full = false;
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
