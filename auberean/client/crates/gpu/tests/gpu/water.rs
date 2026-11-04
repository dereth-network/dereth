//! The water as the device lays it: half its own color over the ground under it, and its
//! reflection laid over by half, rippled one way, or not at all.

use std::sync::Arc;

use auberean_draw::material::{d3d, switch, DrawLayer};
use auberean_draw::mesh::MeshVertex;
use auberean_draw::scene::{Camera, Frame, ObjectBatch, Reflection, SceneBuilder, Spot};
use auberean_primitives::Vec3;
use auberean_world::color::Color;
use auberean_world::math::Mat4;

use crate::common::{checked, install, renderer};

const W: usize = 320;
const H: usize = 240;

fn vertex(p: [f32; 3], uv: [f32; 2]) -> MeshVertex {
    MeshVertex {
        position: p,
        normal: [0.0, 0.0, 1.0],
        diffuse: 0xFFFF_FFFF,
        uv: [uv; 4],
        tangent_u: [1.0, 0.0, 0.0],
        tangent_v: [0.0, 1.0, 0.0],
        to_light: [0.0; 3],
    }
}

/// A flat square from `(x0, y0)` to `(x1, y1)` at height `z` (or standing upright across x at
/// `y0`, from `z` to `z1`), its coordinates `per` meters a repeat.
fn quad(base: &ObjectBatch, corners: [[f32; 3]; 4], per: f32, layer: DrawLayer) -> ObjectBatch {
    let mut b = base.clone();
    b.vertices = Arc::new(
        corners
            .iter()
            .map(|&c| vertex(c, [c[0] / per, c[1] / per]))
            .collect(),
    );
    b.indices = Arc::new(vec![0, 1, 2, 1, 3, 2, 2, 1, 0, 2, 3, 1]);
    b.transform = Mat4::IDENTITY;
    b.layer = layer;
    b
}

/// `b` with its first two triangles only, drawn whichever way it faces: a surface every pixel of
/// which is drawn once, as a blend over what lies under it needs.
fn once(mut b: ObjectBatch) -> ObjectBatch {
    b.indices = Arc::new(b.indices[..6].to_vec());
    b.layer.cull = d3d::CULL_NONE;
    b
}

/// The pixels of `p` that are bright (the reflected white bar, laid over the water by half).
fn bright(p: &[u8], x: std::ops::Range<usize>, y: std::ops::Range<usize>) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for row in y {
        for col in x.clone() {
            let k = (row * W + col) * 4;
            if u32::from(p[k]) + u32::from(p[k + 1]) + u32::from(p[k + 2]) > 3 * 100 {
                out.push((col, row));
            }
        }
    }
    out
}

/// The mean of channel `ch` of `p` (`w` pixels a row) over `x` by `y`.
fn mean(
    p: &[u8],
    w: usize,
    ch: usize,
    x: std::ops::Range<usize>,
    y: std::ops::Range<usize>,
) -> f64 {
    let mut sum = 0.0;
    let mut n = 0.0;
    for row in y {
        for col in x.clone() {
            sum += f64::from(p[(row * w + col) * 4 + ch]);
            n += 1.0;
        }
    }
    sum / n
}

/// A white bar standing 2 m over a wide water surface 20 m ahead of a camera 2 m up looking a
/// little down, with the camp's water layer, its reflection rippled or not.
fn bar_over_water(builder: &mut SceneBuilder<'_>, ripples: bool) -> Frame {
    // the camp's pond, for its water's layer
    let camp = Spot {
        block: (0xC1, 0x56),
        position: Vec3::new(122.842, 89.758, 81.6),
        heading: 90.0,
        pitch: 5.0,
        time_of_day: 0.45,
        day: None,
    };
    let camp_frame = builder.frame(&camp, 60.0);
    let water_batch = camp_frame.water.first().expect("the pond").clone();
    let mut white = water_batch.layer.clone();
    white.stages.clear();
    white.source_blend = 2;
    white.dest_blend = 1;
    white.switches = 0;
    white.cull = 1;
    white.light_variants.clear();
    white.motion = None;
    white.pixel_program = None;
    let water = once(quad(
        &water_batch,
        [
            [-80.0, -20.0, 0.0],
            [80.0, -20.0, 0.0],
            [-80.0, 140.0, 0.0],
            [80.0, 140.0, 0.0],
        ],
        40.0,
        water_batch.layer.clone(),
    ));
    // an upright white bar 20 m ahead, its foot 2 m over the water
    let bar = quad(
        &water_batch,
        [
            [-3.0, 20.0, 2.0],
            [3.0, 20.0, 2.0],
            [-3.0, 20.0, 3.0],
            [3.0, 20.0, 3.0],
        ],
        1.0,
        white,
    );
    let camera = Camera {
        position: Vec3::new(0.0, 0.0, 2.0),
        heading: 0.0,
        pitch: -12.0,
        fov_y: 60.0,
        near: 0.1,
        far: 500.0,
        orthographic: None,
    };
    let day = builder.day(&camp);
    let light = builder.light(&camp, day.as_ref());
    let mut bar = bar;
    bar.anchor = Some(Vec3::new(0.0, 20.0, 2.0));
    let mut frame = builder.parts_frame(&[], vec![bar], camera, light, None);
    frame.water = vec![water];
    frame.reflection = Some(Reflection {
        height: 0.0,
        ripples,
    });
    frame
}

/// The reflection shows an object by where it stands: one standing above the water whole, even a
/// part of it below the water, one standing below the water not at all.
///
/// Behaviour: draw.water.the-reflection-takes-objects-by-where-they-stand
#[test]
fn the_reflection_takes_objects_by_where_they_stand() {
    let i = install();
    let r = &i.records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let mut at = |height: f32| {
        let mut f = bar_over_water(&mut builder, false);
        for v in std::sync::Arc::make_mut(&mut f.objects[0].vertices).iter_mut() {
            v.position[2] -= 3.0;
        }
        f.objects[0].anchor = Some(Vec3::new(0.0, 20.0, height));
        f
    };
    let frames = [at(1.0), at(-1.0)];
    let mut device = renderer();
    let mut load = |id: u32| auberean_draw::picture::load(r, id);
    let p = checked(&mut device, |d| {
        frames.each_ref().map(|f| {
            d.render(
                f,
                u32::try_from(W).expect("a width"),
                u32::try_from(H).expect("a height"),
                &mut load,
            )
            .expect("read back")
            .rgba
        })
    });
    // the part under the water shows through it either way; its reflection only standing above
    let above = bright(&p[0], 0..W, 0..H).len();
    let below = bright(&p[1], 0..W, 0..H).len();
    println!("bright texels: {above} standing above, {below} standing below");
    assert!(
        above > below + 20,
        "standing above the water it shows in the reflection, its part below too: {above}, {below}"
    );
}

/// The reflection shows all the land the mirrored camera can see, what lies under the water too.
///
/// Behaviour: draw.water.the-reflection-shows-the-land-under-the-water
#[test]
fn the_reflection_shows_the_land_under_the_water() {
    let i = install();
    let r = &i.records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let base = bar_over_water(&mut builder, false);
    let ground = base.water[0].layer.stages.first().map_or(0, |s| s.texture);
    // an upright wall of land 30 m ahead, all of it under the water, facing either way
    let wall = |shade: f32| {
        let v = |x: f32, z: f32| auberean_draw::scene::TerrainVertex {
            position: [x, 30.0, z],
            color: [shade, shade, shade, 1.0],
            uv: [0.0, 0.0],
            mask_uv: [0.0, 0.0],
        };
        auberean_draw::scene::TerrainBatch {
            ground,
            far: true,
            vertices: Arc::new(vec![
                v(-40.0, -0.1),
                v(40.0, -0.1),
                v(-40.0, -8.0),
                v(40.0, -8.0),
            ]),
            indices: Arc::new(vec![0, 1, 2, 2, 1, 3, 0, 2, 1, 2, 3, 1]),
            ..auberean_draw::scene::TerrainBatch::default()
        }
    };
    let frame = |shade: f32, reflected: bool| {
        let mut f = base.clone();
        f.objects.clear();
        f.terrain = vec![wall(shade)];
        if !reflected {
            f.reflection = None;
        }
        f
    };
    let frames = [
        frame(1.0, true),
        frame(0.0, true),
        frame(1.0, false),
        frame(0.0, false),
    ];
    let mut device = renderer();
    let mut load = |id: u32| auberean_draw::picture::load(r, id);
    let p = checked(&mut device, |d| {
        frames.each_ref().map(|f| {
            d.render(
                f,
                u32::try_from(W).expect("a width"),
                u32::try_from(H).expect("a height"),
                &mut load,
            )
            .expect("read back")
            .rgba
        })
    });
    // How much lighter the wall makes each texel, with its reflection and without.
    let mut shows = 0;
    for k in (0..W * H * 4).step_by(4) {
        let lighter = |a: &[u8], b: &[u8]| i32::from(a[k]) - i32::from(b[k]);
        if (lighter(&p[0], &p[1]) - lighter(&p[2], &p[3])).abs() > 10 {
            shows += 1;
        }
    }
    println!("{shows} texels show the wall in the reflection");
    assert!(shows > 50, "{shows}");
}

/// The ripples push the reflection one way only: the picture they are laid in holds no offset
/// below zero, so a white bar standing over the water shows in it drawn out downward (and to the
/// left) of where it lies unrippled, in spikes, and never up past its top edge or right past its
/// right edge. Without the ripples (the reflections at Low) the reflection lies as it stands, the
/// same as the ripples slide on.
///
/// Behaviour: draw.water.the-ripples-push-the-reflection-one-way
#[test]
fn the_ripples_push_the_reflection_one_way() {
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let frame = bar_over_water(&mut builder, true);
    let at = |ripples: bool, uptime: f64| {
        let mut f = frame.clone();
        f.reflection = Some(Reflection {
            height: 0.0,
            ripples,
        });
        f.uptime = uptime;
        f
    };
    let frames = [
        at(false, 30.0),
        at(false, 75.0),
        at(true, 30.0),
        at(true, 75.0),
    ];
    let mut device = renderer();
    let mut load = |id: u32| auberean_draw::picture::load(r, id);
    let p = checked(&mut device, |d| {
        frames.each_ref().map(|f| {
            d.render(
                f,
                u32::try_from(W).expect("a width"),
                u32::try_from(H).expect("a height"),
                &mut load,
            )
            .expect("read back")
            .rgba
        })
    });
    // the water under the bar itself (the bar stands about rows 65 to 76, its reflection about
    // 117 to 127)
    let below = H / 2 - 10..H;
    let flat = bright(&p[0], 0..W, below.clone());
    assert!(flat.len() > 100, "the bar reflected: {}", flat.len());
    assert_eq!(
        flat,
        bright(&p[1], 0..W, below.clone()),
        "no ripples: it stands still"
    );
    let (left, right) = (
        flat.iter().map(|q| q.0).min().unwrap_or(0),
        flat.iter().map(|q| q.0).max().unwrap_or(0),
    );
    let (top, bottom) = (
        flat.iter().map(|q| q.1).min().unwrap_or(0),
        flat.iter().map(|q| q.1).max().unwrap_or(0),
    );
    println!("unrippled reflection: columns {left}-{right}, rows {top}-{bottom}");
    for rippled in &p[2..] {
        let lit = bright(rippled, 0..W, below.clone());
        let past = |f: &dyn Fn(&(usize, usize)) -> bool| lit.iter().filter(|q| f(q)).count();
        let above = past(&|q| q.1 < top && q.0 >= left && q.0 <= right);
        let beyond_right = past(&|q| q.0 > right && q.1 >= top && q.1 <= bottom);
        let under = past(&|q| q.1 > bottom);
        let beyond_left = past(&|q| q.0 < left);
        println!("rippled: {above} above, {beyond_right} right, {under} under, {beyond_left} left");
        assert_eq!((above, beyond_right), (0, 0), "never up or right");
        assert!(under > 0, "drawn out downward");
    }
    assert_ne!(
        bright(&p[2], 0..W, below.clone()),
        bright(&p[3], 0..W, below),
        "the ripples slide on"
    );
}

/// The reflection is drawn into a picture of its own 512 texels square, whatever the screen's
/// size: on a screen 1,024 pixels wide each of its texels spans two pixels, so the edge of a white
/// bar's reflection fades over more than one pixel, where the bar itself stands sharp.
///
/// Behaviour: draw.water.the-reflection-is-drawn-512-texels-square
#[test]
fn the_reflection_is_drawn_512_texels_square() {
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let frame = bar_over_water(&mut builder, false);
    let (w, h) = (1024_usize, 768_usize);
    let mut device = renderer();
    let mut load = |id: u32| auberean_draw::picture::load(r, id);
    let p = checked(&mut device, |d| {
        d.render(
            &frame,
            u32::try_from(w).expect("a width"),
            u32::try_from(h).expect("a height"),
            &mut load,
        )
        .expect("read back")
        .rgba
    });
    let sum = |x: usize, y: usize| {
        let k = (y * w + x) * 4;
        u32::from(p[k]) + u32::from(p[k + 1]) + u32::from(p[k + 2])
    };
    // the fade across the left edge of the bar (row 230) and of its reflection (row 390): the
    // pixels between the row's darkest and brightest, a sixth of the way in from each
    let fade = |y: usize| {
        let row: Vec<u32> = (0..w / 2).map(|x| sum(x, y)).collect();
        let (lo, hi) = (
            row.iter().copied().min().unwrap_or(0),
            row.iter().copied().max().unwrap_or(0),
        );
        let margin = (hi - lo) / 6;
        row.iter()
            .filter(|&&s| s > lo + margin && s < hi - margin)
            .count()
    };
    let (bar, reflection) = (fade(230), fade(390));
    println!("the bar's edge fades over {bar} pixels, its reflection's over {reflection}");
    assert!(bar <= 1, "the bar stands sharp: {bar}");
    assert!(reflection >= 2, "the reflection fades: {reflection}");
}

/// The water's own material blends by the destination's alpha, and the frame holds alpha 127 of
/// 255 wherever it is drawn (the device writes only color; every clear leaves that alpha): the
/// water shows half its own color and half the ground under it, as the original's pond does, not
/// the four fifths of its own its layer's alpha would give.
///
/// Behaviour: draw.water.the-water-shows-half-the-ground-under-it
#[test]
fn the_water_shows_half_the_ground_under_it() {
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let base = bar_over_water(&mut builder, false);
    let water = base.water[0].clone();
    assert_eq!(
        (water.layer.source_blend, water.layer.dest_blend),
        (d3d::BLEND_DEST_ALPHA, d3d::BLEND_INV_DEST_ALPHA),
        "the camp's water blends by the destination's alpha"
    );
    // an orange ground a meter under the water, its own color unlit
    let mut orange = base.objects[0].layer.clone();
    orange.switches = switch::MATERIAL_DIFFUSE;
    orange.diffuse = Color::new(0.8, 0.55, 0.3, 1.0);
    let ground = once(quad(
        &water,
        [
            [-80.0, -20.0, -1.0],
            [80.0, -20.0, -1.0],
            [-80.0, 140.0, -1.0],
            [80.0, 140.0, -1.0],
        ],
        40.0,
        orange,
    ));
    let mut alone = water.clone();
    alone.layer.source_blend = d3d::BLEND_ONE;
    alone.layer.dest_blend = d3d::BLEND_ZERO;
    let with = |w: Option<&ObjectBatch>| {
        let mut f = base.clone();
        f.objects = vec![ground.clone()];
        f.water = w.into_iter().cloned().collect();
        f.reflection = None;
        f
    };
    // as drawn; the water's own color alone; the ground alone
    let frames = [with(Some(&water)), with(Some(&alone)), with(None)];
    let mut device = renderer();
    let mut load = |id: u32| auberean_draw::picture::load(r, id);
    let p = checked(&mut device, |d| {
        frames.each_ref().map(|f| {
            d.render(
                f,
                u32::try_from(W).expect("a width"),
                u32::try_from(H).expect("a height"),
                &mut load,
            )
            .expect("read back")
            .rgba
        })
    });
    // the water's near middle, below the horizon
    let (x, y) = (W / 4..3 * W / 4, 3 * H / 4..7 * H / 8);
    let mut measured = 0;
    for ch in 0..3 {
        let [a, w, g] = p.each_ref().map(|q| mean(q, W, ch, x.clone(), y.clone()));
        println!("channel {ch}: drawn {a:.1}, water {w:.1}, ground {g:.1}");
        if (w - g).abs() > 40.0 {
            let share = (a - g) / (w - g);
            println!("  the water's share {share:.3}");
            assert!(
                (0.47..0.53).contains(&share),
                "channel {ch}: the water's share {share:.3}, not half"
            );
            measured += 1;
        }
    }
    assert!(
        measured >= 2,
        "the water and the ground differ enough to measure"
    );
}

/// The reflection is laid over the water by the alpha the frame holds, 127 of 255: half the
/// reflection, half the water as it was drawn, not the four fifths of the water's layer's alpha.
///
/// Behaviour: draw.water.the-reflection-is-laid-over-by-half
#[test]
fn the_reflection_is_laid_over_by_half() {
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let on = bar_over_water(&mut builder, false);
    let mut off = on.clone();
    off.reflection = None;
    let mut device = renderer();
    let mut load = |id: u32| auberean_draw::picture::load(r, id);
    let [p_on, p_off] = checked(&mut device, |d| {
        [&on, &off].map(|f| {
            d.render(
                f,
                u32::try_from(W).expect("a width"),
                u32::try_from(H).expect("a height"),
                &mut load,
            )
            .expect("read back")
            .rgba
        })
    });
    // the white bar as it stands (above the horizon), and so as the reflection holds it
    let bar: [f64; 3] = std::array::from_fn(|ch| {
        (0..H / 2 * W)
            .map(|k| f64::from(p_on[k * 4 + ch]))
            .fold(0.0, f64::max)
    });
    println!("the bar: {bar:?}");
    assert!(bar.iter().all(|&c| c > 250.0), "the bar is white");
    // the middle of the bar's reflection: where the reflection adds the most
    let lit = |k: usize| {
        (0..3)
            .map(|ch| i32::from(p_on[k + ch]) - i32::from(p_off[k + ch]))
            .sum::<i32>()
    };
    let mut spots: Vec<usize> = (H / 2 * W..H * W)
        .map(|k| k * 4)
        .filter(|&k| lit(k) > 150)
        .collect();
    assert!(spots.len() > 50, "the bar reflected: {}", spots.len());
    spots.sort_by_key(|&k| std::cmp::Reverse(lit(k)));
    spots.truncate(spots.len() / 2);
    let mut shares = Vec::new();
    for &k in &spots {
        for ch in 0..3 {
            let (f_on, f_off) = (f64::from(p_on[k + ch]), f64::from(p_off[k + ch]));
            shares.push((f_on - f_off) / (bar[ch] - f_off));
        }
    }
    shares.sort_by(f64::total_cmp);
    let median = shares[shares.len() / 2];
    println!(
        "the reflection's share: median {median:.3} over {} spots",
        spots.len()
    );
    assert!(
        (0.47..0.53).contains(&median),
        "the reflection's share {median:.3}, not half"
    );
}
