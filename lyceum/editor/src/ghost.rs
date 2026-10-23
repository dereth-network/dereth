//! The ghost of a piece about to be set down, drawn in the world view as the piece itself: each
//! polygon of its parts textured with the surfaces the dungeon's look gives it and shaded by the
//! way it faces, as the world draws a room seen from outside (only the faces whose inside is
//! turned to the eye). Where the world stands in front of it, it shows faintly; elsewhere whole.
//! Its edges are drawn over it with the other marks ([`super::EditorApp::dungeon_marks`]).

use std::collections::BTreeMap;

use dereth_assets::Decode;
use dereth_client_contract::overlay::{
    OverlayDepth, OverlayItem, OverlayMaterial, OverlaySampler, OverlaySpace, OverlayTexture,
    OverlayVertex,
};
use dereth_primitives::frame::{localtoglobal, V3};
use dereth_primitives::{DataId, TextureData, TextureFormat, Vec3};

use super::EditorApp;

/// The first overlay key the ghost's pictures are uploaded under: far past the panels' own.
const KEY_BASE: u64 = 0x6000_0000_0000;

/// The largest side a ghost's picture is uploaded at, in texels.
const SIDE: usize = 128;

/// How long a ghost's triangle's side may be before it is cut in four, in meters: the overlay
/// lays a picture across a triangle evenly, so a long one bends it.
const PIECE: f32 = 1.5;

/// How faint the ghost is where the world stands in front of it, of 255.
const HIDDEN_ALPHA: u8 = 70;

/// The ghost's pictures as uploaded: each surface's picture, or its colour on a white texel.
#[derive(Debug, Default)]
pub struct GhostDraw {
    pictures: BTreeMap<u32, Option<(OverlayTexture, u32)>>,
    white: Option<OverlayTexture>,
    /// How many triangles the ghost was drawn with last.
    pub triangles: usize,
}

/// One corner of a ghost's triangle: where it is in the world, and its texture coordinate.
#[derive(Clone, Copy, Debug)]
struct Corner {
    at: Vec3,
    uv: [f32; 2],
}

/// Triangle `t` cut into pieces no side of which is longer than [`PIECE`], at most three times.
fn cut(t: [Corner; 3], depth: u8, out: &mut Vec<[Corner; 3]>) {
    let long = (0..3).any(|k| {
        let d = t[k].at.sub(t[(k + 1) % 3].at);
        d.dot(d) > PIECE * PIECE
    });
    if !long || depth >= 3 {
        out.push(t);
        return;
    }
    let mid = |a: Corner, b: Corner| Corner {
        at: a.at.add(b.at).mul(0.5),
        uv: [(a.uv[0] + b.uv[0]) / 2.0, (a.uv[1] + b.uv[1]) / 2.0],
    };
    let (ab, bc, ca) = (mid(t[0], t[1]), mid(t[1], t[2]), mid(t[2], t[0]));
    for s in [[t[0], ab, ca], [ab, t[1], bc], [ca, bc, t[2]], [ab, bc, ca]] {
        cut(s, depth + 1, out);
    }
}

/// A colour packed `0xAARRGGBB`.
fn packed(r: u8, g: u8, b: u8, a: u8) -> u32 {
    (u32::from(a) << 24) | (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
}

impl EditorApp {
    /// Surface `s`'s picture as the overlay holds it (uploaded the first time), and the colour
    /// its texels are multiplied by: a textured surface's own picture under white, a plain one's
    /// colour on a white texel.
    fn ghost_picture(&mut self, s: DataId) -> Option<(OverlayTexture, u32)> {
        if let Some(p) = self.ghost_draw.pictures.get(&s.raw()) {
            return *p;
        }
        let era = self.base_store.era_of(s);
        let surface = self
            .base_store
            .portal()
            .read(s)
            .ok()
            .and_then(|b| dereth_assets::Surface::decode_payload_in(era, s, &b).ok());
        let picture = surface.and_then(|surf| {
            if let Some(tex) = surf.orig_texture_id.filter(|_| surf.surface_type & 6 != 0) {
                let img = dereth_scene::textures::TextureStore::new(&self.base_store)
                    .bgra8(tex)
                    .ok()?;
                let (w, h) = (img.width as usize, img.height as usize);
                if w == 0 || h == 0 {
                    return None;
                }
                // Halved till it fits.
                let (mut sw, mut sh) = (w, h);
                while sw > SIDE || sh > SIDE {
                    sw = (sw / 2).max(1);
                    sh = (sh / 2).max(1);
                }
                let mut bgra = Vec::with_capacity(sw * sh * 4);
                for y in 0..sh {
                    for x in 0..sw {
                        let p = img.pixels[(y * h / sh) * w + x * w / sw];
                        bgra.extend_from_slice(&[p[0], p[1], p[2], 255]);
                    }
                }
                let key = OverlayTexture {
                    space: OverlaySpace::Local,
                    key: KEY_BASE + u64::from(s.raw()),
                };
                self.shell.uploads.push((
                    key,
                    TextureData {
                        width: u32::try_from(sw).unwrap_or(1),
                        height: u32::try_from(sh).unwrap_or(1),
                        format: TextureFormat::Bgra8,
                        levels: vec![bgra],
                    },
                ));
                Some((key, packed(255, 255, 255, 255)))
            } else {
                let c = surf.color_value.unwrap_or(0xFF80_8080);
                let white = self.ghost_white();
                #[allow(clippy::cast_possible_truncation)]
                Some((white, packed((c >> 16) as u8, (c >> 8) as u8, c as u8, 255)))
            }
        });
        self.ghost_draw.pictures.insert(s.raw(), picture);
        picture
    }

    /// The white texel a plain surface's colour is drawn on, uploaded the first time.
    fn ghost_white(&mut self) -> OverlayTexture {
        if let Some(w) = self.ghost_draw.white {
            return w;
        }
        let key = OverlayTexture {
            space: OverlaySpace::Local,
            key: KEY_BASE - 1,
        };
        self.shell.uploads.push((
            key,
            TextureData {
                width: 1,
                height: 1,
                format: TextureFormat::Bgra8,
                levels: vec![vec![255, 255, 255, 255]],
            },
        ));
        self.ghost_draw.white = Some(key);
        key
    }

    /// The ghost's triangles for the frame about to be drawn, in the window's clip space: nothing
    /// when there is no ghost, or the world view is not the editor's own.
    #[allow(clippy::too_many_lines)]
    pub(super) fn ghost_items(&mut self) -> Vec<OverlayItem> {
        self.ghost_draw.triangles = 0;
        if self.playing.is_some() || self.walking {
            return Vec::new();
        }
        let (Some(lb), Some(g)) = (self.dungeon_mode(), self.layout.ghost.clone()) else {
            return Vec::new();
        };
        let Some(piece) = self
            .kit
            .pieces
            .as_ref()
            .and_then(|p| p.get(g.piece))
            .cloned()
        else {
            return Vec::new();
        };
        let Some(env) = self.environment(piece.environment).cloned() else {
            return Vec::new();
        };
        let Some(view) = self.view() else {
            return Vec::new();
        };
        let surfaces = self.surfaces_for(lb, &piece);
        let (w, h) = self.core.present.size();
        #[allow(clippy::cast_precision_loss)]
        let (w, h) = (w.max(1) as f32, h.max(1) as f32);
        let base = lyceum_foundry::pick::block_origin(lb);
        let eye = self.eye();
        let light = Vec3::new(0.35, -0.45, 0.82);
        // Each picture's triangles: shown whole, and faintly where hidden.
        let mut batches: BTreeMap<u64, (OverlayTexture, Vec<OverlayVertex>, Vec<OverlayVertex>)> =
            BTreeMap::new();
        for cs in &env.cells {
            let vs = &cs.vertex_array.vertices;
            #[allow(clippy::cast_precision_loss)]
            let middle = vs
                .iter()
                .fold(Vec3::ZERO, |s, v| s.add(v.position))
                .mul(1.0 / vs.len().max(1) as f32);
            let portals: std::collections::BTreeSet<u16> = cs.portals.iter().copied().collect();
            for (i, poly) in cs.polygons.iter().enumerate() {
                if u16::try_from(i).is_ok_and(|i| portals.contains(&i)) {
                    continue;
                }
                let corners: Vec<Corner> = poly
                    .vertex_ids
                    .iter()
                    .enumerate()
                    .filter_map(|(k, id)| {
                        let v = vs.get(usize::from(*id))?;
                        let uv = poly
                            .pos_uv_indices
                            .as_ref()
                            .and_then(|u| u.get(k))
                            .and_then(|u| v.uvs.get(usize::from(*u)))
                            .map_or([0.0, 0.0], |(a, b)| [*a, *b]);
                        Some(Corner {
                            at: localtoglobal(&g.frame, v.position).add(base),
                            uv,
                        })
                    })
                    .collect();
                if corners.len() < 3 {
                    continue;
                }
                // The face's inside, toward the part's middle: shown when the eye is on it.
                let pts: Vec<Vec3> = poly
                    .vertex_ids
                    .iter()
                    .filter_map(|id| vs.get(usize::from(*id)).map(|v| v.position))
                    .collect();
                let mut n = Vec3::ZERO;
                for k in 0..pts.len() {
                    let (a, b) = (pts[k], pts[(k + 1) % pts.len()]);
                    n.x += (a.y - b.y) * (a.z + b.z);
                    n.y += (a.z - b.z) * (a.x + b.x);
                    n.z += (a.x - b.x) * (a.y + b.y);
                }
                let len = n.dot(n).sqrt();
                if len < 1e-6 {
                    continue;
                }
                #[allow(clippy::cast_precision_loss)]
                let c = pts
                    .iter()
                    .fold(Vec3::ZERO, |s, p| s.add(*p))
                    .mul(1.0 / pts.len() as f32);
                let mut inward = n.mul(1.0 / len);
                if inward.dot(middle.sub(c)) < 0.0 {
                    inward = inward.mul(-1.0);
                }
                let turn = dereth_primitives::Frame::new(Vec3::ZERO, g.frame.rotation);
                let inward_world = localtoglobal(&turn, inward);
                if inward_world.dot(eye.sub(corners[0].at)) <= 0.0 {
                    continue;
                }
                let Some(surface) = surfaces.get(usize::from(poly.pos_surface)).copied() else {
                    continue;
                };
                let Some((tex, tint)) = self.ghost_picture(surface) else {
                    continue;
                };
                let shade = 0.55 + 0.45 * inward_world.dot(light).abs().min(1.0);
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let scale = |c: u32, shift: u32| ((((c >> shift) & 0xFF) as f32) * shade) as u8;
                let lit = |a: u8| packed(scale(tint, 16), scale(tint, 8), scale(tint, 0), a);
                let mut tris = Vec::new();
                for k in 1..corners.len() - 1 {
                    cut([corners[0], corners[k], corners[k + 1]], 0, &mut tris);
                }
                let entry = batches
                    .entry(tex.key)
                    .or_insert_with(|| (tex, Vec::new(), Vec::new()));
                for t in tris {
                    let projected: Option<Vec<(f32, f32, f32)>> =
                        t.iter().map(|c| view.project_depth(c.at)).collect();
                    let Some(p) = projected else { continue };
                    for (corner, (x, y, z)) in t.iter().zip(&p) {
                        let position = [x / w * 2.0 - 1.0, 1.0 - y / h * 2.0, *z];
                        entry.1.push(OverlayVertex {
                            position,
                            color: lit(235),
                            uv: corner.uv,
                        });
                        entry.2.push(OverlayVertex {
                            position,
                            color: lit(HIDDEN_ALPHA),
                            uv: corner.uv,
                        });
                    }
                    self.ghost_draw.triangles += 1;
                }
            }
        }
        let sampler = OverlaySampler {
            point: false,
            wrap_u: true,
            wrap_v: true,
        };
        let mut out = Vec::new();
        for (tex, _, faint) in batches.values() {
            out.push(OverlayItem::Triangles {
                material: OverlayMaterial::Image,
                texture: *tex,
                sampler,
                depth: OverlayDepth::Over,
                vertices: faint.clone(),
            });
        }
        for (tex, whole, _) in batches.into_values() {
            out.push(OverlayItem::Triangles {
                material: OverlayMaterial::Image,
                texture: tex,
                sampler,
                depth: OverlayDepth::World,
                vertices: whole,
            });
        }
        out
    }
}
