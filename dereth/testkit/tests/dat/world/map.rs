use super::*;

// -------------------------------------------------------------------------------------------
// map.window.*
// -------------------------------------------------------------------------------------------

/// Outdoors, the map window shows the date, the coordinates and the player's own mark, placed
/// where the shipped layout says to put it.
///
/// The premise comes first and is part of the claim: the five elements the window is made of
/// are in the shipped layout and the marker area it scales into is a real box. Every one of the
/// window's writes is guarded on its element being there, so a missing element would be a
/// silent nothing rather than a failure.
pub fn outdoors_the_map_shows_the_date_the_coordinates_and_the_mark() {
    use dereth_ui_screens::mapradar::map::{child, MarkerArea};

    let (mut ui, mut screen) = world_support::shipped_gameplay();
    let g = world_support::as_gameplay(&mut screen);
    let root = *g.roots().first().expect("the gameplay screen has a root");
    let all_five = [
        child::DATE_TIME_TEXT,
        child::MAP_IMAGE,
        child::PLAYER_LOCATION_ICON,
        child::HOUSE_LOCATION_ICON,
        child::COORDINATE_TEXT,
    ]
    .into_iter()
    .all(|id| ui.get_child_recursive(root, id).is_some());
    let all_cached = g.map.date_time_text.is_some()
        && g.map.coordinate_text.is_some()
        && g.map.player_icon.is_some()
        && g.map.house_icon.is_some()
        && g.map.map_image.is_some();
    let area = g.map.marker_area;
    let real_area = area != MarkerArea::default() && area.x1 > area.x0 && area.y1 > area.y0;

    let view = world_support::MapView::outdoors();
    let before = world_support::sample_map(&mut ui, g);
    // The return value is checked after the tree, deliberately: the observable is the three
    // elements, and a build that reports a write it did not make must fail on them and not on
    // its own bookkeeping.
    let reported = g.update_map(&mut ui, &view);
    let after = world_support::sample_map(&mut ui, g);

    let date = !after.date_text.is_empty()
        && after.date_text == world_support::MapView::EXPECTED_DATE_LINE;
    let coords = after.coord_text == world_support::MapView::EXPECTED_COORD_LINE;
    let shown = after.icon_visible;

    // The size of a box is one more than the difference of its edges, which is the client's own
    // arithmetic and was once one short in both the window and the measurement of it.
    let w = before.icon_box.2 - before.icon_box.0 + 1;
    let h = before.icon_box.3 - before.icon_box.1 + 1;
    let (want_x, want_y) = dereth_ui_screens::mapradar::map::place_marker_on_map(
        area,
        world_support::MapView::EAST,
        world_support::MapView::NORTH,
        (w, h),
    );
    let placed = (after.icon_box.0, after.icon_box.1) == (want_x, want_y);
    let inside = (area.x0..=area.x1).contains(&(after.icon_box.0 + w / 2))
        && (area.y0..=area.y1).contains(&(after.icon_box.1 + h / 2));
    // Moving the mark moves it; it does not resize it.
    let same_size = (
        after.icon_box.2 - after.icon_box.0,
        after.icon_box.3 - after.icon_box.1,
    ) == (w - 1, h - 1);
    // The house mark has no valid place, so it stays hidden.
    let no_house = !after.house_visible;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "map.window.outdoors-it-shows-the-date-the-coordinates-and-the-players-own-mark",
        move |_| {
            all_five
                && all_cached
                && real_area
                && reported
                && date
                && coords
                && shown
                && placed
                && inside
                && same_size
                && no_house
        },
    );
}

/// A second look at a world that has not changed writes nothing at all.
///
/// This is what makes every other difference in this section attributable: the window's own
/// guards are "write it only if it is not already what it should be", so a build that rewrote
/// unconditionally would make "the text changed" mean nothing.
pub fn a_second_look_at_an_unchanged_world_writes_nothing() {
    let (mut ui, mut screen) = world_support::shipped_gameplay();
    let g = world_support::as_gameplay(&mut screen);
    let view = world_support::MapView::outdoors();

    let first_wrote = g.update_map(&mut ui, &view);
    let first = world_support::sample_map(&mut ui, g);
    let second_wrote = g.update_map(&mut ui, &view);
    let second = world_support::sample_map(&mut ui, g);
    // And a third, to catch anything that alternates.
    let third_wrote = g.update_map(&mut ui, &view);
    let third = world_support::sample_map(&mut ui, g);

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "map.window.a-second-look-at-an-unchanged-world-writes-nothing",
        move |_| first_wrote && !second_wrote && !third_wrote && first == second && first == third,
    );
}

/// The player's mark follows him, with east to the right and north up.
pub fn the_players_mark_follows_him_east_right_and_north_up() {
    let (mut ui, mut screen) = world_support::shipped_gameplay();
    let g = world_support::as_gameplay(&mut screen);

    let at = |g: &mut dereth_ui_screens::screens::gameplay::GamePlayScreen,
              ui: &mut dereth_ui::UiSystem,
              ns: f32,
              ew: f32| {
        let view = world_support::MapView {
            outside: true,
            coords: Some((ns, ew)),
            date_time: None,
        };
        g.update_map(ui, &view);
        let b = g
            .map
            .player_icon
            .and_then(|h| ui.node(h))
            .expect("the mark")
            .region
            .box_;
        (b.x0, b.y0)
    };
    let centre = at(g, &mut ui, 0.0, 0.0);
    let east = at(g, &mut ui, 0.0, 40.0);
    let west = at(g, &mut ui, 0.0, -40.0);
    let north = at(g, &mut ui, 40.0, 0.0);
    let south = at(g, &mut ui, -40.0, 0.0);

    let signs = east.0 > centre.0 && west.0 < centre.0 && north.1 < centre.1 && south.1 > centre.1;
    // East and west move it sideways only; north and south move it up and down only.
    let axes =
        (east.1, west.1) == (centre.1, centre.1) && (north.0, south.0) == (centre.0, centre.0);

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "map.window.the-players-own-mark-follows-him-with-east-to-the-right-and-north-up",
        move |_| signs && axes,
    );
}

/// Indoors the date keeps going, and only the coordinates and the mark stop.
///
/// "Only outdoors" is two-thirds right, and the third is worth having: the date is written
/// whatever room the player is in, so a player who walks into a dungeon at one time of day does
/// not read that time of day a game-day later.
pub fn indoors_the_date_keeps_going_and_only_the_rest_stops() {
    let (mut ui, mut screen) = world_support::shipped_gameplay();
    let g = world_support::as_gameplay(&mut screen);

    // Outdoors first, so there is something for the indoor pass to have to clear.
    g.update_map(&mut ui, &world_support::MapView::outdoors());
    let outdoors = world_support::sample_map(&mut ui, g);
    let started_right =
        outdoors.coord_text == world_support::MapView::EXPECTED_COORD_LINE && outdoors.icon_visible;

    // Now into a room, with the clock moved on: a different date entirely.
    let inside = world_support::MapView::indoors_later();
    let reported_inside = g.update_map(&mut ui, &inside);
    let indoors = world_support::sample_map(&mut ui, g);
    let date_kept_going = indoors.date_text == world_support::MapView::EXPECTED_LATER_DATE_LINE
        && indoors.date_text != outdoors.date_text;
    // The coordinates are set to nothing rather than hidden, which is where the map differs from
    // the strip beside the radar.
    let coords_blanked = indoors.coord_text.is_empty();
    let mark_hidden = !indoors.icon_visible;

    // And back outside, everything returns. A one-way repair would pass the half above.
    let out_again = g.update_map(&mut ui, &world_support::MapView::outdoors());
    let again = world_support::sample_map(&mut ui, g);
    let restored = out_again
        && again.coord_text == world_support::MapView::EXPECTED_COORD_LINE
        && again.icon_visible
        && again.icon_box == outdoors.icon_box;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "map.window.indoors-the-date-keeps-going-and-only-the-coordinates-and-the-mark-stop",
        move |_| {
            started_right
                && reported_inside
                && date_kept_going
                && coords_blanked
                && mark_hidden
                && restored
        },
    );
}

/// With no clock yet, the date line keeps its two labels and writes a space after each.
///
/// Worth having because "no clock yet" is the state the window is in between entering the world
/// and the region being unpacked, and the window does not blank the line there.
pub fn with_no_clock_yet_the_date_line_keeps_its_labels() {
    let (mut ui, mut screen) = world_support::shipped_gameplay();
    let g = world_support::as_gameplay(&mut screen);
    let view = world_support::MapView {
        outside: true,
        coords: Some((42.2, 33.8)),
        date_time: None,
    };
    g.update_map(&mut ui, &view);
    let text = world_support::sample_map(&mut ui, g).date_text;
    let kept = text == "Date:  \nTime:  ";

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "map.window.with-no-clock-yet-the-date-line-keeps-its-labels",
        move |_| kept,
    );
}

/// The map window is redrawn once every five seconds, and the first look is due at once.
pub fn the_map_is_redrawn_once_every_five_seconds() {
    let (mut ui, mut screen) = world_support::shipped_gameplay();
    let g = world_support::as_gameplay(&mut screen);
    let due_at_once = g.map_update_due(0.0);

    ui.now = dereth_primitives::LocalTime(100.0);
    let still_due = g.map_update_due(100.0);
    g.update_map(&mut ui, &world_support::MapView::outdoors());
    let not_again = !g.map_update_due(100.0);
    let nearly = !g.map_update_due(104.999);
    let due_again = g.map_update_due(105.0);

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "map.window.is-redrawn-once-every-five-seconds-and-the-first-look-is-due-at-once",
        move |_| due_at_once && still_due && not_again && nearly && due_again,
    );
}

// -------------------------------------------------------------------------------------------
// map.page.*
// -------------------------------------------------------------------------------------------

/// Every town the shipped table names is on the map, where the table says and with its name.
///
/// The premise is part of the claim, because without it "there are no towns" and "the layout
/// does not say where to put them" would be the same failure: the map element carries the note
/// element, the note layout and the marker box the page reads out of it.
pub fn every_shipped_town_is_on_the_map_where_the_table_says() {
    use dereth_ui_screens::mapradar::map::{child, notes, MapNote, MAP_PAGE, SHIPPED_NOTE_BINDING};

    let (ui, g) = world_support::shipped_map_page();
    let root = g.root().expect("the gameplay root");
    let map = ui
        .get_child_recursive(root, child::MAP_IMAGE)
        .expect("the map element");
    let element = dereth_ui_screens::bind::attr_enum(&ui, map, 0x47).expect("the note element id");
    let layout = dereth_ui_screens::bind::attr_data_id(&ui, map, 0x48).expect("the note layout id");
    let bound = (element, layout.0) == (SHIPPED_NOTE_BINDING.0, SHIPPED_NOTE_BINDING.1)
        && g.map.marker_area == SHIPPED_NOTE_BINDING.2
        && g.map.page == ui.get_child_recursive(root, MAP_PAGE)
        && g.map.page.is_some();

    let got = world_support::map_notes(&ui, &g);
    let want: Vec<world_support::TownNote> = notes(dereth_primitives::EraId::Eor)
        .map(|m: MapNote| world_support::TownNote {
            // A box is inclusive, so a note `w` wide runs from `x` to `x + w - 1`.
            box_: (m.x, m.y, m.x + m.w - 1, m.y + m.h - 1),
            tip: Some(m.name.to_owned()),
        })
        .collect();
    let all_there = got.len() == 53 && got == want;

    // Every town is a child of the map picture, which is what puts its place in the same space
    // the player's own dot and the marker box are expressed in.
    let map_handle = g.map.map_image.expect("the map element");
    let parented = g.map_notes.iter().all(|h| {
        ui.parent(*h) == Some(map_handle)
            && ui.node(*h).map(dereth_ui::ElementNode::element_id)
                == Some(dereth_ui::ElementId(0x1000_01F0))
    });

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "map.page.every-town-the-shipped-table-names-is-on-the-map-where-it-says",
        move |_| bound && all_there && parented,
    );
}

/// The page draws the map as one picture, once, over the whole of the map element.
///
/// "Exactly once" is what says the map is a single picture rather than a tiled or multi-level
/// one, which is the other half of there being no zoom.
pub fn the_map_is_one_picture_over_the_whole_element() {
    use dereth_ui_screens::mapradar::map::graphic;

    let (mut ui, mut g) = world_support::shipped_map_page();
    world_support::open_the_map_page(&mut ui, &mut g);
    let map = g.map.map_image.expect("the map element");
    let shown = world_support::element_is_visible(&ui, Some(map));
    let want = ui.screen_box(map);
    let sized = (want.width(), want.height()) == (257, 267);

    let mut back = dereth_ui::RecordingDrawBackend::default();
    ui.draw(&mut back);
    let mine: Vec<_> = back.calls.iter().filter(|c| c.who == map).collect();
    let once = mine.len() == 1;
    let right_picture = once && mine[0].image == Some(graphic::MAP_IMAGE);
    let covers = once && mine[0].screen == want;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "map.page.the-map-is-one-picture-drawn-once-over-the-whole-map-element",
        move |_| shown && sized && once && right_picture && covers,
    );
}

/// A map the player has just opened highlights no town at all.
pub fn a_freshly_opened_map_highlights_no_town() {
    use dereth_ui_screens::mapradar::map::{graphic, NOTE_REST_STATE};

    let (mut ui, mut g) = world_support::shipped_map_page();
    world_support::open_the_map_page(&mut ui, &mut g);
    let map_box = ui.screen_box(g.map.map_image.expect("the map element"));

    let mut back = dereth_ui::RecordingDrawBackend::default();
    ui.draw(&mut back);

    let mut ok = !g.map_notes.is_empty();
    let mut edges = 0usize;
    for h in &g.map_notes {
        let b = ui.screen_box(*h);
        ok &= map_box.contains(b.x0, b.y0) && map_box.contains(b.x1, b.y1);
        ok &= ui.node(*h).map(|n| n.state.0) == Some(NOTE_REST_STATE);
        // The one child is the frame around the town; at rest it is hidden.
        ok &= ui
            .children(*h)
            .iter()
            .all(|c| !ui.node(*c).is_some_and(|n| n.region.flags.visible));
        edges += back
            .calls
            .iter()
            .filter(|c| c.image == Some(graphic::NOTE_EDGE) && b.contains(c.screen.x0, c.screen.y0))
            .count();
    }

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "map.page.a-freshly-opened-map-highlights-no-town-at-all",
        move |_| ok && edges == 0,
    );
}

/// Hovering a town frames that town alone, and moving off it un-frames it.
pub fn hovering_a_town_frames_that_town_alone() {
    use dereth_ui_screens::mapradar::map::{graphic, NOTE_REST_STATE, NOTE_ROLLOVER_STATE};

    let (mut ui, mut g) = world_support::shipped_map_page();
    world_support::open_the_map_page(&mut ui, &mut g);

    let note = g.map_notes[0];
    let b = ui.screen_box(note);
    ui.mouse_move(
        dereth_primitives::LocalTime(0.0),
        (b.x0 + b.x1) / 2,
        (b.y0 + b.y1) / 2,
    );
    let over = ui.mouse_over() == Some(note);

    let edges = |ui: &mut dereth_ui::UiSystem| {
        let mut back = dereth_ui::RecordingDrawBackend::default();
        ui.draw(&mut back);
        back.calls
            .iter()
            .filter(|c| c.image == Some(graphic::NOTE_EDGE))
            .count()
    };
    let framed = ui.node(note).map(|n| n.state.0) == Some(NOTE_ROLLOVER_STATE);
    let four = edges(&mut ui) == 4;
    let alone = g
        .map_notes
        .iter()
        .skip(1)
        .all(|h| ui.node(*h).map(|n| n.state.0) == Some(NOTE_REST_STATE));

    // Off the town but still on the map.
    let map_box = ui.screen_box(g.map.map_image.expect("the map element"));
    ui.mouse_move(
        dereth_primitives::LocalTime(1.0),
        map_box.x1 - 1,
        map_box.y1 - 1,
    );
    let unframed = ui.node(note).map(|n| n.state.0) == Some(NOTE_REST_STATE) && edges(&mut ui) == 0;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "map.page.hovering-a-town-frames-that-town-alone-and-leaving-un-frames-it",
        move |_| over && framed && four && alone && unframed,
    );
}

/// Hovering a town shows its name.
///
/// This is the only thing an ordinary player can do on this page, and it asserts the whole
/// chain: a town with no name is not even something the pointer can land on, so the hit test,
/// the hover, the tooltip and the letters in it are all one claim.
pub fn hovering_a_town_shows_its_name() {
    let (mut ui, mut g) = world_support::shipped_map_page();
    world_support::open_the_map_page(&mut ui, &mut g);

    let note = g.map_notes[0];
    let named = ui
        .node(note)
        .and_then(|n| n.tooltip_text.clone())
        .expect("the town has a name");
    let hit_testable = ui.node(note).is_some_and(|n| n.is_mouse_visible);
    let b = ui.screen_box(note);
    ui.mouse_move(
        dereth_primitives::LocalTime(0.0),
        (b.x0 + b.x1) / 2,
        (b.y0 + b.y1) / 2,
    );
    let over = ui.mouse_over() == Some(note);

    // Ten seconds is past any hover delay the shell has.
    ui.check_tooltip(dereth_primitives::LocalTime(10.0));
    let mut back = dereth_ui::RecordingDrawBackend::default();
    ui.draw(&mut back);
    let text: String = back
        .calls
        .iter()
        .flat_map(|c| {
            c.glyphs
                .iter()
                .map(|gl| char::from_u32(u32::from(gl.ch)).unwrap_or('?'))
        })
        .collect();
    let shown = text.contains(&named);

    let mut client = HeadlessClient::model();
    client.assert_behaviour("map.page.hovering-a-town-shows-its-name", move |_| {
        hit_testable && over && shown
    });
}

/// Pressing the map does nothing at all: it does not zoom, it does not move, and it asks the
/// shard for nothing.
pub fn pressing_the_map_does_nothing_for_an_ordinary_player() {
    let (mut ui, mut g) = world_support::shipped_map_page();
    world_support::open_the_map_page(&mut ui, &mut g);
    g.update_map(&mut ui, &world_support::MapView::outdoors());
    ui.requests.clear();

    let map = g.map.map_image.expect("the map element");
    let before = ui.screen_box(map);
    let before_notes = world_support::map_notes(&ui, &g);
    let a = g.map.marker_area;
    // Dead centre of the marker box, in screen space.
    let (mx, my) = (before.x0 + (a.x0 + a.x1) / 2, before.y0 + (a.y0 + a.y1) / 2);

    ui.mouse_move(dereth_primitives::LocalTime(0.0), mx, my);
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, mx, my);
    ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, mx, my, false);
    world_support::pump_screen(&mut ui, &mut g);

    let unmoved = ui.screen_box(map) == before;
    let towns_unmoved = world_support::map_notes(&ui, &g) == before_notes;
    let asked_nothing = ui.requests.take().is_empty();

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "map.page.pressing-the-map-does-nothing-at-all-for-an-ordinary-player",
        move |_| unmoved && towns_unmoved && asked_nothing,
    );
}

/// The player's own dot is where the shipped arithmetic puts it.
///
/// The arithmetic is written out in the support module from the instructions themselves rather
/// than by calling the thing under test, and one position is also worked by hand so that a
/// change to the written-out version cannot quietly follow a change to the client.
pub fn the_players_dot_is_where_the_arithmetic_puts_it() {
    let (mut ui, mut g) = world_support::shipped_map_page();
    world_support::open_the_map_page(&mut ui, &mut g);
    let icon = g.map.player_icon.expect("the player's own mark");

    // The shipped mark is seventeen by sixteen -- the size, not the difference of the edges.
    let b0 = ui.node(icon).expect("the mark").region.box_;
    let sized = (b0.width(), b0.height()) == (17, 16);

    let wrote = g.update_map(&mut ui, &world_support::MapView::outdoors());
    let b = ui.node(icon).expect("the mark").region.box_;
    let a = g.map.marker_area;
    let placed = (b.x0, b.y0)
        == world_support::marker_oracle(
            (a.x0, a.x1, a.y0, a.y1),
            world_support::MapView::EAST,
            world_support::MapView::NORTH,
            17,
            16,
        );
    // The same position, worked by hand from the same instructions with the shipped box.
    let by_hand = (b.x0, b.y0) == (158, 73);
    let shown = world_support::element_is_visible(&ui, Some(icon));
    let inside = (a.x0..=a.x1).contains(&(b.x0 + 8)) && (a.y0..=a.y1).contains(&(b.y0 + 8));

    // And it tracks the player: further east moves it right and nowhere else.
    let east = world_support::MapView {
        outside: true,
        coords: Some((42.2, 60.0)),
        date_time: None,
    };
    g.update_map(&mut ui, &east);
    let e = ui.node(icon).expect("the mark").region.box_;
    let tracks = e.x0 > b.x0 && e.y0 == b.y0;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "map.page.the-players-own-dot-is-where-the-shipped-arithmetic-puts-it",
        move |_| sized && wrote && placed && by_hand && shown && inside && tracks,
    );
}

/// The date and the time are on the page, and drawn rather than merely stored.
pub fn the_date_and_time_are_drawn_on_the_page() {
    let (mut ui, mut g) = world_support::shipped_map_page();
    world_support::open_the_map_page(&mut ui, &mut g);
    let wrote = g.update_map(&mut ui, &world_support::MapView::outdoors());

    let h = g.map.date_time_text.expect("the date line");
    let text = ui
        .text_element_mut(h)
        .expect("a text element")
        .glyphs
        .inq_text(false);
    let right = text == world_support::MapView::EXPECTED_DATE_LINE;

    let mut back = dereth_ui::RecordingDrawBackend::default();
    ui.draw(&mut back);
    let glyphs: String = back
        .calls
        .iter()
        .filter(|c| c.who == h)
        .flat_map(|c| {
            c.glyphs
                .iter()
                .map(|gl| char::from_u32(u32::from(gl.ch)).unwrap_or('?'))
        })
        .collect();
    let drawn = glyphs.contains("Morningthaw");

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "map.page.the-date-and-the-time-are-drawn-on-the-page",
        move |_| wrote && right && drawn,
    );
}

/// Opening the map refreshes it at once, rather than leaving it showing whatever the last look
/// left there; closing it asks for nothing.
pub fn opening_the_map_refreshes_it_at_once() {
    let (mut ui, mut g) = world_support::shipped_map_page();
    ui.now = dereth_primitives::LocalTime(100.0);

    // A look runs while the page is closed; the next one is not due for five seconds.
    g.update_map(&mut ui, &world_support::MapView::outdoors());
    let throttled = !g.map_update_due(ui.now.0) && !g.map_update_due(104.999);

    world_support::open_the_map_page(&mut ui, &mut g);
    let opened = world_support::element_is_visible(&ui, g.map.page);
    let due_at_once = g.map_update_due(ui.now.0);

    // The refresh is real: a position the closed page never saw is on screen after one look.
    let moved = world_support::MapView {
        outside: true,
        coords: Some((10.0, -20.0)),
        date_time: None,
    };
    let wrote = g.update_map(&mut ui, &moved);
    let b = ui
        .node(g.map.player_icon.expect("the mark"))
        .expect("alive")
        .region
        .box_;
    let a = g.map.marker_area;
    let refreshed =
        (b.x0, b.y0) == world_support::marker_oracle((a.x0, a.x1, a.y0, a.y1), -20.0, 10.0, 17, 16);

    // Closing it is the other arm, and it must not ask for an update.
    world_support::close_the_map_page(&mut ui, &mut g);
    let closed = !world_support::element_is_visible(&ui, g.map.page) && !g.map_update_due(ui.now.0);

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "map.page.opening-the-map-refreshes-it-at-once-and-closing-it-asks-for-nothing",
        move |_| throttled && opened && due_at_once && wrote && refreshed && closed,
    );
}
