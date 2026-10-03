//! Inventory, trade, the vendor and use -- the pack and everything an item can be dragged into.
//!
//! The ground container, the shortcut bar, the secure-trade window, the shop, the drag-split, the
//! refusal the shard sends back, and the two lines a use prints. One subject, because one drag
//! crosses all of them.
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
        id: "container.ground.a-drag-out-of-the-open-one-onto-the-pack-moves-it-and-keeps-the-mark",
        says: "A drag out of the container the player has open onto the player's own pack asks \
               the shard to put the thing in the pack, once, and the thing stays in the \
               container, greyed, until the shard answers. What is carried is read off the icon \
               in the air and not off the list it came from.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F59-CHEST"),
        station: "dereth-testkit::dat::inventory::scenario_a_drag_out_of_the_open_chest_moves_it_to_the_pack_and_keeps_the_mark",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "container.ground.a-drop-on-one-in-the-world-is-refused-until-it-is-open",
        says: "A chest lying in the world takes nothing until it is open, and it says which of the two \
               reasons it refused for: one that will not open at all is locked, and one that will but is \
               not the container the player has open says so. Open it and the same drop puts the thing \
               in it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O288-CONTAINER-GATES"),
        station: "dereth-testkit::dat::inventory::give::scenario_a_drop_on_a_chest_in_the_world_is_refused_until_it_is_open",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "container.ground.a-drop-on-the-open-ones-own-list-puts-the-thing-in-it-exactly-once",
        says: "Letting something go on an empty slot of the container the player has open on the \
               ground asks the shard to put that thing into that container, and asks exactly \
               once however many handlers saw the release.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F31-DROP"),
        station: "dereth-testkit::dat::inventory::scenario_a_drop_on_the_open_chest_puts_the_thing_in_it_once",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "container.ground.a-drop-on-the-open-ones-own-row-goes-into-it-just-the-same",
        says: "The row above an open ground container's contents holds the container itself, and \
               letting something go on that tile puts the thing into the container just the \
               same, although the row belongs to no container of its own. What the drop is aimed \
               at is whatever is under the pointer.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F31-TOP-ROW"),
        station: "dereth-testkit::dat::inventory::scenario_a_drop_on_the_chests_own_row_goes_into_it_just_the_same",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "container.ground.a-hook-in-a-house-the-player-does-not-own-refuses-the-drop-in-words",
        says: "A drop onto a hook in a house the player does not own is refused where the player \
               is standing, in a sentence naming the hook and the ownership, and nothing at all \
               is asked of the shard.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F31-HOOK"),
        station: "dereth-testkit::dat::inventory::scenario_a_hook_the_player_does_not_own_refuses_the_drop_in_words",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "container.ground.a-hook-in-nobodys-house-refuses-everything-carried-over-it",
        says: "A hook in nobody's house refuses everything carried over it, whatever it is and \
               wherever the hook is -- the answer is given before either of the hook's own two \
               counts is looked at.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-115-UNOWNED"),
        station: "dereth-testkit::dat::inventory::scenario_a_hook_in_nobodys_house_refuses_everything_carried_over_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "container.ground.a-hook-shows-whether-it-could-take-what-is-carried",
        says: "A hook is the one ground container that says no, and it says no on two counts \
               that hold independently: where the hook is, and what kind of thing it takes. \
               Meeting one and missing the other is still a refusal, and meeting both is the \
               green.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-115-HOOK"),
        station: "dereth-testkit::dat::inventory::scenario_a_hook_shows_whether_it_could_take_what_is_carried",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "container.ground.a-pack-goes-in-through-the-windows-own-strip-of-packs",
        says: "The gesture that does put a pack into the container the player has open is the \
               window's own strip of packs, and it sends the move naming that container.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-115-STRIP"),
        station: "dereth-testkit::dat::inventory::scenario_a_pack_goes_into_the_chest_through_its_own_strip",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "container.ground.a-pack-over-the-contents-lights-green-and-is-still-refused-on-the-drop",
        says: "A pack carried over the open container's contents lights green on the way in and \
               is refused when it is let go, in the list's own words -- the window answers the \
               hover and the list answers the drop, and they disagree. The pack is not left \
               marked and the tile it landed on does not stay lit.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-115-PACK"),
        station: "dereth-testkit::dat::inventory::scenario_a_pack_over_the_contents_lights_green_and_is_still_refused_on_the_drop",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "container.ground.a-pickup-goes-into-whichever-pack-the-player-has-open",
        says: "An item taken out of an open ground container goes into the side pack the player \
               has open rather than straight to the player, and with no pack open it goes to the \
               player. A pack the item will not fit in is passed over and the item lands where it \
               would have landed anyway, so an open pack can redirect a pickup but never refuse \
               one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O130-OPEN-PACK"),
        station: "dereth-testkit::cpu::inventory::scenario_a_pickup_follows_the_pack_the_player_has_open",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "container.ground.a-second-one-closes-the-first-and-tells-the-shard",
        says: "Opening a second container lying in the world closes the first one's panel and \
               tells the shard the player has stopped viewing it, naming the container being \
               left and not the one being opened. When the shard is the one that ends the view \
               the panel closes just the same and nothing at all is sent back to it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O130-CLOSE"),
        station: "dereth-testkit::cpu::inventory::scenario_a_second_ground_container_closes_the_first",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "container.ground.contents-become-a-pickup",
        says: "Double-clicking a container lying in the world opens it as the ground container \
               and tells the shard the player is viewing it. While it is open, double-clicking an \
               item inside it picks the item up into the player's pack instead of using it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O130"),
        station: "dereth-testkit::cpu::inventory::scenario_ground_container_contents_become_a_pickup",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "container.ground.opening-one-takes-no-inventory-lock",
        says: "Opening a container lying in the world asks the shard for its contents without \
               taking the player's one-request-at-a-time inventory lock, so a player who opens a \
               corpse can still move, split or give something in the same breath, and a second \
               container is never refused for the first one being busy.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O448-NO-LOCK"),
        station: "dereth-testkit::cpu::inventory::scenario_opening_a_ground_container_takes_no_lock",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "container.ground.the-contents-reply-fills-the-panel-unless-it-answers-a-pickup",
        says: "When the shard lists the contents of the container the player asked to view, the \
               panel opens on that container showing exactly the items the shard named and \
               nothing else. A list for any other container opens no panel, and a list that is \
               only the answer to picking that container up does not raise the panel either.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O130-CONTENTS"),
        station: "dereth-testkit::cpu::inventory::scenario_the_contents_reply_fills_the_ground_panel",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "container.ground.the-contents-reply-frees-a-waiting-request-unless-it-is-a-pickup",
        says: "The shard's list of the open container's contents releases whatever inventory \
               request the player was waiting on for that container, so the client is usable \
               again. If the player was picking that very container up the release is skipped and \
               the pickup goes on waiting, and a list for any other container releases nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O448"),
        station: "dereth-testkit::cpu::inventory::scenario_the_contents_reply_frees_a_waiting_request",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "container.ground.the-open-one-lights-the-row-the-carried-thing-is-over-and-no-other",
        says: "Carrying something over the contents of the container the player has open lights \
               the row it is over, and only that row. A corpse on the ground answers exactly as \
               a chest does: what the window asks about a drop has nothing to do with what kind \
               of thing is lying there.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-115-LIGHT"),
        station: "dereth-testkit::dat::inventory::scenario_the_open_chest_lights_the_row_the_carried_thing_is_over",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "container.ground.the-shards-unlock-reaches-the-chest-and-the-relock-follows-it",
        says: "The shard saying a chest is unlocked reaches the chest, and saying it is locked \
               again reaches it too. Before the unlock arrives the client believes the chest is \
               locked, so the change afterwards cannot be the state it was created with, and the \
               relock is what makes this a live receiver rather than a one-way switch -- a \
               client that dropped these leaves one stale flag and the chest is dead for the \
               rest of the session however many times the right key is used on it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F22-LOCK"),
        station: "dereth-testkit::dat::inventory::scenario_the_shards_unlock_reaches_the_chest_and_the_relock_follows_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "container.ground.the-windows-own-row-answers-a-carried-thing-as-a-list-of-packs-would",
        says: "The row holding the open container itself does not answer a carried thing the way \
               its contents do: it answers as a list of packs answers something carried over a \
               pack with room in it, which is a third answer and not the green.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-115-TOP-ROW"),
        station: "dereth-testkit::dat::inventory::scenario_the_windows_own_row_answers_as_a_list_of_packs_would",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "container.lock.identifying-a-locked-one-says-so-and-how-hard-the-lock-is",
        says: "Identifying a locked container writes that it is locked into the item pane, and \
               writes how hard its lock is to pick together with how much it resists. A lock the \
               shard says cannot be picked at all is called impossible, and the word for an \
               unlocked thing is never written beside it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F31-LOCK"),
        station: "dereth-testkit::dat::inventory::scenario_identifying_a_locked_container_says_so_and_how_hard_the_lock_is",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "container.own-pack.opens-where-it-is-and-is-not-the-ground-container",
        says: "Double-clicking a pack the player is carrying opens it where it hangs rather than \
               as the ground container, so opening one of your own belt pouches does not close \
               the corpse you are looting.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O130-OWN-PACK"),
        station: "dereth-testkit::cpu::inventory::scenario_your_own_pack_opens_where_it_hangs",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "inventory.attunement.an-attuned-thing-says-so-in-its-description",
        says: "A thing that cannot be dropped says so in its own description, before the player \
               tries: the attuned and the bonded properties are both written, in that order, \
               under one heading.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F46-ATTUNED"),
        station: "dereth-testkit::dat::inventory::scenario_an_attuned_thing_says_so_in_its_description",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.backpack-button.a-drop-on-it-draws-the-waiting-row-at-the-head-of-the-pack",
        says: "Dropping something on the toolbar's backpack button draws the waiting row at the \
               head of the pack rather than at any cell, because the player aimed at no cell. \
               Both the thing's real row and the waiting copy above it are greyed until the \
               shard answers, and the answer leaves one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-50C-BACKPACK"),
        station: "dereth-testkit::dat::inventory::scenario_a_drop_on_the_backpack_button_draws_the_waiting_row_at_the_head",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.backpack-button.a-drop-sends-one-move-and-keeps-the-mark-until-the-answer",
        says: "The big backpack button on the toolbar is a drop target of its own: letting \
               something go on it asks the shard once to put that thing in the player's own \
               pack, and the grey mark deliberately stays on it until the shard answers, \
               because this time something really was asked for.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P150-BUTTON"),
        station: "dereth-testkit::dat::inventory::scenario_the_backpack_button_sends_one_move_and_keeps_the_mark",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.backpack-button.a-refused-drop-sends-nothing-and-takes-the-mark-off",
        says: "The same drop made while the player is already waiting on something else \
               sends nothing at all, leaves the request they were waiting on exactly where \
               it was, and takes the grey mark off -- nothing was asked for, so nothing is \
               pending.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P150-BUTTON-REFUSED"),
        station: "dereth-testkit::dat::inventory::scenario_a_refused_backpack_drop_sends_nothing_and_takes_the_mark_off",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.backpack.the-panel-shows-its-title-headings-and-a-dressed-paper-doll",
        says: "The inventory panel is titled Inventory of and the character's own name and headed \
               Contents of Backpack above the grid, both really drawn, and its burden meter \
               carries a value.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O75-BACKPACK"),
        station: "dereth-client::gpu::inventory::backpack_panel_paper_doll::the_panel_shows_the_title_and_the_heading_the_capture_names",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "inventory.backpack.the-paper-doll-keeps-its-own-lens",
        says: "The paper doll is drawn with its space's own 45-degree field of view over its \
               viewport, so the whole figure fits, head to feet; a lens the classic \
               interface gave the shared space does not carry over to it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-R2-DOLL-LENS"),
        station: "dereth-client::gpu::inventory::backpack_panel_paper_doll::the_paper_doll_draws_with_its_own_45_degree_lens_whatever_the_space_was_given",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "inventory.body.a-click-on-a-bare-region-of-the-picture-selects-the-player",
        says: "A click on a part of the picture of the character where nothing is worn selects \
               the player. That is what makes an undressed character clickable at all, and it is \
               the easiest step on the path to lose as a 'nothing there, do nothing' answer.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F48-BARE"),
        station: "dereth-testkit::dat::inventory::scenario_a_click_on_a_bare_region_of_the_picture_selects_the_player",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.body.a-click-on-the-picture-selects-what-is-worn-in-that-region",
        says: "A click on a part of the picture of the character selects what the player is \
               wearing there, and when two things are worn in the same part, the one that is on \
               top. The picture cannot tell those two apart -- they share one painted region -- \
               so a client that answered with whichever it met first would answer wrongly half \
               the time.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F48-SELECT"),
        station: "dereth-testkit::dat::inventory::scenario_a_click_on_the_picture_selects_what_is_worn_in_that_region",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.body.a-place-refuses-in-silence-where-the-picture-refuses-in-words",
        says: "The same thing is refused two ways: let go on a place on the body it does not \
               belong in, it is refused without a word; let go on the picture of the character, \
               it is refused in the client's own sentence. One thing, two elements, two \
               behaviours -- which is what makes the sentence the picture's own and not a \
               property of any refused drop. Both put the grey back and neither takes the one \
               inventory hold.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O166-TWO-REFUSALS"),
        station: "dereth-testkit::cpu::inventory::scenario_a_place_refuses_in_silence_where_the_picture_refuses_in_words",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "inventory.body.a-wearable-dropped-on-the-figure-is-put-on-with-its-own-list-of-places",
        says: "Letting something wearable go on the picture of your character asks the \
               shard to put it on, and what the client asks with is the item's own list of \
               the places it could go rather than one place the client picked -- the shard \
               chooses. While the shard has not answered the icon is greyed and the \
               one-thing-at-a-time lock is held, and nothing at all is said to the player, \
               because a legal wear has nothing to say. The same piece dropped on the place \
               itself asks for exactly the same thing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1166-WEAR"),
        station: "dereth-testkit::dat::inventory::scenario_a_wearable_dropped_on_the_figure_is_put_on_with_its_own_places",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.body.an-occupied-place-names-what-is-in-the-way",
        says: "Something whose place on the body is already taken is refused by name: one \
               line says which thing has to come off first, rather than only that it cannot \
               go on. Nothing is sent and the piece is not left greyed.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1166-BLOCKED"),
        station: "dereth-testkit::dat::inventory::scenario_an_occupied_place_names_what_is_in_the_way",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.body.asking-for-the-grid-of-places-takes-the-picture-out-of-the-hit-test",
        says: "Asking for the grid of places switches the picture of the character off, so a \
               pointer where it was no longer finds it, while what takes a drop is unchanged: it \
               is the hit test that moves and not the map. The switching off is asserted as well \
               as the pointer, because the grid covers the same rectangle and would answer the \
               pointer whether the picture were off or not.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O166-CHECKBOX"),
        station: "dereth-testkit::dat::inventory::scenario_asking_for_the_grid_of_places_takes_the_picture_out_of_the_hit_test",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.body.every-place-on-the-figure-is-a-place-to-wear-and-no-cell-of-a-pack-is",
        says: "All twenty-four places on the figure take a drop as something to wear, whether \
               anything is worn there or not, and each answers with its own place; no cell of \
               the pack grid or of either strip of packs does; and an element that is neither is \
               nobody's drop at all. A client that answered every drop with a wear would be the \
               same fault turned around.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O83-SPLIT"),
        station: "dereth-testkit::dat::inventory::scenario_every_place_on_the_figure_is_a_place_to_wear_and_no_cell_of_a_pack_is",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.body.every-place-the-figure-is-filled-from-answers-a-drop-with-its-own-place",
        says: "Every place on the figure that a recorded character's clothes filled answers a \
               drop with its own place on the body, and that place is one the recording's own \
               placement lands in. The check runs from the screen back to the recording: what \
               the figure is showing is looked up, and the place showing it must be one the \
               recording put that thing in.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O83-MAP"),
        station: "dereth-testkit::dat::inventory::scenario_every_place_the_figure_is_filled_from_answers_a_drop_with_its_own_place",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.body.every-recorded-equip-let-go-on-the-picture-asks-with-its-whole-list-of-places",
        says: "Every thing the recordings record being put on, let go on the picture of the \
               character, asks the shard to put it on with the whole of that thing's own list of \
               places, and the hold and the grey are taken while it waits. A walk over the places \
               would send exactly one of them, so for a thing whose list names more than one \
               place the two answers cannot coincide -- and the recordings carry such a thing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O166-WHOLE-MASK"),
        station: "dereth-testkit::cpu::inventory::scenario_every_recorded_equip_let_go_on_the_picture_asks_with_its_whole_list_of_places",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "inventory.body.every-recorded-placement-can-be-asked-for-and-the-shards-answer-closes-it",
        says: "For every place a recorded character wears something in, letting that thing go on \
               the place on the figure that covers it asks the shard to put it on, naming a part \
               of the thing's own list of places that the place on screen stands for -- never a \
               move into a container. The hold and the grey are taken by the ask and released by \
               the shard's answer, which also records the placement the figure is drawn from.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O83-CENSUS"),
        station: "dereth-testkit::cpu::inventory::scenario_every_recorded_placement_can_be_asked_for_and_the_shards_answer_closes_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "inventory.body.something-already-worn-is-told-so-when-it-is-dropped",
        says: "Dragging a piece off the figure and dropping it straight back on the figure \
               says that it is already being worn. That is the same state the hover answers \
               with silence, and the difference is deliberate: a client that spoke on the \
               hover would nag about every piece the player owns, and one that said nothing \
               on the drop would leave the gesture looking as though it had worked.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1166-WORN"),
        station: "dereth-testkit::dat::inventory::scenario_something_already_worn_is_told_so_when_it_is_dropped",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.body.something-already-worn-leaves-the-figure-saying-nothing",
        says: "Carrying a piece you are already wearing back over the figure leaves the \
               figure saying nothing at all -- not the offer and not the refusal. It is the \
               one thing carried over the body that gets no answer, and a client that \
               showed the red there would be telling the player they cannot wear what they \
               are already wearing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1148-WORN"),
        station: "dereth-testkit::dat::inventory::scenario_something_already_worn_leaves_the_figure_saying_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.body.something-that-could-be-worn-or-wielded-is-still-worn-on-the-picture",
        says: "The gate a drop on the picture makes asks whether the thing could be worn \
               anywhere and not whether it could be worn everywhere: something whose list of \
               places names a place to wear and a place to wield passes it, and the whole list \
               goes out with the wielding place still in it. The shard chooses.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O166-INTERSECTION"),
        station: "dereth-testkit::cpu::inventory::scenario_something_that_could_be_worn_or_wielded_is_still_worn_on_the_picture",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "inventory.body.the-figure-says-whether-it-could-take-what-is-carried",
        says: "The picture of the character answers for the whole body while something is \
               held over it: yes for a thing that can be worn and whose place is free, no \
               for a thing whose place is already taken by something else, and no again for \
               a thing that can be worn nowhere at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1148-HINT"),
        station: "dereth-testkit::dat::inventory::scenario_the_figure_says_whether_it_could_take_what_is_carried",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.body.the-figures-answer-comes-down-on-leave-and-on-the-drop",
        says: "The figure's answer comes down when the pointer leaves it and again when the \
               drag is let go on it -- both, because a drag ends in exactly those two ways \
               and either one left alone would leave the answer standing on screen for the \
               rest of the session.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1148-DOWN"),
        station: "dereth-testkit::dat::inventory::scenario_the_figures_answer_comes_down_on_leave_and_on_the_drop",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.body.the-figures-own-picture-is-not-one-of-the-places-on-it",
        says: "The picture of the character is not one of the twenty-four places on the body, \
               and neither is anything else the window binds beside it. That is why a drop on \
               the picture is answered differently at all: were the picture ever a place, a drop \
               on it would take the place gate and send one place rather than the whole list.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O166-NOT-A-PLACE"),
        station: "dereth-testkit::cpu::inventory::scenario_the_figures_own_picture_is_not_one_of_the_places_on_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "inventory.body.the-picture-of-the-character-is-a-twenty-fifth-place-to-let-something-go",
        says: "The figure takes a drop on the picture of the character as well as on each of its \
               twenty-four places -- twenty-five in all -- and nothing else the window binds \
               takes one. The picture was the one that was missing: a drag released on the \
               character's own body named no target at all, so nothing was asked and the icon \
               was not even put back.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O166-TWENTY-FIFTH"),
        station: "dereth-testkit::dat::inventory::scenario_the_picture_of_the_character_is_a_twenty_fifth_place_to_let_something_go",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.body.the-same-drag-is-a-wear-on-the-figure-and-a-move-into-a-pack",
        says: "The same thing carried by the same gesture is asked to be worn when it is let go \
               on a place on the figure and packed when it is let go on a pack, and what decides \
               is only what it was let go on. Getting it wrong is silent -- no refusal and no \
               message, and the player simply finds the thing in their pack -- and answering \
               every drop with a wear is the same fault turned around, so both halves are \
               measured in one run.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O83-EITHER-WAY"),
        station: "dereth-testkit::dat::inventory::scenario_the_same_drag_is_a_wear_on_the_figure_and_a_move_into_a_pack",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.body.the-shipped-picture-of-the-character-is-painted-for-every-region",
        says: "Which part of the body the pointer is over is read out of a picture shipped with \
               the client, and that picture is painted for all nine regions, with a point \
               outside it belonging to no region at all. A picture with one region unpainted \
               would leave that part of the body dead to the pointer while every other \
               measurement passed.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F48-CLICK-MAP"),
        station: "dereth-testkit::dat::inventory::scenario_the_shipped_picture_of_the_character_is_painted_for_every_region",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.body.what-can-be-worn-and-what-can-be-wielded-overlap-on-one-place-only",
        says: "What can be worn and what can be wielded are two lists of places overlapping on \
               one place only, and between them they do not cover everything: a thing that can \
               only be held is in neither and is refused on the picture in words. The recordings \
               exercise both sides of that gate, which is required here so that a one-sided \
               measurement reads as a broken premise.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O166-MASKS"),
        station: "dereth-testkit::cpu::inventory::scenario_what_can_be_worn_and_what_can_be_wielded_overlap_on_one_place_only",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "inventory.body.what-cannot-be-worn-at-all-is-refused-in-the-clients-own-words",
        says: "Something that can be worn nowhere, dropped on the figure, gets exactly one \
               line saying it cannot be put there, asks the shard for nothing, and is not \
               left greyed.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1166-NOWHERE"),
        station: "dereth-testkit::dat::inventory::scenario_what_cannot_be_worn_at_all_is_refused_in_words",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.burden.load-uses-enchanted-strength",
        says: "How loaded a character is counts the Strength he has with his active enchantments, \
               not only the Strength he raised: the same burden that is a double load at 100 \
               Strength is a single load once a spell lifts Strength to 200.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-JUMP-QUALITIES-BURDEN"),
        station: "dereth-client::cpu::inventory::burden_load::load_inquiry_uses_active_enchanted_strength_not_only_allocated_ranks",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "inventory.burden.the-bar-and-the-number-are-what-the-character-is-carrying",
        says: "The burden bar on the pack page, and the percentage written under it, are what the \
               character is really carrying rather than a figure fixed at login: the number follows \
               the load up and down as things are picked up and put down, and the bar the player \
               sees carries the same value the panel worked out.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O48-BURDEN"),
        station: "dereth-testkit::dat::inventory::burden::scenario_the_burden_bar_and_number_are_what_the_character_is_carrying",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.burden.the-bar-and-the-number-top-out-together-at-three-times-a-full-load",
        says: "The burden bar is drawn on a scale of three full loads, so an ordinary character \
               fills only the foot of a tall frame, and an overloaded one tops the bar out and the \
               number with it at three hundred per cent rather than running past the frame. A \
               character carrying nothing is at the bottom of the bar and at nought.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O48-BURDEN-SCALE"),
        station: "dereth-testkit::dat::inventory::burden::scenario_the_burden_bar_and_number_top_out_together_at_three_times_a_full_load",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.burden.the-bar-fills-from-its-foot-by-covering-part-of-the-frame",
        says: "The burden bar fills from its foot upwards by uncovering part of the picture inside \
               its frame, and nothing on screen moves or is stretched when the load changes -- an \
               empty bar shows none of the picture and a full one shows all of it, and a partly \
               filled one never spills outside the frame.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O48-BURDEN-CLIP"),
        station: "dereth-testkit::dat::inventory::burden::scenario_the_burden_bar_fills_from_its_foot_by_covering_part_of_its_frame",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.busy-mark.a-drag-out-of-the-open-one-that-ends-over-nothing-leaves-no-grey-row",
        says: "Picking a row out of the container the player has open greys it while it is held, \
               and letting go over nothing takes the grey off again and asks the shard for \
               nothing. The grey is kept both on the thing and on the tile drawing it, and both \
               come off.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-50B-CANCEL"),
        station: "dereth-testkit::dat::inventory::scenario_a_cancelled_drag_out_of_the_chest_leaves_no_grey_row",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.busy-mark.a-drag-that-ends-over-nothing-takes-it-off",
        says: "Letting a drag go where nothing can catch it takes the grey mark back off \
               the thing that was picked up -- on the side-pack strip and on the pack grid \
               alike. This is the report the whole family exists for: a side pack dragged \
               and dropped on nothing kept its grey for the rest of the session.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P150-CANCEL"),
        station: "dereth-testkit::dat::inventory::scenario_a_drag_that_ends_over_nothing_takes_the_mark_off",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.busy-mark.a-drop-on-the-shortcut-bar-takes-it-off",
        says: "Dropping something on the shortcut bar makes the shortcut and leaves no grey \
               behind, because nothing was asked of the shard: a shortcut is a reference to \
               a thing and the thing itself has not moved.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P150-ALIAS"),
        station: "dereth-testkit::dat::inventory::scenario_a_drop_on_the_shortcut_bar_takes_the_mark_off",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.busy-mark.a-drop-that-moves-nothing-takes-it-off",
        says: "Dropping something back on the very slot it came from moves nothing and \
               takes the grey mark off, rather than leaving it standing for a move that was \
               never asked for.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P150-NOOP"),
        station: "dereth-testkit::dat::inventory::scenario_a_drop_that_moves_nothing_takes_the_mark_off",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.busy-mark.a-refused-drag-takes-the-mark-off-the-tile-as-well-as-off-the-thing",
        says: "A drag out of the container the player has open that is refused takes the grey \
               mark off the tile as well as off the thing, and leaves the mark standing on a row \
               that really is still waiting. The mark is kept twice and the tile follows the \
               thing down as well as up.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-50G-MIRROR"),
        station: "dereth-testkit::dat::inventory::scenario_a_refused_drag_takes_the_mark_off_the_tile_as_well",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.busy-mark.a-refused-move-out-of-the-open-one-clears-it-and-lets-the-next-one-go",
        says: "A move out of the container the player has open keeps its grey mark until the \
               shard answers, and a refusal takes the mark off and lets the very same gesture be \
               made again at once. The client holds one inventory request at a time and never \
               times one out, so a refusal that cleared the mark without releasing the hold \
               would leave the row unmovable for the rest of the session.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-50B-REFUSAL"),
        station: "dereth-testkit::dat::inventory::scenario_a_refused_move_out_of_the_chest_clears_it_and_lets_the_next_one_go",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.busy-mark.a-row-with-nothing-behind-it-does-not-keep-a-mark-it-was-given",
        says: "A row a container is showing that the client has no object for can still be \
               picked up, and the grey mark that put on it comes off again on the next redraw -- \
               although nothing in the world changed, because there is nothing there to change. \
               A row whose thing is really waiting keeps its mark in the same redraw.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-50G-STALE"),
        station: "dereth-testkit::dat::inventory::scenario_a_row_with_nothing_behind_it_does_not_keep_a_mark_it_was_given",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.busy-mark.a-thing-the-shard-destroys-leaves-no-tile-and-no-mark",
        says: "A thing destroyed by the shard while it is greyed leaves no tile behind at \
               all, and therefore no mark: the tile going away with the thing is the only \
               thing that takes the grey off a thing that no longer exists.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P150-DESTROYED"),
        station: "dereth-testkit::dat::inventory::scenario_a_destroyed_thing_leaves_no_tile_and_no_mark",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.busy-mark.a-tile-clears-its-own-before-catching-another-thing",
        says: "A tile that is itself waiting on an earlier request clears its own grey when \
               it catches a drop, while the thing that was dropped on it keeps its grey and \
               goes on waiting for the shard. They are two different things said to one \
               tile, and a client that answered them with one would either clear the mark \
               on the thing it has just asked about or leave the catcher grey for ever.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P150-CATCHER"),
        station: "dereth-testkit::dat::inventory::scenario_a_tile_clears_its_own_before_catching_another_thing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.busy-mark.every-tile-mirrors-the-things-own-state",
        says: "The grey mark is not something a list remembers: every tile reads it off the \
               thing itself, every frame, so a list emptied and refilled comes back in the \
               state the thing is in -- neither silently losing the mark nor keeping a \
               stale one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P150-MIRROR"),
        station: "dereth-testkit::dat::inventory::scenario_every_tile_mirrors_the_things_own_state",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.busy-mark.the-shards-refusal-and-the-shards-move-both-take-it-off",
        says: "Both answers a shard can give to a move take the grey mark off: the refusal, \
               and the confirmation. The confirmation is the harder half, because by then \
               the tile the mark has to come off is not the tile it went on to -- the thing \
               has moved.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P150-ANSWERS"),
        station: "dereth-testkit::dat::inventory::scenario_the_shards_two_answers_both_take_the_mark_off",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.busy-mark.the-tile-a-drop-lands-on-clears-its-own-and-not-the-source-s",
        says: "The tile a drop lands on takes its own grey mark off, whatever the drop then does, \
               and the thing the drop was carrying keeps its own. They are two different marks \
               about two different requests, and a client that cleared both would un-grey \
               something the shard has not answered about.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-50B-CATCHER"),
        station: "dereth-testkit::dat::inventory::scenario_the_tile_a_drop_lands_on_clears_its_own_grey_mark",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.click.the-right-button-picks-and-examines-and-a-double-click-only-uses",
        says: "The right button on a thing in the pack picks it and asks the shard about it, in that \
               order, so the target box follows what was right-clicked; a double click with the left \
               button uses the thing and neither picks it nor asks about it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O164-BUTTONS"),
        station: "dereth-testkit::dat::inventory::clicks::scenario_the_right_button_picks_and_examines_and_a_double_click_only_uses",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.contain.an-item-contained-before-it-is-created-lands-in-the-named-slot",
        says: "When the shard puts an item into a container before it has described the item, the \
               client already places it in that container's list at the slot the shard named (or \
               at the end when the list is shorter), in the item or the pack list as the shard \
               says, and this holds for every such item in every recording.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O89-CONTAIN"),
        station: "dereth-client::cpu::inventory::contain_before_create::an_item_contained_before_it_exists_is_recorded_in_the_slot_the_server_named",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "inventory.contain.the-pack-follows-the-servers-reply-order",
        says: "Items the shard placed into a pack before describing them end up in the order of \
               the shard's placements and not in the order their descriptions arrived, including \
               in the recordings where those two orders disagree.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O89-CONTAIN-PACK"),
        station: "dereth-client::cpu::inventory::contain_before_create::the_pack_ends_up_in_the_servers_reply_order_not_the_order_the_creates_arrived",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "inventory.container-strip.hides-its-disabled-bar-and-the-grid-keeps-its-scrollbar",
        says: "The inventory's strip of side packs hides its scrollbar while it has nothing to \
               scroll and shows it again as soon as the packs overflow the strip, while the main \
               item grid keeps its scrollbar on screen; hiding and showing the inventory again \
               changes neither.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-INVENTORY-PRESENTATION-CONTAINER-STRIP"),
        station: "dereth-ui-screens::dat::inventory::item_cell_presentation::shipped_container_strip_hides_its_disabled_bar_and_grid_keeps_its_scrollbar",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.cooldown.a-running-ring-counts-down-while-the-pack-is-otherwise-still",
        says: "The ring on an item that is recharging counts down on its own, frame by frame, \
               while the pack around it is held perfectly still and the panel redraws nothing -- \
               and an item with no cooldown of its own never lights a ring at all. Without that \
               separate pass the ring would freeze where the last redraw left it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O524-COOLDOWN"),
        station: "dereth-testkit::dat::inventory::scenario_the_pack_ring_counts_down_while_the_gate_is_shut",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.cooldown.a-thing-the-shard-says-is-recharging-lights-its-ring-and-it-goes-out-when-the-time-is-up",
        says: "A thing the shard says is recharging lights the ring on its slot, taken from that \
               thing's own recharge and from the entry the shard put on the character, and the ring \
               shrinks as the time runs down and goes out altogether once it is up. A thing with a \
               recharge the character is not waiting on never lights a ring.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O128-COOLDOWN-LIVE"),
        station: "dereth-testkit::dat::inventory::overlays::scenario_a_recharging_thing_lights_its_ring_until_the_time_is_up",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.cooldown.exactly-one-wedge-of-the-ring-is-lit-and-it-is-the-one-the-time-left-names",
        says: "Exactly one wedge of the ring is lit at a time, never a stack of them, and which one \
               it is follows how much of the wait is left: a wait almost over lights the first wedge \
               and one barely begun lights the last. The wedge is the one the client's own \
               arithmetic names, to the last place, and not the one a tidier sum would name.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O128-WEDGE-INDEX"),
        station: "dereth-testkit::dat::inventory::overlays::scenario_exactly_one_wedge_of_the_ring_is_lit",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.cooldown.installing-a-cooldown-repaints-only-its-wedge",
        says: "Starting a cooldown on an item repaints only the cooldown wedge on that item's \
               slot; no pixel outside the slot changes.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-SHOTS-RESIDUAL-COOLDOWN"),
        station: "dereth-client::gpu::ui::panel_pixel_residuals::installing_a_cooldown_repaints_only_the_wedge_on_that_items_slot",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "inventory.cooldown.the-ring-steps-once-a-second-and-a-slot-just-filled-shows-its-wedge-at-once",
        says: "The ring steps once a second rather than sliding, so a look taken inside the second \
               shows the wedge the last step left; but a slot filled while a recharge is running \
               draws its wedge the moment it is filled, without waiting up to a second for the next \
               step.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O128-HEARTBEAT"),
        station: "dereth-testkit::dat::inventory::overlays::scenario_the_ring_steps_once_a_second_and_a_new_slot_shows_its_wedge_at_once",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.double-click.a-blocked-one-asks-for-the-same-two-things-as-a-blocked-drop",
        says: "Double-clicking a weapon whose hand is already full asks the shard for exactly \
               what letting it go on that hand asks for, in the same order: first the weapon in \
               the hand goes to the pack, then -- once the shard says it landed there -- the new \
               one is readied into the place it left. Before this, a double-click on a blocked \
               weapon did nothing at all while the drag worked.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O273-SAME-TWO"),
        station: "dereth-testkit::dat::inventory::equip::scenario_a_blocked_double_click_asks_for_the_same_two_things_as_a_blocked_drop",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.double-click.a-blocked-one-says-only-which-thing-is-being-moved-and-says-it-once",
        says: "A blocked double-click says exactly one thing, and it is the same one thing a \
               blocked drop says: which weapon is being moved to the pack to make room. None of \
               the sentences about already wielding something appear, and the line goes out on \
               the client's own red channel -- which is what makes it look like an error when it \
               is not one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O273-ONE-RED-LINE"),
        station: "dereth-testkit::dat::inventory::equip::scenario_a_blocked_double_click_says_only_which_thing_is_being_moved_and_says_it_once",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.double-click.a-piece-of-armour-whose-place-is-taken-is-refused-out-loud",
        says: "Double-clicking a piece of armour whose place on the body is taken refuses out \
               loud, naming the piece that has to come off, and asks the shard for nothing: \
               nothing is moved out of the way for it, unlike a weapon whose hand is full. \
               Without the line the gesture is indistinguishable from a dead click.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O273-WEAR-REFUSAL"),
        station: "dereth-testkit::dat::inventory::equip::scenario_a_piece_of_armour_whose_place_is_taken_is_refused_out_loud",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.double-click.a-ring-may-go-on-the-free-hand-where-a-drop-on-the-full-one-may-not",
        says: "The one place the two gestures are meant to disagree. A ring that fits either \
               hand, with the hand the client tries first already wearing one and the other free: \
               a double-click puts it on the free hand and takes nothing off, while letting it go \
               on the full hand takes that ring off instead, because the player aimed there.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O273-OTHER-HAND"),
        station: "dereth-testkit::dat::inventory::equip::scenario_a_ring_may_go_on_the_free_hand_where_a_drop_on_the_full_one_may_not",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.double-click.a-thing-that-cannot-be-held-in-a-fight-says-so",
        says: "A torch cannot be held while the player has a weapon out, and double-clicking one \
               says so by name and asks the shard for nothing. The double-click's own refusals \
               are spoken, which is a different silence from the one a blocked drop keeps.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O273-SPOKEN-REFUSAL"),
        station: "dereth-testkit::dat::inventory::equip::scenario_a_thing_that_cannot_be_held_in_a_fight_says_so",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.double-click.every-recorded-one-answers-as-the-drop-does-wherever-they-share-a-path",
        says: "Every thing the recordings record being put on, staged with its own place already \
               taken and then both double-clicked and let go on that place, answers by the path \
               the client decides a double-click on it takes. Where a double-click readies a \
               thing it asks for exactly what the drop asks for, except over the paired hands \
               where the two are meant to differ; where it instead looks for a free place it asks \
               for nothing at all, because that way of putting something on never moves anything \
               out of the way.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O273-CORPUS-BOTH"),
        station: "dereth-testkit::dat::inventory::equip::scenario_every_recorded_one_answers_as_the_drop_does_wherever_they_share_a_path",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.double-click.something-that-can-be-worn-or-held-falls-through-to-the-hand",
        says: "Double-clicking something that could be worn or held, with the place on the body \
               it would be worn in already taken, readies it in the hand instead of giving up: \
               the client tries to wear it, is refused, and goes on to try the hand.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O273-FALL-THROUGH"),
        station: "dereth-testkit::dat::inventory::equip::scenario_something_that_can_be_worn_or_held_falls_through_to_the_hand",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.drag-hint.a-body-slot-shows-whether-it-could-take-what-is-carried",
        says: "Each place on the body says yes or no for the thing being carried over it, \
               and the answer is not the slot's own list of places alone: a torch that may \
               be held is taken by the weapon hand while the player is at peace and refused \
               by the same hand with a weapon out. A one-handed sword is offered the shield \
               hand, and a thing that can be worn nowhere is refused everywhere.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P14-DOLL"),
        station: "dereth-testkit::dat::inventory::scenario_a_body_slot_shows_whether_it_could_take_what_is_carried",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.drag-hint.a-real-drag-puts-the-hint-up-on-what-it-crosses-and-takes-it-down-again",
        says: "Carrying an icon over a cell puts that cell's hint up, moving on takes the hint of \
               the cell that was left down again, and letting go leaves nothing standing \
               anywhere. The three cells crossed give three different answers -- a cell of the \
               player's own grid takes a plain thing, an empty cell of the pack strip refuses \
               one, and a pack with room says the thing would go into it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O271-LIVE-DRAG"),
        station: "dereth-testkit::dat::inventory::scenario_a_real_drag_puts_the_hint_up_on_what_it_crosses_and_takes_it_down_again",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.drag-hint.a-shortcut-tile-offers-itself-for-anything-carried",
        says: "A shortcut tile says yes to anything carried over it and stops saying \
               anything when the pointer moves off, because a shortcut is a place to keep a \
               reference to a thing rather than the thing itself, so nothing about the \
               thing can refuse it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P14-SHORTCUT"),
        station: "dereth-testkit::dat::inventory::scenario_a_shortcut_tile_offers_itself_for_anything_carried",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.drag-hint.every-live-slot-carries-the-overlay-the-hint-is-drawn-on",
        says: "Every cell of the pack grid and of the side-pack strip is built with the overlay \
               the hint is drawn on, and none of them starts with a hint showing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O271-OVERLAY"),
        station: "dereth-testkit::dat::inventory::scenario_every_live_slot_carries_the_overlay_the_hint_is_drawn_on",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.drag-hint.letting-go-on-a-tile-takes-its-own-hint-down",
        says: "Letting go on the very tile that is lit takes its hint down and ends the \
               drag, on the element the player is looking at as well as in the panel's own \
               record. Without it the hint would only ever be cleared by the pointer \
               leaving, and a drop would leave it standing for the rest of the session.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P128-DROP"),
        station: "dereth-testkit::dat::inventory::scenario_letting_go_on_a_tile_takes_its_own_hint_down",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.drag-hint.no-cursor-the-client-ships-is-chosen-by-a-drag",
        says: "The mouse pointer does not change while an icon is being carried, and it cannot: \
               what the drag is doing is not among the things the pointer picture is chosen \
               from, several shipped pictures are chosen by nothing at all, and not one of the \
               shipped pictures is named for a drag, a drop or a refusal. The hint drawn on the \
               cell under the pointer is the whole of what answers a hover.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O271-NO-CURSOR"),
        station: "dereth-testkit::dat::inventory::scenario_no_cursor_the_client_ships_is_chosen_by_a_drag",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.drag-hint.setting-the-same-hint-twice-raises-nothing",
        says: "Putting on a cell the hint it already has changes nothing, and a cell with no \
               overlay to draw a hint on does nothing at all. Without the first, a pointer \
               sitting still over one cell would restate that cell's picture every frame for as \
               long as the player held the icon there.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O271-SAME-TWICE"),
        station: "dereth-testkit::cpu::inventory::scenario_setting_the_same_hint_twice_raises_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "inventory.drag-hint.the-backpack-button-lights-while-something-is-carried-over-it",
        says: "The big backpack button on the toolbar lights up while something is held \
               over it and goes dark again the moment the pointer leaves, so a player \
               carrying an icon can see that letting go there will put it away.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P146-BUTTON"),
        station: "dereth-testkit::dat::inventory::scenario_the_backpack_button_lights_while_something_is_over_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.drag-hint.the-crossing-is-reported-as-a-leave-and-an-enter-in-that-order",
        says: "As a carried icon crosses from one thing that can take a drop to the next, the \
               one being left is told first and the one being entered second. Entering the first \
               thing of a drag tells nothing that it was left, and carrying the icon off \
               everything tells the last one it was left and nothing that it was entered.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O326-EDGES"),
        station: "dereth-testkit::dat::inventory::scenario_the_crossing_is_reported_as_a_leave_and_an_enter_in_that_order",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.drag-hint.the-crossing-message-is-not-the-focus-message",
        says: "What a crossing is reported in is not what a keyboard-focus change is reported \
               in: taking the focus tells the gainer, moving it tells the loser and then the \
               gainer, and giving it up tells the loser alone. Switching an element on tells \
               nothing at all, and switching one off tells whatever holds the focus rather than \
               the element being switched off.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O326-FOCUS"),
        station: "dereth-testkit::dat::inventory::scenario_the_crossing_message_is_not_the_focus_message",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.drag-hint.the-drop-is-aimed-at-the-element-the-cursor-was-over",
        says: "What a carried icon is let go on is the element the cursor was last over -- the \
               cell itself, and not the list that cell belongs to. The release tells that cell \
               and tells the cell the icon came from, and the two copies are told apart by which \
               one names the drag's owner.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O326-DROP-TARGET"),
        station: "dereth-testkit::dat::inventory::scenario_the_drop_is_aimed_at_the_element_the_cursor_was_over",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.drag-hint.the-lists-that-take-no-drop-show-no-hint-rather-than-a-refusal",
        says: "A shop's stock, a salvage list and the shortcut bar answer a carried icon with \
               nothing at all rather than with a refusal, and so does an icon carrying nothing \
               and a drag that is not an inventory move. A drag carrying a pack is not one of \
               those and does get a real answer, which is what makes the silence a claim.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O271-EARLY-OUTS"),
        station: "dereth-testkit::cpu::inventory::scenario_the_lists_that_take_no_drop_show_no_hint_rather_than_a_refusal",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "inventory.drag-hint.which-hint-a-slot-shows-for-what-is-carried-over-it",
        says: "A list of loose things takes a plain thing over any cell of it and refuses a \
               pack; a strip of packs takes a pack, refuses a plain thing over an empty cell, \
               refuses one over a pack with no room, and says 'into this container' over a pack \
               with room. A pack with no limit reads as having room, and something the client \
               knows nothing about is refused.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O271-TABLE"),
        station: "dereth-testkit::cpu::inventory::scenario_which_hint_a_slot_shows_for_what_is_carried_over_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "inventory.drag.a-drag-from-the-paper-doll-lifts-the-worn-item",
        says: "Pressing on the paper doll where something is worn and moving past the drag \
               threshold lifts the item worn outermost there, the breastplate rather than the \
               shirt under it, as an ordinary move; unlike a drag out of a pack, the item is not \
               greyed while it is carried.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G31-DRAG"),
        station: "dereth-ui-screens::dat::inventory::paperdoll_drag::a_real_drag_from_the_paper_doll_picks_the_worn_item_up",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.drag.a-drop-on-a-side-pack-becomes-one-move-naming-the-pack-under-the-pointer",
        says: "Letting an icon go over one of the side packs asks the shard to move that item \
               into that pack, once, and the pack it names is the one the strip slot under the \
               pointer is showing rather than whichever pack the panel happens to have open. \
               Until the shard answers the item has not moved and its slot is held greyed, and \
               it is the shard's own answer that puts it in the pack and clears the grey.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O47-DROP"),
        station: "dereth-testkit::dat::inventory::scenario_a_drop_on_a_side_pack_becomes_one_move",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.drag.a-pack-slot-answers-the-pointer-at-its-own-centre",
        says: "Every slot the pack grid actually shows is something the pointer can find: a hit \
               test at a slot's own centre answers that slot and not the panel behind it, and \
               each of them carries the picture a drag is lifted from. Without it the player \
               cannot touch their inventory at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O47-HIT"),
        station: "dereth-testkit::dat::inventory::scenario_a_pack_slot_answers_the_pointer_at_its_own_centre",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.drag.a-press-and-a-move-lift-the-items-own-picture-off-its-slot",
        says: "Pressing on a filled slot and moving the pointer far enough lifts the item's own \
               picture out of it -- the picture without the slot's frame behind it -- and \
               what is lifted knows which item it is carrying and that it is an ordinary \
               inventory move rather than a sale, a shortcut or a salvage. It follows the \
               pointer, and the slot it came from is greyed from the moment it is picked up \
               while the item itself has not moved.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O47-PICKUP"),
        station: "dereth-testkit::dat::inventory::scenario_a_press_and_a_move_lift_the_items_own_picture",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.drag.an-empty-slot-starts-nothing-and-a-list-that-forbids-it-starts-nothing",
        says: "A press and a move on an empty slot lifts nothing and leaves the pointer clean \
               for the next press, which is what most presses on a pack of a hundred cells are; \
               and a list that is marked as not draggable lifts nothing from a filled slot \
               either, while the same list with the mark back does.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O47-GATES"),
        station: "dereth-testkit::dat::inventory::scenario_an_empty_slot_and_a_forbidden_list_start_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.environment-host.covers-closes-and-reopens-without-toolbar-restore",
        says: "Opening a container on the ground brings up the window that holds it, sized within \
               its limits. Opening another panel in that window covers it and closes the \
               container, telling the shard once; closing the other panel leaves the window shut \
               rather than falling back to the container, and opening the container again brings \
               it back.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-EXTERNAL-CONTAINER-ENVIRONMENT-HOST"),
        station: "dereth-ui-screens::dat::inventory::external_container::environment_host_covers_closes_and_reopens_without_toolbar_restore_rules",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.equip.a-place-that-is-full-with-nothing-recorded-in-it-refuses-without-a-word",
        says: "A place on the body the thing is allowed in, which the client believes is full but \
               has no record of what is in, refuses the drop and says nothing at all: nothing is \
               asked of the shard, the grey comes off and no hold is taken. The silence is the \
               claim -- every line about already wearing one of those belongs to the other way \
               of putting something on, and the figure never shows one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O116-OCCUPIED-SILENT"),
        station: "dereth-testkit::dat::inventory::equip::scenario_a_place_that_is_full_with_nothing_recorded_in_it_refuses_without_a_word",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.equip.a-place-the-thing-cannot-go-in-refuses-it-and-a-place-it-can-takes-it",
        says: "Letting something go on a place on the body its own list of places does not name \
               asks the shard for nothing at all, counts the refusal, takes the grey off the icon \
               and takes no hold; letting the same thing go on a place its list does name asks \
               exactly once and leaves the icon grey. Both directions are measured over every \
               place on the figure, because a client that refused everything would satisfy one \
               half and never reach the other.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O116-GATE"),
        station: "dereth-testkit::dat::inventory::equip::scenario_a_place_the_thing_cannot_go_in_refuses_it_and_a_place_it_can_takes_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.equip.a-shield-is-refused-in-words-while-a-weapon-for-both-hands-is-held",
        says: "A shield dragged onto its place on the figure while a weapon that needs both hands \
               is held is refused, in a sentence naming the weapon, and nothing at all is asked \
               of the shard -- the weapon is not taken off to make room for the shield. The \
               asymmetry is the claim: one direction moves the thing in the way, the other says \
               no.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P152-SHIELD-REFUSED"),
        station: "dereth-testkit::dat::inventory::equip::scenario_a_shield_is_refused_in_words_while_a_weapon_for_both_hands_is_held",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.equip.a-stack-let-go-on-a-matching-one-on-the-body-merges-what-fits",
        says: "Dragging a stack onto a place on the body where the same kind of thing is already \
               worn adds to it rather than taking it off: the client asks for as many as will \
               fit, no more, and asks for nothing else. Both places that can hold a stack are \
               measured, the quiver and the hand, because they are two different tests in the \
               client.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P152-MERGE"),
        station: "dereth-testkit::dat::inventory::equip::scenario_a_stack_let_go_on_a_matching_one_on_the_body_merges_what_fits",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.equip.a-weapon-let-go-on-the-shield-place-is-taken-into-the-off-hand",
        says: "A melee weapon let go on the shield is the one drop the figure invents an answer \
               for: the weapon's own list does not name the shield, so the gate would refuse it, \
               and instead the client asks for the shield -- the weapon goes in the off hand. The \
               same weapon let go on the weapon place asks for the main hand, so what is measured \
               is the side and not the weapon.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O116-OFF-HAND"),
        station: "dereth-testkit::dat::inventory::equip::scenario_a_weapon_let_go_on_the_shield_place_is_taken_into_the_off_hand",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.equip.a-weapon-that-cannot-share-the-hands-takes-what-is-in-the-way-off-first",
        says: "Readying a weapon that cannot share the hands with what is already held takes the \
               thing in the way off first, saying which, and asks for the weapon itself only once \
               the shard says that thing landed in the pack. A bow or a weapon needing both hands \
               with a shield on, and a bow with a sword in the hand, are all one story; a \
               one-handed sword beside a shield is the one pairing that is allowed and goes \
               straight out with the shield left where it is.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P152-HAND-CONFLICT"),
        station: "dereth-testkit::dat::inventory::equip::scenario_a_weapon_that_cannot_share_the_hands_takes_what_is_in_the_way_off_first",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.equip.every-recorded-one-let-go-on-its-own-place-asks-for-what-was-recorded",
        says: "Every thing a recording's own client asked to put on, let go on the place on the \
               figure that thing's own list picks out, asks the shard for a place inside that \
               list which covers the place the player aimed at, and the recordings agree: a place \
               among the clothes asks with the whole list, a place outside them asks with one, \
               and for a thing that can go in only one place the two are the same place.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O116-CORPUS-REPLAY"),
        station: "dereth-testkit::dat::inventory::equip::scenario_every_recorded_equip_let_go_on_its_own_place_asks_for_what_was_recorded",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.equip.part-of-a-stack-let-go-on-the-figure-splits-and-a-whole-one-does-not",
        says: "The quantity the player set decides which request goes out when a stack is let go \
               on the body: an untouched box moves the whole stack, and a box set to some of them \
               splits that many off instead, at the same place and for the player's own number, \
               taking no hold. Both sides of the fork are measured, because the whole list and \
               the single place hand the quantity over at different points.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O116-SPLIT"),
        station: "dereth-testkit::dat::inventory::equip::scenario_part_of_a_stack_let_go_on_the_figure_splits_and_a_whole_one_does_not",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.equip.the-left-and-the-right-of-a-pair-ask-for-different-places",
        says: "The left wrist and the right wrist are two different places, and so are the two \
               ring fingers: something let go on one of them is asked for at that one, and the \
               same thing let go on the other is asked for at the other. The thing is allowed in \
               both halves of the pair, which is what makes the side load-bearing -- with only \
               one half allowed the answer would be forced whatever the client did.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O116-SIDES"),
        station: "dereth-testkit::dat::inventory::equip::scenario_the_left_and_the_right_of_a_pair_ask_for_different_places",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.equip.the-name-in-that-refusal-is-pluralised-by-the-clients-own-simple-rule",
        says: "When the shard never sent a plural name for a thing, the client makes one by \
               adding a single letter and treats only a name already ending in that letter \
               specially -- so a thing called a Lockpix is refused as Lockpixs rather than as \
               Lockpixes. The wording reaches the strip the player reads, on the same real drag \
               as the ordinary refusal.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P152-PLURAL"),
        station: "dereth-testkit::dat::inventory::equip::scenario_the_name_in_that_refusal_is_pluralised_by_the_clients_own_simple_rule",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.equip.the-place-let-go-on-decides-whether-the-whole-list-or-one-place-goes-out",
        says: "What decides whether the client asks with the thing's whole list of places or with \
               one of them is the place it was let go on, not the thing: one and the same piece \
               leaves by both answers depending on where the player aimed. A piece of armour let \
               go outside the clothes is asked for at one place, and a piece of clothing let go \
               among them is asked for with the whole of its list.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O116-FORK"),
        station: "dereth-testkit::dat::inventory::equip::scenario_the_place_let_go_on_decides_whether_the_whole_list_or_one_place_goes_out",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.external-container.leaving-use-range-closes-the-window-once",
        says: "An open corpse's loot window is watched for range: while the player stands on the \
               corpse it stays open, even when the corpse gave no use radius, and once he walks \
               away it closes, once, sending the same close request the recorded retail session \
               sent. Range watches still in reach keep running.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-LOOT-PANEL-EXTERNAL-CONTAINER"),
        station: "dereth-client::gpu::inventory::corpse_loot_window::app_range_exit_closes_the_window_once_and_preserves_an_in_range_control",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "inventory.external-container.moving-the-open-child-reopens-the-top-and-unrelated-moves-do-not",
        says: "With a corpse open and a pack inside it opened, moving some other item, or \
               reordering the pack within the corpse, leaves the pack open; once the pack itself \
               is moved out of the corpse the view goes back to the corpse's own contents, the \
               pack leaves the list and the client asks for the corpse to be the open container \
               again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-EXTERNAL-CONTAINER-EXTERNAL-CONTAINER"),
        station: "dereth-ui-screens::dat::inventory::external_container::moving_the_open_child_reopens_the_top_but_unrelated_moves_do_not",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.give.a-drop-on-a-creature-asks-to-hand-it-over-and-nothing-goes-first",
        says: "Something carried, let go over the world and picked onto a creature, is offered to that \
               creature: the recipient, the thing and how many. Nothing at all goes out before the pick \
               has answered, and nothing is predicted -- the thing stays in the pack, greyed, until the \
               shard says otherwise.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O288-GIVE"),
        station: "dereth-testkit::dat::inventory::give::scenario_a_drop_on_a_creature_asks_to_hand_it_over",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.give.a-drop-on-yourself-is-the-pickup-and-not-a-gift",
        says: "Letting something go over your own body is the pickup -- it asks for the thing to go into \
               your own pack -- and is not a gift to yourself. That answer is given before anything \
               about a target is looked at.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O288-SELF"),
        station: "dereth-testkit::dat::inventory::give::scenario_a_drop_on_yourself_is_the_pickup_and_not_a_gift",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.give.every-recorded-one-is-answered-by-the-move-and-by-nothing-else",
        says: "Every give the recordings carry is answered by the shard saying the thing is now \
               inside the recipient it was handed to, and by nothing else. No recorded failure \
               names a thing that was given, and no recorded give is part of a stack, so the two \
               other answers that could release the client's hold are mechanisms the recordings \
               do not witness.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O416-CENSUS"),
        station: "dereth-testkit::dat::inventory::scenario_every_recorded_give_is_answered_by_the_move_and_nothing_else",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.give.every-recorded-one-is-reproduced-by-a-drop-onto-its-recipient",
        says: "Every gift the recordings hold is reproduced by a drop onto its own recipient, with the \
               recipient, the thing and how many all coming out of the recording. A client naming the \
               wrong recipient would be giving somebody else's things away, and nothing about that is \
               visible on screen.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O288-RECORDED"),
        station: "dereth-testkit::dat::inventory::give::scenario_every_recorded_give_is_reproduced_by_a_drop_onto_its_recipient",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.give.handing-something-over-in-mid-air-is-not-refused",
        says: "Handing something to somebody while the player is falling goes through: the \
               refusal is only on the leg that puts a thing on the ground, and a client that \
               refused the gift as well would be stricter than the one being rebuilt.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O417-GIVE"),
        station: "dereth-testkit::dat::inventory::scenario_a_give_in_mid_air_is_not_refused",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.give.how-many-is-the-splitters-count-for-the-picked-thing",
        says: "How many are handed over is the splitter's own count when the thing handed over is the \
               one the player has picked, and the whole stack for anything else. A client that always \
               sent the whole stack would look right to anybody holding one of something.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O288-AMOUNT"),
        station: "dereth-testkit::dat::inventory::give::scenario_how_many_is_the_splitters_count_for_the_picked_thing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.give.part-of-a-stack-is-released-by-the-stacks-new-count",
        says: "Handing over part of a stack sends the split size and holds the source stack, so \
               what releases the hold is the shard saying what the source stack now holds -- and \
               the stack counts down by what was given away.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O416-PARTIAL"),
        station: "dereth-testkit::dat::inventory::scenario_part_of_a_stack_is_released_by_the_stacks_new_count",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.give.something-not-yours-or-on-the-trade-table-is-refused-in-words",
        says: "Two things are never handed over, and each is refused in its own sentence naming the \
               thing: one that is not the player's to give, and one already on the trade table. Neither \
               asks the shard anything, and the refusal takes the grey off the thing again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O288-ITEM-GATES"),
        station: "dereth-testkit::dat::inventory::give::scenario_something_not_yours_or_on_the_trade_table_is_refused_in_words",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.give.something-that-is-a-creature-and-a-container-too-is-not-given-to",
        says: "What kind of thing the target is decides this, and the client asks for that kind exactly \
               rather than for a bit of it. Something that is a creature and a container both is \
               therefore not given to at all: it falls through to the container answer and is refused \
               for being shut. One bit away, the same thing is given to.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O288-EQUALITY"),
        station: "dereth-testkit::dat::inventory::give::scenario_something_that_is_a_creature_and_a_container_too_is_not_given_to",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.give.the-contents-of-a-container-being-viewed-do-not-release-one",
        says: "The shard listing the contents of a container the player has open does not \
               release a give, because that answer names a container and a give holds a thing. \
               The answer that can release it still does afterwards.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O416-CONTAINER"),
        station: "dereth-testkit::dat::inventory::scenario_a_container_being_viewed_does_not_release_a_give",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.give.the-hold-it-takes-refuses-the-next-gesture-in-the-clients-own-words",
        says: "Handing something over takes the client's one inventory hold, naming the thing \
               and not the recipient, and while it is held the next gesture is refused where the \
               player is standing with nothing at all reaching the shard. The refused gesture \
               neither takes the hold nor releases it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O416-HELD"),
        station: "dereth-testkit::dat::inventory::scenario_a_give_takes_the_hold_and_the_next_gesture_is_refused_in_words",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.give.the-shards-move-releases-the-hold-and-the-next-one-goes-out",
        says: "The shard saying the given thing is now inside the recipient releases the hold, \
               clears the mark the drag left on the thing, and leaves the next give free to go \
               out.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O416-MOVE"),
        station: "dereth-testkit::dat::inventory::scenario_the_shards_move_releases_the_give_and_the_next_one_goes_out",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.give.the-shards-refusal-releases-the-hold-and-says-which-thing",
        says: "The shard refusing a give releases the hold as well, names the thing where the \
               player is standing, and leaves the client able to try again. A refusal that \
               released nothing would leave the player unable to move anything for the rest of \
               the session, because the hold is never timed out.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O416-FAILED"),
        station: "dereth-testkit::dat::inventory::scenario_the_shards_refusal_releases_the_give_and_says_which_thing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.held-item.a-holders-own-list-decides-what-hangs-off-it",
        says: "A creature's description carries the list of what it is holding, and that list is \
               what attaches: an item already known is put into the place the list names even \
               though its own description said nothing about a holder, an item the list names \
               that the client has never seen is counted rather than invented, and when a holder \
               is described afresh anything its new list leaves out is no longer held by it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O76-CHILDREN"),
        station: "dereth-testkit::dat::inventory::scenario_a_holders_own_list_decides_what_it_holds",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.icons.every-spell-in-the-book-draws-its-background-row",
        says: "Every spell in a recorded spellbook is drawn as a composed icon on its level's \
               spell background, tinted in every case and badged where the spell calls for it, \
               rather than as its bare picture.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O169-ICONS-EVERY"),
        station: "dereth-client::dat::inventory::icon_composite::every_spell_in_the_captures_book_draws_a_spell_background_row",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.icons.the-effects-ring-replaces-the-icons-white-contour",
        says: "An item icon's pure-white outline is recoloured by its effects ring -- black for an \
               ordinary item, the magical colouring for a magical one -- and exactly those white \
               pixels change, nothing else in the icon.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O169-ICONS"),
        station: "dereth-client::dat::inventory::icon_composite::ordinary_and_magical_effects_both_replace_the_icons_white_contour",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.lock.an-inventory-request-wedges-until-answered",
        says: "An inventory request holds the inventory lock with no time limit: a second request \
               for the same item is still refused a second, a minute or a whole day later, and \
               nothing more is sent.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-INTERACTION-LOCK"),
        station: "dereth-client::gpu::inventory::inventory_request_lock::the_inventory_lock_wedges_and_no_amount_of_time_clears_it",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "inventory.pack.a-damaged-thing-shows-how-worn-it-is-and-a-whole-one-shows-nothing",
        says: "A thing that has been worn down shows a bar on its slot saying how much of it is left, \
               and a thing that is whole shows no bar at all -- as does a thing that cannot wear down. \
               Repairing it takes the bar away again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O48-STRUCTURE"),
        station: "dereth-testkit::dat::inventory::slots::scenario_a_damaged_thing_shows_how_worn_it_is",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pack.a-pack-that-leaves-the-player-hands-the-grid-back-to-the-players-own-things",
        says: "A side pack the player opened from the strip and then loses while looking into it -- \
               dropped, given away or put inside something else -- stops being shown: the grid goes \
               back to the player's own things and the main pack is the open one again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O164-DEPARTED-PACK"),
        station: "dereth-testkit::dat::inventory::clicks::scenario_a_pack_that_leaves_the_player_hands_the_grid_back",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pack.a-pack-with-something-in-it-shows-how-full-it-is-and-an-empty-one-shows-nothing",
        says: "A pack with something in it carries a small bar on its slot saying how full it is, \
               measured against what it can hold; an empty pack carries no bar at all, which is not \
               the same thing as a bar at nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O48-CAPACITY"),
        station: "dereth-testkit::dat::inventory::slots::scenario_a_pack_with_something_in_it_shows_how_full_it_is",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pack.a-press-on-a-slot-selects-examines-or-uses-the-thing-in-that-slot",
        says: "A press on a slot of the pack does one of three different things depending on how \
               it was pressed -- select it, look at it, or use it -- and every one of them names \
               the thing in the slot that was pressed rather than the last thing touched. A slot \
               with nothing in it does nothing at all, which is not the same as deselecting.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O85-SLOT-PRESS"),
        station: "dereth-testkit::dat::inventory::equip::scenario_a_press_on_a_slot_selects_examines_or_uses_the_thing_in_that_slot",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pack.a-request-the-player-made-ghosts-its-own-item-at-once",
        says: "The moment the player asks for an item to be moved, that item alone is greyed in \
               the pack, with nothing else about the world having changed; the shard answering \
               ungreys it. It is the immediate feedback on a drag, and it is not waiting for the \
               round trip.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O524-WAITING"),
        station: "dereth-testkit::dat::inventory::scenario_a_move_request_ghosts_its_own_item_at_once",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pack.a-slot-is-redrawn-when-anything-it-draws-about-its-item-changes",
        says: "A slot in the pack is redrawn whenever anything it draws about the item in it \
               changes -- its name, its plural name, how many there are -- each one on its own \
               with every id, every capacity and every other fact held still, and the slot beside \
               it keeps what it had. An unchanged frame redraws nothing, and an item arriving \
               still does.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O566"),
        station: "dereth-testkit::dat::inventory::scenario_a_pack_slot_follows_every_word_it_draws",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pack.a-slot-says-on-hover-how-many-there-are-and-only-when-there-are-several",
        says: "Hovering a slot holding several of something says how many there are and what they are \
               called, in the plural where the shard gave one; a slot holding one of something says only \
               its name, with no count in front of it. An empty slot says nothing at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O48-TOOLTIP"),
        station: "dereth-testkit::dat::inventory::slots::scenario_a_slot_says_how_many_there_are_on_hover",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pack.a-thing-that-reaches-the-pack-before-it-is-described-draws-its-picture-the-moment-the-description-lands",
        says: "A thing that is put in the pack before the shard has described it -- an award handed \
               over, or anything the character was already carrying at login -- sits in its slot with \
               no picture until the description arrives, and draws the very picture that description \
               names on the frame it lands, rather than waiting for something else in the pack to \
               move. The picture it names is real art the game ships.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O419-AWARD-ICON"),
        station: "dereth-testkit::dat::inventory::icons::scenario_an_awarded_thing_draws_its_picture_when_its_description_lands",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pack.clicking-a-side-pack-refills-the-grid-in-the-same-frame",
        says: "Clicking one of the packs in the strip fills the grid with that pack's contents in the \
               same frame as the click, and clicking the player's own row above the strip fills it with \
               what the player himself is carrying again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-DELIVERY-PACKS"),
        station: "dereth-testkit::dat::inventory::delivery::scenario_clicking_a_side_pack_refills_the_grid_in_the_same_frame",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pack.clicking-a-side-pack-shows-what-is-inside-it-and-moves-the-frame-onto-it",
        says: "Clicking a side pack fills the grid with what is inside that pack and moves the open \
               frame onto its slot and off whatever wore it before; a pack whose contents the client \
               does not know yet opens on an empty grid rather than on the previous pack's things.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O164-OPEN-PACK"),
        station: "dereth-testkit::dat::inventory::clicks::scenario_clicking_a_side_pack_shows_what_is_inside_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pack.describing-a-thing-the-client-already-has-leaves-the-pack-in-the-order-it-was-in",
        says: "The shard describing again a thing the client already has leaves the pack exactly as \
               it found it: every thing keeps the place it had, in the same order, and nothing is \
               added twice. So asking the shard to send everything over again would put no picture \
               right that was not right already.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O419-RESEND"),
        station: "dereth-testkit::dat::inventory::icons::scenario_describing_a_thing_again_leaves_the_pack_in_order",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pack.every-filled-cell-carries-its-item-types-tile",
        says: "Every filled cell of the pack grid is drawn on the background tile for its kind of \
               item -- the player's own entry on the container tile -- and every empty cell \
               carries no tile, in every recorded session.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O151-PACK"),
        station: "dereth-client::gpu::inventory::backpack_grid_tiles::every_filled_cell_carries_its_item_types_tile_and_every_empty_cell_carries_none",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "inventory.pack.every-slot-stays-clickable-whether-or-not-it-has-anything-to-say",
        says: "Every slot of the pack answers the pointer whether it is holding something or empty, \
               and giving a slot something to say on hover neither makes it clickable nor takes that \
               away -- an empty slot, which has nothing to say, is exactly as clickable as a full one, \
               and stays a place something can be dropped.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O48-HITTEST"),
        station: "dereth-testkit::dat::inventory::slots::scenario_every_slot_stays_clickable_with_or_without_a_tooltip",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pack.no-number-is-painted-on-a-pack-icon-and-a-shop-row-still-paints-one",
        says: "No number is painted on the picture of anything in the pack, on the figure or on the \
               shortcut bar, however many of the thing there are -- the count reaches the player on \
               hover instead. The same slot used for a shop's stock does paint the amount the shop is \
               offering, and hides it again when there is none.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O48-QUANTITY"),
        station: "dereth-testkit::dat::inventory::slots::scenario_no_number_is_painted_on_a_pack_icon",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pack.the-bar-and-wheel-scroll-the-grid-and-a-scrolled-hit-resolves-the-shown-item",
        says: "A full pack's item grid scrolls one row for each press of its scrollbar's arrow and \
               each turn of the mouse wheel, and a press on the scrolled grid finds the item \
               actually drawn under the pointer. The scroll survives the pack being refilled or \
               hidden and shown again, and opening a different pack puts the grid back at the top.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-ITEMLIST-SCROLL-PACK"),
        station: "dereth-ui-screens::dat::inventory::item_list_scroll::inventory_bar_and_wheel_move_slots_and_the_scrolled_hit_resolves_the_displayed_item",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pack.the-grid-draws-as-many-slots-as-the-container-can-hold",
        says: "How many empty slots the pack grid draws follows how much the open container can \
               hold, read for itself rather than inferred from anything drawn in it, so a \
               capacity change with every item and every picture held identical still widens the \
               grid on screen.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O524-CAPACITY"),
        station: "dereth-testkit::dat::inventory::scenario_the_grid_follows_the_containers_own_capacity",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pack.the-pack-that-is-open-is-the-one-wearing-the-frame",
        says: "The pack the player has open is the one wearing the frame on its slot, and no other slot \
               wears one. Before any pack is opened the frame is on the character's own picture, which \
               is what the main pack is, and opening a side pack moves it there.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O128-OPEN-FRAME"),
        station: "dereth-testkit::dat::inventory::overlays::scenario_the_open_pack_is_the_one_wearing_the_frame",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pack.the-side-strip-draws-as-many-slots-as-the-player-can-carry-packs",
        says: "How many slots the side-pack strip draws follows how many packs the player can \
               carry, which is a different number about a different thing from the grid's, so one \
               of them moving widens its own widget and leaves the other where it was.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O524-CONTAINERS"),
        station: "dereth-testkit::dat::inventory::scenario_the_side_strip_follows_the_players_own_capacity",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.panel.the-backpack-and-doll-hold-the-captures-items",
        says: "The pack grid holds the loose items the shard's player description listed, in its \
               order, the side-pack strip holds the packs it listed, and the main-pack strip holds \
               the player.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-PANELS-PANEL"),
        station: "dereth-client::gpu::panels::inventory_and_tabs::the_backpack_holds_the_items_the_captures_own_0x0013_named",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "inventory.panel.the-backpack-and-item-grid-sections-hold-the-three-lists-and-are-never-moved",
        says: "The inventory page's three item lists, the player's own pack cell and the strip of \
               side packs in the backpack section and the main item grid in the items section, are \
               the lists the client fills, each inside the section that owns it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1177-PANEL"),
        station: "dereth-ui-screens::dat::inventory::inventory_sub_panels::the_three_lists_this_build_binds_are_the_three_retail_reaches_through_the_handles",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pending-row.a-destination-that-is-full-draws-none-and-the-move-spills-to-a-side-pack",
        says: "Dropping something into a pack that is already full draws no waiting row at the \
               cell the pointer was over, and the move still goes out -- naming the side pack it \
               spilled into. Whether to draw the row and where the thing goes are two separate \
               questions and only the first of them asks whether that cell could have held it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-50D-FULL"),
        station: "dereth-testkit::dat::inventory::scenario_a_full_destination_draws_no_row_and_the_move_spills_to_a_side_pack",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pending-row.a-destination-with-room-draws-one-grey-in-the-very-cell-aimed-at",
        says: "The same drop into a pack with room draws exactly one waiting row, greyed, in the \
               very cell the pointer was over, before the shard has answered anything.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-50D-ROOM"),
        station: "dereth-testkit::dat::inventory::scenario_a_destination_with_room_draws_one_grey_row_in_the_cell_aimed_at",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pending-row.a-drop-aimed-at-a-side-pack-draws-none-and-still-sends-the-move",
        says: "A drop onto a side pack on the strip sends the move and draws no waiting row \
               anywhere. The row belongs to the list that caught the drop and is drawn only when \
               the thing is going into that list's own container, and a pack's contents are not \
               what the strip is showing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-50C-SIDE-PACK"),
        station: "dereth-testkit::dat::inventory::scenario_a_drop_aimed_at_a_side_pack_draws_no_waiting_row",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pending-row.a-drop-the-client-refuses-for-itself-draws-none-either",
        says: "A drop the client refuses for itself -- a row let go on the very cell it already \
               occupies -- draws no waiting row and sends nothing. Every refusal the client makes \
               for itself happens before the row would be drawn, so the row is never something \
               that has to be taken back.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-50C-LOCAL-REFUSAL"),
        station: "dereth-testkit::dat::inventory::scenario_a_drop_the_client_refuses_for_itself_draws_no_waiting_row",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pending-row.a-refusal-takes-the-row-away-and-un-greys-what-never-moved",
        says: "The shard refusing the move takes the waiting row away again and un-greys the \
               thing, which is still in the container it was always in.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-50C-REFUSAL"),
        station: "dereth-testkit::dat::inventory::scenario_a_refusal_takes_the_waiting_row_away_and_un_greys_the_source",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pending-row.a-second-drop-on-a-different-list-meets-the-one-request-hold-instead",
        says: "While a waiting row is up, a second drop on a different list is not refused by the \
               row at all: it meets the client's one-request-at-a-time hold and the player hears \
               that sentence instead. The row belongs to one list, and a client that keyed the \
               refusal on a row existing anywhere would say the wrong sentence here.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-50F-OTHER-LIST"),
        station: "dereth-testkit::dat::inventory::scenario_a_second_drop_on_a_different_list_meets_the_one_request_hold",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pending-row.a-second-drop-on-the-same-list-names-what-is-already-being-placed",
        says: "While a waiting row is up, a second drop on that same list is refused in a \
               sentence naming the thing already being placed, and nothing reaches the shard. \
               The first row is left exactly as it was and the thing that was refused is not \
               left marked.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-50F-SAME-LIST"),
        station: "dereth-testkit::dat::inventory::scenario_a_second_drop_on_the_same_list_names_what_is_already_being_placed",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pending-row.once-the-row-is-gone-the-very-same-drop-is-taken-again",
        says: "The waiting row is a gate and not a latch: the moment the shard's refusal clears \
               it, the very gesture that was refused is accepted, draws its row again and \
               reaches the shard.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-50F-AGAIN"),
        station: "dereth-testkit::dat::inventory::scenario_once_the_waiting_row_is_gone_the_same_drop_is_taken_again",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pending-row.the-row-is-drawn-before-the-shard-answers-and-the-answer-replaces-it",
        says: "On the frame the player lets go, the destination already shows the thing, greyed, \
               at the cell aimed at, while the place it came from stays greyed too. The shard's \
               answer turns that waiting row into the real one rather than adding a second \
               beside it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-50C"),
        station: "dereth-testkit::dat::inventory::scenario_the_waiting_row_is_drawn_at_once_and_the_answer_replaces_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pickup.a-creature-is-refused-by-name-rather-than-picked-up",
        says: "Something alive lying in the road is not a thing that can be picked up. It passes \
               every test the client makes of what a use means and is refused at the layer below \
               them, in a sentence naming creatures.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O110-CREATURE"),
        station: "dereth-testkit::dat::inventory::pickup::scenario_a_creature_is_refused_by_name_rather_than_picked_up",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pickup.a-pile-that-can-merge-is-merged-rather-than-piled-up-twice",
        says: "A pile on the ground that would go into one the player already carries is merged \
               into it instead of being picked up on its own, naming the pile it came from, the \
               pile it goes into and how many. That is why picking coins up does not leave the \
               player carrying two purses of the same thing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O110-MERGE"),
        station: "dereth-testkit::dat::inventory::pickup::scenario_a_pile_that_can_merge_is_merged_rather_than_piled_up_twice",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pickup.a-second-use-inside-the-short-window-sends-nothing",
        says: "A second use of the same thing inside the client's own short window after the \
               first sends nothing at all, so that one double-click is one pickup and not two.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O110-THROTTLE"),
        station: "dereth-testkit::dat::inventory::pickup::scenario_a_second_use_inside_the_short_window_sends_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pickup.a-stale-or-unknown-stow-changes-nothing",
        says: "A stow message no newer than the item's own last position change is refused, so a \
               repeated or re-ordered one cannot take out of the hand an item that has since been \
               wielded again; a stow naming an object the client does not have is counted rather \
               than silently dropped, and a genuinely newer one still takes.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O76-STALE"),
        station: "dereth-testkit::dat::inventory::scenario_a_stale_or_unknown_stow_changes_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pickup.an-item-the-shard-says-was-stowed-leaves-the-hand-and-the-world",
        says: "When the shard says an object has been picked up, the client stops drawing it: an \
               item that was in somebody's hand is no longer held, an item that was lying on the \
               ground is no longer anywhere in the map, and in both cases the object itself is \
               kept rather than destroyed and whoever was holding it is left alone.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O76"),
        station: "dereth-testkit::dat::inventory::scenario_a_stowed_item_leaves_the_hand_and_the_world",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pickup.every-recorded-one-is-reproduced-with-the-recordings-own-ids",
        says: "For every pickup the recordings hold -- which of the recorded moves those are \
               is the replayed world's answer and not a rule written down -- this client asks \
               for the same thing, into the same container, as the recorded client did; and the \
               shard's own answer, byte for byte as it was recorded, puts that thing into the \
               container the answer names and takes the grey off it. Picking the wrong thing up \
               is invisible and acquisitive, so not one of those ids is written down here.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O110-RECORDED"),
        station: "dereth-testkit::dat::inventory::pickup::scenario_every_recorded_pickup_is_reproduced_with_the_recordings_own_ids",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pickup.something-lying-loose-is-picked-up-and-anything-else-is-used",
        says: "A use on something lying loose in the world picks it up. A use on the same thing \
               held in somebody else's hand, nailed down, already inside something, or a \
               container in its own right is a plain use instead -- unless what it is inside is \
               the container the player has open, which is how loot is taken off a corpse.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O110-GATES"),
        station: "dereth-testkit::dat::inventory::pickup::scenario_something_lying_loose_is_picked_up_and_anything_else_is_used",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pickup.the-gesture-asks-for-one-move-and-predicts-nothing",
        says: "Using something lying in the road asks the shard to move it into the player's own \
               pack, once, naming that thing and that player -- there is no pickup message of its \
               own. Until the shard answers the thing is still lying where it was, greyed, and \
               the pack is empty: the client predicts nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O110-REQUEST"),
        station: "dereth-testkit::dat::inventory::pickup::scenario_the_gesture_asks_for_one_move_and_predicts_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pickup.the-request-reaches-the-wire-naming-the-thing-and-the-player",
        says: "The pickup does not stop at the handler: it goes out on the wire as a game action \
               whose body names the thing and the player. A handler whose request nothing routes \
               would leave the player watching an object that never moves.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O110-WIRE"),
        station: "dereth-testkit::dat::inventory::pickup::scenario_the_request_reaches_the_wire_naming_the_thing_and_the_player",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pickup.the-shipped-key-carries-the-picked-stacks-own-count-into-the-merge",
        says: "When what is picked is a stack and the player already carries a compatible one, \
               the pick-up key merges them and carries the count the picking itself worked out. \
               That count is seeded when the thing is picked, before the key is pressed; a \
               client that started from nothing would send a zero and the player would watch the \
               stack not move.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F22-MERGE"),
        station: "dereth-testkit::dat::inventory::scenario_the_shipped_key_carries_the_picked_stacks_own_count_into_the_merge",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pickup.the-shipped-key-puts-what-is-picked-into-the-players-own-pack",
        says: "Pressing the key the shipped keymap binds to picking things up puts what is \
               picked into the player's own pack. The key resolved and produced its action all \
               along; what was missing was anything that answered it, so nothing went out for \
               any object, ever.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F22-PICKUP"),
        station: "dereth-testkit::dat::inventory::scenario_the_shipped_key_puts_what_is_picked_into_the_players_own_pack",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pickup.the-shipped-key-with-nothing-picked-sends-nothing",
        says: "With nothing picked the pick-up key is not answered at all and nothing goes out. \
               Without that, a client that picked up whatever it found -- including nothing -- \
               would satisfy every other measurement of the key.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F22-NO-SELECTION"),
        station: "dereth-testkit::dat::inventory::scenario_the_shipped_key_with_nothing_picked_sends_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.pickup.while-another-move-is-outstanding-it-is-refused-in-words",
        says: "With one move already waiting on the shard, a pickup is refused in the client's \
               own words -- the player is told he can only move or use one thing at a time -- and \
               nothing at all is asked of the shard.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O110-BUSY"),
        station: "dereth-testkit::dat::inventory::pickup::scenario_a_pickup_while_another_move_is_outstanding_is_refused_in_words",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.place.a-container-with-no-room-refuses-in-the-clients-own-words",
        says: "A thing that says it is a container but has room for nothing refuses a drop in a \
               sentence naming itself, and nothing at all is asked of the shard. Without that \
               answer the drop quietly becomes a reposition in the list the container is sitting \
               in.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O270-NO-ROOM"),
        station: "dereth-testkit::cpu::inventory::scenario_a_container_with_no_room_refuses_in_the_clients_own_words",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "inventory.place.a-dragged-pack-is-looked-up-among-the-packs",
        says: "A player's packs and a player's loose things are two lists, and which one the \
               thing being carried is looked up in is decided by that thing and not by the list \
               it is let go on. A pack carried onto another pack's place is a real move; the same \
               drop with the pack that is already there in hand changes nothing and is refused.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O270-PACK-LIST"),
        station: "dereth-testkit::cpu::inventory::scenario_a_dragged_pack_is_looked_up_among_the_packs",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "inventory.place.a-drop-lands-at-the-place-the-player-aimed-at",
        says: "The move a drop sends asks for the place the pointer was over and not for the head \
               of the list: the head, a slot below the thing's own place, the first empty cell \
               and an empty cell further out than the list has things are four different answers, \
               and the last two are the same because a slot past the end counts as the end.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O270-PLACE"),
        station: "dereth-testkit::cpu::inventory::scenario_a_drop_lands_at_the_place_the_player_aimed_at",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "inventory.place.a-drop-that-would-change-nothing-is-refused-and-takes-no-lock",
        says: "Dropping a thing on its own place, and dropping it on the cell immediately after \
               its own place with the whole stack in hand, both move nothing and are refused \
               outright -- so nothing is sent and the one inventory hold is not taken. With only \
               part of a stack in hand the second of those is a real move and is not refused.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O270-NO-OP"),
        station: "dereth-testkit::cpu::inventory::scenario_a_drop_that_would_change_nothing_is_refused_and_takes_no_lock",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "inventory.place.a-merge-is-answered-before-any-place-is-worked-out",
        says: "Letting one stack go on another of the same kind is a merge, and it is answered \
               before any place in the list is worked out at all: the merge names both stacks and \
               the amount carried, and no reposition is sent as well.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O270-MERGE"),
        station: "dereth-testkit::cpu::inventory::scenario_a_merge_is_answered_before_any_place_is_worked_out",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "inventory.place.a-move-already-asked-for-is-not-overtaken-by-the-use-button",
        says: "A move the player has already asked for keeps its hold on the thing it is about, and the \
               Use button pressed afterwards does not overtake it: the move goes out, nothing is armed, \
               and nothing is used.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-DELIVERY-PREFIX"),
        station: "dereth-testkit::dat::inventory::delivery::scenario_a_move_already_asked_for_is_not_overtaken_by_the_use_button",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.place.a-plain-thing-under-the-pointer-is-a-place-and-a-pack-is-a-destination",
        says: "Letting something go on another plain thing moves it to that thing's place in the \
               list they are both in; letting it go on a pack puts it inside that pack, at the \
               pack's own head. A move aimed at a plain thing is a request no shard can answer, \
               and the hold it would take has no timeout.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O270-DESTINATION"),
        station: "dereth-testkit::cpu::inventory::scenario_a_plain_thing_under_the_pointer_is_a_place_and_a_pack_is_a_destination",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "inventory.place.a-real-drag-across-the-grid-asks-for-the-place-the-player-aimed-at",
        says: "A real pointer drag from one cell of the player's own pack grid to another carries \
               the slot the pointer was over all the way to the move that goes out: the drop \
               names that cell, the thing drawn in it and how many things the list is showing, \
               and the move names the player's pack and the place aimed at less one, because the \
               thing is moving down its own list.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O270-LIVE"),
        station: "dereth-testkit::dat::inventory::scenario_a_real_drag_across_the_grid_asks_for_the_place_the_player_aimed_at",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.place.a-refused-drop-takes-the-grey-back-off-the-icon",
        says: "A drop the client refuses for itself puts back the grey the grid laid on the icon \
               when it was picked up. A refusal sends nothing, so there is nothing on the wire \
               that could ever clear that grey, and it would otherwise survive until something \
               else moved in the pack.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O270-GHOST"),
        station: "dereth-testkit::cpu::inventory::scenario_a_refused_drop_takes_the_grey_back_off_the_icon",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "inventory.place.a-stack-on-a-matching-stack-merges-before-any-target-is-considered",
        says: "A stack let go on a matching stack is merged into it, and that answer is given before \
               anything about what the target is. The target here is a creature as well, so the merge is \
               measured ahead of the gift rather than merely in its absence.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O288-MERGE-FIRST"),
        station: "dereth-testkit::dat::inventory::give::scenario_a_stack_on_a_matching_stack_merges_before_any_target_is_considered",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.place.moving-a-thing-up-its-own-list-does-not-shift-it-one-further",
        says: "The place a move asks for is one less than the slot aimed at only when the thing \
               is moving down its own list: aimed at the head, or at a cell above where it \
               already sits, the place is the slot itself. The same run measures both directions, \
               because an adjustment made every time passes the downward half on its own.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O270-DIRECTION"),
        station: "dereth-testkit::cpu::inventory::scenario_moving_a_thing_up_its_own_list_does_not_shift_it_one_further",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "inventory.place.the-lock-and-the-grey-are-both-released-by-the-shards-own-answer",
        says: "A legal reposition takes the one inventory hold and greys the icon, and the \
               shard's own answer releases both and leaves the thing where the player put it, \
               with nothing further asked of the player. The hold has no timeout, so that answer \
               is the only thing that can release it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O270-LOCK"),
        station: "dereth-testkit::cpu::inventory::scenario_the_lock_and_the_grey_are_both_released_by_the_shards_own_answer",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "inventory.place.the-shards-refusal-lets-go-of-the-one-at-a-time-hold-as-well-as-the-grey",
        says: "The hold the player is put under while the shard has not answered has no timeout \
               at all, so the shard's own word is the only thing that can end it -- and a refusal \
               ends it just as a confirmation does. The next gesture is refused while the hold is \
               on, the refusal arrives, the grey comes off, and the very same gesture then goes \
               out. Until this was wired, the second gesture of a session was refused for ever.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O85-REFUSAL-RELEASES"),
        station: "dereth-testkit::dat::inventory::equip::scenario_the_shards_refusal_lets_go_of_the_one_at_a_time_hold_as_well_as_the_grey",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.refusal.a-container-drop-the-shard-refuses-says-so-in-its-own-words",
        says: "Something the shard refuses to let into a container is refused in the container's \
               own sentence, which is not the one a refused world drop gets. Which one the player \
               reads is decided by what the client recorded itself as having asked for.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F46-CONTAINER"),
        station: "dereth-testkit::dat::inventory::scenario_a_refused_container_drop_says_so_in_its_own_words",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.refusal.a-full-destination-with-nowhere-to-spill-says-so-and-sends-nothing",
        says: "A drop into a container that is full and has nothing to spill into is refused \
               where the player is standing, in a sentence saying that container is completely \
               full, with no waiting row drawn and nothing at all asked of the shard.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-50D-NO-SPILL"),
        station: "dereth-testkit::dat::inventory::scenario_a_full_destination_with_nowhere_to_spill_says_so_and_sends_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.refusal.a-full-pack-is-named-the-way-the-player-would-name-it",
        says: "When nothing the player is carrying has room for a thing, the client says so \
               by name, and the name it uses for the player's own pack is the word a player \
               would use for it rather than the character's name. Any other container keeps \
               its own name, material and all. It is one decision taken twice with one \
               argument flipped, and a client that called every full container a backpack \
               would look correct on half of it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P139-ALIAS"),
        station: "dereth-testkit::dat::inventory::scenario_a_full_pack_is_named_the_way_the_player_would_name_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.refusal.a-pack-on-the-item-grid-is-refused-for-being-the-wrong-kind",
        says: "A pack let go on the pack's own item grid is refused in the grid's own sentence, \
               and the pack is left unmarked. The grid takes things and the strip beside it takes \
               packs, and each says so in its own words.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-50G-CONTAINER"),
        station: "dereth-testkit::dat::inventory::scenario_a_pack_on_the_item_grid_is_refused_for_its_kind",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.refusal.a-plain-thing-on-an-empty-pack-strip-slot-is-refused-for-being-the-wrong-kind",
        says: "A plain thing let go on an empty slot of the side-pack strip is refused for being \
               the wrong kind of thing for that list, nothing is sent, and the thing it came from \
               is left unmarked. The same thing let go on a pack in that strip is the ordinary \
               move into that pack and still works.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-50G-ITEM"),
        station: "dereth-testkit::dat::inventory::scenario_a_plain_thing_on_an_empty_pack_strip_slot_is_refused_for_its_kind",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.refusal.a-world-drop-the-shard-refuses-names-the-thing-as-the-player-sees-it",
        says: "When the shard refuses something dropped into the world, the player is told so in \
               a sentence naming the thing the way they see it named, the material it is made of \
               included. Before this the player saw nothing at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F46-WORLD-DROP"),
        station: "dereth-testkit::dat::inventory::scenario_a_refused_world_drop_names_the_thing_as_the_player_sees_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.refusal.answers-for-the-item-the-client-asked-about",
        says: "When the shard refuses an inventory request, the client answers for the item it is \
               waiting on rather than the one the refusal names, so the refusal line names that item, \
               its ghosted look is cleared and its lock released. With nothing pending the refusal \
               keeps its own item, an item the client has never seen makes nothing happen at all and \
               leaves a held lock held, and an interrupted auto-equip is abandoned for the item the \
               client was waiting on.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O516"),
        station: "dereth-testkit::cpu::inventory::scenario_a_refusal_answers_for_the_item_we_asked_about",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "inventory.refusal.dragging-yourself-into-your-own-pack-is-refused-by-the-list-itself",
        says: "Dragging the player's own row into the player's own pack is refused by the list \
               itself, in the list's own sentence, before anything is sent or drawn. A second \
               rule further down would also refuse it in different words, and what the player \
               hears is the list's, because the destination is the thing being dropped.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-50G-SELF"),
        station: "dereth-testkit::dat::inventory::scenario_dragging_yourself_into_your_own_pack_is_refused_by_the_list_itself",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.request-lock.an-attempt-failed-always-releases-a-held-lock-whatever-object-it-names",
        says: "When the shard says an attempt failed while an inventory request is outstanding, \
               the client takes the failure to be about the object it is waiting on, whatever \
               object the message names, so the wait always ends; with nothing outstanding the \
               client stays idle.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O472-REQUEST-LOCK"),
        station: "dereth-client-model::cpu::inventory::request_lock_guard::the_00a0_arm_never_produces_the_case_the_guard_rejects",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "inventory.selection.a-click-on-a-thing-in-the-pack-picks-it",
        says: "A click on a thing in the pack picks it: the ring goes up on the slot that was \
               clicked, and the thing the client counts as picked is the one under the pointer. A \
               click on an empty slot picks nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O164-CLICK-SELECT"),
        station: "dereth-testkit::dat::inventory::clicks::scenario_a_click_on_a_thing_in_the_pack_picks_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.selection.a-list-rebuilt-under-an-unchanged-choice-puts-the-ring-back",
        says: "A list rebuilt while the player's choice has not changed -- which is what opening a \
               different pack does, and what anything moving in the pack does -- puts the ring back on \
               the thing that is chosen, instead of leaving it bare until the player picks something \
               else.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O128-RING-REBUILD"),
        station: "dereth-testkit::dat::inventory::overlays::scenario_a_list_rebuilt_under_an_unchanged_choice_puts_the_ring_back",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.selection.a-list-that-allows-one-choice-leaves-one-ring-when-a-thing-listed-twice-is-clicked",
        says: "On a list that allows only one thing to be chosen, clicking something that the list \
               happens to show twice leaves exactly one ring -- on the slot that was clicked -- and \
               the other copies of it stop answering a click at all. Only the slots that are really \
               filled are walked.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O164-SINGLE-SELECTION"),
        station: "dereth-testkit::dat::inventory::clicks::scenario_a_single_choice_list_leaves_one_ring_on_the_slot_that_was_clicked",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.selection.a-slot-that-cannot-be-picked-can-still-lose-its-ring",
        says: "A slot that has been made unpickable can still have its ring taken off it, even \
               though it cannot be given one -- which is what lets the duplicate copies of a thing \
               lose their rings instead of keeping them for ever.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O128-SELECTABLE-GATE"),
        station: "dereth-testkit::dat::inventory::clicks::scenario_a_slot_that_cannot_be_picked_can_still_lose_its_ring",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.selection.picking-a-thing-rings-it-everywhere-its-picture-is-and-unrings-the-last-one",
        says: "Picking a thing rings it in every list its picture appears in at once -- the pack \
               grid, the side-pack strip, the figure and the shortcut bar -- and takes the ring off \
               whatever was picked before, everywhere, so the pack never fills up with rings.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O128-SELECTION-EDGE"),
        station: "dereth-testkit::dat::inventory::overlays::scenario_picking_a_thing_rings_it_everywhere_and_unrings_the_last",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.shortcut-bar.a-kit-key-then-the-main-pack-key-uses-the-kit-on-the-player",
        says: "With a healing kit on one tile and the player's own main pack on another, the kit's \
               key asks for a target and leaves the pointer armed without sending anything, and \
               the main pack's key then uses the kit on the player and disarms the pointer. It \
               holds however the player reached the game, including straight from making a new \
               character: each key press is heard once.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-SHORTCUT-KIT-SELF"),
        station: "dereth-testkit::dat::inventory::scenario_a_kit_key_then_the_main_pack_key_uses_the_kit_on_the_player",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.shortcut-bar.a-number-key-pressed-with-a-cursor-armed-finishes-that-gesture",
        says: "A tile's number key pressed while the pointer is already armed for something \
               finishes that gesture with the thing in the tile instead of starting a new one, \
               and disarms the pointer afterwards so the next press is an ordinary use again. \
               That is what makes two keys in a row act one thing on another rather than opening \
               a pack.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F43-ARMED"),
        station: "dereth-testkit::dat::inventory::scenario_a_number_key_pressed_with_a_cursor_armed_finishes_that_gesture",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.shortcut-bar.a-pack-can-be-put-on-a-tile-like-anything-else",
        says: "A pack carried onto a tile of the shortcut bar goes into it by exactly the \
               gesture anything else does, and the shard is told which tile holds it. Nothing \
               about being a container refuses the tile.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F43-PACK"),
        station: "dereth-testkit::dat::inventory::scenario_a_pack_can_be_put_on_a_tile_like_anything_else",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.shortcut-bar.a-thing-dropped-on-a-tile-fills-it-and-tells-the-shard-once",
        says: "A thing carried onto a tile of the shortcut bar goes into that tile, the tile \
               stops drawing the empty numbered plate it is shipped with and draws the thing, \
               and the shard is told exactly once which tile now holds it -- two handlers see \
               the release, and a second telling would put the shortcut in twice.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F43-DROP"),
        station: "dereth-testkit::dat::inventory::scenario_a_thing_dropped_on_a_tile_fills_it_and_tells_the_shard_once",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.shortcut-bar.the-number-key-of-a-filled-tile-uses-what-is-in-it",
        says: "Pressing the key the shipped keymap binds to a filled tile uses the thing in that \
               tile. The whole chain is the claim -- the key, the binding, the bar's own listener \
               and the use -- and which way the use goes belongs to the thing being used.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F43-KEY"),
        station: "dereth-testkit::dat::inventory::scenario_the_number_key_of_a_filled_tile_uses_what_is_in_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.shortcut-bar.the-other-key-for-a-tile-selects-what-is-in-it-instead-of-using-it",
        says: "Each tile of the shortcut bar answers two keys and they do different things: one \
               uses what is in the tile, and the other selects it, so that the next thing the \
               player does acts on it. The whole chain is the claim -- the key, the broadcast \
               every listener on the screen hears, the bar's own listener and the selection -- \
               and the thing in the tile comes from the description the shard sends at login.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O85-SECONDARY-KEY"),
        station: "dereth-testkit::dat::inventory::equip::scenario_the_other_key_for_a_tile_selects_what_is_in_it_instead_of_using_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.split.a-drag-that-ends-nowhere-and-a-refusal-both-free-the-stack",
        says: "A drag of a stack let go where there is nothing asks the shard for nothing and leaves \
               the stack unmarked and the player free; once a split really has gone out, a \
               failed drop somewhere else does not free him, and the shard's refusal does -- \
               leaving the stack at the count it had and the quantity he chose still chosen, \
               so the next try goes out just the same.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-SPLIT-REFUSAL"),
        station: "dereth-testkit::dat::inventory::split::scenario_a_drag_that_ends_nowhere_and_a_refusal_both_free_the_stack",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.split.a-leading-zero-and-a-number-too-big-are-read-the-way-the-client-reads-them",
        says: "A quantity written with a leading zero is read as the client reads it rather than as a \
               plain decimal, and what is showing in the box is left spelled as it was typed; \
               a number too big for the client to hold comes back as the whole stack rather \
               than wrapping round to one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-SPLIT-PARSE"),
        station: "dereth-testkit::dat::inventory::split::scenario_a_leading_zero_and_a_number_too_big_are_read_the_way_the_client_reads_them",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.split.a-quantity-larger-than-the-stack-or-none-at-all-is-corrected-in-the-box",
        says: "Asking for more of a stack than there are is corrected in the quantity box itself to the \
               whole stack, and asking for none of it is corrected to one; the correction is \
               not a latch, and the two land on different requests, because taking all of a \
               stack moves the stack and taking one of it splits it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O373-CLAMP"),
        station: "dereth-testkit::dat::inventory::split::scenario_a_quantity_larger_than_the_stack_or_none_at_all_is_corrected_in_the_box",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.split.a-typed-quantity-is-taken-when-the-caret-leaves-the-box",
        says: "A quantity typed into the box is taken when the caret leaves it and not while it is being \
               typed, and pressing a tile of the pack does not take the caret out of the \
               box; once it is taken, the drag that follows carries it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O373-BOX"),
        station: "dereth-testkit::dat::inventory::split::scenario_a_typed_quantity_is_taken_when_the_caret_leaves_the_box",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.split.dragging-a-whole-stack-is-a-move-and-not-a-split",
        says: "Dragging every last one of a stack into a pack moves the stack itself rather than \
               splitting a new one off it, so the quantity the player chose decides which request \
               is sent and is not merely decoration on one request.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O374-WHOLE"),
        station: "dereth-testkit::cpu::inventory::scenario_a_whole_stack_drop_is_a_move",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "inventory.split.escape-in-the-box-changes-nothing-and-cancel-puts-the-taken-quantity-back",
        says: "Pressing escape while typing a quantity is eaten by the box: nothing is sent, the caret \
               stays where it is, and what is showing is left as it was. The panel's own \
               cancel is a different thing and puts back the quantity that was last handed \
               over, not the whole stack, and the drag that follows carries that one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-SPLIT-ESCAPE"),
        station: "dereth-testkit::dat::inventory::split::scenario_escape_in_the_box_changes_nothing_and_cancel_puts_the_taken_quantity_back",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.split.picking-a-different-thing-sets-the-quantity-back-to-all-of-it",
        says: "Picking a different thing sets the quantity back to the whole of the new thing, both in \
               the box and where the drag reads it, so a stack picked and dragged untouched \
               moves whole rather than splitting off whatever the last stack was set to; \
               picking something that is not a stack at all puts it back to one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O373-RESEED"),
        station: "dereth-testkit::dat::inventory::split::scenario_picking_a_different_thing_sets_the_quantity_back_to_all_of_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.split.picking-a-stack-sets-the-quantity-to-that-stacks-own-count",
        says: "Picking a stack opens the quantity at the whole of that stack and makes that stack's own \
               count the most the box will accept, so a number inside it stands and one \
               above it comes back as the stack rather than as one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O117-SEEDED"),
        station: "dereth-testkit::dat::inventory::split::scenario_picking_a_stack_sets_the_quantity_to_that_stacks_own_count",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.split.starting-to-drag-the-stack-takes-the-quantity-in-the-box",
        says: "Lifting the stack off its tile takes the quantity showing in the box there and then, with \
               no return pressed and nothing else clicked, and lets the typing go; the drop \
               that follows asks the shard for exactly that many.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-SPLIT-DRAG-COMMIT"),
        station: "dereth-testkit::dat::inventory::split::scenario_starting_to_drag_the_stack_takes_the_quantity_in_the_box",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.split.the-quantity-the-player-chose-is-not-taken-back-by-the-frames-after-it",
        says: "The quantity the player chose stays his: the frames that follow do not quietly put it \
               back to the whole stack. Picking something else does set it again, both ways \
               round, and putting the selection down takes the controls away while leaving \
               the quantity where the last stack left it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O117-SETTLE"),
        station: "dereth-testkit::dat::inventory::split::scenario_the_quantity_the_player_chose_is_not_taken_back_by_the_frames_after_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.split.the-slider-chooses-the-quantity-on-the-drag-itself",
        says: "Dragging the slider chooses the quantity on the drag itself, with no second gesture \
               needed: the box follows it, and the drag of the stack that comes next asks the \
               shard for exactly that many.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O373-SLIDER"),
        station: "dereth-testkit::dat::inventory::split::scenario_the_slider_chooses_the_quantity_on_the_drag_itself",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.split.the-sliders-two-ends-are-one-of-them-and-all-of-them",
        says: "Pushing the slider to one end of its bar chooses one of the stack and pushing it to the \
               other chooses all of it, and the two leave by different roads: one of them is \
               a split into the place aimed at, all of them is a move of the stack itself.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-SPLIT-ENDPOINTS"),
        station: "dereth-testkit::dat::inventory::split::scenario_the_sliders_two_ends_are_one_of_them_and_all_of_them",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.split.the-source-stack-shows-its-new-count-once-the-shard-answers",
        says: "Splitting part of a stack into a pack leaves the stack it came from ghosted and at \
               its old count until the shard says what is left, and predicts nothing in the \
               meantime; the shard's answer puts the stack at its new count and worth, clears the \
               ghosting and lets go of the player's inventory lock. The count and the worth are \
               two separate numbers and each is taken from where the shard put it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O374"),
        station: "dereth-testkit::cpu::inventory::scenario_a_split_leaves_the_source_at_its_new_count",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "inventory.split.the-split-controls-are-shown-for-a-stack-and-for-nothing-else",
        says: "The quantity box and its slider are shown when a stack is picked and at no other time -- \
               not for a creature, not for a thing that stacks by kind but is on its own, and \
               not before anything is picked; the strip they sit in stays where it is \
               throughout, and picking a stack again brings them back.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O117-GATE"),
        station: "dereth-testkit::dat::inventory::split::scenario_the_split_controls_are_shown_for_a_stack_and_for_nothing_else",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.split.the-split-key-puts-the-caret-in-the-quantity-and-does-nothing-else",
        says: "The key bound to splitting a stack puts the caret in the quantity box with the whole \
               quantity picked out, so the next digit replaces it; with nothing picked, with \
               one of a thing picked, or with something that is not a stack picked, the key \
               does nothing at all and is still used up rather than left to fall through.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O373-SPLIT-KEY"),
        station: "dereth-testkit::dat::inventory::split::scenario_the_split_key_puts_the_caret_in_the_quantity_and_does_nothing_else",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.split.two-splits-in-a-row-each-go-out-and-the-shards-answers-follow-them",
        says: "Two splits one after the other each go out as the player asked, whether the quantity was \
               handed over by pressing return or by putting the caret somewhere else: while \
               each is waiting the stack is marked and still says the count it said before, \
               the shard's answers put it at its new count, free the player and leave him \
               looking at the stack that was split off, and nothing is left marked at the end.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-SPLIT-JOURNEY"),
        station: "dereth-testkit::dat::inventory::split::scenario_two_splits_in_a_row_each_go_out_and_the_shards_answers_follow_them",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.split.typing-a-number-into-the-box-and-leaving-it-sets-how-many-a-drag-moves",
        says: "Typing a number into the quantity box on the toolbar and then leaving it sets how \
               many of a stack the next drag moves, at both ends: what the box itself keeps and \
               what the part of the client a drop reads keeps. Over the maximum comes back to the \
               maximum and is written into the box, and under one is lifted to one rather than \
               left as a move of nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O85-QUANTITY"),
        station: "dereth-testkit::dat::inventory::equip::scenario_typing_a_number_into_the_box_and_leaving_it_sets_how_many_a_drag_moves",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.split.until-that-answer-arrives-every-later-request-is-refused",
        says: "Between a split and the shard saying what is left of the stack, the player is held \
               to one request at a time: a second split is refused in the client's own words and \
               nothing goes out for it. The reply that places the newly split stack is about that \
               new stack and does not let the player go; the one about the stack it came off does, \
               and the second split then goes through.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O374-LOCK"),
        station: "dereth-testkit::cpu::inventory::scenario_a_second_split_waits_for_the_first_answer",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "inventory.split.while-the-number-box-holds-the-keyboard-a-tiles-key-types-instead-of-firing",
        says: "While the quantity box has the keyboard, every action a key would otherwise fire \
               is swallowed: pressing four types a four instead of firing the fourth tile of the \
               shortcut bar. Asking the box to give the keyboard back puts the number the player \
               last settled on into it, not the maximum, and lets the focus go, so the next thing \
               typed is a hotkey again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O85-FOCUS-GATE"),
        station: "dereth-testkit::dat::inventory::equip::scenario_while_the_number_box_holds_the_keyboard_a_tiles_key_types_instead_of_firing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.stack-size.a-stale-or-oversized-update-is-ignored",
        says: "A stack's count only moves forward: an update older than one already applied is \
               dropped, an update ordering behind a quality change to that same count is dropped \
               with it, a count larger than the stack can hold is refused outright, and one for \
               an object the client does not have changes nothing. The next one in order still \
               applies, so none of this is the client simply refusing everything.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O374-STAMPER"),
        station: "dereth-testkit::cpu::inventory::scenario_a_stale_or_oversized_stack_count_is_ignored",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "inventory.targeted-use.the-source-survives-until-use-done",
        says: "Once an oil is armed, picking something else does not drop it: the next target \
               sends the recorded use-with-target request naming the oil, the player stays busy \
               and the oil stays in the pack until the shard's use-done arrives, and only then \
               does a second oil and target go out exactly as recorded.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-TARGETED-USE-TARGETED-USE-SOURCE"),
        station: "dereth-client::gpu::inventory::targeted_use::retained_source_survives_selection_and_recorded_use_done_allows_a_second_request",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "inventory.targeted-use.using-an-oil-arms-target-mode-without-sending",
        says: "Using an item that has to be applied to something else, such as an oil, puts the \
               pointer into its own use-on-a-target mode rather than the ordinary use mode; \
               nothing is sent to the shard and the player is not made busy until a target is \
               picked.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-TARGETED-USE-TARGETED-USE"),
        station: "dereth-client::gpu::inventory::targeted_use::recorded_oil_use_arms_the_distinct_targeted_mode_without_sending",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "inventory.unblock.a-blocker-that-went-somewhere-else-does-not-put-the-new-thing-on",
        says: "The thing being moved out of the way can land somewhere other than where the \
               client asked for it, and then the new thing is not put on at all: the client drops \
               what it was doing rather than acting on a move it did not make. A wholly unrelated \
               thing moving while one of these is in the air is ignored, because otherwise every \
               pickup would cancel it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O125-WRONG-DESTINATION"),
        station: "dereth-testkit::dat::inventory::equip::scenario_a_blocker_that_went_somewhere_else_does_not_put_the_new_thing_on",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.unblock.a-refused-move-ungreys-the-thing-that-was-waiting-for-it",
        says: "The shard can refuse to move the thing that is in the way, and then there is \
               nothing to come back for: the client forgets what it was doing, takes the grey off \
               the thing the player let go -- which never had a request of its own -- and asks \
               for nothing further.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O125-REFUSED"),
        station: "dereth-testkit::dat::inventory::equip::scenario_a_refused_move_ungreys_the_thing_that_was_waiting_for_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.unblock.a-ring-let-go-on-the-full-hand-takes-that-ring-off-rather-than-the-other",
        says: "A ring let go on the hand that is already wearing one takes that ring off rather \
               than quietly going on the free hand: where the player aimed is where the ring \
               goes, and once the shard has put the old ring in the pack the new one goes on that \
               same hand. Getting this wrong looks like the feature working -- the ring goes on, \
               silently, in the wrong place.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O125-AIMED-SIDE"),
        station: "dereth-testkit::dat::inventory::equip::scenario_a_ring_let_go_on_the_full_hand_takes_that_ring_off_rather_than_the_other",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.unblock.adding-to-a-stack-on-the-body-that-cannot-take-more-says-so-and-moves-nothing",
        says: "Letting ammunition go on the quiver when the quiver already holds the same kind \
               and cannot take any more is not a blocked equip at all: the client says it cannot \
               wield more of them, naming them, and moves nothing -- the stack on the body is not \
               taken off to make room for the stack in the hand, because they are the same thing. \
               A different kind of thing in that place is taken off in the ordinary way.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O125-SAME-KIND"),
        station: "dereth-testkit::dat::inventory::equip::scenario_adding_to_a_stack_on_the_body_that_cannot_take_more_says_so_and_moves_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.unblock.every-recorded-one-whose-place-is-taken-moves-the-blocker-or-is-refused-by-name",
        says: "Every thing the recordings record being put on, let go on its own place with that \
               place already filled, gets one of exactly two answers, and which one depends only \
               on where it was let go. Outside the clothes the client asks the shard to move the \
               thing in the way into the player's own pack and leaves the new thing greyed; among \
               the clothes there is no such move at all and the drop is refused by name, saying \
               which piece has to come off first.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O125-CORPUS-BLOCKED"),
        station: "dereth-testkit::dat::inventory::equip::scenario_every_recorded_one_whose_place_is_taken_moves_the_blocker_or_is_refused_by_name",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.unblock.it-is-never-begun-twice-and-a-thing-is-not-taken-off-to-put-itself-on",
        says: "Making room for something is a thing the client is either doing or not: a second \
               drop while one is already in the air leaves it doing exactly one, never two, so \
               nothing piles up behind a shard that is slow to answer. And the one gesture that \
               would otherwise go round for ever -- letting go, on the very place it is already \
               in, the thing that is in the way -- is refused outright, with nothing asked, \
               nothing remembered and the grey taken off.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O125-LOOP-GUARD"),
        station: "dereth-testkit::dat::inventory::equip::scenario_it_is_never_begun_twice_and_a_thing_is_not_taken_off_to_put_itself_on",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.unblock.the-figure-says-only-what-it-is-moving-where-the-sort-names-every-full-place",
        says: "One and the same blocked equip is spoken about in two entirely different ways \
               depending on the gesture, and both are right. Let go on the figure, the player is \
               told one thing -- which piece is being moved to the pack to make room -- and none \
               of the sentences about already wearing one of those. Double-clicked, where the \
               client finds a place rather than making one, the player gets those sentences \
               instead, one for each full place, in the order the client looked at them.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O125-WHICH-LINES"),
        station: "dereth-testkit::dat::inventory::equip::scenario_the_figure_says_only_what_it_is_moving_where_the_sort_names_every_full_place",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.unblock.the-thing-in-the-way-goes-to-the-pack-and-the-new-one-goes-on-after",
        says: "Letting something go on a place another thing is in asks the shard to put that \
               other thing into the player's pack -- nothing else -- and says so by name. The new \
               thing stays greyed, because nothing has been asked about it yet. When the shard \
               says the old thing landed in the pack, and only then, the client asks for the new \
               one to be put on, at the place the old one has just left.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O125-ROUND-TRIP"),
        station: "dereth-testkit::dat::inventory::equip::scenario_the_thing_in_the_way_goes_to_the_pack_and_the_new_one_goes_on_after",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.unblock.what-comes-off-is-the-last-full-place-the-walk-looked-at",
        says: "When the thing being put on could go in two places and both are full, the one that \
               comes off is the thing in the place the client looked at last rather than the \
               first, and the line the player reads names that same thing. The other one stays \
               where it is and is not mentioned.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O125-LAST-BLOCKER"),
        station: "dereth-testkit::dat::inventory::equip::scenario_what_comes_off_is_the_last_full_place_the_walk_looked_at",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.wear.dropping-a-worn-thing-back-on-its-own-slot-says-it-is-already-worn",
        says: "Dragging something you are wearing off its place on the body and dropping it \
               straight back says that it is already being worn, and the sentence names the \
               thing the way the player sees it named, material and all. Nothing is sent \
               and the thing is not left greyed. The hint on the way over says yes, because \
               there is no exception for the place a thing came from: the refusal belongs \
               to the drop and not to the hover.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P130-WORN"),
        station: "dereth-testkit::dat::inventory::scenario_dropping_a_worn_thing_back_on_its_own_slot_says_so",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.wear.what-the-shard-says-is-worn-is-remembered-and-taking-it-off-forgets-only-that",
        says: "What the client believes the player is wearing follows the shard's own word for \
               it, and the two kinds of thing are remembered differently: something wielded takes \
               the one place it went into, and something worn takes the whole of its own list of \
               places and its rank among the clothes, so a shirt that covers the chest and the \
               legs is drawn in both. Taking one thing off forgets that thing and nothing else, \
               and the list of places the login hands over fills the same memory.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O125-WORN-MEMORY"),
        station: "dereth-testkit::dat::inventory::equip::scenario_what_the_shard_says_is_worn_is_remembered_and_taking_it_off_forgets_only_that",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.world-drop.a-drop-onto-nothing-lands-on-the-ground-and-a-second-is-cancelled",
        says: "A release over nothing at all puts the thing on the ground. The same release for \
               something that is already out of every container and still the player's is cancelled \
               instead, in one word, and nothing is asked of the shard.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O288-GROUND"),
        station: "dereth-testkit::dat::inventory::give::scenario_a_drop_onto_nothing_lands_on_the_ground_and_a_second_one_is_cancelled",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.world-drop.a-dropped-item-is-drawn-in-the-world",
        says: "An item dropped out of the pack onto the ground is drawn in the world as soon as \
               the shard places it there, and nothing else in the scene gains or loses a part; \
               while it was in the pack it was not drawn at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F56-WORLD-DROP"),
        station: "dereth-client::gpu::inventory::world_drop_draw::an_item_dropped_out_of_the_pack_is_drawn_in_the_world",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "inventory.world-drop.a-dropped-things-row-leaves-the-pack-whichever-answer-comes-first",
        says: "Something dropped on the ground loses its row in the pack, and a click where that \
               row used to be reaches nothing -- whichever of the shard's two answers arrives \
               first, because the news that the thing has moved and the news that it is now lying \
               on the ground travel separately and either can overtake the other.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-40-GHOST-ROW"),
        station: "dereth-testkit::dat::inventory::split::scenario_a_dropped_things_row_leaves_the_pack_whichever_answer_comes_first",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.world-drop.a-player-in-mid-air-is-refused-and-one-on-the-ground-is-not",
        says: "Dropping something into the world is refused while the player is off the ground, \
               with the client saying so and the item coming back un-greyed, and goes through \
               while the player is standing -- the same body, the same item, the same gesture, \
               differing in nothing but the jump. A client that has no body to ask at all is \
               treated exactly as one in the air.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O417"),
        station: "dereth-testkit::dat::inventory::scenario_a_world_drop_is_refused_in_mid_air",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.world-drop.letting-go-over-the-view-of-the-world-is-a-drop-into-it",
        says: "Letting something go over the view of the world -- not over a panel, over the \
               world itself -- is a drop into the world, and it arms the search that works out \
               what the thing lands on. Before this the release named no target at all, so \
               nothing was asked and the icon was not even put back.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O85-WORLD-DROP"),
        station: "dereth-testkit::dat::inventory::equip::scenario_letting_go_over_the_view_of_the_world_is_a_drop_into_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "inventory.world-drop.whether-the-player-is-on-the-ground-is-read-again-every-frame",
        says: "Whether the player is on the ground is read again on every frame from the body \
               itself rather than remembered, so a frame with no body at all answers that it does \
               not know, and the difference between not knowing and never having asked is kept.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O417-PRODUCER"),
        station: "dereth-testkit::dat::inventory::scenario_the_ground_answer_is_read_again_every_frame",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "panels.redraw.a-shops-stock-follows-a-picture-or-a-price-arriving-under-an-unchanged-row",
        says: "A stock row whose picture or price arrives after the row itself is redrawn with \
               it, although the list of things on offer has not changed at all -- which is the \
               case a panel that only watched what was listed could never see.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O487-VENDOR"),
        station: "dereth-testkit::dat::inventory::scenario_a_stock_row_follows_a_late_picture_and_a_new_price",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "panels.redraw.the-partners-side-of-the-trade-follows-a-row-being-filled-in-later",
        says: "A row on the partner's side of the trade can arrive before the thing it names is \
               known, with no name and no picture, and is redrawn when they turn up -- although \
               the list of what is being offered never changed.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O487-TRADE"),
        station: "dereth-testkit::dat::inventory::scenario_the_partners_side_follows_a_row_filled_in_later",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "salvage.bag.a-bag-of-salvage-says-what-it-is-made-of",
        says: "A bag of salvage is named after what it is made of wherever the player sees it -- on \
               its tile in the pack and in the window that tells him what a thing is -- and \
               a thing whose name already says what it is made of is left exactly as it is.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-39-MATERIAL-PREFIX"),
        station: "dereth-testkit::dat::inventory::split::scenario_a_bag_of_salvage_says_what_it_is_made_of",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "salvage.button.the-button-sends-the-tool-and-every-row-and-empties-the-window",
        says: "The salvage button asks the shard to work the tool on every row in the window, \
               bottom row first, and then empties the window and puts its button back to \
               sleep while keeping the tool, so another batch needs no second use. Pressing \
               it with the window empty asks for nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O970-BUTTON"),
        station: "dereth-testkit::dat::inventory::split::scenario_the_button_sends_the_tool_and_every_row_and_empties_the_window",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "salvage.report.the-shards-report-is-read-out-on-the-scroll",
        says: "What the shard says came of a salvage is read out on the scroll: one line naming each \
               material, how much of it and how well made, joined the way a person would \
               join them, with the skill used and the bonus only when there was one; what \
               could not be salvaged is a line of its own naming each thing; nothing either \
               way says that the salvage failed; and a player who has silenced everything is \
               told nothing at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O970-REPORT"),
        station: "dereth-testkit::dat::inventory::split::scenario_the_shards_report_is_read_out_on_the_scroll",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "salvage.row.a-double-click-takes-a-row-out-and-a-single-click-does-not",
        says: "Double-clicking a row of the salvage window takes it out and a single click leaves it \
               alone; the window stays locked to its material until the last row goes, and \
               taking a row out asks the shard for nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O970-REMOVE"),
        station: "dereth-testkit::dat::inventory::split::scenario_a_double_click_takes_a_row_out_and_a_single_click_does_not",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "salvage.row.a-drag-the-shell-refuses-still-takes-the-row-off-and-letting-go-does-not-put-it-back",
        says: "A drag out of the salvage window that the shell itself refuses still takes the row \
               off the window, and letting go afterwards does not put it back -- which is what \
               the client being rebuilt does, with nothing on the cursor and the row gone. A row \
               the shipped screen actually ships cannot reach that refusal.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-9G2"),
        station: "dereth-testkit::dat::inventory::scenario_a_refused_drag_still_takes_the_salvage_row_off",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "salvage.row.a-message-naming-the-whole-list-starts-no-drag",
        says: "A refused drag that names the salvage list itself rather than a row in it starts \
               nothing and removes nothing, so the message the shell sends at every level of its \
               own walk cannot start a second drag inside the first.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-9G3"),
        station: "dereth-testkit::dat::inventory::scenario_a_refusal_naming_the_list_starts_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "salvage.row.dragging-one-out-takes-it-off-the-list-and-sends-nothing",
        says: "A row dragged out of the salvage window comes off it the moment the drag begins, \
               carrying the window's own kind of drag, and the button goes dead with the list \
               empty; a click that is not a drag takes nothing off, and none of it sends anything \
               to the shard.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-9G"),
        station: "dereth-testkit::dat::inventory::scenario_a_salvage_row_dragged_out_leaves_the_list",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "salvage.window.a-shortcut-carried-over-it-is-marked-neither-way",
        says: "A quickbar shortcut carried over the salvage window is marked neither welcome nor \
               refused: the row under the cursor keeps whatever mark it already had, because \
               a shortcut is a stand-in for a thing and not the thing itself.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-37-SHORTCUT"),
        station: "dereth-testkit::dat::inventory::split::scenario_a_shortcut_carried_over_it_is_marked_neither_way",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "salvage.window.a-thing-carried-over-it-is-marked-as-welcome-or-as-refused",
        says: "While something is held over the salvage window the row under the cursor says whether \
               it would be taken: welcome for a thing the window would accept, refused for \
               one it would not, and the answer the mark gives is the answer letting go \
               gives.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-37-HINT"),
        station: "dereth-testkit::dat::inventory::split::scenario_a_thing_carried_over_it_is_marked_as_welcome_or_as_refused",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "salvage.window.closing-it-empties-it-and-forgets-the-tool",
        says: "Closing the salvage window empties it, forgets the tool it was opened with and puts \
               its button back to sleep, asking the shard for nothing -- and it happens once \
               on the way down rather than again on every frame afterwards.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O970-CLOSE"),
        station: "dereth-testkit::dat::inventory::split::scenario_closing_it_empties_it_and_forgets_the_tool",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "salvage.window.only-a-suitable-thing-goes-in-and-the-first-one-fixes-the-material",
        says: "The salvage window takes only what can be salvaged: the first thing in decides what \
               the rest must be made of, a thing already in the list does not go in twice, a \
               thing with nothing wrong with it is refused, and a thing that is not the \
               player's is refused in words before anything else about it is looked at. \
               Filling the window asks the shard for nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O970-SUITABLE"),
        station: "dereth-testkit::dat::inventory::split::scenario_only_a_suitable_thing_goes_in_and_the_first_one_fixes_the_material",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "salvage.window.the-mark-comes-off-the-tile-as-soon-as-the-drop-lands",
        says: "The mark a carried thing puts on a row of the salvage window comes off the moment the \
               player lets go on that row, so a window that has taken a drop is not left \
               lit up afterwards.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-37-CLEAR"),
        station: "dereth-testkit::dat::inventory::split::scenario_the_mark_comes_off_the_tile_as_soon_as_the_drop_lands",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "salvage.window.the-second-material-only-goes-in-when-the-player-asked-for-it",
        says: "The salvage window holds one material at a time unless the player has asked to \
               salvage several at once, and with that asked for, a second material goes in \
               and the request carries every row; the other reasons a thing is refused still \
               refuse it either way.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-38-MULTIPLE"),
        station: "dereth-testkit::dat::inventory::split::scenario_the_second_material_only_goes_in_when_the_player_asked_for_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "salvage.window.using-a-tinkering-tool-opens-it-empty-and-remembers-the-tool",
        says: "Using a tinkering tool opens the salvage window, empty, with its button asleep and \
               the tool remembered -- and asks the shard for nothing, because opening a \
               window is the player's own business.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O970-OPEN"),
        station: "dereth-testkit::dat::inventory::split::scenario_using_a_tinkering_tool_opens_it_empty_and_remembers_the_tool",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "shell.logout.the-bound-key-asks-to-leave-and-the-player-stays-until-the-shard-answers",
        says: "Pressing the key that leaves for the character list asks to leave and leaves the \
               player standing in the world: the client puts up no screen of its own, because \
               what ends the session is the shard's own answer arriving later. The chain starts \
               with the shipped keymap really binding a key to that action, and it is the arm \
               that goes to the character list rather than the one that quits.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O85-LOGOUT-KEY"),
        station: "dereth-testkit::dat::inventory::equip::scenario_the_bound_key_asks_to_leave_and_the_player_stays_until_the_shard_answers",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "shortcut.bar.a-later-login-without-the-dropped-row-leaves-that-tile-empty",
        says: "Logging in again after a wielded thing has been lost to death leaves that tile empty \
               -- the client rebuilds the whole bar from what the shard sends and puts back no row the \
               shard left out -- while every tile that did not lose its thing still has it, and a \
               login that is already right asks the shard for nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-107B-RELOGIN"),
        station: "dereth-testkit::dat::inventory::death::scenario_a_later_login_without_the_dropped_row_leaves_that_tile_empty",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "shortcut.bar.a-tile-is-redrawn-when-anything-it-draws-about-its-item-changes",
        says: "A tile on the shortcut bar is redrawn whenever anything it draws about the thing \
               in it changes -- how many there are, the mark over its picture, its name -- each \
               on its own with the eighteen slots, the stance and everything else held still, so \
               a stack drawn down stops showing the count it was filled with.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O549-FIELDS"),
        station: "dereth-testkit::dat::inventory::scenario_a_tile_follows_every_thing_it_draws",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "shortcut.bar.a-wielded-thing-that-dies-with-the-player-empties-its-tile-and-tells-the-shard-once",
        says: "A thing the character was wielding, which is also on the shortcut bar, empties its \
               tile when it drops to the corpse, and the client asks the shard to forget that one \
               tile exactly once. The messages that merely unhand the thing do not empty it; the one \
               that takes the thing away does. No other tile is disturbed.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-107B-DEATH"),
        station: "dereth-testkit::dat::inventory::death::scenario_a_wielded_thing_that_dies_empties_its_tile_and_tells_the_shard_once",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "shortcut.bar.dragging-one-off-the-bar-empties-its-tile-and-tells-the-shard",
        says: "Dragging a shortcut off the bar takes it off the moment it is picked up, puts the \
               tile's numbered plate back without greying it, and tells the shard. Letting the \
               icon go over the pack invents no move for whatever was under the pointer.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F59-REMOVE"),
        station: "dereth-testkit::dat::inventory::scenario_dragging_a_shortcut_off_the_bar_empties_its_tile_and_tells_the_shard",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "shortcut.bar.dragging-one-tile-to-tile-moves-it-and-tells-the-shard-both-halves",
        says: "Dragging a shortcut from one tile to another moves it: the tile it left goes back \
               to its plate, the tile it arrived on draws the picture, and the shard is told both \
               where it was and where it is, the arrival exactly once. Neither message alone \
               would survive the next login.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F59-MOVE"),
        station: "dereth-testkit::dat::inventory::scenario_dragging_a_shortcut_tile_to_tile_moves_it_and_tells_the_shard_both_halves",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "shortcut.bar.drops-an-item-that-leaves-your-pack",
        says: "An item that leaves the player's own possession is taken off the shortcut bar and the \
               shard is told which slot was emptied, and the copy of it shown in the container it moved \
               into carries no shortcut numeral. The other shortcuts are untouched, and moving an item \
               between the player's own containers keeps its shortcut and sends nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-107"),
        station: "dereth-testkit::cpu::inventory::scenario_a_dropped_item_leaves_the_shortcut_bar",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "shortcut.bar.the-corpses-copy-of-a-dropped-thing-draws-no-number-and-asks-for-nothing-again",
        says: "Opening the corpse and seeing the same thing again draws no shortcut number on it, since \
               its tile has already been given up, and asks the shard to forget nothing a second time. \
               The shortcut that did not drop still draws its own number.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-107B-CORPSE"),
        station: "dereth-testkit::dat::inventory::death::scenario_the_corpses_copy_draws_no_number_and_asks_for_nothing_again",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "shortcut.bar.the-eighteen-tiles-fill-and-an-unchanged-frame-redraws-nothing",
        says: "All eighteen shortcut tiles are found in the shipped screen and the ones with \
               something in them are drawn and decorated; a frame in which nothing has changed \
               redraws none of them, and assigning a new shortcut redraws them at once, so the \
               bar is neither frozen nor rebuilt for nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O549"),
        station: "dereth-testkit::dat::inventory::scenario_the_eighteen_tiles_fill_and_hold_still",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "shortcut.cooldown.a-running-ring-counts-down-while-the-bar-is-otherwise-still",
        says: "The ring on a recharging thing on the shortcut bar counts down on its own while \
               the bar redraws nothing, exactly as it does in the pack, and the tile beside it \
               with nothing recharging never lights one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O549-COOLDOWN"),
        station: "dereth-testkit::dat::inventory::scenario_the_bar_ring_counts_down_while_the_gate_is_shut",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "shortcut.number.a-thing-pushed-off-its-tile-keeps-its-shortcut-in-the-next-free-slot",
        says: "Dropping something on an occupied shortcut tile moves the occupant to the \
               first empty slot to the right of that tile, searching to the end of all \
               eighteen slots before wrapping round. When the visible row is full to the \
               right, that slot is in the hidden second row, and the occupant's picture \
               keeps a shortcut plate with no figure on it, the second row's plate.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-SHORTCUT-PUSH-RIGHT"),
        station: "dereth-testkit::dat::inventory::scenario_a_thing_pushed_off_its_tile_keeps_its_shortcut_in_the_next_free_slot",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "shortcut.number.a-tile-naming-a-thing-the-client-has-not-seen-waits-for-it-before-drawing-the-number",
        says: "A shortcut tile naming a thing the client has not heard of yet draws no number on its \
               empty face, and takes the number up on the first pass after the thing arrives. A tile \
               naming a thing the client already has gets its number straight away.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O164-DELAYED-NUMERAL"),
        station: "dereth-testkit::dat::inventory::clicks::scenario_a_tile_naming_an_unknown_thing_waits_before_drawing_its_number",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "shortcut.number.an-assigned-thing-draws-its-slot-number-wherever-its-picture-appears",
        says: "A thing the player has put in a shortcut slot draws that slot's number on \
               every tile its picture appears in -- the pack grid, the side-pack strip and \
               the figure alike -- and each slot draws its own number, so two things in two \
               slots never look the same. A thing in no slot draws nothing at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P153-NUM"),
        station: "dereth-testkit::dat::inventory::scenario_an_assigned_thing_draws_its_slot_number_wherever_its_picture_appears",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "shortcut.number.it-follows-the-assignment-off-and-on-to-a-different-slot",
        says: "The number follows the assignment down as well as up: taking the shortcut \
               away takes the number off every tile, and putting the same thing into a \
               different slot draws that slot's number rather than merely drawing one \
               again. Without the down edge a number, once drawn, could never come off.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P153-EDGES"),
        station: "dereth-testkit::dat::inventory::scenario_the_number_follows_the_assignment_off_and_on_again",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "shortcut.number.it-is-drawn-over-the-busy-mark-and-the-selection-ring",
        says: "The number is the topmost of the three marks a tile can wear, so it stays \
               legible on a thing that is selected and waiting for the shard at the same \
               time.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P153-ORDER"),
        station: "dereth-testkit::dat::inventory::scenario_the_number_is_drawn_over_the_busy_mark_and_the_ring",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "shortcut.number.the-magic-stance-dims-it-everywhere-it-is-drawn",
        says: "Going into the magic stance dims the shortcut bar, and the number on the \
               thing's own picture dims with it wherever that picture is drawn rather than \
               only on the bar itself. The slot has not moved and the number is still \
               there; it is drawn dim.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P153-DIM"),
        station: "dereth-testkit::dat::inventory::scenario_the_magic_stance_dims_the_number_everywhere",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.close.a-completed-trade-empties-the-window-and-leaves-it-up",
        says: "A trade both sides agree to finishes without the window going anywhere: the goods are \
               swapped, both halves of the table are blanked, the window stays on screen still naming the \
               person across it, and the client says nothing more about the trade afterwards.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P160E-COMPLETION"),
        station: "dereth-testkit::dat::inventory::trade_window::scenario_a_completed_trade_empties_the_window_and_leaves_it_up",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.close.clearing-every-item-blanks-the-table-without-ending-the-negotiation",
        says: "The trade window's button for clearing everything asks for the table to be blanked and \
               asks for nothing else: the window stays on screen and the person across it is still the \
               person across it, so clearing what is offered is not the same gesture as walking away.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P160E-CLEAR-ALL"),
        station: "dereth-testkit::dat::inventory::trade_window::scenario_clearing_every_item_blanks_the_table_without_ending_the_negotiation",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.close.logging-off-tells-the-shard-nothing-about-the-trade",
        says: "Leaving the game in the middle of a trade sends nothing at all about that trade: the other \
               side learns it is over from the player leaving, and the client forgets who it was trading \
               with rather than carrying them into the next session.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P160E-LOGOFF"),
        station: "dereth-testkit::dat::inventory::trade_window::scenario_logging_off_tells_the_shard_nothing_about_the_trade",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.close.no-gesture-but-the-close-button-ends-the-negotiation",
        says: "Clearing the table, offering something else, agreeing, taking the agreement back and the \
               other side blanking both halves are five things that are not the end of a trade: none of \
               them ends the negotiation and none of them takes the window down. Each asks for its own \
               thing, and only the close button ends it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P160E-DENOMINATOR"),
        station: "dereth-testkit::dat::inventory::trade_window::scenario_no_gesture_but_the_close_button_ends_the_negotiation",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.close.the-close-button-is-the-only-gesture-that-takes-the-window-down",
        says: "The trade window's own close button is the one gesture that takes it off the \
               screen, and taking it off the screen is what tells the other side the negotiation \
               is over. What the shard has already confirmed is not thrown away with it: the \
               window empties, and only the shard's own close forgets the trade.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P160E-CLOSE-BUTTON"),
        station: "dereth-testkit::dat::inventory::trade_window::scenario_the_close_button_is_the_only_gesture_that_takes_the_window_down",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.close.the-partner-walking-away-ends-it-and-leaves-the-window-on-screen",
        says: "The person you are trading with walking more than a few paces away ends the negotiation \
               with the other side and leaves the window exactly where it is: emptied, but still naming \
               them, because nothing has yet said they are gone.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P160E-RANGE-EXIT"),
        station: "dereth-testkit::dat::inventory::trade_window::scenario_the_partner_walking_away_ends_it_and_leaves_the_window_on_screen",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.close.whatever-ends-a-trade-says-so-and-leaves-the-empty-window-up",
        says: "Whatever reason is given for a trade ending, the player is told the same sentence, both \
               halves of the table are emptied, the things that were on it stop being marked as being \
               traded, the name across the table is blanked, the window is left on screen for the player \
               to dismiss themselves, and nothing goes back out.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P160-CANCELLED"),
        station: "dereth-testkit::dat::inventory::trade_window::scenario_whatever_ends_a_trade_says_so_and_leaves_the_empty_window_up",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.controls.a-stale-window-accepts-an-empty-trade",
        says: "Pressing accept while the window on screen disagrees with what the shard has confirmed \
               sends an empty trade instead of the one shown, so a player cannot be tricked into \
               agreeing to a list they are not looking at; an agreeing window sends the real one. \
               Decline, clear and close each send their own ask, and closing the window takes the \
               traded marks back off the items.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O232-CONTROLS"),
        station: "dereth-testkit::cpu::inventory::scenario_a_stale_trade_window_accepts_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "trade.controls.the-one-button-agrees-and-then-takes-it-back-without-ending-anything",
        says: "The trade window's single agreement button is both answers: pressing it agrees to what is \
               on the table, pressing it again takes that agreement back, and neither press ends the \
               negotiation, empties the table or takes the window down.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P160-BUTTON"),
        station: "dereth-testkit::dat::inventory::trade_window::scenario_the_one_button_agrees_and_then_takes_it_back",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.drag-hint.a-shortcut-carried-over-the-table-is-given-no-answer-at-all",
        says: "A tile dragged off the bar of shortcuts and carried over the trade table is given no \
               answer by it at all: the place under the pointer keeps whatever it was already showing \
               rather than turning red at something that was never an offer.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P160D-SHORTCUT"),
        station: "dereth-testkit::dat::inventory::trade_window::scenario_a_shortcut_carried_over_the_table_is_given_no_answer_at_all",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.drag-hint.something-already-on-the-table-is-shown-as-refused",
        says: "The same thing, carried the same way, gets a different answer once it is already on the \
               trade table: the place under the pointer shows it as refused rather than accepted, and \
               letting go there really does nothing at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P160D-REFUSE-HINT"),
        station: "dereth-testkit::dat::inventory::trade_window::scenario_something_already_on_the_table_is_shown_as_refused",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.drag-hint.something-you-no-longer-carry-is-refused-without-a-word-until-you-let-go",
        says: "Something that has stopped being the player's in the middle of the carry is shown as \
               refused and nothing is said about it while the icon is in the air; only letting go of it \
               says why, in the client's own words.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P160D-NOT-CARRIED"),
        station: "dereth-testkit::dat::inventory::trade_window::scenario_something_you_no_longer_carry_is_refused_without_a_word",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.drag-hint.the-answer-comes-down-however-the-carry-ends",
        says: "However a carry over the trade table ends -- letting go on it, carrying off it on to the \
               window's own furniture, or carrying off the window altogether -- the place under the \
               pointer stops claiming it would take the drop, so no stale promise is left standing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P160D-HINT-DOWN"),
        station: "dereth-testkit::dat::inventory::trade_window::scenario_the_answer_comes_down_however_the_carry_ends",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.drag-hint.the-half-you-cannot-drop-on-answers-nothing-and-swallows-the-drop",
        says: "The other person's half of the trade table answers a carried thing with nothing at all: it \
               never lights up as willing over a drop it is going to throw away, and letting go there \
               does nothing and says nothing. Neither half offers an answer of its own accord.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P160D-PARTNER-SIDE"),
        station: "dereth-testkit::dat::inventory::trade_window::scenario_the_half_you_cannot_drop_on_answers_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.drag-hint.your-own-half-lights-for-something-you-are-carrying",
        says: "Carrying something you are holding over your own half of the trade table lights \
               that place up as willing to take it, before you let go; a place that has never \
               been carried over is not lit at all, and letting go there really does offer the \
               thing, so what the table promises and what it does cannot disagree.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P160D-ACCEPT-HINT"),
        station: "dereth-testkit::dat::inventory::trade_window::scenario_your_own_half_lights_for_something_you_are_carrying",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.drop.sends-add-to-trade-and-marks-the-item",
        says: "Dropping an item onto the trade table sends one add-to-trade naming that item and \
               its position, in drop order, and marks that item as being traded; an item that is \
               not the player's own is not marked.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O232"),
        station: "dereth-testkit::cpu::inventory::scenario_trade_drop_sends_add_to_trade",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "trade.open.a-combat-stance-refuses-the-request-in-the-clients-own-words",
        says: "Asking another player to trade while standing in a combat stance is refused in the \
               client's own words and asks nobody anything; the same gesture in peace mode opens the \
               negotiation, so the refusal is a refusal and not a path that never worked.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P160-PEACE-MODE"),
        station: "dereth-testkit::dat::inventory::trade_window::scenario_a_combat_stance_refuses_the_request_in_the_clients_own_words",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.open.double-clicking-another-player-starts-negotiations",
        says: "Double-clicking another player asks them to trade rather than trying to use them \
               or stuff them into a pack, and a player standing in a combat stance asks nobody to \
               trade and sends nothing at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O130-TRADE"),
        station: "dereth-testkit::cpu::inventory::scenario_double_clicking_a_player_starts_a_trade",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "trade.removal.a-later-confirmation-puts-the-id-back-and-a-reset-forgets-every-refusal",
        says: "A refusal is about one item and no more: the shard later confirming that very item \
               puts it back on the table as a real row, so what has been refused and what is on \
               the table are never the same thing at once, and either resetting the trade or \
               closing it forgets every refusal.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O603-DISJOINT"),
        station: "dereth-testkit::cpu::inventory::scenario_a_refusal_is_about_one_item_and_does_not_outlive_the_trade",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "trade.removal.a-plain-inventory-refusal-leaves-the-row-where-it-is",
        says: "An ordinary inventory refusal naming the item does not take it off the trade \
               table, because that message is about a stack split and not about the trade; the \
               row stays where the player put it, and the shard's own trade failure then does \
               take it off.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O603-NOT-ATTEMPT-FAILED"),
        station: "dereth-testkit::dat::inventory::scenario_a_plain_inventory_refusal_leaves_the_row",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.removal.the-shard-withdrawing-your-row-takes-it-off-and-the-partners-does-not",
        says: "The shard withdrawing an item from your own side of the table takes that row off \
               it; the same message about the partner's side leaves your side exactly as it was, \
               so a partner changing their mind never quietly removes what you offered.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O603-SIDES"),
        station: "dereth-testkit::dat::inventory::scenario_a_withdrawal_names_a_side",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.removal.the-shards-failure-takes-the-optimistic-row-off-the-table",
        says: "When the shard refuses an item the player has already been shown on the trade \
               table, the row comes off it, and the window stops claiming either side has agreed: \
               the accept button and the partner's light are both put back, whatever they were \
               showing before.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O603"),
        station: "dereth-testkit::dat::inventory::scenario_a_shard_failure_takes_the_optimistic_row_off",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.table.a-pack-put-on-the-table-offers-what-is-inside-it-and-says-so",
        says: "Letting a pack with things in it go on the trade table offers the things inside it, one by \
               one and in the pack's own order, tells the player that is what is happening, and does not \
               put the pack itself on the table.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P160-PACK-CONTENTS"),
        station: "dereth-testkit::dat::inventory::trade_window::scenario_a_pack_put_on_the_table_offers_what_is_inside_it_and_says_so",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.table.a-real-drag-from-the-pack-offers-the-item-and-draws-it-at-once",
        says: "Dragging something out of the pack and letting it go on the trade table asks the \
               shard for that one thing and nothing else, and draws it on the table straight \
               away, with its own name, before the shard has said a word. The shard's answer then \
               leaves one row where the player put it rather than adding a second.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P160-DRAG-OFFER"),
        station: "dereth-testkit::dat::inventory::trade_window::scenario_a_real_drag_offers_the_item_and_draws_it_at_once",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.table.a-stack-tells-you-how-many-there-are-and-what-they-are-called",
        says: "A stack offered on the trade table says how many of it there are and calls them by \
               their plural name, and a single one of the same thing is named in the singular \
               with no number at all. No count is painted over the icon, because on this table \
               the client being rebuilt paints none.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O605-TRADE"),
        station: "dereth-testkit::dat::inventory::scenario_a_stack_on_the_trade_table_is_counted_and_pluralised",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.table.an-item-put-on-the-table-wears-the-traded-mark-on-its-slot",
        says: "Putting an item on the trade table marks the item itself as being traded, and the \
               mark is drawn on the slot the player is looking at rather than only recorded, so a \
               player can see at a glance which of their things are committed.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O603-MARK"),
        station: "dereth-testkit::dat::inventory::scenario_an_item_on_the_table_wears_the_traded_mark",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.table.an-item-waiting-for-a-partner-reaches-the-table-when-the-negotiation-opens",
        says: "An item dropped on another player goes on to the trade table by itself the moment they \
               accept the window -- offered to them and drawn for you -- and it is offered exactly once, \
               so a second negotiation does not put it up again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P160-FOR-DUMMIES"),
        station: "dereth-testkit::dat::inventory::trade_window::scenario_an_item_waiting_for_a_partner_reaches_the_table_when_the_negotiation_opens",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.table.an-item-waiting-that-needs-splitting-is-refused-in-the-clients-own-words",
        says: "An item dropped on another player and waiting for them to answer is refused in the \
               client's own words, and never reaches the table, when it is the stack the player is \
               holding only part of.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P160-MUST-SPLIT"),
        station: "dereth-testkit::dat::inventory::trade_window::scenario_an_item_waiting_that_needs_splitting_is_refused_in_the_clients_own_words",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.table.anything-either-side-changes-puts-both-agreements-out",
        says: "Anything either side adds to the trade table puts both agreements out again and offers the \
               choice afresh, even when nothing has arrived to say the other person changed their mind, \
               so a lit agreement can never be sitting beside an offer that person has just altered.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P160-DARKEN"),
        station: "dereth-testkit::dat::inventory::trade_window::scenario_anything_either_side_changes_puts_both_agreements_out",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.table.part-of-a-stack-is-split-first-and-only-the-shards-own-new-stack-is-offered",
        says: "Dragging part of a stack on to the trade table splits it first and says so, and only the \
               new stack that comes back is offered -- never the one the player dragged and never the \
               stack it was taken out of. A split that is refused leaves nothing behind that a later \
               arrival could be mistaken for.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P160-SPLIT-OFFER"),
        station: "dereth-testkit::dat::inventory::trade_window::scenario_part_of_a_stack_is_split_first_and_only_the_shards_stack_is_offered",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.table.the-optimistic-row-is-drawn-before-the-shard-answers-and-is-not-doubled-by-it",
        says: "An item dropped on the trade table is drawn there at once, with its own picture \
               and name, before the shard has said anything; the shard's later confirmation \
               replaces that row's guessed name rather than adding a second row, and dropping the \
               same item twice adds nothing and sends nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O525"),
        station: "dereth-testkit::dat::inventory::scenario_a_drop_is_drawn_before_the_shard_answers",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.table.the-partners-container-keeps-what-is-inside-it-alive-only-while-it-is-on-show",
        says: "What is inside a container the other person offers is only kept while that row is on the \
               table: taking the row away puts the contents down to be forgotten, the row coming back or \
               the container moving to its owner cancels that, and the container itself is never \
               forgotten either way.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P160-PARTNER-CONTENTS"),
        station: "dereth-testkit::dat::inventory::trade_window::scenario_the_partners_container_keeps_its_contents_alive_only_while_shown",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.table.the-shard-voiding-both-agreements-empties-the-window-without-closing-it",
        says: "Both agreements being voided clears the table the player is looking at and takes the \
               traded marks back off, without closing the window and without forgetting the trade itself. \
               Pressing accept on the emptied window then offers nothing rather than what is still \
               remembered, and a fresh drop on to it is taken.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P160-CLEAR-ACCEPTANCE"),
        station: "dereth-testkit::dat::inventory::trade_window::scenario_the_shard_voiding_both_agreements_empties_the_window_without_closing_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.table.the-tooltip-reaches-the-slot-itself-and-not-only-the-panels-record",
        says: "What a stack on the trade table says about itself is written on to the slot the \
               pointer will hover over, and the slot is told it has something to say, so hovering \
               shows the text rather than an empty box.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O605-TOOLTIP"),
        station: "dereth-testkit::dat::inventory::scenario_the_tooltip_reaches_the_slot_itself",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.table.what-is-drawn-follows-every-frozen-fact-about-an-item-and-not-the-clock",
        says: "The trade table is redrawn whenever anything it draws about an item changes -- how \
               many there are, its overlay, its wear, its name, its plural name -- each one on \
               its own with the rest of the window held still. A cooldown counting down is not \
               one of those, because it takes a new value every frame and would flush the table \
               forever.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O605-TRADE-GUARD"),
        station: "dereth-testkit::dat::inventory::scenario_the_trade_table_follows_every_frozen_fact_and_not_the_clock",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.table.what-is-on-the-table-can-be-picked-and-examined-but-not-used",
        says: "A thing on either half of the trade table can be picked out and can be examined, and \
               cannot be used: neither an ordinary press nor a double click on one asks for anything at \
               all, and a right click asks about that very thing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P160-OFFERED-ROWS"),
        station: "dereth-testkit::dat::inventory::trade_window::scenario_what_is_on_the_table_can_be_picked_and_examined_but_not_used",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.window.a-drop-on-another-player-opens-it-only-if-the-player-asked-for-that",
        says: "A drop on another player hands the thing over, unless the player has asked for that \
               gesture to open the trade window instead -- which the shipped settings do not. With it \
               asked for, the partner and the thing are put aside for the window and nothing is handed \
               over.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O288-TRADE-OPTION"),
        station: "dereth-testkit::dat::inventory::give::scenario_a_drop_on_another_player_opens_the_trade_only_if_asked_for",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.window.each-sides-total-counts-only-its-own-side",
        says: "The two counts at the foot of the trade window count two different lists: filling \
               your side moves your count and leaves the partner's alone, and filling theirs does \
               the opposite. An item the player has only just dropped counts too, because the \
               count is of what is on screen.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O526-TOTALS"),
        station: "dereth-testkit::dat::inventory::scenario_each_sides_total_counts_only_its_own_side",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.window.every-element-the-panel-binds-to-is-in-the-shipped-tree",
        says: "Every part of the secure-trade window the client reaches for is really in the \
               screen it builds, exactly once each, and the two item lists really are lists -- so \
               the window is a window and not a panel bound to nothing whose tests all pass.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O232-TREE"),
        station: "dereth-testkit::dat::inventory::scenario_every_trade_element_is_in_the_live_tree",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.window.follows-the-shards-own-two-lists",
        says: "The trade window is filled by the shard: registering a trade raises it, each side's list \
               gains and loses the rows the shard names at the positions it gives, an accept or a \
               decline moves only the side it names, a failed addition is rolled back out, and a close \
               empties the window and leaves it on screen with nobody named in it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O232-WINDOW"),
        station: "dereth-testkit::cpu::inventory::scenario_the_trade_window_follows_the_shards_lists",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "trade.window.the-partners-light-and-your-own-button-are-two-different-things",
        says: "The light that says the partner has agreed and the button that says you have are \
               two separate things with two separate sources: the partner agreeing lights theirs \
               and leaves your button alone, you agreeing does the reverse, and either taking it \
               back puts its own indicator out again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O526-LIGHT"),
        station: "dereth-testkit::dat::inventory::scenario_the_light_and_the_button_are_two_different_things",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.window.the-partners-name-follows-the-partner-and-a-close-blanks-it",
        says: "The name at the top of the trade window is whoever you are trading with and \
               nothing else moves it, and when the negotiation ends the name is blanked rather \
               than left on screen for the next one to be mistaken for.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O526-NAME"),
        station: "dereth-testkit::dat::inventory::scenario_the_partner_name_follows_the_partner_alone",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.window.the-shards-registration-raises-it-and-names-the-partner",
        says: "The shard saying that two people are now trading is what puts the trade window on \
               the screen, and the person across the table is named in it at the same moment. \
               Until that message arrives the window is not on screen at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P160-REGISTER"),
        station: "dereth-testkit::dat::inventory::trade_window::scenario_the_registration_raises_the_window_and_names_the_partner",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "trade.window.your-own-name-is-bound-and-never-written",
        says: "The trade window has a place for your own name and the client deliberately never \
               writes it, because the client being rebuilt never wrote it either: the window can \
               be driven through every other thing it draws and that one field stays exactly as \
               the shipped screen left it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O526-SELF-NAME"),
        station: "dereth-testkit::dat::inventory::scenario_your_own_name_is_bound_and_never_written",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.hot-key.the-key-that-hides-the-interface-hides-it",
        says: "The key that hides the whole interface hides it: the game screen stops drawing and \
               its own root goes invisible with it. It is the other half of the same broadcast \
               the shortcut keys ride on, and the one arm of the game screen's key handling with \
               a visible effect that does not end the session.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O85-TOGGLE-UI"),
        station: "dereth-testkit::dat::inventory::equip::scenario_the_key_that_hides_the_interface_hides_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.pointer.turning-the-camera-with-the-right-button-is-not-an-appraisal",
        says: "Holding the right button and moving turns the camera, and letting it go there \
               looks at nothing; pressing and letting go in the same place is a click, and that \
               does look at what is under the pointer. Without the distinction every camera turn \
               appraised whatever happened to be under the cursor when the button came up. The \
               line between them is the client's own: three pixels of hand-shake is still a \
               click and four is a turn.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O85-CAMERA-TURN"),
        station: "dereth-testkit::dat::inventory::equip::scenario_turning_the_camera_with_the_right_button_is_not_an_appraisal",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "use.button.the-two-buttons-act-on-what-is-selected-or-arm-the-pointer-when-nothing-is",
        says: "The Use and Examine buttons on the toolbar each do one of two things depending on \
               whether anything is selected: with a selection they act on it, and with none they \
               arm the pointer, so that the next thing clicked is what gets used or looked at. A \
               button that is neither of them does nothing, and a real click on one has to reach \
               the part of the client the next click reads.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O85-TARGET-BUTTONS"),
        station: "dereth-testkit::dat::inventory::equip::scenario_the_two_buttons_act_on_what_is_selected_or_arm_the_pointer_when_nothing_is",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "use.confirmation.a-player-killer-altar-asks-before-anything-is-sent",
        says: "Using an altar that would make the player killable by other players puts the \
               question to them first and sends nothing while they think about it; answering yes \
               then uses it exactly as any other use would, and the wording of the question names \
               what the player is about to become.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O130-ALTAR"),
        station: "dereth-testkit::cpu::inventory::scenario_a_player_killer_altar_asks_first",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "use.confirmation.a-second-one-waits-behind-the-first-and-keeps-its-own-pair",
        says: "A second use that has to ask first does not replace the box already on screen: it waits \
               behind it. Answering the first with a No brings the second up, still carrying its own \
               pair, and a Yes then uses that pair and nothing else.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-CONFIRM-QUEUE"),
        station: "dereth-testkit::dat::inventory::confirm::scenario_a_second_box_waits_behind_the_first_and_keeps_its_own_pair",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "use.confirmation.a-yes-uses-the-pair-the-box-was-made-with",
        says: "The box names the thing it is about, in the words the player reads, and a Yes uses the \
               pair the box was made with -- not whatever is picked at the moment the button is pressed. \
               The box is gone afterwards and nothing else is left on screen.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-CONFIRM-YES"),
        station: "dereth-testkit::dat::inventory::confirm::scenario_a_yes_uses_the_pair_the_box_was_made_with",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "use.confirmation.one-nobody-answers-dies-with-the-screen-and-the-next-use-works",
        says: "A box nobody answers dies with the screen it was raised over, taking what it was going to \
               do with it: nothing is sent and nothing is left waiting. The same use made again on the \
               rebuilt screen's own new tiles raises a fresh box and works.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-CONFIRM-ORPHAN"),
        station: "dereth-testkit::dat::inventory::confirm::scenario_one_nobody_answers_dies_with_the_screen_and_the_next_use_still_works",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "use.confirmation.prints-the-same-line-and-the-material-name",
        says: "Confirming a use the client asked about first prints the same progress line as an \
               ordinary use, with no separate wording for a creature, and both lines name the object \
               the way the player sees it named, including the material it is made of.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F26-DIALOG"),
        station: "dereth-testkit::cpu::inventory::scenario_a_confirmed_use_prints_the_same_line",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "use.confirmation.something-kept-is-refused-before-any-box-is-raised",
        says: "Something the player has marked as kept is refused before any box is raised at all, in \
               the client's own words. Unmark it and the same gesture tells the player what is being \
               done to what, in a bubble he can see.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-CONFIRM-RETAINED"),
        station: "dereth-testkit::dat::inventory::confirm::scenario_something_kept_is_refused_before_any_box_and_a_fresh_one_is_told_about",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "use.confirmation.the-salvage-box-says-what-it-may-destroy-and-a-no-cancels-it",
        says: "The box for applying salvage names both things and says the second may be destroyed. A No \
               cancels it: nothing is asked of the shard. Turning the skip-this-box setting on after a \
               box is up does not rewrite what that box was made to do, and with it on from the start \
               there is no box and the use goes straight out.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-CONFIRM-SALVAGE"),
        station: "dereth-testkit::dat::inventory::confirm::scenario_the_salvage_box_says_what_it_may_destroy_and_a_no_cancels_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "use.confirmation.the-shipped-use-key-raises-the-question-and-the-two-answers-differ",
        says: "Pressing the key the shipped keymap binds to use, on something that asks first, \
               raises the question in the client's own words and sends nothing while the player \
               thinks about it. Yes uses it exactly once, leaves the player busy until the shard \
               says the use is done, and takes the question down; no takes the question down and \
               sends nothing at all. What is acted on is what the question was put up about and \
               not whatever happens to be picked when the answer comes.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P197C-USE-KEY"),
        station: "dereth-testkit::dat::inventory::scenario_the_shipped_use_key_raises_the_question_and_the_two_answers_differ",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "use.progress-notice.names-the-object",
        says: "Using something in the world prints a progress line in the notice strip: \"Using \
               the <name>\" for an ordinary object and \"Approaching <name>\" for a creature.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F26"),
        station: "dereth-testkit::cpu::inventory::scenario_use_progress_notice",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "use.refusal.each-arm-is-counted-against-the-recordings-and-three-have-no-witness",
        says: "Every thing the recordings describe is counted into the answer a use on it would get, so \
               that a sentence read off the shipped client is not mistaken for one a player will ever \
               read. Three of the answers have no witness in the recordings at all, each for its own \
               reason, and the counts are held rather than printed.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F70-CENSUS"),
        station: "dereth-testkit::dat::inventory::use_refusal::scenario_each_arm_is_counted_against_the_recordings_and_three_have_no_witness",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "use.refusal.each-refusal-is-named-rather-than-collapsed-into-one",
        says: "The reasons a use cannot go ahead stay apart: a container that is sealed shut is \
               refused, a creature is refused without a word being said about it, an object the \
               client has never heard of says so in the notice strip, an item already on the \
               trade table is refused for being traded, and a weapon that has to be in hand is \
               refused for not being wielded. None of them sends anything to the shard.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O130-REFUSALS"),
        station: "dereth-testkit::cpu::inventory::scenario_every_refusal_keeps_its_own_reason",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "use.refusal.names-the-thing-the-way-the-player-sees-it-named",
        says: "A refusal names the thing the way the player sees it named on screen, which for \
               something made of a material is that material and then its name -- not the bare name the \
               shard sent.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F70-NAME"),
        station: "dereth-testkit::dat::inventory::use_refusal::scenario_the_refusal_names_the_thing_the_way_the_player_sees_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "use.refusal.reaches-the-bubble-strip-and-never-the-scrollback",
        says: "Pressing the shipped Use button on a chest that will not open puts the refusal in the \
               strip of bubbles over the world, as one new bubble, in the words the player reads. It \
               does not reach the chat scrollback at all: a client that shouted every refusal everywhere \
               would look right from the bubble alone.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F70-BUBBLE"),
        station: "dereth-testkit::dat::inventory::use_refusal::scenario_the_chest_that_will_not_open_says_so_in_the_bubble_strip",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "use.refusal.something-attackable-out-of-a-stance-says-to-arm-first",
        says: "Clicking something that could be fought while the player is not in a stance tells him to \
               arm himself first, naming it -- which is a different answer from the one a thing that \
               simply cannot be used gets.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F70-DOVE"),
        station: "dereth-testkit::dat::inventory::use_refusal::scenario_using_something_attackable_out_of_a_stance_says_to_arm_first",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "use.refusal.something-on-the-trade-table-says-so",
        says: "Something already on the trade table cannot be used, and the player is told so in a \
               sentence naming the thing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F70-TRADED"),
        station: "dereth-testkit::dat::inventory::use_refusal::scenario_using_something_on_the_trade_table_says_so",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "use.refusal.something-that-cannot-be-used-says-so",
        says: "Anything else that cannot be used says exactly that, naming the thing. Before this the \
               reason was worked out and thrown away, so a click that could do nothing did nothing at \
               all and said nothing either.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F70-NOT-USEABLE"),
        station: "dereth-testkit::dat::inventory::use_refusal::scenario_using_something_that_cannot_be_used_says_so",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "use.refusal.something-that-must-be-held-says-so",
        says: "Something that can only be used while it is held says so while it is not being held, \
               naming the thing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F70-WIELD"),
        station: "dereth-testkit::dat::inventory::use_refusal::scenario_using_something_that_must_be_held_says_so",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "use.refusal.using-yourself-or-what-you-are-fighting-says-nothing",
        says: "Two answers are deliberately silent: using yourself, and clicking something the player is \
               already in a stance against, which was an attack and not a use. Both are refusals and \
               neither prints a word, so that recovering the other five never turns into shouting.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F70-SILENT"),
        station: "dereth-testkit::dat::inventory::use_refusal::scenario_using_yourself_or_what_you_are_fighting_says_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "use.target-mode.a-batch-of-clicks-reads-each-answer-as-it-is-made",
        says: "Three clicks that arrive together are each read against what the one before it left, not \
               all against the state at the start: the first picks the thing, the second arms the use, \
               and the third uses it on what it lands on -- and that third click does not pick anything.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-DELIVERY-BATCH"),
        station: "dereth-testkit::dat::inventory::delivery::scenario_a_batch_of_clicks_reads_each_answer_as_it_is_made",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "use.target-mode.a-use-that-wants-a-target-arms-the-second-click-and-asks-for-one",
        says: "A use on something that has to be used on something else arms the second click and asks \
               for a target in words, naming the thing being used. It is the one line of the seven that \
               is not a refusal at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F70-TARGET"),
        station: "dereth-testkit::dat::inventory::use_refusal::scenario_a_use_that_wants_a_target_arms_the_second_click_and_asks_for_one",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "use.target-mode.the-button-with-nothing-picked-arms-and-the-next-click-is-the-source",
        says: "Pressing Use with nothing picked arms the gesture rather than refusing it, and the next \
               click on a thing makes that thing the one to be used -- without picking it. The click \
               after that uses it on what it lands on.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-DELIVERY-REARM"),
        station: "dereth-testkit::dat::inventory::delivery::scenario_the_use_button_with_nothing_picked_arms_and_the_next_click_is_the_source",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "use.target-mode.the-cursor-follows-what-is-under-it-while-the-second-click-waits",
        says: "While the client is waiting for the second click the cursor says whether what is under it \
               could take what is armed, and it is worked out again every frame rather than only when \
               the pointer moves: something that becomes untouchable changes the cursor with no movement \
               at all, and changes it back when it is touchable again. A hover never picks anything.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-DELIVERY-CURSOR"),
        station: "dereth-testkit::dat::inventory::delivery::scenario_the_cursor_follows_what_is_under_it_while_the_second_click_waits",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "use.target-mode.the-second-click-uses-what-was-armed-and-not-what-is-picked",
        says: "The second click uses the thing that was armed, on what the click lands on -- not \
               whatever happens to be picked at that moment, and the click itself picks nothing. What \
               goes out is the recorded client's own request, and the pair after it is driven once the \
               shard's own answers have released the first.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-DELIVERY-ARMED"),
        station: "dereth-testkit::dat::inventory::delivery::scenario_the_second_click_uses_what_was_armed_and_not_what_is_picked",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.busy.a-purchase-leaves-the-player-able-to-act",
        says: "Buying from a shop and letting the shard answer leaves the player able to move \
               things about again: the next drag reaches the shard and the one-thing-at-a-time \
               refusal is not on screen. With nothing outstanding at all the same drag goes \
               through, which is what makes the measurement a measurement.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F54-BUY"),
        station: "dereth-testkit::dat::inventory::scenario_a_purchase_leaves_the_player_able_to_act",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.busy.a-sale-leaves-the-player-able-to-act",
        says: "Selling to a shop and letting the shard answer leaves the player able to move \
               things about again, the same way a purchase does -- asserted on its own, because \
               one of the two wired is not both.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F54-SELL"),
        station: "dereth-testkit::dat::inventory::scenario_a_sale_leaves_the_player_able_to_act",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.busy.while-the-shop-has-not-answered-the-refusal-still-fires",
        says: "While a purchase really is outstanding and the shard has not answered, the \
               one-thing-at-a-time refusal still fires, says so on the strip across the top of \
               the screen, and sends nothing -- there is no time limit on it. A reply from a \
               different merchant does not count as the answer, so it does not let the player go \
               either.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F54-LATCH"),
        station: "dereth-testkit::dat::inventory::scenario_while_the_shop_has_not_answered_the_refusal_still_fires",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.buy.a-double-click-on-a-stock-row-buys-it",
        says: "Double-clicking a row of a shopkeeper's stock buys it straight away, just as                pressing Buy with that row chosen does: one purchase of that row, in the number                the quantity slider says, sent at once rather than added to the basket. The                first click of the pair only chooses the row, and the item is never used.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-VENDOR-DBLCLICK-BUY"),
        station: "dereth-ui-screens::dat::inventory::vendor_stock_double_click::a_double_click_on_a_stock_row_buys_that_row_at_the_slider_amount",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.buy.a-player-with-no-money-or-no-room-is-refused-and-sends-nothing",
        says: "The shop refuses a purchase the player cannot make before it ever reaches the \
               shard: with too little money, and with no free slot for what is being bought, the \
               player is told so in the notice strip and nothing at all goes out.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O231-REFUSAL"),
        station: "dereth-testkit::cpu::inventory::scenario_a_broke_or_full_player_buys_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "vendor.buy.the-button-and-the-basket-each-send-one-purchase",
        says: "Buying straight from a stock row sends one purchase naming that row and how many \
               of it, and it goes out the moment the button is pressed. Adding the same row to \
               the basket twice and then buying the lot sends one purchase for two of it rather \
               than two purchases, and the shop being bought from is named in both.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O231-BUY"),
        station: "dereth-testkit::cpu::inventory::scenario_the_button_and_the_basket_each_buy_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "vendor.buy.the-quantity-slider-only-moves-a-stackable-row",
        says: "The quantity the player dials in is honoured only for a stock row that stacks: a \
               row sold one at a time is bought one at a time whatever the slider says, and so is \
               a row whose stack is one. A row that does stack is bought in the number asked for, \
               and the price the money check runs against grows with that number.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O231-SPLIT"),
        station: "dereth-testkit::cpu::inventory::scenario_the_quantity_only_moves_a_stackable_row",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "vendor.close.leaving-with-something-in-a-basket-asks-first",
        says: "Closing a shop while something is still in one of its baskets does not close it: \
               the player is asked whether they mean to, in the client's own words and in a \
               dialog rather than a line across the top of the screen. With both baskets empty \
               the shop closes at once and asks nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F47-CLOSE"),
        station: "dereth-testkit::dat::inventory::scenario_closing_with_a_basket_asks_first",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.filter.a-shop-with-no-category-chosen-shows-nothing",
        says: "With no category chosen at all a shop's shelf is empty rather than showing everything: \
               nothing being picked is not the same as no filter. Asking for one particular kind by \
               name still fills the shelf with exactly that kind, without consulting the strip.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O650-MASK-ZERO"),
        station: "dereth-testkit::dat::inventory::shop::scenario_a_shop_with_no_category_chosen_shows_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.filter.a-thing-the-player-owns-keeps-its-own-kind-when-the-shop-advertises-it",
        says: "When a shop advertises something the player already holds and owns, the thing keeps \
               its own kind rather than taking the shop's word for it, and the strip of categories \
               grows the tab that kind belongs to. A thing the client has never seen, and a thing it \
               holds but the player does not own, both take the kind the shop gives.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O653-OWNED"),
        station: "dereth-testkit::dat::inventory::shop::scenario_a_thing_the_player_owns_keeps_its_own_kind_when_the_shop_advertises_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.filter.choosing-a-category-leaves-exactly-the-things-of-that-kind",
        says: "Choosing a category in a shop leaves exactly the rows of that kind on the shelf and no \
               others, whichever category it is, and moves what is picked to the first of them. A \
               freshly opened shop is already showing its first category rather than everything it \
               sells, and at least one category is a smaller list than the whole shop, so the strip \
               really is filtering.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O650-TAB"),
        station: "dereth-testkit::dat::inventory::shop::scenario_choosing_a_category_leaves_exactly_the_things_of_that_kind",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.filter.coming-back-to-the-stock-page-redraws-it-without-moving-what-is-picked",
        says: "Leaving a shop's stock page and coming back to it draws the same rows again without \
               moving what the player had picked, and a page change while the buying page is up does \
               not touch the shelf at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O650-PAGE"),
        station: "dereth-testkit::dat::inventory::shop::scenario_coming_back_to_the_stock_page_redraws_it_without_moving_what_is_picked",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.filter.every-category-is-a-row-the-player-can-actually-click",
        says: "A shop with more than one category shows every one of them as a row the player can \
               actually reach with the pointer -- none clipped away under the one above it -- and \
               pressing one of those rows replaces what is on the shelf with that category's things.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-91-POPUP"),
        station: "dereth-testkit::dat::inventory::shop::scenario_every_category_is_a_row_the_player_can_actually_click",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.filter.the-list-that-keeps-nothing-still-has-no-room-to-scroll",
        says: "A shop's shelf keeps the empty cells that fill its width even when the chosen category \
               keeps nothing, and however it is filled there is no room to scroll it: the list is as \
               wide as the window and never longer.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O650-SCROLL"),
        station: "dereth-testkit::dat::inventory::shop::scenario_the_list_that_keeps_nothing_still_has_no_room_to_scroll",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.filter.the-screen-carries-a-click-on-the-strip-and-a-change-of-page-to-the-shop",
        says: "The screen the client builds carries a click on a shop's category strip and a change \
               of its page through to the shop window, in the order they happened, and a message no \
               window on the screen owns is carried to nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O650-ROUTING"),
        station: "dereth-testkit::dat::inventory::shop::scenario_the_screen_carries_a_click_on_the_strip_and_a_change_of_page_to_the_shop",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.filter.what-is-picked-moves-only-when-a-shop-opens",
        says: "What the player has picked in a shop moves to the first row only when a shop opens -- \
               including walking from one merchant straight to the next, and a merchant sending its \
               stock again. An ordinary redraw refills the same shelf and leaves the pick alone, a \
               shop that is not open picks nothing at all, and walking away empties the strip of \
               categories without clearing the pick.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O790-OPEN-EDGE"),
        station: "dereth-testkit::dat::inventory::shop::scenario_what_is_picked_moves_only_when_a_shop_opens",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.money.a-basket-is-counted-in-things-and-not-in-rows",
        says: "What a basket says it holds is a count of things and not of rows. Buying counts \
               each selected quantity once; selling counts each offered object's stack, with \
               an absent or zero stack size counting as one. A missing object counts \
               as nothing. The noun is singular at exactly one and plural everywhere else, zero \
               included, and the money is grouped in threes.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-VENDOR-BASKET-QUANTITIES"),
        station: "dereth-testkit::dat::inventory::scenario_a_basket_is_counted_in_things_and_not_in_rows",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.money.each-of-the-four-lines-is-written-by-its-own-source",
        says: "The shop shows four lines of money and each is written from its own number: what \
               the buying basket comes to, what the selling basket comes to, and the player's \
               purse twice over. Moving one of the three moves exactly the lines that read it and \
               leaves the others exactly as they were.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O604-WRITERS"),
        station: "dereth-testkit::dat::inventory::scenario_each_money_line_follows_its_own_source",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.money.the-four-lines-and-the-filter-strip-are-five-different-things-in-the-shipped-tree",
        says: "The four money lines and the strip of category tabs are five separate places in \
               the screen the client builds, and no two of them are the same one -- so a line \
               written into the wrong place shows up as a wrong line rather than as four \
               assertions satisfied at once.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O604"),
        station: "dereth-testkit::dat::inventory::scenario_the_money_lines_are_five_different_places",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.purse.a-purse-that-came-only-from-the-login-can-buy-what-it-can-afford",
        says: "A purse whose only source is the login description can actually buy what it can \
               afford. The shop's money test is a plain comparison against that number, so \
               losing it does not merely mis-draw a label: it refuses every purchase however \
               rich the player is.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F57-AFFORD"),
        station: "dereth-testkit::dat::inventory::scenario_a_purse_that_came_only_from_the_login_can_buy_what_it_can_afford",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.purse.a-stale-coin-update-for-the-player-is-dropped-by-the-same-counter",
        says: "The two forms a coin update arrives in -- the one addressed to the player and the \
               one that leaves the player implied -- share one counter, so an update older than \
               the last one already taken is dropped and a newer one lands. Two stores, each \
               with a counter of its own, would both have accepted the stale one; the shop is \
               re-opened between the update and the reading so that what is measured is the \
               store and not a number the window was already holding.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F57-ONE-STORE"),
        station: "dereth-testkit::dat::inventory::scenario_a_stale_coin_update_for_the_player_is_dropped_by_the_same_counter",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.purse.is-filled-from-the-players-own-coin-when-the-shop-opens",
        says: "Opening a shop fills its purse from how much money the player is carrying, and a \
               player the shard has said nothing about is treated as having none and is refused \
               every purchase; a player with money is not refused and the purchase goes out.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O604-PURSE"),
        station: "dereth-testkit::dat::inventory::scenario_the_purse_is_filled_from_the_players_own_coin",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.purse.the-first-recorded-shop-opens-on-the-purse-the-login-description-carried",
        says: "The first shop of a recording opens on the purse the login description carried, \
               and the same number appears in all three places the shop says what the player \
               has -- the two purse lines the window writes and the picked row's cost line, \
               which a different call writes. The shard has said nothing about that player's \
               coin at this point, so the login description is the only place the number could \
               have come from.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F57-LOGIN-PURSE"),
        station: "dereth-testkit::dat::inventory::scenario_the_first_recorded_shop_opens_on_the_purse_the_login_description_carried",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.purse.the-open-shop-shows-the-players-own-coin-and-follows-it",
        says: "Both places the shop says how much the player has are carrying the player's own \
               money when the window opens, and they follow it afterwards: a purchase and a sale \
               each move both lines rather than leaving the window as the snapshot it opened on.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F47-PURSE"),
        station: "dereth-testkit::dat::inventory::scenario_the_shop_shows_the_players_own_coin_and_follows_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.sell.a-drop-on-a-shopkeeper-is-a-sale-and-parks-the-thing",
        says: "A drop on a shopkeeper is a sale, not a gift: the shop and the thing are both put aside \
               until the shop answers, and nothing is handed over. A shopkeeper is a creature too, and \
               this answer is given first.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O288-VENDOR"),
        station: "dereth-testkit::dat::inventory::give::scenario_a_drop_on_a_shopkeeper_is_a_sale_and_parks_the_thing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.sell.a-drop-on-the-shops-own-sell-window-offers-it-and-sell-everything-sends-it",
        says: "Letting something go on a shop's own selling window puts it on the counter: it joins \
               the list of what is being sold, it is marked as being sold in the player's own pack, \
               and pressing sell-everything puts that very thing on the wire addressed to that shop. \
               A shop just opened has nothing on its counter, which is what makes the measurement a \
               measurement.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F33-SELL-DROP"),
        station: "dereth-testkit::dat::inventory::shop::scenario_a_drop_on_the_shops_own_sell_window_offers_it_and_sell_everything_sends_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.sell.a-pack-let-go-on-the-window-offers-what-is-inside-it-and-not-itself",
        says: "A pack let go on a shop's sell window offers every thing inside it that the shop \
               would buy, in the pack's own order, and does not offer the pack. The gesture is \
               allowed whatever the shop thinks of packs and with no message; each thing inside \
               is judged on its own; each one offered is marked as being sold; and the line the \
               player reads names the pack, so they know a container was emptied onto the \
               counter rather than one thing added.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F63-PROXY"),
        station: "dereth-testkit::dat::inventory::scenario_a_pack_let_go_on_the_window_offers_what_is_inside_it_and_not_itself",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.sell.an-empty-pack-is-offered-as-itself",
        says: "An empty pack has no contents to stand for, so letting it go on the sell window \
               offers the pack itself and its own acceptability decides; the line about selling \
               a container's contents is not printed.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F63-EMPTY-PACK"),
        station: "dereth-testkit::dat::inventory::scenario_an_empty_pack_is_offered_as_itself",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.sell.an-item-the-vendor-will-not-take-is-refused-in-its-own-words",
        says: "A shop says why it will not take a thing, in its own words and in a way the player \
               can act on: not the right sort of goods, worth nothing, too cheap, too valuable, \
               not being carried, a container with something still in it, or part of a stack \
               rather than the whole of it. A refused item is not marked for sale and nothing is \
               sent for it; the whole of a stack goes through as one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O231-ACCEPT"),
        station: "dereth-testkit::cpu::inventory::scenario_an_unwanted_item_is_refused_in_the_shops_words",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "vendor.sell.letting-go-takes-the-hint-down-whichever-answer-it-was-showing",
        says: "Letting the carried icon go on a shop's own tile takes that tile's hint down at \
               once, and it does so whichever of the two answers the tile was showing -- the \
               clear happens before the drop is decided and does not look at the outcome. \
               Without it a circle sits on the window for the rest of the session.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F63-HINT-DOWN"),
        station: "dereth-testkit::dat::inventory::scenario_letting_go_takes_the_shop_hint_down_whichever_answer_it_was_showing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.sell.part-of-a-stack-let-go-on-the-sell-window-is-split-off-and-offered",
        says: "Part of a stack let go on a shop's selling window is split off and that part is \
               what is offered. With the quantity dial off the whole stack, the drop asks the \
               shard to split the dialled amount off beside the stack, says it is splitting, and \
               holds a row in the list with the stack; when the new object of that kind and size \
               arrives it takes the row and wears the offered mark, and the rest of the stack is \
               no longer offered.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-SELL-SPLIT"),
        station: "dereth-testkit::dat::inventory::shop::scenario_part_of_a_stack_let_go_on_the_sell_window_is_split_off_and_offered",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.sell.selling-the-whole-list-sends-one-message-and-clears-the-marks",
        says: "Selling everything in the sell list sends one sale carrying every row of it rather \
               than one sale a row, and afterwards the list is empty and none of those items is \
               marked for sale any more.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O231-SELL"),
        station: "dereth-testkit::cpu::inventory::scenario_selling_the_whole_list_sends_one_message",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "vendor.sell.something-offered-wears-the-mark-in-the-pack-as-well-as-in-the-window",
        says: "Something put into a shop's sell list wears the offered mark in the player's own \
               pack as well as in the window: the mark belongs to the thing and not to either \
               list, and it is raised on the tile rather than only remembered.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F46-MARK"),
        station: "dereth-testkit::dat::inventory::scenario_something_offered_to_the_shop_wears_the_mark_in_both_windows",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.sell.taking-a-row-back-out-of-the-window-is-a-withdrawal-and-not-a-sale",
        says: "Dragging a row back out of a shop's selling window takes the offer back rather than \
               selling it: the row leaves the list, its mark comes off, and nothing is sent. If only \
               part of a stack was dialled in, the amount is put back to the whole stack and the \
               player is told they cannot split things from that panel; with the whole stack dialled \
               in there is no such line and the amount is left alone. A press and release that never \
               moves is not a drag and takes nothing off.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F33-DRAG-OUT"),
        station: "dereth-testkit::dat::inventory::shop::scenario_taking_a_row_back_out_of_the_window_is_a_withdrawal_and_not_a_sale",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.sell.the-list-marks-what-is-in-it-and-the-close-button-asks-first",
        says: "Dragging an item into the sell list marks it as being sold, and taking that one \
               back out unmarks it and leaves the rest marked. Closing the shop while anything is \
               still in a basket asks the player whether they mean to walk away from it and \
               leaves the window up; with the baskets empty the shop closes and every mark comes \
               off.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O231-MARK"),
        station: "dereth-testkit::cpu::inventory::scenario_the_sell_list_marks_what_is_in_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "vendor.sell.the-selling-pages-four-buttons-reach-their-requests",
        says: "Each button of a shopkeeper's selling page does its job when pressed: Sell Item \
               offers the chosen item, Sell All offers the whole list, Clear Item takes the chosen \
               item off the list, Clear List empties it, and Close shuts the shop.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1181-SELL"),
        station: "dereth-ui-screens::dat::inventory::vendor_selling_page::clicking_the_selling_pages_four_buttons_reaches_the_production_requests",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.sell.the-window-says-whether-it-would-buy-what-is-carried-over-it",
        says: "Carrying something over a shop's sell window shows whether she would buy it: one \
               answer for something she takes and a different one for something she will not.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F46-CURSOR"),
        station: "dereth-testkit::dat::inventory::scenario_the_shop_says_whether_it_would_buy_what_is_carried_over_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.shelf.a-container-with-anything-in-it-is-not-on-the-shelf",
        says: "A shop row that is a container with anything inside it is not on the shelf at all -- \
               something in it is enough on its own, and another container in it is enough on its own \
               -- and it has still been set up to the biggest stack it could be sold in by the time \
               it is dropped. An empty one is listed as usual.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O741-CONTAINMENT"),
        station: "dereth-testkit::dat::inventory::shop::scenario_a_container_with_anything_in_it_is_not_on_the_shelf",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.shelf.a-row-the-basket-already-holds-all-of-is-gone-from-the-shelf",
        says: "A row the buying basket already holds the whole supply of is gone from the shelf \
               altogether -- not greyed and not zeroed -- while one of the supply still left keeps it \
               there. The same thing added to the basket twice counts as both, and a shop that really \
               advertises one of something stops showing it the moment that one is added.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O740-BASKET"),
        station: "dereth-testkit::dat::inventory::shop::scenario_a_row_the_basket_already_holds_all_of_is_gone_from_the_shelf",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.shelf.a-row-the-basket-does-not-name-is-never-counted-against-its-own-supply",
        says: "A row the buying basket does not name at all is never counted against the supply the \
               shop advertises, so a row the shop says it has none of is still on the shelf until \
               something of it is actually basketed, and so is one whose advertised supply reads as \
               less than none.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O740-BYPASS"),
        station: "dereth-testkit::dat::inventory::shop::scenario_a_row_the_basket_does_not_name_is_never_counted_against_its_own_supply",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.shelf.a-stackable-row-is-offered-in-the-biggest-stack-the-shop-can-sell",
        says: "A shop row that stacks is set up to the biggest stack it can be sold in: the whole \
               stack where the supply is endless, and only as many as are actually left where it is \
               not. A row that does not stack, and one whose stack is one, are left alone entirely.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O740-STACK-ARMS"),
        station: "dereth-testkit::dat::inventory::shop::scenario_a_stackable_row_is_offered_in_the_biggest_stack_the_shop_can_sell",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.shelf.setting-a-rows-stack-keeps-its-unit-price-and-lets-the-quantity-dial-mean-something",
        says: "Setting a shop row's stack to a number leaves what one of them costs exactly as it \
               was, and it is what lets the quantity dial mean anything: before it the add-to-list \
               button puts one in the basket whatever the dial says, and after it the dialled number. \
               Doing it twice changes nothing the second time.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O740-RESCALE"),
        station: "dereth-testkit::dat::inventory::shop::scenario_setting_a_rows_stack_keeps_its_unit_price_and_lets_the_quantity_dial_mean_something",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.shelf.the-picked-row-can-be-one-that-is-no-longer-on-the-shelf",
        says: "The row a shop picks for the player when it refills the shelf is the first of the \
               chosen kind, claimed before the shelf drops anything -- so it can name a row the \
               player can no longer see, and it is not simply the first row left on the shelf.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O740-FIRST"),
        station: "dereth-testkit::dat::inventory::shop::scenario_the_picked_row_can_be_one_that_is_no_longer_on_the_shelf",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.shelf.the-shelf-never-scrolls-however-little-is-left-on-it",
        says: "A shop's shelf never has anywhere to scroll to, whether every row of the chosen kind \
               has been basketed away or none of them has: the list is padded out to the width of the \
               window either way and the padding is what it is counting.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O740-SCROLL"),
        station: "dereth-testkit::dat::inventory::shop::scenario_the_shelf_never_scrolls_however_little_is_left_on_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.shelf.what-a-row-stacks-to-comes-from-the-same-place-its-kind-does",
        says: "How big a stack a shop row can be sold in is read off the same thing its kind is read \
               off: a thing the player owns keeps its own answer and everything else takes the \
               shop's, so the two can never disagree about which thing they are describing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O740-MAX-FORK"),
        station: "dereth-testkit::dat::inventory::shop::scenario_what_a_row_stacks_to_comes_from_the_same_place_its_kind_does",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.shop.the-shards-own-message-opens-the-shop-and-replaces-it",
        says: "The shard opening a shop is what puts one on screen: the window belongs to the \
               merchant it names, it is in buying mode, and it lists that merchant's stock in the \
               order given with each row's name and how many of it there are. A second shop \
               message replaces the whole window rather than adding to it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O231"),
        station: "dereth-testkit::cpu::inventory::scenario_the_shards_message_opens_the_shop",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "vendor.stock.a-stack-tells-you-how-many-there-are-and-what-they-are-called",
        says: "A stack in a shop's stock says how many of it there are and calls them by their \
               plural name, the same way an item on the trade table does, so a player can read \
               what is on offer without buying it first.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O605-VENDOR"),
        station: "dereth-testkit::dat::inventory::scenario_a_stack_in_vendor_stock_is_counted_and_pluralised",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.stock.coming-back-to-the-stock-tab-draws-every-row-decorated-again",
        says: "Tabbing away from a shop's stock and back draws every row with its own finished \
               picture again, and not with the bare one the row was filled from. The first open \
               is the control in the same run, so drawing that was never right cannot be read as \
               drawing that stopped being right.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F63-TAB-RETURN"),
        station: "dereth-testkit::dat::inventory::scenario_coming_back_to_the_stock_tab_draws_every_row_decorated_again",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.stock.every-row-of-the-shop-is-a-real-thing-and-is-drawn-finished",
        says: "Every row of a shop's stock is a thing the client really holds once the window is up, \
               and every filled row is drawn with its finished picture -- the plate under the icon \
               that the player's own pack draws with -- and not with the bare picture out of the \
               shipped data.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F33-DECORATION"),
        station: "dereth-testkit::dat::inventory::shop::scenario_every_row_of_the_shop_is_a_real_thing_and_is_drawn_finished",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.stock.pressing-a-row-in-the-shops-own-window-picks-it",
        says: "Pressing a row of a shop's stock picks that thing, and it is the press that does it: \
               what was picked is cleared first, so a row that is picked afterwards was picked by the \
               press and not left over from the window opening.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F33-PRESS"),
        station: "dereth-testkit::dat::inventory::shop::scenario_pressing_a_row_in_the_shops_own_window_picks_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.stock.scroll-survives-a-repaint-and-a-refilter-shrinks-the-extent",
        says: "A shopkeeper's stock strip keeps its scroll position when the window repaints for a \
               newly picked row, and keeps its sideways position when a different shopkeeper's \
               stock opens; choosing a filter from the menu afterwards scrolls it back to the \
               first item.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-ITEMLIST-SCROLL-STOCK"),
        station: "dereth-ui-screens::dat::inventory::item_list_scroll::vendor_repaint_preserves_scroll_and_open_restores_x_after_its_real_menu_callback",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.stock.the-filter-strip-is-built-from-the-stock-that-is-actually-there",
        says: "A shop's category tabs are the kinds of thing it actually sells and no others, \
               each tab carrying the kind it stands for; a shop with nothing in it earns no tabs, \
               and a shop that has been closed keeps none, whatever it was showing before.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O604-FILTERS"),
        station: "dereth-testkit::dat::inventory::scenario_the_filter_strip_follows_the_stock",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.stock.the-numeral-is-the-amount-the-shop-advertises-on-the-row-the-player-picked",
        says: "The number painted over a stock row is how many the shop says it has, not how big \
               the stack is, and it is painted on the row the player has picked and on no other. \
               A shop that is out of that thing shows no number rather than a zero, and one with \
               an endless supply shows none either.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O605-QUANTITY"),
        station: "dereth-testkit::dat::inventory::scenario_the_vendor_numeral_is_the_advertised_amount",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.stock.the-picked-row-writes-its-name-and-its-cost-and-wakes-both-buttons",
        says: "Picking a row in a shop's stock writes its name and a line saying what it costs \
               and what the player has, and wakes the two buttons that act on it. Picking nothing \
               clears both lines and puts both buttons back to sleep.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F47-COST"),
        station: "dereth-testkit::dat::inventory::scenario_the_picked_stock_row_writes_its_name_and_its_cost",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.stock.the-strips-scrollbar-comes-alive-only-when-the-stock-overflows-and-its-arrow-scrolls-one-slot",
        says: "A shopkeeper's stock strip keeps its sideways scrollbar hidden and disabled while \
               the stock fits, and shows and enables it, with a thumb sized to the part on show, \
               once the stock overflows the strip.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1176-STOCK"),
        station: "dereth-ui-screens::dat::inventory::vendor_stock_scrollbar::the_bar_comes_alive_only_when_the_stock_overflows_the_strip",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.stock.what-is-drawn-follows-every-frozen-fact-about-an-item-and-not-the-clock",
        says: "A shop's stock list is redrawn whenever anything it draws about an item changes -- \
               how many there are, its overlay, its wear, its name, its plural name -- each one \
               on its own with the rest of the shop held still, and a cooldown counting down is \
               exempt for the same reason the trade table exempts it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O605-VENDOR-GUARD"),
        station: "dereth-testkit::dat::inventory::scenario_the_vendor_stock_follows_every_frozen_fact_and_not_the_clock",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.stock.what-is-on-the-shops-shelf-is-bought-and-never-used",
        says: "What is on a shop's shelf is bought and never used. Double-clicking a stock row does \
               not use it, and picking the row and then pressing the toolbar's own use button does \
               not use it either -- neither gesture sends anything to the shard.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F33-NOT-USED"),
        station: "dereth-testkit::dat::inventory::shop::scenario_what_is_on_the_shops_shelf_is_bought_and_never_used",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.tabs.something-carried-over-the-window-turns-it-to-the-selling-tab",
        says: "Carrying something over a shop's window turns it to the Selling tab by itself, \
               whichever page it was on and wherever in the window the pointer is, so the thing \
               arrives at the sell list. The pointer over the window with nothing carried leaves \
               the page alone.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-SELL-TAB-DRAG"),
        station: "dereth-testkit::dat::inventory::scenario_something_carried_over_the_window_turns_it_to_the_selling_tab",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.window.the-shards-own-message-puts-the-shop-on-screen-and-not-only-its-tabs",
        says: "The shard's own shop message puts the shop on screen: the shop's page is visible \
               and the floating window that carries it has been raised, not merely its strip of \
               tabs. The model half is asserted first in the same run -- the message names the \
               shopkeeper, one shop was opened, the model says a shop is open and the window \
               found its lists -- so a failure to be on screen cannot be blamed on the decoding.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F22-SHOP-ON-SCREEN"),
        station: "dereth-testkit::dat::inventory::scenario_the_shards_own_message_puts_the_shop_on_screen_and_not_only_its_tabs",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vendor.window.the-shop-and-the-ground-container-cannot-both-be-open",
        says: "A shop opening closes the container the player had open on the ground and tells \
               the shard they have stopped viewing it, and opening a different ground container \
               closes the shop before anything of that container arrives. Re-opening the very \
               container that is already open changes neither.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O231-GROUND"),
        station: "dereth-testkit::cpu::inventory::scenario_a_shop_and_a_ground_container_cannot_share",
        tier: Tier::Cpu,
    },
];
