// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Common/Extensions/DoubleExtensions.cs
//! `DoubleExtensions`.

use crate::dotnet::format;

// ACE: DoubleExtensions.FormatChance
/// A chance in `[0, 1]` as a short percentage: `"100%"`, `"12.3%"`, `"0.0001%"`.
///
/// Works on the exact decimal expansion of `chance * 100` (`ToString("F99")`), keeps one decimal
/// for values of at least 1%, and cuts after the first significant digit below 1%.
#[must_use]
pub fn format_chance(chance: f64) -> String {
    if chance == 1.0 {
        return "100%".to_owned();
    }
    if chance == 0.0 {
        return "0%".to_owned();
    }
    let r = chance * 100.0;
    let p = format(r, "F99");
    let p = p.trim_end_matches('0');
    if !p.starts_with("0.") {
        let mut extra = 2;
        if p.contains(".0") || p.ends_with('.') {
            extra = 0;
        }
        let dot = p.find('.').expect("F99 always has a decimal point");
        return p[..dot + extra].to_owned() + "%";
    }
    let Some(i) = p.find(['1', '2', '3', '4', '5', '6', '7', '8', '9']) else {
        return "0%".to_owned();
    };
    p[..=i].to_owned() + "%"
}
