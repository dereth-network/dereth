//! A part drawn as the client draws one: the mesh its instance holds (the one every part showing it
//! shares, or the part's own copy when its look changes the materials), carried through the part's
//! frame, culled by its mesh's box carried the same way.

use std::sync::Arc;

use auberean_assets::mesh::Mesh;
use auberean_assets::ui::UiScene;
use auberean_draw::material::d3d;
use auberean_draw::scene::{Camera, Cull, SceneBuilder, Spot};
use auberean_primitives::{DataId, Vec3};
use auberean_world::inside::{BlockLights, Light};
use auberean_world::look::{MaterialAtom, PartLook, ANY_KIND};
use auberean_world::math::{quat_axis_angle, Mat4};
use auberean_world::place::{entity_parts, Part};
use auberean_world::sky::DayLight;

use crate::common::install;

/// The additive particle template (source alpha onto the frame, one), which no placed mesh's own
/// material uses at the camp.
const ADDITIVE: DataId = DataId(0x2B00_0023);

/// Three parts showing one mesh, the second with a look that swaps its materials' template: the
/// first and third draw the mesh's own materials (the mesh they share), the second its own copy
/// with the new template. Each part is drawn through its own frame, and culled by the box about
/// its mesh's box carried through that frame (here turned a quarter round and moved).
///
/// Behaviour: draw.world.a-part-draws-its-own-copy-when-its-look-changes-materials
#[test]
fn a_part_draws_its_own_copy_when_its_look_changes_its_materials() {
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let camp = Spot {
        block: (0xC1, 0x56),
        position: Vec3::new(122.842, 89.758, 81.6),
        heading: 90.0,
        pitch: 5.0,
        time_of_day: 0.45,
        day: None,
    };
    let camp_frame = builder.frame(&camp, 60.0);
    // a placed mesh at the camp whose own layers are not already additive
    let mesh_id = camp_frame
        .objects
        .iter()
        .find(|b| {
            b.mesh != 0
                && b.mesh >> 24 != 0x01
                && (b.layer.source_blend, b.layer.dest_blend)
                    != (d3d::BLEND_SRC_ALPHA, d3d::BLEND_ONE)
        })
        .map(|b| b.mesh)
        .expect("a placed mesh at the camp");
    let mesh: Mesh = r.get(mesh_id).expect("the mesh");
    let turn = quat_axis_angle(Vec3::new(0.0, 0.0, 1.0), std::f32::consts::FRAC_PI_2);
    let at = |x: f32| Mat4::from_parts(Vec3::new(x, 5.0, 2.0), turn, Vec3::new(1.0, 1.0, 1.0));
    let look = Arc::new(PartLook {
        meshes: Vec::new(),
        materials: vec![MaterialAtom {
            priority: 1.0,
            template: ADDITIVE,
            modifier: DataId(0),
            mesh_kind: ANY_KIND,
            material_kind: ANY_KIND,
        }],
        opacity: None,
    });
    let part = |x: f32, look: Option<Arc<PartLook>>| Part {
        mesh: DataId(mesh_id),
        transform: at(x),
        detail: None,
        look,
    };
    let parts = [part(0.0, None), part(100.0, Some(look)), part(200.0, None)];
    let camera = Camera {
        position: Vec3::new(100.0, -50.0, 10.0),
        heading: 0.0,
        pitch: 0.0,
        fov_y: 60.0,
        near: 0.1,
        far: 500.0,
        orthographic: None,
    };
    let day = builder.day(&camp);
    let light = builder.light(&camp, day.as_ref());
    let frame = builder.parts_frame(&parts, Vec::new(), camera, light, None);
    let of = |x: f32| -> Vec<_> {
        frame
            .objects
            .iter()
            .filter(|b| (b.transform.0[3][0] - x).abs() < 0.01)
            .collect()
    };
    let (first, second, third) = (of(0.0), of(100.0), of(200.0));
    println!("mesh {mesh_id:#010x}: {} draws a part", first.len());
    assert!(!first.is_empty(), "the part drawn");
    assert_eq!(
        first.len(),
        second.len(),
        "the copy has the mesh's fragments"
    );
    let blends = |bs: &[&auberean_draw::scene::ObjectBatch]| -> Vec<(u32, u32)> {
        bs.iter()
            .map(|b| (b.layer.source_blend, b.layer.dest_blend))
            .collect()
    };
    assert!(
        blends(&second)
            .iter()
            .all(|&b| b == (d3d::BLEND_SRC_ALPHA, d3d::BLEND_ONE)),
        "the looked part draws its own copy: {:?}",
        blends(&second)
    );
    assert_eq!(
        blends(&first),
        blends(&third),
        "the parts without a look share the mesh's own"
    );
    assert!(
        blends(&first)
            .iter()
            .all(|&b| b != (d3d::BLEND_SRC_ALPHA, d3d::BLEND_ONE)),
        "the mesh's own materials are its own"
    );
    // the box: the mesh's own box, turned a quarter round (x to y, y to -x) and moved
    let b = mesh.bounding_box();
    for (x, draws) in [(0.0, &first), (100.0, &second), (200.0, &third)] {
        let lo = Vec3::new(x - b.max.y, 5.0 + b.min.x, 2.0 + b.min.z);
        let hi = Vec3::new(x - b.min.y, 5.0 + b.max.x, 2.0 + b.max.z);
        for d in draws.iter() {
            let Cull::Box { lo: l, hi: h } = d.cull else {
                panic!("a part is culled by its box: {:?}", d.cull);
            };
            for (got, want) in [(l, lo), (h, hi)] {
                assert!(
                    (got.x - want.x).abs() < 1e-3
                        && (got.y - want.y).abs() < 1e-3
                        && (got.z - want.z).abs() < 1e-3,
                    "the part's box {got:?}, not {want:?}"
                );
            }
        }
    }
}

/// The avatar hall's scene names lights of its own: each draw takes those it keeps where it stands
/// in place of the sun, shining their colors times their intensities, not the one light the sun
/// would be.
///
/// Behaviour: draw.view.a-scenes-own-lights-light-its-draws-in-place-of-the-sun
#[test]
fn a_scenes_own_lights_light_its_draws_in_place_of_the_sun() {
    const HALL: u32 = 0x2100_000B;
    const LIGHT: u32 = 0x4000_0006;
    let r = &install().records;
    let scene = r.get::<UiScene>(HALL).expect("the hall's scene");
    let mut lights = Vec::new();
    let mut parts = Vec::new();
    for e in &scene.entities {
        if e.kind == Some(LIGHT) {
            let at = e.offset.map(|o| o.frame.origin).unwrap_or(Vec3::ZERO);
            let props = e.properties.clone().unwrap_or_default();
            lights.push(Light::of_properties(0, at, &props).0);
        } else {
            parts.extend(entity_parts(r, e));
        }
    }
    assert!(lights.len() > 1, "the hall has lights of its own");
    assert!(!parts.is_empty(), "the hall has things standing in it");
    let shines: Vec<(f32, f32, f32)> = lights
        .iter()
        .map(|l| {
            (
                l.color.r * l.intensity,
                l.color.g * l.intensity,
                l.color.b * l.intensity,
            )
        })
        .collect();
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let camera = Camera {
        position: Vec3::ZERO,
        heading: 0.0,
        pitch: 0.0,
        fov_y: 60.0,
        near: 0.1,
        far: 500.0,
        orthographic: None,
    };
    let dark = DayLight {
        ambient_brightness: 0.0,
        direct_brightness: 0.0,
        ..DayLight::DEFAULT
    };
    let all = BlockLights::of_scene(lights);
    let lit = builder.parts_frame(&parts, Vec::new(), camera, dark, Some((&all, false)));
    assert!(!lit.objects.is_empty());
    let mut shone = 0;
    for b in &lit.objects {
        assert!(b.points.inside, "no draw takes the sun");
        for l in &b.points.lights[..b.points.count] {
            shone += 1;
            let c = (l.diffuse.r, l.diffuse.g, l.diffuse.b);
            assert!(shines.contains(&c), "a light of the scene shines: {c:?}");
        }
    }
    assert!(shone > 0, "the lights reach what stands in the hall");

    let sunlit = builder.parts_frame(&parts, Vec::new(), camera, dark, None);
    assert!(sunlit.objects.iter().all(|b| !b.points.inside));
}
