//! Link-status snapshots and rolling averages — the packet counters the link-status panel
//! divides.
//!
//! # The whole chain
//!
//! ```text
//! the perf counters            one counter per network event
//! the connection walk          every >= 2 s, on a packet from the current server:
//!         skip unless 2 s have passed since this receiver's last heartbeat
//!         current.round_trip_delay         = the receiver's round-trip field
//!         current.time_since_last_got_data = local_time - last_got_data
//!         current.snapshot_duration        = local_time - local_time_of_snapshot
//!         add current status to the link-status averages
//!         notify every plugin of the heartbeat
//!         the snapshot is zeroed again
//!     the link-status holder's heartbeat (one of those plugins)
//!         last_heard_from_current_server = current_time
//!         packet_loss = average_packet_loss(averages)
//! the packet-loss accessor
//!         returns the cached float
//! the link-status panel's per-frame step
//!         read the packet-loss accessor
//!         add the packet-loss float variable with precision 2
//! ```
//!
//! # The average packet loss, verbatim
//!
//! It adds the received and sent packet totals; if that is `<= 0` it returns 0.0. Otherwise it
//! divides the NAKed plus retransmitted packet totals by it and doubles the quotient.
//!
//! i.e. `2 * (NAKed + retransmitted) / (received + sent)` — the *mean* of the two directions'
//! counts in the denominator, with the `/2` folded into the doubling. The four totals are
//! rolling 40-sample totals.
//!
//! **It is a ratio, not a percentage, despite the accessor's name and despite the shipped string
//! putting a `%` after it** (`ID_LinkStatus_PacketLoss` is
//! `"\n\n\nPacket loss for the last 10 sec: "` + `"%"`). The client initialises the packet
//! loss to **1.0f** — so a client that has heard nothing from a server
//! reads 1.0, which is 100 % loss on the ratio reading and is the sentinel that makes sense of
//! the arithmetic. The display is retail's, wrong label and all; this crate reports the ratio and
//! the link-status panel formats it.
//!
//! **The window is 40 heartbeats, not ten seconds.** The string says ten; the rolling average
//! keeps forty samples and connection processing gates at two seconds, so the real
//! window is up to eighty seconds of traffic. The string is retail's and is left alone.

/// [`Averager`] — a ring of `N` samples and their running total.
///
/// The original ring's `unsigned short`, `unsigned long`, and `double` instantiations share one
/// update rule. Below `N` samples it appends and adds; at `N` it subtracts the sample at `first`, writes
/// the new one over it, advances the index modulo `N` and adds. The total is a `double` and the
/// samples are whatever `T` is, which is why an overflowing `unsigned short` counter still sums
/// correctly.
#[derive(Debug, Clone)]
pub struct Averager<const N: usize> {
    samples: [f64; N],
    /// Running total.
    total: f64,
    /// Number of retained samples.
    count: u16,
    /// Index of the oldest sample.
    first: u16,
}

impl<const N: usize> Default for Averager<N> {
    fn default() -> Self {
        Self {
            samples: [0.0; N],
            total: 0.0,
            count: 0,
            first: 0,
        }
    }
}

impl<const N: usize> Averager<N> {
    /// Add one sample to the bounded ring.
    pub fn add_sample(&mut self, v: f64) {
        if usize::from(self.count) == N {
            let i = usize::from(self.first);
            self.total -= self.samples[i];
            self.samples[i] = v;
            // For retail's two- and four-sample averagers the step is `% 2` and `% 4`; `N` is 40
            // for the four counters this file's consumer reads. Either way it is `(first + 1) % N`.
            self.first = u16::try_from((usize::from(self.first) + 1) % N).unwrap_or(0);
        } else {
            self.samples[usize::from(self.count)] = v;
            self.count = self.count.saturating_add(1);
        }
        self.total += v;
    }

    /// The sum of the last `min(count, N)` samples.
    #[must_use]
    pub fn total(&self) -> f64 {
        self.total
    }

    /// Number of retained samples.
    #[must_use]
    pub fn count(&self) -> u16 {
        self.count
    }
}

/// The link-status snapshot — 28 bytes, one heartbeat's worth of counters.
///
/// The four unsigned-short counters receive these
/// network performance-counter events:
///
/// | counter | enum | field |
/// |---|---|---|
/// | packets sent | 9 | [`Self::pkts_sent`] |
/// | retransmits sent | 10 | [`Self::pkts_retransmitted`] |
/// | packets received (optional headers only / headers and data / data only) | 3, 4, 5 | [`Self::pkts_received`] |
/// | acks received | 6 | [`Self::pkts_naked`] |
///
/// The last row is the one to read twice: retail's acks-received counter is bumped for a
/// **`RequestRetransmit`** arriving from the peer — a NAK — which is why it lands in
/// the NAK count.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Snapshot {
    /// Round-trip delay, seconds.
    pub round_trip_delay: f32,
    /// Packets sent.
    pub pkts_sent: u16,
    /// Packets retransmitted.
    pub pkts_retransmitted: u16,
    /// Packets received.
    pub pkts_received: u16,
    /// Packets NAKed.
    pub pkts_naked: u16,
    /// Bytes sent.
    pub bytes_sent: u32,
    /// Bytes received.
    pub bytes_received: u32,
    /// Time since data was last received, seconds.
    pub time_since_last_got_data: f32,
    /// Snapshot duration, seconds.
    pub snapshot_duration: f32,
}

/// The compiled-in initial packet-loss value. Initialization stores the float `1.0` into the
/// packet-loss cache.
///
/// **1.0, not 0.0.** A client that has heard nothing yet reports total loss, and the panel shows
/// `1.00` until the first heartbeat; four question marks never appear on this line. The panel
/// always formats the packet-loss float, while the four-question-mark
/// fallback belongs to the **ping** line's non-positive round-trip-time arm.
pub const INITIAL_PACKET_LOSS: f32 = 1.0;

/// The connection walk's gate — the double value 2.0.
/// A snapshot is taken at most once every two seconds.
pub const HEARTBEAT_INTERVAL: f64 = 2.0;

/// How many samples the four packet counters keep — forty unsigned-counter samples.
pub const PACKET_SAMPLES: usize = 40;

/// The 536-byte rolling link-status averages record, of which this carries the fields consumers read.
#[derive(Debug, Clone, Default)]
pub struct LinkStatusAverages {
    /// The last snapshot added.
    pub snapshot: Snapshot,
    /// Local time of the last snapshot.
    pub local_time_of_snapshot: f64,
    /// Four round-trip-delay samples.
    pub round_trip_delays: Averager<4>,
    /// Forty packet-sent samples.
    pub pkts_sent: Averager<PACKET_SAMPLES>,
    /// Forty retransmitted-packet samples.
    pub pkts_retransmitted: Averager<PACKET_SAMPLES>,
    /// Forty received-packet samples.
    pub pkts_received: Averager<PACKET_SAMPLES>,
    /// Forty NAK samples.
    pub pkts_naked: Averager<PACKET_SAMPLES>,
    /// Two byte-sent samples.
    pub bytes_sent: Averager<2>,
    /// Two byte-received samples.
    pub bytes_received: Averager<2>,
    /// Two snapshot-duration samples.
    pub time_diffs: Averager<2>,
}

impl LinkStatusAverages {
    /// Add one snapshot.
    ///
    /// The head of the function computes the duration sample, the one piece of the
    /// arithmetic that is not a straight copy:
    /// The sample is the snapshot's duration by default; it is forced to 1.0 on the very first
    /// snapshot and whenever the duration is zero, and the snapshot time is then stored.
    ///
    /// It then copies the 28-byte snapshot; the six packet/byte
    /// averagers take their fields and the duration ring takes the sample. The copied
    /// round-trip delay is deliberately **not** sampled here: EchoResponse calls
    /// [`Self::on_ping_response`] at packet-arrival time.
    pub fn add_snapshot(&mut self, s: &Snapshot, local_time: f64) {
        let sample = if self.local_time_of_snapshot == 0.0 || f64::from(s.snapshot_duration) == 0.0
        {
            1.0
        } else {
            f64::from(s.snapshot_duration)
        };
        self.snapshot = *s;
        self.local_time_of_snapshot = local_time;
        self.pkts_sent.add_sample(f64::from(s.pkts_sent));
        self.pkts_retransmitted
            .add_sample(f64::from(s.pkts_retransmitted));
        self.pkts_received.add_sample(f64::from(s.pkts_received));
        self.pkts_naked.add_sample(f64::from(s.pkts_naked));
        self.bytes_sent.add_sample(f64::from(s.bytes_sent));
        self.bytes_received.add_sample(f64::from(s.bytes_received));
        self.time_diffs.add_sample(sample);
    }

    /// The ping-response sample.
    ///
    /// The entire body adds one value to the four-sample round-trip ring. It is reached
    /// immediately by a current-world EchoResponse, independently of the two-second snapshot.
    pub fn on_ping_response(&mut self, sample: f32) {
        self.round_trip_delays.add_sample(f64::from(sample));
    }

    /// The average packet loss, transcribed in the module header.
    ///
    /// `2 * (NAKed + retransmitted) / (received + sent)`, and `0.0` when the denominator is not
    /// strictly positive.
    #[must_use]
    pub fn average_packet_loss(&self) -> f64 {
        let denominator = self.pkts_received.total() + self.pkts_sent.total();
        if denominator <= 0.0 {
            return 0.0;
        }
        let numerator = self.pkts_naked.total() + self.pkts_retransmitted.total();
        2.0 * (numerator / denominator)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the bounded ring drops the oldest sample once
    /// `N` are held, and the total follows.
    #[test]
    fn the_averager_is_a_ring_of_n_and_its_running_total() {
        let mut a: Averager<3> = Averager::default();
        for v in [1.0, 2.0, 3.0] {
            a.add_sample(v);
        }
        assert_eq!(a.count(), 3);
        assert!((a.total() - 6.0).abs() < f64::EPSILON);
        // The fourth drops the first.
        a.add_sample(4.0);
        assert_eq!(a.count(), 3, "the sample count saturates at N");
        assert!((a.total() - 9.0).abs() < f64::EPSILON, "2 + 3 + 4");
        a.add_sample(5.0);
        assert!((a.total() - 12.0).abs() < f64::EPSILON, "3 + 4 + 5");
    }

    /// Oracle: the average packet loss, both arms.
    #[test]
    fn the_loss_is_twice_the_nak_and_retransmit_share_of_both_directions() {
        let mut a = LinkStatusAverages::default();
        // Nothing measured: the denominator is 0, so the `fcom`/`jp` arm returns 0.0.
        assert!((a.average_packet_loss() - 0.0).abs() < f64::EPSILON);

        // 100 sent, 100 received, 5 NAKed and 5 retransmitted: 2 * 10 / 200 = 0.10.
        a.add_snapshot(
            &Snapshot {
                pkts_sent: 100,
                pkts_received: 100,
                pkts_naked: 5,
                pkts_retransmitted: 5,
                snapshot_duration: 2.0,
                ..Snapshot::default()
            },
            2.0,
        );
        assert!(
            (a.average_packet_loss() - 0.10).abs() < 1e-9,
            "{}",
            a.average_packet_loss()
        );

        // A clean second heartbeat halves it, because the window is the sum of both.
        a.add_snapshot(
            &Snapshot {
                pkts_sent: 100,
                pkts_received: 100,
                snapshot_duration: 2.0,
                ..Snapshot::default()
            },
            4.0,
        );
        assert!(
            (a.average_packet_loss() - 0.05).abs() < 1e-9,
            "{}",
            a.average_packet_loss()
        );
    }

    /// The window is **40 heartbeats** — the string's "last 10 sec" is not the arithmetic.
    #[test]
    fn the_window_is_forty_samples_and_the_forty_first_evicts_the_first() {
        let mut a = LinkStatusAverages::default();
        let lossy = Snapshot {
            pkts_sent: 10,
            pkts_received: 10,
            pkts_naked: 10,
            snapshot_duration: 2.0,
            ..Snapshot::default()
        };
        let clean = Snapshot {
            pkts_sent: 10,
            pkts_received: 10,
            snapshot_duration: 2.0,
            ..Snapshot::default()
        };
        a.add_snapshot(&lossy, 2.0);
        for i in 1..PACKET_SAMPLES {
            #[allow(clippy::cast_precision_loss)]
            a.add_snapshot(&clean, 2.0 * (i as f64 + 1.0));
        }
        // 40 samples: 10 NAKed out of 800 -> 2 * 10 / 800.
        assert!((a.average_packet_loss() - 0.025).abs() < 1e-9);
        // The forty-first evicts the lossy one entirely.
        a.add_snapshot(&clean, 100.0);
        assert!(
            a.average_packet_loss().abs() < 1e-9,
            "the lossy sample must have left the ring: {}",
            a.average_packet_loss()
        );
    }

    /// Oracle: the packet-loss field starts at `1.0f`, not zero.
    #[test]
    fn the_initial_packet_loss_is_one() {
        assert!((INITIAL_PACKET_LOSS - 1.0).abs() < f32::EPSILON);
        assert!((HEARTBEAT_INTERVAL - 2.0).abs() < f64::EPSILON);
        assert_eq!(PACKET_SAMPLES, 40);
    }

    /// The snapshot copies the round-trip delay, but only the ping response
    /// samples the round-trip-delay ring.
    #[test]
    fn heartbeat_copies_rtt_without_sampling_the_ping_ring() {
        let mut a = LinkStatusAverages::default();
        a.add_snapshot(
            &Snapshot {
                round_trip_delay: 1.25,
                ..Snapshot::default()
            },
            2.0,
        );
        assert_eq!(a.snapshot.round_trip_delay, 1.25);
        assert_eq!(a.round_trip_delays.count(), 0);

        a.on_ping_response(1.25);
        assert_eq!(a.round_trip_delays.count(), 1);
        assert_eq!(a.round_trip_delays.total(), 1.25);
    }
}
