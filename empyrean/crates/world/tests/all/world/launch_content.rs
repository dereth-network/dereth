//! Divergence: V461
//! Offline integration of Launch character creation, terrain, vendors and player movement.
//! Fixture: the operator's launch dats (the October 31, 1999 portal and the retail CD's cells) and a
//! Launch pack, hash-pinned by `EMPYREAN_TEST_LAUNCH_FIXTURE`. No synthetic DATs, terrain, content or network host.

use std::collections::BTreeSet;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use dereth_primitives::{ContainerEra, LandblockId as PhysicsLandblockId};
use empyrean_common::clock::ClockSnapshot;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::era::{EraExt, EraId};
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_content::{PackContent, WorldDatabase};
use empyrean_dat::{DatDatabaseType, DatManager, DatSource, RealDats};
use empyrean_entity::enums::{
    DestinationType, HeritageGroup, PositionType, PropertyInt, SkillAdvancementClass, WeenieType,
};
use empyrean_entity::{CharacterCreateInfo, LandblockId, ObjectGuid};
use empyrean_net::{SessionId, SessionState};
use empyrean_testkit::EmptyShard;
use empyrean_world::entity::actions::action_queue::run_actions;
use empyrean_world::entity::actions::i_actor::{action_queue_mut, Actor};
use empyrean_world::entity::{landblock, position_extensions};
use empyrean_world::factories::player_factory::{self, CreateResult, CreatedPlayer};
use empyrean_world::managers::{event_manager, guid_manager, landblock_manager};
use empyrean_world::physics::phys_ext;
use empyrean_world::sessions::SessionData;
use empyrean_world::world_objects::{player_location, player_tick, vendor};
use empyrean_world::World;
use serde::Deserialize;
use sha2::{Digest, Sha256};

mod effects;
mod play;
mod spell_ranges;
mod starters;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    capture: String,
    dat_directory: PathBuf,
    portal_sha256: String,
    cell_sha256: String,
    world_pack: PathBuf,
    world_pack_sha256: String,
}

struct Fixture {
    dats: Arc<DatManager>,
    content: Arc<PackContent>,
}

fn verify_hash(path: &Path, expected: &str) {
    assert_eq!(expected.len(), 64, "SHA-256 must contain 64 hex digits");
    assert!(expected.bytes().all(|b| b.is_ascii_hexdigit()));
    let mut input = BufReader::new(File::open(path).expect("fixture file"));
    let mut hash = Sha256::new();
    let mut buf = [0; 64 * 1024];
    loop {
        let n = input.read(&mut buf).expect("read fixture");
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    assert_eq!(
        format!("{:x}", hash.finalize()),
        expected.to_ascii_lowercase(),
        "fixture hash mismatch: {}",
        path.display()
    );
}

fn fixture() -> &'static Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let path = std::env::var_os("EMPYREAN_TEST_LAUNCH_FIXTURE")
            .expect("set EMPYREAN_TEST_LAUNCH_FIXTURE to the hash-pinned fixture manifest");
        let manifest: Manifest =
            serde_json::from_reader(File::open(path).expect("Launch fixture manifest"))
                .expect("Launch fixture manifest schema");
        assert_eq!(
            manifest.capture, "launch",
            "this suite requires the launch dats"
        );
        verify_hash(
            &manifest.dat_directory.join("portal.dat"),
            &manifest.portal_sha256,
        );
        verify_hash(
            &manifest.dat_directory.join("cell.dat"),
            &manifest.cell_sha256,
        );
        verify_hash(&manifest.world_pack, &manifest.world_pack_sha256);
        let source = RealDats::open_era(&manifest.dat_directory, ContainerEra::Classic)
            .expect("classic DAT pair");
        assert_eq!(source.header_iteration(DatDatabaseType::Portal), Some(105));
        assert_eq!(source.header_iteration(DatDatabaseType::Cell), Some(67));
        let dats = DatManager::initialize(Arc::new(source)).expect("real DAT manager");
        let content = Arc::new(PackContent::open(&manifest.world_pack).expect("candidate pack"));
        assert_eq!(
            content.era(),
            EraId::Launch,
            "never run this fixture with a later pack"
        );
        Fixture { dats, content }
    })
}

fn world() -> World {
    ThreadSafeRandom::seed(0x1999_1009);
    let f = fixture();
    let now = ClockSnapshot {
        portal_year_ticks: 1000.0,
        unix_time: 1_767_225_600.0,
        utc: DotNetDateTime::new(2026, 1, 1),
        monotonic: Duration::ZERO,
    };
    let mut w = World::new(now, Arc::clone(&f.dats));
    w.content = f.content.clone();
    w.era = EraId::Launch.rules();
    assert_eq!(w.content.era(), w.era.id);
    guid_manager::initialize(&mut w, &mut EmptyShard);
    event_manager::initialize(&mut w);
    let account = w
        .auth
        .lock()
        .create_account(
            "offline-fixture",
            "offline-fixture",
            empyrean_entity::enums::AccessLevel::Player,
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        )
        .expect("in-memory fixture account");
    assert_eq!(account.account_id, 1);
    empyrean_world::world_objects::world_object_magic::clear_spell_cache();
    w
}

fn request(w: &World, heritage: u32, sex: u32, template: usize, area: u32) -> CharacterCreateInfo {
    let group = &w.dats.portal_dat().char_gen().heritage_groups[&heritage];
    let model = &group.templates[template];
    let mut skills = vec![SkillAdvancementClass::Inactive; 55];
    for skill in &model.normal_skills {
        skills[usize::try_from(*skill).unwrap()] = SkillAdvancementClass::Trained;
    }
    for skill in &model.primary_skills {
        skills[usize::try_from(*skill).unwrap()] = SkillAdvancementClass::Specialized;
    }
    let color = group.sexes[&sex].clothing_colors[0];
    CharacterCreateInfo {
        heritage: HeritageGroup(i32::try_from(heritage).unwrap()),
        gender: sex,
        appearance: empyrean_entity::appearance::Appearance {
            headgear_color: color,
            shirt_color: color,
            pants_color: color,
            footwear_color: color,
            ..Default::default()
        },
        template_option: i32::try_from(template).unwrap(),
        strength_ability: model.attributes[0],
        endurance_ability: model.attributes[1],
        coordination_ability: model.attributes[2],
        quickness_ability: model.attributes[3],
        focus_ability: model.attributes[4],
        self_ability: model.attributes[5],
        skill_advancement_classes: skills,
        name: Some("Offline test".to_owned()),
        start_area: area,
        ..Default::default()
    }
}

fn create(w: &mut World, info: &CharacterCreateInfo) -> CreatedPlayer {
    let human = w
        .content
        .get_cached_weenie_by_class_name("human")
        .expect("human template");
    let guid = guid_manager::new_player_guid(w);
    let (result, player) = player_factory::create(w, info, human, guid, 1, WeenieType::Creature);
    assert_eq!(
        result,
        CreateResult::Success,
        "heritage {:?}, sex {}, template {}, area {}",
        info.heritage,
        info.gender,
        info.template_option,
        info.start_area
    );
    player
}

fn load_start(w: &mut World, area: usize) -> LandblockId {
    let cell = w.dats.portal_dat().char_gen().starter_areas[area].locations[0]
        .cell
        .0;
    let id = landblock_manager::get_landblock(w, LandblockId::new(cell), false, false);
    // The normal load enqueues construction. Drain without starting a world loop or a host.
    for _ in 0..16 {
        if action_queue_mut(w, Actor::Landblock(id))
            .unwrap()
            .is_empty()
        {
            break;
        }
        run_actions(w, Actor::Landblock(id));
    }
    let queue = action_queue_mut(w, Actor::Landblock(id)).unwrap();
    assert_eq!(
        queue.act_exceptions(),
        0,
        "caught exception loading area {area}"
    );
    assert!(queue.is_empty(), "unbounded load queue for area {area}");
    assert!(w
        .landblock_manager
        .landblocks
        .expect(id)
        .create_world_objects_completed());
    assert!(w
        .physics
        .land()
        .landblock(PhysicsLandblockId(id.landblock()))
        .is_some());
    id
}

#[test]
fn the_launch_dats_decode_the_required_launch_tables() {
    let w = world();
    let portal = w.dats.portal_dat();
    assert_eq!(portal.char_gen().starter_areas.len(), 18);
    assert_eq!(portal.char_gen().heritage_groups.len(), 3);
    assert_eq!(portal.spell_table().spells.len(), 1635);
    // Required accessors fail if a present record did not decode.
    let _ = (
        portal.region_desc(),
        portal.secondary_attribute_table(),
        portal.skill_table(),
        portal.spell_components_table(),
        portal.xp_table(),
    );
    assert_eq!(w.era.id, EraId::Launch);
}

/// Divergence: V493
/// The 1999 curves put the creation bonus where its experience says: rank 5 trained is 526, rank 10
/// specialized is 671, and the next ranks cost 178 and 124.
#[test]
fn the_launch_curves_price_the_creation_bonus() {
    let w = world();
    let xp = w.dats.portal_dat().xp_table();
    assert_eq!((xp.trained_xp[5], xp.specialized_xp[10]), (526, 671));
    assert_eq!(xp.trained_xp[6] - xp.trained_xp[5], 178);
    assert_eq!(xp.specialized_xp[11] - xp.specialized_xp[10], 124);
}

#[test]
fn every_launch_heritage_sex_and_template_creates_at_its_selected_start() {
    let mut w = world();
    let cg = w.dats.portal_dat().char_gen().clone();
    let mut count = 0;
    for (&heritage, group) in &cg.heritage_groups {
        assert_eq!(group.templates.len(), 8);
        assert_eq!(group.sexes.len(), 2);
        for &sex in group.sexes.keys() {
            for template in 0..group.templates.len() {
                let area = group.primary_start_areas[template % group.primary_start_areas.len()];
                let info = request(&w, heritage, sex, template, area);
                let p = create(&mut w, &info);
                let expected = cg.starter_areas[usize::try_from(area).unwrap()].locations[0];
                let actual = p
                    .player
                    .get_position(PositionType::Location)
                    .expect("created location");
                assert_eq!(actual.cell(), expected.cell.0);
                assert!((actual.position_x - expected.frame.origin.x).abs() < 0.001);
                assert!((actual.position_y - expected.frame.origin.y).abs() < 0.001);
                assert!((actual.position_z - expected.frame.origin.z).abs() < 0.001);
                let instantiation = p.player.get_position(PositionType::Instantiation).unwrap();
                assert_eq!(instantiation.cell(), actual.cell());
                assert_eq!(instantiation.pos(), actual.pos());
                assert_eq!(
                    p.player.get_property(PropertyInt::AvailableSkillCredits),
                    Some(0)
                );
                assert_eq!(
                    p.equipped_objects.len(),
                    4,
                    "appearance clothing must resolve"
                );
                starters::assert_kit(&w, &p, &info);
                count += 1;
            }
        }
    }
    assert_eq!(count, 48);
}

#[test]
fn every_launch_start_loads_real_terrain_and_its_vendors_complete_stock() {
    let mut placement_failures = Vec::new();
    for area in 0..18 {
        let mut w = world();
        let id = load_start(&mut w, area);
        let entries = w.content.get_cached_instances_by_landblock(id.landblock());
        let vendors: Vec<_> = entries
            .iter()
            .filter(|i| {
                w.content
                    .get_cached_weenie(i.weenie_class_id)
                    .is_some_and(|o| o.weenie_type == WeenieType::Vendor)
            })
            .collect();
        assert_eq!(vendors.len(), 1, "one outpost vendor in area {area}");
        for entry in vendors {
            let guid = ObjectGuid::new(entry.guid);
            let object = w.objects.get(guid).expect("placed vendor constructed");
            if object.current_landblock != Some(id) {
                placement_failures.push(format!(
                    "area {area} ({}), vendor {}, guid {:08X}, stored cell {:08X}, position {:?}, decoded cell {}, collision cell {}",
                    w.dats.portal_dat().char_gen().starter_areas[area].name,
                    entry.weenie_class_id,
                    entry.guid,
                    entry.obj_cell_id,
                    object.location(),
                    w.dats.cell_dat().read_from_dat::<dereth_assets::EnvCell>(entry.obj_cell_id).is_some(),
                    w.physics.land().env_cell(dereth_primitives::CellId(entry.obj_cell_id)).is_some(),
                ));
            }
            let expected: BTreeSet<_> = object
                .biota
                .properties_create_list
                .iter()
                .flat_map(|rows| rows.iter())
                .filter(|r| r.destination_type == DestinationType::Shop)
                .map(|r| r.weenie_class_id)
                .collect();
            assert!(!expected.is_empty(), "vendor has stock");
            vendor::load_inventory(&mut w, guid);
            let actual: BTreeSet<_> = vendor::default_items_for_sale(&w, guid)
                .iter()
                .map(|g| w.objects.get(*g).expect("sale item").biota.weenie_class_id)
                .collect();
            assert_eq!(actual, expected, "stock construction for area {area}");
        }
    }
    assert!(
        placement_failures.is_empty(),
        "vendors failed to join terrain:\n{}",
        placement_failures.join("\n")
    );
}

#[test]
fn a_created_launch_player_moves_on_the_actual_start_terrain() {
    let mut w = world();
    let info = request(&w, 1, 1, 0, 0);
    let created = create(&mut w, &info);
    let guid = created.player.guid;
    let id = load_start(&mut w, 0);
    for item in created
        .inventory
        .into_iter()
        .chain(created.equipped_objects)
    {
        w.objects.insert(item).expect("created possession");
    }
    w.objects.insert(created.player).expect("created player");
    let session = SessionId {
        client_id: 1,
        generation: 1,
    };
    w.sessions.insert(
        session,
        SessionData {
            state: SessionState::WorldConnected,
            player: Some(guid),
            ..Default::default()
        },
    );
    assert!(landblock_manager::add_object(&mut w, guid, false));
    player_location::on_teleport_complete(&mut w, guid);
    let body = phys_ext::physics_obj(&w, guid).expect("real player physics body");
    let before = w.objects.get(guid).unwrap().location().unwrap();
    assert!(position_extensions::is_walkable(&w, &before));
    for step in 1..=4 {
        w.now.utc = w.now.utc.add_seconds(0.5);
        w.now.unix_time += 0.5;
        w.now.monotonic += Duration::from_millis(500);
        let mut target = before;
        target.position_x += step as f32 * 0.25;
        target.position_z = landblock::physics_landblock_get_z(&w, id, target.pos()) + 0.005;
        player_tick::update_player_position(&mut w, guid, target, false);
        let actual = w.objects.get(guid).unwrap().location().unwrap();
        let physical = phys_ext::position(&w, body).expect("resident body");
        assert!(
            (actual.position_x - target.position_x).abs() < 0.01,
            "movement step {step}"
        );
        assert!((physical.frame.origin.x - actual.position_x).abs() < 0.01);
        assert!((physical.frame.origin.z - target.position_z).abs() < 0.1);
        assert_eq!(physical.cell.0, actual.cell());
        assert_eq!(w.objects.get(guid).unwrap().current_landblock, Some(id));
    }
    assert!(w.objects.get(guid).unwrap().location().unwrap().position_x > before.position_x + 0.9);
}

#[test]
fn the_yanshi_outpost_instances_name_the_classic_rooms_containing_their_locations() {
    use dereth_physics::cell::CellResolver;
    use dereth_primitives::{CellId, Vec3};
    let mut w = world();
    let id = load_start(&mut w, 8);
    let entries = w.content.get_cached_instances_by_landblock(id.landblock());
    let resolver = CellResolver::new(w.physics.land());
    let mut mismatches = Vec::new();
    for (guid, wcid) in [(0x7B973000, 1154), (0x7B973001, 5064), (0x7B973002, 4565)] {
        let entry = entries
            .iter()
            .find(|e| e.guid == guid)
            .expect("outpost instance retained");
        assert_eq!(entry.weenie_class_id, wcid);
        let point = Vec3::new(entry.origin_x, entry.origin_y, entry.origin_z);
        let containing: Vec<_> = (0xB9730100..=0xB973010F)
            .filter(|&cell| {
                resolver
                    .get_visible(CellId(cell))
                    .is_some_and(|c| c.point_in_cell(point))
            })
            .collect();
        assert_eq!(
            containing.len(),
            1,
            "unique classic room for {guid:08X}: {containing:X?}"
        );
        if containing[0] != entry.obj_cell_id {
            mismatches.push(format!(
                "{guid:08X}: stored {:08X}, containing {:08X}",
                entry.obj_cell_id, containing[0]
            ));
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}
