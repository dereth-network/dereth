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
