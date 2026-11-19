//! What the player's scripts tell the panels, heard by plugins: each notice the frame carries as
//! its part's event, what it says as a word with its fields.

use auberean_client_contract::event::{EffectEvent, GameEvent};
use auberean_client_contract::notice::{
    change, vital, FellowshipNotice, InventoryNotice, MapNotice, Notice, PetNotice, PlayerNotice,
    Said, SelectionNotice,
};
use auberean_client_contract::pet;
use auberean_client_contract::session::Phase;
use auberean_client_contract::text::TextRef;
use auberean_client_runtime::frame_hook::HookFrame;
use auberean_plugin_host::{events, notices, PluginHost};

use crate::common::{host, package, printed};

const PLAYER: u64 = 0x2130_0000_0000_0001;
const CARA: u64 = 0x2130_0000_0000_0002;

fn told(host: &mut PluginHost, t: f64, events: &[GameEvent], notices: &[Notice]) {
    let mut f = HookFrame::new(events, t, t, Phase::World).with_notices(notices);
    host.step(&mut f);
}

fn about(about: Option<u64>, said: Said) -> Notice {
    Notice { about, said }
}

/// The notices the frame carries reach a plugin listening, each as its part's event (`notice.player`,
/// `notice.fellowship` and the rest) with what it says as a word and its fields, after the frame's
/// other events and in the order told; one the client does not read is not told, and a plugin
/// hears only the parts it listens to.
///
/// Behaviour: plugins.host.a-plugin-hears-the-notices-the-panels-hear
#[test]
fn the_scripts_notices_reach_a_plugin_as_its_parts_events_in_the_order_told() {
    let mut h = host();
    h.request_load(package(
        "test.ears",
        "auberean",
        "",
        r#"
        on("effects.added", function() print("effect") end)
        on("notice.player", function(n)
            print("player " .. n.what .. " " .. tostring(n.vital or n.mounted) .. " " .. tostring(n.current) .. "/" .. tostring(n.max))
        end)
        on("notice.selection", function(n) print("selection " .. n.what .. " " .. n.selected) end)
        on("notice.fellowship", function(n)
            print("fellowship " .. n.what .. " " .. n.fellow .. " " .. n.health .. " about " .. tostring(n.about))
        end)
        on("notice.inventory", function(n) print("inventory " .. n.what .. " " .. n.change .. " " .. n.item) end)
        on("notice.pet", function(n) print("pet " .. n.what .. " " .. n.name .. " " .. n.mode) end)
        "#,
    ));
    told(&mut h, 0.0, &[], &[]);
    printed(&mut h);
    let lit = TextRef::literal;
    let notices = [
        about(
            None,
            Said::Player(PlayerNotice::Vital {
                vital: vital::HEALTH,
                current: 80,
                max: 100,
            }),
        ),
        about(
            Some(PLAYER),
            Said::Selection(SelectionNotice::Changed { selected: CARA }),
        ),
        about(
            None,
            Said::Unread {
                native: 7,
                words: vec![1],
            },
        ),
        about(
            Some(CARA),
            Said::Fellowship(FellowshipNotice::Health {
                fellow: CARA,
                health: 30,
                taken_by_players: 0,
            }),
        ),
        about(
            None,
            Said::Inventory(InventoryNotice::Worn {
                change: change::ADDED,
                item: 0x8000_0000_0000_0010,
                location: 1,
            }),
        ),
        about(None, Said::Map(MapNotice::MarkersCleared { kind: 1 })),
        about(
            None,
            Said::Pet(PetNotice::Added {
                pet: 0x2140_0000_0000_0011,
                name: Some(lit("Wolf")),
                icon: 0,
                mode: pet::mode::AGGRESSIVE,
            }),
        ),
        about(None, Said::Player(PlayerNotice::Mounted { mounted: true })),
    ];
    told(
        &mut h,
        0.1,
        &[GameEvent::Effect(EffectEvent::Added { effect: 1 })],
        &notices,
    );
    assert_eq!(
        printed(&mut h),
        [
            "test.ears: effect",
            "test.ears: player vital health 80/100",
            "test.ears: selection changed 0x2130000000000002",
            "test.ears: fellowship health 0x2130000000000002 30 about 0x2130000000000002",
            "test.ears: inventory worn added 0x8000000000000010",
            "test.ears: pet added Wolf aggressive",
            "test.ears: player mounted true nil/nil",
        ]
    );
    assert!(
        notices::translate(&notices[2]).is_none(),
        "a notice the client does not read is not told"
    );
    for (name, _) in notices::EVENTS {
        assert!(events::is_known(name), "{name}");
    }
}
