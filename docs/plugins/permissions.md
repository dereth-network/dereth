# Permissions

**Provenance: design.** The plugin interface is this client's own addition, not the original's; every statement here is what a host must do, and the conformance suite pins it.

What a plugin may do. A manifest names the permissions the plugin uses, each with the reason the
player is shown; they tell the player what the plugin does, and nothing is asked. Every plugin may
do what every permission covers, whether its manifest names it or not, with two exceptions: a
request over the network needs `net.http` named and its domain listed, and an action needs the
world's leave for its domain.

| permission | covers | who may |
|---|---|---|
| `read` | the game's state, events and the plugin's own windows | every plugin |
| `read.chat.private` | the text of tells to and from the player | every plugin |
| `messages` | the decoded message stream | every plugin |
| `chat.print` | local lines in the chat window, not sent anywhere | every plugin |
| `chat.send` | speech, tells, channels, emotes and commands sent to the server | every plugin |
| `chat.filter` | hiding incoming lines; rewriting lines the player types | every plugin |
| `act.interactive` | actions that follow a player's gesture on the plugin's own window, command or key | every plugin |
| `auto.loot`, `auto.buff`, `auto.heal`, `auto.combat`, `auto.nav`, `auto.vendor`, `auto.craft`, `auto.trade`, `auto.social`, `auto.support` | actions with no gesture behind them, one domain each | every plugin, as the world's policy allows |
| `session.control` | selecting a character and logging in or out without the player | every plugin, as the world's policy allows |
| `peers.local` | talking to the other clients on the same machine | every plugin |
| `net.http` | HTTPS to the listed domains | a plugin whose manifest names it and lists the domains |
| `plugins.call` | calling the services other plugins in this client offer ([services.md](services.md)) | every plugin |
| `files.import` | a file the player picks | every plugin, a file the player picks each time |
| `retail_ui` | moving and hiding the game's own panels | every plugin |

The launcher shows the permissions a manifest names, as information, and a server's plugin audit
is told them ([audit.md](audit.md)). `permissions.has(name)` answers whether the plugin may use what a
permission covers: true for each of them but `net.http`, which is true only when the manifest
names it.

## Requests over the network

A request goes out only when the manifest both names `net.http` and lists the request's domain
(its subdomains too, at the port listed, 443 when none is) under `[net] domains`; otherwise the
request's promise fails, saying which is missing. Requests are HTTPS only, never to an address.

## A file the player picks

A file a plugin asks for (`files.import`) is a dialog the host puts, drawn as a dialog of the
interface: who asks and why, the files in the import folder that the request takes, one of them
picked, and *Give it* or *Refuse*. *Give it* gives the file picked, and cannot be pressed until one
is; *Refuse* or Escape refuses. The plugins' windows stay drawn and take their presses beside it.
Each dialog carries its own id, and an answer reaches only the dialog it was given to. A plugin has
one file request at a time (another while one waits is turned away); after the player refuses one,
its next waits 30 seconds, doubling with each refusal in the session; and after five refusals it
is asked for no more files until it is reloaded. What a plugin wrote that the dialog shows (its
name, the request's prompt) is put on one line, rid of control characters and cut to its length (a
name 48 characters, a prompt 200), and a dialog taller than the screen keeps its buttons on it, its
words scrolling. The headless client has no player to pick one: a request for a file waits there,
unanswered.

The host's own window, the plugin panel (`/plugin`), is drawn as a window of the interface it
stands in, above every plugin window. What a plugin wrote that it shows (the plugin's name, the
lines of its console, the error it faulted on) is shown as the plugin's words: on one line, cut to
its length, between double quotes and in a colour of its own.

## Gestures and automation

An action is **interactive** when a player gesture on the plugin's own surface started it: a click
or a change on its window or HUD, a key bound to one of its commands, or a command it registered
typed in the chat bar (a line a plugin sends itself, `chat.send`, is never taken for a command,
whatever its spelling). The gesture gives the plugin an **activation** that lasts 30 seconds of the
frame clock and covers every action the plugin starts in that time, so one click can run a whole
sequence. A plugin cannot make an activation; the host keeps them (`permissions.activation()`
reads what is left). Any other action is **automated**.

- The world's rule for an action's domain says whether it runs: with or without a gesture, only
  with one, or not at all. An action the rule refuses fails its promise with `denied_by_world`. A
  world that publishes no policy allows everything.
- A character's advancement and options (`character.*`) are never automated: without a live
  activation the call is an error.
- A selection and an appraisal need nothing: they are reads with a request behind them, held to
  the rate ceilings below.

The domain of an action is its kind: taking from an open corpse or chest is `loot`; using a
container or corpse is `loot`, using anything else `nav`; using a healing kit on someone `heal`;
a wielded item's own spell, used on a target, as the spell cast would be; other uses of one item
on another, and moving, splitting, merging, dropping, wielding (but a weapon, a caster or
ammunition taken up or put away is `combat`), inscribing and salvaging one's own items, `craft`;
giving and trading `trade`; buying and selling `vendor`; fellowships and friends `social`; a
movement of the character ([nav.md](nav.md)) `nav`. Choosing a character and logging in or out
([pregame.md](pregame.md)) is `session`. A spell is:

- `heal` when it is asked for as a heal (a Life spell, `purpose = "heal"`);
- `buff` on oneself, or on an item one holds or wields;
- `support` on another character when it helps: a spell the spell table marks beneficial (a buff,
  a protection), so a world can allow a buffbot and forbid unattended fighting;
- `combat` on anyone else.

## A world's policy

A world may publish which automation domains it allows: each `allow` (with or without a gesture),
`interactive-only` (only with one) or `deny` (not at all). A world that publishes none allows every
domain and `session`, with or without a gesture. [policy.md](policy.md) is the document and where
it comes from.

## Rate ceilings

Every plugin: at most one pending interactive request at a time, following the client's own busy
state; appraisals at most 5 per second; chat sent at most once per second with a burst of 3; local
lines printed at most 50 per frame.
