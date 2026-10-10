//! The camera -- where the viewer stands and what it looks at.
//!
//! One file per subject, so that two changes adding rows at the same time do not edit the same
//! file. [`ROWS`] is in id order; the registry's own test asserts that, and that no id and no
//! evidence handle is repeated anywhere in it.

// `behaviour!` is `#[macro_export]`ed by `mod.rs` above this module's declaration, so it is in
// textual scope here and needs no import.
use super::{Behaviour, Evidence, Tier, RETAIL, THIS_CLIENT};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "camera.keys.the-alternate-camera-key-turns-the-arrows-into-camera-keys",
        says: "The arrow keys move and turn the character, but while the alternate camera key is \
               held they rotate the camera instead, and releasing it gives them back to movement.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O165-KEYS-ALTERNATE"),
        station: "dereth-client::dat::movement::command_lists::the_arrow_keys_are_movement_until_the_alternate_camera_key_is_held",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "camera.mouse-look.a-drag-is-never-the-first-click-of-a-double-click",
        says: "A press that drags -- the pointer moved past the drag threshold, or held still for \
               a camera drag -- is not taken for a click: a click at once after it, where the \
               drag's held pointer is shown again, is a click and not the second of a \
               double-click, so after a drag of the right button it examines, and after a drag \
               of the left it uses nothing.",
        since: THIS_CLIENT,
        divergence: "CD-040",
        evidence: Evidence::Private("AC-EVID-DRAG-NOT-A-CLICK"),
        station: "dereth-client-shell::lib::front_end::pointer_hold_tests::a_click_at_once_after_a_drag_is_a_click_and_not_a_double_click",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "camera.mouse-look.hides-the-pointer-and-shows-it-again-where-it-began",
        says: "While the mouse turns the camera the pointer is hidden and held still, and the \
               mouse's movement turns the camera as the pointer's would have; when the look \
               ends the pointer is shown again where it was when the look began.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-POINTER-HOLD"),
        station: "dereth-client-shell::lib::front_end::pointer_hold_tests::the_game_cameras_mouse_look_holds_the_pointer_once_dragged_and_shows_it_where_it_began",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "camera.mouse-look.only-moves-from-the-sixth-frame",
        says: "Mouse-look is smoothed over five samples: moving the mouse leaves the camera still \
               for the first five samples on an axis and moves it from the sixth, swinging it \
               around the player without changing its distance.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-CAMERA-MOUSE-LOOK"),
        station: "dereth-client::gpu::camera::camera_collision::mouse_look_only_moves_the_camera_from_the_sixth_frame",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "camera.mouse-look.the-pointer-is-held-once-the-drag-moves-it",
        says: "A press of the button that turns the camera leaves the pointer shown; the \
               pointer is hidden and held only once the drag has moved it past the drag \
               threshold, about four pixels, so a click that examines or selects never hides \
               it.",
        since: THIS_CLIENT,
        divergence: "CD-040",
        evidence: Evidence::Private("AC-EVID-POINTER-HOLD-THRESHOLD"),
        station: "dereth-client-shell::lib::pointer::tests::a_drag_holds_the_pointer_past_the_threshold_and_shows_it_where_it_began",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "camera.mouse-turning.the-body-stops-turning-when-the-mouse-stops-or-the-button-is-let-go",
        says: "With Turn your character with camera turning on, the turn mouse look gives the \
               body stops when the mouse has been still for a fifth of a second under mouse look \
               and when the mouse-look button is let go, and the body then holds its heading.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-MOUSE-TURN-STOP"),
        station: "dereth-client::gpu::camera::turn_keys::with_mouse_turning_on_the_body_stops_turning_when_the_mouse_stops_or_is_let_go",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "camera.pitch.the-drawn-frame-is-pitched-down-as-retail",
        says: "The gameplay camera looks down on the player from behind and above, so upright \
               lines in the world converge downward on screen at the angle the retail camera \
               gives, about 16.7 degrees, instead of being drawn parallel.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O968-PITCH"),
        station: "dereth-client::gpu::camera::camera_pitch::the_drawn_door_silhouette_converges_at_retail_s_sixteen_degrees",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "camera.preferences.reach-the-camera-manager",
        says: "The camera settings in the player's profile (align to slope, adjustment speed and \
               stiffness) reach the running camera at start-up, and changing them on the options \
               page updates the live camera while other preferences are left alone.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-PREFERENCES"),
        station: "dereth-client::gpu::camera::camera_preferences::the_profile_s_camera_preferences_reach_the_camera_manager",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "camera.station.frame-and-projection-match-retail",
        says: "At a recorded spot the settled camera matches retail: the eye rests 2.75 metres \
               behind and 0.825 above a pivot 1.5 metres over the body, aimed at that pivot, with \
               retail's field of view in a full 800 by 600 view and near and far planes of 0.1 and \
               4000.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-STATION"),
        station: "dereth-client::gpu::camera::retail_station_match::at_the_yard_station_the_frame_and_projection_are_retail_s",
        private_oracle: true,
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "camera.target.the-chase-camera-frames-an-attackable-target-in-melee",
        says: "With the camera-tracks-target option on, the player in melee mode and an attackable \
               creature selected, the chase camera frames that creature, looking from the player \
               toward it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O924-TARGET"),
        station: "dereth-client::gpu::camera::chase_camera_target::the_camera_frames_an_attackable_target_in_melee_mode",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "camera.turn.in-first-person-the-turn-keys-turn-the-body",
        says: "In first person the camera turn keys turn the character's body and the view turns \
               with it, because the camera is the head; the heading the client then reports to the \
               shard is one the body actually held.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P3-TURN"),
        station: "dereth-client::gpu::camera::turn_keys::in_first_person_the_turn_key_turns_the_body_and_the_camera_follows",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "camera.wall.the-swept-camera-stays-in-the-room",
        says: "Indoors, the camera behind the player is swept against the walls every frame, so in \
               a small room it stays inside the room most of the time where an unswept camera \
               would end up outside it, and it never ends up inside a wall.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-CAMERA-WALL"),
        station: "dereth-client::gpu::camera::camera_collision::the_swept_camera_stays_in_the_room_the_unswept_one_leaves",
        tier: Tier::Gpu,
    },
];
