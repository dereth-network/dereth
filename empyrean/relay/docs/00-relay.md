# The relay

`empyrean-relay` lets a Dereth client play an ac1999 server. Towards Dereth it is a server of the later
protocol, and towards ac1999 it is a November 1999 client, so Dereth's own tree carries one wire
format. It is AGPL, like the rest of Empyrean. Nothing in `core/` or `dereth/` depends on it.

A launcher that lists a world played this way starts the relay when the player presses Play and stops
it when the client ends. It ships in Empyrean's package, beside the server.

**Run it on the player's machine.** It sees the player's password. Towards Dereth it listens on the
loopback address and refuses any other. Towards the server it listens for nothing: each session has
one ephemeral socket `connect()`ed to the server, so it hears that peer alone; that socket binds the
unspecified address only when the server is not on the loopback address.

## A session

1. Dereth logs in and the later handshake finishes. `account:password` must fit the 1999 login's 20
   lowercased bytes, or Dereth is told so and the session ends.
2. The relay asks what Dereth holds (`0xF7E5`), and only then logs in upstream with those iterations.
3. The server pushes what the client lacks, then the character list. The later begin names every
   record, so the relay holds the push and serves it whole: `0xF7EC`, `0xF7E7`, each `0xF7E2`.
4. When Dereth ends the download (`0xF7EA`), it gets the character list and the world's name.
5. From there each message is translated as it comes.

## What it keeps

- A game-event stamp sequence per object: the client holds back a stamp that leaves a gap.
- A sequence byte per property of each object, which the 1999 wire does not have.
- The objects already created, so a repeated `F745` becomes `F7DB`.
- The account field the character list carried.
- The character's option word: the later client changes one option at a time, the 1999 client sends
  the word.
- The squelch list, which the later wire sends whole and a 1999 client shows the server at each login.

Experience rides two Int keys on the 1999 wire and becomes the later Int64 keys. Combat modes are
numbered in sequence there and become the later bits.

## What it refuses

ac1999 is GDLE underneath and still reads most later actions (trade, housing, salvage). The relay
forwards only the 1999 client's own outbound set, and answers anything else with a line in Dereth's
chat. Actions Dereth sends on its own, with nothing a player did to explain them, are dropped in silence.
Every action of the 1999 set goes up. The two later numbers the 1999 client uses for something else,
account squelch and global squelch, are refused.

## What it presents

The status ping answers as a Launch world, from `EraFeatures`. Launch is what the relay's world plays:
run Dereth with `--era launch`, or through a launcher that read the ping.

## How many players the world has

The 1999 wire tells a client no count at login, so the relay's `Login_WorldInfo` (`0xF7E1`) says 0.
Once a character is in the world (Dereth's `0x00A1`), the relay types ac1999's `@pop` command
upstream, as the player's own chat would go (`0x0015`), and again every 15 minutes while the session
lasts. The server answers in a chat line, `Current world population: N` (`0xF62C`). The answer to
the relay's own ask, matched by that exact text, stays out of Dereth's chat; a `@pop` the player types
is answered in the chat as usual. An ask unanswered for a minute is the session's last.

Each count goes to the Dereth Network's hub (`POST /v1/players`, `docs/network/05-hub.md`), or to
the one `DERETH_PLAYER_COUNT_HUB` names, as a Dereth client reports: the server's address and port as
`--upstream` names them, and the count, nothing else, on a thread of its own, and a report that
fails is forgotten. A server on this computer or a private network, by its name or by the address
it stands for, is never reported.

## Known limits

- Messages not translated yet are counted, logged once and dropped. `--report` lists them by opcode.
- A pushed record whose id no 1999 file could hold is held back, so the download can still end.
- The 1999 wire names an enchantment by its spell alone, so each becomes layer 1 on the later wire.
- `0x002D` and `0xF630` have no sender in ac1999's source and are not translated; if one ever comes,
  the report says so.
