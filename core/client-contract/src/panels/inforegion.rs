//! The information row kinds carried by panel views.

/// The five `InfoRegion` classes and the format each renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InfoRegionKind {
    /// The base class; sets the row's state and handles quality changes.
    Base,
    /// Primary attribute — `"%d"`, or `"???"` when unknown.
    Attribute,
    /// Vital attribute — `"%d/%d"`, `"%d %%"`, `"%d/%d (%d %%)"`, `"???"`.
    Attribute2nd,
    /// Skill — `"%d"`, with the vitae modifier applied.
    Skill,
    /// Effect duration — `"%d:%02d"` or `"%d:%02d:%02d"`.
    Effect,
}
