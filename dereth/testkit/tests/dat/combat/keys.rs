use std::collections::BTreeMap;

use dereth_client_model::combat::CombatMode;
use dereth_client_runtime::interaction::Interaction;
use dereth_client_shell::input::InputShell;
use dereth_input::spec::ControlChord;
use dereth_input::{ActionId, InputMapId};
use dereth_primitives::{DataId, LocalTime, ObjectId, ServerTime};
use dereth_ui::{ElementId, MessageId, UiSystem};
use dereth_ui_screens::panels::remaining::RemainingPanels;
use dereth_ui_screens::view::{CombatBar, GameView, PlayerOption, SpellEntry, UiRequest};

use super::{maps, panels};

const PLAYER: ObjectId = ObjectId(0x5410_0002);
const MONSTER: ObjectId = ObjectId(0x8410_0777);

/// The controls all three combat sections bind, discovered from the shipped keymap.
pub fn shared_controls(shell: &InputShell) -> Vec<ControlChord> {
    let section = |m: InputMapId| {
        shell
            .manager
            .keymap
            .section(m)
            .map(|s| s.bindings().to_vec())
            .unwrap_or_else(|| panic!("{m:?} has a shipped section"))
    };
    let melee = section(dereth_input::combat::MELEE_COMBAT_MAP);
    let missile = section(dereth_input::combat::MISSILE_COMBAT_MAP);
    let magic = section(dereth_input::combat::MAGIC_COMBAT_MAP);
    let mut out: Vec<ControlChord> = melee
        .iter()
        .filter(|(qa, _)| {
            missile.iter().any(|(qb, _)| qa.is_conflicting(qb))
                && magic.iter().any(|(qb, _)| qa.is_conflicting(qb))
        })
        .map(|(q, _)| *q)
        .collect();
    out.sort_by_key(|q| q.control.offset());
    out.dedup_by_key(|q| q.control.offset());
    assert!(
        !out.is_empty(),
        "the three shipped sections share something"
    );
    out
}

/// **The calibration**, which every silence below rests on: the driver can produce an action
/// and the arm can move the world. The known positive is the key that toggles combat itself.
pub fn the_driver_and_the_arm_both_work() -> bool {
    use dereth_client_runtime::interaction::action as ia;

    let mut shell = maps::shell();
    let mut d = maps::Driver::new();
    let qc = maps::the_shipped_control(
        &shell,
        dereth_input::combat::COMBAT_MAP.0,
        ia::COMBAT_TOGGLE_COMBAT,
    );
    let (down, _) = d.press_release(&mut shell, &qc);
    if !down.iter().map(|e| e.id.0).eq([ia::COMBAT_TOGGLE_COMBAT]) {
        return false;
    }
    // ...and it reaches the world. This bench has no body, so the client queues the change
    // rather than taking it -- which names the branch the action reached, and is a stronger
    // reading than the mode simply moving.
    let mut bench = Bench::new(CombatMode::NonCombat);
    bench.drive(down, 100.0);
    bench.world().combat.pending_combat_mode == CombatMode::Melee
        && bench.world().combat.combat_mode == CombatMode::NonCombat
}

/// A player in `mode` with something attackable selected and the table the ready arm reads.
pub fn a_world(mode: CombatMode) -> dereth_client_model::World {
    let mut w = dereth_client_model::World::new();
    w.player = Some(PLAYER);
    w.tables
        .weenies
        .insert(PLAYER, dereth_client_model::Weenie::new(PLAYER));
    w.tables
        .weenies
        .get_mut(PLAYER)
        .expect("just inserted")
        .pwd
        .name = "Aldis".into();
    w.tables.inventories.insert(
        PLAYER,
        dereth_client_model::objects::ObjectInventory::new(PLAYER),
    );
    let mut m = dereth_client_model::Weenie::new(MONSTER);
    m.pwd.name = "Mosswart".into();
    m.pwd.obj_type = dereth_rules::weenie::item_type::CREATURE;
    m.pwd.bitfield |= dereth_rules::weenie::bitfield::ATTACKABLE;
    w.tables.weenies.insert(MONSTER, m);
    w.set_selected_object(Some(MONSTER), false, &mut dereth_client_model::NullSink);
    w.combat.combat_mode = mode;
    w.tables
        .weenies
        .get_mut(PLAYER)
        .expect("the player")
        .qualities
        .get_or_insert_with(dereth_client_model::Qualities::new)
        .set(
            dereth_client_model::qualities::StatKey::new(
                dereth_client_model::qualities::StatType::Did,
                dereth_client_model::combat::COMBAT_TABLE_DID,
            ),
            dereth_client_model::qualities::StatValue::Did(DataId(0x3000_0000)),
        );
    if mode == CombatMode::Missile {
        w.combat.current_style = dereth_client_model::combat::MISSILE_READY_STYLES[0];
    }
    assert!(
        w.player_in_ready_position(true, Some(false)),
        "the body must be able to attack at all, or every reading is a refusal it did not \
             intend"
    );
    w
}

/// The release event the input layer would build. The shipped combat bindings are one-shots
/// and produce none, so the arm is asked the question directly with the event it would have
/// been handed.
pub fn released_in(action: u32, _input_map: InputMapId) -> dereth_client_runtime::actions::Action {
    dereth_client_runtime::actions::Action {
        id: ActionId(action),
        phase: dereth_client_runtime::actions::ActionPhase::End,
        extent: 1.0,
        repeats: 1,
    }
}

/// The height a swing carries, whichever kind of swing it is.
pub fn height_of(r: &dereth_client_model::Request) -> Option<u32> {
    match r {
        dereth_client_model::Request::TargetedMeleeAttack(m) => Some(m.attack_height),
        dereth_client_model::Request::TargetedMissileAttack(m) => Some(m.attack_height),
        _ => None,
    }
}

/// The client's own interaction slot over a world, with no scene and no link: the requests are
/// counted, never sent.
pub struct Bench {
    inter: Interaction,
    objects: dereth_client_runtime::objects::ObjectStream,
    store: std::sync::Arc<dereth_dat::RetailDatStore>,
}

impl Bench {
    pub fn new(mode: CombatMode) -> Self {
        let mut objects = dereth_client_runtime::objects::ObjectStream::default();
        objects.world = a_world(mode);
        Self {
            inter: Interaction::new(),
            objects,
            store: std::sync::Arc::new(dereth_dat::testing::open_store_or_fail()),
        }
    }

    pub fn drive(&mut self, actions: Vec<dereth_client_runtime::actions::Action>, now: f64) {
        let _ = dereth_client_runtime::interaction::use_time(
            &mut self.inter,
            &self.store,
            None,
            &mut self.objects,
            None,
            actions,
            false,
            (800, 600),
            LocalTime(now),
        );
    }

    pub const fn world(&self) -> &dereth_client_model::World {
        &self.objects.world
    }

    pub const fn height_changes(&self) -> u64 {
        self.inter.stats.attack_height_changes
    }

    pub const fn attacks_released(&self) -> u64 {
        self.inter.stats.attacks_released
    }

    pub const fn desired_power_changes(&self) -> u64 {
        self.inter.stats.desired_power_changes
    }

    /// What a frame with no link leaves behind: the requests that would have gone out.
    pub fn what_would_go_out(&self) -> Vec<dereth_client_model::Request> {
        self.inter.last_sent.clone()
    }
}

/// One element message, as the shipped tree delivers one.
pub fn element_message(
    source: dereth_ui::ElemHandle,
    source_id: ElementId,
    id: MessageId,
) -> dereth_ui::ElementMessage {
    dereth_ui::ElementMessage {
        source_id,
        source,
        id,
        p1: 0,
        p2: 0,
        point: dereth_ui::msg::element::MessagePoint::default(),
        serial: 0,
    }
}

/// The combat window on the shipped tree, with the client's own request seam under it.
pub struct WindowBench {
    ui: UiSystem,
    root: dereth_ui::ElemHandle,
    p: RemainingPanels,
    view: StubView,
    inter: Interaction,
    objects: dereth_client_runtime::objects::ObjectStream,
}

impl WindowBench {
    pub fn new(mode: CombatMode) -> Self {
        let (mut ui, screen) = panels::gameplay();
        let root = screen.root().expect("the gameplay root");
        let mut p = RemainingPanels::default();
        p.post_init(&mut ui, root);
        let mut objects = dereth_client_runtime::objects::ObjectStream::default();
        objects.world = a_world(mode);
        Self {
            ui,
            root,
            p,
            view: StubView::default(),
            inter: Interaction::new(),
            objects,
        }
    }

    /// The window bound off the shipped tree, with everything it looks up found.
    pub fn the_window_is_bound(&self) -> bool {
        let w = &self.p.combat_window;
        w.bound()
            && w.failures == 0
            && w.height_buttons.len() == 3
            && w.options.len() == 3
            && w.options.iter().map(|o| o.option).eq([
                PlayerOption::AutoRepeatAttack,
                PlayerOption::AutoTarget,
                PlayerOption::ViewCombatTarget,
            ])
            && w.desired_power.is_some()
            && w.actual_power.is_some()
            && w.recklessness_field.is_some_and(|h| {
                // Binding hides it, which is what the boundary scenario measures against.
                !self.ui.node(h).expect("alive").region.flags.visible
            })
    }

    /// One gesture on a control of the window, and what it asked the client for.
    pub fn press(&mut self, id: ElementId, message: MessageId, p1: u32) -> Vec<UiRequest> {
        let h = self
            .ui
            .get_child_recursive(self.root, id)
            .unwrap_or_else(|| panic!("{id:?} is in the shipped tree"));
        let mut m = element_message(h, id, message);
        m.p1 = p1;
        self.ui.requests.clear();
        assert!(
            self.p.on_element_message(&mut self.ui, &m, &self.view),
            "{id:?} is the window's own control and it consumed the gesture"
        );
        self.ui.requests.take()
    }

    /// Tick one of the three option boxes and click it, as a player does: the control carries
    /// its own new value and the arm reads it back off the control.
    pub fn tick_the_option_box(&mut self, i: usize) -> Vec<UiRequest> {
        use dereth_ui_screens::hud::combat_window as cw;
        let element = self.p.combat_window.options[i].element;
        let id = cw::OPTION_CHECKBOXES[i].0;
        self.ui.set_attribute_bool(element, cw::ATTR_CHECKED, true);
        self.ui.requests.clear();
        assert!(
            self.p.on_element_message(
                &mut self.ui,
                &element_message(element, id, dereth_ui::msg::element::id::BUTTON_CLICKED),
                &self.view,
            ),
            "the option box consumed its own click"
        );
        self.ui.requests.take()
    }

    /// A drag on something that is not the window's gauge.
    pub fn drag_something_else(&mut self, position: u32) -> Vec<UiRequest> {
        let id = ElementId(0x1000_0034);
        let Some(h) = self.ui.get_child_recursive(self.root, id) else {
            return Vec::new();
        };
        let mut m = element_message(h, id, dereth_ui::msg::element::id::SCROLL_POSITION);
        m.p1 = position;
        self.ui.requests.clear();
        let _ = self.p.on_element_message(&mut self.ui, &m, &self.view);
        self.ui.requests.take()
    }

    /// Run what a gesture asked for, through the client's own request seam.
    pub fn run(&mut self, requests: Vec<UiRequest>, now: f64) {
        self.inter.queue(Vec::new(), requests);
        self.inter
            .run_ui_requests(&mut self.objects.world, false, ServerTime(now));
    }

    pub const fn world(&self) -> &dereth_client_model::World {
        &self.objects.world
    }

    pub const fn attacks_released(&self) -> u64 {
        self.inter.stats.attacks_released
    }

    pub const fn option_changes_queued(&self) -> u64 {
        self.inter.stats.option_changes_deferred + self.inter.stats.option_changes_unsendable
    }

    pub const fn side_effects_unapplied(&self) -> u64 {
        self.inter.stats.option_side_effects_unapplied
    }
}

/// A view that answers only what these scenarios read; everything else takes the trait's own
/// default, which is what makes those answers the only inputs.
#[derive(Debug, Default)]
pub struct StubView {
    pub spells: Vec<u32>,
    pub spell_entries: Vec<SpellEntry>,
    pub options: BTreeMap<u32, bool>,
    pub recklessness: u32,
    pub bar: Option<CombatBar>,
}

impl StubView {
    pub fn with_spells(spells: Vec<u32>) -> Self {
        let spell_entries = spells
            .iter()
            .map(|id| SpellEntry {
                id: *id,
                name: format!("Spell {id}"),
                icon: Some(DataId(0x0600_13A5)),
                school: 4,
                level: 1,
                icon_power: 1,
                display_order: i32::try_from(*id).expect("a small id"),
                bitfield: 0,
            })
            .collect();
        Self {
            spells,
            spell_entries,
            ..Self::default()
        }
    }
}

impl GameView for StubView {
    fn spellbook(&self) -> &[SpellEntry] {
        &self.spell_entries
    }
    fn spell_tab(&self, _tab: usize) -> &[u32] {
        &self.spells
    }
    fn player_option(&self, o: PlayerOption) -> bool {
        self.options.get(&(o as u32)).copied().unwrap_or(false)
    }
    fn recklessness_advancement_class(&self) -> u32 {
        self.recklessness
    }
    fn combat_bar(&self) -> CombatBar {
        self.bar.unwrap_or_default()
    }
}
