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
        Self {
            ctx,
            textures: HashMap::new(),
            uploads: Vec::new(),
            modifiers: egui::Modifiers::default(),
            pointer: egui::Pos2::ZERO,
        }
    }

    /// The modifiers held, as the window's events last said.
    #[must_use]
    pub fn modifiers(&self) -> egui::Modifiers {
        self.modifiers
    }

    /// egui's input for one frame of a screen of `size` pixels at `time` seconds: `events`, the
    /// window's, translated.
    pub fn input(&mut self, events: &[HostEvent], size: (u32, u32), time: f64) -> egui::RawInput {
        #[allow(clippy::cast_precision_loss)] // a screen
        let screen =
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(size.0 as f32, size.1 as f32));
        let mut out = Vec::new();
        for e in events {
            match e {
                HostEvent::CursorMoved { x, y } => {
                    #[allow(clippy::cast_possible_truncation)]
                    let p = egui::pos2(*x as f32, *y as f32);
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
        egui::RawInput {
            screen_rect: Some(screen),
            time: Some(time),
            events: out,
            max_texture_side: Some(8192),
            ..egui::RawInput::default()
        }
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
        #[allow(clippy::cast_precision_loss)] // a screen
        let (w, h) = (size.0.max(1) as f32, size.1.max(1) as f32);
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
