//! What the game's panels ask of the player's scripts: the interface's functions, each called on
//! the player's instance the way the client's interface glue calls it, and what the player's
//! physics object adds of its own.
//!
//! - **Selecting** an object by clicking it in the world: the interface's selection function,
//!   whose script stores the selection, tells the server, and, while the examination panel is
//!   open, stops examining the old selection and asks to examine the new one; then the player
//!   looks at what it selected.
//! - **Using** the selection: the interface's use function (a double click, the use key).
//! - **Using a shortcut**: the interface's shortcut function with the slot's number (a click on a
//!   toolbar slot, or its key): the scripts read the slot from the player's options and attack
//!   with the skill it holds, use its item, say its text, play its emote or open its recipe.
//! - **Attacking**: the interface's attack-state function, asked to start or to stop.
//! - **The examination panel**: opening it starts the examination with the tabs it shows;
//!   closing it stops the examination.
//! - **Saying** a line typed into the chat box: the say command's handler, called with the line
//!   and its words, as the command shell calls a command's handler.
//! - **The journal**: the player's quests, a copy of the list the player's scripts keep (the
//!   session's start gives it, each quest update changes it), newest given first.
//! - **The toolbar**: the player's shortcuts, slot by slot, copies of the ones in the player's
//!   options.
//!
//! **Looking at the selection.** With the option on, the player looks at each new selection and
//! the client tells the server what it looks at; with it off, the client tells the server once
//! that it looks at nothing, if it looked at something.

use auberean_client_contract::PlayerOption;

/// The interface's functions, by id: each is found in the player's package.
pub mod function {
    /// Take a new selection (an instance id).
    pub const SET_SELECTION: u32 = 0x00B4_0002;
    /// Use the selection.
    pub const USE_SELECTION: u32 = 0x00B4_0006;
    /// Use what a toolbar slot holds (the slot's number).
    pub const USE_SHORTCUT: u32 = 0x00B4_0007;
    /// Every toolbar slot's shortcut, into the list a reference names: 1, or 0 with no options.
    pub const INQ_SHORTCUT_ARRAY: u32 = 0x00B4_0070;
    /// Put a shortcut in a slot (the slot, the shortcut) and tell the server.
    pub const SET_SHORTCUT: u32 = 0x00B4_006F;
    /// Send the windows' saved places and opacities to the server.
    pub const UPLOAD_UI_SETTINGS: u32 = 0x00B4_011D;
    /// Keep the windows' saved places (a table of them).
    pub const SET_UI_SAVE_LOCATIONS: u32 = 0x00B4_007B;
    /// The windows' saved places, into the table a reference names: 1, or 0 with no options.
    pub const INQ_UI_SAVE_LOCATIONS: u32 = 0x00B4_007C;
    /// Keep a window's opacity (the opacity, then the window).
    pub const SET_WINDOW_OPACITY: u32 = 0x00B4_0118;
    /// Set the radar's mask, others' damage range (a float's bits), the toolbar's height.
    pub const SET_RADAR_MASK: u32 = 0x00B4_007F;
    pub const SET_DAMAGE_TEXT_RANGE: u32 = 0x00B4_0079;
    pub const SET_SHORTCUT_HEIGHT: u32 = 0x00B4_0112;
    /// The player's current options profile.
    pub const CURRENT_OPTIONS: u32 = 0x00BD_000C;
    /// Send the damage range, the shortcut height, the radar's mask and the option bits to the
    /// server.
    pub const UPLOAD_GAME_OPTIONS: u32 = 0x00B4_0093;
    /// Whether the player has an item (an instance id): 1 or 0.
    pub const DO_I_HAVE_ITEM: u32 = 0x00B4_0015;
    /// Start (1) or stop (0) attacking.
    pub const REQUEST_ATTACK_STATE_CHANGE: u32 = 0x00B4_0008;
    /// Start examining, with the tabs asked for.
    pub const START_EXAMINATION: u32 = 0x00B4_0067;
    /// Stop examining.
    pub const STOP_EXAMINATION: u32 = 0x00B4_0068;
    /// The player's quest list, into the slot a reference names: 1, or 0 when it has none.
    pub const QUEST_LIST: u32 = 0x00B4_008E;
    /// Give up a quest (its id).
    pub const CANCEL_QUEST: u32 = 0x00B4_011B;
    /// The selection and the one before it (instance ids, 64-bit results).
    pub const GET_SELECTION: u32 = 0x00B4_0003;
    pub const GET_LAST_SELECTION: u32 = 0x00B4_0004;
    /// Whether the player targets what attacks it on its own: 1 or 0.
    pub const GET_AUTO_TARGET: u32 = 0x00B4_0086;
    /// Whether something is selected: 1 or 0.
    pub const HAS_SOMETHING_SELECTED: u32 = 0x00B4_0069;
    /// Whether the player turns to what attacks it on its own: 1 or 0.
    pub const SHOULD_AUTO_TARGET: u32 = 0x00B4_0005;
    /// Get down from a mount.
    pub const DISMOUNT: u32 = 0x00B4_00F1;
    /// Ask to be brought back to life; call off such an offer.
    pub const REQUEST_RESURRECT: u32 = 0x00B4_006B;
    pub const CLEAR_RESURRECTION_REQUEST: u32 = 0x00B4_0100;
    /// Yes to the server's question whether an effect may go on (the effect, the source, the
    /// target).
    pub const SHOULD_APPLY_TOTEM_DONE: u32 = 0x00B4_00D2;
    /// The body began or ended a special attack (its number); its moving on its own.
    pub const SPECIAL_ATTACK_STARTED: u32 = 0x00B4_00C1;
    pub const SPECIAL_ATTACK_ENDED: u32 = 0x00B4_00C2;
    pub const STOP_ATTACKING_IF_AUTO_MOVING: u32 = 0x00B4_00C3;
    /// An object asked by the server to play a behavior with an emote (the emote's number): 1
    /// when its scripts take the emote and the behavior plays, else 0.
    pub const HANDLE_WEENIE_EMOTE: u32 = 0x00B4_00AA;
    /// The tells sent and answered start again from the newest.
    pub const RESET_RETELL_HISTORY_INDEX: u32 = 0x00B4_00AD;
    pub const RESET_REPLY_HISTORY_INDEX: u32 = 0x00B4_00B0;
    /// Whether the player may attack an object on its own (an instance id): 1 or 0.
    pub const CAN_AUTO_ATTACK_TARGET: u32 = 0x00B4_000A;
    /// Whether an object (an instance id) is in the player's fellowship: 1 or 0.
    pub const IS_FELLOWSHIP_MEMBER: u32 = 0x00B4_00A7;
    /// Stop attacking an object (an instance id).
    pub const STOP_ATTACKING: u32 = 0x00B4_00CF;
    /// The radar's map coordinates shown (1) or not (0), and set.
    pub const GET_SHOW_RADAR_COORDS: u32 = 0x00B4_007E;
    pub const SET_SHOW_RADAR_COORDS: u32 = 0x00B4_007D;
    /// Names over heads shown (1) or not (0), and set.
    pub const GET_SHOW_OBJECT_NAMES: u32 = 0x00B4_0114;
    pub const SET_SHOW_OBJECT_NAMES: u32 = 0x00B4_0113;
    /// How an object shows on the radar: its color and its shape, through references; 1 when it
    /// shows. Asked of the object's own instance.
    pub const INQ_RADAR_LOOK: u32 = 0x00B4_00C4;
    /// An object's name for a number of it (a string reference), asked of the object's own
    /// instance.
    pub const GET_POTENTIALLY_PLURALIZED_NAME: u32 = 0x00B4_00C7;
    /// The radar's indoor and outdoor ranges, through references: the radar's control's.
    pub const INQ_RADAR_RANGES: u32 = 0x00A9 << 16 | 0x0001;
    /// The player's commands made known to the command line.
    pub const REGISTER_COMMANDS: u32 = 0x00B4_006A;
    /// The chat options set: a window's filter (the filter, then the window), a kind's font size
    /// and color and popup flag (the value, then the kind), a window's devoted channel (the
    /// kind, then the window); the chat options sent to the server to keep.
    pub const SET_CHAT_WINDOW_FILTER: u32 = 0x00B4_0083;
    pub const SET_CHAT_FONT_SIZE: u32 = 0x00B4_0115;
    pub const SET_CHAT_FONT_COLOR: u32 = 0x00B4_0116;
    pub const DEVOTE_CHAT_WINDOW: u32 = 0x00B4_0117;
    pub const SET_CHAT_POPUP_FLAG: u32 = 0x00B4_0119;
    pub const UPLOAD_CHAT_OPTIONS: u32 = 0x00B4_011C;
    /// The tells sent and the tells answered, a step on or back (a name through a reference).
    pub const INQ_NEXT_RETELL_HISTORY_ITEM: u32 = 0x00B4_00AB;
    pub const INQ_PREV_RETELL_HISTORY_ITEM: u32 = 0x00B4_00AC;
    pub const INQ_NEXT_REPLY_HISTORY_ITEM: u32 = 0x00B4_00AE;
    pub const INQ_PREV_REPLY_HISTORY_ITEM: u32 = 0x00B4_00AF;
}

/// The options panel's yes-or-no options: the option's bit and the function that turns it on
/// (1) or off (0).
pub const OPTION_SETTERS: &[(u32, u32)] = &[
    (0x1, 0x00B4_0073),
    (0x4, 0x00B4_0075),
    (0x8, 0x00B4_0077),
    (0x10, 0x00B4_007D),
    (0x20, 0x00B4_0085),
    (0x40, 0x00B4_0087),
    (0x80, 0x00B4_0089),
    (0x100, 0x00B4_008B),
    (0x200, 0x00B4_0113),
    (0x400, 0x00B4_0108),
    (0x800, 0x00B4_0101),
    (0x1000, 0x00B4_0105),
    (0x2000, 0x00B4_0103),
    (0x4000, 0x00B4_010A),
    (0x8000, 0x00B4_010C),
    (0x1_0000, 0x00B4_010E),
    (0x2_0000, 0x00B4_0110),
];

/// An option the player's scripts keep, as the interface asks and sets it: the function that
/// answers it (1 or 0; others' damage range a float's bits; the shortcut bar's rows a number) and
/// the one that sets it (its argument the same), each called on the player's instance. `None` for
/// the radar's filters, which are bits of the radar's mask ([`RADAR_MASK`]).
#[must_use]
pub const fn option_functions(option: PlayerOption) -> Option<(u32, u32)> {
    use PlayerOption as O;
    Some(match option {
        // D2-00579, D2-01109, D2-00578, D2-01243
        O::AutoTarget => (0x00B4_0086, 0x00B4_0085),
        // D2-00581, D2-01106, D2-00580, D2-01240
        O::TakeAllegianceOffers => (0x00B4_0088, 0x00B4_0087),
        // D2-00583, D2-01107, D2-00582, D2-01241
        O::TakeFellowshipOffers => (0x00B4_008A, 0x00B4_0089),
        // D2-00585, D2-01108, D2-00584, D2-01242
        O::TakeTradeOffers => (0x00B4_008C, 0x00B4_008B),
        // D2-00546, D2-01138, D2-00545, D2-01259
        O::DamageNumbers => (0x00B4_0076, 0x00B4_0075),
        // D2-00548, D2-01139, D2-00547, D2-01260
        O::OthersDamageNumbers => (0x00B4_0078, 0x00B4_0077),
        // D2-00544, D2-01142, D2-00543, D2-01263
        O::PlayerKillerDamage => (0x00B4_0106, 0x00B4_0105),
        // D2-00538, D2-01145, D2-00537, D2-01266
        O::ShortcutNumbers => (0x00B4_0074, 0x00B4_0073),
        // D2-00540, D2-01137, D2-00539, D2-01258
        O::ShortcutLocked => (0x00B4_0102, 0x00B4_0101),
        // D2-00542, D2-01136, D2-00541, D2-01257
        O::ShortcutInstantScroll => (0x00B4_0104, 0x00B4_0103),
        // Kept as its opposite: melt without asking.
        // D2-00554, D2-01112, D2-00553, D2-01244
        O::AskBeforeMelting => (0x00B4_0107, 0x00B4_0108),
        // D2-00556, D2-01124, D2-00555, D2-01252
        O::LatencyGraph => (0x00B4_0109, 0x00B4_010A),
        // D2-00558, D2-01140, D2-00557, D2-01261
        O::FellowsOnMap => (0x00B4_010B, 0x00B4_010C),
        // D2-00560, D2-01143, D2-00559, D2-01264
        O::PetPanel => (0x00B4_010D, 0x00B4_010E),
        // D2-00562, D2-01191, D2-00561, D2-01251
        O::ClassicExamination => (0x00B4_010F, 0x00B4_0110),
        // D2-00566, D2-01144, D2-00565, D2-01265
        O::RadarCoordinates => (0x00B4_007E, 0x00B4_007D),
        // D2-00550, D2-01116, D2-00549, D2-01249
        O::OthersDamageRange => (0x00B4_007A, 0x00B4_0079),
        // D2-00552, D2-01135, D2-00551, D2-01256
        O::ShortcutHeight => (0x00B4_0111, 0x00B4_0112),
        O::RadarMonsters
        | O::RadarPeople
        | O::RadarPlayers
        | O::RadarPortals
        | O::RadarLifestones
        | O::RadarItems => return None,
    })
}

/// The radar's mask: the function that answers it and the one that sets it.
// D2-00570, D2-01133, D2-00569, D2-01253
pub const RADAR_MASK: (u32, u32) = (0x00B4_0080, function::SET_RADAR_MASK);

/// Names over heads: the function that answers it and the one that sets it.
// D2-00568, D2-01141, D2-00567, D2-01262
pub const OBJECT_NAMES: (u32, u32) = (
    function::GET_SHOW_OBJECT_NAMES,
    function::SET_SHOW_OBJECT_NAMES,
);

/// The setter of a yes-or-no option the options window sets (its argument 1 or 0); `None` for the
/// radar's filters (kept in the radar's mask) and the options that are not yes or no.
#[must_use]
pub const fn flag_setter(option: PlayerOption) -> Option<u32> {
    match option {
        PlayerOption::OthersDamageRange | PlayerOption::ShortcutHeight => None,
        o => match option_functions(o) {
            Some((_, set)) => Some(set),
            None => None,
        },
    }
}

/// A radar filter's bit in the radar's mask.
#[must_use]
pub const fn radar_bit(option: PlayerOption) -> Option<u32> {
    use PlayerOption as O;
    Some(match option {
        O::RadarMonsters => 0x1,
        O::RadarPeople => 0x2,
        O::RadarPlayers => 0x4,
        O::RadarPortals => 0x8,
        O::RadarLifestones => 0x10,
        O::RadarItems => 0x20,
        _ => return None,
    })
}

/// The tabs the examination panel asks for.
pub const EXAMINATION_TABS: u32 = 0x3FF;
