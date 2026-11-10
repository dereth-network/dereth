//! Behaviour: none (checks the world builder's inspector fields: every field of a placed row shown, each read as its row writes it and set as the row reads it)

use athenaeum_foundry::fields::{self, Control, ELSEWHERE};
use athenaeum_foundry::Session;
use yalain_world_db::layer::Base;
use yalain_world_db::rows::{Gate, Row, FIELDS};

use crate::{dir, store, world_files};

const PORTAL: &str = "object s-5c@77B8";
const CAMP: &str = "camp c-4586@77B8";

/// A base whose places include one of a row of like rooms.
fn with_a_room_row() -> Session {
    let mut files = crate::base_files();
    let places = files.get_mut("seed/server/places.toml").unwrap();
    places.push_str(
        r#"
[[place]]
key = "barracks-2"
cell = 0x77B80105
at = [10.0, 12.0, 0.5]
row = "barracks"
holds = 0x77B80106
prov = { from = "capture", evidence = "TEST-PLACES", ref = "p:2", confidence = "high" }
"#,
    );
    let base = Base::new(&files).expect("the base reads");
    let world = world_files();
    Session::new(dir(), Box::new(store(&world)), world, base, None).expect("a session")
}

fn row(s: &Session, item: &str) -> Row {
    s.entry(item).expect("the row").row.row.clone()
}

/// Every field the format gives an object, a camp, a place or a clearing is one the inspector
/// shows, or one with a control of its own; and the inspector shows no field the format lacks.
#[test]
fn every_field_of_a_placed_row_is_shown_by_the_inspector_or_a_control_of_its_own() {
    let s = with_a_room_row();
    let mut seen = 0;
    for item in [PORTAL, CAMP, "place barracks-2"] {
        let r = row(&s, item);
        let kind = r.kind();
        let format = FIELDS.iter().find(|(k, _)| *k == kind).unwrap().1;
        let shown: Vec<&str> = fields::of(&r).iter().map(|f| f.name).collect();
        for f in format {
            assert!(
                shown.contains(f) || ELSEWHERE.contains(f),
                "[[{kind}]] {f} is not shown"
            );
        }
        for f in &shown {
            assert!(format.contains(f), "[[{kind}]] has no field {f}");
        }
        seen += 1;
    }
    // A clearing's shape is a radius or a size, each shown for its own shape.
    let format = FIELDS.iter().find(|(k, _)| *k == "clearing").unwrap().1;
    let shown: Vec<&str> = fields::ROUND_CLEARING
        .iter()
        .chain(fields::RECTANGLE_CLEARING)
        .map(|f| f.name)
        .collect();
    for f in format {
        assert!(
            shown.contains(f) || ELSEWHERE.contains(f),
            "[[clearing]] {f}"
        );
    }
    assert_eq!(seen, 3);
}

/// What is typed for a field of words needs no quotes; the rest is TOML as it is typed.
#[test]
fn words_are_typed_without_their_quotes_and_the_rest_as_toml() {
    let s = with_a_room_row();
    let portal = row(&s, PORTAL);
    assert_eq!(fields::toml(&portal, "to", "inn"), "\"inn\"");
    assert_eq!(fields::toml(&portal, "to", "\"inn\""), "\"inn\"");
    assert_eq!(fields::toml(&portal, "to", " "), "");
    assert_eq!(fields::toml(&portal, "min_level", " 20 "), "20");
    assert_eq!(fields::toml(&portal, "name_text", "Old Tom"), "\"Old Tom\"");
    assert_eq!(
        fields::toml(&portal, "name_text", "Tom \"Red\" Lee"),
        "\"Tom \\\"Red\\\" Lee\""
    );
    assert_eq!(fields::text(&portal, "to"), "p-1", "shown without quotes");
    assert_eq!(fields::text(&portal, "template"), "0x4700115B");
    assert_eq!(
        fields::text(&portal, "kind"),
        "\"static\"",
        "a choice's value"
    );
    assert_eq!(fields::text(&portal, "hero"), "", "left out");
    let kind = fields::find(&portal, "kind").unwrap();
    assert!(matches!(
        kind.control,
        Control::Choice {
            optional: false,
            ..
        }
    ));
}

/// A portal's gates are set as the inspector writes them, read back as the server reads them,
/// and taken away.
#[test]
fn a_portals_gates_are_set_read_back_and_taken_away() {
    let mut s = with_a_room_row();
    let quest = 0x8000_01D5;
    for gate in [Gate::Done, Gate::UnderWay, Gate::Phase(3), Gate::Had] {
        s.set(PORTAL, "requires", &fields::gate_toml(quest, gate))
            .unwrap();
        let Row::Object(o) = row(&s, PORTAL) else {
            panic!("an object")
        };
        assert_eq!(o.requires, Some((quest, gate)));
    }
    s.set(PORTAL, "min_level", "20").unwrap();
    s.set(PORTAL, "hero", "true").unwrap();
    s.set(PORTAL, "kingdom", fields::KINGDOMS[1].0).unwrap();
    let r = row(&s, PORTAL);
    let Row::Object(o) = &r else {
        panic!("an object")
    };
    assert_eq!(
        (o.min_level, o.hero, o.kingdom),
        (Some(20), true, Some(2)),
        "{o:?}"
    );
    assert_eq!(fields::text(&r, "hero"), "true");
    assert_eq!(fields::text(&r, "kingdom"), "2");
    for f in ["requires", "hero", "kingdom"] {
        s.set(PORTAL, f, "").unwrap();
    }
    let Row::Object(o) = row(&s, PORTAL) else {
        panic!("an object")
    };
    assert_eq!((o.requires, o.hero, o.kingdom), (None, false, None));
    assert!(s.problems().is_empty(), "{:?}", s.problems());
}

/// A camp's name is a string of the install, set as its two ids and read back; how its creatures
/// move is one of the format's words, or their template's when left out.
#[test]
fn a_camps_name_and_how_its_creatures_move_are_set_and_read_back() {
    let mut s = with_a_room_row();
    s.set(CAMP, "name", &fields::name_toml(0x2500_2AFC, 0x2824_938C))
        .unwrap();
    s.set(CAMP, "movement", fields::MOVEMENTS[1].0).unwrap();
    s.set(CAMP, "wanders", "false").unwrap();
    let r = row(&s, CAMP);
    let Row::Camp(c) = &r else { panic!("a camp") };
    let name = c.name.expect("named");
    assert_eq!((name.table, name.string), (0x2500_2AFC, 0x2824_938C));
    assert_eq!(c.moves.kind.map(|k| k.word()), Some("turn-only"));
    assert_eq!(c.moves.wanders, Some(false));
    assert_eq!(
        fields::text(&r, "name"),
        "{ table = 0x25002AFC, string = 0x2824938C }"
    );
    assert!(s.problems().is_empty(), "{:?}", s.problems());
}

/// A place of a row of like rooms, changed in the world, keeps its row and the cell its creature
/// stands in.
#[test]
fn a_place_of_a_row_of_rooms_changed_keeps_its_row() {
    let mut s = with_a_room_row();
    s.set("place barracks-2", "destination", "\"dng_barracks\"")
        .unwrap();
    let r = row(&s, "place barracks-2");
    let Row::Place(p) = &r else { panic!("a place") };
    assert_eq!(p.destination.as_deref(), Some("dng_barracks"));
    assert_eq!(p.row.as_deref(), Some("barracks"));
    assert_eq!(p.holds, Some(0x77B8_0106));
    assert_eq!(fields::text(&r, "row"), "barracks");
    assert_eq!(fields::text(&r, "holds"), "0x77B80106");
}

/// Every field the format gives a kind of pet and a skill's limit is one the Pets window shows,
/// or the one that names its row.
#[test]
fn every_field_of_a_kind_of_pet_is_shown() {
    let mut files = crate::base_files();
    files.insert(
        "seed/server/pets.toml".into(),
        "[[pet]]\neffect = 0x7000092A\ntemplate = 0x47000862\nstays = true\nprov = { from = \"authored\" }\n\n[[pet_limit]]\nskill = 704\nlimits = [{ most = 1, classes = 0x1000 }]\nprov = { from = \"authored\" }\n".into(),
    );
    let base = Base::new(&files).expect("the base reads");
    let world = world_files();
    let s = Session::new(dir(), Box::new(store(&world)), world, base, None).expect("a session");
    assert_eq!(s.pets().len(), 2);
    for l in s.pets() {
        let kind = l.row.kind();
        let format = FIELDS.iter().find(|(k, _)| *k == kind).unwrap().1;
        let shown: Vec<&str> = fields::of(&l.row).iter().map(|f| f.name).collect();
        for f in format {
            assert!(
                shown.contains(f) || ELSEWHERE.contains(f),
                "[[{kind}]] {f} is not shown"
            );
        }
        for f in &shown {
            assert!(format.contains(f), "[[{kind}]] has no field {f}");
        }
    }
    let pet = s.pets().iter().find(|l| l.row.kind() == "pet").unwrap();
    assert_eq!(fields::text(&pet.row, "stays"), "true");
    assert_eq!(fields::text(&pet.row, "template"), "0x47000862");
}
