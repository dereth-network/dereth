//! The editor's docked tabs and their layout: one layout for the world outdoors and one for dungeon
//! mode, each the default until the creator moves a tab or a split, and both kept in the editor's
//! settings folder (`lyceum-layout.json`) for the next start.
//!
//! **Outdoors** the world view takes the left three quarters; the right column is split in two:
//! the tool of the mode, the map, what stands nearby, the hierarchy and the dungeons over the
//! inspector, the definition and the log, the upper part fitted to the map. **In dungeon mode**
//! the map and the dungeons give way to the dungeon's grid, a tab beside the inspector, and its
//! cross-section takes a strip under the world view.
//!
//! A kept layout is taken only when it was written by this layout's version and holds every tab
//! its mode shows, each once; any other falls back to the default, so a tab added since is never
//! lost.

use std::collections::BTreeSet;
use std::path::Path;

use egui_dock::{DockState, Node, NodeIndex};
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
}

/// The layout's version: a kept layout of another is not taken.
pub const VERSION: u32 = 4;

/// The file the layout is kept in, in the editor's settings folder.
pub const FILE_NAME: &str = "lyceum-layout.json";

/// The tabs the layout outdoors shows.
pub const OUTDOORS: [Tab; 9] = [
    Tab::World,
    Tab::Tool,
    Tab::Map,
    Tab::Nearby,
    Tab::Hierarchy,
    Tab::Dungeons,
    Tab::Inspector,
    Tab::Definition,
    Tab::Log,
];

/// The tabs the layout in dungeon mode shows: no map, and no list of dungeons.
pub const IN_DUNGEON: [Tab; 9] = [
    Tab::World,
    Tab::Side,
    Tab::Tool,
    Tab::Nearby,
    Tab::Hierarchy,
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
    let mut d = DockState::new(vec![Tab::World]);
    let tree = d.main_surface_mut();
    let [_, right] = tree.split_right(
        NodeIndex::root(),
        WORLD_WIDTH,
        vec![
            Tab::Tool,
            Tab::Map,
            Tab::Nearby,
            Tab::Hierarchy,
            Tab::Dungeons,
        ],
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
    let mut d = DockState::new(vec![Tab::World]);
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

/// Whether `dock` is the default layout outdoors as it is made, not yet laid out or moved.
#[must_use]
pub fn is_new_outdoors(dock: &DockState<Tab>) -> bool {
    serde_json::to_value(dock).ok() == serde_json::to_value(outdoors()).ok()
}

/// Whether `dock` shows exactly the tabs `tabs`, each once.
#[must_use]
pub fn holds_exactly(dock: &DockState<Tab>, tabs: &[Tab]) -> bool {
    let shown: Vec<Tab> = dock.iter_all_tabs().map(|(_, t)| *t).collect();
    let want: BTreeSet<Tab> = tabs.iter().copied().collect();
    shown.len() == want.len() && shown.iter().copied().collect::<BTreeSet<Tab>>() == want
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
    if kept.version != VERSION {
        return (outdoors(), in_dungeon());
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
        for dock in [outdoors(), in_dungeon()] {
            let (inspector, at) = dock.main_surface().find_tab(&Tab::Inspector).unwrap();
            let (definition, next) = dock.main_surface().find_tab(&Tab::Definition).unwrap();
            assert_eq!((definition, next.0), (inspector, at.0 + 1));
            let (nearby, _) = dock.main_surface().find_tab(&Tab::Nearby).unwrap();
            let (hierarchy, _) = dock.main_surface().find_tab(&Tab::Hierarchy).unwrap();
            let (tool, _) = dock.main_surface().find_tab(&Tab::Tool).unwrap();
            assert!(nearby == tool && hierarchy == tool, "one pane");
        }
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
}
