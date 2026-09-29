# Images (`0x06xxxxxx`)

An image record is one raw image: width, height, a pixel format, a byte count, and that many bytes of
pixels — plus, for the two palette-indexed formats, the id of a default
[palette](13-palette-and-surfaces.md). A [surface](13-palette-and-surfaces.md) never names one
directly; it names a surface texture, which names one or two images.

**Mip chains are never stored.** The client generates them at upload time, for block-compressed
formats only.

**Provenance:** from the DAT format, which public readers document.

**Readers:** `dereth_assets::material` (`RenderSurface`; the payload is exposed as a byte range, since
pixel decoding belongs to the renderer). Pinned by `the_progressive_jpeg_ids_are_the_documented_six`
in `core/assets/src/material.rs`.

## 1. Layout

A **categorised** type, so the header is two dwords.

| offset | size | field | meaning |
|---:|---:|---|---|
| `0x00` | 4 | id echo | |
| `0x04` | 4 | `category` | 0–10; section 2. |
| `0x08` | 4 | `width` | **Zero** for the raw-JPEG format. |
| `0x0C` | 4 | `height` | **Zero** for the raw-JPEG format. |
| `0x10` | 4 | `format` | Section 3. |
| `0x14` | 4 | `image_size` | Byte count of the pixel block. |
| `0x18` | `image_size` | pixels | Copied verbatim. |
| … | 4 | `palette_id` | **Only** for the two palette-indexed formats. |

After the header the client **validates**:

```text
unless the format's size is underivable:
    image_size must equal (bits_per_pixel * width * height) / 8
```

Every shipped image of every format except raw JPEG satisfies it exactly. A mismatch is a hard error,
not a warning, and treating it as one catches a mis-parsed header on the file that caused it rather
than a thousand files later.

An unknown format id leaves the bits-per-pixel at zero, which then fails the same check.

## 2. The category

The values 0–10 occur, and the client interprets exactly two of them:

- **6** — the image is created in the device's **UI surface format** rather than its own. Two thirds of
  the shipped images are category 6: icons, panels and font art.
- **10** — format conversion is suppressed entirely. Eighty-odd files.

Both 6 and 10 also mark the object as reusable from the free list. The meaning of the other nine
values is unresolved; nothing in the client reads them.

## 3. Pixel formats

The format ids are the Direct3D 8/9 format numbers, plus a block of custom values.

| id | name | bits/pixel | notes |
|---:|---|---:|---|
| 20 | `R8G8B8` | 24 | |
| 21 | `A8R8G8B8` | 32 | The commonest format in the container. |
| 22 | `X8R8G8B8` | 32 | Never appears in shipped data. Red, green and blue have `A8R8G8B8`'s masks, and there is no alpha mask, so the unused top byte reads as opaque. |
| 23 | `R5G6B5` | 16 | |
| 24 | `X1R5G5B5` | 16 | |
| 25 | `A1R5G5B5` | 16 | |
| 26 | `A4R4G4B4` | 16 | |
| 28 | `A8` | 8 | |
| 30 | `X4R4G4B4` | 16 | |
| 31, 35 | `A2B10G10R10`, `A2R10G10B10` | 32 | |
| 32, 33 | `A8B8G8R8`, `X8B8G8R8` | 32 | |
| 41 | `P8` | 8 | Palette-indexed. Carries a trailing palette id. |
| 60 | `V8U8` | 16 | Bump. |
| 70–80 | depth formats | 16–32 | |
| 101 | `INDEX16` | 16 | Palette-indexed, **16-bit indices**. Carries a trailing palette id. |
| 240–244 | custom RGBA, BGR and the two landscape formats | 8–32 | Section 5. |
| 500 | raw JPEG | — | Size not derivable. Section 4. |
| `DXT1`–`DXT5` | four-character codes | 4 or 8 | Block-compressed. |

A dozen further ids exist in the enumeration and are **rejected** by the format setup — the luminance
formats, the 3-3-2 formats, the 32-bit index format and a few bump variants. A reader should reject
them too rather than guessing a bit depth.

In shipped data the distribution is lopsided: 32-bit ARGB for UI art and 16-bit palette-indexed for
world textures account for most of it, with DXT1 a distant third. Everything else is in the dozens.

## 4. The palette-indexed formats and the colour key

The two palette-indexed formats are the only ones that carry a trailing palette id, and they are
reported as **not uploadable** — they must be expanded on the CPU first:

```text
for each pixel:
    index = 16-bit or 8-bit, per the format
    if the referencing surface is a clipmap and index < 8:
        write 0x00000000            # fully transparent
    else:
        write palette[index]
```

**Indices 0–7 are the colour key**, and only when the *surface* that references this image has its
clipmap bit set. The same image can be used opaquely by a different surface, so the rule belongs to
the pairing, not to the image.

Why eight and not one: [palettes are 2048 entries](13-palette-and-surfaces.md) built by replicating a
256-colour table eight times, so indices 0–7 all correspond to original colour 0. A 16-bit index
addresses the 2048-entry table directly, which is the entire point of the expansion — 256 base colours
× 8 shade slots that a subpalette can replace independently.

The 8-bit variant reaches only the first 256 entries, that is, the first 32 base colours. Six shipped
files use it.

## 5. The raw-JPEG variant and the landscape formats

Format 500 stores a complete JFIF stream. `width`, `height` and the bit depth are **zero in the
header** and come from the JPEG itself. The client requires **three channels** and refuses anything
else, decodes to 24-bit RGB, and writes the result BGR at the destination pitch.

**A baseline-only decoder is not enough.** Of the seventy-nine raw-JPEG images in the shipped portal
container, **six are progressive**. A decoder that handles baseline only passes seventy-three images
and silently fails those six — which is exactly the kind of bug that shows up as six missing textures
nobody can place. The six ids are pinned as a constant in the reader.

The two **landscape** formats — 24-bit RGB and 8-bit alpha — are inputs to the terrain compositor, not
textures. They are flagged as not uploadable, and terrain tiles are composited on the CPU (a base tile
blended with an alpha mask under a rotation) before the finished 32-bit buffer is uploaded as a
separate, container-less texture. Do not upload them directly.

## 6. Format substitution at upload

The client substitutes the upload format, but only at runtime and only when a conversion hack flag is
set:

```text
if the format is not a device format:  keep it          # handled on the CPU
if category == 6:                      use the device's UI surface format
if category == 10:                     keep it
if block-compressed:                   keep it, unless the device cannot do compressed textures,
                                       in which case use the device's ARGB texture format
if it has both colour and alpha:       the device's ARGB texture format
if alpha only:                         the device's alpha texture format
if colour only:                        the device's RGB texture format
```

## 7. Mip handling

```text
w, h  = the image dimensions right-shifted by the global texture-detail reduction
levels = max(w, h) > 1 ? min(floor(log2(max(w, h))) + 1, 4) : 1
if the format is not block-compressed: levels = 1
upload level 0, then box-filter the rest
```

Two consequences to match:

- **No mip data is in any container.** Every level below zero is a box-filtered reduction generated at
  load. A different filter — Kaiser, or a gamma-correct box — makes distant terrain and armour differ
  from the original pixels.
- **Only block-compressed textures get mips at all, and at most four levels.** Palette-indexed and
  uncompressed textures upload as a single level and therefore alias in the distance exactly as the
  original did. Adding mips to the 16-bit world textures makes distant terrain visibly smoother than
  the game ever was.

## 8. Where reimplementations differ

| topic | the client | common reimplementations |
|---|---|---|
| The size validation | A hard error. | Usually not checked. |
| The clipmap colour key | Applied per *surface*, not per image. | Usually implemented, and usually correctly. |
| Format substitution and mip generation | As above. | Not modelled — most readers are tooling, not renderers, and the omission is inherited by renderers built on them. |
| Progressive JPEG | Decoded by the library the client links. | Depends entirely on the decoder chosen. Six images are at stake. |
| The category | Two values interpreted. | Read and named "unknown". |
| The high-res container | A second source level, chosen by preference. | Opened as just another database with no level selection. |

## 9. A synthetic example

An 8 × 8 palette-indexed image, all indices zero, as a clipmap surface would use it — 156 bytes:

```text
id echo      0x06000001
category     0x00000007
width        0x00000008
height       0x00000008
format       0x00000065        # 16-bit palette-indexed
image_size   0x00000080        # 128 = 8 * 8 * 16 / 8
pixels       64 little-endian u16 indices, all 0
palette_id   0x04000001        # present only because the format is palette-indexed
```

Under a clipmap surface every pixel is index 0, so the whole tile expands to fully transparent.
Nothing above is taken from a shipped file.

## 10. The unused material types

The portal container also carries a handful of files for a later engine's material system that this
game never shipped content for: one texture-descriptor type (a categorised header, a one-byte texture
kind, a level count and that many image ids), and one file each of a material, a material modifier and
a material instance. Nothing in the game references them, and the mesh type of that family has **no
files at all**. Their layouts past the first are not established, and a rebuild can ignore all five.
