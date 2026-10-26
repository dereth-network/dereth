# Sample: Chat Filter

**Provenance: design.** A walk-through of a sample plugin that ships with Dereth; its source is
[`dereth/plugins/chat-filter/`](../../../../dereth/plugins/chat-filter/).

| | |
|---|---|
| id | `network.dereth.chatfilter`, version 0.2.0, API 0.7; replaces `network.dereth.chatlogger` |
| files | `plugin.toml`, `main.lua` |
| permissions | `chat.filter` ("Hides the lines your rules hide, and shows the ones they highlight again in the style you chose."), `read.chat.private` ("Matches your tells with everything else, and keeps them in the log when a rule says to.") |
| capabilities | `ui.window` optional |

## What it does

Rules for the chat the character sees. A rule matches a line by one of three things:

- its **text**: the words anywhere in the line, whatever their case (no words: every line);
- its **channel**: the line's kind (`speech`, `tell`, `channel`, `combat_self`, …) or a channel's
  name in brackets at its start (`Trade` for `[Trade] …`);
- its **sender**: who said it, read from the line's words (`Bob says, …`, `Bob tells you, …`,
  `[Trade] Bob says, …`).

and does one of three things with it:

- **hide** it;
- **highlight** it: the line is hidden and shown again in the style the rule names (`help`,
  `advancement`, `magic`, `fellowship`, `world_broadcast` or `system`, each a chat kind of its own
  colour);
- **log** it: keep it, with the time and its kind, in a log of the last 300 lines kept in the
  character's store.

Every rule is on or off on its own, and **Rules on** switches them all. A new character starts with
one rule, logging tells.

Its **window** has two tabs. **Rules**: the list of rules (on or off, what it matches, the value,
what it does), the fields of a rule (**Match**, **Value**, **Action**, **Highlight as**), and
**Add** (a new rule from the fields), **Save** (the rule chosen in the list, changed to the fields),
**Remove** and **On / off** (the rule chosen); choosing a rule in the list fills the fields.
**Log**: what the rules kept, filling the window, and **Clear the log**.

The commands are shortcuts to the same:

| command | does |
|---|---|
| `/chatlog [n]` | prints the last `n` logged lines (10 when `n` is left out) |
| `/chatfilter add <words>` | a rule hiding lines containing `<words>` |
| `/chatfilter highlight <words>`, `/chatfilter log <words>` | a rule highlighting them, or logging them |
| `/chatfilter remove <words>` | forgets the rules on those words |
| `/chatfilter list` | the rules |
| `/chatfilter on`, `/chatfilter off` | switches the rules without forgetting them |

**It took the place of the Chat Logger.** The Chat Logger (`network.dereth.chatlogger`) kept every
line and hid the lines holding the words the player filtered. The Chat Filter's manifest says it
replaces it (`replaces`, [manifest.md](../../manifest.md)): turned on where the logger was, and,
the first time it opens a character's store, reading the logger's. Its words become hide rules of
their own, its log the filter's, its switch the filter's, and a rule logging every line keeps on
what the logger did.

## The source

The store opens at login (or at load, for a plugin turned on with a character in the world), so
the plugin remembers whether it is logged in: lines that arrive before (the server's welcome) are
matched by no rule. At login it reads the rules back, or takes up what the logger kept:

```lua
local function load()
	local c = store.character
	rules = c.rules
	if rules == nil then
		if c.filters ~= nil or c.log ~= nil then
			-- The Chat Logger's: its words hid lines, and it kept every line.
			rules = {}
			for _, words in ipairs(c.filters or {}) do
				table.insert(rules, { match = "text", value = words, action = "hide", on = true })
			end
			table.insert(rules, { match = "text", value = "", action = "log", on = true })
			c.filters = nil
		else
			rules = default_rules()
		end
	end
	...
end
```

Every line arrives as a `chat.line` event with its `text` and `kind`. Every rule that is on and
matches has its say: a hide hides it, a highlight hides it and prints it again in its style, a log
keeps it. `chat.hide(e)` hides the line in this frame (it takes the event or its `line` field) and
needs `chat.filter`; `chat.print(text, style)` prints a local line in a chat kind's style.

```lua
	if hide then
		chat.hide(e)
		hidden = hidden + 1
	elseif highlight then
		-- Shown again in the highlight's style, in its place.
		chat.hide(e)
		chat.print(e.text, highlight)
	end
```

`matches` lower-cases both sides and uses a plain `string.find` (the fourth argument `true`), so
the player's words are never read as a Lua pattern.

## How it is tested

- `dereth/testkit/tests/cpu/plugins/windows.rs :: the_chat_filters_rules_hide_highlight_and_log_lines_and_its_window_edits_them`:
  a rule added in the window hides a line and is switched off there, `/chatfilter add` adds the
  same kind of rule, a sender's lines are highlighted, and a tell is kept on the log page.
- `dereth/testkit/tests/cpu/plugins/windows.rs :: the_chat_filter_takes_up_what_the_chat_logger_kept`:
  a stand-in for the logger keeps words and a log; the Chat Filter, loaded after it, shows them as
  rules and its log, and hides by the words.
- The Core conformance cases `chat-lines`, `chat-filter` and `chat-filter-denied`
  ([conformance cases](../../reference/conformance-cases.md)) pin what it relies on: every line
  heard in order with its kind, hiding with `chat.filter`, and the refusal without it.
