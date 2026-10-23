//! A retail building dropped somewhere else: a copy of one of the world's own, with its rooms,
//! its doors and the ground under it.
//!
//! **The copy** ([`copy`]) is exact. The building's entry keeps its model, its doorways and its
//! `num_leaves`, and stands at the new frame. Each room keeps its piece, its surfaces and its
//! ways, and is carried by the same move about the building's origin; its number is the
//! lockfile's, and its portals, its list and the doorways' lists name the new numbers. A way out
//! of the rooms into a cell that is not one of them (a dungeon under an entrance) is shut. The
//! rooms hold the template's furniture, another copy's laid out the same way, or none, and
//! belong to no house.
//!
//! **The drop** ([`Session::drop_building`]) is one step, from the catalog
//! ([`crate::buildings`]): the ground under the shell is made level at the height under its
//! origin, the origin stands as high over it as the template's does, the scenery its shell
//! would stand over in a neighboring land cell is cleared, the building is added as an
//! `[[add-building]]` row, and its model's doors and outdoor statics stand with it. A drop is
//! refused where its origin is outside the landblock or in a land cell that already holds a
//! building (the client draws one building in each), or where the landblock has no room or
//! instance numbers left for it; one whose shell reaches further into the next land cell than
//! any retail copy of the model is warned of. [`Session::drop_plan`] says where a drop would
//! stand and whether it may, and [`Session::drop_snap`] puts it where its example stands in its
//! own land cell, turned by quarter turns.
//!
//! **The ground of the buildings around** is never changed. Neighboring land cells share
//! corners, so where a land cell a drop levels shares corners with one another building stands
//! over (the world's own or a drop, in its landblock or the next), the drop is leveled at the
//! height of those corners instead, and its origin stands on that level. It is refused where
//! those corners are not at one height (it would tilt the ground under the building next door),
//! or where their height is more than [`MOST_LIFT`] from the ground under its origin. A move is
//! leveled the same way.
//!
//! **A dropped building moved** ([`Op::MoveBuilding`]) is its row written anew: its set carried
//! by the one move, on the ground there made level, the ground it stood on given back to what
//! the packs under it and the world make it (but where another drop stands), and its doors,
//! outdoor statics, the spawns and statics in its rooms and the spawns over it carried. **Taken
//! away** ([`Op::RemoveBuilding`]), its row goes, and with it its rooms, doors, outdoor statics,
//! what stands in its rooms, and its level ground. Each is one step.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use dereth_assets::world::{
    BuildInfo, BuildingPortal, CellLandblock, CellPortal, EnvCell, LandblockInfo, ObjectEntry,
};
use dereth_assets::{Decode, Region};
use dereth_dat::ModernDat;
use dereth_primitives::frame::{combine, localtoglobal};
use dereth_primitives::{DataId, EraId, Frame, Vec3};

use crate::buildings::{self, Catalog, Level, Site};
use crate::cells::carried;
use crate::compile::{self, Compiled, Field, StaticOrigin};
use crate::edit::{Op, Session, VertexSet};
use crate::error::{Error, Loc};
use crate::lock::Lock;
use crate::source::{AddBuilding, Furnished, Height, Located, Place, Rotation};
use crate::symbol::Kind;

/// The name of room `k` of dropped building `name`: its rooms are counted over the drop's
/// buildings in order, each one's in ascending order of its template's numbers.
#[must_use]
pub fn room_name(name: &str, k: usize) -> String {
    format!("{name}-room-{k:03}")
}

/// The name of door `k` of dropped building `name`, the symbol of its instance.
#[must_use]
pub fn door_name(name: &str, k: usize) -> String {
    format!("{name}-door-{k}")
}

/// The name of outdoor static `k` of dropped building `name`, its symbol in the landblock.
#[must_use]
pub fn outdoor_name(name: &str, k: usize) -> String {
    format!("{name}-outdoor-{k}")
}

/// A retail building as a copy reads it.
#[derive(Clone, Debug)]
pub struct Template {
    pub site: Site,
    pub building: BuildInfo,
    /// Its rooms, by number.
    pub rooms: BTreeMap<u16, EnvCell>,
    /// The ground's height under its origin, as its landblock's terrain makes it.
    pub ground: Option<f32>,
}

impl Template {
    /// The building at `site` of the files `read` reads (cell-file records by id, in the layout
    /// of the files of `era`); `region` makes the ground under it.
    ///
    /// # Errors
    /// No such building.
    pub fn read(
        read: &dyn Fn(u32) -> Option<Vec<u8>>,
        site: Site,
        region: Option<&Region>,
        era: EraId,
    ) -> Result<Self, String> {
        let first = u32::from(site.landblock) << 16;
        let info = read(first | 0xFFFE)
            .and_then(|b| LandblockInfo::decode_payload_for(era, DataId(first | 0xFFFE), &b).ok())
            .ok_or_else(|| format!("landblock {:04X} has no buildings", site.landblock))?;
        let building = info.buildings.get(site.index).cloned().ok_or_else(|| {
            format!(
                "landblock {:04X} has {} building(s); there is no {}",
                site.landblock,
                info.buildings.len(),
                site.index
            )
        })?;
        let wanted: BTreeSet<u16> = building
            .portals
            .iter()
            .flat_map(|p| p.stab_list.iter().copied().chain([p.other_cell_id]))
            .collect();
        let rooms: BTreeMap<u16, EnvCell> = wanted
            .into_iter()
            .filter_map(|n| {
                let id = first | u32::from(n);
                let c = EnvCell::decode_payload_for(era, DataId(id), &read(id)?).ok()?;
                Some((n, c))
            })
            .collect();
        let ground = region.and_then(|r| {
            let id = first | 0xFFFF;
            let land = CellLandblock::decode_payload_for(era, DataId(id), &read(id)?).ok()?;
            let o = building.frame.origin;
            Some(compile::ground(
                &land,
                r,
                site.landblock,
                o.x.clamp(0.0, 191.99),
                o.y.clamp(0.0, 191.99),
            ))
        });
        Ok(Self {
            site,
            building,
            rooms,
            ground,
        })
    }

    /// How high its origin stands over the ground under it.
    #[must_use]
    pub fn height(&self) -> f32 {
        self.ground
            .map_or(0.0, |g| self.building.frame.origin.z - g)
    }

    fn shape(&self) -> buildings::Shape {
        let rooms: BTreeSet<u16> = self.rooms.keys().copied().collect();
        buildings::shape(&self.building, &self.rooms, &rooms)
    }
}

/// A building copied: its entry, its rooms by their new numbers, and how many ways out of the
/// rooms into other cells were shut.
#[derive(Clone, Debug, PartialEq)]
pub struct Copied {
    pub building: BuildInfo,
    pub rooms: BTreeMap<u16, EnvCell>,
    pub shut: usize,
}

/// `list` with every number `renumber` holds renumbered and the rest left out, in ascending
/// order when `list` was.
fn renumbered(list: &[u16], renumber: &BTreeMap<u16, u16>) -> Vec<u16> {
    let mut out: Vec<u16> = list
        .iter()
        .filter_map(|c| renumber.get(c))
        .copied()
        .collect();
    if list.is_sorted() {
        out.sort_unstable();
    }
    out
}

/// `t` copied into landblock `lb`, standing at `to`: its rooms numbered `numbers` (one for each,
/// in ascending order of their numbers), furnished with the statics of `furniture`'s rooms (the
/// template itself, a copy of it laid out the same way, or none).
///
/// # Errors
/// A count of numbers that is not the rooms', a doorway into no room, a furniture's copy laid
/// out otherwise.
pub fn copy(
    t: &Template,
    furniture: Option<&Template>,
    lb: u16,
    numbers: &[u16],
    to: &Frame,
) -> Result<Copied, String> {
    let from = t.building.frame;
    if numbers.len() != t.rooms.len() {
        return Err(format!(
            "{} has {} room(s); {} are numbered",
            t.site,
            t.rooms.len(),
            numbers.len()
        ));
    }
    let renumber: BTreeMap<u16, u16> = t
        .rooms
        .keys()
        .copied()
        .zip(numbers.iter().copied())
        .collect();
    // Each room's portals kept, old place to new: those to the outdoors and to the rooms.
    let mut kept: BTreeMap<u16, BTreeMap<i16, i16>> = BTreeMap::new();
    let mut shut = 0;
    for (n, c) in &t.rooms {
        let mut map = BTreeMap::new();
        for (i, p) in c.portals.iter().enumerate() {
            #[allow(clippy::cast_possible_truncation)] // a cell's number is 16 bits
            let room = renumber.contains_key(&(p.other_cell_id as u16));
            if p.flags & 4 != 0 || room {
                if let (Ok(from), Ok(to)) = (i16::try_from(i), i16::try_from(map.len())) {
                    map.insert(from, to);
                }
            } else {
                shut += 1;
            }
        }
        kept.insert(*n, map);
    }
    // The furniture, carried to its room's copy.
    let mut statics: BTreeMap<u16, Vec<ObjectEntry>> = BTreeMap::new();
    if let Some(f) = furniture {
        let carry = |o: &ObjectEntry| ObjectEntry {
            id: o.id,
            frame: carried(&o.frame, &f.building.frame, to),
        };
        if f.site == t.site {
            for (n, c) in &t.rooms {
                statics.insert(*n, c.static_objects.iter().map(carry).collect());
            }
        } else {
            let (a, b) = (f.shape(), t.shape());
            let map = buildings::matching(&a, &b, Level::Geometry).ok_or_else(|| {
                format!(
                    "{} is not laid out as {} is: its furniture does not fit",
                    f.site, t.site
                )
            })?;
            for (i, r) in a.rooms.iter().enumerate() {
                let into = b.rooms[map[i]].number;
                statics.insert(
                    into,
                    f.rooms[&r.number]
                        .static_objects
                        .iter()
                        .map(carry)
                        .collect(),
                );
            }
        }
    }
    let mut rooms = BTreeMap::new();
    for (n, c) in &t.rooms {
        let new = renumber[n];
        let id = (u32::from(lb) << 16) | u32::from(new);
        let portals = c
            .portals
            .iter()
            .enumerate()
            .filter(|(i, _)| i16::try_from(*i).is_ok_and(|i| kept[n].contains_key(&i)))
            .map(|(_, p)| {
                if p.flags & 4 != 0 {
                    return *p;
                }
                #[allow(clippy::cast_possible_truncation)] // a cell's number is 16 bits
                let other = p.other_cell_id as u16;
                CellPortal {
                    other_cell_id: u32::from(renumber[&other]),
                    other_portal_id: kept[&other]
                        .get(&p.other_portal_id)
                        .copied()
                        .unwrap_or(p.other_portal_id),
                    ..*p
                }
            })
            .collect();
        let static_objects = statics.remove(n).unwrap_or_default();
        rooms.insert(
            new,
            EnvCell {
                id: DataId(id),
                flags: (c.flags & !(2 | 8)) | if static_objects.is_empty() { 0 } else { 2 },
                cell_id_repeat: id,
                surfaces: c.surfaces.clone(),
                environment: c.environment,
                cell_struct: c.cell_struct,
                frame: carried(&c.frame, &from, to),
                portals,
                visible_cells: renumbered(&c.visible_cells, &renumber),
                static_objects,
                restriction_obj: None,
            },
        );
    }
    let mut doorways = Vec::new();
    for p in &t.building.portals {
        let Some(n) = renumber.get(&p.other_cell_id) else {
            return Err(format!(
                "a doorway of {} leads into cell {:04X}, which is none of its rooms",
                t.site, p.other_cell_id
            ));
        };
        doorways.push(BuildingPortal {
            flags: p.flags,
            other_cell_id: *n,
            other_portal_id: kept[&p.other_cell_id]
                .get(&p.other_portal_id)
                .copied()
                .unwrap_or(p.other_portal_id),
            stab_list: renumbered(&p.stab_list, &renumber),
        });
    }
    Ok(Copied {
        building: BuildInfo {
            id: t.building.id,
            frame: *to,
            num_leaves: t.building.num_leaves,
            portals: doorways,
        },
        rooms,
        shut,
    })
}

/// A drop copied, ready to place in its landblock: its row, its buildings' entries (none for a
/// copied dungeon), its rooms by their numbers and their names, and for a house, the entries of
/// the landblock's restriction table (a land cell, or 0 for the whole landblock, and the house
/// that holds it) and the id each house spawn's symbol was given.
#[derive(Clone, Debug)]
pub struct Dropped {
    pub at: Loc,
    pub name: String,
    pub buildings: Vec<BuildInfo>,
    pub rooms: BTreeMap<u16, EnvCell>,
    pub names: BTreeMap<String, u16>,
    pub shut: usize,
    pub restrictions: Vec<(u32, u32)>,
    pub houses: Vec<(String, u32)>,
}

/// The summary line of drop `d` in landblock `lb`.
#[must_use]
pub fn summary(lb: u16, d: &Dropped) -> String {
    let shut = if d.shut > 0 {
        format!(", {} way(s) into other cells shut", d.shut)
    } else {
        String::new()
    };
    let house = if d.houses.is_empty() {
        ""
    } else {
        ", a house players can buy"
    };
    if d.buildings.is_empty() {
        format!(
            "{lb:04X}: dungeon {} copied with {} cell(s){shut}{house}",
            d.name,
            d.rooms.len()
        )
    } else {
        format!(
            "{lb:04X}: building {} dropped with {} room(s){shut}{house}",
            d.name,
            d.rooms.len()
        )
    }
}

/// How many buckets a restriction table made new has, as every retail one does.
const BUCKETS: u32 = 8;

/// `info`'s restriction table with each of `entries` (a cell, or 0 for the whole landblock, and
/// the house that holds it) in it: an entry for the same cell replaced, and a table made when it
/// has none. A new entry goes after the last of its bucket's, the table's entries kept in bucket
/// order.
pub fn restrict(info: &mut LandblockInfo, entries: &[(u32, u32)]) {
    if entries.is_empty() {
        return;
    }
    let table = info
        .restrictions
        .get_or_insert_with(|| dereth_assets::world::RestrictionTable {
            buckets: BUCKETS,
            entries: Vec::new(),
        });
    if table.buckets == 0 {
        table.buckets = BUCKETS;
    }
    let buckets = table.buckets;
    for (cell, house) in entries {
        if let Some(e) = table.entries.iter_mut().find(|(c, _)| c == cell) {
            e.1 = *house;
            continue;
        }
        let bucket = cell % buckets;
        let at = table
            .entries
            .iter()
            .rposition(|(c, _)| c % buckets <= bucket)
            .map_or(0, |i| i + 1);
        table.entries.insert(at, (*cell, *house));
    }
    info.pack_mask |= 1;
}

/// A drop's row read with its templates and its furniture.
pub(crate) struct Prepared<'a> {
    row: &'a Located<AddBuilding>,
    templates: Vec<Template>,
    furniture: Vec<Option<Template>>,
}

/// The templates and furniture of `row`, as `read` reads the files of `era` under the packs.
pub(crate) fn prepare<'a>(
    row: &'a Located<AddBuilding>,
    read: &dyn Fn(u32) -> Option<Vec<u8>>,
    region: Option<&Region>,
    era: EraId,
) -> Result<Prepared<'a>, Error> {
    let d = &row.item;
    let fail = |why: String| Error::source(&row.at, format!("building {}: {why}", d.name));
    let mut templates = Vec::new();
    for (site, model) in d.templates.iter().zip(&d.models) {
        if templates.iter().any(|t: &Template| t.site == *site) {
            return Err(fail(format!("it names {site} twice")));
        }
        let t = Template::read(read, *site, region, era).map_err(fail)?;
        if t.building.id != *model {
            return Err(fail(format!(
                "{site} is {:#010X}, not {:#010X}",
                t.building.id.raw(),
                model.raw()
            )));
        }
        templates.push(t);
    }
    let furniture = match &d.furnished {
        Furnished::Template => templates.iter().cloned().map(Some).collect(),
        Furnished::Bare => vec![None; templates.len()],
        Furnished::Like(sites) => sites
            .iter()
            .zip(&d.models)
            .map(|(site, model)| {
                let f = Template::read(read, *site, region, era).map_err(fail)?;
                if f.building.id == *model {
                    Ok(Some(f))
                } else {
                    Err(fail(format!(
                        "its furniture's {site} is {:#010X}, not {:#010X}",
                        f.building.id.raw(),
                        model.raw()
                    )))
                }
            })
            .collect::<Result<_, _>>()?,
    };
    Ok(Prepared {
        row,
        templates,
        furniture,
    })
}

/// An instance symbol of landblock `lb`, as the lockfile keeps it: `pack:name@XXYY`.
#[must_use]
pub fn instance_symbol(name: &str, lb: u16) -> String {
    format!("{name}@{lb:04X}")
}

/// The id lock `lock` holds for house spawn `name` (in full) of landblock `lb`.
fn house_guid(lock: &Lock, name: &str, lb: u16) -> Option<u32> {
    lock.get(Kind::Guid, &instance_symbol(name, lb))
}

/// `cells` belonging to house `guid`.
fn restrict_cells<'a>(cells: impl Iterator<Item = &'a mut EnvCell>, guid: u32) {
    for c in cells {
        c.restriction_obj = Some(guid);
        c.flags |= 8;
    }
}

impl Prepared<'_> {
    /// The cell symbol of each of its rooms in landblock `lb`, as the lockfile keeps them.
    pub(crate) fn room_symbols(&self, lb: u16) -> Vec<String> {
        let n: usize = self.templates.iter().map(|t| t.rooms.len()).sum();
        (0..n)
            .map(|k| compile::cell_symbol(&room_name(&self.row.item.name, k), lb))
            .collect()
    }

    /// Every symbol it numbers in landblock `lb`: its rooms' cells, and its houses' spawns.
    pub(crate) fn symbols(&self, lb: u16) -> Vec<(Kind, String)> {
        self.room_symbols(lb)
            .into_iter()
            .map(|s| (Kind::Cell, s))
            .chain(
                self.row
                    .item
                    .house
                    .iter()
                    .map(|h| (Kind::Guid, instance_symbol(h, lb))),
            )
            .collect()
    }

    /// Its copies in landblock `lb`, their rooms numbered as `lock` holds them; `ground_z` is the
    /// ground there.
    pub(crate) fn copy(
        &self,
        lb: u16,
        lock: &Lock,
        ground_z: &dyn Fn(f32, f32) -> f32,
    ) -> Result<Dropped, Error> {
        let d = &self.row.item;
        let fail = |why: String| Error::source(&self.row.at, format!("building {}: {why}", d.name));
        let first = &self.templates[0];
        let z = match d.at.z {
            Height::At(z) => z,
            Height::Ground => ground_z(d.at.x, d.at.y) + first.height(),
        };
        let to = Frame::new(Vec3::new(d.at.x, d.at.y, z), compile::quat_of(d.rotation));
        let symbols = self.room_symbols(lb);
        let mut out = Dropped {
            at: self.row.at.clone(),
            name: d.name.clone(),
            buildings: Vec::new(),
            rooms: BTreeMap::new(),
            names: BTreeMap::new(),
            shut: 0,
            restrictions: Vec::new(),
            houses: Vec::new(),
        };
        let mut k = 0;
        for (member, (t, f)) in self.templates.iter().zip(&self.furniture).enumerate() {
            let mut numbers = Vec::new();
            for _ in 0..t.rooms.len() {
                let n = lock
                    .get(Kind::Cell, &symbols[k])
                    .and_then(|id| u16::try_from(id & 0xFFFF).ok())
                    .ok_or_else(|| fail(format!("its room {k} was not numbered")))?;
                out.names.insert(room_name(&d.name, k), n);
                numbers.push(n);
                k += 1;
            }
            let at = carried(&t.building.frame, &first.building.frame, &to);
            let mut c = copy(t, f.as_ref(), lb, &numbers, &at).map_err(fail)?;
            // A house's: its rooms and the land cell holding its origin belong to its spawn.
            if let Some(h) = d.house.get(member) {
                let guid = house_guid(lock, h, lb)
                    .ok_or_else(|| fail(format!("its house {h} was not numbered")))?;
                restrict_cells(c.rooms.values_mut(), guid);
                let o = at.origin;
                out.restrictions.push((
                    (u32::from(lb) << 16) | crate::edit::outdoor_cell(o.x, o.y),
                    guid,
                ));
                out.houses.push((instance_symbol(h, lb), guid));
            }
            out.buildings.push(c.building);
            out.rooms.extend(c.rooms);
            out.shut += c.shut;
        }
        Ok(out)
    }
}

/// A dungeon copy's row read with its template's cells.
pub(crate) struct PreparedDungeon<'a> {
    row: &'a Located<crate::source::AddDungeon>,
    cells: BTreeMap<u16, EnvCell>,
}

/// The template's cells of dungeon copy `row`, as `read` reads the files of `era` under the
/// packs.
pub(crate) fn prepare_dungeon<'a>(
    row: &'a Located<crate::source::AddDungeon>,
    read: &dyn Fn(u32) -> Option<Vec<u8>>,
    era: EraId,
) -> Result<PreparedDungeon<'a>, Error> {
    let d = &row.item;
    let cells = buildings::read_cells(read, d.template, era);
    if cells.is_empty() {
        return Err(Error::source(
            &row.at,
            format!(
                "dungeon {}: landblock {:04X} has no cells to copy",
                d.name, d.template
            ),
        ));
    }
    if cells.values().any(|c| c.flags & 1 != 0) {
        return Err(Error::source(
            &row.at,
            format!(
                "dungeon {}: landblock {:04X} holds a building's rooms, not a dungeon",
                d.name, d.template
            ),
        ));
    }
    Ok(PreparedDungeon { row, cells })
}

impl PreparedDungeon<'_> {
    /// Every symbol it numbers in landblock `lb`: its cells, and its house's spawn.
    pub(crate) fn symbols(&self, lb: u16) -> Vec<(Kind, String)> {
        let d = &self.row.item;
        (0..self.cells.len())
            .map(|k| (Kind::Cell, compile::cell_symbol(&room_name(&d.name, k), lb)))
            .chain(d.house.iter().map(|h| (Kind::Guid, instance_symbol(h, lb))))
            .collect()
    }

    /// Its copy in landblock `lb`, its cells numbered as `lock` holds them: each in its
    /// template's place, with its statics.
    pub(crate) fn copy(&self, lb: u16, lock: &Lock) -> Result<Dropped, Error> {
        let d = &self.row.item;
        let fail = |why: String| Error::source(&self.row.at, format!("dungeon {}: {why}", d.name));
        let mut names = BTreeMap::new();
        let mut numbers = Vec::new();
        for k in 0..self.cells.len() {
            let name = room_name(&d.name, k);
            let n = lock
                .get(Kind::Cell, &compile::cell_symbol(&name, lb))
                .and_then(|id| u16::try_from(id & 0xFFFF).ok())
                .ok_or_else(|| fail(format!("its cell {k} was not numbered")))?;
            names.insert(name, n);
            numbers.push(n);
        }
        let still = Frame::new(Vec3::ZERO, dereth_primitives::Quat::new(1.0, 0.0, 0.0, 0.0));
        let t = Template {
            site: Site {
                landblock: d.template,
                index: usize::MAX,
            },
            building: BuildInfo {
                id: DataId(0),
                frame: still,
                num_leaves: 0,
                portals: Vec::new(),
            },
            rooms: self.cells.clone(),
            ground: None,
        };
        let mut c = copy(&t, Some(&t), lb, &numbers, &still).map_err(fail)?;
        // It stands where its template does: each frame as the template writes it.
        for (n, new) in self.cells.keys().zip(&numbers) {
            if let (Some(from), Some(to)) = (self.cells.get(n), c.rooms.get_mut(new)) {
                to.frame = from.frame;
                to.static_objects.clone_from(&from.static_objects);
            }
        }
        let mut out = Dropped {
            at: self.row.at.clone(),
            name: d.name.clone(),
            buildings: Vec::new(),
            rooms: BTreeMap::new(),
            names,
            shut: c.shut,
            restrictions: Vec::new(),
            houses: Vec::new(),
        };
        // A house's: every cell, and the whole landblock, belong to its spawn.
        if let Some(h) = &d.house {
            let guid = house_guid(lock, h, lb)
                .ok_or_else(|| fail(format!("its house {h} was not numbered")))?;
            restrict_cells(c.rooms.values_mut(), guid);
            out.restrictions.push((0, guid));
            out.houses.push((instance_symbol(h, lb), guid));
        }
        out.rooms = c.rooms;
        Ok(out)
    }
}

/// Whether point `(x, y)` (landblock meters) is in the landblock.
fn inside(x: f32, y: f32) -> bool {
    (0.0..192.0).contains(&x) && (0.0..192.0).contains(&y)
}

/// Why building `name`'s entries `new` cannot stand in landblock `lb`, when one's origin is
/// outside it or in a land cell that holds one of `info`'s buildings already.
///
/// # Errors
/// That reason, in plain words.
pub fn check_sites(
    lb: u16,
    name: &str,
    info: &LandblockInfo,
    new: &[BuildInfo],
) -> Result<(), String> {
    match site_refusal(info, new) {
        None => Ok(()),
        Some((k, None)) => {
            let o = new[k].frame.origin;
            Err(format!(
                "building {name} would stand at ({:.1}, {:.1}), outside landblock {lb:04X}: its \
                 origin must be within 0 to 192 meters each way",
                o.x, o.y
            ))
        }
        Some((k, Some(i))) => {
            let o = new[k].frame.origin;
            let model = info
                .buildings
                .get(i)
                .or_else(|| new.get(i - info.buildings.len()))
                .map_or(0, |b| b.id.raw());
            Err(format!(
                "building {name} would stand at ({:.1}, {:.1}), in the land cell of landblock \
                 {lb:04X} that building {i} ({model:#010X}) stands in: the client draws only one \
                 building in each 24-meter land cell",
                o.x, o.y
            ))
        }
    }
}

/// The first of the entries `new` that cannot stand where it is among `info`'s buildings, and
/// why: its origin outside the landblock (`None`), or in the land cell building `i` stands in
/// (`Some(i)`, `new`'s own counted after `info`'s).
#[must_use]
pub fn site_refusal(info: &LandblockInfo, new: &[BuildInfo]) -> Option<(usize, Option<usize>)> {
    let mut held: Vec<(usize, &BuildInfo)> = info.buildings.iter().enumerate().collect();
    let base = info.buildings.len();
    for (k, b) in new.iter().enumerate() {
        let o = b.frame.origin;
        if !inside(o.x, o.y) {
            return Some((k, None));
        }
        let cell = buildings::land_cell(o.x, o.y);
        if let Some((i, _)) = held
            .iter()
            .find(|(_, h)| buildings::land_cell(h.frame.origin.x, h.frame.origin.y) == cell)
        {
            return Some((k, Some(*i)));
        }
        held.push((base + k, b));
    }
    None
}

/// The walk into a dropped building: going in through its doorways reaches every room its
/// template's ways join (a building may hold rooms behind one door that another does not lead
/// to), every way leads somewhere and back, and every room has a floor ([`crate::walk`]).
#[must_use]
pub fn walk_in(
    cells: &BTreeMap<u16, EnvCell>,
    building: &BuildInfo,
    env: &dyn Fn(DataId) -> Option<dereth_assets::Environment>,
) -> Vec<crate::walk::Finding> {
    let rooms = buildings::rooms(building, cells);
    // A room no way joins is reached by no walk, as in the template.
    let joined: BTreeSet<u16> = rooms
        .iter()
        .copied()
        .filter(|r| {
            building.portals.iter().any(|p| p.other_cell_id == *r)
                || cells
                    .get(r)
                    .is_some_and(|c| c.portals.iter().any(|p| p.flags & 4 == 0))
        })
        .collect();
    let mut out = crate::walk::check(cells, &joined, None, env);
    let mut reached = BTreeSet::new();
    let mut todo: Vec<u16> = building.portals.iter().map(|p| p.other_cell_id).collect();
    while let Some(c) = todo.pop() {
        if !reached.insert(c) {
            continue;
        }
        for p in cells
            .get(&c)
            .map(|c| c.portals.as_slice())
            .unwrap_or_default()
        {
            if p.flags & 4 == 0 {
                #[allow(clippy::cast_possible_truncation)] // a cell's number is 16 bits
                todo.push(p.other_cell_id as u16);
            }
        }
    }
    for r in joined.difference(&reached) {
        out.push(crate::walk::Finding {
            cell: *r,
            text: "no doorway of its building leads to it".into(),
        });
    }
    out
}

// ---------------------------------------------------------------------------------------------
// The drop, as an editing step.
// ---------------------------------------------------------------------------------------------

/// How many instance ids a landblock has.
pub const INSTANCES: usize = 4096;

/// A number written for SQL: six decimals, trailing zeros dropped, one kept.
fn six(v: f32) -> String {
    let s = format!("{v:.6}");
    let s = if s.trim_start_matches(['-', '0', '.']).is_empty() {
        s.trim_start_matches('-').to_owned()
    } else {
        s
    };
    let t = s.trim_end_matches('0');
    if t.ends_with('.') {
        format!("{t}0")
    } else {
        t.to_owned()
    }
}

/// One door of a drop, as its SQL file places it.
struct DoorRow {
    name: String,
    weenie: u32,
    /// The cell as written: a room's token, or an outdoor cell's id.
    cell: String,
    frame: Frame,
}

/// A door's SQL file text, stamped `today`.
fn door_sql(lb: u16, d: &DoorRow, today: &str) -> String {
    let guid = format!("{{{{guid:{}@{lb:04X}}}}}", d.name);
    let (o, q) = (d.frame.origin, d.frame.rotation);
    format!(
        "/* {}, a door placed in landblock {lb:04X} with its building. One instance: no other \
         instance of the landblock is touched. */\n\
         DELETE FROM `landblock_instance` WHERE `guid` = {guid};\n\
         \n\
         INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, \
         `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, \
         `is_Link_Child`, `last_Modified`)\n\
         VALUES ({guid}, {}, {}, {}, {}, {}, {}, {}, {}, {}, False, '{today}');\n",
        d.name,
        d.weenie,
        d.cell,
        crate::edit::meters(o.x),
        crate::edit::meters(o.y),
        crate::edit::meters(o.z),
        six(q.w),
        six(q.x),
        six(q.y),
        six(q.z),
    )
}

/// The land cells (world grid, 24 meters each) the shells `hulls` placed at `frames` (landblock
/// `lb`'s meters) stand over.
fn cells_under(lb: u16, shells: &[(&buildings::Hull, Frame)]) -> BTreeSet<(i64, i64)> {
    let (bx, by) = (i64::from(lb >> 8), i64::from(lb & 0xFF));
    let mut out = BTreeSet::new();
    for (h, f) in shells {
        for [x, y] in &h.points {
            let g = localtoglobal(f, Vec3::new(*x, *y, 0.0));
            let (cx, cy) = buildings::land_cell(g.x, g.y);
            out.insert((bx * 8 + i64::from(cx), by * 8 + i64::from(cy)));
        }
    }
    out
}

/// The world-grid vertices at the corners of the land cells `cells` (world grid).
fn corners(cells: &BTreeSet<(i64, i64)>) -> BTreeSet<(u32, u32)> {
    let mut out = BTreeSet::new();
    for (cx, cy) in cells {
        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            if let (Ok(gx), Ok(gy)) = (u32::try_from(cx + dx), u32::try_from(cy + dy)) {
                out.insert((gx, gy));
            }
        }
    }
    out
}

/// Where a drop would stand, and what the checks say of it there.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    /// Each building of the set: its model and its frame (landblock meters), the lead first.
    pub shells: Vec<(DataId, Frame)>,
    /// Why it is refused there, when it is.
    pub refusal: Option<String>,
    /// The building of the landblock whose land cell it would stand in, when that is why.
    pub taken: Option<usize>,
    /// What it is warned of there, which stops nothing.
    pub warnings: Vec<String>,
}

/// A drop worked out: the set's templates and furniture, its name, where each building stands,
/// the level of the ground under it (or why the buildings around let it have none), each
/// shell's hull, and the doors and outdoor statics that come with it.
struct Worked {
    templates: Vec<Template>,
    furnished: Furnished,
    /// Its name, bare, and the pack it is written in.
    name: String,
    pack: String,
    to: Frame,
    frames: Vec<Frame>,
    level: u8,
    footing: Option<String>,
    hulls: Vec<buildings::Hull>,
    doors: Vec<buildings::Door>,
    outdoor: Vec<buildings::Outdoor>,
    /// Its kind, and what makes it a house players can buy, when it is dropped as one.
    kind: buildings::Kind,
    house: Option<buildings::House>,
}

/// How far from the ground under its origin a drop may stand to be on the level of the buildings
/// around it, in meters.
pub const MOST_LIFT: f32 = 3.0;

/// How near another building's origin must be for its land cells to be looked at, in meters: no
/// shell reaches this far.
const NEAR_ENOUGH: f32 = 96.0;

/// A building whose land cells hold corners a drop may not change: its landblock, its place in
/// that landblock's compiled building list, and its model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Neighbor {
    landblock: u16,
    index: usize,
    model: DataId,
}

impl Session {
    /// The level of the ground at `(x, y)` of landblock `lb`: the height table's entry nearest the
    /// ground there, and its height.
    fn level_at(&self, lb: u16, x: f32, y: f32) -> Result<(u8, f32), Error> {
        let terrain = self
            .terrain(lb)
            .ok_or_else(|| Error::Refused(format!("the files have no landblock {lb:04X}")))?;
        let region = self.region();
        let table = region.map(dereth_terrain::land::mesh::height_table);
        let ground = region.map_or(0.0, |r| compile::ground(&terrain, r, lb, x, y));
        let level = table
            .as_ref()
            .map_or(0, |t| crate::brush::nearest_height(t, ground));
        let z = table.as_ref().map_or(ground, |t| t[usize::from(level)]);
        Ok((level, z))
    }

    /// The height of height index `level`, through the region's table.
    fn level_z(&self, level: u8) -> f32 {
        self.region().map_or(f32::from(level), |r| {
            dereth_terrain::land::mesh::height_table(r)[usize::from(level)]
        })
    }

    /// Every world-grid vertex at a corner of a land cell that another building stands over, with
    /// that building: the buildings of landblock `lb` and of the eight around it whose origins are
    /// near `(x, y)` (`lb`'s meters), the world's own and the packs' alike, but those of `lb` at
    /// `except`.
    fn held_corners(
        &self,
        lb: u16,
        (x, y): (f32, f32),
        except: &[usize],
    ) -> BTreeMap<(u32, u32), Neighbor> {
        let (bx, by) = (i32::from(lb >> 8), i32::from(lb & 0xFF));
        let mut models = crate::pick::Models::default();
        let mut out = BTreeMap::new();
        for dx in -1..=1 {
            for dy in -1..=1 {
                let (hx, hy) = (bx + dx, by + dy);
                if !(0..0xFF).contains(&hx) || !(0..0xFF).contains(&hy) {
                    continue;
                }
                let Ok(here) = u16::try_from((hx << 8) | hy) else {
                    continue;
                };
                let Some(info) = self.statics(here) else {
                    continue;
                };
                #[allow(clippy::cast_precision_loss)] // a landblock's step, -1 to 1
                let (sx, sy) = (dx as f32 * 192.0, dy as f32 * 192.0);
                for (index, b) in info.buildings.iter().enumerate() {
                    if here == lb && except.contains(&index) {
                        continue;
                    }
                    let o = b.frame.origin;
                    if (o.x + sx - x).abs() > NEAR_ENOUGH || (o.y + sy - y).abs() > NEAR_ENOUGH {
                        continue;
                    }
                    let hull = buildings::hull(&mut models, self.base(), b.id);
                    let n = Neighbor {
                        landblock: here,
                        index,
                        model: b.id,
                    };
                    for c in corners(&cells_under(here, &[(&hull, b.frame)])) {
                        out.entry(c).or_insert(n);
                    }
                }
            }
        }
        out
    }

    /// What a gallery would call building `n`, for a sentence: "the cottage", "the building".
    fn neighbor_name(&self, n: &Neighbor) -> String {
        let kind = self
            .buildings()
            .and_then(|c| c.get(n.model))
            .map(|e| e.kind);
        let word = match kind {
            Some(
                k @ (buildings::Kind::Cottage | buildings::Kind::Villa | buildings::Kind::Mansion),
            ) => k.name().to_lowercase(),
            Some(buildings::Kind::Entrance) => "dungeon entrance".into(),
            _ => "building".into(),
        };
        format!("the {word}")
    }

    /// The level the ground under shells `shells` (of landblock `lb`, the origin of their lead
    /// at `at`) is made: `natural` (the height table's entry nearest the ground under the
    /// origin), unless a land cell it must level shares corners with one another building stands
    /// over (the buildings of `lb` at `except` left out). Then it takes the height of those
    /// corners, which it may not change.
    ///
    /// # Errors
    /// Those corners are not all at one height (it would tilt the ground under the building next
    /// door), or their height is more than [`MOST_LIFT`] from the ground under the origin; the
    /// reason, in plain words.
    fn footing(
        &self,
        lb: u16,
        shells: &[(&buildings::Hull, Frame)],
        natural: u8,
        at: (f32, f32),
        except: &[usize],
    ) -> Result<u8, String> {
        let held = self.held_corners(lb, at, except);
        let fixed: Vec<(u8, Neighbor)> = corners(&cells_under(lb, shells))
            .into_iter()
            .filter_map(|c| Some((self.vertex(c.0, c.1)?.height, *held.get(&c)?)))
            .collect();
        let Some((level, first)) = fixed.first().copied() else {
            return Ok(natural);
        };
        if let Some((_, other)) = fixed.iter().find(|(h, _)| *h != level) {
            // Two buildings at two heights, or one standing on ground that is not level.
            let n = if fixed.iter().all(|(_, n)| *n == first) {
                first
            } else {
                *other
            };
            return Err(format!(
                "It would tilt the ground under {} next door.",
                self.neighbor_name(&n)
            ));
        }
        let ground = self.ground(lb, at.0, at.1).unwrap_or(0.0);
        let rise = self.level_z(level) - ground;
        if rise.abs() > MOST_LIFT {
            return Err(format!(
                "It would stand {:.1} m {} the ground here, on the level of {} next door.",
                rise.abs(),
                if rise > 0.0 { "above" } else { "below" },
                self.neighbor_name(&first)
            ));
        }
        Ok(level)
    }

    /// The retail building at each of `sites`, as the files under the packs hold it.
    fn templates_at(&self, sites: &[Site]) -> Result<Vec<Template>, Error> {
        let c = self.compiled();
        let read = |id: u32| compile::read_under(c, self.base(), ModernDat::Cell, id);
        sites
            .iter()
            .map(|s| {
                Template::read(&read, *s, self.region(), self.base().era).map_err(Error::Refused)
            })
            .collect()
    }

    /// The drop of the catalog's model `model` worked out (see [`Self::drop_building_as`]).
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn work_drop(
        &self,
        model: DataId,
        layout: usize,
        furnished: &Furnished,
        house: bool,
        landblock: u16,
        x: f32,
        y: f32,
        heading: f32,
    ) -> Result<Worked, Error> {
        let refuse = |why: String| Error::Refused(why);
        let catalog: &Catalog = self
            .buildings()
            .ok_or_else(|| refuse("the building catalog has not been read".into()))?;
        let mut entry = catalog.get(model).ok_or_else(|| {
            refuse(format!(
                "{:#010X} is not a building the world places",
                model.raw()
            ))
        })?;
        if !entry.is_lead() {
            if let Some(lead) = entry.set.iter().map(|p| p.model).min() {
                entry = catalog.get(lead).unwrap_or(entry);
            }
        }
        let chosen = entry.layouts.get(layout).ok_or_else(|| {
            refuse(format!(
                "{:#010X} has {} layout(s); there is no {layout}",
                entry.model.raw(),
                entry.layouts.len()
            ))
        })?;
        let region = self.region();
        let c = self.compiled();
        let read = |id: u32| compile::read_under(c, self.base(), ModernDat::Cell, id);
        let era = self.base().era;
        let read_t = |site: Site| Template::read(&read, site, region, era).map_err(&refuse);
        // The set's sites: its partners stand in the example's landblock, each in its place.
        let lead = read_t(chosen.example)?;
        let find_partner = |lb: u16, from: &Frame, p: &buildings::Partner| -> Option<Site> {
            let info = read((u32::from(lb) << 16) | 0xFFFE).and_then(|b| {
                LandblockInfo::decode_payload_for(era, DataId((u32::from(lb) << 16) | 0xFFFE), &b)
                    .ok()
            })?;
            let want = combine(from, &p.frame);
            info.buildings
                .iter()
                .position(|b| {
                    b.id == p.model
                        && (b.frame.origin.x - want.origin.x).abs() < buildings::TOGETHER
                        && (b.frame.origin.y - want.origin.y).abs() < buildings::TOGETHER
                })
                .map(|index| Site {
                    landblock: lb,
                    index,
                })
        };
        let mut templates = vec![lead];
        for p in &entry.set {
            let site = find_partner(chosen.example.landblock, &templates[0].building.frame, p)
                .ok_or_else(|| {
                    refuse(format!(
                        "{} stands without its partner {:#010X}",
                        chosen.example,
                        p.model.raw()
                    ))
                })?;
            templates.push(read_t(site)?);
        }
        let furnished = match furnished {
            Furnished::Like(sites) => {
                let lead_site = *sites
                    .first()
                    .ok_or_else(|| refuse("no furniture named".into()))?;
                let lead_f = read_t(lead_site)?;
                let mut all = vec![lead_site];
                for p in &entry.set {
                    all.push(
                        find_partner(lead_site.landblock, &lead_f.building.frame, p).ok_or_else(
                            || {
                                refuse(format!(
                                    "{lead_site} stands without its partner {:#010X}",
                                    p.model.raw()
                                ))
                            },
                        )?,
                    );
                }
                Furnished::Like(all)
            }
            other => other.clone(),
        };
        // Its name: its kind's, numbered, held by no drop or room of the landblock before.
        let target = &self.packs()[self.target()];
        let pack = target.name.clone();
        let stem = entry.kind.name().to_lowercase().replace(' ', "-");
        let used: BTreeSet<String> = target
            .edits
            .iter()
            .filter(|e| e.landblock == landblock)
            .flat_map(|e| e.drops.iter().map(|d| d.item.name.clone()))
            .collect();
        let held = |n: &str| {
            c.lock
                .get(
                    Kind::Cell,
                    &compile::cell_symbol(&room_name(&format!("{pack}:{n}"), 0), landblock),
                )
                .is_some()
        };
        let name = (1..)
            .map(|k| format!("{stem}-{k}"))
            .find(|n| !used.contains(&format!("{pack}:{n}")) && !held(n))
            .unwrap_or_default();
        // Where it stands: on the ground under its origin, made level, as high over it as the
        // template stands over its own.
        if !inside(x, y) {
            return Err(refuse(format!(
                "({x:.1}, {y:.1}) is outside landblock {landblock:04X}: a building's origin must \
                 be within 0 to 192 meters each way"
            )));
        }
        let (natural, _) = self.level_at(landblock, x, y)?;
        let mut models = crate::pick::Models::default();
        let hulls: Vec<buildings::Hull> = templates
            .iter()
            .map(|t| buildings::hull(&mut models, self.base(), t.building.id))
            .collect();
        // The level: the ground's, or that of the buildings around where it meets theirs.
        let flat = Frame::new(Vec3::new(x, y, 0.0), compile::heading_quat(heading));
        let shells: Vec<(&buildings::Hull, Frame)> = templates
            .iter()
            .zip(&hulls)
            .map(|(t, h)| {
                (
                    h,
                    carried(&t.building.frame, &templates[0].building.frame, &flat),
                )
            })
            .collect();
        let (level, footing) = match self.footing(landblock, &shells, natural, (x, y), &[]) {
            Ok(level) => (level, None),
            Err(why) => (natural, Some(why)),
        };
        let to = Frame::new(
            Vec3::new(x, y, self.level_z(level) + templates[0].height()),
            compile::heading_quat(heading),
        );
        let frames: Vec<Frame> = templates
            .iter()
            .map(|t| carried(&t.building.frame, &templates[0].building.frame, &to))
            .collect();
        Ok(Worked {
            furnished,
            name,
            pack,
            to,
            frames,
            level,
            footing,
            hulls,
            doors: if catalog.doors_known {
                chosen.doors.clone()
            } else {
                Vec::new()
            },
            outdoor: chosen.outdoor.clone(),
            kind: entry.kind,
            // A house only where every building of the set has its own: a room that belongs to
            // no spawn the client knows keeps everyone out.
            house: chosen
                .house
                .clone()
                .filter(|h| house && (0..templates.len()).all(|k| h.house_of(k).is_some())),
            templates,
        })
    }

    /// What the checks say of drop `w` in landblock `landblock`: why it is refused (and the
    /// building whose land cell it would share, when that is why), and what it is warned of.
    fn judge_drop(
        &self,
        w: &Worked,
        landblock: u16,
    ) -> (Option<(String, Option<usize>)>, Vec<String>) {
        let name = &w.name;
        let info = self.statics(landblock).unwrap_or(LandblockInfo {
            id: DataId((u32::from(landblock) << 16) | 0xFFFE),
            num_cells: 0,
            objects: Vec::new(),
            pack_mask: 0,
            buildings: Vec::new(),
            restrictions: None,
        });
        let new: Vec<BuildInfo> = w
            .templates
            .iter()
            .zip(&w.frames)
            .map(|(t, f)| BuildInfo {
                frame: *f,
                ..t.building.clone()
            })
            .collect();
        if let Err(why) = check_sites(landblock, name, &info, &new) {
            let taken = site_refusal(&info, &new).and_then(|(_, i)| i);
            return (Some((why, taken)), Vec::new());
        }
        if let Some(why) = &w.footing {
            return (Some((why.clone(), None)), Vec::new());
        }
        if let Some(why) = w
            .house
            .as_ref()
            .and_then(|h| self.house_refusal(w, h, landblock))
        {
            return (Some((why, None)), Vec::new());
        }
        let c = self.compiled();
        let rooms: usize = w.templates.iter().map(|t| t.rooms.len()).sum();
        let top = self
            .cells(landblock)
            .keys()
            .copied()
            .map(u32::from)
            .chain(
                c.lock
                    .of(Kind::Cell)
                    .filter(|(_, id)| id >> 16 == u32::from(landblock))
                    .map(|(_, id)| id & 0xFFFF),
            )
            .max()
            .map_or(u32::from(crate::cells::FIRST_CELL), |n| n + 1);
        if top + u32::try_from(rooms).unwrap_or(u32::MAX) > 0xFFFE {
            return (
                Some((
                    format!(
                        "landblock {landblock:04X} has no cell numbers left for {name}'s {rooms} \
                         room(s)"
                    ),
                    None,
                )),
                Vec::new(),
            );
        }
        let held_ids = self.server_instances(landblock)
            + c.lock
                .of(Kind::Guid)
                .filter(|(_, id)| (id >> 12) & 0xFFFF == u32::from(landblock))
                .count();
        let house_spawns = w.house.as_ref().map_or(0, |h| {
            h.objects
                .iter()
                .filter(|o| !matches!(o.spot, buildings::Spot::Basement(_)))
                .count()
        });
        if held_ids + w.doors.len() + house_spawns > INSTANCES {
            return (
                Some((
                    format!(
                        "landblock {landblock:04X} has {} instance id(s) left, and {name} has {} \
                         door(s) and {house_spawns} spawn(s) of its house",
                        INSTANCES.saturating_sub(held_ids),
                        w.doors.len()
                    ),
                    None,
                )),
                Vec::new(),
            );
        }
        (None, self.reach_warnings(name, &new))
    }

    /// The warning for each of `shells` (of drop `name`) whose shell reaches further into the
    /// next land cell than any retail copy of its model does, when the catalog is known.
    fn reach_warnings(&self, name: &str, shells: &[BuildInfo]) -> Vec<String> {
        let Some(catalog) = self.buildings() else {
            return Vec::new();
        };
        let mut models = crate::pick::Models::default();
        let mut out = Vec::new();
        for b in shells {
            let most = catalog.get(b.id).map_or(0.0, |e| e.reach);
            let r = buildings::reach(&buildings::hull(&mut models, self.base(), b.id), &b.frame);
            if r > most + 0.01 {
                out.push(format!(
                    "{name}: its shell ({:#010X}) reaches {r:.1} m into the next land cell; no \
                     retail copy of it reaches more than {most:.1} m",
                    b.id.raw()
                ));
            }
        }
        out
    }

    /// Where the drop of the catalog's model `model` (its `layout`th layout) would stand with
    /// its origin at `(x, y)` of `landblock`, turned `heading` degrees clockwise from north, and
    /// whether it may: what the place tool shows before it drops.
    ///
    /// # Errors
    /// No catalog, a model or layout it does not hold, a place outside the landblock.
    #[allow(clippy::too_many_arguments)]
    pub fn drop_plan(
        &self,
        model: DataId,
        layout: usize,
        landblock: u16,
        x: f32,
        y: f32,
        heading: f32,
    ) -> Result<Plan, Error> {
        self.drop_plan_as(model, layout, true, landblock, x, y, heading)
    }

    /// [`Self::drop_plan`] of a drop as a house players can buy (`house`, where the model is a
    /// house and the catalog knows it), or as a plain building.
    ///
    /// # Errors
    /// As [`Self::drop_plan`].
    #[allow(clippy::too_many_arguments)]
    pub fn drop_plan_as(
        &self,
        model: DataId,
        layout: usize,
        house: bool,
        landblock: u16,
        x: f32,
        y: f32,
        heading: f32,
    ) -> Result<Plan, Error> {
        let w = self.work_drop(
            model,
            layout,
            &Furnished::Bare,
            house,
            landblock,
            x,
            y,
            heading,
        )?;
        let (refused, warnings) = self.judge_drop(&w, landblock);
        Ok(Plan {
            shells: w
                .templates
                .iter()
                .zip(&w.frames)
                .map(|(t, f)| (t.building.id, *f))
                .collect(),
            taken: refused.as_ref().and_then(|(_, t)| *t),
            refusal: refused.map(|(why, _)| why),
            warnings,
        })
    }

    /// Where a drop of the catalog's model `model` (its `layout`th layout) stands in the land
    /// cell holding `(x, y)` (landblock meters), turned `turns` quarter turns from its example:
    /// the example's own place in its land cell, turned about the cell's middle. Its origin's
    /// `(x, y)` and its heading, degrees clockwise from north. A retail copy laid out so overhangs
    /// its land cell no more than its example does.
    #[must_use]
    pub fn drop_snap(
        &self,
        model: DataId,
        layout: usize,
        x: f32,
        y: f32,
        turns: u8,
    ) -> Option<(f32, f32, f32)> {
        let catalog = self.buildings()?;
        let mut entry = catalog.get(model)?;
        if !entry.is_lead() {
            entry = catalog.get(entry.set.iter().map(|p| p.model).min()?)?;
        }
        let example = entry.layouts.get(layout)?.example;
        let f = self.templates_at(&[example]).ok()?.first()?.building.frame;
        let middle = |px: f32, py: f32| {
            let (cx, cy) = buildings::land_cell(px, py);
            #[allow(clippy::cast_precision_loss)] // a land cell's index
            (cx as f32 * 24.0 + 12.0, cy as f32 * 24.0 + 12.0)
        };
        let (mx, my) = middle(f.origin.x, f.origin.y);
        let (ox, oy) = (f.origin.x - mx, f.origin.y - my);
        let turn = f32::from(turns % 4) * 90.0;
        // Clockwise, as a heading turns.
        let (s, c) = (
            dereth_primitives::num::math::sinf(turn.to_radians()),
            dereth_primitives::num::math::cosf(turn.to_radians()),
        );
        let (rx, ry) = (ox * c + oy * s, -ox * s + oy * c);
        let (nx, ny) = middle(x.clamp(0.0, 191.99), y.clamp(0.0, 191.99));
        Some((
            nx + rx,
            ny + ry,
            (crate::edit::heading_of(f.rotation) + turn).rem_euclid(360.0),
        ))
    }

    /// The drop of the catalog's model `model` (its `layout`th layout, and its set with it from
    /// the set's lead) with its origin at `(x, y)` of `landblock`, turned `heading` degrees
    /// clockwise from north, furnished as `furnished` says (another copy's furniture named by
    /// the lead's copy): one step, and what it warns of.
    ///
    /// # Errors
    /// No catalog, a model or layout it does not hold, and the refusals of a place: outside the
    /// landblock, a land cell that holds a building already, no room or instance numbers left.
    #[allow(clippy::too_many_arguments)]
    pub fn drop_building(
        &self,
        model: DataId,
        layout: usize,
        furnished: &Furnished,
        landblock: u16,
        x: f32,
        y: f32,
        heading: f32,
    ) -> Result<(Op, Vec<String>), Error> {
        self.drop_building_as(model, layout, furnished, true, landblock, x, y, heading)
    }

    /// [`Self::drop_building`] as a house players can buy (`house`, where the model is a house
    /// and the catalog knows its house), or as a plain building. A house comes with its own
    /// house weenie, its slumlord, storage, hooks, boot spot and house portals, linked as its
    /// example's are, and its basement copied into the nearest landblock of open sea floor that
    /// nothing uses; its rooms and its land cell belong to its house.
    ///
    /// # Errors
    /// As [`Self::drop_building`], and for a house: its house or slumlord would stand outside the
    /// landblock, the landblock has a mansion players can buy already, no landblock is free for
    /// its basement.
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    pub fn drop_building_as(
        &self,
        model: DataId,
        layout: usize,
        furnished: &Furnished,
        house: bool,
        landblock: u16,
        x: f32,
        y: f32,
        heading: f32,
    ) -> Result<(Op, Vec<String>), Error> {
        let w = self.work_drop(model, layout, furnished, house, landblock, x, y, heading)?;
        let (refused, warnings) = self.judge_drop(&w, landblock);
        if let Some((why, _)) = refused {
            return Err(Error::Refused(why));
        }
        let name = &w.name;
        let mut warnings = warnings;
        if !self.buildings().is_some_and(|c| c.doors_known) {
            warnings.push(format!(
                "{name}: its doors are not placed: the server's world database was not read"
            ));
        }
        let mut ops = Vec::new();
        // The ground: every corner of the land cells under the shells set to the level, and the
        // scenery they would stand over cleared.
        let shells: Vec<(&buildings::Hull, Frame)> =
            w.hulls.iter().zip(w.frames.iter().copied()).collect();
        let ground = self.ground_op(landblock, &shells, w.level, &[]);
        let leveled = match &ground {
            Some(Op::SetVertices(sets)) => sets.clone(),
            _ => Vec::new(),
        };
        ops.extend(ground);
        // A house: its basement's landblock, and the spawn each building's rooms belong to.
        let root = w
            .house
            .as_ref()
            .and_then(|h| Some(h.objects[h.root()?].member));
        let basement = match w.house.as_ref().and_then(|h| h.basement) {
            Some(template) => Some((
                self.free_dungeon_near(landblock).ok_or_else(|| {
                    Error::Refused(format!(
                        "no landblock of open sea floor near {landblock:04X} is free for {name}'s \
                         basement"
                    ))
                })?,
                template,
            )),
            None => None,
        };
        let house_names: Vec<String> = match (&w.house, root) {
            (Some(_), Some(root)) => (0..w.templates.len())
                .map(|k| format!("{}:{}", w.pack, house_name(name, k, root)))
                .collect(),
            _ => Vec::new(),
        };
        ops.push(Op::AddBuilding {
            landblock,
            building: AddBuilding {
                name: format!("{}:{name}", w.pack),
                models: w.templates.iter().map(|t| t.building.id).collect(),
                templates: w.templates.iter().map(|t| t.site).collect(),
                at: Place {
                    x,
                    y,
                    z: Height::At(w.to.origin.z),
                },
                rotation: Rotation::Heading(heading),
                furnished: w.furnished.clone(),
                house: house_names,
            },
        });
        // The model's outdoor statics, carried with it, each named after it.
        for (k, o) in w.outdoor.iter().enumerate() {
            let f = combine(&w.to, &o.frame);
            if !inside(f.origin.x, f.origin.y) {
                continue;
            }
            let q = f.rotation;
            ops.push(Op::PlaceStatic {
                landblock,
                model: o.model,
                at: Place {
                    x: f.origin.x,
                    y: f.origin.y,
                    z: Height::At(f.origin.z),
                },
                rotation: Rotation::Quat([q.w, q.x, q.y, q.z]),
                name: Some(outdoor_name(name, k)),
            });
        }
        // Its doors, each in the room it stands in or the land cell outside.
        let mut offsets = Vec::new();
        let mut k = 0;
        for t in &w.templates {
            offsets.push(k);
            k += t.rooms.len();
        }
        let first = u32::from(landblock) << 16;
        let dir = self.packs()[self.target()]
            .dir
            .join("server")
            .join("sql")
            .join("landblocks");
        for (i, d) in w.doors.iter().enumerate() {
            let (Some(t), Some(at)) = (w.templates.get(d.member), w.frames.get(d.member)) else {
                continue;
            };
            let f = combine(at, &d.frame);
            if !inside(f.origin.x, f.origin.y) {
                warnings.push(format!(
                    "{name}: door {i} would stand outside the landblock, and is not placed"
                ));
                continue;
            }
            let cell = match d.room {
                Some(r) => {
                    let Some(pos) = t.rooms.keys().position(|n| *n == r) else {
                        warnings.push(format!(
                            "{name}: door {i} stands in cell {r:04X} of {}, which is none of \
                             its rooms, and is not placed",
                            t.site
                        ));
                        continue;
                    };
                    format!(
                        "{{{{cell:{}@{landblock:04X}}}}}",
                        room_name(name, offsets[d.member] + pos)
                    )
                }
                None => format!(
                    "0x{:08X}",
                    first | crate::edit::outdoor_cell(f.origin.x, f.origin.y)
                ),
            };
            let row = DoorRow {
                name: door_name(name, i),
                weenie: d.weenie,
                cell,
                frame: f,
            };
            ops.push(Op::WriteSql {
                file: dir.join(format!("{landblock:04X} {}.sql", row.name)),
                text: Some(door_sql(landblock, &row, self.stamp())),
            });
        }
        // Its house, and its basement.
        let is_house = matches!(
            w.kind,
            buildings::Kind::Cottage | buildings::Kind::Villa | buildings::Kind::Mansion
        );
        if house && is_house && w.house.is_none() {
            warnings.push(format!(
                "{name}: dropped as a plain building: {}",
                if self.buildings().is_some_and(|c| c.doors_known) {
                    "the server's world database has no house for it"
                } else {
                    "the server's world database was not read"
                }
            ));
        }
        if let Some(h) = &w.house {
            let at = HouseAt {
                lb: landblock,
                name,
                templates: &w.templates,
                frames: &w.frames,
                hulls: &w.hulls,
                leveled: &leveled,
                basement,
            };
            let (files, basement_row, more) =
                self.house_files(&at, h, w.kind).map_err(Error::Refused)?;
            warnings.extend(more);
            if let Some((b, row)) = basement_row {
                ops.push(Op::AddDungeon {
                    landblock: b,
                    dungeon: row,
                });
            }
            for (file, text) in files {
                ops.push(Op::WriteSql {
                    file,
                    text: Some(text),
                });
            }
        }
        Ok((Op::Batch(ops), warnings))
    }

    /// The ground under the shells `shells` of landblock `lb` made level at height index
    /// `level`: every corner of the land cells under them set to it, and the scenery they would
    /// stand over in a land cell not holding a building's origin cleared (`except`: buildings of
    /// `lb` whose origins are left out, as moving away). `None` when nothing changes.
    fn ground_op(
        &self,
        lb: u16,
        shells: &[(&buildings::Hull, Frame)],
        level: u8,
        except: &[usize],
    ) -> Option<Op> {
        let under = cells_under(lb, shells);
        let mut sets: BTreeMap<(u32, u32, Field), u16> = BTreeMap::new();
        for (gx, gy) in corners(&under) {
            if self.vertex(gx, gy).is_some_and(|v| v.height != level) {
                sets.insert((gx, gy, Field::Height), u16::from(level));
            }
        }
        if let Some(region) = self.region() {
            for (gx, gy, scene) in self.scenery_under(lb, shells, &under, level, region, except) {
                sets.insert((gx, gy, Field::Scene), scene);
            }
        }
        (!sets.is_empty()).then(|| {
            Op::SetVertices(
                sets.into_iter()
                    .map(|((gx, gy, field), value)| VertexSet {
                        gx,
                        gy,
                        field,
                        value,
                    })
                    .collect(),
            )
        })
    }

    // -----------------------------------------------------------------------------------------
    // A dropped building moved or taken away.
    // -----------------------------------------------------------------------------------------

    /// The dropped building at place `index` of landblock `lb`'s compiled building list: its
    /// name (its symbol, in full) and the places of its set's buildings, the lead first.
    #[must_use]
    pub fn dropped_building(&self, lb: u16, index: usize) -> Option<(String, Vec<usize>)> {
        let list = self.compiled().dropped.get(&lb)?;
        let name = list.iter().find(|(_, i)| *i == index)?.0.clone();
        let places = list
            .iter()
            .filter(|(n, _)| *n == name)
            .map(|(_, i)| *i)
            .collect();
        Some((name, places))
    }

    /// Dropped building `name`'s row in landblock `lb`, the target pack's.
    ///
    /// # Errors
    /// Another pack drops it: it is changed there.
    fn own_drop(&self, lb: u16, name: &str) -> Result<AddBuilding, Error> {
        for (pi, p) in self.packs().iter().enumerate() {
            let found = p
                .edits
                .iter()
                .filter(|e| e.landblock == lb)
                .flat_map(|e| e.drops.iter())
                .find(|d| d.item.name == name);
            if let Some(d) = found {
                if pi != self.target() {
                    return Err(Error::Refused(format!(
                        "building {name} is dropped by pack {}; it is changed there",
                        p.name
                    )));
                }
                return Ok(d.item.clone());
            }
        }
        Err(Error::Refused(format!(
            "landblock {lb:04X} has no dropped building {name}"
        )))
    }

    /// The rooms of dropped building `name` of landblock `lb`, by their numbers.
    fn drop_rooms(&self, lb: u16, name: &str) -> BTreeSet<u16> {
        let head = format!("{name}-room-");
        self.compiled()
            .rooms
            .get(&lb)
            .map(|m| {
                m.iter()
                    .filter(|(n, _)| n.starts_with(&head))
                    .map(|(_, c)| *c)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The outdoor statics dropped building `name` (in full) of landblock `lb` came with, each
    /// with where it came from and its frame, as compiled.
    fn drop_statics(&self, lb: u16, name: &str) -> Vec<(StaticOrigin, Frame)> {
        let head = format!("{name}-outdoor-");
        let Some(info) = self.statics(lb) else {
            return Vec::new();
        };
        self.origins(lb)
            .into_iter()
            .zip(info.objects.iter())
            .filter(|(o, _)| {
                matches!(o, StaticOrigin::Pack { pack, name: Some(n), .. }
                    if *pack == self.target() && n.starts_with(&head))
            })
            .map(|(o, e)| (o, e.frame))
            .collect()
    }

    /// The target pack's spawns that go with dropped building `name` (in full) of landblock
    /// `lb`, whose set stands at `places` and whose rooms are `rooms`: its doors and those in its
    /// rooms, and with `over`, those outdoors over it too.
    fn drop_spawns(
        &self,
        lb: u16,
        name: &str,
        places: &[usize],
        rooms: &BTreeSet<u16>,
        over: bool,
    ) -> Vec<crate::edit::Spawn> {
        let bare = name.split_once(':').map_or(name, |(_, b)| b);
        let door = format!("{{{{guid:{bare}-door-");
        let mut out: Vec<crate::edit::Spawn> = Vec::new();
        let mut add = |s: crate::edit::Spawn| {
            if !out.iter().any(|o| o.file == s.file && o.guid == s.guid) {
                out.push(s);
            }
        };
        for s in self.spawns() {
            if s.server || s.pack != self.target() || s.landblock() != lb {
                continue;
            }
            let room = u16::try_from(s.cell & 0xFFFF).is_ok_and(|c| rooms.contains(&c));
            if s.guid.starts_with(&door) || room {
                add(s);
            }
        }
        if over {
            for i in places {
                for s in self.building_spawns(lb, *i) {
                    if !s.server && s.pack == self.target() {
                        add(s);
                    }
                }
            }
        }
        out
    }

    /// The world-grid vertices the dropped buildings of landblock `lb` and those around it level,
    /// but those at `places` of `lb`.
    fn other_drops_corners(&self, lb: u16, places: &[usize]) -> BTreeSet<(u32, u32)> {
        let (bx, by) = (i32::from(lb >> 8), i32::from(lb & 0xFF));
        let mut models = crate::pick::Models::default();
        let mut out = BTreeSet::new();
        for dx in -1..=1 {
            for dy in -1..=1 {
                let (x, y) = (bx + dx, by + dy);
                let Ok(here) = u16::try_from((x << 8) | y) else {
                    continue;
                };
                if !(0..0xFF).contains(&x) || !(0..0xFF).contains(&y) {
                    continue;
                }
                let Some(list) = self.compiled().dropped.get(&here) else {
                    continue;
                };
                let Some(info) = self.statics(here) else {
                    continue;
                };
                let hulls: Vec<(buildings::Hull, Frame)> = list
                    .iter()
                    .filter(|(_, i)| here != lb || !places.contains(i))
                    .filter_map(|(_, i)| info.buildings.get(*i))
                    .map(|b| (buildings::hull(&mut models, self.base(), b.id), b.frame))
                    .collect();
                let shells: Vec<(&buildings::Hull, Frame)> =
                    hulls.iter().map(|(h, f)| (h, *f)).collect();
                out.extend(corners(&cells_under(here, &shells)));
            }
        }
        out
    }

    /// Dropped building `index` of landblock `lb` (any building of its set) moved to `at` and
    /// turned `rotation`: its set carried by the one move, standing on the ground there made
    /// level as high over it as its template stands; the ground it stood on given back to what
    /// the packs under it and the world make it (but where another drop stands); its row written
    /// anew; its doors, outdoor statics, and the spawns and statics in its rooms and the spawns
    /// over it carried. One step.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn move_dropped(
        &mut self,
        lb: u16,
        index: usize,
        at: [f32; 3],
        rotation: Rotation,
    ) -> Result<crate::edit::Applied, Error> {
        let (name, places) = self
            .dropped_building(lb, index)
            .ok_or_else(|| Error::Refused(format!("landblock {lb:04X} has no building {index}")))?;
        let row = self.own_drop(lb, &name)?;
        let bare = name
            .split_once(':')
            .map_or(name.as_str(), |(_, b)| b)
            .to_owned();
        let info = self
            .statics(lb)
            .ok_or_else(|| Error::Refused(format!("landblock {lb:04X} has no buildings")))?;
        let builds: Vec<BuildInfo> = places
            .iter()
            .filter_map(|i| info.buildings.get(*i).cloned())
            .collect();
        if builds.len() != places.len() {
            return Err(Error::Refused(format!(
                "building {bare} is not where the compiled world has it"
            )));
        }
        let frames: Vec<Frame> = builds.iter().map(|b| b.frame).collect();
        let member = places.iter().position(|i| *i == index).unwrap_or(0);
        let moved = Frame::new(Vec3::new(at[0], at[1], at[2]), compile::quat_of(rotation));
        let lead = carried(&frames[0], &frames[member], &moved);
        let heading = crate::edit::heading_of(lead.rotation).rem_euclid(360.0);
        let (x, y) = (lead.origin.x, lead.origin.y);
        // Where it would stand, among the landblock's other buildings.
        let kept: Vec<usize> = (0..info.buildings.len())
            .filter(|i| !places.contains(i))
            .collect();
        let others = LandblockInfo {
            buildings: kept.iter().map(|i| info.buildings[*i].clone()).collect(),
            ..info.clone()
        };
        let flat = Frame::new(
            Vec3::new(x, y, frames[0].origin.z),
            compile::heading_quat(heading),
        );
        let there: Vec<BuildInfo> = builds
            .iter()
            .map(|b| BuildInfo {
                frame: carried(&b.frame, &frames[0], &flat),
                ..b.clone()
            })
            .collect();
        if let Some((k, taken)) = site_refusal(&others, &there) {
            let o = there[k].frame.origin;
            return Err(Error::Refused(match taken {
                None => format!(
                    "building {bare} would stand at ({:.1}, {:.1}), outside landblock {lb:04X}: \
                     its origin must be within 0 to 192 meters each way",
                    o.x, o.y
                ),
                Some(i) => {
                    let (index, model) = kept
                        .get(i)
                        .map_or((i, 0), |j| (*j, info.buildings[*j].id.raw()));
                    format!(
                        "building {bare} would stand at ({:.1}, {:.1}), in the land cell of \
                         landblock {lb:04X} that building {index} ({model:#010X}) stands in: the \
                         client draws only one building in each 24-meter land cell",
                        o.x, o.y
                    )
                }
            }));
        }
        let templates = self.templates_at(&row.templates)?;
        let rooms = self.drop_rooms(lb, &name);
        let statics = self.drop_statics(lb, &name);
        // A house's spawns are written again whole where it stands now; the rest are carried.
        let house = if row.house.is_empty() {
            None
        } else {
            Some(self.house_of_row(&row).ok_or_else(|| {
                Error::Refused(format!(
                    "{bare} is a house: moving it needs the building catalog read with the \
                     server's world database"
                ))
            })?)
        };
        let basement = self.basement_of(lb, &bare);
        let dir = self.packs()[self.target()].dir.join("server").join("sql");
        let house_files = house_paths(&dir, lb, &bare, basement.map(|(b, _)| b));
        let spawns: Vec<crate::edit::Spawn> = self
            .drop_spawns(lb, &name, &places, &rooms, true)
            .into_iter()
            .filter(|s| house.is_none() || !house_files.contains(&s.file))
            .collect();
        let mut models = crate::pick::Models::default();
        let hulls: Vec<buildings::Hull> = builds
            .iter()
            .map(|b| buildings::hull(&mut models, self.base(), b.id))
            .collect();
        let start = self.steps();
        let done = (|| -> Result<crate::edit::Applied, Error> {
            let mut applied = crate::edit::Applied::default();
            // The ground it stood on, as the packs under it and the world make it.
            let old: Vec<(&buildings::Hull, Frame)> =
                hulls.iter().zip(frames.iter().copied()).collect();
            let keep = self.other_drops_corners(lb, &places);
            let unset: Vec<(u32, u32, Field)> = corners(&cells_under(lb, &old))
                .difference(&keep)
                .flat_map(|(gx, gy)| [(*gx, *gy, Field::Height), (*gx, *gy, Field::Scene)])
                .collect();
            if !unset.is_empty() {
                let a = self.apply_drafted(&[Op::UnsetVertices(unset)])?;
                applied.touched.add(a.touched);
            }
            // Level where it stands now: on the ground's level, or on that of the buildings
            // around where it meets theirs.
            let (natural, _) = self.level_at(lb, x, y)?;
            let flat = Frame::new(Vec3::new(x, y, 0.0), compile::heading_quat(heading));
            let there: Vec<(&buildings::Hull, Frame)> = hulls
                .iter()
                .zip(frames.iter().map(|f| carried(f, &frames[0], &flat)))
                .collect();
            let level = self
                .footing(lb, &there, natural, (x, y), &places)
                .map_err(Error::Refused)?;
            let to = Frame::new(
                Vec3::new(x, y, self.level_z(level) + templates[0].height()),
                compile::heading_quat(heading),
            );
            let now: Vec<Frame> = frames.iter().map(|f| carried(f, &frames[0], &to)).collect();
            let shells: Vec<(&buildings::Hull, Frame)> =
                hulls.iter().zip(now.iter().copied()).collect();
            let mut ops = Vec::new();
            let ground = self.ground_op(lb, &shells, level, &places);
            let leveled = match &ground {
                Some(Op::SetVertices(sets)) => sets.clone(),
                _ => Vec::new(),
            };
            ops.extend(ground);
            if let Some((kind, h)) = &house {
                let at = HouseAt {
                    lb,
                    name: &bare,
                    templates: &templates,
                    frames: &now,
                    hulls: &hulls,
                    leveled: &leveled,
                    basement,
                };
                let (files, _, more) = self.house_files(&at, h, *kind).map_err(Error::Refused)?;
                applied.warnings.extend(more);
                for (file, text) in files {
                    ops.push(Op::WriteSql {
                        file,
                        text: Some(text),
                    });
                }
            }
            ops.push(Op::AddBuilding {
                landblock: lb,
                building: AddBuilding {
                    at: Place {
                        x,
                        y,
                        z: Height::At(to.origin.z),
                    },
                    rotation: Rotation::Heading(heading),
                    ..row.clone()
                },
            });
            for (which, f) in &statics {
                let g = carried(f, &frames[0], &to);
                let q = g.rotation;
                ops.push(Op::MoveStatic {
                    landblock: lb,
                    which: which.clone(),
                    at: Some(Place {
                        x: g.origin.x,
                        y: g.origin.y,
                        z: Height::At(g.origin.z),
                    }),
                    rotation: Some(Rotation::Quat([q.w, q.x, q.y, q.z])),
                });
            }
            for s in &spawns {
                let g = carried(&s.frame, &frames[0], &to);
                ops.push(Op::MoveSpawn {
                    file: s.file.clone(),
                    guid: s.guid.clone(),
                    origin: g.origin,
                    heading: crate::edit::heading_of(g.rotation).rem_euclid(360.0),
                });
            }
            let mut texts = BTreeMap::new();
            for op in &ops {
                self.draft_into(op, &mut texts)?;
            }
            // The statics added to its rooms, and those of its rooms moved, by the same move.
            self.carry_room_statics(&mut texts, lb, &rooms, &frames[0], &to)?;
            let a = self.apply_texts(texts)?;
            applied.touched.add(a.touched);
            let now: Vec<BuildInfo> = builds
                .iter()
                .zip(&now)
                .map(|(b, f)| BuildInfo {
                    frame: *f,
                    ..b.clone()
                })
                .collect();
            applied.warnings.extend(self.reach_warnings(&bare, &now));
            Ok(applied)
        })();
        if done.is_err() {
            self.roll_back_to(start);
        }
        done
    }

    /// Dropped building `index` of landblock `lb` (any building of its set) taken away: its row,
    /// so its set and its rooms; its doors and outdoor statics; the spawns in its rooms and the
    /// rows that add or move statics there; and the ground it stood on given back to what the
    /// packs under it and the world make it (but where another drop stands). One step.
    ///
    /// # Errors
    /// One of the world's own buildings, another pack's drop, or a spawn's file that places
    /// others too.
    pub(crate) fn remove_dropped(
        &mut self,
        lb: u16,
        index: usize,
    ) -> Result<crate::edit::Applied, Error> {
        let Some((name, places)) = self.dropped_building(lb, index) else {
            let n = self.statics(lb).map_or(0, |i| i.buildings.len());
            return Err(Error::Refused(if index < n {
                format!(
                    "building {index} of landblock {lb:04X} is one of the world's own; only a \
                     dropped building is taken away"
                )
            } else {
                format!("landblock {lb:04X} has no building {index}")
            }));
        };
        let row = self.own_drop(lb, &name)?;
        let bare = name
            .split_once(':')
            .map_or(name.as_str(), |(_, b)| b)
            .to_owned();
        // A house's files, and its basement: taken away whole.
        let basement = self.basement_of(lb, &bare);
        let dir = self.packs()[self.target()].dir.join("server").join("sql");
        let house_files: Vec<PathBuf> = if row.house.is_empty() {
            Vec::new()
        } else {
            let written = &self.packs()[self.target()].sql;
            house_paths(&dir, lb, &bare, basement.map(|(b, _)| b))
                .into_iter()
                .filter(|f| written.iter().any(|w| w.path == *f))
                .collect()
        };
        let info = self
            .statics(lb)
            .ok_or_else(|| Error::Refused(format!("landblock {lb:04X} has no buildings")))?;
        let mut models = crate::pick::Models::default();
        let hulls: Vec<(buildings::Hull, Frame)> = places
            .iter()
            .filter_map(|i| info.buildings.get(*i))
            .map(|b| (buildings::hull(&mut models, self.base(), b.id), b.frame))
            .collect();
        let shells: Vec<(&buildings::Hull, Frame)> = hulls.iter().map(|(h, f)| (h, *f)).collect();
        let keep = self.other_drops_corners(lb, &places);
        let unset: Vec<(u32, u32, Field)> = corners(&cells_under(lb, &shells))
            .difference(&keep)
            .flat_map(|(gx, gy)| [(*gx, *gy, Field::Height), (*gx, *gy, Field::Scene)])
            .collect();
        let rooms = self.drop_rooms(lb, &name);
        let mut ops = Vec::new();
        if !unset.is_empty() {
            ops.push(Op::UnsetVertices(unset));
        }
        // Its outdoor statics, from the last entry back: each names its entry as it still is.
        let mut statics: Vec<StaticOrigin> = self
            .drop_statics(lb, &name)
            .into_iter()
            .map(|(o, _)| o)
            .collect();
        statics.sort_by_key(|o| match o {
            StaticOrigin::Pack { entry, .. } => std::cmp::Reverse(*entry),
            StaticOrigin::Base(i) => std::cmp::Reverse(*i),
        });
        ops.extend(statics.into_iter().map(|which| Op::RemoveStatic {
            landblock: lb,
            which,
        }));
        let mut files: Vec<std::path::PathBuf> = Vec::new();
        for s in self.drop_spawns(lb, &name, &places, &rooms, false) {
            if !files.contains(&s.file) && !house_files.contains(&s.file) {
                files.push(s.file);
            }
        }
        ops.extend(files.into_iter().map(|file| Op::RemoveSpawn { file }));
        ops.extend(
            house_files
                .into_iter()
                .map(|file| Op::WriteSql { file, text: None }),
        );
        let mut texts = BTreeMap::new();
        for op in &ops {
            self.draft_into(op, &mut texts)?;
        }
        self.unwrite_drop(&mut texts, lb, &bare, &rooms)?;
        if let (false, Some((b, _))) = (row.house.is_empty(), basement) {
            self.unwrite_dungeon(&mut texts, b, &basement_name(&bare, lb))?;
        }
        self.apply_texts(texts)
    }

    /// The vertices whose scenery would stand under the shells `shells` of landblock `lb`, over
    /// a land cell not holding a building's origin (the buildings of `lb` at `except` left out),
    /// with the ground under `under` set to height index `level`: each with a scene value that
    /// grows nothing on its terrain.
    fn scenery_under(
        &self,
        lb: u16,
        shells: &[(&buildings::Hull, Frame)],
        under: &BTreeSet<(i64, i64)>,
        level: u8,
        region: &Region,
        except: &[usize],
    ) -> Vec<(u32, u32, u16)> {
        use dereth_terrain::land::mesh::{generate_landblock, Direction};
        use dereth_terrain::scenery::{generate_scenery, SceneryEnv, WithinBlockShape};
        let (bx, by) = (i64::from(lb >> 8), i64::from(lb & 0xFF));
        // The landblocks the land cells under the shells are in.
        let blocks: BTreeSet<(i64, i64)> = under
            .iter()
            .map(|(x, y)| (x.div_euclid(8), y.div_euclid(8)))
            .collect();
        let scenes = |id: DataId| {
            let base = self.base();
            base.read_portal(id)
                .and_then(|b| base.decode::<dereth_assets::world::Scene>(id, &b).ok())
        };
        let shape = |_: DataId| Some(WithinBlockShape::default());
        let mut empty: BTreeMap<u16, Option<u16>> = BTreeMap::new();
        let mut out = Vec::new();
        for (lx, ly) in blocks {
            let (Ok(ux), Ok(uy)) = (u16::try_from(lx), u16::try_from(ly)) else {
                continue;
            };
            if ux > 0xFE || uy > 0xFE {
                continue;
            }
            let here = (ux << 8) | uy;
            let Some(mut land) = self.terrain(here) else {
                continue;
            };
            for (cx, cy) in under {
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let (vx, vy) = (cx + dx - lx * 8, cy + dy - ly * 8);
                    if (0..=8).contains(&vx) && (0..=8).contains(&vy) {
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        let i = (vx * 9 + vy) as usize;
                        land.height[i] = level;
                    }
                }
            }
            // The shells in this landblock's meters, and the land cells holding origins.
            #[allow(clippy::cast_precision_loss)]
            let shift = Vec3::new(((bx - lx) * 192) as f32, ((by - ly) * 192) as f32, 0.0);
            let moved: Vec<(&buildings::Hull, Frame)> = shells
                .iter()
                .map(|(h, f)| {
                    (
                        *h,
                        Frame::new(
                            Vec3::new(f.origin.x + shift.x, f.origin.y + shift.y, f.origin.z),
                            f.rotation,
                        ),
                    )
                })
                .collect();
            let mut owns: BTreeSet<u16> = self
                .statics(here)
                .map(|i| {
                    i.buildings
                        .iter()
                        .enumerate()
                        .filter(|(k, _)| here != lb || !except.contains(k))
                        .map(|(_, b)| {
                            dereth_terrain::scenery::outside_cell_index(
                                b.frame.origin.x,
                                b.frame.origin.y,
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            for (_, f) in &moved {
                if inside(f.origin.x, f.origin.y) {
                    owns.insert(dereth_terrain::scenery::outside_cell_index(
                        f.origin.x, f.origin.y,
                    ));
                }
            }
            let mesh = generate_landblock(
                &land,
                region,
                i32::from(ux),
                i32::from(uy),
                1,
                Direction::InViewerBlock,
            );
            let has_building = |c: u16| owns.contains(&c);
            let grown = generate_scenery(
                &land,
                &mesh,
                region,
                i32::from(ux),
                i32::from(uy),
                &SceneryEnv {
                    scenes: &scenes,
                    has_building: &has_building,
                    shape: &shape,
                },
            );
            for s in grown {
                let p = s.frame.origin;
                let over = moved.iter().any(|(h, f)| {
                    let q = dereth_primitives::frame::globaltolocal(f, p);
                    q.x > h.lo.x + 0.5
                        && q.x < h.hi.x - 0.5
                        && q.y > h.lo.y + 0.5
                        && q.y < h.hi.y - 0.5
                });
                if !over {
                    continue;
                }
                let (i, j) = (s.vertex / 9, s.vertex % 9);
                let terrain = land.terrain_type(i, j);
                let value = *empty.entry(terrain).or_insert_with(|| {
                    let list = crate::models::scenery_of(self.base(), region, terrain);
                    list.iter()
                        .position(Vec::is_empty)
                        .or_else(|| (list.len() < 32).then_some(list.len()))
                        .and_then(|v| u16::try_from(v).ok())
                });
                let Some(value) = value else { continue };
                let (Ok(gx), Ok(gy)) = (
                    u32::try_from(lx * 8 + i64::try_from(i).unwrap_or(0)),
                    u32::try_from(ly * 8 + i64::try_from(j).unwrap_or(0)),
                ) else {
                    continue;
                };
                if !out.iter().any(|(x, y, _)| *x == gx && *y == gy) {
                    out.push((gx, gy, value));
                }
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------------------------
// A house players can buy.
// ---------------------------------------------------------------------------------------------

/// The landblocks whose houses' cells were given ids other than those the lockfile holds now for
/// their spawns: compiled again after the SQL is numbered, they name their spawns by the SQL's
/// ids.
#[must_use]
pub fn stale_houses(c: &Compiled) -> BTreeSet<u16> {
    c.houses
        .iter()
        .filter(|(_, m)| m.iter().any(|(s, g)| c.lock.get(Kind::Guid, s) != Some(*g)))
        .map(|(lb, _)| *lb)
        .collect()
}

/// The symbol of the house of building `member` of drop `name`'s set, whose house holding the
/// slumlord stands with building `root`: that one is `<name>-house`, each other one
/// `<name>-annex-<member>`. An annex's symbol sorts first, so a build numbers it first, from the
/// top down: the house the others answer to has the lowest id of them, and so stands first in
/// its landblock.
#[must_use]
pub fn house_name(name: &str, member: usize, root: usize) -> String {
    if member == root {
        format!("{name}-house")
    } else {
        format!("{name}-annex-{member}")
    }
}

/// The name of the basement of drop `name` of landblock `lb`, a copied dungeon: it names the
/// drop's landblock, since two landblocks may drop the same name.
#[must_use]
pub fn basement_name(name: &str, lb: u16) -> String {
    format!("{name}-basement-{lb:04x}")
}

/// The weenie class symbol of drop `name`'s house in landblock `lb`.
#[must_use]
pub fn house_class(name: &str, lb: u16) -> String {
    format!("{name}-house-{lb:04x}")
}

/// The SQL files of drop `name` of landblock `lb`'s house, in the pack whose `server/sql` is
/// `dir`: its house weenie's, its spawns' with its building, and those in its basement in
/// landblock `basement`.
fn house_paths(dir: &Path, lb: u16, name: &str, basement: Option<u16>) -> Vec<PathBuf> {
    let mut out = vec![
        dir.join("weenies")
            .join(format!("{lb:04X} {name}-house.sql")),
        dir.join("landblocks")
            .join(format!("{lb:04X} {name}-house.sql")),
    ];
    if let Some(b) = basement {
        out.push(
            dir.join("landblocks")
                .join(format!("{b:04X} {}.sql", basement_name(name, lb))),
        );
    }
    out
}

/// Where a drop's house is placed: the drop's landblock and bare name, its buildings' templates,
/// frames and hulls, the ground its drop levels, and its basement's landblock and template.
struct HouseAt<'a> {
    lb: u16,
    name: &'a str,
    templates: &'a [Template],
    frames: &'a [Frame],
    hulls: &'a [buildings::Hull],
    leveled: &'a [VertexSet],
    basement: Option<(u16, u16)>,
}

/// One spawn of a house as its file writes it: its symbol (bare), weenie and cell as written,
/// frame (landblock meters), whether another links it, and what it is.
struct HouseSpawn {
    name: String,
    weenie: String,
    cell: String,
    frame: Frame,
    child: bool,
    what: &'static str,
}

/// What a house's object is, as its symbol and its file's comment name it.
fn word(o: &buildings::HouseObject) -> &'static str {
    use buildings::House;
    if o.beyond {
        return "portal-spot";
    }
    match o.kind {
        House::HOUSE => "house",
        House::SLUMLORD => "slumlord",
        56 => "hook",
        House::STORAGE => "storage",
        House::BOOT_SPOT => "boot-spot",
        House::PORTAL => "portal",
        19 => "door",
        _ => "spawn",
    }
}

/// The bare symbol of each of `house`'s objects, for drop `name` of landblock `lb` whose house the
/// others answer to stands with building `root`: a building's house as [`house_name`] names it, everything in
/// the basement under its name, and each other object by what it is, numbered where there are
/// more than one.
fn spawn_names(name: &str, lb: u16, house: &buildings::House, root: usize) -> Vec<String> {
    let basement = basement_name(name, lb);
    let below = |o: &buildings::HouseObject| matches!(o.spot, buildings::Spot::Basement(_));
    let mut counts: BTreeMap<(bool, &str), usize> = BTreeMap::new();
    for o in &house.objects {
        if o.kind != buildings::House::HOUSE || below(o) {
            *counts.entry((below(o), word(o))).or_default() += 1;
        }
    }
    let mut seen: BTreeMap<(bool, &str), usize> = BTreeMap::new();
    house
        .objects
        .iter()
        .map(|o| {
            if o.kind == buildings::House::HOUSE && !below(o) {
                return house_name(name, o.member, root);
            }
            let key = (below(o), word(o));
            let base = if key.0 { basement.as_str() } else { name };
            let k = seen.entry(key).or_default();
            let out = if counts[&key] == 1 {
                format!("{base}-{}", key.1)
            } else {
                format!("{base}-{}-{k}", key.1)
            };
            *k += 1;
            out
        })
        .collect()
}

/// A number of meters as an instance's file writes it.
fn at_text(f: &Frame) -> String {
    let (o, q) = (f.origin, f.rotation);
    format!(
        "{}, {}, {}, {}, {}, {}, {}",
        crate::edit::meters(o.x),
        crate::edit::meters(o.y),
        crate::edit::meters(o.z),
        six(q.w),
        six(q.x),
        six(q.y),
        six(q.z)
    )
}

/// The text of the SQL file placing `spawns` of landblock `lb` (with the links `links` between
/// them, by their symbols, and the house portals' destinations `portals` of house class `class`),
/// stamped `today`, under the comment `about`.
fn spawns_sql(
    lb: u16,
    about: &str,
    spawns: &[HouseSpawn],
    links: &[(String, String)],
    portals: Option<(&str, &[(String, Frame)])>,
    today: &str,
) -> String {
    let guid = |n: &str| format!("{{{{guid:{n}@{lb:04X}}}}}");
    let mut out = format!("/* {about} */\n");
    for s in spawns {
        let _ = writeln!(
            out,
            "DELETE FROM `landblock_instance` WHERE `guid` = {};",
            guid(&s.name)
        );
    }
    let parents: BTreeSet<&String> = links.iter().map(|(p, _)| p).collect();
    for p in &parents {
        let _ = writeln!(
            out,
            "DELETE FROM `landblock_instance_link` WHERE `parent_GUID` = {};",
            guid(p)
        );
    }
    if let Some((class, _)) = portals {
        let _ = writeln!(
            out,
            "DELETE FROM `house_portal` WHERE `house_Id` = {{{{wcid:{class}}}}};"
        );
    }
    out.push_str(
        "\nINSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, \
         `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, \
         `is_Link_Child`, `last_Modified`)\n",
    );
    for (k, s) in spawns.iter().enumerate() {
        let _ = writeln!(
            out,
            "{} ({}, {}, {}, {}, {}, '{today}') /* {} */",
            if k == 0 { "VALUES" } else { "     ," },
            guid(&s.name),
            s.weenie,
            s.cell,
            at_text(&s.frame),
            if s.child { "True" } else { "False" },
            s.what
        );
    }
    out.push_str(";\n");
    if !links.is_empty() {
        out.push_str(
            "\nINSERT INTO `landblock_instance_link` (`parent_GUID`, `child_GUID`, \
             `last_Modified`)\n",
        );
        for (k, (p, c)) in links.iter().enumerate() {
            let _ = writeln!(
                out,
                "{} ({}, {}, '{today}')",
                if k == 0 { "VALUES" } else { "     ," },
                guid(p),
                guid(c)
            );
        }
        out.push_str(";\n");
    }
    if let Some((class, rows)) = portals.filter(|(_, r)| !r.is_empty()) {
        out.push_str(
            "\nINSERT INTO `house_portal` (`house_Id`, `obj_Cell_Id`, `origin_X`, `origin_Y`, \
             `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `last_Modified`)\n",
        );
        for (k, (cell, f)) in rows.iter().enumerate() {
            let _ = writeln!(
                out,
                "{} ({{{{wcid:{class}}}}}, {cell}, {}, '{today}')",
                if k == 0 { "VALUES" } else { "     ," },
                at_text(f)
            );
        }
        out.push_str(";\n");
    }
    out
}

/// The text of the SQL file of a house weenie, class `class` (bare), a copy of the house weenie
/// whose property rows are `rows` with its own house id: a `kind` house, as `about` says.
fn house_weenie_sql(
    class: &str,
    kind: buildings::Kind,
    rows: &[crate::sql_ids::HouseRow],
    about: &str,
    today: &str,
) -> String {
    let id = format!("{{{{wcid:{class}}}}}");
    let mut out = format!(
        "/* {about} */\n\
         DELETE FROM `weenie` WHERE `class_Id` = {id};\n\
         \n\
         INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)\n\
         VALUES ({id}, 'house{}{id}', {}, '{today}') /* House */;\n",
        kind.name().to_lowercase(),
        crate::sql_ids::HOUSE_TYPE
    );
    // Its own house id, which names it: the class's.
    let mut rows: Vec<crate::sql_ids::HouseRow> = rows
        .iter()
        .filter(|r| !(r.table == "d_i_d" && r.kind == HOUSE_ID))
        .cloned()
        .collect();
    rows.push(crate::sql_ids::HouseRow {
        table: "d_i_d".into(),
        kind: HOUSE_ID,
        value: id.clone(),
    });
    for table in ["int", "int64", "bool", "float", "string", "d_i_d", "i_i_d"] {
        let mut these: Vec<&crate::sql_ids::HouseRow> =
            rows.iter().filter(|r| r.table == table).collect();
        if these.is_empty() {
            continue;
        }
        these.sort_by_key(|r| r.kind);
        let _ = writeln!(
            out,
            "\nINSERT INTO `weenie_properties_{table}` (`object_Id`, `type`, `value`)"
        );
        for (k, r) in these.iter().enumerate() {
            let _ = writeln!(
                out,
                "{} ({id}, {}, {})",
                if k == 0 { "VALUES" } else { "     ," },
                r.kind,
                r.value
            );
        }
        out.push_str(";\n");
    }
    out
}

/// A house weenie's data id that is its house id.
const HOUSE_ID: u32 = 42;

impl Session {
    /// Why drop `w` of landblock `lb` cannot be the house `house` players can buy: its house or
    /// its slumlord would stand outside the landblock, or it is a mansion where one players can
    /// buy stands already (the server makes the mansions of a landblock one).
    fn house_refusal(&self, w: &Worked, house: &buildings::House, lb: u16) -> Option<String> {
        use buildings::{House, Spot};
        for o in &house.objects {
            if !matches!(o.kind, House::HOUSE | House::SLUMLORD)
                || matches!(o.spot, Spot::Basement(_))
            {
                continue;
            }
            let f = combine(w.frames.get(o.member)?, &o.frame);
            if !inside(f.origin.x, f.origin.y) {
                return Some(format!(
                    "Its {} would stand outside the landblock.",
                    word(o)
                ));
            }
        }
        (w.kind == buildings::Kind::Mansion && self.mansion_held(lb, &[])).then(|| {
            "The landblock has a mansion players can buy already, and holds only one.".into()
        })
    }

    /// Whether landblock `lb` has a mansion players can buy, but its buildings at `except`: one
    /// of the world's own, or a drop as a house.
    fn mansion_held(&self, lb: u16, except: &[usize]) -> bool {
        let (Some(info), Some(catalog)) = (self.statics(lb), self.buildings()) else {
            return false;
        };
        let dropped = self.compiled().dropped.get(&lb);
        info.buildings.iter().enumerate().any(|(i, b)| {
            if except.contains(&i)
                || catalog.get(b.id).map(|e| e.kind) != Some(buildings::Kind::Mansion)
            {
                return false;
            }
            match dropped.and_then(|l| l.iter().find(|(_, j)| *j == i)) {
                None => true,
                Some((name, _)) => self
                    .packs()
                    .iter()
                    .flat_map(|p| &p.edits)
                    .filter(|e| e.landblock == lb)
                    .flat_map(|e| &e.drops)
                    .any(|d| d.item.name == *name && !d.item.house.is_empty()),
            }
        })
    }

    /// Where drop `bare` of landblock `lb`'s basement is copied: the landblock of the target
    /// pack's dungeon copy of its name, and the retail dungeon it copies.
    fn basement_of(&self, lb: u16, bare: &str) -> Option<(u16, u16)> {
        let pack = &self.packs()[self.target()];
        let name = format!("{}:{}", pack.name, basement_name(bare, lb));
        pack.edits.iter().find_map(|e| {
            e.dungeon_copies
                .iter()
                .find(|d| d.item.name == name)
                .map(|d| (e.landblock, d.item.template))
        })
    }

    /// The catalog's house of dropped building `row`: that of the layout of its lead's model
    /// whose example its lead's template is.
    fn house_of_row(&self, row: &AddBuilding) -> Option<(buildings::Kind, buildings::House)> {
        let entry = self.buildings()?.get(*row.models.first()?)?;
        let layout = entry
            .layouts
            .iter()
            .find(|l| Some(&l.example) == row.templates.first())?;
        Some((entry.kind, layout.house.clone()?))
    }

    /// The nearest landblock to `from`, ring by ring outward, with room for a new dungeon: open
    /// sea floor with no statics and no cells, where no file of the target pack writes.
    fn free_dungeon_near(&self, from: u16) -> Option<u16> {
        let (hx, hy) = (i32::from(from >> 8), i32::from(from & 0xFF));
        let written: BTreeSet<u16> = self.packs()[self.target()]
            .edits
            .iter()
            .map(|e| e.landblock)
            .collect();
        for r in 1..=64_i32 {
            for dx in -r..=r {
                for dy in -r..=r {
                    if dx.abs() != r && dy.abs() != r {
                        continue;
                    }
                    let (x, y) = (hx + dx, hy + dy);
                    if !(0..0xFF).contains(&x) || !(0..0xFF).contains(&y) {
                        continue;
                    }
                    let Ok(lb) = u16::try_from((x << 8) | y) else {
                        continue;
                    };
                    if written.contains(&lb) {
                        continue;
                    }
                    let free = crate::room::room_for_dungeon(
                        lb,
                        self.terrain(lb).as_ref(),
                        self.statics(lb).as_ref(),
                        self.cells(lb).len(),
                    )
                    .is_ok();
                    if free {
                        return Some(lb);
                    }
                }
            }
        }
        None
    }

    /// The ground's height at `(x, y)` of landblock `lb` once the vertices `leveled` are set.
    fn ground_after(&self, lb: u16, leveled: &[VertexSet], x: f32, y: f32) -> f32 {
        let (Some(mut land), Some(region)) = (self.terrain(lb), self.region()) else {
            return self.ground(lb, x, y).unwrap_or(0.0);
        };
        for v in leveled.iter().filter(|v| v.field == Field::Height) {
            for (owner, i, j) in compile::copies(v.gx, v.gy) {
                if owner == lb {
                    land.height[i * 9 + j] = u8::try_from(v.value).unwrap_or(u8::MAX);
                }
            }
        }
        compile::ground(&land, region, lb, x, y)
    }

    /// Where a house's boot spot stands: outside the land cells its buildings' origins stand in
    /// (which belong to its houses) and off their shells, two meters on from where the way from
    /// the house the others answer to toward its slumlord (or, where that leaves the landblock,
    /// a quarter turn on) leaves them, on the ground there.
    fn boot_spot(&self, at: &HouseAt<'_>, from: Vec3, toward: Option<Vec3>) -> Option<Vec3> {
        let held: BTreeSet<(i32, i32)> = at
            .frames
            .iter()
            .map(|f| buildings::land_cell(f.origin.x, f.origin.y))
            .collect();
        let over = |x: f32, y: f32| {
            at.hulls.iter().zip(at.frames).any(|(h, f)| {
                let q = dereth_primitives::frame::globaltolocal(f, Vec3::new(x, y, f.origin.z));
                q.x >= h.lo.x && q.x <= h.hi.x && q.y >= h.lo.y && q.y <= h.hi.y
            })
        };
        let free = |x: f32, y: f32| {
            (0.5..191.5).contains(&x)
                && (0.5..191.5).contains(&y)
                && !held.contains(&buildings::land_cell(x, y))
                && !over(x, y)
        };
        let (mut dx, mut dy) = toward.map_or((0.0, -1.0), |t| (t.x - from.x, t.y - from.y));
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-3 {
            (dx, dy) = (0.0, -1.0);
        } else {
            (dx, dy) = (dx / len, dy / len);
        }
        for _ in 0..4 {
            let mut t = 0.0;
            while t < NEAR_ENOUGH {
                let (x, y) = (from.x + dx * t, from.y + dy * t);
                if !(0.5..191.5).contains(&x) || !(0.5..191.5).contains(&y) {
                    break;
                }
                if free(x, y) {
                    let (x2, y2) = (x + dx * 2.0, y + dy * 2.0);
                    let (x, y) = if free(x2, y2) { (x2, y2) } else { (x, y) };
                    return Some(Vec3::new(x, y, self.ground_after(at.lb, at.leveled, x, y)));
                }
                t += 0.5;
            }
            (dx, dy) = (dy, -dx);
        }
        None
    }

    /// The files of house `house` placed as `at` says, a `kind`: its weenie's, its spawns' with
    /// its building, and its basement's spawns'; the basement's row and its landblock; and what
    /// it warns of (a spawn that would stand outside the landblock, which is not placed).
    ///
    /// # Errors
    /// Its house, slumlord or house portal cannot be placed.
    #[allow(clippy::too_many_lines, clippy::type_complexity)]
    fn house_files(
        &self,
        at: &HouseAt<'_>,
        house: &buildings::House,
        kind: buildings::Kind,
    ) -> Result<
        (
            Vec<(PathBuf, String)>,
            Option<(u16, crate::source::AddDungeon)>,
            Vec<String>,
        ),
        String,
    > {
        use buildings::{House, Spot};
        let (lb, name) = (at.lb, at.name);
        let pack = self.packs()[self.target()].name.clone();
        let root_at = house.root().ok_or("its house holds no slumlord")?;
        let root = house.objects[root_at].member;
        let names = spawn_names(name, lb, house, root);
        let first = u32::from(lb) << 16;
        let mut warnings = Vec::new();
        // The rooms as their tokens name them.
        let mut offsets = Vec::new();
        let mut k = 0;
        for t in at.templates {
            offsets.push(k);
            k += t.rooms.len();
        }
        let room_token = |n: u16| {
            at.templates.iter().enumerate().find_map(|(m, t)| {
                let pos = t.rooms.keys().position(|r| *r == n)?;
                Some(format!(
                    "{{{{cell:{}@{lb:04X}}}}}",
                    room_name(name, offsets[m] + pos)
                ))
            })
        };
        // The basement's cells, by their template's numbers.
        let basement_cells: Vec<u16> = match at.basement {
            Some((_, template)) => {
                let c = self.compiled();
                let read = |id: u32| compile::read_under(c, self.base(), ModernDat::Cell, id);
                buildings::read_cells(&read, template, self.base().era)
                    .into_keys()
                    .collect()
            }
            None => Vec::new(),
        };
        let below = basement_name(name, lb);
        let basement_token = |n: u16| {
            let (b, _) = at.basement?;
            let pos = basement_cells.iter().position(|c| *c == n)?;
            Some(format!("{{{{cell:{}@{b:04X}}}}}", room_name(&below, pos)))
        };
        // Each object's cell and frame, where it can stand.
        let class = house_class(name, lb);
        let mut placed: Vec<Option<(String, Frame)>> = Vec::new();
        for o in &house.objects {
            let spot = match o.spot {
                Spot::Room(n) => {
                    room_token(n).map(|c| (c, combine(&at.frames[o.member], &o.frame)))
                }
                Spot::Outdoors => {
                    let f = combine(&at.frames[o.member], &o.frame);
                    inside(f.origin.x, f.origin.y).then(|| {
                        (
                            format!(
                                "0x{:08X}",
                                first | crate::edit::outdoor_cell(f.origin.x, f.origin.y)
                            ),
                            f,
                        )
                    })
                }
                Spot::Basement(n) => basement_token(n).map(|c| (c, o.frame)),
            };
            placed.push(spot);
        }
        // The boot spot: off the house's land, toward its slumlord.
        let slumlord = house
            .objects
            .iter()
            .zip(&placed)
            .find(|(o, _)| o.kind == House::SLUMLORD)
            .and_then(|(_, p)| p.as_ref().map(|(_, f)| f.origin));
        for (o, p) in house.objects.iter().zip(placed.iter_mut()) {
            if o.kind != House::BOOT_SPOT || matches!(o.spot, Spot::Basement(_)) {
                continue;
            }
            let rotation = combine(&at.frames[o.member], &o.frame).rotation;
            *p = self
                .boot_spot(at, at.frames[root].origin, slumlord)
                .map(|v| {
                    (
                        format!("0x{:08X}", first | crate::edit::outdoor_cell(v.x, v.y)),
                        Frame::new(v, rotation),
                    )
                });
        }
        // A portal's link spot stands where the house portal on the other side stands.
        let portal_at = |basement: bool| {
            house
                .objects
                .iter()
                .zip(&placed)
                .find(|(o, _)| {
                    o.kind == House::PORTAL && matches!(o.spot, Spot::Basement(_)) == basement
                })
                .and_then(|(_, p)| p.as_ref().map(|(_, f)| *f))
        };
        let (outside, inside_basement) = (portal_at(false), portal_at(true));
        for (i, o) in house.objects.iter().enumerate() {
            if !o.beyond {
                continue;
            }
            let there = if matches!(o.spot, Spot::Basement(_)) {
                outside
            } else {
                inside_basement
            };
            placed[i] = match (placed[i].take(), there) {
                (Some((cell, _)), Some(f)) => Some((cell, f)),
                _ => None,
            };
        }
        // What must stand for the house to work; what else cannot is left out.
        for (i, o) in house.objects.iter().enumerate() {
            if placed[i].is_some() {
                continue;
            }
            let what = word(o);
            if matches!(o.kind, House::HOUSE | House::SLUMLORD | House::BOOT_SPOT)
                || (o.kind == House::PORTAL && at.basement.is_some())
            {
                return Err(format!(
                    "{name}: its {what} would stand outside the landblock"
                ));
            }
            warnings.push(format!(
                "{name}: its {what} {} would stand outside the landblock, and is not placed",
                names[i]
            ));
        }
        // The spawns and links of each side.
        let mut spawns: [Vec<HouseSpawn>; 2] = [Vec::new(), Vec::new()];
        let mut links: [Vec<(String, String)>; 2] = [Vec::new(), Vec::new()];
        for (i, o) in house.objects.iter().enumerate() {
            let Some((cell, frame)) = &placed[i] else {
                continue;
            };
            let side = usize::from(matches!(o.spot, Spot::Basement(_)));
            spawns[side].push(HouseSpawn {
                name: names[i].clone(),
                weenie: if o.kind == House::HOUSE {
                    format!("{{{{wcid:{class}}}}}")
                } else {
                    o.weenie.to_string()
                },
                cell: cell.clone(),
                frame: *frame,
                child: o.link_child,
                what: word(o),
            });
        }
        for (p, c) in &house.links {
            if placed[*p].is_some() && placed[*c].is_some() {
                let side = usize::from(matches!(house.objects[*p].spot, Spot::Basement(_)));
                links[side].push((names[*p].clone(), names[*c].clone()));
            }
        }
        // The house portals' destinations: each portal's own place.
        let portals: Vec<(String, Frame)> = house
            .objects
            .iter()
            .zip(&placed)
            .filter(|(o, _)| o.kind == House::PORTAL)
            .filter_map(|(_, p)| p.clone())
            .collect();
        let dir = self.packs()[self.target()].dir.join("server").join("sql");
        let [weenie_file, spawns_file, ..] = &house_paths(&dir, lb, name, None)[..] else {
            unreachable!("two paths at least");
        };
        let today = self.stamp();
        let mut files = vec![
            (
                weenie_file.clone(),
                house_weenie_sql(
                    &class,
                    kind,
                    &house.rows,
                    &format!(
                        "The house of {name}, landblock {lb:04X}: a {} players can buy, a copy of \
                         its template's house with its own house id.",
                        kind.name().to_lowercase()
                    ),
                    today,
                ),
            ),
            (
                spawns_file.clone(),
                spawns_sql(
                    lb,
                    &format!(
                        "{name}'s house: its house, slumlord, storage, hooks, boot spot and house \
                         portals, placed with its building in landblock {lb:04X}, and written \
                         again whole when the building moves."
                    ),
                    &spawns[0],
                    &links[0],
                    Some((class.as_str(), &portals)),
                    today,
                ),
            ),
        ];
        let mut row = None;
        if let Some((b, template)) = at.basement {
            let house_spawn = house
                .objects
                .iter()
                .position(|o| o.kind == House::HOUSE && matches!(o.spot, Spot::Basement(_)))
                .map(|i| format!("{pack}:{}", names[i]));
            row = Some((
                b,
                crate::source::AddDungeon {
                    name: format!("{pack}:{below}"),
                    template,
                    house: house_spawn,
                },
            ));
            files.push((
                dir.join("landblocks").join(format!("{b:04X} {below}.sql")),
                spawns_sql(
                    b,
                    &format!(
                        "{name}'s basement, a copy of the dungeon of landblock {template:04X}: \
                         its house, storage, hooks and house portal, and its doors."
                    ),
                    &spawns[1],
                    &links[1],
                    None,
                    today,
                ),
            ));
        }
        Ok((files, row, warnings))
    }
}

/// What the checks say of the buildings the packs dropped: the walk into each (faults), and
/// for each one whose shell reaches further into the next land cell than any retail copy of its
/// model, a warning (when `catalog` is known).
#[must_use]
pub fn checks(s: &Session, catalog: Option<&Catalog>) -> (Vec<String>, Vec<String>) {
    let (mut problems, mut warnings) = (Vec::new(), Vec::new());
    let mut models = crate::pick::Models::default();
    for (lb, list) in &s.compiled().dropped {
        let Some(info) = s.statics(*lb) else { continue };
        let cells = s.cells(*lb);
        for (name, index) in list {
            let Some(b) = info.buildings.get(*index) else {
                continue;
            };
            for f in walk_in(&cells, b, &|id| s.environment(id)) {
                problems.push(format!(
                    "cell 0x{:08X}: building {name}: {}",
                    (u32::from(*lb) << 16) | u32::from(f.cell),
                    f.text
                ));
            }
            let Some(e) = catalog.and_then(|c| c.get(b.id)) else {
                continue;
            };
            let r = buildings::reach(&buildings::hull(&mut models, s.base(), b.id), &b.frame);
            if r > e.reach + 0.01 {
                warnings.push(format!(
                    "0x{lb:04X}FFFE: building {index}: {name} reaches {r:.1} m into the next land \
                     cell; no retail copy of {:#010X} reaches more than {:.1} m",
                    b.id.raw(),
                    e.reach
                ));
            }
        }
    }
    (problems, warnings)
}
