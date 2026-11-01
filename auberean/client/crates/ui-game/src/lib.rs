//! The game's panels on the interface's element framework, each finding its elements by name in
//! the retail layouts, as the September 2013 client's panels do.
//!
//! **Depends on** the element framework (`auberean-ui`), where a cell lies on the land
//! (`auberean-primitives`), the game's string references (`auberean-text`: the toolbar's
//! tooltips, the dialogs' messages, a place's map coordinates) and the original's float-to-integer
//! conversion (`dereth-primitives`). **Used by** the client's interface, which
//! opens the screens, feeds them what the model knows and turns what they ask into actions.
//!
//! - [`pregame`]: what the screens before the world share: what lives across them, what they ask
//!   of the client and what it tells them.
//! - [`intro`]: the opening pictures; [`datapatch`]: connecting and the data brought up to date;
//!   [`disconnected`]: why the session ended.
//! - [`character_select`]: the character list, the carousel of characters, enter, create, delete;
//!   [`keyboard`]: the key map it leads to.
//! - [`creation`]: character creation's hall, appearance and name.
//! - [`flow`]: the flow between the game's screens, by mode.
//! - [`gameplay`]: the play screen, the frame every panel of the world hangs from.
//! - [`camera`]: mouse look, first person, the wheel's zoom and the arrows' movement.
//! - [`toolbar`]: the toolbar, its windows' buttons and keys, its wings.
//! - [`options`]: the options window: the commands, the character's and the client's options.
//! - [`inventory`]: the inventory window (the pack, the money bag, the paper doll), an opened
//!   container's window, the stack splitter, dragging items between them and into the world.
//! - [`shortcuts`]: the shortcut bar: its slots, sets, keys and dragging; the peace and war buttons.
//! - [`statusbar`]: the status bar's vitals, effects and indicators.
//! - [`time_display`]: a time left kept current in a text element.
//! - [`radar`]: the radar's place line.
//! - [`map`]: the map window: the world map, the closer map, the markers, the regions'
//!   overviews.
//! - [`selection`]: what a click in the world and the selection keys select; the field over the
//!   world.
//! - [`world`]: the world as the panels see it each frame.
//! - [`chat`]: a chat window's log and entry, and the toolbar's chat panel.
//! - [`credits`]: the credits screen.
//! - [`dialog`]: the dialogs (questions, please wait, death's, messages) and the factory that
//!   puts them up one at a time.
//! - [`examination`]: the examination window: what the player examines, by tab.
//! - [`quests`]: the quest journal: the quests under way and finished; [`story_quests`]: the
//!   story's acts.
//! - [`skills`]: the skill window; [`skill_tree`]: its trees of skills; [`craft`]: its crafting page;
//!   [`ingredients`]: the ingredient panel a recipe opens in.
//! - [`trade`]: the trade window; [`shop`]: a shop's sales, the catalog, the player's
//!   consignments and new ones; [`book`]: a book read; [`pet`]: the pet panel; [`story`]: the
//!   story's scenes played over the screen.
//! - [`social`]: the social window (the fellowship, the allegiance, the friends) and the
//!   fellowship's small panel.
//!
//! **Must never** draw (the framework's painter does) or reach the network: a panel says what
//! the player asked for and the shell turns it into actions.

#![forbid(unsafe_code)]

pub mod book;
pub mod camera;
pub mod character_select;
pub mod chat;
pub mod craft;
pub mod creation;
pub mod credits;
pub mod datapatch;
pub mod dialog;
pub mod disconnected;
pub mod examination;
pub mod flow;
pub mod gameplay;
pub mod ingredients;
pub mod intro;
pub mod inventory;
pub mod keyboard;
pub mod map;
pub mod names;
pub mod options;
pub mod pet;
pub mod pregame;
pub mod quests;
pub mod radar;
pub mod selection;
pub mod shop;
pub mod shortcuts;
pub mod skill_tree;
pub mod skills;
pub mod social;
pub mod statusbar;
pub mod story;
pub mod story_quests;
pub mod time_display;
pub mod toolbar;
pub mod trade;
pub mod world;
