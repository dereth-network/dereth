//! The panels' toolkit: egui, fed the window's events and drawn through the runtime's overlay.
//!
//! **Input.** The window's device events ([`HostEvent`]) become egui's: the pointer, its buttons
//! and wheel, and the keys with the text they type, with the modifiers held. What egui takes (a
//! click on a panel, typing in a field) the world does not; the editor asks after each pass.
//!
//! **Drawing.** egui's shapes are tessellated into triangles over its textures. The textures (its
//! font atlas, the pictures the panels show) are kept here as straight-alpha RGBA and uploaded to
//! the overlay whole whenever egui changes them; the triangles become overlay triangles in clip
//! space, cut to their clip rectangles here (the overlay has no scissor), their colours un-
//! premultiplied, each wound counter-clockwise, as the overlay draws its front faces. So the panels
//! draw on every device the runtime has.
//!
//! **Text** is the carried Liberation Sans (and Mono for numbers in tables), so no font of
//! another licence is linked in.
//!
//! **Scale.** The panels are laid out in points, [`Gui::scale`] pixels each: the window's events
//! come in pixels and are given to egui in points, and its shapes go back out in pixels.

use std::collections::HashMap;
use std::sync::Arc;

use dereth_client_contract::overlay::{
    OverlayItem, OverlayMaterial, OverlaySampler, OverlaySpace, OverlayTexture, OverlayVertex,
    PreviewSpace,
};
use dereth_input::host::HostEvent;
use dereth_input::keys::MouseButton;
use dereth_primitives::{TextureData, TextureFormat};

/// The carried sans, for the panels' text.
static SANS: &[u8] =
    include_bytes!("../../../dereth/client/crates/classic-fonts/fonts/LiberationSans-Bold.ttf");
/// The carried mono, for numbers in tables.
static MONO: &[u8] =
    include_bytes!("../../../dereth/client/crates/classic-fonts/fonts/LiberationMono-Bold.ttf");

/// The most egui's clock moves in one frame, in seconds: a press and its release either side of
/// a slow frame (a world loading) are still a click.
const LONGEST_STEP: f64 = 0.25;

/// The first overlay key egui's textures take.
const KEY_BASE: u64 = 0xED20_0000;

/// One of egui's textures, as the overlay holds it.
#[derive(Debug)]
struct Texture {
    key: OverlayTexture,
    size: [usize; 2],
    /// Straight-alpha RGBA, row by row.
    rgba: Vec<u8>,
    linear: bool,
}

/// The panels' context, its textures, and the keys held.
pub struct Gui {
    pub ctx: egui::Context,
    textures: HashMap<egui::TextureId, Texture>,
    /// Textures to upload before the next frame, as the overlay takes them.
    pub uploads: Vec<(OverlayTexture, TextureData)>,
    modifiers: egui::Modifiers,
    pointer: egui::Pos2,
    /// Pixels to the point.
    scale: f32,
    /// The time egui was last given, and what is added to a frame's time so that egui's never
    /// goes back (the editor's frames and a playtest's are timed by different clocks) and never
    /// leaps more than [`LONGEST_STEP`].
    last_time: Option<f64>,
    time_offset: f64,
    /// The bytes of texture handed to the overlay so far.
    pub texture_bytes: u64,
}

/// A panel's drawing made once into a picture of its own and shown as one image, frame after
/// frame, until what it shows changes: a panel of thousands of shapes ([`Picture::draw`]) costs a
/// frame one textured rectangle, and the device its triangles once.
///
/// The picture is the panel's pixels: its shapes tessellated as egui tessellates them, and each
/// triangle filled as the device fills it (a pixel whose middle it covers, an edge shared by two
/// triangles given to one of them), its colors blended over what is under it.
pub struct Picture {
    texture: egui::TextureHandle,
    rect: egui::Rect,
}

impl std::fmt::Debug for Picture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Picture")
            .field("rect", &self.rect)
            .finish_non_exhaustive()
    }
}

impl Picture {
    /// `shapes` painted in `rect` (points), the first of them filling it, as a picture; the
    /// texture of `old` is drawn over when it is the same size.
    #[must_use]
    pub fn draw(
        ctx: &egui::Context,
        rect: egui::Rect,
        shapes: Vec<egui::Shape>,
        old: Option<Self>,
    ) -> Self {
        let ppp = ctx.pixels_per_point();
        let (lo, hi) = ((rect.min * ppp).floor(), (rect.max * ppp).ceil());
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a panel's pixels
        let size = [
            (hi.x - lo.x).max(1.0) as usize,
            (hi.y - lo.y).max(1.0) as usize,
        ];
        let clipped = shapes
            .into_iter()
            .map(|shape| egui::epaint::ClippedShape {
                clip_rect: rect,
                shape,
            })
            .collect();
        let primitives = ctx.tessellate(clipped, ppp);
        let rgba = rasterize(&primitives, ppp, (lo.x, lo.y), size);
        let image = egui::ColorImage::from_rgba_unmultiplied(size, &rgba);
        let texture = match old {
            Some(mut o) if o.texture.size() == size => {
                o.texture.set(image, egui::TextureOptions::NEAREST);
                o.texture
            }
            _ => ctx.load_texture("panel picture", image, egui::TextureOptions::NEAREST),
        };
        Self {
            texture,
            rect: egui::Rect::from_min_max(lo / ppp, hi / ppp),
        }
    }

    /// Show the picture where `painter` paints, as its next shape.
    pub fn show(&self, painter: &egui::Painter) {
        self.show_moved(painter, egui::Vec2::ZERO);
    }

    /// Show the picture moved by `by` (points), as its next shape.
    pub fn show_moved(&self, painter: &egui::Painter, by: egui::Vec2) {
        painter.image(
            self.texture.id(),
            self.rect.translate(by),
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    }
}

/// The triangles of `primitives` (points, `ppp` pixels each) filled into an image of `size`
/// pixels whose top left is pixel `origin` of the screen, in order, each cut to its clip
/// rectangle: straight RGBA, row by row. Only solid colors are drawn (no texture is sampled), as
/// the panels' shapes are.
fn rasterize(
    primitives: &[egui::ClippedPrimitive],
    ppp: f32,
    origin: (f32, f32),
    size: [usize; 2],
) -> Vec<u8> {
    let [w, h] = size;
    // Points to the image's pixels, in 256ths.
    #[allow(clippy::cast_possible_truncation)]
    let round = |v: f32| (v + 0.5f32.copysign(v)) as i64;
    let fixed = |p: egui::Pos2| {
        (
            round((p.x * ppp - origin.0) * 256.0),
            round((p.y * ppp - origin.1) * 256.0),
        )
    };
    // Every triangle, cut to its clip rectangle, in the order it is drawn, wound one way: worked
    // out a run of triangles a thread at a time, and the runs put back in order.
    const RUN: usize = 4096;
    let mut runs: Vec<(&egui::epaint::Mesh, egui::Rect, std::ops::Range<usize>)> = Vec::new();
    for egui::ClippedPrimitive {
        clip_rect,
        primitive,
    } in primitives
    {
        let egui::epaint::Primitive::Mesh(mesh) = primitive else {
            continue;
        };
        let n = mesh.indices.len() / 3;
        for start in (0..n).step_by(RUN) {
            runs.push((mesh, *clip_rect, start..(start + RUN).min(n)));
        }
    }
    let gather =
        |(mesh, clip_rect, range): &(&egui::epaint::Mesh, egui::Rect, std::ops::Range<usize>)| {
            let mut tris: Vec<Tri> = Vec::with_capacity(range.len());
            // Runs of corners share a color: the last one's straight form is kept.
            let mut last = (egui::Color32::TRANSPARENT, [0.0f32; 4], [0u8; 4]);
            let mut straight_of = move |c: egui::Color32| {
                if c != last.0 {
                    let s = unmultiply(c);
                    last = (c, s.map(|v| f32::from(v) / 255.0), s);
                }
                (last.1, last.2)
            };
            let mut push = |t: &[egui::epaint::Vertex; 3]| {
                let (mut a, mut b, c) = (t[0], t[1], t[2]);
                let (mut pa, mut pb, pc) = (fixed(a.pos), fixed(b.pos), fixed(c.pos));
                let mut area = edge(pa, pb, pc);
                if area == 0 {
                    return;
                }
                if area < 0 {
                    std::mem::swap(&mut a, &mut b);
                    std::mem::swap(&mut pa, &mut pb);
                    area = -area;
                }
                let ((ca, solid), (cb, _), (cc, _)) = (
                    straight_of(a.color),
                    straight_of(b.color),
                    straight_of(c.color),
                );
                tris.push(Tri {
                    p: [pa, pb, pc],
                    area,
                    c: [ca, cb, cc],
                    flat: a.color == b.color && b.color == c.color,
                    solid,
                });
            };
            for t in &mesh.indices.as_chunks::<3>().0[range.clone()] {
                let tri = [
                    mesh.vertices[t[0] as usize],
                    mesh.vertices[t[1] as usize],
                    mesh.vertices[t[2] as usize],
                ];
                if tri.iter().all(|v| clip_rect.contains(v.pos)) {
                    push(&tri);
                } else {
                    for poly in clip(&tri, *clip_rect) {
                        push(&poly);
                    }
                }
            }
            tris
        };
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let done: Vec<std::sync::Mutex<Vec<Tri>>> = runs
        .iter()
        .map(|_| std::sync::Mutex::new(Vec::new()))
        .collect();
    let next = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|s| {
        for _ in 0..threads.min(runs.len()) {
            s.spawn(|| loop {
                let r = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(run) = runs.get(r) else {
                    break;
                };
                let tris = gather(run);
                *done[r]
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = tris;
            });
        }
    });
    let tris: Vec<Tri> = done
        .into_iter()
        .flat_map(|d| {
            d.into_inner()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
        })
        .collect();
    let mut px = vec![[0u8; 4]; w * h];
    // Bands of rows, each with the triangles that reach it, in order; filled a band a thread at a
    // time, as many threads as the machine has.
    const ROWS: usize = 16;
    let bands = h.div_ceil(ROWS);
    let mut reach: Vec<Vec<u32>> = vec![Vec::new(); bands];
    for (i, t) in tris.iter().enumerate() {
        let (lo, hi) = (
            t.p[0].1.min(t.p[1].1).min(t.p[2].1),
            t.p[0].1.max(t.p[1].1).max(t.p[2].1),
        );
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let row = |v: i64| (v.div_euclid(256).clamp(0, h as i64 - 1) as usize) / ROWS;
        if hi < 0 || lo.div_euclid(256) >= h as i64 {
            continue;
        }
        for r in &mut reach[row(lo)..=row(hi)] {
            #[allow(clippy::cast_possible_truncation)]
            r.push(i as u32);
        }
    }
    let threads = std::thread::available_parallelism()
        .map_or(1, std::num::NonZero::get)
        .min(bands)
        .max(1);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let work: Vec<std::sync::Mutex<&mut [[u8; 4]]>> =
        px.chunks_mut(ROWS * w).map(std::sync::Mutex::new).collect();
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| loop {
                let b = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(band) = work.get(b) else {
                    break;
                };
                let mut band = band
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                fill_band(&tris, &reach[b], &mut band, w, b * ROWS);
            });
        }
    });
    drop(work);
    px.into_flattened()
}

/// A triangle in an image's pixels (256ths), wound so that its inside is where every edge is
/// positive, and its corners' straight colors.
struct Tri {
    p: [(i64, i64); 3],
    area: i64,
    c: [[f32; 4]; 3],
    /// Whether the corners are of one color, and that color.
    flat: bool,
    solid: [u8; 4],
}

/// Where `p` is from the edge `a`→`b`: positive inside a triangle wound as [`Tri`]s are.
fn edge(a: (i64, i64), b: (i64, i64), p: (i64, i64)) -> i64 {
    (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0)
}

/// Fill the pixels of `tris` numbered `these` that fall in `band`, the rows of an image `w` wide
/// from row `top`.
fn fill_band(tris: &[Tri], these: &[u32], band: &mut [[u8; 4]], w: usize, top: usize) {
    let h = band.len() / w;
    // A pixel whose middle is on an edge belongs to the triangle the edge is the top or the
    // left of, so that two triangles sharing it never both fill it.
    let top_left = |a: (i64, i64), b: (i64, i64)| {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        dy < 0 || (dy == 0 && dx > 0)
    };
    #[allow(clippy::cast_possible_wrap)]
    let (top, h, w) = (top as i64, h as i64, w as i64);
    for t in these.iter().map(|i| &tris[*i as usize]) {
        let [pa, pb, pc] = t.p;
        // The pixels whose middles the triangle's box holds, in this band.
        let span =
            |lo: i64, hi: i64| ((lo - 128 + 255).div_euclid(256), (hi - 128).div_euclid(256));
        let (x0, x1) = span(pa.0.min(pb.0).min(pc.0), pa.0.max(pb.0).max(pc.0));
        let (y0, y1) = span(pa.1.min(pb.1).min(pc.1), pa.1.max(pb.1).max(pc.1));
        let (x0, x1) = (x0.max(0), x1.min(w - 1));
        let (y0, y1) = (y0.max(top), y1.min(top + h - 1));
        if x0 > x1 || y0 > y1 {
            continue;
        }
        // Each edge's value at the first pixel, and its change a pixel across and down; ties
        // given away by one less.
        let pairs = [(pb, pc), (pc, pa), (pa, pb)];
        let bias = pairs.map(|(a, b)| i64::from(!top_left(a, b)));
        let edges = pairs.map(|(a, b)| {
            (
                edge(a, b, (x0 * 256 + 128, y0 * 256 + 128)),
                -(b.1 - a.1) * 256,
                (b.0 - a.0) * 256,
            )
        });
        #[allow(clippy::cast_precision_loss)]
        let inv = 1.0 / t.area as f32;
        let mut row = [
            edges[0].0 - bias[0],
            edges[1].0 - bias[1],
            edges[2].0 - bias[2],
        ];
        for y in y0..=y1 {
            let mut e = row;
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
            let line = ((y - top) * w) as usize;
            for x in x0..=x1 {
                if e[0] >= 0 && e[1] >= 0 && e[2] >= 0 {
                    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                    let at = line + x as usize;
                    if t.flat && t.solid[3] == 255 {
                        // Opaque: what is under it does not show.
                        band[at] = t.solid;
                        for k in 0..3 {
                            e[k] += edges[k].1;
                        }
                        continue;
                    }
                    let src = if t.flat {
                        t.c[0]
                    } else {
                        // The bias taken back out, for the weights.
                        #[allow(clippy::cast_precision_loss)]
                        let wt = |k: usize| (e[k] + bias[k]) as f32 * inv;
                        let (wa, wb, wc) = (wt(0), wt(1), wt(2));
                        std::array::from_fn(|i| t.c[0][i] * wa + t.c[1][i] * wb + t.c[2][i] * wc)
                    };
                    let dst = &mut band[at];
                    let alpha = src[3];
                    // The device blends in float and keeps eight bits a channel between draws.
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let blend =
                        |s: f32, d: u8| (s * 255.0 + f32::from(d) * (1.0 - alpha) + 0.5) as u8;
                    for i in 0..3 {
                        dst[i] = blend(src[i] * alpha, dst[i]);
                    }
                    dst[3] = blend(alpha, dst[3]);
                }
                for k in 0..3 {
                    e[k] += edges[k].1;
                }
            }
            for k in 0..3 {
                row[k] += edges[k].2;
            }
        }
    }
}

/// The panels' scales a creator chooses from, as pixels to the point.
pub const SCALES: [f32; 8] = [0.75, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0];

/// The scale in [`SCALES`] nearest `scale`.
#[must_use]
pub fn nearest_scale(scale: f32) -> f32 {
    SCALES
        .into_iter()
        .min_by(|a, b| (a - scale).abs().total_cmp(&(b - scale).abs()))
        .unwrap_or(1.0)
}

/// The scale `steps` places along [`SCALES`] from `scale`, held at either end.
#[must_use]
pub fn step_scale(scale: f32, steps: i32) -> f32 {
    let here = SCALES
        .iter()
        .position(|s| *s == nearest_scale(scale))
        .unwrap_or(2);
    let last = SCALES.len() - 1;
    let to = usize::try_from(i64::try_from(here).unwrap_or(0) + i64::from(steps))
        .unwrap_or(0)
        .min(last);
    SCALES[to]
}

/// The panels' scale a settings file `path` keeps (as the nearest of [`SCALES`]), when it keeps
/// one.
#[must_use]
pub fn read_scale(path: &std::path::Path) -> Option<f32> {
    let text = std::fs::read_to_string(path).ok()?;
    let table: toml::Table = toml::from_str(&text).ok()?;
    #[allow(clippy::cast_possible_truncation)] // a scale, between a half and two
    let scale = table.get("ui_scale")?.as_float()? as f32;
    Some(nearest_scale(scale))
}

/// Keep the panels' scale `scale` in the settings file `path`, with the window it keeps.
///
/// # Errors
/// The file would not write.
pub fn write_scale(path: &std::path::Path, scale: f32) -> std::io::Result<()> {
    write_view(path, Some(scale), read_window(path))
}

/// The window's place and size the settings file `path` keeps: its left and top on the desktop,
/// and its width and height, in pixels.
#[must_use]
pub fn read_window(path: &std::path::Path) -> Option<(i32, i32, u32, u32)> {
    let text = std::fs::read_to_string(path).ok()?;
    let table: toml::Table = toml::from_str(&text).ok()?;
    let v: Vec<i64> = table
        .get("window")?
        .as_array()?
        .iter()
        .filter_map(toml::Value::as_integer)
        .collect();
    let [x, y, w, h] = v[..] else {
        return None;
    };
    Some((
        i32::try_from(x).ok()?,
        i32::try_from(y).ok()?,
        u32::try_from(w).ok().filter(|w| *w >= 320)?,
        u32::try_from(h).ok().filter(|h| *h >= 240)?,
    ))
}

/// Keep the window's place and size `window` in the settings file `path`, with the panels' scale
/// it keeps.
///
/// # Errors
/// The file would not write.
pub fn write_window(path: &std::path::Path, window: (i32, i32, u32, u32)) -> std::io::Result<()> {
    write_view(path, read_scale(path), Some(window))
}

fn write_view(
    path: &std::path::Path,
    scale: Option<f32>,
    window: Option<(i32, i32, u32, u32)>,
) -> std::io::Result<()> {
    let mut text = String::new();
    if let Some(scale) = scale {
        text.push_str(&format!(
            "# The size of Lyceum's panels: 1.0 is 100%.\nui_scale = {scale:.2}\n"
        ));
    }
    if let Some((x, y, w, h)) = window {
        text.push_str(&format!(
            "# Where Lyceum's window was left: its left, top, width and height.\nwindow = [{x}, {y}, {w}, {h}]\n"
        ));
    }
    std::fs::write(path, text)
}

impl std::fmt::Debug for Gui {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gui")
            .field("textures", &self.textures.len())
            .finish_non_exhaustive()
    }
}

impl Default for Gui {
    fn default() -> Self {
        Self::new()
    }
}

/// The carried fonts as egui's.
fn fonts() -> egui::FontDefinitions {
    let mut f = egui::FontDefinitions::empty();
    f.font_data
        .insert("sans".into(), Arc::new(egui::FontData::from_static(SANS)));
    f.font_data
        .insert("mono".into(), Arc::new(egui::FontData::from_static(MONO)));
    f.families.insert(
        egui::FontFamily::Proportional,
        vec!["sans".into(), "mono".into()],
    );
    f.families.insert(
        egui::FontFamily::Monospace,
        vec!["mono".into(), "sans".into()],
    );
    f
}

/// The egui key of a window's virtual key.
fn key_of(vk: usize) -> Option<egui::Key> {
    use egui::Key as K;
    Some(match vk {
        0x08 => K::Backspace,
        0x09 => K::Tab,
        0x0D => K::Enter,
        0x1B => K::Escape,
        0x20 => K::Space,
        0x21 => K::PageUp,
        0x22 => K::PageDown,
        0x23 => K::End,
        0x24 => K::Home,
        0x25 => K::ArrowLeft,
        0x26 => K::ArrowUp,
        0x27 => K::ArrowRight,
        0x28 => K::ArrowDown,
        0x2D => K::Insert,
        0x2E => K::Delete,
        0x30..=0x39 | 0x41..=0x5A => {
            let c = char::from(u8::try_from(vk).ok()?);
            K::from_name(&c.to_string())?
        }
        _ => return None,
    })
}

impl Gui {
    #[must_use]
    pub fn new() -> Self {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts());
        ctx.set_visuals(egui::Visuals::dark());
        // The editor's own keys scale the panels (Ctrl+=, Ctrl+- and Ctrl+0).
        ctx.options_mut(|o| o.zoom_with_keyboard = false);
        Self {
            ctx,
            textures: HashMap::new(),
            uploads: Vec::new(),
            modifiers: egui::Modifiers::default(),
            pointer: egui::Pos2::ZERO,
            scale: 1.0,
            last_time: None,
            time_offset: 0.0,
            texture_bytes: 0,
        }
    }

    /// Pixels to the point: how big the panels are drawn.
    #[must_use]
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Draw the panels `scale` pixels to the point from the next pass.
    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale.clamp(SCALES[0], SCALES[SCALES.len() - 1]);
    }

    /// A point of the window, in pixels, as the panels' point.
    #[must_use]
    pub fn to_points(&self, x: f32, y: f32) -> egui::Pos2 {
        egui::pos2(x / self.scale, y / self.scale)
    }

    /// The modifiers held, as the window's events last said.
    #[must_use]
    pub fn modifiers(&self) -> egui::Modifiers {
        self.modifiers
    }

    /// egui's input for one frame of a screen of `size` pixels at `time` seconds: `events`, the
    /// window's, translated.
    pub fn input(&mut self, events: &[HostEvent], size: (u32, u32), time: f64) -> egui::RawInput {
        let mut time = time + self.time_offset;
        if let Some(last) = self.last_time.filter(|last| time < *last) {
            self.time_offset += last - time;
            time = last;
        }
        if let Some(last) = self.last_time.filter(|last| time > *last + LONGEST_STEP) {
            self.time_offset -= time - last - LONGEST_STEP;
            time = last + LONGEST_STEP;
        }
        self.last_time = Some(time);
        let s = self.scale;
        #[allow(clippy::cast_precision_loss)] // a screen
        let screen = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(size.0 as f32 / s, size.1 as f32 / s),
        );
        let mut out = Vec::new();
        for e in events {
            match e {
                HostEvent::CursorMoved { x, y } => {
                    #[allow(clippy::cast_possible_truncation)]
                    let p = egui::pos2(*x as f32 / s, *y as f32 / s);
                    self.pointer = p;
                    out.push(egui::Event::PointerMoved(p));
                }
                HostEvent::CursorLeft => out.push(egui::Event::PointerGone),
                HostEvent::MouseInput { button, pressed } => {
                    let button = match button {
                        MouseButton::Left => egui::PointerButton::Primary,
                        MouseButton::Right => egui::PointerButton::Secondary,
                        MouseButton::Middle => egui::PointerButton::Middle,
                        _ => continue,
                    };
                    out.push(egui::Event::PointerButton {
                        pos: self.pointer,
                        button,
                        pressed: *pressed,
                        modifiers: self.modifiers,
                    });
                }
                HostEvent::MouseWheel { notches } => out.push(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: egui::vec2(0.0, *notches),
                    phase: egui::TouchPhase::Move,
                    modifiers: self.modifiers,
                }),
                HostEvent::ModifiersChanged { alt } => {
                    self.modifiers.alt = *alt;
                    out.push(egui::Event::ModifiersChanged(self.modifiers));
                }
                HostEvent::KeyboardInput { key, pressed, text } => {
                    match key.virtual_key {
                        0x11 | 0xA2 | 0xA3 => {
                            self.modifiers.ctrl = *pressed;
                            self.modifiers.command = *pressed;
                        }
                        0x10 | 0xA0 | 0xA1 => self.modifiers.shift = *pressed,
                        0x12 | 0xA4 | 0xA5 => self.modifiers.alt = *pressed,
                        _ => {}
                    }
                    if matches!(key.virtual_key, 0x10..=0x12 | 0xA0..=0xA5) {
                        out.push(egui::Event::ModifiersChanged(self.modifiers));
                    }
                    if let Some(k) = key_of(key.virtual_key) {
                        out.push(egui::Event::Key {
                            key: k,
                            physical_key: None,
                            pressed: *pressed,
                            repeat: false,
                            modifiers: self.modifiers,
                        });
                    }
                    if *pressed {
                        if let Some(t) = text
                            .as_ref()
                            .filter(|t| !t.is_empty() && t.chars().all(|c| !c.is_control()))
                        {
                            out.push(egui::Event::Text(t.clone()));
                        }
                    }
                }
                HostEvent::Focused(f) => out.push(egui::Event::WindowFocused(*f)),
                _ => {}
            }
        }
        let mut raw = egui::RawInput {
            screen_rect: Some(screen),
            time: Some(time),
            events: out,
            max_texture_side: Some(8192),
            ..egui::RawInput::default()
        };
        if let Some(root) = raw.viewports.get_mut(&raw.viewport_id) {
            root.native_pixels_per_point = Some(s);
        } else {
            raw.viewports.insert(
                raw.viewport_id,
                egui::ViewportInfo {
                    native_pixels_per_point: Some(s),
                    ..egui::ViewportInfo::default()
                },
            );
        }
        raw
    }

    /// The overlay key of egui texture `id`, when it has one.
    #[must_use]
    pub fn texture_key(&self, id: egui::TextureId) -> Option<OverlayTexture> {
        self.textures.get(&id).map(|t| t.key)
    }

    /// egui's output for a screen of `size` pixels: its texture changes queued in
    /// [`Self::uploads`], and its shapes as overlay triangles.
    pub fn paint(&mut self, mut out: egui::FullOutput, size: (u32, u32)) -> Vec<OverlayItem> {
        let set = std::mem::take(&mut out.textures_delta.set);
        let free = std::mem::take(&mut out.textures_delta.free);
        #[allow(clippy::iter_over_hash_type)] // each texture's changes stay in their order
        for (id, deltas) in set {
            for delta in &deltas {
                self.set_texture(id, delta);
            }
        }
        let primitives = self
            .ctx
            .tessellate(std::mem::take(&mut out.shapes), out.pixels_per_point);
        let mut items: Vec<OverlayItem> = Vec::new();
        // The shapes are in points; the screen is `size` pixels.
        #[allow(clippy::cast_precision_loss)] // a screen
        let points = (
            size.0.max(1) as f32 / self.scale,
            size.1.max(1) as f32 / self.scale,
        );
        let mut scratch: Vec<OverlayVertex> = Vec::new();
        for egui::ClippedPrimitive {
            clip_rect,
            primitive,
        } in primitives
        {
            let mesh = match primitive {
                egui::epaint::Primitive::Mesh(mesh) => mesh,
                egui::epaint::Primitive::Callback(cb) => {
                    if let Some(PreviewPaint(space)) = cb.callback.downcast_ref::<PreviewPaint>() {
                        if let Some(rect) = preview_rect(cb.rect, clip_rect, self.scale, size) {
                            items.push(OverlayItem::Preview {
                                space: *space,
                                rect,
                            });
                        }
                    }
                    continue;
                }
            };
            let Some((key, sampler)) = self.texture_of(mesh.texture_id) else {
                continue;
            };
            let vertices = batch(&mut items, key, sampler);
            convert(&mesh, clip_rect, points, vertices, &mut scratch);
            if vertices.is_empty() {
                items.pop();
            }
        }
        #[allow(clippy::iter_over_hash_type)]
        for id in free {
            self.textures.remove(&id);
        }
        items
    }

    /// Take egui's change to texture `id`: a whole image or a patch of one.
    fn set_texture(&mut self, id: egui::TextureId, delta: &egui::epaint::ImageDelta) {
        let egui::ImageData::Color(image) = &delta.image;
        let [w, h] = image.size;
        let mut rgba = Vec::with_capacity(w * h * 4);
        for p in &image.pixels {
            let [r, g, b, a] = unmultiply(*p);
            rgba.extend([r, g, b, a]);
        }
        let linear = delta.options.magnification == egui::TextureFilter::Linear;
        let t = match delta.pos {
            None => {
                let key = self.textures.get(&id).map_or_else(
                    || OverlayTexture {
                        space: OverlaySpace::Local,
                        key: KEY_BASE
                            + match id {
                                egui::TextureId::Managed(m) => m * 2,
                                egui::TextureId::User(u) => u * 2 + 1,
                            },
                    },
                    |t| t.key,
                );
                self.textures.insert(
                    id,
                    Texture {
                        key,
                        size: [w, h],
                        rgba,
                        linear,
                    },
                );
                &self.textures[&id]
            }
            Some([x0, y0]) => {
                let Some(t) = self.textures.get_mut(&id) else {
                    return;
                };
                for row in 0..h {
                    let dst = ((y0 + row) * t.size[0] + x0) * 4;
                    let src = row * w * 4;
                    if dst + w * 4 <= t.rgba.len() {
                        t.rgba[dst..dst + w * 4].copy_from_slice(&rgba[src..src + w * 4]);
                    }
                }
                &self.textures[&id]
            }
        };
        let mut bgra = t.rgba.clone();
        self.texture_bytes += bgra.len() as u64;
        for px in bgra.as_chunks_mut::<4>().0 {
            px.swap(0, 2);
        }
        self.uploads.push((
            t.key,
            TextureData {
                width: u32::try_from(t.size[0]).unwrap_or(0),
                height: u32::try_from(t.size[1]).unwrap_or(0),
                format: TextureFormat::Bgra8,
                levels: vec![bgra],
            },
        ));
    }
}

impl Gui {
    /// The overlay texture and sampler egui's texture `id` is drawn with.
    fn texture_of(&self, id: egui::TextureId) -> Option<(OverlayTexture, OverlaySampler)> {
        let tex = self.textures.get(&id)?;
        Some((
            tex.key,
            OverlaySampler {
                point: !tex.linear,
                wrap_u: false,
                wrap_v: false,
            },
        ))
    }
}

/// A paint callback that draws a preview space where egui lays it out: the space is drawn there,
/// in egui's order, over what egui drew before it and under what it draws after.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreviewPaint(pub PreviewSpace);

/// Where `rect` (in points, cut to `clip`) is on a screen of `size` pixels, `scale` pixels to the
/// point; `None` when its clip cuts it, as a preview cannot be drawn in part.
fn preview_rect(
    rect: egui::Rect,
    clip: egui::Rect,
    scale: f32,
    size: (u32, u32),
) -> Option<dereth_primitives::Viewport> {
    const SLACK: f32 = 0.5;
    if rect.width() < 1.0
        || rect.height() < 1.0
        || rect.min.x < clip.min.x - SLACK
        || rect.min.y < clip.min.y - SLACK
        || rect.max.x > clip.max.x + SLACK
        || rect.max.y > clip.max.y + SLACK
    {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // on screen, in pixels
    let px = |v: f32| (v * scale).round().max(0.0) as u32;
    let (x, y) = (px(rect.min.x), px(rect.min.y));
    let (w, h) = (
        px(rect.max.x).min(size.0).saturating_sub(x),
        px(rect.max.y).min(size.1).saturating_sub(y),
    );
    (w > 0 && h > 0).then_some(dereth_primitives::Viewport {
        x,
        y,
        width: w,
        height: h,
    })
}

/// The last batch of `items` when it draws with `key` and `sampler`, else a new one.
fn batch(
    items: &mut Vec<OverlayItem>,
    key: OverlayTexture,
    sampler: OverlaySampler,
) -> &mut Vec<OverlayVertex> {
    let joins = matches!(
        items.last(),
        Some(OverlayItem::Triangles { texture, sampler: s, .. })
            if *texture == key && *s == sampler
    );
    if !joins {
        items.push(OverlayItem::Triangles {
            material: OverlayMaterial::Image,
            texture: key,
            sampler,
            vertices: Vec::new(),
            depth: dereth_client_contract::overlay::OverlayDepth::Over,
        });
    }
    match items.last_mut() {
        Some(OverlayItem::Triangles { vertices, .. }) => vertices,
        _ => unreachable!("a batch of triangles was just made"),
    }
}

/// `mesh`'s triangles cut to `clip_rect`, onto `out` in clip space for a screen `points` (w, h)
/// points in size: each wound counter-clockwise, its colors straight.
fn convert(
    mesh: &egui::epaint::Mesh,
    clip_rect: egui::Rect,
    points: (f32, f32),
    out: &mut Vec<OverlayVertex>,
    scratch: &mut Vec<OverlayVertex>,
) {
    let (w, h) = points;
    let to_clip = |p: egui::Pos2| [p.x / w * 2.0 - 1.0, 1.0 - p.y / h * 2.0, 0.5];
    // Runs of vertices share a color: the last one's straight form is kept.
    let mut last = (egui::Color32::TRANSPARENT, 0);
    let mut v = move |x: &egui::epaint::Vertex| {
        if x.color != last.0 {
            last = (x.color, straight(x.color));
        }
        OverlayVertex {
            position: to_clip(x.pos),
            color: last.1,
            uv: [x.uv.x, x.uv.y],
        }
    };
    // Counter-clockwise in clip space (y up), the overlay's front face.
    let wound = |a: OverlayVertex, b: OverlayVertex, c: OverlayVertex| {
        let turn = (b.position[0] - a.position[0]) * (c.position[1] - a.position[1])
            - (b.position[1] - a.position[1]) * (c.position[0] - a.position[0]);
        if turn >= 0.0 {
            [a, b, c]
        } else {
            [a, c, b]
        }
    };
    out.reserve(mesh.indices.len());
    let triangles = mesh.indices.as_chunks::<3>().0;
    if mesh.vertices.iter().all(|x| clip_rect.contains(x.pos)) {
        // Inside its clip rectangle whole: each vertex worked out once, however many triangles
        // share it.
        scratch.clear();
        scratch.extend(mesh.vertices.iter().map(&mut v));
        for t in triangles {
            let at = |i: u32| scratch[i as usize];
            out.extend(wound(at(t[0]), at(t[1]), at(t[2])));
        }
    } else {
        for t in triangles {
            let tri = [
                mesh.vertices[t[0] as usize],
                mesh.vertices[t[1] as usize],
                mesh.vertices[t[2] as usize],
            ];
            if tri.iter().all(|x| clip_rect.contains(x.pos)) {
                out.extend(wound(v(&tri[0]), v(&tri[1]), v(&tri[2])));
                continue;
            }
            for poly in clip(&tri, clip_rect) {
                out.extend(wound(v(&poly[0]), v(&poly[1]), v(&poly[2])));
            }
        }
    }
}

/// A premultiplied colour's straight-alpha channels.
fn unmultiply(c: egui::Color32) -> [u8; 4] {
    let a = c.a();
    if a == 0 {
        return [0, 0, 0, 0];
    }
    let f =
        |v: u8| u8::try_from((u32::from(v) * 255 + u32::from(a) / 2) / u32::from(a)).unwrap_or(255);
    [f(c.r()), f(c.g()), f(c.b()), a]
}

/// A premultiplied vertex colour as the overlay takes it: straight alpha, packed `0xAARRGGBB`.
fn straight(c: egui::Color32) -> u32 {
    let [r, g, b, a] = unmultiply(c);
    u32::from_be_bytes([a, r, g, b])
}

/// One coordinate of a point.
type Axis = fn(egui::Pos2) -> f32;

/// Triangle `t` cut to `rect`: the triangles of what is left (none when it is outside).
fn clip(t: &[egui::epaint::Vertex; 3], rect: egui::Rect) -> Vec<[egui::epaint::Vertex; 3]> {
    let inside = |v: &egui::epaint::Vertex| rect.contains(v.pos);
    if t.iter().all(inside) {
        return vec![*t];
    }
    // Sutherland-Hodgman against the four edges, each vertex's colour and texture coordinates
    // carried along the edge it is cut on.
    let mut poly: Vec<egui::epaint::Vertex> = t.to_vec();
    let edges: [(Axis, f32, bool); 4] = [
        (|p| p.x, rect.min.x, true),
        (|p| p.x, rect.max.x, false),
        (|p| p.y, rect.min.y, true),
        (|p| p.y, rect.max.y, false),
    ];
    for (axis, bound, keep_above) in edges {
        if poly.is_empty() {
            break;
        }
        let keep = |v: &egui::epaint::Vertex| {
            if keep_above {
                axis(v.pos) >= bound
            } else {
                axis(v.pos) <= bound
            }
        };
        let mut next = Vec::with_capacity(poly.len() + 2);
        for i in 0..poly.len() {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            let (ka, kb) = (keep(&a), keep(&b));
            if ka {
                next.push(a);
            }
            if ka != kb {
                let (da, db) = (axis(a.pos) - bound, axis(b.pos) - bound);
                let s = da / (da - db);
                next.push(lerp(&a, &b, s));
            }
        }
        poly = next;
    }
    (1..poly.len().saturating_sub(1))
        .map(|i| [poly[0], poly[i], poly[i + 1]])
        .collect()
}

fn lerp(a: &egui::epaint::Vertex, b: &egui::epaint::Vertex, s: f32) -> egui::epaint::Vertex {
    let c = |x: u8, y: u8| {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let v = (f32::from(x) + (f32::from(y) - f32::from(x)) * s).round() as u8;
        v
    };
    egui::epaint::Vertex {
        pos: a.pos + (b.pos - a.pos) * s,
        uv: a.uv + (b.uv - a.uv) * s,
        color: egui::Color32::from_rgba_premultiplied(
            c(a.color.r(), b.color.r()),
            c(a.color.g(), b.color.g()),
            c(a.color.b(), b.color.b()),
            c(a.color.a(), b.color.a()),
        ),
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's panels, drawn through the overlay)
    use super::*;

    fn v(x: f32, y: f32) -> egui::epaint::Vertex {
        egui::epaint::Vertex {
            pos: egui::pos2(x, y),
            uv: egui::pos2(x / 100.0, y / 100.0),
            color: egui::Color32::from_rgba_premultiplied(100, 50, 0, 200),
        }
    }

    #[test]
    fn a_triangle_is_cut_to_its_clip_rectangle_and_one_outside_goes() {
        let rect = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(50.0, 50.0));
        let inside = [v(10.0, 10.0), v(40.0, 10.0), v(10.0, 40.0)];
        assert_eq!(clip(&inside, rect).len(), 1);
        let outside = [v(60.0, 60.0), v(90.0, 60.0), v(60.0, 90.0)];
        assert!(clip(&outside, rect).is_empty());
        let across = [v(10.0, 10.0), v(90.0, 10.0), v(10.0, 90.0)];
        let cut = clip(&across, rect);
        assert!(!cut.is_empty());
        for t in &cut {
            for p in t {
                assert!(rect.expand(1e-3).contains(p.pos), "{:?}", p.pos);
                // The texture coordinates follow the position along the cut edge.
                assert!((p.uv.x - p.pos.x / 100.0).abs() < 1e-4);
            }
        }
        // What is left is the part of the triangle inside: the square from 10 to 50 either way,
        // whose far corner lies on the cut edge.
        let area: f32 = cut
            .iter()
            .map(|t| {
                let (a, b, c) = (t[0].pos, t[1].pos, t[2].pos);
                ((b - a).x * (c - a).y - (b - a).y * (c - a).x).abs() / 2.0
            })
            .sum();
        assert!((area - 1600.0).abs() < 1.0, "{area}");
    }

    #[test]
    fn at_a_scale_the_windows_pixels_reach_egui_as_points_and_its_shapes_fill_the_same_pixels() {
        let mut g = Gui::new();
        g.set_scale(2.0);
        let input = g.input(
            &[HostEvent::CursorMoved { x: 200.0, y: 100.0 }],
            (800, 600),
            1.0,
        );
        assert_eq!(
            input.screen_rect,
            Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, 300.0)
            ))
        );
        assert!(input
            .events
            .iter()
            .any(|e| matches!(e, egui::Event::PointerMoved(p) if *p == egui::pos2(100.0, 50.0))));
        // A rectangle over the right half of the screen in points covers the right half of the
        // window in pixels.
        let ctx = g.ctx.clone();
        let out = ctx.run_ui(input, |ui| {
            ui.painter().rect_filled(
                egui::Rect::from_min_max(egui::pos2(200.0, 0.0), egui::pos2(400.0, 300.0)),
                0.0,
                egui::Color32::WHITE,
            );
        });
        assert!((out.pixels_per_point - 2.0).abs() < 1e-6);
        let items = g.paint(out, (800, 600));
        let xs: Vec<f32> = items
            .iter()
            .flat_map(|i| match i {
                OverlayItem::Triangles { vertices, .. } => {
                    vertices.iter().map(|v| v.position[0]).collect::<Vec<_>>()
                }
                OverlayItem::Preview { .. } => Vec::new(),
            })
            .collect();
        let (lo, hi) = xs
            .iter()
            .fold((f32::MAX, f32::MIN), |(lo, hi), x| (lo.min(*x), hi.max(*x)));
        assert!(
            (lo - 0.0).abs() < 0.01 && (hi - 1.0).abs() < 0.01,
            "{lo} {hi}"
        );
    }

    #[test]
    fn a_preview_is_drawn_where_it_is_laid_out_in_order_and_not_at_all_when_its_clip_cuts_it() {
        let mut g = Gui::new();
        g.set_scale(2.0);
        let input = g.input(&[], (800, 600), 1.0);
        let ctx = g.ctx.clone();
        let paint = std::sync::Arc::new(PreviewPaint(PreviewSpace::Tool));
        let out = ctx.run_ui(input, |ui| {
            let p = ui.painter();
            p.rect_filled(
                egui::Rect::from_min_max(egui::pos2(10.0, 10.0), egui::pos2(110.0, 110.0)),
                0.0,
                egui::Color32::BLACK,
            );
            p.add(egui::PaintCallback {
                rect: egui::Rect::from_min_max(egui::pos2(10.0, 10.0), egui::pos2(110.0, 60.0)),
                callback: paint.clone(),
            });
            p.rect_filled(
                egui::Rect::from_min_max(egui::pos2(0.0, 100.0), egui::pos2(50.0, 150.0)),
                0.0,
                egui::Color32::WHITE,
            );
            // Cut by its clip: not drawn.
            p.with_clip_rect(egui::Rect::from_min_max(
                egui::pos2(0.0, 0.0),
                egui::pos2(200.0, 200.0),
            ))
            .add(egui::PaintCallback {
                rect: egui::Rect::from_min_max(egui::pos2(150.0, 150.0), egui::pos2(250.0, 250.0)),
                callback: paint.clone(),
            });
        });
        let items = g.paint(out, (800, 600));
        let kinds: Vec<&str> = items
            .iter()
            .map(|i| match i {
                OverlayItem::Triangles { .. } => "triangles",
                OverlayItem::Preview { .. } => "preview",
            })
            .collect();
        assert_eq!(kinds, ["triangles", "preview", "triangles"]);
        assert_eq!(
            items[1],
            OverlayItem::Preview {
                space: PreviewSpace::Tool,
                rect: dereth_primitives::Viewport {
                    x: 20,
                    y: 20,
                    width: 200,
                    height: 100
                }
            },
            "in pixels"
        );
    }

    #[test]
    fn panels_inside_their_clip_and_across_its_edge_give_the_triangles_each_cut_alone_gives() {
        let mut g = Gui::new();
        let input = g.input(&[], (400, 300), 1.0);
        let ctx = g.ctx.clone();
        let bounds = egui::Rect::from_min_max(egui::pos2(50.0, 50.0), egui::pos2(250.0, 200.0));
        let out = ctx.run_ui(input, |ui| {
            let p = ui.painter().with_clip_rect(bounds);
            // Inside whole, and across the clip's right edge.
            p.circle_filled(
                egui::pos2(120.0, 120.0),
                30.0,
                egui::Color32::from_rgb(200, 40, 40),
            );
            p.rect_filled(
                egui::Rect::from_min_max(egui::pos2(200.0, 80.0), egui::pos2(320.0, 150.0)),
                4.0,
                egui::Color32::from_rgba_unmultiplied(40, 200, 40, 128),
            );
        });
        // A corner to a hundredth of a point: back from clip space it may differ in its last bit.
        #[allow(clippy::cast_possible_truncation)]
        let at = |x: f32, y: f32| [(x * 100.0).round() as i64, (y * 100.0).round() as i64];
        // What each triangle gives cut alone, worked out from the same shapes.
        let primitives = ctx.tessellate(out.shapes.clone(), out.pixels_per_point);
        let mut want = Vec::new();
        for p in &primitives {
            let egui::epaint::Primitive::Mesh(mesh) = &p.primitive else {
                continue;
            };
            for t in mesh.indices.as_chunks::<3>().0 {
                let tri = [
                    mesh.vertices[t[0] as usize],
                    mesh.vertices[t[1] as usize],
                    mesh.vertices[t[2] as usize],
                ];
                for poly in clip(&tri, p.clip_rect) {
                    let mut c: Vec<[i64; 2]> = poly.iter().map(|v| at(v.pos.x, v.pos.y)).collect();
                    c.sort_unstable();
                    want.push((c, straight(poly[0].color)));
                }
            }
        }
        let items = g.paint(out, (400, 300));
        let mut got = Vec::new();
        for i in &items {
            if let OverlayItem::Triangles { vertices, .. } = i {
                for t in vertices.as_chunks::<3>().0 {
                    let mut c: Vec<[i64; 2]> = t
                        .iter()
                        .map(|v| {
                            at(
                                (v.position[0] + 1.0) / 2.0 * 400.0,
                                (1.0 - v.position[1]) / 2.0 * 300.0,
                            )
                        })
                        .collect();
                    c.sort_unstable();
                    got.push((c, t[0].color));
                }
            }
        }
        assert!(want.len() > 20, "{}", want.len());
        assert_eq!(got, want);
    }

    #[test]
    fn a_picture_fills_each_pixel_of_two_triangles_sharing_an_edge_once() {
        // A square, half see-through, as two triangles along its diagonal and its edges on the
        // pixels' middles: no pixel is covered twice, none is missed.
        let color = egui::Color32::from_rgba_unmultiplied(200, 100, 0, 128);
        let mut mesh = egui::epaint::Mesh::default();
        for (x, y) in [(2.5, 2.5), (12.5, 2.5), (12.5, 12.5), (2.5, 12.5)] {
            mesh.colored_vertex(egui::pos2(x, y), color);
        }
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        let prim = egui::ClippedPrimitive {
            clip_rect: egui::Rect::EVERYTHING,
            primitive: egui::epaint::Primitive::Mesh(mesh),
        };
        let rgba = rasterize(&[prim], 1.0, (0.0, 0.0), [16, 16]);
        let at = |x: usize, y: usize| &rgba[(y * 16 + x) * 4..(y * 16 + x) * 4 + 4];
        let once = at(7, 7).to_vec();
        assert_eq!(once[3], 128, "{once:?}");
        let covered = (0..16)
            .flat_map(|y| (0..16).map(move |x| (x, y)))
            .filter(|(x, y)| at(*x, *y)[3] != 0)
            .inspect(|(x, y)| assert_eq!(at(*x, *y), &once[..], "({x}, {y})"))
            .count();
        assert_eq!(covered, 100);
    }

    #[test]
    fn a_press_and_its_release_either_side_of_a_slow_frame_are_still_a_click() {
        let mut g = Gui::new();
        let ctx = g.ctx.clone();
        let mut clicked = false;
        let button = |ui: &mut egui::Ui, clicked: &mut bool| {
            let r = ui.put(
                egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(80.0, 30.0)),
                egui::Button::new("Go"),
            );
            *clicked |= r.clicked();
        };
        let press = |pressed| HostEvent::MouseInput {
            button: MouseButton::Left,
            pressed,
        };
        let frames: [(f64, Vec<HostEvent>); 4] = [
            (1.0, vec![HostEvent::CursorMoved { x: 40.0, y: 20.0 }]),
            (1.02, vec![press(true)]),
            // Five seconds later, as after a long load.
            (6.02, vec![press(false)]),
            (6.04, vec![]),
        ];
        for (t, events) in frames {
            let input = g.input(&events, (400, 300), t);
            let out = ctx.run_ui(input, |ui| button(ui, &mut clicked));
            let _ = g.paint(out, (400, 300));
        }
        assert!(clicked);
    }

    #[test]
    fn the_scale_chosen_is_kept_and_read_back_and_a_file_without_one_keeps_none() {
        let dir = std::env::temp_dir().join(format!("lyceum-scale-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("lyceum-view.toml");
        assert_eq!(read_scale(&path), None);
        write_scale(&path, 1.25).unwrap();
        assert_eq!(read_scale(&path), Some(1.25));
        // The window kept beside it, and each kept when the other is written.
        write_window(&path, (-8, 30, 1600, 900)).unwrap();
        assert_eq!(read_window(&path), Some((-8, 30, 1600, 900)));
        assert_eq!(read_scale(&path), Some(1.25));
        write_scale(&path, 1.5).unwrap();
        assert_eq!(read_window(&path), Some((-8, 30, 1600, 900)));
        write_scale(&path, 1.25).unwrap();
        std::fs::write(&path, "ui_scale = 1.3\n").unwrap();
        assert_eq!(read_scale(&path), Some(1.25), "the nearest of the scales");
        std::fs::write(&path, "something = 1\n").unwrap();
        assert_eq!(read_scale(&path), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_scales_a_creator_steps_through_stop_at_either_end() {
        assert!((nearest_scale(1.3) - 1.25).abs() < 1e-6);
        assert!((nearest_scale(3.0) - 2.0).abs() < 1e-6);
        assert!((step_scale(1.0, 1) - 1.1).abs() < 1e-6);
        assert!((step_scale(1.0, -1) - 0.9).abs() < 1e-6);
        assert!((step_scale(2.0, 1) - 2.0).abs() < 1e-6);
        assert!((step_scale(0.75, -3) - 0.75).abs() < 1e-6);
        let mut g = Gui::new();
        g.set_scale(9.0);
        assert!((g.scale() - 2.0).abs() < 1e-6);
    }

    #[test]
    fn a_premultiplied_colour_is_given_straight() {
        assert_eq!(
            straight(egui::Color32::from_rgba_premultiplied(100, 50, 0, 200)),
            0xC880_4000
        );
        assert_eq!(straight(egui::Color32::TRANSPARENT), 0);
        assert_eq!(unmultiply(egui::Color32::WHITE), [255, 255, 255, 255]);
    }

    #[test]
    fn a_window_key_and_its_text_reach_egui_with_the_modifiers_held() {
        let mut g = Gui::new();
        let key = |vk, pressed, text: Option<&str>| HostEvent::KeyboardInput {
            key: dereth_input::keys::Key::new(vk, 0),
            pressed,
            text: text.map(str::to_owned),
        };
        let input = g.input(
            &[
                key(0x11, true, None),
                key(0x53, true, Some("\u{13}")),
                key(0x41, true, Some("a")),
                HostEvent::CursorMoved { x: 5.0, y: 6.0 },
                HostEvent::MouseInput {
                    button: MouseButton::Left,
                    pressed: true,
                },
            ],
            (800, 600),
            1.0,
        );
        let keys: Vec<(egui::Key, bool)> = input
            .events
            .iter()
            .filter_map(|e| match e {
                egui::Event::Key { key, modifiers, .. } => Some((*key, modifiers.ctrl)),
                _ => None,
            })
            .collect();
        assert_eq!(keys, [(egui::Key::S, true), (egui::Key::A, true)]);
        // Ctrl+S's text is a control character, which no field types.
        let texts: Vec<&egui::Event> = input
            .events
            .iter()
            .filter(|e| matches!(e, egui::Event::Text(_)))
            .collect();
        assert_eq!(texts, [&egui::Event::Text("a".into())]);
        assert!(input.events.iter().any(|e| matches!(
            e,
            egui::Event::PointerButton { pos, pressed: true, .. } if *pos == egui::pos2(5.0, 6.0)
        )));
    }
}
