//! The world and the body -- movement, physics, the camera and identity.
//!
//! What a moving player reports to the shard, what happens to a body the shard corrects, what the
//! physics refuses, what the camera slides through, and where the local body's identity comes
//! from.
//!
//! One file per subject, so that two changes adding rows at the same time do not edit the same
//! file. [`ROWS`] is in id order; the registry's own test asserts that, and that no id and no
//! evidence handle is repeated anywhere in it.

// `behaviour!` is `#[macro_export]`ed by `mod.rs` above this module's declaration, so it is in
// textual scope here and needs no import.
use super::{Behaviour, Evidence, Tier, RETAIL};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "camera.view.slides-through-a-creature-standing-between-you-and-the-camera",
        says: "The camera keeps its full distance behind the player when a creature stands \
               between the body and the eye, and an object of the very same size and shape that \
               is not a creature pulls the camera in instead.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-9E"),
        station: "dereth-testkit::dat::world::scenario_the_camera_slides_through_a_creature_but_not_through_a_solid_twin",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "camera.wall.settles-in-the-same-place-however-often-the-client-ticks",
        says: "A player who backs into a wall indoors gets a camera that comes to rest and stays \
               there, and it rests in the same place whether the client is ticking thirty times \
               a second or a hundred and forty-four -- over every wall the training academy's \
               rooms offer, not one of them.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O282"),
        station: "dereth-testkit::dat::world::scenario_the_camera_against_a_wall_settles_the_same_at_any_tick_rate",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "map.coordinates.zero-has-no-hemisphere",
        says: "Map and radar coordinates omit hemisphere letters at exactly zero, including negative zero, while tiny nonzero values retain their direction.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-MAP-ZERO-COORDINATE"),
        station: "dereth-presentation::lib::coordinates::tests::zero_components_have_no_hemisphere_and_tiny_values_keep_sign",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "map.page.a-freshly-opened-map-highlights-no-town-at-all",
        says: "A map page the player has just opened shows every town resting: no frame is drawn \
               round any of them, each is inside the map picture, and the frame each one carries \
               is hidden rather than merely undrawn.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-158-REST"),
        station: "dereth-testkit::dat::world::scenario_a_freshly_opened_map_highlights_no_town",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "map.page.every-town-the-shipped-table-names-is-on-the-map-where-it-says",
        says: "Every town the shipped table names is on the world map, each at the point and the \
               size that table gives it, each carrying its own name, and each hung off the map \
               picture so that its place is in the same space the player's own mark is.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-158"),
        station: "dereth-testkit::dat::world::scenario_every_shipped_town_is_on_the_map_where_the_table_says",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "map.page.hovering-a-town-frames-that-town-alone-and-leaving-un-frames-it",
        says: "Putting the pointer on a town draws a frame of four pieces round that town and \
               round no other, and moving the pointer off it, still on the map, takes the frame \
               away again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-158-HOVER"),
        station: "dereth-testkit::dat::world::scenario_hovering_a_town_frames_that_town_alone",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "map.page.hovering-a-town-shows-its-name",
        says: "A town on the map can be pointed at and says its name when it is -- which is the \
               only thing an ordinary player can do on this page, and which a town with no name \
               could not do because the pointer would fall straight through it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-158-TOOLTIP"),
        station: "dereth-testkit::dat::world::scenario_hovering_a_town_shows_its_name",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "map.page.opening-the-map-refreshes-it-at-once-and-closing-it-asks-for-nothing",
        says: "Opening the map refreshes it there and then rather than showing whatever the last \
               look left on it up to five seconds earlier, and the refresh really puts the \
               player's current place on the page; closing it again asks for nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-158-OPEN"),
        station: "dereth-testkit::dat::world::scenario_opening_the_map_refreshes_it_at_once",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "map.page.pressing-the-map-does-nothing-at-all-for-an-ordinary-player",
        says: "Pressing the world map does nothing an ordinary player can see: it does not zoom, \
               it does not move, the towns stay where they were, and nothing is asked of the \
               shard.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-158-PRESS"),
        station: "dereth-testkit::dat::world::scenario_pressing_the_map_does_nothing_for_an_ordinary_player",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "map.page.the-date-and-the-time-are-drawn-on-the-page",
        says: "The date and the time are written on the map page on two lines and are really \
               drawn there, not merely stored in the element that holds them.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-158-CLOCK"),
        station: "dereth-testkit::dat::world::scenario_the_date_and_time_are_drawn_on_the_page",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "map.page.the-map-is-one-picture-drawn-once-over-the-whole-map-element",
        says: "The world map is one picture, the shipped one, drawn exactly once over the whole \
               of the element that holds it -- which is the other half of there being no zoom.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-158-IMAGE"),
        station: "dereth-testkit::dat::world::scenario_the_map_is_one_picture_over_the_whole_element",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "map.page.the-players-own-dot-is-where-the-shipped-arithmetic-puts-it",
        says: "The player's own mark on the world map sits at the point his coordinates scale to \
               inside the shipped marker box, to the pixel, at the size the layout gave it, and \
               it moves east when he does and nowhere else.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-158-DOT"),
        station: "dereth-testkit::dat::world::scenario_the_players_dot_is_where_the_arithmetic_puts_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "map.regions.follow-world-profile",
        says: "Both interfaces show the connected world profile's map regions and Yanshi position, independently of interface artwork.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-MAP-WORLD-PROFILE"),
        station: "dereth-client-contract::lib::panels::map::tests::profiles_select_regions_and_yanshi_position",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "map.teleport.refuses-invalid-coordinates",
        says: "A map teleport sends the bounded landscape cell with the fixed destination origin and heading, and sends nothing for invalid coordinates.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-MAP-TELEPORT-CELLS"),
        station: "dereth-client-runtime::lib::interaction::tests::map_teleport_uses_bounded_landscape_cells_and_preserves_destination",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "map.window.a-second-look-at-an-unchanged-world-writes-nothing",
        says: "Looking at the map again while nothing about the player has changed leaves every \
               part of it exactly as it was and reports that nothing was written, so a change \
               that does appear there is a change and not a redraw.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F67-IDLE"),
        station: "dereth-testkit::dat::world::scenario_a_second_look_at_an_unchanged_world_writes_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "map.window.indoors-the-date-keeps-going-and-only-the-coordinates-and-the-mark-stop",
        says: "Inside a building the map stops giving the player's coordinates -- it empties the \
               line rather than hiding it -- and hides his mark, but the date and time go on \
               being written, so a player who walks into a dungeon does not read a stale hour \
               afterwards. Stepping back outside brings all three back exactly as they were.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F67-INDOORS"),
        station: "dereth-testkit::dat::world::scenario_indoors_the_date_keeps_going_and_only_the_rest_stops",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "map.window.is-redrawn-once-every-five-seconds-and-the-first-look-is-due-at-once",
        says: "The map window is redrawn once every five seconds rather than every frame, and \
               the very first look is due the moment the window exists; four and nine-tenths \
               seconds later is not yet due and five seconds later is.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F67-THROTTLE"),
        station: "dereth-testkit::dat::world::scenario_the_map_is_redrawn_once_every_five_seconds",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "map.window.outdoors-it-shows-the-date-the-coordinates-and-the-players-own-mark",
        says: "Out in the world the map window carries the date and the time on two lines, the \
               player's coordinates with north before east, and his own mark placed inside the \
               map picture at the point those coordinates scale to and left at the size the \
               layout gave it. The mark for a house the player does not own stays hidden.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F67"),
        station: "dereth-testkit::dat::world::scenario_outdoors_the_map_shows_the_date_the_coordinates_and_the_mark",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "map.window.the-players-own-mark-follows-him-with-east-to-the-right-and-north-up",
        says: "The player's mark on the map moves as he does: east takes it right, west left, \
               north up and south down, and moving east or west does not move it up or down.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F67-MARKER"),
        station: "dereth-testkit::dat::world::scenario_the_players_mark_follows_him_east_right_and_north_up",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "map.window.with-no-clock-yet-the-date-line-keeps-its-labels",
        says: "Between entering the world and the game's own clock arriving, the map's date line \
               keeps both of its labels and writes a space after each rather than being left \
               blank.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F67-NO-CLOCK"),
        station: "dereth-testkit::dat::world::scenario_with_no_clock_yet_the_date_line_keeps_its_labels",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.action.a-shard-authored-swing-holds-every-motion-behind-it-for-its-own-animation",
        says: "A swing the shard tells the body to play, which the body has nothing in hand for, \
               holds every motion queued behind it: the player presses a key and nothing moves, \
               for exactly as long as that swing's own animation lasts and not a frame longer, \
               and then the key that was held the whole time moves him.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O694"),
        station: "dereth-testkit::dat::world::scenario_a_shard_authored_swing_holds_the_body_for_its_own_animation",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.action.leaving-the-ground-empties-the-motion-ledger-and-routes-what-it-drained",
        says: "Leaving the ground throws away every motion the body was still waiting on and \
               hands each of them to the part of the client that finishes them, so the long \
               animation is gone from the body's own sequence before it lands and the shard's \
               swing is gone from what the body thinks it is doing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O694-EDGE"),
        station: "dereth-testkit::dat::world::scenario_leaving_the_ground_empties_the_motion_ledger_and_routes_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.approach.a-jump-ends-an-approach-whether-or-not-the-jump-is-allowed",
        says: "Pressing jump while the body is walking somewhere of its own accord ends that \
               walk at once, whether the jump itself is allowed or refused, and a refused jump \
               gives the body no push at all and leaves it standing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-LOCAL-MOTION-JUMP"),
        station: "dereth-testkit::dat::world::scenario_a_jump_ends_an_approach_whether_or_not_it_is_allowed",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.approach.a-key-press-ends-an-approach-on-the-spot",
        says: "A movement key the player presses ends a walk the body was making of its own \
               accord on the very frame it is pressed, rather than waiting for anything else to \
               notice, and the body is free to be driven afterwards.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-LOCAL-MOTION-KEY"),
        station: "dereth-testkit::dat::world::scenario_a_key_press_ends_an_approach_on_the_spot",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.approach.a-replacement-ends-the-old-approach-before-the-new-one-is-installed",
        says: "A movement handed to a body that is already walking somewhere ends that walk \
               before the new one is put in place, reports the walk as having failed exactly \
               once rather than again on the following frames, and leaves the body free to be \
               driven by hand afterwards.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-LOCAL-MOTION"),
        station: "dereth-testkit::dat::world::scenario_a_replacement_ends_the_old_approach",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.approach.a-second-one-replaces-the-first-rather-than-queueing",
        says: "Using a second thing while the body is already walking to the first turns it round rather than \
               queueing behind it: the first walk is reported cancelled, and the thing being watched becomes \
               the new one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O134-SECOND"),
        station: "dereth-testkit::dat::world::motion::scenario_a_second_approach_replaces_the_first",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.approach.a-target-that-leaves-the-world-ends-it-and-says-which-way-it-went",
        says: "A thing that is not there ends the walk to it, and the two ways that happens are told apart: \
               one is that it was never there at all, the other that somebody else got to it while the player \
               was on his way.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O134-TARGET-GONE"),
        station: "dereth-testkit::dat::world::motion::scenario_a_target_that_leaves_the_world_ends_the_approach",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.approach.a-walked-one-leaves-the-players-own-way-of-moving-alone",
        says: "A walk the shard sends that is close enough to be walked rather than run chooses its own pace \
               for the duration and leaves the player's own choice of walking or running exactly as it was, \
               so the next key he presses moves him the way he had set.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F15-MOVE-TO"),
        station: "dereth-testkit::dat::world::motion::scenario_a_walked_approach_leaves_the_players_own_movement_alone",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.approach.ends-by-stopping-within-reach-and-nothing-else-happens",
        says: "Arriving is just stopping. The body comes to rest within the reach the shard named, stops \
               watching the thing, and the client sends nothing: whatever the player asked for happens on the \
               shard, which is watching for the walk to be over.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O134-ARRIVAL"),
        station: "dereth-testkit::dat::world::motion::scenario_an_approach_ends_by_stopping_within_reach",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.approach.gives-up-only-when-the-body-has-strayed-further-than-the-shard-allowed",
        says: "A player shuffling against a wall is never abandoned by his own client. The one thing that \
               makes it give a walk up is the body having strayed further from where it started than the \
               shard allowed, and then it says so.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O134-FAIL-DISTANCE"),
        station: "dereth-testkit::dat::world::motion::scenario_an_approach_gives_up_only_on_straying_too_far",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.approach.is-the-shards-and-the-client-never-starts-one-itself",
        says: "The client never walks the player anywhere of its own accord. The use goes to the shard and \
               the shard answers by telling the player's own body to walk to the thing, naming that thing's \
               own reach, measured as a clearance rather than as a distance between centres.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O134-SERVER-DRIVEN"),
        station: "dereth-testkit::dat::world::motion::scenario_the_approach_is_the_shards_own",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.approach.neither-an-approach-nor-a-target-update-leaves-a-motion-behind",
        says: "Walking to a place, and following an object through every update of where that \
               object is, both leave the body with nothing outstanding when they finish -- so \
               nothing afterwards believes the body is still busy.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O684-LEDGER"),
        station: "dereth-testkit::dat::world::scenario_neither_an_approach_nor_a_target_update_leaves_a_motion_behind",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.approach.once-it-is-over-the-player-can-walk-the-body-himself-again",
        says: "While the shard is walking the body the player's keys do nothing, and the body is not handed \
               back on its own while he is holding nothing. The first movement key he presses afterwards \
               takes it back, and the body moves.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O134-CONTROL-BACK"),
        station: "dereth-testkit::dat::world::motion::scenario_after_an_approach_the_player_has_his_body_back",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.approach.the-old-approachs-clean-up-never-outlives-the-next-approachs-subject",
        says: "When one approach replaces another, what the body ends up watching is the new \
               one's subject: the old one's tidying cannot arrive afterwards and leave the body \
               watching nothing, and neither can the old one's own deadline once it has passed.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-LOCAL-MOTION-TARGET"),
        station: "dereth-testkit::dat::world::scenario_the_old_approachs_clean_up_never_outlives_the_next",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.approach.the-target-is-re-read-on-a-gate-and-only-when-it-has-moved",
        says: "A walking client does not ask about the thing it is walking to every frame. The first answer \
               is never held back; after that it asks again only once a gate's worth of time has passed and \
               the thing has actually moved, and a thing that stays put says nothing more.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O134-TARGET-GATE"),
        station: "dereth-testkit::dat::world::motion::scenario_the_target_is_re_read_on_a_gate",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.approach.walks-to-what-is-out-of-reach-and-never-to-what-is-already-in-reach",
        says: "A body told to walk to something out of reach closes the distance; told to walk to something \
               already within the reach the shard named it does not take a step, and the instruction is over \
               on the first answer about the thing. Exactly at the reach counts as arrived.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O134-REACH"),
        station: "dereth-testkit::dat::world::motion::scenario_an_approach_walks_only_to_what_is_out_of_reach",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.contact.a-body-that-loses-its-footing-with-nothing-to-announce-it-turns-for-ever",
        says: "A body whose footing goes without anything announcing the change turns as freely \
               as ever and walks nowhere at all, and nothing puts it right by itself: it does not \
               fall, because it is still touching the ground it cannot stand on, and only being \
               put down somewhere -- which does announce a change -- gives it back.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-FABLE-DOOR-LATCH"),
        station: "dereth-testkit::dat::world::scenario_a_body_that_loses_its_footing_with_no_edge_turns_for_ever",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.contact.only-turning-is-allowed-to-a-body-with-no-ground-under-it",
        says: "A body with nothing under it may still turn, and may do nothing else: walking, \
               running, sidestepping and standing to attention are all refused, and touching \
               something it cannot stand on is refused as firmly as touching nothing at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-FABLE-DOOR-CONTACT"),
        station: "dereth-testkit::cpu::world::scenario_only_turning_is_allowed_to_a_body_with_nothing_under_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "movement.correction.a-body-on-its-way-somewhere-takes-the-place-and-keeps-its-own-facing",
        says: "A body that is already on its way somewhere walks onto the place the shard corrected it to \
               without turning to the way the shard was facing; a body that is going nowhere of its own does \
               turn to it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-114D-HEADING"),
        station: "dereth-testkit::dat::world::motion::scenario_a_body_on_its_way_keeps_its_own_facing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.correction.a-body-the-client-has-not-simulated-yet-is-put-straight-onto-the-shards-position",
        says: "A body so far away that the client has run no physics for it yet is put straight \
               onto the position the shard named, with no glide and without waiting for a frame.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-114C-SNAP"),
        station: "dereth-testkit::dat::world::scenario_a_body_not_yet_simulated_is_put_straight_onto_the_shards_position",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.correction.a-body-walks-a-correction-at-its-own-speed-and-not-a-default",
        says: "A body walked onto a position the shard corrected it to moves at its own speed rather than at \
               a speed the client picked, so the walk is over in the time that speed implies and a fast \
               creature does not crawl.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-114D-SPEED"),
        station: "dereth-testkit::dat::world::motion::scenario_a_correction_runs_at_the_bodys_own_speed",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.correction.a-correction-out-of-reach-is-taken-in-one-step-instead-of-a-glide",
        says: "A correction further away than a body could plausibly have walked is taken in one \
               step rather than glided, and the body ends up on the position the shard named with \
               nothing left to walk.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-114C-BLIP"),
        station: "dereth-testkit::dat::world::scenario_a_correction_out_of_reach_is_taken_in_one_step",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.correction.a-creature-the-shard-has-said-nothing-about-walks-at-the-clients-own-rate",
        says: "A creature the shard has said nothing about is corrected at the client's own starting rate. \
               That is the right answer and not a rate the client failed to fetch: a body that is not the \
               player's own never carries one of its own.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-114E-DEFAULT-RATE"),
        station: "dereth-testkit::dat::world::motion::scenario_a_creature_the_shard_said_nothing_about_walks_at_the_default",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.correction.a-creature-the-shard-says-is-running-walks-at-the-rate-the-shard-gave-it",
        says: "The only way a creature's speed reaches the player's client is the movement message the shard \
               sends about it, and that message is honoured: the rate in it is remembered and the next \
               correction walks the creature at it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-114E-WIRE-RATE"),
        station: "dereth-testkit::dat::world::motion::scenario_a_running_creature_walks_at_the_rate_the_shard_sent",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.correction.a-destination-in-a-block-the-client-has-not-loaded-takes-the-body-out-of-the-world",
        says: "A correction whose destination lies in a piece of land the client has not loaded \
               takes that body out of the world rather than gliding it there or retrying later, \
               and the very same correction glides or lands normally once the land is loaded.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-140"),
        station: "dereth-testkit::dat::world::scenario_a_destination_in_an_unloaded_block_takes_the_body_out_of_the_world",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.correction.a-nearby-correction-glides-the-body-over-the-following-frames",
        says: "A nearby correction moves nothing when it arrives: the body glides onto the \
               position the shard named over the following frames, and a creature standing in \
               the way stops the glide short instead of being pushed out of it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-114C"),
        station: "dereth-testkit::dat::world::scenario_a_nearby_correction_glides_the_body_over_the_following_frames",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.correction.a-teleport-out-of-the-world-drops-a-glide-that-was-already-in-flight",
        says: "A teleport into land the client has not loaded takes the body out of the world and \
               drops any glide that was already in flight, so nothing afterwards drags the body \
               back towards where it had been going.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-114C-TELEPORT"),
        station: "dereth-testkit::dat::world::scenario_a_teleport_out_of_the_world_drops_a_glide_in_flight",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.correction.an-update-that-does-not-say-the-body-is-on-the-ground-moves-it-nowhere",
        says: "A position update that does not say the body is standing on the ground leaves that \
               body exactly where it was, however far away the position it carries is.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-114B"),
        station: "dereth-testkit::dat::world::scenario_an_update_with_no_ground_contact_moves_the_body_nowhere",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.door.no-way-of-opening-a-door-leaves-a-body-that-turns-but-cannot-walk",
        says: "Twenty-seven journeys through a training-dungeon door -- the shard's own recorded \
               answer with seventeen different things happening around it, six with the door \
               standing in the way as something to collide with, and four approaches and \
               openings one after another -- every one of them ends with a body that both turns \
               and walks.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-FABLE-DOOR"),
        station: "dereth-testkit::dat::world::scenario_no_door_journey_leaves_a_body_that_turns_but_cannot_walk",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.follow.every-way-a-follow-ends-reports-it-once-and-leaves-the-body-free",
        says: "Beginning to follow something does not disturb a walk already in progress, and \
               every way that follow can end -- letting go, a second follow replacing it, losing \
               sight of what was followed, or its own time running out -- reports it exactly \
               once, leaves the right thing being followed afterwards, and leaves the body free.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-LOCAL-MOTION-STICKY"),
        station: "dereth-testkit::dat::world::scenario_every_way_a_follow_ends_reports_it_once",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.jump-charge.a-charged-jump-turns-but-does-not-walk-until-the-body-leaves-the-ground",
        says: "A player holding a charged jump can turn and cannot walk, and neither pressing \
               escape nor clearing everything he has asked for gives it back. Releasing the \
               jump does, and so does any movement the shard sends him, and so does a teleport \
               whose arrival changes the ground under him.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-FABLE-DOOR-CHARGE"),
        station: "dereth-testkit::dat::world::scenario_a_charged_jump_turns_but_does_not_walk",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.jump.a-body-in-the-air-turns-but-does-not-walk-and-takes-up-the-held-key-when-it-lands",
        says: "A body whose feet are off the ground may turn and may not walk, and the forward key the player \
               is holding is remembered rather than thrown away: the moment the floor comes back under it the \
               body walks, with no second press and nothing forcing it back to standing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-AF14-AIRBORNE"),
        station: "dereth-testkit::dat::world::motion::scenario_a_body_in_the_air_turns_but_does_not_walk",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.jump.a-jump-puts-the-bodys-own-position-and-speed-on-the-wire",
        says: "Jumping puts one message on the wire carrying the body's own position at the \
               moment it left the ground, the speed the jump gave it and how hard the jump was \
               pressed, and the body really is off the ground and rising afterwards.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O393"),
        station: "dereth-testkit::dat::world::scenario_a_jump_reports_the_bodys_own_position_and_speed",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.jump.a-running-jump-carries-its-speed-in-the-bodys-own-frame",
        says: "A player who jumps while running reports his speed in his own body's frame rather \
               than the world's: forward along the way he is facing and almost nothing sideways, \
               however far round he has turned first.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O393-RUNNING"),
        station: "dereth-testkit::dat::world::scenario_a_running_jump_carries_its_speed_in_the_bodys_own_frame",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.jump.a-second-jump-in-mid-air-is-refused-and-nothing-goes-out",
        says: "Pressing jump again while the body is still in the air is refused, and the refusal \
               is silent on the wire: the shard hears about the first jump and about no other.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O393-MIDAIR"),
        station: "dereth-testkit::dat::world::scenario_a_second_jump_in_mid_air_sends_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.jump.every-drawn-frame-visits-the-jump-dispatch-and-invents-none",
        says: "Every frame the client draws goes through the place a jump would be issued from, \
               and a client nobody is pressing jump on issues none and tells the shard nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O393-FRAME"),
        station: "dereth-testkit::dat::world::scenario_every_drawn_frame_visits_the_jump_dispatch",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.jump.the-height-the-client-works-out-explains-the-recorded-jumps",
        says: "The speeds the recorded clients sent for their jumps are the height this client \
               works out from how hard the jump was pressed and how good the jumper is: some of \
               them land on the shortest jump the client allows, which many jumpers could have \
               made, and at least one is higher and could have been made by exactly one jumper \
               in a thousand.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O393-HEIGHT"),
        station: "dereth-testkit::dat::world::scenario_the_clients_jump_height_explains_the_recorded_jumps",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.motion-ledger.gates-turning-and-walking-alike-and-a-fresh-key-press-escapes-it",
        says: "While the client is still waiting on a motion it cannot finish, it refuses to take \
               control back for a held walk and for a held turn alike -- the two are gated \
               together, not apart -- and a key the player presses afresh takes control back \
               anyway and both work.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-FABLE-DOOR-LEDGER"),
        station: "dereth-testkit::dat::world::scenario_a_full_ledger_stops_turning_and_walking_alike",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.move-to.an-exact-approach-does-not-carry-the-body-through-what-is-in-the-way",
        says: "A walk the shard sends aims the body exactly at where it is going, which is an aim no key \
               press can produce. Even so the body is stopped by whatever stands in the way and is not \
               carried through it, however hard the player leans on the forward key.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O940"),
        station: "dereth-testkit::dat::world::motion::scenario_an_exact_approach_does_not_go_through_what_is_in_the_way",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.placement.a-placement-inside-a-building-takes-and-raises-the-change-of-footing-it-implies",
        says: "Putting a body down inside a building takes, exactly as putting one down on open \
               ground does, and the change of footing that implies is announced -- which is what \
               empties a motion the client is stuck waiting on, so the body falls, lands and \
               answers a held key again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-FABLE-DOOR-PLACEMENT"),
        station: "dereth-testkit::dat::world::scenario_a_placement_inside_a_building_takes_and_raises_its_footing_change",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.position-report.a-walking-body-reports-where-it-is-and-a-still-one-falls-silent",
        says: "A player walking tells the shard where he is about once a second, and again \
               whenever he crosses into a new piece of ground, and every one of those reports \
               carries the position the body really was standing at. A player standing still \
               reports once and then says nothing at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O384-EMISSION"),
        station: "dereth-testkit::dat::world::scenario_a_walking_body_reports_where_it_is_and_a_still_one_falls_silent",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.position-report.every-drawn-frame-reaches-the-reporter-and-invents-nothing",
        says: "Every frame the client draws reaches the part of it that reports the player's \
               position, and a client with no shard and no body reports nothing rather than \
               inventing a position to report.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O384-FRAME"),
        station: "dereth-testkit::dat::world::scenario_every_drawn_frame_reaches_the_position_reporter",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.position-report.the-state-the-client-reports-is-the-bodys-own",
        says: "What the client tells the shard about where the player is is taken from the \
               player's own body and not from a copy of it: the same room, the same point to the \
               last decimal, the same way of facing, and the ground the body is really standing \
               on.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O384"),
        station: "dereth-testkit::dat::world::scenario_the_reported_state_is_the_bodys_own",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.position-report.walking-forward-reports-the-state-the-recordings-carry",
        says: "Beginning to walk forward changes what the client says it is doing into the \
               description the recorded clients sent while walking, with nothing in it that is \
               already the default, and letting go puts it back to saying nothing at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O384-STATE"),
        station: "dereth-testkit::dat::world::scenario_walking_forward_reports_the_state_the_recordings_carry",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.remote-body.a-create-onto-an-occupied-spot-is-put-down-beside-it",
        says: "A body that first appears on a spot another body already fills is put down beside \
               it rather than inside it, and never further away than a few paces.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-114B-CREATE"),
        station: "dereth-testkit::dat::world::scenario_a_create_onto_an_occupied_spot_is_put_down_beside_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.run.a-body-whose-animation-table-is-replaced-still-runs-when-the-player-said-run",
        says: "Anything the shard sends that replaces a body's animations puts it back to walking pace, and \
               the player never touched anything. The next time he presses a movement key the body works out \
               afresh whether he is running, so it runs again without him toggling anything.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F15-WALK-BIT"),
        station: "dereth-testkit::dat::world::motion::scenario_a_replaced_animation_table_does_not_strand_the_body_in_walk",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.run.reports-walk-with-the-run-hold-key",
        says: "Running forward moves the body at run speed but is reported to the shard as \
               walking forward with the run key held; a separate run-forward command never \
               travels.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O391"),
        station: "dereth-testkit::dat::world::scenario_running_reports_walk_with_the_run_hold_key",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.run.the-report-a-running-body-sends-is-the-one-the-retail-client-sent",
        says: "What a running player's client says about its own movement is byte for byte what \
               the retail client said while running, and beginning to run really does put one \
               such report on the wire.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O391-CORPUS"),
        station: "dereth-testkit::dat::world::scenario_a_running_bodys_report_is_the_retail_clients_own",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.run.turning-run-on-while-already-walking-speeds-the-body-up-and-changes-the-report",
        says: "Turning running on while already walking forward makes the body travel faster and \
               changes what the client reports about it, and turning it off again slows the body \
               and changes the report back.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O391-SHIFT"),
        station: "dereth-testkit::dat::world::scenario_turning_run_on_mid_walk_speeds_the_body_and_changes_the_report",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.server-control.a-key-release-stops-the-body-after-the-shard-has-taken-control",
        says: "After the shard has moved the player's body itself, the next key he presses takes \
               control back, and letting that key go stops him -- rather than leaving him \
               running until he presses the key that means stop.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O684"),
        station: "dereth-testkit::dat::world::scenario_a_key_release_stops_the_body_after_the_shard_has_taken_control",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.server-control.a-press-that-takes-control-back-re-issues-the-key-already-held",
        says: "A key press that takes control back from the shard re-issues whatever the player \
               was already holding, so a player who was running when the shard stopped him and \
               who then presses turn runs while turning rather than turning on the spot.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O684-REISSUE"),
        station: "dereth-testkit::dat::world::scenario_a_press_that_takes_control_back_re_issues_the_key_already_held",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.server-control.a-turn-release-stops-the-turn-the-same-way",
        says: "The same holds for turning, which the client keeps on a list of its own: after \
               the shard has taken control, pressing a turn key takes it back and letting go \
               stops the turn rather than leaving the body spinning.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O684-TURN"),
        station: "dereth-testkit::dat::world::scenario_a_turn_release_stops_the_turn_the_same_way",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.teleport.a-player-arriving-on-another-player-through-the-shards-own-login-is-not-deflected",
        says: "A player teleported onto the exact spot another player stands on lands there to \
               the last decimal and tells the shard that very position, with both bodies arriving \
               the way a logged-in client receives them rather than being put there by hand.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-114F"),
        station: "dereth-testkit::dat::world::scenario_an_arrival_through_the_login_path_lands_on_the_other_players_spot",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.teleport.a-recorded-teleport-moves-the-body-and-the-reports-name-the-destination",
        says: "A teleport the recorded shard sent moves the player's own body to where it said, \
               the body stands on the ground there through the pause that follows, and every \
               word the client then sends about where it is names the destination -- never the \
               place it was taken from.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O418-BODY"),
        station: "dereth-testkit::dat::world::scenario_a_recorded_teleport_moves_the_body_and_the_reports_follow",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.teleport.a-teleport-into-a-room-lands-the-body-on-its-floor",
        says: "A teleport whose destination is inside a building puts the body on that room's \
               floor and keeps it there, even though nothing had loaded the room before the \
               teleport arrived.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O418-INTERIOR"),
        station: "dereth-testkit::dat::world::scenario_a_teleport_into_a_room_lands_the_body_on_its_floor",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.teleport.a-teleport-onto-the-same-kind-of-ground-raises-no-edge-and-empties-nothing",
        says: "A teleport that arrives standing on the same kind of ground it left raises no \
               change of footing at all and leaves everything the body was waiting on exactly \
               where it was -- so what frees a body is the change of footing and not the \
               teleport.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O694-NO-EDGE"),
        station: "dereth-testkit::dat::world::scenario_a_teleport_onto_the_same_ground_raises_no_edge",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.teleport.a-teleport-that-changes-the-ground-empties-the-ledger-and-the-body-moves-again",
        says: "A teleport that takes the body off the ground and drops it back raises both \
               changes of footing, and the first of them throws away everything the body was \
               waiting on -- so a player held still by a swing he cannot play can move again the \
               moment he arrives.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O694-TELEPORT"),
        station: "dereth-testkit::dat::world::scenario_a_teleport_that_changes_the_ground_frees_the_body",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.teleport.a-teleport-with-no-shard-involvement-leaves-a-body-that-still-moves",
        says: "A body teleported with the shard taking no part at all lands on the ground and is \
               still driven by the keys the player holds, which is what makes the same question \
               asked around a shard-driven teleport a question about the handover of control.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O684-CONTROL"),
        station: "dereth-testkit::dat::world::scenario_a_teleport_with_no_shard_involvement_leaves_a_movable_body",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.teleport.arrives-exactly-on-another-player-and-is-pushed-aside-only-by-a-creature",
        says: "Teleporting onto another player's exact spot lands exactly there, while a creature \
               whose body overlaps that spot pushes the arrival a short way aside, and a body \
               that passes through everything is not pushed at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-114"),
        station: "dereth-testkit::dat::world::scenario_a_teleport_onto_another_player_is_deflected_only_by_a_creature",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.teleport.every-drawn-frame-applies-a-teleport-before-it-reports-a-position",
        says: "Every frame the client draws applies a teleport the shard has sent before it \
               reports where the player is, so the first thing said after a teleport names the \
               destination; a client with no shard and no body teleports nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O418-FRAME"),
        station: "dereth-testkit::dat::world::scenario_every_drawn_frame_applies_a_teleport_before_reporting",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.teleport.every-teleport-the-recordings-carry-is-applied-and-no-other-position-is",
        says: "Of all the messages the recorded shards sent about where the player is, only the \
               few that are teleports move his body: they are a small fraction of the traffic, \
               they appear in some recordings and not others, and the rest leave the body to the \
               client's own simulation.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O418"),
        station: "dereth-testkit::dat::world::scenario_every_recorded_teleport_is_applied_and_no_other_position_is",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.teleport.several-teleports-in-a-row-each-move-the-body",
        says: "A recording that teleports the player several times moves him to each destination \
               in turn and leaves him standing on the last of them, rather than following only \
               the first and ignoring the rest.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O418-SEQUENCE"),
        station: "dereth-testkit::dat::world::scenario_several_teleports_in_a_row_each_move_the_body",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.teleport.the-player-can-move-again-after-the-shard-teleports-him",
        says: "A player running into a portal arrives where the shard sent him, standing on the \
               ground and not drifting, and the moment he presses a key he runs again -- even \
               though he let go of the key he was holding while the shard had control of him.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O684-PORTAL"),
        station: "dereth-testkit::dat::world::scenario_the_player_can_move_again_after_the_shard_teleports_him",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.teleport.two-players-pass-through-each-other-only-while-neither-is-marked-for-combat",
        says: "Two player bodies pass through one another only while they are not both marked for \
               the same kind of player combat and neither of them is impenetrable; otherwise one \
               arriving on the other is pushed aside exactly as a creature would push it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-114F-PAIRS"),
        station: "dereth-testkit::dat::world::scenario_two_player_bodies_collide_only_for_the_pairs_that_are_not_exempt",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.walk.reports-walking-forward-with-no-hold-key",
        says: "Walking forward with running turned off is reported to the shard as walking \
               forward and no modifier at all, and letting go of the key reports nothing again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O391-WALK"),
        station: "dereth-testkit::dat::world::scenario_walking_forward_reports_a_walk_with_no_hold_key",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "physics.buildings.a-body-walking-into-a-building-from-the-street-is-stopped-by-its-wall",
        says: "A body walking into a building from the street is stopped by or slides along its \
               outer wall and never ends up inside the wall, where without the building's shell it \
               would have walked in.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-BUILDINGS-BUILDINGS"),
        station: "dereth-physics::dat::collision::building_shells::a_body_walking_into_a_holtburg_building_from_the_street_is_stopped_by_its_outer_wall",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "physics.cells.a-body-resting-on-a-cell-line-reports-the-same-cell-from-both-directions",
        says: "A body that walks onto the line between two 24-metre squares of ground and stops \
               there is placed in the same square whichever side it came from: the one the \
               client's own cell choice picks, not the square the floor alone would give.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O439-CELLS"),
        station: "dereth-physics::cpu::cells::boundary_cell::a_walk_resting_on_a_24_m_line_reports_the_same_cell_from_both_directions",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "physics.cells.an-outdoor-object-registers-every-land-cell-its-part-boxes-reach",
        says: "An object standing outdoors is counted as present in exactly the squares of ground \
               its parts reach: a small object inside one square is in that square alone, and a \
               long one spanning three squares is in those three and no others.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O424-CELLS"),
        station: "dereth-physics::dat::cells::outdoor_bbox_registration::an_outdoor_object_registers_exactly_the_land_cells_its_part_boxes_reach",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "physics.cells.cell-lookup-stops-at-the-resident-window",
        says: "Only the ground the client currently holds around the player can be found by cell: \
               a cell in a neighbouring block outside that window is treated as not there, even \
               though the game data could describe it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1106B-CELLS"),
        station: "dereth-physics::cpu::cells::resident_window::an_outdoor_cell_outside_the_resident_window_resolves_to_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "physics.collision.a-cylinder-only-object-stops-a-walking-body",
        says: "An object whose only solid shape is an upright cylinder stops a body walking into \
               it, so the body never enters the cylinder at any point of its walk, where the same \
               object without that shape would be walked straight through.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-CYLSPHERE-COLLISION"),
        station: "dereth-physics::cpu::collision::cylsphere::a_cylsphere_only_object_stops_a_body_that_used_to_walk_through_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "physics.doors.a-door-presents-the-same-surface-from-both-faces-after-an-open-and-close-cycle",
        says: "A door that has been opened, walked through and closed again stops a body at the \
               same distance as a door that never moved, from the front and from the back, and \
               while it is open a body walks straight through.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-FABLE-DOOR-COLLISION-DOORS"),
        station: "dereth-physics::dat::doors::door_collision::an_open_and_close_cycle_leaves_both_contact_surfaces_unchanged",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "physics.doors.turning-ethereal-off-waits-until-the-doorway-is-clear",
        says: "A door told to become solid while someone is standing in it stays passable and \
               keeps retrying every physics step, becoming solid on its own the moment the doorway \
               is clear, so it never closes on top of a player.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O112-DOORS"),
        station: "dereth-physics::cpu::doors::ethereal_toggle::the_latched_retry_fires_every_substep_and_takes_when_the_body_leaves",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "physics.enter-world.refuses-a-body-inside-a-building",
        says: "An object may only enter the world where its body actually fits: on open ground \
               outside a building it is placed there, inside a room while claiming to stand \
               outdoors it is refused outright, and the same point claiming the room it is really \
               in is accepted.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G34"),
        station: "dereth-testkit::dat::world::scenario_enter_world_refuses_a_body_inside_a_building",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "physics.motion.setting-spin-does-not-wake-a-sleeping-body",
        says: "Giving a resting body a spin does not wake it: over a whole simulated second it \
               neither turns nor moves, even with a velocity loaded, until something else wakes \
               it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F69-MOTION"),
        station: "dereth-physics::cpu::motion::set_omega::set_omega_does_not_wake_a_sleeping_body",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "physics.step-down.a-body-stepping-onto-a-sphere-rides-its-tangent-plane",
        says: "A body walking over a dome-shaped object rides its surface frame by frame, standing \
               on the dome's own slope where it touches, and stops where the dome becomes too \
               steep to walk on.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-SPHERE-STEP-DOWN-STEP-DOWN"),
        station: "dereth-physics::cpu::collision::sphere_step_down::the_body_rides_the_dome_frame_by_frame_with_a_unit_contact_normal",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "physics.step-up.the-step-up-budget-decides-whether-an-obstacle-is-climbable",
        says: "A body walking at a ledge a little lower than its step-up height climbs it and \
               stands on top, while the same ledge a little higher than that height is not climbed \
               at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O434-STEP-UP"),
        station: "dereth-physics::cpu::transition::step_up_budget::the_step_up_budget_is_what_separates_a_climbable_obstacle_from_one_that_is_not",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "physics.transition.a-body-whose-transition-fails-keeps-its-origin",
        says: "A body pressed against something solid that is asked to move forward and turn at \
               once stays where it is but still turns, and its remembered speed drops to zero.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O940-TRANSITION"),
        station: "dereth-physics::cpu::transition::failed_transition::a_failed_transition_keeps_the_bodys_origin_and_takes_only_the_rotation",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "player.identity.comes-from-the-shard",
        says: "The player's own body in the world is identified by the identity the shard gave it \
               at login and not by a placeholder the client invented, and that identity is what \
               the client's own snapshot of the frame reports.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-PLAYER-ID"),
        station: "dereth-testkit::cpu::world::scenario_the_player_identity_comes_from_the_shard",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "radar.blips.are-exactly-the-things-the-radar-shows-that-are-inside-its-range",
        says: "What the radar draws is exactly the things it is allowed to show that are also \
               inside its own range, and never the player himself -- and at least one recording \
               really has something outside the range, so the range is doing work and not merely \
               restating the filter.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O72-BLIPS"),
        station: "dereth-testkit::dat::world::scenario_the_blips_are_the_showable_objects_inside_the_range",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "radar.blips.every-one-takes-the-colour-its-own-description-asks-for",
        says: "Every thing the radar shows takes the colour its own description asks for -- the \
               shard's own override first, then a portal, a trader, a creature and the several \
               kinds of player -- over every recording in the corpus and every thing in them.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O72-COLOUR"),
        station: "dereth-testkit::dat::world::scenario_every_blip_takes_the_colour_its_description_asks_for",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "radar.blips.the-only-things-that-take-no-colour-of-their-own-are-ordinary-players",
        says: "The only things on the radar that take no colour of their own are ordinary players \
               at peace, whom the client leaves plain on purpose; no creature, trader, portal or \
               item ever falls through to it. Several different colours appear across the \
               recordings, and the one the shard's own marked traders take is among them.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O72-WHITE"),
        station: "dereth-testkit::dat::world::scenario_no_blip_falls_through_to_the_colour_that_means_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "radar.click.a-click-on-a-thing-on-the-radar-selects-the-thing-it-stands-for",
        says: "Clicking something on the radar selects one of the things within reach of the \
               point clicked -- worked out afresh from the shard's own descriptions rather than \
               from the client -- and, where only one thing is in reach, that one; never the \
               player himself, and never something the shard did not send. A thing with the \
               radar's own padlock or a neighbouring window drawn over it cannot be clicked at \
               all, which is what being in front means and not a shortfall, and most of the ring \
               is still reachable.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O108"),
        station: "dereth-testkit::dat::world::scenario_a_click_on_a_thing_on_the_radar_selects_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "radar.click.an-empty-part-of-the-radar-is-transparent-and-selects-nothing",
        says: "A part of the radar with nothing on it is not something the pointer can land on at \
               all, so a click there falls through to whatever is behind it and selects nothing; \
               the same radar over one of the things on it can be landed on.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O108-EMPTY"),
        station: "dereth-testkit::dat::world::scenario_empty_radar_is_transparent_and_selects_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "radar.drag.a-radar-the-player-has-moved-writes-its-new-place-into-his-own-options",
        says: "A radar the player has dragged writes its new place into his own saved options, as \
               its two coordinates against the window's own number, so that it is still there \
               when he next logs in -- and a frame in which nothing moved writes nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O108-PLACEMENT"),
        station: "dereth-testkit::dat::world::scenario_a_moved_radar_writes_its_new_place_into_the_options",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "radar.drag.the-handle-moves-the-window-by-how-far-the-pointer-moved-and-keeps-it-on-screen",
        says: "The handle in the corner of the radar drags the whole window by exactly how far \
               the pointer moved rather than jumping it to the pointer, does not resize it, lets \
               go on release, and will not let it leave the screen at either corner.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O108-DRAG"),
        station: "dereth-testkit::dat::world::scenario_the_handle_moves_the_radar_and_keeps_it_on_screen",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "radar.filter.is-the-difference-between-a-crowded-radar-and-a-readable-one",
        says: "The same recorded scene drawn with nothing filtered out puts far more on the radar \
               than the client does, in every recording -- which is the difference between a ring \
               a player can read and one he cannot.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O72-RATIO"),
        station: "dereth-testkit::dat::world::scenario_the_filter_is_the_difference_between_a_crowd_and_a_handful",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "radar.filter.only-what-the-shard-marks-for-the-radar-reaches-it",
        says: "Only the things the shard has marked for the radar reach it -- a thing it said \
               nothing about never appears -- and that throws away more than half of everything \
               in view, in every recording.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O72"),
        station: "dereth-testkit::dat::world::scenario_only_what_the_shard_marks_reaches_the_radar",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "radar.hover.the-name-shown-over-a-thing-is-the-shards-own-name-for-it",
        says: "Resting the pointer on something on the radar shows the shard's own name for that \
               thing, and sets the flag the client reads before it will begin to show one at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O108-TOOLTIP"),
        station: "dereth-testkit::dat::world::scenario_the_name_over_a_thing_is_the_shards_own",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "radar.lock.the-lock-the-player-asks-for-reaches-his-options-and-survives-a-rebuilt-screen",
        says: "Locking the interface from the radar's padlock travels the whole way: the screen \
               hides its drag handle and draws the shut padlock, the player's own saved options \
               learn of it, and a screen built afresh from those options comes up locked with \
               nobody having clicked anything.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O108-MIRROR"),
        station: "dereth-testkit::dat::world::scenario_the_lock_reaches_the_players_options_and_survives_a_rebuild",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "radar.lock.whether-the-interface-is-locked-is-one-bit-of-the-players-own-options",
        says: "Whether the interface is locked is a single bit of the player's own saved options, \
               a fresh character has it clear, and it survives being packed into the blob the \
               shard keeps it in and read back out again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O108-FLAG"),
        station: "dereth-testkit::dat::world::scenario_the_lock_is_one_bit_of_the_players_options",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "radar.padlock.a-click-on-it-asks-to-flip-the-lock-and-does-nothing-else",
        says: "A click on the radar's padlock asks for the interface lock to be flipped, exactly \
               once and to the opposite of what it was, whichever it was -- and it selects \
               nothing, although it lands inside the radar.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O108-CLICK"),
        station: "dereth-testkit::dat::world::scenario_clicking_the_padlock_asks_to_flip_the_lock",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "radar.padlock.is-up-and-drawing-its-open-picture-from-the-first-frame",
        says: "The small padlock in the corner of the radar is there from the moment the screen \
               comes up and draws its open picture, which is a different picture from its shut \
               one; the handle for dragging the ring about is there and hidden.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O72-PADLOCK"),
        station: "dereth-testkit::dat::world::scenario_the_radars_padlock_is_up_and_open_from_the_first_frame",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "radar.padlock.swaps-its-two-pictures-with-the-lock-and-the-drag-handle-follows",
        says: "The padlock draws its shut picture while the interface is locked and its open one \
               while it is not -- two different pictures, both ways round and back again -- and \
               the handle for dragging the radar about is hidden exactly while it is locked.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O108-PICTURES"),
        station: "dereth-testkit::dat::world::scenario_the_padlock_swaps_its_pictures_and_the_handle_follows",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "radar.padlock.works-the-same-at-either-range-and-neither-moves-the-other",
        says: "The padlock on the radar behaves the same whether the player is inside or out: the \
               click still asks for the interface to be locked, locking it still hides the drag \
               handle and draws the shut padlock, and none of it moves how far the radar reaches.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O423-PADLOCK"),
        station: "dereth-testkit::dat::world::scenario_the_padlock_works_the_same_at_either_range",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "radar.player.his-mark-is-laid-on-the-radars-own-element-after-the-blips",
        says: "The player's mark really reaches the radar on the screen: it is the last nine \
               things drawn on that element, in the order the client issues them, and the things \
               around him are still drawn in front of it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O72-ELEMENT"),
        station: "dereth-testkit::dat::world::scenario_the_players_mark_is_laid_on_the_radar_element",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "radar.player.his-own-place-is-nine-bright-green-points-on-the-centre",
        says: "The player's own place on the radar is a cross of nine bright green points on the \
               centre of the ring, drawn one point at a time in a fixed order -- and he is not \
               one of the things on the radar at all, so the mark could never have come from \
               them.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O72-CENTRE"),
        station: "dereth-testkit::dat::world::scenario_the_players_mark_is_nine_bright_green_points_on_the_centre",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "radar.range.a-dungeon-and-the-inside-of-a-building-are-the-same-case",
        says: "A dungeon and the inside of an above-ground building are one case to the client, \
               not two: every room in the shipped data is numbered above the boundary the one \
               question asks about, both take the short range, and the open land of the very same \
               pieces of land takes the long one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O423-DUNGEON"),
        station: "dereth-testkit::dat::world::scenario_a_dungeon_and_a_building_inside_are_the_same_case",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "radar.range.every-place-a-recording-puts-the-player-takes-the-range-its-room-asks-for",
        says: "At every place the recordings put the player, the range the radar uses is the one \
               his own room asks for, and the client's own answer about whether he is outside \
               agrees with the room he is in. The recordings between them offer both kinds of \
               room, so neither case goes unmeasured.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O423-TABLE"),
        station: "dereth-testkit::dat::world::scenario_every_recorded_place_takes_the_range_its_room_asks_for",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "radar.range.follows-the-player-as-he-steps-inside-and-out-again",
        says: "The radar's range follows the player: the same world and the same place, with only \
               the room differing, gives the long range outside and the short one in, and \
               stepping back outside gives the long one back. A client with no player at all uses \
               the short one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O423-SEAM"),
        station: "dereth-testkit::dat::world::scenario_the_range_follows_the_player_in_and_out",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "radar.range.the-chat-windows-own-sweep-uses-the-same-radius-as-the-radar",
        says: "The sweep the chat window makes for something near enough to talk to uses the same \
               radius the radar does: the same scene finds fewer things when the player is inside \
               than when he is out, and more than none either way.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O423-AUTOTARGET"),
        station: "dereth-testkit::dat::world::scenario_the_chat_sweep_uses_the_same_radius",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "radar.range.the-things-on-the-radar-really-zoom-when-the-player-goes-inside",
        says: "The same things drawn from the same place look different on the radar once the \
               player is inside: fewer of them, because what is beyond the short range is cut, \
               and every one that survives both sits further out from the centre -- except \
               anything standing on the player himself, which is already at the centre and \
               cannot move. Each is drawn at the ring's own size divided by the range in force.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O423-ZOOM"),
        station: "dereth-testkit::dat::world::scenario_the_radar_zooms_when_the_player_goes_inside",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "radar.range.there-are-two-and-one-question-about-the-players-own-room-decides",
        says: "The radar reaches seventy-five paces out of doors and twenty-five inside, and one \
               question about the room the player is in decides which: rooms numbered below the \
               boundary are open land and those at or above it are inside, whatever piece of land \
               they belong to.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O423"),
        station: "dereth-testkit::dat::world::scenario_there_are_two_radar_ranges_and_one_question",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "radar.range.what-the-radar-shows-stops-one-short-of-its-own-range-at-either-range",
        says: "The edge of what the radar shows sits one pace inside the range in force, at both \
               ranges: something a third of a pace inside it is drawn and something a third of a \
               pace outside it is not, and the edge moves with the range.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O423-CULL"),
        station: "dereth-testkit::dat::world::scenario_what_the_radar_shows_stops_one_short_of_its_range",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "radar.shared-roles-and-projection-variants",
        says: "Radar interfaces share object visibility and relationship roles while preserving their palette, shape, range-boundary and projection conventions; hidden objects use the default selection color.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-RADAR-PRESENTATION"),
        station: "dereth-client-contract::lib::radar::tests::roles_and_projection_preserve_interface_conventions",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "scenery.animation.a-delayed-effect-still-happens-when-the-frame-empties-the-queue",
        says: "A piece of scenery whose shipped animation schedules its next effect for later still produces \
               it, although the client takes everything that object has raised off it every single frame. A \
               torch keeps flickering for the whole session and not only for the first one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-163-DRAIN"),
        station: "dereth-testkit::dat::world::motion::scenario_a_delayed_scenery_effect_survives_the_frames_drain",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.building.a-body-in-a-building-registers-its-interior-cell",
        says: "A body standing inside one of Holtburg's buildings, even while it is still counted \
               in the outdoor cell around it, is also registered in the building's interior room, \
               so the room's walls and contents apply to it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O447-BUILDING"),
        station: "dereth-client::dat::world::building_interior_transit::a_body_standing_in_a_holtburg_building_registers_that_buildings_interior_cell",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.cell-statics.an-interior-cells-baked-objects-are-drawn-and-collide",
        says: "Standing in a room of the training dungeon draws the furniture baked into that \
               room: every pixel those placed objects add to the frame falls inside their own \
               outlines on screen.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-CELL-STATICS-CELL-STATICS"),
        station: "dereth-client::gpu::world::cell_statics::standing_in_the_training_dungeon_draws_its_furniture",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "world.cell-statics.step-height-decides-which-static-can-be-climbed",
        says: "The step height a player's body carries, 0.6 metres, decides what they can walk up \
               onto without jumping: a low object in a Holtburg room is climbed, and no single \
               frame of any climb lifts the body by more than that step.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-CELL-STATICS-CELL-STATICS-STEP"),
        station: "dereth-client::gpu::world::cell_statics::the_setups_step_height_says_which_of_two_statics_can_be_climbed",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "world.click.a-click-answered-with-nothing-in-view-reports-nothing-picked",
        says: "A click answered on a frame that drew no world at all reports that nothing was \
               picked, instead of leaving the answer at the value the request was armed with, and \
               a click nobody armed is never answered.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O449"),
        station: "dereth-testkit::dat::world::scenario_a_scene_less_click_reports_nothing_picked",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.collision.a-building-shell-stops-a-body-only-when-registered",
        says: "A body walking into the outside wall of a Holtburg building is stopped by it \
               because the client registers the building's shell with the ground it stands on; \
               without that registration the same walk passes through the wall.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-SOLID-COLLISION"),
        station: "dereth-client::dat::world::collision_obstacles::a_body_walking_into_a_holtburg_wall_is_stopped_only_when_the_client_registers_the_shell",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.house.a-closed-house-stops-a-stranger-at-its-boundary",
        says: "A character with no permission is stopped at a closed house's boundary by the \
               house's barrier: they never cross into the fenced cell, are not dropped through the \
               floor, and end up outside the property.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P172-HOUSE"),
        station: "dereth-client::gpu::world::house_barrier::a_stranger_is_stopped_at_a_closed_houses_boundary",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "world.interiors.a-body-inside-a-building-is-stopped-by-its-walls",
        says: "A body walking inside a Holtburg building is stopped by the building's walls and \
               never ends up inside solid geometry, yet every heading with a clear body-width line \
               out through a doorway does get out, and no body leaves a room that has no way out.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-INTERIORS-INTERIORS"),
        station: "dereth-client::gpu::world::interiors::a_body_inside_a_holtburg_building_is_stopped_by_its_walls",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "world.landblock-release.a-returning-parent-rescues-its-held-child",
        says: "An outdoor object that left with its landblock and comes back when the block \
               returns cancels the pending destruction for itself and for everything it holds, so \
               a held item is not deleted when the old deadline runs out.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P168B-LANDBLOCK-RELEASE"),
        station: "dereth-client::gpu::world::landblock_object_release::an_outdoor_parent_returning_with_its_block_rescues_its_held_child_from_the_old_deadline",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "world.landblock-release.objects-in-a-departed-blocks-outdoor-cells-are-released",
        says: "When a landblock leaves the window, a portal standing on its outdoor terrain is \
               released from its cell and stops being drawn, rather than lingering on the ground \
               behind the player.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P168-LANDBLOCK-RELEASE"),
        station: "dereth-client::gpu::world::landblock_object_release::outdoor_objects::a_portal_on_the_terrain_of_a_departed_block_is_released_rather_than_left_drawn",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "world.lod.a-coarse-landblock-bakes-no-objects",
        says: "A landblock the window holds only at coarse detail carries no scenery, no static \
               objects and no buildings; objects are placed only on blocks held at full detail.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F55-LOD"),
        station: "dereth-client::gpu::world::coarse_block_objects::a_coarse_block_bakes_no_objects_of_any_kind",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "world.physics-script.a-body-walking-at-a-scaled-object-stops-at-the-scaled-distance",
        says: "A body walking at an object the shipped data has made bigger is stopped at the \
               bigger distance, and the same walk at the same object at its own size comes \
               closer -- so the size the data asked for is the size the body actually meets.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-154-WALK"),
        station: "dereth-testkit::dat::world::scenario_a_body_walking_at_a_scaled_object_stops_further_away",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.physics-script.a-call-with-no-delay-still-plays-on-the-spot",
        says: "A shipped script that calls another one with no pause at all still plays it on \
               the step it asked for, with no delay reported and nothing brought forward.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-153-IMMEDIATE"),
        station: "dereth-testkit::dat::world::scenario_a_script_call_with_no_delay_plays_on_the_spot",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.physics-script.a-delayed-call-plays-the-script-it-names-after-the-delay-it-rolled",
        says: "A shipped script that asks for another one after a pause really plays it: the \
               pause is rolled once when the step comes round, nothing happens before it is up, \
               and what the second script does then is visible in the world.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-153"),
        station: "dereth-testkit::dat::world::scenario_a_delayed_script_call_plays_after_its_delay",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.physics-script.a-scale-hook-moves-the-objects-own-collision-radius",
        says: "A shipped script that makes an object bigger makes the body other things bump \
               into bigger with it, in both its width and its height, from the size the object's \
               own description gave it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-154"),
        station: "dereth-testkit::dat::world::scenario_a_scale_hook_moves_the_collision_radius",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.physics-script.an-object-that-leaves-its-room-before-the-delay-is-up-plays-nothing",
        says: "An object that leaves the world before a delayed script of its own is due plays \
               nothing at all, the waiting timer is thrown away rather than left lying about, \
               and coming back does not bring it round again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-153-CANCEL"),
        station: "dereth-testkit::dat::world::scenario_an_object_that_leaves_before_the_delay_plays_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.placement.a-downward-placement-is-clamped-to-the-surface",
        says: "An object the server places below the ground settles on the terrain surface however \
               deep the requested height, while one placed above the ground keeps its height.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P121-PLACEMENT"),
        station: "dereth-client::gpu::world::placement_surface_clamp::a_downward_placement_is_clamped_to_the_surface_and_an_upward_one_is_honoured",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "world.placement.an-unplaceable-outdoor-object-is-destroyed-after-twenty-five-seconds",
        says: "An outdoor object the client cannot place in any cell is kept for a while and then \
               destroyed about twenty-five seconds after it was created: it is gone from the \
               world's object table and is no longer drawn.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P122-PLACEMENT"),
        station: "dereth-client::gpu::world::unplaceable_object_lifetime::an_unplaceable_outdoor_cell_object_is_destroyed_after_twenty_five_seconds",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "world.portal-space.a-portal-says-in-portal-space-on-entry-and-every-re-aim",
        says: "Entering a portal prints the fixed in-portal-space line on the frame the tunnel \
               opens and again every time the portal re-aims, and the line reaches the on-screen \
               message strip.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-PORTAL-SPACE"),
        station: "dereth-client::gpu::world::portal_space_chat_line::a_portal_says_in_portal_space_on_entry_and_at_every_re_aim_and_the_line_reaches_the_spew_box",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "world.portals.every-building-opening-leads-into-a-cell-of-its-own-block",
        says: "Every door and window in a building's shell opens into an interior room on the same \
               patch of ground, and that room's matching opening leads back outdoors.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-BUILDING-PORTALS-PORTALS"),
        station: "dereth-world-render::dat::world::building_portals::every_building_portal_leads_into_a_cell_of_its_own_block",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.recentre.every-drawn-frame-writer-runs-after-the-recentre",
        says: "On a landblock crossing the window re-centres before anything drawn is placed: the \
               player's body parts land where the new block puts them, a whole block away from \
               where the old block would have put them.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O426-RECENTRE"),
        station: "dereth-client::gpu::world::landblock_recentre::frame_writers::the_body_writer_is_on_the_far_side_of_the_recentre",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "world.scene-less-frame.a-click-and-a-double-click-are-answered-so-the-gestures-after-them-still-work",
        says: "A click and a double-click made on a frame that drew no world are both answered \
               on that frame, so the right-click that follows them is still allowed to ask what \
               it is pointing at.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O415-CLICK"),
        station: "dereth-testkit::dat::world::scenario_a_click_with_nothing_drawn_is_answered_too",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.scene-less-frame.a-drop-armed-with-nothing-drawn-is-answered-and-the-next-click-still-works",
        says: "An item dropped into the world on a frame that drew no world is answered on that \
               same frame rather than being left hanging: the drag ends with the item back in \
               the pack, the client says it cannot be done there, and the very next click in the \
               viewport still works.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O415"),
        station: "dereth-testkit::dat::world::scenario_a_drop_armed_with_nothing_drawn_is_answered",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.scene-less-frame.a-frame-with-nothing-armed-answers-nothing-and-clears-nothing",
        says: "A frame that drew no world and has nobody waiting on an answer raises no answer \
               at all, and a gesture that was parked without ever asking the question stays \
               parked -- which is what the client does with a drop aimed outside the viewport.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O415-IDLE"),
        station: "dereth-testkit::dat::world::scenario_a_frame_with_nothing_armed_answers_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.scene-less-frame.the-answer-names-nothing-so-a-dropped-item-takes-the-ground-leg",
        says: "The answer a frame with nothing drawn gives names no object at all, even when the \
               last real answer named a creature standing right there, so the item being dropped \
               goes to the ground rather than being handed to whoever was found last.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O415-ZERO"),
        station: "dereth-testkit::dat::world::scenario_the_scene_less_answer_names_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.sight.a-probe-across-an-interior-wall-is-blocked",
        says: "Inside buildings and dungeons, a line of sight aimed through one of a room's walls \
               is blocked by that wall, while a short line that stays inside the same room is not, \
               for every room found in the sampled places.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-INDOOR-SIGHT-SIGHT"),
        station: "dereth-client::dat::world::interior_sight_probe::an_interior_probe_across_a_wall_blocks_and_a_short_one_does_not",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.statics.a-scripted-statics-hooks-reach-its-own-collision-body",
        says: "An object baked into a landblock that runs a script owns its own collision body, \
               and the script's size change reaches it: the one shipped static whose script \
               doubles its size doubles its collision radius as well.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1171-STATICS"),
        station: "dereth-client::gpu::world::scripted_static_collision_body::scene::the_one_shipped_static_that_a_script_doubles_doubles_its_collision_body",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "world.streaming.a-block-that-returns-re-bakes-bit-identically",
        says: "Walking far enough that the home landblock leaves the window and then walking back \
               re-bakes the landscape bit-identically: the frame at home after the round trip \
               matches the first visit pixel for pixel.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O260-STREAMING"),
        station: "dereth-client::gpu::world::landblock_round_trip::a_round_trip_moves_no_pixel_and_the_landscape_re_bakes_bit_identically",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "world.streaming.a-block-that-scrolls-away-and-back-holds-the-same-slots",
        says: "A landblock window that scrolls away from its blocks and back ends up holding the \
               same number of texture slots it started with: blocks that leave give their slots \
               back, and blocks that return share their surfaces rather than piling up copies.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O571-STREAMING"),
        station: "dereth-client::gpu::world::streaming_slot_release::a_window_that_scrolls_away_from_its_blocks_and_back_holds_the_same_slots",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "world.streaming.a-departed-block-releases-the-objects-in-its-interior-cells",
        says: "When a landblock leaves the window, the objects standing in its interior rooms \
               leave visibility and are queued for destruction just as its outdoor objects are, \
               and anything they hold goes with them.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O625-STREAMING"),
        station: "dereth-client::gpu::world::landblock_interior_release::interior_cell_objects::a_departed_block_releases_the_objects_in_its_interior_cells",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "world.streaming.a-departed-block-returns-its-merged-terrain-surfaces",
        says: "Blocks that leave the window give back their merged terrain surfaces, so a window \
               that scrolls away and comes home holds the same number of graphics-device slots as \
               when it first loaded.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O609-STREAMING"),
        station: "dereth-client::gpu::world::streaming_slot_release::terrain_surfaces::a_window_that_scrolls_away_from_its_blocks_and_back_holds_the_same_device_slots",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "world.streaming.a-departing-block-releases-its-interiors-and-baked-bodies",
        says: "When a landblock leaves the window, its interior rooms are released together with \
               the collision bodies of the objects baked into them, so the walls and furniture of \
               a departed block no longer stop a body.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O241-STREAMING"),
        station: "dereth-client::gpu::world::landblock_interior_release::the_walls_of_a_released_block_no_longer_stop_a_body",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "world.streaming.the-crossing-frame-draws-body-and-camera-in-the-new-block",
        says: "On the frame the player crosses into a new landblock, the body's parts and the \
               chase camera are already placed in the new block's space, exactly where the \
               following frame puts them, so the crossing draws no jump.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O387-STREAMING"),
        station: "dereth-client::dat::world::landblock_recentre::a_crossing_frame_places_the_body_and_the_camera_in_the_block_it_moved_to",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "world.streaming.the-landblock-window-follows-the-viewer",
        says: "Walking a body out of its starting landblock carries the landblock window with it: \
               the window re-centres on the block the body walked into, and the body stays inside \
               the block the drawn space is anchored to.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-STREAMING-STREAMING"),
        station: "dereth-client::gpu::world::landblock_streaming::a_walking_body_carries_the_window_with_it",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "world.streaming.the-window-recentres-on-the-swept-viewer",
        says: "The landblock window re-centres on the block holding the camera's swept viewpoint, \
               not the block the chase camera's ideal spot falls in, and it does so on the frame \
               of a teleport before any sweep has run.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-VIEWPOINT-SOURCE-STREAMING"),
        station: "dereth-client::dat::world::streaming_viewpoint::the_block_decision_follows_the_swept_viewer",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "world.teleport.a-teleport-into-a-dungeon-draws-what-logging-in-there-draws",
        says: "A server teleport into a dungeon draws exactly what logging in at the same spot \
               draws: the body settles in the same cell, and the landblock window, the rooms \
               walked and the pixels painted are the same for both arrivals.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F30-TELEPORT"),
        station: "dereth-client::gpu::world::teleport_arrival_draw::a_server_teleport_into_a_dungeon_draws_what_logging_in_there_draws",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "world.teleport.a-teleport-releases-the-whole-origin-window-including-late-arrivals",
        says: "A long-distance teleport releases every object of the old landblock window, \
               including objects the server describes at the origin after the teleport, so nothing \
               of the place the player left stays drawn.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P168B-TELEPORT"),
        station: "dereth-client::gpu::world::landblock_object_release::a_teleport_leaves_nothing_of_the_origin_behind_not_even_what_arrives_after_it",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "world.teleport.the-tunnel-hides-the-world-and-leaves-the-hud",
        says: "While the portal tunnel is up the world is taken off the screen and the portal view \
               is drawn where it was, while every opaque part of the interface stays exactly as it \
               was.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-TELEPORT-TELEPORT"),
        station: "dereth-client::gpu::world::portal_tunnel_view::the_tunnel_takes_the_world_off_the_screen_and_leaves_the_opaque_hud_alone",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "world.viewpoint.the-cell-decision-reads-last-frames-sweep",
        says: "The world update never moves the viewpoint; only the camera sweep does, and only on \
               frames where physics ticked, so each frame's cell and block decisions read the \
               viewpoint the previous frame's sweep left, exactly one frame behind.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O427-VIEWPOINT"),
        station: "dereth-client::gpu::world::viewpoint_sweep_lag::world_scene_update_does_not_move_the_viewpoint_which_is_why_the_lag_is_exactly_one_frame",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "world.viewpoint.the-draw-order-cell-is-read-from-the-viewers-cell-id",
        says: "The cell the client uses to order what it draws is taken from the viewer's own \
               cell, and it steps across every cell line inside a landblock, naming each of the \
               block's sixty-four cells as the viewer passes through it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O425-VIEWPOINT"),
        station: "dereth-client::gpu::world::viewer_cell::the_cell_index_tracks_every_24_m_boundary_inside_a_block",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "world.viewpoint.the-load-time-arms-do-not-recentre",
        says: "Loading the world, and snapping the camera onto the body at load, do not re-centre \
               the landblock window: it stays on the block that was asked for until the first \
               frame update re-centres it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O441-VIEWPOINT"),
        station: "dereth-client::gpu::world::viewer_cell::load_time::neither_load_time_arm_re_centres",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "world.viewpoint.the-viewers-cell-is-its-position-under-the-container-rule",
        says: "The viewer's cell is its own position read through the rule that finds which cell \
               contains a point: walking the camera across a cell line in each of four directions, \
               the cell the client reports agrees with that rule on every frame, even right on the \
               line.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O439-VIEWPOINT"),
        station: "dereth-client::gpu::world::viewer_cell::cell_source::the_viewers_cell_is_its_own_position_read_through_the_container_rule",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "world.window.a-teleport-into-a-dungeon-brings-the-landblock-window-with-it",
        says: "A teleport into a dungeon from another landblock moves the landblock window onto \
               the dungeon's own block, chosen from the cell the player arrives in rather than \
               from the raw position, so the window, the resident rooms and the rooms drawn match \
               what logging in there gives.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F21-WINDOW"),
        station: "dereth-client::gpu::world::dungeon_landblock_window::teleport_into_a_dungeon::a_teleport_into_a_dungeon_brings_the_window_with_it",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "world.window.an-indoor-viewer-that-never-sees-outside-keeps-the-window-on-its-block",
        says: "Inside the training academy, whose room layout reaches beyond the landblock the \
               rooms belong to, the rooms draw and the landblock window stays on the rooms' own \
               block instead of walking off to where the raw position points.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F5-WINDOW"),
        station: "dereth-client::gpu::world::dungeon_landblock_window::the_academy_rooms_draw_and_the_window_stays_on_their_own_block",
        tier: Tier::Gpu,
    },
];
