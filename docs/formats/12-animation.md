# Animations (`0x03xxxxxx`) and frame hooks

An **animation** is a fixed-rate array of key frames. Each frame holds one absolute object-space
transform **per part** of the [setup](11-setup.md) it is played on, plus an optional list of **hooks**
— side effects fired when playback crosses that frame: play a sound, spawn a particle emitter, swap a
part's mesh, fade a part's translucency. An animation may also carry a parallel array of *position
frames*: one whole-object transform per frame, for animations that move the object itself.

There is no interpolation model in the file, no bone hierarchy, and **no timing**. The frame rate
comes from the [motion table](19-motion-table.md) entry that references the animation, and the client
steps frame by frame.

**Provenance:** from the DAT format, which public readers document.

**Readers:** `dereth_assets::geometry` (`Animation`) and `dereth_assets::hook` (every hook type). Pinned
by `a_noop_hook_is_eight_bytes_and_needs_no_padding`, `replace_object_pads_to_four` and
`sound_tweaked_is_probability_then_priority` in `core/assets/src/hook.rs`.

## 1. Layout

| field | type | meaning |
|---|---|---|
| id echo | `u32` | |
| `flags` | `u32` | Section 1.1. |
| `num_parts` | `u32` | Parts per frame. Must match the setup this plays on. |
| `num_frames` | `u32` | |
| `position_frames` | 28 × `num_frames` | Only when `flags & 0x1`. |
| `part_frames` | var | `num_frames` entries, each `num_parts` transforms followed by a hook list. |

There is no trailing alignment: every element is a multiple of four bytes and hooks pad themselves.

### 1.1 Flags

| bit | meaning |
|---:|---|
| `0x1` | The position-frame array is present. **Changes the layout.** |
| `0x2` | At least one frame has hooks. |

Bit 1 does **not** change the layout — every frame always carries its hook count — and nothing in the
client reads it back. It is nevertheless exactly true of the shipped data: it is set if and only if
the animation has at least one hook. A reader can ignore it; a writer should compute it.

### 1.2 The transform

28 bytes: origin `x, y, z`, then a quaternion in the order **`w, x, y, z`**.

The quaternion is validated but **not normalised**, and a degenerate one is kept rather than fixed.
`position_frames[i]` is the whole-object transform for frame *i*; `part_frames[i][j]` is part *j*'s
transform for frame *i*.

### 1.3 A frame

```text
num_parts × 28-byte transform
u32 num_hooks
num_hooks × hook
```

**`num_parts` is a parameter, not a stored field.** A frame is unparseable on its own; the count comes
from the enclosing animation, or from the setup when the frame is a placement pose.

Hooks end up in the client's list in **file order**.

## 2. Hooks

| offset | size | field |
|---:|---:|---|
| `+0` | 4 | `hook_type` |
| `+4` | 4 | `direction` |
| `+8` | var | Per-type payload. |
| … | 0–3 | Pad to 4. |

**The trailing pad is the thing to get right.** Every payload except one is already a multiple of
four, so the padding is invisible — until the one hook with a sub-dword field, which is the hook that
proves the rule.

An unrecognised type word **reads no payload at all**: the reader falls through to the alignment step
and returns nothing. That is the client's entire error handling here, and it means a corrupt type word
turns the rest of the frame's hook list into garbage rather than an error. Type 4 is in this position:
it is a defined type with no reader, so it behaves exactly like an unknown one.

### 2.1 Direction

| value | meaning |
|---:|---|
| 0 | Fire whichever way the animation is playing. The overwhelming majority. |
| 1 | Fire only when playing forwards. |
| −1 | Fire only when playing in reverse. |
| −2 | The in-memory default of a hook before it is read. Never written to a file. |

Animations are played backwards by motion links and by reverse motion commands, which is why every
hook carries a direction. Playing in reverse must skip the forward-only hooks, fire the backward-only
ones, and fire the both-ways ones either way.

### 2.2 The catalogue

| type | payload | bytes |
|---:|---|---:|
| 0, 4, 17 | none | 0 |
| 1 | A wave id. | 4 |
| 2 | A sound type. | 4 |
| 3 | An attack cone (section 2.3). | 28 |
| 5 | A part index as a **`u8`**, then a geometry id packed relative to the geometry base. | 3 or 5 |
| 6 | An "ethereal" flag. | 4 |
| 7, 9, 11 | A part index and a `start`, `end`, `time` ramp (translucency, luminosity, diffuse). | 16 |
| 8, 10, 20 | The same ramp, whole-object. | 12 |
| 12 | Scale: `end`, `time`. | 8 |
| 13, 26 | An emitter-info id, a part index, a 28-byte offset transform and an emitter id. | 40 |
| 14, 15 | An emitter id. | 4 |
| 16 | A no-draw flag. | 4 |
| 18 | A part index. | 4 |
| 19 | A physics-script id and a pause. | 8 |
| 21 | A wave id, then **probability, priority, volume**. | 16 |
| 22 | A rotation axis. | 12 |
| 23 | `u`, `v` texture-scroll speeds. | 8 |
| 24 | A part index and `u`, `v` speeds. | 12 |
| 25 | A lights-on flag. | 4 |

Types grouped on one row share a payload shape; that grouping is a convenience of this table, not a
format rule — each type still has its own meaning.

**Field-order trap in type 21:** the payload is `probability` and then `priority`. Both are floats, so
swapping them does not desynchronise the cursor and cannot be caught by a trailing-byte check — which
is why this one is worth checking against the data rather than against a header. The shipped data
settles it: across all 541 of these hooks the first float never exceeds 1.0 and takes nine distinct
values between 0.1 and 1.0, while the second reaches 3.0. A probability cannot be 3.

### 2.3 Sub-structures

The attack cone, 28 bytes — the swept volume of a melee swing at this frame:

| offset | size | field |
|---:|---:|---|
| `+0` | 4 | part index |
| `+4` | 8 | left point (x, y) |
| `+12` | 8 | right point (x, y) |
| `+20` | 4 | radius |
| `+24` | 4 | height |

The part-change payload, 3 or 5 bytes: a `u8` part index, then a geometry id in the
relative-to-a-known-type encoding of
[03-serialisation-primitives.md](03-serialisation-primitives.md).

### 2.4 What the shipped data actually uses

Hooks divide sharply between animations and [physics scripts](18-physics-scripts.md), which share the
format. Sound-table hooks and particle creation dominate both; attack cones and translucency ramps
appear only in animations; texture-scroll and scale hooks appear only in physics scripts. Eight of the
27 types are implemented by the client and never appear anywhere. **Setup placement poses carry no
hooks at all.**

One side effect worth knowing: reading a sound hook creates its sound immediately, so merely loading
an animation preloads its waves. A rebuild that defers sound loading should make sure the first
playback does not stall.

## 3. Where reimplementations differ

| topic | the client | common reimplementations |
|---|---|---|
| Padding after a hook | Always pads to 4. | Often not padded. Invisible for every hook except the part-change one. |
| The part-change index | One byte. | Often a `u16` masked to a byte. Combined with the missing pad, the two agree **only** when the packed id takes its short form. With the long form they differ by two bytes and everything after desynchronises. Both shipped instances use the short form, so the bug never fires on shipped data. |
| Type 21's two floats | Probability, then priority. | Frequently the other way round. Same bytes, swapped meanings, no error. |
| An unknown hook type | Aligns and stops. | Logs and continues from the same position. Both are equally lost on corrupt data. |
| Bit 1 of the flags | Decoded, never read. | Usually not modelled. |

## 4. A synthetic example

A two-frame animation of a one-part object that swaps its part-0 mesh going forwards and swaps it
back going in reverse — the classic open/close of a two-state prop.

```text
id echo            0x03000001
flags              0x00000002        # hooks, no position frames
num_parts          0x00000001
num_frames         0x00000002

frame 0 part 0     origin (0, 0, 0.04), quaternion (1, 0, 0, 0)
frame 0 num_hooks  2
  hook 0           type 5, direction 1        # forwards only
                   part_index 0x00
                   packed id: u16 with bit 15 clear -> 0x01000000 + 0x0BB4
                   one pad byte                       # 4+4+1+2 = 11 -> 12
  hook 1           type 5, direction -1       # reverse only
                   part_index 0x00
                   packed id -> 0x01000BB5
                   one pad byte

frame 1 part 0     origin (0, 0, 0.04), quaternion (1, 0, 0, 0)
frame 1 num_hooks  0
```

Nothing above is taken from a shipped file. The two pad bytes are the point of the example.
