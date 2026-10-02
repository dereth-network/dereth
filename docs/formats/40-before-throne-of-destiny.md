# Records before Throne of Destiny

Before Throne of Destiny (June 2005) the game shipped two data files, `portal.dat` and `cell.dat`.
Their container is described in [01-dat-container.md](01-dat-container.md) section 11. Most record
types kept their layout across the change; a few were rewritten. This page lists every difference,
type by type, as the reader of the February 2005 files sees it. Each older layout reads into the
same value the later one does, so nothing downstream of a decoder needs to know which files it came
from, except where this page says so.

**Readers:** `Decode::decode_pre_tod` on each type in `dereth-assets` (the default, for the types
whose layout did not change, is the later reader), chosen by `Decode::decode_payload_in` and
`decode_any_in` from the store's `ContainerEra`. Pinned by the public-tier
`pre_tod_layouts` tests in `core/assets/tests/cpu/` and, over the February 2005 files, by
`pre_tod_decode` in the `dat` tier, which decodes every record of both files.

## Unchanged

Animations, setups, motion tables, waves, sound tables, particle emitters, physics scripts and their
tables, palette sets, clothing tables, degrade information, scenes, combat tables, the skill table,
the spell-component table, the vital formula table, the chat-pose table, the object hierarchy, the
bad-data list, landblocks and landblock information all read with the later layout. (Their
*contents* changed at Throne of Destiny for some records; their layout did not.)

## Geometry

- **GfxObj (`0x01`).** Id, flags, then a **plain `u32`** surface count (not compressed) and the
  surface ids; the vertex array; if `flags & 1` a `u32` physics-polygon count, the polygons and the
  physics BSP; the sort centre; if `flags & 2` a `u32` polygon count, the polygons and the drawing
  BSP. There is no degrade id. Bit 2 would add a triangle-strip block, which no shipped record sets
  and this reader refuses. The record ends aligned to four bytes.
- **Polygons** end with alignment to four bytes.
- **BSP trees**: a drawing node's polygon (and portal) list, and a physics leaf's polygon list, end
  with alignment to four bytes. Tags, children and every other field are unchanged.
- **Vertices** end with alignment to four bytes, which their fields always fill already.
- **Environment (`0x0D`)**: the cell structure is unchanged apart from the polygon and BSP
  alignment above.
- **EnvCell (cell file)**: the record starts with the **flags and then the cell id** — there is no
  id before the flags — and aligns to four bytes after the surface list and after the visible-cell
  list. Everything else is unchanged.

## Surfaces and images

- **Surface (`0x08`)** starts with its own id; the rest is unchanged.
- **Image texture (`0x05`)** is itself an image, not a list of image levels: id, image type, width,
  height, `width × height × bytes-per-pixel` bytes of pixels, a default palette id for type 2 only,
  then alignment to four bytes. It reads as a texture whose one level is itself, and its pixels read
  as the later pixel format that holds the same bytes:

  | type | bytes per pixel | later pixel format |
  |---:|---:|---|
  | 2 | 1 | 8-bit palette index (`P8`, id 41), default palette follows |
  | 3 | 2 | `A1R5G5B5` (25) |
  | 4 | 2 | `A4R4G4B4` (26) |
  | 7 | 2 | `R5G6B5` (23) |
  | 10 | 3 | the landscape three-byte format (243) |
  | 11 | 1 | the landscape alpha format (244) |

- **Image (`0x06`)**: id, width, height, then three bytes per pixel in **red, green, blue** order
  (the later format 242), with no category, format, length, palette or padding.
- **Palettes (`0x04`)** read with the later layout.

## Strings and the region

- **String (`0x31`)**: the text is a padded `u16`-length string (with the `0xFFFF` escape to a
  `u32` length), not a packed-count one.
- **Region (`0x13000000`)**: a sky object has **eight words** — no particle-script id — and the land
  surface is **type 1, palette shifting**, not texture merging: a `u32` texture count, then per
  texture its id, a `u32` count `n` of `(index, length)` sub-palette ranges, a `u32` count of road
  codes each followed by `n` sub-palette-type ids, and a `u32` count of `(terrain type, palette)`
  pairs. Everything else is unchanged.

## Game tables

- **XpTable (`0x0E000018`)**: the character-level list is `u32`, not `u64` (126 levels in February
  2005); it reads widened. The other five lists are unchanged.
- **SpellTable (`0x0E00000E`)**, in the layout used from January 2004: the spells alone, each in the
  later layout, with **no spell-set table** after them.
- **CharGen (`0x0E000002`)** is a different, pack-style layout: counts are full `u32`s, strings are a
  `u32` length and bytes (or, when the first word is above `0xFFFF`, a padded `u16`-length string),
  and heritages and sexes are ordered lists without keys.

  | order | field |
  |---|---|
  | 1 | id, then eight help-text string ids |
  | 2 | starter areas: name, `u32` count, positions (cell and frame) |
  | 3 | heritages: name, icon, setup, a description string id, environment setup, primary and secondary starter-area lists (`u32` counts), then the sexes |

  A sex is: name; setup, sound table, icon and a naming-help string id; its base appearance; two
  words; a list of strings; a list of (string, string, word); **attribute credits**, a word, **skill
  credits**; a list of (string, word, word); the **skill costs** (skill, normal cost, primary cost);
  the **templates**; a list of seventeen-word rows; the base palette and skin palette set; hair
  colours; hair styles (icon, a `u32` bald value, appearance); eye colours; eye strips; nose strips;
  mouth strips; headgear, shirts, pants and footwear (name, clothing table, weenie); then a count,
  the largest index and the allowed clothing colours. A template is a name, icon and description id
  followed by a list of profiles, each six attributes and three skill lists.

  It reads into the later shape: heritages keyed 1, 2, 3 in list order; sexes keyed by name (Male 1,
  Female 2); the heritage's credits, skill costs and templates taken from its first sex; a
  template's attributes and first two skill lists from its one profile (a template with more than
  one profile, or a non-empty third list, is refused). What the older table does not have — a sex's
  scale and its physics, motion and combat tables, a hair style's alternate setup, a template's
  title — reads as zero, and a consumer that needs one takes it from elsewhere (the sex's setup
  names its default motion and sound tables).

## Types with no counterpart

The quest table `0x0E00001B` ended at Throne of Destiny and has no reader. `0x0E000010`,
`0x0E000017` and a second region record `0x130F0000` have ids the later type ranges do not name.
