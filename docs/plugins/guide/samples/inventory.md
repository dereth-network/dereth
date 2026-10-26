# Sample: Inventory

**Provenance: design.** A walk-through of a sample plugin that ships with Dereth; its source is
[`dereth/plugins/inventory/`](../../../../dereth/plugins/inventory/).

| | |
|---|---|
| id | `network.dereth.inventory`, version 0.1.0, API 0.7 |
| files | `plugin.toml`, `main.lua` (its window, commands and the task that does the work), `log.lua` (the log of every character's items), `info.lua` (an item's line), `tools.lua` (stacking, packing, sets of gear, trading at a vendor), `suits.lua` (the suit builder) |
| contexts | `world` |
| permissions | `act.interactive` ("Stacks, packs, trades and puts on a set of gear or a suit when you click or type /inv."), `auto.craft` ("Merges partial stacks and puts items in their packs as they come, when you turn that on."), `auto.vendor` ("Sells and buys by a vendor's profile when its wares show, when you turn that on.") |
| capabilities | `ui.window` required |

## What it does

The inventory tools of Mag-Tools, written for Dereth from what they do for a player:

- **The log.** What each character holds and wears is logged for the world: a few seconds after it
  enters the world, and again a few seconds after its inventory changes. Each item's record keeps
  its name, class, count, where it is (worn, the main pack, or which side pack), value and burden,
  and, once the item has been appraised, what its appraisal tells: armor level, spells, set,
  workmanship, ratings, what it takes to wield, arcane lore, tinkers, whether it is attuned. With
  **Appraise** on (the default), the gear the character holds that has not been appraised is
  appraised, four a second, so its record is whole. The log lives in the plugin's world store, so
  every character of the world sees every other character's log.
- **Finding.** `/inv find <words>` (or the **Find** page) lists every logged item, of every
  character, whose name, spells or set's name hold each of the words: `/inv find epic strength`,
  `/inv find defender`. A result chosen in the window shows its line; `/inv info <words>` says the
  lines of the first five.
- **Item information.** When an appraisal of what the player has selected comes, a line of what
  it is goes to the chat (with **Info** on, the default): `Gold Ring: Epic Strength; Dmg 2; Value
  5,000`, its armor level or damage, spells, set, workmanship and tinkers, ratings, what it takes
  to wield, arcane lore, value and burden.
- **Stacking.** `/inv stack` merges partial stacks of one name, the top-most into the bottom-most
  with room, so the one partial stack left is the top-most; with **AutoStack** on, as items come.
- **Packing.** **PackRules** names where items go: `Scarab=2, Taper=2, Salvage=3` puts an item
  whose name holds `Scarab` in pack 2 (the first side pack). `/inv pack` moves each item to its
  rule's pack, while that pack has room; with **AutoPack** on, as items come.
- **Sets of gear.** `/inv gear save <name>` keeps what the character wears as a set;
  `/inv gear wear <name>` puts each of its items on again, one at a time, and says what it could
  not (an item no longer held). The sets are kept with the character, and never sold.
- **Trading at a vendor.** A vendor's own loot profile, `vendors/<its name>.utl` in the plugin's
  data folder, says what to sell to it and what to buy from it, as Mag-Tools reads one: when the
  vendor's wares show (with **AutoTrade** on), or on `/inv trade`, it sells what the profile's Sell
  rules match from the packs (nothing worn, no pack, nothing in a saved set), then buys what a Keep
  # rule matches up to its count and one of what a Keep rule matches that the character lacks, no
  more than its pyreals pay for.
- **Suits** (in the manner of SuitBuilder). From the gear every character of the world has
  logged, the best suit for the character playing: armor and underclothes over each part of the
  body (a hauberk over the chest, abdomen and upper arms takes all three), a necklace, two
  bracelets, two rings, a trinket and a cloak, no two pieces over one place. What matters, in this
  order: the spells asked for (`SuitSpells`: `Strength, Invulnerability`), each family counted
  once at the best tier the suit holds (Legendary, Epic, Major, Moderate, Minor), and the pieces
  of the set asked for (`SuitSet`: `Defender's`, up to five); then the items' ratings; then the
  armor level over each part of the body. `SuitFrom` names the characters to draw from (every one
  when empty). A piece is left out when it takes a higher level to wield than the character has,
  when it is attuned to another character, or when its appraisal is not logged. A piece locked
  (`/inv suit lock <words>`, or **Lock the piece chosen**) is in every suit. The search tries the
  suits in turn, the most promising pieces first, leaving a branch once nothing in it could beat
  the best suit found, so the best it reports is the best there is among each place's eight most
  promising pieces; it shows that and the four best met on the way. `/inv suit wear` (or **Put on
  what is yours**) puts on the pieces the character holds, and names the characters holding the
  rest.

Its **window** has five pages: **Find** (the words, **Search**, the results and the chosen one's
line), **Characters** (each logged character, its items, level and when it was logged; **Log this
character now** and **Forget the one chosen**), **Tools** (the settings above, **Stack now**,
**Pack now** and **Trade now**), **Suits** (the spells, the set and the characters to draw from,
**Build**, the suits found and the chosen one's pieces, **Put on what is yours**, **Lock the piece
chosen** and **Unlock all**) and **Gear** (the sets, a name, **Keep what you wear**, **Put it on**
and **Remove**). The commands are shortcuts:

| command | does |
|---|---|
| `/inv` | opens the window |
| `/inv find <words>`, `/inv info <words>` | the items of every character holding the words, and their lines |
| `/inv log`, `/inv chars`, `/inv forget <character>` | logs this character now; the characters logged; forgets one |
| `/inv stack`, `/inv pack`, `/inv trade` | merges partial stacks; puts items in their packs; trades with the open vendor |
| `/inv gear`, `/inv gear save\|wear\|remove <name>` | the sets of gear, and keeping, wearing or forgetting one |
| `/inv suit`, `/inv suit wear [n]` | builds the best suits; puts on the pieces of suit `n` (1, the best) the character holds |
| `/inv suit lock <words>`, `/inv suit unlock [<words>]` | keeps a logged piece in every suit; lets one go, or all |
| `/inv set <setting> <value>`, `/inv settings` | a setting (`Info`, `Appraise`, `AutoStack`, `AutoPack`, `PackRules`, `AutoTrade`, `SuitSpells`, `SuitSet`, `SuitFrom`), and all of them |

## The source

One task does the work, a step every quarter second: a trade asked for, then a merge, then a move
to a pack, then the log when it is due, then an appraisal. Each step does one thing and awaits it,
so the plugin never asks the client for two inventory changes at once. A step that walks the whole
inventory yields to the next frame every few dozen items (`task.sleep(0)`), so a pack of hundreds
of items stays within an entry's time budget.

An item's record is read in one call, `item:properties()`, every property the client knows of it
by number:

```lua
	local props = item:properties() or {}
	local ints = props.ints or {}
```

and the log is kept per character in the world store, with an index of the characters logged:

```lua
	store.world[key(name)] = rec
```

The vendor's profile is read through `dereth.loot`, and each item of the pack classified by it:

```lua
			local hit = profile:classify(it)
			if hit ~= nil and hit.action == "sell" then
```

The suit builder is a branch-and-bound search over the places of the body, in a fixed order: at
each step the first place still free is given each of its candidates that fits, or left empty, so
each suit is met once. Its bound is the most the free places could still add: their best ratings
and armor, the set's pieces up to its cap, and each family's rise to the best tier among them.
Places are masks of the bits of where an item may be worn, combined with `bit32`:

```lua
	local function fit(c, place)
		if c.either ~= nil then
			return band(covered, place) == 0 and place or nil
		end
		return band(covered, c.cover) == 0 and c.cover or nil
	end
```

## How it is tested

- `dereth/testkit/tests/cpu/plugins/inventory.rs :: the_inventory_logs_each_character_and_finds_items_across_them`:
  one character's items are logged, and another character's client, sharing the stores, finds
  them by a word of a name, a spell's words and a set's name; a logged item's line; an item the
  player selected says what it is when its appraisal comes.
- `dereth/testkit/tests/cpu/plugins/inventory.rs :: the_inventory_stacks_packs_puts_gear_on_again_and_trades_by_a_vendors_profile`:
  two partial stacks merged; a pack rule moving the stack to the side pack; a set of gear put on
  again after a piece came off; at a vendor, its profile's Sell rule selling the dagger and its
  Keep # rule buying tapers up to its count.
- `dereth/testkit/tests/cpu/plugins/inventory.rs :: the_suit_builder_finds_the_best_suit_across_characters_with_its_locks`:
  from two characters' gear, three pieces of the set asked for with the rings' Legendary Strength
  and Minor Invulnerability beat a hauberk's Major Invulnerability and armor; a bracelet attuned to
  the other character and leggings above the character's level are left out; with the hauberk
  locked it is in the suit; the pieces the character holds are put on, and the rest named with
  their holders.
