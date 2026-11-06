//! Sharing out work a frame can do on any thread, where there are threads.
//!
//! Each piece of work only fills in what its own item keeps, the same whichever thread does it and
//! in whatever order, so the frame comes out the same with any number of threads. A browser has
//! none to spare: there everything runs on the thread that asks, one piece after another.
//!
//! The threads are kept from share to share: starting them anew for every frame costs more, and a
//! share would have to wait for one that has not got going, even with all its work done. A thread
//! that wakes late here finds nothing left and goes back to sleep.

/// `work` once on every item, sharing them among the kept threads and this one, and back when all
/// are done.
pub fn share<T, F>(items: Vec<T>, work: F)
where
    T: Send + Sync + 'static,
    F: Fn(&T) + Send + Sync + 'static,
{
    #[cfg(target_arch = "wasm32")]
    items.iter().for_each(work);
    #[cfg(not(target_arch = "wasm32"))]
    threads::share(items, work);
}

#[cfg(not(target_arch = "wasm32"))]
mod threads {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Condvar, Mutex, OnceLock, PoisonError};

    /// The most threads a share runs on, the caller's among them, and at most half of what the
    /// machine has: the device's driver and the rest of the machine want the others.
    const MOST_THREADS: usize = 4;

    /// Items and what to do with them, taken one at a time by whichever thread comes next.
    struct Share<T, F> {
        items: Vec<T>,
        work: F,
        next: AtomicUsize,
        done: AtomicUsize,
    }

    /// A share with its items' type out of sight, as the kept threads see it.
    trait Drain: Send + Sync {
        fn drain(&self);
    }

    impl<T: Send + Sync, F: Fn(&T) + Send + Sync> Drain for Share<T, F> {
        fn drain(&self) {
            /// Counts its item done however the work ends: a panic must not leave the caller
            /// waiting for it.
            struct Done<'a>(&'a AtomicUsize);
            impl Drop for Done<'_> {
                fn drop(&mut self) {
                    self.0.fetch_add(1, Ordering::Release);
                }
            }
            while let Some(item) = self.items.get(self.next.fetch_add(1, Ordering::Relaxed)) {
                let _done = Done(&self.done);
                (self.work)(item);
            }
        }
    }

    /// The kept threads: the number of the latest share, and the share while it runs.
    #[derive(Default)]
    struct Kept {
        turn: Mutex<(u64, Option<Arc<dyn Drain>>)>,
        wake: Condvar,
    }

    impl Kept {
        /// What a kept thread does: wait for a share it has not seen and help with it.
        fn help(&self) {
            let mut seen = 0;
            loop {
                let share = {
                    let mut turn = self.turn.lock().unwrap_or_else(PoisonError::into_inner);
                    while turn.0 == seen {
                        turn = self.wake.wait(turn).unwrap_or_else(PoisonError::into_inner);
                    }
                    seen = turn.0;
                    turn.1.clone()
                };
                if let Some(share) = share {
                    share.drain();
                }
            }
        }

        fn post(&self, share: Option<Arc<dyn Drain>>) {
            let mut turn = self.turn.lock().unwrap_or_else(PoisonError::into_inner);
            if share.is_some() {
                turn.0 += 1;
            }
            turn.1 = share;
        }
    }

    /// The kept threads, started the first time work is shared; none on a machine of too few
    /// threads. A thread that cannot be started leaves its part to the others.
    fn kept() -> Option<&'static Arc<Kept>> {
        static KEPT: OnceLock<Option<Arc<Kept>>> = OnceLock::new();
        KEPT.get_or_init(|| {
            let threads = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
            let helpers = (threads / 2).clamp(1, MOST_THREADS) - 1;
            (helpers > 0).then(|| {
                let kept = Arc::new(Kept::default());
                for _ in 0..helpers {
                    let kept = kept.clone();
                    let _ = std::thread::Builder::new()
                        .name("auberean-worker".into())
                        .spawn(move || kept.help());
                }
                kept
            })
        })
        .as_ref()
    }

    pub(super) fn share<T, F>(items: Vec<T>, work: F)
    where
        T: Send + Sync + 'static,
        F: Fn(&T) + Send + Sync + 'static,
    {
        let Some(kept) = kept().filter(|_| items.len() > 1) else {
            items.iter().for_each(work);
            return;
        };
        let count = items.len();
        let share = Arc::new(Share {
            items,
            work,
            next: AtomicUsize::new(0),
            done: AtomicUsize::new(0),
        });
        kept.post(Some(share.clone()));
        kept.wake.notify_all();
        share.drain();
        while share.done.load(Ordering::Acquire) < count {
            std::hint::spin_loop();
            std::thread::yield_now();
        }
        kept.post(None);
    }
}
