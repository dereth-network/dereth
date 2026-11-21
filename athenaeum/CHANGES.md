# Athenaeum changes

What each Athenaeum release brings for the people who build Asheron's Call 2 worlds with it, a few
lines each.

Add a line under **Unreleased** when a change lands that a world builder would notice. Say what they
see, not how it was done.

## Unreleased
- macOS: fixed arrow keys typing odd characters into text fields.
- New Fast view (F7) for seeing far more of a big world at once.

## 0.1.10 (2026-11-21)
- Worlds can now give examine pages, broker consignment limits, oath gates and weighted effect picks.

## 0.1.9 (2026-11-20)
- Windows: the taskbar now names the editor Athenaeum.

## 0.1.8 (2026-11-17)
- Just moving code around

## 0.1.7 (2026-11-14)
- Play retail now works on worlds that change the game files, from a copy of the client.
- Play now plays the world inside the editor's own window.
- Storekeepers can now have a shop title.
- Every field and pet can now be edited in the editor.
- Files you edit by hand now show in the editor at once.
- Clicking a problem or warning now takes you to its row.
- New format reference and a sample world to start from.

## 0.1.6 (2026-11-10)
- Reads the world rows Yalain added since the last release.

## 0.1.5 (2026-11-09)
- Moving a camp moves its spots too.
- Renaming a place or moving a camp keeps the quests and rows that name it pointing at it.
- Editing a captured row keeps where it came from, marking only the fields you changed.
- Files you edit by hand reach Live without restarting the editor.

## 0.1.4 (2026-11-07)
- New `pets` command: see and change a world's kinds of pet.

## 0.1.3 (2026-11-05)

- New clearing tool keeps a spot free of trees, rocks and grass.
- A covered window on a Mac no longer runs flat out.
- People can be given a name of their own.

## 0.1.2 (2026-11-05)

- New map of the whole world: click to go there.
- Find rows goes to a room by its id, and Enter takes what it offers first.
- A place can be renamed, and what leads to it follows.
- Placed rows now play their effects while you edit, such as a portal's glow.
- The inspector saves what you type when you click away, and needs no quotes.
- macOS: Intel Macs are no longer supported; Apple silicon only.
- The editor asks before closing with changes not saved.
- Clicks on a window over the world no longer reach the world.
- Placing shows the row under the pointer first; R turns it and Shift+click keeps placing.
- A selected row has handles to move it east, north and up and to turn it, with Snap for steps.
- Find rows (Ctrl+F) finds any row of the world and goes to a landblock.
- Names read as people read them, without the game's grammar marks.
- The editor now goes indoors: rooms and dungeons are drawn, and rows are placed on their floors.
- Click a person or an object itself to select it, not just its marker.
- Shift+click selects several rows, to move, turn, copy and take away together.
- Lead a portal to a place from a list, and change a camp's creatures by name.
- Placing and dragging land on platforms and other things the world has built, not just the ground.
- A new world now opens looking at a town, not into a rock.
- Find shows templates as pictures, and what can be worn on a body.
- G puts the selection on the ground, and Shift+G leans objects with it.
- Rows can be copied between landblocks with Ctrl+C and Ctrl+V.
- Objects can be scaled: `=` and `-`, or the scale field.
- Duplicate copies the selected row beside it (Ctrl+D).
- macOS: the editor opens at full size on Retina screens, Cmd works as Ctrl, and Backspace deletes.

## 0.1.1 (2026-11-02)

- New `athenaeum` command: reads, lists and checks Asheron's Call 2 data files.
- New worlds over the world database: `athenaeum new`, `build`, `validate` and `facts`.
- New editor: place, move, turn and remove people, objects, camps and places, then play them.
- Each download carries Yalain and Auberean, so a world can be served and played from the editor.
- New Live and playtest characters with levels, skills, items and quests: play the world as you edit it.
- Moving people and objects, and editing camps and veins, show up in a running test world without a restart.
