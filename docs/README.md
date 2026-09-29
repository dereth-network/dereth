# Dereth — the behaviour specification

This directory is the public specification of what the Asheron's Call client *does*: the numbers, the
ordering, the edge cases, the places where common reimplementations differ, and the test that pins
each claim where one exists.

Dereth is an independent reimplementation of the final retail Asheron's Call client (June 2015),
written from observed runtime behaviour and publicly documented file and wire formats; it is not
affiliated with Turbine, Microsoft, or any current rights holder.

## What this is, and what it is not

It is a **behaviour** specification. Every statement here is meant to be falsifiable: it says what
happens, under what conditions, and what a reader would see if their own implementation got it
wrong. Where a claim is pinned by a test in this repository, the test is named, so that "the spec
says X" and "the code does X" cannot drift apart silently.

It is **not** a map of the original binary. Nothing here names an internal function, an address, a
class or a source file of the original program, and no such artefact is reproduced anywhere in this
repository's public tree. Claims that could only be stated by pointing at one of those were left out
rather than reworded around them.

It is **not** a copy of anyone else's work. Where behaviour is compared with the ACE or GDLE server
emulators, the comparison is a description in prose; no code from those projects appears here. Where
the client's own data files are described, the description is of the *format*, never of the shipped
content.

## How to read a page

Every page opens with a **provenance line**, which is one of three things:

| Provenance | Means |
|---|---|
| **observed at runtime** | Someone ran the thing and watched it happen. The strongest kind of claim here, and the rarest. |
| **wire or file format** | A layout that public readers and server emulators already document, restated in the form this project needed and cross-checked against its own decoders. |
| **design opinion** | A judgement about how to rebuild, not a statement about the original. Only the glossary's few *(rebuild)* terms are this kind. |

A page may carry more than one, section by section. When a section's provenance differs from the
page's, it says so in place.

Claims are written as behaviour with a consequence: not "field *n* is a double" but "field *n* is
eight bytes, and reading it as four shifts every field after it". A claim with no consequence a
reader could check is not worth stating and is not stated.

Numbers are exact. Where a constant is load-bearing — a timeout, a cap, a threshold, a packet size —
it is given as a number, because "roughly two minutes" is not a specification.

## Naming a test

A named test looks like `core/client-net/tests/cpu/net/replay.rs`, or a function inside a module, for example
`core/client-net/src/sequence_gate.rs :: the_deadlock_breaker_fires_at_more_than_19_stamps_held_more_than_300_seconds`.

Tests under `tests/cpu/` and inside `src/` run with no assets and no device: they are the public
tier and anyone can run them. Tests under `tests/dat/` need the original client's data files and
`tests/gpu/` needs a graphics device, so those are named only where nothing in the public tier pins
the claim, and they are marked **(local)**.

## Index

### Corrections

| Page | Provenance |
|---|---|
| [CORRECTIONS.md](CORRECTIONS.md) — every place where the client contradicts ACE, GDLE or the community protocol references, with the test that pins the answer | mixed |

### Vocabulary

| Page | Provenance |
|---|---|
| [glossary.md](glossary.md) — the game's own words, defined once | mixed |

### Networking

| Page | Provenance |
|---|---|
| [networking/01-packet-format.md](networking/01-packet-format.md) — the datagram, the flag bits, the optional header sections, the checksum and the cipher | wire format |
| [networking/02-reliability-and-flow.md](networking/02-reliability-and-flow.md) — sequencing, acknowledgement, retransmission, ordering, the interval clock, timeouts | wire format |
| [networking/03-connection-state-machine.md](networking/03-connection-state-machine.md) — the handshake, the second port, connection states, every way a connection dies | wire format |
| [networking/04-netblobs-and-queues.md](networking/04-netblobs-and-queues.md) — message blobs, blob ids, reassembly, the twelve queues, the send path | wire format |

### Data formats

The on-disk formats, one page per record kind; start with [formats/README.md](formats/README.md).

| Page | Provenance |
|---|---|
| [formats/01-dat-container.md](formats/01-dat-container.md) — The container: header, blocks, the B-tree directory, the write journal, the iteration file, and how four files answer one lookup | file format |
| [formats/02-file-ids-and-types.md](formats/02-file-ids-and-types.md) — The 32-bit file id, the sixty-five types and their id ranges, and why the top byte is not the type | file format |
| [formats/03-serialisation-primitives.md](formats/03-serialisation-primitives.md) — The object header, the alignment rule, the compressed integer, the two string forms, the three hash-table headers and the pack versions | file format |
| [formats/10-gfxobj.md](formats/10-gfxobj.md) — Geometry: vertex arrays, polygons, and the drawing and physics BSP trees | file format |
| [formats/11-setup.md](formats/11-setup.md) — Jointed models: parts, attachment points, placement poses, collision volumes and lights | file format |
| [formats/12-animation.md](formats/12-animation.md) — Key frames and the twenty-seven kinds of frame hook | file format |
| [formats/13-palette-and-surfaces.md](formats/13-palette-and-surfaces.md) — Palettes, palette sets, surfaces, surface textures, and the packed appearance record | file format |
| [formats/14-textures.md](formats/14-textures.md) — Images: pixel formats, the palette-indexed colour key, the raw-JPEG variant, mip handling | file format |
| [formats/15-region.md](formats/15-region.md) — The one region record: the world grid, the calendar, the sky, terrain texturing and ambient sound | file format |
| [formats/16-environment.md](formats/16-environment.md) — Interior cell shapes and how a cell record refers to one | file format |
| [formats/17-scene-and-particles.md](formats/17-scene-and-particles.md) — Landscape scatter — including the deterministic placement hash — and particle emitters | file format |
| [formats/18-physics-scripts.md](formats/18-physics-scripts.md) — Visual-effect timelines and the tables that select them | file format |
| [formats/19-motion-table.md](formats/19-motion-table.md) — The motion state machine: cycles, modifiers and transitions | file format |
| [formats/20-clothing-table.md](formats/20-clothing-table.md) — Wearable appearance across body types, and the dye system | file format |
| [formats/21-sound-tables.md](formats/21-sound-tables.md) — Raw waves and the sound-type tables that name them | file format |
| [formats/22-degrade-info.md](formats/22-degrade-info.md) — Level-of-detail chains and the frame-rate feedback loop that drives them | file format |
| [formats/30-spell-tables.md](formats/30-spell-tables.md) — Spells, spell sets and components — including the obfuscation and the per-account formula randomisation | file format |
| [formats/31-skill-xp-tables.md](formats/31-skill-xp-tables.md) — Skills, the vital formulas and the experience curves | file format |

### Message catalogue

What the client sends and expects above a reassembled message; start with [networking/messages/README.md](networking/messages/README.md).

| Page | Provenance |
|---|---|
| [networking/messages/00-dispatch-and-queues.md](networking/messages/00-dispatch-and-queues.md) — Queues, framing, ordering, per-property sequencing, and the master opcode table | wire format |
| [networking/messages/01-login-and-character.md](networking/messages/01-login-and-character.md) — The character list, creation and deletion, entering the world, the player description, the options blob | wire format |
| [networking/messages/02-world-objects.md](networking/messages/02-world-objects.md) — Object lifecycle, the three descriptors, containment, appraisal | wire format |
| [networking/messages/03-qualities-and-updates.md](networking/messages/03-qualities-and-updates.md) — The forty-four quality updates, and enchantments | wire format |
| [networking/messages/04-game-events.md](networking/messages/04-game-events.md) — What a game event is, and when it arrives without its wrapper | wire format |
| [networking/messages/05-movement.md](networking/messages/05-movement.md) — Position packs, motion states, and the movement buffer | wire format |
| [networking/messages/06-combat-and-magic.md](networking/messages/06-combat-and-magic.md) — Attacks, hit notifications, casting | wire format |
| [networking/messages/07-inventory-and-items.md](networking/messages/07-inventory-and-items.md) — Inventory requests, the stack-size update, salvage | wire format |
| [networking/messages/08-communication.md](networking/messages/08-communication.md) — Chat, tells, emotes, channels, squelch, confirmations | wire format |
| [networking/messages/09-social.md](networking/messages/09-social.md) — Friends, titles, allegiance, fellowship, contracts | wire format |
| [networking/messages/10-trade-housing-vendor.md](networking/messages/10-trade-housing-vendor.md) — Trading, vendors, housing, board games, books | wire format |
| [networking/messages/11-game-actions.md](networking/messages/11-game-actions.md) — How the client frames everything it sends | wire format |
| [networking/messages/12-admin-and-misc.md](networking/messages/12-admin-and-misc.md) — Progression, personal queries, administration, data patching | wire format |

## The rule behind all of it

A claim is worth writing down when someone could act on it and be wrong without it. Everything else
is commentary.
