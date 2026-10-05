//! The process-owned landscape presets an admin environment command writes.
//!
//! It lives in this crate because `dereth_client_runtime::present::Scene`'s
//! `set_environment_override_state` names `EnvironmentOverrideState`, and the trait cannot sit
//! below the type it names. Nothing here draws, holds a device or makes an OS call — it is a
//! seven-field record behind a shared handle and the table that fills it.
//!
//! Visibility follows from that:
//!
//! * `EnvironmentOverride` and its seven fields are `pub`, because `WorldScene`'s lighting and
//!   fog passes read them through `EnvironmentOverrideState::snapshot` from the other crate;
//! * `snapshot` and `EnvironmentOverrideState::advance` are `pub`, for the same reason;
//! * neither method is gated on a device feature: this crate has none, and the type itself is
//!   ungated.
//!
//! `dereth_client_runtime::environment::EnvironmentOverrideState` resolves to it through a `pub use`.

use std::cell::RefCell;
use std::rc::Rc;

/// The seven landscape presets written by
/// the admin environment command.
///
/// Lighting and fog share one transition state. Each reads and advances it at
/// their own native update sites; keeping it here instead of on either consumer preserves that
/// deliberately uneven cadence.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnvironmentOverride {
    pub enabled: bool,
    pub transition: f32,
    pub ambient_level: f32,
    pub ambient_color: u32,
    pub fog_min: f32,
    pub fog_max: f32,
    pub fog_color: u32,
}

/// The process-owned native override globals. `WorldScene` instances receive clones of this
/// handle, so rebuilding the landscape neither clears the selected preset nor restarts its
/// already-running transition.
#[derive(Debug, Clone, Default)]
pub struct EnvironmentOverrideState(Rc<RefCell<EnvironmentOverride>>);

impl EnvironmentOverrideState {
    /// Apply one accepted visual option and return its radar-blank value. Unknown and sound
    /// options leave the process state untouched.
    pub fn apply_option(&self, option: i32) -> Option<bool> {
        let next = admin_environs_override(option)?;
        *self.0.borrow_mut() = next;
        admin_environs_radar_blank(option)
    }

    /// The current preset, as the lighting and fog passes read it.
    #[must_use]
    pub fn snapshot(&self) -> EnvironmentOverride {
        *self.0.borrow()
    }

    /// Advance the one shared transition by a frame's worth.
    pub fn advance(&self) {
        let mut state = self.0.borrow_mut();
        if state.transition < 1.0 {
            state.transition += 0.04;
        }
    }
}

impl Default for EnvironmentOverride {
    fn default() -> Self {
        Self {
            enabled: false,
            transition: 0.0,
            ambient_level: 0.0,
            ambient_color: 0,
            fog_min: 0.0,
            fog_max: 0.0,
            fog_color: 0,
        }
    }
}

/// The accepted visual values and result. Unknown values and
/// the disjoint sound range do not touch either state.
#[must_use]
pub const fn admin_environs_radar_blank(option: i32) -> Option<bool> {
    match option {
        0..=5 | 9999 => Some(false),
        6 => Some(true),
        _ => None,
    }
}

fn admin_environs_override(option: i32) -> Option<EnvironmentOverride> {
    let (ambient_level, ambient_color, fog_color, fog_max) = match option {
        0 => return Some(EnvironmentOverride::default()),
        1 => (0.4, 0x6496_0000, 0x6496_0000, 50.0),
        2 => (0.3, 0x6432_0096, 0x6432_0096, 50.0),
        3 => (0.4, 0x6464_6464, 0x6464_6464, 30.0),
        4 => (0.3, 0x641E_6400, 0x641E_6400, 50.0),
        5 | 6 => (0.8, 0x6496_9696, 0x6400_0000, 40.0),
        9999 => (0.4, 0x3264_6464, 0x6464_6464, 30.0),
        _ => return None,
    };
    Some(EnvironmentOverride {
        enabled: true,
        transition: 0.0,
        ambient_level,
        ambient_color,
        fog_min: 0.0,
        fog_max,
        fog_color,
    })
}
