//! Where the selected object stands on screen, as the target indicator reads it.

/// The two displayable results of the object bounding-box query that places the target
/// indicator. Missing or invalid physics is `None` at the seam, not an off-screen target.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Projection {
    /// On screen: the object's rectangle, `(left, top, right, bottom)`.
    OnScreen((i32, i32, i32, i32)),
    /// Off screen: the direction to it, in radians.
    OffScreen(f32),
}
