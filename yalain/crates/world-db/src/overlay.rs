//! The client's records a world's rows make, from the install's own: what a world's overlay holds,
//! built into a folder beside its tables (the `fs` feature's [`write()`]). The install is never
//! written.
//!
//! A `[[clearing]]` row keeps a spot free of what the land grows. Nothing places scenery: the client
//! works it out from each land vertex's scene field, so a clearing gives the vertices in its shape
//! a field under which their terrain grows nothing. A vertex's scene can spread past the vertex,
//! so the shape is widened by [`SPREAD`], and a vertex on a block's edge is its neighbor's too, so
//! every block the shape reaches is changed. What a vertex grows includes its ambient sound.
//!
//! A shop (`[[store]]`) is a copy of its store template, its saved state, under an id of the
//! world's own ([`crate::compile::Shop`]): the same stock, prices, sorter and portrait, with the
//! shop's title in place of the template's name, and its line under the title when it gives one.
//! The title is literal text, the same in every language. The client names a shop window from the
//! template its store view names, so a keeper of the shop has the copy named and sells the
//! template's own sales.
//!
//! A world with no clearing and no shop has no overlay: the folder is left empty and everything
//! reads the install's own records.
//!
//! The overlay lies over the portal, the first region's cell file and the English file alike: what
//! a world sets in each goes into that file's container, and each file is numbered apart. Its
//! records are made from the install it is built over, so it serves only over those files: a world
//! taken to another install has its overlay built again there.
//!
//! A build is numbered as the revision after the folder's last, of each file it changes, so a
//! client holding the last one is sent what changed alone. A build that changes nothing leaves the
//! folder as it is, and one that does replaces it only once it is written whole, so a server
//! reading the folder as it runs keeps what it has until it opens it again.

use std::collections::BTreeMap;

use auberean_assets::landblock::Landblock;
use auberean_assets::script::ScriptState;
use auberean_assets::{decode, encode, Context};
use auberean_dat::DatFile;
use auberean_script::graph::{Body, Graph};
use auberean_script::library::Library;
use auberean_script::native_object::{NativeObject, TextInfo};
use auberean_world::records::{RegionSet, WorldRecords};
use auberean_world::scenery::barren_field;
use dereth_dat::ModernDat;
use dereth_primitives::num::math;

use crate::compile::Shop;
use crate::merge::Merged;
use crate::rows::{Clearing, Row};

/// The overlay's folder: Yalain reads this one in a world's folder.
pub const FOLDER: &str = auberean_dat::overlay::WORLD_FOLDER;
/// How far past a clearing's shape a vertex is still changed, in meters: what a vertex's scene puts
/// beside it reaches about this far.
pub const SPREAD: f32 = 8.0;
/// A landblock's side, in meters.
const BLOCK: f32 = 160.0;
/// The meters between two land vertices.
const VERTEX: f32 = 10.0;
/// A landblock's vertices along a side.
const SIDE: usize = 17;

/// What a world's overlay holds.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Built {
    /// The landblocks it changes.
    pub blocks: usize,
    /// The vertices in them that it changes.
    pub vertices: usize,
    /// Its shops' copies of their store templates.
    pub shops: usize,
}

/// A clearing's shape in the world's meters, widened by [`SPREAD`].
#[derive(Clone, Copy, Debug)]
struct Shape {
    x: f32,
    y: f32,
    /// The turn's cosine and sine.
    turn: (f32, f32),
    radius: Option<f32>,
    half: (f32, f32),
}

impl Shape {
    fn of(c: &Clearing, landblock: u16) -> Self {
        let turn = c.heading.unwrap_or(0.0).to_radians();
        Self {
            x: f32::from(landblock >> 8) * BLOCK + c.at.x,
            y: f32::from(landblock & 0xFF) * BLOCK + c.at.y,
            turn: (math::cosf(turn), math::sinf(turn)),
            radius: c.radius.map(|r| r + SPREAD),
            half: c
                .size
                .map_or((0.0, 0.0), |[x, y]| (x / 2.0 + SPREAD, y / 2.0 + SPREAD)),
        }
    }

    /// How far it reaches from its center along any axis.
    fn reach(&self) -> f32 {
        self.radius
            .unwrap_or_else(|| math::hypotf(self.half.0, self.half.1))
    }

    /// Whether the point is inside.
    fn holds(&self, x: f32, y: f32) -> bool {
        let (dx, dy) = (x - self.x, y - self.y);
        if let Some(r) = self.radius {
            return math::hypotf(dx, dy) <= r;
        }
        let (c, s) = self.turn;
        // Headings turn clockwise from north, so a shape's own axes are the world's turned so.
        let (u, v) = (dx * c - dy * s, dx * s + dy * c);
        u.abs() <= self.half.0 && v.abs() <= self.half.1
    }

    /// The landblocks it reaches, as `(x, y)` within the world.
    fn blocks(&self) -> impl Iterator<Item = (u32, u32)> {
        let range = |c: f32| {
            let r = self.reach();
            let low = (((c - r) / BLOCK).floor()).clamp(0.0, 255.0);
            let high = (((c + r) / BLOCK).floor()).clamp(0.0, 255.0);
            to_block(low)..=to_block(high)
        };
        let ys = range(self.y);
        range(self.x).flat_map(move |x| ys.clone().map(move |y| (x, y)))
    }
}

/// A block coordinate already clamped to the world.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn to_block(c: f32) -> u32 {
    c as u32
}

/// The edge of a clearing as written, not widened: `steps` points round it, on the world's grid about
/// `center`, for showing where it lies.
#[must_use]
pub fn outline(c: &Clearing, center: [f32; 2], steps: usize) -> Vec<[f32; 2]> {
    #[allow(clippy::cast_precision_loss)]
    let part = |k: usize| k as f32 / steps.max(1) as f32;
    if let Some(r) = c.radius {
        return (0..=steps)
            .map(|k| {
                let a = part(k) * std::f32::consts::TAU;
                [center[0] + r * math::cosf(a), center[1] + r * math::sinf(a)]
            })
            .collect();
    }
    let [x, y] = c.size.unwrap_or([0.0, 0.0]);
    let (hx, hy) = (x / 2.0, y / 2.0);
    let turn = c.heading.unwrap_or(0.0).to_radians();
    let (cos, sin) = (math::cosf(turn), math::sinf(turn));
    // The way round its own axes, and the world's point of each: the shape's axes turned back.
    let corners = [(-hx, -hy), (hx, -hy), (hx, hy), (-hx, hy), (-hx, -hy)];
    let each = (steps / 4).max(1);
    let mut out = Vec::new();
    for w in corners.windows(2) {
        for k in 0..each {
            #[allow(clippy::cast_precision_loss)]
            let t = k as f32 / each as f32;
            let (u, v) = (
                w[0].0 + (w[1].0 - w[0].0) * t,
                w[0].1 + (w[1].1 - w[0].1) * t,
            );
            out.push([center[0] + u * cos + v * sin, center[1] - u * sin + v * cos]);
        }
    }
    out.extend(out.first().copied());
    out
}

/// The world's clearings, with the landblock each is written in.
fn clearings(merged: &Merged) -> Vec<(&Clearing, u16)> {
    merged
        .rows
        .iter()
        .filter_map(|l| match &l.row {
            Row::Clearing(c) => Some((c, l.landblock?)),
            _ => None,
        })
        .collect()
}

/// A landblock's vertices that lie in any of `shapes` given the field their terrain grows nothing
/// under. How many changed.
///
/// # Errors
/// A terrain kind with no such field.
fn clear_vertices(
    land: &mut Landblock,
    (bx, by): (u32, u32),
    shapes: &[Shape],
    region: &RegionSet,
    records: &WorldRecords,
) -> Result<usize, String> {
    let Some(scenes) = &region.scenes else {
        return Err("the region names no scenery".into());
    };
    let word = region.land.word;
    let mut changed = 0;
    #[allow(clippy::cast_precision_loss)]
    let place = |b: u32, v: usize| b as f32 * BLOCK + v as f32 * VERTEX;
    for i in 0..SIDE {
        for j in 0..SIDE {
            let (x, y) = (place(bx, i), place(by, j));
            if !shapes.iter().any(|s| s.holds(x, y)) {
                continue;
            }
            let at = i * SIDE + j;
            let kind = word.terrain.of(land.vertices[at]);
            let Some(field) = barren_field(&region.terrain, scenes, records, kind) else {
                return Err(format!(
                    "terrain kind {kind} of landblock {bx:02X}{by:02X} grows something everywhere"
                ));
            };
            let new = word.scene.with(land.vertices[at], field);
            if new != land.vertices[at] {
                land.vertices[at] = new;
                changed += 1;
            }
        }
    }
    Ok(changed)
}

/// The landblock records the world's clearings change, by id, and what they come to.
///
/// # Errors
/// As [`cleared_rows`].
pub fn cleared_blocks(
    merged: &Merged,
    cell: &DatFile,
    ctx: &Context,
    region: &RegionSet,
    records: &WorldRecords,
) -> Result<(BTreeMap<u32, Vec<u8>>, Built), String> {
    cleared_rows(&clearings(merged), cell, ctx, region, records)
}

/// The landblock records these clearings change, each with the landblock it is written in, by id;
/// and what they come to. `cell` is the file as the install holds it.
///
/// # Errors
/// A clearing in a landblock the install does not hold, and a record that does not read.
pub fn cleared_rows(
    rows: &[(&Clearing, u16)],
    cell: &DatFile,
    ctx: &Context,
    region: &RegionSet,
    records: &WorldRecords,
) -> Result<(BTreeMap<u32, Vec<u8>>, Built), String> {
    let mut by_block: BTreeMap<(u32, u32), Vec<Shape>> = BTreeMap::new();
    for &(c, landblock) in rows {
        let id = landblock_record(u32::from(landblock >> 8), u32::from(landblock & 0xFF));
        if cell.entry(id).is_none() {
            return Err(format!(
                "clearing {}: the install has no landblock {landblock:04X}",
                c.key
            ));
        }
        let shape = Shape::of(c, landblock);
        for block in shape.blocks() {
            by_block.entry(block).or_default().push(shape);
        }
    }
    let mut out = BTreeMap::new();
    let mut built = Built::default();
    for ((bx, by), shapes) in by_block {
        let id = landblock_record(bx, by);
        if cell.entry(id).is_none() {
            continue;
        }
        let bytes = cell.read(id).map_err(|e| format!("{id:#x}: {e}"))?;
        let mut land: Landblock = decode(id, &bytes, ctx).map_err(|e| format!("{id:#x}: {e:?}"))?;
        let changed = clear_vertices(&mut land, (bx, by), &shapes, region, records)?;
        if changed > 0 {
            out.insert(id, encode(id, &land));
            built.blocks += 1;
            built.vertices += changed;
        }
    }
    Ok((out, built))
}

/// A landblock's record.
const fn landblock_record(x: u32, y: u32) -> u32 {
    (x << 24) | (y << 16) | 0xFFFF
}

/// What a world's overlay holds of each of the install's files it changes, by the role the file
/// plays: the records it sets there, by id. A file left out, or given none, reads as the install's
/// own.
pub type Records = BTreeMap<ModernDat, BTreeMap<u32, Vec<u8>>>;

/// A record reader: a record's bytes by id, as the install holds it.
pub type Read<'a> = &'a dyn Fn(u32) -> Option<Vec<u8>>;

/// The portal records the world's shops make, by id: each shop's copy of its store template.
///
/// # Errors
/// A shop whose template does not read, whose copy's id the install holds, or whose template has
/// no title of the form a store's is.
pub fn shop_copies(
    shops: &[Shop],
    read: Read<'_>,
    ctx: &Context,
    lib: &Library,
) -> Result<BTreeMap<u32, Vec<u8>>, String> {
    let mut out = BTreeMap::new();
    for shop in shops {
        if read(shop.id).is_some() {
            return Err(format!(
                "store {}: the data files hold {:#010X} already; give the shop another id in world.lock",
                shop.key, shop.id
            ));
        }
        let bytes = read(shop.from)
            .ok_or_else(|| format!("store {}: no store template {:08X}", shop.key, shop.from))?;
        out.insert(
            shop.id,
            shop_copy(shop, &bytes, ctx, lib).map_err(|e| format!("store {}: {e}", shop.key))?,
        );
    }
    Ok(out)
}

/// One shop's copy of its store template, whose record is `bytes`.
///
/// # Errors
/// A record that is no saved state of the library's, or a template without a title.
pub fn shop_copy(
    shop: &Shop,
    bytes: &[u8],
    ctx: &Context,
    lib: &Library,
) -> Result<Vec<u8>, String> {
    let state: ScriptState = auberean_assets::decode(shop.from, bytes, ctx)
        .map_err(|e| format!("{:08X}: {e}", shop.from))?;
    let mut graph =
        Graph::decode(&state.graph, lib).map_err(|e| format!("{:08X}: {e}", shop.from))?;
    set_text(&mut graph, lib, "m_siName", &shop.name_text)?;
    if let Some(d) = &shop.description_text {
        set_text(&mut graph, lib, "m_siDesc", d)?;
    }
    let packed = graph.encode()?;
    Ok(auberean_assets::encode(
        shop.id,
        &ScriptState {
            graph: packed,
            package: state.package,
        },
    ))
}

/// Make the text the graph's root holds in `field` the literal `text`.
fn set_text(graph: &mut Graph, lib: &Library, field: &str, text: &str) -> Result<(), String> {
    let root = graph.root().ok_or("an empty saved state")?;
    let slot = {
        let Some(Body::Object(o)) = graph
            .ids
            .iter()
            .position(|id| *id == root)
            .map(|i| &graph.entries[i].body)
        else {
            return Err("a saved state whose root is no object".into());
        };
        let package = lib.package_name(o.package_id());
        let member = lib
            .frame(&package)
            .and_then(|f| f.members.iter().find(|m| m.name == field))
            .ok_or_else(|| format!("a {package} has no {field}"))?;
        usize::try_from(member.slot)
            .ok()
            .and_then(|s| o.slots.get(s).copied())
            .ok_or_else(|| format!("{field} is not in the saved state"))?
    };
    let at = graph
        .ids
        .iter()
        .position(|id| *id == slot)
        .ok_or_else(|| format!("{field} names nothing in the saved state"))?;
    match &mut graph.entries[at].body {
        Body::Object(o) => match &mut o.native {
            Some(NativeObject::TextInfo(t)) => {
                *t = TextInfo {
                    has_literal: 1,
                    literal: Some(text.encode_utf16().collect()),
                    ..TextInfo::default()
                };
                Ok(())
            }
            _ => Err(format!("{field} is not a text")),
        },
        _ => Err(format!("{field} is not a text")),
    }
}

/// Writing the overlay into a folder, against an install on disk.
#[cfg(feature = "fs")]
mod folder {
    use std::cell::RefCell;
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::{Path, PathBuf};

    use auberean_assets::Context;
    use auberean_dat::locate::{install_dats, role_file, DatName};
    use auberean_dat::DatFile;
    use auberean_script::library::{Library, RECORD};
    use auberean_world::records::WorldRecords;
    use dereth_dat::overlay::{OverlayDir, OverlayError};
    use dereth_dat::revise::{revise_folder, ReviseError};
    use dereth_dat::{decompose, ModernDat};
    use dereth_primitives::DataId;

    use super::{cleared_rows, clearings, shop_copies, Built, Records, FOLDER};
    use crate::compile::Shop;
    use crate::merge::Merged;
    use crate::rows::{Clearing, Row};

    /// Write the world's overlay into `folder` (a world's own is its [`FOLDER`]), from its rows
    /// merged and its key, over the install in `dats`: the revision after what the folder holds,
    /// when it reads otherwise. Answers what it holds; a world that changes no record and never did
    /// makes no container.
    ///
    /// # Errors
    /// A shop that does not number ([`crate::compile::shops_of`]), and as [`write_rows`].
    pub fn write(merged: &Merged, key: &str, dats: &Path, folder: &Path) -> Result<Built, String> {
        let shops = if merged.rows.iter().any(|l| matches!(l.row, Row::Store(_))) {
            let install = Install::new(dats)?;
            let portal = install.file(ModernDat::Portal)?;
            let ctx = Context::from_portal(&portal).map_err(|e| format!("{e:?}"))?;
            let lib = library(&portal)?;
            let read = |id| portal.read(id).ok();
            let dat = crate::install::DatInstall::new(Box::new(read), ctx, &lib, Vec::new())?;
            crate::compile::shops_of(merged, &dat).map_err(|p| crate::report(&p))?
        } else {
            Vec::new()
        };
        rows_at(&clearings(merged), &shops, key, dats, folder)
    }

    /// The install's script library.
    fn library(portal: &DatFile) -> Result<Library, String> {
        portal
            .read_checked(RECORD, Library::from_record)
            .map(|l| l.value)
            .map_err(|e| format!("the script library: {e}"))
    }

    /// [`write()`] for the clearings a world holds, each with the landblock it is written in, and its
    /// shops, for a world whose rows are in hand.
    ///
    /// # Errors
    /// As [`cleared_rows`] and [`shop_copies`], the install not reading,
    /// and the containers not writing.
    pub fn write_rows(
        rows: &[(&Clearing, u16)],
        shops: &[Shop],
        key: &str,
        dats: &Path,
        out: &Path,
    ) -> Result<(PathBuf, Built), String> {
        let folder = out.join(FOLDER);
        let built = rows_at(rows, shops, key, dats, &folder)?;
        Ok((folder, built))
    }

    /// [`write_rows`] into `folder` itself.
    fn rows_at(
        rows: &[(&Clearing, u16)],
        shops: &[Shop],
        key: &str,
        dats: &Path,
        folder: &Path,
    ) -> Result<Built, String> {
        let dir = OverlayDir::new(folder).map_err(|e| e.to_string())?;
        if rows.is_empty() && shops.is_empty() && dir.containers().is_empty() {
            return Ok(Built::default());
        }
        let install = Install::new(dats)?;
        let portal = install.file(ModernDat::Portal)?;
        let cell = install.file(ModernDat::Cell)?;
        let ctx = Context::from_portal(&portal).map_err(|e| format!("{e:?}"))?;
        let copies = if shops.is_empty() {
            BTreeMap::new()
        } else {
            shop_copies(shops, &|id| portal.read(id).ok(), &ctx, &library(&portal)?)?
        };
        let records = WorldRecords::new(portal, cell.clone(), None, ctx.clone());
        let region = records.region().map_err(|e| format!("{e:?}"))?;
        let (blocks, mut built) = cleared_rows(rows, &cell, &ctx, &region, &records)?;
        built.shops = copies.len();
        publish(
            folder,
            key,
            &Records::from([(ModernDat::Cell, blocks), (ModernDat::Portal, copies)]),
            &install,
        )?;
        Ok(built)
    }

    /// Write `records` as the world `key`'s overlay into `out`'s [`FOLDER`], over the install in `dats`:
    /// the revision after what the folder holds, when it reads otherwise, of each file apart. Answers
    /// the folder; a world that changes no record and never did makes no container.
    ///
    /// # Errors
    /// A file the records lie over that the install lacks or that does not open, and the containers
    /// not writing.
    pub fn write_records(
        records: &Records,
        key: &str,
        dats: &Path,
        out: &Path,
    ) -> Result<PathBuf, String> {
        let folder = out.join(FOLDER);
        let dir = OverlayDir::new(&folder).map_err(|e| e.to_string())?;
        if records.values().all(BTreeMap::is_empty) && dir.containers().is_empty() {
            return Ok(folder);
        }
        publish(&folder, key, records, &Install::new(dats)?)?;
        Ok(folder)
    }

    /// The files of an install an overlay lies over, each opened when first wanted.
    struct Install {
        dir: PathBuf,
        files: Vec<(DatName, PathBuf)>,
        opened: RefCell<BTreeMap<ModernDat, DatFile>>,
    }

    impl Install {
        fn new(dats: &Path) -> Result<Self, String> {
            Ok(Self {
                dir: dats.to_path_buf(),
                files: install_dats(dats).map_err(|e| format!("{}: {e}", dats.display()))?,
                opened: RefCell::default(),
            })
        }

        /// The file playing `role`: the portal, the high-resolution pictures, the first region's cell
        /// file or the English file.
        fn file(&self, role: ModernDat) -> Result<DatFile, String> {
            if let Some(f) = self.opened.borrow().get(&role) {
                return Ok(f.clone());
            }
            let plays = |n: &DatName| match (n, role) {
                (DatName::Portal, ModernDat::Portal)
                | (DatName::HighRes, ModernDat::HighRes)
                | (DatName::Cell(1), ModernDat::Cell) => true,
                (DatName::Local(l), ModernDat::Local) => l.eq_ignore_ascii_case("english"),
                _ => false,
            };
            let path = self
                .files
                .iter()
                .find(|(n, _)| plays(n))
                .map(|(_, p)| p)
                .ok_or_else(|| format!("no {} in {}", role_file(role), self.dir.display()))?;
            let f = DatFile::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
            self.opened.borrow_mut().insert(role, f.clone());
            Ok(f)
        }
    }

    /// Make `folder` read as `records` over the install's files, numbered as the revision after what it
    /// holds, which is left as it is when it reads so already. Every file the world changes now or
    /// changed before is revised: one it no longer changes gets its records back in a revision of its
    /// own.
    fn publish(
        folder: &Path,
        key: &str,
        records: &Records,
        install: &Install,
    ) -> Result<(), String> {
        let last = OverlayDir::new(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
        let mut roles: BTreeSet<ModernDat> = records
            .iter()
            .filter(|(_, r)| !r.is_empty())
            .map(|(role, _)| *role)
            .collect();
        roles.extend(ModernDat::ALL.into_iter().filter(|r| last.holds(*r)));
        let files = roles
            .into_iter()
            .map(|role| Ok((role, install.file(role)?)))
            .collect::<Result<Vec<_>, String>>()?;
        swap(folder, key, records, &files).map_err(|e| format!("{}: {e}", folder.display()))
    }

    /// [`publish`] over files already open. One of another world, or made over other files, is not
    /// followed: the revisions start again.
    fn swap(
        folder: &Path,
        key: &str,
        records: &Records,
        files: &[(ModernDat, DatFile)],
    ) -> Result<(), ReviseError> {
        let beside = |name: &str| folder.with_file_name(name);
        let (fresh, next) = (beside(".overlay-fresh"), beside(".overlay-next"));
        let clean = |dir: &Path| -> Result<(), ReviseError> {
            if dir.exists() {
                std::fs::remove_dir_all(dir).map_err(dereth_dat::DatError::from)?;
            }
            Ok(())
        };
        clean(&fresh)?;
        clean(&next)?;
        let fresh_dir = OverlayDir::new(&fresh)?;
        for (role, base) in files {
            let Some(set) = records.get(role).filter(|r| !r.is_empty()) else {
                continue;
            };
            let mut writer = fresh_dir.writer(*role, base.shared(), role_file(*role), key, 0)?;
            let revision = decompose::revision_over(base.shared());
            for (id, bytes) in set {
                writer.put(DataId(*id), bytes, 1, revision, 0)?;
            }
            writer.add_iteration(revision, 0)?;
            writer.flush(0)?;
        }
        let bases: Vec<_> = files
            .iter()
            .map(|(role, base)| (*role, base.shared().clone()))
            .collect();
        let last = OverlayDir::new(folder)?;
        let next_dir = OverlayDir::new(&next)?;
        let revised = match revise_folder(&bases, Some(&last), &fresh_dir, &next_dir, key) {
            Err(ReviseError::Overlay(
                OverlayError::OtherWorld { .. } | OverlayError::BaseMismatch { .. },
            )) => {
                clean(&next)?;
                revise_folder(&bases, None, &fresh_dir, &next_dir, key)
            }
            other => other,
        }?;
        clean(&fresh)?;
        if revised.iter().all(|(_, r)| r.revision.is_none()) && !last.containers().is_empty() {
            return clean(&next);
        }
        clean(folder)?;
        if next.exists() {
            std::fs::rename(&next, folder).map_err(dereth_dat::DatError::from)?;
        }
        Ok(())
    }
}

#[cfg(feature = "fs")]
pub use folder::{write, write_records, write_rows};

#[cfg(test)]
mod tests {
    //! Behaviour: none (a clearing's shape, as the world builder reads it)
    use super::*;

    fn shape(heading: f32, size: [f32; 2]) -> Shape {
        let half = (size[0] / 2.0 + SPREAD, size[1] / 2.0 + SPREAD);
        let turn = heading.to_radians();
        Shape {
            x: 100.0,
            y: 100.0,
            turn: (math::cosf(turn), math::sinf(turn)),
            radius: None,
            half,
        }
    }

    #[test]
    fn a_turned_rectangle_lies_along_its_heading_clockwise_from_north() {
        // 80 long, 4 wide: unturned, it lies east to west.
        let s = shape(0.0, [80.0, 4.0]);
        assert!(s.holds(140.0, 100.0) && !s.holds(100.0, 140.0));
        // A quarter turn clockwise: north to south.
        let s = shape(90.0, [80.0, 4.0]);
        assert!(s.holds(100.0, 140.0) && !s.holds(140.0, 100.0));
        // An eighth clockwise: its east end swings south of east.
        let s = shape(45.0, [80.0, 4.0]);
        assert!(s.holds(120.0, 80.0) && !s.holds(120.0, 120.0));
    }

    fn row(radius: Option<f32>, size: Option<[f32; 2]>, heading: Option<f32>) -> Clearing {
        Clearing {
            key: "c".into(),
            at: crate::rows::At {
                x: 0.0,
                y: 0.0,
                z: None,
            },
            heading,
            radius,
            size,
            prov: crate::rows::Prov::AUTHORED,
        }
    }

    #[test]
    fn a_clearings_outline_lies_on_the_edge_of_what_it_keeps_clear() {
        for c in [
            row(Some(20.0), None, None),
            row(None, Some([80.0, 4.0]), Some(0.0)),
            row(None, Some([80.0, 4.0]), Some(35.0)),
        ] {
            let outer = Shape::of(&c, 0);
            let tight = Shape {
                radius: outer.radius.map(|r| r - SPREAD + 0.01),
                half: (outer.half.0 - SPREAD + 0.01, outer.half.1 - SPREAD + 0.01),
                ..outer
            };
            let inner = Shape {
                radius: outer.radius.map(|r| r - SPREAD - 0.5),
                half: (outer.half.0 - SPREAD - 0.5, outer.half.1 - SPREAD - 0.5),
                ..outer
            };
            let edge = outline(&c, [0.0, 0.0], 32);
            assert!(edge.len() >= 32);
            let (a, b) = (edge[0], edge[edge.len() - 1]);
            assert!(math::hypotf(a[0] - b[0], a[1] - b[1]) < 0.01, "it closes");
            for [x, y] in edge {
                assert!(tight.holds(x, y), "{c:?} {x} {y}");
                assert!(!inner.holds(x, y), "{c:?} {x} {y}");
            }
        }
    }

    #[test]
    fn a_shape_reaches_the_blocks_it_touches_and_stays_in_the_world() {
        let circle = |x, y, radius| Shape {
            x,
            y,
            turn: (1.0, 0.0),
            radius: Some(radius),
            half: (0.0, 0.0),
        };
        let blocks = |s: Shape| s.blocks().collect::<Vec<_>>();
        assert_eq!(blocks(circle(240.0, 240.0, 10.0)), [(1, 1)]);
        assert_eq!(
            blocks(circle(315.0, 240.0, 10.0)),
            [(1, 1), (2, 1)],
            "across an edge"
        );
        assert_eq!(
            blocks(circle(5.0, 5.0, 10.0)),
            [(0, 0)],
            "the world's corner"
        );
    }
}
