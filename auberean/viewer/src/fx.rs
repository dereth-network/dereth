//! Effects placed in a frame: `--fx TABLE:EFFECT@X,Y,Z[@SECONDS]` plays effect `EFFECT` of effect
//! table `TABLE` (both hexadecimal) at a point of the block's frame, and `--ps ID@X,Y,Z[@SECONDS]`
//! one particle system; each runs for `SECONDS` (default 1) at sixty frames a second before the
//! frame is drawn, and its particles are drawn with the frame's objects.

use auberean_draw::effects::{Effect, PlacedEffects};
use auberean_draw::particles::camera_axes;
use auberean_draw::scene::Frame;
use auberean_draw::systems::ParticleSystems;
use auberean_primitives::Vec3;
use auberean_world::math::Mat4;
use auberean_world::records::WorldRecords;

/// How often the effects are run while they play: the client's frames.
const FRAME: f64 = 1.0 / 60.0;

/// One placed effect: what it is, where, and for how long it has played.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    /// `(table, effect)` for an effect, or `(0, system)` for one particle system.
    pub what: (u32, u32),
    pub at: Vec3,
    pub seconds: f64,
}

fn hex(s: &str) -> Result<u32, String> {
    u32::from_str_radix(s.trim().trim_start_matches("0x"), 16).map_err(|e| format!("{s}: {e}"))
}

/// An `--fx` (`system` false) or `--ps` (`system` true) value.
///
/// # Errors
/// When it is not `WHAT@X,Y,Z[@SECONDS]` with numbers where numbers belong.
pub fn parse(value: &str, system: bool) -> Result<Placed, String> {
    let mut parts = value.split('@');
    let what = parts.next().unwrap_or_default();
    let what = if system {
        (0, hex(what)?)
    } else {
        let (t, e) = what
            .split_once(':')
            .ok_or_else(|| format!("{value}: want TABLE:EFFECT@X,Y,Z"))?;
        (hex(t)?, hex(e)?)
    };
    let at: Vec<f32> = parts
        .next()
        .ok_or_else(|| format!("{value}: no place"))?
        .split(',')
        .map(|x| x.trim().parse::<f32>().map_err(|e| format!("{x}: {e}")))
        .collect::<Result<_, _>>()?;
    let seconds = match parts.next() {
        Some(s) => s.parse::<f64>().map_err(|e| format!("{s}: {e}"))?,
        None => 1.0,
    };
    Ok(Placed {
        what,
        at: Vec3::new(
            at.first().copied().unwrap_or(0.0),
            at.get(1).copied().unwrap_or(0.0),
            at.get(2).copied().unwrap_or(0.0),
        ),
        seconds,
    })
}

/// Play every placed effect and add what it draws to `frame`'s objects; says how many particles
/// each left alive.
pub fn add(frame: &mut Frame, records: &WorldRecords, placed: &[Placed]) {
    let cam = frame.camera;
    let (right, forward, up) = camera_axes(cam.heading, cam.pitch);
    let dice = auberean_primitives::random::Random::new();
    for p in placed {
        let place = Mat4::translation(p.at);
        let one = Vec3::new(1.0, 1.0, 1.0);
        let mut effect = if p.what.0 == 0 {
            Effect::particles(records, p.what.1, place, one, 0.0, &dice)
        } else {
            Effect::start(records, p.what.0, p.what.1, place, one, 0.0, &dice)
        };
        // Outdoors the sun lights lit particles: its color times its brightness.
        let l = frame.light;
        effect.set_light(auberean_world::color::Color::new(
            l.direct_color.r * l.direct_brightness,
            l.direct_color.g * l.direct_brightness,
            l.direct_color.b * l.direct_brightness,
            1.0,
        ));
        effect.run(p.seconds, FRAME);
        let batches = effect.batches(p.seconds, cam.position, right, forward, up);
        let alive: usize = effect.systems.iter().map(|s| s.alive()).sum();
        println!(
            "effect {:08X}:{:08X} at ({:.1}, {:.1}, {:.1}) after {:.2} s: {} systems, {} particles, {} batches",
            p.what.0,
            p.what.1,
            p.at.x,
            p.at.y,
            p.at.z,
            p.seconds,
            effect.systems.len(),
            alive,
            batches.len()
        );
        frame.objects.extend(batches);
    }
}

/// The startup effects of the objects the frame draws with the world, run for `seconds` from their
/// making, a frame every sixtieth of a second, and what they draw added to `frame`'s objects; says
/// how many objects play them.
pub fn add_placed(
    frame: &mut Frame,
    records: &WorldRecords,
    block: (i32, i32),
    seconds: f64,
    pools: Option<(&auberean_draw::scene::SceneBuilder<'_>, f32)>,
) {
    let cam = frame.camera;
    let axes = camera_axes(cam.heading, cam.pitch);
    let l = frame.light;
    let light = auberean_world::color::Color::new(
        l.direct_color.r * l.direct_brightness,
        l.direct_color.g * l.direct_brightness,
        l.direct_color.b * l.direct_brightness,
        1.0,
    );
    let systems = ParticleSystems::default();
    let mut placed = PlacedEffects::default();
    placed.sync(&systems, records, &frame.placed, 0.0);
    let mut out = Vec::new();
    let mut t = 0.0;
    while t < seconds {
        t = (t + FRAME).min(seconds);
        placed.draw(&systems, t, light);
        out = systems.batches(block, 160.0, t, cam.position, axes);
    }
    let lights = placed.lights(block, 160.0, t);
    println!(
        "startup effects: {} objects play them, {} batches, {} lights after {seconds:.2} s",
        placed.len(),
        out.len(),
        lights.len()
    );
    frame.objects.extend(out);
    // The lights' pools on the land below them, as the light source effects preference lays them.
    if let Some((builder, time_of_day)) = pools {
        let material = auberean_draw::decal::pool_material(records, time_of_day);
        let dice = auberean_primitives::random::Random::new();
        let mut decals = auberean_draw::decal::DecalManager::default();
        let ground = |p: auberean_primitives::Vec3| builder.ground_square(block, p);
        for l in lights.iter().filter(|l| l.cell & 0xFFFF < 0x100) {
            let Some(square) = ground(l.origin) else {
                continue;
            };
            let Some(h) = auberean_draw::decal::ground_height(&square, l.origin.x, l.origin.y)
            else {
                continue;
            };
            let range = auberean_world::wave::value(&l.light.range, t, &dice);
            let intensity = auberean_world::wave::value(&l.light.intensity, t, &dice);
            if let Some(desc) = auberean_draw::decal::point_light_pool(
                material,
                l.origin,
                h,
                range,
                intensity,
                l.light.light.color,
                l.opacity,
            ) {
                let tris = auberean_draw::decal::landscape_decal(
                    l.origin,
                    &desc,
                    builder.square_side(),
                    &ground,
                );
                decals.create(l.cell, &desc, t, tris, true);
            }
        }
        let mut layers = std::collections::HashMap::new();
        frame.decals = auberean_draw::decal::decal_batches(records, &mut decals, &mut layers);
        println!("light pools: {} draws", frame.decals.len());
    }
}
