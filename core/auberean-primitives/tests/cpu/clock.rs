use auberean_primitives::time::{Elapsed, Timer};

/// The clock's current time is the host's elapsed time plus the server's offset, and never runs
/// back: an earlier elapsed time moves it on by 10^-12 s. The server's time moves the offset only
/// when more than 10^-9 s ahead, and the local time follows at the next update.
///
/// Behaviour: session.time.the-clock-follows-the-hosts-elapsed-time-and-never-runs-back
#[test]
fn the_clock_follows_the_hosts_elapsed_time_and_never_runs_back() {
    let mut t = Timer::new();
    assert_eq!((t.current(), t.local()), (0.0, 0.0));
    t.update(2.0);
    assert_eq!((t.current(), t.local()), (2.0, 2.0));
    t.update(1.5);
    assert_eq!(t.current(), 2.0 + 1e-12);
    assert!(t.set(100.0, 3.0));
    assert_eq!((t.current(), t.local()), (100.0, 2.0 + 1e-12));
    t.update(4.0);
    assert_eq!((t.current(), t.local()), (101.0, 4.0));
    assert!(!t.set(50.0, 5.0));
    assert!(!t.set(102.0 + 5e-10, 5.0));
    t.update(5.0);
    assert_eq!(t.current(), 102.0);
}

/// A thread's elapsed time never goes back: an earlier reading answers the last again, unless the
/// last was not above zero.
///
/// Behaviour: session.time.a-thread-is-never-answered-an-earlier-elapsed-time
#[test]
fn a_thread_is_never_answered_an_earlier_elapsed_time() {
    let mut e = Elapsed::default();
    assert_eq!(e.answer(-1.0), -1.0);
    assert_eq!(e.answer(-2.0), -2.0);
    assert_eq!(e.answer(3.0), 3.0);
    assert_eq!(e.answer(2.5), 3.0);
    assert_eq!(e.answer(3.5), 3.5);
}
