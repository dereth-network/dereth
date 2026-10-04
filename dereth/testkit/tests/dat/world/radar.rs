use super::*;

// -------------------------------------------------------------------------------------------
// radar.*
// -------------------------------------------------------------------------------------------

/// Only what the shard marks for the radar reaches it, and that throws most of the scene away.
///
/// The wanted set is read out of each recording's own descriptions rather than out of the thing
/// under test, so the two can disagree.
pub fn only_what_the_shard_marks_reaches_the_radar() {
    use dereth_primitives::ObjectId;
    use dereth_ui_screens::mapradar::radar::inq_showable_on_radar;

    let mut measured = 0usize;
    let mut ok = true;
    for session in world_support::recordings_with_a_world() {
        let objects = world_support::replay_objects_at_peak(session);
        let Some(player) = objects.world.player else {
            continue;
        };

        let mut want: std::collections::BTreeSet<ObjectId> = std::collections::BTreeSet::new();
        let mut total = 0usize;
        for (id, presence) in objects.presences() {
            let Some(w) = objects.world.weenie(id) else {
                continue;
            };
            total += 1;
            // The three values that mean "show this", and nothing else -- an object the shard
            // said nothing about reads as "undefined" and never reaches the radar.
            let showable = matches!(w.pwd.radar_enum.unwrap_or(0), 2..=4);
            if showable && presence.position.is_some() && id != player {
                want.insert(id);
            }
        }
        if total == 0 {
            continue;
        }

        let list = world_support::radar_list(&objects);
        let got: std::collections::BTreeSet<ObjectId> = list
            .iter()
            .filter(|o| !o.is_self && inq_showable_on_radar(o))
            .map(|o| o.id)
            .collect();
        ok &= got == want;
        // ...and it is a filter and not a no-op: most of the scene is thrown away.
        ok &= want.len() * 2 < total;
        println!(
            "radar: {session}: {} of {total} objects reach the radar",
            want.len()
        );
        measured += 1;
    }

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.filter.only-what-the-shard-marks-for-the-radar-reaches-it",
        move |_| measured >= 3 && ok,
    );
}

/// The blips drawn are exactly the showable objects inside the radar's own range.
///
/// This is deliberately a separate claim from the filter itself: a filter that is right is worth
/// nothing if the drawing never consults it.
pub fn the_blips_are_the_showable_objects_inside_the_range() {
    use dereth_primitives::ObjectId;
    use dereth_ui_screens::mapradar::radar::{draw_objects, inq_showable_on_radar};

    const RANGE: f32 = 75.0;
    let geom = world_support::radar_geometry();
    let range_sq = (RANGE - 1.0) * (RANGE - 1.0);
    let mut ok = true;
    let mut measured = 0usize;
    let mut with_a_cut = 0usize;
    for session in world_support::recordings_with_a_world() {
        let objects = world_support::replay_objects_at_peak(session);
        let Some(player) = objects.world.player else {
            continue;
        };
        let list = world_support::radar_list(&objects);

        let mut want: std::collections::BTreeSet<ObjectId> = std::collections::BTreeSet::new();
        for o in &list {
            let Some(w) = objects.world.weenie(o.id) else {
                continue;
            };
            if !matches!(w.pwd.radar_enum.unwrap_or(0), 2..=4) || o.id == player || !o.in_world {
                continue;
            }
            let (px, py, _) = o.player_space;
            if px * px + py * py >= range_sq {
                continue;
            }
            want.insert(o.id);
        }

        let blips = draw_objects(&list, None, geom, RANGE, None, false);
        let drawn: std::collections::BTreeSet<ObjectId> =
            blips.iter().map(|b| list[b.index].id).collect();
        ok &= drawn == want;
        ok &= !drawn.contains(&player);
        // At least one recording must really exercise the range, or this only restates the
        // filter.
        let showable = list
            .iter()
            .filter(|o| !o.is_self && inq_showable_on_radar(o))
            .count();
        if want.len() < showable {
            with_a_cut += 1;
        }
        measured += 1;
    }

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.blips.are-exactly-the-things-the-radar-shows-that-are-inside-its-range",
        move |_| measured >= 3 && with_a_cut > 0 && ok,
    );
}

/// The filter is the difference between a crowded radar and a readable one.
///
/// The same scene with the filter taken out is drawn beside it; no count is written down here.
pub fn the_filter_is_the_difference_between_a_crowd_and_a_handful() {
    use dereth_ui_screens::mapradar::radar::draw_objects;
    use dereth_ui_screens::view::RadarEntry;

    let geom = world_support::radar_geometry();
    let mut ok = true;
    let mut measured = 0usize;
    for session in world_support::recordings_with_a_world() {
        let objects = world_support::replay_objects_at_peak(session);
        let list = world_support::radar_list(&objects);
        if list.is_empty() {
            continue;
        }
        let after = draw_objects(&list, None, geom, 75.0, None, false).len();
        // The same scene with nothing filtered out at all.
        let unfiltered: Vec<RadarEntry> = list
            .iter()
            .copied()
            .map(|mut o| {
                o.radar_enum = 4;
                o.is_self = false;
                o
            })
            .collect();
        let before = draw_objects(&unfiltered, None, geom, 75.0, None, false).len();
        println!("radar: {session}: {before} things in view, {after} on the radar");
        ok &= after < before;
        measured += 1;
    }

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.filter.is-the-difference-between-a-crowded-radar-and-a-readable-one",
        move |_| measured >= 3 && ok,
    );
}

/// Every blip takes the colour its own description asks for.
///
/// Each object is coloured twice -- once by the client, once by a reading of the recording's own
/// description written out separately -- and the two must agree about every object in every
/// recording.
pub fn every_blip_takes_the_colour_its_description_asks_for() {
    use dereth_ui_screens::mapradar::radar::{get_blip_color, inq_showable_on_radar};

    let mut checked = 0usize;
    let mut ok = true;
    for session in world_support::recordings_with_a_world() {
        let objects = world_support::replay_objects_at_peak(session);
        for o in &world_support::radar_list(&objects) {
            if o.is_self || !inq_showable_on_radar(o) {
                continue;
            }
            let Some(w) = objects.world.weenie(o.id) else {
                continue;
            };
            let got = get_blip_color(Some(o)).hex;
            let want = world_support::expected_blip_colour(&w.pwd);
            if got != want {
                println!(
                    "radar: {session} {:?}: {got:#08X} against {want:#08X}",
                    o.id
                );
            }
            ok &= got == want;
            checked += 1;
        }
    }
    println!("radar: {checked} things on the radar coloured across the recordings");

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.blips.every-one-takes-the-colour-its-own-description-asks-for",
        move |_| checked > 50 && ok,
    );
}

/// The only things on the radar that take no colour of their own are ordinary players.
///
/// A radar of plain white crosses where retail draws coloured ones is the failure this guards,
/// so a thing that takes the plain colour is exactly a thing every rule missed -- **except** an
/// ordinary player at peace, whom the client's own last branch leaves plain on purpose. No
/// creature, trader, portal or item may reach it. More than one colour must appear, or a radar
/// that painted everything alike would satisfy this too.
///
/// "Not one at all" would be wrong: three of the eighteen recordings carry other players.
pub fn no_blip_falls_through_to_the_colour_that_means_nothing() {
    use dereth_ui_screens::mapradar::radar::{
        get_blip_color, inq_showable_on_radar, semantic, YELLOW,
    };

    let mut all: std::collections::BTreeMap<u32, usize> = std::collections::BTreeMap::new();
    let mut white = 0usize;
    let mut plain_players = 0usize;
    let mut measured = 0usize;
    for session in world_support::recordings_with_a_world() {
        let objects = world_support::replay_objects_at_peak(session);
        for o in &world_support::radar_list(&objects) {
            if o.is_self || !inq_showable_on_radar(o) {
                continue;
            }
            let c = get_blip_color(Some(o)).hex;
            *all.entry(c).or_default() += 1;
            if c == semantic::DEFAULT.hex {
                // Who it is matters: the plain colour is what the client's own last branch gives
                // an ordinary player at peace, and is not what anything else may end up with.
                let is_player = objects
                    .world
                    .weenie(o.id)
                    .is_some_and(|w| w.pwd.bitfield & 0x0000_0008 != 0);
                if is_player {
                    plain_players += 1;
                } else {
                    white += 1;
                    println!("radar: {session} {:?} took no colour of its own", o.id);
                }
            }
        }
        measured += 1;
    }
    println!(
        "radar: the recordings' blip colours are {all:?}; {plain_players} of them are ordinary players taking the plain one"
    );
    let enough_colours = all.len() >= 4;
    // The colour the shard's own marked traders take, which is what a real radar is mostly made
    // of.
    let yellow = all.contains_key(&YELLOW.hex);

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.blips.the-only-things-that-take-no-colour-of-their-own-are-ordinary-players",
        move |_| measured >= 3 && white == 0 && plain_players > 0 && enough_colours && yellow,
    );
}

/// The player's own place is nine bright-green points on the centre of the ring.
///
/// He is not one of the blips at all -- the radar's own list never holds him -- so the mark
/// could never have appeared by repairing the blips.
pub fn the_players_mark_is_nine_bright_green_points_on_the_centre() {
    use dereth_ui_screens::mapradar::radar::{
        center_marker_fills, center_marker_pixels, CENTER_MARKER_COLOR,
    };

    let px = center_marker_pixels();
    let shape = px
        == vec![
            (0, 0),
            (0, -1),
            (0, 1),
            (-1, 0),
            (1, 0),
            (-2, 0),
            (2, 0),
            (0, -2),
            (0, 2),
        ];
    let colour = CENTER_MARKER_COLOR.hex == 0x00FF00
        && CENTER_MARKER_COLOR == dereth_ui_screens::mapradar::radar::BRIGHT_GREEN;

    let geom = world_support::radar_geometry();
    let fills = center_marker_fills(geom);
    #[allow(clippy::cast_possible_truncation)]
    let cx = geom.center.0 as i32;
    #[allow(clippy::cast_possible_truncation)]
    let cy = geom.center.1 as i32;
    let placed = fills.len() == 9
        && fills.iter().zip(px).all(|(f, (dx, dy))| {
            (f.x, f.y) == (cx + dx, cy + dy) && (f.w, f.h) == (1, 1) && f.color == 0xFF00_FF00
        });

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.player.his-own-place-is-nine-bright-green-points-on-the-centre",
        move |_| shape && colour && placed,
    );
}

/// The radar's padlock is up from the first frame, drawing its open picture.
///
/// The picture is read out of the shipped layout rather than written down here, and it is the
/// picture that is asserted rather than the bookkeeping behind it -- asserting the bookkeeping
/// is what once let this pass while the corner of the ring stayed blank.
pub fn the_radars_padlock_is_up_and_open_from_the_first_frame() {
    use dereth_ui_screens::mapradar::radar::{child, lock_state};
    use dereth_ui_screens::screens::gameplay::window;

    let (ui, screen) = world_support::shipped_gameplay();
    let root = *screen.roots().first().expect("the screen has a root");
    let radar = ui
        .get_child_recursive(root, window::RADAR)
        .expect("the radar");

    let lock = ui
        .get_child_recursive(radar, child::LOCK_BUTTON)
        .expect("the padlock");
    let drag = ui
        .get_child_recursive(radar, child::DRAG_BUTTON)
        .expect("the drag handle");
    let up = ui
        .node(lock)
        .expect("the padlock is alive")
        .region
        .flags
        .visible
        && !ui
            .node(drag)
            .expect("the handle is alive")
            .region
            .flags
            .visible;

    let node = ui.node(lock).expect("the padlock is alive");
    let media_of = |s: u32| -> dereth_primitives::DataId {
        let sd = node
            .desc
            .access_state(dereth_ui::StateId(s))
            .unwrap_or_else(|| panic!("the shipped layout gives the padlock no state {s:#x}"));
        match sd.media.first().map(|m| &m.fields) {
            Some(dereth_ui::desc::state_desc::MediaFields::Image { file, .. }) => *file,
            other => panic!("that state carries {other:?}, not a picture"),
        }
    };
    let locked = media_of(lock_state::LOCKED);
    let unlocked = media_of(lock_state::UNLOCKED);
    let two_pictures = locked != unlocked;
    let open = node.region.image.as_ref().map(|g| g.did) == Some(unlocked);

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.padlock.is-up-and-drawing-its-open-picture-from-the-first-frame",
        move |_| up && two_pictures && open,
    );
}

/// The player's mark is laid on the radar's own element, after the blips and in the order the
/// client issues them.
///
/// The whole seam in one scenario: a recording, the object stream, the heads-up display's own
/// view of it, and the fills that end up on the element.
pub fn the_players_mark_is_laid_on_the_radar_element() {
    use dereth_ui_screens::mapradar::radar::{center_marker_fills, RadarGeometry};
    use dereth_ui_screens::screens::gameplay::window;

    let (mut ui, mut screen) = world_support::shipped_gameplay();
    let session = world_support::the_busiest_recording();
    let objects = world_support::replay_objects_at_peak(session);
    let player = objects.world.player.expect("the recording named a player");
    let pos = objects
        .presence(player)
        .and_then(|p| p.position)
        .expect("the player has a place");
    let mut hud = dereth_client::hud::Hud::new();
    hud.sync(
        &objects,
        Some(dereth_client::hud::ViewerFrame {
            position: pos,
            heading_degrees: 0.0,
        }),
    );

    let root = *screen.roots().first().expect("the screen has a root");
    let gameplay = world_support::as_gameplay(&mut screen);
    let wrote = gameplay.update_radar(&mut ui, &hud.view(&objects));
    let geom = RadarGeometry {
        radius: gameplay.radar.radius,
        center: gameplay.radar.center,
    };

    let radar = ui
        .get_child_recursive(root, window::RADAR)
        .expect("the radar");
    let fills = ui
        .node(radar)
        .expect("the radar is alive")
        .region
        .surface_fills
        .clone();
    let want = center_marker_fills(geom);
    let laid = fills.len() >= 9 && fills[fills.len() - 9..] == want[..];
    // ...and the blips are still in front of it: the mark did not replace them.
    let blips_too = fills.len() > 9;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.player.his-mark-is-laid-on-the-radars-own-element-after-the-blips",
        move |_| wrote && want.len() == 9 && laid && blips_too,
    );
}

// -------------------------------------------------------------------------------------------
// radar.range.*
// -------------------------------------------------------------------------------------------

/// There are two radar ranges and one question decides between them: is the player inside?
///
/// The two distances are written out here as numbers rather than fetched through the same call
/// the client makes, so a wrong one is visible. The boundary is driven on both sides, and the
/// piece of land an id belongs to must not reach the question at all.
pub fn there_are_two_radar_ranges_and_one_question() {
    use dereth_physics::landdefs::is_outdoors;
    use dereth_primitives::CellId;
    use dereth_ui_screens::mapradar::radar::radar_range;

    let outdoors_is_75 = (radar_range(true) - world_support::OUTDOOR_RANGE).abs() < f32::EPSILON;
    let indoors_is_25 = (radar_range(false) - world_support::INDOOR_RANGE).abs() < f32::EPSILON;
    // The indoor one must be the smaller: that is the whole of the claim.
    let smaller = radar_range(false) < radar_range(true);

    let block = 0xA9B4_0000u32;
    let outside = [1u32, 0x20, 0x3F, 0x40, 0xFF]
        .into_iter()
        .all(|idx| is_outdoors(CellId(block | idx)));
    let inside = [world_support::ENV_CELL_FLOOR, 0x101, 0x1FF, 0xFFFF]
        .into_iter()
        .all(|idx| !is_outdoors(CellId(block | idx)));
    // A different piece of land, the same answers: the top half of an id is masked off first.
    let masked = is_outdoors(CellId(0x0001_0001)) && !is_outdoors(CellId(0x0001_0100));

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.range.there-are-two-and-one-question-about-the-players-own-room-decides",
        move |_| outdoors_is_75 && indoors_is_25 && smaller && outside && inside && masked,
    );
}

/// Every place a recording puts the player takes the range its own room asks for, and the
/// recordings between them offer both rooms.
pub fn every_recorded_place_takes_the_range_its_room_asks_for() {
    use dereth_physics::landdefs::is_outdoors;
    use dereth_ui_screens::mapradar::radar::radar_range;
    use dereth_ui_screens::view::GameView as _;

    let mut indoors = 0usize;
    let mut outdoors = 0usize;
    let mut ok = true;
    for session in world_support::recordings_with_a_world() {
        let st = world_support::radar_stations(session);
        for (what, station) in [("outdoor", st.outdoor), ("indoor", st.indoor)] {
            let Some((at, recorded, here)) = station else {
                continue;
            };
            let (objects, hud) = world_support::scene_at(session, at);
            let Some(cell) = hud.player_cell else {
                continue;
            };
            // The two passes must reproduce each other, or nothing below is about that room.
            ok &= cell == recorded;
            let view = hud.view(&objects);
            let outside = view.player_outside();
            // Two readings that could disagree: one through the display and its own view, one
            // straight off the recording's own room id.
            ok &= outside == is_outdoors(cell);
            ok &= outside == (what == "outdoor");
            let want = if outside {
                world_support::OUTDOOR_RANGE
            } else {
                world_support::INDOOR_RANGE
            };
            ok &= (radar_range(outside) - want).abs() < f32::EPSILON;
            if outside {
                outdoors += 1;
            } else {
                indoors += 1;
            }
            println!(
                "range: {session} {what}: room {:#010X}, {here} things, range {want}",
                cell.0
            );
        }
    }

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.range.every-place-a-recording-puts-the-player-takes-the-range-its-room-asks-for",
        move |_| indoors > 0 && outdoors > 0 && ok,
    );
}

/// A dungeon and the inside of an above-ground building are the same case.
///
/// The client has one question and cannot tell them apart; the shipped data agrees, because
/// every room in it is numbered above the boundary and both kinds take the short range. The
/// open land of the same two pieces of land is the control.
pub fn a_dungeon_and_a_building_inside_are_the_same_case() {
    use dereth_assets::{world::LandblockInfo, Decode};
    use dereth_dat::DbType;
    use dereth_physics::landdefs::is_outdoors;
    use dereth_primitives::{CellId, DataId};
    use dereth_ui_screens::mapradar::radar::radar_range;

    let store = support::store();

    // Every room in the shipped data, grouped by the piece of land it belongs to.
    let mut rooms: std::collections::BTreeMap<u16, Vec<u32>> = std::collections::BTreeMap::new();
    let mut below_the_boundary = 0usize;
    for id in store.ids_of(DbType::Cell) {
        let idx = id.0 & 0xFFFF;
        if idx == 0xFFFF || idx == 0xFFFE {
            continue;
        }
        if idx < world_support::ENV_CELL_FLOOR {
            below_the_boundary += 1;
        }
        rooms
            .entry(u16::try_from(id.0 >> 16).expect("sixteen bits"))
            .or_default()
            .push(idx);
    }
    // The premise for the whole claim: the shipped numbering really does put every room above
    // the boundary, which is why one question suffices.
    let numbering = !rooms.is_empty() && below_the_boundary == 0;

    let buildings_of = |block: u16| -> Option<usize> {
        let id = DataId((u32::from(block) << 16) | 0xFFFE);
        let bytes = store.read_typed(DbType::Lbi, id).ok()?;
        LandblockInfo::decode_payload(id, &bytes)
            .ok()
            .map(|l| l.buildings.len())
    };

    let mut building_block = None;
    let mut dungeon_block = None;
    for (&block, cells) in &rooms {
        if cells.is_empty() {
            continue;
        }
        match buildings_of(block) {
            Some(n) if n > 0 && building_block.is_none() => building_block = Some(block),
            Some(0) if dungeon_block.is_none() => dungeon_block = Some(block),
            _ => {}
        }
        if building_block.is_some() && dungeon_block.is_some() {
            break;
        }
    }
    let bb = building_block.expect("some piece of land has both rooms and buildings on it");
    let db = dungeon_block.expect("some piece of land has rooms and no buildings");
    let building_room = CellId((u32::from(bb) << 16) | rooms[&bb][0]);
    let dungeon_room = CellId((u32::from(db) << 16) | rooms[&db][0]);

    let both_short = [building_room, dungeon_room].into_iter().all(|cell| {
        !is_outdoors(cell)
            && (radar_range(is_outdoors(cell)) - world_support::INDOOR_RANGE).abs() < f32::EPSILON
    });
    let same_case =
        (radar_range(is_outdoors(building_room)) - radar_range(is_outdoors(dungeon_room))).abs()
            < f32::EPSILON;
    // The control from the same two pieces of land, so this is not simply answering the short
    // range to everything.
    let control = [bb, db].into_iter().all(|block| {
        let land = CellId((u32::from(block) << 16) | 1);
        is_outdoors(land)
            && (radar_range(is_outdoors(land)) - world_support::OUTDOOR_RANGE).abs() < f32::EPSILON
    });
    println!(
        "range: {} pieces of land carry rooms; a building's inside and a dungeon both take \
         the short range",
        rooms.len()
    );

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.range.a-dungeon-and-the-inside-of-a-building-are-the-same-case",
        move |_| numbering && both_short && same_case && control,
    );
}

/// The range follows the player as he steps inside and out again.
///
/// One display, one world, and two places that differ only in the room the player is in -- so
/// nothing but the room can be what moves the range. And back out again in the same scenario,
/// because a change that only ever went one way would satisfy every step before it.
pub fn the_range_follows_the_player_in_and_out() {
    use dereth_physics::landdefs::is_outdoors;
    use dereth_primitives::CellId;
    use dereth_ui_screens::mapradar::radar::radar_range;
    use dereth_ui_screens::view::GameView as _;

    let session = world_support::a_recording_with_a_world();
    let st = world_support::radar_stations(session);
    let (objects, _) = world_support::scene_at(session, st.busiest.0);
    let player = objects.world.player.expect("the recording named a player");
    let pos = objects
        .presence(player)
        .and_then(|p| p.position)
        .expect("the player has a place");
    let block = pos.cell.0 & 0xFFFF_0000;

    let mut hud = dereth_client::hud::Hud::new();
    // No player at all: the client's own answer is "not outside", which takes the short range.
    let nobody = !hud.player_outside()
        && (radar_range(hud.player_outside()) - world_support::INDOOR_RANGE).abs() < f32::EPSILON;

    let mut outdoor = pos;
    outdoor.cell = CellId(block | 0x0001);
    hud.sync(
        &objects,
        Some(dereth_client::hud::ViewerFrame {
            position: outdoor,
            heading_degrees: 0.0,
        }),
    );
    let out_ok = hud.player_cell == Some(outdoor.cell) && hud.view(&objects).player_outside();
    let out_range = radar_range(hud.view(&objects).player_outside());

    let mut indoor = pos;
    indoor.cell = CellId(block | world_support::ENV_CELL_FLOOR);
    hud.sync(
        &objects,
        Some(dereth_client::hud::ViewerFrame {
            position: indoor,
            heading_degrees: 0.0,
        }),
    );
    let in_ok = hud.player_cell == Some(indoor.cell) && !hud.view(&objects).player_outside();
    let in_range = radar_range(hud.view(&objects).player_outside());

    let ranges = (out_range - world_support::OUTDOOR_RANGE).abs() < f32::EPSILON
        && (in_range - world_support::INDOOR_RANGE).abs() < f32::EPSILON
        && out_range > in_range;

    hud.sync(
        &objects,
        Some(dereth_client::hud::ViewerFrame {
            position: outdoor,
            heading_degrees: 0.0,
        }),
    );
    let back = hud.view(&objects).player_outside() && is_outdoors(outdoor.cell);

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.range.follows-the-player-as-he-steps-inside-and-out-again",
        move |_| nobody && out_ok && in_ok && ranges && back,
    );
}

/// What the radar shows stops one short of its own range, at either range.
///
/// Four made-up things straddle both edges a third of a metre either side, on a heading that is
/// not a cardinal one so that neither part of it is degenerate. No recorded place has anything
/// in either one-metre band, which is exactly why the recordings cannot see this.
pub fn what_the_radar_shows_stops_one_short_of_its_range() {
    use dereth_ui_screens::mapradar::radar::{draw_objects, radar_enum, RadarGeometry};
    use dereth_ui_screens::view::RadarEntry;

    let geom = RadarGeometry {
        radius: 50,
        center: (60.0, 60.0),
    };
    let heading = 41.7_f32.to_radians();
    let at = |d: f32| RadarEntry {
        id: dereth_primitives::ObjectId(0x8000_0000),
        player_space: (
            d * dereth_primitives::num::math::sinf(heading),
            d * dereth_primitives::num::math::cosf(heading),
            0.0,
        ),
        in_world: true,
        radar_enum: radar_enum::SHOW_ALWAYS,
        ..RadarEntry::default()
    };
    let objects: Vec<RadarEntry> = [23.7_f32, 24.3, 73.7, 74.3].into_iter().map(at).collect();

    let drawn = |range: f32| -> Vec<usize> {
        draw_objects(&objects, None, geom, range, None, false)
            .iter()
            .map(|b| b.index)
            .collect()
    };
    let out = drawn(world_support::OUTDOOR_RANGE);
    let ind = drawn(world_support::INDOOR_RANGE);
    let edges = out == vec![0, 1, 2] && ind == vec![0];

    // The premise: the square the radar also rejects against is not what is doing the work here.
    let round = [
        (world_support::OUTDOOR_RANGE, &out),
        (world_support::INDOOR_RANGE, &ind),
    ]
    .into_iter()
    .all(|(range, kept)| {
        #[allow(clippy::cast_precision_loss)]
        let scale = geom.radius as f32 / range;
        kept.iter().copied().all(|i| {
            let (x, y, _) = objects[i].player_space;
            (x * scale).abs() <= 50.0 && (y * scale).abs() <= 50.0
        })
    });
    println!("range: at the long range the edge keeps {out:?} of four; at the short, {ind:?}");

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.range.what-the-radar-shows-stops-one-short-of-its-own-range-at-either-range",
        move |_| edges && round,
    );
}

/// The chat window's own sweep for something to talk to uses the same radius as the radar.
pub fn the_chat_sweep_uses_the_same_radius() {
    use dereth_physics::landdefs::is_outdoors;
    use dereth_primitives::CellId;

    let (session, at, recorded) = world_support::a_recorded_indoor_place();
    let (objects, _) = world_support::scene_at(session, at);
    let player = objects.world.player.expect("the recording named a player");
    let pos = objects
        .presence(player)
        .and_then(|p| p.position)
        .expect("the player has a place");
    let block = recorded.0 & 0xFFFF_0000;

    let mut counts = Vec::new();
    let mut agreed = true;
    for cell in [CellId(block | 0x0001), recorded] {
        let mut p = pos;
        p.cell = cell;
        let mut hud = dereth_client::hud::Hud::new();
        hud.sync(
            &objects,
            Some(dereth_client::hud::ViewerFrame {
                position: p,
                heading_degrees: 0.0,
            }),
        );
        agreed &= hud.player_outside() == is_outdoors(cell);
        counts.push(
            hud.auto_target_world(&objects.world)
                .in_range_of_player
                .len(),
        );
    }
    let (out, inside) = (counts[0], counts[1]);
    println!(
        "range: the chat sweep found {out} things at the long range and {inside} at the short"
    );
    // Both must be more than nothing: an indoor zero would satisfy "fewer" for the wrong reason.
    let discriminating = out > 0 && inside > 0 && inside < out;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.range.the-chat-windows-own-sweep-uses-the-same-radius-as-the-radar",
        move |_| agreed && discriminating,
    );
}

/// The things on the radar really zoom when the player goes inside.
///
/// The same world, the same place and the same frame in both arms; the only difference is the
/// room the player is said to be in. The premise is asserted too: both arms must draw
/// something, and the scene must hold something in the band between the two ranges, or "the two
/// pictures differ" would be satisfiable by a scene with nothing in it.
pub fn the_radar_zooms_when_the_player_goes_inside() {
    use dereth_physics::landdefs::is_outdoors;
    use dereth_primitives::CellId;
    use dereth_ui_screens::mapradar::radar::{draw_objects, radar_range, Blip, RadarGeometry};
    use dereth_ui_screens::view::GameView as _;

    struct Arm {
        fills: Vec<dereth_ui::UiFill>,
        blips: Vec<Blip>,
        range: f32,
        geom: RadarGeometry,
    }

    let (mut ui, mut screen) = world_support::shipped_gameplay();
    let root = *screen.roots().first().expect("the screen has a root");
    let radar = ui
        .get_child_recursive(root, dereth_ui_screens::screens::gameplay::window::RADAR)
        .expect("the radar");

    let mut stations = 0usize;
    let mut projected = 0usize;
    let mut shared_total = 0usize;
    let mut moved_outward = 0usize;
    let mut ok = true;
    for session in world_support::recordings_with_a_world() {
        let st = world_support::radar_stations(session);
        let Some((at, recorded, _)) = st.indoor else {
            continue;
        };
        let (objects, hud) = world_support::scene_at(session, at);
        let player = objects.world.player.expect("the recording named a player");
        let pos = objects
            .presence(player)
            .and_then(|p| p.position)
            .expect("the player has a place");
        if pos.cell != recorded || is_outdoors(recorded) {
            continue;
        }
        let block = recorded.0 & 0xFFFF_0000;

        let mut arm = |cell: CellId,
                       ui: &mut dereth_ui::UiSystem,
                       screen: &mut Box<dyn dereth_ui::framework::Screen>|
         -> Arm {
            let mut p = pos;
            p.cell = cell;
            let mut h = dereth_client::hud::Hud::new();
            h.sync(
                &objects,
                Some(dereth_client::hud::ViewerFrame {
                    position: p,
                    heading_degrees: 0.0,
                }),
            );
            let g = world_support::as_gameplay(screen);
            let view = h.view(&objects);
            g.update_radar(ui, &view);
            let geom = RadarGeometry {
                radius: g.radar.radius,
                center: g.radar.center,
            };
            let range = radar_range(view.player_outside());
            let blips = draw_objects(
                &h.radar,
                h.radar.iter().find(|o| o.is_self),
                geom,
                range,
                None,
                false,
            );
            let fills = ui
                .node(radar)
                .expect("the radar is alive")
                .region
                .surface_fills
                .clone();
            Arm {
                fills,
                blips,
                range,
                geom,
            }
        };

        let inside = arm(recorded, &mut ui, &mut screen);
        let outside = arm(CellId(block | 0x0001), &mut ui, &mut screen);

        // Something **the radar would show** in the band between the two ranges, and something
        // inside the short one, or the difference below could not discriminate them. Counting
        // everything in view instead is not enough: one recorded place has eighteen things in
        // the band and only one of them is a thing the radar shows.
        let (mut within_in, mut band) = (0usize, 0usize);
        for e in &hud.radar {
            if !e.in_world
                || e.is_self
                || !dereth_ui_screens::mapradar::radar::inq_showable_on_radar(e)
            {
                continue;
            }
            let (x, y, _) = e.player_space;
            let d = (x * x + y * y).sqrt();
            if d < world_support::INDOOR_RANGE - 1.0 {
                within_in += 1;
            } else if d < world_support::OUTDOOR_RANGE - 1.0 {
                band += 1;
            }
        }
        if band == 0 || within_in == 0 || outside.blips.is_empty() || inside.blips.is_empty() {
            continue;
        }
        stations += 1;

        ok &= (outside.range - world_support::OUTDOOR_RANGE).abs() < f32::EPSILON;
        ok &= (inside.range - world_support::INDOOR_RANGE).abs() < f32::EPSILON;
        ok &= !outside.fills.is_empty() && !inside.fills.is_empty();
        ok &= outside.fills != inside.fills;
        ok &= inside.blips.len() < outside.blips.len();

        // The projection, thing by thing, at both ranges.
        let (cx, cy) = inside.geom.center;
        #[allow(clippy::cast_precision_loss)]
        let px_radius = inside.geom.radius as f32;
        for a in [&outside, &inside] {
            let scale = px_radius / a.range;
            for b in &a.blips {
                let (x, y, _) = hud.radar[b.index].player_space;
                #[allow(clippy::cast_possible_truncation)]
                let want = (x.mul_add(scale, cx) as i32, (cy - y * scale) as i32);
                ok &= (b.x, b.y) == want;
                projected += 1;
            }
        }

        // The zoom, with no tolerance: everything drawn by both arms sits further from the
        // centre at the shorter range.
        let by_index: std::collections::BTreeMap<usize, &Blip> =
            outside.blips.iter().map(|b| (b.index, b)).collect();
        let (fcx, fcy) = (f64::from(cx), f64::from(cy));
        let radius_of =
            |b: &Blip| ((f64::from(b.x) - fcx).powi(2) + (f64::from(b.y) - fcy).powi(2)).sqrt();
        for b in &inside.blips {
            let Some(o) = by_index.get(&b.index) else {
                continue;
            };
            let (was, now) = (radius_of(o), radius_of(b));
            // Something standing on the player himself is already at the centre and has nowhere
            // further in to be; everything else must move outward.
            if was == 0.0 {
                ok &= now == 0.0;
            } else {
                ok &= now > was;
                moved_outward += 1;
            }
            shared_total += 1;
        }
        println!(
            "range: {session} at {:#010X}: {} things at the long range, {} at the short; \
             {band} in the band between them",
            recorded.0,
            outside.blips.len(),
            inside.blips.len()
        );
    }

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.range.the-things-on-the-radar-really-zoom-when-the-player-goes-inside",
        move |_| stations >= 2 && projected > 0 && shared_total > 0 && moved_outward > 0 && ok,
    );
}

/// The padlock works the same at either range, and neither moves the other.
pub fn the_padlock_works_the_same_at_either_range() {
    use dereth_primitives::CellId;
    use dereth_ui_screens::mapradar::radar::{child, lock_state, radar_range};
    use dereth_ui_screens::screens::gameplay::window;
    use dereth_ui_screens::view::{GameView as _, UiRequest};

    let session = world_support::a_recording_with_a_world();
    let st = world_support::radar_stations(session);
    let (objects, _) = world_support::scene_at(session, st.busiest.0);
    let player = objects.world.player.expect("the recording named a player");
    let pos = objects
        .presence(player)
        .and_then(|p| p.position)
        .expect("the player has a place");
    let block = pos.cell.0 & 0xFFFF_0000;

    let mut ok = true;
    for (idx, want) in [
        (0x0001u32, world_support::OUTDOOR_RANGE),
        (world_support::ENV_CELL_FLOOR, world_support::INDOOR_RANGE),
    ] {
        let (mut ui, mut screen) = world_support::shipped_gameplay();
        let root = *screen.roots().first().expect("the screen has a root");
        let radar = ui
            .get_child_recursive(root, window::RADAR)
            .expect("the radar");
        let lock = ui
            .get_child_recursive(radar, child::LOCK_BUTTON)
            .expect("the padlock");
        let drag = ui
            .get_child_recursive(radar, child::DRAG_BUTTON)
            .expect("the drag handle");
        let closed = world_support::state_image(&ui, lock, lock_state::LOCKED);
        let open = world_support::state_image(&ui, lock, lock_state::UNLOCKED);
        ok &= closed != open;

        let mut p = pos;
        p.cell = CellId(block | idx);
        let mut hud = dereth_client::hud::Hud::new();
        hud.sync(
            &objects,
            Some(dereth_client::hud::ViewerFrame {
                position: p,
                heading_degrees: 0.0,
            }),
        );
        {
            let g = world_support::as_gameplay(&mut screen);
            let view = hud.view(&objects);
            g.update_radar(&mut ui, &view);
            ok &= (radar_range(view.player_outside()) - want).abs() < f32::EPSILON;
        }
        ui.requests.clear();

        let b = ui.screen_box(lock);
        let (x, y) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
        ui.mouse_move(dereth_primitives::LocalTime(1.0), x, y);
        ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, x, y);
        ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, x, y, false);
        world_support::pump_boxed(&mut ui, &mut screen);
        let asked: Vec<bool> = ui
            .requests
            .take()
            .into_iter()
            .filter_map(|r| match r {
                UiRequest::SetLockUi(v) => Some(v),
                _ => None,
            })
            .collect();
        ok &= asked == vec![true];

        {
            let g = world_support::as_gameplay(&mut screen);
            g.set_lock_ui(true);
        }
        ui.broadcast_global(dereth_ui::msg::global::UI_LOCK_TOGGLED, 0);
        world_support::pump_boxed(&mut ui, &mut screen);
        ok &= !ui
            .node(drag)
            .expect("the handle is alive")
            .region
            .flags
            .visible;
        ok &= ui
            .node(lock)
            .expect("the padlock is alive")
            .region
            .image
            .as_ref()
            .map(|g| g.did)
            == closed;

        // ...and the range is untouched by any of it.
        let g = world_support::as_gameplay(&mut screen);
        let view = hud.view(&objects);
        g.update_radar(&mut ui, &view);
        ok &= (radar_range(view.player_outside()) - want).abs() < f32::EPSILON;
    }

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.padlock.works-the-same-at-either-range-and-neither-moves-the-other",
        move |_| ok,
    );
}

// -------------------------------------------------------------------------------------------
// radar.click.*, radar.hover.*, radar.lock.*, radar.padlock.*, radar.drag.*
// -------------------------------------------------------------------------------------------

/// A click on a thing on the radar selects the thing it stands for.
///
/// The whole seam runs: a recording, the object stream, the display, the radar's own drawing, a
/// real pointer at a real pixel, and the request the click raises. The answer is worked out
/// again from the recording's own descriptions rather than from the thing under test, so the two
/// can disagree, and no object and no count is written down here.
pub fn a_click_on_a_thing_on_the_radar_selects_it() {
    use dereth_primitives::LocalTime;
    use dereth_ui_screens::view::UiRequest;

    let mut checked = 0usize;
    let mut unambiguous = 0usize;
    let mut occluded = 0usize;
    let mut measured = 0usize;
    let mut covered_by: std::collections::BTreeMap<u32, usize> = std::collections::BTreeMap::new();
    let mut ok = true;
    for session in world_support::recordings_with_a_world() {
        let st = world_support::radar_stations(session);
        let (objects, hud) = world_support::scene_at(session, st.busiest.0);
        let (mut ui, mut screen) = world_support::shipped_gameplay();
        let radar = world_support::radar_element(&ui, &screen);
        let origin = ui.screen_box(radar);

        let blips = world_support::radar_blips(&mut ui, &mut screen, &objects, &hud);
        if blips.is_empty() {
            continue;
        }
        // The independent answer, rebuilt from the recording.
        let want = world_support::expected_blip_map(&objects, &hud, &mut screen);
        ok &= want.len() == blips.len();
        measured += 1;

        for b in &blips {
            let (x, y) = (origin.x0 + b.x, origin.y0 + b.y);
            ui.mouse_move(LocalTime(1.0), x, y);
            {
                let g = world_support::as_gameplay(&mut screen);
                let view = hud.view(&objects);
                g.update_radar(&mut ui, &view);
            }
            let _ = ui.requests.take();

            // Everything the pointer is within the client's own reach of at that point. In a
            // crowded scene several things are, and which of them the client names is its own
            // tie-breaking -- so what is asserted is that it names one of them, and, where
            // exactly one thing is in reach, that it names that one.
            let reachable = world_support::things_within_reach(&want, (b.x, b.y));
            if reachable.is_empty() {
                ok = false;
                continue;
            }

            // A thing with something else drawn over it cannot be clicked, and that is the
            // client's behaviour rather than a shortfall: the pointer lands on whatever is in
            // front, which over part of the ring is the radar's own padlock and drag handle and
            // over another part is a neighbouring window of the screen. They are counted and
            // named rather than skipped, and what is asserted is that most of the field is
            // still reachable.
            let hit = ui.hit_test_screen(x, y);
            if hit != Some(radar) {
                let Some(other) = hit else {
                    ok = false;
                    continue;
                };
                *covered_by
                    .entry(
                        ui.node(other)
                            .expect("the thing in front is alive")
                            .desc
                            .element_id
                            .0,
                    )
                    .or_insert(0usize) += 1;
                occluded += 1;
                continue;
            }

            world_support::click_at(&mut ui, x, y);
            world_support::pump_boxed(&mut ui, &mut screen);
            let got: Vec<dereth_primitives::ObjectId> = ui
                .requests
                .take()
                .into_iter()
                .filter_map(|r| match r {
                    UiRequest::Select(id) => Some(id),
                    _ => None,
                })
                .collect();
            ok &= got.len() == 1 && reachable.contains(&got[0]);
            if reachable.len() == 1 {
                ok &= got == reachable;
                unambiguous += 1;
            }
            // ...and it is a thing the recording really created, never the player himself.
            let picked = got.first().copied().unwrap_or_default();
            ok &= objects.world.weenie(picked).is_some();
            ok &= Some(picked) != objects.world.player;
            checked += 1;
        }
    }
    println!(
        "radar ui: {checked} clicks over {measured} recordings, {unambiguous} of them with one \
         thing in reach and no other; {occluded} had something drawn over them, by \
         {covered_by:#010X?}"
    );

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.click.a-click-on-a-thing-on-the-radar-selects-the-thing-it-stands-for",
        // The denominator, so that a run the pointer could reach nothing in would not look like
        // one it reached everything in. Over the eighteen recordings about a fifth is covered,
        // so what is asserted is the property rather than a figure: most of the field is
        // reachable.
        move |_| {
            measured >= 3 && checked >= 50 && unambiguous >= 20 && occluded * 2 < checked && ok
        },
    );
}

/// An empty part of the radar is transparent to the pointer and selects nothing.
///
/// Both directions in one scenario, on the same element: over nothing it cannot even be hit, so
/// a click there falls through to whatever is behind it, and over a thing it can.
pub fn empty_radar_is_transparent_and_selects_nothing() {
    use dereth_primitives::LocalTime;
    use dereth_ui_screens::view::UiRequest;

    let session = world_support::the_busiest_recording();
    let st = world_support::radar_stations(session);
    let (objects, hud) = world_support::scene_at(session, st.busiest.0);
    let (mut ui, mut screen) = world_support::shipped_gameplay();
    let radar = world_support::radar_element(&ui, &screen);
    let origin = ui.screen_box(radar);

    let blips = world_support::radar_blips(&mut ui, &mut screen, &objects, &hud);
    let drew_something = !blips.is_empty();
    let empty = (0..origin.width())
        .flat_map(|x| (0..origin.height()).map(move |y| (x, y)))
        .find(|&(x, y)| {
            blips.iter().all(|b| {
                let (dx, dy) = (x - b.x, y - b.y);
                dx * dx + dy * dy >= 400
            })
        })
        .expect("the radar has a point twenty away from everything on it");

    let (sx, sy) = (origin.x0 + empty.0, origin.y0 + empty.1);
    ui.mouse_move(LocalTime(1.0), sx, sy);
    let nothing_under = {
        let g = world_support::as_gameplay(&mut screen);
        let view = hud.view(&objects);
        g.update_radar(&mut ui, &view);
        g.radar.object_under_mouse.is_none()
    };
    let _ = ui.requests.take();

    let transparent = !ui.node(radar).expect("the radar is alive").is_mouse_visible
        && ui.hit_test_screen(sx, sy) != Some(radar);

    world_support::click_at(&mut ui, sx, sy);
    world_support::pump_boxed(&mut ui, &mut screen);
    let selected_nothing = !ui
        .requests
        .take()
        .iter()
        .any(|r| matches!(r, UiRequest::Select(_)));

    // The other direction, same element, same scenario.
    let b = blips.first().expect("something is on the radar");
    let (bx, by) = (origin.x0 + b.x, origin.y0 + b.y);
    ui.mouse_move(LocalTime(1.0), bx, by);
    let something_under = {
        let g = world_support::as_gameplay(&mut screen);
        let view = hud.view(&objects);
        g.update_radar(&mut ui, &view);
        g.radar.object_under_mouse.is_some()
    };
    let now_hittable = ui.node(radar).expect("the radar is alive").is_mouse_visible
        && ui.hit_test_screen(bx, by) == Some(radar);

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.click.an-empty-part-of-the-radar-is-transparent-and-selects-nothing",
        move |_| {
            drew_something
                && nothing_under
                && transparent
                && selected_nothing
                && something_under
                && now_hittable
        },
    );
}

/// The name shown over a thing on the radar is the shard's own name for it.
pub fn the_name_over_a_thing_is_the_shards_own() {
    use dereth_primitives::LocalTime;

    let session = world_support::the_busiest_recording();
    let st = world_support::radar_stations(session);
    let (objects, hud) = world_support::scene_at(session, st.busiest.0);
    let (mut ui, mut screen) = world_support::shipped_gameplay();
    let radar = world_support::radar_element(&ui, &screen);
    let origin = ui.screen_box(radar);

    let blips = world_support::radar_blips(&mut ui, &mut screen, &objects, &hud);
    let mut named = 0usize;
    let mut ok = !blips.is_empty();
    for b in &blips {
        ui.mouse_move(LocalTime(1.0), origin.x0 + b.x, origin.y0 + b.y);
        let id = {
            let g = world_support::as_gameplay(&mut screen);
            let view = hud.view(&objects);
            g.update_radar(&mut ui, &view);
            g.radar.object_under_mouse
        };
        let Some(id) = id else {
            ok = false;
            continue;
        };
        let want = objects
            .world
            .weenie(id)
            .map(|w| w.pwd.name.clone())
            .expect("the recording named the thing it sent");
        let node = ui.node(radar).expect("the radar is alive");
        // The flag the hover itself reads, as well as the words.
        ok &= node.region.flags.tooltip;
        ok &= node.tooltip_text.as_deref() == Some(want.as_str());
        if !want.is_empty() {
            named += 1;
        }
    }
    println!(
        "radar ui: {named} of {} things on the radar named themselves",
        blips.len()
    );

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.hover.the-name-shown-over-a-thing-is-the-shards-own-name-for-it",
        move |_| ok && named > 0,
    );
}

/// Whether the interface is locked is one bit of the player's own options, and it survives the
/// blob the shard sends it in.
pub fn the_lock_is_one_bit_of_the_players_options() {
    use dereth_protocol::login::PlayerModule;
    use dereth_protocol::Message as _;

    /// The bit, as a number: reading it back through the client's own name would not notice a
    /// wrong one.
    const LOCK: u32 = 0x0100_0000;

    let bit = LOCK == 1 << 24
        // ...and it is not the bit another option uses, in another word entirely.
        && LOCK != 1 << 21;
    // A fresh character starts with the interface unlocked.
    let fresh = PlayerModule::DEFAULT_OPTIONS2 & LOCK == 0;

    let mut ok = bit && fresh;
    for want in [false, true] {
        let mut m = PlayerModule {
            options2: PlayerModule::DEFAULT_OPTIONS2,
            ..Default::default()
        };
        m.options2 = if want {
            m.options2 | LOCK
        } else {
            m.options2 & !LOCK
        };
        let mut hud = dereth_client::hud::Hud::new();
        hud.player_module = Some(m.clone());
        ok &= hud.lock_ui() == want;

        // And it survives the blob it is kept in. The word only reaches the wire when the blob
        // says it is there, which is why a bit that is never written comes back as the default.
        let m = PlayerModule {
            option_flags: 0x0040,
            spell_bars: vec![Vec::new()],
            ..m
        };
        let mut w = dereth_protocol::Writer::body();
        m.write(&mut w).expect("the module packs");
        let bytes = w.into_inner();
        let mut r = dereth_protocol::Reader::body(&bytes);
        let back = PlayerModule::read(&mut r).expect("and unpacks again");
        ok &= (back.options2 & LOCK != 0) == want;
    }

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.lock.whether-the-interface-is-locked-is-one-bit-of-the-players-own-options",
        move |_| ok,
    );
}

/// Clicking the padlock asks for the lock to be flipped, and does nothing else.
///
/// Both ways round, because a toggle that only ever asked for one of them would satisfy a
/// one-way measurement. The click is a real pointer on the real element.
pub fn clicking_the_padlock_asks_to_flip_the_lock() {
    use dereth_ui_screens::mapradar::radar::child;
    use dereth_ui_screens::view::UiRequest;

    let mut ok = true;
    for start in [false, true] {
        let (mut ui, mut screen) = world_support::shipped_gameplay();
        let radar = world_support::radar_element(&ui, &screen);
        {
            let g = world_support::as_gameplay(&mut screen);
            g.set_lock_ui(start);
            g.cascade_lock(&mut ui, start);
        }
        let lock = ui
            .get_child_recursive(radar, child::LOCK_BUTTON)
            .expect("the padlock");
        let b = ui.screen_box(lock);
        ok &= b.is_valid();
        let _ = ui.requests.take();

        world_support::click_at(&mut ui, (b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
        world_support::pump_boxed(&mut ui, &mut screen);

        let got = ui.requests.take();
        ok &= got
            .iter()
            .filter(|r| matches!(r, UiRequest::SetLockUi(_)))
            .count()
            == 1;
        ok &= got.contains(&UiRequest::SetLockUi(!start));
        // The padlock is a child of the radar, so the click must not also select something.
        ok &= !got.iter().any(|r| matches!(r, UiRequest::Select(_)));
    }

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.padlock.a-click-on-it-asks-to-flip-the-lock-and-does-nothing-else",
        move |_| ok,
    );
}

/// The lock the player asks for reaches his own options, and a screen rebuilt afterwards reads
/// it back.
pub fn the_lock_reaches_the_players_options_and_survives_a_rebuild() {
    use dereth_protocol::login::PlayerModule;
    use dereth_ui_screens::mapradar::radar::{child, lock_state};
    use dereth_ui_screens::view::UiRequest;

    const LOCK: u32 = 0x0100_0000;

    let session = world_support::a_recording_with_a_world();
    let st = world_support::radar_stations(session);
    let (objects, mut hud) = world_support::scene_at(session, st.busiest.0);
    hud.player_module = Some(PlayerModule {
        options2: PlayerModule::DEFAULT_OPTIONS2,
        ..Default::default()
    });
    let starts_unlocked = !hud.lock_ui();

    let (mut ui, mut screen) = world_support::shipped_gameplay();
    let radar = world_support::radar_element(&ui, &screen);
    let lock = ui
        .get_child_recursive(radar, child::LOCK_BUTTON)
        .expect("the padlock");
    let drag = ui
        .get_child_recursive(radar, child::DRAG_BUTTON)
        .expect("the drag handle");
    let b = ui.screen_box(lock);

    // The screen has already run a frame by the time the player reaches for the padlock.
    let seeded = {
        let g = world_support::as_gameplay(&mut screen);
        hud.drive(&mut ui, g, 1, &objects);
        !g.locked
    };
    let _ = ui.requests.take();

    world_support::click_at(&mut ui, (b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
    world_support::pump_boxed(&mut ui, &mut screen);
    let asked: Vec<bool> = ui
        .requests
        .take()
        .into_iter()
        .filter_map(|r| match r {
            UiRequest::SetLockUi(v) => Some(v),
            _ => None,
        })
        .collect();
    let asked_for_it = asked == vec![true];

    // What the shell does with that request: write the screen's own mirror, then tell everything
    // else. The order is the client's and it matters.
    {
        let g = world_support::as_gameplay(&mut screen);
        g.set_lock_ui(true);
    }
    ui.broadcast_global(dereth_ui::msg::global::UI_LOCK_TOGGLED, 0);
    world_support::pump_boxed(&mut ui, &mut screen);
    let cascaded = !ui
        .node(drag)
        .expect("the handle is alive")
        .region
        .flags
        .visible
        && ui
            .node(lock)
            .expect("the padlock is alive")
            .region
            .image
            .as_ref()
            .map(|g| g.did)
            == world_support::state_image(&ui, lock, lock_state::LOCKED);

    // And the display writes it into the player's own options.
    let before = hud.stats.lock_ui_writes;
    {
        let g = world_support::as_gameplay(&mut screen);
        hud.drive(&mut ui, g, 1, &objects);
    }
    let written = hud.stats.lock_ui_writes == before + 1
        && hud.lock_ui()
        && hud.player_module.as_ref().expect("the module").options2 & LOCK != 0;

    // A screen rebuilt from the same options comes up locked, with nobody clicking anything.
    let (mut ui2, mut screen2) = world_support::shipped_gameplay();
    let rebuilt = {
        let g = world_support::as_gameplay(&mut screen2);
        let fresh = !g.locked;
        hud.drive(&mut ui2, g, 2, &objects);
        fresh && g.locked
    };
    let radar2 = world_support::radar_element(&ui2, &screen2);
    let lock2 = ui2
        .get_child_recursive(radar2, child::LOCK_BUTTON)
        .expect("the padlock");
    let drew_shut = ui2
        .node(lock2)
        .expect("the padlock is alive")
        .region
        .image
        .as_ref()
        .map(|g| g.did)
        == world_support::state_image(&ui2, lock2, lock_state::LOCKED);

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.lock.the-lock-the-player-asks-for-reaches-his-options-and-survives-a-rebuilt-screen",
        move |_| {
            starts_unlocked && seeded && asked_for_it && cascaded && written && rebuilt && drew_shut
        },
    );
}

/// The padlock swaps its two pictures with the lock, and the drag handle follows it.
pub fn the_padlock_swaps_its_pictures_and_the_handle_follows() {
    use dereth_ui_screens::mapradar::radar::{child, lock_state};

    let (mut ui, mut screen) = world_support::shipped_gameplay();
    let radar = world_support::radar_element(&ui, &screen);
    let lock = ui
        .get_child_recursive(radar, child::LOCK_BUTTON)
        .expect("the padlock");
    let drag = ui
        .get_child_recursive(radar, child::DRAG_BUTTON)
        .expect("the drag handle");

    let shut = world_support::state_image(&ui, lock, lock_state::LOCKED);
    let open = world_support::state_image(&ui, lock, lock_state::UNLOCKED);
    let mut ok = shut != open;

    // And back, and again: a change that only went one way would satisfy two of these three.
    for locked in [true, false, true] {
        {
            let g = world_support::as_gameplay(&mut screen);
            g.cascade_lock(&mut ui, locked);
        }
        ok &= ui
            .node(lock)
            .expect("the padlock is alive")
            .region
            .image
            .as_ref()
            .map(|g| g.did)
            == if locked { shut } else { open };
        ok &= ui
            .node(drag)
            .expect("the handle is alive")
            .region
            .flags
            .visible
            == !locked;
    }

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.padlock.swaps-its-two-pictures-with-the-lock-and-the-drag-handle-follows",
        move |_| ok,
    );
}

/// The handle in the corner of the radar moves the window by how far the pointer moved, and
/// keeps it inside the screen.
pub fn the_handle_moves_the_radar_and_keeps_it_on_screen() {
    use dereth_primitives::LocalTime;
    use dereth_ui_screens::mapradar::radar::child;

    let (mut ui, mut screen) = world_support::shipped_gameplay();
    let radar = world_support::radar_element(&ui, &screen);
    let drag = ui
        .get_child_recursive(radar, child::DRAG_BUTTON)
        .expect("the drag handle");

    // What kind of thing it is comes from the shipped layout, not from here.
    let is_a_dragbar = ui.node(drag).expect("the handle is alive").desc.ty
        == dereth_ui::ElementType(dereth_ui::factory::ty::DRAGBAR.0);

    // It is hidden while the interface is locked; the player unlocks it to get at it.
    {
        let g = world_support::as_gameplay(&mut screen);
        g.cascade_lock(&mut ui, false);
    }
    let shown = ui
        .node(drag)
        .expect("the handle is alive")
        .region
        .flags
        .visible;

    let before = ui.node(radar).expect("the radar is alive").region.box_;
    let grab = ui.screen_box(drag);
    let (gx, gy) = ((grab.x0 + grab.x1) / 2, (grab.y0 + grab.y1) / 2);

    // Well inside the screen, so the edge is not what is being measured.
    let (dx, dy) = (-120, 90);
    ui.mouse_move(LocalTime(1.0), gx, gy);
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, gx, gy);
    ui.mouse_move(LocalTime(1.1), gx + dx, gy + dy);
    let moved = ui.node(radar).expect("the radar is alive").region.box_;
    let followed = (moved.x0, moved.y0) == (before.x0 + dx, before.y0 + dy)
        && (moved.width(), moved.height()) == (before.width(), before.height());
    ui.mouse_up(
        dereth_ui::focus::action::PRIMARY_CLICK,
        gx + dx,
        gy + dy,
        false,
    );
    let released = !ui
        .node(radar)
        .expect("the radar is alive")
        .flags
        .is_moving();

    // The edges: dragging far past a corner stops at it rather than leaving the screen.
    ui.mouse_move(LocalTime(2.0), gx + dx, gy + dy);
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, gx + dx, gy + dy);
    ui.mouse_move(LocalTime(2.1), -5000, -5000);
    let corner = ui.node(radar).expect("the radar is alive").region.box_;
    let clamped_near = (corner.x0, corner.y0) == (0, 0);
    ui.mouse_move(LocalTime(2.2), 5000, 5000);
    let far = ui.node(radar).expect("the radar is alive").region.box_;
    let parent = ui
        .node(ui.parent(radar).expect("the radar hangs off something"))
        .expect("alive")
        .region
        .box_;
    let clamped_far =
        (far.x0, far.y0) == (parent.width() - far.width(), parent.height() - far.height());
    ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, 5000, 5000, false);

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.drag.the-handle-moves-the-window-by-how-far-the-pointer-moved-and-keeps-it-on-screen",
        move |_| {
            is_a_dragbar && shown && followed && released && clamped_near && clamped_far
        },
    );
}

/// A radar the player has moved writes its new place into his own options, once.
pub fn a_moved_radar_writes_its_new_place_into_the_options() {
    use dereth_primitives::LocalTime;
    use dereth_ui_screens::mapradar::radar::child;
    use dereth_ui_screens::screens::gameplay::placement;
    use dereth_ui_screens::view::UiRequest;

    let session = world_support::a_recording_with_a_world();
    let st = world_support::radar_stations(session);
    let (objects, hud) = world_support::scene_at(session, st.busiest.0);
    let (mut ui, mut screen) = world_support::shipped_gameplay();
    let radar = world_support::radar_element(&ui, &screen);
    let drag = ui
        .get_child_recursive(radar, child::DRAG_BUTTON)
        .expect("the drag handle");
    // The window's own number comes out of the shipped layout, not from here.
    let window_id = world_support::as_gameplay(&mut screen).radar.window_id;
    let has_an_id = window_id != 0;

    {
        let g = world_support::as_gameplay(&mut screen);
        g.cascade_lock(&mut ui, false);
        // The first look only records where the window is; nothing has moved yet.
        let view = hud.view(&objects);
        g.update_radar(&mut ui, &view);
    }
    let _ = ui.requests.take();

    let grab = ui.screen_box(drag);
    let (gx, gy) = ((grab.x0 + grab.x1) / 2, (grab.y0 + grab.y1) / 2);
    let (dx, dy) = (-77, 61);
    ui.mouse_move(LocalTime(1.0), gx, gy);
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, gx, gy);
    ui.mouse_move(LocalTime(1.1), gx + dx, gy + dy);
    ui.mouse_up(
        dereth_ui::focus::action::PRIMARY_CLICK,
        gx + dx,
        gy + dy,
        false,
    );

    let at = ui.node(radar).expect("the radar is alive").region.box_;
    {
        let g = world_support::as_gameplay(&mut screen);
        let view = hud.view(&objects);
        g.update_radar(&mut ui, &view);
    }
    let got: Vec<UiRequest> = ui
        .requests
        .take()
        .into_iter()
        .filter(|r| matches!(r, UiRequest::SetChatWindowOption { .. }))
        .collect();
    let wrote = got
        == vec![
            UiRequest::SetChatWindowOption {
                window: window_id,
                property: placement::X,
                value: at.x0,
            },
            UiRequest::SetChatWindowOption {
                window: window_id,
                property: placement::Y,
                value: at.y0,
            },
        ];

    // A frame with no move writes nothing: the place is written when the window moves and not on
    // a timer.
    {
        let g = world_support::as_gameplay(&mut screen);
        let view = hud.view(&objects);
        g.update_radar(&mut ui, &view);
    }
    let quiet = !ui
        .requests
        .take()
        .iter()
        .any(|r| matches!(r, UiRequest::SetChatWindowOption { .. }));

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "radar.drag.a-radar-the-player-has-moved-writes-its-new-place-into-his-own-options",
        move |_| has_an_id && wrote && quiet,
    );
}
