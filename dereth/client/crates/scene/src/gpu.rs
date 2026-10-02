//! The graphics device with the drawn world and the preview spaces on it: the scene's half of
//! the client's renderer.
//!
//! Device creation and presentation follow the frame bracket: prepare the graphics device, start
//! the frame, and end it. `dereth_render::device` does the work. What draws while no world is
//! loaded is one retail surface stretched over the back buffer, the first pixel from real data.
//! The UI that is drawn over the scene, and the preview spaces' placement in it, are the client
//! shell's (`dereth-client-shell`), whose renderer holds this one.
//!
//! **This module wires; it does not implement.** The texture decode below is
//! `dereth_assets::RenderSurface` + `dereth_render::texture::decode_surface`, and the quad geometry
//! and the half-pixel compensation are `dereth_render::ui`'s.

/// The retail surface the first-pixel quad draws, re-exported from
/// [`dereth_client_runtime::assets`] -- it is a dat id and nothing else.
pub use dereth_client_runtime::assets::FIRST_PIXEL_SURFACE;

/// Which preview space. There are only ever a handful and they are named rather than
/// allocated: `WorldView` has exactly one, and every char-gen page uses the same
/// shared character-preview viewport. The contract's, so any UI names the same four.
pub use dereth_client_contract::overlay::PreviewSpace as PreviewId;

#[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
pub use imp::*;

#[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
mod imp {
    use std::path::Path;

    use dereth_assets::{Decode, RenderSurface};
    use dereth_primitives::{AssetSource, DataId};
    use dereth_render::device::{
        DeviceConfig, Gpu, PerDrawConstants, PerFrameConstants, TextureSlot, WindowHandles,
    };
    use dereth_render::pixel_format::PixelFormatId;
    use dereth_render::pso::{PipelineKey, SurfaceContext};
    use dereth_render::surface::{surface_type as st, Surface};
    use dereth_render::texture::{decode_surface, SourcePixels};
    use dereth_render::vertex::VertexFormat;
    use dereth_render::{ui, DrawConstants, RenderError, ViewParams, ZFunc};

    use super::PreviewId;
    use dereth_client_contract::overlay::{
        OverlayItem, OverlayMaterial, OverlayReleased, OverlaySampler, OverlaySpace,
        OverlayTexture, OverlayVertex,
    };

    #[cfg(test)]
    use super::FIRST_PIXEL_SURFACE;

    /// The device, the first-pixel quad, the drawn world and the preview spaces.
    ///
    /// The quad is what draws while no world is loaded; [`SceneRenderer::load_world`] replaces it.
    /// The client shell's renderer holds this beside what the UI draws, so the renderer's slot in
    /// the documented frame order stays a single call.
    #[derive(Debug)]
    pub struct SceneRenderer {
        /// The device. Public so the renderer that holds this one can draw the UI on it while it
        /// reads the preview spaces.
        pub gpu: Gpu,
        scene: Option<FullScreenQuad>,
        /// The scene's drawing half. Its world state is the application's, and
        /// every method here that reads the world is handed it.
        world: Option<crate::world::SceneDraw>,
        /// Set while the teleport tunnel is up. [`SceneRenderer::draw_scene`] returns early
        /// while it is set.
        world_hidden: bool,
        /// The override view distance, in force while the view-distance override is
        /// set — the teleport animation's projection collapse. Applied around the world draw
        /// rather than left set, because the client's own global is only ever read from inside a
        /// draw.
        pub view_distance: Option<f32>,
        /// The creature-mode preview spaces this process holds, keyed by
        /// [`PreviewId`]. They live for the life of the device rather than of a screen, because
        /// the geometry and the textures behind them are the expensive part and the char-gen
        /// wizard is entered and left repeatedly.
        // ORDER-OK: a lookup by a fixed key; the draw order is the client shell's.
        pub previews: std::collections::BTreeMap<PreviewId, crate::preview::PreviewSpace>,
        /// The game field of view, which a space carrying the smartbox-FOV flag divides by its own
        /// aspect. It comes from the `Render.FieldOfView`
        /// preference; the preference system is not wired, so this is its 90-degree default.
        pub world_fov: f32,
        /// The overlay's textures, by the key their front end chose. See [`Self::overlay_upload`].
        // ORDER-OK: a lookup by key; nothing iterates it in an order anyone sees.
        overlay_textures: std::collections::BTreeMap<OverlayTexture, TextureSlot>,
        /// The overlay's three pipeline states, image / invert / text, built on first use.
        overlay_pipelines: Option<[PipelineKey; 3]>,
    }

    /// A decoded retail surface and the six vertices that stretch it over the back buffer.
    #[derive(Debug)]
    struct FullScreenQuad {
        texture: TextureSlot,
        vertices: Vec<u8>,
        key: PipelineKey,
        alpha_ref: u8,
    }

    impl SceneRenderer {
        /// Construct the renderer and its graphics device.
        ///
        /// `window` is `None` for the headless path, which renders to an offscreen texture; that
        /// is what makes a headless run possible without a display.
        ///
        /// The backend is the default (Vulkan; in a test build, the one the test-only
        /// `DERETH_TEST_RENDERER` names). [`Self::new_on`] is the form the client uses once it
        /// has read `--renderer` and the `Renderer=` preference.
        ///
        /// # Errors
        /// [`RenderError`] when the selected backend can create no device.
        pub fn new(
            window: Option<WindowHandles>,
            width: u32,
            height: u32,
        ) -> Result<Self, RenderError> {
            Self::from_device(Gpu::new(
                window,
                &Self::device_config(window, width, height),
            )?)
        }

        /// The same, on a named backend. A backend this build does not have
        /// is an error, which is what lets the caller log its fallback line.
        ///
        /// # Errors
        /// [`RenderError`] when `backend` is not in this build, or can create no device.
        pub fn new_on(
            backend: dereth_render::device::Backend,
            window: Option<WindowHandles>,
            width: u32,
            height: u32,
        ) -> Result<Self, RenderError> {
            Self::from_device(Gpu::new_on(
                backend,
                window,
                &Self::device_config(window, width, height),
            )?)
        }

        /// The render device configuration as this client fills it. The same for either backend.
        fn device_config(window: Option<WindowHandles>, width: u32, height: u32) -> DeviceConfig {
            DeviceConfig {
                width,
                height,
                // The capture backend prefers a software rasteriser "so its output does not depend
                // on the machine's GPU", which is precisely what "byte-identical across three runs"
                // needs.
                force_software: window.is_none(),
                ..DeviceConfig::default()
            }
        }

        fn from_device(gpu: Gpu) -> Result<Self, RenderError> {
            Ok(Self {
                gpu,
                scene: None,
                world: None,
                world_hidden: false,
                view_distance: None,
                previews: std::collections::BTreeMap::new(),
                world_fov: dereth_render::camera::DEFAULT_FOV_DEGREES
                    * dereth_render::camera::DEG_TO_RAD,
                overlay_textures: std::collections::BTreeMap::new(),
                overlay_pipelines: None,
            })
        }

        /// Build the static scene, textures and all.
        ///
        /// Everything is created **before** the first frame, deliberately: `Gpu::upload_texture`
        /// resets and closes the frame command list, so a texture created between `begin_frame` and
        /// `end_frame` would discard the frame's recorded commands. That is also why the dynamic
        /// upload arena is reserved here rather than grown on demand — see
        /// [`crate::world::WorldScene::upload_reservation`].
        ///
        /// # Errors
        /// [`crate::world::WorldError`] when the region, the landblock or a device resource is
        /// unavailable.
        pub fn load_world(
            &mut self,
            store: &std::sync::Arc<dereth_dat::RetailDatStore>,
            cfg: crate::world::SceneConfig,
            world: &mut Option<dereth_client_runtime::world_state::WorldState>,
        ) -> Result<(), crate::world::WorldError> {
            let (mut scene, mut ws) = crate::world::SceneDraw::load(store, &mut self.gpu, cfg)?;
            if cfg.character {
                // The region is re-read rather than threaded through `load`: it is one
                // record, the data cache memoises it in the client, and keeping `load`'s signature is
                // worth more than the read. It is the world's own region.
                let region = crate::world::world_region(store)?;
                scene.attach_character(&mut ws, store, &region, &mut self.gpu)?;
            }
            scene
                .reserve_upload_arena(&mut self.gpu)
                .map_err(|e| crate::world::WorldError::Render(e.to_string()))?;
            self.scene = None;
            self.world = Some(scene);
            *world = Some(ws);
            Ok(())
        }

        /// Give the server's objects geometry and their new frames.
        ///
        /// Called once per frame, **outside** `begin_frame`/`end_frame`, for the same reason
        /// [`SceneRenderer::load_world`] is: creating a texture runs a command list of its own.
        ///
        /// # Errors
        /// [`crate::world::WorldError`] when a device resource cannot be created.
        pub fn sync_objects(
            &mut self,
            store: &std::sync::Arc<dereth_dat::RetailDatStore>,
            stream: &mut dereth_client_runtime::objects::ObjectStream,
            ws: Option<&mut dereth_client_runtime::world_state::WorldState>,
        ) -> Result<(), crate::world::WorldError> {
            let (Some(world), Some(ws)) = (self.world.as_mut(), ws) else {
                return Ok(());
            };
            world.sync_objects(ws, store, &mut self.gpu, stream)
        }

        /// Install the 3D viewport's rectangle on the scene, if there is one.
        ///
        /// The viewport handler uses the region's x, y, width, and height with depth disabled;
        /// this is the same call, deferred to the scene because in this build the viewport reaches
        /// the device inside [`crate::world::WorldScene::draw`]'s own bracket rather than as
        /// standing device state.
        ///
        /// A no-op with no world loaded, which is the char-gen and intro case: those screens draw
        /// their own viewports through the client shell's preview queue.
        pub fn set_game_viewport(&mut self, viewport: Option<dereth_render::camera::Viewport>) {
            if let Some(world) = self.world.as_mut() {
                world.set_game_viewport(viewport);
            }
        }

        /// The rectangle the scene **actually draws into**, or `None` with no world loaded.
        ///
        /// `WorldScene::effective_viewport`, not the stored field: this answers the window for a
        /// build that took the rectangle and then did not use it.
        #[must_use]
        pub fn game_viewport(
            &self,
            ws: Option<&dereth_client_runtime::world_state::WorldState>,
        ) -> Option<dereth_render::camera::Viewport> {
            let (w, h) = self.size();
            let (world, ws) = (self.world.as_ref()?, ws?);
            Some(world.effective_viewport(ws, w, h))
        }

        /// Run the shared create/appearance prefix before App dispatches accepted player motion.
        /// This never consumes movement snapshots. Like full sync, it must be outside a frame.
        pub fn prepare_object_dispatch(
            &mut self,
            store: &std::sync::Arc<dereth_dat::RetailDatStore>,
            stream: &mut dereth_client_runtime::objects::ObjectStream,
            ws: Option<&mut dereth_client_runtime::world_state::WorldState>,
        ) -> Result<(), crate::world::WorldError> {
            let (Some(world), Some(ws)) = (self.world.as_mut(), ws) else {
                return Ok(());
            };
            world.prepare_object_dispatch(ws, store, &mut self.gpu, stream)
        }

        /// Fetch and generate the landblocks the window scrolled onto.
        ///
        /// Called once per frame, **outside** `begin_frame`/`end_frame`, for the same reason
        /// [`SceneRenderer::sync_objects`] is. It is a no-op on every frame that did not cross a
        /// landblock boundary.
        ///
        /// # Errors
        /// [`crate::world::WorldError`] when a device resource cannot be created.
        pub fn stream_world(
            &mut self,
            store: &dereth_dat::RetailDatStore,
            ws: Option<&mut dereth_client_runtime::world_state::WorldState>,
        ) -> Result<(), crate::world::WorldError> {
            let (Some(world), Some(ws)) = (self.world.as_mut(), ws) else {
                return Ok(());
            };
            world.stream(ws, store, &mut self.gpu)
        }

        /// The per-frame poll over the scene-owned `Render.*` preferences. A no-op on every frame on which none of them
        /// moved, which is every frame but the one after an options-page Apply.
        ///
        /// # Errors
        /// [`crate::world::WorldError`] when the rebuild a changed preference asks for fails.
        pub fn update_render_preferences(
            &mut self,
            store: &dereth_dat::RetailDatStore,
            ws: Option<&mut dereth_client_runtime::world_state::WorldState>,
        ) -> Result<crate::world::RenderPrefWork, crate::world::WorldError> {
            let (Some(world), Some(ws)) = (self.world.as_mut(), ws) else {
                return Ok(crate::world::RenderPrefWork::default());
            };
            world.update_from_preferences(ws, store, &mut self.gpu)
        }

        /// The renderer's half of the smart-box reset on a second login.
        ///
        /// Retail's cell-manager reset: if a current cell is held, it releases that cell, clears
        /// the pointer, and flushes the interior cells. It releases the whole landscape when the
        /// load position is an outdoor cell or the held cell had been seen from outside. It then
        /// flushes the ambient sound tables.
        ///
        /// Here the landblock window, the resident blocks, the interior cells and the body are all
        /// [`crate::world::WorldScene`], so the whole scene is what goes. **The textures are
        /// handed back first** through [`crate::world::WorldScene::release_textures`]: more than
        /// four scenes on one device exhaust the descriptor heap, so a second login that simply
        /// built a second scene would leak a whole world of descriptors.
        ///
        /// Returns the number of texture slots handed back, so a test can tell a teardown that ran
        /// from one that returned.
        pub fn release_world(
            &mut self,
            ws: &mut Option<dereth_client_runtime::world_state::WorldState>,
        ) -> u32 {
            let Some(mut world) = self.world.take() else {
                return 0;
            };
            *ws = None;
            world.release_textures(&mut self.gpu)
        }

        /// The scene, for the camera.
        pub fn world_mut(&mut self) -> Option<&mut crate::world::SceneDraw> {
            self.world.as_mut()
        }

        /// The scene, for the startup log line and the tests.
        #[must_use]
        pub fn world(&self) -> Option<&crate::world::SceneDraw> {
            self.world.as_ref()
        }

        /// The same disjoint pair, with the scene **mutable**: the `Render.*` preferences
        /// whose owner is [`crate::world::SceneConfig::render`] live on the scene, and the device's
        /// two (`Render.TextureFiltering`, `Render.ScreenBrightness`) live on the `Gpu`, so one
        /// options-page Apply touches both.
        pub fn world_mut_and_gpu(&mut self) -> (Option<&mut crate::world::SceneDraw>, &mut Gpu) {
            (self.world.as_mut(), &mut self.gpu)
        }

        /// Which adapter the device came up on, for the startup log line.
        #[must_use]
        pub fn adapter(&self) -> String {
            format!("{:?}", self.gpu.adapter_kind())
        }

        /// Whether the device is a software rasteriser (WARP, lavapipe) rather than a GPU.
        #[must_use]
        pub fn software(&self) -> bool {
            self.gpu.adapter_kind() == dereth_render::device::AdapterKind::Software
        }

        /// The adapter's own name, for the same line.
        #[must_use]
        pub fn adapter_name(&self) -> String {
            format!("{} ({})", self.gpu.adapter_name(), self.adapter())
        }

        /// Which backend this device is.
        #[must_use]
        pub fn backend(&self) -> dereth_render::device::Backend {
            self.gpu.backend()
        }

        /// Decode one retail surface and build the full-screen quad that shows it.
        ///
        /// # Errors
        /// [`RenderError::Device`] when the object is missing or will not decode, which is a
        /// startup failure rather than something to draw around.
        pub fn load_first_pixel_scene(
            &mut self,
            assets: &dyn AssetSource,
            id: DataId,
        ) -> Result<(), RenderError> {
            let bytes = assets
                .read(id)
                .map_err(|e| RenderError::Device(format!("reading {id:?}: {e}")))?;
            let rs = RenderSurface::decode_payload_in(assets.container_era_of(id), id, &bytes)
                .map_err(|e| RenderError::Device(format!("decoding {id:?}: {e}")))?;
            let payload = rs
                .payload(&bytes)
                .ok_or_else(|| RenderError::Device(format!("{id:?} has a truncated payload")))?;

            let format = PixelFormatId::from_raw(rs.format);
            let source = match format {
                PixelFormatId::Dxt1
                | PixelFormatId::Dxt2
                | PixelFormatId::Dxt3
                | PixelFormatId::Dxt4
                | PixelFormatId::Dxt5 => SourcePixels::Dxt {
                    format,
                    blocks: payload,
                },
                // The surface-format choice's palettised arms need a Palette, which is a
                // second dat object and a second decision about *which* palette; the first-pixel
                // quad deliberately uses a surface that needs neither.
                other => SourcePixels::Direct {
                    format: other,
                    bits: payload,
                    pitch: 0,
                },
            };
            let decoded = decode_surface(source, rs.width, rs.height)?;
            let texture = self.gpu.upload_texture(&decoded)?;

            let (w, h) = self.gpu.size();
            // The client's own four pixel rules, over the whole back buffer...
            let rect = ui::update_transform(0, 0, w, h, (w, h), (w, h));
            // ...then the renderer's D3D12 pixel-centre compensation, once and only once. Applying
            // it twice is as wrong as not at all.
            let rect = ui::compensate_for_d3d12(rect, (w, h));

            // FVF 0x142 -- float3 position, D3DCOLOR diffuse, float2 uv, 24 bytes.
            let mut vertices: Vec<u8> = Vec::with_capacity(6 * 24);
            let (l, r, b, t) = (rect.x, rect.right(), rect.y, rect.top());
            for (x, y, u, v) in [
                (l, t, 0.0f32, 0.0f32),
                (l, b, 0.0, 1.0),
                (r, b, 1.0, 1.0),
                (r, b, 1.0, 1.0),
                (r, t, 1.0, 0.0),
                (l, t, 0.0, 0.0),
            ] {
                vertices.extend_from_slice(&x.to_le_bytes());
                vertices.extend_from_slice(&y.to_le_bytes());
                vertices.extend_from_slice(&0.5f32.to_le_bytes());
                vertices.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
                vertices.extend_from_slice(&u.to_le_bytes());
                vertices.extend_from_slice(&v.to_le_bytes());
            }

            // Row 1 of the catalogue: an opaque textured surface.
            let surf = Surface {
                r#type: st::BASE1_IMAGE,
                ..Surface::default()
            };
            let ctx = SurfaceContext {
                vertex_format: VertexFormat::XyzDiffuseTex1,
                ..SurfaceContext::default()
            };
            let (mut key, alpha_ref) = PipelineKey::from_surface(&surf, ctx);
            // The vertices are already in clip space, so there is nothing to depth-test against.
            key.z_func = ZFunc::Always;
            key.z_write = false;

            self.scene = Some(FullScreenQuad {
                texture,
                vertices,
                key,
                alpha_ref,
            });
            Ok(())
        }

        /// True once [`SceneRenderer::load_first_pixel_scene`] has succeeded.
        #[must_use]
        pub fn has_scene(&self) -> bool {
            self.scene.is_some()
        }

        /// Prepare the graphics device for the lost-device check and any changed render preference.
        ///
        /// D3D12 has no `TestCooperativeLevel`: a device is lost through
        /// `GetDeviceRemovedReason`, and the reset path is `Gpu::resize`. Nothing here yet; the
        /// step exists so the frame order is the documented one.
        pub fn prepare_graphics_device(&mut self) {}

        /// Live Render.TextureFiltering owner shared by world, UI material requests and previews.
        pub fn set_texture_filtering(&mut self, preference: u32) {
            self.gpu.set_texture_filtering(preference);
        }

        #[must_use]
        pub fn texture_filtering(&self) -> u32 {
            self.gpu.texture_filtering()
        }

        /// Live `Render.ScreenBrightness` owner, whose clamp to `[-0.2, 1.0]` is
        /// `Gpu::set_gamma`'s.
        pub fn set_gamma(&mut self, brightness: f32) {
            self.gpu.set_gamma(brightness);
        }

        /// The current gamma value after clamping.
        #[must_use]
        pub fn gamma(&self) -> f32 {
            self.gpu.gamma()
        }

        /// Apply the presentation's logical full-screen/vsync pair to the next swap-chain flip.
        pub fn set_presentation_sync(&mut self, full_screen: bool, sync_to_refresh: bool) {
            self.gpu.set_presentation_sync(full_screen, sync_to_refresh);
        }

        /// The interval configured for the next real swap-chain presentation.
        #[must_use]
        pub fn present_sync_interval(&self) -> u32 {
            self.gpu.present_sync_interval()
        }

        /// The interval handed to the most recent real swap-chain presentation, or `None` for the
        /// headless offscreen target.
        #[must_use]
        pub fn last_present_sync_interval(&self) -> Option<u32> {
            self.gpu.last_present_sync_interval()
        }

        /// Beginning the client frame increments the scene timestamp, clears black with depth 1.0,
        /// `BeginScene()`.
        ///
        /// # Errors
        /// Any failure from the runtime.
        pub fn start_frame(&mut self) -> Result<(), RenderError> {
            self.gpu.begin_frame()
        }

        /// Whether the world is hidden and the override view distance, pushed in once a frame by
        /// [`dereth_client_runtime::teleport::Teleport`].
        pub fn set_world_view_state(&mut self, hidden: bool, view_distance: Option<f32>) {
            self.world_hidden = hidden;
            self.view_distance = view_distance;
        }

        /// Whether the world is currently hidden, so a test can assert on it.
        #[must_use]
        pub const fn world_hidden(&self) -> bool {
            self.world_hidden
        }

        // ---- the creature-mode preview spaces ---------------------------------------------

        /// Create and initialize one named preview space. Returns true when this call created it.
        ///
        /// The client makes one per `Viewport` at layout time and destroys it with the
        /// element; here they outlive the screen, because the meshes and textures behind them are
        /// what cost and the char-gen wizard is entered and left repeatedly.
        pub fn ensure_preview(
            &mut self,
            id: PreviewId,
            assets: &std::sync::Arc<dereth_client_runtime::anim_assets::DatAnimAssets>,
        ) -> bool {
            if self.previews.contains_key(&id) {
                return false;
            }
            self.previews.insert(
                id,
                crate::preview::PreviewSpace::new(std::sync::Arc::clone(assets)),
            );
            true
        }

        /// The space, for a consumer that wants to move its camera or read its frame counter.
        #[must_use]
        pub fn preview(&self, id: PreviewId) -> Option<&crate::preview::PreviewSpace> {
            self.previews.get(&id)
        }

        /// See [`SceneRenderer::preview`].
        pub fn preview_mut(&mut self, id: PreviewId) -> Option<&mut crate::preview::PreviewSpace> {
            self.previews.get_mut(&id)
        }

        /// Add an object to a preview space. Loading its setup needs the device, which is why this
        /// method lives here rather than on the space.
        ///
        /// Call it **outside** `begin_frame`/`end_frame`, with the rest of the client shell's UI preparation:
        /// `Gpu::upload_texture` runs a command list of its own.
        ///
        /// # Errors
        /// [`RenderError`] when a device resource cannot be created.
        pub fn add_preview_object(
            &mut self,
            id: PreviewId,
            store: &dereth_dat::RetailDatStore,
            setup: DataId,
        ) -> Result<Option<usize>, RenderError> {
            match self.previews.get_mut(&id) {
                Some(space) => space.add_object(store, &mut self.gpu, setup),
                None => Ok(None),
            }
        }

        /// Add an object and dress it from the defaults —
        /// [`crate::preview::PreviewSpace::add_object_dressed`].
        ///
        /// Same device rules as [`SceneRenderer::add_preview_object`]: outside the frame bracket.
        ///
        /// # Errors
        /// [`RenderError`] when a device resource cannot be created.
        pub fn add_preview_object_dressed(
            &mut self,
            id: PreviewId,
            store: &dereth_dat::RetailDatStore,
            setup: DataId,
            objdesc: Option<&dereth_animation::parts::ObjDesc>,
        ) -> Result<Option<usize>, RenderError> {
            match self.previews.get_mut(&id) {
                Some(space) => space.add_object_dressed(store, &mut self.gpu, setup, objdesc),
                None => Ok(None),
            }
        }

        /// How many objects the identify window's portrait space holds.
        ///
        /// The examine preview draws nothing at all when
        /// the space has no first object, so this being zero is the whole of "the panel stays as
        /// the UI left it" — which is what an appraised *item* must get.
        #[must_use]
        pub fn examine_preview_objects(&self) -> usize {
            self.previews
                .get(&PreviewId::Examine)
                .map_or(0, crate::preview::PreviewSpace::object_count)
        }

        /// Clear the identify window's preview space, so a test
        /// can take the model away and prove its instrument still reads zero.
        pub fn clear_examine_preview(&mut self) {
            if let Some(s) = self.previews.get_mut(&PreviewId::Examine) {
                s.remove_all_objects();
            }
        }

        /// The selection callback uses the same view-distance override as the normal world render,
        /// including its visible world-fade frames. No object projection during a hidden world.
        pub fn target_projection(
            &self,
            id: dereth_primitives::ObjectId,
            ws: Option<&dereth_client_runtime::world_state::WorldState>,
        ) -> Option<dereth_client_contract::target::Projection> {
            if self.world_hidden {
                return None;
            }
            let world = self.world.as_ref()?;
            let ws = ws?;
            dereth_render::camera::view_distance_override::with(self.view_distance, || {
                world.target_projection(ws, id, self.size())
            })
        }

        /// Draw the world scene when present and visible.
        ///
        /// # Errors
        /// Any failure from the runtime.
        pub fn draw_scene(
            &mut self,
            ws: Option<&dereth_client_runtime::world_state::WorldState>,
        ) -> Result<(), RenderError> {
            // Retail draws the world only while it is not hidden, and does nothing else, so the
            // teleport tunnel really does stop the 3D world being drawn — the UI overlay still
            // composites over the cleared frame. This is the smart box's own hiding, not a UI
            // element's hide flag: no element is involved.
            if self.world_hidden {
                return Ok(());
            }
            if let (Some(world), Some(ws)) = (&self.world, ws) {
                // The view-distance override flag selects
                // the override FOV distance over the normal FOV for the whole of the
                // world and sky passes.
                let gpu = &mut self.gpu;
                return dereth_render::camera::view_distance_override::with(
                    self.view_distance,
                    || world.draw(ws, gpu),
                );
            }
            let Some(scene) = &self.scene else {
                return Ok(());
            };
            // Sampler 1 = linear / clamp. The client's material asks for LINEAR filtering
            // with a linear sampler; clamp because this quad's UVs are exactly [0, 1]
            // and a wrap sampler would fetch across the seam at the far edge.
            self.gpu.bind_texture(scene.texture, 1);
            let view = ViewParams::default();
            let mut per_frame = PerFrameConstants::from_view(&view);
            per_frame.view_proj = glam_identity();
            per_frame.view = glam_identity();
            self.gpu.draw_dynamic(
                &scene.key,
                &DrawConstants {
                    alpha_ref: scene.alpha_ref,
                    ..DrawConstants::default()
                },
                &per_frame,
                &PerDrawConstants::identity(),
                &scene.vertices,
            )
        }

        /// Descriptor slots live, ever taken and available, against the heap's capacity.
        #[must_use]
        pub fn descriptor_usage(&self) -> dereth_render::device::DescriptorUsage {
            self.gpu.descriptor_usage()
        }

        /// The allocator's counters. `exhaustions` must be zero.
        #[must_use]
        pub fn descriptor_stats(&self) -> dereth_render::descriptor::DescriptorStats {
            self.gpu.descriptor_stats()
        }

        /// The texture table's counters, including the `cross_space_payload_collisions` census.
        #[must_use]
        pub fn texture_table_stats(&self) -> dereth_render::TextureTableStats {
            self.gpu.texture_table_stats()
        }

        /// Every live keyed texture on this device, sorted by space and payload.
        #[must_use]
        pub fn texture_keys(&self) -> Vec<dereth_render::TextureKey> {
            self.gpu.texture_keys()
        }

        /// How many draws bound each of the device's four samplers, so a test can
        /// assert *which* sampler this file chose rather than read the choice off the call site.
        /// Index 3 is `POINT`/`CLAMP`, which is the only one the glyph draw may use.
        #[must_use]
        pub fn sampler_binds(&self) -> [u64; ui::pixel_rules::UI_SAMPLER_COUNT] {
            self.gpu.sampler_binds()
        }

        /// Zero [`Self::sampler_binds`], so one frame's binds can be counted alone.
        pub fn clear_sampler_binds(&self) {
            self.gpu.clear_sampler_binds();
        }

        /// How many draws put a *detail* texture in texture stage 1, i.e. how many
        /// detail-surface bindings to stage 1 made by the frame. The census the
        /// detail-texture pass is measured by.
        #[must_use]
        pub fn stage1_binds(&self) -> u64 {
            self.gpu.stage1_binds()
        }

        /// Zero [`Self::stage1_binds`], so one frame's detail binds can be counted alone.
        pub fn clear_stage1_binds(&self) {
            self.gpu.clear_stage1_binds();
        }

        /// End the frame with the UI overlay, `EndScene`, and `Present`.
        ///
        /// # Errors
        /// Any failure from the runtime.
        pub fn end_frame(&mut self) -> Result<(), RenderError> {
            self.gpu.end_frame()
        }

        /// Read the last presented frame back and write it as a PNG.
        ///
        /// # Errors
        /// [`RenderError::Device`] for a read-back or an I/O failure.
        pub fn capture_png(&mut self, path: &Path) -> Result<(), RenderError> {
            let image = self.gpu.capture()?;
            let rgba = image.to_rgba();
            let io = |e: std::io::Error| RenderError::Device(format!("{}: {e}", path.display()));
            let file = std::fs::File::create(path).map_err(io)?;
            let mut enc =
                png::Encoder::new(std::io::BufWriter::new(file), image.width, image.height);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            // No ancillary chunks are written, so two runs over identical pixels produce identical
            // bytes -- which is what the acceptance gate measures.
            let mut w = enc
                .write_header()
                .map_err(|e| RenderError::Device(format!("png header: {e}")))?;
            w.write_image_data(&rgba)
                .map_err(|e| RenderError::Device(format!("png data: {e}")))?;
            Ok(())
        }

        /// The last presented frame's raw BGRA pixels, for a test that must compare two frames.
        ///
        /// # Errors
        /// Any failure from the runtime.
        pub fn capture_bgra(&mut self) -> Result<(u32, u32, Vec<u8>), RenderError> {
            let image = self.gpu.capture()?;
            Ok((image.width, image.height, image.bgra.clone()))
        }

        /// How many non-black pixels the last frame produced, for the startup log line and for the
        /// windowed hand check.
        ///
        /// # Errors
        /// Any failure from the runtime.
        pub fn lit_pixel_count(&mut self) -> Result<usize, RenderError> {
            let image = self.gpu.capture()?;
            Ok(image
                .bgra
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[0] != 0 || p[1] != 0 || p[2] != 0)
                .count())
        }

        /// The back buffer's extent.
        #[must_use]
        pub fn size(&self) -> (u32, u32) {
            self.gpu.size()
        }

        /// Resize the swap chain and rebuild size-dependent graphics state.
        ///
        /// This is [`dereth_render::device::Gpu::resize`], the device reset path;
        /// the client shell's `App::change_presentation` is the caller.
        ///
        /// # Errors
        /// Any failure from the runtime.
        pub fn resize(&mut self, width: u32, height: u32) -> Result<(), RenderError> {
            self.gpu.resize(width, height)
        }

        /// Wait for the GPU before dropping any device resource.
        ///
        /// # Errors
        /// Any failure from the runtime.
        pub fn wait_idle(&mut self) -> Result<(), RenderError> {
            self.gpu.wait_idle()
        }
    }

    /// The overlay: any UI's textures and its drawing list. See `dereth_client_contract::overlay`.
    impl SceneRenderer {
        /// Upload an overlay texture, outside the frame bracket. An `Image` or `Glyphs` key is
        /// shared through the device's keyed table, so a resident key is a hit; a `Local` key is
        /// its own, so a resident one is released first and replaced.
        ///
        /// # Errors
        /// The device's own.
        pub fn overlay_upload(
            &mut self,
            texture: OverlayTexture,
            data: &dereth_primitives::TextureData,
        ) -> Result<(), RenderError> {
            let slot = match texture.space {
                OverlaySpace::Image | OverlaySpace::Glyphs => {
                    if self.overlay_textures.contains_key(&texture) {
                        return Ok(());
                    }
                    let key = if texture.space == OverlaySpace::Image {
                        dereth_render::TextureKey::ui(texture.key)
                    } else {
                        dereth_render::TextureKey::font_sheet(texture.key)
                    };
                    self.gpu.upload_texture_keyed(key, data)?
                }
                OverlaySpace::Local => {
                    if let Some(old) = self.overlay_textures.remove(&texture) {
                        let _ = self.gpu.release_texture(old);
                    }
                    self.gpu.upload_texture(data)?
                }
            };
            self.overlay_textures.insert(texture, slot);
            Ok(())
        }

        /// The device slot an overlay texture is resident in, for a test that reads it back.
        #[must_use]
        pub fn overlay_slot(&self, texture: OverlayTexture) -> Option<TextureSlot> {
            self.overlay_textures.get(&texture).copied()
        }

        /// Release an overlay texture.
        pub fn overlay_release(&mut self, texture: OverlayTexture) -> OverlayReleased {
            match self.overlay_textures.remove(&texture) {
                None => OverlayReleased::Absent,
                Some(slot) => match self.gpu.release_texture(slot) {
                    dereth_render::descriptor::Released::Freed => OverlayReleased::Freed,
                    dereth_render::descriptor::Released::StillLinked(_) => {
                        OverlayReleased::StillLinked
                    }
                    dereth_render::descriptor::Released::Unknown => OverlayReleased::Unknown,
                },
            }
        }

        /// The UI's three pipeline states. **Image**: row 1 of the catalogue -- an opaque
        /// textured surface -- with alpha blending on and the depth test off, matching the UI
        /// renderer's own state. **Invert**: the same state with one substitution, a white source
        /// under `INVDESTCOLOR`/`ZERO`, which leaves `1 - dest` in every colour channel.
        /// **Text**: the texture-font material, whose colour op is `TEXOP_SELECTARG2` with arg2
        /// `DIFFUSE`, so the colour comes entirely from the vertex and the sheet contributes only
        /// coverage.
        fn overlay_pipelines(&mut self) -> [PipelineKey; 3] {
            if let Some(p) = self.overlay_pipelines {
                return p;
            }
            let surf = Surface {
                r#type: st::BASE1_IMAGE,
                ..Surface::default()
            };
            let ctx = SurfaceContext {
                vertex_format: VertexFormat::XyzDiffuseTex1,
                ..SurfaceContext::default()
            };
            let (mut image, _) = PipelineKey::from_surface(&surf, ctx);
            image.z_func = ZFunc::Always;
            image.z_write = false;
            image.alpha_blend = true;
            image.src_blend = dereth_render::pso::Blend::SrcAlpha;
            image.dst_blend = dereth_render::pso::Blend::InvSrcAlpha;
            let mut invert = image;
            invert.src_blend = dereth_render::pso::Blend::InvDestColor;
            invert.dst_blend = dereth_render::pso::Blend::Zero;
            let text = dereth_render::font::TextBatch::new().pipeline_key();
            let p = [image, invert, text];
            self.overlay_pipelines = Some(p);
            p
        }

        /// Draw the overlay over the finished world, in order, inside the frame bracket. A batch
        /// whose texture is not resident draws nothing; a preview space that does not exist or
        /// holds no object draws nothing.
        ///
        /// # Errors
        /// The device's own; the rest of the list is not drawn.
        pub fn draw_overlay(&mut self, items: &[OverlayItem]) -> Result<(), RenderError> {
            if items.is_empty() {
                return Ok(());
            }
            let [image, invert, text] = self.overlay_pipelines();
            let view = ViewParams::default();
            let mut per_frame = PerFrameConstants::from_view(&view);
            per_frame.view_proj = glam_identity();
            per_frame.view = glam_identity();
            for item in items {
                match item {
                    OverlayItem::Triangles {
                        material,
                        texture,
                        sampler,
                        vertices,
                    } => {
                        let Some(slot) = self.overlay_textures.get(texture).copied() else {
                            continue;
                        };
                        let key = match material {
                            OverlayMaterial::Image => image,
                            OverlayMaterial::Invert => invert,
                            OverlayMaterial::Text => text,
                        };
                        self.gpu.bind_texture(slot, overlay_sampler_index(*sampler));
                        self.gpu.draw_dynamic(
                            &key,
                            &DrawConstants::default(),
                            &per_frame,
                            &PerDrawConstants::identity(),
                            &overlay_vertex_bytes(vertices),
                        )?;
                    }
                    OverlayItem::Preview { space, rect } => {
                        let Some(preview) = self.previews.get(space) else {
                            continue;
                        };
                        if preview.object_count() == 0 {
                            continue;
                        }
                        preview.draw(&mut self.gpu, *rect, self.view_distance, self.world_fov)?;
                    }
                }
            }
            Ok(())
        }
    }

    /// The device's sampler index for an overlay sampler: the UI's eight, POINT or LINEAR by
    /// each pair of address modes (`ui::pixel_rules::ui_surface_sampler_axes`' table).
    #[must_use]
    pub fn overlay_sampler_index(s: OverlaySampler) -> u32 {
        // Equal sizes and no rotation answer POINT; anything else answers LINEAR.
        let size = if s.point { (1, 1) } else { (1, 2) };
        ui::pixel_rules::ui_surface_sampler_axes((1, 1), size, false, (s.wrap_u, s.wrap_v))
    }

    #[cfg(test)]
    mod overlay_tests {
        use super::*;

        /// Each of the UI's eight sampler indices is one overlay sampler: the filter and the two
        /// address modes it names come back as that index.
        #[test]
        fn every_ui_sampler_is_exactly_one_overlay_sampler() {
            let mut seen = Vec::new();
            for point in [false, true] {
                for wrap_u in [false, true] {
                    for wrap_v in [false, true] {
                        let s = OverlaySampler {
                            point,
                            wrap_u,
                            wrap_v,
                        };
                        let i = overlay_sampler_index(s);
                        let (u, v) = ui::pixel_rules::ui_sampler_address_modes(i);
                        let wraps = |m: u32| m == ui::pixel_rules::TEXADDRESS_WRAP;
                        assert_eq!((wraps(u), wraps(v)), (wrap_u, wrap_v), "{s:?} -> {i}");
                        assert_eq!(
                            ui::pixel_rules::ui_sampler_filter(i)
                                == ui::pixel_rules::TEXFILTER_POINT,
                            point,
                            "{s:?} -> {i}"
                        );
                        seen.push(i);
                    }
                }
            }
            seen.sort_unstable();
            assert_eq!(seen, (0..8).collect::<Vec<u32>>());
            assert_eq!(
                overlay_sampler_index(OverlaySampler::POINT_CLAMP),
                ui::pixel_rules::UI_GLYPH_SAMPLER
            );
        }
    }

    /// The overlay's vertices as the 24-byte `XYZ | DIFFUSE | TEX1` records the device draws.
    #[must_use]
    pub fn overlay_vertex_bytes(vertices: &[OverlayVertex]) -> Vec<u8> {
        let mut out = Vec::with_capacity(vertices.len() * 24);
        for v in vertices {
            for c in v.position {
                out.extend_from_slice(&c.to_le_bytes());
            }
            out.extend_from_slice(&v.color.to_le_bytes());
            out.extend_from_slice(&v.uv[0].to_le_bytes());
            out.extend_from_slice(&v.uv[1].to_le_bytes());
        }
        out
    }

    /// The identity matrix in the column-major array `PerFrameConstants` holds. The quad's vertices
    /// are already in clip space, exactly as `D3DFVF_XYZRHW` geometry would be.
    #[must_use]
    pub fn glam_identity() -> [f32; 16] {
        [
            1.0, 0.0, 0.0, 0.0, //
            0.0, 1.0, 0.0, 0.0, //
            0.0, 0.0, 1.0, 0.0, //
            0.0, 0.0, 0.0, 1.0,
        ]
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use dereth_dat::RetailDatStore;

        fn warp(width: u32, height: u32) -> Option<SceneRenderer> {
            match SceneRenderer::new(None, width, height) {
                Ok(r) => Some(r),
                Err(e) => {
                    eprintln!("skipping: no D3D12 WARP device available ({e})");
                    None
                }
            }
        }

        // Oracle: the retail dat files. The surface is 0x0600378B, a 256x256 DXT1 landscape
        // texture; the assertion is that it covers the whole back buffer, which is contract 11.1's
        // four pixel rules measured rather than restated.
        #[test]
        #[cfg_attr(
            not(feature = "retail-dats"),
            ignore = "reads the retail dats: --features retail-dats"
        )]
        fn the_first_pixel_comes_from_a_retail_surface_and_fills_the_frame() {
            let dir = dereth_dat::testing::dat_dir();
            assert!(
                dereth_dat::testing::have_dats(),
                "the retail dats are this test's oracle and there are none at {} -- \
                 set DERETH_TEST_DAT_DIR",
                dir.display()
            );
            let Some(mut r) = warp(800, 600) else { return };
            let store = RetailDatStore::open_dir(&dir).expect("the retail dats open");
            r.load_first_pixel_scene(&store, DataId(FIRST_PIXEL_SURFACE))
                .expect("decodes");
            assert!(r.has_scene());

            r.prepare_graphics_device();
            r.start_frame().expect("begin");
            r.draw_scene(None).expect("draw");
            r.end_frame().expect("end");

            // The frame is cleared to black and the quad covers every pixel, so a lit count below
            // the full frame would mean the quad is short -- which is exactly the failure the
            // half-pixel compensation exists to prevent.
            let lit = r.lit_pixel_count().expect("capture");
            assert!(
                lit > 800 * 600 * 95 / 100,
                "the quad lit {lit} of 480000 pixels; a full-screen quad should light nearly all \
                 of them (a few texels of this texture are genuinely black)"
            );
        }

        // Oracle: the same capture, twice, in one process. The cross-process form of this is
        // tests/gpu/presentation/headless_capture_determinism.rs, which is the acceptance gate.
        #[test]
        fn two_captures_of_the_same_frame_are_identical() {
            let Some(mut r) = warp(64, 64) else { return };
            r.start_frame().expect("begin");
            r.end_frame().expect("end");
            let a = r.lit_pixel_count().expect("capture");
            let b = r.lit_pixel_count().expect("capture");
            assert_eq!(a, b);
            assert_eq!(a, 0, "starting the frame clears to black");
        }
    }
}
