//! Narrow character and fellowship name filtering and case masks.

pub fn format_name(input: &[u8]) -> String {
    let input = input.split(|b| *b == 0).next().unwrap_or_default();
    let end = input.iter().rposition(|b| *b != b' ').map_or(0, |i| i + 1);
    let raw = &input[..end];
    let raw = &raw[..raw.len().min(32)];
    let mut bytes = vec![];
    for (i, &b) in raw.iter().enumerate() {
        let previous = i.checked_sub(1).map_or(0, |p| raw[p]);
        let next = raw.get(i + 1).copied().unwrap_or(0);
        let valid = if b.is_ascii_alphabetic() {
            true
        } else {
            match b {
                b' ' => previous != b' ' && previous != 0 && next != 0,
                b'\'' => {
                    previous != b'\''
                        && previous != 0
                        && ((previous.is_ascii_alphabetic() || previous >= 0x80)
                            || (next.is_ascii_alphabetic() || next >= 0x80))
                }
                b'-' => {
                    previous != b'-'
                        && (previous.is_ascii_alphabetic() || previous >= 0x80)
                        && (next.is_ascii_alphabetic() || next >= 0x80 || next == b'-')
                }
                _ => false,
            }
        };
        if valid {
            bytes.push(b);
        }
    }
    let mut flags = vec![1; bytes.len()];
    if !flags.is_empty() {
        flags[0] = 0;
    }
    for i in 0..bytes.len() {
        if i == 0 || matches!(bytes[i - 1], b' ' | b'-' | b'\'') {
            if i > 0 {
                flags[i] = 2;
            }
            let tail = &bytes[i..];
            for prefix in [
                b"de".as_slice(),
                b"di",
                b"du",
                b"fitz",
                b"le",
                b"la",
                b"mac",
                b"mc",
                b"von",
                b"van",
            ] {
                if tail.len() >= prefix.len()
                    && tail[..prefix.len()].eq_ignore_ascii_case(prefix)
                    && i + prefix.len() < flags.len()
                {
                    flags[i + prefix.len()] = 2;
                }
            }
        }
    }
    let mut start = 0;
    while start < bytes.len() {
        if !bytes[start].is_ascii_alphabetic() {
            start += 1;
            continue;
        }
        let mut end = start;
        while end < bytes.len() && bytes[end].is_ascii_alphabetic() {
            end += 1;
        }
        if bytes[start..end]
            .iter()
            .all(|b| matches!(*b, b'I' | b'V' | b'X'))
        {
            flags[start..end].fill(0);
        }
        start = end;
    }
    for (b, flag) in bytes.iter_mut().zip(flags) {
        if flag == 0 {
            *b = b.to_ascii_uppercase();
        } else if flag == 1 {
            *b = b.to_ascii_lowercase();
        }
    }
    bytes.into_iter().map(char::from).collect()
}
