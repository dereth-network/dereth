# Environments (`0x0Dxxxxxx`)

An **environment** is a library of interchangeable interior cell shapes. Each entry is a convex room:
a vertex array, the polygons that are drawn, the polygons that collide, the subset of drawing polygons
that are **portals** — doorways to neighbouring cells — and three BSP trees, one defining the convex
boundary, one for physics and one for drawing.

Dungeons and buildings are then assembled from cell records in the cell container, each of which says
"use cell shape *k* of environment *e*, at this transform, with these surfaces".

The geometry primitives are byte-identical to [geometry objects](10-gfxobj.md) — the same vertex
array, the same polygon, the same BSP node encoding — so read that page first.

**Provenance:** from the DAT format, which public readers document.

**Readers:** `dereth_assets::geometry` (`Environment`) over `dereth_assets::common`. Pinned for the BSP
side by `tags_render_as_four_characters` and `an_unrecognised_tag_reads_as_a_childless_node` in
`core/assets/src/common.rs`.

## 1. Layout

| field | type |
|---|---|
| id echo | `u32` |
| `num_cell_structs` | `u32` |
| cell shapes | var |

There is no index and no offset table. Cell shapes are variable-length and must be read in order.

## 2. A cell shape

| size | field | notes |
|---:|---|---|
| 4 | `cell_struct_id` | Its own index within the environment. |
| 4 | `num_polygons` | Drawing polygons. |
| 4 | `num_physics_polygons` | |
| 4 | `num_portals` | |
| var | vertex array | `u32` type (always 1), `u32` count, then the vertices. |
| var | drawing polygons | |
| 2 × n | portal indices | **Indices into the drawing polygons**, not ids. |
| 0–3 | pad to 4 | |
| var | cell BSP | Tree type: cell. |
| var | physics polygons | |
| var | physics BSP | Tree type: physics. |
| 4 | has-drawing flag | Non-zero means a drawing tree follows. |
| var | drawing BSP | Tree type: drawing. Only when the flag is set. |
| 0–3 | pad to 4 | |

**All four counts come first, before any of the data.** That is the main structural difference from a
geometry object, where each array carries its own count immediately before it — and these counts are
plain dwords, not the compressed form. A reader that copies the geometry object's "count, then array"
rhythm desynchronises on the very first cell shape.

**Both alignment points matter.** The portal index list is `2 × num_portals` bytes and therefore leaves
the cursor odd whenever the portal count is odd; and each cell shape is padded at its end, which is
what keeps the *next* shape aligned.

### 2.1 Which polygon array a tree indexes

The three trees do not all index the same array:

| tree | polygon base |
|---|---|
| cell | the drawing polygons |
| physics | the physics polygons |
| drawing | the drawing polygons |

So the base switches to the physics array for the middle tree and back again for the last one. As in a
geometry object, the tree type is implied by position and must be carried down the whole recursion.

### 2.2 The cell tree

The cell BSP is a degenerate chain: interior nodes with a positive child only, terminated by a
childless node. A point is inside the cell when it is on the positive side of every plane in the
chain, within a small tolerance. Cell nodes carry **no bounding sphere and no polygon list** — see the
node payload table in [10-gfxobj.md](10-gfxobj.md).

A cell shape with **no** drawing tree is fully valid: it is collision-only geometry, and a few exist.

### 2.3 Portals

The portal list names drawing polygons: the same polygons are both drawn and used as openings. At
runtime the cell record pairs each portal polygon with the neighbouring cell it leads to; that pairing
is in the cell record, not here. The drawing tree's portal nodes carry their own polygon-and-portal
list, used for view-frustum culling.

## 3. How a cell record refers to one

The reference is **two 16-bit indices, not ids**:

```text
u32  id echo
u32  flags                      # bit 1: static objects follow; bit 3: a restriction id follows
u32  cell_id                    # the id again
u8   num_surfaces
u8   num_portals
u16  num_visible_cells
u16 × num_surfaces  surface indices     -> surface id     = 0x08000000 | index
u16                 environment index   -> environment id = 0x0D000000 | index
u16                 cell shape index    -> the ordinal within that environment
     transform                  # 28 bytes, not padded: unaligned when num_surfaces is odd
     8 × num_portals            portal records
     2 × num_visible_cells      visible-cell indices
     ...
```

The surface list is per **cell record**, not per environment. The same cell shape is reused across many
cells with different surface lists — that is the whole point of the format, and it is why a polygon
stores a surface *index* rather than an id.

## 4. Where reimplementations differ

| topic | the client | common reimplementations |
|---|---|---|
| Cell shapes | An array, indexed by ordinal. | Often a map keyed by the stored id. Equivalent on shipped data, where the ids are `0 … n-1`. |
| Portal indices | Resolved to polygons at load. | Usually kept as plain indices. |
| The two alignment points | Both applied. | Both applied in the well-known readers — this is one of the few places everyone agrees. |

## 5. A synthetic example

One environment with one cell shape: six vertices, five drawing polygons, one physics polygon, four
portals.

```text
id echo               0x0D000001
num_cell_structs      1

cell_struct_id        0
num_polygons          5
num_physics_polygons  1
num_portals           4
vertex_type           1
num_vertices          6
  vertex 0            id 0, num_uvs 1, origin (-5,-5,6), normal (0.707,-0.707,0), uv (0.5, 0.0)
  vertex 1 .. 5       ...
drawing polygons      5 records
portal indices        4 * u16 = 8 bytes      # even, so no padding here
                                             # an odd count would need two pad bytes
cell BSP              a chain of positive-child-only nodes ending in a childless one
physics polygons      1 record
physics BSP           an interior node and two leaves
has-drawing flag      1
drawing BSP           ...
pad                   to 4
```

Nothing above is taken from a shipped file.
