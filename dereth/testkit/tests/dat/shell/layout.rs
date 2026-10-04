//! Shell fixtures and scenarios for layout.

use super::*;
// ---------------------------------------------------------------------------------------------
// window.layout.*
//
// Where each window sits, and whether it is shown, is kept in the settings record and comes back
// when the screen is rebuilt.
// ---------------------------------------------------------------------------------------------

/// The window bag's own field numbers, as the settings record carries them.
const WINDOW_X: u32 = 0x1000_0086;
const WINDOW_Y: u32 = 0x1000_0087;
const WINDOW_VISIBLE: u32 = 0x1000_008A;
const WINDOW_ROW: u32 = 0x1000_008B;
const WINDOW_TITLE: u32 = 0x1000_008D;

/// The toolbar of the live gameplay screen, and the window number the screen gives it.
fn the_toolbar(c: &mut HeadlessClient) -> (dereth_ui::ElemHandle, u32) {
    with_gameplay(c, |ui, s| {
        (
            ui.get_child_recursive(
                s.root().expect("the gameplay root"),
                dereth_ui_screens::screens::gameplay::window::TOOLBAR,
            )
            .expect("the toolbar"),
            s.window_id_of(dereth_ui_screens::screens::gameplay::window::TOOLBAR),
        )
    })
}

/// A settings record whose window bag puts `window` at `x` and says whether it is shown, with an
/// unrelated window and an entry this client does not model beside it -- both of which have to
/// survive every write.
fn a_module_with_a_window(
    window: u32,
    x: i32,
    visible: bool,
) -> dereth_protocol::login::PlayerModule {
    use dereth_protocol::property::{
        BaseProperty, BasePropertyValue as V, PackObjPropertyCollection, PropertyCollection,
    };

    let field = |name: u32, value: V| {
        (
            name,
            BaseProperty {
                name,
                value: Some(value),
            },
        )
    };
    let mut rows = vec![
        BaseProperty {
            name: WINDOW_ROW,
            value: Some(V::Struct(PropertyCollection::default()))
        };
        window as usize
    ];
    rows[window as usize - 1].value = Some(V::Struct(PropertyCollection {
        bucket_index: 3,
        entries: vec![
            field(WINDOW_X, V::Integer(x)),
            field(WINDOW_Y, V::Integer(125)),
            field(WINDOW_VISIBLE, V::Bool(visible)),
            field(
                WINDOW_TITLE,
                V::StringInfo(dereth_protocol::property::StringInfo {
                    string_id: 1234,
                    ..dereth_protocol::property::StringInfo::default()
                }),
            ),
        ],
    }));
    rows.push(BaseProperty {
        name: WINDOW_ROW,
        value: Some(V::Struct(PropertyCollection {
            bucket_index: 2,
            entries: vec![field(WINDOW_X, V::Integer(-77))],
        })),
    });
    dereth_protocol::login::PlayerModule {
        gameplay_options: Some(PackObjPropertyCollection {
            version: 2,
            properties: PropertyCollection {
                bucket_index: 4,
                entries: vec![
                    field(0x1000_008C, V::Array(rows)),
                    (
                        0x0BAD_F00D,
                        BaseProperty {
                            name: WINDOW_X,
                            value: Some(V::Integer(42)),
                        },
                    ),
                ],
            },
        }),
        ..dereth_protocol::login::PlayerModule::default()
    }
}

/// Hand the client the shard's description of the character.
fn describe(c: &mut HeadlessClient, module: dereth_protocol::login::PlayerModule) {
    use dereth_protocol::login::LoginPlayerDescription;
    c.app_mut().apply_hud_events(&[
        dereth_client_net::client_session::SessionEvent::PlayerDescription(Box::new(
            LoginPlayerDescription {
                player_module: module,
                ..LoginPlayerDescription::default()
            },
        )),
    ]);
}

/// What the retained settings record says about one window.
fn kept_placement(
    c: &HeadlessClient,
    window: u32,
) -> dereth_ui_screens::hud::floaty::WindowPlacement {
    dereth_client::hud::decode_placements(
        c.view()
            .expect_app()
            .objects()
            .world
            .player_system
            .module
            .as_ref()
            .expect("a record"),
    )
    .get(window)
    .expect("that window's row")
    .clone()
}

/// Moving or hiding a window is remembered and comes back when the screen is rebuilt.
pub(super) fn moving_a_window_is_remembered_across_a_rebuild() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(1));
    let (h, id) = the_toolbar(&mut c);
    describe(&mut c, a_module_with_a_window(id, 101, true));
    c.tick(1);
    let sent_before = c.view().expect_app().interaction().stats.requests_sent;

    {
        let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;
        ui.move_to(h, 37, 69);
        // Deliberately far bigger than the layout allows, so what is remembered is what the
        // window really became and not what was asked for.
        ui.resize_to(h, 9999, 9999);
        ui.set_visible(h, false);
    }
    c.tick(1);

    let row = kept_placement(&c, id);
    let drawn = c
        .view()
        .expect_app()
        .ui()
        .expect("shell")
        .ui
        .node(h)
        .expect("live")
        .region
        .box_;
    let remembered = (row.x, row.y) == (Some(37), Some(69))
        && (row.w, row.h) == (Some(drawn.width()), Some(drawn.height()))
        && row.visible == Some(false)
        && row.title
            == Some(dereth_ui_screens::view::ChatWindowTitle::Table {
                string_id: 1234,
                table_id: 0,
            });
    // An unrelated window's row and the entry this client does not model both came through.
    let others_survived = kept_placement(&c, id + 1).x == Some(-77);
    // ...and nothing was sent: moving a window is a local write, kept for the next save.
    let quiet = c.view().expect_app().interaction().stats.requests_sent == sent_before
        && c.view()
            .expect_app()
            .objects()
            .world
            .player_system
            .is_dirty();

    // Through the bytes the client would send, and back: the same placement.
    let round_trips = {
        let packed = c
            .view()
            .expect_app()
            .objects()
            .world
            .player_system
            .client_packed_module()
            .expect("the record");
        let mut w = dereth_protocol::Writer::new();
        packed.write(&mut w).expect("it encodes");
        let bytes = w.into_inner();
        let back =
            dereth_protocol::login::PlayerModule::read(&mut dereth_protocol::Reader::new(&bytes))
                .expect("it decodes");
        dereth_client::hud::decode_placements(&back).get(id) == Some(&row)
    };

    // And the screen rebuilt: the new toolbar comes up where the old one was left, hidden.
    c.app_mut()
        .queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    c.tick(1);
    let gone = c
        .view()
        .expect_app()
        .ui()
        .expect("shell")
        .ui
        .node(h)
        .is_none();
    let (new, _) = the_toolbar(&mut c);
    let came_back = {
        let app = c.view().expect_app();
        let n = app.ui().expect("shell").ui.node(new).expect("live");
        (n.region.box_.x0, n.region.box_.y0) == (37, 69) && !n.region.flags.visible
    };

    c.assert_behaviour("window.layout.moving-or-hiding-a-window-is-remembered-and-comes-back-when-the-screen-is-rebuilt", move |_| {
        remembered && others_survived && quiet && round_trips && gone && came_back
    });
    c.shutdown();
}

/// A change made before the character is described survives, and an older description cannot undo
/// a newer change.
pub(super) fn a_change_before_the_description_survives_it() {
    use dereth_ui_screens::view::UiRequest;

    let mut c = HeadlessClient::new(ClientSpec::gameplay(1));
    let (h, id) = the_toolbar(&mut c);
    // Nothing kept yet: a client with no description of its own does not quietly take the
    // defaults as if the shard had sent them.
    {
        let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;
        ui.move_to(h, 73, 79);
        ui.set_visible(h, false);
    }
    c.tick(1);
    let nothing_kept = c
        .view()
        .expect_app()
        .objects()
        .world
        .player_system
        .module
        .is_none()
        && !c
            .view()
            .expect_app()
            .objects()
            .world
            .player_system
            .is_dirty();

    // A move, then the description arriving, then another move: the last one wins, and the
    // requests the description's arrival passes through are not lost.
    c.ui_outbox().clear();
    c.app_mut().ui_mut().expect("shell").ui.move_to(h, 83, 89);
    let unrelated = vec![
        UiRequest::Select(dereth_primitives::ObjectId(123)),
        UiRequest::SetChatWindowOption {
            window: id,
            property: WINDOW_TITLE,
            value: 4321,
        },
    ];
    for r in unrelated.clone() {
        c.ui_outbox().emit(r);
    }
    describe(&mut c, a_module_with_a_window(id, 131, true));
    let preserved = c.ui_outbox().take();
    let kept_the_others = preserved
        .iter()
        .filter(|r| !dereth_ui_screens::requests::is_numeric_placement_update(r))
        .cloned()
        .collect::<Vec<_>>()
        == unrelated;
    for r in preserved {
        c.ui_outbox().emit(r);
    }
    // This move happens after the description and belongs to it; the older position must not
    // overwrite it.
    c.app_mut().ui_mut().expect("shell").ui.move_to(h, 41, 43);
    c.tick(1);
    let newest_wins = (kept_placement(&c, id).x, kept_placement(&c, id).y) == (Some(41), Some(43))
        && c.view()
            .expect_app()
            .ui()
            .expect("shell")
            .ui
            .node(h)
            .expect("live")
            .region
            .flags
            .visible;

    // A move made just before the screen is rebuilt is still a change, even though the very next
    // frame destroys the element it was made on.
    c.app_mut().ui_mut().expect("shell").ui.move_to(h, 59, 61);
    c.app_mut()
        .queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    c.tick(1);
    let (new, _) = the_toolbar(&mut c);
    let survived_the_rebuild = {
        let app = c.view().expect_app();
        let b = app
            .ui()
            .expect("shell")
            .ui
            .node(new)
            .expect("live")
            .region
            .box_;
        (b.x0, b.y0) == (59, 61)
    } && (kept_placement(&c, id).x, kept_placement(&c, id).y)
        == (Some(59), Some(61));

    // ...and an older description arriving after a newer move does overwrite it, because it is
    // the shard's own answer and the move is not.
    {
        let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;
        ui.move_to(new, 83, 89);
        ui.set_visible(new, true);
    }
    describe(&mut c, a_module_with_a_window(id, 203, false));
    c.tick(1);
    let description_wins = {
        let app = c.view().expect_app();
        let n = app.ui().expect("shell").ui.node(new).expect("live");
        (n.region.box_.x0, n.region.box_.y0) == (203, 125) && !n.region.flags.visible
    } && kept_placement(&c, id).visible == Some(false);

    c.assert_behaviour(
        "window.layout.a-change-made-before-the-character-is-described-survives-it",
        move |_| {
            nothing_kept
                && kept_the_others
                && newest_wins
                && survived_the_rebuild
                && description_wins
        },
    );
    c.shutdown();
}

/// A saved layout is pulled onto the screen and moves only the window it names.
pub(super) fn a_saved_layout_is_clamped_and_moves_only_what_it_names() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(1));
    let (h, id) = the_toolbar(&mut c);
    let (stack, stack_id) = with_gameplay(&mut c, |ui, s| {
        (
            ui.get_child_recursive(
                s.root().expect("the gameplay root"),
                dereth_ui_screens::screens::gameplay::window::PANEL_STACK,
            )
            .expect("the panel stack"),
            s.window_id_of(dereth_ui_screens::screens::gameplay::window::PANEL_STACK),
        )
    });

    // A record that places the panel stack somewhere of its own, so "only the window it names"
    // has something to be true of.
    let mut module = a_module_with_a_window(id, 101, true);
    {
        use dereth_protocol::property::{BaseProperty, BasePropertyValue as V, PropertyCollection};
        let Some(V::Array(rows)) = &mut module
            .gameplay_options
            .as_mut()
            .expect("the bag")
            .properties
            .entries[0]
            .1
            .value
        else {
            panic!("the window array")
        };
        rows[stack_id as usize - 1].value = Some(V::Struct(PropertyCollection {
            bucket_index: 0,
            entries: vec![
                (
                    WINDOW_X,
                    BaseProperty {
                        name: WINDOW_X,
                        value: Some(V::Integer(10)),
                    },
                ),
                (
                    WINDOW_Y,
                    BaseProperty {
                        name: WINDOW_Y,
                        value: Some(V::Integer(11)),
                    },
                ),
            ],
        }));
    }
    describe(&mut c, module);
    c.tick(1);
    {
        let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;
        ui.move_to(stack, 77, 91);
        ui.move_to(h, 37, 43);
    }
    c.tick(1);
    let moved_each_on_its_own = {
        let app = c.view().expect_app();
        let b = app
            .ui()
            .expect("shell")
            .ui
            .node(stack)
            .expect("live")
            .region
            .box_;
        (b.x0, b.y0) == (77, 91)
    } && (
        kept_placement(&c, stack_id).x,
        kept_placement(&c, stack_id).y,
    ) == (Some(10), Some(11));

    // A saved layout of this scenario's own -- never the player's file -- placing the toolbar
    // far off the screen.
    let saved = dereth_ui::persist::ScreenLayout::parse("<TBAR> X:9999 Y: 9999 W: 9999 H: 9999 ")
        .expect("the layout parses");
    let applied = with_gameplay(&mut c, |ui, s| s.load_screen_layout(ui, &saved));
    let clamped = {
        let app = c.view().expect_app();
        let ui = &app.ui().expect("shell").ui;
        let b = ui.node(h).expect("live").region.box_;
        let parent = ui
            .node(ui.parent(h).expect("a parent"))
            .expect("live")
            .region
            .box_;
        applied == 1
            && (b.x0, b.y0)
                == (
                    (parent.width() - b.width()).max(0),
                    (parent.height() - b.height()).max(0),
                )
    };
    c.tick(1);
    let kept = {
        let app = c.view().expect_app();
        let b = app
            .ui()
            .expect("shell")
            .ui
            .node(h)
            .expect("live")
            .region
            .box_;
        let row = kept_placement(&c, id);
        (row.x, row.y, row.w, row.h) == (Some(b.x0), Some(b.y0), Some(b.width()), Some(b.height()))
    };
    let source_untouched = saved.windows[0].1.x == 9999;

    // A smaller display pulls it back again, and that placement is kept too.
    c.app_mut().ui_mut().expect("shell").set_display((640, 480));
    let after = c
        .view()
        .expect_app()
        .ui()
        .expect("shell")
        .ui
        .node(h)
        .expect("live")
        .region
        .box_;
    c.tick(1);
    let followed_the_display =
        (kept_placement(&c, id).x, kept_placement(&c, id).y) == (Some(after.x0), Some(after.y0));

    // ...and a rebuild reads what the window really became.
    c.app_mut()
        .queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    c.tick(1);
    let (new, _) = the_toolbar(&mut c);
    let rebuilt_there = c
        .view()
        .expect_app()
        .ui()
        .expect("shell")
        .ui
        .node(new)
        .expect("live")
        .region
        .box_
        == after;

    c.assert_behaviour(
        "window.layout.a-saved-layout-is-pulled-onto-the-screen-and-moves-only-the-window-it-names",
        move |_| {
            moved_each_on_its_own
                && clamped
                && kept
                && source_untouched
                && followed_the_display
                && rebuilt_there
        },
    );
    c.shutdown();
}

// =============================================================================================
// window.full-screen.* -- the three that need a running client
//
// The other full-screen claims are pure arithmetic over a desktop the scenario makes up, and they
// are in the `cpu` tier's `shell.rs`; these three are here because each of them is about what a
// *client* does with the setting, and a client opens the dats.
//
// **Each owns its `App` outright**, through `adapters_shell::AppSpec` / `build_app`, for two
// reasons the harness cannot give: `ClientSpec` has no way to say what the player's saved
// full-screen setting is -- `AppSpec::full_screen` is -- and the device shadow these read is
// `App::device_state`, which is not on `HeadlessClient`. Both are gaps in the harness.
// =============================================================================================

/// The setting is kept from the start and applied on entering the world, both ways.
pub(super) fn the_full_screen_setting_is_kept_and_applied_on_entering_the_world() {
    // It is read out of the player's own saved settings file in the first place.
    let read_from_the_file = {
        let prefs = dereth_client::config::Preferences::parse("[Display]\r\nFullScreen=True\r\n");
        let mut cfg = dereth_client::config::Config::default();
        cfg.display.full_screen = false;
        cfg.apply_preferences(&prefs);
        cfg.display.full_screen
    };

    // Asked for, and still a window: the patch screen and the character list are windowed.
    let app = build_app(&AppSpec {
        shell: true,
        full_screen: true,
        ..AppSpec::default()
    });
    // (a fresh App's UI starts with an empty request queue of its own)
    let kept = app.config().display.full_screen;
    let windowed_at_the_start = !app.device_state().full_screen;
    let _ = app.shutdown();

    // Entering the world applies it...
    let mut app = build_app(&AppSpec {
        full_screen: true,
        ..AppSpec::in_gameplay(4)
    });
    app.ui_mut()
        .expect("the UI shell is up")
        .ui
        .requests
        .clear();
    let in_the_world = app.device_state().full_screen;

    // ...and leaving takes it away again, which is the same edge in the other direction.
    app.queue_ui_mode(mode::CHARACTER_MANAGEMENT);
    for _ in 0..4 {
        assert!(
            app.frame(),
            "the client shut itself down on the way out of the world"
        );
    }
    let back_to_a_window = !app.device_state().full_screen;
    let setting_survived = app.config().display.full_screen;
    let _ = app.shutdown();

    // And a client that never asked for it is a window in the world too, which is what makes the
    // reading above a measurement.
    let off = build_app(&AppSpec::in_gameplay(4));
    // (a fresh App's UI starts with an empty request queue of its own)
    let never = !off.device_state().full_screen;
    let _ = off.shutdown();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "window.full-screen.the-setting-is-kept-from-the-start-and-applied-on-entering-the-world",
        move |_| {
            read_from_the_file
                && kept
                && windowed_at_the_start
                && in_the_world
                && back_to_a_window
                && setting_survived
                && never
        },
    );
}

/// The switch key does nothing at the character list and switches both ways in the world.
///
/// The gate it is refused by is the client's own, and it is read off a **real** client's device
/// shadow in each of the two places -- the copy is taken from the running client and then driven
/// through the same two calls the window loop makes, which is the only way to press a key that
/// the desktop, and not the client's input manager, owns.
pub(super) fn the_full_screen_switch_key_is_refused_outside_the_world() {
    use dereth_client::pump::window_proc::{finish_event_loop, msg, wnd_proc};

    let app = build_app(&AppSpec {
        shell: true,
        full_screen: true,
        ..AppSpec::default()
    });
    // (a fresh App's UI starts with an empty request queue of its own)
    let mut outside = *app.device_state();
    let _ = app.shutdown();
    outside.is_active_app = true;
    let gate_shut = !outside.allow_full_screen_mode;
    let _ = wnd_proc(&mut outside, msg::WM_SYSKEYDOWN, msg::VK_RETURN, 0);
    // The latch still latches -- the refusal is at the gate and not at the keyboard, which is
    // what "refused rather than honoured and quietly undone" means here.
    let latched = outside.toggle_full_screen_mode;
    finish_event_loop(&mut outside, true);
    let refused = !outside.full_screen && !outside.toggle_full_screen_mode;

    let app = build_app(&AppSpec {
        full_screen: true,
        ..AppSpec::in_gameplay(4)
    });
    // (a fresh App's UI starts with an empty request queue of its own)
    let mut inside = *app.device_state();
    let _ = app.shutdown();
    inside.is_active_app = true;
    let gate_open = inside.allow_full_screen_mode && inside.full_screen;

    let _ = wnd_proc(&mut inside, msg::WM_SYSKEYDOWN, msg::VK_RETURN, 0);
    finish_event_loop(&mut inside, true);
    let turned_off = !inside.full_screen;
    let _ = wnd_proc(&mut inside, msg::WM_SYSKEYDOWN, msg::VK_RETURN, 0);
    finish_event_loop(&mut inside, true);
    let and_back_on = inside.full_screen;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "window.full-screen.the-switch-key-is-refused-outside-the-world-and-works-inside-it",
        move |_| gate_shut && latched && refused && gate_open && turned_off && and_back_on,
    );
}

/// The options page's own tick reaches the window on the next frame, both ways.
pub(super) fn the_options_page_turns_full_screen_on_and_off_mid_session() {
    use dereth_ui_screens::{PrefValue, UiRequest as ScreenRequest};

    let mut app = build_app(&AppSpec::in_gameplay(4));
    app.ui_mut()
        .expect("the UI shell is up")
        .ui
        .requests
        .clear();
    let starts_windowed = !app.device_state().full_screen;

    app.ui_mut()
        .expect("the UI shell is up")
        .ui
        .requests
        .emit(ScreenRequest::SetPreference(
            "Display.FullScreen",
            PrefValue::Bool(true),
        ));
    assert!(
        app.frame(),
        "the client shut itself down on the option's own frame"
    );
    let on = app.device_state().full_screen;
    // ...and it is kept for the next time the client starts, which is the half a device-only
    // reading would miss.
    let saved = app.config().display.full_screen;

    app.ui_mut()
        .expect("the UI shell is up")
        .ui
        .requests
        .emit(ScreenRequest::SetPreference(
            "Display.FullScreen",
            PrefValue::Bool(false),
        ));
    assert!(
        app.frame(),
        "the client shut itself down on the option's own frame"
    );
    let off = !app.device_state().full_screen;
    let _ = app.shutdown();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "window.full-screen.the-options-page-turns-it-on-and-off-while-the-player-plays",
        move |_| starts_windowed && on && saved && off,
    );
}
