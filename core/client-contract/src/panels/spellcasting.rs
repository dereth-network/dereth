//! The endowment equipment location.
//!
//! One constant out of `dereth_ui_screens::panels::spellcasting`: the equipment location the
//! spellcasting panel reads the endowment out of, which `dereth_client_shell::hud` looks up in the object
//! table.

/// The equipment location the spellcasting panel's endowment icon reads out of: the object the
/// player wears at location `0x0100_0000`.
pub const ENDOWMENT_LOCATION: u32 = 0x0100_0000;
