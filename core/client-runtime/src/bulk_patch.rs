//! The patch as one TCP stream (client divergence CD-bulk-patch).
//!
//! A server that knows it (Empyrean) answers a client that asks (`FLAG_BULK` in `0xF7E6`) with
//! `0xF7ED` in place of the patch's `0xF7E2`s: a port and a token. A thread connects to that port
//! at the server's address, sends the magic, the version and the token, and reads the records as
//! `DddData` bodies, each behind its length, until a length of 0. TCP's flow control paces it; the
//! records waiting for the frame loop are bounded by their bytes, so a client that applies them
//! more slowly than they arrive slows the stream rather than piling them up. The frame loop
//! applies each one as the `0xF7E2` it stands for. A stream that cannot be opened, breaks, stalls
//! or ends with records still pending is declined (`0xF7EE`) with those records, which the server
//! then sends as `0xF7E2`s. Any other server never offers one, and the patch is retail's.

/// How long a frame may spend applying the stream's records at the least: the rest of the frame
/// draws the screens and keeps the session's messages moving, so the progress shows and the
/// server's messages are answered while the patch is written. A frame that may apply any applies
/// at least one.
pub const FRAME_SHARE: std::time::Duration = std::time::Duration::from_millis(50);

/// How long a frame may spend applying the stream's records when frames are `spacing` apart at
/// the least: the whole spacing, and never less than `floor`. A frame whose work fills its spacing
/// has nothing left to sleep: a window in the background (ten frames a second), whose sleeps the
/// system may stretch far past what was asked, applies the patch in all of each frame rather than
/// waiting out a stretched sleep. One with no limit gives it `floor`, and draws in between.
#[must_use]
pub fn frame_share(
    floor: std::time::Duration,
    spacing: std::time::Duration,
) -> std::time::Duration {
    floor.max(spacing)
}

/// The clock a frame's share of the stream is measured by: the time since some fixed moment.
/// The client's reads the machine's clock; a test's moves as the test says.
pub struct WorkClock(Box<dyn FnMut() -> std::time::Duration + Send>);

impl WorkClock {
    /// A clock that reads `now`.
    pub fn new(now: impl FnMut() -> std::time::Duration + Send + 'static) -> Self {
        Self(Box::new(now))
    }

    /// The time since the clock's fixed moment.
    pub fn now(&mut self) -> std::time::Duration {
        (self.0)()
    }
}

impl Default for WorkClock {
    /// The machine's clock.
    fn default() -> Self {
        let start = web_time::Instant::now();
        Self::new(move || start.elapsed())
    }
}

impl std::fmt::Debug for WorkClock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorkClock")
    }
}

/// What the client does with an offered stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfferChoice {
    /// Open it.
    Take,
    /// Decline the whole offer, so the server sends its records as `0xF7E2`s.
    Decline,
    /// Leave it: a stream is already being read, and it brings the records.
    Ignore,
}

/// What a client does with `0xF7ED`: it opens the stream only when it can open one at all (the
/// browser's cannot), it asked for one in this exchange, and none is being read already; an offer
/// it did not ask for, or cannot take, is declined whole.
#[must_use]
pub fn offer_choice(can_stream: bool, asked: bool, reading: bool) -> OfferChoice {
    if reading {
        OfferChoice::Ignore
    } else if can_stream && asked {
        OfferChoice::Take
    } else {
        OfferChoice::Decline
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use stream::{BulkDownload, Limits};

#[cfg(not(target_arch = "wasm32"))]
mod stream {
    use std::io::{BufReader, Error, ErrorKind, Read, Write};
    use std::net::{SocketAddr, TcpStream};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, TryRecvError};
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::{Duration, Instant};

    use dereth_protocol::admin::{DddData, BULK_MAGIC, BULK_VERSION};

    use crate::ddd::Decoded;

    /// How long a stream may take over each step, and how much it may hold, before it is given up
    /// on or made to wait.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Limits {
        /// To connect.
        pub connect: Duration,
        /// To bring one whole record, its length and its body: a stream that stalls or trickles
        /// fails here.
        pub record: Duration,
        /// The bytes of records waiting for the frame loop at most, as they came and inflated; a
        /// record larger than this alone still goes.
        pub queued: usize,
        /// The largest record body the stream may carry.
        pub largest: usize,
    }

    impl Default for Limits {
        fn default() -> Self {
            Self {
                connect: Duration::from_secs(5),
                record: Duration::from_secs(30),
                queued: 64 << 20,
                largest: 64 << 20,
            }
        }
    }

    #[derive(Debug)]
    enum Msg {
        Record(Decoded),
        Done,
        Failed(String),
    }

    /// The bytes of records handed to the frame loop and not yet drained, and whether the frame
    /// loop has stopped reading.
    #[derive(Debug, Default)]
    struct Budget {
        state: Mutex<(usize, bool)>,
        freed: Condvar,
    }

    impl Budget {
        /// Wait until `n` more bytes fit under `most` (a record alone always fits); false when the
        /// frame loop has stopped reading.
        fn take(&self, n: usize, most: usize) -> bool {
            let Ok(mut s) = self.state.lock() else {
                return false;
            };
            while !s.1 && s.0 > 0 && s.0 + n > most {
                s = match self.freed.wait(s) {
                    Ok(s) => s,
                    Err(_) => return false,
                };
            }
            if s.1 {
                return false;
            }
            s.0 += n;
            true
        }

        fn give(&self, n: usize) {
            if let Ok(mut s) = self.state.lock() {
                s.0 = s.0.saturating_sub(n);
            }
            self.freed.notify_all();
        }

        fn close(&self) {
            if let Ok(mut s) = self.state.lock() {
                s.1 = true;
            }
            self.freed.notify_all();
        }

        fn held(&self) -> usize {
            self.state.lock().map_or(0, |s| s.0)
        }
    }

    /// What the stream's thread has read: its records and their bytes, and whether the stream
    /// has ended with every record it carries.
    #[derive(Debug, Default)]
    struct Arrived {
        records: AtomicU64,
        bytes: AtomicU64,
        all: AtomicBool,
    }

    /// A patch stream being read.
    #[derive(Debug)]
    pub struct BulkDownload {
        rx: Receiver<Msg>,
        /// A message [`Self::wait`] took off the channel, which [`Self::drain`] hands out first.
        next: Option<Msg>,
        budget: Arc<Budget>,
        arrived: Arc<Arrived>,
        end: Option<Result<(), String>>,
        started: Instant,
        records: u64,
        bytes: u64,
        /// The record bytes the offer said the stream carries, 0 when it said none.
        expected: u64,
        /// Whether every record's arrival has been reported, and the tenths of the patch
        /// reported applied.
        all_noted: bool,
        tenths: u64,
    }

    impl BulkDownload {
        /// Open the stream at `addr` with `token`, on a thread of its own.
        ///
        /// # Errors
        /// The thread cannot start.
        pub fn start(addr: SocketAddr, token: [u8; 16]) -> std::io::Result<Self> {
            let limits = Limits::default();
            Self::start_with(
                move || TcpStream::connect_timeout(&addr, limits.connect),
                token,
                limits,
            )
        }

        /// [`Self::start`] over the connection `dial` opens, within `limits`.
        ///
        /// # Errors
        /// The thread cannot start.
        pub fn start_with(
            dial: impl FnOnce() -> std::io::Result<TcpStream> + Send + 'static,
            token: [u8; 16],
            limits: Limits,
        ) -> std::io::Result<Self> {
            let (tx, rx) = channel();
            let budget = Arc::new(Budget::default());
            let arrived = Arc::new(Arrived::default());
            let theirs = Arc::clone(&budget);
            let counted = Arc::clone(&arrived);
            std::thread::Builder::new()
                .name("patch stream".to_owned())
                .spawn(move || {
                    // Each record is decoded here, off the frame loop: inflated and hashed, so
                    // the frame loop only writes it.
                    let take = |d: Box<DddData>| {
                        let n = d.data.len() as u64;
                        let d = crate::ddd::decode(d);
                        let taken =
                            theirs.take(d.held(), limits.queued) && tx.send(Msg::Record(d)).is_ok();
                        if taken {
                            counted.records.fetch_add(1, Ordering::Relaxed);
                            counted.bytes.fetch_add(n, Ordering::Relaxed);
                        }
                        taken
                    };
                    let end = match dial().and_then(|s| read(s, token, &limits, take)) {
                        Ok(()) => {
                            counted.all.store(true, Ordering::Release);
                            Msg::Done
                        }
                        Err(e) => Msg::Failed(e.to_string()),
                    };
                    let _ = tx.send(end);
                })?;
            Ok(Self {
                rx,
                next: None,
                budget,
                arrived,
                end: None,
                started: Instant::now(),
                records: 0,
                bytes: 0,
                expected: 0,
                all_noted: false,
                tenths: 0,
            })
        }

        /// This stream, whose offer said it carries `bytes` of records: what its progress is
        /// counted against until every record has arrived.
        #[must_use]
        pub fn expecting(mut self, bytes: u64) -> Self {
            self.expected = bytes;
            self
        }

        /// The record bytes read off the stream so far, and whether it has ended with every
        /// record it carries.
        #[must_use]
        pub fn arrived(&self) -> (u64, bool) {
            let all = self.arrived.all.load(Ordering::Acquire);
            (self.arrived.bytes.load(Ordering::Relaxed), all)
        }

        /// While every record has arrived and some are still to be applied: the record bytes
        /// applied, and the bytes the stream brought.
        #[must_use]
        pub fn applying(&self) -> Option<(u64, u64)> {
            let (brought, all) = self.arrived();
            (all && self.bytes < brought).then_some((self.bytes, brought))
        }

        /// The stream's stages as log lines, each once: every record arrived, with what is left
        /// to apply, and each further tenth of the patch applied.
        pub fn note_progress(&mut self) {
            #[allow(clippy::cast_precision_loss)] // Megabytes for a log line.
            let mib = |b: u64| b as f64 / 1_048_576.0;
            let (brought, all) = self.arrived();
            if all && !self.all_noted {
                self.all_noted = true;
                tracing::info!(
                    "the patch stream has brought every record: {} records, {:.1} MiB in {:.1} s; \
                     {:.1} MiB of them are still to be applied",
                    self.arrived.records.load(Ordering::Relaxed),
                    mib(brought),
                    self.started.elapsed().as_secs_f64(),
                    mib(brought.saturating_sub(self.bytes))
                );
            }
            let total = if all {
                brought
            } else {
                self.expected.max(brought)
            };
            if total == 0 {
                return;
            }
            let tenths = (self.bytes.saturating_mul(10) / total).min(10);
            if tenths > self.tenths {
                self.tenths = tenths;
                tracing::info!(
                    "the patch is {}% applied: {:.1} of {:.1} MiB",
                    tenths * 10,
                    mib(self.bytes),
                    mib(total)
                );
            }
        }

        fn take(&mut self, m: Msg) {
            match m {
                Msg::Record(_) => unreachable!("records are taken by drain"),
                Msg::Done => self.end = Some(Ok(())),
                Msg::Failed(e) => self.end = Some(Err(e)),
            }
        }

        /// The records that have arrived, decoded, up to about `max_bytes` of them as they came.
        pub fn drain(&mut self, max_bytes: usize) -> Vec<Decoded> {
            let mut out = Vec::new();
            let mut taken = 0usize;
            while self.end.is_none() && taken < max_bytes {
                let m = match self.next.take().map_or_else(|| self.rx.try_recv(), Ok) {
                    Ok(m) => m,
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        Msg::Failed("the stream's thread ended".to_owned())
                    }
                };
                match m {
                    Msg::Record(d) => {
                        taken += d.data.len();
                        self.records += 1;
                        self.bytes += d.data.len() as u64;
                        self.budget.give(d.held());
                        out.push(d);
                    }
                    end => self.take(end),
                }
            }
            out
        }

        /// The next record that has arrived, decoded, if one has.
        pub fn next_record(&mut self) -> Option<Decoded> {
            self.drain(1).pop()
        }

        /// Wait for a record or the stream's end to drain, `guard` at most (a guard against a
        /// hang, for a caller with nothing else to do, never the frame loop); true when one is
        /// ready.
        pub fn wait(&mut self, guard: Duration) -> bool {
            if self.end.is_some() || self.next.is_some() {
                return true;
            }
            match self.rx.recv_timeout(guard) {
                Ok(m) => {
                    self.next = Some(m);
                    true
                }
                Err(RecvTimeoutError::Timeout) => false,
                Err(RecvTimeoutError::Disconnected) => {
                    self.take(Msg::Failed("the stream's thread ended".to_owned()));
                    true
                }
            }
        }

        /// The record bytes read and not yet drained.
        #[must_use]
        pub fn queued(&self) -> usize {
            self.budget.held()
        }

        /// How the stream ended, once every record it brought has been drained.
        #[must_use]
        pub fn ended(&self) -> Option<&Result<(), String>> {
            self.end.as_ref()
        }

        /// Records and bytes drained, and the seconds since the stream started.
        #[must_use]
        pub fn progress(&self) -> (u64, u64, f64) {
            (
                self.records,
                self.bytes,
                self.started.elapsed().as_secs_f64(),
            )
        }
    }

    impl Drop for BulkDownload {
        fn drop(&mut self) {
            // A reader waiting for room stops at once, and its connection closes with it.
            self.budget.close();
        }
    }

    /// Fill `buf` from `r` before `deadline`.
    fn read_by(
        r: &mut BufReader<TcpStream>,
        buf: &mut [u8],
        deadline: Instant,
    ) -> std::io::Result<()> {
        let mut at = 0;
        while at < buf.len() {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(Error::new(ErrorKind::TimedOut, "the stream stalled"));
            }
            r.get_ref().set_read_timeout(Some(left))?;
            match r.read(&mut buf[at..]) {
                Ok(0) => {
                    return Err(Error::new(
                        ErrorKind::UnexpectedEof,
                        "the stream closed early",
                    ))
                }
                Ok(n) => at += n,
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    /// Read the stream on `s`, handing each record to `take` until it answers false or the stream
    /// ends with a length of 0.
    fn read(
        mut s: TcpStream,
        token: [u8; 16],
        limits: &Limits,
        mut take: impl FnMut(Box<DddData>) -> bool,
    ) -> std::io::Result<()> {
        s.set_write_timeout(Some(limits.record))?;
        let mut hello = Vec::with_capacity(24);
        hello.extend_from_slice(&BULK_MAGIC);
        hello.extend_from_slice(&BULK_VERSION.to_le_bytes());
        hello.extend_from_slice(&token);
        s.write_all(&hello)?;
        let mut r = BufReader::with_capacity(1 << 20, s);
        loop {
            let deadline = Instant::now() + limits.record;
            let mut len = [0u8; 4];
            read_by(&mut r, &mut len, deadline)?;
            let len = u32::from_le_bytes(len) as usize;
            if len == 0 {
                return Ok(());
            }
            if len > limits.largest {
                return Err(Error::new(ErrorKind::InvalidData, "a record too large"));
            }
            let mut body = vec![0u8; len];
            read_by(&mut r, &mut body, deadline)?;
            let d = dereth_protocol::read_body::<DddData>(&body)
                .map_err(|e| Error::new(ErrorKind::InvalidData, e.to_string()))?;
            if !take(Box::new(d)) {
                return Ok(());
            }
        }
    }

    #[cfg(test)]
    mod tests;
}

#[cfg(test)]
mod share_tests {
    use std::time::Duration;

    use super::{frame_share, FRAME_SHARE};

    /// Behaviour: none (how much of a frame the patch stream may take)
    /// A frame paced to ten a second, as a window in the background is, gives the patch all of its
    /// 99 ms, leaving nothing to sleep; one with no limit gives it the floor, and so does one
    /// paced so fast that its spacing is less.
    #[test]
    fn a_frame_gives_the_patch_its_whole_spacing_and_never_less_than_the_floor() {
        let spacing = |us| Duration::from_micros(us);
        assert_eq!(
            frame_share(FRAME_SHARE, spacing(99_000)),
            spacing(99_000),
            "ten frames a second"
        );
        assert_eq!(frame_share(FRAME_SHARE, Duration::ZERO), FRAME_SHARE);
        assert_eq!(frame_share(FRAME_SHARE, spacing(6_944)), FRAME_SHARE);
    }
}
