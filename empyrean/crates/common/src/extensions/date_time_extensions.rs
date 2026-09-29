// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Common/Extensions/DateTimeExtensions.cs
//! `DateTimeExtensions`.

use crate::dotnet::DotNetDateTime;

// ACE: DateTimeExtensions.ToCommonString
/// `dateTime.ToString("yyyy-MM-dd h:mm:ss tt")` in `en-US`.
#[must_use]
pub fn to_common_string(date_time: DotNetDateTime) -> String {
    date_time.format("yyyy-MM-dd h:mm:ss tt")
}
