//! Chat entry updates, command dispatch, and squelch queries.

use super::*;

impl Interaction {
    /// The shared timed target sweep runs independently of the active interface.
    pub fn update_chat_target(
        &mut self,
        world: &mut dereth_client_model::World,
        now: f64,
        facts: &dereth_client_contract::chat::mainchat::AutoTargetWorld,
    ) {
        if now < self.chat_target_next {
            return;
        }
        self.chat_target_next = now + 1.0;
        let chat = &mut world.chat;
        if chat.last_speakable_target.is_none()
            && chat.is_talk_focus_enabled(dereth_client_model::chat::TalkFocus::Selected)
        {
            chat.set_talk_focus_enabled(dereth_client_model::chat::TalkFocus::Selected, false);
        }
        if chat.is_talk_focus_enabled(dereth_client_model::chat::TalkFocus::Selected) {
            let target = chat.last_speakable_target.unwrap_or(ObjectId(0));
            let range_id = if facts.container_id == 0 {
                target.0
            } else {
                facts.container_id
            };
            if !facts.owned_by_player && !facts.in_range_of_player.contains(&range_id) {
                chat.set_speakable_target(None, false);
            }
        } else if facts.selected_id != 0
            && facts.selected_id != facts.player_id
            && facts.selected_talkable
            && facts.in_range_of_player.contains(&facts.selected_id)
        {
            chat.set_speakable_target(
                Some(ObjectId(facts.selected_id)),
                !facts.selected_name.is_empty(),
            );
        }
    }

    /// Entry readback is delivered before the next input listener runs.
    pub fn take_chat_entry_updates(
        &mut self,
    ) -> Vec<dereth_client_contract::chat::entry::EntryUpdate> {
        std::mem::take(&mut self.pending_chat_entries)
    }

    pub(super) fn edit_chat_entry(
        &mut self,
        game: &mut dereth_client_model::World,
        window: u32,
        text: String,
        action: dereth_client_contract::chat::entry::EntryAction,
    ) {
        let targets = game.chat.reply_targets();
        let (update, warning) = game.chat.entries.entry(window).or_default().apply(
            window,
            text,
            action,
            &targets,
            self.chat_interface,
        );
        if let Some(update) = update {
            self.pending_chat_entries.push(update);
        }
        if let Some(warning) = warning {
            game.scroll.add_feedback_to_scroll(
                warning,
                0x1a,
                true,
                window,
                dereth_client_contract::feedback::Feedback::WARNING,
            );
        }
    }

    /// Process the whole of what an ordinary spoken
    /// line does, which is **not** just the talk event.
    ///
    /// Walk the text, attempting pose extraction first between `*` delimiters and then
    /// between `<` and `>`. After processing all runs, trim whitespace on both sides.
    /// Send the remainder as speech (`0x0015`) only when it is nonempty.
    ///
    /// # Why this path exists
    ///
    /// `dereth_client_model::emotes::public_chat` and `emotes::pose` transcribe this
    /// — including the asymmetric cursor restore, the `%p` pronoun and the fact that
    /// the wire and the echo carry two *different* strings. Sending the line straight
    /// to `Request::Talk` instead would speak `*wave*` out loud, asterisks and all; no `0x01E1`
    /// could leave this client, no pose animation would play, and `Request::SoulEmote` would have
    /// no producer.
    ///
    /// # The three things one resolved run does, in the pose command's own order
    ///
    /// 1. the motion, **locally and immediately** — the one animation the client
    ///    predicts without waiting for the server. Queued for `App` rather than played here,
    ///    because the body lives on the scene;
    /// 2. soul-emote event `0x01E1`, carrying the
    ///    third-person form with `%p` replaced by the player's `Gender` pronoun;
    /// 3. the `Communication_HearSoulEmote` handler with sender 0, `"You"` and the pose's own
    ///    emote — the local echo,
    ///    with the **first**-person form. Sender id `0` short-circuits the hearing check, so it
    ///    can be neither squelched nor out of earshot, and the `^` the hear-soul-emote handler
    ///    appends makes it always understood; what reaches the scroll is therefore
    ///    `hear_emote_line("You", own_emote)` on type `0xC`, window 0.
    ///
    /// A run whose name misses `inq_chat_pose_command` is **silent and is left in the line**; a run
    /// that resolves but whose motion name misses `string2command` is broadcast and *also* left in
    /// the line, which is the pose command's return value being the `string2command` answer and nothing
    /// else. Both cases are `dereth_client_model::emotes`' and are not re-decided here.
    fn public_chat(
        &mut self,
        line: &str,
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
    ) {
        // Read player integer quality `0x71` (Gender) once per line, as the original
        // path reads it once per nonempty alternate emote. Absence yields 0 rather
        // than 1, selecting the "her" branch.
        let gender = game
            .player_qualities()
            .or_else(|| game.login_player_desc())
            .map_or(0, |q| q.inq_int(crate::hud::GENDER));
        let table = self.chat_pose_table.clone();
        let out = dereth_client_model::emotes::public_chat(line, |name| {
            // The native early return on a null table makes every run a miss, not a
            // crash and not a pose that half-fires.
            let t = table.as_deref()?;
            dereth_client_model::emotes::pose(t, name, gender, |n| {
                dereth_animation::command::MotionCommand::from_name(n).map(|c| c.0)
            })
        });
        for p in &out.poses {
            self.stats.poses_resolved += 1;
            if let Some(cmd) = p.motion_command {
                self.pending_pose_motions.push(cmd);
            }
            if let Some(text) = &p.soul_emote {
                dereth_client_model::RequestSink::send(
                    req,
                    Request::SoulEmote(dereth_protocol::comms::CommunicationSoulEmote {
                        message: text.clone(),
                    }),
                );
                self.stats.soul_emotes_sent += 1;
            }
            if let Some(text) = &p.my_emote {
                let name = crate::chat::soul_emote_sender_name(
                    dereth_client_model::emotes::LOCAL_ECHO_NAME,
                );
                let (_, trimmed) = crate::chat::language_marker(&name);
                let line = crate::chat::hear_emote_line(trimmed, text);
                game.scroll.add_feedback_to_scroll(
                    &line,
                    dereth_client_model::chat::text_type::EMOTE,
                    true,
                    0,
                    dereth_client_contract::feedback::Feedback::LOCAL,
                );
                self.stats.pose_echoes_printed += 1;
            }
        }
        if let Some(text) = out.speech {
            dereth_client_model::RequestSink::send(
                req,
                Request::Talk(dereth_protocol::comms::CommunicationTalk { message: text }),
            );
        }
    }

    /// What a [`dereth_client_model::chat_cmd::ChatCommand`] means on this side of the seam.
    ///
    /// Four kinds of effect, and the window each line goes to is the point of keeping them apart:
    ///
    /// * [`lines`](dereth_client_model::chat_cmd::ChatCommand::lines) are the handler's own
    ///   scroll write to the current command source's window — the chat window the command
    ///   was typed into;
    /// * [`failure_lines`](dereth_client_model::chat_cmd::ChatCommand::failure_lines) are
    ///   the failure-event handler's, and every one of its arms passes window **0**. Three of the four
    ///   reachable here are chat type `0` and `0x422`'s is `0x1A`, so the type travels with the
    ///   line rather than being a constant;
    /// * [`option`](dereth_client_model::chat_cmd::ChatCommand::option) changes a player option by
    ///   `ordinal`, which for an `is_auto_save_option` ordinal is a
    ///   `0x0005 Character_PlayerOptionChangedEvent` — the same path `@allegiance chat on` takes;
    /// * [`die_confirmation`](dereth_client_model::chat_cmd::ChatCommand::die_confirmation) is the one
    ///   chat command in retail that opens a dialog.
    ///
    /// `handled == false` is `do_command`'s failure event `0x26`. Only
    /// `@afk <unknown word>` can reach it in this family — every other refusal returns `true` and
    /// prints its own sentence.
    fn apply_chat_command(
        &mut self,
        cmd: dereth_client_model::chat_cmd::ChatCommand,
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
        out: &mut Notices,
        now: dereth_primitives::ServerTime,
    ) {
        let _ = out;
        for (text, ty) in &cmd.lines {
            game.scroll.add_feedback_to_scroll(
                text,
                *ty,
                true,
                self.chat.current_command_source,
                dereth_client_contract::feedback::Feedback::LOCAL,
            );
            self.stats.chat_command_lines += 1;
            if *ty == dereth_client_model::chat_cmd::REFUSAL_CHAT_TYPE {
                self.stats.chat_commands_refused += 1;
            }
        }
        for (text, ty) in &cmd.failure_lines {
            game.scroll.add_feedback_to_scroll(
                text,
                *ty,
                true,
                0,
                dereth_client_contract::feedback::Feedback::LOCAL,
            );
            self.stats.chat_command_lines += 1;
            self.stats.chat_commands_refused += 1;
        }
        if let Some((ordinal, value)) = cmd.option {
            let change = game.player_system.set_option(ordinal, value, now);
            for (ordinal, value) in &change.sends {
                dereth_client_model::RequestSink::send(
                    req,
                    Request::PlayerOptionChanged(
                        dereth_protocol::login::CharacterPlayerOptionChangedEvent {
                            option: u32::try_from(*ordinal).unwrap_or(u32::MAX),
                            value: u32::from(*value),
                        },
                    ),
                );
                self.stats.option_changes_sent += 1;
            }
            if change.deferred {
                self.stats.option_changes_deferred += 1;
            }
        }
        if let Some(prompt) = cmd.die_confirmation {
            self.pending_die_confirmations.push(prompt);
            self.stats.die_confirmations_raised += 1;
        }
        if let Some(prompt) = cmd.house_abandon_confirmation {
            self.pending_house_abandon_first.push(prompt);
            self.stats.house_abandon_first_raised += 1;
        }
        self.stats.chat_command_requests += cmd.sent;
        if !cmd.handled {
            game.scroll.add_feedback_to_scroll(
                dereth_client_model::cmd::NOT_A_VALID_COMMAND,
                0x1A,
                true,
                self.chat.current_command_source,
                dereth_client_contract::feedback::Feedback::LOCAL,
            );
            self.stats.chat_commands_refused += 1;
        }
    }

    /// Flip one player option from a `PlayerOption_*` input action.
    ///
    /// The new value is the opposite of the current one, written through
    /// [`dereth_client_model::player::PlayerSystem::set_option`]; every `0x0005` that write puts
    /// on the wire is sent, and a moved chat-listening option re-enables the Turbine talk focuses
    /// exactly as a tick on the page does.
    pub(super) fn toggle_player_option(
        &mut self,
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
        ordinal: usize,
        now: ServerTime,
    ) {
        let value = !game.player_system.options.get(ordinal);
        let change = game.player_system.set_option(ordinal, value, now);
        self.stats.option_actions_toggled += 1;
        if change.moved() && matches!(ordinal, 35 | 36 | 37 | 38 | 46) {
            let heritage = game
                .player_qualities()
                .or_else(|| game.login_player_desc())
                .map_or(0, |q| q.inq_int(0xBC));
            game.enable_chat_talk_focuses(crate::chat::is_olthoi(heritage));
        }
        for (ordinal, value) in &change.sends {
            dereth_client_model::RequestSink::send(
                req,
                Request::PlayerOptionChanged(
                    dereth_protocol::login::CharacterPlayerOptionChangedEvent {
                        option: u32::try_from(*ordinal).unwrap_or(u32::MAX),
                        value: u32::from(*value),
                    },
                ),
            );
            self.stats.option_changes_sent += 1;
        }
        if change.deferred {
            self.stats.option_changes_deferred += 1;
        }
    }

    /// Set the player's lock-UI option, including the virtual change-notification `(51)` tail.
    /// Both the radar and the lock-UI command enter here so they cannot disagree on the authoritative
    /// retained module, equality guard, or immediate `0x0005` wire shape.
    pub(super) fn apply_lock_ui_option(
        &mut self,
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
        locked: bool,
        now: ServerTime,
    ) {
        let change = game.player_system.set_option(
            dereth_client_model::player::options::option::LOCK_UI,
            locked,
            now,
        );
        for (ordinal, value) in &change.sends {
            dereth_client_model::RequestSink::send(
                req,
                Request::PlayerOptionChanged(
                    dereth_protocol::login::CharacterPlayerOptionChangedEvent {
                        option: u32::try_from(*ordinal).unwrap_or(u32::MAX),
                        value: u32::from(*value),
                    },
                ),
            );
            self.stats.option_changes_sent += 1;
        }
        if change.deferred {
            self.stats.option_changes_deferred += 1;
        }
    }

    /// Finish one `on_chat_command` result. Physical chat input and `LoadFile` both enter
    /// this exact dispatcher; their only differences happen before it (history and file echo).
    /// It keeps the `UiRequest::ChatLine` routing intact: unknown-command Talk, `public_chat`,
    /// tell/channel/Turbine routing, and the local handlers retain the same request types,
    /// focus/window arguments, and failure path.
    #[allow(clippy::too_many_arguments)] // one parameter per input the call takes
    pub(super) fn dispatch_chat_outcome(
        &mut self,
        outcome: dereth_client_model::cmd::CommandOutcome,
        window: u32,
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
        out: &mut Notices,
        now: dereth_primitives::ServerTime,
        player_desc_received: bool,
        chat_focus: &mut dyn FnMut(&mut dereth_client_model::chat::ChatState),
    ) {
        use dereth_client_model::cmd::CommandOutcome as O;
        match outcome {
            O::ForwardVerbatim(line) => {
                dereth_client_model::RequestSink::send(
                    req,
                    Request::Talk(dereth_protocol::comms::CommunicationTalk { message: line }),
                );
            }
            // on_chat_command's Say arm is public_chat, including its `*pose*` extraction.
            O::Chat {
                destination: dereth_client_model::cmd::TalkFocus::Say,
                text,
            } => {
                self.public_chat(&text, game, req);
            }
            // Tell focus with no current speakable target is silently dropped, not public speech.
            // The destination is the last speakable target, the name "Tell to <name>" shows, and
            // not the selection: once the main chat window has adopted a target it keeps it while
            // it is in range, whatever is selected after.
            O::Chat {
                destination: dereth_client_model::cmd::TalkFocus::Tell,
                text,
            } => {
                if let Some(target) = game.chat.last_speakable_target {
                    dereth_client_model::RequestSink::send(
                        req,
                        Request::TalkDirect(dereth_protocol::comms::CommunicationTalkDirect {
                            message: text,
                            target,
                        }),
                    );
                }
            }
            O::Chat { destination, text } if destination.channel_bit().is_some() => {
                let channel = destination.channel_bit().unwrap_or(0);
                dereth_client_model::RequestSink::send(
                    req,
                    Request::ChannelBroadcast(
                        dereth_protocol::comms::CommunicationChannelBroadcast {
                            channel,
                            message: text,
                        },
                    ),
                );
            }
            O::Handled {
                handler,
                verb,
                args,
            } => {
                self.chat_command(
                    handler,
                    &verb,
                    &args,
                    game,
                    req,
                    out,
                    now,
                    player_desc_received,
                    chat_focus,
                );
            }
            O::Failed(text) => {
                dereth_client_model::NoticeSink::emit(
                    out,
                    Notice::DisplayString {
                        feedback: dereth_client_contract::feedback::Feedback::LOCAL,
                        channel: dereth_client_model::chat::REFUSAL_CHANNEL,
                        text: text.to_owned(),
                    },
                );
                self.stats.chat_commands_refused += 1;
            }
            O::Chat { destination, text } => {
                if let Some(focus) =
                    dereth_client_model::chat::TalkFocus::from_raw(destination as u32)
                {
                    game.send_turbine_chat(req, focus, false, &text, window, chat_real_time());
                }
            }
            O::Empty => {}
        }
    }

    #[allow(clippy::too_many_arguments)] // one parameter per input the call takes
    fn chat_command(
        &mut self,
        handler: dereth_client_model::cmd::CommandHandler,
        verb: &str,
        args: &[String],
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
        out: &mut Notices,
        now: dereth_primitives::ServerTime,
        player_desc_received: bool,
        chat_focus: &mut dyn FnMut(&mut dereth_client_model::chat::ChatState),
    ) {
        use dereth_client_model::chat::TellOutcome as T;
        use dereth_client_model::cmd::CommandHandler as H;
        // `do_command` hands the handler `argc`/`argv`; all three tell handlers begin by re-joining
        // them with `join_args`, which is why the original spacing inside a command is
        // lost and a forwarded line's is not.
        let joined = dereth_client_model::cmd::interp::join_args(args);
        match handler {
            // **The `title` handler.** Sources 1 and 8 are the two main-chat aliases;
            // only the four popup sources may set the chat window title. Retail's stored string
            // length includes its NUL and must be below `0x64`, so at most 98 user characters are
            // admitted.
            H::Title => {
                let source = self.chat.current_command_source;
                let refusal = if source == 1 || source == 8 {
                    Some("This command must be issued from a popup chat window.")
                } else if joined.is_empty() {
                    Some("You must provide a new title for the window.")
                } else if joined.chars().count() >= 99 {
                    Some("Window title length cannot exceed 100 characters.")
                } else {
                    None
                };
                if let Some(text) = refusal {
                    game.scroll.add_feedback_to_scroll(
                        text,
                        dereth_client_model::chat::text_type::DEFAULT,
                        true,
                        source,
                        dereth_client_contract::feedback::Feedback::LOCAL,
                    );
                    self.stats.chat_command_lines += 1;
                    self.stats.chat_commands_refused += 1;
                } else {
                    self.pending_chat_window_titles.push((source, joined));
                }
            }

            // **Version reporting.** The original prints its executable resource
            // version, not protocol or dat versions. This build uses its Cargo identity in
            // the same literal shape. The privileged tail checks the same player Boolean
            // qualities as the privilege predicate and sends bare-Control F7CC after both
            // local lines, never an ordered game action.
            H::Version => {
                let source = self.chat.current_command_source;
                if !args.is_empty() {
                    game.scroll.add_feedback_to_scroll(
                        "Unexpected arguments to @version",
                        dereth_client_model::chat::text_type::LOCAL_ERROR,
                        true,
                        source,
                        dereth_client_contract::feedback::Feedback::LOCAL,
                    );
                    self.stats.chat_command_lines += 1;
                    self.stats.chat_commands_refused += 1;
                    return;
                }
                if game.chat.using_turbine_chat {
                    game.scroll.add_feedback_to_scroll(
                        "Using Turbine Chat.\n",
                        dereth_client_model::chat::text_type::DEFAULT,
                        true,
                        source,
                        dereth_client_contract::feedback::Feedback::LOCAL,
                    );
                    self.stats.chat_command_lines += 1;
                }
                game.scroll.add_feedback_to_scroll(
                    &format!("Client version {}\n", self.client_build_id),
                    dereth_client_model::chat::text_type::DEFAULT,
                    true,
                    source,
                    dereth_client_contract::feedback::Feedback::LOCAL,
                );
                self.stats.chat_command_lines += 1;
                let psr = game
                    .player_qualities()
                    .or_else(|| game.login_player_desc())
                    .is_some_and(|q| [0x2C, 0x2D, 0x61].into_iter().any(|p| q.inq_bool(p)));
                if psr {
                    dereth_client_model::RequestSink::send(
                        req,
                        Request::AdminGetServerVersion(
                            dereth_protocol::admin::AdminSendAdminGetServerVersion,
                        ),
                    );
                    self.stats.chat_command_requests += 1;
                }
            }

            // **`@messagetypes`.** The handler never reads argc/argv. It
            // obtains this process-local list, writes timestamped type zero to the current command
            // source, and returns true without a request; even extra arguments therefore print it.
            H::Messagetypes => {
                let line = message_types_text();
                game.scroll.add_feedback_to_scroll(
                    &line,
                    dereth_client_model::chat::text_type::DEFAULT,
                    true,
                    self.chat.current_command_source,
                    dereth_client_contract::feedback::Feedback::LOCAL,
                );
                self.stats.chat_command_lines += 1;
            }

            // **The `day` handler.** Retail does not inspect argc/argv: it toggles daylight,
            // prints the resulting sentence, then writes that same value
            // through the player-option setter. The retained option model is the source App's
            // ordinary `apply_player_option_effects` latch reads, so one write reaches both persistence
            // and the real landscape without a parallel lighting flag here. Ordinal 5 is deliberately
            // absent from `is_auto_save_option`; this dirties the module for its normal bulk flush and
            // sends no immediate `0x0005`.
            H::Day => {
                let ordinal = dereth_client_model::player::options::option::PERSISTENT_AT_DAY;
                let day = !game.player_system.options.get(ordinal);
                let change = game.player_system.set_option(ordinal, day, now);
                for (ordinal, value) in &change.sends {
                    dereth_client_model::RequestSink::send(
                        req,
                        Request::PlayerOptionChanged(
                            dereth_protocol::login::CharacterPlayerOptionChangedEvent {
                                option: u32::try_from(*ordinal).unwrap_or(u32::MAX),
                                value: u32::from(*value),
                            },
                        ),
                    );
                    self.stats.option_changes_sent += 1;
                }
                if change.deferred {
                    self.stats.option_changes_deferred += 1;
                }
                if change.effect.is_some() {
                    self.stats.option_side_effects_unapplied += 1;
                }
                let line = if day {
                    "Let there be light!"
                } else {
                    "Normality has been restored."
                };
                game.scroll.add_feedback_to_scroll(
                    line,
                    0x1A,
                    true,
                    self.chat.current_command_source,
                    dereth_client_contract::feedback::Feedback::LOCAL,
                );
                self.stats.chat_command_lines += 1;
            }

            // **`@render`.** Retail forwards argc/argv to
            // render-option handling, which reads only the option and value,
            // uses decimal `atoi`, and returns up to two strings. First output is type `0x1A`; usage is
            // type zero. A false usage return then reaches `do_command`'s generic failure `0x26`, while
            // an unknown option is a silent true.
            H::Render => {
                const USAGE: &str = "Usage:\n@render <option> <value>\n  radius #        : set landscape radius (between 5 and 25)\n  fov #           : set field of view (between 10 and 160)\n";
                let source = self.chat.current_command_source;
                let Some(option) = args.first() else {
                    game.scroll.add_feedback_to_scroll(
                        USAGE,
                        0,
                        true,
                        source,
                        dereth_client_contract::feedback::Feedback::LOCAL,
                    );
                    game.scroll.add_feedback_to_scroll(
                        dereth_client_model::cmd::NOT_A_VALID_COMMAND,
                        0x1A,
                        true,
                        0,
                        dereth_client_contract::feedback::Feedback::LOCAL,
                    );
                    self.stats.chat_command_lines += 2;
                    self.stats.chat_commands_refused += 1;
                    return;
                };
                if option.eq_ignore_ascii_case("usage") {
                    game.scroll.add_feedback_to_scroll(
                        USAGE,
                        0,
                        true,
                        source,
                        dereth_client_contract::feedback::Feedback::LOCAL,
                    );
                    game.scroll.add_feedback_to_scroll(
                        dereth_client_model::cmd::NOT_A_VALID_COMMAND,
                        0x1A,
                        true,
                        0,
                        dereth_client_contract::feedback::Feedback::LOCAL,
                    );
                    self.stats.chat_command_lines += 2;
                    self.stats.chat_commands_refused += 1;
                    return;
                }
                let output = if option.eq_ignore_ascii_case("radius") {
                    match args.get(1) {
                        None => Some("Must specify a radius"),
                        Some(value) => {
                            let radius = render_atoi(value) as u32;
                            if (5..=25).contains(&radius) {
                                self.pending_render_preferences.push((
                                    "Render.LandscapeDrawDistance",
                                    dereth_client_contract::view::PrefValue::Int(radius as i32),
                                ));
                                Some("Landscape radius set")
                            } else {
                                Some("Radius must be between 5 and 25")
                            }
                        }
                    }
                } else if option.eq_ignore_ascii_case("fov") {
                    match args.get(1) {
                        None => Some("Must specify a field of view"),
                        Some(value) => {
                            let fov = render_atoi(value) as u32;
                            if (10..=160).contains(&fov) {
                                self.pending_render_preferences.push((
                                    "Render.FieldOfView",
                                    dereth_client_contract::view::PrefValue::Float(fov as f32),
                                ));
                                Some("Field of view set")
                            } else {
                                Some("Field of view must be between 10 and 160")
                            }
                        }
                    }
                } else {
                    None
                };
                if let Some(output) = output {
                    game.scroll.add_feedback_to_scroll(
                        output,
                        0x1A,
                        true,
                        source,
                        dereth_client_contract::feedback::Feedback::LOCAL,
                    );
                    self.stats.chat_command_lines += 1;
                }
            }

            // **`@framerate`.** This is a process-local flag and a UI notice,
            // never a game action. The receiver needs the current render globals and live WorldObjects,
            // so App observes this flag after the ordinary command dispatch and performs the exact
            // show/update/hide edge there. Arguments are handled (TRUE) but do not change the flag.
            H::Framerate => {
                if args.is_empty() {
                    self.framerate_display = !self.framerate_display;
                    self.pending_framerate_display.push(self.framerate_display);
                } else {
                    game.scroll.add_feedback_to_scroll(
                        "Unexpected arguments to @framerate",
                        0x1A,
                        true,
                        self.chat.current_command_source,
                        dereth_client_contract::feedback::Feedback::LOCAL,
                    );
                    self.stats.chat_commands_refused += 1;
                }
            }

            // **`@loadfile`.** The joined name is
            // passed to `fopen("rt")` exactly as typed: unlike `@log`, there is no default extension.
            // Every `fgets(0x400)` chunk is substituted, displayed, then synchronously re-enters the
            // same `on_chat_command` outcome dispatcher as a physical chat line. File commands do not
            // enter typed-command history.
            H::Loadfile => {
                let source = self.chat.current_command_source;
                if joined.is_empty() {
                    game.scroll.add_feedback_to_scroll(
                        "You must provide a file name.",
                        dereth_client_model::chat::text_type::LOCAL_ERROR,
                        true,
                        source,
                        dereth_client_contract::feedback::Feedback::LOCAL,
                    );
                    self.stats.chat_command_lines += 1;
                    self.stats.chat_commands_refused += 1;
                    return;
                }

                let path = std::path::Path::new(&joined);
                let mut file = match std::fs::File::open(path) {
                    Ok(file) => file,
                    Err(_) => {
                        game.scroll.add_feedback_to_scroll(
                            &format!("Cannot open file {joined}"),
                            dereth_client_model::chat::text_type::LOCAL_ERROR,
                            true,
                            source,
                            dereth_client_contract::feedback::Feedback::LOCAL,
                        );
                        self.stats.chat_command_lines += 1;
                        self.stats.chat_commands_refused += 1;
                        return;
                    }
                };
                let mut bytes = Vec::new();
                // A read error makes native `fgets` stop and close the stream; opening, not a later
                // read failure, is the only path that prints "Cannot open file".
                let _ = std::io::Read::read_to_end(&mut file, &mut bytes);
                for chunk in dereth_client_model::cmd::loadfile::text_mode_chunks(&bytes) {
                    let real_time = i64::from(chat_real_time());
                    let date = dereth_client_model::cmd::loadfile::format_date(
                        real_time,
                        crate::platform::clock::local_utc_offset_secs(real_time),
                    );
                    // The original file loader treats each `fgets` result as a C string. Bytes after
                    // the first NUL were consumed in that fetched chunk but do not reach
                    // substitution, display or command parsing. The next fetched chunk still
                    // executes normally.
                    let native_len = chunk
                        .iter()
                        .position(|byte| *byte == 0)
                        .unwrap_or(chunk.len());
                    let narrow_line = dereth_protocol::cp1252::decode(&chunk[..native_len]);
                    let line = dereth_client_model::cmd::loadfile::make_variable_substitutions(
                        &narrow_line,
                        &date,
                    );
                    game.scroll.add_feedback_to_scroll(
                        &line,
                        0,
                        true,
                        source,
                        dereth_client_contract::feedback::Feedback::LOCAL,
                    );
                    self.stats.chat_command_lines += 1;
                    let focus =
                        dereth_client_model::cmd::TalkFocus::from_raw(game.chat.talk_focus as u32)
                            .unwrap_or(dereth_client_model::cmd::TalkFocus::Say);
                    let outcome = self.chat.on_chat_command(&line, source, focus);
                    self.dispatch_chat_outcome(
                        outcome,
                        source,
                        game,
                        req,
                        out,
                        now,
                        player_desc_received,
                        chat_focus,
                    );
                    self.stats.chat_lines_sent += 1;
                }
            }

            // **`@log`.** No arguments closes the native process-wide
            // append handle. A name gets `.txt` only when it has no extension, and replacement closes
            // the old handle before attempting the new open (`start_copy_output_to_file`).
            // This command does not apply the load-file variable substitutions: `%DATE%` is literal.
            H::Log => {
                let source = self.chat.current_command_source;
                if args.is_empty() {
                    let line = if game.scroll.close_log_file(source) {
                        "Chat output now directed only to the screen."
                    } else {
                        "Please specify a file to append chat messages to."
                    };
                    game.scroll.add_feedback_to_scroll(
                        line,
                        0,
                        true,
                        source,
                        dereth_client_contract::feedback::Feedback::LOCAL,
                    );
                    self.stats.chat_command_lines += 1;
                    return;
                }
                let mut path = std::path::PathBuf::from(&joined);
                if path
                    .extension()
                    .is_none_or(|extension| extension.is_empty())
                {
                    path = std::path::PathBuf::from(format!("{joined}.txt"));
                }
                let name = path.to_string_lossy();
                // The `fopen` is the host's: the model takes a callback and runs
                // it where `start_copy_output_to_file` runs it, after the close.
                let line = if game.scroll.start_copy_output_to_file(&name, source, || {
                    crate::platform::text::open_chat_log(&path)
                }) {
                    format!(
                    "Copying chat to {name}.  Run command again with no arguments to turn off logging."
                )
                } else {
                    self.stats.chat_commands_refused += 1;
                    format!("Failed to redirect to file {name}!")
                };
                game.scroll.add_feedback_to_scroll(
                    &line,
                    0,
                    true,
                    source,
                    dereth_client_contract::feedback::Feedback::LOCAL,
                );
                self.stats.chat_command_lines += 1;
            }

            H::Emote => {
                // The emote command silently accepts empty joined arguments. Nonempty text sends
                // the emote event, without public-speech/pose processing or a local echo.
                if let dereth_client_model::chat::EmoteOutcome::Send(message) =
                    dereth_client_model::chat::do_emote(&joined)
                {
                    dereth_client_model::RequestSink::send(
                        req,
                        Request::Emote(dereth_protocol::comms::CommunicationEmote { message }),
                    );
                }
            }

            // The channel-command wrapper delegates nonempty arguments to channel dispatch.
            // Nineteen command names share this one
            // handler — `@a @c @covassal @covassals @co-vassals @f @fellow @fellows @fellowship @g
            // @group @party @m @monarch @p @patron @v @vassal @vassals` — and without this arm every one
            // of them would reach the catch-all below and print *"That is not a valid command."*, while
            // the very same `0x0147` composed from the talk-focus dropdown goes out fine.
            //
            // Retail dispatches to the channel command only when `argc > 0`. Otherwise
            // it prints the missing-text message on chat type `0x1A` in the current command-source
            // window, with logging enabled, and return true.
            //
            // **The `true` is the load-bearing byte.** `do_command` only raises
            // failure event `0x26` — *"That is not a valid command."* — when a handler returns
            // `false`, so an empty `@f` says *"You must specify the text you wish to broadcast!"* and
            // nothing else. The retail literal is 48 wide characters.
            //
            // The channel command then resolves the channel from the current command — hence `verb` —
            // refuses `0` and `0x400` (`@help`), joins the rest and calls
            // channel broadcast `(id, text)`, returning **that** function's
            // answer. The `args.is_empty()` branch is `argc <= 0`, not "the text is blank": retail tests
            // the word count, so `@f " "` has argc 1 and broadcasts a space.
            H::ChannelShortcut => {
                if args.is_empty() {
                    // The scroll write of type `0x1A` to the current command source's window — the
                    // chat window the command was typed into, which this path *does* carry, unlike the
                    // `Notice::DisplayString` route, which loses it.
                    game.scroll.add_feedback_to_scroll(
                        CHANNEL_COMMAND_NEEDS_TEXT,
                        0x1A,
                        true,
                        self.chat.current_command_source,
                        dereth_client_contract::feedback::Feedback::LOCAL,
                    );
                    self.stats.channel_commands_without_text += 1;
                    return;
                }
                let channel = dereth_client_model::chat::get_channel_id(verb);
                if dereth_client_model::chat::channel_command_broadcasts_on(channel) {
                    dereth_client_model::RequestSink::send(
                        req,
                        Request::ChannelBroadcast(
                            dereth_protocol::comms::CommunicationChannelBroadcast {
                                channel,
                                message: joined,
                            },
                        ),
                    );
                    self.stats.channel_commands_sent += 1;
                } else {
                    // The channel-command handler returned `false`, so `do_command`'s
                    // failure event `0x26`
                    // *does* fire here. No word registered against this handler resolves to 0 or
                    // 0x400, so this arm is unreachable from the shipped table and exists because the
                    // two halves refuse for different reasons and only one of them is silent.
                    game.scroll.add_feedback_to_scroll(
                        dereth_client_model::cmd::NOT_A_VALID_COMMAND,
                        0x1A,
                        true,
                        self.chat.current_command_source,
                        dereth_client_contract::feedback::Feedback::LOCAL,
                    );
                    self.stats.chat_commands_refused += 1;
                }
            }

            // ---- `@friends`, `@friends_add`, `@friends_remove` --------------------------------------
            //
            // Three command names whose handlers live here; without this arm every one of them would
            // fall to the catch-all below and answer *"That is not a valid command."*. This is the
            // whole chat entry point for the friends list.
            //
            // The original three friends-command handlers send no requests directly: each
            // notifies the friends panel, which performs the action. This path instead
            // makes the decision in `dereth_client_model::friends`, the same model the panel draws,
            // rather than calling through a UI panel from the command handler.
            //
            // The friends handler re-splits its own argv (`next_arg` for the sub-command, then
            // `join_args_as_name` for the rest), so the model is handed `args` and not `joined`: joining
            // and re-splitting here would lose a name containing a run of spaces that `join_args_as_name`
            // preserves.
            H::Friends | H::FriendsAdd | H::FriendsRemove => {
                if let Some(outcome) = match handler {
                    H::Friends => Some(game.do_friends(req, args)),
                    H::FriendsAdd => Some(game.do_friends_add(req, args)),
                    H::FriendsRemove => Some(game.do_friends_remove(req, args)),
                    _ => None,
                } {
                    use dereth_client_model::friends::FriendsOutcome as F;
                    match outcome {
                        // The three sending arms. The request is already in `req` -- these only count.
                        F::Added(_) | F::RemovedById(_) | F::Cleared | F::OldList => {
                            self.stats.friends_requests += 1;
                            // The remove-all-friends chat command prints its
                            // acknowledgement *before* any answer from the shard, on chat type 0.
                            if matches!(outcome, F::Cleared) {
                                game.scroll.add_feedback_to_scroll(
                                    dereth_client_model::friends::FRIENDS_LIST_CLEARED,
                                    dereth_client_model::friends::LISTING_CHAT_TYPE,
                                    true,
                                    0,
                                    dereth_client_contract::feedback::Feedback::LOCAL,
                                );
                            }
                        }
                        // A scroll write of type 0 to window `0`, not the command source. That
                        // asymmetry with the refusals below is retail's: the listing is the *panel's*
                        // print and the refusals are the command handler's.
                        F::Listed { lines, .. } => {
                            for line in lines {
                                game.scroll.add_feedback_to_scroll(
                                    &line,
                                    dereth_client_model::friends::LISTING_CHAT_TYPE,
                                    true,
                                    0,
                                    dereth_client_contract::feedback::Feedback::LOCAL,
                                );
                            }
                            self.stats.friends_listings += 1;
                        }
                        F::Refused(text) => {
                            game.scroll.add_feedback_to_scroll(
                                text,
                                dereth_client_model::friends::REFUSAL_CHAT_TYPE,
                                true,
                                self.chat.current_command_source,
                                dereth_client_contract::feedback::Feedback::LOCAL,
                            );
                            self.stats.chat_commands_refused += 1;
                        }
                        // Display the object error with an empty detail string -- the failure-event
                        // handler with an empty detail, the same function a `0x028A` reaches.
                        //
                        // `0x561`'s arm prints on channel `0x1A`, while `0x563` formats its message and
                        // prints on channel 0, as confirmed against retail. The table covers the whole
                        // function, so the refusal prints the sentence retail prints.
                        //
                        // Window **0**, not the command source: every one of the failure-event handler's
                        // 232 scroll writes passes `0` as its last
                        // argument, which is the asymmetry the doc on `apply_chat_command` describes.
                        F::WeenieError(code) => {
                            if code == dereth_client_model::friends::ERROR_FRIENDS_LIST_FULL {
                                self.stats.friends_list_full_refusals += 1;
                            } else {
                                self.stats.friends_not_a_friend_refusals += 1;
                            }
                            if let Some(m) =
                                dereth_client_contract::chat::failure::handle_failure_event(
                                    code, "",
                                )
                            {
                                game.scroll.add_feedback_to_scroll(
                                    crate::chat::add_text_to_scroll_trim(&m.body),
                                    u32::from(m.ty),
                                    true,
                                    0,
                                    m.feedback,
                                );
                            }
                        }
                    }
                }
            }

            // ---- `@allegiance`, `@motd`, `@alh`/`@ah`, `@ab` ---------------------------
            //
            // Four command names whose handlers live here; without this arm every one would fall to
            // the catch-all below, count itself in `chat_commands_unimplemented` and answer *"That is
            // not a valid command."*.
            //
            // These commands are the production path for all twenty-four events. Each
            // original event has one command-handler caller. The allegiance panel sends
            // only `0x001D`, `0x001E` and `0x001F`; none of the twenty-four has a button,
            // context menu or confirmation dialog, so command dispatch is essential.
            //
            // The parse lives in `dereth_client_model::allegiance_cmd` for the same reason `@friends`' does:
            // "which allegiance member does this line name, and is the level legal" is a model
            // decision, and this file's job is to turn the answer into a `Request`, a scroll line and
            // a counter. `args` and not `joined`, because these handlers re-split their own argv
            // (`next_arg`, then `join_args` or `join_args_as_name` for the tail) and a name carrying a run
            // of spaces survives that and would not survive a join-then-resplit here.
            H::Allegiance | H::AllegianceHometown | H::AllegianceBroadcast | H::Motd => {
                if let Some(cmd) = match handler {
                    H::Allegiance => Some(game.do_allegiance(req, args)),
                    H::AllegianceHometown => Some(game.do_allegiance_hometown(req, args)),
                    H::AllegianceBroadcast => Some(game.do_allegiance_broadcast(req, args)),
                    H::Motd => Some(game.do_motd(req, args)),
                    _ => None,
                } {
                    // Write the allegiance-chat listening bit, then tail-call the option-change handler
                    // with ordinal `0x1B`, which is the same path
                    // `UiRequest::SetPlayerOption` takes and therefore the same `0x0005`
                    // player-option change when the ordinal is an `is_auto_save_option` one.
                    // The `@allegiance chat on` path is the only one in this family that sends
                    // something other than an allegiance event.
                    if let Some(on) = cmd.hear_allegiance_chat {
                        let change = game.player_system.set_option(
                            dereth_client_model::allegiance_cmd::HEAR_ALLEGIANCE_CHAT_ORDINAL,
                            on,
                            now,
                        );
                        for (ordinal, value) in &change.sends {
                            dereth_client_model::RequestSink::send(
                                req,
                                Request::PlayerOptionChanged(
                                    dereth_protocol::login::CharacterPlayerOptionChangedEvent {
                                        option: u32::try_from(*ordinal).unwrap_or(u32::MAX),
                                        value: u32::from(*value),
                                    },
                                ),
                            );
                            self.stats.option_changes_sent += 1;
                        }
                        if change.deferred {
                            self.stats.option_changes_deferred += 1;
                        }
                    }
                    // The scroll write to the current command source's window — every print site in
                    // this family passes the command's own window, including `do_allegiance_boot`'s
                    // acknowledgement, which is the one that is **not** a refusal (chat type 0).
                    for (text, ty) in &cmd.lines {
                        game.scroll.add_feedback_to_scroll(
                            text,
                            *ty,
                            true,
                            self.chat.current_command_source,
                            dereth_client_contract::feedback::Feedback::LOCAL,
                        );
                        if *ty == dereth_client_model::allegiance_cmd::REFUSAL_CHAT_TYPE {
                            self.stats.chat_commands_refused += 1;
                        }
                    }
                    self.stats.allegiance_command_requests += cmd.sent;
                    // `do_command`'s failure event `0x26`. The `@allegiance` handler never gets here
                    // (it always returns true and prints its own hint); `@motd wibble`, `@ab` with no text
                    // and `@ah` are the entries that can.
                    if !cmd.handled {
                        game.scroll.add_feedback_to_scroll(
                            dereth_client_model::cmd::NOT_A_VALID_COMMAND,
                            0x1A,
                            true,
                            self.chat.current_command_source,
                            dereth_client_contract::feedback::Feedback::LOCAL,
                        );
                        self.stats.chat_commands_refused += 1;
                    }
                }
            }

            // ---- `@loc` -----------------------------------------------------------------
            //
            // Verified against retail:
            //
            // Extra arguments print the wide "Unexpected arguments to @loc" message on `0x1A`,
            // with logging enabled and the command-source window, then return true. With no arguments,
            // an absent player body returns true silently. Otherwise read the body's cell and frame;
            // cell zero prints the wide "Not in valid cell!" message on `0x1A`. A valid cell is
            // formatted into the narrow "Your location is: %s\n" template (21 characters plus NUL)
            // and printed on type 0, again logged in the command-source window.
            //
            // Position formatting uses `_snprintf(buf, 100, "0x%08X [%f %f %f] %f %f %f %f",
            // cell, x, y, z, qw, qx, qy, qz)` -- `%f` at C's default six decimals, confirmed by
            // the observed retail output. The trailing `\n` of the outer template is
            // the scroll write's to trim (`chat::add_text_to_scroll_trim`), as it is for every line.
            H::Loc => {
                if !args.is_empty() {
                    game.scroll.add_feedback_to_scroll(
                        "Unexpected arguments to @loc",
                        0x1A,
                        true,
                        self.chat.current_command_source,
                        dereth_client_contract::feedback::Feedback::LOCAL,
                    );
                    self.stats.chat_commands_refused += 1;
                    return;
                }
                let Some(p) = self.player_position else {
                    return;
                };
                if p.cell.0 == 0 {
                    game.scroll.add_feedback_to_scroll(
                        if self.chat_interface
                            == dereth_client_contract::options::interface::Interface::Classic
                        {
                            "@Loc: not in valid cell!"
                        } else {
                            "Not in valid cell!"
                        },
                        if self.chat_interface
                            == dereth_client_contract::options::interface::Interface::Classic
                        {
                            0
                        } else {
                            0x1a
                        },
                        true,
                        self.chat.current_command_source,
                        dereth_client_contract::feedback::Feedback::LOCAL,
                    );
                    self.stats.chat_commands_refused += 1;
                    return;
                }
                let line = if self.chat_interface
                    == dereth_client_contract::options::interface::Interface::Classic
                {
                    format!(
                    "Your location is Landblock: {:08x}, X: {:.1}, Y: {:.1}, Z: {:.1}, H: {:.1}",
                    p.cell.0,
                    p.frame.origin.x,
                    p.frame.origin.y,
                    p.frame.origin.z,
                    dereth_animation::frame::get_heading(&p.frame)
                )
                } else {
                    format!("Your location is: {}\n", position_to_string(&p))
                };
                game.scroll.add_feedback_to_scroll(
                    &line,
                    0,
                    true,
                    self.chat.current_command_source,
                    dereth_client_contract::feedback::Feedback::LOCAL,
                );
                self.stats.loc_lines_printed += 1;
            }

            // `@lockui`: any argument prints the specific usage line and returns true.
            // The no-argument path toggles the lock-UI option, whose setter tail-jumps
            // the option-change handler with ordinal `0x33`. `LockUI` is in `is_auto_save_option`, so one changed bit
            // is mirrored into the retained module and leaves immediately as `0x0005`; equality is a
            // complete no-op. App performs the following global-0D UI cascade with the chosen value.
            H::Lockui => {
                if args.is_empty() {
                    let ordinal = dereth_client_model::player::options::option::LOCK_UI;
                    let locked = !game.player_system.options.get(ordinal);
                    self.apply_lock_ui_option(game, req, locked, now);
                    self.pending_ui_layout_commands
                        .push(UiLayoutCommand::SetLockUi(locked));
                } else {
                    game.scroll.add_feedback_to_scroll(
                        "Please use @help lockui for proper usage.",
                        0x1A,
                        true,
                        self.chat.current_command_source,
                        dereth_client_contract::feedback::Feedback::LOCAL,
                    );
                    self.stats.chat_command_lines += 1;
                    self.stats.chat_commands_refused += 1;
                }
            }

            // ---- `@saveui` / `@loadui` and their `#auto` siblings -------------------
            //
            // The handlers validate argument count and the optional name, then raise
            // save/load-layout notices. `App` applies them to the live gameplay screen,
            // matching the original two notice handlers. Refusals still return handled,
            // so they do not append the generic failure-`0x26` line.
            H::Saveui | H::Loadui | H::Saveautoui | H::Loadautoui => {
                let auto = matches!(handler, H::Saveautoui | H::Loadautoui);
                let operation = if matches!(handler, H::Saveui | H::Saveautoui) {
                    "save"
                } else {
                    "load"
                };
                if (auto && !args.is_empty()) || (!auto && args.len() > 1) {
                    let auto_word = if auto { "auto" } else { "" };
                    let line =
                        format!("Please use @help {operation}{auto_word}ui for proper usage.");
                    game.scroll.add_feedback_to_scroll(
                        &line,
                        0x1A,
                        true,
                        self.chat.current_command_source,
                        dereth_client_contract::feedback::Feedback::LOCAL,
                    );
                    self.stats.chat_command_lines += 1;
                    self.stats.chat_commands_refused += 1;
                    return;
                }
                let name = if auto {
                    dereth_client_contract::persist::ScreenLayout::AUTO_NAME.to_owned()
                } else {
                    args.first().cloned().unwrap_or_default()
                };
                // The original narrow-string length includes its trailing NUL and is compared
                // with `0x10`: fifteen visible characters pass and sixteen fail. Parsing maps
                // each CP-1252 source byte to one scalar, so counting UTF-8 bytes here would
                // incorrectly reject valid non-ASCII names.
                if !auto && name.chars().count() >= 16 {
                    game.scroll.add_feedback_to_scroll(
                        "The file name must be 16 characters or less.",
                        0x1A,
                        true,
                        self.chat.current_command_source,
                        dereth_client_contract::feedback::Feedback::LOCAL,
                    );
                    self.stats.chat_command_lines += 1;
                    self.stats.chat_commands_refused += 1;
                    return;
                }
                let command = if operation == "save" {
                    UiLayoutCommand::Save(name)
                } else {
                    UiLayoutCommand::Load(name)
                };
                self.pending_ui_layout_commands.push(command);
            }

            // ---- `@fillcomps` ---------------------------------------------
            //
            // The command raises the same local vendor notice as the Components
            // UI. The existing vendor fill operation owns the shortfall, stock, price-cap and
            // missing-row rules; this arm only preserves the command's argc/category/price grammar.
            H::Fillcomps => {
                if args.len() > 2 {
                    game.scroll.add_feedback_to_scroll(
                        "Please use @help fillcomps for proper usage.",
                        dereth_client_model::chat::text_type::LOCAL_ERROR,
                        true,
                        self.chat.current_command_source,
                        dereth_client_contract::feedback::Feedback::LOCAL,
                    );
                    self.stats.chat_command_lines += 1;
                    self.stats.chat_commands_refused += 1;
                    return;
                }
                if args
                    .first()
                    .is_some_and(|arg| arg.eq_ignore_ascii_case("clear"))
                {
                    game.clear_desired_components(req);
                    game.scroll.add_feedback_to_scroll(
                        "Component list cleared.",
                        dereth_client_model::chat::text_type::LOCAL_ERROR,
                        true,
                        self.chat.current_command_source,
                        dereth_client_contract::feedback::Feedback::LOCAL,
                    );
                    self.stats.desired_comp_sets += 1;
                    self.stats.chat_command_requests += 1;
                    self.stats.chat_command_lines += 1;
                    return;
                }

                let category = args.first().and_then(|arg| spell_component_category(arg));
                let price_word = if category.is_some() {
                    args.get(1).map(String::as_str)
                } else {
                    // Retail keeps parsing the first argument when the category mapper misses. A
                    // permitted second argument is ignored in this branch, whether the first is a
                    // price or an invalid word.
                    args.first().map(String::as_str)
                };
                let max_price = match price_word.and_then(|word| word.parse::<i32>().ok()) {
                    Some(price) if price < 1 => {
                        game.scroll.add_feedback_to_scroll(
                            "Please specify a value greater than 0.",
                            dereth_client_model::chat::text_type::LOCAL_ERROR,
                            true,
                            self.chat.current_command_source,
                            dereth_client_contract::feedback::Feedback::LOCAL,
                        );
                        self.stats.chat_command_lines += 1;
                        self.stats.chat_commands_refused += 1;
                        return;
                    }
                    Some(price) => price,
                    None if category.is_some() || args.is_empty() => 0,
                    None => {
                        game.scroll.add_feedback_to_scroll(
                            "Invalid component type specified.",
                            dereth_client_model::chat::text_type::LOCAL_ERROR,
                            true,
                            self.chat.current_command_source,
                            dereth_client_contract::feedback::Feedback::LOCAL,
                        );
                        self.stats.chat_command_lines += 1;
                        self.stats.chat_commands_refused += 1;
                        return;
                    }
                };
                let vendor = game.shop.vendor_id;
                let result = game.fill_component_list(category, max_price, out);
                self.stats.fill_components_rows += u64::try_from(result.added).unwrap_or(0);
                self.stats.fill_components_missing +=
                    u64::try_from(result.not_stocked + result.short_stocked).unwrap_or(0);
                if vendor.is_some() {
                    self.vendor_buying_tab_requested = vendor;
                }
            }

            // ---- `@squelch` / `@unsquelch` -----------------------------------------
            //
            // The two handlers are identical after choosing the
            // add flag. With no argv they run the local squelch query; otherwise
            // `process_squelch_args` chooses either the character event (`0x0058`, object 0,
            // plus `LogTextType`) or account event (`0x0059`, no object/type fields).
            H::Squelch | H::Unsquelch => {
                if args.is_empty() {
                    let line = squelch_query(&game.chat);
                    game.scroll.add_feedback_to_scroll(
                        &line,
                        dereth_client_model::chat::text_type::DEFAULT,
                        true,
                        0,
                        dereth_client_contract::feedback::Feedback::LOCAL,
                    );
                    self.stats.chat_command_lines += 1;
                    return;
                }
                let parsed = match process_squelch_args(args, &game.chat.last_teller_name, true) {
                    Ok(parsed) => parsed,
                    Err(line) => {
                        game.scroll.add_feedback_to_scroll(
                            &line,
                            dereth_client_model::chat::text_type::LOCAL_ERROR,
                            true,
                            self.chat.current_command_source,
                            dereth_client_contract::feedback::Feedback::LOCAL,
                        );
                        self.stats.chat_command_lines += 1;
                        self.stats.chat_commands_refused += 1;
                        return;
                    }
                };
                if let Some(line) = parsed.warning {
                    game.scroll.add_feedback_to_scroll(
                        line,
                        dereth_client_model::chat::text_type::LOCAL_ERROR,
                        true,
                        self.chat.current_command_source,
                        dereth_client_contract::feedback::Feedback::LOCAL,
                    );
                    self.stats.chat_command_lines += 1;
                    self.stats.chat_commands_refused += 1;
                }
                let add = handler == H::Squelch;
                if parsed.account {
                    game.modify_account_squelch(req, add, &parsed.name);
                } else {
                    game.modify_character_squelch(
                        req,
                        ObjectId(0),
                        add,
                        &parsed.name,
                        parsed.message_type,
                    );
                }
                self.stats.squelch_requests += 1;
                self.stats.chat_command_requests += 1;
            }

            // ---- `@filter` / `@unfilter` -------------------------------------------
            //
            // The global squelch modifier reuses `process_squelch_args`, but with
            // `require_target = false`, then accepts only a dash-prefixed message type with neither
            // account nor target. The client sends `0x005B` and does not predict the global table;
            // the authoritative `0x01F4` replacement remains the only local-state writer.
            H::Filter | H::Unfilter => {
                if args.is_empty() {
                    let line = global_squelch_query(&game.chat);
                    game.scroll.add_feedback_to_scroll(
                        &line,
                        dereth_client_model::chat::text_type::DEFAULT,
                        true,
                        0,
                        dereth_client_contract::feedback::Feedback::LOCAL,
                    );
                    self.stats.chat_command_lines += 1;
                    return;
                }
                if !args.first().is_some_and(|arg| arg.starts_with('-')) {
                    game.scroll.add_feedback_to_scroll(
                        "You must specify a valid message type prefixed by a dash.",
                        dereth_client_model::chat::text_type::LOCAL_ERROR,
                        true,
                        self.chat.current_command_source,
                        dereth_client_contract::feedback::Feedback::LOCAL,
                    );
                    self.stats.chat_command_lines += 1;
                    self.stats.chat_commands_refused += 1;
                    return;
                }
                let parsed = match process_squelch_args(args, &game.chat.last_teller_name, false) {
                    Ok(parsed) => parsed,
                    Err(line) => {
                        game.scroll.add_feedback_to_scroll(
                            &line,
                            dereth_client_model::chat::text_type::LOCAL_ERROR,
                            true,
                            self.chat.current_command_source,
                            dereth_client_contract::feedback::Feedback::LOCAL,
                        );
                        self.stats.chat_command_lines += 1;
                        self.stats.chat_commands_refused += 1;
                        return;
                    }
                };
                if let Some(line) = parsed.warning {
                    game.scroll.add_feedback_to_scroll(
                        line,
                        dereth_client_model::chat::text_type::LOCAL_ERROR,
                        true,
                        self.chat.current_command_source,
                        dereth_client_contract::feedback::Feedback::LOCAL,
                    );
                    self.stats.chat_command_lines += 1;
                    self.stats.chat_commands_refused += 1;
                }
                if parsed.account || !parsed.name.is_empty() {
                    game.scroll.add_feedback_to_scroll(
                        "Incorrect usage, use @help for proper arguements.",
                        dereth_client_model::chat::text_type::LOCAL_ERROR,
                        true,
                        self.chat.current_command_source,
                        dereth_client_contract::feedback::Feedback::LOCAL,
                    );
                    self.stats.chat_command_lines += 1;
                    self.stats.chat_commands_refused += 1;
                    return;
                }
                dereth_client_model::RequestSink::send(
                    req,
                    Request::ModifyGlobalSquelch(
                        dereth_protocol::comms::CommunicationModifyGlobalSquelch {
                            add: i32::from(handler == H::Filter),
                            msg_type: parsed.message_type,
                        },
                    ),
                );
                self.stats.chat_command_requests += 1;
            }

            // ---- `@corpse` / `@cor` -----------------------------------------------
            //
            // The corpse-location command reads the local player's position quality `0x0E`
            // and prints one type-zero line without sending a request. Unlike the location
            // command, it ignores all supplied arguments.
            //
            // The original missing-description return is unhandled; this router directly
            // emits the same ordinary failure line. An available description lacking
            // `0x0E` instead produces a handled apology.
            H::Corpse => {
                let Some(qualities) = game.player_qualities() else {
                    game.scroll.add_feedback_to_scroll(
                        dereth_client_model::cmd::NOT_A_VALID_COMMAND,
                        0x1A,
                        true,
                        self.chat.current_command_source,
                        dereth_client_contract::feedback::Feedback::LOCAL,
                    );
                    self.stats.chat_commands_refused += 1;
                    return;
                };
                let coordinate = qualities
                    .positions
                    .as_ref()
                    .and_then(|positions| positions.get(&0x0E))
                    .and_then(|position| corpse_coordinate_string(position.objcell_id));
                let line = coordinate.map_or_else(
                || {
                    "We're sorry, but we have no record of your last outside corpse location.\n"
                        .to_owned()
                },
                |coordinate| {
                    format!(
                        "The last time you died outside, your corpse was located at ({coordinate}).\n"
                    )
                },
            );
                game.scroll.add_feedback_to_scroll(
                    &line,
                    0,
                    true,
                    self.chat.current_command_source,
                    dereth_client_contract::feedback::Feedback::LOCAL,
                );
                self.stats.chat_command_lines += 1;
            }

            // **The `help` handler: `@help` and `@?`.**
            //
            // The command table's very first two entries are `?` and `help`, both the `help`
            // handler; every string they need is in `dereth_client_model::cmd::table::HELP_TEXTS`.
            //
            // The handler is a pure function of the command table, so it lives in the command
            // interpreter beside that table rather than in `dereth_client_model::chat_cmd`: nothing
            // about it is a model question and it sends nothing at all. What this file owns is the
            // window — every line goes to
            // the current command source, which is `self.chat.current_command_source` — and the
            // counters.
            //
            // The handler returns **true** on every path, including the `"Unknown command"` one, so
            // `do_command`'s failure event `0x26` never fires for it.
            H::Help => {
                for (text, ty) in self.chat.do_help(args) {
                    game.scroll.add_feedback_to_scroll(
                        &text,
                        ty,
                        true,
                        self.chat.current_command_source,
                        dereth_client_contract::feedback::Feedback::LOCAL,
                    );
                    self.stats.chat_command_lines += 1;
                    if ty == dereth_client_model::chat_cmd::REFUSAL_CHAT_TYPE {
                        self.stats.chat_commands_refused += 1;
                    }
                }
            }

            // ---- `@join` / `@leave` ------------------------------------------------
            //
            // Both handlers call `next_arg` once, compare that
            // one token case-insensitively against six native channel names, and ignore the tail.
            // A recognized token calls the matching chat-option handler; all six options are
            // auto-saved, so a moved bit leaves as `0x0005 Character_PlayerOptionChangedEvent`.
            // Missing or unknown tokens return FALSE and let `do_command` print its ordinary failure.
            H::Join | H::Leave => {
                let Some(ordinal) = join_leave_channel_option(args.first().map(String::as_str))
                else {
                    game.scroll.add_feedback_to_scroll(
                        dereth_client_model::cmd::NOT_A_VALID_COMMAND,
                        0x1A,
                        true,
                        self.chat.current_command_source,
                        dereth_client_contract::feedback::Feedback::LOCAL,
                    );
                    self.stats.chat_commands_refused += 1;
                    return;
                };
                let change = game
                    .player_system
                    .set_option(ordinal, handler == H::Join, now);
                // The retail option-changed notice immediately recomputes the five
                // Turbine talk-focus rows. Allegiance (27) is instead driven by its room tracker.
                if change.moved() && matches!(ordinal, 35 | 36 | 37 | 38 | 46) {
                    let heritage = Self::player_desc(game, player_desc_received)
                        .map_or(0, |q| q.inq_int(0xBC));
                    game.enable_chat_talk_focuses(crate::chat::is_olthoi(heritage));
                    chat_focus(&mut game.chat);
                }
                for (ordinal, value) in &change.sends {
                    dereth_client_model::RequestSink::send(
                        req,
                        Request::PlayerOptionChanged(
                            dereth_protocol::login::CharacterPlayerOptionChangedEvent {
                                option: u32::try_from(*ordinal).unwrap_or(u32::MAX),
                                value: u32::from(*value),
                            },
                        ),
                    );
                    self.stats.option_changes_sent += 1;
                }
                if change.deferred {
                    self.stats.option_changes_deferred += 1;
                }
            }

            // ---- the character, comms, consent and local families ----------------------
            //
            // Twenty-four more command names whose handlers live here; without this arm every one
            // would fall to the catch-all below, count itself in `chat_commands_unimplemented`
            // and answer *"That is not a valid command."*.
            //
            // **The chat entry is the whole production path for all of them.** Retail gives
            // each teleport/query/consent-list/player-permission event,
            // each AFK/channel/global-squelch event and both house-teleport events
            // exactly one caller in retail, a chat-command handler.
            // There is no button for any of them.
            //
            // The parse and the literals live in `dereth_client_model::chat_cmd` for the reason `@friends`' and
            // `@allegiance`'s do: "is this player a PK", "is the Afk quality set" and "does this word
            // name a channel" are model questions, and this file's job is to turn the answer into a
            // scroll line, a counter, an option write or a dialog. `args` and not `joined`, because
            // the three shape-C handlers re-split their own argv (`next_arg`, then `join_args` or
            // `join_args_as_name`) and a name carrying a run of spaces survives that.
            H::Lifestone
            | H::Marketplace
            | H::HouseRecall
            | H::MansionRecall
            | H::Pkarena
            | H::Pklarena
            | H::Pklite
            | H::Age
            | H::Birth
            | H::Die
            | H::Chat
            | H::Notell
            | H::Index
            | H::Clist
            | H::On
            | H::Off
            | H::Afk
            | H::Consent
            | H::Permit
            | H::Speaker
            | H::Endurance
            | H::Emotes
            | H::House
            | H::Hslist => {
                if let Some(cmd) = match handler {
                    H::Lifestone => Some(game.do_lifestone(req, args)),
                    H::Marketplace => Some(game.do_marketplace(req, args)),
                    H::HouseRecall => Some(game.do_house_recall(req, args)),
                    H::MansionRecall => Some(game.do_mansion_recall(req, args)),
                    H::Pkarena => Some(game.do_pk_arena(req, args)),
                    H::Pklarena => Some(game.do_pkl_arena(req, args)),
                    H::Pklite => Some(game.do_pk_lite(req, args)),
                    H::Age => Some(game.do_age(req, args)),
                    H::Birth => Some(game.do_birth(req, args)),
                    H::Die => Some(game.do_die(args)),
                    H::Chat => Some(game.do_chat_toggle(req, args)),
                    H::Notell => Some(game.do_no_tell(req, args)),
                    H::Index => Some(game.do_channel_index(req, args)),
                    H::Clist => Some(game.do_channel_list(req, args)),
                    H::On => Some(game.do_channel_on(req, args)),
                    H::Off => Some(game.do_channel_off(req, args)),
                    H::Afk => Some(game.do_afk(req, args)),
                    H::Consent => Some(game.do_consent(req, args)),
                    H::Permit => Some(game.do_permit(req, args)),
                    H::Speaker => Some(game.do_speaker(args)),
                    H::Endurance => Some(game.do_endurance(args)),
                    H::Emotes => Some(game.do_emote_list(args)),
                    // ---- `@house`/`@hou` and `@hslist` -----------------------------------
                    //
                    // The `house` handler is handed the whole of `argv`; its own `next_arg` takes the
                    // sub-command and each sub-handler `next_arg`s again, so `args` and not `joined` for
                    // the reason the three shape-C handlers above want it. `@house guest add Baron Lark`
                    // has to reach `join_args_as_name` with its interior spacing intact.
                    H::House => Some(game.do_house(req, args)),
                    H::Hslist => Some(game.do_house_available_list(req, args)),
                    _ => None,
                } {
                    self.apply_chat_command(cmd, game, req, out, now);
                }
            }

            // The `say` handler: `@say` and `@s`.
            //
            // Join arguments and trim whitespace on both sides (the emote command does not trim).
            // An empty result prints "You must specify the text you wish to say!" on `0x1A` in
            // the command-source window; a nonempty result goes through public-chat processing.
            // Both paths return true.
            //
            // Two adjacent handlers, two different answers to an empty line: the `say` handler
            // refuses and the `emote` handler silently returns true. Both are transcribed rather than harmonised.
            H::Say => {
                let text = joined.trim_matches(dereth_client_model::chat::WHITESPACE);
                if text.is_empty() {
                    game.scroll.add_feedback_to_scroll(
                        dereth_client_model::chat_cmd::YOU_MUST_SPECIFY_TEXT_TO_SAY,
                        0x1A,
                        true,
                        self.chat.current_command_source,
                        dereth_client_contract::feedback::Feedback::LOCAL,
                    );
                    self.stats.chat_commands_refused += 1;
                    return;
                }
                let text = text.to_owned();
                self.public_chat(&text, game, req);
            }

            H::Guild | H::General | H::Trade | H::Lfg | H::Roleplay | H::Society | H::Olthoi => {
                let turbine_focus = match handler {
                    H::Guild => Some(dereth_client_model::chat::TalkFocus::Allegiance),
                    H::General => Some(dereth_client_model::chat::TalkFocus::General),
                    H::Trade => Some(dereth_client_model::chat::TalkFocus::Trade),
                    H::Lfg => Some(dereth_client_model::chat::TalkFocus::Lfg),
                    H::Roleplay => Some(dereth_client_model::chat::TalkFocus::Roleplay),
                    H::Society => Some(dereth_client_model::chat::TalkFocus::Society),
                    H::Olthoi => Some(dereth_client_model::chat::TalkFocus::Olthoi),
                    _ => None,
                };
                if let Some(focus) = turbine_focus {
                    // Each explicit handler refuses zero arguments itself; command dispatch also reports
                    // error `0x26` when the handler returns false. No fallback public-speech packet.
                    if args.is_empty() {
                        game.scroll.add_feedback_to_scroll(
                            dereth_client_model::cmd::NOT_A_VALID_COMMAND,
                            0x1A,
                            true,
                            0,
                            dereth_client_contract::feedback::Feedback::LOCAL,
                        );
                    }
                    // The allegiance-channel handler (`@a`) is the **only** one of the seven
                    // that tests its room before speaking. After the `argc` guard above it reads the
                    // tracker and leaves:
                    //
                    // Retail reads the allegiance room from the tracker and continues to
                    // chat sending only if it is nonzero. Otherwise it reports error `0x414` with an
                    // empty detail string ("You are not in an allegiance!") and return false, causing
                    // command dispatch to add error `0x26` below.
                    //
                    // `send_turbine_chat`'s *"Turbine chat is not available."* — which has exactly one
                    // native string referrer — is
                    // therefore **unreachable from `/a`**; printing it would point the diagnosis at the
                    // chat provider when the missing thing is the room id. The six sibling commands
                    // have no such test and keep `send_turbine_chat`'s sentence.
                    if focus == dereth_client_model::chat::TalkFocus::Allegiance
                        && !args.is_empty()
                        && game.chat.chat_rooms.get(&1).copied().unwrap_or(0) == 0
                    {
                        if let Some(m) = dereth_client_contract::chat::failure::handle_failure_event(
                            dereth_client_contract::chat::failure::YOU_ARE_NOT_IN_ALLEGIANCE,
                            "",
                        ) {
                            // A scroll write of type `0x1a` to window 0, not the command
                            // source, exactly as the failure-event handler's own arm passes it.
                            game.scroll.add_feedback_to_scroll(
                                &m.body,
                                u32::from(m.ty),
                                true,
                                0,
                                m.feedback,
                            );
                        }
                        game.scroll.add_feedback_to_scroll(
                            dereth_client_model::cmd::NOT_A_VALID_COMMAND,
                            0x1A,
                            true,
                            0,
                            dereth_client_contract::feedback::Feedback::LOCAL,
                        );
                        self.stats.chat_commands_refused += 1;
                        return;
                    }
                    if args.is_empty()
                        || !game.send_turbine_chat(
                            req,
                            focus,
                            true,
                            &joined,
                            self.chat.current_command_source,
                            chat_real_time(),
                        )
                    {
                        game.scroll.add_feedback_to_scroll(
                            dereth_client_model::cmd::NOT_A_VALID_COMMAND,
                            0x1A,
                            true,
                            0,
                            dereth_client_contract::feedback::Feedback::LOCAL,
                        );
                        self.stats.chat_commands_refused += 1;
                    }
                }
            }

            H::Tell | H::Reply | H::Retell => {
                let outcome = match handler {
                    H::Tell => game.chat.do_tell(&joined),
                    H::Reply => game.chat.do_reply(&joined),
                    H::Retell => game.chat.do_retell(&joined),
                    // No handler the table names reaches this arm; see `chat_commands_unimplemented`.
                    _ => {
                        self.stats.chat_commands_unimplemented += 1;
                        T::Refused(dereth_client_model::cmd::NOT_A_VALID_COMMAND)
                    }
                };
                match outcome {
                    T::ByName {
                        message,
                        target_name,
                    } => {
                        dereth_client_model::RequestSink::send(
                            req,
                            Request::TalkDirectByName(
                                dereth_protocol::comms::CommunicationTalkDirectByName {
                                    message,
                                    target_name,
                                },
                            ),
                        );
                        self.stats.tells_sent += 1;
                    }
                    T::ById { message, target } => {
                        dereth_client_model::RequestSink::send(
                            req,
                            Request::TalkDirect(dereth_protocol::comms::CommunicationTalkDirect {
                                message,
                                target,
                            }),
                        );
                        self.stats.tells_sent += 1;
                    }
                    T::Refused(text) => {
                        dereth_client_model::NoticeSink::emit(
                            out,
                            Notice::DisplayString {
                                feedback: dereth_client_contract::feedback::Feedback::LOCAL,
                                channel: dereth_client_model::chat::REFUSAL_CHANNEL,
                                text: text.to_owned(),
                            },
                        );
                        self.stats.chat_commands_refused += 1;
                    }
                    T::Nothing => self.stats.tells_dropped_silently += 1,
                }
            }
        }
    }
}

const SQUELCH_QUERY_HEADER: &str =
    "(account) denotes a character whose account has also been squelched.\n\
Format: Name : List of squelched message types.\n\
--------\n";
const SQUELCH_REPLY_WITHOUT_TELLER: &str =
    "A player must @tell you before you can squelch them with this command.";
const SQUELCH_TARGET_MISSING: &str = "You have not specified a squelch target.";

#[derive(Debug, PartialEq, Eq)]
struct SquelchCommandArgs {
    account: bool,
    name: String,
    message_type: u32,
    warning: Option<&'static str>,
}

fn squelch_text_type(word: &str) -> Option<u32> {
    if word.eq_ignore_ascii_case("Assessment") {
        return Some(dereth_client_model::chat::text_type::APPRAISAL);
    }
    (0..u32::try_from(dereth_client_model::chat::text_type::TABLE_LEN)
        .expect("small text-type table"))
        .find(|&ty| {
            let name = dereth_client_model::chat::log_text_type_name(ty);
            name != "Unknown" && name.eq_ignore_ascii_case(word)
        })
}

/// `process_squelch_args`. Its odd success return on the two missing-target paths is
/// intentional: both callers print the warning and still send an empty-name `0x0058`/`0x0059`.
fn process_squelch_args(
    args: &[String],
    last_teller_name: &str,
    require_target: bool,
) -> Result<SquelchCommandArgs, String> {
    let mut account = false;
    let mut message_type = dereth_client_model::chat::text_type::ALL_CHANNELS;
    let mut name = String::new();
    let mut reply = false;
    let mut index = 0;
    while let Some(flag) = args.get(index).and_then(|arg| arg.strip_prefix('-')) {
        index += 1;
        if flag.eq_ignore_ascii_case("reply") {
            reply = true;
        } else if flag.eq_ignore_ascii_case("account") {
            account = true;
        } else if let Some(ty) = squelch_text_type(flag) {
            message_type = ty;
        } else {
            return Err(format!("\"{flag}\" is not a valid squelch category."));
        }
    }
    if reply {
        name = last_teller_name.to_owned();
        if name.is_empty() {
            return Ok(SquelchCommandArgs {
                account,
                name,
                message_type,
                warning: Some(SQUELCH_REPLY_WITHOUT_TELLER),
            });
        }
    } else if index < args.len() {
        let joined = dereth_client_model::cmd::interp::join_args(&args[index..]);
        name = dereth_client_model::chat::join_args_as_name(&joined).to_owned();
    }
    let warning = (require_target && name.is_empty()).then_some(SQUELCH_TARGET_MISSING);
    Ok(SquelchCommandArgs {
        account,
        name,
        message_type,
        warning,
    })
}

/// Unlike the character query this
/// formats one `SquelchInfo`, so there is no hash traversal or row-order adaptation.
fn global_squelch_query(chat: &dereth_client_model::chat::ChatState) -> String {
    let mut text =
        "The following types of messages are currently being filtered globally:\n".to_owned();
    let entry = &chat.squelch.global;
    if entry.is_empty() {
        text.push_str("none");
    } else if entry.is_squelched(dereth_client_model::chat::text_type::ALL_CHANNELS) {
        text.push_str("All message types");
    } else {
        let mut first = true;
        for ty in dereth_client_model::chat::LEGAL_CHANNELS {
            if entry.is_squelched(ty) {
                if !first {
                    text.push_str(", ");
                }
                text.push_str(dereth_client_model::chat::log_text_type_name(ty));
                first = false;
            }
        }
    }
    text.push_str("\n(For a list of filter options, type @help filter)\n");
    text
}

/// List the legal squelch channels.
///
/// The original path creates an empty-name squelch record with message type 1,
/// setting all 128 bits, then enumerates only legal channels from 0 through `0x21`.
/// This implementation shares the legality and enum-name table with the parser so
/// the list cannot drift from squelch/filter parsing.
fn message_types_text() -> String {
    let channels = dereth_client_model::chat::LEGAL_CHANNELS
        .iter()
        .map(|&ty| dereth_client_model::chat::log_text_type_name(ty))
        .collect::<Vec<_>>()
        .join(", ");
    format!("Squelch channels are as follows:\n  {channels}\n")
}

/// The decimal-prefix behavior obtains from `atoi`.
/// Extreme overflow is not pinned here; saturation keeps it outside either accepted render range.
fn render_atoi(s: &str) -> i32 {
    let s = s.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let (negative, digits) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let mut saw_digit = false;
    let mut value = 0_i64;
    for b in digits.bytes() {
        if !b.is_ascii_digit() {
            break;
        }
        saw_digit = true;
        value = value.saturating_mul(10).saturating_add(i64::from(b - b'0'));
    }
    if !saw_digit {
        return 0;
    }
    let clamped = if negative {
        value.saturating_neg()
    } else {
        value
    }
    .clamp(i64::from(i32::MIN), i64::from(i32::MAX));
    i32::try_from(clamped).expect("clamped to the i32 range")
}

/// Parse a spell-component category name.
///
/// The command's names are an old narrow enum mapper, not the display captions from the component
/// panel. In particular there are no spaces in `PowderedGem` or `AlchemicalSubstance`, while the
/// two shorter synonyms are `Powder` and `Potion`.
fn spell_component_category(word: &str) -> Option<u32> {
    use dereth_client_model::magic::component_category as c;
    [
        ("Scarab", c::SCARAB),
        ("Scarabs", c::SCARAB),
        ("Herb", c::HERB),
        ("Herbs", c::HERB),
        ("PowderedGem", c::POWDERED_GEM),
        ("PowderedGems", c::POWDERED_GEM),
        ("Powder", c::POWDERED_GEM),
        ("Powders", c::POWDERED_GEM),
        ("AlchemicalSubstance", c::ALCHEMICAL_SUBSTANCE),
        ("AlchemicalSubstances", c::ALCHEMICAL_SUBSTANCE),
        ("Potion", c::ALCHEMICAL_SUBSTANCE),
        ("Potions", c::ALCHEMICAL_SUBSTANCE),
        ("Talisman", c::TALISMAN),
        ("Talismans", c::TALISMAN),
        ("Taper", c::TAPER),
        ("Tapers", c::TAPER),
        ("Pea", c::PEA),
        ("Peas", c::PEA),
    ]
    .into_iter()
    .find_map(|(name, category)| word.eq_ignore_ascii_case(name).then_some(category))
}

/// Format a squelch query using the same retained character hash
/// that feeds the squelch panel. Account-only hash entries are deliberately absent: native walks
/// only the character table, whose per-entry flag supplies the `(account)` marker.
fn squelch_query(chat: &dereth_client_model::chat::ChatState) -> String {
    let mut text = SQUELCH_QUERY_HEADER.to_owned();
    if chat.squelch.characters.is_empty() {
        text.push_str("none\n");
        return text;
    }
    text.push('\n');
    for entry in chat.squelch.characters.values() {
        text.push_str("  ");
        if !entry.name.is_empty() {
            text.push_str("Name: ");
            text.push_str(&entry.name);
            if entry.is_zone_squelch != 0 {
                text.push_str(" (account) ");
            }
            text.push(' ');
        }
        if entry.is_squelched(dereth_client_model::chat::text_type::ALL_CHANNELS) {
            text.push_str("All message types");
        } else {
            let mut first = true;
            for ty in dereth_client_model::chat::LEGAL_CHANNELS {
                if entry.is_squelched(ty) {
                    if !first {
                        text.push_str(", ");
                    }
                    text.push_str(dereth_client_model::chat::log_text_type_name(ty));
                    first = false;
                }
            }
        }
        text.push('\n');
    }
    text
}
