use super::*;
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
        let b = f.button("accept", rect(half, 0, 46, 30), " Trade", trading);
        b.font = "14-6".into();
        b.images = Some(
            [
                if t.accepted { "06001DC0" } else { "06001DC3" },
                "06001DC0",
                "06001DBF",
            ]
            .map(String::from),
        );
        f.image(
            if t.partner_accepted {
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
                "accept" if t.open && t.partner.is_some() => request(if t.accepted {
                    UiRequest::TradeDecline
                } else {
                    UiRequest::TradeAccept {
                        displayed_self: t.self_rows.len(),
                        displayed_partner: t.partner_rows.len(),
                    }
                }),
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
    tool: Option<ObjectId>,
    offered: Vec<ObjectId>,
    selected: Option<ObjectId>,
    width: u32,
    offset: i32,
}
impl Salvage {
    fn add_tree(
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
        if children.is_empty() {
            self.add(item, c);
        } else {
            out.push(PanelAction::Host(HostAction::LocalFeedback {
                severity: crate::panels::FeedbackSeverity::Information,
                text: format!("Adding contents of {}", c.game.name(item).unwrap_or("")),
            }));
            for &child in children {
                self.add_tree(child, c, seen, out);
            }
        }
    }
    fn add(&mut self, item: ObjectId, c: &Context<'_>) {
        let material = self
            .offered
            .first()
            .map(|i| c.game.item_material_type(*i))
            .unwrap_or(0);
        if !self.offered.contains(&item)
            && c.game.item_owned_by_player(item)
            && c.game.salvage_item_suitable(item, material)
        {
            self.offered.push(item);
        }
    }
}
impl Panel for Salvage {
    fn resize(&mut self, width: u32, _: u32) {
        self.width = width;
    }
    fn id(&self) -> &'static str {
        "salvage"
    }
    fn set_object(&mut self, o: ObjectId) {
        self.tool = Some(o);
        self.offered.clear();
    }
    fn frame(&self, c: &Context<'_>) -> PanelFrame {
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
            self.offered.iter().map(|i| entry(c, *i)).collect(),
            self.selected,
            self.offset,
            true,
            Some(crate::panels::DropFilter::Salvage {
                material: self
                    .offered
                    .first()
                    .map_or(0, |i| c.game.item_material_type(*i)),
            }),
        );
        let b = f.button(
            "salvage",
            rect(w - 104, 45, 94, 22),
            "Salvage",
            self.tool.is_some() && !self.offered.is_empty(),
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
        match e {
            ControlEvent::Salvage(notice) => {
                use dereth_client_contract::panels::salvage::SalvageNotice;
                match notice {
                    SalvageNotice::Open(tool) => self.set_object(tool),
                    SalvageNotice::Add(item) => {
                        let mut out = vec![];
                        self.add_tree(item, c, &mut vec![], &mut out);
                        return out;
                    }
                    SalvageNotice::Remove(item) => {
                        self.offered.retain(|id| *id != item);
                        if self.selected == Some(item) {
                            self.selected = None;
                        }
                    }
                }
                vec![]
            }
            ControlEvent::Scroll { id, value } if (id == "items" || id == "items-scroll") => {
                self.offset = value.max(0);
                vec![]
            }
            ControlEvent::Drop {
                id,
                payload: DragPayload::Object(item),
                ..
            } if id == "items" => {
                if !c.game.item_owned_by_player(item) {
                    return vec![PanelAction::Host(HostAction::LocalFeedback {
                        severity: crate::panels::FeedbackSeverity::Warning,
                        text: "You can only salvage items that you own!".into(),
                    })];
                }
                let mut out = vec![];
                self.add_tree(item, c, &mut vec![], &mut out);
                out
            }
            ControlEvent::Select { id, index } if id == "items" => {
                self.selected = self.offered.get(index).copied();
                self.selected
                    .map(|i| request(UiRequest::Select(i)))
                    .unwrap_or_default()
            }
            ControlEvent::DoubleClick { id, index } if id == "items" => {
                if index < self.offered.len() {
                    let item = self.offered.remove(index);
                    return vec![PanelAction::Host(HostAction::LocalFeedback {
                        severity: crate::panels::FeedbackSeverity::Information,
                        text: format!(
                            "Removing {} from salvage list",
                            c.game.name(item).unwrap_or("")
                        ),
                    })];
                }
                vec![]
            }
            ControlEvent::Activate(id) if id == "salvage" => {
                if let Some(tool) = self.tool {
                    if !self.offered.is_empty() {
                        let mut items = std::mem::take(&mut self.offered);
                        items.reverse();
                        return request(UiRequest::SalvageItems { tool, items });
                    }
                }
                vec![]
            }
            ControlEvent::Activate(id) if id == "close" => {
                self.offered.clear();
                vec![PanelAction::Close]
            }
            _ => vec![],
        }
    }
}
