//! Animation: animations (`.anm`), animation maps (`.animmap`), motion descriptions (`.midesc`),
//! behavior tables (`.behaviortable`), character templates (`.ctp`) and valid modes
//! (`.validmodes`).
//!
//! Most of these records are tables keyed by a mode and an element: a body (or a part of one, an
//! "element") is always in some mode (standing, swimming, in combat...), and the tables say how it
//! moves, which behaviors it may play, and how it gets from one mode to another.

use crate::lookup::LookupTable;
use auberean_dat::reader::Result;
use auberean_dat::Reader;
use auberean_primitives::{DataId, Quat, Vec3};

use crate::{Context, Decode};

/// An element of a body in a mode: the key most motion tables use.
// D2-15100
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct ModeKey {
    pub element: u32,
    pub mode: u32,
}

impl ModeKey {
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            element: r.u32()?,
            mode: r.u32()?,
        })
    }
}

/// A change of mode: an element in one mode going to an element in another.
// D2-15098
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct ModeChange {
    pub from: ModeKey,
    pub to: ModeKey,
}

impl ModeChange {
    // D2-15106
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            from: ModeKey::read(r)?,
            to: ModeKey::read(r)?,
        })
    }
}

/// The modes a body may be in (`.validmodes`): a counted list of mode ids. The record stores no
/// id of its own.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct AllowedModes {
    pub modes: Vec<u32>,
}

// D2-05072, D2-15224, D2-15225, D2-15226, D2-15227
impl Decode for AllowedModes {
    // D2-15223
    fn decode(r: &mut Reader<'_>, _id: u32, _ctx: &Context) -> Result<Self> {
        Ok(Self {
            modes: r.u32_list()?,
        })
    }
}

/// An animation map (`.animmap`): a table from an animation key to an animation record.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct AnimMap {
    pub entries: Vec<(u32, DataId)>,
}

// D2-05020, D2-14899, D2-14902, D2-14903, D2-14904, D2-14905, D2-15359, D2-14906
impl Decode for AnimMap {
    /// The id, then a hash table from a key word to an animation id.
    // D2-14897, D2-00124
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let entries = r.hash_table(8, |r| Ok((r.u32()?, r.data_id()?)))?;
        Ok(Self { entries })
    }
}

/// How a body moves in one mode on one surface: lift, drag, speeds, accelerations, slow-downs, the
/// jump and the hover height.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct MotionParams {
    pub lift: f32,
    pub drag: f32,
    pub terminal_speed: f32,
    pub gravity: u32,
    pub sink_offset: f32,
    pub hover_height: f32,
    pub jump_impulse: f32,
    pub max_forward: f32,
    pub max_backward: f32,
    pub max_strafe: f32,
    pub still_turn_rate: f32,
    pub running_turn_rate: f32,
    pub max_vertical: f32,
    pub forward_slowdown: f32,
    pub backward_slowdown: f32,
    pub strafe_slowdown: f32,
    pub turn_slowdown: f32,
    pub forward_acceleration: f32,
    pub backward_acceleration: f32,
    pub strafe_acceleration: f32,
    pub turn_acceleration: f32,
    pub vertical_acceleration: f32,
}

impl MotionParams {
    /// Twenty-two words, in this stored order.
    // D2-15481
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            lift: r.f32()?,
            drag: r.f32()?,
            terminal_speed: r.f32()?,
            gravity: r.u32()?,
            sink_offset: r.f32()?,
            hover_height: r.f32()?,
            jump_impulse: r.f32()?,
            max_forward: r.f32()?,
            max_backward: r.f32()?,
            max_strafe: r.f32()?,
            still_turn_rate: r.f32()?,
            running_turn_rate: r.f32()?,
            max_vertical: r.f32()?,
            forward_slowdown: r.f32()?,
            backward_slowdown: r.f32()?,
            strafe_slowdown: r.f32()?,
            turn_slowdown: r.f32()?,
            forward_acceleration: r.f32()?,
            backward_acceleration: r.f32()?,
            strafe_acceleration: r.f32()?,
            turn_acceleration: r.f32()?,
            vertical_acceleration: r.f32()?,
        })
    }
}

/// The motion of one terrain kind: values per modifier, and the default.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct TerrainMotion {
    pub by_modifier: Vec<(u32, MotionParams)>,
    pub default: MotionParams,
}

impl TerrainMotion {
    // D2-15436, D2-05176
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let by_modifier = r.hash_table(92, |r| Ok((r.u32()?, MotionParams::read(r)?)))?;
        Ok(Self {
            by_modifier,
            default: MotionParams::read(r)?,
        })
    }
}

/// One set of motion values: per terrain kind, per modifier, and the default.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MotionSet {
    pub by_terrain: Vec<(u32, TerrainMotion)>,
    pub by_modifier: Vec<(u32, MotionParams)>,
    pub default: MotionParams,
}

impl MotionSet {
    /// The terrain table, the modifier table, then the default values.
    // D2-15437, D2-05186
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let by_terrain = r.hash_table(96, |r| Ok((r.u32()?, TerrainMotion::read(r)?)))?;
        let by_modifier = r.hash_table(92, |r| Ok((r.u32()?, MotionParams::read(r)?)))?;
        Ok(Self {
            by_terrain,
            by_modifier,
            default: MotionParams::read(r)?,
        })
    }
}

/// How bodies move (`.midesc`): which motion set each element and mode uses, the sets, and the
/// multipliers on the sea floor and in water.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MotionDesc {
    /// Element and mode to an index into `sets`.
    pub set_of: Vec<(ModeKey, u32)>,
    pub sets: Vec<MotionSet>,
    pub sea_floor_multiplier: f32,
    pub water_multiplier: f32,
}

impl Decode for MotionDesc {
    // D2-15435, D2-15449, D2-15461
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let set_of = r.hash_table(12, |r| Ok((ModeKey::read(r)?, r.u32()?)))?;
        let sets = r.list(96, MotionSet::read)?;
        Ok(Self {
            set_of,
            sets,
            sea_floor_multiplier: r.f32()?,
            water_multiplier: r.f32()?,
        })
    }
}

/// A combination of an element, a mode and a behavior.
// D2-15096
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct BehaviorKey {
    pub element: u32,
    pub mode: u32,
    pub behavior: u32,
}

impl BehaviorKey {
    // D2-15108
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            element: r.u32()?,
            mode: r.u32()?,
            behavior: r.u32()?,
        })
    }
}

/// What a body does on a change of mode: the behavior it plays, and the effects started and
/// stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ModeChangeEffects {
    pub behavior: u32,
    pub effect: u32,
    pub stop_effect: u32,
}

/// How a behavior starts: its ramp time and its effect.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct BehaviorStart {
    pub ramp_time: f32,
    pub effect: u32,
}

/// A body kind's behaviors and modes (`.behaviortable`).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct BehaviorTable {
    /// How long each change of mode takes to blend.
    pub ramp_times: Vec<(ModeChange, f32)>,
    /// Which animation key and layer an element, mode and behavior play.
    pub combos: Vec<(BehaviorKey, (u32, u32))>,
    pub transitions: Vec<(ModeChange, ModeChangeEffects)>,
    pub layers: Vec<u32>,
    pub layer_tweaks: Vec<i32>,
    /// The modes each element and mode may change to.
    pub next_modes: Vec<(ModeKey, Vec<ModeKey>)>,
    pub modes: Vec<(ModeKey, i32)>,
    pub behaviors: Vec<(u32, BehaviorStart)>,
    pub initial_behaviors: Vec<u32>,
    pub initial_mode: u32,
}

// D2-05070, D2-15042, D2-15051, D2-15052, D2-15053, D2-15055, D2-15056
impl Decode for BehaviorTable {
    /// The id, then in order: the ramp times, the combinations, the transitions, the layers, the
    /// layer tweaks, the mode changes, the modes, the behaviors, the initial behaviors and the
    /// initial mode.
    // D2-14972, D2-15023, D2-15025, D2-15029, D2-15047, D2-15020, D2-06545, D2-15033
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let ramp_times = r.hash_table(20, |r| Ok((ModeChange::read(r)?, r.f32()?)))?;
        let combos = r.hash_table(20, |r| Ok((BehaviorKey::read(r)?, (r.u32()?, r.u32()?))))?;
        let transitions = r.hash_table(28, |r| {
            Ok((
                ModeChange::read(r)?,
                ModeChangeEffects {
                    behavior: r.u32()?,
                    effect: r.u32()?,
                    stop_effect: r.u32()?,
                },
            ))
        })?;
        let layers = r.u32_list()?;
        let layer_tweaks = r.list(4, Reader::i32)?;
        let next_modes =
            r.hash_table(12, |r| Ok((ModeKey::read(r)?, r.list(8, ModeKey::read)?)))?;
        let modes = r.hash_table(12, |r| Ok((ModeKey::read(r)?, r.i32()?)))?;
        let behaviors = r.hash_table(12, |r| {
            Ok((
                r.u32()?,
                BehaviorStart {
                    ramp_time: r.f32()?,
                    effect: r.u32()?,
                },
            ))
        })?;
        let initial_behaviors = r.u32_list()?;
        let initial_mode = r.u32()?;
        Ok(Self {
            ramp_times,
            combos,
            transitions,
            layers,
            layer_tweaks,
            next_modes,
            modes,
            behaviors,
            initial_behaviors,
            initial_mode,
        })
    }
}

/// The character template (`.ctp`): every behavior with its flags, every slider with its
/// controller, every mode with its combat flag, the default element and mode, and the sliders the
/// player controls.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct CharacterTemplate {
    pub behaviors: Vec<(u32, u64)>,
    pub sliders: Vec<(u32, u32)>,
    pub modes: Vec<(u32, bool)>,
    pub default_element: u32,
    pub default_mode: u32,
    pub player_sliders: Vec<u32>,
}

// D2-15067, D2-15068, D2-15069, D2-15070, D2-15086, D2-15089, D2-15093, D2-14907, D2-15177, D2-15151, D2-15094
impl Decode for CharacterTemplate {
    // D2-15062, D2-14908, D2-15178, D2-15152, D2-15090, D2-15092, D2-15091
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let behaviors = r.hash_table(12, |r| Ok((r.u32()?, r.u64()?)))?;
        let sliders = r.hash_table(8, |r| Ok((r.u32()?, r.u32()?)))?;
        let modes = r.hash_table(8, |r| Ok((r.u32()?, r.bool32()?)))?;
        let default_element = r.u32()?;
        let default_mode = r.u32()?;
        let player_sliders = r.u32_list()?;
        Ok(Self {
            behaviors,
            sliders,
            modes,
            default_element,
            default_mode,
            player_sliders,
        })
    }
}

/// A key of a bone's scale or offset over time: a vector, and the time it holds at.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct VectorKey {
    pub value: Vec3,
    pub time: f32,
}

/// A key of a bone's rotation over time.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct RotationKey {
    pub value: Quat,
    pub time: f32,
}

/// A key of a bone's blend weight over time.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct WeightKey {
    pub weight: f32,
    pub time: f32,
}

fn vector_key(r: &mut Reader<'_>) -> Result<VectorKey> {
    Ok(VectorKey {
        value: r.vec3()?,
        time: r.f32()?,
    })
}

/// One bone's tracks: its scale, offset, rotation and weight keys.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct BoneTrack {
    pub scale: Vec<VectorKey>,
    pub offset: Vec<VectorKey>,
    pub rotation: Vec<RotationKey>,
    pub weight: Vec<WeightKey>,
    /// The lookup tables of the keys read from the data: scale, offset, rotation, weight. One of
    /// a track made by hand, or whose first key is not at 0, is absent.
    pub tables: [Option<LookupTable>; 4],
}

impl BoneTrack {
    // D2-15287, D2-15288, D2-15289, D2-15273, D2-15251, D2-15253, D2-15255, D2-13300, D2-13301, D2-15107
    // D2-13293, D2-13294, D2-13295, D2-13296, D2-13297, D2-13299, D2-15233, D2-15234, D2-15235, D2-15245
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let scale: Vec<VectorKey> = r.list(16, vector_key)?;
        let offset: Vec<VectorKey> = r.list(16, vector_key)?;
        let rotation: Vec<RotationKey> = r.list(20, |r| {
            Ok(RotationKey {
                value: r.quat()?,
                time: r.f32()?,
            })
        })?;
        let weight: Vec<WeightKey> = r.list(8, |r| {
            Ok(WeightKey {
                weight: r.f32()?,
                time: r.f32()?,
            })
        })?;
        fn table<K>(keys: &[K], time: impl Fn(&K) -> f32) -> Option<LookupTable> {
            LookupTable::new(&keys.iter().map(time).collect::<Vec<f32>>())
        }
        let tables = [
            table(&scale, |k| k.time),
            table(&offset, |k| k.time),
            table(&rotation, |k| k.time),
            table(&weight, |k| k.time),
        ];
        Ok(Self {
            scale,
            offset,
            rotation,
            weight,
            tables,
        })
    }
}

/// An effect fired at a moment of an animation.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct EffectTrigger {
    pub effect: u32,
    pub time: f32,
    pub stop: bool,
    pub probability: f32,
    pub intensity: f32,
    pub intensity_variance: f32,
    pub play: bool,
}

/// A moment of an animation the game logic hears about.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct GameHook {
    pub kind: u32,
    pub number: u32,
    pub data: u32,
    pub triggered: bool,
    pub time: f32,
}

/// A push given to the body at a moment of an animation: a jump, or an impulse vector.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Impulse {
    pub jump: bool,
    /// Stored only when the trigger is not a jump.
    pub impulse: Option<Vec3>,
    pub number: u32,
    pub time: f32,
}

/// What happens during an animation besides the bones moving, and its hold loop.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct AnimationEvents {
    // D2-15259
    pub effects: Vec<EffectTrigger>,
    pub hooks: Vec<GameHook>,
    pub impulses: Vec<Impulse>,
    pub has_hold_loop: bool,
    pub loop_start: f32,
    pub loop_end: f32,
}

impl AnimationEvents {
    // D2-14838, D2-05147, D2-15222, D2-14923, D2-15232, D2-05150, D2-15150
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let effects = r.list(28, |r| {
            Ok(EffectTrigger {
                effect: r.u32()?,
                time: r.f32()?,
                stop: r.bool32()?,
                probability: r.f32()?,
                intensity: r.f32()?,
                intensity_variance: r.f32()?,
                play: r.bool32()?,
            })
        })?;
        let hooks = r.list(20, |r| {
            Ok(GameHook {
                kind: r.u32()?,
                number: r.u32()?,
                data: r.u32()?,
                triggered: r.bool32()?,
                time: r.f32()?,
            })
        })?;
        let impulses = r.list(12, |r| {
            let jump = r.bool32()?;
            let impulse = if jump { None } else { Some(r.vec3()?) };
            Ok(Impulse {
                jump,
                impulse,
                number: r.u32()?,
                time: r.f32()?,
            })
        })?;
        Ok(Self {
            effects,
            hooks,
            impulses,
            has_hold_loop: r.bool32()?,
            loop_start: r.f32()?,
            loop_end: r.f32()?,
        })
    }
}

/// Keyframed bone tracks: the period, a track per bone, and the events when present.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct KeyframeAnimation {
    pub period: f32,
    pub bones: Vec<BoneTrack>,
    pub events: Option<AnimationEvents>,
}

/// A slider and the animation ticks along it.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SliderTicks {
    // D2-15312, D2-14834
    pub slider: u32,
    pub ticks: Vec<u32>,
}

/// How an animation is made.
#[derive(Clone, Debug, PartialEq)]
pub enum AnimationKind {
    /// Keyframed bone tracks (kind `0x40000001`).
    Keyframes(KeyframeAnimation),
    /// A blend of other animations placed along sliders (kind `0x40000002`): the sliders with
    /// their ticks, and the animations.
    Blend {
        sliders: Vec<SliderTicks>,
        animations: Vec<DataId>,
    },
    /// Another animation record, played as a body's (kind `0x40000003`).
    Body { animation: DataId },
    /// Turns the head toward a target; stores nothing (kind `0x40000005`).
    Look,
    /// Animations weighted by sliders, one slider each, with an origin animation taking whatever
    /// weight they leave (kind `0x40000006`).
    AxisCube {
        origin: DataId,
        animations: Vec<DataId>,
        sliders: Vec<u32>,
    },
    /// Picks among animations along a slider with some randomness (kind `0x40000007`).
    Random {
        slider: u32,
        animations: Vec<DataId>,
        frequency: f32,
        ramp_time: f32,
        random_slider: u32,
        variation: f32,
    },
    /// Plays a default animation, or another one by a switch's value (kind `0x40000008`).
    Switch {
        switch_kind: u32,
        default: DataId,
        overrides: Vec<(u32, DataId)>,
    },
    /// Turns bones within angle limits (kind `0x4000000A`): each bone with its x and y share.
    Angle {
        bones: Vec<(u32, f32, f32)>,
        max_x: f32,
        min_x: f32,
        max_y: f32,
        min_y: f32,
    },
}

/// An animation (`.anm`, `0x05000000`+): the id, the kind of animator, and what that kind stores.
#[derive(Clone, Debug, PartialEq)]
pub struct Animation {
    pub animator: AnimationKind,
}

// D2-05021, D2-15109, D2-15117, D2-15118, D2-15121, D2-15116, D2-14826, D2-14829, D2-15236, D2-15246, D2-15294, D2-15275, D2-15276, D2-15277, D2-15278, D2-15300, D2-15301, D2-15303, D2-15318, D2-15296, D2-15299
// D2-15319, D2-15323, D2-15324, D2-15328, D2-15335, D2-15336, D2-15348, D2-15355, D2-15356, D2-15357, D2-15358, D2-15382, D2-15387, D2-15388, D2-15390, D2-15399, D2-15400
// D2-14827, D2-15407, D2-15409, D2-15413, D2-15422, D2-15423, D2-15424, D2-15425, D2-15428, D2-15365, D2-15366, D2-15367, D2-15368, D2-15372, D2-15257, D2-15290
// D2-15122, D2-15260, D2-16991
impl Decode for Animation {
    // D2-15123, D2-14831, D2-15292, D2-15283, D2-15306, D2-15298, D2-15317, D2-15326, D2-15410, D2-15418, D2-15381, D2-15341, D2-15342, D2-15351
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let at = r.pos();
        let kind = r.u32()?;
        let animator = match kind {
            0x4000_0001 => {
                let period = r.f32()?;
                // D2-15274
                let bones = r.list(16, BoneTrack::read)?;
                let events = if r.bool32()? {
                    Some(AnimationEvents::read(r)?)
                } else {
                    None
                };
                AnimationKind::Keyframes(KeyframeAnimation {
                    period,
                    bones,
                    events,
                })
            }
            0x4000_0002 => {
                let sliders = r.list(8, |r| {
                    Ok(SliderTicks {
                        slider: r.u32()?,
                        // D2-01875
                        ticks: r.u32_list()?,
                    })
                })?;
                AnimationKind::Blend {
                    sliders,
                    animations: r.list(4, Reader::data_id)?,
                }
            }
            0x4000_0003 => AnimationKind::Body {
                animation: r.data_id()?,
            },
            0x4000_0005 => AnimationKind::Look,
            // D2-15363
            0x4000_0006 => {
                let origin = r.data_id()?;
                let animations = r.list(4, Reader::data_id)?;
                AnimationKind::AxisCube {
                    origin,
                    animations,
                    sliders: r.u32_list()?,
                }
            }
            0x4000_0007 => {
                let slider = r.u32()?;
                let animations = r.list(4, Reader::data_id)?;
                AnimationKind::Random {
                    slider,
                    animations,
                    frequency: r.f32()?,
                    ramp_time: r.f32()?,
                    random_slider: r.u32()?,
                    variation: r.f32()?,
                }
            }
            0x4000_0008 => {
                let default = r.data_id()?;
                let switch_kind = r.u32()?;
                let overrides = r.hash_table(8, |r| Ok((r.u32()?, r.data_id()?)))?;
                AnimationKind::Switch {
                    switch_kind,
                    default,
                    overrides,
                }
            }
            0x4000_000A => {
                let bones = r.list(12, |r| Ok((r.u32()?, r.f32()?, r.f32()?)))?;
                AnimationKind::Angle {
                    bones,
                    max_x: r.f32()?,
                    min_x: r.f32()?,
                    max_y: r.f32()?,
                    min_y: r.f32()?,
                }
            }
            _ => {
                return Err(auberean_dat::DecodeError {
                    at,
                    what: format!("animator kind {kind:#x}"),
                });
            }
        };
        Ok(Self { animator })
    }
}
