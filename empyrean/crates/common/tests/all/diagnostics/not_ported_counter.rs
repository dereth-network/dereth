//! Vectors: local thread-local and global diagnostic counter cases in this module
//! Not_ported! hits are counted per thread (take_local clears) and globally.
//! Fixture: thread-local and global counters driven by synthetic member names.

mod not_ported {
    #[test]
    fn hits_are_counted_per_thread_and_globally() {
        empyrean_common::not_ported::take_local();
        empyrean_common::not_ported!("ACE: Test.Member");
        empyrean_common::not_ported!("ACE: Test.Member");
        let local = empyrean_common::not_ported::take_local();
        assert_eq!(local.get("ACE: Test.Member"), Some(&2));
        assert!(empyrean_common::not_ported::take_local().is_empty());
        assert!(empyrean_common::not_ported::global_snapshot()["ACE: Test.Member"] >= 2);
    }
}
