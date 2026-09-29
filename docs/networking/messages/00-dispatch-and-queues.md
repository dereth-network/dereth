# Dispatch, queues and the master opcode table

Above the transport — fragments reassembled into whole messages, which the
[networking pages](../01-packet-format.md) cover — the client has **no single message loop**. Every reassembled message
carries a **queue id chosen by the server**, and each subsystem drains its own queue on its own tick.

There are exactly two large opcode switches in the client, one for the interface and game-event
queue and one for the render and physics queue, plus two small ones for the data-patch queue and the
login queue.

Outbound traffic is simpler still: every client-to-server message in the game is built by one sender
function and handed to a send helper that wraps it in a message stamped with the right queue id.

**Provenance:** from the wire format, which public readers document.

**Readers:** `dereth_protocol::opcodes` (the master table, section 7), `dereth_protocol::order` (the two wrappers),
`dereth_protocol::wrap` (the sequence comparisons), `dereth_protocol::events` and `dereth_protocol::actions` (the
two sub-type spaces). Pinned by `the_table_is_sorted_and_complete` and `an_unknown_opcode_has_no_row`
in `core/protocol/src/opcodes.rs`, the six tests in `core/protocol/src/order.rs`,
the two in `core/protocol/src/wrap.rs`, and
`read_checked_enforces_the_double_opcode_check` in `core/protocol/src/lib.rs`.

## 1. The twelve queues

The queue id is part of the message's envelope, not of its body. The client allocates twelve slots
and registers **five** of them as receive queues:

| id | name | who drains it | what it dispatches |
|---:|---|---|---|
| 0 | invalid | — | Rejected outright. |
| 1 | event | not registered | Silently discarded. |
| 2 | control | registered, **never drained** | Outbound only; see below. |
| 3 | weenie | not registered | Outbound only: every game action. |
| 4 | logon | the client's own loop | **Only checks for one opcode.** |
| 5 | database | the data cache | The data-patch messages. |
| 6 | secure control | not registered | Silently discarded. |
| 7 | secure weenie | not registered | Outbound only: autonomous position. |
| 8 | secure logon | not registered | Silently discarded. |
| 9 | UI | the interface queue manager | The large game-event switch. |
| 10 | smart box | the render/physics side | The sixteen-case object switch. |
| 11 | observer | not registered | Silently discarded. |

A message whose queue is out of range, or whose queue is registered as nothing, is dropped with a log
line. So **a message on queue 1, 3, 6, 7, 8 or 11 is discarded by this client**, whatever its opcode.

**Queue 2 is outbound only.** It is registered as a receive queue and nothing ever reads it: every
message the client sends on it is client-to-server, and the server answers on the UI queue. The queue
exists because the shared code registers it unconditionally, not because anything arrives on it.

**Queue 4 is a peephole.** The client drains it but looks only for the external-chat message; when it
sees one it hands the whole body to the chat library. **Every other message that arrives on queue 4
is discarded.** That is why the login and character-management replies have to be sent on the **UI**
queue even though the client sends the corresponding requests on queue 4 — a detail that is easy to
get backwards when writing a server, and which produces a client that sits forever at the character
list.

Outbound, the four send helpers pick queues 3, 2, 4 and 5. The recipient follows from the queue:
queues 4, 5 and 8 go to the login and patch server, everything else to the world server.

## 2. The envelope id

Each message carries a 64-bit id whose fields are:

| bits | meaning |
|---|---|
| 63 | An ephemeral flag. |
| 56–60 | An ordering type. |
| the low dword plus part of the high | A sequence id. |
| 32–47 | An ordering stamp, compared with wraparound. |

Stamps are compared with a **half-range window**, never with a plain `>`: a stamp is newer when the
difference is positive within half the range. The same rule applies at 8, 16 and 32 bits everywhere
in this protocol, and naive comparisons break after 256 updates to one property.

**A trap worth stating explicitly.** The interface queue routes a message on its **ordering type**,
comparing it against a constant whose only set bit is bit 63 — which is the *ephemeral flag's* bit,
not an ordering type's. The ordering type occupies bits 56 to 60 and can never equal it, so **that
branch is dead in the shipped client and every message on the UI queue takes the ordered path.**

That matters because at least one widely used server sets the ephemeral bit on every outbound
message. A client that "fixes" the test to look at the ephemeral flag sends the whole UI queue into a
gate that waits for the player description — which arrives on that same queue — and the session
deadlocks at the character list. The structural confirmation is that the list of waiting messages is
appended to in exactly one place, inside the unreachable function, so in practice it is always empty.

## 3. The two ordered wrappers

`0xF7B0` and `0xF7B1` are **headers, not message types**: the real type is the dword that follows.
The community catalogue lists the sub-types as opcodes in their own right, and so does the table
below.

**Game event** (server to client), 12 bytes:

| offset | size | field |
|---:|---:|---|
| `0x0` | 4 | The magic `0xF7B0`. |
| `0x4` | 4 | The object the event is ordered against. **Zero means globally ordered.** |
| `0x8` | 4 | A per-object event sequence. |

**The wrapper is the sender's choice.** When the magic is absent the reader **rewinds** and the whole
payload is dispatched unchanged, so a game event may arrive bare. Measured over a recorded corpus,
about a quarter of the game-event types that appear do arrive bare. A reader that requires the
wrapper mis-parses them.

**Game action** (client to server), 8 bytes:

| offset | size | field |
|---:|---:|---|
| `0x0` | 4 | The magic `0xF7B1`. |
| `0x4` | 4 | The client's action sequence. |

Every action sender starts with this header and, **if the send fails, rolls the counter back**, so the
client's action sequence has no holes even when a send fails.

The event and action number spaces use the same small integers and are **independent**: the same
value means one thing as an event and something else as an action.

## 4. Ordering and the stall breaker

Ordered game events go through a per-object window with these rules:

1. **The first entry on a stream skips only the staleness test.** The high-water mark starts at
   **zero**, so a stream's first stamp delivers immediately only if it is 0 or 1. Anything higher is
   *blocked*. A server whose first game event is numbered 100 stalls every ordered stream — for five
   minutes, until rule 5 fires. Servers that start at 1 never see this.
2. A stamp at or below the high-water mark is **stale and dropped**. This is a plain unsigned compare,
   not the wrap-aware one.
3. A stamp equal to the mark or one above it is **delivered**, and the mark advances.
4. Anything else is **held** in a sorted list, and the time the list became non-empty is recorded.
5. **The stall breaker**: when more than **19** stamps are held *and* more than **300 seconds** have
   passed, the client fabricates the missing stamp and continues. That is the only way it ever skips
   a missing ordered event. The clock is started only when the held list goes from empty to
   non-empty, but the client also **resets it** whenever it checks the list and finds the head still
   not ready, so a client that polls every frame keeps pushing the deadline out.

When an event is ordered against an object the client has not seen yet, the message is parked on a
placeholder for that object and replayed when the real object arrives.

## 5. Per-property sequencing

Quality updates are **not** ordered by the wrapper. They carry a **single byte** per (object, quality
type, property id), and the key is `property_id | (stat_type << 16)`:

| stat type | value |
|---|---:|
| int | 1 |
| float | 2 |
| position | 3 |
| skill | 4 |
| string | 5 |
| data id | 6 |
| instance id | 7 |
| attribute | 8 |
| vital | 9 |
| bool | 13 |
| int64 | 14 |

An absent key is inserted and accepted. A present key accepts only a byte that is not older by the
8-bit wrap rule. A separate byte tracks house restrictions.

## 6. The double opcode check

Every dispatched handler **re-checks the opcode** before reading any field, and returns silently if
it does not match the arm it was routed to. The check exists because the same handlers are reachable
from replayed messages. It is cheap and worth keeping: it converts a routing bug into a dropped
message instead of a mis-parse.

On the object queue a handler returns one of five statuses: undefined, processed, old instance (the
object was re-created, so drop it), error (bad opcode or a short buffer), or queued (the object is
not known yet). A body shorter than four bytes is an error.

## 7. The master opcode table

The table itself lives in code: `core/protocol/src/opcodes.rs` (`dereth-protocol`). Its `OPCODES`
array, which gives every opcode's value, name, direction and send and receive queues, is the
authority and is maintained by hand. It is not repeated here, so the two cannot drift apart. It holds 352 opcodes, taken from this client's own table — which was built from
the client's switches and senders rather than from a community catalogue, and which the two sources
disagree about in a handful of places noted on the family pages.

How to read it:

- The send and receive queues differ for the handful of opcodes that travel both ways, because the
  login server's replies come back on the world server's UI queue.
- Opcodes below `0x0400` are game-event or game-action sub-types; see section 3. **The queue does
  not decide which of the two a sub-type is**, despite what the convention of listing them as
  opcodes suggests — the number spaces are independent and a few sub-types appear in both.
- `0x0291` is spelled `Fellowship_ChangeFellowOpenness`; the catalogue's own spelling drops a doubled
  letter. The opcode is what matters.
- `0xF7DE` carries the payload of a third-party chat library the original client shipped with. It
  arrives on the logon queue, whose consumer discards every other opcode, and the client hands the
  whole body over without reading a field. This client ports the outgoing by-room request and the
  two recognised incoming callback forms and keeps every other form losslessly opaque. The code
  names it after that library (`Communication_TurbineChat`).
- `0x00B5` (`Writing_BookModifyPageResponse`) and `0x01C8` (`Allegiance_AllegianceUpdateDone`) are
  sent by the retail server but have no handler in the client, which drops them. The code carries
  constants for both and no table row.
- `0x0317` and `0x0318` are game events carrying one string, shown exactly as the transient string
  (`0x02EB`) and the pop-up string (`0x0004`) are. The community catalogue has no name for them, so
  they too have constants and no table row; see [08-communication.md](08-communication.md).

## 8. Where reimplementations differ

| topic | the client | common reimplementations |
|---|---|---|
| The action sequence | Kept, and rolled back on a failed send. | Servers generally ignore it; the client is the only side that keeps action ordering. |
| The event sequence | A per-object window. | Servers commonly allocate it per session and only ever order against the player's own id — so the per-object window becomes a per-session one. **Both paths are real**: the "globally ordered" and "unknown object" arms exist because original servers did order events against other objects. |
| The ephemeral bit | Never routed on. | Set on every outbound message by at least one server, which is harmless only because the routing test looks at the ordering type. |
| The first stamp on a stream | Must be 0 or 1 or it blocks. | Servers that start at 1 never notice. |
| Queue 4 replies | Discarded except for one opcode. | A server must answer login requests on the **UI** queue. |
