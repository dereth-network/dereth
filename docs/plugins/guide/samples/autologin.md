# Sample: Auto-login

**Provenance: design.** A walk-through of a sample plugin that ships with Dereth; its source is
[`dereth/plugins/autologin/`](../../../../dereth/plugins/autologin/).

| | |
|---|---|
| id | `network.dereth.autologin`, version 0.2.0, API 0.7 |
| files | `plugin.toml`, `main.lua` |
| permissions | `session.control` ("Chooses your character at the character list and logs out when you ask to switch."), `chat.send` ("Types the commands you listed once you are in the world.") |
| capabilities | `session.pregame` and `ui.window` required |

## What it does

When the character list comes up, it enters the world as the character the player chose. When the
server refuses because that character's last session is still in the world, it tries again every
five seconds, twelve times at most (a minute). After each login it types the lines the player
listed, in order, a second apart. "Log in the next one" logs the next character on the list in,
once.

Its **window** holds all of it, at the character list and in the world:

- **Log in automatically**, on or off;
- **Character**: the account's characters, as the character list shows them (choosing one turns
  logging in on);
- what it does now, in a line;
- **Typed after login, in order**: the lines, a **Line** box and **Add**, and **Remove**, **Up**
  and **Down** for the line chosen in the list;
- **Log in the next one**: at the character list, the character after the chosen one logs in at
  once; in the world, the character logs out and the next one on the list logs in.

The commands are shortcuts to the same:

| command | does |
|---|---|
| `/autologin` | opens the window, and says the settings |
| `/autologin set [name]` | logs this character (or the one named) in automatically |
| `/autologin on`, `/autologin off` | logs the chosen character in automatically, or stops |
| `/autologin cmd add <line>` | types `<line>` after each login |
| `/autologin cmd remove <n>` | forgets the nth line |
| `/autologin cmd clear` | forgets the lines |
| `/autologin next` | logs the next character in, once |

Settings are kept per world and account, in the global store. Choosing a character without the
player is session control: a world's policy may deny it (a world that publishes none allows it).

## The source

A plugin loads before login, so it hears the pre-game. The global store is open from load; the
plugin keys its settings by world and account, which `session.info()` names. Settings kept by
version 0.1, which named a character only while it was on, read as on:

```lua
local function settings()
	local s = store.global[key()] or {}
	s.commands = s.commands or {}
	if s.on == nil then
		s.on = s.character ~= nil
	end
	return s
end
```

The window is declared once, and `refresh()` shows the settings in it whenever they change: the
choice's options are the character list (`session.info().characters`), with the chosen name kept
among them even when it is not on this account. Every change, from the window or a command, goes
through one function, so the two can never disagree:

```lua
local function change(f)
	local s = settings()
	f(s)
	save(s)
	refresh()
end
```

`session.pregame` arrives when the character list is up, with the list. The plugin finds the
chosen name without regard to case and enters a second later; `session.select_character(name)` is
a promise that completes once the character is in the world, or fails with the server's reason.
`character_in_world` (and `character_in_world_server`) mean the last session has not left yet, and
trying again after a few seconds usually succeeds:

```lua
local function enter(name)
	tries = tries + 1
	task.spawn(function()
		local ok, why = pawait(session.select_character(name))
		if ok then
			print("logged in as", name)
			return
		end
		print("login refused", name, why)
		if (why == "character_in_world" or why == "character_in_world_server") and tries < MAX_TRIES then
			task.after(RETRY_MS, function()
				enter(name)
			end)
		end
	end)
end
```

After login it types the listed lines in order, each a second after the one before, with
`chat.command`, which adds the leading `/` when it is missing and needs `chat.send`.

"Log in the next one" picks the character after the one in the world (or, at the character list,
after the chosen one) on `session.info().characters`. At the character list it enters it at once;
in the world it remembers it for the next character list only, and logs out with
`session.logout()`.

## How it is tested

- `dereth/testkit/tests/cpu/plugins.rs :: auto_login_enters_the_chosen_character_and_retries_while_it_is_still_in_the_world`:
  in a scripted pre-game with two characters, the second still in the world once, `/autologin set
  Bravo`, `/autologin cmd add /em waves` and `/autologin next` log out, are refused once with
  `character_in_world`, enter as Bravo on the retry, and type the emote after login.
- `dereth/testkit/tests/cpu/plugins/windows.rs :: auto_logins_window_chooses_the_character_and_orders_the_lines_typed_after_login`:
  the window lists the characters, choosing one turns logging in on as `/autologin set` does, two
  lines added and reordered are typed in the window's order after login, and "Log in the next one"
  at the character list logs the next character in.
