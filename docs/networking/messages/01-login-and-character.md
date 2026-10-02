# Login and character management

Everything from "the transport is connected" to "the player is standing in the world": the character
list, character creation and deletion, entering the world, the player description, and the options
blob the client saves back.

**The asymmetry to understand first.** The client sends its requests on the **logon queue (4)**, and
the server answers on the **UI queue (9)**. That is not a convention, it is forced: the logon queue's
consumer in this client discards every opcode but one (see
[00-dispatch-and-queues.md](00-dispatch-and-queues.md)). A server that replies on the queue it was
asked on gets a client that sits at "connecting" for ever.

**Provenance:** from the wire format, which public readers document.

**Readers:** `dereth_protocol::login`, with `dereth_protocol::types::qualities` for the player description's
payload and `dereth_protocol::property` for the options bag. Pinned by the fourteen tests in
`core/protocol/src/login.rs` — among them `character_set_round_trips_and_carries_the_account`,
`the_chargen_checksum_sums_only_the_named_fields`, `the_spell_bar_gates_are_mutually_exclusive`,
`the_player_module_defaults_are_applied_when_the_gates_are_clear`,
`account_booted_distinguishes_an_absent_reason_from_an_empty_one` and
`log_off_has_a_different_body_in_each_direction`.

## 1. The exchange

```text
server -> 0xF658  the character set          (the first thing after the session authenticates)
client -> 0xF657  enter world  (character id, account)
client -> 0xF7C8  enter-world request        (an empty body)
server -> 0xF7DF  server ready               (an empty body)
server -> 0x0013  the player description
client -> 0x00A1  login complete             (an empty body, and an ordered game action)
```

Two of those deserve a note:

- **Losing the server-ready message leaves the client stuck at "connecting" with no timeout of its
  own.** The client's own disconnect timer keys off the player description, not off this.
- **The login-complete action has preconditions.** The client sends it only once the player
  description has arrived, the player object exists, and **every id in the description's two content
  lists has an object**. A server that waits for it before finishing its own login work, and that
  sent a content list naming an object it never created, waits for ever.

## 2. The character set (`0xF658`)

| field | type |
|---|---|
| `status` | `u32` |
| `characters` | a packed list of identities |
| `deleted` | a packed list of identities — slots pending deletion |
| `num_allowed_characters` | `i32`; the client's own default is 5 |
| `account` | string |
| external-chat flag | `u32` |
| expansion flag | `u32` |

An identity is an object id, a name, and a **countdown in seconds**: zero means the character is not
being deleted, and any other value is how long is left. Each identity is padded to 4.

Two list behaviours a server has to know about because the client's reaction depends on them:

- **Adding an identity dedupes by id only, never by name, and appends.** A repeated character-set
  message cannot double-add, and a new character lands at the **end** of the list rather than in a
  free slot.
- **Restoring a deleted character is a replace in place, not an append.** The restore reply re-states
  the same id with a zero countdown; overwriting the row is what takes the character out of the
  pending-delete state. Appending would be refused by the id dedupe and change nothing — and the
  "please wait" modal a restore puts up is closed by the notice that the replace raises, so getting
  this wrong leaves the modal up for ever.

A replace is refused on three conditions, all of which leave the modal up: the slot is past the end
of the list, the slot is negative (the comparison is unsigned, so −1 fails the same test), or the row
it names is empty.

## 3. Character creation

The creation payload is a long flat structure, and two things about it are easy to get wrong:

- **The fields interleave.** Style and colour alternate for headgear, shirt, trousers and footwear,
  and the six **shades** come only afterwards, as doubles.
- **Field 39 is a checksum**, and it is a plain arithmetic sum of *specific* fields, not of the whole
  packet: heritage, gender, the eight face and hair values, the shirt, trousers and footwear
  **styles** but not their colours, the template number, the six attributes and the slot. The client
  accumulates each value as it writes it, which is exactly why the colours and shades are excluded.

The response (`0xF643`) carries a response code and then, **only when the code is "OK"**, a character
identity. This is the single most consequential conditional in the family: the client's handler reads
the code, steps four bytes, and jump-tables — and the only arm that unpacks an identity is the
success arm. The six refusal arms clear the slot and raise the notice without touching another byte.

A reader that unpacks the identity unconditionally turns every real refusal — a name already in use,
most of all — into a malformed message, so the screen is never told and the modal never closes.

The same opcode is reused for the restore reply, which is why section 2's replace rule matters here.

## 4. The small messages

| opcode | direction | body |
|---|---|---|
| `0xF7E1` world info | S2C | connections, maximum connections, world name |
| `0xF651` awaiting subscription expiration | S2C | minutes |
| `0xF7C1` account banned | S2C | an absolute expiry in seconds — **zero or less means permanent** — and a reason |
| `0xF7DC` account booted | S2C | a reason that **may be absent entirely**, not merely empty |
| `0xF659` character error | S2C | an error code |
| `0xF65A` character screen message | S2C | two strings, shown one after the other in the character screen's message box. Clients before Throne of Destiny showed it; the end-of-retail client has no such message, and Dereth shows it in a window of its own |
| `0xF655` character delete | C2S | account, **slot index** — not the character id |
| `0xF655` character delete | S2C | an empty body |
| `0xF653` log off | C2S | the character id |
| `0xF653` log off | S2C | an empty body |
| `0xF630` set player visual description | S2C | a string the client **parses and ignores** — the notice it raises has an empty body |

Three of those repay attention:

- **The booted reason is distinguishable from an empty one**: when the body ends after the opcode,
  the client substitutes a fixed phrase about conduct. A codec that models the reason as a plain
  string cannot re-encode a bodyless instance as one.
- **Two opcodes have a different body in each direction.** Log-off and character-delete both do. A
  single struct per opcode cannot represent them.
- **Delete identifies the character by slot**, and the client resolves the slot from the id itself,
  refusing when the id is not in the list.

The character-error code space has about twenty values. Three of them have no case in the client at
all, and the undefined value displays nothing. The English text lives in the localised string table,
not in the client.

## 5. The player description (`0x0013`)

The largest message in the protocol: the character's whole quality set, the player module, and two
packed lists — the contents of the main pack, and the items currently equipped.

It is also the message that **opens the gate** on the UI queue: everything the client was holding
back is replayed once it arrives. See [00-dispatch-and-queues.md](00-dispatch-and-queues.md).

The quality half is documented with the [qualities family](03-qualities-and-updates.md); the player
module is below.

## 6. The player module

A flag word followed by optional sections, and the flags are not read in bit order.

| section | gate |
|---|---|
| character options word 1 | always |
| shortcuts | a flag |
| **spell bars** | three **mutually exclusive** gates, tested in a fixed order |
| desired components | a flag |
| spell filters | a flag; **defaults to `0x3FFF` when clear** |
| character options word 2 | a flag; **defaults to a fixed value when clear** |
| timestamp format | a flag |
| generic qualities | a flag |
| the gameplay-options property bag | a flag |

Three details decide whether a rebuild shows the right spellbook:

- **Bar 0 is always present.** The three gates add four more bars, or six more, or seven more — the
  first gate that is set wins and the others are not consulted. Reading them as independent flags
  gives the wrong bar count and then the wrong everything.
- **The two "when clear" defaults are real behaviour, not tidiness.** A client that leaves the fields
  zero shows an empty spell filter and the wrong chat channels.
- **One flag bit has no branch at all** in this client: the squelch list it names arrives as its own
  [communication](08-communication.md) game event instead.

The gameplay-options bag is a typed property collection whose values have **no length and no type tag
on the wire** — the reader recovers each value's width by looking its name up in a schema. See
[the property page](../../formats/03-serialisation-primitives.md) for the shape and
[the qualities family](03-qualities-and-updates.md) for the schema's role.

## 7. Options, going the other way

| opcode | what it carries |
|---|---|
| `0x0005` player option changed | **one** option ordinal and its new value, both full dwords |
| `0x01A1` character options | the **whole** player module |
| `0x019C` add shortcut | one shortcut record: index, object id, spell id |
| `0x019D` remove shortcut | a slot index |

The single-option message is the immediate half: it goes out **at the moment the option changes**.
The bulk message goes out when several change at once, when a shortcut or a spell bar changes, and on
a dirty timer measured in minutes — which is why recorded sessions contain so few of them.

The shortcut messages are sent **at the moment of the drop**, beside the local update, as two
consecutive calls. A client that does only the local half keeps a bar the server has never heard of:
it survives the session and is gone at the next login.

The option ordinal is **the option's index, not its bit**. About twenty of them have a server-visible
effect, and a handful cause the server to join or leave chat channels — which is the only way a
client can rejoin a chat room without logging out.

## 8. Where reimplementations differ

| topic | the client | common reimplementations |
|---|---|---|
| The reply queue | Logon requests, UI replies. | Must match, or the client hangs at the character list. |
| The creation response body | An identity only on success. | Frequently unconditional, which turns every refusal into a malformed message. |
| Restore | Replaces the row in place. | Sometimes an append, which the id dedupe silently refuses. |
| The booted reason | Absent and empty are different. | Usually one string. |
| Log off and delete | Different bodies per direction. | Often one struct per opcode. |
| The player module's clear-gate defaults | Applied. | Frequently omitted. |
| The spell-bar gates | Mutually exclusive, in order. | Frequently treated as independent flags. |
