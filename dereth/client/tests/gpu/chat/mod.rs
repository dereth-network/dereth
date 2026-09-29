//! Chat: the chat window and its log, typed lines and slash commands, tells and replies,
//! heard speech and emotes, and how each family of line is formatted.

mod channel_and_age_replies;
pub(crate) mod chat_commands;
mod chat_entry_after_login;
mod chat_entry_keyboard_barrier;
mod chat_family_post_processing;
mod chat_log_scrolling;
mod chat_window_interaction;
mod heard_emotes;
mod heard_speech_formatting;
mod loc_command;
mod reply_targets;
mod slash_command_forwarding;
