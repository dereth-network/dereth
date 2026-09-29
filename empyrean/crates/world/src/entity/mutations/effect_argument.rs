// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Mutations/EffectArgument.cs
//! Port of `Source/ACE.Server/Entity/Mutations/EffectArgument.cs`.
//!
//! One operand of a mutation effect: a literal (int, int64, double), a quality (a property of the
//! item), a random range, or a variable. The operators are in `effect_argument_op.rs`, the other
//! half of ACE's partial class.

use empyrean_common::dotnet::CsCast;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_entity::enums::{
    EffectArgumentType, PropertyBool, PropertyDataId, PropertyFloat, PropertyInt, PropertyInt64,
    StatType,
};

use crate::world_objects::world_object::WorldObject;

/// ACE `EffectArgument`. A class in ACE: [`Effect`](super::effect::Effect) never mutates its own
/// arguments, it works on copies (`new EffectArgument(other)`), which are `Clone`s here.
// ACE: EffectArgument
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EffectArgument {
    pub r#type: EffectArgumentType,

    // EffectArgumentType.Quality
    pub stat_type: StatType,
    pub stat_idx: i32,

    // EffectArgumentType.Int
    pub int_val: i32,

    // EffectArgumentType.Int64
    pub long_val: i64,

    // EffectArgumentType.Double
    pub double_val: f64,

    // EffectArgumentType.Random
    pub min_val: f32,
    pub max_val: f32,

    // gdle custom
    pub is_valid: bool,
}

impl EffectArgument {
    /// `new EffectArgument()`. The other constructors are [`Self::from_quality`],
    /// [`Self::from_int`], [`Self::from_long`], [`Self::from_double`], [`Self::from_range`] and
    /// [`Self::copy_of`].
    // ACE: EffectArgument.EffectArgument
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `new EffectArgument(StatType statType, int propIdx)`.
    #[must_use]
    pub fn from_quality(stat_type: StatType, prop_idx: i32) -> Self {
        Self {
            r#type: EffectArgumentType::Quality,
            stat_type,
            stat_idx: prop_idx,
            ..Self::default()
        }
    }

    /// `new EffectArgument(int val)`.
    #[must_use]
    pub fn from_int(val: i32) -> Self {
        Self {
            r#type: EffectArgumentType::Int,
            int_val: val,
            is_valid: true,
            ..Self::default()
        }
    }

    /// `new EffectArgument(long val)`.
    #[must_use]
    pub fn from_long(val: i64) -> Self {
        Self {
            r#type: EffectArgumentType::Int64,
            long_val: val,
            is_valid: true,
            ..Self::default()
        }
    }

    /// `new EffectArgument(double val)`.
    #[must_use]
    pub fn from_double(val: f64) -> Self {
        Self {
            r#type: EffectArgumentType::Double,
            double_val: val,
            is_valid: true,
            ..Self::default()
        }
    }

    /// `new EffectArgument(float minVal, float maxVal)`.
    #[must_use]
    pub fn from_range(min_val: f32, max_val: f32) -> Self {
        Self {
            r#type: EffectArgumentType::Random,
            min_val,
            max_val,
            ..Self::default()
        }
    }

    /// `new EffectArgument(EffectArgument other)`: every field but `IsValid`, which starts false;
    /// a `null` other gives a default (`Invalid`) argument.
    #[must_use]
    pub fn copy_of(other: Option<&EffectArgument>) -> Self {
        let Some(other) = other else {
            return Self::default();
        };

        Self {
            r#type: other.r#type,

            stat_type: other.stat_type,
            stat_idx: other.stat_idx,

            int_val: other.int_val,

            long_val: other.long_val,

            double_val: other.double_val,

            min_val: other.min_val,
            max_val: other.max_val,

            is_valid: false,
        }
    }

    /// The boxed value of a literal argument, `null` for the other types.
    // ACE: EffectArgument.GetValue
    #[must_use]
    pub fn get_value(&self) -> Option<EffectValue> {
        match self.r#type {
            EffectArgumentType::Double => Some(EffectValue::Double(self.double_val)),
            EffectArgumentType::Int => Some(EffectValue::Int(self.int_val)),
            EffectArgumentType::Int64 => Some(EffectValue::Int64(self.long_val)),
            _ => None,
        }
    }

    // output conversions

    // ACE: EffectArgument.ToDouble
    #[must_use]
    pub fn to_double(&self) -> f64 {
        match self.r#type {
            EffectArgumentType::Int => f64::from(self.int_val),
            EffectArgumentType::Int64 => self.long_val.cs_cast(),
            EffectArgumentType::Double => self.double_val,
            _ => {
                log::error!("EffectArgument.ToDouble() - invalid type {}", self.r#type);
                0.0
            }
        }
    }

    // ACE: EffectArgument.ToInt
    #[must_use]
    pub fn to_int(&self) -> i32 {
        match self.r#type {
            EffectArgumentType::Int => self.int_val,
            EffectArgumentType::Int64 => self.long_val.cs_cast(),
            EffectArgumentType::Double => self.double_val.cs_cast(),
            _ => {
                log::error!("EffectArgument.ToInt() - invalid type {}", self.r#type);
                0
            }
        }
    }

    // ACE: EffectArgument.ToLong
    #[must_use]
    pub fn to_long(&self) -> i64 {
        match self.r#type {
            EffectArgumentType::Int => i64::from(self.int_val),
            EffectArgumentType::Int64 => self.long_val,
            EffectArgumentType::Double => self.double_val.cs_cast(),
            _ => {
                log::error!("EffectArgument.ToLong() - invalid type {}", self.r#type);
                0
            }
        }
    }

    /// Resolves a quality to the item's current value and a random range to one draw; literals
    /// are valid as they are. A quality the item lacks stays invalid, but its type still becomes
    /// the property's value type (with the value left as it was, zero after a copy).
    // ACE: EffectArgument.ResolveValue
    pub fn resolve_value(&mut self, item: &WorldObject) -> bool {
        // type:enum - invalid, double, int32, quality (2 int32s: type and quality), float range (min, max), variable index (int32)
        match self.r#type {
            EffectArgumentType::Double | EffectArgumentType::Int | EffectArgumentType::Int64 => {
                // these are ok as-is
                self.is_valid = true;
            }

            EffectArgumentType::Quality => match self.stat_type {
                StatType::Int => {
                    self.r#type = EffectArgumentType::Int;
                    if let Some(int_val) = item.get_property(PropertyInt(self.stat_idx.cs_cast())) {
                        self.int_val = int_val;
                        self.is_valid = true;
                    }
                }

                StatType::Int64 => {
                    self.r#type = EffectArgumentType::Int64;
                    if let Some(int64_val) =
                        item.get_property(PropertyInt64(self.stat_idx.cs_cast()))
                    {
                        self.long_val = int64_val;
                        self.is_valid = true;
                    }
                }

                StatType::Bool => {
                    self.r#type = EffectArgumentType::Int;
                    self.int_val = i32::from(
                        item.get_property(PropertyBool(self.stat_idx.cs_cast()))
                            .unwrap_or(false),
                    );
                    self.is_valid = true;
                }

                StatType::Float => {
                    self.r#type = EffectArgumentType::Double;
                    if let Some(double_val) =
                        item.get_property(PropertyFloat(self.stat_idx.cs_cast()))
                    {
                        self.double_val = double_val;
                        self.is_valid = true;
                    }
                }

                StatType::DID => {
                    self.r#type = EffectArgumentType::Int;
                    self.int_val = item
                        .get_property(PropertyDataId(self.stat_idx.cs_cast()))
                        .unwrap_or(0)
                        .cast_signed();
                    self.is_valid = true;
                }

                _ => {}
            },

            EffectArgumentType::Random => {
                self.double_val = ThreadSafeRandom::next_float(self.min_val, self.max_val);
                self.r#type = EffectArgumentType::Double;
                self.is_valid = true;
            }

            EffectArgumentType::Variable => {
                /*if (IntVal < 0 || IntVal >= GTVariables.Count)
                    break;

                this = GTVariables[IntVal];
                IsValid = true;*/
                log::error!("TODO: EffectArgumentType.Variable");
            }

            _ => {}
        }

        self.is_valid
    }

    /// Applies the resolved `result` to the quality this argument names.
    ///
    /// # Panics
    /// `result` is `None` where ACE's operators returned `null` (an invalid operand type); ACE
    /// then throws a `NullReferenceException` here.
    // ACE: EffectArgument.StoreValue
    pub fn store_value(&self, item: &mut WorldObject, result: Option<&EffectArgument>) -> bool {
        // here the resolved value (result) is applied to the qualities specified by our value

        let result =
            result.expect("NullReferenceException: EffectArgument.StoreValue(null result)");

        if !result.is_valid {
            return false;
        }

        match self.r#type {
            EffectArgumentType::Quality => {
                match self.stat_type {
                    StatType::Int => {
                        item.set_property(PropertyInt(self.stat_idx.cs_cast()), result.to_int())
                    }
                    StatType::Int64 => {
                        item.set_property(PropertyInt64(self.stat_idx.cs_cast()), result.to_long())
                    }
                    StatType::Bool => item
                        .set_property(PropertyBool(self.stat_idx.cs_cast()), result.to_int() != 0),
                    StatType::Float => item
                        .set_property(PropertyFloat(self.stat_idx.cs_cast()), result.to_double()),
                    StatType::DID => {
                        item.set_property(
                            PropertyDataId(self.stat_idx.cs_cast()),
                            result.to_int().cast_unsigned(),
                        );
                    }
                    _ => {}
                }
            }

            EffectArgumentType::Variable => {
                // TODO
                /*if (IntVal < 0 || IntVal > GTVariables.Count)
                    break;

                GTVariables[IntVal] = result;*/
            }

            _ => {}
        }
        true
    }
}

/// The boxed `object` [`EffectArgument::get_value`] returns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EffectValue {
    Double(f64),
    Int(i32),
    Int64(i64),
}
