# The link protocol

Provenance: **network specification**.

One implementation: `network/protocol/src/wire.rs` (the messages), `directory.rs` (the
directory's messages, section 7), `reach.rs` (the reach messages, section 8), `say.rs` (the say
messages, section 9), `rooms.rs` (the rooms messages, section 10) and `limits.rs`, with the hub's
side in `network/hub/src/core.rs`. The vectors in `network/conformance/vectors/messages.json`,
`directory.json`, `reach.json`, `say.json` and `rooms.json` give a frame of every message, with the
values its fields must read as.

## 1. Transport and authentication

- **One WebSocket per world**, opened by the world: `GET /v1/servers/<slug>/link` on the hub,
  upgraded, with the world's registry key as `Authorization: Bearer drth_...`. Production hubs are
  reached over `wss://`; plain `ws://` is for loopback only.
- **Before the upgrade** the hub answers `401` for a key that is not the world's current key and
  `403` for a world whose federation flag is not `on`. A world that gets either must not retry
  faster than once a minute.
- **A link lasts as long as its key.** When the world's key is rotated or revoked, the hub closes
  every link opened with the old one, as it does when the flag leaves `on`.
- **A second link** from the same world replaces the first, which the hub closes.
- **Frames.** One JSON object per text frame, at most 16 KiB, either way: the hub never sends a
  longer one (it names the worlds in `welcome` only as far as they fit, and cuts a channel's member
  list to fit; everything else it hands on it read within the field rules below, which keep every
  field it forwards short enough). A binary frame, or a longer one, closes the link.
- **Keep-alive.** The hub pings every 20 seconds and closes a link it has heard nothing on for 70.
  Both are well inside the idle timeout of a proxy in front of the hub (Cloudflare closes a
  connection idle for 100 seconds), so a quiet link is never cut on the way. A world should
  reconnect after a lost link with growing waits (1, 2, 5, 10, then every 30 seconds).
- **A world that stops taking its frames** is dropped as one that falls silent is: once a write to
  it has waited 10 seconds, or at once when the frames waiting for it pass the hub's bound (the
  reference hub's: 4 MiB). Either way the other worlds are told it is offline. A world likewise
  drops a hub that sends faster than it takes the frames, and links again.

## 2. Messages

Every message is an object whose `type` names it. A message of a type the reader does not know is
skipped, not an error, so a later version can add types; a known type with a missing or bad field
is malformed. Addresses are strings in the form of [01-addresses-and-settings.md](01-addresses-and-settings.md).

The **hub fills or checks** every field that names the sending world: a world may speak only for
its own characters. A message whose own-world field names another world is refused with
`error {code: "identity"}` and goes nowhere.

| type | from → to | fields | means |
|---|---|---|---|
| `hello` | world → hub | `proto` (1), `software` | first message on a link |
| `welcome` | hub → world | `proto`, `world` (the world's own slug), `hub`, `worlds` [{`slug`, `name`, `online`}], `rooms`, `limits` | the answer to `hello`; `worlds` names the worlds linked now, as many as fit the frame, and a `world` message follows for each of the rest |
| `world` | hub → world | `slug`, `name`, `online` | a federated world's link came up or went down (or, after `welcome`, is up) |
| `status` | world → hub | `state` (`online`, `maintenance`, `degraded`, `offline`), `players_online`? | the registry heartbeat, about once a minute |
| `watch` | world → hub → world | `owner`, `target`, `on` | `owner` lists `target` as a friend and is online (`on`), or stops |
| `presence` | world → hub → world | `to`, `of`, `online` | `of`'s home world tells watcher `to` what it may see |
| `tell` | world → hub → world | `id`, `from`, `to`, `text`, `friend`? | a tell; `friend`: the sender lists the recipient |
| `tell.result` | world → hub → world | `id`, `outcome`, `message`? | how a tell ended (`message`, for `afk`: the recipient's away text alone; the sending world writes the line around it) |
| `room.join` | world → hub | `room`, `on` | the world carries the room, or stops |
| `room.say` | world → hub | `id`, `room`, `from`, `text`, `via`? | a line one of its characters said in the room; `via`: its person said it elsewhere (section 9) |
| `room.line` | hub → world | `room`, `from`, `text`, `via`? | another world's line in a room this world carries, with its mark |
| `room.result` | hub → world | `id`, `outcome` | how a `room.say` ended |
| `channel` | world → hub | `id`, `op`, `channel`, `by`, `who`? | a channel request (`create`, `invite`, `join`, `leave`, `list`, `members`) |
| `channel.result` | hub → world | `id`, `outcome`, `channel`, `members`?, `channels`? | how it ended |
| `channel.invite` | hub → world | `channel`, `by`, `to` | one of the world's characters is invited |
| `channel.line` | hub → world | `channel`, `from`, `to`, `text` | a channel line for one member on the world |
| `person` | world → hub | `id`, `op`, `who`, `target`?, `targets`?, `shown`? | a request about the person layer for one of the world's characters (`code`, `unlink`, `befriend`, `unfriend`, `carry`, and from worlds of the first version `alts` and `characters`; section 6) |
| `person.result` | hub → world | `id`, `outcome`, `code`?, `expires_secs`?, `url`?, `characters`? | how it ended |
| `person.watch` | world → hub | `who`, `on` | `who` is online and on the network (`on`): send its person view now and whenever it changes; or stop |
| `person.friends` | hub → world | `to`, `linked`, `alts`, `persons` [{`handle`, `characters`}] | `to`'s person view: whether it is linked, and the person friends that hold for it, each with the characters it lists (for a character nobody links, the friends handed back to it); `alts` is always `shown`, for worlds of the first version |
| `listing` | world → hub | `id`, `who`, `on`, `profile`? | `who` is listed, with its profile as it is now (`on`), or is listed no longer (section 7) |
| `listing.result` | hub → world | `id`, `outcome`, `reason`?, `url`? | how a listing ended, and the character's page once it is listed |
| `listing.off` | hub → world | `who`, `by` (`operator`, `person`) | `who` was taken out of the directory: by the hub's operator, and its world holds it out until its next login; or by its person, forgetting itself, and its world takes it off the network |
| `reach` | hub → world | `id`, `from`, `to` | a tell from `from` to `to`, which this world answered `offline`: would `to`, not online, have been shown it? (section 8) |
| `reach.result` | world → hub | `id`, `reachable` | the answer: `to` is not online and would have been shown it, or not |
| `say` | hub → world | `id`, `room`, `who`, `text`, `via` | `who`'s person said `text` in `room` elsewhere (`via`): say it as `who` (section 9) |
| `say.result` | world → hub | `id`, `outcome`, `reason`? | `said`, or `refused` and why |
| `rooms` | hub → world | `id`?, `global`?, `mirrored` | the world's rooms: which of its own rooms carries the Global room (`off`: none; left out: the hub holds none for the world), and those the hub mirrors into Discord for it; `id`: the `rooms.global` it answers (section 10) |
| `rooms.global` | world → hub | `id`, `room`, `seed`? | carry the Global room in `room` (`general`, `trade`, `lfg`, `roleplay`, or `off`) from now on; `seed`: only while the hub holds none for the world (section 10) |
| `error` | either | `code` (`identity`, `order`, `protocol`, `malformed`), `message` | the other side broke the protocol |

An example tell, and its answer:

```json
{"type":"tell","id":"t1790000000-7","from":"Ann@frost","to":"Bob@coldeve","text":"still playing?","friend":true}
{"type":"tell.result","id":"t1790000000-7","outcome":"delivered"}
```

### Field rules

- **Ids** are 1 to 64 characters, chosen by the sender, unique among its messages for at least 5
  minutes. A world should start them with something that changes at each start (its start time),
  since the hub remembers ids it has seen for 5 minutes and drops a repeat as a resend.
- **Text** is at most 1,024 characters on the wire, a `tell.result`'s away text (`message`) as a
  tell's text; a longer one is malformed. The receiving world cuts it to its own limit
  ([04-text-rules.md](04-text-rules.md)), and a world cuts its own characters' away text to 1,024
  before it sends it.
- **A status's `state`** is one of `online`, `maintenance`, `degraded` and `offline`; anything else
  is malformed.
- **A line's mark** (`via`, on `room.say` and `room.line`) says where its speaker's person said
  it, when that was not in the game: 2 to 20 of `a-z`, `0-9` and inner hyphens, as a room's name.
  This version names `discord`. A world that predates it ignores it, as any field it does not
  know, and shows the line without it.
- **Rooms**: 2 to 20 of `a-z`, `0-9` and inner hyphens. The network has one room, `global`. A world
  may also carry rooms of its own, which the hub mirrors into Discord for it alone while its
  operator asks for them ([05](05-hub.md) section 5.7), as the hub tells it (section 10): it joins
  one by its name as it joins `global` (the hub keeps at most 10 such joins for one link), and a
  `room.say` in it goes to Discord alone; in one the hub does not mirror for the world, it is
  answered `not_member`.
- **Channels**: 2 to 20 of `a-z`, `A-Z`, `0-9` and inner hyphens, in `channel`, `channel.invite` and
  `channel.line`, and in a `channel.result`'s `channel` and every name in its `channels`, which a
  world shows its players.
- **`watch.owner`, `presence.of`, `tell.from`, `room.say.from`, `channel.by`, `person.who`,
  `person.watch.who` and `listing.who`** must be the sending world's own characters.
- **A person request's `op`** is one of `code`, `unlink`, `alts`, `befriend`, `unfriend`,
  `characters` and `carry`. `befriend`, `unfriend` and `characters` need a `target`, which names a
  character, never a channel; `carry` needs `targets`, 1 to 100 characters, none a channel; `alts`
  needs `shown` (true or false). Worlds of this version never send `alts` or `characters`.
- **A person result's** `code` is two groups of four of `0-9` and `A-Z` without `I`, `L`, `O` and
  `U`, joined by a hyphen (`K7QD-2MXP`); `expires_secs` is a whole number of seconds, at most 3,600;
  `url` is 1 to 200 printable ASCII characters with no space, `<`, `>`, `"` or `'`; `characters`
  names at most 20 characters, none of them a channel.
- **A person view's** `alts` is a word, which the hub always writes `shown` and a world of this
  version ignores; `persons` holds at most 100 person friends (a person may list more, section 6),
  each with a `handle` and at most 20 `characters`, and at most 100 characters in all; none of them
  is a channel.

### Errors

| code | when | then |
|---|---|---|
| `order` | a message before `hello` | the hub closes the link |
| `order` | a second `hello` | the link stays up; the second is ignored |
| `protocol` | a `hello` naming a version the hub does not speak | the hub closes the link |
| `identity` | a message speaking for another world's character | the link stays up; the message goes nowhere |
| `malformed` | a frame that does not read as a message, or a message only the hub sends | the link stays up; the frame goes nowhere |

A message of a type the reader does not know is skipped without an error.

### Outcomes

| outcome | for | means | the sending world shows (see [03](03-world-behaviour.md)) |
|---|---|---|---|
| `delivered` | tell | shown to the recipient (for a channel, to every member online) | the echo |
| `afk` | tell | shown; the recipient is away; `message` is the away text | the echo and the away message |
| `squelched` | tell | not shown: the recipient squelches the sender | what a local squelch shows |
| `offline` | tell | not shown: not online, not on the network, does not allow the sender, or the receiving world refuses it — one answer for all, so nobody can learn which | `Character not available` |
| `unavailable` | tell | not shown: the recipient's world is on the network but unreachable now | `Character not available`, and that the world can't be reached |
| `not_on_network` | tell | not shown: the recipient's world is unknown, opted out or never opted in | `Character not available`, and that the world isn't on the network |
| `rate_limited` | tell, room | not sent or not shown: over a limit the sender's world shares (the hub's, or one the receiving world keeps for a tell it would otherwise show) | that the network is holding back the world's tells or lines for now |
| `not_member` | tell to a channel, room | not sent: not a member of the channel, or the world does not carry the room | that the sender is not a member |
| `ok` | room, channel | accepted | nothing (room) / the request's answer (channel) |

Channel outcomes: `ok`, `exists`, `no_channel`, `not_member`, `not_invited`, `already_member`,
`refused`.

Person outcomes: `ok`; `not_linked` (the request needs `who` to be linked to a person, and it is
not); `rate_limited` (over a limit of section 5); `unavailable` (this hub does not link characters:
it has no sign-in); `refused` (a limit on the person, such as how many person friends it may
have).

## 3. The order of a link

1. The world connects and sends `hello`. Anything else first is `error {code: "order"}` and the
   link is closed; a `proto` the hub does not speak is `error {code: "protocol"}` and closed.
2. The hub sends `welcome`, then the world's rooms (`rooms`, section 10), and tells every other
   world `world {online: true}`.
3. The world sends `room.join` for each room it carries, then whatever it queued while it had no
   link, then a `person.watch` for each of its characters online and on the network, and a `watch`
   for each remote friend of each of them on a world `welcome` named. Throughout a link, a world sends its own tells, room
   lines and channel requests first, then the answers it owes other worlds, a world at a time in
   turn, then watches and presence answers, also a world at a time in turn and to each world at
   most at that world's share of the pace for watches (section 5), so a flood of the later never
   holds up the earlier, and a world that floods this one with watches delays only its own
   answers. A world owes any one other world at most 100 answers at once: a tell from a world
   already owed that many is neither shown nor answered (the hub answers it `unavailable`), so a
   world that floods another delays only its own answers.
4. When a world's link goes, the hub tells every other world `world {online: false}`. A world that
   hears that shows every friend from that world offline, without the online or offline lines,
   and forgets that world's watches of its own characters (they come again). When it hears
   `world {online: true}` for a world it did not know online, it sends its watches for friends on
   that world.

## 4. Who answers what

- **A tell** goes to the hub, which forwards it to the target world under an id of its own (never
  the sender's) and answers the sender's world with the target's `tell.result`, under the
  sender's id. The target world must answer every tell it is handed, within 30 seconds (one from a
  world already owed its share of answers, section 3, it leaves unanswered); if it does not, or its
  link goes first, the hub answers `unavailable` for it.
- **A tell for a federated world whose link is down** is held by the hub for up to **60 seconds**,
  and forwarded if the world's link comes back in time; otherwise the hub answers `unavailable`.
  This is the only time the hub keeps a body, and it keeps it in memory only.
- **A tell for a world that is not on the network** is answered `not_on_network` at once.
- **A world whose link is down** queues what it would send (tells, room lines, channel requests) for
  up to **60 seconds**, sends them in order when the link is back, and otherwise drops them,
  telling each tell's sender the network is unavailable. What it had handed to a link that went
  before an answer came is queued again at the front, since it may never have reached the hub.
- **A world gives up on a tell** not answered within `limits.result_timeout_secs` (**100
  seconds**) of handing it to the hub. That is longer than the hub's hold and the target's answer
  window together (60 + 30), so the hub has always answered first; a hub must not give a
  `result_timeout_secs` shorter than its hold plus 30 seconds.
- **Presence and watches** are not queued: they are sent again after a reconnect, and so is
  `person.watch`. The hub forgets a world's `person.watch`es when its link goes.
- **A person request** is answered by the hub at once, and queued by a world whose link is down
  as a channel request is.
- **A reach question** (section 8) is answered by the world at once; the hub forgets one not
  answered within 30 seconds, or whose world's link goes first. Neither side queues one.
- **A say** (section 9) is answered by the world at once; the hub gives up on one not answered
  within 10 seconds, or whose world's link goes first. Neither side queues one.
- **A rooms request** (`rooms.global`, section 10) is answered by the hub at once with the world's
  rooms. Neither side queues one: a world whose link is down sends none, and one a lost link took is
  not sent again (a seed goes again on the next link, while the hub still holds no room).
- **A listing** is answered by the hub at once. It is not queued with the rest: a world keeps,
  for each listed character, whether the hub has taken its latest profile, and for a character
  taken out, the `listing` that says so, and sends them when its link is back (section 7).
- **Delivery is at least once.** A world resending after a reconnect may repeat a message; the hub
  takes a repeated `(world, id)` of a tell, a room line, a channel request or a person request
  seen in the last 5 minutes only once, and answers the repeat with the answer it gave the first,
  once it has given one (an answer that reached a world whose link had gone is not lost; a repeated `list` or
  `members` is asked again).

## 5. Rate limits

The hub enforces, and tells each world in `welcome.limits`:

| limit | default |
|---|---|
| tells per character (to characters and channels) | a burst of 10, then one every 2 seconds |
| room lines per character | a burst of 3, then one every 5 seconds |
| messages per world, all characters together | a burst of 200, then 50 a second |
| room lines per world, all characters together | a burst of 10, then one a second |
| tells per world to one recipient, all characters together | a burst of 20, then one a second |
| tells per world to any one other world, all characters together | a quarter of messages per world: a burst of 50, then 12.5 a second |
| watches per world to any one other world, all characters together | the same quarter; a watch over it goes nowhere (a watch stopping one goes always) |
| channel lines and invitations | each is held to the limits of a tell to its recipient: per recipient, and the world pair's share; a channel line held back is not sent (the channel tell is answered `rate_limited` only when every line was), an invitation held back is refused |
| the loop guard | when two correspondents have each sent the other more than 20 tells within 60 seconds, tells between them are refused for 5 minutes |
| person requests per character | as its tells: a burst of 10, then one every 2 seconds; an `unfriend`, which only takes away, is not held to it |
| link codes | 5 an hour for one character; at most 200 given to one world's characters in any ten minutes, spent or not |
| `person.watch`es held for one world | 2,000; one past it is not answered |
| listings per character | a burst of 5, then one a minute; one that takes the character out is not held to it |
| listings per world, all characters together | a burst of 50, then 10 a second |
| listings across the hub, all worlds together | a burst of 200, then 100 a second |

The hub knows a character only by the name its world gives it, so the per-character limits hold
only for an honest world; the per-world ones hold for any. Every tell the hub hands a world is
owed an answer, which counts against that world's own messages per world; the share one world may
send any one other world keeps it from sending more than that world can answer; a watch is owed a
`presence` answer the same way. Neither share is a field of its own: both follow
`messages_per_world`. A world should pace each of its own characters' sends by the per-character
limits too, and answer one over them itself, and pace its watches and presence answers to each
other world under that world's share.

`welcome.limits` is an object of these fields; a rate is `{burst, per_sec}`:

| field | type | |
|---|---|---|
| `tells_per_character` | rate | required |
| `room_lines_per_character` | rate | required |
| `messages_per_world` | rate | required |
| `room_lines_per_world` | rate | optional: a burst of 10, one a second |
| `tells_per_recipient` | rate | optional: a burst of 20, one a second |
| `pair_burst` | integer | required |
| `pair_window_secs`, `pair_block_secs` | number | required |
| `hold_secs` | number | required: the hub's hold for a reconnecting world |
| `result_timeout_secs` | number | required: a world's give-up on a tell (above) |

The listings' paces are this version's own and not in `welcome.limits`: a world paces its listings
by the table's values. So is the pace of a world's `rooms.global` (section 10): a burst of 5, then
one every 10 seconds; one over it changes nothing, and is answered with the world's rooms as they
are.

A field the reader does not know is ignored; a `welcome` without a required field is malformed.

A redemption counts against the messages per world, and is answered `rate_limited` over them.
A tell, a room line, a person request or a listing over a limit is answered `rate_limited`;
anything else over the world's limit is dropped. **A world must pace what it sends** under `welcome.limits.messages_per_world`, holding
the rest for a moment, so that a burst (every watch at a reconnect, say) is never dropped at the
hub. The loop guard catches two automated answerers replying to each other; an away
message is an answer to a tell, never a tell, so it cannot start a loop.

## 6. The person layer

The hub keeps persons ([01](01-addresses-and-settings.md) section 5; [05](05-hub.md) section 5). A
world asks on its own characters' behalf, and the hub takes its word for them, as it does for a
tell's sender.

| `op` | needs | the hub | answers |
|---|---|---|---|
| `code` | | gives `who` a one-time link code, which replaces any it had | `ok` with `code`, `expires_secs` and `url`, the link that carries the code (`https://dereth.network/link/#K7QD-2MXP`): opening it links the character; `rate_limited`; `unavailable` |
| `unlink` | | unlinks `who` from its person, and spends its code | `ok`; `not_linked` |
| `alts` | `shown` | nothing: a person's characters are never hidden. Only a world of the first version sends it | `refused`, with `url`, the person's characters page; `not_linked` |
| `befriend` | `target` | lists `target`'s person as a person friend of `who`'s person, added from `who` | `ok` for any character's address, whether anyone linked it or not; `not_linked`; `refused` when its person has as many person friends as the hub allows (the reference hub: 1,000) |
| `unfriend` | `target` | removes the person friend whose handle is `target`, when it holds for `who` | `ok`; `not_linked` |
| `characters` | `target` | | `ok`, with `characters`: `target`'s person's characters on worlds on the network, `target` first; none, alike, for a character nobody linked and one that does not exist. Only a world of the first version sends it |
| `carry` | `targets` | lists each target's person as `befriend` does, in order, until the person lists as many as the hub allows: the friends `who`'s world kept for it before it was linked | `ok`, however many it listed; `not_linked` |

`person.watch {who, on: true}` asks for `who`'s **person view**, which the hub sends as
`person.friends` at once, and again whenever it changes, until `person.watch {who, on: false}` or
until the world's link goes:

```json
{"type":"person.friends","to":"Ann@frost","linked":true,"alts":"shown",
 "persons":[{"handle":"Bob@coldeve","characters":["Bob@coldeve","Bob@dreamweave"]}]}
```

- For a character nobody linked, `linked` is `false` and `persons` empty, but for
  the friends handed back to it when it was unlinked: they are its `persons`, each a `handle`
  naming itself alone, in the first view the hub sends for it after the unlink, and in no view
  after that. A world takes them as the character's own friends, kept on the world, if the
  character asked for its link (below); otherwise it lists none of them.
- `persons` are the person friends that hold for `to`: all of its person's, whichever character
  added them, in the order they were added. Each names its `handle` first among its `characters`,
  then that person's other characters on worlds on the network, each only while it is on the
  network itself (a linked character off the network pauses its link: its person's friends' views
  do not name it, but as the handle a friend added). A person may list more person friends than a
  view holds: past 100, the view holds those online now first, then those seen online most lately
  (a friend just added counting as seen then), then those added most lately, each as present as the
  most present character it lists, as far as the hub knows while it runs; it sends the view again
  when that changes which it holds. The view is then cut in order to the limits of section 2: each
  person friend keeps its handle; then the other characters, at most 20 for one, take what is left
  of 100 in all, in order. A view still longer than one frame loses the last person friend's last
  other character, and once none is left, the last person friend, until it fits.
- **Linking carries friends over.** When a view first says a character is linked (at once, or when
  it comes online), its world sends `carry` with the friends it keeps for the character, and from
  then on keeps none that the view lists as a handle: they are its person's, so none is listed
  twice. Those the person's limit leaves out stay the world's.
- **Worlds of the first version.** They read `alts` in every view and may send `alts` and
  `characters`. The hub writes `alts` `shown`, answers `alts` as the table says without changing
  anything, and answers `characters` with the person's characters, so that such a world shows
  what the person friends list.
- **A link holds only for the character that asked for its code.** A world keeps, for each of its
  characters, whether it ever asked for one; a view that says a character that never did is linked
  speaks of an earlier character of that name, since deleted. The world takes it as unlinked
  (lists none of its persons) and sends `unlink` for it.

## 7. The directory

A character is listed while it is on the network ([01](01-addresses-and-settings.md) section 2).
Its world sends the hub its profile when it changes, and the hub publishes it at once
([05](05-hub.md) section 6). The three messages are a family of their own: a world or a hub that
does not know them skips them, as it skips any type it does not know, and carries on.

```json
{"type":"listing","id":"l1790000000-4","who":"Sir Bob@coldeve","on":true,
 "profile":{"level":126,"heritage":1,"gender":1,"title":"the Bloodletter",
            "allegiance":{"key":"k3Yq0pW9xLmN","name":"Order of the Dawn","members":41,"rank":3,
                          "patron":"Cara@coldeve"}}}
{"type":"listing.result","id":"l1790000000-4","outcome":"ok",
 "url":"https://dereth.network/directory/character/?a=Sir%20Bob%40coldeve"}
```

### The profile

A `listing` that is `on` carries `profile`; one that is not carries none (either way otherwise is
malformed). Every field but the first three may be left out. A field out of its bounds makes the
whole listing malformed: nothing is cut or cleaned.

| field | holds | bounds |
|---|---|---|
| `level` | the character's level | 1 to 100,000 |
| `heritage` | its heritage group, as the game numbers them (1, Aluvian, to 13) | 1 to 13 |
| `gender` | 1, male, or 2, female | 1 or 2 |
| `title` | the title it shows | shown text of 1 to 64 characters (below) |
| `experience` | `{total, unassigned}`: earned in all, and not yet spent | each at most 2^53 - 1 |
| `credits` | skill credits not yet spent | at most 1,000,000 |
| `luminance` | `{available, maximum}`: to spend, and the most it may hold | each at most 2^53 - 1 |
| `attributes` | `strength`, `endurance`, `coordination`, `quickness`, `focus` and `self`, each `{innate, base}`: as made, and as raised since | all six; each value at most 1,000,000 |
| `vitals` | `health`, `stamina` and `mana`, each `{base}`: its maximum | all three; at most 1,000,000 |
| `skills` | `[{id, training, base}]`: the skill's number as the game numbers its skills, `untrained`, `trained` or `specialized`, and its level | at most 64, each number 1 to 255 and once; levels at most 1,000,000 |
| `augmentations` | `[{id, times}]`: the number of the integer property the game keeps the augmentation in, and how many times taken | at most 64, each number 1 to 65,535 and once; `times` 1 to 1,000 |
| `allegiance` | `{key, name?, members, rank, patron?, monarch?}` | below |
| `birth` | when the character was made, in seconds since 1970 | at most 2^53 - 1 |
| `age` | how long it has been played, in seconds | at most 2^53 - 1 |
| `deaths` | how many times it has died | at most 1,000,000 |
| `chess` | its chess rank | at most 1,000,000 |
| `fishing` | its fishing skill | at most 1,000,000 |
| `titles` | how many titles it has earned | at most 1,000,000 |

- **Attributes, vitals and skills** are without the effects of spells.
- **`birth`, `age`, `deaths`, `chess`, `fishing` and `titles`** are sent only while the
  character's own "Allow others to see your …" option for each allows it (date of birth, age,
  number of deaths, chess rank, fishing skill, number of titles), exactly as appraising the
  character shows them to another player. The hub cannot see a character's options: the world
  leaves out what they hide, and the hub publishes what it is sent.
- **The allegiance**: `key` is 8 to 64 of `A-Z`, `a-z`, `0-9`, `-` and `_`, an opaque key the
  world gives the allegiance, stable while its monarch is the same and derived from nothing a
  player could see in game (a keyed hash under a secret the world keeps, or a random value it
  stores); `name` (shown text of 1 to 40 characters) only when the monarch named the allegiance,
  never a name a world makes up for an unnamed one; `members`, 1 to 100,000, how many characters
  the allegiance holds, listed or not; `rank`, 1 to 100, the character's rank in it; `patron` and
  `monarch`, the addresses of those characters only while each is listed itself, always of the
  same world as `who`, and neither ever `who` (a monarch names neither).
- **Shown text** has no white space at either end, no hidden character (those [05](05-hub.md)
  section 1 refuses) and no `<` or `>`.
- The largest listing these bounds allow fits one frame.

### Answers and offs

| `outcome` | means |
|---|---|
| `ok` | done: the profile is published as sent (with `url`, the character's page on the site), or the character is out of the directory |
| `refused` | the hub will not list the character now; `reason` says why: `taken_down` (the hub's operator took it out, for a while), `full` (the directory holds as many of the world's characters as it may). The world sends the character again only at its next login or its next `@dn on` |
| `rate_limited` | over the hub's pace for listings (section 5): the world sends it again later |
| `unavailable` | this hub keeps no directory |

- A `reason` is given with `refused` only; `url` is 1 to 200 printable ASCII characters with no
  space, `<`, `>`, `"` or `'`, as a person result's.
- **A listing sets what the directory holds**, so a repeated one changes nothing, and the hub
  applies it again rather than remembering its id: a world resending after a reconnect gets the
  same answer, or the one that is true now.
- **`listing.off`**: a character was taken out of the directory, by the hub's operator (`by`
  `operator`), or by its person forgetting itself on the site (`by` `person`). Either way its world
  says it has heard with a `listing` that is not `on`, and tells the player
  ([03](03-world-behaviour.md) section 10). Taken out by the operator, the character stays on the
  network, and its world holds it out until its next login or its next `@dn on`. Taken out by its
  person, its world takes it off the network, as `@dn off` does; the hub holds the character out
  of the directory until its world has heard, and no longer (but for a hold of its operator's that
  runs on). The hub sends a world each off it owes once on each link, at the link's `welcome` and
  within seconds of one made later, until the world's `listing` says the character is off; an off
  by a person waits however long the world is away, while it is in the registry. A `by` other than
  `operator` and `person` is malformed. A world of the network's first version takes an off by a
  person as taking the character out of the directory alone.

## 8. Reach

A hub that can carry a tell to its recipient's person elsewhere while the recipient is not online
(into the person's Discord, [05](05-hub.md) section 5.8) asks the recipient's world first. A world
answers a tell to a character that is not online `offline`, saying nothing more; but whether the
character would have been shown the tell (whether it is on the network, its squelches, the
operator's blocks) is the world's to keep and to apply, so the hub asks it. The two
messages are a family of their own: a world or a hub that does not know them skips them, as it
skips any type it does not know, and carries on; a hub carries nothing elsewhere for a world that
never answers.

```json
{"type":"reach","id":"q1790000000-9","from":"Bob@coldeve","to":"Ann@frost"}
{"type":"reach.result","id":"q1790000000-9","reachable":true}
```

- **The hub asks** about a tell the world answered `offline`, and only one to a linked character
  on the network (a link pauses while its character is off, [05](05-hub.md) section 5.4) whose
  person the hub can reach elsewhere, at once after the answer, under an id of its own.
  `from` and `to` are the tell's. The teller's world is answered as ever, before anything is
  asked: the question changes nothing a teller sees.
- **The world answers** at once: `reachable` true only when `to` is its own character, not online,
  on the network, linked by itself (it asked for its link, section 6), and would have been shown
  the tell by its own rules ([03](03-world-behaviour.md) section 1): squelching neither the sender
  nor the sender's world, and blocking neither (the operator's blocks); false for anything else,
  alike (a character online now, one that does not exist), so the answer says nothing a tell would
  not. `from` may be one of the world's own characters, whose tell by name to an away, linked
  character the world sent the hub ([03](03-world-behaviour.md) section 1); the world then answers
  by the squelches the character had of its own world's characters as it last left, and false when
  it kept none. It shows the character nothing and gives no stand-in id. A `true` counts against the world's pace
  of tells from the sender's world to the character (section 5), as a shown tell does; one past it
  is answered false.
- An answer counts against the world's messages per world, and one over them goes nowhere. The hub
  forgets a question not answered within 30 seconds, or whose world's link goes, and carries that
  tell nowhere else.
- **Only the hub asks**: a `reach` from a world is `error {code: "malformed"}`, as is any message
  only the hub sends.
- **Field rules**: `id` as any id (section 2); `from` and `to` name characters, never a channel;
  `reachable` is true or false. Anything else is malformed. A field the reader does not know is
  ignored (as the reach vectors' later frames are read).
- **A reply from elsewhere.** The person may answer the tell from where it went. The hub then hands
  the teller's world an ordinary `tell` from the character the tell was for (`Ann@frost`), held to
  the paces a tell from that character's world is (section 5), with `friend` true when a person
  friend of the character's lists the teller; the
  teller's world takes it as any tell (section 4), by its own rules, and answers the hub, never a
  world. The character's own world is told nothing of it. A reply from a character off the network
  is not carried.

## 9. Say

A person may speak in a room from elsewhere, as one of its own linked characters: in a Discord
channel that mirrors the room ([05](05-hub.md) section 5.9). The hub asks the character's world
to say the line; the world says it under its own rules, shows it to its own players, and sends it
on as any line of the room, with a mark saying where it was said. The two messages are a family of
their own: a world or a hub that does not know them skips them, as it skips any type it does not
know, and carries on; the person whose line a world never answers is told so.

```json
{"type":"say","id":"h12","room":"global","who":"Ann@coldeve","text":"evening all","via":"discord"}
{"type":"say.result","id":"h12","outcome":"said"}
{"type":"room.say","id":"r12","room":"global","from":"Ann@coldeve","text":"evening all","via":"discord"}
```

- **The hub asks** only `who`'s own world, about a character linked to the person speaking, under
  an id of its own. `room` is `global`, or one of the world's own rooms that the hub mirrors for it
  ([05](05-hub.md) section 5.7); `via` says where the person said it (`discord`). The text has
  passed the line rules of [04](04-text-rules.md) at the hub.
- **The world says it** as `who`, whether or not the character is online, when `who` is its own
  character:
  - on the network, and linked by itself: it asked for its code (section 6), so that a person's
    link to an earlier character of the name speaks for no later one;
  - neither banned nor gagged;
  - no younger than the operator's minimum age for the network;
  - within the world's pace of the character's room lines, as a line its player says;
  - in a room the world carries now, and its operator has not turned off.

  It shows the line to the room's local listeners as the room's own line from `who`, marked
  ([03](03-world-behaviour.md) section 3), sends it on as a `room.say` with the same `via`, under
  every rule a line of the room meets, and answers `said`. Otherwise it answers `refused` with a
  `reason`, and shows and sends nothing.
- **Reasons**: `no_character` (none of that name, or one deleted), `not_on_network`, `not_linked`
  (it never asked for its code), `banned`, `gagged`, `too_new`, `rate_limited` (over its pace),
  `room_off` (the room not carried, or turned off), `not_allowed` (the world's own rules keep the
  character out of the room: an Olthoi player in a room it may not use, say). A world may give another word, which the hub
  takes as a refusal it cannot explain.
- **The mark.** A `room.say` with `via` is handed on as a `room.line` with the same `via`, to every
  other world carrying the room, and to the room's mirrors ([05](05-hub.md) section 5.7), the
  channel the line came from among them. A world that knows the mark shows the line with it
  (`Ann@coldeve (Discord) says, "..."`); one that predates it shows the line without. A world
  marks only a line the hub asked it to say, and the hub hands the mark on as the world sent it.
- An answer counts against the world's messages per world, and one over them goes nowhere. The hub
  gives up on a say not answered within 10 seconds, or whose world's link goes, and tells the
  person it was not answered.
- **Only the hub asks**: a `say` from a world is `error {code: "malformed"}`, as is any message
  only the hub sends.
- **Field rules**: `id` as any id (section 2); `room` a room's name; `who` a character, never a
  channel; `text` at most 1,024 characters; `via` as a line's mark; `outcome` `said` or `refused`;
  `reason` only with `refused`, 1 to 32 of `a-z` and `_`. Anything else is malformed. A field the
  reader does not know is ignored.

## 10. A world's rooms

The hub keeps, for each world, which of the world's own rooms carries the Global room (or none),
and the world's own rooms it mirrors into Discord ([05](05-hub.md) sections 5.7 and 5.10), and
tells the world, which carries them as it is told ([03](03-world-behaviour.md) section 3). The two
messages are a family of their own: a world that does not know them skips them, as it skips any
type it does not know, and carries the rooms its own settings name, as before; a hub that does not
know them never tells a world its rooms, and the world carries its own settings' rooms.

```json
{"type":"rooms","mirrored":["general"]}
{"type":"rooms.global","id":"g1790000000-1","room":"roleplay","seed":true}
{"type":"rooms","id":"g1790000000-1","global":"roleplay","mirrored":["general"]}
```

- **The hub tells** the world its rooms (`rooms`) at once after its `welcome`, and again whenever
  either changes: its operator moves the Global room or mirrors a room on the site, the world moves
  it itself (below), or its mirrors change with it. `global` is the own room that carries the Global
  room, or `off` for none; it is left out while the hub holds none for the world. `mirrored` are the
  world's own rooms the hub mirrors into Discord for it now, never the one carrying the Global room
  (that room is not one of the world's own while it does). A `rooms` that answers a `rooms.global`
  carries its `id`.
- **The world asks** (`rooms.global`) to carry the Global room in one of its own rooms, or nowhere
  (`off`), at its operator's word in game. The hub keeps it, as a change by the world, and answers
  with the world's rooms as they are after it, whether they changed or not.
- **The seed.** A world told `rooms` without `global` (a hub that holds no room for it, as after the
  world's first link to a hub that keeps rooms) sends, once on that link, a `rooms.global` with
  `seed` true naming where it carries the Global room now: where its hub last said, or else where
  its own settings say. The hub keeps a seed only while it holds no room for the world, and answers
  it as any `rooms.global`; once it holds one, a world's settings change nothing.
- **A world keeps** the last rooms it was told, so that it carries them while the hub is away, a
  restart included.
- **Only the hub tells**: a `rooms` from a world is `error {code: "malformed"}`, as is any message
  only the hub sends; a `rooms.global` a hub sends a world is ignored.
- **Field rules**: `id` as any id (section 2), left out of a `rooms` that answers nothing; `global`
  and `room` one of `general`, `trade`, `lfg`, `roleplay` and `off`; `mirrored` a list of the first
  four, each at most once and never `global`'s room; `seed` true or false, and false when left out.
  Anything else is malformed. A field the reader does not know is ignored.
