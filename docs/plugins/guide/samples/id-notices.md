# Sample: ID Notices

**Provenance: design.** A walk-through of a sample plugin that ships with Dereth; its source is
[`dereth/plugins/id-notices/`](../../../../dereth/plugins/id-notices/).

| | |
|---|---|
| id | `network.dereth.idnotices`, version 0.3.0, API 0.7 |
| files | `plugin.toml`, `main.lua` |
| permissions | none beyond `read` and `chat.print` |
| capabilities | `ui.hud` and `ui.window` optional |

## What it does

When a corpse or chest opens, it appraises every item in it and tells the player about the ones
that matter: an item worth at least a set value (5,000 by default), one with a spell whose name
holds a chosen word (`Legendary`, `Epic` and `Major` by default), and a rare. In the manner of
Alinco, it also watches for names: an item in an opened container, or a creature or person coming
into view, whose name holds one of the names on its list (or is it, for a name written in quotes:
`"Pyreal Mote"`), as Alinco's lists of monsters and trophies do; and it notices a spell scroll of
a spell the character does not know, from a level up (6 by default; 0 for none). It notes players
and portals coming into view too.

Each notice is a chat line starting `[ID]`, and the last five are listed on a HUD at the left of
the screen, each in its kind's color; the player moves the HUD where they like by unlocking the
HUDs in the plugin panel ([ui.md](../../ui.md#huds)). Each kind of notice (rares, worth the value,
spell words, watched for, scrolls, players, portals) has its **alert**: `chat` (the chat line and
the HUD), `toast` (those, and a toast at the top of the screen; rares and names watched for by
default) or `off`. An item with several reasons is one notice, under the alert of the rarest.

Its **settings window** (from the plugin bar, or `/idn`) sets what matters, on three pages:
**Items** (**Worth a notice from**, the value; the **Spell words**, a list, a **Word** box with
**Add**, and **Remove** for the word chosen or typed; and the level from which scrolls of spells
not known are noticed), **Watch** (the names watched for, a **Name** box, **Only that name**, and
**Add** and **Remove**) and **Alerts** (each kind's), with a line saying what it notices now. The
commands are shortcuts to the same:

| command | does |
|---|---|
| `/idn` | opens the window, and says the settings |
| `/idn value <n>` | the value from which an item is worth a notice |
| `/idn word <text>`, `/idn unword <text>` | adds a word that makes a spell worth a notice, or removes it |
| `/idn clear` | forgets the words |
| `/idn watch <name>`, `/idn unwatch <name>` | watches for a name (in quotes: that name only), or stops |
| `/idn scroll <level>` | notices scrolls of spells not known from this level (0: none) |
| `/idn alert <kind> <how>` | a kind's alert (`rare`, `value`, `spell`, `watch`, `scroll`, `player`, `portal`): `chat`, `toast` or `off` |

The settings are kept per character.

## The source

An appraisal is an action, but it only reads: it needs no permission and no gesture, and the host
holds every plugin to five a second. So this plugin appraises on its own, with no click behind it,
and asks for nothing.

The settings come from the character's store with their defaults filled in, and every change,
from the window or a command, goes through `change`, which keeps them and shows them in the window:

```lua
local function change(f)
	local s = settings()
	f(s)
	save(s)
	refresh()
end
```

The window starts closed, as every window does: the plugin bar's button and `/idn` open it, and
the HUD opens with it.

When a container opens, a task looks at each item in turn: first what it says before it is
appraised (a name watched for; a scroll's spell, which the item carries), then its appraisal.
`act.appraise`'s promise settles with the appraisal itself: `{ success, ints, int64s, floats,
strings, bools, spells }`. Each reason is a kind and its words.

```lua
local function check(item)
	if noticed[item.id] then
		return
	end
	local why = on_sight(item)
	local ok, a = pawait(act.appraise(item.id))
	if ok and a ~= nil then
		for _, w in ipairs(interesting(item, a)) do
			table.insert(why, w)
		end
	end
	if #why > 0 then
		noticed[item.id] = true
		notice_reasons(item, why)
	end
end

on("inventory.container_opened", function(e)
	task.spawn(function()
		for _, item in ipairs(game.world:find{ container = e.id }) do
			check(item)
		end
	end)
end)
```

`interesting` reads the item's value by its property number (`item:int(19)`), each spell's name
from the spell table (`magic.spell(id)`, nil where the host has none), and the rare's id, which is
integer property 17 of the appraisal:

```lua
	local value = item:int(19) or 0
	if value >= s.value then
		table.insert(why, { "value", string.format("worth %d", value) })
	end
	for _, spell in ipairs(a.spells or {}) do
		local info = magic.spell(spell)
		local name = info and info.name or ""
```

```lua
	-- Rares carry a rare id (property 17).
	if (a.ints or {})[17] ~= nil then
		table.insert(why, { "rare", "a rare" })
	end
```

A scroll's spell is the figure outside the property tables at `0x0D000008`, and the spell table
says whether the character knows it, and its level:

```lua
	local spell = item.class == "Scroll" and s.scroll > 0 and item:int(SCROLL_SPELL) or nil
	local info = spell and spell > 0 and magic.spell(spell) or nil
	if info ~= nil and not info.known and (info.level or 0) >= s.scroll then
```

A notice goes to the chat and the HUD, and to a toast as well where its kind's alert says so:

```lua
	chat.print("[ID] " .. text, "system")
	if how == "toast" then
		pcall(ui.toast, text, 6)
	end
	table.insert(recent, 1, { { text = text, color = KIND[kind].color } })
```

Players, portals and the creatures watched for come from `world.object_created`, whose `class`
names the kind of object; the player's own character is left out by comparing ids, and an item
created inside a container waits for the container to open:

```lua
	local o = game.world:get(e.id)
	-- In view, not in a container (an item in a corpse is noticed when the corpse opens).
	local in_view = o ~= nil and (o.container == nil or o.container == 0)
	if in_view and e.class ~= "Portal" and watched(e.name) ~= nil then
		noticed[e.id] = true
		notice("watch", "Watched for: " .. tostring(e.name) .. " is here")
	elseif e.class == "Player" then
		notice("player", "Player nearby: " .. tostring(e.name))
	elseif e.class == "Portal" then
		notice("portal", "Portal: " .. tostring(e.name))
	end
```

## How it is tested

- `dereth/testkit/tests/cpu/plugins.rs :: the_looter_keeps_what_its_profile_keeps_and_id_notices_reports_it`:
  when the Looter opens a scripted corpse, ID Notices appraises its items and prints
  `[ID] Opal: worth 6000` for the opal worth 6,000.
- `dereth/testkit/tests/cpu/plugins/windows.rs :: id_notices_window_sets_the_value_and_the_spell_words_that_matter`:
  the value set in the window, a word added and another removed there, `/idn` reading back what the
  window set; in a chest the player opens, the ring worth more than the value is noticed and the one
  worth less is not.
- `dereth/testkit/tests/cpu/plugins/windows.rs :: id_notices_watches_for_names_and_unknown_scrolls_and_alerts_as_set`:
  in a chest, the scroll of a spell not known is noticed and one known is not; an item of a name
  watched for in quotes is, one that only holds it is not; a creature watched for by part of its
  name (added in the window) is noticed as it comes into view, with a toast; a portal, its alert
  off, is not.
