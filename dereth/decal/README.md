# Dereth for Decal

> **Very experimental. Not recommended for general use.** Nothing builds or releases it: it is
> built by hand from this folder, and it may change or go away.

**Dereth Plugins** is a Decal plugin that hosts Dereth's plugins inside the retail client: the same
plugins the Dereth client runs, in Decal windows.

For the Dereth Network window in Decal, see [`network/decal-companion`](../../network/decal-companion/README.md).

It doesn't read or change the game's memory. Everything it does goes through Decal's own calls, the
ones every Decal plugin uses: it types a command, uses an item, casts a spell, turns the character,
shows a window. If you let it, it also presses the game's own keys, as you would.

## What you need

- The retail client with Decal (2.9.8, which installs to a `Decal 3.0` folder), on Windows.
- .NET Framework 4.8, which Windows 10 and 11 already have.
- Nothing else. Virindi View Service is not needed: the windows are Decal's own.

## Installing

It comes as a folder holding `Dereth.Decal.dll`, `dereth_decal_host.dll` and a `plugins` folder.
With the game closed, in a PowerShell opened as administrator (Decal keeps its list of plugins in the
machine's registry):

```powershell
powershell -ExecutionPolicy Bypass -File install.ps1 -From <the plugin's folder>
```

The script is `dereth\decal\plugin\install\install.ps1`. It installs to `C:\Games\Dereth\Decal`
(`-To` names another folder), and `uninstall.ps1` beside it takes it out again. It adds two entries
to Decal: a network filter, which hears the game from the character list on, and a plugin, which
shows the windows. Both must be on in Decal's list.

## Dereth Plugins

**Your plugins** go in the Dereth client's plugins folder, `%APPDATA%\Dereth\client\plugins`,
one folder (or `.zip`) each: a plugin put there once is found by both clients. The plugins you
installed or added on the launcher's Plugins page are found here too, each on the first time Dereth
Plugins finds it, and so are the ones that come with Dereth Plugins. Type `/plugin` in chat to open
the plugin panel: every plugin found is listed, and one that is off (yours and the ones that come
with Dereth Plugins, until you enable each) loads when you pick it and click Enable. One you
disable stays off. Its windows stay closed until you open them from Decal's bar. What the plugins
save is kept apart from the Dereth client's, in `%APPDATA%\Dereth\decal`.

A `settings.txt` in `%APPDATA%\Dereth\decal` can:

- find plugins in another folder instead of the client's folder and the launcher's plugins:
  `plugins = D:\my plugins` (each more such line adds one). To keep retail's plugins apart from
  the Dereth client's, name a folder of their own, such as
  `plugins = C:\Users\<you>\AppData\Roaming\Dereth\decal\plugins`;
- let plugins use the game's keys: `keys = on` (below);
- give your world's plugin policy: `policy = <file>`;
- turn off the link to the Dereth clients on the same machine: `peers = off`.

Restart the game after changing it.

**What works.** Plugins hear what they hear in the Dereth client: chat, every object, your
character, inventory, vendors, trades, fellowships, the message stream. Their windows and HUDs are
Decal windows on Decal's bar. Their markers are Decal's 3-D marks: text over an object or a place,
an arrow above it, a ring on the ground, and the arrow over your head that points the way. Their
commands work in the chat line, and they can hide chat lines.

**What retail cannot do.** Decal has no call for these, so they do not happen in retail and nothing
is sent; a plugin that asks is told they are unavailable:

| a plugin asks to | in retail |
|---|---|
| attack (melee or missile) | only with the game's keys on (below); spells work either way |
| split or merge a stack, inscribe an item | not possible |
| make a fellowship | not possible; recruiting, dismissing, leading, opening and leaving work |
| set a game option, close a vendor | not possible |
| jump, walk, strafe or hold a movement key | only with the game's keys on; otherwise a plugin moves you only by turning you and setting the run lock |
| plan a route or travel through portals | not offered: a plugin walks you in a straight line |
| choose a character at the list | not possible: the list is read, logging off works |
| draw radar blips, map marks, lines or squares in the world | not drawn |
| show a picture, a drawing or a chart | a line saying it is not shown |

**The game's keys.** Off unless you turn them on: tick **Let plugins use the game's keys** in the
plugin panel (it writes `keys = on` to `settings.txt`). Then a plugin's attacks, jumps and held
movement press the game's own keys, the ones in your key map or else the game's defaults (attacks
on Page Down, End and Delete, jump on Space, W, X, Z, C, A and D to move). A chat line names the
keys when you turn it on.

- An attack selects its target, then holds the attack key until the power bar reaches the power
  the plugin asked for. If your combat panel's power is set higher, the game swings at that.
- Keys go to the game only while its window is in front and the chat line is closed. Otherwise
  the plugin is told `unavailable (the game window is not in front)` or `unavailable (the chat line
  is open)`, and nothing is pressed.
- Any key you press lets go of every key a plugin holds and stops its movement. So does switching to
  another window or opening the chat line.
- Stopping an attack lets go of its key. If the attack selected its target, the selection is
  cleared too, which ends the swings the game repeats by itself. A target you picked stays selected.
- A jump a plugin stops while it charges still leaps: the game cannot put a charged jump down.

**Moving.** When a plugin moves you, any key you press in the game (with the chat line closed)
stops it and lets go of the run lock.

### Trackers, Navigator and Tank in retail

| plugin | works | does not |
|---|---|---|
| Trackers | everything: kills, damage, experience, items, corpses, mana, players, its HUD, the arrow to your corpse | |
| Navigator | finding a place, coordinates in chat, the arrow, walking you there in a straight line | routes around walls, portals and recalls on the way, map marks, its map drawing |
| Tank | buffing, healing, war and void spells, looting, buying and selling at vendors, fellows, straight routes; with the game's keys on, melee and missile attacks, jumps and side steps | closing a vendor, merging stacks, planned paths; attacks, jumps and side steps with the game's keys off |
| Looter, Buffbot, Chat filter, ID notices, XP meter | all of it | |
| Follow | following your other characters in the Dereth client or in retail on this machine | |
| Auto-login | showing the character list | logging a character in |

## How it differs from the Dereth client

- Windows look like Decal's, with Decal's buttons, boxes, lists and sliders. A number field is a
  value between `-` and `+` buttons. HUDs are Decal windows too, with a frame.
- Messages a plugin pops up go to the chat window.
- A hotkey a plugin offers is a button; Decal binds no key to it.
- Plugin movement is coarser, as above: turning and the run lock only, unless the game's keys are on.

## Building and testing

The host library is the `dereth-decal-host` crate, built 32-bit as the game is; the plugin is
.NET Framework 4.8 and compiles against your Decal install:

```sh
rustup target add i686-pc-windows-msvc
cargo build --release -p dereth-decal-host --target i686-pc-windows-msvc
dotnet build dereth/decal/plugin/Dereth.Decal/Dereth.Decal.csproj -c Release -p:DecalDirectory="C:\Games\Decal 3.0"
cargo test -p dereth-decal-host --profile test-release --target i686-pc-windows-msvc
dotnet test dereth/decal/plugin/tests/Dereth.Decal.Tests.csproj
```

The build's output folder (`target/decal-plugin/bin/Dereth.Decal/release`) is the folder to
install. The tests need no Decal and no game. The crate's tests replay the recorded sessions in
`fixtures/` through the library's C interface: a plugin hears each session as it does in the
Dereth client's own test driver, its actions come out as the Decal calls that send them, and the
conformance cases this host can run pass. The plugin's tests drive the same library from C#, with
stand-ins for Decal's calls, windows and 3-D marks and for the game's window the keys go to.

## Trying it by hand

1. Build and install it, start the game through Decal, and check Dereth Plugins is on in Decal's
   list (twice: the network filter and the plugin).
2. Log in. Type `/plugin`: the plugin panel opens as a Decal window. Enable Trackers, and open
   its window from Decal's bar.
3. Kill something: Trackers' window counts it. Type `/track corpse` after a death: the arrow
   over your head points to your corpse.
4. Enable Navigator, open it from Decal's bar, type a town's name, and pick it: you turn toward it
   and run. Press any key: you stop.
5. Enable Tank with a mage profile near some monsters: it buffs, casts at them and loots. With a
   melee profile its attacks fail: Tank's lines in the chat say `unavailable`.
6. Enable Chat filter and hide a kind of line: those lines stop showing.
7. Log out: every plugin window closes, and the plugin panel. Log in again: the plugins are back,
   and their windows open again from Decal's bar.

The game's keys:

8. In the plugin panel, tick **Let plugins use the game's keys**. A chat line names the keys
   plugins will use: check they are your attack and jump keys. `%APPDATA%\Dereth\decal\settings.txt`
   now has `keys = on`. The key map is the one the game saved when it last closed, so a key you
   rebind shows after a restart.
9. With a melee profile and the game's window in front, enable Tank near a monster: it selects the
    monster and swings, the power bar filling to the profile's power before each swing. Stop Tank
    mid-fight: the monster is no longer selected and the swings stop. Select a monster yourself,
    let Tank attack it and stop Tank: your selection stays.
10. Click another window while it fights: its attacks stop, and Tank's lines in the chat say
    `unavailable (the game window is not in front)`. Click back into the game: it fights again. Open
    the chat line: `unavailable (the chat line is open)`.
11. Enable Navigator and walk somewhere, or let Tank run a route with a jump in it, and press any
    movement key: it stops at once and nothing stays held.
12. Untick the switch: `settings.txt` says `keys = off`, and Tank's melee attacks fail as
    `unavailable` again.

One plugins folder:

13. Put a plugin's folder in `%APPDATA%\Dereth\client\plugins` and start the Dereth client: it is
    in the plugin panel. Start the game through Decal: it is in this plugin panel too, and its saved
    settings in each client are its own.
14. Install a plugin on the launcher's Plugins page, then start the game through Decal: it is in
    the plugin panel. Remove it in the launcher and restart the game: it is gone.
16. Add `plugins = <another folder>` to `settings.txt` and restart the game: the plugin panel lists
    that folder's plugins instead, and none of the launcher's.
