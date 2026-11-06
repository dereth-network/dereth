# ac1999 handshake

The November 1999 client's protocol, which ac1999's servers speak. Only the relay speaks it; see
[the relay](00-relay.md). In these pages "the client" is the relay's side towards the server, which
behaves as that client does. See [the data connection](08-ac1999-data-connection.md) for what happens
after the grant.

## Header and checksums

All numbers are little-endian. A datagram is at most 484 bytes, including the 20-byte header.

| Offset | Type | Meaning |
| --- | --- | --- |
| 0 | u32 | Sequence |
| 4 | u32 | Flags |
| 8 | u32 | Recipient |
| 12 | u32 | Checksum |
| 16 | u16 | Body length |
| 18 | u8 | Time |
| 19 | u8 | Table |

The header's share of a checksum is the sum of its words with `0x03291975` in place of the
checksum, plus `20 << 16`. A body adds the sum of its words with no length term; trailing bytes
contribute at shifts 24, 16 and 8. Login, grant, acknowledgment and every control packet use this
unkeyed sum although their flags carry `0x2`.

Data adds, per fragment, the hash of its 16-byte header and the hash of its payload (each with its
own length term), mixed with a key:

```text
header_hash + (sum(fragment_header_hash + fragment_payload_hash) XOR key)
```

Each direction has its own ISAAC stream. The grant's first seed keys what the server sends, its
second what the client sends; a zero seed still initializes the generator (its first two words are
`0x182600f3` and `0x300b4a8d`). Keys belong to sequences in order: see the data connection.

## Login

The login request is sequence 1, flags `0x2`, recipient 0. Its body:

| Field | Value |
| --- | --- |
| u32 | `0x075BCD15` |
| u16 and bytes | the version, `4.35752`, counted with its NUL, padded to four bytes |
| account field | `0xBEEFBEEF`, 20 characters, a reserved word |
| account field | `0xBEEFBEEF` and 24 zero bytes |
| u32 | 1 |
| u32 | the ticket's length: 0, no ticket |
| u32, u32 | the portal and cell dat iterations |
| u32, u32 | 0, then `0xFFFFFFFF` |

With the default version the body is 96 bytes. The 20 characters are `account:password`,
lowercased as the November 1999 client lowercases them, so an account made by either client is the
same account. They must fit the field whole (20 bytes with the colon); nothing is truncated. The
account name may not contain a colon, and both parts are printable ASCII. `LoginMetadata` carries
the dat iterations, which the relay learns from Dereth, so a server holding the same files has none
to send.

The login is plaintext. `Outgoing`, `Packet` and `Handshake` debug output omit credential-bearing
bytes; a caller must not log the returned datagram either.

`empyrean_relay::handshake::Handshake` is socket-free: `request(LocalTime)` returns the datagram
(at most 20 sends two seconds apart, then a timeout; this timing is client policy) and
`receive(bytes, source)` takes the answer. The source must be the configured endpoint.

## Grant and acknowledgment

A grant's flags include `0x202` and not `0x10000`. Its header names the server's connection id,
which must be below 400 and not zero, and a table byte the client adopts for every later packet.
Its checksum is the unkeyed sum. The body is an ignored f64, the world clock's f64 epoch, the u32
recipient id the client adopts, a u32-counted version with no terminator or padding, and the two
seeds. Anything after the seeds is ignored. The version is accepted when the number it begins with
equals the client's own, so text after the number does not matter.

The acknowledgment is sequence 0, flags `0x202`, the adopted recipient, the adopted table byte and
no body. An identical repeated grant is acknowledged again without resetting either key stream; a
different one fails. A network error (`0x100000`) with an ordinary checksum before the grant is
ac1999's server refusing the login; the handshake reports it with its string and table ids, where
the November 1999 client would only time out. A grant is not world entry.
