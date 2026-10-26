//! Behaviour: none (tooling: Lyceum's Characters tab beyond the build: augmentations, luminance, titles and quests, and the quests a place checks)

use lyceum_editor::characters::{AugmentView, BuildView, CharacterTab};

use crate::world::dungeon::keep;
use crate::{editor_with, gpu_lock, sample};

/// A retail dungeon of level-200 monsters whose way in lets through only those with a flag.
#[cfg(feature = "live-server-tests")]
const VAULT: u16 = 0x0073;

/// The flag its portal asks for.
#[cfg(feature = "live-server-tests")]
const VAULT_FLAG: &str = "blackferahsvault_start";

#[test]
fn the_tab_prices_augmentations_by_the_worlds_gems_and_spends_them_from_the_build() {
    use std::collections::BTreeMap;
    let _g = gpu_lock();
    let (_dir, world) = sample("character-extras-costs");
    let sql = std::env::var_os("EMPYREAN_WORLD_SQL").map(std::path::PathBuf::from);
    let mut more = vec!["--size", "1600x1000"];
    let sql_text;
    if let Some(p) = &sql {
        sql_text = p.display().to_string();
        more.extend(["--weenies", sql_text.as_str()]);
    }
    let mut ed = editor_with(&world, &more);
    if sql.is_some() {
        ed.wait_for_names().unwrap();
    }
    ed.open_characters();
    for _ in 0..3 {
        assert!(ed.frame());
    }
    ed.new_character("Augmented").unwrap();
    ed.characters.recipe.level = 240;
    let plain = ed.characters.build().cloned().unwrap().unwrap();
    ed.characters.recipe.augmentations = BTreeMap::from([("quick-learner".to_owned(), 1)]);
    let priced = ed.characters.build().cloned().unwrap();
    if sql.is_none() {
        // Before the world database is read, a gem's cost is not known.
        assert!(priced.is_err(), "{priced:?}");
        return;
    }
    let priced = priced.unwrap();
    assert_eq!(priced.augmentation_xp, 4_000_000_000, "the world's gem");
    assert!(priced.unspent_xp + priced.spent_xp() < plain.unspent_xp + plain.spent_xp());
    let found = ed.characters.findings().unwrap();
    assert!(found.iter().all(|f| f.warning), "{found:?}");
    // Titles come from the data files.
    ed.extras_findings();
    let titles = ed.characters.extras.titles.clone().unwrap();
    assert!(titles.len() > 100, "{}", titles.len());
    ed.characters.recipe.titles = vec![u32::MAX];
    assert!(ed.extras_findings().iter().any(|f| !f.warning));
    ed.characters.recipe.titles = vec![titles[0].0];
    assert!(ed.extras_findings().iter().all(|f| f.warning));
    // The augmentations, then the titles and quests.
    ed.characters.show(
        CharacterTab::Augments,
        BuildView::Attributes,
        AugmentView::Augmentations,
    );
    for _ in 0..3 {
        assert!(ed.frame());
    }
    keep(&mut ed, "character-extras", "01-augmentations.png");
    ed.characters.tab = CharacterTab::Quests;
    for _ in 0..3 {
        assert!(ed.frame());
    }
    keep(&mut ed, "character-extras", "02-quests.png");
}

#[cfg(feature = "live-server-tests")]
#[test]
fn in_a_level_200_vault_a_character_with_augmentations_auras_and_the_flag_its_way_in_asks_plays_as_made(
) {
    use dereth_progression::extras::Luminous;
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    let _g = gpu_lock();
    let (dir, world) = sample("live-character-extras");
    let server =
        PathBuf::from(std::env::var_os("EMPYREAN_SERVER_BIN").expect("EMPYREAN_SERVER_BIN"));
    let sql = PathBuf::from(std::env::var_os("EMPYREAN_WORLD_SQL").expect("EMPYREAN_WORLD_SQL"));
    let mut ed = editor_with(
        &world,
        &[
            "--empyrean",
            server.parent().unwrap().to_str().unwrap(),
            "--weenies",
            sql.to_str().unwrap(),
            "--size",
            "1600x1000",
        ],
    );
    ed.wait_for_names().unwrap();
    ed.enter_dungeon(VAULT).unwrap();
    ed.open_characters();
    ed.new_character("Sand Diver").unwrap();
    // The place's checks are gathered with its monsters.
    let start = std::time::Instant::now();
    let place = loop {
        assert!(ed.frame());
        if let Some(q) = &ed.characters.extras.place {
            if !q.is_empty() {
                break q.clone();
            }
        }
        // Only against a hang: the place's quests arriving is what ends the wait.
        assert!(
            start.elapsed() < std::time::Duration::from_secs(60),
            "the place's quests"
        );
    };
    println!("the place checks {place:?}");
    assert!(
        place.iter().any(|(q, _)| q == VAULT_FLAG),
        "its way in: {place:?}"
    );
    let r = &mut ed.characters.recipe;
    r.level = 240;
    r.augmentations = BTreeMap::from([
        ("steadfast-will".to_owned(), 1),
        ("clutch-of-the-miser".to_owned(), 3),
        ("enduring-enchantment".to_owned(), 1),
        ("might-of-the-seventh-mule".to_owned(), 2),
        ("quick-learner".to_owned(), 1),
    ]);
    r.luminance = Some(Luminous {
        auras: BTreeMap::from([("aura-of-valor".to_owned(), 3)]),
        seer: None,
    });
    r.quests = BTreeMap::from([(VAULT_FLAG.to_owned(), 1)]);
    r.quest_credits = true;
    ed.extras_findings();
    let title = ed.characters.extras.titles.clone().unwrap()[0].0;
    ed.characters.recipe.titles = vec![title];
    ed.characters.save().unwrap();
    let found = ed.characters.findings().unwrap();
    assert!(found.iter().all(|f| f.warning), "{found:?}");
    assert!(ed.extras_findings().iter().all(|f| f.warning));
    ed.characters.show(
        CharacterTab::Augments,
        BuildView::Attributes,
        AugmentView::Luminance,
    );
    for _ in 0..3 {
        assert!(ed.frame());
    }
    keep(&mut ed, "live-character-extras", "01-luminance.png");
    ed.characters.tab = CharacterTab::Quests;
    for _ in 0..3 {
        assert!(ed.frame());
    }
    keep(&mut ed, "live-character-extras", "01-quests.png");

    ed.set_live(true).unwrap();
    ed.wait_live().unwrap();
    if let Err(e) = ed.play_until_entered(&[]) {
        let log = ed.live_log(0);
        let tail = &log[log.len().saturating_sub(40)..];
        panic!(
            "{e}; the tab says {:?} {:?}; the server's log ends:
{}",
            ed.characters.message,
            ed.characters.error,
            tail.join(
                "
"
            )
        );
    }
    let made = ed.characters.made.clone().expect("what the server made");
    assert_eq!(made.level, 240);
    // What the game was told of the character it plays.
    let start = std::time::Instant::now();
    while ed.play_int(231).unwrap_or(0) == 0 {
        assert!(ed.frame());
        assert!(
            start.elapsed() < std::time::Duration::from_secs(60),
            "the character's augmentations"
        );
    }
    assert_eq!(ed.play_int(223), Some(1), "Steadfast Will");
    assert_eq!(ed.play_int(231), Some(3), "Clutch of the Miser");
    assert_eq!(
        ed.play_int(326),
        Some(1),
        "the heritage's Jack of All Trades"
    );
    assert_eq!(ed.play_int(333), Some(3), "Aura of Valor");
    for _ in 0..60 {
        assert!(ed.frame());
    }
    keep(&mut ed, "live-character-extras", "02-played-as.png");
    ed.stop_play();
    assert!(ed.wait_world_empty(), "the character logged off");

    // What the server stored on its character record.
    let db = rusqlite::Connection::open_with_flags(
        dir.path().join(".playtest").join("world").join("shard.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    // The server's quest names compare as it compares them; read here, they are only read.
    db.create_collation("utf8mb4_uca1400_ai_ci", |a: &str, b: &str| a.cmp(b))
        .unwrap();
    let mut quests: Vec<(String, i64)> = db
        .prepare(
            "SELECT quest_Name, num_Times_Completed FROM character_properties_quest_registry WHERE character_Id = ?1",
        )
        .unwrap()
        .query_map([made.id], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    quests.sort();
    println!("quests {quests:?}");
    assert_eq!(
        quests,
        [
            ("ArantahKill1".to_owned(), 1),
            ("LumAugSkillQuest".to_owned(), 2),
            ("OswaldManualCompleted".to_owned(), 1),
            (VAULT_FLAG.to_owned(), 1),
        ]
    );
    let titles: Vec<i64> = db
        .prepare("SELECT title_Id FROM character_properties_title_book WHERE character_Id = ?1")
        .unwrap()
        .query_map([made.id], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(titles.contains(&i64::from(title)), "{titles:?}");
    drop(db);
    ed.set_live(false).unwrap();
}
