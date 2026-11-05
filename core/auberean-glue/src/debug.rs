//! The scripts' debug lines: a number or a string printed on a line of its own after `interp:  `,
//! for the developer's console, which the host keeps ([`Host::print`]; the client opens a console
//! window for them, a host without one lets them go). None of them answers anything.
//!
//! [`Host::print`]: auberean_script::native::Host::print

use auberean_script::native::{Call, NativeFn, NativeTable, Status, Stop};
use auberean_script::value::join;

const DEBUG: u32 = 0x1C;

/// This module's natives: package, number, function.
const NATIVES: &[(u32, u32, NativeFn)] = &[
    (DEBUG, 0x09, debugger),
    (DEBUG, 0x01, print_int),
    (DEBUG, 0x02, print_long_int),
    (DEBUG, 0x03, print_float),
    (DEBUG, 0x04, print_long_float),
    (DEBUG, 0x05, print_string),
];

/// A command for the script debugger: this client has none, so nothing comes of it.
// D2-09581
fn debugger(c: &mut Call<'_>) -> Result<Status, Stop> {
    let _ = c;
    Ok(Status::Success)
}

/// Bind this module's natives.
pub fn bind(t: &mut NativeTable) {
    crate::bind_table(t, NATIVES);
}

fn print(c: &mut Call<'_>, line: &str) -> Result<Status, Stop> {
    c.host.print(&format!("interp:  {line}"));
    Ok(Status::Success)
}

/// A 32-bit integer (`fp[-2]`), in hexadecimal and as a signed number.
// D2-09574
fn print_int(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = c.cx.at(-2)?;
    #[allow(clippy::cast_possible_wrap)]
    let signed = v as i32;
    print(c, &format!("0x{v:08x} ({signed})"))
}

/// A 64-bit integer (`fp[-3..-2]`, low word first), in hexadecimal (capitals) and as a signed
/// number.
// D2-09576
fn print_long_int(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = join(c.cx.at(-3)?, c.cx.at(-2)?);
    #[allow(clippy::cast_possible_wrap)]
    let signed = v as i64;
    print(c, &format!("0x{v:016X} ({signed})"))
}

/// A float (`fp[-2]`) to three places, as the C library rounds.
// D2-09573
fn print_float(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = f32::from_bits(c.cx.at(-2)?);
    print(c, &auberean_text::crt::fixed(f64::from(v), 3))
}

/// A 64-bit float (`fp[-3..-2]`) to six places.
// D2-09575
fn print_long_float(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = f64::from_bits(join(c.cx.at(-3)?, c.cx.at(-2)?));
    print(c, &auberean_text::crt::fixed(v, 6))
}

/// A string (`fp[-2]`).
// D2-09577
fn print_string(c: &mut Call<'_>) -> Result<Status, Stop> {
    let id = c.cx.at(-2)?;
    let text = crate::machine::text_of(c, id)?;
    print(c, &String::from_utf16_lossy(&text))
}
