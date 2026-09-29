# The connection state machine

**Provenance: wire format**, with the handshake sequence and the port-plus-one rule **observed at
runtime** on recorded sessions. §10 is design opinion.

## Summary

The client holds up to **256 simultaneous connections**, each identified by an id the *server* chooses.
In practice at most two are live: the login server and the current world server. The original
deployment ran both roles on one process, and ACE does too.

Establishing a connection is a three-way handshake:

1. the client sends a **login request** to the server's port `P`;
2. the server answers from `P` with a **connect request** carrying a cookie, the id the client must
   use, and the two cipher seeds;
3. the client answers with a **connect response** echoing the cookie — **to port `P + 1`**;
4. the first packet from the server that is not a connect request completes the connection.

Everything above that — character list, dat patching, enter world, world switching — is ordinary blob
traffic on the login and UI queues.

## 1. The states

| Value | Name | Reached when | Left when |
|---:|---|---|---|
| 0 | disconnected | Initial and cleared value | A connect request arrives |
| 1 | awaiting world auth | *never assigned by the client* | — |
| 2 | auth sent | *never assigned by the client* | — |
| 3 | connection request sent | *never assigned by the client* | — |
| 4 | connect request acknowledged | After the client has answered with its connect response | The first packet from the server that is **not** a connect request |
| 5 | connected | On that first non-connect-request packet | Disconnect, error, or the 140-second timeout |
| 6 | disconnect received | A disconnect-with-error arrives, or the 140-second timeout fires | The connection answers with its own disconnect-with-error |
| 7 | disconnect sent | Our disconnect-with-error went out | Teardown, on the next tick |

**The client occupies five of the eight.** States 1 to 3 exist because the transport library was shared
with the original server; a client never enters them. A rebuild only needs the five.

Two consequences: the send path refuses to queue a blob once the state is 6 or higher, and a referral
naming a connection whose state is above 5 tears that connection down before re-using its id.

> Pinned by `core/transport/src/conn.rs :: the_client_occupies_five_of_the_eight_states`.

A second, smaller state says whether the connection currently wants to **acknowledge** (nothing
missing) or **request retransmission** (something missing). It is recomputed from the parked-key set on
every tick.

## 2. Bring-up, before any packet

1. Fill in the authenticator: a password argument selects account-and-password, a ticket argument
   selects the GLS form, and neither selects account-only. **The credential is serialised in the
   archive string form — a compressed length (one byte for anything under 128) then the bytes, with no
   padding — which is not the 16-bit-length-plus-alignment form the account name uses two fields
   earlier in the same packet.**
   That asymmetry is real, it is confirmed against recorded sessions, and it is the field a
   reimplementation most often gets wrong.
2. Register the link-status observer.
3. Optionally load the packet-logging hook library and resolve its two entry points.
4. Initialise: copy the logger pointers, take a connection sequence number from the real-time clock,
   parse the host string, create and bind the socket, and start the login state machine.
5. **Spin**, pumping the network, the UI and one rendered frame per iteration, until the state machine
   ends. Pressing Escape during that loop aborts the connection.

That spin is worth noticing: `connect` **blocks**, and the frame pump exists only because it blocks. It
is also called **exactly once**, before the main loop, so the original has no reconnect. A rebuild
should make connect asynchronous and reconnect possible from the start — retrofitting re-entrancy into
a subsystem that assumed single initialisation is far worse than designing for it. That is a deliberate
deviation.

## 3. The handshake, packet by packet

### 3.1 Client to login server: the login request

Sent immediately the first time and then **every 2.0 seconds**, for at most **20 attempts**; when a
twenty-first would be due — 40 seconds after the first — the client raises a client-timed-out error
instead. Each attempt drives the progress bar.

The datagram is hand-built:

| field | value |
|---|---|
| sequence | 0 |
| flags | the login-request bit alone |
| checksum | header hash + payload hash, **not** ciphered |
| id | 0 |
| interval | 0 |
| size | the section's length |
| iteration | 0 |

The body carries the client version — always the literal `"1802"` — and the authenticator. ACE checks
the version string exactly.

Destination: the host and port from the command line. The compiled-in default port is **7304**; the
original deployment and ACE both use 9000.

> Pinned by `core/transport/src/conn.rs :: login_request_resends_every_2s_for_20_tries`,
> `:: the_login_request_carries_the_literal_1802`,
> `:: the_login_request_packet_is_unsequenced_and_plaintext`.

### 3.2 Login server to client: the connect request

On arrival:

1. **If the slot is already in use**, compare iterations with a wrapping 16-bit compare. Equal or older
   ⇒ **ignore the packet entirely**. Newer ⇒ tear the old connection down and continue, so a new
   iteration replaces a stale connection. *A server restart that cannot re-adopt its clients is a
   reimplementation that skipped this rule.*
2. Initialise the connection: store the packet's source **address and port**; set the highest received
   sequence to 1, so the server's first sequenced packet must carry 2; store the assigned id, the
   iteration and the peer's current interval; construct the two cipher streams from the two seeds,
   **each from the seed named for the opposite direction**; zero the latency and stamp the heartbeat.
3. Create the send-side state and put it on the half-second timer wheel.
4. Link the connection into the live list, with nothing missing.
5. **If this is the first connection, it is the login server** — both the login and the current-world
   ids are set to it.
6. Remove any matching entry from the referral queue, and store the cookie.
7. Move to state 4 and answer.

**The server's game-time field is not consumed here.** The clock is set by the first time-sync section,
which is why a server has to send one early.

> Pinned by `core/transport/src/conn.rs :: a_newer_iteration_tears_down_and_rebuilds`,
> `:: connect_request_round_trips_and_names_its_seeds_from_the_servers_view`.

### 3.3 Client to login server: the connect response, on port + 1

The answer is one 8-byte section echoing the cookie, sent standalone with sequence 0, the assigned id,
the connection's iteration and interval 0 — **to the stored address with the port incremented by one**.

It is **re-sent every 0.333333333 s** while the connection is in state 4, with **no attempt limit**; the
140-second no-data timeout is the only escape.

**Why port + 1.** The server binds two UDP sockets, `P` and `P + 1`. The login request goes to `P` and
the server replies from `P`, so the client's stored address records port `P`. The connect response
goes deliberately to `P + 1`, which tells the server which source port the client transmits from and
lets it start using its `P + 1` socket for everything afterwards. From then on the client keeps sending
to `P` and receives from `P + 1` — and **header verification compares the source address but never the
source port**, which is exactly what makes that asymmetry legal.

Implement both halves or the first server packet after the handshake is rejected.

ACE routes by listener: packets arriving on `P + 1` must be a connect response — matched against every
session awaiting one, by cookie and source address — or a keep-alive; everything else arrives on `P`.

> Pinned by `core/transport/src/conn.rs :: connect_response_goes_to_port_plus_one`,
> `:: the_connect_response_packet_echoes_the_cookie_in_the_clear`, and
> `core/transport/src/session.rs :: verify_header_rejects_in_order_and_ignores_the_source_port`.

### 3.4 Connection established

```
if state == 4 and this packet is not a connect request:
        state = connected
        clear the referral cookie
        if the login state machine is still running, end it successfully
else if state == connected:
        clear the referral cookie
```

Ending the state machine successfully is what releases the blocking connect and lets the UI proceed.

### 3.5 The sequence, in one picture

```
Client                                    Server (login role, ports P / P+1)

  -- every 2.0 s, at most 20 times -->
  [P]  seq=0  LoginRequest("1802", authenticator)

                                <-- [from P] seq=0 ConnectRequest
                                    {serverTime, cookie, id, two seeds}

  -- every 0.333333333 s until any non-ConnectRequest packet -->
  [P+1] seq=0  ConnectResponse{cookie}

                                <-- [from P+1] seq=2, encrypted
                                    TimeSync + fragments
                                    (character list, server name, dat interrogation)

  state = connected
```

## 4. After the handshake

All of it is ordinary blob traffic; the packet layer is not involved.

```
Server -> Client   0xF658  character set                [UI queue 9]
Server -> Client   0xF7E1  world info                   [UI queue 9]
Server -> Client   0xF7E5  dat interrogation            [dat queue 5]
Client -> Server   0xF7E6  interrogation response       [dat queue 5]

  (only when the dat files are out of date)
Server -> Client   0xF7E7  begin patching               [dat queue 5]
   per resource:
Server -> Client   0xF7E2  data                         [dat queue 5]
Client -> Server   0xF7E3  request data
Server -> Client   0xF7EA  end patching
Client -> Server   0xF7EA  end patching                 -- NOT 0xF7EB

  (the player picks a character)
Client -> Server   0xF7C8  enter-world request          [login queue 4]
Client -> Server   0xF657  enter world {object id, account}  [login queue 4]
Server -> Client   0xF7DF  server ready                 [UI queue 9]
Server -> Client   0xF746  create player                [world queue 10]
Server -> Client   0x0013  player description           [UI queue 9]
```

The patching exchange sits between the character list and enter-world, runs on queue 5, and is the
reason the client registers that queue at all. The community catalogue gets the end message wrong —
see [../CORRECTIONS.md](../CORRECTIONS.md) §5.

> Pinned end to end by `core/client-net/tests/cpu/login/session_state.rs ::
> login_through_enter_world_reaches_playable`,
> `:: the_ddd_exchange_completes_and_replies_with_f7ea`,
> `:: create_player_establishes_the_player_id`,
> `:: the_enter_world_burst_dispatches_live_rather_than_waiting_for_the_player_description`.

## 5. Switching servers

The original deployment split the login and world roles onto different hosts. The client supports that
three ways, all still present and all collapsed by ACE onto one process.

### 5.1 Server switch

Idempotence is enforced per switch type by a small history record — a stamp and a "has ever switched"
flag. A switch is applied if this type has never switched before, or if the stamp is newer by a
wrapping 32-bit compare.

- **World switch:** the current world server becomes the connection this packet arrived on.
- **Logon switch:** the login server becomes it, and the current world server too if the client is not
  in the world.

> Pinned by `core/client-net/tests/cpu/net/referral.rs ::
> a_world_switch_moves_the_current_server_and_a_stale_stamp_does_not`,
> `:: a_logon_switch_moves_the_current_server_only_outside_the_world`,
> `:: exit_world_disconnect_clears_the_world_switch_history_only`.

### 5.2 Referral, and the world login request

A referral carries a cookie, an address and a connection id. If that id is already live, only the
cookie is stored; if its state is above "connected", the connection is torn down first; if the slot is
free, an entry is queued.

The queue is walked on every service tick:

```
for each entry:
    if its next-send time is in the future, skip
    if it has already sent 840                        // 280.0 / 0.333333333
        drop it and raise a world-connection error
    send a standalone world-login request carrying the cookie
       to the entry's address, with sequence, id and iteration all zero
    schedule the next attempt 0.333333333 s later
```

The entry is removed when the target server answers with its own connect request. So: **840 attempts
over 280 seconds, then an error.**

> Pinned by the tests in `core/client-net/tests/cpu/net/referral.rs`, including
> `:: a_referral_queues_one_entry_and_fires_immediately`,
> `:: a_second_referral_for_the_same_server_is_ignored`,
> `:: two_referrals_for_different_worlds_both_run`,
> `:: the_referred_server_s_connect_request_ends_the_cadence`,
> `:: the_referral_never_uses_the_handshake_port`,
> `:: a_pending_referral_does_not_disturb_ordinary_traffic`.

### 5.3 Logon server address

A raw socket address that redirects the remaining login-request retries at a different login server.
Not implemented by ACE.

## 6. Error codes

An error is two dwords — a string hash and a table id that is always **8** — resolved through the
client's own string table, so that the player sees the localised text rather than anything hard-coded.
There are **21** codes.

The player-visible strings are content of the client's data files and are not reproduced here. What a
reimplementation needs is the set of conditions and **which side raises each**, because that is what
decides where the code lives:

| Condition | Raised by |
|---|---|
| The server address could not be parsed | the client, locally, while creating the socket |
| The local socket could not be bound — usually another client already running | the client, locally |
| The local socket could not be created at all | the client, locally |
| The cryptography interface could not be initialised | the client, locally |
| The player pressed Escape during the handshake | the client, locally |
| Twenty login requests went unanswered | the client, after the retry budget |
| No error — the sentinel a connection carries in its own disconnect-with-error reply (§7.2) | either |
| A speed-altering program was detected | the client — **sent to the server**, see [02-reliability-and-flow.md](02-reliability-and-flow.md) §6.3 |
| No logon server is available for the account | the server |
| The client version is not current | the server |
| The server is full | the server |
| The authentication is invalid or corrupt | the server |
| The account has insufficient privilege | the server |
| The connection was pre-empted by another attempt | the server |
| The server closed the connection | the server |
| The server timed the connection out | the server |
| The player is already logged on | the server |
| Client logon failed | the server |
| Account authentication failed | the server |
| Logon was aborted by the server | the server |
| A generic connection error; the client raises it when the login request could not be sent, the authenticator was absent, or the login connection died unexpectedly | either |

**There is one inconsistency worth knowing about:** one of the 21 identifier names carries a spelling
error, and that spelling is load-bearing, because the identifier is hashed to produce the value that
travels on the wire. A reimplementation that "corrects" the name computes a different hash and stops
resolving the string.

> Pinned by `core/transport/src/conn.rs :: all_21_net_error_ids_resolve_to_the_documented_hashes`,
> `:: the_misspelled_id_is_load_bearing`, `:: net_error_packs_to_eight_bytes_with_table_id_8`.

ACE mostly sends its own character-error messages as blobs instead of transport error sections, and its
termination reasons do not map onto these codes.

## 7. Every way a connection dies

### 7.1 The server sends an error section

If the login state machine is still running, it fails with that error. Otherwise the client logs off
every connection and raises the notice that returns the UI to the login screen.

### 7.2 The server sends a disconnect-with-error

Handled twice: the transport moves the connection into "disconnect received", and the client
additionally — when the error is not the "no error" sentinel — raises a login or world connection
error, depending on which connection it arrived on, so the UI can show the string.

From "disconnect received", the connection sends back a disconnect-with-error carrying the "no error"
sentinel once the send queue is empty **or** 10 seconds have elapsed, moves to "disconnect sent", and
is torn down on the next tick.

### 7.3 The server sends a bare disconnect

Zero-length. **There is no client handler at all**: the section is consumed so the packet is not
rejected, and the connection then dies through the 140-second timeout. The client only ever *sends*
this flag.

### 7.4 The client logs off

1. Build one bare disconnect section and send it as a standalone packet to **every** live connection,
   one packet each.
2. **Go silent**: from this point the send path returns without touching the socket.
3. Forget the login server id.
4. Run the exit-world teardown.

The exit-world teardown — also used for "return to character list" — clears the in-game flag, makes the
login server the current server again, tears down every connection that is not the login server,
re-points the outgoing id at the survivor, clears the survivor's missing-packet state, clears the
world-switch history and resets the game-action counter.

> Pinned by `core/client-net/tests/cpu/net/disconnect.rs` and `core/client-net/tests/cpu/login/exit_world_teardown.rs`:
> `:: the_teardown_drops_the_world_and_keeps_the_login_server`,
> `:: the_surviving_connection_leaves_its_nak_state_behind`,
> `:: against_a_single_server_shard_the_loop_removes_nothing_and_the_rest_still_runs`,
> `:: the_teardown_can_never_re_enter_log_off_server`,
> `:: reached_through_the_goodbye_the_teardown_empties_the_table`.

### 7.5 Timeout

140 seconds without a valid packet, provided the client itself was not stalled for that long. See
[02-reliability-and-flow.md](02-reliability-and-flow.md) §7.

### 7.6 Teardown

Free the send-side state, unlink the connection, zero it, free both cipher streams, destroy the
parked-key set, and set the state back to disconnected. **If the connection being torn down was the
login server or the current world server and the client has not already said goodbye, it says goodbye
to everything and raises the server-died notice.**

### 7.7 The whole machine

```
  [start] --> disconnected

  disconnected --> connect-request-acknowledged
        on ConnectRequest: initialise, build the ciphers,
        create the send state, answer on port + 1

  connect-request-acknowledged --> itself
        every 0.333333333 s: resend the connect response to port + 1
  connect-request-acknowledged --> connected
        on any packet that is not a connect request
  connect-request-acknowledged --> disconnect-received
        140 s without data

  connected --> itself
        data; acknowledge every 2 s; request retransmission every 0.6 s;
        time sync and echo every 3 s; a flow section per remote interval
  connected --> disconnect-received     on a disconnect-with-error
  connected --> disconnect-received     140 s without data, if we were not stalled
  connected --> disconnected            we log off: bare disconnect sent, teardown

  disconnect-received --> disconnect-sent
        we answer with a disconnect-with-error, once the queue empties or 10 s pass
  disconnect-sent --> disconnected      teardown on the next tick
```

## 8. How this maps onto ACE

| Client state | ACE session state |
|---|---|
| disconnected | *(no session)* |
| connect request acknowledged | awaiting connect response |
| connected, login role | authenticated |
| connected, in world | world connected |
| disconnect received / sent | terminating |

ACE binds one listener per port — two, on `P` and `P + 1`, both with address reuse — and routes by
which listener received the datagram. It generates the cookie and the two seeds per session and
discards them after constructing the ciphers.

**Four differences that matter for a rebuild tested only against ACE:**

- ACE never sends a server switch, a referral or a logon-server address in normal operation, so those
  paths are untested against it.
- ACE always stamps iteration `0x01`, so the "newer iteration replaces the connection" path is never
  exercised.
- ACE's optional-section parser does not consume the error, error-disconnect, referral,
  logon-server-address or empty sections, so a client packet carrying one fails ACE's checksum check
  and is dropped.
- ACE's session timeout is 60 s against the client's 140, and its pre-authentication timeout is 15 s
  against the client's 20 × 2 s budget.

## 9. Dead machinery worth knowing about

The transport constructs a Diffie-Hellman context with a hard-coded 256-bit parameter pair and creates
a key-exchange object with a 10-bit private random for **every** connection — and **nothing ever puts
one on the wire**. The session keys arrive in clear in the connect request. The whole key-exchange path
is vestigial.

Do not implement it. **Do** implement its *absence*: a server that expects a key exchange is not
speaking this protocol.

> Pinned by `core/transport/src/conn.rs :: there_is_no_key_exchange`.

## 10. Notes for a reimplementation

**Design opinion.**

- Keep the connection id as the index into a 256-entry table. The server chooses it and the client must
  accept whatever it is told. **There are two ids, not one** — the one the server sends back and the one
  the client stamps — and three of the client's senders write a hard zero regardless of what was
  assigned (see [01-packet-format.md](01-packet-format.md) §1). A server that echoes the client's id,
  or a client that expects its own id in inbound packets, will look correct in a unit test and fail
  against the real thing.
- Implement the port-plus-one rule exactly, **including** comparing the source address but not the
  source port.
- The login state machine needs only its four observable outcomes — initialising, authenticating,
  connecting, connected or failed. The generic asynchronous-state-machine plumbing underneath can be a
  small enum.
- Keep the iteration rule, or a server restart cannot re-adopt a client.
- Resolve error codes through the same string table. Do not hard-code English text.
- Make connect asynchronous and reconnect possible. Record both as deviations.

## 11. Known gaps

- Three of the eight connection states are never assigned by the client and are presumed server-side.
- The bare disconnect section has no client-side handler at all. Whether the original server ever sent
  one is unknown.
