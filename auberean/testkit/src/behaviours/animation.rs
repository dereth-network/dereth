//! Animation: how bodies are put together, posed, animated and skinned, and how the recorded
//! sessions' descriptions of how objects look resolve.
//!
//! A row claims what the September 2013 client does when it animates a body.

use super::{Behaviour, Evidence, Tier, RETAIL};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "animation.behaviors.a-new-behavior-takes-its-layer",
        says: "Playing a behavior on a layer another behavior holds stops the holder (whoever \
               waits on it hears it was stopped) and takes the layer; a behavior already playing \
               is refused.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ANIM-LAYERS"),
        station: "auberean-animation::cpu::trackers::a_new_behavior_takes_its_layer",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "animation.behaviors.an-action-ramps-in-plays-once-and-ends",
        says: "An action ramps its weight in over its ramp time, plays its cycle once, ramps out \
               over the last ramp time of its cycle and ends, telling whoever waits on it; while \
               a behavior the server asked for plays the idles stop, and they start again after.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ANIM-ACTION"),
        station: "auberean-animation::cpu::trackers::an_action_ramps_in_plays_once_and_ends",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "animation.behaviors.what-an-object-does-of-itself-is-not-the-networks",
        says: "A behavior an object does of itself is not among those told to the network, so a \
               description that does not list it leaves it playing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ANIM-AUTONOMOUS-UNSENT"),
        station: "auberean-animation::cpu::trackers::a_behavior_an_object_does_of_itself_is_not_among_those_told_to_the_network",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "animation.body.a-changed-look-keeps-what-the-body-plays",
        says: "A body brought up to date with its object's changed look keeps what it plays: each \
               group keeps its part when the body has one of its key, hung from the same group, \
               with the same setup and animation map (a part made anew takes those hung from it \
               with it), takes what the new look does to its parts and hangs from its connection \
               point again; parts of groups the look no longer has go; a new behavior table starts \
               its trackers over. A wounded human male walks on in its wounded look.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ANIM-COMPARE-AND-MODIFY"),
        station: "auberean-animation::dat::bodies::a_wounded_human_male_moves_on_in_its_new_look",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "animation.body.a-new-behavior-table-leaves-an-action-lock-set",
        says: "A behavior table the look changes to while a behavior locks the player's actions drops \
               that behavior and asks nothing of the lock; the next locking behavior's end lets go.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ACTION-LOCK-NEW-TABLE"),
        station: "auberean-animation::dat::bodies::a_new_behavior_table_leaves_an_action_lock_set",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "animation.body.a-slider-just-past-still-moves-at-once",
        says: "A locomotion slider standing just past still (under 0.05 m/s) toward the side the \
               body moves is moved there in 0.05 s, except that forward a change of more than \
               0.75 m/s takes 0.3 s; from still, or across a change of more than 0.75 m/s, in \
               0.3 s; otherwise in 0.1 s.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ANIM-SLIDER-RATE"),
        station: "auberean-animation::cpu::motion::a_slider_just_past_still_moves_at_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "animation.body.a-smaller-bodys-legs-cycle-faster-at-one-speed",
        says: "The forward and sideways locomotion sliders take the body's speed along each over \
               its size along that axis (its look's scale times its own): a body drawn smaller \
               cycles its legs faster at the same speed, covering the ground it covers.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ANIM-SLIDER-SIZE"),
        station: "auberean-animation::cpu::motion::a_smaller_bodys_legs_cycle_faster_at_one_speed",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "animation.body.its-wind-slider-follows-the-wind-over-a-second",
        says: "Each body's wind slider follows how hard the wind blows, reaching a new strength over a \n               second.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ANIM-WIND-SLIDER"),
        station: "auberean-animation::cpu::motion::its_wind_slider_follows_the_wind_over_a_second",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "animation.body.the-human-male-puts-together-and-moves",
        says: "The player's human male is put together from its visual description: a 41-bone body \
               with its head hung from the body's head connection; its behavior table plays moving \
               on the ground in its first mode, and its moving animation's cycle shortens from a \
               walk to a run as its forward speed rises.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ANIM-BODY"),
        station: "auberean-animation::dat::bodies::the_human_male_puts_together_and_moves",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "animation.clip.a-clip-plays-at-the-fraction-of-its-cycle",
        says: "A clip is played at a phase in cycles: the fraction of the phase times the clip's \
               period is the time in seconds, and its bones add into the pose by their weight.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ANIM-CLIP-PHASE"),
        station: "auberean-animation::cpu::clips::a_clip_plays_at_the_fraction_of_its_cycle",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "animation.clip.keys-interpolate-and-the-last-holds",
        says: "A bone's track takes its last key at or before the time and moves toward the next by \
               the fraction of the way, rotations along the short arc; past its last key it holds \
               it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ANIM-CLIP-KEYS"),
        station: "auberean-animation::cpu::clips::keys_interpolate_and_the_last_holds",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "animation.corpus.every-recorded-visual-description-resolves",
        says: "Every object the 2017 sessions create names records that exist and decode; every mode \
               set on an object is one its behavior table has, and so is every behavior played on \
               one but two; every effect played is in an effect table, its own or another's, but \
               six.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ANIM-CORPUS"),
        station: "auberean-animation::corpus::descriptions::every_recorded_visual_description_resolves",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "animation.corpus.recorded-behaviors-and-modes-play",
        says: "Every behavior the 2017 sessions played on an object they created plays on the body \
               its visual description makes, but for those its behavior table or the character \
               template lacks, those already playing and those whose animation is missing or of no \
               length; every mode set on one has a way there along its table's mode graph.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ANIM-CORPUS-PLAY"),
        station: "auberean-animation::corpus::behaviors::recorded_behaviors_and_modes_play",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "animation.cube.a-blend-weighs-its-children-over-the-slider-grid",
        says: "A blend weighs its children multilinearly over its sliders' tick grid, the first \
               slider varying fastest, each slider held at the ends of its ticks.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ANIM-CUBE"),
        station: "auberean-animation::cpu::clips::a_blend_weighs_its_children_over_the_slider_grid",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "animation.cube.a-blends-hooks-merge-and-fall-due-at-their-weighed-time",
        says: "A blend merges its children's game hooks one per kind of data, their times summed \
               by the children's weights, and a merged hook falls due only once the blend's time \
               passes it; a clip's hook falls due at its own time.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ANIM-CUBE-HOOKS"),
        station: "auberean-animation::cpu::clips::a_blends_hooks_merge_and_fall_due_at_their_weighed_time",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "animation.modes.a-body-changes-mode-along-its-table",
        says: "A body starts in its table's initial mode playing the table's initial behaviors; \
               asked for another mode, it walks the table's mode graph a step at a time, blending \
               from one mode's behaviors to the next over each step's ramp time, and whoever \
               asked hears when it is there.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ANIM-MODES"),
        station: "auberean-animation::dat::bodies::the_human_male_changes_mode_along_its_table",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "animation.modes.the-mode-path-steps-to-a-neighbor",
        says: "The way from one mode to another, found depth first along the table's mode graph \
               within the two elements, starts with a mode the first leads to directly.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ANIM-MODE-PATH"),
        station: "auberean-animation::dat::records::every_mode_path_steps_to_a_neighbor",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "animation.random.bodies-pick-their-variants-from-the-clients-one-generator",
        says: "A random node rolls its next variant and its wait from the client's one generator, \
               which every body and everything else in the client draws from, so two bodies playing \
               the same idle do not pick their variants in step.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ANIM-RANDOM-SHARED"),
        station: "auberean-animation::cpu::random::bodies_pick_their_variants_from_the_clients_one_generator",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "animation.records.each-animation-kind-answers-its-number",
        says: "Each kind of animation answers the number its records give it: keyframes 40000001, \
               a blend 40000002, a body's 40000003, a look 40000005, an axis cube 40000006, a \
               random node 40000007, a switch 40000008, the angle limits 4000000A (hexadecimal).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ANIM-KINDS"),
        station: "auberean-animation::cpu::records::each_animation_kind_answers_its_number",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "animation.records.every-animation-decodes-and-plays",
        says: "Every animation record of the end-of-retail set decodes, every keyed track starts \
               at time 0, and every animation has a period and plays into a pose of numbers.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ANIM-RECORDS"),
        station: "auberean-animation::dat::records::every_animation_decodes_and_plays",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "animation.records.the-character-template-is-found-by-enumeration",
        says: "The character template is the one record its enumeration names, and the only one of \
               its kind.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ANIM-TEMPLATE"),
        station: "auberean-animation::dat::records::the_character_template_is_found_by_enumeration",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "animation.scripts.a-script-plays-once-over-its-period",
        says: "An effect script played on its own runs from its start to its end over its period, \
               starting its effects and firing its pushes as it passes them; at its end it stops \
               the effects it started and is then dropped; one not in the data is refused.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ANIM-SCRIPTS"),
        station: "auberean-animation::cpu::scripts::a_script_plays_once_over_its_period",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "animation.setups.a-place-not-read-from-a-setup-is-on-no-bone",
        says: "A holding location or connection point not yet read from a setup is of kind 0 and \
               on no bone.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ANIM-EMPTY-LOCATION"),
        station: "auberean-animation::cpu::records::a_place_not_read_from_a_setup_is_on_no_bone",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "animation.setups.every-setup-packs-back-to-itself",
        says: "A setup packs into its stored form: the size its packed size gives, its poses and \
               places in the order of the tables that hold them, only the two flags the client \
               keeps; packed and read again it is the same setup.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ANIM-SETUP-PACK"),
        station: "auberean-animation::dat::setups::every_setup_packs_back_to_itself",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "animation.skin.a-body-at-rest-skins-to-its-model",
        says: "Posed at rest, every skinned vertex of the human male's body stays where it was \
               modeled: a bone's skinning transform undoes its world rest transform.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ANIM-REST"),
        station: "auberean-animation::dat::bodies::a_body_at_rest_skins_to_its_model",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "animation.skin.a-bones-scale-reaches-the-skin-under-it",
        says: "A bone's scale reaches the skinning transforms of the bones under it (each is its \
               own scaled transform followed by its parent's) but not their bone transforms, \
               which take no scale.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ANIM-DEFORM"),
        station: "auberean-animation::cpu::poses::a_bones_scale_reaches_the_skin_under_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "animation.skin.weights-blend-bone-transforms",
        says: "A skinned vertex moves by the weighted sum of its bones' skinning transforms.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ANIM-SKIN"),
        station: "auberean-animation::cpu::clips::weights_blend_bone_transforms",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "animation.turns.the-lean-follows-the-slope-within-its-limits",
        says: "The slope lean steers its two sliders toward the slope under the body, held within \
               its limits (toward level in the air or sliding down; not at all when the body is \
               not placed), at once when within 0.15 and over a quarter second otherwise, and \
               turns its bones by the sliders' values before the steering times its shares.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ANIM-LEAN"),
        station: "auberean-animation::cpu::turns::the_lean_follows_the_slope_within_its_limits",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "animation.turns.the-look-turns-the-head-fully-and-the-neck-partly",
        says: "The look turns the head bones by 0.35 of the look sliders' direction and the neck \
               bones by a smaller share, the direction cut back to unit length when longer and \
               each bone's rotation made unit before it turns.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ANIM-LOOK"),
        station: "auberean-animation::cpu::turns::the_look_turns_the_head_fully_and_the_neck_partly",
        tier: Tier::Cpu,
    },
];
