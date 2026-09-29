# Communication

Chat, tells, emotes, system text, channels, the squelch database and the confirmation dialogs.

**Provenance:** from the wire format, which public readers document.

**Readers:** `dereth_protocol::comms`. Pinned by the seven tests in
`core/protocol/src/comms.rs` — notably
`channel_broadcast_has_a_different_body_in_each_direction`,
`speech_and_emote_order_their_fields_differently`, `squelch_info_is_not_in_struct_order`,
`squelch_all_requires_every_bit` and `only_seven_confirmation_types_are_handled`.

## 1. Speech and emotes order their fields differently

This is the trap of the family, and it is entirely arbitrary:

- **In the speech messages the string comes first** and the object id after it.
- **In the emote messages the object id comes first.**

| opcode | fields |
|---|---|
| `0x02BB` hear speech | message, sender name, sender id, text type |
| `0x02BC` hear ranged speech | the same, plus a range float **before** the type |
| `0x02BD` hear direct speech (a tell) | message, sender name, sender id, target id, text type, and **a fourth dword the client reads and discards** |
| `0x01E0` hear emote | sender id, sender name, text |
| `0x01E2` hear soul emote | sender id, sender name, text |

Both shapes parse without error against the other, because a string and an id are both plausible
leading fields — the symptom is a chat line attributed to the wrong person.

The tell's trailing flags word is still on the wire and is still worth decoding; the client simply
has no use for it.

## 2. Sending

| opcode | body |
|---|---|
| `0x0015` talk | a string |
| `0x01DF` emote | a string |
| `0x01E1` soul emote | a string; the pose table |
| `0x0032` talk direct | **message first, target id second** |
| `0x005D` talk direct by name | message first, then the name |
| `0x000F` set away mode | a dword |
| `0x0010` set away message | a string |
| `0x0145`, `0x0146` add to / remove from channel | a dword |
| `0x0147` channel broadcast | **channel and message only** |
| `0x0148` channel list request | which channel |
| `0x0149` channel index request | no payload |

In the other direction `0x0147` carries channel, **sender name**, message; `0x0148` and `0x0149`
answer with a packed list of names.

**A correction on the channel broadcast**: the catalogue gives it a sender-name field. That is the
*server-to-client* shape. The client writes only the channel and the message; **the server adds the
sender's name.** The two directions are different bodies under one opcode.

## 3. System text

| opcode | body |
|---|---|
| `0xF7E0` textbox string | text and a log type |
| `0x02EB` transient string | a string |
| `0x0004` pop-up string | a string |
| `0x0317` | a string, shown exactly as the transient string is (the catalogue has no name for it) |
| `0x0318` | a string, shown exactly as the pop-up string is (likewise unnamed) |
| `0x028A` error | **the numeric code only** |
| `0x028B` error with a substitution | the code plus the string to substitute |

The English text for an error code is not hard-coded in the client: it lives in the localised string
table in the language data file (see [the formats pages](../../formats/README.md)), and the proper way
to resolve it is through that table.

## 4. Squelch

The server replaces the client's whole squelch state in one message (`0x01F4`). Three fields, in order: a hash of
squelched **account** names, a hash of squelched **characters**, and the global filter state.

The account hash is unpacked with **duplicate keys discarded rather than fatal**, so a server that
sends the same name twice does not lose the whole message.

A squelch entry's wire order is **the message mask, then the name, then the zone flag** — which is
**not** the struct order, where the zone flag sits in the middle.

The message mask is a variable-length integer: a count of 32-bit limbs and then that many, used as a
128-bit vector with one bit per message type. **"Squelch all" is not a flag**: type 1 is true only
when **every** bit from 0 to 127 is set. A reimplementation that special-cases a single bit for "all"
reports the wrong state whenever a player has squelched most but not all types.

The three modification messages are: by character (`0x0058`: a flag, an id, a name and a message
type), by account (`0x0059`: a flag and a name), and global (`0x005B`: a flag and a message type).

## 5. Confirmation dialogs

The request (`0x0274`) carries a type, a context and the dialog text; the response (`0x0275`) is
type, context and a yes-or-no, sixteen bytes with the opcode. `0x0276` is the server cancelling a
timed-out confirmation: type and context. **The client handles only seven confirmation types**; a server that invents an eighth gets no
dialog and no response.

## 6. Chat rooms

One message (`0x0295`) binds the external chat service's room ids to the game's channels: ten dwords, in a fixed
order — allegiance, general, trade, looking-for-group, roleplay, one faction room, and four society
rooms.

The external chat message itself (`0xF7DE`) arrives on the **logon queue**, whose consumer discards
every other opcode, and the client hands the whole body to a separate chat library without reading a
field. This client ports the outgoing by-room request and the two recognised incoming callback forms,
and keeps every other form losslessly opaque.

## 7. Where reimplementations differ

| topic | the client | common reimplementations |
|---|---|---|
| Speech versus emote field order | String first, id first, respectively. | Easy to get uniform, and it parses either way. |
| The channel broadcast | Different bodies per direction. | The catalogue gives the server's shape for both. |
| "Squelch all" | Every one of 128 bits. | Often a single flag. |
| The squelch entry's field order | Not the struct order. | Frequently the struct order. |
| Duplicate account keys | Discarded. | Frequently fatal to the whole message. |
| Confirmation types | Seven handled. | More are defined than are handled. |
