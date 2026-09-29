// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Command/CommandParameterHelpers.cs
//! Port of `Source/ACE.Server/Command/CommandParameterHelpers.cs`.
//!
//! Command handler sanity preserving parameter parsing.
//!
//! - ACE matches each parameter with a .NET regular expression anchored at the end (`$`, which
//!   also matches before a final `\n`). Each pattern is hand-matched here with the same
//!   leftmost-first result; the `commands/resolve_ace_parameters` vectors pin them. `\d` is ASCII
//!   here: .NET's `\d` also takes other Unicode decimal digits, which the client's CP-1252 chat
//!   cannot send, and which `long.TryParse` would reject anyway.
//! - The .NET `TryParse` calls (`long`, `ulong`, `uint`, `int`, `float`, `double`) are ported
//!   with their default styles and the en-US culture ([`dotnet_parse`]; vectors
//!   `commands/dotnet_try_parse`).
//! - `Value` is `object` in C#; here it is [`AceParamValue`]. `PossibleValues` (an enum `Type`)
//!   is the enum's `Enum.GetValues` list ([`EnumValues`]).

use empyrean_entity::enums::{AceEnum, ChatMessageType};
use empyrean_entity::numerics::Vector2;
use empyrean_entity::{ObjectGuid, Position};
use empyrean_net::SessionId;
use empyrean_world::entity::position_extensions;
use empyrean_world::managers::player_manager;
use empyrean_world::World;

use crate::command_manager::{console_write_line, culture_ends_with};
use crate::handlers::command_handler_helper::send_server_message;

// ACE: CommandParameterHelpers.ACECommandParameterType
/// Types of parameter values
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ACECommandParameterType {
    /// don't use this one
    #[default]
    Invalid,
    /// normal coordinates, example: 37.3s,67w; output is of type ACE.Entity.Position
    Location,
    /// a character name of an online player; output is a Player; must be the first parameter
    OnlinePlayerName,
    /// a character name of an online player or a decimal iid; must be the first parameter
    OnlinePlayerNameOrIid,
    /// a decimal or hexadecimal iid of an online player, examples: 1342177292 or 0x5000000C
    OnlinePlayerIid,
    /// a character name; output is of type string; must be the first parameter
    PlayerName,
    /// a url, example: `http://someserver.net:4321/?get=blah`; output is of type System.Uri
    Uri,
    /// a number, example: 01231242; output is of type ulong: 1231242
    ULong,
    /// a number, example: -01231242; output is of type long: -1231242
    Long,
    /// a number, example: 01231242; output is of type long: 1231242
    PositiveLong,
    /// some text enclosed in double quotes (needs IncludeRaw and rawIncluded)
    DoubleQuoteEnclosedText,
    /// some text preceeded by a comma (needs IncludeRaw and rawIncluded); limit 1 per command
    CommaPrefixedText,
    /// a word: one or more of case insensitive a-z, 1-9, and underscore
    SimpleWord,
    /// an element of an enum (set PossibleValues)
    Enum,
}

/// A parameter's `Value` (C# `object`).
#[derive(Debug, Clone)]
pub enum AceParamValue {
    Position(Position),
    Player(ObjectGuid),
    ULong(u64),
    Long(i64),
    Int(i32),
    String(String),
    /// `System.Uri`, kept as its original string (see [`uri_new`]).
    Uri(String),
    /// An enum member: its `ToString()` and its value.
    Enum(String, i64),
}

/// `Enum.GetValues(type)`, each with its `ToString()`, in .NET order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumValues(pub Vec<(String, i64)>);

impl EnumValues {
    /// The members of `E`.
    #[must_use]
    pub fn of<E: AceEnum>() -> Self {
        Self(
            E::MEMBERS
                .iter()
                .map(|m| (m.to_dotnet_string(), m.key().cast_signed()))
                .collect(),
        )
    }
}

// ACE: CommandParameterHelpers.ACECommandParameter
/// A player supplied parameter
#[derive(Debug, Clone)]
pub struct ACECommandParameter {
    // ACE: CommandParameterHelpers.ACECommandParameter.Type
    /// The type of the parameter value
    pub r#type: ACECommandParameterType,
    // ACE: CommandParameterHelpers.ACECommandParameter.DefaultValue
    /// The value to use upon missing or invalid supplied parameter
    pub default_value: Option<AceParamValue>,
    // ACE: CommandParameterHelpers.ACECommandParameter.Required
    /// If this parameter is required
    pub required: bool,
    // ACE: CommandParameterHelpers.ACECommandParameter.Value
    /// The resultant parsed Value (or the default value)
    pub value: Option<AceParamValue>,
    // ACE: CommandParameterHelpers.ACECommandParameter.Defaulted
    /// The parameter either wasn't supplied or was invalid (doesn't parse, player doesn't exist, etc.)
    pub defaulted: bool,
    // ACE: CommandParameterHelpers.ACECommandParameter.ErrorMessage
    /// The broadcast message to send to the session when the parameter is required and didn't parse or wasn't supplied.
    pub error_message: Option<String>,
    // ACE: CommandParameterHelpers.ACECommandParameter.ParameterNo
    /// Automatically assigned during GetParameters procedure
    pub parameter_no: i32,
    // ACE: CommandParameterHelpers.ACECommandParameter.PossibleValues
    /// if the type is enum set this to the enum that the parameter should be part of
    pub possible_values: Option<EnumValues>,
}

impl Default for ACECommandParameter {
    fn default() -> Self {
        Self {
            r#type: ACECommandParameterType::Invalid,
            default_value: None,
            required: false,
            value: None,
            defaulted: true,
            error_message: None,
            parameter_no: -1,
            possible_values: None,
        }
    }
}

impl ACECommandParameter {
    // ACE: CommandParameterHelpers.ACECommandParameter.AsPosition
    /// # Panics
    /// When the value is not a position (C#'s `InvalidCastException` / `NullReferenceException`).
    #[must_use]
    pub fn as_position(&self) -> &Position {
        match &self.value {
            Some(AceParamValue::Position(p)) => p,
            v => panic!("InvalidCastException: AsPosition on {v:?}"),
        }
    }

    // ACE: CommandParameterHelpers.ACECommandParameter.AsPlayer
    /// `(Player)Value`: `None` for a null value.
    ///
    /// # Panics
    /// When the value is not a player (`InvalidCastException`).
    #[must_use]
    pub fn as_player(&self) -> Option<ObjectGuid> {
        match &self.value {
            None => None,
            Some(AceParamValue::Player(p)) => Some(*p),
            Some(v) => panic!("InvalidCastException: AsPlayer on {v:?}"),
        }
    }

    // ACE: CommandParameterHelpers.ACECommandParameter.AsULong
    /// # Panics
    /// When the value is not a ulong (`InvalidCastException` / `NullReferenceException`).
    #[must_use]
    pub fn as_ulong(&self) -> u64 {
        match &self.value {
            Some(AceParamValue::ULong(v)) => *v,
            v => panic!("InvalidCastException: AsULong on {v:?}"),
        }
    }

    // ACE: CommandParameterHelpers.ACECommandParameter.AsLong
    /// # Panics
    /// When the value is not a long.
    #[must_use]
    pub fn as_long(&self) -> i64 {
        match &self.value {
            Some(AceParamValue::Long(v)) => *v,
            v => panic!("InvalidCastException: AsLong on {v:?}"),
        }
    }

    // ACE: CommandParameterHelpers.ACECommandParameter.AsInt
    /// `(int)Value` widened to long.
    ///
    /// # Panics
    /// When the value is not an int.
    #[must_use]
    pub fn as_int(&self) -> i64 {
        match &self.value {
            Some(AceParamValue::Int(v)) => i64::from(*v),
            v => panic!("InvalidCastException: AsInt on {v:?}"),
        }
    }

    // ACE: CommandParameterHelpers.ACECommandParameter.AsString
    /// `(string)Value`: `None` for a null value.
    ///
    /// # Panics
    /// When the value is not a string.
    #[must_use]
    pub fn as_string(&self) -> Option<&str> {
        match &self.value {
            None => None,
            Some(AceParamValue::String(s)) => Some(s),
            Some(v) => panic!("InvalidCastException: AsString on {v:?}"),
        }
    }

    // ACE: CommandParameterHelpers.ACECommandParameter.AsUri
    /// `(Uri)Value`: `None` for a null value.
    ///
    /// # Panics
    /// When the value is not a Uri.
    #[must_use]
    pub fn as_uri(&self) -> Option<&str> {
        match &self.value {
            None => None,
            Some(AceParamValue::Uri(s)) => Some(s),
            Some(v) => panic!("InvalidCastException: AsUri on {v:?}"),
        }
    }
}

/// Sends `message` to the session as a Broadcast, or writes it to the console.
fn report(w: &mut World, session: Option<SessionId>, message: &str) {
    if session.is_some() {
        send_server_message(w, session, message, ChatMessageType::Broadcast);
    } else {
        console_write_line(message);
    }
}

/// A C# exception thrown inside the resolver's `try` (its `catch` returns false).
struct Thrown;

// ACE: CommandParameterHelpers.ResolveACEParameters
/// Resolve the parameters supplied by the player into usable values.
///
/// `ace_parsed_parameters`: the collection of parameters supplied by the default parameter parser;
/// `parameters`: the resolution details for every parameter; `raw_included`: whether or not the
/// raw unparsed command line minus the command name was included as the first parameter. Returns
/// whether the parameters were successfully resolved.
///
/// # Panics
/// With `raw_included` and no parameters (`First()` on an empty sequence, outside ACE's `try`).
pub fn resolve_ace_parameters(
    w: &mut World,
    session: Option<SessionId>,
    ace_parsed_parameters: &[String],
    parameters: &mut [ACECommandParameter],
    raw_included: bool,
) -> bool {
    let mut parameter_blob: String = if raw_included {
        ace_parsed_parameters
            .first()
            .expect("InvalidOperationException: Sequence contains no elements")
            .clone()
    } else if !ace_parsed_parameters.is_empty() {
        ace_parsed_parameters
            .join(" ")
            .trim_matches([' ', ','])
            .to_owned()
    } else {
        String::new()
    };
    let comma_count = parameter_blob.chars().filter(|&x| x == ',').count();

    let acps_count = parameters.len();
    for i in (0..acps_count).rev() {
        let acp = &mut parameters[i];
        acp.parameter_no = i32::try_from(i + 1).unwrap_or(i32::MAX);
        if !parameter_blob.is_empty() {
            match resolve_one(
                w,
                session,
                acp,
                i,
                acps_count,
                comma_count,
                &mut parameter_blob,
            ) {
                Ok(true) => {}
                Ok(false) | Err(Thrown) => return false,
            }
        }
        let acp = &mut parameters[i];
        if acp.defaulted {
            acp.value = acp.default_value.clone();
        }

        if acp.required && acp.defaulted {
            if let Some(msg) = acp.error_message.clone().filter(|m| !m.trim().is_empty()) {
                report(w, session, &msg);
            }

            return false;
        }
    }
    true
}

/// `s.Substring(0, index).Trim(chars)`, or `""` when `index` is 0.
fn cut(blob: &str, index: usize, chars: &[char]) -> String {
    if index == 0 {
        String::new()
    } else {
        blob[..index].trim_matches(chars).to_owned()
    }
}

/// One `switch (acp.Type)` of the resolver. `Ok(false)` is a `return false`, `Err` a throw.
fn resolve_one(
    w: &mut World,
    session: Option<SessionId>,
    acp: &mut ACECommandParameter,
    i: usize,
    acps_count: usize,
    comma_count: usize,
    parameter_blob: &mut String,
) -> Result<bool, Thrown> {
    let blob = parameter_blob.clone();
    match acp.r#type {
        ACECommandParameterType::PositiveLong => {
            if let Some(start) = match_trailing_integer(&blob) {
                let Some(val) = dotnet_parse::long_try_parse(group_text(&blob, start)) else {
                    return Ok(false);
                };
                if val <= 0 {
                    return Ok(false);
                }
                acp.value = Some(AceParamValue::Long(val));
                acp.defaulted = false;
                *parameter_blob = cut(&blob, start, &[' ']);
            }
        }
        ACECommandParameterType::Long => {
            if let Some(start) = match_trailing_integer(&blob) {
                let Some(val) = dotnet_parse::long_try_parse(group_text(&blob, start)) else {
                    return Ok(false);
                };
                acp.value = Some(AceParamValue::Long(val));
                acp.defaulted = false;
                *parameter_blob = cut(&blob, start, &[' ', ',']);
            }
        }
        ACECommandParameterType::ULong => {
            if let Some(start) = match_trailing_integer(&blob) {
                let Some(val) = dotnet_parse::ulong_try_parse(group_text(&blob, start)) else {
                    return Ok(false);
                };
                acp.value = Some(AceParamValue::ULong(val));
                acp.defaulted = false;
                *parameter_blob = cut(&blob, start, &[' ', ',']);
            }
        }
        ACECommandParameterType::Location => {
            if let Some((g1, g2)) = match_location(&blob) {
                let ns = blob[g1.0..g1.1].to_owned();
                let ew = blob[g2.0..g2.1].to_owned();
                match try_parse_position(w, &[ns, ew], 0) {
                    Err(error_message) => {
                        report(w, session, &error_message);
                        return Ok(false);
                    }
                    Ok(position) => {
                        acp.value = Some(AceParamValue::Position(position));
                        acp.defaulted = false;
                        let coords_start_pos = g1.0.min(g2.0);
                        *parameter_blob = cut(&blob, coords_start_pos, &[' ', ',']);
                    }
                }
            }
        }
        ACECommandParameterType::OnlinePlayerName => {
            if i != 0 {
                // throw new Exception("Player parameter must be the first parameter, since it can contain spaces.");
                return Err(Thrown);
            }
            *parameter_blob = blob.trim_end_matches([' ', ',']).to_owned();
            match player_manager::get_online_player_by_name(w, parameter_blob) {
                None => {
                    report(
                        w,
                        session,
                        &format!("Unable to find player {parameter_blob}"),
                    );
                    return Ok(false);
                }
                Some(target_player) => {
                    acp.value = Some(AceParamValue::Player(target_player));
                    acp.defaulted = false;
                }
            }
        }
        ACECommandParameterType::OnlinePlayerNameOrIid => {
            if i != 0 {
                // throw new Exception("Player parameter must be the first parameter, since it can contain spaces.");
                return Err(Thrown);
            }

            if !blob.contains(' ') {
                if let Some(iid) = dotnet_parse::uint_try_parse(&blob) {
                    return match player_manager::get_online_player(w, iid) {
                        None => {
                            report(w, session, &format!("Unable to find player with iid {iid}"));
                            Ok(false)
                        }
                        Some(target_player2) => {
                            acp.value = Some(AceParamValue::Player(target_player2));
                            acp.defaulted = false;
                            Ok(true)
                        }
                    };
                }
            }
            match player_manager::get_online_player_by_name(w, &blob) {
                None => {
                    report(w, session, &format!("Unable to find player {blob}"));
                    return Ok(false);
                }
                Some(target_player3) => {
                    acp.value = Some(AceParamValue::Player(target_player3));
                    acp.defaulted = false;
                }
            }
        }
        ACECommandParameterType::OnlinePlayerIid => {
            if let Some((str_iid, start)) = match_iid(&blob) {
                let parsed = if let Some(hex) = str_iid.strip_prefix("0x") {
                    // Convert.ToUInt32(strIid, 16): the "0x" prefix is allowed.
                    u32::from_str_radix(hex, 16).ok()
                } else {
                    // uint.Parse: throws on overflow (and on a "0X" prefix).
                    dotnet_parse::uint_try_parse(&str_iid)
                };
                let Some(iid) = parsed else {
                    report(
                        w,
                        session,
                        &format!("Unable to parse {str_iid} into a player iid"),
                    );
                    return Ok(false);
                };

                match player_manager::get_online_player(w, iid) {
                    None => {
                        report(
                            w,
                            session,
                            &format!("Unable to find player with iid {str_iid}"),
                        );
                        return Ok(false);
                    }
                    Some(target_player2) => {
                        acp.value = Some(AceParamValue::Player(target_player2));
                        acp.defaulted = false;
                        // Not ACE's (a fix): the blob is cut where the
                        // iid starts, decimal or hex; ACE cut a hex iid at 0, consuming the whole
                        // blob, so the parameters before it were lost.
                        *parameter_blob = cut(&blob, start, &[' ', ',']);
                    }
                }
            }
        }
        ACECommandParameterType::PlayerName => {
            if i != 0 {
                // throw new Exception("Player name parameter must be the first parameter, since it can contain spaces.");
                return Err(Thrown);
            }
            *parameter_blob = blob.trim_end_matches([' ', ',']).to_owned();
            if !parameter_blob.trim().is_empty() {
                acp.value = Some(AceParamValue::String(parameter_blob.clone()));
                acp.defaulted = false;
            }
        }
        ACECommandParameterType::Uri => {
            if let Some(start) = match_uri(&blob) {
                let str_uri = group_text(&blob, start);
                let Some(url) = uri_new(str_uri) else {
                    return Ok(false);
                };
                acp.value = Some(AceParamValue::Uri(url));
                acp.defaulted = false;
                *parameter_blob = cut(&blob, start, &[' ', ',']);
            }
        }
        ACECommandParameterType::DoubleQuoteEnclosedText => {
            let trimmed = blob.trim_end();
            if let Some(start) = match_double_quoted(trimmed) {
                let txt = &trimmed[start..];
                acp.value = Some(AceParamValue::String(txt.trim_matches('"').to_owned()));
                acp.defaulted = false;
                *parameter_blob = cut(&blob, start, &[' ', ',']);
            }
        }
        ACECommandParameterType::CommaPrefixedText => {
            if i == 0 {
                // throw new Exception("this parameter type is not appropriate as the first parameter");
                return Err(Thrown);
            }
            if i == acps_count - 1 && !acp.required && comma_count < acps_count - 1 {
                return Ok(true);
            }
            let trimmed = blob.trim_end();
            if let Some(start) = match_comma_prefixed(trimmed) {
                let txt = &trimmed[start..];
                acp.value = Some(AceParamValue::String(
                    txt.trim_start_matches([' ', ',']).to_owned(),
                ));
                acp.defaulted = false;
                *parameter_blob = cut(&blob, start, &[' ', ',']);
            }
        }
        ACECommandParameterType::SimpleWord => {
            let trimmed = blob.trim_end();
            if let Some(start) = match_simple_word(trimmed) {
                let txt = &trimmed[start..];
                acp.value = Some(AceParamValue::String(
                    txt.trim_start_matches(' ').to_owned(),
                ));
                acp.defaulted = false;
                *parameter_blob = cut(&blob, start, &[' ', ',']);
            }
        }
        ACECommandParameterType::Enum => {
            let Some(possible_values) = acp.possible_values.clone() else {
                // throw new Exception("The enum parameter type must be accompanied by the PossibleValues");
                return Err(Thrown);
            };
            let trimmed = blob.trim_end();
            if let Some(start) = match_simple_word(trimmed) {
                let txt = trimmed[start..].trim_matches([' ', ',']).to_lowercase();
                for (name, value) in &possible_values.0 {
                    if name.to_lowercase() == txt {
                        acp.value = Some(AceParamValue::Enum(name.clone(), *value));
                        acp.defaulted = false;
                        *parameter_blob = cut(&blob, start, &[' ']);
                        break;
                    }
                }
            }
        }
        ACECommandParameterType::Invalid => {}
    }
    Ok(true)
}

// ---------------------------------------------------------------------------------------------
// The patterns. Each returns where its group(s) start in `s`; a group runs to `$`.
// ---------------------------------------------------------------------------------------------

/// Where `$` matches: the end, and before a final `\n`.
fn dollar_positions(s: &str) -> (usize, Option<usize>) {
    (s.len(), s.strip_suffix('\n').map(str::len))
}

fn is_dollar(s: &str, pos: usize) -> bool {
    let (end, before_nl) = dollar_positions(s);
    pos == end || before_nl == Some(pos)
}

/// The text of a group that starts at `start` and ends where the pattern's `$` matched: before a
/// final `\n` when the group cannot hold one.
fn group_text(s: &str, start: usize) -> &str {
    let end = if s.ends_with('\n') {
        s.len() - 1
    } else {
        s.len()
    };
    &s[start..end.max(start)]
}

fn run_end(s: &str, from: usize, class: impl Fn(char) -> bool) -> usize {
    s[from..]
        .char_indices()
        .find(|&(_, c)| !class(c))
        .map_or(s.len(), |(i, _)| from + i)
}

/// `(-?\d+)$`: the start of the group.
fn match_trailing_integer(s: &str) -> Option<usize> {
    for (p, c) in s.char_indices() {
        let digits_from = if c == '-' { p + 1 } else { p };
        let end = run_end(s, digits_from, |c| c.is_ascii_digit());
        // (when `-?` backtracks to empty, `\d+` at the '-' fails)
        if end > digits_from && is_dollar(s, end) {
            return Some(p);
        }
    }
    None
}

fn is_digit_or_dot(c: char) -> bool {
    c.is_ascii_digit() || c == '.'
}

/// `([\d\.]+[ns])[^\d\.]*([\d\.]+[ew])$` (IgnoreCase): the two groups' spans.
fn match_location(s: &str) -> Option<((usize, usize), (usize, usize))> {
    for (p, c) in s.char_indices() {
        if !is_digit_or_dot(c) {
            continue;
        }
        let run1 = run_end(s, p, is_digit_or_dot);
        let Some(ns) = s[run1..].chars().next() else {
            continue;
        };
        if !matches!(ns, 'n' | 'N' | 's' | 'S') {
            continue;
        }
        let g1 = (p, run1 + 1);
        let gap = run_end(s, g1.1, |c| !is_digit_or_dot(c));
        let run2 = run_end(s, gap, is_digit_or_dot);
        if run2 == gap {
            continue;
        }
        let Some(ew) = s[run2..].chars().next() else {
            continue;
        };
        if !matches!(ew, 'e' | 'E' | 'w' | 'W') {
            continue;
        }
        if is_dollar(s, run2 + 1) {
            return Some((g1, (gap, run2 + 1)));
        }
    }
    None
}

/// `(\d{10})$|(0x[0-9a-f]{8})$` (IgnoreCase): the matched text and where it starts (in whichever
/// alternative matched).
fn match_iid(s: &str) -> Option<(String, usize)> {
    for (p, _) in s.char_indices() {
        let rest = &s[p..];
        let digits = run_end(rest, 0, |c| c.is_ascii_digit()).min(10);
        if digits == 10 && is_dollar(s, p + 10) {
            return Some((rest[..10].to_owned(), p));
        }
        let bytes = rest.as_bytes();
        if bytes.len() >= 10
            && bytes[0] == b'0'
            && (bytes[1] == b'x' || bytes[1] == b'X')
            && bytes[2..10].iter().all(u8::is_ascii_hexdigit)
            && is_dollar(s, p + 10)
        {
            return Some((rest[..10].to_owned(), p));
        }
    }
    None
}

/// `(\".*\")$` on a string without trailing white space: the opening quote.
fn match_double_quoted(s: &str) -> Option<usize> {
    if !s.ends_with('"') {
        return None;
    }
    let last = s.len() - 1;
    s.char_indices()
        .find(|&(p, c)| c == '"' && p < last && !s[p + 1..last].contains('\n'))
        .map(|(p, _)| p)
}

/// `\,\s*([^,]*)$` on a string without trailing white space: the start of the group.
fn match_comma_prefixed(s: &str) -> Option<usize> {
    let comma = s.rfind(',')?;
    Some(run_end(s, comma + 1, char::is_whitespace))
}

fn is_word_class(c: char) -> bool {
    c.is_ascii_alphabetic() || matches!(c, '1'..='9' | '_')
}

/// `([a-zA-Z1-9_]+)\s*$` on a string without trailing white space: the start of the group.
fn match_simple_word(s: &str) -> Option<usize> {
    let start = s
        .char_indices()
        .rev()
        .take_while(|&(_, c)| is_word_class(c))
        .last()
        .map(|(p, _)| p)?;
    Some(start)
}

fn is_host_class(c: char) -> bool {
    c.is_ascii_alphanumeric() || "-@:%._+~#=".contains(c)
}

fn is_path_class(c: char) -> bool {
    c.is_ascii_alphanumeric() || "-@:%_+.~#?&/=".contains(c)
}

/// .NET's `\w` for the characters the URL pattern can see next to `\b`.
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// `(https?:\/\/(www\.)?[-a-zA-Z0-9@:%._\+~#=]{2,256}\.[a-z]{2,6}\b([-a-zA-Z0-9@:%_\+.~#?&//=]*))$`
/// (IgnoreCase): the start of the group.
fn match_uri(s: &str) -> Option<usize> {
    let end = if s.ends_with('\n') {
        s.len() - 1
    } else {
        s.len()
    };
    let text = &s[..end];
    (0..text.len())
        .filter(|&p| text.is_char_boundary(p))
        .find(|&p| uri_matches(&text[p..]))
}

/// Whether all of `t` matches the URL pattern (ASCII case-insensitive).
fn uri_matches(t: &str) -> bool {
    let lower = t.to_ascii_lowercase();
    let after_scheme = if lower.starts_with("https://") {
        8
    } else if lower.starts_with("http://") {
        7
    } else {
        return false;
    };
    let chars: Vec<char> = t[after_scheme..].chars().collect();
    let www = chars.len() >= 4
        && chars[..4]
            .iter()
            .collect::<String>()
            .eq_ignore_ascii_case("www.");
    for skip in if www { vec![4usize, 0] } else { vec![0] } {
        let host = &chars[skip..];
        let host_run = host
            .iter()
            .take_while(|&&c| is_host_class(c))
            .count()
            .min(256);
        for n1 in (2..=host_run).rev() {
            if host.get(n1) != Some(&'.') {
                continue;
            }
            let tld = &host[n1 + 1..];
            let tld_run = tld
                .iter()
                .take_while(|c| c.is_ascii_alphabetic())
                .count()
                .min(6);
            for n2 in (2..=tld_run).rev() {
                let next = tld.get(n2).copied();
                let boundary = !next.is_some_and(is_word_char);
                if boundary && tld[n2..].iter().all(|&c| is_path_class(c)) {
                    return true;
                }
            }
        }
    }
    false
}

/// `new Uri(strUri)`: `None` where it throws.
///
/// DIVERGE: `System.Uri` is not ported; its validation here is the port range only (the one
/// failure the URL pattern lets through in the vectors). No ACE command takes a `Uri` parameter.
fn uri_new(s: &str) -> Option<String> {
    let after = &s[s.find("://")? + 3..];
    let authority = after.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority.rsplit('@').next().unwrap_or(authority);
    if let Some((_, port)) = host_port.rsplit_once(':') {
        if !port.is_empty()
            && !(port.bytes().all(|b| b.is_ascii_digit())
                && port.parse::<u32>().is_ok_and(|p| p <= 65535))
        {
            return None;
        }
    }
    Some(s.to_owned())
}

// ACE: CommandParameterHelpers.TryParsePosition
/// Try to parse a player supplied coordinate into a position: `parameters` holds 2 contiguous
/// elements from `starting_element` (north/south, then east/west).
///
/// # Errors
/// The problem encountered while trying to parse (ACE's `errorMessage`).
pub fn try_parse_position(
    w: &mut World,
    parameters: &[String],
    starting_element: usize,
) -> Result<Position, String> {
    if parameters.len() < starting_element + 2 {
        return Err("not enough parameters".to_owned());
    }

    let north_south = parameters[starting_element].to_lowercase().replace(',', "");
    let north_south = north_south.trim();
    let east_west = parameters[starting_element + 1]
        .to_lowercase()
        .replace(',', "");
    let east_west = east_west.trim();

    if !culture_ends_with(north_south, 'n') && !culture_ends_with(north_south, 's') {
        return Err("Missing n or s indicator on first parameter".to_owned());
    }

    if !culture_ends_with(east_west, 'e') && !culture_ends_with(east_west, 'w') {
        return Err("Missing e or w indicator on second parameter".to_owned());
    }

    let Some(mut coord_ns) = dotnet_parse::float_try_parse(drop_last_char(north_south)) else {
        return Err("North/South coordinate is not a valid number.".to_owned());
    };

    let Some(mut coord_ew) = dotnet_parse::float_try_parse(drop_last_char(east_west)) else {
        return Err("East/West coordinate is not a valid number.".to_owned());
    };

    if culture_ends_with(north_south, 's') {
        coord_ns *= -1.0f32;
    }

    if culture_ends_with(east_west, 'w') {
        coord_ew *= -1.0f32;
    }

    let mut position = Position::from_map_coordinates(Vector2::new(coord_ew, coord_ns));
    if let Err(e) = position_extensions::adjust_map_coords(w, &mut position) {
        // `Console.WriteLine(e);`
        log::info!("{e}");
        return Err("There was a problem with that location (bad coordinates?).".to_owned());
    }
    Ok(position)
}

/// `s.Substring(0, s.Length - 1)`.
fn drop_last_char(s: &str) -> &str {
    s.char_indices().last().map_or(s, |(i, _)| &s[..i])
}

/// .NET's `TryParse` for the types the command handlers read, with their default
/// `NumberStyles` and the en-US culture.
pub mod dotnet_parse {
    /// `NumberStyles.AllowLeadingWhite`/`AllowTrailingWhite`: U+0009..U+000D and U+0020.
    fn trim_white(s: &str) -> &str {
        s.trim_matches(|c: char| matches!(c, '\u{9}'..='\u{D}' | ' '))
    }

    /// .NET's `Number.TrailingZeros`: once the number and its trailing white space have been
    /// read, any rest made only of NULs is accepted. The text without those NULs (the special
    /// values, "NaN" and the infinities, get no such allowance).
    fn without_trailing_nuls(s: &str) -> &str {
        s.trim_end_matches(char::from(0))
    }

    /// `NumberStyles.Integer`: white, one sign, digits, white, then any NULs. The sign and the
    /// magnitude.
    fn integer_parts(s: &str) -> Option<(bool, u128)> {
        let s = trim_white(without_trailing_nuls(s));
        let (negative, digits) = match s.as_bytes().first()? {
            b'-' => (true, &s[1..]),
            b'+' => (false, &s[1..]),
            _ => (false, s),
        };
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let mut value: u128 = 0;
        for b in digits.bytes() {
            value = value
                .saturating_mul(10)
                .saturating_add(u128::from(b - b'0'));
        }
        Some((negative, value))
    }

    /// `int.TryParse(s, out int)`.
    #[must_use]
    pub fn int_try_parse(s: &str) -> Option<i32> {
        let (negative, v) = integer_parts(s)?;
        let v = i128::try_from(v).ok()?;
        i32::try_from(if negative { -v } else { v }).ok()
    }

    /// `uint.TryParse(s, out uint)` (`"-0"` is 0).
    #[must_use]
    pub fn uint_try_parse(s: &str) -> Option<u32> {
        let (negative, v) = integer_parts(s)?;
        if negative && v != 0 {
            return None;
        }
        u32::try_from(v).ok()
    }

    /// `long.TryParse(s, out long)`.
    #[must_use]
    pub fn long_try_parse(s: &str) -> Option<i64> {
        let (negative, v) = integer_parts(s)?;
        let v = i128::try_from(v).ok()?;
        i64::try_from(if negative { -v } else { v }).ok()
    }

    /// `ulong.TryParse(s, out ulong)` (`"-0"` is 0).
    #[must_use]
    pub fn ulong_try_parse(s: &str) -> Option<u64> {
        let (negative, v) = integer_parts(s)?;
        if negative && v != 0 {
            return None;
        }
        u64::try_from(v).ok()
    }

    /// `uint.TryParse(s, NumberStyles.HexNumber, CultureInfo.CurrentCulture, out uint)`.
    #[must_use]
    pub fn uint_try_parse_hex(s: &str) -> Option<u32> {
        let s = trim_white(s);
        if s.is_empty() || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let digits = s.trim_start_matches('0');
        if digits.len() > 8 {
            return None;
        }
        if digits.is_empty() {
            return Some(0);
        }
        u32::from_str_radix(digits, 16).ok()
    }

    /// `NumberStyles.Float | AllowThousands` (en-US) as a Rust float literal (trailing NULs
    /// allowed after the number), or the special value's text.
    fn float_text(s: &str) -> Option<String> {
        if let Some(n) = number_text(trim_white(without_trailing_nuls(s))) {
            return Some(n);
        }
        let t = trim_white(s);
        // The culture's symbols (ICU en-US: "∞", "-∞", "NaN"), ordinal ignoring case, then after
        // a leading sign.
        let special = |x: &str| -> Option<&'static str> {
            if x == "\u{221E}" {
                Some("inf")
            } else if x == "-\u{221E}" {
                Some("-inf")
            } else if x.eq_ignore_ascii_case("NaN") {
                Some("NaN")
            } else {
                None
            }
        };
        if let Some(v) = special(t) {
            return Some(v.to_owned());
        }
        if let Some(rest) = t.strip_prefix('+') {
            if rest == "\u{221E}" {
                return Some("inf".to_owned());
            }
            if rest.eq_ignore_ascii_case("NaN") {
                return Some("NaN".to_owned());
            }
        }
        if let Some(rest) = t.strip_prefix('-') {
            if rest.eq_ignore_ascii_case("NaN") {
                return Some("NaN".to_owned());
            }
        }
        None
    }

    /// `[sign]digits[,digits][.digits][e[sign]digits]`: the text without group separators.
    fn number_text(t: &str) -> Option<String> {
        let b = t.as_bytes();
        let mut i = 0;
        let mut out = String::new();
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            out.push(char::from(b[i]));
            i += 1;
        }
        let mut int_digits = 0;
        while i < b.len() && (b[i].is_ascii_digit() || (b[i] == b',' && int_digits > 0)) {
            if b[i] != b',' {
                out.push(char::from(b[i]));
                int_digits += 1;
            }
            i += 1;
        }
        let mut frac_digits = 0;
        if i < b.len() && b[i] == b'.' {
            out.push('.');
            i += 1;
            while i < b.len() && b[i].is_ascii_digit() {
                out.push(char::from(b[i]));
                frac_digits += 1;
                i += 1;
            }
        }
        if int_digits + frac_digits == 0 {
            return None;
        }
        if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
            let mut j = i + 1;
            let mut exp = String::from("e");
            if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
                exp.push(char::from(b[j]));
                j += 1;
            }
            let digits_from = j;
            while j < b.len() && b[j].is_ascii_digit() {
                exp.push(char::from(b[j]));
                j += 1;
            }
            if j == digits_from {
                return None;
            }
            out.push_str(&exp);
            i = j;
        }
        if i != b.len() {
            return None;
        }
        if out.ends_with('.') {
            out.push('0');
        }
        if out.starts_with('.') || out.starts_with("-.") || out.starts_with("+.") {
            out = out.replacen('.', "0.", 1);
        }
        Some(out)
    }

    /// `float.TryParse(s, out float)`.
    #[must_use]
    pub fn float_try_parse(s: &str) -> Option<f32> {
        float_text(s)?.parse::<f32>().ok()
    }

    /// `double.TryParse(s, out double)`.
    #[must_use]
    pub fn double_try_parse(s: &str) -> Option<f64> {
        float_text(s)?.parse::<f64>().ok()
    }
}
