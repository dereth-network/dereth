# Permissions

**Provenance: design.** Generated from the plugin interface's own code, so it cannot drift from it. Do not edit it: `DERETH_TEST_GOLDEN=1 cargo test -p dereth-testkit --test cpu plugins::the_plugin_reference` rewrites it.

Every permission a manifest may name, what it covers and what a plugin needs to do it. A manifest's permissions tell the player what the plugin uses; nothing is asked. [permissions.md](../permissions.md) is the rule behind them: gestures and automation, and the rate ceilings.

| permission | covers | who may |
|---|---|---|
| `read` | Game state, events and the plugin's own UI. Always implied. | every plugin |
| `read.chat.private` | The text of tells to and from the player. | every plugin |
| `messages` | The decoded message stream. | every plugin |
| `chat.print` | Local text into the chat window, not sent anywhere. | every plugin |
| `chat.send` | Speech, tells, channels, emotes and commands sent to the server. | every plugin |
| `chat.filter` | Hiding or restyling incoming lines, and rewriting lines the player types. | every plugin |
| `act.interactive` | Actions that follow a player gesture on the plugin's own surface. | every plugin |
| `auto.loot` | Actions with no gesture behind them, in one domain. The `loot` domain. | every plugin, as the world's policy allows |
| `auto.buff` | Actions with no gesture behind them, in one domain. The `buff` domain. | every plugin, as the world's policy allows |
| `auto.heal` | Actions with no gesture behind them, in one domain. The `heal` domain. | every plugin, as the world's policy allows |
| `auto.combat` | Actions with no gesture behind them, in one domain. The `combat` domain. | every plugin, as the world's policy allows |
| `auto.nav` | Actions with no gesture behind them, in one domain. The `nav` domain. | every plugin, as the world's policy allows |
| `auto.vendor` | Actions with no gesture behind them, in one domain. The `vendor` domain. | every plugin, as the world's policy allows |
| `auto.craft` | Actions with no gesture behind them, in one domain. The `craft` domain. | every plugin, as the world's policy allows |
| `auto.trade` | Actions with no gesture behind them, in one domain. The `trade` domain. | every plugin, as the world's policy allows |
| `auto.social` | Actions with no gesture behind them, in one domain. The `social` domain. | every plugin, as the world's policy allows |
| `auto.support` | Actions with no gesture behind them, in one domain. The `support` domain. | every plugin, as the world's policy allows |
| `session.control` | Selecting a character and logging in or out without the player. | every plugin, as the world's policy allows |
| `peers.local` | Talking to the other clients on the same machine. | every plugin |
| `net.http` | HTTPS to the domains the manifest lists. | a plugin whose manifest names it and lists the domains |
| `plugins.call` | Calling the services other plugins in this client offer. | every plugin |
| `files.import` | Reading a file the player picks. | every plugin, a file the player picks each time |
| `retail_ui` | Moving and hiding the retail interface's panels. | every plugin |

## Automation domains and a world's rules

Whether an action may run is the world's rule for its domain ([policy.md](../policy.md)): some rules ask for a gesture behind it. The rules:

| rule | allows |
|---|---|
| `allow` | actions with or without a gesture |
| `interactive-only` | actions only with a live activation (a gesture on the plugin's own surface) |
| `deny` | no actions in the domain |

Each domain's rule in a world that publishes no policy, and in a published policy that does not name the domain. `session` is logging in, choosing a character and logging out without the player (the `session.control` permission).

| domain | permission | no policy published | published, not named |
|---|---|---|---|
| `loot` | `auto.loot` | `allow` | `deny` |
| `buff` | `auto.buff` | `allow` | `deny` |
| `heal` | `auto.heal` | `allow` | `deny` |
| `combat` | `auto.combat` | `allow` | `deny` |
| `nav` | `auto.nav` | `allow` | `deny` |
| `vendor` | `auto.vendor` | `allow` | `deny` |
| `craft` | `auto.craft` | `allow` | `deny` |
| `trade` | `auto.trade` | `allow` | `deny` |
| `social` | `auto.social` | `allow` | `deny` |
| `support` | `auto.support` | `allow` | `deny` |
| `session` | `session.control` | `allow` | `deny` |
