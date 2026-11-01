//! `@dn` said in chat.

use super::*;
use yalain_federation::WORLD_OFF;
use yalain_friends::HELP;

/// `@dn` shows the character's place on the network and the hub's link; `@dn on` and `@dn off`
/// put it on or take it off at once, kept in the store; `@dn help` and any word it does not know
/// show the commands. With the world off the network, every one says so.
/// Behaviour: server.friends.dn-said-in-chat
#[test]
fn dn_said_in_chat_shows_puts_on_and_takes_off_the_network() {
    let mut off = World::new();
    off.character("Ann");
    for words in ["", "on", "help"] {
        assert_eq!(
            off.said("Ann", words),
            vec![WORLD_OFF.to_owned()],
            "{words}"
        );
    }

    let mut w = World::linked();
    let ann = w.player("Ann");
    assert_eq!(
        w.said("Ann", ""),
        vec![
            "You are not on the Dereth Network. Say @dn on to join it as Ann@dawnsong.".to_owned()
        ]
    );
    assert_eq!(
        w.said("Ann", "off"),
        vec!["You are not on the Dereth Network.".to_owned()]
    );
    assert_eq!(
        w.said("Ann", "ON"),
        vec![
            "You are on the Dereth Network as Ann@dawnsong. Anyone on the network can see when \
             you are online and send you tells. Say @dn help for the commands."
                .to_owned(),
            "Say @dn off to leave the network.".to_owned(),
        ]
    );
    let (_, id) = w.ids("Ann");
    assert_eq!(w.store.network_characters().expect("reads"), vec![id]);
    assert_eq!(
        w.said("Ann", "on"),
        vec!["You are on the Dereth Network already, as Ann@dawnsong.".to_owned()]
    );
    w.request(
        ann,
        Request::FriendAdd {
            user: "Bob@coldeve".into(),
        },
    );
    assert_eq!(
        w.said("Ann", ""),
        vec![
            "You are on the Dereth Network as Ann@dawnsong. The hub is connected. You have 1 \
             friend on other worlds."
                .to_owned()
        ]
    );
    assert_eq!(
        w.said("Ann", "off"),
        vec![
            "You are off the Dereth Network. Nobody on another world can see you or tell you."
                .to_owned()
        ]
    );
    assert!(w.store.network_characters().expect("reads").is_empty());
    let help: Vec<String> = HELP.iter().map(|l| (*l).to_owned()).collect();
    assert_eq!(w.said("Ann", "help"), help);
    assert_eq!(w.said("Ann", "frobnicate"), help);
}
