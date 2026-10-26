//! Behaviour: none (tooling: the world's quests for Lyceum's Quests tab: where each is named, and one registered again, copied, renamed or taken away)

use std::path::PathBuf;

use lyceum_foundry::edit::{Session, Weenie};
use lyceum_foundry::quests::Whose;
use lyceum_foundry::sql_ids::{QuestDoes, QuestUse};

use crate::common::Fixture;

/// A pack weenie of `name` (a creature) with `rows` after its `weenie` row.
fn weenie(name: &str, rows: &str) -> String {
    format!(
        "DELETE FROM `weenie` WHERE `class_Id` = {{{{wcid:{name}}}}};\n\n\
         INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)\n\
         VALUES ({{{{wcid:{name}}}}}, '{name}', 10, '2026-10-24 00:00:00');\n\n{rows}"
    )
}

/// The keeper: its use stamps `Gate`, asks after it and erases it, answers the test in two
/// sets (`Gate@2` the second's), and it lets only those who have it through.
fn keeper() -> String {
    weenie(
        "keeper",
        "INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({{wcid:keeper}},   1, 'Keeper') /* Name */
     , ({{wcid:keeper}},  37, 'Gate') /* QuestRestriction */;

INSERT INTO `weenie_properties_emote` (`object_Id`, `category`, `probability`, `weenie_Class_Id`, `style`, `substyle`, `quest`, `vendor_Type`, `min_Health`, `max_Health`)
VALUES ({{wcid:keeper}},  7 /* Use */, 1, NULL, NULL, NULL, NULL, NULL, NULL, NULL);

SET @parent_id = LAST_INSERT_ID();

INSERT INTO `weenie_properties_emote_action` (`emote_Id`, `order`, `type`, `delay`, `extent`, `motion`, `message`)
VALUES (@parent_id, 0, 21 /* InqQuest */, 0, 1, NULL, 'Gate')
     , (@parent_id, 1, 30 /* InqQuestSolves */, 0, 1, NULL, 'Gate@2')
     , (@parent_id, 2, 10 /* Tell */, 0, 1, NULL, 'Gate');

INSERT INTO `weenie_properties_emote` (`object_Id`, `category`, `probability`, `weenie_Class_Id`, `style`, `substyle`, `quest`, `vendor_Type`, `min_Health`, `max_Health`)
VALUES ({{wcid:keeper}}, 13 /* QuestFailure */, 1, NULL, NULL, NULL, 'Gate', NULL, NULL, NULL);

SET @parent_id = LAST_INSERT_ID();

INSERT INTO `weenie_properties_emote_action` (`emote_Id`, `order`, `type`, `delay`, `extent`, `motion`, `message`)
VALUES (@parent_id, 0, 22 /* StampQuest */, 0, 1, NULL, 'Gate');

INSERT INTO `weenie_properties_emote` (`object_Id`, `category`, `probability`, `weenie_Class_Id`, `style`, `substyle`, `quest`, `vendor_Type`, `min_Health`, `max_Health`)
VALUES ({{wcid:keeper}}, 12 /* QuestSuccess */, 1, NULL, NULL, NULL, 'Gate@2', NULL, NULL, NULL);

SET @parent_id = LAST_INSERT_ID();

INSERT INTO `weenie_properties_emote_action` (`emote_Id`, `order`, `type`, `delay`, `extent`, `motion`, `message`)
VALUES (@parent_id, 0, 31 /* EraseQuest */, 0, 1, NULL, 'Gate')
     , (@parent_id, 1, 33 /* IncrementQuest */, 0, 1, NULL, 'Gate');
",
    )
}

/// The pack's quest `Gate`, in its own file.
const GATE: &str = "DELETE FROM `quest` WHERE `name` = 'Gate';

INSERT INTO `quest` (`name`, `min_Delta`, `max_Solves`, `message`, `last_Modified`)
VALUES ('Gate', 600, 3, 'Through the gate.', '2026-10-24 00:00:00');
";

/// The server's: a portal that needs the pack's `Gate`; a keeper whose use asks after its own
/// `Bell` and answers it, and stamps `Unknown`, which nothing registers; a creature whose kill
/// counts toward `Bell`; a chest the pack writes again, which stamps `Bell` on who opens it; and
/// the quest `Bell`.
const SERVER: &str = "\
CREATE TABLE `weenie` (`class_Id` int, `class_Name` varchar(100), `type` int, `last_Modified` datetime);
INSERT INTO `weenie` VALUES (9001,'gateportal',7,'2005-02-09 10:00:00'),(9002,'bellkeeper',10,'2005-02-09 10:00:00'),(9003,'bellchest',20,'2005-02-09 10:00:00'),(9004,'bellbat',10,'2005-02-09 10:00:00');
CREATE TABLE `weenie_properties_string` (`id` int, `object_Id` int, `type` smallint, `value` text);
INSERT INTO `weenie_properties_string` VALUES (1,9001,1,'Gate Portal'),(2,9001,37,'Gate'),(3,9002,1,'Bell Keeper'),(4,9003,1,'Bell Chest'),(5,9003,33,'Bell'),(6,9004,1,'Bell Bat'),(7,9004,45,'Bell@Bats');
CREATE TABLE `weenie_properties_emote` (`id` int, `object_Id` int, `category` int, `probability` float, `weenie_Class_Id` int, `style` int, `substyle` int, `quest` text, `vendor_Type` int, `min_Health` float, `max_Health` float);
INSERT INTO `weenie_properties_emote` VALUES (70,9002,7,1,NULL,NULL,NULL,NULL,NULL,NULL,NULL),(71,9002,12,1,NULL,NULL,NULL,'Bell',NULL,NULL,NULL);
CREATE TABLE `weenie_properties_emote_action` (`id` int, `emote_Id` int, `order` int, `type` int, `delay` float, `extent` float, `motion` int, `message` text);
INSERT INTO `weenie_properties_emote_action` VALUES (80,70,0,21,0,1,NULL,'Bell'),(81,71,0,22,0,1,NULL,'Unknown');
CREATE TABLE `quest` (`id` int, `name` varchar(255), `min_Delta` int, `max_Solves` int, `message` text, `last_Modified` datetime);
INSERT INTO `quest` VALUES (1,'Bell',3600,1,'Rang the bell.','2005-02-09 10:00:00');
";

/// The chest, as the pack writes it again: it stamps nothing.
const CHEST: &str = "DELETE FROM `weenie` WHERE `class_Id` = 9003;

INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES (9003, 'bellchest', 20, '2026-10-24 00:00:00');
";

/// The world: the keeper, a drudge whose kill counts toward `Gate@Drudges`, a scroll that stamps
/// `Gate` on who picks it up, the quest `Gate` and the pack's chest; and the server's database.
fn world(tag: &str) -> (Fixture, Session) {
    let f = Fixture::new(tag);
    let world = f.world();
    f.write("sample/server/sql/weenies/keeper.sql", &keeper());
    f.write(
        "sample/server/sql/weenies/drudge.sql",
        &weenie(
            "drudge",
            "INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({{wcid:drudge}},  45, 'Gate@Drudges') /* KillQuest */;
",
        ),
    );
    f.write(
        "sample/server/sql/weenies/scroll.sql",
        &weenie(
            "scroll",
            "INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({{wcid:scroll}},  33, 'Gate') /* Quest */;
",
        ),
    );
    f.write("sample/server/sql/quests/Gate.sql", GATE);
    f.write("sample/server/sql/weenies/9003 Bell Chest.sql", CHEST);
    let mut s = Session::open(&world, &f.dats(), None).unwrap();
    s.learn_weenies(SERVER);
    (f, s)
}

fn sym(name: &str) -> Weenie {
    Weenie::Symbol(format!("sample:{name}"))
}

/// What each weenie does with quest `name`, in the order found, each once.
fn does(s: &Session, name: &str) -> Vec<(Weenie, QuestDoes)> {
    let mut out: Vec<(Weenie, QuestDoes)> = Vec::new();
    for u in s.world_quest(name).unwrap().uses {
        let d = (u.by.clone(), u.how.does());
        if !out.contains(&d) {
            out.push(d);
        }
    }
    out
}

#[test]
fn every_way_a_weenie_names_a_quest_is_found_in_the_packs_and_the_servers_database() {
    let (_f, s) = world("quests-uses");
    let gate = s.world_quest("gate").expect("the pack's");
    assert_eq!(gate.whose, Whose::Pack { mine: true });
    assert_eq!(
        (gate.quest.as_ref().unwrap().min_delta, gate.server.as_ref()),
        (600, None)
    );
    assert!(gate
        .file
        .as_ref()
        .unwrap()
        .ends_with("server/sql/quests/Gate.sql"));
    assert_eq!(
        does(&s, "Gate"),
        [
            (sym("drudge"), QuestDoes::Kills),
            (sym("keeper"), QuestDoes::Needs),
            (sym("keeper"), QuestDoes::Tests),
            (sym("keeper"), QuestDoes::Answers),
            (sym("keeper"), QuestDoes::Stamps),
            (sym("keeper"), QuestDoes::Erases),
            (sym("keeper"), QuestDoes::Changes),
            (sym("scroll"), QuestDoes::StampsWhenTaken),
            (Weenie::Class(9001), QuestDoes::Needs),
        ]
    );
    assert_eq!(gate.places(), 4);
    // A test's answering set by its category; a tell of the name is no use of it.
    let keeper: Vec<QuestUse> = gate
        .uses
        .iter()
        .filter(|u| u.by == sym("keeper"))
        .map(|u| u.how)
        .collect();
    assert!(keeper.contains(&QuestUse::Answers { category: 12 }));
    assert!(keeper.contains(&QuestUse::Action {
        category: 7,
        action: 30
    }));
    assert!(!keeper
        .iter()
        .any(|h| matches!(h, QuestUse::Action { action: 10, .. })));
    // The server's quest, named by its own: the chest the pack writes again stands for itself.
    let bell = s.world_quest("Bell").unwrap();
    assert_eq!(bell.whose, Whose::Server);
    assert_eq!(bell.server.as_ref().unwrap().min_delta, 3600);
    assert_eq!(
        does(&s, "bell"),
        [
            (Weenie::Class(9004), QuestDoes::Kills),
            (Weenie::Class(9002), QuestDoes::Answers),
            (Weenie::Class(9002), QuestDoes::Tests),
        ]
    );
    // Named by the server, registered nowhere, by the name as written.
    let unknown = s.world_quest("unknown").unwrap();
    assert_eq!(
        (unknown.whose, unknown.name.as_str()),
        (Whose::Nobody, "Unknown")
    );
    let names: Vec<String> = s.world_quests().into_iter().map(|q| q.name).collect();
    assert_eq!(names, ["Bell", "Gate", "Unknown"]);
}

#[test]
fn a_rename_writes_every_pack_file_that_names_the_quest_and_no_server_file_as_one_step() {
    let (f, mut s) = world("quests-rename");
    let pack = f.path().join("sample");
    let renamed = s.quest_rename("Gate", "Opened").unwrap();
    let files: Vec<&PathBuf> = renamed.files.iter().map(|(p, _)| p).collect();
    for file in &files {
        assert!(file.starts_with(&pack), "{}", file.display());
    }
    let named = |rel: &str| pack.join("server/sql").join(rel);
    let mut want = vec![
        named("quests/Gate.sql"),
        named("quests/Opened.sql"),
        named("weenies/drudge.sql"),
        named("weenies/keeper.sql"),
        named("weenies/scroll.sql"),
    ];
    want.sort();
    let mut got: Vec<PathBuf> = files.into_iter().cloned().collect();
    got.sort();
    assert_eq!(got, want, "the chest names no quest");
    assert!(renamed
        .files
        .iter()
        .any(|(p, t)| p.ends_with("quests/Gate.sql") && t.is_none()));
    // The server's portal names it still, and is left so.
    assert_eq!(renamed.left.len(), 1);
    assert_eq!(renamed.left[0].by, Weenie::Class(9001));
    let before = s.history().0;
    s.apply(&renamed.op()).unwrap();
    assert_eq!(s.history().0, before + 1, "one step");
    let opened = s.world_quest("Opened").expect("renamed");
    assert_eq!(opened.whose, Whose::Pack { mine: true });
    assert_eq!(opened.quest.as_ref().unwrap().max_solves, 3);
    assert_eq!(opened.places(), 3, "the pack's three");
    let kill = opened
        .uses
        .iter()
        .find(|u| u.by == sym("drudge"))
        .and_then(|u| u.file.clone())
        .unwrap();
    assert!(s.sql_text(&kill).unwrap().contains("'Opened@Drudges'"));
    // The old name is the portal's alone now, registered nowhere.
    let gate = s.world_quest("Gate").unwrap();
    assert_eq!(gate.whose, Whose::Nobody);
    assert_eq!(gate.places(), 1);
    // A server's quest is not the pack's to rename; nor is a name taken.
    assert!(s.quest_rename("Bell", "Rung").is_err());
    assert!(s.quest_rename("Opened", "bell").is_err());
    s.undo().unwrap();
    assert_eq!(
        s.world_quest("Gate").unwrap().whose,
        Whose::Pack { mine: true }
    );
    assert!(s.world_quest("Opened").is_none());
}

#[test]
fn a_servers_quest_is_registered_again_put_back_and_copied_and_a_named_quest_is_not_deleted() {
    let (_f, mut s) = world("quests-change");
    s.apply(&s.quest_change_here("Bell").unwrap()).unwrap();
    let bell = s.world_quest("Bell").unwrap();
    assert_eq!(bell.whose, Whose::Changed { mine: true });
    assert_eq!(bell.quest, bell.server, "as the server has it");
    assert!(bell.file.unwrap().ends_with("server/sql/quests/Bell.sql"));
    assert!(s.quest_change_here("Bell").is_err(), "once");
    s.apply(&s.quest_unregister("Bell", true).unwrap()).unwrap();
    assert_eq!(s.world_quest("Bell").unwrap().whose, Whose::Server);
    // A copy under a new name, which nothing names.
    s.apply(&s.quest_copy("Bell", "Bell Two").unwrap()).unwrap();
    let two = s.world_quest("bell two").unwrap();
    assert_eq!((two.whose, two.places()), (Whose::Pack { mine: true }, 0));
    assert_eq!(two.quest.unwrap().min_delta, 3600);
    assert!(s.quest_copy("Bell", "gate").is_err(), "a name taken");
    assert!(s.quest_copy("Bell", "Bell@2").is_err(), "not a name");
    // Deleted, as nothing names it; the pack's Gate is named, and stays.
    s.apply(&s.quest_unregister("Bell Two", false).unwrap())
        .unwrap();
    assert!(s.world_quest("Bell Two").is_none());
    assert!(s.quest_unregister("Gate", false).is_err());
}
