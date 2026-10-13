//! Small pictures of models, drawn on the CPU.
//!
//! The model is seen from its front (north, where a model faces), a little to the east and from
//! above, with a parallel projection fitted to its box, a light from above and to the front, its
//! pictures sampled nearest and cut out where they are transparent. It is drawn at twice the size and averaged down, so edges are smooth.
//! What is behind the model is transparent. Polygons that face away are not drawn unless they
//! are two-sided, so a room is seen into through its far walls; a model that leaves nothing to
//! see that way (a ceiling seen from above) is drawn again with every polygon.

use crate::model::Mesh;

/// A decoded picture: rows of blue, green, red and alpha.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<[u8; 4]>,
}

/// What one surface draws with: a picture, or a flat colour (blue, green, red, alpha), or
/// nothing (a surface that is wholly see-through, such as a cell's doorway).
#[derive(Debug, Clone, PartialEq)]
pub enum Material {
    Picture(std::sync::Arc<Image>),
    Colour([u8; 4]),
    Hidden,
}

/// A finished picture: rows of red, green, blue and alpha.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    pub size: u32,
    pub rgba: Vec<u8>,
}

impl Picture {
    /// Whether any pixel was drawn.
    #[must_use]
    pub fn is_blank(&self) -> bool {
        self.rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 0)
    }

    /// The picture as PNG bytes.
    ///
    /// # Errors
    /// The encoder's.
    pub fn png(&self) -> Result<Vec<u8>, png::EncodingError> {
        let mut out = Vec::new();
        {
            let mut e = png::Encoder::new(&mut out, self.size, self.size);
            e.set_color(png::ColorType::Rgba);
            e.set_depth(png::BitDepth::Eight);
            let mut w = e.write_header()?;
            w.write_image_data(&self.rgba)?;
        }
        Ok(out)
    }
}

/// Where the camera looks from (north-east and above: a model faces north), and where the light
/// comes from.
pub const VIEW_FROM: [f32; 3] = [0.62, 0.95, 0.62];
const LIGHT_FROM: [f32; 3] = [0.35, 0.55, 0.85];
/// How much of the picture the model's box fills.
const FILL: f32 = 0.92;
/// Below this alpha a texel is cut out.
const CUTOUT: u8 = 128;

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalised(a: [f32; 3]) -> [f32; 3] {
    let l = dot(a, a).sqrt();
    if l > 1e-12 {
        [a[0] / l, a[1] / l, a[2] / l]
    } else {
        [0.0, 0.0, 1.0]
    }
}

/// The camera's axes: right, up, and toward the viewer.
fn axes() -> ([f32; 3], [f32; 3], [f32; 3]) {
    let back = normalised(VIEW_FROM);
    let right = normalised(cross([0.0, 0.0, 1.0], back));
    let up = cross(back, right);
    (right, up, back)
}

/// Draws `mesh` into a `size` by `size` picture. `material` answers what each surface draws
/// with; a surface it has no answer for is drawn grey. With `cull`, polygons facing away are
/// left out unless they are two-sided; without it every polygon is drawn.
#[must_use]
pub fn draw(
    mesh: &Mesh,
    material: &mut dyn FnMut(Option<dereth_primitives::DataId>) -> Option<Material>,
    size: u32,
    cull: bool,
) -> Picture {
    let big = size * 2;
    let n = (big * big) as usize;
    let mut colour = vec![[0u8; 4]; n];
    let mut depth = vec![f32::INFINITY; n];
    let Some((lo, hi)) = mesh.bounds() else {
        return Picture {
            size,
            rgba: vec![0; (size * size * 4) as usize],
        };
    };
    let (right, up, back) = axes();
    let light = normalised(LIGHT_FROM);
    // The box's corners on the picture plane fix the scale and the centre.
    let mut ext = [f32::INFINITY, -f32::INFINITY, f32::INFINITY, -f32::INFINITY];
    for k in 0..8 {
        let c = [
            if k & 1 == 0 { lo[0] } else { hi[0] },
            if k & 2 == 0 { lo[1] } else { hi[1] },
            if k & 4 == 0 { lo[2] } else { hi[2] },
        ];
        let (x, y) = (dot(c, right), dot(c, up));
        ext = [ext[0].min(x), ext[1].max(x), ext[2].min(y), ext[3].max(y)];
    }
    let span = (ext[1] - ext[0]).max(ext[3] - ext[2]).max(1e-4);
    #[allow(clippy::cast_precision_loss)] // a picture's size
    let bigf = big as f32;
    let scale = bigf * FILL / span;
    let (cx, cy) = ((ext[0] + ext[1]) * 0.5, (ext[2] + ext[3]) * 0.5);
    let to_screen = |p: [f32; 3]| -> [f32; 3] {
        [
            bigf * 0.5 + (dot(p, right) - cx) * scale,
            bigf * 0.5 - (dot(p, up) - cy) * scale,
            -dot(p, back),
        ]
    };
    for g in &mesh.groups {
        let mat = material(g.surface);
        if mat == Some(Material::Hidden) {
            continue;
        }
        for t in &g.triangles {
            let face = normalised([
                t[0].normal[0] + t[1].normal[0] + t[2].normal[0],
                t[0].normal[1] + t[1].normal[1] + t[2].normal[1],
                t[0].normal[2] + t[1].normal[2] + t[2].normal[2],
            ]);
            let facing = dot(face, back) >= 0.0;
            if cull && !facing && !g.two_sided {
                continue;
            }
            let s = [
                to_screen(t[0].position),
                to_screen(t[1].position),
                to_screen(t[2].position),
            ];
            raster(&s, big, |px, py, w| {
                let i = (py * big + px) as usize;
                let z = w[0] * s[0][2] + w[1] * s[1][2] + w[2] * s[2][2];
                if z >= depth[i] {
                    return;
                }
                let u = w[0] * t[0].uv[0] + w[1] * t[1].uv[0] + w[2] * t[2].uv[0];
                let v = w[0] * t[0].uv[1] + w[1] * t[1].uv[1] + w[2] * t[2].uv[1];
                let texel = match &mat {
                    Some(Material::Picture(img)) => sample(img, u, v),
                    Some(Material::Colour(c)) => *c,
                    Some(Material::Hidden) | None => [150, 150, 150, 255],
                };
                if texel[3] < CUTOUT {
                    return;
                }
                let mut nrm = normalised([
                    w[0] * t[0].normal[0] + w[1] * t[1].normal[0] + w[2] * t[2].normal[0],
                    w[0] * t[0].normal[1] + w[1] * t[1].normal[1] + w[2] * t[2].normal[1],
                    w[0] * t[0].normal[2] + w[1] * t[1].normal[2] + w[2] * t[2].normal[2],
                ]);
                if !facing {
                    nrm = [-nrm[0], -nrm[1], -nrm[2]];
                }
                let lit = 0.5 + 0.6 * dot(nrm, light).max(0.0);
                let shade = |c: u8| -> u8 {
                    let x = (f32::from(c) * lit).clamp(0.0, 255.0);
                    // LINT-OK: clamped to a byte's range just above.
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let b = x as u8;
                    b
                };
                depth[i] = z;
                colour[i] = [shade(texel[2]), shade(texel[1]), shade(texel[0]), 255];
            });
        }
    }
    Picture {
        size,
        rgba: halve(&colour, big),
    }
}

/// Calls `pixel(x, y, weights)` for every pixel centre inside the screen triangle `s`, with its
/// barycentric weights.
fn raster(s: &[[f32; 3]; 3], size: u32, mut pixel: impl FnMut(u32, u32, [f32; 3])) {
    let area =
        (s[1][0] - s[0][0]) * (s[2][1] - s[0][1]) - (s[2][0] - s[0][0]) * (s[1][1] - s[0][1]);
    if area.abs() < 1e-9 {
        return;
    }
    #[allow(clippy::cast_precision_loss)] // a picture's size
    let max = size as f32 - 1.0;
    let min_x = s
        .iter()
        .map(|p| p[0])
        .fold(f32::INFINITY, f32::min)
        .floor()
        .max(0.0);
    let max_x = s
        .iter()
        .map(|p| p[0])
        .fold(-f32::INFINITY, f32::max)
        .ceil()
        .min(max);
    let min_y = s
        .iter()
        .map(|p| p[1])
        .fold(f32::INFINITY, f32::min)
        .floor()
        .max(0.0);
    let max_y = s
        .iter()
        .map(|p| p[1])
        .fold(-f32::INFINITY, f32::max)
        .ceil()
        .min(max);
    if min_x > max_x || min_y > max_y {
        return;
    }
    // LINT-OK: clamped to the picture just above.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (x0, x1, y0, y1) = (min_x as u32, max_x as u32, min_y as u32, max_y as u32);
    for py in y0..=y1 {
        for px in x0..=x1 {
            #[allow(clippy::cast_precision_loss)] // a picture's size
            let (x, y) = (px as f32 + 0.5, py as f32 + 0.5);
            let w0 = (s[1][0] - x) * (s[2][1] - y) - (s[2][0] - x) * (s[1][1] - y);
            let w1 = (s[2][0] - x) * (s[0][1] - y) - (s[0][0] - x) * (s[2][1] - y);
            let w2 = (s[0][0] - x) * (s[1][1] - y) - (s[1][0] - x) * (s[0][1] - y);
            let inside = if area > 0.0 {
                w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0
            } else {
                w0 <= 0.0 && w1 <= 0.0 && w2 <= 0.0
            };
            if inside {
                pixel(px, py, [w0 / area, w1 / area, w2 / area]);
            }
        }
    }
}

/// The texel at `(u, v)`, repeating the picture, nearest.
fn sample(img: &Image, u: f32, v: f32) -> [u8; 4] {
    if img.width == 0 || img.height == 0 || img.bgra.is_empty() {
        return [150, 150, 150, 255];
    }
    let wrap = |t: f32, n: u32| -> usize {
        let f = t - t.floor();
        #[allow(clippy::cast_precision_loss)] // a picture's size
        let x = (f * n as f32).floor();
        // LINT-OK: f is in [0, 1), so x is in [0, n].
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let i = x as usize;
        i.min(n as usize - 1)
    };
    let (x, y) = (wrap(u, img.width), wrap(v, img.height));
    img.bgra
        .get(y * img.width as usize + x)
        .copied()
        .unwrap_or([150, 150, 150, 255])
}

/// Averages each 2 by 2 block of `big` (a `big` by `big` picture) into one pixel, weighting the
/// colour by coverage.
fn halve(colour: &[[u8; 4]], big: u32) -> Vec<u8> {
    let size = big / 2;
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let mut sum = [0u32; 3];
            let mut covered = 0u32;
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let p = colour[((2 * y + dy) * big + 2 * x + dx) as usize];
                if p[3] > 0 {
                    covered += 1;
                    for c in 0..3 {
                        sum[c] += u32::from(p[c]);
                    }
                }
            }
            for s in sum {
                let mean = s.checked_div(covered).unwrap_or(0);
                out.push(u8::try_from(mean).unwrap_or(u8::MAX));
            }
            out.push(u8::try_from(covered * 255 / 4).unwrap_or(u8::MAX));
        }
    }
    out
}
