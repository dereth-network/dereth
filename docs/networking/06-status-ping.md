# The status ping

**Provenance: this project's own extension.** Retail has nothing like it, and neither has ACE.
It is how a launcher that knows only a world's host and game port learns the world's live status
without logging in and without the operator serving a web page. Empyrean answers it; every other
server ignores it.

The code is `dereth_transport::status_ping` (the datagrams), `empyrean_net::status_ping` (the
server's tokens and limits) and `dereth_launch::probe` (the launcher's two steps).

## Summary

Two steps, both UDP datagrams to the game port **P** (not P + 1), both answered from P:

1. **Hello.** The asker sends a 160-byte request with no token. The server answers with a token
   only, 42 bytes: never more than it was sent, so a forged source address gets nothing larger
   back than it sent.
2. **Ask.** The asker sends the request again, carrying the token. If the server issued that token
   to this address and port in the last two windows, it answers with the status; otherwise it says
   nothing.

No session is made, no account is looked up or created, and nothing is kept per request.

## 1. The header

Every datagram, both ways, starts with 20 bytes shaped as a packet header
(`01-packet-format.md`), then a kind and a version:

| Bytes | Value |
|---|---|
| 0–3 | `ACSP` (`41 43 53 50`), in place of the sequence number. |
| 4–15 | Zero: no flags, no checksum, recipient 0, time 0. |
| 16–17 | `FF FF`: the payload size, larger than any datagram can carry. |
| 18–19 | Zero. |
| 20 | The kind: `01` hello, `02` ask, `81` token, `82` status. |
| 21 | The version: `01` for a request or a token; for a status, the reply format version. |

**Why the size is `FFFF`.** A server's packet reader checks the header's size against the
datagram's before it reads anything past the header. ACE and its forks (ClassicACE among them)
drop a datagram whose size exceeds what follows the header, and GDLE drops one whose size is not
exactly what follows; both drop it before any session, account or login work, and log nothing for
it. A server that does not know the status ping therefore ignores it, and so does an Empyrean with
the ping turned off.

## 2. The requests

Hello and ask have one layout, padded with zeros to **160 bytes**:

| Bytes | Value |
|---|---|
| 0–21 | The header, kind `01` or `02`. |
| 22–25 | The token's window, `u32`. Zero in a hello. |
| 26–41 | The token's code, 16 bytes. Zero in a hello. |
| 42–159 | Zero. |

A server answers only a request of at least 160 bytes.

## 3. The token

The token reply is the header (kind `81`) and the token: 42 bytes.

The token is the server's own business; the asker only echoes it. Empyrean's is stateless:

- **The window** is the server's clock in seconds divided by 30. A token is good in the window it
  was issued in and the next one, so for 30 to 60 seconds.
- **The code** is `HMAC-SHA256(secret, family ‖ address ‖ port ‖ window)` cut to its first 16
  bytes: the family a byte (`4` or `6`), the address 4 or 16 bytes, the port and the window little
  endian. A token is good only from the address and port it was issued to.
- **The secret** is 32 random bytes from the operating system, drawn on the first request and again
  every 20 windows (10 minutes), on a window boundary. The secret before is kept through the first
  window under the new one, so a handshake that spans a rotation still completes, and then it is
  forgotten. Secrets are never written anywhere or logged.

## 4. The status

| Field | Type |
|---|---|
| The header, kind `82`; byte 21 is the reply format version (`1`). | 22 bytes |
| The world's state: `1` open, `2` starting (not open yet), `3` shutting down. Another value is a later version's. | `u8` |
| Players online. | `u16` |
| The era the world plays, as configuration names it (`eor`, `infiltration`). | string |
| The version of the systems' table the next field is written against. | `u16` |
| The world's whole set of systems: a length byte, then the bitfield. | `u8` + bytes |
| The server software (`Empyrean`). | string |
| Its version. | string |
| The world's name. | string |

Integers are little endian. A **string** is a length byte and that many bytes of UTF-8, at most
64; a longer one is cut at a character boundary. The whole reply is therefore always under 1,200
bytes.

A later reply format version only appends fields: a reader takes the fields it knows and ignores
any bytes after them.

### The systems' bitfield

Bit *i* of the bitfield (byte *i* / 8, bit *i* mod 8, lowest bit first) is the *i*-th system of
the era-feature table (`dereth_primitives::EraFeatures`, in declaration order). Systems are only
ever appended to the table, never moved or removed, and each one appended raises the table's
version. Version 1 has 24 systems: ratings, consolidated weapon skills, item spell auras, assessed
armour and ratings, swearing to a lower level, pre-order items and rares, dual wield, weapon
masteries, innate augmentations, aetheria, luminance, contracts, titles, cloaks, trinkets, the
journal, trade, housing, apartments, tinkering, cantrips, spell research, chess and the oath's
experience cost.

A reader takes only the systems its own table and the writer's version both have:

- a system the writer's (older) table does not have, or whose bit was not sent, is **unknown** and
  left to the era's table, never read as off;
- bits past the reader's (older) table are skipped;
- version 0 names no table, so nothing is known.

The same bitfield is what the Dereth client's `--era-features` takes, in text: the version in
decimal, a colon, and the bytes in hex, first byte first. The end of retail is `1:ffff5f`; the
February 2005 world is `1:0000df`.

## 5. Limits and the switch

Empyrean's `[status]` section:

| Key | Default | |
|---|---|---|
| `udp_ping` | `true` | Answer the status ping. Off, it is dropped as any other server drops it. |
| `hellos_per_minute` | `4` | Hellos a minute from one source address. |
| `asks_per_minute` | `4` | Asks a minute from one source address. |
| `replies_per_second` | `50` | Replies a second, tokens and statuses together, to everyone. |

The excess is dropped without an answer. The server counts the drops and logs the count once a
minute, never each one. At most 4,096 source addresses are counted in a minute; a request from one
more is dropped.

## 6. The launcher

For a world with no status document that may run Empyrean (its list entry says so, or says
nothing), the launcher asks with the status ping first: two tries at each step, 600 ms apiece.
Silence means the server does not answer it, and the launcher falls back to the server-tracker
login, which says only up or down. A world that answered is asked again once a minute, within the
default limits. The launcher shows the state, the players, the era and its systems, the software
and its version, and, for a server the player added without naming it, the world's own name.
