//! Objects -- the stream, and what the panels read off it.
//!
//! What a create, a set-state, a position stamp and a dangling reference do to the world the
//! panels poll, and the diagnostic a message nothing received prints about itself.
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
        id: "diagnostics.unreceived-message.names-itself-once-with-its-site",
        says: "A message this client has no receiver for is reported once, naming the message, the part \
               of the client that refused it and how many have arrived there; ten arrivals cost one \
               report and ten counts, a second unknown message gets its own report, and a message that \
               does have a receiver is never reported at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P2-1"),
        station: "dereth-testkit::cpu::objects::scenario_an_unreceived_message_names_itself_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "object.create.a-second-create-carries-the-new-word-to-the-one-table",
        says: "A second create for an object the client already has is a change to that object and not a new \
               one: the description it carries replaces the physics word the first create installed, and the \
               visible-object sweep follows it in the same breath.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O613-MERGE"),
        station: "dereth-testkit::cpu::objects::scenario_a_second_create_carries_the_new_word",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "object.set-state.a-body-the-shard-hides-stops-being-a-target-and-comes-back-in-place",
        says: "A body the shard hides after it was created keeps its place in the world and is no longer \
               something a swing can reach; when the shard unhides it, the same body is a target again \
               without moving, and an unchanged word is no change at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O252-HIDDEN"),
        station: "dereth-testkit::dat::objects::scenario_a_hidden_body_stops_being_a_target",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "object.set-state.a-change-reaches-a-live-doors-collision",
        says: "A physics-state change for a door that already exists reaches its collision body: the \
               body takes exactly the word on the wire, and a closed door that stops a walk lets the \
               same walk into it once the change that opens it has arrived.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O252-DOOR"),
        station: "dereth-client::dat::objects::door_opens_ethereal::set_state::a_recorded_set_state_word_changes_a_live_doors_collision_answer",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "object.set-state.every-recorded-change-reaches-the-body-it-names",
        says: "Every physics-state change the recorded shards sent about an object that has a body reaches \
               that body with exactly the word that was on the wire, on the frame it arrived, and the physics \
               side counts only the ones that really moved the word.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O252"),
        station: "dereth-testkit::dat::objects::scenario_every_recorded_state_change_reaches_its_body",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "object.set-state.the-stamp-gate-refuses-a-change-that-is-not-newer",
        says: "A physics-state change is applied only when its stamp is newer than the one the object already \
               carries, where the create's own stamp is the starting value, an equal stamp is not newer, and \
               a stamp half a period or more ahead is read as a wrap and refused rather than taken.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O252-GATE"),
        station: "dereth-testkit::cpu::objects::scenario_a_state_change_that_is_not_newer_is_refused",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "object.set-state.visible-list-follows",
        says: "When the shard changes an object's physics state after it has been created, the \
               client's visible-object list follows: an object made static drops out of the list \
               the target cycle and the auto-target scan read, and an object made non-static \
               comes back into it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O613"),
        station: "dereth-testkit::cpu::objects::scenario_set_state_moves_the_visible_list",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "objects.animation.the-ethereal-hook-reaches-physics",
        says: "Every shipped animation table whose open motion makes an object passable has a \
               close motion that makes it solid again, and playing them raises that change for the \
               object's body, so nothing that opens stays passable for ever.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O112-ANIMATION"),
        station: "dereth-client::dat::objects::door_opens_ethereal::the_retail_on_and_off_motions_raise_a_matched_pair_of_ethereal_events",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "objects.appearance.a-part-swap-naming-an-absent-object-keeps-the-limb",
        says: "When an appearance change asks to replace a body part with a model the game data \
               does not hold, the part keeps its old model exactly, with its textures and detail \
               levels untouched, while a replacement the data does hold is applied.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O97-APPEARANCE"),
        station: "dereth-animation::dat::parts::part_swap_absent_gfxobj::a_swap_naming_an_absent_gfxobj_keeps_the_limb_exactly_as_it_was",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "objects.appearance.a-subpalette-changes-only-the-ranges-it-names",
        says: "A palette change in an appearance description replaces exactly the colour ranges it \
               names, entry for entry, leaves every other entry of the base palette alone, and the \
               dyed body texture really differs from the plain one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-APPEARANCE-APPEARANCE-SUBPALETTE"),
        station: "dereth-client::gpu::objects::appearance_objdesc::a_wire_subpalette_changes_exactly_the_ranges_it_names",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "objects.appearance.a-swap-naming-an-absent-gfxobj-keeps-the-previous-part",
        says: "When a server object's appearance swaps a part for a graphics object the data files \
               do not hold, the object keeps the part its setup gives it; a swap naming a shipped \
               graphics object uses the replacement.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O184-APPEARANCE"),
        station: "dereth-client::gpu::objects::part_swap_gfxobj::a_server_objects_swap_naming_an_absent_gfxobj_keeps_the_setups_part",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "objects.appearance.a-swapped-part-carries-the-new-objects-degrade-record",
        says: "A body part replaced by an appearance change takes on the new model's own levels of \
               detail, so the distance at which it simplifies is the new model's and not the old \
               one's.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O97-APPEARANCE-SWAPPED"),
        station: "dereth-animation::dat::parts::part_swap_absent_gfxobj::a_swapped_part_carries_the_new_objects_degrade_record",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "objects.appearance.a-wire-objdesc-dresses-the-body-it-names",
        says: "Every creature and player the shard creates is dressed as its appearance \
               description says: part swaps reach the drawn meshes, palette changes reach the \
               textures, no description fails to apply, and differently dressed objects no longer \
               share geometry.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-APPEARANCE-APPEARANCE"),
        station: "dereth-client::gpu::objects::appearance_objdesc::the_captures_objects_wear_what_the_server_sent",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "objects.appearance.marker-parts-draw-only-on-the-local-player",
        says: "Marker parts are drawn only on the local player: on any other object they fall to \
               an empty level of detail at every distance, while on the player they always draw as \
               the part itself.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O99-APPEARANCE"),
        station: "dereth-client::gpu::objects::marker_parts::the_client_draws_a_marker_on_the_local_player_and_refuses_it_on_everything_else",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "objects.appearance.the-body-wears-the-part-swaps-the-server-sent",
        says: "The player's drawn body carries exactly the part swaps the shard sent: each named \
               part uses the graphics object the shard chose, every other part stays the setup's \
               own, and the dressed body looks different from the undressed one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O74-APPEARANCE"),
        station: "dereth-client::gpu::objects::part_swaps::the_drawn_body_carries_exactly_the_captures_part_swaps",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "objects.appearance.the-players-body-is-built-from-the-setup-the-server-named",
        says: "In every recorded session the player's body is built from the setup the shard named \
               and dressed as it described, down to the geometry and textures actually drawn, with \
               no palette left without a base.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O98-APPEARANCE"),
        station: "dereth-client::gpu::objects::player_body_setup::every_captures_body_is_built_from_the_setup_the_server_named",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "objects.carried.a-carried-object-has-no-body",
        says: "An object with no place in the world, such as one being carried, has no body to \
               bump into; once the shard gives it a position it gains one, and it is not moved \
               again by an unchanged update.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-SOLID-CARRIED"),
        station: "dereth-client::dat::world::collision_obstacles::a_carried_object_has_no_body_and_a_dropped_one_gains_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "objects.collision.a-cylsphere-only-object-stops-a-body",
        says: "An object in the training dungeon whose only collision shape is an upright cylinder \
               keeps a walking body outside that cylinder, where the same object without it would \
               let the body walk straight through.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-MESH-COLLISION-COLLISION-CYLSPHERE"),
        station: "dereth-client::dat::objects::mesh_collision::a_cylsphere_only_dungeon_object_stops_a_body_that_used_to_walk_through_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "objects.collision.a-recorded-object-stops-a-body",
        says: "An object the shard placed in a recorded session is solid: a body walking at it is \
               kept clear of it, where without the object the same walk goes right up to where it \
               stands.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-SOLID-COLLISION-RECORDED"),
        station: "dereth-client::dat::world::collision_obstacles::a_corpus_object_stops_a_body_that_would_otherwise_walk_through_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "objects.collision.an-object-with-only-a-physics-bsp-stops-a-body",
        says: "An object in the training dungeon whose only collision shape is its detailed mesh \
               stops a body walking into it outside that mesh, where the same object without its \
               mesh shape would let the body walk straight through.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-MESH-COLLISION-COLLISION"),
        station: "dereth-client::dat::objects::mesh_collision::a_bsp_only_dungeon_object_stops_a_body_that_used_to_walk_through_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "objects.corpse.a-decayed-corpse-is-culled-at-the-lifecycle-deadline",
        says: "If the player leaves a corpse's area and stays away past the client's 25-second \
               deadline, the corpse is removed along with its opened mark and the selection, and \
               on return nothing of it is drawn.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P190B-CORPSE"),
        station: "dereth-client::gpu::objects::corpse_decay::leaving_the_window_past_the_deadline_culls_the_corpse_and_the_return_is_clean",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "objects.corpus.every-recorded-create-description-and-quality-update-lands",
        says: "Every property update in the recorded sessions is applied to the object it names \
               and none is refused as out of order; the few that arrive a moment before their \
               object's own creation always name an object the same session goes on to create.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-CORPUS-REPLAY-CORPUS"),
        station: "dereth-client-model::cpu::objects::corpus_replay::the_quality_updates_from_the_corpus_land_and_are_not_spuriously_rejected",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "objects.create.a-create-carrying-a-velocity-moves",
        says: "An object created with a velocity, such as a spell bolt, flies: its position \
               changes across simulated time without needing any animation to move it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F27-CREATE"),
        station: "dereth-client::gpu::objects::created_velocity::a_projectile_created_with_a_velocity_travels",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "objects.create.a-create-is-active-from-birth",
        says: "An object created with gravity and no declared velocity is active from the moment \
               it is created, before the first physics step.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F50-CREATE"),
        station: "dereth-client::gpu::objects::created_velocity::a_gravity_carrying_create_is_active_from_birth",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "objects.create.every-create-reaches-the-object-tables",
        says: "Every object create in each recorded session reaches the client's object tables as \
               a new object, a merge or a refused stale copy; no drawn object outlives its record, \
               and every delete or rebuild message removes one object.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-POPULATED-CREATE"),
        station: "dereth-client::gpu::objects::populated_world::every_create_the_server_sent_reaches_the_object_tables",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "objects.create.the-shards-state-word-wins-over-the-geometry-scan",
        says: "The physics word a create carries is the word the body ends up with, in both directions: it \
               survives the client's own scan of the object's parts, it is installed whole including the bits \
               nothing reacts to, and a later change from the shard installs its word the same way.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O113"),
        station: "dereth-testkit::dat::objects::scenario_the_descriptors_word_wins_over_the_scan",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "objects.dangling-reference.is-asked-about-once-and-then-dropped",
        says: "A reference to an object the shard never sent is asked about once twenty seconds after \
               it appears and not a moment before, and the placeholder is thrown away five seconds \
               after that, so nothing is asked about it again. The twenty and the twenty-five are both \
               measured from the moment the placeholder was made and both are strict.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O207-SWEEP"),
        station: "dereth-testkit::cpu::objects::scenario_a_dangling_reference_is_asked_about_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "objects.death.a-corpse-is-never-a-radar-blip",
        says: "A corpse never appears on the radar, whether this player opened it or not, because \
               the shard sends no radar setting for it; opening a corpse changes nothing on the \
               radar.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1121-DEATH-CORPSE"),
        station: "dereth-client::gpu::objects::death_sequence::neither_corpse_is_ever_a_radar_blip",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "objects.death.a-killed-creature-falls-over-once-across-itself-and-its-corpse",
        says: "A creature killed in view plays its death fall exactly once: the corpse that \
               replaces it is created already lying down in the same animation rather than falling \
               a second time.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P167-DEATH"),
        station: "dereth-client::gpu::objects::death_animation::a_killed_creature_falls_over_exactly_once_across_itself_and_its_corpse",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "objects.death.a-remote-death-teleport-takes-the-shimmer-off-the-old-site",
        says: "When another player dies and is teleported away, his shimmer leaves this client's \
               screen with him instead of lingering at the death site, while his corpse stays \
               drawn where he fell.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1106-DEATH"),
        station: "dereth-client::gpu::objects::death_teleport_effects::the_death_teleport_takes_the_victims_shimmer_off_the_survivors_screen",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "objects.death.the-recorded-death-plays-from-the-fight-to-vitae",
        says: "A recorded death plays in order: health falls to zero, the death messages appear, \
               the vitae penalty deepens and skill numbers drop without changing colour, temporary \
               enchantments are purged while permanent ones stay, the corpse appears and opens, \
               and vitae wears off.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1121-DEATH"),
        station: "dereth-client::gpu::objects::death_sequence::the_death_sequence_from_the_fight_to_the_vitae_wearing_off",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "objects.deletion.the-shards-remove-detaches-the-item-and-defers-the-rest",
        says: "When the shard removes an object from a pack, the item leaves that pack at once, the pending \
               request and the selection it was holding are released, and the object itself is only scheduled \
               to go rather than deleted on the spot; an object the client never heard of schedules nothing \
               and leaves another item's pending request alone.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-OBJECT-REMOVAL"),
        station: "dereth-testkit::cpu::objects::scenario_a_removal_detaches_the_item_and_defers_the_deletion",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "objects.description.a-new-animation-table-alone-reaches-the-body",
        says: "A description that changes only which animation table a body uses reaches the body: the new \
               table is installed and the body's parts are not rebuilt, offering the same table again does \
               nothing at all, and a table the data files do not hold is refused without disturbing the body.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O599"),
        station: "dereth-testkit::dat::objects::scenario_a_new_animation_table_alone_reaches_the_body",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "objects.descriptor-recovery.a-losing-duplicate-create-asks-for-the-description-once",
        says: "When a second create for an item the client holds arrives in the losing order, the \
               client puts the item back where it was, asks the shard once for the item's full \
               description, and sends that request on the next packet-processing pass.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P140-DESCRIPTOR-RECOVERY"),
        station: "dereth-client::gpu::objects::descriptor_recovery_request::duplicate_drop_create_queues_descriptor_recovery_at_its_message_boundary",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "objects.door.a-closing-door-will-not-close-on-a-body",
        says: "A door told to close while a body stands in its doorway stays open, whichever side \
               of the doorway the body is on, and keeps trying; once the body has walked out it \
               closes without anything else asking.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O112-DOOR-CLOSING"),
        station: "dereth-client::dat::objects::door_opens_ethereal::a_door_refuses_to_close_on_a_body_on_either_side_of_its_cell_boundary_and_closes_once_it_leaves",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "objects.door.an-opened-door-stops-blocking",
        says: "A real door that is opened lets a body walk through it while it still stands and is \
               drawn; closed again, it stops the body short of it once more.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O112-DOOR"),
        station: "dereth-client::dat::objects::door_opens_ethereal::an_opened_retail_door_stops_blocking_and_a_closing_one_will_not_close_on_a_body",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "objects.door.both-faces-of-an-interior-door-stop-a-body-at-the-same-distance",
        says: "A closed door standing in the training academy's own rooms stops a body walking \
               into it at the same distance from either face, within one walking step, and the \
               body never ends up inside the door.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-FABLE-DOOR-COLLISION-DOOR"),
        station: "dereth-client::dat::objects::door_collision_symmetry::an_interior_doorway_presents_the_same_surface_to_both_faces",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "objects.effects.a-level-up-on-the-player-plays-on-his-own-body",
        says: "A level-up effect the shard plays on the player makes the shipped level-up \
               particles appear on the player's own body.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F23-EFFECTS"),
        station: "dereth-client::gpu::objects::player_body_effects::a_level_up_on_the_players_own_object_reaches_the_bodys_emitters",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "objects.force-objdesc.leaves-on-the-control-queue",
        says: "When the client is left holding a reference to an object the shard never sent it, \
               it asks for that object's description again, and the ask really leaves the client \
               as an unordered blob on the control queue rather than being logged and dropped.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O207"),
        station: "dereth-testkit::cpu::objects::scenario_force_objdesc_leaves_on_the_control_queue",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "objects.held-frame.an-out-of-range-part-uses-the-holders-own-frame",
        says: "When a holding place names a part at or beyond the end of the holder's part array, the held \
               object's frame is composed from the holder's own frame; a part that is present still uses \
               that part's frame.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O61-FRAME-FALLBACK"),
        station: "dereth-testkit::dat::objects::scenario_an_out_of_range_holding_part_uses_the_holders_frame",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "objects.held-frame.is-the-holders-part-frame-composed-with-the-holding-location",
        says: "Something held in a character's right hand is placed at the hand part's current \
               position and turn with the human body's shipped right-hand holding offset applied \
               on top, so it moves with the hand as it animates.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-HELD-ITEMS-HELD-FRAME"),
        station: "dereth-client::dat::objects::held_item_frame::the_held_frame_is_the_holders_part_frame_composed_with_the_holding_location",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "objects.held-object.has-no-body-and-leaves-the-cell-with-its-holder",
        says: "An object something else is holding has no body of its own in the world: nothing steps it and \
               nothing collides with it. When the room it is in is released it is not released in its own \
               right -- its holder is, and the held object goes with it, still attached.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P0-11-CONSUMERS"),
        station: "dereth-testkit::dat::objects::scenario_a_held_object_has_no_body_of_its_own",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "objects.held-object.is-a-pick-candidate-while-a-contained-object-is-not",
        says: "An object drawn in a holder's hand is offered to the player's pick sweep at the frame where \
               it was drawn; an object inside a container has no drawn frame and is not offered to the same \
               sweep.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O61-HELD-PICK"),
        station: "dereth-testkit::dat::objects::scenario_a_held_object_is_pickable_and_a_contained_object_is_not",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "objects.held.a-wielded-weapon-is-drawn-in-the-holders-hand",
        says: "A creature the shard says is wielding a weapon is drawn holding it: the weapon is \
               placed at the holder's hand, follows that hand, and is posed in its held placement \
               rather than its resting one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-HELD-DRAW-HELD"),
        station: "dereth-client::gpu::objects::held_objects_draw::a_wielded_weapon_is_drawn_in_the_holders_hand",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "objects.hidden.the-body-is-drawn-again-after-the-unhide-ramp",
        says: "A player the shard hides fades until no part of his body is drawn, and when the \
               shard clears the hidden state he fades back in over three quarters of a second \
               until every part is drawn again, fully opaque.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F40-HIDDEN"),
        station: "dereth-client::gpu::objects::hidden_state::the_body_is_drawn_again_after_the_server_clears_hidden",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "objects.hidden.the-hide-shimmer-lasts-until-the-shard-unhides",
        says: "The shimmer played when the shard hides a player never ends by itself; only the \
               unhide effect stops it, stopping exactly the emitters the hide started, whereas the \
               arrival effect is finite and ends on its own.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F37-HIDDEN"),
        station: "dereth-client::gpu::objects::hidden_state::the_hide_shimmer_is_immortal_and_the_unhide_is_the_only_thing_that_stops_it",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "objects.lifecycle.a-parked-delete-never-removes-a-newer-instance",
        says: "A removal still waiting to be applied to an older copy of an object never removes \
               the newer copy the shard created in the meantime: the newer instance stays, with \
               its own state.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-FRAME-PHASES-LIFECYCLE-PARKED"),
        station: "dereth-client::gpu::objects::frame_phase_ordering::mixed_ui_remove_then_newer_create_must_remove_old_instance_only",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "objects.lifecycle.a-remote-that-leaves-and-returns-rematerializes-with-its-held-child",
        says: "A player removed after leaving view who is created afresh on his return reappears \
               with his wand attached to his hand; he stays undrawn while hidden, and both he and \
               the wand are drawn once the shard makes him visible.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P195-LIFECYCLE"),
        station: "dereth-client::gpu::objects::remote_leave_return::a_culled_remote_returning_as_a_fresh_create_materializes_body_and_wand",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "objects.lifecycle.an-equal-instance-return-reattaches-the-retained-body-and-held-item",
        says: "A player who leaves and comes back as the same instance with a newer position is \
               reactivated in place: his retained body and the item he holds are drawn again and \
               his unhide completes.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P195-LIFECYCLE-EQUAL"),
        station: "dereth-client::gpu::objects::remote_leave_return::equal_instance_return_create_materializes_remote_body_and_held_item",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "objects.lifecycle.repeated-leave-return-keeps-one-drawn-body-per-object",
        says: "Through ten cycles of another player expiring from view and being sent again, the \
               client keeps exactly one drawn body for him, in the right place, with his wand \
               attached and drawn and no removal deadline left pending.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P195C-LIFECYCLE"),
        station: "dereth-client::gpu::objects::leave_return_churn::ten_asymmetric_expiry_cycles_keep_one_drawn_body_and_its_wand",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "objects.lifecycle.ui-that-arrives-mid-frame-waits-for-the-next-frame-entry",
        says: "An inventory change received from the network during a frame is applied at the \
               start of the next frame, not the one it arrived in, and it is stamped with the time \
               as it stood before that frame's clock advanced.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-FRAME-PHASES-LIFECYCLE"),
        station: "dereth-client::gpu::objects::frame_phase_ordering::newly_received_ui_waits_for_next_entry_and_uses_pre_timer_clock",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "objects.maintenance.every-deadline-is-strict-and-a-child-that-left-is-spared",
        says: "The client's own housekeeping destroys an object only once its deadline has actually passed, \
               never at the moment it falls due, and it does nothing at all while it is switched off. A \
               container that goes takes the contents it still holds with it, and spares a child that has \
               moved elsewhere or that is out in the world in its own right.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-OBJECT-REMOVAL-SWEEP"),
        station: "dereth-testkit::cpu::objects::scenario_the_housekeeping_deadlines_are_strict",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "objects.nodraw.a-server-marked-object-submits-no-parts",
        says: "An object the shard marks as not to be drawn submits no parts to the frame, while \
               every other object is drawn exactly as before.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O243-NODRAW"),
        station: "dereth-client::gpu::objects::nodraw_state::an_object_the_server_marks_nodraw_is_not_submitted",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "objects.ordered-replies.a-message-for-a-replaced-owner-waits-for-the-new-owner",
        says: "A message the shard stamped for a newer instance of an object is not handed to the \
               older instance still in the world: it waits unread, and once the old object is \
               removed and the new one arrives it is delivered exactly once, with no stale copy \
               left parked behind it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-FRAME-PHASES-ORDERED-REPLIES"),
        station: "dereth-client-net::cpu::net::session_queue_admission::receive_does_not_admit_future_messages_against_the_old_owner",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "objects.ordered-replies.arrive-in-the-order-the-shard-stamped-them",
        says: "Every ordered reply a recorded shard sent reaches the client's own dispatch, in the order the \
               shard stamped them and with none left holding the stream, so a pickup the player made in the \
               recording ends with the item in the pack the shard put it in.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O131-ORDER"),
        station: "dereth-testkit::cpu::objects::scenario_ordered_replies_arrive_in_the_shards_own_order",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "objects.ordered-replies.the-panels-and-the-scroll-take-the-shards-own-values",
        says: "The recorded shards' own replies fill what the player looks at: a container the shard opens \
               fills the grid with the items it named and loses its list entirely when it is closed, an \
               appraisal reaches the assess cache, an allegiance update rebuilds the tree, an enchantment \
               update reaches the registry, and every blow struck reaches the chat scroll.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O135-CONSUMERS"),
        station: "dereth-testkit::cpu::objects::scenario_the_panels_and_the_scroll_take_the_shards_values",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "objects.ordered-replies.wait-for-an-object-that-has-not-arrived-yet",
        says: "A reply about an object the client has not been told about yet waits for that object and is \
               then replayed in its stamped place, whenever in the stream the object turns up. Waiting \
               delivers nothing early, never goes stale, and never starts the clock that skips a message the \
               shard did send.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O131"),
        station: "dereth-testkit::cpu::objects::scenario_an_ordered_reply_waits_for_the_object_it_is_about",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "objects.panels.poll-the-world-rather-than-waiting-for-a-notice",
        says: "The scene and the inventory panels read the world once a frame rather than waiting to be \
               told: a created object is offered to the scene exactly once, a container's contents and \
               side packs are complete the moment the shard's reply lands and empty again when it \
               closes, and a change of the player's own appearance re-offers the body the same way a \
               create does.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-156"),
        station: "dereth-testkit::cpu::objects::scenario_the_panels_poll_rather_than_wait",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "objects.parent.a-failed-link-keeps-the-old-parent",
        says: "An attempt to attach an object to a holder that cannot take it leaves the object on \
               its old holder; a later valid attachment moves it and cancels its pending removal \
               without rescuing any other object.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-NULL-ATTACHMENTS-PARENT"),
        station: "dereth-client::gpu::objects::parked_attachments::failed_add_child_preserves_old_link_then_valid_reparent_rescues_only_that_null",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "objects.parent.a-holder-holds-only-at-a-place-its-own-body-has",
        says: "An object is attached to the thing holding it only where the holder's own body has somewhere \
               to put it: a place its parts do not carry is refused, and so is every attachment at all when \
               the client has no data files open to answer the question. A holder that really does hold a \
               child keeps that child's placeholder alive past the deadline it would otherwise have died at.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P0-11"),
        station: "dereth-testkit::dat::objects::scenario_a_holder_holds_only_where_its_body_has_a_place",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "objects.parent.a-new-parent-event-attaches-the-child-in-wire-order",
        says: "A new parent event attaches the second object named on the wire to the first at the stated \
               holding place and pose, removes the child from the world, and leaves the holder in place; \
               reversing the two object ids reverses which object becomes the child.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O61-PARENT-WIRE"),
        station: "dereth-testkit::dat::objects::scenario_a_new_parent_event_attaches_the_child_in_wire_order",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "objects.parent.an-old-or-incomplete-parent-event-changes-nothing",
        says: "A parent event at an equal or older position stamp does not change the child's holder or pose, \
               and an event naming a child or holder the client does not have is counted on the missing side \
               without becoming an applied parent event.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O61-PARENT-GATES"),
        station: "dereth-testkit::dat::objects::scenario_old_and_incomplete_parent_events_change_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "objects.parent.recorded-parent-events-attach-objects-to-their-holders",
        says: "Replaying the recorded sessions, the shard's messages that put one object in \
               another's hand or on its body reach the client's objects, and while the player is \
               in the world objects are shown attached to their holders at the places named.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-HELD-ITEMS-PARENT"),
        station: "dereth-client::dat::objects::held_item_frame::the_corpus_parent_events_all_land",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "objects.pick.a-sweep-uses-the-placement-the-shard-named",
        says: "The pick sweep uses the placement the shard named for an object's parts: aimed through the \
               named pose it hits, while the same object at Resting misses, even when both queries share the \
               same placement memo.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O61-PICK-PLACEMENT"),
        station: "dereth-testkit::dat::objects::scenario_a_pick_sweep_uses_the_placement_the_shard_named",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "objects.placement.a-server-named-placement-draws-its-own-frames",
        says: "When the shard names a placement a setup carries, every resolved part uses that placement's \
               frame rather than Resting; resolving the same setup without a named placement still uses \
               Resting.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O61-NAMED-PLACEMENT"),
        station: "dereth-testkit::dat::objects::scenario_a_server_named_placement_draws_its_own_frames",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "objects.placement.a-static-is-drawn-at-its-resting-frame",
        says: "Every object placed in the training dungeon is drawn in its resting pose, the pose \
               the client installs for its parts, rather than in its default pose.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-PLACEMENT-POSE-PLACEMENT"),
        station: "dereth-client::dat::objects::resting_placement_pose::the_training_dungeons_setups_are_all_drawn_at_the_frame_the_client_installs",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "objects.placement.an-object-is-drawn-at-its-named-placement",
        says: "An object whose create names a placement is drawn with its parts posed at that \
               placement's frames, visibly different from the pose it takes when no placement is \
               named.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-PLACEMENT-DRAW-PLACEMENT"),
        station: "dereth-client::gpu::objects::placement_draw::an_object_the_server_names_a_placement_for_is_drawn_at_it",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "objects.placement.the-drawn-pose-is-the-collision-pose",
        says: "An object whose parts carry both a resting and a default pose is drawn in the same \
               pose it collides in, so what the player bumps into is where it is drawn.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-PLACEMENT-POSE-PLACEMENT-DRAWN"),
        station: "dereth-client::dat::objects::resting_placement_pose::the_drawn_pose_is_the_pose_collision_uses",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "objects.player.every-player-setup-collides-with-two-spheres",
        says: "Every body a player character can have, whatever heritage, sex, hair or barber \
               choice, collides as two spheres and never as a cylinder or a detailed mesh.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P49F-PLAYER"),
        station: "dereth-client::dat::objects::player_collision_shape::every_reachable_player_setup_uses_the_same_two_sphere_arm_with_or_without_parts",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "objects.position.a-position-report-releases-an-object-from-its-parent",
        says: "A newer position report for a held object removes its parent before placing it in the named \
               cell, so an item dropped into the world no longer follows the holder it left.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O61-POSITION-UNPARENT"),
        station: "dereth-testkit::dat::objects::scenario_a_position_report_releases_an_object_from_its_parent",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "objects.position.an-older-teleport-generation-puts-the-position-stamp-back",
        says: "A position report from an older teleport generation is undone completely: the position stamp \
               goes back to what it was, so the very stamp that was refused can be used again, and nothing \
               about where the object is, what is holding it or how it is posed has moved. A teleport is \
               completed exactly once, and the comparison treats a difference of half the stamp range or more \
               as a wrap.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-POSITION-TIMESTAMPS"),
        station: "dereth-testkit::dat::objects::scenario_an_older_teleport_puts_the_position_stamp_back",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "objects.position.every-reader-sees-a-remote-objects-one-position",
        says: "While a creature walks to a position the shard sent, selection, the range watch, \
               the radar and the drawn body all use where the body actually is part-way along the \
               walk, never the destination the shard named.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1174-POSITION"),
        station: "dereth-client::gpu::movement::remote_single_position::mid_walk_every_reader_is_the_body_and_not_the_wire_target",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "objects.position.teleport-and-force-stamps-follow-the-shards-generations",
        says: "An object's create carries the teleport and forced-position generations the shard is on, \
               and the client echoes them back in its own position reports. A position message that is \
               not newer moves neither, a newer one moves both, the forced-position generation belongs \
               to the player's own body alone, and the announcement that a teleport is coming does not \
               advance either.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O392"),
        station: "dereth-testkit::cpu::objects::scenario_position_generations_follow_the_shard",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "objects.removal.a-delete-retires-model-presence-render-body-and-selection",
        says: "When the shard deletes an object, here a recorded corpse, the client retires it \
               everywhere at once: its record, its collision body, its drawn body, its \
               opened-corpse mark, the selection and the toolbar's name for it; the destroy effect \
               sent before the delete removes nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-OBJECT-REMOVAL-REMOVAL"),
        station: "dereth-client::gpu::objects::removal_lifecycle::recorded_corpse_destroy_then_delete_retires_every_app_owner",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "objects.removal.a-pickup-keeps-selection-and-toolbar-identity",
        says: "Picking an object up is not a deletion: it stays selected, the toolbar still shows \
               its name, and its model and instance are kept for when the server places it \
               somewhere again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-OBJECT-REMOVAL-REMOVAL-PICKUP"),
        station: "dereth-client::gpu::objects::removal_lifecycle::a_pickup_leaves_the_selected_object_and_toolbar_identity_alive",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "objects.replacement.a-newer-instance-wins-and-the-old-ones-traffic-is-dropped",
        says: "A create for an object that is already here replaces it only when it is a newer instance of \
               it; an older one is ignored and an equal one merges. When it is replaced, everything the old \
               one had pending -- its ordering windows, its stamps and the messages parked behind it -- goes \
               with it, so nothing the shard said about the object that left can change the one that arrived.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-OBJECT-REMOVAL-INSTANCE"),
        station: "dereth-testkit::cpu::objects::scenario_a_newer_instance_replaces_and_the_old_ones_traffic_is_dropped",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "objects.unloaded-cell.a-body-is-rescued-by-the-next-position-the-shard-sends",
        says: "An object whose room the client has unloaded is put on a timer to be destroyed, and the next \
               position the shard sends for it into a room that is still loaded rescues it: the body enters \
               there, the timer is cancelled for it and for whatever it is holding, and the room it left is \
               not reloaded to do it. A report that is not grounded, or not newer, rescues nothing, and a \
               rescue is spent when it is used rather than standing in for the next one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-POSITION-ENTRY"),
        station: "dereth-testkit::dat::objects::scenario_an_unloaded_body_is_rescued_by_a_position",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "selection.attackable.the-clients-own-test-is-more-than-is-it-a-creature",
        says: "Whether the client will let a player attack something is more than whether it is a \
               creature: a creature without the mark is refused, something that is not a creature \
               is refused however it is marked, somebody's pet is refused before the mark is even \
               read, another player is refused unless both of them are marked for the same kind of \
               player combat, and one kind of marking overrides all of it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O124-ATTACKABLE"),
        station: "dereth-testkit::cpu::objects::scenario_what_the_client_will_let_you_attack",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "selection.meters.an-answer-about-a-things-magic-fills-its-bar-only-when-it-succeeded",
        says: "Selecting something of the player's own asks the shard how much magic is left in \
               it, and the answer fills that bar only when the shard says the question succeeded; \
               an answer that says it did not leaves the bar down and empty.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O124-MANA"),
        station: "dereth-testkit::dat::objects::scenario_a_magic_answer_fills_the_bar_only_when_it_succeeded",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "selection.meters.the-shards-answer-fills-the-selected-things-health-bar",
        says: "The shard's answer about a selected creature's health brings that bar up and fills \
               it to the fraction the shard sent; an answer about anything else is dropped, and \
               selecting something new takes the bar down again so the last thing's health is \
               never drawn on the new one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O124-METER"),
        station: "dereth-testkit::dat::objects::scenario_the_shards_answer_fills_the_selected_things_health_bar",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "selection.query.a-new-selection-clears-only-the-bar-that-was-showing",
        says: "Selecting something new tells the shard to stop reporting about the old one, once \
               for each bar that was actually showing and not otherwise -- and since the shipped \
               screen brings both bars up hidden, a first selection tells it nothing at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O124-CLEAR"),
        station: "dereth-testkit::dat::objects::scenario_a_new_selection_clears_only_the_bar_that_was_showing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "selection.query.a-pile-that-shrinks-to-one-asks-again-with-no-selection-change",
        says: "A pile of things the player has selected that the shard shrinks to a single one is \
               asked about afresh, although the player changed nothing -- and the read-out beside \
               it does not run again, because the selection itself did not change.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O124-RESEED"),
        station: "dereth-testkit::dat::objects::scenario_a_pile_that_shrinks_to_one_asks_again",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "selection.query.selecting-a-creature-asks-the-shard-about-its-health-once",
        says: "Selecting a creature asks the shard how much life it has left, exactly once, with \
               that creature named -- and having asked, the client does not ask again every frame \
               while the selection stands.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O124"),
        station: "dereth-testkit::dat::objects::scenario_selecting_a_creature_asks_about_its_health_once",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "selection.query.selecting-a-pile-of-things-asks-the-shard-nothing",
        says: "Selecting a pile of things asks the shard nothing at all -- not about its life and \
               not about its magic -- while still setting up the slider that splits it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O124-STACK"),
        station: "dereth-testkit::dat::objects::scenario_selecting_a_stack_asks_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "selection.query.what-the-client-asks-about-a-selected-thing-has-four-outcomes",
        says: "What the client asks about a selected thing has four answers and not two: a \
               creature it would attack, another player and somebody's pet are all asked about \
               their life; something of the player's own is asked about its magic; and something \
               lying on the ground that is nobody's is not asked about at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O124-BRANCH"),
        station: "dereth-testkit::dat::objects::scenario_what_is_asked_has_four_outcomes_and_not_two",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "selection.read-out.follows-one-selection-change-once",
        says: "Selecting something runs the toolbar's read-out and its stack splitter once between them, on \
               the frame the selection changed and on no other; an idle frame runs neither, and a selected \
               stack whose size the shard changes re-seeds the splitter without any selection having changed.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O123-EDGE"),
        station: "dereth-testkit::dat::objects::scenario_the_selection_read_out_follows_one_change_once",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "selection.split-box.is-not-resurrected-by-a-screen-rebuild",
        says: "Rebuilding the game screen with a creature selected does not bring the stack-splitting box and \
               its slider back up beside it, and rebuilding it with a stack selected does bring them back and \
               re-seeds them.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O123"),
        station: "dereth-testkit::dat::objects::scenario_the_split_box_is_not_resurrected_by_a_rebuild",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "shard-message.fellowship-update-done.is-consumed-and-changes-nothing-else",
        says: "The marker the shard sends to say it has finished telling the client about a \
               fellowship is received rather than thrown away, and receiving it is the whole of \
               what it does: no roster moves and no line appears for the player to read.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P2-2-FELLOWSHIP"),
        station: "dereth-testkit::dat::objects::scenario_the_fellowship_done_marker_is_consumed_and_changes_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "shard-message.house-status.the-shards-answer-redraws-the-house-tab-every-time",
        says: "The shard's answer about the player's house redraws the House tab each time it \
               arrives -- twice for two answers, with no guard that skips the second -- and the \
               redraw replaces what was there rather than adding to it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P2-2-HOUSE"),
        station: "dereth-testkit::dat::objects::scenario_the_shards_house_answer_redraws_the_house_tab",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "shard-message.pop-up.the-first-thing-the-shard-says-opens-a-box-the-player-can-dismiss",
        says: "The prompt the shard sends a character the moment he arrives opens a box on the \
               screen carrying the shard's own words, with one button; pressing it takes the box \
               away and deletes it rather than merely hiding it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P2-2-POPUP"),
        station: "dereth-testkit::dat::objects::scenario_the_shards_first_word_opens_a_box_the_player_can_dismiss",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "shard-message.pop-up.three-in-one-burst-are-three-boxes-at-once",
        says: "Three prompts arriving together put three boxes on the screen at the same time, \
               each with its own element and none of them waiting behind another.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P2-2-BURST"),
        station: "dereth-testkit::dat::objects::scenario_three_prompts_in_a_burst_are_three_boxes_at_once",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "shard-message.the-three-the-shard-sends-on-every-login-all-reach-a-receiver",
        says: "The three messages a shard sends every character on every login -- the welcome \
               prompt, the answer about his house and the end of a run of fellowship updates -- \
               all reach something that consumes them, and the client's own record of what it \
               threw away names none of them.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P2-2"),
        station: "dereth-testkit::dat::objects::scenario_none_of_the_three_reaches_no_receiver",
        tier: Tier::Dat,
    },
];
