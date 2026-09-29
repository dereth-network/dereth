//! `ChatInterface`'s entry-field behaviour around the command line.
//!
//! Covers the chat interface's command processing, its history selection, and the enter,
//! command and reply key handlers.
//!
//! The history itself lives on [`crate::cmd::CommandInterp`], because that is where the submitted
//! line arrives. This module is the small set of actions the chat entry field takes.

use dereth_client_contract::actions::ActionId;

/// The chat-entry action ids, which are the shared vocabulary's.
pub use dereth_client_contract::actions::chat_entry as action;

/// What a chat-entry action asks the field to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatEntryAction {
    /// Focus the field and notify the UI that chat entry is active.
    Focus {
        prefill: &'static str,
    },
    /// Submit, and unless *Stay in Chat Mode After Sending a Message*
    /// (action `0x1000007C`) is set, un-focus and
    /// send the toggle-chat-entry notice with `false`.
    Submit {
        stay_in_chat_mode: bool,
    },
    NotHandled,
}

/// `ChatInterface`'s three entry points, plus the reply keys.
#[must_use]
pub fn on_action(a: ActionId, stay_in_chat_mode: bool) -> ChatEntryAction {
    match a {
        action::BEGIN_CHAT_MODE | action::TOGGLE_CHAT_ENTRY => {
            ChatEntryAction::Submit { stay_in_chat_mode }
        }
        // The start-command key pre-fills from string-table enum 6, i.e. the `/`.
        action::START_COMMAND => ChatEntryAction::Focus { prefill: "/" },
        // The reply key pre-fills the appropriate prefix.
        action::REPLY => ChatEntryAction::Focus { prefill: "@reply " },
        action::MONARCH_REPLY => ChatEntryAction::Focus { prefill: "@mr " },
        action::PATRON_REPLY => ChatEntryAction::Focus { prefill: "@pr " },
        action::TELL_TO_SELECTED => ChatEntryAction::Focus { prefill: "@tell " },
        _ => ChatEntryAction::NotHandled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the recovered command-interpreter behavior §6 — *Start Command* pre-fills `/`, and the two chat
    /// keys submit.
    #[test]
    fn the_chat_entry_actions_do_what_the_document_says() {
        assert_eq!(
            on_action(action::START_COMMAND, false),
            ChatEntryAction::Focus { prefill: "/" }
        );
        assert_eq!(
            on_action(action::BEGIN_CHAT_MODE, true),
            ChatEntryAction::Submit {
                stay_in_chat_mode: true
            }
        );
        assert_eq!(
            on_action(ActionId(0x29), false),
            ChatEntryAction::NotHandled
        );
    }
}
