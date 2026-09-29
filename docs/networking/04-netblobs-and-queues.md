# Message blobs, ordering queues and dispatch

**Provenance: wire format**, restated as behaviour, with the first-blob numbering and the queue
census **observed** on recorded sessions. §9 is design opinion.

## Summary

Above the packet layer the unit of work is a **blob**: one complete application message, a flat byte
buffer whose first dword is the opcode.

On the way out, a blob is split into 448-byte fragments and scattered across packets. On the way in,
the fragments are collected per connection and, when a blob is complete, appended to one of **twelve
numbered queues** chosen by the fragment header's queue field. Each queue is drained by a different
subsystem later in the same frame.

Every blob also carries a **64-bit blob id** encoding an ordering type, a 16-bit ordering stamp, a
stream byte and an "ephemeral" flag. That is how superseded updates are discarded and how per-object
game events are kept in order.

## 1. A blob

| Field | Meaning |
|---|---|
| id | The 64-bit blob id of §2. While a blob is being reassembled its id is temporarily replaced by its *stream key*, and the original is restored when the blob completes. |
| state | Frozen, sending, fragmented, receiving, received. |
| buffer and length | The whole message, opcode first. |
| fragment count, fragments stored | Reassembly bookkeeping. |
| sender | The connection the blob arrived from. |
| queue | The ordering queue. |
| priority | Send priority. **Lower is sent first.** |

The wire form of a blob is just its buffer, cut into fragments. The transport library's own
serialise/deserialise pair for the blob *object* is part of the shared server code and is not used on
the wire by the client at all — ignore it.

### 1.1 The length is the sender's length, and senders pad to four

A blob's length is whatever the sender allocated, and **both ends round a message body up to a multiple
of four** before sending it. The client's own senders write each field with 4-byte alignment, and ACE's
message constructors end with an explicit alignment call — one of them reserves 12 bytes for a message
whose content is 4 + 2.

**So a receiver that requires its cursor to land exactly on the blob's length will reject every message
whose natural length is not a multiple of four.** The original never notices, because each handler
wraps the blob in an archive and stops reading when it has what it wants.

A rebuild that proves its decoders by exhaustion — which is the right discipline — has to allow the
sender's padding **explicitly**: at most three trailing bytes, and only if they are zero. This project
has one reader rule for that, and the message that found it was an object deletion in a recorded
session.

> Pinned by `core/client-net/tests/cpu/combat/attack_notification_width.rs ::
> read_body_padded_accepts_every_recorded_body_now_the_codec_reads_the_high_dword`.

## 2. The 64-bit blob id

Split across the fragment header's two dwords: the low dword is the `Sequence` field, the high dword is
the `Id` field.

| bits | width | name | meaning |
|---|---:|---|---|
| 0–31 | 32 | sequence counter | A monotonically increasing per-connection counter. |
| 32–47 | 16 | **ordering stamp** | A wrapping version number within one stream. |
| 48–55 | 8 | stream byte | Part of the stream identity. Always 0 in blobs the client generates. |
| 56–60 | 5 | **ordering type** | Selects how the receiver orders this blob. |
| 61–62 | 2 | — | One of these is set on the client's login-server-bound blobs; nothing reads it. |
| 63 | 1 | **ephemeral flag** | Marks a stream in which only the newest blob matters. |

The two operations that matter:

- **The stream key** is everything except the ordering stamp, the ordering type and the ephemeral bit.
  Two blobs with the same stream key are two versions of the same thing.
- **The stamp comparison is wrapping over 16 bits**, so a difference of more than `0x7FFF` flips the
  sense. **Never compare blob ids numerically.**

> Pinned by `core/transport/src/blob.rs :: net_blob_id_accessors`, `:: ordering_stamp_comparison_wraps`,
> `:: ordering_stamp_comparison_is_asymmetric_at_exactly_half_a_period`,
> `:: make_net_blob_id_normalises`, `:: non_ephemeral_sequence_counter`.

### 2.1 The first blob the client sends carries sequence 1, not 0

This is short, exact, and it is the difference between a login that works and a login that hangs.

Across recorded sessions, six of seven open with sequence 1 and continue with 2. The seventh sends no
client blob at all — one server blob and nothing back — so it has no first blob rather than a different
one; the claim is unfalsified by it rather than confirmed by it. Both the id counter and the stamp
counter have already been advanced once by the time the first blob is transmitted.

**Why it matters.** ACE's ordered-fragment handler starts its expected sequence at 0 and dispatches a
fragment only when its sequence is *expected + 1*, parking anything else and draining that park by
looking for the same *+ 1*. **A client whose first blob is numbered 0 therefore has its first message
accepted at the packet layer, acknowledged, and never dispatched.** The symptom is a login that reaches
the enter-world request and never sees the server-ready reply.

> Pinned by `core/client-net/tests/cpu/net/all_zero_blob.rs` and
> `core/transport/src/blob.rs :: non_ephemeral_sequence_counter`.

### 2.2 What each direction actually puts there

**Client to server.** Ordering type `0x03` for the world-bound queues and `0x23` for the login-bound
queues (4, 5 and 8); the stamp is a plain 16-bit counter incremented per blob; the sequence comes from
the shared counter. **The client never sets the ephemeral bit.**

**Server to client, as ACE sends it.** The sequence is a per-session fragment counter and the high dword
is the constant `0x80000000` — ephemeral bit set, ordering type 0, stamp 0. That means every blob is
its own ephemeral stream, which makes the supersession logic of §3.2 a no-op and makes the UI queue's
ordering-type test fall through to the per-object path of §6.1. Whether the original servers used real
ordering types is not something any available recording can answer.

> Pinned by `core/client-net/src/queues.rs :: ordering_type_follows_the_login_split` and
> `core/transport/src/indicator.rs :: aces_per_blob_streams_make_supersession_inert`.

## 3. Reassembly

One reassembler per connection. It owns a table of partially received blobs, keyed by the **stream
key** for ephemeral blobs and by the whole id otherwise; a table of "newest seen" records per ephemeral
stream; and a flush timer.

For each fragment of each arriving packet: drop it if it is obsolete for its stream (§3.2), otherwise
accept it (§3.1).

### 3.1 Accepting a fragment

```
key = ephemeral ? stream_key(blob_id) : blob_id
blob = waiting[key]
if blob exists:
    if blob's real id == this blob id        -> use it, another fragment of the same blob
    else if the arriving blob is the newer   -> return, dropping the fragment
    else                                     -> discard the stored blob and start again
if no blob:
    create one, remember its real id, and file it under the stream key
add the fragment
if every fragment has arrived -> restore the real id and hand it to its queue
```

**The intent is clear and the shipped behaviour is the opposite of it.** The intent is that a
half-received multi-fragment blob should be thrown away as soon as a *newer* blob for the same
ephemeral stream starts arriving — a position update still in flight abandoned when a fresher one
appears. That is the whole point of the ephemeral mechanism.

What actually happens is that the branch which returns is the one taken when **the arriving blob is
newer**, so it is the new fragment that is dropped. Supersession runs only when the *stored* blob is
newer, which §3.2 has already made unreachable by dropping every fragment older than the stream's
newest.

**The consequence:** an ephemeral stream that half-receives a multi-fragment blob and then sees a newer
one is dead for the rest of the connection — the new blob's fragments are dropped here and the old
blob's remaining fragments are dropped as obsolete there. It is unreachable against ACE, which gives
every blob its own ephemeral stream; whether an original server ever exercised it is unknown.

This project reproduces the shipped behaviour rather than the intent, and pins both halves so that
nobody "fixes" it by accident.

> Pinned by `core/transport/src/indicator.rs ::
> a_newer_ephemeral_blob_does_not_supersede_a_half_received_one`,
> `:: a_stored_newer_blob_is_superseded_by_an_older_arrival`,
> `:: a_single_fragment_blob_completes_immediately`,
> `:: multi_fragment_blob_completes_with_its_original_id_restored`,
> `:: non_ephemeral_blobs_do_not_supersede`.

### 3.2 Dropping obsolete ephemeral fragments

```
if not ephemeral                          -> accept
key = stream_key(blob_id)
record = arrived[key]
if no record                              -> remember this id, accept
if this id != the remembered id:
    if the remembered id is newer         -> obsolete, drop
    remember this id instead
accept
```

Each record times out after **5.0 seconds**, and the flush runs at most once every **5.0 seconds**, so a
quiet stream costs nothing.

> Pinned by `core/transport/src/indicator.rs :: frag_is_obsolete_ephemeral_drops_an_older_stamp`,
> `:: ephemeral_supersession_wraps_at_0xffff`,
> `:: eph_info_flush_runs_at_most_once_per_five_seconds`, `:: a_live_stream_survives_the_flush`.

### 3.3 The sender's mirror

The send-side cache records, for every ephemeral fragment it caches, the newest blob id per stream, and
removes the entry on flush only when the stored id still matches. **The client never sends ephemeral
blobs**, so this path is dead in the client and matters only to the shared server build.

## 4. The twelve queues

Twelve slots, all empty until a subsystem registers one. On arrival:

```
q = blob.queue
if q == 0 or q >= 12   -> drop
if no queue registered -> drop
else append
```

so the valid range is **1 to 11**, and an unregistered queue **silently discards**.

The client registers exactly five, in this order: **2, 10, 9, 4, 5**.

| id | Name | Registered? | Drained by |
|---:|---|---|---|
| 0 | invalid | no | rejected outright |
| 1 | event | **no** | inbound blobs are discarded |
| 2 | control | **yes** | *nothing.* The list is created and registered and nothing ever drains it. |
| 3 | weenie | no | client to server only |
| 4 | login | yes | the client's own login-event drain, §7 |
| 5 | database | yes | the dat cache and the patching exchange |
| 6 | secure control | no | inbound blobs are discarded |
| 7 | secure weenie | no | client to server only (autonomous position) |
| 8 | secure login | no | client to server only |
| 9 | UI | yes | the UI queue manager, §6.1 |
| 10 | world | yes | the world container, §6.2 |
| 11 | observer | no | inbound blobs are discarded |

**Queue 2 is a slow leak by construction**: it is registered, so blobs on it are retained rather than
dropped, and nothing consumes them. If a server ever sends on it, the list grows without bound. A
rebuild should keep the "unknown queue means drop" rule — servers do send on queues the client does
not register — and should decide deliberately what to do with queue 2.

**Drain order within a frame:** receive and reassemble, then the login queue, then the send pass, then
the dat queue, then the UI element tick, then the world queue, then the UI queue. Blobs enqueued during
the receive phase are all consumed in the same frame, except queue 2's.

> Pinned by `core/client-net/src/queues.rs :: all_twelve_queue_ids_exist_and_number_correctly`,
> `:: queue_zero_and_twelve_and_above_are_dropped_not_errors`,
> `:: an_unregistered_queue_silently_discards`, `:: queue_two_is_registered_and_never_drained`,
> `:: blobs_come_out_in_frame_drain_order`, `:: one_queue_is_fifo`, and
> `core/client-net/tests/cpu/net/session_queues.rs :: the_queue_map_is_honoured`,
> `:: blobs_on_undrained_queues_are_discarded`.

## 5. The send side

There is exactly **one** producer of outbound blobs in the whole client. Four entry points, each with a
fixed queue:

| Entry point | Queue | Typical use |
|---|---:|---|
| to control | **2** | Out-of-band commands: force an object description, ask for the server version, friends commands, restore a character. |
| to weenie | **3** | All in-world player actions. |
| to login | **4** | Enter-world request, enter world, log off, character delete, character-creation result. |
| to database | **5** | Patching replies — **not** the message the community catalogue names as the end of patching, which the client never sends. |

The message body is always **a 32-bit opcode followed by the archive-packed arguments**. Small
messages build the buffer inline; larger ones serialise into a scratch archive first.

### 5.1 Choosing the recipient, and assigning the id

```
q = blob.queue
to_login = (q == 4 or q == 5 or q == 8)
recipient = to_login ? the login server : the current world server
if the blob has no id yet:
    ordering type = to_login ? 0x23 : 0x03     (in the high dword)
    id = make(ordering type, stamp, next sequence)
    ++stamp
send with priority 5
```

**Reproduce the queue-to-recipient mapping exactly.** Against a single-process server such as ACE both
ids are the same, so a rebuild that hard-codes one recipient looks correct — and breaks the moment it
meets a split deployment.

The send refuses outright if there is no world server yet, and the queueing step refuses if the
connection is already tearing down.

> Pinned by `core/client-net/src/queues.rs :: queues_4_5_and_8_are_login_bound_and_nothing_else_is`,
> `:: the_only_send_priority_is_five`, and
> `core/client-net/src/net.rs :: send_routes_login_queues_to_the_login_recipient`,
> `:: send_stamps_the_documented_ordering_types`, `:: send_advances_the_stamp_and_the_sequence_counter`.

### 5.2 The game-action counter

A separate counter travels **inside** the bodies of ordered game actions; it is not a transport field.
It is reset to zero when the client leaves the world, and **decremented again if a send fails**, so it
counts actions that actually left.

> Pinned by `core/client-net/tests/cpu/net/session_queues.rs :: the_action_counter_is_global_and_rolls_back`, and
> `core/client-net/tests/cpu/login/second_entry.rs :: the_second_sessions_first_action_carries_stamp_one`,
> `:: an_entry_that_follows_no_log_off_still_resets_the_counter`.

## 6. Ordering on the receive side

### 6.1 The UI queue

```
while a blob can be popped:
    if the ordering type is the one the test looks for  -> the unordered path
    else                                                -> the ordered path
```

The unordered path is **dead in the shipped client**: nothing the client produces and nothing ACE
produces ever carries that ordering type, so the test never fires. When it would fire, it additionally
refuses to dispatch until the login-critical ordered events have arrived.

The ordered path:

```
peek the first dword
if it is not the ordered-event opcode  -> dispatch the whole blob
else consume 12 bytes {opcode, object id, stamp}
     if the object id is zero          -> dispatch the remainder
     else look the object up
          if it does not exist yet     -> park the blob on that future object
          else hand it to that object's ordering holder
```

Each object's holder is the blocking kind, so per-object game events are held until their stamps are
contiguous, with the 19-entry/300-second deadlock breaker of
[02-reliability-and-flow.md](02-reliability-and-flow.md) §5.

> Pinned by `core/client-net/tests/cpu/net/session_queues.rs :: an_ordered_event_for_an_unknown_object_waits_for_it`.

### 6.2 The world queue

```
while a blob can be popped:
    if the world is not ready to dispatch -> park it
    else                                  -> dispatch it
```

Parked blobs are drained later, and there is a second, per-object parked list for blobs that arrive
before their object exists.

> Pinned by `core/client-net/tests/cpu/net/session_queues.rs ::
> a_world_view_blob_parked_on_an_unknown_object_replays_through_the_world_view_dispatcher`
> and `core/client-net/tests/cpu/net/all_zero_blob.rs :: the_world_view_dispatcher_refuses_it_exactly_as_retail_does`.

### 6.3 The login and database queues

No ordering at all. The consumers dequeue and dispatch in arrival order.

## 7. The login queue is a private channel

The login-event drain runs once per frame and does exactly one thing:

```
while a blob can be popped:
    if it is at least 4 bytes and its opcode is the chat-service opcode:
        hand it to the chat client
    release it
```

**Every other opcode arriving on queue 4 is dropped.** So the login queue is, in the shipped client, a
private channel for the external chat service plus a sink for anything else a server chooses to send
there.

Note the asymmetry, because it catches people out: the client **sends** the whole login flow — enter
world, character delete, character-creation result, log off — on queue 4, and **receives** the
corresponding replies on queue 9.

> Pinned by `core/client-net/tests/cpu/net/session_queues.rs :: the_login_queue_only_yields_turbine_chat`
> and `core/client-net/tests/cpu/login/session_state.rs :: log_off_emits_f653_on_the_logon_queue`.

## 8. How the pieces interact

- The connection table owns the queues; the per-connection reassembler fills them; five subsystems
  drain them.
- Object maintenance provides the per-object ordering used by queue 9.
- The dat cache is both a queue-5 consumer and a queue-5 producer.
- The world container is the queue-10 consumer and owns the physics world; a blob naming an object that
  does not exist yet is parked on that object's future list.

## 9. Notes for a reimplementation

**Design opinion.**

- Model the queue id as an enum with all twelve values and route by it. Keep "unknown queue means drop"
  rather than panicking.
- Keep the blob id as one 64-bit value with accessors. The stream key and the wrapping stamp comparison
  are the two operations that matter, and **numeric comparison of ids is always wrong**.
- Bound the partial-blob and ephemeral-record tables. The original relies on the 5-second flush and on
  the server behaving.
- Reproduce the queue-to-recipient mapping exactly: 4, 5 and 8 to the login server, everything else to
  the world server.
- Priority is uniformly 5 today. Keep the parameter so a rebuild can prioritise movement over chat
  without changing the wire format.
- The supersession rule as *intended* — throw away a partially received blob when a newer one for the
  same ephemeral stream starts — is a real behaviour, not an optimisation; without it a lossy link
  accumulates stale half-blobs. The shipped rule is the inverted one described in §3.1. Decide which
  you are implementing, and say so.

## 10. Known gaps

- Nothing drains queue 2, yet the client registers it. Either the original server never sent on it, or
  a consumer was removed before this build.
- One of the two unused ordering-type bits distinguishes login-bound from world-bound blobs on the
  send side and has no reader anywhere.
- The ordering type the UI queue's unordered path tests for is produced by neither the client nor ACE.
  Which messages used it is unknown.
