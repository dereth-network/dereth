//! A world's defended devices: a creature of a template (a device an item's use sets down) that
//! creatures come to attack while it stands, and that leaves something in its place once it has
//! stood long enough.
//!
//! **Attacked.** From the moment a device comes into the world it keeps creatures of its row's
//! spawns about it, as a camp keeps its own: its `population` at once, each one killed replaced
//! `regen` seconds after its body leaves the world, each appearing `radius` meters from it and going at it. The
//! device itself fights no one.
//!
//! **Held.** Once it has stood its `holds` seconds the device goes, and what its row names stands in
//! its place (a portal for its time); what it made stays until killed. A device killed before
//! then is gone, and nothing stands in its place.
//!
//! **Said.** The device says its row's lines aloud of itself: each at its time after it came, one
//! as it is first badly hurt, one as it holds.
//!
//! A world with no defended devices does none of this.

/// One row: the device's template, how long it must stand, what attacks it, and what it leaves.
#[derive(Debug, Clone, PartialEq)]
pub struct Defense {
    pub device: u32,
    /// Seconds it must stand, above 0.
    pub holds: f64,
    /// What it keeps about it: templates and their weights.
    pub spawns: Vec<(u32, u32)>,
    /// How many it keeps alive at once.
    pub population: u32,
    /// Seconds from a dead attacker's body leaving the world to its replacement (none: the device's
    /// template's generator period).
    pub regen: Option<f64>,
    /// How far from the device each appears, in meters.
    pub radius: f32,
    /// What stands in its place once it has held: a template.
    pub then: Option<u32>,
    /// How long that stands, in seconds (none: a portal until the server stops, a creature until
    /// killed).
    pub then_lasts: Option<u32>,
    /// The name that is shown and examined by, in place of its template's.
    pub then_name: Option<crate::Words>,
    /// What the device says aloud of itself as it stands: each line (a string and its table) so many
    /// seconds after it came (0: as it comes), in order.
    pub says: Vec<(f64, (u32, u32))>,
    /// What it says aloud once, as its health first falls below half.
    pub hurt_line: Option<(u32, u32)>,
    /// What it says aloud as it holds, before it goes.
    pub held_line: Option<(u32, u32)>,
}

/// Every defended device of a world.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DefenseTable {
    pub rows: Vec<Defense>,
}

impl DefenseTable {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The row of a device's template, by its place among the rows.
    #[must_use]
    pub fn of_device(&self, template: u32) -> Option<usize> {
        self.rows.iter().position(|r| r.device == template)
    }
}
