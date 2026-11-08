//! What a body asks of its animation (its behaviors and modes, its effect scripts), and the stamps
//! that order what the server sends it.
//!
//! The body's animation is the host's ([`Notice::Animate`]): stopping a layer, an effect script,
//! the behaviors a move cancels or the behaviors the server asked for. Most wake the body's
//! animation first.
//!
//! The server stamps each kind of update it sends a body; an update is newer when its stamp is
//! ahead of the one kept by less than half the stamps' range (they count round through 65536),
//! and the newer stamp is then kept. The fourth is the stamp of the last move the server ordered.

use super::{Body, Notice};

/// A body's update stamps: its position's, its teleports', and the last move the server ordered.
pub const POSITION_STAMP: usize = 0;
pub const TELEPORT_STAMP: usize = 1;
pub const MOVE_TO_STAMP: usize = 3;

/// A request for the body's animation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Animate {
    /// Play an effect script.
    FxScript(u32),
    /// Stop what holds a layer.
    EndLayer(u32),
    /// Stop every behavior a move cancels.
    StopMoveToCancels,
    /// Stop every behavior the server asked for.
    StopServerBehaviors,
    /// Move a slider to `value` over `time` seconds.
    Slider {
        context: u32,
        slider: u32,
        value: f32,
        time: f32,
    },
    /// Go to the game's default mode (its own character template's).
    DefaultMode,
    /// Place the parts in the setup's placement `0`.
    PlacementFrame(u32),
    /// Go to mode `mode` (for `context`).
    Mode { context: u32, mode: u32 },
    /// Finish at once the change of mode under way.
    SnapMode,
    /// Go to element `element` (the air, the earth, the water...).
    Element(u32),
    /// Play effect `fx` at `intensity`.
    Fx { fx: u32, intensity: f32 },
}

impl Body {
    /// Everything the body asked since last taken, in one list: its notices, then its mode
    /// changes and the effects it played as animation requests.
    pub fn take_requests(&mut self) -> Vec<Notice> {
        let mut out = std::mem::take(&mut self.notices);
        out.extend(
            self.modes
                .drain(..)
                .map(|(context, mode)| Notice::Animate(Animate::Mode { context, mode })),
        );
        out.extend(
            self.effects
                .drain(..)
                .map(|(fx, intensity)| Notice::Animate(Animate::Fx { fx, intensity })),
        );
        out
    }
}

impl Body {
    fn animate(&mut self, a: Animate) {
        self.notices.push(Notice::Animate(a));
    }

    /// Plays an effect script; the body's animation wakes.
    // D2-05881
    pub fn do_fx_script(&mut self, script: u32) -> bool {
        self.set_animation_active(true);
        self.animate(Animate::FxScript(script));
        true
    }

    /// The server plays an effect script on the body.
    // D2-05845
    pub fn receive_do_fx_script(&mut self, script: u32) -> bool {
        self.do_fx_script(script)
    }

    /// Stops what holds a layer; the body's animation wakes.
    // D2-05890
    pub fn stop_layer(&mut self, layer: u32) {
        self.set_animation_active(true);
        self.animate(Animate::EndLayer(layer));
    }

    // D2-05849
    pub fn receive_stop_layer(&mut self, layer: u32) {
        self.stop_layer(layer);
    }

    /// Stops every behavior a move cancels.
    // D2-05888
    pub fn stop_move_to_cancels_behaviors(&mut self) {
        self.animate(Animate::StopMoveToCancels);
    }

    // D2-05850
    pub fn receive_stop_move_to_cancels_behaviors(&mut self) {
        self.stop_move_to_cancels_behaviors();
    }

    /// Stops every behavior the server asked for.
    // D2-05889
    pub fn stop_all_server_behaviors(&mut self) {
        self.animate(Animate::StopServerBehaviors);
    }

    // D2-05851
    pub fn receive_stop_all_server_behaviors(&mut self) {
        self.stop_all_server_behaviors();
    }

    /// Whether the body shows a behavior.
    // D2-05896
    #[must_use]
    pub fn is_doing_behavior(&self, behavior: u32) -> bool {
        self.behaviors.contains(&behavior)
    }

    /// The stamp kept for update kind `kind` (0 to 3).
    #[must_use]
    pub fn update_time(&self, kind: usize) -> u16 {
        self.update_times[kind]
    }

    // D2-06333
    pub fn inc_update_time(&mut self, kind: usize) {
        self.update_times[kind] = self.update_times[kind].wrapping_add(1);
    }

    /// Whether `stamp` is newer than the one kept for update kind `kind`; a newer one is kept.
    // D2-06190, D2-06332
    pub fn is_newer_than_update(&mut self, kind: usize, stamp: u16) -> bool {
        let kept = self.update_times[kind];
        if stamp == kept {
            return false;
        }
        let (gap, mut ahead) = if stamp < kept {
            (kept - stamp, -1)
        } else {
            (stamp - kept, 1)
        };
        if gap > 0x7FFF {
            ahead = -ahead;
        }
        if ahead > 0 {
            self.update_times[kind] = stamp;
            return true;
        }
        false
    }

    /// The stamp of the last move the server ordered.
    #[must_use]
    pub fn move_to_stamp(&self) -> u16 {
        self.update_times[MOVE_TO_STAMP]
    }

    /// One animation step of `dt` seconds, before the body's figure is told how it moves: the
    /// speed it is animated at ([`Body::anim_velocity`], how far it moved over its last tick)
    /// takes half its acceleration times the step squared, in its frame, unless its animation is
    /// frozen. The figure is then told that speed, not the velocity the body asks for: a body
    /// held against a wall stands, and a step up or down shows in its vertical speed. (The rest of
    /// an animation step, the figure's sliders, effects and triggers, is the figure's own.)
    // D2-06252, D2-06489
    pub fn update_animation(&mut self, dt: f32) {
        if self.is_animation_frozen() {
            return;
        }
        let a = crate::geom::global_to_local(&self.position.frame, self.acceleration);
        let v = &mut self.anim_velocity;
        v.x += a.x * 0.5 * dt * dt;
        v.y += a.y * 0.5 * dt * dt;
        v.z += a.z * 0.5 * dt * dt;
    }
}

impl Body {
    /// The object the body's targeted effects aim at.
    // D2-06099
    pub fn set_fx_target(&mut self, object: u64) {
        self.fx_target = object;
    }

    /// The look its effects clone from now on, with an appearance laid over it.
    // D2-06263
    pub fn set_look_to_clone(&mut self, look: u32, appearance: u32, values: &[(u32, f32)]) {
        self.look_to_clone = look;
        self.clone_appearance = appearance;
        self.clone_values = values.to_vec();
    }

    /// No object for its targeted effects, and no look for them to clone.
    pub fn clear_fx_target(&mut self) {
        self.fx_target = 0;
        self.look_to_clone = 0;
    }
}
