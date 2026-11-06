//! The client's model of the world: the order the server's messages take effect in, and the
//! player, its qualities, its inventory and its surroundings as the messages build them.
//!
//! A row claims what the September 2013 client does, held to the client itself and to the
//! recorded January 2017 sessions.

use super::{Behaviour, Evidence, Tier, RETAIL, THIS_CLIENT};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "model.characters.a-character-brought-back-loses-its-grey",
        says: "The answer that a deleted character is made again takes its grey off in the list, \
               where it stays one row.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-CHARACTER-BROUGHT-BACK"),
        station: "auberean-client-model::cpu::characters::a_character_brought_back_loses_its_grey",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.characters.a-deletion-is-told-and-the-list-waits-for-the-server",
        says: "A character deleted is told on, and the account's list is left as it is: the list \
               the server sends after the deletion shows the character gone.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CGAME4-CHARACTER-DELETED"),
        station: "auberean-client-model::cpu::characters::a_deletion_is_told_and_the_list_waits_for_the_servers",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.characters.a-new-list-allows-five-characters",
        says: "A character list made before the server sends one holds no characters and allows \
               five.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CGAME3-CHARACTER-SLOTS"),
        station: "auberean-client-model::cpu::characters::a_new_list_allows_five_characters",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.characters.a-refused-character-keeps-the-servers-error",
        says: "A character refused keeps the error the server gave, for the creation screen to say why.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CHARGEN-ERROR"),
        station: "auberean-client-model::cpu::characters::a_refused_character_keeps_the_servers_error",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.characters.a-slot-holds-the-character-at-its-place",
        says: "The character list is read by slot, a character's place in the list: a slot past \
               the end or holding no id has no character, no name and the invalid id; a \
               character's slot is its place, -1 when absent; how long a slot stays greyed out \
               reads 0 too past the number of characters the account may have, when it may have \
               any.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-CHARACTER-SLOTS"),
        station: "auberean-client-model::cpu::characters::a_slot_holds_the_character_at_its_place_or_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.characters.the-list-and-answers-tell-the-screens",
        says: "The character list, and every answer to a character asked for (made or refused), \
               tell the character screens.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-CHARACTER-NOTICES"),
        station: "auberean-client-model::cpu::characters::the_list_and_every_answer_to_a_character_tell_the_screens",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.events.a-forgotten-object-goes-at-the-next-upkeep",
        says: "An object the server has the client forget stays until the next frame's upkeep, \
               before that frame's messages: what comes with the forgetting still finds it; then it \
               goes, with whatever waited for it, and what comes about it afterwards waits for a new \
               creation.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-FORGET"),
        station: "auberean-client-model::cpu::events::a_forgotten_object_goes_at_the_next_upkeep",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.events.a-let-go-of-an-item-not-yet-made-never-takes-effect",
        says: "A let-go of an item the client does not have yet never lets go of it: the client holds it \
               for the item and, when the item comes, sends it through its holder's visual order again, \
               where its stamp is taken, so it is dropped; the item made and then held stays held.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-LET-GO-WAITING"),
        station: "auberean-client-model::cpu::events::a_let_go_of_an_item_not_yet_made_never_takes_effect",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.events.a-newer-incarnation-replaces-the-old-one",
        says: "A creation of a newer incarnation of an object the client has replaces the old one: \
               the old one goes first, and its forgetting and its time to be destroyed go with it, \
               never taking the new one.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-NEWER-INCARNATION"),
        station: "auberean-client-model::cpu::events::a_newer_incarnation_replaces_the_old_one_and_its_forgetting_and_time_to_be_destroyed",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.events.a-private-gap-is-given-up-one-stamp-at-a-time",
        says: "With twenty private messages held and five minutes blocked, a message arriving gives \
               up one missing stamp, and the five minutes start again if the next held one is still \
               not next; a late stamp the give-ups have not reached still takes effect.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CSTREAM-PRIVATE-GIVE-UP"),
        station: "auberean-client-model::cpu::events::a_private_gap_is_given_up_one_stamp_per_five_minutes",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.events.a-private-message-that-does-not-read-holds-nothing-back",
        says: "A private message the client cannot read counts for its stamp, so what the server \
               sends after it takes effect at once rather than waiting out the give-up; it waits \
               for the player's object as a readable one would, and leaves with the world.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CSTREAM-PRIVATE-UNREAD"),
        station: "auberean-client-model::cpu::events::a_private_message_that_does_not_read_holds_nothing_back",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.events.a-queue-for-an-object-being-made-waits",
        says: "A queue of messages waiting for an object that is being made from its records is \
               never given up, however long it waits.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-BEING-CREATED"),
        station: "auberean-client-model::cpu::events::a_queue_for_an_object_being_made_is_never_given_up",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.events.a-repeated-private-stamp-past-a-gap-takes-effect-twice",
        says: "A private stamp arriving again while it is held past a gap is held again and takes \
               effect twice, the later arrival first, and each counts toward the twenty held.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CSTREAM-PRIVATE-REPEAT"),
        station: "auberean-client-model::cpu::events::a_private_stamp_repeated_past_a_gap_takes_effect_twice_later_arrival_first",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.events.a-visual-message-that-does-not-read-holds-nothing-back",
        says: "A visual message's object and stamps stand at its head, whatever its type: one that \
               does not read still counts for its stamp, or waits for its object, and one of any \
               readable type keeps its place in that object's order.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-VISUAL-HEAD"),
        station: "auberean-client-model::cpu::events::a_visual_message_that_does_not_read_holds_nothing_back",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.events.a-visual-stream-blocked-20-seconds-is-given-up",
        says: "An object whose visual messages have waited on a missing stamp for more than 20 \
               seconds is given up on when its next visual message (or its creation) comes: the \
               client asks the server to send the object again and discards what waited, the new \
               message with it. The upkeep alone gives nothing up, and a missing message that comes \
               late lets everything held through.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-VISUAL-GIVE-UP"),
        station: "auberean-client-model::cpu::events::a_visual_stream_blocked_for_20_seconds_is_given_up",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.events.an-object-asked-to-be-destroyed-goes-in-25-seconds",
        says: "An object a script asks to be destroyed (with the objects it carries; never the player) goes at the first upkeep 25 seconds on, a temporary object's a quarter second on, unless the asking is called off; asking again moves the time, an object the client does not have is not asked for, and an object that goes otherwise is no longer asked for.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-TO-BE-DESTROYED"),
        station: "auberean-client-model::cpu::events::an_object_asked_to_be_destroyed_goes_in_25_seconds_unless_called_off",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.events.an-object-not-created-in-25-seconds-is-asked-for",
        says: "Messages about an object the client does not have wait for its creation; when \
               nothing has joined them for 25 seconds the client asks the server to send the object \
               again and drops them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-FORCE-CREATE"),
        station: "auberean-client-model::cpu::events::an_object_not_created_within_25_seconds_is_asked_for_again",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.events.an-unknown-speaker-is-asked-for-25-seconds-after-its-last-line",
        says: "A creature's line to everyone near it, from an object the client was never sent, \
               waits for that object; 25 seconds after the last message that waited for it the \
               client asks the server to send the object again, at the times the recorded clients \
               asked.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-UNKNOWN-SPEAKER"),
        station: "auberean-client-runtime::corpus::chat::an_unknown_speaker_is_asked_for_25_seconds_after_its_last_line",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "model.events.held-messages-wait-for-the-players-object",
        says: "A message held while the client entered the world goes again, in order, once the \
               player is created, under the world's rules: anything but an object's creation then \
               waits for the player's object.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-EVENTS-FLUSH"),
        station: "auberean-client-model::cpu::events::a_message_held_while_entering_then_waits_for_the_players_object",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.events.leaving-the-world-drops-what-was-held",
        says: "Leaving the world lets go of the messages held while entering it, so entering again \
               starts with none.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-EVENTS-DESTROY"),
        station: "auberean-client-model::cpu::events::leaving_the_world_drops_what_was_held_while_entering",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.events.of-two-visual-messages-held-under-one-stamp-the-heap-picks",
        says: "Of two visual messages held under one stamp, the queue's heap decides which goes first, and \
               the other is dropped: after a smaller stamp has passed through, the later of the two goes.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-VISUAL-QUEUE-HEAP"),
        station: "auberean-client-model::cpu::events::of_two_visual_messages_held_under_one_stamp_the_heap_picks_the_later",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.events.private-messages-in-stamp-order",
        says: "The player's private messages take effect in the order of their stamps, whatever \
               order they arrive in; one stamped at or before the last taken is dropped.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-PRIVATE-ORDER"),
        station: "auberean-client-model::cpu::events::private_messages_take_effect_in_stamp_order",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.events.stamps-exactly-half-the-circle-apart-favor-the-smaller",
        says: "Of two stamps exactly half the circle apart the numerically smaller is the newer: a visual \
               message waits or is dropped by it, and an incarnation replaces or is ignored by it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-STAMP-HALF-CIRCLE"),
        station: "auberean-client-model::cpu::events::stamps_exactly_half_the_circle_apart_favor_the_smaller",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.events.the-world-waits-for-the-player",
        says: "While the client enters the world, every message but the player's creation (or the \
               refusal) waits; once the player is created the world's messages wait for the player's \
               object, and its creation lets them through in the order they came.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-EVENTS"),
        station: "auberean-client-model::cpu::events::the_world_waits_for_the_player_and_its_object",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.events.visual-messages-in-stamp-order-per-object",
        says: "An object's visual messages take effect in the order of their visual stamps, \
               starting after its creation's; one stamped at or before that is dropped, and one \
               about a newer incarnation of the object waits while the client has the older one.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-VISUAL-ORDER"),
        station: "auberean-client-model::cpu::events::visual_messages_take_effect_in_stamp_order_per_object",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.events.visual-stamps-held-across-the-wrap-go-round-the-circle",
        says: "An object's visual messages are held and released by the circle of stamps: a stamp past \
               the wrap waits behind those before the wrap, and the wanted stamp is never held up behind it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-VISUAL-QUEUE-CIRCLE"),
        station: "auberean-client-model::cpu::events::visual_stamps_held_across_the_wrap_take_effect_round_the_circle",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.events.what-came-about-a-newer-incarnation-takes-effect-after-it",
        says: "What came about a newer incarnation of an object while the client had the older one \
               takes effect once the newer one is created in its place.",
        since: THIS_CLIENT,
        divergence: "CD2-newer-incarnation-keeps-what-waited",
        evidence: Evidence::Private("AC2-EVID-MODEL-NEWER-INCARNATION-KEEPS"),
        station: "auberean-client-model::cpu::events::what_came_about_a_newer_incarnation_takes_effect_after_its_creation",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.events.what-waited-goes-again-at-the-creation",
        says: "What waited for an object goes again when it is created, in the order it came; one \
               that must wait again stays in the object's queue, which still asks for the object \
               when it is given up if anything that waited in it would have.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-FLUSH-WAITING"),
        station: "auberean-client-model::cpu::events::what_waited_for_an_object_goes_again_and_what_must_wait_again_stays_in_its_queue",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.ids.an-ids-kind-comes-from-its-top-byte",
        says: "An instance id's top byte names its kind: its top bit set a dynamic object, the next \
               a static one (its landblock in the next two bytes), the next a player, 17 the \
               interface's, 19 a temporary one; the camera's test reads bits its own ids leave \
               clear, so they are of no known kind, which counts as static.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-IDS"),
        station: "auberean-client-model::cpu::model::an_ids_kind_comes_from_its_top_byte",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.ids.temporary-ids-are-given-out-from-the-first-free",
        says: "The client's own objects take temporary ids counting up from the first, the first \
               not given out (and held by no object); an id taken back is free again.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-TEMPORARY-IDS"),
        station: "auberean-client-model::cpu::model::temporary_ids_are_given_out_from_the_first_free",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.objects.a-book-keeps-the-cell-its-object-stands-in",
        says: "The client's book on an object keeps the cell its physics object stands in, as the \
               scene says it enters or leaves one; the objects in a cell are found by it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-BOOK-CELL"),
        station: "auberean-client-model::cpu::events::an_objects_book_keeps_the_cell_its_physics_object_stands_in",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.objects.stand-ins-and-the-clients-own-objects-are-kept",
        says: "The camera's stand-ins and the objects the client makes itself are among its \
               objects, each with a physics object and no game object: a stand-in is made once, \
               and an object the client made ends with its lifetime and goes a quarter second \
               after it is asked to be destroyed.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-STAND-INS"),
        station: "auberean-client-model::cpu::events::stand_ins_and_the_objects_the_client_makes_are_kept_without_game_objects",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.replay.every-newer-incarnation-replaces-the-old-one",
        says: "Every creation of a newer incarnation of an object the client still has, recorded \
               in 2013 and 2017, replaces the old one, and the new one is still the client's after \
               the next upkeep.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-NEWER-INCARNATION-REPLAY"),
        station: "auberean-client-runtime::corpus::inventory::every_recorded_newer_incarnation_replaces_the_old_one",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "model.replay.every-recorded-let-go-names-an-item-its-sender-held",
        says: "Every let-go recorded in 2013 and 2017 that finds its item hanging names an item the \
               model has hanging from the message's sender, and the item hangs from nothing after \
               it; one stamped no later than a hold already taken leaves the item held, and one \
               coming after the item's move to a pack, stamped later, has nothing left to do.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-LET-GO-REPLAY"),
        station: "auberean-client-runtime::corpus::inventory::every_recorded_let_go_names_an_item_its_sender_held",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "model.replay.surroundings-and-inventory-agree-with-the-server",
        says: "Through every recorded 2017 game login, the objects the client asks the server for \
               again are the ones the recorded client asked for where the recording shows why, and \
               the items the server lists as the player's are the model's.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-REPLAY"),
        station: "auberean-headless::corpus::sessions::the_models_surroundings_and_inventory_agree_with_the_server",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "model.state.a-boot-is-kept-as-a-refusal-with-its-words",
        says: "A boot ends the login as a refusal for the character booted (reason 20), counted \
               with the other refusals and kept with the server's words, which a refusal of an \
               entry does not carry.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-BOOTED"),
        station: "auberean-client-model::cpu::model::a_boot_is_kept_as_a_refusal_with_its_words",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.state.a-creation-brings-back-an-object-that-left",
        says: "A creation of an object the client still has puts it back in the world where the                creation says it stands, after it left the world: a player who went on ahead through                a door is there when the client arrives.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-CREATION-BRINGS-BACK"),
        station: "auberean-client-model::cpu::model::a_creation_brings_back_an_object_that_left",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.state.a-let-go-names-the-item-and-its-holder-sends-it",
        says: "A held item is let go of by a message its holder sends: the item let go of is the \
               one the message names, in the incarnation it names, not the message's sender.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-LET-GO"),
        station: "auberean-client-model::cpu::model::a_let_go_takes_the_named_item_out_of_its_holders_hand",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.state.a-refused-entry-is-kept-with-its-reason",
        says: "A refused entry into the world (or a refused login) is kept with its reason and \
               counted, so the screens see each refusal, the same reason twice included.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-REFUSED"),
        station: "auberean-client-model::cpu::model::a_refused_entry_is_kept_with_its_reason_and_counted",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.state.a-stale-leaving-or-holding-changes-nothing",
        says: "Leaving the world or being held counts only when its stamp is later than the newest \n               position stamp the object heard; an older or equal one changes nothing, half the \n               circle ahead counting as behind.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-STALE-POSITION-STAMP"),
        station: "auberean-client-model::cpu::model::a_stale_leaving_or_holding_changes_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.state.an-object-made-held-takes-its-position-stamp",
        says: "An object made already held takes the position stamp its description gives: a let-go \
               older than that changes nothing, and a later one lets it go.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-HELD-POSITION-STAMP"),
        station: "auberean-client-model::cpu::model::an_object_made_held_takes_its_position_stamp",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.state.an-object-made-in-no-cell-is-not-placed",
        says: "An object hanging from nothing is placed where its creation puts it only when that \
               is in a cell: a creation that gives a place in no cell leaves it unplaced. It keeps a \
               copy of the creation's weenie description, which holds a package only when it is \
               one (not all ones).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-MADE-NO-CELL"),
        station: "auberean-client-model::cpu::model::an_object_made_in_no_cell_is_not_placed_and_keeps_a_copy_of_its_description",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.state.an-objects-speed-and-jump-scales-are-kept-with-it",
        says: "The speed and jump scales the server sends for an object it made are kept with the \
               object's physics description, as though its creation had carried them: a body made \
               for it afterwards starts with them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-SCALES"),
        station: "auberean-client-model::cpu::model::an_objects_speed_and_jump_scales_are_kept_with_its_description",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.state.player-qualities-and-inventory-from-the-messages",
        says: "The player's creation names the player; an object's description gives its \
               qualities and the private updates change them; objects created inside the player or \
               inside a container it holds are its inventory; leaving the world forgets the world's \
               objects.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-STATE"),
        station: "auberean-client-model::cpu::model::the_player_its_qualities_and_its_inventory_come_from_the_messages",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.state.the-player-id-and-what-an-object-hangs-from",
        says: "The player's id is none until the player's creation names it and none again once \
               the character leaves the world; an object keeps the whole physics description it \
               was made with, and hangs from another as it says.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-PLAYER-ID-PARENT"),
        station: "auberean-client-model::cpu::model::the_players_id_is_known_from_its_creation_and_an_object_keeps_what_it_hangs_from",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.state.the-players-description-replaces-its-weenie-description",
        says: "The player's description message copies its weenie description over the one the \
               player was made with, whichever arrives first; the player keeps the package and entity it \
               was made with.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-PLAYER-DESCRIPTION"),
        station: "auberean-client-model::cpu::model::the_players_description_replaces_the_weenie_description_it_was_made_with",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.world.a-cell-let-go-of-loses-its-moving-loose-objects",
        says: "A cell let go of loses the objects standing in it whose physics objects move and \
               hang from nothing: they leave visibility; static objects and those hanging from \
               another stay.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-RELEASE-CELL"),
        station: "auberean-client-runtime::cpu::world::objects::a_cell_let_go_of_loses_the_objects_in_it_that_move_and_hang_from_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.world.a-game-objects-physics-object-is-made-from-its-descriptions",
        says: "A game object's physics object is made from the physics record its qualities name, \
               its weenie description and its visual description; the world's resources made again \
               are the landscape's.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-PHYSICS-OBJ"),
        station: "auberean-client-runtime::cpu::world::objects::the_scene_makes_a_game_objects_physics_object_from_its_records_and_descriptions",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.world.a-region-the-data-does-not-name-is-not-taken-up",
        says: "The installed data names one region. A player's creation naming another number \
               changes nothing: the world stays as it was and the scene takes nothing up.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-REGION-ONE-NAMED"),
        station: "auberean-client-runtime::corpus::calls::a_region_the_data_does_not_name_is_not_taken_up",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "model.world.a-regions-number-puts-the-world-in-its-region",
        says: "A region's number puts the world in the region whose record the region mapping \
               names, taken up once; the same number again changes nothing, a number with no \
               record leaves the world where it was, and 0 lets the region go.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-REGION"),
        station: "auberean-client-runtime::cpu::world::objects::a_regions_number_puts_the_world_in_its_region_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.world.an-object-is-being-made-until-the-upkeep-makes-it",
        says: "An object is being made until its creation's records have all finished and the \
               objects' upkeep has made it; a queue waiting for it is not given up meanwhile, and \
               leaving the world gives up the creations under way.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-BEING-CREATED"),
        station: "auberean-client-runtime::cpu::world::objects::an_object_is_being_made_until_its_records_finish_and_the_upkeep_makes_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "model.world.every-recorded-player-creation-puts-the-world-in-its-region",
        says: "Every player's creation recorded in 2005 and 2017 names region 1, whose record the \
               install's region mapping names: the scene takes it up at the first creation after \
               entering the world, and a creation again in the same world changes nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-REGION-2017"),
        station: "auberean-client-runtime::corpus::calls::every_recorded_player_creation_puts_the_world_in_its_region",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "model.world.the-objects-upkeep-waits-for-the-cells",
        says: "The objects' upkeep does not run while the scene waits for the cells around the \
               player: an object the server had the client forget stays until it stops waiting.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-UPKEEP"),
        station: "auberean-client-runtime::cpu::world::objects::the_objects_upkeep_waits_while_the_scene_waits_for_the_cells",
        tier: Tier::Cpu,
    },
];
