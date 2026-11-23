//! One frame: what it writes to the device before it is recorded ([`Renderer::prepare`]), and
//! its passes, recorded from the plan that leaves ([`Plan::record`]).

use dereth_gfx::vk;
use dereth_gfx::{
    Buffer, ColorAttachment, DepthAttachment, IndexType, Load, Pipeline, RasterPass, Recorder,
    Rect, State, Texture, Touch, Use,
};

use super::light::{
    air_drawn, dithered, drawn_linear, frame_rows, frame_rows_seen, free_turns, light_rows,
    pack_lit, sequence_turns, set_rows, sky_projection, swaying,
};
use super::scene::{Bucket, CULLED_BUCKETS, GROUP_BUCKETS, NONE};
use super::{push_words, Pipelines, Renderer, Targets, BUCKET_CAPACITY};
use crate::assets::MeshKey;

/// What a recorded frame needs, as plain values: nothing in it borrows the renderer.
#[derive(Debug, Clone)]
pub struct Plan {
    pipelines: Pipelines,
    /// The bytes one indirect draw command takes on the device.
    command_bytes: u32,
    targets: Targets,
    /// The world's rectangle on the frame's target.
    pub area: Rect,
    // Buffers' indices in the table.
    frame: u32,
    sky_frame: u32,
    items: u32,
    instances: u32,
    vertices: u32,
    materials: u32,
    lights: u32,
    sets: u32,
    offsets: u32,
    overrides: u32,
    wrap: u32,
    clamp: u32,
    /// What the drawn ground's pictures are read through, repeating and held.
    ground_samplers: [u32; 2],
    visible: u32,
    visible_count: u32,
    /// The first of the interior's visible items, when the viewer stands inside or looks in.
    split: u32,
    /// Whether the viewer stands inside with the outdoors drawn: the outdoors first, then the
    /// depth cleared, the openings stamped, the interior.
    inside: bool,
    /// Whether the viewer stands outdoors and looks into rooms through their openings: the
    /// outdoors first, then what it drew behind the openings cleared, the rooms.
    looking_in: bool,
    stamps: u32,
    stamp_corners: u32,
    commands: Buffer,
    counts: Buffer,
    commands_index: u32,
    counts_index: u32,
    /// Each bucket's commands, as the visible items were placed: drawn as they are where the
    /// device takes no count from a buffer.
    bucket_counts: [u32; CULLED_BUCKETS as usize],
    /// Whether the device takes an indirect draw's count from a buffer.
    indirect_count: bool,
    /// The buckets drawn as instanced draws of a mesh's items, where they are: the view's, and
    /// the water's mirrored view's.
    batched: Option<Batched>,
    batched_mirror: Option<Batched>,
    indices: Buffer,
    ground_vertices: u32,
    ground_cells: u32,
    ground_splats: u32,
    /// The materials' maps.
    material_maps: u32,
    /// The ground as a grid of patches, when it is drawn so.
    drawn_ground: Option<super::drawn::DrawnPlan>,
    ground_drawn: u32,
    /// Blocks of one side drawn together: the side, the first entry and how many.
    ground_groups: Vec<(u32, u32, u32)>,
    forward: Buffer,
    /// Runs of commands drawn with one pipeline: the pipeline, the first command, the count; for
    /// the sky, the cut-outs' edges and the forward pass, all in the one command buffer.
    sky_runs: Vec<(Pipeline, u32, u32)>,
    /// The sky's runs after the authored dome's, which the physical sky is drawn between.
    sky_runs_rest: Vec<(Pipeline, u32, u32)>,
    /// The sky's atmosphere, when it is drawn, and the fog's volume.
    atmosphere: Option<super::sky::SkyPlan>,
    fog: Option<super::sky::FogPlan>,
    edge_runs: Vec<(Pipeline, u32, u32)>,
    forward_runs: Vec<(Pipeline, u32, u32)>,
    edge_runs_inside: Vec<(Pipeline, u32, u32)>,
    forward_runs_inside: Vec<(Pipeline, u32, u32)>,
    /// The outdoors' edges and blended surfaces as the device sorted them, where it sorts them:
    /// drawn in place of their runs.
    lists: Option<super::instances::DeviceLists>,
    detail: [u32; 4],
    pixel: bool,
    /// What the per-pixel resolve's masks read, each by its index in the table and its target,
    /// `NONE` and `None` where its pass doesn't draw: how much of the sun reaches each pixel,
    /// and the lamps' light (rgb) with where it replaces the fixed-function lights (a).
    sun_visibility: (u32, Option<Texture>),
    lamp_light: (u32, Option<Texture>),
    /// The sun's shadows, when they are drawn.
    shadow: Option<super::shadow::ShadowPlan>,
    /// The lamps, when they light anything this frame.
    pub lamps: Option<super::lamps::LampsPlan>,
    /// The far trees' impostors, when the frame draws any.
    pub impostors: Option<super::impostor::ImpostorPlan>,
    /// The art pack's far trees, when the frame draws any.
    forest: Option<super::forest::ForestPlan>,
    /// The grass field, when it is drawn.
    grass: Option<super::grass::GrassPlan>,
    /// The blocks' parts chosen and listed on the device, when any is held.
    parts: Option<super::instances::PartsPlan>,
    /// The skinned bodies posed on the device, when there are any.
    skins: Option<super::skin_pass::SkinPlan>,
    /// Buffers outgrown, destroyed once the recording that copies them is begun.
    pub retired: Vec<Buffer>,
    /// The ambient occlusion pass's frame, when it runs.
    ao: Option<super::ao::AoPlan>,
    /// The post passes' frame, when they run.
    post: Option<super::post::PostPlan>,
    /// The upscaler's frame, when one stands in for the anti-aliasing.
    upscale: Option<super::upscale::UpscalePlan>,
    /// The water pass's frame, when it runs.
    water: Option<super::water::WaterPlan>,
    /// The sky all round the eye, drawn for the water.
    sky_probe: Option<super::sky_probe::SkyProbePlan>,
    /// The scene mirrored in the water, drawn for the water.
    planar: Option<super::planar::PlanarPlan>,
    /// The weather's frame, when any of it is drawn.
    weather: Option<super::weather::WeatherPlan>,
    /// The scene's acceleration structures, when the frame traces rays, and the structures of
    /// the meshes posed this frame, refitted once they are posed.
    pub rt: Option<super::rt::RtPlan>,
    rt_refitting: Vec<dereth_gfx::Blas>,
    /// The bounced light, when it is found.
    gi: Option<super::gi::GiPlan>,
    /// The scene's indices' index in the table.
    index_pool: u32,
    /// The sun's share and the occlusion traced through them, where they stand in for the
    /// cascades' look-up and the screen-space occlusion; and the scene's indices, vertices and
    /// materials and the sampler their alpha tests read with.
    rt_shadow: Option<super::rt_shadow::RtShadowPlan>,
    rt_ao: Option<super::rt_ao::RtAoPlan>,
    rt_pools: [u32; 4],
}

impl Renderer {
    /// Write the frame's state to the device for a world of `area`'s size, and the plan its
    /// passes are recorded from; `None` when no frame has been received.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn prepare(&mut self, area: Rect, gamma: f32) -> Result<Option<Plan>, dereth_gfx::Error> {
        let Some(mut frame) = self.frame.take() else {
            return Ok(None);
        };
        // Every pass lights the frame by the intent: retail's light with the look laid over it.
        frame.light = self.intents.of(&frame).light.clone();
        // With an upscaler on the world is drawn at the render size, which is otherwise the
        // display's, and the post passes bring the frame up to the display's.
        let display = (area.width, area.height);
        let render = self.upscale.settings().render_size(display);
        self.use_mip_bias(self.upscale.settings().mip_bias(display))?;
        self.post
            .set_upscaling(self.upscale.settings().runs().then_some(display));
        let (camera, dither) = match self.upscale.next_jitter(render, display) {
            Some(clip) => (
                self.post.jitter_by(&mut frame.camera, clip),
                self.upscale.dither().map(free_turns),
            ),
            None => (
                self.post.jitter(&mut frame.camera, render),
                self.post
                    .dither()
                    .map(|(place, count)| sequence_turns(place, count)),
            ),
        };
        let targets = self.targets_for(render.0, render.1)?;
        self.area = (
            u32::try_from(area.x).unwrap_or(0),
            u32::try_from(area.y).unwrap_or(0),
        );
        // A pack model's levels hand over at the distances the pack says, for the picture shown.
        let packs = &self.packs;
        self.lod
            .prepare(&frame, &self.tables, display.1, &|m| packs.distance(m));
        // The weather's clock and how much falls, before the sky's objects are chosen: the game's
        // own falling weather gives way to the remaster's.
        let rainy = self.intents.last().day == Some(crate::look::DayKind::Rainy);
        self.weather.observe(
            &frame,
            rainy,
            self.scene
                .ground
                .iter()
                .map(|(id, g)| (*id, g.words.as_slice(), g.full)),
        );
        let mut drawn = self.drawn(&frame);
        self.drawn_sizes = drawn.sizes();
        // The trees drawn as meshes, standing on the drawn ground: for the checks. The parts come
        // block by block, so each block is looked up once.
        let mut mesh_trees = Vec::new();
        let mut at: Option<(
            crate::scene::BlockId,
            Option<&super::scene::BlockGpu>,
            super::lod::BlockLod<'_>,
        )> = None;
        for (id, k, d, _) in &drawn.lod {
            if d.impostor >= 1.0 {
                continue;
            }
            if at.as_ref().map(|a| a.0) != Some(*id) {
                at = Some((*id, self.scene.blocks.get(id), self.lod.block(*id)));
            }
            let Some((_, Some(b), lod)) = at.as_ref() else {
                continue;
            };
            let Some(p) = b.parts.get(*k) else {
                continue;
            };
            if p.lifts != [0.0; 2] && p.degrade.is_some_and(|r| lod.is_tree(r)) {
                mesh_trees.push(p.instance);
            }
        }
        self.mesh_trees = mesh_trees;
        let rows = self.lod.rows(&self.scene, &drawn.lod, drawn.lod_tally);
        drawn.instances.extend(rows);
        // The post passes, when they run, draw the scene's colors into their own target, by
        // pipelines of their own.
        let post = self.post.prepare(
            &mut self.gfx,
            &frame,
            &camera,
            &targets,
            self.intents.last().look.exposure,
            &self.scene,
        )?;
        let upscale = match &post {
            Some(p) if self.upscale.settings().runs() => {
                let inputs = p.upscale_inputs();
                Some(
                    self.upscale
                        .prepare(&mut self.gfx, &camera, &targets, display, inputs)?,
                )
            }
            _ => {
                self.upscale.idle(&mut self.gfx);
                None
            }
        };
        let pipelines = post
            .as_ref()
            .map_or(self.pipelines, |p| p.scene_pipelines(self.pipelines));
        let mode = self.mode;
        let mut bytes = 0u64;
        let put = |b: &mut super::PerFrame,
                   gfx: &mut dereth_gfx::Device,
                   data: Vec<u8>|
         -> Result<u64, dereth_gfx::Error> {
            b.write(gfx, &data)?;
            Ok(data.len() as u64)
        };
        // With an art pack the air takes the fog's place on the opaque world, outdoors: the sky and
        // the air belong to the outdoors alone.
        let sky_on = self.atmosphere.settings;
        let air = (self.pack_look || self.landscape.air)
            && self.post.linear()
            && mode == super::light::LightMode::Pixel
            && sky_on.atmosphere
            && sky_on.fog
            && frame.landscape
            && !frame.sky.is_empty()
            && !drawn.inside;
        let rows = dithered(
            swaying(
                air_drawn(
                    pack_lit(
                        drawn_linear(frame_rows(&frame, gamma, mode, render), self.post.linear()),
                        self.pack_look || self.landscape.air,
                    ),
                    air,
                ),
                self.landscape.sway,
            ),
            dither,
        )
        .bytes();
        bytes += put(&mut self.constants, &mut self.gfx, rows.clone())?;
        let sky_rows = (!drawn.sky.is_empty()).then(|| {
            drawn_linear(
                frame_rows_seen(&frame, sky_projection(&frame.camera), gamma, mode, render),
                self.post.linear(),
            )
            .bytes()
        });
        if let Some(rows) = sky_rows.clone() {
            bytes += put(&mut self.sky_constants, &mut self.gfx, rows)?;
        }
        bytes += put(
            &mut self.lights,
            &mut self.gfx,
            light_rows(&frame.light).bytes(),
        )?;
        bytes += put(&mut self.sets, &mut self.gfx, set_rows(&drawn.sets).bytes())?;
        // Each block's offset from the frame's origin, by its slot.
        let slots = self
            .scene
            .blocks
            .values()
            .map(|b| b.slot)
            .filter(|s| *s != super::scene::NONE)
            .max()
            .map_or(0, |s| s + 1);
        let mut offsets = vec![0u8; slots as usize * 16];
        for (id, b) in &self.scene.blocks {
            if b.slot == super::scene::NONE {
                continue;
            }
            let (x, y) = (i32::from(id >> 8), i32::from(id & 0xFF));
            #[allow(clippy::cast_precision_loss)] // a few hundred blocks
            let o = [
                (x - frame.origin[0]) as f32 * 192.0,
                (y - frame.origin[1]) as f32 * 192.0,
                0.0,
                0.0,
            ];
            let at = b.slot as usize * 16;
            for (k, f) in o.iter().enumerate() {
                offsets[at + k * 4..at + k * 4 + 4].copy_from_slice(&f.to_le_bytes());
            }
        }
        bytes += put(&mut self.offsets, &mut self.gfx, offsets)?;
        let visible: Vec<u32> = drawn
            .opaque
            .iter()
            .chain(&drawn.opaque_inside)
            .copied()
            .collect();
        let split = drawn.opaque.len();
        // The processor's blocks' parts at their places in the walk, for the device to place its
        // own among them.
        let (cpu_walk, tails) = walk_counts(&drawn, &self.scene);
        if drawn.inside || drawn.looking_in {
            let corners: Vec<u8> = drawn
                .stamps
                .iter()
                .flat_map(|c| c.iter().flat_map(|f| f.to_le_bytes()))
                .collect();
            bytes += put(&mut self.stamps, &mut self.gfx, corners)?;
        }
        let over: Vec<u8> = drawn
            .overrides
            .iter()
            .flat_map(|r| r.iter().flat_map(|f| f.to_le_bytes()))
            .collect();
        bytes += put(&mut self.overrides, &mut self.gfx, over)?;
        // The ground's blocks, those of one side together.
        let mut ground = drawn.ground.clone();
        ground.sort_by_key(|g| g[2]);
        let mut ground_groups: Vec<(u32, u32, u32)> = Vec::new();
        for (k, g) in ground.iter().enumerate() {
            let k = u32::try_from(k).unwrap_or(0);
            match ground_groups.last_mut() {
                Some(last) if last.0 == g[2] => last.2 += 1,
                _ => ground_groups.push((g[2], k, 1)),
            }
        }
        let gb: Vec<u8> = ground.iter().flat_map(|g| push_words(g)).collect();
        bytes += put(&mut self.ground_drawn, &mut self.gfx, gb)?;
        for (i, row) in &drawn.instances {
            self.scene.write_instance(&mut self.gfx, *i, row)?;
            bytes += row.len() as u64;
        }
        // The sun's shadows, when the light is worked out per pixel.
        let shadow = if mode == super::light::LightMode::Pixel {
            let written: std::collections::BTreeSet<u32> =
                drawn.instances.iter().map(|(i, _)| *i).collect();
            self.shadow.prepare(
                &mut self.gfx,
                &mut self.scene,
                &frame,
                &camera,
                &rows,
                &written,
                render,
                &self.instances,
            )?
        } else {
            None
        };
        self.shadowed = shadow.is_some();
        // The ground as a grid of patches, chosen for the unjittered view.
        let window = self
            .scene
            .ground
            .keys()
            .map(|id| {
                (i32::from(id >> 8) - frame.origin[0])
                    .abs()
                    .max((i32::from(id & 0xFF) - frame.origin[1]).abs())
            })
            .max()
            .unwrap_or(0);
        #[allow(clippy::cast_precision_loss)] // a few dozen blocks
        let window_meters = (window as f32 + 0.5) * 192.0;
        self.last_drawn.clone_from(&frame.drawn);
        let cover = self.forest.cover_rows(&mut self.gfx, &frame)?;
        let drawn_ground = self.drawn_ground.prepare(
            &mut self.gfx,
            &self.scene,
            &frame,
            self.terrain,
            camera.projection * camera.view,
            window_meters,
            cover,
        )?;
        // The sky's atmosphere and the fog's volume, when the light is worked out per pixel.
        let (atmosphere, fog) = if mode == super::light::LightMode::Pixel {
            let low_fog = match (&drawn_ground, frame.drawn.as_ref()) {
                (Some(g), Some(d)) if self.landscape.air => Some(super::sky::LowFog {
                    base: super::sky::lowest_ground(&self.scene, &frame),
                    global: d.global,
                    heights: g.heights().0,
                    cascades: shadow.as_ref().map_or(NONE, |s| s.cascades().0),
                }),
                _ => None,
            };
            let ask = air.then(|| super::sky::AirAsk {
                low_fog,
                size: (targets.width, targets.height),
                clouds: super::sky::cloud_map(&frame)
                    .and_then(|m| Some((m, self.scene.sky_picture(m.gfx)?, self.sampler(0)))),
            });
            let atmosphere = self.atmosphere.prepare(
                &mut self.gfx,
                &frame,
                render,
                post.is_some(),
                self.post.linear(),
                ask,
            )?;
            let fog = if air {
                None
            } else {
                self.atmosphere.prepare_fog(
                    &mut self.gfx,
                    &frame,
                    shadow.as_ref().map(super::shadow::ShadowPlan::cascades),
                )?
            };
            (atmosphere, fog)
        } else {
            (None, None)
        };
        if shadow.is_some() {
            bytes += self.shadow.stats.last_bytes;
        }
        self.view_projection = camera.projection * camera.view;
        self.eye = camera.eye;
        let first = self.scene.particle_items.start;
        bytes += self
            .scene
            .write_frame_items(&mut self.gfx, first, &drawn.particle_items)?;
        // The keys the rooms' walls carry, by their cells' ids, for the lamps that keep to rooms.
        let room_keys: std::collections::BTreeMap<u32, u32> = self
            .scene
            .blocks
            .values()
            .flat_map(|b| b.rooms.iter().map(|r| (r.id, r.key)))
            .collect();
        let lamps = self.lamps.prepare(
            &mut self.gfx,
            &super::lamps::LampInputs {
                frame: &frame,
                room_keys: &room_keys,
                targets: &targets,
                linear: post.is_some(),
            },
        )?;
        // The weather: its constants, and the overhead map where it moved.
        let weather = self.weather.prepare(
            &mut self.gfx,
            &self.scene,
            &frame,
            &camera,
            &rows,
            self.water.settings().puddles && rainy,
            post.is_some(),
            render.1,
            self.landscape.grass || self.landscape.flowers,
        )?;
        // The scene's acceleration structures, kept while anything traces rays; and the passes
        // that trace them where they stand in for the cascades' look-up and the occlusion.
        let ao = self
            .ao
            .prepare(&mut self.gfx, &camera, self.post.drawn_jitter(), &targets)?;
        let wanted = self.rt_kept
            || (self.rt_shadow.wants_rays() && shadow.is_some())
            || (self.rt_ao.wants_rays() && ao.is_some())
            || self.gi.wants_rays()
            || self.water.wants_rays();
        let posed = self
            .skins
            .meshes()
            .into_keys()
            .filter_map(|id| self.scene.meshes.get(&MeshKey::GfxObj(id)))
            .map(|m| m.vertices.start)
            .collect();
        self.rt.set_posed(posed);
        let packs = &self.packs;
        let rt = self.rt.prepare(&mut self.gfx, &self.scene, wanted, &|m| {
            packs.cover_reach(m)
        })?;
        let rt_shadow = self
            .rt_shadow
            .prepare(frame.light.sun, shadow.is_some(), rt.is_some());
        let ao_settings = ao.is_some().then(|| self.ao.settings());
        let rt_ao = self
            .rt_ao
            .prepare(&mut self.gfx, &targets, ao_settings, rt.is_some())?;
        let rt_pools = [
            self.scene.indices.index,
            self.scene.vertices.index,
            self.scene.materials.index,
            self.sampler(0),
        ];
        // The sky all round the eye, for the water to reflect.
        let sky_probe = match &sky_rows {
            Some(rows) if self.water.settings().runs() && frame.landscape && !drawn.inside => {
                let format = if post.is_some() {
                    super::post::scene_format(&self.gfx)
                } else {
                    super::COLOR
                };
                Some(self.sky_probe.prepare(
                    &mut self.gfx,
                    format,
                    &frame.camera,
                    frame.camera.far * super::light::SKY_FAR,
                    rows,
                    atmosphere.as_ref().map(|_| self.atmosphere.rows()),
                )?)
            }
            _ => None,
        };
        // The scene mirrored in the water nearest the eye, where the reflections are not traced.
        let water_traced = self.water.wants_rays() && rt.is_some();
        let planar = match &sky_rows {
            Some(sky)
                if self.water.settings().on
                    && self.water.settings().planar
                    && !water_traced
                    && frame.landscape
                    && !drawn.inside =>
            {
                let format = if post.is_some() {
                    super::post::scene_format(&self.gfx)
                } else {
                    super::COLOR
                };
                self.planar.prepare(
                    &mut self.gfx,
                    &frame,
                    (targets.width, targets.height),
                    format,
                    &rows,
                    sky,
                    sky_projection(&frame.camera),
                    atmosphere.as_ref().map(|_| self.atmosphere.rows()),
                )?
            }
            _ => None,
        };
        // The blocks' parts chosen and listed on the device, after the processor's own items,
        // with the processor's edges and blended surfaces to sort among theirs.
        let mirror = planar.as_ref().map(|p| p.view_projection);
        let packs = &self.packs;
        let parts = self.instances.prepare(
            &mut self.gfx,
            &self.scene,
            &self.lod,
            &|m| packs.distance(m),
            &super::instances::PartsAsk {
                frame: &frame,
                height: display.1,
                clip: camera.projection * camera.view,
                mirror,
                cascades: shadow.as_ref().map(|s| s.parts),
                targets: super::instances::PartsTargets {
                    offsets: (self.offsets.buffer, self.offsets.index),
                    instances: (self.scene.instances.buffer, self.scene.instances.index),
                    items: (self.scene.items.buffer, self.scene.items.index),
                    commands: (self.commands, self.commands_index),
                    counts: (self.counts, self.counts_index),
                    capacity: BUCKET_CAPACITY,
                    trees: self.impostors.trees(),
                    frame: self.constants.index,
                },
                cpu: super::instances::CpuLists {
                    walk: &cpu_walk,
                    tails,
                    edges: &drawn.edges,
                    forward: if self.instances.sorts_forward() {
                        &drawn.forward
                    } else {
                        &[]
                    },
                },
            },
        )?;
        let lists = parts.as_ref().map(super::instances::PartsPlan::lists);
        // Each visible item with its place in its bucket's commands, in the order the walk drew
        // them, so the buckets draw in the same order every frame: surfaces at equal depth then
        // resolve the same way frame after frame. With the device's parts, an outdoor item of a
        // block's part is placed among its part's, the rest after every block's.
        let (placed, next) = placed_items(
            &visible,
            (split, &drawn.opaque_walk),
            parts.is_some(),
            &self.scene,
        );
        bytes += put(&mut self.visible, &mut self.gfx, push_words(&placed))?;
        // Where draws take no count from a buffer, the buckets as instanced draws of each mesh's
        // items, the view's and the mirrored view's, each without what it cannot see.
        let (batched, batched_mirror) = if !self.gfx.caps().indirect_count
            && parts.is_none()
            && super::command_bytes(&self.gfx) == batch_command_bytes()
        {
            let spheres = super::batch::part_spheres(
                self.scene.item_draws.len(),
                self.scene.blocks.iter().map(|(id, b)| {
                    (
                        (
                            i32::from(id >> 8) - frame.origin[0],
                            i32::from(id & 0xFF) - frame.origin[1],
                        ),
                        b.parts.as_slice(),
                    )
                }),
            );
            let lists = |clip| {
                super::batch::batch(
                    &visible,
                    split,
                    &self.scene.buckets,
                    &self.scene.item_draws,
                    &spheres,
                    &super::shadow::frustum(clip),
                )
            };
            let seen = lists(camera.projection * camera.view);
            let runs = seen.runs;
            let (both, mirror_runs) = match mirror {
                Some(m) => {
                    let (both, r) = seen.then(&lists(m));
                    (both, Some(r))
                }
                None => (seen, None),
            };
            bytes += put(&mut self.batched, &mut self.gfx, push_words(&both.commands))?;
            bytes += put(&mut self.remap, &mut self.gfx, push_words(&both.remap))?;
            let made = |runs| Batched {
                commands: self.batched.buffer,
                remap: self.remap.index,
                runs,
            };
            (Some(made(runs)), mirror_runs.map(made))
        } else {
            (None, None)
        };
        // The sky's, the edges' and the forward pass's commands, in their orders, in runs of one
        // pipeline; the outdoors' edges and blended surfaces where the device does not sort them.
        let backend = self.gfx.backend();
        let mut fwd = Vec::new();
        let mut command = 0u32;
        let mut add = |runs: &mut Vec<(Pipeline, u32, u32)>,
                       item: u32,
                       p: Pipeline,
                       draws: &[super::scene::ItemDraw]| {
            let Some(d) = draws.get(item as usize) else {
                return;
            };
            super::command_into(backend, d.index_count, d.first_index, item, &mut fwd);
            match runs.last_mut() {
                Some(last) if last.0 == p && last.1 + last.2 == command => last.2 += 1,
                _ => runs.push((p, command, 1)),
            }
            command += 1;
        };
        let draws = &self.scene.item_draws;
        let mut sky_runs = Vec::new();
        let mut sky_runs_rest = Vec::new();
        for (k, (item, bucket)) in drawn.sky.iter().enumerate() {
            let two = draws.get(*item as usize).is_some_and(|d| d.two_sided);
            let b = match bucket {
                Bucket::Blended => 1,
                Bucket::Additive => 2,
                _ => 0,
            };
            add(
                if k < drawn.sky_dome_items {
                    &mut sky_runs
                } else {
                    &mut sky_runs_rest
                },
                *item,
                pipelines.sky[b][usize::from(two)],
                draws,
            );
        }
        let none_edges: Vec<(f32, u32)> = Vec::new();
        let none_forward: Vec<(f32, u32, Bucket)> = Vec::new();
        let outdoor_edges = if lists.is_some_and(|l| l.edges) {
            &none_edges
        } else {
            &drawn.edges
        };
        let outdoor_forward = if lists.is_some_and(|l| l.forward) {
            &none_forward
        } else {
            &drawn.forward
        };
        self.blended = [
            outdoor_edges.iter().map(|e| e.1).collect(),
            outdoor_forward.iter().map(|f| f.1).collect(),
        ];
        self.blended_sorted = lists.map_or([false; 2], |l| [l.edges, l.forward]);
        let mut edge_runs = [Vec::new(), Vec::new()];
        for (runs, list) in edge_runs
            .iter_mut()
            .zip([outdoor_edges, &drawn.edges_inside])
        {
            for (_, item) in list {
                let two = draws.get(*item as usize).is_some_and(|d| d.two_sided);
                add(runs, *item, pipelines.edge[usize::from(two)], draws);
            }
        }
        let mut forward_runs = [Vec::new(), Vec::new()];
        for (runs, list) in forward_runs
            .iter_mut()
            .zip([outdoor_forward, &drawn.forward_inside])
        {
            for (_, item, bucket) in list {
                let two = draws.get(*item as usize).is_some_and(|d| d.two_sided);
                let b = usize::from(*bucket == Bucket::Additive);
                add(runs, *item, pipelines.forward[b][usize::from(two)], draws);
            }
        }
        let [edge_runs, edge_runs_inside] = edge_runs;
        let [forward_runs, forward_runs_inside] = forward_runs;
        bytes += put(&mut self.forward, &mut self.gfx, fwd)?;
        self.stats.frames += 1;
        self.stats.frame_bytes += bytes;
        self.stats.last_items = u32::try_from(visible.len()).unwrap_or(u32::MAX);
        self.stats.last_particle_items =
            u32::try_from(drawn.particle_items.len()).unwrap_or(u32::MAX);
        self.stats.last_forward =
            u32::try_from(drawn.forward.len() + drawn.forward_inside.len()).unwrap_or(u32::MAX);
        let water_sampler = self.sampler(0);
        let probe_sampler = self.sampler(1);
        let water = self.water.prepare(
            &mut self.gfx,
            &frame,
            &targets,
            (post.is_some(), rainy),
            &super::water::WaterInputs {
                rt: rt.as_ref(),
                pools: [
                    self.scene.indices.index,
                    self.scene.vertices.index,
                    self.scene.materials.index,
                    water_sampler,
                    self.scene.instances.index,
                    self.scene.ground_vertices.index,
                    self.scene.ground_cells.index,
                ],
                sky: atmosphere.as_ref().map(super::sky::SkyPlan::view_inputs),
                probe: sky_probe.map(|p| (p.faces, probe_sampler)),
                planar: planar.map(|p| {
                    (
                        p.targets.color,
                        p.targets.index[5],
                        p.level,
                        p.view_projection,
                    )
                }),
                planar_sampler: probe_sampler,
                heights: drawn_ground.as_ref().map(super::drawn::DrawnPlan::heights),
                shore: drawn_ground.as_ref().map(super::drawn::DrawnPlan::shore),
            },
        )?;
        self.stats.last_water = water.is_some();
        let samplers = [self.sampler(0), self.sampler(1)];
        let impostors = self.impostors.prepare(
            &mut self.gfx,
            &super::impostor::ImpostorInputs {
                frame: &frame,
                scene: &self.scene,
                draws: &drawn.impostors,
                constants: self.constants.index,
                samplers,
                weather: weather
                    .as_ref()
                    .map_or((NONE, None), |w| (w.table, Some(w.cover()))),
                device: parts.as_ref().map(super::instances::PartsPlan::cards),
            },
        )?;
        let forest = self.forest.prepare(
            &mut self.gfx,
            &super::forest::ForestInputs {
                frame: &frame,
                scene: &self.scene,
                clip: camera.projection * camera.view,
                eye: camera.eye,
                pixels: super::forest::pixels_a_meter(camera.projection, targets.height),
                constants: self.constants.index,
                clamp: samplers[1],
                weather: weather
                    .as_ref()
                    .map_or((NONE, None), |w| (w.table, Some(w.cover()))),
                heights: self.drawn_ground.heights().unwrap_or(NONE),
                tables: &self.tables,
                impostors: &self.impostors,
                wrap: samplers[0],
            },
        )?;
        let l = self.landscape;
        let grass = if (l.grass || l.flowers) && drawn_ground.is_some() {
            self.grass.prepare(
                &mut self.gfx,
                &frame,
                (camera.projection * camera.view, camera.eye),
                self.constants.index,
                weather
                    .as_ref()
                    .map_or((NONE, None), |w| (w.table, Some(w.cover()))),
                super::grass::FieldAsk {
                    blades: l.grass,
                    scatter: l.flowers,
                },
            )?
        } else {
            None
        };
        let gi = self
            .gi
            .prepare(&mut self.gfx, &frame, &self.scene, &targets, rt)?;
        Ok(Some(Plan {
            pipelines,
            targets: post.as_ref().map_or(targets, |p| p.scene_targets(targets)),
            area,
            frame: self.constants.index,
            sky_frame: self.sky_constants.index,
            items: self.scene.items.index,
            instances: self.scene.instances.index,
            vertices: self.scene.vertices.index,
            materials: self.scene.materials.index,
            lights: self.lights.index,
            sets: self.sets.index,
            offsets: self.offsets.index,
            overrides: self.overrides.index,
            wrap: self.sampler(0),
            clamp: self.sampler(1),
            ground_samplers: self.ground_samplers,
            visible: self.visible.index,
            visible_count: u32::try_from(visible.len()).unwrap_or(u32::MAX),
            split: u32::try_from(drawn.opaque.len()).unwrap_or(u32::MAX),
            inside: drawn.inside,
            looking_in: drawn.looking_in,
            stamps: self.stamps.index,
            stamp_corners: u32::try_from(drawn.stamps.len()).unwrap_or(0),
            commands: self.commands,
            counts: self.counts,
            commands_index: self.commands_index,
            counts_index: self.counts_index,
            bucket_counts: next.map(|n| n.min(BUCKET_CAPACITY)),
            indirect_count: self.gfx.caps().indirect_count,
            batched,
            batched_mirror,
            indices: self.scene.indices.buffer,
            ground_vertices: self.scene.ground_vertices.index,
            ground_cells: self.scene.ground_cells.index,
            ground_splats: self.scene.splats.index,
            material_maps: self.scene.material_maps.index,
            drawn_ground,
            ground_drawn: self.ground_drawn.index,
            ground_groups,
            forward: self.forward.buffer,
            command_bytes: super::command_bytes(&self.gfx),
            sky_runs,
            sky_runs_rest,
            atmosphere,
            fog,
            edge_runs,
            forward_runs,
            edge_runs_inside,
            forward_runs_inside,
            lists,
            detail: self.detail_push(),
            pixel: mode == super::light::LightMode::Pixel,
            sun_visibility: shadow
                .as_ref()
                .map_or((NONE, None), |s| (s.visibility_index, Some(s.visibility()))),
            shadow,
            lamp_light: lamps
                .as_ref()
                .map_or((NONE, None), |l| (l.light_index, Some(l.light))),
            lamps,
            impostors,
            forest,
            grass,
            parts,
            skins: self
                .skin_pass
                .plan((self.scene.vertices.buffer, self.scene.vertices.index)),
            rt_refitting: if rt.is_some() {
                self.rt.refitting().to_vec()
            } else {
                Vec::new()
            },
            rt,
            gi,
            index_pool: self.scene.indices.index,
            rt_shadow,
            rt_ao,
            rt_pools,
            retired: {
                let mut r = self.scene.take_retired();
                r.append(&mut self.skin_pass.retired);
                r
            },
            ao,
            water,
            sky_probe,
            planar,
            post,
            upscale,
            weather,
        }))
    }
}

impl Plan {
    /// Draw the G-buffer's culled buckets from `first`, one group's: front faces culled, then
    /// both drawn.
    fn draw_buckets(
        &self,
        d: &mut dereth_gfx::Draw<'_>,
        first: u32,
    ) -> Result<(), dereth_gfx::Error> {
        self.draw_buckets_seen(d, first, self.frame, self.batched.as_ref())
    }

    /// Looking into rooms from outdoors, after the outdoors' G-buffer: inside each opening, what
    /// the outdoors drew behind it cleared in a copy of the depth and what stands in front of it
    /// kept; the rooms' opaque surfaces drawn into the copy; and the frame's depth then the
    /// rooms' wherever they drew, the outdoors' own wherever they did not.
    ///
    /// Retail clears the depth inside each opening it looks through before it draws the rooms
    /// beyond, so the ground a building stands on, inside its walls, never covers its rooms: a
    /// cellar or a cave's stairs under the entrance's floor are seen through the doorway.
    ///
    /// # Errors
    /// What the layer refuses.
    fn record_looking_in(
        &self,
        r: &mut Recorder<'_>,
        t: &Targets,
        full: Rect,
        cover: Option<Texture>,
    ) -> Result<(), dereth_gfx::Error> {
        r.copy_texture(t.depth, t.outdoor_depth)?;
        r.timestamp("openings from outdoors");
        r.raster(
            &RasterPass {
                name: "openings from outdoors",
                color: &[],
                depth: Some(DepthAttachment {
                    texture: t.outdoor_depth,
                    clear: None,
                    write: true,
                }),
                touches: &[Touch::Texture(t.depth, Use::Sampled)],
                area: Some(full),
            },
            |d| {
                if self.stamp_corners > 0 {
                    d.set_pipeline(self.pipelines.stamp_behind)?;
                    d.push(0, &push_words(&[self.frame, self.stamps, t.index[4]]))?;
                    d.draw(self.stamp_corners, 1, 0, 0);
                }
                Ok(())
            },
        )?;
        r.timestamp("g-buffer looked into");
        let keep = [t.albedo, t.normal, t.light, t.material].map(|texture| ColorAttachment {
            texture,
            load: Load::Keep,
        });
        let mut touches = vec![
            Touch::Buffer(self.commands, Use::Indirect),
            Touch::Buffer(self.counts, Use::Indirect),
            Touch::Buffer(self.indices, Use::Index),
        ];
        if let Some(b) = &self.batched {
            touches.push(Touch::Buffer(b.commands, Use::Indirect));
        }
        if let Some(c) = cover {
            touches.push(Touch::Texture(c, Use::Sampled));
        }
        r.raster(
            &RasterPass {
                name: "g-buffer looked into",
                color: &keep,
                depth: Some(DepthAttachment {
                    texture: t.outdoor_depth,
                    clear: None,
                    write: true,
                }),
                touches: &touches,
                area: Some(full),
            },
            |d| self.draw_buckets(d, GROUP_BUCKETS),
        )?;
        r.raster(
            &RasterPass {
                name: "depth looked into",
                color: &[],
                depth: Some(DepthAttachment {
                    texture: t.depth,
                    clear: None,
                    write: true,
                }),
                touches: &[Touch::Texture(t.outdoor_depth, Use::Sampled)],
                area: Some(full),
            },
            |d| {
                d.set_pipeline(self.pipelines.stamp_rooms)?;
                d.push(0, &push_words(&[self.frame, self.stamps, t.index[6]]))?;
                d.draw(3, 1, 0, 0);
                Ok(())
            },
        )
    }

    /// The mirrored view's instanced draws, or the view's where it has none of its own: where
    /// the buckets are drawn instanced, no command an item is written.
    fn mirror_batch(&self) -> Option<&Batched> {
        self.batched_mirror.as_ref().or(self.batched.as_ref())
    }

    /// The mirrored view's meshes: the outdoors' buckets, and the rooms' looked into from
    /// outdoors, which the mirror draws with the rest.
    fn draw_mirrored_buckets(
        &self,
        d: &mut dereth_gfx::Draw<'_>,
        frame: u32,
    ) -> Result<(), dereth_gfx::Error> {
        self.draw_buckets_seen(d, 0, frame, self.mirror_batch())?;
        if self.looking_in {
            self.draw_buckets_seen(d, GROUP_BUCKETS, frame, self.mirror_batch())?;
        }
        Ok(())
    }

    /// [`Self::draw_buckets`] with the frame's constants from the buffer `frame`, as `batched`
    /// draws them where it is given.
    fn draw_buckets_seen(
        &self,
        d: &mut dereth_gfx::Draw<'_>,
        first: u32,
        frame: u32,
        batched: Option<&Batched>,
    ) -> Result<(), dereth_gfx::Error> {
        d.bind_index_buffer(self.indices, 0, IndexType::U32)?;
        if let Some(batched) = batched {
            for b in first..(first + GROUP_BUCKETS).min(CULLED_BUCKETS) {
                let (at, n) = batched.runs[b as usize];
                if n == 0 {
                    continue;
                }
                d.set_pipeline(self.pipelines.gbuffer[(b - first) as usize])?;
                d.push(
                    0,
                    &self.gbuffer_push_remapped(frame, 0, NONE, batched.remap),
                )?;
                d.draw_indexed_indirect(
                    batched.commands,
                    u64::from(at) * u64::from(self.command_bytes),
                    n,
                    self.command_bytes,
                )?;
            }
            return Ok(());
        }
        for b in first..(first + GROUP_BUCKETS).min(CULLED_BUCKETS) {
            d.set_pipeline(self.pipelines.gbuffer[(b - first) as usize])?;
            d.push(0, &self.gbuffer_push_seen(frame, 0, NONE))?;
            let offset = u64::from(b * BUCKET_CAPACITY) * u64::from(self.command_bytes);
            if self.indirect_count {
                d.draw_indexed_indirect_count(
                    self.commands,
                    offset,
                    self.counts,
                    u64::from(b) * 4,
                    BUCKET_CAPACITY,
                    self.command_bytes,
                )?;
            } else {
                let n = self.bucket_counts[b as usize];
                if n > 0 {
                    d.draw_indexed_indirect(self.commands, offset, n, self.command_bytes)?;
                }
            }
        }
        Ok(())
    }

    /// The scene mirrored in the water: its G-buffer (the ground, then the meshes the frame
    /// draws), its sky, and the two resolved by retail's light alone, into its own targets.
    ///
    /// # Errors
    /// What the layer refuses.
    fn record_planar(
        &self,
        r: &mut Recorder<'_>,
        p: &super::planar::PlanarPlan,
        gbuffer_touches: &[Touch],
    ) -> Result<(), dereth_gfx::Error> {
        let t = p.targets;
        let full = Rect {
            x: 0,
            y: 0,
            width: t.width,
            height: t.height,
        };
        r.timestamp("water mirrored");
        let gbuffer = [t.albedo, t.normal, t.light, t.material].map(|texture| ColorAttachment {
            texture,
            load: Load::Clear([0.0; 4]),
        });
        r.raster(
            &RasterPass {
                name: "water mirrored g-buffer",
                color: &gbuffer,
                depth: Some(DepthAttachment {
                    texture: t.depth,
                    clear: Some(1.0),
                    write: true,
                }),
                touches: gbuffer_touches,
                area: Some(full),
            },
            |d| {
                if let Some(g) = &self.drawn_ground {
                    let [wrap, clamp] = self.ground_samplers;
                    g.draw_ground(d, p.frame, wrap, clamp, self.weather_table())?;
                    return self.draw_mirrored_buckets(d, p.frame);
                }
                d.set_pipeline(self.pipelines.ground)?;
                for (side, first, count) in &self.ground_groups {
                    d.push(
                        0,
                        &push_words(&[
                            p.frame,
                            self.ground_vertices,
                            self.ground_cells,
                            self.ground_drawn,
                            self.offsets,
                            self.clamp,
                            *first,
                            self.ground_splats,
                            self.wrap,
                            self.weather_table(),
                        ]),
                    )?;
                    d.draw(side * side * 6, *count, 0, 0);
                }
                self.draw_mirrored_buckets(d, p.frame)
            },
        )?;
        let mut sky_touches = vec![
            Touch::Buffer(self.forward, Use::Indirect),
            Touch::Buffer(self.indices, Use::Index),
        ];
        if let Some(a) = &self.atmosphere {
            sky_touches.push(Touch::Texture(a.view_table(), Use::Sampled));
        }
        r.raster(
            &RasterPass {
                name: "water mirrored sky",
                color: &[ColorAttachment {
                    texture: t.color,
                    load: Load::Clear([0.0, 0.0, 0.0, 1.0]),
                }],
                depth: None,
                touches: &sky_touches,
                area: Some(full),
            },
            |d| {
                self.draw_runs(d, &self.sky_runs, p.sky, NONE)?;
                if let (Some(a), Some(seen)) = (&self.atmosphere, p.atmosphere) {
                    a.draw_seen(d, seen, false)?;
                }
                self.draw_runs(d, &self.sky_runs_rest, p.sky, NONE)?;
                if let (Some(a), Some(seen)) = (&self.atmosphere, p.atmosphere) {
                    a.draw_seen(d, seen, true)?;
                }
                Ok(())
            },
        )?;
        r.raster(
            &RasterPass {
                name: "water mirrored resolve",
                color: &[ColorAttachment {
                    texture: t.color,
                    load: Load::Keep,
                }],
                depth: None,
                touches: &[
                    Touch::Texture(t.albedo, Use::Sampled),
                    Touch::Texture(t.normal, Use::Sampled),
                    Touch::Texture(t.light, Use::Sampled),
                    Touch::Texture(t.material, Use::Sampled),
                    Touch::Texture(t.depth, Use::Sampled),
                ],
                area: Some(full),
            },
            |d| {
                d.set_pipeline(if self.pixel {
                    self.pipelines.resolve_pixel
                } else {
                    self.pipelines.resolve_vertex
                })?;
                d.push(
                    0,
                    &push_words(&[
                        p.frame,
                        t.index[0],
                        t.index[1],
                        t.index[2],
                        t.index[3],
                        t.index[4],
                        self.lights,
                        self.sets,
                        0,
                        t.index[4],
                        NONE,
                        NONE,
                        NONE,
                        NONE,
                        NONE,
                        NONE,
                        NONE,
                    ]),
                )?;
                d.draw(3, 1, 0, 0);
                Ok(())
            },
        )
    }

    /// The cut-outs' edges `edges`, then the forward pass's `forward`, over the world's colors,
    /// tested against `depth`, leaving alone the interior's pixels of the material target `mask`
    /// (`NONE`: none).
    fn blend_passes(
        &self,
        r: &mut Recorder<'_>,
        full: Rect,
        depth: Texture,
        mask: u32,
        edges: BlendList<'_>,
        forward: BlendList<'_>,
    ) -> Result<(), dereth_gfx::Error> {
        let t = self.targets;
        for (name, list, write) in [("edges", edges, true), ("forward", forward, false)] {
            r.timestamp(name);
            let mut touches = vec![
                Touch::Buffer(self.forward, Use::Indirect),
                Touch::Buffer(self.indices, Use::Index),
            ];
            if mask != NONE {
                touches.push(Touch::Texture(t.material, Use::Sampled));
            }
            match (list, self.lists) {
                (BlendList::Runs([]), _) => continue,
                (BlendList::Runs(_), _) => {}
                (BlendList::Sorted(_), Some(l)) => {
                    touches.push(Touch::Buffer(l.commands, Use::Indirect));
                    touches.push(Touch::Buffer(l.counts, Use::Indirect));
                }
                (BlendList::Sorted(_), None) => continue,
            }
            r.raster(
                &RasterPass {
                    name,
                    color: &[ColorAttachment {
                        texture: t.color,
                        load: Load::Keep,
                    }],
                    depth: Some(DepthAttachment {
                        texture: depth,
                        clear: None,
                        write,
                    }),
                    touches: &touches,
                    area: Some(full),
                },
                |d| match list {
                    BlendList::Runs(runs) => self.draw_runs(d, runs, self.frame, mask),
                    BlendList::Sorted(l) => self.draw_sorted(d, l, mask),
                },
            )?;
        }
        Ok(())
    }

    /// The outdoors' list `list` as the device sorted it (0 the cut-outs' edges, 1 the blended
    /// surfaces), by its one pipeline and one counted indirect draw, leaving alone the interior's
    /// pixels of the material target `mask` (`NONE`: none).
    fn draw_sorted(
        &self,
        d: &mut dereth_gfx::Draw<'_>,
        list: u32,
        mask: u32,
    ) -> Result<(), dereth_gfx::Error> {
        let Some(l) = self.lists else {
            return Ok(());
        };
        let pipeline = match list {
            0 => self.pipelines.edge_any,
            _ => match self.pipelines.forward_any {
                Some(p) => p,
                None => return Ok(()),
            },
        };
        d.bind_index_buffer(self.indices, 0, IndexType::U32)?;
        d.set_pipeline(pipeline)?;
        d.push(0, &self.gbuffer_push_seen(self.frame, 1, mask))?;
        d.draw_indexed_indirect_count(
            l.commands,
            u64::from(list) * u64::from(l.capacity) * u64::from(l.command_bytes),
            l.counts,
            u64::from(list) * 4,
            l.capacity,
            l.command_bytes,
        )
    }

    /// The weather's constants' index in the table, `NONE` without them.
    fn weather_table(&self) -> u32 {
        self.weather.as_ref().map_or(NONE, |w| w.table)
    }

    /// The G-buffer pass's push words, with the frame's constants from the buffer `frame`, and the
    /// pixels whose material in the target `mask` is the interior's left alone (`NONE`: none).
    fn gbuffer_push_seen(&self, frame: u32, lit_always: u32, mask: u32) -> Vec<u8> {
        self.gbuffer_push_remapped(frame, lit_always, mask, NONE)
    }

    /// [`Self::gbuffer_push_seen`], each instance's item read from `remap` (`NONE`: the draw's
    /// first instance is its item).
    fn gbuffer_push_remapped(&self, frame: u32, lit_always: u32, mask: u32, remap: u32) -> Vec<u8> {
        push_words(&[
            frame,
            self.items,
            self.instances,
            self.vertices,
            self.materials,
            self.lights,
            self.sets,
            self.offsets,
            self.overrides,
            self.wrap,
            self.clamp,
            0,
            lit_always,
            self.detail[0],
            self.detail[1],
            self.detail[2],
            self.detail[3],
            mask,
            self.material_maps,
            self.weather_table(),
            remap,
        ])
    }

    /// The outdoors' edges and blended surfaces: the lists the device sorted, where it sorts them,
    /// else the processor's runs.
    fn outdoor_lists(&self) -> (BlendList<'_>, BlendList<'_>) {
        let sorted = |on: bool, list: u32, runs| {
            if self.lists.is_some() && on {
                BlendList::Sorted(list)
            } else {
                BlendList::Runs(runs)
            }
        };
        let (edges, forward) = self.lists.map_or((false, false), |l| (l.edges, l.forward));
        (
            sorted(edges, 0, &self.edge_runs),
            sorted(forward, 1, &self.forward_runs),
        )
    }

    /// Draw `runs` of commands from the command buffer, lit per vertex, seen through the frame's
    /// constants in the buffer `frame`, leaving alone the interior's pixels of the material
    /// target `mask` (`NONE`: none).
    fn draw_runs(
        &self,
        d: &mut dereth_gfx::Draw<'_>,
        runs: &[(Pipeline, u32, u32)],
        frame: u32,
        mask: u32,
    ) -> Result<(), dereth_gfx::Error> {
        d.bind_index_buffer(self.indices, 0, IndexType::U32)?;
        for (p, first, count) in runs {
            d.set_pipeline(*p)?;
            d.push(0, &self.gbuffer_push_seen(frame, 1, mask))?;
            d.draw_indexed_indirect(
                self.forward,
                u64::from(*first) * u64::from(self.command_bytes),
                *count,
                self.command_bytes,
            )?;
        }
        Ok(())
    }

    /// Record the frame's passes into `r`, drawing the world into `color` and `depth`, the
    /// frame's own images, within [`Self::area`], and hand them back as `color_back` and
    /// `depth_back` say.
    ///
    /// # Errors
    /// What the layer refuses.
    pub fn record(
        &self,
        r: &mut Recorder<'_>,
        color: Texture,
        depth: Texture,
        color_back: State,
        depth_back: State,
    ) -> Result<(), dereth_gfx::Error> {
        let scene = self.record_scene(r)?;
        self.record_finish(r, scene, color, depth, color_back, depth_back)
    }

    /// The frame's upscaler, while one draws it.
    #[cfg(target_os = "macos")]
    pub(super) fn upscale_plan(&self) -> Option<&super::upscale::UpscalePlan> {
        self.upscale.as_ref()
    }

    /// Whether the frame's passes go in two recordings, around Metal's scaler.
    #[must_use]
    pub fn splits(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.upscale
                .as_ref()
                .is_some_and(super::upscale::UpscalePlan::scales_outside)
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }

    /// Record the frame's passes up to the frames gathered, what a scaler outside the layer reads.
    ///
    /// # Errors
    /// What the layer refuses.
    pub fn record_scene(&self, r: &mut Recorder<'_>) -> Result<SceneRecorded, dereth_gfx::Error> {
        let t = self.targets;
        let full = Rect {
            x: 0,
            y: 0,
            width: t.width,
            height: t.height,
        };

        // 0. The skinned bodies posed, before anything reads their vertices.
        if let Some(s) = &self.skins {
            s.record(r)?;
        }

        // 1. The cull: the device's parts chosen first, which place the processor's items in
        // the outdoor buckets, then the processor's items, then the device's parts' commands.
        r.timestamp("cull");
        r.fill_buffer(self.counts, 0, 16, 0)?;
        if let Some(p) = &self.parts {
            p.record_choose(r)?;
        }
        let walk = self
            .parts
            .as_ref()
            .map_or([NONE; 2], super::instances::PartsPlan::walk_indices);
        // Instanced draws are listed by the processor and need no command an item.
        let cull = self.batched.is_none();
        r.compute(
            &[
                Touch::Buffer(self.counts, Use::StorageWrite),
                Touch::Buffer(self.commands, Use::StorageWrite),
            ],
            |c| {
                if !cull {
                    return Ok(());
                }
                c.set_pipeline(self.pipelines.cull)?;
                c.push(
                    0,
                    &push_words(&[
                        self.visible,
                        self.visible_count,
                        self.items,
                        self.commands_index,
                        self.counts_index,
                        BUCKET_CAPACITY,
                        self.split,
                        walk[0],
                        walk[1],
                    ]),
                )?;
                c.dispatch(self.visible_count.div_ceil(64).max(1), 1, 1);
                Ok(())
            },
        )?;

        if let Some(p) = &self.parts {
            p.record_write(r)?;
        }
        if let Some(f) = &self.forest {
            super::forest::ForestPass::record_cull(f, r)?;
        }

        // 1b. The scene's acceleration structures, when the frame traces rays: the skinned
        // bodies' structures fitted to the poses 0. recorded.
        if let Some(rt) = &self.rt {
            rt.record(
                r,
                &self.rt_refitting,
                self.frame,
                self.instances,
                self.offsets,
            )?;
        }

        // The sun's shadow maps, when they are drawn: not while the sun's share is traced, unless
        // the fog's volume looks the sun up in them.
        let traced_sun = self.rt_shadow.is_some() && self.rt.is_some();
        if let Some(s) = self
            .shadow
            .as_ref()
            .filter(|_| !traced_sun || self.fog.is_some())
        {
            s.record_maps(
                r,
                self.indices,
                &|f| self.gbuffer_push_seen(f, 1, NONE),
                &|f, list, first| {
                    push_words(&[
                        f,
                        self.ground_vertices,
                        self.ground_cells,
                        list,
                        self.offsets,
                        self.clamp,
                        first,
                        self.ground_splats,
                        self.wrap,
                        NONE,
                    ])
                },
                self.drawn_ground.as_ref(),
                self.parts.as_ref(),
            )?;
        }

        // 2b. The weather's overhead map, where it moved.
        if let Some(w) = &self.weather {
            w.record_cover(r, self.indices, &|f| self.gbuffer_push_seen(f, 1, NONE))?;
        }

        // 2c. The grass field's clumps, placed over the drawn ground, sheltered by the map above.
        if let (Some(g), Some(d)) = (&self.grass, &self.drawn_ground) {
            g.record_place(r, d)?;
        }

        // 2. The G-buffer: the ground, then the meshes.
        r.timestamp("g-buffer");
        let clear = Load::Clear([0.0; 4]);
        let gbuffer = [t.albedo, t.normal, t.light, t.material].map(|texture| ColorAttachment {
            texture,
            load: clear,
        });
        let mut gbuffer_touches = vec![
            Touch::Buffer(self.commands, Use::Indirect),
            Touch::Buffer(self.counts, Use::Indirect),
            Touch::Buffer(self.indices, Use::Index),
        ];
        if let Some(b) = &self.batched {
            gbuffer_touches.push(Touch::Buffer(b.commands, Use::Indirect));
        }
        if let Some(g) = &self.drawn_ground {
            gbuffer_touches.extend(g.touches());
        }
        let cover = self
            .weather
            .as_ref()
            .map(super::weather::WeatherPlan::cover);
        if let Some(c) = cover {
            gbuffer_touches.push(Touch::Texture(c, Use::Sampled));
        }
        r.raster(
            &RasterPass {
                name: "g-buffer",
                color: &gbuffer,
                depth: Some(DepthAttachment {
                    texture: t.depth,
                    clear: Some(1.0),
                    write: true,
                }),
                touches: &gbuffer_touches,
                area: Some(full),
            },
            |d| {
                if let Some(g) = &self.drawn_ground {
                    let [wrap, clamp] = self.ground_samplers;
                    g.draw_ground(d, self.frame, wrap, clamp, self.weather_table())?;
                    return self.draw_buckets(d, 0);
                }
                d.set_pipeline(self.pipelines.ground)?;
                for (side, first, count) in &self.ground_groups {
                    d.push(
                        0,
                        &push_words(&[
                            self.frame,
                            self.ground_vertices,
                            self.ground_cells,
                            self.ground_drawn,
                            self.offsets,
                            self.clamp,
                            *first,
                            self.ground_splats,
                            self.wrap,
                            self.weather_table(),
                        ]),
                    )?;
                    d.draw(side * side * 6, *count, 0, 0);
                }
                self.draw_buckets(d, 0)
            },
        )?;
        if self.looking_in {
            self.record_looking_in(r, &t, full, cover)?;
        }
        if let Some(i) = &self.impostors {
            super::impostor::ImpostorPass::record(i, r, &t)?;
        }
        if let Some(f) = &self.forest {
            super::forest::ForestPass::record_draw(f, r, &t)?;
        }
        if let Some(g) = &self.grass {
            // Outdoors, the depth before the grass is kept for the frame's own: what the
            // interface lays over the world (names, the ring under the selection) stands in front
            // of the blades.
            if !self.inside {
                r.copy_texture(t.depth, t.outdoor_depth)?;
            }
            g.record_draw(r, &t)?;
        }
        if self.inside {
            // The outdoors' depth kept for the resolve, then cleared, the openings stamped into
            // it, and the interior drawn over the outdoors.
            r.copy_texture(t.depth, t.outdoor_depth)?;
            r.timestamp("stamps");
            r.raster(
                &RasterPass {
                    name: "opening stamps",
                    color: &[],
                    depth: Some(DepthAttachment {
                        texture: t.depth,
                        clear: Some(1.0),
                        write: true,
                    }),
                    touches: &[],
                    area: Some(full),
                },
                |d| {
                    if self.stamp_corners > 0 {
                        d.set_pipeline(self.pipelines.stamp)?;
                        d.push(0, &push_words(&[self.frame, self.stamps]))?;
                        d.draw(self.stamp_corners, 1, 0, 0);
                    }
                    Ok(())
                },
            )?;
            r.timestamp("g-buffer inside");
            let keep = [t.albedo, t.normal, t.light, t.material].map(|texture| ColorAttachment {
                texture,
                load: Load::Keep,
            });
            let mut inside_touches = vec![
                Touch::Buffer(self.commands, Use::Indirect),
                Touch::Buffer(self.counts, Use::Indirect),
                Touch::Buffer(self.indices, Use::Index),
            ];
            if let Some(c) = cover {
                inside_touches.push(Touch::Texture(c, Use::Sampled));
            }
            r.raster(
                &RasterPass {
                    name: "g-buffer inside",
                    color: &keep,
                    depth: Some(DepthAttachment {
                        texture: t.depth,
                        clear: None,
                        write: true,
                    }),
                    touches: &inside_touches,
                    area: Some(full),
                },
                |d| self.draw_buckets(d, GROUP_BUCKETS),
            )?;
        }
        let outdoor_depth = if self.inside {
            t.outdoor_depth
        } else {
            t.depth
        };
        let outdoor_depth_index = if self.inside { t.index[6] } else { t.index[4] };
        // 7. The sun's share: traced where the frame traces rays, the cascades' look-up otherwise.
        if let Some(s) = &self.shadow {
            match (&self.rt_shadow, &self.rt) {
                (Some(traced), Some(rt)) => traced.record(
                    r,
                    &t,
                    self.frame,
                    (outdoor_depth, outdoor_depth_index),
                    &rt.traced(self.rt_pools),
                    s.visibility(),
                )?,
                _ => s.record_mask(r, &t, self.frame, (outdoor_depth, outdoor_depth_index))?,
            }
        }

        // 5. The ambient occlusion: traced where the frame traces rays, from the depth otherwise.
        if let Some(ao) = &self.ao {
            match (&self.rt_ao, &self.rt) {
                (Some(traced), Some(rt)) => traced.record(
                    r,
                    &t,
                    self.frame,
                    (outdoor_depth, outdoor_depth_index),
                    &rt.traced(self.rt_pools),
                    ao.mask().1,
                )?,
                _ => ao.record(r, &t, self.frame, (outdoor_depth, outdoor_depth_index))?,
            }
        }

        // The lamps.
        if let Some(lamps) = &self.lamps {
            super::lamps::LampsPass::record(lamps, r, &t)?;
        }

        // 7b. The bounced light.
        if let Some(g) = &self.gi {
            g.record(
                r,
                &t,
                self.frame,
                (outdoor_depth, outdoor_depth_index),
                self.sun_visibility,
                [
                    self.index_pool,
                    self.vertices,
                    self.materials,
                    self.instances,
                    self.ground_vertices,
                    self.ground_cells,
                    self.lights,
                    self.sets,
                    self.wrap,
                ],
            )?;
        }

        // The fog's volume, and the clouds' shadow map.
        if let Some(f) = &self.fog {
            f.record(r, self.frame)?;
        }
        if let Some(a) = &self.atmosphere {
            a.record_cloud_shadow(r)?;
        }

        // 3. The sky before the ground, then the resolve over it: the atmosphere's tables first,
        // where they changed.
        if let Some(a) = &self.atmosphere {
            a.record_tables(r)?;
        }
        let mut sky_touches = vec![
            Touch::Buffer(self.forward, Use::Indirect),
            Touch::Buffer(self.indices, Use::Index),
        ];
        if let Some(a) = &self.atmosphere {
            sky_touches.push(Touch::Texture(a.view_table(), Use::Sampled));
        }
        r.timestamp("sky");
        r.raster(
            &RasterPass {
                name: "sky",
                color: &[ColorAttachment {
                    texture: t.color,
                    load: Load::Clear([0.0, 0.0, 0.0, 1.0]),
                }],
                depth: None,
                touches: &sky_touches,
                area: Some(full),
            },
            |d| {
                self.draw_runs(d, &self.sky_runs, self.sky_frame, NONE)?;
                if let Some(a) = &self.atmosphere {
                    a.draw_sky(d)?;
                }
                self.draw_runs(d, &self.sky_runs_rest, self.sky_frame, NONE)?;
                if let Some(a) = &self.atmosphere {
                    a.draw_veil(d)?;
                }
                Ok(())
            },
        )?;
        // The sky all round the eye, for the water, each way as the frame's own sky is drawn.
        if let Some(probe) = &self.sky_probe {
            r.timestamp("sky probe");
            for k in 0..6 {
                r.raster(
                    &RasterPass {
                        name: "sky probe",
                        color: &[ColorAttachment {
                            texture: probe.faces[k].0,
                            load: Load::Clear([0.0, 0.0, 0.0, 1.0]),
                        }],
                        depth: None,
                        touches: &sky_touches,
                        area: Some(Rect {
                            x: 0,
                            y: 0,
                            width: super::sky_probe::SIDE,
                            height: super::sky_probe::SIDE,
                        }),
                    },
                    |d| {
                        self.draw_runs(d, &self.sky_runs, probe.frames[k], NONE)?;
                        if let (Some(a), Some(skies)) = (&self.atmosphere, probe.skies) {
                            a.draw_seen(d, skies[k], false)?;
                        }
                        self.draw_runs(d, &self.sky_runs_rest, probe.frames[k], NONE)?;
                        if let (Some(a), Some(skies)) = (&self.atmosphere, probe.skies) {
                            a.draw_seen(d, skies[k], true)?;
                        }
                        Ok(())
                    },
                )?;
            }
        }
        r.timestamp("resolve");
        let mut resolve_touches = vec![
            Touch::Texture(t.albedo, Use::Sampled),
            Touch::Texture(t.normal, Use::Sampled),
            Touch::Texture(t.light, Use::Sampled),
            Touch::Texture(t.material, Use::Sampled),
            Touch::Texture(t.depth, Use::Sampled),
            Touch::Texture(outdoor_depth, Use::Sampled),
        ];
        if let Some(f) = &self.fog {
            resolve_touches.push(Touch::Buffer(f.volume, Use::StorageRead));
        }
        let clouds = self
            .atmosphere
            .as_ref()
            .and_then(super::sky::SkyPlan::cloud_shadows);
        if let Some((_, map)) = clouds {
            resolve_touches.push(Touch::Texture(map, Use::Sampled));
        }
        let ambient_visibility = self.ao.as_ref().map(super::ao::AoPlan::mask);
        let bounced = self.gi.as_ref().map(super::gi::GiPlan::mask);
        for mask in [
            self.sun_visibility.1,
            self.lamp_light.1,
            ambient_visibility.map(|m| m.1),
            bounced.map(|m| m.1),
        ]
        .into_iter()
        .flatten()
        {
            resolve_touches.push(Touch::Texture(mask, Use::Sampled));
        }
        r.raster(
            &RasterPass {
                name: "resolve",
                color: &[ColorAttachment {
                    texture: t.color,
                    load: Load::Keep,
                }],
                depth: None,
                touches: &resolve_touches,
                area: Some(full),
            },
            |d| {
                d.set_pipeline(if self.pixel {
                    self.pipelines.resolve_pixel
                } else {
                    self.pipelines.resolve_vertex
                })?;
                d.push(
                    0,
                    &push_words(&[
                        self.frame,
                        t.index[0],
                        t.index[1],
                        t.index[2],
                        t.index[3],
                        t.index[4],
                        self.lights,
                        self.sets,
                        0,
                        outdoor_depth_index,
                        self.sun_visibility.0,
                        self.lamp_light.0,
                        ambient_visibility.map_or(NONE, |m| m.0),
                        self.fog.as_ref().map_or(NONE, |f| f.constants),
                        self.fog.as_ref().map_or(NONE, |f| f.volume_index),
                        bounced.map_or(NONE, |m| m.0),
                        clouds.map_or(NONE, |c| c.0),
                    ]),
                )?;
                d.draw(3, 1, 0, 0);
                Ok(())
            },
        )?;

        // 10a. The scene mirrored in the water, for the water to reflect.
        if let Some(p) = &self.planar {
            self.record_planar(r, p, &gbuffer_touches)?;
        }

        // 10b. The water, over the resolved world.
        if let Some(w) = &self.water {
            w.record(
                r,
                &t,
                self.frame,
                (outdoor_depth, outdoor_depth_index),
                self.sun_visibility,
                self.drawn_ground.as_ref(),
            )?;
        }

        // 4. The cut-outs' edges, then the forward pass: the outdoors', then, inside, the
        // interior's. The air lies over the edges and under the forward pass, whose surfaces keep
        // the fog they were made with, each by its own distance.
        let mask = if self.inside { t.index[3] } else { NONE };
        let (edges, forward) = self.outdoor_lists();
        if let Some(a) = self.atmosphere.as_ref().filter(|a| a.air()) {
            self.blend_passes(r, full, outdoor_depth, mask, edges, BlendList::Runs(&[]))?;
            // The low fog reads the landscape's heights and the sun's cascades.
            let mut reads = Vec::new();
            if let Some(g) = &self.drawn_ground {
                reads.push(Touch::Buffer(g.heights().1, Use::StorageRead));
            }
            if let Some(s) = &self.shadow {
                reads.extend(
                    s.cascades()
                        .1
                        .into_iter()
                        .map(|m| Touch::Texture(m, Use::Sampled)),
                );
            }
            a.record_air(
                r,
                &t,
                self.frame,
                (outdoor_depth, outdoor_depth_index),
                &reads,
            )?;
            if let Some(u) = &self.upscale {
                u.keep_opaque(r, &t)?;
            }
            self.blend_passes(r, full, outdoor_depth, mask, BlendList::Runs(&[]), forward)?;
        } else {
            if let Some(u) = &self.upscale {
                u.keep_opaque(r, &t)?;
            }
            self.blend_passes(r, full, outdoor_depth, mask, edges, forward)?;
        }
        if self.inside {
            self.blend_passes(
                r,
                full,
                t.depth,
                NONE,
                BlendList::Runs(&self.edge_runs_inside),
                BlendList::Runs(&self.forward_runs_inside),
            )?;
        }

        // 11a. The lamps' halos, over everything the world drew.
        if let Some(lamps) = &self.lamps {
            super::lamps::LampsPass::record_halos(lamps, r, &t, self.frame)?;
        }

        // 11b. The rain, the snow and the splashes, over everything the world drew.
        if let Some(w) = &self.weather {
            w.record_fall(r, &t, self.frame, (outdoor_depth, outdoor_depth_index))?;
        }

        // 12. The post passes, as far as the frames gathered.
        let gathered = match &self.post {
            Some(post) => Some(post.record_scene(
                r,
                &t,
                self.frame,
                (outdoor_depth, outdoor_depth_index),
                ([self.items, self.instances, self.vertices], self.indices),
                (self.upscale.as_ref(), self.grass.as_ref()),
            )?),
            None => None,
        };
        Ok(SceneRecorded(gathered))
    }

    /// Record the rest of the frame's passes: the post passes' end, and the composite into `color`
    /// and `depth`, handed back as `color_back` and `depth_back` say.
    ///
    /// # Errors
    /// What the layer refuses.
    pub fn record_finish(
        &self,
        r: &mut Recorder<'_>,
        SceneRecorded(gathered): SceneRecorded,
        color: Texture,
        depth: Texture,
        color_back: State,
        depth_back: State,
    ) -> Result<(), dereth_gfx::Error> {
        let t = self.targets;
        if let (Some(post), Some(gathered)) = (&self.post, gathered) {
            post.record_finish(r, self.frame, gathered)?;
        }
        let (shown, shown_index) = self
            .post
            .as_ref()
            .map_or((t.color, t.index[5]), super::post::PostPlan::output);
        // The frame's depth: the world's, without the grass outdoors.
        let (depth_kept, depth_kept_index) = if self.grass.is_some() && !self.inside {
            (t.outdoor_depth, t.index[6])
        } else {
            (t.depth, t.index[4])
        };

        // 5. The composite into the frame.
        r.timestamp("composite");
        r.raster(
            &RasterPass {
                name: "composite",
                color: &[ColorAttachment {
                    texture: color,
                    load: Load::Keep,
                }],
                depth: Some(DepthAttachment {
                    texture: depth,
                    clear: None,
                    write: true,
                }),
                touches: &[
                    Touch::Texture(shown, Use::Sampled),
                    Touch::Texture(depth_kept, Use::Sampled),
                ],
                area: Some(self.area),
            },
            |d| {
                d.set_pipeline(self.pipelines.composite)?;
                #[allow(clippy::cast_precision_loss)] // targets' sizes
                let depth_scale = [
                    t.width as f32 / self.area.width as f32,
                    t.height as f32 / self.area.height as f32,
                ];
                d.push(
                    0,
                    &push_words(&[
                        shown_index,
                        depth_kept_index,
                        u32::try_from(self.area.x).unwrap_or(0),
                        u32::try_from(self.area.y).unwrap_or(0),
                        depth_scale[0].to_bits(),
                        depth_scale[1].to_bits(),
                    ]),
                )?;
                d.draw(3, 1, 0, 0);
                Ok(())
            },
        )?;
        r.timestamp("end");
        r.touch(&[
            Touch::Texture(color, Use::External(color_back)),
            Touch::Texture(depth, Use::External(depth_back)),
        ])?;
        Ok(())
    }
}

/// What [`Plan::record_scene`] leaves for [`Plan::record_finish`].
#[derive(Debug)]
pub struct SceneRecorded(Option<super::post::Gathered>);

/// The state the frame's colour image is handed back in: laid out for the interface's drawing,
/// the composite's writes visible to it.
#[must_use]
pub fn color_back(layout: vk::ImageLayout) -> State {
    State {
        layout,
        stages: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
        access: vk::AccessFlags2::COLOR_ATTACHMENT_READ | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
    }
}

/// The state the frame's depth image is handed back in.
#[must_use]
pub fn depth_back(layout: vk::ImageLayout) -> State {
    State {
        layout,
        stages: vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS
            | vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS,
        access: vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ
            | vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE,
    }
}

/// What a blend pass draws: runs of the frame's own commands, or a list the device sorted (0 the
/// cut-outs' edges, 1 the blended surfaces).
#[derive(Debug, Clone, Copy)]
enum BlendList<'a> {
    Runs(&'a [(Pipeline, u32, u32)]),
    Sorted(u32),
}

/// The processor's blocks' parts at their places in the walk, as `drawn` listed them: each place
/// and its commands in the two outdoor buckets, the first in the low half; and the processor's
/// other outdoor items in each bucket.
fn walk_counts(drawn: &super::Drawn, scene: &super::scene::GpuScene) -> (Vec<[u32; 2]>, [u32; 2]) {
    let mut walk: Vec<[u32; 2]> = Vec::new();
    let mut tails = [0u32; 2];
    for (item, w) in drawn.opaque.iter().zip(&drawn.opaque_walk) {
        let bucket = scene.buckets.get(*item as usize).map_or(0, |b| *b as u32);
        if bucket >= GROUP_BUCKETS {
            continue;
        }
        if *w == NONE {
            tails[bucket as usize] += 1;
            continue;
        }
        let add = if bucket == 0 { 1 } else { 1 << 16 };
        match walk.last_mut() {
            Some(last) if last[0] == *w => last[1] += add,
            _ => walk.push([*w, add]),
        }
    }
    (walk, tails)
}

/// Each visible item's three words as the cull reads them, and each bucket's commands as the
/// processor placed them: the item, its place, and its block part's place in the walk (`NONE`
/// for none). The outdoor items, the first `split`, have their parts' places in `walk`. Where
/// the device places them (`walking`), an outdoor item of a block's part takes its place among its
/// part's commands in its bucket, and any other outdoor item its place among the processor's
/// others; otherwise, and for the interior's, its place is its bucket's next.
fn placed_items(
    visible: &[u32],
    (split, walk): (usize, &[u32]),
    walking: bool,
    scene: &super::scene::GpuScene,
) -> (Vec<u32>, [u32; CULLED_BUCKETS as usize]) {
    let mut next = [0u32; CULLED_BUCKETS as usize];
    let mut placed = Vec::with_capacity(visible.len() * 3);
    let mut part = (NONE, [0u32; 2]);
    let mut tails = [0u32; 2];
    for (k, item) in visible.iter().enumerate() {
        let own = scene.buckets.get(*item as usize).map_or(0, |b| *b as u32);
        let bucket = own + if k >= split { GROUP_BUCKETS } else { 0 };
        let slot = next.get_mut(bucket as usize).map_or(0, |n| {
            let s = *n;
            *n += 1;
            s
        });
        let w = walk.get(k).copied().unwrap_or(NONE);
        if walking && bucket < GROUP_BUCKETS {
            let b = bucket as usize;
            let local = if w == NONE {
                tails[b] += 1;
                tails[b] - 1
            } else {
                if part.0 != w {
                    part = (w, [0; 2]);
                }
                part.1[b] += 1;
                part.1[b] - 1
            };
            placed.extend([*item, local, w]);
        } else {
            placed.extend([*item, slot, NONE]);
        }
    }
    (placed, next)
}

/// A view's buckets drawn as instanced draws, one a mesh's items ([`super::batch`]): the commands,
/// the table each instance's item is read from, and each bucket's first command and how many.
#[derive(Debug, Clone, Copy)]
struct Batched {
    commands: Buffer,
    remap: u32,
    runs: [(u32, u32); CULLED_BUCKETS as usize],
}

/// The bytes of the commands [`super::batch`] writes: Vulkan's indexed indirect draw.
const fn batch_command_bytes() -> u32 {
    #[allow(clippy::cast_possible_truncation)] // five words
    let words = super::batch::COMMAND_WORDS as u32;
    words * 4
}
