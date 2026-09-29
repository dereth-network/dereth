// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/SessionTerminationDetails.cs

use empyrean_common::dotnet::DotNetDateTime;

use crate::enums::{SessionTerminationPhase, SessionTerminationReason};

/// ACE `SessionTerminationDetails`.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionTerminationDetails {
    pub extra_reason: String,
    pub termination_status: SessionTerminationPhase,
    pub reason: SessionTerminationReason,
    /// ACE keeps `DateTime.UtcNow.Ticks`; the date-time compares the same.
    pub termination_start_ticks: DotNetDateTime,
    /// Two seconds after the start: the time a terminating session has to flush its last packets.
    pub termination_end_ticks: DotNetDateTime,
}

impl SessionTerminationDetails {
    #[must_use]
    pub fn new(
        reason: SessionTerminationReason,
        extra_reason: String,
        utc_now: DotNetDateTime,
    ) -> Self {
        Self {
            extra_reason,
            termination_status: SessionTerminationPhase::Initialized,
            reason,
            termination_start_ticks: utc_now,
            // `new DateTime(DateTime.UtcNow.Ticks).AddSeconds(2).Ticks`.
            termination_end_ticks: utc_now.add_seconds(2.0),
        }
    }
}
