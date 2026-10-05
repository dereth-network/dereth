use dereth_client_model::combat::CombatMode;
use dereth_client_net::client_session::testing::{Corpus, CorpusBlob, Direction};
use dereth_client_runtime::interaction::Interaction;
use dereth_client_runtime::objects::ObjectStream;
use dereth_client_shell::hud::Hud;
use dereth_primitives::{LocalTime, ObjectId};
use dereth_ui::{Delivery, ElemHandle, ElementId, Screen as _, UiSystem};
use dereth_ui_screens::bind::{attr, attr_float};
use dereth_ui_screens::hud::combat_notice as cn;
use dereth_ui_screens::hud::powerbar as pb;
use dereth_ui_screens::screens::gameplay::{window, GamePlayScreen};

use super::panels;

/// The recorded session that carries the option going on and off again.
const A_RECORDED_SESSION: &str = "melee-attack-run";
/// The bit the option lives in, written as a literal rather than read back through the same
/// table the client uses, so a wrong mask on either side cannot hide behind the symbol.
pub const THE_OPTION_BIT: u32 = 0x0000_1000;
/// The envelope a client's own game action leaves in, and where its sub-opcode sits.
const A_CLIENT_ACTION: u32 = 0xF7B1;
const ACTION_SUB_TYPE: usize = 8;
/// The envelope the shard's ordered events arrive in, and where their sub-opcode sits.
const AN_ORDERED_EVENT: u32 = 0xF7B0;
const EVENT_SUB_TYPE: usize = 12;
/// The message that saves the whole option word, and where that word sits in its body.
const SAVES_THE_OPTIONS: u32 = 0x01A1;
const OPTIONS_WORD: usize = 16;
/// The character's own description.
const A_DESCRIPTION: u32 = 0x0013;
/// The swing, and where the power it went out at sits in its body.
const A_MELEE_SWING: u32 = 0x0008;
const SWING_POWER: usize = 20;

const PLAYER: ObjectId = ObjectId(0x5490_0002);
const MONSTER: ObjectId = ObjectId(0x8490_0777);

fn dword(b: &[u8], off: usize) -> Option<u32> {
    let s = b.get(off..off + 4)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

pub fn the_recorded_session() -> Corpus {
    Corpus::load(A_RECORDED_SESSION)
        .expect("the recording parses")
        .expect("the decoded corpus carries it")
}

pub fn is_a_description(b: &CorpusBlob) -> bool {
    dword(&b.payload, 0) == Some(AN_ORDERED_EVENT)
        && dword(&b.payload, EVENT_SUB_TYPE) == Some(A_DESCRIPTION)
}

fn is_an_option_save(b: &CorpusBlob) -> bool {
    b.dir == Direction::ClientToServer
        && dword(&b.payload, 0) == Some(A_CLIENT_ACTION)
        && dword(&b.payload, ACTION_SUB_TYPE) == Some(SAVES_THE_OPTIONS)
}

/// The option word of every save the recorded client sent, in recorded order.
pub fn the_recorded_option_saves(c: &Corpus) -> Vec<u32> {
    c.blobs
        .iter()
        .filter(|b| is_an_option_save(b))
        .filter_map(|b| dword(&b.payload, OPTIONS_WORD))
        .collect()
}

/// When the recorded player turned the option on.
pub fn when_the_option_went_on(c: &Corpus) -> Option<f64> {
    c.blobs
        .iter()
        .filter(|b| is_an_option_save(b))
        .find(|b| dword(&b.payload, OPTIONS_WORD).is_some_and(|w| w & THE_OPTION_BIT != 0))
        .map(seconds)
}

/// `(when, the power it went out at)` for every swing the recorded client sent.
pub fn the_recorded_swings(c: &Corpus) -> Vec<(f64, f32)> {
    c.blobs
        .iter()
        .filter(|b| {
            b.dir == Direction::ClientToServer
                && dword(&b.payload, 0) == Some(A_CLIENT_ACTION)
                && dword(&b.payload, ACTION_SUB_TYPE) == Some(A_MELEE_SWING)
        })
        .filter_map(|b| {
            let p = b.payload.get(SWING_POWER..SWING_POWER + 4)?;
            Some((seconds(b), f32::from_le_bytes([p[0], p[1], p[2], p[3]])))
        })
        .collect()
}

fn seconds(b: &CorpusBlob) -> f64 {
    std::time::Duration::from_micros(b.t_rel_micros).as_secs_f64()
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

pub struct Bench {
    ui: UiSystem,
    screen: GamePlayScreen,
    hud: Hud,
    objects: ObjectStream,
    inter: Interaction,
    store: std::sync::Arc<dereth_dat::RetailDatStore>,
    serial: u64,
    /// How many element messages the delivery pass handed to the screen: a denominator that
    /// separates "the window did not move" from "no message was ever delivered".
    delivered: u64,
}

impl Bench {
    pub fn new() -> Self {
        let (ui, screen) = panels::gameplay();
        let mut objects = ObjectStream::default();
        objects.world = seed_world();
        Self {
            ui,
            screen,
            hud: Hud::new(),
            objects,
            inter: Interaction::new(),
            store: std::sync::Arc::new(dereth_dat::testing::open_store_or_fail()),
            serial: 1,
            delivered: 0,
        }
    }

    /// One frame, in the order the real one runs: the panels, the charge notices, the
    /// delivery pass, then the combat system.
    pub fn frame(&mut self, actions: Vec<dereth_client_runtime::actions::Action>, t: f64) {
        self.ui.now = LocalTime(t);
        let notices = self.objects.world.combat.take_power_bar_notices();
        self.hud
            .drive(&mut self.ui, &mut self.screen, self.serial, &self.objects);
        dereth_client_shell::hud_drive::deliver_power_bar_notices(
            &mut self.ui,
            &mut self.hud.panels,
            notices,
        );
        self.deliver();
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

    /// The element-message pass, bounded. It is bounded in the client too: a show or a hide
    /// is only broadcast on a change.
    fn deliver(&mut self) {
        for _ in 0..8 {
            let batch = self.ui.drain_outbox();
            if batch.is_empty() {
                return;
            }
            for d in batch {
                if let Delivery::Element { msg, .. } = d {
                    self.delivered += 1;
                    self.screen.on_element_message(
                        &mut dereth_ui::framework::ScreenCx::new(&mut self.ui),
                        &msg,
                    );
                }
            }
        }
    }

    pub const fn delivered(&self) -> u64 {
        self.delivered
    }

    pub const fn world(&self) -> &dereth_client_model::World {
        &self.objects.world
    }

    fn handle(&mut self, id: ElementId) -> ElemHandle {
        let root = self.screen.root().expect("the gameplay root");
        self.ui
            .get_child_recursive(root, id)
            .unwrap_or_else(|| panic!("{id:?} is in the shipped layout"))
    }

    fn visible(&mut self, id: ElementId) -> bool {
        let h = self.handle(id);
        self.ui.node(h).expect("alive").region.flags.visible
    }

    /// The combat window itself -- what the player sees.
    pub fn combat_window(&mut self) -> bool {
        self.visible(window::COMBAT_PANEL)
    }

    /// Its cluster of controls, asserted separately: a change that reaches one and not the
    /// other is a defect of its own.
    pub fn combat_page(&mut self) -> bool {
        self.visible(cn::COMBAT_UI_PAGE)
    }

    /// Both standalone strips.
    pub fn strip(&mut self) -> Vec<bool> {
        vec![
            self.visible(window::SMART_BOX_POWER_BAR),
            self.visible(window::POWER_BAR),
        ]
    }

    /// The level on each strip's own meter. **`None` is not zero**: the shipped layout
    /// declares none until a notice writes one.
    pub fn strip_levels(&mut self) -> Vec<Option<f32>> {
        [window::SMART_BOX_POWER_BAR, window::POWER_BAR]
            .into_iter()
            .map(|id| {
                let owner = self.handle(id);
                let bar = self
                    .ui
                    .get_child_recursive(owner, pb::BAR)
                    .expect("every strip carries its own meter");
                attr_float(&self.ui, bar, attr::METER_LEVEL)
            })
            .collect()
    }

    /// The combat window's own meter, which is the classic display.
    pub fn window_meter(&mut self) -> Option<f32> {
        let h = self.handle(dereth_ui_screens::hud::combat_window::ACTUAL_POWER);
        attr_float(&self.ui, h, attr::METER_LEVEL)
    }

    pub fn set_mode(&mut self, mode: CombatMode) {
        self.objects
            .world
            .set_combat_mode(
                &mut dereth_client_model::NullRequests,
                &mut dereth_client_model::NullSink,
                mode,
                false,
                true,
                false,
            )
            .expect("the shard's own form takes no ready check");
    }

    /// Turn the advanced interface on or off, as the options page does. Nothing else is
    /// touched: the point is that the option alone changes what the UI does.
    pub fn set_advanced(&mut self, on: bool) {
        self.objects.world.player_system.options.set(
            dereth_client_model::player::options::option::ADVANCED_COMBAT_UI,
            on,
        );
    }

    pub fn set_auto_repeat(&mut self, on: bool) {
        self.objects.world.player_system.options.set(
            dereth_client_model::player::options::option::AUTO_REPEAT_ATTACK,
            on,
        );
    }

    /// Nothing to swing at, through the client's own setter.
    pub fn nothing_selected(&mut self) {
        self.objects
            .world
            .set_selected_object(None, false, &mut dereth_client_model::NullSink);
    }

    /// The shard's answer to the swing that went out.
    pub fn the_shard_answers_the_swing(&mut self, now: LocalTime) {
        self.objects
            .world
            .handle_attack_done(&mut dereth_client_model::NullRequests, 0, true, now);
    }
}

/// A player standing at **peace** with an attackable creature selected and the table the melee
/// ready arm reads. Peace deliberately: the first thing these scenarios watch is the mode
/// change that opens the window, and a fixture already in melee would have nothing to show.
fn seed_world() -> dereth_client_model::World {
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
    w.combat.combat_mode = CombatMode::NonCombat;
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
            dereth_client_model::qualities::StatValue::Did(dereth_primitives::DataId(0x3000_0000)),
        );
    assert!(
        w.player_in_ready_position(true, Some(false)),
        "the body must be able to attack at all, or every reading is a refusal it did not \
             intend"
    );
    w
}
