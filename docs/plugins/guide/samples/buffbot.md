# Sample: Buffbot

**Provenance: design.** A walk-through of a sample plugin that ships with Dereth; its source is
[`dereth/plugins/buffbot/`](../../../../dereth/plugins/buffbot/).

| | |
|---|---|
| id | `network.dereth.buffbot`, version 0.2.0, API 0.7 |
| files | `plugin.toml`, `main.lua` (its window and commands), `bot.lua` (the line, the casting, mana and components), `restock.lua` (buying them again), `selfbuffs.lua` (its own buffs) |
| contexts | `world` |
| permissions | `read.chat.private` (the requests' tells), `chat.send` (its answers), `act.interactive`, `auto.support` (the buffs it casts on others), `auto.buff` (its own buffs, and Stamina to Mana on itself), `auto.nav` (the walk to its vendors and back, a mana potion drunk), `auto.vendor` (what it restocks) |
| capabilities | `ui.window` required |

## What it does

A buffbot: a character that buffs other players who ask. A player tells it the name of one of its
**buff profiles**, and the profile's spells are cast on them.

- **Tells it answers.** `help` (what to tell it), `list` (each profile's spells, as it casts them
  now), `cancel` (leave the line) and a profile's name (its buffs). The profiles a new buffbot has
  are `buffs` (the attributes, Invulnerability, Heavy Weapon Mastery, Armor and Regeneration),
  `melee`, `missile` and `mage`.
- **The line.** One player at a time, in the order they asked. A player who asks while it is busy
  is told their place ("I am busy; you are number 2 in line."), and told it again if they ask
  again. At most `QueueMax` players wait (more are told the line is full), and a player may ask at
  most `PerPlayerCap` times an hour.
- **Range.** A player further than `Range` meters when their turn comes is told to come within
  that distance and waited for `ComeCloserSeconds`; one who does not come is told so and skipped.
- **The spells.** Each family of the profile ("Strength Other") is cast at the highest level the
  character knows, at most `MaxLevel`, whose difficulty its skill in the spell's school beats by
  `SkillMargin`. An Incantation ("Incantation of Strength Other") is its family's level 8, and a
  level above VI with a name of its own ("Might of the Lugians", Strength Self's level 7) is found
  by the spell table: the spell of the family's category above its numbered levels whose
  description reads as theirs do. A fizzle casts the same spell again, `Retries` tries at most; a spell that cannot
  be cast (a component missing, out of reach) is left, and the player is told which. When the
  profile is done the player is told "Done: 8 buffs cast on you."
- **Its own mana.** Before a spell it has too little mana for, and between players when its mana is
  under `ManaRest` percent, it gets mana back: Stamina to Mana Self while its stamina allows, else
  a mana potion from its pack, else a rest in peace mode, until `ManaReady` percent (two minutes at
  most between players). The player waiting is told it is getting its mana back.
- **Components.** Each spell component it carries that it is using up and has fewer than
  `ComponentWarn` of is warned of once, in the log and the chat; one it runs out of is said too.
- **Restocking.** What `Restock` lists, each an item, how many to keep and the vendor that sells
  it (`Prismatic Taper=200@Archmage Cindrue, Mana Potion=20@Thelnoth Cort the Healer`, Holtburg's
  sellers of both), it buys again when it holds fewer than `RestockAt` percent of the count.
  Between players, with the line empty (or with players waiting once something has run out), it
  walks to each vendor it needs, buys each item up to its count (no more than its pyreals pay
  for), and walks back to its post, facing the way it faced there. A player who asks meanwhile is
  told "I am buying components and will be back shortly; you are number 1 in line." and is buffed
  once it is back. A trip that bought nothing is not tried again for ten minutes.
- **Its own buffs** (in the manner of Alinco's buffing plugin). A **self profile** names the
  character's own buffs by family: buffs cast on the character, and item spells cast on its gear:
  Blood Drinker, Heart Seeker and Swift Killer on the weapon it wields, Spirit Drinker and Hermetic
  Link on its caster, Defender on either (the weapon it put away to take up its caster counts), and
  Impenetrability and the banes on each piece of armor it wears. The self profiles a new buffbot
  has are `mage`, `melee` and `missile`. `/buffbot self <profile>` (or **Buff now**) casts each of
  the profile's buffs that is missing, has less than `SelfRebuffMinutes` left (five by default), or
  is on at a lower level than the one it would be cast at, in the profile's order, at the level
  chosen as for others; the profile becomes `SelfProfile`. With `SelfBuff` on, the `SelfProfile`'s
  buffs are kept up so, a look every five seconds, between players and whether or not it answers
  requests. A buff on the character is read from its enchantments; one on an item from when it was
  cast there, kept with the character by the character's age, so time logged out does not count.
  It casts them in the magic stance taken **for buffing** (the `buff` domain). A fizzle casts the
  spell again (`Retries` tries); a spell that cannot be cast is left ten minutes, and said.
- **Its post.** Where it stood when it was first turned on (or where `/buffbot post` was typed,
  or **Keep this place as its post** clicked), kept with the character. Between players, standing
  more than 5 meters from it (a connection lost on the way to a vendor, a death), it walks back.

It casts in the magic stance, which it enters with its caster (`Caster`, else the first in its
pack) **for support**: the stance and the caster are taken for the `support` domain, not `combat`,
so a world that forbids unattended fighting and allows support (see
[the policy](../../policy.md)) lets a buffbot run. Its settings, profiles and whether it answers
are kept with the character, so a client that logs in again carries on.

Its **window** holds everything: the **Run** page (answering on or off, what it is doing, the
counts, the line, its log, and keeping where it stands as its post), **Profiles** (each profile and its spells; a profile chosen fills
its name and spells, which **Save** and **Remove** change; **Read the spell book again** after
learning spells), **Self** (the self profile chosen, whether its buffs are kept up and the margin,
**Buff now** and **Stop**, each buff with the time left on it, and the self profiles, which **Save**
and **Remove** change), **Settings** (each setting above) and **Components** (each component held). The
commands are shortcuts:

| command | does |
|---|---|
| `/buffbot on`, `/buffbot off` | answers requests, or stops |
| `/buffbot status` | what it is doing, the counts and the line |
| `/buffbot profiles` | each profile and the spells it casts now |
| `/buffbot profile <name> <family>, <family>, ...` | sets a profile (with no families, removes it) |
| `/buffbot set <setting> <value>`, `/buffbot settings` | a setting, and all of them |
| `/buffbot components` | the components held |
| `/buffbot restock` | buys what `Restock` lists now, between players, and comes back |
| `/buffbot post` | its post is where it stands |
| `/buffbot clear` | empties the line |
| `/buffbot self [profile]`, `/buffbot self stop` | casts your own buffs now by a self profile (else `SelfProfile`'s), or stops |
| `/buffbot selfprofiles` | each self profile, what it casts now and the time left on each |
| `/buffbot selfprofile <name> <family>, <family>, ...` | sets a self profile (with no families, removes it) |

## Running it on its own

A buffbot is meant to be left running. The headless client runs it with no window at all, as a
daemon that stays in the world and logs in again after a lost connection (a server restarted):

```text
# buffbot.script: each connection's setup
enter Buffbel
run 5
plugin load plugins/buffbot
run 3
say /buffbot on
```

```text
dereth-headless --daemon --connect play.example.org:9000 --account mybuffbot \
  --password-file buffbot-password.txt --world --plugin-store buffbot-store \
  --log-file buffbot.log --script buffbot.script
```

The log file holds every request, cast, fizzle and failure the plugin notes, with the time, and the
chat. The headless client's README says more of the daemon.

## Asking it

The [Tank](tank.md) asks a buffbot for buffs on its own (its Buffs page: the buffbot's name, the
tell, and when to go), and understands its answers: a place in line, "come within", "Done", and its
refusals. A player asks by a tell: `/t Buffbel, buffs`.

## The source

`bot.lua` keeps the line and works it in one task. A tell is read from the chat line (the
teller's name and, where the line names it, their object id), and the word decides:

```lua
function B.chat_line(e)
	if e.kind ~= "tell" then
		return
	end
	local name, id, said = B.parse_tell(e.text)
	if name ~= nil and not same(name, game.player.name) then
		B.heard(name, id, said)
	end
end
```

Answers go out through one queue a little over a second apart, as the game takes a few lines a
second from a plugin. A spell is cast on the player's object with `magic.cast(spell, target)`;
the host files a beneficial spell cast on another character under `support`, and the failure a cast
ends with says what to do next: `0x0402` (a fizzle) casts it again, `0x0401` (too little mana) gets
mana first, `0x0550` (out of range) asks the player to come closer.
