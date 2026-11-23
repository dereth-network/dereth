//! A block's parts, chosen and listed on the device, against the processor's own choice of the
//! same scene: the same levels and fades, the same draw items for every part seen, none for a
//! part out of view, the same commands frame after frame; and ground cover cast into the nearest
//! of the sun's cascades alone.
//!
//! Behaviour: none (experimental)

use std::collections::BTreeSet;

use dereth_gfx::Format;
use dereth_remaster::assets::pack::{pack_mesh, PackLevel, PackModel, PackSettings};
use dereth_remaster::assets::{
    Appearance, AssetSource, BlendFactor, CellPieceKey, Kind, Material, MaterialKey, Mesh,
    SubsetInfo, SurfaceRef, TerrainCell, Texture, TextureKey, Use, Vertex,
};
use dereth_remaster::render::instances::PartState;
use dereth_remaster::render::light::LightMode;
use dereth_remaster::render::lod::LodSettings;
use dereth_remaster::render::rt_shadow::RtShadowSettings;
use dereth_remaster::render::shadow::ShadowSettings;
use dereth_remaster::render::Renderer;
use dereth_remaster::scene::{
    Block, BlockId, BlockLevels, Camera, FrameDelta, Part, PartLook, Placement, PlacementKind,
    RetailLight, SceneDelta, Terrain,
};
use glam::{Mat4, Vec3};

const BLOCK: BlockId = 0xA9B4;
const SIZE: (u32, u32) = (640, 360);
/// The objects the pack has models for: a plant of two levels, a rock of one, and ground cover of
/// two, past which it draws nothing.
const PLANT: u32 = 0x0100_0001;
const ROCK: u32 = 0x0100_0002;
const COVER: u32 = 0x0100_0003;
/// An object of retail's own, with no model in the pack.
const RETAIL: u32 = 0x0100_0004;
/// Where the eye stands, block-local, looking north.
const EYE: Vec3 = Vec3::new(96.0, 20.0, 3.0);

/// What stands in the block: the object, how far north of the eye and east of its line, its
/// scale, and whether the eye sees it.
const PLACED: &[(u32, f32, f32, f32, bool)] = &[
    (PLANT, 5.0, 0.5, 1.0, true),
    (PLANT, 12.0, -1.0, 0.8, true),
    (PLANT, 25.0, 2.0, 1.3, true),
    (PLANT, 37.0, 0.0, 1.0, true),
    (PLANT, 42.5, 1.0, 1.0, true),
    (PLANT, 44.0, -2.0, 1.1, true),
    (PLANT, 60.0, 3.0, 0.9, true),
    (PLANT, 84.0, -3.0, 1.0, true),
    (PLANT, 95.0, 4.0, 1.0, true),
    (PLANT, 99.0, -4.0, 1.2, true),
    (PLANT, 140.0, 5.0, 1.0, true),
    // Behind the eye, and far off to the side.
    (PLANT, -15.0, 0.0, 1.0, false),
    (PLANT, 6.0, 70.0, 1.0, false),
    (ROCK, 8.0, 1.5, 1.0, true),
    (ROCK, 50.0, -1.5, 1.0, true),
    (COVER, 8.0, -1.5, 1.0, true),
    (COVER, 50.0, 1.5, 1.0, true),
    (RETAIL, 10.0, 0.0, 1.0, true),
];

/// A triangle pair a meter square at height `z`, drawn from both sides, on surface 7.
fn square(z: f32) -> Mesh {
    let v = |x: f32, y: f32| Vertex {
        position: [x - 0.5, y - 0.5, z],
        normal: [0.0, 0.0, 1.0],
        uv: [x, y],
    };
    Mesh::from_triangle_lists([(
        SubsetInfo {
            surface: SurfaceRef::Surface(7),
            two_sided: true,
            tiled: false,
        },
        vec![
            v(0.0, 0.0),
            v(1.0, 0.0),
            v(1.0, 1.0),
            v(0.0, 0.0),
            v(1.0, 1.0),
            v(0.0, 1.0),
        ],
    )])
}

/// The models' meshes: each model's first level a square on the ground, its second the same 5 cm
/// higher; retail's object a square too.
struct Source;

impl AssetSource for Source {
    fn gfxobj(&mut self, id: u32) -> Option<Mesh> {
        let lifted = dereth_remaster::assets::pack::of_pack_mesh(id)
            .map_or(0.0, |(_, l)| f32::from(l) * 0.05);
        Some(square(lifted))
    }

    fn cell_piece(&mut self, _key: CellPieceKey) -> Option<Mesh> {
        None
    }

    fn appearance(&mut self, _surface: u32, _look: Option<&PartLook>) -> Appearance {
        Appearance::default()
    }

    fn material(&mut self, _key: &MaterialKey, _used: Use) -> Material {
        Material {
            kind: Kind::Item,
            surface_type: 2,
            data_category: Some(1),
            texture: None,
            clip_map: false,
            alpha_test: None,
            vertex_alpha: 255,
            vertex_colored: false,
            translucent: None,
            additive: false,
            blended: false,
            src_blend: BlendFactor::One,
            dst_blend: BlendFactor::Zero,
            z_write: true,
            fogged: true,
            solid: None,
            luminosity: 0.0,
            diffuse: 1.0,
            maps: None,
        }
    }

    fn texture(&mut self, _key: &TextureKey, _clip_map: bool) -> Option<Texture> {
        None
    }

    fn terrain_cells(&mut self, _block: BlockId, _t: &Terrain) -> Vec<TerrainCell> {
        Vec::new()
    }

    fn sky_objects(&mut self) -> Vec<u32> {
        Vec::new()
    }

    fn sky(&mut self, _id: u32) -> Option<Mesh> {
        None
    }

    fn pack_model(&mut self, id: u32) -> Option<PackModel> {
        let level = |model: u32, k: u8, distance: f32| PackLevel {
            mesh: pack_mesh(model, k),
            distance,
        };
        let nothing = PackLevel {
            mesh: 0,
            distance: f32::INFINITY,
        };
        let category = dereth_remaster::assets::pack::Category::Plant;
        match id {
            PLANT => Some(PackModel {
                category,
                levels: vec![level(0, 0, 40.0), level(0, 1, 90.0), nothing],
                cover: false,
            }),
            ROCK => Some(PackModel {
                category: dereth_remaster::assets::pack::Category::Rock,
                levels: vec![level(1, 0, f32::INFINITY)],
                cover: false,
            }),
            COVER => Some(PackModel {
                category,
                levels: vec![level(2, 0, 20.0), level(2, 1, 35.0), nothing],
                cover: true,
            }),
            _ => None,
        }
    }
}

/// The block of [`PLACED`].
fn block() -> Block {
    block_of(
        &PLACED
            .iter()
            .map(|&(id, north, east, scale, _)| (id, Vec3::new(east, north, 0.0), scale))
            .collect::<Vec<_>>(),
    )
}

/// A block of the objects `placed`: each its object, where it stands from the eye's foot (east,
/// north and up), and its scale.
fn block_of(placed: &[(u32, Vec3, f32)]) -> Block {
    let mut vertices = Vec::new();
    for i in 0..9u8 {
        for j in 0..9u8 {
            vertices.push(Vec3::new(f32::from(i) * 24.0, f32::from(j) * 24.0, 0.0));
        }
    }
    let placements = placed
        .iter()
        .map(|&(id, at, scale)| {
            let frame = Mat4::from_translation(Vec3::new(EYE.x, EYE.y, 0.0) + at);
            Placement {
                id,
                kind: PlacementKind::Object,
                frame,
                scale,
                lifts: [0.0; 2],
                parts: vec![Part {
                    gfxobj: 1,
                    frame,
                    scale: Vec3::splat(scale),
                    degrade: None,
                }],
            }
        })
        .collect();
    Block {
        block: BLOCK,
        generation: 1,
        terrain: Terrain {
            side: 8,
            vertices,
            cuts: vec![false; 64],
            words: vec![4; 81],
            steps: 1,
        },
        placements,
        degrades: Vec::new(),
        rooms: Vec::new(),
        lights: Vec::new(),
        lamps: Vec::new(),
    }
}

fn frame() -> FrameDelta {
    let forward = Vec3::new(0.0, 1.0, -0.08).normalize();
    let aspect = SIZE.0 as f32 / SIZE.1 as f32;
    FrameDelta {
        origin: [0xA9, 0xB4],
        landscape: true,
        drawn_blocks: vec![BLOCK],
        // The walk chose levels for none of the block's records: the pack's are all past its own.
        levels: vec![BlockLevels {
            block: BLOCK,
            levels: Vec::new(),
            turned: Vec::new(),
        }],
        camera: Camera {
            view: Mat4::look_to_lh(EYE, forward, Vec3::Z),
            projection: Mat4::perspective_lh(1.0, aspect, 0.3, 1000.0),
            previous_view_projection: None,
            eye: EYE,
            viewport: [0, 0, SIZE.0, SIZE.1],
            fov_y: 1.0,
            aspect,
            near: 0.3,
            far: 1000.0,
        },
        light: RetailLight {
            sun: Vec3::new(0.3, -0.2, 0.9),
            sun_color: [255; 3],
            ambient_level: 0.3,
            ambient_color: [255; 3],
            object_ambient: [0.3; 3],
            outdoor: true,
            sees_outside: true,
            time_of_day: 0.5,
            ..RetailLight::default()
        },
        ..FrameDelta::default()
    }
}

/// A renderer that has drawn the block for three frames, its parts chosen on the device or on the
/// processor.
fn drawn(on_device: bool) -> Renderer {
    drawn_block(on_device, false, block(), 3)
}

/// A renderer that has drawn `b` for `frames` frames, its parts chosen on the device or on the
/// processor, and the sun's shadows looked up in the cascades or `traced`.
fn drawn_block(on_device: bool, traced: bool, b: Block, frames: usize) -> Renderer {
    let mut r = Renderer::new(crate::hardware(), Format::D32Float).expect("a renderer");
    r.set_mode(LightMode::Pixel);
    r.set_shadows(ShadowSettings {
        on: true,
        ..ShadowSettings::default()
    });
    r.set_lod(LodSettings::ON);
    r.set_packs(PackSettings::all());
    r.set_parts_on_device(on_device);
    if traced {
        r.set_rt_shadows(RtShadowSettings { on: true, rays: 16 });
    }
    r.receive(
        vec![SceneDelta::BlockIn(Box::new(b))],
        frame(),
        [None; 2],
        &mut Source,
    );
    assert!(r.draw_offscreen(SIZE.0, SIZE.1).expect("a frame"));
    for _ in 1..frames {
        r.receive(Vec::new(), frame(), [None; 2], &mut Source);
        assert!(r.draw_offscreen(SIZE.0, SIZE.1).expect("a frame"));
    }
    r
}

/// The items of `ranges` in `list`.
fn among(list: &[u32], ranges: &[std::ops::Range<u32>]) -> BTreeSet<u32> {
    list.iter()
        .copied()
        .filter(|i| ranges.iter().any(|r| r.contains(i)))
        .collect()
}

#[test]
fn the_devices_levels_fades_and_items_are_the_processors_for_every_part_seen() {
    let mut cpu = drawn(false);
    assert_eq!(cpu.parts_on_device(), 0);
    let choices = cpu.part_choices(BLOCK);
    let cpu_items = cpu.culled_items();
    let cpu_list: Vec<u32> = cpu_items.iter().flatten().copied().collect();
    drop(cpu);

    let mut gpu = drawn(true);
    // Every part: the pack's models and retail's own.
    assert_eq!(gpu.parts_on_device(), PLACED.len());
    let states: Vec<PartState> = gpu.part_states();
    let gpu_items = gpu.culled_items();
    let gpu_list: Vec<u32> = gpu_items.iter().flatten().copied().collect();
    assert_eq!(states.len(), PLACED.len());
    let mut fading = 0;
    let mut levels = BTreeSet::new();
    for s in &states {
        let (id, north, ..) = PLACED[s.part as usize];
        let want = choices[s.part as usize];
        let (next, fade) = want.fade.unwrap_or((want.level, 0.0));
        let at = format!("part {} ({id:#x}, {north} m north)", s.part);
        assert_eq!(s.level, want.level, "{at}: {s:?} against {want:?}");
        assert_eq!(s.next, next, "{at}: {s:?} against {want:?}");
        assert!((s.fade - fade).abs() < 1e-4, "{at}: {s:?} against {want:?}");
        assert_eq!(s.seen, PLACED[s.part as usize].4, "{at}: {s:?}");
        fading += usize::from(fade > 0.0);
        levels.insert(s.level);
        let items = gpu.part_items(BLOCK, s.part);
        let on_device = among(&gpu_list, &items);
        if s.seen {
            assert_eq!(on_device, among(&cpu_list, &items), "{at}: the same items");
        } else {
            assert!(on_device.is_empty(), "{at}: drawn out of view");
        }
    }
    assert!(fading >= 2, "{fading} models fade");
    assert!(levels.len() >= 3, "levels {levels:?}");
    // Retail's object is drawn as the processor draws it.
    let retail = PLACED.iter().position(|p| p.0 == RETAIL).unwrap() as u32;
    let items = gpu.part_items(BLOCK, retail);
    assert!(!among(&gpu_list, &items).is_empty());
    assert_eq!(among(&gpu_list, &items), among(&cpu_list, &items));

    // The same frame again lists the same commands in the same order.
    gpu.receive(Vec::new(), frame(), [None; 2], &mut Source);
    assert!(gpu.draw_offscreen(SIZE.0, SIZE.1).expect("a frame"));
    assert_eq!(gpu.culled_items(), gpu_items, "the same order");
    drop(gpu);
    crate::no_validation_errors();
}

#[test]
fn ground_cover_casts_into_the_nearest_cascade_alone_and_the_rest_into_every_one_they_reach() {
    // Every cascade is drawn again on the block's first frame.
    let mut first = drawn_first_frame();
    let states = first.part_states();
    let casters = first.part_casters();
    let state = |id: u32, north: f32| {
        let k = PLACED
            .iter()
            .position(|p| p.0 == id && p.1 == north)
            .unwrap() as u32;
        (
            states
                .iter()
                .find(|s| s.part == k)
                .copied()
                .expect("chosen"),
            first.part_items(BLOCK, k),
        )
    };
    let (near_cover, near_items) = state(COVER, 8.0);
    let (far_cover, far_items) = state(COVER, 50.0);
    let (near_rock, rock_items) = state(ROCK, 8.0);
    let (far_rock, far_rock_items) = state(ROCK, 50.0);
    assert_eq!(near_cover.casts, 0b0001, "{near_cover:?}");
    assert_eq!(far_cover.casts, 0, "{far_cover:?}");
    assert!(near_rock.casts & 0b0001 != 0, "{near_rock:?}");
    assert!(near_rock.casts.count_ones() > 1, "{near_rock:?}");
    assert!(far_rock.casts & 0b1000 != 0, "{far_rock:?}");
    let first_level = |items: &[std::ops::Range<u32>]| items[0].clone().collect::<BTreeSet<u32>>();
    for (c, list) in casters.iter().enumerate() {
        let list: BTreeSet<u32> = list.iter().copied().collect();
        let has = |items: &[std::ops::Range<u32>]| !list.is_disjoint(&first_level(items));
        assert_eq!(has(&near_items), c == 0, "cascade {c}: the near cover");
        assert!(!has(&far_items), "cascade {c}: the far cover");
        assert_eq!(
            has(&rock_items),
            near_rock.casts & (1 << c) != 0,
            "cascade {c}"
        );
        assert_eq!(
            has(&far_rock_items),
            far_rock.casts & (1 << c) != 0,
            "cascade {c}"
        );
    }
    drop(first);
    crate::no_validation_errors();
}

/// A renderer that has drawn the block's first frame alone, its cascades all drawn again.
fn drawn_first_frame() -> Renderer {
    drawn_block(true, false, block(), 1)
}

/// The sun's share at world point `p` (block-local) in `r`'s last frame, 0 to 255.
fn sun_at(r: &mut Renderer, p: Vec3) -> u8 {
    let f = frame();
    let clip = f.camera.projection * f.camera.view * p.extend(1.0);
    let ndc = clip.truncate() / clip.w;
    let back = r.read_back().expect("a frame");
    let x = ((ndc.x * 0.5 + 0.5) * back.render_size.0 as f32) as usize;
    let y = ((0.5 - ndc.y * 0.5) * back.render_size.1 as f32) as usize;
    let sun = back.sun_visibility.expect("the sun's share");
    sun[y * back.render_size.0 as usize + x]
}

/// Ground cover's shadow is cast near the eye and not past the nearest cascade's reach, on the
/// device and on the processor, from the cascades and traced, while a rock of the same shape casts
/// at either distance.
#[test]
fn ground_covers_shadow_falls_near_the_eye_and_not_past_the_nearest_cascade() {
    // A broad rock for the ground, and over it a square three meters across and a meter and a
    // half up, 8 m and 22 m ahead.
    let ground = (ROCK, Vec3::new(0.0, 25.0, 0.0), 50.0);
    let over = |id: u32, north: f32| (id, Vec3::new(0.0, north, 1.5), 3.0);
    // Where each square's shadow falls: the sun is 0.3 east, 0.2 south, 0.9 up.
    let under =
        |north: f32| Vec3::new(EYE.x, EYE.y + north, 0.0) + Vec3::new(0.3, -0.2, 0.0) * (1.5 / 0.9);
    let traces = crate::hardware().caps().ray_queries;
    for (on_device, traced) in [(false, false), (true, false), (true, true)] {
        if traced && !traces {
            eprintln!("the device traces no rays: the traced shadows are not checked");
            continue;
        }
        let mut shares = Vec::new();
        for id in [COVER, ROCK] {
            let b = block_of(&[ground, over(id, 8.0), over(id, 22.0)]);
            let mut r = drawn_block(on_device, traced, b, 2);
            shares.push((sun_at(&mut r, under(8.0)), sun_at(&mut r, under(22.0))));
            // Beside the shadows, the ground is in the sun.
            assert!(
                sun_at(&mut r, under(15.0)) > 200,
                "on the device: {on_device}, traced: {traced}"
            );
        }
        let [(cover_near, cover_far), (rock_near, rock_far)] = shares[..] else {
            unreachable!()
        };
        let at = format!("on the device: {on_device}, traced: {traced}: {shares:?}");
        assert!(cover_near < 64, "{at}: cover casts near the eye");
        assert!(cover_far > 200, "{at}: cover casts nothing 22 m off");
        assert!(
            rock_near < 64 && rock_far < 64,
            "{at}: the rock casts at both"
        );
    }
    crate::no_validation_errors();
}
