//! A screen-size prompt rendered from the runtime's tagged view, without a local deadline.
use crate::{ElemHandle, PropertyCollection, PropertyValue, UiSystem};
use dereth_client_contract::resolution::{
    ResolutionAction, ResolutionPrompt, ResolutionPromptKind,
};
use dereth_client_contract::UiRequest;

/// The dialog context belongs to this renderer; its meaning belongs to the runtime.
#[derive(Debug, Default)]
pub struct ResolutionDialog {
    shown: Option<(ResolutionPrompt, u64)>,
}
impl ResolutionDialog {
    /// Remove the previous interface's element without answering its question.
    pub fn clear(&mut self, ui: &mut UiSystem) {
        if let Some((_, context)) = self.shown.take() {
            if let Some(root) = ui.dialogs.close_dialog(context, ui.now.0) {
                ui.remove_and_delete_root(root);
            }
        }
    }
    fn text(ui: &UiSystem, p: ResolutionPrompt) -> String {
        match p.kind {
            ResolutionPromptKind::OfferTest => "Test new screen size first, for 15 seconds?".into(),
            ResolutionPromptKind::Accept => {
                let seconds = p.deadline.map_or(0.0, |d| (d - ui.now.0).max(0.0)).floor();
                let table = ui
                    .env()
                    .and_then(|e| e.did_by_enum(4, 0x10000004))
                    .unwrap_or(dereth_primitives::DataId(0x23000004));
                let token = dereth_primitives::num::hash::str_hash(b"ID_Option_ConfirmChange");
                ui.resolve_string_rendered(table, token, &[format!("{seconds:.0}")])
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| format!("Accept this setting? ({seconds:.0})"))
            }
            ResolutionPromptKind::ApplyFailed => "Unable to change screen size!".into(),
            ResolutionPromptKind::RevertFailed => {
                "Unable to restore the previous screen size!".into()
            }
            ResolutionPromptKind::Reset => "Resolution Reset".into(),
        }
    }
    /// Project or answer a prompt. A failed factory rejects the test rather than keeping it silently.
    pub fn project(&mut self, ui: &mut UiSystem, prompt: Option<ResolutionPrompt>) {
        if self.shown.as_ref().map(|(p, _)| *p) != prompt {
            self.clear(ui);
        }
        let Some(p) = prompt else {
            return;
        };
        let question = matches!(
            p.kind,
            ResolutionPromptKind::OfferTest | ResolutionPromptKind::Accept
        );
        if let Some((_, context)) = self.shown {
            let root = ui.dialogs.info(context).and_then(|i| i.element);
            let answer = root
                .and_then(|r| super::types::dialog_element(ui, r))
                .and_then(|d| d.answer_property())
                .and_then(|(_, v)| {
                    if let PropertyValue::Bool(v) = v {
                        Some(v)
                    } else {
                        None
                    }
                });
            if let Some(yes) = answer {
                ui.requests.emit(UiRequest::Resolution(if question {
                    ResolutionAction::Answer {
                        token: p.token,
                        yes,
                    }
                } else {
                    ResolutionAction::Dismiss { token: p.token }
                }));
                self.clear(ui);
                return;
            }
            if let Some(root) = root {
                Self::caption(ui, root, p);
            }
            return;
        }
        let mut data = PropertyCollection::new();
        data.set(
            crate::props::attr::DIALOG_KIND,
            PropertyValue::Integer(if question {
                super::DialogKind::Confirmation.property()
            } else {
                3
            }),
        );
        data.set(
            crate::props::attr::DIALOG_QUEUE_ID,
            PropertyValue::Integer(1),
        );
        data.set(crate::props::attr::DIALOG_MODAL, PropertyValue::Bool(true));
        data.set(
            crate::props::attr::DIALOG_COUNTDOWN_TEXT,
            PropertyValue::String(Self::text(ui, p)),
        );
        let context = ui.dialogs.make_dialog(data, ui.now.0);
        let root = context.and_then(|_| {
            ui.require_env()
                .and_then(|e| {
                    e.create_and_add_root_element(
                        ui,
                        crate::LayoutEnum(2),
                        if question {
                            super::DialogKind::Confirmation.root_element_id()
                        } else {
                            super::DialogKind::Message.root_element_id()
                        },
                    )
                })
                .ok()
        });
        if let (Some(context), Some(root)) = (context, root) {
            let info = ui.dialogs.info(context).unwrap().clone();
            ui.set_attribute_bool(root, crate::props::attr::DIALOG_MODAL, true);
            super::types::set_dialog_data(ui, root, &info.data);
            Self::caption(ui, root, p);
            if ui.bind_dialog_element(context, root) {
                self.shown = Some((p, context));
                return;
            }
            ui.remove_and_delete_root(root);
        }
        if let Some(context) = context {
            ui.dialogs.close_dialog(context, ui.now.0);
        }
        ui.requests.emit(UiRequest::Resolution(if question {
            ResolutionAction::Unavailable { token: p.token }
        } else {
            ResolutionAction::Dismiss { token: p.token }
        }));
    }
    fn caption(ui: &mut UiSystem, root: ElemHandle, p: ResolutionPrompt) {
        let text = Self::text(ui, p);
        if let Some(e) = ui
            .get_child_recursive(root, super::base::child::TEXT)
            .and_then(|h| ui.text_element_mut(h))
        {
            e.set_text(&text);
        }
        super::base::update_popup_size_and_position(ui, root);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Behaviour: presentation.resolution.shared-transaction
    #[test]
    fn missing_dialog_assets_reject_the_tagged_prompt_without_authorizing_a_size() {
        for kind in [
            ResolutionPromptKind::OfferTest,
            ResolutionPromptKind::Accept,
        ] {
            let mut ui = UiSystem::new((800, 600));
            let mut renderer = ResolutionDialog::default();
            renderer.project(
                &mut ui,
                Some(ResolutionPrompt {
                    token: 42,
                    kind,
                    deadline: Some(15.0),
                }),
            );
            assert_eq!(
                ui.requests.take(),
                vec![UiRequest::Resolution(ResolutionAction::Unavailable {
                    token: 42
                })]
            );
            assert!(ui.dialogs.non_queued().is_empty());
            assert!(renderer.shown.is_none());
        }
    }
}
