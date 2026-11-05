//! A landblock's terrain: its ground (through the shared terrain core), the normal at every vertex
//! (taken across block edges), the ground surface each vertex shows, its colors, and how each
//! 10-unit square blends the surfaces of its four corners.

use auberean_assets::landblock::Landblock;
use auberean_assets::region::{
    MaterialGroup, SurfaceDesc, SurfaceType, TerrainDesc, VertexColoring, WeightedMaterial,
};
use auberean_primitives::{DataId, Vec3};
use dereth_terrain::land::mesh::{land_geometry, LandGeometry};

use crate::color::Color;
use crate::land::{LandConstants, HOLE_TERRAIN};
use crate::math::{cross, normalize, sub};
use crate::random::scaled;

/// The landblocks the terrain reads: a block and its neighbors, by block coordinates.
pub trait LandSource {
    /// The landblock at block coordinates `(x, y)`, if the world has one there.
    fn landblock(&self, x: i32, y: i32) -> Option<&Landblock>;
}

/// The region's descriptions the terrain's looks come from.
#[derive(Clone, Copy, Debug)]
pub struct Surfaces<'a> {
    pub terrain: &'a TerrainDesc,
    pub surface: &'a SurfaceDesc,
}

/// What one vertex shows, from its terrain kind or its road.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct SurfacePick {
    /// The ground material: a material id whose first texture is the ground's picture.
    pub base: DataId,
    /// Units of ground one repeat of the base picture covers.
    pub base_tile: f32,
    pub corner_mask: DataId,
    pub side_mask: DataId,
    pub three_corner_mask: DataId,
    pub diagonal_mask: DataId,
    pub detail: DataId,
    pub detail_tile: f32,
    pub bump: DataId,
    pub bump_tile: f32,
    /// The near color and the far landscape's color.
    pub color: Color,
    pub far_color: Color,
}

impl SurfacePick {
    /// What a vertex shows when nothing could be chosen: no material, white.
    // D2-03699, D2-03701, D2-03698
    pub const NONE: Self = Self {
        base: DataId::NONE,
        base_tile: 0.0,
        corner_mask: DataId::NONE,
        side_mask: DataId::NONE,
        three_corner_mask: DataId::NONE,
        diagonal_mask: DataId::NONE,
        detail: DataId::NONE,
        detail_tile: 0.0,
        bump: DataId::NONE,
        bump_tile: 0.0,
        color: Color::WHITE,
        far_color: Color::WHITE,
    };

    /// Whether two picks show the same ground: the same base, detail and bump materials.
    #[must_use]
    pub fn same_ground(&self, other: &Self) -> bool {
        self.base == other.base && self.detail == other.detail && self.bump == other.bump
    }
}

/// A square's corner, in the order a square's four vertices are kept: south-west, south-east,
/// north-west, north-east.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Corner {
    Sw = 0,
    Se = 1,
    Nw = 2,
    Ne = 3,
}

impl Corner {
    /// The corner's offset from the square's south-west vertex, in vertex steps.
    #[must_use]
    pub const fn offset(self) -> (usize, usize) {
        match self {
            Self::Sw => (0, 0),
            Self::Se => (1, 0),
            Self::Nw => (0, 1),
            Self::Ne => (1, 1),
        }
    }
}

/// Which of a pick's masks a layer is cut out with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaskKind {
    Corner,
    Side,
    ThreeCorner,
    Diagonal,
}

/// A layer's mask: whose mask it is, which kind, and its quarter turn (1 = none, 2 = a quarter
/// turn clockwise, 3 = a half turn, 4 = three quarters).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mask {
    pub owner: Corner,
    pub kind: MaskKind,
    pub rotation: u8,
}

/// One layer a square is drawn with: the ground of one corner's pick, cut out by a mask unless it
/// is the bottom layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layer {
    /// Whose ground the layer shows.
    pub ground: Corner,
    /// Whether the ground is that corner's road rather than its terrain.
    pub road: bool,
    pub mask: Option<Mask>,
}

/// How one square is drawn: up to three layers, bottom first, or nothing for a hole.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SquareLook {
    pub hole: bool,
    pub layers: Vec<Layer>,
}

/// One landblock's terrain, built and dressed.
#[derive(Clone, Debug)]
pub struct TerrainBlock {
    /// Block coordinates: x east, y north.
    pub x: i32,
    pub y: i32,
    /// Vertices, splits, triangles and holes at full detail.
    pub geometry: LandGeometry,
    /// The unit normal at each vertex, x major, taken over every triangle touching it, across
    /// block edges.
    pub normals: Vec<Vec3>,
    /// What each vertex's terrain kind shows.
    pub terrain: Vec<SurfacePick>,
    /// What each vertex's road shows (no base when the vertex has no road).
    pub road: Vec<SurfacePick>,
    /// Each vertex's self-shadow bits (the vertex word's color field).
    pub shadow_bits: Vec<u32>,
    /// Each square's layers, x major.
    pub squares: Vec<SquareLook>,
}

/// A view of the heights and splits around one block, reaching one vertex into its neighbors.
struct Neighborhood<'a> {
    land: &'a LandConstants,
    /// The block and the eight around it, by `dx + 1` and `dy + 1`.
    blocks: [[Option<&'a Landblock>; 3]; 3],
    bx: i32,
    by: i32,
}

impl<'a> Neighborhood<'a> {
    fn new(land: &'a LandConstants, source: &'a dyn LandSource, bx: i32, by: i32) -> Self {
        let block = |dx: i32, dy: i32| source.landblock(bx + dx, by + dy);
        Self {
            land,
            blocks: [-1, 0, 1].map(|dx| [-1, 0, 1].map(|dy| block(dx, dy))),
            bx,
            by,
        }
    }

    /// The block `(dx, dy)` from this one, each -1, 0 or 1.
    fn block(&self, dx: i32, dy: i32) -> Option<&'a Landblock> {
        self.blocks[usize::try_from(dx + 1).ok()?][usize::try_from(dy + 1).ok()?]
    }

    fn side(&self) -> i32 {
        // LINT-OK: a block side is a few dozen vertices.
        i32::try_from(self.land.grid().vertices_per_side()).unwrap_or(i32::MAX)
    }

    /// The height at vertex `(i, j)`, `-1..=side`: an index of -1 reads the western (southern)
    /// neighbor's second-to-last vertex, an index past the last reads the next block's vertex 1,
    /// and a missing block reads 0.
    // D2-03720
    fn height(&self, i: i32, j: i32) -> f32 {
        let n = self.side();
        let wrap = |k: i32| -> (i32, i32) {
            if k < 0 {
                (-1, n - 2)
            } else if k >= n {
                (1, 1)
            } else {
                (0, k)
            }
        };
        let ((dx, i), (dy, j)) = (wrap(i), wrap(j));
        let Some(lb) = self.block(dx, dy) else {
            return 0.0;
        };
        let k = usize::try_from(n * i + j).unwrap_or(0);
        lb.heights
            .get(k)
            .map_or(0.0, |&h| self.land.heights[usize::from(h)])
    }

    /// Whether square `(i, j)`, `-1..=squares`, is cut south-west to north-east; a square of a
    /// missing block is not.
    // D2-03719
    fn cut(&self, i: i32, j: i32) -> bool {
        let squares = self.side() - 1;
        let block = |k: i32| -> i32 {
            if k < 0 {
                -1
            } else if k >= squares {
                1
            } else {
                0
            }
        };
        let (dx, dy) = (block(i), block(j));
        if self.block(dx, dy).is_none() {
            return false;
        }
        (self.land.spec.split)(self.bx * squares + i, self.by * squares + j)
    }

    fn point(&self, i: i32, j: i32) -> Vec3 {
        let step = self.land.grid().vertex_spacing();
        #[allow(clippy::cast_precision_loss)] // a few dozen vertex steps
        let (x, y) = (i as f32 * step, j as f32 * step);
        Vec3::new(x, y, self.height(i, j))
    }

    /// The unit normal at vertex `(i, j)`: the equal-weight sum of the unit normals of every
    /// triangle that touches it, in the four squares around it, normalized.
    // D2-03729, D2-03721
    fn normal(&self, i: i32, j: i32) -> Vec3 {
        let mut sum = Vec3::ZERO;
        for (a, b) in [(i - 1, j - 1), (i, j - 1), (i - 1, j), (i, j)] {
            let (sw, se, ne, nw) = ((a, b), (a + 1, b), (a + 1, b + 1), (a, b + 1));
            let tris = if self.cut(a, b) {
                [[sw, se, ne], [sw, ne, nw]]
            } else {
                [[sw, se, nw], [ne, nw, se]]
            };
            for t in tris {
                if !t.contains(&(i, j)) {
                    continue;
                }
                let [p0, p1, p2] = t.map(|(x, y)| self.point(x, y));
                let n = normalize(cross(sub(p1, p0), sub(p2, p0)));
                sum = Vec3::new(sum.x + n.x, sum.y + n.y, sum.z + n.z);
            }
        }
        normalize(sum)
    }
}

/// Pick an entry of a weighted list at `(x, y)` with `seed`: the first entry whose running total of
/// frequencies exceeds the hash scaled to the list's total, else the last; `None` for an empty list.
// D2-12484, D2-12483
fn pick(
    list: &[WeightedMaterial],
    total: f32,
    x: i32,
    y: i32,
    seed: i32,
) -> Option<&WeightedMaterial> {
    let last = list.len().checked_sub(1)?;
    let value = scaled(x, y, seed, total);
    let mut sum = 0.0;
    for m in &list[..last] {
        sum += m.frequency;
        if value < sum {
            return Some(m);
        }
    }
    list.get(last)
}

/// The tint offset of one channel: `(1 - 2t)^power * min` below the middle, `(2t - 1)^power * max`
/// above it, nothing when both ends are zero.
fn tint(t: f32, min: f32, max: f32, power: u32) -> f32 {
    if min == 0.0 && max == 0.0 {
        return 0.0;
    }
    let (base, end) = if t < 0.5 {
        (1.0 - 2.0 * t, min)
    } else {
        (2.0 * t - 1.0, max)
    };
    let mut v = base;
    for _ in 1..power.max(1) {
        v *= base;
    }
    v * end
}

/// A vertex coloring's near and far colors at hash values `(h, s, b)`.
// D2-12470
#[must_use]
pub fn coloring_at(c: &VertexColoring, h: f32, s: f32, b: f32) -> (Color, Color) {
    let near = Color::from_argb(c.color.0);
    let far = Color::from_argb(c.far_color.0);
    if [
        c.min_hue,
        c.max_hue,
        c.min_saturation,
        c.max_saturation,
        c.min_brightness,
        c.max_brightness,
    ]
    .iter()
    .all(|&v| v == 0.0)
    {
        return (near, far);
    }
    let dh = tint(h, c.min_hue, c.max_hue, c.hue_power);
    let ds = tint(s, c.min_saturation, c.max_saturation, c.saturation_power);
    let db = tint(b, c.min_brightness, c.max_brightness, c.brightness_power);
    let (nh, ns, nb) = near.to_hsb();
    let (fh, fs, fb) = far.to_hsb();
    (
        Color::from_hsb(nh + dh, ns + ds, nb + db),
        Color::from_hsb(fh, fs, fb + 0.5 * db),
    )
}

/// The seeds of the vertex picks.
mod seed {
    pub const BASE: i32 = 0x003C_47F7;
    pub const CORNER: i32 = 9_348_347;
    pub const SIDE: i32 = 0x02F8_8C21;
    pub const THREE_CORNER: i32 = 9_853_424;
    pub const DIAGONAL: i32 = 8_534_401;
    pub const DETAIL: i32 = 9_439_293;
    pub const BUMP: i32 = 0x02E1_AB71;
    pub const COLORING: i32 = 0x05DD_39FD;
    pub const HUE: i32 = 8_843_943;
    pub const SATURATION: i32 = 0x0015_2127;
    pub const BRIGHTNESS: i32 = 0x0000_2473;
}

/// What a material group gives the vertex at global vertex `(x, y)`; `None` when its base, corner
/// or side list is empty.
// D2-12487, D2-12482
fn group_pick(g: &MaterialGroup, x: i32, y: i32) -> Option<SurfacePick> {
    let base = pick(&g.base, g.base_frequency, x, y, seed::BASE)?;
    let corner = pick(&g.corner_masks, g.corner_frequency, x, y, seed::CORNER)?;
    let side = pick(&g.side_masks, g.side_frequency, x, y, seed::SIDE)?;
    let id = |m: Option<&WeightedMaterial>| m.map_or(DataId::NONE, |m| m.material);
    let three = pick(
        &g.three_corner_masks,
        g.three_corner_frequency,
        x,
        y,
        seed::THREE_CORNER,
    );
    let diagonal = pick(
        &g.diagonal_masks,
        g.diagonal_frequency,
        x,
        y,
        seed::DIAGONAL,
    );
    let (color, far_color) = match g.vertex_colors.len() {
        0 => (Color::WHITE, Color::WHITE),
        n => {
            let v = scaled(x, y, seed::COLORING, g.vertex_color_frequency);
            let mut sum = 0.0;
            let mut chosen = &g.vertex_colors[n - 1];
            for c in &g.vertex_colors[..n - 1] {
                sum += c.frequency;
                if v < sum {
                    chosen = c;
                    break;
                }
            }
            let h = scaled(x, y, seed::HUE, 1.0);
            let s = scaled(x, y, seed::SATURATION, 1.0);
            let b = scaled(x, y, seed::BRIGHTNESS, 1.0);
            coloring_at(chosen, h, s, b)
        }
    };
    let detail = pick(&g.detail, g.detail_frequency, x, y, seed::DETAIL);
    let bump = pick(&g.bump, g.bump_frequency, x, y, seed::BUMP);
    Some(SurfacePick {
        base: base.material,
        base_tile: base.tile_width,
        corner_mask: corner.material,
        side_mask: side.material,
        three_corner_mask: id(three),
        diagonal_mask: id(diagonal),
        detail: id(detail),
        detail_tile: detail.map_or(0.0, |m| m.tile_width),
        bump: id(bump),
        bump_tile: bump.map_or(0.0, |m| m.tile_width),
        color,
        far_color,
    })
}

/// What a surface shows at a vertex of slope `pitch` (`n.x² + n.y²`): the first material group
/// whose pitch range holds it.
// D2-12465
fn surface_pick(s: &SurfaceType, pitch: f32, x: i32, y: i32) -> SurfacePick {
    s.groups
        .iter()
        .find(|g| g.min_pitch * g.min_pitch <= pitch && pitch <= g.max_pitch * g.max_pitch)
        .and_then(|g| group_pick(g, x, y))
        .unwrap_or(SurfacePick::NONE)
}

impl Surfaces<'_> {
    /// The surface a terrain kind shows: the kind (or the default kind) names a surface word, and
    /// the surface with that index is it.
    // D2-07444, D2-07445, D2-07548
    #[must_use]
    pub fn terrain_surface(&self, kind: u32) -> Option<&SurfaceType> {
        let t = self
            .terrain
            .terrains
            .iter()
            .find(|t| t.index == kind)
            .or(self.terrain.default_terrain.as_ref())?;
        self.surface.surfaces.iter().find(|s| s.index == t.surface)
    }

    /// The road surface a road value shows: the value is the position in the road list.
    // D2-07549
    #[must_use]
    pub fn road_surface(&self, road: u32) -> Option<&SurfaceType> {
        self.surface
            .roads
            .get(usize::try_from(road).ok()?)
            .filter(|s| !s.groups.is_empty())
    }
}

/// The layers of one square from its corners' terrain and road picks, in corner order SW, SE, NW,
/// NE.
// D2-10894
#[must_use]
pub fn square_layers(
    terrain: [&SurfacePick; 4],
    road: [&SurfacePick; 4],
    blending: bool,
) -> Vec<Layer> {
    use Corner::{Ne, Nw, Se, Sw};
    use MaskKind::{Corner as Cn, Diagonal, Side, ThreeCorner};
    let t = |c: Corner| terrain[c as usize];
    let eq = |a: Corner, b: Corner| t(a).same_ground(t(b));
    let ground = |c: Corner| Layer {
        ground: c,
        road: false,
        mask: None,
    };
    let masked = |c: Corner, kind: MaskKind, rotation: u8| Layer {
        ground: c,
        road: false,
        mask: Some(Mask {
            owner: c,
            kind,
            rotation,
        }),
    };
    let mut layers = if !blending {
        vec![ground(Sw)]
    } else {
        let (s, n, w, e) = (eq(Sw, Se), eq(Nw, Ne), eq(Sw, Nw), eq(Se, Ne));
        let (d1, d2) = (eq(Sw, Ne), eq(Se, Nw));
        if s && n && w {
            vec![ground(Sw)]
        } else if s && w {
            vec![ground(Sw), masked(Ne, Cn, 2)]
        } else if s && e {
            vec![ground(Sw), masked(Nw, Cn, 1)]
        } else if n && w {
            vec![ground(Ne), masked(Se, Cn, 3)]
        } else if n && e {
            vec![ground(Ne), masked(Sw, Cn, 4)]
        } else if w && e {
            vec![ground(Sw), masked(Ne, Side, 2)]
        } else if s && n {
            vec![
                ground(Nw),
                Layer {
                    ground: Se,
                    road: false,
                    mask: Some(Mask {
                        owner: Ne,
                        kind: Side,
                        rotation: 3,
                    }),
                },
            ]
        } else if s {
            vec![ground(Sw), masked(Nw, Cn, 1), masked(Ne, Cn, 2)]
        } else if n {
            vec![ground(Ne), masked(Se, Cn, 3), masked(Sw, Cn, 4)]
        } else if e {
            vec![ground(Ne), masked(Nw, Cn, 1), masked(Sw, Cn, 4)]
        } else if w {
            vec![ground(Sw), masked(Ne, Cn, 2), masked(Se, Cn, 3)]
        } else if d1 {
            vec![ground(Sw), masked(Nw, Cn, 1), masked(Se, Cn, 3)]
        } else if d2 {
            vec![ground(Se), masked(Sw, Cn, 4), masked(Ne, Cn, 2)]
        } else {
            vec![ground(Sw), masked(Nw, Cn, 1), masked(Ne, Cn, 2)]
        }
    };

    // Roads: a pair is two corners with the same road ground.
    let r = |c: Corner| road[c as usize];
    let has = |c: Corner| !r(c).base.is_none();
    let pair = |a: Corner, b: Corner| has(a) && r(a).base == r(b).base;
    let road_layer = |c: Corner, kind: MaskKind, rotation: u8| Layer {
        ground: c,
        road: true,
        mask: Some(Mask {
            owner: c,
            kind,
            rotation,
        }),
    };
    let (rn, re, rs, rw) = (pair(Nw, Ne), pair(Se, Ne), pair(Sw, Se), pair(Sw, Nw));
    if rn && re && rs && rw {
        return vec![Layer {
            ground: Ne,
            road: true,
            mask: None,
        }];
    }
    if !blending {
        return layers;
    }
    let road = if rn && re {
        Some(road_layer(Ne, ThreeCorner, 2))
    } else if rs && re {
        Some(road_layer(Se, ThreeCorner, 3))
    } else if rs && rw {
        Some(road_layer(Sw, ThreeCorner, 4))
    } else if rn && rw {
        Some(road_layer(Nw, ThreeCorner, 1))
    } else if rn {
        Some(road_layer(Nw, Side, 1))
    } else if re {
        Some(road_layer(Ne, Side, 2))
    } else if rs {
        Some(road_layer(Se, Side, 3))
    } else if rw {
        Some(road_layer(Nw, Side, 4))
    } else if pair(Sw, Ne) {
        Some(road_layer(Sw, Diagonal, 1))
    } else if pair(Se, Nw) {
        Some(road_layer(Se, Diagonal, 2))
    } else {
        [(Nw, 1), (Ne, 2), (Se, 3), (Sw, 4)]
            .into_iter()
            .find(|&(c, _)| has(c))
            .map(|(c, rot)| road_layer(c, Cn, rot))
    };
    if let Some(road) = road {
        if layers.len() < 3 {
            layers.push(road);
        }
    }
    layers
}

/// A block's physics materials, x major over its vertices: the ground's by its slope there, and the
/// road's.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VertexMaterials {
    pub ground: Vec<u32>,
    pub road: Vec<u32>,
}

/// The physics material of `surface` at slope `pitch` (the squared length of the normal's level
/// part): that of its first group the slope falls in, else none.
// D2-12466
#[must_use]
pub fn surface_material(surface: &SurfaceType, pitch: f32) -> u32 {
    surface
        .groups
        .iter()
        .find(|g| g.min_pitch * g.min_pitch <= pitch && pitch <= g.max_pitch * g.max_pitch)
        .map_or(0, |g| g.physics_material.0)
}

/// Block `(x, y)`'s physics materials, or `None` when the world has no landblock there.
// D2-07550, D2-07551
#[must_use]
pub fn vertex_materials(
    land: &LandConstants,
    surfaces: Surfaces<'_>,
    source: &dyn LandSource,
    x: i32,
    y: i32,
) -> Option<VertexMaterials> {
    let lb = source.landblock(x, y)?;
    let hood = Neighborhood::new(land, source, x, y);
    let n = hood.side();
    let mut out = VertexMaterials::default();
    for i in 0..n {
        for j in 0..n {
            let normal = hood.normal(i, j);
            let pitch = normal.x * normal.x + normal.y * normal.y;
            let word = lb.vertices[usize::try_from(n * i + j).unwrap_or(0)];
            out.ground.push(
                surfaces
                    .terrain_surface(land.word.terrain.of(word))
                    .map_or(0, |s| surface_material(s, pitch)),
            );
            let road = land.word.road.of(word);
            out.road.push(
                surfaces
                    .surface
                    .roads
                    .iter()
                    .find(|s| s.index == road)
                    .map_or(0, |s| surface_material(s, pitch)),
            );
        }
    }
    Some(out)
}

/// Build block `(x, y)`'s terrain, or `None` when the world has no landblock there.
// D2-03759, D2-03763, D2-10985
#[must_use]
pub fn build_block(
    land: &LandConstants,
    surfaces: Surfaces<'_>,
    source: &dyn LandSource,
    x: i32,
    y: i32,
) -> Option<TerrainBlock> {
    let lb = source.landblock(x, y)?;
    let grid = land.grid();
    let side = grid.vertices_per_side();
    let holes: Vec<bool> = lb
        .vertices
        .iter()
        .map(|&w| land.word.terrain.of(w) == HOLE_TERRAIN)
        .collect();
    let geometry = land_geometry(
        &land.spec,
        &lb.heights,
        &land.heights,
        x,
        y,
        grid.cells_per_side,
        Some(&holes),
    );
    let hood = Neighborhood::new(land, source, x, y);
    let n = hood.side();
    let mut normals = Vec::with_capacity(side * side);
    let mut terrain = Vec::with_capacity(side * side);
    let mut road = Vec::with_capacity(side * side);
    let mut shadow_bits = Vec::with_capacity(side * side);
    let squares_per_side = n - 1;
    for i in 0..n {
        for j in 0..n {
            let normal = hood.normal(i, j);
            normals.push(normal);
            let word = lb.vertices[usize::try_from(n * i + j).unwrap_or(0)];
            let pitch = normal.x * normal.x + normal.y * normal.y;
            let (gx, gy) = (x * squares_per_side + i, y * squares_per_side + j);
            let t = surfaces
                .terrain_surface(land.word.terrain.of(word))
                .map_or(SurfacePick::NONE, |s| surface_pick(s, pitch, gx, gy));
            terrain.push(t);
            let r = surfaces
                .road_surface(land.word.road.of(word))
                .map_or(SurfacePick::NONE, |s| surface_pick(s, pitch, gx, gy));
            road.push(r);
            shadow_bits.push(land.word.color.of(word));
        }
    }
    let squares_n = grid.squares_per_side();
    let mut squares = Vec::with_capacity(squares_n * squares_n);
    for i in 0..squares_n {
        for j in 0..squares_n {
            let at = |c: Corner| {
                let (di, dj) = c.offset();
                (i + di) * side + j + dj
            };
            let corners = [Corner::Sw, Corner::Se, Corner::Nw, Corner::Ne].map(at);
            let hole = geometry.holes[i * squares_n + j];
            let layers = if hole {
                Vec::new()
            } else {
                square_layers(
                    corners.map(|k| &terrain[k]),
                    corners.map(|k| &road[k]),
                    true,
                )
            };
            squares.push(SquareLook { hole, layers });
        }
    }
    Some(TerrainBlock {
        x,
        y,
        geometry,
        normals,
        terrain,
        road,
        shadow_bits,
        squares,
    })
}

impl TerrainBlock {
    /// The index of vertex `(i, j)` in the per-vertex arrays.
    #[must_use]
    pub fn vertex(&self, i: usize, j: usize) -> usize {
        i * self.geometry.vertices_per_side + j
    }

    /// The pick a layer shows at a corner of square `(i, j)`.
    #[must_use]
    pub fn layer_pick(&self, i: usize, j: usize, layer: &Layer) -> &SurfacePick {
        let (di, dj) = layer.ground.offset();
        let k = self.vertex(i + di, j + dj);
        if layer.road {
            &self.road[k]
        } else {
            &self.terrain[k]
        }
    }

    /// The mask picture of a layer of square `(i, j)`, if it has one.
    #[must_use]
    pub fn layer_mask(&self, i: usize, j: usize, layer: &Layer) -> Option<(DataId, u8)> {
        let m = layer.mask?;
        let (di, dj) = m.owner.offset();
        let k = self.vertex(i + di, j + dj);
        let p = if layer.road {
            &self.road[k]
        } else {
            &self.terrain[k]
        };
        let id = match m.kind {
            MaskKind::Corner => p.corner_mask,
            MaskKind::Side => p.side_mask,
            MaskKind::ThreeCorner => p.three_corner_mask,
            MaskKind::Diagonal => p.diagonal_mask,
        };
        Some((id, m.rotation))
    }
}

impl<S: std::hash::BuildHasher> LandSource for std::collections::HashMap<(i32, i32), Landblock, S> {
    fn landblock(&self, x: i32, y: i32) -> Option<&Landblock> {
        self.get(&(x, y))
    }
}
