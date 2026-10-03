//! The HUD's panel driver — everything on `Hud` that takes a live `&mut dereth_ui::UiSystem`.
//!
//! `hud.rs` is two things bolted together: a client-side model of what the HUD knows (the object
//! rows, the chat composition, the property caches, the `GameView` projection) and the code that
//! pushes that model into `dereth_ui`'s live element tree once per frame. This file is the second
//! half. Keeping it apart leaves `hud.rs` naming no `UiSystem` at all, which is what lets the
//! model half stay device-free.
//!
//! **The screen half is the screen's.** The gameplay screen's own work — its panel updates, its
//! message and drop deliveries, the panels the HUD owns across rebuilds — runs inside the screen,
//! reached through [`dereth_ui::framework::Screen::on_game`] with the calls of
//! [`dereth_ui_screens::screens::gameplay_host`]. What is left here is the HUD's half: the queues it
//! hands over, the settings it decides, and the counters it keeps. Each call is made at a fixed
//! point in the frame; moving one changes what the panels see.

use dereth_ui::framework::{GameCx, Screen, ScreenCx};
use dereth_ui_screens::screens::gameplay::PlayerSettingsView;
use dereth_ui_screens::screens::gameplay_host::{
    selection_query_counts, FrameCall, GameCall, ObjectNoticeCall, PanelInputCall,
};

use crate::hud::{character_option, decode_chat_filters, decode_chat_opacity, Hud, HudView};

/// The gameplay screen, when it is the screen that is up.
pub fn game_screen(flow: &mut dereth_ui::UiFlow) -> Option<&mut dyn Screen> {
    flow.current_mut().filter(|s| s.is_game()).map(|s| &mut **s)
}

/// Check that the gameplay screen took a host call it was handed.
///
/// Every call reaches the screen as `&mut dyn Any` (the UI framework's screen trait cannot name
/// the screen crate's call types), and the screen answers `false` when the call is not of its
/// vocabulary or the lent panels are not the ones it expects. On the gameplay screen every call
/// of the vocabulary is taken when the host lends what it needs, so an untaken call means the
/// two sides disagree about a type and the update it carried was skipped. Debug builds stop at
/// the call site; release builds log a warning once and carry on, as the skipped update would.
#[track_caller]
pub fn expect_taken(took: bool, call: &dyn std::fmt::Debug) {
    if took {
        return;
    }
    let what = format!("{call:?}");
    let what: String = what.chars().take(80).collect();
    debug_assert!(took, "the gameplay screen did not take the host call {what}: the call and the lent panels must be the gameplay screen's own types");
    static SAID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !SAID.swap(true, std::sync::atomic::Ordering::Relaxed) {
        tracing::warn!(
            "the gameplay screen did not take the host call {what}; the update was skipped"
        );
    }
}

/// Make one [`GameCall`] with nothing lent and no game view, and hand back its answers.
///
/// `screen` is the gameplay screen ([`game_screen`]); the call must be taken ([`expect_taken`]).
#[track_caller]
pub fn game_call(
    ui: &mut dereth_ui::UiSystem,
    screen: &mut dyn Screen,
    call: GameCall,
) -> GameCall {
    let mut call = call;
    let took = screen.on_game(&mut ScreenCx::new(ui), &mut GameCx::call(&mut call));
    expect_taken(took, &call);
    call
}

/// Make one pre-game host call on `screen` (the current screen, whichever it is), and hand back
/// whether the screen took it and its answers.
pub fn pregame_call(
    ui: &mut dereth_ui::UiSystem,
    screen: &mut dyn Screen,
    call: dereth_ui_screens::screens::pregame_host::PregameCall,
) -> (bool, dereth_ui_screens::screens::pregame_host::PregameCall) {
    let mut call = call;
    let took = screen.on_host_call(&mut ScreenCx::new(ui), &mut call);
    (took, call)
}

/// Make one [`GameCall`] against `view`, and hand back its answers. The call must be taken.
#[track_caller]
pub fn game_call_with_view(
    ui: &mut dereth_ui::UiSystem,
    screen: &mut dyn Screen,
    view: &dyn dereth_client_contract::GameView,
    call: GameCall,
) -> GameCall {
    let mut call = call;
    let took = screen.on_game(
        &mut ScreenCx::with_view(ui, view),
        &mut GameCx::call(&mut call),
    );
    expect_taken(took, &call);
    call
}

/// Fan one consumed combat display batch to all registered power-bar subscribers, in source
/// order. Hiding the power bar sends Combat-zero *before* storing Undef; it must therefore
/// be delivered with its original mode, not with `HudView::combat_bar`'s final snapshot mode.
///
/// Used by `App::ui_use_time` after the real panel bindings are current and before the draw list,
/// and by socket-free hosts of the same panels. This does not infer events from model state.
pub fn deliver_power_bar_notices(
    ui: &mut dereth_ui::UiSystem,
    panels: &mut dereth_ui_screens::panels::remaining::RemainingPanels,
    notices: impl IntoIterator<Item = dereth_client_model::combat::PowerBarNotice>,
) -> u32 {
    use dereth_client_model::combat::PowerBarNotice;
    use dereth_ui_screens::hud::powerbar::mode_from_u32;
    let mut accepted = 0;
    for notice in notices {
        match notice {
            PowerBarNotice::Begin {
                mode,
                melee,
                recklessness_sac,
            } => {
                for bar in panels.power_bar.subscribers() {
                    accepted += u32::from(bar.begin(
                        ui,
                        mode_from_u32(mode as u32),
                        melee,
                        recklessness_sac,
                    ));
                }
            }
            PowerBarNotice::SetLevel { mode, level } => {
                let mode = mode_from_u32(mode as u32);
                accepted += u32::from(panels.combat_window.on_set_powerbar_level(ui, mode, level));
                for bar in panels.power_bar.subscribers() {
                    accepted += u32::from(bar.set_level(ui, mode, level));
                }
            }
            PowerBarNotice::Finish { mode } => {
                for bar in panels.power_bar.subscribers() {
                    accepted += u32::from(bar.finish(ui, mode_from_u32(mode as u32)));
                }
            }
        }
    }
    accepted
}

/// One talk-focus enable notice, delivered to the gameplay screen against the live row and the
/// current talk focus. Notices go in sender order, before the next input or command; the mask
/// cannot be polled, since false/true can leave a selected channel at Say. Answers whether the
/// screen reset the talk focus to All.
pub fn talk_focus_notice(
    ui: &mut dereth_ui::UiSystem,
    screen: &mut dyn Screen,
    talk_focus: dereth_client_model::chat::TalkFocus,
    notice: dereth_client_model::chat::TalkFocusNotice,
) -> bool {
    let call = game_call(
        ui,
        screen,
        GameCall::ChatFocus {
            talk_focus: talk_focus as u32,
            focus: notice.focus as u32,
            enabled: notice.enabled,
            is_olthoi: notice.is_olthoi,
            reset: false,
        },
    );
    matches!(call, GameCall::ChatFocus { reset: true, .. })
}

impl Hud {
    /// The host completes non-element panel subscribers after each screen delivery. No UI
    /// tick or server answer is synthesized here; standalone callers still drain them in drive.
    pub(crate) fn dispatch_panel_input(
        &mut self,
        ui: &mut dereth_ui::UiSystem,
        screen: &mut dyn Screen,
        screen_serial: u64,
        objects: &crate::objects::ObjectStream,
        target_mode_active: bool,
    ) {
        if self.panels_bound_to != Some(screen_serial) {
            return;
        }
        // This entry point builds its own views between input actions, so it needs
        // the same refresh `drive` takes; the guard inside makes the second call free.
        self.refresh_display_names(&objects.world);
        let mut panels = std::mem::take(&mut self.panels);
        let mut call = GameCall::PanelInput(PanelInputCall {
            target_mode_active,
            trace_raise: crate::trace::raise(),
            ..PanelInputCall::default()
        });
        {
            let view = self.view(objects);
            let took = screen.on_game(
                &mut ScreenCx::with_view(ui, &view),
                &mut GameCx::lending(&mut call, &mut panels),
            );
            expect_taken(took, &call);
        }
        self.panels = panels;
        if let GameCall::PanelInput(c) = call {
            self.stats.panel_messages_consumed += c.panel_messages_consumed;
            self.stats.panel_messages += c.panel_messages;
            self.stats.trade_drops += c.trade_drops;
            self.stats.house_drops += c.house_drops;
        }
    }

    /// Selection and container notices read live game facts. Complete their item-facing UI
    /// consumers before the next input action, without advancing the rest of the HUD twice.
    pub(crate) fn refresh_item_input_views(
        &mut self,
        ui: &mut dereth_ui::UiSystem,
        screen: &mut dyn Screen,
        objects: &crate::objects::ObjectStream,
    ) {
        let mut panels = std::mem::take(&mut self.panels);
        let mut call = GameCall::RefreshItemInputs;
        {
            let view = self.view(objects);
            let took = screen.on_game(
                &mut ScreenCx::with_view(ui, &view),
                &mut GameCx::lending(&mut call, &mut panels),
            );
            expect_taken(took, &call);
        }
        self.panels = panels;
    }

    /// Read the gameplay screen's option pages again from the store and the character, with
    /// the live view. Returns how many rows moved.
    pub(crate) fn reread_option_pages(
        &mut self,
        ui: &mut dereth_ui::UiSystem,
        screen: &mut dyn Screen,
        objects: &crate::objects::ObjectStream,
    ) -> usize {
        let mut call = GameCall::RereadOptions(0);
        let view = self.view(objects);
        let took = screen.on_game(
            &mut ScreenCx::with_view(ui, &view),
            &mut GameCx::call(&mut call),
        );
        expect_taken(took, &call);
        match call {
            GameCall::RereadOptions(moved) => moved,
            _ => 0,
        }
    }

    /// Hand everything to the live gameplay screen, once per frame. `screen_serial`
    /// distinguishes constructions so update_from_player_module runs again after replacement.
    pub fn drive(
        &mut self,
        ui: &mut dereth_ui::UiSystem,
        screen: &mut dyn Screen,
        screen_serial: u64,
        objects: &crate::objects::ObjectStream,
    ) {
        // Network link status first: the link lamp reads it through the view, and `UiSystem::now`
        // is the clock that connection-status calculation subtracts from.
        self.now = ui.now;
        // Before the view is built, because every panel that draws a name reads it
        // through that view.
        self.refresh_display_names(&objects.world);
        self.link_status = crate::net::link_status_holder::connection_status(ui.now.0);
        let world = &objects.world;

        // A received player description feeds every window's `update_from_player_module`. Run once
        // per description, not once per frame. See [`AppliedKey`] for what the key
        // is a superset of, and for why `lock_ui` is not in it. Decided here, before the screen
        // runs, from HUD state the screen's pass does not touch.
        let key = self.applied_key(world, screen_serial, character_option::SIDE_BY_SIDE_VITALS);
        let player_settings = if self.applied_to.as_ref() != Some(&key)
            && self.player_module.is_some()
        {
            self.applied_to = Some(key);
            // The two chat-opacity gameplay options, decoded here for the same
            // reason the placements are: the blob belongs to player state and a screen has none.
            let (chat_default_opacity, chat_active_opacity) = self
                .player_module
                .as_ref()
                .map_or((None, None), decode_chat_opacity);
            // The chat-filter read belongs here for the same reason: querying a
            // window's filter option reads the chat-option structure in player state.
            let chat_filters = self
                .player_module
                .as_ref()
                .map(decode_chat_filters)
                .unwrap_or_default();
            Some(PlayerSettingsView {
                placements: self.placements.clone(),
                side_by_side_vitals: self.option_bit(world, character_option::SIDE_BY_SIDE_VITALS),
                lock_ui: self.lock_ui(),
                chat_default_opacity,
                chat_active_opacity,
                chat_filters,
            })
        } else {
            None
        };

        // **The auto-target producer**, the three remembered names, and the stay-in-chat-mode
        // bit: host-pushed values, once a frame, unguarded.
        let target = self.auto_target_world(world);
        self.stats.auto_target_in_range =
            u64::try_from(target.in_range_of_player.len()).unwrap_or(0);
        let chat_target_squelched = world.chat.last_speakable_target.is_some_and(|t| {
            // `(id, "", 1)` — the same three arguments the menu's selection setter passes, and the value the
            // squelch menu row toggles against.
            world.chat.is_squelched(t, "", 1)
        });
        let chat = self.take_chat_lines(screen_serial);
        let frame = FrameCall {
            rebind: self.panels_bound_to != Some(screen_serial),
            external_container: std::mem::take(&mut self.pending_external_container),
            // The same one-frame hop, for `OpenSalvagePanel`: counting it alone would not open
            // the panel.
            salvage: std::mem::take(&mut self.pending_salvage),
            raise_answered: std::mem::take(&mut self.raise_answered),
            env_page_visibility: self.env_page_visibility,
            trace_plus_ten_last: self.trace_plus_ten_last.take(),
            trace_raise: crate::trace::raise(),
            trace_notice: crate::trace::notice(),
            player_settings,
            reply_targets: self.reply_targets(world),
            stay_in_chat_mode: world
                .player_system
                .options
                .get(dereth_client_model::player::options::option::STAY_IN_CHAT_MODE),
            chat_target_squelched,
            auto_target_world: target,
            chat,
            out: Default::default(),
        };

        // The panels are lent for the duration: `HudView` borrows `self` immutably and the panels
        // are driven mutably — `GameView` is read-only by construction.
        let mut panels = std::mem::take(&mut self.panels);
        let mut call = GameCall::Frame(Box::new(frame));
        {
            let view = self.view(objects);
            let took = screen.on_game(
                &mut ScreenCx::with_view(ui, &view),
                &mut GameCx::lending(&mut call, &mut panels),
            );
            expect_taken(took, &call);
        }
        self.panels = panels;
        let GameCall::Frame(f) = call else { return };
        let o = &f.out;
        if o.bound {
            self.panels_bound_to = Some(screen_serial);
        }
        self.env_page_visibility = f.env_page_visibility;
        self.trace_plus_ten_last = f.trace_plus_ten_last;

        if o.map_written {
            self.stats.panels_written += 1;
        }
        self.stats.item_slot_rings += u64::try_from(o.toolbar.rings).unwrap_or(0);
        self.stats.item_slot_heartbeats += u64::try_from(o.beats).unwrap_or(0);
        self.stats.salvage_notices_delivered += o.salvage_notices_delivered;
        self.stats.panel_messages += o.panel_messages;
        self.stats.panel_messages_consumed += o.panel_messages_consumed;
        self.stats.trade_drops += o.trade_drops;
        self.stats.house_drops += o.house_drops;
        self.stats.vendor_sell_drops += o.vendor_sell_drops;
        self.stats.salvage_drops += o.salvage_drops;
        self.stats.raises_answered += o.raises_answered;
        self.stats.panels_written += u64::from(o.panels_written);
        self.stats.skill_rows = o.skill_rows;
        self.stats.spell_slots = o.spell_slots;
        self.stats.shortcuts_written += u64::from(o.shortcuts);
        self.stats.inventory_written += u64::from(o.inventory);
        self.stats.indicators_written += u64::from(o.indicators);
        self.stats.examinations_filled += u64::from(o.examination);
        // How many frames the book panel wrote on.
        self.stats.book_frames += u64::from(o.book);
        self.stats.vitals_written += u64::from(o.vitals);
        // The two halves of the selection-changed handler are counted separately
        // because they are separately guarded.
        self.stats.toolbar_written += u64::from(o.toolbar.wrote);
        self.stats.split_gate_runs += u64::from(o.toolbar.split_gate_ran);
        // Which of the selection-changed handler's two vital queries the
        // not-a-stack arm asked for, and the edge block's clear.
        let (health, mana, declined, unanswerable) = selection_query_counts(&o.toolbar);
        self.stats.selection_health_queries += health;
        self.stats.selection_mana_queries += mana;
        self.stats.selection_queries_declined += declined;
        self.stats.selection_queries_unanswerable += unanswerable;
        self.stats.selection_query_clears += u64::from(o.toolbar.cleared);
        self.stats.selection_meters_written += o.selection_meters_written;
        self.stats.character_option_rows_reread += o.character_option_rows_reread;
        self.stats.character_option_edges += o.character_option_edges;
        self.stats.character_option_rows_noticed += o.character_option_rows_noticed;
        self.stats.chat_option_controls_reread += o.chat_option_controls_reread;
        self.stats.chat_option_edges += o.chat_option_edges;
        self.stats.radar_written += u64::from(o.radar);
        self.stats.placements_applied += o.placements_applied;

        // The HUD-side mirror of the lock, after the authoritative write has run
        // through `PlayerSystem::set_option(51, ..)`. It has to be **after** the
        // `update_from_player_module` pass: that pass seeds a freshly built screen *from* the
        // module, this one writes a toggled screen *into* it. Ordered the other way round this one
        // would first store the new screen's default `false` over the module's `true` and the
        // re-seed would then read back the value it had just destroyed.
        self.set_lock_ui(o.locked);

        for took in &o.chat_taken {
            if *took == 0 {
                self.stats.chat_lines_dropped += 1;
            } else {
                self.stats.chat_lines += 1;
            }
        }
    }

    /// Existing inventory/toolbar subscribers at an object notice boundary, not a second
    /// Hud tick. The panel refills from the live owner while the
    /// old Weenie still exists; the inventory window also checks its open-container edge.
    /// The existing panel implementations retain their container/snapshot/selection guards.
    pub(crate) fn object_notice(
        &mut self,
        ui: &mut dereth_ui::UiSystem,
        screen: &mut dyn Screen,
        world: &dereth_client_model::World,
        notice: &dereth_client_model::Notice,
    ) {
        use dereth_client_model::Notice;
        // **Handle the enchantments-changed notice.**
        //
        // The skills UI inherits the registered handler. It updates from the player description,
        // walking its row list and, for each row, recomputing the buffed and the raw skill value
        // and rewriting the row's value and its buffed/debuffed font.
        //
        // [`Self::rebuild_panel_tables`] is this build's form of that walk: the skills page reads
        // the cached `Hud::skills` join, and that join is a pure function of the skill table and
        // player quality state, so re-running it *is* running on every row.
        // `RemainingPanels::update` then redraws on the next frame, guarded on its own snapshot.
        //
        // `Notice::VitaeChanged` is deliberately **not** here: the skill UI registers for
        // enchantment changes, while vitae changes are absent from its notice set.
        if matches!(notice, Notice::EnchantmentsChanged) {
            self.rebuild_panel_tables(world);
        }
        let inventory = matches!(
            notice,
            Notice::ItemMoved { .. } | Notice::ItemAttributesChanged { .. }
        );
        if inventory {
            // remake_character_inventory reads the current placement list, not last frame's cache.
            self.equipment.clear();
            if let Some(inv) = world
                .player
                .or(self.player)
                .and_then(|p| world.inventory(p))
            {
                self.equipment
                    .extend(inv.placements.iter().map(|p| (p.iid, p.loc)));
            }
        }
        let toolbar = matches!(
            notice,
            Notice::SelectionChanged { .. } | Notice::ItemAttributesChanged { .. }
        );
        let moved = if let Notice::ItemMoved { object, .. } = notice {
            Some(*object)
        } else {
            None
        };
        if !inventory && !toolbar && moved.is_none() {
            return;
        }
        let mut panels = std::mem::take(&mut self.panels);
        let mut call = GameCall::ObjectNotice(ObjectNoticeCall {
            inventory,
            toolbar,
            moved,
            ..ObjectNoticeCall::default()
        });
        {
            let view = HudView { hud: self, world };
            let took = screen.on_game(
                &mut ScreenCx::with_view(ui, &view),
                &mut GameCx::lending(&mut call, &mut panels),
            );
            expect_taken(took, &call);
        }
        self.panels = panels;
        let GameCall::ObjectNotice(c) = call else {
            return;
        };
        if inventory {
            self.stats.inventory_written += u64::from(c.inventory_written);
        }
        if let Some(toolbar) = c.toolbar_out {
            self.stats.toolbar_written += u64::from(toolbar.wrote);
            self.stats.split_gate_runs += u64::from(toolbar.split_gate_ran);
            self.stats.item_slot_rings += u64::try_from(toolbar.rings).unwrap_or(0);
            self.stats.selection_query_clears += u64::from(toolbar.cleared);
            let (health, mana, declined, unanswerable) = selection_query_counts(&toolbar);
            self.stats.selection_health_queries += health;
            self.stats.selection_mana_queries += mana;
            self.stats.selection_queries_declined += declined;
            self.stats.selection_queries_unanswerable += unanswerable;
        }
    }
}
