# Launcher changes

What each launcher release brings for the people who use it, a few lines each. A release's notes on
GitHub start with its section here, as **Highlights**, and follow it with **All changes**: the
subject of every commit since the previous release that changed the launcher.

Add a line under **Unreleased** when a change lands that a player would notice. Say what they see,
not how it was done. `cargo xtask release launcher <version>` turns Unreleased into the release's own
section, dated, and opens a new empty one in the same commit as the version.

## Unreleased

## 0.5.1 (2026-11-05)

- A hosted world can now be saved in the Launch era.
- Fixed the Play page offering no older data files for a world you host.
- AC1999 can be listed and played through a relay; off unless turned on.
- Fixed getting the world database for an Infiltration world on an older Empyrean.

## 0.5.0 (2026-11-05)

- A Yalain world's settings can turn on WebSocket connections.
- A Yalain world's update backups are now kept and trimmed with its own.
- The launcher now has its own releases and updates apart from the games.
- macOS: a running game now shows its own name and icon.
- Fixed the page covering the line at the bottom of the window.

