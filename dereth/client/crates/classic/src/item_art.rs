//! The classic interface's item pictures, independent of the modern enum mapper. Each object
//! gets three composed surfaces (its slot tile and two drag images), and the slot draws its
//! selection, shortcut and trade overlays and its structure or capacity meter over the tile.
use crate::int::i32_from;
use dereth_client_contract::view::SlotDecoration;
use dereth_primitives::num::to_i32_f64;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    Tile,
    Drag,
    AlternateDrag,
}

/// Composite order: background, underlay, icon, overlay, replace opaque white
/// pixels with corresponding effects pixels, then badge. Badge keeps native size.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Recipe {
    pub background: Option<u32>,
    pub underlay: Option<u32>,
    pub icon: Option<u32>,
    pub overlay: Option<u32>,
    pub effects: u32,
    pub badge: Option<u32>,
}

pub fn background(item_type: u32) -> u32 {
    // Empty rows in the 33-entry background table fall back to row zero.
    let low = item_type.trailing_zeros();
    match low {
        0 => 0x060011cb,
        1 => 0x060011cf,
        2 => 0x060011f3,
        3 => 0x060011d5,
        4 => 0x060011d1,
        5 => 0x060011cc,
        6 => 0x060011f4,
        8 => 0x060011d2,
        9 => 0x060011ce,
        10 => 0x060011d0,
        11 => 0x060011d3,
        12 => 0x060011cd,
        20 => 0x060013f6,
        _ => 0x060011d4,
    }
}

pub fn effect_surface(effects: u32) -> u32 {
    // The lowest set effect bit wins; two effects share a surface.
    const ART: [u32; 12] = [
        0x060011ca, 0x060011c6, 0x06001b05, 0x060011ca, 0x06001b06, 0x06001b2e, 0x06001b2d,
        0x06001b2f, 0x06001b2c, 0x060033c3, 0x060033c4, 0x060033c2,
    ];
    ART.get(effects.trailing_zeros() as usize)
        .copied()
        .unwrap_or(0x060011c5)
}

pub fn recipe(decoration: &SlotDecoration, surface: Surface) -> Recipe {
    let icon = if decoration.is_player {
        0x0600127e
    } else {
        decoration.icon_id
    };
    let item_type = if decoration.is_player {
        0x200
    } else {
        decoration.obj_type
    };
    let valid = icon != 0;
    Recipe {
        background: (valid && surface == Surface::Tile).then(|| background(item_type)),
        underlay: (valid && surface == Surface::Tile)
            .then_some(decoration.icon_underlay_id)
            .flatten()
            .map(|id| id.0)
            .filter(|id| *id != 0),
        icon: valid.then_some(icon),
        overlay: valid
            .then_some(decoration.icon_overlay_id)
            .flatten()
            .map(|id| id.0)
            .filter(|id| *id != 0),
        effects: effect_surface(decoration.effects),
        badge: if valid {
            match surface {
                Surface::Tile => None,
                Surface::Drag => Some(0x060011c4),
                Surface::AlternateDrag => Some(0x06001348),
            }
        } else {
            None
        },
    }
}

/// The slot paints these after the completed tile, in this order.
pub fn overlays(d: &SlotDecoration, selected: bool) -> Vec<u32> {
    let mut art = Vec::new();
    if d.waiting {
        art.push(0x06000f7d);
    }
    if selected {
        art.push(0x06000f7e);
    }
    if !d.sell_state && !d.trade_state {
        // The numeral is the key that uses the shortcut: the first slot is "1".
        if let Some(number) = d.shortcut_num.filter(|n| *n < 9) {
            art.push(
                if d.shortcut_ghosted {
                    0x06001acb
                } else {
                    0x0600109d
                } + number
                    + 1,
            );
        }
    }
    if d.sell_state {
        art.push(0x060012d9);
    }
    if d.trade_state {
        art.push(0x06001dae);
    }
    art
}

/// The meter's art is chosen when the slot is made, then structure takes ratio priority. The
/// native 5x30 images split at bottom + 1 - trunc(height * ratio).
pub fn meter(d: &SlotDecoration) -> Option<(u32, u32, i32)> {
    let capacity = d.is_container && d.items_capacity > 0;
    if !capacity && d.max_structure == 0 {
        return None;
    }
    let ratio = if d.max_structure != 0 {
        if d.structure == d.max_structure {
            return None;
        }
        f64::from(d.structure) / f64::from(d.max_structure)
    } else {
        if d.contained_items == 0 {
            return None;
        }
        f64::from(d.contained_items) / f64::from(d.items_capacity)
    };
    let (top, bottom) = if capacity {
        (0x06001216, 0x06001217)
    } else {
        (0x0600135e, 0x0600135f)
    };
    Some((
        top,
        bottom,
        to_i32_f64((30.0 - (30.0 * ratio).trunc()).clamp(0.0, 30.0)),
    ))
}

/// What the slot's drop feedback reads. Keep explicit widget mode separate from object qualities.
#[derive(Clone, Copy, Debug, Default)]
pub struct DropFacts {
    pub framed: bool,
    pub empty: bool,
    pub forced_mode: u32,
    pub dragged_container: bool,
    pub dragged_holds_containers: bool,
    pub target_holds_containers: bool,
    pub target_exists: bool,
    pub empty_item_slots: i32,
    pub empty_container_slots: i32,
    pub merge_possible: bool,
}
pub fn feedback(f: DropFacts) -> u32 {
    match f.forced_mode {
        4 | 6 => return 0x060011f9,
        5 | 7 => return 0x060011f8,
        _ => {}
    }
    if f.empty {
        return if (f.framed && f.dragged_container && !f.dragged_holds_containers)
            || (!f.framed && !f.dragged_container)
        {
            0x060011f9
        } else {
            0x060011f8
        };
    }
    if !f.framed {
        if !f.dragged_container {
            return 0x060011f9;
        }
    } else {
        if f.dragged_container && !f.dragged_holds_containers && !f.target_holds_containers {
            return 0x060011f9;
        }
        if !f.dragged_container {
            if f.target_exists && f.empty_item_slots != 0 {
                return 0x060011f7;
            }
        } else if !f.dragged_holds_containers && f.target_exists && f.empty_container_slots != 0 {
            return 0x060011f7;
        }
    }
    if f.merge_possible {
        0x060011f7
    } else {
        0x060011f8
    }
}
/// `request_ready` means neither an item request nor an attack is pending.
pub fn drop_feedback(
    g: &dyn dereth_client_contract::view::GameView,
    dragged: dereth_primitives::ObjectId,
    target: dereth_primitives::ObjectId,
    framed: bool,
    forced_mode: u32,
    request_ready: bool,
) -> u32 {
    let d = g.slot_decoration(dragged).unwrap_or_default();
    let t = g.slot_decoration(target);
    let remaining = |capacity: i32, count: usize| {
        if capacity == -1 {
            -1
        } else {
            capacity.saturating_sub(i32_from(count))
        }
    };
    let merge = request_ready
        && dragged != target
        && t.is_some()
        && g.int_stat(dragged, 0x0d).unwrap_or(0) > 1
        && g.int_stat(target, 0x0d).unwrap_or(0) > 1
        && !d.trade_state
        && !t.as_ref().is_some_and(|t| t.trade_state)
        && g.item_wcid(dragged) == g.item_wcid(target)
        && g.int_stat(target, 0x0c).unwrap_or(0) < g.int_stat(target, 0x0d).unwrap_or(0);
    feedback(DropFacts {
        framed,
        empty: target.0 == 0,
        forced_mode,
        dragged_container: d.is_container,
        dragged_holds_containers: d.containers_capacity != 0,
        target_holds_containers: t.as_ref().is_some_and(|t| t.containers_capacity != 0),
        target_exists: t.is_some(),
        empty_item_slots: t.as_ref().map_or(0, |t| {
            remaining(t.items_capacity, g.container_contents(target).len())
        }),
        empty_container_slots: t.as_ref().map_or(0, |t| {
            remaining(t.containers_capacity, g.contained_containers(target).len())
        }),
        merge_possible: merge,
    })
}

pub fn paint(
    frame: &mut crate::panels::PanelFrame,
    entry: &crate::panels::ItemEntry,
    rect: crate::widgets::Rect,
    selected: bool,
    clip: Option<[i32; 4]>,
) {
    paint_with_feedback(frame, entry, rect, selected, clip, None);
}

pub fn paint_with_feedback(
    frame: &mut crate::panels::PanelFrame,
    entry: &crate::panels::ItemEntry,
    rect: crate::widgets::Rect,
    selected: bool,
    clip: Option<[i32; 4]>,
    feedback: Option<u32>,
) {
    use crate::{panels::rect as area, Command};
    let framed = rect.w == 36 && rect.h == 36;
    let inset = if framed { 2 } else { 0 };
    let (x, y) = (rect.x + inset, rect.y + inset);
    let d = entry.decoration.unwrap_or_else(|| SlotDecoration {
        icon_id: entry.icon.map_or(0, |id| id.0),
        waiting: entry.disabled,
        ..Default::default()
    });
    if entry.id.0 == 0 {
        // A vacant slot shows its own empty art when it names one (the shortcut bar's numbered
        // slots), else the ordinary empty slot.
        frame.screen.commands.push(Command::Image {
            did: entry.icon.map_or_else(
                || if framed { "06000F6E" } else { "06000F6D" }.into(),
                |id| format!("{:08X}", id.0),
            ),
            x,
            y,
            width: 32,
            height: 32,
            clip,
            color_key: None,
            key_bits: None,
            tile: false,
        });
    } else {
        frame.screen.commands.push(Command::ItemIcon {
            recipe: recipe(&d, Surface::Tile),
            x,
            y,
            width: 32,
            height: 32,
            clip,
        });
    }
    let mut dynamic = overlays(&d, selected && entry.id.0 != 0);
    if let Some(did) = feedback {
        dynamic.insert(
            usize::from(d.waiting) + usize::from(selected && entry.id.0 != 0),
            did,
        );
    }
    for did in dynamic {
        frame.screen.commands.push(Command::Image {
            did: format!("{did:08X}"),
            x,
            y,
            width: 32,
            height: 32,
            clip,
            color_key: Some([0, 0, 0]),
            key_bits: Some([5, 6, 5]),
            tile: false,
        });
    }
    if selected {
        if let Some(amount) = entry.amount {
            let text = amount.to_string();
            let width = crate::renderer::measure_text_width("14-5", &text)
                .unwrap_or(i32_from(text.len()) * 5);
            let x = rect.x + rect.w - width - 3;
            frame.label(x - 1, rect.y + 19, &text, "14-5", 0xff080808, clip);
            frame.label(x + 1, rect.y + 21, &text, "14-5", 0xff080808, clip);
            frame.label(x, rect.y + 20, &text, "14-5", 0xffd2d2c8, clip);
        }
    }
    // The frame is painted after the text, before the slot's meters.
    if framed {
        let parent = clip
            .map(|c| area(c[0], c[1], c[2] - c[0], c[3] - c[1]))
            .unwrap_or(rect);
        for border in [
            area(rect.x, rect.y, 36, 2),
            area(rect.x, rect.y + 34, 36, 2),
            area(rect.x, rect.y + 2, 2, 32),
            area(rect.x + 34, rect.y + 2, 2, 32),
        ] {
            if let Some(r) = border.intersect(parent) {
                frame.fill(r, 0xff000000);
            }
        }
    }
    if framed && entry.active_container && entry.id.0 != 0 {
        frame.screen.commands.push(Command::Image {
            did: "060011B4".into(),
            x: rect.x,
            y: rect.y,
            width: 36,
            height: 36,
            clip,
            color_key: Some([0, 0, 0]),
            key_bits: Some([5, 6, 5]),
            tile: false,
        });
    }
    if let Some((top, bottom, split)) = meter(&d) {
        let bounds = area(x + 26, y + 1, 5, 30);
        let parent = clip
            .map(|c| area(c[0], c[1], c[2] - c[0], c[3] - c[1]))
            .unwrap_or(bounds);
        for (did, part) in [
            (top, area(bounds.x, bounds.y, 5, split)),
            (bottom, area(bounds.x, bounds.y + split, 5, 30 - split)),
        ] {
            if let Some(c) = part.intersect(parent) {
                frame.screen.commands.push(Command::Image {
                    did: format!("{did:08X}"),
                    x: bounds.x,
                    y: bounds.y,
                    width: 5,
                    height: 30,
                    clip: Some([c.x, c.y, c.x + c.w, c.y + c.h]),
                    color_key: None,
                    key_bits: None,
                    tile: false,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;
    use dereth_primitives::DataId;
    #[test]
    fn quiet_merge_feedback_obeys_request_guard_and_trade_state() {
        use dereth_client_contract::view::GameView;
        use dereth_primitives::ObjectId;
        #[derive(Debug)]
        struct World {
            traded: bool,
        }
        impl GameView for World {
            fn slot_decoration(&self, id: ObjectId) -> Option<SlotDecoration> {
                Some(SlotDecoration {
                    is_container: id.0 == 1,
                    containers_capacity: if id.0 == 1 { 1 } else { 0 },
                    trade_state: self.traded,
                    ..Default::default()
                })
            }
            fn item_wcid(&self, _: ObjectId) -> u32 {
                123
            }
            fn int_stat(&self, _: ObjectId, stat: u32) -> Option<i32> {
                match stat {
                    0x0d => Some(10),
                    0x0c => Some(3),
                    _ => None,
                }
            }
        }
        let mut g = World { traded: false };
        assert_eq!(
            drop_feedback(&g, ObjectId(1), ObjectId(2), false, 0, true),
            0x060011f7
        );
        assert_eq!(
            drop_feedback(&g, ObjectId(1), ObjectId(2), false, 0, false),
            0x060011f8
        );
        g.traded = true;
        assert_eq!(
            drop_feedback(&g, ObjectId(1), ObjectId(2), false, 0, true),
            0x060011f8
        );
    }
    #[test]
    fn drag_feedback_preserves_forced_modes_slot_categories_and_capacity_branches() {
        let mut f = DropFacts {
            empty: true,
            ..Default::default()
        };
        assert_eq!(feedback(f), 0x060011f9);
        f.dragged_container = true;
        assert_eq!(feedback(f), 0x060011f8);
        f.framed = true;
        assert_eq!(feedback(f), 0x060011f9);
        f.dragged_holds_containers = true;
        assert_eq!(feedback(f), 0x060011f8);
        f.empty = false;
        f.dragged_container = false;
        f.target_exists = true;
        f.empty_item_slots = -1;
        assert_eq!(feedback(f), 0x060011f7);
        f.empty_item_slots = 0;
        assert_eq!(feedback(f), 0x060011f8);
        f.merge_possible = true;
        assert_eq!(feedback(f), 0x060011f7);
        f.forced_mode = 5;
        assert_eq!(feedback(f), 0x060011f8);
        f.forced_mode = 6;
        assert_eq!(feedback(f), 0x060011f9);
    }
    #[test]
    fn empty_slots_keep_native_art_and_active_container_is_independent_of_selection() {
        use crate::{
            panels::{rect, ItemEntry, PanelFrame},
            Command,
        };
        let mut entry = ItemEntry::empty();
        let mut frame = PanelFrame::new(40, 40);
        paint(&mut frame, &entry, rect(0, 0, 36, 36), true, None);
        assert!(frame.screen.commands.iter().any(
            |c| matches!(c,Command::Image{did,x:2,y:2,width:32,height:32,..} if did=="06000F6E")
        ));
        assert!(!frame
            .screen
            .commands
            .iter()
            .any(|c| matches!(c,Command::Image{did,..} if did=="06000F7E")));
        entry.id = dereth_primitives::ObjectId(1);
        entry.active_container = true;
        frame.screen.commands.clear();
        paint(&mut frame, &entry, rect(0, 0, 36, 36), false, None);
        assert!(frame.screen.commands.iter().any(
            |c| matches!(c,Command::Image{did,x:0,y:0,width:36,height:36,..} if did=="060011B4")
        ));
    }
    #[test]
    fn capacity_and_structure_meters_use_native_crops_and_hide_at_distinct_limits() {
        let mut d = SlotDecoration {
            is_container: true,
            items_capacity: 10,
            contained_items: 3,
            ..Default::default()
        };
        assert_eq!(meter(&d), Some((0x06001216, 0x06001217, 21)));
        d.contained_items = 0;
        assert_eq!(meter(&d), None);
        d.contained_items = 12;
        assert_eq!(meter(&d), Some((0x06001216, 0x06001217, 0)));
        d.max_structure = 4;
        d.structure = 2;
        assert_eq!(meter(&d), Some((0x06001216, 0x06001217, 15)));
        d.is_container = false;
        assert_eq!(meter(&d), Some((0x0600135e, 0x0600135f, 15)));
        d.structure = 4;
        assert_eq!(meter(&d), None);
        d.structure = 0;
        assert_eq!(meter(&d), Some((0x0600135e, 0x0600135f, 30)));
    }
    #[test]
    fn sell_and_trade_suppress_shortcut_art_but_preserve_waiting_and_selection() {
        let mut d = SlotDecoration {
            waiting: true,
            shortcut_num: Some(3),
            ..Default::default()
        };
        assert_eq!(overlays(&d, true), [0x06000f7d, 0x06000f7e, 0x060010a1]);
        d.shortcut_ghosted = true;
        assert_eq!(overlays(&d, false), [0x06000f7d, 0x06001acf]);
        d.sell_state = true;
        d.trade_state = true;
        assert_eq!(
            overlays(&d, true),
            [0x06000f7d, 0x06000f7e, 0x060012d9, 0x06001dae]
        );
    }
    #[test]
    fn amount_requires_explicit_override_and_selected_state() {
        use crate::{
            panels::{rect, ItemEntry, PanelFrame},
            Command,
        };
        let mut entry = ItemEntry {
            id: dereth_primitives::ObjectId(1),
            icon: Some(DataId(0x06001234)),
            decoration: None,
            caption: String::new(),
            count: 20,
            amount: None,
            active_container: false,
            disabled: false,
        };
        let mut frame = PanelFrame::new(40, 40);
        paint(&mut frame, &entry, rect(0, 0, 32, 32), true, None);
        assert!(!frame
            .screen
            .commands
            .iter()
            .any(|c| matches!(c, Command::Text { .. })));
        entry.amount = Some(0);
        frame.screen.commands.clear();
        paint(&mut frame, &entry, rect(0, 0, 32, 32), false, None);
        assert!(!frame
            .screen
            .commands
            .iter()
            .any(|c| matches!(c, Command::Text { .. })));
        frame.screen.commands.clear();
        paint(&mut frame, &entry, rect(0, 0, 32, 32), true, None);
        let labels: Vec<_> = frame
            .screen
            .commands
            .iter()
            .filter_map(|c| match c {
                Command::Text {
                    text,
                    y,
                    font,
                    color,
                    ..
                } => Some((text.as_str(), *y, font.as_str(), *color)),
                _ => None,
            })
            .collect();
        assert_eq!(
            labels,
            [
                ("0", 19, "14-5", 0xff080808),
                ("0", 21, "14-5", 0xff080808),
                ("0", 20, "14-5", 0xffd2d2c8)
            ]
        );
    }
    #[test]
    fn tile_and_drag_have_distinct_underlay_and_badge_layers() {
        let d = SlotDecoration {
            icon_id: 0x06001234,
            obj_type: 1,
            icon_overlay_id: Some(DataId(0x06004567)),
            icon_underlay_id: Some(DataId(0x06007654)),
            ..Default::default()
        };
        let tile = recipe(&d, Surface::Tile);
        assert_eq!(tile.background, Some(0x060011cb));
        assert_eq!(tile.underlay, Some(0x06007654));
        assert_eq!(tile.badge, None);
        let drag = recipe(&d, Surface::Drag);
        assert_eq!(drag.background, None);
        assert_eq!(drag.underlay, None);
        assert_eq!(drag.overlay, tile.overlay);
        assert_eq!(drag.badge, Some(0x060011c4));
        assert_eq!(recipe(&d, Surface::AlternateDrag).badge, Some(0x06001348));
    }
    #[test]
    fn effect_priority_and_reused_surfaces_match_the_explicit_chain() {
        assert_eq!(effect_surface(1 | 2 | 0x800), 0x060011ca);
        assert_eq!(effect_surface(8), effect_surface(1));
        assert_eq!(effect_surface(0x40), 0x06001b2d);
        assert_eq!(effect_surface(0x80), 0x06001b2f);
        assert_eq!(effect_surface(0), 0x060011c5);
        assert_eq!(effect_surface(0x80000000), 0x060011c5);
    }
    #[test]
    fn local_player_uses_backpack_icon_and_container_background() {
        let d = SlotDecoration {
            is_player: true,
            icon_id: 1,
            obj_type: 1,
            ..Default::default()
        };
        let r = recipe(&d, Surface::Tile);
        assert_eq!(r.icon, Some(0x0600127e));
        assert_eq!(r.background, Some(0x060011ce));
    }
    #[test]
    fn missing_icon_does_not_render_background_or_decoration_surfaces() {
        let d = SlotDecoration {
            icon_overlay_id: Some(DataId(1)),
            ..Default::default()
        };
        let r = recipe(&d, Surface::Drag);
        assert_eq!(
            (r.background, r.underlay, r.icon, r.overlay, r.badge),
            (None, None, None, None, None)
        );
        assert_eq!(background(0x4000), 0x060011d4);
        assert_eq!(background(0x100000), 0x060013f6);
    }
}
