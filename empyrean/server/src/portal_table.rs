//! Not ACE: the world's portal table, published by the status endpoint as `GET /portals` when
//! `server.publish_portals` is on.
//!
//! ACE publishes nothing like it. A client that routes a character through portals needs to know
//! where each placed portal stands, where it leads and who may use it; this is that list, read from
//! the world content the server loaded (its landblock instances and their weenies), never from the
//! live world, so no landblock is loaded to build it.
//!
//! The document is tab-separated text with LF line endings (`→` marks a tab):
//!
//! ```text
//! dereth-navigation-store→1
//! world→<world name>
//! portal→published→<wcid>→<name>→<from>→<to>→0→0→0→<min level or ->→<max level or ->→-
//! ```
//!
//! There is one `portal` line per landblock instance whose weenie is a portal with a destination,
//! ordered by its `from` cell, then its weenie class id, then its instance guid. A position is
//! `0x<cell, 8 hex digits> <x> <y> <z>`, the coordinates to three decimals; a cell stored as a bare
//! landblock (index 0) is written as the land cell its x and y fall in. The level limits are the
//! weenie's `MinLevel` and `MaxLevel`, `-` when it has none. A name (the weenie's `Name`, else its
//! class name) has each tab, line feed and carriage return replaced by a space, and each backslash
//! doubled.

use std::sync::{Arc, OnceLock};

use empyrean_content::models::world::{LandblockInstance, Weenie};
use empyrean_content::WorldDatabase;
use empyrean_entity::enums::{PositionType, PropertyInt, PropertyString, WeenieType};

/// The format's first line.
pub const HEADER: &str = "dereth-navigation-store\t1\n";

/// The table for one world, built once, on a thread of its own, from the moment it is made: the
/// status endpoint's own thread never builds it, so `/status` and `/health` never wait on it.
#[derive(Debug)]
pub struct PortalTable {
    document: Arc<OnceLock<String>>,
}

impl PortalTable {
    /// The table of `content`'s portals for the world named `world_name`, its building begun.
    #[must_use]
    pub fn new(world_name: &str, content: Arc<dyn WorldDatabase>) -> Self {
        let world_name = world_name.to_owned();
        Self::building(move || document(&world_name, &*content))
    }

    /// The table `build` makes, its building begun on a thread of its own (or, should no thread
    /// start, here and now).
    #[must_use]
    pub fn building(build: impl FnOnce() -> String + Send + 'static) -> Self {
        let document = Arc::new(OnceLock::new());
        let cell = Arc::clone(&document);
        let build = std::sync::Mutex::new(Some(build));
        let run = move || {
            if let Some(f) = build.lock().ok().and_then(|mut b| b.take()) {
                let _ = cell.set(f());
            }
        };
        let run = Arc::new(run);
        let on_thread = Arc::clone(&run);
        let spawned = std::thread::Builder::new()
            .name("Portal Table".to_owned())
            .spawn(move || on_thread());
        if let Err(e) = spawned {
            log::warn!("The portal table is built in place: no thread started ({e})");
            run();
        }
        Self { document }
    }

    /// The document once it is built; `None` while it is being built.
    #[must_use]
    pub fn ready(&self) -> Option<&str> {
        self.document.get().map(String::as_str)
    }

    /// The document, waiting for it to be built.
    #[must_use]
    pub fn document(&self) -> &str {
        self.document.wait()
    }
}

/// The portal table of `content` for the world named `world_name`.
#[must_use]
pub fn document(world_name: &str, content: &dyn WorldDatabase) -> String {
    let mut lines: Vec<((u32, u32, u32), String)> = content
        .get_landblock_instances_of_type(WeenieType::Portal)
        .iter()
        .filter_map(|(instance, weenie)| portal_line(instance, weenie))
        .collect();
    lines.sort();
    let mut out = String::from(HEADER);
    out.push_str("world\t");
    out.push_str(&field(world_name));
    out.push('\n');
    for (_, line) in lines {
        out.push_str(&line);
    }
    out
}

/// One instance's line and its sort key (from cell, class id, guid); `None` without a destination.
fn portal_line(instance: &LandblockInstance, weenie: &Weenie) -> Option<((u32, u32, u32), String)> {
    let destination = weenie
        .weenie_properties_position
        .iter()
        .find(|p| p.position_type == PositionType::Destination.0 && p.obj_cell_id != 0)?;
    let from_cell = land_cell(instance.obj_cell_id, instance.origin_x, instance.origin_y);
    let from = position(
        from_cell,
        instance.origin_x,
        instance.origin_y,
        instance.origin_z,
    );
    let to = position(
        land_cell(
            destination.obj_cell_id,
            destination.origin_x,
            destination.origin_y,
        ),
        destination.origin_x,
        destination.origin_y,
        destination.origin_z,
    );
    let name = weenie
        .weenie_properties_string
        .iter()
        .find(|p| p.r#type == PropertyString::Name.0)
        .map_or(weenie.class_name.as_str(), |p| p.value.as_str());
    let int = |p: PropertyInt| {
        weenie
            .weenie_properties_int
            .iter()
            .find(|r| r.r#type == p.0)
            .map_or_else(|| "-".to_owned(), |r| r.value.to_string())
    };
    let line = format!(
        "portal\tpublished\t{}\t{}\t{from}\t{to}\t0\t0\t0\t{}\t{}\t-\n",
        instance.weenie_class_id,
        field(name),
        int(PropertyInt::MinLevel),
        int(PropertyInt::MaxLevel),
    );
    Some(((from_cell, instance.weenie_class_id, instance.guid), line))
}

/// `cell`, or for a bare landblock (cell index 0) the outdoor cell that `x`, `y` fall in: cells are
/// 24 units square, eight to a side, numbered from 1 by column then row.
fn land_cell(cell: u32, x: f32, y: f32) -> u32 {
    if cell & 0xFFFF != 0 {
        return cell;
    }
    #[allow(clippy::cast_possible_truncation)]
    let axis = |v: f32| (f64::from(v) / 24.0).floor().clamp(0.0, 7.0) as u32;
    (cell & 0xFFFF_0000) | (axis(x) * 8 + axis(y) + 1)
}

fn position(cell: u32, x: f32, y: f32, z: f32) -> String {
    format!("0x{cell:08X} {x:.3} {y:.3} {z:.3}")
}

/// A text field: tab, line feed and carriage return become spaces; a backslash is doubled.
fn field(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\t' | '\n' | '\r' => out.push(' '),
            '\\' => out.push_str("\\\\"),
            c => out.push(c),
        }
    }
    out
}
