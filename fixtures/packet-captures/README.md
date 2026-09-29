# Packet captures

Recorded UDP sessions between the retail client — acclient 00.00.11.4186, built 5 September 2013
— and a local ACE server. They are the record of what that client puts on the wire in the scenes
recorded, and they cannot be regenerated. Account names, passwords and character names are
pseudonymised: replaced in place by stand-ins of the same byte length, with every checksum
recomputed, and nothing else changed.

Each file is one session, one JSON line per datagram:

```json
{"t": 0.007451, "dir": "s2c", "pair": 1, "len": 52, "data": "0000…"}
```

`t` is seconds from the first datagram, `dir` is `c2s` or `s2c`, `data` is the datagram in hex,
and `pair` is which of the server's two adjacent ports it went through: 0 for the port the client
logs in to, 1 for the next one up, where the client sends its `ConnectResponse`. The server's
`ConnectRequest` carries both key-stream seeds in clear, which is what lets a replay rebuild every
checksum offline.

## The recordings

| file | what it is |
|---|---|
| `first-login-walk-jump` | first login, character creation, enter world, walking and jumping |
| `early-inventory-and-casting` | early play: inventory moves and spellcasting |
| `short-second-connection` | a short session on a second connection |
| `login-account-booted` | a login that the server boots |
| `ddd-interrogation-only` | the data-patch (DDD) interrogation and nothing after it |
| `long-solo-play` | the training dungeon end to end: missile, magic and melee combat, equipping, stack splits, a vendor, portals and recall, chat, chests, corpses, locks (18 min) |
| `short-play-with-training` | a short session with skill training |
| `fellowship-one-vassal`, `fellowship-two-monarch`, `fellowship-three-vassal` | three characters recorded at once: a fellowship formed, led, handed over and disbanded; an allegiance sworn and broken; friends and squelches; other players visible and moving; one natural packet-loss event |
| `house-purchase-refused` | a villa purchase the server refuses |
| `house-purchase-and-trade` | a house bought and maintained, nearly every house command, and a completed secure trade with two refused adds |
| `requested-death-vitae-salvage` | a recording made to order: death and vitae, salvage, and other requested scenarios |
| `combat-mode-while-moving` | changing combat mode while moving |
| `melee-attack-run` | a run of melee attacks |
| `long-movement-run` | a long run of movement |
| `pre-relog-play`, `post-relog-attribute-training` | one recording split at its relog, since two logins in one file are two key streams |
| `unclean-logout-short`, `unclean-logout-long` | deliberate unclean logouts: the connection just stops, with no `Disconnect` |

The figures are not repeated here. [`index.json`](index.json) holds each recording's datagram
census, and lists the recordings that end without a `Disconnect`;
[`manifest.json`](manifest.json) says what is *in* each one (what was done, the message census in
the three opcode spaces, the messages it was first to carry, and the slices tests cite); and
[`../message-corpus/`](../message-corpus/) holds every recording's reassembled messages. All three
are generated from the recordings (the manifest's descriptions apart) and checked by
`cargo test -p dereth-corpus`.

## Reading them

- A session starts at the server's `ConnectRequest`, not at a `LoginRequest`: the client resends
  its `LoginRequest` every 2 s until answered, so one recording can hold many.
- Logging out to character select keeps the connection; one `ConnectRequest` can span several
  enter-world cycles.
- ACE restarts each object property's sequence number on every enter-world.
- A proxy trace cannot measure the client's own latency. Time that inside the client.
- Tests read these through the shared readers rather than parsing the files themselves; see the
  corpus tool's README.

## Adding one

Recording a session, pseudonymising it and regenerating the corpus from it is one tool:
[`dereth-corpus`](../../tools/corpus/README.md). Slugs are lower-case kebab-case, two to four
words, naming what distinguishes the recording.

For captures of the retail servers themselves, which this folder does not hold, see
[`dereth-pcap`](../../tools/pcap/README.md).
