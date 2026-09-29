// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Mutations/EffectArgumentOp.cs
//! Port of `Source/ACE.Server/Entity/Mutations/EffectArgumentOp.cs`.
//!
//! The arithmetic and comparison operators of `EffectArgument`. The result takes the type of the
//! left operand, as in ACE; mixed int/double arithmetic is done in double and cast back with C#'s
//! cast. An operand pair ACE has no case for returns `null` (`None`) after logging.

use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::EffectArgumentType;

use super::effect_argument::EffectArgument;

impl EffectArgument {
    /// `a + b`.
    // ACE: EffectArgument.op_Addition
    #[must_use]
    pub fn op_addition(a: &EffectArgument, b: &EffectArgument) -> Option<EffectArgument> {
        match a.r#type {
            EffectArgumentType::Double => match b.r#type {
                EffectArgumentType::Double => {
                    return Some(Self::from_double(a.double_val + b.double_val))
                }
                EffectArgumentType::Int => {
                    return Some(Self::from_double(a.double_val + f64::from(b.int_val)))
                }
                EffectArgumentType::Int64 => {
                    return Some(Self::from_double(
                        a.double_val + CsCast::<f64>::cs_cast(b.long_val),
                    ));
                }
                _ => {}
            },

            EffectArgumentType::Int => match b.r#type {
                EffectArgumentType::Double => {
                    return Some(Self::from_int(
                        (f64::from(a.int_val) + b.double_val).cs_cast(),
                    ));
                }
                EffectArgumentType::Int => {
                    return Some(Self::from_int(a.int_val.wrapping_add(b.int_val)))
                }
                EffectArgumentType::Int64 => {
                    return Some(Self::from_int(
                        i64::from(a.int_val).wrapping_add(b.long_val).cs_cast(),
                    ));
                }
                _ => {}
            },

            EffectArgumentType::Int64 => match b.r#type {
                EffectArgumentType::Double => {
                    return Some(Self::from_long(
                        (CsCast::<f64>::cs_cast(a.long_val) + b.double_val).cs_cast(),
                    ));
                }
                EffectArgumentType::Int => {
                    return Some(Self::from_long(
                        a.long_val.wrapping_add(i64::from(b.int_val)),
                    ))
                }
                EffectArgumentType::Int64 => {
                    return Some(Self::from_long(a.long_val.wrapping_add(b.long_val)))
                }
                _ => {}
            },

            _ => {}
        }

        log::error!(
            "EffectArgument.Add() - invalid type {}, {}",
            a.r#type,
            b.r#type
        );

        None
    }

    /// `a - b`.
    // ACE: EffectArgument.op_Subtraction
    #[must_use]
    pub fn op_subtraction(a: &EffectArgument, b: &EffectArgument) -> Option<EffectArgument> {
        match a.r#type {
            EffectArgumentType::Double => match b.r#type {
                EffectArgumentType::Double => {
                    return Some(Self::from_double(a.double_val - b.double_val))
                }
                EffectArgumentType::Int => {
                    return Some(Self::from_double(a.double_val - f64::from(b.int_val)))
                }
                EffectArgumentType::Int64 => {
                    return Some(Self::from_double(
                        a.double_val - CsCast::<f64>::cs_cast(b.long_val),
                    ));
                }
                _ => {}
            },

            EffectArgumentType::Int => match b.r#type {
                EffectArgumentType::Double => {
                    return Some(Self::from_int(
                        (f64::from(a.int_val) - b.double_val).cs_cast(),
                    ));
                }
                EffectArgumentType::Int => {
                    return Some(Self::from_int(a.int_val.wrapping_sub(b.int_val)))
                }
                EffectArgumentType::Int64 => {
                    return Some(Self::from_int(
                        i64::from(a.int_val).wrapping_sub(b.long_val).cs_cast(),
                    ));
                }
                _ => {}
            },

            EffectArgumentType::Int64 => match b.r#type {
                EffectArgumentType::Double => {
                    return Some(Self::from_long(
                        (CsCast::<f64>::cs_cast(a.long_val) - b.double_val).cs_cast(),
                    ));
                }
                EffectArgumentType::Int => {
                    return Some(Self::from_long(
                        a.long_val.wrapping_sub(i64::from(b.int_val)),
                    ))
                }
                EffectArgumentType::Int64 => {
                    return Some(Self::from_long(a.long_val.wrapping_sub(b.long_val)))
                }
                _ => {}
            },

            _ => {}
        }

        log::error!(
            "EffectArgument.Subtract() - invalid type {}, {}",
            a.r#type,
            b.r#type
        );

        None
    }

    /// `a * b`.
    // ACE: EffectArgument.op_Multiply
    #[must_use]
    pub fn op_multiply(a: &EffectArgument, b: &EffectArgument) -> Option<EffectArgument> {
        match a.r#type {
            EffectArgumentType::Double => match b.r#type {
                EffectArgumentType::Double => {
                    return Some(Self::from_double(a.double_val * b.double_val))
                }
                EffectArgumentType::Int => {
                    return Some(Self::from_double(a.double_val * f64::from(b.int_val)))
                }
                EffectArgumentType::Int64 => {
                    return Some(Self::from_double(
                        a.double_val * CsCast::<f64>::cs_cast(b.long_val),
                    ));
                }
                _ => {}
            },

            EffectArgumentType::Int => match b.r#type {
                EffectArgumentType::Double => {
                    return Some(Self::from_int(
                        (f64::from(a.int_val) * b.double_val).cs_cast(),
                    ));
                }
                EffectArgumentType::Int => {
                    return Some(Self::from_int(a.int_val.wrapping_mul(b.int_val)))
                }
                EffectArgumentType::Int64 => {
                    return Some(Self::from_int(
                        i64::from(a.int_val).wrapping_mul(b.long_val).cs_cast(),
                    ));
                }
                _ => {}
            },

            EffectArgumentType::Int64 => match b.r#type {
                EffectArgumentType::Double => {
                    return Some(Self::from_long(
                        (CsCast::<f64>::cs_cast(a.long_val) * b.double_val).cs_cast(),
                    ));
                }
                EffectArgumentType::Int => {
                    return Some(Self::from_long(
                        a.long_val.wrapping_mul(i64::from(b.int_val)),
                    ))
                }
                EffectArgumentType::Int64 => {
                    return Some(Self::from_long(a.long_val.wrapping_mul(b.long_val)))
                }
                _ => {}
            },

            _ => {}
        }

        log::error!(
            "EffectArgument.Multiply() - invalid type {}, {}",
            a.r#type,
            b.r#type
        );

        None
    }

    /// `a / b`. A zero divisor returns `a` itself. Integer division truncates; `int.MinValue / -1`
    /// throws `OverflowException` in C# and panics here.
    // ACE: EffectArgument.op_Division
    #[must_use]
    pub fn op_division(a: &EffectArgument, b: &EffectArgument) -> Option<EffectArgument> {
        const OVERFLOW: &str = "OverflowException: EffectArgument division overflow";

        match a.r#type {
            EffectArgumentType::Double => match b.r#type {
                EffectArgumentType::Double => {
                    return Some(if b.double_val != 0.0 {
                        Self::from_double(a.double_val / b.double_val)
                    } else {
                        a.clone()
                    });
                }
                EffectArgumentType::Int => {
                    return Some(if b.int_val != 0 {
                        Self::from_double(a.double_val / f64::from(b.int_val))
                    } else {
                        a.clone()
                    });
                }
                EffectArgumentType::Int64 => {
                    return Some(if b.long_val != 0 {
                        Self::from_double(a.double_val / CsCast::<f64>::cs_cast(b.long_val))
                    } else {
                        a.clone()
                    });
                }
                _ => {}
            },

            EffectArgumentType::Int => match b.r#type {
                EffectArgumentType::Double => {
                    return Some(if b.double_val != 0.0 {
                        Self::from_int((f64::from(a.int_val) / b.double_val).cs_cast())
                    } else {
                        a.clone()
                    });
                }
                EffectArgumentType::Int => {
                    return Some(if b.int_val != 0 {
                        Self::from_int(a.int_val.checked_div(b.int_val).expect(OVERFLOW))
                    } else {
                        a.clone()
                    });
                }
                EffectArgumentType::Int64 => {
                    return Some(if b.long_val != 0 {
                        Self::from_int(
                            i64::from(a.int_val)
                                .checked_div(b.long_val)
                                .expect(OVERFLOW)
                                .cs_cast(),
                        )
                    } else {
                        a.clone()
                    });
                }
                _ => {}
            },

            EffectArgumentType::Int64 => match b.r#type {
                EffectArgumentType::Double => {
                    return Some(if b.double_val != 0.0 {
                        Self::from_long(
                            (CsCast::<f64>::cs_cast(a.long_val) / b.double_val).cs_cast(),
                        )
                    } else {
                        a.clone()
                    });
                }
                EffectArgumentType::Int => {
                    return Some(if b.int_val != 0 {
                        Self::from_long(
                            a.long_val
                                .checked_div(i64::from(b.int_val))
                                .expect(OVERFLOW),
                        )
                    } else {
                        a.clone()
                    });
                }
                EffectArgumentType::Int64 => {
                    return Some(if b.long_val != 0 {
                        Self::from_long(a.long_val.checked_div(b.long_val).expect(OVERFLOW))
                    } else {
                        a.clone()
                    });
                }
                _ => {}
            },

            _ => {}
        }

        log::error!(
            "EffectArgument.Divide() - invalid type {}, {}",
            a.r#type,
            b.r#type
        );

        None
    }

    /// `a < b`: false (after logging) when the types differ.
    // ACE: EffectArgument.op_LessThan
    #[must_use]
    pub fn op_less_than(a: &EffectArgument, b: &EffectArgument) -> bool {
        if a.r#type != b.r#type {
            log::error!(
                "EffectArgument.LessThan() - type mismatch {} {}",
                a.r#type,
                b.r#type
            );
            return false;
        }

        match a.r#type {
            EffectArgumentType::Double => a.double_val < b.double_val,
            EffectArgumentType::Int => a.int_val < b.int_val,
            EffectArgumentType::Int64 => a.long_val < b.long_val,
            _ => false,
        }
    }

    /// `a > b`: false (after logging) when the types differ.
    // ACE: EffectArgument.op_GreaterThan
    #[must_use]
    pub fn op_greater_than(a: &EffectArgument, b: &EffectArgument) -> bool {
        if a.r#type != b.r#type {
            log::error!(
                "EffectArgument.GreaterThan() - type mismatch {} {}",
                a.r#type,
                b.r#type
            );
            return false;
        }

        match a.r#type {
            EffectArgumentType::Double => a.double_val > b.double_val,
            EffectArgumentType::Int => a.int_val > b.int_val,
            EffectArgumentType::Int64 => a.long_val > b.long_val,
            _ => false,
        }
    }
}
