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
- **The command's names.** `@derethnetwork` is the same command spelled out, and `@dng <text>` is
  `@dn global <text>` made short.
- **The commands**, each as [03](03-world-behaviour.md) section 6 has it, the player's lines saying
  *Say @dn* where that page's say *Type @dn*:

  | said | who | does |
  |---|---|---|
  | `@dn`, `@dn status` | any player | the character's network status |
  | `@dn on`, `@dn off` | any player | on or off the network, at once |
  | `@dn friends` | any player | its friends on other worlds and their state |
  | `@dn friends add <Name@world>`, `@dn friends remove <Name@world>` | any player | what the client's own add and delete do, with no request to answer: the world says so itself, and the client's friends page shows the change when it next asks for its list. For a linked character, a person friend (section 8) |
  | `@dn squelch <world>`, `@dn unsquelch <world>` | any player | a whole world, at most 50 ([03](03-world-behaviour.md) section 5). Kept beside the character's ignored names; the client's own squelch list does not show it |
  | `@dn channel ...` | any player | channels ([03](03-world-behaviour.md) section 4). A channel is a tell address: `/tell oldguard@channel, text` is the friends service's tell, a line from one reaches the client as a tell from that address, and squelching the address leaves the channel |
  | `@dn link`, `@dn unlink` | any player | the one-time link to the player's Discord account, and undoing it ([03](03-world-behaviour.md) section 9) |
  | `@dng <text>`, `@dn global <text>` | any player on the network | a line in the Global room, as if said in the room that carries it (section 7) |
  | `@dn help`, or a word `@dn` does not know | any player | the commands, one line each |
  | `@dn admin block <x>`, `@dn admin unblock <x>`, `@dn admin block`, `@dn verify <token>` | an admin, and the server's console | the operator's blocks and verifying the world, as [03](03-world-behaviour.md) sections 5 and 6 have them. From staff below an admin the world's own line that the command needs an admin's access; from a player a word `@dn` does not know |
  | `@dn admin global`, `@dn admin global <general\|trade\|lfg\|roleplay\|off>` | an admin, and the server's console | where the world carries the Global room; with a room, asks the hub to carry it there, or nowhere (section 7) |

  An answer of several lines is one system message, its lines in order. Separate messages of a long
  answer may overtake each other on the way to the client.

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
| `@dng`, the world carrying no Global room | *This world does not carry the Global room.* |
| `@dng`, off the network | *You are not on the Dereth Network. Say @dn on to join it.* |
  | `@dn off`, a linked character | the line above, and *This character's link to your Discord account is paused until you say @dn on.* |
| `@dn`, on and linked | the line above, ending *This character is linked to a Discord account.* (then *Your directory page: <url>* once the hub has named one) |
| `@dn link` | *Open https://dereth.network/link/#K7QD-2MXP within 10 minutes to link this character to your Discord account. Keep the link to yourself.*, and the rest of [03](03-world-behaviour.md) section 9's answers |
| `@dn friends`, none | *You have no friends on other worlds. Add one with @dn friends add Name@world.* |
| `@dn friends` | *Your friends on other worlds:*, then *Bob@coldeve: online* or *offline* for each |
| `@dn friends add Bob@coldeve` | *Bob@coldeve has been added to your friends list.* |
| `@dn friends remove Bob@coldeve` | *Bob@coldeve has been removed from your friends list.* |
| `@dn squelch coldeve` | *Every character of coldeve has been squelched.* |
| an invitation to a channel | *Bob@coldeve invites you to the channel oldguard@channel. Say @dn channel join oldguard to join it.* |

- **The friends service within the world needs no `@dn on`.** Friends, presence and tells between
  characters of one world work for every player with a friends stream. Only another world's
  characters need the character on the network.
- **At the server's console**, the operator's blocks, as [03](03-world-behaviour.md) section 5:
  `dn block <world|Name@world>`, `dn unblock <…>`, and `dn` (the link and the blocks). `dn admin
  block ...` and `dn admin unblock ...` are the same, as they are said in game.

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
- **A character renamed by the operator** stays in the world, present, with its stream and state.
  Every character's friend and ignore entries for it hold the new name, kept with the rename itself.
  A friend present is told the old name is offline and the new one in the character's state. The
  client's friends window keeps the old name until it next asks for the list, at the character's
  next entry or the player's `/friend list`.
- **To other worlds a new name is another character**, since an address is a name: the old address's
  watchers and the friends it lists on other worlds see it offline, and it is taken out of the
  directory ([03](03-world-behaviour.md) section 10). The character stays on the network, watches its
  friends and is listed under the new name. Other worlds' friends lists keep the old address, and its
  link to a Discord account does not follow it: the player says `@dn link` again.

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
| `SendToUserEx` (own world) | to the named character when it is present: `UTellEv User=<the teller>`, with the client's extra data, and `0x0`; nothing shown when it ignores the teller, still `0x0`; a character not present `0xc1120011`; no such character `0xc1120012`. To a character away, on the network and linked by itself, from a teller on the network while the link is up: as another world's, below, so that it may reach the player's Discord; the teller sees the same `0xc1120011` | none, or as another world's |
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
| `presence` | `FriendStat` (`Mode` 1 or 0) to the present character that lists `of`. At most four changes for one friend at once, then one every five seconds ([03](03-world-behaviour.md) section 2). The first report of a friend as offline shows nothing: the client was told so with its friends, and prints a line for every `FriendStat` |
| `tell` to its character | shown as `UTellEv User=<the teller's address>`, after the text rules (section 6), when the character is present, on the network, ignores neither the teller nor the teller's world, the operator blocks neither, and the paces of [03](03-world-behaviour.md) section 8 allow. Answered at once: `delivered`; `afk` when the character is away or not to be disturbed, with the default text *Away from keyboard.* or *Not to be disturbed.*; `squelched` when ignored; `rate_limited` past a pace; `offline` for everything else, alike |
| `tell.result` | answers the waiting `SendToUserEx` |
| `person.result` | shown to the character that asked, if it is in the world, by [03](03-world-behaviour.md) section 9; a world's own quiet request shows nothing |
| `person.friends` | says whether the character is linked: *This character is now linked to a Discord account.* when that changes while it is in the world. A view saying a character that never asked for its link is linked is answered with a quiet `person {op: "unlink"}`. The view's person friends are listed as section 8 says. The view is watched while a character is present, so a character with no friends stream is not told, and its `@dn` does not say it is linked |
| `reach` | answered as [03](03-world-behaviour.md) section 1, *A person's Discord*, has it. For a teller of this world, by the squelch list the world keeps for the character, which it holds whether or not the character is in the world |
| `channel.result` | shown to the character that asked |
| `channel.invite` | shown as a tell from the inviter would be, by [03](03-world-behaviour.md) section 4 |
| `channel.line` | `UTellEv User=<the channel's address>`, its text the speaker's address, `: ` and the line, by [03](03-world-behaviour.md) section 4 |
| `rooms`, `room.line`, `room.result`, `say` | section 7 |

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
| `not_member`, a tell to a channel | `0xc1120018` | the same | *You are not a member of oldguard@channel.* |
| refused before it goes: the teller off the network, the world off it, the teller's own pace | `0xc1120018` | the same | *You are not on the Dereth Network. Say @dn on to join it.*, *This world is not on the Dereth Network.*, *You are sending to the Dereth Network too quickly. Wait a moment and try again.* |

A tell to a character of the same world that is away is shown and answered `0x0`, and the teller
is shown *Cara is away: Away from keyboard.* (or *Not to be disturbed.*): the client's tell answer
knows nothing of away, so the notice comes from the world.

**A tell within the world to a character not in it.** The client sends a name without `@` over the
game connection, and one with `@` as `SendToUserEx`. Either way, when the character is on the network
and linked by itself, and the teller on the network with the link up, the tell also goes to the hub
as `tell {from: Ann@thisworld, to: Cara@thisworld}`, so that it may reach Cara's player's Discord.
The teller sees what any tell to that character shows: *Cara is unavailable.* from the game, *Cara is
not logged in right now.* from the service. The world answers the hub `offline` and the hub asks
`reach`. Cara's reply reaches the world as a tell from `Cara@thisworld`, and is shown as any tell,
written as `Cara@` and the world's name; a tell from the world's own address from any other character
is answered `offline`.

## 5. What the world keeps and limits

- **Kept, per character:**
  - whether it is on the network;
  - its friends (names and addresses, at most 100);
  - its ignored names and addresses (at most 100);
  - the operator's blocks.
- **Not kept:** its state (away and the rest last until it leaves the world), messages, presence
  history.
- **No stand-in ids:** the friends service names everyone by name or address, and so does a room's
  line.
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

## 7. The Global room and the game's rooms

[03](03-world-behaviour.md) section 3 has the rules. This section says what differs on the final
client.

**The rooms.** The final client has one room of each of five kinds: General, Trade, a kingdom's, a
region's and PvP, each with a room id the world gives it as it enters. It has no LFG or Roleplay
room. A kingdom's, an allegiance's and a fellowship's room reach their own members, so they cannot
carry the network's lines, and a world gives no region's room. So a world carries the Global room in
General, Trade or PvP, every character's, and the link's four names for a world's rooms mean:

| on the link, and in the setting | the room | a line reads |
|---|---|---|
| `general` | General | `[General] Bob@coldeve: ...` |
| `trade` | Trade | `[Trade] Bob@coldeve: ...` |
| `lfg`, `roleplay` | PvP | `[PvP] Bob@coldeve: ...` |
| `off`, in the setting empty | none | |

PvP is the room nobody needs: in every recorded session of every era, no line was sent to it and
none was heard from it. Anyone may speak there: the client asks nothing of a speaker but the channel.
Two of the link's names being one room, a world tells the hub the name it was given, and a room the
hub mirrors under both goes under the first.

**The setting.** `[federation] global_room`, in the form of [03](03-world-behaviour.md) section 3's:
`general`, `trade`, `lfg`, `roleplay` or empty; anything else stops the server. **Left out it is
`general`, and on**, not the Roleplay of a world of the first game. A world's setting is read as
[03](03-world-behaviour.md) section 3 says: until its hub has told it a room, and sent to a hub that
holds none, once on each link, as its seed. The hub's word then holds, and is kept with the world's
records.

**Out.** A line a character says in the carrying room is answered and shown to the world's players
as any room's line, whoever said it. It then goes to the hub as `room.say` from a character on the
network, under [03](03-world-behaviour.md) section 3's rules, its lines as there, with *Say @dn* for
*Type @dn*. A world of this game has no setting for a minimum character age yet, so no line is
refused as too new.

**In.** A `room.line` reaches every character in the world, on the network or not, as the chat
service's own line of that room, under the speaker's address (or `Ann@coldeve (Discord)`, marked as
[03](03-world-behaviour.md) section 3 says). It carries no meta letters, and its speaker's note is a
note with none, so the client writes the address as it stands. There is no listen option on the
server's side: a character chooses in its chat windows' filters what each shows. The client squelches
a speaker by its name on its own list, and the world also holds back a line from a character or world
its list ignores, and one the operator blocks.

**From anywhere.** `@dng <text>` is the character's own line in the carrying room: back to it as the
room's line under its own name and meta letters, to the world's other players, and on to the hub. The
client asked for none of it, so nothing answers a request. A gagged character is told so, and nobody
hears it.

**Said from Discord.** The rules of [03](03-world-behaviour.md) section 3, and the reasons but two:
no line is refused `too_new` (above), and none is refused `not_allowed`, since every character of this
game may use these rooms. The line goes to the room's players as `Ann (Discord)`, except one who
ignores the character.

**The world's own rooms in Discord.** As [03](03-world-behaviour.md) section 3 says, by the same names
as the table above. The room carrying the Global room is never one of them, under either of its
names.

**The operator.** `@dn admin global` and its answers are [03](03-world-behaviour.md) section 6's, the
room as the table above names it, with *Say @dn* for *Type @dn*. `dn` at the console and `@dn admin
block` in game add *The Global room is carried in General. Rooms mirrored into Discord: none.*

## 8. The friends of a linked character

[03](03-world-behaviour.md) section 9 has the rules. This section says what differs on the final
client, whose friends page can't be told of a row in play.

**What the client does with what it is sent.**

| the world sends | the chat window | the friends page |
|---|---|---|
| `FriendStat` for a name it holds | a line for the state | the row moves |
| `FriendStat` for a name it doesn't hold | the same line | no row |
| `FStart`, in play or after a reconnect | *Your friends:* | unchanged |
| `GetFriendsListRes`, the only thing that rebuilds it | a line per friend, if the player asked | every row anew |
| `FriendDeleteRes 0x0` | *`<name>` removed from your friends list.* | that row goes |

The client asks for the list as the character enters, at `/friend status`, and after any `FriendAdd`
answered `0x0`. So:

- **Entering.** The list the client asks for as a character that asked for its link enters waits for
  its first person view, at most 3 seconds, so that the page holds its person's friends from the start.
- **A row a view adds in play** comes with the next list. The first state of its friend reaches the
  chat window at once.
- **A row a view takes away in play** stays on the page until the next list, shown offline, with the
  client's line for that if it was shown online. Removing it there answers `FriendDelete` with `0x0`,
  which takes it off the page, and the world has nothing more to do.
- **`FriendAdd`** from a linked character, of another world's character, goes to the hub as `person
  {op: "befriend"}`, and nothing is kept on the world. It is answered once the view names the friend
  and its world says its state, one `FriendStat` and then `FriendAddRes 0x0`; with no such view within
  3 seconds, as offline. The client then asks for the list, which holds the row. A refusal is
  `0x80004005` with the world's line. With the link down, the character keeps the friend, as a
  character nobody links does, and it is carried over at its next arrival.
- **`@dn friends add`** from a linked character asks the same, and is told as the hub answers: *Bob@coldeve
  has been added to your friends list.*, or the refusal's line.
- **`FriendDelete`** of a row a person friend listed removes that person friend (`unfriend` by its
  handle), which takes its other rows with the next view.
- **At most 100 rows**, what the world keeps and what the view lists together. `/friend add` itself
  refuses past 20 in the client's own count, as it does for any friends list; the page's Add and
  `@dn friends add` don't.
- **The line.** *Bob@coldeve is online as Bob@dreamweave.* follows the client's own line for that
  friend, which can't be left out.
- **What goes in the world's rows.** Only the characters of other worlds: a character of this world
  is the character's own to add, by its name, kept as ever.
