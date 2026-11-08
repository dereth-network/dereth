//! The intent: retail's light inputs with the look laid over them, which is what every pass of the
//! remaster lights the frame by. Retail Look reads the frame's [`RetailLight`] and never this.

use crate::look::{DayKind, Layer, Look, LookStack, LookToml};
use crate::scene::{FrameDelta, RetailLight};

/// One frame's intent.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Intent {
    /// Retail's light inputs with the look laid over them.
    pub light: RetailLight,
    /// The look of the frame's moment.
    pub look: Look,
    /// The kind of day the frame's day group is.
    pub day: Option<DayKind>,
}

/// The looks the remaster is given, and the last frame's intent.
#[derive(Debug, Clone, Default)]
pub struct Intents {
    looks: LookStack,
    last: Intent,
}

impl Intents {
    /// No look but the identity.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set one layer's look, or take it away. Returns whether the looks changed.
    pub fn set(&mut self, layer: Layer, look: Option<LookToml>) -> bool {
        self.looks.set(layer, look)
    }

    /// Every layer's look at once, in place of what was set.
    pub fn set_all(&mut self, looks: LookStack) {
        self.looks = looks;
    }

    /// Every layer's look.
    #[must_use]
    pub fn looks(&self) -> &LookStack {
        &self.looks
    }

    /// The intent of `frame`: its light inputs with the look of its moment laid over them.
    pub fn of(&mut self, frame: &FrameDelta) -> &Intent {
        let day = DayKind::of_group(&frame.day_group);
        let look = self.looks.look(day, frame.light.time_of_day);
        self.last = Intent {
            light: look.apply(&frame.light),
            look,
            day,
        };
        &self.last
    }

    /// The last frame's intent.
    #[must_use]
    pub fn last(&self) -> &Intent {
        &self.last
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    /// **A frame's intent is its light under the look of its moment and its kind of day**: the
    /// identity gives the frame's light back, and a rainy table applies on a rainy day only.
    #[test]
    fn a_frames_intent_is_its_light_under_the_look_of_its_day_and_hour() {
        let mut frame = FrameDelta {
            day_group: "Rainy".into(),
            ..FrameDelta::default()
        };
        frame.light.sun = glam::Vec3::new(0.0, 0.0, 1.0);
        frame.light.time_of_day = 0.5;
        let mut i = Intents::new();
        assert_eq!(i.of(&frame).light, frame.light);
        assert_eq!(i.last().day, Some(DayKind::Rainy));
        i.set(
            Layer::Player,
            Some(LookToml::parse("[rainy]\nsun = 2\n").expect("a look")),
        );
        assert!((i.of(&frame).light.sun.z - 2.0).abs() < 1e-6);
        frame.day_group = "Sunny".into();
        assert_eq!(i.of(&frame).light, frame.light);
    }
}
