//! The inventory, equipment and external container panels.
use super::super::*;
use super::common::*;
use crate::int::{i32_from, u32_from};
use dereth_client_contract::view::DropTarget;
use dereth_primitives::num::to_i32;
use dereth_rules::slots::loc;

const EQUIPMENT: [(i32, i32, u32, u32, usize); 10] = [
    (27, 33, 0x06000f68, loc::NECK_WEAR, 0),
    (156, 118, 0x06000f6a, loc::WRIST_WEAR_LEFT, 1),
    (156, 151, 0x06000f6b, loc::FINGER_WEAR_LEFT, 1),
    (13, 118, 0x06000f5d, loc::WRIST_WEAR_RIGHT, 2),
    (13, 151, 0x06000f5a, loc::FINGER_WEAR_RIGHT, 2),
    (138, 202, 0x06000f66, loc::WEAPON_READY_SLOT, 0),
    (172, 202, 0x06000f5e, loc::MISSILE_AMMO, 0),
    (13, 202, 0x06000f6c, loc::SHIELD, 0),
    (190, 118, 0x060032c5, loc::CHEST_WEAR, 0),
    (190, 151, 0x060032c4, loc::UPPER_LEG_WEAR, 0),
];
/// The slots a world after the classic interface adds to the paper doll, where the doll leaves
/// room: the cloak and the trinket on the bottom row, the three aetheria sigils beside the head.
/// The classic portal has no art for them, so each wears a slot composed from its own pieces in
/// the classic slots' style: a bevelled frame with a garment's, a flask's or a crystal's outline.
const LATER_EQUIPMENT: [(i32, i32, u32, u32, usize); 5] = [
    (55, 202, crate::composed::CLOAK_SLOT, loc::CLOAK, 0),
    (97, 202, crate::composed::TRINKET_SLOT, loc::TRINKET_ONE, 0),
    (156, 33, crate::composed::SIGIL_SLOT, loc::SIGIL_ONE, 0),
    (156, 66, crate::composed::SIGIL_SLOT, loc::SIGIL_TWO, 0),
    (13, 66, crate::composed::SIGIL_SLOT, loc::SIGIL_THREE, 0),
];

/// The paper doll's slots on this world: the classic ones, then the cloak, the trinket and the
/// sigils where the world has them.
fn equipment_slots(game: &dyn GameView) -> Vec<(i32, i32, u32, u32, usize)> {
    let features = game.era_features();
    let sigils = game.aetheria_slots();
    let mut slots = EQUIPMENT.to_vec();
    for slot in LATER_EQUIPMENT {
        let wanted = match slot.3 {
            loc::CLOAK => features.cloaks,
            loc::TRINKET_ONE => features.trinkets,
            loc::SIGIL_ONE => sigils & 1 != 0,
            loc::SIGIL_TWO => sigils & 2 != 0,
            loc::SIGIL_THREE => sigils & 4 != 0,
            _ => false,
        };
        if wanted {
            slots.push(slot);
        }
    }
    slots
}

pub fn entry(game: &dyn GameView, id: ObjectId) -> ItemEntry {
    if id.0 == 0 {
        return ItemEntry::empty();
    }
    ItemEntry {
        id,
        icon: game.icon(id),
        decoration: game.slot_decoration(id),
        caption: game.name(id).unwrap_or("").into(),
        count: game.int_stat(id, 0xc).unwrap_or(1).max(0) as u32,
        amount: None,
        active_container: false,
        disabled: game.item_waiting(id),
    }
}
/// A list reserves its actual capacity, or rounds an unbounded list plus its spare slot.
fn slots(
    g: &dyn GameView,
    object: ObjectId,
    containers: bool,
    columns: usize,
    visible: usize,
) -> Vec<ObjectId> {
    let contents = if containers {
        g.contained_containers(object)
    } else {
        g.container_contents(object)
    };
    let capacity = if containers {
        g.containers_capacity(object)
    } else {
        g.items_capacity(object)
    };
    let count = match capacity {
        Some(_)
            if g.slot_decoration(object)
                .is_some_and(|d| !d.openable && !d.is_player) =>
        {
            0
        }
        Some(n) if n >= 0 => n as usize,
        Some(_) => ((contents.len() + columns) / columns * columns).max(visible),
        None => contents.len(),
    };
    let mut slots = contents.iter().copied().take(count).collect::<Vec<_>>();
    slots.resize(count, ObjectId(0));
    slots
}
fn mark_active(frame: &mut PanelFrame, active: Option<ObjectId>) {
    for control in &mut frame.controls {
        if control.id != "containers" {
            continue;
        }
        if let ControlKind::Items {
            entries,
            slot_size: 36,
            ..
        }
        | ControlKind::ItemStrip {
            entries,
            slot_size: 36,
            ..
        } = &mut control.kind
        {
            for entry in entries {
                entry.active_container = entry.id.0 != 0 && Some(entry.id) == active;
            }
        }
    }
}

fn grid(
    f: &mut PanelFrame,
    game: &dyn GameView,
    id: &str,
    r: crate::widgets::Rect,
    items: &[ObjectId],
    columns: u32,
    slot_size: i32,
) {
    f.control(
        id,
        r,
        ControlKind::Items {
            entries: items.iter().map(|id| entry(game, *id)).collect(),
            columns,
            slot_size,
            selected: game.selected_object(),
        },
        true,
    );
}
/// Which button clicked an item's tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Click {
    Left,
    Right,
}
/// A click on an item's tile. A right click selects the item and examines it. A left click with a
/// targeting cursor armed does what the cursor is for: use it, examine it, use the item waiting
/// for a target on it, or (for a spell) select it. `None` is an ordinary left click, which the
/// tile's owner answers by selecting the item. A double click adds nothing to its first click.
pub(super) fn item_click(
    object: ObjectId,
    click: Click,
    ctx: &Context<'_>,
) -> Option<Vec<PanelAction>> {
    let examine = || {
        vec![
            PanelAction::Game(UiRequest::Select(object)),
            PanelAction::OpenObject {
                id: "examine".into(),
                object,
            },
        ]
    };
    if click == Click::Right {
        return Some(examine());
    }
    match ctx.classic.cursor_mode {
        1 => Some(vec![PanelAction::Game(UiRequest::Use(object))]),
        2 => Some(examine()),
        4 => Some(vec![PanelAction::Game(UiRequest::ExecuteTargetItem(
            object,
        ))]),
        5 => Some(vec![PanelAction::Game(UiRequest::Select(object))]),
        _ => None,
    }
}
fn picked_equipment(ctx: &Context<'_>, mask: u32) -> Option<ObjectId> {
    use dereth_client_model::objects::{determine_higher_priority, InventoryPlacement};
    let player = ctx.game.player()?;
    if mask == 0 {
        return None;
    }
    let mut best = InventoryPlacement {
        iid: ObjectId(0),
        loc: 0,
        priority: 0,
    };
    for &(iid, loc) in ctx.game.equipment(player) {
        if loc & mask != 0 {
            best = determine_higher_priority(
                InventoryPlacement {
                    iid,
                    loc,
                    priority: ctx
                        .classic
                        .equipment_priority
                        .get(&iid)
                        .copied()
                        .unwrap_or(0),
                },
                best,
                mask,
            );
        }
    }
    Some(if best.iid.0 == 0 { player } else { best.iid })
}
#[derive(Debug, Default)]
pub struct Inventory {
    item_scroll: std::cell::Cell<i32>,
    container_scroll: i32,
    displayed_container: std::cell::Cell<Option<ObjectId>>,
}
impl Inventory {
    fn container(&self, g: &dyn GameView) -> Option<ObjectId> {
        g.open_inventory_container()
    }
}
impl Panel for Inventory {
    fn id(&self) -> &'static str {
        "inventory"
    }
    fn set_object(&mut self, _id: ObjectId) {
        self.item_scroll.set(0);
    }
    fn frame(&self, ctx: &Context<'_>) -> PanelFrame {
        let g = ctx.game;
        if self
            .displayed_container
            .replace(g.open_inventory_container())
            != g.open_inventory_container()
        {
            self.item_scroll.set(0);
        }
        // Stretched, the page's art keeps its top 360 rows with a dark tile below, and the
        // contents grid grows downwards.
        let height = crate::panels::side_height() as i32;
        let grid_height = (height - 266) / 32 * 32;
        let mut f = PanelFrame::new(300, height as u32);
        image(&mut f, 0x06000f5b, rect(0, 0, 300, 362), None, true, false);
        if height > 360 {
            image(
                &mut f,
                0x06000518,
                rect(0, 360, 300, height - 360),
                None,
                true,
                false,
            );
        }
        text(
            &mut f,
            rect(2, 2, 272, 16),
            format!("Inventory of {}", g.character_name().unwrap_or("")),
            "14-6",
            CREAM,
            1,
            false,
            None,
        );
        art(
            f.button("close", rect(276, 0, 24, 23), "", true),
            0x060011ac,
            0x060011ad,
            0x060011ac,
        );
        f.preview(Preview {
            kind: PreviewKind::PaperDoll,
            rect: rect(0, 23, 246, 215),
            object: g.player(),
            appearance: None,
        });
        let canvas = f.button("paperdoll", rect(59, 23, 80, 212), "", true);
        canvas.paint = false;
        canvas.drop_equipment_canvas = true;
        if let Some(player) = g.player() {
            for (i, (x, y, did, mask, _slot)) in equipment_slots(g).iter().enumerate() {
                let equipped = g
                    .equipment(player)
                    .iter()
                    .find(|(_, location)| location & mask != 0)
                    .map(|r| r.0);
                image(
                    &mut f,
                    if equipped.is_some() { 0x060011f9 } else { *did },
                    rect(*x, *y, 32, 32),
                    None,
                    false,
                    false,
                );
                grid(
                    &mut f,
                    g,
                    &format!("equip:{i}"),
                    rect(*x, *y, 32, 32),
                    &equipped.into_iter().collect::<Vec<_>>(),
                    1,
                    32,
                );
                let control = f.controls.last_mut().unwrap();
                control.drop_location = Some(*mask);
                if let ControlKind::Items { entries, .. } = &mut control.kind {
                    if entries.is_empty() {
                        entries.push(ItemEntry::empty());
                    }
                }
            }
            grid(
                &mut f,
                g,
                "backpack",
                rect(245, 43, 36, 36),
                &[player],
                1,
                36,
            );
            grid(
                &mut f,
                g,
                "containers",
                rect(245, 84, 36, 252),
                &{
                    let all = slots(g, player, true, 1, 7);
                    all.into_iter()
                        .skip((self.container_scroll.max(0) / 36) as usize)
                        .collect::<Vec<_>>()
                },
                1,
                36,
            );
        }
        if let Some(container) = self.container(g) {
            // The player's own pack is "Backpack".
            let name = if Some(container) == g.player() {
                "Backpack"
            } else {
                g.name(container).unwrap_or("")
            };
            text(
                &mut f,
                rect(17, 239, 188, 15),
                format!("Contents of {name}"),
                "14-6",
                CREAM,
                0,
                false,
                None,
            );
            grid(
                &mut f,
                g,
                "items",
                rect(15, 257, 192, grid_height),
                &{
                    let all = slots(g, container, false, 6, (grid_height / 32 * 6) as usize);
                    all.into_iter()
                        .skip((self.item_scroll.get().max(0) / 32) as usize * 6)
                        .collect::<Vec<_>>()
                },
                6,
                32,
            );
        }
        if let Some(player) = g.player() {
            scrollbar(
                &mut f,
                "containers-scroll",
                rect(283, 82, 16, 256),
                i32_from(slots(g, player, true, 1, 7).len()) * 36,
                252,
                self.container_scroll,
                36,
                true,
            );
        }
        if let Some(container) = self.container(g) {
            scrollbar(
                &mut f,
                "items-scroll",
                rect(207, 257, 20, grid_height),
                i32_from(
                    slots(g, container, false, 6, (grid_height / 32 * 6) as usize)
                        .len()
                        .div_ceil(6),
                ) * 32,
                grid_height,
                self.item_scroll.get(),
                32,
                true,
            );
        }
        image(
            &mut f,
            0x0600121d,
            rect(196, 33, 11, 61),
            None,
            false,
            false,
        );
        // The burden meter reads 0 to 300 percent: the bar (green at the bottom, red at the top)
        // is drawn at its own size and uncovered from the bottom, and the percentage is written
        // below it.
        let ratio = g.load().map_or(0.0, |load| (load / 3.0).clamp(0.0, 1.0));
        let h = to_i32(61.0 * ratio);
        image(
            &mut f,
            0x0600121c,
            rect(196, 33, 11, 61),
            Some([196, 94 - h, 207, 94]),
            false,
            false,
        );
        text(
            &mut f,
            rect(167, 98, 60, 15),
            format!("{}%", to_i32(ratio * 300.0)),
            "14-6",
            CREAM,
            1,
            false,
            None,
        );
        mark_active(&mut f, self.container(g));
        f
    }
    fn event(&mut self, e: ControlEvent, ctx: &Context<'_>) -> Vec<PanelAction> {
        let g = ctx.game;
        let drag = matches!(e, ControlEvent::DragStart { .. });
        let double = matches!(e, ControlEvent::DoubleClick { .. });
        let right = matches!(e, ControlEvent::RightClick { .. });
        match e {
            ControlEvent::PreviewHit {
                equipment_mask,
                right_click,
                double_click,
                ..
            } => {
                // A click on the doll: a right click examines what it hit; a left click with a
                // targeting cursor armed acts on it (a targeted use lands on the wearer); an
                // ordinary left click selects it. A double click adds nothing.
                let picked = picked_equipment(ctx, equipment_mask);
                if right_click {
                    return picked
                        .and_then(|object| item_click(object, Click::Right, ctx))
                        .unwrap_or_default();
                }
                if double_click {
                    return vec![];
                }
                if ctx.classic.cursor_mode == 4 {
                    return vec![PanelAction::Game(UiRequest::ExecuteTargetItem(
                        if equipment_mask == 0 {
                            ObjectId(0)
                        } else {
                            g.player().unwrap_or(ObjectId(0))
                        },
                    ))];
                }
                if let Some(object) = picked {
                    return item_click(object, Click::Left, ctx)
                        .unwrap_or_else(|| vec![PanelAction::Game(UiRequest::Select(object))]);
                }
            }
            ControlEvent::Scroll { id, value } if id == "items-scroll" => {
                self.item_scroll.set(value.max(0))
            }
            ControlEvent::Scroll { id, value } if id == "containers-scroll" => {
                self.container_scroll = value.max(0)
            }
            ControlEvent::Activate(id) if id == "close" => return vec![PanelAction::Close],
            ControlEvent::Select { id, index }
            | ControlEvent::DoubleClick { id, index }
            | ControlEvent::RightClick { id, index }
            | ControlEvent::DragStart { id, index } => {
                let object = if id == "items" {
                    self.container(g).and_then(|c| {
                        g.container_contents(c)
                            .get(index + (self.item_scroll.get().max(0) / 32) as usize * 6)
                            .copied()
                    })
                } else if id == "containers" {
                    g.player().and_then(|p| {
                        g.contained_containers(p)
                            .get(index + (self.container_scroll.max(0) / 36) as usize)
                            .copied()
                    })
                } else if id == "backpack" {
                    g.player()
                } else if let Some(slot) = id
                    .strip_prefix("equip:")
                    .and_then(|s| s.parse::<usize>().ok())
                {
                    equipment_slots(g)
                        .get(slot)
                        .copied()
                        .and_then(|(_, _, _, mask, _slot)| {
                            g.player().and_then(|p| {
                                g.equipment(p)
                                    .iter()
                                    .find(|(_, loc)| loc & mask != 0)
                                    .map(|r| r.0)
                            })
                        })
                } else {
                    None
                };
                if let Some(object) = object {
                    if right {
                        return item_click(object, Click::Right, ctx).unwrap_or_default();
                    }
                    // A double click uses an item in the pack or a worn item; the packs column
                    // and the backpack itself do nothing more.
                    if double {
                        return if id == "items" || id.starts_with("equip:") {
                            vec![PanelAction::Game(UiRequest::Use(object))]
                        } else {
                            vec![]
                        };
                    }
                    if !drag {
                        if let Some(actions) = item_click(object, Click::Left, ctx) {
                            return actions;
                        }
                    }
                    if drag {
                        return vec![PanelAction::BeginDrag(DragPayload::Object(object))];
                    }
                    if id == "containers" || id == "backpack" {
                        self.item_scroll.set(0);
                        return vec![
                            PanelAction::Game(UiRequest::NewParentContainer(object)),
                            PanelAction::Game(UiRequest::Select(object)),
                        ];
                    }
                    return vec![PanelAction::Game(UiRequest::Select(object))];
                }
            }
            ControlEvent::Drop {
                id,
                payload: DragPayload::Object(item),
                slot,
            } => {
                if id == "paperdoll" {
                    return vec![PanelAction::Game(UiRequest::DragDrop {
                        item,
                        target: DropTarget::EquipCanvas,
                    })];
                }
                if let Some(index) = id
                    .strip_prefix("equip:")
                    .and_then(|s| s.parse::<usize>().ok())
                {
                    if let Some((_, _, _, mask, side)) = equipment_slots(g).get(index).copied() {
                        return vec![PanelAction::Game(UiRequest::DragDrop {
                            item,
                            target: DropTarget::EquipLocation {
                                mask,
                                side: u32_from(side),
                            },
                        })];
                    }
                }
                if id == "backpack" {
                    return vec![PanelAction::Game(UiRequest::DragDrop {
                        item,
                        target: DropTarget::BackpackButton,
                    })];
                }
                if id == "items" || id == "containers" {
                    if let Some(container) = if id == "containers" {
                        g.player()
                    } else {
                        self.container(g)
                    } {
                        let slot = slot
                            + if id == "containers" {
                                (self.container_scroll.max(0) / 36) as u32
                            } else {
                                (self.item_scroll.get().max(0) / 32) as u32 * 6
                            };
                        let list = if id == "containers" {
                            g.contained_containers(container)
                        } else {
                            g.container_contents(container)
                        };
                        return vec![PanelAction::Game(UiRequest::DragDrop {
                            item,
                            target: DropTarget::ItemListSlot {
                                container,
                                under: list.get(slot as usize).copied(),
                                index: slot,
                                num_ui_items: u32_from(list.len()),
                                dragged_is_container: g
                                    .slot_decoration(item)
                                    .is_some_and(|s| s.is_container),
                                container_list: id == "containers",
                            },
                        })];
                    }
                }
            }
            _ => {}
        }
        vec![]
    }
}
#[derive(Debug, Default)]
pub struct ExternalContainer {
    object: Option<ObjectId>,
    close_pending: bool,
    child: Option<ObjectId>,
    offset: usize,
    containers_offset: usize,
    /// The window's width: the 3D view's, as for every window docked under it.
    width: u32,
}
impl Panel for ExternalContainer {
    fn resize(&mut self, width: u32, _: u32) {
        self.width = width;
    }
    fn id(&self) -> &'static str {
        "external-container"
    }
    fn set_object(&mut self, id: ObjectId) {
        self.object = Some(id);
        self.close_pending = false;
        self.child = None;
        self.offset = 0;
    }
    fn frame(&self, ctx: &Context<'_>) -> PanelFrame {
        let g = ctx.game;
        let host = i32::try_from(self.width).unwrap_or(0).max(400);
        let mut f = PanelFrame::new(u32::try_from(host).unwrap_or(400), 102);
        // The background tiles the whole width (the 2005 game stopped at 4000 pixels, leaving
        // the rest of a wider view unpainted); the close button keeps to the right.
        image(&mut f, 0x060011bb, rect(0, 0, host, 102), None, true, false);
        art(
            f.button(
                "close",
                rect(host - 30, 2, 25, 23),
                "",
                self.object.is_some() && !self.close_pending,
            ),
            0x060011bd,
            0x060011bc,
            0x060011bc,
        );
        if let Some(object) = self.object {
            let width = external_width(g, object, host);
            let center = (host - width) / 2;
            let grid_width = (width - 40) / 32 * 32;
            let x = center + 20;
            image(
                &mut f,
                0x060011ba,
                rect(center + 17, 44, 3, 58),
                None,
                true,
                false,
            );
            image(
                &mut f,
                0x060011ba,
                rect(x + grid_width, 44, 3, 58),
                None,
                true,
                false,
            );
            image(
                &mut f,
                0x060011b9,
                rect(x, 44, grid_width, 3),
                None,
                true,
                false,
            );
            image(
                &mut f,
                0x060011b9,
                rect(x, 99, grid_width, 3),
                None,
                true,
                false,
            );
            grid(&mut f, g, "parent", rect(5, 6, 36, 36), &[object], 1, 36);
            // The packs strip, centred with the items; its own width, and as many slots as the
            // container holds packs.
            let pack_width = ((width - 120) / 36 * 36).max(0);
            let packs = slots(g, object, true, 1, (pack_width / 36).max(0) as usize);
            let pack_count = packs.len();
            f.control(
                "containers",
                rect(center + 67, 6, pack_width, 36),
                ControlKind::ItemStrip {
                    entries: packs.iter().map(|id| entry(g, *id)).collect(),
                    slot_size: 36,
                    selected: g.selected_object(),
                    offset: i32_from(self.containers_offset) * 36,
                },
                true,
            );
            let container = self.child.unwrap_or(object);
            let contents = slots(g, container, false, 1, (grid_width / 32).max(0) as usize);
            f.control(
                "items",
                rect(x, 47, grid_width, 32),
                ControlKind::ItemStrip {
                    entries: contents.iter().map(|id| entry(g, *id)).collect(),
                    slot_size: 32,
                    selected: g.selected_object(),
                    offset: i32_from(self.offset) * 32,
                },
                true,
            );
            scrollbar(
                &mut f,
                "items-scroll",
                rect(x, 79, grid_width, 20),
                i32_from(contents.len()) * 32,
                grid_width,
                i32_from(self.offset) * 32,
                32,
                false,
            );
            // The packs' scroll arrows: the left one fixed beside the container's icon, the
            // right one at the strip's end; each shows only while there is somewhere to scroll.
            let visible_packs = (pack_width / 36).max(0) as usize;
            art(
                f.button(
                    "previous",
                    rect(42, 2, 23, 42),
                    "",
                    self.containers_offset >= 1,
                ),
                0x060011b5,
                0x060011b6,
                0x060011be,
            );
            art(
                f.button(
                    "next",
                    rect(center + 67 + pack_width, 3, 28, 40),
                    "",
                    self.containers_offset + visible_packs < pack_count,
                ),
                0x060011b7,
                0x060011b8,
                0x060011bf,
            );
        }
        mark_active(&mut f, self.child.or(self.object));
        f
    }
    fn event(&mut self, e: ControlEvent, ctx: &Context<'_>) -> Vec<PanelAction> {
        let Some(object) = self.object else {
            return if matches!(e,ControlEvent::Activate(ref id) if id=="close") {
                vec![PanelAction::Close]
            } else {
                vec![]
            };
        };
        let g = ctx.game;
        if let ControlEvent::DragStart { id, index } = &e {
            let target = match id.as_str() {
                "items" => g
                    .container_contents(self.child.unwrap_or(object))
                    .get(*index)
                    .copied(),
                "containers" => g.contained_containers(object).get(*index).copied(),
                "parent" => Some(object),
                _ => None,
            };
            return target
                .map(|id| vec![PanelAction::BeginDrag(DragPayload::Object(id))])
                .unwrap_or_default();
        }
        match e {
            ControlEvent::Activate(id) if id == "close" => {
                if self.close_pending {
                    return vec![];
                }
                // Close disables its button and sends use; the panel hides later, when
                // the server's update for the ground object arrives.
                self.close_pending = true;
                return vec![PanelAction::Game(UiRequest::CloseExternalContainer(object))];
            }
            ControlEvent::Activate(id) if id == "previous" => {
                self.containers_offset = self.containers_offset.saturating_sub(1)
            }
            ControlEvent::Activate(id) if id == "next" => {
                self.containers_offset = (self.containers_offset + 1)
                    .min(g.contained_containers(object).len().saturating_sub(1))
            }
            ControlEvent::Scroll { id, value } if id == "items-scroll" || id == "items" => {
                self.offset = (value.max(0) / 32) as usize
            }
            ControlEvent::Scroll { id, value } if id == "containers" => {
                self.containers_offset = (value.max(0) / 36) as usize
            }
            ControlEvent::Select { id, index } => {
                let target = if id == "parent" {
                    self.child = None;
                    Some(object)
                } else if id == "containers" {
                    let child = g.contained_containers(object).get(index).copied();
                    self.child = child;
                    self.offset = 0;
                    child
                } else if id == "items" {
                    g.container_contents(self.child.unwrap_or(object))
                        .get(index)
                        .copied()
                } else {
                    None
                };
                if let Some(target) = target {
                    return item_click(target, Click::Left, ctx)
                        .unwrap_or_else(|| vec![PanelAction::Game(UiRequest::Select(target))]);
                }
            }
            // A double click on an item in the container uses it.
            ControlEvent::DoubleClick { id, index } if id == "items" => {
                if let Some(item) = g
                    .container_contents(self.child.unwrap_or(object))
                    .get(index)
                    .copied()
                {
                    return vec![PanelAction::Game(UiRequest::Use(item))];
                }
            }
            ControlEvent::RightClick { id, index } => {
                let item = match id.as_str() {
                    "items" => g
                        .container_contents(self.child.unwrap_or(object))
                        .get(index)
                        .copied(),
                    "containers" => g.contained_containers(object).get(index).copied(),
                    "parent" => Some(object),
                    _ => None,
                };
                if let Some(item) = item {
                    return item_click(item, Click::Right, ctx).unwrap_or_default();
                }
            }
            ControlEvent::Drop {
                id,
                payload: DragPayload::Object(item),
                slot,
            } if id == "items" => {
                let container = self.child.unwrap_or(object);
                let list = g.container_contents(container);
                let index = slot;
                return vec![PanelAction::Game(UiRequest::DragDrop {
                    item,
                    target: DropTarget::ItemListSlot {
                        container,
                        under: list.get(index as usize).copied(),
                        index,
                        num_ui_items: u32_from(list.len()),
                        dragged_is_container: g
                            .slot_decoration(item)
                            .is_some_and(|s| s.is_container),
                        container_list: false,
                    },
                })];
            }
            _ => {}
        }
        vec![]
    }
}

fn external_width(g: &dyn GameView, object: ObjectId, host_width: i32) -> i32 {
    match (g.items_capacity(object), g.containers_capacity(object)) {
        (Some(items), Some(containers)) if items >= 0 && containers >= 0 => {
            (items.saturating_mul(32).saturating_add(40))
                .max(containers.saturating_mul(36).saturating_add(120))
                .min(host_width)
        }
        _ => host_width,
    }
}

#[cfg(test)]
mod slot_tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;
    #[derive(Debug)]
    struct Container {
        cap: i32,
        contents: Vec<ObjectId>,
        open: bool,
    }
    impl GameView for Container {
        fn container_contents(&self, _: ObjectId) -> &[ObjectId] {
            &self.contents
        }
        fn items_capacity(&self, _: ObjectId) -> Option<i32> {
            Some(self.cap)
        }
        fn slot_decoration(
            &self,
            _: ObjectId,
        ) -> Option<dereth_client_contract::view::SlotDecoration> {
            Some(dereth_client_contract::view::SlotDecoration {
                openable: self.open,
                ..Default::default()
            })
        }
    }
    #[test]
    fn bounded_capacity_and_unbounded_spare_slots_preserve_native_row_rounding() {
        let mut g = Container {
            cap: 5,
            contents: vec![ObjectId(8), ObjectId(9)],
            open: true,
        };
        assert_eq!(
            slots(&g, ObjectId(1), false, 6, 18),
            [
                ObjectId(8),
                ObjectId(9),
                ObjectId(0),
                ObjectId(0),
                ObjectId(0)
            ]
        );
        g.cap = -1;
        assert_eq!(slots(&g, ObjectId(1), false, 6, 18).len(), 18);
        g.contents = (1..=18).map(ObjectId).collect();
        assert_eq!(slots(&g, ObjectId(1), false, 6, 18).len(), 24);
        g.open = false;
        assert!(slots(&g, ObjectId(1), false, 6, 18).is_empty());
    }
    #[derive(Debug)]
    struct Corpse;
    impl GameView for Corpse {
        fn items_capacity(&self, _: ObjectId) -> Option<i32> {
            Some(120)
        }
        fn containers_capacity(&self, _: ObjectId) -> Option<i32> {
            Some(10)
        }
        fn slot_decoration(
            &self,
            _: ObjectId,
        ) -> Option<dereth_client_contract::view::SlotDecoration> {
            Some(dereth_client_contract::view::SlotDecoration {
                openable: true,
                ..Default::default()
            })
        }
    }
    #[test]
    fn a_wide_container_view_centres_its_rows_and_puts_the_pack_arrow_at_the_strip_end() {
        let pregame = Default::default();
        let keyboard = Default::default();
        let settings = Default::default();
        let classic = Default::default();
        let ctx = Context {
            game: &Corpse,
            pregame: &pregame,
            keyboard: &keyboard,
            settings: &settings,
            map_teleport_allowed: false,
            classic: &classic,
        };
        let mut view = ExternalContainer::default();
        view.resize(4811, 102);
        view.set_object(ObjectId(7));
        let f = view.frame(&ctx);
        let control = |id: &str| f.controls.iter().find(|c| c.id == id).unwrap();
        // content 3880 wide, centred: margin 465; the pack strip 3744 wide holds ten slots.
        assert_eq!(control("containers").rect, rect(532, 6, 3744, 36));
        if let ControlKind::ItemStrip { entries, .. } = &control("containers").kind {
            assert_eq!(entries.len(), 10);
        }
        assert_eq!(control("next").rect, rect(4276, 3, 28, 40));
        assert!(!control("next").enabled && !control("previous").enabled);
        assert_eq!(control("previous").rect, rect(42, 2, 23, 42));
        assert_eq!(control("close").rect, rect(4781, 2, 25, 23));
        assert_eq!(control("items").rect.x, 485);
    }
}

#[cfg(test)]
mod later_slot_tests {
    //! Behaviour: none (classic front-end adapter; which systems a world has is the era's).
    use super::*;
    #[derive(Debug, Default)]
    struct World(Option<dereth_client_contract::EraView>);
    impl GameView for World {
        fn era(&self) -> Option<&dereth_client_contract::EraView> {
            self.0.as_ref()
        }
    }
    #[test]
    fn the_paper_doll_has_the_later_slots_only_where_the_world_has_them() {
        assert_eq!(
            equipment_slots(&World::default()).len(),
            EQUIPMENT.len() + 2
        );
        let mut era = dereth_client_contract::EraView::default();
        era.era = dereth_primitives::era::EraId::Infiltration;
        era.era_announced = true;
        assert_eq!(
            equipment_slots(&World(Some(era.clone()))).len(),
            EQUIPMENT.len()
        );
        era.announced_features.set("cloaks", true);
        let slots = equipment_slots(&World(Some(era)));
        assert_eq!(slots.len(), EQUIPMENT.len() + 1);
        assert_eq!(slots.last().map(|s| s.3), Some(0x0800_0000));
    }
}
