//! The remaster turned on and off, and each published frame handed to it.
//!
//! On, the remaster adopts the Vulkan device and is installed as its world hook; the scene's
//! bridge publishes into an inbox the hook and the scene share; and the walk only decides. At each
//! frame's world end the scene takes what was published, reads the assets it names from the dats
//! through [`DatAssets`], and hands both to the remaster before the device calls the hook. Off,
//! the hook and the inbox go and the walk draws again.
//!
//! The device may uninstall a hook that failed; the walk then still only decides until the next
//! preference poll turns it off ([`SceneDraw::remaster_lost`]), so that frame has no world.

use std::cell::RefCell;
use std::rc::Rc;

use dereth_dat::RetailDatStore;
use dereth_remaster::hook::{Inbox, Remaster};
use dereth_remaster::render::light::LightMode;
pub use dereth_remaster::render::settings::Settings;
pub use dereth_remaster::render::shadow::{Casters, ShadowSettings, ShadowStats};
pub use dereth_remaster::render::sky::SkySettings;
pub use dereth_remaster::render::terrain::TerrainSettings;
use dereth_render::device::{Backend, Gpu};
use dereth_world_render::detail::DetailClass;

use super::{DatAssets, SceneDraw};

/// What the scene keeps of the remaster while it is on.
#[derive(Debug, Default)]
pub(crate) struct View {
    /// The inbox the bridge publishes into, shared with the scene's own receiver slot.
    inbox: Option<Rc<RefCell<Inbox>>>,
    /// The dats the remaster's assets are read from.
    pub(super) store: Option<RetailDatStore>,
    /// How the light is worked out.
    mode: LightMode,
    /// The sun's shadows' settings.
    shadows: ShadowSettings,
    /// The sky's atmosphere's settings.
    sky: SkySettings,
    /// How the ground's cells are drawn.
    terrain: TerrainSettings,
    /// What the box last asked, as the preference poll applied it: `None` before the first.
    applied: Option<bool>,
    /// The looks laid over retail's light, kept while the remaster is off.
    pub(super) looks: dereth_remaster::look::LookStack,
    /// The player's look file, and what was last read of it.
    pub(super) look_file: super::remaster_look::LookTomlWatch,
    /// How the ambient occlusion is found.
    pub(super) ao: super::remaster_effects::RemasterAo,
    /// How the post passes draw.
    pub(super) post: super::remaster_effects::RemasterPost,
    /// How the frame is upscaled.
    pub(super) upscale: super::remaster_effects::RemasterUpscale,
    /// The art pack the application opened, and what of it is drawn.
    pub(super) pack: Option<std::sync::Arc<super::remaster_pack::PackAssets>>,
    pub(super) packs: super::remaster_pack::RemasterPacks,
    /// How the water and the puddles are drawn.
    pub(super) water: super::remaster_effects::RemasterWater,
    /// Whether the rain and the snow fall.
    pub(super) weather: super::remaster_weather::RemasterWeather,
    /// What the remaster's own boxes last asked, and whether the remaster was running then.
    pub(super) rows: super::remaster_rows::RemasterRows,
    pub(super) rows_on: bool,
    /// How the bounced light is found.
    pub(super) gi: super::remaster_rt::RemasterGi,
    /// Whether the sun's shadows and the occlusion are traced.
    pub(super) traced_shadows: super::remaster_traced::RemasterTracedShadows,
    pub(super) traced_ao: super::remaster_traced::RemasterTracedAo,
}

/// Where the remaster works retail's light out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RemasterLight {
    /// Per vertex, as retail does.
    #[default]
    Vertex,
    /// Per pixel, by the same rules.
    Pixel,
}

impl RemasterLight {
    fn mode(self) -> LightMode {
        match self {
            Self::Vertex => LightMode::Vertex,
            Self::Pixel => LightMode::Pixel,
        }
    }
}

/// Why the remaster was not turned on.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemasterRefused {
    /// The device is neither the Vulkan one nor the Direct3D 12 one.
    NotVulkan,
    /// The device has not the remaster's features (on Vulkan it needs Vulkan 1.3, and to have
    /// been made asking for them; on Direct3D 12, shader model 6.6 and every resource bound
    /// through one heap).
    Unsupported,
    /// The device would not take it, or the remaster could not be made on it.
    Failed(String),
}

impl std::fmt::Display for RemasterRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotVulkan => f.write_str("the remaster needs the Vulkan or Direct3D 12 renderer"),
            Self::Unsupported => f.write_str("this graphics card cannot draw the remaster"),
            Self::Failed(why) => write!(f, "the remaster could not start: {why}"),
        }
    }
}

impl SceneDraw {
    /// Whether the remaster is on.
    #[must_use]
    pub fn remaster_on(&self) -> bool {
        self.remaster_view.borrow().inbox.is_some()
    }

    /// Turn the remaster on, reading its assets from `store`, or off. Turning it on again while
    /// it is on, or off while it is off, does nothing.
    ///
    /// # Errors
    /// [`RemasterRefused`] on a device other than Vulkan or Direct3D 12, one without the
    /// remaster's features, or when the remaster cannot be made on it; the scene is then as it
    /// was.
    pub fn set_remaster(
        &mut self,
        on: bool,
        store: &RetailDatStore,
        gpu: &mut Gpu,
    ) -> Result<(), RemasterRefused> {
        if on == self.remaster_on() {
            return Ok(());
        }
        if !on {
            drop(gpu.take_world_hook());
            drop(self.remove_remaster_receiver());
            self.set_decide_only(false);
            let view = self.remaster_view.get_mut();
            view.inbox = None;
            view.store = None;
            return Ok(());
        }
        let adopted = match gpu.backend() {
            Backend::Vulkan => {
                let Some(device) = gpu.remaster_device() else {
                    return Err(RemasterRefused::Unsupported);
                };
                Remaster::adopt(&device)
            }
            #[cfg(windows)]
            Backend::D3d12 => {
                let Some(device) = gpu.remaster_d3d12_device() else {
                    return Err(RemasterRefused::Unsupported);
                };
                Remaster::adopt_d3d12(&device)
            }
            _ => return Err(RemasterRefused::NotVulkan),
        };
        let mut remaster = adopted.map_err(|e| RemasterRefused::Failed(e.to_string()))?;
        remaster
            .renderer()
            .set_mode(self.remaster_view.get_mut().mode);
        remaster
            .renderer()
            .set_shadows(self.remaster_view.get_mut().shadows);
        remaster
            .renderer()
            .set_sky(self.remaster_view.get_mut().sky);
        remaster
            .renderer()
            .set_terrain(self.remaster_view.get_mut().terrain);
        remaster
            .renderer()
            .set_looks(self.remaster_view.get_mut().looks.clone());
        remaster.renderer().set_ao(self.remaster_view.get_mut().ao);
        remaster
            .renderer()
            .set_post(self.remaster_view.get_mut().post);
        remaster
            .renderer()
            .set_upscale(self.remaster_view.get_mut().upscale);
        remaster
            .renderer()
            .set_packs(self.remaster_view.get_mut().packs);
        let view = self.remaster_view.get_mut();
        remaster
            .renderer()
            .set_pack_look(view.pack.is_some() && view.packs.any());
        remaster
            .renderer()
            .set_water(self.remaster_view.get_mut().water);
        remaster
            .renderer()
            .set_weather(self.remaster_view.get_mut().weather);
        remaster.renderer().set_gi(self.remaster_view.get_mut().gi);
        remaster
            .renderer()
            .set_rt_shadows(self.remaster_view.get_mut().traced_shadows);
        remaster
            .renderer()
            .set_rt_ao(self.remaster_view.get_mut().traced_ao);
        // A scene that came before this one on the device, gone without being told to stop, left
        // its remaster behind: entering the world makes a new scene.
        drop(gpu.take_world_hook());
        gpu.install_world_hook(Box::new(remaster))
            .map_err(|e| RemasterRefused::Failed(e.to_string()))?;
        let inbox = Rc::new(RefCell::new(Inbox::default()));
        drop(self.install_remaster_receiver(Box::new(Rc::clone(&inbox))));
        self.set_decide_only(true);
        let view = self.remaster_view.get_mut();
        view.inbox = Some(inbox);
        view.store = Some(store.clone());
        Ok(())
    }

    /// Work the remaster's light out per vertex, as retail does, or per pixel, from the next frame.
    pub fn set_remaster_light(&mut self, light: RemasterLight, gpu: &mut Gpu) {
        let mode = light.mode();
        self.remaster_view.get_mut().mode = mode;
        if let Some(r) = remaster(gpu) {
            r.renderer().set_mode(mode);
        }
    }

    /// Draw the sun's shadows with `s` from the next frame; they show where the light is worked
    /// out per pixel.
    pub fn set_remaster_shadows(&mut self, s: ShadowSettings, gpu: &mut Gpu) {
        self.remaster_view.get_mut().shadows = s;
        if let Some(r) = remaster(gpu) {
            r.renderer().set_shadows(s);
        }
    }

    /// Draw the sky's atmosphere with `s` from the next frame; it shows where the light is worked
    /// out per pixel.
    pub fn set_remaster_sky(&mut self, s: SkySettings, gpu: &mut Gpu) {
        self.remaster_view.get_mut().sky = s;
        if let Some(r) = remaster(gpu) {
            r.renderer().set_sky(s);
        }
    }

    /// Every pass's settings past retail's own light at once, from the next frame on.
    pub fn set_remaster_settings(&mut self, s: Settings, gpu: &mut Gpu) {
        self.set_remaster_shadows(s.shadows, gpu);
        self.set_remaster_ao(s.ao, gpu);
        self.set_remaster_lamps(
            super::RemasterLamps {
                indoor: s.lamps.indoor,
                outdoor: s.lamps.outdoor,
            },
            gpu,
        );
        self.set_remaster_sky(s.sky, gpu);
        self.set_remaster_post(s.post, gpu);
        self.set_remaster_packs(s.packs, gpu);
        self.set_remaster_water(s.water, gpu);
        self.set_remaster_weather(s.weather, gpu);
        self.set_remaster_traced_shadows(s.rt_shadows, gpu);
        self.set_remaster_traced_ao(s.rt_ao, gpu);
        self.set_remaster_terrain(s.terrain, gpu);
        self.set_remaster_gi(s.gi, gpu);
        self.set_remaster_upscale(s.upscale, gpu);
    }

    /// Each of the remaster's passes' time on the device in its last finished frame,
    /// milliseconds; empty while it is off or the device writes no timestamps.
    #[cfg(feature = "test-support")]
    pub fn remaster_timings(&self, gpu: &mut Gpu) -> Vec<(&'static str, f64)> {
        remaster(gpu).map_or_else(Vec::new, |r| r.renderer().pass_timings())
    }

    /// The height the remaster's last frame drew its ground's mesh at, at render-space `points`,
    /// without the fine relief; `None` where it drew no patch over a point. Empty while the
    /// remaster is off.
    #[cfg(feature = "test-support")]
    pub fn remaster_drawn_mesh_heights(
        &self,
        gpu: &mut Gpu,
        points: &[[f32; 2]],
    ) -> Vec<Option<f32>> {
        let points: Vec<glam::Vec2> = points.iter().map(|p| glam::Vec2::from(*p)).collect();
        remaster(gpu).map_or_else(Vec::new, |r| r.renderer().drawn_mesh_heights(&points))
    }

    /// Retail's height at render-space `points`, over the world's vertices the remaster's last
    /// frame drew its ground from. Empty while it drew none.
    #[cfg(feature = "test-support")]
    pub fn remaster_retail_heights(&self, gpu: &mut Gpu, points: &[[f32; 2]]) -> Vec<f32> {
        let points: Vec<glam::Vec2> = points.iter().map(|p| glam::Vec2::from(*p)).collect();
        remaster(gpu).map_or_else(Vec::new, |r| r.renderer().retail_heights(&points))
    }

    /// The trees the remaster's last frame drew as meshes and lifted onto its ground: where each
    /// stands (render space), how far the G-buffer's vertex shader lifts it, and how far the
    /// impostor cards' arithmetic does.
    #[cfg(feature = "test-support")]
    pub fn remaster_mesh_tree_bases(&self, gpu: &mut Gpu) -> Vec<([f32; 3], f32, f32)> {
        remaster(gpu).map_or_else(Vec::new, |r| {
            let r = r.renderer();
            let trees = r.mesh_trees().to_vec();
            r.instance_bases(&trees)
                .into_iter()
                .map(|(o, gpu, cards)| (o.to_array(), gpu, cards))
                .collect()
        })
    }

    /// Where the remaster's last frame stood its impostor cards' trees (render space), and how
    /// far each was lifted onto its ground.
    #[cfg(feature = "test-support")]
    pub fn remaster_impostor_bases(&self, gpu: &mut Gpu) -> Vec<([f32; 3], f32)> {
        remaster(gpu).map_or_else(Vec::new, |r| {
            r.renderer()
                .impostor_bases()
                .into_iter()
                .map(|(b, lift)| (b.to_array(), lift))
                .collect()
        })
    }

    /// Draw the remaster's water with no ripples' picture: for the check that its ripples are
    /// that picture's.
    #[cfg(feature = "test-support")]
    pub fn set_remaster_water_without_picture(&self, gpu: &mut Gpu, without: bool) {
        if let Some(r) = remaster(gpu) {
            r.renderer().set_water_without_picture(without);
        }
    }

    /// Turn every ground cell the remaster draws `quarters` more quarter turns than its own: for
    /// the checks that a cell's pictures and their maps turn with it.
    #[cfg(feature = "test-support")]
    pub fn set_remaster_turned_cells(&self, gpu: &mut Gpu, quarters: u8) {
        if let Some(r) = remaster(gpu) {
            r.renderer().set_turned_cells(quarters);
        }
    }

    /// The particles the remaster was last handed, in the order handed: where the walk put each
    /// (render space), how far it was lifted onto the remaster's ground, and whether it stands
    /// outdoors.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn remaster_walked_particles(&self) -> Vec<([f32; 3], f32, bool)> {
        let lifted = self.remaster_ground.active;
        self.frame_walk
            .borrow()
            .as_ref()
            .map_or_else(Vec::new, |log| {
                log.particles
                    .iter()
                    .map(|w| {
                        let o = w.drawn.frame.origin;
                        let lift = if lifted && w.drawn.outdoors {
                            self.remaster_ground.shift_at(o)
                        } else {
                            0.0
                        };
                        ([o.x, o.y, o.z], lift, w.drawn.outdoors)
                    })
                    .collect()
            })
    }

    /// Where the remaster's last frame drew each particle it was handed (render space), in the
    /// order handed.
    #[cfg(feature = "test-support")]
    pub fn remaster_particle_origins(&self, gpu: &mut Gpu) -> Vec<[f32; 3]> {
        remaster(gpu).map_or_else(Vec::new, |r| {
            r.renderer()
                .particle_origins()
                .into_iter()
                .map(|p| p.to_array())
                .collect()
        })
    }

    /// The drawn height at `points` (render-space x and y, and the distance from the eye) as the
    /// remaster's device works it out for `ground` (its last frame's, when `None`), without the
    /// fine relief; empty while the remaster is off or has no ground.
    #[cfg(feature = "test-support")]
    pub fn remaster_drawn_heights(
        &self,
        gpu: &mut Gpu,
        ground: Option<&dereth_remaster::scene::DrawnGround>,
        points: &[[f32; 3]],
    ) -> Vec<f32> {
        remaster(gpu).map_or_else(Vec::new, |r| r.renderer().drawn_heights(ground, points))
    }

    /// Whether the remaster's ground wants the world's vertices for its grid; the visual offset,
    /// with what stands on the ground drawn on it; and the far ground out to the horizon: only
    /// while the remaster is on.
    pub(super) fn remaster_ground_wanted(&self) -> (bool, bool, bool) {
        let view = self.remaster_view.borrow();
        let on = view.inbox.is_some();
        let t = view.terrain;
        (
            on && t.grid,
            on && t.grid && t.drawn,
            on && t.grid && t.horizon,
        )
    }

    /// Draw the ground's cells as `s` says, from the next frame.
    pub fn set_remaster_terrain(&mut self, s: TerrainSettings, gpu: &mut Gpu) {
        self.remaster_view.get_mut().terrain = s;
        if let Some(r) = remaster(gpu) {
            r.renderer().set_terrain(s);
        }
    }

    /// Whether the remaster is on but its hook is gone: the device uninstalled it after a
    /// failure. The preference poll turns it off then.
    #[must_use]
    pub fn remaster_lost(&self, gpu: &mut Gpu) -> bool {
        self.remaster_on() && remaster(gpu).is_none()
    }

    /// Apply the remaster's box, as the preference poll reads it (`want`): turn the remaster on
    /// or off when the box moved since the last poll, and off when the device took it off after
    /// a failure. Returns what the chat window tells the player when it was refused or failed;
    /// a refused box stays as it is, and is tried again only when it moves.
    pub(super) fn apply_remaster(
        &mut self,
        want: bool,
        store: &RetailDatStore,
        gpu: &mut Gpu,
    ) -> Option<&'static str> {
        use dereth_client_contract::options::remaster;
        if self.remaster_lost(gpu) {
            let _ = self.set_remaster(false, store, gpu);
            self.remaster_view.get_mut().applied = Some(want);
            return Some(remaster::FAILED);
        }
        let view = self.remaster_view.get_mut();
        if view.applied == Some(want) {
            return None;
        }
        view.applied = Some(want);
        // The player's remaster draws every piece of the ground.
        if want {
            self.set_remaster_terrain(TerrainSettings::player(), gpu);
        }
        match self.set_remaster(want, store, gpu) {
            Ok(()) => None,
            Err(RemasterRefused::NotVulkan) => Some(remaster::REFUSED_RENDERER),
            Err(RemasterRefused::Unsupported) if gpu.remaster_requested() => {
                Some(remaster::REFUSED_CARD)
            }
            Err(RemasterRefused::Unsupported) => Some(remaster::REFUSED_UNTIL_RESTART),
            Err(RemasterRefused::Failed(why)) => {
                tracing::warn!(target: "dereth::render", "the remaster did not start: {why}");
                Some(remaster::FAILED)
            }
        }
    }
}

/// The remaster's own pictures of the last frame it drew, at the world's size, and where the
/// world stands on the frame's target. With an upscaler on, the colors are at the display's size
/// (`width` by `height`) and the rest at the size the world was drawn at.
#[derive(Debug, Clone, PartialEq)]
pub struct RemasterCapture {
    pub width: u32,
    pub height: u32,
    /// The size the world was drawn at.
    pub render_size: (u32, u32),
    /// The world's top-left corner on the frame's target.
    pub x: u32,
    pub y: u32,
    /// Its colors, RGBA, row by row.
    pub color: Vec<u8>,
    /// Each pixel's light class: 0 nothing, 1 the ground, 2 lit by the sun, 3 an object indoors,
    /// 4 a room's walls, 5 the sky.
    pub class: Vec<u8>,
    /// Each pixel's material kind (7 water), and on the ground the water's share of it, 0 to 255.
    pub kind: Vec<u8>,
    pub water: Vec<u8>,
    /// On the ground, how far under the water's surface its bed is drawn, in steps of 4 cm.
    pub bed: Vec<u8>,
    /// Each pixel's depth.
    pub depth: Vec<f32>,
    /// Each pixel's roughness, as the G-buffer holds it (its albedo's alpha), 0 to 255.
    pub roughness: Vec<u8>,
    /// Each pixel's color before any light, as the G-buffer holds it.
    pub albedo: Vec<[f32; 3]>,
    /// The frame's view and projection together, as sixteen numbers by column, and its eye.
    pub view_projection: [f32; 16],
    pub eye: [f32; 3],
    /// How much of the sun reached each pixel, 0 to 255, when the frame drew the sun's shadows.
    pub sun_visibility: Option<Vec<u8>>,
    /// Each pixel's normal, as the G-buffer holds it.
    pub normal: Vec<[f32; 3]>,
}

/// The normal the octahedral encoding `e` holds.
fn oct_decoded(e: [f32; 2]) -> [f32; 3] {
    let (x, y) = (e[0], e[1]);
    let z = 1.0 - x.abs() - y.abs();
    let (x, y) = if z < 0.0 {
        ((1.0 - y.abs()) * x.signum(), (1.0 - x.abs()) * y.signum())
    } else {
        (x, y)
    };
    let l = (x * x + y * y + z * z).sqrt().max(1e-9);
    [x / l, y / l, z / l]
}

/// What the remaster has counted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RemasterCounts {
    /// Pipelines built since it was made.
    pub pipelines_built: u64,
    /// Bytes written to the device for its frames' own state, since it was made.
    pub frame_bytes: u64,
    /// Bytes written to the device for its retained scene and assets, since it was made.
    pub scene_bytes: u64,
    /// Frames it drew.
    pub frames: u64,
    /// The draw items of its last frame: in the G-buffer, in the forward pass, and of those the
    /// particles'.
    pub items: u32,
    pub forward: u32,
    pub particle_items: u32,
    /// What the sun's shadows counted.
    pub shadows: ShadowStats,
    /// The bytes its retained scene holds on the device: its pools' buffers and its textures; and
    /// of the pools' bytes, those in use.
    pub resident_bytes: u64,
    pub pool_bytes_used: u64,
    /// The bytes the ground drawn as a grid holds: the world's vertices and the near offset.
    pub ground_bytes: u64,
    /// Blocks it took in, took again whole, and took again for their ground alone.
    pub blocks_in: u64,
    pub blocks_changed: u64,
    pub grounds_changed: u64,
    /// The pictures held, and of them the ground's composite pictures; its splats; and the
    /// pictures they blend.
    pub textures: usize,
    pub terrain_textures: usize,
    pub splats: usize,
    pub terrain_sources: usize,
    /// Blocks, blocks' grounds, objects and meshes it holds.
    pub blocks: usize,
    pub grounds: usize,
    pub objects: usize,
    pub meshes: usize,
}

impl SceneDraw {
    /// What the remaster has counted; `None` while it is off.
    #[must_use]
    pub fn remaster_counts(&self, gpu: &mut Gpu) -> Option<RemasterCounts> {
        let r = remaster(gpu)?;
        let frames = r.frames;
        let renderer = r.renderer();
        let s = renderer.stats();
        Some(RemasterCounts {
            pipelines_built: renderer.device().stats().pipelines_built,
            frame_bytes: s.frame_bytes,
            scene_bytes: renderer.scene_bytes(),
            frames,
            items: s.last_items,
            forward: s.last_forward,
            particle_items: s.last_particle_items,
            shadows: renderer.shadows().1,
            resident_bytes: renderer.resident_bytes(),
            pool_bytes_used: renderer.pool_bytes().iter().map(|p| p.1).sum(),
            ground_bytes: renderer.ground_bytes(),
            blocks_in: renderer.changes().blocks_in,
            blocks_changed: renderer.changes().blocks_changed,
            grounds_changed: renderer.changes().grounds_changed,
            textures: renderer.tables().counts().textures,
            terrain_textures: renderer.tables().counts().terrain_textures,
            splats: renderer.tables().counts().splats,
            terrain_sources: renderer.tables().counts().terrain_sources,
            blocks: renderer.blocks(),
            grounds: renderer.grounds(),
            objects: renderer.objects(),
            meshes: renderer.meshes(),
        })
    }

    /// The remaster's own pictures of the last frame it drew; `None` while it is off or before
    /// it has drawn. The device waits for its work first.
    #[must_use]
    pub fn remaster_capture(&self, gpu: &mut Gpu) -> Option<RemasterCapture> {
        let r = remaster(gpu)?;
        let b = r.renderer().read_back()?;
        Some(RemasterCapture {
            width: b.width,
            height: b.height,
            render_size: b.render_size,
            x: b.x,
            y: b.y,
            color: b.color,
            class: b.material.as_chunks::<4>().0.iter().map(|p| p[0]).collect(),
            kind: b.material.as_chunks::<4>().0.iter().map(|p| p[1]).collect(),
            water: b
                .material
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| if p[0] == 1 { p[2] } else { 0 })
                .collect(),
            bed: b
                .material
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| if p[0] == 1 { p[3] } else { 0 })
                .collect(),
            depth: b
                .depth
                .as_chunks::<4>()
                .0
                .iter()
                .map(|w| f32::from_le_bytes(*w))
                .collect(),
            roughness: b
                .albedo
                .as_chunks::<8>()
                .0
                .iter()
                .map(|p| {
                    let a = super::remaster_lamps::f16_to_f32(u16::from_le_bytes([p[6], p[7]]));
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    // LINT-OK: 0..1 brought to 0..255 and rounded.
                    let byte = (a.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                    byte
                })
                .collect(),
            albedo: b
                .albedo
                .as_chunks::<8>()
                .0
                .iter()
                .map(|p| {
                    let part = |k: usize| {
                        super::remaster_lamps::f16_to_f32(u16::from_le_bytes([p[k], p[k + 1]]))
                    };
                    [part(0), part(2), part(4)]
                })
                .collect(),
            view_projection: b.view_projection.to_cols_array(),
            eye: b.eye.to_array(),
            sun_visibility: b.sun_visibility,
            normal: b
                .normal
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| {
                    let s = |lo: u8, hi: u8| {
                        (f32::from(i16::from_le_bytes([lo, hi])) / 32767.0).max(-1.0)
                    };
                    oct_decoded([s(p[0], p[1]), s(p[2], p[3])])
                })
                .collect(),
        })
    }
}

/// The installed remaster, when the device has one.
pub(super) fn remaster(gpu: &mut Gpu) -> Option<&mut Remaster> {
    gpu.world_hook_mut()?
        .as_any_mut()
        .downcast_mut::<Remaster>()
}

/// Hand the remaster what the bridge published this frame, with its assets read from the dats.
/// Nothing when the remaster is off.
pub(super) fn deliver(draw: &SceneDraw, gpu: &mut Gpu) {
    let view = draw.remaster_view.borrow();
    let (Some(inbox), Some(store)) = (view.inbox.as_ref(), view.store.as_ref()) else {
        return;
    };
    let Some((deltas, frame)) = inbox.borrow_mut().take() else {
        return;
    };
    let Some(r) = remaster(gpu) else {
        return;
    };
    // The detail pictures Retail Look draws the buildings' shells and the rooms' walls under, as
    // the scene has them now: only those it has a picture for.
    let detail = [DetailClass::Building, DetailClass::Environment].map(|cls| {
        draw.current_detail(cls)?;
        let d = draw.detail.current(cls)?;
        Some((d.surface.texture.0, d.tiling))
    });
    let mut src = DatAssets::for_scene(store, draw).with_pack(view.pack.clone(), view.packs);
    r.receive(deltas, frame, detail, &mut src);
}
