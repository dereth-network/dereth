# Dereth.Network for ACE

An ACE mod that puts an ACE world on the Dereth Network. Its players, on the unmodified retail
client, can then:

- send a tell to a character on another world, `@tell Bob@coldeve, hi`, and reply with `@r` or by
  clicking the name;
- list characters on other worlds as friends and see them come online and go offline;
- talk in the shared Global room, carried in one of the world's own TurbineChat rooms;
- talk in invite-only channels, which are tell addresses: `@tell oldguard@channel, anyone around?`;
- squelch a remote character, or a whole world;
- find each other in the network's directory, where a character's profile follows its "Allow
  others to see your ..." options;
- get tells to a linked character in Discord while not playing it, and answer them there;
- speak as a linked character in a Discord channel that mirrors one of the world's rooms.

The world talks to a hub over one outbound WebSocket, using the network's protocol (specified in
`docs/network/`). The client sees only messages it already understands. Every character starts off
the network and joins with `@dn on`; `@dn off` takes it off again, and out of the directory, at
once. On the network a character can be seen online, told and found by anyone, subject to
squelches, the operator's blocks and Appear Offline. Without a `Settings.json` beside the mod, the
world behaves exactly as stock ACE.

## What a world needs

- **An ACE server with mod support, built for .NET 10.** ACE ships `0Harmony.dll` and loads mods
  from its `Mods` folder. The mod is built and checked against ACE's own assemblies. On a fork that
  renamed a method the mod patches, that one feature is off and the log says so (see [When a
  feature is off](#when-a-feature-is-off)).
- **The world registered with a hub.** You need its registry key (`drth_...`, shown once) and its
  network slug: 2 to 12 of `a-z` and `0-9`, with single hyphens inside. A few names are reserved:
  the nine retail worlds' names, `channel`, `hub`, `net`, `network`, `local`, `localhost`, `world`,
  `all` and `any`. On the Dereth Network's own hub, register the world yourself on the site (signing
  in with Discord), or claim its community listing there.
- **The world verified, with federation on.** The site gives you a one-time token. With the mod
  installed and ACE running, type `/dn verify <token>` as an Admin or at the console. The hub sends a
  small challenge to the world's game port, and the mod posts it back over HTTPS. A server behind
  several connections, a load balancer or a VPN verifies the same way, since what counts is that the
  world answers at its registered address. The game port must be reachable from the internet, as it
  is for players. (The site can also check a token you put in a page on the world's host, and the
  hub's operator can vouch for a world that can do neither.) Once verified, the hub turns federation
  on, within its daily allowance of new worlds; otherwise turn it on from the site later. Until then
  the hub refuses the link and the mod retries every minute. On your own hub, the command line does
  all of this:

  ```
  dereth-network-hub --db hub.sqlite federation <slug> on
  ```

- **Outbound HTTPS (port 443) to the hub**, for the link (`wss://`), `/dn verify` and the answer to
  the challenge. The mod opens no port of its own and needs no database.

## Install

Get the latest release (`Dereth.Network-<version>.zip`, from the
[releases](https://github.com/dereth-network/dereth/releases) tagged `network-ace-v<version>`), or
build it (below).

1. Copy the `Dereth.Network` folder into ACE's mods folder (`ModsDirectory` in `Config.js`,
   otherwise `Mods` beside `ACE.Server.dll`). Keep the folder's name. It holds `Dereth.Network.dll`,
   `Meta.json`, `Settings.json.example` and `LICENSE`.
2. Copy `Settings.json.example` to `Settings.json` in that folder and set `Slug`, and
   `CharacterScreenMessage` if you want one.
3. Put the registry key in the ACE process's environment, under `DERETH_NETWORK_KEY` (or the name
   `KeyEnvironmentVariable` gives): your service's protected environment file, or the machine's
   secret store. Never put it in `Settings.json`, a launcher or a script you share. Changing the key
   needs ACE restarted.
4. Start ACE. The log shows `[Dereth.Network] on as <slug>, ...` with the features that are on,
   then `connected to the hub` and `welcomed` once the world is running.

The mod doesn't hot-reload: after changing settings, restart ACE or run `mod restart
Dereth.Network` at the console.

## Build

With the .NET 10 SDK, from this folder:

```
dotnet build Dereth.Network/Dereth.Network.csproj -c Release -p:AceDirectory=C:/ACE/Server -o out/Dereth.Network
```

`AceDirectory` is the folder holding your server's `ACE.Server.dll`, `ACE.Common.dll`,
`ACE.Entity.dll`, `ACE.Database.dll`, `0Harmony.dll` and `log4net.dll`. The mod compiles against
them and copies none of them. It needs no NuGet packages.

## Settings (`Settings.json`)

| Setting | Default | What it is |
|---|---|---|
| `HubUrl` | `wss://hub.dereth.network` | The hub: `wss://host[:port][/prefix]`, with no user, query or fragment. |
| `AllowLocalWs` | `false` | Allows plain `ws://` to a hub on this machine (`ws://127.0.0.1:8650`), for testing. Plain `ws://` to any other host is always refused. |
| `Slug` | (none) | This world's network slug, exactly as registered. |
| `KeyEnvironmentVariable` | `DERETH_NETWORK_KEY` | The environment variable holding the registry key. |
| `MinCharacterAgeSeconds` | `0` | How long a character must have played before it can send tells, Global room lines and channel requests to the network. |
| `StorePath` | `Dereth.Network.store.json` | The network's records, relative to the mod's folder. |
| `CharacterScreenMessage` | (none) | The world's message on the character screen; `\n` starts a new line. |

A setting that can't be served (a bad slug, a `ws://` hub without `AllowLocalWs`, no key in the
variable) keeps the network off, with the reason logged as an error and shown by `@dn admin
block`. So does a setting the mod no longer reads: `EventsRoom` is now `GlobalRoom`, and the error
says so. A `CharacterScreenMessage` the client can't show (anything but Windows-1252 text, a control
character other than a line break or tab, or more than 65,535 characters) also keeps the network
off and sends nothing.

Which room carries the Global room, and which of the world's rooms are mirrored into Discord, are
set on the hub, not here. An old `DiscordMirrors` setting is ignored, with a note in the log. An old
`GlobalRoom` setting (`General`, `Trade`, `LFG`, `Roleplay`, or empty for none) is read only until
the hub keeps a room for the world: the first link tells the hub, and from then on the hub's choice
holds. `GlobalRoom` will go in a later release.

## The character screen's message

`CharacterScreenMessage` belongs to the world, not the network, so it is sent whenever it's set,
even while the network is off. The world sends it at every login, after the character list, as
`Login_CharacterScreenMessage` (`0xF65A`), which ACE never sends. The Dereth client shows it on the
character screen (in the modern interface's Announcements window, and the classic interface's
message box). Clients of the 2005 era show it in their message box; the final retail client ignores
it.

## The Global room

The retail client has ten fixed TurbineChat rooms and drops lines for any other, so the Global room
borrows one the world doesn't otherwise need, usually **Roleplay**. Lines there still read
`[Roleplay] Bob@coldeve says, "..."`, so say so wherever you describe the room. Don't use LFG: a
small world's LFG would fill with other worlds' requests, and muting it would mute both.

The hub keeps which room it is. Change it on the hub's operator page, or in game with
`@dn admin global <general|trade|lfg|roleplay|off>` (Sentinel and above, and the console);
`@dn admin global` alone says where it is now. The world keeps the hub's choice in its store, so it
starts right while the hub is away.

A line an opted-in character says in that room goes to local listeners as always, then to the other
worlds. A line from another world reaches every local player listening to that room, unless they
squelch the speaker or its world. ACE's own switches still apply: nothing goes out or comes in while
`use_turbine_chat` is off or the room's `chat_disable_<room>` property is on, and ACE's checks (gag,
account and character age, level) decide whether a local line is said at all.

`@dng <text>` (or `@dn global <text>`) says a line in the Global room from wherever the player is
talking, as if said in the carrying room. It needs the character on the network.

## Discord

The hub can mirror the world's own rooms into a channel of a Discord server you run, and the Global
room too. Set each mirror up on the hub's operator page, in a channel where you added the hub's bot
(with View Channel and Manage Webhooks) or through a webhook you made. Mirror the Global room as
itself, not the room that carries it.

A line an opted-in character says in a mirrored room goes to local listeners as always, then to the
Discord channel under the speaker's address (`Ann@coldeve`); other worlds don't see it. Its lines
leave the game, so say so wherever you describe the room. A plain line in the Discord channel stays
there.

**Speaking from Discord.** A player who linked a character (`@dn link`) can say a line in a mirrored
channel with the bot's `/dn say <text>`, as that character (or, with several on the world, the one
chosen with the bot's `/dn identity`). The world says it whether the character is online or not,
under the rules the character would meet in game: on the network, its own link, not banned or
gagged, old enough, within its pace, the room open, and ACE's own chat rules. Listeners see
`[Roleplay] Ann (Discord) says, "..."`, and other worlds see `Ann@coldeve (Discord)`. Otherwise the
hub tells the player why.

**Tells into Discord.** A tell to a linked character that isn't online goes to its player's Discord
direct messages, with no setting to turn on, and their reply comes back as a tell from that
character. The teller sees *Character not available*, as for any offline character. The world only
answers the hub's question of whether the character would have been shown the tell: yes only for an
offline character of this world, on the network, linked by its own `@dn link`, and not squelching or
blocking the teller or its world. A tell from this world to such a character goes to the hub too.
Because ACE doesn't keep an offline character's squelches where the mod can read them, the mod
records them as each linked character logs out; a character that hasn't logged out since the mod was
installed can't be reached this way by its own world's players.

The mod holds no Discord code and no Discord settings: everything Discord is the hub's.

## Commands

`@dn` is the network's command. `/dn` works too (the retail client turns an unknown `/` command into
`@`), `@derethnetwork` is the long form, and `@dng <text>` is short for `@dn global <text>`.
`@dn help` lists the player commands, and `@dn admin` the operator's (`dn admin ...` at the
console).

| Command | Who | Does |
|---|---|---|
| `@dn` | any player | the character's network status: on or off, its address, the hub, its remote friends, whether it is linked, and its directory page |
| `@dn on` | any player | puts the character on the network as `Name@slug` and in the directory. Its profile is its level, heritage, gender, title, experience, attributes, vitals, skills, augmentations and allegiance, plus birth date, age, deaths, chess rank, fishing skill and titles where its options allow. It is sent at once, at each login, at most every five minutes as it changes, and at logout |
| `@dn off` | any player | takes the character off the network and out of the directory; a linked character's link is paused until `@dn on` |
| `@dn friends` | any player | its remote friends, online or offline |
| `@dn friends add <Name@world>`, `@dn friends remove <Name@world>` | any player | adds or removes a remote friend. A linked character's friends are kept by the hub for its player, shared by all their linked characters |
| `@dng <text>`, `@dn global <text>` | on the network | a line in the Global room |
| `@dn squelch <world>`, `@dn unsquelch <world>` | any player | squelches every character of a world |
| `@dn channel create\|join\|leave\|members <channel>`, `@dn channel invite <channel> <Name@world>`, `@dn channel list` | on the network | channels (not while gagged or younger than `MinCharacterAgeSeconds`) |
| `@dn link`, `@dn unlink` | on the network | links the character to the player's Discord account: a one-time link to open on the hub's site, good for ten minutes; or unlinks it |
| `@dn help` | any player | the player commands, one line each |
| `@dn admin block <world or Name@world>`, `@dn admin unblock ...` | Sentinel and above, and the console | the operator's blocks, for everyone on this world, both ways |
| `@dn admin block`, `@dn admin unblock` | Sentinel and above, and the console | alone: the network's state on this world and what is blocked |
| `@dn admin global [general\|trade\|lfg\|roleplay\|off]` | Sentinel and above, and the console | where the world carries the Global room; with a room, asks the hub to move it |
| `@dn verify <token>` | Admin, and the console | verifies the world with the hub (about 15 seconds at most) |

Remote friends also work through the ordinary `@friends add Bob@coldeve` and the friends panel, and
remote squelches through `/squelch Bob@coldeve`. A name may be given with the mark a line from
Discord shows: `Ann (Discord)` is the local `Ann`, and `Ann@coldeve (Discord)` is `Ann@coldeve`.

## What each feature patches

Each feature is patched only if every ACE method it needs is found. The table is in
`Dereth.Network/Core/PatchTargets.cs`, and the patches in `Dereth.Network/Ace/Patches.cs`.

| Feature | ACE methods | What it does |
|---|---|---|
| `core` | `PlayerManager.Tick` (after) | On the world thread every loop: handles what the hub sent, runs the clock and hands what to send to the link. Without it the network stays off. |
| `tells` | `GameActionTell.Handle` (before) | Sends a tell to an `@` address to the hub, and a tell to a linked offline character of this world to Discord. The sender sees the echo, the away message or `Character not available` once the hub answers. |
| `replies` | `GameActionTalkDirect.Handle` (before) | Sends a reply to a stand-in id back to the remote sender. |
| `friends` | `GameActionAddFriend.Handle`, `GameActionRemoveFriend.Handle` (before); `GameActionRemoveAllFriends.Handle`, `GameEventFriendsListUpdate.WriteEventBody`, `PlayerManager.SwitchPlayerFromOfflineToOnline`, `SwitchPlayerFromOnlineToOffline` (after) | Remote friends in the friends list, their presence at login and logout, a listed character's directory entry, and a linked character's squelches as it leaves. |
| `directory` | `PlayerManager.HandlePlayerDelete` (after) | Takes a deleted character out of the directory at once. Without it the directory notices within half a minute. |
| `appear-offline` | `GameActionSetSingleCharacterOption.Handle` (before and after) | Appearing offline shows the character offline to remote watchers too. |
| `squelch` | `SquelchManager.HandleActionModifyCharacterSquelch` (before) | Squelching an address or a stand-in id; squelching a channel leaves it. |
| `global-room` | `TurbineChatHandler.TurbineChatReceived` (before and after), `TurbineChatHandler.LogTurbineChat` (before) | Sends a line ACE let through in the Global room's carrying room, or a mirrored room, on to the hub. Without it neither the Global room nor Discord mirrors are carried. |
| `names` | `CharacterHandler.CharacterCreateEx`, `AdminCommands.HandleRename` (before) | Refuses `@` in a new or renamed character's name, since it separates addresses. Without it the network stays off. |
| `verify` | `ClientPacket.Unpack` (before) | Answers the hub's challenge on the game port, over HTTPS, while a `/dn verify` waits. Every other datagram is left to ACE. Without it `/dn verify` can't verify in game. |
| `character-screen` | `AuthenticationHandler.SendConnectResponse` (after) | Sends the character screen's message after the character list, when `CharacterScreenMessage` is set. |

Incoming tells, room lines, channel lines and presence need no patch: they go out as ACE's own
messages (`GameEventTell`, `GameMessageTurbineChat`, a friends-list event, system chat).

### When a feature is off

At load, the log names each feature that is off and why (`feature friends is off: this server has
no ACE.Server...`). That feature then behaves as stock ACE and the others still work. The network
needs `core` and `names`. A fault inside a patch is logged (at most once a minute per place) and the
action is left to ACE.

## Stand-in ids and the store

A remote character has no object on this world, so the mod gives each remote identity an id of its
own, from `0x60000001` to `0x6FFFFFFF`: the retail client only makes a tell sender's name a link,
and a reply target, for ids in the player range. The ids are kept in the store, so a reply, a friend
and a squelch keep meaning the same person across relogs and restarts. An id is never reused. One
that no character's friend or squelch names is forgotten a day after its last use (sooner past
10,000), and its identity gets a new one if seen again.

The store (`StorePath`) holds the stand-in ids, whether each character is on the network, its
remote friends and squelches, the operator's blocks, and the directory's listings. It holds no
messages, no presence history and nothing about players' Discord accounts: links, codes and
person friends are the hub's. It is saved within two seconds of a change, through a temporary file
and a rename. Back it up with the server. **Don't delete it**: ids already shown to clients would
come to mean someone else. A store that exists but can't be read keeps the network off until it is
fixed or restored.

## The link

Nothing the mod does blocks the world thread or a login. The link is one `ClientWebSocket` on a
task of its own, sending the key as `Authorization: Bearer`. After a lost link it reconnects after
1, 2, 5, 10 and then every 30 seconds, and a minute after the hub refuses the key (401) or the world
(403). While the link is down, tells, room lines and channel requests wait up to a minute and go in
order when it's back. A tell that waits longer, or gets no answer within 100 seconds, tells its
sender the network is unavailable. Sends are paced to the hub's limits, tells and room lines first.
Shutting down never waits on the network, and waits at most two seconds for the store to reach the
disk.

## Privacy and limits

The mod logs message types, worlds and counts, never the key, a message's text or a link code. A
world learns about players' Discord accounts only through its own characters: whether each is
linked, and who its person friends are. The hub sees addresses and the lines it carries, in transit,
so a world trusts its hub as it trusts its own operators with local tells. Every remote line and name
passes the network's text rules before a client sees it: Windows-1252 only, no name-link markup, no
line breaks, at most 255 characters, and no `^` or `&` in a name.

Other worlds are run by someone else, so the mod holds each to limits of its own:

- it paces each world's room lines and tells to each recipient, and answers its watches only
  within its share;
- it answers `offline` alike for an unknown name, a character off the network and a blocked
  watcher;
- it keeps at most 100 watchers from one world and 1,000 in all, and gives at most 200 new stand-in
  ids to one world, then 200 an hour;
- it refuses a tell that claims to come from one of its own characters, and shows an invitation
  only where a tell from the inviter would be shown;
- a character can have at most 100 remote friends, and squelch 100 remote characters and 50 worlds;
- it sends nothing about its characters to a world or character the operator blocks.

## Checking a build

The harness in `tests/` compiles the mod's ACE-free sources (`Dereth.Network/Core/`) and checks them
without an ACE server:

```
dotnet build Dereth.Network/Dereth.Network.csproj -c Release -p:AceDirectory=<ACE server folder>
dotnet build tests/Harness.csproj -c Release
dotnet <harness>/Dereth.Network.Harness.dll --vectors ../conformance/vectors --hub <dereth-network-hub executable> \
    --ace <ACE server folder> --mod <the built Dereth.Network.dll>
```

A section whose input isn't given (no hub, no ACE folder, no vectors found) is reported as not run
and fails the run as incomplete, unless skipped by name with `--skip <section>`. The sections:

- `vectors`: the conformance vectors in `network/conformance/vectors/`;
- `units`: the pieces one by one, the text rules (against Windows' own conversion, on Windows
  only), the world half on an in-memory link with a test clock, and the link client against a
  flooding socket;
- `hub`: the link and the world half against a real hub on loopback: tells, replies, friends, the
  Global room, channels, refusals, the hold while the hub is away, that no log holds a key or a
  message's text, and, with stand-ins for Discord, room mirrors, `@dn verify`, tells into Discord
  and `/dn say`;
- `targets`: every ACE method the mod patches, found in the given `ACE.Server.dll`, and every hook
  checked against its target as Harmony binds it.

`tests/PatchApply` goes one step further: it applies every hook to its ACE method with ACE's own
Harmony, without running ACE, and takes them off again:

```
dotnet build tests/PatchApply/PatchApply.csproj -c Release -p:AceDirectory=<ACE server folder>
dotnet <out>/Dereth.Network.PatchApply.dll --ace <ACE server folder> --mod <out>/Dereth.Network.dll
```

None of this runs an ACE world. A trial on a running world needs what ACE needs (MySQL or MariaDB
with `ace_auth`, `ace_shard` and `ace_world` from ACE's scripts, the world database imported, and a
`Config.js` pointing at them and the retail client's `.dat` files), then the mod installed against a
hub with federation on, and two clients, one on this world and one on another world on the hub.

## License

AGPL-3.0-only ([LICENSE](LICENSE)), as ACE is. The Windows-1252 best-fit table in
`Dereth.Network/Core/Cp1252BestFit.cs` comes from a Unicode data file used under the Unicode License
v3, whose notice it carries.
