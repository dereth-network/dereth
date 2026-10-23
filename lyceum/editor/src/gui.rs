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
    /// goes back: the editor's frames and a playtest's are timed by different clocks.
    last_time: Option<f64>,
    time_offset: f64,
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
        let (w, h) = (
            size.0.max(1) as f32 / self.scale,
            size.1.max(1) as f32 / self.scale,
        );
        let to_clip = |p: egui::Pos2| [p.x / w * 2.0 - 1.0, 1.0 - p.y / h * 2.0, 0.5];
        for egui::ClippedPrimitive {
            clip_rect,
            primitive,
        } in primitives
        {
            let egui::epaint::Primitive::Mesh(mesh) = primitive else {
                continue;
            };
            let Some(tex) = self.textures.get(&mesh.texture_id) else {
                continue;
            };
            let sampler = OverlaySampler {
                point: !tex.linear,
                wrap_u: false,
                wrap_v: false,
            };
            let key = tex.key;
            let mut vertices = Vec::with_capacity(mesh.indices.len());
            for t in mesh.indices.as_chunks::<3>().0 {
                let tri = [
                    mesh.vertices[t[0] as usize],
                    mesh.vertices[t[1] as usize],
                    mesh.vertices[t[2] as usize],
                ];
                for poly in clip(&tri, clip_rect) {
                    let v = |x: &egui::epaint::Vertex| OverlayVertex {
                        position: to_clip(x.pos),
                        color: straight(x.color),
                        uv: [x.uv.x, x.uv.y],
                    };
                    let (a, b, c) = (v(&poly[0]), v(&poly[1]), v(&poly[2]));
                    // Counter-clockwise in clip space (y up), the overlay's front face.
                    let turn = (b.position[0] - a.position[0]) * (c.position[1] - a.position[1])
                        - (b.position[1] - a.position[1]) * (c.position[0] - a.position[0]);
                    if turn >= 0.0 {
                        vertices.extend([a, b, c]);
                    } else {
                        vertices.extend([a, c, b]);
                    }
                }
            }
            if vertices.is_empty() {
                continue;
            }
            match items.last_mut() {
                Some(OverlayItem::Triangles {
                    texture,
                    sampler: s,
                    vertices: v,
                    ..
                }) if *texture == key && *s == sampler => v.extend(vertices),
                _ => items.push(OverlayItem::Triangles {
                    material: OverlayMaterial::Image,
                    texture: key,
                    sampler,
                    vertices,
                    depth: dereth_client_contract::overlay::OverlayDepth::Over,
                }),
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
