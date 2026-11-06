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

## Known limits

- Messages not translated yet are counted, logged once and dropped. `--report` lists them by opcode.
- A pushed record whose id no 1999 file could hold is held back, so the download can still end.
- The 1999 wire names an enchantment by its spell alone, so each becomes layer 1 on the later wire.
- `0x002D` and `0xF630` have no sender in ac1999's source and are not translated; if one ever comes,
  the report says so.
