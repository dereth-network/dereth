// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/NetworkStatistics.cs

// ACE: NetworkStatistics, NetworkStatistics.Instance
/// ACE `NetworkStatistics`: aggregate packet, retransmit-request and CRC-error counters.
///
/// DIVERGE: ACE keeps one process-wide singleton (`Instance`, a `Lazy`) with `Interlocked`
/// counters. Here each [`crate::ServerNet`] owns its counters (`ServerNet::statistics`), so two
/// servers (or two tests) in one process do not share them. The one server process has one
/// `ServerNet`, so the numbers are the same.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NetworkStatistics {
    pub c2s_requests_for_retransmit_aggregate: i64,
    pub s2c_requests_for_retransmit_aggregate: i64,
    pub c2s_crc_errors_aggregate: i64,
    pub c2s_packets_aggregate: i64,
    pub s2c_packets_aggregate: i64,
}

impl NetworkStatistics {
    // ACE: NetworkStatistics.NetworkStatistics
    /// `new NetworkStatistics()`: every counter at zero.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    // ACE: NetworkStatistics.C2S_RequestsForRetransmit_Aggregate
    /// Aggregate client to server requests for retransmit.
    #[must_use]
    pub const fn c2s_requests_for_retransmit_aggregate(&self) -> i64 {
        self.c2s_requests_for_retransmit_aggregate
    }

    // ACE: NetworkStatistics.S2C_RequestsForRetransmit_Aggregate
    /// Aggregate server to client requests for retransmit.
    #[must_use]
    pub const fn s2c_requests_for_retransmit_aggregate(&self) -> i64 {
        self.s2c_requests_for_retransmit_aggregate
    }

    // ACE: NetworkStatistics.C2S_CRCErrors_Aggregate
    /// Aggregate client to server CRC errors.
    #[must_use]
    pub const fn c2s_crc_errors_aggregate(&self) -> i64 {
        self.c2s_crc_errors_aggregate
    }

    // ACE: NetworkStatistics.C2S_Packets_Aggregate
    /// Aggregate client to server packets.
    #[must_use]
    pub const fn c2s_packets_aggregate(&self) -> i64 {
        self.c2s_packets_aggregate
    }

    // ACE: NetworkStatistics.S2C_Packets_Aggregate
    /// Aggregate server to client packets.
    #[must_use]
    pub const fn s2c_packets_aggregate(&self) -> i64 {
        self.s2c_packets_aggregate
    }

    // ACE: NetworkStatistics.C2S_RequestsForRetransmit_Aggregate_Increment
    pub fn c2s_requests_for_retransmit_aggregate_increment(&mut self) -> i64 {
        self.c2s_requests_for_retransmit_aggregate += 1;
        self.c2s_requests_for_retransmit_aggregate
    }

    // ACE: NetworkStatistics.S2C_RequestsForRetransmit_Aggregate_Increment
    pub fn s2c_requests_for_retransmit_aggregate_increment(&mut self) -> i64 {
        self.s2c_requests_for_retransmit_aggregate += 1;
        self.s2c_requests_for_retransmit_aggregate
    }

    // ACE: NetworkStatistics.C2S_CRCErrors_Aggregate_Increment
    pub fn c2s_crc_errors_aggregate_increment(&mut self) -> i64 {
        self.c2s_crc_errors_aggregate += 1;
        self.c2s_crc_errors_aggregate
    }

    // ACE: NetworkStatistics.C2S_Packets_Aggregate_Increment
    pub fn c2s_packets_aggregate_increment(&mut self) -> i64 {
        self.c2s_packets_aggregate += 1;
        self.c2s_packets_aggregate
    }

    // ACE: NetworkStatistics.S2C_Packets_Aggregate_Increment
    pub fn s2c_packets_aggregate_increment(&mut self) -> i64 {
        self.s2c_packets_aggregate += 1;
        self.s2c_packets_aggregate
    }

    // ACE: NetworkStatistics.Summary
    #[must_use]
    pub fn summary(&self) -> String {
        #[allow(clippy::cast_precision_loss)]
        let ratio = |num: i64, den: i64| {
            if den < 1 {
                0.0
            } else {
                num as f64 / den as f64
            }
        };
        let rfr_s2c = ratio(
            self.s2c_requests_for_retransmit_aggregate,
            self.s2c_packets_aggregate,
        );
        let rfr_c2s = ratio(
            self.c2s_requests_for_retransmit_aggregate,
            self.c2s_packets_aggregate,
        );
        let crce_c2s = ratio(self.c2s_crc_errors_aggregate, self.c2s_packets_aggregate);
        format!(
            "\nnetwork statistics\npackets\nclient=>server: {}\nserver=>client: {}\nrequests for retransmit\nclient=>server: {} {}\nServer=>client: {} {}\nCRC errors\nclient=>server: {} {}\n",
            format_n0(self.c2s_packets_aggregate),
            format_n0(self.s2c_packets_aggregate),
            format_n0(self.c2s_requests_for_retransmit_aggregate),
            blank_zero_proportion(rfr_c2s),
            format_n0(self.s2c_requests_for_retransmit_aggregate),
            blank_zero_proportion(rfr_s2c),
            format_n0(self.c2s_crc_errors_aggregate),
            blank_zero_proportion(crce_c2s),
        )
    }
}

// ACE: NetworkStatistics.BlankZeroProportion
fn blank_zero_proportion(r: f64) -> String {
    if r == 0.0 {
        String::new()
    } else {
        empyrean_common::extensions::double_extensions::format_chance(r)
    }
}

/// .NET `ToString("N0")` in the invariant culture, for an integer: thousands separated by `,`.
fn format_n0(v: i64) -> String {
    let digits = v.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    if v < 0 {
        out.push('-');
    }
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}
