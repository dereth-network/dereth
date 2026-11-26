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
- Horizon: character creation on a New Dereth world offers only its peoples.
- New Dereth: grass and plants no longer grow on the roads.
- Character creation offers only the heritages a world allows, in every interface.
- Plugins: nav.follow now steers while running and keeps pace with its leader.
- Plugins: large metas load faster.
- /perfcapture now also records GPU times and how much each frame draws.
- dereth-art can build New Dereth's land, every AC2 dungeon in it, from your own installs.
- A world can color its sky by the hour, as the AC2-in-AC1 world now does.
- Plugins can find a spell by its name, icon included.
- Creatures from Asheron's Call 2 can now be brought into worlds, with their own animations.
- Objects the server makes see-through, such as ghostly creatures, now draw see-through.
- Tank: crafting now uses heads and shafts picked up after it starts.
- Navigator: routes now go around the barriers of houses you can't enter.
- Large data patches apply much faster, and show their progress.

## 0.4.23 (2026-11-23)
- Horizon: fixed plugin windows over a moved party list not taking clicks or drags.
- Plugins: a list can now be narrower than 8 cells when its width bounds say so.
- Plugins: buttons and toggles can show a game icon and an accent color.
- Horizon: HUD Layout can hide elements, and plugins can hide the party list, EXP and target bars.
- Horizon: HUD elements can be made as small as 40%.

## 0.4.22 (2026-11-23)
- Fixed Direct3D 12 screenshots showing an older frame, or a black one.
- Navigator: walks round large objects that come in sight, such as yard walls, instead of into them.
- Fixed your character getting up and running after dying on a Navigator journey.
- Navigator: the dungeon map grows with its window.
- Horizon's Smooth Movement now smooths your own character and the camera too.
- macOS: the arrow keys move the chat caret instead of typing odd characters.
- Faster frames in big dungeons.
- Fixed a short stutter every 15 seconds.
- Plugins: fixed messages between clients stopping after a minute on Windows.
- Plugins: routes now run smoothly through their waypoints.
- Fixed the Navigator stumbling or stopping when you click to another window.
- Navigator: a search lists each place once, with an exact name first.
- Navigator: the window no longer opens far too wide.
- The client no longer draws while minimized.
- Remaster: no trees or water showing through a dungeon's open doorway any more.
- New frame rate limits for the active window and the background, replacing Slow Down in the Background.
- Plugins can ask the server what the pack can craft, and answer craft confirmations.
- Tank: crafts missile ammunition and healing kits.
- Plugins: a journey's guide stays put on the ground as you run, and turns smoothly round corners.
- Navigator: learns where your recalls take you, from using them, portal ties and lifestones.
- Worlds can now use a finer land grid, drawn and walked in both looks.
- The maps name a world's own towns and landmarks.
- Fixed a world entered during its data patch being built from the old files.
- A world can color its sky by the hour.
- dereth-art can build the AC2-in-AC1 world from your own installs.

## 0.4.21 (2026-11-21)
- Fixed the game freezing while a world's large data patch downloads; it now shows progress.
- Fixed some worlds sending their whole data patch again at every login.
- Fixed data a world had removed staying in your copy after the world updated.
- Remaster: Launch-era doorways show the room inside instead of solid blue.
- Remaster: furniture set against a wall no longer flickers into it.
- Fixed a stall of a second or two right after a world's data patch.
- Navigator: a simpler window, with one list of places and a Dungeon Map button.
- Fixed the Navigator walking into portals on its way that it wasn't taking.
- Plugins: a journey a shut door stops now says which door and why.
- Fixed the Navigator running into a closed door before opening it.
- Navigator: a faint wisp shows the way ahead while you travel.
- Plugins: draw paths in the world with `draw.path`.
- Horizon: the Spellbook's school tabs no longer come up empty.
- Plugins: metas that count items no longer slow down on a full pack.
- Plugins: a tooltip no longer stays on screen after its window changes.
- Plugins: large VTank metas load without running out of time.
- Tank: the Meta page lists a large meta's rules a hundred at a time.

## 0.4.20 (2026-11-20)
- Navigator: opens doors that only a lever works, by using the lever.
- Navigator: drops off a ledge when nothing else gets there, if the fall won't hurt.
- Fixed the Navigator's turns swinging back and forth at low frame rates.
- Horizon map: the whole world shows only towns, with more places as you zoom in.
- Fixed the Navigator stopping downstairs, and waiting at portals that turn you away.
- Fixed other players jumping back and forth while they fight.
- Fixed the Navigator stopping when you search for a place.
- Fixed the Navigator planning journeys through destroyed portals.
- Remaster: the ground no longer hides a building's downstairs rooms from outside.
- Journeys reach walled-off land, like Mt Esper's crater, through the portals that lead in.
- Fixed Tank waiting forever to go back for a corpse from the Training Academy.
- Fixed the client ignoring the server when it puts your character back.
- Auto-walk goes round doors that can't be opened, and stops short of portals it isn't taking.
- Web: the Navigator knows a world's portals and places when you play through a relay.
- Fixed auto-walk sticking on ramps and heading for portals it can't walk to.
- A journey the Navigator can't take now says why, such as the level a portal needs.
- Fixed journeys that would not go to a portal standing in a far-off building.
- Fixed a portal's light showing through dungeon walls; remaster indoor lamps are softer.
- Navigator: finds the places on your map and walks around walls, faster on long routes.
- Classic: selecting someone's pet now shows its health bar.
- Remaster: indoor lamps no longer over-light rooms, and outdoor lamps light up the streets.
- Remaster: faster on Macs in busy places.
- Remaster: bounced light is faster on Macs and on cards without ray tracing.
- macOS: the client's windows take clicks again after leaving full screen.
- Remaster: now in the Classic and Modern interfaces too, with its options.
- Windows: fixed the taskbar sometimes showing a blank icon for the client.
- Horizon: admin commands no longer act on a creature after it's deselected.
- Windows: the taskbar now names the client Dereth.
- Web: WebGL now asks for the faster graphics card, and the status line names it.
- Fixed sleds starting off facing the wrong way on a slope.
- Fixed examining a creature with negative armor showing a huge armor level.

## 0.4.19 (2026-11-17)
- Remaster: distant forests no longer shimmer with ray-traced shading.
- Asheron's Call 2 art pack: creatures now walk more naturally.
- Fixed logging into some GDLE servers.

## 0.4.18 (2026-11-14)
- Remaster: faster with lots of plants in view.
- Remaster: smoother haze at dawn, with no hard-edged shadows in it.
- Remaster: switching the art pack no longer leaves trees missing or the lamps off.
- Remaster: new Scenery distance option.
- Remaster: the camera no longer stutters walking over uneven ground.
- Remaster: cliff feet blend into the ground instead of banding.
- Remaster: distant trees are sharper and no longer pop to full detail as you walk.
- Remaster: an art pack's trees keep their own colors, and far forests show them.
- Remaster: pebbles and grass take the color of an art pack's ground.
- Remaster: art packs can give the ground a detail texture.
- Explore mode: fly a camera anywhere in the world with no server.
- Remaster: art packs can replace creatures with animated bodies of their own.
- Remaster: rock shelves on cliffs are now off unless you tick Cliff shelves.
- Remaster: the view no longer bobs up and down as it turns with smoothing or an upscaler on.
- Remaster: no more hitch when crossing into a new area.
- Remaster: no more dry-looking strips at the water's edge.
- Remaster: shallow water near the shore is see-through, and things in it stand on its bed.
- Remaster: no more gray band in the water along the shore.
- Remaster: no more bright bars at the water's screen edges.
- Fixed AC1999's patch downloading again on every launch.
- Remaster: trees sway in the wind.
- Remaster: art packs can grow ground cover.
- Remaster: dense art packs no longer cost the processor.
- Remaster: wading leaves a wake of crossing ripples.
- Remaster: no more ring of water around the foot of a tree on a lake's bank.
- Remaster: far mountains no longer flicker, with or without ray-traced shadows.
- Remaster: rain no longer leaves a gray haze over trees.
- Fixed Play Again failing to start after its data files were removed and added again.
- Launcher: a new first run, and the Library downloads the data files for you.
- The map shows the lifestones, portals, shops and houses you have found, and those Empyrean worlds publish.
- Remaster: wet ground in the rain no longer turns nearly black.
- Fixed a world refusing your data after you switched its game files.
- Horizon: a new zoomable world map, with its towns and houses.
- Remaster: doorways in Launch-era dungeons no longer draw black.
- Each world remembers the interface you chose there; the Library's Settings set the default.

## 0.4.17 (2026-11-10)
- New Admin plugin: an administrator's commands in a window.
- Plugins: a hotkey's key can be changed with /plugin key.
- Fixed the paper doll and character previews showing patched items in their old look.
- Horizon: new Status Bar Colors and Large Status Bar Numbers options.
- Remaster: names and selection rings no longer sink into the grass.
- A world's own data now downloads much faster from Empyrean worlds.

## 0.4.16 (2026-11-09)
- Web: a link to an Asheron's Call 2 world now opens it in Auberean.
- Fixed walking after logging in until Shift was pressed and released.
- AC1999: fixed logging back in after logging out.
- Remaster: a new Horizon renderer that draws art packs, off by default.
- Better Lighting is gone; Remaster takes its place.
- Remaster: several art packs can be used at once; a later one's models win.
- Character select shows Holtburg on a fair morning, its clouds and water moving.

## 0.4.15 (2026-11-07)
- The relay that plays AC1999 now comes with Dereth.
- Logging in now reports the world's player count to the hub, with no opt-out.
- Fixed a crash on exit on Linux with Wayland and NVIDIA.
- Launch: the character page and character options show only what 1999's did.
- Launch: spells ask for the tapers of your own account.
- A window nobody can see no longer runs flat out.
- macOS: Command works as Control, for shortcuts and copy and paste too.
- macOS: the client asks for Game Mode.
- macOS: drag the window to any size; it reopens that size, in points on Retina.
- macOS: Horizon draws at the screen's own pixels, sharp in a window too, with a new render scale option.
- Horizon: lighter text outlines, easier to read over a bright sky.
- macOS: fixed the game running at 10 frames a second after starting behind another window.
- macOS: logging out of full screen puts the window back where it was.
- Better Lighting: smoke now covers distant trees instead of fading into the haze.
- Better Lighting: glows and torches add as they do in the ordinary picture.

## 0.4.14 (2026-11-05)

## 0.4.13 (2026-11-05)

- Classic: resting the pointer on a button, bar or slot says what it does.
- Refused stances and emotes now say why.
- Launch worlds: Classic's spellbook, chat menu and messages match the 1999 client.
- Web: the launcher opens much faster, fetching the game only when you play.
- The client now updates on its own, apart from the launcher.
- Launch worlds: the Charge bar and each spell's duration range.
- Fixed spell components missing from the Components tab when they arrived before their pack.

- macOS: Intel Macs are no longer supported; Apple silicon only.
- Better Lighting: no more white line across things standing in water.
- Fixed a right-drag begun away from the window's middle throwing the camera across.

## 0.4.12 (2026-11-03)

- Launcher: you can add a world by its address in Auberean's realm too.
- Launcher: a hosted world can keep its status page open to your local network.

## 0.4.11 (2026-11-02)

- Fix macOS packaging

## 0.4.10 (2026-11-02)

- Launcher: Auberean's realm is open to everyone: play Asheron's Call 2 with Auberean or your retail client, host and update worlds with Yalain, and build, publish and export worlds with Athenaeum.
- The launcher is now Dereth Network, with its own icon.
- Launcher: a settings file it can't fully read now keeps everything it can, and says what was reset.
- Web: ACE and GDLE worlds can host the web relay, so players install nothing.
- Web: the front page is now the launcher.

## 0.4.9 (2026-11-01)

- Launcher: each realm lists only its own game's worlds.
- Commands, help, travel routes and the character sheet follow the systems the world says it has.
- Launcher: 1999 data files are their own kind, which Launch worlds need.
- Launcher: a world shows its era and what it changes, and hosting offers the systems you'd look for by name.

## 0.4.8 (2026-10-28)

- Decal: a Dereth Network window for the retail client.
- Plugins are on now: install, update and build them on the launcher's Plugins page.
- Launcher: a Plugins page to install, update and build plugins.
- Launcher: a much smaller download.

## 0.4.7 (2026-10-26)

- New Dereth icon.
- Fixed the client freezing after buying a stack from a shop.
- Experimental plugin support, off by default.

## 0.4.6 (2026-10-24)

- Started without the launcher, the client opens a new player in the modern interface.
- Horizon: smaller window title bars, and sharper text and art at every scale.
- Host your own world from the launcher, set it up, share it with a link, and run its console.
- Host the worlds you build in Lyceum; players get each publish as a patch.
- Update the Empyrean a world you host runs, backed up first.
- Check from outside whether friends can reach a world you host.
- A world link from a friend opens Add world in the launcher.
- Empyrean's page also shows other world servers running here, and an Empyrean one's console.
- Get Empyrean and Lyceum, and make, open, export and delete Lyceum worlds, from the launcher.
- A world whose data is refused now stops before you play, and says why.

## 0.4.5 (2026-10-20)

- Better Lighting smooths jagged edges.
- New /report and /perfcapture commands.
- Better Lighting runs faster.
- Better Lighting can draw art packs.
- Web: Empyrean worlds with WebSocket on are played without the relay.
- Sync to refresh now works in full screen with Better Lighting.
- Less stutter when walking into new land.
- Faster frames at long draw distances.
- Shorter replies and help for /weather and /tod.
- Horizon: effects with no end show ∞ instead of Lasting.
- Horizon: a spell name cut short shows in full under the pointer.
- Horizon: skill tooltips stop showing vitae once it is gone.
- Horizon: steering with the right mouse button no longer fights turning to face a target.
- Horizon: right-click an item a vendor sells to examine it.
- Horizon: names over people and creatures are hidden behind walls and hills.
- Better Lighting no longer freezes the game for minutes as it starts on Direct3D 12.
- Better Lighting no longer draws trees black or blue in the rain.

## 0.4.4 (2026-10-15)

- Web: fullscreen is on by default and fills the browser window.
- Web: new render scale option.
- Web: the launcher shows the same worlds as the desktop launcher.
- Web: fixed the client sometimes failing to start after an update.
- Launcher: gamepad controls (Windows).

## 0.4.3 (2026-10-13)

- Web addresses (http and https) in chat are links in every interface: click one and, once you say yes, it opens in your browser.
- While a game the launcher started is running, Discord shows its world, client and era and how long you have played, with nothing to sign in to; the Library's "Show what I'm playing on Discord" turns it off.
- Horizon keeps each character's look on character select, HUD layout and gamepad cross hotbars per world, so same-named characters on two worlds, even two on one host, no longer share or overwrite them; what was kept before carries over.
- Costumes, masks and creature parts painted in their own colors keep them, as in the original game: an Undead Guise is a dark skeleton on every heritage instead of speckled noise in the wearer's skin color.
- Client Options in every interface has a Slow Down in the Background box: untick it to keep the full frame rate while the window is in the background.

## 0.4.2 (2026-10-12)

- The launcher shows every world's era instead of asking (what the world says, or End of Retail), and Dereth Network worlds' players and whether they are up, from the hub, ACE worlds included.
- Horizon's Better Lighting no longer brightens the outdoors well past the ordinary picture at low sun, in shade and in towns: by day it stays close to it, at night as dark as before.
- Dereth Network channel lines show as channel lines, "[group] Person@world says, ...", in a channel color instead of as tells from the channel, and so do the lines you send to a channel.
- Horizon's experimental Better Lighting now lights dungeons and rooms: pools of light around the torches, shade between them, no light through walls with Lamps ticked, light bounced about the room with Bounced Light, and an exposure that adjusts to the room and eases in and out of doorways.
- Horizon's skills show their value with your spells in it, green when raised and red when lowered, as attributes already did; hovering a skill, attribute or vital shows its base and what magic adds, as "200 (+35)".
- Horizon shows an item's cooldown until the server lets it be used again, never ending a second or so early, and shows cooldowns on the gamepad's cross hotbars too.
- In Horizon's vendor window, a right-click on an item in the shopping list or the items to sell takes it back off the list.
- An item dragged in Horizon follows the pointer as its own picture, without the dark slot that was drawn behind it.
- Turning on the spot near the edge of a landblock no longer makes the distant landscape flicker and change shape: the landscape follows your character, not the camera.
- Examining a creature: Horizon heads the window with its name and gives its kind beside its level, the modern and classic interfaces spell its kind as the game does (Fire Elemental, not Fire_Elemental), and on a world running the February 2005 files the modern interface frames its model whole instead of looking out from its feet.
- Horizon's names stand still over portals and creatures that move in place, rather than rising and falling with their animation.
- In Horizon's gamepad mode, LB held with the right stick up or down zooms the camera in and out, and a tap of LB turns autorun on only while you are already running forward.
- In Horizon's gamepad mode, picking up an inventory item with A no longer uses it when an item was picked up from the same place before.
- In Horizon's gamepad mode, the target's name and health bar show the alternate selection while its ring is up, and the selection again once it closes. Horizon's target bar shows a health only once the game has told it, and none for a thing with no health.
- The launcher asks which interface to play in the first time it starts (Horizon, Modern or Classic), and its Library can change it later.
- Walking from one landblock into the next at a long draw distance no longer leaves holes in the landscape for a few frames.
- Horizon's camera tilts with the ground as you run up and down hills, as the other interfaces' camera does: Align Camera to Slope, on Horizon's Controls page, on by default.
- The butterflies, bats and birds that fly over the land as scenery circle round as they did in the original game, instead of beating their wings in one place.
- Horizon's experimental effects cost less with the picture unchanged: Better Lighting adds about a third of the processor time it did and a sixth less graphics time, and each other effect less processor time too.
- Horizon's experimental effects cost less: Better Lighting adds about a third of the processor time it did and a sixth less graphics time, and each other effect less processor time too. Above 2560x1440, Ambient Occlusion also takes under a third of the graphics time it did, its finest creases a little softer.
- Horizon's experimental Weather needs Better Lighting, its box greyed until that is ticked, and its snow falls without lying on roofs or the land.
- Horizon's names stay whole twice as far off outdoors (out to 36 meters, gone at 60) and further indoors (out to 20 meters, gone at 25).
- Dragging a pack along Horizon's pack column with the mouse moves it to that place among your packs.
- Horizon's Create Spell window shows the whole frame of every component slot, the first column and row included.
- With Horizon's better lighting, smoke, portals and other see-through effects no longer glow green or light up the trees behind them at night.
- The launcher's world list, on the desktop and in the browser, comes from the Dereth Network, falling back to the community list when it is unreachable; the network's test worlds show the status Test.

## 0.4.1 (2026-10-11)

- Turning your character with the right mouse button in Horizon no longer stutters.
- Horizon can draw characters and creatures moving smoothly between their animations' frames, at any frame rate: Smooth Animation, on the Client page of its options, on by default.
- Horizon can draw other players, creatures and missiles moving smoothly at any frame rate, rather than stepping thirty times a second: Smooth Movement, beside Smooth Animation, on by default.
- Horizon's names stand on the heads of the people and creatures they name, however tall (a lugian's over its head, not on its chest; a rat's just over its back), and stay on them as they and the camera move rather than trailing a frame behind.
- Turning the camera with the mouse hides the pointer and holds it still, as the original client did, and it comes back where the drag began; in the browser, holding both mouse buttons now runs forward in Horizon.
- On a 60 Hz display (and at 30, 120 or 240 Hz) the world moves on a steady 30 steps a second instead of stepping unevenly 20 to 25 times a second.
- Horizon's experimental effects gain Weather (rain that roofs keep off, wet ground, puddles, and snow over snowy land), and /weather sets the weather this client draws: clear, rain, snow or auto.
- A new player starts in Horizon; a player who chose the modern or classic interface keeps it.
- Horizon's character select and character creation can be full screen: Alt+Enter, or the switch at their top right, and the choice carries into the world and back.
- Horizon's interface scale chooses itself until you choose one: the largest the window holds, up to 200% (150% at 1440p, 200% on a 4K monitor), and only the scales the window holds are offered.
- On the wgpu renderer, Horizon's experimental effects no longer say they need wgpu or that the graphics card lacks ray tracing: a box first ticked says to restart, and the restart turns it on.

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
