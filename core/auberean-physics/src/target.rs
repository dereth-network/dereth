//! Watching other bodies: a body tracks one target (a move to an object follows it through this),
//! and is watched by any number of others. A watched body tells each watcher where it is whenever
//! its projected place drifts past the watcher's radius from what it last told, checked every half
//! second; a watcher that hears nothing for ten seconds marks its target timed out.

use std::collections::BTreeMap;

use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use crate::geom::{self, EPS};

mod pack;
pub use pack::{Aim, NO_OBJECT};

/// What a watcher knows of its target.
pub mod status {
    /// Nothing heard yet.
    pub const UNDEFINED: u32 = 0;
    /// In the world where last heard.
    pub const OK: u32 = 1;
    /// Left the world.
    pub const EXITED_WORLD: u32 = 2;
    /// Began a teleport.
    pub const TELEPORTED: u32 = 3;
    /// Put inside a container, or held by another object.
    pub const CONTAINED: u32 = 4;
    pub const PARENTED: u32 = 5;
    /// Nothing heard for ten seconds, or no target.
    pub const TIMED_OUT: u32 = 6;
}

/// How often watchers are checked, seconds.
pub const CHECK_INTERVAL: f64 = 0.5;
/// How long a watcher waits to hear from its target, seconds.
pub const TIMEOUT: f64 = 10.0;

fn origin() -> Position {
    Position::new(CellId(0), Frame::new(Vec3::ZERO, Quat::IDENTITY))
}

/// What a watcher hears of its target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TargetInfo {
    pub context: u32,
    pub object: u64,
    /// How far the target may drift before it says so again.
    pub radius: f32,
    /// How far ahead, in seconds, the target projects its place.
    pub quantum: f64,
    pub target_position: Position,
    pub interpolated_position: Position,
    /// From the watcher toward the target's projected place.
    pub interpolated_heading: Vec3,
    pub velocity: Vec3,
    pub status: u32,
    pub last_update_time: f64,
    pub object_radius: f32,
    pub object_height: f32,
    pub scale: Vec3,
}

impl Default for TargetInfo {
    // D2-07218
    fn default() -> Self {
        Self {
            context: 0,
            object: 0,
            radius: 0.0,
            quantum: 0.0,
            target_position: origin(),
            interpolated_position: origin(),
            interpolated_heading: Vec3::ZERO,
            velocity: Vec3::ZERO,
            status: status::UNDEFINED,
            last_update_time: 0.0,
            object_radius: 0.0,
            object_height: 0.0,
            scale: Vec3::new(1.0, 1.0, 1.0),
        }
    }
}

/// One watcher of a body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Voyeur {
    pub object: u64,
    pub radius: f32,
    pub quantum: f64,
    /// Where the body last told it it was.
    pub last_sent: Position,
}

impl Voyeur {
    // D2-07212, D2-07246
    #[must_use]
    pub fn new(object: u64, radius: f32, quantum: f64) -> Self {
        Self {
            object,
            radius,
            quantum,
            last_sent: origin(),
        }
    }
}

/// The world as a body's tracking sees it: itself, and the other bodies it tells and hears.
pub trait TrackHost {
    fn id(&self) -> u64;
    fn position(&self) -> Position;
    fn velocity(&self) -> Vec3;
    /// The radius, height and scale of the body at the top of its containment.
    fn top_radius(&self) -> f32;
    fn top_height(&self) -> f32;
    fn scale(&self) -> Vec3;
    /// Asks another body to tell this one where it is.
    fn add_voyeur(&mut self, target: u64, radius: f32, quantum: f64);
    /// Asks another body to stop telling this one.
    fn remove_voyeur(&mut self, target: u64);
    /// Tells a watcher where this body is.
    fn send_update(&mut self, watcher: u64, info: TargetInfo);
    /// This body's target changed: a move to it follows.
    fn target_changed(&mut self, info: &TargetInfo);
    /// The clock, seconds.
    fn now(&self) -> f64;
}

/// A body's tracking: its target, and its watchers.
// D2-07243, D2-07237
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Tracking {
    pub target: Option<TargetInfo>,
    pub voyeurs: BTreeMap<u64, Voyeur>,
    pub last_check: f64,
}

impl Tracking {
    // D2-07248
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Where this body will be `quantum` seconds on, moving as it moves.
    // D2-07217
    #[must_use]
    pub fn interpolated_position(host: &dyn TrackHost, quantum: f64) -> Position {
        let mut p = host.position();
        let v = host.velocity();
        #[allow(clippy::cast_possible_truncation)]
        let t = quantum as f32;
        p.frame.origin = Vec3::new(
            v.x * t + p.frame.origin.x,
            v.y * t + p.frame.origin.y,
            v.z * t + p.frame.origin.z,
        );
        p
    }

    /// Stops following the target.
    // D2-07223
    pub fn clear_target(&mut self, host: &mut dyn TrackHost) {
        if let Some(t) = self.target.take() {
            host.remove_voyeur(t.object);
        }
    }

    /// How far ahead the target projects its place.
    // D2-07224
    pub fn set_target_quantum(&mut self, host: &mut dyn TrackHost, quantum: f64) {
        if let Some(t) = self.target.as_mut() {
            t.quantum = quantum;
            host.add_voyeur(t.object, t.radius, t.quantum);
        }
    }

    /// The target says where it is: kept, with the direction to its projected place.
    // D2-07225
    pub fn receive_update(&mut self, host: &mut dyn TrackHost, info: &TargetInfo) {
        let now = host.now();
        let here = host.position();
        let Some(t) = self.target.as_mut() else {
            return;
        };
        if info.object != t.object {
            return;
        }
        t.radius = info.radius;
        t.quantum = info.quantum;
        t.target_position = info.target_position;
        t.interpolated_position = info.interpolated_position;
        t.velocity = info.velocity;
        t.status = info.status;
        t.last_update_time = now;
        t.object_radius = info.object_radius;
        t.object_height = info.object_height;
        t.scale = info.scale;
        let o = geom::offset(&here, &t.interpolated_position);
        let len = (o.z * o.z + o.y * o.y + o.x * o.x).sqrt();
        t.interpolated_heading = if len < EPS {
            Vec3::new(0.0, 0.0, 1.0)
        } else {
            let k = 1.0 / len;
            Vec3::new(k * o.x, k * o.y, k * o.z)
        };
        let t = *t;
        host.target_changed(&t);
    }

    /// Tells a watcher where this body is (`at`, its projected place) and how it stands.
    // D2-07226
    pub fn send_voyeur_update(
        host: &mut dyn TrackHost,
        voyeur: &mut Voyeur,
        at: Position,
        status: u32,
    ) {
        voyeur.last_sent = at;
        let info = TargetInfo {
            context: 0,
            object: host.id(),
            radius: voyeur.radius,
            quantum: voyeur.quantum,
            target_position: host.position(),
            interpolated_position: at,
            velocity: host.velocity(),
            status,
            object_radius: host.top_radius(),
            object_height: host.top_height(),
            scale: host.scale(),
            ..TargetInfo::default()
        };
        host.send_update(voyeur.object, info);
    }

    /// Tells a watcher again when this body's projected place has drifted past its radius.
    // D2-07227
    pub fn check_and_update_voyeur(host: &mut dyn TrackHost, voyeur: &mut Voyeur) {
        let at = Self::interpolated_position(&*host, voyeur.quantum);
        if geom::distance(&at, &voyeur.last_sent) > voyeur.radius {
            Self::send_voyeur_update(host, voyeur, at, status::OK);
        }
    }

    /// Follows a new target (an object id of 0 for none, which reads as timed out at once).
    // D2-07230
    pub fn set_target(
        &mut self,
        host: &mut dyn TrackHost,
        context: u32,
        object: u64,
        radius: f32,
        quantum: f64,
    ) {
        self.clear_target(host);
        if object == 0 {
            let info = TargetInfo {
                context,
                status: status::TIMED_OUT,
                ..TargetInfo::default()
            };
            host.target_changed(&info);
            return;
        }
        let t = TargetInfo {
            context,
            object,
            radius,
            quantum,
            last_update_time: host.now(),
            ..TargetInfo::default()
        };
        self.target = Some(t);
        host.add_voyeur(object, radius, quantum);
    }

    /// Every half second: a target unheard for ten seconds times out, and each watcher is told
    /// again if this body has drifted.
    // D2-07234
    pub fn handle_targetting(&mut self, host: &mut dyn TrackHost) {
        let now = host.now();
        if now - self.last_check < CHECK_INTERVAL {
            return;
        }
        if let Some(t) = self.target.as_mut() {
            if t.status == status::UNDEFINED && t.last_update_time + TIMEOUT < now {
                t.status = status::TIMED_OUT;
                let t = *t;
                host.target_changed(&t);
            }
        }
        for v in self.voyeurs.values_mut() {
            Self::check_and_update_voyeur(host, v);
        }
        self.last_check = host.now();
    }

    /// Tells every watcher of an event (leaving the world, ...).
    // D2-07235
    pub fn notify_voyeurs(&mut self, host: &mut dyn TrackHost, status: u32) {
        let here = host.position();
        for v in self.voyeurs.values_mut() {
            Self::send_voyeur_update(host, v, here, status);
        }
    }

    /// Leaving the world: the target is dropped and every watcher told.
    // D2-07236
    pub fn handle_leave_world(&mut self, host: &mut dyn TrackHost) {
        self.clear_target(host);
        self.notify_voyeurs(host, status::EXITED_WORLD);
    }

    // D2-07238
    pub fn remove_voyeur(&mut self, watcher: u64) {
        self.voyeurs.remove(&watcher);
    }

    /// A new watcher, told at once where this body is; one already watching takes the new radius
    /// and quantum. The watcher kept does not note that telling (the client tells from a copy), so
    /// the next check tells it again.
    // D2-07244
    pub fn add_voyeur(
        &mut self,
        host: &mut dyn TrackHost,
        watcher: u64,
        radius: f32,
        quantum: f64,
    ) {
        if let Some(v) = self.voyeurs.get_mut(&watcher) {
            v.radius = radius;
            v.quantum = quantum;
            return;
        }
        let v = Voyeur::new(watcher, radius, quantum);
        self.voyeurs.insert(watcher, v);
        let mut told = v;
        let here = host.position();
        Self::send_voyeur_update(host, &mut told, here, status::OK);
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the tracking's bookkeeping, held to a made-up pair of bodies)
    use super::*;

    struct Host {
        at: Position,
        now: f64,
        sent: Vec<(u64, TargetInfo)>,
        changed: Vec<TargetInfo>,
        watching: Vec<u64>,
    }

    impl TrackHost for Host {
        fn id(&self) -> u64 {
            7
        }
        fn position(&self) -> Position {
            self.at
        }
        fn velocity(&self) -> Vec3 {
            Vec3::ZERO
        }
        fn top_radius(&self) -> f32 {
            0.5
        }
        fn top_height(&self) -> f32 {
            2.0
        }
        fn scale(&self) -> Vec3 {
            Vec3::new(1.0, 1.0, 1.0)
        }
        fn add_voyeur(&mut self, target: u64, _radius: f32, _quantum: f64) {
            self.watching.push(target);
        }
        fn remove_voyeur(&mut self, target: u64) {
            self.watching.retain(|t| *t != target);
        }
        fn send_update(&mut self, watcher: u64, info: TargetInfo) {
            self.sent.push((watcher, info));
        }
        fn target_changed(&mut self, info: &TargetInfo) {
            self.changed.push(*info);
        }
        fn now(&self) -> f64 {
            self.now
        }
    }

    fn host() -> Host {
        Host {
            at: Position::new(
                CellId(0x1010_0001),
                Frame::new(Vec3::new(10.0, 10.0, 0.0), Quat::IDENTITY),
            ),
            now: 0.0,
            sent: Vec::new(),
            changed: Vec::new(),
            watching: Vec::new(),
        }
    }

    #[test]
    fn a_watcher_is_told_again_only_past_its_radius() {
        let mut h = host();
        let mut t = Tracking::new();
        t.add_voyeur(&mut h, 9, 1.0, 0.0);
        assert_eq!(h.sent.len(), 1);
        // The kept watcher never noted the first telling, so the first check tells again.
        h.now = 0.5;
        t.handle_targetting(&mut h);
        assert_eq!(h.sent.len(), 2);
        h.now = 1.0;
        h.at.frame.origin.x += 0.9;
        t.handle_targetting(&mut h);
        assert_eq!(h.sent.len(), 2, "within the radius");
        h.now = 1.5;
        h.at.frame.origin.x += 0.2;
        t.handle_targetting(&mut h);
        assert_eq!(h.sent.len(), 3, "past it");
    }

    #[test]
    fn a_target_unheard_for_ten_seconds_times_out() {
        let mut h = host();
        let mut t = Tracking::new();
        t.set_target(&mut h, 0, 9, 0.5, 1.0);
        assert_eq!(h.watching, vec![9]);
        h.now = 10.5;
        t.handle_targetting(&mut h);
        assert_eq!(h.changed.last().map(|i| i.status), Some(status::TIMED_OUT));
        t.handle_leave_world(&mut h);
        assert!(h.watching.is_empty());
    }
}
