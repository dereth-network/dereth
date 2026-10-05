//! The renderer: the scene's (`dereth_scene::gpu::SceneRenderer`) and the UI drawn over it.
//!
//! The device, the frame bracket, the drawn world and the preview spaces are the scene's; what
//! this module adds is the UI's half: its textures and font atlases, the overlay's draw, the movie
//! frame, and where each preview space is drawn this frame. `dereth_render::ui` does the work.
//!
//! **This module wires; it does not implement.** The texture decode below is
//! `dereth_assets::RenderSurface` + `dereth_render::texture::decode_surface`, the quad geometry is
//! `dereth_render::ui::update_transform`, and the half-pixel compensation is
//! `dereth_render::ui::compensate_for_d3d12` — **called exactly once, at the API boundary the
//! renderer chose for it**. Do not re-apply it here.

/// The retail surface the first-pixel quad draws, re-exported from
/// [`dereth_client_runtime::assets`] -- it is a dat id and nothing else -- so that
/// `dereth_client_runtime::assets::FIRST_PIXEL_SURFACE` resolves too.
pub use dereth_client_runtime::assets::FIRST_PIXEL_SURFACE;

/// The preview-space id and the UI-texture release report, re-exported from
/// [`crate::present`], whose trait names both.
pub use {
    crate::present::UiReleaseReport, dereth_client_contract::overlay::PreviewSpace as PreviewId,
};

/// The scene's half of the renderer, which [`Renderer`] holds and dereferences to.
#[cfg(gpu)]
pub use dereth_scene::gpu::SceneRenderer;

#[cfg(gpu)]
pub use imp::*;

#[cfg(gpu)]
#[path = "."]
mod imp {
    use std::path::Path;

    use dereth_primitives::{AssetSource, DataId};
    use dereth_render::device::WindowHandles;
    use dereth_render::{ui, RenderError};

    use super::{PreviewId, SceneRenderer, UiReleaseReport};
    use dereth_client_contract::overlay::{
        OverlayItem, OverlayMaterial, OverlayReleased, OverlaySampler, OverlayTexture,
    };

    /// The scene's renderer and the UI drawn over it.
    ///
    /// Both are held here rather than in [`crate::app::App`] so that the renderer's slot in the
    /// documented frame order stays a single call.
    #[derive(Debug)]
    pub struct Renderer {
        /// The device, the drawn world and the preview spaces. The renderer dereferences to it,
        /// so every method that is not the UI's is the scene's own.
        device: SceneRenderer,
        /// One SRV slot per distinct UI image, uploaded once and kept for the life of
        /// the process, exactly as the image-texture cache does. `None` records an image that would
        /// not resolve, so a broken id is looked up once rather than every frame.
        /// Keyed by `(image, operation)`: a spot recoloured by the colour-spot pass is a
        /// different texture from the same template drawn plain.
        /// Per UI image: its slot and the **pixel size actually uploaded**. The sampler choice
        /// needs the second half: it picks the sampler by
        /// comparing the element's virtual size with its surface's physical size, and without the
        /// physical size that condition cannot be evaluated here at all.
        #[allow(clippy::type_complexity)] // a one-off tuple, named where it is read
        ui_textures: std::collections::BTreeMap<
            crate::ui_draw::ImageKey,
            Option<(OverlayTexture, (u32, u32))>,
        >,
        /// One opaque white texel, so a flat-colour fill can go through the same
        /// `TEXOP_MODULATE(TEXTURE, diffuse)` pipeline as every other UI quad and come out as the
        /// vertex colour. A fill has no source image at all, and
        /// this is the shortest honest way to say that with a texturing pipeline.
        ui_white: Option<OverlayTexture>,
        /// One 256x256 `A8R8G8B8` atlas per distinct font DataID, plus its SRV slot --
        /// The reference client builds one per font too and keeps it for the life of the device. `None`
        /// records a font that would not load, so a broken id costs one failed read.
        /// Per font: its atlas, its foreground glyph texture, and the **outline** glyph texture, present for the 37 shipped fonts that have a background
        /// sheet and `None` for the 12 that take the eight-draw neighbourhood arm instead.
        ui_fonts: std::collections::BTreeMap<
            DataId,
            Option<(
                dereth_render::font::FontAtlas,
                OverlayTexture,
                Option<OverlayTexture>,
            )>,
        >,
        /// What [`Renderer::set_movie_frame`]'s per-frame releases have done. `unknown`
        /// is a double release and must stay zero over a whole 219-frame movie.
        ui_release: UiReleaseReport,
        pub ui_stats: crate::ui_draw::UiTextureStats,
        /// Which element each space occupies **this frame**, and where. Refilled every frame by
        /// [`Renderer::queue_preview`] and drained by [`Renderer::draw_ui`], because
        /// Preview drawing belongs to the element's own render, and an element that is not drawn
        /// has none.
        preview_draws: Vec<(
            dereth_ui::ElemHandle,
            PreviewId,
            dereth_render::camera::Viewport,
        )>,
        /// The spaces drawn over a subtree's pictures and under its text this frame
        /// ([`Renderer::queue_preview_under_text`]), drained with [`Self::preview_draws`].
        preview_lifts: Vec<(
            PreviewId,
            dereth_render::camera::Viewport,
            Vec<dereth_ui::ElemHandle>,
        )>,
    }

    impl std::ops::Deref for Renderer {
        type Target = SceneRenderer;
        fn deref(&self) -> &SceneRenderer {
            &self.device
        }
    }

    impl std::ops::DerefMut for Renderer {
        fn deref_mut(&mut self) -> &mut SceneRenderer {
            &mut self.device
        }
    }

    impl Renderer {
        /// Construct the renderer and its graphics device: [`SceneRenderer::new`], with no UI
        /// uploaded yet.
        ///
        /// # Errors
        /// [`RenderError`] when the selected backend can create no device.
        pub fn new(
            window: Option<WindowHandles>,
            width: u32,
            height: u32,
        ) -> Result<Self, RenderError> {
            Ok(Self::around(SceneRenderer::new(window, width, height)?))
        }

        /// The same, on a named backend: [`SceneRenderer::new_on`].
        ///
        /// # Errors
        /// [`RenderError`] when `backend` is not in this build, or can create no device.
        pub fn new_on(
            backend: dereth_render::device::Backend,
            window: Option<WindowHandles>,
            width: u32,
            height: u32,
        ) -> Result<Self, RenderError> {
            Ok(Self::around(SceneRenderer::new_on(
                backend, window, width, height,
            )?))
        }

        /// The renderer over `device`, with no UI uploaded yet.
        #[must_use]
        pub fn around(device: SceneRenderer) -> Self {
            Self {
                device,
                ui_textures: std::collections::BTreeMap::new(),
                ui_white: None,
                ui_fonts: std::collections::BTreeMap::new(),
                ui_release: UiReleaseReport::default(),
                ui_stats: crate::ui_draw::UiTextureStats::default(),
                preview_draws: Vec::new(),
                preview_lifts: Vec::new(),
            }
        }

        /// Say that `id`'s space occupies `who`'s rectangle this frame.
        ///
        /// Refilled every frame and drained by [`Renderer::draw_ui`], because preview drawing runs
        /// from the element's own `DrawSelf`: an element that is
        /// not in this frame's draw list has no preview pass either, which is precisely what
        /// happens when the wizard leaves the Appearance page.
        pub fn queue_preview(
            &mut self,
            id: PreviewId,
            who: dereth_ui::ElemHandle,
            rect: dereth_render::camera::Viewport,
        ) {
            self.preview_draws.push((who, id, rect));
        }

        /// Say that `id`'s space occupies `rect` this frame, drawn over the pictures of the
        /// elements of `subtree` and under their text.
        pub fn queue_preview_under_text(
            &mut self,
            id: PreviewId,
            subtree: Vec<dereth_ui::ElemHandle>,
            rect: dereth_render::camera::Viewport,
        ) {
            self.preview_lifts.push((id, rect, subtree));
        }

        /// Upload whatever images this frame's UI draw list needs.
        ///
        /// **Outside `begin_frame`/`end_frame`**, for the same reason [`SceneRenderer::sync_objects`]
        /// and [`SceneRenderer::stream_world`] are: `Gpu::upload_texture` runs a command list of its own
        /// and would discard the frame's recorded commands. It is called at step 7 of the frame,
        /// and is a no-op on every frame that introduced no new image.
        ///
        /// A texture that will not resolve is **remembered as absent**, so a broken id costs one
        /// failed lookup rather than one per frame, and is counted. The client blits a null
        /// missing image texture as nothing, and a silently invisible
        /// element is exactly the failure that hides inside a green test suite.
        pub fn prepare_ui(
            &mut self,
            store: &dereth_dat::RetailDatStore,
            cmds: &[dereth_ui::UiDrawCmd],
        ) {
            self.prepare_ui_from(store, store, cmds);
        }

        /// [`Self::prepare_ui`] with the interface's pictures and fonts read from `interface`
        /// and the pictures the world names ([`dereth_ui::ImageSource::World`], and every composed
        /// icon) from `world`. Beside an older world the two answer one id with different
        /// pictures: the interface's own art is the later files', the icons the world's.
        pub fn prepare_ui_from(
            &mut self,
            interface: &dereth_dat::RetailDatStore,
            world: &dereth_dat::RetailDatStore,
            cmds: &[dereth_ui::UiDrawCmd],
        ) {
            let store = interface;
            let chrome = dereth_scene::textures::TextureStore::new(interface);
            let content = dereth_scene::textures::TextureStore::new(world);
            for (id, op, source) in crate::ui_draw::images(cmds) {
                if self.ui_textures.contains_key(&(id, op, source)) {
                    continue;
                }
                // An icon composite is a function of up to five *other* surfaces, so it does not
                // start from `id`'s pixels the way a `ReplaceColor` or a `Multiply` does: it makes
                // its own 32x32 local surface. `id`
                // is still the recipe's base and still part of the cache key.
                let decoded =
                    if let Some(r) = op.and_then(dereth_ui::region::SurfaceOp::icon_recipe) {
                        crate::ui_draw::composite(r, &|d| content.icon_data(d).ok())
                            .ok_or(dereth_scene::textures::TextureError::NotATexture(id))
                    } else {
                        // The blit-and-recolour the colour-spot and gradient-disk passes do into
                        // their own local surface, done once per distinct pair rather than once
                        // per frame.
                        crate::ui_draw::derive_plain(&chrome, &content, id, op, source)
                    };
                let slot = match decoded {
                    // Keyed, so a second holder of the same image gets a cache hit and an `AddRef`
                    // rather than a second upload and a second descriptor slot, matching
                    // the original combined-texture cache. A UI image has no
                    // palette, so the palette half of the key is 0 and the key is the image id.
                    Ok(data) => match self
                        .device
                        .overlay_upload(crate::ui_draw::image_texture(id, op, source), &data)
                    {
                        Ok(()) => {
                            self.ui_stats.uploaded += 1;
                            Some((
                                crate::ui_draw::image_texture(id, op, source),
                                (data.width, data.height),
                            ))
                        }
                        Err(e) => {
                            tracing::warn!("UI texture {id:?} would not upload: {e}");
                            self.ui_stats.decode_failures += 1;
                            None
                        }
                    },
                    Err(e) => {
                        tracing::warn!("UI image {id:?} would not decode: {e}");
                        self.ui_stats.decode_failures += 1;
                        None
                    }
                };
                self.ui_textures.insert((id, op, source), slot);
            }
            self.prepare_ui_fonts(store, cmds);
            self.prepare_ui_white(cmds);
        }

        /// The one white texel the flat-colour fills sample. Uploaded on the first frame that has
        /// a visible fill, and then never again -- one SRV slot for the life of the process.
        ///
        /// **Both halves of the condition matter, and dropping either is a silent no-draw**: the
        /// selection quads sample the same texel (a white source is what makes
        /// `INVDESTCOLOR`/`ZERO` a pure complement), so with the test written against *fills*
        /// alone a frame whose only white-textured geometry is a selection would upload nothing,
        /// find `ui_white == None`, and `continue` — no error, no counter, and no highlight
        /// although the draw list carried it correctly. A resource gate that names only one of its
        /// two consumers fails as silence, not as an error.
        fn prepare_ui_white(&mut self, cmds: &[dereth_ui::UiDrawCmd]) {
            if self.ui_white.is_some()
                || !cmds
                    .iter()
                    .any(|c| c.fills.iter().any(|f| !f.is_invisible()) || !c.invert.is_empty())
            {
                return;
            }
            let data = dereth_primitives::TextureData {
                width: 1,
                height: 1,
                format: dereth_primitives::TextureFormat::Bgra8,
                levels: vec![vec![0xFF, 0xFF, 0xFF, 0xFF]],
            };
            match self
                .device
                .overlay_upload(crate::ui_draw::WHITE_TEXEL, &data)
            {
                Ok(()) => {
                    self.ui_stats.uploaded += 1;
                    self.ui_white = Some(crate::ui_draw::WHITE_TEXEL);
                }
                Err(e) => tracing::warn!("the UI fill texel would not upload: {e}"),
            }
        }

        /// Prepare an atlas for every font named by this frame's text.
        ///
        /// Outside the frame bracket for the same reason the images are, and once per font for the
        /// life of the process: an atlas is a device resource the client never rebuilds either
        /// (the end of text rendering re-bakes only when the texture is *lost*).
        fn prepare_ui_fonts(
            &mut self,
            store: &dereth_dat::RetailDatStore,
            cmds: &[dereth_ui::UiDrawCmd],
        ) {
            for did in crate::ui_draw::fonts(cmds) {
                if self.ui_fonts.contains_key(&did) {
                    continue;
                }
                let built = match crate::ui_draw::build_font_atlas(store, did) {
                    Ok(mut atlas) => {
                        // The glyph texture is the font's own foreground sheet, so its size is the
                        // sheet's -- not `ATLAS_SIZE`, which belongs to the debug overlay's bake.
                        // The pixels are *moved* into the upload: nothing reads them again, and a
                        // resident copy of a 2048x952 sheet per font is 8 MiB of nothing.
                        let data = dereth_primitives::TextureData {
                            width: atlas.texture_size.0,
                            height: atlas.texture_size.1,
                            format: dereth_primitives::TextureFormat::Bgra8,
                            levels: vec![std::mem::take(&mut atlas.pixels)],
                        };
                        // The outline sheet is a second texture with the same
                        // metrics, so it is taken out of the atlas the same way and uploaded
                        // under a distinct key. A font that has none simply has no outline pass.
                        let outline_data =
                            atlas
                                .take_outline_pixels()
                                .map(|px| dereth_primitives::TextureData {
                                    width: atlas.texture_size.0,
                                    height: atlas.texture_size.1,
                                    format: dereth_primitives::TextureFormat::Bgra8,
                                    levels: vec![px],
                                });
                        // The font sheets are their own key space. `(0, font DID)`
                        // is exactly the shape an unpalettised world texture keys on, and a
                        // font DID and a `RenderSurface` DID are two ids out of one dat.
                        let sheet = crate::ui_draw::glyph_texture(0, did);
                        match self.device.overlay_upload(sheet, &data) {
                            Ok(()) => {
                                self.ui_stats.fonts_baked += 1;
                                let outline_sheet = outline_data.and_then(|d| {
                                    let outline = crate::ui_draw::glyph_texture(1, did);
                                    match self.device.overlay_upload(outline, &d) {
                                        Ok(()) => {
                                            self.ui_stats.font_outlines_baked += 1;
                                            Some(outline)
                                        }
                                        Err(e) => {
                                            tracing::warn!(
                                                "font {did:?} outline texture would \
                                                 not upload: {e}"
                                            );
                                            self.ui_stats.font_failures += 1;
                                            None
                                        }
                                    }
                                });
                                Some((atlas, sheet, outline_sheet))
                            }
                            Err(e) => {
                                tracing::warn!("font atlas {did:?} would not upload: {e}");
                                self.ui_stats.font_failures += 1;
                                None
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("{e}");
                        self.ui_stats.font_failures += 1;
                        None
                    }
                };
                self.ui_fonts.insert(did, built);
            }
        }

        /// Ending the frame with `true` begins with the 2D UI overlay.
        ///
        /// Called **between the 3D scene and** `EndScene`/`Present`, which is where the client
        /// draws it: the UI is composited over a finished 3D frame with the depth test off.
        ///
        /// The draw list is lowered onto the presentation's overlay ([`Self::overlay_items`]) and
        /// the overlay is what the device draws, so the retail UI draws through the same call any
        /// other UI does.
        ///
        /// # Errors
        /// Any failure from the runtime.
        pub fn draw_ui(&mut self, cmds: &[dereth_ui::UiDrawCmd]) -> Result<(), RenderError> {
            let items = self.overlay_items(cmds);
            self.device.draw_overlay(&items)
        }

        /// The retail draw list as the overlay draws it: each command's own blit, its fills, the
        /// preview spaces it hosts, its glyphs (outline pass first) and its selection inverts, in
        /// that order, with the counters the report reads counted as each is lowered.
        pub fn overlay_items(&mut self, cmds: &[dereth_ui::UiDrawCmd]) -> Vec<OverlayItem> {
            // Taken rather than borrowed: draining here guarantees a queue entry for an element
            // that turned out not to be drawn is dropped rather than carried into the next frame.
            let previews = std::mem::take(&mut self.preview_draws);
            let lifts = std::mem::take(&mut self.preview_lifts);
            let mut drawn_previews: Vec<dereth_ui::ElemHandle> = Vec::new();
            let mut items = Vec::new();
            if cmds.is_empty() {
                return items;
            }
            // A lifted space is drawn after the last command of its subtree, and the subtree's
            // text is held back until then: `(space, rect, subtree, last command, held text)`.
            #[allow(clippy::type_complexity)] // a one-off tuple, named where it is read
            let mut lifted: Vec<(
                PreviewId,
                dereth_render::camera::Viewport,
                Vec<dereth_ui::ElemHandle>,
                usize,
                Vec<OverlayItem>,
            )> = Vec::new();
            for (id, rect, subtree) in lifts {
                match cmds.iter().rposition(|c| subtree.contains(&c.who)) {
                    Some(last) => lifted.push((id, rect, subtree, last, Vec::new())),
                    // A subtree that drew nothing has no text to go over the space: the space is
                    // drawn where an element with no command of its own is.
                    None => self.lower_preview(id, rect, &mut items),
                }
            }
            // The lifted space whose subtree the current command's text belongs to, and where
            // in `items` that text begins.
            let mut holding: Option<(usize, usize)> = None;
            // A preview viewport that is **transparent** and carries no glyphs and no fills
            // emits no `UiDrawCmd` at all (`UiSystem::draw_region`'s
            // `if !transparent || !glyphs.is_empty() || !fills.is_empty()`), and the char-gen
            // viewport is opaque, while a full-screen region with no art is transparent. The
            // client still renders the UI object attached to either element even when the element
            // itself has nothing to blit. So a queued space whose element produced no command is drawn **first**, which
            // is where that element sits in order: before its own children and
            // before every later sibling.
            for (_, id, rect) in previews
                .iter()
                .filter(|(w, _, _)| !cmds.iter().any(|c| c.who == *w))
            {
                self.lower_preview(*id, *rect, &mut items);
            }
            let fb = self.device.gpu.size();
            for (index, cmd) in cmds.iter().enumerate() {
                self.settle_lifts(&mut items, &mut lifted, &mut holding, index);
                // Step 6 `DrawSelf`: the element's own blit first, then the glyphs it composes over
                // it. Both belong to this element and both precede the next command, which is what
                // keeps the whole overlay in order.
                if let Some(id) = cmd.image {
                    match self
                        .ui_textures
                        .get(&(id, cmd.image_op, cmd.image_source))
                        .copied()
                    {
                        Some(Some((texture, physical))) => {
                            match crate::ui_draw::quad(cmd, fb, physical) {
                                Some(crate::ui_draw::UiQuad { vertices, wrap }) => {
                                    // The client's sampler choice is not constant: the element's
                                    // transform update picks POINT at natural size and LINEAR when
                                    // scaled or rotated. See
                                    // [`ui::pixel_rules::ui_surface_sampler`].
                                    //
                                    // **The second argument is the element's own size**, not the
                                    // picture's: passing the picture's size would make an element
                                    // bigger than its picture read as "scaled" and bind LINEAR. It is
                                    // not scaled:
                                    // The transform update compares the element against its own
                                    // UI surface, which object creation allocates at
                                    // the UI region's width and height — the element's own box — for
                                    // every shipped element, and
                                    // sets the physical size before the virtual screen position so a
                                    // resize moves both. A picture smaller than the element is tiled
                                    // into that surface 1:1,
                                    // which is POINT and WRAP — not LINEAR. So the surface size is
                                    // the element's own size, and it is written that way rather than
                                    // by passing `virtual_size` twice, because the two are the same
                                    // number for a different reason each time.
                                    let virtual_size = (
                                        u32::try_from((cmd.screen.x1 - cmd.screen.x0 + 1).max(0))
                                            .unwrap_or(0),
                                        u32::try_from((cmd.screen.y1 - cmd.screen.y0 + 1).max(0))
                                            .unwrap_or(0),
                                    );
                                    let surface_size = virtual_size;
                                    // The graphic-draw fast-path predicate, over the element's
                                    // whole box — which is the pair of extents it takes from
                                    // the draw request, not the dirty piece.
                                    let tiles = ui::pixel_rules::graphic_draw_tiles(
                                        physical,
                                        virtual_size,
                                        cmd.tiling_offset,
                                    );
                                    // **The two address modes are independent.** One mode for both
                                    // axes would be wrong for the common shipped case, which is mixed — a 10 x 5 strip laid
                                    // across a 792 x 5 element wraps in U and covers V exactly once.
                                    // The pair comes back from `ui_draw::quad`, which computes it
                                    // from the same clipped rectangle its UVs are built from;
                                    // recomputing it here from the unclipped element box would answer
                                    // a different question. `graphic_draw_tiles` is still what the
                                    // *counter* means (it is the one-blit fast-path predicate,
                                    // one blit at a time), and it is deliberately not the same
                                    // predicate as `wrap` — see `pixel_rules::graphic_draw_axis`.
                                    let sampler = ui::pixel_rules::ui_surface_sampler_axes(
                                        virtual_size,
                                        surface_size,
                                        cmd.rotation_z_degrees % 360 != 0,
                                        wrap,
                                    );
                                    if tiles {
                                        self.ui_stats.blits_tiled += 1;
                                    }
                                    let point = ui::pixel_rules::ui_sampler_filter(sampler)
                                        == ui::pixel_rules::TEXFILTER_POINT;
                                    if point {
                                        self.ui_stats.blits_unscaled += 1;
                                    } else {
                                        self.ui_stats.blits_scaled += 1;
                                    }
                                    items.push(OverlayItem::Triangles {
                                        material: OverlayMaterial::Image,
                                        texture,
                                        sampler: OverlaySampler {
                                            point,
                                            wrap_u: wrap.0,
                                            wrap_v: wrap.1,
                                        },
                                        vertices: crate::ui_draw::overlay_vertices(&vertices),
                                    });
                                    self.ui_stats.quads_drawn += 1;
                                }
                                None => self.ui_stats.clipped_away += 1,
                            }
                        }
                        _ => self.ui_stats.skipped_draws += 1,
                    }
                }
                // Steps 2 and 6's other half: the flat-colour rectangles the element put in its
                // own UI surface, including widget clears and blips. They are drawn after the
                // element's own blit and before its glyphs,
                // because that is where the element's own draw and the widget's post-blit draw
                // sit relative to each other.
                for f in &cmd.fills {
                    if f.is_invisible() {
                        self.ui_stats.fills_invisible += 1;
                        continue;
                    }
                    let Some(white) = self.ui_white else { continue };
                    match crate::ui_draw::fill_quad(cmd, f, fb) {
                        Some(vertices) => {
                            // A fill writes pixels *into* the element's own UI
                            // surface, so it carries no sampler of its own —
                            // it is shown by that surface's material. Material creation sets it to
                            // POINT/CLAMP and only leaves POINT when
                            // the element is scaled or rotated. The surface is created at the
                            // element's own size, so for a fill the two sizes are equal by
                            // construction and the condition always answers POINT.
                            //
                            // Stated plainly because it must not be over-claimed: the texture
                            // here is **one white texel** under CLAMP, so POINT and LINEAR return
                            // the same value at every sample and this **cannot change a pixel**.
                            // It is the census agreeing with the client's own choice, not a
                            // repair; the UI sampler tests assert both halves.
                            //
                            // A fill has no graphic at all, so the graphic
                            // tiling arm cannot be reached and the address mode is
                            // CLAMP by the same construction that makes the filter POINT.
                            items.push(OverlayItem::Triangles {
                                material: OverlayMaterial::Image,
                                texture: white,
                                sampler: OverlaySampler::POINT_CLAMP,
                                vertices: crate::ui_draw::overlay_vertices(&vertices),
                            });
                            self.ui_stats.fills_drawn += 1;
                        }
                        None => self.ui_stats.fills_clipped += 1,
                    }
                }
                // A preview is this element's own post-blit content, so it belongs
                // between the element's blit and its glyphs --
                // and it is the reason the preview draws *over* the panel art rather than under
                // it. Nothing in the pass clears colour.
                // An element can emit **two** commands -- its own blit and, after its children,
                // the flat-colour rectangles generated into its UI surface -- so the preview pass
                // runs on the first command and not on the second. It happens once per element,
                // not once per blit.
                if !drawn_previews.contains(&cmd.who) {
                    for (_, id, rect) in previews.iter().filter(|(w, _, _)| *w == cmd.who) {
                        self.lower_preview(*id, *rect, &mut items);
                    }
                    drawn_previews.push(cmd.who);
                }
                // The text of a lifted space's subtree goes after the space.
                holding = lifted
                    .iter()
                    .position(|l| l.2.contains(&cmd.who))
                    .map(|li| (li, items.len()));
                // An element with no glyphs has no selection either -- `selection_boxes` walks the
                // same glyph list -- but the two are tested together rather than one standing in
                // for the other, so the invert pass below cannot be skipped by a guard that is
                // about something else.
                if cmd.glyphs.is_empty() && cmd.invert.is_empty() {
                    continue;
                }
                let Some(clip) = crate::ui_draw::visible_box(cmd, fb) else {
                    self.ui_stats.clipped_away += 1;
                    continue;
                };
                for (font, run) in crate::ui_draw::font_runs(&cmd.glyphs) {
                    let Some(Some((atlas, sheet, outline_sheet))) = self.ui_fonts.get(&font) else {
                        self.ui_stats.glyphs_skipped += run.len() as u64;
                        continue;
                    };
                    // **The outline pass, first.** The client's glyph loop runs twice
                    // for an element carrying attribute `0x21`; when bit `0x10` is set, pass 0 is
                    // the outline and pass 1 is the foreground, over the same pens. The outline must be
                    // submitted first because a text batch's only ordering is submission order.
                    //
                    // The two arms sample different textures, which is why this is a separate
                    // draw call and not extra vertices on the foreground one: the `0x7000` arm
                    // reads the font's background sheet and the `0x9000` arm reads the foreground
                    // sheet eight times over.
                    if let Some(outline_colour) = cmd.text_outline {
                        let (verts, arm, dropped) =
                            crate::ui_draw::outline_vertices(atlas, run, outline_colour, clip, fb);
                        let tex = match arm {
                            crate::ui_draw::OutlineArm::BackgroundSheet => *outline_sheet,
                            crate::ui_draw::OutlineArm::Neighbourhood => Some(*sheet),
                        };
                        match arm {
                            crate::ui_draw::OutlineArm::BackgroundSheet => {
                                self.ui_stats.outline_draws_sheet += 1;
                            }
                            crate::ui_draw::OutlineArm::Neighbourhood => {
                                self.ui_stats.outline_draws_neighbourhood += 1;
                            }
                        }
                        match tex {
                            Some(tex) if !verts.is_empty() => {
                                self.ui_stats.outline_glyphs_skipped += dropped;
                                // The same POINT/CLAMP sampler the foreground pass uses: the
                                // outline blit uses the same integer source rectangle too: one
                                // destination pixel to one source texel, only dilated.
                                self.ui_stats.outline_draws += 1;
                                self.ui_stats.outline_glyphs_drawn +=
                                    (verts.len() / crate::ui_draw::UI_VERTEX_BYTES / 6) as u64;
                                items.push(OverlayItem::Triangles {
                                    material: OverlayMaterial::Text,
                                    texture: tex,
                                    sampler: OverlaySampler::POINT_CLAMP,
                                    vertices: crate::ui_draw::overlay_vertices(&verts),
                                });
                            }
                            // An element asked for an outline the font cannot draw: the
                            // background sheet would not decode or would not upload. Tolerated
                            // and counted, as every other UI texture failure is.
                            _ => self.ui_stats.outline_glyphs_skipped += run.len() as u64,
                        }
                    }
                    let (vertices, dropped) = crate::ui_draw::glyph_vertices(atlas, run, clip, fb);
                    self.ui_stats.glyphs_skipped += dropped;
                    if vertices.is_empty() {
                        continue;
                    }
                    // The client's font material sets both minification and
                    // magnification filters to `TEXFILTER_POINT`, and both address modes to
                    // `TEXADDRESS_CLAMP`, overriding the device default of LINEAR/WRAP. That is
                    // sampler 3, not 1. See [`ui::pixel_rules::UI_GLYPH_SAMPLER`] for why this is a
                    // guard rail rather than a fix: the glyph rectangle lands one
                    // texel on one pixel in integers, so at the shipped placement the two agree
                    // exactly -- and a half-pixel error is 0 px under POINT and 256 under LINEAR.
                    // **The geometry is not adjusted to compensate; only the sampler changed.**
                    self.ui_stats.glyph_draws += 1;
                    self.ui_stats.glyphs_drawn +=
                        (vertices.len() / crate::ui_draw::UI_VERTEX_BYTES / 6) as u64;
                    items.push(OverlayItem::Triangles {
                        material: OverlayMaterial::Text,
                        texture: *sheet,
                        sampler: OverlaySampler::POINT_CLAMP,
                        vertices: crate::ui_draw::overlay_vertices(&vertices),
                    });
                }
                // **The element draw's selection arm, and it comes after the glyphs.**
                // Retail inverts each selected glyph's cell immediately after drawing the character
                // returns, inside the **foreground** pass (the pass value is `0x1000`; the
                // outline pass uses `0x9000`/`0x7000` and carries no invert). Adjacent cells
                // share no pixels, because the pen advances by exactly the width the rectangle
                // uses, so inverting them one at a time inside the loop and inverting all of them
                // after it are the same picture -- and doing it here keeps the invert over the
                // outline pass's pixels too, which is where it sits in the client.
                for r in &cmd.invert {
                    let Some(white) = self.ui_white else { continue };
                    match crate::ui_draw::invert_quad(cmd, *r, fb) {
                        Some(vertices) => {
                            // One white texel under CLAMP, exactly as a fill: the quad carries no
                            // picture and the blend does all the work.
                            items.push(OverlayItem::Triangles {
                                material: OverlayMaterial::Invert,
                                texture: white,
                                sampler: OverlaySampler::POINT_CLAMP,
                                vertices: crate::ui_draw::overlay_vertices(&vertices),
                            });
                            self.ui_stats.inverts_drawn += 1;
                        }
                        None => self.ui_stats.inverts_clipped += 1,
                    }
                }
            }
            self.settle_lifts(&mut items, &mut lifted, &mut holding, cmds.len());
            items
        }

        /// Before command `index`: move the text the last command held into its space's keeping,
        /// then draw each space whose subtree ended with the last command, and its held text over
        /// it.
        #[allow(clippy::type_complexity)] // the tuple `overlay_items` names
        fn settle_lifts(
            &mut self,
            items: &mut Vec<OverlayItem>,
            lifted: &mut [(
                PreviewId,
                dereth_render::camera::Viewport,
                Vec<dereth_ui::ElemHandle>,
                usize,
                Vec<OverlayItem>,
            )],
            holding: &mut Option<(usize, usize)>,
            index: usize,
        ) {
            if let Some((li, start)) = holding.take() {
                let held: Vec<OverlayItem> = items.drain(start..).collect();
                lifted[li].4.extend(held);
            }
            for l in lifted.iter_mut().filter(|l| l.3 + 1 == index) {
                self.lower_preview(l.0, l.1, items);
                items.append(&mut l.4);
            }
        }

        /// A queued preview space at this point in the overlay, counted as drawn or as empty. A
        /// space that was never built is neither, as before.
        fn lower_preview(
            &mut self,
            id: PreviewId,
            rect: dereth_render::camera::Viewport,
            items: &mut Vec<OverlayItem>,
        ) {
            let Some(space) = self.device.previews.get(&id) else {
                return;
            };
            if space.object_count() == 0 {
                self.ui_stats.previews_empty += 1;
                return;
            }
            items.push(OverlayItem::Preview { space: id, rect });
            self.ui_stats.previews_drawn += 1;
        }

        /// The intro movie's current frame, as the UI image `id` names.
        ///
        /// The movie path locks the element's own UI surface and blits the decoded frame into it;
        /// the ordinary UI path then copies that surface into
        /// the element's local texture and draws it as a quad. There is no lockable system-memory surface
        /// here, so the frame is a texture — and, because a texture's pixels cannot be rewritten in
        /// place through the renderer's API, **the previous frame's slot is released before the new one
        /// is taken**. That keeps the movie at one live descriptor rather than 219: at 640x480 the
        /// intro would otherwise want 219 slots of the heap for one screen.
        ///
        /// Call it **outside** `begin_frame`/`end_frame`, with the rest of [`Renderer::prepare_ui`]:
        /// a release inside an open frame is parked against that frame's fence and the slot is not
        /// reusable until it passes.
        pub fn set_movie_frame(&mut self, id: DataId, t: &dereth_primitives::TextureData) {
            // A movie frame is the interface's own picture.
            const MOVIE: dereth_ui::ImageSource = dereth_ui::ImageSource::Interface;
            if let Some(Some((texture, _))) = self.ui_textures.get(&(id, None, MOVIE)).copied() {
                match self.device.overlay_release(texture) {
                    OverlayReleased::Freed => self.ui_release.freed += 1,
                    OverlayReleased::StillLinked => self.ui_release.still_linked += 1,
                    OverlayReleased::Unknown | OverlayReleased::Absent => {
                        self.ui_release.unknown += 1;
                    }
                }
                self.ui_textures.remove(&(id, None, MOVIE));
            }
            // Uncached on purpose: 's table is keyed by a *DataID*
            // pair and every frame of a movie would collide on the same key, handing back the first
            // frame for ever. A local texture is the overlay's uncached kind.
            let texture = crate::ui_draw::movie_texture(id);
            match self.device.overlay_upload(texture, t) {
                Ok(()) => {
                    self.ui_stats.uploaded += 1;
                    self.ui_textures
                        .insert((id, None, MOVIE), Some((texture, (t.width, t.height))));
                }
                Err(e) => {
                    tracing::warn!("movie frame would not upload: {e}");
                    self.ui_stats.decode_failures += 1;
                    self.ui_textures.insert((id, None, MOVIE), None);
                }
            }
        }

        /// How many distinct UI images are resident, for the descriptor-heap note in the report.
        #[must_use]
        pub fn ui_texture_count(&self) -> usize {
            self.ui_textures.len()
        }

        /// Drop every UI image and font atlas this screen uploaded, releasing their descriptor
        /// slots.
        ///
        /// Screen teardown releases the element's image-texture links and destroys every element of the
        /// outgoing screen, so a slot lives exactly as long as a screen that
        /// names it, which is what makes the heap bounded rather than monotonic. The renderer's
        /// descriptor allocator is what turns that release into a reusable slot.
        ///
        /// **Call it outside `begin_frame`/`end_frame`** if the slot should be reusable on the very
        /// next upload: a release inside an open frame is parked against the fence that frame will
        /// signal, because that frame's command list may already have bound it.
        pub fn release_ui_textures(&mut self) -> UiReleaseReport {
            let mut report = UiReleaseReport::default();
            let resident: Vec<OverlayTexture> = self
                .ui_textures
                .values()
                .flatten()
                .map(|(texture, _)| *texture)
                .collect();
            for texture in resident {
                match self.device.overlay_release(texture) {
                    OverlayReleased::Freed => report.freed += 1,
                    OverlayReleased::StillLinked => report.still_linked += 1,
                    OverlayReleased::Unknown | OverlayReleased::Absent => report.unknown += 1,
                }
            }
            self.ui_textures.clear();
            // The **font atlases stay**. This loop releases element image-texture links; fonts are
            // owned by the font mapper and their texture atlases by the device, which builds each
            // atlas once and never rebuilds it. Neither belongs to a screen. There are 32 fonts in the
            // dat and one atlas each, against a 32,768-slot budget.
            report
        }
    }

    /// The one implementation of the presentation seam.
    ///
    /// Every method is a forward to the inherent method above it; the trait exists so that `App`
    /// names a presentation rather than a device, and so that a sibling backend can take this
    /// place without `App` changing.
    impl dereth_client_runtime::present::Presentation for Renderer {
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }

        fn prepare_graphics_device(&mut self) {
            SceneRenderer::prepare_graphics_device(self);
        }

        fn start_frame(&mut self) -> Result<(), dereth_client_runtime::present::PresentError> {
            SceneRenderer::start_frame(self).map_err(crate::present::present_error)
        }

        fn set_game_viewport(&mut self, viewport: Option<dereth_render::camera::Viewport>) {
            SceneRenderer::set_game_viewport(self, viewport);
        }

        fn draw_scene(
            &mut self,
            ws: Option<&dereth_client_runtime::world_state::WorldState>,
        ) -> Result<(), dereth_client_runtime::present::PresentError> {
            SceneRenderer::draw_scene(self, ws).map_err(crate::present::present_error)
        }

        fn end_frame(&mut self) -> Result<(), dereth_client_runtime::present::PresentError> {
            SceneRenderer::end_frame(self).map_err(crate::present::present_error)
        }

        fn wait_idle(&mut self) -> Result<(), dereth_client_runtime::present::PresentError> {
            SceneRenderer::wait_idle(self).map_err(crate::present::present_error)
        }

        fn size(&self) -> (u32, u32) {
            SceneRenderer::size(self)
        }

        fn resize(
            &mut self,
            width: u32,
            height: u32,
        ) -> Result<(), dereth_client_runtime::present::PresentError> {
            SceneRenderer::resize(self, width, height).map_err(crate::present::present_error)
        }

        fn set_presentation_sync(&mut self, full_screen: bool, sync_to_refresh: bool) {
            SceneRenderer::set_presentation_sync(self, full_screen, sync_to_refresh);
        }

        fn capture_png(
            &mut self,
            path: &Path,
        ) -> Result<(), dereth_client_runtime::present::PresentError> {
            SceneRenderer::capture_png(self, path).map_err(crate::present::present_error)
        }

        fn texture_filtering(&self) -> u32 {
            SceneRenderer::texture_filtering(self)
        }

        fn apply_device_preference_requests(
            &mut self,
            requests: Vec<dereth_ui_screens::UiRequest>,
        ) -> Vec<dereth_ui_screens::UiRequest> {
            let (_, gpu) = self.world_mut_and_gpu();
            dereth_scene::render_prefs::apply_gpu_preference_requests(gpu, requests)
        }

        fn apply_render_preference_requests(
            &mut self,
            requests: Vec<dereth_ui_screens::UiRequest>,
        ) -> Vec<dereth_ui_screens::UiRequest> {
            crate::render_prefs::apply_preference_requests(self, requests)
        }

        fn load_first_pixel_scene(
            &mut self,
            assets: &dyn AssetSource,
            id: DataId,
        ) -> Result<(), dereth_client_runtime::present::PresentError> {
            SceneRenderer::load_first_pixel_scene(self, assets, id)
                .map_err(crate::present::present_error)
        }

        fn load_world(
            &mut self,
            store: &std::sync::Arc<dereth_dat::RetailDatStore>,
            cfg: dereth_client_runtime::scene::SceneConfig,
            world: &mut Option<dereth_client_runtime::world_state::WorldState>,
        ) -> Result<(), dereth_world_data::landblock::WorldError> {
            SceneRenderer::load_world(self, store, cfg, world)
        }

        fn release_world(
            &mut self,
            world: &mut Option<dereth_client_runtime::world_state::WorldState>,
        ) -> u32 {
            SceneRenderer::release_world(self, world)
        }

        fn stream_world(
            &mut self,
            store: &dereth_dat::RetailDatStore,
            world: Option<&mut dereth_client_runtime::world_state::WorldState>,
        ) -> Result<(), dereth_world_data::landblock::WorldError> {
            SceneRenderer::stream_world(self, store, world)
        }

        fn sync_objects(
            &mut self,
            store: &std::sync::Arc<dereth_dat::RetailDatStore>,
            stream: &mut dereth_client_runtime::objects::ObjectStream,
            world: Option<&mut dereth_client_runtime::world_state::WorldState>,
        ) -> Result<(), dereth_world_data::landblock::WorldError> {
            SceneRenderer::sync_objects(self, store, stream, world)
        }

        fn prepare_object_dispatch(
            &mut self,
            store: &std::sync::Arc<dereth_dat::RetailDatStore>,
            stream: &mut dereth_client_runtime::objects::ObjectStream,
            world: Option<&mut dereth_client_runtime::world_state::WorldState>,
        ) -> Result<(), dereth_world_data::landblock::WorldError> {
            SceneRenderer::prepare_object_dispatch(self, store, stream, world)
        }

        fn update_render_preferences(
            &mut self,
            store: &dereth_dat::RetailDatStore,
            world: Option<&mut dereth_client_runtime::world_state::WorldState>,
        ) -> Result<
            dereth_client_runtime::frame_events::RenderPrefWork,
            dereth_world_data::landblock::WorldError,
        > {
            SceneRenderer::update_render_preferences(self, store, world)
        }

        fn set_world_view_state(&mut self, hidden: bool, view_distance: Option<f32>) {
            SceneRenderer::set_world_view_state(self, hidden, view_distance);
        }

        fn offer_object_identity(
            &mut self,
            identity: std::sync::Arc<dereth_client_runtime::object_identity::ObjectIdentity>,
        ) {
            SceneRenderer::offer_object_identity(self, identity);
        }

        fn overlay_upload(
            &mut self,
            texture: OverlayTexture,
            data: &dereth_primitives::TextureData,
        ) -> Result<(), dereth_client_runtime::present::PresentError> {
            SceneRenderer::overlay_upload(self, texture, data)
                .map_err(crate::present::present_error)
        }

        fn overlay_release(&mut self, texture: OverlayTexture) -> OverlayReleased {
            SceneRenderer::overlay_release(self, texture)
        }

        fn draw_overlay(
            &mut self,
            items: &[OverlayItem],
        ) -> Result<(), dereth_client_runtime::present::PresentError> {
            SceneRenderer::draw_overlay(self, items).map_err(crate::present::present_error)
        }

        fn preview_ensure(
            &mut self,
            id: PreviewId,
            assets: &std::sync::Arc<dereth_world_data::anim_assets::DatAnimAssets>,
        ) -> bool {
            SceneRenderer::ensure_preview(self, id, assets)
        }

        fn preview_set_light(
            &mut self,
            id: PreviewId,
            light: dereth_client_contract::overlay::PreviewLight,
            intensity: f32,
            direction: dereth_primitives::Vec3,
        ) {
            use dereth_client_contract::overlay::PreviewLight as L;
            use dereth_world_render::lighting::LightType;
            let light = match light {
                L::Point => LightType::Point,
                L::Directional => LightType::Directional,
                L::Spot => LightType::Spot,
            };
            if let Some(s) = self.preview_mut(id) {
                s.set_light(light, intensity, direction);
            }
        }

        fn preview_use_sharp_mode(&mut self, id: PreviewId) {
            if let Some(s) = self.preview_mut(id) {
                s.mode.use_sharp_mode();
            }
        }

        fn preview_use_world_fov(&mut self, id: PreviewId) {
            if let Some(s) = self.preview_mut(id) {
                s.mode.use_world_fov();
            }
        }

        fn preview_set_fov(&mut self, id: PreviewId, radians: f32) {
            if let Some(s) = self.preview_mut(id) {
                s.mode.set_fov(radians);
            }
        }

        fn preview_set_camera_position(
            &mut self,
            id: PreviewId,
            position: dereth_primitives::Vec3,
        ) {
            if let Some(s) = self.preview_mut(id) {
                s.mode.set_camera_position(position);
            }
        }

        fn preview_set_camera_direction(
            &mut self,
            id: PreviewId,
            direction: dereth_primitives::Vec3,
        ) {
            if let Some(s) = self.preview_mut(id) {
                s.mode.set_camera_direction(direction);
            }
        }

        fn preview_set_camera_direction_degrees(
            &mut self,
            id: PreviewId,
            degrees: dereth_primitives::Vec3,
        ) {
            if let Some(s) = self.preview_mut(id) {
                s.mode.set_camera_direction_degrees(degrees);
            }
        }

        fn preview_remove_all_objects(&mut self, id: PreviewId) {
            if let Some(s) = self.preview_mut(id) {
                s.remove_all_objects();
            }
        }

        fn objects_in_other_look(&self) -> bool {
            self.world()
                .is_some_and(dereth_scene::world_scene::SceneDraw::objects_from_other_files)
        }

        fn preview_add_object(
            &mut self,
            id: PreviewId,
            store: &dereth_dat::RetailDatStore,
            setup: DataId,
        ) -> Result<Option<usize>, dereth_client_runtime::present::PresentError> {
            SceneRenderer::add_preview_object(self, id, store, setup)
                .map_err(crate::present::present_error)
        }

        fn preview_add_object_dressed(
            &mut self,
            id: PreviewId,
            store: &dereth_dat::RetailDatStore,
            setup: DataId,
            objdesc: Option<&dereth_animation::parts::ObjDesc>,
        ) -> Result<Option<usize>, dereth_client_runtime::present::PresentError> {
            SceneRenderer::add_preview_object_dressed(self, id, store, setup, objdesc)
                .map_err(crate::present::present_error)
        }

        fn preview_set_heading(&mut self, id: PreviewId, index: usize, degrees: f32) {
            if let Some(s) = self.preview_mut(id) {
                s.set_heading(index, degrees);
            }
        }

        fn preview_set_scale(&mut self, id: PreviewId, index: usize, scale: f32) {
            if let Some(s) = self.preview_mut(id) {
                s.set_scale(index, scale);
            }
        }

        fn preview_set_sequence_animation(
            &mut self,
            id: PreviewId,
            index: usize,
            animation: DataId,
            clear: bool,
            low_frame: i32,
            framerate: f32,
        ) -> bool {
            match self.preview_mut(id) {
                Some(s) => s.set_sequence_animation(index, animation, clear, low_frame, framerate),
                None => false,
            }
        }

        fn preview_clear_sequence_anims(&mut self, id: PreviewId, index: usize) {
            if let Some(s) = self.preview_mut(id) {
                s.clear_sequence_anims(index);
            }
        }

        fn preview_has_anims(&self, id: PreviewId, index: usize) -> bool {
            self.preview(id).is_some_and(|s| s.has_anims(index))
        }

        fn preview_use_time(&mut self, id: PreviewId, dt: f64) {
            if let Some(s) = self.preview_mut(id) {
                s.use_time(dt);
            }
        }

        fn preview_curr_frame_number(&self, id: PreviewId, index: usize) -> Option<u32> {
            self.preview(id).map(|s| s.curr_frame_number(index))
        }

        fn preview_object_bounding_box(
            &self,
            id: PreviewId,
            index: usize,
            store: &dereth_dat::RetailDatStore,
        ) -> Option<dereth_physics::geom::BBox> {
            self.preview(id)
                .and_then(|s| s.object(index))
                .map(|o| o.bounding_box(store))
        }

        fn preview_part_array_mut(
            &mut self,
            id: PreviewId,
            index: usize,
        ) -> Option<&mut dereth_animation::parts::PartArray> {
            self.preview_mut(id).and_then(|s| s.part_array_mut(index))
        }

        fn scene<'a>(
            &'a self,
            world: Option<&'a dereth_client_runtime::world_state::WorldState>,
        ) -> Option<Box<dyn dereth_client_runtime::present::Scene + 'a>> {
            let (draw, world) = (self.device.world()?, world?);
            Some(Box::new(dereth_scene::world_scene::WorldSceneRef {
                world,
                draw,
            }))
        }

        fn scene_mut<'a>(
            &'a mut self,
            world: Option<&'a mut dereth_client_runtime::world_state::WorldState>,
        ) -> Option<Box<dyn dereth_client_runtime::present::SceneMut + 'a>> {
            let (draw, world) = (self.device.world_mut()?, world?);
            Some(Box::new(dereth_scene::world_scene::WorldSceneMut {
                world,
                draw,
            }))
        }
    }

    impl crate::present::ClientPresentation for Renderer {
        fn draw_ui(
            &mut self,
            cmds: &[dereth_ui::UiDrawCmd],
        ) -> Result<(), dereth_client_runtime::present::PresentError> {
            Renderer::draw_ui(self, cmds).map_err(crate::present::present_error)
        }

        fn prepare_ui(
            &mut self,
            interface: &dereth_dat::RetailDatStore,
            world: &dereth_dat::RetailDatStore,
            cmds: &[dereth_ui::UiDrawCmd],
        ) {
            Renderer::prepare_ui_from(self, interface, world, cmds);
        }

        fn release_ui_textures(&mut self) -> UiReleaseReport {
            Renderer::release_ui_textures(self)
        }

        fn set_movie_frame(&mut self, id: DataId, texture: &dereth_primitives::TextureData) {
            Renderer::set_movie_frame(self, id, texture);
        }

        fn target_projection(
            &self,
            id: dereth_primitives::ObjectId,
            world: Option<&dereth_client_runtime::world_state::WorldState>,
        ) -> Option<dereth_ui_screens::hud::target::Projection> {
            SceneRenderer::target_projection(self, id, world)
        }

        fn preview_queue(
            &mut self,
            id: PreviewId,
            who: dereth_ui::ElemHandle,
            rect: dereth_render::camera::Viewport,
        ) {
            Renderer::queue_preview(self, id, who, rect);
        }

        fn preview_queue_under_text(
            &mut self,
            id: PreviewId,
            subtree: Vec<dereth_ui::ElemHandle>,
            rect: dereth_render::camera::Viewport,
        ) {
            Renderer::queue_preview_under_text(self, id, subtree, rect);
        }
    }

    #[cfg(test)]
    #[path = "gpu_texture_minification_tests.rs"]
    mod tests;
}
