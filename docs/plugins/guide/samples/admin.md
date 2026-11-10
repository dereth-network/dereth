# Sample: Admin

**Provenance: design.** A walk-through of a sample plugin that ships with Dereth; its source is
[`dereth/plugins/admin/`](../../../../dereth/plugins/admin/).

| | |
|---|---|
| id | `network.dereth.admin`, version 0.1.0, API 0.7 |
| files | `plugin.toml`, `main.lua`, `lines.lua`, `commands.txt` |
| permissions | `chat.send`, `act.interactive` |
| capabilities | `ui.window` required |

## What it does

It gives an administrator their chat commands as a window of forms. Its tabs:

- **Players:** who is on, and what to do with the one named, from teleporting to banning.
- **Travel:** coordinates, points of interest, dungeons, exact locations and the recall slots.
- **Target:** what the selection is, and what can be asked of it.
- **Myself:** the administrator's own powers.
- **Spawn:** objects, loot, salvage and the test kits.
- **World:** messages, the server's state, shutting down, the environment, events and server
  properties.
- **Commands:** a search of every command.

The line each button sent, and the server's replies, show under the tabs.

| command | does |
|---|---|
| `/admin` | opens or closes the window (Ctrl+Shift+A) |

Only a character with the administrator flag has the window. For any other character the plugin
declares no window and no hotkey, and `/admin` says why. The flag only decides what is drawn. Each
button sends the line an administrator would type, so the server checks it as it checks a typed
one, and refuses it for a character without the rights.

## The source

`commands.txt` is the table: each command a server of this project answers from chat, with its
group, its flags, what goes after it and what it does. Its flags:

- `confirm`: hard to undo, so it takes a second press.
- `selection`: alone, it acts on the selection.
- `selection-if-any`: it acts on the selection when there is one.

`lines.lua` reads the table and holds the helpers a line is built from. One of them quotes a value
of several words so it reaches the server as one.

`main.lua` checks the flag once a second, as bool property `0x2C` of the character
(`game.world:get(game.player.id):bool(0x2C)`). It declares the window when the flag comes. When
the flag goes, it puts a notice in the window's place, so the forms and the hotkey go with it.

A button's action builds its line from the boxes, or says what is missing.

- **Confirmation.** A dangerous button arms on its first press and sends on the second. The arming
  is held to the same line and the same selection, so pressing again after selecting something
  else arms it afresh.
- **The selection.** A command on the selection asks `act.appraise` first. It sends the line only
  when the appraisal succeeds, so the server acts on what is selected now, not on an older target.
- **Replies.** They are the lines of the server's own kinds (`system`, `default`, `magic`,
  `help`, `world_broadcast`, `error`) that arrive within three seconds of the command or its last
  reply. The list of players and the location box are filled only from those kinds, so another
  player's speech or emote that reads like a reply fills neither.

The hotkey is an ordinary `hotkey` node. A player gives it another key with
`/plugin key network.dereth.admin key Alt+A`, or none with `none`
([ui.md](../../ui.md)).
