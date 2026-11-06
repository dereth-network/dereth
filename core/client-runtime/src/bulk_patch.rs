//! The patch as one TCP stream (client divergence CD-044).
//!
//! A server that knows it (Empyrean, V509) answers a client that asks (`FLAG_BULK` in `0xF7E6`)
//! with `0xF7ED` in place of the patch's `0xF7E2`s: a port and a token. A thread connects to that
//! port at the server's address, sends the magic, the version and the token, and reads the records
//! as `DddData` bodies, each behind its length, until a length of 0. TCP's flow control paces it;
//! the channel to the frame loop holds a bounded number of records, so a client that applies them
//! more slowly than they arrive slows the stream rather than piling them up. The frame loop
//! applies each one as the `0xF7E2` it stands for. A stream that cannot be opened or breaks is
//! declined (`0xF7EE`) with the records still pending, which the server then sends as `0xF7E2`s.
//! Any other server never offers one, and the patch is retail's.

use std::io::{BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::mpsc::{sync_channel, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use dereth_protocol::admin::{DddData, BULK_MAGIC, BULK_VERSION};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const READ_TIMEOUT: Duration = Duration::from_secs(30);
/// The records waiting for the frame loop at most.
const QUEUED: usize = 256;
/// The largest record body the stream may carry.
const LARGEST: usize = 64 << 20;

#[derive(Debug)]
enum Msg {
    Record(Box<DddData>),
    Done,
    Failed(String),
}

/// A patch stream being read.
#[derive(Debug)]
pub struct BulkDownload {
    rx: Receiver<Msg>,
    end: Option<Result<(), String>>,
    started: Instant,
    records: u64,
    bytes: u64,
}

impl BulkDownload {
    /// Open the stream at `addr` with `token`, on a thread of its own.
    ///
    /// # Errors
    /// The thread cannot start.
    pub fn start(addr: SocketAddr, token: [u8; 16]) -> std::io::Result<Self> {
        let (tx, rx) = sync_channel(QUEUED);
        std::thread::Builder::new()
            .name("patch stream".to_owned())
            .spawn(move || {
                let end = match read(addr, token, |d| tx.send(Msg::Record(d)).is_ok()) {
                    Ok(()) => Msg::Done,
                    Err(e) => Msg::Failed(e.to_string()),
                };
                let _ = tx.send(end);
            })?;
        Ok(Self {
            rx,
            end: None,
            started: Instant::now(),
            records: 0,
            bytes: 0,
        })
    }

    /// The records that have arrived, up to about `max_bytes` of them.
    pub fn drain(&mut self, max_bytes: usize) -> Vec<Box<DddData>> {
        let mut out = Vec::new();
        let mut taken = 0usize;
        while self.end.is_none() && taken < max_bytes {
            match self.rx.try_recv() {
                Ok(Msg::Record(d)) => {
                    taken += d.data.len();
                    self.records += 1;
                    self.bytes += d.data.len() as u64;
                    out.push(d);
                }
                Ok(Msg::Done) => self.end = Some(Ok(())),
                Ok(Msg::Failed(e)) => self.end = Some(Err(e)),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.end = Some(Err("the stream's thread ended".to_owned()));
                }
            }
        }
        out
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

/// Read the stream at `addr`, handing each record to `take` until it answers false or the stream
/// ends with a length of 0.
fn read(
    addr: SocketAddr,
    token: [u8; 16],
    mut take: impl FnMut(Box<DddData>) -> bool,
) -> std::io::Result<()> {
    use std::io::{Error, ErrorKind};
    let mut s = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)?;
    s.set_read_timeout(Some(READ_TIMEOUT))?;
    let mut hello = Vec::with_capacity(24);
    hello.extend_from_slice(&BULK_MAGIC);
    hello.extend_from_slice(&BULK_VERSION.to_le_bytes());
    hello.extend_from_slice(&token);
    s.write_all(&hello)?;
    let mut r = BufReader::with_capacity(1 << 20, s);
    loop {
        let mut len = [0u8; 4];
        r.read_exact(&mut len)?;
        let len = u32::from_le_bytes(len) as usize;
        if len == 0 {
            return Ok(());
        }
        if len > LARGEST {
            return Err(Error::new(ErrorKind::InvalidData, "a record too large"));
        }
        let mut body = vec![0u8; len];
        r.read_exact(&mut body)?;
        let d = dereth_protocol::read_body::<DddData>(&body)
            .map_err(|e| Error::new(ErrorKind::InvalidData, e.to_string()))?;
        if !take(Box::new(d)) {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn record(id: u32) -> DddData {
        DddData {
            dat_file_type: 0,
            dat_file_id: 1,
            resource_type: 6,
            resource_id: id,
            iteration: 2073,
            compressed: 0,
            version: 3,
            data_size: 4 + 3,
            data: vec![1, 2, 3],
        }
    }

    /// A server that checks the hello, then writes `ids` and, when `finish`, the closing 0.
    fn serve(ids: Vec<u32>, finish: bool) -> (SocketAddr, [u8; 16]) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let token = [9u8; 16];
        std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let mut hello = [0u8; 24];
            s.read_exact(&mut hello).unwrap();
            assert_eq!(&hello[..4], b"DBLK");
            assert_eq!(&hello[8..], &[9u8; 16]);
            for id in ids {
                let body = dereth_protocol::write_body(&record(id)).unwrap();
                let len = u32::try_from(body.len()).unwrap();
                s.write_all(&len.to_le_bytes()).unwrap();
                s.write_all(&body).unwrap();
            }
            if finish {
                s.write_all(&0u32.to_le_bytes()).unwrap();
            }
        });
        (addr, token)
    }

    fn all(d: &mut BulkDownload) -> Vec<u32> {
        let mut ids = Vec::new();
        let until = Instant::now() + Duration::from_secs(10);
        while d.ended().is_none() && Instant::now() < until {
            ids.extend(d.drain(usize::MAX).iter().map(|r| r.resource_id));
            std::thread::sleep(Duration::from_millis(5));
        }
        ids
    }

    /// Behaviour: ddd.bulk-patch.a-stream-brings-every-record-and-tells-its-end-from-a-break
    #[test]
    fn a_stream_brings_every_record_then_ends_and_a_cut_one_fails() {
        let (addr, token) = serve((1..=600).collect(), true);
        let mut d = BulkDownload::start(addr, token).unwrap();
        assert_eq!(all(&mut d), (1..=600).collect::<Vec<_>>());
        assert_eq!(d.ended(), Some(&Ok(())));
        assert_eq!(d.progress().0, 600);

        let (addr, token) = serve(vec![7, 8], false);
        let mut d = BulkDownload::start(addr, token).unwrap();
        assert_eq!(all(&mut d), vec![7, 8]);
        assert!(matches!(d.ended(), Some(Err(_))), "closed without its 0");
    }

    /// Behaviour: ddd.bulk-patch.a-stream-nobody-answers-fails-so-the-patch-falls-back
    #[test]
    fn a_stream_nobody_answers_fails() {
        let addr = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        let mut d = BulkDownload::start(addr, [0; 16]).unwrap();
        assert!(all(&mut d).is_empty());
        assert!(matches!(d.ended(), Some(Err(_))));
    }
}
