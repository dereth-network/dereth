# Sample: Trackers

**Provenance: design.** A walk-through of a sample plugin that ships with Dereth; its source is
[`dereth/plugins/trackers/`](../../../../dereth/plugins/trackers/).

| | |
|---|---|
| id | `network.dereth.trackers`, version 0.3.0, API 0.7 |
| files | `plugin.toml`, `main.lua`, and a module per tracker: `combat.lua`, `experience.lua`, `mana.lua`, `items.lua`, `corpses.lua`, `players.lua`, with `util.lua` |
| permissions | none beyond `read` and `chat.print` |
| capabilities | `ui.window`, `ui.hud` and `overlay.arrow` optional |

## What it does

The trackers players kept open in the manner of Mag-Tools, fed by what the chat, the world and
appraisals show. One window has a page for each; a HUD in the top right corner shows the headline,
`70 dealt / 22 taken / 1 kills`. The HUD opens with the window.

| page | what it shows |
|---|---|
| Combat | damage dealt and taken this session; the share of critical hits; your attacks and the share that landed, with how many were evaded and resisted; the share of melee and missile attacks against you that you evaded, and of spells you resisted; the elements you took and dealt damage in; killing blows and kills; a row per creature (dealt, taken, your hit share, your evade and resist shares against it, killing blows, the elements it hits you with); the lifetime totals |
| Experience | experience and luminance this session, per hour since the first gain and per hour over the last five minutes, and the time to the next level at the session's rate |
| Mana | each item worn or wielded: its mana, how long that lasts at its burn, and whether it is `active` (its spells are on the character), `inactive` (they are not), `out` (no mana), `none` (no spells) or `unknown` (not yet appraised); the minutes under which it warns; the last low-on-mana line the server sent |
| Items | spell components, healing kits, mana stones, trade notes and salvage, by name: the count at the session's start, now, the change and the change per hour; the change in what they are worth, and per hour; the items it warns of, and a warning set (**Warn when** an item, picked in the list or typed, **is under** a count) or stopped |
| Corpses | every corpse seen this session, whose it is (`yours`, `fellow`, `player`, `your kill`, `other`), where it lies and how far; your own corpse, kept with the character; **Point at my corpse** and **Clear the arrow** |
| Players | each player seen this session: `here`, or how long ago it was last seen; where it was last seen, which for one gone from view is where it was when it went |

Everything a command changes is in the window too; the commands are shortcuts:

| command | does |
|---|---|
| `/track` | `Dealt 70, taken 22, 1 kills.` |
| `/track reset` | starts the session over (the lifetime stays) |
| `/track kills` | each creature you killed and how many |
| `/track xp` | the experience and luminance line |
| `/track mana`, `/track warn <minutes>` | each worn item's mana and time left; the mark under which an item is warned of (20 minutes at first) |
| `/track items`, `/track low <name> <count>`, `/track low <name> off` | the consumables; a warning when one falls under a count, or none |
| `/track corpse`, `/track corpse clear` | points the arrow at your corpse and says how far it is; takes the arrow down |
| `/track players` | the players seen |
| `/track monster <name>`, `/track lifetime` | one creature's figures; the lifetime's |

It reads only: no permission beyond the two every plugin has. Appraising needs none.

A kill is yours when the creature's corpse shows within a minute of your last blow on it. A
killing blow is yours when the server tells you so (a line naming the creature a moment after your
blow, in the chat's default kind) and the creature's corpse then shows; a death of your own in
between counts none. The warnings (a worn item under its minutes, a component under its count) are
said once, and again only after the item or the component was back over the mark.

## The source

### Combat: reading the lines

`combat.lua` reads a combat line into what it says, who, how much, and the element, or nil for a
line that is none of them. The line's own damage word names the element where it has one
(`… for 8 points of piercing damage!`); a spell's line has none, so the verb does: each element's
four verbs run from a light hit to a heavy one.

```lua
local VERBS = {
	Slashing = { "scratch", "cut", "slash", "mangle" },
	Piercing = { "nick", "stab", "impale", "gore" },
	Bludgeoning = { "graze", "bash", "smash", "crush" },
	...
}
```

```lua
	verb, who, n, word = string.match(text, "^You (%a+) (.-) for (%d+) points? of (.-)damage!")
	if verb ~= nil then
		return { what = "dealt", who = who, damage = tonumber(n), crit = crit, element = element(verb, word) }
	end
	verb, who, n = string.match(text, "^You (%a+) (.-) for (%d+) points? with .+$")
	if verb ~= nil then
		return { what = "dealt", who = who, damage = tonumber(n), crit = crit, element = element(verb), magic = true }
	end
```

Each piece of news is counted into three tallies: the session's, the lifetime's and the
creature's. The shares are worked out when shown: your attacks are those that landed and those
that did not.

```lua
function M.attacks(t)
	return t.hits + t.misses + t.resisted
end
```

The lifetime is kept with the character (`store.character.lifetime`), written out every two
seconds when it changed; a reset starts the session's tallies over and leaves it.

### Experience: two rates

`experience.lua` keeps the session's gains with their times. The hourly rate is the total over the
time since the first gain; the recent rate counts only the last five minutes' gains, over five
minutes (or the session, when it is shorter):

```lua
	local span = math.min(WINDOW, now - t.started)
	return span > 0 and sum * 3600 / span or 0
```

`character.xp` and `character.luminance` carry `gained`; `game.player.xp.to_level` is how far the
next level is, where the host has the experience table.

### Mana: appraisals, a few at a time

An item's mana is in its appraisal: integer properties 107 and 108 (its mana now and at most) and
float property 5, the points it burns a second. A timer appraises one worn item a second, and each
again after three minutes, so the plugin's work in any one entry stays small:

```lua
	local due = mana.due(1)
	if #due > 0 then
		task.spawn(mana.look, due[1])
	end
```

Between appraisals the mana is worked out from the burn, and the time left is the mana over the
burn. An item is `active` when one of its spells shows among `game.player.enchantments`. A
low-on-mana line from the server marks the item to be appraised again at once.

### Items, corpses and players

`items.lua` counts the pack by class and name (`game.inventory.items()`, a stack counting its size),
again whenever an item comes, goes or changes, and at least every half minute; the first count of
a session is its start. `corpses.lua` hears `world.object_created` with the class `Corpse`, keeps
where it lies (`obj.position`), and keeps the character's own with the character, so
`/track corpse` points the arrow there after the corpse is out of view, and after the plugin is
loaded again:

```lua
	if host.has("overlay.arrow") then
		markers.arrow:set({ position = mine.place, label = "Your corpse" })
	end
```

`players.lua` hears players come into view (and finds those already in view at the session's
start with `game.world:find{ class = "Player" }`), follows them with `world.object_moved`, and keeps
each where it was last seen once `world.object_removed` takes it from view.

## How it is tested

`dereth/testkit/tests/cpu/plugins/trackers.rs`, in a scripted world:

- `the_trackers_count_each_creatures_damage_by_element_and_the_share_of_attacks_that_landed`: the
  server's combat messages each way, two spells each way, evasions and resists, and its word on a
  death followed by the corpse: the totals, the shares, the elements, the creature rows, one
  killing blow, and a lifetime a reset keeps.
- `the_trackers_show_experience_and_luminance_per_hour_and_over_the_last_five_minutes`: two gains
  340 seconds apart; ten minutes in, the hourly rate counts both and the recent one only the
  second.
- `the_trackers_tell_how_long_each_worn_items_mana_lasts_and_warn_before_it_runs_out`: a robe
  whose spell is on the character, a hat whose is not, a ring with no spells and boots in the
  pack: the list in order of time left, each warned of once as it passes the mark, and the
  server's low-on-mana line noted.
- `the_trackers_count_the_consumables_used_and_warn_when_one_runs_low`: scarabs used, a bag of
  salvage looted: the changes, the rates, the worth, and the warning said once.
- `the_trackers_remember_your_corpse_and_point_the_arrow_at_it`: the character dies, is carried
  60 metres off, and `/track corpse` points the arrow at the corpse, also after the plugin is
  loaded again; corpses are told apart as yours, a player's and your kill.
- `the_trackers_keep_each_player_seen_and_where_it_was_last`: a player who walks on and goes
  from view is kept where he was last seen, and how long ago.

`dereth/testkit/tests/cpu/plugins/windows.rs :: the_trackers_window_sets_the_low_warnings_and_clears_the_arrow`:
a warning set and stopped on the Items page, beside one set by `/track low`; the arrow put on the
character's corpse by the Corpses page's button and taken down by Clear the arrow.
