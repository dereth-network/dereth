//! The element manager's cursor: it caches the last cursor it pushed (a moved hotspot is not a
//! redundant push); an element's override wins over the default, capture over hover, and leaving
//! the element restores the default; a device reset re-pushes the cursor instead of leaving a stale
//! one; a media cursor track sets and then clears the element's override.
//!
//! None of this is visible in a frame capture (a window capture does not include the cursor), so
//! the cursor transitions are asserted directly on the manager. Fixture: a `UiSystem` with elements
//! built in the test; no dats.

use dereth_primitives::DataId;
use dereth_ui::{MediaEffect, UiSystem};

/// `Default`, in this dat build.
const A: DataId = DataId(0x0600_4D68);
/// `Examine` / `Examine_OverObject`, which really do share one image.
const B: DataId = DataId(0x0600_4D71);

/// Oracle: the cache's early return compares the did **and both
/// hotspots**, and `INVALID_DID` is recorded as the default but never pushed.
#[test]
fn the_last_cursor_cache_swallows_a_redundant_push_but_not_a_moved_hotspot() {
    let mut ui = UiSystem::new((800, 600));
    ui.set_cursor(A, 0, 0, true);
    assert_eq!(
        ui.take_pending_cursor(),
        Some((A, 0, 0)),
        "the first push reaches the device"
    );
    ui.set_cursor(A, 0, 0, true);
    assert_eq!(
        ui.take_pending_cursor(),
        None,
        "the same did at the same hotspot is swallowed"
    );
    // The shipped `UICURSOR` mapper aliases four pairs of enum keys onto one image, so "same did,
    // different hotspot" really occurs and must not be swallowed.
    ui.set_cursor(A, 14, 14, true);
    assert_eq!(
        ui.take_pending_cursor(),
        Some((A, 14, 14)),
        "a moved hotspot is a real change"
    );

    // `INVALID_DID` records the default and pushes nothing.
    ui.set_cursor(DataId(0), 3, 4, true);
    assert_eq!(ui.default_cursor, Some((DataId(0), 3, 4)));
    assert_eq!(
        ui.take_pending_cursor(),
        None,
        "INVALID_DID never reaches the device"
    );
    assert_eq!(
        ui.last_cursor,
        Some((A, 14, 14)),
        "and does not disturb the last-pushed cursor"
    );
}

/// Behaviour: ui.cursor.an-element-override-wins-and-leaving-restores-the-default
///
/// Oracle: capture beats last-entered beats default, each element gated
/// on `has_cursor`, and the element arm passes "not the default" (`false`).
#[test]
fn the_override_chain_prefers_capture_then_hover_then_the_default() {
    let mut ui = UiSystem::new((800, 600));
    let root = ui.root();
    let hovered = ui.create_hollow(Some(root));
    let captured = ui.create_hollow(Some(root));

    ui.set_cursor(A, 0, 0, true);
    let _ = ui.take_pending_cursor();

    // Nothing entered, nothing captured: the default.
    ui.check_cursor();
    assert_eq!(ui.last_cursor, Some((A, 0, 0)));

    // Hovering an element with no cursor of its own changes nothing: `has_cursor` refuses it.
    ui.switch_mouse_over(Some(hovered));
    assert!(!ui.has_cursor(hovered));
    assert_eq!(
        ui.last_cursor,
        Some((A, 0, 0)),
        "an element with no override is skipped"
    );

    // Give it one: now it wins over the default.
    ui.element_set_cursor(hovered, B, 1, 2);
    assert_eq!(
        ui.last_cursor,
        Some((B, 1, 2)),
        "the hovered element's override"
    );
    assert_eq!(
        ui.default_cursor,
        Some((A, 0, 0)),
        "the override is pushed as not-the-default: it must NOT overwrite the default cursor"
    );

    // Capture outranks hover, even though the pointer is still over `hovered`. This is what keeps
    // a drag cursor up while the pointer crosses other windows.
    ui.element_set_cursor(captured, A, 7, 8);
    ui.set_mouse_capture(captured);
    ui.check_cursor();
    assert_eq!(
        ui.last_cursor,
        Some((A, 7, 8)),
        "the capturing element wins"
    );

    // A capture with no cursor of its own falls through to the hovered element, not to the
    // default: with no capture, or a capture without a cursor, the last-entered element is asked.
    ui.element_unset_cursor(captured);
    assert_eq!(
        ui.last_cursor,
        Some((B, 1, 2)),
        "falls through to the last-entered element"
    );

    // And with neither, back to the default the state machine put there.
    ui.element_unset_cursor(hovered);
    assert_eq!(
        ui.last_cursor,
        Some((A, 0, 0)),
        "back to the default cursor"
    );
    assert_eq!(
        ui.default_cursor,
        Some((A, 0, 0)),
        "which was never overwritten"
    );
}

/// Behaviour: ui.cursor.an-element-override-wins-and-leaving-restores-the-default
///
/// Oracle: the mouse-over switch ends in a cursor check, so the cursor changes when the
/// pointer crosses into and out of an element that has one.
#[test]
fn leaving_an_element_with_an_override_restores_the_default() {
    let mut ui = UiSystem::new((800, 600));
    let root = ui.root();
    let e = ui.create_hollow(Some(root));
    ui.set_cursor(A, 0, 0, true);
    ui.element_set_cursor(e, B, 4, 5);

    ui.switch_mouse_over(Some(e));
    assert_eq!(
        ui.last_cursor,
        Some((B, 4, 5)),
        "entering picks the override up"
    );
    ui.switch_mouse_over(None);
    assert_eq!(
        ui.last_cursor,
        Some((A, 0, 0)),
        "leaving puts the default back"
    );
}

/// Oracle: the last-pushed cursor is cleared to `INVALID_DID` *before* the dirty-region
/// redraw and the saved cursor is re-pushed after, as not-the-default (`false`).
///
/// A device reset destroys the `HCURSOR` the surface was turned into; without the clear,
/// `set_cursor`'s own equality check would swallow the re-push and the pointer would be left stale.
#[test]
fn a_device_reset_re_pushes_the_cursor_instead_of_leaving_a_stale_one() {
    let mut ui = UiSystem::new((800, 600));
    let root = ui.root();
    let hovered = ui.create_hollow(Some(root));

    ui.set_cursor(A, 0, 0, true);
    // An element override is in force at the moment of the reset, so the re-push must restore
    // *that* and must not promote it to the default.
    ui.switch_mouse_over(Some(hovered));
    ui.element_set_cursor(hovered, B, 5, 6);
    assert_eq!(ui.take_pending_cursor(), Some((B, 5, 6)));
    assert_eq!(ui.take_pending_cursor(), None, "drained");

    ui.refresh_event((800, 600));
    assert_eq!(
        ui.take_pending_cursor(),
        Some((B, 5, 6)),
        "the reset re-pushed the cursor that was up"
    );
    assert_eq!(
        ui.default_cursor,
        Some((A, 0, 0)),
        "and did not overwrite the default"
    );

    // Outside a reset, asking for the same cursor again changes nothing -- which is exactly why
    // the clear has to be there.
    ui.set_cursor(B, 5, 6, false);
    assert_eq!(ui.take_pending_cursor(), None);
}

/// Oracle: the media machine's cursor update has two arms — a real file sets the element's
/// cursor override, `INVALID_DID` clears it.
#[test]
fn an_animated_cursor_track_sets_and_clears_the_elements_override() {
    let mut ui = UiSystem::new((800, 600));
    let root = ui.root();
    let e = ui.create_hollow(Some(root));
    ui.set_cursor(A, 0, 0, true);
    ui.switch_mouse_over(Some(e));
    let _ = ui.take_pending_cursor();

    ui.apply_media_effect(
        e,
        MediaEffect::SetCursor {
            file: B,
            hot_x: 9,
            hot_y: 9,
        },
    );
    assert_eq!(
        ui.last_cursor,
        Some((B, 9, 9)),
        "the track's frame reached the manager"
    );
    ui.apply_media_effect(e, MediaEffect::UnSetCursor);
    assert_eq!(
        ui.last_cursor,
        Some((A, 0, 0)),
        "INVALID_DID in the track is UnSetCursor"
    );
}

/// The **media track's own arm**, driven through the media machine rather than
/// through a hand-built effect, so the cursor update's `INVALID_DID` branch is covered
/// where it is written.
///
/// Oracle: the cursor media entry is media type **4**, carrying a file and the two hotspots; the
/// update sets the cursor when the file is not `INVALID_DID`, clears it otherwise, and succeeds.
#[test]
fn a_media_cursor_track_emits_set_then_unset() {
    use dereth_assets::ui::{MediaDesc, MediaFields};
    use dereth_ui::media::MediaPlayback;

    let track = [
        MediaDesc {
            media_type: 4,
            type_echo_ok: true,
            fields: MediaFields::Cursor {
                file: B,
                x_hotspot: 9,
                y_hotspot: 9,
            },
        },
        MediaDesc {
            media_type: 4,
            type_echo_ok: true,
            // `INVALID_DID`.
            fields: MediaFields::Cursor {
                file: DataId(0),
                x_hotspot: 0,
                y_hotspot: 0,
            },
        },
    ];
    let mut m = MediaPlayback::default();
    m.reset(&track);
    let mut rng = dereth_primitives::num::rng::Ran2::new(1);
    let fx = m.update(0.0, &mut rng, true);
    assert_eq!(
        fx,
        vec![
            MediaEffect::SetCursor {
                file: B,
                hot_x: 9,
                hot_y: 9
            },
            MediaEffect::UnSetCursor,
        ],
        "a real file is SetCursor; INVALID_DID is UnSetCursor -- both arms, in order"
    );

    // ... and running them through the manager puts the override up and takes it away again.
    let mut ui = UiSystem::new((800, 600));
    let root = ui.root();
    let e = ui.create_hollow(Some(root));
    ui.set_cursor(A, 0, 0, true);
    ui.switch_mouse_over(Some(e));
    for f in fx {
        ui.apply_media_effect(e, f);
    }
    assert_eq!(
        ui.last_cursor,
        Some((A, 0, 0)),
        "the track ended by clearing its own override"
    );
}
