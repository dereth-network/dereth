# Parity with the plugins players know

**Provenance: design.** The plugins Dereth ships, set feature by feature beside the plugins players
ran with the retail client: Mag-Tools, VTank, GoArrow and UtilityBelt. Each feature is **done**
(Dereth does what the original does for a player), **partial** (some of it; the note says what is
not there) or **missing**. Every feature marked done or partial names the sample that has it; the
samples are walked through in [the guide](guide/samples/README.md).

| program | done | partial | missing |
|---|---|---|---|
| Mag-Tools | 15 | 2 | 1 |
| VTank | 22 | 2 | 2 |
| GoArrow | 8 | 1 | 0 |
| UtilityBelt | 7 | 9 | 6 |

## Mag-Tools

| feature | | in Dereth |
|---|---|---|
| Combat tracker: damage dealt and taken by creature and element, crits, evades, resists, kills | done | [Trackers](guide/samples/trackers.md), Combat |
| Experience and luminance per hour, time to the next level | done | [Trackers](guide/samples/trackers.md), Experience; [XP Meter](guide/samples/xp-meter.md) |
| Mana tracker: each worn item's mana and how long it lasts, with a warning | done | [Trackers](guide/samples/trackers.md), Mana |
| Consumables tracker: components, kits, stones, notes and salvage used, with a warning | done | [Trackers](guide/samples/trackers.md), Items |
| Corpse tracker, and the arrow on your own corpse | done | [Trackers](guide/samples/trackers.md), Corpses |
| Player tracker | done | [Trackers](guide/samples/trackers.md), Players |
| Chat filters: lines hidden by their words, channel or sender | done | [Chat Filter](guide/samples/chat-filter.md) |
| Chat logger | done | [Chat Filter](guide/samples/chat-filter.md), Log |
| Inventory logger: every character's items, searched from any of them | done | [Inventory](guide/samples/inventory.md), Find |
| Item information when an item is appraised | done | [Inventory](guide/samples/inventory.md) |
| Partial stacks merged (auto stack) | done | [Inventory](guide/samples/inventory.md); [Tank](guide/samples/tank.md) |
| Items put in their packs (auto pack) | partial | [Inventory](guide/samples/inventory.md): its rules are words of an item's name and a pack number, a format of its own |
| Buying and selling at a vendor by the vendor's own profile | done | [Inventory](guide/samples/inventory.md); [Tank](guide/samples/tank.md) |
| Notices of rares, high value and named spells in what drops | done | [ID Notices](guide/samples/id-notices.md) |
| Logging in a chosen character, again while its last session is in the world | done | [Auto-login](guide/samples/autologin.md) |
| Commands typed after every login | done | [Auto-login](guide/samples/autologin.md) |
| Commands for the next login only | missing | |
| Inventory files for other programs | partial | [Inventory](guide/samples/inventory.md) keeps the log in the plugin's world store, searched in the game; the suits SuitBuilder made from those files it builds itself |

## VTank

| feature | | in Dereth |
|---|---|---|
| Routes (`.nav`): circular, linear, once and follow, with their waypoints | done | [Tank](guide/samples/tank.md), Routes |
| Routes made in the game and saved as `.nav` | done | [Tank](guide/samples/tank.md), Routes |
| Jumps, portals, vendors, NPCs, recalls and pauses on a route | done | [Tank](guide/samples/tank.md), Routes |
| Fighting by the monster list: melee, missile, war and void, by priority | done | [Tank](guide/samples/tank.md), Fighting |
| Streaks, arcs, rings, debuffs and damage over time | done | [Tank](guide/samples/tank.md), Fighting |
| Pets | done | [Tank](guide/samples/tank.md), Fighting |
| Self buffs at the best level known, recast as they run low, Force Buff | done | [Tank](guide/samples/tank.md), Buffs |
| Item buffs, the buff profiles' protections and banes, buff items | done | [Tank](guide/samples/tank.md), Buffs |
| Vitals: kits, foods, potions and spells, by VTank's lists | done | [Tank](guide/samples/tank.md), Vitals |
| Worn items' mana refilled from mana stones | done | [Tank](guide/samples/tank.md), Vitals |
| Dispelling vulnerabilities | partial | [Tank](guide/samples/tank.md), Vitals: `UseDispelDrum` is not carried |
| Looting by a loot profile (`.utl`) | done | [Tank](guide/samples/tank.md), Looting; [Looter](guide/samples/looter.md) |
| Salvaging what the profile salvages, and combining bags by its ranges | done | [Tank](guide/samples/tank.md), Looting |
| Vendor trips: selling by profile, the buy list | done | [Tank](guide/samples/tank.md), Vendors |
| Stacking and cramming the pack | done | [Tank](guide/samples/tank.md) |
| Doors, and picking their locks | done | [Tank](guide/samples/tank.md), Doors |
| Getting unstuck | done | [Tank](guide/samples/tank.md) |
| Dying: going back for the corpse, then carrying on, or stopping | done | [Tank](guide/samples/tank.md), Death |
| Fellows: healing them and giving them stamina and mana, the fellow manager's tells, fellows in the player's other clients | done | [Tank](guide/samples/tank.md), Fellows |
| Metas (`.met`): run, and edited in the game | done | [Tank](guide/samples/tank.md), Metas |
| Expressions | done | [Tank](guide/samples/tank.md), Metas |
| `/vt` commands | partial | [Tank](guide/samples/tank.md), Metas: the commands its pages list |
| Settings profiles (`.usd`), imported and exported | done | [Tank](guide/samples/tank.md), Settings |
| Hotkeys, and the remote control of other clients | done | [Tank](guide/samples/tank.md), Commands and the window |
| Recharge handlers of a settings profile (`RechargeHandlerSet`) | missing | its layout in a settings profile is not published |
| Crafting kits, food, ammunition and components | missing | [Tank](guide/samples/tank.md) keeps the options for a run and does not craft: VTank's pages name the recipes only by their results |

The Tank asks a [Buffbot](guide/samples/buffbot.md) for buffs too, which VTank leaves to metas.

## GoArrow

| feature | | in Dereth |
|---|---|---|
| The arrow on a place or an object, with its distance and direction | done | [Navigator](guide/samples/navigator.md) |
| The character's coordinates (its cell in a dungeon) | done | [Navigator](guide/samples/navigator.md) |
| Places searched by name and type: towns, dungeons, portals, lifestones | partial | [Navigator](guide/samples/navigator.md): its own list of towns with approximate coordinates, the portals and lifestones the client knows and has seen, not GoArrow's database |
| The route finder: portals, the character's recalls and walking | done | [Navigator](guide/samples/navigator.md), Ways |
| Saved places | done | [Navigator](guide/samples/navigator.md), Saved |
| Coordinates seen in chat | done | [Navigator](guide/samples/navigator.md), From chat |
| A click on the map points at the spot | done | [Navigator](guide/samples/navigator.md), Map clicks |
| The target marked on the map | done | [Navigator](guide/samples/navigator.md) |
| Dungeon maps | done | [Navigator](guide/samples/navigator.md), Dungeon |

The Navigator also goes there ([Navigator](guide/samples/navigator.md), Go): the journey walked,
through portals and by the character's recalls, where GoArrow only points.

## UtilityBelt

Its plugin's tools and its scripting. The tools are those its documentation lists by name; its
scripting is described, with what is answered and what is refused, in [ubscript.md](ubscript.md).

| feature | | in Dereth |
|---|---|---|
| Lua scripts (UtilityBelt.Scripting's API) | partial | [ubscript.md](ubscript.md): 370 of the 506 members of its script API answered, 136 refused plainly; all 46 of `game.Actions` |
| Script HUDs (ImGui) | partial | [ubscript.md](ubscript.md): 291 of 352 ImGui functions answered, 61 refused |
| The raw client actions (`IClientActionsRaw`) | partial | [ubscript.md](ubscript.md): 41 of 49 answered, 4 in part, 4 refused |
| Messages between clients (`ubnet`) | done | [ubscript.md](ubscript.md), between the clients on one machine |
| Looter | done | [Looter](guide/samples/looter.md) |
| AutoVendor | done | [Inventory](guide/samples/inventory.md); [Tank](guide/samples/tank.md) |
| AutoSalvage | partial | [Tank](guide/samples/tank.md) salvages what its loot profile salvages while it runs, not on a command of its own |
| AutoTrade | missing | |
| AutoImbue | missing | |
| AutoTinker | missing | |
| Chat logger | done | [Chat Filter](guide/samples/chat-filter.md) |
| Counter | partial | [Inventory](guide/samples/inventory.md): counts in every character's log, not counts of a kind said on a command |
| Dungeon maps | partial | [Navigator](guide/samples/navigator.md), Dungeon: the floors at the character's level and the character, not the monsters and items on them |
| Equipment manager | done | [Inventory](guide/samples/inventory.md), Gear |
| Jumper | done | [Tank](guide/samples/tank.md) (`/tank jump`) |
| Nametags | partial | the plugin interface's nametags layer ([overlays.md](overlays.md)); no sample turns it on |
| Quest tracker | partial | UtilityBelt's own FlagTracker script runs as it is ([ubscript.md](ubscript.md)); its Recalls page needs the game's spell table |
| VisualNav | partial | [Navigator](guide/samples/navigator.md) draws a journey on the map and the radar, not a route's lines in the world |
| VTank control | done | [Tank](guide/samples/tank.md) answers VTank's commands and options |
| Dereth time | missing | |
| Door watcher | missing | |
| Giving items by a profile | missing | |

## Not carried

What served the retail client's own window or Decal rather than the player: changes to the retail
client's window, and Decal's plugin management. Dereth's client has windows of its own, and its
plugin panel loads, unloads and reloads plugins ([README.md](README.md)).
