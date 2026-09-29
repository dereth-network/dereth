# Scenes (`0x12xxxxxx`) and particle emitters (`0x32xxxxxx`)

Two small, unrelated formats that both describe "spawn things with randomness".

A **scene** is the landscape scatter table: a list of records, each naming a [setup](11-setup.md) or
[geometry](10-gfxobj.md) id and the rules for placing copies of it on terrain. The
[region](15-region.md) maps terrain types to scenes, and the landblock loader runs the placement with
a **deterministic hash of the world coordinates**, so every client puts the same tree on the same
square without the server sending anything.

A **particle emitter** is one particle system: an emission rate, a budget, a lifetime, geometry per
particle, three vectors whose meaning depends on the particle type, and the scale and translucency
ramps. Emitters are started by [animation and physics-script hooks](12-animation.md) and by the
region's sky objects.

**Provenance:** from the DAT format, which public readers document; the placement algorithm of section
2 is observed client behaviour.

**Readers:** `dereth_assets::world` (`Scene`, `ParticleEmitterInfo`). The placement algorithm of
section 2 is `dereth_terrain::scenery`, and the per-particle draws of section 3.3 are
`dereth_animation::particles`. The whole-container check is `dereth-assets`' `exhaustive_decode`,
which needs the shipped data files and so is not a public-tier test.

## 1. Scene layout

| field | type |
|---|---|
| id echo | `u32` |
| `num_objects` | `u32` |
| records | 76 × n |

Each record is 76 bytes:

| offset | size | field | meaning |
|---:|---:|---|---|
| `+0` | 4 | `object_id` | A setup or a geometry object. |
| `+4` | 28 | `base_transform` | Origin within the 24 × 24 land cell, then a quaternion. |
| `+32` | 4 | `frequency` | Probability in `[0, 1]` that this record is placed at all on a given cell. |
| `+36` | 4 | `displace_x` | Maximum random displacement; **zero or less means none**. |
| `+40` | 4 | `displace_y` | |
| `+44` | 4 | `min_scale` | |
| `+48` | 4 | `max_scale` | |
| `+52` | 4 | `max_rotation` | Degrees; **zero or less means "use the base transform's rotation"**. |
| `+56` | 4 | `min_slope` | Bound on the terrain normal's z, **inclusive**. |
| `+60` | 4 | `max_slope` | Also inclusive. |
| `+64` | 4 | `align` | 0 = random heading; non-zero = align to the terrain normal. |
| `+68` | 4 | `orient` | Serialised, and **read by nothing**. A few dozen shipped records set it. |
| `+72` | 4 | `server_object` | Non-zero means **the client does not place it** — the server spawns it as a real world object. |

The last field is the one to get right first: placing those records client-side gives every player
doubled objects, one real and one decorative.

## 2. The placement algorithm

This is what has to be reproduced exactly for trees and rocks to appear in the same places as in the
original. It runs only for a full-detail landblock (eight cells a side); lower-detail blocks carry no
scenery at all. It visits every **vertex** of the landblock's 9 × 9 terrain grid — 81 points, not the
64 cells — with global coordinates `gx`, `gy` and that vertex's 16-bit terrain word `T`:

```text
terrain_type = (T >> 2) & 0x1F
scene_slot   = T >> 11

if scene_slot is beyond the terrain type's slot count: skip the cell
n = number of scenes in that slot
if n == 0: skip the cell

if n > 1: pick the scene by a hash of gx and gy (below)
else:     scene 0
if the picked index names no scene:  skip the vertex      # not clamped to scene 0

for k, record in the scene's records:
    if rand01(freq_hash(gx, gy, k)) >= record.frequency: skip
    if record.server_object != 0:                        skip

    pos = place(record, gx, gy, k)          # displacement + a quadrant flip
    pos += the vertex's offset within the landblock (24 units per step)
    if pos is outside [0, 192) on x or y:    skip             # strict bounds
    if pos is on a road:                     skip
    if the cell has a building:              skip
    poly = the terrain polygon under pos
    if there is none:                        skip
    if poly's normal z is outside [min_slope, max_slope]:  skip
    snap pos's z onto the terrain polygon

    frame = align == 0 ? base transform, with a random heading when max_rotation > 0
                       : base transform facing directly downhill
    if the object's sorting sphere, placed by frame, does not clear
       every edge of the landblock by its radius:            skip
    place the object, scaled by scale(record, gx, gy, k)
```

### 2.1 The hash

Every random draw has the same shape: a 32-bit integer expression of `gx`, `gy` and a per-use
constant, **reinterpreted as unsigned** and multiplied by 2⁻³² to give a float in `[0, 1)`. The common
form is

```text
h(gx, gy, m) = gy*0x6C1AC587 - (gy*gx*0x5111BFEF + 0x70892FB7) * m + gx*0xBDE41C43
```

with all arithmetic in 32-bit two's complement. The per-use constant added to the record index `k` is
`0x5B67` for the frequency test, `0xB2CD` for the x displacement, `0x11C0F` for the y displacement and
`0xF697` for the random heading (the heading is that draw times `max_rotation` degrees). The
scene-index draw and the quadrant draw of section 2.2 use different expressions of the same kind.
(`0xBDE41C43` is `−0x421BE3BD` in 32-bit arithmetic.)

**Any deviation moves the world's trees**: a signed conversion instead of an unsigned one, a 64-bit
intermediate that does not wrap, or a different constant. This is one of the few places in the whole
format set where an arithmetic detail is directly visible to a player standing in a field.

### 2.2 Displacement and the quadrant flip

```text
dx, dy = the base transform's x and y
if displace_x > 0: dx += rand01(h(gx, gy, k + 0xB2CD))  * displace_x
if displace_y > 0: dy += rand01(h(gx, gy, k + 0x11C0F)) * displace_y

q = rand01(a hash of gx and gy that does NOT involve k)
q < 0.25 -> ( dx,  dy)
q < 0.50 -> (-dy,  dx)
q < 0.75 -> (-dx, -dy)
else     -> ( dy, -dx)
```

The quadrant draw deliberately does not depend on the record index, so **every object of a scene on
one cell gets the same 90° rotation**. That is what makes a clump of trees look like a placed group
rather than noise.

### 2.3 Scale

```text
if min_scale == max_scale: max_scale
else:                      min_scale * pow(max_scale / min_scale, r)
```

Log-uniform, not linear. With a 1:10 range the linear version puts far too many large objects in.

## 3. Particle emitter layout

172 bytes after the id echo.

| offset | size | field | meaning |
|---:|---:|---|---|
| `0x00` | 4 | id echo | |
| `0x04` | 4 | — | **Read and discarded.** Zero in every shipped file, but it must still be consumed. |
| `0x08` | 4 | `emitter_type` | Section 3.1. |
| `0x0C` | 4 | `particle_type` | Section 3.2. |
| `0x10` | 4 | `gfx_object_id` | Geometry drawn per particle. |
| `0x14` | 4 | `hw_gfx_object_id` | The hardware path's variant. |
| `0x18` | 8 | `birthrate` | **A double.** Seconds per particle, or metres per particle. |
| `0x20` | 4 | `max_particles` | A hard cap on live particles. |
| `0x24` | 4 | `initial_particles` | Emitted immediately on start. |
| `0x28` | 4 | `total_particles` | Total the emitter will ever emit; 0 means unlimited. |
| `0x2C` | 8 | `total_seconds` | **A double.** Emitter lifetime; 0 means forever. |
| `0x34` | 8 | `lifespan` | **A double.** |
| `0x3C` | 8 | `lifespan_rand` | **A double.** |
| `0x44` | 12 | `offset_dir` | Direction of the spawn offset from the emitter origin. |
| `0x50` | 8 | `min_offset`, `max_offset` | The magnitude is uniform between them. |
| `0x58` | 12 | `a` | First per-type vector, usually velocity. |
| `0x64` | 8 | `min_a`, `max_a` | The scale of `a` is uniform between them. |
| `0x6C` | 12 | `b` | Second per-type vector, usually acceleration. |
| `0x78` | 8 | `min_b`, `max_b` | |
| `0x80` | 12 | `c` | Third per-type vector. |
| `0x8C` | 8 | `min_c`, `max_c` | |
| `0x94` | 12 | `start_scale`, `final_scale`, `scale_rand` | |
| `0xA0` | 12 | `start_trans`, `final_trans`, `trans_rand` | Translucency at birth and death. |
| `0xAC` | 4 | `is_parent_local` | Non-zero: particles follow the emitter's frame. Zero: they are emitted into world space. |

Four fields are **8-byte doubles** in an otherwise 4-byte record, and **no padding is inserted for
them**: `birthrate` happens to sit at an 8-aligned offset, but `total_seconds`, `lifespan` and
`lifespan_rand` start at `0x2C`, `0x34` and `0x3C`. A reader that aligns doubles to 8 shifts everything
from `total_seconds` onwards, and one that assumes everything is a float reads garbage from the
birthrate onwards.

The sorting sphere is computed from the extremes of the three ranges and the lifespan. It is not in
the file.

### 3.1 Emitter type

| value | meaning |
|---:|---|
| 0 | Unknown. |
| 1 | Emit one particle every `birthrate` **seconds**. |
| 2 | Emit one particle every `birthrate` **metres the emitter travels**. |

The type is tested as bit flags. The per-metre test compares the **squared** distance travelled
since the last emission against `birthrate²`. Emission also requires the live count to be under
`max_particles` and, when `total_particles` is non-zero, the emitted total to be under it. **A
per-metre emitter whose owner is standing still emits nothing** — that is correct behaviour, not a bug
to fix.

### 3.2 Particle type

| value | what `a`, `b`, `c` mean |
|---:|---|
| 0 | Unknown. |
| 1 | None — the particle stays where it was born. |
| 2 | `a` = velocity in the emitter's local frame. |
| 3 | `a` = local velocity, `b` = global acceleration. |
| 4 | As 3, plus `c` = global rotation rate. |
| 5 | A swarm or orbit parameterised by all three. |
| 6 | An outward burst; `a` scales the speed. |
| 7 | An inward collapse. |
| 8 | `a` = local velocity, `b` = local acceleration. |
| 9 | As 8, plus `c` = local rotation. |
| 10 | `a` = global velocity, `b` = global acceleration. |
| 11 | As 10, plus `c` = global rotation. |
| 12 | `a` = velocity in world space. |

The per-type integrator equations are not stated here, because they are inferred from the type names
and the accessor helpers rather than established. The local/global distinction is the part that
matters for a reader.

### 3.3 The draws

Each range is sampled once per particle, at birth (`r` is a fresh uniform draw in `[0, 1]`, `s` one
in `[-1, 1]`):

```text
offset      = a random direction perpendicular to offset_dir * lerp(min_offset, max_offset, r)
              # zero when the random vector happens to be parallel to offset_dir
a           = a * lerp(min_a, max_a, r)            # and likewise b and c
lifespan    = max(lifespan + lifespan_rand * s, 0)
start_scale = start_scale + scale_rand * s         # clamped to [0.1, 10.0]
final_scale = final_scale + scale_rand * s         # clamped to [0.1, 10.0]
start_trans = start_trans + trans_rand * s         # clamped to [0, 1]
final_trans = final_trans + trans_rand * s         # clamped to [0, 1]
```

The draws come from one shared random stream, so their **order** is observable; the random
direction's three components are drawn z, y, x.

**Both scale draws are clamped to `[0.1, 10.0]`**, which matters for emitters with a large random
spread.

## 4. Where reimplementations differ

| topic | the client | common reimplementations |
|---|---|---|
| The placement algorithm | As above. | Absent by design — it is client-side. Servers use the server-object flag to know which scene records they must spawn. |
| Record field signedness | The three trailing flags are signed. | Often unsigned. No practical difference. |
| The discarded emitter dword | Consumed. | Consumed, under a name meaning "unknown". |
| The `a`/`b`/`c` semantics | From the client's own motion code. | Not modelled — no reimplementation of the reader has an integrator. |

## 5. A synthetic example

A trailing dust puff: one particle per 10 cm of movement, up to 20 alive, each launched up and forward
at 0.1–0.2 m/s under −2 m/s² gravity, shrinking from 0.5 to 0.1 while fading in over one second.

```text
id echo             0x32000001
discarded           0x00000000
emitter_type        2                 # per metre
particle_type       3                 # local velocity, global acceleration
gfx_object_id       0x01000001
hw_gfx_object_id    0x01000002
birthrate           0.1               # double
max_particles       20
initial_particles   0
total_particles     0                 # unlimited
total_seconds       10.0              # double
lifespan            1.0               # double
lifespan_rand       0.0               # double
offset_dir          (0, 1, 0)
min/max offset      0.0 / 0.1
a                   (0, 1, 1)         # local velocity
min/max a           0.1 / 0.2
b                   (0, 0, -2)        # gravity
min/max b           1.0 / 1.0
c                   (0, 0, 0)
min/max c           1.0 / 1.0
scales              start 0.5, final 0.1, rand 0.0
translucency        start 0.0, final 1.0, rand 0.0
is_parent_local     0
```

Nothing above is taken from a shipped file.
