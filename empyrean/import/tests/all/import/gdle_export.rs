//! Vectors: tests/fixtures/patches/base.sql and a local server quest list
//! `empyrean-import gdle` writes the quests SQL content adds or changes as GDLE's quests.json,
//! merged into a server's own list when one is named, and takes the entry of a quest the content
//! deletes out of that list.
//! Fixture: tests/fixtures/patches/base.sql and locally constructed content and quest lists.

use std::path::PathBuf;
use std::process::Command;

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("empyrean-gdle-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn keys(list: &serde_json::Value) -> Vec<String> {
    list.as_array()
        .expect("a list")
        .iter()
        .map(|e| e["key"].as_str().expect("a key").to_owned())
        .collect()
}

/// The dump has one quest. Content that changes it and adds another, and writes a third as the
/// dump has it, writes the two as quests.json; named a server's list, quests.json is that list
/// with the changed quest's entry in place of the server's and the new one after.
#[test]
fn the_quests_content_adds_or_changes_are_merged_into_a_servers_quest_list() {
    let dir = temp_dir("quests");
    let dump = crate::patch_fixtures().join("base.sql");
    let patches = dir.join("patches");
    std::fs::create_dir_all(&patches).unwrap();
    std::fs::write(
        patches.join("quests.sql"),
        "DELETE FROM `quest` WHERE `name` = 'TestQuest';\n\
         INSERT INTO `quest` (`name`, `min_Delta`, `max_Solves`, `message`) VALUES ('TestQuest', 60, 2, 'Tested');\n\
         INSERT INTO `quest` (`name`, `min_Delta`, `max_Solves`, `message`) VALUES ('NewQuest', 5, 1, 'New');\n",
    )
    .unwrap();
    let run = |out: &PathBuf, quests: Option<&PathBuf>| {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_empyrean-import"));
        cmd.arg("gdle")
            .arg("--sql")
            .arg(&dump)
            .arg("--patches")
            .arg(&patches);
        if let Some(q) = quests {
            cmd.arg("--quests").arg(q);
        }
        let done = cmd.arg("--out").arg(out).output().unwrap();
        assert!(
            done.status.success(),
            "{}",
            String::from_utf8_lossy(&done.stderr)
        );
        let text = std::fs::read_to_string(out.join("quests.json")).unwrap();
        serde_json::from_str::<serde_json::Value>(&text).unwrap()
    };

    let alone = run(&dir.join("alone"), None);
    assert_eq!(keys(&alone), ["NewQuest", "TestQuest"]);
    assert_eq!(alone[1]["value"]["mindelta"], 60);

    let host = dir.join("server-quests.json");
    std::fs::write(
        &host,
        r#"[
  {"key": "testquest", "value": {"fullname": "The server's", "mindelta": 3600, "maxsolves": -1}},
  {"key": "ServerQuest", "value": {"fullname": "Kept", "mindelta": 10, "maxsolves": 4}}
]"#,
    )
    .unwrap();
    let merged = run(&dir.join("merged"), Some(&host));
    assert_eq!(keys(&merged), ["TestQuest", "ServerQuest", "NewQuest"]);
    assert_eq!(merged[0]["value"]["fullname"], "Tested");
    assert_eq!(merged[1]["value"]["fullname"], "Kept");

    // Content that writes the quest as the dump has it changes nothing.
    std::fs::write(
        patches.join("quests.sql"),
        "DELETE FROM `quest` WHERE `name` = 'TestQuest';\n\
         INSERT INTO `quest` (`name`, `min_Delta`, `max_Solves`, `message`) VALUES ('TestQuest', 3600, -1, NULL);\n",
    )
    .unwrap();
    let same = run(&dir.join("same"), Some(&host));
    assert_eq!(keys(&same), ["testquest", "ServerQuest"]);
    assert_eq!(same[0]["value"]["fullname"], "The server's");
    assert!(
        !dir.join("same/quests-removed.json").exists(),
        "nothing deleted"
    );

    // Content that deletes the quest: its entry goes from the server's list, and the quests the
    // content deletes are named beside it.
    std::fs::write(
        patches.join("quests.sql"),
        "DELETE FROM `quest` WHERE `name` = 'TestQuest';
",
    )
    .unwrap();
    let gone = run(&dir.join("gone"), Some(&host));
    assert_eq!(keys(&gone), ["ServerQuest"]);
    let removed: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("gone/quests-removed.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(removed, serde_json::json!(["TestQuest"]));
    let alone = run(&dir.join("gone-alone"), None);
    assert_eq!(keys(&alone), Vec::<String>::new());
    assert!(dir.join("gone-alone/quests-removed.json").is_file());
    let _ = std::fs::remove_dir_all(&dir);
}
