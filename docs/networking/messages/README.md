# The message catalogue

What the 2013 client sends and expects above a complete, reassembled message. The transport itself —
packets, fragments, reliability, the connection state machine — is covered by the networking pages
one level up.

Start with [00-dispatch-and-queues.md](00-dispatch-and-queues.md): it has the queue model, the two
ordered wrappers, the ordering rules, and the full 352-opcode table. Every other page assumes it.

| page | family |
|---|---|
| [00-dispatch-and-queues.md](00-dispatch-and-queues.md) | Queues, framing, ordering, per-property sequencing, and the master opcode table. |
| [01-login-and-character.md](01-login-and-character.md) | The character list, creation and deletion, entering the world, the player description, the options blob. |
| [02-world-objects.md](02-world-objects.md) | Object lifecycle, the three descriptors, containment, appraisal. |
| [03-qualities-and-updates.md](03-qualities-and-updates.md) | The forty-six quality updates and removes, and enchantments. |
| [04-game-events.md](04-game-events.md) | What a game event is, and when it arrives without its wrapper. |
| [05-movement.md](05-movement.md) | Position packs, motion states, and the movement buffer. |
| [06-combat-and-magic.md](06-combat-and-magic.md) | Attacks, hit notifications, casting. |
| [07-inventory-and-items.md](07-inventory-and-items.md) | Inventory requests, the stack-size update, salvage. |
| [08-communication.md](08-communication.md) | Chat, tells, emotes, channels, squelch, confirmations. |
| [09-social.md](09-social.md) | Friends, titles, allegiance, fellowship, contracts. |
| [10-trade-housing-vendor.md](10-trade-housing-vendor.md) | Trading, vendors, housing, board games, books. |
| [11-game-actions.md](11-game-actions.md) | How the client frames everything it sends. |
| [12-admin-and-misc.md](12-admin-and-misc.md) | Progression, personal queries, administration, data patching. |

## How to read these pages

Each page names the module of this client that implements the family and the tests that pin its
claims. All of the message tests are public-tier: they run without any shipped data file and without
a recorded capture.

**Where a page says the community catalogue is wrong, it means one specific published description**
that circulates in the emulator community, and the disagreement is always stated as a behaviour with
a consequence: what the client reads, and what goes wrong if a message is built the other way. Those
corrections are the most useful thing on these pages, and they are also the most likely to be
disputed — each one is pinned by a named test, so they can be checked rather than argued about.

No byte sequences from recorded traffic appear anywhere on these pages. Where a layout is shown it is
written out as fields, and where an example is needed it is constructed.

## The three recurring traps

1. **The alignment origin is the whole message, not the field.** A body encoded independently and
   then prefixed with a wrapper aligns on the wrong origin, and every variable-length field after the
   first goes wrong. See [04-game-events.md](04-game-events.md),
   [11-game-actions.md](11-game-actions.md) and the movement buffer in
   [05-movement.md](05-movement.md).
2. **A structure's field offsets are not its wire order.** Four structures in this catalogue are read
   in an order that a layout listing would not suggest: the physics descriptor, the quality
   structures, the allegiance member record and the trade object. Each has cost someone a working
   feature.
3. **Two messages are byte-packed in a dword-aligned protocol**: the stack-size update and the
   housing restriction update. Both have an unaligned object id at offset 5.
