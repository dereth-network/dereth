//! Books as the ac1999 server sends them: every page's text inline, no version marker, normalized to
//! the later messages.

use dereth_primitives::ObjectId;
use dereth_protocol::{
    trade::{BookPageDataResponse, PageData, PageDataList, WritingBookOpen},
    Message, MessageError, Opcode, Reader, Writer,
};

use super::{invalid, read_string, MAX_NEWS};

fn read_page(r: &mut Reader<'_>) -> Result<PageData, MessageError> {
    let author_id = ObjectId(r.u32()?);
    let author_name = read_string(r, MAX_NEWS)?;
    let author_account = read_string(r, MAX_NEWS)?;
    let text_included = r.i32()?;
    let page_text = if text_included != 0 {
        Some(read_string(r, MAX_NEWS)?)
    } else {
        None
    };
    Ok(PageData {
        author_id,
        author_name,
        author_account,
        text_included,
        page_text,
        ..PageData::default()
    })
}

/// Most pages a book holds on the later reader's side: a bound on a count from the wire.
const MAX_PAGES: usize = 4096;

fn read_list(r: &mut Reader<'_>) -> Result<PageDataList, MessageError> {
    let max_num_pages = r.i32()?;
    let max_num_chars_per_page = r.i32()?;
    let count = usize::try_from(r.i32()?).map_err(|_| invalid("ac1999 page count"))?;
    if count > MAX_PAGES {
        return Err(invalid("ac1999 page count"));
    }
    let mut pages = Vec::with_capacity(count);
    for _ in 0..count {
        pages.push(read_page(r)?);
    }
    Ok(PageDataList {
        max_num_pages,
        max_num_chars_per_page,
        pages,
    })
}

/// `0x00B4`: a book opened.
#[derive(Debug, Clone, PartialEq)]
pub struct BookOpen(pub WritingBookOpen);

impl Message for BookOpen {
    const OPCODE: Opcode = Opcode(0x00B4);
    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self(WritingBookOpen {
            book_id: ObjectId(r.u32()?),
            max_num_pages: r.u32()?,
            pages: read_list(r)?,
            inscription: read_string(r, MAX_NEWS)?,
            scribe_id: ObjectId(r.u32()?),
            scribe_name: read_string(r, MAX_NEWS)?,
        }))
    }
    fn write(&self, _: &mut Writer) -> Result<(), MessageError> {
        Err(invalid("the relay reads this message and never writes it"))
    }
}

/// `0x00B8`: one page.
#[derive(Debug, Clone, PartialEq)]
pub struct PageResponse(pub BookPageDataResponse);

impl Message for PageResponse {
    const OPCODE: Opcode = Opcode(0x00B8);
    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self(BookPageDataResponse {
            object_id: ObjectId(r.u32()?),
            page: r.u32()?,
            data: read_page(r)?,
        }))
    }
    fn write(&self, _: &mut Writer) -> Result<(), MessageError> {
        Err(invalid("the relay reads this message and never writes it"))
    }
}
