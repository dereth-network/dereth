# UI node kinds

**Provenance: design.** Generated from the plugin interface's own code, so it cannot drift from it. Do not edit it: `DERETH_TEST_GOLDEN=1 cargo test -p dereth-testkit --test cpu plugins::the_plugin_reference` rewrites it.

Every kind of node a declared window or HUD is built from, with the properties each takes and the type of value each property holds. [ui.md](../ui.md) is how windows are declared, changed and closed, and the events they raise.

Every node may also have an `id`, a string unique within its window; a node that holds others lists them as its children. A declaration or a property change that breaks a rule is refused with the rule. A window holds at most 2000 nodes, at most 16 deep, and a list at most 10000 rows; one text at most 4096 characters, a window's text at most 64 KiB all told, and a canvas at most 1000 draw commands. `width` and `height` are a node's natural size, in character cells across and lines down (a list's height in rows, a canvas's and a chart's size in its own pixels); `grow`, the least and most sizes and a container's `gap`, `align` and `justify` share a window's room out as [ui.md](../ui.md#layout) says. A HUD holds no `tabs`, `number`, `choice`, `input`, `textbox`, `slider`: its only controls are buttons, toggles and lists.

| kind | holds nodes | properties |
|---|---|---|
| [`window`](#window) | yes | `title`, `width`, `height`, `resizable`, `min_width`, `min_height`, `max_width`, `max_height`, `gap`, `align`, `justify` |
| [`hud`](#hud) | yes | `anchor`, `width`, `gap`, `align`, `justify` |
| [`row`](#row) | yes | `grow`, `min_width`, `max_width`, `min_height`, `max_height`, `gap`, `align`, `justify` |
| [`column`](#column) | yes | `grow`, `min_width`, `max_width`, `min_height`, `max_height`, `gap`, `align`, `justify` |
| [`group`](#group) | yes | `title`, `grow`, `min_width`, `max_width`, `min_height`, `max_height`, `gap`, `align`, `justify` |
| [`separator`](#separator) | no | `grow`, `min_width`, `max_width`, `min_height`, `max_height` |
| [`label`](#label) | no | `text`, `color`, `width`, `tooltip`, `grow`, `min_width`, `max_width`, `min_height`, `max_height` |
| [`button`](#button) | no | `text`, `enabled`, `width`, `tooltip`, `grow`, `min_width`, `max_width`, `min_height`, `max_height` |
| [`toggle`](#toggle) | no | `text`, `value`, `enabled`, `tooltip`, `grow`, `min_width`, `max_width`, `min_height`, `max_height` |
| [`meter`](#meter) | no | `value`, `max`, `style`, `text`, `width`, `tooltip`, `grow`, `min_width`, `max_width`, `min_height`, `max_height` |
| [`list`](#list) | no | `columns`, `rows`, `selected`, `height`, `width`, `tooltip`, `grow`, `min_width`, `max_width`, `min_height`, `max_height` |
| [`image`](#image) | no | `src`, `icon`, `height`, `text`, `width`, `tooltip`, `grow`, `min_width`, `max_width`, `min_height`, `max_height` |
| [`tabs`](#tabs) | yes | `selected`, `grow`, `min_width`, `max_width`, `min_height`, `max_height` |
| [`number`](#number) | no | `text`, `value`, `min`, `max`, `step`, `enabled`, `tooltip`, `grow`, `min_width`, `max_width`, `min_height`, `max_height` |
| [`choice`](#choice) | no | `text`, `options`, `value`, `enabled`, `tooltip`, `grow`, `min_width`, `max_width`, `min_height`, `max_height` |
| [`canvas`](#canvas) | no | `width`, `height`, `background`, `draw`, `grow`, `min_width`, `max_width`, `min_height`, `max_height` |
| [`chart`](#chart) | no | `width`, `height`, `series`, `min`, `max`, `style`, `grow`, `min_width`, `max_width`, `min_height`, `max_height` |
| [`input`](#input) | no | `text`, `value`, `placeholder`, `enabled`, `width`, `tooltip`, `grow`, `min_width`, `max_width`, `min_height`, `max_height` |
| [`textbox`](#textbox) | no | `value`, `editable`, `height`, `width`, `tooltip`, `grow`, `min_width`, `max_width`, `min_height`, `max_height` |
| [`slider`](#slider) | no | `text`, `value`, `min`, `max`, `step`, `enabled`, `width`, `tooltip`, `grow`, `min_width`, `max_width`, `min_height`, `max_height` |
| [`hotkey`](#hotkey) | no | `command`, `text`, `key`, `tooltip`, `grow`, `min_width`, `max_width`, `min_height`, `max_height` |

## window

A window: the root of a declared tree, with a frame, a title and a close box.

| property | takes |
|---|---|
| `title` | a string |
| `width` | a whole number from 0 to 10000 |
| `height` | a whole number from 0 to 10000 |
| `resizable` | a boolean |
| `min_width` | a whole number from 0 to 10000 |
| `min_height` | a whole number from 0 to 10000 |
| `max_width` | a whole number from 0 to 10000 |
| `max_height` | a whole number from 0 to 10000 |
| `gap` | a whole number from 0 to 10000 |
| `align` | one of `stretch`, `start`, `center`, `end` |
| `justify` | one of `start`, `center`, `end`, `between` |

## hud

A small always-on-screen window with no frame, title or close box, placed by an anchor.

| property | takes |
|---|---|
| `anchor` | one of `top_left`, `top`, `top_right`, `left`, `center`, `right`, `bottom_left`, `bottom`, `bottom_right` |
| `width` | a whole number from 0 to 10000 |
| `gap` | a whole number from 0 to 10000 |
| `align` | one of `stretch`, `start`, `center`, `end` |
| `justify` | one of `start`, `center`, `end`, `between` |

## row

Its children side by side.

| property | takes |
|---|---|
| `grow` | a number from 0 to 1000 |
| `min_width` | a whole number from 0 to 10000 |
| `max_width` | a whole number from 0 to 10000 |
| `min_height` | a whole number from 0 to 10000 |
| `max_height` | a whole number from 0 to 10000 |
| `gap` | a whole number from 0 to 10000 |
| `align` | one of `stretch`, `start`, `center`, `end` |
| `justify` | one of `start`, `center`, `end`, `between` |

## column

Its children one above the other.

| property | takes |
|---|---|
| `grow` | a number from 0 to 1000 |
| `min_width` | a whole number from 0 to 10000 |
| `max_width` | a whole number from 0 to 10000 |
| `min_height` | a whole number from 0 to 10000 |
| `max_height` | a whole number from 0 to 10000 |
| `gap` | a whole number from 0 to 10000 |
| `align` | one of `stretch`, `start`, `center`, `end` |
| `justify` | one of `start`, `center`, `end`, `between` |

## group

Its children under a title.

| property | takes |
|---|---|
| `title` | a string |
| `grow` | a number from 0 to 1000 |
| `min_width` | a whole number from 0 to 10000 |
| `max_width` | a whole number from 0 to 10000 |
| `min_height` | a whole number from 0 to 10000 |
| `max_height` | a whole number from 0 to 10000 |
| `gap` | a whole number from 0 to 10000 |
| `align` | one of `stretch`, `start`, `center`, `end` |
| `justify` | one of `start`, `center`, `end`, `between` |

## separator

A dividing line.

| property | takes |
|---|---|
| `grow` | a number from 0 to 1000 |
| `min_width` | a whole number from 0 to 10000 |
| `max_width` | a whole number from 0 to 10000 |
| `min_height` | a whole number from 0 to 10000 |
| `max_height` | a whole number from 0 to 10000 |

## label

A line of text.

| property | takes |
|---|---|
| `text` | a string |
| `color` | a colour, "#RRGGBB" or a number 0xRRGGBB |
| `width` | a whole number from 0 to 10000 |
| `tooltip` | a string |
| `grow` | a number from 0 to 1000 |
| `min_width` | a whole number from 0 to 10000 |
| `max_width` | a whole number from 0 to 10000 |
| `min_height` | a whole number from 0 to 10000 |
| `max_height` | a whole number from 0 to 10000 |

## button

A button; a click is a `ui.clicked` event and a player gesture.

| property | takes |
|---|---|
| `text` | a string |
| `enabled` | a boolean |
| `width` | a whole number from 0 to 10000 |
| `tooltip` | a string |
| `grow` | a number from 0 to 1000 |
| `min_width` | a whole number from 0 to 10000 |
| `max_width` | a whole number from 0 to 10000 |
| `min_height` | a whole number from 0 to 10000 |
| `max_height` | a whole number from 0 to 10000 |

## toggle

A box the player ticks; a change is a `ui.changed` event and a player gesture.

| property | takes |
|---|---|
| `text` | a string |
| `value` | a boolean |
| `enabled` | a boolean |
| `tooltip` | a string |
| `grow` | a number from 0 to 1000 |
| `min_width` | a whole number from 0 to 10000 |
| `max_width` | a whole number from 0 to 10000 |
| `min_height` | a whole number from 0 to 10000 |
| `max_height` | a whole number from 0 to 10000 |

## meter

A bar filled to `value` of `max`.

| property | takes |
|---|---|
| `value` | a number |
| `max` | a number |
| `style` | one of `neutral`, `health`, `stamina`, `mana` |
| `text` | a string |
| `width` | a whole number from 0 to 10000 |
| `tooltip` | a string |
| `grow` | a number from 0 to 1000 |
| `min_width` | a whole number from 0 to 10000 |
| `max_width` | a whole number from 0 to 10000 |
| `min_height` | a whole number from 0 to 10000 |
| `max_height` | a whole number from 0 to 10000 |

## list

A table of rows under column headings; a selection is a `ui.selected` event.

| property | takes |
|---|---|
| `columns` | a list of columns, each a heading or `{ title, width, grow, min_width }` |
| `rows` | a list of rows, each a list of cells: a string, a number, or `{ text, color, icon }` |
| `selected` | a row number from 1, or nil |
| `height` | a whole number from 0 to 10000 |
| `width` | a whole number from 0 to 10000 |
| `tooltip` | a string |
| `grow` | a number from 0 to 1000 |
| `min_width` | a whole number from 0 to 10000 |
| `max_width` | a whole number from 0 to 10000 |
| `min_height` | a whole number from 0 to 10000 |
| `max_height` | a whole number from 0 to 10000 |

## image

A picture: a PNG from the plugin's package or one it fetched, by name.

| property | takes |
|---|---|
| `src` | a string |
| `icon` | a number |
| `height` | a whole number from 0 to 10000 |
| `text` | a string |
| `width` | a whole number from 0 to 10000 |
| `tooltip` | a string |
| `grow` | a number from 0 to 1000 |
| `min_width` | a whole number from 0 to 10000 |
| `max_width` | a whole number from 0 to 10000 |
| `min_height` | a whole number from 0 to 10000 |
| `max_height` | a whole number from 0 to 10000 |

## tabs

Pages under tabs: each child a group, its title on its tab; a tab chosen is a `ui.selected` event (`rows = { page }`) and shows that page.

| property | takes |
|---|---|
| `selected` | a row number from 1, or nil |
| `grow` | a number from 0 to 1000 |
| `min_width` | a whole number from 0 to 10000 |
| `max_width` | a whole number from 0 to 10000 |
| `min_height` | a whole number from 0 to 10000 |
| `max_height` | a whole number from 0 to 10000 |

## number

A number with a step down and a step up, kept between `min` and `max`; a step is a `ui.changed` event with the new value.

| property | takes |
|---|---|
| `text` | a string |
| `value` | a number |
| `min` | a number |
| `max` | a number |
| `step` | a number |
| `enabled` | a boolean |
| `tooltip` | a string |
| `grow` | a number from 0 to 1000 |
| `min_width` | a whole number from 0 to 10000 |
| `max_width` | a whole number from 0 to 10000 |
| `min_height` | a whole number from 0 to 10000 |
| `max_height` | a whole number from 0 to 10000 |

## choice

One of `options`, picked by the player from the list of them; a pick is a `ui.changed` event with the new value.

| property | takes |
|---|---|
| `text` | a string |
| `options` | a list of strings |
| `value` | a string |
| `enabled` | a boolean |
| `tooltip` | a string |
| `grow` | a number from 0 to 1000 |
| `min_width` | a whole number from 0 to 10000 |
| `max_width` | a whole number from 0 to 10000 |
| `min_height` | a whole number from 0 to 10000 |
| `max_height` | a whole number from 0 to 10000 |

## canvas

A free drawing `width` by `height` pixels: its `draw` commands (`line`, `rect`, `circle`, `text`) in its own pixels from its top left, over its `background`.

| property | takes |
|---|---|
| `width` | a whole number from 0 to 10000 |
| `height` | a whole number from 0 to 10000 |
| `background` | a colour, "#RRGGBB" or a number 0xRRGGBB |
| `draw` | a list of draw commands (`line`, `rect`, `circle`, `text`) |
| `grow` | a number from 0 to 1000 |
| `min_width` | a whole number from 0 to 10000 |
| `max_width` | a whole number from 0 to 10000 |
| `min_height` | a whole number from 0 to 10000 |
| `max_height` | a whole number from 0 to 10000 |

## chart

Series of numbers drawn as lines (or bars, `style = "bar"`) `width` by `height` pixels, between `min` and `max` (the values' own when left out), each series' label along the top.

| property | takes |
|---|---|
| `width` | a whole number from 0 to 10000 |
| `height` | a whole number from 0 to 10000 |
| `series` | a list of series, each `{ values, color, label }` |
| `min` | a number |
| `max` | a number |
| `style` | one of `line`, `bar` |
| `grow` | a number from 0 to 1000 |
| `min_width` | a whole number from 0 to 10000 |
| `max_width` | a whole number from 0 to 10000 |
| `min_height` | a whole number from 0 to 10000 |
| `max_height` | a whole number from 0 to 10000 |

## input

One line of text the player types; each change is a `ui.changed` event with the text, and a player gesture.

| property | takes |
|---|---|
| `text` | a string |
| `value` | a string |
| `placeholder` | a string |
| `enabled` | a boolean |
| `width` | a whole number from 0 to 10000 |
| `tooltip` | a string |
| `grow` | a number from 0 to 1000 |
| `min_width` | a whole number from 0 to 10000 |
| `max_width` | a whole number from 0 to 10000 |
| `min_height` | a whole number from 0 to 10000 |
| `max_height` | a whole number from 0 to 10000 |

## textbox

Lines of text, wrapped and scrolled; with `editable`, the player types in it and each change is a `ui.changed` event.

| property | takes |
|---|---|
| `value` | a string |
| `editable` | a boolean |
| `height` | a whole number from 0 to 10000 |
| `width` | a whole number from 0 to 10000 |
| `tooltip` | a string |
| `grow` | a number from 0 to 1000 |
| `min_width` | a whole number from 0 to 10000 |
| `max_width` | a whole number from 0 to 10000 |
| `min_height` | a whole number from 0 to 10000 |
| `max_height` | a whole number from 0 to 10000 |

## slider

A number the player slides between `min` and `max` in `step`s; a move is a `ui.changed` event with the new value.

| property | takes |
|---|---|
| `text` | a string |
| `value` | a number |
| `min` | a number |
| `max` | a number |
| `step` | a number |
| `enabled` | a boolean |
| `width` | a whole number from 0 to 10000 |
| `tooltip` | a string |
| `grow` | a number from 0 to 1000 |
| `min_width` | a whole number from 0 to 10000 |
| `max_width` | a whole number from 0 to 10000 |
| `min_height` | a whole number from 0 to 10000 |
| `max_height` | a whole number from 0 to 10000 |

## hotkey

A key the host binds to one of the plugin's commands (`command`, a line as typed after the `/`): pressing it runs the command as if typed, a player gesture. `key` is the key asked for, with Ctrl or Alt; the host owns the actual key.

| property | takes |
|---|---|
| `command` | a string |
| `text` | a string |
| `key` | a key with Ctrl or Alt, as "Ctrl+Shift+F5" or "Alt+K" |
| `tooltip` | a string |
| `grow` | a number from 0 to 1000 |
| `min_width` | a whole number from 0 to 10000 |
| `max_width` | a whole number from 0 to 10000 |
| `min_height` | a whole number from 0 to 10000 |
| `max_height` | a whole number from 0 to 10000 |
