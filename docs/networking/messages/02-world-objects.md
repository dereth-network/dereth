# World objects

The object-queue set: object lifecycle, appearance and physics, plus the containment messages that
tell the client where an item has gone. Sixteen opcodes reach the object dispatcher; the four
movement ones are on the [movement page](05-movement.md) and the twelve others are here.

**Provenance:** from the wire format, which public readers document.

**Readers:** `dereth_protocol::objects`, with `dereth_protocol::types::objdesc`,
`dereth_protocol::types::physicsdesc`, `dereth_protocol::types::weeniedesc` and
`dereth_protocol::types::appraisal` for the payloads. Pinned by the seven tests in
`core/protocol/src/objects.rs` — notably `create_and_update_object_are_byte_identical`,
`is_newer_uses_the_absolute_half_window`, `delete_object_is_six_bytes_and_two_of_padding` and
`the_seven_suppressed_failure_codes_are_exactly_those` — plus the tests in each of the descriptor
modules.

## 1. The two names are back to front

`0xF745` is called "create object" and `0xF7DB` is called "update object". **The two messages are
byte-identical**, and their handlers are the same function with one argument different — but it is
the *create* one that is gentle and the *update* one that is destructive:

- **`0xF745`**: when the client already has the object at the same instance sequence, it **merges**
  the three descriptors into the live object.
- **`0xF7DB`**: the instance-sequence branch is skipped entirely, and the object is **deleted and
  rebuilt**.

Anything the client held on that object — a parent link, a queued message, an interface reference —
survives the first and does not survive the second. Choosing by the name rather than by the behaviour
gets it exactly backwards.

## 2. The instance gate has three outcomes, not two

Every object message carrying a timestamp pack runs the same gate on the pack's **first** value
before looking at anything else. The outcomes are:

| outcome | meaning |
|---|---|
| processed | Normal. |
| old instance | The object was re-created; this message is stale. **Drop it.** |
| error | Bad opcode, or a body shorter than four bytes. The message is dropped. |
| **queued** | The object is not known yet, **or the message's instance sequence is newer than the object's**. Park the message on a placeholder and replay it when the real object arrives. |

**Dropping "future" messages instead of parking them loses objects.** That is the trap: two of the
five statuses mean "not now", and only one of them means "never".

The comparison itself is a 16-bit half-window, and it takes **the absolute difference first**:

```text
newer(a, b) = |b - a| < 0x8000 ? a < b : b < a
```

which is subtly different from the envelope's stamp comparison — that one is not symmetric at exactly
half the period. The client has both, and a rebuild needs both.

## 3. The three descriptors

`0xF745` and `0xF7DB` carry an object id and then three descriptors **in this order**: appearance,
physics, and game data.

### 3.1 Appearance

A version byte, then three counts, then a palette id that is **present only when there are
subpalettes**, then the three change lists:

- a **subpalette** is a packed palette id plus two bytes, an offset and a length, each of which the
  client multiplies by 8 — and a length byte of zero means 256, that is, the whole palette;
- a **texture change** is a part index and two packed texture ids;
- a **part change** is a part index and a packed model id.

The same structure is built from a [clothing table](../../formats/20-clothing-table.md) and applied
to a live object. The packed-id encoding and the ×8 scale are described on the
[palette page](../../formats/13-palette-and-surfaces.md).

### 3.2 Physics

A bitfield and the physics state word, then the gated fields, then **nine 16-bit timestamps and a
two-byte pad**.

**The wire order is not the bit order.** Reading the fields in bit order gives a plausible parse of
the first few and garbage after that. The gated fields, in wire order, are either a raw movement
buffer with an autonomy flag **or** an animation-frame id (the two are mutually exclusive), a
position, four asset ids (motion table, sound table, physics-effect table, setup — plain 32-bit ids,
not packed), a parent link, a child list, scale, friction, elasticity, translucency,
velocity, acceleration, angular velocity, and a default script with its intensity.

The nine timestamps are position, movement, state, vector, teleport, server-controlled move, forced
position, appearance and **instance** — the last being the one the gate reads. Eighteen bytes,
followed by a pad to 4 that is **always two bytes** when the rest is dword-aligned. That pad is the
easiest thing in the family to lose.

### 3.3 Game data

A header word, a name, a class id, an icon, a type and a second bitfield, then some forty optional
fields.

Three things to get right:

- **The class id uses its own packing**, not the protocol's usual packed-id helper: it has no base
  and it masks with `0x7FFF` rather than `0x3FFF`.
- **`0x04000000` means two different things in the two words**: in the first it says an access
  control list follows; in the second it says *a second header dword follows*.
- **Absent and present-but-zero are different.** The client compares the incoming container, wielder
  and location against what it had, so a field that is absent must not read as zero. A codec that
  defaults them silently makes the client think the item moved.

The access-control list for a dwelling has **three shapes**, selected by its first dword: when the
high word is zero, that dword *is* the bitmask and an old-style table follows; a small version gives
version, bitmask, monarch and the old table; a large version gives the same with the modern table.
Everything shipped uses the third, and the client still accepts the other two.

## 4. The rest of the family

| opcode | body | notes |
|---|---|---|
| `0xF746` create player | a player id | Arrives **before** the player's own create, so that create is one of the messages replayed once the player id is known. A second one is ignored. |
| `0xF747` delete object | an id and a `u16`, padded to 4 | Six bytes of content and two of padding; a reader should accept the body without the pad. The `u16` is the **instance** sequence and runs the same three-way gate. **Deleting the player is an error**: the client never deletes itself. |
| `0xF625` appearance event | an id, an appearance descriptor, a timestamp pack | |
| `0xF749` parent event | two ids, a location, a placement, a timestamp pack | Section 5. |
| `0xF74A` pickup event | an id and a timestamp pack | The object **stays in the object table**: it has left the 3-D world but the client still knows it. |
| `0xF74B` set state | an id, a state word, a timestamp pack | Section 6. |
| `0xF750` sound | an id, a sound type, a volume | **No sequence check.** |
| `0xF751` player teleport | a `u16`, padded to 4 | The handler **moves nothing**; see section 6. |
| `0xF754` play script by id | an id and a script id | No sequence check. |
| `0xF755` play script by type | an id, an effect type, an intensity | |
| `0xF6EA` force appearance | an id, **client to server, on the control queue** | Section 7. |

## 5. The parent event

The two ids are **holder first, held object second**, and the distinction matters because the two
lookups are asymmetric:

- the **first** id's object is the one whose instance sequence the gate checks;
- the **second** id's object is the one that gets a parent;
- the **location** key is looked up on the **holder's** setup — see
  [11-setup.md](../../formats/11-setup.md). A miss makes the attachment fail outright rather than
  falling back;
- the **placement** is installed on the **held object's** own parts.

When the holder exists but the item does not, the message is queued on the **item's** id, not the
holder's.

## 6. Teleport takes three messages

The teleport message itself carries only a sequence number and **moves nothing**. The actual move
arrives as an ordinary position event, and **the teleport ends when a set-state message arrives with
the hidden bit clear** on the player's own state. A client that treats the teleport message as the
move, or that never watches for the hidden bit to clear, leaves the player invisible.

## 7. Asking for an appearance

The client can ask the server to resend an object's appearance. It has **three** call sites, and the
recorded traffic is dominated by one of them that is easy to miss: a housekeeping pass that re-asks,
every **twenty seconds**, for every object it is still waiting on. In a recorded corpus every single
instance was that kind — consecutive gaps of twenty seconds to within a millisecond, with no create
for the same object in the ten seconds before.

A server that treats this message as "the client noticed a discrepancy" will draw the wrong
conclusion. Mostly it means "the client is still waiting".

## 8. Containment

These arrive on the UI queue rather than the object queue, and they are what tells the client where
an item went:

| opcode | body |
|---|---|
| `0x0022` server says contain | item, container, slot, and a flag: zero for a plain item list, non-zero for a side pack |
| `0x0023` wear item | item and an equipment location |
| `0x019A` server says move item | an id — **the item leaves your inventory but is not destroyed**; a create usually follows for the world instance |
| `0x0024` server says remove | an id — **the only inventory message on the UI queue that is unordered**. Everything inside the object is queued for destruction too |
| `0x0052` stop viewing contents | an id |
| `0x0196` on view contents | a container id and a list of content profiles |
| `0x00C9` set appraise info | an object id and an appraisal profile |
| `0x00C8` appraise | an id, client to server |
| `0x0195` no longer viewing contents | an id, client to server |
| `0x01CB` appraise done | one dword the client ignores |
| `0x01C7` use done | a failure code; zero means success. The **universal "action finished" acknowledgement**: it also ends spell casts, crafting and salvaging, and a server that omits it leaves the client busy for ever |
| `0x00A0` server says attempt failed | an object id and a failure code. Seven codes do not produce the generic failure text, because the client has already printed a message naming the object: `0x01E`, `0x02B`, `0x3EF`, `0x43E`, `0x46A`, `0x4CE`, `0x4CF` |

The appraisal profile's property-flag bits are **not** the same as the quality system's: the 64-bit
and data-id flags are different values there. Reusing one flag table for both silently misreads every
appraisal.

## 9. Where reimplementations differ

| topic | the client | common reimplementations |
|---|---|---|
| Create versus update | Update is the destructive one. | The names suggest the opposite, and implementations follow the names. |
| The instance gate | Three outcomes; "newer" is parked. | Frequently two outcomes, so future messages are dropped and objects go missing. |
| The descriptor field order | Wire order, not bit order. | The bit order is the obvious reading and is wrong. |
| The two-byte pad after the timestamps | Always emitted. | Easy to lose, and then everything after the descriptor shifts. |
| Absent optional fields | Distinct from zero. | Frequently defaulted. |
| The parent event's two ids | Holder, then held. | Documented the other way round in more than one place. |
| The force-appearance message | Mostly a twenty-second retry. | Read as an error signal. |
