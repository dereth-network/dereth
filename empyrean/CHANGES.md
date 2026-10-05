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

## 0.2.0 (2026-10-05)

### Added

- **Eras:** an `[era]` profile chooses the era a world plays. The end of retail, the default, is
  unchanged. Infiltration (February 2005) plays by ClassicACE's rules: characters start outdoors
  in their chosen town with recalls enabled, the level cap is 126 with the three original
  heritages and the era's 36 skills, and combat, vitae, starter gear, the Welcome Letter, loot and
  magic items are the era's. Empyrean credits ClassicACE beside ACE.
- Empyrean runs on the February 2005 data files (`portal.dat` and `cell.dat`), checks a client's
  files against them, and turns away a client with the wrong set, saying why.
- `empyrean-import --era` builds a world pack for an era, and the server refuses a pack built for
  another; `empyrean-import fetch --world 16py` downloads ACE-World-16PY for Infiltration.
- A world's systems (trade, housing, apartments, tinkering, cantrips, spell research, chess,
  titles, contracts, luminance, aetheria, cloaks, trinkets) follow its era and can be set in
  `[era]`. The server refuses what the world lacks, and an item worn in a slot the world lacks
  goes to the pack at log-in.
- **Data overlays:** a world can serve its own changes to the data files as an overlay over a
  locked base set (`[dat_overlay]`), and patches Dereth clients to it; `empyrean-import
  dat-overlay` makes one from a world's data files and its base. Other clients are patched as
  before.
- `GET /status` and `GET /v1/world` report the world's era, its systems, its data files and the
  client versions it admits.
- **Status ping:** a launcher can ask a world on its game port, with no login and no web address,
  whether it is open, how many are on, its era and systems, the server software and the world's
  name. It is on by default; `[status] udp_ping = false` turns it off, and the section's rate
  limits bound how often one address may ask.
- Spell research: with `spell_research` on, a tested formula of components teaches and casts the
  spell it makes. A test is judged in order: an impossible formula, then a wrong target ("Incorrect
  target type"), then a formula that makes no spell, which is spoken and fizzles.
- The character screen carries a message from the world: `server.character_screen_message`, or
  "Welcome to <world name>!" when none is set.
- On Infiltration, experience keeps arriving at the level cap, up to 4,294,967,295.
- `swear_xp_cost` (on for Infiltration): after breaking from a patron, swearing again costs five
  percent of the next level's experience, between 100 and 5,000, a quarter more for each break.

### Changed

- On an era before the item auras, a weapon enchantment such as Swift Killer or Blood Drinker cast
  on yourself lands on your wielded weapon or caster, and fails with one message when you wield
  nothing it can take.
- Speech, emotes and local chat reach the players who know the speaker within range, as ACE sends
  them, instead of everyone in the nearby landblocks.
- On older data files, motion is sent and read in the numbering of the world's files, so a
  February 2005 logout plays its departure in full.
- On the February 2005 data files a new character's arms and hands are bare, as character
  creation dresses them.

### Fixed

- A beneficial spell cast on another player who is not a player killer lands again: every spell
  between two non-player-killers was refused with "... is not a player killer".
- On the February 2005 data files, start-up no longer logs the tables and records those files
  never had.

## 0.1.2 (2026-10-01)

- Swapping weapons in combat leaves you in the new weapon's combat mode: knuckles swapped for a
  wand leave magic mode, and a wand swapped for a bow leaves missile mode with the arrows in hand,
  instead of dropping to peace.

## 0.1.1 (2026-09-29)

- Each release carries fewer files, each with a label naming its system: the five server archives,
  `SHA256SUMS`, `MANIFEST.txt` and `release.json`. The source is GitHub's own source archives of the
  release's tag, which the notes and `NOTICE.txt` name.
- Only a pre-release version (such as `0.2.0-rc.1`) is published as a GitHub pre-release.
