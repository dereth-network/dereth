//! The HUD: the parameter bar, the target bar, the status row, the menu bar (its icons and their
//! glow) and its system menu, the question Log Out asks, no money on it, and what answers the
//! pointer.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use auberean_client_sdk::contract::notice::{CombatNotice, Notice, Said};
use auberean_client_sdk::contract::view::{self as v, Inventory, View};
use auberean_horizon::draw::{Quad, Rect};
use auberean_horizon::keys::Keyed;
use auberean_horizon::state::{Effect, Frame, GameState, Target, Vital};
use auberean_horizon::ui::chat::Chat;
use auberean_horizon::ui::hud::{
    self, effect_rect, gauge_rects, menu_button_rect, system_row_rect, target_gauge_rect,
    time_left, MenuButton, SystemItem, ATTACKING, EFFECTS_PER_ROW, EFFECT_ROWS, FOCUS, HEALTH,
    VIGOR,
};
use auberean_horizon::ui::{questions, social};

use crate::{centre, drew_piece, in_world, measured, Harness, PLAYER};

/// The flat quads drawn in `colour`.
fn fills(h: &Harness, colour: u32) -> Vec<Rect> {
    h.list
        .quads
        .iter()
        .filter(|q: &&Quad| q.tex.is_none() && q.colour == colour)
        .map(|q| q.dst)
        .collect()
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01
}

#[test]
fn the_parameter_bars_gauges_fill_as_far_as_health_vigor_and_focus_are_full() {
    let mut h = Harness::new();
    let mut s = in_world();
    s.hero = true;
    s.health = Vital {
        current: 30,
        max: 120,
    };
    s.vigor = Vital {
        current: 60,
        max: 60,
    };
    s.focus = Vital {
        current: 10,
        max: 40,
    };
    h.frame(&s);
    let bars = gauge_rects(h.screen, h.k());
    for ((colour, ratio), bar) in [(HEALTH, 0.25), (VIGOR, 1.0), (FOCUS, 0.25)]
        .into_iter()
        .zip(bars)
    {
        let filled = fills(&h, colour);
        let at = filled
            .iter()
            .find(|r| near(r.x, bar.x) && near(r.y, bar.y))
            .unwrap_or_else(|| panic!("{colour:#x} drawn at its bar: {filled:?}"));
        assert!(
            near(at.w, bar.w * ratio),
            "{colour:#x}: {} of {}",
            at.w,
            bar.w
        );
    }
}

/// Where the gauges of `colour` were drawn, their bars' places (the fill starts at its bar's left).
fn gauge_at(h: &Harness, colour: u32, bars: &[Rect]) -> Option<Rect> {
    let filled = fills(h, colour);
    bars.iter()
        .copied()
        .find(|bar| filled.iter().any(|r| near(r.x, bar.x) && near(r.y, bar.y)))
}

#[test]
fn a_hero_sees_focus_beside_health_and_vigor_and_anyone_else_the_two_closed_up() {
    let mut h = Harness::new();
    let mut s = in_world();
    let some = Vital {
        current: 5,
        max: 10,
    };
    (s.health, s.vigor, s.focus) = (some, some, some);
    let k = h.k();
    let three = gauge_rects(h.screen, k);
    let two = hud::vital_rects(h.screen, k, false);
    h.frame(&s);
    assert!(fills(&h, FOCUS).is_empty(), "no focus but a hero's");
    assert_eq!(gauge_at(&h, HEALTH, &[two[0], three[0]]), Some(two[0]));
    assert_eq!(gauge_at(&h, VIGOR, &[two[1], three[1]]), Some(two[1]));
    let centre = |a: Rect, b: Rect| (a.x + b.right()) / 2.0;
    assert!(
        near(centre(two[0], two[1]), centre(three[0], three[2])),
        "closed up where the three stand"
    );
    s.hero = true;
    h.frame(&s);
    assert_eq!(gauge_at(&h, HEALTH, &[two[0], three[0]]), Some(three[0]));
    assert_eq!(gauge_at(&h, VIGOR, &[two[1], three[1]]), Some(three[1]));
    assert_eq!(
        gauge_at(&h, FOCUS, &[three[2]]),
        Some(three[2]),
        "beside vigor"
    );
}

#[test]
fn a_heros_focus_gauge_moved_away_leaves_health_and_vigor_closed_up() {
    let mut h = Harness::new();
    let mut s = in_world();
    s.hero = true;
    let some = Vital {
        current: 5,
        max: 10,
    };
    (s.health, s.focus) = (some, some);
    h.ui.layout.read(Some("focus 0 -200 1\n"));
    h.frame(&s);
    let k = h.k();
    let three = gauge_rects(h.screen, k);
    let two = hud::vital_rects(h.screen, k, false);
    assert_eq!(gauge_at(&h, HEALTH, &[two[0], three[0]]), Some(two[0]));
    let moved = three[2].offset(0.0, -200.0 * k);
    assert_eq!(gauge_at(&h, FOCUS, &[three[2], moved]), Some(moved));
}

#[test]
fn the_target_bar_shows_the_selections_health_once_the_scripts_told_it() {
    let mut h = Harness::new();
    let mut s = in_world();
    s.target = Some(Target {
        id: 7,
        name: "Drudge".into(),
        health: None,
    });
    h.frame(&s);
    let bar = target_gauge_rect(h.screen, h.k());
    let at_bar = |h: &Harness| {
        fills(h, HEALTH)
            .into_iter()
            .find(|r| near(r.x, bar.x) && near(r.y, bar.y))
    };
    assert_eq!(at_bar(&h), None, "no health told yet");
    s.target.as_mut().expect("the target").health = Some(Vital {
        current: 5,
        max: 10,
    });
    h.frame(&s);
    let r = at_bar(&h).expect("its gauge");
    assert!(near(r.w, bar.w / 2.0));
    s.target = None;
    h.frame(&s);
    assert_eq!(at_bar(&h), None, "nothing selected, no bar");
}

#[test]
fn the_status_row_holds_three_rows_of_nine_effects_from_the_top_right() {
    let mut h = Harness::new();
    let mut s = in_world();
    s.effects = (0..30)
        .map(|n| Effect {
            id: n,
            name: format!("Effect {n}"),
            icon: 0,
            left: Some(42.0),
        })
        .collect();
    h.frame(&s);
    // With no picture for an icon, a plain tile stands in its place.
    let tiles = fills(&h, 0xC020_2830);
    assert_eq!(tiles.len(), EFFECTS_PER_ROW * EFFECT_ROWS);
    let k = h.k();
    let first = effect_rect(h.screen, k, 0);
    assert!(
        first.right() <= h.screen.0 && first.right() > h.screen.0 - 40.0 * k,
        "top right"
    );
    let below = effect_rect(h.screen, k, EFFECTS_PER_ROW);
    assert!(
        near(below.x, first.x) && below.y > first.bottom(),
        "the next row under it"
    );
    assert!(effect_rect(h.screen, k, 1).x < first.x, "leftward");
    assert!(tiles.contains(&first));
}

#[test]
fn what_is_left_of_an_effect_is_written_in_seconds_minutes_or_hours() {
    assert_eq!(time_left(42.2), "43s");
    assert_eq!(time_left(59.0), "59s");
    assert_eq!(time_left(61.0), "2m");
    assert_eq!(time_left(3600.0), "1h");
    assert_eq!(time_left(-3.0), "0s");
}

#[test]
fn the_log_window_and_the_menu_bar_answer_the_pointer_and_the_world_between_them_does_not() {
    let mut h = Harness::new();
    let out_of_world = GameState::default();
    h.frame(&out_of_world);
    let k = h.k();
    let log = Chat::rect(h.screen, k);
    assert!(
        !h.ui.covers(log.x + 5.0, log.y + 5.0),
        "nothing drawn out of the world"
    );
    h.frame(&in_world());
    assert!(h.ui.covers(log.x + 5.0, log.y + 5.0));
    let menu = hud::menu_rect(h.screen, k);
    assert!(h.ui.covers(menu.x + 1.0, menu.y + 1.0));
    assert!(
        !h.ui.covers(h.screen.0 / 2.0, h.screen.1 / 2.0),
        "the world"
    );
}

#[test]
fn the_menu_bar_s_plate_holds_every_button_with_room_round_them_on_every_screen() {
    for (screen, step) in crate::play::fit::screens() {
        let mut h = crate::play::fit::on(screen, step);
        h.frame(&in_world());
        let k = h.ui.layout_scale(screen);
        let plate = hud::menu_rect(screen, k);
        for i in 0..MenuButton::ALL.len() {
            let b = menu_button_rect(screen, k, i);
            assert!(
                b.x >= plate.x + 4.0 * k
                    && b.right() <= plate.right() - 4.0 * k
                    && b.y >= plate.y + 4.0 * k
                    && b.bottom() <= plate.bottom() - 4.0 * k,
                "button {i} {b:?} on the plate {plate:?} with room round it, on {screen:?}"
            );
        }
        // One dark plate, a little translucent, under a fine rim along its edges.
        let ground = h
            .list
            .quads
            .iter()
            .find(|q| q.tex.is_none() && q.dst == plate)
            .expect("the plate drawn");
        let alpha = ground.colour >> 24;
        assert!(
            (0x80..0xFF).contains(&alpha),
            "a little translucent: {alpha:#x}"
        );
        assert!(
            h.list.quads.iter().any(|q| q.tex.is_none()
                && q.colour != ground.colour
                && (q.dst.y - plate.y).abs() < 0.5
                && (q.dst.w - plate.w).abs() < 0.5
                && q.dst.h <= 2.0 * k.max(1.0)),
            "its rim along the top"
        );
    }
}

#[test]
fn with_its_art_the_hud_is_drawn_from_its_pieces_and_fonts() {
    let mut h = Harness::with_built_in_art();
    let mut s = in_world();
    s.health = Vital {
        current: 10,
        max: 20,
    };
    h.frame(&s);
    let textured = h.list.quads.iter().filter(|q| q.tex.is_some()).count();
    assert!(textured > 20, "{textured} textured quads");
}

/// The player in the world as a view gives it, with what the scripts said in `notices` followed.
fn told(s: &mut GameState, notices: &[Notice]) {
    let view = View {
        player_id: Some(PLAYER),
        player: Some(v::Player {
            id: PLAYER,
            ..v::Player::default()
        }),
        inventory: Some(Inventory {
            version: 1,
            player: PLAYER,
            money: 1_234,
            ..Inventory::default()
        }),
        ..View::default()
    };
    s.follow(&Frame {
        view: Some(&view),
        notices,
        events: &[],
        told: &[],
        lines: &[],
        write: &|_| None,
        unknown_command: "",
    });
}

fn about_player(said: Said) -> Notice {
    Notice {
        about: Some(PLAYER),
        said,
    }
}

#[test]
fn the_target_bar_draws_no_frame_round_it_while_the_player_attacks() {
    let mut h = Harness::with_built_in_art();
    let k = h.k();
    let mut s = in_world();
    s.target = Some(Target {
        id: 7,
        name: "Drudge".into(),
        health: Some(Vital {
            current: 30,
            max: 60,
        }),
    });
    let bar = target_gauge_rect(h.screen, k);
    let near = Rect::new(
        bar.x - 6.0 * k,
        bar.y - 6.0 * k,
        bar.w + 12.0 * k,
        bar.h + 12.0 * k,
    );
    let round = |h: &Harness| {
        h.list
            .quads
            .iter()
            .filter(|q| {
                q.dst.x < near.right()
                    && q.dst.right() > near.x
                    && q.dst.y < near.bottom()
                    && q.dst.bottom() > near.y
            })
            .map(|q| (q.tex, q.dst, q.src, q.colour))
            .collect::<Vec<_>>()
    };
    h.frame(&s);
    let at_peace = round(&h);
    assert!(!at_peace.is_empty(), "the gauge drawn");
    let target = s.target.clone();
    told(
        &mut s,
        &[about_player(Said::Combat(CombatNotice::Attacking {
            attacking: true,
        }))],
    );
    assert!(s.combat.attacking);
    // The view told has no world, so nothing in it is selected: the selection put back.
    s.target = target;
    h.frame(&s);
    assert_eq!(round(&h), at_peace, "drawn the same while attacking");
    assert!(fills(&h, ATTACKING).is_empty(), "no red round it");
}

#[test]
fn the_hud_shows_no_money_once_the_pack_is_known() {
    let mut h = Harness::new();
    let mut s = in_world();
    told(&mut s, &[]);
    assert_eq!(s.goods.money(), 1_234, "the money is known");
    h.frame(&s);
    let coins = fills(&h, 0xFFE8_C050);
    assert!(coins.is_empty(), "no coins drawn on the HUD: {coins:?}");
}

#[test]
fn each_menu_bar_button_opens_its_window_lit_and_closes_it_again() {
    let mut h = Harness::new();
    let s = in_world();
    h.frame(&s);
    let k = h.k();
    let lit = |h: &Harness, r: Rect| fills(h, hud::MENU_LIT).contains(&r);
    for (i, b) in MenuButton::ALL.into_iter().enumerate() {
        if b == MenuButton::System {
            continue;
        }
        let r = menu_button_rect(h.screen, k, i);
        let open = |h: &Harness| {
            let ui = &h.ui;
            match b {
                MenuButton::Map => ui.map.is_open(),
                MenuButton::Examine => ui.examine.is_open(),
                MenuButton::Inventory => ui.goods.inventory.is_open(),
                MenuButton::Skills => ui.skills.is_open(),
                MenuButton::Crafting => ui.crafting.is_open(),
                MenuButton::Quests => ui.journal.is_open(),
                MenuButton::Fellowship => {
                    ui.social.is_open() && ui.social.tab == social::FELLOWSHIP
                }
                MenuButton::Allegiance => {
                    ui.social.is_open() && ui.social.tab == social::ALLEGIANCE
                }
                MenuButton::System => unreachable!(),
            }
        };
        assert!(!open(&h), "{b:?} closed at first");
        h.click(&s, centre(r));
        assert!(open(&h), "{b:?} opened");
        // Off the bar, so the pointer lights nothing.
        h.input.mouse = (h.screen.0 / 2.0, h.screen.1 / 2.0);
        h.frame(&s);
        assert!(lit(&h, r), "{b:?} lit while its window is open");
        h.click(&s, centre(r));
        assert!(!open(&h), "{b:?} closed again");
        h.input.mouse = (h.screen.0 / 2.0, h.screen.1 / 2.0);
        h.frame(&s);
        assert!(!lit(&h, r), "{b:?} not lit once closed");
    }
}

#[test]
fn the_fellowship_and_allegiance_buttons_show_the_social_window_on_their_tab() {
    let mut h = Harness::new();
    let s = in_world();
    h.frame(&s);
    let (k, screen) = (h.k(), h.screen);
    let at = |b: MenuButton| {
        let i = MenuButton::ALL.iter().position(|x| *x == b).unwrap();
        centre(menu_button_rect(screen, k, i))
    };
    h.click(&s, at(MenuButton::Fellowship));
    assert_eq!(h.ui.social.tab, social::FELLOWSHIP);
    h.click(&s, at(MenuButton::Allegiance));
    assert!(h.ui.social.is_open(), "still open, on the other tab");
    assert_eq!(h.ui.social.tab, social::ALLEGIANCE);
}

/// Open the system menu and press its row for `item`; what was asked.
fn system(h: &mut Harness, s: &GameState, item: SystemItem) -> auberean_horizon::ui::Outcome {
    let k = h.k();
    let button = menu_button_rect(h.screen, k, MenuButton::ALL.len() - 1);
    h.click(s, centre(button));
    assert!(h.ui.system_menu, "the system menu open");
    let n = SystemItem::ALL.iter().position(|x| *x == item).unwrap();
    let row = system_row_rect(h.screen, k, n);
    assert!(
        h.ui.covers(row.x + 2.0, row.y + 2.0),
        "its rows answer the pointer"
    );
    let out = h.click(s, centre(row));
    assert!(!h.ui.system_menu, "closed once chosen");
    out
}

#[test]
fn the_system_menu_opens_the_settings_and_the_layout_editor_logs_out_and_exits_the_game() {
    let s = in_world();
    let mut h = Harness::new();
    h.frame(&s);
    let out = system(&mut h, &s, SystemItem::Settings);
    assert!(h.ui.settings.is_open());
    assert!(!out.log_out && !out.quit);
    let mut h = Harness::new();
    h.frame(&s);
    system(&mut h, &s, SystemItem::Layout);
    assert!(h.ui.layout.editing());
    let mut h = Harness::new();
    h.frame(&s);
    let out = system(&mut h, &s, SystemItem::LogOut);
    assert!(!out.log_out && !out.quit, "Log Out asks first");
    assert!(h.ui.log_out.is_up());
    let mut h = Harness::new();
    h.frame(&s);
    let out = system(&mut h, &s, SystemItem::Quit);
    assert!(out.quit && !out.log_out, "Exit Game leaves the game");
}

/// The log-out question's Yes and No, as last drawn.
fn answers(h: &Harness) -> (Rect, Rect) {
    let (_, buttons) = h.ui.log_out.drawn().expect("the question drawn");
    (buttons[0], buttons[1])
}

#[test]
fn log_out_on_the_menu_asks_first_no_stays_and_yes_logs_out() {
    let s = in_world();
    let mut h = Harness::new();
    h.frame(&s);
    let out = system(&mut h, &s, SystemItem::LogOut);
    assert!(!out.log_out, "asked, not logged out");
    assert!(h.ui.log_out.is_up());
    assert_eq!(h.ui.log_out.words(), "Are you sure you want to log out?");
    let (yes, no) = answers(&h);
    assert!(
        h.ui.covers(yes.x + 2.0, yes.y + 2.0),
        "its buttons answer the pointer"
    );
    let out = h.click(&s, centre(no));
    assert!(!out.log_out, "No stays");
    assert!(!h.ui.log_out.is_up(), "and takes it down");
    system(&mut h, &s, SystemItem::LogOut);
    h.frame(&s);
    let (yes, _) = answers(&h);
    let out = h.click(&s, centre(yes));
    assert!(out.log_out, "Yes logs out");
    assert!(!h.ui.log_out.is_up());
}

#[test]
fn the_log_out_key_asks_first_and_escape_takes_the_question_down() {
    let s = in_world();
    let mut h = Harness::new();
    h.frame(&s);
    assert!(h.ui.keyed(Keyed::LogOut, &s).is_empty(), "nothing sent");
    assert!(h.ui.log_out.is_up(), "asked");
    let out = h.frame(&s);
    assert!(!out.log_out);
    assert!(h.ui.close_top(&mut Vec::new()), "Escape closes it first");
    assert!(!h.ui.log_out.is_up());
    h.ui.keyed(Keyed::LogOut, &s);
    h.frame(&s);
    let (yes, _) = answers(&h);
    assert!(h.click(&s, centre(yes)).log_out, "Yes logs out");
}

#[test]
fn the_log_out_question_takes_the_game_s_words_when_it_has_them() {
    let s = in_world();
    let mut h = Harness::new();
    h.ui.log_out.text = "Leave now?".into();
    h.ui.log_out.ask();
    h.frame(&s);
    assert_eq!(h.ui.log_out.words(), "Leave now?");
    h.ui.log_out.text.clear();
    assert_eq!(
        h.ui.log_out.words(),
        questions::LOG_OUT_TEXT,
        "else Horizon's own"
    );
}

/// The menu bar button `b`'s icon's piece, the first of its pieces the built-in art has.
fn icon_of(h: &Harness, b: MenuButton) -> &'static str {
    b.icons()
        .iter()
        .copied()
        .find(|n| measured(h, |p| p.piece(n)).is_some())
        .unwrap_or_else(|| panic!("{b:?} has an icon"))
}

#[test]
fn with_its_art_the_examine_and_crafting_buttons_draw_their_own_icons() {
    let mut h = Harness::with_built_in_art();
    let s = in_world();
    h.frame(&s);
    let k = h.k();
    for (b, piece) in [
        (MenuButton::Examine, "menu.examine"),
        (MenuButton::Crafting, "menu.crafting"),
    ] {
        let i = MenuButton::ALL.iter().position(|x| *x == b).unwrap();
        let r = menu_button_rect(h.screen, k, i);
        assert_eq!(icon_of(&h, b), piece);
        assert_eq!(drew_piece(&h, piece), vec![r], "{b:?} draws {piece}");
    }
}

/// Whether the glow of `b`'s icon was drawn round its button.
fn glowing(h: &Harness, b: MenuButton) -> bool {
    let i = MenuButton::ALL.iter().position(|x| *x == b).unwrap();
    let r = menu_button_rect(h.screen, h.k(), i);
    let glow = format!("{}.glow", icon_of(h, b));
    let drawn = drew_piece(h, &glow);
    assert!(drawn.len() <= 1, "{glow} drawn once at most");
    drawn.iter().any(|g| {
        let (gx, gy) = centre(*g);
        let (cx, cy) = centre(r);
        (gx - cx).abs() < 0.5 && (gy - cy).abs() < 0.5 && g.w > r.w && g.h > r.h
    })
}

#[test]
fn with_its_art_each_menu_bar_button_glows_while_its_window_is_open_and_not_once_closed() {
    let mut h = Harness::with_built_in_art();
    let s = in_world();
    let away = (h.screen.0 / 2.0, h.screen.1 / 2.0);
    h.input.mouse = away;
    h.frame(&s);
    let k = h.k();
    for (i, b) in MenuButton::ALL.into_iter().enumerate() {
        assert!(!glowing(&h, b), "{b:?} dark at first");
        let r = menu_button_rect(h.screen, k, i);
        h.click(&s, centre(r));
        h.input.mouse = away;
        if b == MenuButton::System {
            assert!(h.ui.system_menu);
            h.frame(&s);
            assert!(glowing(&h, b), "{b:?} glows while its menu is open");
            h.ui.close_top(&mut Vec::new());
        } else {
            h.frame(&s);
            assert!(glowing(&h, b), "{b:?} glows while its window is open");
            for other in MenuButton::ALL.into_iter().filter(|o| *o != b) {
                assert!(!glowing(&h, other), "{other:?} dark while {b:?} is open");
            }
            h.click(&s, centre(r));
            h.input.mouse = away;
        }
        h.frame(&s);
        assert!(!glowing(&h, b), "{b:?} dark once closed");
    }
    // Under the pointer it glows too.
    let r = menu_button_rect(h.screen, k, 0);
    h.input.mouse = centre(r);
    h.frame(&s);
    assert!(glowing(&h, MenuButton::ALL[0]), "glows under the pointer");
}

#[test]
fn a_press_off_the_system_menu_or_escape_closes_it() {
    let s = in_world();
    let mut h = Harness::new();
    h.frame(&s);
    let button = menu_button_rect(h.screen, h.k(), MenuButton::ALL.len() - 1);
    h.click(&s, centre(button));
    assert!(h.ui.system_menu);
    h.click(&s, (h.screen.0 / 2.0, h.screen.1 / 2.0));
    assert!(!h.ui.system_menu, "a press on the world closes it");
    h.click(&s, centre(button));
    assert!(h.ui.system_menu);
    assert!(h.ui.close_top(&mut Vec::new()), "Escape closes it first");
    assert!(!h.ui.system_menu);
    assert!(!h.ui.settings.is_open());
}

#[test]
fn numbers_are_grouped_in_threes() {
    assert_eq!(hud::grouped(0_u32), "0");
    assert_eq!(hud::grouped(999_u32), "999");
    assert_eq!(hud::grouped(1_000_u32), "1,000");
    assert_eq!(hud::grouped(1_234_567_u64), "1,234,567");
}
