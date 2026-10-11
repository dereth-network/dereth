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
- **The reply aliases.** When the client remembers a last teller, a typed `@r `, `/r `, `@rp `,
  `@reply ` (or `/` forms) followed by a space is rewritten in the chat box to
  `@tell <last teller's name>, ` and so goes by name; a line that reaches the client's reply
  command unexpanded goes by id. The client remembers a teller only when its id is in
  `0x50000001`–`0x6FFFFFFF`, which every stand-in id is.
- **Checks before sending**: the sender's gag (answered as for a local tell); the sender must be on
  the network (otherwise it is told *You are not on the Dereth Network. Type /dn on to join it.*
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

A tell from another world is shown only to a character that is online, on the network, and
reachable by the sender (a tell the hub hands on from the receiving world's own address is
refused): Reachable `anyone`; or `friends` with the two mutual friends; or the
sender within the recipient's reply window. The operator's blocks and the recipient's squelches
apply, and the sender must have a stand-in id (section 8). Otherwise the answer is `offline` (or
`squelched`), and the recipient sees nothing. Only a tell that would be shown counts against the
paces section 8 describes, and one held back by them is answered `rate_limited`, so a refusal never
says more about the recipient than a delivered tell would.

"Mutual" rests partly on the other world's word: the recipient's world checks that its character
lists the sender, and takes the sender's `friend` flag as its world says. A world that lies can
claim a friendship that is not returned, but only for a sender the character itself lists.

A shown tell is `Communication_HearDirectSpeech` (`0x02BD`), with:

- the sender's address as the name (after the text rules);
- the sender's stand-in id, so the client draws the name as a link and makes the sender its reply
  target;
- the recipient's own id as the target (the client shows a tell only to its target);
- the tell type.

An away recipient still gets the tell, and the answer is `afk` with the away text alone (the
default away text if it set none), cut to the 1,024 characters a link message carries.

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
  person view.
- **Watching.** While a character is online and on the network, its world sends a `watch` for each
  remote friend; the friend's home world answers with `presence`, and again whenever the answer
  changes (a login, a logout, a change of settings or friends, the choice to appear offline,
  `@friends clear` included). A
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
  online, not appearing offline, visible to the watcher by its Visible setting, and the watcher is
  not blocked; `offline` for anything else. A watch for a name the world does not have, for a
  character not on the network, and from a blocked world or character are all answered `offline`,
  alike, so a watch tells another world nothing a tell would not. The world keeps a watcher only
  where its character could show to it (it lists the watcher, or is visible to anyone), at most 100
  from one world and 1,000 in all for one character (a watcher it lists is always kept), and
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
- The operator's minimum character age applies to room lines as to tells (*Your line did not
  reach the other worlds: this character is too new to send to the Dereth Network yet.*), and
  each character's lines are paced by the hub's per-character limit.
- **From anywhere.** `/dng <text>`, or `/dn global <text>`, says a line in the Global room from
  wherever the player is talking. The world takes it exactly as a line the client said in the
  carrying room: shown to that room's local listeners as the room's own line from the speaker,
  under the world's own room rules, then sent as above under every rule here. A character off the
  network is told *You are not on the Dereth Network. Type /dn on to join it.* and nothing is said;
  on a world that carries no Global room the answer is the one line *This world does not carry the
  Global room.*

## 4. Channels

A channel is a tell address, so it needs no room slot:

```
@tell oldguard@channel, anyone around tonight?
oldguard@channel tells you, "Bob@coldeve: I am!"
```

- `/dn channel create <channel>` makes one, owned by the character;
  `/dn channel invite <channel> Name@world` invites (members may invite); the invitee is told
  *Ann@frost invites you to the channel oldguard@channel. Type /dn channel join oldguard to join
  it.*; `/dn channel join`, `leave`, `members` and `list` do what they say. The last member leaving
  ends the channel.
- An invitation is shown only as a first tell from the inviter would be: to a character the inviter
  may reach by its Reachable setting (or within its reply window), never past a squelch of the
  inviter, of the inviter's world or of the channel, or the operator's blocks, and within the pace a
  tell from the inviter's world to that character is held to (section 8). The hub hands an
  invitation on once; one repeated is not shown again.
- A gagged character, or one younger than the operator's minimum age, makes no channel request.
- A tell to the channel reaches every other member online as a tell from the channel's address and
  stand-in id, its text the speaker's address, `: ` and the line; the speaker sees the echo. A
  line to one member is held to the pace a tell from the speaker's world to that member is, by the
  hub and by the member's world. A reply goes back to the channel, by name or by id.
- Squelching the channel's address leaves the channel.

## 5. Squelch and blocks

- **A remote character.** `/squelch Bob@coldeve` (the client sends
  `Communication_ModifyCharacterSquelch`, `0x0058`, with the name and id 0) squelches a remote
  character for the one local character; a stand-in id squelches the same way. Its tells, room lines
  and invitations are not shown to that character; its sender sees what the world shows a sender
  squelched locally. A remote squelch covers every channel, whatever the message type the client
  sent, and does not appear in the client's own squelch list. A character squelches at most 100
  remote characters and channels (past that: *You squelch 100 characters on other worlds already;
  unsquelch one first.*).
- **A whole world.** `/dn squelch coldeve` squelches every character of a world for one character;
  `/dn unsquelch` undoes it. A character squelches at most 50 worlds.
- **The operator.** `/dn admin block <world>` or `/dn admin block Name@world` refuses a world or one
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

The network has one command, `/dn`; `/derethnetwork` is the same command spelled out, and `/dng
<text>` is `/dn global <text>` made short. None of the three names is a command of the retail
client, ACE or GDLE, nor a chat channel name the client resolves, and the client matches a
command's name exactly, so none is taken.

| command | who | does |
|---|---|---|
| `/dn` | any player | the character's network status |
| `/dn on`, `/dn off` | any player | puts the character on the network as `Name@thisworld`, or takes it off |
| `/dn visible <who>`, `/dn reachable <who>`, `/dn listed on\|off` | any player | the three settings (Listed is kept for a later version: nothing is published yet) |
| `/dn friends` | any player | the character's remote friends and their state |
| `/dn friends add <Name@world>`, `/dn friends remove <Name@world>` | any player | a remote friend, exactly as `@friends add` and removing the row in the friends panel do (section 2); for a linked character, a person friend (section 9) |
| `/dn worlds` | any player | the worlds on the network now: each world's name as the hub gives it, after the text rules |
| `/dn squelch <world>`, `/dn unsquelch <world>` | any player | a whole world |
| `/dng <text>`, `/dn global <text>` | any player on the network | a line in the Global room (section 3) |
| `/dn channel ...` | any player | channels (section 4) |
| `/dn link` | any player | a one-time code to link this character to the player's Discord account on the site (section 9) |
| `/dn unlink` | any player | unlinks this character |
| `/dn alts`, `/dn alts shown\|hidden` | any player | the alts setting of this character's person: shown, or hidden from here; it is shown only on the site (section 9) |
| `/dn alts <Name@world>` | any player | the characters of that character's person, when it shows them |
| `/dn admin status`, `blocks`, `block <x>`, `unblock <x>` | the operator (Sentinel and above, and the server console) | the operator's blocks (section 5) |
| `/dn verify <token>` | the operator (Admin, and the server console, as `dn verify <token>`) | proves to the hub that this world is the one its operator registered or claimed, and that it answers at its registered address: the world sends the token to the hub with its key, and answers the challenge the hub sends its game port (below), never waiting on the answer, and shows it when it comes |

A player who types an operator's word without the access is shown the player's help, as for any
word `/dn` does not know. The game's own friends commands and friends panel keep working for remote
friends too.

### Verifying in game

`/dn verify <token>` is the world's half of the hub's verification in game ([05](05-hub.md)
section 2.5):

1. The world sends the hub `POST /v1/servers/<slug>/verify` with its key and the token, over
   HTTPS, from whatever address its calls leave by, and shows the operator *Asking the Dereth
   Network's hub to verify this world...* One verification goes at a time: a second
   `/dn verify` before the hub answers is *A verification is on its way to the hub already.*
   The hub may hold the call for up to half a minute while it looks the world up and waits; the
   world waits for it off its game loop.
2. Meanwhile the hub sends the world's game port its challenge, a datagram carrying the world's
   slug and a nonce. While a verification it sent waits for the hub, and for a minute at most
   from when `/dn verify` was typed, the world reads a challenge for its own slug from its game
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

The retail client forwards a command it does not know to the world as `Communication_Talk`
(`0x0015`), rewriting `/` to `@`, so `/dn on` arrives as `@dn on`; a player may type either.

## 7. What a world keeps

Its stand-in ids (never reused, a crash included), each character's settings, remote friends and
remote squelches, and the operator's blocks — and nothing else: no message, no presence history,
nothing of persons. Codes, links, the alts setting and person friends are the hub's; a linked
character's friends are its person friends, and the characters they list are held only while the
character is online, and a world that stops, a crash included, drops any it saved when it starts
again.
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
is not is told *You are not on the Dereth Network. Type /dn on to join it.* Each request but an
`unfriend` counts against the character's pace of channel requests, and each is queued and given
up on as one is (*The Dereth Network is unavailable.*).

### Linking

```
/dn link
Your link code is K7QD-2MXP. Within 10 minutes, sign in with Discord at https://dereth.network/characters/ and enter it there. It links this character to the account you sign in with; give it to nobody.
```

- `/dn link` asks for a code (`person {op: "code"}`), and shows the code, the minutes it lasts and
  the page the hub names, in the line above. A new code replaces the last; a code works once.
- `rate_limited` is *You have asked for too many link codes. Wait a while and try again.*;
  `unavailable` is *The Dereth Network's hub does not link characters.*
- Entering the code on the site links the character to the person signed in, and moves it there
  from any other person: the world vouches for its characters, and the code is the player at the
  keyboard. While the character is online its world hears so in the person view, and shows
  *This character is now linked to a Discord account.*
- `/dn unlink` (`person {op: "unlink"}`) is *This character is no longer linked to a Discord
  account.*, or *This character is not linked to a Discord account.*
- **A link holds only for the character that asked for its code.** The world keeps, in the
  character's record, that it asked (`/dn link`). A view saying that a character which never asked
  is linked speaks of an earlier character of its name, since deleted: the world takes the
  character as unlinked, lists none of the view's persons, and sends `person {op: "unlink"}` for
  it, quietly. So a name used again is never its earlier holder's person.

### Alts

- `/dn alts hidden` hides the person's characters from any of its characters (`person {op:
  "alts", shown: false}`): *Your characters are hidden now: nobody learns that they are one
  person.* A character nobody linked is told *Link this character first: type /dn link.*
- `/dn alts shown` asks the same with `shown` true, which the hub refuses: a world speaks for its
  characters, and only the person shows them, signed in on the site. The world shows the page the
  answer names: *Only you can show your characters as one person: sign in with Discord at
  https://dereth.network/characters/ and choose it there.*
- `/dn alts` shows the setting the person view last gave: *This character is not linked to a
  Discord account. Type /dn link to link it.*, *Your characters are hidden: nobody learns that they
  are one person.*, or, when shown, the person's characters (`person {op: "characters"}` for the
  character itself): *Your characters, shown as one person: Ann@frost, Ann@coldeve.* With alts
  hidden the world never learns the person's other characters, not even to show the player: the
  site lists them.
- `/dn alts Name@world` asks the same for another character: *Bob@coldeve is one person with
  Bob@dreamweave.*, or, for every other answer, *Bob@coldeve shows no other characters.*

### Friends of a linked character

```
/dn friends add Bob@coldeve
Bob@coldeve has been added to your friends list.
Bob@coldeve is online as Bob@dreamweave.
```

- **Adding.** `/dn friends add Name@world` and `@friends add Name@world` from a linked character
  list the person behind `Name@world` (`person {op: "befriend"}`): *Name@world has been added to
  your friends list.*, and the row comes with the next person view. The answer is `ok` for any
  character's address, so it says nothing about whether anyone linked that character. Past the
  hub's limit, the add is answered as a full list (section 2). A character the view lists already
  is *That character is already in your friends list*. From a character nobody links, the same
  commands add the one character, kept on the world (section 2).
- **What the world lists.** While a character is online and on the network, its world sends
  `person.watch {who: <its address>, on: true}`, and the hub answers with the person view, and
  again whenever it changes ([02](02-link-protocol.md) section 6). The world lists every character
  the view's person friends name as a remote friend of the character (section 2: rows, watches,
  presence, and mutual friends for its Visible and Reachable settings), except the character
  itself, characters of this world (whose own friends list holds those), any past the 100-friend
  limit, and any new to this world past its world's share of new stand-in ids (section 8), which a
  later view lists. A character the view no longer names goes (a row of type 2, and its watch
  stops); all of them go at logout and at `/dn off`.
- **Linking carries friends over.** When the view first says the character is linked (when it is
  linked while online, or when it comes online linked), the world sends `person {op: "carry"}` with
  the friends it keeps for the character. A friend it keeps that the view lists as a handle is the
  person's from then on: listed once, and no longer kept by the world. Those past the hub's limit
  stay the world's.
- **Unlinking hands them back.** The first view after the character is unlinked, by `/dn unlink`,
  on the site or by the person being forgotten, names the friends that held for it. The world keeps
  them for the character from then on, as friends it added, so its friends panel keeps them. A
  character that never asked for its link takes none of them (section 9, Linking).
- **The line.** When a character listed through a person friend, other than the handle, comes
  online, the line is *Bob@coldeve is online as Bob@dreamweave.*, the online character's name the
  link (`<Tell:IIDString:<id>:Bob@dreamweave>Bob@dreamweave<\Tell>`). Every other change is
  section 2's. While Bob's person hides its alts, Bob@coldeve is the only character listed, and
  nothing is shown of Bob's other characters: the friend is exactly a friend of that one
  character.
- **Removing.** Removing a row listed through a person friend (from the friends panel, or `/dn
  friends remove`) removes that person friend (`unfriend` by its handle), and with it every row it
  listed. `@friends clear` removes the character's person friends with its other friends.
- **Presence is unchanged.** Each listed character's own world answers each watch by its own
  settings ([01](01-addresses-and-settings.md) section 2). A character whose Visible is `friends`
  shows to a watcher only when each lists the other, which a person friend on each side gives two
  persons whose alts are shown, wherever their characters are.
