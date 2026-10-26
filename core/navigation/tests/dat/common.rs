//! The shared fixture: dungeon or outdoor landblocks from the retail dats, loaded the way the
//! client loads them (cells and buildings into the land source; cell statics, scenery and
//! landblock statics into a physics world as bodies), with a planner over the same data and a
//! human body to walk routes with.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use dereth_assets::Decode;
use dereth_dat::{DbType, RetailDatStore};
use dereth_navigation::{
    LandNavSource, NavConfig, NavWorld, PhysicsWalker, Region, StaticPlacement, WalkerConfig,
};
use dereth_physics::{PhysicsWorld, SetupGeometry};
use dereth_primitives::{CellId, DataId, Frame, LandblockId, Position, Quat, Vec3};
use dereth_world_data::env_cells::{
    cell_statics, static_geometry, CellStatic, CellStaticObjects, EnvCellLoader,
};
use dereth_world_data::land_source::DatLandSource;
use dereth_world_data::setup::SetupPartStats;

/// The setups the planner reads, memoised.
pub type SetupFn = Box<dyn Fn(DataId) -> Option<Arc<SetupGeometry>>>;

/// The outdoor placements the planner reads, memoised.
pub type LandFn = Box<dyn Fn(LandblockId) -> Vec<StaticPlacement>>;

/// The planner over the fixture's data.
pub type DatNav = NavWorld<LandNavSource<Arc<DatLandSource>, SetupFn, LandFn>>;

/// Loaded landblocks: a dungeon's one, or an outdoor window.
pub struct Dungeon {
    pub block: u16,
    pub world: PhysicsWorld,
    pub nav: DatNav,
    pub player: Arc<SetupGeometry>,
}

/// The retail store, opened once per test binary.
pub fn store() -> Arc<RetailDatStore> {
    static STORE: std::sync::OnceLock<Arc<RetailDatStore>> = std::sync::OnceLock::new();
    Arc::clone(STORE.get_or_init(|| Arc::new(dereth_dat::testing::open_store_or_fail())))
}

/// The human body the routes are walked with: setup `0x02000001`, spheres only, as the player.
pub fn player_setup(store: &RetailDatStore) -> Arc<SetupGeometry> {
    let id = DataId(0x0200_0001);
    let bytes = store
        .read_typed(DbType::Setup, id)
        .expect("the human setup");
    let s = dereth_assets::Setup::decode_payload(id, &bytes).expect("decodes");
    Arc::new(dereth_world_data::setup::setup_geometry(&s))
}

/// Load `blocks` with `config`: their cells and buildings, every static of their cells and, with
/// `outdoors`, their scenery and landblock statics. The first block is the one tests start in.
pub fn load(blocks: &[u16], outdoors: bool, config: NavConfig) -> Dungeon {
    let store = store();
    let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
    let land = Arc::new(DatLandSource::new(Arc::clone(&store), &region).expect("height table"));
    let mut world = PhysicsWorld::new(Arc::clone(&land) as Arc<dyn dereth_physics::LandSource>);
    let mut statics = CellStaticObjects::new();
    let mut placements: Vec<CellStatic> = Vec::new();
    let mut outdoor: HashMap<u16, Vec<StaticPlacement>> = HashMap::new();
    let mut loader = EnvCellLoader::new();
    for &block in blocks {
        land.load_block_cells(LandblockId(block));
        let decoded = loader.load_block(&store, block);
        placements.extend(decoded.iter().flat_map(cell_statics));
        if outdoors {
            let land_statics =
                dereth_client_runtime::world_build::land_placements(&store, &region, block);
            outdoor.insert(
                block,
                land_statics
                    .iter()
                    .map(|s| StaticPlacement {
                        id: s.id,
                        frame: s.frame,
                        scale: s.scale,
                    })
                    .collect(),
            );
            placements.extend(land_statics);
        }
    }
    statics.init(&store, &mut world, &placements);
    assert_eq!(statics.stats.unplaced, 0, "every static's cell is resident");
    assert_eq!(statics.stats.undecodable, 0, "every static's setup decodes");

    let memo: Mutex<HashMap<u32, Option<Arc<SetupGeometry>>>> = Mutex::new(HashMap::new());
    let setup_store = Arc::clone(&store);
    let setups: SetupFn = Box::new(move |id: DataId| {
        let mut m = memo.lock().expect("not poisoned");
        m.entry(id.0)
            .or_insert_with(|| {
                static_geometry(&setup_store, id, &mut SetupPartStats::default()).map(Arc::new)
            })
            .clone()
    });
    // Only the loaded blocks answer, as only they are resident in the client.
    let land_statics: LandFn =
        Box::new(move |b: LandblockId| outdoor.get(&b.0).cloned().unwrap_or_default());
    let nav = NavWorld::new(
        LandNavSource {
            land,
            setups,
            land_statics,
        },
        config,
    );
    Dungeon {
        block: blocks[0],
        world,
        nav,
        player: player_setup(&store),
    }
}

/// Load a dungeon `block` with `config`.
pub fn dungeon_with(block: u16, config: NavConfig) -> Dungeon {
    load(&[block], false, config)
}

/// Load a dungeon `block` with the default configuration.
pub fn dungeon(block: u16) -> Dungeon {
    dungeon_with(block, NavConfig::default())
}

/// Load the outdoor window of landblocks round `centre`, `radius` blocks each way.
pub fn outdoors(centre: u16, radius: i32) -> Dungeon {
    outdoors_with(centre, radius, NavConfig::default())
}

/// [`outdoors`] with a configuration.
pub fn outdoors_with(centre: u16, radius: i32, config: NavConfig) -> Dungeon {
    let (cx, cy) = (i32::from(centre >> 8), i32::from(centre & 0xFF));
    let mut blocks = vec![centre];
    for x in cx - radius..=cx + radius {
        for y in cy - radius..=cy + radius {
            let id = u16::try_from((x << 8) | y).expect("inside the world");
            if id != centre {
                blocks.push(id);
            }
        }
    }
    load(&blocks, true, config)
}

impl Dungeon {
    /// The first interior cell of the block.
    pub fn first_cell(&self) -> dereth_primitives::CellId {
        dereth_primitives::CellId((u32::from(self.block) << 16) | 0x0100)
    }

    /// A walker over this world.
    pub fn walker(&mut self) -> PhysicsWalker<'_> {
        PhysicsWalker::new(
            &mut self.world,
            Arc::clone(&self.player),
            WalkerConfig::default(),
        )
    }
}

/// A point on the ground of `block`, at the terrain's height.
pub fn ground(d: &Dungeon, block: u16, x: f32, y: f32) -> Position {
    let z = d
        .nav
        .source()
        .land
        .ground_height(LandblockId(block), x, y)
        .expect("the block has terrain");
    let mut c = CellId((u32::from(block) << 16) | 1);
    let mut o = Vec3::new(x, y, z);
    assert!(dereth_physics::landdefs::adjust_to_outside(&mut c, &mut o));
    Position::new(c, Frame::new(o, Quat::IDENTITY))
}

/// A position at a place of a region.
pub fn at(region: &Region, i: u32) -> Position {
    region.position(i)
}

/// The largest connected set of clear places, as the labels and the set's label.
pub fn largest_component(region: &Region) -> (HashMap<u32, u32>, u32) {
    let (label, sizes) = region.components();
    let biggest = sizes
        .iter()
        .enumerate()
        .max_by_key(|(_, s)| **s)
        .map(|(i, _)| u32::try_from(i).expect("small"))
        .expect("the region has a clear place");
    (label, biggest)
}

/// Two far-apart places of the largest connected set: the place farthest, in steps, from an
/// arbitrary member, and the place farthest from that. Both ends of the set's longest walk, near
/// enough.
pub fn far_pair(region: &Region) -> (u32, u32) {
    let (label, biggest) = largest_component(region);
    let seed = region
        .places()
        .find(|i| label.get(i) == Some(&biggest))
        .expect("the set has a member");
    let a = farthest_from(region, seed);
    let b = farthest_from(region, a);
    (a, b)
}

/// The place the most steps from `start`.
pub fn farthest_from(region: &Region, start: u32) -> u32 {
    let mut seen = std::collections::HashSet::from([start]);
    let mut frontier = vec![start];
    let mut last = start;
    while !frontier.is_empty() {
        let mut next = Vec::new();
        for i in frontier {
            last = i;
            for j in region.neighbours(i) {
                if seen.insert(j) {
                    next.push(j);
                }
            }
        }
        frontier = next;
    }
    last
}

/// Where test pictures go.
pub fn out_dir() -> std::path::PathBuf {
    let d = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("navigation");
    std::fs::create_dir_all(&d).expect("the target dir is writable");
    d
}

/// The place a body standing at `start` walks to that lies lowest, the farthest of those within
/// a metre of the lowest.
pub fn bottom(region: &dereth_navigation::Region, start: u32) -> u32 {
    let mut seen = std::collections::HashMap::from([(start, 0_u32)]);
    let mut frontier = vec![start];
    let mut steps = 0;
    while !frontier.is_empty() {
        steps += 1;
        let mut next = Vec::new();
        for i in frontier {
            for j in region.neighbours(i) {
                if let std::collections::hash_map::Entry::Vacant(e) = seen.entry(j) {
                    e.insert(steps);
                    next.push(j);
                }
            }
        }
        frontier = next;
    }
    let low = seen
        .keys()
        .map(|&i| region.node_point(i).z)
        .fold(f32::MAX, f32::min);
    seen.iter()
        .filter(|(&i, _)| region.node_point(i).z < low + 1.0)
        .max_by_key(|(_, &s)| s)
        .map_or(start, |(&i, _)| i)
}

/// The clear place of the region lowest of all, walked to or not.
pub fn lowest(region: &dereth_navigation::Region) -> Option<u32> {
    region
        .places()
        .filter(|&i| region.is_clear(i))
        .min_by(|&a, &b| region.node_point(a).z.total_cmp(&region.node_point(b).z))
}
