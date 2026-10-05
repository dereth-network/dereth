//! The game's questions -- `dereth_client_runtime::dialogs` -- shown through the modern UI's
//! dialog factory.
//!
//! What a question is, which ones open and what an answer does are the runtime's. What is here is
//! only how the modern UI shows one: the property collection the factory builds the box from, the
//! wording of the questions whose sentence is a string-table row, the elements, and taking a box
//! down.
use dereth_client_runtime::dialogs::{Answer, DialogPresenter, Prompt, Question, QuestionId};
use dereth_primitives::LocalTime;
use dereth_ui::{dialog, PropertyCollection, PropertyValue};

const SOURCE: u32 = 0x1000_003d;
const TARGET: u32 = 0x1000_003e;

/// The boxes the modern UI has made for the runtime's questions.
#[derive(Debug, Default)]
pub(crate) struct TargetedDialogs {
    /// Each question's dialog context, and whether it is a one-button message (a pop-up string),
    /// which the factory opens as a plain dialog with no callback.
    shown: Vec<(QuestionId, u64, bool)>,
    /// A box whose close has begun because it was answered, waiting for the answer to be acted on.
    #[allow(clippy::type_complexity)]
    closing: Vec<(
        QuestionId,
        Option<u64>,
        dereth_ui::dialog::factory::DialogInfo,
    )>,
}

impl TargetedDialogs {
    /// One pass of the runtime's dialog service, shown here, or with no UI to show it in.
    pub(crate) fn service_with<S: dereth_client_runtime::shell::Shell>(
        &mut self,
        cx: &mut dereth_client_runtime::ui_context::UiContext<'_, S>,
        shell: Option<&mut crate::ui::UiShell>,
        now: LocalTime,
    ) {
        match shell {
            Some(shell) => {
                let mut presenter = Presenter {
                    shell,
                    state: self,
                    now: now.0,
                };
                cx.service_dialogs(Some(&mut presenter), now);
            }
            None => {
                self.shown.clear();
                self.closing.clear();
                cx.service_dialogs(None, now);
            }
        }
    }

    fn context(&self, id: QuestionId) -> Option<(u64, bool)> {
        self.shown
            .iter()
            .find(|(q, _, _)| *q == id)
            .map(|(_, c, m)| (*c, *m))
    }
}

struct Presenter<'a> {
    shell: &'a mut crate::ui::UiShell,
    state: &'a mut TargetedDialogs,
    now: f64,
}

impl Presenter<'_> {
    /// The sentence a question shows: its own, or the string-table row the modern UI words it with.
    fn text(&mut self, prompt: &Prompt) -> String {
        let ui = &mut self.shell.ui;
        match prompt {
            Prompt::Text(text) => text.clone(),
            Prompt::FellowshipRequest { name } => {
                dereth_ui_screens::panels::fellowship::fellowship_request_prompt(ui, name)
            }
            Prompt::AcceptSwear { name } => {
                dereth_ui_screens::panels::allegiance::accept_swear_prompt(ui, name)
            }
            Prompt::HouseBuy => dereth_ui_screens::panels::slumlord::BUY_CONFIRMATION.to_owned(),
            Prompt::HouseBuyAs(text) => text.clone(),
            Prompt::HouseRentByProxy => {
                dereth_ui_screens::panels::slumlord::RENT_BY_PROXY_CONFIRMATION.to_owned()
            }
        }
    }

    /// Take a box down: the close notice, the root, the factory's close, and the answered record.
    fn finish(
        &mut self,
        context: u64,
        queue: Option<u64>,
        info: dereth_ui::dialog::factory::DialogInfo,
    ) {
        let ui = &mut self.shell.ui;
        let root = info.element;
        ui.send_notice(
            dereth_ui::NoticeId::DialogClosed,
            &dereth_ui::NoticePayload {
                a: u32::try_from(context).expect("32-bit dialog context"),
                ..Default::default()
            },
        );
        if let Some(root) = root {
            ui.remove_and_delete_root(root);
        }
        ui.dialogs.finish_close_dialog(queue, info, self.now);
    }
}

impl DialogPresenter for Presenter<'_> {
    fn accepting(&self) -> bool {
        self.shell.flow.current_mode() == Some(dereth_ui::framework::mode::GAME_PLAY)
            && self.shell.flow.current().is_some()
    }

    fn generation(&self) -> u64 {
        self.shell.flow.switches
    }

    /// The property collection each kind of question is built from: `0x8E` the kind (1 Yes/No,
    /// 3 the one-button message), `0xAC` modal (the server's confirmation requests alone), `0xC3`
    /// the factory's all-at-once queue (the pop-ups and the mini-game's resign question; the rest
    /// take the default queue and wait their turn), `0xC5` the sentence, and the objects the
    /// question is about. A pop-up has no callback: retail opens it as a plain dialog.
    fn open(&mut self, question: &Question) -> bool {
        let text = self.text(&question.prompt);
        let ui = &mut self.shell.ui;
        let mut data = PropertyCollection::new();
        if question.message {
            data.set(dereth_ui::props::attr::DIALOG_KIND, PropertyValue::Enum(3));
            data.set(
                dereth_ui::props::attr::DIALOG_QUEUE_ID,
                PropertyValue::Enum(u32::try_from(dialog::factory::NON_QUEUED).unwrap_or(1)),
            );
            data.set(
                dereth_ui::props::attr::DIALOG_COUNTDOWN_TEXT,
                PropertyValue::String(text),
            );
        } else {
            data.set(0x8e, PropertyValue::Enum(1));
            if question.modal {
                data.set(0xac, PropertyValue::Bool(true));
            }
            if question.all_at_once {
                data.set(0xc3, PropertyValue::Enum(1));
            }
            data.set(0xc5, PropertyValue::String(text));
            if let Some(o) = question.subject {
                data.set(SOURCE, PropertyValue::InstanceId(o.0));
            }
            if let Some(o) = question.target {
                data.set(TARGET, PropertyValue::InstanceId(o.0));
            }
        }
        let Some(context) = ui.dialogs.make_dialog(data, self.now) else {
            return false;
        };
        if !question.message {
            ui.dialogs.note_callback(context);
        }
        self.state
            .shown
            .push((question.id, context, question.message));
        true
    }

    fn poll(&mut self, id: QuestionId) -> Answer {
        let Some((context, message)) = self.state.context(id) else {
            return Answer::Gone;
        };
        let ui = &mut self.shell.ui;
        let Some(info) = ui.dialogs.info(context) else {
            // A pop-up the factory no longer has is gone; any other question is still queued.
            if message {
                self.state.shown.retain(|(q, _, _)| *q != id);
                return Answer::Gone;
            }
            return Answer::Waiting;
        };
        let Some(root) = info.element else {
            return Answer::Waiting; // still owed an element
        };
        if message {
            // The message dialog closes its stored context when element message 0x26 carries
            // code 1: no answer property, no callback, only the close.
            if !dialog::types::dialog_element(ui, root).is_some_and(|d| d.answer.is_some()) {
                return Answer::Waiting;
            }
            let Some((queue, info)) = ui.dialogs.begin_close_dialog(context) else {
                return Answer::Waiting;
            };
            self.state.closing.push((id, queue, info));
            return Answer::Closed(None);
        }
        // Only this question's exact root: unrelated logout and character buttons use ids 17/19.
        let answer = dialog::types::dialog_element(ui, root).and_then(|d| d.answer_property());
        if answer.is_none() && ui.node(root).is_some() {
            return Answer::Waiting;
        }
        if let Some((key, value)) = answer {
            ui.dialogs.set_answer_property(context, key, value);
        }
        let (queue, info) = ui
            .dialogs
            .begin_close_dialog(context)
            .expect("open context");
        let answered = info.data.get_bool(0x92);
        self.state.closing.push((id, queue, info));
        Answer::Closed(answered)
    }

    /// Answered: the callback is taken, then the close notice, the root and the factory's close.
    /// Withdrawn by the server: the close begins here, while the slot is still set, and the
    /// callback is taken after it.
    fn close(&mut self, id: QuestionId) {
        let Some((context, message)) = self.state.context(id) else {
            return;
        };
        if let Some(at) = self.state.closing.iter().position(|(q, _, _)| *q == id) {
            let (_, queue, info) = self.state.closing.remove(at);
            if !message {
                self.shell.ui.dialogs.take_callback(context);
            }
            self.finish(context, queue, info);
        } else if let Some((queue, info)) = self.shell.ui.dialogs.begin_close_dialog(context) {
            self.finish(context, queue, info);
            self.shell.ui.dialogs.take_callback(context);
        } else {
            self.shell.ui.dialogs.take_callback(context);
        }
        self.shell
            .ui
            .dialogs
            .completed
            .retain(|i| i.context != context);
        self.state.shown.retain(|(q, _, _)| *q != id);
    }

    fn deliver(&mut self, request: dereth_client_contract::UiRequest) {
        self.shell.ui.requests.emit(request);
    }

    /// The flow has already reset the boxes as done. The questions' answers are dropped with them;
    /// the pop-ups went with the framework.
    fn reset(&mut self) {
        let shown = std::mem::take(&mut self.state.shown);
        self.state.closing.clear();
        self.shell
            .ui
            .dialogs
            .completed
            .retain(|i| !shown.iter().any(|(_, c, m)| !*m && *c == i.context));
    }

    fn refresh(&mut self) {
        let ui = &mut self.shell.ui;
        for (context, kind) in ui.dialogs.pending_create() {
            // A context this front end raised, with or without a callback: a pop-up has none and
            // still needs its element, and something else's dialog must still not be built here.
            if !self.state.shown.iter().any(|(_, c, _)| *c == context) {
                continue;
            }
            let data = ui
                .dialogs
                .info(context)
                .expect("factory pending create")
                .data
                .clone();
            let root = match ui.require_env().and_then(|e| {
                e.create_and_add_root_element(ui, dereth_ui::LayoutEnum(2), kind.root_element_id())
            }) {
                Ok(root) => root,
                Err(error) => {
                    tracing::warn!("targeted dialog creation failed: {error:?}");
                    continue;
                }
            };
            // Defaults modal=false; preserve shipped Yes/No captions.
            ui.set_attribute_bool(root, dereth_ui::props::attr::DIALOG_MODAL, false);
            if let Some(PropertyValue::String(text)) = data.get(0xc5) {
                if let Some(h) = ui.get_child_recursive(root, dialog::base::child::TEXT) {
                    if let Some(t) = ui.text_element_mut(h) {
                        t.set_text(text);
                    }
                }
            }
            dialog::types::set_dialog_data(ui, root, &data);
            dialog::base::update_popup_size_and_position(ui, root);
            // The UI-flow factory reset deletes these with the framework.
            ui.bind_dialog_element(context, root);
        }
        // Pending-dialog refresh sets only child 0x33's state: pending -> 24, none -> 25.
        // The boolean-to-state arithmetic does NOT format the count into child 0x34.
        for (_, context, message) in &self.state.shown {
            if *message {
                continue;
            }
            let Some(info) = ui.dialogs.info(*context) else {
                continue;
            };
            let Some(root) = info.element else { continue };
            let pending = info.dialog.as_ref().is_some_and(|d| d.pending_behind != 0);
            if let Some(h) = ui.get_child_recursive(root, dereth_ui::ElementId(0x33)) {
                let state = dereth_ui::StateId(if pending { 24 } else { 25 });
                if ui.node(h).is_some_and(|n| n.state != state) {
                    ui.set_state(h, state);
                }
            }
        }
    }
}
