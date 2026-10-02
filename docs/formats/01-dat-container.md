# The `.dat` container

Every asset the 2013 client reads — models, textures, animations, cells of the world, UI layouts,
strings, gameplay tables — lives in one of four container files that ship with the game:
`client_portal.dat`, `client_cell_1.dat`, `client_local_English.dat` and `client_highres.dat`. All
four use one on-disk format: a fixed-size block allocator with a free list, and a B-tree directory
that maps a 32-bit **file id** to the first block of a chain. The format is writable — the client
patches its own data files in place from the server — and every structural change is journalled into
a single 64-byte slot so that an interrupted write can be replayed on the next open.

**Provenance:** from the DAT format, which public readers document.

**Readers:** `dereth_dat::container` (one file), `dereth_dat::btree` (the directory), `dereth_dat::store`
(the four files behind one lookup), `dereth_dat::write` (writing and the journal), `dereth_dat::inflate`
(the zlib decompressor, for compressed data arriving from the server; the container reader itself
refuses a compressed entry, see section 5). Pinned by `node_size_is_0x6b4`,
`bt_entry_fields_match_the_documented_worked_example` and
`an_impossible_entry_count_is_an_error_not_a_truncation` in `core/dat/src/btree.rs`.

All integers are little-endian, and reads are unaligned; see
[03-serialisation-primitives.md](03-serialisation-primitives.md).

## 1. The prologue

| offset | size | content |
|---:|---:|---|
| `0x000` | 256 | Text banner. Nothing validates it; in the shipped files it is 256 zero bytes. |
| `0x100` | 64 | The transaction journal slot: one serialised journal record (section 6). |
| `0x140` | 80 | The header (section 2). |
| `0x190` | 624 | Unused, zero. |
| `0x400` | — | The first block. The data area is an array of blocks from here to the end of the file. |

A read-only reader needs the 80 bytes at `0x140` and nothing else from the prologue.

## 2. The header

80 bytes at `0x140`. The magic must be `0x5442`; a file whose magic does not match is refused rather
than guessed at.

| offset | size | field | meaning |
|---:|---:|---|---|
| `0x140` | 4 | `magic` | `0x5442`. |
| `0x144` | 4 | `block_size` | Block size in bytes **including** the 4-byte link. 1024 in the portal, local and high-res files; 256 in the cell file. |
| `0x148` | 4 | `file_size` | Total file size. Equal to the on-disk size in all four shipped files. |
| `0x14C` | 4 | `data_set` | 1 = portal, 2 = cell, 3 = local. |
| `0x150` | 4 | `data_subset` | Portal 0; cell = region number; local = language number; the high-res file uses the four ASCII bytes `HiFi` as a marker. |
| `0x154` | 4 | `free_head` | Offset of the first free block. |
| `0x158` | 4 | `free_tail` | Offset of the last free block. |
| `0x15C` | 4 | `free_count` | Number of free blocks. Equal to the length of the walked free chain in all four shipped files. |
| `0x160` | 4 | `btree_root` | Offset of the root directory node. |
| `0x164` | 4 | `new_lru` | On-disk LRU list head. The client never enables the LRU. |
| `0x168` | 4 | `old_lru` | On-disk LRU list tail. |
| `0x16C` | 1 + 3 | `use_lru` | Zero in all four shipped files. The three padding bytes are `0xCD` — uninitialised stack that the tool which built the files copied to disk. A reader must not assume they are zero. |
| `0x170` | 4 | `master_map_id` | The portal file names its enum-to-id map here; zero in the other three. |
| `0x174` | 4 | `engine_pack_version` | 110 in the portal, local and high-res files; 22 in the cell file. |
| `0x178` | 4 | `game_pack_version` | 0 in all four. |
| `0x17C` | 16 | `version_guid` | A GUID; all four shipped files carry the same one. It is a Win32 GUID structure, not a byte string, so its first three fields are little-endian words: a reader that prints it left to right as bytes gets a different-looking GUID from one that parses the structure. Nothing in the client compares it. |
| `0x18C` | 4 | `version_minor` | Identical in all four shipped files. |

The client checks the pack versions on open, but the check cannot fail in the shipped client: the two
engine values it compares against are negative, which disables that half of the test, and the two
game values are zero, which is what the headers carry. A reimplementation is free to make the check
strict; it will then reject nothing.

The 64-bit identity a container has in the patching protocol is `(data_set << 32) | data_subset`.
That is the key the server uses, and it keeps the four files distinct even though the portal and
high-res files share a `data_set`.

## 3. Blocks and chains

The first dword of every block is a link to the next block of the same chain:

| link value | meaning |
|---|---|
| `0` | Last block of a file. |
| non-zero, bit 31 clear | Offset of the next block of this file. |
| bit 31 set, low bits non-zero | The block is **free**; the low 31 bits are the next free block. |
| `0x80000000` | Last free block. |

A payload of `n` bytes occupies `ceil(n / (block_size - 4))` blocks. Every block carries
`block_size - 4` payload bytes except the last, which carries the remainder. Blocks are not
contiguous in general: the shipped files are mostly sequential because they were written once, but
any file that has since been patched has a chain that jumps.

Reading a payload is one seek-and-read per block:

```text
remaining = entry.size
offset    = entry.first_block
while remaining > 0 and offset != 0:
    n    = min(remaining, block_size - 4)
    read the link and n payload bytes at offset      # one read, link first
    if the link has bit 31 set: fail                 # walked into the free list
    offset     = link
    remaining -= n
ok = (remaining == 0 and offset == 0)
```

Two failure modes are worth building in from the start, because damaged containers exist: a chain
that walks into a free block, and a chain that ends before the entry's size is satisfied. Both are
errors, not truncations.

Allocation pops blocks from the head of the free list and deallocation appends to the tail, so a
record's blocks are exactly the first *n* blocks of the free list in free-list order. Growing a file
appends a megabyte at a time, pre-linked into the free chain.

## 4. The directory

### 4.1 The entry (24 bytes)

| offset | size | field | meaning |
|---:|---:|---|---|
| `0x00` | 4 | flags | Bit 0: the payload is compressed (section 5). Bits 1–15: reserved, zero. Bits 16–31: the object pack version of this payload — see [03-serialisation-primitives.md](03-serialisation-primitives.md). |
| `0x04` | 4 | `id` | The file id. Entries are sorted ascending by it. |
| `0x08` | 4 | `first_block` | Offset of the first block of the chain. |
| `0x0C` | 4 | `size` | Payload size in bytes: the uncompressed size when the compression bit is clear, the stored size when it is set. |
| `0x10` | 4 | `date` | Unix time of the write. |
| `0x14` | 4 | `iteration` | The patch iteration that last changed this file. |

Across the shipped files the compression bit is clear for every entry, the reserved bits are always
zero, and the pack version is 1 for the iteration file, 2 for files never patched, and 3 for patched
files.

### 4.2 The node (1716 bytes)

| offset | size | field | meaning |
|---:|---:|---|---|
| `0x000` | 62 × 4 | children | Child node offsets. **A zero in child 0 marks a leaf** — that, and not a flag or a depth field, is how a reader knows to stop descending. |
| `0x0F8` | 4 | count | Number of used entries, 0–61. |
| `0x0FC` | 61 × 24 | entries | Sorted ascending by id. |

So the directory is a B-tree of order 62: at most 61 keys and 62 children per node. Child *i* holds
the keys below entry *i*, and child `count` holds the keys above the last entry.

A node is read through the ordinary block walk, so a node can and does span blocks — two blocks in
the 1024-byte files, seven in the 256-byte cell file. A reader that assumes a node is one block works
on the portal file and fails on the cell file.

A count above 61 cannot occur in a well-formed file. Treat it as an error: a reader that clamps it
instead will read entries out of the child array without noticing.

### 4.3 Lookup

```text
node = root
if node.count == 0 or id == 0: not found
loop:
    binary search node.entries[0 .. count) for id
    hit  -> return the entry
    miss -> i = the insertion index
            if node.children[0] == 0: not found        # leaf
            node = load(node.children[i])
```

In every shipped file the in-order key walk is strictly increasing and all leaves are at the same
depth, which is what makes the binary search safe to trust.

## 5. Compression

A compressed payload is `[uncompressed_size: u32][zlib stream]` — a zlib wrapper around DEFLATE, not
a bare DEFLATE stream. Nothing in the shipped files is stored compressed, and the patch path stores
server data decompressed, so a reader may treat the compression bit as unsupported. It must still
*parse* the bit, because it shares a dword with the pack version.

## 6. Writing and the journal

Writing is only needed by a client that accepts data patches from a server. Three rules matter:

- **Never downgrade.** A write whose iteration number is older than the stored entry's is refused.
- **A write with pack version zero is refused.**
- **Every structural mutation is journalled first.** The operation serialises one record into the
  64-byte slot at `0x100`, performs the change, and then writes a "no transaction" record back. On
  open, a slot whose record is not "no transaction" is re-executed and then cleared.

The journal record is byte-packed, with no alignment padding:

| offset | size | content |
|---:|---:|---|
| 0 | 1 | Record type. |
| 1 | 4 | Magic. |
| 5 | … | Type-specific payload. |

There are ten record types: no transaction, add object, delete from a leaf, delete from an internal
node, merge nodes, update object, split node, rotate entry, and two LRU records that the client never
writes and does not replay. The bytes after a short record are the stale tail of whatever longer
record was written before it, so **a reader must trust only the leading type byte and the magic**.
The shipped portal file still carries the tail of an add-object record behind its no-transaction
record.

Rolling back a half-written chain means walking it from its first block, flagging every block free
again and re-linking the free head — which is why the free-list head is part of what the journal
records.

## 7. The iteration file

Every container holds a file with id `0xFFFF0001` whose payload is the set of patch iterations
applied to it. It is not an ordinary object: it has no type, and it is read directly, before any type
machinery exists.

| offset | size | content |
|---:|---:|---|
| 0 | 4 | Total number of iterations in the set. |
| 4 | 4 × n | Items. A non-negative dword is a single value. A **negative** dword `-k` is followed by a dword `first` and expands to the run `first, first+1, …, first+k-1`. |

A single value is written with bit 31 masked off, so a reader restores bit 31 when bit 30 of a
single value is set.

On writing, runs of three or more consecutive values are collapsed into the two-dword form and
shorter runs are written one value at a time. In every shipped file the whole set is a single run
from 1 to the total, so the payload is twelve bytes.

A container without this file is unusable: the client reports an error and closes it rather than
proceeding.

## 8. The four files, and which one answers

The client opens up to four containers and routes a request to one of them by the **type** of the id
(see [02-file-ids-and-types.md](02-file-ids-and-types.md)):

| slot | file | opened | block size |
|---:|---|---|---:|
| 0 | `client_portal.dat` | at start-up | 1024 |
| 1 | `client_local_<language>.dat` | when the language is set | 1024 |
| 2 | `client_cell_<region>.dat` | when the server tells the client its region | 256 |
| 3 | `client_highres.dat` | only when the server's product flags ask for it | 1024 |

Portal types go to slot 0, cell types to slot 2, local types to slot 1. For a portal type, **the
high-res file is asked first** when it is open and holds the id.

That ordering is worth implementing, but it is not an override mechanism in practice: the high-res
file is a *disjoint partition* of the portal id space. Its texture ids and the portal file's do not
collide at all, so an implementation that resolves conflicts the other way round can never be caught
out by shipped data.

The client looks for the files in the directory it was given, then the current directory, then an
environment variable, then a registry key.

## 9. Where reimplementations differ

| topic | the client | common reimplementations |
|---|---|---|
| The entry's first dword | A bitfield: the compression bit plus the pack version, and the pack version selects how the payload is unpacked. | Read and discarded as opaque flags — which works only because nothing shipped is compressed. |
| Compressed payloads | Supported. | Usually not supported. |
| The journal at `0x100` | Replayed on open. | Ignored. Writing to a container without journalling can leave a file the original client will not open. |
| The high-res file | Same id space, asked first. | A separate database consulted first by callers. Indistinguishable in practice; see section 8. |
| `data_subset` | Region, language, or the `HiFi` marker. | Read and unused. |

## 10. A synthetic example

A seven-block container with a 512-byte block size (508 payload bytes per block) holding one
900-byte file with id `0x0A000001`, written by iteration 7. The root node is 1716 bytes, so it
alone takes four blocks (`0x400`, `0x600`, `0x800`, `0xA00`); the file takes two (`0xC00`, `0xE00`);
one block (`0x1000`) is free. The header at `0x140` reads:

```text
magic        0x5442
block_size   0x00000200
file_size    0x00001200      # 0x400 + 7 * 0x200
data_set     1
data_subset  0
free_head    0x00001000      # the seventh block is the only free one
free_tail    0x00001000
free_count   1
btree_root   0x00000400
```

The root node, read through its chain `0x400 -> 0x600 -> 0x800 -> 0xA00`, is a leaf — child 0 is
zero — with count 1 and one entry:

```text
flags        0x00020000      # pack version 2, not compressed
id           0x0A000001
first_block  0x00000C00
size         0x00000384      # 900
date         <unix time>
iteration    7
```

900 bytes at 508 payload bytes per block is two blocks, so the chain runs `0xC00 -> 0xE00`, with a
zero link in the second block. The free block at `0x1000` has link `0x80000000`.

Nothing above is taken from a shipped data file: it is a minimal container these rules permit,
written out longhand.

## 11. The layout before Throne of Destiny

Before Throne of Destiny (June 2005) the game shipped two files, `portal.dat` and `cell.dat`, in an
older layout. Blocks, chains, the bit-31 free flag, the first block at `0x400`, the order-62
directory and its lookup are the same; the header, the entry and the iteration are not. The layout
changed exactly once, with the renaming, so a file's layout is told by where its magic sits: at
`0x140` it is the layout above, at `0x12C` it is this one.

**Readers:** `dereth_dat::container` (`ContainerEra::PreTod`, `DatFile::header_iteration`),
`dereth_dat::btree` (`BtNode::parse_pre_tod`), `dereth_dat::store`
(`RetailDatStore::open_pre_tod_dir`). The layout is read, never written.

| offset | size | field | meaning |
|---:|---:|---|---|
| `0x000` | 300 | — | Zero. There is no transaction journal. |
| `0x12C` | 4 | `magic` | `0x5442`. |
| `0x130` | 4 | `block_size` | As above: 1024 in the portal file, 256 in the cell file. |
| `0x134` | 4 | `file_size` | Total file size. |
| `0x138` | 4 | `iteration` | The whole file's iteration (2112 in the February 2005 portal file, 1593 in its cell file). |
| `0x13C` | 4 | `free_head` | Offset of the first free block. |
| `0x140` | 4 | `free_tail` | Offset of the last free block. |
| `0x144` | 4 | `free_count` | Number of free blocks. |
| `0x148` | 4 | `btree_root` | Offset of the root directory node. |
| `0x14C` | 12 | — | Three words, zero in every shipped file. |

There is no data set, subset, pack version or version stamp: which file is which comes from its name
and block size.

A directory entry is **12 bytes**: `id`, `first_block`, `size`. There is no flags word (so no
compression bit and no pack version), no date and no per-entry iteration. A node is therefore
`62 × 4 + 4 + 61 × 12` = 984 bytes: one block in the portal file, four in the cell file. Child slots
past the count hold `0xCD` fill and are never followed.

There is no `0xFFFF0001` iteration file; the header's word is the iteration. Iterations restarted at
Throne of Destiny, so an older file's iteration is not comparable with a later file's.

The records that later moved to the language file (string tables, interface layouts) are in the
portal file, so a reader routes language-type requests to it; there is no high-resolution file.
