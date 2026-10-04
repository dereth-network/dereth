//! The known scenario declaration grammar; this is not general macro expansion.

use super::{attr_path, close_of, is_i, is_p, strip_quotes, Ignore, Tok, TokKind};

#[derive(Debug)]
pub(super) struct Entry {
    pub test: String,
    pub body: String,
    pub claims: Vec<String>,
    pub docs: Vec<String>,
    pub ignore: Ignore,
    pub line: usize,
}

/// Read complete `test => body ["claim", ...],` entries with outer attributes.
pub(super) fn parse(toks: &[Tok], open: usize, close: usize) -> Result<Vec<Entry>, String> {
    let mut entries = Vec::new();
    let mut k = open + 1;
    while k < close {
        let mut docs = Vec::new();
        let mut ignore = Ignore::No;
        while k < close {
            if toks[k].kind == TokKind::DocOuter {
                docs.push(toks[k].text.trim().to_owned());
                k += 1;
            } else if is_p(toks.get(k), "#") && is_p(toks.get(k + 1), "[") {
                let end = close_of(toks, k + 1);
                if end >= close {
                    return Err("unterminated scenario attribute".into());
                }
                let attr = attr_path(toks, k + 1, end);
                let ignore_at = if attr == "cfg_attr" {
                    // Only conditional ignore is part of the declaration grammar.
                    let mut at = k + 4;
                    while at < end && !is_p(toks.get(at), ",") {
                        if matches!(toks[at].text.as_str(), "(" | "[") {
                            at = close_of(toks, at) + 1;
                        } else {
                            at += 1;
                        }
                    }
                    let suffix = toks.get(at + 1..end).unwrap_or_default();
                    let valid = is_i(suffix.first(), "ignore")
                        && (suffix.len() == 2 && is_p(suffix.get(1), ")")
                            || suffix.len() == 4
                                && is_p(suffix.get(1), "=")
                                && suffix[2].kind == TokKind::Str
                                && is_p(suffix.get(3), ")"));
                    if !valid {
                        return Err("scenarios! supports cfg_attr only for ignore; use cfg to gate an entry".into());
                    }
                    at + 1
                } else {
                    k + 2
                };
                let reason = if is_p(toks.get(ignore_at + 1), "=") {
                    toks.get(ignore_at + 2)
                        .filter(|t| t.kind == TokKind::Str)
                        .map(|t| strip_quotes(&t.text))
                } else {
                    None
                };
                if attr == "ignore" {
                    ignore = Ignore::Always(reason);
                } else if attr == "cfg_attr" && toks[k..end].iter().any(|t| is_i(Some(t), "ignore"))
                {
                    ignore = Ignore::Conditional(reason);
                }
                k = end + 1;
            } else {
                break;
            }
        }
        let test = toks
            .get(k)
            .filter(|t| t.kind == TokKind::Ident)
            .ok_or("scenario entry needs a test identifier")?;
        if !is_p(toks.get(k + 1), "=") || !is_p(toks.get(k + 2), ">") {
            return Err(format!("{}: expected =>", test.text));
        }
        let body = toks
            .get(k + 3)
            .filter(|t| t.kind == TokKind::Ident)
            .ok_or("scenario entry needs a body identifier")?;
        if !is_p(toks.get(k + 4), "[") {
            return Err(format!("{}: expected claim list", test.text));
        }
        let end = close_of(toks, k + 4);
        if end >= close {
            return Err("unterminated scenario claim list".into());
        }
        let mut claims = Vec::new();
        let mut at = k + 5;
        while at < end {
            let claim = toks
                .get(at)
                .filter(|t| t.kind == TokKind::Str)
                .ok_or("scenario claims must be string literals")?;
            claims.push(strip_quotes(&claim.text));
            at += 1;
            if at < end {
                if !is_p(toks.get(at), ",") {
                    return Err("expected comma between claims".into());
                }
                at += 1;
            }
        }
        if !is_p(toks.get(end + 1), ",") {
            return Err("scenario entries require a trailing comma".into());
        }
        entries.push(Entry {
            test: test.text.clone(),
            body: body.text.clone(),
            claims,
            docs,
            ignore,
            line: test.line,
        });
        k = end + 2;
    }
    Ok(entries)
}
