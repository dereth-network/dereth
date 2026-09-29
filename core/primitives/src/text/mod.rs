//! Text: the Windows-1252 table ([`cp1252`]) and the chat slice of retail string width conversion.
//!
//! The chat conversions keep bytes and UTF-16 explicit: no assumed CP1252 interpretation of ACP
//! bytes and no mutation of locale or global settings. Sources: retail's two wide-to-narrow
//! conversions, its narrowing helper, its narrow-to-wide conversion, and the original MSVCR70
//! `wctomb` / char `sprintf(%ws)`.
//!
//! Each conversion has two arms. By default it is computed over the [`cp1252`] table, which is pure.
//! With the `host-nls` feature on a Windows host it is the host's own NLS conversion instead
//! (`windows.rs`, the one unsafe boundary in this crate: bounded slices into four `kernel32`
//! functions, no locale set, nothing retained). Only the application enables that feature; a
//! library that needs a conversion takes a [`crate::HostEncoding`].
//!
//! For code page 1252 the two arms give the same answer, byte for byte and flag for flag: the table
//! carries Windows' best-fit mappings, and on a Windows host `nls_parity` holds it to the operating
//! system. So chat text narrows the same way on every platform, as it does on a western Windows
//! install. [`ChatConversion::table`] selects the table arm explicitly, whatever the build.

/// The host arm: the host's live ANSI code page and NLS tables.
#[cfg(all(windows, feature = "host-nls"))]
#[allow(unsafe_code)]
mod windows;

pub mod cp1252;
mod cp1252_best_fit;

/// The table arm against the host arm, on the host that has both.
#[cfg(all(test, windows, feature = "host-nls"))]
mod nls_parity;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    UnsupportedPlatform,
    TooLong,
    Windows(u32),
    WindowsSizeQuery(u32),
    InvalidUtf16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Narrow {
    /// Raw char bytes, excluding only the appended terminator (interior NULs are preserved).
    pub bytes: Vec<u8>,
    /// The source's whole-string <%04x> fallback, not default-character substitution.
    pub escaped: bool,
}

/// Read-only platform environment, not a user setting. Default0 is the actual OS ACP.
/// An explicit code page permits private NLS-environment tests without changing any global
/// or OS locale. English CRT formatting is independent and is never changed by this selector.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChatConversion {
    ansi_code_page: u32,
    /// The [`cp1252`] table even where the host arm is built.
    table: bool,
}
impl ChatConversion {
    /// The host arm where it is built (the host's NLS, for any code page it knows), the table
    /// otherwise.
    pub const fn for_ansi_code_page(ansi_code_page: u32) -> Self {
        Self {
            ansi_code_page,
            table: false,
        }
    }

    /// The table arm on every build. It is code page 1252 only: `0` (`CP_ACP`) and `1252` select
    /// it, and any other code page is [`Error::UnsupportedPlatform`], never 1252 under another
    /// number.
    pub const fn table(ansi_code_page: u32) -> Self {
        Self {
            ansi_code_page,
            table: true,
        }
    }

    /// Whether this conversion goes through the host's NLS functions.
    #[cfg(all(windows, feature = "host-nls"))]
    const fn uses_host(self) -> bool {
        !self.table
    }
}

// Retail's failure literal, its second-call error path and [`to_wpstring`]. `%d` interprets the
// Windows error word as signed 32-bit; the separate `%08x` keeps all original bits. Only an NLS
// call can fail, so on the table arm these diagnostics are reachable from the tests alone.
#[cfg_attr(not(all(windows, feature = "host-nls")), allow(dead_code))]
fn failure_text(error: u32) -> String {
    let signed = i32::from_ne_bytes(error.to_ne_bytes());
    format!("Failed conversion to codepage 0! GetLastError {signed} (0x{error:08x})\n")
}
#[cfg_attr(not(all(windows, feature = "host-nls")), allow(dead_code))]
fn widen_failure_text(error: u32, query: bool) -> String {
    let signed = i32::from_ne_bytes(error.to_ne_bytes());
    let prefix = if query {
        "Could not determine number of bytes needed to convert"
    } else {
        "Failed conversion"
    };
    format!("{prefix} from codepage 0! GetLastError {signed} (0x{error:08x})\n")
}

/// CP_ACP0 and 1252 are the table; any other explicit selector has no table here.
fn select(code_page: u32) -> Result<(), Error> {
    if code_page == 0 || code_page == cp1252::ACP {
        Ok(())
    } else {
        Err(Error::UnsupportedPlatform)
    }
}

/// Read the current OS ANSI code page without changing it. On the table arm there is no host
/// ACP; the table stands in, so this reports 1252.
pub fn acp() -> Result<u32, Error> {
    #[cfg(all(windows, feature = "host-nls"))]
    {
        Ok(windows::acp())
    }
    #[cfg(not(all(windows, feature = "host-nls")))]
    {
        Ok(cp1252::ACP)
    }
}

/// Both retail wide-to-narrow implementations query WideCharToMultiByte(CP_ACP,0, ...,"?",
/// &usedDefault). Query failure OR substitution escapes every non-ASCII UTF16 unit, not
/// merely the offending character. Best-fit without substitution remains the native bytes.
pub fn to_spstring(units: &[u16]) -> Result<Narrow, Error> {
    ChatConversion::default().to_spstring(units)
}
impl ChatConversion {
    pub fn to_spstring(self, units: &[u16]) -> Result<Narrow, Error> {
        if units.len() >= i32::MAX as usize {
            return Err(Error::TooLong);
        }
        #[cfg(all(windows, feature = "host-nls"))]
        if self.uses_host() {
            let mut terminated = units.to_vec();
            terminated.push(0);
            return match windows::query(self.ansi_code_page, &terminated, true) {
                Ok((size, false)) => {
                    let (mut bytes, _) =
                        match windows::convert(self.ansi_code_page, &terminated, size, true) {
                            Ok(result) => result,
                            Err(Error::Windows(error)) => {
                                return Ok(Narrow {
                                    bytes: failure_text(error).into_bytes(),
                                    escaped: false,
                                })
                            }
                            Err(error) => return Err(error),
                        };
                    bytes.pop();
                    Ok(Narrow {
                        bytes,
                        escaped: false,
                    })
                }
                // Units above 0x7F become the literal `<%04x>`. Retail's second copy agrees.
                Ok((_, true)) | Err(Error::Windows(_)) => Ok(Narrow {
                    bytes: cp1252::escape_non_ascii(units),
                    escaped: true,
                }),
                Err(error) => Err(error),
            };
        }
        // Same shape over the 1252 table: a best fit keeps its byte, usedDefault -> <%04x>. No
        // NLS failure exists here, so the failure_text arm never fires; an unknown selector is
        // Err.
        select(self.ansi_code_page)?;
        match cp1252::narrow(units) {
            (bytes, false) => Ok(Narrow {
                bytes,
                escaped: false,
            }),
            (_, true) => Ok(Narrow {
                bytes: cp1252::escape_non_ascii(units),
                escaped: true,
            }),
        }
    }
}

/// Retail's narrow-to-wide conversion: CP_ACP, flags zero, explicit byte count.
/// Does not interpret bytes as UTF8 or impose a Western code page.
pub fn to_wpstring(bytes: &[u8]) -> Result<Vec<u16>, Error> {
    ChatConversion::default().to_wpstring(bytes)
}
impl ChatConversion {
    pub fn to_wpstring(self, bytes: &[u8]) -> Result<Vec<u16>, Error> {
        if bytes.len() >= i32::MAX as usize {
            return Err(Error::TooLong);
        }
        #[cfg(all(windows, feature = "host-nls"))]
        if self.uses_host() {
            let mut terminated = bytes.to_vec();
            terminated.push(0);
            let mut units = match windows::widen(self.ansi_code_page, &terminated) {
                Ok(units) => units,
                Err(Error::WindowsSizeQuery(error)) => {
                    return Ok(widen_failure_text(error, true).encode_utf16().collect())
                }
                Err(Error::Windows(error)) => {
                    return Ok(widen_failure_text(error, false).encode_utf16().collect())
                }
                Err(error) => return Err(error),
            };
            units.pop();
            return Ok(units);
        }
        // Flags0 widening of 1252 maps every byte, so neither widen_failure_text arm fires.
        select(self.ansi_code_page)?;
        Ok(cp1252::widen(bytes))
    }
}

/// A single %ws field in the main client's char sprintf after Init's setlocale("English").
/// The original MSVCR70 chooses English_United States.1252. Its _output loop converts each
/// UTF16 unit with wctomb; on a substituted/unconvertible unit it stops THIS field, retains
/// its preceding bytes and resumes the enclosing format. Not a whole-string replacement.
/// The host arm uses the same NLS operation directly, without changing any CRT/OS locale.
pub fn english_ws_field(units: &[u16]) -> Result<Vec<u8>, Error> {
    ChatConversion::default().english_ws_field(units)
}
impl ChatConversion {
    /// The field is code page 1252 whatever this conversion's ANSI code page; only the arm comes
    /// from `self`.
    pub fn english_ws_field(self, units: &[u16]) -> Result<Vec<u8>, Error> {
        #[cfg(all(windows, feature = "host-nls"))]
        if self.uses_host() {
            let mut out = Vec::new();
            for &unit in units.iter().take_while(|&&unit| unit != 0) {
                // `wctomb`: codepage from English locale, flags 0, default NULL,
                // usedDefault pointer, output capacity MB_CUR_MAX (one for 1252).
                match windows::convert(1252, &[unit], 1, false) {
                    Ok((bytes, false)) => out.extend(bytes),
                    Ok((_, true)) | Err(Error::Windows(_)) => break,
                    Err(error) => return Err(error),
                }
            }
            return Ok(out);
        }
        // wctomb over the 1252 table: best fit applied, and the first unit that raises
        // usedDefault ends the field.
        Ok(cp1252::ws_field(units))
    }
}

/// Explicit String bridge for consumers whose storage is already Rust Unicode.
pub fn wide_string(units: &[u16]) -> Result<String, Error> {
    String::from_utf16(units).map_err(|_| Error::InvalidUtf16)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn distinct_primary_conversion_failure_formats_keep_signed_error_bits() {
        // Retail's narrow and UTF16 failure literals, across all three error branches.
        assert_eq!(
            failure_text(0x8000_0001),
            "Failed conversion to codepage 0! GetLastError -2147483647 (0x80000001)\n"
        );
        assert_eq!(widen_failure_text(87, true),
            "Could not determine number of bytes needed to convert from codepage 0! GetLastError 87 (0x00000057)\n");
        assert_eq!(
            widen_failure_text(87, false),
            "Failed conversion from codepage 0! GetLastError 87 (0x00000057)\n"
        );
    }
    #[cfg(all(windows, feature = "host-nls"))]
    #[test]
    fn failed_nls_query_uses_source_fallback_or_wide_diagnostic_without_guessing_bytes() {
        // Constructed invalid platform selector: causes a real NLS failure, no OS mutation.
        let environment = ChatConversion::for_ansi_code_page(u32::MAX);
        assert_eq!(
            environment.to_spstring(&[0x41, 0xe9, 0xd800]).unwrap(),
            Narrow {
                bytes: b"A<00e9><d800>".to_vec(),
                escaped: true
            }
        );
        assert_eq!(wide_string(&environment.to_wpstring(b"a").unwrap()).unwrap(),
            "Could not determine number of bytes needed to convert from codepage 0! GetLastError 87 (0x00000057)\n");
    }
    #[cfg(not(all(windows, feature = "host-nls")))]
    #[test]
    fn without_the_host_arm_the_default_is_the_table() {
        assert_eq!(acp(), Ok(1252));
        assert_eq!(
            ChatConversion::default().to_spstring(&[0x0101]),
            ChatConversion::table(0).to_spstring(&[0x0101])
        );
        let other = ChatConversion::for_ansi_code_page(1251);
        assert_eq!(other.to_spstring(&[65]), Err(Error::UnsupportedPlatform));
        assert_eq!(other.to_wpstring(b"a"), Err(Error::UnsupportedPlatform));
    }
    #[test]
    fn the_table_arm_converts_through_the_table_with_best_fit() {
        let table = ChatConversion::table(0);
        assert_eq!(
            table.to_spstring(&[0x41, 0xe9, 0x20ac]).unwrap(),
            Narrow {
                bytes: b"A\xe9\x80".to_vec(),
                escaped: false
            }
        );
        // A best fit is the native byte, not a substitution: nothing is escaped.
        let fullwidth: Vec<u16> = "＜ＴＥＬＬ：Name>".encode_utf16().collect();
        assert_eq!(
            table.to_spstring(&fullwidth).unwrap(),
            Narrow {
                bytes: b"<TELL:Name>".to_vec(),
                escaped: false
            }
        );
        // usedDefault takes the source's whole-string <%04x> arm; a pair escapes per unit.
        assert_eq!(
            table.to_spstring(&[0x41, 0xe9, 0xd83d, 0xde00]).unwrap(),
            Narrow {
                bytes: b"A<00e9><d83d><de00>".to_vec(),
                escaped: true
            }
        );
        assert_eq!(
            table.to_wpstring(b"a\x80\x81\x00\xff").unwrap(),
            [0x61, 0x20ac, 0x81, 0, 0xff]
        );
        assert_eq!(
            table
                .english_ws_field(&[0x41, 0xe9, 0x0101, 0x4e00, 0x42, 0])
                .unwrap(),
            b"A\xe9a"
        );
        assert_eq!(table.english_ws_field(&[0x41, 0, 0x42]).unwrap(), b"A");
        // An explicit selector other than CP_ACP/1252 has no table here.
        let other = ChatConversion::table(1251);
        assert_eq!(other.to_spstring(&[65]), Err(Error::UnsupportedPlatform));
        assert_eq!(other.to_wpstring(b"a"), Err(Error::UnsupportedPlatform));
        assert_eq!(
            ChatConversion::table(1252)
                .to_spstring(&[65])
                .unwrap()
                .bytes,
            b"A"
        );
    }
    #[test]
    fn only_cp_acp_and_1252_select_the_table() {
        assert_eq!(select(0), Ok(()));
        assert_eq!(select(1252), Ok(()));
        assert_eq!(select(1251), Err(Error::UnsupportedPlatform));
        assert_eq!(select(65001), Err(Error::UnsupportedPlatform));
        assert_eq!(select(u32::MAX), Err(Error::UnsupportedPlatform));
    }
}
