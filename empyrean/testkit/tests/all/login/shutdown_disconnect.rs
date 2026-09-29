//! ACE: Source/ACE.Server/Network/Managers/NetworkManager.cs::DisconnectAllSessionsForShutdown
//! Shutdown disconnect through game messages.
//! Fixture: shared virtual-time server state and recorded content where enabled.

mod shutdown {
    //! ACE: Source/ACE.Server/Network/Managers/NetworkManager.cs::DisconnectAllSessionsForShutdown
    use crate::support::object_message_world::*;

    /// A shutdown disconnect sends the worlds character error.
    #[test]
    fn a_shutdown_disconnect_sends_the_worlds_character_error() {
        let mut ts = server();
        let (alpha, _) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
        assert!(ts.received::<CharacterError>(alpha).is_empty());

        let now = ts.world.now;
        ts.world.net.disconnect_all_sessions_for_shutdown(now);
        ts.advance(0.5);

        let errors: Vec<u32> = ts
            .received::<CharacterError>(alpha)
            .iter()
            .map(|e| e.char_error)
            .collect();
        assert_eq!(errors, [SERVER_CRASH_1]);
    }
}
