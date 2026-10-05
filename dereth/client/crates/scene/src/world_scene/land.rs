//! Landblock construction and terrain submission.

use super::*;

impl LandContext {
    /// Generate one block's mesh and its vertex lighting, then the block's merged
    /// land surfaces.
    ///
    /// The merge cache is shared across the whole window, so a block entering the window
    /// usually uploads only the handful of textures its own terrain pairs need.
    pub(super) fn generate(
        &mut self,
        store: &RetailDatStore,
        gpu: &mut Gpu,
        lb: &CellLandblock,
        spec: SlotSpec,
    ) -> Result<GeneratedBlock, WorldError> {
        // A terrain type the ground has no picture for takes its neighbours' for the surfaces
        // alone; the water stays the cells' own.
        let filled = dereth_world_render::land::fill::fill_undrawn_terrain(lb, self.drawn);
        let surface_lb = filled.as_ref().unwrap_or(lb);
        let mut mesh = generate_landblock_with_table(
            surface_lb,
            self.ground.as_deref().unwrap_or(&self.region),
            &self.table,
            spec.block_x,
            spec.block_y,
            spec.lod_div,
            spec.dir,
        );
        if filled.is_some() {
            (mesh.cell_water, mesh.water_type) = dereth_world_render::land::water::calc_water(
                &lb.terrain,
                usize::from(mesh.side_cell_count),
            );
        }
        // `generate` recomputes the vertex lighting after every geometry rebuild.
        bake_lighting(&mut mesh, &self.lighting);

        let merge_keys: Vec<MergeKey> = mesh.cell_keys.iter().map(|&(k, _)| k).collect();
        if let Some(ps) = self.pal_shift.clone() {
            let cell_texture = self.pal_shift_composites(store, gpu, &ps, surface_lb, &mesh)?;
            return Ok((
                mesh,
                cell_texture,
                merge_keys.clone(),
                merge_keys,
                Vec::new(),
            ));
        }
        let (cell_texture, cell_keys) = if self.composites_wanted {
            (
                self.composites(store, gpu, &merge_keys)?,
                merge_keys.clone(),
            )
        } else {
            (Vec::new(), Vec::new())
        };
        let cell_splat = if self.splat_wanted {
            self.splat_layers(store, gpu, &merge_keys)?
        } else {
            Vec::new()
        };
        Ok((mesh, cell_texture, cell_keys, merge_keys, cell_splat))
    }

    /// Each of `keys`' composite, taking one reference per cell on the shared cache.
    ///
    /// One reference per cell, and the key kept so the block can give that
    /// same reference back. Teardown visits every one of `side_cell_count²` cells,
    /// returning the link acquired at terrain selection: cache hits increment the
    /// reference count, while a newly created surface starts with one reference.
    pub(super) fn composites(
        &mut self,
        store: &RetailDatStore,
        gpu: &mut Gpu,
        keys: &[MergeKey],
    ) -> Result<Vec<Option<TextureSlot>>, WorldError> {
        let ground_store = self.ground_store.clone();
        let store = ground_store.as_ref().unwrap_or(store);
        // One cached texture per distinct merge key.
        let sources = DatTerrainTextures {
            textures: TextureStore::with_environment_texture_detail(
                store,
                self.bake.environment_texture_detail,
            ),
            memo: RefCell::new(&mut self.sources),
        };
        let mut cell_texture = Vec::with_capacity(keys.len());
        let mut error: Option<RenderError> = None;
        for key in keys {
            let h = if self.gpu_merge && gpu.terrain_merge_supported() {
                let resident = &mut self.gpu_merge_sources;
                let gpu_merge = &mut self.gpu_merge;
                let gpu = &mut *gpu;
                let sources = &sources;
                self.merge
                    .get_or_build_with(&self.tex_merge, *key, &mut |plan| {
                        match merge_on_device(gpu, resident, sources, plan) {
                            Ok(slot) => TextureHandle(slot.0),
                            Err(e) => {
                                // The two paths are bit-identical, so falling back loses nothing;
                                // it is for the rest of the session, so the line prints once.
                                tracing::warn!(
                                    "composing the landscape on the CPU from here on: {e}"
                                );
                                *gpu_merge = false;
                                let img = dereth_world_render::land::merge::execute_merge_plan(
                                    plan, sources,
                                );
                                let mut backend = TextureUploader {
                                    gpu: &mut *gpu,
                                    error: None,
                                };
                                dereth_primitives::RenderBackend::upload_texture(
                                    &mut backend,
                                    &dereth_primitives::TextureData {
                                        width: img.width,
                                        height: img.height,
                                        format: dereth_primitives::TextureFormat::Bgra8,
                                        levels: vec![img.into_bytes()],
                                    },
                                )
                            }
                        }
                    })
            } else {
                let mut backend = TextureUploader {
                    gpu: &mut *gpu,
                    error: None,
                };
                let h = self
                    .merge
                    .get_or_build(&self.tex_merge, *key, &mut backend, &sources);
                if let Some(e) = backend.error.take() {
                    error.get_or_insert(e);
                }
                h
            };
            cell_texture.push((h.0 != NO_TEXTURE).then_some(TextureSlot(h.0)));
        }
        if let Some(e) = error {
            return Err(WorldError::Render(e.to_string()));
        }
        Ok(cell_texture)
    }

    /// Each cell's palette-shift surface, composed on the CPU once per key and shared through
    /// the same surface cache (one reference per cell). The texture and rotation are chosen
    /// per cell at its global position, so the first cell to need a key decides its picture,
    /// as the surface cache's first request does.
    pub(super) fn pal_shift_composites(
        &mut self,
        store: &RetailDatStore,
        gpu: &mut Gpu,
        ps: &dereth_assets::region::PalShift,
        lb: &CellLandblock,
        mesh: &dereth_world_render::land::mesh::LandblockMesh,
    ) -> Result<Vec<Option<TextureSlot>>, WorldError> {
        use dereth_world_render::land::merge::{cell_rotation_keys, cell_x, cell_y};
        use dereth_world_render::land::palshift;
        let ground_store = self.ground_store.clone();
        let store = ground_store.as_ref().unwrap_or(store);
        let lookup = dereth_assets::texture_lookup::TextureLookup::new(store, 0);
        let side = usize::from(mesh.side_cell_count);
        let mut out = Vec::with_capacity(side * side);
        let mut error: Option<String> = None;
        for i in 0..side {
            for j in 0..side {
                let (keys, _) = cell_rotation_keys(
                    lb,
                    self.ground.as_deref().unwrap_or(&self.region),
                    side,
                    i,
                    j,
                );
                let choice = palshift::select(ps, &keys, cell_x(lb, i), cell_y(lb, j));
                let Some(texture) = ps.textures.get(choice.texture) else {
                    out.push(None);
                    continue;
                };
                let h = self.merge.get_or_build_keyed(choice.key, &mut || {
                    let image = lookup.resolve(texture.tex_gid).ok().and_then(|(_, rs, b)| {
                        let indices = rs.payload(&b)?.to_vec();
                        let base = rs
                            .default_palette_id
                            .and_then(|p| lookup.palette(p).ok())
                            .map(|p| p.colors_argb)
                            .unwrap_or_default();
                        let subs = palshift::sub_palettes(ps, &choice);
                        let palette = palshift::compose_palette(&base, &subs, &|id| {
                            lookup.palette(id).ok().map(|p| p.colors_argb)
                        });
                        Some((rs.width, rs.height, palshift::expand(&indices, &palette)))
                    });
                    let Some((width, height, pixels)) = image else {
                        error.get_or_insert(format!(
                            "the palette-shift texture {} does not resolve",
                            texture.tex_gid
                        ));
                        return (TextureHandle(NO_TEXTURE), 0);
                    };
                    let mut backend = TextureUploader {
                        gpu: &mut *gpu,
                        error: None,
                    };
                    let h = dereth_primitives::RenderBackend::upload_texture(
                        &mut backend,
                        &dereth_primitives::TextureData {
                            width,
                            height,
                            format: dereth_primitives::TextureFormat::Bgra8,
                            levels: vec![pixels],
                        },
                    );
                    if let Some(e) = backend.error.take() {
                        error.get_or_insert(e.to_string());
                    }
                    (h, width)
                });
                out.push((h.0 != NO_TEXTURE).then_some(TextureSlot(h.0)));
            }
        }
        if let Some(e) = error {
            return Err(WorldError::Render(e));
        }
        Ok(out)
    }

    /// Each of `keys`' splat layers, made once per merge key and shared.
    pub(super) fn splat_layers(
        &mut self,
        store: &RetailDatStore,
        gpu: &mut Gpu,
        keys: &[MergeKey],
    ) -> Result<Vec<Option<Arc<TerrainSplat>>>, WorldError> {
        let ground_store = self.ground_store.clone();
        let store = ground_store.as_ref().unwrap_or(store);
        let Self {
            sources,
            splat_sources,
            splats,
            tex_merge,
            merge,
            bake,
            ..
        } = self;
        let src = DatTerrainTextures {
            textures: TextureStore::with_environment_texture_detail(
                store,
                bake.environment_texture_detail,
            ),
            memo: RefCell::new(sources),
        };
        let mut cells = Vec::with_capacity(keys.len());
        for key in keys {
            let splat = match splats.get(key) {
                Some(s) => Arc::clone(s),
                None => {
                    let plan =
                        dereth_world_render::land::merge::merge_plan(tex_merge, *key, merge.shift);
                    // The texels a composite at this detail level would hold for a cell,
                    // before the distance reduction the splat draw leaves to the mips.
                    let size = dereth_world_render::land::merge::merged_texture_size(
                        tex_merge.base_tex_size,
                        merge.shift,
                        1,
                    );
                    let s = Arc::new(splat_of(gpu, splat_sources, &src, &plan, size)?);
                    splats.insert(*key, Arc::clone(&s));
                    s
                }
            };
            cells.push(Some(splat));
        }
        Ok(cells)
    }

    /// Dynamic scenery, buildings and static objects for one block, baked into
    /// **block-local** vertices.
    ///
    /// Block-local coordinates use the block id and an identity frame, letting the window scroll
    /// without touching a vertex.
    #[allow(clippy::too_many_arguments)]
    // LINT-OK: the block's identity is four of them (`lb`, `mesh`, `bx`, `by`) and they are
    // three different views of the same block that the callee needs separately;
    // `cell_statics` is the eighth and `part_degrades` the ninth.
    pub(super) fn bake(
        &mut self,
        store: &RetailDatStore,
        gpu: &mut Gpu,
        lb: &CellLandblock,
        mesh: &LandblockMesh,
        bx: i32,
        by: i32,
        cell_statics: bool,
        part_degrades: bool,
        degrade_levels: bool,
        static_billboards: bool,
        lod_object_guard: bool,
    ) -> Result<BakedObjects, WorldError> {
        let pending_before = self.bake.pending_surfaces;
        let look_pending_before = self
            .objects
            .as_ref()
            .map_or(0, |l| l.cache.pending_surfaces);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // LINT-OK: `read_landblock` already bounded both to 0..=0xFE. Not a float conversion.
        let block = ((bx as u16) << 8) | (by as u16);
        // **What the block holds is the world builder's; this bake draws it.** The scenery,
        // the landblock-info statics resolved to their land cells, the collision record of
        // each and the house restrictions all come from `land_content`, which a presentation
        // with no device calls too. Objects of every kind need full detail there, not scenery
        // alone: static-object and building initialization carry the same guard, so a block
        // on an outer ring is bare terrain.
        //
        // The bake turns the same placements into batches in the same order -- scenery, then
        // building shells, then statics -- and pairs every placement whose setup names a
        // default script with its collision record by index: static initialization creates
        // one physics body per placement and queues its default script on that same body.
        let LandContent {
            lbi,
            full_detail,
            placed,
            object_slots,
            land_statics,
            mut cell_restrictions,
        } = land_content(store, &self.region, lb, mesh, bx, by, lod_object_guard);

        // The scenery, the building shells and the statics, each drawn wholly from one era:
        // the objects' look when it is another era's and stands for the object
        // ([`ObjectLook::takes`]), the world's otherwise. Their collision records above are
        // the world's either way.
        let objects_visual = self.objects.is_some();
        let takes = |id: DataId, building: bool| {
            self.objects.as_ref().is_some_and(|l| l.takes(id, building))
        };
        // (object, frame, scale, building shell, drawn from the look), in bake order.
        let mut items: Vec<(DataId, Frame, f32, bool, bool)> = Vec::new();
        let mut emitters: Vec<EmitterPlacement> = Vec::new();
        for (i, p) in placed.iter().enumerate() {
            items.push((p.gfxobj, p.frame, p.scale, false, takes(p.gfxobj, false)));
            if crate::particles::has_default_script(store, p.gfxobj) {
                // A scenery placement's collision record is `land_statics[i]`.
                emitters.push(EmitterPlacement {
                    cell: p.cell,
                    setup: p.gfxobj,
                    frame: p.frame,
                    slot: StaticSlot::Land(i),
                    body: None,
                });
            }
        }
        let (mut buildings, mut statics) = (0usize, 0usize);
        if let Some(info) = lbi.as_ref().filter(|_| full_detail) {
            buildings = info.buildings.len();
            statics = info.objects.len();
            for b in &info.buildings {
                // Building drawing sets the building-part flag only for the shell.
                // The existing baker still resolves all parts; retail draws parts[0] here.
                items.push((b.id, b.frame, 1.0, true, takes(b.id, true)));
            }
            for (o, slot) in info.objects.iter().zip(&object_slots) {
                items.push((o.id, o.frame, 1.0, false, takes(o.id, false)));
                // A placement whose land-cell lookup answered nothing is destroyed during
                // static-object initialization and never reaches registration, so it has no
                // record to pair with.
                if let Some((cell, index)) = *slot {
                    if crate::particles::has_default_script(store, o.id) {
                        emitters.push(EmitterPlacement {
                            cell,
                            setup: o.id,
                            frame: o.frame,
                            slot: StaticSlot::Land(index),
                            body: None,
                        });
                    }
                }
            }
        }
        // The world's side first, then the look's, each through its own files and cache; the
        // look's degrading placements follow the world's, so its batches' chunks are moved up
        // by the world's count.
        let mut baked: BakedGroups = (Vec::new(), Vec::new(), Vec::new());
        for from_look in [false, true] {
            if !items.iter().any(|it| it.4 == from_look) {
                continue;
            }
            let (draw_store, draw_cache): (&RetailDatStore, &mut BakeCache) =
                match (from_look, self.objects.as_mut()) {
                    (true, Some(look)) => (&*look.files, &mut look.cache),
                    _ => (store, &mut self.bake),
                };
            let mut baker = ObjectBaker::new(
                draw_store,
                draw_cache,
                part_degrades,
                degrade_levels,
                static_billboards,
            );
            baker.from_look = from_look;
            baker.defer = if self.defer_mips {
                Some(&mut self.mip_worker)
            } else {
                None
            };
            for (id, frame, scale, building, _) in items.iter().filter(|it| it.4 == from_look) {
                baker.add_object_kind(*id, frame, *scale, *building);
            }
            let (mut opaque, mut blended, degrade) = baker
                .finish(gpu)
                .map_err(|e| WorldError::Render(e.to_string()))?;
            // LINT-OK: a placement count bounded by the block's part count. Not a float.
            #[allow(clippy::cast_possible_truncation)]
            let offset = baked.2.len() as u32;
            for b in opaque.iter_mut().chain(blended.iter_mut()) {
                for c in &mut b.chunks {
                    if c.placement != NO_PLACEMENT {
                        c.placement += offset;
                    }
                }
            }
            baked.0.append(&mut opaque);
            baked.1.append(&mut blended);
            baked.2.extend(degrade);
        }
        let (opaque, blended, degrade) = baked;
        // A building's rooms keep the world's look with a shell that keeps it.
        let shell_list = lbi.as_ref().map_or(&[][..], |i| &i.buildings[..]);
        let shells: Vec<bool> = shell_list
            .iter()
            .map(|b| self.objects.as_ref().is_some_and(|l| l.takes(b.id, true)))
            .collect();
        // The interior cells' own baked objects are found on the same pass that
        // builds their meshes, and their scripted ones join this block's `emitters` — a
        // dungeon brazier's flame comes exactly as a lamp post's
        // does outdoors.
        let (env_cells, cell_statics, cell_lights) = self.bake_env_cells(
            store,
            gpu,
            block,
            (shell_list, &shells),
            cell_statics,
            part_degrades,
            degrade_levels,
            static_billboards,
            &mut emitters,
            &mut cell_restrictions,
        )?;
        // The outdoor portal pass into a building's interior is
        // reached only through a sorted cell's building link,
        // and building initialization creates no building object
        // on a coarse block, so there is nothing there to open. Gated with the shell.
        let building_views = lbi
            .as_ref()
            .filter(|_| full_detail)
            .map_or_else(Vec::new, |info| bake_building_views(store, block, info));
        let look_pending = self
            .objects
            .as_ref()
            .map_or(0, |l| l.cache.pending_surfaces);
        Ok(BakedObjects {
            textures_pending: self.bake.pending_surfaces != pending_before
                || look_pending != look_pending_before,
            objects_visual,
            opaque,
            blended,
            degrade,
            env_cells,
            building_views,
            scenery: placed.len(),
            buildings,
            statics,
            sim: BlockStatics {
                cell_statics,
                land_statics,
                cell_restrictions,
                statics_registered: false,
                restrictions_registered: false,
            },
            cell_lights,
            emitters,
            hosts: Vec::new(),
            hosts_spawned: false,
        })
    }

    /// Build one block's interior cells for drawing, including each cell mesh.
    ///
    /// Each cell record's polygons are triangulated against the **cell's own**
    /// surface list and composed into block-local space by the cell's placement frame.
    ///
    /// The second half of a cell is its static tables,
    /// pictures, stairs and fireplaces the cell bakes in. They are baked into per-cell batches
    /// rather than into the block's, because a cell's objects are drawn only when the portal
    /// traversal reaches that cell (cell drawing's second loop,
    /// the per-cell object draw) — dropping them into `BlockDraw::opaque` would draw
    /// a dungeon's whole furniture through every wall. The frames are **not** composed with
    /// the cell's own: see [`dereth_world_data::env_cells::CellStatic`].
    ///
    /// With another era's look ([`ObjectLook`]), a room the verdicts say is the same draws
    /// from that era's record of it unless it belongs to one of `shells`' buildings whose
    /// shell keeps the world's look (`shells`: the block's buildings and whether each shell
    /// takes the look).
    #[allow(clippy::too_many_arguments)]
    // LINT-OK: the eighth is `degrade_levels`, which joins `part_degrades`
    // and `want_statics` as a `SceneConfig` control the bake has to be told about; the ninth
    // the shells' verdicts, which the rooms follow.
    #[allow(clippy::type_complexity)] // a one-off tuple, named where it is read
    pub(super) fn bake_env_cells(
        &mut self,
        store: &RetailDatStore,
        gpu: &mut Gpu,
        block: u16,
        shells: (&[dereth_assets::world::BuildInfo], &[bool]),
        want_statics: bool,
        part_degrades: bool,
        degrade_levels: bool,
        static_billboards: bool,
        emitters: &mut Vec<EmitterPlacement>,
        restrictions: &mut Vec<(CellId, dereth_primitives::ObjectId)>,
    ) -> Result<
        (
            Vec<EnvCellDraw>,
            Vec<dereth_world_data::env_cells::CellStatic>,
            Vec<CellLightObj>,
        ),
        WorldError,
    > {
        let decoded = self.cells.load_block(store, block);
        if decoded.is_empty() {
            return Ok((Vec::new(), Vec::new(), Vec::new()));
        }
        // The interior half of the house barrier and every cell's statics are the world
        // builder's (`interior_content`); this bake draws the cells and the statics.
        let (interior_restrictions, per_cell_statics) = interior_content(&decoded, want_statics);
        restrictions.extend(interior_restrictions);
        let textures = TextureStore::with_environment_texture_detail(
            store,
            self.bake.environment_texture_detail,
        );
        // The other era's rooms: its files, and the rooms held to the world's look by their
        // shells.
        let interiors = self.objects.as_ref().and_then(|l| l.interiors.clone());
        let look_files = self.objects.as_ref().map(|l| Arc::clone(&l.files));
        let look_textures = interiors.as_deref().map(|i| {
            TextureStore::with_environment_texture_detail(i, self.bake.environment_texture_detail)
        });
        let held: HashSet<u32> = if interiors.is_some() {
            let (buildings, takes) = shells;
            dereth_world_data::env_cells::building_cells(block, buildings, &decoded)
                .into_iter()
                .zip(takes)
                .filter(|(_, takes)| !**takes)
                .flat_map(|(cells, _)| cells.into_iter().map(|c| c.0))
                .collect()
        } else {
            HashSet::new()
        };
        let mut out = Vec::with_capacity(decoded.len());
        let mut all_statics: Vec<dereth_world_data::env_cells::CellStatic> = Vec::new();
        let mut all_lights: Vec<CellLightObj> = Vec::new();
        // Per static id, decoded once per block -- a dungeon
        // stands the same wall torch forty times.
        let mut setup_lights: HashMap<DataId, Vec<LightInfo>> = HashMap::new();
        for (d, statics) in decoded.iter().zip(per_cell_statics) {
            // The vertices are emitted white here and `burn_static_lighting` writes
            // the static-light burn's colour over them once the pool is
            // known; `minimize_envcell_lighting` picks the dynamic lights at draw.
            //
            // The room is the other era's record of it when the verdicts allow: its
            // geometry, the same as the world's, triangulated against that era's own surface
            // list and drawn through that era's files and cache.
            let room = match (self.objects.as_mut(), interiors.as_deref()) {
                (Some(look), Some(files))
                    if look.identity.same_room(d.id.0) && !held.contains(&d.id.0) =>
                {
                    look.cells.load_cell(files, d.id)
                }
                _ => None,
            };
            let defer = if self.defer_mips {
                Some(&mut self.mip_worker)
            } else {
                None
            };
            let meshes = match (
                &room,
                self.objects.as_mut(),
                interiors.as_deref(),
                &look_textures,
            ) {
                (Some(r), Some(look), Some(files), Some(tex)) => {
                    let groups = dereth_client_runtime::models::triangulate_cell(
                        &r.structure,
                        &r.cell.surfaces,
                    );
                    build_meshes_with(
                        files,
                        &mut look.cache,
                        tex,
                        gpu,
                        &groups,
                        None,
                        Some(&d.cell.frame),
                        defer,
                    )
                }
                _ => {
                    let groups = dereth_client_runtime::models::triangulate_cell(
                        &d.structure,
                        &d.cell.surfaces,
                    );
                    build_meshes_with(
                        store,
                        &mut self.bake,
                        &textures,
                        gpu,
                        &groups,
                        None,
                        Some(&d.cell.frame),
                        defer,
                    )
                }
            }
            .map_err(|e| WorldError::Render(e.to_string()))?;
            let from_look = room.is_some();
            let base = d.id.0 & 0xFFFF_0000;
            let portals = d
                .cell
                .portals
                .iter()
                .filter_map(|p| {
                    let poly = d.structure.polygons.get(p.polygon_id as usize)?;
                    let vertices: Vec<Vec3> = poly
                        .vertex_ids
                        .iter()
                        .filter_map(|&i| d.structure.vertex_array.vertices.get(i as usize))
                        .map(|v| v.position)
                        .collect();
                    // Plane construction is the physics crate's, and the traversal needs
                    // the same plane physics uses.
                    let plane = dereth_physics::geom::Polygon::new(vertices.clone()).plane;
                    Some(EnvPortal {
                        // Portal polarity is `portal_side = ((~flags) >> 1) & 1`.
                        portal_side: u8::from((!p.flags >> 1) & 1 != 0),
                        other_cell_id: if p.other_cell_id == 0xFFFF_FFFF {
                            dereth_world_render::cells::portal_view::OUTDOORS
                        } else {
                            base | (p.other_cell_id & 0xFFFF)
                        },
                        other_portal_id: i32::from(p.other_portal_id),
                        exact_match: p.flags & 1 != 0,
                        vertices,
                        plane_normal: plane.normal,
                        plane_d: plane.d,
                    })
                })
                .collect();
            // Interior static-object initialization's draw half. One
            // `ObjectBaker` per cell, so the cell owns its own batches; `add_object` is the
            // same call landscape-static placements go through, at the
            // placement's own block-local frame and `gfxobj_scale` 1 (an environment-cell static
            // carries no scale, exactly as a landblock-record static does not). With
            // `SceneConfig::cell_statics` off the cell has no statics at all: no triangles, no
            // emitters, no bodies, and none of the furniture's textures.
            // This cell's placements land at `all_statics[statics_base + i]`
            // — the `extend_from_slice` at the end of the cell's block is what puts them
            // there — and `all_statics` is returned as `BlockStatics::cell_statics`, which is
            // the slice `init_cell_statics` hands to `CellStaticObjects::init`. So
            // `statics_base + i` is the client's `i` in `static_objects[i]`.
            let statics_base = all_statics.len();
            // In a room drawn from the other era, each piece of furniture draws wholly from
            // the era the objects' verdicts give it, through that era's files and cache: the
            // world's pieces first, then the look's, whose degrading placements follow the
            // world's in the cell's list.
            let mut statics_opaque = Vec::new();
            let mut statics_blended = Vec::new();
            let mut cell_degrade = Vec::new();
            for side_look in [false, true] {
                let mine: Vec<&dereth_world_data::env_cells::CellStatic> = statics
                    .iter()
                    .filter(|s| {
                        let takes = from_look
                            && self.objects.as_ref().is_some_and(|l| l.takes(s.id, false));
                        takes == side_look
                    })
                    .collect();
                if mine.is_empty() {
                    continue;
                }
                let (draw_store, draw_cache): (&RetailDatStore, &mut BakeCache) =
                    match (side_look, self.objects.as_mut(), look_files.as_deref()) {
                        (true, Some(look), Some(files)) => (files, &mut look.cache),
                        _ => (store, &mut self.bake),
                    };
                let mut baker = ObjectBaker::new(
                    draw_store,
                    draw_cache,
                    part_degrades,
                    degrade_levels,
                    static_billboards,
                );
                baker.from_look = side_look;
                baker.defer = if self.defer_mips {
                    Some(&mut self.mip_worker)
                } else {
                    None
                };
                for s in mine {
                    baker.add_object(s.id, &s.frame, 1.0);
                }
                let (mut opaque, mut blended, degrade) = baker
                    .finish(gpu)
                    .map_err(|e| WorldError::Render(e.to_string()))?;
                // LINT-OK: a placement count bounded by the cell's part count. Not a float.
                #[allow(clippy::cast_possible_truncation)]
                let offset = cell_degrade.len() as u32;
                for b in opaque.iter_mut().chain(blended.iter_mut()) {
                    for c in &mut b.chunks {
                        if c.placement != NO_PLACEMENT {
                            c.placement += offset;
                        }
                    }
                }
                statics_opaque.append(&mut opaque);
                statics_blended.append(&mut blended);
                cell_degrade.extend(degrade);
            }
            for (i, s) in statics.iter().enumerate() {
                if crate::particles::has_default_script(store, s.id) {
                    emitters.push(EmitterPlacement {
                        cell: s.cell,
                        setup: s.id,
                        frame: s.frame,
                        slot: StaticSlot::Cell(statics_base + i),
                        body: None,
                    });
                }
                // Light initialization for the static:
                // Object creation with `(id, 0, 0)` leaves the constructor's `LIGHTING_ON_PS` set and
                // object initialization sets `STATIC_PS`, so every entry becomes a
                // light object with `state |= 1` at the object's frame, registered with the cell.
                let lights = setup_lights
                    .entry(s.id)
                    .or_insert_with(|| read_setup_lights(store, s.id));
                for info in lights.iter() {
                    all_lights.push(CellLightObj {
                        cell: d.id,
                        info: *info,
                        frame: s.frame,
                        is_static: true,
                    });
                }
            }
            all_statics.extend_from_slice(&statics);
            out.push(EnvCellDraw {
                id: d.id,
                frame: d.cell.frame,
                portals,
                meshes,
                from_look,
                statics: statics_opaque,
                statics_blended,
                degrade: cell_degrade,
                burned_count: None,
            });
        }
        Ok((out, all_statics, all_lights))
    }
}

impl PaletteComposition {
    /// Build the palette. `None` when the base palette is missing; the count is how many ranges
    /// could not be applied, which is a data or layout error rather than bad input — hence
    /// [`SceneStats::palette_range_failures`], which the acceptance test asserts is zero.
    pub(super) fn build(&self, textures: &TextureStore<'_>) -> Option<(ExpandedPalette, u32)> {
        let mut p = textures.palette(self.base)?.make_modified();
        let mut failed = 0u32;
        for (i, &(sub, offset, length)) in self.ranges.iter().enumerate() {
            // Each entry in the range is copied from the same index of the sub-palette, so
            // the source is read at the **same absolute indices**, not from the sub-palette's
            // start. Offsets and lengths are in 8-entry units, so both are multiplied by 8.
            let src = if self.from_look.get(i).copied().unwrap_or(false) {
                textures.look_palette(sub)
            } else {
                textures.palette(sub)
            };
            let Some(src) = src else {
                failed += 1;
                continue;
            };
            let start = (offset as usize) * dereth_render::palette::PALETTE_REPLICATION;
            let Some(tail) = src.0.get(start..) else {
                failed += 1;
                continue;
            };
            #[allow(clippy::cast_possible_truncation)]
            // LINT-OK: both are wire bytes widened to u32 on the way in. Not a float
            // conversion.
            let ok = p.apply_subpalette(offset as u16, length as u16, tail);
            if !ok {
                failed += 1;
            }
        }
        Some((p, failed))
    }
}

/// Build `plan`'s composite with the device's compositor, making each source it names
/// resident on first use. A missing alpha map drops its overlay and a missing texture is the
/// debug colour, exactly as [`dereth_world_render::land::merge::execute_merge_plan`] does, so the
/// result is bit-identical to the CPU composite.
pub(super) fn merge_on_device(
    gpu: &mut Gpu,
    resident: &mut HashMap<DataId, Option<MergeSource>>,
    src: &dyn TerrainTextureSource,
    plan: &MergePlan,
) -> Result<TextureSlot, RenderError> {
    let mut get = |id: DataId| -> Result<Option<MergeSource>, RenderError> {
        if let Some(r) = resident.get(&id) {
            return Ok(*r);
        }
        let r = match src.image(id) {
            Some(img) => {
                Some(gpu.upload_merge_source(img.width, img.height, img.pixels.as_flattened())?)
            }
            None => None,
        };
        resident.insert(id, r);
        Ok(r)
    };
    let base = match plan.base {
        Some(id) => get(id)?,
        None => None,
    };
    let mut overlays = Vec::with_capacity(plan.overlays.len());
    for o in &plan.overlays {
        let Some(alpha) = get(o.alpha)? else { continue };
        let tex = match o.tex {
            Some(id) => get(id)?,
            None => None,
        };
        overlays.push(TerrainMergeOverlay {
            alpha,
            rotation: o.rotation as u32,
            tex,
            tiling: o.tiling,
        });
    }
    let job = TerrainMergeJob {
        size: plan.size,
        base,
        base_tiling: plan.base_tiling,
        overlays,
    };
    gpu.merge_terrain_texture(dereth_render::TextureKey::UNCACHED, &job)
}

/// `plan`'s layers as a splat draw's textures, uploading each source as an ordinary texture
/// on first use.
pub(super) fn splat_of(
    gpu: &mut Gpu,
    resident: &mut HashMap<DataId, Option<TextureSlot>>,
    src: &dyn TerrainTextureSource,
    plan: &MergePlan,
    size: u32,
) -> Result<TerrainSplat, WorldError> {
    // Each source is uploaded at the texels the composite at `size` carries of it
    // (`Render.LandscapeTextureDetail`), so the setting lowers the splat draw's detail as it
    // does the composites'. A change of the setting flushes these, as it does the composites.
    let mut get = |id: DataId, tiling: u32| -> Result<Option<TextureSlot>, WorldError> {
        if let Some(r) = resident.get(&id) {
            return Ok(*r);
        }
        let r = match src.image(id) {
            Some(img) => {
                let shrunk = dereth_world_render::land::merge::source_at_scale(&img, size, tiling);
                let img = shrunk.as_ref().unwrap_or(&img);
                Some(
                    gpu.upload_imgtex_keyed(
                        dereth_render::TextureKey::UNCACHED,
                        &dereth_primitives::TextureData {
                            width: img.width,
                            height: img.height,
                            format: dereth_primitives::TextureFormat::Bgra8,
                            levels: vec![img.pixels.as_flattened().to_vec()],
                        },
                    )
                    .map_err(|e| WorldError::Render(e.to_string()))?,
                )
            }
            None => None,
        };
        resident.insert(id, r);
        Ok(r)
    };
    let base = match plan.base {
        Some(id) => get(id, plan.base_tiling)?,
        None => None,
    };
    let mut overlays = Vec::with_capacity(plan.overlays.len());
    for o in &plan.overlays {
        let Some(alpha) = get(o.alpha, 1)? else {
            continue;
        };
        let tex = match o.tex {
            Some(id) => get(id, o.tiling)?,
            None => None,
        };
        overlays.push(TerrainSplatOverlay {
            alpha,
            rotation: o.rotation as u32,
            tex,
            tiling: o.tiling,
        });
    }
    Ok(TerrainSplat {
        base,
        base_tiling: plan.base_tiling,
        overlays,
    })
}

impl TerrainTextureSource for DatTerrainTextures<'_> {
    fn image(&self, id: DataId) -> Option<Arc<Bgra8>> {
        let mut memo = self.memo.borrow_mut();
        memo.entry(id)
            .or_insert_with(|| self.textures.bgra8(id).ok().map(Arc::new))
            .clone()
    }
}
impl SceneDraw {
    /// The outdoor pass: draw sky pass 0, every resident
    /// block's terrain, buildings and statics, the alpha list, then draw sky pass 1.
    ///
    /// **This is its own function because it has two callers, and retail's has
    /// two callers.** Normal world rendering reaches it directly only on
    /// the **outdoors** branch (`(viewer.objcell_id & 0xFFFF) < 0x100`); the indoor branch
    /// draws the interior, whose cell-rendering first step is: if the outside view has any
    /// views, install it as the active portal list and draw the landscape.
    /// So an indoor viewer gets the outdoor world **only** through a portal chain that
    /// reached one, and a dungeon with no outdoor portal in view gets none of it — no
    /// terrain, no scenery and **no sky**. See [`Self::draw_inside`] and
    /// [`SceneConfig::outside_view_gate`].
    ///
    /// # Errors
    /// Any failure from the runtime.
    ///
    /// # Returns
    /// The exact building-interior cells this landscape pass reached.
    pub(super) fn draw_landscape(
        &self,
        ws: &WorldState,
        gpu: &mut Gpu,
        per_frame: &PerFrameConstants,
        sky_per_frame: &PerFrameConstants,
        outside: bool,
    ) -> Result<BTreeSet<u32>, RenderError> {
        // The landscape pass runs under the sunlight-only light set
        // in normal rendering: the sun is the
        // only light every outdoor batch is drawn with. It is set above the
        // pass-0 sky, because sky drawing is inside that same bracket — the sunlight switch
        // is issued before the outdoor draw, whose first call draws sky pass 0 —
        // and the sky's meshes are lit by it like everything else
        // (the subset draw turns fixed-function lighting on unconditionally).
        let sun_set = self
            .cfg
            .object_lighting
            .then(|| self.object_light_set(Vec3::ZERO, 0.0, true));
        let sky_lights: &[D3dLight] = sun_set.as_deref().unwrap_or(&[]);

        // --- the sky, pass 0 -----------------------------------------------------------
        // opens with sky pass 0, before any block.
        if let Some(sky) = &self.sky {
            sky.draw(
                gpu,
                dereth_world_render::sky::SkyPass::Before,
                sky_per_frame,
                ws.camera.position,
                outside,
                sky_lights,
            )?;
        }

        // --- the outdoor portal machinery's per-frame input ----------------------------
        // Built once for the whole frame rather than per building, because a
        // building's portals reach cells of any resident block. Building drawing is reached
        // through even when that landscape is seen from an interior's outside
        // portal (cell drawing). In the villa courtyard the camera can occupy the
        // front-gate env cell while looking across outdoors into the villa's other doorway;
        // rejecting the building pass merely because that camera is indoors loses the room
        // and its door. The caller already omits landscape drawing entirely for a sealed indoor view.
        let building_pass = self.cfg.building_portals
            && self.blocks.values().any(|b| !b.building_views.is_empty())
            && self.blocks.values().any(|b| !b.env_cells.is_empty());
        let interiors = building_pass.then(|| self.traversal_cells(ws));
        // Cell rendering opens by clearing its drawn-cell set, so a cell reached through two
        // buildings' openings is drawn once.
        let mut drawn_cells: std::collections::HashSet<u32> = std::collections::HashSet::new();
        // Set building translucency to 0.0, the first thing building drawing does.
        // See [`Self::flush_alpha_list`].
        let mut alpha_flushed: std::collections::HashSet<(i32, i32)> =
            std::collections::HashSet::new();
        let mut alpha_pending: Vec<(i32, i32)> = Vec::new();

        // --- the landscape -------------------------------------------------------------
        let mid = ws.streamer.window.mid_width();
        let mut vertices: Vec<u8> = Vec::with_capacity(6 * LAND_VERTEX_STRIDE as usize);
        // The landscape detail surface, when `Render.LandscapeDetailTextures` made one, and the
        // camera frame its distance fade is measured in: a point's view-space depth is its
        // distance along the camera's forward (local y) axis.
        let land_detail = self.current_detail(dereth_world_render::detail::DetailClass::Landscape);
        let camera = ws.camera.frame();
        let mut detail_vertices: Vec<u8> = Vec::with_capacity(6 * LAND_VERTEX_STRIDE as usize);
        let order: Vec<(u32, u32)> = block_draw_order(mid)
            .into_iter()
            .map(|i| (i / mid, i % mid))
            .collect();
        for &(xi, yi) in &order {
            let Some(slot) = ws.streamer.window.slot(xi, yi) else {
                continue;
            };
            let Some(block) = self.blocks.get(&(slot.block_x, slot.block_y)) else {
                continue;
            };
            let Some(mesh) = slot.mesh.as_ref().and_then(SlotMesh::get::<LandblockMesh>) else {
                continue;
            };
            // Vertices are block-local, so the viewpoint the back-face test uses has to be too.
            let viewpoint = Vec3::new(
                ws.camera.position.x - block.origin.0,
                ws.camera.position.y - block.origin.1,
                ws.camera.position.z,
            );
            let world = world_constants(&Frame::new(
                Vec3::new(block.origin.0, block.origin.1, 0.0),
                Quat::IDENTITY,
            ));
            let n = mesh.side_cell_count;
            // A block outside the viewer's own is stitched towards it, and `closest_cell`
            // reads `trans_dir` to decide which of its cells is nearest.
            let step = 8 / u16::from(n).max(1);
            #[allow(clippy::cast_possible_truncation)]
            // LINT-OK: both are clamped to side_cell_count - 1 <= 7 on the line above. Not a
            // float conversion.
            let viewer_cell = (
                (u16::from(ws.streamer.viewer_cell.0) / step).min(u16::from(n) - 1) as u8,
                (u16::from(ws.streamer.viewer_cell.1) / step).min(u16::from(n) - 1) as u8,
            );
            for cell in cell_draw_order(n, mesh.trans_dir, viewer_cell) {
                let (i, j) = (
                    usize::from(cell) / usize::from(n),
                    usize::from(cell) % usize::from(n),
                );
                let tris = visible_triangles(mesh, i, j, viewpoint);
                if !tris.is_empty() {
                    vertices.clear();
                    for p in &tris {
                        for v in triangle_vertices(mesh, p) {
                            vertices.extend_from_slice(&v.to_bytes());
                        }
                    }
                    // The current mode's representation, or the other one while a switch is
                    // still converting this block.
                    let has_composites = !block.cell_keys.is_empty();
                    let splat = if self.terrain_splat || !has_composites {
                        block
                            .cell_splat
                            .get(usize::from(cell))
                            .and_then(Option::as_ref)
                    } else {
                        None
                    };
                    if let Some(splat) = splat {
                        gpu.draw_terrain_splat(
                            &self.terrain_key,
                            splat,
                            per_frame,
                            &world,
                            &vertices,
                        )?;
                    } else {
                        if let Some(slot) =
                            block.cell_texture.get(usize::from(cell)).copied().flatten()
                        {
                            // Sampler 1 = linear / clamp: a merged land surface covers exactly
                            // one cell and the UVs are exactly [0, 1], so wrapping would fetch
                            // across the seam.
                            gpu.bind_texture(slot, 1);
                        }
                        gpu.draw_dynamic(
                            &self.terrain_key,
                            &DrawConstants::default(),
                            per_frame,
                            &world,
                            &vertices,
                        )?;
                    }
                    // The landscape detail pass: a block at full detail draws each cell's
                    // triangles again with the detail texture right after the ground.
                    if let (Some((slot, tiling)), true) = (
                        land_detail,
                        dereth_world_render::detail::block_takes_landscape_detail(u32::from(n)),
                    ) {
                        detail_vertices.clear();
                        for p in &tris {
                            for v in triangle_vertices(mesh, p) {
                                let eye = dereth_physics::math::globaltolocal(
                                    &camera,
                                    Vec3::new(
                                        block.origin.0 + v.pos[0],
                                        block.origin.1 + v.pos[1],
                                        v.pos[2],
                                    ),
                                );
                                detail_vertices
                                    .extend_from_slice(&detail_vertex(v, eye.y, tiling).to_bytes());
                            }
                        }
                        // Sampler 0 = linear / wrap: the detail texture repeats across the
                        // cell.
                        gpu.bind_texture(slot, 0);
                        gpu.draw_dynamic(
                            &self.terrain_detail_key,
                            &DrawConstants::default(),
                            per_frame,
                            &world,
                            &detail_vertices,
                        )?;
                    }
                }

                // Block drawing reaches a building through its owning land cell's
                // sort-cell draw: terrain first, then building drawing, while the remaining cells
                // continue far to near. Delaying every building until after all 64 terrain
                // cells lets a far opening's ALWAYS depth stamp erase nearer terrain.
                let draw_cell = cell;
                if let Some((cells, placed)) = interiors.as_ref() {
                    let has_building = block
                        .building_views
                        .iter()
                        .any(|b| b.draw_cell(n) == draw_cell);
                    if has_building && self.cfg.portal_alpha_flush {
                        self.flush_alpha_list(
                            gpu,
                            per_frame,
                            &alpha_pending,
                            &mut alpha_flushed,
                            AlphaFlush::Building,
                        )?;
                        alpha_pending.clear();
                    }
                    self.draw_building_interiors(
                        ws,
                        gpu,
                        per_frame,
                        block,
                        draw_cell,
                        n,
                        cells,
                        placed,
                        &mut drawn_cells,
                    )?;
                }
            }

            // --- this block's scenery, buildings and static objects ---------------------
            // Block drawing draws each land cell's terrain and then
            // its sort cell's building and objects, all inside the block's own
            // level-3 position push of (block id, identity). The batches are block-local, so
            // `world` above is exactly that push.
            for batch in &block.opaque {
                submit_static_batch(
                    gpu,
                    per_frame,
                    &world,
                    batch,
                    sun_set.as_deref(),
                    self.current_detail(dereth_world_render::detail::DetailClass::Building),
                )?;
            }
            // "Multiple Pass Alpha": a clip-mapped subset is drawn in place, where its cell
            // draws it, **and** queued on the clip list, so its second pass goes out with the
            // next flush (the next building's, or the frame's) ahead of everything on the
            // alpha list. Drawing the first pass here, inside the walk, is what lets a
            // building's own flush carry the second.
            if self.cfg.render.multi_pass_alpha {
                let detail =
                    self.current_detail(dereth_world_render::detail::DetailClass::Building);
                let mut queued = false;
                let mut stats = self.frame_landscape_alpha.get();
                for batch in block
                    .blended
                    .iter()
                    .filter(|b| static_multipass_member(b, detail.is_some()))
                {
                    submit_static_batch(gpu, per_frame, &world, batch, sun_set.as_deref(), detail)?;
                    stats.clip += usize::from(batch.key.alpha_test);
                    queued = true;
                }
                self.frame_landscape_alpha.set(stats);
                if queued {
                    self.frame_multipass_pending
                        .borrow_mut()
                        .push((slot.block_x, slot.block_y));
                }
            }
            // This block's translucency is now "queued": the next building's
            // `FlushAlphaList(0.0)` is what draws it, and the frame's own is what draws the
            // rest.
            alpha_pending.push((slot.block_x, slot.block_y));
        }

        // --- the alpha-tested list -----------------------------------------------------
        // `BlockDraw::blended` is keyed on `alpha_blend` alone, so
        // it holds **both** lists: the alpha-tested cut-outs (catalogue rows 7-9, depth write
        // **kept**) and the blended ones (rows 3-6 and 10-13, depth write off). Drawing the
        // bucket as one loop in bake order would interleave them.
        //
        // The two halves are separated here. The alpha-tested half stays
        // in the landscape pass, drawn in place — a standing deviation, and a harmless one:
        // an alpha-tested surface writes depth, so where it sits
        // among the other depth-writing draws cannot change which of them is in front. The
        // blended half does not write depth and is ordered by nothing but when it is drawn, so
        // it goes on the queue below and is drawn at the frame's flush.
        //
        // Drawing the alpha-tested half first is the client's own flush order — that list to
        // exhaustion, then the blended one — and it explains the observed case of
        // distant tree foliage painting over a near plant: the trees are alpha-tested and the
        // plant is not.
        for &(xi, yi) in &order {
            let Some(slot) = ws.streamer.window.slot(xi, yi) else {
                continue;
            };
            let Some(block) = self.blocks.get(&(slot.block_x, slot.block_y)) else {
                continue;
            };
            // With "Multiple Pass Alpha" on, the clip-mapped batches were drawn inside the
            // walk; what is left here is the alpha-tested rest (an `Alpha | ClipMap` surface,
            // a shell under the building detail texture).
            let detail = self.current_detail(dereth_world_render::detail::DetailClass::Building);
            let in_walk = |b: &StaticBatch| {
                self.cfg.render.multi_pass_alpha && static_multipass_member(b, detail.is_some())
            };
            let here = |b: &&StaticBatch| static_clip_list_member(b) && !in_walk(b);
            if !block.blended.iter().any(|b| here(&b)) {
                continue;
            }
            let world = world_constants(&Frame::new(
                Vec3::new(block.origin.0, block.origin.1, 0.0),
                Quat::IDENTITY,
            ));
            let mut stats = self.frame_landscape_alpha.get();
            for batch in block.blended.iter().filter(here) {
                submit_static_batch(
                    gpu,
                    per_frame,
                    &world,
                    batch,
                    sun_set.as_deref(),
                    self.current_detail(dereth_world_render::detail::DetailClass::Building),
                )?;
                stats.clip += 1;
                // How many of the frame flush's blended batches were already on the screen
                // when this alpha-tested one went down. See [`LandscapeAlphaStats`].
                stats.blend_before_clip = stats.blend_before_clip.max(stats.blend);
            }
            self.frame_landscape_alpha.set(stats);
        }

        // --- the blended list, queued --------------------------------------------------
        // This pass does **not** flush. The landscape walk has no
        // flush in it at all: the flushes are the per-building one above and then the
        // caller's, once the objects are down. In the client those objects are the land
        // cells' own, drawn inside the block walk, so every opaque creature standing in an
        // outdoor cell is already on the screen with its depth written before a single
        // blended leaf is.
        //
        // Drawing the queue here instead put the landscape's translucency **under** the
        // objects, and a blended surface writes no depth (catalogue rows 3-6 and 10-13), so an
        // object standing behind a plant passed `ZFunc::Less` where its leaves were and
        // painted over them. See [`Self::flush_pending_alpha_list`].
        self.frame_alpha_pending
            .borrow_mut()
            .extend(alpha_pending.iter().copied());

        // --- the sky, pass 1 -----------------------------------------------------------
        // closes with sky pass 1, after the blocks and their alpha list
        // and before `WorldObjects` draws the objects — which is why a nearby NPC still covers the
        // weather layer even though the layer was painted with `DEPTHTEST_ALWAYS`.
        if let Some(sky) = &self.sky {
            sky.draw(
                gpu,
                dereth_world_render::sky::SkyPass::After,
                sky_per_frame,
                ws.camera.position,
                outside,
                sky_lights,
            )?;
        }

        // Building drawing's own cell walk — the interiors an outdoor viewer
        // sees through a building's openings. `drawn_cells` is that walk's
        // set, so it is exactly the cells this pass put on the screen. See
        // [`Self::frame_drawn_cells`].
        if let Some(seen) = self.frame_drawn_cells.borrow_mut().as_mut() {
            seen.extend(drawn_cells.iter().copied());
        }
        Ok(drawn_cells.into_iter().collect())
    }
}
