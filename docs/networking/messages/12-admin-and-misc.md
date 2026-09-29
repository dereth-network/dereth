# Administration, progression and data patching

Character progression, the personal query commands, the administrative messages, and the data-patch
exchange that keeps the client's [containers](../../formats/01-dat-container.md) up to date.

**Provenance:** from the wire format, which public readers document.

**Readers:** `dereth_protocol::admin`. Pinned by the seven tests in
`core/protocol/src/admin.rs` — notably `the_ddd_end_message_is_f7ea_both_ways`,
`the_query_plugin_messages_carry_payloads`, `the_iteration_set_is_run_length_encoded`,
`the_tagged_iteration_list_puts_the_type_first` and `the_interrogation_exchange_round_trips`.

## 1. Progression

Four training actions, two dwords each and twelve bytes with the sub-type: raise a vital (`0x0044`),
raise an attribute (`0x0045`), raise a skill (`0x0046`), and change a skill's advancement class
(`0x0047`).

## 2. Personal queries

| opcode | what it is |
|---|---|
| `0x01C2` query age | Target zero means self. |
| `0x01C3` age response | A target name — **empty means self** — and an age **already formatted by the server**. |
| `0x01C4` query birth | **There is no dedicated response**: the server answers with an ordinary system-text message. |
| `0x01E9` request ping, `0x01EA` return ping | Measures the round trip **in the game-event layer**, not the transport's own echo. |
| `0x0216`–`0x0218` consent list | Clear, display, remove. |
| `0x0219` add permission | |
| `0x021A` remove permission | **`0x021A`, not `0x0220`.** The client writes those bytes and at least one server agrees; the catalogue's value is a documentation error. |
| `0x0140` abuse report | The reported character, a status that is always 1, and the complaint. |

## 3. The plugin-audit messages

**The catalogue gives all three of these empty payloads. All three carry one.**

| opcode | direction | body |
|---|---|---|
| `0x02AE` query plugin list | S2C | **a context dword** |
| `0x02AF` plugin list response | C2S | the context echoed, plus the list |
| `0x02B1` query plugin | S2C | **a context dword and a name** |
| `0x02B2` plugin response | C2S | context, success, name, author, email, web page |
| `0x02B3` plugin response | S2C | **five consecutive strings** |

The fifth string of the last one has no name in the client, and the handler is a stub that drops all
five — **but the cursor must still be advanced past them**, or everything after the message in the
same batch is read from the wrong place. "The handler ignores it" is not the same as "it is not
there".

## 4. Administrative

| opcode | direction | notes |
|---|---|---|
| `0xF7CA` receive account data, `0xF7CB` receive player data | S2C | A context dword and a packed list of rows, each a name and an id. The client parses both and **does nothing**: the panel that consumed them was removed before this build. |
| `0xF7CC` get server version | C2S, **control queue** | Four bytes, no ordered header. The response arrives as text. |
| `0xF7D9` restore character | C2S, **control queue** | An id and two strings that the client always sends empty. |
| `0xEA60` environs | S2C | One dword. Values 0 to 6, and 9999, set a landscape override; **101 to 124 (`0x65`–`0x7C`) play a one-shot interface sound instead** and leave the landscape alone; anything else is ignored. |

## 5. Data patching

The patch exchange is framed by the archive machinery rather than the message machinery, and — as
everywhere else — **it is not word-aligned**, so nothing in this half of the family pads.

```text
server -> 0xF7E5  interrogation
client -> 0xF7E6  interrogation response
server -> 0xF7E7  begin
server -> 0xF7E2  data          (repeated)
client -> 0xF7E3  request data  (as needed)
server -> 0xF7E4  error         (on a failed request)
  both -> 0xF7EA  end
server -> 0xF7EB  patching pending, wait
```

**The correction that matters here**: the catalogue calls `0xF7EB` the client-to-server
end-of-patching message. **It is not.** The end message is **`0xF7EA` in both directions**, and
`0xF7EB` is a separate **received-only** "patching pending" message. The client never sends `0xF7EB`
at all.

### 5.1 The structures

An **iteration set** is a count followed by a run-length list: a positive value counts one iteration,
a negative value counts one fewer than its magnitude, and the list is read until the accumulated
count is reached. It is the same encoding as the [iteration file](../../formats/01-dat-container.md)
in the containers themselves.

A **tagged iteration list** puts **the type dword first**, then the id dword, then the set. The two
halves are easy to swap — and the switch on the *second* dword is what settles which is which.

The **interrogation** carries the server's region, a language rule, a product id and the supported
languages. **Bit 2 of the product id asks the client to open the high-res container.**

The **response** carries the client's language, two lists of tagged iteration sets and a flag word.
The second list is empty in every observed session, and at least one server reads only the first and
skips the rest.

The **begin** message carries the total bytes to expect — the client subtracts what it already
received during interrogation before showing a figure — and a list of revision blocks, each naming a
container, an iteration, the ids to download and the ids to purge.

A **data** message carries the container's type and id, the resource's type and id, the iteration, a
compression byte, a version, a declared size and the payload. The declared size includes the version
dword, which is the same convention the containers use.

## 6. Where reimplementations differ

| topic | the client | common reimplementations |
|---|---|---|
| `0xF7EB` | Received only. | Listed as the client's end-of-patching message. |
| The end message | `0xF7EA` both ways. | Often only one direction. |
| The plugin-audit payloads | All three carry one. | Documented as empty. |
| The remove-permission opcode | `0x021A`. | The catalogue says `0x0220`. |
| The tagged iteration list | Type first. | Easy to reverse; the values happen to be small either way. |
| The second iteration list | Sent, empty. | Frequently skipped, which is harmless. |
