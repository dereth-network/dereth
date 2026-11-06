//! What a body follows and what it watches for: its target (the one object it follows, which
//! tells it where it is) and its watchers (the objects following it, which it tells), kept once it
//! has either; and its detection spheres and the detections it makes at once, kept once it has
//! any.
//!
//! A body reaches other objects only through [`Notice`]s: asking a target to be watched or to
//! stop, and telling a watcher where it is. [`Body::deliver`] hands such a notice to the body it is
//! for. What the body hears of its target goes nowhere further: the client's handler of a changed
//! target does nothing.

use dereth_primitives::{Position, Vec3};

use super::{Body, Notice};
use crate::detect::manager::{DetectionHost, Detections};
use crate::detect::{Detected, DetectorSettings, ReportType, Sightings};
use crate::target::{TargetInfo, TrackHost, Tracking};

/// The body as its tracking sees it.
struct Tracked<'a> {
    body: &'a mut Body,
}

impl TrackHost for Tracked<'_> {
    fn id(&self) -> u64 {
        self.body.id
    }
    fn position(&self) -> Position {
        self.body.position
    }
    fn velocity(&self) -> Vec3 {
        self.body.velocity
    }
    fn top_radius(&self) -> f32 {
        self.body.radius()
    }
    fn top_height(&self) -> f32 {
        self.body.height()
    }
    fn scale(&self) -> Vec3 {
        let s = self.body.config.scale;
        Vec3::new(s, s, s)
    }
    fn add_voyeur(&mut self, target: u64, radius: f32, quantum: f64) {
        let watcher = self.body.id;
        self.body.notices.push(Notice::Watch {
            target,
            watcher,
            radius,
            quantum,
        });
    }
    fn remove_voyeur(&mut self, target: u64) {
        let watcher = self.body.id;
        self.body.notices.push(Notice::Unwatch { target, watcher });
    }
    fn send_update(&mut self, watcher: u64, info: TargetInfo) {
        self.body
            .notices
            .push(Notice::Whereabouts { watcher, info });
    }
    fn target_changed(&mut self, _info: &TargetInfo) {}
    fn now(&self) -> f64 {
        self.body.clock
    }
}

impl Body {
    /// Runs `f` on the body's tracking (made first when `make` and it has none).
    pub(super) fn with_tracking<R>(
        &mut self,
        make: bool,
        f: impl FnOnce(&mut Tracking, &mut dyn TrackHost) -> R,
    ) -> Option<R> {
        let mut t = match self.tracking.take() {
            Some(t) => t,
            None if make => Tracking::new(),
            None => return None,
        };
        let r = f(&mut t, &mut Tracked { body: &mut *self });
        self.tracking = Some(t);
        Some(r)
    }

    /// Follows `object` (0: none) under `context`, told again whenever it drifts past `radius`,
    /// projected `quantum` seconds ahead.
    // D2-05934
    pub fn set_target(&mut self, context: u32, object: u64, radius: f32, quantum: f32) {
        self.with_tracking(true, |t, h| {
            t.set_target(h, context, object, radius, f64::from(quantum));
        });
    }

    // D2-05935
    pub fn clear_target(&mut self) {
        self.with_tracking(false, |t, h| t.clear_target(h));
    }

    /// The object the body follows; 0 for none.
    // D2-05936
    #[must_use]
    pub fn target_id(&self) -> u64 {
        self.tracking
            .as_ref()
            .and_then(|t| t.target.as_ref())
            .map_or(0, |t| t.object)
    }

    // D2-05937
    pub fn set_target_quantum(&mut self, quantum: f64) {
        self.with_tracking(false, |t, h| t.set_target_quantum(h, quantum));
    }

    /// How far ahead the target projects its place; 0 with no target.
    #[must_use]
    pub fn target_quantum(&self) -> f64 {
        self.tracking
            .as_ref()
            .and_then(|t| t.target.as_ref())
            .map_or(0.0, |t| t.quantum)
    }

    /// What the body knows of its target, when it follows one.
    #[must_use]
    pub fn target_info(&self) -> Option<TargetInfo> {
        self.tracking.as_ref().and_then(|t| t.target)
    }

    /// The target says where it is.
    // D2-05940
    pub fn receive_target_update(&mut self, info: &TargetInfo) {
        self.with_tracking(false, |t, h| t.receive_update(h, info));
    }

    /// `watcher` follows the body: told at once, then whenever it drifts past `radius`.
    // D2-05941
    pub fn add_voyeur(&mut self, watcher: u64, radius: f32, quantum: f64) {
        self.with_tracking(true, |t, h| t.add_voyeur(h, watcher, radius, quantum));
    }

    /// `watcher` stops following the body; whether it followed.
    // D2-05942
    pub fn remove_voyeur(&mut self, watcher: u64) -> bool {
        self.tracking.as_mut().is_some_and(|t| {
            let had = t.voyeurs.contains_key(&watcher);
            t.remove_voyeur(watcher);
            had
        })
    }

    /// Every half second, the target heard from or timed out, and each watcher told again if the
    /// body has drifted.
    pub(crate) fn handle_targetting(&mut self) {
        self.with_tracking(false, |t, h| t.handle_targetting(h));
    }

    /// Hands `notice` to this body when it is addressed to it (by id): a target asked to be
    /// watched or to stop, a watcher told where its target is, or a holder letting go of a
    /// child. False when it is not for it.
    pub fn deliver(&mut self, notice: &Notice) -> bool {
        match *notice {
            Notice::Watch {
                target,
                watcher,
                radius,
                quantum,
            } if target == self.id => self.add_voyeur(watcher, radius, quantum),
            Notice::Unwatch { target, watcher } if target == self.id => {
                self.remove_voyeur(watcher);
            }
            Notice::Whereabouts { watcher, ref info } if watcher == self.id => {
                self.receive_target_update(info);
            }
            Notice::RemoveChild { parent, child } if parent == self.id => {
                self.remove_child(child);
            }
            _ => return false,
        }
        true
    }

    /// The body's detections, made when it has none.
    fn detections_mut(&mut self) -> &mut Detections {
        self.detections.get_or_insert_with(Detections::new)
    }

    /// A detection sphere under `context` around `object` (0: around the body, noticing what
    /// enters the cells it reaches), its radius the body's plus `radius`; the body's other updates
    /// wake to check it.
    pub fn create_detection_cylsphere(
        &mut self,
        context: u32,
        object: u64,
        radius: f32,
        report: ReportType,
        host: &mut dyn DetectionHost,
    ) -> bool {
        self.detections_mut();
        self.set_misc_active(true);
        let r = self.radius() + radius;
        self.detections_mut()
            .create_detector(context, object, r, report, host)
    }

    /// How many detection spheres watch `object` (none for object 0).
    #[must_use]
    pub fn num_targeted_detection_cylspheres_on(&self, object: u64) -> usize {
        match &self.detections {
            Some(d) if object != 0 => d.num_targeted_on(object),
            _ => 0,
        }
    }

    #[must_use]
    pub fn num_targeted_detection_cylspheres(&self) -> usize {
        self.detections.as_ref().map_or(0, Detections::num_targeted)
    }

    #[must_use]
    pub fn num_global_detection_cylspheres(&self) -> usize {
        self.detections.as_ref().map_or(0, Detections::num_global)
    }

    /// Ends the detection spheres watching `object`.
    pub fn remove_targeted_detection_spheres(&mut self, object: u64) -> bool {
        self.detections
            .as_mut()
            .is_some_and(|d| d.remove_targeted(object))
    }

    #[must_use]
    pub fn has_detection_cylsphere(&self, context: u32) -> bool {
        self.detections
            .as_ref()
            .is_some_and(|d| d.has_detector(context))
    }

    /// Ends the detection sphere under `context` (at the next check).
    pub fn delete_detection_cylsphere(&mut self, context: u32) -> bool {
        self.detections
            .as_mut()
            .is_some_and(|d| d.destroy_detector(context))
    }

    pub fn delete_all_targeted_detection_cylspheres(&mut self) -> bool {
        self.detections
            .as_mut()
            .is_some_and(Detections::delete_all_targeted)
    }

    /// The bodies watching the body's cell.
    #[must_use]
    pub fn inq_detectors(host: &dyn DetectionHost) -> Vec<u64> {
        Detections::watchers(host)
    }

    /// A change the cells noticed for the body's global detection; its other updates wake to
    /// report it.
    // D2-05924
    pub fn receive_detection_update(&mut self, info: &Detected) {
        if let Some(d) = self.detections.as_mut() {
            d.receive_update(info);
            self.set_misc_active(true);
        }
    }

    /// Starts a detection made at once under `context`; false when one runs under it already.
    // D2-05930
    pub fn init_detection_list(&mut self, context: u32, settings: &DetectorSettings) -> bool {
        self.detections_mut().init_intersection(context, settings)
    }

    /// Makes the detection `settings` describes at once: what it took in (with the outcome), or
    /// none while it waits for blocks held elsewhere. One already running under its context
    /// comes back at once with nothing and outcome 7.
    // D2-05931
    pub fn do_intersection(
        &mut self,
        settings: &DetectorSettings,
        host: &mut dyn DetectionHost,
    ) -> Option<(Sightings, u32)> {
        let context = settings.context;
        host.increment_physics_stamp();
        if !self.init_detection_list(context, settings) {
            return Some((Sightings::default(), ALREADY_RUNNING));
        }
        let d = self.detections.as_mut()?;
        let mut out = 0;
        if !d.do_intersection(context, &mut out, host) {
            return None;
        }
        let found = std::mem::take(d.profile_list(context));
        d.remove_context(context);
        Some((found, out))
    }
}

/// What a detection made at once comes to when one runs under its context already.
pub const ALREADY_RUNNING: u32 = 7;
