//! The pointer is the hourglass while anything the player asked for is still waiting on its
//! answer: the allegiance panel's request when the game screen comes up, an examine, a swing the
//! server commenced, and the portal space. Each answer takes its part down, and with nothing
//! waiting the pointer is the default arrow again.
//! Fixture: the retail dats' cursors; a headless `App` in gameplay with no network, its world
//! answered by hand.

use crate::common::app::app_in_gameplay;
use crate::common::client_dir;

use dereth_client::app::App;
use dereth_client::cursor::{cursor_enum, UICURSOR_GROUP};
use dereth_client_net::client_session::SessionEvent;
use dereth_primitives::{DataId, ObjectId};

const PLAYER: ObjectId = ObjectId(0x5000_000A);
const SWORD: ObjectId = ObjectId(0x8000_0101);

fn cursor(key: u32) -> DataId {
    assert!(
        dereth_dat::testing::have_dats(),
        "the shipped cursors are this file's oracle: no dats at {} -- set DERETH_TEST_DAT_DIR",
        client_dir().display()
    );
    let store = dereth_dat::RetailDatStore::open_dir(&client_dir()).expect("open the retail dats");
    dereth_client::assets::enum_did(&store, UICURSOR_GROUP, key).expect("the shipped cursor")
}

fn settle(app: &mut App) {
    for _ in 0..2 {
        app.frame();
    }
}

/// Behaviour: ui.cursor.the-hourglass-is-up-while-anything-asked-for-waits-for-its-answer
/// Each waiting request puts the hourglass up and its answer takes it down.
///
/// Falsified by a pointer that reads only the portal space (the examine and the swing would leave
/// the arrow up), and by a raise with no matching lowering (the arrow would never come back).
#[test]
fn the_hourglass_is_up_while_a_request_waits_and_down_once_it_is_answered() {
    let (wait, arrow) = (cursor(cursor_enum::WAIT), cursor(cursor_enum::DEFAULT));
    let mut app = app_in_gameplay(3, Some(PLAYER));

    // The game screen's allegiance panel asks for the allegiance as it comes up.
    settle(&mut app);
    assert_eq!(
        app.objects().world.magic.busy_count,
        1,
        "the allegiance request"
    );
    assert_eq!(
        app.current_cursor_did(),
        Some(wait),
        "waiting on the allegiance"
    );
    app.objects_mut()
        .world
        .handle_allegiance_update(&dereth_protocol::social::AllegianceProfile::default());
    settle(&mut app);
    assert_eq!(app.objects().world.magic.busy_count, 0);
    assert_eq!(
        app.current_cursor_did(),
        Some(arrow),
        "the allegiance answered"
    );

    // An examine.
    let mut req = dereth_client_model::RecordingRequests::default();
    app.objects_mut().world.examine_object(&mut req, SWORD);
    settle(&mut app);
    assert_eq!(
        app.current_cursor_did(),
        Some(wait),
        "waiting on the examine"
    );
    let mut sink = dereth_client_model::RecordingSink::default();
    app.objects_mut()
        .world
        .set_appraise_info(SWORD, Default::default(), &mut sink);
    settle(&mut app);
    assert_eq!(
        app.current_cursor_did(),
        Some(arrow),
        "the examine answered"
    );

    // A swing the server commenced.
    app.objects_mut().world.handle_commence_attack();
    settle(&mut app);
    assert_eq!(
        app.current_cursor_did(),
        Some(wait),
        "the swing is outstanding"
    );
    app.objects_mut().world.handle_attack_done(
        &mut req,
        0,
        true,
        dereth_primitives::LocalTime(0.0),
    );
    settle(&mut app);
    assert_eq!(app.current_cursor_did(), Some(arrow), "the swing is over");

    // The portal space: the player appears with its position unsettled, which starts the tunnel.
    app.teleport
        .apply_events(&[SessionEvent::PlayerCreated(PLAYER)]);
    settle(&mut app);
    assert_eq!(app.current_cursor_did(), Some(wait), "in portal space");
    app.teleport.step_world_view(false);
    for _ in 0..900 {
        app.frame();
    }
    assert_eq!(
        app.teleport.anim.state,
        dereth_ui_screens::screens::teleport::TeleportAnimState::Off,
        "the world has faded back in"
    );
    assert_eq!(app.objects().world.magic.busy_count, 0);
    assert_eq!(app.current_cursor_did(), Some(arrow), "out of portal space");

    app.shutdown();
}
