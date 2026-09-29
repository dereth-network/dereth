# Game actions

**Every message this client sends to the world server is a game action**, and there are about 157 of
them.

```text
+0x00  u32   0xF7B1
+0x04  u32   stamp
+0x08  u32   sub-type
+0x0C  ...   payload, aligned to 4 after every variable-length field
```

**Provenance:** from the wire format, which public readers document.

**Readers:** `dereth_protocol::actions`. Pinned by
`a_game_action_is_the_order_header_then_the_sub_type_then_the_body`,
`the_payload_aligns_on_the_blobs_origin`, `the_master_table_holds_the_documented_number_of_game_actions`,
`the_non_game_actions_are_bare_and_the_queue_map_is_a_contract`, `the_client_never_sends_f7eb` and
`the_two_framing_paths_agree` in `core/protocol/src/actions.rs`.

## 1. The stamp

The stamp comes from **a single global counter shared by every category**, incremented before the
send and **rolled back when the send fails**.

That rollback is not tidiness: **the server drops everything after a hole in the sequence.** A client
that lets a failed send consume a number stops being heard from, without any error on either side.

## 2. Twelve messages are not game actions

The game-action framing applies to the **weenie queue** only. Messages the client sends on the
control, logon and database queues carry **no ordered header**: they start with the opcode dword
directly. There are twelve of them: four on the control queue (force appearance `0xF6EA`, the
friends command `0xF7CD`, and two administrative requests), five on the logon queue (enter world,
the enter-world request, log off, character delete and the character-creation result), and three on
the database queue (the data request, the interrogation response and the end-of-patching message).

The mapping is a contract, not a convention:

| queue | framing | recipient |
|---|---|---|
| weenie (3) | ordered game action | the world server |
| control (2) | bare | the world server |
| logon (4) | bare | **the login server** |
| database (5) | bare | **the login server** |

**Against a combined deployment both server addresses are the same**, so a hard-coded recipient
passes every local test and breaks the moment the two are separate. The send also stamps a different
ordering-type nibble for the login recipient.

## 3. The alignment origin, again

The payload's origin is **offset 12** — the whole message's start, not the body's. Encoding a body
independently and then prefixing the header aligns on the wrong origin. The two framing paths this
client offers — encode-in-place and frame-a-prepared-body — are required to agree, and a test says
so.

## 4. One opcode the client never sends

The community catalogue lists `0xF7EB` as the client-to-server end-of-patching message. **It is
not.** The end message is `0xF7EA` **in both directions**, and `0xF7EB` is a separate
**received-only** "patching pending, wait" message. See [12-admin-and-misc.md](12-admin-and-misc.md).

## 5. Where reimplementations differ

| topic | the client | common reimplementations |
|---|---|---|
| The action stamp | Kept gap-free, rolled back on failure. | Servers generally ignore it, so the discipline only matters against one that does not. |
| The twelve bare messages | No ordered header. | Easy to miss when every other outbound message has one. |
| The login recipient | Queues 4, 5 and 8. | Frequently hard-coded to one address. |
| `0xF7EB` | Never sent. | Listed as client-to-server in the catalogue. |
