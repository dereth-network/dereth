# Clothing tables (`0x10xxxxxx`)

A **clothing table** describes one wearable item's appearance across every body it can be worn on. It
has two halves:

- **clothing bases**, keyed by [setup](11-setup.md) id: a list of "on part *n*, use geometry *g*, and
  swap texture *old* for texture *new*". This is what makes one shirt render correctly on bodies with
  entirely different part layouts.
- **palette templates**, keyed by a small integer — the item's dye template: a list of "replace palette
  entries `[start, start + length)` with the palette that the wearer's shade selects from palette set
  *p*". This is the dye system.

The output of both halves is an [appearance record](13-palette-and-surfaces.md), which is then applied
to the live object.

**Provenance:** from the DAT format, which public readers document.

**Readers:** `dereth_assets::motion` (`ClothingTable`), whose header is
`dereth_dat::packobj::packable_hash_table_header`. Pinned by `packable_hash_table_header_splits_one_dword`
and `the_packobj_header_is_not_an_archive_header` in `core/dat/src/packobj.rs`.

## 1. The header both halves use

Each half begins with the packable hash-table header of
[03-serialisation-primitives.md](03-serialisation-primitives.md): **one dword, the entry count in the
low half and the bucket count in the high half**. A zero bucket count means the table is empty; each
count, being 16 bits, is at most `0xFFFF`.

The bucket count is a hint for the runtime hash and does not affect parsing — but it is real file
content, and a byte-exact repacker has to preserve it. In shipped tables it is 8 for the bases and 32
for the templates.

## 2. Layout

```text
id echo                                     # 0x10xxxxxx
u32   header                                # bases: count | buckets << 16
count × {
    u32  setup_id
    u32  num_object_effects
    num_object_effects × {
        u32  part_index
        u32  geometry_id
        u32  num_texture_effects
        num_texture_effects × { u32 old_texture_id ; u32 new_texture_id }
    }
}
u32   header                                # templates: count | buckets << 16
count × {
    u32  template_key
    u32  icon_id                            # 0 when the item has no per-colour icon
    u32  num_subpalette_effects
    num_subpalette_effects × {
        u32  num_ranges
        num_ranges × { u32 range_start ; u32 range_length }
        u32  palette_set_id                 # AFTER the ranges
    }
}
```

There is no trailing alignment; everything is 4-byte.

**Two ordering traps in the subpalette effect**: the range pairs are **interleaved** on disk, although
they are two parallel arrays in memory, and the palette-set id comes **after** the ranges rather than
before them.

| field | meaning |
|---|---|
| `setup_id` | The body this base applies to. |
| `part_index` | Which part of that setup the effect replaces. Becomes a part change in the appearance record. |
| `geometry_id` | The mesh to put on that part. |
| `old_texture_id`, `new_texture_id` | A texture change on that part: wherever the part's mesh uses the first, use the second. |
| `template_key` | The dye template id. Ninety distinct values are in use. |
| `range_start`, `range_length` | A palette range **in palette entries**, 0 to 2048 — *not* the byte-scaled form the wire record uses. Every shipped value is a multiple of 8. |
| `palette_set_id` | A [palette set](13-palette-and-surfaces.md) from which the wearer's shade picks a palette. |

**The two scales are the thing to keep straight.** In this file a range is an absolute palette-entry
index up to 2048. In the appearance record on the wire the same range is divided by eight to fit in a
byte and multiplied by eight again on the way back. That is why every value here is a multiple of
eight, and it is why a reader that carries the wire scale into this file produces ranges an eighth of
their proper size.

Identity texture effects — old and new equal — exist in shipped data and are common filler. Do not
filter them out: the appearance record's duplicate-removal depends on the full list for ordering.

## 3. Building the appearance record

```text
base = bases[wearer's setup id]
if absent:
    setup_id = a hard-coded default for the wearer's race and sex
    base = bases[setup_id]
    if still absent: fail

for each object effect in the base:
    add a part change  { part_index, geometry_id }
    for each texture effect:
        add a texture change { part_index, old_texture_id, new_texture_id }

if template_key != 0:
    tpl = templates[template_key]
    if absent: fail
    for i, effect in enumerate(tpl.subpalette_effects):
        shade = the item's shade i, clamped to the last available
        sub   = the palette that shade selects from effect.palette_set_id
        for each range in effect:
            add a subpalette { sub, range_start, range_length }
```

Three behaviours matter here:

- **A template key of zero means "no dye"**: only the object and texture changes are applied, and the
  template table is not consulted.
- **The shade list holds four values**, and the accessor **clamps**: the fifth and later subpalette
  effects all reuse the fourth shade. Over a hundred shipped templates have six subpalette effects and
  therefore exercise the clamp — a reimplementation that indexes past the end either crashes or dyes
  the extra ranges with garbage.
- **The per-race fallback is client-only.** When the wearer's setup is not in the table, the client
  substitutes a hard-coded default for that race and sex from a chain of some thirty setup ids. A
  rebuild without it renders several playable races unclothed.

The shade values come from the item's per-slot properties sent by the server, not from this file.

## 4. Where reimplementations differ

| topic | the client | common reimplementations |
|---|---|---|
| The packed header | Both counts read from one dword. | The same. |
| The interleaved ranges | As on disk. | The same. |
| The per-race fallback | A long chain of hard-coded setup ids. | Absent — it is client behaviour, not file content, and a reader has no reason to have it. |
| The shade clamp | The fifth and later effects reuse the fourth shade. | Not modelled. |

## 5. A synthetic example

A bare part swap with no dye, and a whole-palette dye template:

```text
id echo              0x10000001
bases header         0x00080001        # 1 entry, 8 buckets
  setup_id           0x02000001
  num_object_effects 1
    part_index       16
    geometry_id      0x01000001
    num_texture_effects 0
templates header     0x00200000        # 0 entries, 32 buckets
```

```text
id echo              0x10000002
bases header         0x00080001
  setup_id           0x02000002
  num_object_effects 0                 # a base that changes no parts is legal
templates header     0x00200001        # 1 entry, 32 buckets
  template_key       4
  icon_id            0
  num_subpalette_effects 1
    num_ranges       1
    range_start      0
    range_length     2048              # the entire palette
    palette_set_id   0x0F000001        # after the ranges
```

Nothing above is taken from a shipped file.
