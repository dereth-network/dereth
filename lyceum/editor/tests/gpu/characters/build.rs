//! Behaviour: none (tooling: the world editor's Characters tab over the retail dats)
//! The Characters tab's build in detail: each skill untrained, trained or specialized with the
//! credits it leaves (the Build tab), the innate split and the weights (the Tweak tab), the
//! heritage, the gender and an own archetype (at the top); and, against the editor's own server, a
//! character of a chosen heritage and gender made by the server, played as, then given another
//! heritage's body.

use dereth_progression::Sac;
use lyceum_editor::app::EditorApp;
use lyceum_editor::characters::{AugmentView, BuildView, CharacterTab};

use crate::{editor_with, gpu_lock, sample, shots};

fn keep(ed: &mut EditorApp, group: &str, name: &str) {
    ed.capture(&shots(group).join(name)).unwrap();
}

/// The class skill `name` has in the selected recipe's build.
fn class_of(ed: &mut EditorApp, name: &str) -> Sac {
    let id = ed
        .characters
        .tables
        .as_ref()
        .unwrap()
        .tables()
        .skill_named(name)
        .unwrap();
    let b = ed.characters.build().unwrap().clone().unwrap();
    b.skills.iter().find(|s| s.id == id).unwrap().sac
}

/// Ask for skill `name` as `sac` in the selected recipe, as its row's choice does.
fn choose(ed: &mut EditorApp, name: &str, sac: Sac) {
    let t = ed.characters.tables.as_ref().unwrap().tables();
    let id = t.skill_named(name).unwrap();
    ed.characters
        .recipe
        .edit_plan(|p| p.choose(&t, id, sac))
        .unwrap();
}

#[test]
fn a_skill_chosen_in_the_build_spends_its_credits_and_a_heritage_and_own_archetype_follow_the_recipe(
) {
    let _g = gpu_lock();
    let (dir, world) = sample("characters-build");
    let mut ed = editor_with(&world, &["--size", "1600x1000"]);
    ed.characters.archetypes_dir = Some(dir.path().join("settings").join("archetypes"));
    ed.open_characters();
    // Each heritage the data files offer, by the name players read.
    let shown: Vec<String> = ed
        .characters
        .tables
        .as_ref()
        .unwrap()
        .tables()
        .heritages()
        .iter()
        .map(|(id, n)| lyceum_editor::characters::heritage_title(*id, n))
        .collect();
    assert_eq!(
        shown,
        [
            "Aluvian",
            "Gharu'ndim",
            "Sho",
            "Viamontian",
            "Umbraen",
            "Gear Knight",
            "Tumerok",
            "Lugian",
            "Empyrean",
            "Penumbraen",
            "Undead"
        ]
    );
    ed.new_character("Test Melee").unwrap();
    ed.characters.recipe.level = 80;
    ed.characters.show(
        CharacterTab::Build,
        BuildView::Attributes,
        AugmentView::Augmentations,
    );
    for _ in 0..4 {
        assert!(ed.frame());
    }
    keep(&mut ed, "characters-build", "01-attributes.png");
    ed.characters.build_view = BuildView::Skills;
    for _ in 0..4 {
        assert!(ed.frame());
    }
    keep(&mut ed, "characters-build", "01-skills.png");
    let before = ed.characters.build().unwrap().clone().unwrap();
    assert_eq!(class_of(&mut ed, "Light Weapons"), Sac::Untrained);
    // Light Weapons trained, Healing given up: 4 credits spent, 6 back.
    choose(&mut ed, "Light Weapons", Sac::Trained);
    choose(&mut ed, "Healing", Sac::Untrained);
    assert_eq!(class_of(&mut ed, "Light Weapons"), Sac::Trained);
    assert_eq!(class_of(&mut ed, "Healing"), Sac::Untrained);
    let after = ed.characters.build().unwrap().clone().unwrap();
    assert_ne!(after.unspent_credits, before.unspent_credits);
    assert!(ed.characters.findings().unwrap().iter().all(|f| f.warning));
    // Strength from 100 to 90 leaves 10 points: the checks say so until they are spent.
    ed.characters
        .recipe
        .edit_plan(|p| p.innate[0] = 90)
        .unwrap();
    assert!(ed.characters.findings().is_err() || !ed.characters.findings().unwrap().is_empty());
    ed.characters
        .recipe
        .edit_plan(|p| p.innate[1] = 20)
        .unwrap();
    assert!(ed.characters.findings().unwrap().iter().all(|f| f.warning));
    ed.characters.tab = CharacterTab::Tweak;
    for _ in 0..4 {
        assert!(ed.frame());
    }
    keep(&mut ed, "characters-build", "02-tweak.png");
    ed.characters.tab = CharacterTab::Build;
    // A Sho: her own heritage's credits and costs.
    ed.characters.recipe.heritage = Some("Sho".into());
    let sho = ed.characters.build().unwrap().clone().unwrap();
    assert_eq!(sho.heritage, dereth_progression::heritage::SHO);
    assert_eq!(ed.characters.body().unwrap().0, 3);
    for _ in 0..4 {
        assert!(ed.frame());
    }
    keep(&mut ed, "characters-build", "03-chosen.png");
    // Kept as the builder's own archetype, and taken whole by a new recipe.
    ed.characters.save_own("Sho Brawler").unwrap();
    assert_eq!(ed.characters.own.len(), 1);
    let own = ed.characters.own[0].1.clone();
    ed.new_character("Second Melee").unwrap();
    ed.characters.recipe.level = 80;
    ed.characters.recipe.take(&own);
    assert_eq!(class_of(&mut ed, "Light Weapons"), Sac::Trained);
    assert_eq!(ed.characters.recipe.heritage.as_deref(), Some("Sho"));
    for _ in 0..4 {
        assert!(ed.frame());
    }
    keep(&mut ed, "characters-build", "04-own-archetype.png");
}

#[cfg(feature = "live-server-tests")]
#[test]
fn a_sho_war_mage_with_her_skills_chosen_is_made_by_the_server_played_as_and_given_a_lugians_body()
{
    use std::path::PathBuf;
    let _g = gpu_lock();
    let (_dir, world) = sample("live-characters-made");
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
    ed.open_characters();
    ed.new_character("Sho Mage").unwrap();
    {
        let r = &mut ed.characters.recipe;
        r.level = 60;
        r.archetype = "war-mage".into();
        r.heritage = Some("Sho".into());
        r.gender = lyceum_editor::characters::Gender::Female;
    }
    choose(&mut ed, "Life Magic", Sac::Specialized);
    assert_eq!(class_of(&mut ed, "Life Magic"), Sac::Specialized);
    assert!(ed.characters.findings().unwrap().iter().all(|f| f.warning));
    ed.characters.save().unwrap();
    ed.characters.show(
        CharacterTab::Build,
        BuildView::Skills,
        AugmentView::Augmentations,
    );
    for _ in 0..4 {
        assert!(ed.frame());
    }
    keep(&mut ed, "live-characters-made", "01-sho-mage.png");
    ed.set_live(true).unwrap();
    ed.wait_live().unwrap();
    // Play As: the server makes her; no client goes through the creation screens.
    let mark = ed.live_log(0).len();
    ed.play_until_entered(&[]).unwrap();
    let log = ed.live_log(mark);
    assert!(
        log.iter()
            .any(|l| l.contains("Playtest character created.")),
        "{log:#?}"
    );
    let made = ed.characters.made.clone().expect("what the server made");
    assert_eq!(made.level, 60);
    assert!(
        made.items.iter().any(|i| i.slot != 0),
        "made new, it still wears its gear: {made:?}"
    );
    let start = std::time::Instant::now();
    while ed.play_int(188).is_none_or(|h| h == 0) {
        assert!(ed.frame());
        // Only against a hang: her description arriving is what ends the wait.
        assert!(start.elapsed() < std::time::Duration::from_secs(120));
    }
    assert_eq!(ed.play_int(188), Some(3), "a Sho");
    assert_eq!(ed.play_int(113), Some(2), "a woman");
    assert_eq!(ed.play_int(25), Some(60));
    for _ in 0..90 {
        assert!(ed.frame());
    }
    keep(&mut ed, "live-characters-made", "02-played-as-sho.png");
    ed.stop_play();
    assert!(ed.wait_world_empty(), "she logged off");
    // A Lugian now: the same character, given a Lugian's body.
    ed.characters.recipe.heritage = Some("Lugian".into());
    ed.characters.save().unwrap();
    let mark = ed.live_log(0).len();
    ed.play_until_entered(&[]).unwrap();
    assert!(ed
        .live_log(mark)
        .iter()
        .any(|l| l.contains("Playtest character updated;")));
    assert_eq!(ed.characters.made.as_ref().map(|m| m.id), Some(made.id));
    let start = std::time::Instant::now();
    while ed.play_int(188).is_none_or(|h| h == 0) {
        assert!(ed.frame());
        // Only against a hang: her description arriving is what ends the wait.
        assert!(start.elapsed() < std::time::Duration::from_secs(120));
    }
    assert_eq!(ed.play_int(188), Some(8), "a Lugian");
    for _ in 0..90 {
        assert!(ed.frame());
    }
    keep(&mut ed, "live-characters-made", "03-played-as-lugian.png");
    ed.stop_play();
    assert!(ed.wait_world_empty(), "she logged off");
    ed.set_live(false).unwrap();
}
