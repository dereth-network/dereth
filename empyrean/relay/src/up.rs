//! What Dereth sends, as what the ac1999 server reads: exactly the 1999 client's own outbound set.
//!
//! The server still reads most later actions (trade, housing, salvage), because it is GDLE underneath.
//! So the relay forwards an allowlist, and everything else is answered with a line in Dereth's chat
//! or dropped. Without this a Dereth player could do things no 1999 player can.

use dereth_protocol::{
    login::{
        CharGenResult, CharacterDeleteRequest, CharacterSendCharGenResult, LoginSendEnterWorld,
    },
    read_body,
    social::SocialSendFriendsCommand,
    write_blob, MessageError, Reader, Writer,
};

use crate::codec::{self, CharacterGen, CreateCharacter, DeleteCharacter, SelectCharacter};

/// The friends command's numbers, as the ac1999 server reads them.
const FRIENDS_ADD: u32 = 1;
const FRIENDS_REMOVE_ALL: u32 = 3;

/// The ordered game actions the November 1999 client writes, as ranges, ascending.
pub const ORDERED_ACTIONS: &[(u32, u32)] = &[
    (0x0008, 0x0008),
    (0x000A, 0x000A),
    (0x0015, 0x0015),
    (0x0019, 0x001B),
    (0x001D, 0x001F),
    (0x0032, 0x0032),
    (0x0035, 0x0036),
    (0x0044, 0x0048),
    (0x004A, 0x004B),
    (0x0053, 0x0056),
    (0x0058, 0x005D),
    (0x005F, 0x0060),
    (0x0063, 0x0063),
    (0x00A1, 0x00A6),
    (0x00AA, 0x00AE),
    (0x00BF, 0x00BF),
    (0x00C8, 0x00C8),
    (0x00CA, 0x00CA),
    (0x00CD, 0x00CD),
    (0x00D3, 0x00D3),
    (0x00D5, 0x00D6),
    (0x0140, 0x0140),
    (0x0145, 0x0149),
    (0x0195, 0x0195),
    (0x019B, 0x019D),
    (0x01A1, 0x01A2),
    (0x01A8, 0x01A8),
    (0x01B7, 0x01B7),
    (0x01BF, 0x01BF),
    (0x01C2, 0x01C2),
    (0x01C4, 0x01C4),
    (0x01DF, 0x01DF),
    (0x01E1, 0x01E1),
    (0x01E3, 0x01E4),
    (0x01E6, 0x01E6),
    (0x01E9, 0x01E9),
    (0x01EB, 0x01EB),
    (0xF61B, 0xF61C),
    (0xF61E, 0xF61E),
    (0xF649, 0xF649),
    (0xF661, 0xF661),
    (0xF752, 0xF753),
    (0xF7C9, 0xF7C9),
];

/// The messages it writes outside the ordered envelope.
pub const RAW_MESSAGES: [u32; 9] = [
    0xF653, 0xF655, 0xF656, 0xF657, 0xF6EA, 0xF7A9, 0xF7C8, 0xF7CC, 0xF7CD,
];

/// The 1999 client's outbound queues.
pub const QUEUE_CONTROL: u16 = 1;
pub const QUEUE_WEENIE: u16 = 5;
pub const QUEUE_LOGIN: u16 = 6;
pub const QUEUE_DATABASE: u16 = 7;

/// Whether the 1999 client writes this ordered action.
#[must_use]
pub fn is_ordered_action(action: u32) -> bool {
    ORDERED_ACTIONS
        .iter()
        .any(|(first, last)| (*first..=*last).contains(&action))
}

/// Actions whose bodies are the same on both wires: numbers only, no string in them. The later client
/// has no use for `0x005A`, `0x005C`, `0x00D3`, `0x00D5`, `0x01A2`, `0x01E6` and `0x01EB`, and the server
/// reads nothing of the last five; they are here so that nothing of the set is refused.
const SAME_BODY: &[u32] = &[
    0x0008, 0x000A, 0x0019, 0x001A, 0x001B, 0x001D, 0x001E, 0x001F, 0x0035, 0x0036, 0x0044, 0x0045,
    0x0046, 0x0047, 0x0048, 0x004A, 0x004B, 0x0054, 0x0055, 0x0056, 0x005A, 0x005C, 0x0063, 0x00A1,
    0x00A4, 0x00A5, 0x00A6, 0x00AA, 0x00AC, 0x00AD, 0x00AE, 0x00C8, 0x00CA, 0x00CD, 0x00D3, 0x00D5,
    0x0060, 0x0145, 0x0146, 0x0148, 0x0149, 0x0195, 0x019B, 0x019C, 0x019D, 0x01A2, 0x01A8, 0x01B7,
    0x01BF, 0x01C2, 0x01C4, 0x01E6, 0x01E9, 0x01EB, 0xF61B, 0xF61C, 0xF61E, 0xF649, 0xF661, 0xF752,
    0xF753, 0xF7C9,
];

/// Later actions whose numbers the 1999 client uses for something else: account and global squelch.
/// A 1999 squelch is by character, so these are refused rather than sent as the other thing.
const LATER_MEANING: &[u32] = &[0x0059, 0x005B];

/// The login action, which the relay follows with the squelch list.
pub const LOGIN_COMPLETE: u32 = 0x00A1;
/// The 1999 client's own command to tie to the life stone at hand: it never goes up as speech.
const SAVE_COMMAND: &str = "save";

/// What to do with one message from Dereth.
#[derive(Debug, Clone, PartialEq)]
pub enum Up {
    /// Send this to the server on this queue.
    Send { queue: u16, bytes: Vec<u8> },
    /// Say this in Dereth's chat instead; nothing goes to the server.
    Refuse(&'static str),
    /// Consumed or ignored without a word.
    Drop,
}

/// Later actions Dereth sends on its own, with nothing a player did to explain: dropped in silence.
/// AFK, a player option, the desired components, the spell filters, an item's mana, the house query
/// a login asks, contracts, the barber and plugin answers.
const DROPPED_SILENTLY: &[u32] = &[
    0x000F, 0x0010, 0x021E, 0x0224, 0x0263, 0x0286, 0x02AF, 0x02B2, 0x0311, 0x0316,
];

/// A later game action outside the 1999 client's set.
pub const NOT_IN_1999: &str = "That isn't possible in this world.";

fn later_string(r: &mut Reader<'_>) -> Result<String, MessageError> {
    r.pstring()
}

/// Whether a typed line is the 1999 client's `@save`, which it also takes as `/save`.
fn is_save_command(line: &str) -> bool {
    let line = line.trim();
    line.strip_prefix(['@', '/'])
        .is_some_and(|verb| verb.eq_ignore_ascii_case(SAVE_COMMAND))
}

/// An ordered action's opcode and body, re-encoded when it must be. The layouts are what the
/// ac1999 server reads.
fn action_body(action: u32, body: &[u8]) -> Result<(u32, Vec<u8>), MessageError> {
    if SAME_BODY.contains(&action) {
        return Ok((action, body.to_vec()));
    }
    let mut r = Reader::body(body);
    let mut w = Writer::new();
    match action {
        // The later squelch names who and whether; the 1999 client has four actions for that.
        0x0058 => {
            let add = r.i32()? != 0;
            let id = r.u32()?;
            let name = later_string(&mut r)?;
            if name.is_empty() {
                w.u32(id);
                return Ok((if add { 0x005A } else { 0x005B }, w.into_inner()));
            }
            codec::write_string(&mut w, &name, codec::MAX_NAME)?;
            return Ok((if add { 0x0058 } else { 0x0059 }, w.into_inner()));
        }
        // An advocate's teleport: a name, then a position in the same form. The server moves to the
        // next word after this name, an empty one too, as it doesn't after most strings.
        0x00D6 => {
            let name = later_string(&mut r)?;
            codec::write_string(&mut w, &name, codec::MAX_NAME)?;
            w.align4();
            w.bytes(r.rest());
        }
        // ac1999 reads nothing after the opcode; the later client says whether to disband.
        0x00A3 => {}
        0x00A2 => {
            let name = later_string(&mut r)?;
            codec::write_string(&mut w, &name, codec::MAX_NAME)?;
        }
        0x00AB => {
            w.u32(r.u32()?);
            w.u32(r.u32()?);
            let page = later_string(&mut r)?;
            codec::write_string(&mut w, &page, codec::MAX_NEWS)?;
        }
        0x00BF => {
            w.u32(r.u32()?);
            let inscription = later_string(&mut r)?;
            codec::write_string(&mut w, &inscription, codec::MAX_NEWS)?;
        }
        0x0147 => {
            w.u32(r.u32()?);
            let line = later_string(&mut r)?;
            codec::write_string(&mut w, &line, codec::MAX_NEWS)?;
        }
        0x0140 => {
            let target = later_string(&mut r)?;
            let status = r.u32()?;
            let complaint = later_string(&mut r)?;
            codec::write_string(&mut w, &target, codec::MAX_NAME)?;
            w.u32(status);
            codec::write_string(&mut w, &complaint, codec::MAX_NEWS)?;
        }
        // The later buy names an alternate currency after the list; the 1999 list ends the message.
        0x005F => {
            let list_end = body
                .len()
                .checked_sub(4)
                .ok_or_else(|| codec::invalid("a buy shorter than its currency word"))?;
            w.bytes(&body[..list_end]);
        }
        0x0015 | 0x01DF | 0x01E1 => {
            let text = later_string(&mut r)?;
            if action == 0x0015 && is_save_command(&text) {
                return Ok((0x01A2, Vec::new()));
            }
            codec::write_string(&mut w, &text, codec::MAX_NEWS)?;
        }
        0x0032 => {
            let text = later_string(&mut r)?;
            let target = r.u32()?;
            codec::write_string(&mut w, &text, codec::MAX_NEWS)?;
            w.u32(target);
        }
        0x005D => {
            let text = later_string(&mut r)?;
            let target = later_string(&mut r)?;
            codec::write_string(&mut w, &text, codec::MAX_NEWS)?;
            codec::write_string(&mut w, &target, codec::MAX_NAME)?;
        }
        0x0053 => w.u32(codec::player::combat_mode_from_later(r.u32()?)),
        // The later client also names which bar; the 1999 world has the one, and reads no such word.
        0x01E3 => {
            w.u32(r.u32()?);
            w.i32(r.i32()?);
        }
        0x01E4 => w.u32(r.u32()?),
        _ => return Err(codec::invalid("a 1999 action with no reader")),
    }
    Ok((action, w.into_inner()))
}

/// A later player option's bit in the 1999 option word, for the options a 1999 client has
/// (the ac1999 server's bits). The later client says one at a time; the 1999 client sends the word.
fn option_bit(option: u32) -> Option<u32> {
    Some(match option {
        0x0 => 0x2,
        0x1 => 0x4,
        0x2 => 0x8,
        0x6 => 0x40,
        0x7 => 0x80,
        0x8 => 0x100,
        0x9 => 0x200,
        0xA => 0x400,
        0xB => 0x800,
        0xC => 0x1000,
        0xD => 0x2000,
        0xE => 0x8000,
        _ => return None,
    })
}

/// The later character as the 1999 one.
///
/// The later wire numbers heritages from one (Aluvian 1) and sexes as Male 1, Female 2. The 1999 wire
/// numbers them by their place in the file's lists: heritages from zero, and the sexes, listed Female
/// then Male, as Female 0, Male 1. Its table has 35 skills.
#[must_use]
pub fn character_gen(c: &CharGenResult) -> CharacterGen {
    const SKILLS: usize = 35;
    const FEMALE: u32 = 2;
    CharacterGen {
        heritage: c.heritage_group.saturating_sub(1),
        gender: u32::from(c.gender != FEMALE),
        face: [
            c.eyes_strip,
            c.nose_strip,
            c.mouth_strip,
            c.hair_color,
            c.eye_color,
            c.hair_style,
            c.headgear_style,
        ],
        headgear_color: c.headgear_color,
        clothing: [
            (c.shirt_style, c.shirt_color),
            (c.trousers_style, c.trousers_color),
            (c.footwear_style, c.footwear_color),
        ],
        shades: [
            c.skin_shade,
            c.hair_shade,
            c.headgear_shade,
            c.shirt_shade,
            c.trousers_shade,
            c.footwear_shade,
        ],
        template: c.template_num,
        attributes: [
            c.strength,
            c.endurance,
            c.coordination,
            c.quickness,
            c.focus,
            c.self_,
        ],
        player: 0,
        access: 1,
        skills: (0..SKILLS)
            .map(|i| {
                c.skill_advancement_classes
                    .get(i)
                    .map_or(1, |v| v.cast_unsigned())
            })
            .collect(),
        name: c.name.clone(),
        heraldry: -1,
        heraldry_colors: 0,
        start_area: c.start_area,
    }
}

/// The session's translation state this way.
#[derive(Debug, Default)]
pub struct UpState {
    /// The last game action's order number. The server does not check it; it stays in sequence.
    order: u32,
    /// The account field the character list carried.
    pub account: String,
    /// The character's option word, from its description and each change since.
    pub options: u32,
    /// Questions the server has asked that have not been answered.
    pub questions: u32,
}

impl UpState {
    /// An ordered game action with the next order number.
    fn ordered(&mut self, action: u32, body: &[u8]) -> Up {
        self.order = self.order.wrapping_add(1);
        let mut out = 0xF7B1u32.to_le_bytes().to_vec();
        out.extend(self.order.to_le_bytes());
        out.extend(action.to_le_bytes());
        out.extend_from_slice(body);
        Up::Send {
            queue: QUEUE_WEENIE,
            bytes: out,
        }
    }

    /// `0x005C`: the whole squelch list, which a 1999 client shows the server at each login.
    pub fn squelch_upload(&mut self, ids: &[u32]) -> Up {
        let mut body = Vec::with_capacity(4 + ids.len() * 4);
        body.extend(u32::try_from(ids.len()).unwrap_or(u32::MAX).to_le_bytes());
        for id in ids {
            body.extend(id.to_le_bytes());
        }
        self.ordered(0x005C, &body)
    }

    /// One message from Dereth.
    ///
    /// # Errors
    /// A body that does not read.
    pub fn message(&mut self, bytes: &[u8]) -> Result<Up, MessageError> {
        let (opcode, body) = split(bytes)?;
        Ok(match opcode {
            0xF7B1 => {
                let (order_word, rest) = split(body)?;
                let _ = order_word;
                let (action, inner) = split(rest)?;
                if DROPPED_SILENTLY.contains(&action) {
                    return Ok(Up::Drop);
                }
                // A later option folds into the word the 1999 client sends.
                if action == 0x0005 {
                    let mut r = Reader::body(inner);
                    let (option, value) = (r.u32()?, r.u32()?);
                    let Some(bit) = option_bit(option) else {
                        return Ok(Up::Drop);
                    };
                    if value == 0 {
                        self.options &= !bit;
                    } else {
                        self.options |= bit;
                    }
                    return Ok(self.ordered(0x01A1, &self.options.to_le_bytes()));
                }
                // The later whole-module form: its first two words are the flags and the options.
                if action == 0x01A1 {
                    let mut r = Reader::body(inner);
                    let _flags = r.u32()?;
                    self.options = r.u32()?;
                    return Ok(self.ordered(0x01A1, &self.options.to_le_bytes()));
                }
                // The server asked a question of its own, in the later form it still writes.
                if action == 0x0275 && self.questions > 0 {
                    self.questions -= 1;
                    return Ok(self.ordered(0x0275, inner));
                }
                // A later friends change, as the 1999 client's friends command.
                if action == 0x0018 || action == 0x0025 {
                    let mut r = Reader::body(inner);
                    let (command, name) = if action == 0x0018 {
                        (FRIENDS_ADD, later_string(&mut r)?)
                    } else {
                        (FRIENDS_REMOVE_ALL, String::new())
                    };
                    let mut w = Writer::new();
                    w.u32(0xF7CD);
                    w.u32(command);
                    codec::write_string(&mut w, &name, codec::MAX_NAME)?;
                    return Ok(Up::Send {
                        queue: QUEUE_CONTROL,
                        bytes: w.into_inner(),
                    });
                }
                if !is_ordered_action(action) || LATER_MEANING.contains(&action) {
                    log::info!("refused game action {action:#06X}: not in the 1999 client's set");
                    return Ok(Up::Refuse(NOT_IN_1999));
                }
                let (sent_as, translated) = action_body(action, inner)?;
                self.ordered(sent_as, &translated)
            }
            0xF653 | 0xF7C8 => Up::Send {
                queue: QUEUE_LOGIN,
                bytes: bytes.to_vec(),
            },
            0xF656 => {
                let m = read_body::<CharacterSendCharGenResult>(body)?;
                Up::Send {
                    queue: QUEUE_LOGIN,
                    bytes: write_blob(&CreateCharacter {
                        account: self.account.clone(),
                        character: character_gen(&m.result),
                    })?,
                }
            }
            0xF657 => {
                let m = read_body::<LoginSendEnterWorld>(body)?;
                Up::Send {
                    queue: QUEUE_LOGIN,
                    bytes: write_blob(&SelectCharacter {
                        character: m.character,
                        account: self.account.clone(),
                    })?,
                }
            }
            0xF6EA | 0xF7CC => Up::Send {
                queue: QUEUE_CONTROL,
                bytes: bytes.to_vec(),
            },
            // The ac1999 server reads the account field, then the slot.
            0xF655 => {
                let m = read_body::<CharacterDeleteRequest>(body)?;
                Up::Send {
                    queue: QUEUE_LOGIN,
                    bytes: write_blob(&DeleteCharacter {
                        account: self.account.clone(),
                        slot: m.slot_index,
                    })?,
                }
            }
            // To the ac1999 server the friends command is a message of its own.
            0xF7CD => {
                let m = read_body::<SocialSendFriendsCommand>(body)?;
                let mut w = Writer::new();
                w.u32(0xF7CD);
                w.u32(m.cmd);
                codec::write_string(&mut w, &m.player, codec::MAX_NAME)?;
                Up::Send {
                    queue: QUEUE_CONTROL,
                    bytes: w.into_inner(),
                }
            }
            other => {
                log::info!("dropped later message {other:#06X}");
                Up::Drop
            }
        })
    }
}

fn split(bytes: &[u8]) -> Result<(u32, &[u8]), MessageError> {
    let (head, rest) = bytes
        .split_at_checked(4)
        .ok_or_else(|| codec::invalid("a message shorter than its opcode"))?;
    Ok((
        u32::from_le_bytes(head.try_into().expect("four bytes")),
        rest,
    ))
}

#[cfg(test)]
mod tests;
