# Conformance cases

**Provenance: design.** Generated from the plugin interface's own code, so it cannot drift from it. Do not edit it: `DERETH_TEST_GOLDEN=1 cargo test -p dereth-testkit --test cpu plugins::the_plugin_reference` rewrites it.

Every case of the conformance suite, by profile: what it proves (its `expect.toml`'s `says`) and what it runs in. [conformance.md](../conformance.md) is what a case is, what each profile covers and how a host runs them; each case's folder holds its plugin and its expectations.

| profile | cases |
|---|---|
| [Core](#core) | 25 |
| [UI](#ui) | 16 |
| [Overlay](#overlay) | 6 |
| [Nav](#nav) | 17 |
| [Pregame](#pregame) | 3 |
| [Peers](#peers) | 1 |
| [Automation](#automation) | 18 |
| [UtilityBelt scripts](#ubscript) | 7 |

## Core

| case | proves | runs in |
|---|---|---|
| [`appraise`](../conformance/core/appraise/) | An appraisal settles its promise with what it told and raises the appraised event; one nobody answers fails as a timeout. | a scripted world |
| [`budget`](../conformance/core/budget/) | A handler that runs past its time budget is stopped, and a plugin that does so three times within a minute is suspended. | the host's frames alone |
| [`chat-filter`](../conformance/core/chat-filter/) | A plugin hides incoming lines, and those lines are not shown. | the recorded session `long-solo-play` |
| [`chat-lines`](../conformance/core/chat-lines/) | A plugin hears every chat line in the order it arrived, with its kind, its type's number and its place in the frame, and its own local line is printed without being sent. | the recorded session `long-solo-play` |
| [`chat-send`](../conformance/core/chat-send/) | A line a plugin sends is never taken for a command, however its verb is spelled and whenever the verb is claimed: it reaches neither a plugin nor the host and opens no activation, while the player's own typed command still does. | a scripted world |
| [`commands`](../conformance/core/commands/) | A command a plugin claims reaches it with its arguments when the player types it, and a verb that is one of the client's own commands cannot be claimed. | the recorded session `long-solo-play` |
| [`events-order`](../conformance/core/events-order/) | A plugin hears the login, the entry into the world, and then each change of experience, vitals and selection, in the order the client applied the messages behind them. | the recorded session `long-solo-play` |
| [`faults`](../conformance/core/faults/) | A plugin whose handlers keep failing is suspended after ten failures within a minute, and the client carries on. | the host's frames alone |
| [`gestures`](../conformance/core/gestures/) | Without a gesture an action needs the world's leave: a published policy that allows a domain only with a gesture refuses it, and one that denies a domain refuses it with a gesture too; a character's advancement always needs a gesture; and a typed command makes the plugin's actions interactive. | a scripted world |
| [`loot-by-hand`](../conformance/core/loot-by-hand/) | A click starts a loot: the corpse is opened, only the item the profile cannot decide unseen is appraised, and each keeper is moved to the pack after the one before has arrived. | a scripted world |
| [`messages`](../conformance/core/messages/) | A plugin hears each message the client sent and received, by its name or as any message, decoded into its fields, and has no way to send one. | a scripted world |
| [`net`](../conformance/core/net/) | A plugin's HTTPS requests reach only the domains its manifest lists (and their subdomains), at the port listed (443 when none is), over https and never to an address; a request to another domain fails, saying so, and each settles with the answer or why it failed. | the host's frames alone |
| [`net-unnamed`](../conformance/core/net-unnamed/) | A plugin whose manifest does not name net.http makes no HTTPS request, even to a domain it lists: the request fails, saying so. | the host's frames alone |
| [`objects`](../conformance/core/objects/) | A plugin hears an object's state change with the keys that changed and its walking past a metre, reads every property type an appraisal or the object's own qualities carry and a creature's health, and finds objects by property conditions, class, distance and a predicate. | a scripted world |
| [`policy`](../conformance/core/policy/) | A world's published policy document sets each domain's rule, denies what it leaves out, and is what plugins read. | a scripted world |
| [`refused`](../conformance/core/refused/) | A plugin that needs a capability the host does not have is not loaded, and the player is told why. | the host's frames alone |
| [`sandbox`](../conformance/core/sandbox/) | A plugin has no file, process or source-loading functions, cannot change the libraries or the API, and can require only modules in its own package. | the host's frames alone |
| [`spell-book`](../conformance/core/spell-book/) | The spell book comes in id order, whole or a page at a time (counting from 1); a page past the end is empty; each spell carries its category and how long it lasts. | a scripted world |
| [`store`](../conformance/core/store/) | The character store opens once a character is logged in, and holds numbers, strings, lists and tables but not functions. | the recorded session `long-solo-play` |
| [`store-scopes`](../conformance/core/store-scopes/) | The global, world and character stores each keep plain values, and a write past the quota is refused without changing the store. | a scripted world |
| [`timers`](../conformance/core/timers/) | A repeating timer fires on its period until cancelled, a one-shot fires once, and a task that sleeps wakes after its delay, all on the frame clock. | the host's frames alone |
| [`trade-and-salvage`](../conformance/core/trade-and-salvage/) | A trade and a salvage end at the server's word: the vendor's wares open, an item is sold, five of a stackable are bought (the stack the vendor's list gives it, not one), and an item salvaged leaves the pack for a bag of its material; a loot profile's salvage combining reads as its ranges. | a scripted world |
| [`ui-window`](../conformance/core/ui-window/) | A declared window is kept as a tree; a click and a change reach the plugin as events, the plugin's property changes show in the tree, and a malformed node, a text over 4,096 characters, a window over 64 KiB of text and a canvas over 1,000 commands are refused. | the host's frames alone |
| [`unasked`](../conformance/core/unasked/) | A plugin whose manifest names no permission is asked nothing and may do what every permission but net.http covers, and in a world that publishes no policy it acts without a gesture in every domain. | a scripted world |
| [`use-on`](../conformance/core/use-on/) | An item used on a target sends one use-with-target request and settles at the server's acknowledgement: a wielded caster's own spell cast on the character keeps the caster in hand, a kit from the pack is used on the character and a lockpick on a door; an item used on what it does not take is refused by the client with nothing sent, and a use's purpose is only a heal. | a scripted world |

## UI

| case | proves | runs in |
|---|---|---|
| [`bar`](../conformance/ui/bar/) | The plugin bar lists each plugin active here with what it says it is doing (running, paused or idle; error when it stopped on a fault); a click on it opens its main window, or closes it when open, and the plugin hears ui.opened or ui.closed. | the host's frames alone |
| [`canvas`](../conformance/ui/canvas/) | A canvas keeps its draw commands and takes new ones, a chart keeps its series, and a draw command of no known operation is refused. | a scripted world |
| [`controls`](../conformance/ui/controls/) | A tabs node, a number and a choice are kept in the tree; the player's change of each reaches the plugin as an event and stays on the node, and a tabs node that holds anything but group pages is refused. | the host's frames alone |
| [`hotkeys`](../conformance/ui/hotkeys/) | A hotkey runs its plugin's command as if the player had typed it, a gesture; a hotkey whose command the plugin has not claimed (the host's own `/plugin` among them) is told to the player and runs nothing, and a hotkey with no Ctrl or Alt is refused. | the host's frames alone |
| [`hud`](../conformance/ui/hud/) | A HUD is kept as a tree with its anchor, its nodes change like a window's, and a bad anchor, a HUD inside a window or a control in a HUD other than a button, a toggle or a list is refused. | the host's frames alone |
| [`hud-unlock`](../conformance/ui/hud-unlock/) | While the player has the HUDs unlocked (the plugin panel's toggle, or /plugin huds unlock), a HUD's controls take no input: a press on one moves the HUD, and its plugin hears nothing; locked again, they answer. | the host's frames alone |
| [`image`](../conformance/ui/image/) | An image node shows a PNG from the plugin's package, or one it fetched as an image, by name; a fetched file that is not a PNG fails, and a picture over 1024 pixels a side is not kept, from the package or fetched. | a scripted world |
| [`inputs`](../conformance/ui/inputs/) | Text typed in an input, a slider moved and text typed in a text box reach the plugin as changes and stay on their nodes; list cells carry their own colour and icon, nodes their tooltips, and a toast shows with the plugin's name and goes when its time is up. | the host's frames alone |
| [`layout-auto`](../conformance/ui/layout-auto/) | Where no node says grow, the last list or text box of the window, and of each tab's page, grows; what holds a growing node grows with it. | the host's frames alone |
| [`layout-bounds`](../conformance/ui/layout-bounds/) | A node keeps within its bounds: a growing node stops at its most height (max_height, in lines), the room it leaves going to the other growing nodes, and across a column a node is no wider than its most (max_width, in cells). | the host's frames alone |
| [`layout-columns`](../conformance/ui/layout-columns/) | A list's columns share its width: a column of a width is that many cells, the growing columns share what is left, and where no column says grow the last one of no width grows; a column's least width holds the list at least that wide, and a window narrower than its nodes' least width has them laid out at it. | the host's frames alone |
| [`layout-grow`](../conformance/ui/layout-grow/) | A window's nodes share the room left down it: each growing node takes a part in proportion to its grow, a node that does not grow keeps its natural height (a row whose field grows across it too), and across a column every node is as wide as the column. | the host's frames alone |
| [`layout-scroll`](../conformance/ui/layout-scroll/) | A window shorter than its nodes' least height scrolls through them: they are laid out at their least height (a list at three rows), as wide as the body less the scrollbar's room. | the host's frames alone |
| [`layout-stretch`](../conformance/ui/layout-stretch/) | Across a column every node is as wide as the column but a picture, which keeps its own size; in a row a list is as tall as the row and a button stands in its middle; a row's justify end puts the room its nodes leave before them, and a column's align center keeps its nodes at their natural width in its middle. | the host's frames alone |
| [`layout-tabs-scroll`](../conformance/ui/layout-tabs-scroll/) | A window with tabs shorter than its page keeps its row of tabs where it is and scrolls the page alone: the body does not scroll, and the page's nodes are laid out at their least height, as wide as the page less the scrollbar's room. | the host's frames alone |
| [`resize`](../conformance/ui/resize/) | A window declared resizable takes the size the player gives it, within the bounds it declared (cells wide, lines high), keeps it when the plugin declares the window again, and the plugin hears ui.resized. | the host's frames alone |

## Overlay

| case | proves | runs in |
|---|---|---|
| [`depth`](../conformance/overlay/depth/) | A shape, a world marker or a nametags layer may ask to stay on top of whatever stands in front of it, and a nametags layer labels only its nearest objects, nearest first and as many as its limit. | a scripted world |
| [`journey`](../conformance/overlay/journey/) | The host draws a plugin's journey while it runs: an arrow to where the leg goes, the way still to go on the map, and the portal ahead on the map and radar. | a scripted world |
| [`journey-hidden`](../conformance/overlay/journey-hidden/) | A journey asked for with show = false is not drawn, and nav.show_journey hides a drawn one; it answers false for a plugin with no journey. | a scripted world |
| [`map-click`](../conformance/overlay/map-click/) | A press on the game's map reaches the plugin listening with the place under the pointer, the button and the modifiers held, and is no gesture; the place is one the markers take, and a press off the world is not heard. | a scripted world |
| [`markers`](../conformance/overlay/markers/) | A plugin's markers are kept with what they mark and resolved to where it is each frame; a marker goes when removed or when its time is up, and the arrow's turn and distance are from the character. | a scripted world |
| [`shapes`](../conformance/overlay/shapes/) | A plugin draws lines, rings, squares and floating text in the world, each resolved every frame to an outline in the world where its object or place is, and the nametags layer labels each object of the kinds it names within its range. | a scripted world |

## Nav

| case | proves | runs in |
|---|---|---|
| [`automated`](../conformance/nav/automated/) | A movement with no gesture behind it needs the world's leave: a world that allows navigation only with a gesture refuses it. | a scripted world |
| [`follow`](../conformance/nav/follow/) | A follow closes on its object to within range and holds there until stopped; stopping it lets the run lock go and fails its promise as stopped. | a scripted world |
| [`journey`](../conformance/nav/journey/) | A journey to a dungeon walks to the portal that leads there, uses it and walks on to the goal; its legs and what the use taught reach the plugin as events, its promise settles on arrival, and the world's store keeps the portal's destination as learned. | a scripted world |
| [`journey-automated`](../conformance/nav/journey-automated/) | A journey with no gesture behind it needs the world's leave: a world that allows navigation only with a gesture refuses it. | a scripted world |
| [`journey-plan`](../conformance/nav/journey-plan/) | A plan answers the journey's legs and expected seconds without moving the character; with portals left out, a dungeon has no journey. | a scripted world |
| [`journey-refused`](../conformance/nav/journey-refused/) | A portal that refuses the character is learned as refused, and the journey is planned again by another way, which is told with the reason. | a scripted world |
| [`journey-stop`](../conformance/nav/journey-stop/) | nav.stop ends the plugin's journey, which fails as stopped; the player's own movement ends one as interrupted. | a scripted world |
| [`jump`](../conformance/nav/jump/) | A jump faces its heading, charges for the time asked holding the walk key and the direction it leaps in, then leaps; with no direction it goes straight up; the player's own movement while it charges puts it down without a leap, and a charge past the full bar or an unknown direction is refused. | a scripted world |
| [`motion`](../conformance/nav/motion/) | A held motion presses one of the game's movement controls, with the walk key when asked, for the time asked and lets it go; an unknown motion or one held longer than ten seconds is refused. | a scripted world |
| [`move-to`](../conformance/nav/move-to/) | A move to a place turns the character toward it, runs with the run lock, and lets it go within range; the promise settles on arrival. | a scripted world |
| [`path`](../conformance/nav/path/) | A walking route goes round a wall to a monster behind it, through a closed door in a wall's gap (naming the door to open), and not at all to a place walled in; a line of sight is blocked by a wall and by a closed door and clear across open ground; nothing moves the character. | a scripted world |
| [`player-first`](../conformance/nav/player-first/) | The player's own movement ends a plugin's movement at once, and the run lock is then the player's: the host does not let go of it for them. | a scripted world |
| [`player-yield`](../conformance/nav/player-yield/) | A movement no gesture asked for gives way to the player: the player's own movement ends it, and for five seconds after it such a movement is refused as player_moving. | a scripted world |
| [`released`](../conformance/nav/released/) | A plugin unloaded in the middle of a movement lets go of the character: the movement stops and the run lock it set is let go. | a scripted world |
| [`route`](../conformance/nav/route/) | A route read from a VTank .nav file is walked point by point with the plugin's movement, its pause slept, and a once route ends. | a scripted world |
| [`server-control`](../conformance/nav/server-control/) | When the server takes control of the character's movement, a plugin's movement ends at once and the run lock the host set is let go, so nothing of the plugin's fights the server's approach. | a scripted world |
| [`suspended`](../conformance/nav/suspended/) | A plugin suspended in the middle of a movement lets go of the character: the movement stops and the run lock it set is let go, since the plugin can no longer do it. | a scripted world |

## Pregame

| case | proves | runs in |
|---|---|---|
| [`auto-login`](../conformance/pregame/auto-login/) | A plugin hears the character list, enters the world as the character it picks, hears why an entry was refused, and tries again. | a scripted pre-game |
| [`contexts`](../conformance/pregame/contexts/) | A plugin active only in the world hears the session at the character list but no ticks and shows no window there; it becomes active in the world, and idle again once it has logged out. | a scripted pre-game |
| [`denied`](../conformance/pregame/denied/) | In a world whose policy denies session control, a plugin cannot enter the world for the player. | a scripted pre-game |

## Peers

| case | proves | runs in |
|---|---|---|
| [`echo`](../conformance/peers/echo/) | A message a plugin sends reaches the same plugin in every other client on the machine, with the sending character, and not the sender itself; it is run by a scenario with two clients. | the host's frames alone |

## Automation

| case | proves | runs in |
|---|---|---|
| [`attack-outcomes`](../conformance/automation/attack-outcomes/) | An attack settles on what the server does with it: the end of the attack sequence after a swing is a swing made, a refusal before it fails with the refusal's code, and an attack the client did not make fails at once. | a scripted world |
| [`attack-released`](../conformance/automation/attack-released/) | A plugin unloaded while the attack it started is still repeating has the attack cancelled, so it does not go on with no plugin to stop it. | a scripted world |
| [`attack-target`](../conformance/automation/attack-target/) | An attack's swing goes only at its target: one whose target stops being selected while the power bar charges (another monster selected, as the client's automatic targeting does when a target's body goes) is dropped, never sent at the new selection, and fails target_gone. | a scripted world |
| [`combat`](../conformance/automation/combat/) | A plugin enters combat through the combat toggle, swings at a target with the combat window's own requests (height and power), and returns to peace; each settles on what the client saw. | a scripted world |
| [`combat-stop`](../conformance/automation/combat-stop/) | combat.stop cancels a swing already sent with the server, drops one still charging before it goes, and fails the plugin's attacks in flight or queued as stopped. | a scripted world |
| [`expressions`](../conformance/automation/expressions/) | Expressions in VTank's language evaluate with its operators, precedence, variables, lists, patterns and game functions; an unsupported function or a malformed expression is an error, not a value. | a scripted world |
| [`expressions-ub`](../conformance/automation/expressions-ub/) | UtilityBelt's additions to the expression language evaluate as it documents them: dictionaries, indexing and slicing, the list library with item expressions, math and bitwise operators, kept and shared variables, stopwatches, a lazy ifthen, exec, the calendar and the meta's own state; a bracket left open at the end is closed there. | a scripted world |
| [`heal-debuff`](../conformance/automation/heal-debuff/) | A spell is cast only in magic stance (the library takes up the caster and enters it); a Life spell cast as a heal is automated healing, which a world may allow while it denies buffing; the debuffer casts a debuff on a target once until its recast time has passed. | a scripted world |
| [`let-go`](../conformance/automation/let-go/) | Stopping is always allowed: with no gesture behind it and in a world that stops a player killer's automation, a plugin still stops the attack it started and lets go of the run lock it set, while starting again needs what any action needs. | a scripted world |
| [`met-import`](../conformance/automation/met-import/) | A VTank meta file imports as a meta checked in VTank's order: the first true rule of its state runs; its expressions, options, embedded route and meta view work, and what it could not carry is listed. | a scripted world |
| [`met-write`](../conformance/automation/met-write/) | A VTank meta file read as data writes back as VTank writes it, its embedded route and meta view kept, and a rule added as data runs once written and loaded. | a scripted world |
| [`meta`](../conformance/automation/meta/) | A meta checks its current state's rules on a timer and runs the action of each rule whose condition holds; states change on its actions, and its rules may move the character. | a scripted world |
| [`meta-once`](../conformance/automation/meta-once/) | A meta checked by VTank's rule runs every true rule of its state, each once per stay in it; setting the state it is in enters it again and its rules run again. | a scripted world |
| [`parity`](../conformance/automation/parity/) | In a world whose policy asks for read parity, a plugin's search finds only what the radar shows and what the character holds. | a scripted world |
| [`pk`](../conformance/automation/pk/) | In a world whose policy turns automation off for player killers, a player killer's plugin cannot act on its own, and still acts on the player's command. | a scripted world |
| [`portal-death`](../conformance/automation/portal-death/) | Entering and leaving portal space and the character's death are events, in order, and a meta's portal and death conditions hold at its next check after them. | a scripted world |
| [`support`](../conformance/automation/support/) | Beneficial spells cast on another character are their own domain, support: a world that denies combat and allows support lets a plugin take up its caster, enter the magic stance and buff another player for support, and refuses its debuff on a monster. | a scripted world |
| [`watchdog`](../conformance/automation/watchdog/) | A meta's watchdog enters its state when the character has not moved the distance in the time; setting it again does not restart its clock. | a scripted world |

## UtilityBelt scripts

| case | proves | runs in |
|---|---|---|
| [`actions`](../conformance/ubscript/actions/) | A script's actions go through UtilityBelt's queue over the plugin interface's actions: an immediate action at once, the rest one at a time and the highest priority first, each ending with its error, a named and prioritised action, a group in its own order, a removed action never run, and chat lines typed at the interface's pace. | a scripted world |
| [`basics`](../conformance/ubscript/basics/) | A script written for UtilityBelt's scripting runs from its folder alone: it reads its character, the world's objects by class and an object's properties by typed id as UtilityBelt answers them, waits on an appraisal and hears an action's callback. | a scripted world |
| [`events`](../conformance/ubscript/events/) | A script hears UtilityBelt's events over the plugin interface's: objects coming and going, the selection, a container opened with its items, a tell, speech and an emote with the speaker and the words, a line eaten, a handler that waits while the next is heard, Once and Until, and the world's tick. | a scripted world |
| [`huds`](../conformance/ubscript/huds/) | A HUD drawn the immediate way with UtilityBelt's views and ImGui is a declared window: each render builds its nodes, only what changed is set again, and a click, a toggle, text typed, a number stepped, a choice picked and a tab chosen reach the next render as ImGui reports them. | a scripted world |
| [`lifecycle`](../conformance/ubscript/lifecycle/) | A script in a package that names it a UtilityBelt script and is active before the world starts at the character list, logs a character in with UtilityBelt's action, hears the client's state change, loads a module of its own with its globals, and hears its own end when unloaded. | a scripted pre-game |
| [`modules`](../conformance/ubscript/modules/) | A script reaches UtilityBelt's other modules and globals over the plugin interface: its own folder read only and its data folder read and written, JSON, dates and spans, regular expressions, coordinates, marks in the world, enumerations and their flags, property tables by typed id or by number, vectors, and the client's message stream. | a scripted world |
| [`refusals`](../conformance/ubscript/refusals/) | What the plugin interface cannot do is refused plainly: a member, an event or a module that cannot be answered here raises an error saying why, and an action that cannot be carried out ends failed with the reason. | a scripted world |
