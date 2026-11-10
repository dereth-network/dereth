//! Not ACE: the world's portal table and its places, published by the status endpoint as
//! `GET /portals` and `GET /places` when `server.publish_portals` is on.
//!
//! ACE publishes nothing like them. A client that routes a character through portals needs to know
//! where each placed portal stands, where it leads and who may use it; a client that draws a map of
//! the world, or lists where to go, needs its lifestones, vendors and people too. These are those
//! lists, read from the world content the server loaded (its landblock instances and their
//! weenies), never from the live world, so no landblock is loaded to build them, and no player,
//! nothing a player carries and no house owner is ever in them.
//!
//! Both documents are tab-separated text with LF line endings (`→` marks a tab). The portal table:
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
//!
//! The places:
//!
//! ```text
//! dereth-places-store→1
//! world→<world name>
//! place→<kind>→published→<wcid>→<name>→<at>→0→0→-→<to or ->
//! ```
//!
//! There is one `place` line per landblock instance whose weenie is a lifestone (`lifestone`), a
//! portal (`portal`, with where it leads when its weenie says), a vendor (`vendor`), or a creature
//! that cannot be attacked (`npc`), leaving out any whose weenie hides it from the interface. They
//! are ordered by cell, then kind in that order, then weenie class id, then instance guid. Positions
//! and names are written as the portal table writes them. The two zeros are when the place was
//! first and last seen, which a server does not know, and the `-` is a house's owner, which it
//! never names.

use std::sync::{Arc, OnceLock};

use empyrean_content::models::world::{LandblockInstance, Weenie};
use empyrean_content::WorldDatabase;
use empyrean_entity::enums::{PositionType, PropertyBool, PropertyInt, PropertyString, WeenieType};

/// The portal table's first line.
pub const HEADER: &str = "dereth-navigation-store\t1\n";

/// The places document's first line.
pub const PLACES_HEADER: &str = "dereth-places-store\t1\n";

/// What a world publishes: its portal table and its places.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Documents {
    pub portals: String,
    pub places: String,
}

impl Documents {
    /// Both documents of `content` for the world named `world_name`.
    #[must_use]
    pub fn of(world_name: &str, content: &dyn WorldDatabase) -> Self {
        Self {
            portals: document(world_name, content),
            places: places_document(world_name, content),
        }
    }
}

/// The documents for one world, built once, on a thread of their own, from the moment they are
/// made: the status endpoint's own thread never builds them, so `/status` and `/health` never wait
/// on them.
#[derive(Debug)]
pub struct PortalTable {
    documents: Arc<OnceLock<Documents>>,
}

impl PortalTable {
    /// The portal table and places of `content` for the world named `world_name`, their building
    /// begun.
    #[must_use]
    pub fn new(world_name: &str, content: Arc<dyn WorldDatabase>) -> Self {
        let world_name = world_name.to_owned();
        Self::building(move || Documents::of(&world_name, &*content))
    }

    /// The documents `build` makes, their building begun on a thread of its own (or, should no
    /// thread start, here and now).
    #[must_use]
    pub fn building(build: impl FnOnce() -> Documents + Send + 'static) -> Self {
        let documents = Arc::new(OnceLock::new());
        let cell = Arc::clone(&documents);
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
        Self { documents }
    }

    /// The portal table once it is built; `None` while it is being built.
    #[must_use]
    pub fn ready(&self) -> Option<&str> {
        self.documents.get().map(|d| d.portals.as_str())
    }

    /// The places once they are built; `None` while they are being built.
    #[must_use]
    pub fn places_ready(&self) -> Option<&str> {
        self.documents.get().map(|d| d.places.as_str())
    }

    /// The portal table, waiting for it to be built.
    #[must_use]
    pub fn document(&self) -> &str {
        &self.documents.wait().portals
    }

    /// The places, waiting for them to be built.
    #[must_use]
    pub fn places(&self) -> &str {
        &self.documents.wait().places
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
    let to = destination(weenie)?;
    let from_cell = land_cell(instance.obj_cell_id, instance.origin_x, instance.origin_y);
    let from = position(
        from_cell,
        instance.origin_x,
        instance.origin_y,
        instance.origin_z,
    );
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
        field(name(weenie)),
        int(PropertyInt::MinLevel),
        int(PropertyInt::MaxLevel),
    );
    Some(((from_cell, instance.weenie_class_id, instance.guid), line))
}

/// What a place is, in the order the places document lists them within a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum PlaceKind {
    Lifestone,
    Portal,
    Vendor,
    Npc,
}

impl PlaceKind {
    const fn word(self) -> &'static str {
        match self {
            Self::Lifestone => "lifestone",
            Self::Portal => "portal",
            Self::Vendor => "vendor",
            Self::Npc => "npc",
        }
    }
}

/// The item type's creature bit.
const CREATURE: i32 = 0x10;

/// The places of `content` for the world named `world_name`.
#[must_use]
pub fn places_document(world_name: &str, content: &dyn WorldDatabase) -> String {
    let kinds = [
        (WeenieType::LifeStone, PlaceKind::Lifestone),
        (WeenieType::Portal, PlaceKind::Portal),
        (WeenieType::Vendor, PlaceKind::Vendor),
        (WeenieType::Creature, PlaceKind::Npc),
        (WeenieType::Cow, PlaceKind::Npc),
    ];
    let mut lines: Vec<((u32, PlaceKind, u32, u32), String)> = Vec::new();
    for (weenie_type, kind) in kinds {
        lines.extend(
            content
                .get_landblock_instances_of_type(weenie_type)
                .iter()
                .filter_map(|(instance, weenie)| place_line(kind, instance, weenie)),
        );
    }
    lines.sort();
    let mut out = String::from(PLACES_HEADER);
    out.push_str("world\t");
    out.push_str(&field(world_name));
    out.push('\n');
    for (_, line) in lines {
        out.push_str(&line);
    }
    out
}

/// One instance's place line and its sort key (cell, kind, class id, guid); `None` for a weenie
/// hidden from the interface, and for an `npc` that is no creature or may be attacked.
fn place_line(
    kind: PlaceKind,
    instance: &LandblockInstance,
    weenie: &Weenie,
) -> Option<((u32, PlaceKind, u32, u32), String)> {
    let flag = |p: PropertyBool| {
        weenie
            .weenie_properties_bool
            .iter()
            .find(|r| r.r#type == p.0)
            .map(|r| r.value)
    };
    if flag(PropertyBool::UiHidden) == Some(true) {
        return None;
    }
    if kind == PlaceKind::Npc {
        let item_type = weenie
            .weenie_properties_int
            .iter()
            .find(|r| r.r#type == PropertyInt::ItemType.0)
            .map_or(0, |r| r.value);
        // A creature may be attacked unless its weenie says it may not.
        if item_type & CREATURE == 0 || flag(PropertyBool::Attackable) != Some(false) {
            return None;
        }
    }
    let cell = land_cell(instance.obj_cell_id, instance.origin_x, instance.origin_y);
    let at = position(
        cell,
        instance.origin_x,
        instance.origin_y,
        instance.origin_z,
    );
    let to = if kind == PlaceKind::Portal {
        destination(weenie)
    } else {
        None
    };
    let line = format!(
        "place\t{}\tpublished\t{}\t{}\t{at}\t0\t0\t-\t{}\n",
        kind.word(),
        instance.weenie_class_id,
        field(name(weenie)),
        to.as_deref().unwrap_or("-"),
    );
    Some(((cell, kind, instance.weenie_class_id, instance.guid), line))
}

/// Where a portal's weenie leads; `None` when it says nowhere (the destination is set at run time).
fn destination(weenie: &Weenie) -> Option<String> {
    let d = weenie
        .weenie_properties_position
        .iter()
        .find(|p| p.position_type == PositionType::Destination.0 && p.obj_cell_id != 0)?;
    Some(position(
        land_cell(d.obj_cell_id, d.origin_x, d.origin_y),
        d.origin_x,
        d.origin_y,
        d.origin_z,
    ))
}

/// The weenie's `Name`, else its class name.
fn name(weenie: &Weenie) -> &str {
    weenie
        .weenie_properties_string
        .iter()
        .find(|p| p.r#type == PropertyString::Name.0)
        .map_or(weenie.class_name.as_str(), |p| p.value.as_str())
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
