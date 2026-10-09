//! The bridge from the scene to the optional high-fidelity presentation.
//!
//! Two jobs, both read-only towards the game:
//! - **The settings.** The player's `[Fidelity]` preferences are mapped onto the presentation's
//!   own settings, and a change installs, reconfigures or removes the presentation on the device.
//!   Nothing is in effect unless the Horizon interface is shown. The effects drawn inside the
//!   better lighting are off without it and the lamps are off on a device that does not trace
//!   rays, so no setting can ask for a combination that does not draw. The presentation never
//!   sees a runtime type.
//! - **The snapshot.** After the frame's world has been drawn, a plain owned copy of what the
//!   presentation needs about it (camera, sun, fog, sky, lights, lamps, landscape, and the
//!   retained geometry of blocks that changed) is handed to the presentation. It is built from
//!   shared reads after the frame's walk has finished, so it cannot change what the frame drew or
//!   what can be picked in it. With no presentation installed nothing is built.
//!
//! The bridge's own memory between frames ([`Shadow`]) is handed to it by its two callers, so
//! nothing here writes through a shared reference: the scene and the world are only read.

use std::collections::HashMap;

use dereth_client_runtime::render_prefs::{FidelityFeature, FidelityPreferences};
use dereth_render_hifi::snapshot::{
    HifiCamera, HifiFog, HifiFrame, HifiLamp, HifiLight, HifiSky, HifiSkyObject, HifiStaticBatch,
    HifiStaticBlock, HifiTerrainBlock, LampSource,
};
use dereth_render_hifi::{DebugView, HifiRenderer, HifiSettings, Level};

use super::{Gpu, SceneDraw, WorldState};

/// Retained geometry sent in one frame at most, bytes; the rest follows in later frames.
const FEED_BYTES_PER_FRAME: usize = 4 << 20;

/// What the bridge remembers between frames: the preferences and the device's ray queries it
/// last applied, the previous frame's camera, and, for each block whose geometry the presentation
/// holds, the block's generation and the detail levels its objects stood at when it was sent.
#[derive(Debug, Default)]
pub(crate) struct Shadow {
    applied: Option<(FidelityPreferences, bool)>,
    previous_view_projection: Option<glam::Mat4>,
    sent: HashMap<u16, (u32, u32)>,
}

/// The vertices of `batch` the scene draws at the detail levels it chose for its objects: the
/// whole batch where nothing in it changes level, otherwise each object's chosen level alone. An
/// object drawn as a card turned to the eye is left out, as is every level the scene does not
/// draw: what the presentation shades and casts shadows with is what the frame shows.
fn drawn_vertices(batch: &super::StaticBatch, placements: &[super::DegradePlacement]) -> Vec<u8> {
    if batch.chunks.is_empty() {
        return batch.vertices.clone();
    }
    let mut out = Vec::new();
    for c in &batch.chunks {
        let drawn = if c.placement == super::NO_PLACEMENT {
            true
        } else {
            placements
                .get(c.placement as usize)
                .is_some_and(|p| p.level == c.level && !p.billboards)
        };
        if drawn {
            if let Some(v) = batch.vertices.get(c.start as usize..c.end as usize) {
                out.extend_from_slice(v);
            }
        }
    }
    out
}

/// The detail levels a block's objects stand at, as one number: it changes when any of them does.
fn levels_key(placements: &[super::DegradePlacement]) -> u32 {
    placements.iter().fold(0x811C_9DC5u32, |h, p| {
        (h ^ p.level).wrapping_mul(0x0100_0193)
    })
}

/// The level a box asks of its effect: a ticked box is `on`; a number past 1 (from `--set-at`)
/// keeps the level it names.
fn level_of(raw: u32, on: Level) -> Level {
    match raw {
        0 => Level::Off,
        1 => on,
        n => Level::from_raw(n),
    }
}

/// The presentation's settings for `prefs` on a device that traces rays when `rays` says so:
/// each option as it is in effect (nothing outside the Horizon interface), less what cannot draw.
/// The shadows, the bounced light and the lamps are drawn inside the better lighting and are off
/// without it; the lamps are off without ray queries. (The ambient occlusion stands aside, frame
/// by frame, only where the bounced light is drawn: the presentation decides that.) The stored
/// values are untouched, so a prerequisite turned back on
/// brings back what depends on it.
pub(crate) fn settings(prefs: &FidelityPreferences, rays: bool) -> HifiSettings {
    let raw = |f| prefs.effective(f);
    // The better lighting's ticked box is its light alone: the shadows have a box of their own.
    let lighting = level_of(raw(FidelityFeature::Lighting), Level::Low);
    let lit = lighting.is_on();
    let inside = |f| {
        if lit {
            level_of(raw(f), Level::High)
        } else {
            Level::Off
        }
    };
    let global_illumination = inside(FidelityFeature::GlobalIllumination);
    HifiSettings {
        lighting,
        shadows: inside(FidelityFeature::Shadows),
        global_illumination,
        ambient_occlusion: level_of(raw(FidelityFeature::AmbientOcclusion), Level::High),
        lamps: if rays {
            inside(FidelityFeature::Lamps)
        } else {
            Level::Off
        },
        sky: level_of(raw(FidelityFeature::Sky), Level::High),
        debug: DebugView::from_raw(raw(FidelityFeature::Debug)),
    }
}

/// Whether the device traces rays, as the presentation would find it.
fn device_rays(gpu: &Gpu) -> bool {
    gpu.hifi_device_features().is_some_and(|(f, _)| {
        f.contains(dereth_render::wgpu::sidecar::wgpu::Features::EXPERIMENTAL_RAY_QUERY)
    })
}

/// The installed presentation, when the device has one.
fn renderer(gpu: &mut Gpu) -> Option<&mut HifiRenderer> {
    gpu.hifi_sidecar_mut()?
        .as_any_mut()
        .downcast_mut::<HifiRenderer>()
}

/// What applying the preferences did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Applied {
    /// They differed from the ones last applied, and the presentation was installed, changed or
    /// removed.
    pub changed: bool,
    /// They asked for the presentation, and the device cannot draw it.
    pub refused: bool,
}

/// Apply `prefs` when they differ from the ones last applied (`sh`): install the presentation,
/// change its settings, or remove it. Preferences that did not change do nothing, so a
/// presentation that failed and was removed stays removed until they do.
pub(super) fn apply_preferences(
    sh: &mut Shadow,
    prefs: &FidelityPreferences,
    gpu: &mut Gpu,
) -> Applied {
    let rays = device_rays(gpu);
    // Before the first poll the presentation is off, as the default preferences have it.
    let last = sh.applied.unwrap_or((FidelityPreferences::default(), rays));
    sh.applied = Some((*prefs, rays));
    if last == (*prefs, rays) {
        return Applied::default();
    }
    // A new start: the presentation's retained copies go with its old settings.
    sh.sent.clear();
    sh.previous_view_projection = None;
    let wanted = settings(prefs, rays);
    let mut refused = false;
    if wanted.any_effective() {
        match renderer(gpu) {
            Some(r) => r.configure(wanted),
            None => {
                if let Err(why) = gpu.hifi_install(Box::new(HifiRenderer::new(wanted))) {
                    use dereth_render::wgpu::sidecar::HifiRefused;
                    refused = matches!(
                        why,
                        HifiRefused::NotWgpu | HifiRefused::NotRequested | HifiRefused::Unsupported
                    );
                    tracing::warn!(
                        target: "dereth::render",
                        "the high-fidelity presentation was not turned on: {why}"
                    );
                }
            }
        }
    } else {
        drop(gpu.hifi_take());
    }
    Applied {
        changed: true,
        refused,
    }
}

impl SceneDraw {
    /// How many placements the resident blocks keep for their lamps: none on a device that
    /// cannot draw them.
    #[must_use]
    pub fn hifi_lamp_sites(&self) -> usize {
        self.blocks.values().map(|b| b.hifi_lamps.sites()).sum()
    }

    /// How many resident blocks have had their lamps looked for: none while the high-fidelity
    /// presentation does not draw the lamps.
    #[must_use]
    pub fn hifi_lamp_blocks_looked(&self) -> usize {
        self.blocks
            .values()
            .filter(|b| b.hifi_lamps.looked())
            .count()
    }
}

/// Whether the installed presentation draws the lamps, so the blocks' lamps are looked for.
pub(super) fn lamps_drawn(gpu: &mut Gpu) -> bool {
    renderer(gpu).is_some_and(|r| r.settings().lamps.is_on())
}

/// Whether this device could ever draw the lamps: it was asked for the presentation at start-up
/// and traces rays. Only then does a block's bake keep where its lamps may be.
pub(super) fn lamps_possible(gpu: &Gpu) -> bool {
    gpu.hifi_requested() && device_rays(gpu)
}

/// Hand the presentation the snapshot of the frame whose world was just drawn, `sh` being what
/// the bridge remembers between frames. Nothing is built when no presentation is installed.
pub(super) fn publish(draw: &SceneDraw, sh: &mut Shadow, ws: &WorldState, gpu: &mut Gpu) {
    if renderer(gpu).is_none() {
        return;
    }
    let stamp = gpu.frame_stamp();
    let (width, height) = gpu.size();
    let reuse = renderer(gpu).and_then(HifiRenderer::recycle_frame);
    let frame = snapshot(draw, ws, stamp, (width, height), sh, reuse);
    if let Some(r) = renderer(gpu) {
        r.receive_frame(frame);
    }
}

fn v3(v: dereth_primitives::Vec3) -> glam::Vec3 {
    glam::Vec3::new(v.x, v.y, v.z)
}

/// The near and far planes a left-handed perspective transform was made with.
fn planes(projection: glam::Mat4) -> (f32, f32) {
    let a = projection.z_axis.z;
    let b = projection.w_axis.z;
    if a.abs() < f32::EPSILON || (a - 1.0).abs() < f32::EPSILON {
        return (0.1, 1000.0);
    }
    let near = -b / a;
    (near, a * near / (a - 1.0))
}

/// A packed `0xAARRGGBB` colour's red, green and blue.
fn rgb(argb: u32) -> [u8; 3] {
    let [_, r, g, b] = argb.to_be_bytes();
    [r, g, b]
}

/// The eye `view` was drawn from, in render space (height third). Every pass places the eye by
/// this, so it always agrees with the view, the depth and the blocks' origins, whatever camera
/// placed it.
fn drawn_eye(view: glam::Mat4) -> glam::Vec3 {
    // In double precision: a single-precision inverse is off by a centimetre at a high eye.
    let eye = view.as_dmat4().inverse().w_axis.as_vec4();
    glam::Vec3::new(eye.x, eye.z, eye.y)
}

#[allow(clippy::too_many_lines)]
fn snapshot(
    draw: &SceneDraw,
    ws: &WorldState,
    stamp: u64,
    (width, height): (u32, u32),
    sh: &mut Shadow,
    reuse: Option<HifiFrame>,
) -> HifiFrame {
    let mut frame = reuse.unwrap_or_default();
    frame.stamp = stamp;

    // --- the camera -----------------------------------------------------------------------
    let view = draw.view_params(ws, width, height);
    let projection = dereth_render::camera::projection(&view);
    let view_projection = projection * view.view;
    let (near, far) = planes(projection);
    let previous_view_projection = sh.previous_view_projection;
    sh.previous_view_projection = Some(view_projection);
    let shift = draw.block_shift(ws);
    frame.camera = HifiCamera {
        view: view.view,
        projection,
        previous_view_projection,
        eye: drawn_eye(view.view),
        viewport: [
            view.viewport.x,
            view.viewport.y,
            view.viewport.width,
            view.viewport.height,
        ],
        fov_y: view.fov_y_rad,
        aspect: view.aspect,
        near,
        far,
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // LINT-OK: block indices are 0..=0xFE. Not a float conversion.
        viewer_landblock: ws
            .streamer
            .window
            .viewer_block()
            .map_or(0, |(x, y)| ((x as u16) << 8) | (y as u16)),
        block_shift: v3(shift),
    };

    // --- the sun, the fog and the sky -----------------------------------------------------
    let lighting = &draw.land.lighting;
    let sun = v3(lighting.sunlight);
    let fog = draw.world_fog_state(ws);
    let region = draw.sky_region();
    let group = dereth_world_render::sky::present_day_group(
        region,
        ws.clock.current_year,
        ws.clock.current_day,
    );
    frame.sky = HifiSky {
        sun_direction: sun.normalize_or_zero(),
        sun_brightness: sun.length(),
        sun_color: lighting.sunlight_color,
        ambient_level: lighting.ambient_level,
        ambient_color: lighting.ambient_color,
        fog: fog.enabled.then_some(HifiFog {
            min: fog.near,
            max: fog.far,
            color: rgb(fog.color),
        }),
        objects: group
            .map(|g| {
                dereth_world_render::sky::get_sky(g, ws.clock.present_time_of_day)
                    .iter()
                    .zip(0u32..)
                    .map(|(o, index)| HifiSkyObject {
                        index,
                        gfx_id: o.gfx_id.0,
                        heading: o.heading,
                        rotation: o.rotation,
                        transparent: o.transparent,
                        luminosity: o.luminosity,
                        max_bright: o.max_bright,
                        properties: o.properties,
                    })
                    .collect()
            })
            .unwrap_or_default(),
        day_group: group.map(|g| g.day_name.clone()).unwrap_or_default(),
        weather_enabled: draw.weather_enabled().unwrap_or(false),
        outdoor: ws.viewer_cell().is_none(),
        time_of_day: ws.clock.present_time_of_day,
        year: ws.clock.current_year,
        day: ws.clock.current_day,
    };

    // --- the lights -----------------------------------------------------------------------
    frame.lights.clear();
    // The static pool is drawn from the cell lights below, so only the dynamic lights are taken
    // from the pools. A pooled light's hardware record holds its position with height on the
    // second axis and its colour times its intensity; the snapshot holds height on the third
    // axis and the colour alone, as for the cell lights.
    //
    // Outdoors the landscape and everything standing on it, the player's body too, are drawn
    // with the sun alone, so a dynamic light standing outdoors (the light that follows the
    // player, while the player is outdoors) lights only the rooms the frame shows and what
    // stands in them, whose colour as drawn already holds it. Lit again here it would pool on
    // the ground round the player.
    let pools = draw.light_pools();
    for l in pools
        .dynamics
        .iter()
        .filter(|l| !dereth_physics::landdefs::is_outdoors(l.cell_id))
    {
        let [x, height, y] = l.d3d.position;
        frame.lights.push(HifiLight {
            position: glam::Vec3::new(x, y, height),
            color: l.info.color,
            intensity: l.info.intensity,
            falloff: l.info.falloff,
            interior: true,
        });
    }
    // Every cell light belongs to a building's or dungeon's own cell.
    for block in draw.blocks.values() {
        for light in &block.cell_lights {
            let at = dereth_physics::math::localtoglobal(&light.frame, light.info.offset.origin);
            frame.lights.push(HifiLight {
                position: glam::Vec3::new(block.origin.0 + at.x, block.origin.1 + at.y, at.z),
                color: light.info.color,
                intensity: light.info.intensity,
                falloff: light.info.falloff,
                interior: true,
            });
        }
    }

    // --- the outdoor lamps -----------------------------------------------------------------
    frame.lamps.clear();
    for block in draw.blocks.values() {
        for l in block.hifi_lamps.placed() {
            use super::hifi_lamps::LampKind;
            let p = l.position;
            frame.lamps.push(HifiLamp {
                position: glam::Vec3::new(block.origin.0 + p.x, block.origin.1 + p.y, p.z),
                color: l.source.color,
                intensity: l.source.intensity,
                falloff: l.source.falloff,
                source: match l.source.kind {
                    LampKind::Authored => LampSource::Authored,
                    LampKind::Flame => LampSource::Flame,
                    LampKind::Glow => LampSource::Glow,
                },
                flicker: l.source.flicker,
                // LINT-OK: hashing the bits of a position. Not a float conversion.
                seed: p.x.to_bits() ^ p.y.to_bits().rotate_left(11) ^ p.z.to_bits().rotate_left(22),
            });
        }
    }

    // --- the landscape ---------------------------------------------------------------------
    frame.terrain.clear();
    frame.static_feed.clear();
    let window = &ws.streamer.window;
    let mid = window.mid_width();
    let centre = i64::from(mid / 2);
    let mut feed_bytes = 0usize;
    let mut resident: Vec<u16> = Vec::new();
    for xi in 0..mid {
        for yi in 0..mid {
            let Some(slot) = window.slot(xi, yi) else {
                continue;
            };
            let Some(block) = draw.blocks.get(&(slot.block_x, slot.block_y)) else {
                continue;
            };
            let Some(mesh) = slot.mesh.as_ref().and_then(
                dereth_client_runtime::world_stream::SlotMesh::get::<
                    dereth_terrain::land::mesh::LandblockMesh,
                >,
            ) else {
                continue;
            };
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // LINT-OK: block indices are 0..=0xFE. Not a float conversion.
            let id = ((slot.block_x as u16) << 8) | (slot.block_y as u16);
            resident.push(id);
            let ring = (i64::from(xi) - centre)
                .abs()
                .max((i64::from(yi) - centre).abs());
            let origin = glam::Vec3::new(block.origin.0, block.origin.1, 0.0);
            let n = usize::from(mesh.side_cell_count);
            let side = usize::from(mesh.side_vertex_count);
            frame.terrain.push(HifiTerrainBlock {
                block: id,
                ring: u8::try_from(ring).unwrap_or(u8::MAX),
                origin,
                words: block.terrain,
                side: mesh.side_vertex_count,
                heights: mesh.vertices.iter().map(|v| v.z).collect(),
                cuts: mesh.sw_to_ne_cut.clone(),
                generation: block.hifi_generation,
            });
            // The retained geometry, once per block contents and detail levels, a few megabytes
            // a frame at most.
            let levels = levels_key(&block.degrade);
            let key = (block.hifi_generation, levels);
            if sh.sent.get(&id) == Some(&key) || feed_bytes > FEED_BYTES_PER_FRAME {
                continue;
            }
            let statics = block
                .opaque
                .iter()
                .chain(&block.blended)
                .map(|b| (b, drawn_vertices(b, &block.degrade)))
                .chain(block.env_cells.iter().flat_map(|c| {
                    c.statics
                        .iter()
                        .chain(&c.statics_blended)
                        .map(|b| (b, drawn_vertices(b, &c.degrade)))
                }))
                .filter(|(_, v)| !v.is_empty())
                .map(|(b, bytes)| HifiStaticBatch {
                    format: b.key.vertex_format,
                    ranges: std::iter::once(0..u32::try_from(bytes.len()).unwrap_or(u32::MAX))
                        .collect(),
                    bytes,
                    cutout: b.key.alpha_blend || b.key.alpha_test,
                    texture: b.texture.map(|t| t.0),
                    alpha_ref: b.alpha_ref,
                });
            // The buildings' rooms too, and what stands in them: a doorway's lintel and a
            // porch's roof are a room's geometry, and the sun is shut out by them as by the
            // walls outside, and so are the lamps.
            let rooms = block
                .env_cells
                .iter()
                .flat_map(|c| &c.meshes)
                .filter(|m| !m.vertices.is_empty())
                .map(|m| HifiStaticBatch {
                    format: m.key.vertex_format,
                    ranges: std::iter::once(0..u32::try_from(m.vertices.len()).unwrap_or(u32::MAX))
                        .collect(),
                    bytes: m.vertices.clone(),
                    cutout: m.key.alpha_blend || m.key.alpha_test,
                    texture: m.texture.map(|t| t.0),
                    alpha_ref: m.alpha_ref,
                });
            let batches: Vec<HifiStaticBatch> = statics.chain(rooms).collect();
            let mut terrain_triangles = Vec::with_capacity(n * n * 2);
            let p = |a: usize, b: usize| {
                mesh.vertices
                    .get(a * side + b)
                    .map_or(glam::Vec3::ZERO, |v| glam::Vec3::new(v.x, v.y, v.z))
            };
            for i in 0..n {
                for j in 0..n {
                    let (sw, se, ne, nw) = (p(i, j), p(i + 1, j), p(i + 1, j + 1), p(i, j + 1));
                    if mesh.sw_to_ne_cut.get(i * n + j).copied().unwrap_or(true) {
                        terrain_triangles.push([sw, se, ne]);
                        terrain_triangles.push([sw, ne, nw]);
                    } else {
                        terrain_triangles.push([sw, se, nw]);
                        terrain_triangles.push([se, ne, nw]);
                    }
                }
            }
            feed_bytes +=
                batches.iter().map(|b| b.bytes.len()).sum::<usize>() + terrain_triangles.len() * 36;
            frame.static_feed.push(HifiStaticBlock {
                block: id,
                // The presentation keeps a block's copy until this changes: with the contents,
                // or with any object's detail level.
                generation: block.hifi_generation.wrapping_mul(0x9E37_79B9) ^ levels,
                batches,
                terrain_triangles,
            });
            sh.sent.insert(id, key);
        }
    }
    sh.sent.retain(|id, _| resident.contains(id));

    frame
}

#[cfg(test)]
mod tests {
    use super::*;

    use dereth_client_contract::options::fidelity::{blocked, OPTIONS};
    use dereth_render_hifi::composite::CompositeMode;

    /// The preferences with the boxes `ticked` (one bit each, in the page's order) under the
    /// Horizon interface.
    fn boxes(ticked: u32) -> FidelityPreferences {
        let mut p = FidelityPreferences {
            interface: true,
            ..FidelityPreferences::default()
        };
        for (i, o) in OPTIONS.iter().enumerate() {
            let at = FidelityFeature::ALL
                .iter()
                .position(|f| f.name() == o.name)
                .expect("every box is an option");
            p.values[at] = u32::from(ticked & (1 << i) != 0);
        }
        p
    }

    /// The level the settings give the box `name`.
    fn level(s: &HifiSettings, name: &str) -> Level {
        use dereth_client_contract::options::names::fidelity as n;
        match name {
            n::LIGHTING => s.lighting,
            n::SHADOWS => s.shadows,
            n::GLOBAL_ILLUMINATION => s.global_illumination,
            n::AMBIENT_OCCLUSION => s.ambient_occlusion,
            n::LAMPS => s.lamps,
            n::SKY => s.sky,
            _ => unreachable!("{name}"),
        }
    }

    /// Behaviour: hifi.options.the-presentation-settings-follow-the-preferences-in-effect
    #[test]
    fn the_settings_are_off_outside_horizon_and_follow_each_box_inside_it() {
        assert_eq!(
            settings(&FidelityPreferences::default(), true),
            HifiSettings::default()
        );
        // Every box ticked, under another interface: nothing on.
        let mut every = boxes(0b11_1111);
        every.interface = false;
        assert_eq!(settings(&every, true), HifiSettings::default());
        assert!(!settings(&every, true).any_effective());
        every.interface = true;
        let on = settings(&every, true);
        assert_eq!(
            on.lighting,
            Level::Low,
            "the light alone: shadows have their own box"
        );
        assert_eq!(on.shadows, Level::High);
        assert_eq!(on.global_illumination, Level::High);
        assert_eq!(
            on.ambient_occlusion,
            Level::High,
            "kept with the bounced light: indoors and underground it is the only effect drawn"
        );
        assert_eq!(on.lamps, Level::High);
        assert_eq!(on.sky, Level::High);
        assert_eq!(on.debug, DebugView::Off);
        // A capture's level past a tick is kept.
        let mut rig = FidelityPreferences::parse_switch("Lighting=3,Shadows=4,Debug=6").unwrap();
        assert_eq!(settings(&rig, true).lighting, Level::High);
        assert_eq!(settings(&rig, true).shadows, Level::Ultra);
        assert_eq!(settings(&rig, true).debug, DebugView::Census);
        rig.interface = false;
        assert!(!settings(&rig, true).any_effective());
    }

    /// Every combination of the six boxes, with and without ray queries: a box draws exactly when
    /// it is ticked and the page does not grey it, nothing is re-shaded without the better
    /// lighting, and a dependent comes back when its prerequisite is ticked again.
    ///
    /// Behaviour: hifi.options.no-combination-of-the-boxes-draws-what-cannot-draw
    #[test]
    fn every_combination_of_the_boxes_draws_only_what_its_prerequisites_allow() {
        for rays in [false, true] {
            for ticked in 0..64u32 {
                let prefs = boxes(ticked);
                let s = settings(&prefs, rays);
                let is = |name: &str| {
                    OPTIONS
                        .iter()
                        .position(|o| o.name == name)
                        .is_some_and(|i| ticked & (1 << i) != 0)
                };
                for o in &OPTIONS {
                    let drawn = level(&s, o.name).is_on();
                    let greyed = blocked(o.name, is, Some(rays)).is_some();
                    assert_eq!(
                        drawn,
                        is(o.name) && !greyed,
                        "{} with boxes {ticked:06b}, rays {rays}",
                        o.name
                    );
                }
                assert_eq!(
                    CompositeMode::for_settings(&s) == CompositeMode::Reshade,
                    s.lighting.is_on(),
                    "boxes {ticked:06b}: re-shaded exactly with the better lighting"
                );
                if !s.lighting.is_on() {
                    assert!(
                        !s.shadows.is_on() && !s.global_illumination.is_on() && !s.lamps.is_on()
                    );
                }
            }
        }
        // Lighting off keeps the dependents' ticks; on again, they draw again.
        let both = boxes(0b00_0011);
        assert!(settings(&both, true).shadows.is_on());
        let unlit = boxes(0b00_0010);
        assert!(!settings(&unlit, true).shadows.is_on());
        assert_eq!(unlit.value(FidelityFeature::Shadows), 1);
        assert_eq!(settings(&boxes(0b00_0011), true), settings(&both, true));
    }

    /// The planes come back out of the projection they went into.
    #[test]
    fn the_near_and_far_planes_come_back_out_of_the_projection() {
        let v = dereth_render::ViewParams {
            znear: 0.5,
            zfar: 2400.0,
            ..dereth_render::ViewParams::default()
        };
        let (near, far) = planes(dereth_render::camera::projection(&v));
        assert!((near - 0.5).abs() < 1e-3, "{near}");
        assert!((far - 2400.0).abs() < 1.0, "{far}");
    }
}
