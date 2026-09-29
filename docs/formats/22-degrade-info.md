# Degrade records (`0x11xxxxxx`)

A **degrade record** is a [geometry object](10-gfxobj.md)'s level-of-detail chain: an ordered list of
alternative geometry ids, each with a distance band and a billboarding mode. A geometry object points
at one through its degrade id. At draw time the client asks for the level to use, given the viewer
distance and a global quality bias, and swaps the mesh.

The bias is driven by a **frame-rate feedback loop** that slides every threshold at once. That is the
client's automatic performance governor, and reproducing it is what makes a rebuild's draw-call count
behave like the original's.

**Provenance:** the layout is from the DAT format, which public readers document; the selection rule
and the feedback loop of sections 3 and 4 are observed client behaviour.

**Readers:** `dereth_assets::motion` (`GfxObjDegradeInfo`). Pinned by
`gfxobj_info_is_twenty_bytes_and_keeps_the_flt_max_terminator` in
`core/assets/src/motion.rs`.

## 1. Layout

| offset | size | field |
|---:|---:|---|
| `0x00` | 4 | id echo |
| `0x04` | 4 | `num_levels` |
| `0x08` | 20 × n | levels |

Each level is 20 bytes:

| offset | size | field | meaning |
|---:|---:|---|---|
| `+0` | 4 | `geometry_id` | The mesh for this level. **Zero means "draw nothing"** — cull, not "load object 0". |
| `+4` | 4 | `mode` | A billboarding mode; section 2. |
| `+8` | 4 | `min_dist` | Near edge of the band. |
| `+12` | 4 | `ideal_dist` | The distance at which this level is exactly right. |
| `+16` | 4 | `max_dist` | Far edge of the band. |

No alignment and no trailing data.

**Levels are stored nearest-first**, and the last level is almost always a terminator: a zero geometry
id with all three distances at the largest representable float. Most records have two to five levels.

A helper that reports the object's maximum useful distance returns the **second-to-last** level's far
edge when there are more than two levels — that is, it skips the terminator — and the first level's
far edge otherwise.

## 2. The mode

The client has no named enumeration for this field, but the draw path interprets it exactly: it is a
billboarding mode applied to the part's **draw** frame whenever the viewer distance is recomputed.

| value | behaviour |
|---:|---|
| 1 | Nothing — draw with the part's own transform. The overwhelming majority. |
| 2 | Full billboard: the part turns to face the viewer. |
| 3 | Billboard about the x axis. Implemented, and **never used in shipped data**. |
| 4 | Billboard about the y axis. |
| 5 | Billboard about the z axis — the classic foliage card that spins to face you but stays upright. |

The mode applies to the draw transform **only**, never to the physics transform. Keeping those two
separate is the point: a billboarded tree still collides as the mesh it is.

## 3. Choosing a level

```text
if degrades are disabled:      level 0
if a level is forced:          min(forced, num_levels - 1)

d = max(|distance| - 50.0, 0.0)              # the first 50 units are subtracted, not just zeroed
bias = the automatic bias, or a user-supplied one when the automatic loop is off

if bias >= 0:
    for i in 0 .. num_levels:
        threshold = ideal[i] - (ideal[i] - max[i]) * bias
        if d < threshold: level = i; stop
else:
    for i in 0 .. num_levels:
        threshold = ideal[i] + (ideal[i] - min[i]) * bias
        if d < threshold: level = i; stop
otherwise: level = num_levels - 1
```

With a bias of zero both formulas reduce to the ideal distance, so a level is chosen while the viewer
is nearer than its ideal distance. A **positive** bias — a good frame rate — pushes the threshold out
towards the far edge, keeping detailed meshes at longer range; a **negative** bias pulls it in towards
the near edge, degrading earlier. The automatic bias is clamped to `[-1, +1]`, so under the feedback
loop a threshold never leaves its band; a user-supplied bias reaches the selection unclamped.

**The 50-unit offset is a subtraction, and it is load-bearing.** The distance the thresholds are
compared against is the viewer distance *minus* 50, floored at zero — not the raw distance with
everything inside 50 units zeroed. Read as a plain threshold, over a third of the real levels in the
shipped data become unreachable; read as a subtraction, almost none do. The floor at zero also keeps
objects near the camera from flickering between levels as the bias moves.

If a rebuild does not implement the feedback loop, using a bias of zero gives the reference selection
— which is exactly what the client does when automatic degrades are switched off.

## 4. The frame-rate feedback loop

Once per frame the client computes a candidate bias from five overlapping frame-rate response curves,
each a triangular weight around a point derived from three frame-rate constants:

```text
delta = (w_fast*0.10 + w_ok_fast*0.01 + w_middle*0.00
         - w_slow*0.15 - w_ok_slow*0.02) / (sum of the weights)
candidate = clamp(bias + delta, -1.0, +1.0)

if any of the last 29 history entries is within 0.01 of the candidate: do nothing
else: accept it
history[newest] = the current bias
```

Four details decide whether a reimplementation behaves the same way:

- **The history is a sliding 30-frame window of the bias, not a log of accepted changes.** It shifts on
  every call, before anything else is tested. A value the loop passed through is forgotten thirty
  frames later, and a bias it walked away from can be returned to. Treating it as a log of changes
  makes the dead-band remember for ever.
- **The scan compares 29 entries, not 30.** The newest slot is written afterwards on every path and is
  never compared against the candidate that was just accepted.
- **The asymmetry is deliberate.** The "too slow" weights carry −0.15 and −0.02 while the "too fast"
  weights carry only +0.10 and +0.01, so quality is shed about 1.5 times faster than it is restored.
  Saturated — below the slowest curve's support or above the fastest's — the steps are exactly −0.15
  and +0.10.
- **The three frame-rate constants are compiled in, not preferences.** They are 8, 10 and 20, and
  nothing in the client writes them: no registry value, no preference, no console command. A rebuild
  that exposes them and defaults them to something modern gets a completely different curve — at 30
  frames per second the original curve is already saturated at the top, where a 20/30/60 curve would
  call 30 ideal and never move at all.

Two landmarks of the original curve, both exact:

- **The rest band is 9.5 to 12.5 frames per second.** Inside it the only non-zero weight is the middle
  one, whose coefficient is zero, so the change is exactly zero and the loop rests rather than hunting.
- **Below 6 and above 25 frames per second the loop is saturated**, at −0.15 and +0.10 per frame
  respectively.

The thirty-frame window plus the 0.01 dead-band is what stops the level from flickering when the
frame rate sits on a threshold.

## 5. Where reimplementations differ

| topic | the client | common reimplementations |
|---|---|---|
| The mode field | Signed. | Often unsigned. No shipped value is negative. |
| Selection and the feedback loop | As above. | Absent — level of detail is purely client-side, so no server-side reader has any of it. |

## 6. A synthetic example

A four-level chain ending in the usual terminator:

```text
id echo        0x11000001
num_levels     4

level 0        geometry 0x01000001, mode 1, band 10.0 / 25.0 / 50.0
level 1        geometry 0x01000002, mode 1, band 25.0 / 50.0 / 100.0
level 2        geometry 0x01000003, mode 1, band 50.0 / 100.0 / 200.0
level 3        geometry 0,          mode 1, band FLT_MAX / FLT_MAX / FLT_MAX
               # 4 + 4 + 4*20 = 88 bytes
```

With a bias of zero the compared distance is the viewer distance minus 50, so this object draws the
first mesh nearer than 75 units, the second from 75 to 100, the third from 100 to 150, and nothing
beyond 150. Its reported maximum useful distance is 200 — level
2's far edge, not the terminator's.

A geometry object with no degrade id has no chain at all and is always drawn at full detail.

Nothing above is taken from a shipped file.
