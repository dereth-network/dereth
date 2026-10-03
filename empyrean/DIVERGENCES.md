# Deliberate divergences from ACE

Empyrean is a port of [ACE](https://github.com/ACEmulator/ACE) (at commit `47edade3`), and this is
the list of places where it deliberately behaves differently. The code names a row by its number
(`V336`). Each row has one of five kinds:

- **retail**: Empyrean follows the retail client and server, or the crates it shares with the
  Dereth client, where ACE does not;
- **arch**: forced by Rust or by the server's architecture, with no observable difference intended;
- **fix**: an ACE defect deliberately not ported;
- **brand**: Empyrean's own name, commands, sites and tracker where ACE named ACEmulator;
- **era**: a rule of an earlier era than the end of retail, chosen by `[era] profile` in
  `empyrean.toml`; the end of retail (the default) keeps ACE's behaviour.

A retired row was too small to note; its number stays so that code citing it still resolves.

This file is generated from the project's divergence register, which also records the detail and
the evidence behind each row. Edit the register, not this file.

| id | kind | divergence | tests |
|---|---|---|---|
| V1 | retail | Physics is the shared crates the Dereth client uses, which follow the retail client, not ACE's own physics. | - |
| V2 | arch | The dat files are read by the shared readers the client uses. | - |
| V3 | arch | World content comes from a pack file and characters live in SQLite; no external database server is needed. | - |
| V4 | fix | Console commands run on the world thread, which removes a data race. | - |
| V5 | arch | Landblock-group multithreading (off by default in ACE) is not ported; the world always runs serially. | - |
| V6 | arch | Physics collisions are reported as notices after each object's update rather than as callbacks inside it. | - |
| V7 | arch | Retired: a Rust keyword rename (`Self` to `Self_`) with no observable difference. | - |
| V8 | arch | Attribute lookups that ACE does by reflection are generated per attribute. | - |
| V9 | arch | Retired: a static field ACE never changes is a constant; no observable difference. | - |
| V10 | arch | A missing dat file stops the server at start instead of failing at first use. | - |
| V11 | arch | A dat record that is absent is reported as absent instead of as an empty object. | - |
| V12 | arch | The cell dat is always loaded. | - |
| V13 | arch | Logging uses the `log` crate and configuration errors are returned as errors. | - |
| V14 | arch | Every time read goes through an injected clock, so tests can control time. | - |
| V15 | arch | Random numbers come from a seeded generator, so a run can be reproduced. | - |
| V16 | fix | Text is formatted as en-US on every thread, whatever the host's locale. | - |
| V17 | arch | A malformed configuration (a duplicate key, a null where text is expected) is an error. | - |
| V18 | arch | Retired: .NET's DateTime kind is not modelled; ACE only compares times. | - |
| V19 | arch | Each session's random numbers are seeded and its network statistics are per session. | - |
| V20 | arch | Sessions are worked serially, in slot order, instead of in parallel. | - |
| V21 | arch | UDP binds one host without address reuse, and datagrams over 1024 bytes are dropped on every OS. | - |
| V22 | retail | Inbound packets go through the client's own stricter parser; at most 256 sessions. | - |
| V23 | arch | Retired: loot tables are public so other crates can use them; no observable difference. | - |
| V24 | arch | Retired: dictionary literals are static sorted maps with the same order; no observable difference. | - |
| V25 | arch | Retired: a Rust keyword rename (`crate` to `crate_`) with no observable difference. | - |
| V26 | arch | Retired: a loot-table check that only affects a log line sums in floats. | - |
| V27 | arch | Retired: C# `ref`, `out` and null become Rust references and options with the same values. | - |
| V28 | arch | A repeating action rebuilds its body on each pass; the order is unchanged. | - |
| V29 | arch | The delayed-action sequence counter belongs to the world, not the process. | - |
| V30 | arch | Timers read the tick's clock snapshot rather than the live clock. | - |
| V31 | arch | Retired: an invalid-delay warning goes to the log instead of the console. | - |
| V32 | arch | Lock parameters on the biota and property helpers are dropped, as the world is single-threaded. | - |
| V33 | arch | Retired: a landblock id hashes consistently with its equality, which ACE never relies on. | - |
| V34 | fix | A character's copy of its weenie's shared lists is copy-on-write, so changing a character never changes the cached weenie. | - |
| V35 | arch | Retired: helpers return copies instead of shared objects; the values are the same. | - |
| V36 | arch | A message's session state is checked on the world thread, when the network hands it over. | - |
| V37 | arch | A session is split into a transport half and a game half that each hold its state. | - |
| V38 | arch | Error and unhandled-opcode counters are added. | - |
| V39 | arch | Shutdown wakes each blocked socket receive before joining it. | - |
| V40 | arch | Retired: reading a position returns a copy; the values are the same. | - |
| V41 | arch | Retired: setting a position stores a copy; the values are the same. | - |
| V42 | arch | Retired: the workmanship getter's write is visible in its signature; no observable difference. | - |
| V43 | arch | A failing world-loop stage is logged and the loop continues instead of ending the world thread. | - |
| V44 | arch | The network is pumped at the top of each world-loop iteration. | - |
| V45 | arch | Shutdown is a state machine polled by the world loop rather than a thread of its own. | - |
| V46 | fix | Ctrl-C shuts down gracefully, saving players, instead of exiting at once. | - |
| V47 | arch | A missing configuration file means defaults, not an interactive setup. | - |
| V48 | arch | The cancelled-shutdown message shows UTC instead of local time. | - |
| V49 | arch | Content lookups return rows in the same order ACE's database returns them. | - |
| V50 | arch | Retired: the points-of-interest cache is returned as a sorted copy; no observable difference. | - |
| V51 | arch | Retired: database rows carry no back-references to their parents; no observable difference. | - |
| V52 | arch | Retired: concurrent caches are maps behind a mutex with the same behaviour. | - |
| V53 | arch | Character saves rewrite a biota's rows in one savepoint instead of tracking changes. | - |
| V54 | arch | Retired: lock parameters on the shard helpers are dropped in favour of owned snapshots. | - |
| V55 | fix | Save callbacks run on the world thread after the batch commits, and a failed commit stops the writer, so an acknowledged save is never lost. | - |
| V56 | arch | Each database read returns a fresh copy, and saving a character always upserts. | - |
| V57 | arch | Retired: a rename changes the snapshot and the world's own copy; the result is the same. | - |
| V58 | arch | Parallel loops run sequentially in a fixed order (an item, then its contents). | - |
| V59 | arch | Retired: a saved row's id is written back only to the database thread's copy; the same rows are written. | - |
| V60 | arch | The database's biota count is exact rather than estimated. | - |
| V61 | arch | The banned-accounts list shows UTC. | - |
| V62 | arch | Configuration and the clock are carried by the database backend so it can be tested. | - |
| V63 | arch | Retired: an unused conversion takes the content model's weenie type; ACE never calls it. | - |
| V64 | arch | The database schema is SQLite's, with ACE's text collation reimplemented and .NET ticks for times. | - |
| V65 | arch | A failing action or delay is logged and counted and the world goes on, instead of ending the process. | - |
| V66 | arch | The world pack's path is a configuration setting (`WorldPackPath`, default `./world.pack`). | - |
| V67 | arch | A missing or invalid world pack is logged and the server boots with no content, instead of retrying. | - |
| V68 | arch | Retired: a rotation-fix warning names the object by its stored name; it only affects a log line. | - |
| V69 | arch | Retired: object constructors take the object and a read-only view of the world; the result is the same. | - |
| V70 | arch | A generator becomes available when the clock reaches its time, not only after it, because the clock is frozen for a tick. | - |
| V71 | arch | Each generator profile holds its own copy of its settings, so capping one never writes back. | - |
| V72 | arch | Entering a cell is handled after a placement or update, on a cell change. | - |
| V73 | retail | Only the moving object reports a collision, as in the retail client's physics. | - |
| V74 | arch | The last physics update time is kept with the physics body. | - |
| V75 | arch | Retired: the destruction queue is keyed by handle; no observable difference. | - |
| V76 | arch | A duplicate object id is refused and logged instead of overwriting. | - |
| V77 | arch | The world is built on the world thread; start-up steps run there in ACE's order. | - |
| V78 | arch | A destroyed object leaves the world at once; values ACE reads from it later are captured first. | - |
| V79 | arch | The id manager reads the database directly at boot instead of queuing and waiting. | - |
| V80 | arch | Login and the player load run on the world thread in shard order, not in parallel. | - |
| V81 | arch | A session's game half is cleaned up once its transport is gone and its player is offline. | - |
| V82 | arch | Lower-casing ignores the host's culture. | - |
| V83 | arch | Removing an object destroys its physics body; what ACE later reads from the old body is kept until a new one exists. | - |
| V84 | log | An aborted landblock load is logged. | - |
| V85 | arch | The starter gear is compiled in rather than read from a file at start. | - |
| V86 | arch | Character creation draws from the seeded random generator. | - |
| V87 | arch | Server properties start filled with ACE's defaults and are refreshed by the world loop. | - |
| V88 | arch | Two property-manager edge cases that throw in ACE (a resync before start-up, a bad container setting) are harmless. | - |
| V89 | arch | Retired: enchantment registry entries are copies; the values are the same. | - |
| V90 | arch | Retired: vital enchantment caches are keyed by the vital, which is equivalent. | - |
| V91 | arch | Retired: spell sets are memoised per dat manager and spells hold copies; no observable difference. | - |
| V92 | retail | Landscape height comes from the shared crate's formula; no numeric difference was found. | - |
| V93 | arch/log | A missing starter-gear file aborts start-up before the world exists, and each landblock load is logged. | - |
| V94 | arch | Retired: creature attributes, vitals and skills are handles returning copies; the values are the same. | - |
| V95 | retail | Movement parameters use the retail client's defaults; gameplay moves use retail's parameters for each kind of move. | - |
| V96 | retail | ACE's two server-side additions to move-to (arriving "in range while closing", a heading snap after a corrective turn) are absent; retail's move-to applies. | - |
| V97 | retail | Landing or leaving the ground re-applies interpreted movement, and target updates are computed every step, as retail does. | - |
| V98 | arch | Motion-done and move-to notices are delivered right after the physics call that raised them. | - |
| V99 | arch | A missing movement manager or object is a no-op instead of an exception. | - |
| V100 | arch | Retired: a new player's items are staged in the world's store; the result is the same. | - |
| V101 | arch | A container's contents are generated by whoever inserts it, not by its constructor. | - |
| V102 | arch | Objects are removed explicitly from the store instead of being collected. | - |
| V103 | log | Retired: two armour and treasure debug lines go to the log instead of the console. | - |
| V104 | arch/log | Retired: spell formulas hold copies, projectile caches are per thread and diagnostics go to the log; no observable difference. | - |
| V105 | arch | Retired: a creature's damage history is held in a different struct; no observable difference. | - |
| V106 | arch | Loot generation returns the items and the caller puts them in the world. | - |
| V107 | - | Retired. | - |
| V108 | arch | Swapping loot tables at runtime (`LootSwap`) is not ported; the tables are fixed at build time. | - |
| V109 | file name | `/testlootgen` names its CSV files by the world clock's UTC time. | - |
| V110 | log | Retired: loot diagnostics name items by their stored name; it only affects log lines. | - |
| V111 | forced | A dropped item's slide is committed through the shared physics' public placement call. | - |
| V112 | arch | Merging a whole stack into another does not send a create message for the destroyed source. | - |
| V113 | arch | Inventory actions hold object ids; unused new split or give objects are removed from the world. | - |
| V114 | arch | Retired: a creature's body-part table is rebuilt from the cached weenie instead of a static cache; same result. | - |
| V115 | arch | Retired: skill arguments name their creature by id; no observable difference. | - |
| V116 | arch | Retired: the combat type is declared in another file; no observable difference. | - |
| V117 | arch | Asking whether a player with no physics body is jumping answers no instead of throwing. | - |
| V118 | retail | Attack hooks come from the shared animation crate. | - |
| V119 | forced | A new character's spellbook filters are stored as all on (16383), the value ACE's database reads back. | - |
| V120 | arch | Saving a character copies it into the session rather than sharing one object. | - |
| V121 | arch | A logged-off player and their possessions are removed from the world once the session lets go. | - |
| V122 | forced | Logoff log lines show UTC. | - |
| V123 | arch | A creature's attack target is held by id; a target gone from the world reads as none. | - |
| V124 | arch | Retired: the missing-attack-frames warning and the awareness range are kept per thread and computed per read; same values. | - |
| V125 | arch/log | Retired: a creature's return home uses the shared placement and its diagnostics go to the log. | - |
| V126 | ace | Superseded by V310. | - |
| V127 | retail | A server-side position update keeps the transition's velocity and collisions but does not commit its contact plane. | - |
| V128 | arch/retail | Requested positions are kept only for players, and physics steps use the shared crate's quanta. | - |
| V129 | retail/arch | The "colliding with environment" latch is not kept in the server's transition path. | - |
| V130 | - | Retired. | - |
| V131 | arch | Retired: queued emotes hold the object's id and a copy of its emote set; no observable difference. | - |
| V132 | arch | An emote target destroyed between actions reads as none from the next action on. | - |
| V133 | arch/log | A created treasure item that cannot be given is removed from the world, and debug output goes to the log. | - |
| V134 | arch | Console commands are looked up and run on the world thread; a chat command before start-up is dropped and logged. | - |
| V135 | arch/forced | A few console edge cases ACE throws or spins on (end of input, a bad setting) end quietly. | - |
| V136 | - | Retired. | - |
| V137 | - | Retired. | - |
| V138 | arch | Retired: the magic state takes the player as an argument and is always on one thread; no observable difference. | - |
| V139 | arch | Vendor items are held by id in the world store, and a bought service item is removed after use. | - |
| V140 | arch | Retired: vendor stack sizes and a new player's coin value are held and summed elsewhere; same values. | - |
| V141 | retail | A vendor's closeness check uses the shared crate's cylinder distance. | - |
| V142 | arch | Retired: a player's attack queue and combat table are set up on first use; same values. | - |
| V143 | arch | The projectile radius cache is per world thread, and a spent projectile leaves the world store. | - |
| V144 | - | Retired. | - |
| V145 | arch | Retired: a player's next use time lives in another struct; no observable difference. | - |
| V146 | arch | The squelch list is built during player construction; a player with no character record gets an empty one. | - |
| V147 | arch | Retired: the chat packet lives in the world crate and its send is two functions; no observable difference. | - |
| V148 | arch | Retired: a portal checked for use requirements is put in the store for that call only; same result. | - |
| V149 | - | Retired. | - |
| V150 | arch | Reading fellowship members from read-only callers skips dropped members, and the mana cost counts live members. | - |
| V151 | forced | Server text shows UTC times, server status reports what Rust can see, and the shard-cache line is not written. | - |
| V152 | forced | `/movetome` shows a general failure when placement fails, and `/telepoi` sorts names with its own culture-aware comparison. | - |
| V153 | arch | Objects that `/morph` and `/create` build go straight into the world store and are removed when unused. | - |
| V154 | arch | Retired: a player's possessions are loaded right after the player joins the world instead of in its constructor; same result. | - |
| V155 | arch | A destroyed object a container still lists stays in the world store, marked destroyed, until the container lets go. | - |
| V156 | arch | Allegiances are held by id in the world store and replaced on a rebuild. | - |
| V157 | arch/forced/log | Allegiance walks collect their nodes before acting, chat filters are read once, and names sort with a culture-aware comparison. | - |
| V158 | arch | A recipe never writes to an object it has destroyed; what its messages need is captured first, so they read as ACE's. | - |
| V159 | arch | Retired: a crafted item that does not fit is removed from the world and a recipe lookup writes nothing; same result. | - |
| V160 | arch | Retired: a door's motion check compares values, and an item a use cannot place is removed from the world; same result. | - |
| V161 | arch | Retired: confirmations are one struct with a type, and a trade's move-to holds the partner by id; same result. | - |
| V162 | arch | A recycled object id still held by an object is never reissued, so two objects never share one id. | - |
| V163 | arch/forced | `/loadalllandblocks` runs on the world thread, `/forcegc` only replies, and command-created objects are removed when unused. | - |
| V164 | forced | Content-browsing admin commands list in content order, `/setproperty` reports only the error type, and times are UTC. | - |
| V165 | - | Retired. | - |
| V166 | arch/log | Retired: chess matches live in a world store and pieces are tagged by kind; no observable difference. | - |
| V167 | arch | Each house is loaded once per id and reused, and a held house that left the world is reloaded. | - |
| V168 | arch | Reading a house (appraisal, its root) has no side effects, and a deleted guest's removal is saved with the house. | - |
| V169 | perf/arch | House ids are found only on landblocks holding a house, so start-up takes seconds instead of minutes. | - |
| V170 | arch | Changing an object's weenie class only logs, as content is read-only; a pet stops ticking once destroyed. | - |
| V171 | arch | The performance monitor and the dat-download handlers use an injected clock and configuration. | - |
| V172 | arch | The dat-download manager's work runs on the world thread once the world exists; its log line keeps ACE's place. | - |
| V173 | arch/forced | Database fix commands and offline tools read through the SQLite store with ACE's filters, in object-id order. | - |
| V174 | forced/arch | Database commands are deterministic: the emote hash is FNV-1a, there are no console waits, and times are UTC. | - |
| V175 | arch | A queued removal's messages are captured when queued; an object destroyed before the action sends nothing. | - |
| V176 | arch | A dying creature's equipped items stay in the store until it is destroyed, and a bodiless player knows no objects. | - |
| V177 | arch | Superseded by V333. | - |
| V178 | arch | Indoor cells are rebuilt from the physics land on each call instead of being cached for the process's life. | - |
| V179 | arch | Retired: a creature's wield list and treasure are created right after it joins the world; the same draws. | - |
| V180 | arch | Barrier entry is answered through the shared transition's entry-restriction hook, evaluated just before each move. | - |
| V181 | arch | Configuration is found as `empyrean.toml` (or `Config.js`) in the working directory or beside the executable, or named with `--config`. | - |
| V182 | arch | Objects a teleporting player sees are delayed as in ACE, but one destroyed during the delay is skipped. | - |
| V183 | arch | The event manager starts before the world loop instead of racing the first landblock loads. | - |
| V184 | arch | Retired: the spell-component table is built from the world's dats per call; same table. | - |
| V185 | retail | The direct-visibility probe sweeps without a projectile-target filter. | - |
| V186 | arch | Build information comes from compile-time variables set by a release build. | - |
| V187 | arch | Retired: an unused onboarding timer is a plain struct with its interval. | - |
| V188 | arch | An object's debug dump prints its header lines only, since there is no reflection. | - |
| V189 | arch | ACE's model-mesh code is not ported; ACE never calls it. | - |
| V190 | arch | Hot-swapping loot tables is not ported; the tables are compiled in. | - |
| V191 | - | Retired. | - |
| V192 | arch | Spell-cast recording stamps the tick's UTC time. | - |
| V193 | arch | Retired: an advocate item that cannot be given leaves the world store; same result. | - |
| V194 | retail | Missiles pass through every candidate as in the retail client, not ACE's two-way rule. | - |
| V195 | - | Retired. | - |
| V196 | arch | A projectile gone from the world counts as having reported its hit. | - |
| V198 | retail | Terrain height uses the shared crate's polygon plane; it matches ACE's to within 3 ULPs. | - |
| V199 | arch | Retired: a container's contents are generated when it joins the world; same items, same order. | - |
| V200 | arch | Retired: creating an object in the world draws its id from the world's guid manager; same result. | - |
| V197 | arch | Retired: deleting a selected object builds its delete message just before the delete; the bytes are the same. | - |
| V201 | arch | Fellowships and chess matches no one refers to are dropped from the world store at the next creation. | - |
| V202 | arch | A target destroyed between the swing and the strike reads as dead and takes no damage. | - |
| V203 | arch | An object whose world entry fails is removed from the world with its body and contents; its id is not recycled. | - |
| V204 | arch | An offline player holds its own copy of the biota and takes the player's final one at logoff. | - |
| V205 | arch | A wielded item takes a copy of its wielder's position rather than sharing it. | - |
| V206 | arch | A death's drop message is built from details remembered before the items are destroyed, so the death always completes. | - |
| V207 | arch | A missile launch queued by a creature destroyed since is dropped. | - |
| V208 | arch | The content importer stamps times from `empyrean-import --now`, so a pack build is reproducible. | - |
| V209 | arch | A failing SQL file stops the content build, which writes nothing. | - |
| V210 | arch | Content SQL runs against the content overlay, and a failing file changes nothing. | - |
| V211 | arch | Content files are imported in case-insensitive path order and written as LF SQL with no labels. | - |
| V212 | arch | Retired: the content writers are per thread and a generator's children are read before it is destroyed; same result. | - |
| V213 | arch | `/nudge` uses the shared placement and reports a general failure, and times are UTC. | - |
| V214 | arch | The content overlay and base SQL are configuration settings (`WorldOverlayPath`, `WorldBaseSql`, `WorldBasePatches`). | - |
| V215 | arch | GDLE and Lifestoned files are loaded in path order. | - |
| V216 | arch | Lifestoned files with equal creation times load in path order. | - |
| V217 | arch | A GDLE event or spell with no name imports with an empty name. | - |
| V218 | arch | Exported quest names sort in ordinal order. | - |
| V219 | arch | A player whose body has left the world reads as not jumping and not moving. | - |
| V220 | arch | A missile weapon destroyed while its projectile flies stays in the store until the projectile lands. | - |
| V221 | fix | A character with no level stops the level-up loop instead of hanging the world thread. | - |
| V222 | arch | A rare sine and cosine fallback is correctly rounded, so every host gets the same bits. | - |
| V223 | arch | The trajectory solver uses portable `pow`, `acos` and `cos`, so every host gets the same bits. | - |
| V224 | retail | Burden and experience-rank rules come from the shared rules crate. | - |
| V225 | retail | Vendor prices are worked out as the client's window shows them: the unit value times the count, in double precision. | `rule3_vendor_prices_agree_with_dere_rules` (no longer ignored), `*_cost_is_retail_within_ace_rounding`, `a_stack_is_priced_per_unit_times_count_as_the_client_does` |
| V226 | retail | A fellowship's experience shares are the client's table, so a full fellowship shares 0.3111 each, not 0.30. | - |
| V227 | retail | Character names are checked against the taboo table with the retail glob matcher. | `the_shared_taboo_matcher_refuses_what_aces_regex_refuses` |
| V228 | ace | Superseded by V311. | - |
| V229 | ace | ACE's equip-mask composites are kept; the Gear Knight wield check uses retail's clothing and armour set. | - |
| V230 | retail | Jump height and jump speed are computed as the retail client computes them. | - |
| V231 | retail | Encumbrance capacity saturates instead of wrapping. | - |
| V232 | retail | Strings on the wire are Windows-1252 with a byte count, as the client sends them, so accents and curly quotes survive. | `read_string16l_reads_the_clients_packed_string` (empyrean-common, empyrean-world), `passwords_of_any_length_read_as_the_client_sends_them` |
| V233 | retail | Jumping is blocked during the retail client's spell windups (Purple 01–10, Sanctuary and the cast telegraph). | - |
| V234 | retail | A public string update writes the object before the property, so renames show at once instead of after a relog. | `rule3_public_update_string` (no longer ignored); the identity tests expect ACE's bytes with the two dwords swapped (`protocol_identity::retail_ruled`) |
| V235 | retail | Superseded by V293. | - |
| V236 | retail | Known-type data ids use the two-byte form only below 0x4000, as the client's packer does. | - |
| V237 | retail | A data id the client cannot express is logged as a content error and sent as none; oversized counts are logged. | `messages::a_known_type_data_id_the_retail_packer_refuses_is_logged_and_sent_as_none`, `messages::a_clothing_sub_palette_held_as_an_offset_reaches_the_client_as_its_palette`, `serialization::object_descriptions_match_aces_writers` (ACE's synthetic vector objects, icon 0x7FFF and palettes and textures 0x1001–0x1003, 0xABCD, now refuse none) |
| V238 | retail | The channel list is one count-prefixed list for the highest role, and an empty one for a player with none. | `channel_index_writes_exactly_one_list` |
| V239 | retail | Turbine chat extents and string lengths are the client's exact layout. | `chat::the_servers_turbine_chat_is_retails_layout`, every TurbineChat a empyrean-testkit client receives (`decode::assert_reencodes`); the identity tests expect ACE's bytes with both extents 8 smaller (`protocol_identity::retail_ruled`) |
| V240 | retail | An account-banned message always carries its reason, empty if none, so the ban dialog appears. | `account_banned_counts_seconds_from_the_tick_clock` |
| V241 | retail | A string of 65,535 units or more is written in the client's long form. | `a_long_string_is_written_in_the_clients_long_form` |
| V242 | fix | A taken character name is answered once, not twice. | `a_taken_name_is_answered_name_in_use_once` (empyrean-world), the testkit chargen scenario |
| V243 | fix | An out-of-range character template is answered as corrupt instead of leaving the client waiting. | `an_out_of_range_template_is_answered_corrupt` |
| V244 | retail | Enlightenment adds to skills and maximum health only when it is positive, as the client computes it. | `creature_and_player_stats_match_ace`, `enchanted_stats_match_ace` (negative cases expect the gated values), `rule3_player_stats_agree_with_dere_game` |
| V245 | retail | Luminance skill augmentations add only when positive, so a negative count cannot wrap a skill. | as V244 |
| V246 | retail | The burden bonus from augmentations is capped at five, as the client computes it. | `burden_arithmetic_matches_ace`, `rule3_encumbrance_capacity_above_five_augs` (no longer ignored) |
| V247 | retail | Pack and container capacities are read signed, as the client reads them: 255 is unlimited and 128–254 leaves no room. | `rule3_main_pack_free_slots_agree_with_dere_game` now agrees for every byte |
| V248 | retail | A character name outside 1 to 32 characters is refused as corrupt. | `a_name_outside_one_to_thirty_two_characters_is_answered_corrupt` |
| V249 | retail | Character-creation skill credits are counted as the creation wizard counts them, and an unnamed skill is refused. | `skill_zero_and_unnamed_skills_cannot_refund_credits` |
| V250 | retail | Of two equal enchantments in one category, the later wins, as retail's registry chooses. | `rule3_equal_start_time_tie` |
| V251 | retail | Additive and multiplicative enchantments of one category compete with each other, as in retail. | `rule3_mixed_category` |
| V252 | fix | Retired: ACE has since fixed the same defect, so the code is a plain port again. | - |
| V253 | fix | Each strike of a multi-strike monster swing uses its own body part. | `each_strike_of_a_swing_uses_its_own_body_part` |
| V254 | retail | Delete-object and teleport messages are padded as Turbine's server padded them; the bytes are unchanged. | - |
| V255 | retail | The allegiance update-done event follows each member's own age update and the login update, as retail sent it. | `allegiance_update_done_rides_the_members_own_age_update`, `update_request_login_notices_and_offline_passup` |
| V256 | retail | A fellowship full update carries the fellowship locks table, as retail's did; the bytes are unchanged. | `the_full_update_decodes_with_dere_proto` |
| V257 | retail | Every move-to uses walk/run threshold 15 and the retail flag word for its kind of move. | `retail_move_to` |
| V258 | fix | A new fellow's update carries the share-loot flag in its share-loot field. | - |
| V259 | fix | Stamina and mana ticks flag a fellow vital update, so fellows' bars no longer go stale. | `regeneration_ticks_match_ace` |
| V260 | fix | An object returning to view cancels its pending forget, so its updates keep coming. | `an_object_back_in_view_is_taken_off_the_forget_queue` |
| V261 | fix | Allegiance chat boots and gags survive an allegiance rebuild (until a restart). | `chat_gags_and_boots_survive_a_rebuild` |
| V262 | fix | An approved vassal who swears leaves the approved list. | `an_approved_vassal_who_swears_leaves_the_approved_list`, `locking_approving_and_banning` |
| V263 | fix | Booting an offline allegiance member completes even before the allegiance has been rebuilt. | `booting_an_offline_member_never_linked_since_start_up` |
| V264 | fix | A sender squelched by several fellows sees their fellowship chat line once. | `a_sender_squelched_by_fellows_gets_one_fellow_echo` |
| V265 | fix | A monarch speaking on the patron or co-vassals channel gets an echo instead of a dropped line. | `a_monarch_on_the_patron_or_covassals_channel_gets_its_echo_and_no_one_hears_it` |
| V266 | fix | A gagged player's tell gets the gag message and no "You tell" echo. | `talk_direct_echo_is_withheld_from_a_gagged_sender_but_kept_for_a_squelched_one` |
| V267 | retail | Client packet keys are checked with the client's own receive window: each sequence takes the next key, and a duplicate is refused. | `net::client_checksum_keys::login_and_play_complete_at_5_percent_loss`, `net::client_checksum_keys::login_and_play_complete_at_10_percent_loss`, `a_nak_and_its_retransmission_decrypt_under_the_parked_key`, `a_damaged_packet_keeps_its_key_for_the_resend`, `accept_parks_the_key_of_a_failed_packet_for_its_resend` |
| V268 | fix | A repeated login request from the same session no longer aborts the login. | `a_repeated_login_request_does_not_abort_the_login`, `another_login_from_the_same_endpoint_is_not_a_copy`, `a_lost_connect_request_is_recovered_by_the_next_login_request`, `net::client_checksum_keys::login_and_play_complete_at_5_percent_loss`, `net::client_checksum_keys::login_and_play_complete_at_10_percent_loss` (the login under loss, reordering and duplication, answered 2.5 s late) |
| V269 | fix | A wrong client version (or a shutdown before login) is refused in the form a client can read before connecting, so it shows the version dialog. | `a_wrong_client_version_is_refused_in_the_form_a_client_reads_before_connecting`, `shutdown_disconnects_every_session` |
| V270 | fix | Every client header section is parsed and hashed, so an unexpected section no longer drops the whole packet. | `net::optional_header_sections::an_echo_response_beside_an_ack_and_a_fragment_is_taken`, `net::optional_header_sections::a_net_error_disconnect_ends_the_session`, `net::optional_header_sections::every_client_sent_section_verifies_on_a_session`, `net::optional_header_sections::every_section_kind_the_shared_writer_builds_verifies_on_the_server` |
| V271 | fix | A message whose length is a multiple of 448 gets a full last fragment, so datagrams stay retail-sized. | `message_fragment_counts_and_tail_sizes` |
| V272 | fix | Damage and drain chat lines report the amount taken, not the amount rolled. | `a_drain_bolt_that_empties_the_pool_reports_what_it_took` |
| V273 | retail | Armour with no nether multiplier counts as 1.0 against nether damage, as retail's appraisals showed. | `armour_without_a_nether_multiplier_counts_as_one_against_nether` |
| V274 | retail | A member who left a locked fellowship can be recruited back for 15 minutes, as the lock message says, not 10. | `a_locked_fellowship_takes_back_only_recent_departures` |
| V277 | retail | A player's age updates every 4 to 6 seconds, drawn afresh each time, as retail sent them. | `allegiance_update_done_rides_the_members_own_age_update`, `enter_world::the_age_is_sent_every_four_to_six_seconds_from_the_first_tick` |
| V276 | retail | Equal-power enchantments rank by start time, then the later entry; level-8 auras and set spells get no special key. | `rule3_level8_aura_tie_break`, `rule3_equal_power_cross_set_tie`, `equal_power_cross_set_tie_goes_to_the_later_entry`, `level_8_aura_self_spells_follow_the_generic_rule` |
| V278 | retail | A player forgets an object 21 seconds after it leaves view, and one returning after that is created afresh. | `an_object_out_of_view_is_forgotten_at_21_seconds_not_before`, `an_object_back_in_view_is_taken_off_the_forget_queue`, `objects_leaving_view_are_queued_then_forgotten_after_21_seconds` |
| V279 | retail | The fellowship lock table is written in hash-bucket order, as retail wrote it. | `locks_are_written_in_bucket_order_and_held_order_within_a_bucket`, `a_locked_full_update_from_the_retail_captures_round_trips`, `structures_match_aces_writers` |
| V280 | retail | A player's heartbeat comes every 4 to 6 seconds, and health regeneration and per-beat timers scale with the time since the last beat. | `stats::a_players_heartbeat_comes_every_four_to_six_seconds`, `stats::a_players_health_step_scales_with_elapsed_and_stamina_and_mana_do_not`, `stats::a_players_average_regeneration_matches_aces_fixed_five_seconds`, `stats::a_players_per_beat_timers_track_real_elapsed_time`, `allegiance_update_done_rides_the_members_own_age_update`, `enter_world::the_age_is_sent_every_four_to_six_seconds_from_the_first_heartbeat` (renamed from V277's `..._from_the_first_tick`); `stats::regeneration_ticks_match_ace` replays ACE's vectors at 5 s elapsed |
| V281 | retail | The allegiance update request's on/off flag is honoured as the player's subscription to allegiance updates. | `allegiance::the_ticks_allegiance_update_done_follows_the_members_subscription` Superseded in part by V289: retail had no periodic AllegianceUpdate; a changed view is sent on the tick in place of the Done |
| V282 | retail | The server re-sends the connect request every second until the client answers, and ignores repeated login requests. | `the_connect_request_is_re_sent_every_second_until_the_connect_response`, `the_connect_request_re_sends_stop_at_the_auth_timeout`, `a_lost_connect_request_is_recovered_by_the_server_timer`, `a_repeated_login_request_does_not_abort_the_login`, `net::client_checksum_keys::login_and_play_complete_at_5_percent_loss`, `net::client_checksum_keys::login_and_play_complete_at_10_percent_loss` |
| V283 | retail | The server accepts client packets from sessions numbered 256 and above. | `a_client_id_of_300_is_accepted_by_the_server` (empyrean-net `sections`), `only_the_client_parser_limits_rec_id`, `a_server_accepts_any_client_rec_id`, `rec_id_must_fit_the_receiver_table` (dereth-transport) |
| V284 | retail | A tell to a player who squelches the sender is echoed normally with no "has you squelched" notice. | `talk_direct_echo_is_withheld_from_a_gagged_sender_but_kept_for_a_squelched_one`, `a_character_squelch_is_added_widened_narrowed_and_removed_with_aces_messages`, `an_account_squelch_blocks_every_character_of_the_account` |
| V285 | retail | Allegiance pass-up from a vassal uses retail's formula, including time sworn, with no cap on loyalty. | `stargren_tithes_as_recorded` (364,000,000 XP tithe 227,030,934), `russet_tithes_as_recorded` (160,000 XP tithe 80,618), `effective_loyalty_rounds_to_nearest`, `effective_loyalty_is_not_capped`, `pass_through_is_five_and_a_half_percent_of_effective_loyalty`, `the_chain_passes_through_at_the_retail_rate`, `the_hierarchy_record_carries_the_times_sworn`, `do_pass_xp_matches_ace` (ACE's recorded vassal tithe kept as the record on the shared direct step), `swear_pass_xp_relog_and_break` (empyrean-testkit: the stamps saved, the record's age after a relog) |
| V286 | retail | Object updates and deletes go to every player in the object's landblock and its neighbours, as retail broadcast them. | `an_update_reaches_every_player_in_the_3x3_landblocks_known_or_not`, `a_create_broadcast_stays_with_the_players_who_know_the_object`, `a_delete_reaches_players_in_the_reach_who_never_knew_or_forgot_the_object`, `a_ranged_broadcast_is_still_distance_limited_over_the_reach`; re-pinned: `a_player_who_forgot_the_object_still_gets_its_delete` (was `a_player_who_never_saw_the_object_hears_nothing`) |
| V287 | retail | A player is created the objects in a round window of outdoor cells around them (and in the rooms they can see), as retail decides. | `the_outdoor_create_set_is_the_round_cell_window_across_landblock_edges`, `rooms_seen_outside_are_created_at_landblock_reach_with_the_diagonal_near_side_test`, `cellars_and_dungeons_follow_the_visible_cell_list`, `an_object_in_the_window_beyond_112_5_m_is_created`, `an_object_walking_into_a_standing_players_window_is_created`, `a_monster_outside_the_window_still_targets_and_is_in_the_players_view`; re-pinned: `a_player_sees_objects_in_ace_order_and_the_clamp_holds_back_far_ones` (2 creates, was 3), empyrean-testkit `teleport_arrival` (`expected_in_view` is the create set, departures by the known objects; real-content, not run here: no `world.pack`) |
| V288 | retail | A client's request to redescribe an object it knows is answered with a fresh create message, as retail answered. | `a_known_object_is_answered_with_a_create_to_the_asker_only`, `an_object_never_sent_is_not_answered_even_in_the_players_landblock`, `a_forgotten_object_is_not_answered`, `held_contained_wielded_and_traded_items_are_answered` (empyrean-world `physics`), `asking_for_a_known_object_brings_its_create_back` (empyrean-testkit `enter_world`: the wire round trip through the test client) |
| V289 | retail | A changed allegiance view is sent as an allegiance update on the heartbeat, in place of the update-done. | `allegiance::a_subscribed_members_changed_view_replaces_the_ticks_done_with_the_update`, `allegiance::an_unsubscribed_member_is_pushed_changes_off_the_tick_at_most_every_30_s`, `allegiance::an_unnamed_allegiances_name_time_is_the_current_time` An unnamed allegiance also sends an empty name, as retail did (ACE sent the monarch's); the client only stores the field |
| V290 | fix | A client's dat iteration list is read as the client writes it, and a file of unknown type no longer ends the session. | empyrean-world `ddd::the_iteration_list_is_read_as_the_client_writes_it`, `ddd::the_iteration_file_is_sent_with_the_type_the_client_gives_it`, `ddd::get_missing_iterations_and_begin_ddd_match_ace`; empyrean-testkit `ddd::a_client_with_only_the_latest_iteration_is_patched_to_the_end`, `ddd::a_client_with_only_the_latest_iteration_is_booted_when_patching_is_off`; dereth-client `dat_patch_apply::the_servers_iteration_list_counts_as_delivered_and_is_not_written`. |
| V291 | fix | Treasure carried in a creature's inventory leaves it before going onto the corpse, so looted items stay live. | `content::loot::world_treasure::inventory_treasure_moves_to_the_corpse_and_outlives_the_creature` (unit), `content::world_pack_corrections::real_content::inventory_treasure_creatures_leave_live_loot_on_their_corpses` (real content, the five weenies); both failed before the fix (the corpse's items were destroyed) |
| V292 | fix | Gear Knights and Tumeroks trained in Light Weapons get their starting training weapon. | `the_starter_gear_table_is_what_aces_deserializer_builds` |
| V293 | retail | Attack notifications carry eight-byte conditions, as both ACE and retail send them. | - |
| V294 | retail | The contract tracker message is padded to retail's 48 bytes with zeros. | - |
| V295 | fix | The account's house is found with the same owner lookup the login uses. | empyrean-testkit `housing::an_alt_recalls_to_the_account_house` (an alt recalls to the account's house after a normal login; with its slot emptied the lookup still finds the house, which failed before the fix; with `house_per_char` on, none; an account with no house, none) |
| V296 | retail | Giving or trading a unique the receiver cannot hold gets the client's own "You cannot pick up more of that item!" error. | `add_to_trade_refuses_summoned_pets_uniques_and_missing_items` |
| V297 | fix | Awarding skill points raises the skill by that many ranks. | `award_skill_points_raises_the_skill_by_that_many_ranks` |
| V298 | fix | A turned portal with a relative destination sends players out with the destination's whole rotation. | `a_turned_portal_takes_its_relative_destinations_whole_rotation` |
| V299 | retail | Allegiance pass-up to a patron uses retail's formula, so the patron's share grows with its vassals' time sworn. | `effective_leadership_fits_the_recorded_sessions` (13 patrons, e.g. Ferah Palacost L 240 -> E 321, Celeth L 145 -> E 177), `effective_leadership_is_not_capped` (Archie Summons L 448 -> E 515; 8 vassals count as 4), `the_patron_gain_rounds_from_the_tithe` (12 single tithes to the unit, e.g. R(321) x 6,153 = 4,604), `cheleth_passes_through_per_event` (received 4,384,998 + 4,631,968, tithed 97,141 at Loyalty E 57), `the_chain_passes_through_at_the_retail_rate`, `do_pass_xp_matches_ace` |
| V300 | retail | The jump action is read in the client's 56-byte layout. | `action_decode::a_jump_reads_the_clients_56_byte_pack_and_nothing_after_it` (the 56-byte body decodes extent, velocity, position and sequences, ends exactly at the body's end, and the dispatch makes that one read) |
| V301 | retail | `/age` and `/birth` read the target as an object id and always answer for the player asking. | `players::character_options_and_queries::query_age_answers_with_the_age`, `players::character_options_and_queries::query_birth_answers_with_the_date_of_birth` (targets 0, 0x50000002 and 0x50000103, the last unanswered before) |
| V302 | retail | Turbine chat reads the client's packed character count, including its four-byte form. | `chat::the_turbine_chat_length_reads_the_clients_packed_form` (0, 0x7F, 0x80, 0x3FFF, 0x4000, 0x4001, 0x12345 against dereth-protocol's writer), `chat::a_general_message_of_0x4000_characters_or_more_reads_whole` |
| V303 | retail | The login request's "account to log in as" is read only when its flag is set. | empyrean-net `net::login_request::account_to_login_as_is_read_only_when_the_flag_says_it_is_there` (without the flag and the field the request parses; with both, the field is read and the password still reads right) |
| V304 | fix | Deleting a character the client cannot find in its list is refused instead of going unanswered. | empyrean-testkit `login::a_delete_of_a_slot_outside_the_list_is_refused` (slot -1: the CharacterError Delete and nothing else; no character marked) |
| V305 | fix | A new object's physics body takes the weenie's stored state bits (missile, align-path, cloaked and the rest), and a static weenie gets a static body. | `a_new_body_gets_the_calculated_physics_state` |
| V306 | retail | An en passant capture is reported to the mover as en passant. | empyrean-testkit `chess::an_en_passant_capture_is_reported_as_en_passant`, `chess::blacks_en_passant_capture_is_reported_as_en_passant` |
| V308 | fix | A monster past 112.5 m targets a player that already knows it. | `server_physics::past_the_initial_clamp_a_monster_targets_only_a_player_that_knows_it` |
| V309 | fix | A player's first legacy chess game is counted. | `content::chess::lifecycle::a_legacy_game_counts_a_first_game_as_lost` |
| V310 | retail | Cylinder distance is the shared crate's, so a body directly below another on a ledge is the gap away, not touching. | `a_monster_directly_below_a_player_on_a_ledge_is_the_gap_away`, `cylinder_distance_is_the_shared_crates` |
| V311 | retail | Chess follows the client's rules: illegal moves are refused as the client refuses them, checkmate ends the game, and the AI plays for its own side. | empyrean-world `chess::the_fools_mate_is_checkmate`, `chess::castling_through_an_attacked_empty_square_is_refused`, `chess::castling_rights_follow_the_rooks`, `chess::undo_puts_back_the_side_to_move_the_castling_rights_and_a_promoted_pawn`, `chess::an_ai_playing_white_takes_a_free_queen`, `chess::the_ai_plays_only_the_clients_legal_moves` (both sides against the client's rules at every ply); the ACE replays `chess::chess_logic_games_replay_ace`, `chess::chess_logic_games_directed_replay_ace`, `chess::chess_logic_scripted_replay_ace` (ACE's record while it agrees with the client's rules, the client's rules at every ply) and `chess::chess_ai_complex_and_minimax_on_aces_positions`; empyrean-testkit `chess::two_players_play_the_fools_mate_and_the_game_ends_in_checkmate`, `chess::an_illegal_move_is_refused_with_the_clients_reason` |
| V312 | retail | `/age` answers in retail's form: months, days, hours, minutes and always seconds. | `dispatch::calculate_age_message_is_retails` (every full answer quoted from the captures, at the seconds it spells out), `players::character_options_and_queries::query_age_answers_with_the_age` |
| V313 | retail | Nine ring, wave and bomb weenies, and a few other projectiles, fly in retail's physics state; the content is corrected as it is read. | `a_wave_piece_flies_in_retails_live_state`, `dark_vortex_and_lightning_volley_fly_as_on_retail`, `the_nether_projectiles_have_retails_gravity`, `corrections::tests::*`; real content: `launched_projectiles_carry_retails_in_flight_state` |
| V314 | retail | A spell projectile keeps the align-path and scripted-collision bits its weenie stores for flight, as retail flew them. | `a_ring_piece_keeps_its_stored_scripted_collision`, `a_projectile_stored_without_align_path_keeps_it_off`; real content: `launched_projectiles_carry_retails_in_flight_state` |
| V315 | fix | Buying or selling is refused when the vendor is closed, out of reach, or outside its value range. | empyrean-testkit `vendors::a_vendor_out_of_reach_closed_or_out_of_its_value_range_is_refused` |
| V316 | fix | A monster's item that cannot be worn goes back into its inventory, so it reaches the corpse. | `containers::a_monsters_armor_that_cannot_be_worn_stays_in_its_inventory` |
| V317 | fix | The moving-target ballistic solver tries only real roots, so a missile never gets a NaN aim. | `server_physics::the_moving_target_solver_reads_only_real_roots`, `server_physics::trajectory_ballistic_solvers_match_ace` |
| V318 | fix | Rotating to face an object on exactly the same spot keeps the rotation instead of making it NaN. | empyrean-testkit `trade::opening_a_trade_on_the_partners_exact_spot_keeps_the_rotation` |
| V319 | fix | Chat listen filters and disable switches apply to the room the message goes to, and an Olthoi player leaving the Olthoi channel is told. | `chat::a_by_name_request_is_routed_by_its_chat_type`, `chat::an_olthoi_player_is_told_it_left_the_olthoi_channel` |
| V320 | fix | Squelching a character already squelched through an account squelch keeps the account squelch, and a one-channel unsquelch is saved. | `chat::an_account_squelch_blocks_every_character_of_the_account`, `chat::a_character_squelch_is_added_widened_narrowed_and_removed_with_aces_messages` |
| V321 | fix | The resend cache's age wraps by a full 65,536 seconds. | empyrean-net `net::timers_resend_and_routing::the_resend_cache_age_wraps_by_a_full_turn` |
| V322 | fix | A deleted guest is left out of a house's access list, and revoking an offline player's permission is answered. | empyrean-testkit `housing::a_deleted_guest_is_left_out_of_the_access_list`, `death_xp::revoking_a_permit_of_a_player_not_online_is_answered` |
| V323 | fix | A blow or death that lands after a player's logoff save is still saved. | `object_lifetime::release_hands_the_offline_player_the_players_final_biota` |
| V324 | retail | The shared physics treats a restricted cell with no gate keeper as closed without running the restriction handler, as the client does. | dereth-physics `host_seams::an_absent_gate_keeper_closes_the_cell_without_the_restriction_handler` |
| V325 | fix | A target that has logged off is no target: a spell launches forward with no target and a missile is not fired. | empyrean-world `monster_ai::a_missile_monster_does_not_shoot_at_a_target_that_left_the_world`, `monster_ai::a_missile_monster_does_not_release_at_a_target_that_left_during_the_windup`; empyrean-testkit `player_creature_combat::no_arrow_is_fired_at_a_target_that_left_the_world_during_the_windup`, `spellcasting::a_bolt_at_a_logged_off_player_flies_straight_ahead`, `logged_off_targets::a_missile_at_a_logged_off_player_aims_at_the_bodys_cell_zero_frame` |
| V326 | retail | The shared chess rules follow the retail client's order of refusals, its checkmate search and its en passant tests. | dereth-rules `chess::castling_refusals_come_in_the_clients_order`, `chess::the_clients_refusal_codes_for_a_move_a_piece_cannot_make`, `chess::an_en_passant_capture_that_opens_the_kings_rank_is_a_self_check`, `chess::a_check_only_an_en_passant_capture_answers_is_checkmate`, `chess::a_check_is_answered_by_a_block_a_capture_or_a_step`; dereth-client-model `minigame::a_promotion_keeps_the_pawns_check_bits_and_adds_the_queens`; empyrean-world `chess::castling_rights_follow_the_rooks` (a castle towards an opponent's bishop on the rook's corner: `BadMoveWouldCollide`); empyrean-testkit `chess::an_illegal_move_is_refused_with_the_clients_reason` (`BadMoveDirection`) |
| V327 | retail | The fletching item-type bit is the client's 0x1000000, and eight weenies take the item type retail sent. | empyrean-entity `shared::shared_sets::item_type_matches_the_rules_item_type` (no difference left), `entity::enum_vectors::enum_alias_order_and_names_match_dotnet`; empyrean-command `commands::developer_commands::echoflags_matches_ace`; empyrean-content `corrections::tests::the_brewmasters_pieces_and_two_neighbours_read_with_retails_item_type`; empyrean-world `death_xp::death_items_sort_halve_and_draw_in_aces_order`, real content `content::world_pack_corrections::real_content::corrected_items_are_created_with_retails_item_type` |
| V330 | fix | With `persist_movement` on, a motion in an unchanged stance keeps the previous sidestep and turn. | `creature_combat::persisted_movement_keeps_the_sidestep_and_turn_when_the_stance_is_unchanged` |
| V331 | retail | Target-selection and combat eat and drink motion commands use the final client's numbering. | empyrean-entity `shared::shared_equality::motion_command_names_and_values_match_the_client_table` (every name both sides share has the client's id), `shared::shared_sets::motion_command_matches_the_client_table` (only `Invalid` differs); empyrean-world `serialization::a_motion_items_raw_index_reads_as_the_clients_command` |
| V332 | fix | The server's own movement simulation runs a player at their encumbered run rate. | `stats_in_the_world::inq_run_rate_applies_the_players_burden_as_get_run_rate_does` |
| V333 | retail | `/birth` prints US Eastern time with the 2007 daylight-saving rule, whatever the host's zone. | `players::character_options_and_queries::query_birth_prints_us_eastern_with_the_2007_dst_rule_for_every_year` (every quoted answer, the four gap births, the changeover hours), `query_birth_answers_with_the_date_of_birth` |
| V334 | retail | A house's restriction effect is sent as the description's script and no object sends its default script there. | empyrean-world `serialization::the_descriptions_script_is_data_id_44_and_the_default_script_data_id_30`, `housing::a_houses_create_object_carries_its_restriction_effect_as_the_description_script`, `serialization::object_descriptions_match_aces_writers` (transformed), real content `pwd_mirror` (the data-id-44 category gone) |
| V335 | retail | Nalicana (wcid 43398) has retail's icon. | empyrean-content `corrections::tests::nalicana_reads_with_retails_icon`; real content `content::world_pack_corrections` (every correction matches the pack) |
| V336 | retail | A ranged monster out of range runs a random distance towards its target and attacks from there, as retail's did; it is a server option, on by default. | `monster_ai::a_missile_monster_beyond_its_range_closes_to_a_drawn_distance_and_shoots`, `a_caster_beyond_its_spell_range_closes_to_a_drawn_distance_and_casts`, `with_the_option_on_a_melee_monster_still_chases_stickily`, `spell_range_is_uncapped_with_the_option_and_capped_at_seventy_five_without`, `a_missile_monster_keeps_its_range_turns_and_shoots` (within range: no move); option off: `with_the_option_off_a_missile_monster_beyond_its_range_switches_to_melee`, `with_the_option_off_a_caster_beyond_its_spell_range_chases_stickily`; `server_properties::default_tables_match_aces_in_order_with_descriptions` (the added option after ACE's) |
| V337 | retail | Default physics scripts stored in the old numbering are shifted to the end-of-retail numbering when weenies are read. | empyrean-content `corrections::tests::the_play_script_shift_rule_reads_retails_default_script`, `the_play_script_shift_rule_keeps_to_what_was_observed`; real content `content::world_pack_corrections::real_content::every_recorded_script_shift_is_read_without_changing_the_pack` (the 132, and the finding's wcids) |
| V338 | retail | Four emote motions stored in the old numbering are shifted to the end-of-retail numbering, and nine monsters' taunt plays retail's level-up script. | empyrean-content `corrections::tests::the_emote_motion_rule_reads_retails_command`, `the_emote_motion_rule_covers_a_style_and_counts_what_it_changes`, `the_taunt_scripts_read_with_retails_level_up`, `a_taunt_entry_is_stale_when_its_action_changed_or_is_gone`; empyrean-world `turning_and_position_broadcast::a_corrected_emote_motion_is_the_command_broadcast`; real content `content::world_pack_corrections::real_content::every_recorded_emote_correction_is_applied_and_other_fields_are_preserved` (the 4 values, the 9 entries, the unchanged rows and data id 44) |
| V339 | brand | Player-visible text that named ACE (the login welcome, the dat warnings, the help texts, the version line) names Empyrean and dereth.network. | empyrean-testkit `small_handlers_in_world::friends_old_and_server_version_answer_with_a_line` (welcome, FriendsOld, version line), `ddd::a_client_missing_iterations_is_booted_when_patching_is_off`, `ddd::a_client_with_newer_dats_is_booted`, `advocate_account_help_commands_in_chat::a_player_reads_the_help`, `commands::player_commands_in_chat_get_aces_replies`; empyrean-command `commands::account_advocate_help_commands::acehelp_from_the_console_matches_ace`, `commands::account_advocate_help_commands::acecommands_from_the_console_matches_ace` and empyrean-world `server_properties::default_tables_match_aces_in_order_with_descriptions`, `server_properties::cache_script_matches_ace_through_list_properties` (ACE's vectors through `empyrean_common::vectors::brand_ruled`; empyrean-common `dotnet::net10_vectors::brand_rules_each_hit_a_vector`). Not covered: the boot reason and the skill-content message (text only) |
| V340 | fix | A treasure roll that creates nothing spawns nothing, and the corpse limit no longer fails a death when no corpse can be shortened. | `generators::a_wielded_treasure_roll_that_creates_nothing_spawns_nothing`, `players::death_xp::corpse_limit::a_corpse_over_the_limit_is_added_when_none_can_be_shortened` |
| V341 | fix | A yes/no emote that cannot ask runs its failure branch, and an object with no activation text says nothing and carries on. | `content::emotes::activation_failures::a_yes_no_question_that_cannot_be_sent_runs_the_failure_set`, `content::emotes::activation_failures::talk_without_activation_talk_says_nothing` |
| V342 | fix | An attribute transfer refusal names the right attribute, a hotspot draws its cycle time once, and a refused book edit is reported as refused. | empyrean-testkit `skill_proficiency::an_attribute_transfer_device_refusal_names_the_attribute_at_its_maximum`, `content::object_use::hotspot_cycle::a_hotspot_draws_its_cycle_time_once_when_touched`, empyrean-testkit `world_object_uses::a_player_writes_reads_and_deletes_book_pages` |
| V343 | fix | A saved object is not saved again until it changes. | `saves_and_logout::save_biota_writes_the_cached_positions_stamps_the_save_and_snapshots_the_biota` |
| V344 | fix | The pending-shutdown notice puts separators only between parts that are there. | `world_loop::shutdown_notice_text_separates_only_the_parts_that_are_there` |
| V345 | fix | The `cloak_min_proc` setting is read under the name it is registered with. | `combat::creature_combat::cloak_proc_floor::the_cloak_min_proc_setting_is_the_floor_of_the_proc_chance` |
| V350 | retail | Character options are read in the client's layout, so float and string options no longer lose the rest of the options and the window layout. | `players::character_options_and_queries::character_options_with_float_and_string_qualities_keep_options2_and_the_layout` |
| V351 | retail | Vendor buy and sell requests are read in the client's layout. | `dispatch::buy_and_sell_read_the_clients_layout` |
| V352 | retail | Animation and attack-frame timings are the client's playback times. | `server_physics::motion::*`, dereth-animation `seq::update::tests::play_time_and_fired_frames_match_the_update_loop` |
| V360 | fix | `/faction` sends its private updates to the player alone. | empyrean-testkit `developer_commands_in_chat::faction_sends_its_private_updates_to_the_player_alone` |
| V361 | fix | Command parsing handles an unclosed quote, sudo of raw commands and hex player ids correctly. | empyrean-command `commands::command_parsing::parse_command_matches_ace` (ACE's vectors, the fixed cases checked as ACE's less the repeated words), `commands::command_manager::resolve_finds_online_players_and_cuts_at_either_iid`; empyrean-testkit `commands::a_sudoed_raw_command_gets_its_raw_line` |
| V362 | fix | Several admin and developer commands do what their messages say instead of throwing or printing the wrong value. | empyrean-testkit `developer_commands_in_chat::admin_command_fall_throughs_and_wrong_messages_are_fixed` (delete, regen, monsterspell, de_n, modifyvital, vendordump), `developer_commands_in_chat::telexyz_setposition_teletype_and_teledungeon`, `admin_commands_in_chat::an_admin_teleports_to_a_player_and_to_a_named_location` (telepoi list); empyrean-command `commands::admin_commands::an_event_that_cannot_start_says_so_on_the_console`, `commands::account_advocate_help_commands::bestow_name_parsing_matches_ace` (ACE's vectors, the name now the words before the level). Not covered by a test: fumble (an adjacent landblock), qst fellow list (a fellowship), rename (a character missing from its account), contract (a contract table in the dats) |
| V363 | fix | `set-characteraccess` works, a forced delete of an online character reports success, and dat exports stop after printing their usage. | empyrean-command `commands::fix_and_account_commands::set_characteraccess_sets_the_characters_permission_flags`, `commands::admin_commands::a_dat_export_with_the_wrong_parameter_count_prints_its_usage_and_stops`; empyrean-testkit `advocate_account_help_commands_in_chat::an_admin_deletes_an_online_character` |
| V364 | fix | The content authoring commands' export and import defects (folder renames, missing fields, NULL splices) are fixed. | empyrean-content's ACE vector suites (`export_sql`, `export_sql_more`, `json`, `export_json`, `patch`) now accept exactly these differences from ACE's output (`as_ace_wrote`, the named cases), and `null_fields_are_filled_outside_strings_and_comments_only`, `a_characters_gameplay_options_are_written_as_a_binary_literal`; empyrean-command `commands::account_advocate_help_commands::content_folders_swap_only_their_own_level`, `commands::account_advocate_help_commands::a_sql_file_type_is_read_past_blank_lines` |
| V365 | fix | Several operator fix commands, `/reportbug`'s trim and the orphan purge do what they say. | empyrean-command `commands::fix_and_account_commands::verify_attributes_renews_the_free_reset_only_for_a_redistributed_player`, `commands::fix_and_account_commands::verify_clothing_wield_level_fixes_an_item_with_some_of_the_rows`, `commands::fix_and_account_commands::database_queue_info_and_the_perf_test_report_from_the_console`, `persistence::shard_offline_tools::purge_orphaned_biotas_purges_each_kind_of_orphan`; empyrean-testkit `commands::reportbug_builds_aces_url` (ACE's handler called directly, since Empyrean's `reportbug` replaced it in the table, V367). **Converged with upstream** for the reportbug trim: ACE fixed the same at `e0f9ce83`. |
| V366 | brand | IOUs are signed Empyrean, and IOUs signed ACEmulator still redeem. | empyrean-testkit `developer_commands_in_chat::inventory_commands_create_items_in_the_pack` (the signature, and both authors accepted, others not) |
| V367 | brand | `/reportbug` shows the Dereth issue tracker's address and sends nothing about the player. | empyrean-testkit `commands::reportbug_points_at_the_issue_tracker`, `commands::reportbug_builds_aces_url` (ACE's handler, called directly); empyrean-command `commands::empyrean_commands::reportbug_is_empyrean_s_in_ace_s_slot`, `commands::empyrean_commands::reportbug_points_at_the_issue_tracker` |
| V368 | brand | `@emphelp`, `@empcommands` and `@empversion` are added; ACE's names still work. | empyrean-command `commands::empyrean_commands::empyrean_s_names_are_ace_s_commands`, `commands::empyrean_commands::both_names_answer_on_the_console`, `vectors::the_command_table_matches_aces_for_these_handler_files` (ACE's other rows unchanged); empyrean-testkit `advocate_account_help_commands_in_chat::a_player_reads_the_help`, `commands::player_commands_in_chat_get_aces_replies`; empyrean-world `server_properties::default_tables_match_aces_in_order_with_descriptions` (via `brand_ruled`) |
| V369 | brand | The console, start-up log and environment variables name Empyrean, and there is no container special case. | empyrean-command `commands::command_manager::a_console_line_is_parsed_on_the_console_thread_and_run_on_the_world_thread` (banner and prompt), `commands::command_manager::the_interactive_console_key_turns_the_prompt_off` (V373 replaced the variables with a key); empyrean-server `dat_directory::*` (default and lookup order), `config::config_discovery::empyrean_toml_example_is_the_generated_defaults`, `database_manager` and `world_pack` constant-name tests; empyrean-world `server_properties::the_content_folder_has_no_container_special_case`. Not covered: the log lines (text only) |
| V370 | brand | `@forcelogoff` and `@propertydump` texts name Empyrean and the Dereth issue tracker. | empyrean-testkit `developer_commands_in_chat::forcelogoff_boots_a_player_and_reports_the_path`, `developer_commands_in_chat::the_last_appraised_monster_is_inspected_and_edited` (the propertydump header) |
| V371 | brand | JSON exports are signed Empyrean. | empyrean-content `export::export_json::weenies_export_as_ace_exports_them`, `export::export_json::append_metadata_carries_an_existing_file_over` and the other `content_export_json` comparisons (ACE's vectors through `empyrean_common::vectors::brand_ruled`) |
| V372 | brand | The world name defaults to Empyrean, and the stored property descriptions name Empyrean. | empyrean-common `config::config_js_reader::class_defaults_match_ace`; empyrean-testkit `login::a_good_login_reaches_the_character_list_with_the_accounts_characters` (the world name sent), `properties::the_server_starts_open_and_initialised_as_program_main_leaves_it`, `object_messages::a_test_configuration_does_not_leak_into_parallel_servers`; empyrean-world `server_properties::default_tables_match_aces_in_order_with_descriptions` and `cache_script_matches_ace_through_list_properties` (via `brand_ruled`) |
| V373 | arch | `empyrean.toml` is the only configuration; `Config.js` is converted once, ACE's unported settings are removed, and no environment variable is read. | empyrean-common `toml_config::*` (every kept key, `removed_keys_and_sections_warn_once_and_load`, the converter on ACE's own `Config.js.example`), `config::config_js_reader::aces_removed_settings_are_ignored`; empyrean-server `config_file::*` (discovery, `write_config_from_converts_aces_config_js_once`, `startup_with_only_a_config_js_fails_with_the_converter_message`, `the_server_ignores_the_removed_environment_variables`, `log_level_names_the_five_levels`), `status::status_endpoint::address_from_the_command_line_or_the_configuration`; empyrean-command `commands::command_manager::the_interactive_console_key_turns_the_prompt_off`; empyrean-content `import::content_clock::the_overlay_default_clock_ignores_source_date_epoch`, `import::content_clock::the_importer_ignores_source_date_epoch_and_honours_now` |
| V374 | arch | ACE's mod loader, auto-updaters and physics caches are not carried. | - |
| V375 | arch | Every configuration path resolves against the configuration file's folder, with `~` as the home directory. | empyrean-common `config_paths::*` (`~`, `~/x`, `~\x`, `~user/x`, no home, absolute, relative to the config's folder, no config: the working directory, the search order); empyrean-server `dat_directory::*` (empty: config folder then beside the executable; relative; `~`; absolute), `boot::world_pack::the_default_pack_is_also_found_beside_the_executable_a_written_one_is_not`, `boot::world_pack::a_relative_overlay_resolves_beside_the_config`, `boot::database_manager::database_paths_come_from_the_config_then_the_default`, `config::config_discovery::relative_paths_resolve_beside_the_config_file_not_the_working_directory` (the binary started from another working directory creates `shard.db` beside its `--config` file), `config::config_discovery::write_config_from_converts_aces_config_js_once` (the converter's note) |
| V376 | arch | The server offers its source, as the AGPL asks: in the login welcome, with `@source`, in the version report and in the status endpoint. | empyrean-testkit `small_handlers_in_world::friends_old_and_server_version_answer_with_a_line` (the welcome with the default address and no separate source line; the version report's last line), `enter_world::a_created_character_enters_the_world_with_aces_messages_in_aces_order` (one welcome F7E0, as ACE), `commands::source_names_the_licence_and_the_source` (default and configured); empyrean-server `status::status_endpoint::status_json_is_exact_and_escaped`, `the_configured_source_url_is_reported`; empyrean-common `config::config_js_reader::source_url_overrides_the_compiled_default` (the welcome with a configured address) |
| V377 | retail | `fellow_kt_killer` defaults to false, so fellows in range share kill-task credit as retail did. | empyrean-world `fellowship::a_kill_task_credit_is_shared_with_fellows_in_range` (default: shared; set true: not) |
| V378 | arch | A console command's reply always prints to the console, whatever the log level, and the prompt is redrawn after log lines. | empyrean-command `commands::console_prompt::a_console_commands_reply_does_not_depend_on_the_log_level`, `commands::console_prompt::a_line_printed_above_the_prompt_redraws_the_half_typed_line`, `commands::console_prompt::a_prompt_line_that_fills_its_row_is_redrawn_cleanly`, `commands::console_prompt::the_prompt_line_edits_and_recalls`; empyrean-server `config::console_end_to_end::a_console_reply_is_written_at_log_level_error`, `config::console_end_to_end::a_non_interactive_console_prints_no_prompt` |
| V379 | retail | Item experience curves are the client's, so the level an item shows matches the server's. | empyrean-world `formulas::experience_system_item_level_to_total_xp`, `formulas::experience_system_item_total_xp_to_level`, `formulas::rule3_item_xp_fixed_plus_base_and_undef_agree_with_dereth_rules`, `formulas::rule3_item_xp_fixed_and_scales_with_level_agree_with_dereth_rules` |
| V380 | retail | The number of Prismatic Tapers comes from the most powerful scarab, as the client shows it. | empyrean-world `spell_effects::rule3_foci_formula_agrees_with_dereth_client_model`, `spell_effects::real_content::spell_formulas_match_ace_on_retail_dats` (ACE's record with retail's taper count wherever the two rules part; exactly 45), `spell_effects::real_content::rule3_foci_formula_on_retail_dats` |
| V381 | retail | A caster's elemental bonus against players is reduced by 0.25, as the client's examine panel shows. | empyrean-world `creature_combat::rule3_caster_elemental_pvp_modifier_agrees_with_the_client` |
| V382 | retail | The inscription response is written in the client's layout. | empyrean-world `messages::rule3_inscription_response`, `messages::inscription_response_and_contain_id_read_the_objects_properties` |
| V383 | arch | ACE's direct console writes go through the server log, levelled and filterable. | empyrean-common `dotnet::console::a_console_write_is_a_log_record_on_the_console_target` |
| V384 | retail | The default second options word includes hearing PK deaths, as the 6096 client assumes. | empyrean-entity `enums::ext::character_option::tests::the_default_second_option_word_hears_pk_deaths`, `shared_equality::character_options2_default_is_the_player_modules` |
| V385 | arch | The server can update itself from its releases, off by default, within a patch or minor policy, checked by SHA-256 and rolled back on failure. | empyrean-server `update::selection::*`, `update::install_from_loopback::*` (patch and minor installs from a loopback release, checksum and declaration mismatches refused, a failed trial rolled back), `update::running_server::*` (with `real-content`: the real server's countdown, install and restart, `cancel-shutdown` installing nothing, a failing new build rolled back to the old server); empyrean-store `upgrade::*` (the backup before an install). Linux and macOS's in-place restart is not run by the tests on Windows |
| V386 | era | On an Infiltration world, a new character starts outdoors in the chosen town, with recalls enabled. | empyrean-world `players::chargen::an_infiltration_character_starts_outdoors_in_its_town_with_recalls_enabled`, `players::chargen::an_end_of_retail_character_starts_in_the_starter_area_with_recalls_disabled`, `players::chargen::real_content::a_character_created_in_each_town_starts_where_the_packs_era_says`; empyrean-common `era::tests::infiltration_starts_outdoors_by_town_and_caps_at_126`; empyrean-testkit `login::era_world::era_real::a_new_character_enters_the_packs_era_world_and_moves` |
| V387 | era | On an Infiltration world, loot that rolls a weenie the content lacks is rolled again. | empyrean-world `content::loot::a_rolled_weenie_the_world_lacks_is_rolled_again_only_under_the_pack_only_rule`, `content::loot::a_world_without_a_types_weenies_drops_nothing_of_it_under_the_pack_only_rule`, `content::loot::real_content::loot_rolled_under_the_packs_era_creates_every_item_of_tiers_1_to_6` (on both packs; ACE's loot vectors unchanged on the end-of-retail pack) |
| V388 | era | The world pack records its era, and the server refuses a pack built for another era. | empyrean-server `boot::database_manager::a_pack_built_for_another_era_aborts_and_one_built_for_the_configured_era_passes`, `boot::world_pack::the_start_up_corrections_line_counts_what_applies`; empyrean-import `import::pack_era::a_pack_records_the_era_it_was_built_for_and_the_dumps_lineage`, `import::pack_era::an_unknown_era_or_an_era_outside_a_build_is_a_usage_error`, `import::pack_era::a_pack_with_an_era_this_build_does_not_know_is_refused`, `import::world_data_tools::empyrean_import_corrections_exits_0_on_content_built_for_another_era`; empyrean-content `corrections::tests::the_entries_apply_only_to_end_of_retail_content_and_the_rules_to_every_era` |
| V389 | era | On an Infiltration world, the character list says the account has no Throne of Destiny, so the client caps levels at 126 and offers only the original heritages. | empyrean-world `net::messages::character_list_sends_the_eras_throne_of_destiny_flag`; empyrean-testkit `login::era_world::era_real::a_new_character_enters_the_packs_era_world_and_moves` |
| V390 | era | On an Infiltration world, the level cap is 126. | empyrean-world `players::death_xp::an_era_level_cap_stops_advancement_below_the_tables_last_level` |
| V391 | era | The February 2005 `portal.dat` and `cell.dat` can be served, read in their own layouts and motion numbering. | empyrean-world `motion::era_motions::on_the_february_2005_dats_the_logout_lasts_its_own_departure`, `motion::era_motions::on_the_february_2005_dats_sitting_has_its_cycle_and_its_way_in`, `net::era_motion_wire::an_older_world_sends_the_logout_and_an_atlatl_stance_as_its_own_indices`, `net::era_motion_wire::a_queued_command_the_older_numbering_lacks_is_left_out`, `net::era_motion_wire::an_older_clients_motion_state_and_actions_are_read_into_the_final_numbering`; empyrean-dat `dat::pre_tod_dats::the_february_2005_dats_open_with_their_header_iterations_and_era_tables`, `dat::pre_tod_dats::every_february_2005_body_enters_the_world_at_every_starter_area`, `dat::pre_tod_dats::a_body_enters_a_february_2005_dungeon_cell`; empyrean-world `players::chargen::an_infiltration_starter_area_named_as_a_listed_area_starts_exactly_there`; empyrean-testkit `login::era_world::era_real::a_new_character_enters_holtburg_on_the_february_2005_dats_and_moves` |
| V392 | era | On an Infiltration world, character creation allows only the 36 skills and three heritages of February 2005. | empyrean-world `players::chargen::an_infiltration_character_may_not_train_a_skill_from_after_the_2012_consolidation`, `players::chargen::an_infiltration_world_answers_a_later_heritage_pending_with_a_popup`; empyrean-common `era::tests::infiltration_creation_rules_are_the_february_2005_tables`; empyrean-testkit `login::era_world::era_real::a_new_character_enters_holtburg_on_the_february_2005_dats_and_moves` |
| V393 | era | On an Infiltration world, there are no ratings. | empyrean-world `combat::player_combat::an_era_without_ratings_reads_every_rating_as_none` (over ACE's rating vectors) |
| V394 | era | On an Infiltration world, assessing a creature shows no armour levels or ratings. | empyrean-world `combat::player_combat::an_era_before_assessed_armor_levels_shows_none` |
| V395 | era | On an Infiltration world, the old weapon skills are used as they are, and an empty-handed player fights with Unarmed Combat. | empyrean-world `combat::player_combat::a_players_old_weapon_skill_is_its_own_before_the_consolidation`, `monsters::era_weapon_skill::a_creature_before_the_consolidation_attacks_with_its_highest_old_melee_skill`, `combat::player_combat::an_era_before_the_consolidation_uses_the_older_melee_damage` |
| V396 | era | On an Infiltration world, a character cannot swear to a lower-level patron. | empyrean-world `social::allegiance::an_era_refuses_an_oath_to_a_lower_level_patron` |
| V397 | era | On an Infiltration world, weapon item spells stay on the item instead of being cast on the wielder as auras. | empyrean-world `magic::enchantments::an_era_before_the_auras_casts_no_item_spell_on_the_wielder` |
| V398 | era | On an Infiltration world, there are no pre-order gifts and no rares. | empyrean-testkit `login::enter_world::a_first_login_in_an_era_without_the_gifts_receives_none`, empyrean-world `players::death_xp::an_era_without_rares_rolls_none_on_a_kill` |
| V399 | era | On an Infiltration world, four property defaults follow the era (`corpse_destroy_pyreals`, `item_dispel`, `vendor_shop_uses_generator`, `allow_fast_chug`). | empyrean-world `world::server_properties::an_eras_property_defaults_replace_aces_and_the_database_still_wins` |
| V400 | era | On an Infiltration world, vitae is worked off on the older, cheaper curve, capped at 12,500 experience a point. | empyrean-world `players::death_xp::an_era_before_throne_of_destiny_prices_vitae_by_the_older_curve` |
| V401 | era | On an Infiltration world, multiplicative weapon-speed spells (Rockslide) change weapon speed. | empyrean-world `combat::player_combat::an_era_with_multiplicative_weapon_speed_scales_the_weapon_by_it` |
| V402 | era | On an Infiltration world, melee damage uses the older rules: daggers take Coordination, and unarmed attacks add Unarmed Combat skill / 20. | empyrean-world `combat::player_combat::an_era_before_the_consolidation_uses_the_older_melee_damage` |
| V403 | era | On an Infiltration world, a creature's war-magic projectile does half its base damage. | empyrean-world `magic::spellcasting::an_era_halves_a_creatures_war_projectile` |
| V404 | era | On an Infiltration world, a shield guards in any stance at its full level, with no Shield skill. | empyrean-world `combat::player_combat::an_era_before_the_shield_skill_counts_a_shield_in_full_in_any_stance` |
| V405 | era | On an Infiltration world, a death drops level / 10 items (plus 0 to 2) instead of level / 20. | empyrean-world `players::death_xp::an_era_before_the_halving_drops_a_tenth_of_the_level_in_items` |
| V406 | era | On an Infiltration world, unassigned experience is capped at 4,294,967,295. | empyrean-world `players::death_xp::an_era_before_throne_of_destiny_caps_unassigned_experience_at_32_bits` |
| V407 | era | On an Infiltration world, there is no dual wield. | empyrean-world `inventory::player_inventory::an_era_without_dual_wield_refuses_a_weapon_in_the_off_hand` |
| V408 | era | On an Infiltration world, there are no weapon masteries. | empyrean-world `combat::player_combat::an_era_without_masteries_gives_no_heritage_bonus`, `players::chargen::an_infiltration_character_gets_the_eras_starter_gear_and_no_innate_augmentation` |
| V409 | era | On an Infiltration world, new characters get that era's starter gear. | empyrean-world `players::chargen::an_infiltration_character_gets_the_eras_starter_gear_and_no_innate_augmentation` |
| V410 | era | On an Infiltration world, a first login opens the Welcome Letter instead of the training halls' popup. | empyrean-testkit `login::enter_world::a_first_login_in_an_era_with_the_welcome_letter_opens_it` |
| V411 | era | On an Infiltration world, there are no innate augmentations. | empyrean-world `players::chargen::an_infiltration_character_gets_the_eras_starter_gear_and_no_innate_augmentation` |
| V412 | era | On an Infiltration world, weapons, armour, jewelry, clothing, food and pyreals drop by that era's tables. | empyrean-world `content::loot::the_infiltration_loot_scripts_compile_over_six_tiers`, `content::loot::an_infiltration_weapon_roll_names_its_eras_script_and_is_never_two_handed`, `content::loot::an_infiltration_armor_roll_is_one_of_its_eras_kinds`, `content::loot::an_infiltration_pyreal_drop_is_its_eras_tier_range`, real-content `content::loot::real_content::every_infiltration_weapon_drop_is_an_old_skill_weapon_the_world_has` |
| V413 | era | On an Infiltration world, creature drops scale by tier and chests are fuller, as that era's loot did. | empyrean-world `content::loot::an_infiltration_creature_and_chest_drop_by_their_eras_profiles`, real-content `content::loot::real_content::every_infiltration_weapon_drop_is_an_old_skill_weapon_the_world_has` (the item-count rule) |
| V414 | era | On the February 2005 dats, the dat check compares only the portal and cell iterations, and a client missing iterations is booted. | empyrean-world real-content `net::ddd::real_content::a_server_on_the_february_2005_dats_admits_a_client_drawing_that_world`; dereth-client-runtime `app::tests::a_client_drawing_the_february_2005_world_answers_ddd_with_that_worlds_iterations` |
| V415 | era | On the February 2005 dats, which have no component map, spell components are found from the world's component weenies. | empyrean-world real-content `magic::spellcasting::real_content_components::the_worlds_component_weenies_name_the_same_weenies_the_dats_map`, `magic::spellcasting::real_content_components::on_the_february_2005_dats_each_component_a_spell_uses_is_found_as_its_weenie` |
| V416 | era | On an Infiltration world, magic loot follows that era: lower spell levels, fewer and smaller cantrips, and that era's scrolls. | empyrean-world `content::loot::the_infiltration_spell_and_scroll_levels_are_its_eras`, `content::loot::infiltration_cantrips_are_minor_or_major_and_its_casters_spells_its_own`, real-content `content::loot::real_content::every_infiltration_magic_item_carries_its_eras_magic` |
| V417 | arch | Combat mode changes that wait on an animation run in the order they were asked for, so a weapon swap in combat ends in the right stance. | empyrean-testkit `combat::combat_mode_swap::*` (the swaps with a live server's clock drift between the unwield and the wield) |
| V418 | era | A world's systems are its era's table with its `[era]` settings over it, and the server tells the client which it has. | empyrean-common `era::tests::the_era_configuration_turns_systems_on_and_off_over_the_profile`, `config::toml_config::the_era_section_turns_systems_on_and_off`; empyrean-server `status::status_endpoint::the_status_names_the_era_its_systems_the_dats_and_the_client_versions` |
| V419 | era | A world without secure trade refuses to open one. | empyrean-world `inventory::trade::a_world_without_trade_refuses_opening_one` |
| V420 | era | A world without housing or apartments refuses buying, renting, entering and recalling to houses it lacks, and allegiance storage. | empyrean-testkit `housing::housing::a_world_without_housing_refuses_the_cottage_and_one_without_apartments_the_recall` |
| V421 | era | A world without tinkering refuses salvaging and tinkering recipes. | empyrean-testkit `crafting::recipes_tinkering_salvage::a_world_without_tinkering_refuses_salvaging_and_tinkering_recipes` |
| V422 | era | A world without cantrips rolls none onto loot. | empyrean-world real-content `content::loot::real_content::a_world_without_cantrips_rolls_none_onto_loot` |
| V423 | era | A world without chess refuses joining a game. | empyrean-testkit `games::chess::a_world_without_chess_refuses_joining_a_game` |
| V424 | era | A world without aetheria refuses sigil slots and raising aetheria. | empyrean-world `inventory::player_inventory::a_world_without_aetheria_cloaks_or_trinkets_refuses_their_slots`; empyrean-testkit `interactions::world_object_uses::uses::a_world_without_aetheria_reveals_no_sigil` |
| V425 | era | A world without luminance awards and spends none. | empyrean-world `players::death_xp::luminance::a_world_without_luminance_awards_and_spends_none` |
| V426 | era | A world without the contract tracker takes on no contracts. | empyrean-world `content::quests::a_world_without_contracts_or_titles_takes_none_on_and_grants_none` |
| V427 | era | A world without titles grants and shows none. | empyrean-world `content::quests::a_world_without_contracts_or_titles_takes_none_on_and_grants_none` |
| V428 | era | A world without cloaks or trinkets refuses those slots. | empyrean-world `inventory::player_inventory::a_world_without_aetheria_cloaks_or_trinkets_refuses_their_slots` |
| V429 | era | An item worn in a slot the world lacks comes off into the pack at login. | empyrean-world `inventory::player_inventory::at_login_an_item_in_a_slot_the_world_lacks_goes_to_the_pack` |
| V430 | retail | A character-creation body wears the creation table's bare parts, so a new 2005 character has bare arms, not mail. | empyrean-world `players::bare_body` (3) |
| V431 | arch | A character screen message (`server.character_screen_message`, or a welcome by default) follows the character list. | empyrean-world `players::login::the_character_screen_message_follows_the_character_list_and_defaults_to_a_welcome`; empyrean-common `config::config_js_reader::the_character_screen_message_is_read_from_the_toml_file` |
| V432 | era | On a world with spell research, the early clients' formula test teaches and casts the spell its components name. | empyrean-world `magic::spellcasting::real_content_components::a_research_formula_names_the_spell_whose_formula_it_is` |
| V434 | era | On an Infiltration world, experience keeps arriving at level 126 up to a 32-bit cap. | empyrean-world `players::death_xp::an_era_before_throne_of_destiny_keeps_earning_at_the_level_cap_up_to_32_bit_counts` |
| V435 | era | On a world with the oath cost, swearing after a break from a patron costs unassigned experience. | empyrean-world `social::allegiance::an_era_charges_an_oath_after_a_break_from_a_patron` |
| V436 | era | On a world with the oath cost, breaking from one's patron counts a break. | empyrean-world `social::allegiance::an_era_charges_an_oath_after_a_break_from_a_patron` |
