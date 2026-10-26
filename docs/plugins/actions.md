# Actions

**Provenance: design.** The plugin interface is this client's own addition, not the original's; every statement here is what a host must do, and the conformance suite pins it.

An action is a plugin asking the game to do something. Each becomes the requests the game's own
interface makes for the same gesture, so it passes the same checks and sends the same messages as
the player's click would; a plugin has no other way to send anything. The functions are listed in
[api.md](api.md#actions); who may call them, and when, in
[permissions.md](permissions.md#gestures-and-automation).

## The queue

Every action answers a promise at once and waits its turn. The host holds one queue for every
plugin, in the order the actions were asked for, and sends the next only when:

- nothing a plugin asked for is still outstanding;
- for an item action, the client is not busy with an item (the interface's own rule: one item
  request at a time);
- for a use or a cast, no use or cast is awaiting its acknowledgement;
- for a use, a fifth of a second has passed since the last (the interface's own pace for uses);
- for an appraisal, the plugin has asked for fewer than five in the last second.

An action that waits more than 30 seconds for its turn fails as `busy`. A plugin may have at most
256 actions queued. Unloading a plugin drops its queued actions.

## How an action ends

Each action has a completion signal, read from what the client applied, and a timeout counted from
when it was sent. The promise settles with the first of them, in the frame in which the client
applied what settled it, after that frame's state events.

| failure | meaning |
|---|---|
| `timeout` | no answer within the action's time |
| `busy` | it waited more than 30 seconds for its turn |
| `refused (0x…)` | the server refused it; the number is the reason it gave |
| `failed (0x…)` | a use or a cast was acknowledged as failed, with the reason |
| `refused by the client` | the client's own checks stopped it: nothing was sent (a use while busy, an item already moving, a spell the character cannot cast) |
| `target_gone` | an attack's target stopped being the selection before the attack was answered: its swing was dropped, or cancelled if it had gone out, and never lands on what is selected now ([automation](automation.md)) |
| `denied_by_world` | the world does not allow the action's automation domain without a gesture |
| `unavailable` | the host has no way to carry it out, so nothing was sent. `unavailable (why)` is one the host cannot carry out just now |

**Which refusal is an action's.** A server refusal names no request: `Communication_WeenieError`
(`0x028A`) and its form with a string (`0x028B`) carry only a reason, and
`Character_ServerSaysAttemptFailed` (`0x00A0`) and `Trade_TradeFailure` (`0x0207`) only the object
they are about. So an action takes a refusal as its own by what it is about: one naming an object
settles only an action on that object (a buy or a sale also takes one naming the vendor or the
character, the server's refusal of a whole transaction, whose reason comes with its
acknowledgement), and one of something the character said (a tell to someone who is not there, a channel, a chat
emote, a message blocked by a squelch, a gag, the notice that someone told is away) settles no
action, whatever is outstanding when it arrives. Any other refusal settles the action outstanding.
Movements, pre-game operations and HTTPS requests are settled by their own signals.

| action | sends | settles when | timeout | domain |
|---|---|---|---|---|
| `act.select(id)` | nothing (a local selection) | the object is selected | 2 s | none |
| `act.appraise(id)` | an appraisal request, without opening the examination window | the appraisal arrives; the value is the appraisal | 5 s | none |
| `act.use(id)` | a use | the use is acknowledged; or, for an item the client moves itself (into the pack, onto the body), when it has moved | 10 s | `loot` for a container or corpse, `nav` otherwise |
| `act.use_on(item, target [, opts])` | one use-with-target request, as the interface for plugins sends it: `target` taken for the use (the player's selection left as it is), and an item the character wields used as a targeted use's second click, so it stays in hand and its own spell is cast; an item used on what it does not take sends nothing (`refused by the client`) | the use is acknowledged | 20 s (a healing kit used in a fighting stance takes some ten seconds) | `heal` for a healing kit, or a wielded item's spell with `purpose = "heal"`; a wielded item's spell `buff` on oneself or what one holds, on anyone else `support` when the spell helps (the spell table marks it beneficial) and `combat` when not; `craft` otherwise |
| `items.move(id [, container [, slot]])` | a put-in-container | the item is in the container, or the client's item request is answered | 20 s (the server walks to an item in the world and waits while the character is busy) | `loot` from an open corpse or chest, `craft` otherwise |
| `items.split(id, count [, container])` | a split into a container | the stack has shrunk | 10 s | as `items.move` |
| `items.merge(id, target [, count])` | a merge | the stack has shrunk or gone | 10 s | as `items.move` |
| `items.drop(id)` | a drop | the item is on the ground or gone | 10 s | `craft` |
| `items.give(id, target [, count])` | a give | the item has left, or its stack has shrunk | 10 s | `trade` |
| `items.wield(id)` | a wield (or wear) where the item goes | the player wields it | 10 s | `combat` for a weapon, caster or ammunition; else `craft` |
| `items.unwield(id)` | a put-in-container to the main pack | as `items.move` | 10 s | as `items.wield` |
| `items.inscribe(id, text)` | an inscription | at once | 2 s | `craft` |
| `items.salvage(ids [, tool])` | the salvage of the items, with the pack's salvaging tool when none is named | every item has left the pack (salvaged into bags), or the use is acknowledged | 10 s | `craft` |
| `vendor.buy{ … }` | the items added to the buy list (a stackable one given first the stack size the vendor's list gives it when it shows the stock, so the count asked for is bought), then a buy of the list | the vendor answers | 10 s | `vendor` |
| `vendor.sell(ids)` | the items added to the sell list, then a sale of the list | the vendor answers | 10 s | `vendor` |
| `vendor.close()` | nothing (the window closes) | the vendor is closed | 2 s | `vendor` |
| `vendor.fill_components([max_price])` | nothing (the buy list is filled) | at once | 2 s | `vendor` |
| `trade.open(player)` | a use of the player, which opens a trade | the trade is open | 20 s | `trade` |
| `trade.add(id)` | an add to the trade | the item is on the player's side | 10 s | `trade` |
| `trade.accept()`, `decline()`, `reset()`, `close()` | the matching trade message | the trade shows it | 10 s | `trade` |
| `social.fellowship_create(name [, share_xp])` and the other fellowship actions | the matching fellowship message | the fellowship shows it | 10 s | `social` |
| `social.friend_add(name)`, `friend_remove(id)` | the matching friends message | the friends list shows it | 10 s | `social` |
| `character.raise_skill(id)`, `raise_attribute(id)`, `raise_vital(id)` | a raise by one, at its experience cost | the level has risen | 10 s | only with a gesture |
| `character.set_option(name, value)` | the option change (`name` in snake case: `view_combat_target`) | the option reads the value | 2 s | only with a gesture |
| `combat.set_mode(mode)` | the combat toggle, when the mode differs | the character is in (or out of) a combat mode | 5 s | `combat` |
| `combat.attack(target [, opts])` | the target's selection, then the combat window's power, height and release | a swing ended or begun, or the attack sequence's end with no refusal ([automation](automation.md)) | 20 s | `combat` |
| `magic.cast(spell [, target])` | a selection of the target, then the cast | the cast is acknowledged | 10 s | `buff` on oneself or what one holds; on anyone else `support` when the spell helps (the spell table marks it beneficial: a buff, a protection) and `combat` when not; `heal` asked for as a heal |

A spell is cast in magic stance only, as the spell bar is only there in it: a cast asked for in
any other stance fails `not_in_magic_mode` and nothing is sent (a server lets such a cast end with
nothing cast). `fight.ready_magic()` ([automation](automation.md)) takes up a caster and enters
the stance.

A server's refusal of anything the player asked for fails the action outstanding at the time.
