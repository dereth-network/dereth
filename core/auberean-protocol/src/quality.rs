//! Quality updates: one quality of the player (`private`), or of an object others see
//! (`visual`), changed to a value. Ten kinds of value: int, bool, float, string, data id,
//! instance id, position, timestamp, string reference, long int.

use crate::types::ObjectStamp;
use crate::wire::{Field, Reader, Result, Writer};

/// A quality of the player.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PrivateQuality<V> {
    pub quality: u32,
    pub value: V,
}

impl<V: Field> Field for PrivateQuality<V> {
    // D2-04082, D2-04074, D2-04078, D2-04092, D2-04076, D2-04080, D2-04086, D2-04090, D2-04088, D2-04084
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let quality = r.u32()?;
        r.align()?;
        let value = V::read(r)?;
        r.align()?;
        Ok(Self { quality, value })
    }
    fn write(&self, w: &mut Writer) {
        w.u32(self.quality);
        w.align();
        self.value.write(w);
        w.align();
    }
}

/// A quality of an object others see.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VisualQuality<V> {
    pub object: ObjectStamp,
    pub quality: u32,
    pub value: V,
}

impl<V: Field> Field for VisualQuality<V> {
    // D2-04083, D2-04075, D2-04079, D2-04093, D2-04077, D2-04081, D2-04087, D2-04091, D2-04089, D2-04085
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let object = ObjectStamp::read(r)?;
        let quality = r.u32()?;
        r.align()?;
        let value = V::read(r)?;
        r.align()?;
        Ok(Self {
            object,
            quality,
            value,
        })
    }
    fn write(&self, w: &mut Writer) {
        self.object.write(w);
        w.u32(self.quality);
        w.align();
        self.value.write(w);
        w.align();
    }
}
