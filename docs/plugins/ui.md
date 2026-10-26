# Declared windows

**Provenance: design.** The plugin interface is this client's own addition, not the original's; every statement here is what a host must do, and the conformance suite pins it.

A plugin says **what** its window is, never where its pixels go. A window is a tree of typed
nodes; each host draws it its own way: Dereth's desktop and browser clients in the look of the
interface the player chose (see [what each interface draws](README.md#host-capabilities)), a
controller interface as a focus-driven window the pad moves through, a host with no display not at
all (it keeps the tree, which a test can read and click).

```lua
local win = ui.window{ id = "main", title = "Looter", width = 30,
  ui.toggle{ id = "auto", text = "Loot corpses", value = false },
  ui.button{ id = "now", text = "Loot open container" },
  ui.list{ id = "log", columns = { "Item", "Rule" }, rows = {} },
}
win:get("log").rows = { { "Pyreal", "value" } }   -- or ui.get("main.log").rows = …
on("ui.clicked", function(e) if e.id == "now" then ... end end)
```

`ui.window{...}` declares (or replaces) the window with that `id` and answers a handle:
`win:get(id)` is a node's handle, `win:close()` hides the window (keeping its tree) and
`win:show()` shows it again, while the plugin answers the player ([opening a
window](#opening-a-window)). Setting a property on a node handle changes it in the window; reading
one reads it back. `ui.get("window.node")` is the same handle by path. The desktop and browser
clients show a window's tree afresh at most 30 times a second: a plugin changing it faster has
its changes shown together, the latest each time (a window opened, closed or sized shows at once).

## HUDs

`ui.hud{ id = …, anchor = …, nodes… }` declares a **HUD**: a small window that stays on the
screen, with no frame, title or close box, placed by `anchor` (`top_left`, `top`, `top_right`,
`left`, `center`, `right`, `bottom_left`, `bottom`, `bottom_right`; `top_left` when absent). Its
only controls are buttons, toggles and lists: a HUD holding an `input`, a `textbox`, a `number`, a
`choice`, a `slider` or `tabs` is refused. Only its buttons, toggles and list rows take the mouse,
and only where nothing of the game's stands: a click on one of the game's own panels (its chat,
toolbar, radar or any other) goes to the game even with a HUD drawn over it, and a click anywhere
else off the HUD's controls goes through to the game. It answers the same handle as a window, and
needs the host capability `ui.hud`.

```lua
local hud = ui.hud{ id = "xp", anchor = "top_right", ui.label{ id = "rate", text = "0 / hour" } }
hud:get("rate").text = "1.2M / hour"
```

**Unlocking the HUDs.** The player moves any plugin's HUD by unlocking the HUDs: the plugin
panel's *Unlock HUDs* toggle, or `/plugin huds unlock` (`/plugin huds lock` locks them again,
`/plugin huds` alone toggles). While they are unlocked, every plugin's HUD is outlined and a press
anywhere on one takes hold of it: it follows the pointer until let go, and its controls take no
input, so its plugin hears nothing of the press. The place the player puts it is kept for the
character, as a window's is, and the HUD stands there from then on, its anchor aside; it is kept
wholly on the screen as it is dragged, as it opens and whenever the screen or the interface's
scale changes. Locked again, the HUD stays where it was put, takes no press but on its controls,
and they answer as before. This holds under every interface the desktop and browser clients
have: under Horizon the place is the HUD's in Horizon's own HUD layout, kept with it for the
character the moment the player lets go. A host that draws no HUDs keeps the rule for their
controls.

## Nodes

A node is `ui.<kind>{ properties, children... }`: named keys are properties, the list part is the
children. An unknown kind, an unknown property or a value of the wrong type is refused with an
error; ids are unique within a window; a window or HUD cannot hold a window or a HUD. A window holds at most 2,000
nodes, 16 deep; a list at most 10,000 rows; one text (a property, an entry of a list, a cell, a
drawing's words) at most 4,096 characters, and a window's text at most 64 KiB all told; a canvas
at most 1,000 draw commands. A declaration or a change past one of these is refused, the window as
it was.

| kind | properties | holds nodes |
|---|---|---|
| `window` | `title`, `width`, `height`, `resizable` (boolean), `min_width`, `min_height`, `max_width`, `max_height`, `gap`, `align`, `justify` | yes |
| `hud` | `anchor`, `width`, `gap`, `align`, `justify` | yes |
| `row`, `column` | `gap`, `align`, `justify` | yes |
| `group` | `title`, `gap`, `align`, `justify` | yes |
| `separator` | none | no |
| `label` | `text`, `color` (a colour), `width` | no |
| `button` | `text`, `enabled`, `width` | no |
| `toggle` | `text`, `value` (boolean), `enabled` | no |
| `meter` | `value`, `max`, `style` (`neutral`, `health`, `stamina`, `mana`), `text`, `width` | no |
| `list` | `columns` (each a heading, or `{ title, width, grow, min_width }`), `rows` (lists of cells: a string, a number, or `{ text, color, icon }`), `selected` (a row from 1, or nil), `height`, `width` | no |
| `image` | `src`, `icon` (an icon from the game's data, by id), `text`, `width`, `height` | no |
| `tabs` | `selected` (a page from 1) | only `group`s: each a page, its `title` on its tab |
| `number` | `text`, `value`, `min`, `max`, `step` (numbers), `enabled` | no |
| `choice` | `text`, `options` (strings), `value` (one of them), `enabled` | no |
| `canvas` | `width`, `height` (pixels), `background` (a colour), `draw` (draw commands, below) | no |
| `input` | `text` (its label), `value` (the text), `placeholder`, `enabled`, `width` | no |
| `textbox` | `value` (the text, lines split at `\n`), `editable` (boolean), `height` (lines), `width` | no |
| `slider` | `text`, `value`, `min`, `max`, `step` (numbers), `enabled`, `width` | no |
| `hotkey` | `command` (a command line, as typed after the `/`), `text`, `key` (`Ctrl+…` or `Alt+…`) | no |
| `chart` | `width`, `height` (pixels), `series` (each `{ values, color, label }`), `min`, `max`, `style` (`line` or `bar`) | no |

Every node but a window and a HUD also takes `grow` (its share of the room left over, a number from
0), `min_width` and `max_width` (cells) and `min_height` and `max_height` (lines): how it is laid out
([Layout](#layout), below).

A `tabs` node shows a tab for each of its pages and the chosen page's nodes below them; choosing a
tab is a `ui.selected` event (`rows = { page }`) and shows that page. A `number` shows its value
between a step down and a step up, kept between `min` and `max`; a `choice` shows its value on a
box that opens the list of its options, and the option the player picks there is its new value.
Either change is a `ui.changed` event with the new value, already set on the node. A `list` with
more rows than it shows scrolls (the wheel, its scrollbar, or the pad's focus moving through its
rows); scrolled to its end, or with every row in sight, it keeps its last row in sight as rows are
added at its end, so a log shows its newest line.

An `image` shows a PNG picture: `src` names a file in the plugin's package (`"maps/town.png"`) or
a picture the plugin fetched (the name `net.fetch{ …, as = "image" }` answered, see
[peers.md](peers.md#https-net)). It is scaled to the node's size; `text` is a caption, and is shown
in its place while there is no picture. The desktop and browser clients draw it as a picture of
its own. A picture is looked at once, as it arrives (with its package, or fetched): one over 1,024
pixels a side, or one that would take the plugin's pictures over 32 MiB decoded, is not kept (the
plugin's console says why, and a fetch of it fails), and the image shows its caption. Pictures are
made one a frame, an image showing its caption until its turn.

An `input` is one line the player types in, a `textbox` lines of text (typed in when it is
`editable`): each change is a `ui.changed` event with the whole text, a player gesture, and stays on
the node. A host that draws them does not build the window again under the player's caret while
they type. A `slider` is a number dragged between `min` and `max` in steps of `step`: each step
it lands on is a `ui.changed` event with the new value. A list cell may be `{ text, color, icon }`:
the row is drawn in the first colour one of its cells names. Every control may have a `tooltip`, a
line the host shows when the pointer rests on it.

An `image` with an `icon` shows that icon from the game's data (`obj.icon` of an object, a spell's
icon) instead of a picture, where the host has the game's data (`ui.icons`); a list cell's `icon`
is drawn at its row's start.

**The game's own panels** (`retail_ui`, a host that draws the game's interface): a plugin hides,
shows or moves the game's panels by name: `retail_ui.hide("radar")`,
`retail_ui.show(p)`, `retail_ui.move("chat", x, y)` (screen pixels, the panel's top left),
`retail_ui.set(p, { hidden, x, y })`, and `retail_ui.reset(p)` gives the panel back as the player
had it. The panels are `chat`, `vitals`, `side_vitals`, `toolbar`, `radar`, `panels`,
`environment`, `combat`, `power_bar` and `examination`. The last plugin to set a panel has it; a
plugin's panels are given back when it unloads.

Each interface of the desktop and browser clients applies them to what it has that does what the
panel does, and what it cannot do it leaves alone (the call still succeeds):

| panel | modern | classic (hidden only: its panels are docked) | Horizon |
|---|---|---|---|
| `chat` | the chat window | the HUD's chat | the log window |
| `vitals` | the stacked vitals | the HUD's vitals strip | the parameter bar |
| `side_vitals` | the side-by-side vitals | none | none |
| `toolbar` | the toolbar | the HUD's toolbar | the hotbars |
| `radar` | the radar | the HUD's radar | the minimap |
| `panels` | the panel stack | the side pages | the character, inventory, spellbook, social, allegiance, map, settings and journal windows |
| `environment` | the environment panel | the windows docked for a vendor, a container, a trade, salvage and maintenance | the loot, shop, trade, salvage and housing windows |
| `combat` | the combat panel | the combat and spell bars | the stance gauge |
| `power_bar` | the power bar | none (part of the combat bar) | none (part of the stance gauge) |
| `examination` | the examination panel | the examination page | the examine window |

Under Horizon a panel is hidden or moved as an overlay: the player's own layout and window places
are kept, and come back when the plugin lets go. `show` under the classic interface and Horizon is
no more than not hiding.

**Horizon's own places for a plugin.** A plugin's HUD is one of the elements of Horizon's HUD
layout editor, moved, scaled and kept like them (by plugin and window id); unlocked, it is moved by
a drag without the editor, as above. A plugin's commands
(`commands.add`) are offered on the Spellbook's *Plugins* page, to be put on a cross hotbar slot;
the slot runs the command as the player typing it would, a gesture.

A `hotkey` declares a key for one of the plugin's commands. The plugin never sees a key: it names
the command and the key it would like, and the host binds it. Pressing it runs the command as if
the player had typed it (the plugin's command handler hears its words), and it is a player
gesture, as a click or a typed command is. The key must hold Ctrl or Alt (`Ctrl+Shift+F5`,
`Alt+K`; the keys are `F1` to `F24`, `A` to `Z` and `0` to `9`), so a key the game uses alone stays
the game's; a hotkey asking for anything else is refused. A hotkey is live while its plugin is
active, in an open window or a closed one, and shows in its window as its text and its key. The
desktop and browser clients bind the key the hotkey asks for under every interface (the first
plugin to ask for a key has it), and leave a key alone while the player types in a text box and
while the host's dialog for a file is up; the headless client's script
presses one with `ui hotkey <plugin> <node>`. A hotkey whose command the plugin has not claimed
is told to the player.

`ui.toast(text [, seconds])` shows a line briefly at the top of the screen with the plugin's name:
1 to 30 seconds (5 when left out), three at once a plugin (a fourth takes the place of the oldest).
The host shows the toasts as its own HUD, `dereth.plugins/toasts`; a host that draws no plugin
windows shows each toast as a line in the chat instead, once.

A `canvas` is a free drawing, in its own pixels from its top left: `draw` is a list of commands,
`{ op = "line", x1, y1, x2, y2, color, width }`, `{ op = "rect", x, y, w, h, color, fill }`,
`{ op = "circle", x, y, r, color, fill }` and `{ op = "text", x, y, text, color }` (colours
`"#RRGGBB"` or `0xRRGGBB`, white when left out; at most 2,000 commands). Setting `draw` again
redraws it: a map, a radar of one's own, a graph. A `chart` draws each series as a line through
its values (or as bars side by side), between `min` and `max` (the values' own when left out),
with each series' label along its top. Both are drawn in the interface's text; a host with no
display keeps them in the tree.

`width` and `height` are a node's natural size: in character cells across and lines down (a
list's height in rows, a canvas's and a chart's size in its own pixels). The host lays the window
out by the rules of [Layout](#layout) and remembers where the player put it. Every node may have
an `id`; a node the plugin never touches does not need one.

## Layout

Every host lays a window out by these rules, so that a window fills the room it is given: a log
takes the rest of its window, a list's line column runs to the list's right edge, and a window too
short for its nodes scrolls. The desktop and browser clients lay out in their own fonts; a host
with no display lays out with the reference measures below, and the conformance suite checks what
it computes.

**Measures.** A host lays out in layout units (a screen pixel at scale 1) from its font: a
**cell** (a character's width: a tenth of `0000000000`'s), a **line** (the text's line height), a
**control** (a one-line control's height: a line and 6, rounded, and at least a check box's side
and 4), a **gap** of 3, a group's **padding** of 6, a list's **rim** of 2, a **row** of a list (a
line and 2), and its check box's side and its scrollbar's width. The reference measures are a cell
of 7, a line of 16, a check box of 11 and a scrollbar of 8 (so a control is 22 and a list's row
18), and every character is a cell wide.

**What a node asks for.** Each node has, across and down, a **least** size it is never made
smaller than, a **natural** size it asks for, and a **most** size (none unless it says). `width`
(cells) and `height` (lines) set the natural size; `min_width`, `max_width` (cells) and
`min_height`, `max_height` (lines) bound all three. A labelled control's label is its text and a
cell (nothing when it has no text).

| node | natural width | natural height | least |
|---|---|---|---|
| `label` | its text and half a cell | a line | its natural size |
| `hotkey` | its text, its key and a cell | a line | its natural size |
| `button` | its label, in the face the host draws a button's label in, and at each end half a control and a gap; at least 3 cells | a control | its natural size |
| `toggle` | its check box, two gaps, its text and half a cell | a control | its natural size |
| `meter` | 20 cells | 0.9 of a line, at least 12 | its natural size |
| `number` | its label and 14 cells | a control | its natural size |
| `choice` | its label and its box: the longest of its options and its value (at most 40 cells), two cells and a control; at least 8 cells | a control | its natural size |
| `input` | its label and 20 cells | a control | its natural size |
| `slider` | its label, 20 cells for its track and 7 for its value | a control | its natural size |
| `textbox` | 30 cells | its lines (4) and 8 | 10 cells; a line and 8 |
| `list` | its columns (below), a cell between each two, a line and a gap where a row has an icon, its rim and a gap at each side, and a scrollbar and a gap when it has more rows than it shows; at least 8 cells | a line and 2 for its headings (when it has any), its rows (its `height`, else as many as it has, 1 to 8), and its rim above and below | its columns at their least; three rows, or fewer where it asks for fewer |
| `image` | 24 cells | 12 lines | its natural size |
| `canvas`, `chart` | its own pixels | its own pixels | its natural size |
| `separator` | two gaps and 1 | two gaps and 1 | its natural size |

**Columns and rows.** A `column` (and a window, a HUD, a group, each tab's page) sets its nodes one
above the other, `gap` apart (a gap when it says none); a `row` sets them side by side, `gap` apart
(two gaps when it says none). Along its direction a container asks for its nodes' sizes added up,
with the gaps; across, for the largest of them. A group adds its padding round its nodes and a line
over them for its title, and is at least as wide as its title and a cell. `tabs` ask for a control
and a gap for the row of tabs (each tab its title and 34, at least 70), and for the widest and the
tallest of their pages, so a window keeps its size from page to page; their least width is the
chosen tab's and 28 for each other tab (narrower, the tabs share their row), and their least height
the row of tabs and the smaller of the chosen page's least height and three lines. A page shorter
than its nodes' least height **scrolls** under its row of tabs, which stays where it is: its nodes
are laid out at their least height and as wide as the page less its scrollbar's room (the
scrollbar and a gap), and the wheel, its scrollbar and the pad's focus move through them. So a
window with tabs that is too short scrolls its page, not its tabs.

**Sharing the room.** Laid out in a size, a container gives each node its natural size along its
direction. The room left over goes to the nodes that grow, in proportion to their `grow`, none past
its most size (what one cannot take goes to the others); where none grows, it is left at the end
(`justify = "start"`, the default), or put before the nodes (`"end"`), half before and half after
(`"center"`), or between them (`"between"`). Room short is taken from the nodes above their least
size, each in proportion to how far above it it is, none below its least. Each node's edges are
whole units, rounded.

**Across.** In a column every node takes the column's whole width, within its least and most,
but an image, a canvas and a chart, which keep their own size and stand at the column's start. In a
row a list, a text box, a separator and a container take the row's whole height; every other node
keeps its own height and stands in the row's middle. A container's `align` (`"stretch"`, the
default, or `"start"`, `"center"`, `"end"`) otherwise keeps each of its nodes at its natural size
across, at its start, middle or end.

**What grows.** A container grows as much as the most growing node it holds, so a growing list
deep in a window takes the window's room. A row grows down its column only by the nodes that take
its height (a list, a text box, a container): a field growing across a row fills the row's width
and leaves the row one control high. Where no node of a window says `grow`, its last list or text
box grows by 1; on each tab's page the same, page by page.

**A list's columns.** A column is a heading (`"Line"`) or `{ title, width, grow, min_width }`. A
column of a `width` is that many cells. Any other asks for its widest heading or cell (of its first
500 rows), at most 40 cells, and is at least its `min_width` (cells), else its heading and at least
3 cells. The list's width less its rim and gaps, its icons' room, its scrollbar's room and the cells
between its columns is shared among its columns as a row shares its room: where no column says
`grow`, the last column of no `width` grows by 1. A cell's text is cut with an ellipsis to its
column.

**Labels in line.** The labels of the `number`, `choice`, `input` and `slider` nodes with text that
a column holds are as wide as the widest of them, so their boxes start in one place.

**The window.** A window asks for its nodes' natural size, and at least its own `width` and
`height`.
Its host keeps it at least as wide as its nodes' least width and within the bounds it declares and
the screen; a body narrower than its nodes' least width has them laid out at it. A body shorter
than its nodes' least height **scrolls**: its nodes are laid out at their least height and as wide
as the body less its scrollbar's room (the scrollbar and a gap), and the wheel, its scrollbar and
the pad's focus move through them. A HUD is laid out at its natural size.

## Resizable windows

The player sizes a window by dragging its left, right or bottom edge, either bottom corner, or the
grip in its bottom right corner (its top edge is the title bar, which moves it). The grip is part of
the window's frame and takes nothing from its body: in the frame's corner, inside its border, the
interface's own mark, or none where the frame's corner is a handle already (Horizon's). The
frame keeps as much room round the body below it as at its sides. The window stays
within `min_width`/`max_width` (character cells) and `min_height`/`max_height` (lines) when it
declares them, and within the screen. What the new size does to the window's nodes depends on how
the window is declared:

| `resizable` | what the player's size does |
|---|---|
| `true` | The nodes are laid out again at the new size ([Layout](#layout)). On letting go, the plugin hears `ui.resized` with the size in cells and lines, and the window keeps that size when the plugin declares it again. |
| left out | The nodes are laid out again at the new size, as for `true`; the host keeps the size, and the plugin hears nothing. |
| `false` | The window keeps the size its nodes ask for, and has no grip. |

Either way the window is never narrower than its nodes' least width or its title's room, and a
window shorter than its nodes scrolls.

The host remembers each window's size for the character, with where the window was put, and opens
the window there again. A window is kept on the screen as it opens and whenever the screen or the
interface's scale changes, so at least its title bar can always be reached.

## Opening a window

A window opens only when the player asks. Every window and HUD starts closed: one a plugin declares
is open only if it was open already, or if the plugin declares it while answering the player (a
press on one of its windows, one of its commands typed, bound to a key or put on a slot, the
player opening its main window from the plugin bar). `win:show()` opens a window only while the
plugin answers the player; shown of the plugin's own accord (as it loads, on a timer, on a game
event), it stays closed. The player opens a plugin's main window from the plugin bar, and a
plugin opens the rest, and its HUDs, as it answers them. A window the player had open opens again
when its plugin is reloaded and declares it again; at the next start, every window is closed.

## The plugin bar

Outside the windows, the host shows a **plugin bar**: a button for each plugin active in the
session's part that has a window or a HUD, showing its name, what it says it is doing, and whether
its main window is open. The main window is the one with id `main`, or else the first window it
declared, or else its first HUD. A click on a button opens or closes that window, and the plugin
hears `ui.opened` or `ui.closed`. The
player moves the bar by its handle at its left end and widens or narrows it by its grip at its
right end; the host remembers where and how wide for the character. The bar is never wider than the
screen: where its buttons do not all fit, the wheel over it and its two steps scroll them along.

A window the player opened takes the pad's focus as it opens: one opened from the plugin bar or
the pad's menu, or one its plugin opens while answering the player (a press on one of its windows,
one of its commands typed, bound to a key or put on a slot). A window a plugin opens of its own
accord (as it loads, on a timer, on a game event) never takes it.

`ui.status(state)` sets what the bar shows for the plugin: `running` (the default), `paused` or
`idle`; any other value is an error. A plugin the host stopped on a fault shows `error`, whatever
it last said.

## Events

| event | when |
|---|---|
| `ui.clicked` `{ window, id }` | a button was clicked |
| `ui.changed` `{ window, id, value }` | a toggle was flipped, a number stepped, a choice picked or a slider moved; its `value` is already the new one |
| `ui.selected` `{ window, id, rows }` | rows of a list were selected, or a tab chosen |
| `ui.closed` `{ window }` | the player closed the window, from its close box or the plugin bar |
| `ui.opened` `{ window }` | the player opened the window from the plugin bar |
| `ui.resized` `{ window, width, height }` | the player resized a resizable window (cells and lines) |

A click is a player gesture (see [permissions.md](permissions.md)).

## The tree as text

A host with no display shows a window as text, one node a line, indented by depth: the kind, `#id`
when there is one, then the properties in name order as JSON:

```text
window #main title="Looter" width=30
  toggle #auto text="Loot corpses" value=false
  button #now text="Loot open container"
  list #log columns=["Item","Rule"] rows=[["Pyreal","value"]]
```

The conformance suite compares windows in this form.

A host lays a window out as text the same way: each node's kind and `#id`, then its place in its
parent (its top left, from the parent's; a group's nodes from the group's own top left) and its
size, `x,y wxh`; a labelled control's label width (`label=`); a list's column widths and the rows
it shows (`columns=` and `rows=`); and for a body or a tab's page that scrolls, the height it
scrolls through (`scrolls=`). The window's own line has its body's size. The headless client
prints each open window so (`ui layout`), at the reference measures, at the size its plugin keeps
for it or else its natural size:

```text
window #main 280x320
  label #head 0,0 280x16
  list #a 0,19 280x104 columns=270 rows=4
  list #b 0,126 280x194 columns=270 rows=9
```

The conformance suite's `layout` expectation compares windows in this form.
