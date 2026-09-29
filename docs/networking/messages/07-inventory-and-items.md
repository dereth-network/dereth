# Inventory and items

The request half of inventory handling, the stack-size update and the salvage result. The containment
messages the client **receives**, and appraisal, are on [the world-objects page](02-world-objects.md).

**Provenance:** from the wire format, which public readers document.

**Readers:** `dereth_protocol::items`. Pinned by the six tests in `core/protocol/src/items.rs` —
notably `update_stack_size_is_byte_packed_with_an_unaligned_object_id`,
`reading_stack_size_as_aligned_takes_the_wrong_object_id`,
`salvage_result_data_has_no_percent_return_on_the_wire` and `a_trailing_byte_is_rejected`.

## 1. The byte-packed message

`0x0197`, the stack-size update, is **byte-packed in an otherwise dword-aligned protocol**:

| offset | size | field |
|---:|---:|---|
| `0x00` | 4 | opcode |
| `0x04` | 1 | sequence |
| `0x05` | 4 | item id — **unaligned** |
| `0x09` | 4 | amount |
| `0x0D` | 4 | new value |

Seventeen bytes including the opcode, and the dispatcher advances exactly that.

The community catalogue documents it as aligned like everything else. Reading it that way takes the
**wrong object id** — three bytes of the id plus one of the amount — and, because the misparse
shifts the cursor, corrupts everything after it in the same message. One other message has the same
shape: the housing restriction update, on [the trade page](10-trade-housing-vendor.md).

Two behaviours attached to it:

- The sequence goes through the per-property gate with the **integer stack-size property's** key, not
  a key of its own. See [03-qualities-and-updates.md](03-qualities-and-updates.md).
- The client **rejects the update when the amount exceeds the item's maximum stack size** — silently,
  and **the sequence number is still consumed**. A server that sends an over-large amount does not
  get an error; it gets an item that never updates again until the sequence moves on.

## 2. The requests

| opcode | size | body |
|---|---:|---|
| `0x0019` put item in container | 16 | item, container, 0-based slot |
| `0x001A` get and wield item | 12 | item, equipment location |
| `0x001B` drop item | 8 | an id |
| `0x0035` use with target | 12 | the item being used, the target |
| `0x0036` use | 8 | an id |
| `0x0054` stackable merge | 16 | from, to, amount |
| `0x0055` stackable split to container | 20 | stack, container, slot, amount |
| `0x0056` stackable split to ground | 12 | stack, amount |
| `0x019B` stackable split to wield | 16 | stack, equipment location, amount |
| `0x00CD` give object | 16 | **recipient first**, item, amount |
| `0x027D` create tinkering tool | var | the tool, then the objects to salvage |
| `0x0263` query item mana | 8 | an id |

Two things a server should know:

- **Wielding part of a stack is a different message.** The client sends the split-to-wield message
  instead of the plain wield when the item is a stack of more than one and the interface's split size
  is not the whole stack.
- **Tinkering has no opcode of its own.** Applying a salvage tool to an item is the ordinary
  use-with-target message, guarded by a local confirmation dialog. A server that expects a distinct
  message will never see one.

## 3. The responses

| opcode | body |
|---|---|
| `0x0264` query item mana response | an object, a mana fraction in `0..1`, and a flag that says whether to show the bar |
| `0x00C3` inscription response | an object, **an unnamed dword the client skips**, the inscription, the scribe's name and account — the whole message is parsed and discarded |
| `0x02B4` salvage result | the skill used, a list of unsalvageable ids, a list of results, and an augmentation bonus |

**A correction on the salvage result**: it does **not** carry a percentage-return double between the
results list and the bonus. That value is a member of the client's own structure and is not on the
wire — both the reader and the writer bracket the payload with a size check that leaves room for
exactly two dwords beside the two lists.

Each result is material, a workmanship **double**, and a unit count.

## 4. Where reimplementations differ

| topic | the client | common reimplementations |
|---|---|---|
| The stack-size message | Byte-packed, with an unaligned id. | Documented and implemented as aligned. |
| An over-large stack amount | Rejected, sequence consumed. | Not modelled. |
| Splitting to wield | Its own opcode. | Easy to miss behind the plain wield. |
| Tinkering | The use-with-target message. | Sometimes expected as its own message. |
| The salvage result's percentage | Not on the wire. | Listed in the catalogue. |
