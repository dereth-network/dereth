# Athenaeum changes

What each Athenaeum release brings for the people who build Asheron's Call 2 worlds with it, a few
lines each.

Add a line under **Unreleased** when a change lands that a world builder would notice. Say what they
see, not how it was done.

## Unreleased

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
- Duplicate copies the selected row beside it (Ctrl+D).
- macOS: the editor opens at full size on Retina screens, Cmd works as Ctrl, and Backspace deletes.

## 0.1.1 (2026-11-02)

- New `athenaeum` command: reads, lists and checks Asheron's Call 2 data files.
- New worlds over the world database: `athenaeum new`, `build`, `validate` and `facts`.
- New editor: place, move, turn and remove people, objects, camps and places, then play them.
- Each download carries Yalain and Auberean, so a world can be served and played from the editor.
- New Live and playtest characters with levels, skills, items and quests: play the world as you edit it.
- Moving people and objects, and editing camps and veins, show up in a running test world without a restart.
