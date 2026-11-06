//! The run-length form of the vertices a light reaches in a cell: a list of 16-bit numbers in which
//! a run of numbers each one above the last is kept as its first number with the top bit set, then
//! the run's length.

/// The top bit: what marks a run's start.
const RUN: u16 = 0x8000;
/// The largest output either direction takes, in numbers; a value of this size is refused.
const LIMIT: usize = 0x7FFF;

/// The numbers a run-length form holds. A run's length that is not above 0 gives nothing; a run
/// with no length after it reads as the end of the input (no numbers).
// D2-03903
#[must_use]
pub fn decode(input: &[u16]) -> Vec<u16> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < input.len() {
        let v = input[i];
        if v & RUN == 0 {
            out.push(v);
        } else {
            i += 1;
            let length = input.get(i).map_or(0, |n| n.cast_signed());
            let first = v & !RUN;
            for k in 0..length {
                out.push(first.wrapping_add(k.cast_unsigned()));
            }
        }
        i += 1;
    }
    out
}

/// The run-length form of a list of numbers: two or more numbers each one above the last become a
/// run. Refused when a number is the largest kept (`0x7FFF`) or the form would pass its limit: a
/// number needs room for itself under `0x7FFE` places, a run for two under `0x7FFD`.
// D2-03904
#[must_use]
pub fn encode(input: &[u16]) -> Option<Vec<u16>> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < input.len() {
        let v = input[at];
        if v == 0x7FFF {
            return None;
        }
        // Compared as signed numbers, as the client compares them.
        let mut run = 1usize;
        #[allow(clippy::cast_possible_truncation)]
        while run < input.len() - at
            && i32::from(input[at + run].cast_signed())
                == i32::from((run as u16).cast_signed()) + i32::from(v.cast_signed())
        {
            run += 1;
        }
        if run < 2 {
            if out.len() >= LIMIT - 1 {
                return None;
            }
            out.push(v);
        } else {
            if out.len() >= LIMIT - 2 {
                return None;
            }
            out.push(v | RUN);
            #[allow(clippy::cast_possible_truncation)]
            out.push(run as u16);
            at += run - 1;
        }
        at += 1;
    }
    Some(out)
}
