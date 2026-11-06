//! ac1999 application codecs. Dispatch explicitly by dialect; these are not in the later registry.

use dereth_primitives::{text::cp1252, ObjectId};

use dereth_protocol::{login::CharacterIdentity, Message, MessageError, Opcode, Reader, Writer};

pub mod appraise;
pub mod book;
pub mod dat;
pub mod objects;
pub mod player;
pub mod social;

/// Decoder resource limits, not an advertised account capacity or name policy.
pub const MAX_CHARACTERS: usize = 64;
pub(crate) const MAX_NAME: usize = 255;
pub(crate) const MAX_NEWS: usize = 16 * 1024;
const ACCOUNT_MARKER: u32 = 0xBEEF_BEEF;

pub(crate) fn invalid(field: &'static str) -> MessageError {
    MessageError::InvalidValue { field, value: 0 }
}

pub(crate) fn read_string(r: &mut Reader<'_>, limit: usize) -> Result<String, MessageError> {
    let n = usize::from(r.u16()?);
    if n == 0 {
        return Ok(String::new());
    }
    if n > limit + 1 {
        return Err(invalid("ac1999 string length"));
    }
    let bytes = r.bytes(n)?;
    if bytes[n - 1] != 0 || bytes[..n - 1].contains(&0) {
        return Err(invalid("ac1999 string terminator"));
    }
    let text = cp1252::decode(&bytes[..n - 1]);
    let padding = r.blob_offset().wrapping_neg() & 3;
    if r.bytes(padding)?.iter().any(|&b| b != 0) {
        return Err(invalid("ac1999 string padding"));
    }
    Ok(text)
}

fn narrow(text: &str, limit: usize) -> Result<Vec<u8>, MessageError> {
    let raw = cp1252::encode(text).ok_or(invalid("ac1999 string encoding"))?;
    if raw.len() > limit || raw.contains(&0) {
        return Err(invalid("ac1999 string length or NUL"));
    }
    Ok(raw)
}

pub(crate) fn write_string(w: &mut Writer, text: &str, limit: usize) -> Result<(), MessageError> {
    let raw = narrow(text, limit)?;
    if raw.is_empty() {
        w.u16(0);
    } else {
        w.u16(u16::try_from(raw.len() + 1).map_err(|_| invalid("ac1999 string length"))?);
        w.bytes(&raw);
        w.u8(0);
        w.align4();
    }
    Ok(())
}

/// An account field: a marker word, twenty characters ending at the first NUL, and a reserved
/// word. The client reads the characters and skips the rest without checking them.
fn read_account(r: &mut Reader<'_>) -> Result<String, MessageError> {
    let _marker = r.u32()?;
    let bytes = r.bytes(20)?;
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let _reserved = r.u32()?;
    Ok(cp1252::decode(&bytes[..end]))
}

/// The client writes its own marker in what it sends; the server's roster has zero there. The
/// reserved word is written as zero.
fn write_account(w: &mut Writer, account: &str, marker: u32) -> Result<(), MessageError> {
    let raw = narrow(account, 20)?;
    w.u32(marker);
    let mut field = [0; 20];
    field[..raw.len()].copy_from_slice(&raw);
    w.bytes(&field);
    w.u32(0);
    Ok(())
}

/// Slots remain in wire order, including pending deletion. No capacity is advertised.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Roster {
    pub characters: Vec<CharacterIdentity>,
    pub account: String,
}

impl Message for Roster {
    const OPCODE: Opcode = Opcode(0xf658);

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let count = usize::try_from(r.u32()?).map_err(|_| invalid("ac1999 roster count"))?;
        if count > MAX_CHARACTERS || r.u32()? != 0 {
            return Err(invalid("ac1999 roster count or reserved field"));
        }
        let mut characters: Vec<CharacterIdentity> = Vec::with_capacity(count);
        for slot in 0..count {
            if r.u32()? != u32::try_from(slot).map_err(|_| invalid("ac1999 roster slot"))? {
                return Err(invalid("ac1999 roster slot"));
            }
            let gid = ObjectId(r.u32()?);
            if gid.0 == 0 || characters.iter().any(|c| c.gid == gid) {
                return Err(invalid("ac1999 roster identity"));
            }
            characters.push(CharacterIdentity {
                gid,
                name: read_string(r, MAX_NAME)?,
                seconds_greyed_out: r.u32()?,
            });
        }
        let account = read_account(r)?;
        r.expect_exhausted()?;
        Ok(Self {
            characters,
            account,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        if self.characters.len() > MAX_CHARACTERS {
            return Err(invalid("ac1999 roster count"));
        }
        w.u32(u32::try_from(self.characters.len()).map_err(|_| invalid("ac1999 roster count"))?);
        w.u32(0);
        for (slot, character) in self.characters.iter().enumerate() {
            if character.gid.0 == 0
                || self.characters[..slot]
                    .iter()
                    .any(|c| c.gid == character.gid)
            {
                return Err(invalid("ac1999 roster identity"));
            }
            w.u32(u32::try_from(slot).map_err(|_| invalid("ac1999 roster slot"))?);
            w.u32(character.gid.0);
            write_string(w, &character.name, MAX_NAME)?;
            w.u32(character.seconds_greyed_out);
        }
        write_account(w, &self.account, 0)
    }
}

/// The leading string is carried without interpreting it as later world metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct News {
    pub leading: String,
    pub text: String,
}

impl Message for News {
    const OPCODE: Opcode = Opcode(0xf65a);
    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let value = Self {
            leading: read_string(r, MAX_NAME)?,
            text: read_string(r, MAX_NEWS)?,
        };
        r.expect_exhausted()?;
        Ok(value)
    }
    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        write_string(w, &self.leading, MAX_NAME)?;
        write_string(w, &self.text, MAX_NEWS)
    }
}

/// The empty request to enter the world, which the client sends when its enter button is pressed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryRequest;

impl Message for EntryRequest {
    const OPCODE: Opcode = Opcode(0xf7c8);
    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        r.expect_exhausted()?;
        Ok(Self)
    }
    fn write(&self, _: &mut Writer) -> Result<(), MessageError> {
        Ok(())
    }
}

/// The server's empty answer: the client may now name its character.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryReady;

impl Message for EntryReady {
    const OPCODE: Opcode = Opcode(0xf7c7);
    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        r.expect_exhausted()?;
        Ok(Self)
    }
    fn write(&self, _: &mut Writer) -> Result<(), MessageError> {
        Ok(())
    }
}

/// The selected character and the account field the roster carried; the server checks ownership.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectCharacter {
    pub character: ObjectId,
    pub account: String,
}

impl Message for SelectCharacter {
    const OPCODE: Opcode = Opcode(0xf657);
    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let character = ObjectId(r.u32()?);
        let account = read_account(r)?;
        if character.0 == 0 {
            return Err(invalid("ac1999 selected identity"));
        }
        r.expect_exhausted()?;
        Ok(Self { character, account })
    }
    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        if self.character.0 == 0 {
            return Err(invalid("ac1999 selected identity"));
        }
        w.u32(self.character.0);
        write_account(w, &self.account, ACCOUNT_MARKER)
    }
}

/// The character the creation screen built, in the order the client packs it. Appearance values
/// are indices into the heritage's tables in the portal dat; `skills` holds one advancement class
/// per skill id (0 none, 1 untrained, 2 trained, 3 specialized).
#[derive(Debug, Clone, PartialEq)]
pub struct CharacterGen {
    pub heritage: u32,
    pub gender: u32,
    /// Eyes, nose, mouth, hair color, eye color, hair style, headgear style.
    pub face: [i32; 7],
    pub headgear_color: u32,
    /// Shirt, trousers and footwear: style, then palette.
    pub clothing: [(i32, u32); 3],
    /// Skin, hair, headgear, shirt, trousers and footwear shades.
    pub shades: [f64; 6],
    pub template: i32,
    /// Strength, endurance, coordination, quickness, focus, self.
    pub attributes: [i32; 6],
    pub player: u32,
    /// The access the screen asks for; 1 for a player. The server decides.
    pub access: u32,
    pub skills: Vec<u32>,
    pub name: String,
    pub heraldry: i32,
    pub heraldry_colors: u32,
    pub start_area: u32,
}

const MAX_SKILLS: usize = 64;

impl CharacterGen {
    /// The closing word the client adds: the sum of every integer choice before the shades and
    /// after them, through the attributes.
    fn sum(&self) -> u32 {
        let mut sum = self.heritage.wrapping_add(self.gender);
        let styles = self.clothing.iter().map(|c| c.0);
        for v in self
            .face
            .iter()
            .copied()
            .chain(styles)
            .chain([self.template])
        {
            sum = sum.wrapping_add(v as u32);
        }
        self.attributes
            .iter()
            .fold(sum, |s, &v| s.wrapping_add(v as u32))
    }

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        if r.u32()? != 1 {
            return Err(invalid("ac1999 creation version"));
        }
        let heritage = r.u32()?;
        let gender = r.u32()?;
        let mut face = [0; 7];
        for v in &mut face {
            *v = r.i32()?;
        }
        let headgear_color = r.u32()?;
        let mut clothing = [(0, 0); 3];
        for c in &mut clothing {
            *c = (r.i32()?, r.u32()?);
        }
        let mut shades = [0.0; 6];
        for v in &mut shades {
            *v = r.f64()?;
        }
        let template = r.i32()?;
        let mut attributes = [0; 6];
        for v in &mut attributes {
            *v = r.i32()?;
        }
        let player = r.u32()?;
        let access = r.u32()?;
        let count = usize::try_from(r.u32()?).map_err(|_| invalid("ac1999 skill count"))?;
        if count > MAX_SKILLS {
            return Err(invalid("ac1999 skill count"));
        }
        let skills = (0..count).map(|_| r.u32()).collect::<Result<_, _>>()?;
        let value = Self {
            heritage,
            gender,
            face,
            headgear_color,
            clothing,
            shades,
            template,
            attributes,
            player,
            access,
            skills,
            name: read_string(r, MAX_NAME)?,
            heraldry: r.i32()?,
            heraldry_colors: r.u32()?,
            start_area: r.u32()?,
        };
        if r.u32()? != value.sum() {
            return Err(invalid("ac1999 creation sum"));
        }
        Ok(value)
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        if self.skills.len() > MAX_SKILLS || self.name.is_empty() {
            return Err(invalid("ac1999 creation"));
        }
        w.u32(1);
        w.u32(self.heritage);
        w.u32(self.gender);
        for &v in &self.face {
            w.i32(v);
        }
        // Each article's style, then its color, headgear's color first.
        w.u32(self.headgear_color);
        for &(style, color) in &self.clothing {
            w.i32(style);
            w.u32(color);
        }
        for &v in &self.shades {
            w.f64(v);
        }
        w.i32(self.template);
        for &v in &self.attributes {
            w.i32(v);
        }
        w.u32(self.player);
        w.u32(self.access);
        w.u32(u32::try_from(self.skills.len()).map_err(|_| invalid("ac1999 skill count"))?);
        for &v in &self.skills {
            w.u32(v);
        }
        write_string(w, &self.name, MAX_NAME)?;
        w.i32(self.heraldry);
        w.u32(self.heraldry_colors);
        w.u32(self.start_area);
        w.u32(self.sum());
        Ok(())
    }
}

/// The request to create a character: the account field, then the created character.
#[derive(Debug, Clone, PartialEq)]
pub struct CreateCharacter {
    pub account: String,
    pub character: CharacterGen,
}

impl Message for CreateCharacter {
    const OPCODE: Opcode = Opcode(0xf656);
    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let account = read_account(r)?;
        let character = CharacterGen::read(r)?;
        r.expect_exhausted()?;
        Ok(Self { account, character })
    }
    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        write_account(w, &self.account, ACCOUNT_MARKER)?;
        self.character.write(w)
    }
}

/// `0xF655`: delete the character in a roster slot. The account field, then the slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteCharacter {
    pub account: String,
    pub slot: i32,
}

impl Message for DeleteCharacter {
    const OPCODE: Opcode = Opcode(0xf655);
    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let account = read_account(r)?;
        let slot = r.i32()?;
        r.expect_exhausted()?;
        Ok(Self { account, slot })
    }
    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        write_account(w, &self.account, ACCOUNT_MARKER)?;
        w.i32(self.slot);
        Ok(())
    }
}

/// The answer to a creation: 1 and the new character's roster entry, or a failure code alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateResponse {
    pub code: u32,
    pub character: Option<CharacterIdentity>,
}

impl Message for CreateResponse {
    const OPCODE: Opcode = Opcode(0xf643);
    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let code = r.u32()?;
        let character = if code == 1 {
            let gid = ObjectId(r.u32()?);
            if gid.0 == 0 {
                return Err(invalid("ac1999 created identity"));
            }
            Some(CharacterIdentity {
                gid,
                name: read_string(r, MAX_NAME)?,
                seconds_greyed_out: r.u32()?,
            })
        } else {
            None
        };
        r.expect_exhausted()?;
        Ok(Self { code, character })
    }
    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.code);
        if let Some(c) = &self.character {
            w.u32(c.gid.0);
            write_string(w, &c.name, MAX_NAME)?;
            w.u32(c.seconds_greyed_out);
        }
        Ok(())
    }
}

/// `0xF7B0`: a message ordered by an object's event stamp: the object, the stamp, then a whole
/// message of the character-list and game set, opcode first. The client holds or drops it by the
/// stamp before it reads the message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    pub object: ObjectId,
    pub stamp: u32,
    pub message: Vec<u8>,
}

impl Envelope {
    /// The wrapped message's opcode.
    #[must_use]
    pub fn opcode(&self) -> u32 {
        u32::from_le_bytes(self.message[..4].try_into().expect("at least an opcode"))
    }
}

impl Message for Envelope {
    const OPCODE: Opcode = Opcode(0xf7b0);
    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let object = ObjectId(r.u32()?);
        let stamp = r.u32()?;
        if r.remaining() < 4 {
            return Err(invalid("ac1999 wrapped message"));
        }
        Ok(Self {
            object,
            stamp,
            message: r.rest().to_vec(),
        })
    }
    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        if self.message.len() < 4 {
            return Err(invalid("ac1999 wrapped message"));
        }
        w.u32(self.object.0);
        w.u32(self.stamp);
        w.bytes(&self.message);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
