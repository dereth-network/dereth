# Sample: Tank

**Provenance: design.** A walk-through of a sample plugin that ships with Dereth; its source is
[`dereth/plugins/tank/`](../../../../dereth/plugins/tank/). What it takes from VTank is what VTank's
public documentation says VTank does; each such statement names its page by a key in brackets
(listed at the end, under *VTank's documentation*).

| | |
|---|---|
| id | `network.dereth.tank`, version 0.3.0, API 0.7 |
| files | `plugin.toml`, `main.lua` (commands, events), `engine.lua` (the macro), `buffbot.lua` (buffs asked of a buffbot), `settings.lua` (the settings), `monsters.lua` (the monster list), `names.lua` (the game's species and materials), `spells.lua` (spells by family), `buffs.lua`, `profile.lua` (VTank `.usd` settings profiles), `metas.lua` (VTank `.met` metas and VTank's `/vt` commands), `metaedit.lua` (the meta editor), `hotkeys.lua` (its hotkeys), `recharge.lua` (vitals and worn mana), `vendor.lua` (trading at vendors, and the trip to the vendor and back), `pack.lua` (salvaging, salvage bags combined, stacking and cramming), `fellows.lua` (helping fellows, its own fellow on the machine, running a fellowship), `pets.lua` (summoning), `window.lua` (its window), `routes.lua` (routes made in the game, and written as VTank files), `quest.lua` (quest steps: an item handed over, picked up or used on something), `routes/` (VTank `.nav` routes), `profiles/` (VTank `.utl` loot profiles), `metas/` (VTank `.met` metas: the Training Academy profile and the litmus profile) |
| contexts | `world` |
| permissions | `act.interactive`, `auto.combat`, `auto.nav`, `auto.loot`, `auto.heal`, `auto.buff` (buffs and the recall spell), `auto.craft` (worn items' mana, salvaging, stacking and cramming), `auto.vendor` (selling and buying at vendors), `auto.trade` (a quest step's item handed to a person), `auto.social` (recruiting into the fellowship it leads), `auto.support` (helper buffs on other players), `chat.send` (a route's chat waypoints, the fellowship manager's tells), `messages` (the server's word on where the character stands, for checkpoints), `peers.local` (its own fellow on the machine) |
| capabilities | `nav.moveto` and `ui.window` required; `nav.journey`, `nav.path`, `messages`, `peers.local` and `ui.hud` (the remote control) optional |

## What it does

An unattended macro in the manner of VTank, for the way most players used it: leave the character
on a route through a dungeon (or outdoors), fighting what it meets, looting its kills by a loot
profile, and levelling. Started, it loops a few times a second, each time doing one short step of
the most pressing thing:

1. waits out portal space and death;
2. stops, or recalls and stops, when told to: a full pack (`OnFullPack`; or the trip to its
   vendor and back, below), too many deaths (`StopAfterDeaths`), a time limit
   (`StopAfterMinutes`); and carries out the vendor trip under way;
3. heals when health is under `Recharge-Norm-HitP`, then dispels a heavy vulnerability
   (`UseDispelItems`, `CastDispelSelf`, under *Vitals*), then helps a fellow in need (`DoHelp`,
   under *Fellows*);
4. casts the buffs of a Force Buff under way (below), carries out the vendor trip and the buffbot
   trip under way (below), goes back to the corpse a death left (below), and, far from its route
   (back from a death, or a client come back in town), travels back to it;
5. with a monster about, gets stamina and mana back under `Recharge-Norm-Stam` and `-Mana` (see
   *Vitals* below);
6. summons a pet when enough monsters gather (`SummonPets`, below), then fights a monster within
   `AttackDistance` and in a straight line of sight, or walks to one within `ApproachDistance`
   (with navigation on) along the client's route to it; the last steps of a melee attack are the
   server's own;
7. loots the corpses of its own kills (and, when told, others') within
   `CorpseApproachRange-Max`, taking what the loot profile's rules take, and salvages what its
   Salvage rules took before the next corpse;
8. with nothing in range, tops health, stamina and mana up to `Recharge-NoTarg-HitP`, `-Stam`
   and `-Mana` with what it has, keeps its buffs up (its buff items too), casts its helper buffs,
   refills worn items' mana, keeps its pack (salvage bags combined, stacks merged, the main pack
   crammed into the side packs), and otherwise rests in peace mode;
9. walks the route, a leg at a time, or follows its leader (a follow route); a leg stops as soon
   as a monster, a corpse or low health wants attention.

**Start, Pause and Stop.** VTank's Run Macro switch works as a pause [std]: so does the Tank's
**Pause** (`/tank pause`, and a meta's `/vt stop`): the macro does nothing, the plugin bar says it
is paused, and **Start** (`/tank start`, a meta's `/vt start`) resumes the same run, its tally,
its place on the route, the monsters it left alone, the corpses still to loot and a Force Buff
under way kept. **Stop** ends the run. `/tank toggle` runs or pauses, as VTank's pause hotkey works
the switch [log 0.3.1.174].

### Fighting

Which monsters it fights, and how, is its **monster list**, as VTank's [std, mlm]: a row per
monster name (the first row, `<DEFAULT>`, for any other) with a priority (-1 leaves it alone;
higher first), whether to attack it (VTank's A), to ring it (R) and to cast streaks at it (S), its
damage type or mode, the weapon or caster to use on it (`auto`: chosen, below) and the shield or
offhand weapon beside a melee weapon (Offhand), the debuffs to put on it first (Imperil, the damage
type's vulnerability, Yield, Fester, Broadside, Gravity Well, Weakening Curse, and the void curses
and damage over time, Festering Curse, Corruption, Destructive Curse and Corrosion: VTank's I, V, Y,
F, B, G, WC, FC, Cp, DC and Cs), and an extra vulnerability (VTank's Ex. Vuln: an element, or
`auto` for the damage type's), and the element of the pet summoned for it (VTank's PetDmg:
`pauto`, `auto`, `none` or an element; below). A row may be an expression over VTank's monster variables [mlm]:
`name`, `typeid`, `species` (its species' name, `Drudge`), `maxhp` (from the monster's appraisal,
asked for when an expression needs it), `range` (metres), `hasshield` (it wields armour),
`setting_<name>` (one of the settings) and `metastate`; `range>5 && species==drudge` matches a drudge
further than five metres off. On the Monsters page a row is added by its name, or from the monster
selected in the game (**The selected**), and its Weapon and Offhand are chosen from the combat items
(below) and the pack's [std].

Of the monsters of one priority it fights first one in a straight line of sight, before one a
wall, the ground or a closed door stands in front of: a bolt or an arrow would hit that first
(VTank's wall detection [log 1.0.0.0], which chooses another target). Then, by `TargetSelectMethod`
[adv, log 0.3.0.17], the nearest (1), the one it turns least to face (2), or (3, the default) by
turning within `TargetSelectAngleRange` and by distance beyond it. It keeps its target unless one
of higher priority comes, or it goes out of sight while one of its priority is in sight. With
`TargetLock` [adv, faq], the monster the player selects is fought first while it is one the Tank
would fight, unless one of higher priority is about.

**Getting to a monster.** A monster further than `AttackDistance`, or behind a wall, is walked to
along the client's own route to it (its `nav.path` capability, the planner its journeys walk): round
walls, through doors, which it opens on the way, and across a dungeon's rooms, a corner a step, so a
monster coming near, or health dropping, is answered at the next step. The route is planned again
when the monster moves three metres from where it stood, and every ten seconds. A monster no route
reaches (walled in, on a ledge) is left alone, as one it cannot hit is, and the next one fought. A
corpse is walked to the same way. Without the capability every walk goes straight at its target.
A melee attack from beyond melee distance is the server's charge straight at the monster, so with
a person (a townsperson, a vendor, a player) standing across that line the Tank walks round them
first, as a charge into them would stop short. After a kill it chooses the next monster itself: a
swing still charging when the fallen monster's body goes is dropped, never sent at whatever the
client selects next.

With a caster in hand it casts the row's debuffs, then the best attack bolt it knows of the row's
damage type (`auto`: the first type it knows one of; for a character with Void Magic trained and
War Magic not, nether first [faq]), no higher than `MaxSpellLevel`; under `Recharge-Norm-Mana` in a
fight it gets mana back first (Stamina to Mana, a mana potion) rather than casting with too little.

- **Streaks** [std]: a row marked S casts the type's streak in place of the bolt.
- **Arcs** [adv, log 0.3.0.17]: with `UseArcs`, the arc is cast in place of the bolt where it is
  known at the bolt's level, at monsters `ArcRange` away or more (0: at any range).
- **Rings** [std, adv]: a row marked R that attacks casts its type's ring (Searing Disc, Halo of
  Frost, Clouded Soul and the rest, at the best level known) once `MinimumRingTargets` monsters or
  more are within `RingDistance`, and attacks as usual with fewer; one marked R that does not attack
  rings while any monster of a row marked R is within `RingDistance`, and attacks those beyond with
  the usual spell.
- **Damage modes** [faq, log 0.3.1.113, 0.3.1.115]: `drain` (VTank's Drain Auto) drains the vital
  the character is short of from the monster (mana under `Recharge-NoTarg-Mana`, else stamina under
  `Recharge-NoTarg-Stam`, else its health; which drain VTank picks is not published, so this is the
  plugin's reading); `auto`, for a character with neither War Magic nor Void Magic trained, is
  drains [faq]. `harm` casts Harm Other (else Martyr's Hecatomb). `fists` faces the monster and
  casts Tusker Fists ahead, its hail going down a straight path. `random` fights each monster with a
  damage type chosen at random, once a monster, of those it knows an attack spell of.
- **Damage over time** [faq, adv]: Corruption, Destructive Curse and Corrosion are cast again only
  once they have run their course (VTank's `DebuffPrecastSeconds` does not apply to them).
- **Breakable turn** [log 1.0.0.0]: with `BreakableTurn` (VTank's option has a name of its own that
  its documentation does not give), before a cast at a monster more than 20° off to the side it
  turns to it, then looks again: a monster now to be fought first (one of higher priority that came
  meanwhile) is fought instead.
- **DoJiggle** [adv]: VTank describes it only as a "secret spell dodge mode"; with it, after each
  cast or shot at a monster out of melee reach the Tank side-steps, left and right in turn, so the
  bolts aimed at where it stood miss. With `DebuffEachFirst` [adv, log 0.3.0.19] at 2 (Priority) it
debuffs every monster of its target's priority in reach before attacking, at 3 (All) every monster
in reach; at 1 (One, the default) the target only. A debuff is cast again `DebuffPrecastSeconds`
before its spell's duration ends [adv] (where the client does not know the duration,
`DebuffRecastSeconds` after it landed). The row's weapon is taken up in peace mode before the
fight (else the `Weapon` setting, else the weapon put away for a spell). Spells are cast in magic
stance: for buffs and its own restoring spells it takes up `CasterWeapon` (else the first caster in
the pack) and enters the stance, and takes its weapon up again for the next fight. With
`SwitchWandsToDebuff` [adv], a monster its row fights with a weapon is debuffed with a caster first,
and the weapon taken up again after: a melee character debuffs, then swings. A melee weapon has the
row's Offhand (else the `Offhand` setting) taken up beside it, a shield or an offhand weapon [std].

**The combat items** [std] are VTank's Items tab, on the Items page: the weapons, casters and
shields `Weapons` names, and the pets `Pets` names, each added from the item selected in the game
(**Add**; a summoning essence goes to the pets), or added never to be buffed (**Add (No Buffs)**,
into `NoBuffItems`); a combat item chosen is switched between buffed and not, or removed. The page
also holds the slots set from the selection (`Weapon`, a bow, `CasterWeapon`, `Ammo`, `Offhand`).

**Choosing the weapon** [main, log 0.3.1.168]: a row whose weapon is `auto` fights with the weapon,
of those `Weapons` names (VTank's Items tab; empty: every weapon in the pack), that strikes with the
row's damage type; of those, a rending weapon of that type first, but on a monster made vulnerable
(a vulnerability of the Tank's on it) a critical-strike, crippling-blow or armor-rending one first.

**Grenades** [main, std, log 1.0.0.0]: a thrown weapon `Consumables` names is a grenade. A fighter
whose row is marked R throws one at its target, where the crowd a ring would be cast for is about,
one of the row's element first, the shield or offhand weapon kept; when VTank throws its grenades
is not published, so this is the plugin's reading.

**Summoning** [std, adv, log 0.3.1.151, 0.3.1.156, 0.3.1.157]: with `SummonPets`, no pet of the
character's out (a creature named for it, "Tester's ..."), and `PetMonsterDensity` monsters or more
that their rows summon for (PetDmg not `none`, priority 0 or more) within the pet range
(`PetRangeMode` 0: `AttackDistance`; 1: `PetCustomRange`, the Combat page's *Custom pet range*), it
uses a summoning device (an essence) of those `Pets` names (VTank's Items tab's pets; empty: any in
the pack) of the character's summoning mastery, its skill enough for it, with charges left. Its
element is the one the row of the highest-priority monster in range asks for: `pauto` (the
default) the element the character fights it with, else the first of the auto list a device has;
`auto` that first; `none` never summons for it; an element only that element. A failed summon
waits half a minute. How VTank refills a device's charges (`PetRefillCount-Idle`,
`PetRefillCount-Normal`) is not published, so it is not carried. `/vt testpet` [cmd] says what it
would summon here now, or why nothing.

A melee or missile swing goes at `AttackPower` (`TiredAttackPower` when stamina is under
`Recharge-Norm-Stam`). With `AutoAttackPower` [adv, log 0.3.1.0], a melee weapon that both thrusts
and slashes is swung on the side of the thrust mark (a third of the bar) that strikes with the
row's damage type, pierce or slash. With `UseRecklessness` [adv] and Recklessness trained, the
power stays within Recklessness's band of the bar, 20% to 80%. VTank's `/vt setattackbar` [cmd]
sets `AttackPower`; its `/vt equipitemsfor <monster>` [cmd] takes one step toward wielding the
weapon that monster's row names (to peace mode, then the weapon).

It recovers on its own: a monster it cannot reach, or that will not die, is left alone for
`BlacklistMonsterTimeoutSeconds` (longer each time it fails again), after
`BlacklistMonsterAttemptCount` failed attacks; a monster on another floor is not run at; stuck on
the route, it first tries a journey to the waypoint, which finds a way round, then backs off to one
side and tries again, and after `StuckTries` skips the waypoint. A **ghost** [adv, cmd], a monster
the client shows that the server no longer has, is left alone until it goes: one whose casts never
begin `GhostMonsterSpellAttemptCount` times running (with `DeleteGhostMonsters`), one whose health
has not changed in `GhostDeleteHPTrackerSeconds` of fighting (with
`DeleteGhostMonstersByHPTracker`), and one the player marks (`/vt deletemonster`). VTank has the
client delete a ghost; a plugin cannot make the client forget an object, so the Tank only leaves it.

### Buffs

With `EnableBuffing` it keeps **buffs** up: VTank's default set (the attributes, a buff for each
trained skill, the protections, the regeneration buffs) or the families `BuffSpells` names, recast
with less than `RebuffTimeRemainingSeconds` left; and with `BuffItems` the item buffs on the weapon
held (Blood Drinker, Swift Killer, Heart Seeker, Defender), the caster held (Spirit Drinker,
Hermetic Link, Defender) and the armour worn (Impenetrability and the banes). Where the weapon buffs
are auras (`Aura of Blood Drinker Self` and the rest, as in the final retail data), they are cast on
the character instead: a weapon's for a character carrying a weapon, a caster's for one carrying a
caster. The buffs that raise the casting skills (the magic masteries, Focus, Willpower) come first.
Each buff is cast at the highest level its buffed skill casts reliably, the spell's difficulty and
`SpellDiffExcessThreshold-Buff` more, and is cast again at a higher level once the skill reaches it.

With `FastCastBuffs` [log 1.0.0.0] (VTank's option, under a name of the plugin's own, as VTank's
documentation gives none) the character moves backward while it casts a buff, which casts it
sooner; VTank's own note is to use it with caution.

What is left out [beg, std, log 0.3.1.132]: a buff `ExcludedBuffs` names (VTank's Buffs tab's
exclusions) is never cast; a scarab `BlacklistedSpellComps` names leaves out every spell of its
level (`Platinum Scarab`: level 7), a lower level cast instead; an item `NoBuffItems` names (VTank's
Items tab's Add (No Buffs)) is never buffed. After a buff, the others are recast
`BuffCastRecast_Seconds` earlier for `BuffCastRecastReset_Seconds` [adv], so buffs running low
together are cast together.

**Buff profiles** [faq]: of the protections and banes, only those `BuffProfile_Prots` and
`BuffProfile_Banes` name are cast: `All` (bludgeon, pierce, slash, acid, lightning, cold and fire),
`None`, `B`, `BPS`, `BPSA`, `ALFC`, or `Custom`, whose letters (B, P, S, A, L, F, C) are then
`BuffProfile-Prots` and `BuffProfile-Banes`, as VTank's FAQ sets them. `AddedBuffs` adds buffs to
the default set, as VTank's Buffs tab adds one for a skill not trained, such as Leadership [std].
The Buffs page lists the buffs the Tank keeps up, each with the spell it is cast with now (the spell
book read when the page opens) and why it is there (the default set, `BuffSpells`, added, an aura,
or left out); a buff named is added (**Also cast**), and one chosen is left out (**Never cast**,
into `ExcludedBuffs`), cast again, or taken off the buffs added.

**Buff items** [std]: a food, potion or gem the `Consumables` name whose spell lasts five minutes
or more (VTank allows no shorter one, as its buffs are recast with five minutes left) is used when
its buff is missing or running out, for a buff the character does not cast itself; which VTank
uses when both would serve is not published, so this is the plugin's reading. The item is
appraised once for its spell.

**Force Buff** [std, cmd] (`/tank forcebuff`, a meta's `/vt forcebuff`, the Buffs page's button)
casts every buff again now, whatever time it has left, each once; a pause holds it and a resume
carries it on where it left off; `/tank cancelforcebuff` ends it. With the macro stopped, it starts a
run for the buffs alone, which stops once they are cast.

An item buff's time runs while the character plays, by the character's age (its seconds played),
not by the clock [log 0.3.0.20]: an item buffed before a logout is not buffed again until it needs
it. It lasts its spell's duration (`ItemBuffMinutes` where the client does not know it).

A buff is done only when its effect shows on its target: the character's enchantments, or the
item's appraised spells. A fizzle, a cast that ends with nothing to show, or any other failure casts
the same spell again as soon as the character can cast (the server's end of the failed cast, once
the character is back in its ready stance), never the next buff. Too little mana runs the mana cycle
(Stamina to Mana, Revitalize) and then casts it again; a level the skill no longer casts reliably
drops to the next one down. Only a refusal casting again cannot mend (the spell not known, a
component missing, a target it cannot take) skips the buff, said once; a cast the client itself
will not send is left two minutes, said. A spell that fails 15 times in a row pauses buffing for a
minute, said, and is then cast again.

With `RandomHelperBuffs` [adv, faq], every `RandomHelperIntervalSeconds` with nothing to fight, it
casts a protection it knows, at random, on a player nearby, at random, remembering nothing of it,
as VTank's author describes the feature.

**Buffs from a buffbot.** A character that casts no buffs of its own asks another player's
character for them: a **buffbot** (the [Buffbot](buffbot.md) sample, or a player buffing by hand).
On the Buffs page: the buffbot (`Buffbot`, its character's name, or **Set the selected buffbot**,
which keeps its place and where the character stood), the tell that asks it (`BuffbotRequest`,
`buffs`), and when to go: when a buff it gave has under `BuffbotMinutes` left or is gone (a death
takes them all), or when the character has none from it yet (`0`: only when told). **Go to the
buffbot** (`/tank buffbot`, a meta's or a route's line typing it) goes now. The **buffbot trip**
goes to it (a journey when it is far, through portals too), tells it the request, and waits for the
buffs to land, answering what it says: a place in line keeps it waiting, "come within N m" walks it
nearer, "Done" ends the wait (once the last buffs have landed). It gives up, and goes back to where
it left as the vendor trip does, on a refusal (the buffbot's cap, a full line, a word it does not
know), on no answer (it asks three times, half a minute apart), on a buffbot not where it was (a
minute's wait), or after `BuffbotWaitSeconds`; and after a trip that brought no buffs it asks again
only after `BuffbotRetryMinutes`, so an absent buffbot does not hold the macro up. The buffs that
landed while it waited are the buffbot's, kept with the character. Dead on the trip, it asks again
where it comes back to life and stays there after. On a vendor trip, buffs it gave with under twice
`BuffbotMinutes` left are renewed while the character is in town: from the vendor it goes on to the
buffbot, and from there back to where the vendor trip set out. The words it understands are the
Buffbot sample's.

### Vitals

**Vitals** follow VTank's own lists [faq], chosen by the vital and the stance, the first way at
hand used. Health in magic stance: under 15%, Stamina to Health, Mana to Health, heal self, a health
food; then a kit (with `UseKitsInMagicMode`), Stamina to Health, Mana to Health, heal self, a health
food. Health in other stances: under 15%, a health food; then a kit, a health food, Stamina to
Health, heal self. Stamina to Health and Mana to Health are cast only when they give more health
for their mana than a heal by `StaminaToHealthMultiplier` and `ManaToHealthMultiplier` (counting the
restam, or the mana, each costs to get back). Stamina or mana: a kit of that vital, a food or potion
of it, then the spell (Stamina to Mana Self; Revitalize Self for stamina, tried before stamina foods
in magic stance). A health kit is used only when its heal is at least `MinimumHealKitSuccessChance`
likely; of those, the one of the highest bonus. Kits, foods and potions are those `Consumables`
names (empty: any the pack holds); a food's vital is the one its spell restores. The consumables are
VTank's Consumables tab [std], on the Items page: each with its kind and how many the pack holds,
added from the item selected in the game or by name, and removed. With
`RefillWornMana`, a worn item under `RefillWornMana-Item-ManaPercent` of its mana is refilled from a
charged mana stone or charge used on the character (with `ManaChargesWhenOff`, while the macro is
stopped too); looting keeps up to `ManaStoneLootCount` mana stones and drains looted items the
profile does not keep, holding at least `ManaTankMinimumMana`, into empty ones.

**Dispelling** [std, adv, log 0.3.1.153]: with a vulnerability of level 7 or more on the character
(an element's vulnerability or Imperil), `UseDispelItems` uses a dispel item from the pack (a Gem
of Stillness, a dispel potion or gem, a Rune of Dispel, a Black Potion, those `Consumables` allows),
and `CastDispelSelf` casts Eradicate Life Magic Self; a dispel is tried again three seconds later
while the vulnerability stays. VTank's `UseDispelDrum` is listed without a description, so it is
not carried.

With `GoToPeaceModeToUseKits` [adv] the character drops to peace mode before it uses a healing kit.
**The recharge boost** [adv]: once the character has switched to magic stance for a restoring
spell, for `RechargeBoostTimeSeconds` its marks are raised by `RechargeBoostAmount` points of the
vital (never past full), so it goes on recharging while it is in the stance; with
`ClearLevelBoostFlagOnCast` the boost ends at the next spell of another kind (VTank's options say
the boost has the macro "prefer to recharge once it switches to magic mode to recharge"; how far,
and when it ends, is this plugin's reading of their names).

**Wand spells** [faq, log 0.3.1.123]: a caster's own restoring spell (the Casting Stein's Stamina
to Mana, any "X to Y" spell built into a wand) is checked with the character's own spells of its
kind, as VTank's FAQ says, and used before them, as the caster pays its mana: the caster magic
stance takes up is used on the character where it is held (`act.use_on`, the interface's use of an
equipped item), never put in the pack. A caster whose magic needs more Arcane Lore than the
character has (its difficulty, from its appraisal) is passed over, and one whose use gave nothing
back is left two minutes. Not carried: VTank's `RechargeHandlerSet`, whose layout in a settings
profile VTank does not publish.

### Looting

It loots the corpses of its kills within `CorpseApproachRange-Max`, remembered
`CorpseCacheTimeoutMinutes`, and others' as its options say [adv]: with `LootFellowCorpses` those
of its fellows' kills, with `LootAllCorpses` those of anyone else's, each told by its appraisal
(its description names its killer, "Killed by ..."); with `LootOnlyRareCorpses` only a corpse
that generated a rare (its description says so) is looted, its own or another's. Of those near
enough to open where the character stands, first the one it turns least to face, else the nearest
[log 0.3.1.157]. It walks to within
`CorpseApproachRange-Min` and opens it; a corpse that does not open is tried again after
`CorpseOpenTimeoutSeconds`, and after `BlacklistCorpseOpenAttemptCount` tries left for
`BlacklistCorpseOpenTimeoutSeconds` [adv, log 0.3.1.146]. It waits for the corpse's items to stop
coming (`CorpseItemAppearanceTimeoutSeconds` at most) and for their appraisals
(`CorpseItemIDTimeoutSeconds`), then takes what the profile's rules take; an item that will not come is
tried again, `CorpseLootItemMaxAttempts` times. A keep-up-to rule's items are taken until the pack
holds the rule's count. With `ReadUnknownScrolls` [adv, faq], once the corpse is looted it takes and
reads the scrolls the profile does not keep of spells the character does not know and can learn
(its buffed skill in the spell's school within 15 of the spell's difficulty).

**Loot this Container** [log 0.3.1.118] (`/tank lootcontainer`, the Loot page's **Loot the open
container**, or its hotkey) takes from the container open now, a chest or a corpse the player
opened, what the loot profile's rules take, as a corpse the macro loots.

**What a rule takes.** Every rule of the profile takes its item from the corpse, as this plugin
reads VTank's loot actions (how VTank treats Sell, Read and User rules is not in its published
pages): a Keep or Keep # rule's item is kept; a Salvage rule's is salvaged; a Sell rule's is kept to
sell at a vendor (below); a Read rule's is read (used) once the corpse is done; a User rule's is
kept for the player.

**Salvage** [adv, tips, log 0.1.0.4, 0.3.0.11, 0.3.0.10]: what Salvage rules took is salvaged with
the pack's salvaging tool (an Ust) once the corpse is looted and before the next one; an item
tinkered or inscribed is kept, not salvaged, nor anything from a corpse of the character's own
name [log 0.3.0.13, 0.3.1.37]. With `CombineSalvage` [adv] the bags of one material are combined
as the loot profile's salvage ranges say (its `SalvageCombine` block, read by
`profile:salvage_combine()`, the shape the published profiles the Tank ships hold): two bags whose
workmanship (its whole number) falls in one range of the material's own list, else of the
default list, are salvaged together into one, the smallest first, while they fill no more than a
bag of 100. A material with no ranges is not combined (the plugin's reading).

**The pack** [std, adv, log 0.3.1.157]: with `AutoStack` stacks are merged toward the bottom-most
of their kind (the main pack first, then each side pack), so the one partial stack is the
top-most, which the game uses first; items of one name that do not stack, and what is worn, are
left. With `AutoCram` the main pack's items are moved into the side packs that have room. Each
is a step at a time, with nothing to fight.

### Vendors

VTank leaves buying and selling to the Virindi Item Tool [vit]; the Tank does it itself, at a
vendor whose wares it opens:

- **It sells** (`VendorSell`) what the pack holds that the loot profile's Sell rules match, and
  what the vendor's own profile's Sell rules match. Nothing worn or wielded is sold, nor a pack,
  nor an item a setting names for the macro's own use (`Weapon`, `CasterWeapon`, `Ammo`,
  `Offhand`, `Weapons`, `Consumables`, `NoBuffItems`).
- **It buys** (`VendorBuy`) each item of its **buy list** the vendor stocks, up to the list's
  count, counting what the pack holds (the Item Tool's list of what to buy, with a number each
  [vit]); and by the vendor's own profile, what a Keep # rule matches up to its count and one of
  what a Keep rule matches that the pack lacks. It buys no more than its pyreals pay for.
- **A vendor's own profile** is `vendors/<the vendor's name>.utl` in the data folder, the vendor
  profiles Mag-Tools reads [mag]: its published samples (named after their vendors) sell with
  Sell rules and buy with Keep # and Keep rules.

It trades at a route's vendor waypoint, once the wares are open, and closes them after; on the
vendor trip; and with `/tank sell` at a vendor whose wares the player opened. Each trade is said
in the log: what it sold, what it bought, and the pyreals before and after.

**The vendor trip** goes to the vendor it was given (**Set the selected vendor**, `/tank vendor
set`, or `/tank vendor set <name>` for the nearest in sight of that name: its place, and where the
character stands then, are kept with the character; else the first vendor waypoint of the route).
From further than 30 metres it travels by a journey (portals and recalls too) to where the
character stood when the vendor was set (a place a body stands, near the vendor), else to the
vendor's own place; within 30 metres it walks as far as a walking route goes and uses the vendor,
the server walking the character the last of the way (to a shopkeeper behind a counter, say). It
trades, goes back to where it left by a journey, and carries on the route from its nearest
waypoint; a monster close at hand on the way is fought first. It starts on:

- `/tank vendor` (the Vendor page's **Go to the vendor**): with the macro stopped, a run for the
  trip alone, which stops when it is done;
- a meta's Chat Command, or a route's chat waypoint, typing `/tank vendor` (a VTank meta's usual
  vendor rule, Pack Slots <= n, can so send the Tank);
- a full pack, with `OnFullPack` `vendor`: back from the vendor, the macro hunts on, or stops
  when the pack is still full.

### Death

With `StopMacroOnDeath` off, the macro carries on after a death. With `RecoverCorpse` (on by
default) it goes back for the **corpse**: where the character fell is kept with it (so a client that
logs in again still goes back), and once it is alive again, buffed again where a buffbot trip is
due, it travels there by a journey, finds the corpse of its own name, opens it and takes back
everything in it, then carries on. A death the game says left nothing ("You have retained all your
items") is not gone back for; a corpse not found where the character fell, or not reached after
three tries, or after half an hour, is given up. Then, far from its route, it travels back to it:
to the hunting place (`/tank hunt`) when there is one, else to the route's nearest waypoint.

### Doors

With `OpenDoors` [adv] (on by default), before it walks a leg of its route a closed door standing
across the leg (within `DoorOpenRange` of it) is opened: the Tank uses it and waits, three seconds
at most, for it to stand open, as the client's own journeys open the doors on their way, and then
walks through. Walking to a monster or a corpse, it opens each door the client's route to it names,
the same way, from the corner before it; a door that does not open leaves the monster alone a
while.

A door that does not open is locked. Its lock is picked [std, adv, beg] with a lockpick
`Consumables` allows (empty: any in the pack; the one of the highest bonus): the door is looked at
(appraised, from within `DoorIDRange`) for its lock's resistance, and picked when the character's
lockpicking and the pick's bonus beat it by `DoorLockpickDiffExcessThreshold`, three tries at most,
the door used after each. One it cannot pick is left as it is: it says once that a locked door
blocks the way to the waypoint, and walks the leg as before, its stuck recovery finding a way
round or the waypoint being skipped.

### Routes

Where the client makes journeys (`NavJourneys` on, the default), it travels: `/tank hunt <place>`
goes there first, through portals and by the character's recalls, and then runs the macro (typed
while it runs, a meta's line, say, it travels there at its next step); a route further than
`JourneyToRouteMetres` is traveled to: to the hunting place when there is one (a dungeon's
entrance, which a route's map coordinates cannot name from outside it), else to the route's nearest
waypoint. Three such journeys failing in a row stop the macro. Where only a drop off a ledge leads
on (the client's journeys leave a drop to the player), any of its journeys steps down one no higher
than the body steps down without falling (1.5 m): it walks to the ledge's edge, steps off and makes
the journey again. Run off its route further than `NavFarStopRange` [adv], the macro turns
navigation off.

**A route's waypoints** are carried out as VTank carries them out [std, log 0.3.1.168]. At a jump
waypoint it faces the waypoint's heading and charges the jump for the waypoint's time, holding the
walk key when the waypoint says shift, then leaps (with `nav.jump`). At a Use Portal/NPC waypoint it
uses the portal of that name nearest the waypoint and goes on once the character is through (three
tries, then it goes on without); when what it names is an NPC, it uses the NPC once and goes on. At
an NPC Talk waypoint it uses the NPC again and again until the NPC tells the character something
(twelve uses, five seconds apart, at most). At a checkpoint it waits until the server places the
character there (the server's own word on where it stands runs a little behind the client's),
stepping forward now and then, twenty seconds at most. At a vendor waypoint it opens the vendor and
waits while its wares are open (a minute at most), so a plugin that trades by profile can buy and
sell. A chat waypoint's line is typed, unless it is a VTank command (answered by the macro).

**Following** [std, beg, adv, huds]: a follow route (VTank's, a route file or a meta's embedded
one) follows the character it names, by its object while the client knows it, else the nearest of
its name: the Tank closes to within `NavCloseStopRange` (VTank's "Follow/Nav Min", 2 m at least)
and holds there, between its fights, loots and heals. With `FollowAroundCorners` it walks the
leader's own way, the places the leader passed, round the corners the leader went round (VTank's
"mostly used inside"); without, straight at the leader ("mostly used outside"). A leader the client
does not see is walked to, or travelled to by a journey when far, where its own Tank on this machine
last said it was (*Fellows*, below). **Follow the selected** on the Route page, or `/tank follow`
(VTank's MiniRemote FC), makes a follow route of the selected character (`/tank follow <name>`, of
the character named); **New route** with `follow` chosen does the same, and with a follow route
**Add point** sets whom it follows to the selection, as VTank's Route tab's Add does. Saved, it is
VTank's follow route file (its kind, then the name and object id).

A route runs **reversed** [std, cmd, log 0.3.1.153] (`/tank route reverse`, a meta's
`/vt reverseroute`) the other way round without changing the order of its points, so it saves as
it was made; the title of the Route page says so.

**Routes are made in the game** on the window's Route page, as VTank's Route tab makes them:
**New route** starts one of the kind chosen (circular, linear or once); **Add point** puts a point
where the character stands, or, with a portal, a vendor or an NPC selected, a waypoint that uses
it; **Portal/NPC**, **NPC talk** and **Vendor** a waypoint that uses the selected portal or NPC, the
selected NPC until it speaks, or the selected vendor; **Add jump** a jump at the heading the
character faces, with the charge, the walk key and the sidestep chosen (**Jump now** jumps so at
once); **Add pause**, **Add chat**, **Add recall** and **Checkpoint** the other waypoints. The list
of waypoints is the route as it stands: a waypoint chosen there moves **Up** and **Down** or is
removed; **Reverse** runs it the other way. **Record while walking** adds a point each time the
character has gone `RouteRecordMetres` (5 m, set beside it) from the last point it added. **Save**
writes the route as a VTank route file, `routes/<name>.nav` in the data folder, which VTank and any
VTank-format reader take; **Load** reads one of the routes it lists (those in the data folder, and
its own). A route made or changed in the game is the macro's until it is saved or another is
chosen. The `/tank route` commands do the same from the chat bar, and a meta's `/vt` commands do
the same from a meta.

### Quest steps

VTank gives and uses items only through its meta expressions (`actiontrygiveitem`,
`actiontryapplyitem`, `actiontryuseitem` [meta]), which fire and forget. The Tank has quest steps
of its own, typed as VTank types its commands: by a route's chat waypoint, a meta's chat action,
or the player. Typed by a route, a step is finished before the route goes on:

- `/tank give <item> to <person>` walks to the nearest person of that name, along the client's own
  route to them, and hands them the item from the pack; a person still talking takes nothing, so
  it is given again a little later while it stays in the pack, a few times at most.
- `/tank pickup <item>` walks to the nearest of that item lying on the ground and picks it up.
- `/tank useon <item> on <target>` uses an item from the pack on the nearest object of that name
  (a key on a locked door) or on an item the character holds (an oil on a weapon), in peace mode
  as a trade skill wants; a wielded weapon the use remakes, which comes back in the pack, is
  wielded again.
- `/tank go <place>` stops the macro and travels to the place by the client's journey (a place as
  `/tank hunt` takes it): through portals, by recalls, or by a token handed back where a person
  sends the character on (the Training Academy's way out, which the player is asked about first).

A name ending in `*` names whatever begins with the rest (`Training *`: the character's training
weapon, whichever it is). Whether a step was done is the game's word: the item gone from the pack,
in it, the use acknowledged.

### The Training Academy

The Tank ships a profile for the Training Academy, where a new character begins: a VTank meta
for each starting town's Academy, `metas/academy-holtburg.met`, `academy-shoushi.met`,
`academy-yaraq.met` and `academy-sanamar.met` (the town's first Academy, where a new character of
that town is put), its routes embedded, and the loot profile `profiles/academy.utl`, which keeps
the quest's items. Its routes are the walk of a recorded run through the Academy, and it plays
the quest by what the game says, not by where it is: an item come into the pack, a person's
tell, a portal passed. In order, it:

1. talks to the Society Greeter and hands her the Calling Stone;
2. goes through the doors to the Training Master and fights a Sparring Golem for its Academy
   Token, which it hands him (his tell says the combat training is done);
3. takes the Central Courtyard portal;
4. talks to the Academy Foreman and fights the Carpenter Wasps in his woodpile until one leaves a
   wing, which it hands him;
5. talks to the Academy Blacksmith and fights the Thieving Thrungus in the gardens until one
   leaves his Bellows, which it hands him, for the Academy Library Key;
6. unlocks the Library with the key, buys Oil of Rendering from the Academy Researcher (its buy
   list) and uses it on its training weapon, which it makes the Academy's own;
7. goes upstairs to the Senior Guard and through the Outer Courtyard portal;
8. talks to the Sentry, attunes to the life stone, and goes through the Olthoi hive to the
   Adolescent Olthoi's den, fighting only what comes at it on the way, for a Protection Orb;
9. takes the orb back to the Sentry (his gifts say the Academy is done), and goes to the town
   with `/tank go portal Exit to <town>`, by the Academy's exit portal.

A stage that hunts fights what it finds. One that leaves a hunting ground to hand something over
(the token, the wing, the bellows) fights nothing, since what comes back as fast as it falls would
keep it there; the stages after them fight only what comes at the character (a Thieving Thrungus
follows it out of the gardens). Corpses are looted before the next fight (`LootPriorityBoost`),
except in the hive. A character that dies in the courtyard's rooms comes back at the Academy's
start: the meta walks it back through the Central Courtyard portal (a state called, VTank's Call
State, and returned from) and takes the stage up again. A stage that stalls (a gift not taken)
starts again a while later; the meta's state can be set in the window (Meta, State) to start
partway.

```text
/tank meta academy-holtburg.met
/tank start
```

Leaving early instead is a journey too: `/tank go portal Exit to Holtburg` from the Academy's first
rooms, where no portal leaves, hands Jonathan's Academy Exit Token back (the player is asked first:
the Academy cannot be entered again).

### The litmus profile: a fresh character's hunt from Holtburg

`metas/litmus-holtburg.met` hunts the **Holtburg Redoubt** with a character fresh from the
Academy, with buffs from a buffbot, selling its loot in Holtburg, as long as it is left running:

- **The dungeon.** The Holtburg Redoubt's portal stands 450 m north-east of Holtburg's life stone
  and admits characters of levels 1 to 20; inside are eighteen level-8 drudges (Skulkers, Sneakers,
  Prowlers, Slinkers) that a generator brings back.
- **The route** (`routes/holtburg-redoubt.nav`, embedded in the meta) walks the drudges' rooms from
  the entrance down to the deepest level and back (linear).
- **The loot profile** (`profiles/litmus.utl`) keeps pyreals, trade notes, healing kits, food and
  potions, and takes weapons, armor and clothing to sell; jewelry, gems and wands, which its vendor
  does not buy, it leaves.
- **The meta** sets the macro up (a full pack goes to the vendor; the drudges that come back while
  it loots are fought first; dying does not stop it; the corpse is gone back for), sets the vendor
  (Holtburg's Contract Broker, who buys what it takes to sell and sells trade notes: its buy list
  turns the pyreals the sales bring into 250,000 notes, one stack, where coins would fill the pack
  25,000 to a slot), leaves Holtburg's event creature alone (the Pumpkin Buffer, level 42), travels
  to the Redoubt (`/tank hunt portal Holtburg Redoubt`) and hunts; every twenty minutes of hunting
  it goes to the vendor and back.
- **The buffbot** is the player's to set, once, beside it in Holtburg: `/tank buffbot set <name>`.

```text
/tank buffbot set Buffbel
/tank meta litmus-holtburg.met
/tank start
```

### Fellows

**Helping** [adv, std, main]: with `DoHelp`, a fellow under a helper mark (`Recharge-Helper-HitP`,
`-Stam`, `-Mana`) and within its distance (`HelperDistanceHitP`, `-Stam`, `-Mana`) is healed (Heal
Other), given stamina (Revitalize Other) or given mana (Mana Boost Other; which spell VTank gives
mana with is not published, so this is the plugin's reading), the lowest first, health before
stamina before mana. The fellows are the fellowship's, by the figures the game keeps of them,
which it keeps current while its fellowship panel is open, as VTank's helper needs [std]; a figure
that has not changed since the Tank helped is waited on twenty seconds at most, and a fellow the
server finds beyond the spell's reach is walked to within five metres and helped again. With
`UseHealersHeart` a Healer's Heart heals instead (one `Weapons` names, when it names any): taken
up, and used on the fellow where it is held, its own heal cast; with `JumpOutWandCasting` the
character jumps as soon as it is used, which sends the heal at once [adv].

**Its own fellow on the machine** [vi2, faq, beg]: VTank's fellows of Virindi Integrator 2 are
groups of characters, kept by VI2's own server; the Tank's are the Tanks in the player's other
clients on this machine that `PeerFellow` gives one name (over the client's bus between them,
`peers.local`; nothing leaves the machine). Once a second each tells the others where its character
is, its vitals and the debuffs it has cast; so, as VI2's fellows do, they heal each other without
the fellowship panel (from the vitals told), do not cast a debuff a fellow has already put on a
monster ("try not to double-vuln"), and fight first a monster one of them has made vulnerable
("prefer to attack monsters that have been vulned first"); and a follower finds a leader it does
not see.

**Running a fellowship** [fel, std, adv, log 0.3.1.34, 0.3.1.39]: with `AutoFellowManagement`, as
the fellowship's leader, it answers tells as VTank's fellow manager does, each line signed
`[VT Fellow Manager] ... -v-`: `xp` (recruits the player: "I will recruit you in a moment. Please
stand close to me."; one further than 20 m is told "You are too far away. I will wait 20 seconds
and give you one more chance." and recruited then if near; with the fellowship full, put in line,
and recruited when a place comes), `line` (the fellowship's count and who waits), `remove` (out of
line), `leader`, `startvote giveleader <name>` (a vote called in the fellowship's chat, "To vote
either tell me 'vote 1 yes' or 'vote 1 no'. You have 2 minutes.", its total said when it ends, and
the lead handed on when most voted yes), `vote <n> yes|no` and `help` ("Available commands: xp,
line, remove, leader, startvote, vote, help"). At most eight commands are answered in any three
minutes [log 0.3.1.39]. The other answers' words are the plugin's own (VTank's are not published).

### Metas and VTank's commands

**The Meta page** [meta] is VTank's Meta tab: the meta's switch and file, its state set by hand
(VTank's dropdown) and bound to keys (VTank's meta state hotkeys [log 0.3.1.134]: a key typed
beside the state, **Bind the state**; each bound key shown with **Unbind**), and the meta's rules,
those that ran in this stay in the state in red, as VTank lists them. A rule chosen fills the
editor under the list: its state, its condition and its action, each a kind of VTank's (Never,
Always, All, Any, Not, Chat Message, Pack Slots <=, Seconds in State >=, ..., Expression; Set Meta
State, Chat Command, All, Load Embedded Navroute, Call Meta State, ..., Destroy All Views) with
that kind's fields (a monster count's name, count and distance; a route file to embed). A
condition or action that holds others (All, Any, Not; All) lists its parts, and **Edit part**
edits one the same way (**Back** returns). **Save the rule** puts it in place of the rule chosen,
**Add as new** after it, **New rule** starts an empty one; a rule is moved **Up** and **Down** or
removed. Each change is the running meta's at once, its state, variables and views kept and the
rules of its state that ran in this stay (those unchanged) still counted as run, and is saved, the
meta's file in the data folder (`metas/<name>`), as VTank writes a meta. **New meta** makes a meta
of the name typed with no rules, run from then on; **Save as** saves the meta under that name too.
The page also runs an expression and lists the meta's variables.

A **meta** (VTank's `.met`) runs on top of the macro, as VTank's do [meta]: its rules are checked on
its timer and whenever the macro decides what to do, its embedded routes become the macro's route,
"set option" sets a setting for the run (one of VTank's options the Tank has not got is kept for
the run, so "get option" and `vtgetsetting` read it back), its monster-count conditions use the
monster list, and its chat actions' `/vt` commands are answered by the macro. So is
`/tank vt <command>`, typed by the player [cmd]:

| `/vt` command | does |
|---|---|
| `opt set <option> <value>`, `opt get <option>`, `opt list` | sets (for the run), reads or lists the options |
| `opt setinall <option> <value>` | sets it, and in every settings profile in the data folder (its own row of each, the rest of the file kept) |
| `nav load <file>`, `nav save <file>`, `nav clear` | the route |
| `loot load <file>`, `loot save <file>` | the loot profile; saving writes its file under the other name |
| `meta load <file>`, `meta save <file>` | the meta, the one running stopping at once; saving writes its file under the other name |
| `settings load <file>`, `settings save <file>` | a settings profile, read or written (the settings and monster list) |
| `settings loadchar <file>`, `settings savechar <file>` | the same, kept for this character alone (`characters/<name>/` in the data folder) |
| `start`, `stop` | resumes or pauses the macro (the meta runs on) |
| `forcebuff`, `cancelforcebuff` | Force Buff, and its cancel |
| `setmetastate <state>`, `fakedeath` | the meta's state; its Character Death condition, as though the character had died |
| `mexec <expression>`, `echo <text>` | runs an expression with the meta's variables and says its value; prints the text |
| `dumpmetavars`, `listmetafunctions`, `metafunchelp <function>` | the meta's variables; the expression functions this client answers |
| `addnavpt [coordinates]`, `addnavjump …`, `addnavcheckpoint` | waypoints, as the Route page adds them |
| `reverseroute [true or false]`, `reverseroutequery` | runs the route reversed or as made (alone: the other way); says which |
| `jump …`, `tapjump` | a jump now |
| `setattackbar <0 to 1>`, `equipitemsfor <monster>` | the power bar; a step toward the monster's weapon |
| `deletemonster` | the selected monster taken for a ghost |
| `testitem`, `propertydump` | what the loot profile does with the selected item; its properties from its appraisal |
| `testmonster`, `listmonstervariables` | the monster list's row for the selected monster, with its variables; the variables |
| `testspell <id>`, `testpet` | whether the character knows the spell and casts it within the hunting and buffing thresholds; what pet it would summon here now, or why none |
| `dumpspells`, `dumpspecies`, `dumpmats`, `dumpskills` | the game's spells, species and materials and the character's skills, written to `dumps/` in the data folder |
| `log [type] [on or off]` | the log's types switched off and on (`ActiveRule`, `SalvageList`, `SpellCast`, `RuleInfo`, `Timers`, `CastInfo`, `DebuffChoice`, `Loot`, `CharProps`, `Misc`); alone, which are on |
| `refresh` | the window built again |

Any other `/vt` command is said not to be answered.

### Commands and the window

| command | does |
|---|---|
| `/tank start`, `/tank pause`, `/tank stop` | runs (or resumes), pauses or stops the macro |
| `/tank toggle` | runs (or resumes) the macro, or pauses it while it runs |
| `/tank forcebuff`, `/tank cancelforcebuff` | Force Buff, and its cancel |
| `/tank status` | the state, the target and the run's tally |
| `/tank log [n]` | the last lines of the run's log (kept with the character, so it survives the client) |
| `/tank settings` | every setting and its value, forty lines a frame |
| `/tank set <setting> <value>` | changes a setting (names without case; distances in map units, 1 = 240 m, as VTank's) |
| `/tank route <file>`, `/tank profile <file>` | the route and the loot profile, from its folders |
| `/tank route new [circular, linear, once or follow]` | starts a route made in the game (a follow route of the selected character) |
| `/tank follow [name]` | follows the selected character (or the one named), VTank's MiniRemote FC |
| `/tank route add [coordinates]` | a point where the character stands (or at the coordinates), or a waypoint using the selected portal, vendor or NPC |
| `/tank route portal`, `talk`, `vendor` | a waypoint using the selected portal or NPC (once), the selected NPC (until it speaks), or the selected vendor |
| `/tank route jump <heading> <true or false> <ms> [forward, strafeleft or straferight]` | a jump waypoint here: its heading, whether it walks (shift), its charge and its direction |
| `/tank route pause <ms>`, `chat <line>`, `recall <spell id>`, `checkpoint` | the other waypoints, here |
| `/tank route reverse`, `/tank route points`, `/tank route save <name>` | runs the route the other way; lists its waypoints; saves it as `routes/<name>.nav` |
| `/tank jump <heading> <true or false> <ms> [direction]` | one jump now, as VTank's `/vt jump` |
| `/tank hunt <place>` | travels to the place by a journey, then runs the macro there; a place is `42.2N 33.8E`, a cell `0x019E0140`, `portal <name>` (where a known portal sends) or `lifestone` / `marketplace` |
| `/tank go <place>` | stops the macro and travels to the place by a journey |
| `/tank give <item> to <person>`, `/tank pickup <item>`, `/tank useon <item> on <target>` | the quest steps (above) |
| `/tank monsters`, `/tank monster <field> <value> <name>` | the monster list; sets a row's priority, attack, ring, streak, damage (a type, or `drain`, `harm`, `fists`, `random`), weapon (`auto` to choose), offhand, extra vulnerability (`exvuln`), pet element (`petdmg`) or a debuff (`remove` drops it) |
| `/tank vendor`, `/tank vendor set [name]` | the vendor trip now; the selected vendor (or the nearest in sight of that name) taken for the trip's |
| `/tank buffbot`, `/tank buffbot set [name]` | the buffbot trip now; the selected player (or the nearest in sight of that name) taken for the buffbot |
| `/tank sell` | sells and buys at the vendor whose wares are open |
| `/tank lootcontainer` | takes from the open container what the loot profile takes |
| `/tank buy <count> <item>`, `/tank buy` | buys the item up to the count at vendors (0 takes it off the list); the buy list |
| `/tank import <x.usd>`, `/tank export <x.usd>` | reads a VTank settings profile (its settings, monster list, combat items, consumables and buffs) from the data folder, or saves this character's as one |
| `/tank meta <x.met>` | runs a VTank meta with the macro (`none` for none) |
| `/tank vt <command>` | a VTank command as a meta types it (above) |
| `/tank files` | what is in its data folder |

The **window** holds everything the commands change, in VTank's tabs: Run (Start, Pause and Stop, a
place to hunt at, Hunt and Go, the remote control switched on, the meta and its state, the state,
the vitals, the tally, and the log, its time a fixed column and its line the rest, taking the rest
of the page), Combat (the targeting, rings, arcs, debuffing, melee, dodging and ghost settings,
summoning; the selected monster tested, its properties, or marked a ghost; a step toward a
monster's weapon), Monsters (the monster list: a row chosen fills the fields under it, its name
typed or the selected monster's, its attack, rings and streaks, damage, weapon and offhand of the
combat items, extra vulnerability, pet element and debuffs, which save it or remove it), Items (the
slots set from the selected item, the combat items and the consumables, above), Nav (following
around corners, and the doors and their locks), Route (above, Follow the selected, and the quest
steps: an item named, Give to the selected person, Pick up, Use on the selected object, at once or
as a waypoint), Loot (the corpse settings, others' and rare corpses, combining salvage, stacking
and cramming, the selected item tested against the profile, the open container looted, the profile
saved as another file), Vitals (with the dispels, kits in peace mode, the recharge boost and
refilling worn mana while stopped; helping fellows, the Healer's Heart, running the fellowship,
and its own fellow on the machine), Buffs (the buffs kept up, added and left out, the buff
profiles, fast-cast buffs and helper buffs, Force Buff and its cancel, a spell tested, and the
buffbot: who, the tell, when, Set the selected buffbot and Go to the buffbot), Vendor
(selling and buying, the vendor set from the selection, the trip, a trade at the open vendor, and
the buy list, each row its item, its count and what the pack holds, set from a name or the selected
item, or removed), Meta (above), and Settings (when to stop, the hotkeys, then every setting by
name with its value, found by part of its name as VTank's advanced options are [log 0.3.1.41],
set, put back to its default or set in every profile; the log's types; the
dumps; a settings profile imported, the character's exported, and this character's own saved and
chosen from a list to load, as VTank's [Char] profiles [std]).
Each setting is a toggle, a number (distances in metres there), a choice, or text typed in. A page
too tall for the window scrolls under its row of tabs. The commands stay, as shortcuts to the same.

**The remote control** [huds, log 0.3.1.142]: with `MiniRemote` (the Run page's switch) a small bar
stands on the screen, as VTank's Remote Control does: **F** Force Buff, **CF** its cancel, **FC**
follow the selected character, and two switches, **C** following around corners
(`FollowAroundCorners`) and the lock (`TargetLock`), each showing its setting however it was set.
It opens with the Tank's window, or as the player turns the switch on. It needs a host that draws
plugin HUDs (`ui.hud`); the player moves it as any plugin HUD.

**Hotkeys** [log 0.3.0.10, 0.3.1.113, 0.3.1.118, 0.3.1.134, 0.3.1.174]: VTank's run-or-pause
hotkey (`HotkeyRun`), its melee attack heights (`HotkeyHigh`, `HotkeyMid`, `HotkeyLow`), "Loot this
Container" (`HotkeyLootContainer`) and its meta state hotkeys (`MetaStateHotkeys`, set on the Meta
page). Each is a key typed on the Settings page (Ctrl or Alt held, then F1 to F24, A to Z or 0 to
9: `Ctrl+Shift+P`), bound by **Bind the keys**, to the command it runs (`/tank toggle`, `/tank set
DefaultMeleeAttackHeight 1`, `/tank lootcontainer`, `/tank vt setmetastate <state>`); a key a
hotkey cannot take is said and not bound. VTank bound its keys through other plugins' hotkey
systems; which keys it offered beyond these is not published.

Other plugins in the client reach the Tank by its **services** ([services.md](../../services.md)):
`running` (whether the macro is running: the Looter waits while it is), `tally` (whether it runs,
its state and the run's tally in words, for a plugin that logs the run), `get_meta_state`,
`set_meta_state`, `get_setting` and `set_setting`.

The player's own files (routes, loot profiles, settings profiles, metas) go in the plugin's **data
folder** (`plugin-data/network.dereth.tank/` beside the client's settings), looked in before the
plugin's own `routes/` and `profiles/`.

## Settings

The settings carry **VTank's option names and units** [opt, adv], so a VTank settings profile and a
meta's "set option" mean the same thing here. Distances are in map units (240 m, VTank's
"landblocks"); vital marks in percent. Where VTank's documentation gives no default, the Tank's is
its own.

- **VTank's:** `EnableCombat`, `EnableNav`, `EnableLooting`, `EnableBuffing`, `EnableMeta`;
  `AttackDistance`, `AttackMinimumDistance`, `ApproachDistance`, `DefaultMeleeAttackHeight`,
  `TargetLock`, `TargetSelectMethod`, `TargetSelectAngleRange`, `DebuffEachFirst`,
  `DebuffPrecastSeconds`, `AutoAttackPower`, `UseRecklessness`, `RingDistance`,
  `MinimumRingTargets`, `UseArcs`, `ArcRange`, `SwitchWandsToDebuff` (its options page also spells
  it `SwitchWandToDebuff`; both are read), `DoJiggle`, `IdlePeaceMode`,
  `BlacklistMonsterTimeoutSeconds`, `BlacklistMonsterAttemptCount`, `DeleteGhostMonsters`,
  `GhostMonsterSpellAttemptCount`, `DeleteGhostMonstersByHPTracker`, `GhostDeleteHPTrackerSeconds`,
  `StopMacroOnDeath`, `SummonPets`, `PetRangeMode`, `PetCustomRange`, `PetMonsterDensity`;
  `NavCloseStopRange`, `NavFarStopRange`, `NavPriorityBoost`, `FollowAroundCorners`, `OpenDoors`,
  `DoorOpenRange`, `DoorIDRange`, `DoorLockpickDiffExcessThreshold`, `LootPriorityBoost`; `CorpseApproachRange-Max`, `CorpseApproachRange-Min`,
  `CorpseOpenTimeoutSeconds`, `BlacklistCorpseOpenAttemptCount`,
  `BlacklistCorpseOpenTimeoutSeconds`, `CorpseItemAppearanceTimeoutSeconds`,
  `CorpseItemIDTimeoutSeconds`, `CorpseCacheTimeoutMinutes`, `CorpseLootItemMaxAttempts`,
  `ReadUnknownScrolls`, `LootFellowCorpses`, `LootAllCorpses`, `LootOnlyRareCorpses`,
  `CombineSalvage`, `AutoStack`, `AutoCram`; `Recharge-Norm-HitP/Stam/Mana`, `Recharge-NoTarg-HitP/Stam/Mana`,
  `UseKitsInMagicMode`, `GoToPeaceModeToUseKits`, `RechargeBoostTimeSeconds`,
  `RechargeBoostAmount`, `ClearLevelBoostFlagOnCast`, `MinimumHealKitSuccessChance`, `StaminaToHealthMultiplier`,
  `ManaToHealthMultiplier`, `ManaChargesWhenOff`, `RefillWornMana`,
  `RefillWornMana-Item-ManaPercent`, `ManaStoneLootCount`, `ManaTankMinimumMana`,
  `UseDispelItems`, `CastDispelSelf`; `DoHelp`, `Recharge-Helper-HitP/Stam/Mana`,
  `HelperDistanceHitP`, `HelperDistanceStam`, `HelperDistanceMana`, `UseHealersHeart`,
  `JumpOutWandCasting`, `AutoFellowManagement`; `Consumables` (its
  Consumables tab, as a list of names); `RebuffTimeRemainingSeconds`, `IdleBuffTopoff`,
  `IdleBuffTopoffTimeSeconds`, `BuffCastRecast_Seconds`, `BuffCastRecastReset_Seconds`,
  `BlacklistedSpellComps`, `BuffProfile_Prots`, `BuffProfile_Banes`, `BuffProfile-Prots`,
  `BuffProfile-Banes`, `RandomHelperBuffs`, `RandomHelperIntervalSeconds`,
  `SpellDiffExcessThreshold-Buff` and `SpellDiffExcessThreshold-Hunt`.
- **VTank's tabs, as settings:** `ExcludedBuffs` (the Buffs tab's exclusions), `AddedBuffs` (the
  buffs it adds), `NoBuffItems`
  (the Items tab's Add (No Buffs)), `Weapons` (the Items tab's weapons an `auto` row chooses from),
  `Pets` (its pets) and `Offhand` (its shields and offhand weapons, for rows that name none).
- **VTank's, under names of the plugin's own** (VTank's documentation does not give theirs):
  `BreakableTurn` and `FastCastBuffs`.
- **The plugin's own:** `PeerFellow` (its own fellow on the machine), `AttackPower`, `TiredAttackPower`, `GiveUpSeconds`, `StuckTries`,
  `NavJourneys`, `JourneyToRouteMetres`, `HealSpells`, `UseHealingKits`, `RestWhenIdle`,
  `MinFreeSlots`, `OnFullPack` (stop, recall, vendor or keep), `VendorSell`, `VendorBuy`,
  `BuyList` (`Prismatic Taper=50, Bread=5`; a name may hold a comma, as
  `Trade Note (250,000)=4`), `Vendor`, `StopAfterDeaths`, `RecallSpell`,
  `StopAfterMinutes`, `RecoverCorpse` (back to the corpse a death left), `Buffbot`, `BuffbotRequest`,
  `BuffbotMinutes`, `BuffbotWaitSeconds`, `BuffbotRetryMinutes` (buffs from a buffbot),
  `StartOnLogin`, `RouteRecordMetres`, `Weapon`, `CasterWeapon`, `Ammo`, `Route`, `LootProfile`,
  `Meta`, `BuffSpells`, `BuffItems`, `ItemBuffMinutes`, `MaxSpellLevel`, `DebuffRecastSeconds`,
  `LogOff` (the log's types turned off), `MiniRemote` (the remote control), `HotkeyRun`,
  `HotkeyHigh`, `HotkeyMid`, `HotkeyLow`, `HotkeyLootContainer` and `MetaStateHotkeys` (the
  hotkeys).

Each character keeps its own; `/tank export` writes the ones that differ from the defaults, as
VTank writes its profiles.

**Settings profiles** (`.usd`): VTank's documentation gives the layout of a profile's settings
alone; what its other tables hold is read from the profiles its players published [pmr]. A profile
read gives the Tank:
- its settings (`Settings`), those the Tank has;
- its monster list (`MyMonsters`): each row's priority, attack, ring and streak, its debuffs (the
  void curses too), and its weapon and offhand, items named by their ids. A row's damage type, extra
  vulnerability and pet element are read as their defaults (8, 98 and 101, the numbers every row
  left at its default holds in the published profiles): the numbers of the others are not
  published;
- its consumables (`AssistItems`, kits, potions, foods, charges and stones; `GemFoodItems`, the
  gems, foods and beers that buff);
- its buffs added and left out (`ExtraBuffSpells`, `AntiExtraBuffSpells`), each by its family's
  first-level spell;
- its combat items (`BuffedItems`, VTank's Items tab): each by the item's id, those of this
  character (an id is another character's item in another's profile), never buffed where its
  spell is -1 (Add (No Buffs)).

A profile written holds each of these tables as the published ones do: the consumables by the kinds
they hold (0 a healing kit, 1 a health potion or food, 3 a stamina one, 6 a mana charge, 8 a mana
stone; a buff item with its spell), the buffs by their families' first-level spells the character
knows, a combat item by its id with the item buffs it gets (a weapon's or a caster's, by their
families' first spells) or -1, and the monster rows with columns of the plugin's own
(`DerethDamage`, `DerethWeapon`, `DerethOffhand`, `DerethExVuln`, `DerethPetDamage`) for what
VTank's numbers cannot carry, so a profile the Tank writes reads back the same. Not carried:
`ItemUseSpecifiers`, which no published profile fills.

**Not carried:** VTank's crafting [adv, log 0.3.0.9] (`AutoCraftItems`, `IdleCraftCount_*`,
`SplitPeas`, `SpellCompMin-*`, `UseSpecialAmmo`, `ArrowheadFletchDiffExcessThreshold`): VTank makes
kits, foods, ammunition and components from what it knows of the game's recipes, which its pages
name only by their results, and which the client does not hold. Set, they are kept for the run as
any of VTank's options this macro has not got.

## Trying it

Put a route in `routes/` and a loot profile in `profiles/`, log in, and:

```text
/tank route my-dungeon.nav
/tank profile my-loot.utl
/tank set Weapon Long Sword
/tank start
```

Everything it does has no gesture behind it, so each action needs a world that allows its domain
(combat, nav, loot, heal) without one; a world that publishes no policy allows them all. A domain
the world does not allow so waits, and the Tank says once why.

## VTank's documentation

The pages of the virindi.net wiki (`http://www.virindi.net/wiki/index.php/<page>`) the bracketed keys
name:

| key | page |
|---|---|
| std | Virindi_Tank_Standard_Options (the tabs, the monster columns, the route types and buttons) |
| adv | Virindi_Tank_Advanced_Options (each option, described) |
| opt | Options_List (the option names) |
| cmd | Virindi_Tank_Commands |
| meta | Virindi_Tank_Meta_System |
| mlm | Virindi_Tank_Monster_List_Matching |
| faq | Virindi_Plugins_FAQ (its Virindi Tank section) |
| log | Virindi_Tank_Changelog (by version) |
| main | Virindi_Tank (the feature list) |
| beg | BeginnerBundleGuide |
| fel | Virindi_Tank_Fellowship_Commands |
| huds | Virindi_HUDs (the VTank Remote Control: F, CF, FC, C and the lock) |
| vi2 | Virindi_Integrator_2 (its fellows) |
| vit | Virindi_Item_Tool (its buy and sell, and its buy list) |
| tips | VTClassic_Advanced_Tips (salvage rules) |

**[pmr]** is the settings profiles (`.usd`) in the zips of VTank's Public_Meta_Repository page on
the same wiki, whose tables show what a profile holds beyond its settings.

**[mag]** is the sample vendor profiles Mag-Tools publishes (`docs/Mag-Tools/Samples` of
https://github.com/Mag-nus/Mag-Plugins), each named after its vendor, with `S:` rules that sell and
`B:` rules that buy.
