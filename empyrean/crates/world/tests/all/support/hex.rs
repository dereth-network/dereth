//! Wire bytes as hex text, for comparing encodings against recorded strings.

/// Upper-case hex, two digits per byte, no separators.
pub(crate) fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect()
}

/// The bytes a hex string spells, two digits per byte; a pair that is not hex panics with "hex".
pub(crate) fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
        .collect()
}
