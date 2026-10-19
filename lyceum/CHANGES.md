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

- First release of Lyceum, the world builder.
- A start screen opens your recent worlds, or a new one from the sample.
- Edit a world in a window that draws it as the client does.
- Place and move statics, shape the land, add spawns and build dungeons.
- Play your world in Dereth's client on Empyrean, straight from the editor.
- Export a world's weenies, spawns and quests for GDLE.
- Every change is saved as a content pack, and packs that disagree are all listed at once.
- The `lyceum` command builds, checks, bakes and imports worlds.
