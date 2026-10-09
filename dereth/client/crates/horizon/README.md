# The Horizon interface

The client's third interface, beside the modern and the classic one: character select and
character creation, the in-game HUD and its windows, drawn from its own art.
It is chosen in Options (Interface: Horizon) and switched to and from live, as the classic interface
is. Unlike the other two, it draws the screens before the world at the player's own window size.

The game underneath is the client's: the same runtime, network session, object model, physics,
world renderer and key maps. Only the interface differs.

## Its art

The interface draws its own art: pieces packed into atlases at several scales with a manifest that
names them (`pieces/`), and its fonts as glyph pages with their table. The desktop client carries
these files built in. The browser client's module does not: they are served beside it
(`pkg/horizon/`), and the page fetches them from when it opens, without the client waiting for
them. Until they are in, a choice of Horizon waits (a saved one starts in the modern interface)
and the chat says the art is loading; the frame they are in, Horizon is shown. When they could not
be fetched, choosing Horizon is refused and the chat says why. The modern and classic interfaces
are not affected either way. Where a piece is missing it
draws plainly in its place. The game's own icons (items, spells, effects) are read from the game's
data.

## Its settings

The interface scale (100%, 150%, 200% or 300%, on the Settings window's Client page) and the
camera's settings (its Controls page) are kept in `horizon.txt` beside the preferences.

## How the game's concepts are drawn

| In the game | Drawn as |
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

## Playing with it

- **Character select.** Click a character to select it and click it again, or press Enter, to log
  in. Delete (or the Delete key) asks before deleting, and restores a character being deleted.
  A character the client has seen in the world stands in the middle as it last looked; the looks
  are kept in `horizon-looks.txt` beside the preferences.
- **Character creation.** Drag the model to turn it.
- **The camera** is this interface's own: it circles the character, the wheel brings it closer
  or further, and either mouse button held turns it with the pointer (the directions can be
  reversed in Settings). Hold V to look at the character's front.
- **Movement.** Camera-based (the default): W runs where the camera looks, S turns the character
  round and runs toward the camera, and A, D, Q and E step sideways. Character-based, chosen in
  Settings: W and S move forward and back, A and D turn, Q and E step sideways. Space
  jumps and R runs on.
- **Keys.** This interface keeps its own key map, `dereth-horizon.keymap` beside the preferences, so
  its keys never change the other interfaces': C character, I inventory, M map, J journal,
  K settings, `;` allegiance, O friends, P spellbook, Tab and Shift-Tab the next and the previous
  creature, and `/` opens the chat line with a `/` in it.
- **Hotbar 1** is the shortcut bar. Click a shortcut to use it, drag it to another slot to move
  it, or drag it off the bar to remove it. Drag an item from the inventory onto a slot to make a
  shortcut.
- **Hotbar 2** is the current tab of the spell bar. Click a spell to cast it.
- **The log window.** Enter opens the input line, which speaks to the game's own talk-focus
  destination: click its label to choose one, or Tab to step through them. Up and Down recall
  sent lines, and a line that starts with `/` or `@` is a command. The tabs are the game's chat
  windows; a tab's right-click menu pops it out into a window of its own and sets its filter.
- **HUD Layout** is in the System menu. Drag an element to move it, and use the wheel to resize
  it. Save keeps the layout for this character, beside the preferences.
- **The stance gauge** shows the combat mode. Click the peace crest, or the crest of the weapon's
  mode, to change stance.

## Its shape

- `src/runtime.rs`: the front end the client shell drives, over the shared runtime.
- `src/ui/`: the interface. The pre-game screens are in `pregame.rs`, the HUD in `hud.rs`, its
  layout in `layout.rs` and the windows in `panels.rs` and `panels/`. The widgets are in
  `kit.rs`, the drawing in `paint.rs` and the game state it reads in `game.rs`.
- `src/state.rs`: the game's state, read from the runtime once a frame.
- `src/dialogs.rs`: the game's questions, shown in the interface's boxes; the shared dialog
  service decides what each answer does.
- `src/art.rs`: the art: the own pieces, the game's icons and the fonts.
- `src/pieces.rs` and `pieces/`: the interface's own pieces and fonts, as the files a host hands
  over.
- `src/font.rs`: the font tables the text is drawn from.
- `src/draw.rs`: the quads, turned into the shared overlay's batches.
