# Dereth.Network for ACE

An ACE mod that puts an ACE world on the Dereth Network. Its players, on the unmodified retail
client, can then:

- send a tell to a character on another world, `@tell Bob@coldeve, hi`, and reply with `@r` or by
  clicking the name;
- list characters on other worlds as friends and see them come online and go offline;
- talk in the shared Global room, carried in one of the world's own TurbineChat rooms;
- talk in invite-only channels, which are tell addresses: `@tell oldguard@channel, anyone around?`;
- squelch a remote character, or a whole world.

The world speaks the network's protocol (the specification is in `docs/network/`) to a hub over one
outbound WebSocket; the client sees only messages it already reads. Every character starts off the
network and joins with `/dn on`. A world without a `Settings.json` beside the mod behaves exactly
as ACE always does.

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
2. Copy `Settings.json.example` to `Settings.json` in that folder and set `Slug` (and `GlobalRoom`,
   see below).
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
| `GlobalRoom` | `Roleplay` | The TurbineChat room that carries the Global room: `General`, `Trade`, `LFG`, `Roleplay`, or empty for none. |
| `MinCharacterAgeSeconds` | `0` | Seconds a character must have played before it may send to the network: tells, Global room lines and channel requests. |
| `StorePath` | `Dereth.Network.store.json` | The network's records, relative to the mod's folder. |

Any setting that cannot be served (a bad slug, a `ws://` hub without the opt-in, no key in the
variable, an unknown room) keeps the network off, with the reason in the log as an error and in
`/dn admin status`. So does a setting under a name the mod no longer reads: `EventsRoom` is now
`GlobalRoom`, and the error says so.

## The Global room

The retail client has ten fixed TurbineChat rooms and drops a line for any other, so the network's
Global room borrows one the world does not otherwise need, usually **Roleplay**. Lines there still
read `[Roleplay] Bob@coldeve says, "..."` in the client; say so wherever you describe the room. Do
not use LFG: a small world's LFG would fill with other worlds' requests, and the client's listen
option would mute both.

A line an opted-in character says in the room goes to the local listeners exactly as before, then
to the other worlds. A line from another world reaches every local player who listens to that room
(its character option), unless they squelch the speaker or its world. The room follows ACE's own
switches: nothing goes out or comes in while `use_turbine_chat` is off or the room's
`chat_disable_<room>` property is on, and ACE's own checks (gag, account and character age, level)
decide whether a local line is said at all.

`/dng <text>` (or `/dn global <text>`) says a line in the Global room from wherever the player is
talking: the world takes it as the client's own line in the carrying room, through ACE's own room
handling, so it is shown here as that room's line from the speaker and goes to the other worlds
under every rule above. It needs the character on the network; on a world that carries no Global
room it says so in one line.

## Commands

`/dn` is the network's command; `/derethnetwork` is the same command spelled out, and `/dng <text>`
is `/dn global <text>` made short.

| Command | Who | Does |
|---|---|---|
| `/dn` | any player | the character's network status |
| `/dn on`, `/dn off` | any player | puts the character on the network as `Name@slug`, or takes it off |
| `/dn visible nobody\|friends\|channels\|anyone` | any player | who sees it online (first `/dn on`: mutual friends) |
| `/dn reachable nobody\|friends\|anyone` | any player | who may send it a first tell (first `/dn on`: anyone; anyone it told in the last 30 minutes may always answer) |
| `/dn listed on\|off` | any player | kept for a later version: the setting is stored, and nothing is published yet |
| `/dn friends`, `/dn worlds` | any player | its remote friends; the worlds on the network now |
| `/dng <text>`, `/dn global <text>` | any player on the network | a line in the Global room, as said in the room that carries it (above) |
| `/dn friends add <Name@world>`, `/dn friends remove <Name@world>` | any player | a remote friend, as `@friends add` and the friends panel do; for a linked character, a person friend kept by the hub: every character that character's person shows, if it shows them, listed as the character's friends while it is online. Linking carries the character's friends to its person, and unlinking hands them back |
| `/dn squelch <world>`, `/dn unsquelch <world>` | any player | every character of a world |
| `/dn link`, `/dn unlink` | any player | a one-time code to link the character to the player's Discord account on the hub's site (it shows the code and the page), or unlinks it |
| `/dn alts`, `/dn alts shown\|hidden`, `/dn alts <Name@world>` | any player | whether anyone may learn that the person's characters are one person (hidden until the player shows them on the hub's site; hidden again from here), and the characters of a person that shows them |
| `/dn channel create\|join\|leave\|members <channel>`, `/dn channel invite <channel> <Name@world>`, `/dn channel list` | any player | channels (not while gagged, or younger than `MinCharacterAgeSeconds`); an invitation is shown only to a character the inviter may reach, as a first tell would be |
| `/dn admin status`, `blocks`, `block <world or Name@world>`, `unblock <...>` | Sentinel and above, and the server console | the operator's blocks, for everyone on this world, both ways |
| `/dn verify <token>` | Admin, and the server console (as `dn verify <token>`) | sends the hub the site's one-time token with the world's key, answers the hub's challenge on the game port while the hub waits (about 15 seconds at most), and shows the hub's answer when it comes; nothing waits on it. With the `verify` feature off it says the world cannot be verified in game |

Remote friends also work through the ordinary friends commands (`@friends add Bob@coldeve`, the
friends panel); remote squelches through the ordinary `/squelch Bob@coldeve`.

## What each feature patches

Each feature is patched only if every ACE method it needs is found. The table is
`Dereth.Network/Core/PatchTargets.cs`; the patches are in `Dereth.Network/Ace/Patches.cs`.

| Feature | ACE methods | What it does |
|---|---|---|
| `core` | `PlayerManager.Tick` (after) | On the world thread every loop: what the hub sent is handled, the clock runs (the one-minute hold, the 100-second answer timeout, the heartbeat), and what to send is handed to the link. Without it the network stays off. |
| `tells` | `GameActionTell.Handle` (before) | A tell whose name holds `@` goes to the hub; this world's own address is a local tell. The sender sees the echo, the away message, or `Character not available`, once the hub answers. A sender squelched by the recipient sees what ACE shows for a local squelch: the echo, then *... has you squelched.* |
| `replies` | `GameActionTalkDirect.Handle` (before) | A reply by id to a stand-in id goes back to the remote sender. |
| `friends` | `GameActionAddFriend.Handle`, `GameActionRemoveFriend.Handle` (before); `GameActionRemoveAllFriends.Handle`, `GameEventFriendsListUpdate.WriteEventBody`, `PlayerManager.SwitchPlayerFromOfflineToOnline`, `SwitchPlayerFromOnlineToOffline` (after) | Remote friends kept in the network's own records, rows under their stand-in ids, the full list at login carrying them after the local ones, watches and presence at login and logout, the online and offline lines. |
| `appear-offline` | `GameActionSetSingleCharacterOption.Handle` (before and after) | Appearing offline shows a character offline to its remote watchers too. |
| `squelch` | `SquelchManager.HandleActionModifyCharacterSquelch` (before) | Squelching an address or a stand-in id; squelching a channel leaves it. |
| `global-room` | `TurbineChatHandler.TurbineChatReceived` (before and after), `TurbineChatHandler.LogTurbineChat` (before) | A line ACE let through in the carrying room goes on to the hub. Only when `GlobalRoom` is set; if it cannot be patched, the room is not carried at all. |
| `names` | `CharacterHandler.CharacterCreateEx`, `AdminCommands.HandleRename` (before) | No new or renamed character's name may hold `@`, the address separator. Without it the network stays off: a local name holding `@` could pass for a character of another world. |
| `verify` | `ClientPacket.Unpack` (before) | The hub's challenge on the game port, while a `/dn verify` waits for the hub's answer and within a minute of it being typed: its nonce goes back to the hub over HTTPS (never over UDP), and ACE does not unpack it (its own size check would drop it anyway). Any other datagram is ACE's, untouched; a challenge at any other time is dropped. Without it `/dn verify` cannot verify the world in game, and says so. |

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

The store (`StorePath`) holds the stand-in ids, each character's network settings, remote friends
and remote squelches, and the operator's blocks: nothing else, no message, no presence history and
nothing of persons. A linked character's friends are its person friends: the characters they list
are held only while it is online, and never written to the file; links, codes, the alts setting and
person friends are the hub's.
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
link code. A world hears of persons only through its own characters: whether each is linked, its
person's setting, and the characters its person friends list; while a person hides its alts, the
world never learns its other characters. The hub sees addresses and the lines it carries, in
transit; a world trusts its hub as it trusts its own
operators with local tells. Every remote line and remote name passes the network's text rules
before a client sees it (Windows-1252 only, no name-link markup, no line breaks, at most 255
characters; a name holding `^` or `&` is refused).

## What other worlds can and cannot make this world do

A world on the network is run by someone else. The mod holds each other world to the hub's
per-world room-line and per-recipient tell pace itself (channel lines and invitations count as its
tells), answers each other world's watches only within its share of this world's pace, and every
one alike: `offline` unless the character is on the network, online and visible to the watcher, so
a watch for a name this world does not have, a character off the network and a blocked watcher
get the same answer. It keeps remote watchers only where the character could show to them (at most
100 from one world, 1,000 in all) and each remote friend's status per local character, shown at
most four changes at once and then one every five seconds (a world flipping its answer is shown its
latest state), gives a stand-in id only for a line it shows or a character a person view lists
(at most 200 new ones from one world, then 200 an hour), never shows a new correspondent once every
id is given, refuses a tell claiming one of its own characters, shows an invitation only where the
inviter may reach, writes the away line around the away text itself, and shows a world's name and a
channel's from the hub only after their rules. It sends its own tells first, owes any one other world at most 100 answers (sent a
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
  messages, the challenge datagram;
- the pieces one by one, the text rules against Windows' own conversion, the world half on a link
  in memory with a clock it turns, and the link client against a socket that floods it;
- the link client and the world half against a real hub on loopback: a temporary registry, two
  worlds with federation on and one without, tells and replies, friends, the Global room, channels,
  401 and 403, the hold while the hub is away, and that nothing logged holds a key or a message's
  text; and `/dn verify` end to end, on a hub with its operator API on (signing in through a
  stand-in for Discord): a world registered at a game port the harness listens on is verified once
  the hub's challenge, heard there, is answered;
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
