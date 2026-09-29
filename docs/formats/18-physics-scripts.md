# Physics scripts (`0x33xxxxxx`) and script tables (`0x34xxxxxx`)

A **physics script** is a timeline of [animation hooks](12-animation.md): a list of
`(start_time, hook)` pairs sorted by time. It is the client's visual-effect script — spell effects,
portal flashes, blood splatter, level-up bursts — and it is driven by **wall-clock seconds** rather
than by animation frames.

A **script table** maps an effect type — the semantic name of an effect, which is what the server sends
on the wire — to a short list of `(threshold, script id)` pairs, so one logical effect can pick a
different script depending on a magnitude. Every [setup](11-setup.md) can name a default script table.

**Provenance:** from the DAT format, which public readers document.

**Readers:** `dereth_assets::motion` (`PhysicsScript`, `PhysicsScriptTable`) over `dereth_assets::hook`.
Pinned for the hook side by the three tests in `core/assets/src/hook.rs`.

## 1. Script layout

| offset | size | field |
|---:|---:|---|
| `0x00` | 4 | id echo |
| `0x04` | 4 | `num_entries` |
| `0x08` | var | `num_entries` × { `f64 start_time` ; hook } |
| … | 0–3 | pad to 4 |

Each entry is an **8-byte double** — seconds from the start of the script — immediately followed by a
complete hook: the 8-byte header, the per-type payload, and the hook's own pad to 4.

`start_time` is a double in a stream that is 4-aligned and never 8-aligned, because the entries begin
twelve bytes into the record. **Read it unaligned**; there is no padding before it.

After reading, the client does two things that are not in the file:

1. **It sorts the entries by start time.** Do the same rather than assuming the file is sorted — it
   happens to be. The comparator never returns zero, so the order of two hooks at the same time is
   whatever the sort does with them and is not part of the format.
2. **It sets the script's length from the last entry's start time.** That is the duration.

Almost every hook in a script is create-particle, call-another-script or sound-table. The chaining
hook takes a script id and a pause, and that is how compound effects are built. **The chain can be
arbitrarily deep and the client does not guard against a cycle** — a rebuild should.

A hook whose type the reader does not recognise makes the rest of the script unreadable, exactly as in
an animation. No shipped script contains an unknown type.

## 2. Script table layout

| offset | size | field |
|---:|---:|---|
| `0x00` | 4 | id echo |
| `0x04` | 4 | `num_entries` |
| `0x08` | var | `num_entries` × { `u32 effect_type` ; entry } |

where an entry is

```text
u32 count
count × { f32 threshold ; u32 script_id }
```

**Duplicate keys are silently dropped**: the reader checks whether the key is already present and
discards the later one. No shipped table has duplicates, which matters mainly because a reader built
on a map type that rejects duplicates would fail on a file the client would load.

Selection:

```text
entry = look up the effect type
if absent: no script
for (threshold, script_id) in the entry, in file order:
    if query <= threshold: return script_id
no script
```

So the thresholds are **upper bounds tested in file order** — a step function over an externally
supplied magnitude in `[0, 1]`. It is not a nearest-match and it does not interpolate. A query of
exactly 0.0 against a table whose thresholds are 0.0, 0.5, 1.0 selects the **first** script, not the
second.

In shipped data almost all tables use one of three shapes: a single always-matching entry, three tiers
at 0.0 / 0.5 / 1.0, or five tiers at quarters.

## 3. Effect types

The effect-type space runs to about 175 values, and roughly 150 of them appear in shipped tables. They
fall into families that are worth knowing because they explain the size of the enumeration:

- launch and explode, for projectiles;
- attribute, skill, health, regeneration, shield, enchantment and vitae changes, each in an "up" and a
  "down" form, and each in one of eight or nine colours;
- health-swap effects between the three vital colours;
- portal entry and exit, fizzle, create, destroy, projectile collision;
- four breath weapons: flame, frost, acid and lightning;
- **twelve splatter and twelve spark effects**, one per body quadrant: low/mid/up × left/right ×
  back/front;
- hide, unhide, hidden, disappear;
- ten numbered and eight coloured "special state" slots;
- late-expansion effects — level up, wedding effects, dispels, augmentations, the per-school surge
  effects and the dirty-fighting debuffs.

The server sends the effect **type**, not a script id. The client's table lookup is what resolves it to
a script, which is why a server reimplementation needs the type space and a client reimplementation
needs the lookup.

## 4. Where reimplementations differ

| topic | the client | common reimplementations |
|---|---|---|
| Sorting and the script length | Both computed at load. | Neither; both are trivially derivable. |
| Duplicate table keys | Silently dropped. | A map type that throws on a duplicate key would fail on a file the client accepts. |
| The threshold lookup | Implemented in the client. | Usually absent: a server picks the effect type and sends it, and only the client resolves it. |
| The trailing alignment | Applied. | Applied. |

## 5. A synthetic example

A one-line trampoline script that immediately runs another, and a projectile's two-entry table:

```text
script
  id echo          0x33000001
  num_entries      1
  start_time       0.0            # f64, at offset 0x08, unaligned
  hook type        19             # call another script
  hook direction   0
  script id        0x33000002
  pause            0.0
                                  # 32 bytes, length = 0.0

table
  id echo          0x34000001
  num_entries      2
  effect_type      4              # launch
  count            1
  threshold        1.0
  script_id        0x33000001
  effect_type      5              # explode
  count            1
  threshold        1.0
  script_id        0x33000002
```

Nothing above is taken from a shipped file.
