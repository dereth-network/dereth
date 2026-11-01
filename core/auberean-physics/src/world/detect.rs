//! What an object of the world answers a detection or a line of sight about itself: whether any
//! part of it (a sphere at its scale, an upright cylinder, or the box around its physics trees) is
//! inside a detection's volume, and whether it blocks a line of sight.

use dereth_primitives::Position;

use super::World;
use crate::collision::manager::BodyKey;
use crate::detect::{DetectorSettings, ReportType, Sighting, Sightings};
use crate::ethereal::SightLine;
use crate::{geom, solid};

/// What the game objects add to a detection: whether an object is of a kind it reports, and what
/// they note of an object it noticed.
pub trait GameObjects {
    fn matches(&self, object: u64, report: &ReportType) -> bool;
    fn note(&self, object: u64, sighting: &mut Sighting);
}

impl World {
    /// Whether entry `key` is inside the volume of `settings`: one of its spheres (at its scale),
    /// one of its cylinders, or, when it has physics trees, the box around them.
    // D2-05933
    #[must_use]
    pub fn within_detect_volume(&self, key: BodyKey, settings: &DetectorSettings) -> bool {
        let (Some(e), Some(at)) = (self.entries.get(&key), self.position(key)) else {
            return false;
        };
        let v = &settings.volume;
        let s = &e.shape;
        if s.spheres
            .iter()
            .any(|sp| v.is_within_sphere(sp, &at, s.scale))
        {
            return true;
        }
        if s.cylinders.iter().any(|c| v.is_within_cylinder(c, &at)) {
            return true;
        }
        s.has_physics_tree() && v.is_within_box(&s.physics_bounding_box(), &at)
    }

    /// Whether entry `key` blocks the line of sight `los`: by its physics type against the types
    /// the line passes through, and by its id against the line's lists.
    // D2-06132
    #[must_use]
    pub fn is_los_blocking(&self, key: BodyKey, los: &SightLine) -> bool {
        self.entries
            .get(&key)
            .is_some_and(|e| solid::is_blocking(e.physics_type, e.id, &los.objects, &los.physics))
    }

    /// Whether a detection with `settings` is after entry `key`: never one it ignores; one it
    /// targets when it has targets; else one of a kind it reports.
    // D2-06131
    #[must_use]
    pub fn check_type_match(
        &self,
        key: BodyKey,
        settings: &DetectorSettings,
        game: &dyn GameObjects,
    ) -> bool {
        let id = self.object_id(key);
        if settings.in_exclude_list(id) {
            return false;
        }
        if !settings.targets.is_empty() {
            return settings.in_target_list(id);
        }
        game.matches(id, &settings.report)
    }

    /// Notes entry `key` among `sightings` when it is not there yet: what its game object notes,
    /// where it is, and its distance from the detection's `origin`. One already noted is left as
    /// it was.
    // D2-06264
    pub fn add_to_target_list(
        &self,
        key: BodyKey,
        origin: &Position,
        sightings: &mut Sightings,
        game: &dyn GameObjects,
    ) {
        let id = self.object_id(key);
        let Some(at) = self.position(key) else {
            return;
        };
        if sightings.contains_key(&id) {
            return;
        }
        let mut s = Sighting::default();
        s.init(id);
        game.note(id, &mut s);
        s.position = at;
        s.distance = geom::distance(origin, &at);
        sightings.insert(id, s);
    }
}
