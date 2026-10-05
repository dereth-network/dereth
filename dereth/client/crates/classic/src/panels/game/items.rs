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
/// The slots a world after the classic interface adds to the paper doll, which has no room for
/// them: the cloak, the trinket and the three aetheria sigils (blue, yellow and red). They open
/// from the accessories button beside the shield, in a flyout over the doll's legs. The classic
/// portal has no art for them; each wears the client's own picture in the classic slots' style,
/// under the later files' id for that slot ([`crate::composed`] makes one should that record be
/// missing).
const ACCESSORIES: [(u32, u32); 5] = [
    (crate::composed::CLOAK_SLOT, loc::CLOAK),
    (crate::composed::TRINKET_SLOT, loc::TRINKET_ONE),
    (crate::composed::SIGIL_SLOTS[0], loc::SIGIL_ONE),
    (crate::composed::SIGIL_SLOTS[1], loc::SIGIL_TWO),
    (crate::composed::SIGIL_SLOTS[2], loc::SIGIL_THREE),
];

/// The accessories this world and character have, as indexes into [`ACCESSORIES`]: the cloak
/// and the trinket where the world has them, and each sigil the character has unlocked on a
/// world with aetheria.
fn accessories(game: &dyn GameView) -> Vec<usize> {
    let features = game.era_features();
    let sigils = game.aetheria_slots();
    [
        features.cloaks,
        features.trinkets,
        sigils & 1 != 0,
        sigils & 2 != 0,
        sigils & 4 != 0,
    ]
    .iter()
    .enumerate()
    .filter_map(|(k, shown)| shown.then_some(k))
    .collect()
}

/// The accessories button: one slot beside the shield, where the doll would put a cloak.
const BUTTON: crate::widgets::Rect = rect(55, 202, 32, 32);
/// How long a dragged item held over the closed button takes to open the flyout.
const SPRING_SECONDS: f64 = 0.4;

/// How the flyout sets its slots out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // one layout is chosen; the other stays to be compared
enum FlyoutLayout {
    /// One row: the cloak, the trinket, then the sigils.
    Row,
    /// The sigils on one row, over the cloak and the trinket on the other.
    Grid,
}
/// The layout the flyout uses.
const FLYOUT_LAYOUT: FlyoutLayout = FlyoutLayout::Row;

/// The flyout's frame, its title row and its collapse arrow, and each slot it shows (an index
/// into [`ACCESSORIES`] and where it is).
#[derive(Clone, Debug, PartialEq, Eq)]
struct FlyoutGeometry {
    panel: crate::widgets::Rect,
    title: crate::widgets::Rect,
    collapse: crate::widgets::Rect,
    slots: Vec<(usize, crate::widgets::Rect)>,
}

/// Where the flyout lies for the accessories `shown`: a bevelled panel standing on the button,
/// centred over the doll's legs and kept on the doll, its title row on top and its slots under it.
fn flyout_geometry(layout: FlyoutLayout, shown: &[usize]) -> Option<FlyoutGeometry> {
    const EDGE: i32 = 3;
    const PAD: i32 = 4;
    const TITLE: i32 = 16;
    const GAP: i32 = 2;
    const SLOT: i32 = 32;
    // Wide enough for the title and the arrow.
    const MIN_WIDTH: i32 = 96;
    // The middle of the doll's legs, and the doll's right edge before the packs.
    const LEGS: i32 = 99;
    const DOLL_RIGHT: i32 = 244;
    if shown.is_empty() {
        return None;
    }
    let rows: Vec<Vec<usize>> = match layout {
        FlyoutLayout::Row => vec![shown.to_vec()],
        FlyoutLayout::Grid => {
            let (sigils, worn): (Vec<usize>, Vec<usize>) = shown.iter().partition(|&&k| k >= 2);
            [sigils, worn]
                .into_iter()
                .filter(|r| !r.is_empty())
                .collect()
        }
    };
    let span = |n: usize| i32_from(n) * SLOT + (i32_from(n) - 1).max(0) * GAP;
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    let w = (span(columns) + 2 * (EDGE + PAD)).max(MIN_WIDTH);
    let h = EDGE + TITLE + span(rows.len()) + PAD + EDGE;
    let x = (LEGS - w / 2).clamp(2, DOLL_RIGHT - w);
    let y = BUTTON.y - GAP - h;
    let mut slots = vec![];
    for (r, row) in rows.iter().enumerate() {
        let left = x + (w - span(row.len())) / 2;
        let top = y + EDGE + TITLE + i32_from(r) * (SLOT + GAP);
        for (c, k) in row.iter().enumerate() {
            slots.push((*k, rect(left + i32_from(c) * (SLOT + GAP), top, SLOT, SLOT)));
        }
    }
    Some(FlyoutGeometry {
        panel: rect(x, y, w, h),
        title: rect(
            x + EDGE + PAD,
            y + EDGE,
            w - 2 * (EDGE + PAD) - 14,
            TITLE - 1,
        ),
        collapse: rect(x + w - EDGE - 2 - 12, y + EDGE + 2, 12, 12),
        slots,
    })
}

/// The accessories flyout's state.
#[derive(Debug, Default)]
struct Flyout {
    open: bool,
    /// Opened by a dragged item held over the button: the drag leaving it without dropping, or
    /// ending elsewhere, closes it again.
    sprung: bool,
    /// Since when a dragged item has been held over the closed button.
    hover_since: Option<dereth_primitives::LocalTime>,
    /// The item a drag in progress carries.
    dragged: Option<ObjectId>,
}
impl Flyout {
    fn close(&mut self) {
        self.open = false;
        self.sprung = false;
        self.hover_since = None;
    }
}

/// The equipment location a slot control stands for, and the side a paired slot names: a slot
/// on the doll (`equip:<n>`) or one of the flyout's (`accessory:<k>`).
fn slot_location(id: &str) -> Option<(u32, usize)> {
    if let Some(n) = id.strip_prefix("equip:") {
        return EQUIPMENT
            .get(n.parse::<usize>().ok()?)
            .map(|&(_, _, _, mask, side)| (mask, side));
    }
    let k = id.strip_prefix("accessory:")?.parse::<usize>().ok()?;
    ACCESSORIES.get(k).map(|&(_, mask)| (mask, 0))
}

/// What the player wears at `mask`.
fn worn(g: &dyn GameView, mask: u32) -> Option<ObjectId> {
    let player = g.player()?;
    g.equipment(player)
        .iter()
        .find(|(_, location)| location & mask != 0)
        .map(|r| r.0)
}

/// Whether `item` goes only in the flyout's slots: it fits one of them and nowhere else.
fn fits_only_accessories(g: &dyn GameView, item: ObjectId, shown: &[usize]) -> bool {
    let hidden = shown.iter().fold(0, |m, &k| m | ACCESSORIES[k].1);
    g.item_valid_locations(item)
        .is_some_and(|valid| valid & hidden != 0 && valid & !hidden == 0)
}

/// One equipment slot's picture and its item: the slot's own picture empty, the filled slot's
/// ground under a worn item, and the item over it as a control `id` that takes drops for `mask`.
fn equipment_slot(
    f: &mut PanelFrame,
    g: &dyn GameView,
    id: String,
    r: crate::widgets::Rect,
    did: u32,
    mask: u32,
    overlay: bool,
) {
    let equipped = worn(g, mask);
    let mut picture = PanelFrame::new(0, 0);
    image(
        &mut picture,
        if equipped.is_some() { 0x060011f9 } else { did },
        r,
        None,
        false,
        false,
    );
    if overlay {
        f.overlay.extend(picture.screen.commands);
    } else {
        f.screen.commands.extend(picture.screen.commands);
    }
    grid(
        f,
        g,
        &id,
        r,
        &equipped.into_iter().collect::<Vec<_>>(),
        1,
        32,
    );
    let control = f.controls.last_mut().unwrap();
    control.drop_location = Some(mask);
    control.overlay = overlay;
    if let ControlKind::Items { entries, .. } = &mut control.kind {
        if entries.is_empty() {
            entries.push(ItemEntry::empty());
        }
    }
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
    height: Option<u32>,
    item_scroll: std::cell::Cell<i32>,
    container_scroll: std::cell::Cell<i32>,
    displayed_container: std::cell::Cell<Option<ObjectId>>,
    flyout: Flyout,
}
impl Inventory {
    fn container(&self, g: &dyn GameView) -> Option<ObjectId> {
        g.open_inventory_container()
    }
    /// The accessories button, lit while a slot behind it holds an item and marked while a
    /// dragged item fits only those slots, and the flyout over the doll when it is open.
    fn accessories(&self, f: &mut PanelFrame, g: &dyn GameView) {
        let shown = accessories(g);
        if shown.is_empty() || g.player().is_none() {
            return;
        }
        let filled = shown.iter().any(|&k| worn(g, ACCESSORIES[k].1).is_some());
        image(
            f,
            crate::composed::ACCESSORIES_BUTTON[usize::from(filled)],
            BUTTON,
            None,
            false,
            false,
        );
        if !self.flyout.open
            && self
                .flyout
                .dragged
                .is_some_and(|item| fits_only_accessories(g, item, &shown))
        {
            image(f, 0x060011f9, BUTTON, None, false, true);
        }
        // The button lies over the doll's picture, as the flyout does, so a click on it is not
        // a click on the doll.
        let button = f.button("accessories-button", BUTTON, "", true);
        button.paint = false;
        button.overlay = true;
        let Some(geometry) = self
            .flyout
            .open
            .then(|| flyout_geometry(FLYOUT_LAYOUT, &shown))
            .flatten()
        else {
            return;
        };
        let p = geometry.panel;
        let mut layer = PanelFrame::new(0, 0);
        // The inventory's own dark ground, framed as the container strips are.
        image(&mut layer, 0x06000518, p, None, true, false);
        for (did, r) in [
            (0x060011b9, rect(p.x, p.y, p.w, 3)),
            (0x060011b9, rect(p.x, p.y + p.h - 3, p.w, 3)),
            (0x060011ba, rect(p.x, p.y, 3, p.h)),
            (0x060011ba, rect(p.x + p.w - 3, p.y, 3, p.h)),
        ] {
            image(&mut layer, did, r, None, true, false);
        }
        text(
            &mut layer,
            geometry.title,
            "Accessories",
            "14-6",
            CREAM,
            0,
            false,
            None,
        );
        // The collapse arrow: a small cream triangle pointing down, the way the flyout folds.
        let c = geometry.collapse;
        for row in 0..5 {
            layer.fill(rect(c.x + 1 + row, c.y + 3 + row, 9 - 2 * row, 1), CREAM);
        }
        f.overlay.extend(layer.screen.commands);
        // The panel takes the pointer and drops over the doll and its slots beneath it.
        let back = f.button("accessories", p, "", true);
        back.paint = false;
        back.overlay = true;
        let collapse = f.button("accessories-collapse", c, "", true);
        collapse.paint = false;
        collapse.overlay = true;
        for (k, r) in geometry.slots {
            let (did, mask) = ACCESSORIES[k];
            equipment_slot(f, g, format!("accessory:{k}"), r, did, mask, true);
        }
    }
    /// What the accessories button and flyout make of an event; `None` leaves it to the rest of
    /// the panel.
    fn flyout_event(&mut self, e: &ControlEvent, ctx: &Context<'_>) -> Option<Vec<PanelAction>> {
        let g = ctx.game;
        let shown = accessories(g);
        let geometry = flyout_geometry(FLYOUT_LAYOUT, &shown);
        let over_flyout = |x: i32, y: i32| {
            BUTTON.contains(x, y) || geometry.as_ref().is_some_and(|g| g.panel.contains(x, y))
        };
        match e {
            ControlEvent::Tick if shown.is_empty() => self.flyout.close(),
            ControlEvent::Activate(id) if id == "accessories-button" => {
                if self.flyout.open {
                    self.flyout.close();
                } else if !shown.is_empty() {
                    self.flyout.open = true;
                    self.flyout.sprung = false;
                }
                return Some(vec![]);
            }
            ControlEvent::Activate(id) if id == "accessories-collapse" => {
                self.flyout.close();
                return Some(vec![]);
            }
            // Escape (and a close the panel is told of) closes the flyout first.
            ControlEvent::Activate(id) if id == "close" && self.flyout.open => {
                self.flyout.close();
                return Some(vec![]);
            }
            // A press anywhere else on the panel closes the flyout, and goes on to what it hit.
            ControlEvent::Pointer {
                x,
                y,
                pressed: true,
            } if self.flyout.open && !over_flyout(*x, *y) => self.flyout.close(),
            ControlEvent::DragOver { object: None, .. } => {
                if self.flyout.sprung {
                    self.flyout.close();
                }
                self.flyout.hover_since = None;
                self.flyout.dragged = None;
            }
            ControlEvent::DragOver {
                object: Some(item),
                at,
            } => {
                self.flyout.dragged = Some(*item);
                if !self.flyout.open {
                    // Held over the closed button long enough, the drag opens the flyout.
                    if !shown.is_empty() && at.is_some_and(|(x, y)| BUTTON.contains(x, y)) {
                        let since = *self.flyout.hover_since.get_or_insert(ctx.now);
                        if crate::clock::seconds(ctx.now, since) >= SPRING_SECONDS {
                            self.flyout.open = true;
                            self.flyout.sprung = true;
                            self.flyout.hover_since = None;
                        }
                    } else {
                        self.flyout.hover_since = None;
                    }
                } else if self.flyout.sprung && !at.is_some_and(|(x, y)| over_flyout(x, y)) {
                    self.flyout.close();
                }
            }
            // A drop in the flyout keeps it open, however it was opened.
            ControlEvent::Drop { id, .. }
                if id == "accessories" || id.starts_with("accessory:") =>
            {
                self.flyout.sprung = false;
                if id == "accessories" {
                    return Some(vec![]);
                }
            }
            // An item let go on the button goes where it fits, as on the doll.
            ControlEvent::Drop {
                id,
                payload: DragPayload::Object(item),
                ..
            } if id == "accessories-button" => {
                return Some(vec![PanelAction::Game(UiRequest::DragDrop {
                    item: *item,
                    target: DropTarget::EquipCanvas,
                })]);
            }
            _ => {}
        }
        None
    }
}
impl Panel for Inventory {
    fn resize(&mut self, _width: u32, height: u32) {
        self.height = Some(height.max(362));
    }
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
        let height = self.height.unwrap_or_else(|| ctx.layout.side_height()) as i32;
        let grid_height = (height - 266) / 32 * 32;
        let container_extent = g.player().map_or(0, |player| {
            i32_from(slots(g, player, true, 1, 7).len()) * 36
        });
        let item_extent = self.container(g).map_or(0, |container| {
            i32_from(
                slots(g, container, false, 6, (grid_height / 32 * 6) as usize)
                    .len()
                    .div_ceil(6),
            ) * 32
        });
        self.container_scroll.set(
            self.container_scroll
                .get()
                .clamp(0, (container_extent - 252).max(0)),
        );
        self.item_scroll.set(
            self.item_scroll
                .get()
                .clamp(0, (item_extent - grid_height).max(0)),
        );
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
            for (i, (x, y, did, mask, _slot)) in EQUIPMENT.iter().enumerate() {
                equipment_slot(
                    &mut f,
                    g,
                    format!("equip:{i}"),
                    rect(*x, *y, 32, 32),
                    *did,
                    *mask,
                    false,
                );
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
                        .skip((self.container_scroll.get().max(0) / 36) as usize)
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
        if container_extent > 252 {
            scrollbar(
                &mut f,
                "containers-scroll",
                rect(283, 82, 16, 256),
                container_extent,
                252,
                self.container_scroll.get(),
                36,
                true,
            );
        }
        if item_extent > grid_height {
            scrollbar(
                &mut f,
                "items-scroll",
                rect(207, 257, 20, grid_height),
                item_extent,
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
        self.accessories(&mut f, g);
        mark_active(&mut f, self.container(g));
        f
    }
    fn dismiss(&mut self) -> bool {
        let open = self.flyout.open;
        self.flyout.close();
        open
    }
    fn event(&mut self, e: ControlEvent, ctx: &Context<'_>) -> Vec<PanelAction> {
        let g = ctx.game;
        if let Some(actions) = self.flyout_event(&e, ctx) {
            return actions;
        }
        let drag = matches!(e, ControlEvent::DragStart { .. });
        let double = matches!(e, ControlEvent::DoubleClick { .. });
        let right = matches!(e, ControlEvent::RightClick { .. });
        match e {
            ControlEvent::PreviewDrag { equipment_mask } => {
                return picked_equipment(ctx, equipment_mask)
                    .filter(|object| Some(*object) != g.player())
                    .map(|object| vec![PanelAction::BeginDrag(DragPayload::Object(object))])
                    .unwrap_or_default();
            }
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
                self.container_scroll.set(value.max(0))
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
                            .get(index + (self.container_scroll.get().max(0) / 36) as usize)
                            .copied()
                    })
                } else if id == "backpack" {
                    g.player()
                } else if let Some((mask, _)) = slot_location(&id) {
                    worn(g, mask)
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
                        return if id == "items" || slot_location(&id).is_some() {
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
                if let Some((mask, side)) = slot_location(&id) {
                    return vec![PanelAction::Game(UiRequest::DragDrop {
                        item,
                        target: DropTarget::EquipLocation {
                            mask,
                            side: u32_from(side),
                        },
                    })];
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
                                (self.container_scroll.get().max(0) / 36) as u32
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
            resources: &crate::resources::Resources::default(),
            layout: crate::panels::Layout::default(),
            now: dereth_primitives::LocalTime(0.0),
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
mod accessories_tests {
    //! Behaviour: none (the classic paper doll's accessories flyout; see each test's anchor).
    use super::*;
    use crate::control_host::ControlHost;
    use dereth_client_contract::view::EquipmentHover;

    const PLAYER: ObjectId = ObjectId(1);
    const WORN: ObjectId = ObjectId(9);
    const CARRIED: ObjectId = ObjectId(50);

    /// A character on a world: the world's era (none: the end of retail), the sigils unlocked,
    /// what is worn, where the carried item may go, and where the equipment rules accept it.
    #[derive(Debug, Default)]
    struct Doll {
        era: Option<dereth_client_contract::EraView>,
        unlocks: i32,
        worn: Vec<(ObjectId, u32)>,
        valid: u32,
        accepted: u32,
    }
    impl GameView for Doll {
        fn era(&self) -> Option<&dereth_client_contract::EraView> {
            self.era.as_ref()
        }
        fn player(&self) -> Option<ObjectId> {
            Some(PLAYER)
        }
        fn int_stat(&self, id: ObjectId, prop: u32) -> Option<i32> {
            (id == PLAYER && prop == 0x142).then_some(self.unlocks)
        }
        fn equipment(&self, _: ObjectId) -> &[(ObjectId, u32)] {
            &self.worn
        }
        fn item_valid_locations(&self, _: ObjectId) -> Option<u32> {
            Some(self.valid)
        }
        fn equipment_hover(&self, _: ObjectId) -> EquipmentHover {
            EquipmentHover {
                slot_mask: Some(self.accepted),
                canvas: Some(false),
            }
        }
    }
    fn infiltration(features: &[&str]) -> dereth_client_contract::EraView {
        let mut era = dereth_client_contract::EraView::default();
        era.era = dereth_primitives::era::EraId::Infiltration;
        era.era_announced = true;
        for feature in features {
            era.announced_features.set(feature, true);
        }
        era
    }
    fn at<R>(game: &dyn GameView, now: f64, f: impl FnOnce(&Context<'_>) -> R) -> R {
        f(&Context {
            resources: &crate::resources::Resources::default(),
            layout: crate::panels::Layout::default(),
            now: dereth_primitives::LocalTime(now),
            game,
            pregame: &Default::default(),
            keyboard: &Default::default(),
            settings: &Default::default(),
            classic: &Default::default(),
            map_teleport_allowed: false,
        })
    }
    fn frame(panel: &Inventory, game: &dyn GameView) -> PanelFrame {
        at(game, 0.0, |c| panel.frame(c))
    }
    fn send(
        panel: &mut Inventory,
        game: &dyn GameView,
        now: f64,
        e: ControlEvent,
    ) -> Vec<PanelAction> {
        at(game, now, |c| panel.event(e, c))
    }
    fn control<'f>(f: &'f PanelFrame, id: &str) -> Option<&'f Control> {
        f.controls.iter().find(|c| c.id == id)
    }
    fn shown_slots(f: &PanelFrame) -> Vec<String> {
        f.controls
            .iter()
            .filter(|c| c.id.starts_with("accessory:"))
            .map(|c| c.id.clone())
            .collect()
    }
    fn centre(r: crate::widgets::Rect) -> (i32, i32) {
        (r.x + r.w / 2, r.y + r.h / 2)
    }
    fn hover(object: Option<ObjectId>, at: Option<(i32, i32)>) -> ControlEvent {
        ControlEvent::DragOver { object, at }
    }
    fn images(f: &PanelFrame) -> Vec<(String, i32, i32)> {
        f.screen
            .commands
            .iter()
            .chain(&f.overlay)
            .filter_map(|c| match c {
                crate::Command::Image { did, x, y, .. } => Some((did.clone(), *x, *y)),
                _ => None,
            })
            .collect()
    }
    fn opened(game: &Doll) -> Inventory {
        let mut panel = Inventory::default();
        send(
            &mut panel,
            game,
            0.0,
            ControlEvent::Activate("accessories-button".into()),
        );
        assert!(panel.flyout.open);
        panel
    }

    /// Behaviour: classic.paper-doll.accessories-follow-the-world-and-the-unlocks
    #[test]
    fn the_flyout_has_the_worlds_slots_and_each_sigil_the_character_has_unlocked() {
        // The end of retail has the cloak and the trinket; the sigils follow the unlock bits.
        for (bits, sigils) in [
            (0, vec![]),
            (1, vec![2]),
            (3, vec![2, 3]),
            (7, vec![2, 3, 4]),
        ] {
            let game = Doll {
                unlocks: bits,
                ..Doll::default()
            };
            let mut expected = vec![0, 1];
            expected.extend(sigils);
            assert_eq!(accessories(&game), expected, "unlocks {bits}");
        }
        // Only the unlock bits count.
        let game = Doll {
            unlocks: 0x18,
            ..Doll::default()
        };
        assert_eq!(accessories(&game), [0, 1]);
        // Infiltration has none, so the doll has no button; the systems a server announces
        // bring theirs, and its sigils still wait on the unlocks.
        let game = Doll {
            era: Some(infiltration(&[])),
            unlocks: 7,
            ..Doll::default()
        };
        assert!(accessories(&game).is_empty());
        assert!(control(&frame(&Inventory::default(), &game), "accessories-button").is_none());
        let mut game = Doll {
            era: Some(infiltration(&["cloaks", "aetheria"])),
            ..Doll::default()
        };
        assert_eq!(accessories(&game), [0]);
        game.unlocks = 2;
        assert_eq!(accessories(&game), [0, 3]);
        assert!(control(&frame(&Inventory::default(), &game), "accessories-button").is_some());
        // The classic slots stay on the doll and the later ones are only in the flyout.
        let f = frame(&Inventory::default(), &game);
        let doll_slots = f
            .controls
            .iter()
            .filter(|c| c.id.starts_with("equip:"))
            .count();
        assert_eq!(doll_slots, EQUIPMENT.len());
        assert!(shown_slots(&f).is_empty());
    }

    /// Behaviour: classic.paper-doll.accessories-follow-the-world-and-the-unlocks
    #[test]
    fn a_sigil_unlocked_while_the_flyout_is_open_appears_in_it_at_once() {
        let mut game = Doll {
            unlocks: 0,
            ..Doll::default()
        };
        let mut panel = opened(&game);
        assert_eq!(
            shown_slots(&frame(&panel, &game)),
            ["accessory:0", "accessory:1"]
        );
        game.unlocks = 1;
        assert_eq!(
            shown_slots(&frame(&panel, &game)),
            ["accessory:0", "accessory:1", "accessory:2"]
        );
        // A world with none of them closes the flyout and hides the button.
        game.era = Some(infiltration(&[]));
        send(&mut panel, &game, 0.0, ControlEvent::Tick);
        assert!(!panel.flyout.open);
        let f = frame(&panel, &game);
        assert!(control(&f, "accessories-button").is_none() && shown_slots(&f).is_empty());
    }

    /// Behaviour: classic.paper-doll.accessories-flyout-opens-and-closes
    #[test]
    fn the_button_toggles_the_flyout_and_a_press_elsewhere_or_escape_closes_it() {
        let game = Doll {
            unlocks: 7,
            ..Doll::default()
        };
        let mut panel = opened(&game);
        let geometry = flyout_geometry(FLYOUT_LAYOUT, &accessories(&game)).unwrap();
        let f = frame(&panel, &game);
        assert_eq!(
            control(&f, "accessories").map(|c| c.rect),
            Some(geometry.panel)
        );
        assert_eq!(shown_slots(&f).len(), 5);
        // The button again closes it.
        send(
            &mut panel,
            &game,
            0.0,
            ControlEvent::Activate("accessories-button".into()),
        );
        assert!(!panel.flyout.open);
        assert!(control(&frame(&panel, &game), "accessories").is_none());
        // A press inside the flyout or on its button keeps it; anywhere else on the panel
        // closes it, and the press goes on to what it hit.
        let mut panel = opened(&game);
        for (x, y) in [centre(geometry.panel), centre(BUTTON)] {
            assert!(send(
                &mut panel,
                &game,
                0.0,
                ControlEvent::Pointer {
                    x,
                    y,
                    pressed: true
                }
            )
            .is_empty());
            assert!(panel.flyout.open);
        }
        // A release elsewhere is not a press.
        send(
            &mut panel,
            &game,
            0.0,
            ControlEvent::Pointer {
                x: 260,
                y: 300,
                pressed: false,
            },
        );
        assert!(panel.flyout.open);
        send(
            &mut panel,
            &game,
            0.0,
            ControlEvent::Pointer {
                x: 260,
                y: 300,
                pressed: true,
            },
        );
        assert!(!panel.flyout.open);
        // The collapse arrow closes it.
        let mut panel = opened(&game);
        send(
            &mut panel,
            &game,
            0.0,
            ControlEvent::Activate("accessories-collapse".into()),
        );
        assert!(!panel.flyout.open);
        // Escape closes the flyout before the panel: first the flyout, then nothing to close.
        let mut panel = opened(&game);
        assert!(panel.dismiss());
        assert!(!panel.flyout.open && !panel.dismiss());
        // The panel's close, told while the flyout is open, closes only the flyout; told again,
        // the panel.
        let mut panel = opened(&game);
        assert!(send(
            &mut panel,
            &game,
            0.0,
            ControlEvent::Activate("close".into())
        )
        .is_empty());
        assert!(!panel.flyout.open);
        assert_eq!(
            send(
                &mut panel,
                &game,
                0.0,
                ControlEvent::Activate("close".into())
            ),
            [PanelAction::Close]
        );
    }

    /// Behaviour: classic.paper-doll.accessories-flyout-springs-open-under-a-held-item
    #[test]
    fn an_item_held_over_the_closed_button_opens_the_flyout_after_the_delay() {
        let game = Doll {
            unlocks: 7,
            ..Doll::default()
        };
        let geometry = flyout_geometry(FLYOUT_LAYOUT, &accessories(&game)).unwrap();
        let button = Some(centre(BUTTON));
        let outside = Some((260, 300));
        let mut panel = Inventory::default();
        send(&mut panel, &game, 10.0, hover(Some(CARRIED), button));
        send(&mut panel, &game, 10.3, hover(Some(CARRIED), button));
        assert!(!panel.flyout.open, "not before the delay");
        // Leaving restarts the delay.
        send(&mut panel, &game, 10.35, hover(Some(CARRIED), outside));
        send(&mut panel, &game, 10.5, hover(Some(CARRIED), button));
        send(&mut panel, &game, 10.85, hover(Some(CARRIED), button));
        assert!(!panel.flyout.open, "the delay counts from coming back");
        send(&mut panel, &game, 10.91, hover(Some(CARRIED), button));
        assert!(
            panel.flyout.open,
            "open once the item has been held there 400 ms"
        );
        // Moving into the flyout keeps it; leaving it without dropping closes it.
        send(
            &mut panel,
            &game,
            11.0,
            hover(Some(CARRIED), Some(centre(geometry.panel))),
        );
        assert!(panel.flyout.open);
        send(&mut panel, &game, 11.1, hover(Some(CARRIED), outside));
        assert!(!panel.flyout.open);
        // A drag that springs it open and ends elsewhere closes it.
        let mut panel = Inventory::default();
        send(&mut panel, &game, 0.0, hover(Some(CARRIED), button));
        send(&mut panel, &game, 0.5, hover(Some(CARRIED), button));
        assert!(panel.flyout.open);
        send(&mut panel, &game, 0.6, hover(None, None));
        assert!(!panel.flyout.open);
        // A drop in it keeps it open after the drag.
        let mut panel = Inventory::default();
        send(&mut panel, &game, 0.0, hover(Some(CARRIED), button));
        send(&mut panel, &game, 0.5, hover(Some(CARRIED), button));
        let drop = ControlEvent::Drop {
            id: "accessory:0".into(),
            payload: DragPayload::Object(CARRIED),
            slot: 0,
        };
        assert_eq!(send(&mut panel, &game, 0.6, drop).len(), 1);
        send(&mut panel, &game, 0.7, hover(None, None));
        assert!(panel.flyout.open);
        // Open before the drag (a drag out of it, or one passing over), it stays open.
        let mut panel = opened(&game);
        send(&mut panel, &game, 0.0, hover(Some(WORN), outside));
        send(&mut panel, &game, 0.1, hover(Some(WORN), None));
        send(&mut panel, &game, 0.2, hover(None, None));
        assert!(panel.flyout.open);
        // With no accessories there is no button to hold an item over.
        let none = Doll {
            era: Some(infiltration(&[])),
            ..Doll::default()
        };
        let mut panel = Inventory::default();
        send(&mut panel, &none, 0.0, hover(Some(CARRIED), button));
        send(&mut panel, &none, 1.0, hover(Some(CARRIED), button));
        assert!(!panel.flyout.open);
    }

    /// Behaviour: classic.paper-doll.accessories-flyout-springs-open-under-a-held-item
    #[test]
    fn the_desktop_tells_the_inventory_of_a_drag_and_escape_closes_the_flyout_before_the_panel() {
        use crate::desktop::Desktop;
        use crate::widgets::Input;
        let game = Doll {
            unlocks: 7,
            ..Doll::default()
        };
        let geometry = flyout_geometry(FLYOUT_LAYOUT, &accessories(&game)).unwrap();
        let cloak = centre(geometry.slots[0].1);
        let mut d = Desktop::new(crate::panels::factory, (800, 600));
        at(&game, 0.0, |c| d.open("inventory", c));
        d.set_position("inventory", 0, 0);
        let panel = centre(geometry.panel);
        // An item carried from anywhere (the desktop holds the drag) and held over the button.
        d.drag_payload = Some(DragPayload::Object(CARRIED));
        at(&game, 1.0, |c| d.drag_over(Some(centre(BUTTON)), c));
        assert!(!d.overlay_at(panel.0, panel.1));
        at(&game, 1.5, |c| d.drag_over(Some(centre(BUTTON)), c));
        assert!(d.overlay_at(panel.0, panel.1), "sprung open");
        // Let go on the cloak's slot: the shared equip request, and the flyout stays open.
        at(&game, 1.6, |c| d.drag_over(Some(cloak), c));
        at(&game, 1.6, |c| {
            d.input(
                Input::PointerUp {
                    x: cloak.0,
                    y: cloak.1,
                },
                c,
            )
        });
        assert_eq!(
            d.requests,
            [UiRequest::DragDrop {
                item: CARRIED,
                target: DropTarget::EquipLocation {
                    mask: loc::CLOAK,
                    side: 0
                }
            }]
        );
        at(&game, 1.7, |c| d.drag_over(Some(cloak), c));
        assert!(d.overlay_at(panel.0, panel.1));
        // Escape closes the flyout, and only the flyout.
        assert!(at(&game, 2.0, |c| d.dismiss(c)));
        assert!(!d.overlay_at(panel.0, panel.1) && d.is_visible("inventory"));
        assert!(!at(&game, 2.0, |c| d.dismiss(c)));
        // Sprung open and carried away, it closes again.
        d.drag_payload = Some(DragPayload::Object(CARRIED));
        at(&game, 3.0, |c| d.drag_over(Some(centre(BUTTON)), c));
        at(&game, 3.5, |c| d.drag_over(Some(centre(BUTTON)), c));
        assert!(d.overlay_at(panel.0, panel.1));
        at(&game, 3.6, |c| d.drag_over(Some((600, 100)), c));
        assert!(!d.overlay_at(panel.0, panel.1));
    }

    /// Behaviour: classic.paper-doll.accessories-button-shows-what-is-behind-it
    #[test]
    fn the_button_is_lit_while_a_hidden_slot_holds_an_item_and_marked_for_an_item_only_it_takes() {
        let button = |f: &PanelFrame| {
            images(f)
                .into_iter()
                .filter(|(_, x, y)| (*x, *y) == (BUTTON.x, BUTTON.y))
                .map(|(did, _, _)| did)
                .collect::<Vec<_>>()
        };
        let plain = format!("{:08X}", crate::composed::ACCESSORIES_BUTTON[0]);
        let lit = format!("{:08X}", crate::composed::ACCESSORIES_BUTTON[1]);
        let mut game = Doll {
            unlocks: 1,
            ..Doll::default()
        };
        assert_eq!(
            button(&frame(&Inventory::default(), &game)),
            std::slice::from_ref(&plain)
        );
        // Something worn on the doll itself does not light it.
        game.worn = vec![(WORN, loc::NECK_WEAR)];
        assert_eq!(
            button(&frame(&Inventory::default(), &game)),
            std::slice::from_ref(&plain)
        );
        for mask in [loc::CLOAK, loc::TRINKET_ONE, loc::SIGIL_ONE] {
            game.worn = vec![(WORN, mask)];
            assert_eq!(
                button(&frame(&Inventory::default(), &game)),
                std::slice::from_ref(&lit),
                "{mask:#x}"
            );
        }
        // A sigil worn in a slot the character has not unlocked is not one of the flyout's.
        game.worn = vec![(WORN, loc::SIGIL_TWO)];
        assert_eq!(
            button(&frame(&Inventory::default(), &game)),
            std::slice::from_ref(&plain)
        );
        // Dragging an item that goes only in a flyout slot marks the closed button.
        game.worn.clear();
        let hint = "060011F9".to_owned();
        for (valid, marked) in [
            (loc::CLOAK, true),
            (loc::SIGIL_ONE, true),
            (loc::CLOAK | loc::NECK_WEAR, false),
            (loc::SIGIL_TWO, false),
            (loc::NECK_WEAR, false),
        ] {
            game.valid = valid;
            let mut panel = Inventory::default();
            send(
                &mut panel,
                &game,
                0.0,
                hover(Some(CARRIED), Some((260, 300))),
            );
            let expected = if marked {
                vec![plain.clone(), hint.clone()]
            } else {
                vec![plain.clone()]
            };
            assert_eq!(button(&frame(&panel, &game)), expected, "{valid:#x}");
            send(&mut panel, &game, 0.1, hover(None, None));
            assert_eq!(button(&frame(&panel, &game)), std::slice::from_ref(&plain));
        }
    }

    /// Behaviour: classic.paper-doll.accessory-slots-work-as-the-doll-slots
    #[test]
    fn a_flyout_slot_shows_whether_it_takes_the_carried_item_as_the_doll_slots_do() {
        for (accepted, hint) in [(loc::CLOAK, "060011F9"), (loc::NECK_WEAR, "060011F8")] {
            let game = Doll {
                unlocks: 7,
                accepted,
                ..Doll::default()
            };
            let panel = opened(&game);
            let f = frame(&panel, &game);
            let cloak = control(&f, "accessory:0").unwrap().rect;
            let mut host = ControlHost::default();
            host.sync(&f);
            host.update_item_drop_preview(&game, Some(CARRIED), Some(centre(cloak)), true, 0);
            let drawn = host.draw(&f);
            assert!(
                drawn.commands.iter().any(|c| matches!(c,
                    crate::Command::Image { did, x, y, .. }
                        if did == hint && (*x, *y) == (cloak.x, cloak.y))),
                "{accepted:#x}"
            );
        }
    }

    /// Behaviour: classic.paper-doll.accessory-slots-work-as-the-doll-slots
    #[test]
    fn a_flyout_slot_drags_out_unequips_examines_and_takes_drops_as_a_doll_slot() {
        for (k, (_, mask)) in ACCESSORIES.iter().enumerate() {
            let game = Doll {
                unlocks: 7,
                worn: vec![(WORN, *mask)],
                ..Doll::default()
            };
            let mut panel = opened(&game);
            let id = format!("accessory:{k}");
            let index = 0;
            assert_eq!(
                send(
                    &mut panel,
                    &game,
                    0.0,
                    ControlEvent::DragStart {
                        id: id.clone(),
                        index
                    }
                ),
                [PanelAction::BeginDrag(DragPayload::Object(WORN))]
            );
            assert_eq!(
                send(
                    &mut panel,
                    &game,
                    0.0,
                    ControlEvent::DoubleClick {
                        id: id.clone(),
                        index
                    }
                ),
                [PanelAction::Game(UiRequest::Use(WORN))]
            );
            assert_eq!(
                send(
                    &mut panel,
                    &game,
                    0.0,
                    ControlEvent::Select {
                        id: id.clone(),
                        index
                    }
                ),
                [PanelAction::Game(UiRequest::Select(WORN))]
            );
            assert_eq!(
                send(
                    &mut panel,
                    &game,
                    0.0,
                    ControlEvent::RightClick {
                        id: id.clone(),
                        index
                    }
                )
                .len(),
                2,
                "a right click selects and examines"
            );
            let drop = ControlEvent::Drop {
                id,
                payload: DragPayload::Object(CARRIED),
                slot: 0,
            };
            assert_eq!(
                send(&mut panel, &game, 0.0, drop),
                [PanelAction::Game(UiRequest::DragDrop {
                    item: CARRIED,
                    target: DropTarget::EquipLocation {
                        mask: *mask,
                        side: 0
                    },
                })]
            );
            // The slot's item is under the pointer, for its name.
            let f = frame(&panel, &game);
            let mut host = ControlHost::default();
            host.sync(&f);
            let (x, y) = centre(control(&f, &format!("accessory:{k}")).unwrap().rect);
            assert_eq!(host.item_at(x, y), Some(WORN));
        }
        // An item let go on the button goes where it fits, as on the doll; on the flyout's
        // own panel, nowhere.
        let game = Doll {
            unlocks: 7,
            ..Doll::default()
        };
        let mut panel = opened(&game);
        let drop = |id: &str| ControlEvent::Drop {
            id: id.into(),
            payload: DragPayload::Object(CARRIED),
            slot: 0,
        };
        assert_eq!(
            send(&mut panel, &game, 0.0, drop("accessories-button")),
            [PanelAction::Game(UiRequest::DragDrop {
                item: CARRIED,
                target: DropTarget::EquipCanvas,
            })]
        );
        assert!(send(&mut panel, &game, 0.0, drop("accessories")).is_empty());
    }

    /// Behaviour: classic.paper-doll.accessories-flyout-opens-and-closes
    #[test]
    fn the_open_flyout_lies_over_the_doll_and_takes_the_pointer_from_what_it_covers() {
        let game = Doll {
            unlocks: 7,
            worn: vec![(WORN, loc::FINGER_WEAR_RIGHT), (ObjectId(10), loc::CLOAK)],
            ..Doll::default()
        };
        let panel = opened(&game);
        let f = frame(&panel, &game);
        let geometry = flyout_geometry(FLYOUT_LAYOUT, &accessories(&game)).unwrap();
        let mut host = ControlHost::default();
        host.sync(&f);
        // Over the flyout's panel, the button and its slots the overlay has the pointer; the
        // doll beside it is the doll's.
        assert!(host.overlay_at(geometry.panel.x + 4, geometry.panel.y + 4));
        assert!(host.overlay_at(centre(BUTTON).0, centre(BUTTON).1));
        assert!(!host.overlay_at(100, 40));
        // A drop over a doll slot the flyout covers lands in the flyout.
        for (x, y, _, _, _) in EQUIPMENT {
            let slot = rect(x, y, 32, 32);
            let Some(cover) = slot.intersect(geometry.panel) else {
                continue;
            };
            let Some(ControlEvent::Drop { id, .. }) =
                host.drop_event(cover.x, cover.y, DragPayload::Object(CARRIED))
            else {
                panic!("a drop")
            };
            assert!(id == "accessories" || id.starts_with("accessory:"), "{id}");
        }
        // The flyout is drawn over every doll slot's item: its panel comes after them.
        let drawn = host.draw(&f);
        let first =
            |pred: &dyn Fn(&crate::Command) -> bool| drawn.commands.iter().position(pred).unwrap();
        let panel_art = first(&|c| {
            matches!(c, crate::Command::Image { did, x, y, .. }
                if did == "06000518" && (*x, *y) == (geometry.panel.x, geometry.panel.y))
        });
        let icons = |commands: &[crate::Command]| {
            commands
                .iter()
                .filter(|c| matches!(c, crate::Command::ItemIcon { .. }))
                .count()
        };
        assert!(
            icons(&drawn.commands[..panel_art]) >= 1,
            "the ring's, under the flyout"
        );
        assert_eq!(
            icons(&drawn.commands[panel_art..]),
            1,
            "the cloak's, in the flyout"
        );
    }

    /// Behaviour: classic.paper-doll.accessories-flyout-opens-and-closes
    #[test]
    fn both_layouts_stand_the_flyout_on_the_button_over_the_doll() {
        let all = [0, 1, 2, 3, 4];
        for layout in [FlyoutLayout::Row, FlyoutLayout::Grid] {
            for shown in [&all[..], &[0, 1], &[2], &[0, 3]] {
                let g = flyout_geometry(layout, shown).unwrap();
                let p = g.panel;
                assert_eq!(p.y + p.h, BUTTON.y - 2, "{layout:?} stands on the button");
                assert!(
                    p.x >= 2 && p.x + p.w <= 244 && p.y >= 23,
                    "{layout:?} on the doll"
                );
                assert_eq!(
                    g.slots
                        .iter()
                        .map(|(k, _)| *k)
                        .collect::<std::collections::BTreeSet<_>>(),
                    shown.iter().copied().collect(),
                );
                for (i, (_, r)) in g.slots.iter().enumerate() {
                    assert!(
                        r.intersect(p) == Some(*r),
                        "{layout:?} slot inside the panel"
                    );
                    assert!(
                        r.y >= g.title.y + g.title.h,
                        "{layout:?} slots under the title"
                    );
                    for (_, other) in &g.slots[i + 1..] {
                        assert!(r.intersect(*other).is_none(), "{layout:?} slots apart");
                    }
                }
                assert!(g.collapse.intersect(p) == Some(g.collapse));
            }
        }
        // The row is one row; the grid puts the sigils over the cloak and the trinket.
        let row = flyout_geometry(FlyoutLayout::Row, &all).unwrap();
        assert!(row.slots.iter().all(|(_, r)| r.y == row.slots[0].1.y));
        let grid = flyout_geometry(FlyoutLayout::Grid, &all).unwrap();
        let y = |k: usize| grid.slots.iter().find(|(s, _)| *s == k).unwrap().1.y;
        assert!(y(2) == y(3) && y(3) == y(4) && y(0) == y(1) && y(2) < y(0));
        assert!(flyout_geometry(FlyoutLayout::Row, &[]).is_none());
    }
}

#[cfg(test)]
mod fitting_scroll_tests {
    //! Behaviour: none (inventory control layout and scroll projection).
    use super::*;
    #[derive(Debug)]
    struct Contents {
        items: i32,
        packs: i32,
    }
    impl GameView for Contents {
        fn player(&self) -> Option<ObjectId> {
            Some(ObjectId(1))
        }
        fn open_inventory_container(&self) -> Option<ObjectId> {
            self.player()
        }
        fn items_capacity(&self, _: ObjectId) -> Option<i32> {
            Some(self.items)
        }
        fn containers_capacity(&self, _: ObjectId) -> Option<i32> {
            Some(self.packs)
        }
    }
    /// Behaviour: classic.inventory.hides-fitting-scrollbars
    #[test]
    fn inventory_bars_follow_capacity_and_stretch_and_clamp_the_visible_origin() {
        let mut game = Contents {
            items: 120,
            packs: 10,
        };
        let mut panel = Inventory::default();
        let frame = |game: &Contents, panel: &Inventory| {
            panel.frame(&Context {
                resources: &crate::resources::Resources::default(),
                layout: crate::panels::Layout::default(),
                now: dereth_primitives::LocalTime(0.0),
                game,
                pregame: &Default::default(),
                keyboard: &Default::default(),
                settings: &Default::default(),
                classic: &Default::default(),
                map_teleport_allowed: false,
            })
        };
        panel.resize(300, 362);
        let initial = frame(&game, &panel);
        for id in ["items-scroll", "containers-scroll"] {
            assert!(initial.controls.iter().any(|c| c.id == id));
        }
        panel.item_scroll.set(544);
        panel.container_scroll.set(108);
        game.items = 18;
        game.packs = 7;
        let fits = frame(&game, &panel);
        assert!(!fits.controls.iter().any(|c| c.id.ends_with("-scroll")));
        assert_eq!(
            (panel.item_scroll.get(), panel.container_scroll.get()),
            (0, 0)
        );
        game.items = 60;
        assert!(frame(&game, &panel)
            .controls
            .iter()
            .any(|c| c.id == "items-scroll"));
        panel.item_scroll.set(224);
        panel.resize(300, 618);
        let stretched = frame(&game, &panel);
        assert!(!stretched.controls.iter().any(|c| c.id == "items-scroll"));
        assert_eq!(panel.item_scroll.get(), 0);
        let items = stretched.controls.iter().find(|c| c.id == "items").unwrap();
        assert!(matches!(&items.kind, ControlKind::Items { entries, .. } if entries.len() == 60));
    }
}
