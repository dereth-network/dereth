// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Command/Handlers/AccountCommands.cs
//! Port of `Source/ACE.Server/Command/Handlers/AccountCommands.cs`.
//!
//! `DatabaseManager.Authentication` is `World.auth`; `ConfigManager.Config.Server.Accounts` is the
//! authentication database's copy of it (`accounts_config`), as the login path reads it.

use std::net::{IpAddr, Ipv4Addr};

use empyrean_common::dotnet::{to_string, CsCast, TimeSpan};
use empyrean_entity::enums::{AccessLevel, ChatMessageType, PropertyString};
use empyrean_net::SessionId;
use empyrean_world::World;

use crate::command_handler_attribute::CommandHandlerAttribute;
use crate::command_handler_flag::CommandHandlerFlag;
use crate::command_handler_info::{CommandHandlerInfo, NamedHandler};
use crate::command_manager::console_write_line;
use crate::handler;
use crate::handlers::admin_commands::try_parse_access_level;
use crate::handlers::command_handler_helper;

const ACCESS_LEVEL_USAGE: &str = concat!(
    "accesslevel can be a number or enum name\n",
    "0 = Player | 1 = Advocate | 2 = Sentinel | 3 = Envoy | 4 = Developer | 5 = Admin",
);

/// This file's `[CommandHandler]` decorations, in declaration order.
#[must_use]
pub fn command_handlers() -> Vec<CommandHandlerInfo> {
    let admin = AccessLevel::Admin;
    let none = CommandHandlerFlag::None;
    let rows: [(CommandHandlerAttribute, NamedHandler); 5] = [
        (
            CommandHandlerAttribute::with_count(
                "accountcreate",
                admin,
                none,
                2,
                "Creates a new account.",
                &format!("username password (accesslevel)\n{ACCESS_LEVEL_USAGE}"),
            ),
            handler!(handle_account_create),
        ),
        (
            CommandHandlerAttribute::with_count(
                "accountget",
                admin,
                CommandHandlerFlag::ConsoleInvoke,
                1,
                "Gets an account.",
                "username",
            ),
            handler!(handle_account_get),
        ),
        (
            CommandHandlerAttribute::with_count(
                "set-accountaccess",
                admin,
                none,
                1,
                "Change the access level of an account.",
                &format!("accountname (accesslevel)\n{ACCESS_LEVEL_USAGE}"),
            ),
            handler!(handle_account_update_access_level),
        ),
        (
            CommandHandlerAttribute::with_count(
                "set-accountpassword",
                admin,
                none,
                2,
                "Set the account password.",
                "accountname newpassword\n",
            ),
            handler!(handle_account_set_password),
        ),
        (
            CommandHandlerAttribute::with_count(
                "passwd",
                AccessLevel::Player,
                CommandHandlerFlag::RequiresWorld,
                2,
                "Change your account password.",
                "oldpassword newpassword\n",
            ),
            handler!(handle_passwd),
        ),
    ];
    rows.into_iter()
        .map(|(attribute, (handler, handler_name))| CommandHandlerInfo {
            handler,
            handler_name,
            attribute,
        })
        .collect()
}

/// `Enum.GetName(typeof(AccessLevel), accessLevel)`: null (an empty concatenation) when undefined.
fn access_level_name(access_level: AccessLevel) -> &'static str {
    access_level.name().unwrap_or("")
}

/// ACE's `articleAorAN`.
fn article_a_or_an(access_level: AccessLevel) -> &'static str {
    if access_level == AccessLevel::Advocate
        || access_level == AccessLevel::Admin
        || access_level == AccessLevel::Envoy
    {
        "an"
    } else {
        "a"
    }
}

// ACE: AccountCommands.HandleAccountCreate
/// `accountcreate username password (accesslevel)`: creates a new account.
pub fn handle_account_create(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let mut default_access_level = AccessLevel(
        w.auth
            .lock()
            .accounts_config()
            .default_access_level
            .cs_cast(),
    );

    if !default_access_level.is_defined() {
        default_access_level = AccessLevel::Player;
    }

    let mut access_level = default_access_level;

    if parameters.len() > 2 {
        // Enum.TryParse's out value is default(AccessLevel), Player, when it fails
        match try_parse_access_level(&parameters[2], true) {
            Some(parsed) => {
                access_level = if parsed.is_defined() {
                    parsed
                } else {
                    default_access_level
                }
            }
            None => access_level = AccessLevel::Player,
        }
    }

    let article_a_or_an = article_a_or_an(access_level);

    let message;

    let account_exists = w.auth.lock().get_account_by_name(&parameters[0]);

    if account_exists.is_some() {
        message = "Account already exists. Try a new name.".to_owned();
    } else {
        let created = w.auth.lock().create_account(
            &parameters[0].to_lowercase(),
            &parameters[1],
            access_level,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        );
        match created {
            Ok(account) => {
                if w.auth.auto_promote_next_account_to_admin() && access_level == AccessLevel::Admin
                {
                    w.auth.set_auto_promote_next_account_to_admin(false);
                }

                message = format!(
                    "Account successfully created for {} ({}) with access rights as {article_a_or_an} {}.",
                    account.account_name,
                    account.account_id,
                    access_level_name(access_level)
                );
            }
            Err(_) => message = "Account already exists. Try a new name.".to_owned(),
        }
    }

    command_handler_helper::write_output_info(
        w,
        session,
        &message,
        ChatMessageType::WorldBroadcast,
    );
}

// ACE: AccountCommands.HandleAccountGet
/// `accountget username`: gets an account (console only).
///
/// # Panics
/// An unknown account (ACE's `NullReferenceException`; the command manager logs it).
pub fn handle_account_get(w: &mut World, _session: Option<SessionId>, parameters: &[String]) {
    let account = w
        .auth
        .lock()
        .get_account_by_name(&parameters[0])
        .expect("NullReferenceException: account");
    console_write_line(&format!(
        "User: {}, ID: {}",
        account.account_name, account.account_id
    ));
}

// ACE: AccountCommands.HandleAccountUpdateAccessLevel
/// `set-accountaccess accountname (accesslevel)`: change the access level of an account.
pub fn handle_account_update_access_level(
    w: &mut World,
    session: Option<SessionId>,
    parameters: &[String],
) {
    let account_name = parameters[0].to_lowercase();

    let account_id = w.auth.lock().get_account_id_by_name(&account_name);

    if account_id == 0 {
        command_handler_helper::write_output_info(
            w,
            session,
            &format!("Account {account_name} does not exist."),
            ChatMessageType::Broadcast,
        );
        return;
    }

    let mut access_level = AccessLevel::Player;

    if parameters.len() > 1 {
        match try_parse_access_level(&parameters[1], true) {
            Some(parsed) => {
                access_level = if parsed.is_defined() {
                    parsed
                } else {
                    AccessLevel::Player
                }
            }
            None => access_level = AccessLevel::Player,
        }
    }

    let article_a_or_an = article_a_or_an(access_level);

    if account_id == 0 {
        command_handler_helper::write_output_info(
            w,
            session,
            &format!("Account {account_name} does not exist."),
            ChatMessageType::Broadcast,
        );
        return;
    }

    w.auth
        .lock()
        .update_account_access_level(account_id, access_level);

    if w.auth.auto_promote_next_account_to_admin() && access_level == AccessLevel::Admin {
        w.auth.set_auto_promote_next_account_to_admin(false);
    }

    command_handler_helper::write_output_info(
        w,
        session,
        &format!(
            "Account {account_name} updated with access rights set as {article_a_or_an} {}.",
            access_level_name(access_level)
        ),
        ChatMessageType::Broadcast,
    );
}

// ACE: AccountCommands.HandleAccountSetPassword
/// `set-accountpassword accountname newpassword`: set the account password.
pub fn handle_account_set_password(
    w: &mut World,
    session: Option<SessionId>,
    parameters: &[String],
) {
    let account_name = parameters[0].to_lowercase();

    let account = w.auth.lock().get_account_by_name(&account_name);

    let Some(mut account) = account else {
        command_handler_helper::write_output_info(
            w,
            session,
            &format!("Account {account_name} does not exist."),
            ChatMessageType::Broadcast,
        );
        return;
    };

    if parameters.is_empty() {
        command_handler_helper::write_output_info(
            w,
            session,
            "You must specify a password for the account.",
            ChatMessageType::Broadcast,
        );
        return;
    }

    {
        let mut auth = w.auth.lock();
        let config = auth.accounts_config().clone();
        account.set_password(&parameters[1], &config);
        account.set_salt_for_bcrypt();

        auth.update_account(&account);
    }

    command_handler_helper::write_output_info(
        w,
        session,
        &format!("Account password for {account_name} successfully changed."),
        ChatMessageType::Broadcast,
    );
}

/// Rate limiter for /passwd command (`PasswdInterval`, a static field).
fn passwd_interval() -> TimeSpan {
    TimeSpan::from_seconds(5.0)
}

// ACE: AccountCommands.HandlePasswd
/// `passwd oldpassword newpassword`: change your account password.
pub fn handle_passwd(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let Some(s) = session else {
        command_handler_helper::write_output_info(
            w,
            session,
            "This command is run from ingame client only",
            ChatMessageType::Broadcast,
        );
        return;
    };

    let player_name = w
        .sessions
        .player(s)
        .and_then(|p| w.objects.get(p))
        .and_then(|p| p.get_property(PropertyString::Name))
        .unwrap_or_default();
    log::debug!("{player_name} is changing their password");

    let current_time = w.now.utc;

    let last_pass_time = w
        .sessions
        .get(s)
        .expect("NullReferenceException: session")
        .last_pass_time;
    if current_time - last_pass_time < passwd_interval() {
        command_handler_helper::write_output_info(
            w,
            session,
            &format!(
                "This command may only be run once every {} seconds.",
                to_string(passwd_interval().total_seconds())
            ),
            ChatMessageType::Broadcast,
        );
        return;
    }
    if let Some(data) = w.sessions.get_mut(s) {
        data.last_pass_time = current_time;
    }

    if parameters.is_empty() {
        command_handler_helper::write_output_info(
            w,
            session,
            "You must specify the current password for the account.",
            ChatMessageType::Broadcast,
        );
        return;
    }

    // (dead: the parameter count, and the check above, keep this from running)
    if parameters.is_empty() {
        command_handler_helper::write_output_info(
            w,
            session,
            "You must specify a new password for the account.",
            ChatMessageType::Broadcast,
        );
        return;
    }

    let (account_id, account_name) = w
        .sessions
        .get(s)
        .map(|d| (d.account_id, d.account.clone().unwrap_or_default()))
        .unwrap_or_default();
    let account = w.auth.lock().get_account_by_id(account_id);

    let Some(mut account) = account else {
        command_handler_helper::write_output_info(
            w,
            session,
            &format!("Account {account_name} ({account_id}) wasn't found in the database! How are you in world without a valid account?"),
            ChatMessageType::Broadcast,
        );
        return;
    };

    let oldpassword = &parameters[0];
    let newpassword = &parameters[1];

    let matched = {
        let mut auth = w.auth.lock();
        if account.password_matches(oldpassword, &mut **auth) {
            let config = auth.accounts_config().clone();
            account.set_password(newpassword, &config);
            account.set_salt_for_bcrypt();
            true
        } else {
            false
        }
    };
    if !matched {
        command_handler_helper::write_output_info(
            w,
            session,
            "Unable to change password: Password provided in first parameter does not match current account password for this account!",
            ChatMessageType::Broadcast,
        );
        return;
    }

    w.auth.lock().update_account(&account);

    command_handler_helper::write_output_info(
        w,
        session,
        "Account password successfully changed.",
        ChatMessageType::Broadcast,
    );
}
