use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use dereth_client_shell::input::InputShell;
use dereth_input::fire::ControlType;
use dereth_input::spec::ControlChord;
use dereth_input::{ActionId, InputMapId};
use dereth_primitives::{AssetSource, DataId, LocalTime, ObjectId};
use dereth_ui::{Delivery, ElemHandle, ElementMessage, Screen as _, UiSystem};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use dereth_ui_screens::view::{CombatBar, GameView, MagicNotice, PlayerOption, SpellEntry};
use {
    dereth_client_contract::actions::mapped as ia, dereth_client_runtime::interaction::Interaction,
};

const PLAYER: ObjectId = ObjectId(0x5410_1002);
const MONSTER: ObjectId = ObjectId(0x8410_1777);

/// The shipped gameplay tree, built by the screen's own startup.
///
/// **`combat.rs` keeps a second copy of this**, because each subject owns its own scenario
/// file; promoting it into `dereth-testkit`'s own `src/` would remove the duplicate.
pub fn gameplay() -> (UiSystem, GamePlayScreen) {
    let store = Arc::new(dereth_dat::testing::open_store_or_fail());
    let master_id = DataId(0x3900_0001);
    let master = <dereth_assets::MasterProperty as dereth_assets::Decode>::decode_payload(
        master_id,
        &store.read(master_id).expect("the shipped property table"),
    )
    .expect("it decodes");
    let mut ui = UiSystem::new((800, 600));
    ui.property_types = master.property_types();
    let mut flow = dereth_ui::UiFlow::new();
    dereth_ui_screens::register_all(&mut ui, &mut flow);
    let assets = Rc::new(store);
    let resolver = Rc::new(
        dereth_ui::framework::DidMapperResolver::load_via_master(assets.as_ref())
            .expect("the shipped mapper"),
    );
    dereth_ui_screens::env::install(&mut ui, assets, resolver);
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds from the shipped layout");
    pump(&mut ui);
    ui.requests.clear();
    ui.notice_inbox.clear();
    (ui, s)
}

/// Drain the tree's own delivery queue.
pub fn pump(ui: &mut UiSystem) {
    for _ in 0..8 {
        if ui.drain_outbox().is_empty() {
            return;
        }
    }
}

/// The same, delivering what it drains to the screen -- which is what the flow does once a
/// frame.
pub fn pump_to(ui: &mut UiSystem, screen: &mut GamePlayScreen) {
    for _ in 0..8 {
        let batch = ui.drain_outbox();
        if batch.is_empty() {
            return;
        }
        for d in &batch {
            if let Delivery::Element { msg, .. } = d {
                screen.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), msg);
            }
        }
    }
}

/// One press, hit-tested and broadcast, then delivered to the screen. Answers what the
/// screen's own filter forwarded to the panels.
pub fn press_through(
    ui: &mut UiSystem,
    screen: &mut GamePlayScreen,
    action: u32,
    at: (i32, i32),
) -> Vec<ElementMessage> {
    ui.mouse_down(action, at.0, at.1);
    pump_to(ui, screen);
    screen.take_panel_messages()
}

/// Show everything between the screen's root and `target` that is shut, with the call the
/// toolbar would make, and answer how many that was.
pub fn open_the_window_over(ui: &mut UiSystem, target: ElemHandle) -> usize {
    let mut chain = Vec::new();
    let mut h = Some(target);
    while let Some(c) = h {
        chain.push(c);
        h = ui.parent(c);
    }
    let mut opened = 0;
    for c in chain.into_iter().rev() {
        if !ui.node(c).is_some_and(|n| n.region.flags.visible) {
            ui.set_visible(c, true);
            opened += 1;
        }
    }
    opened
}

/// The client's own input shell over the shipped tables.
pub fn shell() -> InputShell {
    let store = dereth_dat::testing::open_store_or_fail();
    InputShell::new(&store, None).expect("the shipped input tables decode")
}

/// Every `(action, control)` the shipped keymap carries for one section.
pub fn shipped_bindings(s: &InputShell, map: InputMapId) -> Vec<(ActionId, ControlChord)> {
    s.manager
        .keymap
        .section(map)
        .map(|sec| sec.bindings().iter().map(|(q, a)| (*a, *q)).collect())
        .unwrap_or_default()
}

/// The one control the shipped keymap binds `action` to in `map`.
pub fn the_shipped_control(s: &InputShell, map: InputMapId, action: u32) -> ControlChord {
    let mut found: Vec<ControlChord> = shipped_bindings(s, map)
        .into_iter()
        .filter(|(a, _)| a.0 == action)
        .map(|(_, q)| q)
        .collect();
    found.dedup();
    assert_eq!(
        found.len(),
        1,
        "{action:#010X} has one shipped default control in {map:?}"
    );
    found[0]
}

/// Which instruction a key stands for, as the client's own arm decides it.
pub fn the_instruction_for(action: u32) -> MagicNotice {
    match ActionId(action) {
        ia::COMBAT_CAST_CURRENT_SPELL => MagicNotice::CastCurrentSpell,
        ia::COMBAT_PREV_SPELL => MagicNotice::PrevSpellSelection,
        ia::COMBAT_NEXT_SPELL => MagicNotice::NextSpellSelection,
        ia::COMBAT_PREV_SPELL_TAB => MagicNotice::PrevSpellTab,
        ia::COMBAT_NEXT_SPELL_TAB => MagicNotice::NextSpellTab,
        ia::COMBAT_FIRST_SPELL => MagicNotice::FirstSpellSelection,
        ia::COMBAT_LAST_SPELL => MagicNotice::LastSpellSelection,
        ia::COMBAT_FIRST_SPELL_TAB => MagicNotice::FirstSpellTab,
        ia::COMBAT_LAST_SPELL_TAB => MagicNotice::LastSpellTab,
        a => MagicNotice::CastQuickslotSpell {
            slot: usize::try_from(a.0 - ia::USE_SPELL_SLOT_FIRST.0).expect("a small slot"),
        },
    }
}

/// The release event the input layer would build. The shipped casting defaults are one-shots
/// and produce none, so the arm is asked the question directly.
pub fn released(action: ActionId) -> dereth_client_runtime::actions::Action {
    dereth_client_runtime::actions::Action {
        id: action,
        phase: dereth_client_runtime::actions::ActionPhase::End,
        extent: 1.0,
        repeats: 1,
    }
}

/// One key press through the client's own input shell, drained through its own frame.
pub struct Driver {
    /// Well clear of the double-click window and the button history, so two presses of one
    /// key are never read as a gesture of each other.
    clock: u32,
}

impl Driver {
    pub const fn new() -> Self {
        Self { clock: 1_000 }
    }

    /// A press and its release, with any modifiers held across both.
    pub fn press_release(
        &mut self,
        shell: &mut InputShell,
        qc: &ControlChord,
    ) -> (
        Vec<dereth_client_runtime::actions::Action>,
        Vec<dereth_client_runtime::actions::Action>,
    ) {
        let metas: Vec<_> = shell
            .manager
            .keymap
            .meta_keys
            .iter()
            .filter(|(_, bit)| qc.meta_mode & bit != 0)
            .map(|(cs, _)| *cs)
            .collect();
        self.clock += 6_000;
        let mut t = self.clock;
        for cs in &metas {
            shell
                .manager
                .fire_input_event(*cs, ControlType::Button, 0x80, t);
            t += 10;
        }
        shell.use_time(LocalTime(f64::from(t) / 1000.0));
        shell.take_events();

        shell
            .manager
            .fire_input_event(qc.control, ControlType::Button, 0x80, t);
        shell.use_time(LocalTime(f64::from(t) / 1000.0));
        let down = shell.take_events();

        t += 10;
        shell
            .manager
            .fire_input_event(qc.control, ControlType::Button, 0, t);
        shell.use_time(LocalTime(f64::from(t) / 1000.0));
        let up = shell.take_events();

        for cs in metas.iter().rev() {
            t += 10;
            shell
                .manager
                .fire_input_event(*cs, ControlType::Button, 0, t);
        }
        shell.use_time(LocalTime(f64::from(t) / 1000.0));
        shell.take_events();
        self.clock = t;
        let hand_on = |events: Vec<dereth_input::InputEvent>| {
            events
                .iter()
                .map(dereth_input::InputEvent::to_action)
                .collect()
        };
        (hand_on(down), hand_on(up))
    }
}

/// The client's own interaction slot over a character standing in the casting stance.
pub struct Bench {
    inter: Interaction,
    objects: dereth_client_runtime::objects::ObjectStream,
    store: Arc<dereth_dat::RetailDatStore>,
}

impl Bench {
    pub fn new() -> Self {
        let mut objects = dereth_client_runtime::objects::ObjectStream::default();
        objects.world = a_world();
        Self {
            inter: Interaction::new(),
            objects,
            store: Arc::new(dereth_dat::testing::open_store_or_fail()),
        }
    }

    /// The magic notices the interaction layer raised and has not handed to a UI.
    pub fn take_notices(&mut self) -> Vec<dereth_ui_screens::view::MagicNotice> {
        self.inter.magic_notices.take()
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

    pub const fn magic_actions(&self) -> u64 {
        self.inter.stats.magic_actions
    }
}

fn a_world() -> dereth_client_model::World {
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
    w.combat.combat_mode = dereth_client_model::combat::CombatMode::Magic;
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
    w
}

/// A view that answers only what these scenarios read.
#[derive(Debug, Default)]
pub struct StubView {
    pub spells: Vec<u32>,
    pub spell_entries: Vec<SpellEntry>,
    pub options: BTreeMap<u32, bool>,
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
    fn combat_bar(&self) -> CombatBar {
        self.bar.unwrap_or_default()
    }
}
