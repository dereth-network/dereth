# The link protocol

Provenance: **network specification**.

One implementation: `network/protocol/src/wire.rs` (the messages) and `limits.rs`, with the hub's
side in `network/hub/src/core.rs`. The vectors in `network/conformance/vectors/messages.json` give
a frame of every message, with the values its fields must read as.

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
| `room.say` | world → hub | `id`, `room`, `from`, `text` | a line one of its characters said in the room |
| `room.line` | hub → world | `room`, `from`, `text` | another world's line in a room this world carries |
| `room.result` | hub → world | `id`, `outcome` | how a `room.say` ended |
| `channel` | world → hub | `id`, `op`, `channel`, `by`, `who`? | a channel request (`create`, `invite`, `join`, `leave`, `list`, `members`) |
| `channel.result` | hub → world | `id`, `outcome`, `channel`, `members`?, `channels`? | how it ended |
| `channel.invite` | hub → world | `channel`, `by`, `to` | one of the world's characters is invited |
| `channel.line` | hub → world | `channel`, `from`, `to`, `text` | a channel line for one member on the world |
| `person` | world → hub | `id`, `op`, `who`, `target`?, `targets`?, `shown`? | a request about the person layer for one of the world's characters (`code`, `unlink`, `alts`, `befriend`, `unfriend`, `characters`, `carry`; section 6) |
| `person.result` | hub → world | `id`, `outcome`, `code`?, `expires_secs`?, `url`?, `characters`? | how it ended |
| `person.watch` | world → hub | `who`, `on` | `who` is online and on the network (`on`): send its person view now and whenever it changes; or stop |
| `person.friends` | hub → world | `to`, `linked`, `alts`, `persons` [{`handle`, `characters`}] | `to`'s person view: whether it is linked, its person's alts setting, and the person friends that hold for it, each with the characters it lists (for a character nobody links, the friends handed back to it) |
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
- **Rooms**: 2 to 20 of `a-z`, `0-9` and inner hyphens. The first version has one room, `global`.
- **Channels**: 2 to 20 of `a-z`, `A-Z`, `0-9` and inner hyphens, in `channel`, `channel.invite` and
  `channel.line`, and in a `channel.result`'s `channel` and every name in its `channels`, which a
  world shows its players.
- **`watch.owner`, `presence.of`, `tell.from`, `room.say.from`, `channel.by`, `person.who` and
  `person.watch.who`** must be the sending world's own characters.
- **A person request's `op`** is one of `code`, `unlink`, `alts`, `befriend`, `unfriend`,
  `characters` and `carry`. `befriend`, `unfriend` and `characters` need a `target`, which names a
  character, never a channel; `carry` needs `targets`, 1 to 100 characters, none a channel; `alts`
  needs `shown` (true or false).
- **A person result's** `code` is two groups of four of `0-9` and `A-Z` without `I`, `L`, `O` and
  `U`, joined by a hyphen (`K7QD-2MXP`); `expires_secs` is a whole number of seconds, at most 3,600;
  `url` is 1 to 200 printable ASCII characters with no space, `<`, `>`, `"` or `'`; `characters`
  names at most 20 characters, none of them a channel.
- **A person view's** `alts` is `hidden` or `shown`; `persons` holds at most 100 person friends,
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
2. The hub sends `welcome`, and tells every other world `world {online: true}`.
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

A field the reader does not know is ignored; a `welcome` without a required field is malformed.

A tell, a room line or a person request over a limit is answered `rate_limited`; anything else
over the world's limit is dropped. **A world must pace what it sends** under `welcome.limits.messages_per_world`, holding
the rest for a moment, so that a burst (every watch at a reconnect, say) is never dropped at the
hub. The loop guard catches two automated answerers replying to each other; an away
message is an answer to a tell, never a tell, so it cannot start a loop.

## 6. The person layer

The hub keeps persons ([01](01-addresses-and-settings.md) section 5; [05](05-hub.md) section 5). A
world asks on its own characters' behalf, and the hub takes its word for them, as it does for a
tell's sender.

| `op` | needs | the hub | answers |
|---|---|---|---|
| `code` | | gives `who` a one-time link code, which replaces any it had | `ok` with `code`, `expires_secs` and `url` (the page to enter it on); `rate_limited`; `unavailable` |
| `unlink` | | unlinks `who` from its person, and spends its code | `ok`; `not_linked` |
| `alts` | `shown` | with `shown` false, hides the characters of `who`'s person; a world may hide them, never show them | `ok`; `not_linked`; `refused` for `shown` true, with `url`, the page the person shows them on |
| `befriend` | `target` | lists `target`'s person as a person friend of `who`'s person, added from `who` | `ok` for any character's address, whether anyone linked it or not; `not_linked`; `refused` when 100 person friends hold for `who` (below) |
| `unfriend` | `target` | removes the person friend whose handle is `target`, when it holds for `who` | `ok`; `not_linked` |
| `characters` | `target` | | `ok`, with `characters`: when `target`'s person shows its alts, its characters on worlds on the network, `target` first; otherwise none, alike for a character nobody linked, one whose person hides its alts, and one that does not exist |
| `carry` | `targets` | lists each target's person as `befriend` does, in order, until 100 person friends hold for `who`: the friends `who`'s world kept for it before it was linked | `ok`, however many it listed; `not_linked` |

`person.watch {who, on: true}` asks for `who`'s **person view**, which the hub sends as
`person.friends` at once, and again whenever it changes, until `person.watch {who, on: false}` or
until the world's link goes:

```json
{"type":"person.friends","to":"Ann@frost","linked":true,"alts":"hidden",
 "persons":[{"handle":"Bob@coldeve","characters":["Bob@coldeve","Bob@dreamweave"]}]}
```

- For a character nobody linked, `linked` is `false`, `alts` `hidden` and `persons` empty, but for
  the friends handed back to it when it was unlinked: they are its `persons`, each a `handle`
  naming itself alone, in the first view the hub sends for it after the unlink, and in no view
  after that. A world takes them as the character's own friends, kept on the world, if the
  character asked for its link (below); otherwise it lists none of them.
- `persons` are the person friends that hold for `to`: the ones added from `to`, and, while `to`'s
  person shows its alts, all of that person's. Each names its `handle` first among its
  `characters`, then, while the handle's person shows its alts, that person's other characters on
  worlds on the network. A view over the limits of section 2 is cut in order: past 100 person
  friends the rest are left out; each keeps its handle; then the other characters, at most 20 for
  one, take what is left of 100 in all, in order. A view still longer than one frame loses the last
  person friend's last other character, and once none is left, the last person friend, until it
  fits.
- **Linking carries friends over.** When a view first says a character is linked (at once, or when
  it comes online), its world sends `carry` with the friends it keeps for the character, and from
  then on keeps none that the view lists as a handle: they are its person's, so none is listed
  twice. Those the person's limit leaves out stay the world's.
- So a view tells `to`'s world nothing of a person that hides its alts but the handle `to`'s own
  person typed.
- **A world speaks for its characters**, so it can do for a person only what its character could
  rightly do alone: hide the person's characters, never show them (showing them is the person's
  own choice, signed in on the site), and count, toward `befriend`'s limit, only the person friends
  that hold for its character, so that a refusal says nothing of the person's other characters
  while they are hidden.
- **A link holds only for the character that asked for its code.** A world keeps, for each of its
  characters, whether it ever asked for one; a view that says a character that never did is linked
  speaks of an earlier character of that name, since deleted. The world takes it as unlinked
  (lists none of its persons) and sends `unlink` for it.
