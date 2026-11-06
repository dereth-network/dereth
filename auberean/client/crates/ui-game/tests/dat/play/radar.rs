//! The radar's panel on the retail play layout: the dial, the blips, the place line, the groove.

use auberean_ui::types::button_id;
use auberean_ui::{Token, Ui};
use auberean_ui_game::gameplay::{GamePlay, Request, View};
use auberean_ui_game::radar::{self, shape, Radar};
use auberean_ui_game::world::{kind, Thing, World};

use crate::common::records;

const ME: u64 = 0x2130_0000_0000_00A1;

fn play(ui: &mut Ui) -> GamePlay {
    let mut out = Vec::new();
    let mut p = GamePlay::new(ui, &mut out);
    p.login_finished(ui, &mut out);
    p
}

fn tick(ui: &mut Ui, p: &mut GamePlay, t: &mut f64, world: &World) -> Vec<Request> {
    *t += 0.05;
    ui.use_time(*t);
    let mut out = Vec::new();
    let view = View {
        world: Some(world),
        ..View::default()
    };
    p.tick(ui, &view, &[], &mut out);
    out
}

/// An object `x` meters right of the player and `y` ahead, on the radar in white as `shape`.
fn blip(id: u64, x: f32, y: f32, shape: u32) -> Thing {
    Thing {
        id,
        kind: kind::CREATURE,
        from_player: [x, y, 0.0],
        selectable: true,
        radar: Some(([1.0, 1.0, 1.0, 1.0], shape)),
        name: format!("Thing {id}"),
        ..Thing::default()
    }
}

fn world(heading: f32, inside: bool, things: Vec<Thing>) -> World {
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
        inside,
        cell: if inside { 0x0298_0109 } else { 0x76B8_0027 },
        blocks: (255, 255),
        heading,
        ranges: (50.0, 75.0),
        show_coordinates: true,
        things: all,
        ..World::default()
    }
}

fn radar_of(p: &GamePlay) -> &Radar {
    p.child::<Radar>().expect("the radar")
}

/// The texels painted at `(x, y)` of the radar's picture.
fn painted(ui: &Ui, p: &GamePlay, x: i32, y: i32) -> bool {
    let root = radar_of(p).root.expect("the radar's field");
    ui.node(root)
        .is_some_and(|n| n.fills.iter().any(|(r, _)| r.contains(x, y)))
}

/// The radar's dial stands on the radar's corner turned counterclockwise by the player's heading
/// (so its north points north while the way the player faces is up); each object the scripts put
/// on the radar within its range is a blip of its shape, scaled from the range to the radar's
/// radius (an object 30 meters ahead outdoors is 24 pixels above the middle), the selection's
/// bracketed; one beyond the range is not drawn, and indoors the range is shorter.
///
/// Behaviour: ui.radar.the-dial-turns-and-the-blips-show-what-is-around
#[test]
fn the_dial_turns_and_the_blips_show_what_is_around() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let mut w = world(
        270.0,
        false,
        vec![
            blip(7, 0.0, 30.0, shape::POINT),
            blip(8, -15.0, 0.0, shape::CROSS),
            blip(9, 70.0, 70.0, shape::FILL),
        ],
    );
    w.selection = 8;
    tick(&mut ui, &mut p, &mut t, &w);
    tick(&mut ui, &mut p, &mut t, &w);
    let r = radar_of(&p);
    let root = r.root.expect("the radar");
    let dial = r.heading.expect("the dial");
    assert_eq!(ui.node(dial).map(|n| n.rotation), Some(270));
    let (rx, ry) = ui.screen_origin(root).expect("placed");
    let d = ui.screen_rect(dial).expect("placed");
    assert_eq!((d.x0, d.y0), (rx, ry), "the dial on the radar's corner");
    // The middle (71, 69), radius 60, 75 meters outdoors: 0.8 pixels a meter.
    assert!(painted(&ui, &p, 71, 69 - 24), "ahead: up");
    assert!(painted(&ui, &p, 71 - 12, 69), "left: left");
    assert!(painted(&ui, &p, 71 - 12, 69 - 1), "a cross's edges");
    assert!(
        painted(&ui, &p, 71 - 12, 69 + 3),
        "the selection's brackets"
    );
    assert!(!painted(&ui, &p, 71, 69 + 3), "not the others'");
    assert!(!painted(&ui, &p, 71 + 56, 69 - 56), "beyond the range");

    w.inside = true;
    w.cell = 0x0298_0109;
    tick(&mut ui, &mut p, &mut t, &w);
    // 50 meters indoors: 1.2 pixels a meter.
    assert!(
        painted(&ui, &p, 71, 69 - 36),
        "indoors the range is shorter"
    );
}

/// The blip under the pointer names its object in the radar's tooltip, and a press on the radar
/// selects it.
///
/// Behaviour: ui.radar.the-blip-under-the-pointer-names-and-selects-its-object
#[test]
fn the_blip_under_the_pointer_names_and_selects_its_object() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let w = world(0.0, false, vec![blip(7, 10.0, 0.0, shape::FILL)]);
    tick(&mut ui, &mut p, &mut t, &w);
    let root = radar_of(&p).root.expect("the radar");
    let (rx, ry) = ui.screen_origin(root).expect("placed");
    ui.pointer_move(rx + 71 + 8, ry + 69 + 1);
    tick(&mut ui, &mut p, &mut t, &w);
    let base = ui
        .element(Token::of("BaseField"))
        .expect("the radar's base");
    assert_eq!(ui.element_tooltip(base), Some("Thing 7"));
    ui.pointer_button(button_id::LEFT, true);
    let out = tick(&mut ui, &mut p, &mut t, &w);
    ui.pointer_button(button_id::LEFT, false);
    assert!(out.contains(&Request::SetSelection(7)), "{out:?}");
    ui.pointer_move(rx + 71 - 30, ry + 69);
    tick(&mut ui, &mut p, &mut t, &w);
    assert_eq!(
        ui.element_tooltip(base),
        Some(""),
        "no blip under the pointer"
    );
}

/// The place line reads "Inside" in a building and the map coordinates outdoors; the
/// coordinates' button hides and shows it, its tooltip saying which, the option kept.
///
/// Behaviour: ui.radar.the-place-line-reads-inside-or-the-map-coordinates
#[test]
fn the_place_line_reads_inside_or_the_map_coordinates() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let text = |ui: &Ui| {
        ui.element(Token::of("CoordinatesText"))
            .and_then(|e| ui.text(e))
            .unwrap_or_default()
    };
    tick(&mut ui, &mut p, &mut t, &world(0.0, true, Vec::new()));
    assert_eq!(text(&ui), "Inside");
    // Block 0x76, 0xB8; cell 0x27: column 0x76 * 8 + 4, row 0xB8 * 8 + 6, of 2040 each.
    tick(&mut ui, &mut p, &mut t, &world(0.0, false, Vec::new()));
    assert_eq!(
        text(&ui),
        radar::map_coordinates(&ui, 0x76B8_0027, (255, 255))
    );
    let line = text(&ui);
    assert!(line.contains("44.9") && line.contains("7.1"), "{line}");

    let button = ui.element(Token::of("CoordBoxButton")).expect("the button");
    let r = ui.screen_rect(button).expect("placed");
    let w = world(0.0, false, Vec::new());
    ui.pointer_move((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2);
    tick(&mut ui, &mut p, &mut t, &w);
    let mut out = Vec::new();
    for down in [true, false] {
        ui.pointer_button(button_id::LEFT, down);
        out.extend(tick(&mut ui, &mut p, &mut t, &w));
    }
    assert!(out.contains(&Request::ShowCoordinates(false)), "{out:?}");
    let base = ui.element(Token::of("BaseField")).expect("the base");
    assert_eq!(ui.node(base).map(|n| n.state), Some(Token::of("closed")));
    assert!(ui.element_tooltip(button).is_some_and(|s| !s.is_empty()));
}

/// The objects around with a groove level add theirs while in range: the sum is the music's
/// groove level, told when it changes.
///
/// Behaviour: ui.radar.the-groove-level-adds-up-what-is-in-range
#[test]
fn the_groove_level_adds_up_what_is_in_range() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let mut a = blip(7, 0.0, 10.0, shape::POINT);
    a.groove = 2;
    let mut b = blip(8, 0.0, 20.0, shape::POINT);
    b.groove = 3;
    let mut far = blip(9, 0.0, 90.0, shape::POINT);
    far.groove = 5;
    let w = world(0.0, false, vec![a, b, far]);
    let mut out = Vec::new();
    for _ in 0..4 {
        out.extend(tick(&mut ui, &mut p, &mut t, &w));
    }
    assert!(out.contains(&Request::GrooveLevel(5)), "{out:?}");
    assert_eq!(radar_of(&p).groove_level(), 5);
}

/// The radar's field moved, the play screen asks once for the panels' places to be kept with the
/// player's options and sent to the server; the options coming again ask for the panels to be put
/// back from them, and the display set up again asks for them to be put back as the player's
/// interface keeps them.
///
/// Behaviour: ui.gameplay.the-panels-places-are-kept-and-put-back
#[test]
fn the_panels_places_are_kept_when_moved_and_put_back_when_the_options_come() {
    use auberean_ui_game::gameplay::{notice, Notice};
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let w = world(0.0, false, Vec::new());
    tick(&mut ui, &mut p, &mut t, &w);
    let root = radar_of(&p).root.expect("the radar");
    let base = ui
        .child_recursive(root, Token::of("BaseField"))
        .expect("the radar's field");
    // The radar's field says its place changed, as a drag of the radar makes it say.
    ui.broadcast(base, Token::of("location_changed"), 0);
    let out = tick(&mut ui, &mut p, &mut t, &w);
    let saves = out.iter().filter(|r| **r == Request::SaveLocations).count();
    assert_eq!(saves, 1, "kept once: {out:?}");
    assert!(
        !out.contains(&Request::KeepOptions),
        "the game options are not sent: {out:?}"
    );

    t += 0.05;
    ui.use_time(t);
    let reload = Notice {
        native: notice::RELOAD_OPTIONS,
        ..Notice::default()
    };
    let view = View {
        world: Some(&w),
        ..View::default()
    };
    let mut out = Vec::new();
    p.tick(&mut ui, &view, &[reload], &mut out);
    assert!(
        out.contains(&Request::PutBackLocations { reloaded: true }),
        "{out:?}"
    );
    assert!(!out.contains(&Request::SaveLocations), "{out:?}");

    ui.refresh(1280, 960);
    let out = tick(&mut ui, &mut p, &mut t, &w);
    assert!(
        out.contains(&Request::PutBackLocations { reloaded: false }),
        "{out:?}"
    );
}

/// The radar's kept places put back with its field hidden hide the whole radar, dial and all, so it
/// does not come back as a bare dial.
///
/// Behaviour: ui.radar.a-radar-kept-hidden-comes-back-hidden
#[test]
fn a_radar_kept_hidden_comes_back_hidden() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let w = world(0.0, false, Vec::new());
    tick(&mut ui, &mut p, &mut t, &w);
    let (root, heading) = {
        let r = radar_of(&p);
        (r.root.expect("the radar"), r.heading.expect("the dial"))
    };
    assert!(ui.shown(heading), "the dial shows");
    ui.set_visible(root, false);
    ui.frame_apply_locations(p.frame(), &Default::default());
    tick(&mut ui, &mut p, &mut t, &w);
    assert!(!ui.shown(heading), "the dial goes with it");
    assert!(!ui.shown(root));
}
