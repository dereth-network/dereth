//! The drawn ground the shader draws beside and under the water is the ground the scene stands
//! bodies on.
//!
//! Behaviour: none (the drawn ground, an art-pack presentation's own: the presentation draws the
//! ground from the same numbers the scene moves what stands on it by)

use std::sync::Arc;

use dereth_render_hifi::passes::drawn::heights_on_device;
use dereth_render_hifi::snapshot::{
    HifiDrawnGround, HifiFrame, HifiGroundKnobs, HifiGroundTile, HifiWater, HifiWorldGround,
};
use dereth_terrain::land::drawn::{self, shore, GroundSource, Knob, Knobs, Point, Vertex};
use glam::Vec2;

/// The world's landscape vertices a side.
const SIDE: usize = 2041;
/// The block the ground is drawn about.
const BLOCK: (i32, i32) = (100, 100);

/// A river running north through block (100, 100): a hillside eight meters a cell rising to the
/// west, whose foot meets a flat bank two cells wide, so the rounded ground dips under retail's
/// there, 36 meters from the water; the river at 28 meters, eight cells wide; and a gentle bank
/// rising to the east.
struct Valley {
    heights: Vec<f32>,
    words: Vec<u16>,
}

impl Valley {
    fn new() -> Self {
        let mut heights = vec![0.0f32; SIDE * SIDE];
        let mut words = vec![1u16 << 2; SIDE * SIDE];
        for gy in 760..860usize {
            for gx in 760..860usize {
                // LINT-OK: vertex coordinates, exact in f32.
                #[allow(clippy::cast_precision_loss)]
                let x = gx as f32;
                let (h, water) = match gx {
                    0..=803 => (28.0 + 8.0 * (804.0 - x), false),
                    804..=805 => (28.0, false),
                    806..=813 => (28.0, true),
                    _ => (28.0 + 2.0 * (x - 813.0), false),
                };
                heights[gy * SIDE + gx] = h;
                if water {
                    words[gy * SIDE + gx] = 17 << 2;
                }
            }
        }
        Self { heights, words }
    }
}

impl GroundSource for Valley {
    fn vertex(&self, gx: i32, gy: i32) -> Option<Vertex> {
        let side = i32::try_from(SIDE).ok()?;
        if !(0..side).contains(&gx) || !(0..side).contains(&gy) {
            return None;
        }
        let k = usize::try_from(gy).ok()? * SIDE + usize::try_from(gx).ok()?;
        Some(Vertex {
            height: self.heights[k],
            word: self.words[k],
        })
    }
}

#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "draws on the hardware GPU: --features retail-dats, one GPU run at a time"
)]
fn the_ground_beside_and_under_the_water_is_drawn_where_the_scene_stands_bodies() {
    let (device, queue) = crate::hardware();
    let valley = Valley::new();
    let field = shore::held(&shore::water_field(SIDE, &valley.heights, &valley.words));
    let mut knobs = Knobs::default();
    // A body stands on the ground under its middle, so the two are compared exactly.
    knobs.set(Knob::WalkLowpass, 0.0);
    knobs.set(Knob::Far, 0.0);
    let block = drawn::block_field(&valley, BLOCK, &[], &knobs);
    let water = drawn::Water {
        shore: shore::Shore {
            side: SIDE,
            heights: &valley.heights,
            field: &field,
        },
        depth: knobs.get(Knob::WaterDepth),
        shelf: knobs.get(Knob::WaterShelf),
    };
    let ground = HifiDrawnGround {
        world: Arc::new(HifiWorldGround {
            serial: 1,
            // LINT-OK: 2041.
            #[allow(clippy::cast_possible_truncation)]
            side: SIDE as u32,
            heights: Arc::new(valley.heights.clone()),
            words: Arc::new(valley.words.clone()),
            water: Arc::new(field.clone()),
        }),
        tiles: vec![HifiGroundTile {
            // LINT-OK: block coordinates, 0..255.
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
            block: ((BLOCK.0 as u16) << 8) | BLOCK.1 as u16,
            generation: 1,
            offset: Arc::new(block.offset.clone()),
            weight: Arc::new(block.weight.clone()),
        }],
        reach: 1,
        global: Vec2::ZERO,
        knobs: HifiGroundKnobs {
            micro: (0.0, 0.7),
            water: Some(HifiWater::default()),
            ..HifiGroundKnobs::default()
        },
        bodies: Vec::new(),
        cover: None,
    };
    // Across the hillside's foot, the bank, the river and the far bank, along three lines.
    // LINT-OK: block coordinates, exact in f32.
    #[allow(clippy::cast_precision_loss)]
    let (ox, oy) = (BLOCK.0 as f32 * 192.0, BLOCK.1 as f32 * 192.0);
    let mut points = Vec::new();
    for row in [41.25, 96.5, 150.75] {
        for k in 0..100u8 {
            points.push([ox + 40.0 + f32::from(k) * 1.5, oy + row, 30.0]);
        }
    }
    let shader = heights_on_device(&device, &queue, &HifiFrame::default(), &ground, &points);
    assert_eq!(shader.len(), points.len(), "the probe read back");
    let (mut dipped, mut wet, mut worst) = (0, 0, 0.0f32);
    for (q, drawn_h) in points.iter().zip(&shader) {
        let (x, y) = (q[0], q[1]);
        let p = Point::at(&valley, x, y);
        let near = drawn::sample(&block.offset, x - ox, y - oy);
        let scene = drawn::drawn_at(&p, x, y, near, 0.0, Some(&water), &knobs);
        let off = (scene.height - drawn_h).abs();
        worst = worst.max(off);
        assert!(
            off < 2e-3,
            "at ({x}, {y}) the shader draws the ground at {drawn_h}, the scene at {}",
            scene.height
        );
        // A body standing there, where physics puts it: on retail's ground on land, sunk as
        // retail sinks it in its water. Drawn on the ground on land; in the water never under the
        // bed, and where the bed lies deeper than retail sinks it, as deep as retail sinks it.
        let inside = water.shore.distance(x, y, 0.0);
        let sink = if inside > 6.0 {
            0.9
        } else if inside > 0.0 {
            0.45
        } else {
            0.0
        };
        let z = p.retail - sink;
        let feet = z + drawn::shift(z, scene, 1.0);
        if sink == 0.0 {
            assert!(
                (feet - drawn_h).abs() < 2e-3,
                "at ({x}, {y}) a body stands at {feet} on ground drawn at {drawn_h}"
            );
            if near < -0.2 {
                dipped += 1;
            }
        } else {
            assert!(
                feet >= drawn_h - 2e-3,
                "at ({x}, {y}) a body sunk {sink} m stands at {feet} under the bed at {drawn_h}"
            );
            if p.retail - drawn_h > sink {
                assert!((feet - z).abs() < 2e-3, "at ({x}, {y}) {feet} for {z}");
                wet += 1;
            }
        }
    }
    eprintln!(
        "{} points, worst difference {worst:e}; {dipped} where the rounding dips, {wet} in deep water",
        points.len()
    );
    assert!(dipped > 5, "the hill's foot dips under retail's: {dipped}");
    assert!(wet > 20, "bodies sink into deep water: {wet}");
}

/// A cliff running north through block (100, 100): a high shelf to the west, a face of rock
/// bands thirty meters a cell down to the flat land to the east, steeper than a body stands on.
fn cliff() -> Valley {
    let mut heights = vec![0.0f32; SIDE * SIDE];
    let words = vec![1u16 << 2; SIDE * SIDE];
    for gy in 760..860usize {
        for gx in 760..860usize {
            heights[gy * SIDE + gx] = match gx {
                0..=801 => 100.0,
                802 => 70.0,
                803 => 40.0,
                _ => 10.0,
            };
        }
    }
    Valley { heights, words }
}

#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "draws on the hardware GPU: --features retail-dats, one GPU run at a time"
)]
fn a_cliffs_pictures_lie_on_the_rock_itself_wherever_the_mesh_lies() {
    use dereth_render_hifi::passes::drawn::surface_points_on_device;
    use dereth_render_hifi::snapshot::HifiCamera;
    let (device, queue) = crate::hardware();
    let world = cliff();
    let knobs = Knobs::default();
    let block = drawn::block_field(&world, BLOCK, &[], &knobs);
    let (ox, oy) = (19_200.0f32, 19_200.0f32);
    let ground = HifiDrawnGround {
        world: Arc::new(HifiWorldGround {
            serial: 1,
            // LINT-OK: 2041.
            #[allow(clippy::cast_possible_truncation)]
            side: SIDE as u32,
            heights: Arc::new(world.heights.clone()),
            words: Arc::new(world.words.clone()),
            water: Arc::new(Vec::new()),
        }),
        tiles: vec![HifiGroundTile {
            block: 0x6464,
            generation: 1,
            offset: Arc::new(block.offset.clone()),
            weight: Arc::new(block.weight.clone()),
        }],
        reach: 1,
        global: Vec2::ZERO,
        knobs: HifiGroundKnobs {
            micro: (0.0, 0.7),
            ..HifiGroundKnobs::default()
        },
        bodies: Vec::new(),
        cover: None,
    };
    // The eye on the flat land below the face, looking up at it from past the mesh's finest
    // level, whose vertices do not slide.
    let eye = glam::Vec3::new(ox + 188.0, oy + 96.0, 12.0);
    let frame = HifiFrame {
        camera: HifiCamera {
            eye,
            ..HifiCamera::default()
        },
        ..HifiFrame::default()
    };
    let surface = |x: f32, y: f32| {
        let p = Point::at(&world, x, y);
        let near = drawn::sample(&block.offset, x - ox, y - oy);
        drawn::drawn_at(&p, x, y, near, 0.0, None, &knobs).height
    };
    // Lines of sight to the face at heights up it and across it; where each first meets the
    // drawn surface, found on the CPU; and points along each line either side of it, as meshes
    // of one level or the next put them under the pixel.
    let mut rays = Vec::new();
    let mut points = Vec::new();
    for (k, (up, across)) in [20.0f32, 35.0, 50.0, 65.0, 80.0]
        .into_iter()
        .flat_map(|z| [60.0f32, 85.0, 110.0, 135.0].map(move |y| (z, y)))
        .enumerate()
    {
        let target = glam::Vec3::new(ox + 48.0, oy + across, up);
        let r = (target - eye).normalize();
        let mut t = 1.0;
        while t < 300.0 {
            let q = eye + r * t;
            if q.z < surface(q.x, q.y) {
                break;
            }
            t += 0.05;
        }
        let (mut lo, mut hi) = (t - 0.05, t);
        for _ in 0..30 {
            let mid = 0.5 * (lo + hi);
            let q = eye + r * mid;
            if q.z < surface(q.x, q.y) {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        let hit = 0.5 * (lo + hi);
        // A line that only grazes a band's corner, barely in the rock and out again, is left
        // out: it meets no face to lay pictures on.
        let past = eye + r * (hit + 0.3);
        if hit >= 299.0 || past.z > surface(past.x, past.y) - 0.05 {
            continue;
        }
        rays.push((k, eye + r * hit));
        for d in [-0.8f32, -0.3, 0.25, 0.7] {
            let q = eye + r * (hit + d);
            points.push([q.x, q.y, q.z, hit]);
        }
    }
    let laid = surface_points_on_device(&device, &queue, &frame, &ground, &points);
    assert_eq!(laid.len(), points.len(), "the probe read back");
    let mut worst = 0.0f32;
    for (i, (k, hit)) in rays.iter().enumerate() {
        for j in 0..4 {
            let [x, y, z] = laid[i * 4 + j];
            let off = (glam::Vec3::new(x, y, z) - *hit).length();
            worst = worst.max(off);
            assert!(
                off < 0.05,
                "line {k}: the mesh's point {j} lays its pictures {off} m from the rock it meets"
            );
        }
    }
    eprintln!(
        "{} lines of sight to the face; the pictures lie within {worst} m of the rock",
        rays.len()
    );
    assert!(rays.len() >= 15, "{} lines met the face", rays.len());
}
