# Empyrean changes

What each Empyrean release brings for server operators and players, a few lines each. A release's
notes on GitHub start with its section here, as **Highlights**, and follow it with **All
changes**: the subject of every commit since the previous release that changed the server or a
shared crate it is built from, grouped.

Add a line under **Unreleased** when a change lands that an operator or a player would notice. Say
what they see, not how it was done. `cargo xtask release empyrean <version>` turns Unreleased into
the release's own section, dated, and opens a new empty one in the same commit as the version (a
pre-release leaves it for the final release, and its notes show it as it is);
`cargo xtask release-notes empyrean` shows the notes the next release would get
([`RELEASING.md`](RELEASING.md)).

## Unreleased

- Swapping weapons in combat leaves you in the new weapon's combat mode: knuckles swapped for a
  wand leave magic mode, and a wand swapped for a bow leaves missile mode with the arrows in hand,
  instead of dropping to peace.

## 0.1.1 (2026-09-29)

- Each release carries fewer files, each with a label naming its system: the five server archives,
  `SHA256SUMS`, `MANIFEST.txt` and `release.json`. The source is GitHub's own source archives of the
  release's tag, which the notes and `NOTICE.txt` name.
- Only a pre-release version (such as `0.2.0-rc.1`) is published as a GitHub pre-release.
