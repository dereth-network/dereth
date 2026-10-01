# Dereth changes

What each Dereth release (the launcher and the client) brings for players, a few lines each. A
release's notes on GitHub start with its section here, as **Highlights**, and follow it with
**All changes**: the subject of every commit since the previous release that changed the client
or the launcher, grouped.

Add a line under **Unreleased** when a change lands that a player would notice. Say what the
player sees, not how it was done. `cargo xtask release dereth <version>` turns Unreleased into the
release's own section, dated, and opens a new empty one in the same commit as the version (a
pre-release leaves it for the final release, and its notes show it as it is);
`cargo xtask release-notes dereth` shows the notes the next release would get
([`launcher/RELEASING.md`](launcher/RELEASING.md)).

## Unreleased

- After making a character, shortcut keys act once: a healing kit on a shortcut asks for a target,
  and the backpack's shortcut then uses it on you.
- Part of a stack, split off with T, can be dragged straight into a shopkeeper's selling list; and
  carrying an item over a shop's window turns it to the Selling page.
- A tell to the chat target goes to that target, not to whatever happens to be selected.
- The hourglass shows while the server is still answering something you did: a use, a spell, a
  swing, an examine or a shop request.

## 0.1.2 (2026-10-01)

- Campfires, braziers and torches keep their flames and smoke on the object in every landblock,
  instead of drawing them hanging in the air near the player.
- Doors, footsteps and the other sounds of the world's objects play.
- A body standing in several rooms, such as on a stair below an opening, is drawn whole from
  every room that sees it.
- Double-clicking a row of a shopkeeper's stock buys it at once.
- The in-world keys do nothing on the intro, character and disconnected screens: E no longer
  brings up the examine cursor and R the use cursor before you are in the world.
