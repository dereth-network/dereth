//! Behaviour: none (experimental Horizon rendering effects: No Flame Flicker)
//!
//! A flame's light moves with the scene's clock, and holds steady while No Flame Flicker is
//! ticked. The better lighting lights a place with its light sources alone, with none of the
//! flat sprites of light that stand for them.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware `wgpu` device.

#![cfg(gpu)]

use dereth_client_runtime::present::Scene as _;
use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_primitives::Vec3;
use dereth_render::device::{Backend, DeviceConfig, Gpu};
use dereth_scene::world_scene::SceneWrites;

use crate::instruments::hifi_stations::{self, capture, Clock, Place, Shot, Station};

const W: u32 = 480;
const H: u32 = 270;

/// The station of that name.
fn named(name: &str) -> Station {
    hifi_stations::stations(&crate::common::dats())
        .into_iter()
        .find(|s| s.name == name)
        .expect("the station")
}

/// A body in a room with a fire burning in its hearth.
fn by_the_hearth() -> Station {
    Station {
        name: "hearth",
        why: "a room lit by the fire in its hearth",
        place: Place::Body {
            cell: 0x5650_013E,
            origin: Vec3::new(40.0, -51.0, 0.0),
            yaw: 0.0,
        },
        clock: Clock::Sunny(0.5),
        weather: false,
        land_radius: 3,
    }
}

/// `station`'s frames over two seconds, with the preferences `spec`.
fn frames(station: &Station, spec: &str) -> Vec<Vec<u8>> {
    let store = crate::common::dats();
    let cfg = DeviceConfig {
        width: W,
        height: H,
        hifi: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(Backend::Wgpu, None, &cfg).expect("a wgpu device");
    let mut shot = Shot::open(&store, &mut gpu, station);
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch(spec).expect("a fidelity spec");
    shot.scene
        .update_from_preferences(&store, &mut gpu)
        .expect("the preferences poll");
    // Past the frames in which the history and the eye's adaptation are still settling.
    for _ in 0..28 {
        shot.step(&mut gpu);
        assert!(gpu.hifi_settle(std::time::Duration::from_secs(60)));
    }
    (0..16)
        .map(|_| {
            // Long enough for a flame to have moved, and for the lamps' history to catch up.
            for _ in 0..4 {
                shot.step_by(&mut gpu, 0.03);
                assert!(gpu.hifi_settle(std::time::Duration::from_secs(60)));
            }
            capture(&mut gpu)
        })
        .collect()
}

/// How many pixels move by more than a few levels from one of `frames` to another.
fn pixels_that_move(frames: &[Vec<u8>]) -> usize {
    let (first, rest) = frames.split_first().expect("frames");
    let (mut most, mut least) = (first.clone(), first.clone());
    for picture in rest {
        for (i, v) in picture.iter().enumerate() {
            most[i] = most[i].max(*v);
            least[i] = least[i].min(*v);
        }
    }
    most.chunks_exact(4)
        .zip(least.chunks_exact(4))
        .filter(|(m, l)| m.iter().zip(*l).any(|(a, b)| a - b > 4))
        .count()
}

#[test]
fn a_flames_pool_of_light_moves_and_the_option_stills_it() {
    let _gpu = crate::common::gpu_lock();
    let street = named("lamps-street");
    let on = pixels_that_move(&frames(&street, "Lighting=1,Lamps=1,NoFlameFlicker=0"));
    let off = pixels_that_move(&frames(&street, "Lighting=1,Lamps=1,NoFlameFlicker=1"));
    assert!(
        on > off * 4 && on > 400,
        "{on} pixels move with it, {off} without"
    );
}

#[test]
fn a_rooms_light_moves_with_the_fire_it_stands_at() {
    let _gpu = crate::common::gpu_lock();
    let hearth = by_the_hearth();
    let on = frames(&hearth, "Lighting=1,Lamps=1,NoFlameFlicker=0");
    let off = frames(&hearth, "Lighting=1,Lamps=1,NoFlameFlicker=1");
    // The body and the fire's particles move either way, so each moment is held against its own.
    let lit = on
        .iter()
        .zip(&off)
        .map(|(a, b)| {
            a.chunks_exact(4)
                .zip(b.chunks_exact(4))
                .filter(|(x, y)| x.iter().zip(*y).any(|(p, q)| p.abs_diff(*q) > 4))
                .count()
        })
        .max()
        .unwrap_or(0);
    assert!(lit > 500, "the flicker changes {lit} pixels at most");
}

/// How many particles are drawn at a place with a glow sprite and a brazier's flame, with `spec`.
fn particles_drawn(spec: &str) -> usize {
    let store = crate::common::dats();
    let station = Station {
        name: "sprite",
        why: "a dungeon's room with a sprite of light by its brazier",
        place: Place::Body {
            cell: 0x019E_0133,
            origin: Vec3::new(40.7, -39.3, 0.0),
            yaw: 0.0,
        },
        clock: Clock::Sunny(0.5),
        weather: true,
        land_radius: 3,
    };
    let cfg = DeviceConfig {
        width: W,
        height: H,
        hifi: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(Backend::Wgpu, None, &cfg).expect("a wgpu device");
    let mut shot = Shot::open(&store, &mut gpu, &station);
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch(spec).expect("a fidelity spec");
    shot.scene
        .update_from_preferences(&store, &mut gpu)
        .expect("the preferences poll");
    for _ in 0..60 {
        shot.step(&mut gpu);
        assert!(gpu.hifi_settle(std::time::Duration::from_secs(60)));
    }
    shot.scene.draw.drawn_particles().drawn
}

#[test]
fn the_better_lighting_leaves_out_sprites_of_light_and_keeps_the_flames() {
    let _gpu = crate::common::gpu_lock();
    let standard = particles_drawn("off");
    let enhanced = particles_drawn("Lighting=1,Lamps=1");
    assert!(
        enhanced > 0,
        "the brazier's flame is drawn, {enhanced} and {standard}"
    );
    assert!(
        enhanced < standard,
        "{enhanced} drawn, and {standard} without the better lighting"
    );
}
