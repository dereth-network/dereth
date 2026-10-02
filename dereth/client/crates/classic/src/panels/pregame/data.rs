//! The creation tables, as the classic portal holds them (read by [`crate::art`]).
pub use dereth_classic_dat::creation::{
    AppearanceData, Area, ClothingColor, CreationData, Discount, Heritage, Named, Profile, Sex,
    Skill, Strip, Template,
};

/// What the creation screens ask of a heritage's sex beyond its fields.
pub trait SexExt {
    /// The heraldry a symbol and colour choice gives, if this sex offers heraldry at all.
    fn heraldry(&self, symbol: Option<usize>, color: usize) -> Option<super::super::Heraldry>;
    /// The clothing choices of one slot: headgear, shirts, trousers, then footwear.
    fn clothes(&self, i: usize) -> &[Named];
}

impl SexExt for Sex {
    fn heraldry(&self, symbol: Option<usize>, color: usize) -> Option<super::super::Heraldry> {
        let row = self.legacy_80.get(symbol?)?;
        // Each symbol row starts with one header word; its sixteen colour textures follow.
        let texture = *row.get(1 + color.min(15))?;
        (self.legacy_18 != 0 && texture != 0).then_some(super::super::Heraldry {
            setup_id: self.legacy_18,
            old_texture: self.legacy_1c,
            texture,
            holding_location: 6,
        })
    }
    fn clothes(&self, i: usize) -> &[Named] {
        match i {
            0 => &self.headgear,
            1 => &self.shirts,
            2 => &self.trousers,
            _ => &self.footwear,
        }
    }
}

#[cfg(test)]
mod heraldry_tests {
    //! Behaviour: none (reading the creation table's heraldry rows).
    use super::*;
    use crate::int::u32_from;
    #[test]
    fn heraldry_uses_the_selected_colour_after_the_record_header() {
        let mut s = Sex {
            legacy_18: 0x02000001,
            legacy_1c: 0x05000001,
            ..Sex::default()
        };
        let mut row = vec![0xfeed; 17];
        for (i, v) in row.iter_mut().skip(1).enumerate() {
            *v = 0x05000100 + u32_from(i);
        }
        s.legacy_80.push(row);
        let h = s.heraldry(Some(0), 15).unwrap();
        assert_eq!(h.texture, 0x0500010f);
        assert_eq!(h.old_texture, 0x05000001);
        assert_eq!(h.holding_location, 6);
        assert!(s.heraldry(None, 0).is_none());
        assert!(s.heraldry(Some(1), 0).is_none());
        s.legacy_80[0][1] = 0;
        assert!(s.heraldry(Some(0), 0).is_none());
    }
}
