//! The login connection's messages: the character list, creating, deleting, entering and
//! leaving, the world's name and notices; and the data download (queue 5).

use alloc::vec::Vec;

use crate::table::Table;
use crate::text::{NarrowString, StringInfo, WideString};
use crate::types::{CharacterIdentity, IterationSet, Qualities};
use crate::wire::{read_list, write_list, Field, Reader, Result, Writer};

/// A message with no fields.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Empty;

impl Field for Empty {
    // D2-03982, D2-03986
    fn read(_: &mut Reader<'_>) -> Result<Self> {
        Ok(Self)
    }
    fn write(&self, _: &mut Writer) {}
}

/// A character id (client: leave the world with it).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CharacterRef {
    pub character: u64,
}

impl Field for CharacterRef {
    // D2-05339
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            character: r.u64()?,
        })
    }
    fn write(&self, w: &mut Writer) {
        w.u64(self.character);
    }
}

/// The server: this character is deleted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CharacterDeleted {
    pub character: u64,
}

impl Field for CharacterDeleted {
    // D2-03983
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let character = r.u64()?;
        r.align()?;
        Ok(Self { character })
    }
    fn write(&self, w: &mut Writer) {
        w.u64(self.character);
        w.align();
    }
}

/// The client: delete this character of this account.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeleteCharacter {
    pub account: NarrowString,
    pub character: u64,
}

impl Field for DeleteCharacter {
    // D2-05341
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            account: NarrowString::read(r)?,
            character: r.u64()?,
        })
    }
    fn write(&self, w: &mut Writer) {
        self.account.write(w);
        w.u64(self.character);
    }
}

/// The client: bring back this deleted character of this account, while it is still greyed out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RestoreCharacter {
    pub character: u64,
    pub name: WideString,
    pub account: NarrowString,
}

impl Field for RestoreCharacter {
    // D2-05343
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            character: r.u64()?,
            name: WideString::read(r)?,
            account: NarrowString::read(r)?,
        })
    }
    fn write(&self, w: &mut Writer) {
        w.u64(self.character);
        self.name.write(w);
        self.account.write(w);
    }
}

/// The client: create a character with these choices. A default one is a fresh result: no name,
/// every number 0, no physique.
// D2-11931
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreateCharacter {
    pub account: NarrowString,
    pub name: WideString,
    /// The template the character is made from (the same for every new character).
    pub entity_did: u32,
    pub start_area: u32,
    /// A species bit: 1, 2, 4, 8 or 16.
    pub race: u32,
    /// `0x1000` or `0x2000`.
    pub sex: u32,
    pub class: u32,
    pub implement: u32,
    /// The physique sliders: per physique type, the value chosen.
    pub physique: Table<f32>,
}

impl Field for CreateCharacter {
    // D2-05340, D2-11920, D2-11923, D2-11921
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            account: NarrowString::read(r)?,
            name: WideString::read(r)?,
            entity_did: r.u32()?,
            start_area: r.u32()?,
            race: r.u32()?,
            sex: r.u32()?,
            class: r.u32()?,
            implement: r.u32()?,
            physique: Table::read(r)?,
        })
    }
    // D2-11919
    fn write(&self, w: &mut Writer) {
        self.account.write(w);
        self.name.write(w);
        for v in [
            self.entity_did,
            self.start_area,
            self.race,
            self.sex,
            self.class,
            self.implement,
        ] {
            w.u32(v);
        }
        self.physique.write(w);
    }
}

/// The client: enter the world as this character.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlayCharacter {
    pub character: u64,
    pub account: NarrowString,
}

impl Field for PlayCharacter {
    // D2-05342
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            character: r.u64()?,
            account: NarrowString::read(r)?,
        })
    }
    fn write(&self, w: &mut Writer) {
        w.u64(self.character);
        self.account.write(w);
    }
}

/// The server: the account's characters, with how each looks. A copy is a copy of both lists and
/// every word.
// D2-13109, D2-03988
#[derive(Clone, Debug, PartialEq)]
pub struct CharacterList {
    /// Every character, a deleted one (greyed out) included until it is gone.
    pub characters: Vec<CharacterIdentity>,
    /// Empty in every recorded list.
    pub deleted: Vec<CharacterIdentity>,
    pub status: u32,
    /// How many characters the account may have.
    pub slots: u32,
    pub account: NarrowString,
    /// Non-zero: chat channels go through the chat service.
    pub uses_chat_service: u32,
    /// Non-zero: the account has the expansion.
    pub has_expansion: u32,
    /// A third word, 1 or 0 in the recorded lists; its meaning is not known.
    pub unknown: u32,
}

/// A new list, before the server sends one: no characters, and room for five.
impl Default for CharacterList {
    // D2-13103
    fn default() -> Self {
        Self {
            characters: Vec::new(),
            deleted: Vec::new(),
            status: 0,
            slots: 5,
            account: NarrowString::default(),
            uses_chat_service: 0,
            has_expansion: 0,
            unknown: 0,
        }
    }
}

impl Field for CharacterList {
    // D2-03997, D2-13111, D2-13105
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            characters: read_list(r)?,
            deleted: read_list(r)?,
            status: r.u32()?,
            slots: r.u32()?,
            account: NarrowString::read(r)?,
            uses_chat_service: r.u32()?,
            has_expansion: r.u32()?,
            unknown: r.u32()?,
        })
    }
    fn write(&self, w: &mut Writer) {
        write_list(w, &self.characters);
        write_list(w, &self.deleted);
        w.u32(self.status);
        w.u32(self.slots);
        self.account.write(w);
        w.u32(self.uses_chat_service);
        w.u32(self.has_expansion);
        w.u32(self.unknown);
    }
}

/// The server: the account's characters in play, by name and id only. Dropping it lets go of the
/// names, the ids and the account.
// D2-03999
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CharacterNames {
    /// 0 in nearly every recorded list; 1 or 2 in a few. Its meaning is not known.
    pub number: u32,
    pub account: NarrowString,
    pub names: Vec<WideString>,
    pub ids: Vec<u64>,
}

impl Field for CharacterNames {
    // D2-04003, D2-04001
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            number: r.u32()?,
            account: NarrowString::read(r)?,
            names: read_list(r)?,
            ids: read_list(r)?,
        })
    }
    fn write(&self, w: &mut Writer) {
        w.u32(self.number);
        self.account.write(w);
        write_list(w, &self.names);
        write_list(w, &self.ids);
    }
}

/// The server: a character could not enter the world, or the login was refused.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CharacterRefused {
    pub reason: u32,
}

impl CharacterRefused {
    /// "The server has crashed."
    pub const SERVER_CRASH: u32 = 3;
    /// "You have been logged off."
    pub const LOGGED_OFF: u32 = 4;
    /// "There has been an error."
    pub const ERROR: u32 = 9;
    /// The character is still in the world.
    pub const IN_WORLD: u32 = 11;
    /// The character is not on this account.
    pub const NOT_OWNED: u32 = 13;
    /// "You are trying to enter with an old character."
    pub const OLD_CHARACTER: u32 = 15;
    /// "This character is corrupt. Contact tech support for help."
    pub const CORRUPT_CHARACTER: u32 = 16;
    /// The server found no place for the character.
    pub const COULD_NOT_PLACE: u32 = 18;
    /// The server has no room.
    pub const SERVER_FULL: u32 = 19;
    /// The character was booted from the world (the reason a boot ends a login with).
    pub const BOOTED: u32 = 20;
}

impl Field for CharacterRefused {
    // D2-03984
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let reason = r.u32()?;
        r.align()?;
        Ok(Self { reason })
    }
    fn write(&self, w: &mut Writer) {
        w.u32(self.reason);
        w.align();
    }
}

/// The server's answer to a character creation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CharacterCreated {
    /// 1 made, 3 name in use, 4 name refused, 8 no free slot.
    pub response: u32,
    /// The new character (empty when it was not made).
    pub identity: CharacterIdentity,
    pub result: u32,
}

impl CharacterCreated {
    pub const OK: u32 = 1;
    pub const NAME_IN_USE: u32 = 3;
    pub const NAME_REFUSED: u32 = 4;
    /// The account may not make an admin character.
    pub const ADMIN_REFUSED: u32 = 7;
    pub const NO_FREE_SLOT: u32 = 8;
}

impl Field for CharacterCreated {
    // D2-03987
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let response = r.u32()?;
        r.align()?;
        let identity = CharacterIdentity::read(r)?;
        let result = r.u32()?;
        r.align()?;
        Ok(Self {
            response,
            identity,
            result,
        })
    }
    fn write(&self, w: &mut Writer) {
        w.u32(self.response);
        w.align();
        self.identity.write(w);
        w.u32(self.result);
        w.align();
    }
}

/// The server: this object is the player (opens the world connection's traffic).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerObject {
    pub id: u64,
    pub region: u32,
}

impl Field for PlayerObject {
    // D2-04013
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let id = r.u64()?;
        r.align()?;
        let region = r.u32()?;
        r.align()?;
        Ok(Self { id, region })
    }
    fn write(&self, w: &mut Writer) {
        w.u64(self.id);
        w.align();
        w.u32(self.region);
        w.align();
    }
}

/// The server: the player's qualities.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlayerQualities(pub Qualities);

impl Field for PlayerQualities {
    // D2-03985
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self(Qualities::read(r)?))
    }
    fn write(&self, w: &mut Writer) {
        self.0.write(w);
    }
}

/// A text the server shows: the world's name, an account notice.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Text(pub StringInfo);

impl Field for Text {
    // D2-03976, D2-03974
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self(StringInfo::read(r)?))
    }
    fn write(&self, w: &mut Writer) {
        self.0.write(w);
    }
}

/// A text the server shows in a given place.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Notice {
    pub kind: u32,
    pub text: StringInfo,
}

impl Field for Notice {
    // D2-03975
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let kind = r.u32()?;
        r.align()?;
        Ok(Self {
            kind,
            text: StringInfo::read(r)?,
        })
    }
    fn write(&self, w: &mut Writer) {
        w.u32(self.kind);
        w.align();
        self.text.write(w);
    }
}

/// A chat-service packet, carried whole.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChatPacket(pub Vec<u8>);

impl Field for ChatPacket {
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let n = r.count()?;
        Ok(Self(r.bytes(n)?.to_vec()))
    }
    fn write(&self, w: &mut Writer) {
        w.count(self.0.len());
        w.bytes(&self.0);
    }
}

// ---- the data download ----------------------------------------------------------------------

/// The client asks for a data record: its type and id.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct DataRequest {
    pub kind: u32,
    pub id: u32,
}

impl Field for DataRequest {
    // D2-12844
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            kind: r.u32()?,
            id: r.u32()?,
        })
    }
    fn write(&self, w: &mut Writer) {
        w.u32(self.kind);
        w.u32(self.id);
    }
}

/// The server cannot send a requested record.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DataRefused {
    pub kind: u32,
    pub id: u32,
    /// 1 in every recorded refusal.
    pub error: u32,
}

impl Field for DataRefused {
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            kind: r.u32()?,
            id: r.u32()?,
            error: r.u32()?,
        })
    }
    fn write(&self, w: &mut Writer) {
        w.u32(self.kind);
        w.u32(self.id);
        w.u32(self.error);
    }
}

/// A data record the server sends: the data file it belongs to, its kind and its id, and the
/// record itself, zlib-compressed when `size` is not 0.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DataRecord {
    /// The data file, as the client names its files when it tells which data it has.
    pub dat_file: u64,
    pub kind: u32,
    pub id: u32,
    /// The record's length once inflated, or 0 when `bytes` is the record as it is.
    pub size: u32,
    pub bytes: Vec<u8>,
}

impl Field for DataRecord {
    // D2-12841
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let dat_file = r.u64()?;
        let kind = r.u32()?;
        let id = r.u32()?;
        // The length of what follows: the size word and the bytes.
        let n = r.u32()? as usize;
        let Some(len) = n.checked_sub(4) else {
            return Err(r.invalid("record length"));
        };
        let size = r.u32()?;
        Ok(Self {
            dat_file,
            kind,
            id,
            size,
            bytes: r.bytes(len)?.to_vec(),
        })
    }
    fn write(&self, w: &mut Writer) {
        w.u64(self.dat_file);
        w.u32(self.kind);
        w.u32(self.id);
        w.u32(u32::try_from(self.bytes.len() + 4).unwrap_or(u32::MAX));
        w.u32(self.size);
        w.bytes(&self.bytes);
    }
}

/// The server asks which data the client has: its region and languages.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DataQuery {
    pub region: u32,
    pub name_language: u32,
    pub languages: Vec<u32>,
}

impl Field for DataQuery {
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            region: r.u32()?,
            name_language: r.u32()?,
            languages: read_list(r)?,
        })
    }
    fn write(&self, w: &mut Writer) {
        w.u32(self.region);
        w.u32(self.name_language);
        write_list(w, &self.languages);
    }
}

/// The iterations a client has of one data file.
// D2-05165
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DataFileIterations {
    pub dat_file: u64,
    pub iterations: IterationSet,
}

impl Field for DataFileIterations {
    // D2-12845
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            dat_file: r.u64()?,
            iterations: IterationSet::read(r)?,
        })
    }
    fn write(&self, w: &mut Writer) {
        w.u64(self.dat_file);
        self.iterations.write(w);
    }
}

/// The client's answer: its language and, per data file, the iterations it has.
// D2-05129
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DataHeld {
    pub language: u32,
    pub files: Vec<DataFileIterations>,
}

impl Field for DataHeld {
    // D2-12846, D2-12842, D2-05163
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            language: r.u32()?,
            files: read_list(r)?,
        })
    }
    fn write(&self, w: &mut Writer) {
        w.u32(self.language);
        write_list(w, &self.files);
    }
}

/// What the client must fetch and drop of one data file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MissingIterations {
    pub dat_file: u64,
    pub iteration: u32,
    pub download: Vec<u32>,
    pub purge: Vec<u32>,
}

impl Field for MissingIterations {
    // D2-12849
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            dat_file: r.u64()?,
            iteration: r.u32()?,
            download: read_list(r)?,
            purge: read_list(r)?,
        })
    }
    fn write(&self, w: &mut Writer) {
        w.u64(self.dat_file);
        w.u32(self.iteration);
        write_list(w, &self.download);
        write_list(w, &self.purge);
    }
}

/// The server: a download begins.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DownloadBegins {
    pub bytes: u32,
    pub files: Vec<MissingIterations>,
}

impl Field for DownloadBegins {
    // D2-12843, D2-05167
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            bytes: r.u32()?,
            files: read_list(r)?,
        })
    }
    fn write(&self, w: &mut Writer) {
        w.u32(self.bytes);
        write_list(w, &self.files);
    }
}

/// One word, as the patch-time notice carries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Value(pub u32);

impl Field for Value {
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self(r.u32()?))
    }
    fn write(&self, w: &mut Writer) {
        w.u32(self.0);
    }
}
