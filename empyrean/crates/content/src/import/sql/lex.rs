//! A tokenizer for the SQL that ACE's content files are written in (MySQL syntax, default
//! `sql_mode`, so backslash escapes are on and `"…"` is a string, not an identifier).
//!
//! Not ACE-derived: this reads the text MySQL's own parser reads. Comments (`/* */`, `-- `, `#`)
//! are dropped, except MySQL's executable comments `/*!NNNNN … */`, whose body MySQL runs and so
//! is tokenized as code.

/// One token and the 1-based line it starts on.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: Tok,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    /// A bare word (keyword or identifier), as written.
    Word(String),
    /// A `` `quoted` `` identifier.
    Quoted(String),
    /// A string literal, escapes resolved.
    Str(Vec<u8>),
    /// A number literal as written (`12`, `-` is separate, `1.5`, `.5`, `1e-3`).
    Num(String),
    /// `0x…` or `X'…'`: the bytes.
    Hex(Vec<u8>),
    /// `b'…'`: the bit string's value.
    Bits(u64),
    /// `@name` (a user variable).
    UserVar(String),
    /// `@@name` or `@@session.name` (a system variable).
    SysVar(String),
    /// Punctuation or an operator: `( ) , ; . = < > <= >= <> != <=> + - * / >> << | &`.
    Punct(&'static str),
}

impl Tok {
    /// The word, upper-cased, if this is a bare word.
    #[must_use]
    pub fn keyword(&self) -> Option<String> {
        match self {
            Tok::Word(w) => Some(w.to_ascii_uppercase()),
            _ => None,
        }
    }
}

/// Tokenize a whole file. The error names the line.
pub fn tokenize(text: &[u8]) -> Result<Vec<Token>, String> {
    let mut out = Vec::new();
    let mut i = 0usize;
    let mut line: u32 = 1;
    // The end offset of the executable comment we are inside, if any.
    let mut exec_end: Option<usize> = None;
    let n = text.len();
    while i < n {
        let c = text[i];
        if Some(i) == exec_end {
            // The `*/` closing an executable comment.
            exec_end = None;
            i += 2;
            continue;
        }
        match c {
            b'\n' => {
                line += 1;
                i += 1;
            }
            b' ' | b'\t' | b'\r' | 0x0B | 0x0C => i += 1,
            b'/' if text.get(i + 1) == Some(&b'*') => {
                let close = find(text, i + 2, b"*/")
                    .ok_or_else(|| format!("line {line}: unterminated /* comment"))?;
                if text.get(i + 2) == Some(&b'!') && exec_end.is_none() {
                    // `/*!40101 SET … */`: skip the version digits and run the body.
                    let mut j = i + 3;
                    while j < close && text[j].is_ascii_digit() {
                        j += 1;
                    }
                    exec_end = Some(close);
                    i = j;
                } else {
                    line += count_lines(&text[i..close]);
                    i = close + 2;
                }
            }
            b'-' if text.get(i + 1) == Some(&b'-')
                && matches!(text.get(i + 2), None | Some(b' ' | b'\t' | b'\r' | b'\n')) =>
            {
                i = skip_line(text, i);
            }
            b'#' => i = skip_line(text, i),
            b'\'' | b'"' => {
                let (s, next, lines) = string(text, i).map_err(|e| format!("line {line}: {e}"))?;
                out.push(Token {
                    kind: Tok::Str(s),
                    line,
                });
                line += lines;
                i = next;
            }
            b'`' => {
                let close = text[i + 1..]
                    .iter()
                    .position(|&b| b == b'`')
                    .ok_or_else(|| format!("line {line}: unterminated `identifier`"))?;
                let name = String::from_utf8_lossy(&text[i + 1..i + 1 + close]).into_owned();
                out.push(Token {
                    kind: Tok::Quoted(name),
                    line,
                });
                i += close + 2;
            }
            b'@' => {
                let sys = text.get(i + 1) == Some(&b'@');
                let start = if sys { i + 2 } else { i + 1 };
                let mut j = start;
                while j < n
                    && (text[j].is_ascii_alphanumeric() || matches!(text[j], b'_' | b'.' | b'$'))
                {
                    j += 1;
                }
                let name = String::from_utf8_lossy(&text[start..j]).into_owned();
                if name.is_empty() {
                    return Err(format!("line {line}: '@' without a name"));
                }
                out.push(Token {
                    kind: if sys {
                        Tok::SysVar(name)
                    } else {
                        Tok::UserVar(name)
                    },
                    line,
                });
                i = j;
            }
            b'0' if matches!(text.get(i + 1), Some(b'x' | b'X'))
                && text.get(i + 2).is_some_and(u8::is_ascii_hexdigit) =>
            {
                let mut j = i + 2;
                while j < n && text[j].is_ascii_hexdigit() {
                    j += 1;
                }
                out.push(Token {
                    kind: Tok::Hex(hex_bytes(&text[i + 2..j])),
                    line,
                });
                i = j;
            }
            b'0'..=b'9' => {
                let j = number_end(text, i);
                out.push(Token {
                    kind: Tok::Num(ascii(&text[i..j])),
                    line,
                });
                i = j;
            }
            b'.' if text.get(i + 1).is_some_and(u8::is_ascii_digit) => {
                let j = number_end(text, i);
                out.push(Token {
                    kind: Tok::Num(ascii(&text[i..j])),
                    line,
                });
                i = j;
            }
            b'x' | b'X' if text.get(i + 1) == Some(&b'\'') => {
                let close = find(text, i + 2, b"'")
                    .ok_or_else(|| format!("line {line}: unterminated X'' literal"))?;
                out.push(Token {
                    kind: Tok::Hex(hex_bytes(&text[i + 2..close])),
                    line,
                });
                i = close + 1;
            }
            b'b' | b'B' if text.get(i + 1) == Some(&b'\'') => {
                let close = find(text, i + 2, b"'")
                    .ok_or_else(|| format!("line {line}: unterminated b'' literal"))?;
                let mut v: u64 = 0;
                for &d in &text[i + 2..close] {
                    if !matches!(d, b'0' | b'1') {
                        return Err(format!("line {line}: bad bit literal"));
                    }
                    v = (v << 1) | u64::from(d - b'0');
                }
                out.push(Token {
                    kind: Tok::Bits(v),
                    line,
                });
                i = close + 1;
            }
            c if c.is_ascii_alphabetic() || c == b'_' || c == b'$' || c >= 0x80 => {
                let mut j = i;
                while j < n
                    && (text[j].is_ascii_alphanumeric()
                        || matches!(text[j], b'_' | b'$')
                        || text[j] >= 0x80)
                {
                    j += 1;
                }
                out.push(Token {
                    kind: Tok::Word(String::from_utf8_lossy(&text[i..j]).into_owned()),
                    line,
                });
                i = j;
            }
            _ => {
                const OPS: &[&str] = &[
                    "<=>", "<=", ">=", "<>", "!=", ">>", "<<", "||", "&&", "(", ")", ",", ";", ".",
                    "=", "<", ">", "+", "-", "*", "/", "|", "&", "!",
                ];
                let op = OPS
                    .iter()
                    .find(|op| text[i..].starts_with(op.as_bytes()))
                    .ok_or_else(|| format!("line {line}: unexpected character {:?}", c as char))?;
                out.push(Token {
                    kind: Tok::Punct(op),
                    line,
                });
                i += op.len();
            }
        }
    }
    if exec_end.is_some() {
        return Err(format!("line {line}: unterminated /*! comment"));
    }
    Ok(out)
}

fn ascii(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

fn count_lines(b: &[u8]) -> u32 {
    u32::try_from(b.iter().filter(|&&c| c == b'\n').count()).unwrap_or(u32::MAX)
}

fn skip_line(text: &[u8], i: usize) -> usize {
    text[i..]
        .iter()
        .position(|&b| b == b'\n')
        .map_or(text.len(), |p| i + p)
}

fn find(text: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    if from > text.len() {
        return None;
    }
    text[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| from + p)
}

fn number_end(text: &[u8], mut j: usize) -> usize {
    let n = text.len();
    while j < n && (text[j].is_ascii_digit() || text[j] == b'.') {
        j += 1;
    }
    if j < n && matches!(text[j], b'e' | b'E') {
        let mut k = j + 1;
        if k < n && matches!(text[k], b'+' | b'-') {
            k += 1;
        }
        if k < n && text[k].is_ascii_digit() {
            j = k;
            while j < n && text[j].is_ascii_digit() {
                j += 1;
            }
        }
    }
    j
}

fn hex_bytes(digits: &[u8]) -> Vec<u8> {
    let val = |d: u8| match d {
        b'0'..=b'9' => d - b'0',
        b'a'..=b'f' => d - b'a' + 10,
        _ => d - b'A' + 10,
    };
    // An odd number of digits has an implied leading zero.
    let mut out = Vec::with_capacity(digits.len().div_ceil(2));
    let mut rest = digits;
    if digits.len() % 2 == 1 {
        out.push(val(digits[0]));
        rest = &digits[1..];
    }
    for pair in rest.chunks(2) {
        out.push((val(pair[0]) << 4) | val(pair[1]));
    }
    out
}

/// A `'…'` or `"…"` literal starting at `text[start]`: MySQL's escapes (`\0 \' \" \b \n \r \t \Z
/// \\`, `\%` and `\_` kept with their backslash, any other `\X` is `X`), and a doubled quote is one
/// quote. Returns the bytes, the offset past the literal and the newlines it spans.
fn string(text: &[u8], start: usize) -> Result<(Vec<u8>, usize, u32), String> {
    let q = text[start];
    let mut out = Vec::new();
    let mut i = start + 1;
    let mut lines = 0;
    while i < text.len() {
        let c = text[i];
        if c == b'\\' && i + 1 < text.len() {
            let e = text[i + 1];
            match e {
                b'0' => out.push(0),
                b'b' => out.push(0x08),
                b'n' => out.push(b'\n'),
                b'r' => out.push(b'\r'),
                b't' => out.push(b'\t'),
                b'Z' => out.push(0x1A),
                b'%' | b'_' => {
                    out.push(b'\\');
                    out.push(e);
                }
                b'\n' => {
                    lines += 1;
                    out.push(b'\n');
                }
                other => out.push(other),
            }
            i += 2;
            continue;
        }
        if c == q {
            if text.get(i + 1) == Some(&q) {
                out.push(q);
                i += 2;
                continue;
            }
            return Ok((out, i + 1, lines));
        }
        if c == b'\n' {
            lines += 1;
        }
        out.push(c);
        i += 1;
    }
    Err("unterminated string literal".into())
}
