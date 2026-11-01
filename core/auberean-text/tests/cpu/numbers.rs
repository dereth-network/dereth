//! Numbers in a language's format.

use auberean_text::number::{float_to_string, int_to_string, uint_to_string, TableNumbers};

use crate::english;

fn text(u: Option<Vec<u16>>) -> Option<String> {
    u.map(|u| String::from_utf16_lossy(&u))
}

/// A number is written in its base with the language's numerals and separators: rounded half up
/// at its last decimal, grouped by threes when asked, negative through the language's pattern;
/// a table that keeps leading zeros pads to its least number of digits. Nothing is written for too
/// many decimals, a base below 2, or no language.
///
/// Behaviour: text.numbers.format
#[test]
fn a_number_is_written_rounded_grouped_and_padded_as_the_table_says() {
    let lang = english();
    let t = TableNumbers::default();
    let int = |v: i32, g: i32| text(int_to_string(t, Some(&lang), v, g, -1));
    let float = |x: f64, d: u16| text(float_to_string(t, Some(&lang), x, d, 1, -1));
    assert_eq!(int(1_234_567, 1).as_deref(), Some("1,234,567"));
    assert_eq!(int(1_234_567, 0).as_deref(), Some("1234567"));
    assert_eq!(int(-5, 1).as_deref(), Some("-5"));
    assert_eq!(float(1234.5678, 2).as_deref(), Some("1,234.57"));
    assert_eq!(float(0.5, 0).as_deref(), Some("1"));
    assert_eq!(float(2.5, 0).as_deref(), Some("3"));
    assert_eq!(float(0.25, 1).as_deref(), Some("0.3"));
    let padded = TableNumbers {
        base: -1,
        decimal_digits: 3,
        leading_zero: 1,
    };
    assert_eq!(
        text(uint_to_string(padded, Some(&lang), 7, 1, -1)).as_deref(),
        Some("007")
    );
    assert_eq!(
        text(uint_to_string(t, Some(&lang), 5, 0, 2)).as_deref(),
        Some("101")
    );
    assert_eq!(
        text(uint_to_string(t, Some(&lang), 255, 0, 16)).as_deref(),
        Some("0xFF")
    );
    assert_eq!(float(1.0, 21), None, "at most 20 decimals");
    assert_eq!(text(uint_to_string(t, Some(&lang), 5, 0, 1)), None);
    assert_eq!(text(uint_to_string(t, None, 5, 0, -1)), None);
}
