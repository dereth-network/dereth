# Reliability, ordering and flow

**Provenance: wire format**, restated as behaviour and cross-checked against this project's own
transport and a corpus of recorded sessions. §11 is design opinion and says so.

## Summary

The transport is a **selective-retransmission datagram protocol**. Packets that carry payload get a
32-bit sequence number and are cached by the sender. The receiver tracks the highest contiguous
sequence it has seen, asks for holes explicitly by number, and periodically sends a cumulative
acknowledgement that lets the sender free its cache.

**Reordering of packets is not done at the packet layer at all.** The packet layer guarantees delivery
and nothing else; ordering is done a level up, per blob and per ordering queue. A reimplementation
that adds a packet reorder buffer has built something the client does not have, and a reimplementation
that omits the blob-level ordering will deliver ordered game events out of order.

On top of that sits a coarse **interval clock**, one tick per 0.5 s, used for flow accounting, time
synchronisation, echo timing and the keep-alive.

**The client has no outbound rate limiting.** The two send-side pacing hooks always answer "there is
room", the command-line switch that would disable the flow queue is never read, and the flow section
and the interval machinery survive as pure telemetry for the server's benefit. If you add pacing, you
have changed behaviour; keep the priority bypass so that retransmit requests, server switches and
referrals still get out.

> Pinned by `core/transport/src/flow.rs :: there_is_no_outbound_flow_control`.

## 1. State per connection

The client keeps up to 256 connections, indexed by the id the **server** chose. In practice at most two
are live: the login server and the current world server.

Per connection, on the receive side:

| State | Meaning |
|---|---|
| assigned id | Non-zero means the slot is in use. |
| peer's id for us | What goes into outgoing transport headers. |
| iteration | Connection generation, from the connect request. |
| highest received | Highest sequence number accepted so far. |
| parked keys | A map from a sequence number we have asked for to **the cipher key that was drawn for it**. |
| nak state | Whether the parked-key map is empty, which decides whether the connection sends acknowledgements or retransmit requests. |
| connection state | See [03-connection-state-machine.md](03-connection-state-machine.md). |
| last data time | Drives the 140-second timeout. |
| current remote interval and bytes received | For the flow section. |
| the two cipher streams | One per direction. |
| round-trip latency | Last echo measurement. |

On the send side: the flow queue, the reassembly bookkeeping, a pending cumulative acknowledgement to
apply, and whether the connection is queued for service.

## 2. Sequence numbers

- One 32-bit space **per direction per connection**. The sender's counter starts at **1** and is
  incremented before use, so the first sequenced packet carries **2**; the receiver correspondingly
  starts with a highest-received value of 1.
- A packet with no fragments and only disposable sections — a pure acknowledgement, retransmit
  request or control packet — **does not advance the counter**. It is stamped with the *current*
  value and sent in the clear.
- Anything else increments the counter (forced back to 1 if it wraps to 0), takes that number, is
  encrypted, and is cached for retransmission.
- Sequence 0 means "no sequence". It is used for every handshake packet, and it is the only value the
  receiver will accept from a connection that is not yet established.

> Pinned by `core/transport/src/flow.rs :: highest_id_sent_starts_at_one_and_is_not_advanced_by_a_control_packet`,
> `:: highest_id_sent_never_becomes_zero`.

### 2.1 Receive-side acceptance, in order

1. Verify the header.
2. Look up the connection by the header's id.
3. If the sequence is 0, reject the packet if the encrypted flag is set; otherwise skip to step 4.
   Otherwise run the sequence check of §2.2 and reject on failure.
4. Parse and verify the checksum.
5. If the peer's interval changed, emit a flow section for the interval that just ended.
6. Add this datagram's size plus 20 to the bytes-received counter.
7. Process optional sections.
8. Stamp the last-data time.
9. Hand any fragments to reassembly.

**If step 4 fails and the packet was sequenced and encrypted, its sequence number is added to the
parked-key set** — a corrupt packet is treated as lost and re-requested. The key parked for it is the
one it was just checked against, never a fresh draw: a fresh draw would shift every later key by one.

Header verification rejects, in this order:

| Test | Result |
|---|---|
| id is `0x100` or more | reject |
| slot unused and sequence is non-zero | reject |
| slot unused and sequence is zero | **accept** — this is the handshake |
| source **address** differs from the stored one | reject. **The port is not checked**, and that is deliberate: see [03-connection-state-machine.md](03-connection-state-machine.md) §3. |
| iteration older than stored, by wrapping 16-bit compare | reject |
| iteration newer than stored and no connect-request flag | reject |
| size field is `0xFFE1` or more | reject |

> Pinned by `core/transport/src/session.rs :: verify_header_rejects_in_order_and_ignores_the_source_port`.

### 2.2 The sequence check, and duplicate suppression

```
if no connection, or a non-zero sequence on an unestablished slot   -> reject
if the sanity check fails                                           -> reject
key = none
if encrypted and the sequence is NOT newer than the highest received:
        key = take the parked key for this sequence
        if there was none -> reject            // duplicate, or never requested
if the sequence IS newer than the highest received:
        request the gap, advance the highest
if encrypted:
        decipher the checksum with `key`, or with a fresh draw
```

The comparison is the wrapping 32-bit one: a difference of more than `0x7FFFFFFF` flips the sense.

**That is the duplicate suppression.** An encrypted packet whose sequence is not newer and is not in
the parked-key set is dropped *without consuming a cipher value* — which is the property that keeps
the two ends' key streams aligned. Get this wrong and every subsequent packet fails its checksum.

The sanity check rejects when the sequence is more than **32,767** ahead of the highest received, or
when the parked-key set already holds more than **40,000** entries.

> Pinned by `core/transport/src/session.rs :: lhs_newer_wraps_and_is_asymmetric_at_half_a_period`,
> `:: a_duplicate_encrypted_packet_does_not_consume_an_isaac_value`, `:: seq_id_sanity_check_bounds`.

### 2.3 Creating retransmit requests

```
target = sequence + (encrypted ? 0 : 1)
for (i = highest_received + 1; i != target; ++i)
    if (i != 0) park a key for i and remember we want it
highest_received = sequence
```

Parking the key is the whole point: the receiver **draws the cipher value for the sequence number it
just skipped** and stores it, so that the retransmission, whenever it arrives, deciphers correctly.
Asking twice for the same id is a no-op.

The `+1` on the unencrypted branch asks for the packet's own sequence number, and it is reachable on
a lossy link. Both ends stamp an unencrypted acknowledgement or retransmit request with the *current*,
already-used sequence value (§2). When the encrypted packet that used that value is lost, the
acknowledgement that follows it arrives as the newest sequence; its number is the lost packet's, and
the `+1` is what parks a key for it and asks for it rather than skipping it.

> Pinned by `core/transport/src/session.rs :: add_nakked_is_idempotent`,
> `:: an_unencrypted_newest_sequence_naks_its_own_number_too`,
> `core/transport/src/flow.rs :: highest_id_sent_starts_at_one_and_is_not_advanced_by_a_control_packet`.

## 3. Cumulative acknowledgement

**Sending.** Whenever nothing is missing, and at most once every **2.0 seconds**, the connection
enqueues a 4-byte acknowledgement carrying its highest contiguous sequence. The acknowledgement and the
retransmit request of §4 share one "last sent" timestamp, so the two cadences interfere with each
other.

**Receiving.** A zero is ignored; otherwise the value is stored if it is newer, and applied on the
**next** service tick, after that tick's resends have gone out: every cached packet whose sequence is
older than it is dropped, along with the pending resends below it. The packet whose sequence *equals*
the value survives. The deferral is what lets one datagram carry both an acknowledgement and a
retransmit request for something below it and still get the resend.

Because the acknowledgement section is disposable, a packet whose only content is an acknowledgement
is unencrypted and does not advance the sequence counter.

ACE matches the 2-second cadence. It also bounds its own cached packets by age at about 120 seconds,
pruned every 5 seconds, which the client does not.

> Pinned by `core/transport/src/flow.rs :: a_cumulative_ack_retires_the_cache_and_the_pending_resends_below_it`
> and `core/transport/src/session.rs :: flush_evicts_strictly_below_the_ack_and_keeps_the_named_sequence`.

## 4. Retransmit requests and rejections

**Sending a request.** Whenever something *is* missing, and at most once every **0.6 seconds**, the
connection walks its parked-key set in order, takes up to **114** ids, and sends them as one section.
There is no per-sequence retry counter and **no give-up**: the client will keep asking forever, and
only the 140-second no-data timeout ends the connection.

**Receiving a request.** For each requested id: if a cached packet exists, queue it for resend; if not,
queue the id into a **rejection** instead — it is unrecoverable, so tell the peer to stop asking. Then:

- the requested ids are merged into the ascending resend list, and an id already queued that is older
  than an id the peer is still asking for, and is not itself asked for again, is dropped — the peer
  has it. A queued `[5, 10]` meeting a request for `[3, 12]` becomes `[3, 12]`, not `[3, 5, 10, 12]`;
- an id already waiting in the rejection list is not queued there a second time;
- the cumulative acknowledgement is advanced to the first requested id, because **a retransmit request
  implicitly acknowledges everything below its first id**;
- the connection is serviced on the next tick rather than waiting for its timer.

**Resending.** For each queued id, the cached packet is found, its time-sensitive sections are
refreshed, its checksum is recomputed if that invalidated it, and it is re-ciphered with **the key
stored for that packet**, so the resend deciphers under the key the receiver parked for that sequence.
The header is rebuilt with the retransmission flag, the encrypted flag, the fragments flag if
applicable, and the masks of whatever sections survived caching — with the **original** sequence but
the **current** interval and iteration. If the cached packet became empty because all its sections were
disposable and were stripped when it was cached, it is not resent; a rejection is sent instead.

**Rejections** carry up to **114** ids. The receiver forgets each named id, which ends its retry loop.
It does **not** advance its highest-received counter, so the parked keys for those sequences are simply
forgotten — the stream stays aligned because those keys were already drawn.

> Pinned by `core/client-net/tests/cpu/net/retransmit.rs`, which covers the whole family:
> `:: no_loss_produces_no_naks_and_no_resends`, `:: a_single_dropped_datagram_is_naked_and_resent`,
> `:: a_run_of_drops_is_naked_in_one_burst_and_wholly_recovered`,
> `:: a_late_arrival_and_its_retransmission_do_not_desynchronise_the_key_stream`,
> `:: a_nak_is_a_cumulative_ack_of_everything_below_its_first_id`,
> `:: a_reject_retransmit_ends_the_peers_retry_loop`,
> `:: a_reject_retransmit_advances_nothing_and_keeps_the_key_stream_aligned`,
> `:: a_reject_retransmit_keeps_the_link_alive_and_resets_its_140_second_clock`;
> plus `core/transport/src/flow.rs :: a_resend_carries_the_current_interval_not_the_cached_one`,
> `:: reject_retransmit_caps_at_114`, `:: a_nak_resends_what_is_cached_and_rejects_what_is_not`,
> `:: a_nak_implicitly_acknowledges_everything_below_its_last_id`,
> `:: an_id_already_awaiting_rejection_is_not_queued_twice`.

### 4.1 The retransmit cache

Every packet that gets a sequence number is appended to a plain list, searched linearly. When it is
cached:

1. **every disposable section is stripped** and the packet's checksum is invalidated, so that a resend
   does not carry a stale acknowledgement;
2. for each fragment on an ephemeral stream, the newest blob id for that stream is recorded, so a
   newer update supersedes an older one.

Eviction is only by cumulative acknowledgement, by retransmit request, or by teardown. **The original
has no size or age cap on this cache.** This project bounds it by age (120 seconds) and by count
(4,096 packets) and keeps the eviction semantics, which is a deliberate deviation; a request for a
packet that has aged out is answered with a rejection.

> Pinned by `core/transport/src/session.rs :: cache_strips_disposable_headers_and_retransmit_reuses_the_stored_key`,
> `:: the_cache_is_bounded_by_age`.

## 5. Ordering

The packet layer does not reorder. Everything that needs ordering is ordered by the blob layer:

| Level | Mechanism |
|---|---|
| fragments of one blob | The fragment index addresses a pre-allocated buffer; the blob completes when the count is reached. |
| blobs of one ephemeral stream | A fragment whose blob is older than the newest already seen for the same stream is dropped, by wrapping 16-bit stamp comparison. |
| ordered blobs on the UI queue | A 12-byte prefix names an object and a stamp; a per-object holder keeps blobs until the stamps are contiguous. |
| queue selection | The fragment header's queue field. |

The per-object holder has two modes — **block**, which holds out-of-order entries, and **latest only**,
which keeps the newest and discards anything older — and these rules:

- An entry is delivered immediately when its stamp equals the highest seen or the highest plus one;
  otherwise it goes on a sorted blocked list.
- **The highest starts at 0, and the "first entry" flag suppresses only the staleness test, not the
  contiguity test.** So a stream whose first stamp is neither 0 nor 1 blocks from the outset. This is
  the single most common way a reimplementation's login hangs.
- **Deadlock breaker:** if more than **19** stamps are blocked **and** they have been blocked for more
  than **300 seconds**, the missing stamp is force-inserted as a null entry and the queue drains.

> Pinned by `core/client-net/src/sequence_gate.rs ::
> blocking_delivers_highest_and_highest_plus_one_and_blocks_the_rest`,
> `:: the_very_first_entry_is_not_special_cased_into_delivery`,
> `:: the_deadlock_breaker_fires_at_more_than_19_stamps_held_more_than_300_seconds`,
> `:: polling_a_blocked_queue_resets_the_deadlock_timer`, `:: latest_only_keeps_the_newest`.

ACE keeps **both** a packet reorder buffer and a fragment reorder buffer. The client does neither: it
delivers packets in arrival order and relies on the blob layer.

## 6. Intervals, flow and time

### 6.1 The half-second interval

Once per connection per tick, the interval clock is advanced to catch up with the monotonic local
clock, half a second at a time. The interval id is a 16-bit counter written into every outgoing
header, and crossing certain multiples fires periodic work:

| Condition | Action |
|---|---|
| the counter crosses a multiple of **6** | enqueue a time-sync section **and** an echo request — that is, **every 3 seconds** |
| the counter crosses a multiple of **220** | send a standalone keep-alive to the server's **port + 1** — that is, **every 110 seconds** |

A large jump in the clock fires each of those **once**, not once per skipped interval.

> Pinned by `core/transport/src/flow.rs :: the_interval_cadence_fires_timesync_every_3s_and_icmd_every_110s`,
> `:: a_large_time_jump_fires_each_event_once`.

### 6.2 Flow sections

Every accepted packet is counted, header included, toward the peer's interval being accumulated.
When an incoming packet carries an interval **newer** than that one (a 16-bit wrapping comparison),
the client enqueues a 6-byte flow section — the byte count (`u32`), then the interval id (`u16`) of
the interval that just ended — resets the counter, stores the new interval id and counts the new
packet into it. The very first packet of a connection (the connect request) only sets the interval,
with no report; an older or equal interval ends nothing. So the client tells the server, roughly
twice a second, how much it actually received in the server's previous interval. The section is not
disposable, so a packet carrying it is sequenced and encrypted, and like the time-sync pair it waits
for the next packet to ride (§9.2). Nothing on the client reads the reverse direction.

> Pinned by `core/transport/src/session.rs :: a_newer_interval_closes_the_counted_one_with_its_byte_count`
> and `core/client-net/tests/cpu/net/flow_report.rs ::
> every_recorded_flow_report_is_reproduced_from_the_server_datagrams`, which rebuilds every report
> the recorded client sent from the server's datagrams alone.

### 6.3 Time synchronisation

**Client to server:** a time-sync section about every 3 seconds carrying the client's game time.

**Server to client:** on receipt,

```
set the game clock to the server's value          // a hard set, not a slew
if the game clock is at most 60 s ahead of the server's value
        clear the offence latch
else if the latch is clear
        latch the current time                    // first offence, remember when
else if the latch is more than 60 s old
        send a speed-hack error to this server
```

That is: **the client hard-slams its game clock to the server's value on every time sync**, and if its
own clock has been more than 60 seconds ahead continuously for more than 60 seconds, **it reports
itself**. The clock set here is the one physics, animation, spell durations and the day/night cycle all
read, so time sync is the master clock rather than a cosmetic display.

ACE sends a time sync every **20 seconds** and does not act on incoming ones; it does its own
speed-hack detection from the echo timestamps. Note the asymmetric cadence: the client sends every 3
seconds, the server every 20.

> Pinned by `core/client-net/tests/cpu/net/time_sync_speed_check.rs ::
> a_client_inside_the_window_sends_nothing_and_never_latches`,
> `:: past_the_window_and_the_latch_the_client_accuses_itself_with_one_lone_net_error`,
> `:: the_accusation_repeats_until_a_sync_lands_inside_the_window`.

### 6.4 Echo, and the link-quality display

The client sends an echo request about every 3 seconds carrying its monotonic local time, restamped
immediately before each transmission. The peer answers with the echoed value plus the time it held the
request, recomputed at *its* send time. So:

```
round trip = local_time - echoed - held
```

and queuing delay on the responder is subtracted out rather than counted as latency.

A heartbeat every **2.0 seconds** takes a snapshot for the link-quality display. The rolling windows
are: 4 samples of round-trip delay; 40 samples each of packets sent, retransmitted, received and
"naked"; 2 samples each of bytes sent and received. The displayed loss figure is

```
loss = 2 * (naks + retransmits) / (received + sent)
```

with the doubling being literal rather than an average of two terms. It is a ratio, not a percentage,
whatever the label says, and it starts at **1.0** — a client that has heard nothing yet reports total
loss. The window is 40 heartbeats, so up to 80 seconds of traffic.

The "naked" count is the number of sequence ids named in retransmit requests received **from the
peer**, the "retransmitted" count the number of packets this side resent, so both terms measure the
peer's losses of this side's packets.

> Pinned by `core/client-net/src/linkstatus.rs :: the_loss_is_twice_the_nak_and_retransmit_share_of_both_directions`,
> `:: the_window_is_forty_samples_and_the_forty_first_evicts_the_first`,
> `:: the_initial_packet_loss_is_one`, `:: heartbeat_copies_rtt_without_sampling_the_ping_ring`.

The connection-progress value the UI shows during login moves in documented steps: 0.1 while
initialising, `0.1 + 0.5 × sent/total` while authenticating, `0.6 + 0.03 × attempt` clamped at 0.9
while connecting, and 1.0 once connected.

## 7. Keep-alive and losing the connection

| Mechanism | Period | Effect |
|---|---:|---|
| Link snapshot (current world server only) | 2.0 s | Updates the rolling windows. |
| Cumulative acknowledgement | 2.0 s, when nothing is missing | Keeps traffic flowing both ways. |
| Time sync and echo request | ~3.0 s (every 6 intervals) | Keeps the peer's last-data time fresh. |
| Keep-alive to port + 1 | 110 s (every 220 intervals) | Keeps the NAT binding for the server's second port alive. |
| Connect-response resend | 0.333333333 s | Until the handshake completes. |
| World-login retry | 0.333333333 s, up to 840 tries (280 s) | Then a world-connection error. |
| Login-request retry | 2.0 s, up to 20 tries (40 s) | Then a client-timed-out error. |

**Declaring the connection lost**, evaluated every tick for every live connection:

```
if we were not ourselves stalled for 140 s
 and nothing valid has arrived for 140 s
 and the connection is not already tearing down
        move it to "disconnect received"
```

The first clause matters: it suppresses the timeout when the *client* was frozen — a debugger, a
minimised window, a disk stall — for more than 140 s since the previous tick, so a resumed client does
not immediately disconnect itself.

From "disconnect received", the connection sends its own disconnect-with-error once the send queue is
empty **or** 10 seconds have passed, moves to "disconnect sent", and is torn down on the next tick.

**ACE's session timeout is 60 seconds against the client's 140.** The client tolerates more than twice
the silence the server does, so in practice the server gives up first.

## 8. The receive loop

Single-threaded, on the main thread. **There is no network thread anywhere in the client**, and that is
not an accident: packet processing mutates the game clock and the blob queues, which the rest of the
frame reads without locking.

```
once per rendered frame:
    update the clock
    pump window messages
    network tick:
        zero the per-frame counters
        start a 50 ms budget
        read and process packets   -- until the socket is empty or the budget is gone
        service connections        -- same budget
    drain the login-event queue
    send
    drain the dat-cache queue
    tick the UI
    drain the world queue
    drain the UI queue
    ... render ...
```

The **50 ms receive budget** is re-checked on every iteration of both loops, so a flood cannot stall the
frame indefinitely. The socket is non-blocking, so the read loop drains the receive buffer and stops
when the socket would block.

There is **exactly one** receive buffer, reused for every datagram, and fragments point into it — which
is why reassembly copies immediately rather than retaining a reference.

A negative result from the socket that is not "would block" — in practice the connection-reset that
Windows raises after an ICMP port-unreachable for an earlier send — ends the loop. The client does not
act on it.

> Pinned by `core/client-net/src/socket.rs :: the_receive_budget_is_fifty_milliseconds`,
> `:: conn_reset_and_would_block_end_the_loop_quietly`,
> `:: a_real_socket_binds_non_blocking_with_a_large_receive_buffer`.

## 9. The send loop

Each connection sits on a **0.5-second timer wheel**, so an idle connection is still serviced twice a
second; and it is queued for immediate service whenever a blob or a section is enqueued for it, or a
retransmit request arrives, so outgoing data is not delayed by the wheel.

Servicing a connection does, in order:

1. flush the rejection list;
2. resend everything the peer asked for;
3. coalesce and send new data;
4. if the connection is tearing down and either the send queue is empty or the state is more than
   **10.0 seconds** old, enqueue the disconnect-with-error;
5. advance the interval.

### 9.1 Fragment batching

```
while a blob can be dequeued:            // min-heap by priority
    cut it into 448-byte fragments
    for each fragment:
        find the first waiting packet that
              has not been checksummed or sent
          and can still take this fragment within 464 payload bytes
          and holds fewer than 29 fragments
          and is not a lone exclusive section
        if found: append to it
        else:     start a new packet and enqueue it
```

**A packet that carries optional sections goes to the head of the waiting list and a pure-fragment
packet to the tail**, so control traffic overtakes bulk data.

**Lower priority values are sent first**, and every outbound blob in the client carries priority 5 —
there is no other priority anywhere — so the priority heap is effectively first-in-first-out. Keep the
parameter in the API anyway, so a rebuild can prioritise movement over chat without changing the wire
format.

> Pinned by `core/transport/src/flow.rs :: control_packets_overtake_bulk_data`,
> `:: lower_priority_blobs_are_sent_first`, `:: a_large_blob_fragments_across_packets_in_index_order`,
> and `core/client-net/src/queues.rs :: the_only_send_priority_is_five`.

### 9.2 Transmitting

Optional sections wait apart from the packets until the next packets are built. They are then placed
one at a time in ascending mask order: each joins the first waiting packet of sections (not yet
checksummed, holding no fragments, not a lone exclusive section) that does not already carry its mask,
or else starts a new packet at the head of the waiting list. An exclusive section always travels alone.

For each waiting packet: build the flags from the fragment count and the sections present; refuse to
send an empty tail packet — one whose flags below `0x01000000` are all clear, tested before the
encrypted-checksum bit is added, while it is the last packet waiting. The time-sync, echo-request and
flow sections all lie above that mask, so a packet holding only those waits until an acknowledgement
or a data fragment joins it, or another packet queues ahead of it; that is why the recorded periodic
packet is `0x0B004002` and why a second flow report can go out alone (`0x08000002`) ahead of an older
one still waiting. Then refresh time-sensitive sections (so a waiting time-sync carries the time it
is sent at); compute the checksum if it is not already
computed; then either reuse the current sequence (unreliable control traffic) or take a new one,
cipher the checksum and cache the packet. The datagram is emitted as one scatter list — the header,
each section's bytes, and for each fragment its 16-byte header and its payload separately — linearised
into a single send. A short write is reported as a failure and stops the loop.

> Pinned by `core/transport/src/flow.rs ::
> the_periodic_sections_wait_for_an_acknowledgement_and_carry_the_send_time`,
> `:: a_second_report_goes_first_and_the_acknowledgement_takes_the_held_one`.

## 10. How the pieces interact

- The **connection table** and the socket are ticked from the frame loop; so is the **send side** and
  its timer wheel.
- **Reassembly** is per connection: it turns fragments into blobs and pushes them onto the numbered
  queues, which consumers drain later in the same frame.
- The **clock** is both a consumer and a producer here: the game clock is set from time sync, while the
  monotonic local clock drives every timer. Note the asymmetry — the connection timeout compares
  monotonic deltas, but the stall guard compares *game-clock* deltas.

## 11. Notes for a reimplementation

**Design opinion.**

- Drive everything from one tick with an explicit 50 ms receive budget. Do not use a blocking socket or
  a reader thread; parse off-thread if you like, but *apply* inside the frame.
- Keep the exact timer constants: 0.5 s interval, 2.0 s acknowledgement, 0.6 s retransmit request, 3 s
  time-sync and echo, 110 s keep-alive, 140 s connection timeout, 0.333333333 s handshake resend. A
  server that expects the original cadence will otherwise see the client as idle or as flooding.
- Reproduce both sequence rules: the counter never becomes 0, and unreliable packets reuse the current
  value rather than taking a new one. ACE special-cases acknowledgement and retransmit-request packets
  on exactly this basis.
- Bound the retransmit cache by count and age, but keep the eviction semantics — and remember to strip
  disposable sections when caching, or a resent packet carries a stale acknowledgement.
- **Do not implement packet reordering. Do implement blob-level ordering**, including the
  19-entry/300-second deadlock breaker.
- Send-side pacing is a behaviour *change*. If you add it, keep the priority bypass.

## 12. Known gaps

- The reverse flow hook — the one that would consume the peer's interval acknowledgement — has no
  caller. It is dead in the client.
- The branch that re-refers a connection on timeout when a referral cookie is still held is
  reproduced from its observable behaviour rather than from a complete reading:
  `core/client-net/tests/cpu/net/referral.rs :: the_140_second_timeout_re_refers_with_the_stored_cookie`,
  `:: a_timeout_without_a_referral_cookie_is_the_ordinary_disconnect`.
