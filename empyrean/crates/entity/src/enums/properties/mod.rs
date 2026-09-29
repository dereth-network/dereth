//! ACE's `Source/ACE.Entity/Enum/Properties` enums, one module per C# file.
// @generated from ACE's `Source/ACE.Entity/Enum/Properties` folder; do not edit by hand

use super::support;

#[rustfmt::skip]
mod position_type;
#[rustfmt::skip]
mod property_attribute;
#[rustfmt::skip]
mod property_attribute2nd;
#[rustfmt::skip]
mod property_bool;
#[rustfmt::skip]
mod property_data_id;
#[rustfmt::skip]
mod property_float;
#[rustfmt::skip]
mod property_instance_id;
#[rustfmt::skip]
mod property_int;
#[rustfmt::skip]
mod property_int64;
#[rustfmt::skip]
mod property_string;
#[rustfmt::skip]
mod property_type;

pub use position_type::PositionType;
pub use property_attribute::PropertyAttribute;
pub use property_attribute2nd::PropertyAttribute2nd;
pub use property_bool::PropertyBool;
pub use property_data_id::PropertyDataId;
pub use property_float::PropertyFloat;
pub use property_instance_id::PropertyInstanceId;
pub use property_int::PropertyInt;
pub use property_int64::PropertyInt64;
pub use property_string::PropertyString;
pub use property_type::PropertyType;
