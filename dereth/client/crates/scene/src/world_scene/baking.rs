//! Building views, object batches and mesh cache residency.

use super::*;

/// Sutherland–Hodgman against the single plane `w > ε`, in homogeneous clip space.
///
/// The one plane cannot skip, and the only one
/// [`WorldScene::building_portal_screen_polygons`] needs: an opening entirely off the side of
/// the screen projects to a polygon off the side of the screen, which the pixel mask simply
/// does not reach, but one straddling the eye plane projects to nonsense unless it is cut.
pub(super) fn clip_to_near_plane(poly: &[glam::Vec4]) -> Vec<glam::Vec4> {
    const EPS: f32 = 1.0e-4;
    let mut out = Vec::with_capacity(poly.len() + 2);
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        let (ain, bin) = (a.w > EPS, b.w > EPS);
        if ain {
            out.push(a);
        }
        if ain != bin {
            let t = (EPS - a.w) / (b.w - a.w);
            out.push(a + (b - a) * t);
        }
    }
    out
}

/// The portal half of building initialization.
///
/// Building data keeps `BuildInfo::portals` as the outdoor portal list and wraps
/// `BuildInfo::id` in a one-part setup whose graphics object carries the **drawing** BSP. This
/// method joins the two: each entry is `(polygon index, portal_index)`.
///
/// Three polarity details are the same three as for a cell portal:
/// `portal_side = ((~flags) >> 1) & 1`, `other_cell_id` is the **low 16 bits** and needs the
/// landblock base OR-ed in, and a portal index is an index and not an id.
///
/// **One set of openings per degrade level.** The shell part takes each level's graphics
/// object from its degrade record, and the openings drawn are those of the level the shell
/// draws, so each level's drawing BSP is read here (see [`level_openings`]). With
/// `per_level` clear (the scene's degrade switches off) or no record, the building has the one
/// level-0 entry, as the shell bake does. `shell_placements[i]` is the degrade placement of
/// `info.buildings[i]`'s shell part, as the block's object bake numbered it.
///
/// **The openings come from the files that drew the shell.** A shell the objects' look takes
/// (`from_look[i]`) is baked from the look's files and record, and its placement's level
/// indexes that record, so its openings are read through `look` too; every other shell's come
/// from the world's files and cache.
///
/// A `BuildInfo` none of whose levels has a drawing BSP with a portal node yields nothing:
/// that is a building with no interior, which is most of the world's walls and bridges.
pub(super) fn bake_building_views(
    (store, cache): (&RetailDatStore, &mut BakeCache),
    mut look: Option<(&RetailDatStore, &mut BakeCache)>,
    block: u16,
    info: &LandblockInfo,
    from_look: &[bool],
    shell_placements: &[Option<u32>],
    per_level: bool,
) -> Vec<BuildingView> {
    use dereth_world_render::cells::portal_view::BuildingPortal as BldPortal;

    let base = u32::from(block) << 16;
    // Shared by graphics object within one set of files: a level that reuses another level's
    // mesh, or a shell that appears twice in the block, is decoded once.
    let mut by_gfxobj: HashMap<(bool, DataId), Option<Arc<LevelOpenings>>> = HashMap::new();
    let mut out = Vec::new();
    for (i, b) in info.buildings.iter().enumerate() {
        if b.portals.is_empty() {
            continue;
        }
        let shell_from_look = from_look.get(i).copied().unwrap_or(false) && look.is_some();
        let (files, files_cache): (&RetailDatStore, &mut BakeCache) =
            match (shell_from_look, look.as_mut()) {
                (true, Some((look_files, look_cache))) => (*look_files, &mut **look_cache),
                _ => (store, &mut *cache),
            };
        let record = if per_level {
            files_cache.degrade_record(files, b.id)
        } else {
            None
        };
        let ids: Vec<DataId> = record.as_ref().map_or_else(
            || vec![b.id],
            |r| r.degrades.iter().map(|e| e.gfxobj_id).collect(),
        );
        let levels: Vec<Option<Arc<LevelOpenings>>> = ids
            .iter()
            .map(|&id| {
                by_gfxobj
                    .entry((shell_from_look, id))
                    .or_insert_with(|| level_openings(files, id).map(Arc::new))
                    .clone()
            })
            .collect();
        if levels.iter().all(Option::is_none) {
            continue;
        }
        let portals = b
            .portals
            .iter()
            .map(|p| BldPortal {
                portal_side: u8::from((!p.flags >> 1) & 1 != 0),
                other_cell_id: base | u32::from(p.other_cell_id),
                other_portal_id: i32::from(p.other_portal_id),
                stab_list: p
                    .stab_list
                    .iter()
                    .map(|&c| CellId(base | u32::from(c)))
                    .collect(),
            })
            .collect();
        out.push(BuildingView {
            cell_index: outside_cell_index(b.frame.origin.x, b.frame.origin.y),
            frame: b.frame,
            levels,
            // Only a shell baked with its record has a placement to follow.
            shell_placement: record.and(shell_placements.get(i).copied().flatten()),
            portals,
        });
    }
    out
}

/// One shell level's openings: the graphics object's drawing BSP and the polygons its portal
/// nodes name. `None` for a level that opens nothing: graphics object id 0 (the level draws
/// nothing at all), no drawing BSP, or no portal node in it, which is what most degraded
/// shells are.
pub(super) fn level_openings(store: &RetailDatStore, id: DataId) -> Option<LevelOpenings> {
    if id.0 == 0 {
        return None;
    }
    let bytes = store.read_typed(DbType::GfxObj, id).ok()?;
    let g = dereth_assets::GfxObj::decode_payload_in(store.era_of(id), id, &bytes).ok()?;
    let bsp = g.drawing_bsp.clone()?;
    // Only the polygons the BSP's portal nodes name; a 226-polygon shell names two.
    let mut portal_polygons: BTreeMap<usize, BuildingPortalPolygon> = BTreeMap::new();
    for node in &bsp.nodes {
        for &(poly, _) in &node.in_portals {
            let Ok(i) = usize::try_from(poly) else {
                continue;
            };
            let Some(p) = g.polygons.get(i) else { continue };
            let vertices: Vec<Vec3> = p
                .vertex_ids
                .iter()
                .filter_map(|&v| g.vertex_array.vertices.get(v as usize))
                .map(|v| v.position)
                .collect();
            if vertices.len() < 3 {
                continue;
            }
            // Plane construction is the physics crate's, and the sidedness test has to read the same
            // plane that the physics side does.
            let plane = dereth_physics::geom::Polygon::new(vertices.clone()).plane;
            portal_polygons.insert(
                i,
                BuildingPortalPolygon {
                    plane_normal: plane.normal,
                    plane_d: plane.d,
                    vertices,
                },
            );
        }
    }
    (!portal_polygons.is_empty()).then_some(LevelOpenings {
        bsp,
        portal_polygons,
    })
}

impl std::fmt::Debug for BakeCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BakeCache")
            .field("surfaces", &self.surfaces.len())
            .field("slots", &self.by_slot.len())
            .finish_non_exhaustive()
    }
}

impl Default for BakeCache {
    fn default() -> Self {
        Self {
            parts: HashMap::new(),
            geometry: HashMap::new(),
            surface_blends: HashMap::new(),
            surfaces: HashMap::new(),
            links: HashMap::new(),
            by_slot: HashMap::new(),
            key_clip_map: HashMap::new(),
            palettised: HashMap::new(),
            draws: HashMap::new(),
            degrades: HashMap::new(),
            sort_centers: HashMap::new(),
            drawing_spheres: HashMap::new(),
            parts_not_drawn: 0,
            palette_range_failures: 0,
            palette_missing: 0,
            // The `TextureScale` preference's initial value is
            // `FULL_RES`; `Render.EnvironmentTextureDetail`'s registered default is 1,
            // which `max(v,1)-1` also makes `FULL_RES`, so the two agree on a fresh install.
            image_scale: dereth_render::texture::ImageScale::FullRes,
            pending_surfaces: 0,
            environment_texture_detail:
                dereth_client_runtime::render_prefs::RenderPreferences::default()
                    .environment_texture_detail,
            last_texture_built: None,
            texture_uploads: 0,
            texture_upload_texels: 0,
            texture_source_texels: 0,
            textures_downscaled: 0,
            textures_uploaded: 0,
            unowned_releases: 0,
            texture_key_hits: 0,
            solid_texel_key_hits: 0,
            solid_texels_uploaded: 0,
            solid_colour_words: std::collections::BTreeSet::new(),
            clipmap_key_conflicts: 0,
            clipmap_conflicts: Vec::new(),
            surfaces_resolved: 0,
            surfaces_translucent: 0,
            // The client always writes surface setup's `curr_alpha`; the switch is a control
            // for the differential, so the default is the client's own behaviour and a
            // `BakeCache` nobody configures honours translucency.
            surface_translucency: true,
        }
    }
}

impl BakeCache {
    /// [`dereth_client_runtime::models::draws_at_near_band`], memoised per graphics object.
    pub(crate) fn draws_at_near_band(&mut self, store: &RetailDatStore, gfxobj: DataId) -> bool {
        *self
            .draws
            .entry(gfxobj)
            .or_insert_with(|| dereth_client_runtime::models::draws_at_near_band(store, gfxobj))
    }

    /// The `GfxObjDegradeInfo` named by a graphics object's `flags & 8`, memoised and shared.
    ///
    /// `None` for an object with no record — the native null-record arm makes a one-entry
    /// `gfxobj[]` holding the object itself —
    /// and also for a record with a single level, which cannot switch and is cheaper drawn as
    /// ordinary geometry.
    pub(crate) fn degrade_record(
        &mut self,
        store: &RetailDatStore,
        gfxobj: DataId,
    ) -> Option<Arc<dereth_assets::motion::GfxObjDegradeInfo>> {
        self.degrades
            .entry(gfxobj)
            .or_insert_with(|| {
                let bytes = store.read_typed(DbType::GfxObj, gfxobj).ok()?;
                let obj =
                    dereth_assets::GfxObj::decode_payload_in(store.era_of(gfxobj), gfxobj, &bytes)
                        .ok()?;
                let did = obj.did_degrade?;
                let bytes = store.read_typed(DbType::DegradeInfo, did).ok()?;
                let info = dereth_assets::GfxObjDegradeInfo::decode_payload_in(
                    store.era_of(did),
                    did,
                    &bytes,
                )
                .ok()?;
                (info.degrades.len() > 1).then(|| Arc::new(info))
            })
            .clone()
    }

    /// A graphics object's level-zero sort centre,
    /// memoised. The viewer-distance update reads
    /// the graphics object's sort centre, i.e. the **level-0** mesh's, before it picks a level.
    pub(crate) fn sort_center(&mut self, store: &RetailDatStore, gfxobj: DataId) -> Vec3 {
        *self.sort_centers.entry(gfxobj).or_insert_with(|| {
            let Ok(bytes) = store.read_typed(DbType::GfxObj, gfxobj) else {
                return Vec3::ZERO;
            };
            dereth_assets::GfxObj::decode_payload_in(store.era_of(gfxobj), gfxobj, &bytes)
                .map_or(Vec3::ZERO, |o| o.sort_center)
        })
    }

    /// A graphics object's drawing sphere,
    /// memoised exactly as [`Self::sort_center`] is.
    ///
    /// It goes through [`dereth_world_data::setup::drawing_sphere`] — the **one** transcription
    /// of the sphere at the root of the drawing tree — rather than repeating
    /// `drawing_bsp.nodes[0].sphere` here. The collision setup in `object_physics` and the
    /// selection ray in `pick.rs` read it too: one fork with three readers, so a wrong
    /// answer is wrong in all three places at once and a mutation of it reddens all three.
    ///
    /// `None` is a real state and not a decode failure: a graphics object stores a sphere only
    /// when there is a drawing BSP to take one from.
    pub(crate) fn drawing_sphere(
        &mut self,
        store: &RetailDatStore,
        gfxobj: DataId,
    ) -> Option<(Vec3, f32)> {
        *self.drawing_spheres.entry(gfxobj).or_insert_with(|| {
            let bytes = store.read_typed(DbType::GfxObj, gfxobj).ok()?;
            let o = dereth_assets::GfxObj::decode_payload_in(store.era_of(gfxobj), gfxobj, &bytes)
                .ok()?;
            dereth_world_data::setup::drawing_sphere(&o).map(|s| (s.center, s.radius))
        })
    }

    /// Surface setup's answer, memoised. It depends only on the surface record and the two polygon
    /// flags, so a wall texture shared by forty interior cells is uploaded once.
    ///
    /// The device's descriptor heap is finite (32,768 pairs); a town's env cells alone
    /// exhaust a 2,048-texture heap without this, and the failure
    /// is `descriptor heap exhausted` several blocks into a walk rather than at load.
    /// **Every call takes one link**, hit or miss, when the answer carries a texture. That is
    /// the keyed texture cache's shape: *"if the key is non-zero and already in the texture
    /// table, take a reference and return it"*. The caller owns that link
    /// and must return it through [`Self::release_group_texture`] when the mesh it built goes
    /// away, or hold it for the life of the scene (which is what the terrain, the statics and
    /// the local body do, and what [`WorldScene::release_textures`] settles at teardown).
    pub(crate) fn resolve(
        &mut self,
        store: &RetailDatStore,
        textures: &TextureStore<'_>,
        gpu: &mut Gpu,
        key: GroupKey,
    ) -> Result<ResolvedSurface, RenderError> {
        let r = self.resolve_with(store, textures, gpu, key, None)?;
        Ok(r.expect("a resolve that defers nothing is never pending"))
    }

    /// [`Self::resolve`], asking `defer` for a compressed texture's mip chain rather than
    /// building it here. `None` when that chain is still being built: nothing is remembered and
    /// no link is taken, so the caller throws its work away and resolves the surface again
    /// later. See [`crate::mip_worker`].
    pub(crate) fn resolve_with(
        &mut self,
        store: &RetailDatStore,
        textures: &TextureStore<'_>,
        gpu: &mut Gpu,
        key: GroupKey,
        defer: Option<&mut crate::mip_worker::MipWorker>,
    ) -> Result<Option<ResolvedSurface>, RenderError> {
        if let Some(r) = self.surfaces.get(&key).copied() {
            if let Some(slot) = r.texture {
                *self.links.entry(slot.0).or_insert(0) += 1;
            }
            return Ok(Some(r));
        }
        // `image_scale` is at the moment
        // the device texture is created, and `built` is what actually went to the device.
        let mut built = None;
        let Some(r) = resolve_surface(
            store,
            textures,
            gpu,
            &key,
            self.image_scale,
            &mut built,
            defer,
        )?
        else {
            self.pending_surfaces += 1;
            return Ok(None);
        };
        if let Some((w, h, src_w, src_h)) = built {
            self.last_texture_built = Some((w, h, self.image_scale as u32));
            self.texture_uploads += 1;
            self.texture_upload_texels += u64::from(w) * u64::from(h);
            self.texture_source_texels += u64::from(src_w) * u64::from(src_h);
            if (w, h) != (src_w, src_h) {
                self.textures_downscaled += 1;
            }
        }
        self.palette_range_failures += r.palette_failures;
        self.palette_missing += u32::from(r.palette_missing);
        // Counted on the **miss** path: this is a census of distinct surface
        // groups, not of draws.
        self.surfaces_resolved += 1;
        self.surfaces_translucent += u32::from(r.vertex_alpha != 0xFF);
        if let Some(slot) = r.texture {
            // `upload_texture_keyed` has already answered
            // the combined-texture creation's question. If it handed back a slot this memo
            // already owns, it incremented the image texture's reference — but this memo
            // deliberately holds exactly **one** image-texture link per slot however many
            // groups name it (which is
            // what [`WorldScene::release_textures`] balances against), so the extra reference
            // goes straight back and the group's link is counted here instead.
            match self.by_slot.entry(slot.0) {
                std::collections::hash_map::Entry::Occupied(mut e) => {
                    gpu.release_texture(slot);
                    e.get_mut().push(key.clone());
                    // Two counters, not one: see [`ResolvedSurface::solid_texel`]. A single
                    // total over both transcriptions cannot fail when one of them is
                    // unwired.
                    if r.solid_texel {
                        self.solid_texel_key_hits += 1;
                    } else {
                        self.texture_key_hits += 1;
                    }
                    *self.links.entry(slot.0).or_insert(0) += 1;
                }
                std::collections::hash_map::Entry::Vacant(e) => {
                    e.insert(vec![key.clone()]);
                    self.links.insert(slot.0, 1);
                    self.textures_uploaded += 1;
                    // Its own counter, on the miss arm, so the declared
                    // deviation's population is a measurement rather than an assumption. One
                    // counter per transcription: the textured half is counted by
                    // `textures_uploaded` minus this.
                    if r.solid_texel {
                        self.solid_texels_uploaded += 1;
                        // The *word*, as distinct from the *upload*. `texture_key`
                        // for this arm is `TextureKey::solid_color(argb)`, so its payload is
                        // the colour word the solid-colour texture setter would have been handed.
                        self.solid_colour_words
                            .insert(u32::try_from(r.texture_key.raw() & 0xFFFF_FFFF).unwrap_or(0));
                    }
                }
            }
            if !r.texture_key.is_uncached() {
                match self.key_clip_map.entry(r.texture_key) {
                    std::collections::hash_map::Entry::Occupied(e) => {
                        if *e.get() != r.clip_map {
                            self.clipmap_key_conflicts += 1;
                            // Keep the first few, with the texture id the
                            // group named, so the difference the bit makes can be measured
                            // rather than assumed either way.
                            if self.clipmap_conflicts.len() < CLIPMAP_CONFLICT_SAMPLE {
                                self.clipmap_conflicts.push(ClipMapConflict {
                                    texture: key.appearance.texture.or(key.surface),
                                    surface: key.surface,
                                    first_clip_map: *e.get(),
                                    second_clip_map: r.clip_map,
                                });
                            }
                        }
                    }
                    std::collections::hash_map::Entry::Vacant(e) => {
                        e.insert(r.clip_map);
                    }
                }
            }
        }
        self.surfaces.insert(key, r);
        Ok(Some(r))
    }

    /// Return one of the links [`Self::resolve`] handed out.
    ///
    /// Retail decrements only while the count is above the cache's own
    /// link and frees the object once it is not. This build's memo holds no link of
    /// its own, so the equivalent edge is **zero**: the last consumer's release frees the entry.
    ///
    /// Retail would then put the object on a per-type free list
    /// bounded by a per-type maximum (`DB_TYPE_SURFACE` 200, `DB_TYPE_SURFACETEXTURE`
    /// 400, `DB_TYPE_GFXOBJ` 200), trimming
    /// the **oldest** by timestamp when the free count *exceeds* the cap
    /// only when the free count is strictly greater than the cap. **That retention is
    /// handled separately here**: retail's caps are per
    /// *dat record*, and this memo's unit
    /// is a resolved surface *group*, which has no counterpart on that side and therefore no
    /// cap to copy. Retaining nothing is the strict end of the same edge.
    ///
    /// Returns `true` when the entry left the cache and its descriptor pair was handed back.
    pub(crate) fn release_group_texture(&mut self, gpu: &mut Gpu, slot: TextureSlot) -> bool {
        let Some(n) = self.links.get_mut(&slot.0) else {
            // A slot this cache never owned: a handle from a previous device, or a double
            // release. Tolerated and counted rather than fatal, exactly as
            // `Gpu::release_texture` treats the same case, because every caller is a teardown.
            self.unowned_releases += 1;
            return false;
        };
        *n -= 1;
        if *n > 0 {
            return false;
        }
        self.links.remove(&slot.0);
        // Every memo entry that named this texture goes with it. They are
        // one image texture between them, so the last consumer of the last of them is the last
        // consumer of all of them.
        for key in self.by_slot.remove(&slot.0).unwrap_or_default() {
            if let Some(r) = self.surfaces.remove(&key) {
                self.key_clip_map.remove(&r.texture_key);
            }
        }
        self.textures_uploaded = self.textures_uploaded.saturating_sub(1);
        gpu.release_texture(slot);
        true
    }

    /// How many consumers hold a slot, for the tests.
    #[cfg(test)]
    pub(crate) fn link_count(&self, slot: TextureSlot) -> Option<u32> {
        self.links.get(&slot.0).copied()
    }

    /// Whether a shift palette can change anything about this surface's texture, memoised.
    ///
    /// Palette application refuses anything that is not `PFID_P8` or `PFID_INDEX16`,
    /// so a surface record backed by a DXT image is untouched by an `ObjDesc`'s palette — and
    /// keying its cache entry by the palette anyway would upload one copy of it per outfit.
    /// The descriptor heap is finite (32,768 pairs); that is not a saving to
    /// leave on the table.
    pub(super) fn palettised(&mut self, textures: &TextureStore<'_>, id: DataId) -> bool {
        if let Some(hit) = self.palettised.get(&id) {
            return *hit;
        }
        let v = textures.is_palettised(id).unwrap_or(false);
        self.palettised.insert(id, v);
        v
    }

    /// The effective [`SurfaceAppearance`] for one surface record under one part's overrides.
    ///
    /// Texture-id substitution keys on the surface's **original**
    /// `SurfaceTexture` id, while palette application applies the shift palette
    /// to every surface the part owns. Both are dropped here when they cannot change a pixel,
    /// so that an unaffected surface keeps sharing one cache entry across every object.
    pub(crate) fn appearance_for(
        &mut self,
        store: &RetailDatStore,
        textures: &TextureStore<'_>,
        surface: Option<DataId>,
        overrides: Option<&dereth_animation::parts::SurfaceOverrides>,
    ) -> SurfaceAppearance {
        let (Some(sid), Some(ov)) = (surface, overrides) else {
            return SurfaceAppearance::default();
        };
        let Some(decoded) = read_surface(store, sid) else {
            return SurfaceAppearance::default();
        };
        let orig = decoded.orig_texture_id;
        let texture = orig.and_then(|o| {
            ov.texture_maps
                .iter()
                .find(|(old, _)| *old == o)
                .map(|(_, n)| *n)
        });
        let effective = texture.or(orig);
        let palette = match (ov.shift_palette, effective) {
            (Some(pal), Some(tex)) if self.palettised(textures, tex) => Some(PaletteComposition {
                base: pal,
                ranges: ov
                    .subpalettes
                    .iter()
                    .map(|r| (r.palette_set, r.offset, r.length))
                    .collect(),
                from_look: textures.look_ranges().to_vec(),
            }),
            _ => None,
        };
        SurfaceAppearance { texture, palette }
    }
}
impl SceneDraw {
    /// One window slot: landblock fetch and generation, and — for a slot that is new rather
    /// than merely resized — dynamic-object, building, and static-object initialization.
    pub(super) fn build_slot(
        &mut self,
        ws: &mut WorldState,
        store: &RetailDatStore,
        gpu: &mut Gpu,
        xi: u32,
        yi: u32,
        work: SlotWork,
    ) -> Result<(), WorldError> {
        let Some(slot) = ws.streamer.window.slot(xi, yi) else {
            return Ok(());
        };
        let spec = SlotSpec {
            block_x: slot.block_x,
            block_y: slot.block_y,
            lod_div: slot.lod_div,
            dir: slot.trans_dir,
        };
        let (bx, by) = (spec.block_x, spec.block_y);
        let _build = tracing::debug_span!("build_landblock", bx, by, lod = spec.lod_div).entered();
        let Some(lb) = read_landblock(store, bx, by) else {
            // Landblock lookup returns no record for a block the dat does not
            // carry -- the world edge. Nothing is drawn there and nothing is an error.
            // The block still has to hand its bake's links back, exactly as one
            // that scrolled off the edge does.
            if let Some(b) = self.blocks.remove(&(bx, by)) {
                self.released_blocks.push(b);
                self.release_departed_blocks(gpu);
            }
            return Ok(());
        };
        let (mesh, cell_texture, cell_keys, merge_keys, cell_splat) =
            self.land.generate(store, gpu, &lb, spec)?;
        // Kept before `set_mesh` takes the mesh.
        let side_cell_count = mesh.side_cell_count;

        // Only this bake's requests are wanted below.
        self.land.mip_worker.take_requested();
        let wants_objects = self.wants_objects(ws, xi, yi);
        let previous = self.blocks.remove(&(bx, by));
        // `previous` is either reused whole by the `SlotWork::Mesh` arm below
        // — a re-mesh is a size change, which rebuilds the terrain arrays and keeps the
        // block's objects, so its links stay taken — or thrown away, and a bake that is thrown
        // away has to give its links back before the replacement takes its own. This is the
        // *in-place* half of the same edge `release_departed_blocks` runs for a scroll, and it
        // is the one a `--scenery-radius` change and a re-bake go through.
        // **The third clause is what keeps scenery on a re-entered block.**
        // A bake taken on an outer LOD ring contains **no scenery** —
        // `dereth_world_render::scenery::generate_scenery` opens with the same full-detail
        // guard — and the empty bake must not survive the block's promotion back to
        // full detail. Without this, a block that entered the window coarse keeps its
        // scenery-free bake for the life of the session: `scenery_objects` 333 -> 0 over one
        // out-and-back walk, never recovering, while `static_objects` (which the guard does not
        // gate here) comes back every time and hides it.
        //
        // **The test is `!=`, in BOTH directions, and that is retail and not a choice.**
        // A landblock size-change notification destroys the block's objects on the
        // demotion out of full detail, and rebuilds
        // them only for a block that is at full detail with no static objects (see
        // [`SlotWork`]). A promotion-only rule breaks the invariant the full-detail guard exists to
        // state — *scenery if and only if full
        // detail* — because a block baked at 8 and demoted to 4 then carries 14 to 167 scenery
        // objects on a four-cell mesh. The two halves are one rule.
        //
        // **Declared cost.** At `scenery_radius: 2`, the only shipped configuration in
        // which a resident block can be both baking objects and on a coarse ring, this moves
        // one pixel of 307,200, 53.8 to 54.6 px from the nearest building opening, in pixel
        // differentials whose tolerances were taken over the stale bake.
        let reuse = matches!(work, SlotWork::Mesh)
            && wants_objects
            && previous
                .as_ref()
                .is_some_and(|b| b.baked && b.baked_side_cell_count == mesh.side_cell_count);
        let previous = match previous {
            Some(b) if !reuse => {
                self.released_blocks.push(b);
                self.release_departed_blocks(gpu);
                None
            }
            // **The reuse arm returns its TERRAIN links even though it keeps its
            // objects.** `generate` above has already run and taken a fresh reference for
            // every cell of the new mesh; the references the *previous* mesh took are about to
            // be dropped on the floor when `cell_keys` is replaced below. This is the one
            // release site the object half does not have, because an object bake survives
            // a re-mesh (a size change keeps it at an unchanged detail level) and a
            // *terrain* bake never does — `generate` is unconditional.
            //
            // The new references are taken **before** the old ones are returned, deliberately
            // so that a surface the two meshes share is never taken to
            // zero and re-uploaded. Retail's order is the opposite (destroy, then
            // `generate`), which costs it exactly that re-upload; the counts are identical
            // either way and only the peak differs.
            Some(mut b) => {
                self.stats.blocks_remeshed_in_place += 1;
                let keys = std::mem::take(&mut b.cell_keys);
                self.release_block_terrain(gpu, &keys);
                Some(b)
            }
            None => None,
        };
        let built = match (work, previous) {
            // A resize or a restitch keeps the block's objects — **and this arm is reachable
            // only because `reuse` above has already established that the detail level did not
            // change.** A size change does not rebuild only the terrain arrays; it is at an
            // unchanged `side_cell_count` that
            // retail's objects genuinely are untouched: no size change fires,
            // and the next visibility refresh finds the block's static objects and only
            // re-drops the scene objects' heights and re-crosses their cells.
            (SlotWork::Mesh, Some(b)) if wants_objects && b.baked => BakedObjects {
                opaque: b.opaque,
                blended: b.blended,
                degrade: b.degrade,
                env_cells: b.env_cells,
                building_views: b.building_views,
                scenery: b.scenery,
                buildings: b.buildings,
                statics: b.statics,
                sim: b.sim,
                cell_lights: b.cell_lights,
                emitters: b.emitters,
                hosts: b.hosts,
                hosts_spawned: b.hosts_spawned,
                textures_pending: false,
                objects_visual: b.objects_visual,
            },
            _ if wants_objects => self.land.bake(
                store,
                gpu,
                &lb,
                &mesh,
                bx,
                by,
                self.cfg.cell_statics,
                self.cfg.part_degrades,
                self.cfg.degrade_levels,
                self.cfg.static_billboards,
                self.cfg.lod_object_guard,
            )?,
            _ => BakedObjects::default(),
        };

        // A bake that left a texture's mip chain to the worker is handed back whole: its links go
        // back, the block is kept for now as terrain alone -- the state an outer-ring block is
        // in -- and it is queued to bake again once every chain it asked for is done. See
        // [`crate::mip_worker`].
        let (built, baked) = if built.textures_pending {
            let (outside, inside) = Self::baked_texture_slots(&built);
            for (slot, from_look) in outside.into_iter().chain(inside) {
                self.release_look_texture(gpu, slot, from_look);
            }
            let waiting = self.land.mip_worker.take_requested();
            self.awaiting.insert((bx, by), waiting);
            self.pending.insert((bx, by), SlotWork::Full);
            (BakedObjects::default(), false)
        } else {
            (built, wants_objects)
        };
        ws.streamer.window.set_mesh(xi, yi, SlotMesh::new(mesh));
        self.blocks.insert(
            (bx, by),
            BlockDraw {
                origin: ws.streamer.window.block_frame_origin(xi, yi),
                terrain: lb.terrain,
                cell_texture,
                // The references `generate` just took, so that whichever of the
                // two release sites this block eventually leaves through can give them back.
                cell_keys,
                merge_keys,
                cell_splat,
                opaque: built.opaque,
                blended: built.blended,
                degrade: built.degrade,
                // A block re-meshed by an LOD change keeps its batches and its placements, so
                // it also keeps its assembly; a freshly baked one has none yet. Either way the
                // next `refresh_degrade_levels` is what fills it, and it runs before any draw.
                degrade_assembled: false,
                env_cells: built.env_cells,
                building_views: built.building_views,
                baked,
                // What the bake above was taken at, so the next `SlotWork::Mesh`
                // can tell a re-stitch from a change of detail level.
                baked_side_cell_count: side_cell_count,
                scenery: built.scenery,
                buildings: built.buildings,
                statics: built.statics,
                sim: built.sim,
                cell_lights: built.cell_lights,
                emitters: built.emitters,
                // Spawned on the next `sync_objects`; a `MotionDriver` needs the shared store.
                hosts: built.hosts,
                hosts_spawned: built.hosts_spawned,
                objects_visual: built.objects_visual,
            },
        );
        Ok(())
    }

    // The residency window and the viewpoint belong to
    // [`dereth_client_runtime::world_stream::WorldStreamer`]: the `LandblockWindow` and the viewer's
    // cell index live on it, and so do the nine reads below. These are the scene's call
    // sites; `recenter`'s one presentation act -- handing the window's `SlotAction`s to the
    // bake queue -- is done here, which is why it is the one that does not simply forward.

    pub(super) fn wants_objects(&self, ws: &WorldState, xi: u32, yi: u32) -> bool {
        ws.streamer
            .wants_objects(xi, yi, self.cfg.land_radius, self.cfg.scenery_radius)
    }

    /// Queue `work` for the block in window slot `(xi, yi)`. A full build already covers a
    /// re-mesh, so `Full` is never downgraded.
    pub(super) fn want(&mut self, ws: &WorldState, xi: u32, yi: u32, work: SlotWork) {
        let Some(slot) = ws.streamer.window.slot(xi, yi) else {
            return;
        };
        self.pending
            .entry((slot.block_x, slot.block_y))
            .and_modify(|w| {
                if work == SlotWork::Full {
                    *w = SlotWork::Full;
                }
            })
            .or_insert(work);
    }

    /// The window slot currently holding `block`, if it is still in the window.
    pub(super) fn slot_of_block(
        &self,
        ws: &WorldState,
        (bx, by): (i32, i32),
    ) -> Option<(u32, u32)> {
        let (vx, vy) = ws.streamer.window.viewer_block()?;
        #[allow(clippy::cast_possible_wrap)]
        // LINT-OK: the window radius, at most 25. Not a float conversion.
        let r = ws.streamer.window.mid_radius() as i32;
        let (xi, yi) = (
            u32::try_from(bx - vx + r).ok()?,
            u32::try_from(by - vy + r).ok()?,
        );
        let slot = ws.streamer.window.slot(xi, yi)?;
        ((slot.block_x, slot.block_y) == (bx, by)).then_some((xi, yi))
    }

    /// Whether a block the player can reach -- one inside the scenery radius, where
    /// objects, interiors and their collision live -- is still waiting to be built. The
    /// portal tunnel is held while this is true; blocks further out may keep streaming in
    /// after it ends.
    #[must_use]
    pub fn loading_near_viewer(&self, ws: &WorldState) -> bool {
        self.pending
            .keys()
            .filter_map(|&b| self.slot_of_block(ws, b))
            .any(|(xi, yi)| self.wants_objects(ws, xi, yi))
    }

    pub(super) fn recenter(&mut self, ws: &mut WorldState) -> RenderSpace {
        let (space, actions) = world_step::recenter(ws, &self.cfg);
        // `Some` only on the path that actually scrolled the window. `queue` derives its
        // departed set from the actions it is handed, so handing it an empty list would
        // release every resident block -- which is why the streamer distinguishes "no
        // actions" from "did not scroll".
        if let Some(actions) = actions {
            self.queue(ws, &actions);
        }
        space
    }

    /// How many blocks currently have geometry, for the tests.
    #[must_use]
    pub fn resident_blocks(&self) -> usize {
        self.blocks.len()
    }

    // -----------------------------------------------------------------------------------
    // **Queue instrumentation.** The six queues this scene defers to a later frame phase.
    // Every one is drained inside `App::frame`, so a settled frame reads zero; a length that
    // never comes back down is a drain that stopped running — which on
    // [`Self::released_blocks`] or [`Self::released_interiors`] is a landblock's bake and
    // interior cells never being handed back at all, i.e. exactly the growth
    // the frame-state latch exists to prevent.
    //
    // Reported one queue at a time rather than as a sum because they fail for six different
    // reasons and a growth station has to name which drain stopped.
    // -----------------------------------------------------------------------------------

    /// How many surface resolves were left pending on the mip worker, ever. See
    /// [`crate::mip_worker`].
    #[must_use]
    pub fn deferred_surfaces(&self) -> u64 {
        self.land.bake.pending_surfaces
    }

    /// Window slots [`Self::stream`] still owes geometry.
    #[must_use]
    pub fn pending_slot_count(&self) -> usize {
        self.pending.len()
    }

    /// Departed bakes waiting to hand their texture links back.
    #[must_use]
    pub fn released_block_count(&self) -> usize {
        self.released_blocks.len()
    }

    /// `(width, height)` of the merged terrain surface the texture merger last composited.
    /// This is
    /// `Render.LandscapeTextureDetail`'s whole observable effect.
    #[must_use]
    pub const fn last_terrain_surface_built(&self) -> Option<(u32, u32, u32)> {
        self.land.merge.last_built
    }

    /// `(width, height, scale)` of the last object texture created — the
    /// observable result of `Render.EnvironmentTextureDetail`.
    #[must_use]
    pub const fn last_object_texture_built(&self) -> Option<(u32, u32, u32)> {
        self.land.bake.last_texture_built
    }

    /// `(uploads, texels uploaded, texels at the source extent, uploads actually downscaled)`
    /// through the object-texture path, cumulative over the session.
    ///
    /// The population `Render.EnvironmentTextureDetail` has to be measured
    /// against, because a single sample can land on the one arm
    /// `dereth_render::texture::scale_surface` declines to resample. The **pair** of texel sums
    /// is what separates "the scale moved" from "the population moved": they are equal by
    /// construction at `FULL_RES`, whatever mix of surfaces the rebuild happened to touch.
    #[must_use]
    pub const fn object_texture_census(&self) -> (u64, u64, u64, u64) {
        (
            self.land.bake.texture_uploads,
            self.land.bake.texture_upload_texels,
            self.land.bake.texture_source_texels,
            self.land.bake.textures_downscaled,
        )
    }

    /// [`dereth_client_runtime::render_prefs::RenderPreferences`]'s shadow bank, i.e. what the last preference
    /// poll believes the renderer is running at.
    /// A test that could only read [`Self::cfg`] back would be reading the
    /// value the options page wrote, not the one the renderer acted on.
    #[must_use]
    pub const fn render_shadow(&self) -> dereth_client_runtime::render_prefs::RenderPreferences {
        self.render_shadow
    }

    /// **One block's** bake: the two populations counted
    /// separately, the detail level they were taken at, and whether the bake ran at all.
    /// `None` when the block is not resident.
    ///
    /// [`SceneStats::scenery_objects`] is a sum over the window, and a sum cannot say whether
    /// a *particular* block re-baked when it scrolled inward.
    /// [`BlockBake::baked`] is the field that makes a zero a measurement: without it, "this
    /// block's scenery was refused by the full-detail guard" and "this slot is outside
    /// `SceneConfig::scenery_radius` and bakes nothing at all" read identically.
    #[must_use]
    pub fn block_bake(&self, block: (i32, i32)) -> Option<BlockBake> {
        self.blocks.get(&block).map(|b| BlockBake {
            scenery: b.scenery,
            statics: b.statics,
            buildings: b.buildings,
            side_cell_count: b.baked_side_cell_count,
            baked: b.baked,
        })
    }

    /// Every meshed window slot as
    /// `(xi, yi, (block_x, block_y), side_cell_count, the ring's side_cell_count)`.
    ///
    /// The last two are what a seam looks like from a test: a block that scrolled into a
    /// different ring and kept the detail of the one it left.
    #[must_use]
    pub fn window_rings(&self, ws: &WorldState) -> Vec<WindowRing> {
        let w = ws.streamer.window.mid_width();
        let expected = ws.streamer.window.ring_map();
        let mut out = Vec::new();
        for xi in 0..w {
            for yi in 0..w {
                let Some(slot) = ws.streamer.window.slot(xi, yi) else {
                    continue;
                };
                let Some(mesh) = slot.mesh.as_ref().and_then(SlotMesh::get::<LandblockMesh>) else {
                    continue;
                };
                #[allow(clippy::cast_possible_wrap)]
                // LINT-OK: window indices, bounded by mid_width <= 31. Not a float conversion.
                let (a, b) = (xi as i32, yi as i32);
                out.push(WindowRing {
                    xi: a,
                    yi: b,
                    block: (slot.block_x, slot.block_y),
                    side_cell_count: mesh.side_cell_count,
                    expected_side_cell_count: expected[(w * xi + yi) as usize],
                });
            }
        }
        out
    }
}

impl AppearanceKey {
    pub(super) fn new(setup: DataId, od: &dereth_protocol::types::ObjDesc) -> Self {
        Self {
            setup,
            palette: od.palette_id,
            subpalettes: od
                .subpalettes
                .iter()
                .map(|x| (x.sub_id, x.offset, x.num_colors))
                .collect(),
            textures: od
                .texture_changes
                .iter()
                .map(|x| (x.part_index, x.old_tex_id, x.new_tex_id))
                .collect(),
            parts: od
                .anim_part_changes
                .iter()
                .map(|x| (x.part_index, x.part_id))
                .collect(),
        }
    }
}
