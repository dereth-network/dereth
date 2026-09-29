//! Hand ports of the small helper classes that live next to the enums in `Source/ACE.Entity/Enum`.
//!
//! One module per C# file, with ACE's extension methods as inherent methods on the generated
//! enum and ACE's static classes as modules of the same name in snake case. Not ported, because
//! the generator replaces it: `AttributeExtensions.GetAttributeOfType` (reflection; every attribute
//! it reads is a generated query on the enum).

mod attack_height;
mod attack_type;
mod augmentation_type;
mod character_option;
mod chat_message_type;
mod damage_type;
mod environ_change_type;
mod faction_bits;
mod heritage_group;
mod hook_group_type;
mod motion_command;
mod movement_params;
mod properties;
mod property_attribute;
mod property_data_id;
mod property_int;
mod quadrant_index;
mod skill;
mod spell_id;
mod squelch_mask;
mod usable;

pub use augmentation_type::aug_type_helper;
pub use character_option::character_option_extensions;
pub use motion_command::motion_command_helper;
pub use movement_params::movement_params_extensions;
pub use properties::{assessment_properties, ephemeral_properties, send_on_login_properties};
pub use quadrant_index::quadrant_index_extensions;
pub use skill::{skill_extensions, skill_helper};
pub use spell_id::spell_extensions;

/// The fallback of ACE's `ToSentence` helpers:
/// `new string(s.ToCharArray().SelectMany((c, i) => i > 0 && char.IsUpper(c) ? new[] { ' ', c } :
/// new[] { c }).ToArray())`, which puts a space before every upper-case letter but the first.
fn spaced_capitals(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && c.is_uppercase() {
            out.push(' ');
        }
        out.push(c);
    }
    out
}
