# Sample: Follow

**Provenance: design.** A walk-through of a sample plugin that ships with Dereth; its source is
[`dereth/plugins/follow/`](../../../../dereth/plugins/follow/).

| | |
|---|---|
| id | `network.dereth.follow`, version 0.2.0, API 0.7 |
| files | `plugin.toml`, `main.lua` |
| permissions | `peers.local` ("Talks to Follow in your other clients on this machine: where each character is, and the lines the leader sends."), `act.interactive` ("Follows the leader when you click Follow or type /follow."), `auto.nav` ("Walks to where the leader was last seen when it is out of sight, where the world allows it."), `chat.send` ("Types the lines the leader sends to its followers.") |
| capabilities | `peers.local`, `nav.moveto` and `ui.window` required |

## What it does

Multi-box following between the player's clients on one machine. Twice a second each client tells
the others where its character is, and whether it leads. A client told to follow a leading
character follows it while it can see it; when it cannot, it walks to where the leader last said it
was. The leader can also have every follower type a line.

Its **window**: what it is doing now (leading, following whom, neither), **Lead** and **Stop**, the
**Leader** choice (the player's other clients on this machine, the characters the other copies of
Follow say they are, heard in the last ten seconds) with **Follow**, and a line **To all
followers** with **Send** (at the leader). The commands are shortcuts to the same:

| command | does |
|---|---|
| `/follow lead` | this client leads |
| `/follow <leader>` | this client follows the leading character, by name |
| `/follow stop` | stops leading or following |
| `/follow all <line>` | (at the leader) every follower types `<line>` |

Load it in every client that takes part. The clients reach each other through the host's bus
between clients on one machine: loopback datagrams on the desktop, a broadcast channel between tabs
in the browser. A message reaches only the same plugin in the other clients.

## The source

Every client broadcasts its character's id, name and position on the channel `where`, and whether
it leads. `peers.send` reaches the same plugin in every other client; a plugin sends at most 20
messages a second, and the data is any plain value up to 64 KiB. Hearing these is how each copy
lists the others in its window.

```lua
task.every(500, function()
	now = now + 0.5
	if game.player.id ~= nil then
		peers.send("where", {
			id = game.player.id,
			name = game.player.name,
			position = game.player.position,
			leading = leading,
		})
	end
	refresh()
end)
```

A follower hears `peers.message` with `from` (the sending client's character), `channel` and
`data`. It keeps each leader's latest message, so `/follow <leader>` can start at once, and acts
only on the leader it was told to follow:

```lua
on("peers.message", function(e)
	if e.channel == "where" then
		latest[string.lower(e.from)] = e.data
	end
	if leader == nil or string.lower(e.from) ~= string.lower(leader) then
		return
	end
	if e.channel == "where" then
		chase(e.data)
	elseif e.channel == "all" and type(e.data) == "string" then
		chat.command(e.data)
	end
end)
```

`chase` has two ways to move. When the follower's client knows the leader's character (the object
is in view), `nav.follow(id, 3)` closes to within 3 metres and holds there until stopped or the
object is out of sight for 3 seconds. Started by the `/follow` command, a gesture, it needs
`act.interactive`, and a follow started by a gesture keeps running after the activation ends.

```lua
local function chase(msg)
	if following then
		return
	end
	local seen = msg.id and game.world:get(msg.id)
	if seen ~= nil then
		following = true
		task.spawn(function()
			local _, why = pawait(nav.follow(msg.id, 3))
			following = false
			print("follow ended", why)
		end)
	elseif msg.position ~= nil and nav.owner() == nil then
		task.spawn(function()
			print("walked to leader", pawait(nav.move_to(msg.position, 3)))
		end)
	end
end
```

When the leader is out of view, the follower walks to its last position, but only when no plugin
is moving the character (`nav.owner()` is nil). Within the activation of the player's last
gesture the move is interactive; after it, the move is automated, and a world that allows
navigation only with a gesture fails its promise (`denied_by_world`, printed). A world that
publishes no policy allows it.

`/follow all` sends the typed line on the channel `all`; each follower types it with
`chat.command`.

## How it is tested

- `dereth/testkit/tests/cpu/plugins.rs :: follow_brings_another_client_to_the_leader` runs two
  hosts on one bus in scripted worlds: `/follow lead` in one, `/follow Leader` in the other; the
  leader's character moves, and the follower ends within range of it.
- `dereth/testkit/tests/cpu/plugins/windows.rs :: follows_window_lists_the_other_clients_to_follow_and_sends_a_line_to_the_followers`:
  each client lists the other; Lead, the leader chosen and Follow, and Stop by the window's
  buttons; a line sent from the leader's window is typed by the follower.
