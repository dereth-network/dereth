# File ids and object types

Every file in the four containers is addressed by a 32-bit **file id**, and the id is the only place
the kind of object is recorded — there is no type field on disk. The client derives the type from a
fixed table of id ranges and then carries the pair (type, id) everywhere, because the type is what
decides which container answers the request and which cache the object lands in.

**Provenance:** from the DAT format, which public readers document.

**Readers:** `dereth_dat::divine` — `divine_type`, the type table, and the container routing. Pinned by
`the_registered_table_has_sixty_five_types`, `divine_type_is_not_the_top_byte`,
`game_ranges_win_over_engine_ranges`,
`the_font_local_string_state_overlap_resolves_by_registration_order`,
`the_unused_spans_divine_to_nothing`,
`exactly_three_types_are_categorized`, `cell_ids_are_classified_by_position_not_by_divine_type` and
`routing_matches_the_catalogue` in `core/dat/src/divine.rs`.

## 1. The type is divined, never stored

Id `0` is invalid and is rejected by every load path. For every other id the type comes from one of
three places:

1. **Divined from the id** — a search over the inclusive `[first, last]` ranges of the type table,
   in registration order, so that where two ranges overlap the one registered first wins. An id in no
   range divines to nothing, and the load fails cleanly rather than guessing.
2. **Supplied by the caller** — the typed accessors each hard-code the type they want.
3. **Divined by position in the container** — when the server pushes a file during patching and
   sends only the bare id, the type defaults by container: cell files get the landblock type, portal
   files get the geometry type, local files get the string-table type.

The table is built in two layers: an engine layer of fifty types and a game layer of fifteen. **The
game layer is searched first**, and it has to be: several game ranges sit inside spans the engine
layer would otherwise answer for. A single flat table sorted by range resolves some ids to the wrong
type. Six engine types — the three cell types, `Lbo`, `Instantiation` and `AnimationHook` — are
registered with no id range at all, so no id ever divines to them.

## 2. The catalogue

Sixty-five registered types. `categorised` marks the three types whose payload carries a second dword
after the id echo (section 4).

| type | value | id range | container | extensions | categorised |
|---|---:|---|---|---|:-:|
| `LandBlock` | `0x01` | — (positional) | cell | — | |
| `Lbi` | `0x02` | — (positional) | cell | `.lbi` | |
| `Cell` | `0x03` | — (positional) | cell | — | |
| `Lbo` | `0x04` | — | server only | `.lbo` | |
| `Instantiation` | `0x05` | — | server only | `.ins` | |
| `GfxObj` | `0x06` | `01000000`–`0100FFFF` | portal | `.obj` | |
| `Setup` | `0x07` | `02000000`–`0200FFFF` | portal | `.set` | |
| `Anim` | `0x08` | `03000000`–`0300FFFF` | portal | `.anm` | |
| `AnimationHook` | `0x09` | — | server only | `.ahk` | |
| `Palette` | `0x0A` | `04000000`–`0400FFFF` | portal | `.pal` | |
| `SurfaceTexture` | `0x0B` | `05000000`–`05FFFFFF` | portal | `.texture` | y |
| `RenderSurface` | `0x0C` | `06000000`–`07FFFFFF` | portal | `.jpg`, `.dds`, `.tga`, `.iff`, `.256`, `.csi`, `.alp` | y |
| `Surface` | `0x0D` | `08000000`–`0800FFFF` | portal | `.surface` | |
| `MTable` | `0x0E` | `09000000`–`0900FFFF` | portal | `.dsc` | |
| `Wave` | `0x0F` | `0A000000`–`0A00FFFF` | portal | `.wav` | |
| `Environment` | `0x10` | `0D000000`–`0D00FFFF` | portal | `.env` | |
| `ChatPoseTable` | `0x11` | `0E000007` | portal | `.cps` | |
| `ObjectHierarchy` | `0x12` | `0E00000D` | portal | `.hrc` | |
| `BadData` | `0x13` | `0E00001A` | portal | `.bad` | |
| `TabooTable` | `0x14` | `0E00001E` | portal | `.taboo` | |
| `File2IdTable` | `0x15` | `0E00001F` | portal | — | |
| `NameFilterTable` | `0x16` | `0E000020` | portal | `.nft` | |
| `MonitoredProperties` | `0x17` | `0E020000`–`0E02FFFF` | portal | `.monprop` | |
| `PalSet` | `0x18` | `0F000000`–`0F00FFFF` | portal | `.pst` | |
| `Clothing` | `0x19` | `10000000`–`1000FFFF` | portal | `.clo` | |
| `DegradeInfo` | `0x1A` | `11000000`–`1100FFFF` | portal | `.deg` | |
| `Scene` | `0x1B` | `12000000`–`1200FFFF` | portal | `.scn` | |
| `Region` | `0x1C` | `13000000`–`1300FFFF` | portal | `.rgn` | |
| `Keymap` | `0x1D` | `14000000`–`1400FFFF` | portal | `.keymap` | |
| `RenderTexture` | `0x1E` | `15000000`–`15FFFFFF` | portal | `.rtexture` | y |
| `RenderMaterial` | `0x1F` | `16000000`–`16FFFFFF` | portal | `.mat` | |
| `MaterialModifier` | `0x20` | `17000000`–`17FFFFFF` | portal | `.mm` | |
| `MaterialInstance` | `0x21` | `18000000`–`18FFFFFF` | portal | `.mi` | |
| `STable` | `0x22` | `20000000`–`2000FFFF` | portal | `.stb` | |
| `UiLayout` | `0x23` | `21000000`–`21FFFFFF` | local | `.uil` | |
| `EnumMapper` | `0x24` | `22000000`–`22FFFFFF` | portal | `.emp` | |
| `StringTable` | `0x25` | `23000000`–`24FFFFFF` | local | `.stt`, `.stt_ansi`, `.stt_bin` | |
| `DidMapper` | `0x26` | `25000000`–`25FFFFFF` | portal | `.imp` | |
| `ActionMap` | `0x27` | `26000000`–`2600FFFF` | portal | `.actionmap` | |
| `DualDidMapper` | `0x28` | `27000000`–`27FFFFFF` | portal | `.dimp` | |
| `StringType` | `0x29` | `31000000`–`3100FFFF` | portal | `.str` | |
| `ParticleEmitter` | `0x2A` | `32000000`–`3200FFFF` | portal | `.emt` | |
| `PhysicsScript` | `0x2B` | `33000000`–`3300FFFF` | portal | `.pes` | |
| `PhysicsScriptTable` | `0x2C` | `34000000`–`3400FFFF` | portal | `.pet` | |
| `MasterProperty` | `0x2D` | `39000000`–`39FFFFFF` | portal | `.mpr` | |
| `Font` | `0x2E` | `40000000`–`40000FFF` | portal | `.font` | |
| `FontLocal` | `0x2F` | `40001000`–`40FFFFFF` | local | `.font_local` | |
| `StringState` | `0x30` | `41000000`–`41FFFFFF` | local | `.sti` | |
| `DbProperties` | `0x31` | `78000000`–`7FFFFFFF` | portal | `.dbpc`, `.pmat` | |
| `RenderMesh` | `0x43` | `19000000`–`19FFFFFF` | portal | `.rendermesh` | |
| `WeenieDef` | `0x10000001` | `00000001`–`0000FFFF` | server only | `.wdf` | |
| `CharGen` | `0x10000002` | `0E000002` | portal | `.cgd` | |
| `Attribute2ndTable` | `0x10000003` | `0E000003` | portal | `.wa2` | |
| `SkillTable` | `0x10000004` | `0E000004` | portal | `.wsk` | |
| `SpellTable` | `0x10000005` | `0E00000E` | portal | `.spt` | |
| `SpellComponentTable` | `0x10000006` | `0E00000F` | portal | `.sct` | |
| `WTreasureSystem` | `0x10000007` | `0E000011` | server only | `.wts` | |
| `WCraftTable` | `0x10000008` | `0E000019` | server only | `.cft` | |
| `XpTable` | `0x10000009` | `0E000018` | portal | `.xpt` | |
| `QuestDefDb` | `0x1000000A` | `0E00001B` | portal | `.qdd` | |
| `GameEventDb` | `0x1000000B` | `0E00001C` | server only | `.ged` | |
| `QualityFilter` | `0x1000000C` | `0E010000`–`0E01FFFF` | portal | `.wqf` | |
| `CombatTable` | `0x1000000D` | `30000000`–`3000FFFF` | portal | `.wct` | |
| `MutateFilter` | `0x1000000E` | `38000000`–`3800FFFF` | server only | `.imf` | |
| `ContractTable` | `0x10000010` | `0E00001D` | portal | `.acc` | |

Two footnotes on the table:

- `MonitoredProperties` is registered — so its ids do divine to a type — but the client has no cache
  that can instantiate one. It is the only registered type in that position.
- `StringState` is registered from `0x40001000`, overlapping `FontLocal` entirely; because
  `FontLocal` is registered first it wins that span, leaving `StringState` the effective range shown.
  No shipped file sits in the overlap.
- The types marked *server only* are registered so that ids resolve, but no shipped container holds a
  file of that type and the client never asks for one.

## 3. Why `id >> 24` is not the type

The single most common way to get this wrong is to take the top byte of the id as the type. It fails
in five distinct ways:

- **`0x0E00xxxx` is not one type.** Every id in that span is its own single-file type: the chargen
  table, the skill table, the spell table, the contract table and so on each occupy exactly one id.
- **`0x06` and `0x07` are one type.** The render-surface range is two top-byte values wide.
- **`0x23` and `0x24` are one type**, as are `0x78`–`0x7F`.
- **The cell container has no type in the id at all** (section 5).
- **The type value and the top byte disagree.** The geometry type is `0x06` and its ids begin `0x01`;
  the render-surface type is `0x0C` and its ids begin `0x06`. Anything that conflates the two is
  reading the table upside down.

Large spans divine to nothing at all: `0x0B000000`, `0x0C000000`, `0x1A000000`–`0x1FFFFFFF`,
`0x28000000`–`0x2FFFFFFF`, `0x35000000`–`0x37FFFFFF`, `0x3A000000`–`0x3FFFFFFF`,
`0x42000000`–`0x77FFFFFF` and everything from `0x80000000` up to the iteration file. A reader should
treat those as "no such type" rather than extrapolating.

## 4. The three categorised types

`SurfaceTexture`, `RenderSurface` and `RenderTexture` — and only those three — carry an extra dword
after the id echo at the start of the payload. Getting this wrong shifts every texture header by four
bytes, which is the kind of bug that produces plausible-looking garbage rather than an error. See
[03-serialisation-primitives.md](03-serialisation-primitives.md).

## 5. The cell container's id space

Cell ids are positional, not typed. The top 16 bits are the landblock coordinate — `x << 24 | y <<
16` — and the bottom 16 are the index within the landblock:

| id pattern | what it is | on disk |
|---|---|---|
| `0xXXYY0001`–`0xXXYY0040` | The 8 × 8 outdoor land cells of a landblock. | **Never stored.** They are generated from the landblock's height and terrain arrays. |
| `0xXXYY0000`, `0xXXYY0041`–`0xXXYY00FF` | Not cell indices: the client treats them as invalid cell ids. | never stored |
| `0xXXYY0100` and up | Indoor and dungeon cells. Numbering starts at `0x0100` and is dense within a landblock. | stored |
| `0xXXYYFFFE` | The landblock's static objects, buildings and cell count. | stored |
| `0xXXYYFFFF` | The landblock itself: terrain and heights. | stored |

So `0xFFFE` and `0xFFFF` are reserved cell indices, and the shipped cell container holds one landblock
record for every `x`, `y` in `0x00`–`0xFE` — a full 255 × 255 grid. The three record layouts have no
page of their own yet; the reader is `dereth_assets::world`.

## 6. Two ids that are not objects

- **`0xFFFF0001`** is the iteration file, present in all four containers. It divines to nothing and is
  read directly; see [01-dat-container.md](01-dat-container.md).
- **The high-res container** holds only render surfaces, in one contiguous id run, plus its own
  iteration file. It shares the portal id space rather than having one of its own.

## 7. What the shipped containers hold

Useful as a smoke test for a new reader: decode every id in a container and the counts below should
fall out. They are measured over the 2013 files, not copied from them.

| container | files | contents |
|---|---:|---|
| portal | 79,694 | 15,318 geometry objects, 5,935 setups, 2,066 animations, 4,521 palettes, 7,221 surface textures, 20,684 render surfaces, 6,152 surfaces, 436 motion tables, 786 waves, 772 environments, 14 singleton tables, 2,681 palette sets, 1,917 clothing tables, 4,131 degrade records, 179 scenes, 1 region, 2 keymaps, 190 sound tables, 40 enum mappers, 22 id mappers, 1 action map, 5 dual id mappers, 71 combat tables, 28 strings, 2,051 particle emitters, 4,248 physics scripts, 164 physics script tables, 1 master property table, 49 fonts, 2 property collections, and a handful of one-file placeholders for the unused material types |
| cell | 805,348 | 65,025 landblocks, 5,346 landblock-info records, 734,976 indoor cells |
| local English | 118 | 101 UI layouts, 15 string tables, 1 language-state record |
| high-res | 2,295 | 2,294 render surfaces |

Each count is one above or below a container total because of the iteration file.

The five material and mesh types (`RenderTexture`, `RenderMaterial`, `MaterialModifier`,
`MaterialInstance`, `RenderMesh`) have one placeholder file each at most, and nothing in the shipped
renderer consumes them. They are a next-generation renderer's asset types that never shipped, and a
rebuild can omit them entirely.

## 8. Where reimplementations differ

| topic | the client | common reimplementations |
|---|---|---|
| The localised-font range | `0x40001000`–`0x40FFFFFF`. | Often `0x40001000`–`0x400FFFFF`. An id above `0x40100000` is then mis-typed. Nothing shipped is in that span, so the difference is invisible until a server patches one in. |
| Divining a type | A search over the registered ranges in registration order, game layer first. | Attributes scanned by reflection, or a single flat table. Both lose the game-first precedence. |
| Routing to a container | Three booleans per type, consulted per request. | The high-res file as a separate database the caller queries first. Indistinguishable on shipped data. |
| An unknown id | Divines to nothing; the load fails and says so. | Throws, or returns null, depending on the caller. |
| Server-only types | Marked as such and never requested. | Usually not distinguished from client types. |
