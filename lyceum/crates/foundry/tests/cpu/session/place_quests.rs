//! Behaviour: none (tooling: the quests a place checks, for a playtest character's flags: its portals' and its emotes', and the portals into a dungeon)

use lyceum_foundry::edit::Session;
use lyceum_foundry::sql_ids::server_content;

use crate::common::{Fixture, HOME};

/// The dungeon the portal leads into.
const DUNGEON: u16 = 0x01AB;

/// A portal into the dungeon that lets through only those with `PortalFlag`, an NPC whose use
/// asks after `NpcFlag@Asked` (and stamps another, which is no check), both standing in `HOME`;
/// and a gem that costs 4 billion to use. As a dump writes them.
const SERVER: &str = "\
CREATE TABLE `weenie` (`class_Id` int, `class_Name` varchar(100), `type` int, `last_Modified` datetime);
INSERT INTO `weenie` VALUES (9001,'testportal',7,'2005-02-09 10:00:00'),(9002,'testnpc',10,'2005-02-09 10:00:00'),(29267,'gemaugmentationbonusxp',67,'2005-02-09 10:00:00');
CREATE TABLE `weenie_properties_string` (`id` int, `object_Id` int, `type` smallint, `value` text);
INSERT INTO `weenie_properties_string` VALUES (1,9001,1,'Test Portal'),(2,9001,37,'PortalFlag'),(3,9002,1,'Test Keeper'),(4,29267,1,'Quick Learner');
CREATE TABLE `weenie_properties_int64` (`id` int, `object_Id` int, `type` smallint, `value` bigint);
INSERT INTO `weenie_properties_int64` VALUES (5,29267,3,4000000000);
CREATE TABLE `weenie_properties_position` (`id` int, `object_Id` int, `position_Type` smallint, `obj_Cell_Id` int, `origin_X` float, `origin_Y` float, `origin_Z` float, `angles_W` float, `angles_X` float, `angles_Y` float, `angles_Z` float);
INSERT INTO `weenie_properties_position` VALUES (6,9001,2,27984128,10,10,0,1,0,0,0);
CREATE TABLE `weenie_properties_emote` (`id` int, `object_Id` int, `category` int, `probability` float, `weenie_Class_Id` int, `style` int, `substyle` int, `quest` text, `vendor_Type` int, `min_Health` float, `max_Health` float);
INSERT INTO `weenie_properties_emote` VALUES (70,9002,7,1,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
CREATE TABLE `weenie_properties_emote_action` (`id` int, `emote_Id` int, `order` int, `type` int, `delay` float, `extent` float, `motion` int, `message` text);
INSERT INTO `weenie_properties_emote_action` VALUES (80,70,0,21,0,1,NULL,'NpcFlag@Asked'),(81,70,1,22,0,1,NULL,'StampedNotChecked');
CREATE TABLE `landblock_instance` (`guid` int, `weenie_Class_Id` int, `obj_Cell_Id` int, `origin_X` float, `origin_Y` float, `origin_Z` float, `angles_W` float, `angles_X` float, `angles_Y` float, `angles_Z` float, `is_Link_Child` bit(1), `last_Modified` datetime);
INSERT INTO `landblock_instance` VALUES (1879113728,9001,16842777,84,4,94,1,0,0,0,'\\0','2019-05-16 00:00:00'),(1879113729,9002,16842777,86,5,94,1,0,0,0,'\\0','2019-05-16 00:00:00');
";

#[test]
fn a_dump_says_which_quest_a_portal_needs_which_its_emotes_test_and_what_a_gem_costs() {
    let c = server_content(SERVER);
    assert_eq!(
        c.weenies[&9001].quest_restriction.as_deref(),
        Some("PortalFlag")
    );
    assert_eq!(c.weenies[&9002].tests_quests, ["NpcFlag@Asked"]);
    assert_eq!(c.weenies[&29267].augmentation_cost, Some(4_000_000_000));
    assert_eq!(c.weenies[&9001].destination.map(|d| d >> 16), Some(0x01AB));
}

#[test]
fn a_place_checks_the_quests_of_its_portals_and_emotes_and_a_dungeon_those_of_its_way_in() {
    let f = Fixture::new("place-quests");
    let mut s = Session::open(&f.world(), &f.dats(), None).unwrap();
    s.learn_weenies(SERVER);
    s.show_server_spawns([HOME]);
    let here: std::collections::BTreeSet<u16> = [HOME].into();
    let found = s.quests_checked_in(&here, None);
    assert_eq!(
        found,
        [
            ("PortalFlag".to_owned(), "Test Portal".to_owned()),
            ("NpcFlag".to_owned(), "Test Keeper".to_owned())
        ],
        "a stamp is no check, and a quest's comment is not its name"
    );
    // Inside the dungeon, nothing stands; the portal that leads in is still its check.
    let inside: std::collections::BTreeSet<u16> = [DUNGEON].into();
    let found = s.quests_checked_in(&inside, Some(DUNGEON));
    assert_eq!(found, [("PortalFlag".to_owned(), "Test Portal".to_owned())]);
    assert!(s.quests_checked_in(&inside, None).is_empty());
}
