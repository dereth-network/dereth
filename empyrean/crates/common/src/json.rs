//! The `System.Text.Json` options ACE reads `Config.js` with (`ConfigManager.SerializerOptions`),
//! as serde helpers:
//!
//! * `ReadCommentHandling = Skip`: `//` and `/* */` comments are removed before parsing;
//! * `AllowTrailingCommas = true`: one comma before `}` or `]` is removed;
//! * `NumberHandling = AllowReadingFromString`: a number may be written as a JSON string
//!   (`"ShutdownInterval": "60"`), which the `num_*` deserializers accept;
//! * property names match case-sensitively and unknown properties are ignored (serde's defaults).
//!
//! Source: dotnet/runtime `System.Text.Json` documentation for `JsonSerializerOptions`.

use serde::de::{self, Deserializer, Visitor};
use std::fmt;

/// Removes comments and trailing commas outside string literals, keeping every other byte.
#[must_use]
pub fn strip_comments_and_trailing_commas(text: &str) -> String {
    // Pass 1: comments (replaced by nothing; a line comment keeps its newline).
    let bytes: Vec<char> = text.chars().collect();
    let mut no_comments = String::with_capacity(text.len());
    let mut i = 0usize;
    let mut in_string = false;
    while i < bytes.len() {
        let c = bytes[i];
        if in_string {
            no_comments.push(c);
            if c == '\\' {
                if let Some(&n) = bytes.get(i + 1) {
                    no_comments.push(n);
                    i += 1;
                }
            } else if c == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if c == '"' {
            in_string = true;
            no_comments.push(c);
            i += 1;
        } else if c == '/' && bytes.get(i + 1) == Some(&'/') {
            while i < bytes.len() && bytes[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && bytes.get(i + 1) == Some(&'*') {
            i += 2;
            while i < bytes.len() && !(bytes[i] == '*' && bytes.get(i + 1) == Some(&'/')) {
                i += 1;
            }
            i += 2;
        } else {
            no_comments.push(c);
            i += 1;
        }
    }

    // Pass 2: a comma whose next significant character closes an object or array.
    let chars: Vec<char> = no_comments.chars().collect();
    let mut out = String::with_capacity(no_comments.len());
    let mut in_string = false;
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            out.push(c);
            if c == '\\' {
                if let Some(&n) = chars.get(i + 1) {
                    out.push(n);
                    i += 1;
                }
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
            out.push(c);
        } else if c == ',' {
            let next = chars[i + 1..].iter().find(|ch| !ch.is_whitespace());
            if !matches!(next, Some('}' | ']')) {
                out.push(c);
            }
        } else {
            out.push(c);
        }
        i += 1;
    }
    out
}

struct NumVisitor<T>(std::marker::PhantomData<T>);

macro_rules! num_de {
    ($name:ident, $t:ty) => {
        /// Deserializes a JSON number, or a JSON string holding one (`AllowReadingFromString`).
        ///
        /// # Errors
        /// When the value is neither, or does not fit.
        pub fn $name<'de, D: Deserializer<'de>>(d: D) -> Result<$t, D::Error> {
            d.deserialize_any(NumVisitor::<$t>(std::marker::PhantomData))
        }

        impl<'de> Visitor<'de> for NumVisitor<$t> {
            type Value = $t;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "a {} number or a string holding one", stringify!($t))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<$t, E> {
                <$t>::try_from(v)
                    .map_err(|_| E::custom(format!("{v} is out of range for {}", stringify!($t))))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<$t, E> {
                <$t>::try_from(v)
                    .map_err(|_| E::custom(format!("{v} is out of range for {}", stringify!($t))))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<$t, E> {
                Err(E::custom(format!("{v} is not an integer")))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<$t, E> {
                v.parse::<$t>()
                    .map_err(|_| E::custom(format!("{v:?} is not a valid {}", stringify!($t))))
            }
        }
    };
}

num_de!(num_u32, u32);
num_de!(num_i32, i32);

/// Deserializes a JSON number, or a JSON string holding a finite one, as `f64`.
///
/// # Errors
/// When the value is neither.
pub fn num_f64<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    d.deserialize_any(NumVisitor::<f64>(std::marker::PhantomData))
}

impl<'de> Visitor<'de> for NumVisitor<f64> {
    type Value = f64;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a number or a string holding one")
    }
    #[allow(clippy::cast_precision_loss)]
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<f64, E> {
        Ok(v as f64)
    }
    #[allow(clippy::cast_precision_loss)]
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<f64, E> {
        Ok(v as f64)
    }
    fn visit_f64<E: de::Error>(self, v: f64) -> Result<f64, E> {
        Ok(v)
    }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<f64, E> {
        // Named literals (NaN, Infinity) need AllowNamedFloatingPointLiterals, which ACE does not set.
        match v.parse::<f64>() {
            Ok(x)
                if x.is_finite()
                    && !v.contains(|c: char| c.is_ascii_alphabetic() && c != 'e' && c != 'E') =>
            {
                Ok(x)
            }
            _ => Err(E::custom(format!("{v:?} is not a valid number"))),
        }
    }
}
