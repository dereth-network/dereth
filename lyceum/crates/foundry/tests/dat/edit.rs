//! Behaviour: none (tooling: the world editor's edit session and picks over the sample world on the retail files)

use dereth_dat::ModernDat;
use dereth_primitives::{DataId, Vec3};
use lyceum_foundry::compile;
use lyceum_foundry::edit::Session;
use lyceum_foundry::pick::{self, Hit, Mask, Models, Ray, BLOCK};
use lyceum_foundry::world::World;

const EDITS: &str = r#"
[[edit]]
do = "place-static"
landblock = "A9B4"
model = 0x0200040B
at = [30.0, 60.0, "ground"]
heading = 45.0

[[edit]]
do = "move-static"
landblock = "A9B4"
static = "stone-west"
at = [20.0, 26.0, "ground"]

[[edit]]
do = "move-static"
landblock = "A9B4"
index = 0
heading = 15.0

[[edit]]
do = "brush"
brush = "raise"
amount = 3
landblock = "A9B4"
at = [0.0, 72.0]
radius = 30.0

[[edit]]
do = "brush"
brush = "terrain"
value = 4
landblock = "A9B4"
at = [12.0, 96.0]
radius = 20.0

[[edit]]
do = "spawn"
weenie = "guide"
landblock = "A8B4"
at = [170.0, 36.0, "ground"]
heading = 270.0

[[edit]]
do = "remove-static"
landblock = "A8B4"
static = "house-stone-2"
"#;

fn session(tag: &str) -> (dereth_dat::testing::ScratchDir, Session) {
    let (dir, world) = super::sample::sample(tag);
    let mut s = Session::open(&world, &dereth_dat::testing::dat_dir(), None).unwrap();
    s.set_today("2026-10-18 00:00:00");
    (dir, s)
}

fn full(s: &Session) -> compile::Compiled {
    let world = World::read(&s.world().path).unwrap();
    let lock = lyceum_foundry::lock::Lock::read(&world.lock_path()).unwrap();
    compile::compile(&world, s.packs(), s.base(), &lock).unwrap()
}

#[test]
fn an_edit_session_over_the_sample_world_compiles_the_bytes_a_build_does() {
    let (dir, mut s) = session("edit-sample");
    let actions = lyceum_foundry::script::actions(EDITS, "holtburg").unwrap();
    assert_eq!(actions.len(), 7);
    for (i, a) in actions.iter().enumerate() {
        let done = s.act(a).unwrap();
        assert!(!done.touched.landblocks.is_empty(), "edit {}", i + 1);
        let f = full(&s);
        assert_eq!(s.compiled().records, f.records, "edit {}", i + 1);
        assert_eq!(s.compiled().sql, f.sql, "edit {}", i + 1);
    }
    // The raise on the A9B4/A8B4 edge reached both landblocks' copies of x = 0 / x = 8.
    let (east, west) = (s.terrain(0xA9B4).unwrap(), s.terrain(0xA8B4).unwrap());
    for y in 2..=4 {
        assert_eq!(east.height[y], west.height[8 * 9 + y], "row {y}");
    }
    s.save().unwrap();
    compile::build(&s.world().path, s.base().dir.as_path(), None, false).unwrap();
    let mine = dir.path().join("mine");
    compile::write_overlay(s.compiled(), s.base(), "sample-world", &mine).unwrap();
    for t in [ModernDat::Portal, ModernDat::Cell] {
        let name = dereth_dat::overlay::container_name(t);
        assert_eq!(
            std::fs::read(mine.join(name)).unwrap(),
            std::fs::read(dir.path().join("build/overlay").join(name)).unwrap(),
            "{name}"
        );
    }
    // Undo all seven: the sources and the records are the sample's again.
    for _ in 0..7 {
        s.undo().unwrap();
    }
    let (_, fresh) = session("edit-sample-fresh");
    assert_eq!(s.compiled().records, fresh.compiled().records);
}

fn down(lb: u16, x: f32, y: f32) -> Ray {
    let o = pick::block_origin(lb);
    Ray {
        origin: Vec3::new(o.x + x, o.y + y, 300.0),
        dir: Vec3::new(0.0, 0.0, -1.0),
    }
}

#[test]
fn a_pick_names_the_static_the_building_and_the_ground_vertex_under_it() {
    let (_dir, s) = session("edit-pick");
    let mut models = Models::default();
    let near = [0xA9B4, 0xA8B4];
    // The sample's west stone, placed at (24, 24) on the ground: the first static after
    // Holtburg's own 114.
    let hit = pick::pick(&s, &mut models, &down(0xA9B4, 24.0, 24.0), &near, Mask::ALL).unwrap();
    assert_eq!(
        hit.hit,
        Hit::Static {
            landblock: 0xA9B4,
            index: 114,
            model: DataId(0x0200_040B)
        }
    );
    // The house at (79.5, 37.5).
    let buildings = Mask {
        statics: false,
        spawns: false,
        ground: false,
        buildings: true,
        cell_statics: false,
        cells: false,
    };
    let hit = pick::pick(&s, &mut models, &down(0xA9B4, 79.5, 37.5), &near, buildings).unwrap();
    assert_eq!(
        hit.hit,
        Hit::Building {
            landblock: 0xA9B4,
            index: 8,
            model: DataId(0x0100_0BC3)
        }
    );
    // The ground just off a vertex names it, and the point is on the compiled terrain.
    let ground = Mask::GROUND;
    let hit = pick::pick(&s, &mut models, &down(0xA8B4, 100.0, 50.0), &near, ground).unwrap();
    let Hit::Ground { point, vertex } = hit.hit else {
        panic!("{hit:?}")
    };
    assert_eq!(vertex, (0xA8 * 8 + 4, 0xB4 * 8 + 2));
    let z = s.ground(0xA8B4, 100.0, 50.0).unwrap();
    assert!((point.z - z).abs() < 0.01, "{point:?} {z}");
    assert!((point.x - (f32::from(0xA8u8) * BLOCK + 100.0)).abs() < 0.01);
}
