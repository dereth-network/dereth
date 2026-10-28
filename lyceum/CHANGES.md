# Lyceum changes

What each Lyceum release brings for the people who build worlds with it, a few lines each. A
release's notes on GitHub start with its section here, as **Highlights**, and follow it with
**All changes**: the subject of every commit since the previous release that changed the editor,
the `lyceum` command or the Foundry, grouped.

Add a line under **Unreleased** when a change lands that a world builder would notice. Say what
they see, not how it was done. `cargo xtask release lyceum <version>` turns Unreleased into the
release's own section, dated, and opens a new empty one in the same commit as the version; it
refuses a release with nothing here ([`RELEASING.md`](RELEASING.md)).

## Unreleased

## 0.2.2 (2026-10-28)

- Moving code; no material changes

## 0.2.1 (2026-10-26)

- Definition shows a weenie read only; Edit opens it in its own Editor tab.
- Each kind of weenie gets its own sections, with many more fields.
- Portals list who may use them; flag fields are dropdowns of check boxes.
- Fields changed from the server's are marked; a right click puts one back.
- Choose a weenie's model and icon from pictures.
- The weenie editor shows how a weenie looks, and picks clothing, colors and shade by sight.
- New Characters tab: choose a level, archetype, heritage and skills, then Play As.
- Characters: see and reroll the gear before playing, replace any slot, kits and carried items.
- Characters: augmentations, luminance, summoning mastery, titles and quest flags, including those a place checks.
- Characters: export a character for another world.
- The character builder is faster and laid out in tabs, with Reset beside Play As.
- New Weenie: start from a plain one of 14 kinds, or copy any weenie.
- Edit what a weenie carries, spawns and casts, with chances in percent.
- Edit a creature's attributes, vitals, skills and body parts.
- Emotes read as plain lines, each set with its chance of running.
- The editor lists a weenie's problems; a click goes to the field.
- Find usages shows what carries, spawns or gives a weenie.
- A pack's own weenies can be deleted once nothing uses them.
- Descriptions and speech are written in boxes of several lines.
- A Quests tab lists every quest, where it's used, and changes, renames or removes them.
- A Treasure tab lists every loot table, where it's used, and edits them.
- Levers, pressure plates, doors and creatures can be linked to what they set off.
- The spawn tool's New... makes every weenie, portals included.
- With Collision on, the flying camera slides along walls and floors and steps over small lips.
- Generators that run between set times get From and Until date fields.
- Pickers for palettes, sound and motion tables; long lists are fast.
- The pointer hides while turning the camera, and Play hides windows over the view.
- On macOS 26, Lyceum's icon is drawn in the system's own style, as Dereth's is.
- Fixed the grid and handles stopping short of the world view's edges above 100% UI scale.
- Fixed Play As stalling in some dungeons.
- Fixed Live losing server weenies, quests and spawns after Put back or a refused edit.
- Fixed a removed house leaving behind what was placed in its basement.

## 0.1.1 (2026-10-24)

- Lyceum is faster while flying, browsing scenery and with big dungeons open.
- Build worlds on the Infiltration (February 2005) data files, portal and cell alike.
- Export a world for Empyrean, ACE or GDLE with `lyceum export`.
- Fixed the start screen listing a world twice.
- Drop any retail building from Place, with its rooms and doors, then move it.
- Dropped cottages, villas and mansions are houses players can buy.
- Find rooms in Hierarchy and keep favorite scenery handy.
- Search scenery, filter it by several kinds, star favorites, and tag it by right click.
- Scenery lands on the ground, or on top of what you click.
- Terrain brushes have icons and dropdowns, and a radius in land vertices.
- The world view no longer draws behind the panels.
- Going to something in a room from Nearby takes you into that room.
- Dungeon pieces are whole rooms, drawn as floor plans and grouped by shape.
- Click a doorway or an empty square to see what fits there.
- Delete a room or a whole dungeon, with what stands in it.
- Choose a look to restyle a whole dungeon; renaming renames its portals.
- Doors snap into doorways, and Ctrl+D copies what is selected.
- Lyceum reopens where you left off, window and world.
- Add your own dungeon pieces from glTF and join them in the kit.
- Import your own models from glTF.
- Check how collision fits your model and compare replacement models.
- Playtest characters keep their progress between playtests.
- Play inside the editor's world view; Stop ends the playtest.
- Live edits update the running playtest without logging out.
- Scale the editor's panels from Settings or with Ctrl+= and Ctrl+-.
- Lay out dungeons on a grid a floor at a time, building up and down from where players arrive.
- New dungeons start on the open sea floor, near where you choose.
- Dungeons are a mode: go in from the list, name them, and choose their look.
- Things placed in a dungeon room move and go with it.
- Portals are added like any object; their card picks where they lead.
- A new default layout, with a Tool tab that follows the tool, kept as you arrange it.
- The camera keeps its own speed indoors (10 m/s to start) and outdoors (100 m/s).
- With Collision on, a camera stuck inside something passes through until it is clear.
- With Collision on, the camera passes through doors.
- Shift no longer lowers the camera while you type.
- New weenies take the name you type.
- See the server's weenies and generators in place; your first change copies them.
- Empyrean comes with Lyceum; download its world database from the editor.
- Upgrade older world sources with backups and keep their assigned IDs.
- `lyceum new` makes a world, from the sample or empty.

## 0.1.0 (2026-10-20)

- First release of Lyceum, the world builder.
- A start screen opens your recent worlds, or a new one from the sample.
- Edit a world in a window that draws it as the client does.
- Place and move statics, shape the land, add spawns and build dungeons.
- Play your world in Dereth's client on Empyrean, straight from the editor.
- Export a world's weenies, spawns and quests for GDLE.
- Every change is saved as a content pack, and packs that disagree are all listed at once.
- The `lyceum` command builds, checks, bakes and imports worlds.
