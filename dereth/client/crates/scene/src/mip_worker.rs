//! Building compressed textures' mip chains off the main thread.
//!
//! A DXT texture's system-memory mip chain is built on the CPU by a bit-exact port of the D3DX9
//! encoder (`dereth_render::mip::compressed_system_chain`), as retail built it: Direct3D 9 could not
//! generate mips for compressed formats itself. It is by far the largest part of making a
//! landblock's objects the first time their textures are seen -- several milliseconds a texture --
//! and it is a pure function of the one decoded level. So the landscape asks for it here instead
//! of running it in the upload: a texture whose chain is not ready is requested and reported
//! pending, the landblock that wanted it is baked again once it is, and the upload then hands the
//! device the finished chain, which skips its own build. The bytes are the ones the upload would
//! have made.
//!
//! One worker thread; the chain build spreads each large level over a few more threads itself.
//! Finished chains are kept for the session, so a texture's chain is built once however often its
//! texture is released and made again.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc;
use std::sync::Arc;

use dereth_primitives::{TextureData, TextureFormat};

/// A chain's identity: the texture's cache key and the image scale its level was made at.
pub type ChainKey = (u64, u32);

/// What [`MipWorker::prepare`] has for a texture.
#[derive(Debug)]
pub enum Prepared {
    /// The texture needs no chain from the CPU; upload it as it is.
    NotNeeded,
    /// The finished chain: upload this instead.
    Ready(Arc<TextureData>),
    /// Requested and not finished; try again later.
    Pending,
}

type Job = (ChainKey, TextureData);
type Done = (ChainKey, Option<TextureData>);

/// The worker thread and the chains it has made.
#[derive(Debug)]
pub struct MipWorker {
    jobs: Option<mpsc::Sender<Job>>,
    done: mpsc::Receiver<Done>,
    thread: Option<std::thread::JoinHandle<()>>,
    // ORDER-OK: looked up by key only.
    ready: HashMap<ChainKey, Arc<TextureData>>,
    /// Chains that failed to build: the upload is left to make them, and to report the error.
    failed: HashSet<ChainKey>,
    in_flight: HashSet<ChainKey>,
    /// Keys reported pending since the last [`Self::take_requested`].
    requested: Vec<ChainKey>,
}

impl MipWorker {
    /// Start the worker thread.
    #[must_use]
    pub fn new() -> Self {
        let (jobs, rx) = mpsc::channel::<Job>();
        let (tx, done) = mpsc::channel::<Done>();
        let thread = std::thread::Builder::new()
            .name("dere mip chains".into())
            .spawn(move || {
                for (key, data) in rx {
                    let chain = dereth_render::mip::compressed_system_chain(&data)
                        .ok()
                        .flatten();
                    if tx.send((key, chain)).is_err() {
                        break;
                    }
                }
            })
            .ok();
        Self {
            jobs: thread.is_some().then_some(jobs),
            done,
            thread,
            ready: HashMap::new(),
            failed: HashSet::new(),
            in_flight: HashSet::new(),
            requested: Vec::new(),
        }
    }

    /// Whether `data` is a texture the upload would build a CPU chain for: one level of a
    /// block-compressed format.
    #[must_use]
    pub fn needs_chain(data: &TextureData) -> bool {
        data.levels.len() == 1
            && matches!(
                data.format,
                TextureFormat::Bc1
                    | TextureFormat::Bc2
                    | TextureFormat::Bc3
                    | TextureFormat::Bc2Premultiplied
                    | TextureFormat::Bc3Premultiplied
            )
    }

    /// Collect the chains the worker has finished.
    pub fn poll(&mut self) {
        while let Ok((key, chain)) = self.done.try_recv() {
            self.in_flight.remove(&key);
            match chain {
                Some(c) => {
                    self.ready.insert(key, Arc::new(c));
                }
                None => {
                    self.failed.insert(key);
                }
            }
        }
    }

    /// The chain for `data` under `key`: ready, not needed, or requested now and pending.
    pub fn prepare(&mut self, key: ChainKey, data: &TextureData) -> Prepared {
        if !Self::needs_chain(data) || self.failed.contains(&key) {
            return Prepared::NotNeeded;
        }
        if let Some(c) = self.ready.get(&key) {
            return Prepared::Ready(Arc::clone(c));
        }
        let Some(jobs) = self.jobs.as_ref() else {
            return Prepared::NotNeeded;
        };
        if self.in_flight.insert(key) && jobs.send((key, data.clone())).is_err() {
            // The worker is gone; build it the ordinary way.
            self.in_flight.remove(&key);
            return Prepared::NotNeeded;
        }
        self.requested.push(key);
        Prepared::Pending
    }

    /// Whether every one of `keys` has finished (built or failed).
    #[must_use]
    pub fn all_done(&self, keys: &[ChainKey]) -> bool {
        keys.iter().all(|k| !self.in_flight.contains(k))
    }

    /// The keys reported pending since the last call.
    pub fn take_requested(&mut self) -> Vec<ChainKey> {
        std::mem::take(&mut self.requested)
    }
}

impl Default for MipWorker {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for MipWorker {
    fn drop(&mut self) {
        // Closing the job channel ends the worker's loop.
        self.jobs = None;
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bc1(width: u32, height: u32, seed: u8) -> TextureData {
        let blocks = (width.div_ceil(4) * height.div_ceil(4)) as usize;
        #[allow(clippy::cast_possible_truncation)] // a deliberately wrapping byte pattern
        let level = (0..blocks * 8)
            .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
            .collect();
        TextureData {
            width,
            height,
            format: TextureFormat::Bc1,
            levels: vec![level],
        }
    }

    fn key(n: u64) -> ChainKey {
        (n, 0)
    }

    /// A chain made here is the chain the upload would have made, and a texture that needs none
    /// is never queued.
    #[test]
    fn the_worker_builds_the_upload_s_own_chain() {
        let mut w = MipWorker::new();
        let data = bc1(128, 64, 7);
        assert!(matches!(w.prepare(key(1), &data), Prepared::Pending));
        assert_eq!(w.take_requested(), vec![key(1)]);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        let chain = loop {
            w.poll();
            if let Prepared::Ready(c) = w.prepare(key(1), &data) {
                break c;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the worker never finished"
            );
            std::thread::yield_now();
        };
        let direct = dereth_render::mip::compressed_system_chain(&data)
            .unwrap()
            .unwrap();
        assert_eq!(
            (chain.width, chain.height, chain.format),
            (direct.width, direct.height, direct.format)
        );
        assert_eq!(chain.levels, direct.levels);
        assert!(w.all_done(&[key(1)]));
        // The polling above asked again while it waited; those requests are spent.
        w.take_requested();

        let bgra = TextureData {
            width: 4,
            height: 4,
            format: TextureFormat::Bgra8,
            levels: vec![vec![0; 64]],
        };
        assert!(matches!(w.prepare(key(2), &bgra), Prepared::NotNeeded));
        assert!(w.take_requested().is_empty());
    }
}
