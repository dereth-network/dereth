# The Dereth Network — the specification

The Dereth Network links Asheron's Call worlds through a hub, so that a player on one world can
send a tell to a character on another and get the reply, see opted-in friends' online status
across worlds, and talk in a shared Global room and in invite-only channels. A player on the
**unmodified final retail client** gets all of it through their own world's server: the world
speaks the network's protocol to the hub, and the client sees only messages it already reads.

This folder is the protocol's specification. Any world server (an ACE mod, Empyrean, a GDLE patch,
a private fork) and any hub can implement it from these pages alone. The conformance suite
([06-conformance.md](06-conformance.md)) checks the rules most easily got wrong; these pages stay
the definition.

## Provenance

Each page opens with a provenance line. Besides the three kinds the behaviour specification uses
([../README.md](../README.md)), this folder has one of its own:

| Provenance | Means |
|---|---|
| **network specification** | A rule of this protocol: normative. An implementation that does otherwise is not conformant. |
| **observed at runtime** | What the final retail client does, which the protocol is shaped around. |

The words **must**, **must not**, **should** and **may** are used in their usual normative sense.

## Pages

| Page | What it says |
|---|---|
| [01-addresses-and-settings.md](01-addresses-and-settings.md) | `Name@world`, world slugs, channels, a character's three settings, a world's federation flag, stand-in ids, persons and the alts setting, and who learns what |
| [02-link-protocol.md](02-link-protocol.md) | The world-to-hub link: the WebSocket, the key, every message, ids and answers, limits, the one-minute hold, the person layer's requests and views |
| [03-world-behaviour.md](03-world-behaviour.md) | What a world shows a retail player: tells, replies, errors, friends, rooms, channels, squelch, commands, a lost hub, linking characters and person friends |
| [04-text-rules.md](04-text-rules.md) | What every receiving world does to every remote line before a client sees it |
| [05-hub.md](05-hub.md) | The hub: the world registry and its HTTP API, world operators (sign-in, registration, verification, approval, keys, reports), routing, presence, rooms, channels, privacy, persons (link codes, the site's routes, what worlds hear), and Discord later |
| [06-conformance.md](06-conformance.md) | The conformance suite: the checks a world and a hub run |

## The shape of it

```
 retail client --UDP-- world A --WebSocket--+
                                            |-- hub --- registry (slugs, keys, federation flags)
 retail client --UDP-- world B --WebSocket--+
```

- **No game account.** A character opts in with `/dn on`; its home world vouches for it as
  `Name@slug`. A player may link characters to their Discord account on the site, so that friends
  find them on any of them; nobody learns that two characters are one person unless the player
  shows it, signed in on the site.
- **The home world decides.** Whether a character is shown online, and who may reach it, is its
  own world's answer, from the character's settings. The hub routes; it holds no presence history
  and never logs a message body.
- **The receiving world never trusts the sending world.** Every remote line passes the text rules
  on the way to a client.
- **Off by default.** A world that does not turn the network on behaves exactly as its emulator
  always did; a world whose operator says no is refused everywhere.

## Licence

These pages are licensed under the Creative Commons Attribution 4.0 International licence
([LICENSE](LICENSE)), so that every implementer may copy and adapt them.
