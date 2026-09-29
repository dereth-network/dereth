// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Command/CommandHandlerAttribute.cs
//! Port of `Source/ACE.Server/Command/CommandHandlerAttribute.cs`.
//!
//! C# attributes become data: each handler file lists its `[CommandHandler(...)]` decorations as
//! values of this type (see `command_manager::initialize`).

use empyrean_entity::enums::AccessLevel;

use crate::command_handler_flag::CommandHandlerFlag;

// ACE: CommandHandlerAttribute
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandHandlerAttribute {
    // ACE: CommandHandlerAttribute.Command
    pub command: String,
    // ACE: CommandHandlerAttribute.Access
    pub access: AccessLevel,
    // ACE: CommandHandlerAttribute.Flags
    pub flags: CommandHandlerFlag,
    // ACE: CommandHandlerAttribute.ParameterCount
    pub parameter_count: i32,
    // ACE: CommandHandlerAttribute.Description
    pub description: String,
    // ACE: CommandHandlerAttribute.Usage
    pub usage: String,
    // ACE: CommandHandlerAttribute.IncludeRaw
    /// include the raw, unparsed command minus the command name as the first parameter
    pub include_raw: bool,
}

impl CommandHandlerAttribute {
    // ACE: CommandHandlerAttribute.CommandHandlerAttribute
    /// The full constructor; C#'s shorter overloads are this one with their defaults
    /// (`flags = None`, `includeRaw = false`, `parameterCount = -1`, `description = ""`,
    /// `usage = ""`).
    #[must_use]
    pub fn new(
        command: &str,
        access: AccessLevel,
        flags: CommandHandlerFlag,
        include_raw: bool,
        parameter_count: i32,
        description: &str,
        usage: &str,
    ) -> Self {
        Self {
            command: command.to_owned(),
            access,
            flags,
            parameter_count,
            description: description.to_owned(),
            usage: usage.to_owned(),
            include_raw,
        }
    }

    // ACE: CommandHandlerAttribute.CommandHandlerAttribute
    /// `(command, access, flags, parameterCount, description, usage)`.
    #[must_use]
    pub fn with_count(
        command: &str,
        access: AccessLevel,
        flags: CommandHandlerFlag,
        parameter_count: i32,
        description: &str,
        usage: &str,
    ) -> Self {
        Self::new(
            command,
            access,
            flags,
            false,
            parameter_count,
            description,
            usage,
        )
    }

    // ACE: CommandHandlerAttribute.CommandHandlerAttribute
    /// `(command, access, flags, description, usage)`: `ParameterCount = -1`.
    #[must_use]
    pub fn with_description(
        command: &str,
        access: AccessLevel,
        flags: CommandHandlerFlag,
        description: &str,
        usage: &str,
    ) -> Self {
        Self::new(command, access, flags, false, -1, description, usage)
    }
}
