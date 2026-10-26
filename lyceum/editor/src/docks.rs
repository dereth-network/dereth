//! The editor's docked tabs and their layout: one layout for the world outdoors and one for dungeon
//! mode, each the default until the creator moves a tab or a split, and both kept in the editor's
//! settings folder (`lyceum-layout.json`) for the next start.
//!
//! **Outdoors** the world view takes the left three quarters, with the dungeons, the quests and the
//! loot as tabs beside it; the right column is split in two: the tool of the mode, the map, what
//! stands nearby and the hierarchy over the inspector, the definition and the log, the upper part
//! fitted to the map. **In dungeon mode** the map and the dungeons give way to the dungeon's grid,
//! a tab beside the inspector, and its cross-section takes a strip under the world view.
//!
//! A kept layout is taken only when it was written by this layout's version and holds every tab
//! its mode shows, each once; any other falls back to the default, so a tab added since is never
//! lost. One of the version before is taken with the dungeons, the quests and the loot moved
//! beside the world view. An optional tab (the weenie editor) may be in it or not: it opens
//! floating over the world view, and closes with its ×.
//!
//! While a playtest shows in the world view, the floating windows over it are taken out of the
//! layout ([`take_windows_over`]) and put back as they were when it stops ([`put_back`]).

use std::collections::BTreeSet;
use std::path::Path;

use egui_dock::{DockState, Node, NodeIndex, Surface, SurfaceIndex, Tree};
use serde::{Deserialize, Serialize};

/// A docked tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Tab {
    /// The world view.
    World,
    /// A dungeon's floors seen from the south.
    Side,
    /// The tool of the mode.
    Tool,
    Map,
    /// A dungeon's grid, a floor at a time.
    Grid,
    Dungeons,
    Nearby,
    /// The selected weenie's definition.
    Definition,
    Inspector,
    Hierarchy,
    /// The checks' findings and the status bar's messages.
    Log,
    /// The weenie editor: optional, opened on a weenie and closed with its ×.
    Editor,
    /// The playtest characters' recipes: optional, floating at first, closed with its ×.
    Characters,
    /// Every quest of the world, where each is used, and its changes.
    Quests,
    /// Every loot table of the world, where each is used, and its changes.
    Loot,
}

/// The tabs a layout may hold or not.
pub const OPTIONAL: [Tab; 2] = [Tab::Editor, Tab::Characters];

/// The layout's version: a kept layout of another is not taken, but for the one before, whose
/// lists are moved beside the world view.
pub const VERSION: u32 = 5;

/// The tabs beside the world view, behind it at first: the lists the world is built from.
pub const BESIDE_WORLD: [Tab; 3] = [Tab::Dungeons, Tab::Quests, Tab::Loot];

/// The file the layout is kept in, in the editor's settings folder.
pub const FILE_NAME: &str = "lyceum-layout.json";

/// The tabs the layout outdoors shows.
pub const OUTDOORS: [Tab; 11] = [
    Tab::World,
    Tab::Tool,
    Tab::Map,
    Tab::Nearby,
    Tab::Hierarchy,
    Tab::Dungeons,
    Tab::Quests,
    Tab::Loot,
    Tab::Inspector,
    Tab::Definition,
    Tab::Log,
];

/// The tabs the layout in dungeon mode shows: no map, and no list of dungeons.
pub const IN_DUNGEON: [Tab; 11] = [
    Tab::World,
    Tab::Side,
    Tab::Tool,
    Tab::Nearby,
    Tab::Hierarchy,
    Tab::Quests,
    Tab::Loot,
    Tab::Grid,
    Tab::Inspector,
    Tab::Definition,
    Tab::Log,
];

/// How much of the window's width the world view takes: the right column starts 1458 points
/// into a window 2000 wide.
pub const WORLD_WIDTH: f32 = 0.729;

/// How much of the right column's height its upper half takes, until it is fitted to the map.
pub const UPPER: f32 = 0.52;

/// How much of the left column's height the world view takes over the cross-section.
const WORLD_HEIGHT: f32 = 0.74;

/// The default layout outdoors.
#[must_use]
pub fn outdoors() -> DockState<Tab> {
    let mut d = DockState::new(vec![Tab::World, Tab::Dungeons, Tab::Quests, Tab::Loot]);
    let tree = d.main_surface_mut();
    let [_, right] = tree.split_right(
        NodeIndex::root(),
        WORLD_WIDTH,
        vec![Tab::Tool, Tab::Map, Tab::Nearby, Tab::Hierarchy],
    );
    tree.split_below(
        right,
        UPPER,
        vec![Tab::Inspector, Tab::Definition, Tab::Log],
    );
    d
}

/// The default layout in dungeon mode.
#[must_use]
pub fn in_dungeon() -> DockState<Tab> {
    let mut d = DockState::new(vec![Tab::World, Tab::Quests, Tab::Loot]);
    let tree = d.main_surface_mut();
    let [left, right] = tree.split_right(
        NodeIndex::root(),
        WORLD_WIDTH,
        vec![Tab::Tool, Tab::Nearby, Tab::Hierarchy],
    );
    tree.split_below(
        right,
        UPPER,
        vec![Tab::Grid, Tab::Inspector, Tab::Definition, Tab::Log],
    );
    tree.split_below(left, WORLD_HEIGHT, vec![Tab::Side]);
    d
}

/// Make the part of `dock` that holds `tab` taller by `more` points (shorter when negative),
/// moving the split over or under it: the two parts' sizes are a fraction of the split's.
/// Answers whether it could.
pub fn grow(dock: &mut DockState<Tab>, tab: Tab, more: f32) -> bool {
    let tree = dock.main_surface_mut();
    let Some((leaf, _)) = tree.find_tab(&tab) else {
        return false;
    };
    let Some(parent) = leaf.parent() else {
        return false;
    };
    let Node::Vertical(split) = &mut tree[parent] else {
        return false;
    };
    let height = split.rect().height();
    if height.is_nan() || height <= 1.0 {
        return false;
    }
    let by = more / height;
    split.fraction = if leaf.is_left() {
        split.fraction + by
    } else {
        split.fraction - by
    }
    .clamp(0.2, 0.85);
    true
}

/// The body of the part of `dock` that holds `tab`, as laid out last, whichever of its tabs is
/// shown.
#[must_use]
pub fn body_of(dock: &DockState<Tab>, tab: Tab) -> Option<egui::Rect> {
    let at = dock.find_tab(&tab)?;
    match &dock[at.surface][at.node] {
        Node::Leaf(leaf) => Some(leaf.viewport).filter(|r| r.is_positive()),
        _ => None,
    }
}

/// The lists of [`BESIDE_WORLD`] that `dock` holds moved beside the world view, after its other
/// tabs, the world view shown.
pub fn lists_beside_world(dock: &mut DockState<Tab>) {
    for tab in BESIDE_WORLD {
        let (Some(world), Some(at)) = (dock.find_tab(&Tab::World), dock.find_tab(&tab)) else {
            continue;
        };
        if (world.surface, world.node) == (at.surface, at.node) {
            continue;
        }
        let Some(tab) = dock.remove_tab(at) else {
            continue;
        };
        // Taking it may have moved the world view's part.
        if let Some(world) = dock.find_tab(&Tab::World) {
            dock[world.surface][world.node].append_tab(tab);
        }
    }
    if let Some(world) = dock.find_tab(&Tab::World) {
        let _ = dock.set_active_tab(world);
    }
}

/// Whether `dock` is the default layout outdoors as it is made, not yet laid out or moved.
#[must_use]
pub fn is_new_outdoors(dock: &DockState<Tab>) -> bool {
    serde_json::to_value(dock).ok() == serde_json::to_value(outdoors()).ok()
}

/// Whether `dock` shows exactly the tabs `tabs`, each once, and an optional tab at most once.
#[must_use]
pub fn holds_exactly(dock: &DockState<Tab>, tabs: &[Tab]) -> bool {
    let all: Vec<Tab> = dock.iter_all_tabs().map(|(_, t)| *t).collect();
    let shown: Vec<Tab> = all
        .iter()
        .copied()
        .filter(|t| !OPTIONAL.contains(t))
        .collect();
    let want: BTreeSet<Tab> = tabs.iter().copied().collect();
    let once = OPTIONAL
        .iter()
        .all(|o| all.iter().filter(|t| *t == o).count() <= 1);
    once && shown.len() == want.len() && shown.iter().copied().collect::<BTreeSet<Tab>>() == want
}

/// Whether `dock` holds `tab`, docked or floating.
#[must_use]
pub fn holds(dock: &DockState<Tab>, tab: Tab) -> bool {
    dock.find_tab(&tab).is_some()
}

/// `tab` opened in `dock` as a window of its own over `rect` (points), unless it holds it.
pub fn open_floating(dock: &mut DockState<Tab>, tab: Tab, rect: egui::Rect) {
    if holds(dock, tab) {
        return;
    }
    let surface = dock.add_window(vec![tab]);
    if let Some(w) = dock.get_window_state_mut(surface) {
        w.set_position(rect.min).set_size(rect.size());
    }
}

/// `tab` taken out of `dock`, wherever it is (a window it leaves empty goes with it).
pub fn close(dock: &mut DockState<Tab>, tab: Tab) {
    if let Some(at) = dock.find_tab(&tab) {
        dock.remove_tab(at);
    }
}

/// Where the editor opens: the right part of the world view `world`, about 900 by 700 points,
/// as much of that as the view has.
#[must_use]
pub fn editor_rect(world: egui::Rect) -> egui::Rect {
    let size = egui::vec2(
        900.0_f32.min(world.width() - 32.0).max(320.0),
        700.0_f32.min(world.height() - 32.0).max(240.0),
    );
    let min = egui::pos2(
        (world.max.x - 16.0 - size.x).max(world.min.x),
        world.min.y + 16.0,
    );
    egui::Rect::from_min_size(min, size)
}

/// The two layouts as kept.
#[derive(Serialize, Deserialize)]
struct Kept {
    version: u32,
    outdoors: DockState<Tab>,
    in_dungeon: DockState<Tab>,
}

/// The two layouts kept in `text`, outdoors and in dungeon mode: each the kept one when `text` is
/// of this version and it shows every tab of its mode once, else the default.
#[must_use]
pub fn read(text: &str) -> (DockState<Tab>, DockState<Tab>) {
    let Ok(kept) = serde_json::from_str::<Kept>(text) else {
        return (outdoors(), in_dungeon());
    };
    let mut kept = kept;
    match kept.version {
        VERSION => {}
        // The version before: its lists moved beside the world view.
        v if VERSION.checked_sub(1) == Some(v) => {
            lists_beside_world(&mut kept.outdoors);
            lists_beside_world(&mut kept.in_dungeon);
        }
        _ => return (outdoors(), in_dungeon()),
    }
    let out = if holds_exactly(&kept.outdoors, &OUTDOORS) {
        kept.outdoors
    } else {
        outdoors()
    };
    let dungeon = if holds_exactly(&kept.in_dungeon, &IN_DUNGEON) {
        kept.in_dungeon
    } else {
        in_dungeon()
    };
    (out, dungeon)
}

/// The two layouts as they are kept.
#[must_use]
pub fn text(out: &DockState<Tab>, dungeon: &DockState<Tab>) -> String {
    #[derive(Serialize)]
    struct Keep<'a> {
        version: u32,
        outdoors: &'a DockState<Tab>,
        in_dungeon: &'a DockState<Tab>,
    }
    let Ok(mut v) = serde_json::to_value(&Keep {
        version: VERSION,
        outdoors: out,
        in_dungeon: dungeon,
    }) else {
        return String::new();
    };
    zero_unset_points(&mut v);
    serde_json::to_string_pretty(&v).unwrap_or_default()
}

/// Every point in `v` with no coordinates (a rectangle the dock has not laid out yet, which JSON
/// cannot hold) at zero: the dock lays it out again as it is shown.
fn zero_unset_points(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::Object(o) => {
            if o.len() == 2 && o.contains_key("x") && o.contains_key("y") {
                for c in o.values_mut() {
                    if c.is_null() {
                        *c = serde_json::Value::from(0.0);
                    }
                }
            }
            o.values_mut().for_each(zero_unset_points);
        }
        serde_json::Value::Array(a) => a.iter_mut().for_each(zero_unset_points),
        _ => {}
    }
}

/// The two layouts kept in the file `path`, or the defaults.
#[must_use]
pub fn read_file(path: &Path) -> (DockState<Tab>, DockState<Tab>) {
    std::fs::read_to_string(path).map_or_else(|_| (outdoors(), in_dungeon()), |t| read(&t))
}

/// Keep the two layouts in the file `path`.
///
/// # Errors
/// The file would not write.
pub fn write_file(
    path: &Path,
    out: &DockState<Tab>,
    dungeon: &DockState<Tab>,
) -> std::io::Result<()> {
    std::fs::write(path, text(out, dungeon))
}

/// How far a floating window reaches into the world view, across and down, to be over it: a
/// window whose edge only meets the view's is not.
pub const OVER_VIEW: f32 = 8.0;

/// A floating window taken out of a layout while a playtest shows in the world view.
#[derive(Debug, Clone)]
pub struct Hidden {
    /// Its place among the layout's windows, by which egui keeps where it stands and its size.
    index: SurfaceIndex,
    surface: Surface<Tab>,
    /// Where it stood, for when its place is taken meanwhile.
    rect: Option<egui::Rect>,
}

impl Hidden {
    /// Whether it holds `tab`.
    #[must_use]
    pub fn holds(&self, tab: Tab) -> bool {
        self.surface
            .node_tree()
            .is_some_and(|t| t.find_tab(&tab).is_some())
    }
}

/// The egui window a layout's floating window `index` is drawn as.
#[must_use]
pub fn window_id(index: SurfaceIndex) -> egui::Id {
    egui::Id::new(format!("window {index:?}"))
}

/// Where the parts of floating window `tree` were laid out last, together; none before it is
/// first laid out.
fn laid_out(tree: &Tree<Tab>) -> Option<egui::Rect> {
    tree.iter()
        .filter_map(|n| match n {
            Node::Leaf(l) if l.rect.is_finite() && l.rect.is_positive() => Some(l.rect),
            _ => None,
        })
        .reduce(|a, b| a.union(b))
}

/// The floating windows of `dock` over `view` (by [`OVER_VIEW`] at least), taken out of it in
/// the order they stand among its windows. `drawn` answers where egui last drew a window, by its
/// place; one it has not drawn is where its parts were laid out.
pub fn take_windows_over(
    dock: &mut DockState<Tab>,
    view: egui::Rect,
    drawn: impl Fn(SurfaceIndex) -> Option<egui::Rect>,
) -> Vec<Hidden> {
    let over: Vec<(SurfaceIndex, egui::Rect)> = dock
        .iter_surfaces_indexed()
        .filter_map(|(i, s)| {
            let Surface::Window(tree, _) = s else {
                return None;
            };
            let rect = drawn(i).or_else(|| laid_out(tree))?;
            let shared = rect.intersect(view);
            (shared.width() >= OVER_VIEW && shared.height() >= OVER_VIEW).then_some((i, rect))
        })
        .collect();
    // From the last, so each one's place is still its own as it is taken.
    let mut hidden: Vec<Hidden> = over
        .into_iter()
        .rev()
        .filter_map(|(index, rect)| {
            let surface = dock.remove_surface(index)?;
            Some(Hidden {
                index,
                surface,
                rect: Some(rect),
            })
        })
        .collect();
    hidden.reverse();
    hidden
}

/// Window `hidden` put back in `dock`: in its own place among the windows when that is free,
/// where it stands and is as big as it was; else in a new place, at where it stood. Windows taken
/// together are put back in the order they were taken.
pub fn put_back(dock: &mut DockState<Tab>, hidden: Hidden) {
    let free = dock
        .get_surface(hidden.index)
        .is_some_and(Surface::is_empty);
    let at = if free {
        hidden.index
    } else {
        dock.add_window(Vec::new())
    };
    if let Some(s) = dock.get_surface_mut(at) {
        *s = hidden.surface;
    }
    if at != hidden.index {
        if let (Some(w), Some(rect)) = (dock.get_window_state_mut(at), hidden.rect) {
            w.set_position(rect.min);
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's docked tabs)
    use super::*;

    #[test]
    fn each_default_layout_shows_every_tab_of_its_mode_once() {
        assert!(holds_exactly(&outdoors(), &OUTDOORS));
        assert!(holds_exactly(&in_dungeon(), &IN_DUNGEON));
        assert!(!holds_exactly(&outdoors(), &IN_DUNGEON));
    }

    #[test]
    fn the_dungeon_layout_has_no_map_and_no_list_of_dungeons_and_the_definition_follows_the_inspector(
    ) {
        // In both, what stands nearby shares a pane with the hierarchy, and the quests and the
        // loot are tabs beside the world view, which is shown.
        for dock in [outdoors(), in_dungeon()] {
            let (inspector, at) = dock.main_surface().find_tab(&Tab::Inspector).unwrap();
            let (definition, next) = dock.main_surface().find_tab(&Tab::Definition).unwrap();
            assert_eq!((definition, next.0), (inspector, at.0 + 1));
            let (nearby, _) = dock.main_surface().find_tab(&Tab::Nearby).unwrap();
            let (hierarchy, _) = dock.main_surface().find_tab(&Tab::Hierarchy).unwrap();
            let (tool, _) = dock.main_surface().find_tab(&Tab::Tool).unwrap();
            let (world, _) = dock.main_surface().find_tab(&Tab::World).unwrap();
            let (quests, _) = dock.main_surface().find_tab(&Tab::Quests).unwrap();
            let (loot, _) = dock.main_surface().find_tab(&Tab::Loot).unwrap();
            assert!(nearby == tool && hierarchy == tool, "one pane");
            assert!(quests == world && loot == world, "beside the world view");
            let Node::Leaf(leaf) = &dock.main_surface()[world] else {
                panic!("a part of tabs");
            };
            assert_eq!(leaf.tabs[leaf.active.0], Tab::World, "the world view shown");
        }
        let (world, _) = outdoors().main_surface().find_tab(&Tab::World).unwrap();
        let (dungeons, _) = outdoors().main_surface().find_tab(&Tab::Dungeons).unwrap();
        assert_eq!(world, dungeons);
        let dungeon = in_dungeon();
        assert!(dungeon.main_surface().find_tab(&Tab::Map).is_none());
        assert!(dungeon.main_surface().find_tab(&Tab::Dungeons).is_none());
    }

    #[test]
    fn outdoors_the_right_column_starts_at_1458_of_2000() {
        let d = outdoors();
        let Node::Horizontal(split) = &d.main_surface()[NodeIndex::root()] else {
            panic!("the world view beside the right column");
        };
        assert!(
            (split.fraction * 2000.0 - 1458.0).abs() < 1.0,
            "{}",
            split.fraction
        );
    }

    #[test]
    fn growing_a_part_moves_its_split_by_that_part_of_the_splits_height() {
        let mut d = outdoors();
        // Laid out: the right column 1000 points high.
        let tree = d.main_surface_mut();
        let (leaf, _) = tree.find_tab(&Tab::Map).unwrap();
        let parent = leaf.parent().unwrap();
        if let Node::Vertical(s) = &mut tree[parent] {
            s.set_rect(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(400.0, 1000.0),
            ));
        }
        assert!(grow(&mut d, Tab::Map, 100.0));
        let fraction = |d: &DockState<Tab>| match &d.main_surface()[parent] {
            Node::Vertical(s) => s.fraction,
            _ => unreachable!(),
        };
        assert!((fraction(&d) - (UPPER + 0.1)).abs() < 1e-5);
        // The part under it, grown, makes the upper one shorter.
        assert!(grow(&mut d, Tab::Log, 200.0));
        assert!((fraction(&d) - (UPPER - 0.1)).abs() < 1e-5);
        assert!(
            !grow(&mut d, Tab::World, 10.0),
            "the world view's split is across"
        );
    }

    #[test]
    fn a_layout_moved_is_kept_and_read_back_as_it_was_moved() {
        let mut out = outdoors();
        let at = out.find_tab(&Tab::Log).unwrap();
        let log = out.remove_tab(at).unwrap();
        out.main_surface_mut()
            .split_left(NodeIndex::root(), 0.2, vec![log]);
        let text = text(&out, &in_dungeon());
        let (back, dungeon) = read(&text);
        assert!(holds_exactly(&back, &OUTDOORS));
        assert_eq!(
            back.main_surface().num_tabs(),
            out.main_surface().num_tabs()
        );
        assert_eq!(super::text(&back, &dungeon), text);
        assert!(holds_exactly(&dungeon, &IN_DUNGEON));
    }

    #[test]
    fn the_editor_opens_floating_once_closes_and_a_kept_layout_holds_it_or_not() {
        let mut out = outdoors();
        let world = egui::Rect::from_min_size(egui::pos2(0.0, 30.0), egui::vec2(1400.0, 1000.0));
        let at = editor_rect(world);
        assert_eq!(at.size(), egui::vec2(900.0, 700.0));
        assert!((at.max.x - 1384.0).abs() < 1e-3 && (at.min.y - 46.0).abs() < 1e-3);
        open_floating(&mut out, Tab::Editor, at);
        open_floating(&mut out, Tab::Editor, at);
        assert!(holds(&out, Tab::Editor));
        assert_eq!(
            out.iter_all_tabs()
                .filter(|(_, t)| **t == Tab::Editor)
                .count(),
            1
        );
        let (path, _) = out
            .iter_all_tabs()
            .find(|(_, t)| **t == Tab::Editor)
            .unwrap();
        assert!(!path.surface.is_main(), "a window of its own");
        assert!(holds_exactly(&out, &OUTDOORS));
        // Kept with it open, it is read back so; kept without it, too.
        let (back, _) = read(&text(&out, &in_dungeon()));
        assert!(holds(&back, Tab::Editor) && holds_exactly(&back, &OUTDOORS));
        close(&mut out, Tab::Editor);
        assert!(!holds(&out, Tab::Editor));
        assert_eq!(
            out.iter_surfaces().filter(|s| !s.is_empty()).count(),
            1,
            "its window gone"
        );
        assert!(holds_exactly(&out, &OUTDOORS));
        // Twice over, it is no layout to keep.
        let mut twice = outdoors();
        twice.add_window(vec![Tab::Editor]);
        twice.add_window(vec![Tab::Editor]);
        assert!(!holds_exactly(&twice, &OUTDOORS));
        // A small world view: the editor fits in it.
        let small = editor_rect(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(600.0, 500.0),
        ));
        assert!(small.min.x >= 0.0 && small.width() <= 568.0 && small.height() <= 468.0);
    }

    #[test]
    fn a_layout_is_kept_with_the_characters_tab_open_or_closed_but_not_twice() {
        let mut out = outdoors();
        let world = egui::Rect::from_min_size(egui::pos2(0.0, 30.0), egui::vec2(1400.0, 1000.0));
        open_floating(&mut out, Tab::Characters, world);
        open_floating(&mut out, Tab::Editor, world);
        assert!(holds_exactly(&out, &OUTDOORS), "both open, floating");
        let (back, _) = read(&text(&out, &in_dungeon()));
        assert!(
            holds(&back, Tab::Characters) && holds(&back, Tab::Editor),
            "kept open"
        );
        close(&mut out, Tab::Characters);
        assert!(!holds(&out, Tab::Characters) && holds_exactly(&out, &OUTDOORS));
        out.add_window(vec![Tab::Editor]);
        assert!(!holds_exactly(&out, &OUTDOORS), "twice");
    }

    #[test]
    fn a_layout_of_another_version_or_lacking_a_tab_falls_back_to_the_default() {
        let default = serde_json::to_string(&outdoors()).unwrap();
        let mut moved = outdoors();
        let at = moved.find_tab(&Tab::Log).unwrap();
        let log = moved.remove_tab(at).unwrap();
        moved
            .main_surface_mut()
            .split_left(NodeIndex::root(), 0.3, vec![log]);
        assert_ne!(serde_json::to_string(&moved).unwrap(), default);
        let older = text(&moved, &in_dungeon())
            .replace(&format!("\"version\": {VERSION}"), "\"version\": 1");
        let (out, _) = read(&older);
        assert_eq!(serde_json::to_string(&out).unwrap(), default);
        // Of this version, but without the log: its default, the log in it.
        let mut lacking = outdoors();
        let at = lacking.find_tab(&Tab::Log).unwrap();
        lacking.remove_tab(at);
        let (out, dungeon) = read(&text(&lacking, &in_dungeon()));
        assert!(is_new_outdoors(&out) && !is_new_outdoors(&moved));
        assert!(holds_exactly(&out, &OUTDOORS));
        assert!(holds_exactly(&dungeon, &IN_DUNGEON));
        // Nothing that reads: both defaults.
        let (out, _) = read("not a layout");
        assert_eq!(serde_json::to_string(&out).unwrap(), default);
    }

    #[test]
    fn a_layout_of_the_version_before_is_kept_with_its_lists_moved_beside_the_world_view() {
        // As the version before laid it out: the lists in the right column; the log moved.
        let mut out = DockState::new(vec![Tab::World]);
        let tree = out.main_surface_mut();
        let [_, right] = tree.split_right(
            NodeIndex::root(),
            WORLD_WIDTH,
            vec![
                Tab::Tool,
                Tab::Map,
                Tab::Nearby,
                Tab::Hierarchy,
                Tab::Dungeons,
                Tab::Quests,
                Tab::Loot,
            ],
        );
        tree.split_below(right, UPPER, vec![Tab::Inspector, Tab::Definition]);
        tree.split_left(NodeIndex::root(), 0.2, vec![Tab::Log]);
        let mut dungeon = DockState::new(vec![Tab::World]);
        let tree = dungeon.main_surface_mut();
        let [left, right] = tree.split_right(
            NodeIndex::root(),
            WORLD_WIDTH,
            vec![
                Tab::Tool,
                Tab::Nearby,
                Tab::Hierarchy,
                Tab::Quests,
                Tab::Loot,
            ],
        );
        tree.split_below(
            right,
            UPPER,
            vec![Tab::Grid, Tab::Inspector, Tab::Definition, Tab::Log],
        );
        tree.split_below(left, WORLD_HEIGHT, vec![Tab::Side]);
        assert!(holds_exactly(&out, &OUTDOORS) && holds_exactly(&dungeon, &IN_DUNGEON));
        let older = text(&out, &dungeon).replace(
            &format!("\"version\": {VERSION}"),
            &format!("\"version\": {}", VERSION - 1),
        );
        assert_ne!(older, text(&out, &dungeon));
        let (back, inside) = read(&older);
        assert!(!is_new_outdoors(&back), "kept, not the default");
        let tree = back.main_surface();
        let (world, _) = tree.find_tab(&Tab::World).unwrap();
        for t in BESIDE_WORLD {
            assert_eq!(
                tree.find_tab(&t).unwrap().0,
                world,
                "{t:?} beside the world view"
            );
        }
        let (log, _) = tree.find_tab(&Tab::Log).unwrap();
        assert_ne!(log, world, "the rest as it was moved");
        let Node::Leaf(leaf) = &tree[world] else {
            panic!("a part of tabs");
        };
        assert_eq!(leaf.tabs[leaf.active.0], Tab::World, "the world view shown");
        let (world, _) = inside.main_surface().find_tab(&Tab::World).unwrap();
        assert_eq!(inside.main_surface().find_tab(&Tab::Loot).unwrap().0, world);
        assert!(holds_exactly(&back, &OUTDOORS) && holds_exactly(&inside, &IN_DUNGEON));
    }

    #[test]
    fn a_layout_kept_before_the_quests_tab_is_read_as_the_default_with_it() {
        let mut moved = outdoors();
        let at = moved.find_tab(&Tab::Log).unwrap();
        let log = moved.remove_tab(at).unwrap();
        moved
            .main_surface_mut()
            .split_left(NodeIndex::root(), 0.3, vec![log]);
        let mut dungeon = in_dungeon();
        for d in [&mut moved, &mut dungeon] {
            let at = d.find_tab(&Tab::Quests).unwrap();
            d.remove_tab(at);
        }
        let (out, inside) = read(&text(&moved, &dungeon));
        assert!(is_new_outdoors(&out), "the default, the quests in it");
        assert!(holds(&out, Tab::Quests) && holds(&inside, Tab::Quests));
    }

    #[test]
    fn a_layout_kept_before_the_loot_tab_is_read_as_the_default_with_it() {
        let mut moved = outdoors();
        let at = moved.find_tab(&Tab::Log).unwrap();
        let log = moved.remove_tab(at).unwrap();
        moved
            .main_surface_mut()
            .split_left(NodeIndex::root(), 0.3, vec![log]);
        let mut dungeon = in_dungeon();
        for d in [&mut moved, &mut dungeon] {
            let at = d.find_tab(&Tab::Loot).unwrap();
            d.remove_tab(at);
        }
        let (out, inside) = read(&text(&moved, &dungeon));
        assert!(is_new_outdoors(&out), "the default, the loot in it");
        assert!(holds(&out, Tab::Loot) && holds(&inside, Tab::Loot));
    }

    /// The world view of a window 2000 by 1000 points.
    const VIEW: egui::Rect = egui::Rect {
        min: egui::pos2(0.0, 30.0),
        max: egui::pos2(1458.0, 970.0),
    };

    /// `tab` floating in `dock`, laid out at `rect`: answers its place among the windows.
    fn floating(dock: &mut DockState<Tab>, tab: Tab, rect: egui::Rect) -> SurfaceIndex {
        let at = dock.add_window(vec![tab]);
        if let Some(Node::Leaf(leaf)) = dock
            .get_surface_mut(at)
            .and_then(Surface::node_tree_mut)
            .map(|t| &mut t[NodeIndex::root()])
        {
            leaf.rect = rect;
        }
        at
    }

    /// Each of `dock`'s places among its windows, as written.
    fn windows(dock: &DockState<Tab>) -> Vec<String> {
        dock.iter_surfaces()
            .map(|s| serde_json::to_string(s).unwrap())
            .collect()
    }

    #[test]
    fn floating_windows_over_the_world_view_are_taken_out_and_put_back_as_they_were() {
        let mut dock = outdoors();
        let over = egui::Rect::from_min_size(egui::pos2(900.0, 50.0), egui::vec2(560.0, 720.0));
        let beside = egui::Rect::from_min_size(egui::pos2(1500.0, 50.0), egui::vec2(400.0, 300.0));
        floating(&mut dock, Tab::Editor, over);
        floating(&mut dock, Tab::Characters, beside);
        let before = windows(&dock);
        let hidden = take_windows_over(&mut dock, VIEW, |_| None);
        assert_eq!(hidden.len(), 1);
        assert!(hidden[0].holds(Tab::Editor) && !hidden[0].holds(Tab::Characters));
        assert!(!holds(&dock, Tab::Editor), "over the view: taken out");
        assert!(holds(&dock, Tab::Characters), "beside it: left");
        assert!(holds(&dock, Tab::Inspector), "docked tabs stay");
        for h in hidden {
            put_back(&mut dock, h);
        }
        assert_eq!(windows(&dock), before, "each back in its own place");
    }

    #[test]
    fn the_last_windows_taken_out_are_put_back_in_their_own_places() {
        let mut dock = outdoors();
        let over =
            |x: f32| egui::Rect::from_min_size(egui::pos2(x, 100.0), egui::vec2(300.0, 300.0));
        floating(&mut dock, Tab::Editor, over(100.0));
        floating(&mut dock, Tab::Characters, over(600.0));
        let before = windows(&dock);
        let hidden = take_windows_over(&mut dock, VIEW, |_| None);
        assert_eq!(hidden.len(), 2);
        assert_eq!(dock.surfaces_count(), 1, "only the main layout left");
        for h in hidden {
            put_back(&mut dock, h);
        }
        assert_eq!(windows(&dock), before);
    }

    #[test]
    fn a_window_whose_edge_only_meets_the_world_view_is_not_over_it() {
        let mut dock = outdoors();
        let edge = egui::Rect::from_min_size(
            egui::pos2(VIEW.max.x - OVER_VIEW + 2.0, 100.0),
            egui::vec2(400.0, 300.0),
        );
        floating(&mut dock, Tab::Editor, edge);
        assert!(take_windows_over(&mut dock, VIEW, |_| None).is_empty());
        // Where egui drew it is taken over where it was laid out.
        let drawn = egui::Rect::from_min_size(egui::pos2(200.0, 100.0), egui::vec2(400.0, 300.0));
        let hidden = take_windows_over(&mut dock, VIEW, |_| Some(drawn));
        assert_eq!(hidden.len(), 1);
    }

    #[test]
    fn a_window_whose_place_was_taken_meanwhile_comes_back_in_another() {
        let mut dock = outdoors();
        let over = egui::Rect::from_min_size(egui::pos2(100.0, 100.0), egui::vec2(300.0, 300.0));
        let beside = egui::Rect::from_min_size(egui::pos2(1500.0, 50.0), egui::vec2(400.0, 300.0));
        let first = floating(&mut dock, Tab::Editor, over);
        floating(&mut dock, Tab::Characters, beside);
        let hidden = take_windows_over(&mut dock, VIEW, |_| None);
        // A tab dragged out into a window of its own meanwhile takes the free place.
        let log = dock.find_tab(&Tab::Log).unwrap();
        let taken = dock.detach_tab(log, over);
        assert_eq!(taken, first);
        for h in hidden {
            put_back(&mut dock, h);
        }
        assert!(holds(&dock, Tab::Editor) && holds(&dock, Tab::Log));
        assert_ne!(dock.find_tab(&Tab::Editor).unwrap().surface, first);
    }
}
