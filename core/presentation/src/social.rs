//! Social names and allegiance rank presentation.

/// Apply the name filter after the client's wide-to-narrow conversion.
#[must_use]
pub fn format_name(input: &str) -> String {
    let units: Vec<u16> = input.encode_utf16().collect();
    dereth_primitives::text::to_spstring(&units)
        .map(|narrow| dereth_rules::names::format_name(&narrow.bytes))
        .unwrap_or_default()
}

#[must_use]
pub fn title_and_name(full: &str) -> (&str, &str) {
    full.split_once(' ').unwrap_or(("", full))
}

/// The earlier interface renders both the quality and its subtraction as unsigned words.
#[must_use]
pub fn classic_rank(quality: i32, rank: u16) -> String {
    let raw = u32::from_ne_bytes(quality.to_ne_bytes());
    let rank = u32::from(rank);
    if raw == rank {
        rank.to_string()
    } else {
        format!("{raw} (+{})", raw.wrapping_sub(rank))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Behaviour: social.names.shared-narrow-filter
    #[test]
    fn both_name_consumers_use_narrow_neighbors_and_the_byte_limit() {
        assert_eq!(format_name("mCLOUD III"), "McLoud III");
        assert_eq!(format_name("iii"), "Iii");
        assert_eq!(format_name("A\u{e9}-b"), "A-b");
        assert_eq!(
            format_name("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef rest"),
            "Abcdefghijklmnopqrstuvwxyzabcdef"
        );
        assert_eq!(classic_rank(0, 3), "0 (+4294967293)");
        assert_eq!(classic_rank(-1, 3), "4294967295 (+4294967292)");
    }
}
