# ac1999 login application boundary

Character enumeration, login news and the two-phase selection exchange, as the relay carries them
between the server and Dereth's later session.

## Dedicated codecs

`empyrean_relay::codec` implements paired message codecs. No 1999 bytes reach Dereth: each message
is read here and written again as the later one.

| Opcode | Body after opcode |
|---|---|
| `F658` roster | count, zero reserved dword; each row: ordinal slot, id, string, deletion seconds; fixed account blob |
| `F65A` news | two strings; only the second is displayed |
| `F659` error | one error-code dword |
| `F7C7` ready | empty |
| client `F7C8` | empty request to enter the world |
| client `F657` | chosen id, fixed account blob |
| client `F656` | the account field, then the created character |
| `F643` creation answer | a code; on success (1) the new roster entry without its slot |

Strings use a u16 byte count including the NUL, Windows-1252 bytes, then zero
padding to the absolute stream position's next four-byte boundary. A zero count
means empty and consumes only two bytes, with **no alignment**. The fixed account
field is a marker dword, 20 NUL-padded account bytes, then a reserved dword. The server's roster
has zero for the marker; the client's selection writes `0xBEEFBEEF`. When reading, only the
characters before the first NUL count: the marker, the reserved word and the bytes after the NUL
are not checked, as the client does not check them.

The decoder consumes the whole message and refuses truncation, embedded NULs,
missing terminators, nonzero string padding or roster reserved fields, nonordinal roster slots,
zero or duplicate ids, and trailing bytes. Resource policy limits rosters to 64
rows, names and the unused news prefix to 255 encoded bytes, and news to 16 KiB.
These are bounded compatibility policies, not historical account/name limits.

Roster rows preserve order and deletion timers. The relay sends them as the later character list
and the news as the later screen message, and names the world itself. ac1999 does not advertise slot
capacity: the relay offers at least five.

The relay sends `F656` from the character list: version 1, heritage,
gender, seven face choices, the headgear color, each article's style and color, six f64 shades,
the template, six attributes, the player and requested-access words, a counted list of skill
advancement classes, the name, the heraldry symbol and colors, the start area, and a closing word
that sums the integer choices. The server answers `F643` and then sends a fresh roster.
The later wire numbers heritages from one and sexes as Male 1, Female 2; this one numbers them by
their place in the file's lists, heritages from zero and sexes Female 0, Male 1. The relay translates.
Deletion and restoration remain unsupported.
Repeated rosters in new blobs refresh the view; duplicate packets do not.

## Selection and entry

Dereth's later session sends `F7C8`, which goes up on the login queue, 6. `F7C7` reaches Dereth as
`F7DF`, and its `F657` goes up with the id it names and the roster account field, ignoring its
later-format account parameter. The selected row is checked again after
any intervening roster refresh. A removed or now-deleting row closes the exchange.
The server authenticates character ownership itself. `F7C7` reaches Dereth as `F7DF`, and Dereth's
later session keeps its own entry deadline.

Messages enter the reliable sender on queue 6, with the
connection's blob ids, keys and resend cache. Generic
`Transport::send` still refuses ac1999, including unsupported gameplay and later
login encodings. Known malformed or wrong-queue messages produce metadata only;
unknown bodies are discarded. Runtime diagnostics retain the most recent 64
entries independently of session-model consumption, so not polling diagnostics
cannot block login. Normal session events still require the host's usual drain.

## Proof and remaining boundary

Synthetic byte vectors cover both directions, string alignment, CP1252, counts,
truncation and invalid fields. An in-process stand-in runs the whole exchange through the relay on a
virtual clock.

[Objects](10-ac1999-objects.md) and [dat downloads, the world stream and
movement](11-ac1999-world.md) follow.
Player-description/qualities decoding, the remaining object updates, classic-asset readiness,
gameplay and front-end wire selection remain outstanding.
Unsupported player-description messages cannot set `Playable`. A received player
identity alone establishes neither a physical object nor asset readiness.
No official endpoint, deployment version or account contract is inferred here.
