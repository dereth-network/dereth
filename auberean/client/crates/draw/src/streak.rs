//! Streaks: the trails an object's moving part leaves while they are on, such as a sword's swing.
//!
//! A streak holds a line on its object, two points in the object's own frame. While the streak is
//! on and its object is drawn, each frame lays the line where it then stands as the newest of a run
//! of segments, a new one once the line has moved far enough (or long enough has passed); each
//! segment fades over the streak's trail time and goes once that is past. The segments are drawn as
//! one ribbon in the streak's material, the line's second point along one edge and its first along
//! the other, its picture running along the trail by the distance covered.

use auberean_primitives::Vec3;
use auberean_world::color::Color;
use auberean_world::inside::block_offset;
use auberean_world::math::{length, scale as times, sub, Mat4};

use crate::material::DrawLayer;
use crate::mesh::MeshVertex;
use crate::scene::{Cull, ObjectBatch, Pass, PointLights};

/// The vertices the frame's streaks share: a streak whose ribbon would pass them is not drawn.
pub const STREAK_ROOM: usize = 4000;

/// One step of a streak's trail: when it was laid (seconds after the streak was made), where the
/// line's first and second points stood (in the frame of the block the trail started in), how
/// strongly it shows (one when new) and how far the trail ran up to it.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Segment {
    pub time: f64,
    pub head: Vec3,
    pub tail: Vec3,
    pub alpha: f32,
    pub distance: f32,
}

/// What an effect's streak says: the line on the object, how long the streak lasts (0: until it is
/// taken away), how long its trail takes to fade, the least time and distance between segments, its
/// color, its material and how often its picture repeats along a meter of trail.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StreakDesc {
    pub start: Vec3,
    pub end: Vec3,
    pub lifespan: f32,
    pub trail: f32,
    pub min_time: f32,
    pub min_distance: f32,
    pub color: Color,
    pub material: u32,
    pub texture_scale: f32,
}

impl StreakDesc {
    /// The streak an effect's node stores.
    #[must_use]
    pub fn of(s: &auberean_assets::fx::Streak) -> Self {
        Self {
            start: s.start,
            end: s.end,
            lifespan: s.lifespan,
            trail: s.trail_duration,
            min_time: s.min_segment_time,
            min_distance: s.min_segment_distance,
            color: Color::from_argb(s.color.0),
            material: s.material.0,
            texture_scale: s.texture_scale,
        }
    }
}

/// One streak on an object, and the trail it lays.
#[derive(Clone, Debug, PartialEq)]
pub struct Trail {
    desc: StreakDesc,
    active: bool,
    /// Its object's place has been given since it was turned on.
    placed: bool,
    /// The cell the trail started in: its segments stand in that cell's block's frame.
    base_cell: u32,
    /// Where its object was last drawn: its cell, its frame (in that cell's block's frame) and the
    /// largest of its scales.
    cell: u32,
    frame: Mat4,
    scale: f32,
    created: f64,
    segments: Vec<Segment>,
    /// The frame its object was last drawn in.
    stamp: u64,
}

impl Trail {
    /// A streak made at `now` (the client's running time): one that lasts a while is on at once,
    /// one that lasts until taken away waits to be turned on.
    // D2-10320, D2-10333
    #[must_use]
    pub fn new(desc: StreakDesc, now: f64) -> Self {
        Self {
            desc,
            active: desc.lifespan > 0.0,
            placed: false,
            base_cell: 0,
            cell: 0,
            frame: Mat4::IDENTITY,
            scale: 1.0,
            created: now,
            segments: Vec::new(),
            stamp: 0,
        }
    }

    /// Turn it on: its trail starts afresh where its object is next drawn.
    // D2-10315
    pub fn activate(&mut self) {
        if !self.active {
            self.placed = false;
            self.active = true;
        }
    }

    /// Turn it off: it lays no more segments, and those laid fade.
    // D2-10316
    pub fn deactivate(&mut self) {
        self.active = false;
    }

    /// Whether it is on.
    #[must_use]
    pub fn active(&self) -> bool {
        self.active
    }

    /// Its material instance.
    #[must_use]
    pub fn material(&self) -> u32 {
        self.desc.material
    }

    /// The trail's segments, oldest first.
    #[must_use]
    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    /// Its object drawn in frame `stamp`, in cell `cell` with frame `frame` (that cell's block's
    /// frame) and the largest of its scales `scale`. The first place given since it was turned on
    /// (or one in a cell after none) starts its trail afresh there.
    // D2-10325
    pub fn set_position(&mut self, stamp: u64, cell: u32, frame: Mat4, scale: f32) {
        self.cell = cell;
        self.frame = frame;
        self.scale = scale;
        self.stamp = stamp;
        if self.placed && (self.base_cell != 0 || cell == 0) {
            return;
        }
        self.segments.clear();
        self.base_cell = cell;
        self.placed = true;
    }

    /// The streak `now` (the client's running time, as the client keeps it, in single precision):
    /// past its lifespan it is off. While on and placed, the line where its object stands is the
    /// newest segment; it stays the newest (moving with the line) while within the least distance
    /// of the one before and, with a least time, until that time has passed, else a copy of it is
    /// laid after it. Every segment fades as the square of one less its age over the trail time,
    /// and goes when its age passes the trail time by a tenth of a second.
    // D2-10327
    pub fn think(&mut self, now: f64) {
        #[allow(clippy::cast_possible_truncation)] // the client's own single precision
        let age = f64::from(now as f32) - self.created;
        #[allow(clippy::cast_possible_truncation)]
        if self.desc.lifespan != 0.0 && self.desc.lifespan < age as f32 {
            self.active = false;
            return;
        }
        if self.active && self.placed {
            if self.segments.is_empty() {
                self.segments.push(Segment::default());
            }
            let n = self.segments.len();
            let off = block_offset(self.cell, self.base_cell);
            let line = |p: Vec3| sub(self.frame.point(times(p, self.scale)), off);
            let (head, tail) = (line(self.desc.start), line(self.desc.end));
            self.segments[n - 1] = Segment {
                time: age,
                head,
                tail,
                alpha: 1.0,
                distance: 0.0,
            };
            let mut stays = false;
            if n > 1 {
                let before = self.segments[n - 2];
                let d = length(sub(head, before.head)).max(length(sub(tail, before.tail)));
                self.segments[n - 1].distance = d + before.distance;
                #[allow(clippy::cast_possible_truncation)]
                let soon = f64::from(self.desc.min_time) > age - before.time;
                stays = d <= self.desc.min_distance && (self.desc.min_time == 0.0 || soon);
            }
            if !stays {
                let newest = self.segments[n - 1];
                self.segments.push(newest);
            }
        }
        let trail = self.desc.trail;
        let gone = f64::from(trail) + 0.1;
        self.segments.retain_mut(|s| {
            let dt = age - s.time;
            if dt > gone {
                return false;
            }
            #[allow(clippy::cast_possible_truncation)]
            let a = ((1.0 - dt / f64::from(trail)) as f32).max(0.0);
            s.alpha = a * a;
            true
        });
    }

    /// The ribbon of its trail for frame `stamp`, when its object was drawn in that frame and it
    /// has two segments or more, and it fits in the room the frame's streaks have left (`used`
    /// vertices taken; [`STREAK_ROOM`] in all), which it then takes: two vertices a segment, oldest
    /// first, the line's second point (the picture's v 0) then its first (v 1), the picture's u the
    /// distance run times the texture scale, each the streak's color with its alpha times the
    /// segment's, in the frame of the block the trail started in.
    // D2-10322
    #[must_use]
    pub fn geometry(&self, stamp: u64, used: &mut usize) -> Option<Vec<MeshVertex>> {
        let n = self.segments.len();
        if n < 2 || self.stamp != stamp || 2 * n + *used > STREAK_ROOM {
            return None;
        }
        // truncated, as the client turns a float into an integer
        #[allow(clippy::cast_sign_loss)]
        let byte = |v: f32| dereth_primitives::num::to_i32(v * 255.0) as u32 & 0xFF;
        let c = self.desc.color;
        let rgb = (byte(c.r) << 16) | (byte(c.g) << 8) | byte(c.b);
        let mut out = Vec::with_capacity(2 * n);
        for s in &self.segments {
            let diffuse = (byte(s.alpha * c.a) << 24) | rgb;
            let u = s.distance * self.desc.texture_scale;
            for (p, v) in [(s.tail, 0.0), (s.head, 1.0)] {
                out.push(MeshVertex {
                    position: [p.x, p.y, p.z],
                    normal: [0.0, 0.0, 1.0],
                    diffuse,
                    uv: [[u, v]; 4],
                    tangent_u: [0.0; 3],
                    tangent_v: [0.0; 3],
                    to_light: [0.0; 3],
                });
            }
        }
        *used += 2 * n;
        Some(out)
    }

    /// The draws of its ribbon (see [`Self::geometry`]) in `layers`, its material's, for a frame
    /// whose world is block `home`'s frame: the strip of two triangles a segment, in the step of
    /// the streaks.
    // D2-10318
    #[must_use]
    pub fn batches(
        &self,
        stamp: u64,
        used: &mut usize,
        layers: &[DrawLayer],
        home: u32,
    ) -> Vec<ObjectBatch> {
        let Some(vertices) = self.geometry(stamp, used) else {
            return Vec::new();
        };
        let count = u16::try_from(vertices.len()).unwrap_or(u16::MAX);
        let mut indices = Vec::new();
        for k in 0..count.saturating_sub(2) {
            if k % 2 == 0 {
                indices.extend([k, k + 1, k + 2]);
            } else {
                indices.extend([k + 1, k, k + 2]);
            }
        }
        let vertices = std::sync::Arc::new(vertices);
        let indices = std::sync::Arc::new(indices);
        let place = Mat4::translation(block_offset(home, self.base_cell));
        layers
            .iter()
            .map(|layer| ObjectBatch {
                vertices: vertices.clone(),
                posed: None,
                played: None,
                indices: indices.clone(),
                transform: place,
                layer: layer.clone(),
                tint: Color::WHITE,
                points: PointLights::NONE,
                mesh_type: 0,
                shade: None,
                outside: false,
                cull: Cull::Never,
                mesh: 0,
                casts_volume: false,
                caster: None,
                seen_through: false,
                kept_by: None,
                anchor: None,
                step: Some(Pass::Streaks),
                visible: None,
                block: None,
                far: false,
                ground: None,
                lod: None,
            })
            .collect()
    }
}
