//! Every material and material instance of the install, made ready to draw.

use auberean_assets::material::{Material, MaterialInstance};
use auberean_draw::material::{
    draw_layers, instance_material, layer_light_variants, light_variants, program_length,
    sub_objects, switches, vertex_inputs, DEVICE_FLAGS, HARDWARE_CLASS, PROGRAM_PIECES_LIMIT,
};

use crate::common::{ids_of, install};

/// Every material instance is made ready from its material and its modifiers, and the device
/// draws at least one of its layers; every field of every material's parameters reaches a layer
/// the material has, and every coordinate-animation field an animation of that layer; none sets a
/// diffuse or normal modifier.
///
/// Behaviour: draw.material.every-material-instance-is-drawn-with-a-layer
#[test]
fn every_material_instance_is_drawn_with_a_layer() {
    let r = &install().records;
    let (mut materials, mut fields) = (0, 0);
    let mut stray = Vec::new();
    for id in ids_of(0x2B) {
        let m: Material = r.get(id).unwrap_or_else(|| panic!("{id:08X} decodes"));
        materials += 1;
        for p in &m.parameters.parameters {
            for f in &p.fields {
                fields += 1;
                let [field, _, layer, index] = f.words;
                // The fields that set a layer's diffuse and normal modifiers: no layer holds one.
                if (13..=32).contains(&field) {
                    stray.push((id, f.words));
                    continue;
                }
                let Some(l) = usize::try_from(layer).ok().and_then(|k| m.layers.get(k)) else {
                    stray.push((id, f.words));
                    continue;
                };
                if (33..=37).contains(&field)
                    && usize::try_from(index)
                        .ok()
                        .and_then(|k| l.animations.get(k))
                        .is_none()
                {
                    stray.push((id, f.words));
                }
            }
        }
    }
    let (mut instances, mut drawn) = (0, 0);
    let mut undrawn = Vec::new();
    for id in ids_of(0x31) {
        let _: MaterialInstance = r.get(id).unwrap_or_else(|| panic!("{id:08X} decodes"));
        instances += 1;
        let m = instance_material(r, id).unwrap_or_else(|| panic!("{id:08X} is made ready"));
        if draw_layers(&m, DEVICE_FLAGS).is_empty() {
            undrawn.push(id);
        } else {
            drawn += 1;
        }
    }
    println!("{materials} materials ({fields} fields), {instances} instances, {drawn} drawn");
    assert!(stray.is_empty(), "{stray:08X?}");
    assert!(
        materials > 100 && instances > 1_000,
        "{materials} {instances}"
    );
    assert!(
        undrawn.is_empty(),
        "{} undrawn: {:08X?}",
        undrawn.len(),
        &undrawn[..undrawn.len().min(20)]
    );
}

/// A device keeps of each material the layers made for its hardware class that have stages, and
/// the parameter fields that write to them, renumbered; a layer's vertex program takes each stage's
/// coordinate source and a texture matrix for each stage an animation moves: the interior cells'
/// material's normal-mapped layer reads the vertex sets on its first three stages and the vector
/// toward the light on its fourth, its detail map moved by a matrix.
///
/// Behaviour: draw.material.a-device-keeps-its-classs-layers-with-stages
#[test]
fn a_device_keeps_its_classs_layers_with_stages() {
    // The layer flags of hardware classes 1 and 3, and of class 2.
    const OTHER_CLASSES: u32 = 1 << 20 | 1 << 22;
    const CLASS_TWO: u32 = 1 << 21;
    let r = &install().records;
    let (mut layers, mut kept, mut empty) = (0, 0, 0);
    for id in ids_of(0x2B) {
        let m: Material = r.get(id).unwrap_or_else(|| panic!("{id:08X} decodes"));
        let s = sub_objects(&m, HARDWARE_CLASS);
        layers += m.layers.len();
        kept += s.layers.len();
        empty += m
            .layers
            .iter()
            .filter(|l| l.stages.is_empty() && l.flags_set & CLASS_TWO != 0)
            .count();
        assert!(s.layers.iter().all(|l| !l.stages.is_empty()), "{id:08X}");
        assert!(
            s.layers
                .iter()
                .all(|l| l.flags_set & OTHER_CLASSES == 0 && l.flags_clear & CLASS_TWO == 0),
            "{id:08X}"
        );
        for p in &s.parameters.parameters {
            for f in &p.fields {
                assert!(
                    (f.words[2] as usize) < s.layers.len(),
                    "{id:08X} {:?}",
                    f.words
                );
            }
        }
    }
    println!("{layers} layers, {kept} kept for the class, {empty} of the class's without stages");
    assert!(kept > 0 && kept < layers);
    let cells: Material = r.get(0x2B00_005B).expect("the interior cells' material");
    let s = sub_objects(&cells, HARDWARE_CLASS);
    let bump = s
        .layers
        .iter()
        .find(|l| !l.pixel_shader.is_empty())
        .expect("the normal-mapped layer");
    assert_eq!(vertex_inputs(bump), ([0, 0, 0, 0x500], 0b100));
}

/// The layers' programs, lit by each count of lights they are made for, whose program is too long
/// for the device: by material, stored layer and (directional, point) lights. Every one is a
/// normal-mapped layer lit by four point lights (or three and the sun); the interior cells' own.
const TOO_LONG: [(u32, usize, u32, u32); 28] = [
    (0x2B00_0022, 5, 1, 4),
    (0x2B00_0027, 5, 1, 4),
    (0x2B00_0030, 5, 1, 4),
    (0x2B00_004F, 5, 1, 4),
    (0x2B00_0052, 5, 0, 4),
    (0x2B00_0052, 5, 1, 4),
    (0x2B00_005B, 5, 0, 4),
    (0x2B00_005C, 5, 0, 4),
    (0x2B00_005C, 5, 1, 4),
    (0x2B00_0061, 5, 0, 4),
    (0x2B00_0061, 5, 1, 4),
    (0x2B00_0069, 5, 1, 4),
    (0x2B00_006A, 5, 0, 4),
    (0x2B00_006A, 5, 1, 4),
    (0x2B00_0079, 5, 1, 4),
    (0x2B00_007F, 5, 1, 4),
    (0x2B00_0081, 5, 1, 4),
    (0x2B00_0084, 5, 1, 4),
    (0x2B00_0086, 5, 0, 4),
    (0x2B00_0086, 5, 1, 3),
    (0x2B00_0086, 5, 1, 4),
    (0x2B00_0088, 5, 0, 4),
    (0x2B00_0088, 5, 1, 4),
    (0x2B00_008A, 5, 0, 4),
    (0x2B00_008A, 5, 1, 4),
    (0x2B00_00D2, 5, 1, 4),
    (0x2B00_00E2, 5, 1, 4),
    (0x2B00_00E5, 5, 1, 4),
];

/// A layer has no program for a count of lights whose program would be too long for the device: of
/// every program the materials of the install make for the shader-capable class, exactly the 28
/// above are too long, and a draw with that many lights takes the nearest shorter program; the
/// interior cells' normal-mapped layer is lit by at most three point lights.
///
/// Behaviour: draw.material.a-program-too-long-for-the-device-is-not-made
#[test]
fn a_program_too_long_for_the_device_is_not_made() {
    // The layer flags of the shader-capable hardware class, and of the others.
    const CLASS_TWO: u32 = 1 << 21;
    const OTHER_CLASSES: u32 = 1 << 20 | 1 << 22;
    let r = &install().records;
    let (mut programs, mut too_long) = (0, Vec::new());
    for id in ids_of(0x2B) {
        let m: Material = r.get(id).unwrap_or_else(|| panic!("{id:08X} decodes"));
        for (k, l) in m.layers.iter().enumerate() {
            if l.stages.is_empty()
                || l.flags_set & OTHER_CLASSES != 0
                || l.flags_clear & CLASS_TWO != 0
            {
                continue;
            }
            let sw = switches(l);
            let (sources, matrices) = vertex_inputs(l);
            for (d, p) in light_variants(sw) {
                let n = program_length(sw, sources, matrices, d, p)
                    .unwrap_or_else(|| panic!("{id:08X} layer {k} makes a program for {d}, {p}"));
                programs += 1;
                if n > PROGRAM_PIECES_LIMIT {
                    too_long.push((id, k, d, p));
                }
            }
            let made = layer_light_variants(l);
            assert!(
                made.iter()
                    .all(|&(d, p)| !TOO_LONG.contains(&(id, k, d, p))),
                "{id:08X} layer {k}: {made:?}"
            );
        }
    }
    too_long.sort_unstable();
    println!("{programs} programs, {} too long", too_long.len());
    assert!(programs > 1_000, "{programs}");
    assert_eq!(too_long, TOO_LONG);
    let cells: Material = r.get(0x2B00_005B).expect("the interior cells' material");
    let bump = &cells.layers[5];
    let (sources, matrices) = vertex_inputs(bump);
    assert_eq!(layer_light_variants(bump), [(0, 1), (0, 2), (0, 3)]);
    assert_eq!(
        program_length(switches(bump), sources, matrices, 0, 3),
        Some(136)
    );
    assert_eq!(
        program_length(switches(bump), sources, matrices, 0, 4),
        Some(161)
    );
}

/// The layers whose pixel program lights the specular per pixel from the half vector are exactly
/// the device's layers that hand a stage the half vector between the light and the eye in the
/// surface's frame as its coordinates (source `0x600`): six materials, the vector in the third
/// stage or the fourth; none has a texture there.
///
/// Behaviour: draw.material.a-half-vector-stage-feeds-the-per-pixel-specular
#[test]
fn a_half_vector_stage_feeds_the_per_pixel_specular() {
    use auberean_draw::material::PixelProgram;
    let r = &install().records;
    let mut with = Vec::new();
    for id in ids_of(0x2B) {
        let m: Material = r.get(id).unwrap_or_else(|| panic!("{id:08X} decodes"));
        for l in &sub_objects(&m, HARDWARE_CLASS).layers {
            let (sources, _) = vertex_inputs(l);
            let stage = sources.iter().position(|&s| s == 0x600);
            let program = PixelProgram::from_text(&l.pixel_shader).half_vector_specular;
            assert_eq!(stage.is_some(), program, "{id:08X}");
            if let Some(k) = stage {
                assert_eq!(l.stages[k].texture.0, 0, "{id:08X}");
                with.push((id, k));
            }
        }
    }
    with.sort_unstable();
    with.dedup();
    let materials: std::collections::BTreeSet<u32> = with.iter().map(|(m, _)| *m).collect();
    assert_eq!(
        materials.into_iter().collect::<Vec<_>>(),
        [
            0x2B00_005C,
            0x2B00_0061,
            0x2B00_006A,
            0x2B00_0086,
            0x2B00_0088,
            0x2B00_008A
        ]
    );
    assert!(with.iter().all(|(_, k)| *k == 2 || *k == 3));
}

/// The stages that read the anisotropic coordinates (source `0x200`: how squarely the light and the
/// half vector meet the surface) are three materials' anisotropic maps: every instance of the two
/// in use names a picture for that stage, which the coordinates look up.
///
/// Behaviour: draw.material.an-anisotropic-stage-looks-up-the-instances-map
#[test]
fn an_anisotropic_stage_looks_up_the_instances_map() {
    let r = &install().records;
    let mut stages = Vec::new();
    for id in ids_of(0x2B) {
        let m: Material = r.get(id).unwrap_or_else(|| panic!("{id:08X} decodes"));
        for (k, l) in sub_objects(&m, HARDWARE_CLASS).layers.iter().enumerate() {
            if let Some(s) = vertex_inputs(l).0.iter().position(|&s| s == 0x200) {
                stages.push((id, k, s));
            }
        }
    }
    let templates: Vec<u32> = stages.iter().map(|(m, _, _)| *m).collect();
    assert_eq!(
        templates,
        [0x2B00_0051, 0x2B00_0052, 0x2B00_0086],
        "{stages:X?}"
    );
    let mut instances = 0;
    for id in ids_of(0x31) {
        let i: MaterialInstance = r.get(id).unwrap_or_else(|| panic!("{id:08X} decodes"));
        let Some(&(_, layer, stage)) = stages.iter().find(|(m, _, _)| *m == i.material.0) else {
            continue;
        };
        let m = instance_material(r, id).unwrap_or_else(|| panic!("{id:08X} is made ready"));
        let l = &sub_objects(&m, HARDWARE_CLASS).layers[layer];
        let t = l.stages[stage].texture.0;
        assert_eq!(t >> 24, 0x40, "{id:08X}'s anisotropic map {t:08X}");
        instances += 1;
    }
    println!("{instances} instances with an anisotropic map");
    assert_eq!(instances, 6);
}

/// A material whose pictures slide, turn or stretch, or whose shine moves, is drawn as it stands
/// at the moment of the client's run: the layers that move change from one moment to the next,
/// and a picture sliding at a speed slides ten times as far in ten seconds as in one.
///
/// Behaviour: draw.material.a-moving-material-is-drawn-as-it-stands-at-the-moment
#[test]
fn a_moving_material_is_drawn_as_it_stands_at_the_moment() {
    let r = &install().records;
    let rng = auberean_primitives::random::Random::new();
    let (mut moving, mut changed, mut sliding) = (0, 0, 0);
    for id in ids_of(0x31) {
        let Some(m) = instance_material(r, id) else {
            continue;
        };
        for layer in draw_layers(&m, DEVICE_FLAGS) {
            if layer.motion.is_none() {
                continue;
            }
            moving += 1;
            let at = |t: f64| {
                let mut l = layer.clone();
                l.move_on(t, &rng);
                l
            };
            let (a, b, c) = (at(100.0), at(101.0), at(110.0));
            if a != b {
                changed += 1;
            }
            // a slide: the offsets move evenly
            let offsets = |l: &auberean_draw::material::DrawLayer| {
                l.stages
                    .iter()
                    .map(|s| (s.uv_transform[2], s.uv_transform[5]))
                    .collect::<Vec<_>>()
            };
            let (oa, ob, oc) = (offsets(&a), offsets(&b), offsets(&c));
            for ((a, b), c) in oa.iter().zip(&ob).zip(&oc) {
                let (d1, d10) = (b.0 - a.0, c.0 - a.0);
                if d1.abs() > 1e-3 && (d10 - 10.0 * d1).abs() < 1e-2 * d10.abs() {
                    sliding += 1;
                }
            }
        }
    }
    println!("{moving} moving layers, {changed} changed in a second, {sliding} sliding stages");
    assert!(
        moving > 50 && changed > 50 && sliding > 10,
        "{moving} {changed} {sliding}"
    );
}

/// The stages that read the eye's reflection about the surface (source `0x100`) are three
/// materials' environment maps, in the normal-mapped layer's third stage of two of them and its
/// second of the third. The pixel programs of the two in use add the map to the diffuse map, one
/// alone and one times the diffuse map's alpha; the third has no instance. Every one of their 42
/// instances (40 of the first, 2 of the second) is drawn with a layer whose third stage looks a
/// picture up by the reflection and whose program adds it.
///
/// Behaviour: draw.material.an-environment-map-is-looked-up-by-the-eyes-reflection
#[test]
fn an_environment_map_is_looked_up_by_the_eyes_reflection() {
    use auberean_draw::material::PixelProgram;
    let r = &install().records;
    let mut stages = Vec::new();
    for id in ids_of(0x2B) {
        let m: Material = r.get(id).unwrap_or_else(|| panic!("{id:08X} decodes"));
        for l in &sub_objects(&m, HARDWARE_CLASS).layers {
            if let Some(s) = vertex_inputs(l).0.iter().position(|&s| s == 0x100) {
                let p = PixelProgram::from_text(&l.pixel_shader);
                stages.push((id, s, p.environment_add, p.environment_gloss));
            }
        }
    }
    assert_eq!(
        stages,
        [
            (0x2B00_0069, 2, true, false),
            (0x2B00_00DD, 1, false, false),
            (0x2B00_00E5, 2, false, true),
        ]
    );
    let mut by_template = std::collections::BTreeMap::new();
    let mut maps = std::collections::BTreeSet::new();
    for id in ids_of(0x31) {
        let i: MaterialInstance = r.get(id).unwrap_or_else(|| panic!("{id:08X} decodes"));
        if !stages.iter().any(|(m, ..)| *m == i.material.0) {
            continue;
        }
        *by_template.entry(i.material.0).or_insert(0) += 1;
        let m = instance_material(r, id).unwrap_or_else(|| panic!("{id:08X} is made ready"));
        let drawn: Vec<_> = draw_layers(&m, DEVICE_FLAGS)
            .into_iter()
            .filter(|l| l.stages.get(2).is_some_and(|s| s.tex_coord == 0x100))
            .collect();
        assert_eq!(drawn.len(), 1, "{id:08X}");
        let l = &drawn[0];
        let t = l.stages[2].texture;
        assert_eq!(t >> 24, 0x40, "{id:08X}'s environment map {t:08X}");
        assert!(
            auberean_draw::picture::load(r, t).is_some(),
            "{id:08X}'s map {t:08X} loads"
        );
        maps.insert(t);
        let p = l.pixel_program.unwrap_or_default();
        assert!(p.environment_add != p.environment_gloss, "{id:08X}");
    }
    println!("{by_template:08X?}, maps {maps:08X?}");
    assert_eq!(
        by_template.into_iter().collect::<Vec<_>>(),
        [(0x2B00_0069, 40), (0x2B00_00E5, 2)]
    );
}

/// Most of the install's material instances hold one layer for each material detail (low,
/// medium, high), of which the device draws the one its detail asks for; every instance draws a
/// layer at every detail.
///
/// Behaviour: draw.material.a-material-draws-the-layer-its-detail-asks-for
#[test]
fn a_material_draws_the_layer_its_detail_asks_for() {
    use auberean_draw::material::device_flags;
    let r = &install().records;
    let (mut each, mut instances) = (0, 0);
    let mut undrawn = Vec::new();
    for id in ids_of(0x31) {
        let m = instance_material(r, id).unwrap_or_else(|| panic!("{id:08X} is made ready"));
        instances += 1;
        let kept = sub_objects(&m, HARDWARE_CLASS);
        let details: Vec<u32> = kept.layers.iter().map(|l| l.flags_set & 0x7_0000).collect();
        let drawn = [0, 1, 2].map(|d| draw_layers(&m, device_flags(d)));
        if drawn.iter().any(Vec::is_empty) {
            undrawn.push(id);
        }
        if details == [0x1_0000, 0x2_0000, 0x4_0000] {
            each += 1;
            for (d, layers) in drawn.iter().enumerate() {
                assert_eq!(layers.len(), 1, "{id:08X} at {d}");
                assert_eq!(
                    layers[0],
                    auberean_draw::material::draw_layer(&kept.layers[d]),
                    "{id:08X} at {d}"
                );
            }
        }
    }
    println!("{each} of {instances} instances hold a layer for each detail");
    assert!(undrawn.is_empty(), "{undrawn:08X?}");
    assert!(
        each > 2_000 && each * 100 / instances > 80,
        "{each} {instances}"
    );
}
