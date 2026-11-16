# Command audit

Every command the Empyrean server registered on 2026-11-16 (the 334 rows `@emphelp commands`
drew from), sorted by what it does on this server, and what this change did about it. File and
line references are to the code as it was audited, before the change.

The table code lives in `empyrean/crates/command`. ACE's rows are ported row for row in
`src/handlers/`; the change leaves those rows alone and changes them from one place,
`src/empyrean/command_table.rs` (V524), so an upstream ACE change still lands only in the ported
files. Handler fixes are marked in the code as V525.

## Summary

| Sort | Rows | What the change did |
|---|---|---|
| Meaningless on this server | 17 | Removed from the table. |
| Stub | 23 | Removed, except `event` (half works; follow-up). |
| Broken | 22 | Fixed, except `debugchess`, `debugboard` and `show-allegiances` (follow-ups). |
| Works, needed a description or usage | 78 | Descriptions and usage written. |
| Works, but does very little | 14 | Left as they are; some have follow-ups. |
| Works and is documented | 180 | Left as they are. |
| All | 334 | 39 removed, 295 left |

"Removed" means the row is no longer in the command table: the command is unknown, and `@emphelp`
and `@empcommands` do not list it. The handler code stays in the ported files, so a row can come
back once it does something. Removed rows are also gone from the Dereth client's Admin plugin
table (`dereth/plugins/admin/commands.txt`).

Rows whose description or usage was rewritten keep ACE's access level and handler; the test
`the_command_table_s_edits_are_ace_s_rows_changed_on_purpose` holds every edit to an ACE row and
checks that each one still differs from ACE's text.

## Meaningless on this server

| Command | Access | Finding | Status |
|---|---|---|---|
| `adminui` | Developer | Empty body; ACE's comment says a Decal plugin would do it (admin_commands.rs:591). | Removed |
| `clearphysicscaches` | Developer | All four cache clears are empty (developer_commands.rs:~6491): the shared physics has none of ACE's caches, yet it says the caches were cleared. | Removed |
| `forcegc` | Developer | Only prints ".NET Garbage Collection forced" (developer_commands.rs:3315); no GC in Rust. | Removed |
| `forcegc2` | Developer | Only prints a fake LOH-compact message (developer_commands.rs:3328); no GC in Rust. | Removed |
| `gcstatus` | Advocate | .NET GC figures, printed as fixed zeros (admin_stat_commands.rs:625-637); there is no managed heap. | Removed |
| `generate-classnames` | Developer | Writes a C# `WeenieClassName.cs` enum for ACE's factories (developer_content_commands.rs:3614-3660); nothing in a Rust server reads it. | Removed |
| `lbinterval` | Admin | Empty (admin_commands.rs:6459); retail farm load balancing. | Removed |
| `lbthresh` | Admin | Empty (admin_commands.rs:6467); retail farm load balancing. | Removed |
| `myserver` | Envoy | Empty body (admin_commands.rs:1027); one server process, so a game server number means nothing. | Removed |
| `queryplugin` | Envoy | Empty body (admin_commands.rs:1098); a Decal plugin query. | Removed |
| `querypluginlist` | Envoy | Empty body (admin_commands.rs:1087); a Decal plugin query the server cannot make. | Removed |
| `reload-loot-tables` | Admin | Would parse ACE's C# loot table sources by .NET reflection (`not_ported!` in loot_swap.rs:133-154); its default folder is an ACE source path. | Removed |
| `reloadsysmsg` | Developer | Empty (admin_commands.rs:6311); retail's farm-wide reload of system_messages.txt. | Removed |
| `serverlist` | Envoy | Empty body (admin_commands.rs:1188); lists servers in a multi-server farm, one process here. | Removed |
| `stormnumstormed` | Admin | Empty (admin_commands.rs:6492); retail farm portal-storm tuning. | Removed |
| `stormthresh` | Admin | Empty (admin_commands.rs:6500); retail farm portal-storm tuning. | Removed |
| `unlock` | Envoy | Empty body (admin_commands.rs:1759); cleans a SQL player lock that does not exist here. | Removed |

## Stub

| Command | Access | Finding | Status |
|---|---|---|---|
| `deaf` | Sentinel | Empty `// TODO: output` (sentinel_commands.rs:736); ACE never implemented it. | Removed |
| `draw` | Developer | Empty body, `// TODO: output` (admin_commands.rs:672). | Removed |
| `dumpattackers` | Developer | Empty `// TODO: output` (admin_commands.rs:6442). | Removed |
| `equiptest` | Developer | Parses its arguments only; the `TestWieldItem` call is commented out (developer_commands.rs:566-611). | Removed |
| `event` | Developer | Only start, stop and status <name> work; enable, disable and clear are empty arms (admin_commands.rs:4181). | Follow-up: Port `enable`, `disable` and `clear`, and let `status` list every event. |
| `freeze` | Sentinel | Empty body, `// TODO: output` (admin_commands.rs:869). | Removed |
| `housekeep` | Developer | Empty `// TODO: output` (admin_commands.rs:5210). | Removed |
| `image-export` | Admin | `dat_file_is_texture` and `texture_export_texture` are `not_ported!` (console_commands.rs:399-411): nothing is written, but it reports files exported. Usage empty. | Removed |
| `limbo` | Sentinel | Empty body, `// TODO: output` (admin_commands.rs:1001). | Removed |
| `location` | Developer | Empty `// TODO: output` (admin_commands.rs:5246). | Removed |
| `magic god` | Developer | A two-word name the lookup can never match (it takes the first word only); the body only prints a line (admin_commands.rs:4935). | Removed |
| `radar` | Developer | Empty `// TODO: output` (admin_commands.rs:6476). | Removed |
| `raise` | Developer | Empty `// TODO: output` (admin_commands.rs:6105). | Removed |
| `rares dump` | Developer | Two-word name the lookup can never match; empty body (admin_commands.rs:6484). | Removed |
| `readdat` | Developer | Reads the skill table and returns (developer_commands.rs:1935); nothing to see. | Removed |
| `repeat` | Sentinel | Empty body (admin_commands.rs:1106). | Removed |
| `setadvclass` | Developer | Empty `// TODO: output` (admin_commands.rs:6287). | Removed |
| `snoop` | Envoy | Empty body (admin_commands.rs:1206). | Removed |
| `spendxp` | Developer | Empty `// TODO: output` (admin_commands.rs:6295). | Removed |
| `trainskill` | Developer | Empty `// TODO: output` (admin_commands.rs:6303). | Removed |
| `unfreeze` | Sentinel | Empty body, `// TODO: output` (admin_commands.rs:877). | Removed |
| `userlimit` | Admin | Empty `// TODO: output` (admin_commands.rs:6357). | Removed |
| `wave-export` | Admin | `dat_file_is_wave` is `not_ported!` and returns false (console_commands.rs:405), so nothing is written, yet it prints that the export is complete. Usage empty. | Removed |

## Broken

| Command | Access | Finding | Status |
|---|---|---|---|
| `accountget` | Admin | An unknown account hits `.expect("NullReferenceException")` (account_commands.rs:197): the operator sees nothing. Small fix. | Fixed: Says the account does not exist. |
| `debugboard` | Developer | Console-only output. | Follow-up: Send the board to the caller instead of the server console. |
| `debugchess` | Developer | Console-only output, and only in a chess match. | Follow-up: Send the move history to the caller instead of the server console. |
| `destructionqueue` | Developer | Console-only output (developer_commands.rs:2650). As ACE. | Fixed: Answers in chat. |
| `echo` | Developer | No argument panicked at `parameters[0]` (developer_commands.rs:671). | Fixed: Needs its argument now. |
| `getenchantments` | Admin | `get_enchantments_top_layer(...).expect(...)` (admin_commands.rs:6985) panics on an item with no enchantments. | Fixed: Says when the item has no enchantments. |
| `knownobjs` | Developer | Dump goes only to the server console (`object_maint_dump`, developer_commands.rs ~2530): in game the caller sees nothing. As ACE. | Fixed: Answers in chat. |
| `knownplayers` | Developer | Console-only output (developer_commands.rs:2577). As ACE. | Fixed: Answers in chat. |
| `listcb` | Developer | Reads `parameters[0]` with min params -1, so no argument panics (developer_commands.rs:629). Usage missing. | Fixed: Needs its argument now. |
| `lostest` | Developer | Result goes only to the server console (developer_commands.rs:2245). As ACE. | Fixed: Answers in chat (the no-target error still goes to the console). |
| `modifyattr` | Admin | As modifyvital. | Fixed: In game only now (RequiresWorld). |
| `modifyskill` | Admin | As modifyvital. | Fixed: In game only now (RequiresWorld). |
| `modifyvital` | Admin | Flags None, but `require(session)` (admin_commands.rs:4954) panics from the console. | Fixed: In game only now (RequiresWorld). |
| `retaliatetargets` | Developer | Console-only output (developer_commands.rs:2626). As ACE. | Fixed: Answers in chat. |
| `setpurchasetime` | Developer | `player_house(...).expect(...)` panics if the player owns no house (developer_commands.rs:2958). | Fixed: Says when you own no house. |
| `show-allegiances` | Admin | Works from the console; in game it shows nothing, because `allegiance::show_info` in the world crate writes to the server console (admin_commands.rs:6963). | Follow-up: answer in the caller's chat. The Admin plugin sends it from chat, so it stays a chat command. |
| `teleallto` | Developer | Uses only parameters[0] (admin_commands.rs:1437): a multi-word name is not found, so everyone goes to the admin instead. As ACE. Small fix. | Fixed: Takes the whole name. |
| `teledist` | Developer | `LAST_SPAWN_POS.expect(...)` panics unless something was spawned earlier this run (developer_commands.rs:2411). | Fixed: Says when nothing was spawned or the distance is not a number. |
| `testlootgen` | Admin | `testlootgen 100` (min params 1) panics at `parameters.get(1).expect(...)` (developer_loot_commands.rs:~125). | Fixed: Says the tier is missing.; Described |
| `visibleobjs` | Developer | Console-only output (`object_maint_dump`). As ACE. | Fixed: Answers in chat. |
| `visibleplayers` | Developer | Console-only output (developer_commands.rs:2594). As ACE. | Fixed: Answers in chat. |
| `visibletargets` | Developer | Console-only output (developer_commands.rs:2609). As ACE. | Fixed: Answers in chat. |

## Works, needed a description or usage

| Command | Access | Finding | Status |
|---|---|---|---|
| `adminhouse` | Admin | Works (admin_commands.rs:1860); no args send help text. Usage empty though it takes subcommands. | Described |
| `castmeter` | Player | Usage empty while it takes on/off (player_commands.rs:350). | Described |
| `clearcache` | Developer | Usage empty but it takes an optional cache type. | Described |
| `damagehistory` | Developer | Prints your own damage history (developer_commands.rs:4515); description empty. | Described |
| `database-shard-cache-npbrt` | Developer | Same as pbrt, for non-player biotas. | Described |
| `database-shard-cache-pbrt` | Developer | Description is a label, not an action; optional minutes not in usage. | Described |
| `debugspell` | Developer | Works (developer_commands.rs:4649); optional on/off arg not in usage. | Described |
| `delete` | Envoy | Works (admin_commands.rs:600). Usage holds a note, not arguments; takes an optional type name that must match the selected object. | Described |
| `dispel` | Developer | Also dispels worn items; usage "/dispel" for a command with no arguments. | Described |
| `effect` | Developer | Scale is parsed but always 1.0 (developer_commands.rs:743). Usage typo ("namefloat"). | Described; Follow-up: Use the scale argument. |
| `enable-aetheria` | Developer | Usage missing for the optional flags value. | Described |
| `export-json` | Developer | Usage lists "spell", which does nothing (:2413). | Described |
| `fast` | Developer | Applies Quickness, Sprint, Strength Self VIII to you (developer_commands.rs:4557); description empty. | Described |
| `fetchbool` | Admin | Usage repeats the command name. | Described |
| `fetchdouble` | Admin | Usage repeats the command name. | Described |
| `fetchlong` | Admin | Usage repeats the command name. | Described |
| `fetchstring` | Admin | Usage repeats the command name. | Described |
| `fix-biota-emote-delay` | Admin | Without `fix` it only prints help. | Described |
| `fix-gear-plating` | Admin | Dry run unless "execute". | Described |
| `fix-shortcut-bars` | Admin | Dry run unless "execute"; mentions SQL scripts that do not exist here. | Described |
| `fix-spell-bars` | Admin | Dry run unless "execute". | Described |
| `forcelogoff` | Developer | Works (developer_commands.rs:3644); name arg optional, usage empty. | Described |
| `forcelogout` | Developer | Alias of forcelogoff (developer_commands.rs:3638); optional name missing from usage. | Described |
| `fumble` | Developer | Forces the selected player to drop everything; description empty. | Described |
| `gamecastlocalemote` | Developer | Same as gamecastemote here; "within chat range" is wrong. | Described |
| `getallspellformula` | Developer | Usage empty. A missing component panics (developer_commands.rs:1924). | Described; Follow-up: Say which spell lacks a component instead of panicking. |
| `getproperty` | Developer | Usage does not show the `Type.Name` form. | Described |
| `getspellformula` | Developer | Usage empty. An unknown spell id panics (developer_commands.rs:1872). | Described; Follow-up: Say when the spell id is unknown instead of panicking. |
| `grantitemxp` | Developer | Usage missing though it takes an amount. | Described |
| `heal` | Envoy | Players only; usage has a stray newline. | Described |
| `house-select` | Player | Usage empty while it needs a house number (player_commands.rs:205). | Described |
| `import-json` | Developer | Needs `server.world_overlay_path`; without it `execute_sql_raw` panics (developer_content_commands.rs:1430). Usage lists "spell", which does nothing (:454). | Described; Follow-up: Without `server.world_overlay_path` the import panics uncaught (developer_content_commands.rs:1430); say the overlay is needed. Same for import-sql-folders, createinst, removeinst, addenc, removeenc. |
| `import-sql` | Developer | Usage omitted the event type. | Described |
| `listplayers` | Developer | Usage empty though it takes an optional access level. | Described |
| `modifybool` | Admin | Usage repeats the command name. | Described |
| `modifydouble` | Admin | Usage names "modifyfloat". | Described |
| `modifylong` | Admin | Usage repeats the command name. | Described |
| `modifystring` | Admin | Usage repeats the command name. | Described |
| `monsterspell` | Developer | Works (developer_commands.rs:5397); usage omits the required spell and the target guid is hex. | Described |
| `movement` | Developer | Usage empty though it takes an argument; an invalid number panics (developer_commands.rs:858). | Described; Follow-up: Say when the argument is not a number instead of panicking. |
| `nudge` | Developer | Works (needs the overlay to save). Flags None but `require(session)` (developer_content_commands.rs:3217) panics from the console. Usage omits the optional guid and amount. | Fixed: In game only now (RequiresWorld). |
| `objsend` | Player | Usage empty while it takes an optional "creature" (player_commands.rs:553). | Described |
| `playtest-character` | Admin | Usage left out the `account <account>` check. | Described |
| `pscript` | Developer | Works (developer_commands.rs:4693); description and usage empty although it needs a PlayScript. | Described |
| `qst` | Developer | Usage incomplete; bare `qst` returns silently. | Described |
| `recordcast` | Developer | Works (developer_commands.rs:4671); optional on/off arg not in usage. | Described |
| `reload-dat-overlay` | Developer | Usage left out the `--revision <n>` option. | Described |
| `removeitemspell` | Developer | Works (developer_commands.rs:3960); description typo "Removes a spell to". | Described |
| `removespell` | Developer | Works (admin_commands.rs:1801). Description says "to" for "from". | Described |
| `rip` | Developer | Deals your current health as damage to you (developer_commands.rs:4585); description empty. | Described |
| `rotate` | Developer | Flags None but `require(session)` (:3510) panics from the console. Usage omits the optional guid and `here`. | Fixed: In game only now (RequiresWorld). |
| `rotate-x` | Developer | Flags None but `require` panics from the console. Optional guid not in usage. | Fixed: In game only now (RequiresWorld). |
| `rotate-y` | Developer | As rotate-x. | Fixed: In game only now (RequiresWorld). |
| `rotate-z` | Developer | As rotate-x. | Fixed: In game only now (RequiresWorld). |
| `serverperformance` | Advocate | Usage empty while it takes start/stop/reset [cumulative] (admin_stat_commands.rs:344). | Described |
| `setcoin` | Developer | Changes only the CoinValue property; usage missing. | Described |
| `setproperty` | Developer | Usage vague; `null` removes the property. | Described |
| `slow` | Developer | Applies Slowness, Leaden Feet, Weakness Self VIII to you (developer_commands.rs:4571); description empty. | Described |
| `sticky` | Developer | Description reads backwards; any argument but "off" means on. | Described |
| `targetloc` | Developer | Works (developer_commands.rs:4427); optional guid not in usage. | Described |
| `testaim` | Developer | Dev test (developer_commands.rs:4740); usage empty although it needs a motion. | Described |
| `testdeathitems` | Developer | Works; takes an optional online player name, usage empty (developer_commands.rs:3579). | Described |
| `testlootgencorpse` | Admin | Usage omits -log and some tables. | Described |
| `trywield` | Developer | Works (developer_commands.rs:5495); description and usage empty although it needs two args. | Described |
| `verify-armor-levels` | Admin | Optional `fix` not in usage. | Described |
| `verify-attributes` | Admin | Optional `fix` not in usage. | Described |
| `verify-beneficial-enchantments` | Admin | Optional `fix` not in usage. | Described |
| `verify-clothing-wield-level` | Admin | Optional `fix` not in usage. | Described |
| `verify-heritage-augs` | Admin | Description says verify only, but `fix` grants the missing augmentation. | Described |
| `verify-legendary-wield-level` | Admin | Optional `fix` not in usage. | Described |
| `verify-max-augs` | Admin | Optional `fix` not in usage. | Described |
| `verify-melee-rares` | Admin | Optional `fix` not in usage. | Described |
| `verify-player-data` | Admin | Optional `fix` not in usage. | Described |
| `verify-shield-rating` | Admin | Optional `fix` not in usage. | Described |
| `verify-skill-credits` | Admin | Optional `fix` not in usage. | Described |
| `verify-skills` | Admin | Optional `fix` not in usage. | Described |
| `verify-vitals` | Admin | Optional `fix` not in usage. | Described |
| `verify-xp` | Admin | Optional `fix` not in usage. | Described |

## Works, but does very little

| Command | Access | Finding | Status |
|---|---|---|---|
| `allstats` | Advocate | Chains serverstatus, serverperformance, landblockstats, lbgroupstats, gcstatus and DB queue info (admin_stat_commands.rs:114). | Fixed: No longer prints the .NET GC lines. |
| `barrier-test` | Developer | Output goes only to the server console (developer_commands.rs:4395); in-game operator sees nothing. Panics if the landblock is missing (:4403). | Follow-up: Answer in chat, and say when the landblock is not loaded instead of panicking. |
| `chatdump` | Developer | Sends 1000 test chat lines (developer_commands.rs:750). | - |
| `comps` | Developer | Hard-coded spell components (developer_commands.rs:1688). | - |
| `currency` | Developer | Hard-coded currency (developer_commands.rs:1710). | - |
| `databaseperftest` | Developer | ACE never fills its biota list (database_perf_test.rs:74 `// TODO fix this`), so every stage reports 0/0. | Follow-up: ACE never fills its biota list, so every stage reports 0/0; port the test properly or remove it. |
| `deathxp` | Developer | Prints only the `xp_override` of the last appraised object (admin_commands.rs:3985); blank for a creature without one. | Follow-up: Compute the kill XP rather than print only the XP override. |
| `food` | Developer | Hard-coded food (developer_commands.rs:1702). | - |
| `inv` | Developer | Hard-coded test weenies (developer_commands.rs:1672). | - |
| `MoveTo` | Developer | Spawns a wand (12748) in front of you and puts it in your pack; distance argument parsed but unused (developer_commands.rs:876-879). | Described; Follow-up: Use the distance argument, or drop it. |
| `serverstatus` | Advocate | Thread pool, runtime, CPU time, thread count and memory read hard-coded zeros (admin_stat_commands.rs:~198-210); sessions, landblocks and caches are real. | Follow-up: Report the process's real thread count, CPU time, uptime and memory instead of zeros. |
| `splits` | Developer | Hard-coded test weenies (developer_commands.rs:1680). | - |
| `vloc2loc` | Developer | Needs Content's `vlocs/vlocDB.txt` and writes a file. | Follow-up: Needs Content's vlocs/vlocDB.txt; say so when it is missing. |
| `weapons` | Developer | Hard-coded test weenies (developer_commands.rs:1662). | - |

## Works and is documented

| Command | Access | Finding | Status |
|---|---|---|---|
| `accountcreate` | Admin | Creates the account, prints result (account_commands.rs:117). | - |
| `acecommands` | Player | ACE's name for empcommands. | - |
| `acehelp` | Player | ACE's name for emphelp. | - |
| `aceversion` | Player | ACE's name for empversion. | - |
| `addallspells` | Developer | - | - |
| `addalltitles` | Developer | - | - |
| `addenc` | Developer | Needs the overlay (same panic, :1430). | - |
| `additemspell` | Developer | Adds spell to the last appraised item's biota (developer_commands.rs:3934). | - |
| `addspell` | Developer | Learns the spell (admin_commands.rs:1787). An unknown spell id does nothing and says nothing, as ACE. | Follow-up: Say when the spell id is unknown. |
| `addtitle` | Developer | - | - |
| `adminvision` | Sentinel | Delegates to player::handle_adminvision_toggle (admin_commands.rs:575). | - |
| `animation` | Developer | - | - |
| `attackable` | Advocate | Returns silently for an Advocate below level 5 (as ACE). | Follow-up: Advocates below level 5 get no answer (as ACE); say why. |
| `auditobjectmaint` | Developer | Real audit of the physics known/visible/voyeur tables (developer_commands.rs:3375); prints counts. | - |
| `ban` | Sentinel | - | - |
| `banlist` | Sentinel | - | - |
| `barbershop` | Developer | - | - |
| `bestow` | Advocate | Full ACE logic. | - |
| `boot` | Sentinel | - | - |
| `bornagain` | Admin | Restores a deleted character via do_copy_char (admin_commands.rs:2739). | - |
| `buff` | Sentinel | - | - |
| `bumpvelocity` | Developer | Sets velocity (0,0,0.5) and sends vector update to you only (developer_commands.rs:4891). | - |
| `cancel-shutdown` | Admin | Cancels via server_manager::cancel_shutdown. | - |
| `castspell` | Developer | Casts numeric spell id on last appraised object (developer_commands.rs:6190). | - |
| `cell-export` | Admin | Extracts landblock contents. | - |
| `check-collision` | Developer | Reports collision check result for you (developer_commands.rs:4908). | - |
| `ci` | Developer | Spawns a weenie in your inventory and audit-logs it. | - |
| `ciaetheria` | Developer | Full port (developer_commands.rs:5722). | - |
| `ciloot` | Developer | Creates randomized MagicItem loot in inventory (developer_commands.rs:3518). | - |
| `cirand` | Developer | - | - |
| `cisalvage` | Developer | Panics on a material with no salvage weenie (admin_commands.rs:7027). | Follow-up: Say when a material has no salvage bag instead of panicking. |
| `cloak` | Sentinel | - | - |
| `cm` | Developer | Same as cisalvage. | - |
| `config` | Player | Gated by player_config_command. | - |
| `config-write` | Admin | Writes empyrean.toml (Empyrean's). | - |
| `contract` | Developer | `bestow` without an id returns silently. | - |
| `copychar` | Developer | Copies the character via do_copy_char (admin_commands.rs:2865). | - |
| `crack` | Envoy | Cracks the last appraised door or chest. | - |
| `create` | Developer | Parses weenie, amount, palette and shade, then spawns (admin_commands.rs:3342). | - |
| `createinst` | Developer | Needs the overlay (same panic, :1430). | - |
| `createliveops` | Developer | Same as create with a lifespan in seconds (admin_commands.rs:3351). | - |
| `createnamed` | Developer | Creates copies of the weenie with the given name (admin_commands.rs:3737). | - |
| `databasequeueinfo` | Developer | Shard queue count and wait time (developer_database_commands.rs:138). | - |
| `de_n` | Developer | Same as direct_emote_name. | - |
| `de_s` | Developer | Same as direct_emote_select. | - |
| `debugcast` | Player | - | - |
| `debugdamage` | Developer | - | - |
| `debugemote` | Developer | Confirmation is console-only. | Follow-up: The confirmation goes to the server console only. |
| `debugmove` | Developer | - | - |
| `debugspellbook` | Developer | Prints the appraised creature's spell book (developer_commands.rs:5471). | - |
| `deletecharacter` | Admin | Boots and deletes the character, reports result. | - |
| `delevel` | Developer | Full ACE port with xp/credit checks and confirmation (developer_commands.rs:5201). | - |
| `derethnetwork` | Player | - | - |
| `direct_emote_name` | Developer | - | - |
| `direct_emote_select` | Developer | - | - |
| `dist` | Developer | - | - |
| `dn` | Player | - | - |
| `dng` | Player | - | - |
| `dungeonname` | Developer | - | - |
| `echoflags` | Developer | - | - |
| `empcommands` | Player | - | - |
| `emphelp` | Player | - | - |
| `empversion` | Player | - | - |
| `exit` | Admin | Calls shutdown_server_now; console only. | - |
| `export-json-folders` | Developer | - | - |
| `export-sql` | Developer | - | - |
| `export-sql-folders` | Developer | - | - |
| `faction` | Developer | Sets society faction and rank on you; no args shows current state (developer_commands.rs:4941). | - |
| `fellow-dist` | Developer | Prints 2d/3d distance and scalar per fellow (developer_commands.rs:4093). | - |
| `fellow-info` | Developer | Prints XP share percentages, share flags and distance scalars (developer_commands.rs:3996). | - |
| `fellowbuff` | Sentinel | - | - |
| `finger` | Sentinel | Works from the console too. | - |
| `fix-allegiances` | Admin | Console only. | - |
| `fixbusy` | Player | Sends the use-done event (developer_commands.rs:539). | - |
| `fixcast` | Player | - | - |
| `gag` | Sentinel | Calls player_manager::gag_player and reports the result (admin_commands.rs:885). | - |
| `gamecast` | Envoy | Broadcasts world-wide; works from the console (admin_commands.rs:1769). | - |
| `gamecastemote` | Developer | - | - |
| `gamecastlocal` | Developer | Same as gamecast here. | - |
| `generatordump` | Developer | Full property dump of the selected generator (developer_commands.rs:4148). | - |
| `getinfo` | Developer | Shows GUID, WCID and class name of last appraised object (developer_commands.rs:4717). | - |
| `givemana` | Developer | Non-numeric input panics (developer_commands.rs:2352). | Follow-up: Say when the argument is not a number instead of panicking. |
| `god` | Sentinel | - | - |
| `gps` | Developer | - | - |
| `grantluminance` | Developer | - | - |
| `grantxp` | Developer | - | - |
| `harmself` | Developer | - | - |
| `highres-export` | Admin | - | - |
| `home` | Sentinel | Recalls to the sanctuary or saved position 1-9 (admin_commands.rs:957). | - |
| `idlist` | Developer | - | - |
| `import-character` | Admin | - | - |
| `import-sql-folders` | Developer | Needs the overlay (same panic, :1430). | - |
| `import-treasure` | Developer | - | - |
| `landblockperformance` | Advocate | Top-10 busiest landblocks by average and longest tick. | - |
| `landblockstats` | Advocate | Same handler as landblockperformance. | - |
| `language-export` | Admin | - | - |
| `lbgroupstats` | Advocate | Group tracker tables; multi-threaded columns mean little on a single-thread world. | - |
| `listpositions` | Developer | - | - |
| `loadalllandblocks` | Developer | - | - |
| `lootgen` | Developer | Creates the item, mutates it, gives it to you (developer_commands.rs:3448). Tier optional (default 1). | - |
| `makeiou` | Developer | Creates the IOU into inventory (developer_commands.rs:3557); silent if create_iou returns None. | - |
| `modifypropertydesc` | Admin | - | - |
| `morph` | Developer | - | - |
| `movetome` | Admin | - | - |
| `mrt` | Sentinel | Delegates to player::handle_mrt (admin_commands.rs:993). | - |
| `myiid` | Envoy | Prints GUID, low, high and hex (admin_commands.rs:1010). | - |
| `myloc` | Developer | - | - |
| `myquests` | Player | Gated by quest_info_enabled. | - |
| `netstats` | Developer | - | - |
| `neversaydie` | Sentinel | - | - |
| `passwd` | Player | 5s rate limit, checks old password. | - |
| `pk` | Developer | Sets PK status, broadcasts and audits it (admin_commands.rs:1035). An unknown argument still reports success. | Follow-up: Refuse an unknown argument instead of reporting success. |
| `pktimer` | Developer | Updates your PK timer (developer_commands.rs:3986). | - |
| `playsound` | Developer | - | - |
| `pop` | Player | Online player count, AI Players left out. | - |
| `portal-export` | Admin | - | - |
| `portal_bypass` | Sentinel | - | - |
| `portalstorm` | Developer | Sends storm events; level 2 teleports you to 0x7F7F001C (developer_commands.rs:6272). | - |
| `propertydump` | Developer | - | - |
| `purchase-house` | Developer | Sets house owner and gives deed for the appraised slumlord (developer_commands.rs:4378). | - |
| `regen` | Envoy | Resets the selected generator and regenerates it (admin_commands.rs:1116). | - |
| `reload-landblock` | Developer | Destroys non-player objects and re-inits the landblock next tick (developer_commands.rs:4817). | - |
| `remove` | Advocate | Requires online target. | - |
| `remove-vitae` | Developer | Removes vitae from the appraised player or yourself (developer_commands.rs:4524). | - |
| `removeenc` | Developer | Needs the overlay (same panic, :1430). | - |
| `removeinst` | Developer | Needs the overlay (same panic, :1430). | - |
| `rename` | Envoy | - | - |
| `reportbug` | Player | Prints the issue tracker URL (Empyrean's). | - |
| `requirecomps` | Developer | Toggles spell-components-required on you (developer_commands.rs:3853). | - |
| `resist-info` | Developer | Lists the eight resistances sorted descending (developer_commands.rs:4610). | - |
| `resyncproperties` | Admin | Prints no confirmation. | - |
| `roll-treasure` | Developer | - | - |
| `run` | Sentinel | - | - |
| `safecomps` | Developer | Sets safe spell components on you (developer_commands.rs:3878). | - |
| `save` | Sentinel | Saves position as sanctuary or recall 1-9 (admin_commands.rs:1144). | - |
| `save-now` | Developer | - | - |
| `save-world` | Admin | - | - |
| `set-accountaccess` | Admin | A bad level silently becomes Player (as ACE). | - |
| `set-accountpassword` | Admin | Unknown account is handled. | - |
| `set-characteraccess` | Admin | Works online and offline. | - |
| `set-shutdown-interval` | Admin | Parses 0-99999, prints usage on bad input. | - |
| `setglobalenviron` | Developer | - | - |
| `sethealth` | Developer | - | - |
| `setlbenviron` | Developer | - | - |
| `setposition` | Developer | - | - |
| `setvital` | Developer | - | - |
| `show-wielded-treasure` | Developer | Prints the WieldedTreasure table for a weenie (developer_commands.rs:5542). | - |
| `showprops` | Admin | - | - |
| `showsession` | Developer | Shows IP, ports, client id and account of the appraised player (developer_commands.rs:3815). | - |
| `showstats` | Developer | - | - |
| `showtier` | Developer | Shows DeathTreasure tier of the appraised creature (developer_commands.rs:5119). | - |
| `showvelocity` | Developer | Prints velocity and cached velocity (developer_commands.rs:4854). | - |
| `shutdown` | Admin | Begins shutdown with optional broadcast message. | - |
| `smite` | Envoy | Selected target, `all`, or a player name (admin_commands.rs:1217). `all` is case-sensitive. | - |
| `source` | Player | - | - |
| `spendallxp` | Developer | - | - |
| `stop-now` | Admin | Sets interval 0 then shuts down. | - |
| `tele` | Advocate | Returns silently for an Advocate below level 5 (as ACE). | Follow-up: Advocates below level 5 get no answer (as ACE); say why. |
| `teledungeon` | Developer | - | - |
| `teleloc` | Developer | Teleports to cell and coordinates (admin_commands.rs:1608). | - |
| `telepoi` | Developer | Lists POIs or teleports you to one (admin_commands.rs:1494). | - |
| `telereturn` | Sentinel | Returns the player to the saved return position (admin_commands.rs:1383). | - |
| `teleto` | Sentinel | Teleports you to the named online player (admin_commands.rs:1324). | - |
| `teletome` | Sentinel | Teleports the player to you and saves their return position (admin_commands.rs:1347). | - |
| `teletype` | Developer | - | - |
| `telexyz` | Developer | A bad number returns silently (developer_commands.rs:1161). | - |
| `tiermobs` | Developer | Scans weenie content for creatures with that tier (developer_commands.rs:5141). | - |
| `time` | Envoy | Prints UTC, in-game time and time of day (admin_commands.rs:1629). | - |
| `trophies` | Envoy | Dumps the creature's create list and wielded drops (admin_commands.rs:1653). | - |
| `turnto` | Developer | Errors go to the server console only. | Follow-up: Errors go to the server console only. |
| `unban` | Sentinel | - | - |
| `ungag` | Sentinel | Calls un_gag_player and reports the result (admin_commands.rs:907). | - |
| `ungod` | Sentinel | - | - |
| `usewith` | Developer | Uses the guid on the last appraised object (developer_commands.rs:6230). | - |
| `vendordump` | Developer | Full property dump of the selected vendor (developer_commands.rs:5890). | - |
| `version` | Admin | Console prints version info. | - |
| `watchmen` | Admin | - | - |
| `we` | Developer | Same as gamecastemote. | - |
| `whoami` | Developer | - | - |
| `world` | Admin | open/close/boot work (admin_shard_commands.rs:382). | - |

## Follow-ups

Bigger fixes this change leaves alone, each small enough for a change of its own:

- `show-allegiances`: answer in the caller's chat; `empyrean_world::world_objects::allegiance::show_info` writes to the server console.
- `event`: Port `enable`, `disable` and `clear`, and let `status` list every event.
- `debugchess`: Send the move history to the caller instead of the server console.
- `debugboard`: Send the board to the caller instead of the server console.
- `barrier-test`: Answer in chat, and say when the landblock is not loaded instead of panicking.
- `import-json`: Without `server.world_overlay_path` the import panics uncaught (developer_content_commands.rs:1430); say the overlay is needed. Same for import-sql-folders, createinst, removeinst, addenc, removeenc.
- `serverstatus`: Report the process's real thread count, CPU time, uptime and memory instead of zeros.
- `databaseperftest`: ACE never fills its biota list, so every stage reports 0/0; port the test properly or remove it.
- `effect`: Use the scale argument.
- `MoveTo`: Use the distance argument, or drop it.
- `getspellformula`: Say when the spell id is unknown instead of panicking.
- `getallspellformula`: Say which spell lacks a component instead of panicking.
- `movement`: Say when the argument is not a number instead of panicking.
- `givemana`: Say when the argument is not a number instead of panicking.
- `cisalvage`: Say when a material has no salvage bag instead of panicking.
- `deathxp`: Compute the kill XP rather than print only the XP override.
- `pk`: Refuse an unknown argument instead of reporting success.
- `addspell`: Say when the spell id is unknown.
- `tele`: Advocates below level 5 get no answer (as ACE); say why.
- `attackable`: Advocates below level 5 get no answer (as ACE); say why.
- `turnto`: Errors go to the server console only.
- `debugemote`: The confirmation goes to the server console only.
- `vloc2loc`: Needs Content's vlocs/vlocDB.txt; say so when it is missing.
- The removed retail placeholders (`freeze`, `limbo`, `snoop`, `repeat`, `raise`, `trainskill`
  and the rest) could be written from scratch; ACE never implemented them.
- `wave-export` and `image-export` could come back once the dat crate can tell a wave or texture
  file apart and write it out.
