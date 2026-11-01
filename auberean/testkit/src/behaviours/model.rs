//! The client's model of the world: the order the server's messages take effect in, and the
//! player, its qualities, its inventory and its surroundings as the messages build them.
//!
//! A row claims what the September 2013 client does, held to the client itself and to the
//! recorded January 2017 sessions.

use super::{Behaviour, Evidence, Tier, RETAIL};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
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
        id: "model.events.a-visual-stream-blocked-20-seconds-is-given-up",
        says: "An object whose visual messages have waited on a missing stamp for more than 20 \
               seconds is given up on: the client asks the server to send the object again and \
               discards what waited.",
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
        id: "model.events.private-messages-in-stamp-order",
        says: "The player's private messages take effect in the order of their stamps, whatever \
               order they arrive in; one stamped at or before the last taken is dropped.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-PRIVATE-ORDER"),
        station: "auberean-client-model::cpu::events::private_messages_take_effect_in_stamp_order",
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
               about a newer incarnation of the object waits for that incarnation's creation.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-VISUAL-ORDER"),
        station: "auberean-client-model::cpu::events::visual_messages_take_effect_in_stamp_order_per_object",
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
        id: "model.replay.every-recorded-let-go-names-an-item-its-sender-held",
        says: "Every let-go recorded in 2013 and 2017 that takes effect names an item the model \
               has hanging from the message's sender, and the item hangs from nothing after it.",
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
        id: "model.state.the-players-description-replaces-its-weenie-description",
        says: "The player's description message copies its weenie description over the one the \
               player was made with, whichever arrives first; the player keeps the package and entity it \
               was made with.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-MODEL-PLAYER-DESCRIPTION"),
        station: "auberean-client-model::cpu::model::the_players_description_replaces_the_weenie_description_it_was_made_with",
        tier: Tier::Cpu,
    },
];
