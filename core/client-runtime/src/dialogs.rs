//! The questions the game asks the player, whatever UI shows them: which ones open, the
//! one-at-a-time slots, what an answer does and what the server's withdrawal does.
//!
//! The game's questions -- a use that needs a yes, a server's confirmation request, an invitation,
//! a swear, a house payment, `@die`, the vendor's unfinished basket, a pop-up message -- arrive as
//! queues on [`crate::interaction::Interaction`]. [`DialogService::service`](crate::dialogs::DialogService::service) turns them into
//! [`Question`](crate::dialogs::Question)s, runs each answer's rule when the player answers, and answers **No** on the way
//! out when the server withdraws a question it asked. What a question looks like, and how a UI
//! queues several of them, is the front end's: it implements [`DialogPresenter`](crate::dialogs::DialogPresenter).
//!
//! The order of [`DialogService::service`](crate::dialogs::DialogService::service) is the client's own, step for step, and it is one
//! function so that every front end gets it: questions that arrive while no world screen is up are
//! dropped (all but the pop-up strings, which wait); a new framework drops every open question
//! without answering it; pop-ups are shown before answers are read; answers run before the
//! server's withdrawals; and new questions open after both, so a question an answer raises (the
//! second `@house abandon` question) opens in the same pass.

use dereth_client_contract::view::AllegianceAction;
use dereth_client_contract::UiRequest;
use dereth_client_model::inventory::targeted_use::TargetedUsageConfirmation;
use dereth_primitives::{LocalTime, ObjectId, ServerTime};

use crate::interaction::Interaction;

/// A question's id, unique for the life of the service.
pub type QuestionId = u64;

/// What a question says. Most carry their sentence already composed; the ones whose wording is a
/// string-table row the UI resolves carry what goes into it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prompt {
    /// The sentence itself.
    Text(String),
    /// A fellowship invitation from `name`.
    FellowshipRequest { name: String },
    /// `name` asks to swear allegiance to the player.
    AcceptSwear { name: String },
    /// Buy the house the player is at.
    HouseBuy,
    /// Buy the house the player is at, asked in the world's own words.
    HouseBuyAs(String),
    /// Pay the house's rent on the owner's behalf.
    HouseRentByProxy,
}

/// A question as a UI shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub id: QuestionId,
    pub prompt: Prompt,
    /// A message with one button rather than a Yes/No question.
    pub message: bool,
    /// Blocks clicks elsewhere and comes to the front (the server's confirmation requests).
    pub modal: bool,
    /// Shown at once beside any other, rather than waiting its turn behind the open one.
    pub all_at_once: bool,
    /// The object the question is about, and the one it would be used on, where it has them.
    pub subject: Option<ObjectId>,
    pub target: Option<ObjectId>,
}

/// What a UI's box says about a question this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    /// Not answered yet, or not on screen yet.
    Waiting,
    /// Closed: `Some(yes)` answered, `None` gone without an answer.
    Closed(Option<bool>),
    /// The front end no longer has it, and has nothing left to take down.
    Gone,
}

/// What the service asks of the front end that shows its questions.
pub trait DialogPresenter {
    /// Whether the front end can show questions now (the world screen is up). Questions that
    /// arrive while it cannot are dropped, except pop-up messages, which wait.
    fn accepting(&self) -> bool;
    /// Which framework is up: a change drops every open question without an answer.
    fn generation(&self) -> u64;
    /// Show `question`. `false` when the box could not be made.
    fn open(&mut self, question: &Question) -> bool;
    /// Whether `id` has been answered. A question the front end no longer knows is
    /// `Closed(None)`.
    fn poll(&mut self, id: QuestionId) -> Answer;
    /// Take the box for `id` down, after its answer has been acted on.
    fn close(&mut self, id: QuestionId);
    /// Hand an answer to the panel that owns the question (the house and mini-game panels).
    fn deliver(&mut self, request: UiRequest);
    /// A new framework is up: forget every box shown in the old one.
    fn reset(&mut self) {}
    /// Bring the boxes up to date once the pass is done.
    fn refresh(&mut self) {}
}

/// What an answer does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rule {
    /// Altar and volatile-rare uses: a yes performs the use.
    Usage(ObjectId),
    /// Mana stone and salvage: a yes performs the targeted use.
    TargetedUse(ObjectId, ObjectId),
    /// The vendor's unfinished-basket question: a yes closes the vendor.
    CloseVendor,
    /// `@house abandon`'s first question: a yes raises the second and sends nothing.
    HouseAbandonFirst,
    /// The second: a yes abandons the house.
    HouseAbandonSecond,
    /// The server's own confirmation request, answered either way.
    ServerConfirmation {
        confirmation_type: i32,
        context_id: u32,
    },
    /// The allegiance panel's swear, break and kick: a yes acts.
    Allegiance {
        action: AllegianceAction,
        target: ObjectId,
    },
    /// A patron asked to accept a swear: answered either way.
    AcceptSwear { context_id: u32 },
    /// A fellowship invitation: answered either way.
    FellowshipRequest { context_id: u32 },
    /// `@die`: a yes dies.
    Die,
    /// The mini-game's resign question: answered either way to the panel.
    MiniGameQuit,
    /// The house panel's buy or rent question: answered either way to the panel.
    HousePayment { rent: bool },
    /// A pop-up message: nothing to answer.
    PopUp,
}

/// The questions the game has asked and the player has not answered.
#[derive(Debug, Default)]
pub struct DialogService {
    next: QuestionId,
    generation: Option<u64>,
    open: Vec<(QuestionId, Rule)>,
}

impl DialogService {
    /// Drop every open question without answering it: the front end has nowhere to show them.
    /// The mini-game's resign question is withdrawn from the game, so a new one can be asked.
    pub fn drop_all(&mut self, world: &mut dereth_client_model::World) {
        if self.open.iter().any(|(_, r)| *r == Rule::MiniGameQuit) {
            world.minigame.cancel_resign_dialog();
        }
        self.open.clear();
        self.generation = None;
    }

    fn ask(
        &mut self,
        ui: &mut dyn DialogPresenter,
        rule: Rule,
        prompt: Prompt,
        shape: (bool, bool, bool),
        objects: (Option<ObjectId>, Option<ObjectId>),
    ) -> bool {
        self.next += 1;
        let q = Question {
            id: self.next,
            prompt,
            message: shape.0,
            modal: shape.1,
            all_at_once: shape.2,
            subject: objects.0,
            target: objects.1,
        };
        if ui.open(&q) {
            self.open.push((q.id, rule));
            true
        } else {
            false
        }
    }

    fn has(&self, f: impl Fn(&Rule) -> bool) -> bool {
        self.open.iter().any(|(_, r)| f(r))
    }

    /// One pass: take what the game asked, read the answers, act on them and on the server's
    /// withdrawals, and open the new questions. Retail runs it several times a frame, at each
    /// point its own UI delivers input; every pass is complete on its own.
    #[allow(clippy::too_many_lines)]
    pub fn service(
        &mut self,
        ui: Option<&mut dyn DialogPresenter>,
        interaction: &mut Interaction,
        world: &mut dereth_client_model::World,
        now: LocalTime,
    ) {
        let usage = interaction.take_usage_confirmations();
        let notices = interaction.take_targeted_confirmations();
        let closes = interaction.take_vendor_close_confirmations();
        let house_payments = interaction.take_house_payment_confirmations();
        let requests = interaction.take_server_confirmations();
        let aborts = interaction.take_confirmation_aborts();
        let fellowship = interaction.take_fellowship_requests();
        let Some(ui) = ui else {
            // No subscriber, and no retained notice history.
            self.drop_all(world);
            return;
        };
        if self.generation != Some(ui.generation()) {
            // The flow already took the boxes down. An unanswered question has no answer; do not
            // manufacture a No or replay a stale Yes into the new framework.
            if self.has(|r| *r == Rule::MiniGameQuit) {
                world.minigame.cancel_resign_dialog();
            }
            self.open.clear();
            ui.reset();
            self.generation = Some(ui.generation());
        }
        if !ui.accepting() {
            return;
        }
        // **The pop-up strings**, which every log-in opens: one-button boxes, all on screen at
        // once, with nothing to answer. Taken only once the world screen is up, so one that
        // arrives earlier waits rather than being dropped.
        for text in interaction.take_pop_up_strings() {
            if self.ask(
                &mut *ui,
                Rule::PopUp,
                Prompt::Text(text),
                (true, false, true),
                (None, None),
            ) {
                interaction.stats.pop_ups_shown += 1;
            }
        }
        for (id, rule) in self.open.clone() {
            if rule != Rule::PopUp {
                continue;
            }
            match ui.poll(id) {
                Answer::Waiting => {}
                Answer::Gone => self.open.retain(|(q, _)| *q != id),
                Answer::Closed(_) => {
                    ui.close(id);
                    self.open.retain(|(q, _)| *q != id);
                    interaction.stats.pop_ups_dismissed += 1;
                }
            }
        }
        // The answers, each to its own rule.
        for (id, rule) in self.open.clone() {
            if rule == Rule::PopUp {
                continue;
            }
            let answered = match ui.poll(id) {
                Answer::Waiting => continue,
                Answer::Gone => None,
                Answer::Closed(answered) => answered,
            };
            self.act(&mut *ui, rule, answered, interaction, world, now);
            ui.close(id);
            self.open.retain(|(q, _)| *q != id);
        }
        // **The server's withdrawal.** It closes the question while the slot is still set, so
        // the close answers **No** on the way out: a server that withdraws one of its questions
        // hears a refusal. A fellowship withdrawal matches on the type alone, any context.
        for (confirmation_type, context_id) in aborts {
            let found = self.open.iter().position(|(_, r)| match *r {
                Rule::FellowshipRequest { .. } => confirmation_type == 4,
                Rule::ServerConfirmation {
                    confirmation_type: t,
                    context_id: c,
                } => confirmation_type != 4 && t == confirmation_type && c == context_id,
                Rule::AcceptSwear { context_id: c } => confirmation_type == 1 && c == context_id,
                _ => false,
            });
            let Some(at) = found else {
                interaction.stats.confirmations_aborts_unmatched += 1;
                continue;
            };
            let (id, rule) = self.open[at];
            // The context answered with is the slot's, not the withdrawal's.
            let answered_context = match rule {
                Rule::FellowshipRequest { context_id } => context_id,
                _ => context_id,
            };
            interaction.confirm_server_confirmation(confirmation_type, answered_context, false);
            interaction.stats.confirmations_aborted += 1;
            ui.close(id);
            self.open.retain(|(q, _)| *q != id);
        }
        // **The mini-game's resign question**: all at once, its own slot on the game.
        if world.minigame.resign_prompt_pending {
            world.minigame.resign_prompt_pending = false;
            if self.ask(
                &mut *ui,
                Rule::MiniGameQuit,
                Prompt::Text(dereth_client_model::minigame::RESIGN_PROMPT.to_owned()),
                (false, false, true),
                (None, None),
            ) {
                world.minigame.resign_dialog = u32::try_from(self.next).unwrap_or(u32::MAX);
            } else {
                world.minigame.cancel_resign_dialog();
            }
        }
        // **The house panel's two questions**, a slot each.
        for rent in house_payments {
            if self.has(|r| *r == Rule::HousePayment { rent }) {
                continue;
            }
            let prompt = if rent {
                Prompt::HouseRentByProxy
            } else if let Some(worded) = world
                .world_rules
                .text(dereth_primitives::TextKey::HouseBuyConfirmation)
            {
                Prompt::HouseBuyAs(worded.to_owned())
            } else {
                Prompt::HouseBuy
            };
            if !self.ask(
                &mut *ui,
                Rule::HousePayment { rent },
                prompt,
                (false, false, false),
                (None, None),
            ) {
                // The panel set its slot before asking; a box that could not be made releases it.
                ui.deliver(UiRequest::HousePaymentConfirmationAnswer {
                    rent,
                    confirmed: None,
                });
            }
        }
        // **The server's confirmation requests.** The pair is stored before the one-at-a-time
        // guard, so a second question while one is up takes over the open box's pair: nothing
        // new is shown, and answering the box answers the second question. Retail's own, kept.
        for (confirmation_type, context_id, text) in requests {
            if let Some((_, slot)) = self
                .open
                .iter_mut()
                .find(|(_, r)| matches!(r, Rule::ServerConfirmation { .. }))
            {
                *slot = Rule::ServerConfirmation {
                    confirmation_type,
                    context_id,
                };
                continue;
            }
            self.ask(
                &mut *ui,
                Rule::ServerConfirmation {
                    confirmation_type,
                    context_id,
                },
                Prompt::Text(prompt_for(confirmation_type, &text)),
                (false, true, false),
                (None, None),
            );
        }
        // **The fellowship invitation**: its own slot, not modal, so it can sit beside the rest.
        for (context_id, name) in fellowship {
            if self.has(|r| matches!(r, Rule::FellowshipRequest { .. })) {
                continue;
            }
            self.ask(
                &mut *ui,
                Rule::FellowshipRequest { context_id },
                Prompt::FellowshipRequest { name },
                (false, false, false),
                (None, None),
            );
        }
        // **The three ordinary-use confirmations**: altar, altar, volatile rare. No guard; a
        // second waits behind the first.
        for (object, kind) in usage {
            self.ask(
                &mut *ui,
                Rule::Usage(object),
                Prompt::Text(kind.prompt().to_string()),
                (false, false, false),
                (Some(object), None),
            );
        }
        for (source, target, kind) in notices {
            let (Some(src), Some(tgt)) = (world.weenie(source), world.weenie(target)) else {
                continue;
            };
            // These callers use raw names, not the object-name lookup.
            let prompt = match kind {
                TargetedUsageConfirmation::ManaStone => format!(
                    "\nAre you sure you want to attempt to destroy your {} and drain its mana into this stone?",
                    tgt.pwd.name
                ),
                TargetedUsageConfirmation::Salvage => format!(
                    "\nAre you sure you want to apply the {} to the {}? The {} may be destroyed.",
                    src.pwd.name.replace(" (100)", ""),
                    tgt.pwd.name,
                    tgt.pwd.name
                ),
            };
            self.ask(
                &mut *ui,
                Rule::TargetedUse(source, target),
                Prompt::Text(prompt),
                (false, false, false),
                (Some(source), Some(target)),
            );
        }
        // **A swear request** asks the patron to accept: its own slot.
        for (context_id, name) in interaction.take_swear_requests() {
            if self.has(|r| matches!(r, Rule::AcceptSwear { .. })) {
                continue;
            }
            self.ask(
                &mut *ui,
                Rule::AcceptSwear { context_id },
                Prompt::AcceptSwear { name },
                (false, false, false),
                (None, None),
            );
        }
        // **The allegiance panel's swear, break and kick**: a slot each.
        for (action, target, prompt) in interaction.take_allegiance_confirmations() {
            if self.has(|r| matches!(r, Rule::Allegiance { action: a, .. } if *a == action)) {
                continue;
            }
            self.ask(
                &mut *ui,
                Rule::Allegiance { action, target },
                Prompt::Text(prompt),
                (false, false, false),
                (None, None),
            );
        }
        // **`@die`** keeps no slot: a second `@die` asks again, behind the first.
        for prompt in interaction.take_die_confirmations() {
            self.ask(
                &mut *ui,
                Rule::Die,
                Prompt::Text(prompt.to_string()),
                (false, false, false),
                (None, None),
            );
        }
        // **`@house abandon`**'s two stages keep no slot either.
        for prompt in interaction.take_house_abandon_first() {
            self.ask(
                &mut *ui,
                Rule::HouseAbandonFirst,
                Prompt::Text(prompt.to_string()),
                (false, false, false),
                (None, None),
            );
        }
        for prompt in interaction.take_house_abandon_second() {
            self.ask(
                &mut *ui,
                Rule::HouseAbandonSecond,
                Prompt::Text(prompt.to_string()),
                (false, false, false),
                (None, None),
            );
        }
        // **The vendor's unfinished basket**: one at a time.
        for prompt in closes {
            if self.has(|r| *r == Rule::CloseVendor) {
                continue;
            }
            self.ask(
                &mut *ui,
                Rule::CloseVendor,
                Prompt::Text(prompt.to_string()),
                (false, false, false),
                (None, None),
            );
        }
        ui.refresh();
    }

    /// What an answer does, by the question's rule.
    fn act(
        &mut self,
        ui: &mut dyn DialogPresenter,
        rule: Rule,
        answered: Option<bool>,
        interaction: &mut Interaction,
        world: &mut dereth_client_model::World,
        now: LocalTime,
    ) {
        let yes = answered == Some(true);
        match rule {
            Rule::Usage(object) if yes => {
                interaction.confirm_usage(world, object, ServerTime(now.0));
            }
            Rule::TargetedUse(source, target) if yes => {
                interaction.confirm_targeted_usage(world, source, target, ServerTime(now.0));
            }
            Rule::CloseVendor if yes => interaction.confirm_vendor_close(world),
            // No branch on the answer: a refusal is a `0x0275` with `accepted = 0`, so the server
            // learns of it at once. A box that died with its framework has no answer, and sends
            // nothing.
            Rule::ServerConfirmation {
                confirmation_type,
                context_id,
            } => {
                if let Some(accepted) = answered {
                    interaction.confirm_server_confirmation(
                        confirmation_type,
                        context_id,
                        accepted,
                    );
                }
            }
            Rule::Allegiance { action, target } if yes => {
                interaction.confirm_allegiance(world, action, target);
            }
            Rule::Die if yes => interaction.confirm_die(world),
            Rule::MiniGameQuit => match answered {
                Some(confirmed) => ui.deliver(UiRequest::MiniGameQuitAnswer(confirmed)),
                None => world.minigame.cancel_resign_dialog(),
            },
            Rule::HousePayment { rent } => ui.deliver(UiRequest::HousePaymentConfirmationAnswer {
                rent,
                confirmed: answered,
            }),
            Rule::HouseAbandonFirst if yes => interaction.confirm_house_abandon_first(),
            Rule::HouseAbandonSecond if yes => interaction.confirm_house_abandon(world),
            Rule::AcceptSwear { context_id } => {
                if let Some(accepted) = answered {
                    interaction.confirm_server_confirmation(1, context_id, accepted);
                }
            }
            Rule::FellowshipRequest { context_id } => {
                if let Some(accepted) = answered {
                    interaction.confirm_server_confirmation(4, context_id, accepted);
                }
            }
            Rule::Usage(_)
            | Rule::TargetedUse(..)
            | Rule::CloseVendor
            | Rule::Allegiance { .. }
            | Rule::Die
            | Rule::HouseAbandonFirst
            | Rule::HouseAbandonSecond
            | Rule::PopUp => {}
        }
    }
}

/// What the box says for a server confirmation, by its type: four of the five append the same
/// `" Continue?"` to the server's text, and the plain Yes/No request (type 7) is the server's text
/// verbatim. That is why an NPC's question reads as the NPC wrote it and a gem's warning gets a
/// question mark bolted on.
#[must_use]
pub fn prompt_for(confirmation_type: i32, text: &str) -> String {
    match confirmation_type {
        7 => text.to_string(),
        _ => format!("{text} Continue?"),
    }
}
