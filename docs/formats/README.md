# Data formats

The on-disk formats of the four data files the 2013 client ships with. Every page describes one
format as a record layout, says which module of this client reads it and which test pins the claims,
and ends with a synthetic example — none of the examples are bytes taken from a shipped file.

Read the first three in order; the rest stand alone.

| page | what it covers |
|---|---|
| [01-dat-container.md](01-dat-container.md) | The container: header, blocks, the B-tree directory, the write journal, the iteration file, and how four files answer one lookup. |
| [02-file-ids-and-types.md](02-file-ids-and-types.md) | The 32-bit file id, the sixty-five types and their id ranges, and why the top byte is not the type. |
| [03-serialisation-primitives.md](03-serialisation-primitives.md) | The object header, the alignment rule, the compressed integer, the two string forms, the three hash-table headers and the pack versions. |
| [10-gfxobj.md](10-gfxobj.md) | Geometry: vertex arrays, polygons, and the drawing and physics BSP trees. |
| [11-setup.md](11-setup.md) | Jointed models: parts, attachment points, placement poses, collision volumes and lights. |
| [12-animation.md](12-animation.md) | Key frames and the twenty-seven kinds of frame hook. |
| [13-palette-and-surfaces.md](13-palette-and-surfaces.md) | Palettes, palette sets, surfaces, surface textures, and the packed appearance record. |
| [14-textures.md](14-textures.md) | Images: pixel formats, the palette-indexed colour key, the raw-JPEG variant, mip handling. |
| [15-region.md](15-region.md) | The one region record: the world grid, the calendar, the sky, terrain texturing and ambient sound. |
| [16-environment.md](16-environment.md) | Interior cell shapes and how a cell record refers to one. |
| [17-scene-and-particles.md](17-scene-and-particles.md) | Landscape scatter — including the deterministic placement hash — and particle emitters. |
| [18-physics-scripts.md](18-physics-scripts.md) | Visual-effect timelines and the tables that select them. |
| [19-motion-table.md](19-motion-table.md) | The motion state machine: cycles, modifiers and transitions. |
| [20-clothing-table.md](20-clothing-table.md) | Wearable appearance across body types, and the dye system. |
| [21-sound-tables.md](21-sound-tables.md) | Raw waves and the sound-type tables that name them. |
| [22-degrade-info.md](22-degrade-info.md) | Level-of-detail chains and the frame-rate feedback loop that drives them. |
| [30-spell-tables.md](30-spell-tables.md) | Spells, spell sets and components — including the obfuscation and the per-account formula randomisation. |
| [31-skill-xp-tables.md](31-skill-xp-tables.md) | Skills, the vital formulas and the experience curves. |
| [40-before-throne-of-destiny.md](40-before-throne-of-destiny.md) | The record layouts of the two data files from before Throne of Destiny, type by type. |

## Conventions used on these pages

- **Offsets** are relative to the start of the record's payload unless a page says otherwise, and
  every integer is little-endian.
- **Nothing is aligned** unless a page says it is. See
  [03-serialisation-primitives.md](03-serialisation-primitives.md) — this is the single most common
  source of a reader that works on most files and fails on some.
- A **"readers" line** on each page names the module of this client that implements the format and
  the tests that pin its claims. Tests named as public-tier run without any shipped data file; a
  whole-container check that needs the shipped files is named as such.
- A **"where reimplementations differ"** table lists behaviour, not code: what the client does, and
  what independently written readers are commonly observed to do instead. Several of those rows are
  differences that shipped data never exercises, and they are marked as such.
