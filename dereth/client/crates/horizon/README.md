# The Horizon interface

Horizon is the Dereth client's third interface, beside the modern and the classic one. It has its
own character select, character creation, HUD and windows, drawn from its own art. Underneath it
is the same game: the same runtime, network session, object model, physics, world renderer and key
handling. Only the interface differs.

A new player starts in Horizon: the launcher asks which interface to use before the client first
starts, with Horizon chosen. Switch to and from it at any time in Options (Interface). Unlike the
other two, it draws the screens before the world at the window's own size.

## Playing with it

- **Character select.** Click a character to select it, then click it again or press Enter to log
  in. Delete (the button or the key) asks before deleting, and restores a character being deleted.
  A character the client has seen in the world stands in the middle as it last looked.
- **Character creation.** Drag the model to turn it.
- **The camera** circles the character. The wheel zooms, and holding either mouse button turns it
  with the pointer (the directions can be reversed in Settings). Hold V to look at the character's
  front.
- **Movement.** Camera-based, the default: W runs where the camera looks, S turns the character
  round and runs toward the camera, and A, D, Q and E step sideways. Character-based, chosen in
  Settings: W and S move forward and back, A and D turn, Q and E step sideways. Space jumps, R
  runs on, Shift+R switches walking and running, and X stops.
- **Keys.** Horizon keeps its own key map, so its keys never change the other interfaces'. C
  character, I or B inventory, M map, J or L journal, K settings, `;` allegiance, O friends, P
  spellbook. Tab and Shift-Tab pick the next and previous creature. F uses the selection, Shift+F
  picks it up and G examines it. `/` opens the chat line with a `/` in it, and F12 takes a
  screenshot.
- **Hotbar 1** is the shortcut bar. Click a shortcut to use it, drag it to another slot to move it,
  or drag it off the bar to remove it. Drag an item from the inventory onto a slot to make a
  shortcut.
- **Hotbar 2** is the current tab of the spell bar. Click a spell to cast it.
- **The log window.** Enter opens the input line. Click its label, or press Tab, to choose who you
  talk to. Up and Down recall sent lines, and a line starting with `/` or `@` is a command. The tabs
  are the game's chat windows; right-click a tab to pop it out or set its filter.
- **HUD Layout**, in the System menu: drag an element to move it and use the wheel to resize it.
  Save keeps the layout for this character.
- **The stance gauge** shows the combat mode. Click the peace crest, or the weapon's crest, to
  change stance.

## With a pad

On Windows, Horizon can be played entirely with a pad. Press any button to switch gamepad mode on;
press a key to switch it off. The mouse still works beside it.

| In the world | |
|---|---|
| Left stick, right stick | move, and turn the camera |
| A | use the selection; with none, select the nearest usable thing ahead |
| B | let go of the selection and stop |
| X, Y | the map; jump (held to charge) |
| LT or RT held | the left or right half of the cross hotbars, used with the d-pad and face buttons; RB held picks the set |
| D-pad up and down, left and right | the fellowship's members; cycle what can be selected |
| LB | tap for autorun; hold to zoom with the right stick |
| Left stick or right stick pressed | peace or combat; run or walk |
| Start, Back | the menu; the next open window |

In a window, the d-pad moves the focus, A clicks, B goes back, X opens an item's options, Y uses or
equips, and LB and RB switch packs or tabs. On a text line an on-screen keyboard comes up.

## Settings

The interface scale (the Settings window's Client page) and the camera's settings (its Controls
page) are kept in `horizon.txt` beside the preferences. Its key map is `dereth-horizon.keymap`
there, and how each character last looked is in `horizon-looks.txt`.

The interface is drawn at 100%, 150%, 200% or 300%. A step is offered only when the window is big
enough for every screen at that step: 150% from 2160 by 1296 pixels, 200% from 2880 by 1728, 300%
from 4320 by 2592. The system's display scaling is already counted in the window's pixels. Auto,
the default, picks the largest step the window offers, up to 200%: 100% at 1080p, 150% at 1440p
and 200% on a 4K monitor. A chosen step the window doesn't offer is kept, and Auto's is used until
the window offers it again.

## Its art

Horizon draws its own art: pieces packed into atlases at several scales, with a manifest that
names them (`pieces/`), and its fonts as glyph pages. The game's own icons (items, spells,
effects) come from the game's data files. Everything is drawn on whole pixels, and text at its
font's own size. Where a piece is missing, a plain shape is drawn in its place.

The desktop client has the art built in. The web client fetches it from `pkg/horizon/` beside the
module when the page opens. Until it arrives, a choice of Horizon waits (a saved one starts in the
modern interface) and the chat says the art is loading. If it can't be fetched, choosing Horizon is
refused and the chat says why.

## How the game's concepts are drawn

| In the game | In Horizon |
|---|---|
| The server | The world named at the top of character select |
| Character select | The first screen, over a view of Holtburg: the world's news on the left, the character cards on the right, and the selected character between them as it last stood in the world |
| Health, mana, stamina | The parameter bar's three gauges |
| The shortcut bar, the spell bar | Hotbars 1 and 2 |
| Combat mode, power | The stance gauge beside the parameter bar |
| The selection | The target bar, and the target marker over it |
| The people and creatures nearby | Nameplates: people green, creatures that can be fought yellow, players blue |
| Enchantments, vitae | The status row, and the weakness status |
| The radar | The minimap |
| Contracts | The duty list |
| Experience, unassigned experience | The EXP bar, with unassigned experience as the rested segment |
| Pyreals | The gil counter |
| The fellowship | The party list |
| Paper doll | The character window: the figure between the equipment slots |
| Packs and foci | The inventory's pack column, and the open pack's grid |
| The spellbook | The Spellbook window |
| Allegiance | The Allegiance window |

## The code

- `src/runtime.rs`: the front end the client shell drives, over the shared runtime.
- `src/ui/`: the interface. The pre-game screens are in `pregame.rs` and `creation.rs`, the HUD in
  `hud.rs` and `hud/`, its layout in `layout.rs`, the windows in `panels.rs` and `panels/`, and the
  log window in `chat.rs`. The widgets are in `kit.rs`, the drawing in `paint.rs`, the pad's focus
  in `nav.rs`, and the game state it reads in `game.rs`.
- `src/state.rs`: the game's state, read from the runtime once a frame.
- `src/dialogs.rs`: the game's questions, shown in Horizon's boxes.
- `src/pad.rs`: gamepad mode.
- `src/options.rs`, `src/scale.rs`, `src/looks.rs`: its settings, the interface scale, and the
  remembered looks.
- `src/art.rs`, `src/pieces.rs` and `pieces/`, `src/font.rs`, `src/resample.rs`: the art, the fonts
  and pictures drawn at sizes their atlases lack.
- `src/draw.rs`: the quads, turned into the shared overlay's batches.
