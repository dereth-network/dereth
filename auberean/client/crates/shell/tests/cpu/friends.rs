//! Where a session finds the friends service, from the options.

use auberean_shell::FriendsService;

/// With no friends service named, a session reaches none; named, it reaches that one, on the port
/// named, and a shell starts with none.
/// Behaviour: friends.login.none-unless-named
#[test]
fn a_session_reaches_a_friends_service_only_where_the_options_name_one() {
    assert_eq!(
        FriendsService::named(None, Some(9000), true, Some("t")),
        FriendsService::Off
    );
    assert_eq!(FriendsService::default(), FriendsService::Off);
    assert_eq!(
        FriendsService::named(Some("10.0.0.5"), Some(9000), true, None),
        FriendsService::At {
            host: "10.0.0.5".into(),
            port: Some(9000),
            use_ticket: true,
            ticket: None,
        }
    );
}
