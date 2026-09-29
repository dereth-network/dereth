# Palettes, palette sets, surfaces and the appearance record

The material layer. A **palette** (`0x04`) is a 2048-entry colour table; a **palette set** (`0x0F`) is
a list of palettes selected by a normalised shade; a **surface** (`0x08`) is what a
[geometry](10-gfxobj.md) polygon actually paints with — either a flat colour or a texture plus a
palette, with translucency, luminosity, diffuse and a bag of blend flags; a **surface texture**
(`0x05`) is the indirection between a surface and the one or two [images](14-textures.md) that back
it.

The **appearance record** at the end of this page is not a container type at all. It is the packed
"how this object differs from its default look" structure, and it appears both inside
[clothing](20-clothing-table.md) results and on the wire, which makes it the one structure shared
between this section and the [message catalogue](../networking/messages/README.md) (its wire use is in
[the world-object messages](../networking/messages/02-world-objects.md#3-the-three-descriptors)).

**Provenance:** from the DAT format, which public readers document.

**Readers:** `dereth_assets::material` (`Palette`, `PaletteSet`, `Surface`, `SurfaceTexture`). Pinned by
`a_retail_palette_is_exactly_8200_bytes` and `a_surface_starts_with_its_type_not_an_id` in
`core/assets/src/material.rs`. The appearance record's wire form is read and written by
`dereth_protocol::types::objdesc`, and the copy embedded in the character-generation table by
`dereth_assets::tables`.

## 1. Palette (`0x04xxxxxx`)

| field | type | meaning |
|---|---|---|
| id echo | `u32` | |
| `num_colors` | `u32` | |
| `colors` | `u32 × n` | `0xAARRGGBB`. |

**Every shipped palette has exactly 2048 entries**, so a palette file is exactly 8200 bytes. That is
not a coincidence, and the number 2048 explains three other things:

- If a palette has 256 entries, the client expands it to 2048 after loading by replicating each entry
  **eight** times. Nothing shipped needs this, but the ratio is the reason for the rest.
- A texture stores **16-bit** indices straight into the 2048-entry table.
- A subpalette replaces a run whose offset and length are both a byte **multiplied by 8** — so
  subpalette ranges have 8-entry granularity. Clothing palette templates use the same units.

Modifying a palette copies an **absolute index range** from another palette: destination index *i*
takes source index *i*, not source index *i − offset*. Both palettes are therefore expected to be the
same size and the copy is index-aligned. A modification is refused if the range runs past the end, or
if the palette is still the cache's shared copy — the caller has to take a private copy first.

## 2. Palette set (`0x0Fxxxxxx`)

| field | type |
|---|---|
| id echo | `u32` |
| `num_palettes` | `u32` |
| `palette_ids` | `u32 × n` |

This is the one type in this group whose reader bounds-checks twice against the remaining buffer and
rewinds on failure. Most palette sets hold a single palette; the largest holds thirteen.

The selection rule takes a **shade** in `[0, 1]`, which the server supplies as a character or clothing
property:

```text
if num_palettes == 0 or shade < 0.0 or shade > 1.0:  no id
index = (int)((num_palettes - 0.000001) * shade)     # truncating
return palette_ids[index]
```

The tiny subtraction is what stops a shade of exactly 1.0 from indexing one past the end. It is worth
copying verbatim rather than replacing with a clamp: with four palettes, a shade of 0.5 selects index
1, not index 2, and the difference is visible on every dyed item.

## 3. Surface (`0x08xxxxxx`)

**A surface file has no id header.** It starts directly with its type word; the id comes from the
directory entry. A generic "read the id echo, then the body" loader mis-parses every surface in the
container, and it mis-parses them *plausibly*, which is worse.

| offset | size | field | condition |
|---:|---:|---|---|
| `0x00` | 4 | `type` | always |
| `0x04` | 4 | `texture_id` | when `type & 0x6` |
| `0x08` | 4 | `palette_id` | when `type & 0x6` |
| `0x04` | 4 | `color_value` (`0xAARRGGBB`) | when **not** `type & 0x6` |
| … | 4 | `translucency` | always |
| … | 4 | `luminosity` | always |
| … | 4 | `diffuse` | always |

So a textured surface is 24 bytes and a flat-colour surface is 20.

### 3.1 The type flags

| bit | meaning |
|---|---|
| `0x1` | Flat colour; `color_value` is in the file. |
| `0x2` | Textured; a texture id and a palette id are in the file. |
| `0x4` | Textured **and alpha-tested**: palette indices 0–7 become fully transparent when the image is expanded, and the renderer turns alpha testing on. |
| `0x10` | Modulate by `translucency`; forces source-alpha / inverse-source-alpha blending. |
| `0x20` | Diffuse. Never set in shipped data. |
| `0x40` | Luminous. Never set in shipped data. |
| `0x100` | Source-alpha blending. |
| `0x200` | Inverse-alpha blending. Never set in shipped data. |
| `0x10000` | Additive: the destination factor becomes one, and fog alpha is disabled. |
| `0x20000` | A second-stage detail texture. Never set in shipped data. |
| `0x10000000` | Gouraud. Forced on at draw time regardless of the file. |
| `0x40000000` | Set at draw time when the polygon asks for wrapped addressing. |
| `0x80000000` | A legacy software-rasteriser flag. Never set. |

Four fifths of all shipped surfaces are the plain textured case. `translucency` is zero on all but a
few hundred of them.

### 3.2 The load-time fix-up

After reading, the client does one thing that is not in the file and that a rebuild must reproduce:

```text
if translucency > 0.0002: type |= 0x10
```

Surfaces exist that carry a translucency value without the translucent bit. Without the fix-up they
render opaque. Do it at load, not at draw.

### 3.3 What the type means at draw time

Condensed, because reproducing it is what makes a scene look right rather than approximately right:

```text
if source-alpha or blending is forced:
    src = SRCALPHA;      dst = additive ? ONE : INVSRCALPHA;   blend on
elif not inverse-alpha:
    src = ONE;           dst = additive ? ONE : ZERO;          blend = additive
else:
    src = INVSRCALPHA;   dst = additive ? ONE : SRCALPHA;      blend on

if clipmap:
    if blending was off: src = ONE; dst = INVSRCALPHA
    alpha test on, GREATEREQUAL, reference = palettised source ? 100 : 200
    blend on

if not translucent: alpha = 255
else:               if alpha testing, switch to SRCALPHA/INVSRCALPHA and turn it off
                    alpha = translucency * 255

depth write   = not (blending and not alpha testing)
fog alpha off = additive
```

Two details that are easy to miss and visible when missed:

- **The alpha-test reference depends on the source format**: 100 for a palettised image, where the
  clip colour expanded to alpha 0, and 200 for a block-compressed one. A single threshold visibly
  changes the edges of every leaf and fence.
- Luminosity replicates to r, g, b; diffuse is multiplied by the sunlight colour when sunlight is on.

## 4. Surface texture (`0x05xxxxxx`)

A **categorised** type, so the header is two dwords.

| offset | size | field | meaning |
|---:|---:|---|---|
| `0x00` | 4 | id echo | |
| `0x04` | 4 | `category` | Always zero in every shipped file of this type. |
| `0x08` | 1 | — | Written as the constant `2`, read and discarded. Probably a version tag from an older build. |
| `0x09` | 4 | `num_levels` | 1 or 2. |
| `0x0D` | 4 × n | `level_ids` | [Image](14-textures.md) ids. |

**Note the offsets**: the count at `0x09` is not 4-aligned. This file is the cleanest proof in the
whole container that padding is disabled — see
[03-serialisation-primitives.md](03-serialisation-primitives.md).

Level selection:

```text
if num_levels == 2 and the client is dropping high detail: use level 1
else:                                                       use level 0
anything other than 1 or 2 levels is a data error
```

"Dropping high detail" is false — that is, level 0 wins — only when the high-res container is loaded
**and** the environment-texture-detail preference is zero. It is a global preference, not a per-texture
decision, so a rebuild should keep both ids and let the preference be toggled at runtime.

This is where the high-res container earns its place. Of the shipped surface textures, roughly three
in five have a single level; of the two-level ones, most have their level 0 **only** in the high-res
container and their level 1 in the portal container. The two containers share no ids at all. **The
high-res file is not a set of replacements — it is the top half of a split texture set.** Without it,
those textures silently fall back to their low-detail level, and nothing else changes. A missing
level-0 image is therefore not an error.

## 5. The appearance record

Not a container type. It is built from a [clothing table](20-clothing-table.md) plus a palette
template, sent by the server in the object-appearance messages, and applied to a live object.

| offset | size | field | meaning |
|---:|---:|---|---|
| `+0` | 1 | version | **Must be `0x11`.** Any other value rejects the record. |
| `+1` | 1 | `num_subpalettes` | |
| `+2` | 1 | `num_texture_changes` | |
| `+3` | 1 | `num_part_changes` | |
| `+4` | 2 or 4 | `palette_id` | Packed relative to the palette base. **Only present when `num_subpalettes` is non-zero.** |
| … | var | subpalettes | |
| … | var | texture changes | |
| … | var | part changes | |
| … | 0–3 | pad to 4 | |

A **subpalette** is 4 or 6 bytes: a packed palette id, then a `u8` offset and a `u8` length. Both are
**shifted left by three**, and a length byte of zero means 256 — so the pair `(o, l)` replaces palette
entries `[o × 8, o × 8 + (l ? l : 256) × 8)`. A length of zero therefore means the whole palette.

A **texture change** is 5 to 9 bytes: a `u8` part index, then two packed ids relative to the
**surface-texture** base. Not the image base — these are `0x05` ids, not `0x06` ids.

A **part change** is 3 or 5 bytes: a `u8` part index and a packed id relative to the geometry base. It
is the same structure the mesh-swap animation hook carries, read identically.

Two behaviours of the record's lists, which matter if a rebuild builds records as well as reading
them: inserting removes a duplicate for the same part first, and **a list silently drops new entries
once it reaches 255**. Subpalette insertion merges overlapping ranges rather than appending blindly.

## 6. Where reimplementations differ

| topic | the client | common reimplementations |
|---|---|---|
| The translucency fix-up | Applied at load. | Usually absent, because a reader with no renderer does not need it. A renderer that inherits the reader inherits the omission. |
| The surface id header | There is none. | Correctly absent in the well-known readers — but it catches everyone writing a new one. |
| The shade formula | The truncating form with its epsilon. | Sometimes an extra clamp, which changes nothing, and sometimes a plain multiply, which changes the palette chosen at the top of the range. |
| The rejected-shade result | No id. | Often zero, which is the same thing by another name. |
| The 256-entry palette expansion | Implemented. | Usually not modelled, since nothing shipped needs it. |
| The constant byte in a surface texture | Read and discarded. | The same, under a name meaning "unknown". |

## 7. A synthetic example

A flat-colour translucent surface (20 bytes) and the surface texture that a textured one would name:

```text
surface
  type           0x00000011      # flat colour + translucent
  color_value    0xFF000000      # opaque black
  translucency   1.0
  luminosity     0.0
  diffuse        1.0

surface texture
  id echo        0x05000001
  category       0x00000000
  constant       0x02            # one byte, at offset 0x08
  num_levels     0x00000002      # this dword starts at offset 0x09, unaligned
  level 0        0x06000001      # high detail, typically in the high-res container
  level 1        0x06000002      # low detail
```

Nothing above is taken from a shipped file.
