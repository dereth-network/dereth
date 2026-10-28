# Running the game with the samples

**Provenance: design.** How to load Dereth's own plugins and what each shows; the
transcript below is the headless client's own output over a recorded session.

The eleven samples are in `dereth/plugins/` of the repository, one folder per plugin. Every host
can load the whole folder at once: a folder of packages loads each package in it.

## In the desktop client

**From source**, a client built in the repository finds what any client finds, and not the
samples by itself. To load them all at once, at the repository's root:

```text
cargo run -p dereth-client -- --plugins dereth/plugins
```

with the switches you normally start it with: a `--plugins` folder loads at once. Or add
`dereth/plugins` as a folder on this computer on the launcher's Plugins page: each sample in it is
enabled the first time the client finds it. **With the launcher**, install the ones you want from
Dereth's own source on its Plugins page
([install-and-update.md](install-and-update.md#the-launcher-plugin-sources)) and play a Dereth
world as usual: each is on the first time the client finds it. Or copy the sample folders into the
client's plugins folder (`%APPDATA%\Dereth\client\plugins` on Windows; the other systems are in
[install-and-update.md](install-and-update.md#the-desktop-client-its-plugins-folder)), and
enable the ones you want in the plugin panel.

A sample enabled loads at once; nothing is asked. Its windows start closed: open each from the
plugin bar. What a sample does without a click of yours is bounded by the world's
[policy](../policy.md): a world that publishes none allows everything.

Type `/plugin` for the plugin panel: every plugin and its state, with **Enable**, **Disable**,
**Reload** and **Reload All**. What a sample prints, and any error it meets, is in the chat. The
desktop client offers every capability the samples use, so all nine load.

## In the headless client

The headless client plays a recorded session (or a server, with `--connect`) with no window. It
draws nothing but keeps every plugin window's tree, which a script prints with `ui dump`, and it
reads the game's data files from `--dat-dir`.

```text
# samples.script
login long-solo-play +Aldric
replay 120
plugin list
say /xp
say /track
say /chatlog 2
say /nav Holtburg
tick 1
chat
ui dump
```

```text
cargo run -p dereth-headless -- --script samples.script --plugins dereth/plugins --dat-dir <the game's data folder> --captures fixtures/packet-captures
```

`login` replays the recording's login and enters the world as `+Aldric`; `replay 120` carries on
with the recording at its own pace for 120 seconds (the character fights a sparring golem);
`say` types a line in the chat bar; `tick 1` runs one more frame, so the last command's answer is
printed; `chat` prints the chat lines since the last `chat`, each after its chat type number.
An excerpt of what it prints:

```text
plugin network.dereth.autologin 0.2.0 running
plugin network.dereth.chatfilter 0.2.0 running
plugin network.dereth.idnotices 0.2.0 running
plugin network.dereth.looter 0.2.0 running
plugin network.dereth.navigator 0.4.0 running
plugin network.dereth.trackers 0.3.0 running
plugin network.dereth.xpmeter 0.1.0 running
…
chat 5 [ID] Portal: Central Courtyard
chat 5 XP this session: 11 in 1 gains, 1,173 per hour. Watched in all: 11.
chat 0 Dealt 36, taken 3, 1 kills.
chat 5 [combat_enemy] Critical hit! Sparring Golem grazes your foot for 3 points of bludgeoning damage!
chat 5 [combat_self] You stab Sparring Golem for 5 points of piercing damage!
chat 0 [Nav] Pointing at Holtburg (42.2N 33.8E).
…
window network.dereth.trackers hud open=false
  hud #hud anchor="top_right"
    label #line text="36 dealt / 3 taken / 1 kills"
…
window network.dereth.xpmeter main open=false
  window #main title="XP Meter" width=24
    row
      label text="This session:"
      label #earned text="11"
    row
      label text="Rate:"
      label #rate text="1,205 / hour"
    row
      label text="Unassigned:"
      label #unassigned text="11"
    label #level text="989 to next level"
    button #reset text="Reset"
```

Seven of the nine load. **Follow** and **Tank** require the `nav.moveto` capability, which the
headless client offers only with `--world` (a device-free world in which a body stands and moves);
without it they are refused with the reason, and with it all nine load.

Every window starts closed, as in the desktop client; `ui bar <plugin>` opens a plugin's main
window as a click on its plugin bar button does (`ui bar network.dereth.xpmeter`). To click a
window, `ui click <plugin> <window> <node>` (`ui click network.dereth.xpmeter main reset`);
to change a toggle, `ui set <plugin> <window> <node> <json>` (`ui set network.dereth.looter main auto
true`). `plugin console <id>` prints what a plugin printed, and the errors it met.

## What to look for

| sample | try | you should see |
|---|---|---|
| [XP Meter](samples/xp-meter.md) | kill something; `/xp`; **Reset** | the session's experience and hourly rate in its window, and on `/xp` the total it has ever watched |
| [Chat Filter](samples/chat-filter.md) | in its window add a rule hiding `golem`, then fight one; a rule logging `tell` | lines containing the word hidden and counted; your tells on its Log page |
| [Looter](samples/looter.md) | open a corpse, click **Loot**; pick a profile | the items its profile keeps moved to the pack and counted in its window; the profile's rules in plain words |
| [Trackers](samples/trackers.md) | fight; `/track xp`; `/track mana` | damage dealt and taken by creature and element, crits, evades and kills in its window and its HUD; experience and luminance per hour; each worn item's mana and time left |
| [ID Notices](samples/id-notices.md) | open a corpse or chest; walk near a portal; set the value in its window; `/idn watch drudge`, then walk near one | `[ID]` lines for valuable items, rares, named spells and scrolls you do not know; portals, players and the drudge coming into view, the drudge with a toast |
| [Inventory](samples/inventory.md) | log in two characters in turn; `/inv find ring` on the second; appraise a ring; `/inv stack` | both characters' rings listed with where each is; a line of what the ring is; partial stacks merged |
| [Navigator](samples/navigator.md) | type `holt` in Where to?, pick Holtburg, **Go**; `/nav travel portal Drudge Hideout` | the town found as you type, an arrow and markers toward it, the distance on a HUD, and the character going there; a journey into the dungeon through its portal, drawn on the map and radar |
| [Auto-login](samples/autologin.md) | pick your character in its window, log out | the character entered again as soon as the character list is up |
| [Follow](samples/follow.md) | two clients: **Lead** in one, pick the leader and **Follow** in the other | the second character following the first |

Automation needs the world's leave: the Looter's **Auto**, the Tank, Follow walking to an unseen
leader and the Navigator carrying on along a route all act with no gesture behind them, so each needs its `auto.<domain>` permission and a world that allows the
domain ([permissions](../permissions.md#gestures-and-automation)). A click or a typed command of the
plugin's own gives it 30 seconds of activation in which its actions need only `act.interactive`.
