# The conformance suite

Provenance: **network specification**.

Implemented in `network/conformance/` (`dereth-network-conformance`, MIT). A world or hub that
fails a check is not conformant. Passing every check is necessary, not sufficient: the checks hold
an implementation to the rules most easily got wrong, not to every rule of the other pages, which
stay the definition.

## 1. The vectors

Nine JSON files in `network/conformance/vectors/`, readable from any language:

| file | pins |
|---|---|
| `addresses.json` | addresses that parse (with their halves and identity key), addresses that must be refused, and the reserved slugs |
| `text.json` | lines in and out of the text rules (one, `LONG`, stands for 300 `x` and expects the first 255; `input_parts` and `output_parts` stand for their pieces joined, each repeated), names in and out or refused; each output must pass the rules again unchanged |
| `messages.json` | a frame of every message type that must read, whose writing must hold every field of the frame with the same value and read back unchanged, with the values some fields must read as (`expect`, by each field's meaning); frames of unknown types that must be skipped; and malformed frames that must be refused. `${c*N}` in a frame stands for the character c written N times. |
| `directory.json` | the directory's messages ([02](02-link-protocol.md) section 7): frames that must read, write back covering every field and read back unchanged, with the values some fields must read as (a profile's by their path, a list's by its length); a later version's frames that must read, leaving out what they add; frames a reader of the directory alone skips; and frames that must be refused, every bound of a profile among them |
| `reach.json` | the reach messages ([02](02-link-protocol.md) section 8): frames that must read, write back covering every field and read back unchanged, with the values some fields must read as; a later version's frames that must read, leaving out what they add; frames a reader of the reach messages alone skips, the other link messages and the directory's among them; and frames that must be refused, a channel in place of a character among them |
| `say.json` | the say messages ([02](02-link-protocol.md) section 9): frames that must read, write back covering every field and read back unchanged, with the values some fields must read as, a reason this version does not name among them; a later version's frames that must read, leaving out what they add; frames a reader of the say messages alone skips, the room's marked lines and the reach messages among them; and frames that must be refused, a channel in place of a character, a mark that is not a word and a reason with `said` among them; and names a player gives with the mark a line said from Discord shows beside its speaker, each with the name it gives once the mark is off ([03](03-world-behaviour.md) section 1) |
| `rooms.json` | the rooms messages ([02](02-link-protocol.md) section 10): frames that must read, write back covering every field and read back unchanged, with the values some fields must read as; a later version's frames that must read, leaving out what they add; frames a reader of the rooms messages alone skips, the other link messages among them; and frames that must be refused, a room that is none of a world's own, a mirrored room twice or carrying the Global room, and a seed that is not true or false among them |
| `web.json` | a world's web addresses ([02](02-link-protocol.md) section 2): `hello` frames that name them or not, which a hub must read with the values given (null: not named); and the WebSocket endpoints and status documents a hub keeps, and those it must not keep, every bound of the rule among them |
| `challenge.json` | challenge datagrams ([05](05-hub.md) section 2.5), in hex: those that must read as their slug and nonce and be written again byte for byte from them, a later version's that must read ignoring what it appends, and those that must not read as a challenge (the status ping's magic, another kind, version 0, a size a packet could have, a datagram cut short, a slug that cannot be carried) |

`dereth-network-conformance vectors` runs them against this repository's protocol crate.

## 2. The hub checks

`dereth-network-conformance hub --url ws://host:port --world <slug>=<key> --world <slug>=<key> [--world ...] [--off <slug>=<key>] [--slow]`
plays two or three worlds against a running hub, each check over links of its own, with names of
its own. The directory's checks also read the hub's public routes ([05](05-hub.md) section 6.3)
at the same address, as a page on another origin reads them, and take out every character they
list before they end. A hub that answers a listing `unavailable` keeps no directory, and is not
held to them.

| check | what it holds the hub to |
|---|---|
| `link.a_wrong_key_is_refused_401` | a key that is not the world's is refused before the upgrade |
| `link.a_world_whose_flag_is_not_on_is_refused_403` | a world not `on` may not link (with `--off`) |
| `link.hello_is_welcomed_with_the_worlds_own_slug` | `welcome` answers `hello`, naming the world |
| `link.anything_before_hello_is_refused` | `error {order}`, then the link is closed |
| `link.another_protocol_version_is_refused` | `error {protocol}`, then closed |
| `link.an_unknown_type_is_skipped` | the link carries on |
| `link.a_second_link_replaces_the_first` | the first is closed, the second stays |
| `link.a_binary_or_oversized_frame_closes_the_link` | either closes it |
| `rooms.a_world_is_told_its_rooms_after_its_welcome` | `rooms` follows `welcome` |
| `rooms.a_worlds_global_room_is_set_and_a_seed_taken_only_while_none_is_held` | `rooms.global` is answered under its id with the room it set, never among those mirrored; a seed while the hub holds a room changes nothing (it leaves the first world carrying the Global room in LFG) |
| `rooms.a_worlds_rooms_message_is_refused_as_the_hubs_own` | `error {malformed}` for a `rooms` a world sends |
| `identity.a_world_cannot_watch_say_or_ask_as_another_worlds_character` | `error {identity}` for a spoofed `watch.owner`, `room.say.from` and `channel.by`, and nothing forwarded |
| `world.a_link_up_and_down_is_announced` | `world {online}` both ways |
| `link.a_worlds_web_addresses_are_listed_only_while_its_link_is_up` | a world's `/v1/servers/<slug>` names the `websocketUrl` and `statusUrl` its `hello` named while its link is up, and `null` once it is down; a plain address to another machine is never named |
| `tell.is_routed_and_answered_under_the_senders_id` | the target gets the tell under an id of the hub's own, not the sender's; the sender the answer under its own |
| `tell.a_world_cannot_send_as_another_worlds_character` | `error {identity}`, nothing forwarded |
| `tell.to_a_world_not_on_the_network_is_not_on_network` | an unknown world, and an `off` one |
| `tell.a_resent_id_is_delivered_once` | duplicates are dropped |
| `tell.held_for_a_world_that_reconnects` | the one-minute hold |
| `limits.a_characters_tells_are_rate_limited` | the per-character limit, at its burst as `welcome.limits` gives it |
| `presence.watches_and_presence_go_between_the_named_worlds` | and a world cannot speak for another's characters |
| `room.a_line_reaches_the_other_worlds_carrying_the_room` | and not back to the world that said it, and (with a third world) not those that do not carry it, which may not say anything there |
| `channel.joined_by_invitation_and_its_lines_reach_the_other_members` | the channel lifecycle and its lines; a non-member can neither invite nor tell the channel; the last leave ends it |
| `tell.an_unanswered_tell_is_answered_unavailable` | the hub answers within its timer (with `--slow`: about 30 seconds) |
| `tell.a_held_tell_lapses_unavailable` | a tell held for a world that never returns is answered `unavailable` after the hold (with `--slow`: about a minute) |
| `person.a_world_cannot_ask_for_another_worlds_character` | `error {identity}` for a `person` or `person.watch` naming another world's character, and no answer |
| `person.a_character_nobody_linked_has_no_person` | its view is unlinked and empty; `characters` names none; `befriend`, `carry` and `alts` are `not_linked` |
| `person.a_link_code_is_well_formed_or_refused_unavailable` | `ok` with a well-formed code that lapses within ten minutes and a link, or `unavailable` from a hub without sign-in |
| `listing.a_world_cannot_list_another_worlds_character` | `error {identity}` for a listing of another world's character, and no answer |
| `listing.a_listing_is_answered_and_taking_it_out_is_ok` | a listing of the world's own character is `ok` with the character's page, and taking it out `ok`; or both `unavailable` from a hub that keeps no directory |
| `directory.a_listing_is_public_at_once_and_gone_at_once` | a listing shows on the character's route at once, with its fields, the hour it changed and no finer time, readable by any origin and cacheable for a minute, and found by a search of its name, its address matched without regard to case; taken out, its route answers `404` and no search finds it, at once |
| `directory.absence_is_one_404` | a character taken out, one never listed, one of a world not on the network (and, with `--off`, one of a world whose flag is not on), a channel, text that is no address and a name no character may have answer the same `404`, kept by no cache; an allegiance whose last listed member was taken out, a key that names none and one that is no key, likewise |
| `directory.a_patron_shows_only_while_listed` | a listed member's patron and monarch are named only while each is listed itself, and an unlisted one's name is nowhere in the answer; one taken out is struck from the listings that named it, so listed again it is named once its vassal's world names it again |
| `directory.an_allegiances_listed_members_come_highest_rank_first` | an allegiance's answer gives its key, name, size and how many are listed, its listed members highest rank first and then by name, each with its patron while listed, paged as a search is |
| `directory.only_the_profiles_own_fields_are_published` | a listing carrying fields no profile names (online, last seen, an account, a house, a person), at its top, in its profile and in a skill, is published with none of them; an option-governed field the world left out is not published; a search's results carry only a result's fields |
| `directory.a_search_is_paged_within_its_first_thousand_results` | `limit` and `offset` page a search, `nextOffset` naming the next page until the last; a page at the end of the first 1,000 is answered, empty past the results |
| `directory.a_search_field_out_of_its_bounds_is_400` | `q` of one letter or of a character no name holds, a `world` that is no slug, a level or heritage out of its bounds, `level_min` past `level_max`, `sort` neither `updated` nor `level`, `limit` 0 or 51, and `offset` and `limit` past 1,000 are each `400`, the message naming the field; every field at its bound is answered |
| `directory.an_unchanged_profile_keeps_its_hour` | a profile sent again unchanged changes nothing of its answer, not even the hour, two hours later; changed, it is published with the hour of the change |
| `directory.a_search_sorts_by_the_latest_change_or_by_level_then_by_name` | by default the profiles that changed most lately come first, characters listed hours apart in that order; `sort=level` the highest first; either way then by name, over every world |

The hour a profile last changed is the hub's, so two of the directory's checks need hours to pass
on the hub's clock. The hub's own tests let them pass (`hub::HubClock`); against a running hub,
those two hold what one hour shows.

## 3. The world checks

The suite plays the hub against a world through `world::WorldUnderTest`, a Rust trait: carry
frames each way, let the world's time pass, and act as and look at two of its characters online. A
Rust implementation writes that adapter in its own tests. **For now the world checks run only for
a world that can be driven from Rust**; a world in another language (the ACE mod, in C#) is held to
the vectors and to its own checks of the same behaviour, against a real hub. The checks:

| check | what it holds the world to |
|---|---|
| `link.hello_comes_first_then_the_room_and_the_watches` | `hello` first and alone; after `welcome`, `room.join` (for a world that carries the room) and a watch for each remote friend; on a new link, both again after its `welcome` |
| `tell.to_a_character_it_does_not_have_is_offline` | `offline` |
| `tell.to_a_character_off_the_network_is_offline_and_unseen` | `offline`, nothing shown |
| `tell.a_character_on_the_network_is_shown_it_from_a_stand_in_id` | shown under the sender's address and a stand-in id, the same id for the same identity; `delivered` |
| `tell.a_character_on_the_network_is_reachable_by_anyone` | `@dn on` makes a character reachable by anyone: a stranger's tell is shown and `delivered` |
| `tell.the_sender_is_shown_each_outcome_as_a_local_tell_would_show_it` | `afk` (the line written around the away text), `unavailable`, `not_on_network`, `rate_limited` |
| `text.a_remote_line_is_shown_only_after_the_text_rules` | markup out, Windows-1252 in, line breaks to spaces |
| `tell.a_local_sender_sees_the_echo_only_once_the_hub_answers` | the tell goes as the character's address; the echo waits for `delivered` |
| `tell.an_offline_answer_is_character_not_available` | the native error |
| `tell.a_reply_by_id_goes_to_the_stand_in_ids_identity` | the by-id reply path |
| `tell.by_name_to_an_away_linked_character_of_its_own_goes_to_the_hub` | a tell by name to one of its characters away, on the network and linked goes to the hub from the sender's address; an `offline` answer is `Character not available` |
| `tell.by_name_to_an_online_character_of_its_own_stays_local` | a tell by name to one of its characters online is the world's own: nothing to the hub, shown under the sender's own id |
| `tell.a_reply_as_an_away_linked_character_of_its_own_is_shown` | a tell from its own address, as one of its characters away, on the network and linked, is shown under that address and a stand-in id; `delivered` |
| `tell.from_its_own_character_in_the_world_is_offline_and_unseen` | a tell from its own address, as one of its characters in the world, is `offline` and shown to nobody |
| `squelch.a_squelched_sender_is_answered_squelched_and_unseen` | remote squelch |
| `presence.a_character_on_the_network_shows_online_to_anyone_who_watches` | a stranger's watch is answered online, and offline once the character goes off the network |
| `presence.a_friend_shows_as_its_home_world_says` | a status row under the stand-in id |
| `room.a_line_from_another_world_is_shown_to_listeners` | the Global room's slot |
| `room.a_line_from_another_world_passes_the_text_rules` | markup out, line breaks to spaces |
| `link.a_tell_queued_while_down_goes_after_the_next_welcome` | the queue, and `hello` before it |
| `person.a_character_on_the_network_asks_for_its_person_view` | `person.watch` for its own address once it is on the network |
| `person.a_link_is_asked_for_the_character_and_shown` | `@dn link` asks for the character's own address, and shows the link the hub gives, whole, in one line |
| `person.a_link_holds_only_for_the_character_that_asked_for_its_code` | a view saying a character that never asked for a code is linked lists nothing, and the world sends `unlink` for it |
| `person.a_person_friends_characters_are_listed_and_watched` | for a character that asked for a code, each character a view names becomes a friends row and is watched; the character itself and this world's characters are not listed |
| `person.a_listed_character_comes_online_as_its_person` | *Bob@remote is online as Alt@remote.* |
| `person.a_linked_characters_friend_is_asked_of_the_hub` | an unlinked character's `@friends add` asks the hub nothing; a linked one's is `befriend` |
| `person.linking_carries_the_characters_friends_to_its_person` | the first linked view brings `carry` with the friends the world kept; a later view listing one as a handle neither adds nor removes its row |
| `person.friends_handed_back_on_unlinking_are_the_characters_own` | an unlinked view naming a friend keeps its row, and a later empty view does not remove it |
| `person.a_character_the_view_no_longer_names_is_removed` | a row of type 2, and its watch stops |
| `listing.nothing_is_sent_while_off_the_network` | no listing for a character off the network, at login, a level gained or logout |
| `listing.going_on_the_network_sends_the_characters_own_profile` | `@dn on` sends a listing of the character's own address, with its level, heritage and gender, and the lines of section 6 |
| `listing.a_field_its_option_hides_is_never_sent` | with each "Allow others to see your …" option off, its field is left out; turned on, it is sent, as appraising the character shows it |
| `listing.changes_go_at_most_every_five_minutes_and_at_once_at_logout` | changes within five minutes of a listing wait, and only the latest goes; logout sends at once |
| `listing.taking_out_goes_at_once_and_is_kept_until_answered` | `@dn off` sends a listing that is not `on` at once, sent again on the next link until the hub answers it |
| `listing.an_off_from_the_hub_holds_the_character_out_until_its_next_login` | `listing.off` is answered with a listing that is not `on` and shows the player the line of section 10, at once or at its next login; nothing more is sent for the character until its next login, which lists it again |
| `listing.an_off_by_its_person_takes_the_character_off_the_network` | `listing.off` by `person` is answered with a listing that is not `on` and shows the player the line of section 10, at once or at its next login; the character is off the network: a tell to it is answered `offline`, `@dn` says it is off, and its next login lists nothing |
| `listing.only_listed_patrons_and_monarchs_are_named` | a patron or monarch off the network is never named, and an allegiance its monarch did not name is sent with no name; once the patron goes on the network, its vassal's next listing names it, and once it goes off, no longer |
| `listing.a_refusal_holds_the_character_out_until_its_next_login` | `taken_down` and `full` show their lines and send nothing more for the character until its next login; `rate_limited` sends the latest profile again a minute later |

For the directory's checks the adapter also lets a character log out and in again, gain a level,
set the six fields its "Allow others to see your …" options govern and those options, and swear
allegiance to the other, and carries the directory's messages apart from the link's.

## 4. Who runs what

- **The hub** (`network/hub`) runs the hub checks against itself, over real WebSockets and its
  HTTP routes on loopback, letting hours pass on its own clock, in its own tests, and the person
  layer's site routes and privacy rules, the rooms it mirrors into Discord and the tells it carries
  there, with a stand-in for Discord.
- **Empyrean** runs the world checks against its network half in process
  (`empyrean/testkit/tests/all/federation/conformance.rs`).
- **The ACE mod** (`network/ace-mod`) runs the vector files against its own C# code, the
  directory's among them, and
  its world half, which has no ACE dependency, against a real hub over loopback with two in-memory
  worlds, with its own checks of what the world checks hold a world to; a separate check resolves
  and applies every one of its patches against ACE's own assemblies. Only the patched calls inside
  a running ACE world need a live ACE world, which the mod's README describes.
