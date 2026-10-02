# The region record (`0x13000000`)

There is exactly **one** region record in the shipped data, and it is the most behaviourally dense
file in the whole container. It defines the landblock grid and the 256-entry terrain height table; the
entire calendar and clock — year length, months, week days, sixteen named times of day; the sky, as
twenty weather day groups each with a set of sky objects and a per-time-of-day lighting and fog curve;
the mapping from terrain type to ground textures, alpha masks and detail textures; the ambient sound
tables; the object-scatter tables; and a handful of loose ids.

Landscape rendering, day and night lighting, fog and ambient audio are all driven from here.

**Provenance:** from the DAT format, which public readers document.

**Readers:** `dereth_assets::region` (`Region`). The whole-record check is `dereth-assets`'
`exhaustive_decode` (driven from `core/assets/tests/dat/decode/exhaustive_decode.rs`), which needs
the shipped data files and so is not a public-tier test.

## 1. Primitives

Every field here is 4-byte (or 8-byte for a double) and naturally aligned, so the only padding is
around strings. The string form is the packed-object one of
[03-serialisation-primitives.md](03-serialisation-primitives.md) — a `u16` length with a `0xFFFF`
escape, the characters, and then padding to 4. In the client the string reader itself does not pad;
each caller in this record recomputes the consumed size and aligns. (The Rust cursor's string read
consumes the padding itself, which lands in the same place.)

## 2. Top level

| field | type | meaning |
|---|---|---|
| id echo | `u32` | |
| `region_number` | `u32` | |
| `version` | `u32` | **Must be 3.** |
| `region_name` | string + pad | |
| grid constants | 8 × 4 | Read and **discarded**; section 3. |
| height table | 256 × `f32` | Section 3. |
| calendar | var | Section 4. |
| `parts_mask` | `u32` | Which optional blocks follow. |
| sky | var | Only when `parts_mask & 0x10`. Section 5. |
| ambient sound | var | Only when `parts_mask & 0x1`. Section 6. |
| scene tables | var | Only when `parts_mask & 0x2`. Section 6. |
| terrain | var | **Read unconditionally**; section 7. |
| miscellaneous ids | 24 | Only when `parts_mask & 0x200`. Section 8. |

A version other than 3 is a hard stop with a modal error — the client will not load a region file it
does not recognise.

`parts_mask` bits: `0x1` ambient sound, `0x2` scene tables, `0x4` terrain, `0x8` encounters, `0x10`
sky, `0x200` the miscellaneous block. Two footnotes matter:

- **Bit 3 is set in the shipped file, but no encounter data is ever written or read.** Encounter
  placement lives in the landblock-info records of the cell container instead. The record type also
  declares water, fog, distance-fog and map blocks that no bit in this build reads or writes.
- **The terrain block is gated on write but not on read.** The writer checks bit 2; the reader reads
  the block unconditionally. Since bit 2 is always set the two never disagree on shipped data, but a
  rebuild should follow the reader.

If the miscellaneous bit is clear the client **synthesises** a default block and sets the bit itself.

## 3. Grid constants and the height table

The eight words after the name are the world grid: blocks per side (twice), the length of a terrain
square, cells per landblock, vertices per cell, the maximum object height, the sky height and the road
width.

**The client reads all eight and stores none of them.** The values it uses are compiled in, and the
writer writes those compiled-in values back out. So the file's copy is decorative: editing it changes
nothing. A rebuild should hard-code them too, or a modified region file will change world geometry in
the rebuild and not in the original — which is a difference nobody wants to debug.

From the constants: a landblock is 8 × 8 cells of 24 units, so 192 units square, and the world is 255
× 255 landblocks.

Then 256 floats are read into the height table. **The load stops at the first value outside
`[0, 800]`** and leaves the remaining entries at whatever they were — zeros on a first load. Reproduce
that, or reject the file outright; what a rebuild must not do is keep reading.

A landblock's height byte indexes this table. In the shipped file the first 201 entries are exactly
`2 × i` — the familiar two-unit terrain quantisation — and the rest are non-linear, ending well above
where the linear run would. **Do not substitute a `2 × i` multiplier**; the top of the table is where
the mountains are.

## 4. The calendar

| field | type | meaning |
|---|---|---|
| `zero_time_of_year` | `f64` | |
| `zero_year` | `i32` | |
| `day_length` | `f32` | Real seconds per game day. |
| `days_per_year` | `i32` | |
| `year_spec` | string + pad | The suffix in a formatted date. |
| times of day | `u32` count, then entries | |
| week days | `u32` count, then entries | |
| seasons | `u32` count, then entries | |

The year length is `day_length × days_per_year`, computed rather than stored.

- A **time of day** is `f32 begin` (a fraction of the day in `[0, 1)`), `i32 is_night`, a name and
  padding. Sixteen of them, at sixteenths of a day, alternating a named tide or song with its
  "-and-Half".
- A **week day** is just a name and padding. Six of them.
- A **season** is `i32 begin` (a day of the year), a name and padding. Twelve 30-day months.

The client converts a server timestamp into the current year, day, season, week day and time of day,
plus the `[0, 1)` day fraction the sky uses.

## 5. The sky

| field | type | meaning |
|---|---|---|
| `tick_size` | `f64` | How often the sky is re-evaluated, in game time. |
| `light_tick_size` | `f64` | How often the lighting is. |
| pad to 4 | | |
| day groups | `u32` count, then entries | |

**Evaluating either one every frame makes the result smoother than the original** — the coarse
lighting cadence is visible in the game, and matching it is part of matching the look.

A **day group** is a chance of occurring, a weather name, a list of sky objects and a list of
per-time-of-day entries. The shipped file has twenty groups, all with the *same* chance, so the draw
is effectively uniform over twenty slots and the weather distribution comes from how many slots each
name occupies — sunny weather is commoner than cloudy because it holds more slots, not because it is
weighted.

### 5.1 Sky object (36 bytes)

Until July 2012 a sky object is eight words, without the particle-script id; see
[41](41-older-records-in-the-later-files.md).

| offset | size | field | meaning |
|---:|---:|---|---|
| `+0` | 4 | `begin_time` | Day fraction at which the object appears. |
| `+4` | 4 | `end_time` | And disappears. |
| `+8` | 4 | `begin_angle` | Degrees, at `begin_time`. |
| `+12` | 4 | `end_angle` | At `end_time`; linear in between. |
| `+16` | 8 | `tex_velocity` | Scrolling texture speed, x and y. |
| `+24` | 4 | `gfx_object_id` | The [geometry](10-gfxobj.md) drawn. |
| `+28` | 4 | `script_id` | A [physics script](18-physics-scripts.md) for weather particles; zero for most. |
| `+32` | 4 | `properties` | Bit flags. |

**The file order is not the struct order**: `properties` comes after the two ids on disk and before
them in memory. This kind of reordering recurs throughout the region record and is the main reason to
read it field by field rather than as a block.

An object with `begin_time == end_time == 0` is always present — that is how the sky dome halves are
encoded. The rest are the sun, the moons and the stars, with real spans and sweeping angles.

`properties` takes five distinct values across the shipped objects. Three bits have inferred meanings
— hide under a fog override, a weather or viewer-locked layer, and a first-pass layer — and **a fourth
bit, set on a third of all sky objects, has no known meaning at all.** It is left unexplained here
rather than guessed at.

### 5.2 Time-of-day entry

| offset | size | field |
|---:|---:|---|
| `+0` | 4 | `begin` — the day fraction this entry takes effect at |
| `+4` | 4 | directional light brightness |
| `+8` | 4 | sun heading, degrees |
| `+12` | 4 | sun pitch |
| `+16` | 4 | directional colour, ARGB |
| `+20` | 4 | ambient brightness |
| `+24` | 4 | ambient colour, ARGB |
| `+28` | 4 | fog near distance |
| `+32` | 4 | fog far distance |
| `+36` | 4 | fog colour, ARGB |
| `+40` | 4 | fog mode |
| … | 0–3 | pad |
| … | 4 | replacement count |
| … | 24 × n | replacements (section 5.3) |

Again the file order differs from the struct order: the fog mode is written **last** of the eleven
scalars and sits in the middle in memory.

Lighting and fog are interpolated between consecutive entries by the current day fraction. That
interpolation is what produces the continuous dawn and dusk ramp — the entries themselves are a dozen
key points, not a curve.

### 5.3 Sky-object replacement (24 bytes)

| offset | size | field | meaning |
|---:|---:|---|---|
| `+0` | 4 | `object_index` | Index into the day group's sky objects. |
| `+4` | 4 | `gfx_object_id` | **Zero means "keep the default"**, not "draw nothing". |
| `+8` | 4 | `rotate` | Degrees. |
| `+12` | 4 | `transparent` | **`-1` means "no override"**, not "fully opaque". |
| `+16` | 4 | `luminosity` | |
| `+20` | 4 | `max_bright` | |

Those two sentinel values are the easiest thing on this page to get wrong, and both produce a sky that
looks subtly wrong rather than obviously broken.

## 6. Ambient sound and scene tables

The sound block is a count and that many entries, each a [sound table](21-sound-tables.md) id, a
count, and that many 20-byte records:

| offset | size | field |
|---:|---:|---|
| `+0` | 4 | sound type |
| `+4` | 4 | volume |
| `+8` | 4 | base chance |
| `+12` | 4 | minimum rate |
| `+16` | 4 | maximum rate |

**Whether a sound loops is derived, not stored**: a base chance of zero means continuous. Otherwise
the sound is re-rolled at a random interval between the two rates with that probability.

The scene block is a count and that many scene types, each preceded by a `u32` **index into the sound
block** — with all bits set meaning "no ambient table" — followed by a count and that many
[scene](17-scene-and-particles.md) ids. The scene type's name is not serialised and stays empty.

## 7. Terrain

```text
u32 count
count × {
    string terrain_name + pad
    u32    map_colour                  # ARGB, the colour on the map and radar
    u32    count
    i32[count] scene_type_indices      # into the scene block; -1 for none
}
land surface
```

The shipped file has 32 terrain types, each with 32 scene-type indices — the object-scatter tables per
landblock quadrant, and the mechanism that puts different trees on grassland than on snow.

The land surface begins with a `u32` selecting one of two techniques. **The shipped value selects
texture merging**; the other, palette shifting, is the older technique and no shipped data exercises
it, so its layout is read from the client and not confirmed against bytes.

Texture merging:

| field | type | meaning |
|---|---|---|
| `base_tex_size` | `u32` | |
| corner alpha maps | count + 8 × n | A code and a [surface-texture](13-palette-and-surfaces.md) id. |
| side alpha maps | count + 8 × n | The same shape. |
| road alpha maps | count + 8 × n | The same shape. |
| terrain descriptions | count + 44 × n | Below. |

The alpha-mask textures are in the 8-bit landscape format of [14-textures.md](14-textures.md).

A terrain description is a `u32` terrain type followed by **exactly one** 40-byte texture record,
even though the in-memory structure is a list:

| offset | size | field |
|---:|---:|---|
| `+0` | 4 | ground texture id |
| `+4` | 4 | tiling — repeats across a landblock |
| `+8` | 16 | maximum and minimum vertex brightness and saturation |
| `+24` | 8 | maximum and minimum vertex hue |
| `+32` | 4 | detail tiling |
| `+36` | 4 | detail texture id |

**There is one more terrain description than there are terrain types**: the extra one is the road.

Compositing takes the terrain texture of each of a cell's four corners, blends them through the corner
and side alpha masks — rotated per corner — overlays the road mask, and produces one tile.

## 8. The miscellaneous block

Six dwords: a version, the world-map image id, a test-map id and its size, and the geometry ids used
for an empty cell and for a placeholder creature.

## 9. Where reimplementations differ

| topic | the client | common reimplementations |
|---|---|---|
| The grid constants | Read and discarded; compiled-in values win. | Kept as data. A modified region file then changes world geometry in one and not the other. |
| `version == 3` | Enforced, with a modal error. | Usually not enforced. |
| The looping-sound derivation | `base_chance == 0` means continuous. | Usually not modelled. |
| Palette shifting | Implemented, never exercised. | Usually not implemented, which matches the data. |
| The encounter block | Neither reads nor writes it, despite the flag. | The same. |

## 10. A synthetic example

The head of a minimal region record — the part that shows the string padding and the discarded block:

```text
id echo           0x13000000
region_number     1
version           3
name length       0x0006
name              6 characters
                  # 2 + 6 = 8, already a multiple of 4, so no padding here
grid constants    255, 255, 24.0, 8, 1, 200.0, 1000.0, 5.0     # read, discarded
height table      256 floats: 0.0, 2.0, 4.0, ... then a non-linear tail
calendar          ...
parts_mask        0x0000021F
```

A name one character longer would push two pad bytes in before the grid constants, and everything
after would move. That is the whole reason the string rule is worth stating twice.

Nothing above is taken from a shipped file.
