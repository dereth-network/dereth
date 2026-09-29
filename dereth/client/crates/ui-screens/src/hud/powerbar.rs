//! `PowerBar`, `CombatWindow` and `CombatPanelStack` — the charge bar and the combat cluster.
//!
//! A rebuild must keep the begin / set-level / finish power-bar notice triple and
//! the `PowerBarMode` guard **or the jump bar and the attack bar will fight**. The guard is what
//! this module exists for.

use dereth_ui::{ElemHandle, ElementId, ElementType, UiSystem};

use crate::bind::{attr, bind_children, set_attr_enum, set_attr_float, Bound};

/// The power bar's mode, which the combat state decides and every bar guards on.
pub use dereth_client_contract::powerbar::PowerBarMode;

/// The meter every one of the four notice handlers writes: child `0x10000034`, whose level
/// attribute `0x69` it sets.
pub const BAR: ElementId = ElementId(0x1000_0034);
/// The client's **text** element, not a second meter — child `0x10000035`, cast to a text element
/// and given a string, and it carries the patcher's file name.
///
/// Read back from the shipped `classic_gameplay` tree: `0x10000035` is element type `0x0C` and is a
/// **child of** `0x10000034`, not its sibling. They are not "two meters `PowerBar` drives, both
/// with attribute `0x69`"; only `0x10000034` ever takes `0x69`.
pub const SECONDARY_BAR: ElementId = ElementId(0x1000_0035);
/// The recklessness sub-field the post-init caches **and hides**.
pub const RECKLESSNESS_FIELD: ElementId = ElementId(0x1000_05EE);
/// The powerbar panel's element-type answer.
pub const POWERBAR_TYPE: ElementType = ElementType(0x1000_000F);
/// The `<PBAR>` floaty window, which **derives from** `PowerBar`: it chains the base constructor
/// and answers to `0x10000053` **and** `0x1000000F`. So it
/// inherits all four power-bar notice handlers and is a second, independent power bar with its
/// own current mode and its own `0x10000034`.
pub const FLOATY_POWERBAR_TYPE: ElementType = ElementType(0x1000_0053);
/// The element state the bar switches to when it is re-purposed as the live dat-patch progress bar
/// by the in-game patch-status notice.
pub const DDD_STATE: u32 = 4;
/// The file-name string variable's placeholder when the patcher has not named a file.
pub const DDD_UNKNOWN_FILE: &str = "???";

/// One `PowerBar` **object** -- not the class. The shipped `classic_gameplay` tree carries
/// **two**, and each keeps its own current power-bar mode and its own `0x10000034`.
#[derive(Debug, Default)]
pub struct PowerBar {
    /// The `PowerBar` element itself -- the one the begin-powerbar notice's tail shows, and the
    /// subtree every recursive child search here searches.
    pub element: Option<ElemHandle>,
    /// The bar (`0x10000034`), the secondary bar (`0x10000035`) and the recklessness field
    /// (`0x100005EE`), from the catalogue's own table.
    pub bound: Bound,
    /// The current power-bar mode. Construction sets `PBM_UNDEF`, and **only** the
    /// begin-powerbar notice's tail and the finish-powerbar notice ever move it.
    pub mode: PowerBarMode,
    /// The last level written to `0x10000034`, 0..=1.
    pub level: f32,
    /// The element's own visibility. `Begin` shows it, `Finish` hides it, and the shipped layout
    /// starts both instances hidden.
    pub visible: bool,
    /// Whether this object's post-init registered the power-bar notices with the global event
    /// handler -- see [`registers_power_bar_notices`]. A bar that registered none receives none,
    /// so nothing ever shows it.
    pub registered: bool,
}

/// **Which `PowerBar` objects are subscribers -- and it is only the floaty one.**
///
/// The powerbar panel's post-init is short and all of it is accounted for: the base post-init, a
/// recursive search for `0x100005EE`, a cast to a field, caching and hiding it -- with no notice
/// registration anywhere in it. Only the floaty power bar's post-init -- which chains to it
/// first -- registers, for the begin, level and finish power-bar notices, the dat-patch status
/// and the player description. Sending a notice walks only the handlers registered for it, so an
/// object that registers nothing hears nothing.
///
/// The shipped `classic_gameplay` tree carries one object of each class: `0x10000044` (type
/// `0x1000000F`, inside `<SBOX>`, spanning the bottom of the screen) and `0x10000613` (`<PBAR>`,
/// type `0x10000053`, the movable Advanced-Combat window). **Only `<PBAR>` is reachable by a
/// notice**; the plain `PowerBar` object is inert in retail and can never become visible.
///
/// Fanning every notice to both would draw the jump/power bar twice -- once at the bottom of the
/// screen and once in the middle.
#[must_use]
pub const fn registers_power_bar_notices(ty: ElementType) -> bool {
    ty.0 == FLOATY_POWERBAR_TYPE.0
}

impl PowerBar {
    /// The powerbar panel's post-init -- the base post-init, then a recursive search for
    /// `0x100005EE`, a cast to a field, cache, **and hide it**.
    pub fn post_init(&mut self, ui: &mut UiSystem, element: ElemHandle) {
        self.element = Some(element);
        self.mode = PowerBarMode::Undef;
        self.level = 0.0;
        self.visible = ui.node(element).is_some_and(|n| n.region.flags.visible);
        // Which class's post-init ran decides whether this object is on any notice list at all.
        self.registered = ui
            .node(element)
            .is_some_and(|n| registers_power_bar_notices(n.ty()));
        let table = crate::panels::catalogue::spec("PowerBar").map_or(&[][..], |s| s.children);
        self.bound = bind_children(ui, element, table);
        if let Some(h) = self.bound.get("recklessness_field") {
            ui.set_visible(h, false);
        }
    }

    /// The powerbar panel's begin-power-bar notice.
    ///
    /// **`PBM_COMBAT` does nothing at all**, and that is the whole reason the classic combat
    /// window carries a meter of its own:
    ///
    /// Advanced combat selects state `0x10000043` in melee and shows the Recklessness
    /// field only when skill `0x32` has advancement class at least 2. Other combat modes
    /// use state `0x10000044` and hide that field. Jump selects `0x10000042`; DDD selects
    /// `0x10000045`. Undefined and out-of-range mode values skip caption selection.
    /// All paths except plain combat then store the mode, reset child `0x10000034`'s
    /// level attribute `0x69` to zero and show the power bar.
    ///
    /// Returns whether the notice did anything, which is the observable half of the `PBM_COMBAT`
    /// hole. [verified against retail]
    pub fn begin(
        &mut self,
        ui: &mut UiSystem,
        mode: PowerBarMode,
        melee: bool,
        recklessness_sac: u32,
    ) -> bool {
        let caption = match mode {
            // Plain combat returns immediately: no state, no mode, no show.
            PowerBarMode::Combat => return false,
            PowerBarMode::AdvancedCombat => Some(if melee {
                ADVANCED_COMBAT_MELEE_STATE
            } else {
                ADVANCED_COMBAT_STATE
            }),
            PowerBarMode::Jump => Some(JUMP_STATE),
            PowerBarMode::Ddd => Some(DDD_PATCH_STATE),
            // Undefined goes straight to the tail: no caption or recklessness write.
            PowerBarMode::Undef => None,
        };
        if let (Some(state), Some(element)) = (caption, self.element) {
            ui.set_state(element, dereth_ui::StateId(state));
            if let Some(h) = self.bound.get("recklessness_field") {
                let show = mode == PowerBarMode::AdvancedCombat
                    && melee
                    && recklessness_sac >= RECKLESSNESS_TRAINED;
                ui.set_visible(h, show);
            }
        }
        self.mode = mode;
        self.level = 0.0;
        if let Some(h) = self.bound.get("bar") {
            set_attr_float(ui, h, attr::METER_LEVEL, 0.0);
        }
        self.visible = true;
        if let Some(element) = self.element {
            ui.set_visible(element, true);
        }
        true
    }

    /// The set-power-bar-level notice: "**if the mode matches**, set `0x10000034`'s
    /// attribute `0x69` to the level".
    ///
    /// Returns whether the level was accepted, which is the observable half of the guard.
    pub fn set_level(&mut self, ui: &mut UiSystem, mode: PowerBarMode, level: f32) -> bool {
        if mode != self.mode {
            return false;
        }
        self.level = level;
        if let Some(h) = self.bound.get("bar") {
            set_attr_float(ui, h, attr::METER_LEVEL, level);
        }
        true
    }

    /// The finish-power-bar notice: if the mode matches, clear the mode, set
    /// the bar to 0 and hide the power bar.
    pub fn finish(&mut self, ui: &mut UiSystem, mode: PowerBarMode) -> bool {
        if mode != self.mode {
            return false;
        }
        self.mode = PowerBarMode::Undef;
        self.level = 0.0;
        self.visible = false;
        if let Some(h) = self.bound.get("bar") {
            set_attr_float(ui, h, attr::METER_LEVEL, 0.0);
        }
        if let Some(element) = self.element {
            ui.set_visible(element, false);
        }
        true
    }

    /// The in-game patch-status notice "switches the element state to 4 and
    /// re-purposes the bar as the **live dat-patch progress bar**".
    pub fn runtime_ddd_status(&mut self, ui: &mut UiSystem, fraction: f32) {
        self.mode = PowerBarMode::Ddd;
        self.visible = true;
        self.level = fraction;
        if let Some(h) = self.bound.get("bar") {
            set_attr_enum(ui, h, attr::MEDIA_STATE, DDD_STATE);
            set_attr_float(ui, h, attr::METER_LEVEL, fraction);
        }
    }
}

/// The four states the begin-powerbar notice sets. They are **element states**, not string ids.
pub const ADVANCED_COMBAT_MELEE_STATE: u32 = 0x1000_0043;
/// See [`ADVANCED_COMBAT_MELEE_STATE`].
pub const ADVANCED_COMBAT_STATE: u32 = 0x1000_0044;
/// See [`ADVANCED_COMBAT_MELEE_STATE`].
pub const JUMP_STATE: u32 = 0x1000_0042;
/// See [`ADVANCED_COMBAT_MELEE_STATE`].
pub const DDD_PATCH_STATE: u32 = 0x1000_0045;
/// `0x0A` arm uses.
pub const RECKLESSNESS_TRAINED: u32 = 2;

/// **The producer.** Every `PowerBar` in the screen, driven off the same
/// [`crate::view::CombatBar`] snapshot the combat window reads, so the two displays of one value
/// cannot diverge.
///
/// # The notice stream is ordered, not a mode snapshot
///
/// The host fans the combat system's journal to every bound bar and the combat-window meter.
/// A final mode cannot reconstruct the following transitions when multiple producers run before
/// the next UI frame. In particular Combat-zero precedes Undef, and hide/restart may end in the
/// original mode. The original send sites have these shapes:
///
/// * Every begin site (the commence-attack handler's jump) stores the new mode, then sends
///   `SetPowerbarLevel(PBM_COMBAT, 0.0)` if the new mode is combat and `BeginPowerbar(new mode)`
///   otherwise.
/// * Every finish site (the execute-attack tail and attack-done handling) takes the old mode and
///   clears the build-in-progress flag and start time, then sends
///   `SetPowerbarLevel(PBM_COMBAT, 0.0)` if the old mode was combat and `FinishPowerbar(old)`
///   otherwise, and finally resets the mode to `PBM_UNDEF`.
///
/// **Declared deviation: deferred to the next UI frame**, because `App::ui_use_time` runs before
/// `App::interaction_use_time`. Order and the original mode arguments are retained within it.
#[derive(Debug, Default)]
pub struct PowerBars {
    /// One per `PowerBar` object in the tree, in depth-first order.
    pub bars: Vec<PowerBar>,
}

/// `PowerBarMode` from the snapshot's `u32`. Anything outside `1..=4` is `PBM_UNDEF`, which is
/// what the begin-powerbar notice's range check does with it.
#[must_use]
pub const fn mode_from_u32(v: u32) -> PowerBarMode {
    match v {
        1 => PowerBarMode::Combat,
        2 => PowerBarMode::AdvancedCombat,
        3 => PowerBarMode::Jump,
        4 => PowerBarMode::Ddd,
        _ => PowerBarMode::Undef,
    }
}

impl PowerBars {
    /// Bind every `PowerBar` under `root`.
    ///
    /// The class test accepts either type `0x1000000F` or `0x10000053`. The shipped
    /// `classic_gameplay` tree has one
    /// of each -- `0x10000044` inside `<SBOX>` and `0x10000613`, the `<PBAR>` floaty -- and both
    /// start hidden.
    pub fn post_init(&mut self, ui: &mut UiSystem, root: ElemHandle) {
        self.bars.clear();
        let mut found = Vec::new();
        collect(ui, root, &mut found);
        for element in found {
            let mut b = PowerBar::default();
            b.post_init(ui, element);
            self.bars.push(b);
        }
    }

    /// How many `PowerBar` objects were bound -- the denominator that separates "the bar did
    /// not move" from "there is no bar on this layout".
    #[must_use]
    pub fn bound(&self) -> usize {
        self.bars.len()
    }

    /// The bound objects that are on the power-bar notice lists, i.e. the ones a
    /// power-bar notice can reach. See [`registers_power_bar_notices`].
    pub fn subscribers(&mut self) -> impl Iterator<Item = &mut PowerBar> {
        self.bars.iter_mut().filter(|b| b.registered)
    }
}

/// Depth-first, so the two instances come back in tree order.
fn collect(ui: &UiSystem, h: ElemHandle, out: &mut Vec<ElemHandle>) {
    if ui
        .node(h)
        .is_some_and(|n| n.ty() == POWERBAR_TYPE || n.ty() == FLOATY_POWERBAR_TYPE)
    {
        out.push(h);
    }
    for c in ui.children(h) {
        collect(ui, c, out);
    }
}

/// `CombatWindow` — the three notice→element→attribute rows.
pub mod combat {
    use dereth_ui::ElementId;

    /// The client writes attribute `0xB1` (media state) on this
    /// element.
    pub const ATTACK_HEIGHT_ELEMENT: ElementId = ElementId(0x1000_0056);
    /// The three media states: high, medium, low.
    pub const ATTACK_HEIGHT_STATES: [u32; 3] = [0x1000_0057, 0x1000_0058, 0x1000_0059];
    ///  (only `PBM_COMBAT`) writes `0x69` here.
    pub const ACTUAL_POWER_ELEMENT: ElementId = ElementId(0x1000_0050);
    /// The client writes `0x85` — the notch the player sets.
    pub const DESIRED_POWER_ELEMENT: ElementId = ElementId(0x1000_004F);

    /// The attack heights, in the order the three states are listed.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[allow(missing_docs)]
    pub enum AttackHeight {
        High = 0,
        Medium = 1,
        Low = 2,
    }

    /// The media state for one attack height.
    #[must_use]
    pub const fn state_for(h: AttackHeight) -> u32 {
        ATTACK_HEIGHT_STATES[h as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The power bar mode guard stops the jump bar and the attack bar fighting.
    #[test]
    fn the_power_bar_mode_guard_stops_the_jump_bar_and_the_attack_bar_fighting() {
        let mut ui = UiSystem::new((800, 600));
        let mut p = PowerBar::default();
        assert_eq!(p.mode, PowerBarMode::Undef);
        assert!(!p.visible);

        // Mode zero takes the default arm. The classic combat bar never reaches this widget.
        assert!(
            !p.begin(&mut ui, PowerBarMode::Combat, true, 0),
            "PBM_COMBAT is the empty arm"
        );
        assert_eq!(
            p.mode,
            PowerBarMode::Undef,
            "and it does not even take the mode"
        );
        assert!(!p.visible);
        assert!(
            !p.set_level(&mut ui, PowerBarMode::Combat, 0.4),
            "so a COMBAT level is refused"
        );

        // The advanced-combat bar does own the widget.
        assert!(p.begin(&mut ui, PowerBarMode::AdvancedCombat, true, 0));
        assert_eq!(p.mode, PowerBarMode::AdvancedCombat);
        assert!(p.visible);
        // A level for the *jump* bar arrives while the attack bar owns the widget: rejected.
        assert!(!p.set_level(&mut ui, PowerBarMode::Jump, 0.9));
        assert_eq!(p.level, 0.0);
        // The matching mode is accepted.
        assert!(p.set_level(&mut ui, PowerBarMode::AdvancedCombat, 0.4));
        assert_eq!(p.level, 0.4);
        // ...and a mismatched finish does not clear it either.
        assert!(!p.finish(&mut ui, PowerBarMode::Jump));
        assert_eq!(p.mode, PowerBarMode::AdvancedCombat);
        assert!(p.visible);
        assert!(p.finish(&mut ui, PowerBarMode::AdvancedCombat));
        assert_eq!(p.mode, PowerBarMode::Undef);
        assert_eq!(p.level, 0.0);
        assert!(!p.visible);
    }

    /// Oracle: the four-way mode table and its unsigned range check after subtracting one.
    ///
    /// The mode word arrives from the snapshot as a `u32`; the client's own range check is what
    /// decides which values reach an arm.
    #[test]
    fn the_snapshots_mode_word_maps_the_way_the_range_check_does() {
        assert_eq!(mode_from_u32(0), PowerBarMode::Undef);
        assert_eq!(mode_from_u32(1), PowerBarMode::Combat);
        assert_eq!(mode_from_u32(2), PowerBarMode::AdvancedCombat);
        assert_eq!(mode_from_u32(3), PowerBarMode::Jump);
        assert_eq!(mode_from_u32(4), PowerBarMode::Ddd);
        assert_eq!(
            mode_from_u32(5),
            PowerBarMode::Undef,
            "the range check sends >4 to the tail"
        );
        assert_eq!(mode_from_u32(u32::MAX), PowerBarMode::Undef);
    }

    /// The four state values the begin notice sets, pinned as literals rather than through the
    /// symbols that use them.
    #[test]
    fn the_four_begin_captions_are_the_four_pushed_states() {
        assert_eq!(ADVANCED_COMBAT_MELEE_STATE, 0x1000_0043);
        assert_eq!(ADVANCED_COMBAT_STATE, 0x1000_0044);
        assert_eq!(JUMP_STATE, 0x1000_0042);
        assert_eq!(DDD_PATCH_STATE, 0x1000_0045);
        assert_eq!(RECKLESSNESS_TRAINED, 2);
        assert_eq!(POWERBAR_TYPE.0, 0x1000_000F);
        assert_eq!(FLOATY_POWERBAR_TYPE.0, 0x1000_0053);
    }

    /// Oracle: §6's `PowerBarMode` enumeration.
    #[test]
    fn the_five_power_bar_modes_have_the_documented_values() {
        assert_eq!(PowerBarMode::Undef as i32, 0);
        assert_eq!(PowerBarMode::Combat as i32, 1);
        assert_eq!(PowerBarMode::AdvancedCombat as i32, 2);
        assert_eq!(PowerBarMode::Jump as i32, 3);
        assert_eq!(PowerBarMode::Ddd as i32, 4);
    }

    /// Oracle: §6's fourth notice row — "the same widget is the attack power bar, the jump charge
    /// bar and the in-game patch bar".
    #[test]
    fn the_patch_status_notice_repurposes_the_same_widget() {
        let mut ui = UiSystem::new((800, 600));
        let mut p = PowerBar::default();
        assert!(p.begin(&mut ui, PowerBarMode::Jump, false, 0));
        p.runtime_ddd_status(&mut ui, 0.5);
        assert_eq!(p.mode, PowerBarMode::Ddd);
        assert_eq!(p.level, 0.5);
        assert!(p.visible);
        assert_eq!(DDD_STATE, 4);
        assert_eq!(DDD_UNKNOWN_FILE, "???");
    }

    /// Oracle: §6's second table — the combat cluster's three element/attribute pairs and the
    /// three attack-height media states.
    #[test]
    fn the_combat_cluster_maps_its_notices_to_the_documented_elements() {
        use combat::AttackHeight;
        assert_eq!(combat::ATTACK_HEIGHT_ELEMENT, ElementId(0x1000_0056));
        assert_eq!(combat::state_for(AttackHeight::High), 0x1000_0057);
        assert_eq!(combat::state_for(AttackHeight::Medium), 0x1000_0058);
        assert_eq!(combat::state_for(AttackHeight::Low), 0x1000_0059);
        assert_eq!(combat::ACTUAL_POWER_ELEMENT, ElementId(0x1000_0050));
        assert_eq!(combat::DESIRED_POWER_ELEMENT, ElementId(0x1000_004F));
        // The two power elements use different attributes: the fill and the notch.
        assert_eq!(attr::METER_LEVEL, 0x69);
        assert_eq!(attr::MARKER, 0x85);
    }

    /// Oracle: — "caches the recklessness sub-field (`0x100005EE`) and
    /// **hides it**", and the two bars.
    #[test]
    fn the_power_bar_binds_the_documented_children() {
        assert_eq!(BAR, ElementId(0x1000_0034));
        assert_eq!(SECONDARY_BAR, ElementId(0x1000_0035));
        assert_eq!(RECKLESSNESS_FIELD, ElementId(0x1000_05EE));
        let spec = crate::panels::catalogue::spec("PowerBar").unwrap();
        assert!(spec.children.iter().any(|c| c.id == RECKLESSNESS_FIELD));
        assert!(spec.children.iter().any(|c| c.id == BAR));
        assert!(spec.children.iter().any(|c| c.id == SECONDARY_BAR));
    }
}
