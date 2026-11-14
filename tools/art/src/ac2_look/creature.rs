//! A creature's bodies: each an AC2 entity's skinned meshes posed in its bind clip's first keys,
//! scaled and placed as its table row says, drawn in place of every part of an Asheron's Call
//! setup, with AC2's own clips for each motion slot carried onto it; and its looks, the body under
//! other AC2 pictures, tinted.
//!
//! What a body file holds is the pack format's skinned body: a joint for each part it is drawn in
//! place of (`part_NN`, the first carrying where the body stands in it), its own bones posed only
//! by its clips, a vertex's weight on each, and one animation a slot.

use std::collections::BTreeMap;

use auberean_assets::animation::{Animation, AnimationKind, KeyframeAnimation};
use auberean_assets::entity::Entity;
use auberean_assets::mesh::Mesh;
use auberean_assets::setup::Setup as Ac2Setup;
use auberean_assets::visual::VisualDesc;
use glam::{DMat3, DMat4, DQuat, DVec3, DVec4};
use serde_json::{json, Value};

use super::picture;
use super::source::{Ac2, Rgba};
use super::tables::{Body, Creature, Look, SLOTS};

/// The largest picture a body takes, texels a side.
pub const PICTURE_LIMIT: u32 = 1024;

/// Keys a second a carried clip is written at.
const RATE: f64 = 30.0;

/// The least a bone is scaled to, so the bones under one AC2 shrinks to nothing stay placed.
const MIN_SCALE: f64 = 1e-3;

/// The group a player's body is; a creature's own one group is taken where it has none.
const BODY_GROUP: u32 = 0x8100_0001;

/// The groups drawn, in order: the body, the head, the beard, the crest, the tail and its frill.
const GROUPS: [u32; 6] = [
    0x8100_0001,
    0x8100_0002,
    0x8100_0003,
    0x8100_0004,
    0x8100_0005,
    0x8100_0006,
];

/// A number as glTF stores it.
#[allow(clippy::cast_possible_truncation)] // LINT-OK: glTF stores single precision.
const fn single(v: f64) -> f32 {
    v as f32
}

/// An AC2 point in glTF's axes: (x, z, -y).
fn cv(x: f32, y: f32, z: f32) -> DVec3 {
    DVec3::new(f64::from(x), f64::from(z), -f64::from(y))
}

/// An AC2 turn in glTF's axes, of unit length.
fn cq(w: f32, x: f32, y: f32, z: f32) -> DQuat {
    let (w, x, y, z) = (f64::from(w), f64::from(x), f64::from(y), f64::from(z));
    let n = (w * w + x * x + y * y + z * z).sqrt();
    let n = if n == 0.0 { 1.0 } else { n };
    DQuat::from_xyzw(x / n, z / n, -y / n, w / n)
}

/// An AC2 scale in glTF's axes.
fn cs(x: f32, y: f32, z: f32) -> DVec3 {
    DVec3::new(f64::from(x), f64::from(z), f64::from(y))
}

fn trs(t: DVec3, q: DQuat, s: DVec3) -> DMat4 {
    DMat4::from_scale_rotation_translation(s, q, t)
}

/// The turn nearest a matrix's: its scale and shear taken out. A matrix scaled to nothing (a bone
/// AC2 hides) turns nothing.
fn rot(m: &DMat4) -> DMat3 {
    let r = DMat3::from_mat4(*m);
    let largest = r.to_cols_array().iter().fold(0f64, |a, v| a.max(v.abs()));
    if largest == 0.0 {
        return DMat3::IDENTITY;
    }
    // However small its scale, a matrix's turn is its own.
    let mut r = r * (1.0 / largest);
    if r.determinant().abs() < 1e-24 {
        return DMat3::IDENTITY;
    }
    for _ in 0..60 {
        let inv_t = r.inverse().transpose();
        let next = (r + inv_t) * 0.5;
        let done = (next - r).to_cols_array().iter().all(|v| v.abs() < 1e-14);
        r = next;
        if done {
            break;
        }
    }
    r
}

/// A matrix's uniform scale: the mean length of its axes.
fn size(m: &DMat4) -> f64 {
    (m.x_axis.truncate().length() + m.y_axis.truncate().length() + m.z_axis.truncate().length())
        / 3.0
}

fn with_turn(r: DMat3, s: f64, t: DVec3) -> DMat4 {
    DMat4::from_cols(
        (r.x_axis * s).extend(0.0),
        (r.y_axis * s).extend(0.0),
        (r.z_axis * s).extend(0.0),
        t.extend(1.0),
    )
}

/// One animated value of a bone: its times and values (a turn in `xyzw`, else `xyz`).
#[derive(Debug, Clone)]
struct Channel {
    times: Vec<f64>,
    values: Vec<DVec4>,
    turn: bool,
}

impl Channel {
    /// The value at `t`, the channel looping over its own span: linear, a turn kept to the short
    /// way round and brought back to unit length.
    fn at(&self, t: f64) -> DVec4 {
        let ts = &self.times;
        if ts.len() == 1 {
            return self.values[0];
        }
        let span = (ts[ts.len() - 1] - ts[0]).max(1e-6);
        let t2 = ts[0] + (t - ts[0]).rem_euclid(span);
        let found = ts.partition_point(|v| *v < t2);
        let i = found.saturating_sub(1).min(ts.len() - 2);
        let f = ((t2 - ts[i]) / (ts[i + 1] - ts[i]).max(1e-9)).clamp(0.0, 1.0);
        let a = self.values[i];
        let mut b = self.values[i + 1];
        if self.turn && a.dot(b) < 0.0 {
            b = -b;
        }
        let v = a + (b - a) * f;
        if self.turn {
            v / v.length().max(1e-9)
        } else {
            v
        }
    }
}

/// Per body bone, its translation, turn and scale at one moment (none where nothing animates it).
type Moment = Vec<[Option<DVec4>; 3]>;

/// A clip on the AC2 skeleton: per body bone, its translation, turn and scale channels.
#[derive(Debug, Clone, Default)]
struct NativeClip {
    bones: Vec<[Option<Channel>; 3]>,
}

impl NativeClip {
    /// `a`'s keys, each channel closed at the clip's period: on its first keys for a loop, on its
    /// last for a clip played once (`held`). A scale that never leaves 1 is no channel.
    fn of(a: &KeyframeAnimation, held: bool) -> Self {
        let period = f64::from(a.period);
        let close = |times: &mut Vec<f64>, values: &mut Vec<DVec4>, turn: bool| {
            if turn {
                for i in 1..values.len() {
                    if values[i].dot(values[i - 1]) < 0.0 {
                        values[i] = -values[i];
                    }
                }
            }
            if times.len() > 1 && times[times.len() - 1] < period - 1e-4 {
                times.push(period);
                let last = values[values.len() - 1];
                let mut first = if held { last } else { values[0] };
                if turn && first.dot(last) < 0.0 {
                    first = -first;
                }
                values.push(first);
            }
        };
        let bones = a
            .bones
            .iter()
            .map(|b| {
                let mut out: [Option<Channel>; 3] = [None, None, None];
                if !b.offset.is_empty() {
                    let mut times: Vec<f64> = b.offset.iter().map(|k| f64::from(k.time)).collect();
                    let mut values: Vec<DVec4> = b
                        .offset
                        .iter()
                        .map(|k| cv(k.value.x, k.value.y, k.value.z).extend(0.0))
                        .collect();
                    close(&mut times, &mut values, false);
                    out[0] = Some(Channel {
                        times,
                        values,
                        turn: false,
                    });
                }
                if !b.rotation.is_empty() {
                    let mut times: Vec<f64> =
                        b.rotation.iter().map(|k| f64::from(k.time)).collect();
                    let mut values: Vec<DVec4> = b
                        .rotation
                        .iter()
                        .map(|k| {
                            DVec4::from_array(
                                cq(k.value.w, k.value.x, k.value.y, k.value.z).to_array(),
                            )
                        })
                        .collect();
                    close(&mut times, &mut values, true);
                    out[1] = Some(Channel {
                        times,
                        values,
                        turn: true,
                    });
                }
                let unit = |x: f32, y: f32, z: f32| {
                    [x, y, z].iter().all(|c| (f64::from(*c) - 1.0).abs() < 1e-4)
                };
                if !b.scale.is_empty()
                    && !b
                        .scale
                        .iter()
                        .all(|k| unit(k.value.x, k.value.y, k.value.z))
                {
                    let mut times: Vec<f64> = b.scale.iter().map(|k| f64::from(k.time)).collect();
                    let mut values: Vec<DVec4> = b
                        .scale
                        .iter()
                        .map(|k| cs(k.value.x, k.value.y, k.value.z).extend(0.0))
                        .collect();
                    close(&mut times, &mut values, false);
                    out[2] = Some(Channel {
                        times,
                        values,
                        turn: false,
                    });
                }
                out
            })
            .collect();
        Self { bones }
    }

    /// The clip's length: its latest key.
    fn period(&self) -> f64 {
        self.bones
            .iter()
            .flatten()
            .flatten()
            .map(|c| c.times[c.times.len() - 1])
            .fold(0.0, f64::max)
    }

    /// Each body bone's channels at `t`.
    fn at(&self, t: f64) -> Vec<[Option<DVec4>; 3]> {
        self.bones
            .iter()
            .map(|b| [0, 1, 2].map(|k| b[k].as_ref().map(|c| c.at(t))))
            .collect()
    }

    /// `self` and `other` played together at the same point of their cycles, `other` weighed by
    /// `share`: the cycle's length the weighted sum of theirs, keyed `RATE` times a second. Each
    /// bone's place, turn and scale are their weighted sums (a turn kept to the first's side and
    /// brought back to unit length); a bone one of them leaves alone stands in `rest` there.
    fn mixed(&self, other: &Self, share: f64, rest: &[(DVec3, DQuat, DVec3)]) -> Self {
        let (pa, pb) = (self.period(), other.period());
        let period = pa * (1.0 - share) + pb * share;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // LINT-OK: a cycle's length in keys is small and positive.
        let count = ((period * RATE).ceil() as usize + 1).max(2);
        let times: Vec<f64> = (0..count)
            .map(|k| period * k as f64 / (count - 1) as f64)
            .collect();
        let at =
            |clip: &Self, own: f64, phase: f64| clip.at((phase * own).min(own - 1e-4).max(0.0));
        let steps: Vec<(Moment, Moment)> = (0..count)
            .map(|k| {
                let phase = k as f64 / (count - 1) as f64;
                (at(self, pa, phase), at(other, pb, phase))
            })
            .collect();
        let bones = (0..self.bones.len().min(other.bones.len()))
            .map(|bi| {
                let (t0, q0, s0) =
                    rest.get(bi)
                        .copied()
                        .unwrap_or((DVec3::ZERO, DQuat::IDENTITY, DVec3::ONE));
                let rest_of = [
                    t0.extend(0.0),
                    DVec4::from_array(q0.to_array()),
                    s0.extend(0.0),
                ];
                [0, 1, 2].map(|path| {
                    let animated =
                        self.bones[bi][path].is_some() || other.bones[bi][path].is_some();
                    animated.then(|| {
                        let values = steps
                            .iter()
                            .map(|(a, b)| {
                                let va = a[bi][path].unwrap_or(rest_of[path]);
                                let mut vb = b[bi][path].unwrap_or(rest_of[path]);
                                if path == 1 && va.dot(vb) < 0.0 {
                                    vb = -vb;
                                }
                                let v = va * (1.0 - share) + vb * share;
                                if path == 1 {
                                    v / v.length().max(1e-9)
                                } else {
                                    v
                                }
                            })
                            .collect();
                        Channel {
                            times: times.clone(),
                            values,
                            turn: path == 1,
                        }
                    })
                })
            })
            .collect();
        Self { bones }
    }

    /// Each body bone's first keys.
    fn first(&self) -> Vec<[Option<DVec4>; 3]> {
        self.bones
            .iter()
            .map(|b| [0, 1, 2].map(|k| b[k].as_ref().map(|c| c.values[0])))
            .collect()
    }
}

/// One vertex of an AC2 mesh in glTF's axes, with up to four joints and their weights.
#[derive(Debug, Clone, Copy)]
struct NativeVertex {
    position: DVec3,
    normal: DVec3,
    uv: [f32; 2],
    joints: [usize; 4],
    weights: [f64; 4],
}

/// One mesh's triangles of one material.
#[derive(Debug, Clone)]
struct NativePrimitive {
    mesh: usize,
    indices: Vec<u32>,
    material: usize,
}

/// An AC2 entity's skeleton and skinned meshes, at rest, in glTF's axes.
#[derive(Debug, Clone, Default)]
struct Native {
    names: Vec<String>,
    parent: Vec<Option<usize>>,
    rest: Vec<(DVec3, DQuat, DVec3)>,
    inverse_binds: Vec<DMat4>,
    /// The body group's bones, by joint: the bones a clip's tracks move.
    body: Vec<usize>,
    meshes: Vec<Vec<NativeVertex>>,
    primitives: Vec<NativePrimitive>,
    /// Each material's picture list, by material instance in the order first drawn.
    materials: Vec<(u32, Option<u32>)>,
}

/// The joints' worlds with `over` (per body bone: translation, turn, scale) in place of their
/// rest.
fn worlds(n: &Native, over: Option<&[[Option<DVec4>; 3]]>) -> Vec<DMat4> {
    let mut local: Vec<(DVec3, DQuat, DVec3)> = n.rest.clone();
    if let Some(over) = over {
        for (bi, &j) in n.body.iter().enumerate() {
            if let Some(o) = over.get(bi) {
                if let Some(t) = o[0] {
                    local[j].0 = t.truncate();
                }
                if let Some(q) = o[1] {
                    local[j].1 = DQuat::from_xyzw(q.x, q.y, q.z, q.w);
                }
                if let Some(s) = o[2] {
                    local[j].2 = s.truncate();
                }
            }
        }
    }
    let mut out: Vec<Option<DMat4>> = vec![None; n.rest.len()];
    fn world_of(
        k: usize,
        n: &Native,
        local: &[(DVec3, DQuat, DVec3)],
        out: &mut Vec<Option<DMat4>>,
    ) -> DMat4 {
        if let Some(m) = out[k] {
            return m;
        }
        let (t, q, s) = local[k];
        let own = DMat4::from_scale_rotation_translation(s, q, t);
        let m = match n.parent[k] {
            Some(p) => world_of(p, n, local, out) * own,
            None => own,
        };
        out[k] = Some(m);
        m
    }
    (0..n.rest.len())
        .map(|k| world_of(k, n, &local, &mut out))
        .collect()
}

/// A mesh's vertices with their joints: each vertex's matrix weights (the bone index bytes and the
/// float weights after its pairs of picture coordinates), the four heaviest, made to sum to one;
/// a vertex with none is bound to its group's first bone.
fn mesh_vertices(m: &Mesh, joint_of: &dyn Fn(usize) -> usize) -> Option<Vec<NativeVertex>> {
    let format = m.vertices.format;
    let stride = auberean_assets::mesh::vertex_size(format);
    let stored = m.vertices.attributes()?;
    let matrices = ((format >> 16) & 0xFF) as usize;
    // The weights sit after everything else in the vertex, before any padding to a round size.
    let unpadded = {
        let mut n = if format & 0xE != 0 { 12 } else { 0 };
        n += 4 * match format & 0xE {
            6 => 1,
            8 => 2,
            10 => 3,
            12 => 4,
            14 => 5,
            _ => 0,
        };
        for (bit, size) in [
            (0x10, 12),
            (0x20, 4),
            (0x40, 4),
            (0x80, 4),
            (0x1000_0000, 12),
            (0x2000_0000, 12),
            (0x4000_0000, 12),
        ] {
            if format & bit != 0 {
                n += size;
            }
        }
        let pairs = match format & 0xF00 {
            p @ 0x100..=0x800 if p % 0x100 == 0 => (p >> 8) as usize,
            _ => 0,
        };
        n + 8 * pairs
    };
    let bytes = &m.vertices.bytes;
    Some(
        stored
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let at = i * stride + unpadded;
                let mut pairs: Vec<(f64, usize)> = if matrices == 0 {
                    vec![(1.0, 0)]
                } else {
                    (0..matrices)
                        .map(|k| {
                            let bone = usize::from(bytes[at + k]);
                            let o = at + matrices + 4 * k;
                            let w = f32::from_le_bytes([
                                bytes[o],
                                bytes[o + 1],
                                bytes[o + 2],
                                bytes[o + 3],
                            ]);
                            (f64::from(w), bone)
                        })
                        .collect()
                };
                pairs.sort_by(|a, b| b.0.total_cmp(&a.0).then(b.1.cmp(&a.1)));
                pairs.truncate(4);
                let total: f64 = pairs.iter().map(|p| p.0).sum();
                let total = if total == 0.0 { 1.0 } else { total };
                let mut joints = [0usize; 4];
                let mut weights = [0f64; 4];
                for (k, (w, b)) in pairs.iter().enumerate() {
                    joints[k] = joint_of(*b);
                    weights[k] = w / total;
                }
                NativeVertex {
                    position: cv(v.position.x, v.position.y, v.position.z),
                    normal: v.normal.map_or(cv(0.0, 0.0, 1.0), |n| cv(n.x, n.y, n.z)),
                    uv: v.uv.unwrap_or([0.0, 0.0]),
                    joints,
                    weights,
                }
            })
            .collect(),
    )
}

/// The entity's skeleton and meshes: its body group, and the head, beard, crest and tail groups
/// each hung from its parent group's connection point, each group's bones in its rest pose; each
/// mesh's most detailed level.
fn native(ac2: &Ac2, entity: u32) -> Result<Native, String> {
    let e: Entity = ac2
        .record(entity)
        .ok_or_else(|| format!("entity {entity:08X} will not read"))?;
    let vd_id = e.object.unwrap_or(0);
    let vd: VisualDesc = ac2
        .record(vd_id)
        .ok_or_else(|| format!("entity {entity:08X}: its description {vd_id:08X}"))?;
    let groups: Vec<(u32, auberean_assets::visual::PartGroup)> =
        vd.part_groups.clone().unwrap_or_default();
    let has = |k: u32| groups.iter().any(|(g, _)| *g == k);
    let body_key = if has(BODY_GROUP) {
        BODY_GROUP
    } else {
        groups
            .iter()
            .find(|(_, g)| g.setup.is_some())
            .map_or(BODY_GROUP, |(k, _)| *k)
    };
    let group = |k: u32| groups.iter().find(|(g, _)| *g == k).map(|(_, g)| g);
    let mut order: Vec<u32> = GROUPS.iter().copied().filter(|k| has(*k)).collect();
    if order.is_empty() {
        order.push(body_key);
    }
    // Parents before children.
    let mut placed: Vec<u32> = Vec::new();
    let mut pending = order.clone();
    while !pending.is_empty() {
        let before = pending.len();
        pending.retain(|&k| {
            let parent = group(k).and_then(|g| g.parent_key);
            let ready = k == body_key
                || parent.is_some_and(|p| placed.contains(&p))
                || !parent.is_some_and(|p| order.contains(&p));
            if ready {
                placed.push(k);
            }
            !ready
        });
        if pending.len() == before {
            break;
        }
    }
    let mut n = Native::default();
    let mut setups: BTreeMap<u32, Ac2Setup> = BTreeMap::new();
    let mut nodes_of: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for &k in &placed {
        let Some(g) = group(k) else { continue };
        let Some(setup_id) = g.setup else { continue };
        let s: Ac2Setup = ac2
            .record(setup_id.0)
            .ok_or_else(|| format!("entity {entity:08X}: setup {:08X}", setup_id.0))?;
        let rest = s
            .poses
            .iter()
            .find(|p| p.kind == 1)
            .or_else(|| s.poses.first())
            .ok_or_else(|| format!("setup {:08X} has no rest pose", setup_id.0))?
            .clone();
        let mut attach = None;
        if k != body_key {
            let parent = g.parent_key;
            let conn = parent.and_then(|p| {
                setups.get(&p).and_then(|ps| {
                    ps.connection_points
                        .iter()
                        .find(|c| Some(c.kind) == g.connection_point)
                })
            });
            match (parent.and_then(|p| nodes_of.get(&p)), conn) {
                (Some(nodes), Some(c)) => {
                    let Some(&node) = nodes.get(c.bone as usize) else {
                        continue;
                    };
                    let (o, r) = (c.transform.offset, c.transform.rotation);
                    attach = Some((node, cv(o.x, o.y, o.z), cq(r.w, r.x, r.y, r.z)));
                }
                _ => continue,
            }
        }
        let mut nodes = Vec::new();
        let mut wrest: Vec<(DVec3, DQuat)> = Vec::new();
        for (bi, b) in s.bones.iter().enumerate() {
            let r = rest.bones.get(bi).copied().unwrap_or_default();
            let (mut t, mut q) = (
                cv(r.offset.x, r.offset.y, r.offset.z),
                cq(r.rotation.w, r.rotation.x, r.rotation.y, r.rotation.z),
            );
            let sc = cs(r.scale.x, r.scale.y, r.scale.z);
            let parent_bone = (b.parent != u32::MAX).then_some(b.parent as usize);
            let parent_node;
            if let Some(pb) = parent_bone {
                let (pt, pq) = wrest[pb];
                wrest.push((pt + pq * t, pq * q));
                parent_node = Some(nodes[pb]);
            } else {
                wrest.push((t, q));
                parent_node = attach.map(|a| a.0);
                if let Some((_, at, aq)) = attach {
                    t = at + aq * t;
                    q = aq * q;
                }
            }
            let scale = if [sc.x, sc.y, sc.z].iter().any(|v| (v - 1.0).abs() > 1e-4) {
                sc
            } else {
                DVec3::ONE
            };
            n.names.push(format!(
                "Group{:02}_{bi:02}",
                placed.iter().position(|x| *x == k).unwrap_or(0)
            ));
            n.parent.push(parent_node);
            n.rest.push((t, q, scale));
            let (wt, wq) = wrest[bi];
            n.inverse_binds.push(trs(wt, wq, DVec3::ONE).inverse());
            nodes.push(n.rest.len() - 1);
        }
        if k == body_key {
            n.body = nodes.clone();
        }
        nodes_of.insert(k, nodes);
        setups.insert(k, s);
    }
    let mut material_index: BTreeMap<u32, usize> = BTreeMap::new();
    for &k in &placed {
        let (Some(s), Some(nodes)) = (setups.get(&k), nodes_of.get(&k)) else {
            continue;
        };
        for mesh in &s.meshes {
            let m: Mesh = ac2
                .record(mesh.0)
                .ok_or_else(|| format!("entity {entity:08X}: mesh {:08X}", mesh.0))?;
            if m.levels.is_empty() || m.vertices.count < 4 {
                continue;
            }
            let total = |l: &auberean_assets::mesh::DetailLevel| {
                l.fragments.iter().map(|f| f.indices.len()).sum::<usize>()
            };
            let mut best = 0;
            for (i, l) in m.levels.iter().enumerate().skip(1) {
                if total(l) > total(&m.levels[best]) {
                    best = i;
                }
            }
            let joint_of = |b: usize| nodes.get(b).copied().unwrap_or(nodes[0]);
            let Some(vertices) = mesh_vertices(&m, &joint_of) else {
                continue;
            };
            n.meshes.push(vertices);
            let mesh_at = n.meshes.len() - 1;
            for f in &m.levels[best].fragments {
                if f.indices.is_empty() {
                    continue;
                }
                let Some(&mi) = m.materials.get(f.material as usize) else {
                    continue;
                };
                let material = *material_index.entry(mi.0).or_insert_with(|| {
                    n.materials.push((mi.0, ac2.diffuse(mi.0)));
                    n.materials.len() - 1
                });
                n.primitives.push(NativePrimitive {
                    mesh: mesh_at,
                    indices: f.indices.iter().map(|i| u32::from(*i)).collect(),
                    material,
                });
            }
        }
    }
    if n.body.is_empty() {
        return Err(format!("entity {entity:08X} has no body"));
    }
    Ok(n)
}

/// The keyframe animation `id`.
fn keyframes(ac2: &Ac2, id: u32) -> Result<KeyframeAnimation, String> {
    let a: Animation = ac2
        .record(id)
        .ok_or_else(|| format!("animation {id:08X} will not read"))?;
    match a.animator {
        AnimationKind::Keyframes(k) => Ok(k),
        _ => Err(format!("animation {id:08X} is not keyframes")),
    }
}

/// One primitive of the made body: its vertices (only those its triangles use), triangles and
/// material.
#[derive(Debug, Clone)]
struct Primitive {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    joints: Vec<[u16; 4]>,
    weights: Vec<[f32; 4]>,
    dqs: Vec<f32>,
    indices: Vec<u32>,
    material: usize,
}

/// A made body: its bones (parents first) and primitives, and its clips.
#[derive(Debug, Clone)]
pub struct Made {
    /// Each bone's name, parent (by place) and world at bind (glTF's axes, as the pack has it).
    bones: Vec<(String, Option<usize>, DMat4)>,
    primitives: Vec<Primitive>,
    materials: Vec<(u32, Option<u32>)>,
    clips: Vec<Carried>,
}

/// Per step of a carried clip, each bone's local translation, turn and scale.
type Steps = Vec<Vec<(DVec3, DQuat, f64)>>;

/// One slot's clip on a made body: its times and steps, and its blow and stride.
#[derive(Debug, Clone)]
struct Carried {
    slot: String,
    times: Vec<f64>,
    steps: Steps,
    hit: Option<f64>,
    stride: Option<f64>,
}

/// The native joints, each after its parent.
fn parents_first(n: &Native) -> Vec<usize> {
    let mut order = Vec::new();
    let mut seen = vec![false; n.rest.len()];
    fn visit(k: usize, n: &Native, seen: &mut Vec<bool>, order: &mut Vec<usize>) {
        if seen[k] {
            return;
        }
        if let Some(p) = n.parent[k] {
            visit(p, n, seen, order);
        }
        seen[k] = true;
        order.push(k);
    }
    for k in 0..n.rest.len() {
        visit(k, n, &mut seen, &mut order);
    }
    order
}

/// The turn from glTF's axes as an AC2 bone's frame stands in them to the pack's bone frame:
/// a quarter turn about x.
fn frame_turn() -> DMat3 {
    DMat3::from_cols(
        DVec3::new(1.0, 0.0, 0.0),
        DVec3::new(0.0, 0.0, 1.0),
        DVec3::new(0.0, -1.0, 0.0),
    )
}

/// Body `b` from the player's files: posed in its bind clip's first keys, scaled and placed, with
/// every slot's clip carried onto it.
///
/// # Errors
/// A record on the way will not read.
#[allow(clippy::too_many_lines)]
pub fn make(ac2: &Ac2, b: &Body) -> Result<Made, String> {
    let n = native(ac2, b.entity)?;
    let bind_clip = NativeClip::of(&keyframes(ac2, b.bind)?, false);
    if bind_clip.bones.len() != n.body.len() {
        return Err(format!(
            "animation {:08X} moves {} bones, the body has {}",
            b.bind,
            bind_clip.bones.len(),
            n.body.len()
        ));
    }
    let first = worlds(&n, Some(&bind_clip.first()));
    let palette: Vec<DMat4> = first
        .iter()
        .zip(&n.inverse_binds)
        .map(|(w, ib)| *w * *ib)
        .collect();
    let order = parents_first(&n);
    let place_of: BTreeMap<usize, usize> = order.iter().enumerate().map(|(i, k)| (*k, i)).collect();
    let offset = DVec3::from_array(b.offset);
    let turn = frame_turn();
    let bones: Vec<(String, Option<usize>, DMat4)> = order
        .iter()
        .map(|&k| {
            let w = first[k];
            // A bone scaled to nothing stands unturned in the pack's axes too.
            let hidden = DMat3::from_mat4(w)
                .to_cols_array()
                .iter()
                .all(|v| *v == 0.0);
            let r = if hidden {
                DMat3::IDENTITY
            } else {
                rot(&w) * turn
            };
            let t = w.w_axis.truncate() * b.scale + offset;
            (
                n.names[k].clone(),
                n.parent[k].map(|p| place_of[&p]),
                with_turn(r, 1.0, t),
            )
        })
        .collect();
    let flat: Vec<bool> = (0..order.len()).map(|i| b.trunk.contains(&i)).collect();
    // Every mesh posed once in the bind clip's first keys.
    let posed: Vec<Vec<(DVec3, DVec3)>> = n
        .meshes
        .iter()
        .map(|vs| {
            vs.iter()
                .map(|v| {
                    let mut m = DMat4::ZERO;
                    for k in 0..4 {
                        m += palette[v.joints[k]] * v.weights[k];
                    }
                    let p = (m * v.position.extend(1.0)).truncate();
                    let nn = DMat3::from_mat4(m) * v.normal;
                    (p, nn / nn.length().max(1e-9))
                })
                .collect()
        })
        .collect();
    let primitives = n
        .primitives
        .iter()
        .map(|pr| {
            let vs = &n.meshes[pr.mesh];
            let ps = &posed[pr.mesh];
            let mut tri: Vec<[u32; 3]> = pr.indices.as_chunks::<3>().0.to_vec();
            // Wound the way most of its normals face.
            let against = tri
                .iter()
                .filter(|t| {
                    let [a, b2, c] = t.map(|i| ps[i as usize].0);
                    let face = (b2 - a).cross(c - a);
                    let normals = t.iter().map(|i| ps[*i as usize].1).sum::<DVec3>();
                    face.dot(normals) < 0.0
                })
                .count();
            if 2 * against > tri.len() {
                for t in &mut tri {
                    t.swap(1, 2);
                }
            }
            // Only the vertices its triangles use, in order.
            let mut used: Vec<u32> = tri.iter().flatten().copied().collect();
            used.sort_unstable();
            used.dedup();
            let at: BTreeMap<u32, u32> = used
                .iter()
                .enumerate()
                .map(|(i, v)| (*v, u32::try_from(i).unwrap_or(0)))
                .collect();
            let mut out = Primitive {
                positions: Vec::new(),
                normals: Vec::new(),
                uvs: Vec::new(),
                joints: Vec::new(),
                weights: Vec::new(),
                dqs: Vec::new(),
                indices: tri.iter().flatten().map(|i| at[i]).collect(),
                material: pr.material,
            };
            for &u in &used {
                let v = vs[u as usize];
                let (p, nn) = ps[u as usize];
                let p = p * b.scale + offset;
                out.positions.push(p.as_vec3().to_array());
                out.normals.push(nn.as_vec3().to_array());
                out.uvs.push(v.uv);
                let total: f64 = v.weights.iter().sum::<f64>().max(1e-9);
                let mut j = [0u16; 4];
                let mut w = [0f32; 4];
                let mut on_trunk = 0.0;
                for k in 0..4 {
                    let bone = place_of[&v.joints[k]];
                    j[k] = u16::try_from(bone + b.parts).unwrap_or(0);
                    let wk = v.weights[k] / total;
                    w[k] = single(wk);
                    if flat[bone] {
                        on_trunk += wk;
                    }
                }
                out.joints.push(j);
                out.weights.push(w);
                out.dqs.push(single((1.0 - on_trunk).clamp(0.0, 1.0)));
            }
            out
        })
        .collect();
    // The clips, carried bone by bone onto the made body.
    let ref_pose = worlds(&n, Some(&bind_clip.at(0.0)));
    let mut clips = Vec::new();
    for (slot, looped) in SLOTS {
        let Some(c) = b.clips.get(slot) else { continue };
        let mut clip = NativeClip::of(&keyframes(ac2, c.animation)?, !looped);
        if let (Some(mix), Some(share)) = (c.mix, c.share) {
            let other = NativeClip::of(&keyframes(ac2, mix)?, !looped);
            if other.bones.len() != n.body.len() {
                return Err(format!(
                    "animation {mix:08X} moves {} bones, the body has {}",
                    other.bones.len(),
                    n.body.len()
                ));
            }
            let rest: Vec<(DVec3, DQuat, DVec3)> = n.body.iter().map(|&j| n.rest[j]).collect();
            clip = clip.mixed(&other, share, &rest);
        }
        if clip.bones.len() != n.body.len() {
            return Err(format!(
                "animation {:08X} moves {} bones, the body has {}",
                c.animation,
                clip.bones.len(),
                n.body.len()
            ));
        }
        let (times, steps) = carry(&n, &order, &bones, &ref_pose, &clip, b.scale);
        clips.push(Carried {
            slot: slot.to_owned(),
            times,
            steps,
            hit: c.hit,
            stride: c.stride,
        });
    }
    Ok(Made {
        bones,
        primitives,
        materials: n.materials.clone(),
        clips,
    })
}

/// `clip` carried onto the made body: each bone turns by the change the clip makes from the bind
/// pose, keeps the made body's own bone lengths, and moves as far as AC2's moves it beyond where its
/// parent carries it, scaled. The times, and per step each bone's local translation, turn and scale.
fn carry(
    n: &Native,
    order: &[usize],
    bones: &[(String, Option<usize>, DMat4)],
    ref_pose: &[DMat4],
    clip: &NativeClip,
    scale: f64,
) -> (Vec<f64>, Steps) {
    let period = clip.period();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // LINT-OK: a clip's length in steps is small and positive.
    let count = ((period * RATE).ceil() as usize + 1).max(2);
    let mut times: Vec<f64> = (0..count)
        .map(|k| period * k as f64 / (count - 1) as f64)
        .collect();
    let bind_size: Vec<f64> = order.iter().map(|&k| size(&ref_pose[k])).collect();
    let local_t: Vec<DVec3> = bones
        .iter()
        .map(|(_, p, w)| match p {
            Some(p) => (bones[*p].2.inverse() * *w).w_axis.truncate(),
            None => w.w_axis.truncate(),
        })
        .collect();
    let mut steps = Vec::with_capacity(count);
    for &t in &times {
        let over = clip.at(t.min(period - 1e-4));
        let now = worlds(n, Some(&over));
        let mut world: Vec<DMat4> = Vec::with_capacity(order.len());
        let mut grown: Vec<f64> = Vec::with_capacity(order.len());
        let mut local = Vec::with_capacity(order.len());
        for (i, &k) in order.iter().enumerate() {
            let nw = now[k];
            let r = rot(&nw) * rot(&ref_pose[k]).transpose() * rot(&bones[i].2);
            let above = bones[i].1.map_or(DMat4::IDENTITY, |p| world[p]);
            let (nat_above, nat_above0) = match n.parent[k] {
                Some(p) => (now[p], ref_pose[p]),
                None => (DMat4::IDENTITY, DMat4::IDENTITY),
            };
            let own = if DMat3::from_mat4(nat_above0).determinant().abs() < 1e-9 {
                DVec3::ZERO
            } else {
                let carried =
                    nat_above * nat_above0.inverse() * ref_pose[k].w_axis.truncate().extend(1.0);
                nw.w_axis.truncate() - carried.truncate()
            };
            let s = if bind_size[i] > 1e-6 {
                (size(&nw) / bind_size[i]).max(MIN_SCALE)
            } else {
                1.0
            };
            let m = with_turn(
                r,
                s,
                (above * local_t[i].extend(1.0)).truncate() + own * scale,
            );
            let lm = above.inverse() * m;
            let q = DQuat::from_mat3(&rot(&lm)).normalize();
            let parent_grown = bones[i].1.map_or(1.0, |p| grown[p]);
            local.push((lm.w_axis.truncate(), q, s / parent_grown));
            world.push(m);
            grown.push(s);
        }
        steps.push(local);
    }
    if times[times.len() - 1] <= 0.0 {
        // A held pose: one step's length.
        times = vec![0.0, 1.0 / RATE];
        let first = steps[0].clone();
        steps = vec![first.clone(), first];
    }
    (times, steps)
}

/// A glTF document being written: its JSON and its buffer.
struct Writer {
    bin: Vec<u8>,
    views: Vec<Value>,
    accessors: Vec<Value>,
}

impl Writer {
    fn put(&mut self, bytes: &[u8], target: Option<u32>) -> usize {
        while !self.bin.len().is_multiple_of(4) {
            self.bin.push(0);
        }
        let mut v = json!({"buffer": 0, "byteOffset": self.bin.len(), "byteLength": bytes.len()});
        if let Some(t) = target {
            v["target"] = json!(t);
        }
        self.bin.extend_from_slice(bytes);
        self.views.push(v);
        self.views.len() - 1
    }

    fn floats<const N: usize>(
        &mut self,
        values: &[[f32; N]],
        kind: &str,
        target: Option<u32>,
        bounds: bool,
    ) -> usize {
        let bytes: Vec<u8> = values
            .iter()
            .flatten()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let view = self.put(&bytes, target);
        let mut a =
            json!({"bufferView": view, "componentType": 5126, "count": values.len(), "type": kind});
        if bounds && !values.is_empty() {
            let lo: Vec<f32> = (0..N)
                .map(|c| values.iter().map(|v| v[c]).fold(f32::INFINITY, f32::min))
                .collect();
            let hi: Vec<f32> = (0..N)
                .map(|c| {
                    values
                        .iter()
                        .map(|v| v[c])
                        .fold(f32::NEG_INFINITY, f32::max)
                })
                .collect();
            a["min"] = json!(lo);
            a["max"] = json!(hi);
        }
        self.accessors.push(a);
        self.accessors.len() - 1
    }

    fn shorts(&mut self, values: &[[u16; 4]]) -> usize {
        let bytes: Vec<u8> = values
            .iter()
            .flatten()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let view = self.put(&bytes, Some(34962));
        self.accessors.push(json!({"bufferView": view, "componentType": 5123, "count": values.len(), "type": "VEC4"}));
        self.accessors.len() - 1
    }

    fn indices(&mut self, values: &[u32]) -> usize {
        let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
        let view = self.put(&bytes, Some(34963));
        self.accessors.push(json!({"bufferView": view, "componentType": 5125, "count": values.len(), "type": "SCALAR"}));
        self.accessors.len() - 1
    }
}

/// A body's files: its glTF document (pictures named by the caller), its buffer, and the picture
/// lists its images draw in order.
#[derive(Debug)]
pub struct BodyFile {
    pub json: Value,
    pub bin: Vec<u8>,
    /// Each image's picture list.
    pub images: Vec<u32>,
}

/// The made body as the pack's glTF: its part joints, its bones, its mesh and its clips. Images
/// are named `tex_<list>.png`.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn body_file(made: &Made, b: &Body) -> BodyFile {
    let mut w = Writer {
        bin: Vec::new(),
        views: Vec::new(),
        accessors: Vec::new(),
    };
    let mut nodes: Vec<Value> = Vec::new();
    for k in 0..b.parts {
        let mut extras = json!({"part": k});
        if let Some(g) = b.gfxobj.as_ref().and_then(|g| g.get(k)) {
            if *g != 0 {
                extras["gfxobj"] = json!(g);
            }
        }
        nodes.push(json!({"name": format!("part_{k:02}"), "extras": extras}));
    }
    let first_bone = nodes.len();
    for (name, parent, world) in &made.bones {
        let local = match parent {
            Some(p) => made.bones[*p].2.inverse() * *world,
            None => *world,
        };
        let q = DQuat::from_mat3(&DMat3::from_mat4(local)).normalize();
        nodes.push(json!({
            "name": name,
            "translation": local.w_axis.truncate().to_array(),
            "rotation": q.to_array(),
        }));
    }
    for (i, (_, parent, _)) in made.bones.iter().enumerate() {
        if let Some(p) = parent {
            let children = nodes[first_bone + p]
                .as_object_mut()
                .map(|o| o.entry("children").or_insert_with(|| json!([])));
            if let Some(Value::Array(c)) = children {
                c.push(json!(first_bone + i));
            }
        }
    }
    let mut binds: Vec<[f32; 16]> = (0..b.parts)
        .map(|k| {
            if k == 0 {
                b.anchor.map(single)
            } else {
                DMat4::IDENTITY.as_mat4().to_cols_array()
            }
        })
        .collect();
    binds.extend(
        made.bones
            .iter()
            .map(|(_, _, m)| m.inverse().as_mat4().to_cols_array()),
    );
    let ibm = w.floats(&binds, "MAT4", None, false);
    let mut prims = Vec::new();
    for p in &made.primitives {
        let pos = w.floats(&p.positions, "VEC3", Some(34962), true);
        let nrm = w.floats(&p.normals, "VEC3", Some(34962), false);
        let uv = w.floats(&p.uvs, "VEC2", Some(34962), false);
        let j = w.shorts(&p.joints);
        let wt = w.floats(&p.weights, "VEC4", Some(34962), false);
        let dqs: Vec<[f32; 1]> = p.dqs.iter().map(|v| [*v]).collect();
        let dq = w.floats(&dqs, "SCALAR", Some(34962), false);
        let idx = w.indices(&p.indices);
        prims.push(json!({
            "attributes": {"POSITION": pos, "NORMAL": nrm, "TEXCOORD_0": uv, "JOINTS_0": j, "WEIGHTS_0": wt, "_DQS": dq},
            "indices": idx,
            "material": p.material,
        }));
    }
    let body_node = nodes.len();
    nodes.push(json!({"name": "body", "mesh": 0, "skin": 0}));
    let mut materials = Vec::new();
    let mut textures = Vec::new();
    let mut images: Vec<u32> = Vec::new();
    for (mi, list) in &made.materials {
        let mut m = json!({"name": format!("{mi:08X}"), "pbrMetallicRoughness": {"metallicFactor": 0.0, "roughnessFactor": 0.75}});
        if let Some(list) = list {
            images.push(*list);
            textures.push(json!({"sampler": 0, "source": images.len() - 1}));
            m["pbrMetallicRoughness"]["baseColorTexture"] = json!({"index": textures.len() - 1});
        }
        materials.push(m);
    }
    let mut animations = Vec::new();
    for Carried {
        slot: name,
        times,
        steps,
        hit,
        stride,
    } in &made.clips
    {
        let ts: Vec<[f32; 1]> = times.iter().map(|t| [single(*t)]).collect();
        let input = w.floats(&ts, "SCALAR", None, true);
        let mut samplers = Vec::new();
        let mut channels = Vec::new();
        for i in 0..made.bones.len() {
            let node = first_bone + i;
            let t: Vec<[f32; 3]> = steps.iter().map(|s| s[i].0.as_vec3().to_array()).collect();
            let mut q: Vec<DQuat> = steps.iter().map(|s| s[i].1).collect();
            for k in 1..q.len() {
                if q[k].dot(q[k - 1]) < 0.0 {
                    q[k] = -q[k];
                }
            }
            let q: Vec<[f32; 4]> = q.iter().map(|v| v.as_quat().to_array()).collect();
            let scaled = steps.iter().any(|s| (s[i].2 - 1.0).abs() > 1e-3);
            let mut put = |values: usize, path: &str| {
                samplers.push(json!({"input": input, "output": values, "interpolation": "LINEAR"}));
                channels.push(
                    json!({"sampler": samplers.len() - 1, "target": {"node": node, "path": path}}),
                );
            };
            let tv = w.floats(&t, "VEC3", None, false);
            put(tv, "translation");
            let qv = w.floats(&q, "VEC4", None, false);
            put(qv, "rotation");
            if scaled {
                let s: Vec<[f32; 3]> = steps.iter().map(|s| [single(s[i].2); 3]).collect();
                let sv = w.floats(&s, "VEC3", None, false);
                put(sv, "scale");
            }
        }
        let mut a = json!({"name": name, "samplers": samplers, "channels": channels});
        let mut extras = serde_json::Map::new();
        if let Some(h) = hit {
            extras.insert("hit".into(), json!(h));
        }
        if let Some(s) = stride {
            extras.insert("stride".into(), json!(s));
        }
        if !extras.is_empty() {
            a["extras"] = Value::Object(extras);
        }
        animations.push(a);
    }
    let roots: Vec<usize> = made
        .bones
        .iter()
        .enumerate()
        .filter(|(_, (_, p, _))| p.is_none())
        .map(|(i, _)| first_bone + i)
        .collect();
    let mut scene: Vec<usize> = (0..b.parts).collect();
    scene.extend(roots);
    scene.push(body_node);
    let joints: Vec<usize> = (0..first_bone + made.bones.len()).collect();
    let mut doc = json!({
        "asset": {"version": "2.0", "generator": "dereth-art ac2-look"},
        "scene": 0,
        "scenes": [{"nodes": scene}],
        "nodes": nodes,
        "meshes": [{"name": "body", "primitives": prims}],
        "skins": [{"joints": joints, "inverseBindMatrices": ibm}],
        "materials": materials,
        "accessors": w.accessors,
        "bufferViews": w.views,
        "buffers": [{"uri": "body.bin", "byteLength": w.bin.len()}],
        "animations": animations,
    });
    if !images.is_empty() {
        doc["images"] = json!(images
            .iter()
            .map(|l| json!({"uri": format!("tex_{l:08X}.png")}))
            .collect::<Vec<_>>());
        doc["textures"] = json!(textures);
        doc["samplers"] =
            json!([{"magFilter": 9729, "minFilter": 9987, "wrapS": 10497, "wrapT": 10497}]);
    }
    BodyFile {
        json: doc,
        bin: w.bin,
        images,
    }
}

/// A look's pictures: the file each of the body's changed images is drawn from, and its picture.
#[derive(Debug)]
pub struct LookFiles {
    /// The look's glTF document (the body's, its pictures swapped).
    pub json: Value,
    /// Each picture written beside it: its file name and picture.
    pub pictures: Vec<(String, Rgba)>,
}

/// Look `l` of a body whose file is `body`, its own pictures `own` (by image), and the body's
/// difference weights for a masked look.
///
/// # Errors
/// A picture will not read.
pub fn look_files(
    ac2: &Ac2,
    l: &Look,
    body: &BodyFile,
    own: &BTreeMap<u32, Rgba>,
    mask: Option<&(u32, u32, Vec<f32>)>,
) -> Result<LookFiles, String> {
    // The body's own skin: the first of its pictures by number.
    let skin_list = *body
        .images
        .iter()
        .min()
        .ok_or_else(|| format!("look {}: its body has no picture", l.name))?;
    let plain = own
        .get(&skin_list)
        .ok_or_else(|| format!("look {}: no picture {skin_list:08X}", l.name))?;
    let (w, h) = (plain.width, plain.height);
    let read = |list: u32| -> Result<Rgba, String> {
        ac2.picture(list, PICTURE_LIMIT)
            .ok_or_else(|| format!("look {}: picture {list:08X} will not read", l.name))
    };
    let skin = picture::resized(&picture::opaque(&read(l.picture)?), w, h);
    let mut main = skin.clone();
    if let Some(d) = l.detail {
        main = picture::detailed(&main, &read(d)?);
    }
    let weights = match (l.masked, mask) {
        (true, Some((mw, mh, m))) => Some(picture::weights_at(*mw, *mh, m, w, h)),
        _ => None,
    };
    main = if l.colorize {
        picture::colorized(&main, l.tint, None)
    } else {
        picture::multiplied(&main, l.tint, weights.as_deref())
    };
    let mut json = body.json.clone();
    let mut pictures = vec![(format!("{}.png", l.name), main)];
    let like = l.colorize.then(|| picture::colored(&skin, l.tint));
    let mut others = 0;
    if let Some(Value::Array(images)) = json.get_mut("images") {
        let mut done: BTreeMap<u32, String> = BTreeMap::new();
        for (k, im) in images.iter_mut().enumerate() {
            let list = body.images[k];
            if list == skin_list {
                im["uri"] = json!(format!("{}.png", l.name));
            } else if l.every {
                let file = match done.get(&list) {
                    Some(f) => f.clone(),
                    None => {
                        others += 1;
                        let file = format!("{}-{others}.png", l.name);
                        let p = own
                            .get(&list)
                            .ok_or_else(|| format!("look {}: no picture {list:08X}", l.name))?;
                        let p = if l.colorize {
                            picture::colorized(p, l.tint, like)
                        } else {
                            picture::multiplied(p, l.tint, None)
                        };
                        pictures.push((file.clone(), p));
                        done.insert(list, file.clone());
                        file
                    }
                };
                im["uri"] = json!(file);
            }
        }
    }
    if let Some(list) = l.normal {
        let normal = picture::opaque(&read(list)?);
        let file = format!("{}-normal.png", l.name);
        pictures.push((file.clone(), normal));
        if let Some(Value::Array(images)) = json.get_mut("images") {
            images.push(json!({"uri": file}));
            let source = images.len() - 1;
            if let Some(Value::Array(textures)) = json.get_mut("textures") {
                textures.push(json!({"source": source}));
                let index = textures.len() - 1;
                if let Some(Value::Array(materials)) = json.get_mut("materials") {
                    for m in materials {
                        m["normalTexture"] = json!({"index": index});
                    }
                }
            }
        }
    }
    Ok(LookFiles { json, pictures })
}

/// The `[[skin]]` rows of creature `c`, each body file named under `dir`.
#[must_use]
pub fn rows(c: &Creature, dir: &str, credit: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    for r in &c.skin {
        let Some(b) = c.body.iter().find(|b| b.folder == r.body) else {
            continue;
        };
        let file = r.look.as_deref().unwrap_or("body");
        let clips: Vec<String> = SLOTS
            .iter()
            .filter(|(s, _)| b.clips.contains_key(*s))
            .map(|(s, _)| format!("{s} = \"{s}\""))
            .collect();
        let _ = write!(
            out,
            "[[skin]]\nsetup = \"0x{:08X}\"\nfile = \"{dir}/{}/{file}.gltf\"\nclips = {{ {} }}\n",
            r.setup,
            b.folder,
            clips.join(", ")
        );
        if !r.palettes.is_empty() {
            let p: Vec<String> = r
                .palettes
                .iter()
                .map(|p| format!("\"0x{p:08X}\""))
                .collect();
            let _ = writeln!(out, "palettes = [{}]", p.join(", "));
        }
        let _ = writeln!(out, "{credit}");
    }
    out
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 look's creatures)
    use super::*;
    use auberean_assets::animation::{BoneTrack, VectorKey};

    fn key(x: f32, t: f32) -> VectorKey {
        let mut k = VectorKey::default();
        k.value.x = x;
        k.time = t;
        k
    }

    #[test]
    fn a_looping_clip_closes_on_its_first_keys_and_one_played_once_on_its_last() {
        let a = KeyframeAnimation {
            period: 2.0,
            bones: vec![BoneTrack {
                offset: vec![key(0.0, 0.0), key(1.0, 1.0)],
                ..BoneTrack::default()
            }],
            events: None,
        };
        let looped = NativeClip::of(&a, false);
        assert!((looped.period() - 2.0).abs() < 1e-12);
        assert!((looped.at(1.5)[0][0].expect("a place").x - 0.5).abs() < 1e-9);
        let held = NativeClip::of(&a, true);
        assert!((held.at(1.5)[0][0].expect("a place").x - 1.0).abs() < 1e-9);
        // Past its span it loops round.
        assert!((looped.at(2.5)[0][0].expect("a place").x - 0.5).abs() < 1e-9);
    }

    #[test]
    fn two_clips_mixed_play_at_one_phase_their_cycle_and_places_weighed_by_the_share() {
        let steady = |x: f32, period: f32| KeyframeAnimation {
            period,
            bones: vec![BoneTrack {
                offset: vec![key(x, 0.0), key(x, period / 2.0)],
                ..BoneTrack::default()
            }],
            events: None,
        };
        let a = NativeClip::of(&steady(0.0, 1.0), false);
        let b = NativeClip::of(&steady(2.0, 3.0), false);
        let rest = [(DVec3::ZERO, DQuat::IDENTITY, DVec3::ONE)];
        let m = a.mixed(&b, 0.25, &rest);
        assert!((m.period() - 1.5).abs() < 1e-9, "{}", m.period());
        let x = m.at(0.7)[0][0].expect("a place").x;
        assert!((x - 0.5).abs() < 1e-6, "{x}");
        // A turn neither moves stays unanimated.
        assert!(m.bones[0][1].is_none());
    }

    #[test]
    fn a_scale_that_never_leaves_one_is_no_channel() {
        let mut unit = VectorKey::default();
        unit.value.x = 1.0;
        unit.value.y = 1.0;
        unit.value.z = 1.0;
        let a = KeyframeAnimation {
            period: 1.0,
            bones: vec![BoneTrack {
                scale: vec![unit, VectorKey { time: 1.0, ..unit }],
                ..BoneTrack::default()
            }],
            events: None,
        };
        assert!(NativeClip::of(&a, false).bones[0][2].is_none());
    }

    #[test]
    fn a_matrix_turns_as_its_rotation_whatever_its_scale_and_one_scaled_to_nothing_not_at_all() {
        let q = DQuat::from_rotation_y(0.7);
        for s in [1.0, 3.0, 1e-7] {
            let m = DMat4::from_scale_rotation_translation(DVec3::splat(s), q, DVec3::X);
            let r = rot(&m);
            assert!(
                (DQuat::from_mat3(&r).dot(q).abs() - 1.0).abs() < 1e-9,
                "scale {s}"
            );
        }
        assert_eq!(rot(&DMat4::from_scale(DVec3::ZERO)), DMat3::IDENTITY);
        assert!((size(&DMat4::from_scale(DVec3::splat(2.0))) - 2.0).abs() < 1e-12);
    }

    #[test]
    fn a_creatures_rows_name_its_body_files_with_their_slots_in_order() {
        let t = super::super::tables::Tables::built_in().expect("the tables");
        let rabbit = t.creatures.named("rabbit").expect("the rabbit");
        let text = rows(rabbit, "creatures/rabbit", "license = \"x\"");
        assert_eq!(text.matches("[[skin]]").count(), rabbit.skin.len());
        let first = text
            .lines()
            .find(|l| l.starts_with("clips"))
            .expect("clips");
        assert!(first.starts_with("clips = { idle = \"idle\", "), "{first}");
        assert!(first.find("walk").expect("a walk") < first.find("die").expect("a death"));
        assert!(text.contains("file = \"creatures/rabbit/body/body.gltf\""));
        assert!(text.contains("palettes = [\"0x"));
    }
}
