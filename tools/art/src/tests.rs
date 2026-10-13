//! Behaviour: none (tooling: the art survey)

use std::collections::BTreeMap;

use dereth_assets::motion::ObjectEffect;
use dereth_assets::ClothingTable;
use dereth_primitives::{DataId, Vec3};

use super::character::fold_clothing;
use super::model::{Collision, Facts, Group, Mesh, Vert};
use super::output::{models_tsv, report_html, summary_md, uses_tsv};
use super::spawns;
use super::survey::{parse_tags, Survey};
use super::tally::*;
use super::thumb::{draw, Material, VIEW_FROM};
use super::world::terrain_and_scene;

fn obj(id: u32) -> ModelId {
    ModelId::Object(DataId(id))
}

fn at(x: f32, y: f32) -> Sighting {
    Sighting {
        position: Some(Vec3::new(x, y, 0.0)),
        ..Sighting::default()
    }
}

#[test]
fn a_ranking_puts_the_most_used_model_first_and_breaks_ties_by_id() {
    let r = rank([
        (obj(0x0100_0003), 5),
        (obj(0x0100_0001), 9),
        (obj(0x0100_0002), 5),
        (obj(0x0100_0004), 1),
    ]);
    let ids: Vec<ModelId> = r.iter().map(|x| x.id).collect();
    assert_eq!(
        ids,
        vec![
            obj(0x0100_0001),
            obj(0x0100_0002),
            obj(0x0100_0003),
            obj(0x0100_0004)
        ]
    );
    assert!((r[0].share - 0.45).abs() < 1e-12);
    assert!((r[1].cumulative - 0.70).abs() < 1e-12);
    assert!((r[3].cumulative - 1.0).abs() < 1e-12);
    assert_eq!(total(&r), 20);
}

#[test]
fn coverage_is_the_fewest_top_ids_whose_instances_reach_the_threshold() {
    let r = rank([(obj(1), 50), (obj(2), 30), (obj(3), 15), (obj(4), 5)]);
    assert_eq!(ids_covering(&r, 50), 1);
    assert_eq!(ids_covering(&r, 80), 2);
    assert_eq!(ids_covering(&r, 81), 3);
    assert_eq!(ids_covering(&r, 95), 3);
    assert_eq!(ids_covering(&r, 96), 4);
    assert_eq!(ids_covering(&r, 100), 4);
    assert_eq!(ids_covering(&[], 50), 0);
}

#[test]
fn a_landblock_adds_every_instance_and_counts_once_toward_landblocks() {
    let mut t = Tally::default();
    let mut a = BlockTally::new(0xA9B4);
    a.see(obj(0x0200_0001), Use::Scenery, at(1.0, 2.0));
    a.see(obj(0x0200_0001), Use::Scenery, at(3.0, 4.0));
    a.see(obj(0x0200_0001), Use::Static, at(5.0, 6.0));
    t.fold(a);
    let mut b = BlockTally::new(0xA9B5);
    b.see(obj(0x0200_0001), Use::Scenery, at(7.0, 8.0));
    t.fold(b);
    let s = &t.uses[&obj(0x0200_0001)][&Use::Scenery];
    assert_eq!((s.instances, s.landblocks), (3, 2));
    let st = &t.uses[&obj(0x0200_0001)][&Use::Static];
    assert_eq!((st.instances, st.landblocks), (1, 1));
}

#[test]
fn scenery_keeps_the_terrain_it_grew_on_and_the_range_of_its_scale() {
    let mut b = BlockTally::new(1);
    for (terrain, scale) in [((3, 1), 0.8), ((3, 1), 1.3), ((5, 0), 1.0)] {
        b.see(
            obj(0x0100_0001),
            Use::Scenery,
            Sighting {
                scale: Some(scale),
                terrain: Some(terrain),
                ..Sighting::default()
            },
        );
    }
    let mut t = Tally::default();
    t.fold(b);
    let s = &t.uses[&obj(0x0100_0001)][&Use::Scenery];
    assert_eq!(s.terrain, BTreeMap::from([((3, 1), 2), ((5, 0), 1)]));
    assert_eq!(s.scale, Some((0.8, 1.3)));
}

#[test]
fn examples_are_the_landblocks_holding_the_most_with_the_first_place_seen() {
    let mut t = Tally::default();
    for (lb, n) in [
        (10u16, 1u16),
        (11, 4),
        (12, 4),
        (13, 2),
        (14, 9),
        (15, 1),
        (16, 3),
    ] {
        let mut b = BlockTally::new(lb);
        for k in 0..n {
            b.see(obj(1), Use::Static, at(f32::from(lb), f32::from(k)));
        }
        t.fold(b);
    }
    let ex = &t.uses[&obj(1)][&Use::Static].examples;
    let got: Vec<(u16, u64)> = ex.iter().map(|e| (e.landblock, e.count)).collect();
    assert_eq!(got, vec![(14, 9), (11, 4), (12, 4), (16, 3), (13, 2)]);
    assert_eq!(ex[0].position, Some(Vec3::new(14.0, 0.0, 0.0)));
}

fn blocks() -> Vec<BlockTally> {
    (0u16..12)
        .map(|lb| {
            let mut b = BlockTally::new(lb);
            for k in 0..=(lb % 4) {
                b.see(
                    obj(0x0200_0000 + u32::from(k)),
                    Use::Scenery,
                    Sighting {
                        position: Some(Vec3::new(f32::from(lb), 0.0, 0.0)),
                        scale: Some(f32::from(lb) * 0.1),
                        terrain: Some((u8::try_from(lb % 3).unwrap(), 0)),
                        ..Sighting::default()
                    },
                );
            }
            b
        })
        .collect()
}

#[test]
fn tallies_merge_to_the_same_counts_however_the_landblocks_were_split() {
    let mut whole = Tally::default();
    for b in blocks() {
        whole.fold(b);
    }
    let (mut odd, mut even) = (Tally::default(), Tally::default());
    for b in blocks() {
        if b.landblock % 2 == 1 {
            odd.fold(b);
        } else {
            even.fold(b);
        }
    }
    let mut merged = Tally::default();
    merged.merge(odd);
    merged.merge(even);
    assert_eq!(merged.uses, whole.uses);
}

#[test]
fn a_piece_is_drawn_with_the_surfaces_of_its_lowest_numbered_cell() {
    let p = ModelId::Piece {
        environment: DataId(0x0D00_0001),
        cell_struct: 2,
    };
    let mut t = Tally::default();
    t.piece_cell(p, 0x0007_0105, &[DataId(0x0800_0002)]);
    t.piece_cell(p, 0x0007_0101, &[DataId(0x0800_0001)]);
    t.piece_cell(p, 0x0007_0109, &[DataId(0x0800_0003)]);
    assert_eq!(
        t.piece_surfaces[&p],
        (0x0007_0101, vec![DataId(0x0800_0001)])
    );
}

#[test]
fn model_ids_read_back_from_their_text() {
    let p = ModelId::Piece {
        environment: DataId(0x0D00_012C),
        cell_struct: 3,
    };
    assert_eq!(p.text(), "0x0D00012C/3");
    assert_eq!(p.file_stem(), "0x0D00012C-3");
    assert_eq!(ModelId::parse("0x0D00012C/3"), Some(p));
    assert_eq!(ModelId::parse("02000AF1"), Some(obj(0x0200_0AF1)));
    assert_eq!(obj(0x0200_0AF1).kind(), Kind::Setup);
    assert_eq!(obj(0x0100_0AF1).kind(), Kind::GfxObj);
    assert_eq!(p.kind(), Kind::Piece);
    assert_eq!(ModelId::parse("rock"), None);
}

#[test]
fn a_vertex_word_holds_its_terrain_type_and_scene_type() {
    // Road bits 0-1, terrain type bits 2-6, scene type bits 11-15.
    let word: u16 = (7 << 11) | (19 << 2) | 0b11;
    assert_eq!(terrain_and_scene(word), (19, 7));
}

const SPAWNS: &str = "setup\ttype\tspawns\tlandblocks\tweenies\twhere\n\
0x02000001\tcreature\t12\t3\t1:drudge:creature:10;2:guard:npc:2\tA9B4:7 0102:4 C6A9:1\n\
0x02000002\tdoor\t1\t1\t3:door:door:1\tA9B4:1\n";

#[test]
fn the_spawn_table_becomes_each_setups_spawn_use() {
    let rows = spawns::parse(SPAWNS).expect("the table reads");
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0].weenies[1],
        (2, "guard".to_owned(), "npc".to_owned(), 2)
    );
    assert_eq!(spawns::landblocks(&rows), 3);
    let mut t = Tally::default();
    spawns::fold(&mut t, &rows);
    let s = &t.uses[&obj(0x0200_0001)][&Use::Spawn];
    assert_eq!((s.instances, s.landblocks), (12, 3));
    let ex: Vec<(u16, u64)> = s.examples.iter().map(|e| (e.landblock, e.count)).collect();
    assert_eq!(ex, vec![(0xA9B4, 7), (0x0102, 4), (0xC6A9, 1)]);
    assert_eq!(s.notes["type creature"], 12);
    assert_eq!(s.notes["2 guard (npc)"], 2);
}

#[test]
fn a_malformed_spawn_row_is_refused_with_its_line() {
    let e = spawns::parse("0x02000001\tcreature\tmany\t1\t\tA9B4:1\n").unwrap_err();
    assert!(e.starts_with("line 1"), "{e}");
    assert!(spawns::parse("0x02000001\tcreature\t1\n").is_err());
    assert!(spawns::parse("0x02000001\tcreature\t1\t1\t\tA9B4\n").is_err());
}

#[test]
fn tags_are_read_by_id_and_a_bad_id_names_its_line() {
    let tags =
        parse_tags("id\ttags\n# sorted by hand\n0x02000001\trock, granite\n0x0D000001/2\tfloor\n")
            .expect("the tags read");
    assert_eq!(tags[&obj(0x0200_0001)], "rock, granite");
    assert_eq!(tags.len(), 2);
    assert!(parse_tags("rock\tstone\n")
        .unwrap_err()
        .starts_with("line 1"));
}

#[test]
fn a_models_total_counts_the_world_and_leaves_out_the_character_uses() {
    let mut t = Tally::default();
    t.stats_mut(obj(1), Use::Scenery).instances = 5;
    t.stats_mut(obj(1), Use::Spawn).instances = 2;
    t.stats_mut(obj(2), Use::Clothing).instances = 40;
    t.stats_mut(obj(3), Use::DungeonPiece).instances = 6;
    let r = t.ranked_total();
    let got: Vec<(ModelId, u64)> = r.iter().map(|x| (x.id, x.count)).collect();
    assert_eq!(got, vec![(obj(1), 7), (obj(3), 6)]);
}

#[test]
fn a_clothing_table_counts_once_for_each_part_it_swaps_in() {
    let effect = |part, id| ObjectEffect {
        part_num: part,
        object_id: DataId(id),
        texture_effects: Vec::new(),
    };
    let table = ClothingTable {
        id: DataId(0x1000_0001),
        clothing_base_buckets: 0,
        clothing_bases: BTreeMap::from([
            (
                DataId(0x0200_0001),
                vec![effect(9, 0x0100_0010), effect(10, 0x0100_0011)],
            ),
            (DataId(0x0200_004E), vec![effect(9, 0x0100_0010)]),
        ]),
        palette_template_buckets: 0,
        palette_templates: BTreeMap::new(),
    };
    let mut t = Tally::default();
    fold_clothing(&mut t, &table);
    fold_clothing(&mut t, &table);
    let s = &t.uses[&obj(0x0100_0010)][&Use::Clothing];
    assert_eq!(s.instances, 2);
    assert_eq!(s.notes["part 9 of 0x02000001"], 2);
    assert_eq!(s.notes["part 9 of 0x0200004E"], 2);
}

/// A square of side 2 through the origin facing `normal`'s way, as two triangles.
fn square(normal: [f32; 3], two_sided: bool) -> Mesh {
    let n = normal;
    // Two axes across the square, perpendicular to the normal.
    let a = if n[2].abs() < 0.9 {
        [n[1], -n[0], 0.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let b = [
        n[1] * a[2] - n[2] * a[1],
        n[2] * a[0] - n[0] * a[2],
        n[0] * a[1] - n[1] * a[0],
    ];
    let corner = |s: f32, t: f32| Vert {
        position: [
            a[0] * s + b[0] * t,
            a[1] * s + b[1] * t,
            a[2] * s + b[2] * t,
        ],
        normal: n,
        uv: [0.0, 0.0],
    };
    let (p, q, r, s) = (
        corner(-1.0, -1.0),
        corner(1.0, -1.0),
        corner(1.0, 1.0),
        corner(-1.0, 1.0),
    );
    Mesh {
        groups: vec![Group {
            surface: None,
            two_sided,
            triangles: vec![[p, q, r], [p, r, s]],
        }],
    }
}

fn coverage(pic: &super::thumb::Picture) -> usize {
    pic.rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[3] > 0)
        .count()
}

#[test]
fn a_picture_shows_what_faces_the_viewer_and_hides_what_faces_away() {
    let toward = VIEW_FROM;
    let away = [-toward[0], -toward[1], -toward[2]];
    let mut grey = |_: Option<DataId>| None;
    let front = draw(&square(toward, false), &mut grey, 32, true);
    assert_eq!(front.rgba.len(), 32 * 32 * 4);
    // Fitted to the picture by its box, the square covers a good part of it.
    assert!(coverage(&front) > 32 * 32 / 4, "{}", coverage(&front));
    assert!(draw(&square(away, false), &mut grey, 32, true).is_blank());
    assert!(!draw(&square(away, true), &mut grey, 32, true).is_blank());
    assert!(!draw(&square(away, false), &mut grey, 32, false).is_blank());
}

#[test]
fn a_picture_draws_a_surface_in_its_colour_and_leaves_out_a_hidden_one() {
    let mesh = square(VIEW_FROM, false);
    let mut red = |_: Option<DataId>| Some(Material::Colour([0, 0, 255, 255]));
    let pic = draw(&mesh, &mut red, 16, true);
    let i = (8 * 16 + 8) * 4;
    let centre = &pic.rgba[i..i + 4];
    assert!(
        centre[0] > 100 && centre[1] == 0 && centre[2] == 0,
        "{centre:?}"
    );
    assert_eq!(centre[3], 255);
    let mut hidden = |_: Option<DataId>| Some(Material::Hidden);
    assert!(draw(&mesh, &mut hidden, 16, true).is_blank());
}

#[test]
fn a_model_with_nothing_to_draw_gives_a_blank_picture() {
    let mut grey = |_: Option<DataId>| None;
    let pic = draw(&Mesh::default(), &mut grey, 8, true);
    assert!(pic.is_blank());
    assert_eq!(pic.rgba.len(), 8 * 8 * 4);
    assert!(pic.png().expect("encodes").starts_with(b"\x89PNG"));
}

fn small_survey() -> Survey {
    let mut t = Tally::default();
    for (lb, n) in [(0xA9B4u16, 3u32), (0xA9B5, 1)] {
        let mut b = BlockTally::new(lb);
        for _ in 0..n {
            b.see(obj(0x0200_0001), Use::Scenery, at(1.0, 1.0));
        }
        b.see(obj(0x0100_0002), Use::Static, at(2.0, 2.0));
        t.fold(b);
    }
    t.world.landblocks = 2;
    t.world.scenery_landblocks = 2;
    t.world.static_landblocks = 2;
    let facts = BTreeMap::from([(
        obj(0x0200_0001),
        Facts {
            found: true,
            bounds: Some(([-1.0, -1.0, 0.0], [1.0, 1.0, 4.0])),
            triangles: 12,
            parts: vec![DataId(0x0100_0005)],
            textures: Vec::new(),
            solid_surfaces: 0,
            collision: Collision::Cylinders(vec![(0.5, 4.0)]),
        },
    )]);
    Survey {
        tally: t,
        facts,
        thumbnails: BTreeMap::from([(obj(0x0200_0001), "thumbs/0x02000001.png".to_owned())]),
        tags: BTreeMap::from([(obj(0x0200_0001), "tree </script>".to_owned())]),
        characters: super::character::CharacterCounts::default(),
        spawn_rows: 0,
        spawn_landblocks: 0,
        landblocks_walked: 2,
        terrain_names: Vec::new(),
        seconds: 1.0,
    }
}

#[test]
fn the_tables_hold_one_row_per_model_and_one_per_model_and_use() {
    let s = small_survey();
    let models = models_tsv(&s);
    let lines: Vec<&str> = models.lines().collect();
    assert_eq!(lines.len(), 3);
    assert!(lines[0].starts_with("id\tkind\trank\ttotal\tuses\t"));
    let tree: Vec<&str> = lines[2].split('\t').collect();
    assert_eq!(&tree[..5], &["0x02000001", "setup", "1", "4", "scenery:4"]);
    assert_eq!(tree[13], "cylinder");
    assert_eq!(tree[15], "thumbs/0x02000001.png");
    let uses = uses_tsv(&s);
    assert_eq!(uses.lines().count(), 3);
    assert!(
        uses.contains("0x02000001\tsetup\tscenery\t1\t4\t1.000000\t1.000000\t2\t"),
        "{uses}"
    );
}

#[test]
fn the_summary_reports_each_uses_ids_instances_and_coverage() {
    let md = summary_md(&small_survey());
    assert!(
        md.contains("| Procedural scenery | 1 | 4 | 2 | 1 | 1 | 1 |"),
        "{md}"
    );
    assert!(
        md.contains("| Landblock statics | 1 | 2 | 2 | 1 | 1 | 1 |"),
        "{md}"
    );
    assert!(
        md.contains("| 1 | `0x02000001` | setup | 4 | 100.0% | 100.0% | 2 | 12 |"),
        "{md}"
    );
}

#[test]
fn the_report_page_carries_its_data_without_ending_its_own_script() {
    let html = report_html(&small_survey());
    assert!(html.starts_with("<!doctype html>"));
    assert!(!html.contains("/*DATA*/null"));
    assert!(html.contains("\"id\":\"0x02000001\""));
    assert!(html.contains("tree <\\/script>"));
    assert_eq!(html.matches("</script>").count(), 1);
}

/// A few real landblocks around Holtburg and the first dungeon block: the client grows scenery
/// there, places statics and buildings, the buildings have rooms behind their portals and the
/// dungeon block's cells are a dungeon's. The counts do not depend on the thread count.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn surveying_real_landblocks_counts_scenery_statics_buildings_and_cells() {
    let store = dereth_dat::testing::open_store_or_fail();
    let region = dereth_world_data::landblock::load_region(&store).expect("the region");
    let blocks = [0xA9B3, 0xA9B4, 0xAAB4, 0x0007];
    let one = super::world::walk(&store, &region, &blocks, 1, &|_| {});
    let four = super::world::walk(&store, &region, &blocks, 4, &|_| {});
    assert_eq!(one.uses, four.uses);
    assert_eq!(one.world, four.world);
    let w = one.world;
    assert_eq!(w.landblocks, 4);
    assert!(w.scenery_landblocks >= 2, "{w:?}");
    assert!(w.buildings > 0 && w.interior_cells > 0, "{w:?}");
    assert_eq!(w.dungeons, 1, "{w:?}");
    assert!(w.dungeon_cells > 0, "{w:?}");
    assert_eq!(w.cells_unloaded, 0, "{w:?}");
    for u in [
        Use::Scenery,
        Use::Static,
        Use::Building,
        Use::DungeonPiece,
        Use::InteriorPiece,
    ] {
        assert!(total(&one.ranked(u)) > 0, "{u:?}");
    }
    // Every building counts the rooms behind its portals.
    let rooms: u64 = one
        .uses
        .values()
        .filter_map(|u| u.get(&Use::Building))
        .map(|s| s.interior_cells)
        .sum();
    assert_eq!(rooms, w.interior_cells);
    // Every model the walk found reads, and the pieces draw with their cell's surfaces.
    let mut models = super::model::Models::new(&store);
    for id in one.uses.keys() {
        let surfaces = one
            .piece_surfaces
            .get(id)
            .map_or(&[][..], |(_, s)| s.as_slice());
        let (f, mesh) = models.facts(*id, surfaces, &mut |_| Some((1, 1)));
        assert!(f.found, "{}", id.text());
        assert_eq!(f.triangles, mesh.triangles());
    }
}
