# The skill, vital and experience tables

Three small single-file records define character advancement: the **skill table**, the **vital formula
table** and the **experience table**. All three are flat byte streams with no padding except inside
strings, which pad to 4.

The server owns the authoritative advancement arithmetic. The client uses these tables for display and
for the character-creation cost preview, so where the two disagree the server wins — but a client that
computes a different skill base from the same attributes shows the player wrong numbers, which is its
own kind of bug.

**Provenance:** from the DAT format, which public readers document.

**Readers:** `dereth_assets::tables`. The whole-record check is `dereth-assets`' `tables_decode`
(`core/assets/tests/dat/decode/tables_decode.rs`), which needs the shipped data files and so is
not a public-tier test.

## 1. The skill table

| field | type |
|---|---|
| id echo | `u32` |
| hash header | `u32` — the packable form: count in the low half, buckets in the high |
| skills | count × { `u32 skill_id` ; skill record } |

Entries are in hash order, not sorted.

A skill record:

| field | type | meaning |
|---|---|---|
| `description` | string | Tooltip text. **Not** obfuscated — unlike the [spell table](30-spell-tables.md). |
| `name` | string | Display name. Also plain. |
| `icon_id` | `u32` | |
| `trained_cost` | `i32` | Skill credits to train. |
| `specialised_cost` | `i32` | **The total**, including the trained cost — not a delta. Upgrading from trained to specialised costs the difference. |
| `category` | `u32` | 1 weapon, 2 non-weapon, 3 magic. |
| `chargen_use` | `i32` | 1 for every shipped skill. |
| `min_advancement` | `u32` | The lowest advancement class at which the skill is usable: 1 means usable untrained, 2 means it must be trained. |
| `formula` | 24 | Section 1.1. |
| `upper_bound` | `f64` | Experience-cost interpolation bounds. |
| `lower_bound` | `f64` | |
| `learn_mod` | `f64` | 1.0 for every shipped skill. |

**A value of 999 or more in the specialised cost is a sentinel** meaning "cannot be specialised at
character creation", not a very expensive skill. Several shipped skills use one.

The shipped table has **holes**: a dozen ids in the range are absent, because they are the retired
pre-expansion weapon skills that were folded into the modern ones. The client simply has no record for
them, and a character still carrying one renders nothing for it. Some reimplementations re-add the
retired skills at load time for the benefit of legacy characters; **the client does not**, and a
rebuild that does will show skills the original never showed.

### 1.1 The attribute formula

Twenty-four bytes, six dwords:

| offset | field | meaning |
|---:|---|---|
| `+0` | `w` | Additive constant. |
| `+4` | `x` | Multiplier for the first attribute. |
| `+8` | `y` | Multiplier for the second. |
| `+12` | `z` | Divisor. **Zero means the formula yields nothing at all.** |
| `+16` | `attr1` | 1 strength, 2 endurance, 3 quickness, 4 coordination, 5 focus, 6 self; 0 for none. |
| `+20` | `attr2` | The same, 0 when unused. |

```text
if z == 0: no value
n   = x * attr(attr1) + y * attr(attr2) + w      # 32-bit, wrapping
out = floor((float)n / (float)z + 0.5)           # the division is in 32-bit FLOAT
```

**The intermediate is a 32-bit float, not a double.** For ordinary attribute values it makes no
difference; for very large ones it does, and it is a one-character change to get wrong. The
integer-to-float conversions treat `n` as **unsigned**, so a negative sum wraps to a large value rather
than going negative; rounding is round-half-up (`floor(x + 0.5)`), not banker's rounding.

A term is absent when its multiplier or its attribute is zero, which is how the interface renders
"(Focus + Self) / 4" for a two-attribute skill and a single attribute for a one-attribute skill.
Several shipped skills have both multipliers zero and therefore no attribute base at all.

The attribute numbering is confirmed by the data itself: the shipped formulas reproduce the
long-documented relationships — melee defence from quickness and coordination, the magic schools from
focus and self, two-handed combat from strength and coordination.

## 2. The vital formula table

The smallest record in the container: an id echo and **three formulas in a fixed order**, using the
same 24-byte structure as a skill's. Seventy-six bytes total.

The shipped formulas are the ones every player knows: maximum health is endurance halved, maximum
stamina is endurance, maximum mana is self.

There is no count and no key — the order *is* the identity, which makes this the one table where a
field-order mistake is silent and total.

## 3. The experience table

Five maxima, then five cumulative cost curves, then the credit array:

| field | type | meaning |
|---|---|---|
| id echo | `u32` | |
| `max_attribute_level` | `u32` | 190 in shipped data. |
| `max_vital_level` | `u32` | 196. |
| `max_trained_skill_level` | `u32` | 208. |
| `max_specialised_skill_level` | `u32` | 226. |
| `max_character_level` | `u32` | 275. |
| attribute costs | `u32` × (max + 1) | |
| vital costs | `u32` × (max + 1) | |
| trained-skill costs | `u32` × (max + 1) | |
| specialised-skill costs | `u32` × (max + 1) | |
| character-level costs | **`u64`** × (max + 1) | |
| skill credits per level | `u32` × (max + 1) | |

Three things to get right:

- **Each array has one more entry than its maximum**, because index 0 is "zero raises". Index *n* is
  the **total** experience to have raised the stat *n* times, not the increment.
- **The character-level array is 64-bit** while every other array is 32-bit. The totals at the top of
  the curve do not fit in 32 bits, and a reader that assumes a uniform width desynchronises at the
  fifth array and then reads the credit array from the wrong place.
- **The level-1 entry is zero**: level 1 costs nothing.

The credit array is 1 at exactly 46 levels and 0 everywhere else — the 46 skill credits a character
earns by levelling. The levels are dense early and sparse late, ending at the level cap.

The reader guards each array against an all-bits-set count and skips a destination slot that is
already non-zero, which is an artefact of a shared reload path; a fresh load fills every slot.

## 4. Where reimplementations differ

| topic | the client | common reimplementations |
|---|---|---|
| Retired skills | Absent; nothing renders for them. | Frequently re-added at load for legacy characters. |
| The specialised cost | A total. | Usually exposed as a total plus an explicit upgrade cost. Same numbers. |
| The formula division | 32-bit float. | Often double. Identical over the real attribute range. |
| The level array width | 64-bit. | The same, in readers that get past it. |

## 5. A synthetic example

The vital table in full — it is small enough to show entirely, and it is the clearest illustration of
a fixed-order record:

```text
id echo         0x0E000003
health formula  w=0 x=1 y=0 z=2 attr1=2 attr2=0      # endurance / 2
stamina formula w=0 x=1 y=0 z=1 attr1=2 attr2=0      # endurance
mana formula    w=0 x=1 y=0 z=1 attr1=6 attr2=0      # self
                # 4 + 3*24 = 76 bytes
```

and one skill record's shape:

```text
skill_id            32
description         string, plain, padded to 4
name                string, plain, padded to 4
icon_id             0x06000001
trained_cost        8
specialised_cost    16                 # a total: the upgrade costs 8
category            3                  # magic
chargen_use         1
min_advancement     2                  # must be trained
formula             w=0 x=1 y=1 z=4 attr1=5 attr2=6  # (focus + self) / 4
upper/lower bound   900.0 / 120.0
learn_mod           1.0
```

Nothing above is taken from a shipped file; the field values are the shape a record takes, written out
longhand.
