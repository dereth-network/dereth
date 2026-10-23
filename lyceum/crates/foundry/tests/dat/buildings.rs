//! Behaviour: none (tooling: retail buildings in a world over the retail files: the building catalog, copies dropped with their rooms, doors and ground, retail houses dropped as houses players can buy, the ground of the buildings around a drop, and the checks of one a pack moves)

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::sync::OnceLock;

use dereth_assets::world::{BuildInfo, EnvCell, Scene};
use dereth_assets::{Decode, Environment};
use dereth_primitives::frame::localtoglobal;
use dereth_primitives::{DataId, Frame, Quat};
use lyceum_foundry::buildings::{self, Catalog, Kind, Site};
use lyceum_foundry::cells::carried;
use lyceum_foundry::compile;
use lyceum_foundry::dats::DatSet;
use lyceum_foundry::drops::{self, Template};
use lyceum_foundry::edit::{Fault, Session};
use lyceum_foundry::source::{Furnished, Rotation};
use lyceum_foundry::world::World;

/// The most placed cottage, the most placed villa, and the lead of the mansion's set of four.
const COTTAGE: u32 = 0x0100_2228;
const VILLA: u32 = 0x0100_22CE;
const MANSION: u32 = 0x0100_2286;

/// The door class the tests' world database stands at the copies the catalog takes.
const DOOR: u32 = 90_412;

/// The one landblock the tests call a town.
fn town(lb: u16) -> Option<String> {
    (lb == 0x3B9C).then(|| "Testville".to_owned())
}

fn files() -> &'static DatSet {
    static SET: OnceLock<DatSet> = OnceLock::new();
    SET.get_or_init(|| {
        dereth_dat::testing::require_dats();
        DatSet::open(&dereth_dat::testing::dat_dir()).unwrap()
    })
}

/// The catalog without the server's world: no doors.
fn bare_catalog() -> &'static Catalog {
    static C: OnceLock<Catalog> = OnceLock::new();
    C.get_or_init(|| Catalog::read(files(), None, &town))
}

fn region() -> dereth_assets::Region {
    let id = compile::REGION;
    dereth_assets::Region::decode_payload(id, &files().read_portal(id).unwrap()).unwrap()
}

fn read(id: u32) -> Option<Vec<u8>> {
    files().cell.read(DataId(id)).ok()
}

fn environment(id: DataId) -> Option<Environment> {
    Environment::decode_payload(id, &files().read_portal(id)?).ok()
}

/// The sites a drop of `model` copies: its first layout's example, and its set's partners there.
fn sites(c: &Catalog, model: u32) -> Vec<Site> {
    let e = c.get(DataId(model)).unwrap();
    let lead = e.layouts[0].example;
    let t = Template::read(&read, lead, None, dereth_primitives::EraId::Eor).unwrap();
    let info = dereth_assets::world::LandblockInfo::decode_payload(
        DataId((u32::from(lead.landblock) << 16) | 0xFFFE),
        &read((u32::from(lead.landblock) << 16) | 0xFFFE).unwrap(),
    )
    .unwrap();
    let mut out = vec![lead];
    for p in &e.set {
        let want = dereth_primitives::frame::combine(&t.building.frame, &p.frame);
        let index = info
            .buildings
            .iter()
            .position(|b| {
                b.id == p.model
                    && (b.frame.origin.x - want.origin.x).abs() < 0.5
                    && (b.frame.origin.y - want.origin.y).abs() < 0.5
            })
            .unwrap();
        out.push(Site {
            landblock: lead.landblock,
            index,
        });
    }
    out
}

/// The doors the tests' world database stands at the building at `site`: one at the foot of
/// each doorway, in the land cell outside, and one in its lowest room, as the server's world
/// stands doors. Each is its guid, its cell and its frame (landblock meters).
fn site_doors(site: Site) -> Vec<(u32, u32, Frame)> {
    let t = Template::read(&read, site, None, dereth_primitives::EraId::Eor).unwrap();
    let mut env = environment;
    let first = u32::from(site.landblock) << 16;
    let mut out: Vec<(u32, Frame)> = buildings::doorway_feet(&t.building, &t.rooms, &mut env)
        .into_iter()
        .map(|f| {
            (
                first | lyceum_foundry::edit::outdoor_cell(f.x, f.y),
                Frame::new(f, t.building.frame.rotation),
            )
        })
        .collect();
    let (n, room) = t.rooms.iter().next().unwrap();
    out.push((
        first | u32::from(*n),
        Frame::new(room.frame.origin, t.building.frame.rotation),
    ));
    out.into_iter()
        .enumerate()
        .map(|(k, (cell, f))| {
            let guid = 0x7000_0000
                | (u32::from(site.landblock) << 12)
                | (0x800 + u32::try_from(site.index * 16 + k).unwrap());
            (guid, cell, f)
        })
        .collect()
}

/// A world database with a door class, standing [`site_doors`] at every building of `models`
/// and of their sets in the landblocks of the catalog's examples of them.
fn doors_sql(models: &[u32]) -> String {
    let mut sql = format!(
        "INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
         VALUES ({DOOR}, 'testdoor', 19, '2005-02-09 10:00:00');
"
    );
    for m in models {
        let e = bare_catalog().get(DataId(*m)).unwrap();
        let lb = e.layouts[0].example.landblock;
        let wanted: BTreeSet<DataId> = e.set.iter().map(|p| p.model).chain([e.model]).collect();
        let id = (u32::from(lb) << 16) | 0xFFFE;
        let info =
            dereth_assets::world::LandblockInfo::decode_payload(DataId(id), &read(id).unwrap())
                .unwrap();
        for (index, b) in info.buildings.iter().enumerate() {
            if !wanted.contains(&b.id) {
                continue;
            }
            for (guid, cell, f) in site_doors(Site {
                landblock: lb,
                index,
            }) {
                let (o, q) = (f.origin, f.rotation);
                let _ = writeln!(
                    sql,
                    "INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`,                      `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`,                      `angles_Z`, `is_Link_Child`, `last_Modified`)
                     VALUES ({guid:#010X}, {DOOR}, {cell:#010X}, {}, {}, {}, {}, {}, {}, {},                      False, '2005-02-09 10:00:00');",
                    o.x, o.y, o.z, q.w, q.x, q.y, q.z
                );
            }
        }
    }
    sql
}

/// The sample world open for editing, with the catalog and the world database `sql`.
fn session(tag: &str, sql: &str) -> (dereth_dat::testing::ScratchDir, Session) {
    let (dir, world) = crate::sample::sample(tag);
    let mut s = Session::open(&world, &dereth_dat::testing::dat_dir(), None).unwrap();
    s.set_today("2026-10-22 00:00:00");
    let content = lyceum_foundry::sql_ids::server_content(sql);
    s.learn_buildings(Catalog::read(s.base(), Some(&content), &town));
    s.learn_server(content);
    (dir, s)
}

fn same_frame(a: &Frame, b: &Frame) -> bool {
    let (p, q) = (a.origin, b.origin);
    let close = |x: f32, y: f32| (x - y).abs() < 2e-3;
    let turn = |s: f32| {
        let (r, t) = (a.rotation, b.rotation);
        (r.w - s * t.w).abs() < 1e-4
            && (r.x - s * t.x).abs() < 1e-4
            && (r.y - s * t.y).abs() < 1e-4
            && (r.z - s * t.z).abs() < 1e-4
    };
    close(p.x, q.x) && close(p.y, q.y) && close(p.z, q.z) && (turn(1.0) || turn(-1.0))
}

/// The copy `b` (with the compiled cells `cells`) is template `t` moved to `b`'s frame: the same
/// model, `num_leaves` and doorways; each room the template's piece, surfaces and ways, carried,
/// its list and the doorways' lists renumbered, furnished as `furniture` is (or bare), and no
/// house's.
fn assert_copy(
    t: &Template,
    b: &BuildInfo,
    cells: &BTreeMap<u16, EnvCell>,
    furniture: Option<&Template>,
) {
    assert_eq!(b.id, t.building.id);
    assert_eq!(b.num_leaves, t.building.num_leaves, "{}", t.site);
    let rooms = buildings::rooms(b, cells);
    assert_eq!(rooms.len(), t.rooms.len(), "{}", t.site);
    let number: BTreeMap<u16, u16> = t.rooms.keys().copied().zip(rooms.iter().copied()).collect();
    let renumber = |list: &[u16]| -> Vec<u16> { list.iter().map(|c| number[c]).collect() };
    assert_eq!(b.portals.len(), t.building.portals.len());
    for (p, q) in b.portals.iter().zip(&t.building.portals) {
        assert_eq!(
            (p.flags, p.other_cell_id, p.other_portal_id, &p.stab_list),
            (
                q.flags,
                number[&q.other_cell_id],
                q.other_portal_id,
                &renumber(&q.stab_list)
            ),
            "{} doorway",
            t.site
        );
    }
    let furnished: BTreeMap<u16, Vec<_>> = match furniture {
        None => BTreeMap::new(),
        // Each of the furniture's rooms is the template's room in the same place in its building.
        Some(f) => f
            .rooms
            .values()
            .map(|c| {
                let at = carried(&c.frame, &f.building.frame, &t.building.frame);
                let (n, _) = t
                    .rooms
                    .iter()
                    .find(|(_, r)| {
                        (r.environment, r.cell_struct) == (c.environment, c.cell_struct)
                            && same_frame(&r.frame, &at)
                    })
                    .unwrap();
                (
                    *n,
                    c.static_objects
                        .iter()
                        .map(|s| (s.id, carried(&s.frame, &f.building.frame, &b.frame)))
                        .collect(),
                )
            })
            .collect(),
    };
    for (n, r) in &t.rooms {
        let c = &cells[&number[n]];
        assert_eq!(
            (c.environment, c.cell_struct, &c.surfaces),
            (r.environment, r.cell_struct, &r.surfaces),
            "{} room {n:04X}",
            t.site
        );
        assert!(
            same_frame(&c.frame, &carried(&r.frame, &t.building.frame, &b.frame)),
            "{} room {n:04X}: {:?}",
            t.site,
            c.frame
        );
        let ways: Vec<_> = r
            .portals
            .iter()
            .map(|p| {
                let to = if p.flags & 4 != 0 {
                    p.other_cell_id
                } else {
                    u32::from(number[&u16::try_from(p.other_cell_id).unwrap()])
                };
                (p.flags, p.polygon_id, to, p.other_portal_id)
            })
            .collect();
        let got: Vec<_> = c
            .portals
            .iter()
            .map(|p| (p.flags, p.polygon_id, p.other_cell_id, p.other_portal_id))
            .collect();
        assert_eq!(got, ways, "{} room {n:04X}", t.site);
        assert_eq!(
            c.visible_cells,
            renumber(&r.visible_cells),
            "{} room {n:04X}",
            t.site
        );
        assert_eq!(c.restriction_obj, None);
        assert_eq!(c.flags & 8, 0);
        let want = furnished.get(n).cloned().unwrap_or_default();
        assert_eq!(
            c.static_objects.len(),
            want.len(),
            "{} room {n:04X}",
            t.site
        );
        for (s, (id, f)) in c.static_objects.iter().zip(&want) {
            assert_eq!(s.id, *id);
            assert!(same_frame(&s.frame, f), "{} room {n:04X}", t.site);
        }
    }
}

/// The scenery landblock `lb` grows (on its terrain as the session compiled it) inside a shell
/// of its buildings: each object's model and the building's.
fn scenery_under_shells(s: &Session, lb: u16, region: &dereth_assets::Region) -> Vec<(u32, u32)> {
    let Some(info) = s.statics(lb) else {
        return Vec::new();
    };
    let land = s.terrain(lb).unwrap();
    let mesh = dereth_terrain::land::mesh::generate_landblock(
        &land,
        region,
        i32::from(lb >> 8),
        i32::from(lb & 0xFF),
        1,
        dereth_terrain::land::mesh::Direction::InViewerBlock,
    );
    let owns: BTreeSet<u16> = info
        .buildings
        .iter()
        .map(|b| dereth_terrain::scenery::outside_cell_index(b.frame.origin.x, b.frame.origin.y))
        .collect();
    let scenes = |id: DataId| Scene::decode_payload(id, &s.base().read_portal(id)?).ok();
    let grown = dereth_terrain::scenery::generate_scenery(
        &land,
        &mesh,
        region,
        i32::from(lb >> 8),
        i32::from(lb & 0xFF),
        &dereth_terrain::scenery::SceneryEnv {
            scenes: &scenes,
            has_building: &|c| owns.contains(&c),
            shape: &|_| Some(dereth_terrain::scenery::WithinBlockShape::default()),
        },
    );
    let mut models = lyceum_foundry::pick::Models::default();
    let mut out = Vec::new();
    for b in &info.buildings {
        let (lo, hi) = extent(&mut models, s.base(), b.id);
        for g in &grown {
            let q = dereth_primitives::frame::globaltolocal(&b.frame, g.frame.origin);
            if q.x > lo[0] + 0.5 && q.x < hi[0] - 0.5 && q.y > lo[1] + 0.5 && q.y < hi[1] - 0.5 {
                out.push((g.gfxobj.raw(), b.id.raw()));
            }
        }
    }
    out
}

/// Model `id`'s extent seen from above, in its own frame: its lowest and highest x and y.
fn extent(
    models: &mut lyceum_foundry::pick::Models,
    set: &DatSet,
    id: DataId,
) -> ([f32; 2], [f32; 2]) {
    let m = models.get(set, id).unwrap();
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for p in m.triangles.iter().flatten() {
        lo = [lo[0].min(p.x), lo[1].min(p.y)];
        hi = [hi[0].max(p.x), hi[1].max(p.y)];
    }
    (lo, hi)
}

#[test]
fn a_drop_clears_the_scenery_its_shell_would_stand_over_in_the_next_land_cell() {
    let (_dir, mut s) = session("buildings-scenery", "");
    let region = region();
    let lb = 0x9C9Cu16;
    let (lo, hi) = extent(
        &mut lyceum_foundry::pick::Models::default(),
        s.base(),
        DataId(COTTAGE),
    );
    // A tree or a rock the landblock grows near the west edge of its land cell: the cottage,
    // unturned, stands with its origin in the land cell to the west and its shell over it.
    let land = s.terrain(lb).unwrap();
    let mesh = dereth_terrain::land::mesh::generate_landblock(
        &land,
        &region,
        i32::from(lb >> 8),
        i32::from(lb & 0xFF),
        1,
        dereth_terrain::land::mesh::Direction::InViewerBlock,
    );
    let scenes = |id: DataId| Scene::decode_payload(id, &s.base().read_portal(id)?).ok();
    let grown = dereth_terrain::scenery::generate_scenery(
        &land,
        &mesh,
        &region,
        i32::from(lb >> 8),
        i32::from(lb & 0xFF),
        &dereth_terrain::scenery::SceneryEnv {
            scenes: &scenes,
            has_building: &|_| false,
            shape: &|_| Some(dereth_terrain::scenery::WithinBlockShape::default()),
        },
    );
    let into = hi[0] - 1.0;
    let (x, y) = grown
        .iter()
        .map(|g| g.frame.origin)
        .find_map(|p| {
            let (x, y) = (p.x - into, p.y - (lo[1] + hi[1]) / 2.0);
            let ok = p.x.rem_euclid(24.0) < into - 0.5
                && x > 24.0
                && (1.0..191.0).contains(&y)
                && buildings::land_cell(x, y) != buildings::land_cell(p.x, p.y);
            ok.then_some((x, y))
        })
        .expect("scenery near a land cell's west edge");
    let (op, _) = s
        .drop_building(DataId(COTTAGE), 0, &Furnished::Template, lb, x, y, 0.0)
        .unwrap();
    let lyceum_foundry::edit::Op::Batch(ops) = &op else {
        panic!("{op:?}")
    };
    let cleared = ops.iter().any(|o| {
        matches!(o, lyceum_foundry::edit::Op::SetVertices(v)
            if v.iter().any(|v| v.field == compile::Field::Scene))
    });
    assert!(cleared, "the drop clears a vertex's scenery");
    s.apply(&op).unwrap();
    let under = scenery_under_shells(&s, lb, &region);
    assert!(under.is_empty(), "{under:?}");
}

#[test]
fn a_drop_stands_as_high_over_the_ground_as_its_template() {
    let (_dir, mut s) = session("buildings-height", "");
    let region = region();
    // The most placed building with no set whose example stands a meter or more over its ground.
    let (model, height) = s
        .buildings()
        .unwrap()
        .gallery()
        .into_iter()
        .filter(|e| e.set.is_empty())
        .find_map(|e| {
            let t = Template::read(
                &read,
                e.layouts[0].example,
                Some(&region),
                dereth_primitives::EraId::Eor,
            )
            .ok()?;
            (t.height().abs() >= 1.0).then_some((e.model, t.height()))
        })
        .unwrap();
    let (op, _) = s
        .drop_building(model, 0, &Furnished::Template, 0xA0A8, 84.0, 84.0, 0.0)
        .unwrap();
    s.apply(&op).unwrap();
    let b = &s.statics(0xA0A8).unwrap().buildings[0];
    let ground = s
        .ground(0xA0A8, b.frame.origin.x, b.frame.origin.y)
        .unwrap();
    assert!(
        (b.frame.origin.z - ground - height).abs() < 2e-3,
        "{:#010X}: origin {} over ground {ground}, template's height {height}",
        model.raw(),
        b.frame.origin.z
    );
}

#[test]
fn the_catalog_holds_every_building_model_with_its_layouts_kinds_and_sets() {
    let c = bare_catalog();
    assert_eq!(c.len(), 398);
    assert_eq!(c.entries().map(|e| e.count).sum::<u32>(), 6979);
    let kinds = |k: Kind| c.entries().filter(|e| e.kind == k).count();
    assert_eq!(
        [Kind::Cottage, Kind::Villa, Kind::Mansion].map(kinds),
        [10, 2, 4]
    );
    assert_eq!(kinds(Kind::Shell), 193);
    // One interior for nearly every model; three have a second with other surfaces.
    let multi: Vec<&buildings::Entry> = c
        .entries()
        .filter(|e| e.layouts.len() > 1 && e.kind != Kind::Entrance)
        .collect();
    assert!(multi.len() <= 4, "{}", multi.len());
    let cottage = c.get(DataId(COTTAGE)).unwrap();
    assert_eq!(
        (
            cottage.count,
            cottage.kind,
            cottage.num_leaves,
            cottage.layouts.len()
        ),
        (445, Kind::Cottage, 200, 1)
    );
    let l = &cottage.layouts[0];
    assert_eq!(
        (l.example, l.rooms, l.doorways),
        (Site::parse("3B9C/3").unwrap(), 7, 2)
    );
    // Its furnishings, the most common first, each named by a town's copy when one has it.
    assert!(l.furnishings[0].count >= 400);
    assert_eq!(l.furnishings.iter().map(|f| f.count).sum::<u32>(), l.count);
    assert_eq!(l.furnishings[0].town.as_deref(), Some("Testville"));
    // The mansion's four stand together: each names the other three, and the lead drops them.
    let mansion = c.get(DataId(MANSION)).unwrap();
    assert_eq!((mansion.kind, mansion.set.len()), (Kind::Mansion, 3));
    assert!(mansion.is_lead());
    for p in &mansion.set {
        let other = c.get(p.model).unwrap();
        assert_eq!((other.kind, other.set.len()), (Kind::Mansion, 3));
        assert!(!other.is_lead());
    }
    let gallery = c.gallery();
    assert_eq!(gallery[0].model, DataId(COTTAGE));
    assert!(gallery.iter().all(|e| e.is_lead()));
    assert_eq!(
        gallery.iter().filter(|e| e.kind == Kind::Mansion).count(),
        1
    );
    // Without the world database, no doors.
    assert!(!c.doors_known);
    assert!(c
        .entries()
        .all(|e| e.layouts.iter().all(|l| l.doors.is_empty())));
}

#[test]
fn the_catalog_kept_in_a_file_reads_back_as_it_was() {
    let sql = doors_sql(&[COTTAGE]);
    let c = Catalog::read(
        files(),
        Some(&lyceum_foundry::sql_ids::server_content(&sql)),
        &town,
    );
    assert!(c.doors_known);
    let text = c.to_text();
    assert_eq!(Catalog::from_text(&text).as_ref(), Some(&c));
    let dir = dereth_dat::testing::ScratchDir::new("buildings-catalog-file").unwrap();
    let file = dir.path().join("cache").join("buildings.tsv");
    c.save(&file).unwrap();
    assert_eq!(Catalog::load(&file).as_ref(), Some(&c));
    assert_eq!(
        Catalog::from_text(&text.replacen("catalog 2", "catalog 0", 1)),
        None
    );
}

/// Where each drop goes: the landblock (none has a statics record), the lead's origin, its
/// heading.
const DROPS: [(u32, u16, f32, f32, f32); 3] = [
    (COTTAGE, 0x9C9C, 61.31, 79.8, 180.0),
    (VILLA, 0xA0A8, 84.0, 84.0, 90.0),
    (MANSION, 0xA9AD, 132.0, 108.0, 0.0),
];

#[test]
fn a_dropped_cottage_villa_and_mansion_are_their_retail_copies_with_doors_and_level_ground() {
    let models: Vec<u32> = DROPS.iter().map(|(m, ..)| *m).collect();
    let (dir, mut s) = session("buildings-drop", &doors_sql(&models));
    let before = s.compiled().records.clone();
    let region = region();
    let table = dereth_terrain::land::mesh::height_table(&region);
    for (model, lb, x, y, heading) in DROPS {
        assert!(
            s.statics(lb).is_none(),
            "{lb:04X} starts with no statics record"
        );
        let (op, _) = s
            .drop_building(DataId(model), 0, &Furnished::Template, lb, x, y, heading)
            .unwrap();
        s.apply(&op).unwrap();
    }
    for (model, lb, x, y, heading) in DROPS {
        let catalog = s.buildings().unwrap();
        let entry = catalog.get(DataId(model)).unwrap();
        let templates: Vec<Template> = sites(catalog, model)
            .into_iter()
            .map(|site| {
                Template::read(&read, site, Some(&region), dereth_primitives::EraId::Eor).unwrap()
            })
            .collect();
        let info = s.statics(lb).unwrap();
        let cells = s.cells(lb);
        // The landblock's terrain says it has a statics record now, and its count of cells is
        // its rooms.
        assert_eq!(s.terrain(lb).unwrap().lbi_exists, 1);
        let rooms: usize = templates.iter().map(|t| t.rooms.len()).sum();
        assert_eq!((info.num_cells as usize, cells.len()), (rooms, rooms));
        assert_eq!(info.buildings.len(), templates.len());
        // Its origin stands on the level ground, as high over it as the template's does.
        let b0 = &info.buildings[0];
        let level = s.ground(lb, b0.frame.origin.x, b0.frame.origin.y).unwrap();
        assert!(
            (b0.frame.origin.z - level - templates[0].height()).abs() < 2e-3,
            "{lb:04X}: origin {} over ground {level}, template's height {}",
            b0.frame.origin.z,
            templates[0].height()
        );
        assert!(table.iter().any(|h| (h - level).abs() < 1e-3));
        assert!(same_frame(
            &Frame::new(
                dereth_primitives::Vec3::new(x, y, b0.frame.origin.z),
                compile::heading_quat(heading)
            ),
            &b0.frame
        ));
        // Every building of the set is its template, carried by the one move.
        for (t, b) in templates.iter().zip(&info.buildings) {
            assert!(same_frame(
                &b.frame,
                &carried(&t.building.frame, &templates[0].building.frame, &b0.frame)
            ));
            assert_copy(t, b, &cells, Some(t));
        }
        // The ground under every shell is level: each corner of each land cell under it.
        let mut models = lyceum_foundry::pick::Models::default();
        for b in &info.buildings {
            let m = models.get(s.base(), b.id).unwrap();
            for p in m.triangles.iter().flatten() {
                let g = localtoglobal(&b.frame, *p);
                let (cx, cy) = buildings::land_cell(g.x, g.y);
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let gx = u32::try_from(i32::from(lb >> 8) * 8 + cx + dx).unwrap();
                    let gy = u32::try_from(i32::from(lb & 0xFF) * 8 + cy + dy).unwrap();
                    assert!(
                        (s.vertex(gx, gy).unwrap().z - level).abs() < 1e-3,
                        "{lb:04X}: vertex {gx},{gy}"
                    );
                }
            }
        }
        // No scenery stands under a shell.
        let under = scenery_under_shells(&s, lb, &region);
        assert!(under.is_empty(), "{lb:04X}: {under:?}");
        // Its doors: each of the template's, carried, in the copied room it stands in or the
        // land cell outside.
        let doors: Vec<_> = s
            .compiled()
            .sql
            .iter()
            .filter(|(p, _)| {
                let p = p.to_string_lossy();
                p.contains(&format!("{lb:04X} ")) && p.contains("-door-")
            })
            .flat_map(|(_, text)| lyceum_foundry::sql_ids::instances(text))
            .collect();
        assert_eq!(
            doors.len(),
            entry.layouts[0].doors.len(),
            "{lb:04X}: {:?}",
            s.compiled().sql.keys().collect::<Vec<_>>()
        );
        // The template's doors, each once (a door between two of a set's buildings stands at
        // the doorways of both).
        let mut want = BTreeMap::new();
        for (t, b) in templates.iter().zip(&info.buildings) {
            let number: BTreeMap<u16, u16> = t
                .rooms
                .keys()
                .copied()
                .zip(buildings::rooms(b, &cells))
                .collect();
            for (guid, cell, f) in site_doors(t.site) {
                let f = carried(&f, &t.building.frame, &b.frame);
                #[allow(clippy::cast_possible_truncation)]
                let cell = match number.get(&((cell & 0xFFFF) as u16)) {
                    Some(n) => (u32::from(lb) << 16) | u32::from(*n),
                    None => {
                        (u32::from(lb) << 16)
                            | lyceum_foundry::edit::outdoor_cell(f.origin.x, f.origin.y)
                    }
                };
                want.entry(guid).or_insert((cell, f));
            }
        }
        assert_eq!(doors.len(), want.len(), "{lb:04X}");
        for d in &doors {
            assert_eq!(lyceum_foundry::sql_ids::number(&d.weenie), Some(DOOR));
            let [w, qx, qy, qz] = d.angles;
            let f = Frame::new(
                dereth_primitives::Vec3::new(d.origin[0], d.origin[1], d.origin[2]),
                Quat::new(w, qx, qy, qz),
            );
            assert!(
                want.values()
                    .any(|(c, g)| *c == d.cell && same_frame(&f, g)),
                "{lb:04X}: door {} in {:#010X} at {:?} is no template door",
                d.guid,
                d.cell,
                d.origin
            );
            assert!(!d.link_child);
        }
        // The walk goes in through each door to every room.
        for b in &info.buildings {
            let found = drops::walk_in(&cells, b, &|id| s.environment(id));
            assert!(found.is_empty(), "{lb:04X}: {found:?}");
        }
    }
    // The checks find no fault in them.
    let overlay = dir.path().join("overlay");
    s.write_scratch(&overlay).unwrap();
    let faults: Vec<_> = s
        .check(&overlay)
        .unwrap()
        .into_iter()
        .filter(|f| !f.warning)
        .collect();
    assert!(faults.is_empty(), "{faults:#?}");
    // What the session compiled is what a build makes of the saved sources, and the build's
    // checks pass.
    s.save().unwrap();
    let world = World::read(&s.world().path).unwrap();
    let packs: Vec<_> = world
        .packs
        .iter()
        .map(|p| lyceum_foundry::source::read_pack(p).unwrap())
        .collect();
    let lock = lyceum_foundry::lock::Lock::read(&world.lock_path()).unwrap();
    let full = compile::compile(&world, &packs, s.base(), &lock).unwrap();
    assert_eq!(full.records, s.compiled().records);
    assert_eq!(full.sql, s.compiled().sql);
    compile::build(
        &s.world().path,
        &dereth_dat::testing::dat_dir(),
        None,
        false,
    )
    .unwrap();
    // Undone, the world is as it was.
    for _ in DROPS {
        s.undo().unwrap();
    }
    assert_eq!(s.compiled().records, before);
}

#[test]
fn a_drop_is_refused_where_a_building_stands_and_outside_its_landblock() {
    let (_dir, s) = session("buildings-refused", "");
    let drop = |lb: u16, x: f32, y: f32| {
        s.drop_building(DataId(COTTAGE), 0, &Furnished::Template, lb, x, y, 0.0)
            .map(|_| ())
            .unwrap_err()
            .to_string()
    };
    // Holtburg's building 8 stands at (79.5, 37.5): its land cell is (3, 1).
    let why = drop(0xA9B4, 74.0, 30.0);
    assert!(
        why.contains("building 8") && why.contains("only one building in each"),
        "{why}"
    );
    let why = drop(0xA9B4, 200.0, 30.0);
    assert!(why.contains("outside landblock A9B4"), "{why}");
    // The source says the same where a row puts it, and the build stops.
    let edits = s.packs()[0]
        .dir
        .join("edits")
        .join("landblocks")
        .join("A9B4.toml");
    let mut text = std::fs::read_to_string(&edits).unwrap();
    text.push_str(
        "\n[[add-building]]\nname = \"cottage-9\"\nmodel = 0x01002228\ntemplate = \"3B9C/3\"\n\
         at = [74.0, 30.0, 94.0]\nheading = 0.0\n",
    );
    std::fs::write(&edits, text).unwrap();
    let why = compile::build(&s.world().path, &dereth_dat::testing::dat_dir(), None, true)
        .unwrap_err()
        .to_string();
    assert!(
        why.contains("A9B4.toml") && why.contains("only one building in each"),
        "{why}"
    );
}

#[test]
fn a_drop_reaching_further_into_the_next_land_cell_than_retail_is_warned_of() {
    let (_dir, s) = session("buildings-reach", "");
    // As the template stands in its land cell: no warning.
    let (_, quiet) = s
        .drop_building_as(
            DataId(COTTAGE),
            0,
            &Furnished::Template,
            false,
            0x9C9C,
            61.31,
            79.8,
            180.0,
        )
        .unwrap();
    assert!(quiet.is_empty(), "{quiet:?}");
    // Turned and off its usual place, it reaches past every retail cottage.
    let (_, warned) = s
        .drop_building_as(
            DataId(COTTAGE),
            0,
            &Furnished::Template,
            false,
            0x9C9C,
            60.0,
            60.0,
            30.0,
        )
        .unwrap();
    assert!(
        warned
            .iter()
            .any(|w| w.contains("into the next land cell") && w.contains("0x01002228")),
        "{warned:?}"
    );
}

#[test]
fn a_drop_takes_another_copys_furniture_or_none() {
    let (_dir, mut s) = session("buildings-furnished", "");
    let entry = s.buildings().unwrap().get(DataId(COTTAGE)).unwrap().clone();
    let other = entry.layouts[0].furnishings[1].example;
    let region = region();
    for (furnished, lb) in [
        (Furnished::Bare, 0x9C9C),
        (Furnished::Like(vec![other]), 0xA0A8),
    ] {
        let (op, _) = s
            .drop_building(DataId(COTTAGE), 0, &furnished, lb, 61.31, 79.8, 180.0)
            .unwrap();
        s.apply(&op).unwrap();
        let t = Template::read(
            &read,
            entry.layouts[0].example,
            Some(&region),
            dereth_primitives::EraId::Eor,
        )
        .unwrap();
        let f = match &furnished {
            Furnished::Like(_) => Some(
                Template::read(&read, other, Some(&region), dereth_primitives::EraId::Eor).unwrap(),
            ),
            _ => None,
        };
        let info = s.statics(lb).unwrap();
        assert_copy(&t, &info.buildings[0], &s.cells(lb), f.as_ref());
    }
}

#[test]
fn a_dungeon_entrance_copied_shuts_its_way_into_the_dungeon() {
    let c = bare_catalog();
    let entrance = c
        .entries()
        .find(|e| e.kind == Kind::Entrance && e.set.is_empty())
        .unwrap();
    let region = region();
    let t = Template::read(
        &read,
        entrance.layouts[0].example,
        Some(&region),
        dereth_primitives::EraId::Eor,
    )
    .unwrap();
    let numbers: Vec<u16> = (0..u16::try_from(t.rooms.len()).unwrap())
        .map(|k| 0x0100 + k)
        .collect();
    let to = carried(&t.building.frame, &t.building.frame, &t.building.frame);
    let copy = drops::copy(&t, None, 0x9C9C, &numbers, &to).unwrap();
    assert!(copy.shut > 0, "{}", t.site);
    let cells = copy.rooms.clone();
    for c in cells.values() {
        for p in &c.portals {
            assert!(
                p.flags & 4 != 0 || cells.contains_key(&u16::try_from(p.other_cell_id).unwrap()),
                "{}: a way out of the rooms stays",
                t.site
            );
        }
    }
    let found = drops::walk_in(&cells, &copy.building, &environment);
    assert!(found.is_empty(), "{}: {found:?}", t.site);
}

// ---------------------------------------------------------------------------------------------
// The checks of a retail building a pack moves.
// ---------------------------------------------------------------------------------------------

/// The sample world open for editing.
fn moved_session(tag: &str) -> (dereth_dat::testing::ScratchDir, Session) {
    let (dir, world) = crate::sample::sample(tag);
    let mut s = Session::open(&world, &dereth_dat::testing::dat_dir(), None).unwrap();
    s.set_today("2026-10-19 00:00:00");
    (dir, s)
}

/// Building `index` of landblock `lb` moved to `at`, turned `heading`: the checks' faults about
/// it.
fn checks_after_moving(tag: &str, lb: u16, index: usize, at: [f32; 3], heading: f32) -> Vec<Fault> {
    let (dir, mut s) = moved_session(tag);
    let op = s
        .move_building(lb, index, at, Rotation::Heading(heading))
        .unwrap();
    s.apply(&op).unwrap();
    let overlay = dir.path().join("overlay");
    s.write_scratch(&overlay).unwrap();
    let head = format!("0x{lb:04X}FFFE: building {index}:");
    s.check(&overlay)
        .unwrap()
        .into_iter()
        .filter(|f| f.text.starts_with(&head))
        .collect()
}

const HOLTBURG: u16 = 0xA9B4;

/// Holtburg's building 8, a cottage at (79.5, 37.5, 94.0), turned 45° (heading 45).
const HOLTBURG_COTTAGE: usize = 8;

#[test]
fn a_building_moved_a_little_on_level_ground_is_warned_of_nothing() {
    let f = checks_after_moving(
        "moves-quiet",
        HOLTBURG,
        HOLTBURG_COTTAGE,
        [80.0, 37.5, 94.0],
        45.0,
    );
    assert!(f.is_empty(), "{f:#?}");
}

#[test]
fn a_building_moved_onto_another_is_warned_of_and_the_warning_stands_at_it() {
    let f = checks_after_moving(
        "moves-onto",
        HOLTBURG,
        HOLTBURG_COTTAGE,
        [104.0, 36.0, 94.0],
        45.0,
    );
    let w = f
        .iter()
        .find(|f| f.text.contains("overlaps building 7"))
        .unwrap_or_else(|| panic!("{f:#?}"));
    assert!(w.warning);
    let (lb, at) = w.at.unwrap();
    assert_eq!(lb, HOLTBURG);
    assert!(
        (at.x - 104.0).abs() < 0.01 && (at.y - 36.0).abs() < 0.01,
        "{at:?}"
    );
}

#[test]
fn a_building_moved_over_a_static_is_warned_of() {
    // Static 95 stands at (106.0, 17.2, 94.0).
    let f = checks_after_moving(
        "moves-static",
        HOLTBURG,
        HOLTBURG_COTTAGE,
        [106.0, 17.5, 94.0],
        45.0,
    );
    assert!(
        f.iter()
            .any(|f| f.text.contains("static 95") && f.text.contains("stands inside it")),
        "{f:#?}"
    );
}

#[test]
fn a_building_raised_off_the_ground_is_warned_of_its_doors() {
    let f = checks_after_moving(
        "moves-raised",
        HOLTBURG,
        HOLTBURG_COTTAGE,
        [79.5, 37.5, 97.0],
        45.0,
    );
    assert!(
        f.iter()
            .any(|f| f.text.contains("door into cell") && f.text.contains("3.0 m over the ground")),
        "{f:#?}"
    );
}

#[test]
fn a_house_moved_off_its_plot_is_warned_of() {
    // 3C98's one building, a house on a plot of one land cell.
    let f = checks_after_moving("moves-house", 0x3C98, 0, [60.0, 60.0, 82.0], 0.0);
    assert!(
        f.iter()
            .any(|f| f.text.contains("a house moved off its plot")),
        "{f:#?}"
    );
}

// ---------------------------------------------------------------------------------------------
// A dropped building moved, turned and taken away; where a drop snaps, and what it is told.
// ---------------------------------------------------------------------------------------------

/// The corners of the land cells (world grid) under the shells of the buildings at `places` of
/// landblock `lb`, each with its height index now.
fn ground_under(s: &Session, lb: u16, places: &[usize]) -> BTreeMap<(u32, u32), u8> {
    let info = s.statics(lb).unwrap();
    let mut models = lyceum_foundry::pick::Models::default();
    let mut out = BTreeMap::new();
    for i in places {
        let b = &info.buildings[*i];
        let m = models.get(s.base(), b.id).unwrap();
        for p in m.triangles.iter().flatten() {
            let g = localtoglobal(&b.frame, *p);
            let (cx, cy) = buildings::land_cell(g.x, g.y);
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let gx = u32::try_from(i32::from(lb >> 8) * 8 + cx + dx).unwrap();
                let gy = u32::try_from(i32::from(lb & 0xFF) * 8 + cy + dy).unwrap();
                out.insert((gx, gy), s.vertex(gx, gy).unwrap().height);
            }
        }
    }
    out
}

/// Every vertex of landblock `lb` and of the land around it, with its height index and scene.
fn ground_around(s: &Session, lb: u16) -> BTreeMap<(u32, u32), (u8, u16)> {
    let (bx, by) = (u32::from(lb >> 8) * 8, u32::from(lb & 0xFF) * 8);
    let mut out = BTreeMap::new();
    for gx in bx - 8..=bx + 16 {
        for gy in by - 8..=by + 16 {
            let v = s.vertex(gx, gy).unwrap();
            out.insert((gx, gy), (v.height, v.scene));
        }
    }
    out
}

/// Whether two frames stand in the same place facing the same way (a spawn's SQL keeps its
/// heading).
fn same_place(a: &Frame, b: &Frame) -> bool {
    let heading = dereth_primitives::frame::get_heading;
    let turn = (heading(a) - heading(b)).rem_euclid(360.0);
    (a.origin.x - b.origin.x).abs() < 2e-3
        && (a.origin.y - b.origin.y).abs() < 2e-3
        && (a.origin.z - b.origin.z).abs() < 2e-3
        && turn.min(360.0 - turn) < 0.02
}

/// The spawns of `s` whose guid holds `part`, by guid: each one's cell and frame.
fn spawns_named(s: &Session, part: &str) -> BTreeMap<String, (u32, Frame)> {
    s.spawns()
        .into_iter()
        .filter(|x| x.guid.contains(part))
        .map(|x| (x.guid, (x.cell, x.frame)))
        .collect()
}

/// The statics of landblock `lb` named with `part`, by name: each one's frame.
fn statics_named(s: &Session, lb: u16, part: &str) -> BTreeMap<String, Frame> {
    let Some(info) = s.statics(lb) else {
        return BTreeMap::new();
    };
    s.origins(lb)
        .into_iter()
        .zip(&info.objects)
        .filter_map(|(o, e)| match o {
            lyceum_foundry::compile::StaticOrigin::Pack { name: Some(n), .. }
                if n.contains(part) =>
            {
                Some((n, e.frame))
            }
            _ => None,
        })
        .collect()
}

/// The first model of the catalog's gallery with no set, doors where it stands and outdoor
/// statics that come with it.
fn with_outdoor_statics(c: &Catalog) -> DataId {
    c.gallery()
        .into_iter()
        .find(|e| e.set.is_empty() && !e.layouts[0].outdoor.is_empty() && e.layouts[0].doorways > 0)
        .unwrap()
        .model
}

/// The first place of landblock `lb`, a land cell at a time, where a drop of `model` snaps to
/// and stands on ground it levels: its origin and heading.
fn sloped_drop(s: &Session, model: DataId, lb: u16) -> (f32, f32, f32) {
    for cx in 1..7u8 {
        for cy in 1..7u8 {
            let (px, py) = (f32::from(cx) * 24.0 + 12.0, f32::from(cy) * 24.0 + 12.0);
            let (x, y, h) = s.drop_snap(model, 0, px, py, 0).unwrap();
            let Ok((lyceum_foundry::edit::Op::Batch(ops), _)) =
                s.drop_building(model, 0, &Furnished::Template, lb, x, y, h)
            else {
                continue;
            };
            let levels = ops.iter().any(|o| {
                matches!(o, lyceum_foundry::edit::Op::SetVertices(v)
                    if v.iter().any(|v| v.field == compile::Field::Height))
            });
            if levels {
                return (x, y, h);
            }
        }
    }
    panic!("no land cell of {lb:04X} that a drop levels");
}

/// What the session compiled is what a build makes of its saved sources.
fn assert_saved_builds_the_same(s: &mut Session) {
    s.save().unwrap();
    let world = World::read(&s.world().path).unwrap();
    let packs: Vec<_> = world
        .packs
        .iter()
        .map(|p| lyceum_foundry::source::read_pack(p).unwrap())
        .collect();
    let lock = lyceum_foundry::lock::Lock::read(&world.lock_path()).unwrap();
    let full = compile::compile(&world, &packs, s.base(), &lock).unwrap();
    assert_eq!(full.records, s.compiled().records);
    assert_eq!(full.sql, s.compiled().sql);
}

#[test]
fn a_dropped_building_moved_and_turned_takes_its_rooms_doors_outdoor_statics_and_level_ground() {
    let model = with_outdoor_statics(bare_catalog());
    let (_dir, mut s) = session("buildings-moved", &doors_sql(&[model.raw()]));
    let lb = 0x9C9C;
    // Where it first stands, and the ground there before it.
    let (x, y, h) = sloped_drop(&s, model, lb);
    let plain = ground_around(&s, lb);
    let (op, _) = s
        .drop_building(model, 0, &Furnished::Template, lb, x, y, h)
        .unwrap();
    s.apply(&op).unwrap();
    let index = s.compiled().dropped[&lb][0].1;
    let under_before = ground_under(&s, lb, &[index]);
    let doors_before = spawns_named(&s, "-door-");
    let outdoor_before = statics_named(&s, lb, "-outdoor-");
    assert!(!doors_before.is_empty(), "the drop has doors");
    assert!(!outdoor_before.is_empty(), "the drop has outdoor statics");
    // Something placed in one of its rooms: a static and a spawn.
    let from = s.statics(lb).unwrap().buildings[index].frame;
    let room = *s.compiled().rooms[&lb].values().next().unwrap();
    let room_frame = s.cells(lb)[&room].frame;
    s.apply(&lyceum_foundry::edit::Op::AddCellStatic {
        landblock: lb,
        cell: room,
        model: DataId(0x0200_040B),
        at: [
            room_frame.origin.x,
            room_frame.origin.y,
            room_frame.origin.z,
        ],
        rotation: Rotation::Heading(0.0),
    })
    .unwrap();
    // Moved two land cells across and one along, and turned a quarter.
    let (dx, dy) = (
        if x > 96.0 { -48.0 } else { 48.0 },
        if y > 96.0 { -24.0 } else { 24.0 },
    );
    let to = Frame::new(
        dereth_primitives::Vec3::new(x + dx, y + dy, from.origin.z + 5.0),
        compile::heading_quat(h + 90.0),
    );
    let steps = s.history().0;
    let op = s
        .move_building(
            lb,
            index,
            [to.origin.x, to.origin.y, to.origin.z],
            Rotation::Heading(h + 90.0),
        )
        .unwrap();
    assert!(
        matches!(op, lyceum_foundry::edit::Op::MoveBuilding { .. }),
        "a dropped building is moved by its row: {op:?}"
    );
    s.apply(&op).unwrap();
    assert_eq!(s.history().0, steps + 1, "one step");
    // The row says where it stands now, on level ground as high as its template stands.
    let row = s.packs()[0]
        .edits
        .iter()
        .flat_map(|e| e.drops.iter())
        .next()
        .unwrap()
        .item
        .clone();
    assert!((row.at.x - to.origin.x).abs() < 1e-3 && (row.at.y - to.origin.y).abs() < 1e-3);
    let Rotation::Heading(turned) = row.rotation else {
        panic!("{:?}", row.rotation)
    };
    let off = (turned - (h + 90.0)).rem_euclid(360.0);
    assert!(off.min(360.0 - off) < 0.01, "{turned}");
    assert!(
        s.packs()[0].edits.iter().all(|e| e.buildings.is_empty()),
        "no move-building row"
    );
    let b = s.statics(lb).unwrap().buildings[index].clone();
    let t = Template::read(
        &read,
        row.templates[0],
        Some(&region()),
        dereth_primitives::EraId::Eor,
    )
    .unwrap();
    let ground = s.ground(lb, b.frame.origin.x, b.frame.origin.y).unwrap();
    assert!(
        (b.frame.origin.z - ground - t.height()).abs() < 2e-3,
        "origin {} over ground {ground}",
        b.frame.origin.z
    );
    let level = ground_under(&s, lb, &[index]);
    let first = *level.values().next().unwrap();
    assert!(level.values().all(|v| *v == first), "{level:?}");
    // The ground it stood on is the world's again, but where it stands now.
    for (v, was) in &under_before {
        if !level.contains_key(v) {
            let now = s.vertex(v.0, v.1).unwrap();
            assert_eq!(
                (now.height, now.scene),
                plain[v],
                "vertex {v:?} (was {was} under the drop)"
            );
        }
    }
    // Its doors, outdoor statics, and what is in its rooms are carried by the one move.
    let carry = |f: &Frame| carried(f, &from, &b.frame);
    let doors = spawns_named(&s, "-door-");
    assert_eq!(doors.len(), doors_before.len());
    for (g, (cell, f)) in &doors_before {
        let (now_cell, now) = &doors[g];
        assert!(same_place(now, &carry(f)), "door {g}: {now:?}");
        if cell & 0xFFFF >= 0x0100 {
            assert_eq!(now_cell, cell, "door {g} keeps its room");
        } else {
            assert_eq!(
                *now_cell & 0xFFFF,
                lyceum_foundry::edit::outdoor_cell(now.origin.x, now.origin.y),
                "door {g} stands in the land cell it is in now"
            );
        }
    }
    let outdoor = statics_named(&s, lb, "-outdoor-");
    assert_eq!(outdoor.len(), outdoor_before.len());
    for (n, f) in &outdoor_before {
        assert!(same_frame(&outdoor[n], &carry(f)), "{n}");
    }
    let placed = s.cells(lb)[&room].static_objects.last().unwrap().frame;
    let was = Frame::new(room_frame.origin, compile::heading_quat(0.0));
    assert!(
        same_frame(&placed, &carry(&was)),
        "the static in its room went with it: {placed:?}"
    );
    // Undone, it stands where it stood.
    s.undo().unwrap();
    assert_eq!(s.statics(lb).unwrap().buildings[index].frame, from);
    assert_eq!(ground_under(&s, lb, &[index]), under_before);
    s.redo().unwrap();
    assert_saved_builds_the_same(&mut s);
}

#[test]
fn a_drop_taken_away_beside_another_leaves_the_other_on_its_level_ground() {
    let (_dir, mut s) = session("buildings-side-by-side", "");
    let lb = 0x9C9C;
    let cottage = DataId(COTTAGE);
    // One on ground its drop levels, and another in the land cell beside it, east or west.
    let (x, y, h) = sloped_drop(&s, cottage, lb);
    let (op, _) = s
        .drop_building(cottage, 0, &Furnished::Template, lb, x, y, h)
        .unwrap();
    s.apply(&op).unwrap();
    let beside = [x + 24.0, x - 24.0]
        .into_iter()
        .find_map(|bx| {
            s.drop_building(cottage, 0, &Furnished::Template, lb, bx, y, h)
                .ok()
        })
        .expect("a free land cell beside it");
    s.apply(&beside.0).unwrap();
    let [(_, first), (_, second)] = &s.compiled().dropped[&lb][..] else {
        panic!("{:?}", s.compiled().dropped)
    };
    let (first, second) = (*first, *second);
    let level = ground_under(&s, lb, &[second]);
    let shared = ground_under(&s, lb, &[first])
        .keys()
        .filter(|v| level.contains_key(v))
        .count();
    assert!(shared > 0, "the two share corners");
    s.apply(&lyceum_foundry::edit::Op::RemoveBuilding {
        landblock: lb,
        index: first,
    })
    .unwrap();
    // The one left is the drop's only building now, on the ground it was left on.
    let left = s.compiled().dropped[&lb][0].1;
    assert_eq!(ground_under(&s, lb, &[left]), level);
}

#[test]
fn a_mansion_moved_by_any_of_its_four_moves_all_four_together() {
    let (_dir, mut s) = session("buildings-moved-set", "");
    let lb = 0xA9AD;
    let (op, _) = s
        .drop_building(
            DataId(MANSION),
            0,
            &Furnished::Template,
            lb,
            132.0,
            108.0,
            0.0,
        )
        .unwrap();
    s.apply(&op).unwrap();
    let places: Vec<usize> = s.compiled().dropped[&lb].iter().map(|(_, i)| *i).collect();
    assert_eq!(places.len(), 4);
    let before: Vec<Frame> = places
        .iter()
        .map(|i| s.statics(lb).unwrap().buildings[*i].frame)
        .collect();
    // The third of the four, two land cells west.
    let f = before[2];
    let op = s
        .move_building(
            lb,
            places[2],
            [f.origin.x - 48.0, f.origin.y, f.origin.z],
            Rotation::Quat([f.rotation.w, f.rotation.x, f.rotation.y, f.rotation.z]),
        )
        .unwrap();
    s.apply(&op).unwrap();
    let info = s.statics(lb).unwrap();
    for (i, was) in places.iter().zip(&before) {
        let now = info.buildings[*i].frame.origin;
        assert!(
            (now.x - (was.origin.x - 48.0)).abs() < 2e-3 && (now.y - was.origin.y).abs() < 2e-3,
            "building {i}: {now:?} from {:?}",
            was.origin
        );
    }
    assert_eq!(s.compiled().dropped[&lb].len(), 4);
}

#[test]
fn a_dropped_building_moved_onto_another_s_land_cell_is_refused_and_nothing_changes() {
    let (_dir, mut s) = session("buildings-moved-refused", "");
    let lb = 0x9C9C;
    for (x, y) in [(61.31, 79.8), (109.31, 79.8)] {
        let (op, _) = s
            .drop_building(DataId(COTTAGE), 0, &Furnished::Template, lb, x, y, 180.0)
            .unwrap();
        s.apply(&op).unwrap();
    }
    let records = s.compiled().records.clone();
    let steps = s.history();
    let first = s.compiled().dropped[&lb][0].1;
    let f = s.statics(lb).unwrap().buildings[first].frame;
    let op = s
        .move_building(
            lb,
            first,
            [110.0, 80.0, f.origin.z],
            Rotation::Heading(180.0),
        )
        .unwrap();
    let why = s.apply(&op).unwrap_err().to_string();
    assert!(
        why.contains("cottage-1") && why.contains("only one building in each"),
        "{why}"
    );
    assert_eq!(s.compiled().records, records);
    assert_eq!(s.history(), steps);
}

#[test]
fn a_dropped_building_taken_away_takes_its_rooms_doors_outdoor_statics_and_level_ground() {
    let model = with_outdoor_statics(bare_catalog());
    let (_dir, mut s) = session("buildings-removed", &doors_sql(&[model.raw()]));
    let lb = 0x9C9C;
    let plain = ground_around(&s, lb);
    let (x, y, h) = sloped_drop(&s, model, lb);
    let (op, _) = s
        .drop_building(model, 0, &Furnished::Template, lb, x, y, h)
        .unwrap();
    s.apply(&op).unwrap();
    assert_ne!(ground_around(&s, lb), plain, "the drop levels the ground");
    let index = s.compiled().dropped[&lb][0].1;
    let room = *s.compiled().rooms[&lb].values().next().unwrap();
    let rf = s.cells(lb)[&room].frame;
    s.apply(&lyceum_foundry::edit::Op::PlaceSpawnInCell {
        landblock: lb,
        cell: room,
        weenie: lyceum_foundry::edit::Weenie::Class(DOOR),
        origin: rf.origin,
        heading: 0.0,
    })
    .unwrap();
    s.apply(&lyceum_foundry::edit::Op::AddCellStatic {
        landblock: lb,
        cell: room,
        model: DataId(0x0200_040B),
        at: [rf.origin.x, rf.origin.y, rf.origin.z],
        rotation: Rotation::Heading(0.0),
    })
    .unwrap();
    let steps = s.history().0;
    s.apply(&lyceum_foundry::edit::Op::RemoveBuilding {
        landblock: lb,
        index,
    })
    .unwrap();
    assert_eq!(s.history().0, steps + 1, "one step");
    assert!(!s.compiled().dropped.contains_key(&lb));
    assert!(s.packs()[0]
        .edits
        .iter()
        .all(|e| e.drops.is_empty() && e.cell_adds.is_empty()));
    assert!(
        spawns_named(&s, "@9C9C").is_empty(),
        "its doors and the spawn in its room"
    );
    assert!(statics_named(&s, lb, "-outdoor-").is_empty());
    assert!(s.statics(lb).is_none_or(|i| i.buildings.is_empty()));
    // The ground is as it was before the drop.
    assert_eq!(ground_around(&s, lb), plain);
    // Undone, it is back with all it had.
    s.undo().unwrap();
    assert_eq!(s.compiled().dropped[&lb].len(), 1);
    assert!(!statics_named(&s, lb, "-outdoor-").is_empty());
    // One of the world's own is not taken away.
    let why = s
        .apply(&lyceum_foundry::edit::Op::RemoveBuilding {
            landblock: 0xA9B4,
            index: 8,
        })
        .unwrap_err()
        .to_string();
    assert!(why.contains("one of the world's own"), "{why}");
}

#[test]
fn a_drop_snaps_where_its_example_stands_in_its_land_cell_and_turns_by_quarters_overhanging_no_more(
) {
    let (_dir, s) = session("buildings-snap", "");
    let c = s.buildings().unwrap();
    let e = c.get(DataId(COTTAGE)).unwrap();
    let t = Template::read(
        &read,
        e.layouts[0].example,
        None,
        dereth_primitives::EraId::Eor,
    )
    .unwrap();
    let o = t.building.frame.origin;
    let lb = 0x9C9C;
    let (x, y, h) = s.drop_snap(DataId(COTTAGE), 0, 100.0, 50.0, 0).unwrap();
    // In the pointer's land cell, where the example stands in its own.
    assert_eq!(
        buildings::land_cell(x, y),
        buildings::land_cell(100.0, 50.0)
    );
    assert!(
        (x.rem_euclid(24.0) - o.x.rem_euclid(24.0)).abs() < 1e-3,
        "{x}"
    );
    assert!(
        (y.rem_euclid(24.0) - o.y.rem_euclid(24.0)).abs() < 1e-3,
        "{y}"
    );
    for turns in 0..4u8 {
        let (x, y, heading) = s.drop_snap(DataId(COTTAGE), 0, 100.0, 50.0, turns).unwrap();
        let off = (heading - h - f32::from(turns) * 90.0).rem_euclid(360.0);
        assert!(off.min(360.0 - off) < 1e-3, "turn {turns}: {heading}");
        let plan = s.drop_plan(DataId(COTTAGE), 0, lb, x, y, heading).unwrap();
        assert_eq!(plan.refusal, None);
        assert!(
            plan.warnings.is_empty(),
            "turn {turns}: {:?}",
            plan.warnings
        );
        assert_eq!(
            buildings::land_cell(x, y),
            buildings::land_cell(100.0, 50.0)
        );
    }
}

#[test]
fn a_drops_plan_says_where_its_shells_stand_and_why_a_taken_land_cell_refuses_it() {
    let (_dir, s) = session("buildings-plan", "");
    // Holtburg's building 8 stands at (79.5, 37.5).
    let plan = s
        .drop_plan(DataId(COTTAGE), 0, 0xA9B4, 74.0, 30.0, 0.0)
        .unwrap();
    assert_eq!(plan.taken, Some(8));
    assert!(plan.refusal.unwrap().contains("only one building in each"));
    let plan = s
        .drop_plan(DataId(MANSION), 0, 0xA9AD, 132.0, 108.0, 0.0)
        .unwrap();
    assert_eq!((plan.refusal, plan.taken), (None, None));
    assert_eq!(plan.shells.len(), 4);
    assert_eq!(plan.shells[0].0, DataId(MANSION));
    let (op, _) = s
        .drop_building(
            DataId(MANSION),
            0,
            &Furnished::Bare,
            0xA9AD,
            132.0,
            108.0,
            0.0,
        )
        .unwrap();
    let mut s = s;
    s.apply(&op).unwrap();
    let info = s.statics(0xA9AD).unwrap();
    for ((model, f), b) in plan.shells.iter().zip(&info.buildings) {
        assert_eq!(*model, b.id);
        assert!(same_frame(f, &b.frame));
    }
}

#[test]
fn a_move_building_row_past_the_worlds_own_buildings_says_a_drop_moves_by_its_row() {
    let (_dir, s) = session("buildings-move-row", "");
    let edits = s.packs()[0]
        .dir
        .join("edits")
        .join("landblocks")
        .join("A9B4.toml");
    let mut text = std::fs::read_to_string(&edits).unwrap();
    text.push_str("\n[[move-building]]\nindex = 99\nat = [60.0, 60.0, 94.0]\nheading = 0.0\n");
    std::fs::write(&edits, text).unwrap();
    let why = compile::build(&s.world().path, &dereth_dat::testing::dat_dir(), None, true)
        .unwrap_err()
        .to_string();
    assert!(
        why.contains("of its own") && why.contains("add-building row"),
        "{why}"
    );
}

/// Retail houses dropped as houses players can buy, checked against the server's world database.
mod houses {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::PathBuf;
    use std::sync::OnceLock;

    use dereth_assets::world::LandblockInfo;
    use dereth_assets::Decode;
    use dereth_primitives::frame::combine;
    use dereth_primitives::{DataId, Frame, Quat, Vec3};
    use lyceum_foundry::buildings::{self, Catalog, Kind, Site};
    use lyceum_foundry::cells::{carried, inverse};
    use lyceum_foundry::compile;
    use lyceum_foundry::dats::DatSet;
    use lyceum_foundry::drops::{self, Template};
    use lyceum_foundry::edit::{Op, Session};
    use lyceum_foundry::source::Rotation;
    use lyceum_foundry::sql_ids::{self, ServerContent};
    use lyceum_foundry::symbol::Kind as Sym;
    use lyceum_foundry::world::World;

    const COTTAGE: u32 = 0x0100_2228;
    const VILLA: u32 = 0x0100_22CE;
    const MANSION: u32 = 0x0100_2286;

    /// Where each house drops: the landblock (none has a statics record), the lead's origin, its
    /// heading.
    const HOUSES: [(u32, u16, f32, f32, f32); 3] = [
        (COTTAGE, 0x9C9C, 61.31, 79.8, 180.0),
        (VILLA, 0xA0A8, 84.0, 84.0, 90.0),
        (MANSION, 0xA9AD, 132.0, 108.0, 0.0),
    ];

    /// The weenie types of a house, a slumlord, a hook, storage, a boot spot and a house portal.
    const HOUSE: u32 = 53;
    const SLUMLORD: u32 = 55;
    const HOOK: u32 = 56;
    const STORAGE: u32 = 57;
    const BOOT_SPOT: u32 = 58;
    const PORTAL: u32 = 59;

    fn files() -> &'static DatSet {
        static SET: OnceLock<DatSet> = OnceLock::new();
        SET.get_or_init(|| {
            dereth_dat::testing::require_dats();
            DatSet::open(&dereth_dat::testing::dat_dir()).unwrap()
        })
    }

    fn read(id: u32) -> Option<Vec<u8>> {
        files().cell.read(DataId(id)).ok()
    }

    /// The server's world database: `EMPYREAN_WORLD_SQL`, else the newest dump in the importer's
    /// per-user cache.
    fn world_sql() -> PathBuf {
        if let Some(p) = std::env::var_os("EMPYREAN_WORLD_SQL").filter(|v| !v.is_empty()) {
            return PathBuf::from(p);
        }
        let var = |n: &str| {
            std::env::var_os(n)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let dir = if cfg!(windows) {
            var("LOCALAPPDATA").map(|d| d.join("Empyrean"))
        } else if cfg!(target_os = "macos") {
            var("HOME").map(|d| d.join("Library/Caches/Empyrean"))
        } else {
            var("XDG_CACHE_HOME")
                .or_else(|| var("HOME").map(|h| h.join(".cache")))
                .map(|d| d.join("empyrean"))
        }
        .map(|d| d.join("world-database"));
        let mut found: Vec<PathBuf> = dir
            .and_then(|d| std::fs::read_dir(d).ok())
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("ACE-World-Database-v") && n.ends_with(".sql"))
            })
            .collect();
        found.sort();
        found.pop().expect(
        "the server's world database: set EMPYREAN_WORLD_SQL, or fetch it with empyrean-import \
         fetch",
    )
    }

    fn content() -> &'static ServerContent {
        static C: OnceLock<ServerContent> = OnceLock::new();
        C.get_or_init(|| sql_ids::server_content(&std::fs::read_to_string(world_sql()).unwrap()))
    }

    fn catalog() -> &'static Catalog {
        static C: OnceLock<Catalog> = OnceLock::new();
        C.get_or_init(|| Catalog::read(files(), Some(content()), &|_| None))
    }

    /// The sample world open for editing, with the catalog and the server's world database.
    fn session(tag: &str) -> (dereth_dat::testing::ScratchDir, Session) {
        let (dir, world) = crate::sample::sample(tag);
        let mut s = Session::open(&world, &dereth_dat::testing::dat_dir(), None).unwrap();
        s.set_today("2026-10-22 00:00:00");
        s.learn_buildings(catalog().clone());
        (dir, s)
    }

    fn info(lb: u16) -> LandblockInfo {
        let id = (u32::from(lb) << 16) | 0xFFFE;
        LandblockInfo::decode_payload(DataId(id), &read(id).unwrap()).unwrap()
    }

    /// The guid the restriction table of `site`'s landblock gives the land cell holding its origin.
    fn house_at(site: Site) -> u32 {
        let i = info(site.landblock);
        let o = i.buildings[site.index].frame.origin;
        let key = (u32::from(site.landblock) << 16) | lyceum_foundry::edit::outdoor_cell(o.x, o.y);
        i.restrictions
            .unwrap()
            .entries
            .iter()
            .find(|(k, _)| *k == key)
            .unwrap()
            .1
    }

    /// The sites of a drop of `model`: its first layout's example, and its set's partners there.
    fn sites(model: u32) -> Vec<Site> {
        let e = catalog().get(DataId(model)).unwrap();
        let lead = e.layouts[0].example;
        let i = info(lead.landblock);
        let from = i.buildings[lead.index].frame;
        let mut out = vec![lead];
        for p in &e.set {
            let want = combine(&from, &p.frame);
            let index = i
                .buildings
                .iter()
                .position(|b| {
                    b.id == p.model
                        && (b.frame.origin.x - want.origin.x).abs() < 0.5
                        && (b.frame.origin.y - want.origin.y).abs() < 0.5
                })
                .unwrap();
            out.push(Site {
                landblock: lead.landblock,
                index,
            });
        }
        out
    }

    /// One spawn of the server's world: its weenie, cell and frame (landblock meters), and whether
    /// it is a link child.
    #[derive(Clone, Debug)]
    struct Spawn {
        weenie: u32,
        cell: u32,
        frame: Frame,
        child: bool,
    }

    /// The spawns of `content`, by guid, and the guids each links.
    fn spawns_of(content: &ServerContent) -> (BTreeMap<u32, Spawn>, BTreeMap<u32, Vec<u32>>) {
        let mut by = BTreeMap::new();
        for i in &content.instances {
            let (Some(g), Some(w)) = (sql_ids::number(&i.guid), sql_ids::number(&i.weenie)) else {
                continue;
            };
            let [qw, qx, qy, qz] = i.angles;
            by.insert(
                g,
                Spawn {
                    weenie: w,
                    cell: i.cell,
                    frame: Frame::new(
                        Vec3::new(i.origin[0], i.origin[1], i.origin[2]),
                        Quat::new(qw, qx, qy, qz),
                    ),
                    child: i.link_child,
                },
            );
        }
        let mut links: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        for (p, c) in &content.links {
            links.entry(*p).or_default().push(*c);
        }
        (by, links)
    }

    /// Every guid `roots` link, the roots first, each once.
    fn linked(roots: &[u32], links: &BTreeMap<u32, Vec<u32>>) -> Vec<u32> {
        let mut out: Vec<u32> = Vec::new();
        let mut todo: Vec<u32> = roots.to_vec();
        while let Some(g) = todo.first().copied() {
            todo.remove(0);
            if out.contains(&g) {
                continue;
            }
            out.push(g);
            todo.extend(links.get(&g).into_iter().flatten().copied());
        }
        out
    }

    fn kind(w: u32) -> u32 {
        content().weenies.get(&w).and_then(|f| f.kind).unwrap_or(0)
    }

    fn same_frame(a: &Frame, b: &Frame) -> bool {
        let (p, q) = (a.origin, b.origin);
        let close = |x: f32, y: f32| (x - y).abs() < 2e-3;
        let turn = |s: f32| {
            let (r, t) = (a.rotation, b.rotation);
            (r.w - s * t.w).abs() < 1e-4
                && (r.x - s * t.x).abs() < 1e-4
                && (r.y - s * t.y).abs() < 1e-4
                && (r.z - s * t.z).abs() < 1e-4
        };
        close(p.x, q.x) && close(p.y, q.y) && close(p.z, q.z) && (turn(1.0) || turn(-1.0))
    }

    /// `f` in the frame `of` stands in.
    fn relative(of: &Frame, f: &Frame) -> Frame {
        combine(&inverse(of), f)
    }

    #[test]
    fn the_catalog_knows_each_house_s_spawns_as_its_example_has_them() {
        let (by, links) = spawns_of(content());
        for model in [COTTAGE, VILLA, MANSION] {
            let e = catalog().get(DataId(model)).unwrap();
            let house = e.layouts[0].house.as_ref().unwrap();
            let sites = sites(model);
            let roots: Vec<u32> = sites.iter().map(|s| house_at(*s)).collect();
            // Each building's own house first, then all they link, and the basement's spawns.
            let outdoor = linked(&roots, &links);
            let basement: Vec<u32> = match house.basement {
                None => Vec::new(),
                Some(b) => by
                    .iter()
                    .filter(|(_, s)| s.cell >> 16 == u32::from(b))
                    .map(|(g, _)| *g)
                    .collect(),
            };
            let want: Vec<u32> = outdoor.iter().chain(&basement).copied().collect();
            assert_eq!(house.objects.len(), want.len(), "{model:#010X}");
            let place: BTreeMap<u32, usize> =
                want.iter().enumerate().map(|(i, g)| (*g, i)).collect();
            let lb = sites[0].landblock;
            let building = |k: usize| info(lb).buildings[sites[k].index].frame;
            for (k, g) in roots.iter().enumerate() {
                assert_eq!(
                    house.house_of(k),
                    Some(place[g]),
                    "{model:#010X} building {k}"
                );
            }
            for (o, g) in house.objects.iter().zip(&want) {
                let s = &by[g];
                assert_eq!(o.weenie, s.weenie);
                assert_eq!(o.kind, kind(s.weenie));
                assert_eq!(o.link_child, s.child);
                #[allow(clippy::cast_possible_truncation)]
                let low = (s.cell & 0xFFFF) as u16;
                match o.spot {
                    buildings::Spot::Basement(n) => {
                        assert_eq!(Some((s.cell >> 16) as u16), house.basement);
                        assert_eq!(n, low);
                        assert!(same_frame(&o.frame, &s.frame));
                    }
                    spot => {
                        assert_eq!(s.cell >> 16, u32::from(lb));
                        if low >= 0x100 {
                            assert_eq!(spot, buildings::Spot::Room(low));
                        } else {
                            assert_eq!(spot, buildings::Spot::Outdoors);
                        }
                        assert!(same_frame(
                            &o.frame,
                            &relative(&building(o.member), &s.frame)
                        ));
                    }
                }
            }
            // The links, each once.
            let mut want_links: BTreeSet<(usize, usize)> = BTreeSet::new();
            for p in &want {
                for c in links.get(p).into_iter().flatten() {
                    if let Some(c) = place.get(c) {
                        want_links.insert((place[p], *c));
                    }
                }
            }
            let have: BTreeSet<(usize, usize)> = house.links.iter().copied().collect();
            assert_eq!(have, want_links, "{model:#010X}");
            // Its house class's rows, and what each house is made of.
            assert_eq!(house.class, by[&roots[0]].weenie);
            assert_eq!(house.rows, content().houses[&house.class]);
            let count = |k: u32| house.objects.iter().filter(|o| o.kind == k).count();
            assert_eq!(count(SLUMLORD), 1, "{model:#010X}");
            assert!(count(HOOK) >= 20, "{model:#010X}: {} hooks", count(HOOK));
            assert!(count(STORAGE) >= 1, "{model:#010X}");
            assert_eq!(count(BOOT_SPOT), 1, "{model:#010X}");
            let root = house.root().unwrap();
            assert_eq!(house.objects[root].kind, HOUSE);
            match e.kind {
                Kind::Cottage => {
                    assert_eq!(house.basement, None);
                    assert_eq!(count(PORTAL), 0);
                    assert_eq!(count(HOUSE), 1);
                }
                Kind::Villa => {
                    assert!(house.basement.is_some());
                    assert_eq!(count(PORTAL), 2);
                    assert_eq!(count(HOUSE), 2);
                }
                Kind::Mansion => {
                    assert!(house.basement.is_some());
                    assert_eq!(count(PORTAL), 2);
                    assert_eq!(count(HOUSE), 5, "four buildings' and the basement's");
                }
                other => panic!("{model:#010X} is a {other:?}"),
            }
        }
        // No building that is not a house has one.
        assert!(catalog().entries().all(|e| {
            matches!(e.kind, Kind::Cottage | Kind::Villa | Kind::Mansion)
                || e.layouts.iter().all(|l| l.house.is_none())
        }));
    }

    #[test]
    fn the_catalog_kept_in_a_file_keeps_its_houses() {
        let text = catalog().to_text();
        let back = Catalog::from_text(&text).unwrap();
        assert_eq!(&back, catalog());
    }

    /// Each retail spawn of the house of the copies at `sites` (each building's house first, then
    /// what they link, then every spawn of its basement), with the building it stands with: the one
    /// whose house links it first.
    fn retail_house(sites: &[Site], basement: Option<u16>) -> Vec<(u32, usize)> {
        let (by, links) = spawns_of(content());
        let mut out: Vec<(u32, usize)> = Vec::new();
        let mut todo: Vec<(u32, usize)> = sites
            .iter()
            .enumerate()
            .map(|(k, s)| (house_at(*s), k))
            .collect();
        while !todo.is_empty() {
            let (g, k) = todo.remove(0);
            if out.iter().any(|(h, _)| *h == g) {
                continue;
            }
            out.push((g, k));
            todo.extend(links.get(&g).into_iter().flatten().map(|c| (*c, k)));
        }
        if let Some(b) = basement {
            out.extend(
                by.iter()
                    .filter(|(_, s)| s.cell >> 16 == u32::from(b))
                    .map(|(g, _)| (*g, 0)),
            );
        }
        out
    }

    /// What the session's SQL places, filled: its spawns, links and house weenies, and its house
    /// portals' rows (each its house id, cell and frame).
    fn placed(s: &Session) -> (ServerContent, Vec<(u32, u32, Frame)>) {
        let text: String = s
            .compiled()
            .sql
            .values()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        let mut portals = Vec::new();
        sql_ids::rows(&text, |row| {
            if row.table != "house_portal" {
                return;
            }
            let n = |c: &str| row.get(c).and_then(sql_ids::number).unwrap();
            let f = |c: &str| row.get(c).unwrap().trim().parse::<f32>().unwrap();
            portals.push((
                n("house_Id"),
                n("obj_Cell_Id"),
                Frame::new(
                    Vec3::new(f("origin_X"), f("origin_Y"), f("origin_Z")),
                    Quat::new(f("angles_W"), f("angles_X"), f("angles_Y"), f("angles_Z")),
                ),
            ));
        });
        (sql_ids::server_content(&text), portals)
    }

    /// What a session compiled is what a build makes of its saved sources.
    fn assert_saved_builds_the_same(s: &mut Session) {
        s.save().unwrap();
        let world = World::read(&s.world().path).unwrap();
        let packs: Vec<_> = world
            .packs
            .iter()
            .map(|p| lyceum_foundry::source::read_pack(p).unwrap())
            .collect();
        let lock = lyceum_foundry::lock::Lock::read(&world.lock_path()).unwrap();
        let full = compile::compile(&world, &packs, s.base(), &lock).unwrap();
        assert_eq!(full.records, s.compiled().records);
        assert_eq!(full.sql, s.compiled().sql);
    }

    /// The landblock the basement of drop `name` (in full) of landblock `lb` was copied into.
    fn basement_landblock(s: &Session, name: &str, lb: u16) -> u16 {
        let below = drops::basement_name(name, lb);
        *s.compiled()
            .rooms
            .iter()
            .find(|(_, r)| r.keys().any(|n| n.starts_with(&below)))
            .unwrap()
            .0
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn a_dropped_cottage_villa_and_mansion_are_houses_as_their_retail_copies_are() {
        let (dir, mut s) = session("houses-drop");
        let before = s.compiled().records.clone();
        for (model, lb, x, y, heading) in HOUSES {
            assert!(s.statics(lb).is_none(), "{lb:04X} starts with no statics");
            let (op, warnings) = s
                .drop_building_as(
                    DataId(model),
                    0,
                    &lyceum_foundry::source::Furnished::Template,
                    true,
                    lb,
                    x,
                    y,
                    heading,
                )
                .unwrap();
            assert!(warnings.is_empty(), "{warnings:?}");
            s.apply(&op).unwrap();
        }
        let (now, portals) = placed(&s);
        let (by, links) = spawns_of(&now);
        let (retail, retail_links) = spawns_of(content());
        let lock = s.compiled().lock.clone();
        for (model, lb, ..) in HOUSES {
            let e = catalog().get(DataId(model)).unwrap();
            let sites = sites(model);
            let house = e.layouts[0].house.as_ref().unwrap();
            let name = s.compiled().dropped[&lb][0].0.clone();
            let bare = name.split_once(':').unwrap().1.to_owned();
            let templates: Vec<Template> = sites
                .iter()
                .map(|site| {
                    Template::read(&read, *site, None, dereth_primitives::EraId::Eor).unwrap()
                })
                .collect();
            let info_now = s.statics(lb).unwrap();
            let cells = s.cells(lb);
            let root = house.objects[house.root().unwrap()].member;
            let guid = |k: usize| {
                lock.get(
                    Sym::Guid,
                    &format!("holtburg:{}@{lb:04X}", drops::house_name(&bare, k, root)),
                )
                .unwrap()
            };
            // Each building's rooms belong to its own house, as each retail copy's do to its.
            let mut restricted = Vec::new();
            for (k, (t, b)) in templates.iter().zip(&info_now.buildings).enumerate() {
                let rooms = buildings::rooms(b, &cells);
                assert_eq!(rooms.len(), t.rooms.len());
                for r in &rooms {
                    assert_eq!(cells[r].restriction_obj, Some(guid(k)), "{lb:04X}/{r:04X}");
                    assert_ne!(cells[r].flags & 8, 0);
                }
                assert!(t
                    .rooms
                    .values()
                    .all(|c| c.restriction_obj == Some(house_at(t.site))));
                let o = b.frame.origin;
                restricted.push((
                    (u32::from(lb) << 16) | lyceum_foundry::edit::outdoor_cell(o.x, o.y),
                    guid(k),
                ));
            }
            // The landblock's restriction table: the land cell holding each origin, to its house.
            let table = info_now.restrictions.as_ref().unwrap();
            let mut have = table.entries.clone();
            have.sort_unstable();
            restricted.sort_unstable();
            assert_eq!(have, restricted, "{lb:04X}");
            assert_eq!(table.buckets, 8);
            assert_ne!(info_now.pack_mask & 1, 0);
            // The house the others answer to has the lowest id of them: it stands first.
            assert!((0..templates.len()).all(|k| guid(root) <= guid(k)));
            // Its basement: a copy of the retail dungeon, every cell belonging to its own house.
            let basement = house
                .basement
                .map(|template| (basement_landblock(&s, &name, lb), template));
            if let Some((b, template)) = basement {
                let want = buildings::read_cells(&read, template, dereth_primitives::EraId::Eor);
                let got = s.cells(b);
                assert_eq!(got.len(), want.len(), "{b:04X}");
                let number: BTreeMap<u16, u16> =
                    want.keys().copied().zip(got.keys().copied()).collect();
                let below = lock
                    .get(
                        Sym::Guid,
                        &format!("holtburg:{}-house@{b:04X}", drops::basement_name(&bare, lb)),
                    )
                    .unwrap();
                let retail_below = info(template).restrictions.unwrap().entries[0].1;
                for (n, c) in &want {
                    let g = &got[&number[n]];
                    assert_eq!(
                        (g.environment, g.cell_struct, &g.surfaces),
                        (c.environment, c.cell_struct, &c.surfaces)
                    );
                    assert!(same_frame(&g.frame, &c.frame));
                    assert_eq!(g.static_objects, c.static_objects);
                    assert_eq!(
                        g.visible_cells,
                        c.visible_cells
                            .iter()
                            .map(|v| number[v])
                            .collect::<Vec<_>>()
                    );
                    assert_eq!(g.portals.len(), c.portals.len());
                    for (p, q) in g.portals.iter().zip(&c.portals) {
                        #[allow(clippy::cast_possible_truncation)]
                        let other = number[&(q.other_cell_id as u16)];
                        assert_eq!(p.other_cell_id, u32::from(other));
                    }
                    assert_eq!(g.restriction_obj, Some(below));
                    assert_eq!(c.restriction_obj, Some(retail_below));
                }
                let t = s.statics(b).unwrap();
                assert!(t.buildings.is_empty());
                assert_eq!(t.restrictions.unwrap().entries, vec![(0, below)]);
                assert_eq!(s.terrain(b).unwrap().lbi_exists, 1);
            }
            // Every spawn of the retail house stands with the copy as it stands with the retail
            // building: the same weenie (a house its own new class), in the room copied from its
            // room or the land cell outside, at the same place in its building's frame.
            let class = lock
                .get(
                    Sym::Wcid,
                    &format!("holtburg:{}", drops::house_class(&bare, lb)),
                )
                .unwrap();
            let first = u32::from(lb) << 16;
            let room_of = |n: u16| -> Option<u32> {
                templates
                    .iter()
                    .zip(&info_now.buildings)
                    .find_map(|(t, b)| {
                        let pos = t.rooms.keys().position(|r| *r == n)?;
                        let rooms: Vec<u16> = buildings::rooms(b, &cells).into_iter().collect();
                        Some(first | u32::from(rooms[pos]))
                    })
            };
            let retail_lb = sites[0].landblock;
            let frame_of = |k: usize| info(retail_lb).buildings[sites[k].index].frame;
            let below_of = |g: &u32| retail[g].cell >> 16 != u32::from(retail_lb);
            let mut matched: BTreeMap<u32, u32> = BTreeMap::new();
            let want = retail_house(&sites, house.basement);
            for (g, k) in &want {
                let r = &retail[g];
                let weenie = if kind(r.weenie) == HOUSE {
                    class
                } else {
                    r.weenie
                };
                #[allow(clippy::cast_possible_truncation)]
                let low = (r.cell & 0xFFFF) as u16;
                // A link spot stands where the other side's house portal does (checked below).
                let is_spot = retail_links
                    .iter()
                    .any(|(p, cs)| cs.contains(g) && kind(retail[p].weenie) == PORTAL);
                let frame = if below_of(g) {
                    r.frame
                } else {
                    carried(&r.frame, &frame_of(*k), &info_now.buildings[*k].frame)
                };
                let cell = match basement {
                    Some((b, template)) if below_of(g) => {
                        let want =
                            buildings::read_cells(&read, template, dereth_primitives::EraId::Eor);
                        let got = s.cells(b);
                        let pos = want.keys().position(|c| *c == low).unwrap();
                        (u32::from(b) << 16) | u32::from(*got.keys().nth(pos).unwrap())
                    }
                    _ if low >= 0x100 => room_of(low).unwrap(),
                    _ => first | lyceum_foundry::edit::outdoor_cell(frame.origin.x, frame.origin.y),
                };
                let found = by.iter().find(|(h, n)| {
                    !matched.values().any(|m| m == *h)
                        && n.weenie == weenie
                        && n.child == r.child
                        && if kind(r.weenie) == BOOT_SPOT {
                            n.cell >> 16 == cell >> 16
                        } else if is_spot {
                            n.cell == cell
                        } else {
                            n.cell == cell && same_frame(&n.frame, &frame)
                        }
                });
                let (h, n) = found.unwrap_or_else(|| {
                    panic!(
                        "{lb:04X}: no spawn of {} ({}) in {cell:#010X} at {:?}",
                        r.weenie,
                        kind(r.weenie),
                        frame.origin
                    )
                });
                matched.insert(*g, *h);
                // A boot spot stands off the house's land, on the ground.
                if kind(r.weenie) == BOOT_SPOT {
                    let o = n.frame.origin;
                    for b in &info_now.buildings {
                        assert_ne!(
                            buildings::land_cell(o.x, o.y),
                            buildings::land_cell(b.frame.origin.x, b.frame.origin.y)
                        );
                    }
                    let ground = s.ground(lb, o.x, o.y).unwrap();
                    assert!(
                        (o.z - ground).abs() < 0.01,
                        "{lb:04X}: boot spot {o:?} over {ground}"
                    );
                }
            }
            assert_eq!(matched.len(), want.len());
            // A link spot stands where the other side's house portal stands, and the house portals'
            // rows are each portal's own place, by the new house's id.
            let portal = |below: bool| {
                want.iter()
                    .map(|(g, _)| *g)
                    .find(|g| kind(retail[g].weenie) == PORTAL && below_of(g) == below)
            };
            if let (Some(outside), Some(inside)) = (portal(false), portal(true)) {
                for (g, other) in [(outside, inside), (inside, outside)] {
                    let spot = links[&matched[&g]]
                        .iter()
                        .map(|c| &by[c])
                        .find(|c| kind(c.weenie) != PORTAL)
                        .unwrap();
                    assert!(same_frame(&spot.frame, &by[&matched[&other]].frame));
                }
                let rows: Vec<&(u32, u32, Frame)> =
                    portals.iter().filter(|p| p.0 == class).collect();
                assert_eq!(rows.len(), 2);
                for g in [outside, inside] {
                    let p = &by[&matched[&g]];
                    assert!(rows
                        .iter()
                        .any(|r| r.1 == p.cell && same_frame(&r.2, &p.frame)));
                }
            } else {
                assert!(portals.iter().all(|p| p.0 != class));
            }
            // The same links between them, and no more.
            let new_house: BTreeSet<u32> = matched.values().copied().collect();
            let mut want_links: BTreeSet<(u32, u32)> = BTreeSet::new();
            for (p, _) in &want {
                for c in retail_links.get(p).into_iter().flatten() {
                    if let Some(c) = matched.get(c) {
                        want_links.insert((matched[p], *c));
                    }
                }
            }
            let have_links: BTreeSet<(u32, u32)> = new_house
                .iter()
                .flat_map(|p| links.get(p).into_iter().flatten().map(move |c| (*p, *c)))
                .collect();
            assert_eq!(have_links, want_links, "{lb:04X}");
            // Its house weenie: the retail house's, with its own house id, named for it.
            let mut rows = now.houses[&class].clone();
            let mut retail_rows = content().houses[&house.class].clone();
            let id = |r: &mut Vec<sql_ids::HouseRow>| {
                let k = r
                    .iter()
                    .position(|r| r.table == "d_i_d" && r.kind == 42)
                    .unwrap();
                r.remove(k).value
            };
            assert_eq!(sql_ids::number(&id(&mut rows)), Some(class));
            let _ = id(&mut retail_rows);
            rows.sort_by(|a, b| (&a.table, a.kind).cmp(&(&b.table, b.kind)));
            retail_rows.sort_by(|a, b| (&a.table, a.kind).cmp(&(&b.table, b.kind)));
            assert_eq!(rows, retail_rows);
            assert_eq!(now.weenies[&class].kind, Some(HOUSE));
            let named =
                s.compiled().sql.values().any(|t| {
                    t.contains(&format!("'house{}{class}'", e.kind.name().to_lowercase()))
                });
            assert!(named, "its class is named by its house id");
        }
        // The checks find no fault, and the session's world is what a build makes.
        let overlay = dir.path().join("overlay");
        s.write_scratch(&overlay).unwrap();
        let faults: Vec<_> = s
            .check(&overlay)
            .unwrap()
            .into_iter()
            .filter(|f| !f.warning)
            .collect();
        assert!(faults.is_empty(), "{faults:#?}");
        assert_saved_builds_the_same(&mut s);
        // Undone, the world is as it was.
        for _ in HOUSES {
            s.undo().unwrap();
        }
        assert_eq!(s.compiled().records, before);
    }

    /// A house of `model` dropped as a house players can buy into `lb` at `(x, y)` turned `heading`,
    /// with its drop's name, in full.
    fn drop_house(s: &mut Session, model: u32, lb: u16, x: f32, y: f32, heading: f32) -> String {
        let (op, warnings) = s
            .drop_building_as(
                DataId(model),
                0,
                &lyceum_foundry::source::Furnished::Template,
                true,
                lb,
                x,
                y,
                heading,
            )
            .unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        let before = s.compiled().dropped.get(&lb).map_or(0, Vec::len);
        s.apply(&op).unwrap();
        s.compiled().dropped[&lb][before].0.clone()
    }

    /// The spawns the session's SQL places in landblock `lb`, by guid.
    fn spawns_in(s: &Session, lb: u16) -> BTreeMap<u32, Spawn> {
        let (by, _) = spawns_of(&placed(s).0);
        by.into_iter()
            .filter(|(g, _)| (g >> 12) & 0xFFFF == u32::from(lb))
            .collect()
    }

    #[test]
    fn a_dropped_house_moved_takes_its_house_its_land_cell_and_its_portals_with_it() {
        let (_dir, mut s) = session("houses-moved");
        let (lb, x, y, heading) = (0xA0A8, 84.0, 84.0, 90.0);
        let name = drop_house(&mut s, VILLA, lb, x, y, heading);
        let index = s.compiled().dropped[&lb][0].1;
        let b = basement_landblock(&s, &name, lb);
        let was = s.statics(lb).unwrap().buildings[index].frame;
        let before: BTreeMap<u32, Spawn> = spawns_in(&s, lb);
        let below_before = spawns_in(&s, b);
        // Two land cells west.
        let op = s
            .move_building(
                lb,
                index,
                [x - 48.0, y, was.origin.z],
                Rotation::Heading(heading),
            )
            .unwrap();
        s.apply(&op).unwrap();
        let now = s.statics(lb).unwrap().buildings[index].frame;
        let after = spawns_in(&s, lb);
        assert_eq!(
            after.keys().collect::<Vec<_>>(),
            before.keys().collect::<Vec<_>>()
        );
        let (_, links) = spawns_of(&placed(&s).0);
        for (g, a) in &after {
            let p = &before[g];
            assert_eq!((a.weenie, a.child), (p.weenie, p.child));
            let spot = links
                .iter()
                .any(|(q, cs)| cs.contains(g) && kind(after[q].weenie) == PORTAL);
            if kind(a.weenie) == BOOT_SPOT {
                assert_ne!(
                    buildings::land_cell(a.frame.origin.x, a.frame.origin.y),
                    buildings::land_cell(now.origin.x, now.origin.y)
                );
            } else if !spot {
                assert!(
                    same_frame(&a.frame, &carried(&p.frame, &was, &now)),
                    "{g:08X} at {:?}, was {:?}",
                    a.frame.origin,
                    p.frame.origin
                );
            }
        }
        // The basement's spawns stay, but the link spot of its portal, which goes where the portal
        // above now stands.
        let below_after = spawns_in(&s, b);
        let portal_above = after
            .values()
            .find(|v| kind(v.weenie) == PORTAL)
            .unwrap()
            .frame;
        for (g, a) in &below_after {
            let p = &below_before[g];
            if links
                .iter()
                .any(|(q, cs)| cs.contains(g) && kind(below_after[q].weenie) == PORTAL)
            {
                assert!(same_frame(&a.frame, &portal_above));
            } else {
                assert!(same_frame(&a.frame, &p.frame));
            }
        }
        // The land cell it stands in now belongs to its house, and the one it left to nothing.
        let entries = s.statics(lb).unwrap().restrictions.unwrap().entries;
        let cell = |f: &Frame| {
            (u32::from(lb) << 16) | lyceum_foundry::edit::outdoor_cell(f.origin.x, f.origin.y)
        };
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, cell(&now));
        assert!(entries.iter().all(|(c, _)| *c != cell(&was)));
        assert_saved_builds_the_same(&mut s);
    }

    #[test]
    fn a_dropped_house_taken_away_takes_its_house_and_its_basement_with_it() {
        let (_dir, mut s) = session("houses-removed");
        let before = s.compiled().records.clone();
        let sql_before = s.compiled().sql.clone();
        let lb = 0xA0A8;
        let name = drop_house(&mut s, VILLA, lb, 84.0, 84.0, 90.0);
        let b = basement_landblock(&s, &name, lb);
        assert!(!s.cells(b).is_empty());
        let index = s.compiled().dropped[&lb][0].1;
        s.apply(&Op::RemoveBuilding {
            landblock: lb,
            index,
        })
        .unwrap();
        assert!(s.cells(b).is_empty(), "the basement goes with it");
        assert!(!s.compiled().dropped.contains_key(&lb));
        assert_eq!(s.compiled().sql, sql_before);
        assert_eq!(s.compiled().records, before);
        assert!(s.packs()[s.target()]
            .edits
            .iter()
            .all(|e| e.dungeon_copies.is_empty() && e.drops.is_empty()));
    }

    #[test]
    fn a_house_dropped_as_a_plain_building_belongs_to_no_one() {
        let (_dir, mut s) = session("houses-plain");
        let lb = HOUSES[0].1;
        let (op, _) = s
            .drop_building_as(
                DataId(COTTAGE),
                0,
                &lyceum_foundry::source::Furnished::Template,
                false,
                lb,
                61.31,
                79.8,
                180.0,
            )
            .unwrap();
        s.apply(&op).unwrap();
        let info = s.statics(lb).unwrap();
        assert!(info.restrictions.is_none());
        assert!(s.cells(lb).values().all(|c| c.restriction_obj.is_none()));
        assert!(spawns_in(&s, lb)
            .values()
            .all(|v| !matches!(kind(v.weenie), HOUSE | SLUMLORD | HOOK | STORAGE)));
        assert!(s.packs()[s.target()]
            .edits
            .iter()
            .all(|e| e.drops.iter().all(|d| d.item.house.is_empty())));
    }

    #[test]
    fn a_landblock_holds_one_mansion_players_can_buy() {
        let (_dir, mut s) = session("houses-mansions");
        let lb = 0xA9AD;
        drop_house(&mut s, MANSION, lb, 132.0, 108.0, 0.0);
        let plan = s
            .drop_plan_as(DataId(MANSION), 0, true, lb, 36.0, 36.0, 0.0)
            .unwrap();
        assert_eq!(
            plan.refusal.as_deref(),
            Some("The landblock has a mansion players can buy already, and holds only one.")
        );
        let plain = s
            .drop_plan_as(DataId(MANSION), 0, false, lb, 36.0, 36.0, 0.0)
            .unwrap();
        assert_eq!(plain.refusal, None);
    }
}

/// A drop or a move levels its ground without changing the ground of the buildings around it.
mod neighbors {
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::OnceLock;

    use dereth_primitives::frame::localtoglobal;
    use dereth_primitives::DataId;
    use lyceum_foundry::buildings::{self, Catalog};
    use lyceum_foundry::compile;
    use lyceum_foundry::drops::{Template, MOST_LIFT};
    use lyceum_foundry::edit::{Op, Session, VertexSet};
    use lyceum_foundry::source::{Furnished, Rotation};

    /// The most placed cottage.
    const COTTAGE: u32 = 0x0100_2228;

    fn catalog() -> &'static Catalog {
        static C: OnceLock<Catalog> = OnceLock::new();
        C.get_or_init(|| {
            dereth_dat::testing::require_dats();
            let set = lyceum_foundry::dats::DatSet::open(&dereth_dat::testing::dat_dir()).unwrap();
            Catalog::read(&set, None, &|_| None)
        })
    }

    fn session(tag: &str) -> (dereth_dat::testing::ScratchDir, Session) {
        let (dir, world) = crate::sample::sample(tag);
        let mut s = Session::open(&world, &dereth_dat::testing::dat_dir(), None).unwrap();
        s.set_today("2026-10-22 00:00:00");
        s.learn_buildings(catalog().clone());
        (dir, s)
    }

    fn table(s: &Session) -> Vec<f32> {
        dereth_terrain::land::mesh::height_table(s.region().unwrap()).to_vec()
    }

    /// The level a drop at `(x, y)` of `lb` takes from its own ground: the height table's entry
    /// nearest the ground there.
    fn natural(s: &Session, lb: u16, x: f32, y: f32) -> u8 {
        lyceum_foundry::brush::nearest_height(&table(s), s.ground(lb, x, y).unwrap())
    }

    /// The corners of the land cells (world grid) under the shells of the buildings at `places` of
    /// landblock `lb`, each with its height index now.
    fn ground_under(s: &Session, lb: u16, places: &[usize]) -> BTreeMap<(u32, u32), u8> {
        let info = s.statics(lb).unwrap();
        let mut models = lyceum_foundry::pick::Models::default();
        let mut out = BTreeMap::new();
        for i in places {
            let b = &info.buildings[*i];
            let m = models.get(s.base(), b.id).unwrap();
            for p in m.triangles.iter().flatten() {
                let g = localtoglobal(&b.frame, *p);
                let (cx, cy) = buildings::land_cell(g.x, g.y);
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let gx = u32::try_from(i32::from(lb >> 8) * 8 + cx + dx).unwrap();
                    let gy = u32::try_from(i32::from(lb & 0xFF) * 8 + cy + dy).unwrap();
                    out.insert((gx, gy), s.vertex(gx, gy).unwrap().height);
                }
            }
        }
        out
    }

    /// A cottage dropped in land cell `(cx, cy)` of `lb`, where its example stands in its own:
    /// its origin, heading, and the step.
    fn cottage_at(s: &Session, lb: u16, cx: u8, cy: u8) -> (f32, f32, f32, Result<Op, String>) {
        let (px, py) = (f32::from(cx) * 24.0 + 12.0, f32::from(cy) * 24.0 + 12.0);
        let (x, y, h) = s.drop_snap(DataId(COTTAGE), 0, px, py, 0).unwrap();
        let op = s
            .drop_building(DataId(COTTAGE), 0, &Furnished::Template, lb, x, y, h)
            .map(|(op, _)| op)
            .map_err(|e| e.to_string());
        (x, y, h, op)
    }

    /// Retail buildings standing on level land cells, each with a free land cell beside it whose far
    /// corners are a step lower: its landblock, its place there and that land cell.
    const LEVEL: [(u16, usize, (u8, u8)); 8] = [
        (0x1135, 0, (1, 3)),
        (0x137D, 1, (5, 3)),
        (0x1748, 2, (5, 5)),
        (0x2172, 0, (3, 2)),
        (0x25E1, 0, (1, 4)),
        (0x26EC, 0, (4, 5)),
        (0x10BB, 0, (5, 5)),
        (0x2384, 0, (4, 3)),
    ];

    #[test]
    fn a_drop_beside_one_of_the_worlds_own_buildings_takes_the_level_of_its_land_and_leaves_it_be()
    {
        let (_dir, mut s) = session("neighbors-world");
        let heights = table(&s);
        // The first of them where the cottage's own ground would level it otherwise.
        let found = LEVEL.iter().find_map(|(lb, index, (cx, cy))| {
            let (x, y, h, op) = cottage_at(&s, *lb, *cx, *cy);
            let theirs = ground_under(&s, *lb, &[*index]);
            let level = *theirs.values().next()?;
            (op.is_ok() && theirs.values().all(|v| *v == level) && natural(&s, *lb, x, y) != level)
                .then(|| (*lb, *index, x, y, h, op.unwrap(), theirs, level))
        });
        let (lb, index, x, y, _, op, theirs, level) =
            found.expect("a retail building to drop beside");
        s.apply(&op).unwrap();
        // Its ground is just as it was, and the drop's is at its level.
        assert_eq!(ground_under(&s, lb, &[index]), theirs);
        let drop = s.compiled().dropped[&lb][0].1;
        let mine = ground_under(&s, lb, &[drop]);
        assert!(
            mine.keys().any(|v| theirs.contains_key(v)),
            "the two share corners"
        );
        assert!(mine.values().all(|v| *v == level), "{mine:?} at {level}");
        let b = &s.statics(lb).unwrap().buildings[drop];
        let t = Template::read(
            &|id| s.base().cell.read(DataId(id)).ok(),
            catalog().get(DataId(COTTAGE)).unwrap().layouts[0].example,
            s.region(),
            s.base().era,
        )
        .unwrap();
        assert!((b.frame.origin.z - heights[usize::from(level)] - t.height()).abs() < 2e-3);
        assert!((b.frame.origin.x - x).abs() < 1e-3 && (b.frame.origin.y - y).abs() < 1e-3);
    }

    /// Retail buildings whose land cell's edge toward a free land cell is not level: its landblock,
    /// its place there and that land cell.
    const TILTED: [(u16, usize, (u8, u8)); 6] = [
        (0x0408, 0, (5, 5)),
        (0x040A, 0, (4, 3)),
        (0x0604, 0, (3, 4)),
        (0x0B09, 0, (6, 3)),
        (0x0503, 0, (2, 1)),
        (0x070F, 0, (5, 2)),
    ];

    #[test]
    fn a_drop_that_would_tilt_the_ground_under_the_building_next_door_is_refused() {
        let (_dir, s) = session("neighbors-tilt");
        let refused = TILTED.iter().find_map(|(lb, _, (cx, cy))| {
            let (px, py) = (f32::from(*cx) * 24.0 + 12.0, f32::from(*cy) * 24.0 + 12.0);
            let (x, y, h) = s.drop_snap(DataId(COTTAGE), 0, px, py, 0).unwrap();
            let plan = s.drop_plan(DataId(COTTAGE), 0, *lb, x, y, h).ok()?;
            let why = plan.refusal?;
            why.contains("tilt").then(|| {
                // A step refuses it the same way.
                let step = s
                    .drop_building(DataId(COTTAGE), 0, &Furnished::Template, *lb, x, y, h)
                    .unwrap_err()
                    .to_string();
                (why, step, plan.taken)
            })
        });
        let (why, step, taken) = refused.expect("a tilted neighbor");
        assert!(
            why.starts_with("It would tilt the ground under the ") && why.ends_with(" next door."),
            "{why}"
        );
        assert_eq!(step, why);
        assert_eq!(taken, None, "no land cell is taken: the ground is why");
    }

    /// The land cells of `lb` (none holding a building) in which a cottage drops and levels its
    /// ground, with a free land cell east or west of it whose own ground is at another level but
    /// within reach: the first's land cell, and the second's.
    fn neighbors(s: &Session, lb: u16) -> ((u8, u8), (u8, u8)) {
        for cx in 2..6u8 {
            for cy in 1..7u8 {
                let (x, y, _, a) = cottage_at(s, lb, cx, cy);
                if a.is_err() {
                    continue;
                }
                for nx in [cx + 1, cx - 1] {
                    let (bx, by, _, b) = cottage_at(s, lb, nx, cy);
                    let (na, nb) = (natural(s, lb, x, y), natural(s, lb, bx, by));
                    let rise = (table(s)[usize::from(na)] - s.ground(lb, bx, by).unwrap()).abs();
                    if b.is_ok() && na != nb && rise < MOST_LIFT - 0.5 {
                        return ((cx, cy), (nx, cy));
                    }
                }
            }
        }
        panic!("no two land cells of {lb:04X} side by side at two levels");
    }

    #[test]
    fn a_drop_beside_another_takes_its_level_and_a_move_beside_it_too() {
        let (_dir, mut s) = session("neighbors-drops");
        let lb = 0x9C9C;
        let (first, _) = neighbors(&s, lb);
        let (.., op) = cottage_at(&s, lb, first.0, first.1);
        s.apply(&op.unwrap()).unwrap();
        let a = s.compiled().dropped[&lb][0].1;
        let theirs = ground_under(&s, lb, &[a]);
        let level = *theirs.values().next().unwrap();
        assert!(theirs.values().all(|v| *v == level));
        // A land cell beside it where its own ground, as the first left it, would level a cottage
        // otherwise, and the land cell past that one.
        let (spot, away) = [(1i8, 0i8), (-1, 0), (0, 1), (0, -1)]
            .into_iter()
            .find_map(|(dx, dy)| {
                let cell = |k: i8| {
                    let x = first.0.checked_add_signed(dx * k)?;
                    let y = first.1.checked_add_signed(dy * k)?;
                    (x < 8 && y < 8).then_some((x, y))
                };
                let (spot, away) = (cell(1)?, cell(2)?);
                let (x, y, _, op) = cottage_at(&s, lb, spot.0, spot.1);
                let (.., far) = cottage_at(&s, lb, away.0, away.1);
                (op.is_ok() && far.is_ok() && natural(&s, lb, x, y) != level)
                    .then_some((spot, away))
            })
            .expect("a land cell beside the first whose own ground is at another level");
        // Dropped there, at the first's level: the first's ground stands.
        let (bx, by, _, op) = cottage_at(&s, lb, spot.0, spot.1);
        s.apply(&op.unwrap()).unwrap();
        let b = s.compiled().dropped[&lb][1].1;
        assert_eq!(ground_under(&s, lb, &[a]), theirs);
        let mine = ground_under(&s, lb, &[b]);
        assert!(mine.keys().any(|v| theirs.contains_key(v)));
        assert!(mine.values().all(|v| *v == level));
        // Taken away and dropped again a land cell on, then moved back beside the first: the move
        // takes its level too, and the first's ground stands.
        s.apply(&Op::RemoveBuilding {
            landblock: lb,
            index: b,
        })
        .unwrap();
        assert_eq!(ground_under(&s, lb, &[a]), theirs);
        let (.., op) = cottage_at(&s, lb, away.0, away.1);
        s.apply(&op.unwrap()).unwrap();
        let b = s.compiled().dropped[&lb][1].1;
        let f = s.statics(lb).unwrap().buildings[b].frame;
        let op = s
            .move_building(
                lb,
                b,
                [bx, by, f.origin.z],
                Rotation::Quat([f.rotation.w, f.rotation.x, f.rotation.y, f.rotation.z]),
            )
            .unwrap();
        s.apply(&op).unwrap();
        assert_eq!(ground_under(&s, lb, &[a]), theirs);
        let b = s.compiled().dropped[&lb][1].1;
        assert!(ground_under(&s, lb, &[b]).values().all(|v| *v == level));
    }

    #[test]
    fn a_drop_that_would_stand_too_far_from_its_ground_on_the_level_of_its_neighbor_is_refused() {
        let (_dir, mut s) = session("neighbors-far");
        let lb = 0x9C9C;
        let (first, second) = neighbors(&s, lb);
        let (.., op) = cottage_at(&s, lb, first.0, first.1);
        s.apply(&op.unwrap()).unwrap();
        let a = s.compiled().dropped[&lb][0].1;
        let theirs = ground_under(&s, lb, &[a]);
        let level = *theirs.values().next().unwrap();
        // The far edge of the land cell beside it raised well over the first's level.
        let (gx, gy) = (
            u32::from(lb >> 8) * 8 + u32::from(second.0),
            u32::from(lb & 0xFF) * 8 + u32::from(second.1),
        );
        let far = if second.0 > first.0 { gx + 1 } else { gx };
        let high = u16::from(level) + 40;
        s.apply(&Op::SetVertices(
            [gy, gy + 1]
                .into_iter()
                .map(|y| VertexSet {
                    gx: far,
                    gy: y,
                    field: compile::Field::Height,
                    value: high,
                })
                .collect(),
        ))
        .unwrap();
        let (.., op) = cottage_at(&s, lb, second.0, second.1);
        let why = op.unwrap_err();
        assert!(
            why.starts_with("It would stand ")
                && why.contains(" m below the ground here, on the level of the cottage next door."),
            "{why}"
        );
        assert_eq!(ground_under(&s, lb, &[a]), theirs);
    }

    /// Every corner a drop's ground op sets is a corner no other building's land cells hold.
    #[test]
    fn a_drops_ground_sets_no_corner_of_another_buildings_land() {
        let (_dir, s) = session("neighbors-corners");
        for (lb, index, (cx, cy)) in LEVEL.iter().chain(&TILTED) {
            let (.., op) = cottage_at(&s, *lb, *cx, *cy);
            let Ok(Op::Batch(ops)) = op else { continue };
            let theirs: BTreeSet<(u32, u32)> =
                ground_under(&s, *lb, &[*index]).into_keys().collect();
            for o in &ops {
                if let Op::SetVertices(v) = o {
                    for v in v.iter().filter(|v| v.field == compile::Field::Height) {
                        assert!(
                            !theirs.contains(&(v.gx, v.gy)),
                            "{lb:04X}: the drop sets {},{} of building {index}'s land",
                            v.gx,
                            v.gy
                        );
                    }
                }
            }
        }
    }
}

/// A building of the February 2005 files dropped into an Infiltration world comes whole: its rooms
/// are cells of that layout, and the overlay passes the checks.
#[test]
fn a_february_2005_building_drops_whole_in_its_own_layout() {
    let (_scratch, mut s) = super::pretod::session("classic-drop");
    let content = super::pretod::world_sql()
        .map(|p| lyceum_foundry::sql_ids::server_content(&std::fs::read_to_string(p).unwrap()));
    let catalog = Catalog::read(s.base(), content.as_ref(), &|_| None);
    assert!(catalog.len() > 100, "{} building models", catalog.len());
    s.learn_buildings(catalog);
    // The most placed building that stands alone, dropped on Holtburg's open west side.
    let model = s
        .buildings()
        .unwrap()
        .gallery()
        .into_iter()
        .find(|e| e.set.is_empty() && !e.layouts.is_empty())
        .map(|e| e.model)
        .unwrap();
    let (op, _) = s
        .drop_building_as(
            model,
            0,
            &Furnished::Template,
            false,
            0xA8B4,
            84.0,
            84.0,
            0.0,
        )
        .unwrap();
    s.apply(&op).unwrap();
    let info = s.statics(0xA8B4).unwrap();
    assert_eq!(info.buildings.len(), 1);
    assert_eq!(info.buildings[0].id, model);
    let cells = s.cells(0xA8B4);
    assert!(!cells.is_empty(), "the building's rooms");
    assert_eq!(u32::try_from(cells.len()).unwrap(), info.num_cells);
    super::pretod::check(&s);
}
