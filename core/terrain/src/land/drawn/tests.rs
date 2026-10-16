//! Behaviour: none (the drawn ground's offset, an art-pack presentation's own)

use super::*;

/// A landscape from a function of the global vertex coordinates.
struct Hills<F: Fn(i32, i32) -> (f32, u16)>(F);

impl<F: Fn(i32, i32) -> (f32, u16)> GroundSource for Hills<F> {
    fn vertex(&self, gx: i32, gy: i32) -> Option<Vertex> {
        let (height, word) = (self.0)(gx, gy);
        Some(Vertex { height, word })
    }
}

/// Rolling grass, heights in two-meter steps as retail's are, walkable everywhere: rises of up
/// to 14 meters a cell.
fn rolling() -> Hills<impl Fn(i32, i32) -> (f32, u16)> {
    Hills(|gx, gy| {
        // LINT-OK: vertex coordinates, exact in f32.
        #[allow(clippy::cast_precision_loss)]
        let (x, y) = (gx as f32, gy as f32);
        let h = 100.0 + 20.0 * math::sinf(x * 0.31) + 16.0 * math::cosf(y * 0.23 + x * 0.07);
        ((h / 2.0).round() * 2.0, 1 << 2)
    })
}

/// A mesa: a flat top, walkable; faces of 60 to 70 degrees all round it, unwalkable; flat land
/// at its foot.
fn mesa() -> Hills<impl Fn(i32, i32) -> (f32, u16)> {
    Hills(|gx, gy| {
        let (cx, cy) = (gx - 84, gy - 84);
        let r = cx.abs().max(cy.abs());
        let h = match r {
            0..=2 => 200.0,
            3 => 160.0,
            4 => 120.0,
            5 => 80.0,
            _ => 40.0,
        };
        (h, 13 << 2)
    })
}

fn knobs(f: impl FnOnce(&mut Knobs)) -> Knobs {
    let mut k = Knobs::default();
    f(&mut k);
    k
}

/// The block-local sample of block `(bx, by)`'s field at global vertex `(gx, gy)`.
fn vertex_sample(field: &BlockField, gx: i32, gy: i32) -> Option<f32> {
    let (i, j) = (gx - field.block.0 * 8, gy - field.block.1 * 8);
    if !(0..=8).contains(&i) || !(0..=8).contains(&j) {
        return None;
    }
    // LINT-OK: inside the block, checked above.
    #[allow(clippy::cast_sign_loss)]
    let at = (j as usize * 24) * SIDE + i as usize * 24;
    Some(field.offset[at])
}

#[test]
fn walkable_ground_is_drawn_through_every_retail_vertex() {
    let src = rolling();
    // The offset is nought at every vertex: the smooth surface passes through every one.
    let f = block_field(&src, (10, 7), &[], &Knobs::default());
    for gx in 80..=88 {
        for gy in 56..=64 {
            let v = vertex_sample(&f, gx, gy).expect("a vertex of the block");
            assert!(v.abs() < 1e-4, "({gx}, {gy}): {v}");
        }
    }
    let largest = f.offset.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    assert!(
        largest > 0.2,
        "the ground between the vertices is rounded: {largest}"
    );
}

#[test]
fn a_body_stands_on_the_offset_averaged_over_its_stance() {
    let src = rolling();
    let f = block_field(&src, (10, 7), &[], &Knobs::default());
    let at = |x: f32, y: f32| f.at(x, y).0;
    // Where the offset is level the body stands on it as it is; a plane is its own average.
    let plane = |x: f32, y: f32| 0.3 + 0.01 * x - 0.02 * y;
    let k = Knobs::default();
    assert!((body_offset(plane, 50.0, 60.0, &k) - plane(50.0, 60.0)).abs() < 1e-5);
    // Without the low-pass, the offset under its middle.
    let sharp = knobs(|k| {
        k.set(Knob::WalkLowpass, 0.0);
    });
    assert!((body_offset(at, 50.3, 60.7, &sharp) - at(50.3, 60.7)).abs() < 1e-6);
    // With it, between the least and the most of the offset round it.
    let r = k.get(Knob::WalkLowpass) * 0.5;
    let near: Vec<f32> = (0..=20)
        .flat_map(|i| (0..=20).map(move |j| (i, j)))
        .map(|(i, j)| {
            // LINT-OK: a few dozen grid points.
            #[allow(clippy::cast_precision_loss)]
            let (dx, dy) = ((i as f32 / 10.0 - 1.0) * r, (j as f32 / 10.0 - 1.0) * r);
            at(50.3 + dx, 60.7 + dy)
        })
        .collect();
    let b = body_offset(at, 50.3, 60.7, &k);
    let lo = near.iter().copied().fold(f32::MAX, f32::min);
    let hi = near.iter().copied().fold(f32::MIN, f32::max);
    assert!(lo - 1e-5 <= b && b <= hi + 1e-5, "{lo} {b} {hi}");
}

#[test]
fn walkable_ground_never_leaves_retail_by_more_than_its_cap() {
    let src = rolling();
    for cap in [0.3, 1.0, 2.0] {
        let f = block_field(
            &src,
            (10, 7),
            &[],
            &knobs(|k| {
                k.set(Knob::WalkCap, cap);
            }),
        );
        let most = f.offset.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(most <= cap + 1e-4, "cap {cap}: {most}");
        assert!(
            most > cap * 0.3,
            "cap {cap}: the rounding reaches toward it, {most}"
        );
    }
    // No rounding at all: retail's facets.
    let flat = block_field(
        &src,
        (10, 7),
        &[],
        &knobs(|k| {
            k.set(Knob::Round, 0.0);
        }),
    );
    assert!(flat.offset.iter().all(|v| v.abs() < 1e-6));
}

#[test]
fn the_ground_is_kept_at_retails_height_at_every_pin_and_rises_over_the_fade() {
    let src = rolling();
    // A building's round in the middle of block (10, 7), 6 meters across with its margin.
    let (px, py) = (10.0 * 192.0 + 100.0, 7.0 * 192.0 + 90.0);
    let pins = [Pin::round(px, py, 6.0)];
    let f = block_field(&src, (10, 7), &pins, &Knobs::default());
    for j in 0..SIDE {
        for i in 0..SIDE {
            // LINT-OK: sample indices.
            #[allow(clippy::cast_precision_loss)]
            let d = ((i as f32 - 100.0).powi(2) + (j as f32 - 90.0).powi(2)).sqrt();
            let k = j * SIDE + i;
            if d <= 6.0 {
                assert!(f.offset[k].abs() < 1e-6 && f.weight[k] < 1e-6, "{i} {j}");
            } else if d > 6.0 + 8.0 + 1.5 {
                assert!(f.weight[k] > 0.999, "{i} {j}: {}", f.weight[k]);
            }
        }
    }
    // The fade rises steadily from the pin's edge.
    let w = |d: f32| f.at(100.0 + 6.0 + d, 90.0).1;
    assert!(
        w(1.0) < w(4.0) && w(4.0) < w(7.0),
        "{} {} {}",
        w(1.0),
        w(4.0),
        w(7.0)
    );
    // Roads: a road along the cells' west edges of a column of vertices.
    let road = Hills(|gx, gy| {
        let (h, _) = (rolling().0)(gx, gy);
        (h, if gx == 84 { (1 << 2) | 1 } else { 1 << 2 })
    });
    let f = block_field(&road, (10, 7), &[], &Knobs::default());
    // On the road (within 5 m of the vertex line at x = 96) the ground is retail's.
    for j in 0..SIDE {
        for x in [92usize, 96, 100] {
            assert!(f.offset[j * SIDE + x].abs() < 1e-6, "{x} {j}");
        }
    }
    // And without the road pinned it is not.
    let free = block_field(
        &road,
        (10, 7),
        &[],
        &knobs(|k| {
            k.set(Knob::PinRoads, 0.0);
        }),
    );
    assert!((0..SIDE).any(|j| free.offset[j * SIDE + 92].abs() > 0.05));
}

#[test]
fn two_blocks_side_by_side_draw_their_shared_edge_alike() {
    for src in [&rolling() as &dyn GroundSource, &mesa()] {
        let pins = [
            Pin::round(11.0 * 192.0 + 3.0, 7.0 * 192.0 + 60.0, 4.0),
            Pin {
                x: 11.0 * 192.0 - 2.0,
                y: 7.0 * 192.0 + 120.0,
                half: (6.0, 2.0),
                axis: (0.8, 0.6),
                margin: 1.0,
            },
        ];
        let west = block_field(src, (10, 7), &pins, &Knobs::default());
        let east = block_field(src, (11, 7), &pins, &Knobs::default());
        let south = block_field(src, (10, 6), &pins, &Knobs::default());
        for j in 0..SIDE {
            let (a, b) = (west.offset[j * SIDE + TILE], east.offset[j * SIDE]);
            assert!((a - b).abs() < 1e-4, "east edge, {j}: {a} {b}");
            let (a, b) = (west.weight[j * SIDE + TILE], east.weight[j * SIDE]);
            assert!((a - b).abs() < 1e-4, "east edge weight, {j}: {a} {b}");
        }
        for i in 0..SIDE {
            let (a, b) = (west.offset[i], south.offset[TILE * SIDE + i]);
            assert!((a - b).abs() < 1e-4, "south edge, {i}: {a} {b}");
        }
    }
}

#[test]
fn rock_bands_leave_the_brink_at_retails_surface_and_keep_within_their_cap() {
    let src = mesa();
    // The mesa stands about global vertex (84, 84): block (10, 10), whose south-west vertex is
    // (80, 80).
    let k = knobs(|k| {
        k.set(Knob::WalkLowpass, 0.0);
    });
    let f = block_field(&src, (10, 10), &[], &k);
    let cap = k.get(Knob::CliffCap).max(k.get(Knob::CliffCapTall));
    let mut on_faces = 0usize;
    let mut largest = 0.0f32;
    for j in 0..SIDE {
        for i in 0..SIDE {
            // LINT-OK: sample indices.
            #[allow(clippy::cast_precision_loss)]
            let (x, y) = (10.0 * 192.0 + i as f32, 10.0 * 192.0 + j as f32);
            let p = Point::at(&src, x, y);
            let v = f.offset[j * SIDE + i];
            assert!(v.abs() <= cap + 1e-3, "{i} {j}: {v}");
            if !p.walkable {
                on_faces += 1;
                largest = largest.max(v.abs());
            }
        }
    }
    assert!(on_faces > 1000, "the mesa has faces: {on_faces}");
    assert!(largest > 1.0, "the faces are banded: {largest}");
    // The band's room is nought within the edge of walkable ground: just past the top's east rim
    // (block-local x = 144) the offset is no more than the walkable rounding's.
    let rim = f.at(144.5, 96.0).0;
    assert!(rim.abs() <= k.get(Knob::WalkCap) + 1e-3, "{rim}");
    // Without the bands, unwalkable ground is rounded and capped as walkable ground is.
    let plain = block_field(
        &src,
        (10, 10),
        &[],
        &knobs(|k| {
            k.set(Knob::Cliffs, 0.0);
            k.set(Knob::WalkLowpass, 0.0);
        }),
    );
    let most = plain.offset.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    assert!(most <= k.get(Knob::WalkCap) + 1e-4, "{most}");
}

#[test]
fn a_band_cuts_flat_treads_and_steep_risers_centered_on_retails_surface() {
    // Along a slope, the drawn height (retail plus the offset) holds level over a tread and
    // climbs a whole band over the riser.
    let band = 8.0;
    let drawn = |h: f32| h + band_offset(h, band, 0.25, 0.0);
    let tread: Vec<f32> = (0..=10).map(|k| drawn(16.0 + 0.5 * k as f32)).collect();
    assert!(
        tread.windows(2).all(|w| (w[1] - w[0]).abs() < 1e-4),
        "{tread:?}"
    );
    assert!((drawn(24.0 - 1e-3) - drawn(16.0)) > 7.9);
    let worst = (0..800).fold(0.0f32, |m, k| {
        // LINT-OK: a few hundred steps.
        #[allow(clippy::cast_precision_loss)]
        let h = k as f32 * 0.1;
        m.max(band_offset(h, band, 0.25, 0.0).abs())
    });
    assert!((worst - band * 0.75 * 0.5).abs() < 0.05, "{worst}");
}

#[test]
fn the_far_ground_takes_the_whole_rounding_and_gives_way_to_it_across_the_band() {
    let src = rolling();
    let k = Knobs::default();
    let (x, y) = (10.0 * 192.0 + 37.0, 7.0 * 192.0 + 11.0);
    let p = Point::at(&src, x, y);
    let mut no_relief = k.clone();
    no_relief.set(Knob::FarDetail, 0.0);
    assert!((far_offset(&p, x, y, &no_relief) - (p.smooth - p.retail)).abs() < 1e-5);
    assert!(morph(k.get(Knob::MorphStart) - 1.0, &k) < 1e-6);
    assert!(morph(k.get(Knob::MorphEnd) + 1.0, &k) > 1.0 - 1e-6);
    let mid = morph(0.5 * (k.get(Knob::MorphStart) + k.get(Knob::MorphEnd)), &k);
    assert!((mid - 0.5).abs() < 1e-4, "{mid}");
    let mut near_only = k;
    near_only.set(Knob::Far, 0.0);
    assert!(morph(10_000.0, &near_only) < 1e-6);
}

#[test]
fn a_footprint_is_pinned_along_its_own_turned_sides_and_its_margin_round_them() {
    // A wall 12 meters long and 2 deep, turned so its length runs north-east, with a meter round.
    let wall = Pin {
        x: 100.0,
        y: 50.0,
        half: (6.0, 1.0),
        axis: (0.8, 0.6),
        margin: 1.0,
    };
    // Along its length, to its end and the margin past it.
    assert!(wall.distance(100.0 + 0.8 * 6.9, 50.0 + 0.6 * 6.9) <= 0.0);
    assert!(wall.distance(100.0 + 0.8 * 7.1, 50.0 + 0.6 * 7.1) > 0.0);
    // Across it: a meter deep each side and the margin.
    assert!(wall.distance(100.0 - 0.6 * 1.9, 50.0 + 0.8 * 1.9) <= 0.0);
    assert!(wall.distance(100.0 - 0.6 * 2.1, 50.0 + 0.8 * 2.1) > 0.0);
    // Round its corner, the margin keeps its distance.
    let corner = (100.0 + 0.8 * 6.0 - 0.6 * 1.0, 50.0 + 0.6 * 6.0 + 0.8 * 1.0);
    let out = (corner.0 + 0.6, corner.1 + 0.6);
    assert!((wall.distance(out.0, out.1) - (0.72f32.sqrt() - 1.0)).abs() < 1e-4);
    assert!((wall.reach() - (37.0f32.sqrt() + 1.0)).abs() < 1e-5);
}
