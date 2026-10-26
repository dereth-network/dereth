//! Behaviour: none (tooling: the world editor's links between spawns over the retail dats)
//! A lever is linked to the door it opens by a click on the door in the world, or from what stands
//! nearby; a line joins them while either is selected, and undo takes the link back. A server
//! door's own lever is listed as what sets it off.
//! Fixture: the sample world over Holtburg, with a door and a lever of the server's standing in its
//! open west side.

use dereth_primitives::Vec3;
use lyceum_editor::app::{EditorApp, Selection, Tab};
use lyceum_foundry::edit::Session;
use lyceum_foundry::pick;
use lyceum_foundry::script::Action as Edit;

use crate::{capture, click, editor_with, gpu_lock, hold_key, sample, shots, HOLTBURG};

const DOOR: &str = "0x7A9B4060";
const LEVER: &str = "0x7A9B4061";

/// A door and a lever of the server's, drawn with the models of Holtburg's house doors and a
/// lever, standing at `door` and `lever` (landblock meters on the ground); the lever set to open
/// the door when `linked`.
fn server_sql(door: Vec3, lever: Vec3, linked: bool) -> String {
    let cell = |p: Vec3| (u32::from(HOLTBURG) << 16) | lyceum_foundry::edit::outdoor_cell(p.x, p.y);
    let mut sql = format!(
        "INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES (90412, 'door-aluvian-house', 19, '2005-02-09 10:00:00')
     , (90413, 'leverbigswitch', 26, '2005-02-09 10:00:00');
INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES (90412, 1, 'Door'), (90413, 1, 'Lever');
INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES (90412, 1, 0x020019FF), (90413, 1, 0x0200031F);
INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `is_Link_Child`, `last_Modified`)
VALUES ({DOOR}, 90412, 0x{:08X}, {}, {}, {}, 1, 0, 0, 0, False, '2019-05-16 00:00:00')
     , ({LEVER}, 90413, 0x{:08X}, {}, {}, {}, 1, 0, 0, 0, {}, '2019-05-16 00:00:00');
",
        cell(door),
        door.x,
        door.y,
        door.z,
        cell(lever),
        lever.x,
        lever.y,
        lever.z,
        if linked { "True" } else { "False" },
    );
    if linked {
        sql.push_str(&format!(
            "INSERT INTO `landblock_instance_link` (`parent_GUID`, `child_GUID`, `last_Modified`)
VALUES ({DOOR}, {LEVER}, '2019-05-16 00:00:00');
"
        ));
    }
    sql
}

/// The editor over the sample world with the server's door and lever (linked or not), looking at
/// them; and where each stands, landblock meters.
fn open(tag: &str, linked: bool) -> (dereth_dat::testing::ScratchDir, EditorApp, Vec3, Vec3) {
    let (dir, world) = sample(tag);
    let (door, lever) = {
        let s = Session::open(&world, &dereth_dat::testing::dat_dir(), None).unwrap();
        let ground = |x: f32, y: f32| Vec3::new(x, y, s.ground(HOLTBURG, x, y).unwrap());
        (ground(60.0, 60.0), ground(66.0, 58.0))
    };
    let sql = dir.path().join("server.sql");
    std::fs::write(&sql, server_sql(door, lever, linked)).unwrap();
    let mut ed = editor_with(
        &world,
        &["--weenies", sql.to_str().unwrap(), "--size", "1280x900"],
    );
    ed.look_at(
        HOLTBURG,
        Vec3::new(63.0, 59.0, door.z + 1.0),
        14.0,
        0.0,
        -25.0,
    );
    for _ in 0..3 {
        ed.frame();
    }
    (dir, ed, door, lever)
}

/// Where `p` (landblock meters, a half meter up) is on screen.
fn screen(ed: &EditorApp, p: Vec3) -> (f32, f32) {
    let o = pick::block_origin(HOLTBURG);
    ed.view()
        .unwrap()
        .project(Vec3::new(o.x + p.x, o.y + p.y, p.z + 0.5))
        .unwrap()
}

fn sets_off(ed: &EditorApp, guid: &str) -> Option<String> {
    let s = ed.session.spawns().into_iter().find(|x| x.guid == guid)?;
    ed.session.sets_off(&s).map(|x| x.guid)
}

fn rect_click(ed: &mut EditorApp, r: egui::Rect) {
    click(ed, (r.center().x, r.center().y));
}

#[test]
fn a_lever_linked_to_a_door_in_the_world_is_drawn_joined_to_it_and_undo_takes_the_link_back() {
    let _gpu = gpu_lock();
    let (dir, mut ed, door, lever) = open("editor-links", false);
    // The lever selected by a click on it.
    let at = screen(&ed, lever);
    click(&mut ed, at);
    let file = ed.session.override_file(HOLTBURG);
    assert_eq!(
        ed.selection,
        Some(Selection::Spawn {
            file: file.clone(),
            guid: LEVER.into()
        })
    );
    ed.show_tab(Tab::Inspector);
    ed.frame();
    assert!(ed.links.target.is_none(), "it sets nothing off");
    assert!(ed.links.lines.is_empty());
    // Sets off…, then a click on the door: the lever opens it, as one step.
    let choose = ed.links.choose.expect("its Sets off…");
    rect_click(&mut ed, choose);
    assert_eq!(ed.links.picking, Some((file.clone(), LEVER.into())));
    let steps = ed.session.history().0;
    let at = screen(&ed, door);
    click(&mut ed, at);
    assert_eq!(ed.links.picking, None, "{}", ed.panels.message);
    assert_eq!(sets_off(&ed, LEVER), Some(DOOR.into()));
    assert_eq!(ed.session.history().0, steps + 1);
    assert!(
        matches!(&ed.selection, Some(Selection::Spawn { guid, .. }) if guid == LEVER),
        "the lever stays selected"
    );
    ed.frame();
    ed.frame();
    // A line from the lever to the door, where each stands on screen.
    assert_eq!(ed.links.lines.len(), 1);
    let (from, to) = ed.links.lines[0];
    let (a, b) = (screen(&ed, lever), screen(&ed, door));
    let near = |p: (f32, f32), q: (f32, f32)| (p.0 - q.0).abs() < 1.5 && (p.1 - q.1).abs() < 1.5;
    assert!(near(from, a) && near(to, b), "{from:?} {to:?}");
    assert!(ed.links.target.is_some(), "the door named on its card");
    let shots = shots("links");
    ed.capture(&shots.join("lever-sets-off-door.png")).unwrap();
    // The door selected shows the lever as what sets it off, and the same line.
    let target = ed.links.target.unwrap();
    rect_click(&mut ed, target);
    assert!(matches!(&ed.selection, Some(Selection::Spawn { guid, .. }) if guid == DOOR));
    ed.frame();
    assert_eq!(ed.links.set_off_by.len(), 1);
    assert_eq!(ed.links.set_off_by[0].0 .1, LEVER);
    assert_eq!(ed.links.lines.len(), 1);
    assert!(
        ed.generator_marks.children.is_empty(),
        "the door owns nothing"
    );
    ed.capture(&shots.join("door-set-off-by-lever.png"))
        .unwrap();
    // Undone: no link, no line.
    ed.edit(&Edit::Undo).unwrap();
    ed.frame();
    assert_eq!(sets_off(&ed, LEVER), None);
    assert!(ed.links.lines.is_empty() && ed.links.set_off_by.is_empty());
    // From the nearby list instead: the door is offered, and chosen.
    ed.selection = Some(Selection::Spawn {
        file: file.clone(),
        guid: LEVER.into(),
    });
    ed.frame();
    let choose = ed.links.choose.unwrap();
    rect_click(&mut ed, choose);
    ed.frame();
    let nearby = ed.links.nearby.expect("the nearby list");
    rect_click(&mut ed, nearby);
    ed.frame();
    let (_, row) = ed
        .links
        .nearby_rows
        .iter()
        .find(|(x, _)| x.1 == DOOR)
        .cloned()
        .expect("the door offered");
    ed.capture(&shots.join("nearby.png")).unwrap();
    rect_click(&mut ed, row);
    assert_eq!(sets_off(&ed, LEVER), Some(DOOR.into()));
    // Picking again and let go of with Escape: the lever stays selected and linked.
    ed.frame();
    let choose = ed.links.choose.unwrap();
    rect_click(&mut ed, choose);
    assert!(ed.links.picking.is_some());
    hold_key(&mut ed, 0x1B, 1);
    assert_eq!(ed.links.picking, None);
    assert!(matches!(&ed.selection, Some(Selection::Spawn { guid, .. }) if guid == LEVER));
    // Removed from its card: one step, undone again.
    ed.frame();
    let steps = ed.session.history().0;
    let remove = ed.links.remove.expect("its Remove");
    rect_click(&mut ed, remove);
    assert_eq!(sets_off(&ed, LEVER), None);
    assert_eq!(ed.session.history().0, steps + 1);
    ed.edit(&Edit::Undo).unwrap();
    assert_eq!(sets_off(&ed, LEVER), Some(DOOR.into()));
    let _ = capture(&mut ed, dir.path());
}

#[test]
fn a_server_doors_own_lever_is_listed_as_what_sets_it_off_and_a_click_selects_it() {
    let _gpu = gpu_lock();
    let (_dir, mut ed, door, _) = open("editor-links-server", true);
    let at = screen(&ed, door);
    click(&mut ed, at);
    assert!(matches!(&ed.selection, Some(Selection::Spawn { guid, .. }) if guid == DOOR));
    ed.show_tab(Tab::Inspector);
    ed.frame();
    ed.frame();
    assert_eq!(ed.links.set_off_by.len(), 1);
    let ((_, guid), row) = ed.links.set_off_by[0].clone();
    assert_eq!(guid, LEVER);
    assert_eq!(ed.links.lines.len(), 1, "a line from the lever");
    assert!(
        ed.generator_marks.children.is_empty(),
        "the lever is not the door's own"
    );
    ed.capture(&shots("links").join("server-door.png")).unwrap();
    rect_click(&mut ed, row);
    assert!(matches!(&ed.selection, Some(Selection::Spawn { guid, .. }) if guid == LEVER));
    ed.frame();
    assert!(
        ed.links.target.is_some(),
        "the lever names the door it opens"
    );
    assert_eq!(sets_off(&ed, LEVER), Some(DOOR.into()));
}

/// A prison door no one opens by hand, and a lever beside it, placed by the pack in Holtburg's
/// open meadow while the editor's world runs, and linked there: the playtest character uses the
/// lever, and the door opens.
#[cfg(feature = "live-server-tests")]
#[test]
fn a_lever_linked_to_a_door_while_the_world_runs_opens_it_when_the_playtest_character_uses_it() {
    use lyceum_foundry::edit::{Op, Weenie};
    let _gpu = gpu_lock();
    let (_dir, world) = sample("live-links");
    let server = std::path::PathBuf::from(
        std::env::var_os("EMPYREAN_SERVER_BIN").expect("EMPYREAN_SERVER_BIN"),
    );
    let sql = std::path::PathBuf::from(
        std::env::var_os("EMPYREAN_WORLD_SQL").expect("EMPYREAN_WORLD_SQL"),
    );
    let mut ed = editor_with(
        &world,
        &[
            "--empyrean",
            server.parent().unwrap().to_str().unwrap(),
            "--weenies",
            sql.to_str().unwrap(),
            "--size",
            "1280x900",
        ],
    );
    ed.wait_for_names().unwrap();
    ed.set_live(true).unwrap();
    ed.wait_live().unwrap();
    // `doorprisonactivated` and `leverbigswitch`, as the world database has them.
    let place = |ed: &mut EditorApp, class: u32, x: f32, y: f32| {
        let z = ed.session.ground(HOLTBURG, x, y).unwrap();
        let file = ed
            .edit(&Edit::Op(Op::PlaceSpawn {
                landblock: HOLTBURG,
                weenie: Weenie::Class(class),
                origin: Vec3::new(x, y, z),
                heading: 0.0,
            }))
            .unwrap()
            .spawned
            .unwrap();
        ed.session
            .spawns()
            .into_iter()
            .find(|s| s.file == file)
            .unwrap()
    };
    let door = place(&mut ed, 2179, 120.0, 117.0);
    let lever = place(&mut ed, 2609, 124.0, 114.0);
    ed.edit(&Edit::Op(Op::Link {
        file: lever.file.clone(),
        guid: lever.guid.clone(),
        target: Some((door.file.clone(), door.guid.clone())),
    }))
    .unwrap();
    let door_guid = ed.session.spawn_number(&door).unwrap();
    let lever_guid = ed.session.spawn_number(&lever).unwrap();
    // The character comes in a few meters south of the lever, facing north.
    let z = ed.session.ground(HOLTBURG, 124.0, 110.0).unwrap();
    ed.look_at(HOLTBURG, Vec3::new(124.0, 110.0, z + 1.6), 4.0, 0.0, 0.0);
    ed.frame();
    ed.play_options.interface = lyceum_editor::app::PlayInterface::Modern;
    ed.play_until_entered(&["--use".to_owned(), format!("0x{lever_guid:08X}")])
        .unwrap();
    let shots = shots("live-links");
    for _ in 0..30 {
        assert!(ed.frame());
    }
    ed.capture(&shots.join("arrived.png")).unwrap();
    // A door open is one the world lets things pass through.
    const ETHEREAL: u32 = 0x4;
    let start = std::time::Instant::now();
    while ed
        .play_object_state(door_guid)
        .is_none_or(|s| s & ETHEREAL == 0)
    {
        assert!(ed.frame());
        // Only against a hang: the door opening is what ends the wait.
        assert!(
            start.elapsed() < std::time::Duration::from_secs(120),
            "the door did not open: {:?}",
            ed.play_object_state(door_guid)
        );
    }
    for _ in 0..30 {
        assert!(ed.frame());
    }
    ed.capture(&shots.join("door-open.png")).unwrap();
    ed.stop_play();
    assert!(ed.wait_world_empty(), "the character logged off");
    ed.set_live(false).unwrap();
}
