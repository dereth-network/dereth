//! Missiles and combat stories: how a missile is placed, launched, steered toward its target and
//! set off, and how an attack's story plays its hits and missiles in step with the attacker's
//! animation.
//!
//! A row claims what the September 2013 client does with a missile or a story.

use super::{Behaviour, Evidence, Tier, RETAIL};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "missile.detonate.a-collision-sets-a-missile-off-on-what-it-strikes",
        says: "A missile goes off on the first object it begins touching that it does not pass \
               through; of the objects it passes through, on its target, or (when it goes off on \
               a miss) on any player or creature, never on its source; and on a collision normal \
               with nothing struck.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MISSILE-DETONATE"),
        station: "auberean-physics::cpu::paths::missile::a_collision_sets_a_missile_off_on_what_it_strikes",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "missile.detonate.a-missile-goes-off-within-its-reach-of-its-target",
        says: "A missile that goes off by proximity, with its aim's drawn chance within its own, \
               hits its target object anywhere within its detonation proximity plus its own reach \
               of the target's, without testing the target's shape; it then goes off on the \
               reported hit and fades out.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MISSILE-PROXIMITY"),
        station: "auberean-physics::cpu::contact::solid::a_missile_goes_off_within_its_reach_of_its_target",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "missile.detonate.a-missile-that-goes-off-fades-out-unless-it-sticks",
        says: "A missile that goes off stops flying (its flight effect and streak stop) and fades \
               out over half a second; a sticky missile instead sticks to a moving object it \
               struck that is not itself a missile, and stays put on anything else.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MISSILE-COLLISION"),
        station: "auberean-physics::cpu::paths::missile::a_missile_that_goes_off_fades_out_unless_it_sticks",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "missile.detonate.an-object-noted-says-whether-it-is-a-player-or-a-creature",
        says: "What a missile knows of another object says whether its game object is a player \
               or a creature, as its host answered, for an object that moves and one that does \
               not.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MISSILE-NOTED-KIND"),
        station: "auberean-physics::cpu::paths::missile::an_object_noted_says_whether_it_is_a_player_or_a_creature",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "missile.flight.a-lobbed-missile-comes-down-on-its-target",
        says: "A missile pulled by gravity is launched at its speed on the flattest course that \
               comes down on its target: the flight time searched from the shortest to the \
               longest in five steps, narrowed to a thousandth of a second.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MISSILE-LOB"),
        station: "auberean-physics::cpu::paths::missile::a_lobbed_missile_comes_down_on_its_target",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "missile.flight.a-missile-moves-under-gravity-drag-and-wind-and-expires-once",
        says: "Each step a flying missile moves by its velocity and half its acceleration over \
               the step, the acceleration taking gravity and drag (from the velocity before the \
               wind is added), and takes its new velocity; its lifespan running out is reported \
               once; a missile not flying does not move.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MISSILE-PHYSICS"),
        station: "auberean-physics::cpu::paths::missile::a_missile_moves_under_gravity_drag_and_wind_and_expires_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "missile.guidance.a-missile-keeping-its-distance-circles-and-closes-the-gap",
        says: "A missile keeping its distance flies sideways around its target at up to that \
               distance and toward or away from it by the gap left, moving with the target and \
               climbing by the height it is above it; over the target it flies east.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MISSILE-MAINT-DIST"),
        station: "auberean-physics::cpu::paths::missile::a_missile_keeping_its_distance_circles_and_closes_the_gap",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "missile.guidance.a-missile-leads-a-moving-target",
        says: "A homing missile flies at its wanted speed toward where a moving target will be \
               when they meet, the sooner of the two meeting times.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MISSILE-LEAD"),
        station: "auberean-physics::cpu::paths::missile::a_missile_leads_a_moving_target",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "missile.guidance.a-missile-meets-a-still-target-head-on",
        says: "A homing missile flies straight at a still target at its top speed.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MISSILE-COLLIDE"),
        station: "auberean-physics::cpu::paths::missile::a_missile_meets_a_still_target_head_on",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "missile.guidance.a-missile-that-cannot-catch-its-target-keeps-the-velocity-it-wanted",
        says: "When no meeting time is ahead (a target fleeing faster than the missile flies), \
               the missile keeps the velocity it wanted before, under the speed and height rules: \
               the chase course is worked out and dropped.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MISSILE-NO-INTERCEPT"),
        station: "auberean-physics::cpu::paths::missile::a_missile_that_cannot_catch_its_target_keeps_the_velocity_it_wanted",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "missile.guidance.a-missile-turns-and-brakes-within-its-own-limits",
        says: "A missile accelerates toward the velocity it wants over its reaction time; in its \
               own frame (forward along its flight, up as near the world's up as that allows) the \
               push is held to its greatest acceleration each way, and only a held push is turned \
               back into the world's frame.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MISSILE-ACCEL"),
        station: "auberean-physics::cpu::paths::missile::a_missile_turns_and_brakes_within_its_own_limits",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "missile.guidance.fuel-burns-with-acceleration-and-an-empty-missile-coasts",
        says: "A missile burns fuel at its acceleration times the time passed, down to none; out \
               of fuel it keeps the acceleration it had, and is reported out of fuel once.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MISSILE-FUEL"),
        station: "auberean-physics::cpu::paths::missile::fuel_burns_with_acceleration_and_an_empty_missile_coasts",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "missile.guidance.speed-and-height-rules-shape-every-wanted-velocity",
        says: "A wanted velocity is kept between the missile's least and greatest speeds (one of \
               no length becomes due north at the limit), and a missile following the ground \
               climbs at least to its height over it, or holds that height.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MISSILE-SPEED-HEIGHT"),
        station: "auberean-physics::cpu::paths::missile::speed_and_height_rules_shape_every_wanted_velocity",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "missile.launch.a-missile-aimed-at-a-place-keeps-the-place-it-already-had",
        says: "A missile aimed at a bare place takes the place its aim already held, not the \
               place it is given, so a new missile so aimed has no target.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MISSILE-PLACE-TARGET"),
        station: "auberean-physics::cpu::paths::missile::a_missile_aimed_at_a_place_keeps_the_place_it_already_had",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "missile.launch.a-missile-asked-its-direction-answers-north-and-level",
        says: "Asked which way it would launch, a missile works out its launch and answers north \
               and level whatever it found.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MISSILE-INQUIRE"),
        station: "auberean-physics::cpu::paths::missile::a_missile_asked_its_direction_answers_north_and_level",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "missile.launch.a-missile-is-not-launched-past-its-range-or-at-an-unheard-target",
        says: "A missile is not launched from a place outside the world, before its target has \
               been heard of, or at a target past its range.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MISSILE-RANGE"),
        station: "auberean-physics::cpu::paths::missile::a_missile_is_not_launched_past_its_range_or_at_an_unheard_target",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "missile.launch.a-missile-is-placed-then-launched-once-its-target-is-known",
        says: "A missile aimed at an object finds and tracks it; once placed and fired it is put \
               in the world at its start, launched facing along its path with its streak and \
               flight effect on, and steers toward its target every reaction interval.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MISSILE-UPDATE"),
        station: "auberean-physics::cpu::paths::missile::a_missile_is_placed_then_launched_once_its_target_is_known",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "missile.launch.a-missile-launches-straight-ahead-or-toward-a-target-within-45-degrees",
        says: "A missile launches straight ahead at its starting speed (else its top speed); one \
               that starts toward its target does so when told to always, or when the target is \
               within 45 degrees of where it faces.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MISSILE-LAUNCH"),
        station: "auberean-physics::cpu::paths::missile::a_missile_launches_straight_ahead_or_toward_a_target_within_45_degrees",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "missile.settings.conserving-fuel-and-the-surroundings-top-speeds-share-a-bit",
        says: "Conserving fuel and taking the surroundings' top speeds are marked set by one bit, \
               so setting either sends and copies both; no group uses the bit between following \
               the ground and keeping a distance.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MISSILE-PARAMS"),
        station: "auberean-physics::cpu::paths::missile::conserving_fuel_and_the_surroundings_top_speeds_share_a_bit",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "missile.story.a-hit-shows-its-effects-and-damage",
        says: "A hit plays the effects and behaviors the rules give it on the attacker, the \
               target and the weapon, shows the damage on the target (with no skill for a basic \
               attack), and tells a target that is the player who attacked it when the skill \
               harms.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STORY-HIT"),
        station: "auberean-physics::cpu::paths::missile::a_hit_shows_its_effects_and_damage",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "missile.story.a-story-packs-as-the-protocol-reads-it",
        says: "A combat story's binary form carries its attacker, behavior, client context and \
               skill when set, a basic attack as a bit, and its hooks in animation-hook order, \
               each with its kind and the fields that are set.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STORY-PACK"),
        station: "auberean-physics::cpu::paths::missile::a_story_packs_as_the_protocol_reads_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "missile.story.an-ended-behavior-runs-the-remaining-hooks-and-a-failed-one-drops-them",
        says: "When the attacker's behavior ends the story runs every hook left and is emptied, \
               ending the player's attack; when it fails the story is emptied without them. A \
               story with no attacker behavior runs its hooks at once, and a new story finishes \
               the one playing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STORY-FINISH"),
        station: "auberean-physics::cpu::paths::missile::an_ended_behavior_runs_the_remaining_hooks_and_a_failed_one_drops_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "missile.story.hooks-run-in-step-with-the-attack-animation",
        says: "A combat story plays the attacker's behavior under a new context (never 0) and \
               runs the hooks waiting for no animation hook; each story animation hook of the \
               attack lets the next hooks run, in the order of the animation hooks they wait for, \
               and as added among equals.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STORY-HOOKS"),
        station: "auberean-physics::cpu::paths::missile::hooks_run_in_step_with_the_attack_animation",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "missile.story.missiles-are-made-in-hand-fired-in-order-and-the-unfired-removed",
        says: "A make-missiles hook makes the missile of every fire hook that the rules hold \
               before firing and puts it in the attacker's hand, fading a thrown weapon out; each \
               fire hook fires the first made, or makes one then; a story ended early removes the \
               unfired missiles and fades the weapon back in over half a second.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STORY-MISSILES"),
        station: "auberean-physics::cpu::paths::missile::missiles_are_made_in_hand_fired_in_order_and_the_unfired_removed",
        tier: Tier::Cpu,
    },
];
