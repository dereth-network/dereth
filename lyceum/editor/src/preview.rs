//! The pieces' pictures: each piece drawn where retail uses it, inside one of its dungeons. The
//! piece is lit and textured, the cells joined to it are dimmed round it, and all are seen from
//! above and to the south-west with every polygon that faces away from the eye left out, so the
//! piece's ceiling and near walls are cut away and its floor, its far walls and its doorways
//! show. Each is drawn on the processor, off the frame loop, a picture at a time, and kept as a
//! PNG beside the model browser's pictures.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::Arc;

use dereth_assets::world::{EnvCell, LandblockInfo};
use dereth_assets::{Decode, Environment, Surface};
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::frame::{localtoglobal, V3};
use dereth_primitives::num::math;
use dereth_primitives::{DataId, Vec3};

/// A picture's size, in pixels.
pub const WIDTH: usize = 128;
pub const HEIGHT: usize = 96;

/// How bright the cells round the piece are drawn, against the piece.
const DIM: f32 = 0.35;

/// How many cells round the piece a picture shows at most.
const AROUND: usize = 14;

/// A piece: its environment and its place there.
pub type Key = (u32, u16);

/// The pictures asked for, drawn and shown.
#[derive(Default)]
pub struct Previews {
    shown: BTreeMap<Key, egui::TextureHandle>,
    failed: BTreeSet<Key>,
    asked: BTreeSet<Key>,
    jobs: Option<mpsc::Sender<(Key, u16)>>,
    done: Option<mpsc::Receiver<(Key, Option<Vec<u8>>)>>,
    /// How many pictures were drawn, and how many read from the cache.
    pub drawn: u64,
    pub read: u64,
}

impl std::fmt::Debug for Previews {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Previews")
            .field("shown", &self.shown.len())
            .field("asked", &self.asked.len())
            .finish_non_exhaustive()
    }
}

/// Where piece `key`'s picture is kept in `dir`.
#[must_use]
pub fn path(dir: &Path, key: Key) -> PathBuf {
    dir.join(format!("{:08X}-{}.png", key.0, key.1))
}

impl Previews {
    /// Piece `key`'s picture, drawn in landblock `landblock`: shown, else read from `dir`, else
    /// asked of the drawing thread (started over `store` the first time). `None` until it is
    /// drawn, or for one that will not draw.
    pub fn get(
        &mut self,
        ctx: &egui::Context,
        store: &Arc<RetailDatStore>,
        dir: &Path,
        key: Key,
        landblock: u16,
    ) -> Option<egui::TextureId> {
        self.take(ctx, dir);
        if let Some(t) = self.shown.get(&key) {
            return Some(t.id());
        }
        if self.failed.contains(&key) {
            return None;
        }
        if let Some(rgba) = std::fs::read(path(dir, key))
            .ok()
            .and_then(|b| decode_png(&b))
        {
            self.read += 1;
            self.show(ctx, key, &rgba);
            return self.shown.get(&key).map(egui::TextureHandle::id);
        }
        if self.asked.insert(key) {
            if self.jobs.is_none() {
                let (jobs, inbox) = mpsc::channel::<(Key, u16)>();
                let (outbox, done) = mpsc::channel();
                let store = Arc::clone(store);
                std::thread::spawn(move || {
                    let mut painter = Painter::new(store);
                    while let Ok((key, lb)) = inbox.recv() {
                        let _ = outbox.send((key, painter.draw(key, lb)));
                    }
                });
                self.jobs = Some(jobs);
                self.done = Some(done);
            }
            if let Some(j) = &self.jobs {
                let _ = j.send((key, landblock));
            }
        }
        None
    }

    /// Wait for every picture asked for, as a test does.
    pub fn wait(&mut self, ctx: &egui::Context, dir: &Path) {
        while self.asked.len() > self.shown.len() + self.failed.len() {
            let Some(rx) = &self.done else { return };
            let Ok((key, rgba)) = rx.recv() else { return };
            self.arrived(ctx, dir, key, rgba);
        }
    }

    /// The pictures the drawing thread has finished, kept and shown.
    fn take(&mut self, ctx: &egui::Context, dir: &Path) {
        let mut got = Vec::new();
        if let Some(rx) = &self.done {
            while let Ok(x) = rx.try_recv() {
                got.push(x);
            }
        }
        for (key, rgba) in got {
            self.arrived(ctx, dir, key, rgba);
        }
    }

    fn arrived(&mut self, ctx: &egui::Context, dir: &Path, key: Key, rgba: Option<Vec<u8>>) {
        let Some(rgba) = rgba else {
            self.failed.insert(key);
            return;
        };
        self.drawn += 1;
        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::write(path(dir, key), encode_png(&rgba));
        self.show(ctx, key, &rgba);
    }

    fn show(&mut self, ctx: &egui::Context, key: Key, rgba: &[u8]) {
        let image = egui::ColorImage::from_rgba_unmultiplied([WIDTH, HEIGHT], rgba);
        let handle = ctx.load_texture(
            format!("piece {:08X} {}", key.0, key.1),
            image,
            egui::TextureOptions::LINEAR,
        );
        self.shown.insert(key, handle);
    }
}

fn encode_png(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        #[allow(clippy::cast_possible_truncation)]
        let mut e = png::Encoder::new(&mut out, WIDTH as u32, HEIGHT as u32);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        if let Ok(mut w) = e.write_header() {
            let _ = w.write_image_data(rgba);
        }
    }
    out
}

fn decode_png(bytes: &[u8]) -> Option<Vec<u8>> {
    let d = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut r = d.read_info().ok()?;
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width as usize, info.height as usize);
    if (w, h) != (WIDTH, HEIGHT) || info.color_type != png::ColorType::Rgba {
        return None;
    }
    buf.truncate(w * h * 4);
    Some(buf)
}

/// A texture as the painter samples it: its side and its texels, red, green, blue.
struct Texels {
    width: usize,
    height: usize,
    rgb: Vec<[f32; 3]>,
}

/// One polygon to draw: its corners (landblock meters), their texture coordinates, its texture
/// (or plain color) and how bright it is drawn.
struct Face {
    corners: Vec<Vec3>,
    uvs: Vec<(f32, f32)>,
    look: Look,
    bright: f32,
}

#[derive(Clone)]
enum Look {
    Texture(Arc<Texels>),
    Color([f32; 3]),
}

/// The drawing thread's state: the files, and what it has read of them.
struct Painter {
    store: Arc<RetailDatStore>,
    envs: BTreeMap<u32, Option<Arc<Environment>>>,
    looks: BTreeMap<u32, Look>,
}

impl Painter {
    fn new(store: Arc<RetailDatStore>) -> Self {
        Self {
            store,
            envs: BTreeMap::new(),
            looks: BTreeMap::new(),
        }
    }

    fn environment(&mut self, id: DataId) -> Option<Arc<Environment>> {
        let store = &self.store;
        self.envs
            .entry(id.raw())
            .or_insert_with(|| {
                store
                    .read_typed(DbType::Environment, id)
                    .ok()
                    .and_then(|b| Environment::decode_payload(id, &b).ok())
                    .map(Arc::new)
            })
            .clone()
    }

    fn cell(&self, id: u32) -> Option<EnvCell> {
        let b = self.store.read_cell(DataId(id)).ok()?;
        EnvCell::decode_payload(DataId(id), &b).ok()
    }

    /// What surface `id` lays: its texture's texels, else its color.
    fn look(&mut self, id: DataId) -> Look {
        if let Some(l) = self.looks.get(&id.raw()) {
            return l.clone();
        }
        let surface = self
            .store
            .read_typed(DbType::Surface, id)
            .ok()
            .and_then(|b| Surface::decode_payload(id, &b).ok());
        let look = surface
            .as_ref()
            .and_then(|s| s.orig_texture_id)
            .and_then(|t| {
                dereth_scene::textures::TextureStore::new(&self.store)
                    .bgra8(t)
                    .ok()
            })
            .filter(|img| img.width > 0 && img.height > 0)
            .map_or_else(
                // A plain surface's color; mid gray when it has none.
                || {
                    surface
                        .and_then(|s| s.color_value)
                        .map_or(Look::Color([0.5, 0.5, 0.5]), |c| {
                            #[allow(clippy::cast_precision_loss)]
                            let ch = |shift: u32| ((c >> shift) & 0xFF) as f32 / 255.0;
                            Look::Color([ch(16), ch(8), ch(0)])
                        })
                },
                |img| {
                    let rgb = img
                        .pixels
                        .iter()
                        .map(|p| {
                            [
                                f32::from(p[2]) / 255.0,
                                f32::from(p[1]) / 255.0,
                                f32::from(p[0]) / 255.0,
                            ]
                        })
                        .collect();
                    Look::Texture(Arc::new(Texels {
                        width: img.width as usize,
                        height: img.height as usize,
                        rgb,
                    }))
                },
            );
        self.looks.insert(id.raw(), look.clone());
        look
    }

    /// The first cell of landblock `lb` built of piece `key`.
    fn cell_of(&self, key: Key, lb: u16) -> Option<EnvCell> {
        let lbi = DataId((u32::from(lb) << 16) | 0xFFFE);
        let info = self
            .store
            .read_cell(lbi)
            .ok()
            .and_then(|b| LandblockInfo::decode_payload(lbi, &b).ok())?;
        (0..info.num_cells).find_map(|i| {
            let n = u32::from(lyceum_foundry::cells::FIRST_CELL) + i;
            let c = self.cell((u32::from(lb) << 16) | n)?;
            (c.environment.raw() == key.0 && c.cell_struct == key.1).then_some(c)
        })
    }

    /// Piece `key`'s picture in landblock `lb`: [`WIDTH`] by [`HEIGHT`] pixels, red, green, blue,
    /// alpha. `None` when the landblock holds no cell of it.
    fn draw(&mut self, key: Key, lb: u16) -> Option<Vec<u8>> {
        let first = self.cell_of(key, lb)?;
        // The piece, and the cells joined to it, two ways out deep.
        let mut cells: Vec<(EnvCell, bool)> = vec![(first.clone(), true)];
        let mut seen: BTreeSet<u32> = [first.id.raw()].into();
        let mut queue: VecDeque<(EnvCell, usize)> = [(first, 0)].into();
        while let Some((c, depth)) = queue.pop_front() {
            if depth >= 2 {
                continue;
            }
            for p in &c.portals {
                let Ok(o) = u16::try_from(p.other_cell_id) else {
                    continue;
                };
                let id = (u32::from(lb) << 16) | u32::from(o);
                if o < lyceum_foundry::cells::FIRST_CELL
                    || cells.len() >= AROUND
                    || !seen.insert(id)
                {
                    continue;
                }
                if let Some(next) = self.cell(id) {
                    cells.push((next.clone(), false));
                    queue.push_back((next, depth + 1));
                }
            }
        }
        let mut faces = Vec::new();
        for (cell, piece) in &cells {
            let Some(env) = self.environment(cell.environment) else {
                continue;
            };
            let Some(cs) = env.cells.get(usize::from(cell.cell_struct)) else {
                continue;
            };
            let portals: BTreeSet<u16> = cs.portals.iter().copied().collect();
            for (i, poly) in cs.polygons.iter().enumerate() {
                if u16::try_from(i).is_ok_and(|i| portals.contains(&i)) {
                    continue;
                }
                let mut corners = Vec::new();
                let mut uvs = Vec::new();
                for (k, v) in poly.vertex_ids.iter().enumerate() {
                    let Some(sv) = cs.vertex_array.vertices.get(usize::from(*v)) else {
                        continue;
                    };
                    corners.push(localtoglobal(&cell.frame, sv.position));
                    let uv = poly
                        .pos_uv_indices
                        .as_ref()
                        .and_then(|u| u.get(k))
                        .and_then(|u| sv.uvs.get(usize::from(*u)))
                        .copied()
                        .unwrap_or((0.0, 0.0));
                    uvs.push(uv);
                }
                if corners.len() < 3 {
                    continue;
                }
                let look = cell
                    .surfaces
                    .get(usize::from(poly.pos_surface))
                    .map_or(Look::Color([0.5, 0.5, 0.5]), |s| self.look(*s));
                faces.push(Face {
                    corners,
                    uvs,
                    look,
                    bright: if *piece { 1.0 } else { DIM },
                });
            }
        }
        Some(paint(
            &faces,
            &cells[0].0,
            &*self.environment(cells[0].0.environment)?,
        ))
    }
}

/// A polygon's normal by Newell's method, of length one; zero for a degenerate one.
fn normal(c: &[Vec3]) -> Vec3 {
    let mut n = Vec3::ZERO;
    for k in 0..c.len() {
        let (a, b) = (c[k], c[(k + 1) % c.len()]);
        n.x += (a.y - b.y) * (a.z + b.z);
        n.y += (a.z - b.z) * (a.x + b.x);
        n.z += (a.x - b.x) * (a.y + b.y);
    }
    let len = n.dot(n).sqrt();
    if len < 1e-9 {
        Vec3::ZERO
    } else {
        n.mul(1.0 / len)
    }
}

/// Draw `faces` round the piece `cell` (built of `env`) into a picture: framed on the piece, lit
/// from above, the faces that look away from the eye left out.
fn paint(faces: &[Face], cell: &EnvCell, env: &Environment) -> Vec<u8> {
    // Twice the size, halved at the end, so edges are smooth.
    let (w, h) = (WIDTH * 2, HEIGHT * 2);
    let cs = &env.cells[usize::from(cell.cell_struct)];
    let pts: Vec<Vec3> = cs
        .vertex_array
        .vertices
        .iter()
        .map(|v| localtoglobal(&cell.frame, v.position))
        .collect();
    #[allow(clippy::cast_precision_loss)]
    let middle = pts
        .iter()
        .fold(Vec3::ZERO, |m, p| m.add(*p))
        .mul(1.0 / pts.len().max(1) as f32);
    let radius = pts
        .iter()
        .map(|p| p.sub(middle).magnitude())
        .fold(1.0f32, f32::max);
    // Which way the polygons face: a cell's walls are drawn from inside, so its own face its
    // middle; the painter turns them so that a face drawn is one that looks at the eye.
    let inward = {
        let mut s = 0.0;
        for f in faces.iter().filter(|f| f.bright >= 1.0) {
            #[allow(clippy::cast_precision_loss)]
            let c = f
                .corners
                .iter()
                .fold(Vec3::ZERO, |m, p| m.add(*p))
                .mul(1.0 / f.corners.len() as f32);
            s += normal(&f.corners).dot(middle.sub(c));
        }
        s >= 0.0
    };
    // The eye: south-west of the piece and above it, far enough to hold it.
    let fov = 0.9f32;
    let back = Vec3::new(-0.45, -0.75, 0.85);
    let back = back.mul(1.0 / back.magnitude());
    let distance = radius / math::tanf(fov / 2.0) * 1.05;
    let eye = middle.add(back.mul(distance));
    let forward = back.mul(-1.0);
    let right = {
        let r = forward.cross(Vec3::new(0.0, 0.0, 1.0));
        r.mul(1.0 / r.magnitude().max(1e-6))
    };
    let up = right.cross(forward);
    #[allow(clippy::cast_precision_loss)]
    let (fw, fh) = (w as f32, h as f32);
    let focal = (fh / 2.0) / math::tanf(fov / 2.0);
    let project = |p: Vec3| -> Option<(f32, f32, f32)> {
        let d = p.sub(eye);
        let z = d.dot(forward);
        if z < 0.05 {
            return None;
        }
        Some((
            fw / 2.0 + d.dot(right) * focal / z,
            fh / 2.0 - d.dot(up) * focal / z,
            z,
        ))
    };
    let light = Vec3::new(0.3, -0.4, 1.0);
    let light = light.mul(1.0 / light.magnitude());
    let mut color = vec![[0.05f32, 0.05, 0.06]; w * h];
    let mut depth = vec![f32::MAX; w * h];
    for f in faces {
        let mut n = normal(&f.corners);
        if !inward {
            n = n.mul(-1.0);
        }
        // Looking away from the eye: cut away.
        if n.dot(eye.sub(f.corners[0])) <= 0.0 {
            continue;
        }
        let shade = (0.5 + 0.8 * n.dot(light).max(0.0)) * f.bright;
        let Some(projected) = f
            .corners
            .iter()
            .map(|c| project(*c))
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        for k in 1..projected.len() - 1 {
            let tri = [projected[0], projected[k], projected[k + 1]];
            let uv = [f.uvs[0], f.uvs[k], f.uvs[k + 1]];
            fill(&mut color, &mut depth, (w, h), &tri, &uv, &f.look, shade);
        }
    }
    // Halved: each pixel the mean of four.
    let mut out = vec![0u8; WIDTH * HEIGHT * 4];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let mut s = [0.0f32; 3];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let c = color[(y * 2 + dy) * w + x * 2 + dx];
                for k in 0..3 {
                    s[k] += c[k] / 4.0;
                }
            }
            let o = (y * WIDTH + x) * 4;
            for k in 0..3 {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                {
                    out[o + k] = (s[k].clamp(0.0, 1.0) * 255.0).round() as u8;
                }
            }
            out[o + 3] = 255;
        }
    }
    out
}

/// One triangle `tri` (pixels and depth) into `color`, nearer than what `depth` holds, its texture
/// coordinates `uv` interpolated for perspective.
fn fill(
    color: &mut [[f32; 3]],
    depth: &mut [f32],
    (w, h): (usize, usize),
    tri: &[(f32, f32, f32); 3],
    uv: &[(f32, f32); 3],
    look: &Look,
    shade: f32,
) {
    let (x0, x1) = (
        tri.iter()
            .map(|p| p.0)
            .fold(f32::MAX, f32::min)
            .floor()
            .max(0.0),
        tri.iter().map(|p| p.0).fold(f32::MIN, f32::max).ceil(),
    );
    let (y0, y1) = (
        tri.iter()
            .map(|p| p.1)
            .fold(f32::MAX, f32::min)
            .floor()
            .max(0.0),
        tri.iter().map(|p| p.1).fold(f32::MIN, f32::max).ceil(),
    );
    let area = (tri[1].0 - tri[0].0) * (tri[2].1 - tri[0].1)
        - (tri[2].0 - tri[0].0) * (tri[1].1 - tri[0].1);
    if area.abs() < 1e-6 {
        return;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (xa, xb, ya, yb) = (
        x0 as usize,
        (x1 as usize).min(w),
        y0 as usize,
        (y1 as usize).min(h),
    );
    for y in ya..yb {
        for x in xa..xb {
            #[allow(clippy::cast_precision_loss)]
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let e = |a: (f32, f32, f32), b: (f32, f32, f32)| {
                (b.0 - a.0) * (py - a.1) - (b.1 - a.1) * (px - a.0)
            };
            let w0 = e(tri[1], tri[2]) / area;
            let w1 = e(tri[2], tri[0]) / area;
            let w2 = e(tri[0], tri[1]) / area;
            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                continue;
            }
            // Perspective: one over depth is what interpolates on screen.
            let iz = w0 / tri[0].2 + w1 / tri[1].2 + w2 / tri[2].2;
            let z = 1.0 / iz;
            let i = y * w + x;
            if z >= depth[i] {
                continue;
            }
            depth[i] = z;
            let base = match look {
                Look::Color(c) => *c,
                Look::Texture(t) => {
                    let u = (w0 * uv[0].0 / tri[0].2
                        + w1 * uv[1].0 / tri[1].2
                        + w2 * uv[2].0 / tri[2].2)
                        * z;
                    let v = (w0 * uv[0].1 / tri[0].2
                        + w1 * uv[1].1 / tri[1].2
                        + w2 * uv[2].1 / tri[2].2)
                        * z;
                    #[allow(
                        clippy::cast_precision_loss,
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss
                    )]
                    let (tx, ty) = (
                        ((u - u.floor()) * t.width as f32) as usize % t.width,
                        ((v - v.floor()) * t.height as f32) as usize % t.height,
                    );
                    t.rgb[ty * t.width + tx]
                }
            };
            color[i] = [base[0] * shade, base[1] * shade, base[2] * shade];
        }
    }
}
