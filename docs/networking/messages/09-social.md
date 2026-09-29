# Social

Friends, titles, allegiance, fellowship and contracts.

**Provenance:** from the wire format, which public readers document.

**Readers:** `dereth_protocol::social`. Pinned by the fourteen tests in
`core/protocol/src/social.rs` — notably `friends_update_carries_a_list_not_one_friend`,
`update_fellow_begins_with_an_object_id`, `an_allegiance_record_is_laid_out_as_unpack_reads_it`,
`the_two_length_gates_shorten_the_record`, `the_allegiance_hierarchy_is_versioned_in_the_first_dword`,
`logged_in_and_may_passup_experience_are_not_the_same_bit` and
`a_contract_tracker_is_twenty_eight_bytes_with_two_doubles`.

## 1. Three corrections up front

- **`0x0021` friends update carries a *list* of friends, not one.** The client reads a
  count-prefixed list and *then* the update type. The catalogue lists a single record.
- **`0x02C0` update fellow begins with an object id** that the catalogue omits entirely. The client
  reads the id at offset 4, the fellow record from offset 8, and the update type after it.
- **A contract tracker's two times are IEEE doubles, not longs.** Twenty-eight bytes in total.

## 2. Friends and titles

| opcode | direction | body |
|---|---|---|
| `0x0018` add friend | C2S | **by name** |
| `0x0017` remove friend | C2S | **by object id** |
| `0x0025` clear friends | C2S | — |
| `0x0021` friends update | S2C | a list of friends, then an update type: full, added, removed, or a login change |
| `0xF7CD` friends command | C2S, **control queue** | a command word and a player name — **not a game action**: no ordered header and no stamp. The client only ever sends command 0 |
| `0x0029` title table | S2C | **a version dword**, the display title, then the list |
| `0x002B` add or set title | S2C | the new title and whether to display it |
| `0x002C` set display title | C2S | a title |

Adding a friend by **name** and removing one by **id** is asymmetric and deliberate.

**The title table's leading version dword is easy to miss**, and missing it is not subtle: the reader
then takes the list count out of the display title and overruns the message. The version is 1 in
every observed message and the display title is not, which is what makes the mistake visible.

## 3. Allegiance

The hierarchy is the most version-dependent structure in the protocol.

**The first dword is two 16-bit fields**: the low half is the record count and the high half is a
**version**. Everything after it is gated on that version — the client's forward-compatibility scheme
for a structure that grew over thirteen years:

| from version | what is added |
|---:|---|
| 1 | a single officer id, which the client **skips** |
| 2 | the broadcast pools |
| 3 | a two-part message of the day |
| 4 | a chat-room id |
| 6 | an officer hash **replacing** the single id |
| 7 | a bind point, 32 bytes |
| 8 | an allegiance name and a flag |
| 9 | officer titles |
| 10 | a lock flag |
| 11 | an approved vassal |

Version 5 added a ban list that **has no field on the wire**: the bans never travel with the profile.

**The version-gated blocks are not read in version order.** The wire order is: the officer hash (6+)
or the single officer id (1–5), the officer titles (9+), the pools, the message of the day, the chat
room, the bind point, the name and flag, the lock flag, the approved vassal — and then the records.

**The tree is not serialised as a tree.** Record 0 is the monarch, with no prefix; **every later
record is preceded by the object id of its patron.**

### 3.1 A member record: wire order, not struct order

This is the single hardest thing in the family to get right, because a struct-offset listing of the
same record exists and is not the wire order:

```text
u32  id
u32  cp_cached
u32  cp_tithed
u32  bitfield
u8   gender ; u8 heritage ; u16 rank
if bitfield has the packed-level bit:  u32 level
else:                                  set the may-pass-up bit on this record
u16  loyalty ; u16 leadership
if bitfield has the allegiance-age bit:  i32 time_online ; i32 allegiance_age
else:                                    f64 time_online ; allegiance_age = 0
     name (a string)
```

**Both gates change the record's *length*, not merely its meaning.** A reader that ignores them
desynchronises every record after the first — and an update carrying zero members, which is what a
quiet shard mostly sends, never exercises the record loop at all. That combination is how a broken
record layout survives a corpus-based test suite.

Two bits are easy to confuse and produce a plausible, wrong game:

- **"logged in" and "may pass up experience" are different bits.** Mixing them up makes every online
  vassal tithe and every offline one not, which looks like a rule rather than a bug.
- The monarch's pass-up bit is **forced to zero after every unpack**: the monarch can never tithe.

### 3.2 The messages

| opcode | direction | body |
|---|---|---|
| `0x0020` allegiance update | S2C | a rank and a profile — total members, total vassals, the hierarchy |
| `0x0003` update aborted | S2C | — |
| `0x027A` login notification | S2C | a member and whether they are now logged in |
| `0x027C` info response | S2C | a target and a profile. **Not cached** — unlike the update, this one is formatted and printed |
| `0x001D`–`0x0042` | C2S | swear, break, subscribe, the name queries, the officer commands, the lock and gag actions |
| `0x0254`–`0x0256` | C2S | set, query and clear the message of the day |
| `0x0277`, `0x027B`, `0x02A0`–`0x02AB` | C2S | break-and-boot, the info request, the chat boot, bans, officer lists and hometown recall |
| `0x01C8` update done | S2C | sent by the original server; the client has no handler and drops it |

## 4. Fellowship

| opcode | direction | body |
|---|---|---|
| `0x02BE` full update | S2C | members, name, leader, share-experience, even-split, open and locked flags, and the departed list |
| `0x02C0` update fellow | S2C | **an object id**, a fellow record, an update type |
| `0x02BF` disband | S2C | — |
| `0x01C9`, `0x01CA` update done | S2C | **the client does nothing with either**; they exist so that a server can bracket a burst |
| `0x00A2` create | C2S | |
| `0x00A3` quit | C2S: whether to disband; S2C: the member who left | **different bodies per direction** |
| `0x00A4` dismiss | both | the target one way, the dismissed member the other |
| `0x00A5` recruit | C2S | |
| `0x0290` assign new leader | C2S | |
| `0x00A6` update request | C2S | the subscribe toggle for the live vitals feed |
| `0x0291` change openness | C2S | |

**The server writes a lock table after the departed list that this client never reads.** The
client's reader stops at the departed list and ignores whatever follows. Every full update the
original server sent carries that table: a packed hash of lock name to lock record (a dword, a
double age, a timestamp and a renewal count — twenty bytes), with 32 buckets, written in bucket
order; for an unlocked fellowship it is the four-byte empty table (count 0, size 32). So **a reader
that demands the message be fully consumed must read the table**, and one that stops where the
client stops must tolerate it; a body without it should still decode. This project's codec writes
it always and reads it when it is there.

## 5. Contracts

| opcode | body |
|---|---|
| `0x0314` contract tracker table | the whole table, replaced |
| `0x0315` contract tracker | one tracker, plus a delete flag and a display flag |
| `0x0316` abandon contract | a contract id |

## 6. Where reimplementations differ

| topic | the client | common reimplementations |
|---|---|---|
| The friends update | A list. | A single record. |
| The update-fellow message | Starts with an object id. | The id is omitted from the catalogue. |
| The title table | Has a leading version dword. | Frequently omitted, which overruns. |
| The allegiance record | Wire order, with two length gates. | A struct-offset listing exists and is wrong. |
| The logged-in and pass-up bits | Different bits. | Easy to swap; the result looks like a game rule. |
| The fellowship trailer | Ignored. | A strict reader rejects real traffic. |
| Contract times | Doubles. | Often longs. |
| The contract-tracker update (`0x0315`) | Reads 36 bytes and never checks the end. The original server sent 48: twelve more bytes that vary per message and look like uninitialised memory. | 36 bytes. This project sends 48 with the twelve as zeros, and its decoder accepts the tail without interpreting it. |
