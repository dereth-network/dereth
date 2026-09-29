# Combat and magic

The combat set, plus the casting and spell-bar actions. The enchantment and spellbook messages
belong to [the qualities family](03-qualities-and-updates.md), which owns them.

**Provenance:** from the wire format, which public readers document.

**Readers:** `dereth_protocol::combat`. Pinned by the nine tests in
`core/protocol/src/combat.rs` — notably `attacker_notification_percent_is_a_double`,
`reading_percent_as_a_float_would_leave_a_trailing_dword`,
`the_damage_location_table_has_twenty_eight_slots_not_nine`,
`the_two_victim_notifications_are_the_same_message` and
`the_overpower_bit_round_trips_without_being_acted_on`.

## 1. The two hit notifications, and the two things everyone gets wrong

| `0x01B1` attacker notification | "You hurt X." |
|---|---|
| defender name | string |
| damage type | `u32`, a bit mask; the verb table keys off the **lowest set bit** |
| percent | **`f64`** |
| damage | `u32` |
| critical | `u32` |
| attack conditions | **4 bytes** |

`0x01B2`, the defender notification — "X hurts you." — is the same shape with a **damage location**
inserted after the damage.

**Correction 1: the percentage is a double, not a float.** The client advances exactly 0x18 bytes
past the name string for the attacker form and 0x1C for the defender form, and a double is what fits.
Reading it as a float shifts every field after it.

**Correction 2: the attack-conditions field is four bytes on the wire.** The client loads it with a
single 32-bit read, sign-extends it — because its own in-memory value is 64-bit, which is the
likeliest reason every reimplementation widened it — and advances **four**. It is the last field, and
neither handler ever compares the cursor against the end of the message, so **the servers' extra four
bytes are trailing slack the client never reads**.

Two consequences worth spelling out:

- Servers that write eight bytes are not producing a message this client rejects; they are producing
  four bytes of slack. One well-known server writes a 64-bit value; another writes a 32-bit value
  followed by an explicit zero dword it labels as probable alignment. Both agree on the bytes.
- **The original server wrote the eight bytes too.** In the retail packet captures every one of
  41,767 attacker and defender notifications carries the eight-byte field, the high dword zero in
  all of them. So a server should write eight, as the original did.
- **Those four zero bytes are a field, not alignment.** A strict "the body must be fully consumed"
  decoder must read them as the server's high dword when they are there (this project's does), and
  still accept a body that stops after the client's four.

One bit of the conditions word is **not tested by this build** and has no string. Round-trip it; do
not act on it.

**Correction 3: the damage-location table has 28 entries, not nine.** The higher values are reachable
in normal play on non-humanoid creatures. Two values in the range are gaps and render as unknown. The
client lower-cases the name and turns underscores into spaces before displaying it.

## 2. The rest of combat

| opcode | direction | body |
|---|---|---|
| `0x0008` targeted melee attack | C2S | target, attack height, and an attack **power** level (a float) |
| `0x000A` targeted missile attack | C2S | the same layout, but the third field is **accuracy** |
| `0x0053` change combat mode | C2S | a combat mode |
| `0x01B7` cancel attack | C2S | no payload |
| `0x01B8` commence attack | S2C | no payload |
| `0x01A7` attack done | S2C | an error code |
| `0x01B3`, `0x01B4` evasion notifications | S2C | one string each |
| `0x01AC`, `0x01AD` victim notifications | S2C | one string each |
| `0x019E` player death | S2C, **unordered** | a message, the killed id, the killer id |
| `0x01BF` query health | C2S | a target |
| `0x01C0` query health response | S2C | an object and a health fraction in `0..1` (a float) |
| `0x0063` teleport to lifestone | C2S | no payload |
| `0x028D` teleport to marketplace | C2S | no payload |
| `0x0026`, `0x0027` teleport to the PK-lite and PK arenas | C2S | no payload |
| `0x028F` enter PK-lite | C2S | no payload; the server answers with a confirmation request |
| `0x0279` suicide | C2S | no payload; the client shows its own confirmation dialog first |

Five behaviours behind that table:

- **The melee attack is only sent when the client is in melee mode and the target is attackable.**
  The client does its own gating.
- **Commence attack increments the interface's busy count**, so a server that sends it **must**
  eventually send the attack-done message or the interface stays busy.
- **The attack-done code is not unused**, whatever the catalogue says: a non-zero code makes the
  client abort an auto-repeat attack and send a cancel.
- **The two victim notifications reach the same handler with the same arguments.** The client draws
  no distinction between "you died" and "you killed something"; the distinction exists only on the
  server side.
- **The player-death broadcast is printed only when neither id is the local player.** The local
  player's own death comes through the victim notifications.

The combat mode is learned back through an ordinary integer
[quality update](03-qualities-and-updates.md), not through a reply to the change message.

## 3. Casting

| opcode | body |
|---|---|
| `0x0048` cast untargeted spell | a spell id |
| `0x004A` cast targeted spell | **target first, spell second** |
| `0x0224` set desired component level | a component data id and a level |
| `0x0286` spellbook filter | a filter mask |
| `0x01E3` add spell favourite | spell id, slot index, spell bar — 16 bytes with the opcode |
| `0x01E4` remove spell favourite | spell id, spell bar — 12 bytes with the opcode, **no slot index** |

The field order of the targeted cast is the kind of thing that works by accident when both values are
plausible object-sized integers, and then fails on the first spell id that is not.

## 4. Where reimplementations differ

| topic | the client | common reimplementations |
|---|---|---|
| The hit percentage | A double. | The catalogue says float. |
| Attack conditions | Four bytes read; the rest is slack. | Eight bytes written. Harmless, but not the same message. |
| The damage-location table | 28 entries. | Nine. Non-humanoid hits then render as unknown. |
| The attack-done code | Acted on. | Documented as unused. |
| The two victim notifications | One handler. | Two distinct meanings. |
