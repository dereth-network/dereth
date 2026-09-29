// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Command/CommandManager.cs
//! Port of `Source/ACE.Server/Command/CommandManager.cs`.
//!
//! - **The table.** ACE's static `commandHandlers` is a process-wide
//!   `Dictionary<string, CommandHandlerInfo>(StringComparer.OrdinalIgnoreCase)`; here it is a
//!   process-wide [`DotNetDict`] behind a lock, so its enumeration order is .NET's. `Initialize`
//!   fills it from each handler file's attribute list in ACE's assembly type order (see
//!   [`initialize`]), which is what reflection over `ACE.Server` yields (the `commands` vectors).
//! - **Threads.** DIVERGE (arch): ACE's console thread parses a line, looks the handler up and
//!   invokes it on the console thread itself. Here the console thread parses; the lookup and the
//!   invoke are sent to the world thread (`submit`), where every command runs with the world.
//! - **The chat path.** `GameActionTalk.Handle`'s `@` branch lives here ([`handle_talk_command`])
//!   because empyrean-world cannot name this crate; [`initialize`] installs it into empyrean-world's Talk
//!   handler.
//! - **Exceptions.** A C# exception inside a handler is a panic here, caught and logged where ACE
//!   catches it.
//! - **Console output.** `Console.WriteLine` is [`console_write_line`] (stdout, through
//!   [`console`]) and the console's `log.Info` is [`console_log_info`], which writes the reply the
//!   same way whatever the log level (DIVERGE, arch); both can be captured on the calling thread
//!   for tests. On an interactive terminal the console reads lines with its line editor, so log
//!   lines and replies print above the prompt ([`editor_command_thread`]).

use std::cell::RefCell;
use std::io::{BufRead, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

use empyrean_common::dotnet::dict::DotNetDict;
use empyrean_entity::enums::{AccessLevel, ChatMessageType};
use empyrean_net::SessionId;
use empyrean_world::managers::player_manager::{equals_ordinal_ignore_case, OrdinalIgnoreCase};
use empyrean_world::network::game_action::actions::game_action_talk;
use empyrean_world::network::game_messages::game_message::enqueue_send;
use empyrean_world::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use empyrean_world::World;

use crate::command_handler::CommandHandler;
use crate::command_handler_attribute::CommandHandlerAttribute;
use crate::command_handler_flag::CommandHandlerFlag;
use crate::command_handler_info::{CommandHandlerInfo, NamedHandler};
use crate::command_handler_response::CommandHandlerResponse;
use crate::console;
use crate::handlers::{
    account_commands, admin_commands, admin_shard_commands, admin_stat_commands, advocate_commands,
    character_commands, console_commands, developer_commands, developer_content_commands,
    developer_database_commands, developer_fix_commands, developer_loot_commands, help_commands,
    player_commands, sentinel_commands,
};

/// A piece of work for the world thread (the host's world command queue).
pub type WorldCommand = Box<dyn FnOnce(&mut World) + Send>;

/// Hands a [`WorldCommand`] to the world thread; false when the world thread is gone.
pub type Submit = Box<dyn Fn(WorldCommand) -> bool + Send>;

/// The console prompt (ACE's is `ACE >> `).
pub const PROMPT: &str = "empyrean>> ";

/// `CommandManager.NonInteractiveConsole`: true when the configuration's
/// `server.interactive_console` is false.
///
/// DIVERGE: ACE reads the environment variable `ACE_NONINTERACTIVE_CONSOLE`; Empyrean's switch is
/// a configuration key, and no environment variable is read.
#[must_use]
pub fn non_interactive_console() -> bool {
    !empyrean_common::config_manager::ConfigManager::config()
        .server
        .interactive_console
}

/// `CommandManager.commandHandlers`.
static COMMAND_HANDLERS: LazyLock<RwLock<DotNetDict<OrdinalIgnoreCase, CommandHandlerInfo>>> =
    LazyLock::new(|| RwLock::new(DotNetDict::new()));

fn command_handlers() -> RwLockReadGuard<'static, DotNetDict<OrdinalIgnoreCase, CommandHandlerInfo>>
{
    COMMAND_HANDLERS
        .read()
        .unwrap_or_else(PoisonError::into_inner)
}

fn command_handlers_mut(
) -> RwLockWriteGuard<'static, DotNetDict<OrdinalIgnoreCase, CommandHandlerInfo>> {
    COMMAND_HANDLERS
        .write()
        .unwrap_or_else(PoisonError::into_inner)
}

fn key(command: &str) -> OrdinalIgnoreCase {
    OrdinalIgnoreCase(command.to_owned())
}

// ACE: CommandManager.GetCommands
#[must_use]
pub fn get_commands() -> Vec<CommandHandlerInfo> {
    command_handlers().values().cloned().collect()
}

// ACE: CommandManager.GetCommandByName
/// The entries whose `Command` equals `commandname`, ignoring case (ordinal).
#[must_use]
pub fn get_command_by_name(commandname: &str) -> Vec<CommandHandlerInfo> {
    command_handlers()
        .values()
        .filter(|p| equals_ordinal_ignore_case(&p.attribute.command, commandname))
        .cloned()
        .collect()
}

// ACE: CommandManager.GetDelegate
/// `Delegate.CreateDelegate` over the handler's method: a Rust handler is already the delegate.
#[must_use]
pub fn get_delegate(handler: CommandHandler) -> CommandHandler {
    handler
}

// ACE: CommandManager.TryAddCommand
/// The `(handler, command, access, flags, description, usage, overrides)` overloads: an attribute
/// with `ParameterCount = -1`. The handler comes with its name ([`crate::handler!`]).
pub fn try_add_command_handler(
    (handler, handler_name): NamedHandler,
    command: &str,
    access: AccessLevel,
    flags: CommandHandlerFlag,
    description: &str,
    usage: &str,
    overrides: bool,
) -> bool {
    let del = get_delegate(handler);
    let info = CommandHandlerInfo {
        attribute: CommandHandlerAttribute::with_description(
            command,
            access,
            flags,
            description,
            usage,
        ),
        handler: del,
        handler_name,
    };

    if try_add_command(Some(info), overrides) {
        return true;
    }

    false
}

// ACE: CommandManager.TryAddCommand
/// The `(CommandHandlerInfo, overrides = true)` overload.
pub fn try_add_command(command_handler: Option<CommandHandlerInfo>, overrides: bool) -> bool {
    let Some(command_handler) = command_handler else {
        return false;
    };

    let command = command_handler.attribute.command.clone();
    let mut handlers = command_handlers_mut();

    //Add if the command doesn't exist
    if !handlers.contains_key(&key(&command)) {
        handlers.add(key(&command), command_handler);
        log::info!("Command created: {command}");
        return true;
    }
    //Update if overriding and the command exists
    else if overrides {
        log::info!("Command updated: {command}");
        handlers.insert(key(&command), command_handler);
        return true;
    }
    log::warn!("Failed to add command: {command}");
    false
}

// ACE: CommandManager.TryRemoveCommand
pub fn try_remove_command(command: &str) -> bool {
    let mut handlers = command_handlers_mut();
    if !handlers.contains_key(&key(command)) {
        return false;
    }

    log::info!("Removed command: {command}");
    handlers.remove(&key(command));
    true
}

/// Every `[CommandHandler]` decoration in `ACE.Server`, in the order reflection yields them
/// (`Assembly.GetTypes()`, then each type's methods, then each method's attributes).
///
/// The handler classes come in name order: AccountCommands, AdminCommands, AdminShardCommands,
/// AdminStatCommands, AdvocateCommands, CharacterCommands, ConsoleCommands, DeveloperCommands,
/// DeveloperDatabaseCommands, DeveloperFixCommands, DeveloperLootCommands, HelpCommands,
/// PlayerCommands, SentinelCommands, then DeveloperContentCommands (its namespace is
/// `...Handlers.Processors`), then the mod loader's ModCommands, which is not ported.
fn all_command_handlers() -> Vec<CommandHandlerInfo> {
    let mut all = Vec::new();
    all.extend(account_commands::command_handlers());
    all.extend(admin_commands::command_handlers());
    all.extend(admin_shard_commands::command_handlers());
    all.extend(admin_stat_commands::command_handlers());
    all.extend(advocate_commands::command_handlers());
    all.extend(character_commands::command_handlers());
    all.extend(console_commands::command_handlers());
    all.extend(developer_commands::command_handlers());
    all.extend(developer_database_commands::command_handlers());
    all.extend(developer_fix_commands::command_handlers());
    all.extend(developer_loot_commands::command_handlers());
    all.extend(help_commands::command_handlers());
    all.extend(player_commands::command_handlers());
    all.extend(sentinel_commands::command_handlers());
    // DeveloperContentCommands comes last: it is declared in the `...Handlers.Processors`
    // namespace, and reflection yields it after the other handler classes (ACE's own table, the
    // `command_registry` vector, has it there). Its `nudge` takes DeveloperCommands' slot.
    all.extend(developer_content_commands::command_handlers());
    // Not ACE: Empyrean's own commands, after ACE's (a row with an ACE command's name takes that
    // command's slot; see `crate::empyrean`).
    all.extend(crate::empyrean::command_handlers());
    all
}

// ACE: CommandManager.Initialize
/// Fills the table and installs the chat path; then, unless the host gives no world queue
/// (`submit` is `None`, as in tests) or the console is turned off ([`non_interactive_console`]),
/// starts the console thread.
pub fn initialize(submit: Option<Submit>) {
    {
        let mut handlers = command_handlers_mut();
        for command_handler in all_command_handlers() {
            let command = command_handler.attribute.command.clone();
            handlers.insert(key(&command), command_handler);
        }
    }

    // Not ACE: empyrean-world's Talk handler reaches this crate through a hook (see the module docs).
    game_action_talk::set_command_path(handle_talk_command);

    let Some(submit) = submit else { return };

    if non_interactive_console() {
        // DIVERGE: names Empyrean and the configuration key (brand; ACE names its variable).
        log::info!(
            "{} command prompt disabled - server.interactive_console is false",
            empyrean_common::brand::PRODUCT
        );
        return;
    }

    let spawned = std::thread::Builder::new()
        .name("Command Manager".to_owned())
        .spawn(move || {
            if console::editor_available() {
                editor_command_thread(&*submit);
            } else {
                let stdin = std::io::stdin();
                command_thread(stdin.lock(), &*submit);
            }
        });
    if let Err(e) = spawned {
        log::error!("Unable to start the Command Manager thread: {e}");
    }
}

/// Not ACE: where the lines [`command_thread`] reads go.
enum LineSink {
    /// Run as console commands.
    Commands,
    /// Kept, unrun, until they can be forwarded (the server is handing over to another process).
    Held(Vec<String>),
    /// Written to another server process's standard input.
    Forwarded(Box<dyn Write + Send>),
}

static LINE_SINK: Mutex<LineSink> = Mutex::new(LineSink::Commands);

/// Set while [`command_thread`] reads its input.
static LINE_READER: AtomicBool = AtomicBool::new(false);

/// Not ACE: whether [`command_thread`] is reading the console's input (a pipe or a file; the line
/// editor reads a terminal instead and ends with [`console::stop`]).
pub fn line_reader_running() -> bool {
    LINE_READER.load(Ordering::SeqCst)
}

/// Not ACE: from now on the lines [`command_thread`] reads are kept instead of run, for
/// [`forward_console_input`]. A server that is about to start another server process in its place
/// calls it once its own world has stopped: the reader cannot be interrupted, so it must not take
/// the new server's input and drop it.
pub fn hold_console_input() {
    let mut sink = LINE_SINK.lock().unwrap_or_else(PoisonError::into_inner);
    if matches!(*sink, LineSink::Commands) {
        *sink = LineSink::Held(Vec::new());
    }
}

/// Not ACE: the lines [`command_thread`] reads, those held first, are written to `to` (the new
/// server process's standard input) from now on. False when no line reader is running (the new
/// process can then read the console's input itself).
pub fn forward_console_input(mut to: Box<dyn Write + Send>) -> bool {
    if !line_reader_running() {
        return false;
    }
    let mut sink = LINE_SINK.lock().unwrap_or_else(PoisonError::into_inner);
    if let LineSink::Held(lines) = &*sink {
        for line in lines {
            let _ = to.write_all(line.as_bytes());
        }
        let _ = to.flush();
    }
    *sink = LineSink::Forwarded(to);
    true
}

/// Takes `line` (as read, with its terminator) when the lines are held or forwarded; false when
/// it is to be run.
fn divert(line: &str) -> bool {
    let mut sink = LINE_SINK.lock().unwrap_or_else(PoisonError::into_inner);
    let mut line = line.to_owned();
    if !line.ends_with('\n') {
        line.push('\n');
    }
    match &mut *sink {
        LineSink::Commands => false,
        LineSink::Held(lines) => {
            lines.push(line);
            true
        }
        LineSink::Forwarded(to) => {
            let _ = to.write_all(line.as_bytes());
            let _ = to.flush();
            true
        }
    }
}

/// Whether the lines read are run here (the prompt is shown only then).
fn running_commands() -> bool {
    matches!(
        *LINE_SINK.lock().unwrap_or_else(PoisonError::into_inner),
        LineSink::Commands
    )
}

/// Marks [`LINE_READER`] for the life of [`command_thread`]; at the end of the input a forwarded
/// process's standard input is closed too.
struct ReaderMark;

impl ReaderMark {
    fn new() -> Self {
        LINE_READER.store(true, Ordering::SeqCst);
        Self
    }
}

impl Drop for ReaderMark {
    fn drop(&mut self) {
        LINE_READER.store(false, Ordering::SeqCst);
        let mut sink = LINE_SINK.lock().unwrap_or_else(PoisonError::into_inner);
        if matches!(*sink, LineSink::Forwarded(_)) {
            *sink = LineSink::Commands;
        }
    }
}

// ACE: CommandManager.CommandThread
/// The console loop over `input`. Each line is parsed here; the lookup and the invoke run on the
/// world thread through `submit` (see the module docs).
///
/// At the end of the input the prompt is disabled and the thread ends. Not ACE: once the server
/// hands over to another server process, the lines are held and then forwarded to it
/// ([`hold_console_input`], [`forward_console_input`]).
pub fn command_thread(mut input: impl BufRead, submit: &dyn Fn(WorldCommand) -> bool) {
    let _reading = ReaderMark::new();
    console_banner();

    loop {
        if running_commands() {
            console_write(PROMPT);
        }

        let mut line = String::new();
        match input.read_line(&mut line) {
            Ok(0) => {
                if running_commands() {
                    console_input_closed();
                }
                return;
            }
            Err(_) => return,
            Ok(_) => {}
        }
        if divert(&line) {
            continue;
        }
        // Console.ReadLine drops the line terminator.
        let command_line = match line.strip_suffix('\n') {
            Some(l) => l.strip_suffix('\r').unwrap_or(l).to_owned(),
            None => line.clone(),
        };
        if !console_line(command_line, submit) {
            return;
        }
    }
}

/// Not ACE: [`command_thread`] on an interactive terminal, reading each line with the console's
/// line editor so that log lines and replies print above the prompt (see [`console`]).
pub fn editor_command_thread(submit: &dyn Fn(WorldCommand) -> bool) {
    console_banner();
    if console::run_editor(PROMPT, |command_line| console_line(command_line, submit))
        == console::EditorEnd::EndOfInput
    {
        console_input_closed();
    }
}

fn console_banner() {
    // DIVERGE: the banner and prompt name Empyrean and `empcommands` where ACE's name ACEmulator,
    // `acecommands` and `ACE >> ` (brand).
    console_write_line("");
    console_write_line("Empyrean command prompt ready.");
    console_write_line("");
    console_write_line("Type \"empcommands\" for help.");
    console_write_line("");
}

/// The end of the console's input: the prompt is disabled and the thread ends.
fn console_input_closed() {
    // DIVERGE: names Empyrean where ACE's names ACEmulator (brand).
    let message = format!(
        "{} command prompt disabled - console input stream was closed",
        empyrean_common::brand::PRODUCT
    );
    if !captured(&message) {
        log::info!("{message}");
    }
}

/// One console line without its terminator: skipped when blank, else parsed here and sent to the
/// world thread. False when the console thread ends (a parse failure, or the world thread gone).
fn console_line(command_line: String, submit: &dyn Fn(WorldCommand) -> bool) -> bool {
    if command_line.chars().all(char::is_whitespace) {
        return true;
    }

    let (command, parameters) = match parse_command(&command_line) {
        Ok(parsed) => parsed,
        Err(ex) => {
            log::error!("Exception while parsing command: {command_line} ({ex})");
            return false;
        }
    };
    submit(Box::new(move |w: &mut World| {
        run_console_command(w, &command_line, command, parameters)
    }))
}

/// The rest of one console line (`CommandThread`), on the world thread:
/// `GetCommandHandler(null, ...)` and, when `Ok`, the invoke.
pub fn run_console_command(
    w: &mut World,
    command_line: &str,
    command: Option<String>,
    parameters: Option<Vec<String>>,
) {
    let got = catch_unwind(AssertUnwindSafe(|| {
        get_command_handler(w, None, command.as_deref(), parameters.as_deref())
    }));
    match got {
        Ok(Ok((CommandHandlerResponse::Ok, Some(command_handler)))) => {
            let mut parameters = parameters.unwrap_or_default();
            let invoked = catch_unwind(AssertUnwindSafe(|| {
                if command_handler.attribute.include_raw {
                    parameters = stuff_raw_into_parameters(
                        command_line,
                        command.as_deref().unwrap_or(""),
                        &parameters,
                    );
                }
                // Add command to world manager's main thread...
                (command_handler.handler)(w, None, &parameters);
            }));
            if invoked.is_err() {
                log::error!("Exception while invoking command handler for: {command_line}");
            }
        }
        Ok(_) => {}
        Err(_) => log::error!("Exception while getting command handler for: {command_line}"),
    }
}

// ACE: CommandManager.StuffRawIntoParameters
/// The raw line minus the first (ordinal) occurrence of `command`, trimmed at the start, followed
/// by `parameters`.
#[must_use]
pub fn stuff_raw_into_parameters(raw: &str, command: &str, parameters: &[String]) -> Vec<String> {
    let mut parameters_rehash = Vec::with_capacity(parameters.len() + 1);
    // new Regex(Regex.Escape(command)).Replace(raw, "", 1): the escaped command matches itself.
    let replaced = match raw.find(command) {
        Some(i) => format!("{}{}", &raw[..i], &raw[i + command.len()..]),
        None => raw.to_owned(),
    };
    let new_cmd_line = replaced.trim_start().to_owned();
    parameters_rehash.push(new_cmd_line);
    parameters_rehash.extend(parameters.iter().cloned());
    parameters_rehash
}

/// The exception `ParseCommand` throws on a line of spaces (`commandSplit[0]` on an empty array).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexOutOfRangeException;

impl std::fmt::Display for IndexOutOfRangeException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("System.IndexOutOfRangeException: Index was outside the bounds of the array.")
    }
}

/// `(command, parameters)`: both `None` for `"/"` and `""` (C#'s nulls).
pub type ParsedCommand = (Option<String>, Option<Vec<String>>);

// ACE: CommandManager.ParseCommand
/// # Errors
/// A line of spaces only: ACE indexes an empty split.
pub fn parse_command(command_line: &str) -> Result<ParsedCommand, IndexOutOfRangeException> {
    if command_line == "/" || command_line.is_empty() {
        return Ok((None, None));
    }
    let command_split: Vec<&str> = command_line.split(' ').filter(|s| !s.is_empty()).collect();
    let mut command = (*command_split.first().ok_or(IndexOutOfRangeException)?).to_owned();

    // remove leading '/' or '@' if erroneously entered in console
    if culture_starts_with(&command, '/') || culture_starts_with(&command, '@') {
        command = substring_1(&command);
    }

    let mut parameters: Vec<String> = command_split[1..].iter().map(|s| (*s).to_owned()).collect();

    if command_line.contains('"') {
        let mut list_parameters: Vec<String> = Vec::new();

        let mut start = 0;
        while start < parameters.len() {
            if !culture_starts_with(&parameters[start], '"')
                || culture_ends_with(&parameters[start], '"')
            {
                // Make sure we catch parameters like: "someParam"
                list_parameters.push(parameters[start].replace('"', ""));
            } else {
                list_parameters.push(parameters[start].replace('"', ""));
                let mut end = start + 1;
                while end < parameters.len() {
                    let last = list_parameters.len() - 1;
                    if culture_ends_with(&parameters[end], '"') {
                        list_parameters[last] = format!(
                            "{} {}",
                            list_parameters[last],
                            parameters[end].replace('"', "")
                        );
                        break;
                    }
                    list_parameters[last] =
                        format!("{} {}", list_parameters[last], parameters[end]);
                    end += 1;
                }
                // Not ACE's (a fix): an opening quote that is never
                // closed takes the rest of the line once ("x \"a b" gives "a b"); ACE left `start`
                // at the quote, so the words after it were added again on their own ("a b", "b").
                start = end;
            }
            start += 1;
        }
        parameters = list_parameters;
    }

    Ok((Some(command), Some(parameters)))
}

/// `s.Substring(1)` (the prefixes stripped here are one UTF-16 unit).
fn substring_1(s: &str) -> String {
    let mut chars = s.chars();
    chars.next();
    chars.as_str().to_owned()
}

/// Characters the en-US culture comparison ignores (ICU's completely ignorable code points that a
/// chat line can hold): `"a\u{AD}".EndsWith("a")` is true under ICU.
pub(crate) fn is_culture_ignorable(c: char) -> bool {
    matches!(c, '\u{0}'..='\u{8}' | '\u{E}'..='\u{1F}' | '\u{7F}'..='\u{9F}' | '\u{AD}' | '\u{200B}'..='\u{200F}' | '\u{2060}'..='\u{2064}' | '\u{FEFF}')
}

/// `s.StartsWith("<c>")` with the current culture (en-US).
pub(crate) fn culture_starts_with(s: &str, c: char) -> bool {
    s.chars().find(|&x| !is_culture_ignorable(x)) == Some(c)
}

/// `s.EndsWith("<c>")` with the current culture (en-US).
pub(crate) fn culture_ends_with(s: &str, c: char) -> bool {
    s.chars().rev().find(|&x| !is_culture_ignorable(x)) == Some(c)
}

/// The `NullReferenceException` `GetCommandHandler` throws for a session without a player.
#[derive(Debug, Clone)]
pub struct NullReferenceException {
    /// What the `out commandInfo` held when it threw.
    pub command_info: Option<CommandHandlerInfo>,
}

/// `GetCommandHandler`'s return and its `out commandInfo`.
pub type CommandLookup = (CommandHandlerResponse, Option<CommandHandlerInfo>);

// ACE: CommandManager.GetCommandHandler
/// # Errors
/// A session whose `Player` is null (ACE reads `session.Player.IsAdvocate`).
pub fn get_command_handler(
    w: &World,
    session: Option<SessionId>,
    command: Option<&str>,
    parameters: Option<&[String]>,
) -> Result<CommandLookup, NullReferenceException> {
    let (Some(mut command), Some(mut parameters)) = (command, parameters) else {
        return Ok((CommandHandlerResponse::InvalidCommand, None));
    };
    let mut is_sud_oauthorized = false;
    let sudo_parameters: Vec<String>;

    if command.to_lowercase() == "sudo" {
        let mut sudo_command = "";
        if !parameters.is_empty() {
            sudo_command = &parameters[0];
        }

        let Some(info) = command_handlers().get(&key(sudo_command)).cloned() else {
            return Ok((CommandHandlerResponse::InvalidCommand, None));
        };

        let Some(session) = session else {
            console_write_line("SUDO does not work on the console because you already have full access. Remove SUDO from command and execute again.");
            return Ok((CommandHandlerResponse::InvalidCommand, Some(info)));
        };

        if info.attribute.access <= session_access_level(w, session) {
            is_sud_oauthorized = true;
        }

        if is_sud_oauthorized {
            command = sudo_command;
            sudo_parameters = parameters[1..].to_vec();
            parameters = &sudo_parameters;
        }
    }

    let Some(info) = command_handlers().get(&key(command)).cloned() else {
        // Provide some feedback for why the console command failed
        if session.is_none() {
            console_write_line("Invalid Command");
        }

        return Ok((CommandHandlerResponse::InvalidCommand, None));
    };

    if (info.attribute.flags & CommandHandlerFlag::ConsoleInvoke).0 != 0 && session.is_some() {
        return Ok((CommandHandlerResponse::NoConsoleInvoke, Some(info)));
    }

    if let Some(session) = session {
        let Some(player) = w.sessions.player(session).and_then(|g| w.objects.get(g)) else {
            return Err(NullReferenceException {
                command_info: Some(info),
            });
        };
        let is_advocate = player.is_advocate();
        let is_sentinel = player.is_sentinel_prop();
        let is_envoy = player.is_envoy();
        let is_arch = player.is_arch();
        let is_admin = player.is_admin_prop();

        let access = info.attribute.access;
        if access == AccessLevel::Advocate
            && !(is_advocate
                || is_sentinel
                || is_envoy
                || is_arch
                || is_admin
                || is_sud_oauthorized)
            || access == AccessLevel::Sentinel
                && !(is_sentinel || is_envoy || is_arch || is_admin || is_sud_oauthorized)
            || access == AccessLevel::Envoy
                && !(is_envoy || is_arch || is_admin || is_sud_oauthorized)
            || access == AccessLevel::Developer && !(is_arch || is_admin || is_sud_oauthorized)
            || access == AccessLevel::Admin && !(is_admin || is_sud_oauthorized)
        {
            return Ok((CommandHandlerResponse::NotAuthorized, Some(info)));
        }
    }

    let parameter_count = info.attribute.parameter_count;
    if parameter_count != -1
        && i64::try_from(parameters.len()).unwrap_or(i64::MAX) < i64::from(parameter_count)
    {
        // Provide some feedback for why the console command failed
        if session.is_none() {
            console_write_line(&format!(
                "The syntax of the command is incorrect.\nUsage: {} {}",
                info.attribute.command, info.attribute.usage
            ));
        }
        return Ok((CommandHandlerResponse::InvalidParameterCount, Some(info)));
    }

    if (info.attribute.flags & CommandHandlerFlag::RequiresWorld).0 != 0 {
        let in_world = session
            .and_then(|s| w.sessions.player(s))
            .and_then(|g| w.objects.get(g))
            .is_some_and(|p| p.current_landblock.is_some());
        if !in_world {
            return Ok((CommandHandlerResponse::NotInWorld, Some(info)));
        }
    }

    if is_sud_oauthorized {
        return Ok((CommandHandlerResponse::SudoOk, Some(info)));
    }

    Ok((CommandHandlerResponse::Ok, Some(info)))
}

/// `session.AccessLevel` (the game half's copy).
fn session_access_level(w: &World, session: SessionId) -> AccessLevel {
    w.sessions
        .get(session)
        .map_or(AccessLevel::Player, |s| s.access_level)
}

// ACE: GameActionTalk.Handle
/// The `@` branch of `GameActionTalk.Handle`; `message` still has its `@`. Installed into
/// empyrean-world's Talk handler by [`initialize`].
pub fn handle_talk_command(w: &mut World, session: SessionId, message: &str) {
    let command_raw = substring_1(message);
    let mut response = CommandHandlerResponse::InvalidCommand;
    let mut command_handler: Option<CommandHandlerInfo> = None;

    let (command, parameters) = match parse_command(&command_raw) {
        Ok(parsed) => parsed,
        Err(ex) => {
            log::error!("Exception while parsing command: {command_raw} ({ex})");
            return;
        }
    };

    let got = catch_unwind(AssertUnwindSafe(|| {
        get_command_handler(w, Some(session), command.as_deref(), parameters.as_deref())
    }));
    match got {
        Ok(Ok((r, info))) => {
            response = r;
            command_handler = info;
        }
        Ok(Err(e)) => {
            command_handler = e.command_info;
            log::error!("Exception while getting command handler for: {command_raw}");
        }
        Err(_) => log::error!("Exception while getting command handler for: {command_raw}"),
    }

    let mut parameters = parameters.unwrap_or_default();
    match (response, command_handler) {
        (CommandHandlerResponse::Ok, Some(command_handler)) => {
            let invoked = catch_unwind(AssertUnwindSafe(|| {
                if command_handler.attribute.include_raw {
                    parameters = stuff_raw_into_parameters(
                        &command_raw,
                        command.as_deref().unwrap_or(""),
                        &parameters,
                    );
                }
                (command_handler.handler)(w, Some(session), &parameters);
            }));
            if invoked.is_err() {
                log::error!("Exception while invoking command handler for: {command_raw}");
            }
        }
        (CommandHandlerResponse::SudoOk, Some(command_handler)) => {
            let mut sudo_parameters: Vec<String> = parameters.get(1..).unwrap_or_default().to_vec();
            let invoked = catch_unwind(AssertUnwindSafe(|| {
                // Not ACE's (a fix): a sudo'd IncludeRaw command gets its
                // raw line (the line after `sudo` and the command's own name) ahead of its
                // parameters, as it does when typed directly; ACE stuffed the raw line into a
                // list it then did not pass, so the handler never got it.
                if command_handler.attribute.include_raw {
                    let after_sudo = stuff_raw_into_parameters(
                        &command_raw,
                        command.as_deref().unwrap_or(""),
                        &[],
                    )
                    .remove(0);
                    let sudo_command = parameters.first().map_or("", String::as_str);
                    sudo_parameters =
                        stuff_raw_into_parameters(&after_sudo, sudo_command, &sudo_parameters);
                }
                (command_handler.handler)(w, Some(session), &sudo_parameters);
            }));
            if invoked.is_err() {
                log::error!("Exception while invoking command handler for: {command_raw}");
            }
        }
        (CommandHandlerResponse::InvalidCommand, _) => {
            let msg = game_message_system_chat(
                &format!("Unknown command: {}", command.as_deref().unwrap_or("")),
                ChatMessageType::Help,
            );
            enqueue_send(w, session, msg);
        }
        (CommandHandlerResponse::InvalidParameterCount, Some(command_handler)) => {
            let a = &command_handler.attribute;
            let msg = game_message_system_chat(
                &format!(
                    "Invalid parameter count, got {}, expected {}!",
                    parameters.len(),
                    a.parameter_count
                ),
                ChatMessageType::Help,
            );
            enqueue_send(w, session, msg);
            let msg = game_message_system_chat(
                &format!("@{} - {}", a.command, a.description),
                ChatMessageType::Broadcast,
            );
            enqueue_send(w, session, msg);
            let msg = game_message_system_chat(
                &format!("Usage: @{} {}", a.command, a.usage),
                ChatMessageType::Broadcast,
            );
            enqueue_send(w, session, msg);
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------------------------
// The console
// ---------------------------------------------------------------------------------------------

thread_local! {
    static CAPTURE: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
}

/// Starts capturing this thread's console output (tests).
pub fn start_console_capture() {
    CAPTURE.with(|c| *c.borrow_mut() = Some(Vec::new()));
}

/// Stops capturing and returns this thread's console output since [`start_console_capture`]:
/// each `Console.WriteLine` and console `log` line, without its line terminator.
pub fn take_console_output() -> Vec<String> {
    CAPTURE.with(|c| c.borrow_mut().take().unwrap_or_default())
}

fn captured(line: &str) -> bool {
    CAPTURE.with(|c| {
        if let Some(lines) = c.borrow_mut().as_mut() {
            lines.push(line.to_owned());
            true
        } else {
            false
        }
    })
}

/// `Console.WriteLine(value)` (on the console, above the prompt while the line editor runs).
pub fn console_write_line(value: &str) {
    if !captured(value) {
        console::write_line(value);
    }
}

/// `Console.Write(value)` (the prompt; never captured).
pub fn console_write(value: &str) {
    if CAPTURE.with(|c| c.borrow().is_none()) {
        console::write(value);
    }
}

/// A console handler's `log.Info(output)`: the reply to a console command.
///
/// DIVERGE (arch): ACE logs it at Info and its console log appender shows it, so a log level
/// above Info would hide every console command's reply. Here the reply is written to the console
/// itself, as [`console_write_line`], whatever the log level, and is not also logged.
pub fn console_log_info(output: &str) {
    console_write_line(output);
}

/// A console handler's `log.Debug(output)`.
pub fn console_log_debug(output: &str) {
    captured(output);
    log::debug!("{output}");
}

/// A console handler's `log.Error(output)`.
pub fn console_log_error(output: &str) {
    captured(output);
    log::error!("{output}");
}
