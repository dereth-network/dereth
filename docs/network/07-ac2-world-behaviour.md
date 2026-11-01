# What an Asheron's Call 2 world does

Provenance: **network specification**; the client behavior it serves is **observed at runtime** in
the final Asheron's Call 2 client (30 September 2013).

One implementation: Yalain, `yalain/crates/friends` (the friends service), with
`yalain/crates/federation` (the world's half of the link) and `yalain/server/src/friends.rs` (the
listener).

This page is the counterpart of [03-world-behaviour.md](03-world-behaviour.md) for a world of
Asheron's Call 2: what it shows a player on the final client. The client reaches friends,
presence, ignore, away and tells to `Name@world` through its friends service, never through the
game connection, and the world serves that service over the friends stream
([08-friends-stream.md](08-friends-stream.md)). The world reaches other worlds, of either game,
over its link ([02-link-protocol.md](02-link-protocol.md)).

A world that does not serve the friends stream gives its players no friends service, and the
client says so in its own lines. A world that serves the stream but is not on the network gives
its players the friends service within the world.

## 1. On the network

- **Every character starts off the network.** Its world keeps one setting per character, as
  [01](01-addresses-and-settings.md) section 2 says. On the network means **visible and
  reachable**: anyone on the network who lists it sees it online, and anyone may tell it.
- **A character on the network is listed in the directory** with the Asheron's Call 2 profile of
  [02](02-link-protocol.md) section 7: its level, race, sex, health and vigor, and its allegiance,
  with no title. Its world lists it at each login on the network and at `@dn on`, sends a changed
  profile at most once every five minutes while it plays and at once at logout, and takes it out
  at `@dn off`, as [03](03-world-behaviour.md) section 10 describes.
- **The command is said in chat.** The client handles every `/` command itself and answers one it
  does not know with its own error line, so a `/dn` never reaches the world. A line typed without
  the `/` is said, and reaches the world as text. So the world takes a said line whose text is
  `@dn` or begins with `@dn ` (in any case) as the command:
  - it is never repeated to anyone, whether said aloud or in one of the game's chat rooms (a room
    line is answered as the room's request, and shown to nobody);
  - the world answers in the speaker's chat window with its own system lines.
- **The commands:**

  | said | does |
  |---|---|
  | `@dn` | the character's network status |
  | `@dn on`, `@dn off` | on or off the network, at once |
  | `@dn help`, or a word `@dn` does not know | the commands, one line each |

- **What the player sees** (the world `shardfall`, the character Ann):

  | said | the world shows |
  |---|---|
  | `@dn on` | *You are on the Dereth Network as Ann@shardfall. Anyone on the network can see when you are online and send you tells. Say @dn help for the commands.* and *Say @dn off to leave the network.* |
  | `@dn on`, already on | *You are on the Dereth Network already, as Ann@shardfall.* |
  | `@dn off` | *You are off the Dereth Network. Nobody on another world can see you or tell you.* |
  | `@dn off`, already off | *You are not on the Dereth Network.* |
  | `@dn`, on | *You are on the Dereth Network as Ann@shardfall. The hub is connected. You have 2 friends on other worlds.* (the hub *connecting* or *not connected*; *You have no friends on other worlds.*, *1 friend*) |
  | `@dn`, off | *You are not on the Dereth Network. Say @dn on to join it as Ann@shardfall.* |
  | any `@dn`, the world not on the network | *This world is not on the Dereth Network.* |
  | `@dn help` | `@dn - show your Dereth Network status`, `@dn on - put this character on the Dereth Network`, `@dn off - take this character off the Dereth Network`, `@dn help - show these commands`, `/tell <Name@world>, <text> - tell a character on another world`, `/friend add <Name@world> - add a friend on another world` |

- **The friends service within the world needs no `@dn on`.** Friends, presence and tells between
  characters of one world work for every player with a friends stream. Only another world's
  characters need the character on the network.
- **At the server's console**, the operator's blocks, as [03](03-world-behaviour.md) section 5:
  `dn block <world|Name@world>`, `dn unblock <…>`, and `dn` (the link and the blocks).

## 2. A stream, an account, a character

- **The stream is served** on TCP at the world's address, on the same port number as its login.
- **The ticket admits exactly whom the game login admits**: an open world makes the account the
  first time its name comes, with that secret, and admits it from then on with that secret only,
  as its login does; a world of listed accounts admits only an account made beforehand, with its
  own. The client presents its ticket before its game login, so a new account may be made by
  either.
- The stream binds to the account at its ticket, and to the account's character while it is in
  the world ([08](08-friends-stream.md) section 3).
- **A character with a stream is present.** It is online to its friends, and receives tells
  through the service.
- **A character without a stream is not.** It is offline to every friend, and a tell through the
  service is answered as to one not logged in. A player who started the client without the
  friends options keeps the game's own tells between characters of the world, which never touch
  the service. The world sends no tell from another world through the game connection, since the
  client's tell message names its teller by an object id a remote character does not have.
- **The entry line.** A character whose account has no stream as it enters, or who enters a world
  that serves no stream, is shown the client's own line that it could not be logged into the
  authorization server and its friends list is out of reach. One with a stream is not.
- **A stream that comes back** while the character is in the world, after a drop or in place of
  one still open, is sent the friends list again at its ticket. One that replaces an open stream
  leaves the character present throughout.
- **A character handed over to a new client.** A client that dies with its character in the world
  leaves the character there for a while (its logoff), and the game login refuses a new client's
  entry until it has gone. The new client's stream is admitted at once, against the character still
  there, which is then present. When that character leaves the world, at the end of its logoff (not
  when the old client's connections go, which can be well before), a character whose stream came
  while it was already in the world stays present for **19 seconds**, counted from then, the
  logoff's own length; entering again within them, it never stops being present: nobody, on this
  world or another, is told it left and came back, and its stream is sent its friends again as it
  enters. If it does not enter again in time, its stream ends, or another character of the account
  enters, it is seen to leave then. A character that entered with its stream already open (every
  ordinary logoff) is seen to leave as it leaves.

## 3. The client's requests

Names are read and written as [08](08-friends-stream.md) section 7 says: a name with no world, or
whose world is the world's own name (in any case) or its slug, is a character of this world, and
is written back as `Name@` and the world's name exactly; any other is another world's character,
by its address.

| request | the world does | the link |
|---|---|---|
| `GetFriendsList` | every friend of the character with its `Mode` | none |
| `FriendAdd` (own world) | a character of that name must exist, and not be the character itself (else `0xc1120012`); not listed already (else `0xc1121009`); keeps the friend; `FriendStat` (the friend's mode now), then `FriendAddRes 0x0` | none |
| `FriendAdd` (another world) | the world must be on the network (else the line *This world is not on the Dereth Network.*); the character must be on it (else *You are not on the Dereth Network. Say @dn on to join it.*), each refused with `0x80004005`; keeps the friend; then, once its world answers the watch, one `FriendStat` with the friend's state and `FriendAddRes 0x0`. With the link down, the friend's world not linked, or no answer within 3 seconds, `FriendStat Mode=0` at once (an answer after that is shown only if online). Never `Mode=0` and then the real state: the client prints a line for each `FriendStat` | `watch {owner, target, on: true}` while its link is up and the target's world is linked; otherwise when it is |
| `FriendDelete` | forgets the friend, `0x0`; one not listed `0xc1121401` | `watch {on: false}` for another world's friend |
| `GetIgnoredList` | every ignored name, as `Jerk`, written as names are | none |
| `IgnoredAdd`, `IgnoredDelete` | keeps or forgets the name: a character of this world must exist; one listed already `0xc1121009`, one not listed `0xc1121401` | none: the receiving world applies its squelches ([03](03-world-behaviour.md) section 5) |
| `Flags` | `FlagsRes`, echoing | none |
| `SetLoc` | sets the character's state: away (2), available (1), not to be disturbed (4); `0x0` to each; friends present on the same world get `FriendStat` with the new mode only when it changed (the client asks for away twice for one `/afk`, and prints a line for each `FriendStat`) | none: the link's presence is online or not; another world learns of away only from a tell's answer |
| `SendToUserEx` (own world) | to the named character when it is present: `UTellEv User=<the teller>`, with the client's extra data, and `0x0`; nothing shown when it ignores the teller, still `0x0`; a character not present `0xc1120011`; no such character `0xc1120012` | none |
| `SendToUserEx` (another world) | checks the teller as [03](03-world-behaviour.md) section 1 does: on the network, the operator's blocks, the per-character pace. Then sends the tell, and answers when it ends, by section 4's table | `tell {id, from, to, text, friend}` |
| `CallCreate`, `SendToRoomEx` | not implemented (`0x80004001`) | none |

**Modes.** A friend present is `1`, away `3`, not to be disturbed `5`: the client tests away
before not-to-be-disturbed before online, and the online bit goes with each, so the friends page
and the chat line both read right. Offline is `0`. A friend on another world is only ever `0` or
`1`, since the link's presence is online or not.

**Codes the friend and squelch answers use**, picked from the client's own lines:

| code | `FriendAdd` | `FriendDelete` | `IgnoredAdd` | `IgnoredDelete` |
|---|---|---|---|---|
| `0x0` | *`<name>` added to your friends list.* | *`<name>` removed from your friends list.* | *You squelch `<name>`.* | *You unsquelch `<name>`.* |
| `0xc1120012` | *`<name>` does not exist.* | the same | the same | the same |
| `0xc1121009` | *`<name>` is already in your friends list.* | | *`<name>` is already squelched.* | |
| `0xc1121401` | | *That user is not in your friends list.* | | *`<name>` is not in your squelch list.* |
| `0x80004005` | the client's error line, beside the world's own line saying why | the same | | |
| `0xc1120011` | | | *Unable to access the squelch list…*, beside the world's line *Your squelch list is full.* | |

## 4. A tell's outcome, and the link's messages

| message | the world does |
|---|---|
| `welcome` | sends a `watch` for each friend on a linked world of each present character on the network, paced as [02](02-link-protocol.md) section 3 says |
| `world` up | sends the watches for friends on that world |
| `world` down | `FriendStat Mode=0` for each friend on that world |
| `watch` for its character | answers `presence {online}`: online only while the character is present, on the network, and neither the watcher nor its world is blocked. Watchers are kept as [03](03-world-behaviour.md) section 2 keeps them (at most 100 from one world, 1,000 for one character), and told each change |
| `presence` | `FriendStat` (`Mode` 1 or 0) to the present character that lists `of`. At most four changes for one friend at once, then one every five seconds ([03](03-world-behaviour.md) section 2) |
| `tell` to its character | shown as `UTellEv User=<the teller's address>`, after the text rules (section 6), when the character is present, on the network, ignores neither the teller nor the teller's world, the operator blocks neither, and the paces of [03](03-world-behaviour.md) section 8 allow. Answered at once: `delivered`; `afk` when the character is away or not to be disturbed, with the default text *Away from keyboard.* or *Not to be disturbed.*; `squelched` when ignored; `rate_limited` past a pace; `offline` for everything else, alike |
| `tell.result` | answers the waiting `SendToUserEx` |
| `rooms`, `room.line`, `channel.*`, `person.friends`, `person.result`, `listing.*`, `reach`, `say` | none: skipped, as [02](02-link-protocol.md) lets a world skip a family it does not carry |

**A tell's outcome, as the teller on an AC2 world sees it.** The code goes back in
`SendToUserExRes`, one answer for each request; the line is the world's own system text, as
[03](03-world-behaviour.md) section 1 writes it for an AC1 teller, so a player of either game
reads the same network:

| outcome | `HR` | the client shows | the world's line |
|---|---|---|---|
| `delivered` | `0x0` | its own echo | none |
| `afk` | `0x0` | its own echo | *Bob@coldeve is away: …* |
| `squelched` | `0x0` | its own echo | none, as a local squelch shows a teller nothing |
| `offline` | `0xc1120011` | *Bob@coldeve is not logged in right now.* | none |
| `unavailable` | `0xc1120010` | *Your message could not be sent right now…* | *The Dereth Network can't reach coldeve right now.* |
| `not_on_network` | `0xc1120010` | the same | *coldeve isn't on the network.* |
| `rate_limited` | `0xc1120018` | the same | *The Dereth Network is holding back tells from this world to Bob@coldeve for now. Wait a moment and try again.* |
| queued past 60 seconds without a link | `0xc1120010` | the same | *The Dereth Network is unavailable.* |
| no answer within 100 seconds of reaching the hub | `0xc1120010` | the same | *The Dereth Network can't reach coldeve right now.* |
| refused before it goes: the teller off the network, the world off it, the teller's own pace | `0xc1120018` | the same | *You are not on the Dereth Network. Say @dn on to join it.*, *This world is not on the Dereth Network.*, *You are sending to the Dereth Network too quickly. Wait a moment and try again.* |

A tell to a character of the same world that is away is shown and answered `0x0`, and the teller
is shown *Cara is away: Away from keyboard.* (or *Not to be disturbed.*): the client's tell answer
knows nothing of away, so the notice comes from the world.

## 5. What the world keeps and limits

- **Kept, per character:**
  - whether it is on the network;
  - its friends (names and addresses, at most 100);
  - its ignored names and addresses (at most 100);
  - the operator's blocks.
- **Not kept:** its state (away and the rest last until it leaves the world), messages, presence
  history.
- **No stand-in ids:** the friends service names everyone by name or address.
- **The game's own tell honours the ignore list:** a character never hears a tell from one it
  ignores, and the teller is shown its tell as usual.
- **Limits** are [03](03-world-behaviour.md) section 8's that apply: per-world paces held by the
  receiving world, watchers, and the answers owed one other world. Tells within the world are held
  to the network's pace for one character. Plus the stream's own bounds
  ([08](08-friends-stream.md) section 4): at most 8 streams from one address (an IPv6 one by its
  /64), 2,000 for the world, and five refused tickets from one address in a minute refuse its next
  minute's unchecked.
- **A world on the network refuses a new character whose name holds `@`.** An AC2 name never can.

## 6. Text rules for the AC2 client

Every remote line and name passes the network's text rules ([04](04-text-rules.md)) before the
world shows it:
- printable Windows-1252;
- at most 255 characters;
- no line breaks;
- AC1's link markup removed.

The client escapes a heard tell's text before it shows it, so no markup in it is read; a teller's
name is shown as markup would read it, and an address holds none of the characters it reads.

Each system line the world shows (the network's answers, the friends service's lines) ends with a
line end (LF), as the client's own strings do: the client adds each line to the chat window as it
comes, so a line without one runs into the next.
