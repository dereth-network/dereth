//! The client draws the dat's own cursors: each pointer state resolves through the shipped
//! `UICURSOR` mapper (`DidMapper 0x2500000F`, master group 6) to the cursor whose name the dat
//! gives it (`Default`, `Examine_OverObject`, `TargetedUse_OverInvlaidObject`, ...), with the
//! hotspot the client pushes, and the legal and illegal targeting cursors are the right way round.
//! The mapper's names are read from the dat, so the branch arithmetic is checked against data and
//! not against this file. The cursor manager's override chain and cache are tested in
//! `dereth-ui`, where the manager lives. A cursor is not visible in a window capture, so the
//! evidence is table-level. The cursor update compares only the resolved `DataId` before it pushes
//! a cursor and its hotspot, so it is correct only while no two keys the client can choose resolve
//! to one image at two hotspots: every reachable key is driven and every aliased pair is checked to
//! share one hotspot (the compare is faithful and must not be widened; if these fail, the data
//! changed). Fixture: the retail dats (missing dats fail); one test drives a headless `App`.

use crate::common::client_dir;

use std::collections::BTreeMap;

use dereth_client::cursor::{
    self, cursor_enum, update_cursor_state, CursorInputs, CursorSystem, TargetMode, UICURSOR_GROUP,
};
use dereth_client_model::combat::CombatMode;
use dereth_dat::RetailDatStore;
use dereth_primitives::DataId;

/// **An `expect`, never a skip.** The shipped mapper is this file's only oracle; without it there
/// is nothing to assert, so a missing mapper must fail the test rather than pass without evidence.
fn store() -> RetailDatStore {
    assert!(
        dereth_dat::testing::have_dats(),
        "the shipped UICURSOR mapper is this file's oracle: no dats at {} -- set DERETH_TEST_DAT_DIR",
        client_dir().display()
    );
    RetailDatStore::open_dir(&client_dir()).expect("open the retail dats")
}

/// The `UICURSOR` mapper's own `enum_to_name`, read out of the dat.
///
/// This is the oracle for every branch assertion below, and it is read rather than written down so
/// that a DDD patch that renumbered the cursors would break the test rather than pass it.
fn cursor_names(store: &RetailDatStore) -> Vec<(u32, String)> {
    use dereth_assets::Decode;
    use dereth_primitives::AssetSource;

    let s: &dyn AssetSource = store;
    let master = dereth_assets::DidMapper::decode_payload(
        dereth_client::assets::MASTER_DID_MAPPER,
        &s.read(dereth_client::assets::MASTER_DID_MAPPER)
            .expect("the master DidMapper reads"),
    )
    .expect("the master DidMapper decodes");
    let second = master
        .enum_to_id
        .iter()
        .find(|(k, _)| *k == UICURSOR_GROUP)
        .map(|(_, v)| DataId(*v))
        .expect("master group 6 is UICURSOR");
    assert_eq!(second, DataId(0x2500_000F), "the shipped UICURSOR mapper");
    let m = dereth_assets::DidMapper::decode_payload(
        second,
        &s.read(second).expect("the UICURSOR mapper reads"),
    )
    .expect("the UICURSOR mapper decodes");
    let mut v: Vec<(u32, String)> = m.enum_to_name.clone();
    v.sort_by_key(|(k, _)| *k);
    v
}

// -------------------------------------------------------------------------------------------
// 1. The dat side: 41 cursors, all of them loadable as a 32x32 icon.
// -------------------------------------------------------------------------------------------

/// Oracle: shipped mapper `0x2500000F` names 41 `UICURSOR` entries and no unnamed ID-only entries.
#[test]
fn all_forty_one_shipped_cursors_resolve_through_the_enum_seam() {
    let store = store();
    let s: &dyn dereth_primitives::AssetSource = &store;
    let names = cursor_names(&store);
    assert_eq!(names.len(), 41, "the mapper's enum_to_name half");
    assert_eq!(
        names.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
        (1..=cursor_enum::LAST).collect::<Vec<_>>(),
        "the keys are 1..=41 with no gaps, which is what makes `+ h` arithmetic legal"
    );

    let mut resolved = 0;
    for (k, name) in &names {
        let did = dereth_client::assets::enum_did(s, UICURSOR_GROUP, *k)
            .unwrap_or_else(|| panic!("UICURSOR {k} ({name}) resolves"));
        assert_eq!(
            did.0 >> 24,
            0x06,
            "{name} is a RenderSurface id, got {did:?}"
        );
        resolved += 1;
    }
    assert_eq!(
        resolved, 41,
        "41 of 41 UICURSOR entries resolve to a 0x06xxxxxx surface"
    );

    // The names the state machine's arithmetic depends on, in the places it depends on them.
    let by_key = |k: u32| {
        names
            .iter()
            .find(|(n, _)| *n == k)
            .map(|(_, s)| s.as_str())
            .unwrap()
    };
    assert_eq!(by_key(cursor_enum::DEFAULT), "Default");
    assert_eq!(
        by_key(cursor_enum::DEFAULT_OVER_OBJECT),
        "Default_OverObject"
    );
    assert_eq!(by_key(cursor_enum::COMBAT), "Combat");
    assert_eq!(by_key(cursor_enum::SPELLCAST), "Spellcast");
    assert_eq!(by_key(cursor_enum::EXAMINE), "Examine");
    assert_eq!(by_key(cursor_enum::USE), "Use");
    assert_eq!(by_key(cursor_enum::WAIT), "Wait");
    assert_eq!(by_key(cursor_enum::TARGETED_USE), "TargetedUse");
    assert_eq!(
        by_key(cursor_enum::TARGETED_USE_OVER_OBJECT),
        "TargetedUse_OverObject"
    );
    // The typo is the dat's.
    assert_eq!(
        by_key(cursor_enum::TARGETED_USE_OVER_INVLAID_OBJECT),
        "TargetedUse_OverInvlaidObject"
    );
    // The original default cursor resolves mapper enum 1 at hotspot (0, 0).
    assert_eq!(by_key(1), "Default");
}

/// Oracle: the original icon builder accepts `width < 0x21 && height < 0x21`, then creates a 32x32
/// A8R8G8B8 surface with the source blitted into its corner.
///
/// A cursor the client cannot build an icon from falls all the way through to
/// `LoadCursorA(NULL, IDC_ARROW)`, so "how many of the 41 build" is the number that decides whether
/// this feature is on. The test asserts the full 41-entry denominator.
#[test]
fn every_shipped_cursor_decodes_to_a_surface_the_icon_builder_accepts() {
    let store = store();
    let s: &dyn dereth_primitives::AssetSource = &store;
    let tex = dereth_client::textures::TextureStore::new(&store);

    let mut built = 0;
    let mut sizes = std::collections::BTreeSet::new();
    for (k, name) in cursor_names(&store) {
        let did = dereth_client::assets::enum_did(s, UICURSOR_GROUP, k).expect("resolves");
        let img = tex
            .bgra8(did)
            .unwrap_or_else(|e| panic!("UICURSOR {k} ({name}) {did:?} decodes: {e}"));
        sizes.insert((img.width, img.height));
        let bits = dereth_render::cursor::build(img.width, img.height, &img.pixels, 0, 0)
            .unwrap_or_else(|e| panic!("UICURSOR {k} ({name}) becomes an icon: {e}"));
        assert_eq!(bits.and_mask.len(), 128);
        assert_eq!(bits.color_bgra.len(), 32 * 32 * 4);
        // A cursor whose mask is entirely transparent would draw nothing at all, which looks
        // exactly like a cursor that failed to load.
        assert!(
            bits.and_mask.iter().any(|b| *b != 0xFF),
            "UICURSOR {k} ({name}) has at least one opaque pixel"
        );
        built += 1;
    }
    assert_eq!(built, 41, "41 of 41 shipped cursors build a 32x32 icon");
    // They are **not** all 32x32 -- the shipped set runs from 15x28 to 32x32, and 18 distinct
    // sizes appear. That is exactly why the client's cursor-icon builder blits into a cleared 32x32
    // canvas rather than assuming the source fills it, and why the AND mask starts all-ones: a
    // 15x28 cursor leaves 55% of the icon untouched and it must be transparent, not black.
    assert!(
        sizes
            .iter()
            .all(|(w, h)| *w <= 32 && *h <= 32 && *w > 0 && *h > 0),
        "every shipped cursor fits `width < 0x21 && height < 0x21`: {sizes:?}"
    );
    assert!(
        sizes.len() > 1,
        "and they are genuinely different sizes: {sizes:?}"
    );
    assert!(
        sizes.contains(&(32, 32)),
        "including at least one that fills the icon"
    );
    eprintln!(
        "41 of 41 UICURSOR surfaces built; {} distinct sizes",
        sizes.len()
    );
}

// -------------------------------------------------------------------------------------------
// 2. The state machine, end to end: nine states -> DataID and hotspot.
// -------------------------------------------------------------------------------------------

/// The nine cursor-update states, each named by the cursor the **dat** calls it.
fn nine_states() -> Vec<(&'static str, CursorInputs)> {
    let n = CursorInputs::default;
    vec![
        ("Default", n()),
        (
            "Combat",
            CursorInputs {
                combat_mode: CombatMode::Melee,
                ..n()
            },
        ),
        (
            "Spellcast",
            CursorInputs {
                combat_mode: CombatMode::Magic,
                ..n()
            },
        ),
        (
            "Examine",
            CursorInputs {
                target_mode: TargetMode::Examine,
                ..n()
            },
        ),
        (
            "Use",
            CursorInputs {
                target_mode: TargetMode::Use,
                ..n()
            },
        ),
        ("Wait", CursorInputs { busy: 1, ..n() }),
        (
            "TargetedUse",
            CursorInputs {
                target_mode: TargetMode::UseTarget,
                ..n()
            },
        ),
        (
            "TargetedUse_OverObject",
            CursorInputs {
                target_mode: TargetMode::UseTarget,
                hovering: true,
                target_compatible: true,
                ..n()
            },
        ),
        (
            "TargetedUse_OverInvlaidObject",
            CursorInputs {
                target_mode: TargetMode::UseTarget,
                hovering: true,
                target_compatible: false,
                ..n()
            },
        ),
    ]
}

/// Behaviour: ui.cursor.every-pointer-state-shows-the-shipped-cursor-for-it
/// Every branch driven to the **`DataID` and the hotspot the client resolves**, with the dat's own
/// name for the cursor as the oracle rather than a number written into this file.
#[test]
fn each_of_the_nine_states_pushes_the_did_and_hotspot_the_client_pushes() {
    let store = store();
    let s: &dyn dereth_primitives::AssetSource = &store;
    let names = cursor_names(&store);
    let id_of = |want: &str| -> DataId {
        let (k, _) = names
            .iter()
            .find(|(_, n)| n == want)
            .expect("a shipped cursor name");
        dereth_client::assets::enum_did(s, UICURSOR_GROUP, *k).expect("resolves")
    };

    let mut checked = 0;
    for (name, inputs) in nine_states() {
        // The hover partner of each non-targeting state is the `_OverObject` name; the three
        // targeting rows have no partner because they encode the hover in the key itself.
        let variants: Vec<(String, CursorInputs)> = if inputs.target_mode == TargetMode::UseTarget {
            vec![(name.to_string(), inputs)]
        } else {
            vec![
                (name.to_string(), inputs),
                (
                    format!("{name}_OverObject"),
                    CursorInputs {
                        hovering: true,
                        ..inputs
                    },
                ),
            ]
        };
        for (want_name, i) in variants {
            // A fresh system per row, so the current-data-ID cache cannot mask a wrong answer by
            // suppressing the push.
            let mut cs = CursorSystem::new(None);
            let mut ui = dereth_ui::UiSystem::new((800, 600));
            let choice = cs.update_cursor_state(s, Some(&mut ui), i);

            let want_did = id_of(&want_name);
            assert_eq!(
                cs.current(),
                Some(want_did),
                "{want_name}: resolved current cursor from {i:?} (enum {})",
                choice.enum_value
            );
            let want_hot = if matches!(i.target_mode, TargetMode::Use | TargetMode::UseTarget)
                && i.busy == 0
            {
                (14, 14)
            } else {
                (0, 0)
            };
            assert_eq!(choice.hot, want_hot, "{want_name}: hotspot");
            // ... and it reached the manager as the **default** cursor, hotspot included.
            assert_eq!(
                ui.default_cursor,
                Some((want_did, want_hot.0, want_hot.1)),
                "{want_name}: default cursor data ID and hotspot"
            );
            assert_eq!(
                ui.take_pending_cursor(),
                Some((want_did, want_hot.0, want_hot.1)),
                "{want_name}: pending cursor push reached the UI manager"
            );
            checked += 1;
        }
    }
    assert_eq!(
        checked, 15,
        "6 non-targeting states x hover off/on, plus the 3 targeting states"
    );
}

/// Behaviour: ui.cursor.legal-and-illegal-target-cursors-are-the-right-way-round
/// **A legal target and an illegal one must not resolve to the same
/// cursor**, and the legal one must be the dat's `TargetedUse_OverObject`.
///
/// Oracle: the targeted-use hover key is `0x29 - compatible`, where compatibility is either zero
/// or one.
#[test]
fn the_legal_and_illegal_targeting_cursors_are_different_surfaces_and_the_right_way_round() {
    let store = store();
    let s: &dyn dereth_primitives::AssetSource = &store;
    let names = cursor_names(&store);
    let id_of = |want: &str| -> DataId {
        let (k, _) = names
            .iter()
            .find(|(_, n)| n == want)
            .expect("a shipped cursor name");
        dereth_client::assets::enum_did(s, UICURSOR_GROUP, *k).expect("resolves")
    };

    let legal = id_of("TargetedUse_OverObject");
    let illegal = id_of("TargetedUse_OverInvlaidObject");
    assert_ne!(
        legal, illegal,
        "the two answers must be visibly different surfaces, or the affordance is not there"
    );

    let base = CursorInputs {
        target_mode: TargetMode::UseTarget,
        hovering: true,
        ..CursorInputs::default()
    };
    for (compatible, want) in [(true, legal), (false, illegal)] {
        let mut cs = CursorSystem::new(None);
        let mut ui = dereth_ui::UiSystem::new((800, 600));
        cs.update_cursor_state(
            s,
            Some(&mut ui),
            CursorInputs {
                target_compatible: compatible,
                ..base
            },
        );
        assert_eq!(cs.current(), Some(want), "target_compatible = {compatible}");
    }
}

// -------------------------------------------------------------------------------------------
// 3. The wire: the per-frame call.
// -------------------------------------------------------------------------------------------

/// **The call site.** `App::frame` must run the cursor-state update once per frame.
///
/// This is the assertion that reddens when the per-frame call in `app.rs` is deleted, so the
/// cursor code cannot be implemented and yet have no production caller.
#[test]
fn the_cursor_state_machine_runs_once_per_frame_and_settles_on_the_default_cursor() {
    use dereth_client::app::App;
    use dereth_client::config::Config;
    use dereth_client::present::NullPresentation;

    let dat_dir = client_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "the cursor tests need the retail dats"
    );
    let cfg = Config {
        headless: true,
        ui: true,
        frames: Some(3),
        dat_dir,
        ..Config::default()
    };
    let mut app = App::with_presentation(cfg, Box::new(NullPresentation::new(800, 600)))
        .expect("a headless app");
    // Starting the shell creates the UI system that gates cursor installation.
    app.start_shell().expect("the UI shell comes up");

    assert_eq!(
        app.cursor_stats().updates,
        0,
        "nothing before the first frame"
    );
    for n in 1..=3u64 {
        app.frame();
        assert_eq!(
            app.cursor_stats().updates,
            n,
            "cursor-state update must run on every frame -- frame {n}"
        );
    }

    let st = app.cursor_stats();
    // Idle, out of combat, nothing hovered: `Default`, and it is pushed exactly once because the
    // current-data-ID cache swallows the other two frames.
    let store = store();
    let s: &dyn dereth_primitives::AssetSource = &store;
    let default_did =
        dereth_client::assets::enum_did(s, UICURSOR_GROUP, cursor_enum::DEFAULT).expect("resolves");
    assert_eq!(
        app.current_cursor_did(),
        Some(default_did),
        "current cursor is `Default`"
    );
    assert_eq!(st.state_changes, 1, "three frames, one state change");
    assert_eq!(
        st.device_pushes, 1,
        "and one push past the last-cursor cache"
    );
    // `CreateIconIndirect` really succeeded on a real dat surface: this counts only when
    // `WinCursor::new` returned a live handle, and it runs headless too because building the icon
    // needs no window -- only installing it does.
    assert_eq!(
        st.icons_built, 1,
        "which built exactly one HCURSOR from the dat"
    );
    assert_eq!(st.failures, 0, "nothing failed to resolve or decode");
    // The third state: headless has no window, so the icon was built and deliberately not
    // installed. A windowed run logs
    // `GetCursor() agrees: true` at this point instead.
    assert_eq!(
        st.device_installs, 0,
        "a headless run has no window to install it on"
    );

    app.shutdown();
}

/// The state machine is not a constant: driving one of its inputs moves the answer.
///
/// Oracle: the cursor update's busy arm. This asserts through the same seam the frame uses that
/// replacing the state machine with a constant result fails.
#[test]
fn the_answer_moves_when_an_input_moves() {
    let store = store();
    let s: &dyn dereth_primitives::AssetSource = &store;
    let mut cs = CursorSystem::new(None);
    let mut ui = dereth_ui::UiSystem::new((800, 600));

    cs.update_cursor_state(s, Some(&mut ui), CursorInputs::default());
    let idle = cs.current().expect("Default resolves");
    cs.update_cursor_state(
        s,
        Some(&mut ui),
        CursorInputs {
            busy: 1,
            ..CursorInputs::default()
        },
    );
    let busy = cs.current().expect("Wait resolves");
    assert_ne!(
        idle, busy,
        "busy must not resolve to the same surface as idle"
    );
    assert_eq!(cs.stats.updates, 2);
    assert_eq!(cs.stats.state_changes, 2);

    // ... and going back is a third change, not a suppressed one.
    cs.update_cursor_state(s, Some(&mut ui), CursorInputs::default());
    assert_eq!(cs.current(), Some(idle));
    assert_eq!(cs.stats.state_changes, 3);
}

/// The targeting-compatibility predicate against real object tables.
///
/// The unit tests in `cursor.rs` drive the eight refusal arms as data; this one drives the same
/// function through `dereth_client_model::World`, so source useability and target type, target trade state,
/// source and target ownership, and target/player identity are checked with the logic.
#[test]
fn the_compatibility_test_reads_the_fields_it_claims_to_read() {
    use dereth_primitives::ObjectId;

    const PLAYER: ObjectId = ObjectId(0x5000_0001);
    const WAND: ObjectId = ObjectId(0x5000_0002);
    const ROCK: ObjectId = ObjectId(0x5000_0003);

    let mut w = dereth_client_model::World::new();
    w.player = Some(PLAYER);
    for (id, obj_type) in [(PLAYER, 0x10u32), (WAND, 0x8000), (ROCK, 0x80)] {
        let mut weenie = dereth_client_model::weenie::Weenie::default();
        weenie.pwd.obj_type = obj_type;
        w.tables.weenies.insert(id, weenie);
    }
    // The wand may be used remotely, on a `TYPE_MISC` target.
    {
        let wand = w.weenie_mut(WAND).expect("the wand exists");
        wand.pwd.useability = Some((32 << 16) | 32);
        wand.pwd.target_type = Some(0x80);
    }

    assert!(
        cursor::is_target_compatible_with_targeting_object(&w, WAND, ROCK),
        "a TYPE_MISC target the wand declares it can be used on"
    );
    // The player is TYPE_CREATURE, which the wand does not declare.
    assert!(!cursor::is_target_compatible_with_targeting_object(
        &w, WAND, PLAYER
    ));
    // Put the rock on the trade window and it stops being a legal target.
    w.weenie_mut(ROCK).expect("the rock exists").trade_state = 1;
    assert!(!cursor::is_target_compatible_with_targeting_object(
        &w, WAND, ROCK
    ));
    w.weenie_mut(ROCK).expect("the rock exists").trade_state = 0;
    assert!(cursor::is_target_compatible_with_targeting_object(
        &w, WAND, ROCK
    ));
    // Widening the wand's target type to include `TYPE_CREATURE` is **not** enough to make the
    // player a legal target: aiming at yourself needs `USEABLE_SELF` in the *target* half of
    // `_useability` as well: aimed at the player with the self-target bit clear, the check refuses.
    // This is the arm a rebuild is most likely to drop, because it is the only one that fires when
    // the whole not-owned-by-the-player block was skipped.
    w.weenie_mut(WAND).expect("the wand exists").pwd.target_type = Some(0x90);
    assert!(
        !cursor::is_target_compatible_with_targeting_object(&w, WAND, PLAYER),
        "the type masks intersect, but the wand cannot be used on the self"
    );
    // Set that bit and it becomes legal.
    w.weenie_mut(WAND).expect("the wand exists").pwd.useability = Some(((32 | 2) << 16) | 32);
    assert!(cursor::is_target_compatible_with_targeting_object(
        &w, WAND, PLAYER
    ));
    // ... and the rock, which is not the player, is unaffected either way.
    assert!(cursor::is_target_compatible_with_targeting_object(
        &w, WAND, ROCK
    ));
}

// The aliased cursor keys

/// Every `(enum key, hotspot)` the cursor-state logic can produce, over **every** combination of
/// its five inputs.
///
/// This is the whole reachable set rather than `cursor.rs`'s fifteen-row table, because the claim
/// under test is about what the client can *choose*, and a table is a list of the cases somebody
/// thought of. **4 target modes x 5 combat modes x 2 hover x 2 compatible x 2 busy = 160 drives**,
/// collapsing onto **15** distinct enum keys — the count the caller asserts, so a `CursorInputs`
/// that grows a variant and is not driven here shows up as a changed denominator.
fn reachable() -> BTreeMap<u32, (i32, i32)> {
    let mut out: BTreeMap<u32, (i32, i32)> = BTreeMap::new();
    let mut drives = 0usize;
    for target_mode in [
        TargetMode::None,
        TargetMode::Use,
        TargetMode::Examine,
        TargetMode::UseTarget,
    ] {
        for combat_mode in [
            CombatMode::Undef,
            CombatMode::NonCombat,
            CombatMode::Melee,
            CombatMode::Missile,
            CombatMode::Magic,
        ] {
            for hovering in [false, true] {
                for target_compatible in [false, true] {
                    for busy in [0, 1] {
                        drives += 1;
                        let c = update_cursor_state(CursorInputs {
                            busy,
                            target_mode,
                            combat_mode,
                            hovering,
                            target_compatible,
                        });
                        // The premise of the whole file: one enum key never resolves to two
                        // hotspots *within the function*. If it did, the alias question would be
                        // moot and the compare unfixable.
                        if let Some(prev) = out.insert(c.enum_value, c.hot) {
                            assert_eq!(
                                prev, c.hot,
                                "enum {} is produced at two hotspots by the cursor-state update itself",
                                c.enum_value
                            );
                        }
                    }
                }
            }
        }
    }
    assert_eq!(drives, 160, "the input space this sweep actually covered");
    out
}

fn did_of(s: &RetailDatStore, key: u32) -> Option<DataId> {
    dereth_client::assets::enum_did(s, UICURSOR_GROUP, key)
}

/// Behaviour: ui.cursor.aliased-cursor-keys-share-one-hotspot
/// Every pair of enum keys the client can choose that
/// resolves to one `DataId` must resolve to one hotspot — because the original cursor update
/// compares the did alone, so a colliding pair with different hotspots would leave the wrong
/// hotspot installed.
///
/// Calibrated in both directions: the run fails unless it actually found at least one alias
/// (otherwise a mapper that resolved nothing would pass on silence) **and** unless every reachable
/// key resolved at all.
#[test]
fn every_aliased_pair_of_reachable_cursor_keys_shares_one_hotspot() {
    let s = store();
    let keys = reachable();
    assert_eq!(
        keys.len(),
        15,
        "the cursor-update path's reachable enum keys"
    );

    /// `(enum key, hotspot)`, named so the map below is legible rather than a nest of tuples.
    type KeyHot = (u32, (i32, i32));
    let mut by_did: BTreeMap<DataId, Vec<KeyHot>> = BTreeMap::new();
    let mut unresolved = Vec::new();
    for (&k, &hot) in &keys {
        match did_of(&s, k) {
            Some(d) => by_did.entry(d).or_default().push((k, hot)),
            None => unresolved.push(k),
        }
    }
    assert!(
        unresolved.is_empty(),
        "these reachable cursor keys do not resolve through the shipped mapper: {unresolved:?} \
         -- the instrument saw nothing for them, so their alias status is unmeasured"
    );

    let aliased: Vec<_> = by_did.iter().filter(|(_, v)| v.len() > 1).collect();
    assert!(
        !aliased.is_empty(),
        "no two reachable keys alias onto one DataId -- the instrument found no aliases and \
         therefore cannot test shared-hotspot consistency"
    );

    for (did, group) in &by_did {
        let hots: Vec<(i32, i32)> = group.iter().map(|(_, h)| *h).collect();
        let first = hots[0];
        assert!(
            hots.iter().all(|h| *h == first),
            "UICURSOR {:#010X} is shared by keys {:?} at different hotspots {:?} -- \
             the cursor update compares only the data ID, so moving between these two \
             leaves the earlier hotspot on screen. Read cursor.rs's module note \
             before widening the compare.",
            did.0,
            group.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
            hots
        );
    }

    eprintln!(
        "{} reachable cursor keys resolve to {} distinct DataIds; {} of those are shared by \
         more than one key, and every share is at one hotspot",
        keys.len(),
        by_did.len(),
        aliased.len()
    );
}

/// The module comment's own count and its two named pairs, pinned as literals against the shipped
/// mapper. A constant read back through the same symbol it was written through is unfalsifiable,
/// so the two ids `cursor.rs` names in prose are stated here as independent numbers.
#[test]
fn the_alias_pairs_the_module_comment_claims_are_in_the_shipped_mapper() {
    use dereth_client::cursor::cursor_enum;
    let s = store();

    let examine = did_of(&s, cursor_enum::EXAMINE).expect("Examine resolves");
    let examine_over = did_of(&s, cursor_enum::EXAMINE + 1).expect("Examine_OverObject resolves");
    assert_eq!(
        examine, examine_over,
        "Examine and Examine_OverObject are one image"
    );
    assert_eq!(
        examine.0, 0x0600_4D71,
        "and cursor.rs's module comment names this id"
    );

    let use_c = did_of(&s, cursor_enum::USE).expect("Use resolves");
    let use_over = did_of(&s, cursor_enum::USE + 1).expect("Use_OverObject resolves");
    assert_eq!(use_c, use_over, "Use and Use_OverObject are one image");
    assert_eq!(
        use_c.0, 0x0600_4D72,
        "and cursor.rs's module comment names this id"
    );

    // The whole 41-key mapper, so the "four pairs" in the module comment is a measurement rather
    // than a recollection. The two movement pairs it does not name are discovered here.
    let mut by_did: BTreeMap<DataId, Vec<u32>> = BTreeMap::new();
    let mut missing = Vec::new();
    for k in 1..=cursor_enum::LAST {
        match did_of(&s, k) {
            Some(d) => by_did.entry(d).or_default().push(k),
            None => missing.push(k),
        }
    }
    assert!(
        missing.is_empty(),
        "the mapper's keys are 1..=41 with no gaps; these missed: {missing:?}"
    );
    let shared: Vec<(u32, Vec<u32>)> = by_did
        .iter()
        .filter(|(_, v)| v.len() > 1)
        .map(|(d, v)| (d.0, v.clone()))
        .collect();
    // Pinned as literals against this dat build rather than as a count,
    // because a count alone would survive the table being rearranged.
    assert_eq!(
        shared,
        vec![
            (0x0600_4D71, vec![0x0A, 0x0B]),
            (0x0600_4D72, vec![0x0C, 0x0D]),
            (0x0600_4D7F, vec![0x19, 0x1A]),
            (0x0600_4D81, vec![0x1C, 0x1D]),
        ],
        "cursor.rs's module comment says four aliased pairs; the shipped mapper now says \
         otherwise -- correct the comment and this list together. The first two are named there \
         (Examine and Use); the other two are adjacent key pairs pinned by id and key rather than \
         by name"
    );

    // **The denominator, and the blind spot.** The cursor-update path can reach only **two** of
    // those four pairs: keys 0x0A/0x0B (Examine) and 0x0C/0x0D (Use). That denominator is measured
    // here rather than assumed, by intersecting with `reachable()`. The other
    // two are never produced by that function, so its did-only compare cannot move between them
    // and this file says nothing about their hotspots. Whichever path *does* set them carries its
    // own hotspot with the did, so the question is a different one. "Four pairs" is therefore not
    // four checked pairs.
    let reach = reachable();
    let checked: Vec<u32> = shared
        .iter()
        .filter(|(_, keys)| keys.iter().all(|k| reach.contains_key(k)))
        .map(|(d, _)| *d)
        .collect();
    assert_eq!(
        checked,
        vec![0x0600_4D71, 0x0600_4D72],
        "which aliased pairs cursor-state selection can actually choose between"
    );
}
