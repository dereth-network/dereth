//! Classic panel gestures through the shared runtime.

use dereth_classic_ui::panels::{Context, ControlEvent, PanelAction};
use dereth_primitives::ObjectId;

/// Behaviour: container.ground.a-pickup-goes-into-whichever-pack-the-player-has-open
#[test]
fn choosing_a_pack_updates_the_shared_pickup_destination() {
    use dereth_client_model::{weenie::Weenie, World};
    use dereth_client_runtime::{hud::Hud, interaction::Interaction};
    let player = ObjectId(1);
    let pack = ObjectId(2);
    let mut world = World::new();
    world.player = Some(player);
    world.tables.weenies.insert(player, Weenie::new(player));
    let mut bag = Weenie::new(pack);
    bag.pwd.container_id = Some(player);
    world.tables.weenies.insert(pack, bag);
    let mut inventory = dereth_client_model::objects::ObjectInventory::default();
    inventory.containers.push(pack);
    world.tables.inventories.insert(player, inventory);
    let hud = Hud::new();
    let pregame = Default::default();
    let keyboard = Default::default();
    let settings = Default::default();
    let classic = Default::default();
    let mut panel = dereth_classic_ui::panels::game::make("inventory").unwrap();
    for (control, destination) in [("containers", pack), ("backpack", player)] {
        let view = dereth_client_runtime::hud::HudView {
            hud: &hud,
            world: &world,
        };
        let ctx = Context {
            game: &view,
            pregame: &pregame,
            keyboard: &keyboard,
            settings: &settings,
            map_teleport_allowed: false,
            classic: &classic,
        };
        let actions = panel.event(
            ControlEvent::Select {
                id: control.into(),
                index: 0,
            },
            &ctx,
        );
        let requests = actions
            .into_iter()
            .filter_map(|action| match action {
                PanelAction::Game(request) => Some(request),
                _ => None,
            })
            .collect();
        let mut interaction = Interaction::new();
        interaction.queue(Vec::new(), requests);
        interaction.run_ui_requests(&mut world, false, dereth_primitives::ServerTime(1.0));
        assert_eq!(world.open_container, Some(destination));
    }
}
