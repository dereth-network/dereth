# Setups (`0x02xxxxxx`)

A **setup** is the description of a multi-part model: an ordered list of
[geometry object](10-gfxobj.md) ids (the *parts*), the collision volumes of the whole object, the
attachment points for held and worn items, one or more static poses selected by a *placement* key, and
the default animation, physics script, motion table, sound table and physics-script table that an
object using this setup starts with.

Every physical object in the world is either a setup id or a bare geometry id promoted to a
throwaway one-part setup.

**Provenance:** from the DAT format, which public readers document.

**Readers:** `dereth_assets::geometry` (`Setup`), with `dereth_assets::hook` for the frame hooks a
placement frame may carry. The whole-container check is `dereth-assets`' `exhaustive_decode`, which
needs the shipped data files and so is not a public-tier test.

## 1. Layout

| field | type | meaning |
|---|---|---|
| id echo | `u32` | Not a categorised type. |
| `flags` | `u32` | Section 1.1. |
| `num_parts` | `u32` | A plain dword. |
| `parts` | `u32 × n` | Geometry ids. |
| `parent_index` | `u32 × n` | Only when `flags & 0x1`. |
| `default_scale` | `f32[3] × n` | Only when `flags & 0x2`. |
| `holding_locations` | table | `u32 count`, then per entry: `u32 key`, `u32 part_index`, 28-byte transform. |
| `connection_points` | table | The identical encoding. **The count is zero in every shipped setup** — but the count word is always there and must be read. |
| `placement_frames` | table | `u32 count`, then per entry: `u32 key`, one pose (section 3). |
| `capsules` | array | `u32 count`, then 20 bytes each. |
| `spheres` | array | `u32 count`, then 16 bytes each. |
| `height` | `f32` | |
| `radius` | `f32` | |
| `step_up_height` | `f32` | |
| `step_down_height` | `f32` | |
| `sorting_sphere` | 16 bytes | Bounding sphere for render sorting and visibility. |
| `selection_sphere` | 16 bytes | Bounding sphere for mouse picking. |
| `lights` | array | `u32 count`, then 48 bytes each (section 4). |
| `default_anim_id` | `u32` | |
| `default_script_id` | `u32` | |
| `default_motion_table_id` | `u32` | |
| `default_sound_table_id` | `u32` | |
| `default_physics_script_table_id` | `u32` | |
| padding | 0–3 | Align to 4. |

**Two field-order traps**, both the reverse of the obvious order, and both silently wrong if guessed:

- The file order is **height, radius, step up, step down** — not step-down before step-up.
- A capsule is stored **low point, radius, height** — not low point, height, radius.

### 1.1 Flags

| bit | meaning |
|---:|---|
| `0x1` | A parent-index array follows the part list. **Changes the layout.** |
| `0x2` | A default-scale array follows. **Changes the layout.** |
| `0x4` | The object may turn to any heading rather than being locked to a terrain-aligned frame. |
| `0x8` | "Has a physics BSP." Advisory only — see below. |

Only the first two bits affect the byte layout; the other two are decoded into fields.

Bit `0x8` is worth a warning: **the runtime never reads it.** It recomputes the answer by scanning the
parts' geometry objects. A rebuild should do the same and treat the stored bit as a hint, because a
setup whose flag disagrees with its parts is not a contradiction the client would ever notice.

### 1.2 `parent_index` and `default_scale`

`parent_index[i]` names the part that part *i* hangs from, and **nothing in the client reads it**.
Part transforms come entirely from the animation frame, which stores an absolute object-space
transform per part. Keep the array for fidelity, but do not build a bone hierarchy out of it and
compose transforms down it: that doubles every rotation.

`default_scale[i]` *is* used — it is copied into each part when the setup is instantiated.

## 2. Holding locations

A holding location says: when something is attached at slot `key`, parent it to part `part_index` of
this setup and place it at the given transform in that part's space.

The transform on disk is 28 bytes — origin `x, y, z` then a quaternion `w, x, y, z`. A quaternion
that does not validate rejects the record rather than being normalised.

Nine keys occur in shipped data: none, right hand, left hand, shield, belt, quiver, a heraldry slot,
and two left-side weapon slots. A tenth (a mouth slot) exists in community enumerations but no
shipped setup uses it. The client carries no names for these keys at all — they are small integers,
and the names in circulation are community vocabulary.

## 3. Placement frames

```text
u32 count
count × { u32 key ; pose }
```

where a pose is

```text
num_parts × 28-byte transform      # one static transform per part
u32 num_frame_hooks
num_frame_hooks × hook             # see 12-animation.md
```

**`num_parts` is not stored in the pose.** It comes from the enclosing setup (or, for an animation,
from the animation's part count). A reader that tries to make the pose self-describing cannot.

In shipped setups a placement pose never carries hooks.

Lookup **falls back to key 0** when the requested key is missing, and key 0 is present in every
shipped setup. This matters: objects routinely request placements like "right hand, combat" on setups
that do not define one, and the client quietly uses the default rather than failing. A reader that
returns nothing produces objects frozen in a T-pose.

Keys in the shipped data run from 0 to just over a thousand, and a handful of them have no name in
the client or in any community enumeration.

## 4. Collision volumes and lights

| field | size | meaning |
|---|---:|---|
| capsule | 20 | Low point (3 floats), radius, height. A vertical capsule for the cheap collision pass. |
| sphere | 16 | Centre (3 floats), radius. |
| `height`, `radius` | 4 each | Overall object dimensions. |
| `step_up_height` | 4 | The largest ledge the object walks up. |
| `step_down_height` | 4 | The largest drop it steps down rather than falling. |

A light is 48 bytes:

| offset | size | field | meaning |
|---:|---:|---|---|
| `+0` | 4 | `key` | The light's key in the setup's light table. **Zero in every shipped light**, since no setup has more than one. |
| `+4` | 28 | `offset` | Position and orientation in object space. |
| `+32` | 4 | `colour` | Packed `0xRRGGBB`, expanded to three floats in `0..1`. |
| `+36` | 4 | `intensity` | |
| `+40` | 4 | `falloff` | |
| `+44` | 4 | `cone_angle` | In practice written from **uninitialised memory** — `0xCD` fill bytes, in the same way as the terminal BSP node's plane. Never validate it. |

Where a setup has lights at all it has exactly one.

## 5. What the runtime does with it

This is the entire skinning model, and it is worth stating because it is so much simpler than a
modern one:

```text
for i in 0 .. min(setup.num_parts, animframe.num_parts):
    part[i].frame = combine(object_frame, animframe.frame[i], scale)
```

No bone hierarchy. No blending. Each part gets an absolute object-space transform straight out of the
animation frame, scaled uniformly. An animation with fewer parts than the setup is legal, and the
extra parts simply keep the transform they had.

A setup's dependency closure is its part ids plus the five default table ids.

## 6. Where reimplementations differ

| topic | the client | common reimplementations |
|---|---|---|
| The trailing alignment | Pads to 4 at the end. | Often stops after the five default ids. Harmless on shipped data, where the record is already a multiple of four, but it desynchronises on anything else. |
| `connection_points` | Always read, always empty. | The same. Worth keeping, since the count word is real. |
| `parent_index` | Read, never used. | The same. |
| The light colour | Converted to three floats immediately. | Often kept as a packed dword. |
| Placement and holding-slot names | None — they are integers. | Named enumerations, including two names that carry spelling mistakes from the community source rather than from the game. |

## 7. A synthetic example

A one-part door: one capsule for collision, a large sorting sphere, no lights, and no default tables.

```text
id echo               0x02000001
flags                 0x0000000C     # free heading + the advisory physics bit; no parent
                                     # array and no scale array, so neither follows
num_parts             0x00000001
parts[0]              0x01000001
holding count         0
connection count      0
placement count       1
  key                 0              # the default
  pose part 0         origin (0,0,0), quaternion (1,0,0,0)
  num_frame_hooks     0
capsule count         1
  low point           (-1.05, 0.0, -1.39)
  radius              0.269
  height              2.776
sphere count          0
height                2.776
radius                0.826
step_up_height        1.12
step_down_height      1.12
sorting sphere        centre (-0.525, 0.0, -0.002), radius 1.913
selection sphere      centre (0, 0, 1.388), radius 1.388
light count           0
five default ids      all zero
```

Nothing above is taken from a shipped file. Note the ordering of the four scalars and of the capsule's
three numbers: those are the two traps of section 1 in concrete form.
