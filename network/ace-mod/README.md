# Dereth.Network for ACE

An ACE mod that puts an ACE world on the Dereth Network. Its players, on the unmodified retail
client, can then:

- send a tell to a character on another world, `@tell Bob@coldeve, hi`, and reply with `@r` or by
  clicking the name;
- list characters on other worlds as friends and see them come online and go offline;
- talk in the shared Global room, carried in one of the world's own TurbineChat rooms;
- talk in invite-only channels, which are tell addresses: `@tell oldguard@channel, anyone around?`;
- squelch a remote character, or a whole world;
- find each other in the network's directory: a character's profile, kept up to date as it plays,
  leaving out what its "Allow others to see your ..." options hide;
- get tells to a linked character in their Discord while they are not playing it, and answer them
  there as that character;
- say a line, as one of their own linked characters, in a Discord channel that mirrors the Global
  room or one of the world's own rooms, with the hub's `/dn say`: the world says it as that
  character, marked as said from Discord.

The world speaks the network's protocol (the specification is in `docs/network/`) to a hub over one
outbound WebSocket; the client sees only messages it already reads. Every character starts off the
network and joins with `@dn on`. On the network a character is seen online by anyone, told by
anyone and listed in the directory; squelches, the operator's blocks and Appear Offline still have
their say. `@dn off` takes it off again, and out of the directory, at once. A world without a
`Settings.json` beside the mod behaves exactly as ACE always does.

## What a world needs

- An ACE server with mod support, built for .NET 10 (ACE ships `0Harmony.dll` and loads mods from
  its `Mods` folder). The mod was built and checked against ACE's own assemblies; a fork that
  renamed a method the mod patches loses that one feature, and says so in the log (see *When a
  feature is off*).
- The world registered with a hub, with its **registry key** (`drth_...`, shown once at
  registration) and a **network slug**: 2 to 12 of `a-z` and `0-9` with single inner hyphens, not
  one of the reserved names (the nine retail worlds' names, `channel`, `hub`, `net`, `network`,
  `local`, `localhost`, `world`, `all`, `any`). On the Dereth Network's own hub, you register the
  world yourself on the site (signing in with Discord), or claim its community listing there.
- **The world verified, and federation on.** The site gives you a one-time token. Install the mod
  with the key, start ACE, and as an Admin character (or at the server console) type
  `/dn verify <token>`: the mod sends the token to the hub with the world's key, and the hub sends
  a small challenge to your world's endpoint, at its game port. The mod hears it there and posts it
  back to the hub over HTTPS, from whatever address the server's outgoing traffic leaves by, so a
  server behind two connections, a load balancer or a VPN's exit verifies as any other: what counts
  is that the world answers at its registered address. The game port must be reachable from the
  internet, as players need it to be anyway. (Or put the token in a page on your world's host and
  have the site check it; and the hub's operator can vouch for a world that can do neither.) Once
  the world is verified the hub turns federation on by itself, within
  its daily allowance of new worlds; otherwise you turn it on from the site later. Until federation
  is on, the hub refuses the world's link and the mod tries again every minute. On a hub you run yourself, the hub's command line does all of this:

  ```
  dereth-network-hub --db hub.sqlite federation <slug> on
  ```

- Outbound HTTPS (port 443) to the hub: the link (`wss://`), `/dn verify` and the answer to the
  hub's challenge (`https://`, the same host). The mod opens no port of its own (the challenge comes
  to ACE's game port) and needs no database of its own.

## Build

With the .NET 10 SDK, from this folder:

```
dotnet build Dereth.Network/Dereth.Network.csproj -c Release -p:AceDirectory=C:/ACE/Server -o out/Dereth.Network
```

`AceDirectory` is the folder holding your server's `ACE.Server.dll`, `ACE.Common.dll`,
`ACE.Entity.dll`, `ACE.Database.dll`, `0Harmony.dll` and `log4net.dll`. The mod is compiled against
them and copies none of them. It needs no NuGet package.

## Install

Get the latest release (`Dereth.Network-<version>.zip`, from the
[releases](https://github.com/dereth-network/dereth/releases) tagged `network-ace-v<version>`), or
build it yourself (above).

1. Copy the `Dereth.Network` folder, from the zip or the build, into ACE's mods folder
   (`ModsDirectory` in `Config.js`; otherwise `Mods` beside `ACE.Server.dll`). The folder's name
   must stay `Dereth.Network`: it holds `Dereth.Network.dll`, `Meta.json`, `Settings.json.example`
   and `LICENSE`.
2. Copy `Settings.json.example` to `Settings.json` in that folder and set `Slug` (and the character
   screen's `CharacterScreenMessage` if you want one; see below).
3. Put the registry key in the environment of the ACE process, under the name `KeyEnvironmentVariable`
   gives (by default `DERETH_NETWORK_KEY`): your service's protected environment file, or the
   machine's secret store. Never put it in `Settings.json`, a launcher or a script you share. A
   change of key needs the ACE process restarted.
4. Start ACE. The log shows `[Dereth.Network] on as <slug>, ...` with the features that are on, and
   then `connected to the hub` and `welcomed` once the world is running.

`Meta.json` has `HotReload: false`: change settings, then restart ACE (or `mod restart Dereth.Network`
from the console).

## Settings (`Settings.json`)

| Setting | Default | What it is |
|---|---|---|
| `HubUrl` | `wss://hub.dereth.network` | The hub. `wss://host[:port][/prefix]`; no user, query or fragment. |
| `AllowLocalWs` | `false` | Allows plain `ws://` to a hub on this machine (`ws://127.0.0.1:8650`), for testing. Plain `ws://` to any other host is always refused. |
| `Slug` | (none) | This world's network slug, exactly as registered. |
| `KeyEnvironmentVariable` | `DERETH_NETWORK_KEY` | The environment variable holding the registry key. |
| `MinCharacterAgeSeconds` | `0` | Seconds a character must have played before it may send to the network: tells, Global room lines and channel requests. |
| `StorePath` | `Dereth.Network.store.json` | The network's records, relative to the mod's folder. |
| `CharacterScreenMessage` | (none) | The world's message on the character screen, sent after the character list at every login; `\n` starts a new line. See *The character screen's message*. |

Any setting that cannot be served (a bad slug, a `ws://` hub without the opt-in, no key in the
variable, an unknown room) keeps the network off, with the reason in the log as an error and in
the answer to `@dn admin block`. So does a setting under a name the mod no longer reads: `EventsRoom` is now
`GlobalRoom`, and the error says so. A `CharacterScreenMessage` the client cannot show (anything
but Windows-1252 text, a control character other than a line break or tab, more than 65,535
characters, or not text at all) keeps the network off too, and sends no message.

The rooms are no setting of the world's: which room carries the Global room, and which of the
world's own rooms are mirrored into Discord, are kept by the hub and set on its operator page (or
the first with `@dn admin global`, below). A `Settings.json` that still has `DiscordMirrors` is used
without it, and the log says once that it is not read. One that still has `GlobalRoom` (`General`,
`Trade`, `LFG`, `Roleplay`, or empty for none; `Roleplay` when it is not there) is read only until
the hub keeps a room for the world: the first link tells a hub that keeps none, and from then on the
hub's room holds; the log says once if the setting differs. `GlobalRoom` goes in a later release.

## The character screen's message

`CharacterScreenMessage` is the world's own, not the network's: it is sent whenever it is set,
even while another setting keeps the network off. The world sends it at every login, after the
character list, as `Login_CharacterScreenMessage` (`0xF65A`), which ACE never sends. The Dereth
client shows it on the character screen: in the modern interface in its Announcements window, in
the classic one in the message box. Clients of the 2005 era show it in their character screen's
message box; the final retail client ignores it. Unset or blank, nothing is sent, as ACE does.

## The Global room

The retail client has ten fixed TurbineChat rooms and drops a line for any other, so the network's
Global room borrows one the world does not otherwise need, usually **Roleplay**. Lines there still
read `[Roleplay] Bob@coldeve says, "..."` in the client; say so wherever you describe the room. Do
not use LFG: a small world's LFG would fill with other worlds' requests, and the client's listen
option would mute both.

The hub keeps which room it is. Move it on the hub's operator page, or in game with
`@dn admin global <general|trade|lfg|roleplay|off>` (Sentinel and above, and the console),
which asks the hub; `@dn admin global` alone says where it is now. The world carries the room
where the hub says at once, and keeps the hub's word in its store, so it starts right while the hub
is away.

A line an opted-in character says in the room goes to the local listeners exactly as before, then
to the other worlds. A line from another world reaches every local player who listens to that room
(its character option), unless they squelch the speaker or its world. The room follows ACE's own
switches: nothing goes out or comes in while `use_turbine_chat` is off or the room's
`chat_disable_<room>` property is on, and ACE's own checks (gag, account and character age, level)
decide whether a local line is said at all.

`@dng <text>` (or `@dn global <text>`) says a line in the Global room from wherever the player is
talking: the world takes it as the client's own line in the carrying room, through ACE's own room
handling, so it is shown here as that room's line from the speaker and goes to the other worlds
under every rule above. It needs the character on the network; on a world that carries no Global
room it says so in one line.

## Rooms in Discord

The hub mirrors the world's own rooms, General say, into a channel of a Discord server you run, and
the Global room too. Mirror each on the hub's operator page
(`PUT /v1/operator/worlds/<slug>/discord/mirrors/general`), into a channel where you added the bot
and gave it View Channel and Manage Webhooks, or through a webhook you made in the channel; the hub
tells the world at once which rooms to send. The room that carries the Global room is not one of the
world's own: mirror the Global room instead.

A line an opted-in character says in such a room goes to the local listeners exactly as before,
then to the hub, which says it in the room's Discord channel under its speaker's address
(`Ann@coldeve`); no other world sees it. The room's rules are the Global room's: a character off the
network is heard here only, and one too new, too quick, or held back by the hub is told its line did
not reach Discord. A plain line in the Discord channel stays there; only a person's `/dn say` comes
back into the game (see *Lines said from Discord*). Should the hub answer that it does not mirror
a room it said it did, the mod logs it once and keeps the room's lines here, sending none for a
minute at a time, and nobody is told anything. Its lines leave the game: say so wherever you
describe the room.

## Lines said from Discord

In a channel of your server that mirrors the Global room or one of the world's own rooms through the
hub's bot, a player who linked a character (`@dn link`) says a line with the bot's `/dn say <text>`:
as that character when it is the player's one character on the world, and otherwise as the one
the player chooses, in a menu the first time or with the bot's `/dn identity`. The hub asks this
world to say it as that character, and the mod does, whether the character is online or not, under
the rules a line the character says itself meets: on the network, linked by its own `@dn link` (not
an earlier character of its name), its
account not banned, not gagged, no younger than `MinCharacterAgeSeconds`, within its pace of room
lines, the room carried here and open (ACE's `use_turbine_chat` and `chat_disable_<room>`), and
ACE's own rules for its rooms letting the character speak there (not an Olthoi player,
`chat_echo_only` off, and the account's age, time played and level its `chat_requires_...` settings
ask for). The room's listeners here see it as the room's own line from the character, marked:
`[Roleplay] Ann (Discord) says, "..."`; it then goes to the hub as any line of the room, so the
other worlds see `Ann@coldeve (Discord)` and every mirror of the room shows it. Otherwise the mod
answers why, and nothing is said; the hub tells the player. A line from another world said this way
is shown with the same mark, under the speaker's own stand-in id. The mod needs no setting and holds
no Discord code: the player and the command are the hub's.

## Tells into Discord

A player who links a character (`@dn link`) gets a tell to that character in their Discord direct
messages while they are not playing it, with no setting to turn on, and their reply there comes
back to the teller as a tell from that character. The world holds no Discord code and needs no
setting: the teller is answered as for any character not online (*Character not available*), and
the hub then asks this world whether the character would have been shown the tell. The mod answers
from the store alone, yes only for a character of this world that is not online, on the network,
linked by its own `@dn link` (not an earlier character of its name, since deleted), squelching
neither the teller nor the teller's world, and blocking neither (the operator's blocks). A yes
counts against the pace of tells from the teller's world to the character, as a shown tell does.
The answer shows nobody anything and gives no stand-in id. A reply from Discord reaches this
world's teller as any tell from another world does.

A teller on this world reaches it too. A tell by name (`@tell Ann`, or `Ann@<slug>`) from a
character on the network to one of this world's characters that is not online but is on the
network and linked goes to the hub instead of being answered here, so that it reaches the player's
Discord; the teller is answered *Character not available*, as for any character not online. A tell
to a character online, off the network or never linked, from a teller off the network, or while
the link is down stays ACE's own tell. ACE keeps an offline character's squelches nowhere the mod
can read, so as each linked character on the network leaves the world the mod keeps, in its
record, which of this world's characters and accounts it squelches for tells and whether it
squelches everyone, and answers a teller of this world by that; a character that has not logged
out since this version is reached by no teller of its own world until it has. The reply from
Discord is shown to the teller as a tell from `Ann@<slug>`, under a stand-in id, unless the teller
squelches Ann.

## Commands

`@dn` is the network's command (typed `/dn` too: the retail client forwards a command it does not
know, rewriting `/` to `@`); `@derethnetwork` is the same command spelled out, and `@dng <text>` is
`@dn global <text>` made short. `@dn help` shows one line for each player's command, the command, a
dash and what it does; a command given wrongly is answered with its own line. `@dn admin` with any
other word shows the operator's (`dn admin ...` at the console).

| Command | Who | Does |
|---|---|---|
| `@dn` | any player | the character's network status: on the network or not, as what, the hub, its friends on other worlds, whether it is linked, and its page in the directory |
| `@dn on` | any player | puts the character on the network as `Name@slug`: seen online by anyone, told by anyone, and listed in the network's directory. Its profile there is its level, heritage, gender, title, experience, attributes, vitals, skills, augmentations and allegiance, and its date of birth, age, deaths, chess rank, fishing skill and titles only as its "Allow others to see your ..." options allow; sent at once, at each login, at most every five minutes as it changes, and at logout |
| `@dn off` | any player | takes the character off the network, and out of the directory, at once; a linked character's link is paused until `@dn on` |
| `@dn friends` | any player | its remote friends, online or offline |
| `@dn friends add <Name@world>`, `@dn friends remove <Name@world>` | any player | a remote friend, as `@friends add` and the friends panel do; for a linked character, a person friend kept by the hub: every character that character's person links, listed as the character's friends while it is online. Linking carries the character's friends to its person, and unlinking hands them back |
| `@dng <text>`, `@dn global <text>` | any player on the network | a line in the Global room, as said in the room that carries it (above) |
| `@dn squelch <world>`, `@dn unsquelch <world>` | any player | every character of a world |
| `@dn channel create\|join\|leave\|members <channel>`, `@dn channel invite <channel> <Name@world>`, `@dn channel list` | any player on the network | channels (not while gagged, or younger than `MinCharacterAgeSeconds`); an invitation is shown as a tell from the inviter would be, never past a squelch or a block |
| `@dn link`, `@dn unlink` | any player on the network | a link that links the character to the player's Discord account on the hub's site, one line with the link to open (it carries a one-time code, good for ten minutes); or unlinks it |
| `@dn help` | any player | the player's commands, one line each |
| `@dn admin block <world or Name@world>`, `@dn admin unblock <world or Name@world>` | Sentinel and above, and the server console | the operator's blocks, for everyone on this world, both ways |
| `@dn admin block`, `@dn admin unblock` | Sentinel and above, and the server console | with nothing after them: the network's state on this world (the hub, the worlds online, what waits for the hub, the rooms) and what is blocked |
| `@dn admin global [general\|trade\|lfg\|roleplay\|off]` | Sentinel and above, and the server console | where the world carries the Global room; with a room, asks the hub to carry it there (or nowhere), and says where it is once the hub answers |
| `@dn verify <token>` | Admin, and the server console (as `dn verify <token>`) | sends the hub the site's one-time token with the world's key, answers the hub's challenge on the game port while the hub waits (about 15 seconds at most), and shows the hub's answer when it comes; nothing waits on it. With the `verify` feature off it says the world cannot be verified in game |

Remote friends also work through the ordinary friends commands (`@friends add Bob@coldeve`, the
friends panel); remote squelches through the ordinary `/squelch Bob@coldeve`.

A name a command, a tell, a squelch or a friend takes may be given with the mark a line said from
Discord shows beside its speaker, as a player clicks or copies it: `Ann (Discord)` is the local
`Ann`, and `Ann@coldeve (Discord)` the address `Ann@coldeve`. No character's name holds a
parenthesis, so the mark is never part of one.

## What each feature patches

Each feature is patched only if every ACE method it needs is found. The table is
`Dereth.Network/Core/PatchTargets.cs`; the patches are in `Dereth.Network/Ace/Patches.cs`.

| Feature | ACE methods | What it does |
|---|---|---|
| `core` | `PlayerManager.Tick` (after) | On the world thread every loop: what the hub sent is handled, the clock runs (the one-minute hold, the 100-second answer timeout, the heartbeat), and what to send is handed to the link. Without it the network stays off. |
| `tells` | `GameActionTell.Handle` (before) | A tell whose name holds `@` goes to the hub; this world's own address is a local tell, and so is a name given with the mark a line said from Discord shows (`Ann (Discord)`, as the client sends a clicked name), except to a character of this world away, on the network and linked, whose tell goes to the hub (*Tells into Discord*). The sender sees the echo, the away message, or `Character not available`, once the hub answers. A sender squelched by the recipient sees what ACE shows for a local squelch: the echo, then *... has you squelched.* |
| `replies` | `GameActionTalkDirect.Handle` (before) | A reply by id to a stand-in id goes back to the remote sender. |
| `friends` | `GameActionAddFriend.Handle`, `GameActionRemoveFriend.Handle` (before); `GameActionRemoveAllFriends.Handle`, `GameEventFriendsListUpdate.WriteEventBody`, `PlayerManager.SwitchPlayerFromOfflineToOnline`, `SwitchPlayerFromOnlineToOffline` (after) | Remote friends kept in the network's own records, rows under their stand-in ids, the full list at login carrying them after the local ones, watches and presence at login and logout, the online and offline lines, a listed character's listing at login and logout, and a linked character's tell squelches of this world's characters as it leaves. |
| `directory` | `PlayerManager.HandlePlayerDelete` (after) | A listed character deleted on the character screen leaves the directory at once. Without it the directory finds the deletion itself within half a minute, as it finds a rename. |
| `appear-offline` | `GameActionSetSingleCharacterOption.Handle` (before and after) | Appearing offline shows a character offline to its remote watchers too. |
| `squelch` | `SquelchManager.HandleActionModifyCharacterSquelch` (before) | Squelching an address or a stand-in id; squelching a channel leaves it. A name given with the mark a line said from Discord shows is squelched without it, by ACE too. |
| `global-room` | `TurbineChatHandler.TurbineChatReceived` (before and after), `TurbineChatHandler.LogTurbineChat` (before) | A line ACE let through in the room that carries the Global room, or in one of the world's own rooms the hub mirrors, goes on to the hub. If it cannot be patched, neither the Global room nor a room for Discord is carried at all, whatever the hub says. |
| `names` | `CharacterHandler.CharacterCreateEx`, `AdminCommands.HandleRename` (before) | No new or renamed character's name may hold `@`, the address separator. Without it the network stays off: a local name holding `@` could pass for a character of another world. |
| `verify` | `ClientPacket.Unpack` (before) | The hub's challenge on the game port, while a `/dn verify` waits for the hub's answer and within a minute of it being typed: its nonce goes back to the hub over HTTPS (never over UDP), and ACE does not unpack it (its own size check would drop it anyway). Any other datagram is ACE's, untouched; a challenge at any other time is dropped. Without it `/dn verify` cannot verify the world in game, and says so. |
| `character-screen` | `AuthenticationHandler.SendConnectResponse` (after) | The character screen's message after the character list at login. Only when `CharacterScreenMessage` is set, and whether or not the network is on. |

Incoming tells, room lines, channel lines and presence need no patch: they are sent with ACE's own
messages (`GameEventTell`, `GameMessageTurbineChat`, a friends-list event, system chat).

### When a feature is off

At load the log names each feature that is off and why (`feature friends is off: this server has
no ACE.Server...`). That feature then behaves as ACE always did; the others work. Two features are
needed for the network at all: `core`, and `names`. A fault inside a
patch is logged (at most once a minute per place) and the action is left to ACE.

## Stand-in ids and the store

A remote character has no object on this world, so the mod gives each remote identity an id of its
own, from `0x60000001` to `0x6FFFFFFF`: the retail client makes a tell sender's name a link, and
the sender its reply target, only for ids in the player range. The ids are kept in the store, so a
reply, a stored friend and a squelch keep meaning the same correspondent across relogs and
restarts; an id is never given to anyone else. An id no character's record names (as a friend or a
squelch) is forgotten a day after it was last used, and the least lately used past 10,000 sooner;
the id is still never given again, and the identity gets a new one if it is seen again.

The store (`StorePath`) holds the stand-in ids, whether each character is on the network, its
remote friends and remote squelches (and, for a linked one, its tell squelches of this world's
characters as it last left), the operator's blocks, and the directory's listings (each listed character's
latest profile and whether the hub has answered it, the listings owed that take characters out, and
the random keys allegiances are listed under): nothing else, no message, no presence history and
nothing of persons. A linked character's friends are its person friends: the characters they list
are held only while it is online, and never written to the file; links, codes and person friends
are the hub's. A store an earlier version wrote, with more settings for each character, reads as
whether each was on the network: a character that was on is on, and so seen, told and listed by
anyone.
A save copies on the world thread only what changed since the last one (the records that changed,
and the stand-in table if it did), and turns it into JSON and writes it off it, through a
temporary file and a rename, within two seconds of a change. Back it up with the server. **Do not delete it**: ids already shown to clients would come
to mean someone else. A store that exists but cannot be read keeps the network off until it is
fixed or restored.

## Threads and the link

Nothing the mod does blocks the world thread or a login. The link is one `ClientWebSocket` on a task
of its own, started on the world's first tick. It sends the key as `Authorization: Bearer`, and
reconnects after a lost link with growing waits (1, 2, 5, 10, then every 30 seconds), and a minute
after the hub refuses the key (401) or the world (403). While the link is down, tells, room lines
and channel requests wait up to a minute and go in order when it is back (with any that were on
their way when the link went); a tell that waits longer, or is not answered within 100 seconds of
reaching the hub, tells its sender the network is unavailable. Sends are paced to the hub's
per-world limit, and each character's to the hub's per-character limits; tells, their answers, room
lines and channel requests go before watches and presence answers, which go a world at a time in
turn, to each world within its share of the pace. Each tick takes at most 256 of the hub's frames,
and a hub that sends faster than the world takes them (20,000 waiting) loses the link, which is made
again. Stopping the mod (ACE's shutdown, `mod restart`) never waits on the network; it waits at most
two seconds for the last change to the store to reach the disk.

## Privacy

The mod logs message types, worlds and counts: never the key, never a message's text and never a
link code. A world hears of persons only through its own characters: whether each is linked, and
the characters its person friends list. To the hub's question about a tell to a character not
online, the world answers yes or no and nothing else. The hub sees addresses and the lines it carries, in
transit; a world trusts its hub as it trusts its own
operators with local tells. Every remote line and remote name passes the network's text rules
before a client sees it (Windows-1252 only, no name-link markup, no line breaks, at most 255
characters; a name holding `^` or `&` is refused).

## What other worlds can and cannot make this world do

A world on the network is run by someone else. The mod holds each other world to the hub's
per-world room-line and per-recipient tell pace itself (channel lines and invitations count as its
tells), answers each other world's watches only within its share of this world's pace, and every
one alike: `offline` unless the character is on the network, online and not appearing offline, and
the watcher not blocked, so a watch for a name this world does not have, a character off the
network and a blocked watcher get the same answer. It keeps remote watchers of a character on the
network (at most 100 from one world, 1,000 in all) and each remote friend's status per local character, shown at
most four changes at once and then one every five seconds (a world flipping its answer is shown its
latest state), gives a stand-in id only for a line it shows or a character a person view lists
(at most 200 new ones from one world, then 200 an hour), never shows a new correspondent once every
id is given, refuses a tell claiming one of its own characters, shows an invitation only where a
tell from the inviter would be shown, writes the away line around the away text itself, and shows a
channel's name from the hub only after its rules. It sends its own tells first, owes any one other world at most 100 answers (sent a
world at a time in turn), counts a tell against its pace only when it would show it, lets no one
character's watchers push out another's presence answers and no world's watches hold up another's,
lets a character list at most 100 remote friends and squelch at most 100 remote characters and 50
worlds, cuts its own away text to the 1,024 characters a link message carries, and sends nothing
about its characters to a world or character the operator blocks: a block made while they are online
tells it once that they are offline, a watch from it is answered `offline` as any other is, and
lifting the block shows every watcher and friend it covered what it may see again.

## Checking a build

The harness in `tests/` compiles only the mod's ACE-free sources (`Dereth.Network/Core/`) and checks
them without an ACE server:

```
dotnet build Dereth.Network/Dereth.Network.csproj -c Release -p:AceDirectory=<ACE server folder>
dotnet build tests/Harness.csproj -c Release
dotnet <harness>/Dereth.Network.Harness.dll --vectors ../conformance/vectors --hub <dereth-network-hub executable> \
    --ace <ACE server folder> --mod <the built Dereth.Network.dll>
```

A section whose input is not given (no hub, no ACE folder, no vectors found) is reported as not run
and the run fails as incomplete, unless it is left out by name with `--skip <section>` (`vectors`,
`hub`, `targets`, `units`); `units` includes a check against Windows' own conversion, which runs on
Windows only.

- the conformance vectors (`network/conformance/vectors/`): addresses, the text rules, the link
  messages, the directory's, the reach messages, the say messages (and the names given with a line's
  mark), the rooms messages, the challenge datagram;
- the pieces one by one, the text rules against Windows' own conversion, the world half on a link
  in memory with a clock it turns (the hub's reach questions and its says among it), and the link
  client against a socket that floods it;
- the link client and the world half against a real hub on loopback: a temporary registry, two
  worlds with federation on and one without, tells and replies, friends, the Global room, channels,
  401 and 403, the hold while the hub is away, and that nothing logged holds a key or a message's
  text; a world's own room mirrored into Discord, on a hub with its sign-in and broker on (a
  stand-in for Discord's sign-in and a webhook the world's operator made): a line said in the room is
  said through the webhook under its speaker's address, and none of the world's own comes back; and
  `@dn verify` end to end, on a hub with its operator API on (signing in through a
  stand-in for Discord): a world registered at a game port the harness listens on is verified once
  the hub's challenge, heard there, is answered; and a tell into Discord, on a hub with its sign-in
  and broker on (a stand-in for Discord's sign-in and the bot's direct messages): a tell to a
  character whose person linked it on the site with the link `@dn link` gave, logged off, is said in
  the person's direct messages, a stranger's as well as anyone's, with no setting turned on; the
  reply there comes back to the teller as a tell from that character, and a teller the character
  squelches does not reach them; and a line said from Discord, on a hub with
  its sign-in and broker on (a stand-in for Discord's sign-in, the application's key, its command,
  two mirrors' webhooks and the answers' edits, each interaction signed as Discord signs it): `/dn
  say` from a person with one character on the world speaks as it, with nothing to choose; the
  person chooses the character it linked with `/dn identity`, and `/dn say` in the world's mirror of
  the Global room has the world say it as that character, marked, to its listeners, to another
  world's and to both mirrors, and the answer edited; a gagged character says nothing, and the answer
  says why; a bad signature is refused, and the admins' mirror answers that it is for reading;
- every ACE method the mod patches resolved in the given `ACE.Server.dll`, and every hook of the
  built mod checked against its target as Harmony binds it.

`tests/PatchApply` goes one step further: it applies every hook of the built mod to its ACE method
with ACE's own Harmony, without running ACE, and takes them off again:

```
dotnet build tests/PatchApply/PatchApply.csproj -c Release -p:AceDirectory=<ACE server folder>
dotnet <out>/Dereth.Network.PatchApply.dll --ace <ACE server folder> --mod <out>/Dereth.Network.dll
```

None of this runs an ACE world. A trial on a running world needs what ACE itself needs: MySQL or
MariaDB with ACE's `ace_auth`, `ace_shard` and `ace_world` databases created from ACE's scripts,
the world database release imported, a `Config.js` pointing at them and at the retail client's
`.dat` files, then the mod installed as above against a hub with the world's federation on, and two
clients (one on this world, one on another world on the same hub).

## Licence

AGPL-3.0-only ([LICENSE](LICENSE)), as ACE is. The Windows-1252 best-fit table in
`Dereth.Network/Core/Cp1252BestFit.cs` comes from a Unicode data file used under the Unicode
License v3, whose notice it carries.
