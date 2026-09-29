# Game events

A **game event** is server-to-client and is dispatched on **the first dword after any header**. It
**may** be wrapped in the 12-byte ordered header; the wrapper is the *sender's* choice, because the
reader rewinds when the magic is absent and the whole payload is then dispatched unchanged.

**Provenance:** from the wire format, which public readers document; the census in section 3 is
observed from recorded traffic.

**Readers:** `dereth_protocol::events`. Pinned by `a_game_event_is_the_wrapper_then_the_sub_type_then_the_body`,
`an_unwrapped_blob_still_splits_correctly`, `the_not_game_events_list_is_the_documented_fifteen`,
`the_event_and_action_spaces_are_independent` and `the_event_body_aligns_on_sixteen` in
`core/protocol/src/events.rs`.

## 1. Framing

```text
wrapped:  [0xF7B0][object id][stamp][sub-type][payload]
bare:                               [sub-type][payload]
```

**The payload's alignment origin is the start of the whole message**, not the start of the payload.
For a wrapped event that origin is 16 — twelve bytes of wrapper plus the sub-type dword. Framing a
body independently and then prefixing the wrapper aligns on the wrong origin, and every
variable-length field after the first goes wrong.

Splitting a received message is therefore: try the wrapper; if the magic is absent, treat the whole
payload as the body, with the origin set accordingly.

## 2. Two number spaces that look like one

The event and action spaces **use the same small integers and are independent**. `0x0005` is an
option change as an *action*; `0x0013` is the player description as an *event*. A value can exist in
one space and not the other.

This is worth stating because the usual convention — listing sub-types as opcodes in one flat table —
makes them look like a single space.

## 3. Which messages are events, and which arrive bare

Fifteen opcodes share the UI queue with the game events and are **not** game events: they arrive
unwrapped and are handled by a different path. They are the character set, the character error, the
creation response, the log-off and delete acknowledgements, the account banned and booted messages,
the subscription-expiry message, the server-ready message, the world info, the visual-description
message, the environment message, the two administrative data messages, and the text-box string.

Beyond that list, **no queue-derived rule decides whether a given message arrives wrapped.** This is
worth being blunt about, because it is the most natural thing in the world to assume otherwise:

- Measured over a recorded corpus of several thousand server-to-client messages, of the 68 opcode
  values observed, **13 are sent bare while a queue-based predicate calls them wrapped** — over a
  thousand occurrences in all. None goes the other way, so a queue-based rule is a sound *upper
  bound* on "might be wrapped" and nothing more.
- The shortest proof that no better queue-derived rule exists: two **consecutive** opcodes in the
  table, with identical send and receive queues, are observed one way each — one always wrapped, the
  other always bare.

A reader must therefore do what the client does: **try the wrapper, and rewind if the magic is not
there.** Anything else mis-parses one message in five.

## 4. Where reimplementations differ

| topic | the client | common reimplementations |
|---|---|---|
| The wrapper | Optional, detected by magic. | Frequently assumed present for everything on the UI queue. |
| The alignment origin | The whole message. | Frequently the body, which breaks every variable-length field after the first. |
| The two number spaces | Independent. | Often flattened into one table, which reads as one space. |
