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

- Horizon's Better Lighting no longer lights sloped ground face by face, which showed as lime triangles under Global Illumination.
- Horizon's Sky and Atmosphere effect no longer hazes over the land seen through a building's windows and doors.
- Horizon's camera stops directly behind you after a turn
- Horizon's camera comes round behind you quickly and firmly, in a fifth of a second, and with character-based movement running forward brings it round too; Camera Recentre Speed on the Controls page sets how quickly, up to at once.
- A speed no motion means, sent by a server, no longer freezes the client.
- Turning your character with the right mouse button in Horizon no longer stutters.
- Horizon can draw characters and creatures moving smoothly between their animations' frames, at any frame rate: Smooth Animation, on the Client page of its options, off by default.
- Horizon can draw other players, creatures and missiles moving smoothly at any frame rate, rather than stepping thirty times a second: Smooth Movement, beside Smooth Animation, off by default.
- Horizon's names stand on the heads of the people and creatures they name, however tall (a lugian's over its head, not on its chest; a rat's just over its back), and stay on them as they and the camera move rather than trailing a frame behind.
- Turning the camera with the mouse hides the pointer and holds it still, as the original client did, and it comes back where the drag began; in the browser, holding both mouse buttons now runs forward in Horizon.
- In Horizon one press of an attack key or of the Low, Medium or High buttons is the whole attack: the bar charges to the power you aim at and the arrow or swing goes by itself, and goes on repeating until Escape, a step, peace or losing the target stops it. Horizon never uses the advanced combat interface. The buttons used to charge for ever and never attack.

## 0.4.0 (2026-10-09)

- Horizon gamepad mode (experimental): the whole game played with a controller.
- Horizon's options choose the renderer for the next start.
- Horizon has highly experimental rendering effects, off by default and only on the wgpu renderer: better lighting, sun shadows, global illumination, ambient occlusion, lamps and torches, and sky.
- The browser client loads Horizon's art beside the module, so it fits Cloudflare Pages again.
- Horizon shows the client's real version.
- /tod sets the time of day this client draws, for looking at the world by day or night.

## 0.3.0 (2026-10-09)

- **Horizon, a third interface** beside the modern and classic ones, chosen in the options and
  switched live: its own art and fonts, an orbit camera that eases behind you, its own key layout,
  a HUD you can lay out, chat tabs that pop out, and every game window and dialog.
- A building seen from a distance no longer shows its rooms' contents through its simpler, distant
  shell: as in retail, a building opens only through the doors and windows of the shell it draws.
- Stairs, timbers, porch roofs and other structure that reaches out of a room are drawn from
  outdoors again, at any distance; a room's hidden furniture is no longer drawn.
- Furniture and scenery are solid in the same places as in retail: a few objects that reached
  into the next cell could be bumped into from there before you reached them.
- Forests no longer thin out along every landblock boundary: 289,589 trees, bushes and other
  pieces of scenery that retail grows within about 35 m of a boundary are drawn, and solid, again.
  Drawing them costs frame time in dense forest, about a quarter more.
- Rocks and other pieces placed to follow a slope stand upright and face downhill, as in retail,
  where 57,662 of them were tipped over on their sides.
- A creature whose body reaches over a cell boundary is solid from the cell beyond, and you are
  drawn with the landscape only once you are through a doorway, as in retail.

## 0.2.1 (2026-10-06)

- **The web client opens on a launcher**: pick a world from the community list or add one by
  address, choose the data files kept in the browser that the world takes, and play. The client
  is told the world's era, systems, logon version and rules as the desktop launcher tells it. The
  page says which worlds the browser reaches directly (Empyrean with its WebSocket endpoint on)
  and which need a local web relay.
- **The web client keeps each world's overlay** in the browser's own storage: a world that patches
  its data files is drawn with its patch at once, and a later visit downloads nothing it already
  sent. A world's overlay can be removed or refused from its page.
- **A world that updates its data files no longer needs a private copy of them** in the desktop
  launcher when it is played with the Dereth client, which keeps the update in the world's own
  overlay.
- **Worlds that run a client of their own** (Dekarutide, Unfamiliar Shores, Seedsow, Snowreap) are
  known to the launcher: it sends their logon version, plays by their rules, and says where their
  own data files come from.
- **A world that patches its data files is drawn from the patch at once**, the character screen
  included, without a restart.
- The classic interface says why a session ended in a sentence ("Server connection lost") rather
  than a string id; `@version` and the classic options window name the running version in every
  session, not 0.0.0; lists in panels made taller than their default keep their rows in place; and
  the experience to the next level reads INFINITY (classic) or Infinity! at the cap.
- The launcher keeps the world list where it was scrolled, takes a Modern data set without
  `client_highres.dat`, and asks for the Modern files beside the Classic ones for a world before
  Throne of Destiny.
- Sharper icons in the title bar and the taskbar, at every display scale.
- The web page's fields take a paste, the password included.

## 0.2.0 (2026-10-05)

### Added

- **Every era of the game's data, 1999 to 2017, is read**, and worlds built on the February 2005
  data files (Infiltration) play: the client logs in, draws their palette-shifted ground, objects,
  clothing and portal tunnel, casts and logs out as on an end-of-retail world.
- **The classic interface**, the game's interface from before its 2005 redesign, is a choice on the
  options page (Interface), switched live at the character screen or in the world. It needs the
  early-2005 data files. On later worlds it shows their systems in its own style: titles,
  contracts, the journal, Friends and Squelch pages, the eighth-level and Void spells, and the
  cloak, trinket and aetheria slots, which open from an Accessories button beside the shield.
- **The web client plays in the classic interface too**, with the early-2005 files picked beside
  the later ones.
- **The classic interface runs on macOS and Linux**, and its text in the web client comes out the
  same in every browser: it is drawn with the Liberation fonts the client carries, at the sizes and
  spacing the Windows desktop gives it, so every line is as long and wraps where it does on
  Windows.
- The screens follow the world's era and the systems it announces: what it lacks (Contracts,
  Titles, the journal, luminance, cloak and trinket slots, House) is left out, and the tabs and
  panel buttons left close up without a gap. On a February 2005 world, character creation uses
  that world's own texts and shows a dot on the map for each starting town.
- The launcher lists the community's worlds, with each world's era and emulator, and keeps a
  Modern and a Classic data set, each with its own default.
- The launcher shows an Empyrean world's live state, players, era and systems, server version and
  name from the world itself, with no status page, refreshed every minute, and hands the world's
  systems to the client.
- A world can keep its own changes to the data files: the client keeps them in a separate overlay
  for that world and never alters the installed files. One data folder (`--dat-dir`) holds both the
  later and the early-2005 sets; `--classic-dat-dir` names another folder for the early set.
- Terrain Mode (Palette Shift, Classic Blend, Modern Blend) and Sky Mode (Classic Software,
  Classic Hardware, Modern) draw any world's ground and sky in another era's style, given the older data
  files. Object Mode draws the world's objects, buildings, rooms and bodies, the paper doll
  included, in another era's look, switched live.
- Landscape Detail Textures lays the fine ground texture over nearby land, as the clients of 2005
  to 2012 did. Off at first.
- Spell research, on a world that has it: the magic window's Create Spell tab, in both interfaces,
  takes up to eight carried components, by double-click or drag, and Test casts the formula. A
  spell learned there brings up the spellbook.
- The character screen shows the world's message in both interfaces: in the modern interface, an
  Announcements window beside Create Character, scrolled with the wheel.
- A performance panel over either interface shows the frame rate, frame times and where each
  frame's time goes. It is an option on the client options page and an action a key can be bound
  to.
- Every key that can be bound does something in the modern interface: Show Cloak, Select Self,
  Give, Drop, Move to Main Pack, hold sidestep, and the trade and spell research windows. This
  client's own actions are listed under Dereth on the key page.
- On a world that charges unassigned experience to swear allegiance again, the swear confirmation
  and the classic allegiance panel say what the oath costs.

### Changed

- One options set for both interfaces: the same pages (Game and Support, Character Options, Chat
  Options, Client Options), each drawn in its interface's style. A change on either page shows on
  the other, takes effect when applied and lasts past logging out; rows for what the world's era
  lacks are left out. Urgent Assistance and Report Abuse open the in-game forms.
- One set of key bindings for both interfaces, each with key map files of its own
  (`<name>-modern.keymap`, `<name>-classic.keymap`). Each key page lists only the rows its
  interface acts on and offers both interfaces' defaults, and a cleared key stays cleared at the
  next start. The classic defaults are the game's 2004 key map, and classic keys held with Shift,
  Ctrl or Alt work.
- Multiple Pass Alpha softens the edges of cut-out textures such as leaves, and particles and
  translucent objects are drawn in their true order by distance.
- Detail levels are measured from the eye the frame is drawn from, and Degrade Distance applies on
  every world.
- The modern interface's 3D view fills any screen, however wide.
- The chat font's face and size are chosen on the modern interface's Chat Options page.
- Use Mouse Turning Settings sets the mouse-turning preset; it no longer restores the defaults or
  changes the character's options.
- The client starts in a 1024 by 768 window, and Defaults keeps that size.
- A word the language filter censors keeps the punctuation at either end of it, so a censored
  first or last word keeps the line's quote.
- A crash log is kept only for a run that goes wrong, without the login, and only the newest 20
  are kept. `DERETH_SETTINGS_DIR` names the settings folder outright.
- `--world-dat-dir`, `--legacy-dat-dir` and the `LegacyDatDir` setting are gone: one `--dat-dir`
  holds every set.

### Fixed

- Animated scenery and objects move: butterflies flutter over the meadows, and the other placed
  objects with an animation of their own play it instead of standing frozen in their first pose.
- Switching interface from the options page no longer leaves the other interface needing an extra
  click for every switch made.
- The classic chat log shows the world's welcome once, however often you log back in or switch
  interface.
- A random heritage in character creation is always one the screen offers: Aluvian, Gharu'ndim or
  Sho in the classic interface, and one the world has in the modern one.
- `@version` names the client's own version instead of 0.0.0.
- At a load of exactly 110% the burden penalty shows 20%, as the game's own client does.
- Components carried at log-in are listed in the Components tab whatever order they arrive in.
- Component icons, in the magic window and a spell's examination, have black outlines instead of
  white, and an examined spell shows its icon composed as the spellbook does.
- Scenery and buildings draw each part at its own scale.
- Logging off releases the world, its objects and its sounds, so the next login starts clean.
- With mouse turning on, the body stops turning when the mouse stops under mouse look, and when
  the mouse-look button is let go.
- Switching back to the modern interface brings it up on the screen the game is at, with the paper
  doll at its own size and every option as the other interface left it.
- The classic interface: its Brightness slider no longer doubles the world's brightness; the
  pointer no longer trails the mouse; text is drawn as the game's own glyphs are; it draws in a
  fraction of the time it took; the copy key copies the chat log's selection; a drag in the 3D
  view picks up only what is under the pointer; a salvaging tool opens the salvage window;
  inverted mouse look reverses every step; holding the run key walks; tells read "Name tells
  you"; the wheel scrolls every list and the window under the pointer; the startup screen's text
  is centred.
- On a February 2005 world, objects without a detail record of their own (Holtburg's cottage
  stairs, doors, beams) draw their own models, and the portal tunnel is no longer black; drawn in
  the later look, a room's furniture keeps its detail up close.

## 0.1.3 (2026-10-01)

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
