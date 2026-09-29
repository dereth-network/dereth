# Trade, housing, vendors and the rest

Trading, vendors, housing, the board games, books and inscriptions, the barber, portal storms and a
few loose ends.

**Provenance:** from the wire format, which public readers document.

**Readers:** `dereth_protocol::trade`. Pinned by the ten tests in
`core/protocol/src/trade.rs` — notably
`trade_add_to_trade_has_a_third_dword_and_register_has_a_double_stamp`,
`update_restrictions_is_byte_packed_like_the_stack_size_message`,
`item_profile_packs_a_signed_24_bit_amount_and_an_optional_weenie_desc`,
`page_data_has_a_versioned_and_an_unversioned_form`, `har_has_three_shapes`,
`house_data_reads_type_and_maintenance_before_the_two_price_lists` and
`house_profile_reads_nine_dwords_before_the_name_and_the_two_lists`.

## 1. A warning that applies to this whole family

Several of these messages **are never sent by the servers people test against**, and a few appear in
no recorded traffic at all. A reader and a writer that were derived from the same wrong assumption
round-trip each other perfectly and stay green for years. Three structures in this family were wrong
in exactly that way, each caught only when a real shard finally sent one.

So: where this page says a field order is surprising, **the surprise is the point**. A struct's field
offsets are not its wire order, and this family is where that bites hardest.

## 2. Trade

| opcode | direction | body |
|---|---|---|
| `0x01F6`, `0x01F7` open / close negotiations | C2S | |
| `0x01F8` add to trade | C2S | item, slot |
| `0x01FA` accept | C2S | **a whole packed trade object** |
| `0x01FB` decline | C2S | |
| `0x0204` reset | C2S | |
| `0x01FD` register trade | S2C | initiator, partner, **a stamp that is an IEEE double** |
| `0x01FE`, `0x01FF` open / close | S2C | |
| `0x0200` add to trade | S2C | item, side, **and a third dword** |
| `0x0201` remove from trade | S2C | item, side |
| `0x0202`, `0x0203` accept / decline | S2C | |
| `0x0205` reset | S2C | |
| `0x0207` failure | S2C | item, reason |
| `0x0208` clear acceptance | S2C | "someone changed the contents; both accepts are void" |

Notes:

- **The accept message is the only client-to-server message in the game that sends a whole packed
  object** rather than scalars. The server uses it to detect desynchronisation.
- **In that object the scalars come first and the two lists last** — partner, stamp, status,
  initiator, and the two accept flags, *then* the two content lists. The struct's field order is the
  other way round, and reading in struct order takes the partner's object id as a list length.
- **The side field is 1 for self and 2 for partner**; anything else is ignored.
- **The received accept message's source field is three-valued**: zero clears *your* accept flag,
  your own id sets it, and anything else sets the partner's.
- A failure rolls the item back off your side of the window locally, before anything else.
- **Register trade (`0x01FD`) is what opens the window.** Open trade (`0x01FE`) causes no visible
  change, and the most widely used emulator server never sends it. Close trade (`0x01FF`) empties both lists and the partner's name but
  leaves the window on screen. Only the local close button hides it, so each player closes their own.

## 3. Vendors

A vendor profile is what it buys (a type mask), a value range, a magic flag, buy and sell price
multipliers, and an alternate currency.

**An item profile has a genuinely unusual encoding**: the first dword packs a **24-bit signed**
amount together with a flag byte, and the item's full description follows **only when that byte is
all ones**. So:

```text
v = u32
amount = sign-extend v's low 24 bits
if (i32)v >> 24 == -1: a full object description follows
```

| opcode | body |
|---|---|
| `0x0062` vendor info | the merchant, a profile, a list of item profiles |
| `0x005F` buy | the vendor, items, **and a trailing alternate-currency dword** |
| `0x0060` sell | the vendor, items — **no currency field** |

Buy and sell are *not* the same shape; only one of them carries the currency.

## 4. Housing

### 4.1 The byte-packed message

`0x0248`, the restriction update, is the **second** of the two byte-packed messages in the protocol —
the other is the [stack-size update](07-inventory-and-items.md):

```text
+0x00  u32  opcode
+0x04  u8   sequence
+0x05  u32  sender      <- unaligned
+0x09  ...  the restriction table
```

The catalogue documents it as aligned like everything else. Reading it that way corrupts the
restriction table and everything after it.

### 4.2 The access record and the restriction table

Both have **three shapes keyed on their first dword**, in the same way: when the high word is zero
the dword *is* the bitmask; a small version number gives an older table; a large one gives the modern
table. The access record also has **a trailing align to 4 that its oldest shape does not**.

### 4.3 Two structures whose order surprises everyone

**A house profile** is **nine dwords first**, then the name and the two price lists:

```text
id, owner, bitmask, min level, max level,
min allegiance rank, max allegiance rank,      # version >= 1
maintenance free,                              # version >= 2
type,                                          # version >= 3
name, buy list, rent list
```

The version is hard-coded to 3 by the client, and the size floor grows with it. A reader that puts
the name and the price lists where the structure's layout suggests — after the owner, after the
bitmask — misaligns **everything after the second dword**.

**A house payment** is read in *member* order, not layout order: **count, paid, class id**, then the
name and the plural name. Reading it as class-id-first decodes a live shard's price list with the
quantity in the class field.

**The house data message** reads the two times, then **the type and the maintenance flag**, and only
then the two price lists and the position.

### 4.4 The rest of housing

Buying and renting share a layout. Beyond that there are about twenty small actions — query, abandon,
tele-to-house, tele-to-mansion, boot everyone, the guest and storage permission commands, the
allegiance permission commands, hook visibility, the open-house flag, and the available-house
listing — plus the rent-payment update and the guest list.

## 5. The board game

| opcode | direction | body |
|---|---|---|
| `0x0269` join | C2S | a game id and a team |
| `0x026A` quit | C2S | |
| `0x026B` move | C2S | **four raw integers**, not a move structure |
| `0x026D` move pass | C2S | |
| `0x026E` stalemate | C2S | |
| `0x0281` join response | S2C | a team; **−1 means refused** |
| `0x0282` start | S2C | the team that moves first |
| `0x0283` move response | S2C | a result code; the server's codes only ever reject |
| `0x0284` opponent turn | S2C | a game id, a team, and the variable-length move form |
| `0x0285` opponent stalemate state | S2C | a game id, a team, and an on/off flag |
| `0x028C` game over | S2C | |

The outgoing move and the incoming turn are **different shapes for the same idea**, which is the kind
of asymmetry that only shows up when both directions are implemented.

## 6. Books and inscriptions

| opcode | direction | body |
|---|---|---|
| `0x00B4` book open | S2C | book id, a page maximum, the page list, the inscription, the scribe's id and name |
| `0x00B6`, `0x00B7` add / delete page response | S2C | book id, page number, success — 16 bytes with the opcode |
| `0x00B8` page data response | S2C | an object id, a page number, and one page's data |
| `0x00AA` book data | C2S | |
| `0x00AB` modify page | C2S | book id, page number, text |
| `0x00AC` add page | C2S | |
| `0x00AD` delete page, `0x00AE` page data | C2S | a shared layout: book id, page number |
| `0x00BF` set inscription | C2S | an object id and the text |

**Page data is versioned**, and the version is not a leading field: the dword after the author's
account is **either the "text included" flag directly, or a marker whose high half is all ones**. Only
one marker value is followed by two further flag dwords. A reader that assumes the un-versioned shape
works on some books and not others.

## 7. The odds and ends

The barber start (`0x0075`, S2C) and finish (`0x0311`, C2S), the four portal-storm messages
(`0x02C9`–`0x02CC`, S2C), and an advocate teleport (`0x00D6`, C2S: a target name and a
destination position). The barber's finish message carries a fixed 0x44-byte payload after the
action header: the sub-type dword and the same sixteen appearance dwords the start message carries,
in the same order. The first two portal-storm warnings carry one float extent,
and an extent of zero or less resets the warning timer.

## 8. Where reimplementations differ

| topic | the client | common reimplementations |
|---|---|---|
| The trade object | Scalars first, lists last. | Struct order, which reads an object id as a list length. |
| The register-trade stamp | A double. | The catalogue says a long. |
| The received add-to-trade | Three dwords. | The catalogue lists two. |
| The restriction update | Byte-packed. | Documented as aligned. |
| The house profile and payment | Nine dwords first; count-paid-class. | Layout order, which misaligns everything. |
| Page data | Versioned. | Frequently only the un-versioned shape. |
| Buy versus sell | Only buy carries a currency. | Easy to make symmetric. |
