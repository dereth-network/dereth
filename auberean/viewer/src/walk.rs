//! `--walk`: put the player's body at the spot, walk it with collision over the ground for the
//! given seconds (standing first, then running, with an optional turn and jump), and draw a strip
//! of frames from the chase camera.

use std::path::PathBuf;

use auberean_animation::body::{Body as Figure, AIR, EARTH};
use auberean_animation::CachedRecords;
use auberean_assets::animation::{MotionDesc, MotionParams};
use auberean_physics::body::{setup_spheres, Body, BodyConfig, Ticker};
use auberean_physics::camera::{Camera, CameraSettings};
use auberean_physics::land::{cell_of, land_cell_id, Land};
use auberean_physics::mesh::Trees;
use auberean_physics::motion::MotionInput;
use auberean_primitives::{DataId, Quat, Vec3};
use auberean_world::math::Mat4;
use auberean_world::records::{RegionSet, WorldRecords};
use dereth_primitives::{Frame, LandblockId, Position};

use crate::Args;

/// The player's human male.
const HUMAN_MALE: DataId = DataId(0x1F00_0023);
/// How bodies of the player's kind move.
const PLAYER_MOTION: u32 = 0x3200_0000;
/// The frames drawn per second of the walk.
const FPS: f64 = 30.0;

fn motion_values(md: &MotionDesc, element: u32, mode: u32) -> Option<MotionParams> {
    let (_, set) = md
        .set_of
        .iter()
        .find(|(k, _)| k.element == element && k.mode == mode)?;
    md.sets.get(*set as usize).map(|s| s.default)
}

fn dq(q: dereth_primitives::Quat) -> Quat {
    Quat {
        w: q.w,
        x: q.x,
        y: q.y,
        z: q.z,
    }
}

fn dv(v: dereth_primitives::Vec3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

/// Shrink an RGBA picture by `k` in each direction, keeping every `k`th pixel.
fn shrink(pic: &auberean_gpu::Picture, k: u32) -> (u32, u32, Vec<u8>) {
    let (w, h) = (pic.width / k, pic.height / k);
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let at = (((y * k) * pic.width + x * k) * 4) as usize;
            out.extend_from_slice(&pic.rgba[at..at + 4]);
        }
    }
    (w, h, out)
}

pub fn run(a: &Args, records: &WorldRecords, region: &RegionSet) -> Result<(), String> {
    let seconds = a.walk.unwrap_or(4.0);
    let out_dir = a.out.clone();
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
    let (bx, by) = (
        u8::try_from(a.block.0).map_err(|e| e.to_string())?,
        u8::try_from(a.block.1).map_err(|e| e.to_string())?,
    );

    // The world's cells around the spot.
    let land = Land::new();
    let mut trees = Trees::default();
    for dx in -1i32..=1 {
        for dy in -1i32..=1 {
            if let (Ok(x), Ok(y)) = (u8::try_from(a.block.0 + dx), u8::try_from(a.block.1 + dy)) {
                land.add_block(records, region, &mut trees, x, y);
            }
        }
    }

    // The body: its figure, its collision and its motion values.
    let recs = CachedRecords::new(records);
    let mut figure =
        Figure::assemble(&recs, HUMAN_MALE).ok_or("the human male does not put together")?;
    let setup = figure.parts[0].setup.clone();
    let md: MotionDesc = records.get(PLAYER_MOTION).ok_or("no motion description")?;
    let ground = motion_values(&md, EARTH, figure.mode).ok_or("no ground motion values")?;
    let air = motion_values(&md, AIR, figure.mode).unwrap_or(ground);
    let config = BodyConfig {
        spheres: setup_spheres(&setup),
        step_up: setup.step_up_height,
        step_down: setup.step_down_height,
        scale: 1.0,
        ground,
        air,
    };
    let at = dereth_primitives::Vec3::new(a.at.x, a.at.y, a.at.z);
    let (cx, cy) = cell_of(at).ok_or("the spot is outside its block")?;
    let start = Position::new(
        land_cell_id(bx, by, cx, cy),
        Frame::new(at, dereth_primitives::Quat::IDENTITY),
    );
    let mut body = Body::new(config, start, a.heading);
    println!(
        "body: {} parts, spheres {:?}, step up {} down {}, run {} m/s, element {:#x} mode {:#x}",
        figure.parts.len(),
        body.config.spheres,
        body.config.step_up,
        body.config.step_down,
        ground.max_forward,
        figure.element,
        figure.mode
    );

    let mut renderer = auberean_gpu::Renderer::new().map_err(|e| e.to_string())?;
    let mut builder = auberean_draw::scene::SceneBuilder::new(records, region);
    builder.uptime = a.uptime;
    builder.static_shadows = a.shadows;
    builder.cloud_shadows = a.clouds;
    builder.water_reflections = a.reflections;
    let mut drawer = auberean_draw::body::BodyDrawer::default();
    let mut volumes = auberean_draw::volume::VolumeMaker::default();
    let mut decals = auberean_draw::decal::DecalManager::default();
    let mut layers = std::collections::HashMap::new();
    let camera = Camera::new(CameraSettings::for_body(setup.height, 1.0));
    let frames = a.frames.max(1);
    let every = ((seconds * FPS) / f64::from(frames)).max(1.0);
    let mut ticker = Ticker::new(0.0);
    let mut strip: Vec<(u32, u32, Vec<u8>)> = Vec::new();
    let total =
        u64::try_from(dereth_primitives::num::to_i64_f64((seconds * FPS).round())).unwrap_or(0);
    let mut next_capture = every;
    let mut jumped = false;
    for k in 1..=total {
        let t = k as f64 / FPS;
        let mut input = MotionInput::default();
        if t >= 0.5 {
            input.forward = 1.0;
        }
        if let Some(turn_at) = a.turn_at {
            if t >= turn_at && t < turn_at + 1.0 {
                input.turn = 1.0;
            }
        }
        body.steer(input);
        if let Some(jump_at) = a.jump_at {
            if !jumped && t >= jump_at {
                jumped = body.jump();
            }
        }
        for _ in 0..ticker.ticks(t) {
            body.tick(&land, auberean_physics::body::tick_seconds());
        }
        // The figure is animated at how fast the body moved over its last tick.
        #[allow(clippy::cast_possible_truncation)]
        let step = (1.0 / FPS) as f32;
        body.update_animation(step);
        figure.set_motion(
            dv(body.anim_velocity),
            body.turn_rate,
            dv(body.total_scale(None)),
        );
        let v = body.local_velocity();
        figure.advance(&recs, step);
        if (k as f64) < next_capture && k != total {
            continue;
        }
        next_capture += every;
        let poses = figure.pose(&recs);
        let origin = body.position.frame.origin;
        let block = body.position.cell.landblock();
        let place = Mat4::from_parts(
            dv(origin),
            dq(body.position.frame.rotation),
            Vec3::new(1.0, 1.0, 1.0),
        );
        let view = camera.place(
            &land,
            LandblockId::new(block.x(), block.y()),
            origin,
            body.heading,
        );
        let spot = auberean_draw::scene::Spot {
            block: (i32::from(block.x()), i32::from(block.y())),
            position: dv(view.eye),
            heading: view.heading,
            pitch: view.pitch,
            time_of_day: a.time,
            day: a.day.map(DataId),
        };
        let mut spot = spot;
        if let (Some(gt), Some(cal), Some(sky)) =
            (a.game_time, region.calendar.as_ref(), region.sky.as_ref())
        {
            let c = auberean_world::sky::day_clock(cal, gt);
            spot.time_of_day = c.fraction;
            spot.day =
                auberean_world::sky::day_for(sky, c.year * i64::from(cal.days_per_year) + c.day);
        }
        let mut frame = builder.frame(&spot, a.fov);
        drawer.begin_frame();
        let mut batches = drawer.batches(records, &figure, &poses, &place);
        // By way of the threads, as a client's crowd is.
        let mut asks = Vec::new();
        auberean_draw::body::ask_ahead(&batches, (0.0, 1.0), a.volumes, &mut asks);
        auberean_draw::body::answer_ahead(asks);
        if a.volumes {
            // The body casts its volume of shadow along the sun's light and is drawn after it.
            for b in &mut batches {
                b.casts_volume = true;
            }
            frame.volumes.strength =
                auberean_draw::volume::Volumes::strength_of(frame.light.shadow_color, false);
            let mut room = auberean_draw::volume::VolumeRoom::FRAME;
            volumes.cast(
                records,
                &batches,
                frame.light.light_direction,
                1.0,
                builder.model_detail,
                &mut room,
                &mut frame.volumes.volumes,
            );
            crate::placed_volumes(&mut frame, records, builder.model_detail, &mut volumes);
        }
        if a.disc {
            // The simple setting: a dark disc laid on the land under the body, drawn once.
            let strength =
                auberean_draw::volume::Volumes::strength_of(frame.light.shadow_color, false);
            let b = setup.physics_bounds;
            let disc = auberean_draw::decal::disc(
                auberean_draw::decal::disc_material(records),
                Vec3::new(b.min.x, b.min.y, b.min.z),
                Vec3::new(b.max.x, b.max.y, b.max.z),
                Vec3::new(1.0, 1.0, 1.0),
                body.heading,
                strength,
            );
            if let Some(desc) = disc {
                let block = spot.block;
                let ground = |p: Vec3| builder.ground_square(block, p);
                let at = dv(origin);
                let tris = auberean_draw::decal::landscape_decal(
                    at,
                    &desc,
                    builder.square_side(),
                    &ground,
                );
                decals.create(body.position.cell.0, &desc, t, tris, true);
            }
            frame.decals = auberean_draw::decal::decal_batches(records, &mut decals, &mut layers);
            decals.think(t);
        }
        frame.objects.extend(batches);
        let mut load = |id: u32| auberean_draw::picture::load(records, id);
        let pic = renderer
            .render(&frame, a.size.0, a.size.1, &mut load)
            .map_err(|e| e.to_string())?;
        let n = strip.len();
        let path: PathBuf = out_dir.join(format!("walk-{n:02}.png"));
        crate::write_png(&path, &pic)?;
        println!(
            "{}: t {t:.2} s, cell {:08X} at ({:.2}, {:.2}, {:.2}) heading {:.1}, speed {:.2} m/s, contact {}, forward slider {:.2}",
            path.display(),
            body.position.cell.0,
            origin.x,
            origin.y,
            origin.z,
            body.heading,
            (v.x * v.x + v.y * v.y).sqrt(),
            body.contact,
            figure.sliders.get(auberean_animation::body::slider::FORWARD),
        );
        strip.push(shrink(&pic, 2));
    }
    // The strip: the frames side by side, at half size.
    if let Some((w, h, _)) = strip.first() {
        let (w, h) = (*w, *h);
        let count = u32::try_from(strip.len()).unwrap_or(1);
        let mut rgba = vec![0u8; (w * count * h * 4) as usize];
        for (i, (_, _, px)) in strip.iter().enumerate() {
            for y in 0..h {
                let src = (y * w * 4) as usize;
                let dst = ((y * w * count + u32::try_from(i).unwrap_or(u32::MAX) * w) * 4) as usize;
                rgba[dst..dst + (w * 4) as usize].copy_from_slice(&px[src..src + (w * 4) as usize]);
            }
        }
        let pic = auberean_gpu::Picture {
            width: w * count,
            height: h,
            rgba,
        };
        crate::write_png(&out_dir.join("walk-strip.png"), &pic)?;
    }
    Ok(())
}
