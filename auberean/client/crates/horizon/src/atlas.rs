//! What the map window draws over its picture ([`crate::world_map`]), from the game's records and
//! its view: the land's size, the continents' names where the game's own map writes them, the
//! closer maps' labels, the markers the scripts put on the map and the player's fellows, where the
//! player stands, and the game's date and time.
//!
//! Places on the map are in land cells: across from the world's west edge, and up from its south
//! edge.

use std::sync::Arc;

use auberean_assets::region::{DetailMap, MapNotes};
use auberean_client_sdk::contract::view::{MapMarkers, View};
use auberean_client_sdk::text::{StringInfo, Value};
use auberean_ui::token::Token;

use crate::art::Records;
use crate::state::GameState;

/// A land block's side, in land cells.
pub const BLOCK_CELLS: f32 = 8.0;

/// The calendar's string table, by the enumeration the game's records map it by; and that map.
const CALENDAR_TABLE: u32 = 0x4000_0002;
const TABLE_MAPPING: u32 = 0x4000_0004;

/// A rectangle of land, in land cells: its west and south edges, and its width and height.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Land {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Land {
    /// Whether it and `other` overlap.
    #[must_use]
    pub fn meets(&self, other: &Self) -> bool {
        self.x < other.x + other.w
            && other.x < self.x + self.w
            && self.y < other.y + other.h
            && other.y < self.y + self.h
    }

    /// Whether the place `(x, y)` is in it.
    #[must_use]
    pub fn holds(&self, (x, y): (f32, f32)) -> bool {
        (self.x..self.x + self.w).contains(&x) && (self.y..self.y + self.h).contains(&y)
    }
}

/// A label of the closer maps: where it is, its picture, its words (once written) and the map
/// coordinates of its place.
#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    pub at: (f32, f32),
    pub icon: u32,
    pub words: Option<String>,
    pub coordinates: String,
    /// What its words are written from: its string table and its entry.
    pub text: (u32, u32),
}

/// A continent's name and where the game's own world map writes it, in land cells.
#[derive(Clone, Debug, PartialEq)]
pub struct Continent {
    pub name: String,
    pub at: (f32, f32),
}

/// What the map draws over its picture, from the world's region.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Atlas {
    /// The region's map record, which tells one world's atlas from another's.
    pub world: u32,
    /// The land's size, in land cells across and up.
    pub land: (i32, i32),
    /// The continents, as the game's map window names them (none while its words are not at
    /// hand).
    pub continents: Vec<Continent>,
    pub labels: Vec<Label>,
}

/// A marker on the map, as the map window shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Marker {
    pub kind: u32,
    pub object: u64,
    /// Where it stands: the middle of its land cell.
    pub at: (f32, f32),
    pub picture: u32,
    pub words: String,
    pub coordinates: String,
}

/// What the map window shows of the game.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapState {
    pub atlas: Option<Arc<Atlas>>,
    /// The markers the scripts put on the map.
    pub markers: Vec<Marker>,
    /// The player's fellows where each stands, as markers of the fellows' kind, unless the player
    /// keeps them off the map.
    pub fellows: Vec<Marker>,
    /// Where the player stands outdoors, and the way it faces (degrees clockwise from north);
    /// none inside.
    pub player: Option<((f32, f32), f32)>,
    /// The player stands inside (a building, a dungeon).
    pub inside: bool,
    /// The game's date and time of day, as the game writes them.
    pub date: Option<String>,
    pub time: Option<String>,
    /// The markers' version last followed.
    markers_version: Option<u64>,
    /// Frames since the labels whose words are not written yet were last tried.
    labels_tried: u32,
}

/// The kind of marker the fellows are.
pub const FELLOW_MARKERS: u32 = 4;

/// The player's option that keeps fellows off the map.
pub const FELLOWS_OFF_MAP: u32 = 0x8000;

/// The fellows' marker's picture, by its value in the game's records' picture mapping, and that
/// mapping.
const FELLOW_PICTURE: u32 = 0x4100_0010;
const PICTURE_MAPPING: u32 = 0x4000_0003;

/// The fellows' markers' picture, from the game's records.
#[must_use]
pub fn fellow_picture(records: &Records) -> u32 {
    records.data_id_by_enum(FELLOW_PICTURE, PICTURE_MAPPING)
}

/// The player's fellows on the map, as the fellowship puts them there: each fellow but the player
/// at its cell, named, with `picture`, put by the map's rules for markers in the fellowship's
/// order; none while the player's options keep fellows off the map (or are not here yet).
#[must_use]
pub fn fellow_markers(
    view: &View,
    land: (i32, i32),
    picture: u32,
    write: &dyn Fn(&auberean_client_sdk::contract::text::TextRef) -> Option<String>,
) -> Vec<Marker> {
    let (Some(f), Some(o)) = (&view.social.fellowship, &view.options) else {
        return Vec::new();
    };
    if o.bits & FELLOWS_OFF_MAP != 0 {
        return Vec::new();
    }
    let mut kept = MapMarkers::default();
    for (id, fellow) in &f.fellows {
        if Some(*id) == view.player_id {
            continue;
        }
        kept.add(
            auberean_client_sdk::contract::view::MapMarker {
                kind: FELLOW_MARKERS,
                object: *id,
                cell: fellow.cell,
                picture,
                label: Some(fellow.name.clone()),
            },
            land,
        );
    }
    kept.markers
        .iter()
        .filter_map(|m| {
            Some(Marker {
                kind: m.kind,
                object: m.object,
                at: cell_middle(m.cell, land)?,
                picture: m.picture,
                words: m.label.as_ref().and_then(write).unwrap_or_default(),
                coordinates: coordinates(m.cell, land),
            })
        })
        .collect()
}

/// A land cell's map coordinates as the game writes them: north or south, then east or west.
#[must_use]
pub fn coordinates(cell: u32, land: (i32, i32)) -> String {
    auberean_client_sdk::text::place::real_map_coords(cell, land)
}

/// The middle of a land cell, in land cells; none for a cell that is not one of the land's.
#[must_use]
pub fn cell_middle(cell: u32, land: (i32, i32)) -> Option<(f32, f32)> {
    let (x, y) = auberean_primitives::land::land_coords(cell, land)?;
    #[allow(clippy::cast_precision_loss)]
    Some((x as f32 + 0.5, y as f32 + 0.5))
}

/// Where a body stands, in land cells: its block's corner and its place in the block; none in an
/// interior cell.
#[must_use]
pub fn body_place(cell: u32, origin: [f32; 3]) -> Option<(f32, f32)> {
    if cell & 0xFFFF >= 0x100 {
        return None;
    }
    let block = |b: u32| f32::from(u8::try_from(b & 0xFF).unwrap_or(0)) * BLOCK_CELLS;
    let per_cell = auberean_client_sdk::contract::view::BLOCK_SIDE / BLOCK_CELLS;
    Some((
        block(cell >> 24) + origin[0] / per_cell,
        block(cell >> 16) + origin[1] / per_cell,
    ))
}

/// The game's map window's layout, by its enumeration value; the fields its world map picture is
/// drawn in, outermost first; and its buttons for the continents, each over the picture where the
/// picture writes that continent's name, its tooltip naming it.
const MAP_LAYOUT: u32 = 0x4100_0003;
const LAYOUT_MAPPING: u32 = 0x4000_0003;
const MAP_FIELDS: [&str; 3] = ["ParentField", "MapParentField", "MapField"];
const CONTINENT_BUTTONS: [&str; 5] = [
    "NavigationZoomToOverviewOstethButton",
    "NavigationZoomToOverviewOmishanButton",
    "NavigationZoomToOverviewLinvakMassifButton",
    "NavigationZoomToOverviewArramoraButton",
    "NavigationZoomToOverviewKnorrButton",
];

/// A rectangle of the layout: left, top, right, bottom, in its pixels.
type Box4 = [f32; 4];

/// Where element `id` lies in the layout, its rectangle placed in its parents' (an edge given as
/// -1 is its parent's), and the element; searched through every state's children.
fn place_of(
    elements: &[auberean_assets::ui::Element],
    parent: Box4,
    id: u32,
) -> Option<(Box4, &auberean_assets::ui::Element)> {
    elements.iter().find_map(|e| {
        let edge = |v: i16, own: f32, far: f32| if v < 0 { far } else { own + f32::from(v) };
        let r = [
            edge(e.left, parent[0], parent[0]),
            edge(e.top, parent[1], parent[1]),
            edge(e.right, parent[0], parent[2]),
            edge(e.bottom, parent[1], parent[3]),
        ];
        if e.id == id {
            return Some((r, e));
        }
        e.states.iter().find_map(|s| place_of(&s.children, r, id))
    })
}

/// The continents the game's map window names: each button for a continent stands over the world
/// map's picture where the picture writes its name, and its tooltip names it in a sentence the
/// buttons share (the name is what differs). Each is put on `land` where its button stands over
/// the picture, which shows the whole land. None when the layout or its words are not at hand.
#[must_use]
pub fn continents(records: &Records, land: (i32, i32)) -> Vec<Continent> {
    use auberean_ui::token::attr::TOOLTIP_ENTRY;
    let Some(layout) = records.layout(records.data_id_by_enum(MAP_LAYOUT, LAYOUT_MAPPING)) else {
        return Vec::new();
    };
    let top = [0.0, 0.0, f32::from(layout.width), f32::from(layout.height)];
    // The field the picture fills, each inside the one before.
    let Some(mut at) = place_of(&layout.elements, top, Token::of(MAP_FIELDS[0]).raw()) else {
        return Vec::new();
    };
    for name in &MAP_FIELDS[1..] {
        let (r, e) = at;
        let inner = e
            .states
            .iter()
            .find_map(|s| place_of(&s.children, r, Token::of(name).raw()));
        let Some(inner) = inner else {
            return Vec::new();
        };
        at = inner;
    }
    let field = at.0;
    let mut found: Vec<(String, (f32, f32))> = Vec::new();
    for name in CONTINENT_BUTTONS {
        let Some((r, e)) = place_of(&layout.elements, top, Token::of(name).raw()) else {
            continue;
        };
        let tip = e
            .states
            .iter()
            .flat_map(|s| &s.attributes)
            .find_map(|a| match a.value {
                auberean_assets::ui::AttributeValue::StringToken(t)
                    if a.id == TOOLTIP_ENTRY.raw() =>
                {
                    Some(t)
                }
                _ => None,
            });
        let Some(words) =
            tip.and_then(|t| records.write(&StringInfo::entry(layout.string_table.0, t)))
        else {
            continue;
        };
        let (w, h) = (
            (field[2] - field[0]).max(1.0),
            (field[3] - field[1]).max(1.0),
        );
        let (cx, cy) = ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0);
        #[allow(clippy::cast_precision_loss)]
        let at = (
            ((cx - field[0]) / w).clamp(0.0, 1.0) * land.0 as f32,
            (1.0 - ((cy - field[1]) / h).clamp(0.0, 1.0)) * land.1 as f32,
        );
        found.push((words, at));
    }
    names_in(&found)
}

/// The continents put on the land itself: the game's world map is a painting, its names beside
/// the land rather than on it, so each name goes to the middle of one of the largest stretches of
/// land, as many of them as there are names, matched so the names move least in all. As the game
/// writes them while the land is not known (fewer stretches than names).
#[must_use]
pub fn on_the_land(
    continents: &[Continent],
    masses: &[crate::world_map::Landmass],
    land: (i32, i32),
) -> Vec<Continent> {
    let n = continents.len();
    if n == 0 || masses.len() < n || n > 8 {
        return continents.to_vec();
    }
    #[allow(clippy::cast_precision_loss)]
    let (w, h) = (land.0.max(1) as f32, land.1.max(1) as f32);
    let far = |c: &Continent, m: &crate::world_map::Landmass| {
        dereth_primitives::num::math::hypotf(c.at.0 / w - m.centre.0, c.at.1 / h - m.centre.1)
    };
    // Every way of giving each name its own stretch, the one that moves the names least.
    fn best(
        at: usize,
        used: &mut Vec<bool>,
        cost: &dyn Fn(usize, usize) -> f32,
        n: usize,
        way: &mut Vec<usize>,
        found: &mut (f32, Vec<usize>),
    ) {
        if at == n {
            let total: f32 = way.iter().enumerate().map(|(c, m)| cost(c, *m)).sum();
            if total < found.0 {
                *found = (total, way.clone());
            }
            return;
        }
        for m in 0..n {
            if !used[m] {
                used[m] = true;
                way.push(m);
                best(at + 1, used, cost, n, way, found);
                way.pop();
                used[m] = false;
            }
        }
    }
    let cost = |c: usize, m: usize| far(&continents[c], &masses[m]);
    let mut found = (f32::INFINITY, Vec::new());
    best(
        0,
        &mut vec![false; n],
        &cost,
        n,
        &mut Vec::new(),
        &mut found,
    );
    continents
        .iter()
        .zip(found.1)
        .map(|(c, m)| Continent {
            name: c.name.clone(),
            at: (masses[m].centre.0 * w, masses[m].centre.1 * h),
        })
        .collect()
}

/// Each sentence's own words: what is left of it past the words all of `sentences` begin and end
/// with, cut at whole words; none from fewer than two sentences, or one left with nothing.
fn names_in(sentences: &[(String, (f32, f32))]) -> Vec<Continent> {
    if sentences.len() < 2 {
        return Vec::new();
    }
    let words: Vec<Vec<&str>> = sentences
        .iter()
        .map(|(s, _)| s.split_whitespace().collect())
        .collect();
    let shortest = words.iter().map(Vec::len).min().unwrap_or(0);
    let first = &words[0];
    let mut front = 0;
    while front < shortest && words.iter().all(|w| w[front] == first[front]) {
        front += 1;
    }
    let mut back = 0;
    while back < shortest - front
        && words
            .iter()
            .all(|w| w[w.len() - 1 - back] == first[first.len() - 1 - back])
    {
        back += 1;
    }
    let mut out = Vec::new();
    for (w, (_, at)) in words.iter().zip(sentences) {
        let name = w[front..w.len() - back].join(" ");
        if name.is_empty() {
            return Vec::new();
        }
        out.push(Continent { name, at: *at });
    }
    out
}

/// The land's size, the continents and the closer maps' labels, read from the world's records,
/// the continents' names and the labels' words written from `records` where it can.
#[must_use]
pub fn read(world: &auberean_client_sdk::sim::WorldData, records: Option<&Records>) -> Atlas {
    let region = &world.region.region;
    let cells = |blocks: u32| i32::try_from(blocks * 8).unwrap_or(0);
    let land = (cells(region.blocks_wide), cells(region.blocks_long));
    let mut atlas = Atlas {
        world: region.map.0,
        land,
        continents: records.map_or_else(Vec::new, |r| continents(r, land)),
        ..Atlas::default()
    };
    let Some(detail) = region
        .detail_maps
        .first()
        .and_then(|t| world.records.get::<DetailMap>(t.record.0))
    else {
        return atlas;
    };
    #[allow(clippy::cast_precision_loss)]
    let f = |v: u32| v as f32;
    let mut read_notes: Vec<u32> = Vec::new();
    for row in &detail.rows {
        for id in &row.notes {
            if read_notes.contains(id) {
                continue;
            }
            read_notes.push(*id);
            let Some(notes) = world.records.get::<MapNotes>(*id) else {
                continue;
            };
            for n in &notes.notes {
                let (x, y) = (
                    i32::try_from(n.x).unwrap_or(-1),
                    i32::try_from(n.y).unwrap_or(-1),
                );
                let cell = auberean_primitives::land::cell_at(x, y, land);
                atlas.labels.push(Label {
                    at: (f(n.x) + 0.5, f(n.y) + 0.5),
                    icon: n.icon.0,
                    words: None,
                    coordinates: coordinates(cell, land),
                    text: (notes.strings.0, n.text),
                });
            }
        }
    }
    write_labels(&mut atlas, records);
    atlas
}

/// The labels' words written where they are not yet; whether any is still not.
fn write_labels(atlas: &mut Atlas, records: Option<&Records>) -> bool {
    let Some(records) = records else {
        return true;
    };
    let mut missing = false;
    for l in atlas.labels.iter_mut().filter(|l| l.words.is_none()) {
        let mut info = StringInfo::entry(l.text.0, l.text.1);
        info.add_var(
            Token::of("ID_Navigation_MapCoordinates").raw(),
            Value::Text(Box::new(StringInfo::literal(&l.coordinates))),
        );
        l.words = records
            .write(&info)
            .filter(|w| !w.trim().is_empty())
            .map(|w| w.trim().to_owned());
        missing |= l.words.is_none();
    }
    missing
}

/// The game's date and time of day at world time `t`, by the region's calendar, as the game
/// writes them.
#[must_use]
pub fn date_and_time(
    world: &auberean_client_sdk::sim::WorldData,
    records: &Records,
    t: f64,
) -> Option<(String, String)> {
    let cal = world.region.calendar.as_ref()?;
    let d = auberean_world::calendar::date(cal, t)?;
    let table = records.data_id_by_enum(CALENDAR_TABLE, TABLE_MAPPING);
    let token = |s: &str| Token::of(s).raw();
    let named = |id: u32| {
        Value::Text(Box::new(StringInfo {
            string_id: id,
            ..StringInfo::default()
        }))
    };
    let mut date = StringInfo::entry(table, token("ID_DateString"));
    let mut time = StringInfo::entry(table, token("ID_TimeString"));
    time.add_var(token("TIME"), named(d.part_name));
    date.add_var(
        token("YEAR"),
        Value::Int(i32::try_from(d.year).unwrap_or(0)),
    );
    date.add_var(token("YEARSPEC"), named(d.year_name));
    date.add_var(token("DAY"), Value::Int(i32::try_from(d.day).unwrap_or(0)));
    date.add_var(token("SEASON"), named(d.season_name));
    Some((records.write(&date)?, records.write(&time)?))
}

/// What a world frame brings the map: the world's records and the strings, and the view.
#[derive(Clone, Copy)]
pub struct WorldFrame<'a> {
    pub world: &'a auberean_client_sdk::sim::WorldData,
    pub records: Option<&'a Records>,
    pub view: Option<&'a View>,
    /// A marker's words written out; none while its strings are not at hand.
    pub write: &'a dyn Fn(&auberean_client_sdk::contract::text::TextRef) -> Option<String>,
    /// How many times the player's body has come to be placed in the world.
    pub arrivals: u32,
}

impl std::fmt::Debug for WorldFrame<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorldFrame").finish_non_exhaustive()
    }
}

impl MapState {
    /// The markers from the view's, as the map window shows them.
    pub fn follow_markers(
        &mut self,
        markers: &MapMarkers,
        land: (i32, i32),
        write: &dyn Fn(&auberean_client_sdk::contract::text::TextRef) -> Option<String>,
    ) {
        if self.markers_version == Some(markers.version)
            && self.markers.iter().all(|m| !m.words.is_empty())
        {
            return;
        }
        self.markers_version = Some(markers.version);
        self.markers = markers
            .markers
            .iter()
            .filter_map(|m| {
                Some(Marker {
                    kind: m.kind,
                    object: m.object,
                    at: cell_middle(m.cell, land)?,
                    picture: m.picture,
                    words: m.label.as_ref().and_then(write).unwrap_or_default(),
                    coordinates: coordinates(m.cell, land),
                })
            })
            .collect();
    }
}

impl GameState {
    /// The map window's part of the game: the region's pictures (read once), the markers, where
    /// the player stands, the date and the time.
    pub fn follow_world(&mut self, f: &WorldFrame<'_>) {
        self.journal.arrived = self.journal.arrivals != f.arrivals;
        self.journal.arrivals = f.arrivals;
        let map = &mut self.map;
        let region_map = f.world.region.region.map.0;
        match &mut map.atlas {
            Some(a) if a.world == region_map => {
                map.labels_tried += 1;
                let unwritten = a.labels.iter().any(|l| l.words.is_none());
                if map.labels_tried >= 60 && (unwritten || a.continents.is_empty()) {
                    map.labels_tried = 0;
                    let a = Arc::make_mut(a);
                    write_labels(a, f.records);
                    if a.continents.is_empty() {
                        a.continents = f.records.map_or_else(Vec::new, |r| continents(r, a.land));
                    }
                }
            }
            _ => map.atlas = Some(Arc::new(read(f.world, f.records))),
        }
        let land = map.atlas.as_ref().map_or((0, 0), |a| a.land);
        let Some(view) = f.view else {
            map.markers.clear();
            map.fellows.clear();
            map.player = None;
            return;
        };
        map.follow_markers(&view.markers, land, f.write);
        let picture = f.records.map_or(0, fellow_picture);
        map.fellows = fellow_markers(view, land, picture, f.write);
        let body = view.player.as_ref().map(|p| p.body);
        map.inside = body.is_some_and(|b| b.cell & 0xFFFF >= 0x100);
        map.player = body.and_then(|b| Some((body_place(b.cell, b.origin)?, b.heading)));
        if let Some(r) = f.records {
            if let Some((d, t)) = date_and_time(f.world, r, view.clock.world_time) {
                map.date = Some(d);
                map.time = Some(t);
            }
        }
    }
}
