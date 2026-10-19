//! Behaviour: none (tooling: what server SQL says of a weenie, the category it files the weenie's model under, and the tags kept beside a world)

use dereth_dat::testing::ScratchDir;
use dereth_primitives::DataId;
use lyceum_foundry::models::{Category, Tags};
use lyceum_foundry::sql_ids::{weenie_facts, WeenieFacts};

/// A dump's rows: no column lists, the tables' own; bools as a dump writes its bits.
const DUMP: &str = "\
CREATE TABLE `weenie` (`class_Id` int, `class_Name` varchar(100), `type` int, `last_Modified` datetime);
INSERT INTO `weenie` VALUES (5,'guard',10,'2005-02-09 10:00:00'),(6,'statue',1,'2005-02-09 10:00:00');
CREATE TABLE `weenie_properties_bool` (`id` int, `object_Id` int, `type` smallint, `value` bit(1));
INSERT INTO `weenie_properties_bool` VALUES (1,5,1,'\u{1}'),(2,5,19,'\\0'),(3,6,1,'\u{1}');
CREATE TABLE `weenie_properties_string` (`id` int, `object_Id` int, `type` smallint, `value` text);
INSERT INTO `weenie_properties_string` VALUES (4,5,1,'Holtburg Guard'),(5,6,1,'Statue of Asheron\\'s Ward');
CREATE TABLE `weenie_properties_d_i_d` (`id` int, `object_Id` int, `type` smallint, `value` int);
INSERT INTO `weenie_properties_d_i_d` VALUES (6,5,1,33554433),(7,5,2,150994945),(8,6,1,33556000);
";

#[test]
fn a_dumps_weenie_rows_say_its_name_setup_type_and_flags() {
    let f = weenie_facts(DUMP);
    assert_eq!(
        f.get("5"),
        Some(&WeenieFacts {
            name: Some("Holtburg Guard".into()),
            setup: Some(0x0200_0001),
            kind: Some(10),
            stuck: Some(true),
            attackable: Some(false),
            generator_radius: None,
            destination: None,
        })
    );
    assert_eq!(
        f.get("6"),
        Some(&WeenieFacts {
            name: Some("Statue of Asheron's Ward".into()),
            setup: Some(33_556_000),
            kind: Some(1),
            stuck: Some(true),
            attackable: None,
            generator_radius: None,
            destination: None,
        })
    );
    assert_eq!(f.len(), 2);
}

#[test]
fn a_weenie_is_filed_by_its_type_and_a_creature_that_cannot_be_attacked_is_an_npc() {
    let w = |kind: u32, stuck: Option<bool>, attackable: Option<bool>| WeenieFacts {
        kind: Some(kind),
        stuck,
        attackable,
        ..WeenieFacts::default()
    };
    let of = Category::of_weenie;
    assert_eq!(of(&w(10, Some(true), Some(true))), Category::Creature);
    assert_eq!(of(&w(10, Some(true), None)), Category::Creature);
    assert_eq!(of(&w(10, Some(true), Some(false))), Category::Npc);
    assert_eq!(of(&w(12, None, None)), Category::Npc);
    assert_eq!(of(&w(19, None, None)), Category::Door);
    assert_eq!(of(&w(7, None, None)), Category::Portal);
    assert_eq!(of(&w(59, None, None)), Category::Portal);
    assert_eq!(of(&w(20, None, None)), Category::Chest);
    assert_eq!(of(&w(25, None, None)), Category::Lifestone);
    assert_eq!(of(&w(26, None, None)), Category::Object);
    assert_eq!(of(&w(1, Some(true), None)), Category::Object);
    assert_eq!(of(&w(1, None, None)), Category::Item);
    assert_eq!(of(&w(6, None, None)), Category::Item);
}

#[test]
fn tags_are_kept_beside_the_world_file_and_read_back_with_its_comments() {
    let dir = ScratchDir::new("pack-tags").unwrap();
    let world = dir.path().join("world.toml");
    let stone = DataId(0x0200_040B);
    let tree = DataId(0x0200_02D8);
    let mut t = Tags::read(&world).unwrap();
    assert!(t.of(stone).is_empty() && t.all().is_empty());
    assert!(t.add(stone, " standing stone ").unwrap());
    assert!(!t.add(stone, "standing stone").unwrap(), "had it already");
    assert!(!t.add(stone, "  ").unwrap(), "nothing to add");
    assert!(t.add(stone, "holtburg").unwrap());
    assert!(t.add(tree, "holtburg").unwrap());
    assert_eq!(t.path(), dir.path().join(Tags::FILE));
    let text = std::fs::read_to_string(t.path()).unwrap();
    assert!(
        text.contains(r#"0x0200040B = ["standing stone", "holtburg"]"#),
        "{text}"
    );
    // A comment the user writes stays.
    std::fs::write(t.path(), format!("# mine\n{text}")).unwrap();
    let mut t = Tags::read(&world).unwrap();
    assert_eq!(t.of(stone), ["standing stone", "holtburg"]);
    assert_eq!(t.all().get("holtburg"), Some(&2));
    assert!(t.remove(stone, "holtburg").unwrap());
    assert!(!t.remove(stone, "holtburg").unwrap());
    assert!(t.remove(tree, "holtburg").unwrap());
    let text = std::fs::read_to_string(t.path()).unwrap();
    assert!(text.starts_with("# mine\n"), "{text}");
    assert!(!text.contains("0x020002D8"), "no tags, no line: {text}");
    assert_eq!(Tags::read(&world).unwrap().of(stone), ["standing stone"]);
    // A key that is not a model's id is refused.
    std::fs::write(t.path(), "[models]\nstone = [\"a\"]\n").unwrap();
    assert!(Tags::read(&world).is_err());
}

#[test]
fn a_portals_destination_is_read_from_its_position_row() {
    let f = weenie_facts(
        "INSERT INTO `weenie_properties_position` (`object_Id`, `position_Type`, `obj_Cell_Id`, \
         `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`)
VALUES (90010, 2, 0x019E0114, 40, -40, 0, 1, 0, 0, 0)
     , (90011, 1, 0x019E0100, 0, 0, 0, 1, 0, 0, 0);",
    );
    assert_eq!(
        f.get("90010").and_then(|f| f.destination),
        Some(0x019E_0114)
    );
    // Another position is not where it sends you.
    assert_eq!(f.get("90011").and_then(|f| f.destination), None);
}
