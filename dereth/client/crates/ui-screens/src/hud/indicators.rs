//! The indicator strip and its six lamp element types.
//!
//! Each lamp derives from `Button` and drives its appearance by setting its element state `n`;
//! **state `0x0D` is the "nothing to show" state throughout**.

use dereth_ui::ElementType;

use crate::element_types::ty;
use crate::view::GameView;

/// The state every lamp shows when it has nothing to say.
pub const STATE_NOTHING: u32 = 0x0D;

/// The burden indicator's three load states.
pub mod burden {
    /// `load < 1.0`, and also "no value".
    pub const UNDER: u32 = 0x0E;
    /// `1.0 ≤ load < 2.0`.
    pub const OVER: u32 = 0x0F;
    /// `load ≥ 2.0`.
    pub const WAY_OVER: u32 = 0x10;
}

/// `EffectsIndicator`'s attribute `0x1000000C`, which selects which enchantments it
/// counts.
pub mod effects_kind {
    /// Both helpful and harmful.
    pub const BOTH: u32 = 0;
    /// Helpful only.
    pub const HELPFUL: u32 = 1;
    /// Harmful only.
    pub const HARMFUL: u32 = 2;
    /// The attribute the kind is read from.
    pub const ATTRIBUTE: u32 = 0x1000_000C;
}

/// One indicator lamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Indicator {
    /// A local behavior label used in diagnostics.
    pub class: &'static str,
    pub ty: ElementType,
}

const fn ind(class: &'static str, ty: ElementType) -> Indicator {
    Indicator { class, ty }
}

/// The six lamps, in registration order.
pub const INDICATORS: [Indicator; 6] = [
    ind("BurdenIndicator", ty::BURDEN_INDICATOR),
    ind("EffectsIndicator", ty::EFFECTS_INDICATOR),
    // "— (global message 3)": the link lamp flashes from the frame tick.
    ind("LinkStatusIndicator", ty::LINK_STATUS_INDICATOR),
    ind("MiniGameIndicator", ty::MINI_GAME_INDICATOR),
    ind("PortalStormIndicator", ty::PORTAL_STORM_INDICATOR),
    ind("VitaeIndicator", ty::VITAE_INDICATOR),
];

/// The burden indicator's update.
///
/// "reads the player's load: no value → state `0x0E`; `load < 1.0` → `0x0E`;
/// `1.0 ≤ load < 2.0` → `0x0F`; `load ≥ 2.0` → `0x10`."
///
/// Note that "no value" and "under-burdened" share a state, so this lamp never shows
/// [`STATE_NOTHING`].
///
/// The bands are [`dereth_rules::burden::load_band`]'s, so this lamp and the encumbrance
/// messages cannot disagree about where `1.0` and `2.0` fall.
#[must_use]
pub fn burden_state(load: Option<f32>) -> u32 {
    use dereth_rules::burden::{load_band, LoadBand};
    match load.map(load_band) {
        None | Some(LoadBand::Normal) => burden::UNDER,
        Some(LoadBand::Encumbered) => burden::OVER,
        Some(LoadBand::OverBurdened) => burden::WAY_OVER,
    }
}

/// The effects indicator's update.
///
/// "sums the helpful / harmful enchantment counts; zero (or no registry) → state
/// `0x0D`, otherwise `0x01`."
#[must_use]
pub fn effects_state(kind: u32, helpful: u32, harmful: u32) -> u32 {
    let n = match kind {
        effects_kind::HELPFUL => helpful,
        effects_kind::HARMFUL => harmful,
        _ => helpful + harmful,
    };
    if n == 0 {
        STATE_NOTHING
    } else {
        1
    }
}

/// The portal-storm indicator's update.
///
/// "level > 0 → state 1, remember the last portal-storm warning time and subscribe to global message
/// 3 (to flash); level ≤ 0 → state `0x0D` and unsubscribe."
///
/// Returns `(state, subscribe_to_tick)`. The subscription half matters: there is no per-frame
/// walk, so a lamp that forgets to register simply never flashes, silently.
///
/// **`level` is `f32`**: the comparison in the client is a floating-point test against 0.0, the
/// level on the wire is the `0x02C9`/`0x02CA` warnings' `extent`, and truncating a fractional
/// extent would read as no storm.
#[must_use]
pub fn portal_storm_state(level: f32) -> (u32, bool) {
    if level > 0.0 {
        (1, true)
    } else {
        (STATE_NOTHING, false)
    }
}

/// The vitae indicator's update.
///
/// "asks for the player's vitae; if present and its multiplier < 1.0 → state 1, else
/// `0x0D`."
#[must_use]
pub fn vitae_state(vitae: Option<f32>) -> u32 {
    match vitae {
        Some(m) if m < 1.0 => 1,
        _ => STATE_NOTHING,
    }
}

/// The minigame indicator's begin-game notice and
/// The end game notice.
///
/// The two handlers contain no game-state branch: the former writes media state 1 and the latter
/// writes `0x0D`. Join, start and turn changes therefore leave the active image alone.
#[must_use]
pub const fn minigame_state(visible: bool) -> u32 {
    if visible {
        1
    } else {
        STATE_NOTHING
    }
}

/// Every lamp's state for one `GameView`, in [`INDICATORS`] order. The link-status lamp is driven
/// by timing state this seam does not carry, so it is reported as `None`.
#[must_use]
pub fn all_states(view: &dyn GameView, effects_kind: u32) -> [Option<u32>; 6] {
    let (helpful, harmful) = view.enchantment_counts();
    [
        Some(burden_state(view.load())),
        Some(effects_state(effects_kind, helpful, harmful)),
        None,
        Some(minigame_state(
            view.minigame().is_some_and(|game| game.visible),
        )),
        Some(portal_storm_state(view.portal_storm_level()).0),
        Some(vitae_state(view.vitae())),
    ]
}

/// The indicator strip's own behaviour: `IndicatorStrip` is "a plain container: its only behaviour
/// is that a click (message 1) on child `0x100000FA` calls
/// raises the end-character-session notice with argument 1 — that is the **log-out button**".
pub const LOGOUT_BUTTON: dereth_ui::ElementId = dereth_ui::ElementId(0x1000_00FA);

/// `LinkStatusIndicator` (`0x10000003`) — the connection lamp.
///
/// This is the one lamp that needs its post-init to be visible at all: the shipped
/// `classic_gameplay` gives element `0x100000F8` **no base image at all**. Its
/// four pictures live only in element states `0x11`…`0x14`, and the only thing that ever puts it
/// into one of them is its own post-init: after the base post-init it registers for global
/// message 3 and, if its link state is not already 1, sets it to 1 and shows element state `0x11`.
///
/// Registered as a `PlainElement`, no post-init would run and the lamp would stay in state 0,
/// where the layout puts nothing. The five other lamps carry a base image and draw regardless.
///
/// **Setting the media state is setting the element state** — the same call a `Meter` makes.
pub mod link_status {
    /// The client's four link-state values, in the order its switch takes them.
    pub mod state {
        /// Heard from the server within [`super::GOOD_SECONDS`].
        pub const GOOD: u32 = 1;
        /// Within [`super::MODERATE_SECONDS`].
        pub const MODERATE: u32 = 2;
        /// Within [`super::POOR_SECONDS`]; this is the one that flashes.
        pub const POOR: u32 = 3;
        /// Longer than that, **or not connected at all**.
        pub const LOST: u32 = 4;
    }

    /// The element state each link state shows — the client's four
    /// set-state calls.
    pub mod media {
        /// Link state 1.
        pub const GOOD: u32 = 0x11;
        /// Link state 2, and one half of the state-3 flash.
        pub const MODERATE: u32 = 0x12;
        /// Link state 3, and the other half of the flash.
        pub const POOR: u32 = 0x13;
        /// Link state 4.
        pub const LOST: u32 = 0x14;
    }

    /// The client's three thresholds, in seconds since the last datagram.
    pub const GOOD_SECONDS: f64 = 5.0;
    /// See [`GOOD_SECONDS`].
    pub const MODERATE_SECONDS: f64 = 20.0;
    /// See [`GOOD_SECONDS`].
    pub const POOR_SECONDS: f64 = 40.0;
    /// The per-frame step re-evaluates the link state once more than 4 s have passed since the last update.
    pub const UPDATE_SECONDS: f64 = 4.0;
    /// The per-frame step's flash period while the link state is 3.
    pub const FLASH_SECONDS: f64 = 0.75;

    /// The link-status indicator's state update.
    ///
    /// With `d` the seconds since the last datagram: connected and `d <= 5` is 1, else connected and
    /// `d <= 20` is 2, else connected and `d <= 40` is 3, otherwise 4.
    ///
    /// Note the shape: **every** arm re-tests `connected`, so a disconnected client falls all the
    /// way through to 4 whatever the elapsed time is. That is why the argument here is an
    /// `Option` — `None` is "not connected" — rather than a time with a separate flag.
    #[must_use]
    pub fn link_state(status: Option<f64>) -> u32 {
        let Some(d) = status else { return state::LOST };
        if d <= GOOD_SECONDS {
            state::GOOD
        } else if d <= MODERATE_SECONDS {
            state::MODERATE
        } else if d <= POOR_SECONDS {
            state::POOR
        } else {
            state::LOST
        }
    }

    /// The element state one link state shows.
    #[must_use]
    pub fn media_state(link_state: u32) -> u32 {
        match link_state {
            state::GOOD => media::GOOD,
            state::MODERATE => media::MODERATE,
            state::POOR => media::POOR,
            _ => media::LOST,
        }
    }
}

/// The live `LinkStatusIndicator`, as a `dereth_ui` widget.
///
/// It is a real element behaviour rather than something the screen drives, because that is what it
/// is in the client: it registers for global message 3 in its own post-init and nothing else ever
/// calls it. The one thing it cannot reach on its own is the network connection, so
/// [`LinkStatusIndicator::use_time`] takes the connection status as an argument and
/// [`crate::screens::gameplay::GamePlayScreen::update_indicators`] supplies it from the
/// [`GameView`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinkStatusIndicator {
    /// The link state, 0 before post-init.
    pub link_state: u32,
    /// The time of the last link-state update.
    pub last_update: f64,
    /// The time of the last flash.
    pub last_flash: f64,
    /// The element state last asked for, so the flash can read it back without the tree.
    pub media_state: u32,
}

impl Default for LinkStatusIndicator {
    /// The constructor: link state 0, both timestamps 0.
    fn default() -> Self {
        Self {
            link_state: 0,
            last_update: 0.0,
            last_flash: 0.0,
            media_state: 0,
        }
    }
}

impl LinkStatusIndicator {
    /// Register for the tick and take state 1 immediately.
    ///
    /// Returns the element state to show. **This is the whole of the missing lamp**: without it
    /// element `0x100000F8` never leaves state 0, and state 0 has no media.
    pub fn post_init(&mut self) -> u32 {
        if self.link_state != link_status::state::GOOD {
            self.link_state = link_status::state::GOOD;
            self.media_state = link_status::media::GOOD;
        }
        self.media_state
    }

    /// A no-op unless the state actually changed, which is what stops
    /// the flash from being reset every four seconds.
    fn set_link_state(&mut self, s: u32, now: f64) {
        if self.link_state == s {
            return;
        }
        self.link_state = s;
        self.media_state = link_status::media_state(s);
        if s == link_status::state::POOR {
            self.last_flash = now;
        }
    }

    /// The per-frame step, called from the global-message handler on global message 3.
    ///
    /// Returns the element state to show now.
    pub fn use_time(&mut self, now: f64, status: Option<f64>) -> u32 {
        if now - self.last_update > link_status::UPDATE_SECONDS {
            self.set_link_state(link_status::link_state(status), now);
            self.last_update = now;
        }
        // "Link state 3 and 0.75 s since the last flash": toggle between `0x12` and `0x13`.
        // The client reads the *element's* current state to decide which way to toggle; this reads the
        // state it last asked for, which is the same value unless something else moved the element.
        if self.link_state == link_status::state::POOR
            && now - self.last_flash >= link_status::FLASH_SECONDS
        {
            self.media_state = if self.media_state == link_status::media::MODERATE {
                link_status::media::POOR
            } else {
                link_status::media::MODERATE
            };
            self.last_flash = now;
        }
        self.media_state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the client's three thresholds, at each boundary, plus the
    /// "not connected falls all the way through" shape its repeated `connected` test gives it.
    #[test]
    fn the_link_lamp_reads_seconds_since_the_last_datagram_and_not_a_round_trip() {
        use link_status::state;
        assert_eq!(link_status::link_state(Some(0.0)), state::GOOD);
        assert_eq!(
            link_status::link_state(Some(5.0)),
            state::GOOD,
            "5.0 is still good"
        );
        assert_eq!(link_status::link_state(Some(5.001)), state::MODERATE);
        assert_eq!(link_status::link_state(Some(20.0)), state::MODERATE);
        assert_eq!(link_status::link_state(Some(20.001)), state::POOR);
        assert_eq!(link_status::link_state(Some(40.0)), state::POOR);
        assert_eq!(link_status::link_state(Some(40.001)), state::LOST);
        // Not connected is state 4 whatever the elapsed time is — every arm re-tests the flag.
        assert_eq!(link_status::link_state(None), state::LOST);
        // …and the four element states.
        assert_eq!(
            link_status::media_state(state::GOOD),
            link_status::media::GOOD
        );
        assert_eq!(
            link_status::media_state(state::MODERATE),
            link_status::media::MODERATE
        );
        assert_eq!(
            link_status::media_state(state::POOR),
            link_status::media::POOR
        );
        assert_eq!(
            link_status::media_state(state::LOST),
            link_status::media::LOST
        );
        assert_eq!(
            (
                link_status::media::GOOD,
                link_status::media::MODERATE,
                link_status::media::POOR,
                link_status::media::LOST
            ),
            (0x11, 0x12, 0x13, 0x14)
        );
    }

    /// The link lamp takes its good state the moment it is created.
    #[test]
    fn the_link_lamp_takes_its_good_state_the_moment_it_is_created() {
        let mut l = LinkStatusIndicator::default();
        assert_eq!(l.link_state, 0, "the constructor leaves it at 0");
        assert_eq!(l.post_init(), link_status::media::GOOD);
        assert_eq!(l.link_state, link_status::state::GOOD);
    }

    /// Oracle: — the 4-second update cadence and the 0.75-second flash.
    #[test]
    fn the_link_lamp_updates_every_four_seconds_and_flashes_only_in_state_three() {
        let mut l = LinkStatusIndicator::default();
        l.post_init();
        // Inside four seconds nothing is re-read, however bad the link has become.
        assert_eq!(l.use_time(1.0, None), link_status::media::GOOD);
        assert_eq!(l.use_time(4.0, None), link_status::media::GOOD);
        // Past it, the state is taken.
        assert_eq!(l.use_time(4.5, None), link_status::media::LOST);
        // A poor link flashes between 0x12 and 0x13, and only after 0.75 s each time.
        let mut l = LinkStatusIndicator::default();
        l.post_init();
        let poor = Some(link_status::POOR_SECONDS);
        assert_eq!(l.use_time(5.0, poor), link_status::media::POOR);
        assert_eq!(
            l.use_time(5.5, poor),
            link_status::media::POOR,
            "too soon to flash"
        );
        assert_eq!(l.use_time(5.75, poor), link_status::media::MODERATE);
        assert_eq!(l.use_time(6.5, poor), link_status::media::POOR);
        assert_eq!(l.use_time(7.25, poor), link_status::media::MODERATE);
        // A good link does not flash at all, however long it runs.
        let mut l = LinkStatusIndicator::default();
        l.post_init();
        let mut t = 0.0;
        for _ in 0..100 {
            t += 0.5;
            assert_eq!(l.use_time(t, Some(0.0)), link_status::media::GOOD);
        }
    }

    /// Oracle: the recovered HUD behavior's six-row lamp table — six consecutive element types,
    /// and the logout button's own element id.
    #[test]
    fn the_six_lamps_are_six_consecutive_element_types_and_the_logout_button_has_its_own_id() {
        assert_eq!(INDICATORS.len(), 6);
        for (i, ind) in INDICATORS.iter().enumerate() {
            let i = u32::try_from(i).unwrap();
            assert_eq!(ind.ty, ElementType(0x1000_0001 + i), "{}", ind.class);
        }
        assert_eq!(LOGOUT_BUTTON, dereth_ui::ElementId(0x1000_00FA));
    }

    /// Oracle: — the three burden thresholds, tested at each boundary.
    #[test]
    fn the_burden_lamp_switches_at_one_and_two() {
        assert_eq!(burden_state(None), burden::UNDER);
        assert_eq!(burden_state(Some(0.0)), burden::UNDER);
        assert_eq!(burden_state(Some(0.999)), burden::UNDER);
        assert_eq!(
            burden_state(Some(1.0)),
            burden::OVER,
            "1.0 is over, not under"
        );
        assert_eq!(burden_state(Some(1.999)), burden::OVER);
        assert_eq!(burden_state(Some(2.0)), burden::WAY_OVER);
        assert_eq!(burden_state(Some(50.0)), burden::WAY_OVER);
        // The burden lamp is the one that never shows the "nothing" state.
        for l in [None, Some(0.0), Some(1.5), Some(9.0)] {
            assert_ne!(burden_state(l), STATE_NOTHING);
        }
    }

    /// The effects lamp counts the enchantment kind selected by attribute `0x1000000C`.
    #[test]
    fn the_effects_lamp_counts_the_kind_its_attribute_selects() {
        assert_eq!(effects_state(effects_kind::BOTH, 0, 0), STATE_NOTHING);
        assert_eq!(effects_state(effects_kind::BOTH, 1, 0), 1);
        assert_eq!(effects_state(effects_kind::BOTH, 0, 1), 1);
        assert_eq!(
            effects_state(effects_kind::HELPFUL, 0, 5),
            STATE_NOTHING,
            "harmful ignored"
        );
        assert_eq!(effects_state(effects_kind::HELPFUL, 5, 0), 1);
        assert_eq!(
            effects_state(effects_kind::HARMFUL, 5, 0),
            STATE_NOTHING,
            "helpful ignored"
        );
        assert_eq!(effects_state(effects_kind::HARMFUL, 0, 5), 1);
        assert_eq!(effects_kind::ATTRIBUTE, 0x1000_000C);
    }

    /// Oracle: — the level test *and* the message-3 subscription it controls.
    #[test]
    fn the_portal_storm_lamp_subscribes_to_the_tick_only_while_a_storm_is_on() {
        assert_eq!(portal_storm_state(0.0), (STATE_NOTHING, false));
        assert_eq!(portal_storm_state(-1.0), (STATE_NOTHING, false));
        assert_eq!(portal_storm_state(1.0), (1, true));
        assert_eq!(portal_storm_state(9.0), (1, true));
        assert_eq!(portal_storm_state(0.25), (1, true));
        assert_eq!(portal_storm_state(-0.25), (STATE_NOTHING, false));
    }

    /// Oracle: — "if present **and** its multiplier < 1.0".
    #[test]
    fn the_vitae_lamp_lights_only_for_a_real_penalty() {
        assert_eq!(vitae_state(None), STATE_NOTHING);
        assert_eq!(
            vitae_state(Some(1.0)),
            STATE_NOTHING,
            "a multiplier of 1.0 is no penalty"
        );
        assert_eq!(vitae_state(Some(0.95)), 1);
    }

    /// Oracle: the same five non-timed lamp functions, driven through the `GameView` seam so the
    /// wiring is exercised too.
    #[test]
    fn the_lamps_read_the_game_view_and_the_timed_one_reports_nothing() {
        #[derive(Debug)]
        struct V;
        impl GameView for V {
            fn load(&self) -> Option<f32> {
                Some(1.5)
            }
            fn enchantment_counts(&self) -> (u32, u32) {
                (2, 0)
            }
            fn portal_storm_level(&self) -> f32 {
                3.0
            }
            fn vitae(&self) -> Option<f32> {
                Some(0.9)
            }
            fn minigame(&self) -> Option<crate::view::MiniGameView> {
                Some(crate::view::MiniGameView {
                    visible: true,
                    ..Default::default()
                })
            }
        }
        let s = all_states(&V, effects_kind::BOTH);
        assert_eq!(s[0], Some(burden::OVER));
        assert_eq!(s[1], Some(1));
        assert_eq!(s[2], None, "the link lamp's state is not on this seam");
        assert_eq!(
            s[3],
            Some(1),
            "BeginGame's visible window lights the mini-game lamp"
        );
        assert_eq!(s[4], Some(1));
        assert_eq!(s[5], Some(1));

        // An empty world leaves every modelled lamp dark except burden, which shows "under".
        let e = all_states(&crate::view::EmptyGameView, effects_kind::BOTH);
        assert_eq!(e[0], Some(burden::UNDER));
        assert_eq!(e[1], Some(STATE_NOTHING));
        assert_eq!(e[3], Some(STATE_NOTHING));
        assert_eq!(e[4], Some(STATE_NOTHING));
        assert_eq!(e[5], Some(STATE_NOTHING));
    }
}
