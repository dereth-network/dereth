//! Where the game socket binds, from the preferences and the line.

use std::net::Ipv4Addr;

use auberean_shell::net_binding::{parse_interface, NetBinding};
use auberean_shell::preferences::{key, section, EnumNames, Names, Preferences};
use auberean_shell::profile::Profile;

fn prefs(text: &str) -> Preferences {
    Preferences::startup(
        Profile::new(text),
        Names {
            sections: EnumNames::of(&[(section::NET, "Net")]),
            keys: EnumNames::of(&[
                (key::BIND_INTERFACE, "BindInterface"),
                (key::COMPUTE_UNIQUE_PORT, "ComputeUniquePort"),
                (key::USER_SPECIFIED_PORT, "UserSpecifiedPort"),
            ]),
        },
    )
}

/// The game socket takes the first free port of a series worked out from its interface unless told
/// not to; then the line's port, else the preferences'. A filter picks the interface.
///
/// Behaviour: desktop.net.the-game-socket-binds-as-the-preferences-say
#[test]
fn the_game_socket_binds_as_the_preferences_say() {
    let here = Ipv4Addr::new(192, 168, 1, 5);
    let start = u32::from(here) % 0xFA07;

    // Nothing said: the series, 0x100 at a time from past its start, none under 0x400.
    let plain = NetBinding::read(&prefs(""), 0);
    let ports = plain.ports(here);
    assert_eq!(u32::from(ports[0]), (start + 0x100) & 0xFFFF);
    assert_eq!(ports.len(), 251, "{ports:?}");
    assert!(ports.iter().all(|p| *p >= 0x400));
    assert!(
        !ports.contains(&u16::try_from(start).unwrap()),
        "not its start"
    );
    assert_eq!(plain.address(here), Ipv4Addr::UNSPECIFIED);

    // A port named means nothing while the series is on.
    assert_eq!(NetBinding::read(&prefs(""), 9000).ports(here), ports);

    // The series off: the line's port, else the preferences', else the system's.
    let off = "[Net]\r\nComputeUniquePort=0\r\nUserSpecifiedPort=9100\r\n";
    assert_eq!(NetBinding::read(&prefs(off), 9000).ports(here), [9000]);
    assert_eq!(NetBinding::read(&prefs(off), 0).ports(here), [9100]);
    assert_eq!(
        NetBinding::read(&prefs("[Net]\r\nComputeUniquePort=0\r\n"), 0).ports(here),
        [0]
    );

    // The filter takes the interface it matches; else any; one it cannot read is none.
    let at = |f: &str| {
        NetBinding::read(&prefs(&format!("[Net]\r\nBindInterface={f}\r\n")), 0).address(here)
    };
    assert_eq!(at("192.168.1.0/24"), here);
    assert_eq!(at("192.168.1.0/255.255.255.0"), here);
    assert_eq!(at("10.0.0.0/8"), Ipv4Addr::UNSPECIFIED);
    assert_eq!(at("192.168.1.5"), Ipv4Addr::UNSPECIFIED);
    assert_eq!(parse_interface("10.0.0.0/33"), None);

    // The first port that takes it; the last refusal when none does.
    let mut tried = 0;
    let got = plain.bind(here, "none", |p| {
        tried += 1;
        if tried < 3 {
            Err("taken")
        } else {
            Ok(p)
        }
    });
    assert_eq!(got, Ok(ports[2]));
    assert_eq!(
        plain.bind(here, "none", |_| Err::<u16, _>("taken")),
        Err("taken")
    );
}
