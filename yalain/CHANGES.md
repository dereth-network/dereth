# Yalain changes

What each Yalain release brings for server operators and players, a few lines each.

Add a line under **Unreleased** when a change lands that an operator or a player would notice. Say
what they see, not how it was done.

## Unreleased

- First Asheron's Call 2 server: log in, make a character and play a starting area.
- Serves the whole world from the official world database, fetched with `yalain world fetch`.
- Serves worlds made in Athenaeum over the world database.
- New `reload` console command, to take database edits without a restart.
- Worlds can be listed on a Dereth Network hub with `--hub`.
- New `--federation` option links the world to the Dereth Network; the console gains `dn` blocks.
- New `shutdown` console command warns players for two minutes, then stops the server.
- New `--banned-names` option; character names are capitalized as in 2017.
- New `--game-time` and `--hold-clock` options set the time of day and keep it there.
- Clients get missing game data from the server.
- Stopping the server, Ctrl-C and service stops included, saves everyone in the world first.
- Open worlds check an account's password after its first login.
- Logging in again after a crash frees the old character in about twenty seconds.
- Logging off takes about nineteen seconds.
- The start area's quests, the training hall's included, can be started and finished.
- The quest journal keeps finished quests across logins, and quests can be cancelled.
- Timed quests fail when their time runs out, and quest-locked portals refuse entry.
- People say their lines: when used, with quests, to players nearby, and in a fight.
- Story shards play their scenes, and books can be read.
- Players can die and be resurrected, protected by the lifestone for a while after.
- Lifestone recall works, and Lugian and Tumerok lifestones bind players too.
- Walking into a portal takes you through it.
- Skills can be trained, raised and untrained.
- Special attacks work, and skills affect creatures, hit whole areas and toggle auras.
- Heals, buffs and quest blessings work, and show in the effects list.
- Buffs and gear that grant regeneration work while fighting or running.
- Bows shoot from afar and staves throw, as each weapon attacks.
- Creatures fight with their weapons, call their kin and use their own specials.
- Creatures turn on whoever hurts them most, heal and buff themselves and their kin, and feel slowing and armor debuffs.
- Some creatures shoot from a distance, and training dummies can be attacked.
- Creatures walk around walls, rocks and trees, and corpses lie down.
- Guards walk their patrol routes, and camps can set how their creatures move.
- Barrels and turrets appear, and rifts, turrets and orbs can be summoned.
- Creatures' idle emotes reach players nearby.
- Selecting or examining a creature, corpse or item shows its details, stats, effects and maker.
- Armor, clothing and jewelry can be worn, and others see what you wear and hold.
- Worn and wielded items give their bonuses, crafted ones included.
- A wielded weapon stays in hand, and skill waits carry over, after logging out.
- Items can be dropped on the ground and picked up again.
- Bags, rearranging the pack, selling to the money bag and Take All work.
- Transmute All keeps quest items.
- Potions, cards, charms and many more items work from the pack, and are refused when they can't be used.
- Saddles and Crystalline Tyrant Eggs put players on their mounts.
- Pets: summon a Ranger's Lasher or other class pets, name them and give them orders.
- Players can trade items with each other, and trades and shops mind a full pack.
- Market brokers show their catalogs, and players can sell items there and buy what others sell.
- Market doors play their scenes.
- The crafting page lists craft skills and recipes, and recipes can be made.
- Crafting earns craft experience, craft skills can be raised, and corpses can be butchered.
- Hunting knives wear out with use, and some corpses are too damaged to butcher.
- Delving stones summon resource nodes to mine, and hoard veins can be mined.
- Doors open and lock, keys open doors and chests, and chests hold loot.
- Surveyor's maps lead to their nodes, and glyphs open their vaults.
- Hoard keys open the Shaper's hoards to a whole fellowship, and the Sentries walk their rounds.
- Allegiances: swear to a patron, walk the allegiance's tree, pass experience up, and export the tree.
- Fellowships: make one, recruit, share experience and follow your fellows' health.
- Players can follow others.
- Aliases and inscriptions work, and tarot cards show their pictures.
- Vault and Dominion Center portals keep out players without the quest they need.
- Monsters open doors to chase players; summoned creatures can't.
- Friends, squelches and tells to Name@world now work, across worlds after `@dn on`.
- Kill experience is split by damage dealt, none goes to fighters a monster can't reach, and
  fellows out of level range still earn their own.
- Summon Lifestone and Summon Portal open a portal anyone can use for 30 seconds.
- Placed people and objects with nothing to say now answer when used, and faction shrines record your kingdom rating.
- Pieces of food heal health and vigor and wear out after five uses.
- Portal storms warn crowded towns and send some players away.
- Dye plants can be planted from seeds, watered, fertilized, pollinated and harvested.
- Heroes earn focus from kills; the console's `hero <name>` makes a character a hero.
- Kingdom stones, quest shrines and the Cauldron of Tanacha move their quests, and journals list what's left.
- Failed quests wait before they can be taken again, and some quests need a level or fail when you die.
- Characters on the Dereth Network appear in its directory.
- Items can be given to people and shrines, so quest hand-ins and the Hero Shrine work.
