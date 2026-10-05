//! The four portal-storm handlers — `0x02C9`…`0x02CC`.
//!
//! A portal storm is the server evicting players from an over-crowded landblock. The client is
//! told four things about one: it is brewing, it is imminent, it has taken you, it has passed.
//! The net-blob router sends each to its own portal-storm dispatch,
//! and each of those to its own portal-storm handler.
//!
//! # The four
//!
//! | opcode | handler | body | what it does |
//! |---:|---|---|---|
//! | `0x02C9` brewing | brewing | one `float` | notice line, `play_script(0x73, 0.0)`, level = the float |
//! | `0x02CA` imminent | imminent | one `float` | notice line, `play_script(0x73, 1.0)`, level = the float |
//! | `0x02CB` struck | struck | none | player-portal-stormed notice, level = 0, **chat** line |
//! | `0x02CC` subsided | subsided | none | notice line, level = 0 |
//!
//! Three readings a plainer transcription loses, and each is a *difference between the four*:
//!
//! 1. **Only `0x02CB` writes to the chat log.** The other three use
//!    display local feedback on channel `0x1A`, the same one
//!    `dereth_client_model::scroll::LOCAL_ERROR_TYPE` names, which the main chat window's default filter
//!    *clears*. `0x02CB` calls the scroll's add-text entry point with chat type `0`
//!    instead, i.e. the ordinary chat channel. So a player with default settings sees the
//!    warnings float over the viewport and the "you have been moved" line in the log.
//! 2. **`0x02CB` raises the player-portal-stormed notice and none of the others does.**
//!    That is the notice a teleport listener would key on.
//! 3. **The level the indicator reads is the float off the wire, not a constant.** Brewing and
//!    imminent both pass their own parameter through to the level notice
//!    (the incoming `float` off the wire);
//!    struck and subsided push a literal `0`.
//!
//! # What is not modelled, and why it is named rather than faked
//!
//! Play physics script `0x73` with the computed modifier on the **player's own** physics object
//! is queued here rather than played, because `dereth_client_model` has no scene: the request is drained by
//! `dereth_scene::world_scene::WorldScene` beside `ObjectStream`'s own script events, which is the one
//! place in this build that holds a physics object.

use crate::world::World;

/// The portal-storm play script — the script both warning handlers push.
pub const PS_PORTAL_STORM: u32 = 0x73;

/// The `float` `play_script` is given for a **brewing** storm (a pushed `0`).
pub const BREWING_INTENSITY: f32 = 0.0;
/// …and for an **imminent** one (a pushed `1.0f`).
pub const IMMINENT_INTENSITY: f32 = 1.0;

/// A wide string literal. `0x02C9`'s.
pub const BREWING: &str = "This area is getting too crowded - a Portal Storm is brewing.";
/// `0x02CA`'s.
pub const IMMINENT: &str = "A Portal Storm is imminent - leave this crowded area!";
/// `0x02CB`'s — note the trailing newline, which the other three do not have.
pub const STRUCK: &str = "The Portal Storm has teleported you away from the crowded area!\n";
/// `0x02CC`'s.
pub const SUBSIDED: &str = "The Portal Storm has subsided";

impl World {
    /// The brewing-storm handler — **`0x02C9`**.
    ///
    /// ```text
    ///   StringInfo(L"This area is getting too crowded - a Portal Storm is brewing.")
    ///   display local feedback on channel 0x1A
    ///   player = current player id when the global player state exists, otherwise 0
    ///   look up the player's physics object in the object-maint system
    ///   play physics script 73h with modifier 0    ; only if non-null
    ///   raise the portal-storm level notice with the float
    /// ```
    ///
    /// The level notice is raised **last** and **unconditionally** — after the script, and whether
    /// or not the player had a physics object.
    pub fn portal_storm_brewing(&mut self, level: f32) {
        self.scroll
            .on_display_string_info(crate::scroll::LOCAL_ERROR_TYPE, BREWING);
        self.pending_portal_storm_scripts.push(BREWING_INTENSITY);
        self.portal_storm_level = level;
    }

    /// The imminent-storm handler — **`0x02CA`**. The brewing handler with a different
    /// literal and a modifier of `1.0` in place of `0`.
    pub fn portal_storm_imminent(&mut self, level: f32) {
        self.scroll
            .on_display_string_info(crate::scroll::LOCAL_ERROR_TYPE, IMMINENT);
        self.pending_portal_storm_scripts.push(IMMINENT_INTENSITY);
        self.portal_storm_level = level;
    }

    /// The struck handler — **`0x02CB`**, the one that has already happened.
    ///
    /// ```text
    ///   construct "The Portal Storm has teleported you away from the crowded area!\n"
    ///   call the player-portal-stormed notice
    ///   raise the portal-storm level notice with 0
    ///   call the scroll's add-text entry point (text, 0, true, 0)
    /// ```
    ///
    /// **No display-string notice, and no `play_script`.** This is the only one of the
    /// four that reaches the chat log, and it is on chat type **0** rather than `0x1A`.
    ///
    /// Returns whether the notice would have a listener — always `true` here, because
    /// the player-portal-stormed notice's fan-out is a walk this build has no bus for
    /// and the count is the observable instead.
    pub fn portal_storm_struck(&mut self) {
        self.portal_storm_level = 0.0;
        self.portal_storms_struck += 1;
        self.scroll.add_feedback_to_scroll(
            STRUCK,
            crate::chat::text_type::DEFAULT,
            true,
            0,
            dereth_client_contract::feedback::Feedback::LOCAL,
        );
    }

    /// Behavior: **`0x02CC`**. The notice line and
    /// a portal-storm-level notice of 0; no script and nothing in the chat log.
    pub fn portal_storm_subsided(&mut self) {
        self.scroll
            .on_display_string_info(crate::scroll::LOCAL_ERROR_TYPE, SUBSIDED);
        self.portal_storm_level = 0.0;
    }

    /// The two warnings request portal-storm physics script `0x73`, with their computed modifier,
    /// on the player's own physics object and in arrival order.
    ///
    /// `dereth_client_model` has no scene, so the call is queued and `dereth_scene::world_scene::WorldScene` drains
    /// it beside `ObjectStream`'s own script events — the same split, and for the same reason,
    /// that `crate::objects::ScriptEvent` already has.
    pub fn take_portal_storm_scripts(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.pending_portal_storm_scripts)
    }
}
