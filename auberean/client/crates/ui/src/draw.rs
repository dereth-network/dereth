//! The interface drawn to a picture as the original composes it: each top-level element into a
//! layer of its own, its elements' pictures and text in order (an element, then its children from
//! the deepest), and the layers over the scene from the deepest, each blended by its alpha.
//!
//! A picture is drawn once when it is the element's size, stretched over the element when the
//! element asks for that (`size_image_to_owner`), and otherwise repeated across it. Media items
//! of draw mode 1 and 2 blend the picture over the layer by its alpha, keeping the layer's alpha;
//! modes 0 and 3 copy its texels, alpha included.

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

/// One source texel onto one layer texel.
// D2-09960
// The original truncates the faded alpha toward zero.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn blit_texel(dst: &mut Surface, x: i32, y: i32, s: [u8; 4], how: Blit) {
    let blend = match how {
        Blit::Copy => false,
        Blit::CopyFaded(a) => {
            let faded = (f32::from(s[3]) * a.clamp(0.0, 1.0)) as u8;
            dst.put(x, y, [s[0], s[1], s[2], faded]);
            return;
        }
        Blit::Blend => true,
    };
    if !blend || s[3] == 0xFF {
        if !blend || s[3] != 0 {
            dst.put(x, y, s);
        }
        return;
    }
    if s[3] == 0 {
        return;
    }
    let d = dst.get(x, y);
    let a = i32::from(s[3]);
    let mix = |dc: u8, sc: u8| -> u8 {
        let dc = i32::from(dc);
        u8::try_from(dc - (dc - i32::from(sc)) * a / 256).unwrap_or(0)
    };
    dst.put(
        x,
        y,
        [mix(d[0], s[0]), mix(d[1], s[1]), mix(d[2], s[2]), d[3]],
    );
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
    let Some(c) = area.intersect(clip) else {
        return;
    };
    let (bw, bh) = (area.width(), area.height());
    if center && !(iw == bw && ih == bh) {
        // Once, its top left at half the room left over (none when it is larger).
        let x0 = area.x0 + ((bw - iw) / 2).max(0);
        let y0 = area.y0 + ((bh - ih) / 2).max(0);
        let Some(c) = Rect::new(x0, y0, x0 + iw - 1, y0 + ih - 1).intersect(c) else {
            return;
        };
        for y in c.y0..=c.y1 {
            let sy = u32::try_from(y - y0).unwrap_or(0);
            for x in c.x0..=c.x1 {
                let sx = u32::try_from(x - x0).unwrap_or(0);
                blit_texel(dst, x, y, img.texel(sx, sy), how);
            }
        }
        return;
    }
    if (iw == bw && ih == bh) || !stretch {
        // Once at its size, or repeated across the element from its top left.
        for y in c.y0..=c.y1 {
            let sy = u32::try_from((y - area.y0).rem_euclid(ih)).unwrap_or(0);
            for x in c.x0..=c.x1 {
                let sx = u32::try_from((x - area.x0).rem_euclid(iw)).unwrap_or(0);
                blit_texel(dst, x, y, img.texel(sx, sy), how);
            }
        }
        return;
    }
    for y in c.y0..=c.y1 {
        let sy = u32::try_from(i64::from(y - area.y0) * i64::from(ih) / i64::from(bh)).unwrap_or(0);
        for x in c.x0..=c.x1 {
            let sx =
                u32::try_from(i64::from(x - area.x0) * i64::from(iw) / i64::from(bw)).unwrap_or(0);
            blit_texel(
                dst,
                x,
                y,
                img.texel(sx.min(img.width - 1), sy.min(img.height - 1)),
                how,
            );
        }
    }
}

/// Draw one element and what is under it into a layer.
// D2-11176, D2-11169
fn draw_element(ui: &Ui, layer: &mut Surface, h: Handle, ox: i32, oy: i32, parent_clip: Rect) {
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
        }
    }
    if n.kind == kind::TEXT {
        crate::types::text::post_blit(ui, layer, h, area, clip);
    }
    for c in n.children.iter().rev() {
        draw_element(ui, layer, *c, area.x0, area.y0, clip);
    }
    if n.kind == kind::TREE {
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
    for (r, texel) in &n.fills {
        let Some(c) = r.offset(area.x0, area.y0).intersect(clip) else {
            continue;
        };
        for y in c.y0..=c.y1 {
            for x in c.x0..=c.x1 {
                layer.put(x, y, *texel);
            }
        }
    }
}

/// A top-level element's layer turned about the middle of its rectangle by whole degrees,
/// counterclockwise, as the original turns a drawn object in the screen's plane (texels sampled
/// between their four neighbours); the rectangle the turned picture covers.
// D2-10429
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn turn_layer(layer: &Surface, area: Rect, degrees: i32) -> (Surface, Rect) {
    let mut out = Surface::new(layer.width, layer.height);
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
        return (out, covers);
    };
    let sample = |x: f32, y: f32| -> [f32; 4] {
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let (ix, iy) = (to_i32(x0), to_i32(y0));
        let mut acc = [0.0f32; 4];
        for (dx, dy, w) in [
            (0, 0, (1.0 - fx) * (1.0 - fy)),
            (1, 0, fx * (1.0 - fy)),
            (0, 1, (1.0 - fx) * fy),
            (1, 1, fx * fy),
        ] {
            let (sx, sy) = (ix + dx, iy + dy);
            let t = if area.contains(sx, sy) {
                layer.get(sx, sy)
            } else {
                [0; 4]
            };
            let alpha = f32::from(t[3]) * w;
            acc[0] += f32::from(t[0]) * alpha;
            acc[1] += f32::from(t[1]) * alpha;
            acc[2] += f32::from(t[2]) * alpha;
            acc[3] += alpha;
        }
        acc
    };
    for y in c.y0..=c.y1 {
        for x in c.x0..=c.x1 {
            // Where this texel's middle comes from: turned back by the angle (a positive angle
            // turns the picture counterclockwise on the screen, whose rows run down).
            let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            let sx = dx * cos - dy * sin + cx - 0.5;
            let sy = dx * sin + dy * cos + cy - 0.5;
            let acc = sample(sx, sy);
            if acc[3] <= 0.0 {
                continue;
            }
            let px = |v: f32| (v / acc[3]).round().clamp(0.0, 255.0) as u8;
            out.put(
                x,
                y,
                [
                    px(acc[0]),
                    px(acc[1]),
                    px(acc[2]),
                    acc[3].round().clamp(0.0, 255.0) as u8,
                ],
            );
        }
    }
    (out, covers)
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
/// then the nearer ones.
pub fn draw_where(ui: &Ui, frame: &mut Surface, keep: impl Fn(i32) -> bool) {
    let mut layer = Surface::new(frame.width, frame.height);
    let screen = frame.bounds();
    for (r, z) in layers(ui) {
        let Some(n) = ui.node(r) else { continue };
        if !n.visible || !keep(z) {
            continue;
        }
        let Some(clip) = layer_clip(ui, r, screen) else {
            continue;
        };
        layer.clear();
        draw_element(ui, &mut layer, r, 0, 0, clip);
        fade_layer(&mut layer, n.rect, n.object_alpha);
        if n.rotation == 0 {
            composite(frame, &layer, n.rect);
        } else {
            let (turned, covers) = turn_layer(&layer, n.rect, n.rotation);
            composite(frame, &turned, covers);
        }
    }
}

/// Where a layer is drawn: the screen, and for an item being dragged only the area it is shown
/// in (the size of the item it was copied from).
fn layer_clip(ui: &Ui, h: Handle, screen: Rect) -> Option<Rect> {
    if ui.dragged() == Some(h) {
        return ui.drag_area()?.intersect(screen);
    }
    Some(screen)
}

/// The top-level elements from the deepest, each with the depth it is drawn at, then an item
/// being dragged, in front of everything.
fn layers(ui: &Ui) -> Vec<(Handle, i32)> {
    let mut out: Vec<(Handle, i32)> = ui
        .roots()
        .iter()
        .rev()
        .filter_map(|r| ui.node(*r).map(|n| (*r, n.z)))
        .collect();
    if let Some(d) = ui.dragged() {
        out.push((d, i32::MIN));
    }
    out
}

/// A top-level element drawn as a whole at an alpha below one (a fade): every texel's alpha
/// scaled by it.
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
    let Some(c) = area.intersect(dst.bounds()) else {
        return;
    };
    for y in c.y0..=c.y1 {
        for x in c.x0..=c.x1 {
            let s = layer.get(x, y);
            let sa = u32::from(s[3]);
            if sa == 0 {
                continue;
            }
            let d = dst.get(x, y);
            let da = u32::from(d[3]);
            if sa == 255 || da == 0 {
                dst.put(x, y, s);
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
            dst.put(x, y, [mix(s[0], d[0]), mix(s[1], d[1]), mix(s[2], d[2]), a]);
        }
    }
}

/// Draw the top-level elements whose depth passes `keep`, deepest first, into a picture that
/// starts transparent and keeps how much each texel is covered: laid over a scene by its alpha, it
/// gives what [`draw_where`] gives over that scene.
pub fn draw_layers_where(ui: &Ui, out: &mut Surface, keep: impl Fn(i32) -> bool) {
    out.clear();
    let mut layer = Surface::new(out.width, out.height);
    let screen = out.bounds();
    for (r, z) in layers(ui) {
        let Some(n) = ui.node(r) else { continue };
        if !n.visible || !keep(z) {
            continue;
        }
        let Some(clip) = layer_clip(ui, r, screen) else {
            continue;
        };
        layer.clear();
        draw_element(ui, &mut layer, r, 0, 0, clip);
        fade_layer(&mut layer, n.rect, n.object_alpha);
        if n.rotation == 0 {
            composite_over(out, &layer, n.rect);
        } else {
            let (turned, covers) = turn_layer(&layer, n.rect, n.rotation);
            composite_over(out, &turned, covers);
        }
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
