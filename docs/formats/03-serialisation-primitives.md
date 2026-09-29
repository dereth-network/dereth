# The object header and the serialisation primitives

Every payload in a container starts with the same few bytes, and every record in every format is
built out of the same small set of primitives: a compressed integer, two string forms, three
hash-table headers, and exactly one alignment rule. Getting any of them wrong produces plausible
garbage rather than an error, so they are worth reading once before any of the per-format pages.

**Provenance:** from the DAT format, which public readers document.

**Readers:** `dereth_dat::cursor` (the byte cursor and every primitive), `dereth_dat::archive` (the
hash-table headers, the version header, the narrow-string code page), `dereth_dat::packobj` (the
packed-object headers), `dereth_assets::dbobj` (the object header). Pinned by the ten tests in
`core/dat/src/cursor.rs`, the five in `core/dat/src/archive.rs`, the three in
`core/dat/src/packobj.rs` and the three in `core/assets/src/dbobj.rs` —
notably `no_alignment_is_applied_by_ordinary_reads`,
`compressed_u32_round_trips_at_the_three_encoding_boundaries`,
`the_two_archive_hash_headers_are_different` and `the_id_echo_check_fires_on_a_mismatch`.

## 1. The rule that governs everything: nothing is aligned

**A payload is not word-aligned.** The client has an alignment facility, it is opt-in per stream, and
the container path never switches it on. Every alignment check inside a type's reader is therefore
inert, and multi-byte fields are read at whatever offset they land on.

This is directly observable: in an indoor cell record, the cell's transform — seven floats — starts
at payload offset 42 whenever the surface count is odd, so the client performs unaligned float loads.
x86 allows it. A reimplementation that casts a byte slice to a float array, or that "helpfully"
aligns the cursor, corrupts every record with an odd-sized field ahead of it.

The only padding that exists is padding a **reader asks for**, at a point the reader chooses:

```text
align4: skip (-position) & 3 bytes
align2: skip  (-position) & 1 byte
```

The position is measured from the start of the payload, which is itself 4-aligned, so the offset and
the absolute address agree. Where a format needs padding — after a string, after a list of 16-bit
values — the per-format page says so explicitly. Nowhere else.

Every decoder should end with the same assertion: **the cursor lands exactly on the end of the
payload**, with nothing left over and nothing missing. That single check, run over every file in
every container, is what turns a plausible reader into a correct one.

## 2. The object header

| offset | size | field | meaning |
|---:|---:|---|---|
| `0x00` | 4 | `id` | The file id, echoed. It must equal the id that was requested; a mismatch fails the load. |
| `0x04` | 4 | `category` | **Only** for the three categorised types — surface texture, render surface, render texture. See [02-file-ids-and-types.md](02-file-ids-and-types.md). |

That is the whole header: one or two little-endian dwords, then the type's own fields. The id echo is
worth checking rather than skipping; it catches a routing bug on the first file rather than the
thousandth.

## 3. The compressed integer

A variable-length unsigned integer with three encodings, chosen by the top bits of the first byte:

| first byte | total size | value |
|---|---:|---|
| `0xxxxxxx` | 1 | The byte itself, `0`–`0x7F`. |
| `10xxxxxx` | 2 | `((b0 & 0x3F) << 8) + b1` — **big-endian**, up to `0x3FFF`. |
| `11xxxxxx` | 4 | `((b0 & 0x3F) << 24) + (b1 << 16) + u16le(b2, b3)` — the top two bytes big-endian, the low 16 bits a native little-endian `u16`. |

The hybrid byte order in the four-byte form is deliberate, not a transcription error: the writer
stores the high half a byte at a time and the low half as a machine word. The representable range
tops out at `0x3FFFFFFF`.

Encoders must use the shortest form: a reader that accepts a padded encoding will silently accept
files no writer produces, and a writer that emits a long form produces files the client rejects.

## 4. Strings

There are two string forms in the containers and they are **not** interchangeable.

### 4.1 The archive form

```text
[compressed length][that many bytes]
```

No terminator, no padding at all. The bytes are **Windows-1252**, not UTF-8 and not Latin-1: the
client renders them through the Windows ANSI code page. Thirty-two byte values in `0x80`–`0x9F`
differ from Latin-1, and those are exactly the ones that appear in item names — the curly quotes and
the dashes. A reader that assumes Latin-1 produces mojibake in precisely the places a player notices.

Localised text uses the same shape with a wide payload: **a compressed *character* count** followed
by that many UTF-16LE code units. No BOM, no terminator, no padding. The unit of the length differs
between the two — characters here, bytes above — which is the single most common mistake in reading
the string tables.

### 4.2 The packed-object form

```text
[u16 length]                      # 0xFFFF escapes to a u32 length that follows
[payload bytes]
[zero padding to a 4-byte boundary]
```

Two quirks of the client's reader have to be reproduced, because shipped data relies on them:

- A payload of exactly one NUL byte is the **empty string**.
- A trailing NUL inside a longer payload **shortens the string by one**.

A reader that keeps the NUL produces strings that compare unequal to everything, and one that strips
NULs generally loses a character from strings that legitimately end in one.

## 5. Ids relative to a known type

Where a record refers to a file of a type the reader already knows, the id is stored as a delta from
the type's base:

| first `u16` | size | value |
|---|---:|---|
| bit 15 clear | 2 | `base + delta`. |
| bit 15 set | 4 | `base + ((delta & 0x3FFF) << 16) + u16le(next)`. Bit 14 of the first `u16` is discarded. |

A packed delta of zero means the base itself, which is how the writer encodes "no id".

## 6. The three hash-table headers

Three different headers exist and they read different numbers out of the same bytes. This is the
trap: all three are short, all three parse without error against the wrong data, and the divergence
only shows up hundreds of bytes later.

| form | header | notes |
|---|---|---|
| Intrusive hash **table** | `u8` **index** into a table of 23 bucket sizes, then a compressed element count. | The sizes are the primes 11, 23, 47, 89, 191, 383, 761, 1531, 3067, 6143, 12281, 24571, 49139, 98299, 196597, 393209, 786431, 1572853, 3145721, 6291449, 12582893, 25165813, 50331599. An index of 23 or more is an error. |
| Intrusive hash **list** | A compressed **bucket count**, then a compressed element count. | Stores the count itself, not an index. |
| Packable hash table | **One `u32`**: the element count in the low half, the bucket count in the high half. | The only form that packs both numbers into a single dword, so each is at most `0xFFFF`. |

One shipped file uses two of these forms in the same record — the input map uses the list form for
its maps and the table form for its conflict table — so "which header does this file use" is a
per-field question, not a per-file one.

The plain list is simpler: a `u32` count and then the elements in order, with no alignment of its
own.

## 7. Pack versions

The directory entry carries a **pack version** for each payload (see
[01-dat-container.md](01-dat-container.md)), and the reader turns it into three version tokens that a
type's reader may query:

| token | value |
|---|---|
| `Core` | 1 for pack version 1, 2 for versions 2 and 3. |
| `DObj` | The pack version itself. |
| `UIL ` | 1 only for pack version 3. |

Shipped values are 1 for the iteration file, 2 for files untouched since the game shipped, and 3 for
files that have been patched. A pack version of zero fails the load outright. The UI-layout reader is
the main consumer of the third token (the UI layout format has no page here yet).

A stream may also carry its own version header: a `u32` whose bit 31 clear means "this is simply the
core version", and whose bit 31 set means the low 30 bits are an offset at which a full token table
lives. A stream carrying nothing but the current core version stores four bytes.

## 8. Two families of reader

Most of the older formats — geometry, setups, animations, palettes, surfaces, environments, scenes,
regions, motion tables, waves, sound tables, clothing, degrade records, particle emitters, physics
scripts, landblocks and cells — do not use the archive machinery at all past the header. After the id
echo the reader is a raw pointer walk with hand-written padding. That is why the per-format pages
below read as plain structure dumps with explicit `align4` calls.

The newer table formats — string tables, UI layouts, enum mappers, id mappers, the master property
table, property collections, fonts, the language state and the action map — read through the archive
machinery and use the compressed integer, the archive string form and the hash headers instead.

Knowing which family a format belongs to tells you immediately which string form and which padding
rule to expect.

## 9. Where reimplementations differ

| topic | the client | common reimplementations |
|---|---|---|
| Alignment | Off; padding only where a reader asks. | Sometimes inferred from field widths, which works until the first odd-length field. |
| Narrow strings | Windows-1252. | Often ASCII or Latin-1. Invisible until a name contains a curly apostrophe. |
| Wide strings | A **character** count. | Sometimes read as a byte count, which halves every localised string. |
| The packed-string NUL quirks | Reproduced. | Usually not; the difference is one trailing character. |
| Hash headers | Three distinct forms. | Frequently one form applied everywhere. |
| The id echo | Checked, and a mismatch fails the load. | Usually skipped. |
