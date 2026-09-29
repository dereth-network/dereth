//! Retail language-filter matching for `TabooTable` (`0x0E00001E`).
//!
//! The chat scroll splits the composed line on ASCII spaces and runs the taboo table's
//! wide-string censor check on each token with arguments `(token, 1, 1)` and an out-parameter for
//! the matched pattern.  Mode 1 leaves
//! every byte in the check string, the censor check lowercases it, and
//! the filter match performs whole-string `*` glob matching.  A match in table
//! bucket 1, 2, 3 or 4 returns a non-zero censor code; the caller then replaces every literal
//! occurrence of that original token in the full line with `L"****"`.

use dereth_assets::TabooTable;
use dereth_primitives::HostEncoding;

/// The audience the chat scroll passes to the wide censor check.
pub const CHAT_AUDIENCE: u32 = 1;

/// The literal the chat scroll loads to replace a censored token.
pub const CENSOR_REPLACEMENT: &str = "****";

/// Whether one mode-1 check string matches one table pattern.
///
/// Retail's matcher has one wildcard, `*`, and otherwise compares bytes exactly. The caller has
/// already made the candidate lowercase; shipped patterns are stored lowercase.
#[must_use]
pub fn string_matches_filter(candidate: &[u8], pattern: &[u8]) -> bool {
    // A direct state-machine transcription of the client's filter match. Missing bytes are the C strings' NUL.
    let at = |bytes: &[u8], index: usize| bytes.get(index).copied().unwrap_or(0);
    let (mut candidate_at, mut pattern_at) = (0, 0);
    let mut last_star = None;

    'match_again: loop {
        while at(pattern, pattern_at) == b'*' {
            let star = pattern_at;
            let next = at(pattern, star + 1);
            loop {
                let candidate_byte = at(candidate, candidate_at);
                if candidate_byte == 0 {
                    return next == 0;
                }
                last_star = Some(star);
                if candidate_byte == next {
                    pattern_at = star + 1;
                    break;
                }
                candidate_at += 1;
            }
        }

        if at(pattern, pattern_at) == at(candidate, candidate_at) {
            loop {
                if at(candidate, candidate_at) == 0 {
                    return true;
                }
                pattern_at += 1;
                candidate_at += 1;
                if at(pattern, pattern_at) == b'*' {
                    continue 'match_again;
                }
                if at(pattern, pattern_at) != at(candidate, candidate_at) {
                    break;
                }
            }
        }
        let Some(star) = last_star else { return false };
        // Native resumes the last star at the current mismatch character. It does not keep a
        // separate conventional-glob retry cursor, which matters for overlapping literal runs.
        pattern_at = star;
    }
}

/// Return whether the taboo-table censor check accepts `token` for audience 1 and mode 1.
///
/// `encoding` is the host's, because the narrowing is a `CP_ACP` conversion and this crate has no
/// platform. Every host takes the one retail path through whichever implementation it installed;
/// `dereth_protocol::cp1252::encode` is a *different* rule (it rejects a token it cannot encode
/// instead of escaping it) and must not stand in for it.
#[must_use]
pub fn censors_chat_token(table: &TabooTable, encoding: &dyn HostEncoding, token: &str) -> bool {
    // The wide censor check first narrows the string, and that narrowing
    // uses WideCharToMultiByte(CP_ACP, 0),
    // including its whole-string fallback.
    let Some(mut check) = encoding.narrow(&token.encode_utf16().collect::<Vec<_>>()) else {
        return false;
    };
    check.make_ascii_lowercase();
    table
        .audiences
        .iter()
        .find(|(audience, _)| *audience == CHAT_AUDIENCE)
        .is_some_and(|(_, buckets)| {
            buckets.iter().any(|(key, patterns)| {
                // The censor check maps only these four bucket keys to non-zero return values:
                // 1 -> 3, 2 -> 4, 3 -> 6, 4 -> 5. Other keys never tell the caller to replace.
                (1..=4).contains(key)
                    && patterns.iter().any(|pattern| {
                        dereth_protocol::cp1252::encode(pattern)
                            .is_some_and(|pattern| string_matches_filter(&check, &pattern))
                    })
            })
        })
}

/// Whether any space-separated word of `text` matches the table, with the same plain-mode check the
/// chat censor runs on each token (lowercased, whole-word `*` glob, audience 1). The server refuses
/// a character name this answers true for: the matcher is the one retail's client and
/// server shared. The matcher's letters-only and space-stripped modes are left unused.
#[must_use]
pub fn contains_taboo_word(table: &TabooTable, encoding: &dyn HostEncoding, text: &str) -> bool {
    text.split(' ')
        .any(|token| censors_chat_token(table, encoding, token))
}

/// Apply the exact token loop retail runs when adding a line to the chat scroll to a composed line.
#[must_use]
pub fn filter_chat_line(table: &TabooTable, encoding: &dyn HostEncoding, text: &str) -> String {
    let mut filtered = text.to_owned();
    // Splitting on spaces produces the original tokens before replacement. Snapshot
    // them for the same reason: replacements must not change which later tokens are checked.
    let tokens: Vec<&str> = text.split(' ').collect();
    for token in tokens {
        if !token.is_empty() && censors_chat_token(table, encoding, token) {
            // Retail replaces matches across the whole line, not only this token's span.
            filtered = filtered.replace(token, CENSOR_REPLACEMENT);
        }
    }
    filtered
}

#[cfg(test)]
mod tests {
    use dereth_primitives::DataId;
    use dereth_protocol::cp1252::Cp1252;

    use super::*;

    #[allow(clippy::type_complexity)] // a one-off tuple, named where it is read
    fn table(audiences: Vec<(u32, Vec<(u32, Vec<String>)>)>) -> TabooTable {
        TabooTable {
            id: DataId(0x0E00_001E),
            audiences,
        }
    }

    #[test]
    fn mode_one_is_case_insensitive_and_star_is_the_only_wildcard() {
        let table = table(vec![(1, vec![(1, vec!["sh*t".into()])])]);
        assert!(censors_chat_token(&table, &Cp1252, "SHOUT"));
        assert!(censors_chat_token(&table, &Cp1252, "shit"));
        assert!(!censors_chat_token(&table, &Cp1252, "shot!"));
        assert!(
            !censors_chat_token(&table, &Cp1252, "a-shit"),
            "the pattern matches the whole token"
        );
        assert!(
            string_matches_filter(b"anything", b"*"),
            "one trailing star matches the rest"
        );
        assert!(
            !string_matches_filter(b"anything", b"**"),
            "native does not coalesce stars"
        );
        assert!(
            string_matches_filter(b"*", b"**"),
            "the second star is first searched literally"
        );
        assert!(
            !string_matches_filter(b"aaab", b"*aab"),
            "native resumes at the current mismatch and does not retry the overlapping start"
        );
    }

    #[test]
    fn only_chat_audience_and_nonzero_censor_buckets_replace() {
        let wrong_audience = table(vec![(2, vec![(1, vec!["bad".into()])])]);
        let zero_bucket = table(vec![(1, vec![(0, vec!["bad".into()])])]);
        let unknown_bucket = table(vec![(1, vec![(5, vec!["bad".into()])])]);
        assert!(!censors_chat_token(&wrong_audience, &Cp1252, "bad"));
        assert!(!censors_chat_token(&zero_bucket, &Cp1252, "bad"));
        assert!(!censors_chat_token(&unknown_bucket, &Cp1252, "bad"));
    }

    #[test]
    fn replacement_uses_the_original_token_over_the_whole_line() {
        let table = table(vec![(1, vec![(1, vec!["bad".into()])])]);
        assert_eq!(
            filter_chat_line(&table, &Cp1252, "bad badger bad"),
            "**** ****ger ****"
        );
    }
}
