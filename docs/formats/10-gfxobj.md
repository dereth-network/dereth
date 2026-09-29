# Geometry objects (`0x01xxxxxx`)

A geometry object is the client's mesh primitive: one indexed vertex array plus up to two independent
polygon soups over it — a **drawing** set, which is rasterised, and a **physics** set, which collides
— each indexed by its own BSP tree. It carries no transform and no material of its own. The surfaces
it names are a per-object array of [surface](13-palette-and-surfaces.md) ids, and a polygon references
a surface by *index into that array*, never by id.

A [setup](11-setup.md) assembles geometry objects into a jointed model; a
[scene](17-scene-and-particles.md) or an [environment](16-environment.md) places them directly.

**Provenance:** from the DAT format, which public readers document.

**Readers:** `dereth_assets::geometry` (`GfxObj`, the vertex array, polygons) and `dereth_assets::common`
(the BSP node reader, shared with environments). Pinned by `tags_render_as_four_characters` and
`an_unrecognised_tag_reads_as_a_childless_node` in `core/assets/src/common.rs`. The
whole-container check — every object decodes with the cursor landing exactly on the payload end — is
`dereth-assets`' `exhaustive_decode`, which needs the shipped data files and so is not a public-tier
test.

## 1. Layout

| field | type | meaning |
|---|---|---|
| id echo | `u32` | See [03-serialisation-primitives.md](03-serialisation-primitives.md). Not a categorised type, so no second dword. |
| `flags` | `u32` | Section 1.1. |
| `num_surfaces` | compressed | |
| `surfaces` | `u32 × n` | Surface ids. Polygons index **into this array**. |
| vertex array | — | Section 2. |
| physics polygons + physics BSP | — | Only when `flags & 0x1`. |
| `sort_center` | `f32 × 3` | Object-space point used to depth-sort the whole object. |
| drawing polygons + drawing BSP | — | Only when `flags & 0x2`. |
| `degrade_id` | `u32` | Only when `flags & 0x8`; a [degrade record](22-degrade-info.md). |

Note the ordering: the physics block comes **before** `sort_center` and the drawing block after it.

### 1.1 Flags

The value is recomputed when the file is written, so only four bits can ever appear:

| bit | meaning |
|---:|---|
| `0x1` | A physics polygon array and physics BSP are present. |
| `0x2` | A drawing polygon array and drawing BSP are present. |
| `0x4` | Never set and never read. No shipped file has it. |
| `0x8` | A degrade id follows at the end. |

Across the shipped portal container only four combinations occur: drawing alone, drawing with a
degrade id, drawing with physics, and all three. **Every shipped geometry object has drawing
geometry**; physics geometry is the exception, not the rule.

## 2. The vertex array

| field | type | meaning |
|---|---|---|
| `vertex_type` | `u32` | 0 = unknown, 1 = the software vertex below. Always 1 in shipped data. |
| `num_vertices` | `u32` | A plain dword — **not** a compressed count, unlike every other length in this format. |
| `vertices` | — | Read only when the type is 1. For type 0 the original allocates the array but neither reads nor initialises it; a reader should refuse the record, since nothing in the file says what the vertices are. |

Each vertex:

| offset | size | field | meaning |
|---:|---:|---|---|
| `+0` | 2 | `vertex_id` | The vertex's own index. Redundant — it equals the array position in all shipped data — but a byte-exact writer has to keep it. |
| `+2` | 2 | `num_uvs` | How many texture-coordinate pairs follow the normal. |
| `+4` | 12 | `origin` | Object-space position. |
| `+16` | 12 | `normal` | Object-space unit normal. |
| `+28` | 8 × `num_uvs` | `uvs` | `(u, v)` pairs. |

A vertex carries several UV pairs because one vertex can be shared by polygons that map different
surfaces onto it: a polygon's UV index selects *which* pair of its vertex to use. One pair is the
common case; the shipped maximum is 57. A vertex with **no** UV pairs belongs to a solid-colour,
untextured surface, and those exist — a reader that requires at least one pair rejects real files.

## 3. Polygons

| offset | size | field | meaning |
|---:|---:|---|---|
| `+0` | 2 | `polygon_id` | The polygon's own index within its array. |
| `+2` | 1 | `num_points` | 3 to 12 in shipped data; triangles and quads are 99.5% of them. |
| `+3` | 1 | `texture_mode` | Section 3.1. |
| `+4` | 4 | `sides_type` | Section 3.2. |
| `+8` | 2 | `pos_surface` | Index into the object's surface array, front face. |
| `+10` | 2 | `neg_surface` | Index for the back face; `0xFFFF` when unused. |
| `+12` | 2 × `n` | `vertex_ids` | Indices into the vertex array. |
| … | 1 × `n` | `pos_uv_indices` | Present **unless** `texture_mode & 0x4`. |
| … | 1 × `n` | `neg_uv_indices` | Present **only when** `sides_type == 2` and not `texture_mode & 0x8`. |

Two fix-ups the client applies after reading, which a reader has to reproduce:

- When `sides_type == 1`, the back face takes the **front face's surface and the front face's UV
  index array**. Omitting the UV half makes double-sided foliage render untextured from one side.
- The polygon's plane is computed from its vertices. It is **not** stored in the file.

### 3.1 `texture_mode`

| bit | meaning |
|---:|---|
| `0x1` | The front face uses **wrapped** texture addressing rather than clamped. |
| `0x2` | The same for the back face. |
| `0x4` | **No front UV array** in the file. |
| `0x8` | **No back UV array** in the file. |

Despite what the field is often called, there is no stipple pattern involved: bits 0 and 1 choose
wrap versus clamp for the sampler. Using clamp everywhere makes every tiling wall and floor show a
stretched row of texels at the seams — a highly visible difference, and an easy one to check.

Only three values occur in shipped data: none of the bits, bit 0 alone, and bit 2 alone. Bits 1 and 3
never occur, because the two-surface mode they depend on is nearly extinct.

### 3.2 `sides_type`

| value | meaning |
|---:|---|
| 0 | Single-sided. Clockwise culling; only the front surface is drawn. This is the ordinary case, used by nearly everything. |
| 1 | Double-sided. No culling; the back face uses the same surface and UVs. |
| 2 | Two-surface. Clockwise culling, drawn in **two** passes, front and back with their own surfaces and UV arrays. |

The triangle-count estimate doubles for values 1 **and** 2, but only value 2 issues two surface
passes. Value 2 appears on eleven polygons in the whole shipped container, which is few enough that
nobody has seen the two-pass path in situ.

## 4. The two BSP trees

Both blocks have the same shape:

```text
compressed  num_polygons
            polygon × num_polygons          # each is self-describing, sizes vary
            one BSP node (the root)
```

**The tree type is not stored in the file.** It is implied by position: the first block is the
physics tree, the second is the drawing tree, and environments add a third kind, the cell tree. The
type changes what a node's payload contains, so a reader must carry it down the whole recursion.
Getting it wrong silently mis-parses everything after the first node.

### 4.1 Node tags

A node starts with a four-byte tag, compared as a little-endian dword — so **the four characters
appear reversed on disk**.

| tag | node kind | children read |
|---|---|---|
| `PORT` | portal | positive **and** negative |
| `LEAF` | leaf | none |
| `BPnn` | interior | positive |
| `BPIn` | interior | positive |
| `BpIN` | interior | negative |
| `BpnN` | interior | negative |
| `BPIN` | interior | positive **and** negative |
| `BPnN` | interior | positive **and** negative |
| anything else | interior | none |

The naming is positional: character 1 is `P`/`p` for a positive child present or absent, character 2
is `I`/`n` for a non-empty or empty polygon list, character 3 is `N`/`n` for a negative child present
or absent. Two further tags occur in shipped data and fall through to the childless case:

- A terminal drawing node that carries only polygons. **Its splitting plane is written from
  uninitialised memory** — in practice sixteen `0xCD` bytes, which decode to a large negative float.
  It is never read back, because the node has no children. A validation pass that insists every float
  in the file is sane will reject legitimate files; skip the plane of a childless node.
- A terminal node of a cell-boundary chain, whose plane *is* written correctly but is never tested.

Both the client and the common reimplementations tolerate an unrecognised tag as a childless node,
and so should a rebuild.

### 4.2 Interior node payload

In this exact order, after the tag:

| size | field | condition |
|---:|---|---|
| 12 | plane normal | always |
| 4 | plane distance | always |
| var | positive child | the tag says so, or the tag is `PORT` |
| var | negative child | the tag says so, or the tag is `PORT` |
| 16 | bounding sphere (centre, radius) | **not** in a cell tree |
| 4 | `num_polys` | drawing tree only |
| 2 × n | polygon indices | drawing tree only |

So a cell node is plane and children only; a physics node adds a sphere; a drawing node adds a sphere
and a polygon list. The plane is `Ax + By + Cz + d`, positive side `> 0`.

### 4.3 Portal node

A portal extends the interior node. Both children are always read, with no tag inspection, and in a
drawing tree it adds a portal list:

| size | field |
|---:|---|
| 16 | plane |
| var | positive child |
| var | negative child |
| 16 | sphere (drawing only) |
| 4 | `num_polys` (drawing only) |
| 4 | `num_portals` (drawing only) |
| 2 × `num_polys` | polygon indices (drawing only) |
| 4 × `num_portals` | `(polygon index, portal index)` pairs (drawing only) |

The ordering trap: **both counts are read before either list.** Portals appear only in drawing trees,
and only in indoor objects — a few hundred nodes in the whole shipped container.

### 4.4 Leaf

| size | field | condition |
|---:|---|---|
| 4 | `leaf_index` | always |
| 4 | `solid` | physics only |
| 16 | sphere | physics only |
| 4 | `num_polys` | physics only |
| 2 × n | polygon indices | physics only |

A leaf in a **drawing** tree is therefore eight bytes — tag and index — and carries no geometry. In a
physics tree a non-zero `solid` marks the leaf as inside solid matter. When `solid` is zero the sphere
and polygon list are still written but mean nothing, and over half of the shipped physics leaves have
an empty polygon list.

### 4.5 What each tree is for

- **Drawing** — polygon ordering for the painter's algorithm, and portal culling of indoor geometry.
  A modern renderer does not need it for depth sorting, but it does need the portal nodes to reproduce
  the client's indoor culling.
- **Physics** — every collision query: collisions, sliding a sphere, stepping up and down, walkability,
  ray traces. Reproduce it verbatim if movement is meant to feel the same.
- **Cell** — a degenerate tree used only by environments: a **chain** of positive-child-only nodes
  ending in a terminal. A point is inside the cell when it is on the positive side of every plane in
  the chain, within a small tolerance. That makes a cell a convex polyhedron expressed as an
  intersection of half-spaces.

The shipped census is worth knowing because it tells you which paths are actually exercised: physics
trees use exactly two node kinds, an interior node with both children and a leaf, and one of the six
interior tags never occurs at all.

## 5. Where reimplementations differ

| topic | the client | common reimplementations |
|---|---|---|
| `vertex_id` | Read and kept. | Usually dropped, because the array index is the same thing in shipped data. A byte-exact writer needs it. |
| `polygon_id` | Read and kept as a field. | Often preserved as a dictionary key instead. |
| Vertex ids in a polygon | Unsigned 16-bit indices. | Sometimes signed. No practical difference below 65,534 vertices. |
| Polygon indices in a BSP node | Signed 16-bit, added to the array base. A corrupt file can index backwards, so bound-check. | Usually an unsigned list. |
| `sides_type` naming | A three-valued mode: single-sided, double-sided, two-surface. | Often named as a cull mode with four values, which misdescribes 0 and 2. |
| Binding polygons to vertices | The reader binds them as it goes. | Usually resolved lazily afterwards. Same result. |

## 6. A synthetic example

The smallest useful object: a single untextured triangle with one surface, no physics, and a drawing
tree that is one terminal node.

```text
id echo        0x01000001
flags          0x00000002        # drawing only
num_surfaces   01                # one compressed byte
surfaces[0]    0x08000001
vertex_type    0x00000001
num_vertices   0x00000003
vertex 0       id 0, num_uvs 0, origin (0,0,0),   normal (0,0,-1)
vertex 1       id 1, num_uvs 0, origin (1,0,0),   normal (0,0,-1)
vertex 2       id 2, num_uvs 0, origin (0,1,0),   normal (0,0,-1)
sort_center    (0.33, 0.33, 0.0)
num_polygons   01
polygon 0      id 0, num_points 3, texture_mode 0x04, sides_type 0,
               pos_surface 0, neg_surface 0xFFFF, vertex_ids 0,1,2
               (no UV arrays: texture_mode bit 2 is set)
BSP root       tag "BPOL" reversed on disk, 16 bytes of plane (ignored),
               sphere centre (0.33,0.33,0), radius 0.75,
               num_polys 1, in_polys [0]
```

The surface index count is one compressed byte, so the surface id that follows it starts at an odd
offset — which is the alignment rule of
[03-serialisation-primitives.md](03-serialisation-primitives.md) in one line.

Nothing above is taken from a shipped file.

## 7. Not the same thing

A separate mesh type exists in the type catalogue — an indexed-triangle mesh with vertex and index
buffers, fragments and material references, belonging to a later engine. **No shipped container holds
one**, and nothing in the game creates one. All of this game's geometry is the format above.
