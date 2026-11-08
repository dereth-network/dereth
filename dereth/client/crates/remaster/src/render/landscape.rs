//! What the remaster adds to the landscape past retail's own pictures, each on its own switch:
//! the color grade by kind of day.

/// Which of the landscape's additions are drawn. All off is the frame as it was drawn before any
/// of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LandscapeSettings {
    /// The frame graded by a look-up table for its kind of day and hour, after the tone curve.
    pub grade: bool,
}

impl super::Renderer {
    /// Which of the landscape's additions are drawn, from the next frame on.
    pub fn set_landscape(&mut self, s: LandscapeSettings) {
        self.landscape = s;
        self.post.set_grade(s.grade);
    }

    /// Which of the landscape's additions are drawn.
    #[must_use]
    pub fn landscape(&self) -> LandscapeSettings {
        self.landscape
    }
}
