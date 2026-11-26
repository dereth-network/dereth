//! The settings window.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use auberean_client_sdk::contract::view::GameOptions;
use auberean_client_sdk::contract::{ChatWindowAction, PlayerOption};
use auberean_client_sdk::Action;
use auberean_horizon::draw::Rect;
use auberean_horizon::ui::options::{self, PREFERENCE_PAGES};
use auberean_horizon::ui::settings::{tab, Controls, Orbit, OrbitChange, Settings};
use auberean_options::page::{section, Change, Listing, Request as PageRequest};
use auberean_options::preference::{Choice as ChoicePref, Flag, Number};
use auberean_shell::interface::{Choice, MovementMode};

use crate::{centre, in_world, Harness};

/// The window open and drawn once; where its controls are.
fn opened(h: &mut Harness) -> Controls {
    h.ui.settings.toggle();
    h.frame(&in_world());
    let body = h.ui.settings.body().expect("drawn");
    Settings::controls(body, h.k())
}

/// The middle of the left (`0`) or right (`1`) choice's mark in a row of two.
fn choice(h: &Harness, row: auberean_horizon::draw::Rect, i: usize) -> (f32, f32) {
    #[allow(clippy::cast_precision_loss)]
    let x = row.x + i as f32 * row.w / 2.0 + 9.0 * h.k();
    (x, row.y + row.h / 2.0)
}

#[test]
fn choosing_retail_in_the_settings_asks_for_it_and_choosing_the_one_shown_asks_nothing() {
    let mut h = Harness::new();
    let c = opened(&mut h);
    let s = in_world();
    let horizon = choice(&h, c.interface, 1);
    assert_eq!(h.click(&s, horizon).choose, None, "Horizon is shown");
    let retail = choice(&h, c.interface, 0);
    assert_eq!(h.click(&s, retail).choose, Some(Choice::Retail));
}

#[test]
fn the_orbit_cameras_preferences_are_shown_as_listed_and_each_set_is_asked_for() {
    let mut h = Harness::new();
    let c = opened(&mut h);
    let s = in_world();
    // Not listed yet: nothing to set.
    assert_eq!(h.click(&s, choice(&h, c.movement, 1)).orbit, None);
    h.ui.orbit = Some(Orbit::default());
    assert_eq!(
        h.click(&s, choice(&h, c.movement, 0)).orbit,
        None,
        "camera-based already"
    );
    assert_eq!(
        h.click(&s, choice(&h, c.movement, 1)).orbit,
        Some(OrbitChange::Movement(MovementMode::Character))
    );
    let k = h.k();
    let box_x = |r: auberean_horizon::draw::Rect| (r.x + 10.0 * k, r.y + r.h / 2.0);
    assert_eq!(
        h.click(&s, box_x(c.reverse_x)).orbit,
        Some(OrbitChange::ReverseX(true))
    );
    h.ui.orbit = Some(Orbit {
        reverse_y: true,
        ..Orbit::default()
    });
    assert_eq!(
        h.click(&s, box_x(c.reverse_y)).orbit,
        Some(OrbitChange::ReverseY(false))
    );
}

#[test]
fn log_out_in_the_settings_asks_first_and_yes_logs_out() {
    let mut h = Harness::new();
    let c = opened(&mut h);
    let s = in_world();
    let out = h.click(&s, (c.log_out.x + 4.0, c.log_out.y + 4.0));
    assert!(!out.log_out, "not before the answer");
    assert!(h.ui.log_out.is_up(), "asked");
    h.frame(&s);
    let (r, buttons) = h.ui.log_out.drawn().cloned().expect("drawn");
    assert!(h.ui.covers(r.x + r.w / 2.0, r.y + 4.0), "over the settings");
    let out = h.click(&s, centre(buttons[0]));
    assert!(out.log_out, "Yes logs out");
    assert!(!h.ui.log_out.is_up());
}

#[test]
fn the_settings_window_covers_the_hud_under_it_and_closes_again() {
    let mut h = Harness::new();
    opened(&mut h);
    let r = h.ui.settings.rect().expect("open");
    assert!(h.ui.covers(r.x + r.w / 2.0, r.y + r.h / 2.0));
    h.ui.settings.toggle();
    h.frame(&in_world());
    assert!(!h.ui.settings.is_open());
    assert!(!h.ui.covers(r.x + r.w / 2.0, r.y + r.h / 2.0));
}

/// The window open on page `tab` and drawn once; where its pages are drawn.
fn page(h: &mut Harness, tab: usize, s: &auberean_horizon::state::GameState) -> Rect {
    h.ui.settings.toggle();
    h.ui.settings.tab = tab;
    h.frame(s);
    Settings::page(h.ui.settings.body().expect("drawn"), h.k())
}

/// The middle of a check box at the start of `r`.
fn check(h: &Harness, r: Rect) -> (f32, f32) {
    (r.x + 10.0 * h.k(), r.y + r.h / 2.0)
}

#[test]
fn each_page_of_the_client_s_preferences_lists_its_sections_while_open() {
    let mut h = Harness::new();
    assert!(h.ui.settings.sections().is_empty(), "closed");
    page(&mut h, 1, &in_world());
    assert_eq!(h.ui.settings.sections(), PREFERENCE_PAGES[0].1);
    h.ui.settings.tab = 2;
    assert_eq!(h.ui.settings.sections(), &[section::SOUND]);
    h.ui.settings.tab = tab::GENERAL;
    assert!(h.ui.settings.sections().is_empty());
}

#[test]
fn a_preference_set_on_its_page_is_tried_applied_and_kept() {
    let mut h = Harness::new();
    let mut flag = Flag::new();
    flag.description.section = section::SOUND;
    flag.description.key = 0x77;
    let mut number = Number::new();
    number.description.key = 0x78;
    h.ui.settings.data.listing = Some(Listing {
        serial: 1,
        sections: vec![(
            section::SOUND,
            vec![Change::Flag(flag), Change::Number(number)],
        )],
    });
    let s = in_world();
    let area = page(&mut h, 2, &s);
    let r = options::row(area, 0.0, 0, h.k());
    h.click(&s, check(&h, r));
    let mut on = flag;
    assert!(on.set_value(!flag.get()));
    assert_eq!(
        h.ui.settings.pages.asked,
        vec![
            PageRequest::Change(Change::Flag(on)),
            PageRequest::Apply(section::SOUND),
            PageRequest::Save
        ]
    );
}

/// A page of the client's preferences that scrolls: a box, then choices and numbers in turn.
pub(crate) fn long_listing() -> Listing {
    let mut rows = Vec::new();
    for i in 0..30u32 {
        rows.push(match i % 3 {
            0 => {
                let mut f = Flag::new();
                f.description.key = 0x100 + i;
                Change::Flag(f)
            }
            1 => {
                let mut c = ChoicePref::new();
                c.description.key = 0x100 + i;
                for (v, words) in [(0, "Low"), (1, "Medium"), (2, "High")] {
                    c.assign_literal_choice(v, words);
                }
                Change::Choice(c)
            }
            _ => {
                let mut n = Number::new();
                n.description.key = 0x100 + i;
                Change::Number(n)
            }
        });
    }
    Listing {
        serial: 1,
        sections: vec![(section::GRAPHICS, rows)],
    }
}

#[test]
fn a_preference_set_on_its_page_shows_as_set_from_the_next_frame() {
    let mut h = Harness::new();
    let mut flag = Flag::new();
    flag.description.section = section::SOUND;
    flag.description.key = 0x77;
    h.ui.settings.data.listing = Some(Listing {
        serial: 1,
        sections: vec![(section::SOUND, vec![Change::Flag(flag)])],
    });
    let s = in_world();
    let area = page(&mut h, 2, &s);
    let r = options::row(area, 0.0, 0, h.k());
    h.click(&s, check(&h, r));
    // The client lists the page again only as it attaches it: the box ticked shows ticked, and a
    // second click clears it rather than ticking it again.
    h.ui.settings.pages.asked.clear();
    h.click(&s, check(&h, r));
    let asked: Vec<bool> =
        h.ui.settings
            .pages
            .asked
            .iter()
            .filter_map(|r| match r {
                PageRequest::Change(Change::Flag(f)) => Some(f.get()),
                _ => None,
            })
            .collect();
    assert_eq!(asked, vec![flag.get()], "cleared again");
}

/// With Horizon's own scrollbar, no row's box, choice or slider on a page that scrolls reaches
/// into the scrollbar's lane: the client's preferences and the player's options alike.
#[test]
fn no_control_on_a_scrolled_page_meets_its_scrollbar() {
    let s = in_world();
    for tab in [1, tab::CHARACTER] {
        let mut h = Harness::with_built_in_art();
        h.ui.settings.data.listing = Some(long_listing());
        h.ui.settings.data.options = Some(GameOptions::default());
        page(&mut h, tab, &s);
        h.frame(&s);
        let pages = &h.ui.settings.pages;
        let bar = pages.bar.expect("the page scrolls");
        assert!(!pages.drawn.is_empty());
        for r in &pages.drawn {
            assert!(
                r.right() <= bar.x,
                "page {tab}: a control to {} meets the bar from {}",
                r.right(),
                bar.x
            );
        }
    }
}

#[test]
fn the_player_s_options_show_as_the_game_keeps_them_and_each_set_is_asked_for() {
    let mut h = Harness::new();
    // Melting without asking is kept as its bit; the radar's items in its mask.
    h.ui.settings.data.options = Some(GameOptions {
        bits: 0x401,
        radar_mask: 0x20,
        shortcut_height: 2,
        ..GameOptions::default()
    });
    let o = h.ui.settings.data.options.clone().expect("set");
    assert_eq!(o.flag(PlayerOption::AskBeforeMelting), Some(false));
    assert_eq!(o.flag(PlayerOption::RadarItems), Some(true));
    assert_eq!(o.flag(PlayerOption::RadarPortals), Some(false));
    let s = in_world();
    let area = page(&mut h, tab::CHARACTER, &s);
    let at = |opt| {
        options::PLAYER_FLAGS
            .iter()
            .position(|(o, _)| *o == opt)
            .expect("listed")
    };
    let melt = options::row(area, 0.0, at(PlayerOption::AskBeforeMelting), h.k());
    let out = h.click(&s, check(&h, melt));
    assert_eq!(
        out.acts,
        vec![Action::SetOptions {
            changes: vec![(PlayerOption::AskBeforeMelting, 1)],
            tell: true
        }]
    );
    let numbers = options::row(area, 0.0, at(PlayerOption::ShortcutNumbers), h.k());
    let out = h.click(&s, check(&h, numbers));
    assert_eq!(
        out.acts,
        vec![Action::SetOptions {
            changes: vec![(PlayerOption::ShortcutNumbers, 0)],
            tell: true
        }]
    );
}

#[test]
fn the_character_page_leaves_out_the_options_horizon_has_no_use_for() {
    use PlayerOption as O;
    let listed = |o: O| options::PLAYER_FLAGS.iter().any(|(at, _)| *at == o);
    for o in [O::ShortcutInstantScroll, O::PlayerKillerDamage] {
        assert!(!listed(o), "{o:?} left to the game's own window");
    }
    for o in [
        O::AutoTarget,
        O::TakeAllegianceOffers,
        O::TakeFellowshipOffers,
        O::TakeTradeOffers,
        O::DamageNumbers,
        O::OthersDamageNumbers,
        O::ShortcutNumbers,
        O::ShortcutLocked,
        O::AskBeforeMelting,
        O::LatencyGraph,
        O::FellowsOnMap,
        O::PetPanel,
        O::ClassicExamination,
        O::RadarMonsters,
        O::RadarPeople,
        O::RadarPlayers,
        O::RadarPortals,
        O::RadarLifestones,
        O::RadarItems,
        O::RadarCoordinates,
    ] {
        assert!(listed(o), "{o:?} listed");
    }
}

#[test]
fn the_chat_page_lists_the_log_window_s_opacities_then_each_window_s_name_opacities_and_kinds() {
    use options::ChatRow;
    let rows = options::chat_rows();
    let kinds = options::CHAT_KINDS.len();
    assert_eq!(
        rows[..3],
        [
            ChatRow::LogWindow,
            ChatRow::Opacity(None, true),
            ChatRow::Opacity(None, false)
        ],
        "the log window's own first"
    );
    let mut at = 3;
    for w in 0..4 {
        assert_eq!(
            rows[at],
            ChatRow::Heading(w),
            "each window under its heading"
        );
        assert_eq!(rows[at + 1], ChatRow::Name(w), "its tab's name first");
        at += 2;
        if w > 0 {
            assert_eq!(
                rows[at..at + 2],
                [
                    ChatRow::Opacity(Some(w), true),
                    ChatRow::Opacity(Some(w), false)
                ],
                "then its opacities popped out"
            );
            at += 2;
        }
        for (i, (kind, _)) in options::CHAT_KINDS.iter().enumerate() {
            assert_eq!(rows[at + i], ChatRow::Kind(w, *kind), "then its kinds");
        }
        at += kinds;
    }
    assert_eq!(
        at,
        rows.len(),
        "no channel to give a window over to, nor a light"
    );
}

/// With Horizon's own scrollbar, no name box, channel box or check box on the chat page reaches
/// into the scrollbar's lane.
#[test]
fn no_control_on_the_chat_page_meets_its_scrollbar() {
    let s = in_world();
    let mut h = Harness::with_built_in_art();
    h.ui.settings.data.options = Some(GameOptions::default());
    page(&mut h, tab::CHAT, &s);
    h.frame(&s);
    let pages = &h.ui.settings.pages;
    let bar = pages.bar.expect("the page scrolls");
    assert!(pages.drawn.len() > 3, "its rows drawn");
    for r in &pages.drawn {
        assert!(
            r.right() <= bar.x,
            "a control to {} meets the bar from {}",
            r.right(),
            bar.x
        );
    }
}

/// Where the chat page's row `line` is, scrolled by `offset` pixels.
fn chat_row(h: &Harness, area: Rect, offset: f32, line: options::ChatRow) -> Rect {
    let i = options::chat_rows()
        .iter()
        .position(|r| *r == line)
        .expect("listed");
    let gutter = crate::measured(h, options::gutter);
    options::page_row(area, offset, i, h.k(), gutter)
}

#[test]
fn a_chat_tab_is_named_on_the_chat_page_and_horizon_keeps_the_name() {
    let mut h = Harness::new();
    h.ui.settings.data.options = Some(GameOptions::default());
    let s = in_world();
    let area = page(&mut h, tab::CHAT, &s);
    let field = options::control(chat_row(&h, area, 0.0, options::ChatRow::Name(0)));
    let _ = h.ui.settings.take_own_changed();
    h.click(&s, centre(field));
    assert!(h.ui.settings.typing(), "the box has the keys");
    assert!(h.ui.typing(), "and the game's keys wait");
    let out = h.type_text(&s, " Log");
    assert!(out.acts.is_empty(), "nothing for the server");
    assert_eq!(h.ui.settings.own.tab_names[0], "Main Log");
    assert!(h.ui.settings.take_own_changed(), "kept");
    let kept = h.ui.settings.own.write();
    assert!(kept.contains("tab-name 1 Main Log\n"), "{kept}");
    assert_eq!(auberean_horizon::own::Own::read(&kept), h.ui.settings.own);
    // Typed away to nothing, it has its own name again.
    for _ in 0.." Main Log".len() {
        h.key(&s, auberean_horizon::ui::vk::BACK);
    }
    assert_eq!(h.ui.settings.own.tab_names[0], "");
    h.key(&s, auberean_horizon::ui::vk::ENTER);
    assert!(!h.ui.settings.typing(), "Enter lets the box go");
    h.frame(&s);
    assert!(!h.ui.settings.own.write().contains("tab-name"));
}

#[test]
fn horizon_s_file_keeps_the_chat_tabs_names_and_where_and_how_large_each_popped_out_was() {
    use auberean_horizon::own::{Own, TAB_NAME_MAX};
    let mut own = Own::default();
    own.set_tab_name(1, "  Loot and Trade  ");
    own.set_tab_name(3, &"x".repeat(40));
    own.chat_popped[2] = Some((40.5, 120.0));
    own.chat_popped_size[2] = Some((320.0, 200.0));
    own.chat_opacity[0] = (0.75, 0.25);
    own.chat_opacity[2].1 = 0.5;
    assert_eq!(own.tab_names[1], "Loot and Trade");
    assert_eq!(own.tab_names[3].len(), TAB_NAME_MAX);
    let kept = own.write();
    assert!(kept.contains("tab-name 2 Loot and Trade\n"), "{kept}");
    assert!(kept.contains("chat-popped 3 40.5 120\n"), "{kept}");
    assert!(kept.contains("chat-popped-size 3 320 200\n"), "{kept}");
    assert!(kept.contains("chat-opacity 1 0.75 0.25\n"), "{kept}");
    assert!(kept.contains("chat-opacity 3 0.5647059 0.5\n"), "{kept}");
    assert!(!kept.contains("chat-opacity 2"), "{kept}");
    assert_eq!(Own::read(&kept), own);
    let odd = Own::read(
        "tab-name 5 Nowhere\ntab-name 0 Zero\nchat-popped 1 4 4\nchat-popped 2 oops 4\n\
         chat-popped-size 2 -1 50\nchat-opacity 5 1 1\nchat-opacity 2 0.5\n",
    );
    assert_eq!(
        odd,
        Own::default(),
        "no fifth tab, the first never pops out, numbers that read"
    );
}

#[test]
fn a_chat_window_s_box_sets_its_filter_and_the_chat_options_are_kept() {
    let mut h = Harness::new();
    let mut o = GameOptions::default();
    o.chat.filters = [0x1, 0x2, 0x4, 0x8];
    h.ui.settings.data.options = Some(o);
    let s = in_world();
    let area = page(&mut h, tab::CHAT, &s);
    let k = h.k();
    let say = options::chat_rows()
        .iter()
        .position(|r| *r == options::ChatRow::Kind(1, 0x10))
        .expect("listed");
    // Scrolled down to the second window's sayings, a row under the page's top.
    #[allow(clippy::cast_precision_loss)]
    let offset = (say - 1) as f32 * options::ROW * k;
    h.input.mouse = centre(area);
    h.input.wheel = -offset / (48.0 * k);
    h.frame(&s);
    let b = options::chat_check(options::row(area, offset, say, k), k);
    let out = h.click(&s, centre(b));
    assert_eq!(
        out.acts,
        vec![
            Action::ChatWindows(ChatWindowAction::Filter {
                window: 2,
                filter: 0x12
            }),
            Action::ChatWindows(ChatWindowAction::Keep),
        ]
    );
}

#[test]
fn an_opacity_is_set_on_the_chat_page_and_kept_in_horizon_s_own_settings() {
    let mut h = Harness::new();
    h.ui.settings.data.options = Some(GameOptions::default());
    let s = in_world();
    let area = page(&mut h, tab::CHAT, &s);
    let _ = h.ui.settings.take_own_changed();
    let slider = options::control(chat_row(
        &h,
        area,
        0.0,
        options::ChatRow::Opacity(None, false),
    ));
    let out = h.click(&s, (slider.x + 1.0, slider.y + slider.h / 2.0));
    assert!(out.acts.is_empty(), "nothing for the server");
    let (active, inactive) = h.ui.settings.own.chat_opacity[0];
    assert!(inactive < 0.05, "all but clear: {inactive}");
    assert_eq!(
        active,
        auberean_horizon::own::CHAT_OPACITY.0,
        "the other as it was"
    );
    assert!(h.ui.settings.take_own_changed(), "kept");
    let kept = h.ui.settings.own.write();
    assert!(kept.contains("chat-opacity 1 "), "{kept}");
    assert_eq!(auberean_horizon::own::Own::read(&kept), h.ui.settings.own);
}

#[test]
fn an_old_file_with_the_tabs_lights_still_reads_and_they_are_not_written_again() {
    use auberean_horizon::own::Own;
    let old = Own::read("scale auto\nvista random\ntab-lights off on on on\nmap-follow off\n");
    let want = Own {
        map_follow: false,
        ..Own::default()
    };
    assert_eq!(old, want, "the rest read as written");
    assert!(!old.write().contains("tab-lights"));
}

/// The key bindings page open on the Movement group, Horizon's own layout in it, drawn once.
fn key_page(h: &mut Harness, s: &auberean_horizon::state::GameState) -> Rect {
    h.ui.settings.bindings.maps = Some(auberean_horizon::ui::bindings::defaults());
    let area = page(h, tab::KEYS, s);
    h.frame(s);
    area
}

/// Where the key buttons of `action`'s row were drawn.
fn slots(h: &Harness, action: u32) -> [Rect; auberean_horizon::ui::bindings::KEY_SLOTS] {
    h.ui.settings
        .bindings
        .drawn
        .iter()
        .find(|(a, _)| *a == action)
        .map(|(_, r)| *r)
        .expect("the action's row drawn")
}

const FORWARD: u32 = 0x4000_0020;
const STOP: u32 = 0x4000_00A3;
const STRAFE_LEFT: u32 = 0x4000_0022;

#[test]
fn a_key_pressed_for_one_of_an_actions_keys_binds_it_there_and_apply_asks_for_the_maps() {
    use auberean_horizon::ui::bindings::{self, Bindings};
    let w = auberean_shell::input::Key::W.input_word();
    let q = auberean_shell::input::Key::Q.input_word();
    assert_eq!(bindings::key_name(w), "W");
    assert_eq!(bindings::keys_of(&bindings::defaults(), FORWARD), ["W"]);
    let mut h = Harness::new();
    let s = in_world();
    let area = key_page(&mut h, &s);
    h.click(&s, centre(slots(&h, FORWARD)[0]));
    assert_eq!(h.ui.settings.bindings.capturing, Some(FORWARD));
    // Q, Side Step Left's key, now runs forward in W's place; W runs nothing; Side Step Left has
    // none.
    h.ui.settings.bindings.bind(q, 0);
    let maps = h.ui.settings.bindings.maps.clone().expect("edited");
    assert_eq!(bindings::keys_of(&maps, FORWARD), ["Q"]);
    assert!(bindings::keys_of(&maps, STRAFE_LEFT).is_empty());
    assert!(!maps[0].iter().any(|(k, _)| *k == w));
    h.click(&s, centre(Bindings::parts(area, h.k()).apply));
    assert_eq!(h.ui.settings.bindings.applied, Some(maps));
    // Closing lets go of the edits.
    h.ui.settings.close();
    assert_eq!(h.ui.settings.bindings.maps, None);
}

#[test]
fn a_changed_row_is_marked_and_restore_defaults_puts_horizons_own_layout_back_to_apply() {
    use auberean_horizon::ui::bindings::{self, Bindings};
    let q = auberean_shell::input::Key::Q.input_word();
    let mut h = Harness::new();
    let s = in_world();
    let area = key_page(&mut h, &s);
    let defaults = bindings::defaults();
    assert!(
        bindings::actions()
            .iter()
            .all(|a| !bindings::changed(&defaults, *a)),
        "Horizon's own layout marks no row"
    );
    // Q runs forward: Forward's row and Side Step Left's, which lost Q, are marked; Jump's is not.
    h.click(&s, centre(slots(&h, FORWARD)[0]));
    h.ui.settings.bindings.bind(q, 0);
    let maps = h.ui.settings.bindings.maps.clone().expect("edited");
    assert!(bindings::changed(&maps, FORWARD));
    assert!(bindings::changed(&maps, STRAFE_LEFT));
    assert!(!bindings::changed(&maps, 0x4000_0026));
    // Applied, then Restore Defaults: every row Horizon's own again, to apply.
    h.click(&s, centre(Bindings::parts(area, h.k()).apply));
    assert_eq!(h.ui.settings.bindings.applied.take(), Some(maps));
    h.frame(&s);
    h.click(&s, centre(Bindings::parts(area, h.k()).defaults));
    assert_eq!(h.ui.settings.bindings.maps, Some(defaults.clone()));
    assert!(h.ui.settings.bindings.changed, "Apply lit");
    h.frame(&s);
    h.click(&s, centre(Bindings::parts(area, h.k()).apply));
    assert_eq!(h.ui.settings.bindings.applied, Some(defaults));
}

#[test]
fn an_empty_key_button_adds_a_key_a_right_click_takes_one_off_and_cancel_keeps_the_keys() {
    use auberean_horizon::ui::bindings;
    let up = auberean_shell::input::Key::ArrowUp.input_word();
    let mut h = Harness::new();
    let s = in_world();
    key_page(&mut h, &s);
    // Beside W: Shift+Up as well.
    h.click(&s, centre(slots(&h, FORWARD)[1]));
    assert_eq!(
        h.ui.settings.bindings.replacing, None,
        "added, not in a key's place"
    );
    h.ui.settings.bindings.bind(up, bindings::METAS[1]);
    let maps = h.ui.settings.bindings.maps.clone().expect("edited");
    assert_eq!(bindings::keys_of(&maps, FORWARD), ["W", "Shift+Up"]);
    // A right click on W takes it off.
    h.right_click(&s, centre(slots(&h, FORWARD)[0]));
    let maps = h.ui.settings.bindings.maps.clone().expect("edited");
    assert_eq!(bindings::keys_of(&maps, FORWARD), ["Shift+Up"]);
    // Waiting for a key, the box over the page asks for it; its Cancel leaves the keys alone.
    h.click(&s, centre(slots(&h, STOP)[0]));
    assert_eq!(h.ui.settings.bindings.capturing, Some(STOP));
    h.frame(&s);
    let cancel = h.ui.settings.bindings.cancel_drawn.expect("the box asks");
    h.click(&s, centre(cancel));
    assert_eq!(h.ui.settings.bindings.capturing, None);
    assert_eq!(h.ui.settings.bindings.maps, Some(maps), "as they were");
}

#[test]
fn the_key_page_shows_the_group_chosen_in_its_box_and_only_its_actions() {
    use auberean_horizon::ui::bindings::{self, Bindings, GROUPS};
    let mut h = Harness::new();
    let s = in_world();
    let area = key_page(&mut h, &s);
    let shown = |h: &Harness| -> Vec<usize> {
        h.ui.settings
            .bindings
            .drawn
            .iter()
            .map(|(a, _)| bindings::group_of(*a))
            .collect()
    };
    assert!(!shown(&h).is_empty());
    assert!(shown(&h).iter().all(|g| *g == 0), "Movement first");
    let group = Bindings::parts(area, h.k()).group;
    h.click(&s, centre(group));
    let camera = GROUPS.iter().position(|g| *g == "Camera").expect("a group");
    let list = crate::measured(&h, |p| {
        dereth_ui_kit::widgets::dropdown_list_rect(p, group, GROUPS.len())
    });
    #[allow(clippy::cast_precision_loss)]
    let row = list.y + 3.0 + (camera as f32 + 0.5) * group.h.max(24.0 * h.k());
    h.click(&s, (list.x + list.w / 2.0, row));
    h.frame(&s);
    assert_eq!(h.ui.settings.bindings.group, camera);
    assert!(!shown(&h).is_empty());
    assert!(shown(&h).iter().all(|g| *g == camera), "{:?}", shown(&h));
    // Every group has actions, and every action a group of its own name.
    for (g, name) in GROUPS.iter().enumerate() {
        assert!(
            bindings::actions()
                .iter()
                .any(|a| bindings::group_of(*a) == g),
            "{name} is empty"
        );
    }
}

/// With Horizon's own scrollbar, a group long enough to scroll draws no key button into the
/// scrollbar's lane.
#[test]
fn no_key_button_meets_the_key_pages_scrollbar() {
    use auberean_horizon::ui::bindings::GROUPS;
    let mut h = Harness::with_built_in_art();
    let s = in_world();
    h.ui.settings.bindings.group = GROUPS
        .iter()
        .position(|g| *g == "Shortcuts")
        .expect("a group");
    key_page(&mut h, &s);
    let b = &h.ui.settings.bindings;
    let bar = b.bar.expect("the shortcuts scroll");
    for (_, slots) in &b.drawn {
        for r in slots {
            assert!(
                r.right() <= bar.x,
                "{} meets the bar at {}",
                r.right(),
                bar.x
            );
        }
    }
}

#[test]
fn every_action_on_the_key_bindings_page_has_a_name() {
    use auberean_horizon::ui::bindings;
    for a in bindings::actions() {
        let name = bindings::action_name(a);
        assert!(!name.starts_with("Action "), "{a:#x} reads {name}");
    }
    assert_eq!(bindings::action_name(0x4000_0020), "Walk Forward");
    let num_7 = auberean_shell::input::Key::W.input_word() - 0x11 + 0x47;
    assert_eq!(bindings::key_name(num_7), "Num 7");
}

#[test]
fn the_settings_tabs_and_the_key_pages_buttons_fit_their_words() {
    use auberean_horizon::ui::bindings::Bindings;
    use auberean_horizon::ui::settings::TABS;
    use dereth_ui_kit::widgets as kit;
    let mut h = Harness::with_built_in_art();
    let s = in_world();
    let area = page(&mut h, tab::KEYS, &s);
    let body = h.ui.settings.body().expect("drawn");
    let k = h.k();
    let bar = Settings::tab_bar(body, k);
    let parts = Bindings::parts(area, k);
    let (apply, defaults) = (parts.apply, parts.defaults);
    crate::measured(&h, |p| {
        let natural: Vec<f32> = kit::tab_rects(p, bar, &TABS).iter().map(|r| r.w).collect();
        for selected in 0..TABS.len() {
            let drawn: Vec<f32> = kit::tab_rects_for(p, bar, &TABS, selected)
                .iter()
                .map(|r| r.w)
                .collect();
            assert_eq!(drawn, natural, "each tab as wide as its name");
        }
        for (r, label) in [(defaults, "Restore Defaults"), (apply, "Apply")] {
            assert!(
                kit::button_label_width(p, label) <= r.w - 12.0 * k,
                "{label} fits its button"
            );
        }
    });
}
