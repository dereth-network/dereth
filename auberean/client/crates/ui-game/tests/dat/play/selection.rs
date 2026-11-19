//! The selection's panel on the retail play layout: the field over the world, the presses on it,
//! the selection keys, the targeting cursor.

use auberean_ui::types::button_id;
use auberean_ui::{Token, Ui};
use auberean_ui_game::gameplay::{GamePlay, Notice, Request, View};
use auberean_ui_game::selection::{self, key, Selection};
use auberean_ui_game::world::{kind, Thing, World};

use crate::common::records;

const ME: u64 = 0x2130_0000_0000_00A1;

/// The play screen made and the player in the world.
fn play(ui: &mut Ui) -> GamePlay {
    let mut out = Vec::new();
    let mut p = GamePlay::new(ui, &mut out);
    p.login_finished(ui, &mut out);
    p
}

/// One frame with this world and these notices: the clock, then the play screen's tick.
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
        ..View::default()
    };
    p.tick(ui, &view, &[], &mut out);
    out
}

/// A creature, item or player standing `x` meters right and `y` ahead of the player.
fn thing(id: u64, kind: u32, x: f32, y: f32) -> Thing {
    Thing {
        id,
        kind,
        from_player: [x, y, 0.0],
        selectable: true,
        ..Thing::default()
    }
}

fn world(things: Vec<Thing>) -> World {
    let mut all = vec![Thing {
        id: ME,
        is_player: true,
        kind: kind::PLAYER,
        selectable: true,
        ..Thing::default()
    }];
    all.extend(things);
    World {
        player: ME,
        player_kind: kind::PLAYER,
        ranges: (50.0, 75.0),
        things: all,
        ..World::default()
    }
}

fn selects(out: &[Request]) -> Vec<u64> {
    out.iter()
        .filter_map(|r| match r {
            Request::Select(id) => Some(*id),
            _ => None,
        })
        .collect()
}

/// Press a button on the world at the middle of the screen, then let the world answer `picked`
/// at the next tick; what the panels asked over both.
fn press_world(
    ui: &mut Ui,
    p: &mut GamePlay,
    t: &mut f64,
    w: &World,
    button: u32,
    picked: Option<u64>,
) -> Vec<Request> {
    ui.pointer_move(512, 300);
    let mut out = tick(ui, p, t, w, &[]);
    ui.pointer_button(button, true);
    out.extend(tick(ui, p, t, w, &[]));
    ui.pointer_button(button, false);
    let answered = World {
        picked: Some(picked),
        ..w.clone()
    };
    out.extend(tick(ui, p, t, &answered, &[]));
    out
}

/// The selection's field lies over the whole screen behind every panel, so a press on the world
/// reaches it; a left press asks the world what lies under the pointer, and at the next tick
/// after the answer that is selected; a press on nothing selects nothing; an object that may not
/// be selected is passed over, unless the player's kind lets it select anything; a double press
/// selects and uses the object (not an item the player holds); a right press on an object asks
/// for the examination panel.
///
/// Behaviour: ui.selection.a-press-on-the-world-selects-what-lies-under-it
#[test]
fn a_press_on_the_world_selects_what_lies_under_it() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let field = ui
        .element(selection::field_name())
        .expect("the field over the world");
    let r = ui.screen_rect(field).expect("placed");
    assert_eq!(
        (r.x0, r.y0, r.x1, r.y1),
        (0, 0, 1023, 767),
        "it covers the screen"
    );
    assert_eq!(
        ui.hit(512, 300),
        Some(field),
        "the world under the pointer is the field"
    );
    let toolbar = ui.element(Token::of("ToolbarField")).expect("the toolbar");
    let tr = ui.screen_rect(toolbar).expect("placed");
    assert_ne!(
        ui.hit((tr.x0 + tr.x1) / 2, tr.y1 - 2),
        Some(field),
        "panels lie in front of it"
    );

    let mut w = world(vec![thing(7, kind::CREATURE, 1.0, 5.0)]);
    let out = press_world(&mut ui, &mut p, &mut t, &w, button_id::LEFT, Some(7));
    assert!(out.contains(&Request::Pick { x: 512, y: 300 }), "{out:?}");
    assert_eq!(selects(&out), vec![7]);
    assert!(!out.contains(&Request::UseSelected));

    let out = press_world(&mut ui, &mut p, &mut t, &w, button_id::LEFT, None);
    assert_eq!(
        selects(&out),
        vec![0],
        "nothing under the pointer: nothing selected"
    );

    w.things[1].selectable = false;
    let out = press_world(&mut ui, &mut p, &mut t, &w, button_id::LEFT, Some(7));
    assert!(selects(&out).is_empty(), "not selectable: passed over");
    w.player_kind = kind::ALL_SELECTABLE;
    let out = press_world(&mut ui, &mut p, &mut t, &w, button_id::LEFT, Some(7));
    assert_eq!(selects(&out), vec![7], "a kind that may select anything");

    let out = press_world(&mut ui, &mut p, &mut t, &w, button_id::DOUBLE_LEFT, Some(7));
    assert_eq!(selects(&out), vec![7]);
    assert!(
        out.contains(&Request::UseSelected),
        "a double press uses it"
    );
    w.things[1].container = Some(ME);
    let out = press_world(&mut ui, &mut p, &mut t, &w, button_id::DOUBLE_LEFT, Some(7));
    assert!(
        !out.contains(&Request::UseSelected),
        "not an item the player holds"
    );

    let out = press_world(&mut ui, &mut p, &mut t, &w, button_id::RIGHT, Some(7));
    assert!(out.contains(&Request::OpenExaminationPanel));
    let out = press_world(&mut ui, &mut p, &mut t, &w, button_id::RIGHT, None);
    assert!(
        !out.contains(&Request::OpenExaminationPanel),
        "not on nothing"
    );
}

/// A press taken by a panel in front (a toolbar button clicked, opening its window) leaves the
/// field over the world untouched: the next press on the world selects what lies under it, as
/// the first press did before.
///
/// Behaviour: ui.selection.a-press-on-the-world-after-a-button-still-selects
#[test]
fn a_press_on_the_world_after_a_button_still_selects() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let w = world(vec![thing(7, kind::CREATURE, 1.0, 5.0)]);
    let button = ui
        .element(Token::of("InventoryButton"))
        .expect("the inventory's button");
    let r = ui.screen_rect(button).expect("placed");
    ui.pointer_move((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2);
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    for down in [true, false] {
        ui.pointer_button(button_id::LEFT, down);
        tick(&mut ui, &mut p, &mut t, &w, &[]);
    }
    let window = ui.element(Token::of("InventoryField")).expect("the window");
    assert!(ui.shown(window), "the button opened its window");
    let out = press_world(&mut ui, &mut p, &mut t, &w, button_id::LEFT, Some(7));
    assert!(out.contains(&Request::Pick { x: 512, y: 300 }), "{out:?}");
    assert_eq!(selects(&out), vec![7]);
}

/// A selection key on its own panel: what it asked.
fn select(w: &World, k: u32) -> Vec<Request> {
    let mut ui = Ui::new(records(), 1024, 768);
    let frame = ui.new_main_frame();
    let s = Selection::new(&mut ui, frame);
    let mut out = Vec::new();
    s.select(&ui, w, k, &mut out);
    out
}

/// Each selection key asks the game to select as it does (the next, previous and nearest of each
/// kind, the player itself, the last selection, the last attacker), whose rules choose what;
/// Escape selects nothing; before the player is here, no key asks anything.
///
/// Behaviour: ui.selection.the-selection-keys-ask-the-game-to-select
#[test]
fn the_selection_keys_ask_the_game_to_select() {
    let w = world(vec![thing(10, kind::CREATURE, 0.0, 3.0)]);
    let keys = (key::SELF..=key::LAST)
        .chain([key::LAST_ATTACKER])
        .chain(key::PREVIOUS_PET..=key::NEAREST_PET);
    for k in keys {
        assert_eq!(select(&w, k), vec![Request::SelectBy(k)], "{k:#x}");
    }
    assert_eq!(select(&w, key::ESCAPE), vec![Request::Select(0)]);
    assert!(
        select(&World::default(), key::NEXT_CREATURE).is_empty(),
        "no player yet"
    );
}

/// The scripts' targeting notice shows the field's usage cursor over the world while an item
/// waits for its target, and puts the cursor back after.
///
/// Behaviour: ui.selection.the-targeting-notice-shows-the-usage-cursor
#[test]
fn the_targeting_notice_shows_the_usage_cursor() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let w = world(Vec::new());
    // The game's default cursor, as its flow sets it.
    let default = auberean_ui::manager::Cursor {
        picture: 0x4100_0870,
        hot_x: 0,
        hot_y: 0,
    };
    ui.set_cursor(default, true);
    ui.pointer_move(512, 300);
    tick(&mut ui, &mut p, &mut t, &w, &[]);
    let notice = |on: u32| Notice {
        native: selection::notice::SET_TARGETING_POINTER,
        about: Some(ME),
        words: vec![on],
        ..Notice::default()
    };
    tick(&mut ui, &mut p, &mut t, &w, &[notice(1)]);
    let c = ui.cursor().expect("a cursor");
    assert_eq!((c.picture, c.hot_x, c.hot_y), (0x4100_44BF, 11, 11));
    tick(&mut ui, &mut p, &mut t, &w, &[notice(0)]);
    assert_eq!(ui.cursor(), Some(default), "put back");
}

/// The war button asks the game to attack, and asks for no selection itself, whether or not the
/// player targets automatically: the game selects the nearest foe first when it does.
///
/// Behaviour: ui.selection.the-war-button-asks-the-game-to-attack
#[test]
fn the_war_button_asks_the_game_to_attack() {
    let mut foe = thing(10, kind::CREATURE, 0.0, 3.0);
    foe.attackable = true;
    let other = thing(11, kind::CREATURE, 0.0, 2.0);
    // The scripts say so only with nothing selected the player could attack.
    for auto_target in [true, false] {
        let mut ui = Ui::new(records(), 1024, 768);
        let mut p = play(&mut ui);
        let mut t = 0.0;
        let w = World {
            auto_target: true,
            should_auto_target: auto_target,
            ..world(vec![foe.clone(), other.clone()])
        };
        tick(&mut ui, &mut p, &mut t, &w, &[]);
        let button = ui
            .element(Token::of("StartAttackButton"))
            .expect("the war button");
        let r = ui.screen_rect(button).expect("placed");
        ui.pointer_move((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2);
        let mut out = tick(&mut ui, &mut p, &mut t, &w, &[]);
        for down in [true, false] {
            ui.pointer_button(button_id::LEFT, down);
            out.extend(tick(&mut ui, &mut p, &mut t, &w, &[]));
        }
        let asked: Vec<&Request> = out
            .iter()
            .filter(|r| matches!(r, Request::SelectBy(_) | Request::Attack { .. }))
            .collect();
        assert_eq!(
            asked,
            vec![&Request::Attack { start: true }],
            "targeting automatically: {auto_target}"
        );
    }
}
