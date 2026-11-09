//! What the remaster adds to the landscape past retail's own pictures, each on its own switch:
//! the color grade by kind of day; the air over the land, the sky's light in the shade and a low
//! fog lit by the sun, on every frame outdoors; the grass field; flowers and pebbles.

/// Which of the landscape's additions are drawn. All off is the frame as it was drawn before any
/// of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LandscapeSettings {
    /// The frame graded by a look-up table for its kind of day and hour, after the tone curve.
    pub grade: bool,
    /// The air over the land and the sky's light in the shade, as an art pack's look draws them,
    /// with or without a pack; and a fog lying low over the lowest ground round the eye, lit by the
    /// sun.
    pub air: bool,
    /// A field of grass blades over the ground round the eye, where the terrain grows grass.
    pub grass: bool,
    /// Flowers in clusters over grassland, and pebbles on bare ground and along the roads.
    pub flowers: bool,
}

impl super::Renderer {
    /// Which of the landscape's additions are drawn, from the next frame on.
    pub fn set_landscape(&mut self, s: LandscapeSettings) {
        self.landscape = s;
        self.post.set_grade(s.grade);
        self.drawn_ground.grass = s.grass;
        self.drawn_ground.flowers = s.flowers;
    }

    /// Which of the landscape's additions are drawn.
    #[must_use]
    pub fn landscape(&self) -> LandscapeSettings {
        self.landscape
    }
}
