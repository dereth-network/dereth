//! The building catalog: every building model the world places, gathered from every copy of it,
//! with what a copy dropped somewhere else takes along.
//!
//! For each model:
//! - its **layouts**: the interior a copy has. A copy's rooms are the cells its doorways' lists
//!   name; a layout is each room's piece, surfaces and place in the building's frame, and the ways
//!   between the rooms and out of them. Nearly every model has one layout; a few have a second
//!   with other surfaces, and an entrance over a dungeon has one for each copy.
//! - each layout's **example**, the copy a drop is taken from, and its **furnishings**: the
//!   statics its rooms hold, which vary from copy to copy, each named by the town of a copy that
//!   has it and counted.
//! - its **doors**, as the server's world database stands them at the example: each one's weenie
//!   class, its frame in the building's, and the room it stands in, or the land cell outside. A
//!   model's copies have the same doors in the same places, nearly always.
//! - its **outdoor statics**: those of the landblock standing at the shell that most of its
//!   copies have (steps, a porch, a chimney's smoke), in the building's frame.
//! - its **kind**: a cottage, villa or mansion (a house: its rooms belong to one), another
//!   building, a shell with nothing inside, or an entrance over a dungeon.
//! - its **set**: the models that stand with it wherever it stands, as a mansion's four do. A set
//!   is dropped whole, from its lead (the lowest model id of it).
//! - its **reach**: how far its shell reaches past the land cell holding its origin, at most, over
//!   every copy.
//! - `num_leaves`, which a copy takes from a copy of the same model.
//! - for a house, what makes its example one players can buy, from the server's world database:
//!   the house of each building of its set (the building whose origin's land cell the landblock's
//!   restriction table gives a house), everything those link (slumlord, storage, hooks, boot spot,
//!   house portals and their link spots), each in the frame of its building and in its room or
//!   outdoors, and, where a house portal leads to a basement of its own, that dungeon's landblock
//!   and every spawn in it.
//!
//! Reading it takes several seconds, so it is kept in a file ([`Catalog::save`]).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Write as _};
use std::path::Path;

use dereth_assets::world::{BuildInfo, EnvCell, LandblockInfo};
use dereth_assets::{Decode, Environment};
use dereth_primitives::frame::{combine, get_heading, globaltolocal, localtoglobal};
use dereth_primitives::{DataId, EraId, Frame, Quat, Vec3};

use crate::cells::inverse;
use crate::dats::DatSet;
use crate::pick::Models;
use crate::sql_ids::ServerContent;

/// A building of the files: its landblock and its place in that landblock's building list,
/// written `3B9C/3`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Site {
    pub landblock: u16,
    pub index: usize,
}

impl Site {
    /// A site as written, `3B9C/3`.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let (lb, i) = s.trim().split_once('/')?;
        let landblock = (lb.len() == 4)
            .then(|| u16::from_str_radix(lb, 16).ok())
            .flatten()?;
        Some(Self {
            landblock,
            index: i.parse().ok()?,
        })
    }
}

impl fmt::Display for Site {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04X}/{}", self.landblock, self.index)
    }
}

/// What kind of building a model is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    Cottage,
    Villa,
    Mansion,
    /// A building with rooms that is no house.
    Building,
    /// A shell with no rooms and no doorways.
    Shell,
    /// A building whose rooms lead on into a dungeon under it.
    Entrance,
}

impl Kind {
    pub const ALL: [Self; 6] = [
        Self::Cottage,
        Self::Villa,
        Self::Mansion,
        Self::Building,
        Self::Shell,
        Self::Entrance,
    ];

    /// What a gallery calls it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Cottage => "Cottage",
            Self::Villa => "Villa",
            Self::Mansion => "Mansion",
            Self::Building => "Building",
            Self::Shell => "Shell",
            Self::Entrance => "Dungeon entrance",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.name() == s)
    }
}

/// A door standing with a building.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Door {
    /// Its weenie class.
    pub weenie: u32,
    /// Which building of the set it stands with: 0 the model's own, `k` the set's `k`th partner.
    pub member: usize,
    /// Its frame in the frame of the building it stands with.
    pub frame: Frame,
    /// The room it stands in, by its cell number in that building's example; `None` for the land
    /// cell outside.
    pub room: Option<u16>,
}

/// One way a layout's rooms are furnished.
#[derive(Clone, Debug, PartialEq)]
pub struct Furnishing {
    /// A copy furnished so: one in a town when any is.
    pub example: Site,
    /// That copy's town.
    pub town: Option<String>,
    /// How many copies are furnished so.
    pub count: u32,
    /// How many statics the rooms hold.
    pub statics: u32,
}

/// A static of the landblock that stands with a building.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outdoor {
    pub model: DataId,
    /// Its frame in the building's.
    pub frame: Frame,
}

/// One interior of a model.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    /// The copy a drop takes: one with the layout's most common doors, and of those one with its
    /// most common furnishing.
    pub example: Site,
    /// How many copies have it.
    pub count: u32,
    pub rooms: u32,
    pub doorways: u32,
    /// Most common first.
    pub furnishings: Vec<Furnishing>,
    /// The example's doors, its set's with them.
    pub doors: Vec<Door>,
    /// The statics most of its copies have at the shell, its set's with them, in the model's frame.
    pub outdoor: Vec<Outdoor>,
    /// What makes the example a house players can buy, when it is one and the server's world
    /// database was read.
    pub house: Option<House>,
}

/// Where one of a house's objects stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spot {
    /// In a room of the set, by its cell number in the example's landblock.
    Room(u16),
    /// Outdoors, by its building.
    Outdoors,
    /// In the house's basement, a dungeon of its own: in the cell of this number there.
    Basement(u16),
}

/// One spawn of the server's world that makes a building a house players can buy: a house, its
/// slumlord, storage, hooks, boot spot, house portals, and what else stands in its basement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HouseObject {
    /// Its weenie class.
    pub weenie: u32,
    /// Its weenie type.
    pub kind: u32,
    /// Which building of the set it stands with: 0 the model's own, `k` the set's `k`th partner.
    pub member: usize,
    /// Its frame in that building's frame; in the basement, in the basement landblock's.
    pub frame: Frame,
    pub spot: Spot,
    /// Whether it stands where the house portal that links it leads: its place is the other
    /// house portal's.
    pub beyond: bool,
    /// Whether another of the house's objects links it.
    pub link_child: bool,
}

/// What makes a model's example a house players can buy, as the server's world database has it.
#[derive(Clone, Debug, PartialEq)]
pub struct House {
    /// The example's house weenie class, and its property rows, which a new house's class copies.
    pub class: u32,
    pub rows: Vec<crate::sql_ids::HouseRow>,
    /// Its objects: the house of each building of the set first, in the set's order, then the
    /// rest.
    pub objects: Vec<HouseObject>,
    /// Which object links which, by their places in [`Self::objects`].
    pub links: Vec<(usize, usize)>,
    /// The retail dungeon landblock that is its basement, when it has one.
    pub basement: Option<u16>,
}

impl House {
    /// The weenie type of a house, a slumlord, storage, a boot spot and a house portal.
    pub const HOUSE: u32 = crate::sql_ids::HOUSE_TYPE;
    pub const SLUMLORD: u32 = 55;
    pub const STORAGE: u32 = 57;
    pub const BOOT_SPOT: u32 = 58;
    pub const PORTAL: u32 = 59;

    /// The place in [`Self::objects`] of the house of building `member` of the set.
    #[must_use]
    pub fn house_of(&self, member: usize) -> Option<usize> {
        self.objects.iter().position(|o| {
            o.kind == Self::HOUSE && o.member == member && !matches!(o.spot, Spot::Basement(_))
        })
    }

    /// The place of the house that holds the slumlord: the one the others answer to.
    #[must_use]
    pub fn root(&self) -> Option<usize> {
        self.links
            .iter()
            .find(|(p, c)| {
                self.objects[*p].kind == Self::HOUSE && self.objects[*c].kind == Self::SLUMLORD
            })
            .map(|(p, _)| *p)
    }
}

/// A model that stands with another wherever it stands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Partner {
    pub model: DataId,
    /// Its frame in the other's.
    pub frame: Frame,
}

/// One building model.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub model: DataId,
    /// How many copies the world places.
    pub count: u32,
    pub kind: Kind,
    pub num_leaves: u32,
    /// How far its shell reaches past the land cell holding its origin, at most, in meters.
    pub reach: f32,
    /// Most common first.
    pub layouts: Vec<Layout>,
    /// The models that stand with it wherever it stands, by id.
    pub set: Vec<Partner>,
}

impl Entry {
    /// Whether a drop of its set starts from it: it stands alone, or has the lowest id of its set.
    #[must_use]
    pub fn is_lead(&self) -> bool {
        self.set.iter().all(|p| p.model.raw() > self.model.raw())
    }
}

/// Every building model, by id.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Catalog {
    entries: BTreeMap<u32, Entry>,
    /// Whether the server's world database was read, and so the doors are known.
    pub doors_known: bool,
}

// ---------------------------------------------------------------------------------------------
// A layout, in the building's frame.
// ---------------------------------------------------------------------------------------------

/// How near two frames are the same: meters, and each quaternion component.
const SAME_PLACE: f32 = 0.02;
const SAME_TURN: f32 = 0.002;

/// Where a room's portal leads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Way {
    Out,
    /// Another room, by its place in the layout.
    Room(usize),
    /// A cell that is not one of the rooms: a dungeon's, under an entrance.
    Other(u16),
}

/// A room of a building, in the building's frame.
#[derive(Clone, Debug)]
pub(crate) struct Room {
    pub number: u16,
    pub environment: u32,
    pub piece: u16,
    pub surfaces: Vec<u32>,
    pub frame: Frame,
    /// Each portal: its flags, its polygon, where it leads and that portal's place there.
    pub portals: Vec<(u16, u16, Way, i16)>,
    pub statics: Vec<(u32, Frame)>,
}

/// A building's rooms and doorways, in its frame, its rooms in ascending order of number.
#[derive(Clone, Debug)]
pub(crate) struct Shape {
    pub rooms: Vec<Room>,
    /// Each doorway: its flags, the room it leads into and that room's portal.
    pub doorways: Vec<(u16, Way, i16)>,
}

/// How much of two layouts is compared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Level {
    /// The pieces, their places and the ways.
    Geometry,
    /// And the surfaces.
    Surfaces,
    /// And the rooms' statics.
    Statics,
}

/// The rooms of a building: the cells its doorways' lists name, and those its doorways lead
/// into, of `cells`.
#[must_use]
pub fn rooms(b: &BuildInfo, cells: &BTreeMap<u16, EnvCell>) -> BTreeSet<u16> {
    b.portals
        .iter()
        .flat_map(|p| p.stab_list.iter().copied().chain([p.other_cell_id]))
        .filter(|c| cells.contains_key(c))
        .collect()
}

/// A quaternion with `w` not negative.
fn canon(q: Quat) -> Quat {
    if q.w < 0.0 {
        Quat::new(-q.w, -q.x, -q.y, -q.z)
    } else {
        q
    }
}

fn near(a: Vec3, b: Vec3, t: f32) -> bool {
    (a.x - b.x).abs() <= t && (a.y - b.y).abs() <= t && (a.z - b.z).abs() <= t
}

fn turned_alike(a: Quat, b: Quat, t: f32) -> bool {
    let d = |s: f32| {
        (a.w - s * b.w).abs() <= t
            && (a.x - s * b.x).abs() <= t
            && (a.y - s * b.y).abs() <= t
            && (a.z - s * b.z).abs() <= t
    };
    d(1.0) || d(-1.0)
}

/// `f` in the frame `of` stands in.
fn relative(of: &Frame, f: &Frame) -> Frame {
    let r = combine(&inverse(of), f);
    Frame::new(r.origin, canon(r.rotation))
}

/// Building `b`'s rooms `rooms` of `cells`, in its frame.
pub(crate) fn shape(b: &BuildInfo, cells: &BTreeMap<u16, EnvCell>, rooms: &BTreeSet<u16>) -> Shape {
    let numbers: Vec<u16> = rooms
        .iter()
        .copied()
        .filter(|c| cells.contains_key(c))
        .collect();
    let place: BTreeMap<u16, usize> = numbers.iter().enumerate().map(|(i, c)| (*c, i)).collect();
    let way = |c: u16| place.get(&c).map_or(Way::Other(c), |i| Way::Room(*i));
    let rooms = numbers
        .iter()
        .map(|n| {
            let c = &cells[n];
            Room {
                number: *n,
                environment: c.environment.raw(),
                piece: c.cell_struct,
                surfaces: c.surfaces.iter().map(|s| s.raw()).collect(),
                frame: relative(&b.frame, &c.frame),
                portals: c
                    .portals
                    .iter()
                    .map(|p| {
                        #[allow(clippy::cast_possible_truncation)] // a cell's number is 16 bits
                        let to = if p.flags & 4 != 0 {
                            Way::Out
                        } else {
                            way(p.other_cell_id as u16)
                        };
                        (p.flags, p.polygon_id, to, p.other_portal_id)
                    })
                    .collect(),
                statics: c
                    .static_objects
                    .iter()
                    .map(|s| (s.id.raw(), relative(&b.frame, &s.frame)))
                    .collect(),
            }
        })
        .collect();
    Shape {
        rooms,
        doorways: b
            .portals
            .iter()
            .map(|p| (p.flags, way(p.other_cell_id), p.other_portal_id))
            .collect(),
    }
}

/// Whether two rooms' statics are the same, in any order.
fn same_statics(a: &[(u32, Frame)], b: &[(u32, Frame)]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut taken = vec![false; b.len()];
    a.iter().all(|(id, f)| {
        let k = (0..b.len()).find(|k| {
            !taken[*k]
                && b[*k].0 == *id
                && near(b[*k].1.origin, f.origin, SAME_PLACE)
                && turned_alike(b[*k].1.rotation, f.rotation, SAME_TURN)
        });
        k.is_some_and(|k| {
            taken[k] = true;
            true
        })
    })
}

/// Which room of `b` each room of `a` is, when the two are one layout at `level`: the same
/// pieces in the same places, joined the same way.
pub(crate) fn matching(a: &Shape, b: &Shape, level: Level) -> Option<Vec<usize>> {
    let n = a.rooms.len();
    if n != b.rooms.len() || a.doorways.len() != b.doorways.len() {
        return None;
    }
    let mut map = vec![usize::MAX; n];
    let mut used = vec![false; n];
    for (i, r) in a.rooms.iter().enumerate() {
        let j = (0..n).find(|j| {
            let s = &b.rooms[*j];
            !used[*j]
                && s.environment == r.environment
                && s.piece == r.piece
                && s.portals.len() == r.portals.len()
                && (level == Level::Geometry || s.surfaces == r.surfaces)
                && near(s.frame.origin, r.frame.origin, SAME_PLACE)
                && turned_alike(s.frame.rotation, r.frame.rotation, SAME_TURN)
        })?;
        used[j] = true;
        map[i] = j;
    }
    let mapped = |w: Way| match w {
        Way::Room(i) => Way::Room(map[i]),
        other => other,
    };
    for (i, r) in a.rooms.iter().enumerate() {
        let s = &b.rooms[map[i]];
        let mut x: Vec<_> = r
            .portals
            .iter()
            .map(|(f, p, w, o)| (*f, *p, mapped(*w), *o))
            .collect();
        let mut y = s.portals.clone();
        x.sort_unstable();
        y.sort_unstable();
        if x != y || (level == Level::Statics && !same_statics(&r.statics, &s.statics)) {
            return None;
        }
    }
    let mut x: Vec<_> = a
        .doorways
        .iter()
        .map(|(f, w, p)| (*f, mapped(*w), *p))
        .collect();
    let mut y = b.doorways.clone();
    x.sort_unstable();
    y.sort_unstable();
    (x == y).then_some(map)
}

// ---------------------------------------------------------------------------------------------
// What else stands with a building.
// ---------------------------------------------------------------------------------------------

/// A door this near a doorway's foot stands in it, in meters.
pub const AT_DOORWAY: f32 = 1.5;

/// How far from its usual place a set's partner may stand, in meters (a tenth of that in each
/// quaternion component): a mansion's four stand up to 0.16 m out in some copies.
pub const TOGETHER: f32 = 0.5;

/// The server's weenie type of a door.
const DOOR_TYPE: u32 = 19;

/// The foot of each of building `b`'s doorways: the middle of its opening, at its lowest point,
/// in landblock meters.
pub fn doorway_feet(
    b: &BuildInfo,
    cells: &BTreeMap<u16, EnvCell>,
    env: &mut dyn FnMut(DataId) -> Option<Environment>,
) -> Vec<Vec3> {
    let mut out = Vec::new();
    for p in &b.portals {
        let Some(cell) = cells.get(&p.other_cell_id) else {
            continue;
        };
        let Some(cp) = usize::try_from(p.other_portal_id)
            .ok()
            .and_then(|i| cell.portals.get(i))
        else {
            continue;
        };
        let Some(e) = env(cell.environment) else {
            continue;
        };
        let Some(cs) = e.cells.get(usize::from(cell.cell_struct)) else {
            continue;
        };
        let Some(poly) = cs.polygons.get(usize::from(cp.polygon_id)) else {
            continue;
        };
        let pts: Vec<Vec3> = poly
            .vertex_ids
            .iter()
            .filter_map(|v| cs.vertex_array.vertices.get(usize::from(*v)))
            .map(|v| localtoglobal(&cell.frame, v.position))
            .collect();
        if pts.is_empty() {
            continue;
        }
        #[allow(clippy::cast_precision_loss)]
        let k = pts.len() as f32;
        let x = pts.iter().map(|q| q.x).sum::<f32>() / k;
        let y = pts.iter().map(|q| q.y).sum::<f32>() / k;
        let z = pts.iter().map(|q| q.z).fold(f32::MAX, f32::min);
        out.push(Vec3::new(x, y, z));
    }
    out
}

/// One door instance of the server's world: its guid, weenie, cell and frame (landblock meters).
#[derive(Clone, Copy, Debug)]
struct DoorAt {
    guid: u32,
    weenie: u32,
    cell: u32,
    frame: Frame,
}

/// The doors standing with a building in frame `frame`, its rooms `rooms` and its doorways'
/// feet `feet`: those in its rooms, and those outdoors at a doorway. Each door's room is its
/// cell's number, and the door's frame is in the building's; each comes with its guid.
fn doors_at(
    frame: &Frame,
    rooms: &BTreeSet<u16>,
    feet: &[Vec3],
    doors: &[DoorAt],
) -> Vec<(u32, Door)> {
    let mut out = Vec::new();
    for d in doors {
        #[allow(clippy::cast_possible_truncation)] // a cell's number is its low 16 bits
        let cell = (d.cell & 0xFFFF) as u16;
        let o = d.frame.origin;
        let in_room = rooms.contains(&cell);
        let at_doorway = cell < crate::cells::FIRST_CELL
            && feet.iter().any(|f| {
                ((o.x - f.x).powi(2) + (o.y - f.y).powi(2) + (o.z - f.z).powi(2)).sqrt()
                    <= AT_DOORWAY
            });
        if in_room || at_doorway {
            out.push((
                d.guid,
                Door {
                    weenie: d.weenie,
                    member: 0,
                    frame: relative(frame, &d.frame),
                    room: in_room.then_some(cell),
                },
            ));
        }
    }
    out
}

/// Whether two door lists are the same doors in the same places (a tenth of a meter, two
/// degrees), in any order.
fn same_doors(a: &[Door], b: &[Door]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut taken = vec![false; b.len()];
    a.iter().all(|d| {
        let h = get_heading(&d.frame);
        let k = (0..b.len()).find(|k| {
            let e = &b[*k];
            let turn = (get_heading(&e.frame) - h).rem_euclid(360.0);
            !taken[*k]
                && e.weenie == d.weenie
                && e.member == d.member
                && near(e.frame.origin, d.frame.origin, 0.1)
                && turn.min(360.0 - turn) <= 2.0
        });
        k.is_some_and(|k| {
            taken[k] = true;
            true
        })
    })
}

/// A model's extent in its own frame, `[lo, hi]`, and the corners of its triangles seen from
/// above.
#[derive(Clone, Debug, Default)]
pub(crate) struct Hull {
    pub lo: Vec3,
    pub hi: Vec3,
    pub points: Vec<[f32; 2]>,
}

/// Model `id`'s hull, as `models` reads it.
pub(crate) fn hull(models: &mut Models, set: &DatSet, id: DataId) -> Hull {
    let Some(m) = models.get(set, id) else {
        return Hull::default();
    };
    let mut lo = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
    let mut hi = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
    let mut points: BTreeSet<(u32, u32)> = BTreeSet::new();
    for p in m.triangles.iter().flatten() {
        lo = Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
        hi = Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
        points.insert((p.x.to_bits(), p.y.to_bits()));
    }
    if m.triangles.is_empty() {
        return Hull::default();
    }
    Hull {
        lo,
        hi,
        points: points
            .into_iter()
            .map(|(x, y)| [f32::from_bits(x), f32::from_bits(y)])
            .collect(),
    }
}

/// The land cell `(x, y)` (landblock meters) is in, `0..8` each way.
#[must_use]
pub fn land_cell(x: f32, y: f32) -> (i32, i32) {
    (
        dereth_primitives::num::floor_to_i32(x / 24.0),
        dereth_primitives::num::floor_to_i32(y / 24.0),
    )
}

/// How far a shell `hull` placed at `frame` reaches past the land cell holding its origin, in
/// meters.
pub(crate) fn reach(hull: &Hull, frame: &Frame) -> f32 {
    let (cx, cy) = land_cell(frame.origin.x, frame.origin.y);
    #[allow(clippy::cast_precision_loss)] // a land cell's index
    let (lo_x, lo_y) = (cx as f32 * 24.0, cy as f32 * 24.0);
    hull.points
        .iter()
        .map(|[x, y]| {
            let g = localtoglobal(frame, Vec3::new(*x, *y, 0.0));
            (lo_x - g.x)
                .max(g.x - lo_x - 24.0)
                .max(lo_y - g.y)
                .max(g.y - lo_y - 24.0)
                .max(0.0)
        })
        .fold(0.0, f32::max)
}

// ---------------------------------------------------------------------------------------------
// The catalog, read.
// ---------------------------------------------------------------------------------------------

/// A copy's doors and outdoor statics, its set's with them, in its frame.
type WithSet = (Vec<Door>, Vec<(u32, Frame)>);

/// One copy of a model, as the catalog reads it.
struct Copy {
    site: Site,
    model: u32,
    frame: Frame,
    num_leaves: u32,
    shape: Shape,
    house: bool,
    joined: bool,
    reach: f32,
    /// Its doors, each with its guid.
    doors: Vec<(u32, Door)>,
    /// Whether the world database places anything in its landblock.
    covered: bool,
    outdoor: Vec<(u32, Frame)>,
    /// Its house's guid: what its landblock's restriction table gives the land cell holding its
    /// origin.
    house_guid: Option<u32>,
}

/// The server's world database, looked up: each instance by guid, the instances each links, each
/// landblock's instances, and the instances of each house weenie class.
struct Spawns<'a> {
    by_guid: BTreeMap<u32, &'a crate::sql_ids::Instance>,
    children: BTreeMap<u32, Vec<u32>>,
    by_landblock: BTreeMap<u16, Vec<u32>>,
    houses: BTreeMap<u32, Vec<u32>>,
    content: &'a ServerContent,
}

impl<'a> Spawns<'a> {
    fn new(content: &'a ServerContent) -> Self {
        let mut out = Self {
            by_guid: BTreeMap::new(),
            children: BTreeMap::new(),
            by_landblock: BTreeMap::new(),
            houses: BTreeMap::new(),
            content,
        };
        for i in &content.instances {
            let (Some(guid), Some(w)) = (
                crate::sql_ids::number(&i.guid),
                crate::sql_ids::number(&i.weenie),
            ) else {
                continue;
            };
            out.by_guid.insert(guid, i);
            if let Ok(lb) = u16::try_from(i.cell >> 16) {
                out.by_landblock.entry(lb).or_default().push(guid);
            }
            if content.weenies.get(&w).and_then(|f| f.kind) == Some(House::HOUSE) {
                out.houses.entry(w).or_default().push(guid);
            }
        }
        for (p, c) in &content.links {
            out.children.entry(*p).or_default().push(*c);
        }
        out
    }

    fn weenie(&self, guid: u32) -> Option<u32> {
        crate::sql_ids::number(&self.by_guid.get(&guid)?.weenie)
    }

    fn kind(&self, guid: u32) -> u32 {
        self.weenie(guid)
            .and_then(|w| self.content.weenies.get(&w))
            .and_then(|f| f.kind)
            .unwrap_or(0)
    }

    /// Instance `guid`'s frame, landblock meters.
    fn frame(&self, guid: u32) -> Option<Frame> {
        let i = self.by_guid.get(&guid)?;
        let [w, x, y, z] = i.angles;
        Some(Frame::new(
            Vec3::new(i.origin[0], i.origin[1], i.origin[2]),
            Quat::new(w, x, y, z),
        ))
    }

    /// The house of the copies `members` (each its landblock, frame and house guid; the first the
    /// model's own, then its set's partners): each one's house, everything those link, and the
    /// basement the house portals lead to, with all that stands in it.
    fn house(&self, members: &[(u16, Frame, Option<u32>)]) -> Option<House> {
        let (lb, _, root) = *members.first()?;
        let class = self.weenie(root?)?;
        let mut objects: Vec<HouseObject> = Vec::new();
        let mut links = Vec::new();
        let mut place: BTreeMap<u32, usize> = BTreeMap::new();
        let mut todo: Vec<(u32, usize)> = Vec::new();
        // Each building's house first.
        for (k, (_, _, house)) in members.iter().enumerate() {
            let Some(g) = house else { continue };
            place.insert(*g, objects.len());
            objects.push(self.object(*g, k, members)?);
            todo.push((*g, k));
        }
        // Then what they link, and what those link.
        let mut i = 0;
        while i < todo.len() {
            let (parent, member) = todo[i];
            i += 1;
            for c in self.children.get(&parent).map_or(&[][..], Vec::as_slice) {
                let child = match place.get(c) {
                    Some(at) => *at,
                    None => {
                        let Some(o) = self.object(*c, member, members) else {
                            continue;
                        };
                        place.insert(*c, objects.len());
                        objects.push(o);
                        todo.push((*c, member));
                        objects.len() - 1
                    }
                };
                links.push((place[&parent], child));
            }
        }
        // The basement: the landblock of the house of the same class that stands elsewhere,
        // and every spawn there.
        let basement = self
            .houses
            .get(&class)
            .into_iter()
            .flatten()
            .filter_map(|g| u16::try_from(self.by_guid.get(g)?.cell >> 16).ok())
            .find(|b| *b != lb);
        if let Some(b) = basement {
            let start = objects.len();
            for g in self.by_landblock.get(&b).map_or(&[][..], Vec::as_slice) {
                let Some(o) = self.object(*g, 0, members) else {
                    continue;
                };
                place.insert(*g, objects.len());
                objects.push(o);
            }
            for g in self.by_landblock.get(&b).map_or(&[][..], Vec::as_slice) {
                for c in self.children.get(g).map_or(&[][..], Vec::as_slice) {
                    if let (Some(p), Some(c)) = (place.get(g), place.get(c)) {
                        if *p >= start && *c >= start {
                            links.push((*p, *c));
                        }
                    }
                }
            }
        }
        // A house portal's link spot stands where the portal leads.
        for (p, c) in &links {
            if objects[*p].kind == House::PORTAL {
                objects[*c].beyond = true;
            }
        }
        Some(House {
            class,
            rows: self.content.houses.get(&class).cloned().unwrap_or_default(),
            objects,
            links,
            basement,
        })
    }

    /// Instance `guid` as an object of the house of `members`, standing with building `member`
    /// (outside the house's landblock, in its basement).
    fn object(
        &self,
        guid: u32,
        member: usize,
        members: &[(u16, Frame, Option<u32>)],
    ) -> Option<HouseObject> {
        let i = self.by_guid.get(&guid)?;
        let weenie = self.weenie(guid)?;
        let frame = self.frame(guid)?;
        let (lb, building, _) = members.get(member)?;
        #[allow(clippy::cast_possible_truncation)] // a cell's number is its low 16 bits
        let low = (i.cell & 0xFFFF) as u16;
        let here = i.cell >> 16 == u32::from(*lb);
        let (spot, frame) = if !here {
            (Spot::Basement(low), frame)
        } else if low >= crate::cells::FIRST_CELL {
            (Spot::Room(low), relative(building, &frame))
        } else {
            (Spot::Outdoors, relative(building, &frame))
        };
        Some(HouseObject {
            weenie,
            kind: self.kind(guid),
            member,
            frame,
            spot,
            beyond: false,
            link_child: i.link_child,
        })
    }
}

/// One landblock's buildings and cells, as the files hold them.
struct Block {
    info: LandblockInfo,
    cells: BTreeMap<u16, EnvCell>,
}

/// The cells of landblock `lb` that `read` reads (records of the files of `era`), from `0x0100`
/// on while there is one.
#[must_use]
pub fn read_cells(
    read: &dyn Fn(u32) -> Option<Vec<u8>>,
    lb: u16,
    era: EraId,
) -> BTreeMap<u16, EnvCell> {
    let mut out = BTreeMap::new();
    let first = u32::from(lb) << 16;
    let mut n = crate::cells::FIRST_CELL;
    while n < 0xFFFE {
        let id = first | u32::from(n);
        let Some(c) = read(id).and_then(|b| EnvCell::decode_payload_for(era, DataId(id), &b).ok())
        else {
            break;
        };
        out.insert(n, c);
        n += 1;
    }
    out
}

/// Whether most of `list` holds.
fn most(list: &[bool]) -> bool {
    list.iter().filter(|b| **b).count() * 2 > list.len()
}

impl Catalog {
    /// Every building `set` places, with the doors `server` (the world database, when given)
    /// stands at them; `town` names the town a landblock is in.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn read(
        set: &DatSet,
        server: Option<&ServerContent>,
        town: &dyn Fn(u16) -> Option<String>,
    ) -> Self {
        // Every landblock with buildings.
        let mut blocks: BTreeMap<u16, Block> = BTreeMap::new();
        let read = |id: u32| set.cell.read(DataId(id)).ok();
        for id in set.cell.iter_ids() {
            if id.raw() & 0xFFFF != 0xFFFE {
                continue;
            }
            #[allow(clippy::cast_possible_truncation)] // the landblock is the id's top half
            let lb = (id.raw() >> 16) as u16;
            let Some(info) = read(id.raw()).and_then(|b| set.decode::<LandblockInfo>(id, &b).ok())
            else {
                continue;
            };
            if info.buildings.is_empty() {
                continue;
            }
            let cells = read_cells(&read, lb, set.era);
            blocks.insert(lb, Block { info, cells });
        }
        // The world database's doors, by landblock.
        let mut doors: BTreeMap<u16, Vec<DoorAt>> = BTreeMap::new();
        let mut covered: BTreeSet<u16> = BTreeSet::new();
        if let Some(s) = server {
            for i in &s.instances {
                let Ok(lb) = u16::try_from(i.cell >> 16) else {
                    continue;
                };
                covered.insert(lb);
                let (Some(guid), Some(w)) = (
                    crate::sql_ids::number(&i.guid),
                    crate::sql_ids::number(&i.weenie),
                ) else {
                    continue;
                };
                if s.weenies.get(&w).and_then(|f| f.kind) != Some(DOOR_TYPE) {
                    continue;
                }
                let [qw, qx, qy, qz] = i.angles;
                doors.entry(lb).or_default().push(DoorAt {
                    guid,
                    weenie: w,
                    cell: i.cell,
                    frame: Frame::new(
                        Vec3::new(i.origin[0], i.origin[1], i.origin[2]),
                        Quat::new(qw, qx, qy, qz),
                    ),
                });
            }
        }
        let spawns = server.map(Spawns::new);
        let mut envs: BTreeMap<u32, Option<Environment>> = BTreeMap::new();
        let mut env = |id: DataId| -> Option<Environment> {
            envs.entry(id.raw())
                .or_insert_with(|| {
                    set.read_portal(id)
                        .and_then(|b| set.decode::<Environment>(id, &b).ok())
                })
                .clone()
        };
        let mut models = Models::default();
        let mut hulls: BTreeMap<u32, Hull> = BTreeMap::new();
        // Every copy.
        let mut copies: Vec<Copy> = Vec::new();
        for (lb, b) in &blocks {
            for (index, bld) in b.info.buildings.iter().enumerate() {
                let rooms = rooms(bld, &b.cells);
                let shape = shape(bld, &b.cells, &rooms);
                let hull = hulls
                    .entry(bld.id.raw())
                    .or_insert_with(|| hull(&mut models, set, bld.id));
                let feet = doorway_feet(bld, &b.cells, &mut env);
                let here = doors.get(lb).map_or(&[][..], Vec::as_slice);
                let outdoor = b
                    .info
                    .objects
                    .iter()
                    .filter(|o| {
                        let q = globaltolocal(&bld.frame, o.frame.origin);
                        q.x >= hull.lo.x - 1.0
                            && q.x <= hull.hi.x + 1.0
                            && q.y >= hull.lo.y - 1.0
                            && q.y <= hull.hi.y + 1.0
                            && q.z >= hull.lo.z - 1.0
                            && q.z <= hull.hi.z + 1.0
                    })
                    .map(|o| (o.id.raw(), relative(&bld.frame, &o.frame)))
                    .collect();
                copies.push(Copy {
                    site: Site {
                        landblock: *lb,
                        index,
                    },
                    model: bld.id.raw(),
                    frame: bld.frame,
                    num_leaves: bld.num_leaves,
                    house: rooms
                        .iter()
                        .any(|r| b.cells.get(r).is_some_and(|c| c.restriction_obj.is_some())),
                    joined: shape
                        .rooms
                        .iter()
                        .any(|r| r.portals.iter().any(|p| matches!(p.2, Way::Other(_)))),
                    reach: reach(hull, &bld.frame),
                    doors: doors_at(&bld.frame, &rooms, &feet, here),
                    covered: covered.contains(lb),
                    outdoor,
                    shape,
                    house_guid: b.info.restrictions.as_ref().and_then(|t| {
                        let o = bld.frame.origin;
                        let key = (u32::from(*lb) << 16) | crate::edit::outdoor_cell(o.x, o.y);
                        t.entries.iter().find(|(k, _)| *k == key).map(|(_, g)| *g)
                    }),
                });
            }
        }
        // The sets: models placed three times or more that stand in the same landblocks as
        // another, one of each, in nearly the same place from it every time.
        let mut by_model: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
        for (i, c) in copies.iter().enumerate() {
            by_model.entry(c.model).or_default().push(i);
        }
        let mut by_blocks: BTreeMap<Vec<u16>, Vec<u32>> = BTreeMap::new();
        for (m, list) in &by_model {
            let lbs: Vec<u16> = list.iter().map(|i| copies[*i].site.landblock).collect();
            let unique: BTreeSet<u16> = lbs.iter().copied().collect();
            if list.len() >= 3 && unique.len() == list.len() {
                by_blocks.entry(lbs).or_default().push(*m);
            }
        }
        let mut sets: BTreeMap<u32, Vec<Partner>> = BTreeMap::new();
        for models in by_blocks.values().filter(|m| m.len() > 1) {
            for m in models {
                for q in models.iter().filter(|q| *q != m) {
                    let rel: Vec<Frame> = by_model[m]
                        .iter()
                        .zip(&by_model[q])
                        .map(|(a, b)| relative(&copies[*a].frame, &copies[*b].frame))
                        .collect();
                    // Its place from the model: where most copies have it.
                    let alike = |a: &Frame, b: &Frame, t: f32| {
                        near(a.origin, b.origin, t)
                            && turned_alike(a.rotation, b.rotation, t / 10.0)
                    };
                    let Some(usual) = rel
                        .iter()
                        .max_by_key(|f| rel.iter().filter(|g| alike(f, g, 0.02)).count())
                    else {
                        continue;
                    };
                    if rel.iter().all(|f| alike(f, usual, TOGETHER)) {
                        sets.entry(*m).or_default().push(Partner {
                            model: DataId(*q),
                            frame: *usual,
                        });
                    }
                }
            }
        }
        // A copy's set: the copies of its partners in its landblock.
        let copy_at: BTreeMap<(u16, u32), usize> = copies
            .iter()
            .enumerate()
            .map(|(i, c)| ((c.site.landblock, c.model), i))
            .collect();
        let mut entries = BTreeMap::new();
        for (m, list) in &by_model {
            let partners = sets.get(m).cloned().unwrap_or_default();
            // The doors and outdoor statics of each copy with its set's, in its frame.
            let with_set = |i: usize| -> WithSet {
                let c = &copies[i];
                // A door between two of the set's buildings stands with the first of them.
                let mut seen: BTreeSet<u32> = c.doors.iter().map(|(g, _)| *g).collect();
                let mut doors: Vec<Door> = c.doors.iter().map(|(_, d)| *d).collect();
                let mut outdoor = c.outdoor.clone();
                for (k, p) in partners.iter().enumerate() {
                    let Some(j) = copy_at.get(&(c.site.landblock, p.model.raw())) else {
                        continue;
                    };
                    let o = &copies[*j];
                    for (g, d) in &o.doors {
                        if !seen.insert(*g) {
                            continue;
                        }
                        doors.push(Door {
                            member: k + 1,
                            ..*d
                        });
                    }
                    for (s, f) in &o.outdoor {
                        outdoor.push((*s, relative(&c.frame, &combine(&o.frame, f))));
                    }
                }
                (doors, outdoor)
            };
            // Its layouts, without the statics.
            let mut groups: Vec<Vec<usize>> = Vec::new();
            for &i in list {
                match groups.iter_mut().find(|g| {
                    matching(&copies[i].shape, &copies[g[0]].shape, Level::Surfaces).is_some()
                }) {
                    Some(g) => g.push(i),
                    None => groups.push(vec![i]),
                }
            }
            groups.sort_by_key(|g| std::cmp::Reverse(g.len()));
            let mut layouts = Vec::new();
            for g in &groups {
                let sets_of: Vec<WithSet> = g.iter().map(|i| with_set(*i)).collect();
                // The furnishings.
                let mut furnished: Vec<Vec<usize>> = Vec::new();
                for (k, &i) in g.iter().enumerate() {
                    match furnished.iter_mut().find(|f| {
                        matching(&copies[i].shape, &copies[g[f[0]]].shape, Level::Statics).is_some()
                    }) {
                        Some(f) => f.push(k),
                        None => furnished.push(vec![k]),
                    }
                }
                furnished.sort_by_key(|f| std::cmp::Reverse(f.len()));
                // The most common doors among the copies the world database covers.
                let mut door_sets: Vec<Vec<usize>> = Vec::new();
                for (k, &i) in g.iter().enumerate() {
                    if !copies[i].covered {
                        continue;
                    }
                    match door_sets
                        .iter_mut()
                        .find(|s| same_doors(&sets_of[k].0, &sets_of[s[0]].0))
                    {
                        Some(s) => s.push(k),
                        None => door_sets.push(vec![k]),
                    }
                }
                // The most common; of two as common, one with doors.
                door_sets.sort_by_key(|s| (std::cmp::Reverse(s.len()), sets_of[s[0]].0.is_empty()));
                let candidates: Vec<usize> = door_sets
                    .first()
                    .cloned()
                    .unwrap_or_else(|| (0..g.len()).collect());
                let top: BTreeSet<usize> = furnished
                    .first()
                    .map(|f| f.iter().copied().collect())
                    .unwrap_or_default();
                let example = candidates
                    .iter()
                    .copied()
                    .find(|k| top.contains(k))
                    .unwrap_or(candidates[0]);
                // The outdoor statics most of the layout's copies have.
                let mut clusters: Vec<(u32, Frame, BTreeSet<usize>)> = Vec::new();
                for (k, (_, outdoor)) in sets_of.iter().enumerate() {
                    for (s, f) in outdoor {
                        match clusters.iter_mut().find(|(t, e, _)| {
                            t == s
                                && near(e.origin, f.origin, 0.05)
                                && turned_alike(e.rotation, f.rotation, 0.005)
                        }) {
                            Some(c) => {
                                c.2.insert(k);
                            }
                            None => clusters.push((*s, *f, BTreeSet::from([k]))),
                        }
                    }
                }
                let outdoor = if g.len() >= 2 {
                    clusters
                        .into_iter()
                        .filter(|(_, _, k)| k.len() * 2 > g.len())
                        .map(|(s, f, _)| Outdoor {
                            model: DataId(s),
                            frame: f,
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                let ex = &copies[g[example]];
                // Its house: the example's, its set's buildings' with it.
                let house = spawns.as_ref().and_then(|s| {
                    let mut members = vec![(ex.site.landblock, ex.frame, ex.house_guid)];
                    for p in &partners {
                        let m = copy_at
                            .get(&(ex.site.landblock, p.model.raw()))
                            .map(|j| &copies[*j]);
                        members.push(m.map_or((ex.site.landblock, ex.frame, None), |m| {
                            (m.site.landblock, m.frame, m.house_guid)
                        }));
                    }
                    ex.house_guid.and_then(|_| s.house(&members))
                });
                layouts.push(Layout {
                    example: ex.site,
                    count: u32::try_from(g.len()).unwrap_or(u32::MAX),
                    rooms: u32::try_from(ex.shape.rooms.len()).unwrap_or(u32::MAX),
                    doorways: u32::try_from(ex.shape.doorways.len()).unwrap_or(u32::MAX),
                    furnishings: furnished
                        .iter()
                        .map(|f| {
                            let shown = f
                                .iter()
                                .map(|k| copies[g[*k]].site)
                                .find(|s| town(s.landblock).is_some())
                                .unwrap_or(copies[g[f[0]]].site);
                            Furnishing {
                                example: shown,
                                town: town(shown.landblock),
                                count: u32::try_from(f.len()).unwrap_or(u32::MAX),
                                statics: u32::try_from(
                                    copies[g[f[0]]]
                                        .shape
                                        .rooms
                                        .iter()
                                        .map(|r| r.statics.len())
                                        .sum::<usize>(),
                                )
                                .unwrap_or(u32::MAX),
                            }
                        })
                        .collect(),
                    doors: sets_of[example].0.clone(),
                    outdoor,
                    house,
                });
            }
            let all: Vec<&Copy> = list.iter().map(|i| &copies[*i]).collect();
            let top = &layouts[0];
            let kind = if top.doorways == 0 {
                Kind::Shell
            } else if most(&all.iter().map(|c| c.joined).collect::<Vec<_>>()) {
                Kind::Entrance
            } else if most(&all.iter().map(|c| c.house).collect::<Vec<_>>()) {
                if !partners.is_empty() {
                    Kind::Mansion
                } else if top.rooms >= 20 {
                    Kind::Villa
                } else {
                    Kind::Cottage
                }
            } else {
                Kind::Building
            };
            let mut leaves: BTreeMap<u32, usize> = BTreeMap::new();
            for c in &all {
                *leaves.entry(c.num_leaves).or_default() += 1;
            }
            entries.insert(
                *m,
                Entry {
                    model: DataId(*m),
                    count: u32::try_from(list.len()).unwrap_or(u32::MAX),
                    kind,
                    num_leaves: leaves
                        .iter()
                        .max_by_key(|(v, n)| (**n, std::cmp::Reverse(**v)))
                        .map_or(0, |(v, _)| *v),
                    reach: all.iter().map(|c| c.reach).fold(0.0, f32::max),
                    layouts,
                    set: partners,
                },
            );
        }
        Self {
            entries,
            doors_known: server.is_some(),
        }
    }

    /// Model `model`'s entry.
    #[must_use]
    pub fn get(&self, model: DataId) -> Option<&Entry> {
        self.entries.get(&model.raw())
    }

    /// Every entry, by model id.
    pub fn entries(&self) -> impl Iterator<Item = &Entry> + '_ {
        self.entries.values()
    }

    /// The models a gallery shows, the most placed first: a set by its lead alone.
    #[must_use]
    pub fn gallery(&self) -> Vec<&Entry> {
        let mut out: Vec<&Entry> = self.entries.values().filter(|e| e.is_lead()).collect();
        out.sort_by_key(|e| (std::cmp::Reverse(e.count), e.model.raw()));
        out
    }

    /// How many models it holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether it holds none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ---------------------------------------------------------------------------------------------
// The catalog kept in a file.
// ---------------------------------------------------------------------------------------------

/// Model `model`'s entry of `c`, when it has one.
fn entry(c: &mut Catalog, model: Option<u32>) -> Option<&mut Entry> {
    c.entries.get_mut(&model?)
}

/// The first line of a catalog file, which names its layout.
const HEADER: &str = "# building catalog 2";

fn spot_text(s: Spot) -> String {
    match s {
        Spot::Room(n) => format!("room:{n:04X}"),
        Spot::Outdoors => "out".into(),
        Spot::Basement(n) => format!("basement:{n:04X}"),
    }
}

fn spot_of(s: &str) -> Option<Spot> {
    let hex = |s: &str| u16::from_str_radix(s, 16).ok();
    match s.split_once(':') {
        None if s == "out" => Some(Spot::Outdoors),
        Some(("room", n)) => Some(Spot::Room(hex(n)?)),
        Some(("basement", n)) => Some(Spot::Basement(hex(n)?)),
        _ => None,
    }
}

/// The last layout's house, of `c`'s entry `model`.
fn house_mut(c: &mut Catalog, model: Option<u32>) -> Option<&mut House> {
    entry(c, model)?.layouts.last_mut()?.house.as_mut()
}

fn frame_text(f: &Frame) -> String {
    let (o, q) = (f.origin, f.rotation);
    format!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{}",
        o.x, o.y, o.z, q.w, q.x, q.y, q.z
    )
}

fn frame_of(f: &[&str]) -> Option<Frame> {
    let v: Vec<f32> = f.iter().map(|s| s.parse().ok()).collect::<Option<_>>()?;
    let [x, y, z, w, qx, qy, qz] = v[..] else {
        return None;
    };
    Some(Frame::new(Vec3::new(x, y, z), Quat::new(w, qx, qy, qz)))
}

impl Catalog {
    /// The catalog as a file's text: a header line, then a line for each model and under it a
    /// line for each partner, layout, furnishing, door and outdoor static, fields separated by
    /// tabs.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = format!("{HEADER}\ndoors\t{}\n", u8::from(self.doors_known));
        for e in self.entries.values() {
            let _ = writeln!(
                out,
                "model\t{:08X}\t{}\t{}\t{}\t{}",
                e.model.raw(),
                e.count,
                e.kind.name(),
                e.num_leaves,
                e.reach
            );
            for p in &e.set {
                let _ = writeln!(
                    out,
                    "partner\t{:08X}\t{}",
                    p.model.raw(),
                    frame_text(&p.frame)
                );
            }
            for l in &e.layouts {
                let _ = writeln!(
                    out,
                    "layout\t{}\t{}\t{}\t{}",
                    l.example, l.count, l.rooms, l.doorways
                );
                for f in &l.furnishings {
                    let _ = writeln!(
                        out,
                        "furnished\t{}\t{}\t{}\t{}",
                        f.example,
                        f.count,
                        f.statics,
                        f.town.as_deref().unwrap_or("")
                    );
                }
                for d in &l.doors {
                    let _ = writeln!(
                        out,
                        "door\t{}\t{}\t{}\t{}",
                        d.weenie,
                        d.member,
                        d.room
                            .map_or_else(|| "-".to_owned(), |r| format!("{r:04X}")),
                        frame_text(&d.frame)
                    );
                }
                for o in &l.outdoor {
                    let _ = writeln!(
                        out,
                        "outdoor\t{:08X}\t{}",
                        o.model.raw(),
                        frame_text(&o.frame)
                    );
                }
                let Some(h) = &l.house else { continue };
                let _ = writeln!(
                    out,
                    "house\t{}\t{}",
                    h.class,
                    h.basement
                        .map_or_else(|| "-".to_owned(), |b| format!("{b:04X}"))
                );
                for r in h.rows.iter().filter(|r| !r.value.contains(['\t', '\n'])) {
                    let _ = writeln!(out, "house-row\t{}\t{}\t{}", r.table, r.kind, r.value);
                }
                for o in &h.objects {
                    let _ = writeln!(
                        out,
                        "house-object\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                        o.weenie,
                        o.kind,
                        o.member,
                        spot_text(o.spot),
                        u8::from(o.beyond),
                        u8::from(o.link_child),
                        frame_text(&o.frame)
                    );
                }
                for (p, c) in &h.links {
                    let _ = writeln!(out, "house-link\t{p}\t{c}");
                }
            }
        }
        out
    }

    /// The catalog [`Self::to_text`] wrote; `None` for any other text.
    #[must_use]
    pub fn from_text(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        if lines.next()? != HEADER {
            return None;
        }
        let hex = |s: &str| u32::from_str_radix(s, 16).ok();
        let mut out = Self::default();
        let mut model: Option<u32> = None;
        for line in lines {
            let f: Vec<&str> = line.split('\t').collect();
            match f[..] {
                ["doors", d] => out.doors_known = d == "1",
                ["model", id, count, kind, leaves, reach] => {
                    let id = hex(id)?;
                    out.entries.insert(
                        id,
                        Entry {
                            model: DataId(id),
                            count: count.parse().ok()?,
                            kind: Kind::parse(kind)?,
                            num_leaves: leaves.parse().ok()?,
                            reach: reach.parse().ok()?,
                            layouts: Vec::new(),
                            set: Vec::new(),
                        },
                    );
                    model = Some(id);
                }
                ["partner", id, ref frame @ ..] => {
                    let p = Partner {
                        model: DataId(hex(id)?),
                        frame: frame_of(frame)?,
                    };
                    entry(&mut out, model)?.set.push(p);
                }
                ["layout", site, count, rooms, doorways] => {
                    let l = Layout {
                        example: Site::parse(site)?,
                        count: count.parse().ok()?,
                        rooms: rooms.parse().ok()?,
                        doorways: doorways.parse().ok()?,
                        furnishings: Vec::new(),
                        doors: Vec::new(),
                        outdoor: Vec::new(),
                        house: None,
                    };
                    entry(&mut out, model)?.layouts.push(l);
                }
                ["furnished", site, count, statics, town] => {
                    let f = Furnishing {
                        example: Site::parse(site)?,
                        town: (!town.is_empty()).then(|| town.to_owned()),
                        count: count.parse().ok()?,
                        statics: statics.parse().ok()?,
                    };
                    entry(&mut out, model)?
                        .layouts
                        .last_mut()?
                        .furnishings
                        .push(f);
                }
                ["door", weenie, member, room, ref frame @ ..] => {
                    let d = Door {
                        weenie: weenie.parse().ok()?,
                        member: member.parse().ok()?,
                        frame: frame_of(frame)?,
                        room: if room == "-" {
                            None
                        } else {
                            Some(u16::from_str_radix(room, 16).ok()?)
                        },
                    };
                    entry(&mut out, model)?.layouts.last_mut()?.doors.push(d);
                }
                ["outdoor", id, ref frame @ ..] => {
                    let o = Outdoor {
                        model: DataId(hex(id)?),
                        frame: frame_of(frame)?,
                    };
                    entry(&mut out, model)?.layouts.last_mut()?.outdoor.push(o);
                }
                ["house", class, basement] => {
                    let h = House {
                        class: class.parse().ok()?,
                        rows: Vec::new(),
                        objects: Vec::new(),
                        links: Vec::new(),
                        basement: if basement == "-" {
                            None
                        } else {
                            Some(u16::from_str_radix(basement, 16).ok()?)
                        },
                    };
                    entry(&mut out, model)?.layouts.last_mut()?.house = Some(h);
                }
                ["house-row", table, kind, value] => {
                    let r = crate::sql_ids::HouseRow {
                        table: table.to_owned(),
                        kind: kind.parse().ok()?,
                        value: value.to_owned(),
                    };
                    house_mut(&mut out, model)?.rows.push(r);
                }
                ["house-object", weenie, kind, member, spot, beyond, child, ref frame @ ..] => {
                    let o = HouseObject {
                        weenie: weenie.parse().ok()?,
                        kind: kind.parse().ok()?,
                        member: member.parse().ok()?,
                        frame: frame_of(frame)?,
                        spot: spot_of(spot)?,
                        beyond: beyond == "1",
                        link_child: child == "1",
                    };
                    house_mut(&mut out, model)?.objects.push(o);
                }
                ["house-link", p, c] => {
                    let l = (p.parse().ok()?, c.parse().ok()?);
                    house_mut(&mut out, model)?.links.push(l);
                }
                _ => return None,
            }
        }
        Some(out)
    }

    /// Keep the catalog in the file `path`.
    ///
    /// # Errors
    /// The file cannot be written.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let part = path.with_extension("part");
        std::fs::write(&part, self.to_text())?;
        std::fs::rename(&part, path)
    }

    /// The catalog kept in the file `path`, when it holds one.
    #[must_use]
    pub fn load(path: &Path) -> Option<Self> {
        Self::from_text(&std::fs::read_to_string(path).ok()?)
    }
}
