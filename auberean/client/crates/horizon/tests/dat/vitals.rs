//! Horizon's parameter bar from the player's qualities the server sent: each gauge its own vital's
//! current and greatest.

use auberean_client_sdk::protocol::login::PlayerQualities;
use auberean_client_sdk::protocol::table::{Table, SMALL};
use auberean_client_sdk::protocol::types::Qualities;
use auberean_client_sdk::protocol::ServerMessage;
use auberean_horizon::state::Vital;

use crate::session::{self, Session};

/// Horizon's health gauge reads the player's health of its greatest, and its vigor gauge the
/// player's vigor of its greatest, as the server's qualities give them.
///
/// Behaviour: desktop.horizon.the-parameter-bar-reads-each-vital-s-current-and-greatest
#[test]
fn the_parameter_bar_reads_each_vitals_current_and_greatest() {
    let qualities = session::s2c(
        8.10,
        1,
        1,
        session::WORLD | 7,
        &ServerMessage::PlayerQualities(PlayerQualities(Qualities {
            header: Qualities::INTS,
            ints: Table::from_inserted(SMALL, [(258, 80), (263, 100), (259, 55), (264, 120)]),
            ..Qualities::default()
        })),
    );
    let mut s = Session::going_on(Some("[UI]\r\nInterface=Horizon\r\n"), vec![qualities]);
    s.enter_the_world();
    let vitals = |s: &Session| {
        let h = s.horizon();
        let h = h.borrow();
        (h.state().health, h.state().vigor)
    };
    let told = s.until(10.0, |s| vitals(s).1.max == 120);
    assert!(told, "the vitals: {:?}", vitals(&s));
    assert_eq!(
        vitals(&s),
        (
            Vital {
                current: 80,
                max: 100
            },
            Vital {
                current: 55,
                max: 120
            }
        )
    );
}
