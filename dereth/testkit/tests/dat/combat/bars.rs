use dereth_client_model::combat::{CombatMode, PowerBarMode};
use dereth_client_runtime::interaction::Interaction;
use dereth_client_shell::hud::Hud;
use dereth_primitives::{LocalTime, ObjectId};
use dereth_ui::UiSystem;
use dereth_ui_screens::bind::{attr, attr_float};
use dereth_ui_screens::hud::powerbar as pb;
use dereth_ui_screens::panels::remaining::RemainingPanels;

use super::panels;

/// What the ramp reads at step `i`: one frame behind the clock, in tenths.
#[allow(clippy::cast_precision_loss)]
pub fn step_of(i: usize) -> f32 {
    i as f32 / 10.0
}

pub fn key(action: u32, start: bool) -> dereth_client_runtime::actions::Action {
    dereth_client_runtime::actions::Action {
        id: dereth_input::ActionId(action),
        phase: if start {
            dereth_client_runtime::actions::ActionPhase::Begin
        } else {
            dereth_client_runtime::actions::ActionPhase::End
        },
        extent: 1.0,
        repeats: 1,
    }
}

/// **The calibration the two `None` readings rest on**, on a tree of its own because writing
/// a level is exactly what the scenarios assert did not happen: a level read back off the
/// shipped tree can be made non-zero, a charge beginning zeroes it, and the second bar in the
/// tree is a second bar and not an alias of the first.
pub fn the_readback_can_produce_a_non_zero() -> bool {
    let (mut ui, screen) = panels::gameplay();
    let mut p = RemainingPanels::default();
    p.post_init(&mut ui, screen.root().expect("the gameplay root"));
    let read = |ui: &UiSystem, b: &pb::PowerBar| {
        attr_float(ui, b.bound.get("bar").expect("bound"), attr::METER_LEVEL)
    };
    let before = read(&ui, &p.power_bar.bars[0]).is_none();
    let zeroed = p.power_bar.bars[0].begin(&mut ui, pb::PowerBarMode::Jump, false, 0)
        && read(&ui, &p.power_bar.bars[0]) == Some(0.0);
    let moved = p.power_bar.bars[0].set_level(&mut ui, pb::PowerBarMode::Jump, 0.375)
        && read(&ui, &p.power_bar.bars[0]) == Some(0.375);
    let independent = read(&ui, &p.power_bar.bars[1]).is_none();
    before && zeroed && moved && independent
}

pub struct Bench {
    inter: Interaction,
    objects: dereth_client_runtime::objects::ObjectStream,
    store: std::sync::Arc<dereth_dat::RetailDatStore>,
    hud: Hud,
    ui: UiSystem,
    panels: RemainingPanels,
}

impl Bench {
    pub fn new() -> Self {
        let (mut ui, screen) = panels::gameplay();
        let root = screen.root().expect("the gameplay root");
        let mut panels = RemainingPanels::default();
        panels.post_init(&mut ui, root);

        // **The two calibrations.** Both stand as premises here: every zero below is worthless
        // without them.
        assert_eq!(
            panels.power_bar.bound(),
            2,
            "the shipped tree carries two of these bars"
        );
        assert!(
            panels.power_bar.bars.iter().all(|b| b.bound.is_complete()),
            "and every child each of them looks up is in the tree"
        );
        assert!(
            panels.power_bar.bars.iter().all(|b| {
                let h = b.element.expect("bound");
                !ui.node(h).expect("alive").region.flags.visible
            }),
            "both start down, which is what makes the showing observable"
        );
        assert!(
            panels.power_bar.bars.iter().all(|b| {
                attr_float(&ui, b.bound.get("bar").expect("bound"), attr::METER_LEVEL).is_none()
            }),
            "and neither carries a level until a notice writes one -- which is what makes \
                 `never written` and `written with a nought` two different readings"
        );

        let mut objects = dereth_client_runtime::objects::ObjectStream::default();
        objects.world = seed_world();
        Self {
            inter: Interaction::new(),
            objects,
            store: std::sync::Arc::new(dereth_dat::testing::open_store_or_fail()),
            hud: Hud::new(),
            ui,
            panels,
        }
    }

    /// One frame: draw what the previous frame produced, then run the combat system -- which
    /// is the order `App::frame` runs them in, and the reason every reading is one frame
    /// behind the clock.
    pub fn frame(&mut self, actions: Vec<dereth_client_runtime::actions::Action>, t: f64) {
        let notices = self.objects.world.combat.take_power_bar_notices();
        let view = self.hud.view(&self.objects);
        let _ = self.panels.update(&mut self.ui, &view);
        drop(view);
        let _ = dereth_client_shell::hud_drive::deliver_power_bar_notices(
            &mut self.ui,
            &mut self.panels,
            notices,
        );
        let _ = dereth_client_runtime::interaction::use_time(
            &mut self.inter,
            &self.store,
            None,
            &mut self.objects,
            None,
            actions,
            false,
            (800, 600),
            LocalTime(t),
        );
    }

    pub fn combat(&self) -> &dereth_client_model::combat::CombatState {
        &self.objects.world.combat
    }

    pub fn set_style(&mut self, style: u32) {
        self.objects.world.combat.current_style = style;
    }

    pub fn set_requested_power_from_a_drag(&mut self, position: u32) {
        self.objects
            .world
            .combat
            .set_ui_requested_power_from_scrollbar(position);
    }

    pub fn hide_the_bar(&mut self) {
        self.objects.world.combat.hide_power_bar();
    }

    pub fn the_swing_goes_out(&mut self) {
        self.objects.world.handle_commence_attack();
    }

    /// The level attribute on each standalone bar, read back off the element tree. **`None`
    /// is not zero**: the shipped layout declares none until a notice writes one.
    pub fn drawn_levels(&self) -> Vec<Option<f32>> {
        self.panels
            .power_bar
            .bars
            .iter()
            .map(|b| {
                attr_float(
                    &self.ui,
                    b.bound.get("bar").expect("bound"),
                    attr::METER_LEVEL,
                )
            })
            .collect()
    }

    pub fn bar_modes(&self) -> Vec<pb::PowerBarMode> {
        self.panels.power_bar.bars.iter().map(|b| b.mode).collect()
    }

    /// Which of the two bars the notices can really reach.
    pub fn subscriber(&self) -> usize {
        self.panels
            .power_bar
            .bars
            .iter()
            .position(|b| b.registered)
            .expect("exactly one of the two hears the charge notices")
    }

    pub fn drawn_level(&self) -> f32 {
        self.drawn_levels()[self.subscriber()].expect("a notice has written this bar")
    }

    pub fn window_meter(&self) -> Option<f32> {
        let h = self
            .panels
            .combat_window
            .actual_power
            .expect("the window's meter");
        attr_float(&self.ui, h, attr::METER_LEVEL)
    }

    /// The mark the drag leaves, which is a different attribute on a different element.
    pub fn drawn_notch(&self) -> Option<f32> {
        let h = self
            .panels
            .combat_window
            .desired_power
            .expect("the window's gauge");
        attr_float(&self.ui, h, attr::MARKER)
    }

    pub fn bars_visible(&self) -> Vec<bool> {
        self.panels
            .power_bar
            .bars
            .iter()
            .map(|b| {
                let h = b.element.expect("bound");
                self.ui.node(h).is_some_and(|n| n.region.flags.visible)
            })
            .collect()
    }

    /// Turn the advanced interface on through the client's own producer: the option, and then
    /// a mode change, which is the only writer of the cached flag in a running client.
    pub fn enter_advanced_combat(&mut self) {
        let w = &mut self.objects.world;
        w.player_system.options.set(
            dereth_client_model::player::options::option::ADVANCED_COMBAT_UI,
            true,
        );
        assert!(
            !w.combat.advanced_combat_mode,
            "the option alone changes nothing"
        );
        let mut req = dereth_client_model::NullRequests;
        let mut sink = dereth_client_model::NullSink;
        w.set_combat_mode(
            &mut req,
            &mut sink,
            CombatMode::NonCombat,
            false,
            true,
            false,
        )
        .expect("peace is always compatible");
        w.set_combat_mode(&mut req, &mut sink, CombatMode::Melee, false, true, false)
            .expect("unarmed melee is always compatible");
        assert!(
            w.combat.advanced_combat_mode,
            "the mode change re-read the option; without that every reading below would be \
                 about a field written by hand"
        );
        assert_ne!(w.combat.power_bar_mode, PowerBarMode::Combat);
    }
}

/// A player in melee with an attackable creature selected, carrying the table the melee ready
/// arm reads -- which is the state a swing is actually allowed from.
fn seed_world() -> dereth_client_model::World {
    let mut w = dereth_client_model::World::new();
    let player = ObjectId(0x5430_0002);
    let monster = ObjectId(0x8430_0777);
    w.player = Some(player);
    w.tables
        .weenies
        .insert(player, dereth_client_model::Weenie::new(player));
    w.tables
        .weenies
        .get_mut(player)
        .expect("just inserted")
        .pwd
        .name = "Aldis".into();
    w.tables.inventories.insert(
        player,
        dereth_client_model::objects::ObjectInventory::new(player),
    );
    let mut m = dereth_client_model::Weenie::new(monster);
    m.pwd.name = "Mosswart".into();
    m.pwd.obj_type = dereth_rules::weenie::item_type::CREATURE;
    m.pwd.bitfield |= dereth_rules::weenie::bitfield::ATTACKABLE;
    w.tables.weenies.insert(monster, m);
    w.set_selected_object(Some(monster), false, &mut dereth_client_model::NullSink);
    w.combat.combat_mode = CombatMode::Melee;
    w.tables
        .weenies
        .get_mut(player)
        .expect("the player")
        .qualities
        .get_or_insert_with(dereth_client_model::Qualities::new)
        .set(
            dereth_client_model::qualities::StatKey::new(
                dereth_client_model::qualities::StatType::Did,
                dereth_client_model::combat::COMBAT_TABLE_DID,
            ),
            dereth_client_model::qualities::StatValue::Did(dereth_primitives::DataId(0x3000_0000)),
        );
    assert!(
        w.player_in_ready_position(true, Some(false)),
        "the body must be able to attack at all, or every reading is a refusal it did not \
             intend"
    );
    w
}
