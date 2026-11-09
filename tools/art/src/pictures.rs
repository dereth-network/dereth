//! What each retail picture is drawn on: where in the world, which way it faces and how large one
//! repeat of it is. Unlike a model's own row, this reads every surface list a piece's cells use.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use dereth_dat::RetailDatStore;
use dereth_primitives::{DataId, TextureFormat};
use dereth_render_cpu::surface::surface_type;
use dereth_scene::textures::TextureStore;

use crate::model::{Mesh, Models, Vert};
use crate::tally::{ModelId, Tally, Use};

/// The kinds of place a world use is in.
pub const PLACES: [&str; 3] = ["dungeon", "room", "outdoors"];
/// The ways a triangle may face.
pub const FACINGS: [&str; 4] = ["wall", "floor", "slope", "ceiling"];

/// How far up a floor's normal points (the cosine); past this a surface is a floor.
const FLOOR_FROM: f32 = 0.9;
/// How far up a slope's normal points; between this and [`FLOOR_FROM`] a surface is a slope.
const SLOPE_FROM: f32 = 0.2;

/// What one picture is drawn on.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PictureUse {
    pub width: u32,
    pub height: u32,
    /// The first surface that draws it, by which its pixels are decoded.
    pub surface: Option<DataId>,
    /// Every surface type bit its surfaces set.
    pub surface_bits: u32,
    /// Instances of what draws it, by use.
    pub instances: BTreeMap<Use, u64>,
    /// Square meters drawn over every instance, by [`PLACES`] and [`FACINGS`].
    pub area: [[f64; 4]; 3],
    /// The meters one repeat spans along its width and its height, the middle by area.
    pub repeat: Option<(f32, f32)>,
    /// A place it is drawn most in: the model, and a cell or block to find it at.
    pub seen: Option<(ModelId, String)>,
    /// The area [`Self::seen`] was picked by.
    seen_area: f64,
    /// Each triangle's meters along the width and the height, and its area over every instance.
    spans: Vec<(f32, f32, f64)>,
}

impl PictureUse {
    /// Whether a body wears it.
    #[must_use]
    pub fn worn(&self) -> bool {
        self.instances
            .keys()
            .any(|u| matches!(u, Use::Character | Use::Clothing))
    }

    /// Whether it is on the fabric of the world: a cell's piece or a building's shell.
    #[must_use]
    pub fn built(&self) -> bool {
        self.instances
            .keys()
            .any(|u| matches!(u, Use::DungeonPiece | Use::InteriorPiece | Use::Building))
    }
}

/// `use_`'s index in [`PLACES`]; `None` for what is not placed in the world's geometry.
fn place_of(use_: Use) -> Option<usize> {
    match use_ {
        Use::DungeonPiece | Use::DungeonStatic => Some(0),
        Use::InteriorPiece | Use::InteriorStatic => Some(1),
        Use::Scenery | Use::Static | Use::Building => Some(2),
        Use::Spawn | Use::Character | Use::Clothing => None,
    }
}

/// `up`'s index in [`FACINGS`], where `up` is how far up a normal points.
#[must_use]
pub fn facing_of(up: f32) -> usize {
    if up > FLOOR_FROM {
        1
    } else if up > SLOPE_FROM {
        2
    } else if up < -SLOPE_FROM {
        3
    } else {
        0
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn length(a: [f32; 3]) -> f32 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

/// A triangle's area, in square meters, and the meters one repeat of its picture spans along the
/// width and the height; no spans where its coordinates do not change across it.
#[must_use]
pub fn measure(t: &[Vert; 3]) -> (f32, Option<(f32, f32)>) {
    let e1 = sub(t[1].position, t[0].position);
    let e2 = sub(t[2].position, t[0].position);
    let c = [
        e1[1] * e2[2] - e1[2] * e2[1],
        e1[2] * e2[0] - e1[0] * e2[2],
        e1[0] * e2[1] - e1[1] * e2[0],
    ];
    let area = length(c) / 2.0;
    let (du1, dv1) = (t[1].uv[0] - t[0].uv[0], t[1].uv[1] - t[0].uv[1]);
    let (du2, dv2) = (t[2].uv[0] - t[0].uv[0], t[2].uv[1] - t[0].uv[1]);
    let det = du1 * dv2 - du2 * dv1;
    if det.abs() < 1e-6 {
        return (area, None);
    }
    let along = |a: f32, b: f32| {
        length([
            (e1[0] * a + e2[0] * b) / det,
            (e1[1] * a + e2[1] * b) / det,
            (e1[2] * a + e2[2] * b) / det,
        ])
    };
    (area, Some((along(dv2, -dv1), along(-du2, du1))))
}

/// How far up a triangle faces: by its corners, as its vertices' normals are smoothed round a room's
/// corners, and on the side those normals are on.
#[must_use]
pub fn up_of(t: &[Vert; 3]) -> f32 {
    let (e1, e2) = (
        sub(t[1].position, t[0].position),
        sub(t[2].position, t[0].position),
    );
    let c = [
        e1[1] * e2[2] - e1[2] * e2[1],
        e1[2] * e2[0] - e1[0] * e2[2],
        e1[0] * e2[1] - e1[1] * e2[0],
    ];
    let side = (0..3)
        .map(|k| t[0].normal[k] * c[k] + t[1].normal[k] * c[k] + t[2].normal[k] * c[k])
        .sum::<f32>();
    let up = c[2] / length(c).max(1e-9);
    if side < 0.0 {
        -up
    } else {
        up
    }
}

/// The middle of `spans` by their weights, along the width or the height.
fn middle(spans: &mut [(f32, f32, f64)], along: fn(&(f32, f32, f64)) -> f32) -> Option<f32> {
    spans.sort_by(|a, b| along(a).total_cmp(&along(b)));
    let half = spans.iter().map(|s| s.2).sum::<f64>() / 2.0;
    let mut sum = 0.0;
    for s in spans.iter() {
        sum += s.2;
        if sum >= half {
            return Some(along(s));
        }
    }
    spans.last().map(along)
}

/// What every picture is drawn on, over every instance and cell in `tally`.
#[must_use]
pub fn collect(store: &RetailDatStore, tally: &Tally) -> BTreeMap<DataId, PictureUse> {
    let mut reader = Reader {
        models: Models::new(store),
        textures: TextureStore::new(store),
        surfaces: HashMap::new(),
        pictures: BTreeMap::new(),
    };
    for ((id, list), cells) in &tally.cell_lists {
        let Some(mesh) = reader.models.mesh(*id, list) else {
            continue;
        };
        let seen = format!("cell 0x{:08X}", cells.lowest);
        reader.add(*id, &mesh, &cells.uses, &seen);
    }
    for (id, uses) in &tally.uses {
        if !matches!(id, ModelId::Object(_)) {
            continue;
        }
        let Some(mesh) = reader.models.mesh(*id, &[]) else {
            continue;
        };
        let counts: BTreeMap<Use, u64> = uses.iter().map(|(u, s)| (*u, s.instances)).collect();
        let seen = uses
            .values()
            .flat_map(|s| s.examples.first())
            .max_by_key(|e| e.count)
            .map_or_else(String::new, |e| match (e.cell, e.position) {
                // Cells from 0x100 up are indoors; the rest are the land's, found by their place.
                (Some(c), _) if c & 0xFFFF >= 0x100 => format!("cell 0x{c:08X}"),
                (_, Some(p)) => {
                    format!(
                        "block {:04X} at {:.1},{:.1},{:.1}",
                        e.landblock, p.x, p.y, p.z
                    )
                }
                (_, None) => format!("block {:04X}", e.landblock),
            });
        reader.add(*id, &mesh, &counts, &seen);
    }
    let mut pictures = reader.pictures;
    for p in pictures.values_mut() {
        let u = middle(&mut p.spans, |s| s.0);
        let v = middle(&mut p.spans, |s| s.1);
        p.repeat = u.zip(v);
        p.spans = Vec::new();
    }
    pictures
}

/// What collecting reads with and adds to.
struct Reader<'a> {
    models: Models<'a>,
    textures: TextureStore<'a>,
    // ORDER-OK: a decode memo, only ever looked up.
    surfaces: HashMap<DataId, Option<(DataId, u32, u32, u32)>>,
    pictures: BTreeMap<DataId, PictureUse>,
}

impl Reader<'_> {
    /// A surface's picture, its size and its type bits; `None` for one with no picture.
    fn picture_of(&mut self, s: DataId) -> Option<(DataId, u32, u32, u32)> {
        if let Some(known) = self.surfaces.get(&s) {
            return *known;
        }
        let found = self.models.surface(s).and_then(|r| {
            let t = r.orig_texture_id?;
            let (_, rs, _) = self.textures.resolve(t).ok()?;
            Some((t, rs.width, rs.height, r.surface_type))
        });
        self.surfaces.insert(s, found);
        found
    }

    /// Adds what `id`'s `mesh` draws, `counts` instances of it in each use.
    fn add(&mut self, id: ModelId, mesh: &Mesh, counts: &BTreeMap<Use, u64>, seen: &str) {
        let mut placed = [0.0f64; 3];
        for (u, n) in counts {
            if let Some(p) = place_of(*u) {
                #[allow(clippy::cast_precision_loss)] // LINT-OK: a count of instances.
                let many = *n as f64;
                placed[p] += many;
            }
        }
        let every: f64 = placed.iter().sum();
        // A picture is counted once for each model that draws it, however many surfaces do.
        let mut counted: Vec<DataId> = Vec::new();
        for g in &mesh.groups {
            let Some(surface) = g.surface else {
                continue;
            };
            let Some((t, width, height, bits)) = self.picture_of(surface) else {
                continue;
            };
            let p = self.pictures.entry(t).or_default();
            p.width = width;
            p.height = height;
            p.surface.get_or_insert(surface);
            p.surface_bits |= bits;
            if !counted.contains(&t) {
                counted.push(t);
                for (u, n) in counts {
                    *p.instances.entry(*u).or_default() += n;
                }
            }
            let mut here = 0.0;
            for tri in &g.triangles {
                let (area, spans) = measure(tri);
                let facing = facing_of(up_of(tri));
                for (k, many) in placed.iter().enumerate() {
                    p.area[k][facing] += f64::from(area) * many;
                }
                if let Some((u, v)) = spans {
                    p.spans.push((u, v, f64::from(area) * every));
                }
                here += f64::from(area) * every;
            }
            if here > p.seen_area {
                p.seen_area = here;
                p.seen = Some((id, seen.to_owned()));
            }
        }
    }
}

/// Writes each picture the world draws as `<id>.png` in `dir`, as the client decodes it.
///
/// # Errors
/// The folder or a picture will not write.
pub fn write_pixels(
    store: &RetailDatStore,
    pictures: &BTreeMap<DataId, PictureUse>,
    dir: &Path,
) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let textures = TextureStore::new(store);
    let mut models = Models::new(store);
    for (id, p) in pictures {
        let Some(surface) = p
            .surface
            .filter(|_| p.instances.keys().any(|u| place_of(*u).is_some()))
        else {
            continue;
        };
        let Some(record) = models.surface(surface) else {
            continue;
        };
        let clip = record.surface_type & surface_type::BASE1_CLIPMAP != 0;
        let decoded = if textures.is_palettised(surface).unwrap_or(false) {
            textures
                .texture_data_clipped(surface, clip)
                .ok()
                .filter(|d| d.format == TextureFormat::Bgra8)
                .and_then(|d| {
                    let (w, h) = (d.width, d.height);
                    let level = d.levels.into_iter().next()?;
                    Some((w, h, level.as_chunks::<4>().0.to_vec()))
                })
        } else {
            textures
                .bgra8(surface)
                .ok()
                .map(|b| (b.width, b.height, b.pixels))
        };
        let Some((w, h, bgra)) = decoded else {
            continue;
        };
        let rgba: Vec<u8> = bgra.iter().flat_map(|p| [p[2], p[1], p[0], p[3]]).collect();
        let path = dir.join(format!("0x{:08X}.png", id.0));
        let bytes = png_of(w, h, &rgba).map_err(|e| format!("{}: {e}", path.display()))?;
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}

fn png_of(w: u32, h: u32, rgba: &[u8]) -> Result<Vec<u8>, png::EncodingError> {
    let mut out = Vec::new();
    {
        let mut e = png::Encoder::new(&mut out, w, h);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header()?.write_image_data(rgba)?;
    }
    Ok(out)
}
