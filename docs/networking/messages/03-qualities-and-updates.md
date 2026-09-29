# Qualities, property updates and enchantments

**Forty-six small game events** keep the client's copy of every object's qualities up to date. For
each of the eleven quality types there is a **public** update carrying an object id and a
**private** update that is implicitly the player; skills, attributes and vitals add public and
private *level* updates (and skills an *advancement class* pair), which makes thirty update opcodes
(`0x02CD`–`0x02EA`). The eight table-backed types each add a **remove** pair: sixteen more
(`0x01D1`–`0x01DE`, and `0x02B8`/`0x02B9` for int64). The community catalogue's count of
"forty-four" undercounts the level pairs; the opcode table is the authority.

There is no "remove skill" and no "remove attribute": attributes, vitals and skills can only be
updated.

**Provenance:** from the wire format, which public readers document.

**Readers:** `dereth_protocol::qualities`, with `dereth_protocol::types::qualities` for the structures the
[player description](01-login-and-character.md) carries. Pinned by the ten tests in
`core/protocol/src/qualities.rs` — notably
`a_private_update_has_no_pad_after_the_sequence_byte`, `float_updates_carry_a_double`,
`only_the_string_updates_align`, `all_forty_four_quality_events_are_implemented` and
`remove_and_dispel_are_the_same_bytes` — and the eight in
`core/protocol/src/types/qualities.rs`.

## 1. Two shapes, and neither pads

```text
private  [u32 type][u8 seq][u32 property][value...]
public   [u32 type][u8 seq][u32 object][u32 property][value...]
```

**Nothing pads after the sequence byte.** The property id at offset 5 of a private update, and the
object id at offset 5 of a public one, are **unaligned**. The client reads them with an unaligned
dword load, which its platform permits.

This is the single thing to get right in the family, and it is invisible in a header-style
description: a reader that aligns after the byte reads three bytes of the property id plus one byte
of the value, and every field after that is wrong.

**The string updates are the only exception.** They align to 4 immediately before the string, and
only there.

## 2. The sequence byte

Every one of these carries an 8-bit sequence, validated per (object, stat type, property) with the
key `property_id | (stat_type << 16)` and the 8-bit wrap rule described in
[00-dispatch-and-queues.md](00-dispatch-and-queues.md).

Two aliases in that key space matter, because they make separate messages share one counter:

- **Skill, skill level and skill advancement class all use stat type 4.** The three messages for one
  skill share a single sequence counter.
- **Attribute and attribute level both use stat type 8**, and **vital and vital level both use
  stat type 9**.

Sending those out of order loses updates — silently, because a rejected update is simply not applied.

The stack-size message shares the integer property "stack size", so its key is that property's key,
not one of its own.

## 3. The value types

| quality type | value | note |
|---|---|---|
| int | `i32` | |
| int64 | `i64` | |
| bool | `i32` | **Not a byte.** |
| float | `f64` | **A double, not a float.** |
| string | a string | Preceded by an align to 4 — the only one. |
| data id | `u32` | |
| instance id | `u32` | |
| position | a wire position | |
| skill | a skill record | 28 bytes, with a version in the high half of a word. |
| attribute | an attribute record | |
| vital | an attribute record plus its current value | Regeneration ticks use the vital *level* update, a bare `u32`. |

The two that catch people are **bool is a dword** and **float is a double**. Neither produces an
error; both shift everything after them.

## 4. Enchantments

| opcode | body |
|---|---|
| `0x02C2` update enchantment | one enchantment |
| `0x02C4` update multiple enchantments | a packed list of enchantments |
| `0x02C3` remove enchantment | a layered spell id — the spell id with the layer in the high half |
| `0x02C7` dispel enchantment | **byte-identical to remove** |
| `0x02C1` update spell | a spellbook addition |
| `0x01A8` remove spell | a spellbook removal; the client sends this opcode too |
| `0x02C5` remove multiple enchantments | a packed list of layered spell ids |
| `0x02C8` dispel multiple enchantments | **byte-identical to remove multiple**, and silent |
| `0x02C6` purge enchantments | an empty body |
| `0x0312` purge bad enchantments | an empty body; drops only the harmful enchantments |

**Remove and dispel are the same bytes.** The only difference is in the handler: a dispel passes
"do not notify", so it is silent, where a removal prints the "spell expired" line. A client that
treats them as one message prints a line that the original did not.

Every enchantment message repeats the event type as its first dword, which is the double opcode
check of [00-dispatch-and-queues.md](00-dispatch-and-queues.md) rather than a field.

## 5. The quality structures

These are the payloads of the [player description](01-login-and-character.md) as well as of the
individual updates, and the two places must agree.

**Two things the wire order will catch you on:**

- **The optional blocks are not in bit order** in either the base structure or the full one. As with
  the physics descriptor, reading them in bit order parses the first few and then drifts.
- **A clear bit deletes the client's existing sub-object** rather than leaving it alone. "Not
  present" means "remove what you have", not "no change". A server that omits a block to save bytes
  is asking the client to forget it.

Further details worth stating, each pinned by its own test:

- the data-id and instance-id flags are **the client's way round**, which is not the way round every
  reimplementation has them;
- a skill record is **28 bytes with a version in the high half** of one of its words;
- the relative times are **rebased** the way the client does it, not carried absolute;
- the enchantment registry reads **cooldowns before vitae**;
- a spellbook page has **a four-byte and a twelve-byte form**;
- **undocumented optional blocks are rejected, not guessed at.**

## 6. Options in the description

The player description also carries the typed property bag the interface is built out of. Its values
have **no length and no type tag on the wire**: the reader recovers each value's width by looking the
property name up in a schema of several hundred descriptors. **A reader without the schema cannot
even skip a property**, let alone read one.

The two character-option words are carried verbatim, and the mapping from an option's index to its
bit is the client's own table — which is why the
[single-option message](01-login-and-character.md) sends the **index**, not the bit.

## 7. Where reimplementations differ

| topic | the client | common reimplementations |
|---|---|---|
| Padding after the sequence byte | None. | Frequently aligned, which corrupts every update. |
| Bool updates | A dword. | Sometimes a byte. |
| Float updates | A double. | Sometimes a float. |
| Remove versus dispel | Same bytes, different notification. | Often merged. |
| A clear optional bit | Deletes the sub-object. | Often read as "no change". |
| The optional block order | Wire order. | Bit order is the obvious reading and is wrong. |
| The shared sequence spaces | Skill and attribute messages share counters. | Rarely modelled; the symptom is dropped updates under load. |
