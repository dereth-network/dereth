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
//!   presentation needs about it (camera, sun, fog, sky, lights, lamps, landscape, the retained
//!   geometry of blocks that changed, and the art pack with where its models stand and the ground
//!   it lays, with each cell's layers as retail lays them, and its materials for retail pictures with
//!   the texture slots holding those pictures) is handed to the presentation. It is built from
//!   shared reads after the frame's walk has finished, so it cannot change what the frame drew or
//!   what can be picked in it. With no presentation installed nothing is built.
//!
//! The bridge's own memory between frames ([`Shadow`]) is handed to it by its two callers, so
//! nothing here writes through a shared reference: the scene and the world are only read.

use std::collections::HashMap;

use dereth_client_runtime::render_prefs::{FidelityFeature, FidelityPreferences};
use dereth_render_hifi::snapshot::{
    AskedFall, HifiArtPack, HifiCamera, HifiFog, HifiFrame, HifiGround, HifiGroundMaterial,
    HifiLamp, HifiLight, HifiMaterials, HifiPackBlock, HifiPackImage, HifiPackLod,
    HifiPackMaterial, HifiPackModel, HifiPackPlacement, HifiPackPrimitive, HifiSky, HifiSkyObject,
    HifiStaticBatch, HifiStaticBlock, HifiTerrainBlock, HifiTextureMaterial, LampSource,
};
use dereth_render_hifi::{DebugView, HifiRenderer, HifiSettings, Level};

use super::{Gpu, SceneDraw, WorldState};

/// Retained geometry sent in one frame at most, bytes; the rest follows in later frames.
const FEED_BYTES_PER_FRAME: usize = 4 << 20;

/// What the bridge remembers between frames: the preferences and the device's ray queries it
/// last applied, the scene's clock at its last step, the previous frame's camera, and, for each
/// block whose geometry the presentation holds, the block's generation and the detail levels its
/// objects stood at when it was sent.
#[derive(Debug, Default)]
pub(crate) struct Shadow {
    applied: Option<(FidelityPreferences, bool)>,
    pub(super) now: f64,
    previous_view_projection: Option<glam::Mat4>,
    sent: HashMap<u16, (u32, u32)>,
    /// The art pack as the presentation draws it, made once per pack (by its serial).
    pack: Option<(u64, std::sync::Arc<HifiArtPack>)>,
    /// The art pack's materials for retail pictures as the presentation draws them, made once
    /// per pack (by its serial).
    materials: Option<(u64, std::sync::Arc<HifiMaterials>)>,
    /// For each block whose art pack placements the presentation holds, the block's generation.
    sent_pack: HashMap<u16, u32>,
    /// The art pack's ground as the presentation draws it, made once per retail ground read (by
    /// its serial).
    ground: Option<(u64, std::sync::Arc<HifiGround>)>,
    /// Each resident block's cell layers, with the block's generation and cells a side they were
    /// worked out for.
    ground_cells: HashMap<u16, ((u32, u8), CellLayers)>,
}

/// A block's cell layers, as the presentation reads them.
type CellLayers = std::sync::Arc<[[u32; 4]; 64]>;

/// A picture of the art pack as the presentation reads it. Its levels are shared, not copied.
fn picture(i: &crate::art_pack::Image) -> HifiPackImage {
    HifiPackImage {
        width: i.width,
        height: i.height,
        srgb: i.srgb,
        levels: i.levels.clone(),
        format: i.format,
    }
}

/// The art pack's materials for retail pictures as the presentation draws them.
fn materials_for_presentation(pack: &crate::art_pack::ArtPack) -> HifiMaterials {
    HifiMaterials {
        serial: pack.serial,
        materials: pack
            .materials
            .iter()
            .map(|m| HifiTextureMaterial {
                albedo: picture(&m.albedo),
                normal: picture(&m.normal),
                surface: picture(&m.surface),
            })
            .collect(),
    }
}

/// The art pack's ground as the presentation draws it: the pack's materials, then retail's
/// pictures for the grounds the pack has none for, and retail's masks.
fn ground_for_presentation(
    pack: &crate::art_pack::ArtPack,
    retail: &crate::art_pack::RetailGround,
) -> HifiGround {
    use crate::art_pack::Ground;
    let mut materials = Vec::new();
    let mut of_ground = vec![None; 33];
    for g in &pack.grounds {
        let index = u16::try_from(materials.len()).ok();
        for k in &g.grounds {
            let n = match k {
                Ground::Type(t) => usize::from(*t),
                Ground::Road => 32,
            };
            if n < of_ground.len() {
                of_ground[n] = index;
            }
        }
        materials.push(HifiGroundMaterial {
            albedo: picture(&g.albedo),
            normal: picture(&g.normal),
            surface: picture(&g.surface),
            size: g.size,
            normal_scale: g.normal_scale,
            retail: false,
        });
    }
    for p in &retail.pictures {
        let index = u16::try_from(materials.len()).ok();
        for g in &p.grounds {
            let n = usize::from(*g);
            if n < of_ground.len() {
                of_ground[n] = index;
            }
        }
        #[allow(clippy::cast_precision_loss)] // a handful of repeats
        let size = dereth_terrain::consts::CELL_SIZE / p.tiling.max(1) as f32;
        materials.push(HifiGroundMaterial {
            albedo: picture(&p.albedo),
            normal: picture(&p.normal),
            surface: picture(&p.surface),
            size,
            normal_scale: 1.0,
            retail: true,
        });
    }
    HifiGround {
        serial: retail.serial,
        materials,
        of_ground,
        masks: retail.masks.iter().map(|(_, i)| picture(i)).collect(),
        broad: retail
            .broad
            .iter()
            .map(|(i, tiling)| (picture(i), *tiling))
            .collect(),
    }
}

/// The layers of each cell of a block with terrain words `words`, as retail lays them over `tm`
/// with the block drawn `cells` cells a side (8 at full detail; 4, 2 or 1 farther off, each
/// drawn cell's layers laid over every cell it covers), naming masks as `retail` holds them; a
/// terrain type the ground has no picture for (`drawn`) is laid as retail lays it, as its
/// neighbours'.
fn ground_layers(
    tm: &dereth_assets::region::TexMerge,
    retail: &crate::art_pack::RetailGround,
    words: &[u16; 81],
    drawn: u32,
    cells: u8,
) -> [[u32; 4]; 64] {
    use dereth_render_hifi::passes::ground::{cell_words, CellLayer};
    use dereth_terrain::land::merge::{ground_plan, MergeKey};
    let filled = dereth_terrain::land::fill::fill_undrawn_terrain(
        &dereth_assets::world::CellLandblock {
            id: dereth_primitives::DataId(0),
            lbi_exists: 0,
            terrain: *words,
            height: [0; 81],
        },
        drawn,
    );
    let words = filled.as_ref().map_or(words, |lb| &lb.terrain);
    let mut out = [[0u32; 4]; 64];
    let cells = usize::from(cells.clamp(1, 8));
    // A drawn cell's corners are every `step`th of the block's words.
    let step = 8 / cells;
    // LINT-OK: terrain types, road values, mask indices and rotations, each a few bits.
    #[allow(clippy::cast_possible_truncation)]
    for e in 0..cells {
        for n in 0..cells {
            let w = |a: usize, b: usize| words[a * step * 9 + b * step];
            let corners = [w(e, n), w(e + 1, n), w(e + 1, n + 1), w(e, n + 1)];
            let key = MergeKey::new(corners.map(|c| (c >> 2) & 0x1F), corners.map(|c| c & 3), 1);
            let plan = ground_plan(tm, key);
            let overlays: Vec<CellLayer> = plan
                .overlays
                .iter()
                .filter_map(|o| {
                    Some(CellLayer {
                        ground: o.ground.map(|g| g.min(63) as u8),
                        mask: retail.mask(o.alpha)?,
                        rotation: o.rotation.index() as u8,
                    })
                })
                .collect();
            let laid = cell_words(plan.base.map(|g| g.min(63) as u8), &overlays, step as u8);
            for de in 0..step {
                for dn in 0..step {
                    out[(e * step + de) * 8 + n * step + dn] = laid;
                }
            }
        }
    }
    out
}

/// Where one of the art pack's models stands, as the presentation places it: block-local, turned
/// and scaled as the object it replaces is baked.
fn pack_placement(p: &crate::art_pack::PackPlacement) -> HifiPackPlacement {
    let q = p.frame.rotation;
    HifiPackPlacement {
        id: p.id.0,
        origin: v3(p.frame.origin),
        rotation: glam::Quat::from_xyzw(q.x, q.y, q.z, q.w),
        scale: p.scale,
    }
}

/// The art pack `pack` as the presentation draws it. Its vertices and pictures are shared, not
/// copied.
fn pack_for_presentation(pack: &crate::art_pack::ArtPack) -> HifiArtPack {
    HifiArtPack {
        serial: pack.serial,
        models: pack
            .models
            .iter()
            .map(|m| HifiPackModel {
                id: m.id.0,
                foliage: m.foliage,
                lods: m
                    .lods
                    .iter()
                    .map(|l| HifiPackLod {
                        distance: l.distance,
                        views: l.views.map(|v| (v.count, v.columns)),
                        shadows_only: l.shadows_only,
                        primitives: l
                            .primitives
                            .iter()
                            .map(|p| HifiPackPrimitive {
                                vertices: std::sync::Arc::clone(&p.vertices),
                                indices: std::sync::Arc::clone(&p.indices),
                                material: HifiPackMaterial {
                                    base_color: p.material.base_color,
                                    base_color_image: p.material.base_color_image,
                                    normal_image: p.material.normal_image,
                                    normal_scale: p.material.normal_scale,
                                    metal_rough_image: p.material.metal_rough_image,
                                    metallic: p.material.metallic,
                                    roughness: p.material.roughness,
                                    occlusion_image: p.material.occlusion_image,
                                    occlusion_strength: p.material.occlusion_strength,
                                    alpha_cutoff: p.material.alpha_cutoff,
                                    double_sided: p.material.double_sided,
                                },
                            })
                            .collect(),
                    })
                    .collect(),
                bounds: (glam::Vec3::from(m.bounds.0), m.bounds.1),
            })
            .collect(),
        images: pack.images.iter().map(picture).collect(),
    }
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
/// The shadows, the bounced light, the lamps and the weather are drawn inside the better lighting
/// and are off without it; the lamps are off without ray queries. (The ambient occlusion stands
/// aside, frame by frame, only where the bounced light is drawn: the presentation decides that.)
/// The stored values are untouched, so a prerequisite turned back on brings back what depends on
/// it.
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
        weather: inside(FidelityFeature::Weather),
        // An art pack's models are drawn for the light per pixel, unless the retail look is
        // asked for; so is the anti-aliasing, unless it is turned off.
        art_pack: lit && raw(FidelityFeature::RetailLook) == 0,
        anti_aliasing: lit && raw(FidelityFeature::NoAntiAliasing) == 0,
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
    sh.sent_pack.clear();
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
    let mut frame = snapshot(draw, ws, stamp, (width, height), sh, reuse);
    // The slots holding the retail pictures the pack's materials stand for, as they are now.
    frame.material_slots.clear();
    if frame.materials.is_some() {
        for (key, m) in &draw.land.material_keys {
            if let Some(slot) = gpu.texture_slot_of(*key) {
                frame.material_slots.push((slot.0, *m));
            }
        }
    }
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
    frame.time = sh.now;

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
        // The presentation sets its own jitter.
        jitter: glam::Vec2::ZERO,
        dither: 0.0,
    };

    // --- the sun, the fog and the sky -----------------------------------------------------
    let lighting = &draw.land.lighting;
    let sun = v3(lighting.sunlight);
    let fog = draw.world_fog_state(ws);
    let region = draw.sky_region();
    let group = dereth_world_render::sky::drawn_day_group(
        region,
        ws.clock.current_year,
        ws.clock.current_day,
        super::lighting::asked_day(ws),
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
        sees_outside: ws
            .viewer_cell()
            .is_none_or(|cell| ws.cell_seen_outside(cell)),
        asked_fall: match ws.weather {
            dereth_client_runtime::weather::Weather::Rain => AskedFall::Rain,
            dereth_client_runtime::weather::Weather::Snow => AskedFall::Snow,
            dereth_client_runtime::weather::Weather::Auto
            | dereth_client_runtime::weather::Weather::Clear => AskedFall::Land,
        },
        time_of_day: ws.clock.present_time_of_day,
        year: ws.clock.current_year,
        day: ws.clock.current_day,
    };

    // --- the lights -----------------------------------------------------------------------
    frame.lights.clear();
    // A pooled dynamic light's hardware record holds its position with height on the second
    // axis and its color times its intensity; the snapshot holds height on the third axis and
    // the color alone, as for the cell lights.
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
            burned: false,
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
                burned: false,
            });
        }
    }
    // And the static pool once more, exactly as the rooms' vertex colors are burned from it: its
    // lights are already in render space, with the color they were burned with.
    for l in &pools.statics {
        if l.info.light_type != dereth_world_render::lighting::LightType::Point {
            continue;
        }
        let at = l.info.offset.origin;
        frame.lights.push(HifiLight {
            position: glam::Vec3::new(at.x, at.y, at.z),
            color: l.info.color,
            intensity: l.info.intensity,
            falloff: l.info.falloff,
            interior: true,
            burned: true,
        });
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

    // --- the art pack ----------------------------------------------------------------------
    frame.art_pack = match draw.land.art_pack.as_deref() {
        Some(pack) => {
            if sh
                .pack
                .as_ref()
                .is_none_or(|(serial, _)| *serial != pack.serial)
            {
                sh.pack = Some((
                    pack.serial,
                    std::sync::Arc::new(pack_for_presentation(pack)),
                ));
                sh.sent_pack.clear();
            }
            sh.pack.as_ref().map(|(_, p)| std::sync::Arc::clone(p))
        }
        None => None,
    };
    // --- the art pack's materials for retail pictures ---------------------------------------
    frame.materials = match draw
        .land
        .art_pack
        .as_deref()
        .filter(|p| !p.materials.is_empty())
    {
        Some(pack) => {
            if sh
                .materials
                .as_ref()
                .is_none_or(|(serial, _)| *serial != pack.serial)
            {
                sh.materials = Some((
                    pack.serial,
                    std::sync::Arc::new(materials_for_presentation(pack)),
                ));
            }
            sh.materials.as_ref().map(|(_, m)| std::sync::Arc::clone(m))
        }
        None => {
            sh.materials = None;
            None
        }
    };
    // --- the art pack's ground ------------------------------------------------------------
    frame.ground = match (
        draw.land.art_pack.as_deref(),
        draw.land.retail_ground.as_deref(),
    ) {
        (Some(pack), Some(retail)) => {
            if sh
                .ground
                .as_ref()
                .is_none_or(|(serial, _)| *serial != retail.serial)
            {
                sh.ground = Some((
                    retail.serial,
                    std::sync::Arc::new(ground_for_presentation(pack, retail)),
                ));
                sh.ground_cells.clear();
            }
            sh.ground.as_ref().map(|(_, g)| std::sync::Arc::clone(g))
        }
        _ => {
            sh.ground = None;
            sh.ground_cells.clear();
            None
        }
    };
    frame.pack_feed.clear();
    if frame.art_pack.is_some() {
        for ((bx, by), block) in &draw.blocks {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // LINT-OK: block indices are 0..=0xFE. Not a float conversion.
            let id = ((*bx as u16) << 8) | (*by as u16);
            if sh.sent_pack.get(&id) == Some(&block.hifi_generation) {
                continue;
            }
            frame.pack_feed.push(HifiPackBlock {
                block: id,
                generation: block.hifi_generation,
                placements: block.art_pack.iter().map(pack_placement).collect(),
            });
            sh.sent_pack.insert(id, block.hifi_generation);
        }
        sh.sent_pack.retain(|id, _| {
            draw.blocks
                .contains_key(&(i32::from(*id >> 8), i32::from(*id & 0xFF)))
        });
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
            let layers = match (frame.ground.as_ref(), draw.land.retail_ground.as_deref()) {
                (Some(_), Some(retail)) => {
                    let made = (block.hifi_generation, mesh.side_cell_count);
                    let fresh = sh
                        .ground_cells
                        .get(&id)
                        .filter(|(m, _)| *m == made)
                        .map(|(_, l)| std::sync::Arc::clone(l));
                    Some(fresh.unwrap_or_else(|| {
                        let l = std::sync::Arc::new(ground_layers(
                            &draw.land.tex_merge,
                            retail,
                            &block.terrain,
                            draw.land.drawn,
                            mesh.side_cell_count,
                        ));
                        sh.ground_cells
                            .insert(id, (made, std::sync::Arc::clone(&l)));
                        l
                    }))
                }
                _ => None,
            };
            frame.terrain.push(HifiTerrainBlock {
                block: id,
                ring: u8::try_from(ring).unwrap_or(u8::MAX),
                origin,
                words: block.terrain,
                side: mesh.side_vertex_count,
                heights: mesh.vertices.iter().map(|v| v.z).collect(),
                cuts: mesh.sw_to_ne_cut.clone(),
                generation: block.hifi_generation,
                layers,
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
                    art_pack: b.art_pack,
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
                    art_pack: false,
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
    sh.ground_cells.retain(|id, _| resident.contains(id));

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

    /// Whether the settings draw what the box `name` asks for: its effect, or for the retail
    /// look's box the original models in place of an art pack's.
    fn drawn(s: &HifiSettings, name: &str) -> bool {
        use dereth_client_contract::options::names::fidelity as n;
        match name {
            n::LIGHTING => s.lighting.is_on(),
            n::SHADOWS => s.shadows.is_on(),
            n::GLOBAL_ILLUMINATION => s.global_illumination.is_on(),
            n::AMBIENT_OCCLUSION => s.ambient_occlusion.is_on(),
            n::LAMPS => s.lamps.is_on(),
            n::SKY => s.sky.is_on(),
            n::WEATHER => s.weather.is_on(),
            n::RETAIL_LOOK => s.lighting.is_on() && !s.art_pack,
            n::NO_ANTI_ALIASING => s.lighting.is_on() && !s.anti_aliasing,
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
        let mut every = boxes((1 << OPTIONS.len()) - 1);
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
        assert_eq!(on.weather, Level::High);
        assert!(!on.art_pack, "the retail look's box is ticked too");
        assert!(!on.anti_aliasing, "the anti-aliasing's box is ticked too");
        assert_eq!(on.debug, DebugView::Off);
        // The art pack is drawn with the better lighting, until the retail look is asked for.
        let lit = settings(&boxes(0b1), true);
        assert!(lit.art_pack);
        assert!(!settings(&boxes(0), true).art_pack);
        // So is the anti-aliasing, until it is turned off.
        assert!(lit.anti_aliasing);
        assert!(!settings(&boxes(0), true).anti_aliasing);
        // A capture's level past a tick is kept.
        let mut rig = FidelityPreferences::parse_switch("Lighting=3,Shadows=4,Debug=6").unwrap();
        assert_eq!(settings(&rig, true).lighting, Level::High);
        assert_eq!(settings(&rig, true).shadows, Level::Ultra);
        assert_eq!(settings(&rig, true).debug, DebugView::Census);
        rig.interface = false;
        assert!(!settings(&rig, true).any_effective());
    }

    /// Every combination of the boxes, with and without ray queries: a box draws exactly when
    /// it is ticked and the page does not grey it, nothing is re-shaded without the better
    /// lighting, and a dependent comes back when its prerequisite is ticked again.
    ///
    /// Behaviour: hifi.options.no-combination-of-the-boxes-draws-what-cannot-draw
    #[test]
    fn every_combination_of_the_boxes_draws_only_what_its_prerequisites_allow() {
        for rays in [false, true] {
            for ticked in 0..(1u32 << OPTIONS.len()) {
                let prefs = boxes(ticked);
                let s = settings(&prefs, rays);
                let is = |name: &str| {
                    OPTIONS
                        .iter()
                        .position(|o| o.name == name)
                        .is_some_and(|i| ticked & (1 << i) != 0)
                };
                for o in &OPTIONS {
                    let drawn = drawn(&s, o.name);
                    let greyed = blocked(o.name, is, Some(rays)).is_some();
                    assert_eq!(
                        drawn,
                        is(o.name) && !greyed,
                        "{} with boxes {ticked:07b}, rays {rays}",
                        o.name
                    );
                }
                assert_eq!(
                    CompositeMode::for_settings(&s) == CompositeMode::Reshade,
                    s.lighting.is_on(),
                    "boxes {ticked:07b}: re-shaded exactly with the better lighting"
                );
                if !s.lighting.is_on() {
                    assert!(
                        !s.shadows.is_on()
                            && !s.global_illumination.is_on()
                            && !s.lamps.is_on()
                            && !s.weather.is_on()
                    );
                }
            }
        }
        // Lighting off keeps the dependents' ticks; on again, they draw again.
        let both = boxes(0b000_0011);
        assert!(settings(&both, true).shadows.is_on());
        let unlit = boxes(0b000_0010);
        assert!(!settings(&unlit, true).shadows.is_on());
        assert_eq!(unlit.value(FidelityFeature::Shadows), 1);
        assert_eq!(settings(&boxes(0b000_0011), true), settings(&both, true));
    }

    /// An art pack's model is turned and placed exactly as the bake turns and places the object
    /// it replaces.
    ///
    /// Behaviour: none (experimental hi-fi art packs)
    #[test]
    fn a_pack_model_stands_where_the_bake_puts_the_object_it_replaces() {
        let (c, s) = (0.6f32, 0.8f32);
        // A turn about a tilted axis, as a scenery object leaning with its slope has.
        let rotation = dereth_primitives::Quat::new(c, s * 0.48, -s * 0.6, s * 0.64);
        let frame = dereth_primitives::Frame::new(
            dereth_primitives::Vec3::new(37.5, 120.25, 61.0),
            rotation,
        );
        let placed = pack_placement(&crate::art_pack::PackPlacement {
            id: dereth_primitives::DataId(0x0200_0258),
            frame,
            scale: 1.0,
        });
        for local in [
            dereth_primitives::Vec3::new(1.0, 0.0, 0.0),
            dereth_primitives::Vec3::new(0.3, -2.0, 5.0),
        ] {
            let baked = dereth_terrain::math::localtoglobal(&frame, local);
            let drawn =
                placed.rotation * glam::Vec3::new(local.x, local.y, local.z) + placed.origin;
            assert!(
                drawn.distance(glam::Vec3::new(baked.x, baked.y, baked.z)) < 1e-4,
                "{drawn} against {baked:?}"
            );
        }
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

    /// Behaviour: none (experimental hi-fi art packs)
    /// A block's cell layers are retail's at full detail, cell by cell east by north, with each
    /// mask named by its index in the retail ground.
    #[test]
    fn a_blocks_cell_layers_are_retails_east_by_north_at_the_detail_it_is_drawn() {
        use dereth_assets::region::{CodeTexture, TerrainDesc, TexMerge};
        use dereth_render_hifi::passes::ground::{cell_layers, cell_span};
        use dereth_terrain::land::merge::{ground_plan, MergeKey};
        let desc = |t: u32, gid: u32| TerrainDesc {
            terrain_type: t,
            tex_gid: dereth_primitives::DataId(gid),
            tex_tiling: 2,
            max_vert_bright: 0,
            min_vert_bright: 0,
            max_vert_saturate: 0,
            min_vert_saturate: 0,
            max_vert_hue: 0,
            min_vert_hue: 0,
            detail_tex_tiling: 0,
            detail_tex_gid: dereth_primitives::DataId(0),
        };
        let mask = |gid: u32| CodeTexture {
            code: 8,
            tex_gid: dereth_primitives::DataId(gid),
        };
        let tm = TexMerge {
            base_tex_size: 16,
            corner_terrain_maps: vec![mask(0x0500_0100), mask(0x0500_0101)],
            side_terrain_maps: vec![CodeTexture {
                code: 9,
                tex_gid: dereth_primitives::DataId(0x0500_0200),
            }],
            road_maps: vec![mask(0x0500_0300)],
            terrain_desc: vec![
                desc(1, 0x0600_0001),
                desc(9, 0x0600_0009),
                desc(32, 0x0600_0020),
            ],
        };
        let image = crate::art_pack::images::with_levels(1, 1, vec![0; 4], false, None);
        // The retail ground holds the masks in another order than the land surface lists them.
        let retail = crate::art_pack::RetailGround {
            serial: 1,
            masks: [0x0500_0300, 0x0500_0200, 0x0500_0101, 0x0500_0100]
                .map(|id| (dereth_primitives::DataId(id), image.clone()))
                .to_vec(),
            pictures: Vec::new(),
            broad: Vec::new(),
        };
        // Grassland, with patchy grass at vertex (2, 3) and a road through vertex (6, 1).
        let mut words = [1u16 << 2; 81];
        words[2 * 9 + 3] = 9 << 2;
        words[6 * 9 + 1] |= 1;
        let layers = ground_layers(&tm, &retail, &words, u32::MAX, 8);
        for e in 0..8usize {
            for n in 0..8usize {
                let w = |a: usize, b: usize| words[a * 9 + b];
                let c = [w(e, n), w(e + 1, n), w(e + 1, n + 1), w(e, n + 1)];
                let key = MergeKey::new(c.map(|c| (c >> 2) & 0x1F), c.map(|c| c & 3), 1);
                let plan = ground_plan(&tm, key);
                let (base, overlays) = cell_layers(layers[e * 8 + n]).expect("laid");
                assert_eq!(base.map(u32::from), plan.base, "cell {e} {n}");
                assert_eq!(overlays.len(), plan.overlays.len(), "cell {e} {n}");
                for (o, p) in overlays.iter().zip(&plan.overlays) {
                    assert_eq!(o.ground.map(u32::from), p.ground);
                    assert_eq!(Some(o.mask), retail.mask(p.alpha));
                    assert_eq!(usize::from(o.rotation), p.rotation.index());
                }
            }
        }
        // The patchy grass is the north-east corner of cell (1, 2), the north-west of (2, 2),
        // the south-east of (1, 3) and the south-west of (2, 3); no other cell has an overlay
        // but the four round the road.
        let laid = |e: usize, n: usize| cell_layers(layers[e * 8 + n]).expect("laid").1;
        for (e, n) in [(1, 2), (2, 2), (1, 3), (2, 3)] {
            assert_eq!(laid(e, n)[0].ground, Some(9), "cell {e} {n}");
        }
        assert!(laid(4, 6).is_empty());
        assert_eq!(laid(6, 1)[0].ground, Some(32));
        assert_eq!(
            laid(6, 1)[0].mask,
            0,
            "the road's mask, first in the retail ground"
        );
        assert!(layers.iter().all(|w| cell_span(*w) == 1));
        // Drawn two cells a side, far off: each drawn cell's corners are every fourth word, and
        // its layers are laid over the sixteen cells it covers. The patchy grass and the road
        // fall between them: no drawn cell has an overlay.
        let coarse = ground_layers(&tm, &retail, &words, u32::MAX, 2);
        for (k, words) in coarse.iter().enumerate() {
            assert_eq!(cell_span(*words), 4, "cell {k}");
            let (base, overlays) = cell_layers(*words).expect("laid");
            assert_eq!(base, Some(1));
            assert!(overlays.is_empty(), "cell {k}");
        }
        // Patchy grass on a drawn corner shows in the drawn cells round it, over all their cells.
        words[4 * 9 + 4] = 9 << 2;
        let coarse = ground_layers(&tm, &retail, &words, u32::MAX, 2);
        for (e, n) in [(0, 0), (3, 3), (4, 4), (7, 7), (0, 7), (7, 0)] {
            let (_, overlays) = cell_layers(coarse[e * 8 + n]).expect("laid");
            assert_eq!(overlays[0].ground, Some(9), "cell {e} {n}");
        }
    }
}
