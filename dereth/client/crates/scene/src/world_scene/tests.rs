//! World scene behavior through real assets and devices.

use super::*;

/// Evaluate what `legacy.hlsl` computes for `mul(v, M)`, given the sixteen floats uploaded
/// into the constant buffer: `result[j] = sum_i v[i] * data[j*4 + i]`, because the cbuffer
/// matrix is column-major so `M[i][j]` lives at `data[j*4 + i]`.
fn shader_mul(data: &[f32; 16], v: glam::Vec4) -> glam::Vec4 {
    let a = [v.x, v.y, v.z, v.w];
    let mut out = [0.0f32; 4];
    for (j, o) in out.iter_mut().enumerate() {
        for (i, &vi) in a.iter().enumerate() {
            *o += vi * data[j * 4 + i];
        }
    }
    glam::Vec4::new(out[0], out[1], out[2], out[3])
}

// Oracle: `dereth_render/src/shaders/legacy.hlsl` (`mul(float4(i.pos, 1.0), g_world)` and
// `mul(world, g_viewProj)`) plus HLSL's documented default column-major cbuffer packing --
// D3DCompile is called without D3DCOMPILE_PACK_MATRIX_ROW_MAJOR. What the shader computes
// has to agree with what glam computes, and with `to_cols_array` it does not.
#[test]
fn the_shader_reads_the_matrix_layout_that_dereth_render_uploads() {
    let m = glam::Mat4::from_cols_array(&[
        1.0, 2.0, 3.0, 4.0, //
        5.0, 6.0, 7.0, 8.0, //
        9.0, 10.0, 11.0, 12.0, //
        13.0, 14.0, 15.0, 16.0,
    ]);
    let v = glam::Vec4::new(0.5, -1.5, 2.0, 1.0);
    let want = m * v;
    assert_eq!(shader_mul(&hlsl_matrix(m), v), want);
    assert_ne!(
        shader_mul(&m.to_cols_array(), v),
        want,
        "if these ever agree the workaround can go"
    );
    let i = glam::Mat4::IDENTITY;
    assert_eq!(hlsl_matrix(i), i.to_cols_array());
}

// Oracle: `dereth_render::camera`'s own convention -- "a client-space point (x, y, z) reaches
// [the view matrix] as (x, z, y)", applied by the world matrix. A point on the ground of a
// block one step north of the viewer must land in front of a camera looking north.
#[test]
fn the_world_matrix_swaps_z_up_into_d3d_and_then_the_view_matrix_agrees() {
    let cam = FreeCamera::new(Vec3::new(96.0, 0.0, 50.0), 0.0, 0.0);
    let view = dereth_render::camera::view_from_frame(&cam.frame());
    let block = Frame::new(Vec3::new(0.0, BLOCK_LENGTH, 0.0), Quat::IDENTITY);
    let world = world_constants(&block);
    // A block-local point at the block's south-west corner, 192 m north of the viewer.
    let p = glam::Vec4::new(96.0, 0.0, 50.0, 1.0);
    let w = shader_mul(&world.world, p);
    // After the swap the client's z is D3D's y.
    assert!(
        (w - glam::Vec4::new(96.0, 50.0, 192.0, 1.0)).length() < 1e-3,
        "{w:?}"
    );
    let v = shader_mul(&hlsl_matrix(view), w);
    assert!(
        (v - glam::Vec4::new(0.0, 0.0, 192.0, 1.0)).length() < 1e-3,
        "{v:?}"
    );
}

// Oracle: `dereth_world_render::math::l2g`. A rotated frame's
// matrix must move a point the same way `localtoglobal` does, or every scenery object with
// a heading is placed wrong.
#[test]
fn the_frame_matrix_agrees_with_the_clients_own_localtoglobal() {
    let h = std::f32::consts::FRAC_PI_4;
    let f = Frame::new(
        Vec3::new(3.0, -7.0, 11.0),
        Quat::new(
            dereth_primitives::num::math::cosf(h),
            0.0,
            0.0,
            dereth_primitives::num::math::sinf(h),
        ),
    );
    for p in [
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 2.0, 0.0),
        Vec3::new(0.0, 0.0, 3.0),
        Vec3::new(-4.0, 5.0, -6.0),
    ] {
        let want = dereth_terrain::math::localtoglobal(&f, p);
        let got = frame_matrix(&f) * glam::Vec4::new(p.x, p.y, p.z, 1.0);
        assert!(
            (got - glam::Vec4::new(want.x, want.y, want.z, 1.0)).length() < 1e-4,
            "{p:?}: {got:?} != {want:?}"
        );
    }
}

// Oracle: the retail region. A landblock id has to unpack to
// the (blockX, blockY) the whole landscape path is indexed by, and Holtburg is (0xA9, 0xB4).
#[test]
fn the_landblock_id_unpacks_the_documented_way() {
    assert_eq!(block_xy(0xA9B4), (0xA9, 0xB4));
    assert_eq!(landblock_did(0xA9B4), DataId(0xA9B4_FFFF));
    assert_eq!(lbi_did(0xA9B4), DataId(0xA9B4_FFFE));
}

use dereth_render::surface::surface_type as st;

/// **An `expect`, never a skip.** A run that cannot open the dats has proved nothing about
/// them and must not pass by silently skipping the assertions.
fn retail_store() -> RetailDatStore {
    let dir = dereth_dat::testing::dat_dir();
    dereth_dat::testing::open_store().unwrap_or_else(|| {
        panic!(
            "no retail dats under {} -- set DERETH_TEST_DAT_DIR",
            dir.display()
        )
    })
}

/// Surface alpha is opaque without the translucent flag, otherwise it is the truncated
/// complement of translucency scaled to a byte:
///
/// ```text
/// if (!(type & TRANSLUCENT)) curr_alpha = 0xFF
/// else                       curr_alpha = trunc((1.0 - translucency) * 255.0)
/// ```
fn curr_alpha(surface_type: u32, translucency: f32) -> u8 {
    if surface_type & st::TRANSLUCENT == 0 {
        return 0xFF;
    }
    let v = dereth_primitives::num::to_i32((1.0 - translucency) * 255.0);
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    {
        // LINT-OK: the client keeps the low byte of the truncated result.
        (v as u32 & 0xFF) as u8
    }
}

/// Every surface in `client_portal.dat`, split by what the texture-alpha path does to it.
struct SurfaceCensus {
    total: usize,
    flagged: usize,
    flagged_nonzero: usize,
    flagged_zero: usize,
    /// Distinct non-zero `translucency` values among the flagged surfaces (by bit
    /// pattern, because that is what the dat stores), and how many surfaces carry each.
    values: BTreeMap<u32, usize>,
    /// One representative surface id per distinct non-zero `translucency`.
    reps: BTreeMap<u32, DataId>,
    /// Surfaces with the flag **clear**, whose vertices must not move at all.
    clear: Vec<DataId>,
    /// Surfaces with the flag set at `translucency == 0`, i.e. `curr_alpha == 0xFF`.
    zero: Vec<DataId>,
}

fn census(store: &RetailDatStore) -> SurfaceCensus {
    let ids = store.ids_of(DbType::Surface);
    assert!(
        !ids.is_empty(),
        "client_portal.dat holds no 0x08 surface records at all"
    );
    let mut c = SurfaceCensus {
        total: 0,
        flagged: 0,
        flagged_nonzero: 0,
        flagged_zero: 0,
        values: BTreeMap::new(),
        reps: BTreeMap::new(),
        clear: Vec::new(),
        zero: Vec::new(),
    };
    for id in ids {
        let Some(s) = read_surface(store, id) else {
            continue;
        };
        c.total += 1;
        if s.surface_type & st::TRANSLUCENT == 0 {
            if c.clear.len() < 24 {
                c.clear.push(id);
            }
            continue;
        }
        c.flagged += 1;
        if s.translucency == 0.0 {
            c.flagged_zero += 1;
            if c.zero.len() < 8 {
                c.zero.push(id);
            }
            continue;
        }
        c.flagged_nonzero += 1;
        let bits = s.translucency.to_bits();
        *c.values.entry(bits).or_insert(0) += 1;
        c.reps.entry(bits).or_insert(id);
    }
    c
}

/// The denominator this unit is measured against. Printed in full and asserted on both
/// sides: a corpus with **no** translucent surface would make every other test here
/// vacuous, and a corpus with nothing but them would mean the byte-identical assertion
/// covers nothing.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn the_dat_carries_translucent_surfaces_at_more_than_one_translucency() {
    let store = retail_store();
    let c = census(&store);
    eprintln!(
        "Surface translucency: {} surfaces decoded; {} carry TRANSLUCENT ({} at a non-zero \
         translucency, {} at zero); {} distinct non-zero translucency values",
        c.total,
        c.flagged,
        c.flagged_nonzero,
        c.flagged_zero,
        c.values.len()
    );
    for (bits, n) in &c.values {
        let t = f32::from_bits(*bits);
        eprintln!(
            "    translucency {t} -> curr_alpha {} on {n} surface(s), e.g. {:?}",
            curr_alpha(st::TRANSLUCENT, t),
            c.reps[bits]
        );
    }
    assert!(c.total > 1000, "only {} surfaces decoded", c.total);
    assert!(
        c.flagged_nonzero > 0,
        "no surface would change, so nothing here is tested"
    );
    assert!(
        c.values.len() > 2,
        "only {} distinct translucency values -- a linear map and a wrong-but-monotonic \
         one agree at the ends, so the sweep needs interior values",
        c.values.len()
    );
    assert!(!c.clear.is_empty(), "no flag-clear surface to hold still");
}

#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn the_other_two_scalars_setsurface_hands_the_vertex_path_are_counted() {
    let store = retail_store();
    let ids = store.ids_of(DbType::Surface);
    let (mut total, mut luminous, mut diffuse_not_one) = (0usize, 0usize, 0usize);
    for id in ids {
        let Some(s) = read_surface(&store, id) else {
            continue;
        };
        total += 1;
        if s.luminosity > 0.0 {
            luminous += 1;
        }
        if (s.diffuse - 1.0).abs() > f32::EPSILON {
            diffuse_not_one += 1;
        }
    }
    eprintln!(
        "Surface material channels: of {total} surfaces, {luminous} carry luminosity > 0 and \
         {diffuse_not_one} carry diffuse != 1.0; the fixture counts both material channels."
    );
    assert!(total > 1000, "only {total} surfaces decoded");
}

fn warp_gpu() -> Gpu {
    let cfg = dereth_render::device::DeviceConfig {
        width: 64,
        height: 64,
        ..dereth_render::device::DeviceConfig::default()
    };
    Gpu::new(None, &cfg).expect("a D3D12 WARP device")
}

/// One triangle on one surface — enough to read the diffuse word out of, and it keeps
/// the sweep to one texture upload per surface (the descriptor heap is finite).
fn one_triangle(surface: DataId) -> Vec<dereth_client_runtime::models::SurfaceGroup> {
    vec![dereth_client_runtime::models::SurfaceGroup {
        surface: Some(surface),
        two_sided: false,
        tiled: false,
        normals: Vec::new(),
        vertices: vec![
            (Vec3::new(0.0, 0.0, 0.0), 0.0, 0.0),
            (Vec3::new(1.0, 0.0, 0.0), 1.0, 0.0),
            (Vec3::new(0.0, 1.0, 0.0), 0.0, 1.0),
        ],
    }]
}

/// The `D3DFVF_DIFFUSE` word of every vertex of a built buffer.
fn diffuse_words(vertices: &[u8]) -> Vec<u32> {
    assert_eq!(vertices.len() % OBJECT_VERTEX_STRIDE, 0, "partial vertex");
    let o = OBJECT_DIFFUSE_OFFSET;
    vertices
        .chunks(OBJECT_VERTEX_STRIDE)
        .map(|v| u32::from_le_bytes([v[o], v[o + 1], v[o + 2], v[o + 3]]))
        .collect()
}

/// Build one surface's triangle through [`build_meshes`] and return its diffuse words.
fn words_for(store: &RetailDatStore, gpu: &mut Gpu, honour: bool, surface: DataId) -> Vec<u32> {
    let textures = TextureStore::new(store);
    let mut cache = BakeCache {
        surface_translucency: honour,
        ..BakeCache::default()
    };
    let meshes = build_meshes(
        store,
        &mut cache,
        &textures,
        gpu,
        &one_triangle(surface),
        None,
        None,
    )
    .expect("the surface resolves");
    assert_eq!(meshes.len(), 1, "one group in, one mesh out");
    diffuse_words(&meshes[0].vertices)
}

#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn a_resolved_surface_is_freed_on_its_last_release_and_not_before() {
    let store = retail_store();
    let mut gpu = warp_gpu();
    let textures = TextureStore::new(&store);
    let mut cache = BakeCache::default();
    // A surface every shipped landscape uses, taken from the dat rather than invented.
    let id = store
        .ids_of(dereth_dat::DbType::Surface)
        .into_iter()
        .find(|id| read_surface(&store, *id).is_some_and(|s| s.orig_texture_id.is_some()))
        .expect("client_portal.dat carries a textured surface record");
    let key = GroupKey {
        surface: Some(id),
        two_sided: false,
        tiled: false,
        appearance: cache.appearance_for(&store, &textures, Some(id), None),
    };
    let a = cache
        .resolve(&store, &textures, &mut gpu, key.clone())
        .expect("resolves");
    let slot = a.texture.expect("the surface names a texture");
    assert_eq!(cache.link_count(slot), Some(1), "one resolve, one link");
    let b = cache
        .resolve(&store, &textures, &mut gpu, key.clone())
        .expect("resolves");
    assert_eq!(
        b.texture,
        Some(slot),
        "the second resolve is a cache hit, not an upload"
    );
    assert_eq!(
        cache.link_count(slot),
        Some(2),
        "a hit adds a reference, as retail's combined-texture creation does"
    );
    assert_eq!(
        cache.textures_uploaded, 1,
        "one pair held for two consumers"
    );

    // First half: one consumer leaves and the entry survives.
    assert!(
        !cache.release_group_texture(&mut gpu, slot),
        "freed at one link remaining"
    );
    assert_eq!(cache.link_count(slot), Some(1));
    assert_eq!(cache.textures_uploaded, 1, "still held");
    assert_eq!(
        gpu.texture_table_stats().frees,
        0,
        "the texture-image link was dropped while a consumer still held the surface"
    );

    // Second half: the last consumer leaves and it is freed.
    assert!(
        cache.release_group_texture(&mut gpu, slot),
        "not freed at zero links"
    );
    assert_eq!(cache.link_count(slot), None, "the entry is gone");
    assert_eq!(cache.textures_uploaded, 0);
    assert_eq!(
        gpu.texture_table_stats().frees,
        1,
        "the descriptor pair went back"
    );
    assert_eq!(cache.unowned_releases, 0);

    // And a release of a slot the cache no longer owns is counted, not silent.
    assert!(!cache.release_group_texture(&mut gpu, slot));
    assert_eq!(
        cache.unowned_releases, 1,
        "a double release must be visible"
    );
}

/// **The arithmetic, at every distinct translucency the dat actually uses** rather than
/// only at the two ends: a linear map and a wrong-but-monotonic map agree at 0 and 1.
///
/// Oracle: `surface alpha conversion`'s own expression, in [`curr_alpha`] above, applied to the
/// `translucency` read out of the dat at run time. Nothing here is a literal.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn build_meshes_writes_setsurfaces_curr_alpha_for_every_translucency_in_the_dat() {
    let store = retail_store();
    let c = census(&store);
    let mut gpu = warp_gpu();
    let mut checked = 0usize;
    for (bits, id) in &c.reps {
        let t = f32::from_bits(*bits);
        let s = read_surface(&store, *id).expect("the representative decodes");
        let want = u32::from(curr_alpha(s.surface_type, s.translucency)) << 24 | 0x00FF_FFFF;
        for w in words_for(&store, &mut gpu, true, *id) {
            assert_eq!(
                w, want,
                "{id:?} at translucency {t}: diffuse {w:#010X}, surface alpha conversion says {want:#010X}"
            );
        }
        checked += 1;
    }
    eprintln!(
        "Surface alpha arithmetic: {checked} distinct translucency values checked against \
         surface alpha conversion's own expression, over {} translucent surfaces",
        c.flagged_nonzero
    );
    assert_eq!(
        checked,
        c.values.len(),
        "the sweep skipped a translucency value"
    );
    assert!(
        checked > 2,
        "only {checked} values -- the ends alone do not discriminate"
    );
}

#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn a_surface_without_the_flag_is_byte_identical_to_the_previous_build() {
    let store = retail_store();
    let c = census(&store);
    let mut gpu = warp_gpu();
    let mut clear = 0usize;
    for id in &c.clear {
        for w in words_for(&store, &mut gpu, true, *id) {
            assert_eq!(w, 0xFFFF_FFFF, "{id:?} has TRANSLUCENT clear and moved");
        }
        clear += 1;
    }
    let mut zeroed = 0usize;
    for id in &c.zero {
        for w in words_for(&store, &mut gpu, true, *id) {
            assert_eq!(
                w, 0xFFFF_FFFF,
                "{id:?} is TRANSLUCENT at translucency 0 and moved"
            );
        }
        zeroed += 1;
    }
    eprintln!(
        "Opaque surface alpha: {clear} flag-clear and {zeroed} flagged-at-zero surfaces are \
         fully opaque (of {} clear and {} flagged-at-zero in the dat)",
        c.total - c.flagged,
        c.flagged_zero
    );
    assert!(
        clear > 0,
        "the control covered no flag-clear surface at all"
    );
    // The second bucket is **empty in this dat** -- measured, not assumed: all 261
    // `TRANSLUCENT` surfaces carry a non-zero `translucency`. Asserting the equality
    // rather than a sample count is what keeps that from reading as a passing check on no
    // data; the asserted total is the denominator.
    assert_eq!(
        zeroed,
        c.zero.len(),
        "the flagged-at-zero sample was not walked"
    );
    assert_eq!(
        c.flagged_zero, 0,
        "the dat has grown a TRANSLUCENT surface at translucency 0; {zeroed} sampled"
    );
}

#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn clearing_the_switch_reproduces_the_previous_build_exactly() {
    let store = retail_store();
    let c = census(&store);
    let mut gpu = warp_gpu();
    let ids: Vec<DataId> = c
        .reps
        .values()
        .copied()
        .chain(c.clear.iter().copied())
        .collect();
    assert!(ids.len() > 3, "nothing to compare");
    for id in &ids {
        for w in words_for(&store, &mut gpu, false, *id) {
            assert_eq!(w, 0xFFFF_FFFF, "{id:?} moved with the switch off");
        }
    }
}

/// The **other** call site. [`ObjectBaker`] appends vertices before it has a `Gpu` to
/// resolve the surface with, so its alpha is stamped in `finish`; this asserts the two
/// paths agree byte for byte on the same geometry, which is the only thing that keeps a
/// static and a part of the same object from drawing at different opacities.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn the_object_baker_writes_the_same_alpha_as_build_meshes() {
    let store = retail_store();
    let mut gpu = warp_gpu();

    // A graphics object that actually carries a translucent surface, **found** rather than
    // named: the first `0x01` whose triangulated groups include one.
    let mut chosen: Option<(DataId, Vec<dereth_client_runtime::models::SurfaceGroup>)> = None;
    for id in store.ids_of(DbType::GfxObj) {
        let groups = build_gfxobj(&store, id);
        let translucent = groups.iter().any(|g| {
            g.surface
                .and_then(|s| read_surface(&store, s))
                .is_some_and(|s| s.surface_type & st::TRANSLUCENT != 0 && s.translucency != 0.0)
        });
        if translucent && groups.iter().any(|g| !g.vertices.is_empty()) {
            chosen = Some((id, groups));
            break;
        }
    }
    let (gfxobj, groups) =
        chosen.expect("no graphics object in the dat carries a non-zero translucent surface");

    // The baker's arm.
    let mut cache = BakeCache::default();
    let mut baker = ObjectBaker::new(&store, &mut cache, false, false, false);
    baker.add_object(
        gfxobj,
        &Frame::new(Vec3::new(0.0, 0.0, 0.0), Quat::IDENTITY),
        1.0,
    );
    let (opaque, blended, degrade) = baker.finish(&mut gpu).expect("the object bakes");
    assert!(
        degrade.is_empty(),
        "part_degrades is off, so no placement may be registered"
    );

    // What `surface alpha conversion` says, per surface, from the dat.
    let mut want: BTreeSet<u32> = BTreeSet::new();
    for g in &groups {
        if g.vertices.is_empty() {
            continue;
        }
        let s = g.surface.and_then(|s| read_surface(&store, s));
        let a = s.map_or(0xFF, |s| curr_alpha(s.surface_type, s.translucency));
        want.insert(u32::from(a) << 24 | 0x00FF_FFFF);
    }
    let mut got: BTreeSet<u32> = BTreeSet::new();
    let mut vertices = 0usize;
    for b in opaque.iter().chain(blended.iter()) {
        for w in diffuse_words(&b.vertices) {
            got.insert(w);
            vertices += 1;
        }
    }
    eprintln!(
        "Baked surface alpha: {gfxobj:?} baked {vertices} vertices; diffuse words {got:#010X?}, \
         surface alpha conversion says {want:#010X?}"
    );
    assert!(
        vertices > 0,
        "the baker emitted nothing, so it proves nothing"
    );
    assert_eq!(
        got, want,
        "the baker's vertex alpha is not surface alpha conversion's"
    );
    assert!(
        got.iter().any(|w| *w != 0xFFFF_FFFF),
        "the chosen object baked nothing translucent, so the mutation could not be seen"
    );

    // And the free-function path, on the same groups, agrees.
    let textures = TextureStore::new(&store);
    let mut c2 = BakeCache::default();
    let meshes = build_meshes(&store, &mut c2, &textures, &mut gpu, &groups, None, None)
        .expect("the groups resolve");
    let mut through_build: BTreeSet<u32> = BTreeSet::new();
    for m in &meshes {
        if m.vertices.is_empty() {
            continue;
        }
        through_build.extend(diffuse_words(&m.vertices));
    }
    assert_eq!(
        through_build, got,
        "build_meshes and ObjectBaker disagree on the alpha"
    );
}

/// The packing itself, against the polygon draw's literal
/// `curr_alpha << 0x18 | 0xffffff`, and the stamp's placement inside a 24-byte vertex.
#[test]
fn the_diffuse_word_is_packed_argb_and_the_stamp_lands_on_the_alpha_byte() {
    assert_eq!(vertex_diffuse(0xFF), 0xFFFF_FFFF);
    assert_eq!(vertex_diffuse(0x00), 0x00FF_FFFF);
    assert_eq!(vertex_diffuse(0xBF), 0xBFFF_FFFF);
    let mut v = vec![0u8; OBJECT_VERTEX_STRIDE * 2];
    let o = OBJECT_DIFFUSE_OFFSET;
    v[o..o + 4].copy_from_slice(&vertex_diffuse(0xFF).to_le_bytes());
    v[o + OBJECT_VERTEX_STRIDE..o + OBJECT_VERTEX_STRIDE + 4]
        .copy_from_slice(&vertex_diffuse(0xFF).to_le_bytes());
    stamp_vertex_alpha(&mut v, 0x2A);
    assert_eq!(diffuse_words(&v), vec![0x2AFF_FFFF, 0x2AFF_FFFF]);
    assert_eq!(v[VERTEX_ALPHA_BYTE], 0x2A);
}

/// Material translucency updates all four alpha components and their combined alpha flag:
///
/// ```text
/// applying translucency `t`:
///     material_alpha = 1.0 - t;
///     Ambient.a = Diffuse.a = Specular.a = Emissive.a = material_alpha;
///     update the material's alpha-state flag;
/// alpha-state check:
///     has_alpha = !(1.0 <= Ambient.a && 1.0 <= Diffuse.a
///                   && 1.0 <= Specular.a && 1.0 <= Emissive.a)
/// ```
///
/// Written out here rather than referenced, so the assertion below is against the
/// client's own expression and not against [`material_texture_factor`]'s.
fn material_alpha_byte(t: f32) -> Option<u8> {
    // The four `D3DMATERIAL9` alphas written by the translucency operation, kept as four values
    // because the alpha-state check tests four -- it is the *conjunction over all four* that
    // makes `has_alpha` clear, which is why a material whose Specular alpha alone were
    // low would still be flagged.
    let (ambient, diffuse, specular, emissive) = (1.0f32 - t, 1.0f32 - t, 1.0f32 - t, 1.0f32 - t);
    let has_alpha = !(ambient >= 1.0 && diffuse >= 1.0 && specular >= 1.0 && emissive >= 1.0);
    if !has_alpha {
        return None;
    }
    let a = diffuse;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // LINT-OK: truncation towards zero of a value in 0..=255.
    Some((a * 255.0).clamp(0.0, 255.0) as u8)
}

/// The material alpha is one minus t at every interior value.
#[test]
fn the_material_alpha_is_one_minus_t_at_every_interior_value() {
    let mut checked = 0usize;
    let mut interior = 0usize;
    let mut distinct: BTreeSet<u32> = BTreeSet::new();
    for i in 0..=100u32 {
        #[allow(clippy::cast_precision_loss)] // 0..=100
        let t = i as f32 / 100.0;
        let want = material_alpha_byte(t);
        let got = material_texture_factor(Some(MaterialOverride {
            translucency: t,
            diffuse: dereth_animation::parts::DEFAULT_DIFFUSE,
            luminosity: dereth_animation::parts::DEFAULT_LUMINOSITY,
        }));
        match (want, got) {
            (None, None) => assert_eq!(i, 0, "has_alpha is clear only at t == 0"),
            (Some(a), Some(word)) => {
                assert_eq!(
                    word >> 24,
                    u32::from(a),
                    "t = {t}: material translucency writes 1 - t = {}, so the alpha byte \
                     is {a}, not {}",
                    1.0 - t,
                    word >> 24
                );
                // The polygon draw's RGB: the material's default `Diffuse` is (1,1,1).
                assert_eq!(word & 0x00FF_FFFF, 0x00FF_FFFF, "t = {t}: the RGB moved");
                distinct.insert(word);
                if i > 0 && i < 100 {
                    interior += 1;
                }
            }
            (w, g) => panic!("t = {t}: oracle {w:?}, got {g:?}"),
        }
        checked += 1;
    }
    eprintln!(
        "Runtime translucency: {checked} translucencies checked against the material translucency \
         `1 - t`, {interior} of them strictly interior, {} distinct texture-factor words",
        distinct.len()
    );
    assert_eq!(checked, 101);
    assert_eq!(interior, 99, "the sweep collapsed to its endpoints");
    // 0.5 is where `t` and `1 - t` agree; a sweep that only produced two words could not
    // tell a ramp from a step.
    assert!(
        distinct.len() > 90,
        "only {} distinct alphas over the ramp",
        distinct.len()
    );
    // No material at all means the material pointer is null, which
    // the material binding answers with the vertex as colour source: nothing is substituted.
    assert_eq!(material_texture_factor(None), None);
    // A material carrying only a *lighting* override has all four alphas at 1.0, so
    // the alpha-state check leaves `has_alpha` clear and this channel does nothing. Those two
    // scalars are the plain luminosity and diffuse setters and have no driver here.
    assert_eq!(
        material_texture_factor(Some(MaterialOverride {
            translucency: dereth_animation::parts::DEFAULT_TRANSLUCENCY,
            diffuse: 0.5,
            luminosity: 0.25,
        })),
        None,
        "a lighting-only material must not force the alpha path"
    );
}

#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn the_material_alpha_replaces_the_surfaces_own_vertex_alpha_rather_than_multiplying_it() {
    let store = retail_store();
    let c = census(&store);
    // A surface the dat actually carries at a translucency whose `curr_alpha` is neither
    // 0x00 nor 0xFF, so the two channels are distinguishable at all.
    let (bits, id) = c
        .reps
        .iter()
        .map(|(b, i)| (*b, *i))
        .find(|(b, _)| {
            let a = curr_alpha(st::TRANSLUCENT, f32::from_bits(*b));
            a > 0x20 && a < 0xE0
        })
        .expect("no TRANSLUCENT surface at a mid-range curr_alpha in the dat");
    let surface_t = f32::from_bits(bits);
    let vertex = curr_alpha(st::TRANSLUCENT, surface_t);

    let mut gpu = warp_gpu();
    let textures = TextureStore::new(&store);
    let mut cache = BakeCache::default();
    let meshes = build_meshes(
        &store,
        &mut cache,
        &textures,
        &mut gpu,
        &one_triangle(id),
        None,
        None,
    )
    .expect("the group resolves");
    let words: BTreeSet<u32> = meshes
        .iter()
        .flat_map(|m| diffuse_words(&m.vertices))
        .collect();
    assert_eq!(
        words,
        BTreeSet::from([vertex_diffuse(vertex)]),
        "{id:?}: the mesh does not carry surface alpha conversion's own curr_alpha"
    );

    // The runtime channel, on the same part, at a translucency of its own.
    let part_t = 0.25f32;
    let factor = material_texture_factor(Some(MaterialOverride {
        translucency: part_t,
        diffuse: dereth_animation::parts::DEFAULT_DIFFUSE,
        luminosity: dereth_animation::parts::DEFAULT_LUMINOSITY,
    }))
    .expect("t = 0.25 sets has_alpha");
    let material = (factor >> 24) as u8;
    let product = u8::try_from((u32::from(vertex) * u32::from(material)) / 255)
        .expect("a product of two bytes over 255");
    eprintln!(
        "Runtime alpha override: {id:?} is TRANSLUCENT at translucency {surface_t}, so surface conversion \
         bakes vertex alpha {vertex:#04X}; a part at translucency {part_t} carries material \
         alpha {material:#04X}; multiplying them would produce {product:#04X}"
    );
    assert_eq!(
        material,
        dereth_primitives::num::to_i32((1.0 - part_t) * 255.0).clamp(0, 255) as u8,
        "the material alpha is not 1 - t"
    );
    assert_ne!(
        material, vertex,
        "the two channels are indistinguishable on this surface"
    );
    assert_ne!(
        material, product,
        "a multiply and an override agree here, so this proves nothing"
    );
    // `draw_part` submits `factor` with `draw_params.w = 1`; the shader's
    // `lerp(i.color, g_textureFactor, 1.0)` is the texture factor and nothing else, so the
    // baked word above is discarded rather than modulated.
    assert_eq!(
        factor & 0x00FF_FFFF,
        0x00FF_FFFF,
        "the substituted RGB is not white"
    );
}

#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn every_surface_in_the_dat_takes_row_thirteen_exactly_once() {
    use dereth_render::pso::Blend;

    let store = retail_store();
    let ctx = SurfaceContext {
        vertex_format: VertexFormat::XyzNormalDiffuseTex1,
        texture_is_set: true,
        lighting: true,
        ..SurfaceContext::default()
    };

    let (mut total, mut moved, mut spared) = (0usize, 0usize, 0usize);
    let mut sample: Vec<DataId> = Vec::new();
    for id in store.ids_of(DbType::Surface) {
        let Some(s) = read_surface(&store, id) else {
            continue;
        };
        let state = RenderState {
            r#type: s.surface_type,
            handler: SurfaceHandler::Database,
            color_value: s.color_value.unwrap_or(0),
            translucency: s.translucency,
            luminosity: s.luminosity,
            diffuse: s.diffuse,
        };
        total += 1;
        let base = PipelineKey::state_from_surface(&state, ctx).key;
        let over = PipelineKey::state_from_surface(
            &state,
            SurfaceContext {
                material_has_alpha: Some(true),
                ..ctx
            },
        )
        .key;
        if sample.len() < 48 {
            sample.push(id);
        }
        if base.alpha_blend && !base.alpha_test {
            // Row 13's first branch: already blending without a test, left alone. This is
            // what spares an `Additive` glow its `ONE / ONE`.
            assert_eq!(
                over, base,
                "{id:?} was changed by an override that must not fire"
            );
            spared += 1;
        } else {
            assert_eq!(over.src_blend, Blend::SrcAlpha, "{id:?}");
            assert_eq!(over.dst_blend, Blend::InvSrcAlpha, "{id:?}");
            assert!(
                over.alpha_blend && !over.alpha_test && !over.z_write,
                "{id:?}"
            );
            // Nothing *else* may move: it is the same surface, same texture, same cull.
            assert_eq!(
                (
                    over.vertex_format,
                    over.cull,
                    over.stage_ops,
                    over.fog,
                    over.lighting
                ),
                (
                    base.vertex_format,
                    base.cull,
                    base.stage_ops,
                    base.fog,
                    base.lighting
                ),
                "{id:?}: the override changed more than row 13's five fields"
            );
            moved += 1;
        }
    }
    eprintln!(
        "Material alpha selection: {total} surfaces resolved; the override \
         changes {moved} and spares {spared}"
    );
    assert!(total > 1000, "only {total} surfaces resolved");
    assert!(
        moved > 0,
        "the override fired on nothing, so this test asserts nothing"
    );
    assert!(
        spared > 0,
        "the override fired on everything; the first branch is untested"
    );

    // **And the wire.** The sweep above is over `state_from_surface` directly; this is the
    // path a part actually takes — [`resolve_surface`] and [`build_meshes`] — asserted to
    // carry the *same* two keys onto the [`PartMesh`]. A sample rather than the whole dat
    // because each resolve uploads a texture and the descriptor heap is finite.
    let mut gpu = warp_gpu();
    let textures = TextureStore::new(&store);
    let mut cache = BakeCache::default();
    let mut wired = 0usize;
    for id in &sample {
        let Some(s) = read_surface(&store, *id) else {
            continue;
        };
        let meshes = build_meshes(
            &store,
            &mut cache,
            &textures,
            &mut gpu,
            &one_triangle(*id),
            None,
            None,
        )
        .expect("the group resolves");
        let state = RenderState {
            r#type: s.surface_type,
            handler: SurfaceHandler::Database,
            color_value: s.color_value.unwrap_or(0),
            translucency: s.translucency,
            luminosity: s.luminosity,
            diffuse: s.diffuse,
        };
        // `texture_is_set` follows whichever way the texture actually resolved, so take
        // the base key from the mesh and only assert that the *override* is row 13 of it.
        for m in &meshes {
            let want = PipelineKey::state_from_surface(
                &state,
                SurfaceContext {
                    material_has_alpha: Some(true),
                    texture_is_set: m.texture.is_some(),
                    ..ctx
                },
            )
            .key;
            assert_eq!(
                m.key_material_alpha, want,
                "{id:?}: PartMesh::key_material_alpha is not surface alpha conversion's row 13"
            );
            assert!(
                !m.key_material_alpha.z_write,
                "{id:?}: row 13 always turns the depth write off"
            );
            wired += 1;
        }
    }
    eprintln!(
        "Mesh material channels: {wired} PartMesh(es) over {} sampled surfaces",
        sample.len()
    );
    assert!(
        wired > 0,
        "build_meshes emitted nothing, so the wire is untested"
    );
}
