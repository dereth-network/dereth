# What a world shows a retail player

Provenance: **network specification**; the client's behaviour it relies on is **observed at
runtime** in the final retail client (June 2015).

Empyrean's network module (`empyrean/crates/world/src/federation/`, with its scenarios in
`empyrean/testkit/tests/all/federation/`) and the ACE mod (`network/ace-mod/`) are two
implementations.

A world that has not turned the network on behaves exactly as before: a tell to a name holding `@`
finds nobody, and is answered as any unknown name is.

## 1. Tells

```
@tell Bob@coldeve, long time! still playing?
You tell Bob@coldeve, "long time! still playing?"
Bob@coldeve tells you, "yes! come say hi Sat"
@r see you then
You tell Bob@coldeve, "see you then"
```

### Sending

- **By name.** `@tell Name@world, text` reaches the world as `Communication_TalkDirectByName`
  (`0x005D`). A target name holding `@` is a network address. The world sends the tell to the
  hub; its own address for a local character (`Ann@thisworld`) is a local tell to `Ann`.
- **By id.** A reply that the client sends by id, `Communication_TalkDirect` (`0x0032`), carries
  the stand-in id the remote sender was shown under; the world turns it back into the address.
- **A marked name.** A name given with the mark a line said from Discord shows beside its speaker
  (section 3), as the client sends a clicked speaker's name or a player copies it, names the
  character without it: the world takes a trailing `(Discord)`, in any case, off the name before it
  resolves it, since no character's name holds a parenthesis
  ([01](01-addresses-and-settings.md) section 1). `Ann (Discord)` is the local `Ann`, and
  `Ann@coldeve (Discord)` the address `Ann@coldeve`, whose tell goes as any tell to an address (on
  to its person's Discord too, section 1, *A person's Discord*). It does so wherever a player gives
  a name: a tell by name, `@friends add` (section 2), a squelch and an unsquelch (section 5), and
  every `@dn` command that takes a name or an address (section 6). With the network off, a name is
  taken as given, as before. Where the mark is shown, it stays.
- **The reply aliases.** When the client remembers a last teller, a typed `@r `, `/r `, `@rp `,
  `@reply ` (or `/` forms) followed by a space is rewritten in the chat box to
  `@tell <last teller's name>, ` and so goes by name; a line that reaches the client's reply
  command unexpanded goes by id. The client remembers a teller only when its id is in
  `0x50000001`–`0x6FFFFFFF`, which every stand-in id is.
- **Checks before sending**: the sender's gag (answered as for a local tell); the sender must be on
  the network (otherwise it is told *You are not on the Dereth Network. Type @dn on to join it.*
  and nothing is sent); the operator's blocks; any minimum character age the operator sets; and the
  hub's per-character pace, which the world keeps itself (*You are sending to the Dereth Network
  too quickly. Wait a moment and try again.*).
- **What the sender sees** comes after the hub's answer, so it is what a local tell would give:

  | outcome | the sender sees |
  |---|---|
  | `delivered` | `You tell Bob@coldeve, "text"` as the outgoing-tell echo (`Communication_TextboxString`, `0xF7E0`) |
  | `afk` | the echo, then *Bob@coldeve is away: <the away text>* (`Communication_WeenieErrorWithString`, `0x028B`, AFK), the line written by the sender's world around the text the recipient's world sent |
  | `squelched` | whatever the world shows a sender squelched by a local character (Empyrean: the echo alone) |
  | `offline` | `Character not available` (`Communication_WeenieError`, `0x028A`, `0x052B`) |
  | `unavailable` | `Character not available`, then *The Dereth Network can't reach coldeve right now.* |
  | `not_on_network` | `Character not available`, then *coldeve isn't on the network.* |
  | `rate_limited` | *The Dereth Network is holding back tells from this world to Bob@coldeve for now. Wait a moment and try again.* (the character's own pace was kept before the tell went, so a refusal is its world's share, or the loop guard) |
  | `not_member` (a channel) | *You are not a member of oldguard@channel.* |
  | dropped from the queue after 60 seconds without a link | `Character not available`, then *The Dereth Network is unavailable.* |
  | no answer within 100 seconds of reaching the hub | `Character not available`, then *The Dereth Network can't reach coldeve right now.* |

### Receiving

A tell from another world is shown to a character that is online and on the network, from anyone
(a tell the hub hands on from the receiving world's own address is refused). The operator's blocks
and the recipient's squelches apply, and the sender must have a stand-in id (section 8). Otherwise
the answer is `offline` (or `squelched`), and the recipient sees nothing. Only a tell that would be
shown counts against the paces section 8 describes, and one held back by them is answered
`rate_limited`, so a refusal never says more about the recipient than a delivered tell would.

A world still sends a tell's `friend` flag, which worlds of the first version read; it decides
nothing here.

A shown tell is `Communication_HearDirectSpeech` (`0x02BD`), with:

- the sender's address as the name (after the text rules);
- the sender's stand-in id, so the client draws the name as a link and makes the sender its reply
  target;
- the recipient's own id as the target (the client shows a tell only to its target);
- the tell type.

An away recipient still gets the tell, and the answer is `afk` with the away text alone (the
default away text if it set none), cut to the 1,024 characters a link message carries.

### A person's Discord

A tell to a linked character reaches its player's Discord while they are not playing it
([05](05-hub.md) section 5.8), and while the character is on the network: its link pauses while it
is off (section 9). There is no setting for it. The world's part is small, and holds no Discord
code:

- **It answers `reach`** ([02](02-link-protocol.md) section 8): when the hub asks about a tell the
  world answered `offline`, it answers `reachable` true only when the character is its own, not
  online, on the network, linked by itself (it asked for its link, section 9), and would have been
  shown the tell by this section's rules: squelching neither the sender nor the sender's world,
  and blocking neither; false for anything else. It shows nothing, gives no stand-in id, and counts
  a `true` against its pace of tells from the sender's world to the character, as a shown tell
  does. A world that cannot look its characters up while they are not online answers false, or not
  at all. A question about a character that never asked for its link, which the hub asks for an
  earlier character of its name, is answered false. An answer waits with those owed to the teller's
  world (section 8), so a question about a tell from a world already owed its share is not
  answered.
- **A reply is a tell.** The person's reply reaches the teller's world as a tell from the character
  the tell was for, which may come while that character is not online: the world takes it as any
  tell, by its own rules, and the teller may answer it as any tell. The character's own world is
  told nothing of it.

## 2. Friends

- **Adding.** `@friends add Bob@coldeve` (the client sends `Social_AddFriend`, `0x0018`, with the
  name only) adds a remote friend to the character's own network record; the world's ordinary
  friend list holds local characters only. The answer is a `Social_FriendsUpdate` (`0x0021`) row
  of type 1 (added) under the friend's stand-in id, and *Bob@coldeve has been added to your friends
  list.* The friend shows offline until its home world says otherwise. No row of type 1 is sent
  for a friend already listed (the client does not check for duplicates). A character lists at
  most 100 remote friends (the 6096 client's own limit for its list); past that the add is answered
  as a full list (`Communication_WeenieError`, `0x028A`, `0x0561`). A linked character's friend is
  its person's instead (section 9): the world asks the hub to list it, and its row comes with the
  person view, which holds at most 100 of a person's up to 1,000 person friends.
- **Watching.** While a character is online and on the network, its world sends a `watch` for each
  remote friend; the friend's home world answers with `presence`, and again whenever the answer
  changes (a login, a logout, `@dn on` and `@dn off`, a block, the choice to appear offline). A
  character's world keeps each remote friend's status for that character alone, as the friend's
  world showed it to that character, never one character's answer for another.
- **Showing.** A `presence` that changes what the character's client shows is a row of type 4
  (status) with the friend's id, its name as the character listed it (the spelling the client
  holds), and its state; a change from a state the client was already shown is also the line
  *Bob@coldeve has come online.* or *... has gone offline.*, with the name as a link
  (`<Tell:IIDString:<id>:<name>><name><\Tell>`) that starts a tell when clicked. The first report
  after a friend is added, or after its world or the hub comes back, is a state, not news: no line.
  A character is shown at most four changes for one friend at once, then one every five seconds:
  a friend's world that changes its answer faster is shown only its latest state, as that pace
  allows. The client's Send Tell button works for online rows only, and fills
  `@tell Bob@coldeve, ` by name.
- **The full list** at login carries the remote friends after the local ones, each with the state
  last reported (offline until one is).
- **Person friends** list their characters for as long as the character is online (section 9).
  Each is a remote friend of this section in every way while it is listed.
- **Removing.** The client removes a friend by id (`Social_RemoveFriend`, `0x0017`), which for a
  remote friend is its stand-in id. The answer is a row of type 2 with the same id **and name** the
  client holds (it removes a stored entry only when both match).
- **A world that goes away**, or a lost hub, shows its characters offline with status rows and no
  lines: they did not log off.
- **Answering watches.** A world answers every `watch` for an address of its own with `presence`,
  naming the character as the watch named it: online only when the character is on the network,
  online and not appearing offline, and the watcher is not blocked; `offline` for anything else. A
  watch for a name the world does not have, for a character not on the network, and from a blocked
  world or character are all answered `offline`, alike, so a watch tells another world nothing a
  tell would not. The world keeps a watcher of a character on the network, at most 100 from one
  world and 1,000 in all for one character (a watcher it lists is always kept), and
  forgets a world's watchers when that world goes. A blocked watcher is kept too, and told nothing
  while the block lasts. A character's status change is sent to its kept watchers that are not
  blocked and to every remote friend it lists. Another world's watches are held to its share of
  this world's pace (a quarter of `messages_per_world`); one past it is neither answered nor
  kept.

## 3. The Global room

- **The slot.** The retail client has ten fixed TurbineChat room slots and drops a line for any
  other room, so a world carries the Global room in one of its own rooms that it does not otherwise
  need — usually Roleplay. Lines there still read `[Roleplay] Bob@coldeve says, "..."`; the
  world should say so wherever it describes the room. LFG should not be used: a small world's local
  LFG would fill with other worlds' requests, and the client's listen option would mute both.
- **Which room, as the hub says.** The hub keeps which room it is for each world
  ([05](05-hub.md) section 5.10), and tells the world ([02](02-link-protocol.md) section 10): from
  then on the world carries the Global room there, or in none (`off`), joining or leaving the
  Global room on its link as that changes, and keeps the hub's word, so that it carries it there
  while the hub is away, a restart included. Its operator moves it on the hub's operator page, or
  with `@dn admin global` (section 6), which asks the hub.
- **Until the hub says.** A world no hub has told carries the Global room where its own settings
  say (Empyrean: `[federation] global_room`; the ACE mod: `GlobalRoom`; Roleplay when they name
  none, no room when they name it empty). A hub that holds no room for the world is sent that one,
  once (the seed); from then on the hub's room holds and the setting is not read, and a world whose
  setting names another room logs so once. The setting goes in a later release.
- **Out.** A line a character on the network says in that room goes to the local listeners as any
  room line does, then to the hub as `room.say`. A character off the network is heard only locally.
  A line over the speaker's own pace stays here, and the speaker is told *Your line did not reach
  the other worlds: you are sending too quickly.*; a `rate_limited` answer (the world's share of the
  room is spent) tells it *Your line did not reach the other worlds: the Dereth Network is holding
  back this world's lines for now.*
- **In.** A `room.line` from another world goes to every local player who listens to that room (by
  the room's character option), as a `Communication_TurbineChat` (`0xF7DE`) room event under the
  speaker's address and stand-in id. The client checks that id against its squelch list; the name
  link it draws always carries id 0. A world must not show a room line from one of its own
  characters (it already showed it locally). A room the operator has turned off for the world
  (Empyrean and ACE: `chat_disable_<room>`) carries no line from other worlds either.
- **Discord.** A hub may mirror the Global room into channels of Discord servers ([05](05-hub.md)
  section 5.7). A line said here then reaches Discord too, under the speaker's address; a line said
  in Discord comes back into the game only as below, said by a person as one of its own
  characters. The world should say so wherever it describes the room, since such lines leave the
  game, and stay in those channels, for everyone in their servers to read.
- The operator's minimum character age applies to room lines as to tells (*Your line did not
  reach the other worlds: this character is too new to send to the Dereth Network yet.*), and
  each character's lines are paced by the hub's per-character limit.
- **From anywhere.** `@dng <text>`, or `@dn global <text>`, says a line in the Global room from
  wherever the player is talking. The world takes it exactly as a line the client said in the
  carrying room: shown to that room's local listeners as the room's own line from the speaker,
  under the world's own room rules, then sent as above under every rule here. A character off the
  network is told *You are not on the Dereth Network. Type @dn on to join it.* and nothing is said;
  on a world that carries no Global room the answer is the one line *This world does not carry the
  Global room.*
- **Said from Discord.** A person may say a line in a world operator's mirror of the Global room,
  or of one of the world's own rooms, as one of its own linked characters on this world
  ([05](05-hub.md) section 5.9). The hub asks the world (`say`, [02](02-link-protocol.md) section
  9), and the world says it as that character, online or not, under the rules a line the
  character says itself meets: the character on the network and linked by itself (it asked for
  its code, section 9), its account not banned, the character not gagged and no younger than the
  operator's minimum age, its pace of room lines not spent, the room carried here and not turned
  off, and the character one the room's own rules let speak there (an Olthoi player in a room it
  may not use is not). Otherwise it answers `refused` with the reason (`no_character`,
  `not_on_network`, `not_linked`, `banned`, `gagged`, `too_new`, `rate_limited`, `room_off`,
  `not_allowed`), and shows and sends nothing; the hub tells the person why. A line it says goes
  to the room's local listeners as the room's own line from the character, its name marked:
  `[Roleplay] Ann (Discord) says, "evening all"`; then to the hub as a `room.say` with `via`
  `discord`, under every rule of this section, so that it reaches the other worlds and every
  mirror of the room, the channel it was said in among them. A line said in one of the world's own
  rooms reaches Discord alone, as any line said there. The character is told nothing: its player
  said it.
- **The mark.** A `room.line` with `via` `discord` is shown as any line from another world, its
  speaker's name marked: `[Roleplay] Ann@coldeve (Discord) says, "evening all"`. The mark is
  where it was said, never part of the name a squelch or a link uses. A world that predates the
  mark shows the line without it.
- **A world's own rooms in Discord.** A world may also have its own rooms (General, say) mirrored
  into Discord ([05](05-hub.md) section 5.7). Its operator mirrors them on the hub's operator page
  alone: the hub tells the world which of its own rooms it mirrors
  ([02](02-link-protocol.md) section 10), and the world sends those, and no other, under the name it
  joins each by. The room that carries the Global room is never one of them: its lines reach the
  hub as the Global room's already, and the operator mirrors the Global room. No setting of the
  world's names them any more: a world whose settings still do (Empyrean: `[federation]
  discord_mirrors`; the ACE mod: `DiscordMirrors`) starts without them, and logs once that they are
  set on the hub's operator page.
  - **Joining.** At every `welcome` the world joins each own room its hub last said it mirrors, by
    its name in lower case (`general`, `trade`, `lfg`, `roleplay`), after `global`; when the hub's
    `rooms` changes them, it joins those that start and leaves those that stop.
  - **Out.** A line a character on the network says there goes to the local listeners as any room
    line does, then to the hub as a `room.say` in that room, under every rule of this section; the
    speaker is told of Discord where a Global line's speaker is told of the other worlds (*Your
    line did not reach Discord: you are sending too quickly.*). The hub says it in Discord alone. A
    line said in a room the hub does not mirror for the world never leaves it.
  - **Not mirrored.** A `not_member` answer says the hub does not mirror the room for this world
    (it stopped before the world heard so, or the hub mirrors nothing). The speaker is told
    nothing, the world logs it once, and the room's lines stay here, unsent, for a minute, so that a
    room the hub does not mirror costs its speakers nothing; the next line after that goes to the
    hub again, and a new `welcome` ends the wait at once.
  - **Nothing comes back.** A `room.line` in one of the world's own rooms is never shown, whoever
    it is from: the hub sends none.
  - The world should say so wherever it describes the room, since its lines leave the game.

## 4. Channels

A channel is a tell address, so it needs no room slot:

```
@tell oldguard@channel, anyone around tonight?
oldguard@channel tells you, "Bob@coldeve: I am!"
```

- `@dn channel create <channel>` makes one, owned by the character;
  `@dn channel invite <channel> Name@world` invites (members may invite); the invitee is told
  *Ann@frost invites you to the channel oldguard@channel. Type @dn channel join oldguard to join
  it.*; `@dn channel join`, `leave`, `members` and `list` do what they say. The last member leaving
  ends the channel.
- An invitation is shown only as a tell from the inviter would be: to a character on the network,
  never past a squelch of the inviter, of the inviter's world or of the channel, or the operator's
  blocks, and within the pace a
  tell from the inviter's world to that character is held to (section 8). The hub hands an
  invitation on once; one repeated is not shown again.
- A gagged character, or one younger than the operator's minimum age, makes no channel request.
- A tell to the channel reaches every other member online as a tell from the channel's address and
  stand-in id, its text the speaker's address, `: ` and the line; the speaker sees the echo. A
  line to one member is held to the pace a tell from the speaker's world to that member is, by the
  hub and by the member's world. A reply goes back to the channel, by name or by id.
- Squelching the channel's address leaves the channel.
- In a channel the hub mirrors into Discord ([05](05-hub.md) section 5.7), a member's line reaches
  Discord too.

## 5. Squelch and blocks

- **A remote character.** `/squelch Bob@coldeve` (the client sends
  `Communication_ModifyCharacterSquelch`, `0x0058`, with the name and id 0) squelches a remote
  character for the one local character; a stand-in id squelches the same way. Its tells, room lines
  and invitations are not shown to that character; its sender sees what the world shows a sender
  squelched locally. A remote squelch covers every channel, whatever the message type the client
  sent, and does not appear in the client's own squelch list. A character squelches at most 100
  remote characters and channels (past that: *You squelch 100 characters on other worlds already.
  Unsquelch one first.*).
- **A whole world.** `@dn squelch coldeve` squelches every character of a world for one character;
  `@dn unsquelch` undoes it. A character squelches at most 50 worlds (past that: *You squelch 50
  worlds already. Unsquelch one first.*).
- **The operator.** `@dn admin block <world>` or `@dn admin block Name@world` refuses a world or one
  character for everyone on this world, both ways: what they send is answered `offline` (and a
  watch not at all, so they never see this world's characters online), and what this world's
  characters send them is answered `Character not available`. Nothing about this world's
  characters goes to them: a watch from them is answered `offline`, as a watch for a name this
  world does not have is, and they are told no change. A block made while characters are online
  takes hold at once: everything it covers is told once that this world's characters are offline
  and is no longer watched, its watches are kept but told nothing, and its characters show offline
  here (status rows, without the lines). Lifting a block gives back what it took away: every
  watcher and every friend it covered is shown what it may see now, and the friends it covered are
  watched again.

## 6. Commands

The network has one command, `@dn`; `@derethnetwork` is the same command spelled out, and `@dng
<text>` is `@dn global <text>` made short. None of the three names is a command of the retail
client, ACE or GDLE, nor a chat channel name the client resolves, and the client matches a
command's name exactly, so none is taken. The retail client forwards a command it does not know to
the world as `Communication_Talk` (`0x0015`), rewriting `/` to `@`, so `/dn on` arrives as
`@dn on`; a player may type either.

| command | who | does |
|---|---|---|
| `@dn` | any player | the character's network status |
| `@dn on`, `@dn off` | any player | puts the character on the network as `Name@thisworld`, visible, reachable and listed, or takes it off at once, a linked character's link paused until it is back on ([01](01-addresses-and-settings.md) section 2, section 9 here) |
| `@dn friends` | any player | the character's remote friends and their state |
| `@dn friends add <Name@world>`, `@dn friends remove <Name@world>` | any player | a remote friend, exactly as `@friends add` and removing the row in the friends panel do (section 2); for a linked character, a person friend (section 9) |
| `@dng <text>`, `@dn global <text>` | any player on the network | a line in the Global room (section 3) |
| `@dn squelch <world>`, `@dn unsquelch <world>` | any player | a whole world (section 5) |
| `@dn channel ...` | any player | channels (section 4) |
| `@dn link` | any player | a one-time link that links this character to the player's Discord account (section 9) |
| `@dn unlink` | any player | unlinks this character |
| `@dn help` | any player | the player's commands, one line each |
| `@dn admin block <x>`, `@dn admin unblock <x>` | the operator (Sentinel and above, and the server console) | the operator's blocks (section 5) |
| `@dn admin block`, `@dn admin unblock` | the operator (Sentinel and above, and the server console) | with nothing after them: the blocks and the world's link to the network |
| `@dn admin global`, `@dn admin global <general\|trade\|lfg\|roleplay\|off>` | the operator (Sentinel and above, and the server console) | where the world carries the Global room (section 3); with a room, asks the hub to carry it there, or nowhere (below) |
| `@dn verify <token>` | the operator (Admin, and the server console, as `dn verify <token>`) | proves to the hub that this world is the one its operator registered or claimed, and that it answers at its registered address: the world sends the token to the hub with its key, and answers the challenge the hub sends its game port (below), never waiting on the answer, and shows it when it comes |

**Help.** `@dn help` shows one line for each of the player's commands: the command, ` - ` and what
it does, with no padding:

```
@dn - show your Dereth Network status
@dn on - put this character on the Dereth Network
@dn off - take this character off the Dereth Network
```

and so on for each command of the table, then `@tell <Name@world>, <text> - tell a character on
another world` and `@tell <channel>@channel, <text> - say a line in a channel`. An operator's help
is the same, one line for each of the operator's commands (`dn admin ...` at the console). A word
`@dn` does not know shows the help; a command given without what it needs shows its own line. A
player who types an operator's word without the access is shown the player's help. The game's own
friends commands and friends panel keep working for remote friends too.

**What the player sees:**

| typed | the world shows |
|---|---|
| `@dn on` | *You are on the Dereth Network as Ann@frost. Anyone on the network can see when you are online, send you tells and find your profile in its directory. Type @dn help for the commands.* and *Your profile shows your level, skills, allegiance and more, and changes as you play. Type @dn off to leave the network.* |
| `@dn on`, already on | *You are on the Dereth Network already, as Ann@frost.* |
| `@dn on`, a name holding `@` | *This character's name cannot be a network address, so it cannot join the Dereth Network.* |
| `@dn off` | *You are off the Dereth Network. Nobody on another world can see you or tell you, and your profile is out of its directory.*, and for a linked character *This character's link to your Discord account is paused until you type @dn on.* |
| `@dn off`, already off | *You are not on the Dereth Network.* |
| `@dn`, on | *You are on the Dereth Network as Ann@frost. The hub is connected. You have 2 friends on other worlds. This character is linked to a Discord account.* (the hub *connecting* or *not connected*; *You have no friends on other worlds.*, *1 friend*; the last sentence only when linked), then *Your directory page: <url>* when the hub has named one |
| `@dn`, off | *You are not on the Dereth Network. Type @dn on to join it as Ann@frost.* |
| any `@dn` command, the network off for the world | *This world is not on the Dereth Network.* |
| `@dn admin block`, `@dn admin unblock` | *This world is on the Dereth Network as frost. The hub is connected. 12 worlds are online. 0 messages are waiting for the hub. The Global room is carried in Roleplay. Rooms mirrored into Discord: none.* and *Blocked: coldeve, Mal@dreamweave.* or *Nothing is blocked.* |

### Moving the Global room in game

`@dn admin global` says where the world carries the Global room, and whence it knows: *This world
carries the Global room in Roleplay, as the Dereth Network's hub keeps it. Type @dn admin global
<general|trade|lfg|roleplay|off> to move it.* (before a hub keeps a room, *..., as
federation.global_room says, until the Dereth Network's hub keeps a room.*, naming the world's own
setting). With a room, the world asks the hub
(`rooms.global`, [02](02-link-protocol.md) section 10), over its link and under its key, and needs
no site and no Discord: *Asking the Dereth Network's hub to carry this world's Global room in
Trade...*; when the hub answers, the world carries the room where the hub says, and tells whoever
typed it (the log, at the console): *The Dereth Network's hub carries this world's Global room in
Trade now.* `off` carries it in no room. A word that is no room is answered with the command's
line of the help; with the link down, *The Dereth Network's hub can't be reached now. Try again once the link
is up.*; a link that goes before the hub answers is said so, to ask again; and a hub that never told
the world its rooms keeps none (it predates them), which the world says, carrying the Global room
as its settings say.

### Verifying in game

`@dn verify <token>` is the world's half of the hub's verification in game ([05](05-hub.md)
section 2.5):

1. The world sends the hub `POST /v1/servers/<slug>/verify` with its key and the token, over
   HTTPS, from whatever address its calls leave by, and shows the operator *Asking the Dereth
   Network's hub to verify this world...* One verification goes at a time: a second
   `@dn verify` before the hub answers is *A verification is on its way to the hub already.*
   The hub may hold the call for up to half a minute while it looks the world up and waits; the
   world waits for it off its game loop.
2. Meanwhile the hub sends the world's game port its challenge, a datagram carrying the world's
   slug and a nonce. While a verification it sent waits for the hub, and for a minute at most
   from when `@dn verify` was typed, the world reads a challenge for its own slug from its game
   port and posts its nonce to `POST /v1/servers/<slug>/verify/answer`, with its key, over
   HTTPS. It posts each nonce once and at most eight nonces for one verification, and never
   answers over UDP. Outside that window, or for another slug, it ignores the datagram, as it
   ignores it with the network off: the challenge's header is one the packet reader of a world
   that does not know it drops.
3. When the hub answers the call, the world shows the operator (the log, at the console) one
   line:

| the hub's answer | the line |
|---|---|
| verified, `approval` `approved` | *The Dereth Network verified this world. It is on the network now; the link comes up within a minute.* |
| verified, `rate_limited` | *The Dereth Network verified this world. Today's allowance of new worlds is spent: turn federation on from the site later.* |
| verified, `not_network_slug` | *The Dereth Network verified this world. Its slug is too long, or reserved, to join the network.* |
| verified, `locked` | *The Dereth Network verified this world, but the hub's operator has locked it.* |
| `401` | *The Dereth Network did not verify this world: the hub does not know this world's key.* and where the key is set |
| `429` | *The Dereth Network did not verify this world: too many attempts. Wait a while and try again.* |
| any other refusal | *The Dereth Network did not verify this world:* and the hub's own message, without its control characters, at most 200 characters, ended with a period unless it already ends with one, a question mark or an exclamation point: *The Dereth Network did not verify this world: The hub's challenge to retail.dereth.network:19000 (99.113.32.94) got no answer: is the game port reachable from the internet?* |
| no answer from the hub | *The Dereth Network's hub could not be reached* and why |

A world that cannot read datagrams before its packet reader cannot be verified in game, and says
so; its operator verifies it with a page on its host, or asks the hub's operator to vouch for it.

## 7. What a world keeps

Its stand-in ids (never reused, a crash included), each character's setting, remote friends and
remote squelches, the operator's blocks, the directory's listings (section 10): each listed
character's latest profile, which it lists an away character with, and the listings it owes the
hub, with the keys its allegiances are listed under, and the rooms its hub last told it (section
3) — and nothing else: no message, no presence history, nothing of persons. Codes, links and
person friends are the hub's; a linked character's friends are its person friends, and the
characters they list are held only while the character is online, and a world that stops, a crash
included, drops any it saved when it starts again.
A stand-in id no character's record names (as a friend or a squelch) is forgotten a day after it
was last used, and the least lately used past 10,000 sooner: its id is never given again, and the
identity gets a new one if it is seen again. Saving the store copies only what changed since the
last save.

A world on the network must also refuse a new or renamed local character whose name holds `@`
(Empyrean answers creation with the banned-name response and the operator's rename with an
error); with the network off it behaves as its emulator always did. A world that cannot refuse
such names must stay off the network.

## 8. What a world limits

Another world is run by someone else, and the receiving world never trusts it:

- **Stand-in ids** are given only for a line actually shown (a tell delivered, a room line some
  local player hears, a channel line shown) or a character a person view lists (section 9); a
  remote world may cause at most 200 new ones at once, then 200 an hour, counting the characters
  of that world a person view names, and an identity past that is not shown (its tell is answered
  `rate_limited`; a person view's character is not listed until a later view names it).
  Once every id in the block is given, no new correspondent is shown at all, never under another's
  id (its tell is answered `offline`).
- **Its pace.** A world holds each other world to the hub's per-world limits itself as well: its
  room lines to `room_lines_per_world`, and its tells to any one local character to
  `tells_per_recipient` (those over it answered `rate_limited`), so a hub that does not enforce
  them cannot flood this world either.
- **Its answers.** It owes any one other world at most 100 answers at once, sends them after its
  own tells and a world at a time in turn, and leaves a tell from a world owed its share unanswered
  ([02](02-link-protocol.md) section 3); an answer the hub has stopped waiting for (after 30
  seconds) is dropped.
- **Channel lines and invitations** from another world are held to the pace of its tells to the
  recipient (`tells_per_recipient`, shared with its tells), past which they are not shown.
- **Remote friends**: at most 100 for one character (section 2); **remote squelches** at most 100,
  and **squelched worlds** at most 50 (section 5).
- **Presence** is kept only for a remote character a local one lists, and only while that local
  character is online, and shown at the pace section 2 gives.
- **Watchers** are capped as section 2 says, and another world's watches held to its share.
- **Its own link**: watches and presence answers wait behind everything else, at most 2,000 deep,
  and go a world at a time in turn, to each world at most at nine tenths of its share. A newer one
  for the same pair of characters takes the waiting one's place, and past 2,000 the local
  character with the most waiting loses its oldest (a reconnect sends them again), so no one
  character can push out everyone else's.
- **What its hub sends**: the world takes at most a few hundred frames from its link at a time
  between iterations of its loop, and drops a link whose frames pile up past 20,000 (it links
  again).

## 9. Persons

A player may link their characters to one person, their Discord account, on the site
([01](01-addresses-and-settings.md) section 5, [05](05-hub.md) section 5). The world's part is to
ask the hub on its character's behalf, vouching for the character as it does for a tell, and to
show what the hub answers. Every request below needs the character to be on the network; one that
is not is told *You are not on the Dereth Network. Type @dn on to join it.* Each request but an
`unfriend` counts against the character's pace of channel requests, and each is queued and given
up on as one is (*The Dereth Network is unavailable.*).

### Linking

```
@dn link
Open https://dereth.network/link/#K7QD-2MXP within 10 minutes to link this character to your Discord account. Keep the link to yourself.
```

- `@dn link` asks for a code (`person {op: "code"}`), and shows the link the hub gives, which
  carries the code, and the minutes it lasts, in the one line above. In the client the link is
  text the player copies into a browser. A new code replaces the last; a code works once. A hub
  whose answer is no link carrying the code is shown as *Your link code is K7QD-2MXP. Enter it at
  <url> within 10 minutes to link this character to your Discord account. Keep the code to
  yourself.*
- `rate_limited` is *You have asked for too many links. Wait a while and try again.*;
  `unavailable` is *The Dereth Network's hub does not link characters.*; any other answer is *The
  Dereth Network gave no link.*
- Opening the link signs the player in with Discord if they are not, and links the character to
  that person with nothing to type, moving it there from any other person: the world vouches for
  its characters, and the link is the player at the keyboard. A link used already, or past its
  minutes, says so on the site, and to type `@dn link` for a new one. While the character is online
  its world hears of the link in the person view, and shows *This character is now linked to a
  Discord account.*
- `@dn unlink` (`person {op: "unlink"}`) is *This character is no longer linked to a Discord
  account.*, or *This character is not linked to a Discord account.*
- **A link holds only for the character that asked for its code.** The world keeps, in the
  character's record, that it asked (`@dn link`). A view saying that a character which never asked
  is linked speaks of an earlier character of its name, since deleted: the world takes the
  character as unlinked, lists none of the view's persons, and sends `person {op: "unlink"}` for
  it, quietly. So a name used again is never its earlier holder's person.

### Earlier worlds

A world of this version never sends `person {op: "alts"}` or `person {op: "characters"}`, and reads
the `alts` word of a person view as nothing ([02](02-link-protocol.md) section 6). Nothing of a
person is hidden: a person friend lists every character of its person
([01](01-addresses-and-settings.md) section 5).

### Friends of a linked character

```
@dn friends add Bob@coldeve
Bob@coldeve has been added to your friends list.
Bob@coldeve is online as Bob@dreamweave.
```

- **Adding.** `@dn friends add Name@world` and `@friends add Name@world` from a linked character
  list the person behind `Name@world` (`person {op: "befriend"}`): *Name@world has been added to
  your friends list.*, and the row comes with the next person view. The answer is `ok` for any
  character's address, so it says nothing about whether anyone linked that character. Past the
  hub's limit on a person's friends across the network (the reference hub's: 1,000), the hub answers
  `refused` and the add is answered with the one line *You have as many friends on the Dereth
  Network as it allows. Remove one first.* A character the view lists already is *That character is
  already in your friends list*. From a character nobody links, the same
  commands add the one character, kept on the world (section 2).
- **What the world lists.** While a character is online and on the network, its world sends
  `person.watch {who: <its address>, on: true}`, and the hub answers with the person view, and
  again whenever it changes ([02](02-link-protocol.md) section 6). The world lists every character
  the view's person friends name as a remote friend of the character (section 2: rows, watches
  and presence), except the character
  itself, characters of this world (whose own friends list holds those), any past the 100-friend
  limit, and any new to this world past its world's share of new stand-in ids (section 8), which a
  later view lists. A character the view no longer names goes (a row of type 2, and its watch
  stops); all of them go at logout and at `@dn off`. A view names at most 100 person friends: the
  hub picks them from a person's up to 1,000 ([02](02-link-protocol.md) section 6), and sends the view
  again as that changes.
- **Linking carries friends over.** When the view first says the character is linked (when it is
  linked while online, or when it comes online linked), the world sends `person {op: "carry"}` with
  the friends it keeps for the character. A friend it keeps that the view lists as a handle is the
  person's from then on: listed once, and no longer kept by the world. Those past the hub's limit
  stay the world's.
- **Unlinking hands them back.** The first view after the character is unlinked, by `@dn unlink`,
  on the site or by the person being forgotten, names the friends that held for it. The world keeps
  them for the character from then on, as friends it added, so its friends panel keeps them. A
  character that never asked for its link takes none of them (section 9, Linking).
- **The line.** When a character listed through a person friend, other than the handle, comes
  online, the line is *Bob@coldeve is online as Bob@dreamweave.*, the online character's name the
  link (`<Tell:IIDString:<id>:Bob@dreamweave>Bob@dreamweave<\Tell>`). Every other change is
  section 2's.
- **Removing.** Removing a row listed through a person friend (from the friends panel, or `@dn
  friends remove`) removes that person friend (`unfriend` by its handle), and with it every row it
  listed. `@friends clear` removes the character's person friends with its other friends.
- **Presence is unchanged.** Each listed character's own world answers each watch by its own
  rules (section 2): online while the character is on the network and online, not appearing
  offline and not blocking the watcher.

### Off the network, a link pauses

`@dn off` takes a linked character off the network and keeps its link: the world asks the hub
nothing more. The hub takes the character's listing out of the directory as the sign that it is off
([05](05-hub.md) section 5.4), and while it is: its person's friends' views do not name it (but as
the handle a friend added), a tell to it does not reach the person's Discord (the hub asks the
world no `reach`, and the world would answer false anyway), a reply from Discord as it is not
carried, and the person does not speak as it from Discord. The player is told so, in a second line
after `@dn off`'s: *This character's link to your Discord account is paused until you type @dn on.*
`@dn on` lists it again, and everything comes back with nothing to redo. Only `@dn unlink` breaks
the link.

### Forgetting the person

When a person forgets itself on the site, the hub sends each of its characters' worlds `listing.off`
with `by` `person` ([02](02-link-protocol.md) section 7), at once or at the world's next link. The
world takes the character off the network, as `@dn off` does (its watchers see it offline, the
rows its person friends listed leave its friends panel, its profile is out of the directory), says
it has heard with a listing that is not `on`, and tells the player *You asked the Dereth Network to
forget you, so this character is off the network now. Type @dn on to join it again.*, at once if
the character is in the world, else at its next login. The hub then unlinks it; its first view after
that hands its friends back, when it is next on the network.

## 10. The directory

A character on the network has its profile in the network's directory, on the site
([01](01-addresses-and-settings.md) section 2 says what it publishes). The world composes the
profile, sends it when it changes, and takes it out when the player goes off the network, or holds
it out while the hub's operator says so ([02](02-link-protocol.md) section 7, [05](05-hub.md)
section 6).

### What the world sends

- **The profile** is the character's own, as the world keeps it: every field of
  [02](02-link-protocol.md) section 7 the world has. Attributes, vitals and skills are without
  the effects of spells.
- **The six fields the character's options govern** (date of birth, age, number of deaths, chess
  rank, fishing skill, number of titles) go only while its own "Allow others to see your …" option
  for each is on: the same test the world's appraisal of the character makes before it shows the
  field to another player. Turning one of those options on or off is a change of the profile.
- **The allegiance**: its key, which the world keeps for it while its monarch is the same; its name
  only when the monarch gave it one (a world that fills an unnamed allegiance's name with the
  monarch's sends none); how many characters it holds; the character's rank; and its patron and
  its monarch only while that character is listed. A character of the allegiance that is not listed
  is never named to the hub, nor counted anywhere but in `members`. When a character comes into the
  directory or leaves it, its world should send again, paced, the listings of its listed vassals
  (whose `patron` it is) and, for a monarch, of every listed member, online or not, so that their
  pages link to it, or stop.
- **Nothing else**: nothing of the character's game account, its person, its location, house or
  fellowship, and no time.

### When it sends

| event | sends |
|---|---|
| `@dn on` | a listing, at once |
| the character logs in, on the network | a listing |
| a level gained, a title chosen, a skill, attribute or vital raised, an augmentation taken, an allegiance joined, left or changed, one of the six options turned, or any other change to a field | a listing, at most one every 5 minutes for one character: a change within 5 minutes of the last listing waits until they are up, and only the latest profile goes. The age alone, which grows every second the character plays, is no change of its own: it goes with the next other change, and at logout |
| the character logs out | its latest profile, at once, if it changed since the last listing |
| `@dn off` | a listing that is not `on`, at once |
| the character is deleted, or renamed | a listing that is not `on` for the name it had, at once (a renamed character's new name is listed when it is next in the world) |
| `listing.off` from the hub, `by` `operator` | a listing that is not `on`, at once; the character is held out, and none is sent for it until its next login or its next `@dn on` |
| `listing.off` from the hub, `by` `person` | a listing that is not `on`, at once; the character is off the network (section 9) |
| a `refused` answer | nothing more until the character's next login or its next `@dn on` |

- **Pace.** A world paces its listings under the hub's listings per world
  ([02](02-link-protocol.md) section 5) and `welcome.limits.messages_per_world`, and one
  character's under the listings per character. One answered `rate_limited` is sent again a minute
  later, the latest profile then.
- **Owed.** A world keeps, for each listed character, whether the hub has answered its latest
  profile, and for each character taken out, the listing that says so, saved with the character's
  setting until the hub answers it, so that a lost link, a restart or a crash never leaves a
  character in the directory its player took out. When its link comes up again, after its `hello`
  and the rest of section 2's order, it sends what it owes, the characters taken out first.
- A listing is never sent for a character off the network, or one held out, but to take it out.

### What the player sees

| typed or heard | the world shows |
|---|---|
| `@dn on` | the two lines of section 6 |
| the hub answers `ok` with a page, after `@dn on` | *Your directory page: https://dereth.network/directory/character/?a=Sir%20Bob%40coldeve* |
| `@dn off` | the line of section 6, which says the profile is out of the directory |
| `@dn` | the status line of section 6, then *Your directory page: <url>* when the hub last named one |
| `refused`, `taken_down`, after `@dn on` | *The Dereth Network's operator has taken this character out of the directory for now.* |
| `refused`, `full`, after `@dn on` | *The Dereth Network's directory holds as many of this world's characters as it may.* |
| `unavailable` | *This network's hub keeps no directory, so your profile is not in one.* |
| `listing.off`, `by` `operator` | *The Dereth Network's operator has taken this character out of the directory for now.*, at once if the character is online, else at its next login |
| `listing.off`, `by` `person` | *You asked the Dereth Network to forget you, so this character is off the network now. Type @dn on to join it again.*, at once if the character is online, else at its next login, on the network or not |

A character that was on the network with Listed off under an earlier version is listed at its next
login, since being on the network is being listed.
