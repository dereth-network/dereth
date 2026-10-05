//! The world and the body: which movement commands a body with nothing under it may still make,
//! and whose identity the player carries once in the world.
//!
//! The movement rule is driven straight through the motion interpreter and opens no data file; the
//! identity is read off a model-only client that replays `first-login-walk-jump` into the world.
//! `ALL` is this file's own list, concatenated with the other subjects' in `census.rs`, so a
//! scenario that is written and not listed shows up as a shortfall rather than as a silent gap.

use dereth_client_contract::GameView as _;
use dereth_client_runtime::character::PLAYER_OBJECT_ID;
use dereth_testkit::{Given, HeadlessClient};

// -------------------------------------------------------------------------------------------
// movement.contact.only-turning-is-allowed-to-a-body-with-no-ground-under-it
// -------------------------------------------------------------------------------------------

/// A body with nothing under it may still turn, and may do nothing else.
///
/// The asymmetry is the client's own and it is what makes "he can turn but he cannot walk" a
/// recognisable state rather than a contradiction. Both halves of the footing test are driven
/// separately, because a body that is touching something it cannot stand on is refused as
/// firmly as one touching nothing at all -- and a turn never gets as far as either question.
pub fn only_turning_is_allowed_to_a_body_with_nothing_under_it() {
    use dereth_animation::motion::{MotionEnv, MotionInterp};
    use dereth_animation::MotionCommand;

    let air = MotionEnv {
        on_ground: false,
        contact: false,
        ..MotionEnv::default()
    };
    // The configuration the rule is about: a creature with a body, subject to gravity.
    let gated_configuration = air.gravity_affected && air.is_creature && air.has_weenie;

    let exempt = [
        MotionCommand::TURN_LEFT,
        MotionCommand::TURN_RIGHT,
        MotionCommand::DEAD,
        MotionCommand::FALLING,
    ]
    .into_iter()
    .all(|c| MotionInterp::contact_allows_move(c, &air));

    // Sidestepping is the discriminating one: it sits immediately beside the two turns in the
    // client's own numbering and is refused with everything else.
    let refused = [
        MotionCommand::WALK_FORWARD,
        MotionCommand::WALK_BACKWARDS,
        MotionCommand::RUN_FORWARD,
        MotionCommand::SIDE_STEP_LEFT,
        MotionCommand::SIDE_STEP_RIGHT,
        MotionCommand::READY,
    ]
    .into_iter()
    .all(|c| !MotionInterp::contact_allows_move(c, &air));

    // Touching something that is not standable is refused as firmly as touching nothing.
    let contact_only = MotionEnv {
        on_ground: false,
        contact: true,
        ..MotionEnv::default()
    };
    let touching_is_not_standing =
        !MotionInterp::contact_allows_move(MotionCommand::WALK_FORWARD, &contact_only)
            && MotionInterp::contact_allows_move(MotionCommand::TURN_RIGHT, &contact_only);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "movement.contact.only-turning-is-allowed-to-a-body-with-no-ground-under-it",
        move |_| gated_configuration && exempt && refused && touching_is_not_standing,
    );
}

dereth_testkit::scenarios! {
    scenario_only_turning_is_allowed_to_a_body_with_nothing_under_it => only_turning_is_allowed_to_a_body_with_nothing_under_it ["movement.contact.only-turning-is-allowed-to-a-body-with-no-ground-under-it"],
    scenario_the_player_identity_comes_from_the_shard => the_player_identity_comes_from_the_shard ["player.identity.comes-from-the-shard"],
}

// -------------------------------------------------------------------------------------------
// 13. player.identity.comes-from-the-shard
// -------------------------------------------------------------------------------------------

/// The player's identity in the world is the one the shard gave it, not the offline placeholder.
pub fn the_player_identity_comes_from_the_shard() {
    let mut c = HeadlessClient::model();
    let character = {
        c.given(Given::EnteredWorld {
            session: "first-login-walk-jump",
            character: None,
        });
        c.characters()
            .first()
            .cloned()
            .expect("the recording offers a character")
    };
    // A second client, because a login is a login: the replay runs once, all the way through.
    let mut c = HeadlessClient::model();
    let want: &'static str = Box::leak(character.into_boxed_str());
    c.given(Given::EnteredWorld {
        session: "first-login-walk-jump",
        character: Some(want),
    });

    c.assert_behaviour("player.identity.comes-from-the-shard", |v| {
        let Some(player) = v.objects().player() else {
            return false;
        };
        player != PLAYER_OBJECT_ID
            && v.snapshot().player() == Some(player)
            // The shard's own player range, which a placeholder is not in.
            && (0x5000_0001..=0x5FFF_FFFF).contains(&player.0)
    });
}
