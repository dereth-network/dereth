//! The names' panel on the retail play layout: the selection's name, meters and corner marks,
//! the arrow to a selection off the screen, the names over heads.

use auberean_text::StringInfo;
use auberean_ui::types::button_id;
use auberean_ui::{Token, Ui};
use auberean_ui_game::gameplay::{GamePlay, Notice, Request, View};
use auberean_ui_game::names::{self, action, Box2D, Names};
use auberean_ui_game::world::{kind, Body, Camera, Thing, World};

use crate::common::records;

const ME: u64 = 0x2130_0000_0000_00A1;

fn play(ui: &mut Ui) -> GamePlay {
    let mut out = Vec::new();
    let mut p = GamePlay::new(ui, &mut out);
    p.login_finished(ui, &mut out);
    p
}

fn tick(
    ui: &mut Ui,
    p: &mut GamePlay,
    t: &mut f64,
    world: &World,
    notices: &[Notice],
) -> Vec<Request> {
    *t += 0.05;
    ui.use_time(*t);
    let mut out = Vec::new();
    let view = View {
        world: Some(world),
        everyone: notices,
        preclude: (0, 38),
        ..View::default()
    };
    p.tick(ui, &view, &[], &mut out);
    out
}

/// A body a meter wide and two tall standing at `(x, y)` of the block, on the radar in `color`.
fn body_at(id: u64, x: f32, y: f32, color: [f32; 4]) -> Thing {
    Thing {
        id,
        kind: kind::CREATURE,
        from_player: [x, y, 0.0],
        selectable: true,
        radar: Some((color, 0x4000_0001)),
        name: format!("Thing {id}"),
        body: Some(Body {
            origin: [x, y, 0.0],
            heading: 0.0,
            bounds: Some(([-0.5, -0.5, 0.0], [0.5, 0.5, 2.0])),
        }),
        ..Thing::default()
    }
}

/// The player at the block's corner, the camera a meter up behind it looking north.
fn world(things: Vec<Thing>) -> World {
    let mut all = vec![Thing {
        id: ME,
        is_player: true,
        kind: kind::PLAYER,
        ..Thing::default()
    }];
    all.extend(things);
    World {
        player: ME,
        player_kind: kind::PLAYER,
        ranges: (50.0, 75.0),
        things: all,
        camera: Some(Camera {
            position: [0.0, -1.0, 1.0],
            heading: 0.0,
            pitch: 0.0,
            fov_y: 75.0,
        }),
        ..World::default()
    }
}

/// A selection its animation has faded out of sight is neither marked nor named: its marks and
/// name hide until it can be seen again.
///
/// Behaviour: ui.names.a-selection-faded-out-of-sight-is-not-marked
#[test]
fn a_selection_faded_out_of_sight_is_not_marked() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let mut faded = body_at(7, 0.0, 10.0, [1.0, 0.5, 0.0, 1.0]);
    faded.faded = true;
    let mut w = world(vec![faded]);
    w.selection = 7;
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    let shown = |ui: &Ui, p: &GamePlay| {
        let n = names_of(p);
        (
            ui.shown(n.reticle[0].expect("a mark")),
            ui.shown(n.selection_name.expect("the name")),
        )
    };
    assert_eq!(shown(&ui, &p), (false, false), "faded");
    w.things[1].faded = false;
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    assert_eq!(shown(&ui, &p), (true, true), "seen again");
}

/// An item selected in the pack and then put down is named and marked as soon as it lies in the
/// world, with no new selection.
///
/// Behaviour: ui.names.an-item-selected-in-the-pack-is-marked-once-put-down
#[test]
fn an_item_selected_in_the_pack_is_marked_once_put_down() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let mut w = world(vec![]);
    w.selection = 7;
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    let shown = |ui: &Ui, p: &GamePlay| {
        let n = names_of(p);
        (
            ui.shown(n.reticle[0].expect("a mark")),
            ui.shown(n.selection_name.expect("the name")),
        )
    };
    assert_eq!(shown(&ui, &p), (false, false), "in the pack");
    w.things.push(body_at(7, 0.0, 10.0, [1.0, 0.5, 0.0, 1.0]));
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    assert_eq!(shown(&ui, &p), (true, true), "on the ground");
}

fn names_of(p: &GamePlay) -> &Names {
    p.child::<Names>().expect("the names")
}

fn rect(ui: &Ui, h: Option<auberean_ui::Handle>) -> auberean_ui::Rect {
    ui.screen_rect(h.expect("made")).expect("placed")
}

/// While portal space is drawn no name, and no mark of the selection, stands over it, even with the
/// server done with the player and its objects known; they come back as it goes.
///
/// Behaviour: ui.names.no-names-stand-over-portal-space
#[test]
fn no_names_stand_over_portal_space() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let mut w = world(vec![
        body_at(7, 0.0, 10.0, [1.0; 4]),
        body_at(8, 3.0, 10.0, [1.0; 4]),
    ]);
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    ui.key(action::SHOW_NAMES, true);
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    ui.key(action::SHOW_NAMES, false);
    w.selection = 7;
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    let shown = |ui: &Ui, p: &GamePlay| {
        let names = names_of(p).names().iter();
        names.filter(|x| ui.shown(x.element)).count()
    };
    let marked = |ui: &Ui, p: &GamePlay| {
        let n = names_of(p);
        n.reticle.iter().flatten().any(|m| ui.shown(*m))
    };
    assert_eq!(shown(&ui, &p), 1, "the other's name");
    assert!(marked(&ui, &p), "the selection's marks");

    w.portal_drawn = true;
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    assert_eq!(shown(&ui, &p), 0, "no name over portal space");
    assert!(!marked(&ui, &p), "no marks over it");

    w.portal_drawn = false;
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    assert_eq!(shown(&ui, &p), 1, "the name is back");
    assert!(marked(&ui, &p), "and the marks");
}

/// The selection's name stands over it in its radar color, its meters under the name once the
/// scripts send its vitals (health, and vigor, each its share of its greatest), and its four corner
/// marks stand off the corners of its box on the screen, each turned on from the last (a quarter,
/// a half, three quarters more); the name and the meters are centered over the box.
///
/// Behaviour: ui.names.the-selection-is-named-and-marked-over-its-box
#[test]
fn the_selection_is_named_and_marked_over_its_box() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let mut w = world(vec![body_at(7, 0.0, 10.0, [1.0, 0.5, 0.0, 1.0])]);
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    w.selection = 7;
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    let n = names_of(&p);
    let name = n.selection_name;
    let r = rect(&ui, name);
    assert!(ui.shown(name.expect("the name")), "the name shows");
    assert_eq!(
        ui.text(name.expect("the name")).as_deref(),
        Some("Thing 7"),
        "its name"
    );
    let marks: Vec<_> = n.reticle.iter().map(|m| rect(&ui, *m)).collect();
    let turns: Vec<_> = n
        .reticle
        .iter()
        .map(|m| ui.node(m.expect("a mark")).map(|n| n.rotation))
        .collect();
    assert_eq!(turns, vec![Some(0), Some(90), Some(270), Some(180)]);
    // Top left, bottom left, top right, bottom right, each off its corner.
    assert!(marks[0].x0 < marks[2].x0 && marks[0].y0 < marks[1].y0);
    assert_eq!((marks[0].x0, marks[0].y0), (marks[1].x0, marks[2].y0));
    assert_eq!((marks[3].x0, marks[3].y0), (marks[2].x0, marks[1].y0));
    let middle = (marks[0].x0 + marks[2].x0 + marks[2].width()) / 2;
    assert!(
        ((r.x0 + r.x1) / 2 - middle).abs() <= 1,
        "the name centered over the box: {r:?} {marks:?}"
    );
    assert_eq!(r.y1 + 1, marks[0].y0, "the name stands on the marks");
    let stuff = n.selection_stuff;
    assert!(
        !ui.shown(stuff.expect("the meters")),
        "no meters before the vitals"
    );

    let vitals = Notice {
        native: names::notice::UPDATE_SELECTION_INFO,
        about: Some(ME),
        words: vec![names::SELECTION_WITH_NUMBERS, 30, 0, 120, 50, 0, 100],
        ..Notice::default()
    };
    tick(&mut ui, &mut p, &mut t, &w, &[vitals]);
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    let n = names_of(&p);
    let s = rect(&ui, n.selection_stuff);
    let r = rect(&ui, n.selection_name);
    assert!(ui.shown(n.selection_stuff.expect("the meters")));
    assert_eq!(s.y0, r.y1 + 1, "the meters under the name");
    let meter = |name: &str| {
        ui.element(Token::of(name))
            .and_then(|m| ui.node(m))
            .and_then(|m| m.meter)
    };
    assert_eq!(meter("SelectionHealthMeter"), Some(0.25));
    assert_eq!(meter("SelectionVigorMeter"), Some(0.5));
}

/// When no corner of the selection's box is on the screen, an arrow on the screen's edge points
/// the way to it: behind the camera, at the foot pointing down; and the marks hide.
///
/// Behaviour: ui.names.an-arrow-points-to-a-selection-off-the-screen
#[test]
fn an_arrow_points_to_a_selection_off_the_screen() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let mut w = world(vec![body_at(7, 0.0, -10.0, [1.0; 4])]);
    w.selection = 7;
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    let n = names_of(&p);
    let arrow = n.off_screen.expect("the arrow");
    assert!(ui.shown(arrow));
    assert!(!ui.shown(n.reticle[0].expect("a mark")), "the marks hide");
    let turn = ui.node(arrow).map(|n| n.rotation);
    let r = rect(&ui, Some(arrow));
    let foot = 768 - 38;
    assert_eq!(
        (turn, r.y1 + 1),
        (Some(270), foot),
        "at the foot, pointing down"
    );

    // Beyond the greatest selection distance it is no longer selected.
    let mut far = world(vec![body_at(8, 0.0, 450.0, [1.0; 4])]);
    far.selection = 8;
    let out = tick(&mut ui, &mut p, &mut t, &far, &[]);
    assert!(out.contains(&Request::SetSelection(0)), "{out:?}");
}

/// Placed after the world moves on, the clock only asks for the marks to be placed: they stay
/// where the last camera put them until the placing is made with the world whose camera this
/// frame's is, and then stand where that camera sees the selection.
///
/// Behaviour: ui.names.placed-after-the-world-the-marks-take-its-camera
#[test]
fn placed_after_the_world_the_marks_take_its_camera() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let mut w = world(vec![body_at(7, 0.0, 10.0, [1.0; 4])]);
    w.selection = 7;
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    let mark = names_of(&p).reticle[0];
    let first = rect(&ui, mark);
    p.child_mut::<Names>().expect("the names").after_world = true;
    let mut turned = w.clone();
    if let Some(c) = turned.camera.as_mut() {
        c.heading = 10.0;
    }
    tick(&mut ui, &mut p, &mut t, &turned, &[]);
    assert_eq!(rect(&ui, mark), first, "the clock alone moves nothing");
    let mut out = Vec::new();
    p.child_mut::<Names>()
        .expect("the names")
        .place_after_world(&mut ui, &turned, &mut out);
    let placed = rect(&ui, mark);
    assert!(
        placed.x0 < first.x0,
        "turned right, the camera sees it further left: {first:?} {placed:?}"
    );
    p.child_mut::<Names>()
        .expect("the names")
        .place_after_world(&mut ui, &w, &mut out);
    assert_eq!(
        rect(&ui, mark),
        placed,
        "placed once for each time the clock asked"
    );
}

/// The names' key shows a name over every object on the radar within its range; a name that
/// would lie over another is moved off it (moving up or left the client stops two texels short);
/// a press on a name selects its object; the key again hides them, the option kept each time.
///
/// Behaviour: ui.names.names-over-heads-stand-clear-of-each-other
#[test]
fn names_over_heads_stand_clear_of_each_other() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let w = world(vec![
        body_at(7, 0.0, 10.0, [1.0; 4]),
        body_at(8, 0.05, 10.0, [1.0; 4]),
        body_at(9, 0.0, 90.0, [1.0; 4]),
    ]);
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    assert!(names_of(&p).names().is_empty());
    ui.key(action::SHOW_NAMES, true);
    let out = tick(&mut ui, &mut p, &mut t, &w, &[]);
    ui.key(action::SHOW_NAMES, false);
    assert!(out.contains(&Request::ShowNames(true)), "{out:?}");
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    let n = names_of(&p);
    let shown: Vec<_> = n
        .names()
        .iter()
        .filter(|x| ui.shown(x.element))
        .map(|x| (x.object, ui.screen_rect(x.element).expect("placed")))
        .collect();
    assert_eq!(shown.len(), 2, "the far one is beyond the range: {shown:?}");
    // Moved up off the other: as the client moves a name up or left, it stops two texels short.
    let (a, b) = (shown[0].1, shown[1].1);
    let apart = (a.y0 - b.y0).abs();
    assert!(
        apart >= a.height() - 2 && apart < a.height(),
        "moved up off the other: {shown:?}"
    );
    let (id, r) = shown[0];
    ui.pointer_move((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2);
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    ui.pointer_button(button_id::LEFT, true);
    let out = tick(&mut ui, &mut p, &mut t, &w, &[]);
    ui.pointer_button(button_id::LEFT, false);
    assert!(out.contains(&Request::SetSelection(id)), "{out:?}");

    ui.key(action::SHOW_NAMES, true);
    let out = tick(&mut ui, &mut p, &mut t, &w, &[]);
    ui.key(action::SHOW_NAMES, false);
    assert!(out.contains(&Request::ShowNames(false)));
    assert!(names_of(&p).names().is_empty());
}

/// A name over a head near the screen's side runs off the edge, cut there, not pushed back in.
///
/// Behaviour: ui.names.a-name-at-the-screens-side-runs-off-it
#[test]
fn a_name_at_the_screens_side_runs_off_it() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    // Eleven meters off is just inside the right edge from eleven meters back at this view.
    let w = world(vec![body_at(7, 11.0, 10.0, [1.0; 4])]);
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    ui.key(action::SHOW_NAMES, true);
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    ui.key(action::SHOW_NAMES, false);
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    let n = names_of(&p);
    let shown: Vec<_> = n
        .names()
        .iter()
        .filter(|x| ui.shown(x.element))
        .map(|x| ui.screen_rect(x.element).expect("placed"))
        .collect();
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert!(shown[0].x0 < 1024 && shown[0].x1 > 1023, "{shown:?}");
}

/// A name the scripts give as a string reference is given to the name over the object's head and
/// to the selection's name as one: when the scripts say the object's look changed (it was
/// renamed), both are made again from its new name, which shows once the string table it reads
/// has come.
///
/// Behaviour: ui.names.a-renamed-object-is-named-anew-once-its-string-table-comes
#[test]
fn a_renamed_object_is_named_anew_once_its_string_table_comes() {
    use super::stage::{Lacking, LACKING};
    let assets = Lacking::new();
    let mut ui = Ui::new(assets.clone(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let mut thing = body_at(7, 0.0, 10.0, [1.0; 4]);
    thing.name_info = Some(StringInfo::literal("Corpse"));
    let mut w = world(vec![thing]);
    w.selection = 7;
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    ui.key(action::SHOW_NAMES, true);
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    ui.key(action::SHOW_NAMES, false);
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    let texts = |ui: &Ui, p: &GamePlay| {
        let n = names_of(p);
        let over: Vec<String> = n
            .names()
            .iter()
            .filter(|x| x.object == 7)
            .map(|x| ui.text(x.element).unwrap_or_default())
            .collect();
        let selected = ui
            .text(n.selection_name.expect("the name"))
            .unwrap_or_default();
        (over, selected)
    };
    assert_eq!(
        texts(&ui, &p),
        (vec!["Corpse".to_owned()], "Corpse".to_owned())
    );
    w.things[1].name_info = Some(StringInfo::entry(LACKING, 5));
    let renamed = Notice {
        native: names::notice::CHANGE_RADAR_LOOK,
        about: Some(7),
        ..Notice::default()
    };
    tick(&mut ui, &mut p, &mut t, &w, &[renamed]);
    assert_eq!(
        texts(&ui, &p),
        (vec![String::new()], String::new()),
        "waiting on the table"
    );
    assets.arrive();
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    let (over, selected) = texts(&ui, &p);
    assert!(over[0].starts_with("From the server"), "{over:?}");
    assert!(selected.starts_with("From the server"), "{selected:?}");
}

/// An item changing names it anew, the name over its head as well as the selection's, so a stack's
/// count does not go stale; another object's name stays as it was made.
///
/// Behaviour: ui.names.an-item-that-changes-is-named-anew
#[test]
fn an_item_that_changes_is_named_anew() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let mut thing = body_at(7, 0.0, 10.0, [1.0; 4]);
    thing.name_info = Some(StringInfo::literal("5 Ore"));
    let mut other = body_at(8, 3.0, 10.0, [1.0; 4]);
    other.name_info = Some(StringInfo::literal("Pick"));
    let mut w = world(vec![thing, other]);
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    ui.key(action::SHOW_NAMES, true);
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    ui.key(action::SHOW_NAMES, false);
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    let over = |ui: &Ui, p: &GamePlay, id: u64| {
        names_of(p)
            .names()
            .iter()
            .find(|x| x.object == id)
            .map(|x| ui.text(x.element).unwrap_or_default())
    };
    assert_eq!(over(&ui, &p, 7).as_deref(), Some("5 Ore"));
    w.things[1].name_info = Some(StringInfo::literal("3 Ore"));
    w.things[2].name_info = Some(StringInfo::literal("Renamed"));
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    assert_eq!(
        over(&ui, &p, 7).as_deref(),
        Some("5 Ore"),
        "nothing said yet"
    );
    let changed = Notice {
        native: names::notice::ITEM_UPDATE,
        about: Some(7),
        ..Notice::default()
    };
    tick(&mut ui, &mut p, &mut t, &w, &[changed]);
    assert_eq!(
        over(&ui, &p, 7).as_deref(),
        Some("3 Ore"),
        "the stack's new count"
    );
    assert_eq!(
        over(&ui, &p, 8).as_deref(),
        Some("Pick"),
        "the other as it was"
    );
}

/// Boxes overlap when they share a texel; a box moved off another goes the shortest way it may
/// and may not come straight back.
///
/// Behaviour: ui.names.a-name-moves-off-another-the-shortest-way
#[test]
fn a_name_moves_off_another_the_shortest_way() {
    let a = Box2D::sized(0, 0, 10, 10);
    assert!(a.collide(&Box2D::sized(9, 9, 5, 5)));
    assert!(!a.collide(&Box2D::sized(10, 0, 5, 5)));
    let mut b = Box2D::sized(8, 2, 10, 4);
    let mut ways = 0xF;
    names::make_new_data(&mut b, &a, &mut ways);
    // Left is 16 away, up 4, right 2, down 8: right, and left is barred.
    assert_eq!(b, Box2D::sized(10, 2, 10, 4));
    assert_eq!(ways, 0xE);
    assert_eq!(names::place_here(&b, &[a]), None);
}

/// A hit's damage text as the scripts fill it: red, two seconds, scale 1.
fn hit(object: u64, text: &str) -> Notice {
    hit_at(object, text, 1.0)
}

/// A hit's damage text at a scale.
fn hit_at(object: u64, text: &str, scale: f32) -> Notice {
    Notice {
        native: names::notice::DO_3D_DAMAGE,
        about: Some(object),
        words: Vec::new(),
        records: vec![Some(auberean_ui_game::gameplay::Record::DamageText(
            names::DamageBlob {
                text: text.to_owned(),
                color: [1.0, 0.0, 0.0, 1.0],
                duration: 2.0,
                scale,
            },
        ))],
    }
}

/// A hit's damage text is drawn at its scale, and stands over the object by the size it is drawn
/// at, so a larger one stands further left.
///
/// Behaviour: ui.names.damage-text-is-drawn-at-its-scale
#[test]
fn damage_text_is_drawn_at_its_scale() {
    let at = |scale: f32| {
        let mut ui = Ui::new(records(), 1024, 768);
        let mut p = play(&mut ui);
        let mut t = 0.0;
        let w = world(vec![body_at(7, 0.0, 10.0, [1.0; 4])]);
        tick(&mut ui, &mut p, &mut t, &w, &[]);
        tick(&mut ui, &mut p, &mut t, &w, &[hit_at(7, "12", scale)]);
        let text = names_of(&p).damage()[0].element;
        let drawn = ui.node(text).expect("the text").object_scale;
        (drawn, rect(&ui, Some(text)))
    };
    let (one, plain) = at(1.0);
    let (two, large) = at(2.0);
    assert_eq!((one, two), (1.0, 2.0));
    assert_eq!(
        (plain.width(), plain.height()),
        (large.width(), large.height()),
        "its own size stays; the drawing is what scales"
    );
    let half = plain.width() / 2;
    assert!(
        (plain.x0 - large.x0 - half).abs() <= 1,
        "half its width further left: {plain:?} {large:?}"
    );
}

/// A hit's damage text fades out over its life: whole as it begins, about half gone half way.
///
/// Behaviour: ui.names.damage-text-fades-out-over-its-life
#[test]
fn damage_text_fades_out_over_its_life() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let w = world(vec![body_at(7, 0.0, 10.0, [1.0; 4])]);
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    tick(&mut ui, &mut p, &mut t, &w, &[hit(7, "12")]);
    let text = names_of(&p).damage()[0].element;
    let alpha = |ui: &Ui| ui.node(text).expect("the text").object_alpha;
    assert!(alpha(&ui) > 0.95, "whole as it begins: {}", alpha(&ui));
    for _ in 0..19 {
        tick(&mut ui, &mut p, &mut t, &w, &[]);
    }
    assert!(
        (0.4..=0.6).contains(&alpha(&ui)),
        "about half gone half way: {}",
        alpha(&ui)
    );
}

/// A hit's damage text stands over the struck object's box in its color and rises three of its
/// own heights over its two seconds, then goes; a second hit's text at once is moved off the
/// first.
///
/// Behaviour: ui.names.damage-text-rises-over-the-struck-object-and-goes
#[test]
fn damage_text_rises_over_the_struck_object_and_goes() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let w = world(vec![body_at(7, 0.0, 10.0, [1.0; 4])]);
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    tick(&mut ui, &mut p, &mut t, &w, &[hit(7, "12"), hit(7, "3")]);
    let texts: Vec<_> = names_of(&p).damage().to_vec();
    assert_eq!(texts.len(), 2);
    let first = rect(&ui, Some(texts[0].element));
    let second = rect(&ui, Some(texts[1].element));
    assert!(ui.shown(texts[0].element) && ui.shown(texts[1].element));
    // Moved off the first: as the client moves a text left or up, it stops two texels short.
    let across = first.x1.min(second.x1) - first.x0.max(second.x0) + 1;
    let down = first.y1.min(second.y1) - first.y0.max(second.y0) + 1;
    assert!(across <= 2 || down <= 2, "apart: {first:?} {second:?}");
    // Over the body's middle (the screen's middle, the camera looking at it).
    assert!((first.x0 + first.x1) / 2 - 512 <= 2, "centered: {first:?}");
    for _ in 0..19 {
        tick(&mut ui, &mut p, &mut t, &w, &[]);
    }
    let later = rect(&ui, Some(texts[0].element));
    let travel = first.y0 - later.y0;
    let h = first.height();
    assert!(
        travel >= h && travel <= 2 * h,
        "half way, about one and a half heights up: {travel} of {h}"
    );
    for _ in 0..21 {
        tick(&mut ui, &mut p, &mut t, &w, &[]);
    }
    assert!(names_of(&p).damage().is_empty(), "gone after two seconds");
}
