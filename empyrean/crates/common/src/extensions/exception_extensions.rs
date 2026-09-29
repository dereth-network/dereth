// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Common/Extensions/ExceptionExtensions.cs
//! `ExceptionExtensions`, over Rust's error `source()` chain in place of `InnerException`.

use std::error::Error;

// ACE: ExceptionExtensions.GetFullMessage
/// `"outer --> inner --> innermost"`.
#[must_use]
pub fn get_full_message(ex: &dyn Error) -> String {
    match ex.source() {
        None => ex.to_string(),
        Some(inner) => ex.to_string() + " --> " + &get_full_message(inner),
    }
}
