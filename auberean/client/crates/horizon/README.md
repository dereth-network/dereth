# Horizon for Auberean

Horizon is the client's own interface, beside the game's. It is chosen in Options: the client
page's Interface choice in the game's own options window, or the Interface row of Horizon's
settings. The choice is kept in the preferences file, and switching keeps the session: the player
stays in the world, and the log carries over (each interface shows the chat lines written while
the other was shown).

The desktop client carries Horizon's art. The browser's fetches it when the page starts, and until
it is in, choosing Horizon is refused, saying the art is not here; a player whose kept choice is
Horizon waits for it before the client starts.

Underneath it is the same game: the same session, the player's scripts, the world and its camera.
Horizon reads the game through the client's view and asks for things as the client's actions, as
the game's own interface does.

## What it has so far

- **Before the world** it shows its own screens, and logs in itself with the account the client
  was started with:
  - **Connecting:** how the login and the data download go.
  - **Character select:** over a view of Cragstone, the camera turning slowly across the town
    (or of the Drudge Citadel from above its vault, as the settings choose),
    with the world's name and an online lamp at the top and the client's version at the bottom
    left. The account's characters are down the right, the selected one standing in the middle as
    it looks, playing its idle. Click a character to select it, and click it again (or press Enter)
    to enter the world with it, after a question. Delete asks before deleting; a character being
    deleted shows how long until it is gone, and the same button restores it. Create opens
    creation while the account has room; Credits shows the game's credits; Quit closes the
    client.
  - **Creation:** the race and sex, a slider for each kind of appearance choice the race has
    (Height makes the whole figure taller or shorter), Randomize, and the name. Create makes the character and enters the world with it once the
    game says it is made, or says why not. Drag across the figure to turn it; Back returns to
    character select.
  - **Disconnected:** why the session ended, Log in to log in again, and Quit.
  - **Logging out:** the HUD goes and a box says the client is logging out, over the world, for
    as long as the server keeps the character (about 19 seconds on the game's own servers, the
    character playing its leaving near the end); then portal space, kept in sight two seconds
    more, and character select.
- **The parameter bar** along the foot of the screen: health, vigor and focus, with a thin
  experience line under them showing how far the player is through its level (the numbers over
  it).
- **The target bar** at the top: the selection's name, and its health once the game has said it,
  with no frame round it.
- **The selection's marks** in the world: four arrows round the selection, pointing in at it, with
  its name over it, red while the player attacks. With the selection off the screen, one arrow at
  the screen's edge points the way to it.
- **The status row** at the top right: the effects on the player, up to three rows of nine, each
  with what is left of it.
- **The hotbar** over the parameter bar: the shortcut bar's set shown, ten slots a row, as many
  rows as the game's options give (up to four). Click a slot to use it, right-click it to examine
  what it holds, drag it onto another slot to move it or off the bar to empty it (with Shift while
  the slots are locked). The arrows at its end show the next and the previous set. A skill waiting
  is shaded as long as it waits; a skill's special attack lights its slots: amber while asked for,
  gold once it starts, blue while it is turned on.
- **The attack button** right of the hotbar: an icon with no words or frame, the dove at peace and
  the axe, glowing, while the player attacks. A click starts or stops attacking; a player who
  targets on its own has the nearest foe picked first.
- **The skill window** (C): one skill panel at a time as a tree, growing up from its first row at
  the bottom as the game's own does, and scrolling when it is taller than the window. The tabs
  across the top show each kind of skill's first panel, Back the panel before. Press a skill to
  select it; the foot shows its level and points and lets it be trained, raised or untrained
  (asking first where it can't be undone). Drag a trained skill onto the hotbar to put it there.
- **The examine window** (G): what is examined. With Classic Examination on (in Options) every
  tab's lines show on one page; with it off they are on the General, Details, Effects and Usage
  tabs, and a tab with nothing on it is disabled.
- **The inventory window** (I or B): what the player wears around its figure, the pack in four
  segments (Pack and Bags 1 to 3, one shown at a time by its tab), and the money. Press an item to
  select it, press it again soon after to use it, right-click it to examine it, and drag it:
  - to another slot (or a bag's tab, after that bag's last item) to move it; a stack asks how
    many first;
  - to a place on the paper doll to wear it there, or onto the figure to wear it at its own place;
    a worn item dragged to the pack is taken off;
  - onto the money to turn it into gold (after a question, unless that is turned off in the
    game's options or Shift is held);
  - onto the hotbar to make a shortcut of it, onto another window (below), or onto the world, where
    it goes to what is under the pointer: someone, a container, or the ground.

  Drag the coins out of the money to take some out, into the pack or onto the world.
- **The loot window**: what an opened chest or corpse holds. Drag its items to the pack, or take
  them all; Make Coins turns them into gold. Closing it closes the container.
- **The trade window**, once a trade opens (the game's question asks when someone asks to trade),
  or while the player waits for an answer to its own ask: the player's offers beside the
  partner's. Drag an item from the pack onto the player's side to offer it, and drag an offer off
  it to take it back. Accept and Decline wait for the game's answer. When the partner closes the
  trade the window says so until it is closed.
- **The shop window**: a shop's name and description and its page of sales, what each costs and
  how many are left. Press a sale to select it; press it again, or Buy, to buy it (asking how many
  when more than one may be bought), or drag it into the pack. The game asks to confirm each
  purchase. The arrows show the next page and the one before.
- **The market**: its shops, each entered with a second press or Enter, and My consignments: the
  player's own sales, on sale, expired or sold. Collect, or dragging one to the pack, collects its
  coins or its item. Drag an item onto a consignment of the same kind to restock it, or anywhere
  else on the list (or New) for a new sale: its item, its price, and what putting it up costs.
  While a sale waits to be collected, Sold! lights over the menu bar.
- **The map** (M): the world's map, zoomed with the wheel about the pointer, or with its plus and
  minus about the player, and back out with World; drag it to pan. Zoomed in, the game's closer
  maps show, with their labels and the markers the game puts on the map; the one under the
  pointer is named with its map coordinates, and a click on a marker writes its words under the
  map. The player's arrow shows where the player stands outdoors; under the map, the player's map
  coordinates (or Indoors), and the game's date and time of day.
- **The journal** (J or L): the quests Underway and Finished, newest first, each with its time
  left. Choose one to read its description, its journal entry and how hard it is (or how it
  ended); Abandon asks before it gives a quest up. A quest the game changes is marked and chosen
  until its row is pressed. The Story page lists the story's acts and their scenes; Play plays a
  scene seen before again.
- **A book**, a note or a sign the game opens, a page at a time: a press on the page turns it,
  and after the last closes the book. A book with page controls has the arrows, its page count and
  Copy.
- **The story's scenes** over the whole screen in Horizon's frame, the world hidden meanwhile:
  each picture fading in over the last, its subtitle under it, its music and voice. Escape cuts
  them short, except a scene on the way through a portal, which lasts until the player arrives.
- **The minimap** at the top right, under the status row: what the game puts on its radar around
  the player, in the game's own colors and shapes, north up, the player's arrow at the middle.
  Under it, where the player stands: the map coordinates outdoors, "Inside" in a building or a
  dungeon. Its plus and minus zoom in to a quarter of the radar's reach and back out. Over an
  object it names it, and a click selects it.
- **The party list** down the left while the player is in a fellowship: the player first, then
  the fellows, each with its health (what other players took of it shown past what is left) and
  vigor, the leader marked. A click selects a fellow, a quick second click starts a tell to it, and
  F1 to F9 select the first nine.
- **The conflict lamp** at the status row's left end: whether the place is at peace, controlled,
  kingdom against kingdom or free for all, as a picture for each where the art has them; over it,
  the player's kingdom.
- **The social window** (O), a tab each:
  - **Fellowship:** out of one, a name, whether the experience is shared and how loot is shared,
    and Create. In one, its fellows with their levels, shares and vitals; Leave, and for the leader
    Recruit and Dismiss (the one selected) and Disband.
  - **Allegiance:** the allegiance's name, size and monarch, the member shown (the player at
    first), its patron and vassals, and the message of the day. Home, Up and a vassal's row move
    through the tree; Swear offers the player's oath to the player selected, and Break asks first.
  - **Friends:** the player's own state (Available, Busy, Away), the friends online and offline;
    Add the name typed or the player selected, Remove and Tell the friend chosen.
- **The game's questions** in a box over the middle of the screen, one at a time: the player
  died (Resurrect), someone offers to bring the player back, an effect asks whether it may go on,
  a message to read (OK), a fellowship's place, an oath or a trade offered (Yes or No).
- **The log window** at the bottom left, one translucent pane as in Dereth's Horizon: the game's
  four chat windows as its tabs, each named for its channel (Main, General, Fellowship and
  Allegiance for a new character) unless named on the settings' Chat page, and showing what its
  filter lets through, each kind of text in its color. Click a tab, or press Shift+F2 to F4, to show
  it. Right-click a tab for its menu, as in Dereth's Horizon: Filters, a box for each kind of text
  its window shows, and Pop Out, which takes it (any but the first) into a window of its own, moved
  by its tab and sized by its corner, where it was put and its size kept; that window's tab's menu
  has Dock, which puts it back. Closed, it shows only the log; the entry
  opens under the lines while a line is typed. Drag the mark in its bottom right corner to size it;
  the size is kept. The entry names the channel it speaks on: a tab shown speaks on its own, a
  channel's command at the start of the line (`/cg `, `/ct `, `/f `, `/a `, `/s `...) switches to
  it, and Tab steps to the next channel; the channel stays after a line is sent. Enter opens the
  chat entry and `/` opens it with a `/`; Enter sends the line to the game's command line and Escape
  drops it. Up and Down step through the lines sent before. The wheel over the window, or its
  scrollbar, scrolls it back.
- **The crafting window** (N): the player's craft skills down its left, each with its rating
  (green while raised for now, red while lowered; a skill whose use earns less is dimmed). Press a
  skill to select it: across the top its level, the experience to spend and what raising it
  costs, and Raise; below, its categories of recipes and the selected category's recipes (dimmed
  until a recipe may be made again). Press a recipe to examine it; press it again, or Make, to
  open it for making. Drag a recipe onto the hotbar to make a shortcut of it.
- **The making window**, once the game opens a recipe for making: its name colored by how hard it
  is for the skill (gray far below it, green, blue, gold, orange, red far above it, dark red past
  what the skill may make), what making it costs, its charges, the spinner, and a slot for each
  ingredient it takes. Drag an item onto a slot to put it in (the slot waits for the game's
  answer); drag it out to take it out. Fill in fills the slots that fill themselves. Craft makes
  it (not while mounted, out of the spinner's range or past what the skill may make; its tip says
  why); the window then says how it went, and closes when the recipe has no charges left.
- **The pets** at the right, under the minimap, while the player has pets (unless the game's
  options hide them): each pet's icon, its name (red a few seconds after something attacks it)
  and its mode between arrows that turn it through the game's nine modes. Press a pet to select
  it; right-click it to give it another name. Attack sends the pets at the selection; Return,
  Buff and Heal command them.
- **Dismount** beside the attack button while the player rides a mount.
- **The menu bar** at the bottom right, on Dereth's Horizon's plate: a button for the map, examine,
  inventory, skills, crafting, quests, fellowship and allegiance windows, lit while the window is
  open, and a last one whose menu holds Settings, HUD Layout, Log Out and Exit Game.
- **The settings** (the menu bar, or K), a tab a page:
  - **General:** the interface, the camera's movement and directions, whether the map
    coordinates show under the minimap, the interface scale, HUD Layout, and Log Out.
  - **Graphics**, **Sound** and **Camera:** the client's own preferences, as the game's own
    options window lists them. Each one set takes effect and is kept at once.
  - **Character:** the player's options, kept by the server (offers heard, damage numbers, the
    hotbar's numbers, lock and rows, what the minimap shows).
  - **Chat:** for each of the four chat windows, its tab's name (blank for its channel's name,
    kept by Horizon), the channel it is given over to (a drop-down list), whether its tab lights up,
    and the kinds of text it shows.
  - **Key Bindings:** each of Horizon's actions and its keys, an action brighter where its keys
    are not Horizon's own. Click an action's keys and press a key to bind it there (Escape leaves
    them as they were); Restore Defaults puts Horizon's own layout back, and Apply writes Horizon's
    key files and binds them.
- **HUD layout**, in the settings: drag an element of the HUD to move it, and use the wheel over
  it to resize it. Save keeps the layout for this character (in
  `auberean-horizon-hud-<world>-<name>.txt` beside the preferences); closing the editor without
  saving puts back the layout last kept. Whatever its place and size, an element stays on the
  screen.

Escape closes the window on top.

## Keys and the camera

Horizon has its own key layout, so its keys never change the game's own interface's. Movement: W
forward, S back, A and D turn, Q and E strafe, Space jumps, R runs on, X stops. The arrows turn and
tilt the camera, and V held looks at the character's front. Enter opens the chat entry, `/` a
slash command and T a reply to the last tell, and Shift+F2 to F4 show the chat's second to fourth
tab; K the settings, C the
skills, G the examine window, I or B the inventory, O the social window, N the crafting window, M
the map, J or L the journal; F1 to F9 select the party's fellows; P and Shift+P the next and
previous of the player's pets; Tab and Shift+Tab pick the next and previous creature; F uses the
selection; 1 to 0 use the hotbar's first row, Ctrl with them its second, `-` and `=` show the set
before and after, and `` ` `` starts and stops attacking; F12 takes a screenshot; Shift+Escape logs
out. The layout is kept in `auberean-horizon.keymap` (and its `-shift` and `-control` files) in the
settings folder, beside the player's own key maps, and changed on the settings' Key Bindings page.

In the world Horizon shows the orbit camera. A press over the world turns it while held, and the
wheel zooms it. A left click on something that did not turn the camera selects it, and a quick
second click on it uses it. In the settings, the movement is camera-based (W runs where the
camera looks) or character-based (the game's own), and the mouse can turn the camera the other
way across or up and down.

## With a pad

Press any button on a pad to switch gamepad mode on (that press does nothing else); press a key
to switch it off. The mouse still works beside it.

| In the world | |
|---|---|
| Left stick, right stick | move, and turn the camera |
| A | use the selection; with none, select the nearest creature |
| B | let go of the selection and stop |
| X, Y | the map; jump |
| LT or RT held | the left or right cross hotbar, used with the d-pad and the face buttons: the first eight slots of the set shown (LT) or of the row over it (RT) |
| RB held with a face button or the d-pad | show that set of the shortcut bar (Y, B, A, X the first four, the d-pad's up, right, down, left the next four) |
| D-pad up and down, left and right | the fellowship's members; the creature before or after |
| LB | tap for autorun; hold to zoom with the right stick |
| Left stick pressed | start or stop attacking |
| Start | the settings |

In a window, the d-pad moves the focus, A presses, B closes the window, LB and RB turn its tabs,
the right stick scrolls, and Back goes to the next window open. On the screens before the world,
the d-pad moves the focus over the characters and the buttons, A presses, B is Escape and Start is
Enter. In gamepad mode the cross
hotbars stand over the hotbar, the one held lit.

## Plugins

The desktop client runs the plugins in the `plugins` folder beside its preferences. Horizon draws
their windows and HUDs in its own art: the plugin bar at the top left has a button for each
plugin window, which opens and closes it; a window's close box closes it too. A HUD stands at the
place its plugin anchors it. A line typed in the chat entry that a plugin claims is the plugin's,
and what plugins print shows in the log.

## Scale

The interface is drawn at 100%, 150%, 200% or 300%. A step is offered only when the window is big
enough for every screen at that step: 150% from 2160 by 1296 pixels, 200% from 2880 by 1728, 300%
from 4320 by 2592. Auto, the default, picks the largest step the window offers, up to 200%. A
chosen step the window doesn't offer is kept, and Auto's is used until the window offers it again.
The choice is kept in `auberean-horizon.txt` beside the preferences.

## The code

- `src/lib.rs`: the interface the shell drives, and how it logs in.
- `src/pregame.rs`: the game before the world as Horizon follows it; `src/creation.rs`, a
  character being made.
- `src/shared.rs`: the maker the hosts use, which offers the game's own interface and Horizon.
- `src/state.rs`: the game as Horizon draws it, followed from the view, the notices and the events;
  the shortcut bar, combat and the skills in `src/abilities.rs`, what is examined in
  `src/examined.rs`, the player's things (the pack, a trade, the shops) in `src/state/goods.rs`,
  the fellowship, allegiance and friends in `src/social.rs`, crafting in `src/state/craft.rs`, the
  pets and the mount in `src/state/pets.rs`.
- `src/ui/`: the screens before the world (`pregame.rs`, with creation in `creation.rs`), the HUD
  (`hud.rs`, with the menu bar), the selection's marks (`marker.rs`), where the HUD's elements
  stand and their editor (`layout.rs`), the hotbar and the attack button (`hotbar.rs`), the minimap
  (`minimap.rs`), the log window (`chat.rs`), the game's questions (`questions.rs`), the skill
  window (`skills.rs`), the examine window (`examine.rs`), the party list and the conflict lamp
  (`party.rs`), the social window (`social.rs`), the windows of things (`goods.rs`, which holds
  `inventory.rs`, `loot.rs`, `trade.rs`, `shop.rs`, `market.rs` and what they share in
  `items.rs`), the crafting window (`crafting.rs`), the making window (`making.rs`), the pets and
  the Dismount button (`pets.rs`) and the settings (`settings.rs`, with its pages of options in
  `options.rs` and the key bindings in `bindings.rs`).
- `src/figure.rs`: a figure in a viewport: the player's in the inventory window, a character's
  before the world.
- `src/keys.rs`: the keys' actions Horizon carries out (its layout is the shell's).
- `src/art.rs`, `src/draw.rs`, `src/scale.rs`: the art, the batches it is drawn as, and the scale.
- `src/own.rs`: Horizon's own settings and their file.
- `src/ui/plugins.rs`: the plugins' windows, HUDs and bar.
- `src/pad.rs`, `src/gamepad.rs`: the pad read frame by frame, and gamepad mode; the cross
  hotbars in `src/ui/cross.rs`.
