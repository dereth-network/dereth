# Records before Throne of Destiny

Before Throne of Destiny (June 2005) the game shipped two data files, `portal.dat` and `cell.dat`.
Their container is described in [01-dat-container.md](01-dat-container.md) section 11. Most record
types kept their layout across the change; a few were rewritten. This page lists every difference,
type by type, as the reader of the February 2005 files sees it. Each older layout reads into the
same value the later one does, so nothing downstream of a decoder needs to know which files it came
from, except where this page says so.

Every file held from this era, from the October 1999 retail CD to February 2005, reads whole with
the layouts below: every record has a type and decodes with nothing left over. Where a layout
changed within the era, the difference is listed with when it appears; the record itself (its
length, or a type word it holds) decides which one a reader takes, never the file's date.

**Readers:** `Decode::decode_pre_tod` on each type in `dereth-assets` (the default, for the types
whose layout did not change, is the later reader), chosen by `Decode::decode_payload_in` and
`decode_any_in` from the store's `ContainerEra`; the type of an id is `divine_type_in`. Pinned by
the public-tier `pre_tod_layouts` and `legacy_tables` tests in `core/assets/tests/cpu/`; in the
`dat` tier, over the February 2005 files, by `pre_tod_decode`, over every held file of every era
by `capture_census`, and the per-era contents by `legacy_layouts` (the last two read
`DERETH_TEST_DAT_CAPTURES_DIR`).

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
- **Palettes (`0x04`)** read with the later layout, and hold 256 colours (the later ones hold
  2048: each colour eight times). A 256-colour image indexes its palette directly: index `i` is
  colour `i`, which is entry `8i` of the palette expanded to 2048.

## Strings and the region

- **String (`0x31`)**: the text is a padded `u16`-length string (with the `0xFFFF` escape to a
  `u32` length), not a packed-count one.
- **Padded strings of the files through 2002 count their terminating NUL.** Their length is one
  more than the text, the last byte a NUL, and the client reads the string up to that NUL. From
  January 2004 the length is the text alone. Dropping one trailing NUL reads both.
- **Region (`0x13000000`)**: a sky object has **eight words** — no particle-script id (the later
  files keep this until August 2012; see [41](41-older-records-in-the-later-files.md)) — and the
  land surface is **type 1, palette shifting**, not texture merging (the type word in the record
  says which): a `u32` texture count, then per
  texture its id, a `u32` count `n` of `(index, length)` sub-palette ranges, a `u32` count of road
  codes each followed by `n` sub-palette types, and a `u32` count of `(terrain type, palette)`
  pairs. Everything else is unchanged.
- **The second region (`0x130F0000`).** The older files hold a second, complete region of the same
  world (same name, grid and heights) at `0x130F0000`. The client loads region `n` from
  `0x13000000 + n` or, under one of its display settings, from `0x130F0000 + n`; the later files
  have only the first.

### Palette shifting

The February 2005 region names two 256×256, 256-colour textures, their palettes cut into ranges
of 20 colours. A cell is drawn with one texture whose ranges are refilled from the palettes of the
terrain types at its corners (and of the road), so the picture's regions take the corners'
colours:

1. The cell's four keys are the ones texture merging computes, one per rotation of its corners.
2. The first texture tried is `(u32(y·0x6C1AC587 − x·(y·0x622DBEDF + 0x421BE3BD) − 0x791C2B27) ·
   count) >> 32` for the global cell `(x, y)`; the others follow in order, wrapping.
3. A texture is used only if its terrain list has a palette for each corner's terrain type, and,
   when the cell has a road, for the road (terrain type 32).
4. The rotations are tried from the one whose key is smallest, each in turn; the first whose road
   pattern (a bit per corner with a road, SW highest) the texture lists is taken. That rotation's key is
   the surface's cache key, and the cell's texture coordinates are rotated by it.
5. Range `k` takes its colours from the palette the road entry's type `k` names: 0-3 are the
   corners counted from the rotation, 4 the road. The colours copied are those at the same indices;
   indices outside every range keep the texture's own palette.

If no texture fits, the cell is drawn with the first texture unrotated. On the February 2005 world
every cell finds one.

The land-surface type in the region record is what decides between palette shifting and texture
merging; the container's layout does not.

**A dat set from before Throne of Destiny carries two regions**: `0x13000000`, palette shifting,
and `0x130F0000`, texture merging. They are otherwise the same record (land definitions, calendar,
sounds, scenes, terrain types and the trailing block are equal) except the sky. The clients of the
time chose by renderer: drawing with 3D
hardware they loaded `0x130F0000 + n`, drawing in software `0x13000000 + n`, for region number `n`.
The later client loads one region and the later files carry only `0x13000000`. The texture-merge
region of February 2005 has a base texture size of 128 and names the same texture ids as the later
region (128×128 and 64×64 terrain images, 64×64 alpha masks, 256×256 detail textures), with its own
tilings (the landscape detail texture eight times a cell, the building and environment one three).

The detail textures (landscape, building, environment) are read from the texture-merge terrain
descriptions, so the palette-shift region has none: in software the clients of that time drew no
detail texture, whatever their options said.

The terrain types are numbered alike in every region from 1999 on: the same name and map colour
at the same index (27 types in October 1999, 31 in the Dark Majesty and February 2005 files, 32 at
the end of retail, each adding at the end), and the road is type 32 in both techniques. A cell's
terrain words therefore read the same under either technique, which is what lets any region's land
surface draw any world's cells.

Every land surface, of either technique and in every era, lists all 33 rows (types 0 to 31 and the
road), but a row past the end of the region's own terrain list is a filler: another type's picture.
The texture-merge regions before Throne of Destiny give the unnamed types Argila's tile
(`0x0500145C`, type 24's), and their palette-shift textures give them a stand-in palette
(`0x040004C2`, Semi-Barren Rock's, on the first texture and `0x040004D6` on the second). The
end-of-retail region names type 31, DesolateLands, and draws it with the same tile id as Argila.
The October 1999 cells use type 31 at 325 vertices, all of them with road bits 3: a grid of roads
laid over deep sea, in 25 blocks of columns `0xE1` to `0xE9` and rows `0x05` to `0x08`. The
end-of-retail cells use it at three vertices, in blocks `0xF930` and `0xECF8`.

**The two skies of a dat set before Throne of Destiny, and the later one.** The software region's
sky has fewer objects than the hardware region's in every day group (February 2005: 3 to 12
against 7 to 19), names different dome and cloud objects, and carries its own light and fog: none
of the twenty day groups has the same sun, ambient light or fog in the two. The February 2005
hardware sky has the same light and fog in all twenty day groups as the end-of-retail sky; the
end-of-retail one replaces its clouds and weather with new objects and sets new property bits on
them. October 1999 has eleven day groups, the later files twenty. The tick and light-tick sizes are
the same in every region.

## Game tables

- **XpTable (`0x0E000018`)**: the character-level list is `u32`, not `u64` (126 levels in February
  2005); it reads widened. The other five lists are unchanged.
- **SpellTable (`0x0E00000E`)**: the spells alone (the `u16` count and bucket header, then each
  key and spell), with **no spell-set table** after them. A spell is the later layout up to and
  including its recovery amount; what follows grew twice:

  | files | a spell ends with |
  |---|---|
  | October 1999 to January 2002 | the recovery amount |
  | late 2002 | the display order |
  | January 2004 on, and the later files | the display order, the non-component target type and the per-target mana |

  Nothing in the table says which. The table is read with each ending, the later first, and the
  one that ends exactly on the record's end is taken; exactly one does for every table held. A
  field a spell does not carry reads as zero. The meta-spell types run 1 to 8 in these files, with
  the payloads of the later layout. The October 1999 table has 1,635 spells; February 2005, 3,737.
- **CharGen (`0x0E000002`)** is a different, pack-style layout: counts are full `u32`s, strings are a
  `u32` length and the bytes, **padded to four** (or, when the first word is above `0xFFFF`, a padded
  `u16`-length string), and heritages and sexes are ordered lists without keys. The layout is the
  same in every file of the era, October 1999 included; the files through 2002 write their strings
  in the `u32`-length form, which is where the padding shows.

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

  The word between the attribute and skill credits is the sex's **starting-spell credits**, and the
  (string, word, word) list after the skill credits its starting spells; the client offers a
  starting-spell page only when the credits are above zero. In every file of the era the credits
  are 0 and the list is empty, so character creation never offered starting spells. The
  seventeen-word rows are heraldry choices, and every list of them is empty too.

  The contents changed within the era: the October 1999 table has eighteen starter areas (six per
  heritage, two each in three towns) and eight templates (Adventurer, Archer, Blademaster,
  Enchanter, Life Mage, Sorcerer, Vagabond, Warrior); from September 2001 the templates are the
  seven of the later files, and from January 2002 the starter areas are the six above.

  It reads into the later shape: heritages keyed 1, 2, 3 in list order; sexes keyed by name (Male 1,
  Female 2); the heritage's credits, skill costs and templates taken from its first sex; a
  template's attributes and first two skill lists from its one profile (a template with more than
  one profile, or a non-empty third list, is refused). What the older table does not have — a sex's
  scale and its physics, motion and combat tables, a hair style's alternate setup, a template's
  title — reads as zero, and a consumer that needs one takes it from elsewhere (the sex's setup
  names its default motion and sound tables).

- **Quality filters (`0x0E000010`, `0x0E000017`)**: the later layout without the 64-bit integer
  list: the id, **seven** property-list counts (integer, then the six the later layout lists after
  its 64-bit integer list), the lists, then the three attribute lists as later. The int64 list reads
  empty. The two filters are the later files' `0x0E010001` and `0x0E010002`.
- **Quest table (`0x0E00001B`)**, which ended at Throne of Destiny: the id, the `u16` count and
  `u16` bucket header, then per quest a padded string key, an `i32` minimum interval in seconds, an
  `i32` solve limit (-1 without limit) and the display name, a padded string whose bytes are
  nibble-swapped as a spell name's are. February 2005 has 190 quests; the table is present from
  August 2000.

## The later interface beside an older world

The two older files have no interface records of the later kind (layout properties, fonts, the
enum and id maps, interface images, the language file's layouts and strings). A store that opens
the older world with the end-of-retail files beside it (`RetailDatStore::open_pre_tod_with_later`,
the client's `--world-dat-dir`) answers every record the older portal and cell files hold from
them, in their layouts, and every other portal record and every language record from the later
files, in the later layouts; `RetailDatStore::era_of` says which layout a record is in.

## Ids only these files type

Three ids of these files lie outside every later type range: the two quality filters at
`0x0E000010` and `0x0E000017` (the later files put them at `0x0E010000` on) and the second region at
`0x130F0000`. `divine_type_in` types them for a record of these files; `divine_type`, the later
client's own lookup, does not.
