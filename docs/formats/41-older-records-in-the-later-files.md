# Older records in the later files

The later data files (`client_portal.dat` and its siblings, from June 2005) kept their container
from Throne of Destiny to the end, but three record types changed layout inside it. A file from
before a change holds the older layout, and nothing in the container says so. This page lists each
older layout and how a reader tells it apart. Everything else in the later files reads with the
layouts the other pages describe, in every file held from June 2005 to December 2017.

**Readers:** `Decode::decode` on `SpellTable`, `CharGen` and `Region` in `dereth-assets`. Pinned by
the public-tier `legacy_tables` tests in `core/assets/tests/cpu/` and, in the `dat` tier, by
`capture_census` (every record of every held file) and `legacy_layouts` (the contents of each
layout), both reading `DERETH_TEST_DAT_CAPTURES_DIR`.

## Choosing a layout

None of the three records carries a version that changed with its layout. A reader tries the
layouts in turn, the latest first, and takes the one whose reading ends exactly on the record's end.
For every record held exactly one layout does: the layouts differ by a word in a structure that
repeats hundreds of times, so a wrong one either fails outright or misses the end by far.

## SpellTable (`0x0E00000E`)

The tables of June 2005 and January 2006 end after the spells: there is **no spell-set table**, not
even an empty header. The set table is read only when bytes follow the spells. The spells
themselves are in the layout of [30](30-spell-tables.md). Every later table held has the set table.

## CharGen (`0x0E000002`)

Four layouts, each differing from the next only where listed:

| layout | held in | differs from the next |
|---|---|---|
| launch | June 2005, January 2006 | seven help-text string ids after the two ids; a description string id after a heritage's setup; a naming-help string id after a sex's icon; a template's fourth word is a description string id, not a title |
| 2009 | April and August 2009 | a sex has no scale and no physics, motion or combat table (its name is followed by its setup) |
| 2010 | June and September 2010 | a hair style has no alternate setup (its icon and bald byte are followed by its appearance) |
| current | 2012 on | — |

A field a layout does not have reads as zero; the launch layout's description and help ids are read
and not kept. The heritages, sexes and hair styles held: four heritages, eight sexes and 56 hair
styles in the launch and 2009 tables; ten, twenty and 707 in 2010; thirteen, twenty-six and 869 at
the end.

## Region (`0x13000000`)

Until July 2012 a sky object has **eight words**, as before Throne of Destiny: no particle-script
id between its object id and its properties. From August 2012 it has nine. The August 2012 region
is also the first with rainy day groups, whose sky objects name the rain's particle script. The
land surface is texture merging throughout.

## A damaged file

One held cell file, January 2014, has four records broken by the file itself rather than by any
layout: three records of landblock `0x6757` whose block chains end early, and the cell
`0x576602B8`, whose block holds other bytes. The same records read whole in the files before and
after it.
