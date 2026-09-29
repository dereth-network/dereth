// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Models/IWeenie.cs
//! `IWeenie`: what [`Weenie`](crate::models::Weenie) and [`Biota`](crate::models::Biota) share.
//!
//! ACE uses the interface only as the common shape of the two classes. Here it also carries the
//! seven typed property bags, so that one generic `get_property`/`set_property` (keyed by
//! [`PropertyKey`]) replaces ACE's seven overloads of each.

use std::hash::Hash;

use empyrean_common::dotnet::DotNetDict;

use crate::enums::{
    PropertyBool, PropertyDataId, PropertyFloat, PropertyInstanceId, PropertyInt, PropertyInt64,
    PropertyString, WeenieType,
};

/// ACE: IWeenie
pub trait IWeenie {
    // ACE: IWeenie.WeenieClassId
    fn weenie_class_id(&self) -> u32;
    // ACE: IWeenie.WeenieType
    fn weenie_type(&self) -> WeenieType;

    // ACE: IWeenie.PropertiesBool
    fn properties_bool(&self) -> &Option<DotNetDict<PropertyBool, bool>>;
    // ACE: IWeenie.PropertiesBool
    fn properties_bool_mut(&mut self) -> &mut Option<DotNetDict<PropertyBool, bool>>;
    // ACE: IWeenie.PropertiesDID
    fn properties_did(&self) -> &Option<DotNetDict<PropertyDataId, u32>>;
    // ACE: IWeenie.PropertiesDID
    fn properties_did_mut(&mut self) -> &mut Option<DotNetDict<PropertyDataId, u32>>;
    // ACE: IWeenie.PropertiesFloat
    fn properties_float(&self) -> &Option<DotNetDict<PropertyFloat, f64>>;
    // ACE: IWeenie.PropertiesFloat
    fn properties_float_mut(&mut self) -> &mut Option<DotNetDict<PropertyFloat, f64>>;
    // ACE: IWeenie.PropertiesIID
    fn properties_iid(&self) -> &Option<DotNetDict<PropertyInstanceId, u32>>;
    // ACE: IWeenie.PropertiesIID
    fn properties_iid_mut(&mut self) -> &mut Option<DotNetDict<PropertyInstanceId, u32>>;
    // ACE: IWeenie.PropertiesInt
    fn properties_int(&self) -> &Option<DotNetDict<PropertyInt, i32>>;
    // ACE: IWeenie.PropertiesInt
    fn properties_int_mut(&mut self) -> &mut Option<DotNetDict<PropertyInt, i32>>;
    // ACE: IWeenie.PropertiesInt64
    fn properties_int64(&self) -> &Option<DotNetDict<PropertyInt64, i64>>;
    // ACE: IWeenie.PropertiesInt64
    fn properties_int64_mut(&mut self) -> &mut Option<DotNetDict<PropertyInt64, i64>>;
    // ACE: IWeenie.PropertiesString
    fn properties_string(&self) -> &Option<DotNetDict<PropertyString, String>>;
    // ACE: IWeenie.PropertiesString
    fn properties_string_mut(&mut self) -> &mut Option<DotNetDict<PropertyString, String>>;
}

/// A property enum whose values live in one of the seven typed bags of an [`IWeenie`]. Not ACE:
/// this is the static dispatch behind ACE's `GetProperty`/`SetProperty`/`TryRemoveProperty`
/// overload sets.
pub trait PropertyKey: Copy + Eq + Hash {
    /// The C# value type (`bool`, `uint`, `double`, `int`, `long`, `string`).
    type Value: Clone + PartialEq;

    /// The bag this property lives in.
    fn bag<W: IWeenie + ?Sized>(w: &W) -> &Option<DotNetDict<Self, Self::Value>>;

    /// The bag this property lives in, mutably.
    fn bag_mut<W: IWeenie + ?Sized>(w: &mut W) -> &mut Option<DotNetDict<Self, Self::Value>>;
}

macro_rules! property_key {
    ($key:ty, $value:ty, $get:ident, $get_mut:ident) => {
        impl PropertyKey for $key {
            type Value = $value;

            fn bag<W: IWeenie + ?Sized>(w: &W) -> &Option<DotNetDict<Self, Self::Value>> {
                w.$get()
            }

            fn bag_mut<W: IWeenie + ?Sized>(
                w: &mut W,
            ) -> &mut Option<DotNetDict<Self, Self::Value>> {
                w.$get_mut()
            }
        }
    };
}

property_key!(PropertyBool, bool, properties_bool, properties_bool_mut);
property_key!(PropertyDataId, u32, properties_did, properties_did_mut);
property_key!(PropertyFloat, f64, properties_float, properties_float_mut);
property_key!(PropertyInstanceId, u32, properties_iid, properties_iid_mut);
property_key!(PropertyInt, i32, properties_int, properties_int_mut);
property_key!(PropertyInt64, i64, properties_int64, properties_int64_mut);
property_key!(
    PropertyString,
    String,
    properties_string,
    properties_string_mut
);

/// Implements [`IWeenie`] for a struct with ACE's field names.
macro_rules! impl_i_weenie {
    ($ty:ty) => {
        impl $crate::models::i_weenie::IWeenie for $ty {
            fn weenie_class_id(&self) -> u32 {
                self.weenie_class_id
            }
            fn weenie_type(&self) -> $crate::enums::WeenieType {
                self.weenie_type
            }
            fn properties_bool(&self) -> &Option<DotNetDict<$crate::enums::PropertyBool, bool>> {
                &self.properties_bool
            }
            fn properties_bool_mut(
                &mut self,
            ) -> &mut Option<DotNetDict<$crate::enums::PropertyBool, bool>> {
                &mut self.properties_bool
            }
            fn properties_did(&self) -> &Option<DotNetDict<$crate::enums::PropertyDataId, u32>> {
                &self.properties_did
            }
            fn properties_did_mut(
                &mut self,
            ) -> &mut Option<DotNetDict<$crate::enums::PropertyDataId, u32>> {
                &mut self.properties_did
            }
            fn properties_float(&self) -> &Option<DotNetDict<$crate::enums::PropertyFloat, f64>> {
                &self.properties_float
            }
            fn properties_float_mut(
                &mut self,
            ) -> &mut Option<DotNetDict<$crate::enums::PropertyFloat, f64>> {
                &mut self.properties_float
            }
            fn properties_iid(
                &self,
            ) -> &Option<DotNetDict<$crate::enums::PropertyInstanceId, u32>> {
                &self.properties_iid
            }
            fn properties_iid_mut(
                &mut self,
            ) -> &mut Option<DotNetDict<$crate::enums::PropertyInstanceId, u32>> {
                &mut self.properties_iid
            }
            fn properties_int(&self) -> &Option<DotNetDict<$crate::enums::PropertyInt, i32>> {
                &self.properties_int
            }
            fn properties_int_mut(
                &mut self,
            ) -> &mut Option<DotNetDict<$crate::enums::PropertyInt, i32>> {
                &mut self.properties_int
            }
            fn properties_int64(&self) -> &Option<DotNetDict<$crate::enums::PropertyInt64, i64>> {
                &self.properties_int64
            }
            fn properties_int64_mut(
                &mut self,
            ) -> &mut Option<DotNetDict<$crate::enums::PropertyInt64, i64>> {
                &mut self.properties_int64
            }
            fn properties_string(
                &self,
            ) -> &Option<DotNetDict<$crate::enums::PropertyString, String>> {
                &self.properties_string
            }
            fn properties_string_mut(
                &mut self,
            ) -> &mut Option<DotNetDict<$crate::enums::PropertyString, String>> {
                &mut self.properties_string
            }
        }
    };
}

pub(crate) use impl_i_weenie;
