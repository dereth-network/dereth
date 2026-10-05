//! Scripted runs drive the same screens and actions as physical input.

use super::{ClientShell, Cx, Ui};
use crate::platform::host::Host;
use dereth_client_runtime::app::EnterWorldScript;
/// How far `--enter-world` has driven the character-management and character-generation screens.
///
/// The retail client has no script: the *screen* is what enters the world, and `-u`/`-user` and
/// `-r`/`-create` are what the launcher passes to say which character. `--enter-world` presses
/// the screen's own buttons instead of calling the session directly, so a
/// headless run exercises the path a player does rather than a parallel one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum PregameDrive {
    /// Nothing pressed yet.
    #[default]
    Idle,
    /// *Create Character* has been pressed; waiting for the wizard.
    OpeningWizard,
    /// The heritage, town and summary page have been chosen; the name and *Finish* follow on the
    /// next frame, once the summary page is up.
    Naming,
    /// The wizard has been filled in and *Finish* pressed; waiting for the credit warning, which
    /// the wizard's finish step raises and which cannot be seen until the click has been
    /// delivered — the element messages are queued, so the answer is a second frame's work.
    Finishing,
    /// The credit warning has been answered and `0xF656` has gone out.
    Creating,
    /// A row has been double-clicked; `EnterGame` has run.
    Entering,
}

/// The object `--cast` selects before casting `spell_id`: none for a spell cast on the caster or
/// on no one, else the player when the client's target test takes it, else the first worn, then
/// carried, item it takes.
fn scripted_cast_target(
    world: &dereth_client_model::world::World,
    spell_id: u32,
) -> Option<dereth_primitives::ObjectId> {
    let table = world.magic.spell_table.as_ref()?;
    let base = table.spells.get(&spell_id)?;
    let target_type = dereth_client_model::magic::spell_target_type(base);
    if base.bitfield & dereth_client_model::magic::spell_index::SELF_TARGETED != 0
        || target_type == 0
    {
        return None;
    }
    let player = world.player?;
    let mut candidates = vec![player];
    if let Some(inv) = world.inventory(player) {
        candidates.extend(inv.placements.iter().map(|p| p.iid));
        candidates.extend(inv.items.iter().copied());
    }
    candidates.into_iter().find(|&id| {
        world
            .object_compatible_with_spell_target_type(
                &mut dereth_client_model::NullSink,
                Some(id),
                target_type,
                true,
            )
            .is_ok()
    })
}

/// The first caster (wand, staff or orb) the player carries, which `--cast` wields to enter
/// magic mode as a player would.
fn scripted_caster(
    world: &dereth_client_model::world::World,
) -> Option<dereth_primitives::ObjectId> {
    let inv = world.inventory(world.player?)?;
    inv.items.iter().copied().find(|&id| {
        world
            .weenie(id)
            .is_some_and(|w| w.inq_type() & dereth_rules::weenie::item_type::CASTER != 0)
    })
}

/// A key a scripted run presses, as its action's begin.
fn scripted_key(id: u32) -> dereth_client_contract::actions::Action {
    dereth_client_contract::actions::Action {
        id: dereth_client_contract::actions::ActionId(id),
        phase: dereth_client_contract::actions::ActionPhase::Begin,
        extent: 1.0,
        repeats: 0,
    }
}

/// How far `--cast` has driven the world screen: the backpack opened, a caster wielded, magic
/// mode entered, the target selected, the spell cast, peace again, then the closest compass item
/// used.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(super) enum WorldDrive {
    /// Not in the world yet.
    #[default]
    Idle,
    /// In the world since this time; the inventory and the materialization settle first.
    Arrived(f64),
    /// The backpack button was pressed at this time.
    BackpackOpened(f64),
    /// The carried caster was used (wielded) at this time.
    CasterWielded(f64),
    /// The combat toggle was pressed at this time.
    MagicMode(f64),
    /// The target, if the spell needs one, was selected at this time.
    TargetChosen(f64),
    /// The cast was asked for at this time.
    Cast(f64),
    /// The combat toggle was pressed again (back to peace) at this time.
    Peace(f64),
    /// The closest compass item (the nearest person) was selected at this time.
    Approaching(f64),
    /// Walking up to it; nothing more to do.
    Done,
}

/// How far `--say` and `--use` have got. The configuration is the run's script and stays as it
/// was given; the lines said and the targets used are counted here.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(super) struct SayDrive {
    /// When the last line was sent or the last target acted on (or the first may be).
    last: Option<f64>,
    /// How many `--say` lines have been sent.
    said: usize,
    /// How many `--use` targets have been used.
    used: usize,
    /// Whether the closest item has been selected and waits to be used.
    selected: bool,
}

impl<H: Host> Ui<'_, '_, H> {
    /// `--enter-world`, pressed into the pre-game screens rather than into the session.
    ///
    /// Every click here is an element-message broadcast on the element the layout
    /// actually carries, and every consequence is the screen's own: `EnterGame` for the
    /// double-click and the finish step for the wizard. Nothing touches the desktop, so this
    /// is usable as evidence in a way an injected keystroke is not.
    pub(super) fn drive_pregame_screens(&mut self) {
        use dereth_ui::framework::mode;
        use dereth_ui::msg::element::id::BUTTON_CLICKED;
        use dereth_ui::{ElementId, MessageId};

        if !self.cx.config().enter_world {
            return;
        }
        let wanted = if self.cx.config().create_char.is_empty() {
            self.cx.config().start_char.clone()
        } else {
            self.cx.config().create_char.clone()
        };
        let creating = !self.cx.config().create_char.is_empty();
        let create_char = self.cx.config().create_char.clone();
        let script = self.cx.entry_script();
        let mut pregame = self.front.pregame;
        let Some(shell) = self.front.ui.as_mut() else {
            return;
        };
        match shell.flow.current_mode() {
            Some(mode::CHARACTER_MANAGEMENT) => {
                if pregame == PregameDrive::OpeningWizard
                    || script != EnterWorldScript::AwaitingCharacterSet
                {
                    return;
                }
                let root = match shell.flow.current() {
                    Some(s) if !s.roots().is_empty() => s.roots()[0],
                    _ => return,
                };
                // `-r`/`-create`: press *Create Character* (`0x100003A0`) rather than entering.
                if creating && pregame == PregameDrive::Idle {
                    if let Some(h) = shell.ui.get_child_recursive(root, ElementId(0x1000_03A0)) {
                        tracing::info!(target: "dereth_client_shell::front_end", "pressing Create Character");
                        shell.ui.broadcast_element_message(h, BUTTON_CLICKED, 7, 0);
                        self.front.pregame = PregameDrive::OpeningWizard;
                    }
                    return;
                }
                // Otherwise pick the row `-u` named, or the first, and double-click it.
                let row = {
                    use dereth_ui_screens::screens::pregame_host::PregameCall;
                    let Some(s) = shell.flow.current_mut() else {
                        return;
                    };
                    let (took, call) = crate::hud_drive::pregame_call(
                        &mut shell.ui,
                        &mut **s,
                        PregameCall::PickCharacterRow {
                            wanted: wanted.clone(),
                            out: None,
                        },
                    );
                    let PregameCall::PickCharacterRow { out, .. } = call else {
                        return;
                    };
                    if !took {
                        return;
                    }
                    out
                };
                let Some(row) = row else { return };
                tracing::info!(target: "dereth_client_shell::front_end", "double-clicking a character row");
                shell
                    .ui
                    .broadcast_element_message(row, BUTTON_CLICKED, 7, 0);
                shell
                    .ui
                    .broadcast_element_message(row, MessageId(0x1A), 7, 0);
                pregame = PregameDrive::Entering;
            }
            Some(mode::CHAR_GEN) if pregame == PregameDrive::OpeningWizard => {
                use dereth_ui_screens::screens::chargen::{HERITAGE_BUTTONS, TOWN_BUTTONS};
                let root = match shell.flow.current() {
                    Some(s) if !s.roots().is_empty() => s.roots()[0],
                    _ => return,
                };
                // Aluvian and Holtburg: the wizard's first heritage and first town, and the two
                // that need no expansion. The scripted path does not visit the appearance and
                // profession pages, so the character takes the default template's six fifties.
                // Then the summary tab: the finish button answers only on the summary page.
                for id in [
                    HERITAGE_BUTTONS[0].0,
                    TOWN_BUTTONS[0].0,
                    dereth_ui_screens::screens::chargen::EcgProgress::Summary
                        .select_button()
                        .expect("the summary tab"),
                ] {
                    if let Some(h) = shell.ui.get_child_recursive(root, id) {
                        shell.ui.broadcast_element_message(h, BUTTON_CLICKED, 7, 0);
                    }
                }
                // The name and Finish are the next frame's: the summary page, once shown,
                // writes its own text into the name field.
                pregame = PregameDrive::Naming;
            }
            Some(mode::CHAR_GEN) if pregame == PregameDrive::Naming => {
                use dereth_ui_screens::screens::chargen::NAME_FIELD;
                let root = match shell.flow.current() {
                    Some(s) if !s.roots().is_empty() => s.roots()[0],
                    _ => return,
                };
                if let Some(h) = shell.ui.get_child_recursive(root, NAME_FIELD) {
                    if let Some(t) = shell.ui.text_element_mut(h) {
                        t.set_text(&create_char);
                    }
                    shell.ui.broadcast_element_message(h, MessageId(0x44), 0, 0);
                }
                if let Some(h) = shell.ui.get_child_recursive(root, ElementId(0x1000_03C8)) {
                    tracing::info!(target: "dereth_client_shell::front_end", "creating {create_char:?} -- heritage 1, Holtburg");
                    shell.ui.broadcast_element_message(h, BUTTON_CLICKED, 7, 0);
                }
                pregame = PregameDrive::Finishing;
            }
            Some(mode::CHAR_GEN) if pregame == PregameDrive::Finishing => {
                use dereth_ui_screens::screens::pregame_host::PregameCall;
                // The finish step refuses while attribute credits are unspent, and the default
                // template always leaves 30 of the 330. Answering the warning is the player's
                // second click; here it is the same call the dialog-close handler makes.
                // It has to be a later frame than the Finish click: element messages are queued and
                // delivered inside `UiShell::frame`, so the dialog does not exist yet when the
                // click is broadcast.
                if let Some(w) = shell.flow.current_mut() {
                    let (_, open) = crate::hud_drive::pregame_call(
                        &mut shell.ui,
                        &mut **w,
                        PregameCall::CreditWarningOpen(false),
                    );
                    if matches!(open, PregameCall::CreditWarningOpen(true)) {
                        tracing::info!(target: "dereth_client_shell::front_end", "answering the unspent-credit warning");
                        let (took, _) = crate::hud_drive::pregame_call(
                            &mut shell.ui,
                            &mut **w,
                            PregameCall::CloseDialog(true),
                        );
                        // The screen that just said its warning is open is the wizard, so it
                        // must take the answer too.
                        debug_assert!(took, "the creation wizard did not take the dialog answer");
                        pregame = PregameDrive::Creating;
                    }
                }
            }
            _ => {}
        }
        self.front.pregame = pregame;
    }

    /// `--say`, submitted from the world screen: five seconds after arriving, then one line every
    /// two seconds, each as the chat window's Send submits it. Each line is sent once. Then each
    /// `--use` target, six seconds apart, used as a double-click uses it; `closest` is first
    /// selected by the closest-compass-item key's action and used a second later, and `logout`
    /// ends the character session as the confirmed logout button does.
    pub(super) fn drive_say(&mut self, now: f64) {
        use dereth_ui::framework::mode;

        let drive = self.front.say_drive;
        let say_left = self.cx.config().say.len().saturating_sub(drive.said);
        let use_left = self
            .cx
            .config()
            .use_targets
            .len()
            .saturating_sub(drive.used);
        if say_left == 0 && use_left == 0 {
            return;
        }
        let in_world = self.cx.pregame().in_world;
        let Some(shell) = self.front.ui.as_mut() else {
            return;
        };
        if !in_world || shell.flow.current_mode() != Some(mode::GAME_PLAY) {
            return;
        }
        let Some(last) = drive.last else {
            self.front.say_drive.last = Some(now + 3.0);
            return;
        };
        if say_left == 0 {
            let target = self.cx.config().use_targets[drive.used].clone();
            let id = target
                .strip_prefix("0x")
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .map(dereth_primitives::ObjectId);
            if let Some(id) = id {
                if now - last < 6.0 {
                    return;
                }
                self.front.say_drive.used += 1;
                tracing::info!(target: "dereth_client_shell::front_end", "using {id:?}");
                shell
                    .ui
                    .requests
                    .emit(dereth_ui_screens::view::UiRequest::Use(id));
            } else if target == "logout" {
                if now - last < 6.0 {
                    return;
                }
                self.front.say_drive.used += 1;
                tracing::info!(target: "dereth_client_shell::front_end", "logging out, as the confirmed logout button does");
                shell
                    .ui
                    .requests
                    .emit(dereth_ui_screens::view::UiRequest::EndCharacterSession { ask: false });
            } else if drive.selected {
                if now - last < 1.0 {
                    return;
                }
                self.front.say_drive.selected = false;
                self.front.say_drive.used += 1;
                let world = self.cx.model();
                if let Some(id) = world.selected {
                    let name = world
                        .weenie(id)
                        .map(|w| w.object_name(dereth_client_model::weenie::NameType::Appropriate));
                    tracing::info!(target: "dereth_client_shell::front_end", "using the closest compass item {id:?} {name:?}");
                    shell
                        .ui
                        .requests
                        .emit(dereth_ui_screens::view::UiRequest::Use(id));
                }
            } else if now - last >= 6.0 {
                tracing::info!(target: "dereth_client_shell::front_end", "selecting the closest compass item");
                self.cx.inject_action(scripted_key(0x1000_002F));
                self.front.say_drive.selected = true;
            } else {
                return;
            }
            self.front.say_drive.last = Some(now);
            return;
        }
        if now - last < 2.0 {
            return;
        }
        let text = self.cx.config().say[drive.said].clone();
        self.front.say_drive.said += 1;
        tracing::info!(target: "dereth_client_shell::front_end", "saying {text:?} ({} more to say)", say_left - 1);
        shell
            .ui
            .requests
            .emit(dereth_ui_screens::view::UiRequest::ChatLine { text, window: 0 });
        self.front.say_drive.last = Some(now);
    }

    /// `--cast`, pressed into the world screen: the backpack button, the carried caster used
    /// (which wields it), the combat toggle (magic mode, with a caster in hand), a selection when
    /// the spell wants a target, the spell bar's cast, the toggle back to peace, and then a walk
    /// up to the closest compass item, used.
    ///
    /// The backpack is opened by the element message its toolbar button takes from a click, the
    /// caster by the use request a double-click raises, the mode by the toggle's action, and the
    /// cast is the request the spell bar's Cast button raises, so the client's own component
    /// check runs before anything goes to the server. The waits let the world settle: the
    /// inventory arrives with the player description and the body materializes after it.
    pub(super) fn drive_world_script(&mut self, now: f64) {
        use dereth_ui::framework::mode;
        use dereth_ui::msg::element::id::BUTTON_CLICKED;
        use dereth_ui_screens::toolbar::INVENTORY_BUTTON;

        let Some(spell_id) = self.cx.config().cast else {
            return;
        };
        let in_world = self.cx.pregame().in_world;
        let drive = self.front.world_drive;
        let Some(shell) = self.front.ui.as_mut() else {
            return;
        };
        if !in_world || shell.flow.current_mode() != Some(mode::GAME_PLAY) {
            return;
        }
        self.front.world_drive = match drive {
            WorldDrive::Idle => WorldDrive::Arrived(now),
            WorldDrive::Arrived(t) if now - t >= 5.0 => {
                let root = match shell.flow.current() {
                    Some(s) if !s.roots().is_empty() => s.roots()[0],
                    _ => return,
                };
                let Some(h) = shell.ui.get_child_recursive(root, INVENTORY_BUTTON) else {
                    return;
                };
                tracing::info!(target: "dereth_client_shell::front_end", "opening the backpack");
                shell.ui.broadcast_element_message(h, BUTTON_CLICKED, 7, 0);
                WorldDrive::BackpackOpened(now)
            }
            WorldDrive::BackpackOpened(t) if now - t >= 2.0 => {
                // Spells are cast in magic mode, and magic mode wants a caster in hand.
                if let Some(caster) = scripted_caster(self.cx.model()) {
                    tracing::info!(target: "dereth_client_shell::front_end", "wielding the caster {caster:?}");
                    shell
                        .ui
                        .requests
                        .emit(dereth_ui_screens::view::UiRequest::Use(caster));
                }
                WorldDrive::CasterWielded(now)
            }
            WorldDrive::CasterWielded(t) if now - t >= 3.0 => {
                tracing::info!(target: "dereth_client_shell::front_end", "entering combat (magic) mode");
                self.cx.inject_action(scripted_key(0x1000_005A));
                WorldDrive::MagicMode(now)
            }
            WorldDrive::MagicMode(t) if now - t >= 3.0 => {
                // A spell cast at another needs a selection, as a player's would: the player,
                // else the first worn or carried item the client's own target test accepts.
                if let Some(target) = scripted_cast_target(self.cx.model(), spell_id) {
                    tracing::info!(target: "dereth_client_shell::front_end", "selecting {target:?} for spell {spell_id}");
                    shell
                        .ui
                        .requests
                        .emit(dereth_ui_screens::view::UiRequest::Select(target));
                }
                WorldDrive::TargetChosen(now)
            }
            WorldDrive::TargetChosen(t) if now - t >= 1.0 => {
                tracing::info!(target: "dereth_client_shell::front_end", "casting spell {spell_id}");
                shell
                    .ui
                    .requests
                    .emit(dereth_ui_screens::view::UiRequest::CastSpell { spell_id });
                WorldDrive::Cast(now)
            }
            // Only once the cast is over (the server's use-done has come back): leaving magic
            // mode earlier breaks the cast off.
            WorldDrive::Cast(t) if now - t >= 2.0 && self.cx.model().magic.busy_count == 0 => {
                tracing::info!(target: "dereth_client_shell::front_end", "leaving combat (peace) mode");
                self.cx.inject_action(scripted_key(0x1000_005A));
                WorldDrive::Peace(now)
            }
            WorldDrive::Peace(t) if now - t >= 3.0 => {
                // Then go up to the nearest person or thing, as the frame's close-up.
                tracing::info!(target: "dereth_client_shell::front_end", "selecting the closest compass item");
                self.cx.inject_action(scripted_key(0x1000_002F));
                WorldDrive::Approaching(now)
            }
            WorldDrive::Approaching(t) if now - t >= 1.0 => {
                let world = self.cx.model();
                if let Some(id) = world.selected {
                    let name = world
                        .weenie(id)
                        .map(|w| w.object_name(dereth_client_model::weenie::NameType::Appropriate));
                    tracing::info!(target: "dereth_client_shell::front_end", "walking up to {id:?} {name:?}");
                    shell
                        .ui
                        .requests
                        .emit(dereth_ui_screens::view::UiRequest::Use(id));
                }
                WorldDrive::Done
            }
            other => other,
        };
    }
}

impl<H: Host> ClientShell<H> {
    pub(super) fn scripted_pregame(&mut self, cx: &mut Cx<'_, H>) {
        if self.classic.active {
            return;
        }
        Ui {
            cx,
            front: &mut self.modern,
            shared: &mut self.shared,
        }
        .drive_pregame_screens();
    }

    pub(super) fn scripted_world(&mut self, cx: &mut Cx<'_, H>, now: dereth_primitives::LocalTime) {
        // `--action-at`: each action as its key would give it. A window toggle goes through the
        // interface's own input maps; any other action to the game, as an unclaimed key does.
        for action in cx.take_scripted_actions() {
            let window = dereth_input::names::enum_name_for_action(action).starts_with("Toggle");
            let own = dereth_input::dereth::name(action).is_some();
            if let Some(ui) = self.classic.active_mut() {
                ui.press_action(action);
            } else if let (true, Some(input)) = (own, self.shared.input.as_mut()) {
                // This client's own actions, in their own map, pressed and let go.
                for start in [true, false] {
                    input.inject_action(dereth_input::InputEvent {
                        action,
                        input_map: dereth_input::dereth::INPUT_MAP,
                        toggle: dereth_input::ToggleType::Hold,
                        extent: 1.0,
                        start,
                        repeat_delta: 0,
                        repeat_total: 0,
                        from_key_down: start,
                    });
                }
            } else if !window {
                cx.inject_action(dereth_client_runtime::actions::Action::begin(action));
                cx.inject_action(dereth_client_runtime::actions::Action::end(action));
            } else if let Some(input) = self.shared.input.as_mut() {
                // On the map a key of it is bound in, else the UI's.
                let input_map = input
                    .manager
                    .keymap
                    .sections
                    .iter()
                    .find(|s| !s.keys_for_action(action).is_empty())
                    .map_or(crate::ui::UI_INPUT_MAP, |s| s.input_map_id);
                input.inject_action(dereth_input::InputEvent {
                    action,
                    input_map,
                    toggle: dereth_input::ToggleType::OneShot,
                    extent: 1.0,
                    start: true,
                    repeat_delta: 1,
                    repeat_total: 0,
                    from_key_down: false,
                });
            }
        }
        if self.classic.active {
            return;
        }
        let mut ui = Ui {
            cx,
            front: &mut self.modern,
            shared: &mut self.shared,
        };
        ui.drive_world_script(now.0);
        ui.drive_say(now.0);
    }
}
