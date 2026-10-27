# Getting started: a plugin in Lua

**Provenance: design.** A walk-through of the plugin interface; the rules are in the
[specification](../README.md).

This page builds a small plugin, a corpse counter, in five steps: the package, an event, a window,
a command and a store, and then an action that needs a permission. Each step is loaded and tried
before the next. The language is Luau, a Lua 5.1 derivative; the [README](../README.md#how-a-plugin-runs)
says which subset stays portable to plain Lua 5.1.

## 1. The package

A plugin is a folder with `plugin.toml` at its root. (`dereth-plugin new` makes a repository with
one in it, ready to publish: [publishing.md](publishing.md). This guide builds one by hand.) Make a
folder `kills` holding two files:

```toml
# kills/plugin.toml
id = "example.kills"
name = "Kill Counter"
version = "0.1.0"
api = "0.5"

[capabilities]
optional = ["ui.window"]
```

```lua
-- kills/main.lua
print("Kill Counter loaded")
```

`id` is lowercase reverse-DNS and unique; `api` is the lowest API version the plugin needs, and a
host runs it when the majors match and its own minor is at least this one. `entry` is left out, so
the entry file is `main.lua`. Every other key and the full rules are in [manifest.md](../manifest.md);
an unknown or misspelt key refuses the whole manifest, so a typo cannot silently drop a permission.

**Load it.** In the desktop client, pass the folder and ask for reloads on every save:

```text
cargo run -p dereth-client -- --plugins path/to/kills --plugin-dev
```

A `--plugins` folder loads whatever the client's **Plugins (experimental)** setting says. (Or
copy the folder into the `plugins` folder of the client's settings folder; the client finds it
there, and loads it once you tick Plugins (experimental) on the Client Options page and turn it on
in the plugin panel; [install-and-update.md](install-and-update.md) says where that is.) Type
`/plugin` to open the plugin panel: it lists the plugin as running and shows its console, where
`print` writes, with the line `Kill Counter loaded`.

## 2. Hear an event

The entry file's top level runs once, at load. It registers handlers with `on(name, handler)`; each
handler receives one table. A corpse appearing is an object created with the class `Corpse`:

```lua
local kills = 0

on("world.object_created", function(e)
	if e.class == "Corpse" then
		kills = kills + 1
		print("corpses seen:", kills)
	end
end)
```

A misspelt event name is an error at the `on` call, so it shows in the console at once. Every
event and its fields is in [events.md](../events.md); the order in which a frame's events arrive
is there too.

## 3. A window and a command

A window is declared, not drawn: the plugin says what it holds and the host draws it its own way
(the headless client keeps the tree without drawing it). `ui.window` answers a handle; `win:get(id)`
is a node, and setting one of its properties changes the window.

```lua
local win = nil
if host.has("ui.window") then
	win = ui.window{ id = "main", title = "Kills", width = 20,
		ui.label{ id = "count", text = "0" },
		ui.button{ id = "reset", text = "Reset" },
	}
end

local function show()
	if win then
		win:get("count").text = tostring(kills)
	end
end

on("ui.clicked", function(e)
	if e.id == "reset" then
		kills = 0
		show()
	end
end)

commands.add("kills", "/kills says how many corpses have appeared", function(args, words)
	chat.print("Corpses seen: " .. kills, "system")
end)
```

`ui.window` is guarded by `host.has("ui.window")` because the manifest lists the capability as
optional: a host without it still loads the plugin. A plugin that cannot work without a window
lists it under `required` instead, and a host without it refuses the plugin with that reason. The
node kinds and their properties are in [ui.md](../ui.md) and the generated
[ui-nodes.md](../reference/ui-nodes.md).

`commands.add` claims `/kills`; the player's line arrives as `args` (the rest of the line) and
`words` (it split on spaces). A verb the client already has, `/plugin`, or one another plugin
claimed cannot be claimed. `chat.print` writes a local line, seen only by the player.

Call `show()` from the event handler too, and save: with `--plugin-dev` the client reloads the
plugin. The plugin bar now has a button for the plugin: a click opens the window, which starts
closed as every window does until the player asks for it, and a second click closes it. A window
open when the plugin reloads stays open. The player can drag the window's edges to size it; add
`resizable = true` to have its nodes laid out again at that size and hear it, or
`resizable = false` to keep its size ([ui.md](../ui.md#resizable-windows)).

## 4. Keep something

A plugin has three stores: `store.global`, `store.world` and `store.character`. Each is a table
whose values are written out at logout, at unload and every 60 seconds. `store.global` is open from
load; the other two open at login, and using them before is an error.

```lua
on("world.object_created", function(e)
	if e.class == "Corpse" then
		kills = kills + 1
		store.global.total = (store.global.total or 0) + 1
		show()
	end
end)
```

Values are nil, booleans, numbers, strings and tables of them. A reload loses the plugin's
locals; to carry them over, define `on_unload` to return a plain value and `on_reload(state)` to
take it back ([lifecycle-and-sandbox.md](lifecycle-and-sandbox.md)).

## 5. Act, with a permission

A plugin changes the game only through **actions**: each becomes the same request the game's own
interface makes for the same gesture, and answers a **promise** at once. Inside a task, `await`
waits for it and raises its failure; `pawait` answers `ok, value_or_reason` instead.

Using an object needs a permission. A use that follows the player's click on the plugin's own
window is *interactive* and needs `act.interactive`; request it in the manifest with the reason the
player is shown:

```toml
[permissions]
"act.interactive" = "Opens the selected corpse when you click Open."
```

```lua
-- in the window: ui.button{ id = "open", text = "Open" },
on("ui.clicked", function(e)
	if e.id == "open" then
		task.spawn(function()
			local target = game.selection()
			if target == nil or target.class ~= "Corpse" then
				chat.print("Select a corpse first.")
				return
			end
			local ok, why = pawait(act.use(target.id))
			print(ok and "opened" or ("not opened: " .. tostring(why)))
		end)
	end
end)
```

The manifest's permissions tell the player what the plugin uses; nothing is asked, and a plugin
the player turns on loads at once. The click gives the plugin an **activation** of 30 seconds in which its actions
count as interactive. The same use with no click behind it would be *automated*: the world's
policy would have to allow that domain (`loot`, or `nav`) without a gesture. [permissions.md](../permissions.md) has the rule
and [actions.md](../actions.md) every action, what ends it and its timeout.

## Trying it without a window: the headless client

The headless client runs a script against the game with no window and no graphics device. It keeps
each plugin's windows as trees, which a script can print and click. It reads the game's data files from `--dat-dir` (or the working folder)
and replays the recorded sessions in `fixtures/packet-captures/`.

```text
# kills.script
login long-solo-play +Aldric
replay 120
ui dump
ui click example.kills main reset
say /kills
tick 1
chat
plugin console example.kills
```

```text
cargo run -p dereth-headless -- --script kills.script --plugins path/to/kills --dat-dir <the game's data folder> --captures fixtures/packet-captures
```

Two minutes into that recording the character has killed a sparring golem, so `ui dump` shows
`label #count text="1"`; after the click on **Reset**, `/kills` answers `Corpses seen: 0`. `ui
dump` prints each window in the text form of [ui.md](../ui.md#the-tree-as-text), which is also
what the conformance suite compares; `tick 1` runs one more frame, so the command's answer is
there for `chat`. The script verbs are listed by `dereth-headless --help`.

## Next

- Check it as the clients will, with `dereth-plugin check kills`, and publish it in a plugin
  source of your own, which players add in the launcher: [publishing.md](publishing.md).
- How players install and update plugins: [install-and-update.md](install-and-update.md).
- What stops a plugin, and what it cannot reach: [lifecycle-and-sandbox.md](lifecycle-and-sandbox.md).
- Working plugins, walked through: [samples/](samples/README.md). The [XP Meter](samples/xp-meter.md)
  is the smallest; the [Looter](samples/looter.md) shows actions and promises in full.
