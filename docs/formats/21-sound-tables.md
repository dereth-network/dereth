# Waves (`0x0Axxxxxx`) and sound tables (`0x20xxxxxx`)

A **wave** is one raw audio clip: a Win32 `WAVEFORMATEX` header and a block of samples, with **no RIFF
container around it**.

A **sound table** maps a sound type — a semantic slot such as "attack 1", "open", "ambient 3" — to one
or more wave ids with a priority, a probability and a volume, so an object can be told "make your
attack noise" without the caller knowing which file that is. A [setup](11-setup.md) names a default
sound table; the sound-table [frame hook](12-animation.md) and the [region](15-region.md)'s ambient
descriptors both index one.

**Provenance:** from the DAT format, which public readers document.

**Readers:** `dereth_assets::audio` (`Wave`, `SoundTable`). Pinned by
`sound_table_children_are_read_depth_first_in_file_order` in
`core/assets/src/audio.rs`.

## 1. Wave

| offset | size | field |
|---:|---:|---|
| `0x00` | 4 | id echo |
| `0x04` | 4 | header size — bytes of `WAVEFORMATEX` that follow |
| `0x08` | 4 | data size — bytes of samples that follow |
| `0x0C` | header size | the format header |
| … | data size | the samples |

Both blocks are copied verbatim. **There is no RIFF chunk structure**: to play one, a rebuild has to
synthesise `"RIFF" <size> "WAVE" "fmt " <header size> <header> "data" <data size> <data>` around it,
which is exactly what the client does before handing the buffer to the audio API.

The header is the standard structure:

| offset | size | field |
|---:|---:|---|
| `+0` | 2 | format tag |
| `+2` | 2 | channels |
| `+4` | 4 | samples per second |
| `+8` | 4 | average bytes per second |
| `+12` | 2 | block alignment |
| `+14` | 2 | bits per sample |
| `+16` | 2 | extra byte count |
| `+18` | n | codec-specific extra data |

The shipped corpus is almost entirely mono 16-bit PCM at 11 kHz, with a long tail of other rates
including a few that are not round numbers at all.

**One shipped wave is an MP3**, with a 30-byte header instead of 18. A decoder that assumes PCM
because 785 of 786 files are PCM leaves exactly one sound silent — and it will be a while before
anyone notices which.

## 2. Sound table

The record is a single root node, recursively defined, followed by a pad to 4:

| size | field |
|---:|---|
| 4 | key — a sound type; the root's key is 0 |
| 4 | `num_entries` |
| 16 × n | sound entries |
| 4 | child count |
| var | children, each read recursively |

A sound entry is 16 bytes:

| offset | size | field | value when the record is truncated |
|---:|---:|---|---|
| `+0` | 4 | wave id | 0 |
| `+4` | 4 | priority | 0.0 |
| `+8` | 4 | probability | 1.0 |
| `+12` | 4 | volume | 1.0 |

The reader **pre-fills each entry with those defaults** and only overwrites them when a full sixteen
bytes remain, so a truncated file yields silent but structurally valid entries rather than garbage.
Reproducing that is cheap and makes a reader behave the same way on damaged data.

### 2.1 The tree is two levels deep, and the lookup is one

The format is arbitrarily recursive. Shipped data never is:

```text
root (key 0, one entry, all zeros)
 +-- one child per sound type, each with one entry and no children
```

More importantly, **the lookup searches only the root's immediate children**. It does not recurse. So
the nesting is a generalisation the client never exercises, and implementing a deep search changes
nothing on shipped data while diverging on anything else.

When a sound type has several entries, the client picks one by index `trunc(u × (n − 1))` for a
uniform draw `u` in `[0, 1)`, so **the last entry of a multi-entry list can never be picked**; the
chosen entry then plays only if a roll passes its probability. Shipped tables barely exercise this:
every entry is priority 0, probability 1, volume 1.

### 2.2 Eager loading

Every non-zero wave id in a sound table is created as a sound **as the table is read**. Loading a
sound table therefore pre-creates every wave it references — the same eager behaviour the sound frame
hooks have. A rebuild that defers should make sure the first playback does not stall.

## 3. Sound types

The sound-type space runs to about 205 values, of which some 124 appear in shipped tables. The
families:

- speech, attack, special attack, damage, wound, death, grunt and exertion sets, numbered 1 to 3;
- weapon-specific sounds — bow pull and release, crossbow pull and release, sling, dagger, spear,
  thrown release, arrow whiz and land;
- impact sounds by material: flesh, leather, chain, plate, and three missile variants;
- footsteps, walking, dancing, hiding, eating, drinking;
- open, close, and their slam variants;
- eight ambient slots plus a waterfall, which is the range the region's ambient descriptors draw from;
- login and logout, lifestone, and the attribute/skill/health/shield/enchantment/vision/translucency
  up-and-down pairs;
- fizzle, launch, explode, create, destroy, lockpicking;
- four breath weapons: flame, acid, frost and lightning;
- a large UI family — portals, queries, errors, button presses, sliders, icon pick-up and drop, and a
  set of atmospheric stingers;
- fifty numbered trigger slots and a few late additions.

## 4. Where reimplementations differ

| topic | the client | common reimplementations |
|---|---|---|
| The RIFF container | Synthesised at play time. | The same, in the readers that can export a playable file. |
| The MP3 wave | Handled by the platform decoder. | Depends on the decoder chosen. One file is at stake. |
| Entry defaults on truncation | Pre-filled. | Not modelled. |
| Lookup depth | The root's direct children only. | The same, usually via a map keyed by sound type. |
| Eager wave creation | Yes. | Not modelled — a reader has nothing to create. |

## 5. A synthetic example

One wave and the table that names it:

```text
wave
  id echo        0x0A000001
  header size    18
  data size      558
  format tag     1                # PCM
  channels       1
  samples/sec    11025
  bytes/sec      22050
  block align    2
  bits/sample    16
  extra size     0
  samples        279 signed 16-bit samples, about 25 ms
                 # 4 + 4 + 4 + 18 + 558 = 588 bytes

sound table
  id echo        0x20000001
  root key       0
  root entries   1
    wave id      0                # no sound
    priority     0.0
    probability  1.0
    volume       1.0
  root children  1
    child key    66               # "open"
    entries      1
      wave id    0x0A000001
      priority   0.0
      probability 1.0
      volume     1.0
    children     0
                 # 60 bytes
```

Nothing above is taken from a shipped file.
