// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/StarterSpell.cs
//! Port of `Source/ACE.Server/Entity/StarterSpell.cs`.
//!
//! ACE deserializes `starterGear.json` at start-up. Here the server's generator applies the same
//! rules at build time, so the `StarterSpell` data class is the generated
//! table's row type, [`crate::factories::starter_gear_factory::StarterSpell`] (re-exported here).
//! `StringToBoolConverter` is ported over a minimal model of the JSON tokens it sees, so the
//! generator's rule can be tested against ACE's.

pub use crate::factories::starter_gear_factory::StarterSpell;

/// The JSON token `Utf8JsonReader` is positioned on when the converter's `Read` is called.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsonToken<'a> {
    /// `JsonTokenType.String`, with its unescaped value.
    String(&'a str),
    /// `JsonTokenType.True`.
    True,
    /// `JsonTokenType.False`.
    False,
    /// Any other token (a number, `null`, an object or array start).
    Other,
}

/// `Convert.ToBoolean(string)`: `bool.Parse`, which ignores leading and trailing white space and
/// trailing nulls and compares case-insensitively with "True" and "False". `null` is false.
///
/// # Errors
/// Any other text (.NET's `FormatException`).
pub fn convert_to_boolean(value: Option<&str>) -> Result<bool, &'static str> {
    let Some(value) = value else { return Ok(false) };
    let trimmed = value.trim_matches(|c: char| c.is_whitespace() || c == '\0');
    if trimmed.eq_ignore_ascii_case("True") {
        Ok(true)
    } else if trimmed.eq_ignore_ascii_case("False") {
        Ok(false)
    } else {
        Err("FormatException")
    }
}

/// ACE's `StringToBoolConverter`, a `JsonConverter<bool>`.
// ACE: StringToBoolConverter
#[derive(Debug, Clone, Copy, Default)]
pub struct StringToBoolConverter;

impl StringToBoolConverter {
    /// Reads a bool from a JSON string ("true", "True", ...) or a JSON literal.
    ///
    /// # Errors
    /// `FormatException` for a string that is not a bool, `JsonException` for any other token.
    // ACE: StringToBoolConverter.Read
    pub fn read(token: &JsonToken<'_>) -> Result<bool, &'static str> {
        match token {
            JsonToken::String(bool_string) => convert_to_boolean(Some(bool_string)),
            JsonToken::True => Ok(true),
            JsonToken::False => Ok(false),
            JsonToken::Other => Err("JsonException"),
        }
    }

    /// `writer.WriteBooleanValue(value)`: the JSON literal.
    // ACE: StringToBoolConverter.Write
    #[must_use]
    pub fn write(value: bool) -> &'static str {
        if value {
            "true"
        } else {
            "false"
        }
    }
}
