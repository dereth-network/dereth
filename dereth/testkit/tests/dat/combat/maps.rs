use dereth_input::fire::ControlType;
use dereth_input::spec::ControlChord;
use dereth_input::{ActionId, InputMapId};
use dereth_testkit::HeadlessClient;
use {dereth_client_shell::input::InputShell, dereth_client_shell::input::BASE_MAP_REGISTRATIONS};

/// The three sets of keys a combat mode chooses between, and the one that belongs to combat
/// itself. Ids of shipped data, which is what the client's own table names them by.
pub const MELEE_MAP: u32 = 0x1000_0003;
pub const MISSILE_MAP: u32 = 0x1000_0004;
pub const MAGIC_MAP: u32 = 0x1000_0005;
pub const COMBAT_MAP: u32 = 0x1000_0002;
pub const QUICKSLOT_MAP: u32 = 0x1000_000C;
pub const UI_COMMANDS_MAP: u32 = 0x1000_0009;
pub const MODE_MAPS: [u32; 3] = [MELEE_MAP, MISSILE_MAP, MAGIC_MAP];

pub fn quick() -> InputMapId {
    InputMapId(QUICKSLOT_MAP)
}

/// The client's own input shell over the shipped tables.
pub fn shell() -> InputShell {
    let store = dereth_dat::testing::open_store_or_fail();
    InputShell::new(&store, None).expect("the shipped input tables decode")
}

/// The registration band the gameplay keys live in, in walk order.
pub fn band(s: &InputShell) -> Vec<u32> {
    s.manager
        .maps
        .entries()
        .iter()
        .filter(|e| e.priority == dereth_input::dispatch::priority::GAMEPLAY)
        .map(|e| e.map.0)
        .collect()
}

/// Which of the three mode sets are live, in walk order.
pub fn live_mode_maps(s: &InputShell) -> Vec<u32> {
    band(s)
        .into_iter()
        .filter(|m| MODE_MAPS.contains(m))
        .collect()
}

/// None of the three is in the client's own start-up table.
pub fn none_at_start_up() -> bool {
    !BASE_MAP_REGISTRATIONS
        .iter()
        .any(|(_, m, _)| MODE_MAPS.contains(m))
}

/// Every `(action, control)` the shipped keymap carries for one set of keys.
pub fn shipped_bindings(s: &InputShell, map: u32) -> Vec<(ActionId, ControlChord)> {
    s.manager
        .keymap
        .section(InputMapId(map))
        .map(|sec| sec.bindings().iter().map(|(q, a)| (*a, *q)).collect())
        .unwrap_or_default()
}

/// The one control the shipped keymap binds `action` to in `map`, discovered rather than
/// written down.
pub fn the_shipped_control(s: &InputShell, map: u32, action: u32) -> ControlChord {
    let section = s
        .manager
        .keymap
        .section(InputMapId(map))
        .unwrap_or_else(|| panic!("the shipped keymap has a section for {map:#010X}"));
    let mut found: Vec<ControlChord> = section
        .bindings()
        .iter()
        .filter(|(_, a)| a.0 == action)
        .map(|(q, _)| *q)
        .collect();
    found.dedup();
    assert_eq!(
        found.len(),
        1,
        "{action:#010X} has exactly one shipped default control in {map:#010X}"
    );
    found[0]
}

/// The controls all three combat modes bind -- taken from the melee set's own shipped
/// section, so the population is the data and not a list here.
pub fn contested_controls(s: &InputShell) -> Vec<ControlChord> {
    let out: Vec<ControlChord> = shipped_bindings(s, MELEE_MAP)
        .iter()
        .map(|(_, q)| *q)
        .collect();
    assert!(!out.is_empty(), "the shipped melee section binds something");
    out
}

/// The one control the set that belongs to combat itself binds: the calibration key.
pub fn the_combat_maps_own_control(s: &InputShell) -> ControlChord {
    let b = shipped_bindings(s, COMBAT_MAP);
    assert_eq!(
        b.len(),
        1,
        "the shipped combat section binds exactly one control"
    );
    b[0].1
}

/// The mode and the mode sets a **running** client is carrying.
pub fn live_in(c: &mut HeadlessClient) -> (u32, Vec<u32>) {
    let s = c
        .app_mut()
        .input_manager_mut()
        .expect("the input shell is up");
    (
        s.combat_input_mode(),
        s.manager
            .maps
            .entries()
            .iter()
            .map(|e| e.map.0)
            .filter(|m| MODE_MAPS.contains(m))
            .collect(),
    )
}

/// The shard's own mode change: the form that takes no readiness check and sends nothing back.
pub fn the_shard_sets_the_mode(c: &mut HeadlessClient, m: dereth_client_model::combat::CombatMode) {
    c.world_mut()
        .set_combat_mode(
            &mut dereth_client_model::NullRequests,
            &mut dereth_client_model::RecordingSink::default(),
            m,
            false,
            true,
            false,
        )
        .expect("the shard's own form takes no ready check");
}

/// One key press through the production shell, drained through the shell's own frame.
pub struct Driver {
    /// Well clear of the double-click window and of the button history, so two presses of one
    /// physical key are never read as a gesture of each other.
    clock: u32,
}

impl Driver {
    pub const fn new() -> Self {
        Self { clock: 1_000 }
    }

    fn press(&mut self, shell: &mut InputShell, qc: &ControlChord) -> Vec<(InputMapId, ActionId)> {
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
        shell.use_time(dereth_primitives::LocalTime(f64::from(t) / 1000.0));
        shell.take_events();

        shell
            .manager
            .fire_input_event(qc.control, ControlType::Button, 0x80, t);
        shell.use_time(dereth_primitives::LocalTime(f64::from(t) / 1000.0));
        // Membership rather than the first: an action state outlives the key that set it, so
        // a latched toggle's release can be queued ahead of the new action.
        let got: Vec<(InputMapId, ActionId)> = shell
            .take_events()
            .iter()
            .map(|e| (e.input_map, e.action))
            .collect();

        t += 10;
        shell
            .manager
            .fire_input_event(qc.control, ControlType::Button, 0, t);
        for cs in metas.iter().rev() {
            t += 10;
            shell
                .manager
                .fire_input_event(*cs, ControlType::Button, 0, t);
        }
        shell.use_time(dereth_primitives::LocalTime(f64::from(t) / 1000.0));
        shell.take_events();
        got
    }

    /// A press followed by its release, with any modifiers held across both, answering the
    /// two event batches the client's own input layer produced.
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
        shell.use_time(dereth_primitives::LocalTime(f64::from(t) / 1000.0));
        shell.take_events();

        shell
            .manager
            .fire_input_event(qc.control, ControlType::Button, 0x80, t);
        shell.use_time(dereth_primitives::LocalTime(f64::from(t) / 1000.0));
        let down = shell.take_events();

        t += 10;
        shell
            .manager
            .fire_input_event(qc.control, ControlType::Button, 0, t);
        shell.use_time(dereth_primitives::LocalTime(f64::from(t) / 1000.0));
        let up = shell.take_events();

        for cs in metas.iter().rev() {
            t += 10;
            shell
                .manager
                .fire_input_event(*cs, ControlType::Button, 0, t);
        }
        shell.use_time(dereth_primitives::LocalTime(f64::from(t) / 1000.0));
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

    /// The one action a press resolved to, or `None` for silence.
    pub fn resolve(
        &mut self,
        shell: &mut InputShell,
        qc: &ControlChord,
    ) -> Option<(InputMapId, ActionId)> {
        let got = self.press(shell, qc);
        assert!(
            got.len() <= 1,
            "a press resolved to {} actions: {got:?}",
            got.len()
        );
        got.first().copied()
    }
}
