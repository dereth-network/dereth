# ac1999 data connection

After a verified grant, `Net` owns `ac1999::connection::Connection`, which runs sequencing, keys,
reassembly, flow reports and resends as the November 1999 client does. The end-of-retail receiver
and its optional-header parser are not used. All clocks are injected; nothing sleeps. A datagram
that fails a check commits nothing: no key, sequence, partial message, flow count or liveness.

## What the client accepts

Every accepted packet has flag `0x2`, names the server's connection id as its recipient, and
carries the table byte the grant set (a time synchronisation may carry a lower one, ac1999's denial
a higher one). The endpoint is also pinned, as client policy. In order of precedence:

| Flags | Meaning | The client |
| --- | --- | --- |
| `0x200` | a repeated grant, or ac1999's keepalive | acknowledges it again |
| `0x10` | a clock synchronisation (8-byte time) | waits a minute before asking again |
| `0x20000` | the server ends the session (header-only checksum) | answers `0x20002` and closes |
| `0x100000` | a network error | header-only: closes unless data arrived in the last two minutes; with a body, as ac1999 sends it: closes |
| `0x100` | a server switch (8-byte time) | answers `0x102`; the connect progress advances |
| `0x2000` | ac1999's list of sequences it will not resend | stops waiting for them |
| `0x80` | a list of sequences the server will not resend | stops waiting for them |
| `0x8` | a list of sequences the server wants resent | resends those it holds, denies the rest |
| `0x20` | a flow value | records it |
| `0x800` | application fragments | delivers them in sequence order |

Lists are a u32 count and that many u32 sequences. The client reads as many words as the count says
and the packet holds, skipping zeros. A resend request counting zero, or any list counting more
than 114, is rejected. A
sequenced `0x8` or `0x20` packet also announces the sender's latest sequence, which the client then
waits for, so a lost last packet shows up as missing. Other flags carry no field the client reads.

The connection is up on the first packet that reaches the lists or the data, not on a server
switch: that is when `Event::Connected` is published.

## Sequences and keys

The server's first accepted data sequence is 2; sequence 1 counts as already received. Each new
sequence draws the next key from the inbound stream. A packet that skips ahead draws keys for the
sequences it skipped and holds them; a missing packet that arrives later uses its held key, and a
packet with no held key at or below the highest sequence is a duplicate and is dropped. A data
packet whose checksum fails is rejected whole, so its sequence stays missing. Messages reach the
caller in sequence order; a skipped sequence is passed over.

Client policy bounds a skip ahead to 256 sequences and assemblies to 16. A received message may have
as many parts as the largest dat file a download carries needs (under four megabytes inflated, so
about 9,600 parts of 444 bytes), and the client holds at most three such messages' worth of
payload. The client's own messages are at most 64 parts. At most 64 messages wait for the caller;
when a filled hole releases more, the rest stay parked in order and follow as the caller polls.

## What the client sends

Every packet repeats the newest time byte the server has sent (starting at 1) and the grant's table
byte. Data and control packets carry, after the header and outside its length, a flow word: the
bytes received since the server's time byte last advanced. When it advances (to anything but 1),
the client reports the finished period's count in a `0x1002` packet of its own, sequence 0, time
byte one less than the new one, the count as its 4-byte body.

| Packet | Flags | Sequence | Body |
| --- | --- | --- | --- |
| data | `0x802`, `0x803` when resent | 2, 3, ... | fragments |
| resend request, every quarter second while packets are missing | `0xA` | the last data sequence sent | count, up to 113 missing sequences |
| heartbeat, every two seconds otherwise | `0x22` | the last data sequence sent | the highest sequence received |
| denial of a resend it cannot serve | `0x82` | 0 | count, sequences |
| server-switch reply, acknowledgment, flow report | `0x102`, `0x202`, `0x1002` | 0 | none, none, the count |
| log-off | `0x42` | 0 | none |

Until the server synchronises the clock, and then once a minute, a control packet also asks for a
synchronisation with `0x10`. Data is split into 444-byte fragments, each its own packet, keyed by
the next outbound key. Blob ids start at `0x8000000000000001`. Messages go out on queue 6, the
login queue; the client's other sending queues are 1, 5 and 7, and it receives on 3, 4 and 7.

A resend request is served from the 256 packets kept: those held are resent with `0x803`, the same
sequence and key, and the current flow word and time byte (at most once a quarter second each, as
policy). The rest are denied with `0x82`; because ac1999's server reads denials under `0x2000`, the
same sequences (from 2 up) follow in a `0x2002` list without the flow word. The goodbye is the
log-off followed by `0x8002`, the reset ac1999's server ends a session on.

Thirty seconds of silence, or a message that gains no part for that long, closes the connection
(policy): a large file still arriving keeps its message open however long it takes.
Teardown clears parked data, assemblies, output and the resend cache, and leaves `Closed`.

## Boundary

Nothing of the later transport sees any of this. Messages reach the relay's translation through the
1999 codecs ([login](09-ac1999-login.md), [objects](10-ac1999-objects.md),
[dat downloads, the world stream and movement](11-ac1999-world.md)); others are counted, logged once
and dropped. Tests drive synthetic datagrams, with an in-process stand-in for the server; none
connects anywhere or uses real credentials.
