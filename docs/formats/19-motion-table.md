# Motion tables (`0x09xxxxxx`)

A **motion table** is the state machine that turns a motion command — "run forward", "swing high",
"ready" — plus a stance — "sword and shield", "non-combat" — into a list of
[animations](12-animation.md) to play, with frame ranges, frame rates, a linear velocity and an
angular velocity.

It has four parts: a default stance, a per-stance default motion, **cycles** (looping animations that
define a state), **modifiers** (animations layered on top of a cycle) and **links** (transition
animations played when moving from one motion to another). All four are keyed by a packed
`(stance, motion)` pair.

A [setup](11-setup.md) names a default motion table. **The server sends bare motion commands on the
wire** and the client resolves them here, which makes this table the join between the
[movement messages](../networking/messages/05-movement.md) and what is actually drawn.

**Provenance:** from the DAT format, which public readers document.

**Readers:** `dereth_assets::motion` (`MotionTable`). Pinned by
`motion_data_header_is_three_bytes_plus_one_pad` in `core/assets/src/motion.rs`.

## 1. Layout

| field | type |
|---|---|
| id echo | `u32` |
| `default_stance` | `u32` |
| `num_stance_defaults` | `u32` |
| stance defaults | 8 × n — `{ u32 stance ; u32 motion }` |
| `num_cycles` | `u32` |
| cycles | var — each a motion record, key first |
| `num_modifiers` | `u32` |
| modifiers | var — the same |
| `num_links` | `u32` |
| link groups | var — `{ u32 outer_key ; u32 count ; motion record × count }` |

There is no trailing alignment; the motion records align themselves.

The reader bounds-checks as it goes and rewinds on underflow rather than reading past the end.

## 2. The motion record

| offset | size | field | meaning |
|---:|---:|---|---|
| `+0` | 4 | key | The packed key. Part of the record, not of the enclosing list. |
| `+4` | 1 | `num_anims` | |
| `+5` | 1 | `flags` | Behaviour bits; section 2.1. |
| `+6` | 1 | `present` | **Bit 0: a velocity vector follows the animations. Bit 1: an angular velocity follows.** |
| `+7` | 1 | pad | |
| `+8` | 16 × n | animations | Section 2.2. |
| … | 12 | `velocity` | Only when `present & 1`. |
| … | 12 | `omega` | Only when `present & 2`. |

**The header is three bytes plus one pad, and the presence mask is the third byte.** This is the
single most consequential detail on the page: taking the presence mask from byte `+5` instead of byte
`+6` parses most shipped tables and fails on well over half of them, with no error until the cursor
walks off the end. The distinction is easy to miss because the third byte is not an independent field
— a writer recomputes it from whether the two vectors are non-zero.

The reader names the two header bytes `bitfield` (`+5`, the behaviour flags) and `flags` (`+6`, the
presence mask).

A record with **zero** animations is valid: it is a "change state and do nothing" entry, and over a
thousand exist.

### 2.1 The behaviour flags

| bit | meaning |
|---:|---|
| `0x1` | Starting this cycle **clears all active modifiers**. |
| `0x2` | This motion is only allowed when the object's current motion is the stance's default. |

The second bit is how "you cannot draw a bow while sitting" is expressed — the check is against the
stance default, not against a list of forbidden combinations.

The first bit has a partner behaviour: after a cycle change the client **re-applies** the modifiers
that were active, by snapshotting the list, removing them one at a time and re-issuing each. Skipping
that leaves stale modifiers layered over the new cycle.

### 2.2 The animation record (16 bytes)

| offset | size | field | meaning |
|---:|---:|---|---|
| `+0` | 4 | `animation_id` | |
| `+4` | 4 | `low_frame` | First frame to play. |
| `+8` | 4 | `high_frame` | Last frame. **`-1` means "to the end of the animation"** — not frame `0xFFFFFFFF`. |
| `+12` | 4 | `framerate` | Frames per second. **Negative plays the animation in reverse**, which also flips which [frame hooks](12-animation.md) fire. |

## 3. The key

```text
key = ((stance & 0xFFFF) << 16) | (motion & 0xFFFFFF)
```

A stance is a motion command with its top bit set, and the shift keeps only the low sixteen bits, so a
stance contributes a small ordinal to the top half. An action command contributes its own ordinal to
the bottom. The two halves formally overlap in bits 16–23 and never collide in practice, because every
shipped ordinal is small.

| table | keyed by |
|---|---|
| stance defaults | the **raw** stance value, unpacked |
| cycles | the packed key |
| modifiers | the packed key, **with a fallback lookup of the motion ordinal alone** |
| links | outer: the packed key of the *from* motion; inner: the **full** destination command |

The stance-defaults table is the one keyed by the raw value. Everything else uses the packed form, and
mixing the two is the second-easiest mistake here.

## 4. Transitions

```text
get_link(stance, from, from_speed, to, to_speed):
    reverse = to_speed < 0 or from_speed < 0
    if reverse: group = links[pack(stance, to)];   inner = from
    else:       group = links[pack(stance, from)]; inner = to
    if the group has that inner key: use it

    # fallbacks
    if reverse: group = links[pack(stance, from)]; inner = the stance's default motion
    else:       group = links[stance << 16]        # the "entering this stance" group
                inner = to
```

Two things have to be reproduced or transitions silently go missing and animations pop:

- **The reverse case swaps the roles of `from` and `to`**, so one stored transition serves both
  directions when played backwards. That is what the negative frame rate is for.
- **The group whose key has a zero motion ordinal is the "entering this stance from nothing" group**,
  and it is present in shipped data.

## 5. Modifiers

A motion command with bit `0x20000000` set is treated as a **modifier** rather than a state change: it
is looked up in the modifier table — first with the stance prefix, then bare — and added to the
object's modifier list without disturbing the current cycle.

That bit, and the top bit meaning "this is a stance", are the two bits of a motion command that the
client's own code tests. The rest of the top byte carries further structure — there are distinct
values for held states and for translational motions — but the client has no named enumeration for
any of it, and the names in community use come from elsewhere. They are not reproduced here.

## 6. Where reimplementations differ

| topic | the client | common reimplementations |
|---|---|---|
| The motion mask in the key | 24 bits. | Frequently 20 bits. Identical for every shipped ordinal, all of which are small, but the wider mask is the one the client uses. |
| Duplicate outer link keys | Chained at the head of their bucket, so a lookup finds the entry listed **last**. | A map type that throws. No shipped table has duplicates. |
| The behaviour flags | Drive the state machine. | Not modelled — a reader with no state machine has no use for them. |
| The three header bytes | As above. | The same split, under different names. |

## 7. A synthetic example

The smallest useful table: a single idle loop for a static prop.

```text
id echo               0x09000001
default_stance        0x8000003D
num_stance_defaults   1
  stance              0x8000003D          # raw, not packed
  default motion      0x41000003
num_cycles            1
  key                 0x003D0003          # (0x3D << 16) | 0x03
  num_anims           1
  flags               0
  present             0                   # no velocity, no omega
  pad                 1 byte
  animation_id        0x03000001
  low_frame           0
  high_frame          -1                  # to the end
  framerate           30.0
num_modifiers         0
num_links             0
```

Fifty-six bytes. A player-character table by contrast runs to tens of thousands of bytes, with
hundreds of cycles and hundreds of link groups — the format is the same, only the counts change.

Nothing above is taken from a shipped file.
