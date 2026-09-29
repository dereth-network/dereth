// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Mutations/Effect.cs
//! Port of `Source/ACE.Server/Entity/Mutations/Effect.cs`.

use empyrean_entity::enums::{MutationEffectType, PropertyString};

use super::effect_argument::EffectArgument;
use crate::world_objects::world_object::WorldObject;

/// ACE `Effect`: one line of a mutation script, `Quality <op> Arg1 [<op2> Arg2]`.
// ACE: Effect
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Effect {
    pub quality: Option<EffectArgument>,
    pub r#type: MutationEffectType,
    pub arg1: Option<EffectArgument>,
    pub arg2: Option<EffectArgument>,
}

/// An operand of an operator that ACE dereferences: a `null` from an earlier operator throws.
fn deref(arg: Option<&EffectArgument>) -> &EffectArgument {
    arg.expect("NullReferenceException: EffectArgument operator on a null operand")
}

impl Effect {
    /// Resolves the quality and both arguments against `wo`, applies the operator and stores the
    /// result back into the quality. The effect itself is not changed (it is shared).
    // ACE: Effect.TryMutate
    pub fn try_mutate(&self, wo: &mut WorldObject) -> bool {
        // type:enum - invalid, double, int32, quality (2 int32s: type and quality), float range (min, max), variable index (int32)
        // a=b,a+=b,a-=b,a*=b,a/=b,a=a<b?b:a+c,a=a>b?b:a-c,a+=b*c,a+=b/c,a-=b*c,a-=b/c,a=b+c,a=b-c,a=b*c,a=b/c

        // do not make changes to the members since this object will be reused

        let mut result = EffectArgument::copy_of(self.quality.as_ref());
        let mut arg1 = EffectArgument::copy_of(self.arg1.as_ref());
        let mut arg2 = EffectArgument::copy_of(self.arg2.as_ref());

        result.resolve_value(wo);
        arg1.resolve_value(wo);
        arg2.resolve_value(wo);

        if !self.validate(wo, &result, &arg1, &arg2, self.r#type) {
            return false;
        }

        let result: Option<EffectArgument> = match self.r#type {
            MutationEffectType::Assign => Some(arg1),

            MutationEffectType::Add => EffectArgument::op_addition(&result, &arg1),

            MutationEffectType::Subtract => EffectArgument::op_subtraction(&result, &arg1),

            MutationEffectType::Multiply => EffectArgument::op_multiply(&result, &arg1),

            MutationEffectType::Divide => EffectArgument::op_division(&result, &arg1),

            MutationEffectType::AtLeastAdd => {
                if !result.is_valid || EffectArgument::op_less_than(&result, &arg1) {
                    Some(arg1)
                } else {
                    EffectArgument::op_addition(&result, &arg2)
                }
            }

            MutationEffectType::AtMostSubtract => {
                if !result.is_valid || EffectArgument::op_greater_than(&result, &arg1) {
                    Some(arg1)
                } else {
                    EffectArgument::op_subtraction(&result, &arg2)
                }
            }

            MutationEffectType::AddMultiply => {
                let product = EffectArgument::op_multiply(&arg1, &arg2);
                EffectArgument::op_addition(&result, deref(product.as_ref()))
            }

            MutationEffectType::AddDivide => {
                let quotient = EffectArgument::op_division(&arg1, &arg2);
                EffectArgument::op_addition(&result, deref(quotient.as_ref()))
            }

            MutationEffectType::SubtractMultiply => {
                let product = EffectArgument::op_multiply(&arg1, &arg2);
                EffectArgument::op_subtraction(&result, deref(product.as_ref()))
            }

            MutationEffectType::SubtractDivide => {
                let quotient = EffectArgument::op_division(&arg1, &arg2);
                EffectArgument::op_subtraction(&result, deref(quotient.as_ref()))
            }

            MutationEffectType::AssignAdd => EffectArgument::op_addition(&arg1, &arg2),

            MutationEffectType::AssignSubtract => EffectArgument::op_subtraction(&arg1, &arg2),

            MutationEffectType::AssignMultiply => EffectArgument::op_multiply(&arg1, &arg2),

            // ACE-BUG: Effect.TryMutate - AssignDivide multiplies (`result = arg1 * arg2`) instead of
            // dividing; no shipped mutation script uses `a = b / c`, so loot is unaffected.
            MutationEffectType::AssignDivide => EffectArgument::op_multiply(&arg1, &arg2),

            _ => Some(result),
        };

        self.quality
            .as_ref()
            .expect("NullReferenceException: Effect.Quality is null")
            .store_value(wo, result.as_ref());

        true
    }

    /// Argument 1 must resolve; argument 2 too for the two-operator effects. The result (the
    /// quality's current value) may be missing.
    // ACE: Effect.Validate
    pub fn validate(
        &self,
        wo: &WorldObject,
        _result: &EffectArgument,
        arg1: &EffectArgument,
        arg2: &EffectArgument,
        r#type: MutationEffectType,
    ) -> bool {
        /*if (!result.IsValid)
        {
            log.Error($"{wo.Name} ({wo.Guid}).TryMutate({type}) - result invalid");
            return false;
        }*/

        if !arg1.is_valid {
            log::error!(
                "{} ({}).TryMutate({}) - argument 1 invalid",
                wo.get_property(PropertyString::Name).unwrap_or_default(),
                wo.guid,
                r#type
            );
            return false;
        }

        match r#type {
            MutationEffectType::AtLeastAdd
            | MutationEffectType::AtMostSubtract
            | MutationEffectType::AddMultiply
            | MutationEffectType::AddDivide
            | MutationEffectType::SubtractMultiply
            | MutationEffectType::SubtractDivide
            | MutationEffectType::AssignAdd
            | MutationEffectType::AssignSubtract
            | MutationEffectType::AssignMultiply
            | MutationEffectType::AssignDivide
                if !arg2.is_valid =>
            {
                log::error!(
                    "{} ({}).TryMutate({}) - argument 2 invalid",
                    wo.get_property(PropertyString::Name).unwrap_or_default(),
                    wo.guid,
                    r#type
                );
                return false;
            }
            _ => {}
        }

        true
    }
}
