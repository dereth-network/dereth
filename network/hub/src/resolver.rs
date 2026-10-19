//! The hub's own name lookups: never more than a few at once, never waited on past a deadline,
//! and remembered for a minute, so that a slow or hostile name server cannot hold the hub's
//! connections.

use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, ToSocketAddrs as _};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

/// One lookup: a host name to its addresses.
pub type LookupFn = dyn Fn(&str) -> io::Result<Vec<IpAddr>> + Send + Sync;

/// How a host name is looked up: the system's resolver, or a test's stand-in.
#[derive(Clone)]
pub struct Lookup(pub Arc<LookupFn>);

impl std::fmt::Debug for Lookup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Lookup")
    }
}

impl Default for Lookup {
    fn default() -> Self {
        Self(Arc::new(|host| {
            Ok((host, 0).to_socket_addrs()?.map(|a| a.ip()).collect())
        }))
    }
}

/// What the hub knows of a host name's addresses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    /// Its addresses: none when it does not resolve.
    Found(Vec<IpAddr>),
    /// Not known: the lookup ran past its deadline, or every worker was busy.
    Unknown,
}

/// Lookups that may run at once.
pub const WORKERS: usize = 8;
/// How long a caller waits for a lookup.
pub const DEADLINE: Duration = Duration::from_secs(2);
/// How long what a lookup found is used.
pub const REMEMBER: Duration = Duration::from_secs(60);
/// The most host names remembered.
const REMEMBERED: usize = 4096;

#[derive(Debug, Default)]
struct State {
    running: usize,
    found: HashMap<String, (Instant, Vec<IpAddr>)>,
}

/// The lookups, on a bounded set of workers, with what they found.
#[derive(Debug)]
pub struct Resolver {
    lookup: Lookup,
    deadline: Duration,
    workers: usize,
    remember: Duration,
    state: Arc<Mutex<State>>,
}

impl Resolver {
    /// Lookups through `lookup`, waited on for at most `deadline`, at most `workers` at once,
    /// remembered for `remember`.
    #[must_use]
    pub fn new(lookup: Lookup, deadline: Duration, workers: usize, remember: Duration) -> Self {
        Self {
            lookup,
            deadline,
            workers,
            remember,
            state: Arc::default(),
        }
    }

    /// `host`'s addresses: a literal address is itself; a name is what a lookup within the last
    /// minute found, or what one started now finds before the deadline.
    #[must_use]
    pub fn resolve(&self, host: &str) -> Resolved {
        self.resolve_within(host, self.deadline)
    }

    /// [`Self::resolve`], waiting at most `deadline` for a lookup started now.
    #[must_use]
    pub fn resolve_within(&self, host: &str, deadline: Duration) -> Resolved {
        if let Ok(ip) = host.parse::<IpAddr>() {
            return Resolved::Found(vec![ip]);
        }
        let name = host.to_ascii_lowercase();
        if let Some(found) = self.remembered(&name) {
            return Resolved::Found(found);
        }
        match self.start(&name) {
            Some(rx) => rx
                .recv_timeout(deadline)
                .map_or(Resolved::Unknown, Resolved::Found),
            None => Resolved::Unknown,
        }
    }

    /// `host`'s addresses if the hub knows them, never waiting: when it does not, a lookup is
    /// started for the next caller.
    #[must_use]
    pub fn known(&self, host: &str) -> Resolved {
        if let Ok(ip) = host.parse::<IpAddr>() {
            return Resolved::Found(vec![ip]);
        }
        let name = host.to_ascii_lowercase();
        if let Some(found) = self.remembered(&name) {
            return Resolved::Found(found);
        }
        drop(self.start(&name));
        Resolved::Unknown
    }

    fn remembered(&self, name: &str) -> Option<Vec<IpAddr>> {
        let state = self.state.lock().ok()?;
        state
            .found
            .get(name)
            .filter(|(at, _)| at.elapsed() < self.remember)
            .map(|(_, found)| found.clone())
    }

    /// Starts a lookup of `name` on a worker of its own, unless every worker is busy. What it
    /// finds comes on the receiver, and is remembered even when nobody waits for it any more.
    fn start(&self, name: &str) -> Option<mpsc::Receiver<Vec<IpAddr>>> {
        {
            let mut state = self.state.lock().ok()?;
            if state.running >= self.workers {
                return None;
            }
            state.running += 1;
        }
        let (tx, rx) = mpsc::channel();
        let state = Arc::clone(&self.state);
        let lookup = self.lookup.clone();
        let remember = self.remember;
        let owned = name.to_owned();
        let spawned = std::thread::Builder::new()
            .name("hub-resolve".into())
            .spawn(move || {
                let found = (lookup.0)(&owned).unwrap_or_default();
                if let Ok(mut s) = state.lock() {
                    s.running -= 1;
                    if s.found.len() >= REMEMBERED {
                        s.found.retain(|_, (at, _)| at.elapsed() < remember);
                        if s.found.len() >= REMEMBERED {
                            s.found.clear();
                        }
                    }
                    s.found.insert(owned, (Instant::now(), found.clone()));
                }
                drop(tx.send(found));
            });
        if spawned.is_err() {
            if let Ok(mut s) = self.state.lock() {
                s.running -= 1;
            }
            return None;
        }
        Some(rx)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Condvar;

    use super::*;

    /// Only a guard against a hang: no test waits this long when it passes.
    const HANG: Duration = Duration::from_secs(30);

    fn ip(s: &str) -> IpAddr {
        s.parse().expect("an address")
    }

    /// A lookup that holds every call until the test opens it, counts its calls and the most
    /// running at once, says when each call begins, and finds 192.0.2.1.
    struct Held {
        lookup: Lookup,
        calls: Arc<AtomicUsize>,
        most: Arc<AtomicUsize>,
        began: mpsc::Receiver<()>,
        gate: Arc<(Mutex<bool>, Condvar)>,
    }

    impl Held {
        fn new() -> Self {
            let calls = Arc::new(AtomicUsize::new(0));
            let most = Arc::new(AtomicUsize::new(0));
            let running = Arc::new(AtomicUsize::new(0));
            let gate = Arc::new((Mutex::new(false), Condvar::new()));
            let (tx, began) = mpsc::channel();
            let tx = Mutex::new(tx);
            let (c, m, g) = (Arc::clone(&calls), Arc::clone(&most), Arc::clone(&gate));
            let lookup = Lookup(Arc::new(move |_host| {
                c.fetch_add(1, Ordering::SeqCst);
                let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                m.fetch_max(now, Ordering::SeqCst);
                tx.lock()
                    .expect("began")
                    .send(())
                    .expect("the test is listening");
                let (open, cv) = &*g;
                let mut open = open.lock().expect("gate");
                while !*open {
                    open = cv.wait(open).expect("gate");
                }
                drop(open);
                running.fetch_sub(1, Ordering::SeqCst);
                Ok(vec![ip("192.0.2.1")])
            }));
            Self {
                lookup,
                calls,
                most,
                began,
                gate,
            }
        }

        fn open(&self) {
            let (open, cv) = &*self.gate;
            *open.lock().expect("gate") = true;
            cv.notify_all();
        }

        fn wait_began(&self, n: usize) {
            for _ in 0..n {
                self.began.recv_timeout(HANG).expect("a lookup began");
            }
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    /// Waits until every lookup `r` started has ended and what it found is remembered.
    fn wait_idle(r: &Resolver) {
        let until = Instant::now() + HANG;
        while r.state.lock().expect("state").running > 0 {
            assert!(Instant::now() < until, "a lookup never ended");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// Spec: hub: a lookup is waited on for at most its deadline; what it finds later is
    /// remembered for the next caller, for a minute.
    #[test]
    fn a_slow_lookup_is_given_up_at_its_deadline_and_remembered_when_it_ends() {
        let held = Held::new();
        let r = Resolver::new(held.lookup.clone(), Duration::from_millis(50), 4, REMEMBER);
        assert_eq!(r.resolve("Slow.Example"), Resolved::Unknown, "given up");
        held.open();
        wait_idle(&r);
        assert_eq!(
            r.resolve("slow.example"),
            Resolved::Found(vec![ip("192.0.2.1")])
        );
        assert_eq!(held.calls(), 1, "remembered, not looked up");

        let held = Held::new();
        held.open();
        let r = Resolver::new(held.lookup.clone(), HANG, 4, Duration::ZERO);
        for _ in 0..2 {
            assert_eq!(
                r.resolve("slow.example"),
                Resolved::Found(vec![ip("192.0.2.1")])
            );
        }
        assert_eq!(held.calls(), 2, "forgotten once its time is up");
    }

    /// Spec: hub: no more lookups run at once than the hub has workers for; a caller past them
    /// is answered at once, not queued.
    #[test]
    fn no_more_lookups_run_at_once_than_there_are_workers() {
        let held = Held::new();
        let r = Arc::new(Resolver::new(
            held.lookup.clone(),
            Duration::from_millis(20),
            3,
            REMEMBER,
        ));
        let callers: Vec<_> = (0..12)
            .map(|i| {
                let r = Arc::clone(&r);
                std::thread::spawn(move || r.resolve(&format!("w{i}.example")))
            })
            .collect();
        for c in callers {
            assert_eq!(c.join().expect("a caller"), Resolved::Unknown);
        }
        held.wait_began(3);
        held.open();
        wait_idle(&r);
        assert_eq!(held.calls(), 3);
        assert_eq!(held.most.load(Ordering::SeqCst), 3);
    }

    /// Spec: hub: what the hub already knows is answered without waiting; a name it does not
    /// know is unknown at once, and looked up for the next caller.
    #[test]
    fn a_known_answer_never_waits_on_a_lookup() {
        let held = Held::new();
        let r = Arc::new(Resolver::new(held.lookup.clone(), HANG, 2, REMEMBER));
        let (tx, rx) = mpsc::channel();
        let asker = Arc::clone(&r);
        std::thread::spawn(move || {
            drop(tx.send((asker.known("later.example"), asker.known("192.0.2.9"))));
        });
        let (later, literal) = rx
            .recv_timeout(HANG)
            .expect("answered while the lookup is held");
        assert_eq!(later, Resolved::Unknown);
        assert_eq!(literal, Resolved::Found(vec![ip("192.0.2.9")]));
        held.open();
        wait_idle(&r);
        assert_eq!(
            r.known("later.example"),
            Resolved::Found(vec![ip("192.0.2.1")])
        );
        assert_eq!(held.calls(), 1);
    }
}
