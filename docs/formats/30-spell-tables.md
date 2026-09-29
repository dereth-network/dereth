# The spell table and the spell-component table

Two single-file records hold the whole magic system's static data. One holds every spell and every
equipment spell-set; the other holds every spell component. Both are read as a flat byte stream with
no padding except around strings.

Two things make these files unusual:

- **Spell names and descriptions are stored with their nibbles swapped**, as are component names and
  spoken syllables.
- **The eight-slot component formula is additively encrypted with a key derived from those two
  strings** — so a reader that cannot de-obfuscate the strings cannot read the formulas either.

Both mechanisms are given in full below.

**Provenance:** from the DAT format, which public readers document.

**Readers:** `dereth_assets::tables`. Pinned by `the_de_obfuscation_is_a_nibble_swap` and
`the_spell_hash_treats_bytes_as_signed` in `core/assets/src/tables.rs`.

## 1. The spell table

| field | type |
|---|---|
| id echo | `u32` |
| spell hash header | `u32` — count in the low **16** bits, bucket count in the high 16 |
| spells | count × { `u32 spell_id` ; spell record } |
| spell-set hash header | `u32` — count in the low **24** bits, a bucket-size *index* in the high 8 |
| spell sets | count × { `u32 set_id` ; spell set } |

**The two headers are not the same shape.** The first is the packable form; the second is the
intrusive form with an index into the bucket-size table, as described in
[03-serialisation-primitives.md](03-serialisation-primitives.md). Reading the second as the first
happens to work on shipped data only because the bucket index is small. It is the kind of coincidence
that fails on the first modified file.

### 1.1 A spell record

In order, all unaligned little-endian:

| field | type | meaning |
|---|---|---|
| `name` | string | **Nibble-swapped**; section 2. |
| `description` | string | **Nibble-swapped**. |
| `school` | `u32` | 0 none, 1 war, 2 life, 3 item enchantment, 4 creature enchantment, 5 void. |
| `icon_id` | `u32` | An [image](14-textures.md). |
| `category` | `u32` | Spells in one category do not stack. |
| `flags` | `u32` | Section 1.2. |
| `base_mana` | `i32` | |
| `range_constant` | `f32` | Range is `constant + mod * skill`. |
| `range_mod` | `f32` | |
| `power` | `i32` | Strength ordering within a category. |
| `economy_mod` | `f32` | See the warning below. |
| `formula_version` | `u32` | 1, 2 or 3 — selects the per-account taper randomiser of section 4. |
| `component_loss` | `f32` | Probability a component burns on a failed cast. |
| meta-spell | var | `u32` type, then the type's payload; section 3. |
| `formula` | 32 | Eight dwords, **encrypted**; section 2. |
| `caster_effect` | `u32` | A [physics-script effect type](18-physics-scripts.md) played on the caster. |
| `target_effect` | `u32` | And on the target. |
| `fizzle_effect` | `u32` | Zero for every shipped spell. |
| `recovery_interval` | `f64` | Zero for every shipped spell. |
| `recovery_amount` | `f32` | Zero for every shipped spell. |
| `display_order` | `i32` | The spell book's sort key. |
| `non_component_target_type` | `u32` | The target mask used when components are not required. |
| `mana_mod` | `i32` | Extra mana per additional target. |

**A warning about `economy_mod`.** The name invites reading it as a mana-cost multiplier. **The client
never reads the field at all**, and it is not 1.0 everywhere: several hundred shipped spells carry
0.0, including some well-known ones. A server that treated it as a multiplier would make those spells
**free to cast**, silently. If a rebuild uses it for anything, that is a decision it is making on its
own, not one the data supports.

### 1.2 The flag bits

| bit | meaning |
|---|---|
| `0x1` | Resistable. |
| `0x2` | Sensitive to player-killer status. |
| `0x4` | Beneficial. |
| `0x8` | Self-targeted. |
| `0x10` | Reversed. |
| `0x20` | Not castable indoors. |
| `0x40` | Not castable outdoors. |
| `0x80` | Not researchable. |
| `0x100` | Projectile. |
| `0x200` | A creature spell. |
| `0x400` | Excluded from item descriptions. |
| `0x800` | Ignores mana conversion. |
| `0x1000` | A non-tracking projectile. |
| `0x2000` | A fellowship spell. |
| `0x4000` | Fast cast. |
| `0x8000` | Long range indoors. |
| `0x10000` | Damage over time. |

The set stops at `0x10000`. Two of the bits are directly confirmable against shipped data: the
beneficial bit is set on the healing and buff lines and clear on the harm and debuff lines, and the
self-targeted bit is set on every "self" spell and clear on every "other" spell.

### 1.3 Derived values worth reimplementing

- **School to skill**: each of the five schools maps to one skill id, and anything else maps to none.
- **The spell's level** as the interface shows it, 1 to 8, comes from the power level of the scarab in
  slot 0: levels 1 to 6 map through unchanged, 7 and 8 lose one, and 9 and 10 lose two.
- **The targeting type** comes from the (decrypted) formula's *target* slot, which is found by walking
  up from slot 5 while the slot is non-zero, stopping at slot 8, and taking the slot before the one the
  walk stopped at: slot 4 when slot 5 is empty, slot 7 when all eight are filled. It requires a
  "complete" formula, meaning the first **five** slots are non-zero; an incomplete one has targeting
  type 0. The component in the target slot maps to the targeting type: 0x31–0x38, 0x3C–0x3E and 0xBE
  give `0x10` (creatures), 0x39 gives `0x88B8F` (enchantable items), 0x3B gives `0x10010000` (portals
  and lifestones), and every other component gives 0, which makes the spell untargeted. The record's
  `non_component_target_type` field plays no part.
- **The power level of a component** is a small fixed table: components 1 to 6 are levels 1 to 6, and
  four further ids are levels 7 to 10. Everything else is level 0.

## 2. The obfuscation

### 2.1 The nibble swap

Names, descriptions, component names and component syllables are stored with **each byte's nibbles
exchanged**:

```text
for each visible character: c = (c >> 4) | (c << 4)
```

The string reader itself does not transform anything; every consumer swaps on a private copy. The
length includes a terminating NUL, and exactly the visible characters are swapped. The same trick is
used for quest names.

**Apply it exactly where it belongs and nowhere else.** Applying it to any other string in these files
produces garbage, and not applying it to these produces garbage — there is no ambiguity and no middle
ground.

### 2.2 The string hash

A PJW/ELF-style hash over **signed** bytes, stopping at the first NUL:

```text
h = 0
for each byte c of the de-obfuscated string:
    if c >= 0x80: c -= 0x100          # signed char
    h = (h * 16 + c) & 0xFFFFFFFF
    if h & 0xF0000000:
        h = (h ^ ((h & 0xF0000000) >> 24)) & 0x0FFFFFFF
return h == 0xFFFFFFFF ? 0xFFFFFFFE : h
```

The detail that decides whether the hash matches is that **the bytes are signed**, so anything above
0x7F contributes a negative value. The client also replaces a result of `0xFFFFFFFF` by `0xFFFFFFFE`,
because the first value is the "not yet computed" sentinel it caches in the string buffer; the masking
keeps every result below `0x10000000`, so that fix-up can never fire and a reader may omit it.

### 2.3 The formula key

```text
key = (hash(name) % 0x12107680) + (hash(description) % 0xBEADCF45)     # 32-bit wrapping add
for i in 0..8:
    if formula[i] != 0:
        formula[i] = (formula[i] - key) & 0xFFFFFFFF
```

**Zero slots are left as zero and are not compacted.** That matters: the version 2 and 3 randomisers
of section 4 index fixed slots of an eight-element array, and they are meaningless against a compacted
list. In shipped data every zero slot is trailing, so a compacted list happens to agree — until it
does not.

## 3. Meta-spells

The meta-spell is a `u32` type followed by the type's payload. **Every type first reads the spell's own
id again.** Most types read nothing beyond it.

| type | extra payload |
|---:|---|
| 1 enchantment | `f64` duration, `f32` degrade modifier, `f32` degrade limit |
| 2 projectile | none |
| 3 boost | none |
| 4 transfer | none |
| 5 portal link | none |
| 6 portal recall | none |
| 7 portal summon | `f64` portal lifetime |
| 8 portal sending | none |
| 9 dispel | none |
| 10 life projectile | none |
| 11 fellowship boost | none |
| 12 fellowship enchantment | duration, degrade modifier, degrade limit |
| 13 fellowship portal sending | none |
| 14 fellowship dispel | none |
| 15 enchantment projectile | none |

Type 0 fails the read, and so does any type above 15: no other type has a payload the client can
read, so the rest of the record would be read at the wrong offsets. Where a field is absent the constructor's defaults stand: a duration of −1.0, a
degrade modifier of 0.0 and a degrade limit of −666.0. That last value is a sentinel, not data.

The gameplay data an enchantment needs at run time — which vital it modifies, by how much — **is not in
this file**. The client has only the spell id and rebuilds the enchantment from server messages, which
is why a server reimplementation needs its own spell data and a client one does not.

## 4. Per-account formula randomisation

The formula in the file is the canonical one. The formula a **particular account** must use replaces
the tapers in slots 3 and 6 with values derived from the account name (version 1 also rewrites slot 1,
but from the formula alone, so that taper is the same for every account). There are three algorithms,
selected by the spell's formula version; any other version leaves the formula alone.

All three work in the taper id space: the lowest taper id is 63 and there are twelve tapers, hence
every result is `63 + something % 12`. The seed is the same string hash as above, applied to the
account name.

**Version 1** counts the non-zero components and picks slot roles by that count, then:

```text
seed = key % 0x13D573
if n > 5: formula[1] = 63 + (powder + scarab + talisman + 2*herb + potion) % 12
if n > 6: formula[3] = 63 + ((seed / (powder + potion + scarab))
                             * (talisman + 2*(powder+potion) + herb + scarab)) % 12
if n > 7: formula[6] = 63 + ((seed / (talisman + scarab))
                             * (powder + scarab + herb + potion + 2*talisman)) % 12
```

with **three guards** that force the scarab term to 1 when `herb + scarab`, `powder + potion + scarab`
or `talisman + scarab` respectively would be zero — the last two are the divisors — and the forced
value **persists** into the later arms. The divisions are unsigned 32-bit. The guards are frequently
absent from reimplementations; the arithmetic is otherwise identical.

**Version 2** has no slot count and no divide-by-zero guard at all; it always writes slots 3 and 6,
and every version-2 spell has all eight slots filled:

```text
seed = key % 0x13D573
formula[3] = 63 + (f[7] + 3*f[0] + 2*f[5]*f[4] + f[2] + f[1]) % 12
formula[6] = 63 + ((seed / (f[7]*f[1] + 2*f[4]))
                   * (f[7] + 3*f[2]*f[0] + 2*f[5] + f[4])) % 12
```

**Version 3** derives six intermediate values, each a component plus a different modulus of the key,
all taken modulo 12, and combines them polynomially into slots 3 and 6. Slot 7 contributes zero for
spells with fewer than eight components.

The client caches the string hash inside the string buffer, which is why version 3 appears to compute
seven different hashes that are in fact one: six moduli of it seed the intermediates, and a seventh
(`key % 0x65039 % 12`) is added into the slot-6 sum.

## 5. Spell sets

An equipment set grants progressively more spells as more pieces are worn.

```text
u32 tier_count      # a plain list: one whole dword, no bucket half
tier_count × {
    u32 pieces_required
    u32 spell_count
    u32 × spell_count   spell ids
}
```

The shipped table holds 139 sets, and no set repeats a `pieces_required` value.

Shipped sets run up to fifty tiers; a typical tier replaces the previous tier's spells with
higher-level versions rather than adding to them.

## 6. The component table

| field | type |
|---|---|
| id echo | `u32` |
| hash header | `u32` — the packable form: count in the low half, buckets in the high |
| components | count × { `u32 component_id` ; component record } |

A component record:

| field | type | meaning |
|---|---|---|
| `name` | string | **Nibble-swapped.** |
| `category` | `u32` | 0 scarab, 1 herb, 2 powdered gem, 3 alchemical substance, 4 talisman, 5 taper, 6 pea, 8 undefined. |
| `icon_id` | `u32` | |
| `type` | `u32` | 0 undefined, 1 power, 2 action, 3 concept prefix, 4 concept suffix, 5 target, 6 accent, 7 pea. |
| `gesture` | `u32` | A motion command played while the component is used; **all bits of the top nibble set means "none"**. |
| `time` | `f32` | How long the gesture takes; zero when there is none. |
| `text` | string | **Nibble-swapped.** The spoken syllable. Empty for scarabs, talismans and tapers. |
| `destruction_mod` | `f32` | Scales the spell's component-loss probability for this component: 1.0 for consumables, 0.1 for scarabs and talismans. |

The incantation the interface shows is the concatenation of the syllables in formula order — which is
why only some component categories have one.

The **target** component decides what a spell may be cast at. A small fixed table maps a handful of
component ids to target masks and everything else to zero.

Three id-mapper files translate between spell-component ids and the class ids of the physical items,
in both directions, plus one from school of magic to a class id.

## 7. Where reimplementations differ

| topic | the client | common reimplementations |
|---|---|---|
| The two hash headers | Two different shapes. | Often one shape for both. Works on shipped data by coincidence. |
| Zero formula slots | Kept in place. | Frequently dropped, producing a compacted list. Agrees on shipped data; breaks the fixed-slot randomisers. |
| A component id above 198 | No clamp. | Sometimes masked to a byte. |
| The string hash | 32-bit, with the sentinel fix-up. | Sometimes accumulated in 64 bits with no fix-up. Identical results: the masking keeps the value in 28 bits and the fix-up never fires. |
| Version 1's divide-by-zero guards | Present. | Frequently absent. |
| `economy_mod` | Never read. | Not read either — but the name is a trap for anyone building a server from the field list alone. |

## 8. A synthetic example

The obfuscation end to end, on a two-character name:

```text
stored name bytes     0x41 0x42          # as they sit in the file
after the swap        0x14 0x24          # (c >> 4) | (c << 4)
```

and a spell record's shape, with a formula that decrypts to five components and three empty slots:

```text
spell_id              1
name                  string, nibble-swapped, padded to 4
description           string, nibble-swapped, padded to 4
school                4
icon_id               0x06000001
category              1
flags                 0x00000006         # player-killer sensitive + beneficial
base_mana             10
range constant/mod    5.0 / 1.0
power                 1
economy_mod           1.0
formula_version       1
component_loss        0.0
meta-spell            type 1, spell id 1, duration 1800.0,
                      degrade modifier 0.0, degrade limit -666.0
formula (encrypted)   eight dwords, five non-zero
  key                 hash(name) % 0x12107680 + hash(description) % 0xBEADCF45
  decrypted           [1, 7, 26, 41, 51, 0, 0, 0]
caster/target/fizzle  0 / 6 / 0
recovery              0.0 / 0.0
display_order         10412
non_component_target  16
mana_mod              0
```

Nothing above is taken from a shipped file; the decrypted formula is five plausible component ids
chosen to show the shape.
