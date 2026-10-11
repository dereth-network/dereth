//! Folding a world's display name to the letters a slug is made of.

use dereth_network_protocol::text::cp1252_fit;

/// `c` as plain ASCII letters where it has an obvious spelling (`É` → `E`, `Æ` → `AE`, `ß` → `ss`,
/// `ā` → `a`); anything else is returned unchanged, for the caller to treat as a separator.
#[must_use]
pub fn fold_char(c: char) -> String {
    let s = match c {
        'À'..='Å' => "A",
        'à'..='å' => "a",
        'Æ' => "AE",
        'æ' => "ae",
        'Ç' => "C",
        'ç' => "c",
        'È'..='Ë' => "E",
        'è'..='ë' => "e",
        'Ì'..='Ï' => "I",
        'ì'..='ï' => "i",
        'Ð' => "D",
        'ð' => "d",
        'Ñ' => "N",
        'ñ' => "n",
        'Ò'..='Ö' | 'Ø' => "O",
        'ò'..='ö' | 'ø' => "o",
        'Ù'..='Ü' => "U",
        'ù'..='ü' => "u",
        'Ý' | 'Ÿ' => "Y",
        'ý' | 'ÿ' => "y",
        'Þ' => "TH",
        'þ' => "th",
        'ß' => "ss",
        'Œ' => "OE",
        'œ' => "oe",
        'Š' => "S",
        'š' => "s",
        'Ž' => "Z",
        'ž' => "z",
        _ => {
            return match cp1252_fit(c) {
                Some(f) if f.is_ascii() => f.to_string(),
                _ => c.to_string(),
            }
        }
    };
    s.to_owned()
}
