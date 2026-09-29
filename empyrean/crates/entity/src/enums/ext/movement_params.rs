// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/MovementParams.cs

/// ACE's `MovementParamsExtensions` static class.
pub mod movement_params_extensions {
    use crate::enums::MovementParams;

    // ACE: MovementParamsExtensions.Default
    // DIVERGE: a `const`; ACE's is a public static field that nothing assigns.
    pub const DEFAULT: MovementParams = MovementParams(
        MovementParams::CanWalk.0
            | MovementParams::CanRun.0
            | MovementParams::CanSideStep.0
            | MovementParams::CanWalkBackwards.0
            | MovementParams::MoveTowards.0
            | MovementParams::UseSpheres.0
            | MovementParams::SetHoldKey.0
            | MovementParams::ModifyRawState.0
            | MovementParams::ModifyInterpretedState.0
            | MovementParams::CancelMoveTo.0,
    );
}
