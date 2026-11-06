//! A world's overlay: the records of the client's data files a world changes, built from its rows
//! into a folder beside its tables. The install is never written.
//!
//! A `[[clearing]]` row keeps a spot free of what the land grows. Nothing places scenery: the client
//! works it out from each land vertex's scene field, so a clearing gives the vertices in its shape
//! a field under which their terrain grows nothing. A vertex's scene can spread past the vertex,
//! so the shape is widened by [`SPREAD`], and a vertex on a block's edge is its neighbor's too, so
//! every block the shape reaches is changed. What a vertex grows includes its ambient sound.
//!
//! A world with no clearing has no overlay: the folder is left empty and everything reads the
//! install's own records.
//!
//! A build is numbered as the revision after the folder's last, so a client holding the last one
//! is sent what changed alone. A build that changes nothing leaves the folder as it is, and one
//! that does replaces it only once it is written whole, so a server reading the folder as it runs
//! keeps what it has until it opens it again.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use auberean_assets::landblock::Landblock;
use auberean_assets::{decode, encode, Context};
use auberean_dat::locate::{install_dats, role_file, DatName};
use auberean_dat::overlay::WORLD_FOLDER;
use auberean_dat::DatFile;
use auberean_world::records::{RegionSet, WorldRecords};
use auberean_world::scenery::barren_field;
use dereth_dat::overlay::{OverlayDir, OverlayError};
use dereth_dat::revise::{revise_folder, ReviseError};
use dereth_dat::{decompose, ModernDat};
use dereth_primitives::num::math;
use dereth_primitives::DataId;
use yalain_world_db::merge::Merged;
use yalain_world_db::rows::{self, Clearing, Row};

use crate::build;
use crate::world::Opened;

/// The overlay's folder: Yalain reads this one in a world's folder.
pub const FOLDER: &str = WORLD_FOLDER;
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

/// Write the world's overlay into `out`'s [`FOLDER`]: the revision after what the folder holds,
/// when it reads otherwise. Answers the folder and what it holds; a world that changes no record
/// and never did makes no container.
///
/// # Errors
/// As [`write_rows`], and the world not checking.
pub fn write(world: &Opened, dats: &Path, out: &Path) -> Result<(PathBuf, Built), String> {
    let merged = build::merged(world)?;
    let key = rows::read(&world.files)
        .map_err(|p| yalain_world_db::report(&p))?
        .world
        .key;
    write_rows(&clearings(&merged), &key, dats, out)
}

/// [`write()`] for the clearings a world holds, each with the landblock it is written in, for a world
/// whose rows are in hand.
///
/// # Errors
/// As [`cleared_rows`], the install not reading, and the containers not writing.
pub fn write_rows(
    rows: &[(&Clearing, u16)],
    key: &str,
    dats: &Path,
    out: &Path,
) -> Result<(PathBuf, Built), String> {
    let folder = out.join(FOLDER);
    let dir = OverlayDir::new(&folder).map_err(|e| e.to_string())?;
    if rows.is_empty() && !dir.holds(ModernDat::Cell) {
        return Ok((folder, Built::default()));
    }
    let files = install_dats(dats).map_err(|e| format!("{}: {e}", dats.display()))?;
    let open = |name: DatName| {
        let path = files
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, p)| p)
            .ok_or_else(|| format!("no {} in {}", name.file_name(), dats.display()))?;
        DatFile::open(path).map_err(|e| format!("{}: {e}", path.display()))
    };
    let portal = open(DatName::Portal)?;
    let cell = open(DatName::Cell(1))?;
    let ctx = Context::from_portal(&portal).map_err(|e| format!("{e:?}"))?;
    let records = WorldRecords::new(portal, cell.clone(), None, ctx.clone());
    let region = records.region().map_err(|e| format!("{e:?}"))?;
    let (blocks, built) = cleared_rows(rows, &cell, &ctx, &region, &records)?;
    publish(&folder, &cell, key, &blocks).map_err(|e| format!("{}: {e}", folder.display()))?;
    Ok((folder, built))
}

/// Make `folder` read as `blocks` over `cell`, numbered as the revision after what it holds, which is
/// left as it is when it reads so already. One of another world, or made over other files, is
/// not followed: the revisions start again.
fn publish(
    folder: &Path,
    cell: &DatFile,
    key: &str,
    blocks: &BTreeMap<u32, Vec<u8>>,
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
    if !blocks.is_empty() {
        let mut writer = fresh_dir.writer(
            ModernDat::Cell,
            cell.shared(),
            role_file(ModernDat::Cell),
            key,
            0,
        )?;
        let revision = decompose::revision_over(cell.shared());
        for (id, bytes) in blocks {
            writer.put(DataId(*id), bytes, 1, revision, 0)?;
        }
        writer.add_iteration(revision, 0)?;
        writer.flush(0)?;
    }
    let bases = [(ModernDat::Cell, cell.shared().clone())];
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
    if revised.iter().all(|(_, r)| r.revision.is_none()) && last.holds(ModernDat::Cell) {
        return clean(&next);
    }
    clean(folder)?;
    if next.exists() {
        std::fs::rename(&next, folder).map_err(dereth_dat::DatError::from)?;
    }
    Ok(())
}

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
            at: yalain_world_db::rows::At {
                x: 0.0,
                y: 0.0,
                z: None,
            },
            heading,
            radius,
            size,
            prov: yalain_world_db::rows::Prov::AUTHORED,
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
