//! Free functions that turn the decoded dat records (`dereth_assets`) into the animation layer's
//! runtime shapes (`dereth_animation::data`).
//!
//! This is the only place the two meet: `dereth-animation` does not depend on the decoders, and the
//! decoders know nothing of the runtime. Nothing here interprets: it drops the record ids, resolves
//! the two-meaning `MotionData` header bytes into the runtime shape, keys the motion-table lists by
//! their motion, converts the decoded hooks (dropping the ones the client drops), sorts a physics
//! script's timeline the way the client does, and runs a particle emitter's end-of-init step. The
//! small records the two sides share (`AnimData`, `LightInfo`, `LocationEntry`, `ScriptAndMod`,
//! `GfxObjInfo`, `Sphere`, `CylSphere`) are the same `dereth_primitives` values on both, so they are
//! copied as they are.

use std::collections::BTreeMap;

use dereth_animation::command::MotionCommand;
use dereth_animation::data::{
    AnimFrame, AnimationData, DegradeInfo, MotionData, MotionTableData, ParticleEmitterInfo,
    ParticleType, PhysicsScriptData, PhysicsScriptTableData, ScriptStep, SetupData, Sphere,
};
use dereth_animation::hooks::{add_to_list, AnimHook, HookKind};
use dereth_animation::script::sort_script_data;
use dereth_primitives::{DataId, Vec3};

/// The unpacked motion data → the runtime shape.
///
/// `bitfield` is header byte +5 and the velocity/omega presence mask is byte +6.
/// The decoder keeps them separate and so does this: `bitfield` survives with both of its
/// *runtime* meanings, and the presence mask has already done its job by deciding whether the
/// vectors were read at all.
#[must_use]
pub fn motion_data(md: &dereth_assets::motion::MotionData) -> MotionData {
    MotionData {
        anims: md.anims.clone(),
        bitfield: md.bitfield,
        velocity: md.velocity.unwrap_or(Vec3::ZERO),
        omega: md.omega.unwrap_or(Vec3::ZERO),
    }
}

/// The unpacked motion table → the runtime shape.
///
/// Each `MotionData` carries its own key, which is what the inner link hash is keyed by (the
/// **full** destination command) and what the cycle/modifier hashes are keyed by (the combined
/// `(style << 16) | (motion & 0xFFFFFF)`).
#[must_use]
pub fn motion_table(mt: &dereth_assets::MotionTable) -> MotionTableData {
    let mut cycles = BTreeMap::new();
    for md in &mt.cycles {
        cycles.insert(md.key, motion_data(md));
    }
    let mut modifiers = BTreeMap::new();
    for md in &mt.modifiers {
        modifiers.insert(md.key, motion_data(md));
    }
    let mut links = BTreeMap::new();
    for (outer, group) in &mt.links {
        let mut inner = BTreeMap::new();
        for md in group {
            inner.insert(md.key, motion_data(md));
        }
        links.insert(*outer, inner);
    }
    MotionTableData {
        default_style: MotionCommand(mt.default_style),
        style_defaults: mt
            .style_defaults
            .iter()
            .map(|(k, v)| (*k, MotionCommand(*v)))
            .collect(),
        cycles,
        modifiers,
        links,
    }
}

/// Converts to the runtime shape, hooks in file order with type 4 dropped.
#[must_use]
pub fn animation(a: &dereth_assets::Animation) -> AnimationData {
    AnimationData {
        num_frames: a.num_frames,
        num_parts: a.num_parts,
        pos_frames: a.pos_frames.clone(),
        part_frames: a
            .part_frames
            .iter()
            .map(|f| AnimFrame {
                frames: f.frames.clone(),
                hooks: hooks_from_decoded(&f.hooks),
            })
            .collect(),
        has_hooks: a.has_hooks,
    }
}

/// The unpacked setup → the animation half.
///
/// A zero `DataId` is the client's `INVALID_DID`, so it becomes `None` rather than `Some(0)`:
/// Setup conversion tests each of the five ids for non-zero before using it.
#[must_use]
pub fn setup(s: &dereth_assets::Setup) -> SetupData {
    let did = |d: DataId| if d.0 == 0 { None } else { Some(d) };
    SetupData {
        parts: s.parts.clone(),
        placement_frames: s
            .placement_frames
            .iter()
            .map(|(k, p)| {
                (
                    *k,
                    AnimFrame {
                        frames: p.frames.clone(),
                        hooks: hooks_from_decoded(&p.hooks),
                    },
                )
            })
            .collect(),
        parent_index: s.parent_index.clone(),
        default_scale: s.default_scale.clone(),
        allow_free_heading: s.allow_free_heading,
        has_physics_bsp: s.has_physics_bsp,
        height: s.height,
        radius: s.radius,
        step_up_height: s.step_up_height,
        step_down_height: s.step_down_height,
        sorting_sphere: s.sorting_sphere,
        selection_sphere: s.selection_sphere,
        spheres: s.spheres.clone(),
        cylspheres: s.cylspheres.clone(),
        lights: s.lights.clone(),
        holding_locations: s.holding_locations.clone(),
        connection_points: s.connection_points.clone(),
        default_animation: did(s.default_anim_id),
        default_script: did(s.default_script_id),
        default_motion_table: did(s.default_mtable_id),
        default_sound_table: did(s.default_stable_id),
        default_phs_table: did(s.default_phstable_id),
    }
}

/// The unpacked physics script → the runtime timeline.
///
/// **The sort happens here**, exactly where the client does it: `UnPack` hands `script_data` to the
/// CRT `qsort` with a start-time comparator, then sets `length` from the **last**
/// entry's `start_time`. A hook that fails to convert (type 4, or an unknown type) is dropped, and
/// its timeline slot goes with it. The decoded hook has no supported runtime conversion here.
/// The observed client would retain a null hook and try to call it during playback; this model
/// cannot represent that state, so it drops the entry. No shipped script contains a type 4.
#[must_use]
pub fn physics_script(s: &dereth_assets::PhysicsScript) -> PhysicsScriptData {
    let mut steps: Vec<ScriptStep> = s
        .script_data
        .iter()
        .filter_map(|st| {
            hook_from_decoded(&st.hook).map(|hook| ScriptStep {
                start_time: st.start_time,
                hook,
            })
        })
        .collect();
    sort_script_data(&mut steps);
    let length = steps.last().map_or(0.0, |s| s.start_time);
    PhysicsScriptData { steps, length }
}

/// Converts to the runtime table. Rows stay in **file order**.
#[must_use]
pub fn physics_script_table(t: &dereth_assets::PhysicsScriptTable) -> PhysicsScriptTableData {
    PhysicsScriptTableData {
        scripts: t.script_table.clone(),
    }
}

/// Converts to the runtime LOD table.
#[must_use]
pub fn degrade_info(d: &dereth_assets::GfxObjDegradeInfo) -> DegradeInfo {
    DegradeInfo {
        degrades: d.degrades.clone(),
    }
}

/// Converts to the runtime shape, with `InitEnd` already run.
#[must_use]
pub fn emitter_info(i: &dereth_assets::world::ParticleEmitterInfo) -> ParticleEmitterInfo {
    let mut out = ParticleEmitterInfo {
        emitter_type: i.emitter_type,
        particle_type: ParticleType::from_i32(i.particle_type),
        gfxobj_id: i.gfxobj_id,
        hw_gfxobj_id: i.hw_gfxobj_id,
        birthrate: i.birthrate,
        max_particles: i.max_particles,
        initial_particles: i.initial_particles,
        total_particles: i.total_particles,
        total_seconds: i.total_seconds,
        lifespan: i.lifespan,
        lifespan_rand: i.lifespan_rand,
        offset_dir: i.offset_dir,
        min_offset: i.min_offset,
        max_offset: i.max_offset,
        a: i.a,
        min_a: i.min_a,
        max_a: i.max_a,
        b: i.b,
        min_b: i.min_b,
        max_b: i.max_b,
        c: i.c,
        min_c: i.min_c,
        max_c: i.max_c,
        start_scale: i.start_scale,
        final_scale: i.final_scale,
        scale_rand: i.scale_rand,
        start_trans: i.start_trans,
        final_trans: i.final_trans,
        trans_rand: i.trans_rand,
        is_parent_local: i.is_parent_local,
        sorting_sphere: Sphere::default(),
    };
    out.init_end();
    out
}

/// Turn one decoded `dereth_assets::AnimHook` into the runtime form.
///
/// Returns `None` for type 4 (`AnimationDone`) and for any type this build does not construct —
/// the decoded-hook switch's `default` arm, which aligns the pointer and returns
/// `NULL`. **This is a drop, not an error**: an animation frame whose hook list contains a type 4
/// keeps its other hooks, in file order, with the type 4 simply absent.
#[must_use]
pub fn hook_from_decoded(h: &dereth_assets::AnimHook) -> Option<AnimHook> {
    use dereth_assets::HookData as D;
    let kind = match (h.hook_type, &h.data) {
        (0, D::None) => HookKind::NoOp,
        (1, D::Sound { sound_id }) => HookKind::Sound { gid: *sound_id },
        (2, D::SoundTable { sound_type }) => HookKind::SoundTable {
            sound_type: *sound_type,
        },
        (3, D::Attack(value)) => HookKind::Attack(*value),
        // Animation completion is generated by the sequence, not decoded from a hook.
        (4, _) => return None,
        (
            5,
            D::ReplaceObject {
                part_index,
                part_id,
            },
        ) => HookKind::ReplaceObject {
            part_index: u32::from(*part_index),
            part_id: *part_id,
        },
        (6, D::Ethereal { ethereal }) => HookKind::Ethereal {
            ethereal: *ethereal,
        },
        (7, D::PartRamp(value)) => HookKind::TransparentPart(*value),
        (8, D::Ramp(value)) => HookKind::Luminous(*value),
        (9, D::PartRamp(value)) => HookKind::LuminousPart(*value),
        (10, D::Ramp(value)) => HookKind::Diffuse(*value),
        (11, D::PartRamp(value)) => HookKind::DiffusePart(*value),
        (12, D::Scale(value)) => HookKind::Scale(*value),
        (13, D::CreateParticle(value)) => HookKind::CreateParticle(*value),
        (14, D::Particle { emitter_id }) => HookKind::DestroyParticle {
            emitter_id: *emitter_id,
        },
        (15, D::Particle { emitter_id }) => HookKind::StopParticle {
            emitter_id: *emitter_id,
        },
        (16, D::NoDraw { nodraw }) => HookKind::NoDraw {
            // The decoded unsigned field becomes a signed flag; out-of-range values mean on.
            nodraw: i32::try_from(*nodraw).unwrap_or(1),
        },
        (17, D::None) => HookKind::DefaultScript,
        (18, D::DefaultScriptPart { part_index }) => HookKind::DefaultScriptPart {
            part_index: *part_index,
        },
        (19, D::CallPes(value)) => HookKind::CallPes(*value),
        (20, D::Ramp(value)) => HookKind::Transparent(*value),
        (21, D::SoundTweaked(value)) => HookKind::SoundTweaked(*value),
        (22, D::SetOmega { axis }) => HookKind::SetOmega { axis: *axis },
        (23, D::TextureVelocity(value)) => HookKind::TextureVelocity(*value),
        (24, D::TextureVelocityPart(value)) => HookKind::TextureVelocityPart(*value),
        (25, D::SetLight { lights_on }) => HookKind::SetLight {
            lights_on: *lights_on,
        },
        (26, D::CreateParticle(value)) => HookKind::CreateBlockingParticle(*value),
        // Unknown types and mismatched payloads do not construct an executable hook.
        _ => return None,
    };
    Some(AnimHook {
        direction: h.direction,
        kind,
    })
}

/// Convert a decoded hook list, dropping the hooks the client's hook unpacker drops, in file order.
#[must_use]
pub fn hooks_from_decoded(hooks: &[dereth_assets::AnimHook]) -> Vec<AnimHook> {
    let mut out = Vec::with_capacity(hooks.len());
    for h in hooks {
        if let Some(h) = hook_from_decoded(h) {
            add_to_list(&mut out, h);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_assets::HookData as D;
    use dereth_primitives::Frame;

    fn decoded(hook_type: u32, direction: i32, data: D) -> dereth_assets::AnimHook {
        dereth_assets::AnimHook {
            hook_type,
            direction,
            data,
        }
    }

    /// ORACLE: the recovered animation-hook behavior section 2, the
    /// 27-row catalogue with each type's payload and target.
    #[test]
    fn every_catalogued_type_converts_and_reports_its_own_number() {
        let f = Frame::default();
        let cases: Vec<dereth_assets::AnimHook> = vec![
            decoded(0, 0, D::None),
            decoded(
                1,
                0,
                D::Sound {
                    sound_id: DataId(0x0A00_0001),
                },
            ),
            decoded(2, 0, D::SoundTable { sound_type: 3 }),
            decoded(
                3,
                0,
                D::Attack(dereth_primitives::records::AttackCone {
                    part_index: 1,
                    left: (0.0, 1.0),
                    right: (1.0, 0.0),
                    radius: 0.5,
                    height: 1.5,
                }),
            ),
            decoded(
                5,
                0,
                D::ReplaceObject {
                    part_index: 2,
                    part_id: DataId(0x0100_0002),
                },
            ),
            decoded(6, 0, D::Ethereal { ethereal: 1 }),
            decoded(
                7,
                0,
                D::PartRamp(dereth_primitives::records::HookPartRamp {
                    part: 0,
                    start: 0.0,
                    end: 1.0,
                    time: 0.5,
                }),
            ),
            decoded(
                8,
                0,
                D::Ramp(dereth_primitives::records::HookRamp {
                    start: 0.0,
                    end: 1.0,
                    time: 0.5,
                }),
            ),
            decoded(
                9,
                0,
                D::PartRamp(dereth_primitives::records::HookPartRamp {
                    part: 0,
                    start: 0.0,
                    end: 1.0,
                    time: 0.5,
                }),
            ),
            decoded(
                10,
                0,
                D::Ramp(dereth_primitives::records::HookRamp {
                    start: 0.0,
                    end: 1.0,
                    time: 0.5,
                }),
            ),
            decoded(
                11,
                0,
                D::PartRamp(dereth_primitives::records::HookPartRamp {
                    part: 0,
                    start: 0.0,
                    end: 1.0,
                    time: 0.5,
                }),
            ),
            decoded(
                12,
                0,
                D::Scale(dereth_primitives::records::HookScale {
                    end: 2.0,
                    time: 1.0,
                }),
            ),
            decoded(
                13,
                0,
                D::CreateParticle(dereth_primitives::records::HookCreateParticle {
                    emitter_info_id: DataId(0x3200_0001),
                    part_index: 0,
                    offset: f,
                    emitter_id: 7,
                }),
            ),
            decoded(14, 0, D::Particle { emitter_id: 7 }),
            decoded(15, 0, D::Particle { emitter_id: 7 }),
            decoded(16, 0, D::NoDraw { nodraw: 1 }),
            decoded(17, 0, D::None),
            decoded(18, 0, D::DefaultScriptPart { part_index: 3 }),
            decoded(
                19,
                0,
                D::CallPes(dereth_primitives::records::HookCallPes {
                    pes: DataId(0x3300_0001),
                    pause: 0.25,
                }),
            ),
            decoded(
                20,
                0,
                D::Ramp(dereth_primitives::records::HookRamp {
                    start: 0.0,
                    end: 1.0,
                    time: 0.5,
                }),
            ),
            decoded(
                21,
                0,
                D::SoundTweaked(dereth_primitives::records::HookSoundTweaked {
                    sound_id: DataId(0x0A00_0002),
                    probability: 0.25,
                    priority: 0.75,
                    volume: 1.0,
                }),
            ),
            decoded(
                22,
                0,
                D::SetOmega {
                    axis: Vec3::new(0.0, 0.0, 1.0),
                },
            ),
            decoded(
                23,
                0,
                D::TextureVelocity(dereth_primitives::records::HookTextureVelocity {
                    u_speed: 0.1,
                    v_speed: 0.2,
                }),
            ),
            decoded(
                24,
                0,
                D::TextureVelocityPart(dereth_primitives::records::HookTextureVelocityPart {
                    part_index: 1,
                    u_speed: 0.1,
                    v_speed: 0.2,
                }),
            ),
            decoded(25, 0, D::SetLight { lights_on: 1 }),
            decoded(
                26,
                0,
                D::CreateParticle(dereth_primitives::records::HookCreateParticle {
                    emitter_info_id: DataId(0x3200_0002),
                    part_index: 0,
                    offset: f,
                    emitter_id: 9,
                }),
            ),
        ];
        assert_eq!(
            cases.len(),
            26,
            "26 constructible types; 4 is deliberately absent"
        );
        for c in &cases {
            let got =
                hook_from_decoded(c).unwrap_or_else(|| panic!("type {} dropped", c.hook_type));
            assert_eq!(
                got.hook_type(),
                c.hook_type,
                "round-trip of type {}",
                c.hook_type
            );
        }
    }

    /// The trap: type 4 and unknown types are dropped, not errors, and the rest of the frame's
    /// list survives in file order.
    #[test]
    fn type_4_and_unknown_types_are_dropped_not_errors() {
        assert_eq!(hook_from_decoded(&decoded(4, 0, D::None)), None);
        assert_eq!(hook_from_decoded(&decoded(99, 0, D::None)), None);
        // A payload that does not match its type also falls into the default arm.
        assert_eq!(hook_from_decoded(&decoded(1, 0, D::None)), None);

        let list = vec![
            decoded(0, 0, D::None),
            decoded(4, 0, D::None),
            decoded(17, 0, D::None),
        ];
        let out = hooks_from_decoded(&list);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].hook_type(), 0);
        assert_eq!(out[1].hook_type(), 17, "file order survives the drop");
    }
}
