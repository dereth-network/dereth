//! What a creature table is written from: an AC2 entity's body (its setup, bones and meshes) and
//! its animation map, each key with the keyframe clips it plays and their lengths, ids and
//! numbers only; and the entities whose bodies are drawn with a given setup.

use std::fmt::Write as _;

use auberean_assets::animation::{AnimMap, Animation, AnimationKind, BehaviorTable, MotionDesc};
use auberean_assets::entity::Entity;
use auberean_assets::property::PropertyValue;
use auberean_assets::visual::VisualDesc;

use crate::ac2_look::source::Ac2;

/// The visual description an entity (or a visual description, `0x1F...`) is drawn with,
/// following parent entities, then parent descriptions to the first that has part groups.
pub(crate) fn description(ac2: &Ac2, entity: u32) -> Option<VisualDesc> {
    let mut id = entity;
    for _ in 0..8 {
        if id >> 24 != 0x47 {
            break;
        }
        let e: Entity = ac2.record(id)?;
        id = e.object?;
    }
    for _ in 0..8 {
        let vd: VisualDesc = ac2.record(id)?;
        // A description whose groups only lay appearance values over its parent's is drawn with
        // its parent's body.
        match (vd.part_groups.as_ref(), vd.parent) {
            (Some(g), _) if g.iter().any(|(_, g)| g.setup.is_some()) => return Some(vd),
            (_, Some(parent)) => id = parent.0,
            _ => return Some(vd),
        }
    }
    None
}

/// An animator's keyframe clips, a line each, indented by `depth`.
fn clips(ac2: &Ac2, id: u32, depth: usize, out: &mut String) {
    let pad = "  ".repeat(depth);
    let Some(a) = ac2.record::<Animation>(id) else {
        let _ = writeln!(out, "{pad}{id:08X} (will not read)");
        return;
    };
    if depth > 8 {
        return;
    }
    match a.animator {
        AnimationKind::Keyframes(k) => {
            let hit = k.events.as_ref().and_then(|e| {
                e.hooks
                    .iter()
                    .find(|h| h.kind == 0x4000_0002)
                    .map(|h| h.time)
            });
            let hit = hit.map_or(String::new(), |h| format!(" hit {h:.3}"));
            let _ = writeln!(
                out,
                "{pad}{id:08X} clip {:.2} s, {} bones{hit}",
                k.period,
                k.bones.len()
            );
        }
        AnimationKind::Switch {
            default, overrides, ..
        } => {
            let _ = writeln!(out, "{pad}{id:08X} switch, default:");
            clips(ac2, default.0, depth + 1, out);
            for (k, a) in overrides {
                let _ = writeln!(out, "{pad}  when {k:08X}:");
                clips(ac2, a.0, depth + 2, out);
            }
        }
        AnimationKind::Blend {
            sliders,
            animations,
        } => {
            let s: Vec<String> = sliders
                .iter()
                .map(|s| {
                    // Each tick is the slider's value at that cell, a float's bits.
                    let ticks: Vec<String> = s
                        .ticks
                        .iter()
                        .map(|t| format!("{}", f32::from_bits(*t)))
                        .collect();
                    format!("{:08X}[{}]", s.slider, ticks.join(" "))
                })
                .collect();
            let _ = writeln!(out, "{pad}{id:08X} blend over {}:", s.join(" "));
            for a in animations {
                clips(ac2, a.0, depth + 1, out);
            }
        }
        AnimationKind::AxisCube {
            origin, animations, ..
        } => {
            let _ = writeln!(out, "{pad}{id:08X} cube, origin:");
            clips(ac2, origin.0, depth + 1, out);
            for a in animations {
                clips(ac2, a.0, depth + 1, out);
            }
        }
        AnimationKind::Random { animations, .. } => {
            let _ = writeln!(out, "{pad}{id:08X} random:");
            for a in animations {
                clips(ac2, a.0, depth + 1, out);
            }
        }
        AnimationKind::Body { animation } => {
            let _ = writeln!(out, "{pad}{id:08X} body:");
            clips(ac2, animation.0, depth + 1, out);
        }
        AnimationKind::Look | AnimationKind::Angle { .. } => {
            let _ = writeln!(out, "{pad}{id:08X} (no clip)");
        }
    }
}

/// An entity's body groups (key, setup, bones, meshes, animation map) and each animation map key
/// with its clips.
///
/// # Errors
/// The entity or its description will not read.
pub fn entity(ac2: &Ac2, entity: u32) -> Result<String, String> {
    let vd =
        description(ac2, entity).ok_or_else(|| format!("entity {entity:08X} will not read"))?;
    let mut out = String::new();
    for (key, g) in vd.part_groups.unwrap_or_default() {
        let setup = g.setup.map_or(0, |s| s.0);
        let s: Option<auberean_assets::setup::Setup> = ac2.record(setup);
        let _ = writeln!(
            out,
            "group {key:08X}: setup {setup:08X} ({} bones, {} meshes, height {:.2}), animation map {:08X}",
            s.as_ref().map_or(0, |s| s.bones.len()),
            s.as_ref().map_or(0, |s| s.meshes.len()),
            s.as_ref().map_or(0.0, |s| s.height),
            g.anim_map.map_or(0, |a| a.0)
        );
        if let Some(s) = s.as_ref() {
            levels(ac2, s, &mut out);
        }
        if let Some(map) = g.anim_map.and_then(|a| ac2.record::<AnimMap>(a.0)) {
            let mut entries = map.entries.clone();
            entries.sort_by_key(|e| e.0);
            for (k, a) in entries {
                let _ = writeln!(out, "  key {k:08X}:");
                clips(ac2, a.0, 2, &mut out);
            }
        }
    }
    if let Some(table) = behavior_table(ac2, entity) {
        behaviors(&table, &mut out);
    }
    if let Some(m) = motion_desc(ac2, entity) {
        motions(&m, &mut out);
    }
    Ok(out)
}

/// A setup's detail: its three sets of detail distances (low, medium and high model detail), and
/// each mesh's levels of detail, each with its triangles, its degrade mode, the model details it is
/// marked for (a bit each, low first), how many vertices it draws that the fullest does not, and
/// how many of those are copies of one the fullest draws (the same place, normal and picture
/// coordinates).
fn levels(ac2: &Ac2, s: &auberean_assets::setup::Setup, out: &mut String) {
    for (k, d) in s.detail.iter().enumerate() {
        let _ = writeln!(
            out,
            "  detail {k}: visible {:.1}, degrade near {:.1} ideal {:.1} far {:.1}, materials high {:.1} medium {:.1}",
            d.visible, d.near_degrade, d.ideal_degrade, d.far_degrade, d.high_detail, d.medium_detail
        );
    }
    for m in &s.meshes {
        let Some(mesh) = ac2.record::<auberean_assets::mesh::Mesh>(m.0) else {
            let _ = writeln!(out, "  mesh {:08X} (will not read)", m.0);
            continue;
        };
        // The vertices the fullest level draws, against which each other level's are counted.
        let used = |l: &auberean_assets::mesh::DetailLevel| -> std::collections::BTreeSet<u16> {
            l.fragments
                .iter()
                .flat_map(|f| f.indices.iter().copied())
                .collect()
        };
        let fullest = mesh
            .levels
            .iter()
            .max_by_key(|l| l.fragments.iter().map(|f| f.indices.len()).sum::<usize>())
            .map(used)
            .unwrap_or_default();
        // A vertex as its place, normal and picture coordinates' bits, to find copies by.
        let stored = mesh.vertices.attributes().unwrap_or_default();
        let key = |i: u16| {
            stored.get(usize::from(i)).map(|v| {
                let n = v.normal.unwrap_or_default();
                let uv = v.uv.unwrap_or_default();
                [
                    v.position.x,
                    v.position.y,
                    v.position.z,
                    n.x,
                    n.y,
                    n.z,
                    uv[0],
                    uv[1],
                ]
                .map(f32::to_bits)
            })
        };
        let copies: std::collections::BTreeSet<[u32; 8]> =
            fullest.iter().filter_map(|&i| key(i)).collect();
        let levels: Vec<String> = mesh
            .levels
            .iter()
            .map(|l| {
                let triangles: usize = l.fragments.iter().map(|f| f.indices.len() / 3).sum();
                let own: Vec<u16> = used(l).difference(&fullest).copied().collect();
                let copied = own
                    .iter()
                    .filter(|&&i| key(i).is_some_and(|k| copies.contains(&k)))
                    .count();
                let own = if own.is_empty() {
                    String::new()
                } else {
                    format!(" +{} vertices of its own, {copied} copies", own.len())
                };
                format!("{triangles} ({:X}/{:X}){own}", l.degrade, l.detail)
            })
            .collect();
        let _ = writeln!(
            out,
            "  mesh {:08X}: {} vertices, levels {}",
            m.0,
            mesh.vertices.count,
            if levels.is_empty() {
                "none".to_owned()
            } else {
                levels.join(", ")
            }
        );
    }
}

/// The first record `pick` takes from an entity's chain of descriptions, nearest first.
fn first_named(ac2: &Ac2, entity: u32, pick: fn(&VisualDesc) -> Option<u32>) -> Option<u32> {
    let mut id = entity;
    for _ in 0..8 {
        if id >> 24 != 0x47 {
            break;
        }
        let e: Entity = ac2.record(id)?;
        id = e.object?;
    }
    for _ in 0..8 {
        let vd: VisualDesc = ac2.record(id)?;
        if let Some(t) = pick(&vd) {
            return Some(t);
        }
        id = vd.parent?.0;
    }
    None
}

/// The behavior table an entity is drawn with: the first its chain of descriptions names.
fn behavior_table(ac2: &Ac2, entity: u32) -> Option<BehaviorTable> {
    ac2.record(first_named(ac2, entity, |vd| {
        vd.behavior_table.map(|t| t.0)
    })?)
}

/// How an entity's body moves: the first motion description its chain names.
pub fn motion_desc(ac2: &Ac2, entity: u32) -> Option<MotionDesc> {
    ac2.record(first_named(ac2, entity, |vd| vd.motion_desc.map(|t| t.0))?)
}

/// A motion description's sets by element and mode: the fastest forward, and the turn rates
/// standing and running.
fn motions(m: &MotionDesc, out: &mut String) {
    let _ = writeln!(
        out,
        "motion sets (element mode: forward, turn standing, turn running):"
    );
    let mut set_of = m.set_of.clone();
    set_of.sort_by_key(|(k, _)| (k.element, k.mode));
    for (k, i) in set_of {
        let Some(s) = usize::try_from(i).ok().and_then(|i| m.sets.get(i)) else {
            continue;
        };
        let d = &s.default;
        let _ = writeln!(
            out,
            "  {:08X} {:08X}: set {i}, forward {:.2}, turn {:.2} standing, {:.2} running",
            k.element, k.mode, d.max_forward, d.still_turn_rate, d.running_turn_rate
        );
    }
}

/// A behavior table's combinations (element, mode and behavior to an animation key and layer)
/// by mode, and its mode changes with the behavior each plays.
fn behaviors(t: &BehaviorTable, out: &mut String) {
    let mut combos = t.combos.clone();
    combos.sort_by_key(|(k, _)| (k.element, k.mode, k.behavior));
    let _ = writeln!(out, "behaviors (element mode behavior -> key layer):");
    for (k, (key, layer)) in combos {
        let _ = writeln!(
            out,
            "  {:08X} {:08X} {:08X} -> key {key:08X} layer {layer:08X}",
            k.element, k.mode, k.behavior
        );
    }
    let mut changes = t.transitions.clone();
    changes.sort_by_key(|(c, _)| (c.from.element, c.from.mode, c.to.element, c.to.mode));
    let _ = writeln!(
        out,
        "mode changes (element mode -> element mode: behavior):"
    );
    for (c, e) in changes {
        let _ = writeln!(
            out,
            "  {:08X} {:08X} -> {:08X} {:08X}: {:08X}",
            c.from.element, c.from.mode, c.to.element, c.to.mode, e.behavior
        );
    }
    let mut ramps = t.ramp_times.clone();
    ramps.sort_by_key(|(c, _)| (c.from.element, c.from.mode, c.to.element, c.to.mode));
    for (c, s) in ramps {
        let _ = writeln!(
            out,
            "  ramp {:08X} {:08X} -> {:08X} {:08X}: {s:.2} s",
            c.from.element, c.from.mode, c.to.element, c.to.mode
        );
    }
}

/// The entities among `0x47000000`..`0x47008000` whose body group is drawn with `setup`.
#[must_use]
pub fn drawn_with(ac2: &Ac2, setup: u32) -> Vec<u32> {
    (0x4700_0000..0x4700_8000)
        .filter(|&e| {
            description(ac2, e).is_some_and(|vd| {
                vd.part_groups
                    .unwrap_or_default()
                    .iter()
                    .any(|(_, g)| g.setup.is_some_and(|s| s.0 == setup))
            })
        })
        .collect()
}

/// A template property naming the drawable entity it is drawn as.
const LOOK_PROPERTY: u32 = 0x4100_020B;
/// A template property giving the size it is drawn at.
const SCALE_PROPERTY: u32 = 0x4100_0219;
/// The entity kinds a template's chain walks: a class, and a template derived from another.
const CLASS: u32 = 0x4000_0002;
const DERIVED: u32 = 0x4000_0007;

/// A template's look: the drawable entity and the scale its chain of entities names first,
/// walking from the template through the templates it derives from to its class (or a class it
/// names); `None` when the chain names no drawable entity.
#[must_use]
pub fn template_look(ac2: &Ac2, template: u32) -> Option<(u32, Option<f32>)> {
    let (mut id, mut look, mut scale) = (template, None, None);
    for _ in 0..16 {
        let e: Entity = ac2.record(id)?;
        for p in e.properties.iter().flat_map(|c| c.iter()) {
            match (p.id, &p.value) {
                (LOOK_PROPERTY, PropertyValue::DataFile(d)) => {
                    look.get_or_insert(d.0);
                }
                (SCALE_PROPERTY, PropertyValue::Float(v)) => {
                    scale.get_or_insert(*v);
                }
                _ => {}
            }
        }
        match (e.kind, e.object) {
            (Some(DERIVED), Some(o)) => id = o,
            (Some(CLASS), Some(o)) if o >> 24 == 0x47 => id = o,
            _ => break,
        }
    }
    look.map(|l| (l, scale))
}

/// Every look of `base`'s body: each entity drawn with its setup, with its size, the meshes it
/// swaps and the pictures it lays on the body's materials (by the material's place in the body),
/// then each template drawn as it, with the scale the template names.
///
/// # Errors
/// The base entity or its body will not read.
pub fn looks(ac2: &Ac2, base: u32) -> Result<String, String> {
    let vd = description(ac2, base).ok_or_else(|| format!("entity {base:08X} will not read"))?;
    let setup = vd
        .part_groups
        .unwrap_or_default()
        .iter()
        .find_map(|(_, g)| g.setup)
        .ok_or_else(|| format!("entity {base:08X} has no body"))?
        .0;
    let materials = crate::ac2_look::creature::native(ac2, base)?.materials;
    let bodies = drawn_with(ac2, setup);
    let templates: Vec<(u32, u32, Option<f32>)> = (0x4700_0000..0x4700_8000)
        .filter_map(|t| {
            template_look(ac2, t)
                .filter(|(l, _)| bodies.contains(l))
                .map(|(l, s)| (t, l, s))
        })
        .collect();
    let mut out = String::new();
    let _ = writeln!(out, "body {setup:08X}, {} materials", materials.len());
    for &b in &bodies {
        match super::make::look_pictures(ac2, &materials, b) {
            Ok(l) => {
                let pictures: Vec<String> = l
                    .pictures
                    .iter()
                    .enumerate()
                    .filter_map(|(k, p)| {
                        p.map(|p| match p.bake {
                            Some(b) => format!(
                                "{k}:{:08X}x({:.2},{:.2},{:.2})+({:.2},{:.2},{:.2})",
                                p.list, b.a[0], b.a[1], b.a[2], b.b[0], b.b[1], b.b[2]
                            ),
                            None => format!("{k}:{:08X}", p.list),
                        })
                    })
                    .collect();
                let alpha = l.alpha.map_or(String::new(), |a| format!(" alpha {a:.2}"));
                let lost = if l.lost.is_empty() {
                    String::new()
                } else {
                    format!(" (not carried: {})", l.lost.join(", "))
                };
                let _ = writeln!(
                    out,
                    "look {b:08X} scale {:.3} swaps {} pictures {}{alpha}{lost}",
                    l.scale,
                    l.swapped,
                    if pictures.is_empty() {
                        "own".to_owned()
                    } else {
                        pictures.join(" ")
                    }
                );
            }
            Err(e) => {
                let _ = writeln!(out, "look {b:08X} ({e})");
            }
        }
        for (t, _, s) in templates.iter().filter(|(_, l, _)| *l == b) {
            let s = s.map_or(String::new(), |s| format!(" scale {s:.3}"));
            let _ = writeln!(out, "  template {t:08X}{s}");
        }
    }
    Ok(out)
}
