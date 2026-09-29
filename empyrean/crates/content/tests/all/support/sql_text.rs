//! SQL text normalization for comparisons with ACE output under V364.

pub(crate) fn as_ace_wrote(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("X'") {
        let hex = rest[at + 2..].find('\'').map(|e| &rest[at + 2..at + 2 + e]);
        match hex.filter(|h| h.bytes().all(|b| b.is_ascii_hexdigit())) {
            Some(h) if at == 0 || !rest.as_bytes()[at - 1].is_ascii_alphanumeric() => {
                out.push_str(&rest[..at]);
                out.push_str("System.Byte[]");
                rest = &rest[at + 3 + h.len()..];
            }
            _ => {
                out.push_str(&rest[..at + 2]);
                rest = &rest[at + 2..];
            }
        }
    }
    out.push_str(rest);
    let out = out.replace(", ,", ", NULL,");
    let out = out.replace(", ,", ", NULL,");
    let out = out.replace(", )", ", NULL)");
    out.replace(" /*  */", "")
}
