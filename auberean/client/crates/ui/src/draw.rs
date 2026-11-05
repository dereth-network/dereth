//! The interface drawn to a picture as the original composes it: each top-level element into a
//! layer of its own, its elements' pictures and text in order (an element, then its children from
//! the deepest), and the layers over the scene from the deepest, each blended by its alpha.
//!
//! A picture is drawn once when it is the element's size, stretched over the element when the
//! element asks for that (`size_image_to_owner`), and otherwise repeated across it. Media items
//! of draw mode 1 and 2 blend the picture over the layer by its alpha, keeping the layer's alpha;
//! modes 0 and 3 copy its texels, alpha included.
//!
//! A top-level element marked `no_surface_object` has no layer: it is never drawn.

use crate::source::Image;
use crate::system::{Handle, Ui};
use crate::token::{attr, kind};
use crate::Rect;
use dereth_primitives::num::{floor_to_i32, to_i32};

/// A picture in memory: `width * height` texels of blue, green, red, alpha.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Surface {
    pub width: i32,
    pub height: i32,
    pub bgra: Vec<u8>,
}

impl Surface {
    /// A transparent picture.
    #[must_use]
    pub fn new(width: i32, height: i32) -> Self {
        let n = usize::try_from(width.max(0) * height.max(0)).unwrap_or(0) * 4;
        Self {
            width,
            height,
            bgra: vec![0; n],
        }
    }

    fn index(&self, x: i32, y: i32) -> Option<usize> {
        (x >= 0 && y >= 0 && x < self.width && y < self.height)
            .then(|| usize::try_from((y * self.width + x) * 4).unwrap_or(0))
    }

    /// The texel at `(x, y)`, or transparent black off the picture.
    #[must_use]
    pub fn get(&self, x: i32, y: i32) -> [u8; 4] {
        self.index(x, y).map_or([0; 4], |i| {
            [
                self.bgra[i],
                self.bgra[i + 1],
                self.bgra[i + 2],
                self.bgra[i + 3],
            ]
        })
    }

    pub fn put(&mut self, x: i32, y: i32, p: [u8; 4]) {
        if let Some(i) = self.index(x, y) {
            self.bgra[i..i + 4].copy_from_slice(&p);
        }
    }

    /// Make every texel transparent black.
    pub fn clear(&mut self) {
        self.bgra.fill(0);
    }

    /// Make the texels of `area` transparent black.
    fn clear_area(&mut self, area: Rect) {
        let Some(c) = area.intersect(self.bounds()) else {
            return;
        };
        for y in c.y0..=c.y1 {
            let (from, to) = self.row(y, c.x0, c.x1);
            self.bgra[from..to].fill(0);
        }
    }

    /// Where the texels of row `y` from column `x0` to `x1` lie, all of them on the picture.
    fn row(&self, y: i32, x0: i32, x1: i32) -> (usize, usize) {
        let at = |x: i32| usize::try_from((y * self.width + x) * 4).unwrap_or(0);
        (at(x0), at(x1 + 1))
    }

    /// The whole picture.
    #[must_use]
    pub const fn bounds(&self) -> Rect {
        Rect::sized(0, 0, self.width, self.height)
    }
}

/// How a picture's texels go onto the layer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Blit {
    /// Copied outright, alpha included.
    Copy,
    /// Copied, the alpha then scaled by the element's own transparency.
    CopyFaded(f32),
    /// Blended over the layer by its alpha, the layer keeping its alpha.
    Blend,
}

impl Blit {
    /// How a media item's draw mode puts its picture down, at the element's transparency.
    #[must_use]
    pub fn of_mode(mode: u32, alpha: f32) -> Self {
        match mode {
            1 | 2 => Self::Blend,
            3 => Self::CopyFaded(alpha),
            _ => Self::Copy,
        }
    }
}

/// The window `r` (inclusive) opens onto a picture `width` by `height`, pressed onto it: its left
/// and top edges held at 0 or more, its right and bottom edges at the picture's at most and at
/// its first column and row at least, and a window left with no width or height kept one texel
/// wide or high inside its far edge. A window reaching off the picture paints the edge it reaches
/// past.
// D2-09947, D2-09948, D2-09955, D2-09956, D2-09957
#[must_use]
pub fn window(r: Rect, width: i32, height: i32) -> Rect {
    let (mut x0, mut y0, mut x1, mut y1) = (r.x0, r.y0, r.x1 + 1, r.y1 + 1);
    x0 = x0.max(0);
    y0 = y0.max(0);
    x1 = x1.min(width).max(1);
    y1 = y1.min(height).max(1);
    if x0 >= x1 {
        x0 = x1 - 1;
    }
    if y0 >= y1 {
        y0 = y1 - 1;
    }
    Rect::new(x0, y0, x1 - 1, y1 - 1)
}

/// One source texel onto one layer texel.
// D2-09960
// The original truncates the faded alpha toward zero.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
#[inline]
fn blit_one(d: &mut [u8; 4], s: [u8; 4], how: Blit) {
    let blend = match how {
        Blit::Copy => false,
        Blit::CopyFaded(a) => {
            let faded = (f32::from(s[3]) * a.clamp(0.0, 1.0)) as u8;
            *d = [s[0], s[1], s[2], faded];
            return;
        }
        Blit::Blend => true,
    };
    if !blend || s[3] == 0xFF {
        if !blend || s[3] != 0 {
            *d = s;
        }
        return;
    }
    if s[3] == 0 {
        return;
    }
    let a = i32::from(s[3]);
    let mix = |dc: u8, sc: u8| -> u8 {
        let dc = i32::from(dc);
        u8::try_from(dc - (dc - i32::from(sc)) * a / 256).unwrap_or(0)
    };
    *d = [mix(d[0], s[0]), mix(d[1], s[1]), mix(d[2], s[2]), d[3]];
}

/// One source texel onto the layer at `(x, y)`, when that is on the layer.
fn blit_texel(dst: &mut Surface, x: i32, y: i32, s: [u8; 4], how: Blit) {
    if let Some(i) = dst.index(x, y) {
        if let Some((d, _)) = dst.bgra[i..].split_first_chunk_mut::<4>() {
            blit_one(d, s, how);
        }
    }
}

/// A run of source texels onto a run of layer texels, as many as both have.
#[inline]
fn blit_run(dst: &mut [u8], src: &[u8], how: Blit) {
    let (to, _) = dst.as_chunks_mut::<4>();
    let (from, _) = src.as_chunks::<4>();
    match how {
        Blit::Copy => {
            let n = to.len().min(from.len()) * 4;
            dst[..n].copy_from_slice(&src[..n]);
        }
        Blit::Blend => to
            .iter_mut()
            .zip(from)
            .for_each(|(d, s)| blit_one(d, *s, Blit::Blend)),
        Blit::CopyFaded(_) => to
            .iter_mut()
            .zip(from)
            .for_each(|(d, s)| blit_one(d, *s, how)),
    }
}

/// Draw a picture over an element's rectangle `area`, clipped to `clip`: once at its size; else
/// once in the middle (`center`), stretched over the element (`stretch`), or repeated across it.
// D2-09624, D2-09619
pub fn draw_image(
    dst: &mut Surface,
    img: &Image,
    area: Rect,
    clip: Rect,
    how: Blit,
    stretch: bool,
    center: bool,
) {
    let (iw, ih) = (
        i32::try_from(img.width).unwrap_or(0),
        i32::try_from(img.height).unwrap_or(0),
    );
    if iw <= 0 || ih <= 0 {
        return;
    }
    let Some(c) = area.intersect(clip).and_then(|c| c.intersect(dst.bounds())) else {
        return;
    };
    // Where column `x` of row `y` of the picture lies.
    let at = |x: i32, y: i32| usize::try_from((y * iw + x) * 4).unwrap_or(0);
    let (bw, bh) = (area.width(), area.height());
    if center && !(iw == bw && ih == bh) {
        // Once, its top left at half the room left over (none when it is larger).
        let x0 = area.x0 + ((bw - iw) / 2).max(0);
        let y0 = area.y0 + ((bh - ih) / 2).max(0);
        let Some(c) = Rect::new(x0, y0, x0 + iw - 1, y0 + ih - 1).intersect(c) else {
            return;
        };
        for y in c.y0..=c.y1 {
            let (from, to) = dst.row(y, c.x0, c.x1);
            let start = at(c.x0 - x0, y - y0);
            blit_run(&mut dst.bgra[from..to], &img.bgra[start..], how);
        }
        return;
    }
    if (iw == bw && ih == bh) || !stretch {
        // Once at its size, or repeated across the element from its top left.
        for y in c.y0..=c.y1 {
            let sy = (y - area.y0).rem_euclid(ih);
            let mut x = c.x0;
            while x <= c.x1 {
                let sx = (x - area.x0).rem_euclid(iw);
                let last = (x + iw - sx - 1).min(c.x1);
                let (from, to) = dst.row(y, x, last);
                let start = at(sx, sy);
                blit_run(&mut dst.bgra[from..to], &img.bgra[start..], how);
                x = last + 1;
            }
        }
        return;
    }
    let columns: Vec<usize> = (c.x0..=c.x1)
        .map(|x| {
            let sx = i64::from(x - area.x0) * i64::from(iw) / i64::from(bw);
            usize::try_from(sx).unwrap_or(0).min(img.width as usize - 1)
        })
        .collect();
    for y in c.y0..=c.y1 {
        let sy = u32::try_from(i64::from(y - area.y0) * i64::from(ih) / i64::from(bh))
            .unwrap_or(0)
            .min(img.height - 1) as usize;
        let row = &img.bgra[sy * img.width as usize * 4..];
        let (from, to) = dst.row(y, c.x0, c.x1);
        let (run, _) = dst.bgra[from..to].as_chunks_mut::<4>();
        for (d, sx) in run.iter_mut().zip(&columns) {
            let i = sx * 4;
            blit_one(d, [row[i], row[i + 1], row[i + 2], row[i + 3]], how);
        }
    }
}

/// The part of a layer anything has been drawn in so far.
#[derive(Clone, Copy, Debug, Default)]
struct Drawn(Option<Rect>);

impl Drawn {
    fn add(&mut self, r: Rect) {
        self.0 = Some(match self.0 {
            Some(d) => Rect::new(
                d.x0.min(r.x0),
                d.y0.min(r.y0),
                d.x1.max(r.x1),
                d.y1.max(r.y1),
            ),
            None => r,
        });
    }
}

/// Draw one element and what is under it into a layer.
// D2-11176, D2-11169
#[allow(clippy::too_many_arguments)]
fn draw_element(
    ui: &Ui,
    layer: &mut Surface,
    h: Handle,
    ox: i32,
    oy: i32,
    parent_clip: Rect,
    drawn: &mut Drawn,
) {
    let Some(n) = ui.node(h) else { return };
    if !n.visible {
        return;
    }
    let area = n.rect.offset(ox, oy);
    let Some(clip) = area.intersect(parent_clip) else {
        return;
    };
    if let Some(p) = n.picture {
        if let Some(img) = ui.picture(p.id) {
            let how = Blit::of_mode(p.mode, n.alpha);
            let stretch = n.bool_attr(attr::SIZE_IMAGE_TO_OWNER);
            draw_image(layer, &img, area, clip, how, stretch, n.center_image);
            drawn.add(clip);
        }
    }
    if n.kind == kind::TEXT {
        crate::types::text::post_blit(ui, layer, h, area, clip);
        drawn.add(clip);
    }
    for c in n.children.iter().rev() {
        draw_element(ui, layer, *c, area.x0, area.y0, clip, drawn);
    }
    if n.kind == kind::TREE {
        drawn.add(clip);
        let (segments, color) = ui.tree_segments(h);
        for [x0, y0, x1, y1] in segments {
            draw_line(
                layer,
                (area.x0 + x0, area.y0 + y0),
                (area.x0 + x1, area.y0 + y1),
                color,
                clip,
            );
        }
    }
    // Each fill is a window onto the element's own picture, filled with one texel.
    // D2-09962, D2-09963, D2-09964
    for (r, texel) in &n.fills {
        let w = window(*r, n.rect.width(), n.rect.height());
        let Some(c) = w.offset(area.x0, area.y0).intersect(clip) else {
            continue;
        };
        drawn.add(c);
        for y in c.y0..=c.y1 {
            for x in c.x0..=c.x1 {
                layer.put(x, y, *texel);
            }
        }
    }
    if let Some((ox, oy, picture)) = &n.overlay {
        let (x0, y0) = (area.x0 + ox, area.y0 + oy);
        let r = Rect::sized(x0, y0, picture.width, picture.height);
        if let Some(c) = r.intersect(clip) {
            drawn.add(c);
            for y in c.y0..=c.y1 {
                for x in c.x0..=c.x1 {
                    blit_texel(layer, x, y, picture.get(x - x0, y - y0), Blit::Blend);
                }
            }
        }
    }
}

/// The largest whole number at or below `x`, of a size a screen's texels have.
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
#[inline]
fn floor_small(x: f32) -> i32 {
    let i = x as i32; // LINT-OK: a screen's texels, far inside the range
    i - i32::from((i as f32) > x)
}

/// A channel worked out as a number from 0 up, to the nearest whole value it can hold.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
#[inline]
fn round_channel(v: f32) -> u8 {
    let i = v as i32; // LINT-OK: a channel's value, far inside the range
    (i + i32::from(v - i as f32 >= 0.5)).min(255) as u8
}

/// A top-level element's layer turned about the middle of its rectangle by whole degrees,
/// counterclockwise, as the original turns a drawn object in the screen's plane (texels sampled
/// between their four neighbours); the rectangle the turned picture covers.
// D2-10429
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn turn_layer(layer: &Surface, out: &mut Surface, area: Rect, degrees: i32) -> Rect {
    let a = (degrees as f32).to_radians();
    let (sin, cos) = (
        dereth_primitives::num::math::sinf(a),
        dereth_primitives::num::math::cosf(a),
    );
    let cx = (area.x0 + area.x1) as f32 / 2.0 + 0.5;
    let cy = (area.y0 + area.y1) as f32 / 2.0 + 0.5;
    let (hw, hh) = (area.width() as f32 / 2.0, area.height() as f32 / 2.0);
    let rw = hw * cos.abs() + hh * sin.abs();
    let rh = hw * sin.abs() + hh * cos.abs();
    let covers = Rect::new(
        floor_to_i32(cx - rw),
        floor_to_i32(cy - rh),
        to_i32((cx + rw).ceil()),
        to_i32((cy + rh).ceil()),
    );
    let Some(c) = covers.intersect(layer.bounds()) else {
        return covers;
    };
    // The texels a sample can read: those of the area on the layer. Off it they are clear.
    let Some(read) = area.intersect(layer.bounds()) else {
        return covers;
    };
    let sample = |x: f32, y: f32| -> [f32; 4] {
        let (ix, iy) = (floor_small(x), floor_small(y));
        let (fx, fy) = (x - ix as f32, y - iy as f32);
        let mut acc = [0.0f32; 4];
        for (dx, dy, w) in [
            (0, 0, (1.0 - fx) * (1.0 - fy)),
            (1, 0, fx * (1.0 - fy)),
            (0, 1, (1.0 - fx) * fy),
            (1, 1, fx * fy),
        ] {
            let (sx, sy) = (ix + dx, iy + dy);
            if !read.contains(sx, sy) {
                continue;
            }
            let i = (sy * layer.width + sx) as usize * 4;
            let Some((t, _)) = layer.bgra[i..].split_first_chunk::<4>() else {
                continue;
            };
            if t[3] == 0 {
                continue;
            }
            let alpha = f32::from(t[3]) * w;
            acc[0] += f32::from(t[0]) * alpha;
            acc[1] += f32::from(t[1]) * alpha;
            acc[2] += f32::from(t[2]) * alpha;
            acc[3] += alpha;
        }
        acc
    };
    for y in c.y0..=c.y1 {
        let (from, to) = out.row(y, c.x0, c.x1);
        let (run, _) = out.bgra[from..to].as_chunks_mut::<4>();
        for (x, d) in (c.x0..=c.x1).zip(run) {
            // Where this texel's middle comes from: turned back by the angle (a positive angle
            // turns the picture counterclockwise on the screen, whose rows run down).
            let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            let sx = dx * cos - dy * sin + cx - 0.5;
            let sy = dx * sin + dy * cos + cy - 0.5;
            let acc = sample(sx, sy);
            if acc[3] <= 0.0 {
                continue;
            }
            *d = [
                round_channel(acc[0] / acc[3]),
                round_channel(acc[1] / acc[3]),
                round_channel(acc[2] / acc[3]),
                round_channel(acc[3]),
            ];
        }
    }
    covers
}

/// A line one texel wide from `a` to `b` (both ends drawn) in a color, inside `clip`.
// D2-11439
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn draw_line(layer: &mut Surface, a: (i32, i32), b: (i32, i32), color: [f32; 3], clip: Rect) {
    let px = |c: f32| (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    let texel = [px(color[2]), px(color[1]), px(color[0]), 255];
    let (dx, dy) = ((b.0 - a.0).abs(), -(b.1 - a.1).abs());
    let (sx, sy) = (
        if a.0 < b.0 { 1 } else { -1 },
        if a.1 < b.1 { 1 } else { -1 },
    );
    let (mut x, mut y, mut err) = (a.0, a.1, dx + dy);
    loop {
        if clip.contains(x, y) {
            layer.put(x, y, texel);
        }
        if x == b.0 && y == b.1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

/// Lay one layer over the picture beneath by the layer's alpha.
fn composite(dst: &mut Surface, layer: &Surface, area: Rect) {
    let Some(c) = area.intersect(dst.bounds()) else {
        return;
    };
    for y in c.y0..=c.y1 {
        for x in c.x0..=c.x1 {
            let s = layer.get(x, y);
            let a = u32::from(s[3]);
            if a == 0 {
                continue;
            }
            if a == 255 {
                dst.put(x, y, [s[0], s[1], s[2], 255]);
                continue;
            }
            let d = dst.get(x, y);
            let mix = |sc: u8, dc: u8| -> u8 {
                u8::try_from((u32::from(sc) * a + u32::from(dc) * (255 - a) + 127) / 255)
                    .unwrap_or(255)
            };
            dst.put(
                x,
                y,
                [mix(s[0], d[0]), mix(s[1], d[1]), mix(s[2], d[2]), 255],
            );
        }
    }
}

/// Draw the interface over `frame`, which holds what is beneath it (the scene, or black).
pub fn draw(ui: &Ui, frame: &mut Surface) {
    draw_where(ui, frame, |_| true);
}

/// Draw only the top-level elements whose depth passes `keep`, deepest first. A scene shown in a
/// viewport stands at the viewport's depth: a host draws the elements deeper than it, the scene,
/// then the nearer ones. Each is drawn into a transparent layer of its own, then laid over.
pub fn draw_where(ui: &Ui, frame: &mut Surface, keep: impl Fn(i32) -> bool) {
    let mut layer = Surface::new(frame.width, frame.height);
    let mut turned = None;
    let screen = frame.bounds();
    for (r, z) in layers(ui) {
        let Some(n) = ui.node(r) else { continue };
        if !n.visible || !keep(z) || !has_surface(ui, r) {
            continue;
        }
        let Some(clip) = layer_clip(ui, r, screen) else {
            continue;
        };
        let mut drawn = Drawn::default();
        draw_element(ui, &mut layer, r, 0, 0, clip, &mut drawn);
        if let Some(area) = drawn.0 {
            fade_layer(&mut layer, area, n.object_alpha);
            if n.rotation == 0 {
                composite(frame, &layer, area.intersect(n.rect).unwrap_or(area));
            } else {
                let turned = turned.get_or_insert_with(|| Surface::new(layer.width, layer.height));
                let covers = turn_layer(&layer, turned, n.rect, n.rotation);
                composite(frame, turned, covers);
                turned.clear_area(covers);
            }
            // The next element starts from a clear layer.
            layer.clear_area(area);
        }
        draw_resize_outline(ui, frame, r, screen);
    }
}

/// While a window with a resize line is being resized its picture stays as it was, and a white
/// outline one texel wide shows the place and size it takes on release, at the window's own depth.
// D2-10398, D2-10399, D2-11282
fn draw_resize_outline(ui: &Ui, out: &mut Surface, root: Handle, screen: Rect) {
    const WHITE: [f32; 3] = [1.0, 1.0, 1.0];
    let Some(line) = ui.resize_outline(root) else {
        return;
    };
    let (x0, y0, x1, y1) = (line.x0, line.y0, line.x1, line.y1);
    draw_line(out, (x0, y0), (x1, y0), WHITE, screen);
    draw_line(out, (x0, y1), (x0, y0), WHITE, screen);
    draw_line(out, (x0, y1), (x1, y1), WHITE, screen);
    draw_line(out, (x1, y0), (x1, y1), WHITE, screen);
}

/// Where a layer is drawn: the screen, and for an item being dragged only the area it is shown
/// in (the size of the item it was copied from).
fn layer_clip(ui: &Ui, h: Handle, screen: Rect) -> Option<Rect> {
    if ui.dragged() == Some(h) {
        return ui.drag_area()?.intersect(screen);
    }
    Some(screen)
}

/// Whether a top-level element is drawn into a layer of its own: every one but those marked
/// `no_surface_object`, which take the pointer and show nothing.
// D2-11886, D2-11885
fn has_surface(ui: &Ui, root: Handle) -> bool {
    !ui.node(root)
        .is_some_and(|n| n.bool_attr(attr::NO_SURFACE_OBJECT))
}

/// The top-level elements from the deepest, each with the depth it is drawn at, then an item
/// being dragged, in front of everything: each top-level element is an object of its own while it
/// is on the screen. Of equal depths the one put on the screen first is drawn last, in front; a
/// tooltip is in front of them all.
// D2-11281, D2-11220, D2-11265, D2-11152, D2-10386
fn layers(ui: &Ui) -> Vec<(Handle, i32)> {
    let mut out: Vec<(Handle, i32)> = ui
        .roots()
        .iter()
        .rev()
        .filter_map(|r| ui.node(*r).map(|n| (*r, n.z)))
        .collect();
    if let Some(tip) = ui.tooltip() {
        if let Some(at) = out.iter().position(|(r, _)| *r == tip) {
            let shown = out.remove(at);
            out.push(shown);
        }
    }
    if let Some(d) = ui.dragged() {
        out.push((d, i32::MIN));
    }
    out
}

/// A top-level element drawn as a whole at an alpha below one (a fade): every texel's alpha
/// scaled by it, as the original sets its drawn object's material opacity.
// D2-10436
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn fade_layer(layer: &mut Surface, area: Rect, alpha: f32) {
    if alpha >= 1.0 {
        return;
    }
    let a = alpha.max(0.0);
    let Some(c) = area.intersect(layer.bounds()) else {
        return;
    };
    for y in c.y0..=c.y1 {
        for x in c.x0..=c.x1 {
            let mut t = layer.get(x, y);
            t[3] = (f32::from(t[3]) * a) as u8;
            layer.put(x, y, t);
        }
    }
}

/// Lay one layer over a picture that keeps its own alpha (`over`): what the layer covers wins by
/// the layer's alpha, and the picture's alpha grows by what the layer adds.
fn composite_over(dst: &mut Surface, layer: &Surface, area: Rect) {
    let Some(c) = area
        .intersect(dst.bounds())
        .and_then(|c| c.intersect(layer.bounds()))
    else {
        return;
    };
    for y in c.y0..=c.y1 {
        let (from, to) = layer.row(y, c.x0, c.x1);
        let (at, end) = dst.row(y, c.x0, c.x1);
        let texels = layer.bgra[from..to].as_chunks::<4>().0;
        let under = dst.bgra[at..end].as_chunks_mut::<4>().0;
        for (s, d) in texels.iter().zip(under) {
            let sa = u32::from(s[3]);
            if sa == 0 {
                continue;
            }
            let da = u32::from(d[3]);
            if sa == 255 || da == 0 {
                *d = *s;
                continue;
            }
            // Straight alpha: out = s * sa + d * da * (1 - sa), over out's alpha.
            let keep = da * (255 - sa);
            let oa = sa * 255 + keep;
            let mix = |sc: u8, dc: u8| -> u8 {
                u8::try_from((u32::from(sc) * sa * 255 + u32::from(dc) * keep + oa / 2) / oa)
                    .unwrap_or(255)
            };
            let a = u8::try_from((oa + 127) / 255).unwrap_or(255);
            *d = [mix(s[0], d[0]), mix(s[1], d[1]), mix(s[2], d[2]), a];
        }
    }
}

/// Draw the top-level elements whose depth passes `keep`, deepest first, into a picture that
/// starts transparent and keeps how much each texel is covered: laid over a scene by its alpha, it
/// gives what [`draw_where`] gives over that scene.
pub fn draw_layers_where(ui: &Ui, out: &mut Surface, keep: impl Fn(i32) -> bool) {
    draw_layers_with(ui, out, keep, &mut Layers::default());
}

/// The clear pictures top-level elements are drawn into on their way to the frame, kept so that a
/// frame need not make them again.
#[derive(Debug, Default)]
pub struct Layers {
    layer: Option<Surface>,
    turned: Option<Surface>,
}

/// [`draw_layers_where`], drawing into `layers`, and leaving them clear.
// D2-11219, D2-11163
pub fn draw_layers_with(
    ui: &Ui,
    out: &mut Surface,
    keep: impl Fn(i32) -> bool,
    layers_kept: &mut Layers,
) {
    out.clear();
    let fits = |s: &Surface| s.width == out.width && s.height == out.height;
    layers_kept.layer = layers_kept.layer.take().filter(fits);
    layers_kept.turned = layers_kept.turned.take().filter(fits);
    let (width, height) = (out.width, out.height);
    let layer = layers_kept
        .layer
        .get_or_insert_with(|| Surface::new(width, height));
    let turned = &mut layers_kept.turned;
    let screen = out.bounds();
    for (r, z) in layers(ui) {
        let Some(n) = ui.node(r) else { continue };
        if !n.visible || !keep(z) || !has_surface(ui, r) {
            continue;
        }
        let Some(clip) = layer_clip(ui, r, screen) else {
            continue;
        };
        let mut drawn = Drawn::default();
        draw_element(ui, layer, r, 0, 0, clip, &mut drawn);
        if let Some(area) = drawn.0 {
            fade_layer(layer, area, n.object_alpha);
            if n.rotation == 0 {
                composite_over(out, layer, area);
            } else {
                let turned = turned.get_or_insert_with(|| Surface::new(layer.width, layer.height));
                let covers = turn_layer(layer, turned, n.rect, n.rotation);
                composite_over(out, turned, covers);
                turned.clear_area(covers);
            }
            // The next element starts from a clear layer.
            layer.clear_area(area);
        }
        draw_resize_outline(ui, out, r, screen);
    }
}

/// The viewports on the screen (where the game draws a scene inside the interface), deepest
/// first, as rectangles; each viewport's scene and camera are its `uiscene` and `camera`
/// attributes.
#[must_use]
pub fn viewports(ui: &Ui) -> Vec<(Handle, Rect)> {
    let mut out = Vec::new();
    fn walk(ui: &Ui, h: Handle, out: &mut Vec<(Handle, Rect)>) {
        let Some(n) = ui.node(h) else { return };
        if !n.visible {
            return;
        }
        if n.kind == kind::VIEWPORT {
            if let Some(r) = ui.screen_rect(h) {
                out.push((h, r));
            }
        }
        for c in n.children.iter().rev() {
            walk(ui, *c, out);
        }
    }
    for r in ui.roots().iter().rev() {
        walk(ui, *r, &mut out);
    }
    out
}
