# The UDP packet format

**Provenance: wire format.** The layouts below are the ones server emulators and the community
protocol catalogue already document; they are restated here in the form this project needed, and
cross-checked against this project's own encoder, its decoder, and a corpus of recorded sessions. No
recorded payload is reproduced.

## Summary

Every byte the client exchanges with a server travels in UDP datagrams shaped like this:

```
+--------------------------------------------------+  offset 0
|  transport header                     20 bytes   |
+--------------------------------------------------+  offset 20
|  optional header sections, in ascending flag     |
|  order, back to back, no padding between them    |
+--------------------------------------------------+
|  blob fragment 0 : 16-byte header + payload      |
|  blob fragment 1 : 16-byte header + payload      |
|  ...                                             |
+--------------------------------------------------+  offset 20 + size
```

A 32-bit additive checksum covers the header and every section. On "encrypted" packets the payload
half of that checksum is exclusive-ORed with one value drawn from an ISAAC stream whose seed was
handed to the client **in clear** during the handshake. Nothing else is encrypted: the protocol has no
confidentiality, only a per-packet message authenticator that is cheap to compute and hard to forge
without the seed.

Everything is little-endian except the socket-address fields inside two of the optional sections,
which are in network byte order because they are raw socket addresses copied onto the wire.

The header's size field counts the bytes **after** the 20-byte header. The receiver requires that
parsing consume exactly that many: **a single leftover byte fails the whole packet.**

> Pinned by `core/transport/src/wire/packet.rs :: a_single_leftover_byte_fails_the_packet` and
> `:: datalen_must_match_the_datagram`.

## 1. The transport header — 20 bytes

| offset | size | field (ACE's name) | meaning |
|---:|---:|---|---|
| 0x00 | 4 | `Sequence` | Packet sequence number for this direction of this connection. 0 means "unsequenced" and is used for the handshake and for pure acknowledgements. The sender's counter starts at **1** and is advanced before it is used, so the first sequenced packet in each direction carries **2**; the counter skips 0 on wrap. |
| 0x04 | 4 | `Flags` | The flag bits of §2. |
| 0x08 | 4 | `Checksum` | §5. |
| 0x0C | 2 | `Id` | The **receiver's** id for this connection: the sender writes the id its peer gave it. The receiver indexes a 256-entry table with it, so a value of `0x100` or more is dropped. |
| 0x0E | 2 | `Time` | The sender's current half-second interval counter, wrapping at 16 bits. It is flow accounting, not a clock — do not read a time out of it. |
| 0x10 | 2 | `Size` | Bytes following the header. Must be below `0xFFE1` (65,505) or the packet is dropped. |
| 0x12 | 2 | `Iteration` | Connection generation. Set from the peer's connect request and echoed on every packet. A packet whose iteration is *older* than the stored one is dropped; a *newer* one is accepted only if it carries a connect request — that is how a re-handshake replaces a stale connection. ACE always sends `0x01`, so this path is never exercised against it. |

The header is packed with no padding: exactly 20 bytes.

**Who fills in what.** Ordinary data packets take the connection's assigned id and the current
interval and iteration. Standalone control packets — login request, connect response, disconnect,
keep-alive, world login request — are built by a different path that stamps `Sequence = 0` and the
single section's flag mask, and **three of those senders write a hard zero into the id and the
iteration** regardless of what the server assigned: the login request and the world-login retry,
because there is no connection yet, and the keep-alive, which has one. The interval is zero on every
standalone section, including the goodbye and the connect response.

> This matters because it is invisible locally. A server that looks a session up by id before
> dispatching will discard the keep-alive as stale; behind a NAT, the mapping then expires about two
> minutes in and the connection dies. Route on the listening socket and the flag bits **before** any
> session lookup.
>
> Pinned by `core/client-net/tests/cpu/net/recipient_id.rs`:
> `:: the_keep_alive_recipient_is_a_hard_zero_in_retail`,
> `:: the_flow_queue_stamps_the_assigned_net_id_on_every_packet`,
> `:: every_recorded_keep_alive_carries_recipient_zero`, and
> `:: this_build_takes_the_null_arm_at_the_keep_alive_and_the_receiver_arm_at_the_goodbye`.

## 2. The flag bits

Exactly 22 bits are defined. **A packet with any undefined bit at or above `0x100` set is rejected**;
undefined bits below that are neither consumed nor rejected.

| bit | value | name | see |
|---|---:|---|---|
| 0 | `0x00000001` | Retransmission | This packet is a resend of a sequence number already sent. The receiver does not treat it specially beyond its own bookkeeping. |
| 1 | `0x00000002` | EncryptedChecksum | The payload half of the checksum is ciphered — §5.3. |
| 2 | `0x00000004` | BlobFragments | One or more fragments follow the optional sections. Requires an established connection. |
| 3–7 | `0x08`–`0x80` | *(undefined)* | Neither read nor rejected. |
| 8 | `0x00000100` | ServerSwitch | §3.1 |
| 9 | `0x00000200` | LogonServerAddr | §3.2 |
| 10 | `0x00000400` | EmptyHeader1 | §3.3 |
| 11 | `0x00000800` | Referral | §3.4 |
| 12 | `0x00001000` | RequestRetransmit | §3.5 |
| 13 | `0x00002000` | RejectRetransmit | §3.6 |
| 14 | `0x00004000` | AckSequence | §3.7 |
| 15 | `0x00008000` | Disconnect | §3.8 |
| 16 | `0x00010000` | LoginRequest | §3.9 |
| 17 | `0x00020000` | WorldLoginRequest | §3.10 |
| 18 | `0x00040000` | ConnectRequest | §3.11 |
| 19 | `0x00080000` | ConnectResponse | §3.12 |
| 20 | `0x00100000` | NetError | §3.13 |
| 21 | `0x00200000` | NetErrorDisconnect | §3.14 |
| 22 | `0x00400000` | CICMDCommand | §3.15 |
| 23 | `0x00800000` | *(undefined)* | **Packet rejected if set.** |
| 24 | `0x01000000` | TimeSync | §3.16 |
| 25 | `0x02000000` | EchoRequest | §3.17 |
| 26 | `0x04000000` | EchoResponse | §3.18 |
| 27 | `0x08000000` | Flow | §3.19 |
| 28–31 | `0x10000000`–`0x80000000` | *(undefined)* | **Packet rejected if set.** |

> Pinned by `core/transport/src/wire/header.rs :: nineteen_optional_bits`,
> `:: undefined_bits_reject_only_at_or_above_0x100`,
> `:: datalen_rejection_threshold`, `:: rec_id_must_fit_the_receiver_table`.

## 3. The optional sections

### 3.0 Wire order, and how it is guaranteed

Sections appear on the wire in **strictly ascending flag-mask order**, back to back, with **no padding
between them** and **no per-section length or type tag**: the flag bits are the only framing. Both
ends enforce the order independently — the sender sorts by mask before emitting, the receiver walks
its section table from the lowest mask upward, consuming one section per bit present.

A packet can hold at most **32 optional sections** and **29 fragments**.

Each section also carries local behaviour bits that never appear on the wire but decide how the packet
is handled. Five of them matter to a reimplementation:

| behaviour | effect |
|---|---|
| **disposable** | Stripped when the packet is cached for possible retransmission, and does not by itself make the packet sequenced or encrypted. |
| **exclusive** | Must be the only thing in the packet; the packet is rejected if it also carries fragments or a second section. |
| **pre-connection** | May appear on a packet whose connection is not yet established. Without it, such a packet is rejected. |
| **time-sensitive** | The payload is rewritten immediately before every send, including every resend, which invalidates and forces a recomputation of the checksum. |
| **priority** | Lets the packet leave even when send-side pacing says no. |

> Pinned by `core/transport/src/wire/optional.rs :: table_is_ascending_and_complete`,
> `:: exactly_twelve_headers_are_disposable`, `:: net_error_disconnect_is_not_disposable`, and
> `core/transport/src/wire/packet.rs :: sections_serialise_in_ascending_mask_order`,
> `:: exclusive_headers_must_be_alone`, `:: caching_strips_disposable_headers`,
> `:: section_and_fragment_limits`, `:: every_optional_header_round_trips_at_its_documented_length`.

### 3.1 ServerSwitch — 8 bytes

| offset | size | type | meaning |
|---:|---:|---|---|
| 0x00 | 4 | u32 | Monotonic switch stamp, compared with wrapping 32-bit arithmetic against the last stamp of this type, so a repeated switch is ignored. |
| 0x04 | 4 | u32 | Switch type: 0 = world switch, 1 = logon switch. |

A world switch retargets the client's idea of "current world server" at the connection this packet
arrived on; a logon switch retargets the login server, and the world server too if the client is not
yet in the world. ACE emits this with the *constant* in the first dword and zero in the second — that
is, a world switch with a stamp — but never in normal operation.

### 3.2 LogonServerAddr — 16 bytes

A raw socket address: a 2-byte family (2 = IPv4, little-endian), a 2-byte port and a 4-byte address
**both in network order**, and 8 zero bytes. It retargets the remaining login-request retries at a
different address. Not implemented by ACE.

### 3.3 EmptyHeader1 — 0 bytes

No payload and no handler. Its only effect is that the flag bit is consumed so the packet is not
rejected. Not implemented by ACE.

### 3.4 Referral — 32 bytes

| offset | size | type | meaning |
|---:|---:|---|---|
| 0x00 | 8 | u64 | Handshake cookie to present to the referred-to server. |
| 0x08 | 16 | socket address | Target server; port and address in network order. |
| 0x18 | 2 | u16 | The connection id the client must use for that server. |
| 0x1A | 2 | — | Padding. The structure is 32 bytes and the reader consumes 32. |

If a connection with that id already exists, only the cookie is stored; otherwise the referral is
queued and the world-login cadence of §3.10 starts. ACE hand-assembles the socket address — a literal
family word, then the port big-endian, then the four address bytes.

> Pinned by `core/client-net/tests/cpu/net/referral.rs :: the_referral_bytes_are_ace_s_bytes` and the
> behaviour tests beside it.

### 3.5 RequestRetransmit (NAK) — 4 + 4·n bytes

| offset | size | type | meaning |
|---:|---:|---|---|
| 0x00 | 4 | u32 | Count. Must be below `0x73`, so at most **114** entries. |
| 0x04 | 4·n | u32[] | The sequence numbers the sender is missing. |

The client's own request caps at **114**. ACE uses 115 and always puts its
last-received-plus-one first, so ACE's cap is one larger than the client's: the client accepts 114 and
rejects a section claiming 115 or more.

### 3.6 RejectRetransmit — 4 + 4·n bytes

Same layout as §3.5. It means *those sequence numbers you asked for carried nothing; stop asking*. The
receiver forgets each named id and the retry loop for it ends.

### 3.7 AckSequence (PAK) — 4 bytes

One dword: the highest sequence number received **contiguously**. It is cumulative — everything at or
below it may be dropped from the peer's retransmit cache. It is disposable, so a packet whose only
content is an acknowledgement is unencrypted and carries sequence 0.

### 3.8 Disconnect — 0 bytes

Zero-length, disposable and exclusive. The client sends one standalone packet per live connection when
it logs off, and then **goes silent**: every subsequent send is suppressed. The client never handles an
inbound one — a server that sends it gets no reaction, and the connection dies through the 140-second
timeout instead.

> Pinned by `core/client-net/tests/cpu/net/disconnect.rs :: the_goodbye_is_the_bytes_retail_sends`,
> `:: the_client_goes_silent_after_the_goodbye`, `:: every_connection_gets_its_own_packet`.

### 3.9 LoginRequest — variable

| field | type | meaning |
|---|---|---|
| client version | pack string | Always the literal `"1802"`. |
| auth length | u32 | Byte count of the authenticator that follows. Must be below `0xFFE1` and must not run past the end of the packet. |
| authenticator | — | Below. |

A **pack string** is a 16-bit length (or `0xFFFF` followed by a 32-bit length when the length reaches
`0xFFFF`), then the bytes, then zero to three zero bytes so that the whole is a multiple of four.

The authenticator:

| offset | size | type | meaning |
|---:|---:|---|---|
| 0x00 | 4 | u32 | Auth type: 1 = account only, 2 = account and password, `0x40000002` = GLS ticket. |
| 0x04 | 4 | u32 | Auth flags. Bit `0x2` means an "account to log on as" string follows, for impersonation. The client never sets it. |
| 0x08 | 4 | u32 | A connection sequence number taken from the real-time clock at startup. |
| 0x0C | var | pack string | Account name, lower-cased. |
| … | var | pack string | The impersonation account — **present only when the flag bit is set**. |
| … | 4 + n | length-prefixed bytes | Crypto data. Always empty in this client. |
| … | 4 + n | length-prefixed bytes | The password or ticket. **The inner data starts with its own compressed length** — one byte for lengths below 128 — not a 16-bit one, and carries no padding, because it is written in the archive string form rather than the pack string form. |

> **Difference from ACE.** ACE reads the impersonation string *unconditionally*. It works by accident:
> with the flag clear, the four bytes it consumes are the empty crypto field's length word plus
> alignment, which decodes as an empty string. A client that ever set the flag would desynchronise
> ACE's parser.
>
> Pinned by `core/transport/src/conn.rs :: the_login_request_carries_the_literal_1802`,
> `:: account_to_logon_as_is_conditional`, `:: the_login_request_packet_is_unsequenced_and_plaintext`,
> and `core/transport/src/wire/optional.rs :: login_request_length_is_computed_from_its_own_fields`,
> `:: pstring_pads_to_four`.

### 3.10 WorldLoginRequest — 8 bytes

One dword pair: the referral cookie from §3.4. Sent standalone to the referred address with sequence,
id and iteration all zero, **retried every 0.333333333 s up to 840 times** — 280 seconds — after which
the entry is dropped and a world-connection error is raised.

> Pinned by `core/client-net/tests/cpu/net/referral.rs ::
> the_world_login_request_is_28_bytes_with_a_zeroed_header_and_the_cookie`,
> `:: the_resend_cadence_is_one_third_of_a_second_on_the_clock_alone`,
> `:: the_cadence_gives_up_after_the_computed_cap_and_raises_a_world_connection_error`.

### 3.11 ConnectRequest — 32 bytes, server to client only

| offset | size | type | meaning |
|---:|---:|---|---|
| 0x00 | 8 | f64 | The server's game time. The client does **not** set its clock from this field; the clock is set by the first time-sync section. |
| 0x08 | 8 | u64 | Handshake cookie the client must echo. |
| 0x10 | 4 | u32 | The id the client must stamp into the transport header when talking to this server. |
| 0x14 | 4 | u32 | Cipher seed for the **server → client** direction. |
| 0x18 | 4 | u32 | Cipher seed for the **client → server** direction. |
| 0x1C | 4 | — | Padding, because the structure begins with a double. ACE writes an explicit zero. |

The two seed field names are written from the *server's* point of view, which is the single most
common way to wire the two streams backwards.

> Pinned by `core/transport/src/conn.rs ::
> connect_request_round_trips_and_names_its_seeds_from_the_servers_view`.

### 3.12 ConnectResponse — 8 bytes

The cookie from §3.11, echoed verbatim — and sent **to the server's port + 1**. See
[03-connection-state-machine.md](03-connection-state-machine.md) §3 for why.

> Pinned by `core/transport/src/conn.rs :: connect_response_goes_to_port_plus_one`,
> `:: the_connect_response_packet_echoes_the_cookie_in_the_clear`.

### 3.13 NetError — 8 bytes

Two dwords: a string hash naming the error, and a table id that is always 8. The client resolves the
pair through its own string table, so a reimplementation should resolve it the same way rather than
hard-coding English text. Both fields are optional if the buffer is short: the writer emits the first
if four bytes remain and the second if eight do. The code list is in
[03-connection-state-machine.md](03-connection-state-machine.md) §6.

### 3.14 NetErrorDisconnect — 8 bytes

The same 8-byte payload, but **exclusive and not disposable**, so the packet carrying it is sequenced
and encrypted. Receiving one moves the connection into "disconnect received"; sending one moves it
into "disconnect sent".

### 3.15 CICMDCommand — 8 bytes

| offset | size | type | values |
|---:|---:|---|---|
| 0x00 | 4 | u32 | Command: 1 = no-op, `0x6C705245` = echo reply, `0x71655245` = echo request. |
| 0x04 | 4 | u32 | Parameter; zero for the no-op. |

The client sends the no-op to the server's **port + 1** every **220 intervals**, that is every
**110 seconds**, as a NAT keep-alive for the second port. ACE recognises the flag on its second
listener and ignores the contents.

**Every recorded keep-alive carries an id and an iteration of zero**, even on a named connection — see
§1. That is the whole reason a server must route on the listener and the flags before any session
lookup.

### 3.16 TimeSync — 8 bytes

One double: the sender's current game time. Time-sensitive, so it is rewritten immediately before
every send and is fresh even on a retransmission; not disposable, so its packet is encrypted. Handling
is in [02-reliability-and-flow.md](02-reliability-and-flow.md) §6.

### 3.17 EchoRequest — 4 bytes

One float: the sender's monotonic local time, restamped before every send.

### 3.18 EchoResponse — 8 bytes

| offset | size | type | meaning |
|---:|---:|---|---|
| 0x00 | 4 | f32 | The request's value, echoed. |
| 0x04 | 4 | f32 | Seconds the responder held the request before answering, recomputed at send time. |

Round-trip time is `local_time − echoed − held`, so queuing delay on the responder is subtracted out
rather than counted as latency.

### 3.19 Flow — 6 bytes

| offset | size | type | meaning |
|---:|---:|---|---|
| 0x00 | 4 | u32 | Bytes received from the peer during the interval that just ended, including the 20-byte headers. |
| 0x04 | 2 | u16 | The peer interval id those bytes belong to. |

**This is the only section whose length is not a multiple of four**, and being the highest mask it is
always last. That is not a coincidence and it is load-bearing — see §5.2.

> Pinned by `core/transport/src/wire/optional.rs :: only_flow_is_not_a_multiple_of_four_and_it_is_last`.

## 4. Blob fragments

### 4.1 The fragment header — 16 bytes

| offset | size | field (ACE's name) | meaning |
|---:|---:|---|---|
| 0x00 | 4 | `Sequence` | Low 32 bits of the 64-bit blob id. |
| 0x04 | 4 | `Id` | High 32 bits: ordering stamp, sequence byte, ordering type, ephemeral bit — see [04-netblobs-and-queues.md](04-netblobs-and-queues.md) §2. |
| 0x08 | 2 | `Count` | Total fragments in this blob. |
| 0x0A | 2 | `Size` | **Whole fragment** size, including this 16-byte header. |
| 0x0C | 2 | `Index` | Zero-based index of this fragment. |
| 0x0E | 2 | `Queue` | Ordering queue, 1 to 11. |

Immediately followed by `Size − 16` payload bytes.

### 4.2 Sizes

| constant | value |
|---|---:|
| Fragment header | 16 |
| Maximum fragment payload | **448** |
| Maximum whole fragment | **464** |
| Maximum datagram the client will build | **484** = 20 + 464 |
| Maximum size field accepted | 65,504 |

So the client **accepts** datagrams up to 65,524 bytes and never **sends** more than 484. ACE's send
limit is likewise 464 payload bytes; its *receive* buffer is only 1,024, so a larger client packet
would be silently truncated — harmless in practice, and a good reason not to raise the send cap even
though the receiver tolerates more.

> Pinned by `core/transport/src/wire/frag.rs :: fragment_size_bounds` and
> `core/transport/src/flow.rs :: coalesce_packs_up_to_464_payload_bytes_then_starts_a_new_packet`.

### 4.3 Parsing rules

When the fragments flag is set:

1. The connection must be established, otherwise the packet is rejected.
2. Loop while at least **17** bytes remain.
3. A fragment is accepted only when its size is between 16 and 464 inclusive **and** no larger than
   the bytes remaining; otherwise the whole packet is rejected.
4. After the loop the cursor must be exactly at the end of the payload.

### 4.4 Reassembly

- On the **first** fragment stored for a blob the buffer is sized from that fragment. Arriving out of
  order — a non-final fragment first — over-allocates by up to 447 bytes, and the size is corrected
  when the final fragment arrives.
- Every subsequent fragment is accepted only if the total count, the queue and the index agree with
  what is already stored and the destination range fits. **A fragment failing any of those is dropped
  silently, without counting**, so a corrupted blob simply never completes.
- A blob completes when the number of stored fragments equals the declared count.

**The original does not detect duplicate fragments**: a retransmitted fragment counts again and can
complete a blob early, with a hole. In practice the transport's own duplicate suppression
([02-reliability-and-flow.md](02-reliability-and-flow.md) §2) prevents it, and ACE de-duplicates on
its side. **This project de-duplicates and bounds the allocation**, because the declared count is
attacker-controlled: 65,535 fragments times 448 bytes is a 29 MB allocation from one datagram. This
project refuses a blob whose declared size exceeds 8 MiB and grows the buffer only as fragments land.
That is a deliberate deviation and it is recorded as one.

> Pinned by `core/transport/src/blob.rs :: buffer_sizing_over_allocates_then_corrects`,
> `:: mismatched_fragments_are_dropped_without_counting`,
> `:: duplicate_fragment_indices_are_de_duplicated`, `:: reassembly_allocation_is_bounded`,
> `:: zero_fragment_blob_is_refused`,
> `:: fragmentize_produces_ceil_size_over_448_fragments_in_order`.

## 5. Checksum and cipher

### 5.1 The checksum function

```
u32 checksum(const u8* p, u32 n)
{
    if (p == null) return 0;
    u32 sum = n << 16;                      // the length, in the high half
    u32 whole = n & ~3u;
    for (u32 i = 0; i < whole; i += 4)
        sum += load_le32(p + i);            // wrapping
    int shift = 3;
    for (u32 i = whole; i < n; ++i)
        sum += (u32)p[i] << (8 * shift--);  // 24, then 16, then 8
    return sum;
}
```

Every addition wraps, **including the length term**. It is identical to ACE's hash function.

> Pinned by `core/client-net/tests/cpu/net/checksum_and_cipher_vectors.rs :: hash32_matches_the_published_sweep_for_every_length_0_to_599`
> and `core/transport/src/crc.rs :: length_term_wraps`, `:: tail_bytes_shift_down_from_24`.

### 5.2 The three pieces

**Header hash.** Copy the 20-byte header, replace the checksum field with the literal
**`0xBADD70DD`**, and hash all 20 bytes. An implementation that hashes the header with a zero checksum
field fails every packet.

**Payload hash.** The sum of: one hash per optional section over that section's bytes; and, for each
fragment, the hash of its 16-byte header plus the hash of its payload, separately.

**Wire value.**

```
checksum = headerHash + payloadHash                     // plaintext packet
checksum = headerHash + (payloadHash ^ cipherKey)       // encrypted packet
```

The receiver subtracts the header hash, exclusive-ORs the cipher key back out if the packet is
encrypted, and compares against the payload hash it recomputed while parsing.

> **Why ACE's different method agrees.** ACE hashes the whole optional-section block as one buffer
> rather than section by section. The two agree because the `n << 16` term is additive over a
> partition *and* every section except the flow section is a multiple of four bytes long — and the
> flow section, the only exception, always sorts last. **Any future section of non-multiple-of-four
> length that is not last would break the equivalence.**
>
> Pinned by `core/transport/src/crc.rs :: placeholder_substitution_is_load_bearing`,
> `:: per_section_equals_whole_block_only_when_aligned`.

### 5.3 Which packets are encrypted

**A packet is encrypted if and only if it carries blob fragments or at least one non-disposable
optional section.** The disposable sections are the acknowledgement, the two retransmit sections, the
disconnect, the empty section, the logon-server address, the login request, the world login request,
the connect request, the connect response, the error and the keep-alive — twelve in all. So the whole
handshake and all pure control traffic goes in the clear, and everything else is ciphered.

The receiver **enforces the equivalence**: a packet whose encrypted flag disagrees with its own
contents is dropped, and an encrypted packet with sequence 0 is dropped.

> Pinned by `core/transport/src/wire/packet.rs :: encrypted_iff_fragments_or_non_disposable_header`,
> `:: encrypted_packet_needs_a_sequence_number`.

### 5.4 The cipher

ISAAC with a 256-word state, with one modification: the 32-bit seed is loaded into all three of the
generator's accumulators *before* initialisation, and initialisation is asked not to clear them.

Construction:

1. Zero the result and memory arrays.
2. Set the three accumulators to the seed.
3. Set the eight mixing words to the golden-ratio constant and mix four times.
4. First pass: in steps of eight over the 256 entries, add the result array (all zero here) into the
   mixing words, mix, store into memory.
5. Second pass: the same, adding the memory array instead.
6. Run one round; set the draw counter to 256.

Drawing:

```
old = count; count = old - 1;
if (old == 0) { round(); count = 0xFF; return result[0xFF]; }
return result[count];
```

so the stream is `result[255], result[254], …, result[0]`, then a fresh round and `result[255]` again:
256 values per round, no repeats and no gaps. ACE's generator produces the identical order.

**One value is consumed per encrypted packet**, in send order on one side and in receive order on the
other, and only the four checksum bytes are ever ciphered.

**Retransmission and loss.** Because the key stream is positional, the receiver draws and **parks** the
key for every sequence number it skips, keyed by that sequence number, so that when the retransmission
finally arrives it is deciphered with the stored key rather than with a fresh draw. The sender keeps
the key it used for each cached packet and reuses it verbatim on the resend. ACE approximates this by
scanning up to 256 values forward and keeping a set of skipped keys; **do not adopt the scan on the
client side**, because it desynchronises on a lost retransmission.

> Pinned by `core/client-net/tests/cpu/net/checksum_and_cipher_vectors.rs :: isaac_matches_the_published_streams_for_1000_draws_across_five_seeds`,
> `:: isaac_construction_state_matches_the_client_transcription`,
> `core/transport/src/isaac.rs :: draws_walk_down_and_refill`, `:: encrypt_covers_whole_dwords_only`,
> and `core/transport/src/session.rs :: skipped_sequences_park_their_keys_and_retransmits_reuse_them`,
> `:: a_duplicate_encrypted_packet_does_not_consume_an_isaac_value`.

## 6. A worked example

Synthetic, not recorded. A server-to-client packet with seed `0xDEADBEEF`, sequence 1, carrying one
fragment on the UI queue with a 12-byte message body:

```
header hash       = 0xBBF2710B      hash of the header with the checksum field set to 0xBADD70DD
fragment hash     = 0xD041F7B5      hash(16-byte fragment header) + hash(12-byte payload)
payload hash      = 0xD041F7B5      no optional sections
cipher key        = 0x5DA22D96      first draw of the stream seeded with 0xDEADBEEF
wire checksum     = 0x49D64B2E      = header hash + (payload hash ^ key)
```

The receiver recomputes `0xBBF2710B`, subtracts it to get `0x8DE3DA23`, exclusive-ORs its own first
draw `0x5DA22D96` to get `0xD041F7B5`, and compares that with the payload hash it computed while
parsing. The first eight values of that stream are
`5DA22D96 DB3BA3B6 9FD967F9 07487047 0A8E4664 74803C1F EEFDEC2C A4E4FB92`.

> Pinned end to end by `core/client-net/tests/cpu/net/checksum_and_cipher_vectors.rs :: the_worked_packet_reproduces_end_to_end`
> and `core/transport/src/wire/packet.rs :: worked_example_parses_and_verifies`,
> `:: worked_example_round_trips_through_outpacket`.

## 7. Notes for a reimplementation

**Design opinion from here on.**

- Model a packet as `header | map<mask, bytes> | vec<fragment>` and keep the sections in a map keyed by
  mask, so wire order falls out for free. Never emit two sections with the same mask.
- Use wrapping 32-bit arithmetic everywhere in the checksum. The length term wraps too.
- The `0xBADD70DD` placeholder and the header/payload split are load-bearing.
- Drive the cipher strictly one draw per encrypted packet, **in sequence order**, and keep a map from
  skipped sequence number to parked key.
- Cap outgoing datagrams at 484 bytes even though the receiver accepts more.
- Do not trust the declared fragment count or size: bound the reassembly allocation and de-duplicate
  fragment indices, which the original does not do.

## 8. Known gaps

- Two of the per-section behaviour bits — one present only on the connect request and response, one
  only on the referral — have no reader anywhere. They are carried through unchanged and their meaning
  is unknown. Pinned as present-and-unread by
  `core/transport/src/wire/optional.rs :: the_two_unread_m_flags_bits_appear_only_where_documented`.
- Bits 3 to 7 of the flags word are neither consumed nor rejected. Whether the original server ever set
  them is unknown.
