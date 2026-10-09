//! The lamps of the outdoor world, found in the data for the high-fidelity presentation.
//!
//! Outdoors the ordinary renderer lights objects by the sun alone, so the lamp posts, the lanterns
//! hung on buildings and the torches by the gates give no light at night. The data says where
//! their light would come from in three ways, read here without changing anything:
//! - **authored lights:** a setup's own point lights, which only interiors use;
//! - **flames:** a setup's default script starting a particle emitter whose sprite is additive and
//!   fire-coloured (torches, braziers, the flame of a lamp);
//! - **glowing surfaces:** luminous surfaces (lamp glass, lantern panes), tinted from their
//!   texture, where nothing else says the object gives light.
//!
//! Each object id is looked at once; a block's lamps are its outdoor placements' sources placed by
//! their frames. The result is visual only: it is read by the presentation and by nothing else.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use dereth_assets::hook::HookData;
use dereth_assets::motion::PhysicsScript;
use dereth_assets::world::ParticleEmitterInfo;
use dereth_assets::{Decode, GfxObj, Setup, Surface};
use dereth_dat::{divine_type, DbType, RetailDatStore};
use dereth_primitives::{DataId, Frame};
use glam::Vec3;

fn g(v: dereth_primitives::Vec3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

fn p3(v: Vec3) -> dereth_primitives::Vec3 {
    dereth_primitives::Vec3::new(v.x, v.y, v.z)
}

/// Where a lamp's light was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum LampKind {
    /// The setup's own point light.
    Authored,
    /// A fire-coloured particle emitter the setup's default script starts.
    Flame,
    /// A warm luminous surface.
    Glow,
}

/// One light of an object, in the object's own space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LampSource {
    pub(crate) kind: LampKind,
    /// Where, object space (height on the third axis).
    pub(crate) offset: Vec3,
    /// Linear colour, brightest channel 1.
    pub(crate) color: [f32; 3],
    pub(crate) intensity: f32,
    /// Metres.
    pub(crate) falloff: f32,
    /// Whether it flickers: a flame, or an authored light over one.
    pub(crate) flicker: bool,
}

/// What one object id holds.
#[derive(Debug, Clone, Default)]
pub(crate) struct LampObject {
    pub(crate) sources: Vec<LampSource>,
    /// Why a candidate was turned down, for the survey.
    pub(crate) rejected: Vec<String>,
}

/// The surface type's additive bit.
const ADDITIVE: u32 = 0x1_0000;

fn decode<T: Decode>(store: &RetailDatStore, kind: DbType, id: DataId) -> Option<T> {
    let bytes = store.read_typed(kind, id).ok()?;
    T::decode_payload_in(store.era_of(id), id, &bytes).ok()
}

fn srgb(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        dereth_primitives::num::math::powf((c + 0.055) / 1.055, 2.4)
    }
}

/// `0xAARRGGBB` as linear colour.
fn argb_linear(argb: u32) -> [f32; 3] {
    let [_, r, g, b] = argb.to_be_bytes();
    [r, g, b].map(|v| srgb(f32::from(v) / 255.0))
}

/// Hue (degrees), saturation and value of a linear colour.
fn hsv(c: [f32; 3]) -> (f32, f32, f32) {
    let max = c[0].max(c[1]).max(c[2]);
    let min = c[0].min(c[1]).min(c[2]);
    let d = max - min;
    let s = if max > 0.0 { d / max } else { 0.0 };
    let h = if d <= 1e-6 {
        0.0
    } else if (max - c[0]).abs() < 1e-6 {
        60.0 * ((c[1] - c[2]) / d).rem_euclid(6.0)
    } else if (max - c[1]).abs() < 1e-6 {
        60.0 * ((c[2] - c[0]) / d + 2.0)
    } else {
        60.0 * ((c[0] - c[1]) / d + 4.0)
    };
    (h, s, max)
}

/// Whether a colour reads as lamp or fire light: orange to yellow, or a warm white. A deep red
/// is a magic glow or an ember far more often than a lamp, and is left out.
fn warm(c: [f32; 3]) -> bool {
    let (h, s, v) = hsv(c);
    if v <= 0.0 {
        return false;
    }
    (s < 0.18 && c[0] >= c[2]) || (10.0..=62.0).contains(&h)
}

/// `c` with its brightest channel at 1.
fn normalised(c: [f32; 3]) -> [f32; 3] {
    let m = c[0].max(c[1]).max(c[2]).max(1e-4);
    c.map(|v| v / m)
}

/// The colour a surface shows: the mean of its brighter texels, linear, or its solid colour.
fn surface_colour(
    store: &RetailDatStore,
    id: DataId,
    cache: &mut HashMap<DataId, Option<[f32; 3]>>,
) -> Option<[f32; 3]> {
    if let Some(c) = cache.get(&id) {
        return *c;
    }
    let found = (|| {
        let s: Surface = decode(store, DbType::Surface, id)?;
        if let Some(v) = s.color_value {
            return Some(argb_linear(v));
        }
        let pixels = crate::textures::TextureStore::new(store).bgra8(id).ok()?;
        let mut lum: Vec<(f32, [f32; 3])> = pixels
            .pixels
            .iter()
            .filter(|p| p[3] > 32)
            .map(|p| {
                let c = [
                    srgb(f32::from(p[2]) / 255.0),
                    srgb(f32::from(p[1]) / 255.0),
                    srgb(f32::from(p[0]) / 255.0),
                ];
                (0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2], c)
            })
            .collect();
        if lum.is_empty() {
            return None;
        }
        lum.sort_by(|a, b| b.0.total_cmp(&a.0));
        let n = (lum.len() / 4).max(1);
        let mut sum = [0.0f32; 3];
        for (_, c) in &lum[..n] {
            for k in 0..3 {
                sum[k] += c[k];
            }
        }
        #[allow(clippy::cast_precision_loss)]
        let n = n as f32;
        Some(sum.map(|v| v / n))
    })();
    cache.insert(id, found);
    found
}

/// A setup's parts with their resting frames and scales.
fn parts(setup: &Setup) -> Vec<(DataId, Frame, Vec3)> {
    let frames = setup
        .placement_frames
        .get(&0x65)
        .or_else(|| setup.placement_frames.get(&0))
        .map(|p| p.frames.clone())
        .unwrap_or_default();
    setup
        .parts
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let f = frames.get(i).copied().unwrap_or_default();
            let s = setup
                .default_scale
                .as_ref()
                .and_then(|d| d.get(i).copied())
                .map_or(Vec3::ONE, g);
            (*id, f, s)
        })
        .collect()
}

fn place(f: &Frame, s: Vec3, p: Vec3) -> Vec3 {
    g(dereth_physics::math::localtoglobal(f, p3(p * s)))
}

/// The glowing surfaces of one part: per luminous surface, its area-weighted centre and its area,
/// part space; and the part's whole area.
fn glowing(store: &RetailDatStore, gfx: &GfxObj) -> (Vec<(DataId, Vec3, f32, f32)>, f32) {
    let mut lum: HashMap<u16, (DataId, f32)> = HashMap::new();
    for (i, s) in gfx.surfaces.iter().enumerate() {
        if let Some(surf) = decode::<Surface>(store, DbType::Surface, *s) {
            if surf.luminosity > 0.0 {
                lum.insert(u16::try_from(i).unwrap_or(u16::MAX), (*s, surf.luminosity));
            }
        }
    }
    let verts = &gfx.vertex_array.vertices;
    let mut out: HashMap<u16, (Vec3, f32)> = HashMap::new();
    let mut total = 0.0;
    for p in &gfx.polygons {
        let pts: Vec<Vec3> = p
            .vertex_ids
            .iter()
            .filter_map(|v| verts.get(usize::from(*v)).map(|v| g(v.position)))
            .collect();
        if pts.len() < 3 {
            continue;
        }
        let mut area = 0.0;
        let mut centre = Vec3::ZERO;
        for k in 1..pts.len() - 1 {
            let a = pts[k] - pts[0];
            let b = pts[k + 1] - pts[0];
            let t = 0.5 * a.cross(b).length();
            area += t;
            centre += (pts[0] + pts[k] + pts[k + 1]) * (t / 3.0);
        }
        total += area;
        if lum.contains_key(&p.pos_surface) {
            let e = out.entry(p.pos_surface).or_insert((Vec3::ZERO, 0.0));
            e.0 += centre;
            e.1 += area;
        }
    }
    let list = out
        .into_iter()
        .filter(|(_, (_, a))| *a > 0.0)
        .map(|(k, (c, a))| {
            let (sid, l) = lum[&k];
            (sid, c * (1.0 / a), a, l)
        })
        .collect();
    (list, total)
}

/// The particle emitters a setup's default script starts: (emitter, part index, offset).
fn script_emitters(
    store: &RetailDatStore,
    setup: &Setup,
) -> Vec<(ParticleEmitterInfo, u32, Frame)> {
    let id = setup.default_script_id;
    if id.0 == 0 {
        return Vec::new();
    }
    let Some(script) = decode::<PhysicsScript>(store, DbType::PhysicsScript, id) else {
        return Vec::new();
    };
    script
        .script_data
        .iter()
        .filter_map(|step| match &step.hook.data {
            HookData::CreateParticle(c) => {
                let e: ParticleEmitterInfo =
                    decode(store, DbType::ParticleEmitter, c.emitter_info_id)?;
                Some((e, c.part_index, c.offset))
            }
            _ => None,
        })
        .collect()
}

/// Whether an emitter draws fire: an additive sprite of a warm, saturated colour.
fn flame_colour(
    store: &RetailDatStore,
    e: &ParticleEmitterInfo,
    colours: &mut HashMap<DataId, Option<[f32; 3]>>,
) -> Result<[f32; 3], String> {
    let gid = if e.hw_gfxobj_id.0 != 0 {
        e.hw_gfxobj_id
    } else {
        e.gfxobj_id
    };
    let gfx: GfxObj = decode(store, DbType::GfxObj, gid).ok_or("no sprite")?;
    let mut sum = [0.0f32; 3];
    let mut additive = false;
    for s in &gfx.surfaces {
        let Some(surf) = decode::<Surface>(store, DbType::Surface, *s) else {
            continue;
        };
        additive |= surf.surface_type & ADDITIVE != 0 || surf.luminosity > 0.0;
        if let Some(c) = surface_colour(store, *s, colours) {
            for k in 0..3 {
                sum[k] += c[k];
            }
        }
    }
    if !additive {
        return Err(format!("sprite {:08X} not additive", gid.0));
    }
    let (h, s, v) = hsv(sum);
    if v <= 0.0 || s < 0.3 || !(8.0..=60.0).contains(&h) {
        return Err(format!(
            "sprite {:08X} not fire-coloured (h {h:.0} s {s:.2})",
            gid.0
        ));
    }
    Ok(normalised(sum))
}

/// The middle of the top of an object's parts, a little below the top.
fn top_of(store: &RetailDatStore, parts: &[(DataId, Frame, Vec3)]) -> Option<Vec3> {
    let mut lo = Vec3::splat(f32::MAX);
    let mut hi = Vec3::splat(f32::MIN);
    for (gid, f, sc) in parts {
        let gfx = decode::<GfxObj>(store, DbType::GfxObj, *gid)?;
        for v in &gfx.vertex_array.vertices {
            let p = place(f, *sc, g(v.position));
            lo = lo.min(p);
            hi = hi.max(p);
        }
    }
    (hi.z >= lo.z).then(|| Vec3::new((lo.x + hi.x) * 0.5, (lo.y + hi.y) * 0.5, hi.z - 0.1))
}

/// What one object id gives light with, found from the data.
#[allow(clippy::too_many_lines)]
pub(crate) fn examine(store: &RetailDatStore, id: DataId) -> LampObject {
    let mut out = LampObject::default();
    let mut colours = HashMap::new();
    let setup = match divine_type(id) {
        Some(DbType::Setup) => decode::<Setup>(store, DbType::Setup, id),
        _ => None,
    };
    let part_list: Vec<(DataId, Frame, Vec3)> = match &setup {
        Some(s) => parts(s),
        None if divine_type(id) == Some(DbType::GfxObj) => {
            vec![(id, Frame::default(), Vec3::ONE)]
        }
        None => return out,
    };

    // Flames first: they also say whether an authored light should flicker.
    let mut flames = Vec::new();
    let mut emitter_at = Vec::new();
    if let Some(s) = &setup {
        for (e, part, offset) in script_emitters(store, s) {
            let at = part_list
                .get(part as usize)
                .map_or(g(offset.origin), |(_, f, sc)| {
                    place(f, *sc, g(offset.origin))
                });
            emitter_at.push(at);
            match flame_colour(store, &e, &mut colours) {
                Ok(c) => {
                    // The flame's light sits a little above where its particles are born.
                    let rise = e.a.z * e.max_a.max(e.min_a) * 0.15;
                    flames.push((at + Vec3::new(0.0, 0.0, rise.clamp(0.0, 0.4)), c));
                }
                Err(why) => out.rejected.push(why),
            }
        }
    }

    if let Some(s) = &setup {
        for l in s.lights.values() {
            let color = normalised(argb_linear(l.color_argb));
            if !warm(color) {
                out.rejected.push(format!(
                    "authored light not warm {:?}",
                    color.map(|c| (c * 100.0).round() / 100.0)
                ));
                continue;
            }
            // Many outdoor lights stand at the same placeholder offset, two metres out from the
            // object on both level axes: no lamp is there. Such a light is moved to where the
            // object's own flame is started, or else to the top of the object.
            let mut at = g(l.frame.origin);
            if (at - Vec3::new(-2.0, -2.0, 2.0)).length() < 1e-3 {
                at = emitter_at
                    .first()
                    .map(|p| *p + Vec3::new(0.0, 0.0, 0.15))
                    .or_else(|| top_of(store, &part_list))
                    .unwrap_or(at);
            }
            let near_flame = flames.iter().any(|(p, _)| (*p - at).length() < 1.5);
            out.sources.push(LampSource {
                kind: LampKind::Authored,
                offset: at,
                color,
                intensity: l.intensity,
                falloff: l.falloff,
                flicker: near_flame,
            });
        }
    }
    if out.sources.is_empty() {
        for (p, c) in &flames {
            out.sources.push(LampSource {
                kind: LampKind::Flame,
                offset: *p,
                color: *c,
                intensity: 1.0,
                falloff: 8.0,
                flicker: true,
            });
        }
    }
    if !out.sources.is_empty() {
        return out;
    }

    // Glowing surfaces, where nothing else says the object gives light.
    let mut total_area = 0.0;
    let mut glows: Vec<(Vec3, f32, [f32; 3])> = Vec::new();
    for (gid, f, sc) in &part_list {
        let Some(gfx) = decode::<GfxObj>(store, DbType::GfxObj, *gid) else {
            continue;
        };
        let (list, total) = glowing(store, &gfx);
        let scale = sc.x.abs().max(sc.y.abs()).max(sc.z.abs());
        total_area += total * scale * scale;
        for (sid, centre, area, lum) in list {
            let Some(c) = surface_colour(store, sid, &mut colours) else {
                continue;
            };
            let _ = lum;
            if !warm(c) {
                let (h, s, _) = hsv(c);
                out.rejected
                    .push(format!("glow {:08X} not warm (h {h:.0} s {s:.2})", sid.0));
                continue;
            }
            glows.push((place(f, *sc, centre), area * scale * scale, normalised(c)));
        }
    }
    if glows.is_empty() {
        return out;
    }
    let glow_area: f32 = glows.iter().map(|g| g.1).sum();
    if glow_area > 0.6 * total_area {
        out.rejected.push(format!(
            "glows over most of the object ({glow_area:.1} of {total_area:.1} m2)"
        ));
        return out;
    }
    if glow_area > 12.0 {
        out.rejected
            .push(format!("glowing area too large ({glow_area:.1} m2)"));
        return out;
    }
    // Merge glows closer than a metre: one lamp's panes are one light.
    let mut merged: Vec<(Vec3, f32, [f32; 3])> = Vec::new();
    for (p, a, c) in glows {
        if let Some(m) = merged
            .iter_mut()
            .find(|m| (m.0 * (1.0 / m.1) - p).length() < 1.0)
        {
            m.0 += p * a;
            m.1 += a;
            for (sum, v) in m.2.iter_mut().zip(c) {
                *sum += v * a;
            }
        } else {
            merged.push((p * a, a, c.map(|v| v * a)));
        }
    }
    for (p, a, c) in merged {
        let inv = 1.0 / a;
        out.sources.push(LampSource {
            kind: LampKind::Glow,
            offset: p * inv,
            color: normalised(c.map(|v| v * inv)),
            // A bigger pane throws more light, within reason.
            intensity: (0.6 + a.sqrt() * 0.8).min(1.6),
            falloff: (5.0 + a.sqrt() * 4.0).min(10.0),
            flicker: false,
        });
    }
    out
}

/// Every object id's lamps, looked at once per process.
fn cache() -> &'static Mutex<HashMap<DataId, Arc<LampObject>>> {
    static CACHE: OnceLock<Mutex<HashMap<DataId, Arc<LampObject>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// One object id's lamps, from the cache.
pub(crate) fn lamps_of(store: &RetailDatStore, id: DataId) -> Arc<LampObject> {
    if let Some(hit) = cache().lock().ok().and_then(|c| c.get(&id).cloned()) {
        return hit;
    }
    let found = Arc::new(examine(store, id));
    if let Ok(mut c) = cache().lock() {
        c.insert(id, Arc::clone(&found));
    }
    found
}

/// One placed lamp, block-local.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PlacedLamp {
    pub(crate) position: Vec3,
    pub(crate) source: LampSource,
    /// The object it belongs to.
    pub(crate) object: DataId,
}

/// A block's lamps: where they may be (its outdoor placements, kept from the bake, which reads
/// nothing for them), and the lamps themselves once they are looked for. They are looked for only
/// while the high-fidelity presentation draws them, so a client that never draws them reads
/// nothing of the data for them.
#[derive(Debug, Clone, Default)]
pub(crate) struct BlockLamps {
    /// The outdoor placements: `(object, frame, scale)`, block-local.
    sites: Vec<(DataId, Frame, f32)>,
    /// The lamps found at them, once looked for.
    placed: Option<Vec<PlacedLamp>>,
}

impl BlockLamps {
    /// The lamps of the placements `sites`, not looked for yet.
    pub(crate) fn at<'a>(sites: impl IntoIterator<Item = (DataId, &'a Frame, f32)>) -> Self {
        Self {
            sites: sites
                .into_iter()
                .map(|(id, frame, scale)| (id, *frame, scale))
                .collect(),
            placed: None,
        }
    }

    /// Look for the lamps, if they have not been looked for. Whether this call looked.
    pub(crate) fn place(&mut self, store: &RetailDatStore) -> bool {
        if self.placed.is_some() {
            return false;
        }
        self.placed = Some(place_lamps(
            store,
            self.sites
                .iter()
                .map(|(id, frame, scale)| (*id, frame, *scale)),
        ));
        true
    }

    /// The lamps found, none before they are looked for.
    pub(crate) fn placed(&self) -> &[PlacedLamp] {
        self.placed.as_deref().unwrap_or(&[])
    }

    /// Whether they have been looked for.
    pub(crate) fn looked(&self) -> bool {
        self.placed.is_some()
    }

    /// Let go of the lamps found, once they are not drawn: they are looked for again if they
    /// are drawn again.
    pub(crate) fn forget(&mut self) {
        self.placed = None;
    }

    /// How many placements the block keeps for its lamps.
    pub(crate) fn sites(&self) -> usize {
        self.sites.len()
    }
}

/// How many times [`place_lamps`] has looked at a block's placements, in this crate's tests.
#[cfg(test)]
pub(crate) static LOOKS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The lamps of a block's outdoor placements: `(object, frame, scale)`, block-local.
pub(crate) fn place_lamps<'a>(
    store: &RetailDatStore,
    placements: impl IntoIterator<Item = (DataId, &'a Frame, f32)>,
) -> Vec<PlacedLamp> {
    #[cfg(test)]
    LOOKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut out = Vec::new();
    for (id, frame, scale) in placements {
        let lamps = lamps_of(store, id);
        for s in &lamps.sources {
            out.push(PlacedLamp {
                position: g(dereth_physics::math::localtoglobal(
                    frame,
                    p3(s.offset * scale),
                )),
                source: LampSource {
                    falloff: s.falloff * scale.max(0.5),
                    ..*s
                },
                object: id,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A block's bake keeps where its lamps may be and reads nothing for them: no lamp is looked
    /// for until the presentation draws the lamps and the poll asks.
    ///
    /// Behaviour: hifi.lamps.no-lamp-is-looked-for-unless-the-lamps-are-drawn
    #[test]
    fn a_baked_block_reads_nothing_for_its_lamps_until_they_are_asked_for() {
        use std::sync::atomic::Ordering;
        let frame = Frame::default();
        let before = LOOKS.load(Ordering::Relaxed);
        let lamps = BlockLamps::at([
            (DataId(0x0100_0001), &frame, 1.0),
            (DataId(0x0200_0002), &frame, 2.0),
        ]);
        assert!(!lamps.looked());
        assert!(lamps.placed().is_empty());
        assert_eq!(lamps.sites(), 2);
        assert_eq!(
            LOOKS.load(Ordering::Relaxed),
            before,
            "a bake looked for lamps"
        );
        // Found lamps are let go of when they stop being drawn.
        let mut found = BlockLamps {
            placed: Some(Vec::new()),
            ..lamps
        };
        assert!(found.looked());
        found.forget();
        assert!(!found.looked());
        assert_eq!(found.sites(), 2, "the sites stay for the next time");
    }
}

#[cfg(test)]
mod survey {
    use super::*;

    /// Not a claim: prints what the finder makes of the town's and the world's outdoor objects.
    /// Behaviour: none (a survey printed for a person to read)
    #[test]
    #[ignore = "survey: prints, reads the retail dats"]
    fn survey_the_lamps_of_the_outdoor_world() {
        let Some(store) = dereth_dat::testing::open_store() else {
            eprintln!("no dats");
            return;
        };
        let mut uses: HashMap<DataId, (usize, usize)> = HashMap::new();
        let near = |lb: u16| {
            let (x, y) = (i32::from(lb >> 8), i32::from(lb & 0xFF));
            (x - 0xA9).abs() <= 3 && (y - 0xB4).abs() <= 3
        };
        for did in store.ids_of(DbType::Lbi) {
            #[allow(clippy::cast_possible_truncation)]
            let lb = (did.0 >> 16) as u16;
            let Some(info) =
                decode::<dereth_assets::world::LandblockInfo>(&store, DbType::Lbi, did)
            else {
                continue;
            };
            for o in &info.objects {
                let e = uses.entry(o.id).or_default();
                e.0 += 1;
                if near(lb) {
                    e.1 += 1;
                }
            }
            for b in &info.buildings {
                let e = uses.entry(b.id).or_default();
                e.0 += 1;
                if near(lb) {
                    e.1 += 1;
                }
            }
        }
        let mut scenery = std::collections::BTreeSet::new();
        for sid in store.ids_of(DbType::Scene) {
            if let Some(s) = decode::<dereth_assets::world::Scene>(&store, DbType::Scene, sid) {
                for o in s.objects {
                    scenery.insert(o.obj_id);
                }
            }
        }
        eprintln!("{} placed ids, {} scenery ids", uses.len(), scenery.len());
        let mut rows: Vec<(DataId, usize, usize, Arc<LampObject>)> = uses
            .iter()
            .map(|(id, (w, n))| (*id, *w, *n, lamps_of(&store, *id)))
            .collect();
        rows.sort_by_key(|r| std::cmp::Reverse(r.1));
        let mut by_kind: HashMap<LampKind, (usize, usize, usize)> = HashMap::new();
        for (_, w, n, l) in &rows {
            for k in [LampKind::Authored, LampKind::Flame, LampKind::Glow] {
                let c = l.sources.iter().filter(|s| s.kind == k).count();
                if c > 0 {
                    let e = by_kind.entry(k).or_default();
                    e.0 += 1;
                    e.1 += w * c;
                    e.2 += n * c;
                }
            }
        }
        eprintln!("by kind (ids, world lights, near-Holtburg lights): {by_kind:?}");
        for (id, w, n, l) in rows.iter().filter(|r| !r.3.sources.is_empty()).take(80) {
            let s: Vec<String> = l
                .sources
                .iter()
                .map(|s| {
                    format!(
                        "{:?} at ({:.2},{:.2},{:.2}) col ({:.2},{:.2},{:.2}) i {:.2} f {:.1}{}",
                        s.kind,
                        s.offset.x,
                        s.offset.y,
                        s.offset.z,
                        s.color[0],
                        s.color[1],
                        s.color[2],
                        s.intensity,
                        s.falloff,
                        if s.flicker { " flicker" } else { "" }
                    )
                })
                .collect();
            eprintln!("{:08X} x{w} (near {n}): {}", id.0, s.join("; "));
        }
        eprintln!("--- rejected (most used) ---");
        for (id, w, n, l) in rows.iter().filter(|r| !r.3.rejected.is_empty()).take(40) {
            eprintln!(
                "{:08X} x{w} (near {n}): {:?} kept {}",
                id.0,
                l.rejected,
                l.sources.len()
            );
        }
        eprintln!("--- scenery with lamps ---");
        for id in &scenery {
            let l = lamps_of(&store, *id);
            if !l.sources.is_empty() || !l.rejected.is_empty() {
                eprintln!(
                    "{:08X}: kept {:?} rejected {:?}",
                    id.0,
                    l.sources.iter().map(|s| s.kind).collect::<Vec<_>>(),
                    l.rejected
                );
            }
        }
    }
}

#[cfg(test)]
mod survey_town {
    use super::*;

    fn write_png(path: &std::path::Path, w: u32, h: u32, rgba: &[u8]) {
        let f = std::fs::File::create(path).expect("png");
        let mut e = png::Encoder::new(std::io::BufWriter::new(f), w, h);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header()
            .expect("header")
            .write_image_data(rgba)
            .expect("data");
    }

    /// Not a claim: the lamps of the blocks around the town, and pictures of their textures.
    /// Behaviour: none (a survey printed for a person to read)
    #[test]
    #[ignore = "survey: prints, reads the retail dats"]
    fn survey_the_town_lamps() {
        let Some(store) = dereth_dat::testing::open_store() else {
            return;
        };
        let out = std::env::var("LAMP_SURVEY_DIR").ok();
        let mut seen = std::collections::BTreeSet::new();
        let centre = std::env::var("LAMP_CENTER")
            .ok()
            .and_then(|v| u32::from_str_radix(v.trim(), 16).ok())
            .unwrap_or(0xA9B4);
        let r: i32 = if centre == 0xA9B4 { 3 } else { 0 };
        #[allow(clippy::cast_possible_wrap)]
        let (cx, cy) = ((centre >> 8) as i32, (centre & 0xFF) as i32);
        for dx in -r..=r {
            for dy in -r..=r {
                #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                let lb = (((cx + dx) as u32) << 8) | ((cy + dy) as u32);
                let did = DataId((lb << 16) | 0xFFFE);
                let Some(info) =
                    decode::<dereth_assets::world::LandblockInfo>(&store, DbType::Lbi, did)
                else {
                    continue;
                };
                let lamps = place_lamps(
                    &store,
                    info.objects
                        .iter()
                        .map(|o| (o.id, &o.frame, 1.0))
                        .chain(info.buildings.iter().map(|b| (b.id, &b.frame, 1.0))),
                );
                if lamps.is_empty() {
                    continue;
                }
                eprintln!("block {lb:04X}: {} lamps", lamps.len());
                for l in &lamps {
                    seen.insert(l.object);
                    eprintln!(
                        "  {:08X} {:?} at ({:.1},{:.1},{:.1}) f {:.1} i {:.0}",
                        l.object.0,
                        l.source.kind,
                        l.position.x,
                        l.position.y,
                        l.position.z,
                        l.source.falloff,
                        l.source.intensity
                    );
                }
            }
        }
        let Some(dir) = out else { return };
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let ts = crate::textures::TextureStore::new(&store);
        for id in seen {
            let Some(setup) = decode::<Setup>(&store, DbType::Setup, id) else {
                continue;
            };
            eprintln!(
                "{:08X}: {} parts {:?} script {:08X}",
                id.0,
                setup.parts.len(),
                setup
                    .parts
                    .iter()
                    .map(|p| format!("{:08X}", p.0))
                    .collect::<Vec<_>>(),
                setup.default_script_id.0
            );
            for (k, part) in setup.parts.iter().enumerate().take(3) {
                let Some(gfx) = decode::<GfxObj>(&store, DbType::GfxObj, *part) else {
                    continue;
                };
                for (j, s) in gfx.surfaces.iter().enumerate().take(4) {
                    if let Ok(px) = ts.bgra8(*s) {
                        let rgba: Vec<u8> = px
                            .pixels
                            .iter()
                            .flat_map(|p| [p[2], p[1], p[0], 255])
                            .collect();
                        write_png(
                            &dir.join(format!("{:08X}-p{k}-s{j}.png", id.0)),
                            px.width,
                            px.height,
                            &rgba,
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod survey_blocks {
    use super::*;

    /// Not a claim: the landblocks with the most outdoor lamps, and what lamps they hold.
    /// Behaviour: none (a survey printed for a person to read)
    #[test]
    #[ignore = "survey: prints, reads the retail dats"]
    fn survey_the_lampiest_blocks() {
        let Some(store) = dereth_dat::testing::open_store() else {
            return;
        };
        let mut rows = Vec::new();
        for did in store.ids_of(DbType::Lbi) {
            let Some(info) =
                decode::<dereth_assets::world::LandblockInfo>(&store, DbType::Lbi, did)
            else {
                continue;
            };
            let lamps = place_lamps(
                &store,
                info.objects
                    .iter()
                    .map(|o| (o.id, &o.frame, 1.0))
                    .chain(info.buildings.iter().map(|b| (b.id, &b.frame, 1.0))),
            );
            if lamps.len() >= 8 {
                rows.push((did.0 >> 16, lamps));
            }
        }
        rows.sort_by_key(|r| std::cmp::Reverse(r.1.len()));
        for (lb, lamps) in rows.iter().take(30) {
            let mut kinds: std::collections::BTreeMap<String, usize> =
                std::collections::BTreeMap::new();
            for l in lamps {
                *kinds
                    .entry(format!("{:08X} {:?}", l.object.0, l.source.kind))
                    .or_default() += 1;
            }
            eprintln!("{lb:04X}: {} lamps {:?}", lamps.len(), kinds);
            if let Some(l) = lamps.iter().find(|l| {
                matches!(
                    l.object.0,
                    0x0200_0338 | 0x0200_0337 | 0x0200_0AB9 | 0x0200_0354
                )
            }) {
                eprintln!(
                    "    e.g. {:08X} at ({:.1},{:.1},{:.1})",
                    l.object.0, l.position.x, l.position.y, l.position.z
                );
            }
        }
    }
}
