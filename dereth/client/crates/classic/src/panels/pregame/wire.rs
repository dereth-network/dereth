//! The early 2005 body of `Character_SendCharGenResult` (`0xF656`), built from the classic
//! creation wizard's choices.
use crate::int::i32_from;
use crate::panels::LegacyCreation;

fn word(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn legacy_string(out: &mut Vec<u8>, value: &str) -> Result<(), String> {
    // Classic character names are ASCII once the wizard formats them. Account names use ANSI;
    // require ASCII here until the host supplies its explicit negotiated code page.
    if !value.is_ascii() || value.len() > u16::MAX as usize - 1 {
        return Err("Legacy account/name requires supported ANSI bytes".into());
    }
    out.extend_from_slice(&u16::try_from(value.len()).unwrap_or(u16::MAX).to_le_bytes());
    out.extend_from_slice(value.as_bytes());
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
    Ok(())
}

pub fn encode(account: &str, creation: &LegacyCreation) -> Result<Vec<u8>, String> {
    let r = &creation.result;
    let mut out = vec![];
    word(&mut out, 0xf656);
    legacy_string(&mut out, account)?;
    let checksum_values = [
        r.heritage_group,
        r.gender,
        r.eyes_strip as u32,
        r.nose_strip as u32,
        r.mouth_strip as u32,
        r.hair_color as u32,
        r.eye_color as u32,
        r.hair_style as u32,
        r.headgear_style as u32,
        r.shirt_style as u32,
        r.trousers_style as u32,
        r.footwear_style as u32,
        r.template_num as u32,
        r.strength as u32,
        r.endurance as u32,
        r.coordination as u32,
        r.quickness as u32,
        r.focus as u32,
        r.self_ as u32,
    ];
    for value in [
        1,
        r.heritage_group,
        r.gender,
        r.eyes_strip as u32,
        r.nose_strip as u32,
        r.mouth_strip as u32,
        r.hair_color as u32,
        r.eye_color as u32,
        r.hair_style as u32,
        r.headgear_style as u32,
        r.headgear_color,
        r.shirt_style as u32,
        r.shirt_color,
        r.trousers_style as u32,
        r.trousers_color,
        r.footwear_style as u32,
        r.footwear_color,
    ] {
        word(&mut out, value);
    }
    for value in [
        r.skin_shade,
        r.hair_shade,
        r.headgear_shade,
        r.shirt_shade,
        r.trousers_shade,
        r.footwear_shade,
    ] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    for value in [
        r.template_num,
        r.strength,
        r.endurance,
        r.coordination,
        r.quickness,
        r.focus,
        r.self_,
        r.slot,
        r.class_id as i32,
        i32_from(r.skill_advancement_classes.len()),
    ] {
        word(&mut out, value as u32);
    }
    for &value in &r.skill_advancement_classes {
        word(&mut out, value as u32);
    }
    legacy_string(&mut out, &r.name)?;
    for value in [
        creation.heraldry_symbol as u32,
        creation.heraldry_color,
        r.start_area,
        checksum_values.into_iter().fold(0u32, u32::wrapping_add),
    ] {
        word(&mut out, value);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;
    use crate::panels::pregame::{data::*, model::Creation};
    #[test]
    fn legacy_creation_keeps_zero_based_sex_and_checksum_exclusions() {
        let d = CreationData {
            heritages: vec![Heritage {
                sexes: vec![Sex {
                    name: "Female".into(),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut result = Creation::default().result(&d, 3);
        result.name = "Ab".into();
        result.heritage_group = 2;
        result.gender = 0;
        result.headgear_style = 0;
        result.template_num = 0;
        let mut value = LegacyCreation {
            result,
            heraldry_symbol: -1,
            heraldry_color: 7,
        };
        let a = encode("xy", &value).unwrap();
        assert_eq!(
            &a[..20],
            &[0x56, 0xf6, 0, 0, 2, 0, b'x', b'y', 1, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0]
        );
        let checksum = u32::from_le_bytes(a[a.len() - 4..].try_into().unwrap());
        assert_eq!(checksum, 302);
        value.result.headgear_color = 13;
        value.result.skin_shade = 0.9;
        value.result.slot = 4;
        value.heraldry_color = 9;
        let b = encode("xy", &value).unwrap();
        assert_ne!(a, b);
        assert_eq!(&a[a.len() - 4..], &b[b.len() - 4..]);
        value.result.strength += 1;
        let c = encode("xy", &value).unwrap();
        assert_eq!(
            u32::from_le_bytes(c[c.len() - 4..].try_into().unwrap()),
            303
        );
        assert_eq!(a.len(), 8 + 172 + 4 + 4); // account prefix, fixed body, one skill, padded name.
    }
}
