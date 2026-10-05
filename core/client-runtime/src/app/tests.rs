use super::*;

/// The landblock line prints when any number in it moves under a held block.
#[test]
fn the_landblock_line_prints_when_any_number_in_it_moves_under_a_held_block() {
    use crate::present::SceneCensus;
    let block = (0xA9, 0xB4);
    let base = SceneCensus::default();
    let quiet = ViewerBlockReport::new(block, &base);

    // Every one of the six printed counters, on its own, with the block never moving. Each
    // closure is a landblock finishing its mesh under a player who has not walked anywhere.
    type BumpScene = fn(&mut SceneCensus);
    let held: [(&str, BumpScene); 6] = [
        ("blocks_meshed", |s| s.blocks_meshed += 1),
        ("terrain_surfaces", |s| s.terrain_surfaces += 1),
        ("scenery_objects", |s| s.scenery_objects += 1),
        ("buildings", |s| s.buildings += 1),
        ("static_objects", |s| s.static_objects += 1),
        ("object_triangles", |s| s.object_triangles += 1),
    ];
    for (name, bump) in held {
        let mut s = base;
        bump(&mut s);
        assert_ne!(
            quiet,
            ViewerBlockReport::new(block, &s),
            "{name} climbed and the landblock line stayed silent"
        );
    }
    // And the term it has always carried.
    assert_ne!(
        quiet,
        ViewerBlockReport::new((0xA9, 0xB5), &base),
        "the crossing itself"
    );
}

/// **`last_viewer_cell`.** The key was the cell; the line also prints `env_cell_counts().1`,
/// which streams and is asked of the *camera's* cell rather than the body's.
///
/// Falsified by: keying on the cell alone.
#[test]
fn the_viewer_cell_line_prints_when_the_interior_batch_count_moves_under_a_held_cell() {
    // An **interior** cell: `is_outdoors` is `(id & 0xFFFF) < 0x100`, so 0x0129 is indoors.
    let cell = dereth_primitives::CellId(0xA9B4_0129);
    let quiet = ViewerCellReport::new(cell, 0);
    assert!(
        !quiet.outdoors,
        "the fixture cell is indoors, which is what the line prints"
    );

    // The body has not moved a millimetre; the block carrying its cell finished meshing.
    assert_ne!(
        quiet,
        ViewerCellReport::new(cell, 7),
        "the interior batch count arrived and the line stayed silent"
    );
    // The two terms the line has always carried, through the same constructor the frame uses.
    let outdoor = dereth_primitives::CellId(0xA9B4_0001);
    assert_ne!(quiet, ViewerCellReport::new(outdoor, 0));
    assert!(
        ViewerCellReport::new(outdoor, 0).outdoors,
        "is_outdoors is not being consulted, so the line's own word is a constant"
    );
}

/// **`last_object_count`.** The key was the drawn count; eleven other printed numbers can move
/// while it holds still, and the first arm below is the one that actually happens — a sync in
/// which as many objects left as arrived.
///
/// Falsified by: keying on `drawn` alone (every arm fails), or by dropping the magnitude on the
/// two message-rate counters (the last arm fails and the line floods).
#[test]
fn the_object_census_prints_when_creates_and_removes_move_together() {
    use crate::objects::ObjectStats;
    use crate::present::SceneCensus;
    let w = SceneCensus::default();
    let s = ObjectStats::default();
    let quiet = ObjectReport::new(4, &w, &s).key();

    // One object created and one removed in the same sync: the count is unchanged and the
    // line used to stay silent over it.
    let churn = ObjectStats {
        creates: 1,
        removes: 1,
        ..s
    };
    assert_ne!(
        quiet,
        ObjectReport::new(4, &w, &churn).key(),
        "a create and a remove cancelled in the count and the census stayed silent"
    );

    // Each of the other printed numbers that leaves the count alone, **on its own**.
    //
    // `creates` and `removes` are in this list as well as in the churn arm above, and that is
    // not redundancy: a mutation blanking `creates` in the constructor SURVIVED the churn arm,
    // because that arm moves both counters at once and `removes` alone still moved the key.
    // The station confounded two variables; this is it rebuilt on one. `drawn` is a
    // parameter here, so a create with no matching remove can be driven with the count held.
    type BumpBoth = fn(&mut SceneCensus, &mut ObjectStats);
    let held: [(&str, BumpBoth); 10] = [
        ("creates", |_, s| s.creates += 1),
        ("removes", |_, s| s.removes += 1),
        ("merges", |_, s| s.merges += 1),
        ("recreates", |_, s| s.recreates += 1),
        ("parent_events", |_, s| s.parent_events += 1),
        ("container_exits_offered", |_, s| {
            s.container_exits_offered += 1
        }),
        ("server_objects_animated", |w, _| {
            w.server_objects_animated += 1
        }),
        ("server_objects_held", |w, _| w.server_objects_held += 1),
        ("server_object_setups", |w, _| w.server_object_setups += 1),
        ("server_object_triangles", |w, _| {
            w.server_object_triangles += 1
        }),
    ];
    for (name, bump) in held {
        let (mut w2, mut s2) = (w, s);
        bump(&mut w2, &mut s2);
        assert_ne!(
            quiet,
            ObjectReport::new(4, &w2, &s2).key(),
            "{name} climbed and the object census stayed silent"
        );
    }
    // And the term it has always carried.
    assert_ne!(
        quiet,
        ObjectReport::new(5, &w, &s).key(),
        "the drawn count itself"
    );

    // The two message-rate counters: announced on the first one and at every power of two,
    // silent in between. Both halves matter -- never silent, and never a line per frame.
    let mut printed = 0u32;
    let mut last = Some(quiet);
    for n in 0..=1024u64 {
        let r = ObjectReport::new(
            4,
            &w,
            &ObjectStats {
                position_updates: n,
                ..s
            },
        );
        if last != Some(r.key()) {
            last = Some(r.key());
            printed += 1;
            assert_eq!(
                r.position_updates, n,
                "the print carries the raw value, not the key's"
            );
        }
    }
    assert_eq!(
        printed, 11,
        "1,025 position updates must announce at 1, 2, 4 .. 1024 and nowhere else"
    );
    assert!(
        ObjectReport::new(
            4,
            &w,
            &ObjectStats {
                movement_updates: 1,
                ..s
            }
        )
        .key()
            != quiet,
        "the first movement update was never announced at all"
    );
}

/// **`net_error_reported`.** It was a `bool` over a printed *code*, so the second distinct
/// error was silent -- and the second one is the interesting one, because it is what the
/// reconnection did.
///
/// Falsified by: `last.is_none()` (the old latch: the third arm fails), or by returning `true`
/// unconditionally (the second arm fails and the line prints every frame the error is live).
#[test]
fn the_net_error_line_prints_the_second_distinct_error_and_not_a_repeat() {
    use dereth_transport::conn::NetErrorCode;
    // Two real connection-error codes, in the order a dropped session
    // then a refused re-login produces them.
    let first = NetErrorCode::ClientTimedOutServer;
    let second = NetErrorCode::PlayerAlreadyLoggedOn;
    assert_ne!(
        first, second,
        "the two arms below are the same code; this asserts nothing"
    );

    let mut last: Option<NetErrorCode> = None;
    let mut printed: Vec<NetErrorCode> = Vec::new();
    for code in [first, first, first, second, second, first] {
        if should_report_net_error(last, code) {
            last = Some(code);
            printed.push(code);
        }
    }
    assert_eq!(
        printed,
        vec![first, second, first],
        "each distinct error prints once and a repeat is silent"
    );
}

// Oracle: the original fixed window class and non-resizable window style.
#[test]
fn the_documented_window_style_has_no_resize_and_no_maximise() {
    assert_eq!(style::WINDOWED, 0x12CA_0000);
    const WS_THICKFRAME: u32 = 0x0004_0000;
    const WS_MAXIMIZEBOX: u32 = 0x0001_0000;
    assert_eq!(style::WINDOWED & WS_THICKFRAME, 0);
    assert_eq!(style::WINDOWED & WS_MAXIMIZEBOX, 0);
    assert_eq!(style::EX_STYLE, 0);
    assert_eq!(RETAIL_WINDOW_CLASS, "Turbine Device Class");
}

/// Drawing the February 2005 world beside the modern interface files, the client answers the
/// DDD interrogation (`0xF7E6`) with the world it draws: the 2005 portal and cell files as one
/// run of their header iterations (2112 and 1593), and the later language file's own list.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail and February 2005 dats: --features retail-dats"
)]
fn a_client_drawing_the_february_2005_world_answers_ddd_with_that_worlds_iterations() {
    let old = dereth_dat::testing::classic_dat_dir().unwrap_or_else(|| {
        panic!(
            "{}",
            dereth_dat::testing::classic_shortfall().unwrap_or_default()
        )
    });
    let store =
        dereth_dat::RetailDatStore::open_classic_with_modern(&old, &dereth_dat::testing::dat_dir())
            .expect("the two dat sets");
    let r = ddd_interrogation_response(&store, 0);
    let by = |ty: u32, id: u32| {
        r.iters_with_keys
            .iter()
            .find(|l| (l.dat_file_type, l.dat_file_id) == (ty, id))
            .map(|l| (l.iterations.iterations, l.iterations.ints.clone()))
    };
    assert_eq!(by(0, 1), Some((2112, vec![-2112, 1])), "portal");
    assert_eq!(by(1, 2), Some((1593, vec![-1593, 1])), "cell");
    let local = by(1, 3).expect("the language file");
    assert!(local.0 > 0, "the later language file's own list");
}

/// A store the platform opened itself is the one the application reads: bring-up never looks
/// at `dat_dir`, which here names nothing.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn a_store_opened_by_the_platform_is_the_one_the_app_reads() {
    let store = std::sync::Arc::new(dereth_dat::testing::open_store().unwrap_or_else(|| {
        panic!(
            "the retail dats are this test's oracle and they are not under {}",
            dereth_dat::testing::dat_dir().display()
        )
    }));
    let cfg = Config {
        headless: true,
        frames: None,
        connect: false,
        sound: false,
        dat_dir: std::path::PathBuf::from("a directory that does not exist"),
        ..Config::default()
    };
    let app = App::<NullShell>::bring_up_with_store(
        cfg,
        Some(std::sync::Arc::clone(&store)),
        |_| Ok(Platform::headless(64, 64)),
        |_, _, _, _| Ok(Box::new(crate::present::NullPresentation::new(64, 64))),
    )
    .expect("bring-up takes the store it is given");
    assert!(std::sync::Arc::ptr_eq(app.probe().dat_store(), &store));
}

/// Behaviour: none (a host may attach one socket-free transport endpoint).
/// A windowed App takes a relay-fed endpoint, which the replay attachment refuses it, and a
/// second endpoint is handed back while the first is attached.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn a_windowed_app_takes_one_relay_endpoint() {
    let store = std::sync::Arc::new(dereth_dat::testing::open_store().unwrap_or_else(|| {
        panic!(
            "the retail dats are this test's oracle and they are not under {}",
            dereth_dat::testing::dat_dir().display()
        )
    }));
    let cfg = Config {
        headless: false,
        frames: None,
        connect: false,
        sound: false,
        dat_dir: std::path::PathBuf::from("a directory that does not exist"),
        ..Config::default()
    };
    let mut app = App::<NullShell>::bring_up_with_store(
        cfg,
        Some(store),
        |_| Ok(Platform::headless(64, 64)),
        |_, _, _, _| Ok(Box::new(crate::present::NullPresentation::new(64, 64))),
    )
    .expect("bring-up");
    let endpoint = || {
        crate::net::ClientNetwork::new("127.0.0.1", 9000, "account", "password", 1)
            .expect("an endpoint")
    };
    assert!(
        app.attach_replay_network(endpoint()).is_err(),
        "the replay attachment is for a headless run"
    );
    assert!(app.attach_relay_network(endpoint()).is_ok());
    assert!(
        app.replay_network_mut().is_some(),
        "the endpoint is socket-free"
    );
    assert!(
        app.attach_relay_network(endpoint()).is_err(),
        "one link at a time"
    );
}
