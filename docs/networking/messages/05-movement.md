# Movement

The client is **authoritative for its own object and a pure receiver for everyone else's**. It sends
ordered game actions describing what it wants to do, and the server sends unordered object-queue
messages describing where every object actually is.

**Provenance:** from the wire format, which public readers document.

**Readers:** `dereth_protocol::movement`. Pinned by the eleven tests in
`core/protocol/src/movement.rs` — notably `position_pack_omission_bits_are_inverted`,
`reading_the_omission_bits_the_wrong_way_round_would_overrun`,
`a_movement_buffer_round_trips_with_the_one_byte_blob_relative_pad`,
`raw_motion_state_mixes_ids_and_indices` and `set_object_movement_has_no_position_pack`.

## 1. The three traps

### 1.1 The position pack's quaternion bits are inverted

| bit | meaning |
|---:|---|
| `0x01` | A velocity vector follows the origin. |
| `0x02` | A placement id follows. |
| `0x04` | Ground contact. |
| `0x08` | The quaternion's **w is omitted**. |
| `0x10` | **x omitted.** |
| `0x20` | **y omitted.** |
| `0x40` | **z omitted.** |

Bits 0 to 2 have the ordinary sense: set means present. **Bits 3 to 6 are the other way round**: a
*set* bit means the component is **absent** and takes its default.

Reading them the usual way round makes every object face north — and, because the reader then expects
four floats that are not there, it overruns. Both halves of that are pinned by their own test.

### 1.2 The motion command wire format is not uniform

- **Client to server**: full **32-bit** command ids for style, forward, sidestep and turn, but
  **16-bit indices** for queued actions.
- **Server to client**: **16-bit indices everywhere**, and no hold-key fields at all.

The index is a position in the client's command table (412 entries in the final retail client, 408 in
the 2013 one), not the low half of the id. That
happens to be the same thing for the shipped table and is **not a property of the format**, so a
rebuild should carry the index verbatim and resolve it through the table rather than truncating an
id.

The count of queued actions lives in the flags word: bits 11–15 of the client-to-server form and bits
7–11 of the server-to-client one.

### 1.3 The movement buffer's pad is blob-relative

The buffer that carries a movement state has a five-byte header — two 16-bit timestamps and an
autonomy byte — and then **aligns to 4**. The alignment origin is **the blob, not the buffer**: in a
set-object-movement message the buffer begins ten bytes into the message, so the pad after those five
bytes is **one byte and not three**.

Over a recorded corpus, every buffer consumes exactly under that rule and **none** does under the
other. This is the clearest example in the protocol of the rule that
[the alignment origin is the message, not the field](../../formats/03-serialisation-primitives.md).

The same buffer also appears embedded in a [physics descriptor](02-world-objects.md), where the
three header fields come from the descriptor's own timestamps instead of from the buffer.

## 2. Client to server

| opcode | body |
|---|---|
| `0xF61B` jump | a power-bar extent, a velocity, a full position, and four echoed sequence numbers |
| `0xF61C` move to state | a raw motion state, a position, four sequence numbers, and a trailing flag byte holding ground contact and long-jump mode |
| `0xF753` autonomous position | **44 bytes**: a full position, four sequence numbers and a contact byte |
| `0xF649` turn to | an absolute heading in degrees (a float) and a run flag (a dword) |
| `0xF7C9` non-autonomous jump | a power-bar extent, aligned to 4 |
| `0xF61E` do movement command | a full 32-bit motion command id, a speed, and a hold key |
| `0xF661` stop movement command | a full 32-bit motion command id and a hold key |
| `0xF752` autonomy level | a level dword, aligned to 4 |

**The four echoed sequence numbers** — instance, server control, teleport and forced position — are
how the server tells whether the client had already seen a given correction when it produced the
move. A client that sends zeros loses every correction race.

A field travels only when it **differs from its default**, which is why a recorded session can be
misleading: in a run-locked capture the flag combination that walking produces never appears at all.
Presence is decided by the writer's rules, not by what a capture happens to contain.

## 3. Server to client

| opcode | body |
|---|---|
| `0xF748` position event | an id and a position pack |
| `0xF619` position and movement event | an id, a position pack, and then a movement buffer to the end of the message |
| `0xF74C` set object movement | an id, a `u16` instance sequence, and a **movement buffer** from offset 10 — and **no position pack** |
| `0xF74E` vector update | an id, a velocity, an angular velocity, and a timestamp pack |

The movement buffer's body is selected by a discriminant. **Only the default arm carries an
interpreted motion state**; the other four carry movement-parameter payloads, which come in **three
size variants**. Two bits of the combined header word say that a target object id follows.

**An autonomous movement event about the player himself is ignored** by the client: it is the server
echoing what he just sent. A server that expects its echo to move the player will find that it does
not.

## 4. Where reimplementations differ

| topic | the client | common reimplementations |
|---|---|---|
| The quaternion bits | Set means **omitted**. | The natural reading is backwards, and the result is an overrun rather than a wrong rotation. |
| Command ids versus indices | Ids one way, indices the other. | Frequently uniform, which works until a table without the coincidence. |
| The movement buffer's pad | Blob-relative: one byte after `0xF74C`'s header (three after `0xF619`'s, whose position pack keeps the buffer dword-aligned). | Buffer-relative: three bytes. Every `0xF74C` buffer then mis-parses. |
| The echoed sequence numbers | Meaningful. | Sometimes zeroed. |
| The player's own autonomous events | Ignored. | Sometimes applied, which fights the local simulation. |
