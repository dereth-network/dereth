//! Tagged hook payloads attached to animation frames, setup placement frames and
//! physics scripts.
//!
//! A hook record is `u32 type`, `i32 direction`, then a per-type payload, then
//! alignment to a four-byte boundary.
//!
//! Hook records are described in `docs/formats/12-animation.md`.

use dereth_dat::{Cursor, DatError};
use dereth_primitives::{DataId, Frame, Vec3};

use crate::error::AssetError;

/// The `AnimationHookType` values the client's switch has cases for.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum HookData {
    /// 0 `NOOP`, 4 `ANIMDONE`, 17 `DEFAULT_SCRIPT` — no payload.
    None,
    /// 1 `SOUND`.
    Sound { sound_id: DataId },
    /// 2 `SOUND_TABLE`.
    SoundTable { sound_type: u32 },
    /// 3 `ATTACK`.
    Attack {
        part_index: u32,
        left: (f32, f32),
        right: (f32, f32),
        radius: f32,
        height: f32,
    },
    /// 5 `REPLACE_OBJECT`. The part id is a compressed DataID relative to `GFXOBJ`'s base.
    ReplaceObject { part_index: u8, part_id: DataId },
    /// 6 `ETHEREAL`.
    Ethereal { ethereal: i32 },
    /// 7 `TRANSPARENT_PART`, 9 `LUMINOUS_PART`, 11 `DIFFUSE_PART`.
    PartRamp {
        part: u32,
        start: f32,
        end: f32,
        time: f32,
    },
    /// 8 `LUMINOUS`, 10 `DIFFUSE`, 20 `TRANSPARENT`.
    Ramp { start: f32, end: f32, time: f32 },
    /// 12 `SCALE`.
    Scale { end: f32, time: f32 },
    /// 13 `CREATE_PARTICLE`, 26 `CREATE_BLOCKING_PARTICLE`.
    CreateParticle {
        emitter_info_id: DataId,
        part_index: u32,
        offset: Frame,
        emitter_id: u32,
    },
    /// 14 `DESTROY_PARTICLE`, 15 `STOP_PARTICLE`.
    Particle { emitter_id: u32 },
    /// 16 `NODRAW`.
    NoDraw { nodraw: u32 },
    /// 18 `DEFAULT_SCRIPT_PART`.
    DefaultScriptPart { part_index: u32 },
    /// 19 `CALL_PES`.
    CallPes { pes: DataId, pause: f32 },
    /// 21 `SOUND_TWEAKED`.
    ///
    /// **Probability before priority.** ACE's `SoundTweakedHook` has the two swapped.
    /// Contract-relevant: the fields are floats, so a swap does not
    /// desynchronise the cursor and cannot be caught by `expect_end`.
    SoundTweaked {
        sound_id: DataId,
        probability: f32,
        priority: f32,
        volume: f32,
    },
    /// 22 `SET_OMEGA`.
    SetOmega { axis: Vec3 },
    /// 23 `TEXTURE_VELOCITY`.
    TextureVelocity { u_speed: f32, v_speed: f32 },
    /// 24 `TEXTURE_VELOCITY_PART`.
    TextureVelocityPart {
        part_index: u32,
        u_speed: f32,
        v_speed: f32,
    },
    /// 25 `SET_LIGHT`.
    SetLight { lights_on: i32 },
}

/// One hook: its type, its direction and its payload.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimHook {
    pub hook_type: u32,
    pub direction: i32,
    pub data: HookData,
}

impl AnimHook {
    /// Decodes one hook: the type tag, the direction, then the payload the tag selects.
    pub fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let hook_type = c.u32()?;
        let direction = c.i32()?;
        let data = match hook_type {
            0 | 4 | 17 => HookData::None,
            1 => HookData::Sound {
                sound_id: c.data_id()?,
            },
            2 => HookData::SoundTable {
                sound_type: c.u32()?,
            },
            3 => HookData::Attack {
                part_index: c.u32()?,
                left: (c.f32()?, c.f32()?),
                right: (c.f32()?, c.f32()?),
                radius: c.f32()?,
                height: c.f32()?,
            },
            5 => HookData::ReplaceObject {
                part_index: c.u8()?,
                // A data id of a known type, read against GFXOBJ's base.
                part_id: c.data_id_of_known_type(0x0100_0000)?,
            },
            6 => HookData::Ethereal { ethereal: c.i32()? },
            7 | 9 | 11 => HookData::PartRamp {
                part: c.u32()?,
                start: c.f32()?,
                end: c.f32()?,
                time: c.f32()?,
            },
            8 | 10 | 20 => HookData::Ramp {
                start: c.f32()?,
                end: c.f32()?,
                time: c.f32()?,
            },
            12 => HookData::Scale {
                end: c.f32()?,
                time: c.f32()?,
            },
            13 | 26 => HookData::CreateParticle {
                emitter_info_id: c.data_id()?,
                part_index: c.u32()?,
                offset: c.placed_frame()?,
                emitter_id: c.u32()?,
            },
            14 | 15 => HookData::Particle {
                emitter_id: c.u32()?,
            },
            16 => HookData::NoDraw { nodraw: c.u32()? },
            18 => HookData::DefaultScriptPart {
                part_index: c.u32()?,
            },
            19 => HookData::CallPes {
                pes: c.data_id()?,
                pause: c.f32()?,
            },
            21 => HookData::SoundTweaked {
                sound_id: c.data_id()?,
                probability: c.f32()?,
                priority: c.f32()?,
                volume: c.f32()?,
            },
            22 => HookData::SetOmega { axis: c.vec3()? },
            23 => HookData::TextureVelocity {
                u_speed: c.f32()?,
                v_speed: c.f32()?,
            },
            24 => HookData::TextureVelocityPart {
                part_index: c.u32()?,
                u_speed: c.f32()?,
                v_speed: c.f32()?,
            },
            25 => HookData::SetLight {
                lights_on: c.i32()?,
            },
            other => {
                return Err(AssetError::UnknownTag {
                    what: "animation hook",
                    tag: other,
                })
            }
        };
        c.align_ptr();
        Ok(Self {
            hook_type,
            direction,
            data,
        })
    }

    /// The DataIDs this hook references.
    pub fn sub_data_ids(&self, out: &mut Vec<DataId>) {
        match &self.data {
            HookData::Sound { sound_id } | HookData::SoundTweaked { sound_id, .. } => {
                out.push(*sound_id);
            }
            HookData::ReplaceObject { part_id, .. } => out.push(*part_id),
            HookData::CreateParticle {
                emitter_info_id, ..
            } => out.push(*emitter_info_id),
            HookData::CallPes { pes, .. } => out.push(*pes),
            _ => {}
        }
    }
}

/// A helper for the many `u32 count` + hooks lists.
pub fn read_hooks(c: &mut Cursor<'_>) -> Result<Vec<AnimHook>, AssetError> {
    let n = c.u32()? as usize;
    let mut v = Vec::new();
    for _ in 0..n {
        v.push(AnimHook::decode(c)?);
    }
    Ok(v)
}

/// The count-less form, for callers that already read the count.
pub fn read_n_hooks(c: &mut Cursor<'_>, n: usize) -> Result<Vec<AnimHook>, AssetError> {
    let mut v = Vec::new();
    for _ in 0..n {
        v.push(AnimHook::decode(c)?);
    }
    Ok(v)
}

/// Re-exported so callers do not have to name two error types.
pub type HookResult<T> = Result<T, DatError>;

#[cfg(test)]
mod tests {
    use super::*;

    /// A hook whose type is none the client knows is refused, and the record holding it with it,
    /// rather than read with a guessed payload: the client itself reads no payload for such a type,
    /// so everything after it in the record would be misread.
    #[test]
    fn an_animation_hook_of_an_unknown_type_is_refused() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&0x7Fu32.to_le_bytes()); // type
        buf.extend_from_slice(&1i32.to_le_bytes()); // direction
        buf.extend_from_slice(&[0u8; 16]);
        assert!(matches!(
            AnimHook::decode(&mut Cursor::new(&buf)),
            Err(AssetError::UnknownTag {
                what: "animation hook",
                tag: 0x7F
            })
        ));
    }

    /// Oracle: an independent reader of the shipped dats, which decodes every hook in the 2,066
    /// animations, 5,935 setups and 4,248 physics scripts with zero trailing bytes.
    #[test]
    fn a_noop_hook_is_eight_bytes_and_needs_no_padding() {
        let buf = [0u8; 8];
        let mut c = Cursor::new(&buf);
        let h = AnimHook::decode(&mut c).unwrap();
        assert_eq!(h.hook_type, 0);
        assert_eq!(h.data, HookData::None);
        c.expect_end().unwrap();
    }

    /// `REPLACE_OBJECT` is the one hook with a sub-dword field, so it is the one that proves the
    /// trailing `ALIGN_PTR`.
    #[test]
    fn replace_object_pads_to_four() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&5u32.to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.push(3); // part_index
        buf.extend_from_slice(&0x0123u16.to_le_bytes()); // compressed DataID delta
        buf.push(0); // pad to 12
        let mut c = Cursor::new(&buf);
        let h = AnimHook::decode(&mut c).unwrap();
        assert_eq!(
            h.data,
            HookData::ReplaceObject {
                part_index: 3,
                part_id: DataId(0x0100_0123)
            }
        );
        c.expect_end().unwrap();
    }

    /// ACE reads priority before probability. The client does not.
    #[test]
    fn sound_tweaked_is_probability_then_priority() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&21u32.to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&0x0A00_0002u32.to_le_bytes());
        buf.extend_from_slice(&0.25f32.to_le_bytes()); // probability
        buf.extend_from_slice(&0.75f32.to_le_bytes()); // priority
        buf.extend_from_slice(&1.0f32.to_le_bytes()); // volume
        let mut c = Cursor::new(&buf);
        let h = AnimHook::decode(&mut c).unwrap();
        assert_eq!(
            h.data,
            HookData::SoundTweaked {
                sound_id: DataId(0x0A00_0002),
                probability: 0.25,
                priority: 0.75,
                volume: 1.0,
            }
        );
        c.expect_end().unwrap();
    }
}
