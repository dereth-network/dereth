//! What the main chat window's auto-target sweep asks the world.
//!
//! Cut out of `dereth_ui_screens::chat::mainchat`, which keeps the window, the menu and the sweep
//! itself. These two are the sweep's *question* and its *answer* as values: `dereth_client::hud`
//! builds an `AutoTargetWorld` once a second out of the object table and the radar radius, and
//! selection receives the `SpeakableTarget` `AutoTargetWorld::adopted` makes from it.
//! Plain data and one constructor; nothing here draws.

/// The object one call is about, with the three questions the client asks
/// the object system already answered.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpeakableTarget {
    /// The object id. `0` with a non-empty name is not a state the client can be in.
    pub id: u32,
    /// The selected object's wide name; an empty name means nothing is selected.
    pub name: String,
    /// Whether the selected object's item type is talkable.
    pub talkable: bool,
    /// Whether the selected character is squelched for all message types.
    pub squelched: bool,
}

/// The world state needed for automatic chat targeting.
#[derive(Debug, Clone, Default)]
pub struct AutoTargetWorld {
    /// The currently selected object's id.
    pub selected_id: u32,
    /// The local player's object id. The client has two access paths and both name the same object.
    pub player_id: u32,
    /// Whether the selected object's description marks it as talkable.
    pub selected_talkable: bool,
    /// Whether the last speakable object is owned by the player.
    pub owned_by_player: bool,
    /// The last speakable object's container id, `0` for a free-standing object.
    pub container_id: u32,
    /// The ids within the player's radar radius, using the client's inclusive range query.
    pub in_range_of_player: Vec<u32>,
    /// The selected object's wide display name — what automatic targeting asks
    /// the object system for **after** the timed sweep decides to adopt the selection. It is carried
    /// here rather than fetched later so that one sweep is one crossing of this crate's seam.
    pub selected_name: String,
    /// Whether the selected id is squelched for all message types, the third fact automatic
    /// targeting asks.
    pub selected_squelched: bool,
}

impl AutoTargetWorld {
    /// Construct the `SpeakableTarget` passed on when automatic targeting adopts this id.
    #[must_use]
    pub fn adopted(&self, id: u32) -> SpeakableTarget {
        SpeakableTarget {
            id,
            name: self.selected_name.clone(),
            talkable: self.selected_talkable,
            squelched: self.selected_squelched,
        }
    }
}

/// Communication facts projected into either interface's target menu.
#[derive(Debug, Clone, Default)]
pub struct ChatFocusView {
    pub focus: u32,
    pub enabled: [bool; 14],
    pub selectable: [bool; 14],
    pub is_olthoi: bool,
    pub target: Option<SpeakableTarget>,
}

/// Menu reset and enable notices deliberately allow different selected-target states.
#[must_use]
pub fn reset_focus_rows(enabled: [bool; 14], is_olthoi: bool) -> [bool; 14] {
    if !is_olthoi {
        return enabled;
    }
    let mut rows = [false; 14];
    rows[1] = true;
    rows[13] = true;
    rows
}

/// One enable notice: row availability and whether the current destination falls back to Say.
#[must_use]
pub const fn focus_enable_transition(
    previous: bool,
    current: u32,
    focus: u32,
    enabled: bool,
    is_olthoi: bool,
) -> (bool, bool) {
    if is_olthoi && !matches!(focus, 1 | 2 | 13) {
        return (false, false);
    }
    (enabled, previous && !enabled && current == focus)
}
