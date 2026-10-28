//! Terrain brushes: what one dab of a brush sets, vertex by vertex, over the world grid.
//!
//! A brush covers every world vertex within its radius of its center, whichever landblocks they
//! lie in; [`crate::edit`] writes each one where it belongs, so a dab across a landblock's edge or
//! corner changes every copy of the vertices it covers. Heights move over the region's height
//! table: raising by one is the next entry of the table, and smoothing settles each vertex on the
//! entry nearest the mean of it and its eight neighbors.

use dereth_landscape::WorldGrid;

use crate::compile::Field;
use crate::edit::{Session, VertexSet};

/// The world's grid of vertices: 255 landblocks a side, each 8 cells of 24 meters with a vertex at
/// every corner.
pub const WORLD: WorldGrid = WorldGrid::DEFAULT;

/// The spacing of the world grid, in meters.
pub const GRID: f32 = WORLD.spacing();

/// The highest world grid point on either axis: 255 landblocks of 8 cells.
pub const GRID_MAX: u32 = WORLD.last_point();

/// What a brush does to each vertex it covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Brush {
    /// Up the height table by this many entries.
    Raise(u8),
    /// Down the height table by this many entries.
    Lower(u8),
    /// To one height-table entry: the one given, or the center vertex's.
    Flatten(Option<u8>),
    /// To the entry nearest the mean of the vertex and its eight neighbors.
    Smooth,
    /// The terrain type, 0 to 31.
    Terrain(u16),
    /// The scene type, 0 to 31.
    Scene(u16),
    /// A road (any value but 0; a vertex on a road already keeps its own), or none (0).
    Road(u16),
}

impl Brush {
    /// Whether the brush sets a value at each vertex it covers (a terrain type, a scene type, a
    /// road) rather than moving the ground: the world editor centers it on a vertex and sizes it
    /// in whole vertices.
    #[must_use]
    pub const fn by_vertex(self) -> bool {
        matches!(self, Self::Terrain(_) | Self::Scene(_) | Self::Road(_))
    }
}

/// The radius brush `b` takes from `r` meters moved `steps` steps: whole vertices, a step of one
/// grid space, as the ground has a height, a terrain and a scene only at its vertices. A brush
/// that sets each vertex goes from none (the one vertex under it) to ten; one that moves the
/// ground, from one to ten.
#[must_use]
pub fn step_radius(b: Brush, r: f32, steps: i32) -> f32 {
    #[allow(clippy::cast_precision_loss)] // a few steps
    let steps = steps as f32;
    let least = if b.by_vertex() { 0.0 } else { GRID };
    ((r / GRID).round() * GRID + steps * GRID).clamp(least, 10.0 * GRID)
}

/// The radius a brush starts with when it is chosen over brush `from` at `r` meters: the road
/// brush starts at the one vertex under it, as a road is painted vertex by vertex; any other keeps
/// the radius it had, sized as it is sized ([`step_radius`]). Painting and erasing road are one
/// brush, so turning from one to the other keeps the radius too.
#[must_use]
pub fn chosen_radius(from: Brush, to: Brush, r: f32) -> f32 {
    if matches!(to, Brush::Road(_)) && !matches!(from, Brush::Road(_)) {
        0.0
    } else {
        step_radius(to, r, 0)
    }
}

/// The world vertex nearest world meters `(x, y)`, in world meters.
#[must_use]
pub fn nearest_vertex(x: f32, y: f32) -> (f32, f32) {
    WORLD.nearest_vertex(x, y)
}

/// Every world vertex within `radius` meters of world meters `(x, y)`, nearest the center first
/// (ties by grid order).
#[must_use]
pub fn covered(x: f32, y: f32, radius: f32) -> Vec<(u32, u32)> {
    WORLD.within(x, y, radius)
}

/// The height-table entry nearest `z` meters.
#[must_use]
pub fn nearest_height(table: &[f32], z: f32) -> u8 {
    let mut best = (f32::INFINITY, 0usize);
    for (i, h) in table.iter().enumerate().take(256) {
        let d = (h - z).abs();
        if d < best.0 {
            best = (d, i);
        }
    }
    u8::try_from(best.1).unwrap_or(u8::MAX)
}

/// What one dab of `brush` at world meters `(x, y)` with radius `radius` sets: one entry for each
/// covered vertex whose value it changes, and none for one it leaves as it is.
#[must_use]
pub fn dab(session: &Session, brush: Brush, x: f32, y: f32, radius: f32) -> Vec<VertexSet> {
    let table = session
        .region()
        .map(dereth_terrain::land::mesh::height_table);
    let points = covered(x, y, radius);
    let center = points.first().copied();
    let mut out = Vec::new();
    for (gx, gy) in points {
        let Some(v) = session.vertex(gx, gy) else {
            continue;
        };
        let (field, now, new) = match brush {
            Brush::Raise(n) => (Field::Height, v.height, v.height.saturating_add(n)),
            Brush::Lower(n) => (Field::Height, v.height, v.height.saturating_sub(n)),
            Brush::Flatten(to) => {
                let Some(to) = to.or_else(|| {
                    center
                        .and_then(|(cx, cy)| session.vertex(cx, cy))
                        .map(|c| c.height)
                }) else {
                    continue;
                };
                (Field::Height, v.height, to)
            }
            Brush::Smooth => {
                let Some(table) = table.as_ref() else {
                    continue;
                };
                let mut sum = 0.0;
                let mut n = 0.0;
                for dx in -1i64..=1 {
                    for dy in -1i64..=1 {
                        let (Ok(nx), Ok(ny)) = (
                            u32::try_from(i64::from(gx) + dx),
                            u32::try_from(i64::from(gy) + dy),
                        ) else {
                            continue;
                        };
                        if let Some(w) = session.vertex(nx, ny) {
                            sum += w.z;
                            n += 1.0;
                        }
                    }
                }
                (Field::Height, v.height, nearest_height(table, sum / n))
            }
            Brush::Terrain(t) => {
                out.extend((v.terrain != t).then_some(VertexSet {
                    gx,
                    gy,
                    field: Field::Terrain,
                    value: t,
                }));
                continue;
            }
            Brush::Scene(s) => {
                out.extend((v.scene != s).then_some(VertexSet {
                    gx,
                    gy,
                    field: Field::Scene,
                    value: s,
                }));
                continue;
            }
            Brush::Road(r) => {
                let kept = r != 0 && v.road != 0;
                out.extend((!kept && v.road != r).then_some(VertexSet {
                    gx,
                    gy,
                    field: Field::Road,
                    value: r,
                }));
                continue;
            }
        };
        if now != new {
            out.push(VertexSet {
                gx,
                gy,
                field,
                value: u16::from(new),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the reach of a terrain brush)
    use super::*;

    #[test]
    fn a_brush_covers_the_grid_points_within_its_radius_nearest_first() {
        // At a vertex, a radius just under the grid covers that vertex alone, and one just over
        // it its four nearest neighbors too.
        assert_eq!(covered(48.0, 48.0, 23.0), [(2, 2)]);
        assert_eq!(
            covered(48.0, 48.0, 25.0),
            [(2, 2), (1, 2), (2, 1), (2, 3), (3, 2)]
        );
        // Between four vertices, a radius over half a diagonal covers all four.
        assert_eq!(covered(12.0, 12.0, 17.0), [(0, 0), (0, 1), (1, 0), (1, 1)]);
        // Never off the world.
        assert!(covered(1.0, 1.0, 30.0)
            .iter()
            .all(|(x, y)| *x <= 1 && *y <= 1));
    }

    #[test]
    fn every_brush_is_sized_in_whole_vertices_and_one_that_sets_vertices_is_centered_on_one() {
        let road = Brush::Road(1);
        assert!(road.by_vertex() && Brush::Terrain(3).by_vertex() && Brush::Scene(0).by_vertex());
        assert!(!Brush::Raise(1).by_vertex() && !Brush::Smooth.by_vertex());
        // From a height brush's 30 meters, the nearest whole vertex; then a vertex a step, down
        // to the one vertex under it and up to ten.
        assert_eq!(step_radius(road, 30.0, 0), 24.0);
        assert_eq!(step_radius(road, 24.0, 1), 48.0);
        assert_eq!(step_radius(road, 24.0, -1), 0.0);
        assert_eq!(step_radius(road, 0.0, -1), 0.0);
        assert_eq!(step_radius(road, 240.0, 1), 240.0);
        assert_eq!(covered(48.0, 48.0, step_radius(road, 0.0, 0)), [(2, 2)]);
        // A height brush too, from one vertex.
        assert_eq!(step_radius(Brush::Raise(1), 0.0, 0), 24.0);
        assert_eq!(step_radius(Brush::Raise(1), 30.0, 1), 48.0);
        assert_eq!(step_radius(Brush::Smooth, 24.0, -1), 24.0);
        // The vertex nearest a point, never off the world.
        assert_eq!(nearest_vertex(35.0, 37.0), (24.0, 48.0));
        assert_eq!(nearest_vertex(-5.0, 1e6), (0.0, 2040.0 * GRID));
    }

    #[test]
    fn the_road_brush_starts_at_one_vertex_and_keeps_its_size_between_painting_and_erasing() {
        for from in [Brush::Raise(1), Brush::Terrain(3), Brush::Smooth] {
            let r = chosen_radius(from, Brush::Road(1), 48.0);
            assert_eq!(covered(48.0, 48.0, r), [(2, 2)], "{from:?}");
        }
        assert_eq!(chosen_radius(Brush::Road(1), Brush::Road(0), 48.0), 48.0);
        assert_eq!(chosen_radius(Brush::Road(0), Brush::Road(1), 24.0), 24.0);
        // Another brush keeps the size it had, in its own steps.
        assert_eq!(
            chosen_radius(Brush::Raise(1), Brush::Terrain(2), 30.0),
            24.0
        );
        assert_eq!(chosen_radius(Brush::Road(1), Brush::Raise(1), 0.0), 24.0);
    }

    #[test]
    fn a_height_settles_on_the_nearest_entry_of_the_table() {
        let table = [0.0, 2.0, 4.0, 8.0];
        assert_eq!(nearest_height(&table, 2.9), 1);
        assert_eq!(nearest_height(&table, 3.1), 2);
        assert_eq!(nearest_height(&table, 100.0), 3);
    }
}
