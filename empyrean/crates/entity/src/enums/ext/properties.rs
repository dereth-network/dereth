// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/Properties/AssessmentProperties.cs
//
// Also ports Properties/EphemeralProperties.cs and Properties/SendOnLoginProperties.cs. Each ACE
// class builds `HashSet`s by reflecting over the members that carry its attribute, in declaration
// order; the generator emits those members as `<Enum>::ASSESSMENT_PROPERTY`, `::EPHEMERAL` and
// `::SEND_ON_LOGIN` (same order), and `<Enum>::is_*` answers the `Contains` lookups. The sets here
// are those slices under ACE's names.
//
// DIVERGE: ACE's assessment and send-on-login sets are `HashSet<ushort>`; these are typed
// (`&[PropertyInt]`, ...). Every property enum is `ushort`-backed, so `.0` is ACE's element.

/// ACE's `AssessmentProperties`: the properties sent to the client on a successful appraisal.
pub mod assessment_properties {
    use crate::enums::*;

    // ACE: AssessmentProperties.PropertiesInt
    pub const PROPERTIES_INT: &[PropertyInt] = PropertyInt::ASSESSMENT_PROPERTY;
    // ACE: AssessmentProperties.PropertiesInt64
    pub const PROPERTIES_INT64: &[PropertyInt64] = PropertyInt64::ASSESSMENT_PROPERTY;
    // ACE: AssessmentProperties.PropertiesBool
    pub const PROPERTIES_BOOL: &[PropertyBool] = PropertyBool::ASSESSMENT_PROPERTY;
    // ACE: AssessmentProperties.PropertiesString
    pub const PROPERTIES_STRING: &[PropertyString] = PropertyString::ASSESSMENT_PROPERTY;
    // ACE: AssessmentProperties.PropertiesDouble
    pub const PROPERTIES_DOUBLE: &[PropertyFloat] = PropertyFloat::ASSESSMENT_PROPERTY;
    // ACE: AssessmentProperties.PropertiesDataId
    pub const PROPERTIES_DATA_ID: &[PropertyDataId] = PropertyDataId::ASSESSMENT_PROPERTY;
    /// No `PropertyInstanceId` member is `[AssessmentProperty]`, so ACE's set is empty.
    // ACE: AssessmentProperties.PropertiesInstanceId
    pub const PROPERTIES_INSTANCE_ID: &[PropertyInstanceId] = &[];
}

/// ACE's `EphemeralProperties`: the properties that are never saved to the shard.
pub mod ephemeral_properties {
    use crate::enums::*;

    // ACE: EphemeralProperties.PropertiesInt
    pub const PROPERTIES_INT: &[PropertyInt] = PropertyInt::EPHEMERAL;
    /// No `PropertyInt64` member is `[Ephemeral]`, so ACE's set is empty.
    // ACE: EphemeralProperties.PropertiesInt64
    pub const PROPERTIES_INT64: &[PropertyInt64] = &[];
    // ACE: EphemeralProperties.PropertiesBool
    pub const PROPERTIES_BOOL: &[PropertyBool] = PropertyBool::EPHEMERAL;
    // ACE: EphemeralProperties.PropertiesString
    pub const PROPERTIES_STRING: &[PropertyString] = PropertyString::EPHEMERAL;
    // ACE: EphemeralProperties.PropertiesDouble
    pub const PROPERTIES_DOUBLE: &[PropertyFloat] = PropertyFloat::EPHEMERAL;
    /// No `PropertyDataId` member is `[Ephemeral]`, so ACE's set is empty.
    // ACE: EphemeralProperties.PropertiesDataId
    pub const PROPERTIES_DATA_ID: &[PropertyDataId] = &[];
    // ACE: EphemeralProperties.PropertiesInstanceId
    pub const PROPERTIES_INSTANCE_ID: &[PropertyInstanceId] = PropertyInstanceId::EPHEMERAL;
    // ACE: EphemeralProperties.PositionTypes
    pub const POSITION_TYPES: &[PositionType] = PositionType::EPHEMERAL;
}

/// ACE's `SendOnLoginProperties`: the properties sent in the player description event.
pub mod send_on_login_properties {
    use crate::enums::*;

    // ACE: SendOnLoginProperties.PropertiesInt
    pub const PROPERTIES_INT: &[PropertyInt] = PropertyInt::SEND_ON_LOGIN;
    // ACE: SendOnLoginProperties.PropertiesInt64
    pub const PROPERTIES_INT64: &[PropertyInt64] = PropertyInt64::SEND_ON_LOGIN;
    // ACE: SendOnLoginProperties.PropertiesBool
    pub const PROPERTIES_BOOL: &[PropertyBool] = PropertyBool::SEND_ON_LOGIN;
    // ACE: SendOnLoginProperties.PropertiesString
    pub const PROPERTIES_STRING: &[PropertyString] = PropertyString::SEND_ON_LOGIN;
    // ACE: SendOnLoginProperties.PropertiesDouble
    pub const PROPERTIES_DOUBLE: &[PropertyFloat] = PropertyFloat::SEND_ON_LOGIN;
    // ACE: SendOnLoginProperties.PropertiesDataId
    pub const PROPERTIES_DATA_ID: &[PropertyDataId] = PropertyDataId::SEND_ON_LOGIN;
    // ACE: SendOnLoginProperties.PropertiesInstanceId
    pub const PROPERTIES_INSTANCE_ID: &[PropertyInstanceId] = PropertyInstanceId::SEND_ON_LOGIN;
}
